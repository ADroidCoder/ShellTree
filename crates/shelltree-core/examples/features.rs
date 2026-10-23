//! 对着一台真实的 SSH 服务器测跳板机、端口转发、编辑远程文件。服务器要允许 TCP 转发（AllowTcpForwarding yes）。
//! 用法：cargo run -p shelltree-core --example features -- host port user password

use shelltree_core::conn::{self, AuthKind, Callbacks, ConnectParams};
use shelltree_core::forward::{self, ForwardKind, ForwardRule};
use shelltree_core::sftp;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// 连上去读 SSH 服务器的欢迎行（SSH-2.0-...），用来确认转发通了
async fn banner(mut s: TcpStream) -> String {
    let mut buf = vec![0u8; 256];
    let n = tokio::time::timeout(Duration::from_secs(5), s.read(&mut buf)).await.expect("banner timeout").unwrap();
    String::from_utf8_lossy(&buf[..n]).lines().next().unwrap_or("").to_string()
}

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let p = ConnectParams {
        host: a[1].clone(),
        port: a[2].parse().unwrap(),
        user: a[3].clone(),
        auth: AuthKind::Password,
        password: Some(a[4].clone()),
        ..Default::default()
    };
    let tmp = std::env::temp_dir().join(format!("shelltree-features-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let cb = Callbacks { host_key: Arc::new(|_| Box::pin(async { true })), prompt: None };

    // 跳板机：先登录服务器，再通过它连服务器自己的 22 端口
    let inner = ConnectParams { host: "127.0.0.1".into(), port: 22, ..p.clone() };
    let c = conn::connect_chain(&[p.clone(), inner.clone()], &cb, Some(tmp.join("known_hosts"))).await.unwrap();
    println!("jump ok, os = {:?}", c.detect_os().await);
    // 跳板机那一跳的密码错误，错误要指出是第几跳
    let mut bad = p.clone();
    bad.password = Some("nope".into());
    let e = conn::connect_chain(&[bad, inner], &cb, Some(tmp.join("known_hosts"))).await.err().unwrap();
    assert_eq!((e.hop, e.message.as_str()), (0, shelltree_core::AUTH_FAILED));
    println!("jump auth error ok: hop {}", e.hop);
    c.disconnect().await;

    let c = Arc::new(conn::connect_with_known_hosts(&p, cb.host_key.clone(), Some(tmp.join("known_hosts"))).await.unwrap());

    // 本地转发：本机随机端口 → 服务器上的 127.0.0.1:22
    let rule = ForwardRule { kind: ForwardKind::Local, dest_host: "127.0.0.1".into(), dest_port: 22, ..Default::default() };
    let f = forward::start(c.clone(), &rule).await.unwrap();
    let b = banner(TcpStream::connect(("127.0.0.1", f.port)).await.unwrap()).await;
    assert!(b.starts_with("SSH-2.0"), "{b}");
    println!("local forward ok on {}: {b}", f.port);
    let port = f.port;
    f.stop().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(TcpStream::connect(("127.0.0.1", port)).await.is_err(), "still listening after stop");

    // 动态转发：SOCKS5 CONNECT 到服务器上的 127.0.0.1:22
    let rule = ForwardRule { kind: ForwardKind::Dynamic, ..Default::default() };
    let f = forward::start(c.clone(), &rule).await.unwrap();
    let mut s = TcpStream::connect(("127.0.0.1", f.port)).await.unwrap();
    s.write_all(&[5, 1, 0]).await.unwrap();
    let mut r = [0u8; 2];
    s.read_exact(&mut r).await.unwrap();
    assert_eq!(r, [5, 0]);
    s.write_all(&[5, 1, 0, 3, 9]).await.unwrap();
    s.write_all(b"localhost").await.unwrap();
    s.write_all(&22u16.to_be_bytes()).await.unwrap();
    let mut r = [0u8; 10];
    s.read_exact(&mut r).await.unwrap();
    assert_eq!(r[1], 0, "socks reply {r:?}");
    let b = banner(s).await;
    assert!(b.starts_with("SSH-2.0"), "{b}");
    println!("socks ok on {}: {b}", f.port);
    drop(f);

    // 远程转发：服务器上的 127.0.0.1:19000 → 本机一个回显服务
    let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let echo_port = echo.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut s, _)) = echo.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                while let Ok(n) = s.read(&mut buf).await {
                    if n == 0 || s.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    let rule = ForwardRule { kind: ForwardKind::Remote, bind_port: 19000, dest_host: "127.0.0.1".into(), dest_port: echo_port, ..Default::default() };
    let f = forward::start(c.clone(), &rule).await.unwrap();
    let out = exec(&c, "echo remote-forward-ok | nc -w 2 127.0.0.1 19000").await;
    assert!(out.contains("remote-forward-ok"), "{out:?}");
    println!("remote forward ok: {}", out.trim());
    f.stop().await;

    // 大量数据经过本地转发：服务器上 cat 一个 20 MB 的文件到端口，确认完整
    exec(&c, "head -c 20000000 /dev/urandom > /tmp/big.bin").await;
    let rule = ForwardRule { kind: ForwardKind::Local, dest_host: "127.0.0.1".into(), dest_port: 19001, ..Default::default() };
    let f = forward::start(c.clone(), &rule).await.unwrap();
    let c2 = c.clone();
    let server = tokio::spawn(async move { exec(&c2, "nc -l -p 19001 < /tmp/big.bin; echo done").await });
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut s = TcpStream::connect(("127.0.0.1", f.port)).await.unwrap();
    let t = std::time::Instant::now();
    let mut got = Vec::new();
    s.read_to_end(&mut got).await.unwrap();
    println!("forwarded {} bytes in {:?}", got.len(), t.elapsed());
    assert_eq!(got.len(), 20_000_000);
    // 关掉本机这头，服务器上的 nc 才会退出
    drop(s);
    server.await.unwrap();
    drop(f);

    // 编辑远程文件：覆盖写，新内容比原来短时不能留下旧内容的尾巴
    let s = sftp::open(&c).await.unwrap();
    sftp::write_file(&s, "/tmp/edit.txt", b"a long original line\n").await.unwrap();
    exec(&c, "chmod 751 /tmp/edit.txt").await;
    sftp::write_file(&s, "/tmp/edit.txt", b"short\n").await.unwrap();
    assert_eq!(sftp::read_file(&s, "/tmp/edit.txt").await.unwrap(), b"short\n");
    let perm = exec(&c, "stat -c %a /tmp/edit.txt").await;
    assert_eq!(perm.trim(), "751");
    println!("edit write ok, perm kept {}", perm.trim());

    c.disconnect().await;
    let _ = std::fs::remove_dir_all(tmp);
    println!("all ok");
}

async fn exec(c: &conn::Connection, cmd: &str) -> String {
    let mut ch = c.handle.channel_open_session().await.unwrap();
    ch.exec(true, cmd).await.unwrap();
    let mut out = Vec::new();
    while let Some(msg) = ch.wait().await {
        match msg {
            shelltree_core::russh::ChannelMsg::Data { data } => out.extend_from_slice(&data),
            shelltree_core::russh::ChannelMsg::Close => break,
            _ => {}
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
