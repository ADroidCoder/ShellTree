//! 保存的主机列表。主机信息存在配置目录的 hosts.json，密码存在系统钥匙串里。

use crate::{Result, tr};
use crate::conn::AuthKind;
use crate::forward::{ForwardKind, ForwardRule};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Host {
    pub id: String,
    /// 显示的名字，空的时候显示 user@host
    pub name: String,
    /// 分组，空表示不分组
    pub group: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: AuthKind,
    pub key_path: String,
    /// 钥匙串里存了密码（或私钥密码）
    pub save_password: bool,
    /// 上次连接时间（秒），用来排序“最近使用”
    pub last_used: u64,
    /// 连接时识别到的系统，例如 ubuntu，用来显示图标
    pub os: String,
    /// 跳板机：另一台主机的 id 或名字，或者 user@host:port；多个用逗号隔开，按顺序经过（和 ProxyJump 一样）
    pub jump: String,
    /// 连接后自动开启的端口转发
    pub forwards: Vec<ForwardRule>,
    /// 从 ~/.ssh/config 读来的，不保存到 hosts.json
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub from_ssh_config: bool,
}

#[derive(Default, Serialize, Deserialize)]
pub struct HostFile {
    pub hosts: Vec<Host>,
    /// 分组列表。有主机的分组从主机上就能看出来，这里主要是为了保存空分组和顺序
    #[serde(default)]
    pub groups: Vec<String>,
}

pub struct HostStore {
    path: PathBuf,
}

impl HostStore {
    pub fn new(config_dir: &Path) -> Self {
        Self { path: config_dir.join("hosts.json") }
    }

    pub fn load(&self) -> HostFile {
        std::fs::read(&self.path)
            .ok()
            .and_then(|b| serde_json::from_slice::<HostFile>(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, hosts: &[Host], groups: &[String]) -> Result<()> {
        let hosts: Vec<Host> = hosts.iter().filter(|h| !h.from_ssh_config).cloned().collect();
        let groups = groups.to_vec();
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_vec_pretty(&HostFile { hosts, groups }).map_err(|e| e.to_string())?;
        // 先写临时文件再改名，写到一半崩溃也不会把主机列表弄坏
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| tr!("保存主机列表失败：{e}", "Couldn't save hosts: {e}"))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| tr!("保存主机列表失败：{e}", "Couldn't save hosts: {e}"))
    }
}

const KEYRING_SERVICE: &str = "ShellTree";

pub fn get_secret(id: &str) -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, id).ok()?.get_password().ok()
}

pub fn set_secret(id: &str, secret: &str) -> Result<()> {
    keyring::Entry::new(KEYRING_SERVICE, id)
        .and_then(|e| e.set_password(secret))
        .map_err(|e| tr!("保存密码到系统钥匙串失败：{e}", "Couldn't save the password to the system keychain: {e}"))
}

pub fn delete_secret(id: &str) {
    if let Ok(e) = keyring::Entry::new(KEYRING_SERVICE, id) {
        let _ = e.delete_credential();
    }
}

/// 读取 ~/.ssh/config 里明确写出的主机（带通配符的规则跳过）
pub fn read_ssh_config() -> Vec<Host> {
    let Some(home) = crate::conn::home_dir() else { return vec![] };
    let Ok(text) = std::fs::read_to_string(home.join(".ssh").join("config")) else { return vec![] };
    parse_ssh_config(&text)
}

pub fn parse_ssh_config(text: &str) -> Vec<Host> {
    let mut out: Vec<Host> = Vec::new();
    // 当前 Host 段落对应的主机（一行 Host 可以写多个名字）
    let mut current: Vec<usize> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = match line.split_once(|c: char| c.is_whitespace() || c == '=') {
            Some((k, v)) => (k.to_ascii_lowercase(), v.trim_start_matches(|c: char| c.is_whitespace() || c == '=').trim()),
            None => continue,
        };
        let value = value.trim_matches('"');
        match key.as_str() {
            "host" => {
                current.clear();
                for name in value.split_whitespace() {
                    if name.contains(['*', '?', '!']) {
                        continue;
                    }
                    current.push(out.len());
                    out.push(Host {
                        id: format!("ssh-config:{name}"),
                        name: name.to_string(),
                        group: "~/.ssh/config".into(),
                        host: name.to_string(),
                        port: 22,
                        from_ssh_config: true,
                        ..Default::default()
                    });
                }
            }
            "match" => current.clear(),
            _ => {
                for &i in &current {
                    let h = &mut out[i];
                    match key.as_str() {
                        "hostname" => h.host = value.to_string(),
                        "user" => h.user = value.to_string(),
                        "port" => h.port = value.parse().unwrap_or(22),
                        // 只取第一个 IdentityFile，和 ssh 的优先级一致
                        "identityfile" if h.key_path.is_empty() => {
                            h.key_path = value.to_string();
                            h.auth = AuthKind::Key;
                        }
                        "proxyjump" if h.jump.is_empty() && !value.eq_ignore_ascii_case("none") => h.jump = value.to_string(),
                        "localforward" | "remoteforward" | "dynamicforward" => {
                            if let Some(f) = parse_forward(&key, value) {
                                h.forwards.push(f);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    let user = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default();
    for h in &mut out {
        if h.user.is_empty() {
            h.user = user.clone();
        }
    }
    out
}

/// 解析 LocalForward / RemoteForward 的 `[地址:]端口 目标:端口`，DynamicForward 的 `[地址:]端口`
fn parse_forward(key: &str, value: &str) -> Option<ForwardRule> {
    let mut parts = value.split_whitespace();
    let (bind_host, bind_port) = split_host_port(parts.next()?)?;
    let kind = match key {
        "localforward" => ForwardKind::Local,
        "remoteforward" => ForwardKind::Remote,
        _ => return Some(ForwardRule { kind: ForwardKind::Dynamic, bind_host, bind_port, ..Default::default() }),
    };
    let (dest_host, dest_port) = split_host_port(parts.next()?)?;
    Some(ForwardRule { kind, bind_host, bind_port, dest_host, dest_port })
}

/// `host:port`、`[v6]:port` 或只有 `port`
fn split_host_port(s: &str) -> Option<(String, u16)> {
    let (host, port) = match s.rsplit_once(':') {
        Some((h, p)) => (h.trim_matches(['[', ']']).to_string(), p),
        None => (String::new(), s),
    };
    let host = if host == "*" { "0.0.0.0".to_string() } else { host };
    Some((host, port.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssh_config() {
        let hosts = parse_ssh_config(
            "Host *\n  ServerAliveInterval 30\n\nHost web db\n  HostName 10.0.0.5\n  User root\n  Port 2222\n  IdentityFile ~/.ssh/work\n\nHost=git\n HostName=github.com\n",
        );
        assert_eq!(hosts.len(), 3);
        assert_eq!(hosts[0].name, "web");
        assert_eq!(hosts[1].host, "10.0.0.5");
        assert_eq!(hosts[1].port, 2222);
        assert_eq!(hosts[1].user, "root");
        assert_eq!(hosts[1].auth, AuthKind::Key);
        assert_eq!(hosts[2].host, "github.com");
        assert_eq!(hosts[2].auth, AuthKind::Auto);
    }

    #[test]
    fn parses_jump_and_forwards() {
        let hosts = parse_ssh_config(
            "Host inner\n  HostName 10.0.0.9\n  ProxyJump bastion,ops@1.2.3.4:2222\n  LocalForward 3307 db:3306\n  RemoteForward *:8080 localhost:3000\n  DynamicForward 1080\n",
        );
        let h = &hosts[0];
        assert_eq!(h.jump, "bastion,ops@1.2.3.4:2222");
        assert_eq!(h.forwards.len(), 3);
        assert_eq!(h.forwards[0], ForwardRule { kind: ForwardKind::Local, bind_host: "".into(), bind_port: 3307, dest_host: "db".into(), dest_port: 3306 });
        assert_eq!(h.forwards[1].bind_host, "0.0.0.0");
        assert_eq!(h.forwards[1].dest_host, "localhost");
        assert_eq!(h.forwards[2].kind, ForwardKind::Dynamic);
        assert_eq!(h.forwards[2].bind_port, 1080);
    }
}
