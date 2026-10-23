//! 查看和删除 ~/.ssh/known_hosts 里记录的主机指纹

use crate::{Result, tr};
use russh::keys::{self, HashAlg};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub struct KnownHost {
    /// 文件里的行号（从 1 开始），删除时用
    pub line: usize,
    /// 主机名，hashed 时为空
    pub hosts: String,
    pub hashed: bool,
    pub algorithm: String,
    pub fingerprint: String,
    /// 原始行，删除时核对，防止文件在这期间被改过
    pub raw: String,
}

fn path() -> Result<PathBuf> {
    crate::conn::home_dir().map(|h| h.join(".ssh").join("known_hosts")).ok_or_else(|| tr!("找不到用户主目录", "Home directory not found"))
}

pub fn list() -> Result<Vec<KnownHost>> {
    let Ok(text) = std::fs::read_to_string(path()?) else { return Ok(vec![]) };
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let l = raw.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let mut parts = l.split_whitespace();
        let mut hosts = parts.next().unwrap_or("");
        // @cert-authority / @revoked 开头的行
        if hosts.starts_with('@') {
            hosts = parts.next().unwrap_or("");
        }
        let (Some(alg), Some(b64)) = (parts.next(), parts.next()) else { continue };
        let fingerprint = keys::parse_public_key_base64(b64)
            .map(|k| k.fingerprint(HashAlg::Sha256).to_string())
            .unwrap_or_default();
        let hashed = hosts.starts_with("|1|");
        out.push(KnownHost {
            line: i + 1,
            hosts: if hashed { String::new() } else { hosts.to_string() },
            hashed,
            algorithm: alg.to_string(),
            fingerprint,
            raw: raw.to_string(),
        });
    }
    Ok(out)
}

pub fn remove(line: usize, raw: &str) -> Result<()> {
    let p = path()?;
    let text = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    let mut out = String::with_capacity(text.len());
    let mut found = false;
    for (i, l) in text.split_inclusive('\n').enumerate() {
        if i + 1 == line && l.trim_end_matches(['\r', '\n']) == raw {
            found = true;
            continue;
        }
        out.push_str(l);
    }
    if !found {
        return Err(tr!("known_hosts 已经被修改过，请刷新后再试", "known_hosts has changed, please refresh and try again"));
    }
    std::fs::write(&p, out).map_err(|e| tr!("写入 known_hosts 失败：{e}", "Couldn't write known_hosts: {e}"))
}
