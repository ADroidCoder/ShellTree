//! 本地文件操作，给 SFTP 双栏界面的“本地”一栏用。列表格式和远程的一样。

use crate::{Result, tr};
use crate::sftp::Entry;
use std::path::Path;
use std::time::UNIX_EPOCH;

pub fn home() -> String {
    crate::conn::home_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| "/".into())
}

pub fn list(dir: &str) -> Result<Vec<Entry>> {
    let rd = std::fs::read_dir(dir).map_err(|e| tr!("打开 {dir} 失败：{e}", "Couldn't open {dir}: {e}"))?;
    let mut out = Vec::new();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(lmeta) = e.metadata() else { continue };
        let is_link = e.file_type().map(|t| t.is_symlink()).unwrap_or(false);
        // 链接看它指向的东西
        let meta = if is_link { std::fs::metadata(e.path()).unwrap_or(lmeta) } else { lmeta };
        let mtime = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs() as u32);
        #[cfg(unix)]
        let perm = {
            use std::os::unix::fs::PermissionsExt;
            meta.permissions().mode() & 0o7777
        };
        #[cfg(not(unix))]
        let perm = if meta.permissions().readonly() { 0o444 } else { 0o644 };
        out.push(Entry { name, is_dir: meta.is_dir(), is_link, size: if meta.is_dir() { 0 } else { meta.len() }, mtime, perm });
    }
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

pub fn mkdir(path: &str) -> Result<()> {
    std::fs::create_dir(path).map_err(|e| tr!("创建文件夹失败：{e}", "Couldn't create folder: {e}"))
}

pub fn rename(from: &str, to: &str) -> Result<()> {
    if Path::new(to).exists() {
        return Err(tr!("已经有同名的文件或文件夹", "A file or folder with that name already exists"));
    }
    std::fs::rename(from, to).map_err(|e| tr!("重命名失败：{e}", "Rename failed: {e}"))
}

pub fn remove(path: &str, is_dir: bool) -> Result<()> {
    let r = if is_dir { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
    r.map_err(|e| tr!("删除失败：{e}", "Delete failed: {e}"))
}

/// Windows 上列出所有盘符，作为“根目录”
pub fn roots() -> Vec<String> {
    #[cfg(windows)]
    {
        (b'A'..=b'Z')
            .map(|c| format!("{}:\\", c as char))
            .filter(|d| Path::new(d).exists())
            .collect()
    }
    #[cfg(not(windows))]
    {
        vec!["/".into()]
    }
}
