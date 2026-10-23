//! SFTP：浏览远程文件、上传下载。
//!
//! 传输速度主要靠两点：单个文件读写时同时发出多个请求（流水线，russh-sftp 内部处理），
//! 传整个文件夹时几个小文件同时传，避免每个文件打开、关闭的往返时间叠加。

use crate::{Result, tr};
use crate::conn::Connection;
use russh_sftp::client::SftpSession;
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::protocol::{FileAttributes, OpenFlags, StatusCode};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::task::JoinSet;

/// 传文件夹时同时传几个文件
const PARALLEL_FILES: usize = 4;
/// 本地读写缓冲区
const CHUNK: usize = 1024 * 1024;

pub fn describe(e: SftpError) -> String {
    match e {
        SftpError::Status(s) => match s.status_code {
            StatusCode::NoSuchFile => tr!("文件或文件夹不存在", "No such file or folder"),
            StatusCode::PermissionDenied => tr!("没有权限", "Permission denied"),
            StatusCode::Failure if !s.error_message.is_empty() => s.error_message,
            c => format!("{c}: {}", s.error_message),
        },
        SftpError::Timeout => tr!("服务器响应超时", "The server timed out"),
        e => e.to_string(),
    }
}

pub async fn open(conn: &Connection) -> Result<Arc<SftpSession>> {
    let ch = conn.handle.channel_open_session().await.map_err(|e| tr!("打开 SFTP 失败：{e}", "Couldn't open SFTP: {e}"))?;
    ch.request_subsystem(true, "sftp").await.map_err(|e| tr!("服务器不支持 SFTP：{e}", "The server doesn't support SFTP: {e}"))?;
    let cfg = russh_sftp::client::Config {
        // 同时发出更多读写请求，高延迟的线路上也能跑满带宽
        max_concurrent_reads: 64,
        max_concurrent_writes: 64,
        request_timeout_secs: 30,
        ..Default::default()
    };
    let s = SftpSession::new_with_config(ch.into_stream(), cfg).await.map_err(describe)?;
    Ok(Arc::new(s))
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub is_link: bool,
    pub size: u64,
    pub mtime: u32,
    /// 权限位，例如 0o755
    pub perm: u32,
}

pub fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') { format!("{dir}{name}") } else { format!("{dir}/{name}") }
}

pub async fn home(s: &SftpSession) -> Result<String> {
    s.canonicalize(".").await.map_err(describe)
}

pub async fn list(s: &SftpSession, dir: &str) -> Result<Vec<Entry>> {
    let mut out = Vec::new();
    for e in s.read_dir(dir).await.map_err(describe)? {
        let name = e.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let m = e.metadata();
        let is_link = m.is_symlink();
        // 指向文件夹的链接也要能点进去
        let is_dir = if is_link {
            s.metadata(join(dir, &name)).await.map(|t| t.is_dir()).unwrap_or(false)
        } else {
            m.is_dir()
        };
        out.push(Entry {
            name,
            is_dir,
            is_link,
            size: m.size.unwrap_or(0),
            mtime: m.mtime.unwrap_or(0),
            perm: m.permissions.unwrap_or(0) & 0o7777,
        });
    }
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

pub async fn mkdir(s: &SftpSession, path: &str) -> Result<()> {
    s.create_dir(path).await.map_err(describe)
}

pub async fn rename(s: &SftpSession, from: &str, to: &str) -> Result<()> {
    s.rename(from, to).await.map_err(describe)
}

pub async fn chmod(s: &SftpSession, path: &str, perm: u32) -> Result<()> {
    let attrs = FileAttributes { permissions: Some(perm & 0o7777), ..Default::default() };
    s.set_metadata(path, attrs).await.map_err(describe)
}

/// 删除文件或整个文件夹
pub async fn remove(s: &SftpSession, path: &str, is_dir: bool) -> Result<()> {
    if !is_dir {
        return s.remove_file(path).await.map_err(describe);
    }
    // 先深度优先列出所有内容，再从最里层往外删
    let mut dirs = vec![path.to_string()];
    let mut i = 0;
    while i < dirs.len() {
        let dir = dirs[i].clone();
        for e in s.read_dir(&dir).await.map_err(describe)? {
            let name = e.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let p = join(&dir, &name);
            if e.metadata().is_dir() {
                dirs.push(p);
            } else {
                s.remove_file(&p).await.map_err(describe)?;
            }
        }
        i += 1;
    }
    for d in dirs.iter().rev() {
        s.remove_dir(d).await.map_err(describe)?;
    }
    Ok(())
}

/// 一个传输任务的进度，界面定时来取
#[derive(Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub files_done: AtomicU64,
    pub files_total: AtomicU64,
    pub cancelled: AtomicBool,
    /// 正在传的文件名
    pub current: Mutex<String>,
}

impl Progress {
    fn check(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Relaxed) { Err(CANCELLED.into()) } else { Ok(()) }
    }
    fn set_current(&self, name: &str) {
        if let Ok(mut c) = self.current.lock() {
            name.clone_into(&mut c);
        }
    }
}

/// 传输被取消（不是错误，界面不显示）
pub const CANCELLED: &str = "cancelled";

/// 要传的一个文件：本地路径、远程路径、大小
struct Job {
    local: PathBuf,
    remote: String,
    size: u64,
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// 把本地文件或文件夹上传到远程文件夹 `remote_dir` 里
pub async fn upload(s: Arc<SftpSession>, local: PathBuf, remote_dir: String, p: Arc<Progress>) -> Result<()> {
    let meta = tokio::fs::metadata(&local).await.map_err(|e| tr!("读取 {} 失败：{e}", "Couldn't read {}: {e}", local.display()))?;
    let target = join(&remote_dir, &file_name(&local));
    let mut jobs = Vec::new();
    if meta.is_dir() {
        // 列出本地文件夹，先建好远程目录结构
        let mut dirs = vec![(local.clone(), target.clone())];
        let mut i = 0;
        while i < dirs.len() {
            p.check()?;
            let (ldir, rdir) = dirs[i].clone();
            if let Err(e) = s.create_dir(&rdir).await {
                // 已经存在就继续往里面传
                if !s.metadata(&rdir).await.map(|m| m.is_dir()).unwrap_or(false) {
                    return Err(tr!("创建文件夹 {rdir} 失败：{}", "Couldn't create folder {rdir}: {}", describe(e)));
                }
            }
            let mut rd = tokio::fs::read_dir(&ldir).await.map_err(|e| tr!("读取 {} 失败：{e}", "Couldn't read {}: {e}", ldir.display()))?;
            while let Some(e) = rd.next_entry().await.map_err(|e| e.to_string())? {
                let name = e.file_name().to_string_lossy().into_owned();
                let ft = e.file_type().await.map_err(|e| e.to_string())?;
                if ft.is_dir() {
                    dirs.push((e.path(), join(&rdir, &name)));
                } else if ft.is_file() {
                    let size = e.metadata().await.map(|m| m.len()).unwrap_or(0);
                    jobs.push(Job { local: e.path(), remote: join(&rdir, &name), size });
                }
                // 符号链接等特殊文件跳过
            }
            i += 1;
        }
    } else {
        jobs.push(Job { local, remote: target, size: meta.len() });
    }
    run_jobs(jobs, p, move |job, p| {
        let s = s.clone();
        async move { upload_file(&s, &job, &p).await }
    })
    .await
}

async fn upload_file(s: &SftpSession, job: &Job, p: &Progress) -> Result<()> {
    p.set_current(&file_name(&job.local));
    let mut src = tokio::fs::File::open(&job.local).await.map_err(|e| tr!("打开 {} 失败：{e}", "Couldn't open {}: {e}", job.local.display()))?;
    let mut dst = s
        .open_with_flags(&job.remote, OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE)
        .await
        .map_err(|e| tr!("创建 {} 失败：{}", "Couldn't create {}: {}", job.remote, describe(e)))?;
    let mut buf = vec![0u8; CHUNK];
    let r: Result<()> = async {
        loop {
            p.check()?;
            let n = src.read(&mut buf).await.map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            // write_all 会拆成多个写请求同时发出，不用等每个都回复
            dst.write_all(&buf[..n]).await.map_err(|e| tr!("写入 {} 失败：{e}", "Couldn't write {}: {e}", job.remote))?;
            p.done.fetch_add(n as u64, Ordering::Relaxed);
        }
        dst.shutdown().await.map_err(|e| tr!("写入 {} 失败：{e}", "Couldn't write {}: {e}", job.remote))
    }
    .await;
    if r.is_err() {
        // 传了一半的文件删掉，避免留下不完整的文件
        drop(dst);
        let _ = s.remove_file(&job.remote).await;
        return r;
    }
    // 保留可执行权限，上传脚本后可以直接运行
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(m) = tokio::fs::metadata(&job.local).await {
            let mode = m.permissions().mode();
            if mode & 0o111 != 0 {
                let _ = chmod(s, &job.remote, mode & 0o777).await;
            }
        }
    }
    Ok(())
}

/// 把远程文件或文件夹下载到本地文件夹 `local_dir` 里
pub async fn download(
    s: Arc<SftpSession>,
    remote: String,
    is_dir: bool,
    local_dir: PathBuf,
    p: Arc<Progress>,
) -> Result<()> {
    let name = remote.trim_end_matches('/').rsplit('/').next().unwrap_or("download").to_string();
    let target = unique_path(&local_dir, &name);
    let mut jobs = Vec::new();
    if is_dir {
        let mut dirs = vec![(remote.clone(), target.clone())];
        let mut i = 0;
        while i < dirs.len() {
            p.check()?;
            let (rdir, ldir) = dirs[i].clone();
            tokio::fs::create_dir_all(&ldir).await.map_err(|e| tr!("创建 {} 失败：{e}", "Couldn't create {}: {e}", ldir.display()))?;
            for e in s.read_dir(&rdir).await.map_err(describe)? {
                let n = e.file_name();
                if n == "." || n == ".." {
                    continue;
                }
                let m = e.metadata();
                if m.is_dir() {
                    dirs.push((join(&rdir, &n), ldir.join(&n)));
                } else if m.is_regular() {
                    jobs.push(Job { local: ldir.join(&n), remote: join(&rdir, &n), size: m.size.unwrap_or(0) });
                }
            }
            i += 1;
        }
    } else {
        let size = s.metadata(&remote).await.map_err(describe)?.size.unwrap_or(0);
        jobs.push(Job { local: target, remote, size });
    }
    run_jobs(jobs, p, move |job, p| {
        let s = s.clone();
        async move { download_file(&s, &job, &p).await }
    })
    .await
}

async fn download_file(s: &SftpSession, job: &Job, p: &Progress) -> Result<()> {
    p.set_current(&file_name(&job.local));
    let mut src = s.open(&job.remote).await.map_err(|e| tr!("打开 {} 失败：{}", "Couldn't open {}: {}", job.remote, describe(e)))?;
    let mut dst = tokio::fs::File::create(&job.local).await.map_err(|e| tr!("创建 {} 失败：{e}", "Couldn't create {}: {e}", job.local.display()))?;
    let mut buf = vec![0u8; CHUNK];
    let r: Result<()> = async {
        loop {
            p.check()?;
            let n = src.read(&mut buf).await.map_err(|e| tr!("读取 {} 失败：{e}", "Couldn't read {}: {e}", job.remote))?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n]).await.map_err(|e| e.to_string())?;
            p.done.fetch_add(n as u64, Ordering::Relaxed);
        }
        dst.flush().await.map_err(|e| e.to_string())
    }
    .await;
    if r.is_err() {
        drop(dst);
        let _ = tokio::fs::remove_file(&job.local).await;
    }
    r
}

/// 本地已经有同名文件时改成“名字 (1).扩展名”，和浏览器下载一样，不覆盖已有文件
fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (1..)
        .map(|i| dir.join(format!("{stem} ({i}){ext}")))
        .find(|p| !p.exists())
        .unwrap()
}

/// 同时执行几个文件的传输；有一个失败就取消其余的
async fn run_jobs<F, Fut>(jobs: Vec<Job>, p: Arc<Progress>, run: F) -> Result<()>
where
    F: Fn(Job, Arc<Progress>) -> Fut,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    p.total.store(jobs.iter().map(|j| j.size).sum(), Ordering::Relaxed);
    p.files_total.store(jobs.len() as u64, Ordering::Relaxed);
    let mut set = JoinSet::new();
    let mut jobs = jobs.into_iter();
    let mut first_err = None;
    loop {
        while first_err.is_none() && set.len() < PARALLEL_FILES {
            let Some(job) = jobs.next() else { break };
            set.spawn(run(job, p.clone()));
        }
        let Some(r) = set.join_next().await else { break };
        match r.map_err(|e| e.to_string()).and_then(|r| r) {
            Ok(()) => {
                p.files_done.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(e);
                    p.cancelled.store(true, Ordering::Relaxed);
                }
            }
        }
    }
    match first_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
