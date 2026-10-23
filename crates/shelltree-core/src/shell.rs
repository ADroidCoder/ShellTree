//! 远程终端：一个带 pty 的 shell 通道。
//!
//! 输出先攒在后端，界面处理完一批（ack）再发下一批，积压时自动合并成大块，
//! 减少前后端之间的调用次数，`cat 大文件` 或 `yes` 刷屏时界面也不会卡死。
//!
//! 注意不能靠“暂停读取通道”来限流：russh 收到数据就会扩大窗口，
//! 不读的话会卡住整条连接，同一条连接上的 SFTP 和心跳也会停。
//! 所以通道一直在读，界面实在跟不上时丢掉最旧的输出（反正刷屏时也看不清）。

use crate::{Result, tr};
use crate::conn::Connection;
use russh::client::Msg;
use russh::{ChannelMsg, ChannelWriteHalf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
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
}

pub struct Shell {
    writer: ChannelWriteHalf<Msg>,
    shared: Arc<Shared>,
}

impl Shell {
    /// 打开 shell。`on_data` 收到合并后的输出，`on_exit` 在 shell 结束时调用一次（参数是退出码）
    pub async fn open(
        conn: &Connection,
        cols: u32,
        rows: u32,
        mut on_data: impl FnMut(Vec<u8>) + Send + 'static,
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
        let shared = Arc::new(Shared { status: AtomicU32::new(u32::MAX), ..Default::default() });

        // 读通道，永远不阻塞
        let sh = shared.clone();
        tokio::spawn(async move {
            while let Some(msg) = reader.wait().await {
                match msg {
                    ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                        let mut p = sh.pending.lock().unwrap();
                        p.extend_from_slice(&data);
                        if p.len() > MAX_PENDING {
                            // 从一个换行处截断，尽量不把一行或一个转义序列切成两半
                            let cut = p.len() - MAX_PENDING / 2;
                            let cut = p[cut..].iter().position(|&b| b == b'\n').map_or(cut, |i| cut + i + 1);
                            p.drain(..cut);
                        }
                        drop(p);
                        sh.wake.notify_one();
                    }
                    ChannelMsg::ExitStatus { exit_status } => sh.status.store(exit_status, Ordering::Release),
                    ChannelMsg::Close => break,
                    _ => {}
                }
            }
            sh.closed.store(true, Ordering::Release);
            sh.wake.notify_one();
        });

        // 发给界面：等界面处理完上一批，再把这段时间攒下的一起发过去
        let sh = shared.clone();
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
        Ok(Shell { writer, shared })
    }

    pub async fn write(&self, data: &[u8]) -> Result<()> {
        self.writer.data_bytes(data.to_vec()).await.map_err(|e| tr!("发送失败：{e}", "Send failed: {e}"))
    }

    pub async fn resize(&self, cols: u32, rows: u32) -> Result<()> {
        self.writer.window_change(cols, rows, 0, 0).await.map_err(|e| e.to_string())
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
        let _ = self.writer.close().await;
    }
}
