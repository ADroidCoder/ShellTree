//! 建立 SSH 连接：校验主机指纹、登录。终端和 SFTP 共用同一条连接。

use crate::forward::{self, RemoteForwards};
use crate::{AUTH_FAILED, BAD_PASSPHRASE, CANCELLED, NEED_PASSPHRASE, NEED_PASSWORD, Result, tr};
use russh::client::{self, AuthResult, ChannelOpenHandle, Handle, KeyboardInteractiveAuthResponse, Msg, Session};
use russh::keys::{self, HashAlg, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate};
use russh::{Channel, MethodKind};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthKind {
    /// 和命令行 ssh 一样：先试 ssh-agent 和 ~/.ssh 里的默认密钥，不行再用密码
    #[default]
    Auto,
    Password,
    Key,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConnectParams {
    pub host: String,
    pub port: u16,
    pub user: String,
    #[serde(default)]
    pub auth: AuthKind,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub key_path: Option<String>,
    #[serde(default)]
    pub passphrase: Option<String>,
}

/// 遇到没见过或变了的主机指纹时交给界面问用户
#[derive(Debug, Clone, Serialize)]
pub struct HostKeyInfo {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub fingerprint: String,
    /// true 表示这台主机以前记录的指纹和现在的不一样（可能被中间人攻击）
    pub changed: bool,
}

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
/// 返回 true 表示信任这个指纹并继续连接
pub type HostKeyAsk = Arc<dyn Fn(HostKeyInfo) -> BoxFuture<bool> + Send + Sync>;

/// 服务器在登录时提的问题，例如验证码（keyboard-interactive）
#[derive(Debug, Clone, Serialize)]
pub struct PromptInfo {
    /// 正在登录哪台主机，user@host
    pub host: String,
    pub name: String,
    pub instructions: String,
    pub prompts: Vec<PromptItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptItem {
    pub prompt: String,
    /// false 表示输入时不显示（密码、验证码）
    pub echo: bool,
}

/// 返回每个问题的回答；None 表示用户取消
pub type PromptAsk = Arc<dyn Fn(PromptInfo) -> BoxFuture<Option<Vec<String>>> + Send + Sync>;

/// 连接过程中需要问用户的事
#[derive(Clone)]
pub struct Callbacks {
    pub host_key: HostKeyAsk,
    /// 没有时遇到验证码之类的问题直接算登录失败
    pub prompt: Option<PromptAsk>,
}

/// 连接失败：hop 是出错的那一跳（0 是第一台跳板机，最后一个是目标主机）
#[derive(Debug)]
pub struct ConnectError {
    pub hop: usize,
    pub message: String,
}

pub struct Client {
    host: String,
    port: u16,
    ask: HostKeyAsk,
    known_hosts: Option<PathBuf>,
    remote_forwards: RemoteForwards,
}

impl client::Handler for Client {
    type Error = russh::Error;

    /// 远程转发（-R）有新连接进来
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<Msg>,
        _connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> std::result::Result<(), Self::Error> {
        // 没有对应的转发规则时 reply 被丢掉，自动拒绝
        let target = self.remote_forwards.lock().unwrap().get(&connected_port).cloned();
        if let Some(target) = target {
            reply.accept().await;
            tokio::spawn(forward::serve_remote(channel, target));
        }
        Ok(())
    }

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> std::result::Result<bool, Self::Error> {
        // 证书形式的主机密钥暂不支持
        let PublicKeyOrCertificate::PublicKey { key, .. } = key else { return Ok(false) };
        let Some(path) = self.known_hosts.clone() else { return Ok(false) };
        let (host, port) = (self.host.as_str(), self.port);
        let changed = match keys::known_hosts::check_known_hosts_path(host, port, key, &path) {
            Ok(true) => return Ok(true),
            Ok(false) => false,
            Err(keys::Error::KeyChanged { .. }) => true,
            // known_hosts 读不了时当作没见过，让用户确认
            Err(_) => false,
        };
        let info = HostKeyInfo {
            host: host.to_string(),
            port,
            algorithm: key.algorithm().to_string(),
            fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
            changed,
        };
        if !(self.ask)(info).await {
            return Ok(false);
        }
        if changed {
            let _ = forget_host(&path, host, port, key);
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = keys::known_hosts::learn_known_hosts_path(host, port, key, &path);
        Ok(true)
    }
}

/// 删掉 known_hosts 里这台主机同类型的旧指纹。
/// 行号的算法和 russh 读取时一致：注释行不计数。
fn forget_host(path: &Path, host: &str, port: u16, key: &PublicKey) -> std::io::Result<()> {
    let stale: Vec<usize> = keys::known_hosts::known_host_keys_path(host, port, path)
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, k)| k.algorithm() == key.algorithm())
        .map(|(line, _)| line)
        .collect();
    if stale.is_empty() {
        return Ok(());
    }
    let text = std::fs::read_to_string(path)?;
    let mut n = 0;
    let mut out = String::with_capacity(text.len());
    for l in text.split_inclusive('\n') {
        if !l.starts_with('#') {
            n += 1;
            if stale.contains(&n) {
                continue;
            }
        }
        out.push_str(l);
    }
    std::fs::write(path, out)
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::home_dir()
}

/// 支持 ~/ 开头的路径
pub fn expand_tilde(p: &str) -> PathBuf {
    match (p.strip_prefix("~/").or_else(|| p.strip_prefix("~\\")), home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(p),
    }
}

/// 一条已登录的连接
pub struct Connection {
    pub handle: Handle<Client>,
    /// 经过跳板机时，到跳板机的连接。要一直保持着，目标主机的连接是从它里面走的
    via: Option<Box<Connection>>,
    pub(crate) remote_forwards: RemoteForwards,
}

/// 依次登录 chain 里的主机：前面的是跳板机（和 ssh -J 一样），最后一个是目标主机
pub async fn connect(chain: &[ConnectParams], cb: &Callbacks) -> std::result::Result<Connection, ConnectError> {
    connect_chain(chain, cb, home_dir().map(|h| h.join(".ssh").join("known_hosts"))).await
}

/// 和 [connect] 一样，但指定 known_hosts 文件（测试时不碰用户自己的）
pub async fn connect_chain(
    chain: &[ConnectParams],
    cb: &Callbacks,
    known_hosts: Option<PathBuf>,
) -> std::result::Result<Connection, ConnectError> {
    let mut prev: Option<Connection> = None;
    for (hop, p) in chain.iter().enumerate() {
        let fail = |message| ConnectError { hop, message };
        let port = if p.port == 0 { 22 } else { p.port };
        let c = match prev.take() {
            None => {
                // 只限制建立 TCP 连接的时间，确认指纹时用户可能要想一会儿
                let stream = tokio::time::timeout(Duration::from_secs(15), tokio::net::TcpStream::connect((p.host.as_str(), port)))
                    .await
                    .map_err(|_| fail(tr!("连接 {}:{} 超时", "Connection to {}:{} timed out", p.host, port)))?
                    .map_err(|e| fail(tr!("无法连接 {}:{}：{e}", "Can't connect to {}:{}: {e}", p.host, port)))?;
                let _ = stream.set_nodelay(true);
                handshake(stream, p, cb, known_hosts.clone()).await.map_err(fail)?
            }
            Some(jump) => {
                // 让跳板机代为连接下一台，在这个通道里再走一遍 SSH
                let ch = tokio::time::timeout(
                    Duration::from_secs(15),
                    jump.handle.channel_open_direct_tcpip(p.host.as_str(), port as u32, "127.0.0.1", 0),
                )
                .await
                .map_err(|_| fail(tr!("通过跳板机连接 {}:{} 超时", "Connecting to {}:{} through the jump host timed out", p.host, port)))?
                .map_err(|e| fail(tr!("跳板机无法连接 {}:{}：{e}", "The jump host can't reach {}:{}: {e}", p.host, port)))?;
                let mut c = handshake(ch.into_stream(), p, cb, known_hosts.clone()).await.map_err(fail)?;
                c.via = Some(Box::new(jump));
                c
            }
        };
        prev = Some(c);
    }
    prev.ok_or_else(|| ConnectError { hop: 0, message: "empty chain".into() })
}

/// 只连一台主机，没有跳板机，也不回答验证码（测试用）
pub async fn connect_with_known_hosts(p: &ConnectParams, ask: HostKeyAsk, known_hosts: Option<PathBuf>) -> Result<Connection> {
    let cb = Callbacks { host_key: ask, prompt: None };
    connect_chain(std::slice::from_ref(p), &cb, known_hosts).await.map_err(|e| e.message)
}

async fn handshake<S>(stream: S, p: &ConnectParams, cb: &Callbacks, known_hosts: Option<PathBuf>) -> Result<Connection>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let config = client::Config {
        // 每 20 秒发一次心跳，连续 3 次没回应就算断线
        keepalive_interval: Some(Duration::from_secs(20)),
        keepalive_max: 3,
        inactivity_timeout: None,
        // 接收窗口和包大小调大，下载更快
        window_size: 16 * 1024 * 1024,
        maximum_packet_size: 65535,
        nodelay: true,
        ..Default::default()
    };
    let remote_forwards = RemoteForwards::default();
    let client = Client {
        host: p.host.clone(),
        port: p.port,
        ask: cb.host_key.clone(),
        known_hosts,
        remote_forwards: remote_forwards.clone(),
    };
    let mut handle = client::connect_stream(Arc::new(config), stream, client)
        .await
        .map_err(|e| match e {
            russh::Error::UnknownKey => tr!("没有信任服务器的主机指纹，已取消连接", "The host key wasn't trusted, connection cancelled"),
            e => tr!("SSH 握手失败：{e}", "SSH handshake failed: {e}"),
        })?;
    let mut auth = Auth { h: &mut handle, p, prompt: cb.prompt.clone(), partial: false };
    auth.run().await?;
    Ok(Connection { handle, via: None, remote_forwards })
}

/// 登录一台主机
struct Auth<'a> {
    h: &'a mut Handle<Client>,
    p: &'a ConnectParams,
    prompt: Option<PromptAsk>,
    /// 服务器已经接受了一步（例如私钥），但还要求再验证一步（通常是验证码）
    partial: bool,
}

impl Auth<'_> {
    async fn run(&mut self) -> Result<()> {
        let p = self.p;
        match p.auth {
            AuthKind::Password => {
                let Some(pw) = p.password.as_deref() else { return Err(NEED_PASSWORD.into()) };
                if self.password(pw).await? {
                    return Ok(());
                }
                Err(AUTH_FAILED.into())
            }
            AuthKind::Key => {
                let path = p.key_path.as_deref().filter(|s| !s.is_empty()).ok_or_else(|| tr!("没有选择私钥文件", "No private key file selected"))?;
                let key = load_key(&expand_tilde(path), p.passphrase.as_deref())?;
                if self.key(key).await? || (self.partial && self.interactive(p.password.as_deref()).await?) {
                    return Ok(());
                }
                Err(tr!("服务器拒绝了这个私钥", "The server rejected this private key"))
            }
            AuthKind::Auto => {
                if self.agent().await {
                    return Ok(());
                }
                if !self.partial
                    && let Some(home) = home_dir()
                {
                    for name in ["id_ed25519", "id_ecdsa", "id_rsa"] {
                        let path = home.join(".ssh").join(name);
                        // 有密码保护的默认密钥跳过，用户可以在主机设置里单独指定
                        if let Ok(key) = keys::load_secret_key(&path, None)
                            && self.key(key).await?
                        {
                            return Ok(());
                        }
                        if self.partial {
                            break;
                        }
                    }
                }
                // 密钥通过了，还要输验证码
                if self.partial {
                    return if self.interactive(p.password.as_deref()).await? { Ok(()) } else { Err(AUTH_FAILED.into()) };
                }
                match p.password.as_deref() {
                    Some(pw) if self.password(pw).await? => Ok(()),
                    Some(_) => Err(AUTH_FAILED.into()),
                    None => Err(NEED_PASSWORD.into()),
                }
            }
        }
    }

    /// 成功返回 true；服务器说“这一步通过了但还不够”时记下来
    fn check(&mut self, r: AuthResult) -> bool {
        match r {
            AuthResult::Success => true,
            AuthResult::Failure { partial_success, remaining_methods } => {
                if partial_success && remaining_methods.contains(&MethodKind::KeyboardInteractive) {
                    self.partial = true;
                }
                false
            }
        }
    }

    async fn key(&mut self, key: keys::PrivateKey) -> Result<bool> {
        let hash = self.h.best_supported_rsa_hash().await.ok().flatten().flatten();
        let r = self
            .h
            .authenticate_publickey(&self.p.user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
            .await
            .map_err(|e| tr!("登录失败：{e}", "Login failed: {e}"))?;
        Ok(self.check(r))
    }

    async fn password(&mut self, pw: &str) -> Result<bool> {
        let r = self.h.authenticate_password(&self.p.user, pw).await.map_err(|e| tr!("登录失败：{e}", "Login failed: {e}"))?;
        if self.check(r) {
            return Ok(true);
        }
        // 有些服务器关掉了 password 方式，只开了 keyboard-interactive，问的其实还是密码，可能还要验证码
        self.interactive(Some(pw)).await
    }

    /// keyboard-interactive：问密码的地方填密码，其他问题（验证码等）问用户
    async fn interactive(&mut self, pw: Option<&str>) -> Result<bool> {
        let mut resp = self
            .h
            .authenticate_keyboard_interactive_start(&self.p.user, None)
            .await
            .map_err(|e| tr!("登录失败：{e}", "Login failed: {e}"))?;
        let mut pw_sent = false;
        for _ in 0..8 {
            match resp {
                KeyboardInteractiveAuthResponse::Success => return Ok(true),
                KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
                KeyboardInteractiveAuthResponse::InfoRequest { name, instructions, prompts } => {
                    let mut answers = Vec::with_capacity(prompts.len());
                    let mut ask = Vec::new();
                    for (i, q) in prompts.iter().enumerate() {
                        if let Some(pw) = pw
                            && !q.echo
                            && is_password_prompt(&q.prompt)
                        {
                            // 第二次问密码说明密码错了
                            if pw_sent {
                                return Ok(false);
                            }
                            pw_sent = true;
                            answers.push(Some(pw.to_string()));
                        } else {
                            answers.push(None);
                            ask.push(i);
                        }
                    }
                    if !ask.is_empty() {
                        let Some(prompt) = self.prompt.clone() else { return Ok(false) };
                        let info = PromptInfo {
                            host: format!("{}@{}", self.p.user, self.p.host),
                            name,
                            instructions,
                            prompts: ask.iter().map(|&i| PromptItem { prompt: prompts[i].prompt.clone(), echo: prompts[i].echo }).collect(),
                        };
                        let got = prompt(info).await.ok_or_else(|| CANCELLED.to_string())?;
                        for (&i, a) in ask.iter().zip(got) {
                            answers[i] = Some(a);
                        }
                    }
                    let answers = answers.into_iter().map(Option::unwrap_or_default).collect();
                    resp = self
                        .h
                        .authenticate_keyboard_interactive_respond(answers)
                        .await
                        .map_err(|e| tr!("登录失败：{e}", "Login failed: {e}"))?;
                }
            }
        }
        Ok(false)
    }

    /// 用 ssh-agent 里的密钥登录；没有 agent 或都不行时返回 false
    async fn agent(&mut self) -> bool {
        #[cfg(unix)]
        let agent = keys::agent::client::AgentClient::connect_env().await.ok().map(|a| a.dynamic());
        #[cfg(windows)]
        let agent = match keys::agent::client::AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await {
            Ok(a) => Some(a.dynamic()),
            Err(_) => keys::agent::client::AgentClient::connect_pageant().await.ok().map(|a| a.dynamic()),
        };
        let Some(mut agent) = agent else { return false };
        let Ok(ids) = agent.request_identities().await else { return false };
        let hash = self.h.best_supported_rsa_hash().await.ok().flatten().flatten();
        for id in ids {
            let key = id.public_key().into_owned();
            if let Ok(r) = self.h.authenticate_publickey_with(&self.p.user, key, hash, &mut agent).await
                && self.check(r)
            {
                return true;
            }
            if self.partial {
                break;
            }
        }
        false
    }
}

fn is_password_prompt(s: &str) -> bool {
    let s = s.to_lowercase();
    (s.contains("password") || s.contains("密码")) && !s.contains("otp") && !s.contains("one-time")
}

fn load_key(path: &Path, passphrase: Option<&str>) -> Result<keys::PrivateKey> {
    match keys::load_secret_key(path, passphrase.filter(|s| !s.is_empty())) {
        Ok(k) => Ok(k),
        Err(keys::Error::KeyIsEncrypted) => Err(NEED_PASSPHRASE.into()),
        Err(e) if passphrase.is_some() && matches!(e, keys::Error::SshKey(_)) => Err(BAD_PASSPHRASE.into()),
        Err(e) => Err(tr!("读取私钥 {} 失败：{e}", "Couldn't read private key {}: {e}", path.display())),
    }
}

impl Connection {
    /// 识别服务器系统（读 /etc/os-release 的 ID，例如 ubuntu、debian、centos），界面用来显示图标。
    /// 识别不了返回 None，不影响连接
    pub async fn detect_os(&self) -> Option<String> {
        let run = async {
            let mut ch = self.handle.channel_open_session().await.ok()?;
            ch.exec(true, "cat /etc/os-release 2>/dev/null || uname -s").await.ok()?;
            let mut out = Vec::new();
            while let Some(msg) = ch.wait().await {
                match msg {
                    russh::ChannelMsg::Data { data } => out.extend_from_slice(&data),
                    russh::ChannelMsg::Close | russh::ChannelMsg::Eof => break,
                    _ => {}
                }
                if out.len() > 64 * 1024 {
                    break;
                }
            }
            let text = String::from_utf8_lossy(&out);
            let id = text
                .lines()
                .find_map(|l| l.strip_prefix("ID="))
                .map(|v| v.trim().trim_matches('"').to_lowercase())
                .or_else(|| text.lines().next().map(|l| l.trim().to_lowercase()))?;
            (!id.is_empty() && id.len() < 32).then_some(id)
        };
        tokio::time::timeout(Duration::from_secs(3), run).await.ok().flatten()
    }

    pub fn is_closed(&self) -> bool {
        self.handle.is_closed() || self.via.as_ref().is_some_and(|v| v.is_closed())
    }

    /// 断开，经过跳板机时跳板机的连接也一起断开
    pub async fn disconnect(&self) {
        let mut c = Some(self);
        while let Some(conn) = c {
            let _ = conn.handle.disconnect(russh::Disconnect::ByApplication, "", "").await;
            c = conn.via.as_deref();
        }
    }
}
