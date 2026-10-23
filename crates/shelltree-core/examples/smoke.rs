//! 对着一台真实的 SSH 服务器跑一遍：登录、终端、上传下载。
//! 用法：cargo run --release -p shelltree-core --example smoke -- host port user password

use shelltree_core::conn::{self, AuthKind, ConnectParams};
use shelltree_core::{sftp, shell::Shell};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

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
    let tmp = std::env::temp_dir().join(format!("shelltree-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let kh = tmp.join("known_hosts");
    let ask: conn::HostKeyAsk = Arc::new(|info| {
        println!("host key: {} {} changed={}", info.algorithm, info.fingerprint, info.changed);
        Box::pin(async { true })
    });
    let t = Instant::now();
    let c = conn::connect_with_known_hosts(&p, ask.clone(), Some(kh.clone())).await.unwrap();
    println!("connected in {:?}, os = {:?}", t.elapsed(), c.detect_os().await);
    // 第二次连接应该不再询问指纹
    let never: conn::HostKeyAsk = Arc::new(|_| Box::pin(async { panic!("should be known") }));
    conn::connect_with_known_hosts(&p, never, Some(kh.clone())).await.unwrap();
    // 错误密码
    let mut bad = p.clone();
    bad.password = Some("nope".into());
    println!("bad password: {:?}", conn::connect_with_known_hosts(&bad, ask.clone(), Some(kh)).await.err());

    // 终端
    let out = Arc::new(std::sync::Mutex::new(Vec::new()));
    let o = out.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let sh = Shell::open(&c, 80, 24, move |d| o.lock().unwrap().extend(d), move |s| { let _ = tx.send(s); }).await.unwrap();
    sh.write(b"echo hello-$((40+2)); stty size; exit 3\n").await.unwrap();
    let status = rx.await.unwrap();
    sh.ack(usize::MAX);
    let text = String::from_utf8_lossy(&out.lock().unwrap()).into_owned();
    assert!(text.contains("hello-42"), "{text}");
    assert!(text.contains("24 80"), "{text}");
    println!("shell ok, exit {status:?}");

    // 刷屏时的流控：不 ack 的话不再往界面发
    let got = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let g = got.clone();
    let sh = Shell::open(&c, 80, 24, move |d| { g.fetch_add(d.len(), Ordering::Relaxed); }, |_| {}).await.unwrap();
    sh.write(b"yes\n").await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    let paused = got.load(Ordering::Relaxed);
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    assert_eq!(paused, got.load(Ordering::Relaxed), "should pause without ack");
    sh.ack(usize::MAX);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(got.load(Ordering::Relaxed) > paused);
    sh.write(b"\x03exit\n").await.unwrap();
    println!("flow control ok ({paused} bytes before pause)");

    // SFTP
    let s = sftp::open(&c).await.unwrap();
    let home = sftp::home(&s).await.unwrap();
    println!("home = {home}");
    let big = tmp.join("big.bin");
    let data: Vec<u8> = (0..200u32 * 1024 * 1024 / 4).flat_map(|i| i.wrapping_mul(2654435761).to_le_bytes()).collect();
    std::fs::write(&big, &data).unwrap();
    let pr = Arc::new(sftp::Progress::default());
    let t = Instant::now();
    sftp::upload(s.clone(), big.clone(), home.clone(), pr.clone()).await.unwrap();
    let dt = t.elapsed().as_secs_f64();
    println!("upload 200MB: {:.2}s, {:.0} MB/s", dt, 200.0 / dt);
    let dl = tmp.join("dl");
    std::fs::create_dir_all(&dl).unwrap();
    let t = Instant::now();
    sftp::download(s.clone(), sftp::join(&home, "big.bin"), false, dl.clone(), Arc::new(Default::default())).await.unwrap();
    let dt = t.elapsed().as_secs_f64();
    println!("download 200MB: {:.2}s, {:.0} MB/s", dt, 200.0 / dt);
    assert!(std::fs::read(dl.join("big.bin")).unwrap() == data, "content mismatch");

    // 文件夹：很多小文件
    let dir = tmp.join("many");
    std::fs::create_dir_all(dir.join("sub/deeper")).unwrap();
    for i in 0..500 {
        std::fs::write(dir.join(if i % 3 == 0 { format!("sub/f{i}.txt") } else { format!("f{i}.txt") }), format!("file {i}\n").repeat(i % 50 + 1)).unwrap();
    }
    std::fs::write(dir.join("sub/deeper/x"), b"x").unwrap();
    let t = Instant::now();
    let pr = Arc::new(sftp::Progress::default());
    sftp::upload(s.clone(), dir.clone(), home.clone(), pr.clone()).await.unwrap();
    println!("upload 501 small files: {:?} ({} files)", t.elapsed(), pr.files_done.load(Ordering::Relaxed));
    let back = tmp.join("back");
    std::fs::create_dir_all(&back).unwrap();
    let t = Instant::now();
    sftp::download(s.clone(), sftp::join(&home, "many"), true, back.clone(), Arc::new(Default::default())).await.unwrap();
    println!("download folder: {:?}", t.elapsed());
    assert_eq!(std::fs::read(back.join("many/sub/f3.txt")).unwrap(), std::fs::read(dir.join("sub/f3.txt")).unwrap());
    let list = sftp::list(&s, &home).await.unwrap();
    println!("home list: {:?}", list.iter().map(|e| (&e.name, e.is_dir)).collect::<Vec<_>>());

    // 取消
    let pr = Arc::new(sftp::Progress::default());
    let p2 = pr.clone();
    tokio::spawn(async move { tokio::time::sleep(std::time::Duration::from_millis(100)).await; p2.cancelled.store(true, Ordering::Relaxed); });
    let r = sftp::upload(s.clone(), big.clone(), sftp::join(&home, "many"), pr).await;
    println!("cancel: {r:?}, partial left = {}", s.try_exists(sftp::join(&home, "many/big.bin")).await.unwrap());

    sftp::rename(&s, &sftp::join(&home, "big.bin"), &sftp::join(&home, "big2.bin")).await.unwrap();
    sftp::remove(&s, &sftp::join(&home, "many"), true).await.unwrap();
    sftp::remove(&s, &sftp::join(&home, "big2.bin"), false).await.unwrap();
    println!("after cleanup: {:?}", sftp::list(&s, &home).await.unwrap().iter().map(|e| &e.name).collect::<Vec<_>>());
    std::fs::remove_dir_all(&tmp).unwrap();
    println!("ALL OK");
}
