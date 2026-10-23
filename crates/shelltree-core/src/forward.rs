//! 端口转发：本地转发（ssh -L）、远程转发（ssh -R）、动态转发（ssh -D，SOCKS5 代理）。
//!
//! 每个转发进来的连接对应一个 SSH 通道。和终端一样，通道必须一直读（见 shell.rs 的说明），
//! 本机程序读得慢时数据先放在内存里，积压太多就断开这一个连接，不会卡住整条 SSH 连接。

use crate::conn::Connection;
use crate::{Result, tr};
use russh::client::Msg;
use russh::{Channel, ChannelMsg};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, mpsc};
use tokio::task::{JoinHandle, JoinSet};

/// 一个转发连接最多积压这么多还没写给本机程序的数据
const MAX_BACKLOG: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForwardKind {
    /// 本机端口 → 经服务器 → 目标地址
    #[default]
    Local,
    /// 服务器端口 → 经本机 → 目标地址
    Remote,
    /// 本机 SOCKS5 代理，目标地址由使用代理的程序决定
    Dynamic,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ForwardRule {
    pub kind: ForwardKind,
    /// 监听的地址，空表示 127.0.0.1（远程转发时是服务器上的地址）
    pub bind_host: String,
    pub bind_port: u16,
    /// 动态转发不用
    pub dest_host: String,
    pub dest_port: u16,
}

/// 远程转发的目标，按服务器上监听的端口查
#[derive(Clone)]
pub(crate) struct RemoteTarget {
    host: String,
    port: u16,
    active: Arc<AtomicUsize>,
}

pub(crate) type RemoteForwards = Arc<Mutex<HashMap<u32, RemoteTarget>>>;

/// 一个正在运行的转发，丢掉时停止（已经建立的连接也会断开）
pub struct Forward {
    /// 实际监听的端口（规则里写 0 时由系统或服务器分配）
    pub port: u16,
    active: Arc<AtomicUsize>,
    task: Option<JoinHandle<()>>,
    /// 远程转发：服务器上监听的地址，停止时要告诉服务器
    remote: Option<(Arc<Connection>, String, u32)>,
}

impl Forward {
    /// 当前有几个连接正在通过这个转发
    pub fn active(&self) -> usize {
        self.active.load(Ordering::Relaxed)
    }

    /// 停止并等到端口真正释放（马上重连时要用同一个端口）
    pub async fn stop(mut self) {
        if let Some(t) = self.task.take() {
            t.abort();
            let _ = t.await;
        }
        if let Some((conn, addr, port)) = self.remote.take() {
            conn.remote_forwards.lock().unwrap().remove(&port);
            let _ = tokio::time::timeout(Duration::from_secs(3), conn.handle.cancel_tcpip_forward(addr, port)).await;
        }
    }
}

impl Drop for Forward {
    fn drop(&mut self) {
        if let Some(t) = self.task.take() {
            t.abort();
        }
        if let Some((conn, _, port)) = &self.remote {
            conn.remote_forwards.lock().unwrap().remove(port);
        }
    }
}

fn bind_host(rule: &ForwardRule) -> &str {
    if rule.bind_host.trim().is_empty() { "127.0.0.1" } else { rule.bind_host.trim() }
}

pub async fn start(conn: Arc<Connection>, rule: &ForwardRule) -> Result<Forward> {
    let active = Arc::new(AtomicUsize::new(0));
    let host = bind_host(rule).to_string();
    if rule.kind != ForwardKind::Dynamic && (rule.dest_host.trim().is_empty() || rule.dest_port == 0) {
        return Err(tr!("没有填写目标地址", "The destination is missing"));
    }
    if rule.kind == ForwardKind::Remote {
        let port = conn
            .handle
            .tcpip_forward(host.as_str(), rule.bind_port as u32)
            .await
            .map_err(|e| tr!("服务器拒绝了远程转发 {host}:{}：{e}", "The server refused remote forwarding on {host}:{}: {e}", rule.bind_port))?;
        // 端口写 0 时服务器返回分配的端口，否则返回 0
        let port = if rule.bind_port == 0 { port } else { rule.bind_port as u32 };
        let target = RemoteTarget { host: rule.dest_host.trim().to_string(), port: rule.dest_port, active: active.clone() };
        conn.remote_forwards.lock().unwrap().insert(port, target);
        return Ok(Forward { port: port as u16, active, task: None, remote: Some((conn, host, port)) });
    }
    let listener = TcpListener::bind((host.as_str(), rule.bind_port))
        .await
        .map_err(|e| tr!("无法监听 {host}:{}：{e}", "Can't listen on {host}:{}: {e}", rule.bind_port))?;
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(rule.bind_port);
    let (rule, count) = (rule.clone(), active.clone());
    let task = tokio::spawn(async move {
        // 停止转发时 JoinSet 跟着被丢掉，里面的连接全部断开
        let mut conns = JoinSet::new();
        loop {
            tokio::select! {
                r = listener.accept() => {
                    let Ok((tcp, peer)) = r else { continue };
                    let _ = tcp.set_nodelay(true);
                    let (conn, rule, count) = (conn.clone(), rule.clone(), count.clone());
                    conns.spawn(async move {
                        count.fetch_add(1, Ordering::Relaxed);
                        let _ = match rule.kind {
                            ForwardKind::Dynamic => serve_socks(&conn, tcp, peer).await,
                            _ => serve_local(&conn, tcp, peer, &rule.dest_host, rule.dest_port).await,
                        };
                        count.fetch_sub(1, Ordering::Relaxed);
                    });
                }
                // 回收结束的连接
                Some(_) = conns.join_next(), if !conns.is_empty() => {}
            }
        }
    });
    Ok(Forward { port, active, task: Some(task), remote: None })
}

async fn open(conn: &Connection, host: &str, port: u16, peer: SocketAddr) -> Result<Channel<Msg>> {
    conn.handle
        .channel_open_direct_tcpip(host, port as u32, peer.ip().to_string(), peer.port() as u32)
        .await
        .map_err(|e| e.to_string())
}

async fn serve_local(conn: &Connection, tcp: TcpStream, peer: SocketAddr, host: &str, port: u16) -> Result<()> {
    let ch = open(conn, host.trim(), port, peer).await?;
    relay(tcp, ch).await;
    Ok(())
}

pub(crate) async fn serve_remote(ch: Channel<Msg>, target: RemoteTarget) {
    target.active.fetch_add(1, Ordering::Relaxed);
    match TcpStream::connect((target.host.as_str(), target.port)).await {
        Ok(tcp) => {
            let _ = tcp.set_nodelay(true);
            relay(tcp, ch).await;
        }
        Err(_) => {
            let _ = ch.close().await;
        }
    }
    target.active.fetch_sub(1, Ordering::Relaxed);
}

/// SOCKS5（只支持 CONNECT，不需要用户名密码）
async fn serve_socks(conn: &Connection, mut tcp: TcpStream, peer: SocketAddr) -> Result<()> {
    let handshake = async {
        let mut head = [0u8; 2];
        tcp.read_exact(&mut head).await?;
        if head[0] != 5 {
            return Ok(None);
        }
        let mut methods = vec![0u8; head[1] as usize];
        tcp.read_exact(&mut methods).await?;
        if !methods.contains(&0) {
            tcp.write_all(&[5, 0xff]).await?;
            return Ok(None);
        }
        tcp.write_all(&[5, 0]).await?;
        let mut req = [0u8; 4];
        tcp.read_exact(&mut req).await?;
        let host = match req[3] {
            1 => {
                let mut a = [0u8; 4];
                tcp.read_exact(&mut a).await?;
                std::net::Ipv4Addr::from(a).to_string()
            }
            3 => {
                let mut len = [0u8; 1];
                tcp.read_exact(&mut len).await?;
                let mut name = vec![0u8; len[0] as usize];
                tcp.read_exact(&mut name).await?;
                String::from_utf8_lossy(&name).into_owned()
            }
            4 => {
                let mut a = [0u8; 16];
                tcp.read_exact(&mut a).await?;
                std::net::Ipv6Addr::from(a).to_string()
            }
            _ => return Ok(None),
        };
        let mut port = [0u8; 2];
        tcp.read_exact(&mut port).await?;
        if req[1] != 1 {
            // 只支持 CONNECT
            tcp.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
            return Ok(None);
        }
        std::io::Result::Ok(Some((host, u16::from_be_bytes(port))))
    };
    let target = tokio::time::timeout(Duration::from_secs(10), handshake).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    let Some((host, port)) = target else { return Ok(()) };
    match open(conn, &host, port, peer).await {
        Ok(ch) => {
            tcp.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await.map_err(|e| e.to_string())?;
            relay(tcp, ch).await;
        }
        Err(_) => {
            // 5 = 连接被拒绝
            let _ = tcp.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await;
        }
    }
    Ok(())
}

/// 在本机 TCP 连接和 SSH 通道之间双向转发，直到任一方关闭
async fn relay(tcp: TcpStream, ch: Channel<Msg>) {
    let (mut rd, wr) = ch.split();
    let (mut tcp_rd, mut tcp_wr) = tcp.into_split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let backlog = AtomicUsize::new(0);
    let overflow = Notify::new();

    // 通道 → 本机：一直读通道，写不过来的先排队
    let down = async {
        let mut tx = Some(tx);
        while let Some(msg) = rd.wait().await {
            match msg {
                ChannelMsg::Data { data } => {
                    if backlog.fetch_add(data.len(), Ordering::AcqRel) + data.len() > MAX_BACKLOG {
                        overflow.notify_one();
                        return;
                    }
                    if let Some(tx) = &tx {
                        let _ = tx.send(data.to_vec());
                    }
                }
                // 服务器那边不再发数据了：写完排队的数据后关闭本机连接的写入，但还可以继续往服务器发
                ChannelMsg::Eof => tx = None,
                ChannelMsg::Close => break,
                _ => {}
            }
        }
    };
    let write = async {
        while let Some(b) = rx.recv().await {
            backlog.fetch_sub(b.len(), Ordering::AcqRel);
            if tcp_wr.write_all(&b).await.is_err() {
                return;
            }
        }
        let _ = tcp_wr.shutdown().await;
    };
    // 本机 → 通道。本机程序关闭写入后告诉服务器，但还要继续收服务器的回复
    let up = async {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match tcp_rd.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if wr.data_bytes(buf[..n].to_vec()).await.is_err() {
                        break;
                    }
                }
            }
        }
        let _ = wr.eof().await;
        std::future::pending::<()>().await
    };
    tokio::select! {
        _ = async { tokio::join!(down, write) } => {}
        _ = up => {}
        _ = overflow.notified() => {}
    }
    let _ = wr.close().await;
}
