//! 建立 SSH 连接：校验主机指纹、登录。终端和 SFTP 共用同一条连接。

use crate::{AUTH_FAILED, BAD_PASSPHRASE, NEED_PASSPHRASE, NEED_PASSWORD, Result, tr};
use russh::client::{self, Handle, KeyboardInteractiveAuthResponse};
use russh::keys::{self, HashAlg, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

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

pub struct Client {
    host: String,
    port: u16,
    ask: HostKeyAsk,
    known_hosts: Option<PathBuf>,
}

impl client::Handler for Client {
    type Error = russh::Error;

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
}

pub async fn connect(p: &ConnectParams, ask: HostKeyAsk) -> Result<Connection> {
    connect_with_known_hosts(p, ask, home_dir().map(|h| h.join(".ssh").join("known_hosts"))).await
}

/// 和 [connect] 一样，但指定 known_hosts 文件（测试时不碰用户自己的）
pub async fn connect_with_known_hosts(p: &ConnectParams, ask: HostKeyAsk, known_hosts: Option<PathBuf>) -> Result<Connection> {
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
    let client = Client {
        host: p.host.clone(),
        port: p.port,
        ask,
        known_hosts,
    };
    let port = if p.port == 0 { 22 } else { p.port };
    // 只限制建立 TCP 连接的时间，确认指纹时用户可能要想一会儿
    let stream = tokio::time::timeout(Duration::from_secs(15), tokio::net::TcpStream::connect((p.host.as_str(), port)))
        .await
        .map_err(|_| tr!("连接 {}:{} 超时", "Connection to {}:{} timed out", p.host, port))?
        .map_err(|e| tr!("无法连接 {}:{}：{e}", "Can't connect to {}:{}: {e}", p.host, port))?;
    let _ = stream.set_nodelay(true);
    let mut handle = client::connect_stream(Arc::new(config), stream, client)
        .await
        .map_err(|e| match e {
            russh::Error::UnknownKey => tr!("没有信任服务器的主机指纹，已取消连接", "The host key wasn't trusted, connection cancelled"),
            e => tr!("SSH 握手失败：{e}", "SSH handshake failed: {e}"),
        })?;
    authenticate(&mut handle, p).await?;
    Ok(Connection { handle })
}

async fn authenticate(h: &mut Handle<Client>, p: &ConnectParams) -> Result<()> {
    let user = p.user.as_str();
    match p.auth {
        AuthKind::Password => {
            let Some(pw) = p.password.as_deref() else { return Err(NEED_PASSWORD.into()) };
            if try_password(h, user, pw).await? {
                return Ok(());
            }
            Err(AUTH_FAILED.into())
        }
        AuthKind::Key => {
            let path = p.key_path.as_deref().filter(|s| !s.is_empty()).ok_or_else(|| tr!("没有选择私钥文件", "No private key file selected"))?;
            let key = load_key(&expand_tilde(path), p.passphrase.as_deref())?;
            if try_key(h, user, key).await? {
                return Ok(());
            }
            Err(tr!("服务器拒绝了这个私钥", "The server rejected this private key"))
        }
        AuthKind::Auto => {
            if try_agent(h, user).await {
                return Ok(());
            }
            if let Some(home) = home_dir() {
                for name in ["id_ed25519", "id_ecdsa", "id_rsa"] {
                    let path = home.join(".ssh").join(name);
                    // 有密码保护的默认密钥跳过，用户可以在主机设置里单独指定
                    if let Ok(key) = keys::load_secret_key(&path, None)
                        && try_key(h, user, key).await?
                    {
                        return Ok(());
                    }
                }
            }
            match p.password.as_deref() {
                Some(pw) if try_password(h, user, pw).await? => Ok(()),
                Some(_) => Err(AUTH_FAILED.into()),
                None => Err(NEED_PASSWORD.into()),
            }
        }
    }
}

fn load_key(path: &Path, passphrase: Option<&str>) -> Result<keys::PrivateKey> {
    match keys::load_secret_key(path, passphrase.filter(|s| !s.is_empty())) {
        Ok(k) => Ok(k),
        Err(keys::Error::KeyIsEncrypted) => Err(NEED_PASSPHRASE.into()),
        Err(e) if passphrase.is_some() && matches!(e, keys::Error::SshKey(_)) => Err(BAD_PASSPHRASE.into()),
        Err(e) => Err(tr!("读取私钥 {} 失败：{e}", "Couldn't read private key {}: {e}", path.display())),
    }
}

async fn try_key(h: &mut Handle<Client>, user: &str, key: keys::PrivateKey) -> Result<bool> {
    let hash = h.best_supported_rsa_hash().await.ok().flatten().flatten();
    let r = h
        .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
        .await
        .map_err(|e| tr!("登录失败：{e}", "Login failed: {e}"))?;
    Ok(r.success())
}

async fn try_password(h: &mut Handle<Client>, user: &str, pw: &str) -> Result<bool> {
    let r = h.authenticate_password(user, pw).await.map_err(|e| tr!("登录失败：{e}", "Login failed: {e}"))?;
    if r.success() {
        return Ok(true);
    }
    // 有些服务器关掉了 password 方式，只开了 keyboard-interactive，问的其实还是密码
    let mut resp = h
        .authenticate_keyboard_interactive_start(user, None)
        .await
        .map_err(|e| tr!("登录失败：{e}", "Login failed: {e}"))?;
    for _ in 0..5 {
        match resp {
            KeyboardInteractiveAuthResponse::Success => return Ok(true),
            KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
            KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                let answers = prompts.iter().map(|_| pw.to_string()).collect();
                resp = h
                    .authenticate_keyboard_interactive_respond(answers)
                    .await
                    .map_err(|e| tr!("登录失败：{e}", "Login failed: {e}"))?;
            }
        }
    }
    Ok(false)
}

/// 用 ssh-agent 里的密钥登录；没有 agent 或都不行时返回 false
async fn try_agent(h: &mut Handle<Client>, user: &str) -> bool {
    #[cfg(unix)]
    let agent = keys::agent::client::AgentClient::connect_env().await.ok().map(|a| a.dynamic());
    #[cfg(windows)]
    let agent = match keys::agent::client::AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await {
        Ok(a) => Some(a.dynamic()),
        Err(_) => keys::agent::client::AgentClient::connect_pageant().await.ok().map(|a| a.dynamic()),
    };
    let Some(mut agent) = agent else { return false };
    let Ok(ids) = agent.request_identities().await else { return false };
    let hash = h.best_supported_rsa_hash().await.ok().flatten().flatten();
    for id in ids {
        let key = id.public_key().into_owned();
        if let Ok(r) = h.authenticate_publickey_with(user, key, hash, &mut agent).await
            && r.success()
        {
            return true;
        }
    }
    false
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
        self.handle.is_closed()
    }

    pub async fn disconnect(&self) {
        let _ = self.handle.disconnect(russh::Disconnect::ByApplication, "", "").await;
    }
}
