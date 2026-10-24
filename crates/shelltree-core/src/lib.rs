//! SSH / SFTP 的核心逻辑，不依赖界面，方便单独测试。

pub mod conn;
pub mod forward;
pub mod hosts;
pub mod known_hosts;
pub mod local;
pub mod log;
pub mod sftp;
pub mod shell;
pub mod stats;

use std::sync::atomic::{AtomicBool, Ordering};

/// 错误直接用给用户看的文字
pub type Result<T> = std::result::Result<T, String>;

static EN: AtomicBool = AtomicBool::new(false);

/// 界面切换语言时同步过来，后端的提示信息跟着变
pub fn set_english(en: bool) {
    EN.store(en, Ordering::Relaxed);
}

pub fn is_english() -> bool {
    EN.load(Ordering::Relaxed)
}

/// 中英文两种写法，按当前语言选一个：`tr!("打开 {p} 失败", "Couldn't open {p}")`。
/// 两种写法都会被编译检查，参数可以直接写在花括号里，也可以跟在后面
#[macro_export]
macro_rules! tr {
    ($zh:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::is_english() { format!($en $(, $arg)*) } else { format!($zh $(, $arg)*) }
    };
}

/// 没有保存密码，需要界面问用户（界面按这个字符串识别）
pub const NEED_PASSWORD: &str = "need-password";
/// 私钥有密码保护，需要界面问用户
pub const NEED_PASSPHRASE: &str = "need-passphrase";
/// 密码错误（界面据此重新询问）
pub const AUTH_FAILED: &str = "auth-failed";
/// 私钥密码错误
pub const BAD_PASSPHRASE: &str = "bad-passphrase";
/// 用户取消了（例如没有输入验证码）
pub const CANCELLED: &str = "cancelled";

pub use russh;
pub use russh_sftp;
