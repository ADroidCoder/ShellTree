//! 测登录时的验证码（keyboard-interactive）：在本机起一个要求“密码 + 验证码”的 SSH 服务器，用 ShellTree 的登录逻辑去连。
//! 用法：cargo run -p shelltree-core --example otp（需要系统里有 ssh-keygen）

use shelltree_core::conn::{self, AuthKind, Callbacks, ConnectParams, PromptAsk};
use shelltree_core::russh::keys::{self, PrivateKey};
use shelltree_core::russh::server::{self, Auth, Response};
use shelltree_core::russh::{MethodKind, MethodSet};
use std::borrow::Cow;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const PASSWORD: &str = "pw";
const CODE: &str = "123456";

#[derive(Clone, Copy)]
enum Mode {
    /// password 方式验证密码，再用 keyboard-interactive 问验证码（常见的 PAM + Google Authenticator 配置）
    PasswordThenCode,
    /// 关掉了 password 方式，keyboard-interactive 里先问密码再问验证码
    InteractiveOnly,
    /// 私钥 + 验证码
    KeyThenCode,
}

struct Server {
    mode: Mode,
    /// keyboard-interactive 进行到第几步
    step: usize,
    password_ok: bool,
}

fn only_interactive() -> MethodSet {
    MethodSet::from(&[MethodKind::KeyboardInteractive][..])
}

fn ask(prompt: &'static str) -> Auth {
    Auth::Partial { name: Cow::Borrowed(""), instructions: Cow::Borrowed(""), prompts: Cow::Owned(vec![(Cow::Borrowed(prompt), false)]) }
}

impl server::Handler for Server {
    type Error = shelltree_core::russh::Error;

    async fn auth_password(&mut self, _user: &str, password: &str) -> Result<Auth, Self::Error> {
        if matches!(self.mode, Mode::PasswordThenCode) && password == PASSWORD {
            self.password_ok = true;
            return Ok(Auth::Reject { proceed_with_methods: Some(only_interactive()), partial_success: true });
        }
        Ok(Auth::Reject { proceed_with_methods: Some(only_interactive()), partial_success: false })
    }

    async fn auth_publickey(&mut self, _user: &str, _key: &keys::PublicKey) -> Result<Auth, Self::Error> {
        if matches!(self.mode, Mode::KeyThenCode) {
            self.password_ok = true;
            return Ok(Auth::Reject { proceed_with_methods: Some(only_interactive()), partial_success: true });
        }
        Ok(Auth::reject())
    }

    async fn auth_keyboard_interactive<'a>(&'a mut self, _user: &str, _sub: &str, response: Option<Response<'a>>) -> Result<Auth, Self::Error> {
        let answer = response.and_then(|mut r| r.next()).map(|b| String::from_utf8_lossy(&b).into_owned());
        let reject = Auth::Reject { proceed_with_methods: Some(only_interactive()), partial_success: false };
        self.step += 1;
        Ok(match (self.mode, self.step, answer.as_deref()) {
            (Mode::PasswordThenCode | Mode::KeyThenCode, 1, _) if self.password_ok => ask("Verification code: "),
            (Mode::PasswordThenCode | Mode::KeyThenCode, 2, Some(CODE)) => Auth::Accept,
            (Mode::InteractiveOnly, 1, _) => ask("Password: "),
            (Mode::InteractiveOnly, 2, Some(PASSWORD)) => ask("Verification code: "),
            (Mode::InteractiveOnly, 3, Some(CODE)) => Auth::Accept,
            _ => {
                self.step = 0;
                reject
            }
        })
    }
}

async fn serve(mode: Mode, key: PrivateKey) -> u16 {
    let config = Arc::new(server::Config {
        keys: vec![key],
        auth_rejection_time: Duration::from_millis(10),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        ..Default::default()
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((s, _)) = listener.accept().await {
            let handler = Server { mode, step: 0, password_ok: false };
            let config = config.clone();
            tokio::spawn(async move {
                if let Ok(session) = server::run_stream(config, s, handler).await {
                    let _ = session.await;
                }
            });
        }
    });
    port
}

/// 回答验证码的“界面”：记下被问了什么，回答 code（None 表示用户取消）
fn answerer(code: Option<&'static str>, asked: Arc<Mutex<Vec<String>>>) -> PromptAsk {
    Arc::new(move |info| {
        asked.lock().unwrap().extend(info.prompts.iter().map(|p| p.prompt.clone()));
        let answers = code.map(|c| info.prompts.iter().map(|_| c.to_string()).collect());
        Box::pin(async move { answers })
    })
}

async fn login(port: u16, auth: AuthKind, code: Option<&'static str>) -> (Result<(), String>, Vec<String>) {
    login_with(port, auth, code, None).await
}

async fn login_with(port: u16, auth: AuthKind, code: Option<&'static str>, key: Option<String>) -> (Result<(), String>, Vec<String>) {
    let password = key.is_none().then(|| PASSWORD.to_string());
    let p = ConnectParams { host: "127.0.0.1".into(), port, user: "u".into(), auth, password, key_path: key, ..Default::default() };
    let asked = Arc::new(Mutex::new(Vec::new()));
    let cb = Callbacks { host_key: Arc::new(|_| Box::pin(async { true })), prompt: Some(answerer(code, asked.clone())) };
    let kh = std::env::temp_dir().join(format!("shelltree-otp-{}", std::process::id()));
    let r = conn::connect_chain(&[p], &cb, Some(kh)).await.map(|_| ()).map_err(|e| e.message);
    let asked = asked.lock().unwrap().clone();
    (r, asked)
}

#[tokio::main]
async fn main() {
    let dir = std::env::temp_dir().join(format!("shelltree-otp-key-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let key_path = dir.join("host");
    let ok = std::process::Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(&key_path)
        .status()
        .unwrap()
        .success();
    assert!(ok, "ssh-keygen failed");
    let key = keys::load_secret_key(&key_path, None).unwrap();

    let port = serve(Mode::PasswordThenCode, key.clone()).await;
    for auth in [AuthKind::Password, AuthKind::Auto] {
        let (r, asked) = login(port, auth, Some(CODE)).await;
        assert_eq!(r, Ok(()), "{auth:?}");
        // 密码自动填，只问验证码
        assert_eq!(asked, ["Verification code: "], "{auth:?}");
        println!("password + code ({auth:?}) ok");
    }
    let (r, _) = login(port, AuthKind::Password, Some("000000")).await;
    assert_eq!(r, Err(shelltree_core::AUTH_FAILED.into()));
    println!("wrong code rejected ok");
    let (r, _) = login(port, AuthKind::Password, None).await;
    assert_eq!(r, Err(shelltree_core::CANCELLED.into()));
    println!("cancel ok");

    let port = serve(Mode::InteractiveOnly, key.clone()).await;
    let (r, asked) = login(port, AuthKind::Password, Some(CODE)).await;
    assert_eq!(r, Ok(()));
    assert_eq!(asked, ["Verification code: "]);
    println!("interactive password + code ok");

    // 用户私钥就用服务器的那把，服务器这边不校验是哪把
    let port = serve(Mode::KeyThenCode, key).await;
    let (r, asked) = login_with(port, AuthKind::Key, Some(CODE), Some(key_path.to_string_lossy().into_owned())).await;
    assert_eq!(r, Ok(()));
    assert_eq!(asked, ["Verification code: "]);
    println!("key + code ok");

    let _ = std::fs::remove_dir_all(dir);
    println!("all ok");
}
