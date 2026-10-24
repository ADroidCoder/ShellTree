//! 终端：远程的是一个带 pty 的 SSH shell 通道，本地的是本机的 pty。
//!
//! 输出先攒在后端，界面处理完一批（ack）再发下一批，积压时自动合并成大块，
//! 减少前后端之间的调用次数，`cat 大文件` 或 `yes` 刷屏时界面也不会卡死。
//!
//! 注意不能靠“暂停读取通道”来限流：russh 收到数据就会扩大窗口，
//! 不读的话会卡住整条连接，同一条连接上的 SFTP 和心跳也会停。
//! 所以通道一直在读，界面实在跟不上时丢掉最旧的输出（反正刷屏时也看不清）。

use crate::{Result, tr};
use crate::conn::Connection;
use crate::log::SessionLog;
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize};
use russh::client::Msg;
use russh::{ChannelMsg, ChannelWriteHalf};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

/// 界面还没处理完的输出超过这么多字节就先不发
const HIGH_WATER: usize = 1024 * 1024;
/// 一批最多发这么多字节
const BATCH_MAX: usize = 512 * 1024;
/// 后端最多积压这么多，超出时丢掉最旧的部分
const MAX_PENDING: usize = 32 * 1024 * 1024;

#[derive(Default)]
struct Shared {
    pending: Mutex<Vec<u8>>,
    unacked: AtomicUsize,
    wake: Notify,
    closed: AtomicBool,
    /// 退出码，u32::MAX 表示没有收到
    status: AtomicU32,
    /// 会话日志。在读输出时写，界面跟不上丢掉的输出也会记下来
    log: Mutex<Option<SessionLog>>,
}

impl Shared {
    fn new() -> Arc<Shared> {
        Arc::new(Shared { status: AtomicU32::new(u32::MAX), ..Default::default() })
    }

    /// 收到一段输出
    fn push(&self, data: &[u8]) {
        if let Some(log) = self.log.lock().unwrap().as_mut() {
            log.write(data);
        }
        let mut p = self.pending.lock().unwrap();
        p.extend_from_slice(data);
        if p.len() > MAX_PENDING {
            // 从一个换行处截断，尽量不把一行或一个转义序列切成两半
            let cut = p.len() - MAX_PENDING / 2;
            let cut = p[cut..].iter().position(|&b| b == b'\n').map_or(cut, |i| cut + i + 1);
            p.drain(..cut);
        }
        drop(p);
        self.wake.notify_one();
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.wake.notify_one();
    }
}

/// 发给界面：等界面处理完上一批，再把这段时间攒下的一起发过去
fn spawn_sender(sh: Arc<Shared>, mut on_data: impl FnMut(Vec<u8>) + Send + 'static, on_exit: impl FnOnce(Option<u32>) + Send + 'static) {
    tokio::spawn(async move {
        loop {
            let notified = sh.wake.notified();
            if sh.unacked.load(Ordering::Acquire) <= HIGH_WATER {
                let chunk = {
                    let mut p = sh.pending.lock().unwrap();
                    if p.len() <= BATCH_MAX { std::mem::take(&mut *p) } else { p.drain(..BATCH_MAX).collect() }
                };
                if !chunk.is_empty() {
                    sh.unacked.fetch_add(chunk.len(), Ordering::AcqRel);
                    on_data(chunk);
                    continue;
                }
                if sh.closed.load(Ordering::Acquire) {
                    let s = sh.status.load(Ordering::Acquire);
                    on_exit((s != u32::MAX).then_some(s));
                    break;
                }
            }
            notified.await;
        }
    });
}

/// 本机的 pty
struct Local {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
}

enum Backend {
    Ssh(ChannelWriteHalf<Msg>),
    Local(Local),
}

pub struct Shell {
    backend: Backend,
    shared: Arc<Shared>,
}

impl Shell {
    /// 打开远程 shell。`on_data` 收到合并后的输出，`on_exit` 在 shell 结束时调用一次（参数是退出码）
    pub async fn open(
        conn: &Connection,
        cols: u32,
        rows: u32,
        on_data: impl FnMut(Vec<u8>) + Send + 'static,
        on_exit: impl FnOnce(Option<u32>) + Send + 'static,
    ) -> Result<Shell> {
        let ch = conn.handle.channel_open_session().await.map_err(|e| tr!("打开终端失败：{e}", "Couldn't open terminal: {e}"))?;
        ch.request_pty(false, "xterm-256color", cols, rows, 0, 0, &[])
            .await
            .map_err(|e| tr!("申请终端失败：{e}", "PTY request failed: {e}"))?;
        // 让远程程序知道终端支持真彩色
        let _ = ch.set_env(false, "COLORTERM", "truecolor").await;
        ch.request_shell(false).await.map_err(|e| tr!("启动 shell 失败：{e}", "Couldn't start shell: {e}"))?;
        let (mut reader, writer) = ch.split();
        let shared = Shared::new();

        // 读通道，永远不阻塞
        let sh = shared.clone();
        tokio::spawn(async move {
            while let Some(msg) = reader.wait().await {
                match msg {
                    ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => sh.push(&data),
                    ChannelMsg::ExitStatus { exit_status } => sh.status.store(exit_status, Ordering::Release),
                    ChannelMsg::Close => break,
                    _ => {}
                }
            }
            sh.close();
        });
        spawn_sender(shared.clone(), on_data, on_exit);
        Ok(Shell { backend: Backend::Ssh(writer), shared })
    }

    /// 打开本机的 shell：macOS 和 Linux 上是用户的默认 shell（登录方式启动，PATH 和平时一样），Windows 上是 PowerShell
    pub fn open_local(
        cols: u32,
        rows: u32,
        on_data: impl FnMut(Vec<u8>) + Send + 'static,
        on_exit: impl FnOnce(Option<u32>) + Send + 'static,
    ) -> Result<Shell> {
        fn err(e: impl std::fmt::Display) -> String {
            tr!("打开本地终端失败：{e}", "Couldn't open the local terminal: {e}")
        }
        let pty = portable_pty::native_pty_system()
            .openpty(PtySize { rows: rows as u16, cols: cols as u16, pixel_width: 0, pixel_height: 0 })
            .map_err(err)?;
        #[cfg(windows)]
        let mut cmd = {
            let mut c = CommandBuilder::new("powershell.exe");
            c.arg("-NoLogo");
            c
        };
        #[cfg(not(windows))]
        let mut cmd = CommandBuilder::new_default_prog();
        if let Some(home) = crate::conn::home_dir() {
            cmd.cwd(home);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "ShellTree");
        // 从访达启动的程序没有 LANG，shell 里中文会显示成乱码
        #[cfg(not(windows))]
        if std::env::var_os("LANG").is_none() && std::env::var_os("LC_ALL").is_none() {
            cmd.env("LANG", "en_US.UTF-8");
        }
        let mut child = pty.slave.spawn_command(cmd).map_err(err)?;
        // 子进程已经拿到了 slave 这头，这边关掉，shell 退出时读才会结束
        drop(pty.slave);
        let mut reader = pty.master.try_clone_reader().map_err(err)?;
        let writer = pty.master.take_writer().map_err(err)?;
        let killer = child.clone_killer();
        let shared = Shared::new();

        // pty 只能阻塞着读，单独开线程
        let sh = shared.clone();
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 64 * 1024];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                sh.push(&buf[..n]);
            }
        });
        // 等 shell 退出，记下退出码再标记结束（读线程不标记：它可能比这边先结束，退出码就丢了）。
        // Windows 上 shell 退出后读不一定马上结束，稍等一下让最后的输出先读完
        let sh = shared.clone();
        std::thread::spawn(move || {
            let code = child.wait().map(|s| s.exit_code()).unwrap_or(u32::MAX);
            sh.status.store(code, Ordering::Release);
            std::thread::sleep(Duration::from_millis(200));
            sh.close();
        });
        spawn_sender(shared.clone(), on_data, on_exit);
        Ok(Shell {
            backend: Backend::Local(Local { master: Mutex::new(pty.master), writer: Arc::new(Mutex::new(writer)), killer: Mutex::new(killer) }),
            shared,
        })
    }

    pub async fn write(&self, data: &[u8]) -> Result<()> {
        match &self.backend {
            Backend::Ssh(w) => w.data_bytes(data.to_vec()).await.map_err(|e| tr!("发送失败：{e}", "Send failed: {e}")),
            Backend::Local(l) => {
                // 程序不读输入时写 pty 会阻塞（比如粘贴一大段），放到单独的线程里写
                let (w, data) = (l.writer.clone(), data.to_vec());
                tokio::task::spawn_blocking(move || {
                    let mut w = w.lock().unwrap();
                    w.write_all(&data).and_then(|_| w.flush())
                })
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| tr!("发送失败：{e}", "Send failed: {e}"))
            }
        }
    }

    pub async fn resize(&self, cols: u32, rows: u32) -> Result<()> {
        match &self.backend {
            Backend::Ssh(w) => w.window_change(cols, rows, 0, 0).await.map_err(|e| e.to_string()),
            Backend::Local(l) => l
                .master
                .lock()
                .unwrap()
                .resize(PtySize { rows: rows as u16, cols: cols as u16, pixel_width: 0, pixel_height: 0 })
                .map_err(|e| e.to_string()),
        }
    }

    /// 开始或停止记录会话日志
    pub fn set_log(&self, log: Option<SessionLog>) {
        *self.shared.log.lock().unwrap() = log;
    }

    /// 界面处理完了这么多字节的输出
    pub fn ack(&self, n: usize) {
        let _ = self
            .shared
            .unacked
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| Some(v.saturating_sub(n)));
        self.shared.wake.notify_one();
    }

    pub async fn close(&self) {
        match &self.backend {
            Backend::Ssh(w) => {
                let _ = w.close().await;
            }
            Backend::Local(l) => {
                let _ = l.killer.lock().unwrap().kill();
            }
        }
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        // 本地终端的标签页关了，shell 也要结束，不然会留在后台
        if let Backend::Local(l) = &self.backend {
            let _ = l.killer.lock().unwrap().kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn local_shell() {
        // 用 /bin/sh：用户自己的 zsh 配置启动慢、提示符花样多，测试结果不稳定
        #[cfg(unix)]
        unsafe {
            std::env::set_var("SHELL", "/bin/sh");
        }
        let out = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = tokio::sync::oneshot::channel();
        let o = out.clone();
        let sh = Shell::open_local(80, 24, move |d| o.lock().unwrap().extend_from_slice(&d), move |s| {
            let _ = tx.send(s);
        })
        .unwrap();
        sh.resize(100, 30).await.unwrap();
        let log = std::env::temp_dir().join(format!("shelltree-local-{}.log", std::process::id()));
        sh.set_log(Some(SessionLog::open(&log, false, "").unwrap()));
        // shell 启动时会丢掉提前输入的内容（zsh 就是），等输出停下来、出现提示符再输入。
        // Windows 的 ConPTY 启动时会问光标位置（ESC[6n），不回答就一直等着，界面里是 xterm 回答的，这里自己回答
        let mut last = (0, std::time::Instant::now());
        let mut answered = false;
        for _ in 0..300 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let (n, asked) = {
                let o = out.lock().unwrap();
                (o.len(), o.windows(4).any(|w| w == b"\x1b[6n"))
            };
            if asked && !answered {
                answered = true;
                sh.write(b"\x1b[1;1R").await.unwrap();
            }
            if n != last.0 {
                last = (n, std::time::Instant::now());
            } else if n > 0 && last.1.elapsed() > Duration::from_millis(800) {
                break;
            }
        }
        sh.write(b"echo hello-shelltree\r").await.unwrap();
        sh.write(b"exit 3\r").await.unwrap();
        let status = match tokio::time::timeout(Duration::from_secs(30), rx).await {
            Ok(s) => s.unwrap(),
            Err(_) => panic!("shell didn't exit, output: {:?}", String::from_utf8_lossy(&out.lock().unwrap())),
        };
        let text = String::from_utf8_lossy(&out.lock().unwrap()).into_owned();
        assert_eq!(status, Some(3), "{text}");
        // 回显的命令里也有这个字符串，所以要出现两次
        assert!(text.matches("hello-shelltree").count() >= 2, "{text}");
        drop(sh);
        assert!(std::fs::read_to_string(&log).unwrap().contains("hello-shelltree"));
        let _ = std::fs::remove_file(log);
    }
}
