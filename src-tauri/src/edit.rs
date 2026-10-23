//! 编辑远程文件：下载到本机临时文件夹，用本机的编辑器打开，保存后自动传回服务器。

use crate::{AppState, CmdResult};
use serde::Serialize;
use shelltree_core::{sftp, tr};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager, State};

/// 超过这个大小的文件不在这里编辑
const MAX_SIZE: u64 = 20 * 1024 * 1024;

pub struct EditFile {
    id: u64,
    pub session: String,
    remote: String,
    local: PathBuf,
    /// 上次看到的本机文件修改时间和大小，变了就说明编辑器保存过
    seen: (SystemTime, u64),
    /// 下载或上次上传后服务器上文件的修改时间，用来发现别人同时改了这个文件
    remote_mtime: u32,
}

#[derive(Serialize, Clone)]
struct EditEvent {
    id: u64,
    session: String,
    remote: String,
    /// saved：已传回服务器；failed：上传失败；conflict：服务器上的文件在这期间被改过，等用户决定是否覆盖
    state: &'static str,
    error: Option<String>,
}

fn local_stamp(p: &Path) -> Option<(SystemTime, u64)> {
    let m = std::fs::metadata(p).ok()?;
    Some((m.modified().ok()?, m.len()))
}

/// 打开远程文件。同一个文件已经在编辑时直接再打开一次编辑器
#[tauri::command]
pub async fn edit_open(app: AppHandle, state: State<'_, AppState>, session: String, path: String, editor: Option<String>) -> CmdResult<()> {
    let existing = state
        .edits
        .lock()
        .unwrap()
        .iter()
        .find(|e| e.session == session && e.remote == path)
        .map(|e| e.local.clone());
    if let Some(local) = existing {
        return launch(&local, editor.as_deref()).await;
    }
    let s = state.sftp(&session).await?;
    let (remote_mtime, size) = sftp::stat(&s, &path).await?;
    if size > MAX_SIZE {
        return Err(tr!("文件太大（超过 20 MB），请下载后再编辑", "The file is too large (over 20 MB). Download it to edit"));
    }
    let data = sftp::read_file(&s, &path).await?;
    if data.iter().take(8192).any(|&b| b == 0) {
        return Err(tr!("这看起来不是文本文件", "This doesn't look like a text file"));
    }
    let id = state.id();
    let name = path.rsplit('/').next().filter(|n| !n.is_empty()).unwrap_or("file");
    let dir = edit_root(&app)?.join(id.to_string());
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let local = dir.join(name);
    std::fs::write(&local, &data).map_err(|e| tr!("写入临时文件失败：{e}", "Couldn't write the temporary file: {e}"))?;
    let seen = local_stamp(&local).ok_or_else(|| tr!("写入临时文件失败", "Couldn't write the temporary file"))?;
    state.edits.lock().unwrap().push(EditFile { id, session, remote: path, local: local.clone(), seen, remote_mtime });
    launch(&local, editor.as_deref()).await
}

/// 不管服务器上的文件有没有被别人改过，直接用本机的版本覆盖（用户在冲突提示里选了覆盖）
#[tauri::command]
pub async fn edit_upload(app: AppHandle, id: u64) -> CmdResult<()> {
    upload(&app, id, true).await;
    Ok(())
}

/// 标签页关掉了，不再把这个会话里编辑的文件传回去（断线重连时不调用，重连后保存还能上传）
#[tauri::command]
pub fn edit_forget(state: State<'_, AppState>, session: String) {
    state.edits.lock().unwrap().retain(|e| e.session != session);
}

fn edit_root(app: &AppHandle) -> CmdResult<PathBuf> {
    Ok(app.path().app_cache_dir().map_err(|e| e.to_string())?.join("edit"))
}

/// 用编辑器打开。没有指定编辑器时用系统的文本编辑器
/// （不用文件关联的默认程序：Windows 上 .bat、.sh 这类文件双击会直接运行）
async fn launch(path: &Path, editor: Option<&str>) -> CmdResult<()> {
    let editor = editor.map(str::trim).filter(|s| !s.is_empty());
    #[cfg(target_os = "macos")]
    {
        let mut cmd = tokio::process::Command::new("open");
        match editor {
            Some(app) => cmd.arg("-a").arg(app),
            None => cmd.arg("-t"),
        };
        let out = cmd.arg(path).output().await.map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(tr!(
                "打开编辑器失败：{}",
                "Couldn't open the editor: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        #[cfg(windows)]
        let default = "notepad.exe";
        #[cfg(not(windows))]
        let default = "xdg-open";
        std::process::Command::new(editor.unwrap_or(default))
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| tr!("打开编辑器失败：{e}", "Couldn't open the editor: {e}"))
    }
}

/// 每秒检查一次正在编辑的文件，保存过就传回服务器
pub fn start_watcher(app: AppHandle) {
    // 上次运行留下的临时文件
    if let Ok(root) = edit_root(&app) {
        let _ = std::fs::remove_dir_all(root);
    }
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let changed: Vec<u64> = {
                let state = app.state::<AppState>();
                let mut list = state.edits.lock().unwrap();
                list.iter_mut()
                    .filter_map(|e| {
                        let now = local_stamp(&e.local)?;
                        // 有的编辑器保存时先删再建，文件暂时不存在，下一秒再看
                        (now != e.seen).then(|| {
                            e.seen = now;
                            e.id
                        })
                    })
                    .collect()
            };
            for id in changed {
                upload(&app, id, false).await;
            }
        }
    });
}

async fn upload(app: &AppHandle, id: u64, force: bool) {
    let state = app.state::<AppState>();
    let Some((session, remote, local, remote_mtime)) = state
        .edits
        .lock()
        .unwrap()
        .iter()
        .find(|e| e.id == id)
        .map(|e| (e.session.clone(), e.remote.clone(), e.local.clone(), e.remote_mtime))
    else {
        return;
    };
    let event = |state: &'static str, error: Option<String>| EditEvent { id, session: session.clone(), remote: remote.clone(), state, error };
    let r: CmdResult<Option<u32>> = async {
        let s = state.sftp(&session).await?;
        if !force && let Ok((mtime, _)) = sftp::stat(&s, &remote).await
            && mtime != remote_mtime
        {
            return Ok(None);
        }
        let data = std::fs::read(&local).map_err(|e| e.to_string())?;
        sftp::write_file(&s, &remote, &data).await?;
        Ok(Some(sftp::stat(&s, &remote).await.map(|(m, _)| m).unwrap_or(0)))
    }
    .await;
    let ev = match r {
        Ok(Some(mtime)) => {
            if let Some(e) = state.edits.lock().unwrap().iter_mut().find(|e| e.id == id) {
                e.remote_mtime = mtime;
            }
            event("saved", None)
        }
        Ok(None) => event("conflict", None),
        Err(e) => event("failed", Some(e)),
    };
    let _ = app.emit("edit", ev);
}
