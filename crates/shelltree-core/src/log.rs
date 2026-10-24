//! 会话日志：把终端输出存成纯文本文件，去掉颜色、光标移动这些控制字符。

use crate::{Result, tr};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

pub struct SessionLog {
    file: File,
    strip: Strip,
    buf: Vec<u8>,
    /// 写失败过（比如磁盘满了），之后不再尝试
    failed: bool,
}

impl SessionLog {
    /// 打开日志文件，`append` 为 false 时清空原有内容。`note` 会先写成单独的一行（比如主机和时间）
    pub fn open(path: &Path, append: bool, note: &str) -> Result<SessionLog> {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(path)
            .map_err(|e| tr!("打开日志文件失败：{e}", "Couldn't open the log file: {e}"))?;
        if !note.is_empty() {
            let sep = if append && file.metadata().map(|m| m.len() > 0).unwrap_or(false) { "\n" } else { "" };
            file.write_all(format!("{sep}{note}\n").as_bytes()).map_err(|e| tr!("写入日志失败：{e}", "Couldn't write the log: {e}"))?;
        }
        Ok(SessionLog { file, strip: Strip::default(), buf: Vec::new(), failed: false })
    }

    pub fn write(&mut self, data: &[u8]) {
        if self.failed {
            return;
        }
        self.buf.clear();
        self.strip.feed(data, &mut self.buf);
        if !self.buf.is_empty() && self.file.write_all(&self.buf).is_err() {
            self.failed = true;
        }
    }
}

/// 去掉终端控制序列。序列可能被拆在两次输出里，所以要记住状态
#[derive(Default, Clone, Copy, PartialEq, Debug)]
enum Strip {
    #[default]
    Text,
    /// 刚收到 ESC
    Esc,
    /// ESC 后面跟中间字符（比如 ESC ( B 选字符集），等最后一个字符
    EscInter,
    /// ESC [ ... 到 0x40..0x7e 结束
    Csi,
    /// ESC ] ... 这类字符串，到 BEL 或 ESC \ 结束
    Str,
    StrEsc,
}

impl Strip {
    fn feed(&mut self, data: &[u8], out: &mut Vec<u8>) {
        for &b in data {
            *self = match (*self, b) {
                (Strip::Text, 0x1b) => Strip::Esc,
                (Strip::Text, b'\n' | b'\t') => {
                    out.push(b);
                    Strip::Text
                }
                // \r、退格、响铃等控制字符不要
                (Strip::Text, 0..=0x1f | 0x7f) => Strip::Text,
                (Strip::Text, _) => {
                    out.push(b);
                    Strip::Text
                }
                (Strip::Esc, b'[') => Strip::Csi,
                (Strip::Esc, b']' | b'P' | b'_' | b'^' | b'X') => Strip::Str,
                (Strip::Esc, 0x20..=0x2f) | (Strip::EscInter, 0x20..=0x2f) => Strip::EscInter,
                (Strip::Esc | Strip::EscInter, _) => Strip::Text,
                (Strip::Csi, 0x40..=0x7e) => Strip::Text,
                (Strip::Csi, _) => Strip::Csi,
                (Strip::Str, 0x07) => Strip::Text,
                (Strip::Str, 0x1b) => Strip::StrEsc,
                (Strip::Str, _) => Strip::Str,
                (Strip::StrEsc, _) => Strip::Text,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(chunks: &[&[u8]]) -> String {
        let mut s = Strip::default();
        let mut out = Vec::new();
        for c in chunks {
            s.feed(c, &mut out);
        }
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn strips_control_sequences() {
        assert_eq!(strip(&[b"\x1b[01;32mroot@host\x1b[0m:~# ls\r\n"]), "root@host:~# ls\n");
        // 序列被拆开
        assert_eq!(strip(&[b"a\x1b[3", b"1mb\x1b", b"[0mc"]), "abc");
        // 窗口标题（OSC），BEL 结束和 ESC \ 结束
        assert_eq!(strip(&[b"\x1b]0;title\x07x\x1b]7;file:///root\x1b\\y"]), "xy");
        assert_eq!(strip(&[b"\x1b(Bok\x1b=\x1b>"]), "ok");
        assert_eq!(strip(&["中文\x07\x08\t1\n".as_bytes()]), "中文\t1\n");
    }

    #[test]
    fn appends() {
        let p = std::env::temp_dir().join(format!("shelltree-log-{}", std::process::id()));
        let mut l = SessionLog::open(&p, false, "# one").unwrap();
        l.write(b"\x1b[1mhi\x1b[0m\r\n");
        drop(l);
        let mut l = SessionLog::open(&p, true, "# two").unwrap();
        l.write(b"bye\r\n");
        drop(l);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "# one\nhi\n\n# two\nbye\n");
        SessionLog::open(&p, false, "").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "");
        let _ = std::fs::remove_file(p);
    }
}
