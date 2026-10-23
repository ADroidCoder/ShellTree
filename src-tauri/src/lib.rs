mod edit;

use serde::{Deserialize, Serialize};
use shelltree_core::conn::{self, AuthKind, ConnectParams, Connection, HostKeyInfo, PromptInfo};
use shelltree_core::forward::{self, Forward, ForwardRule};
use shelltree_core::hosts::{self, Host, HostStore};
use shelltree_core::{AUTH_FAILED, BAD_PASSPHRASE, NEED_PASSPHRASE, NEED_PASSWORD, known_hosts, local, tr};
use shelltree_core::russh_sftp::client::SftpSession;
use shelltree_core::sftp::{self, Entry, Progress};
use shelltree_core::shell::Shell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::oneshot;

type CmdResult<T> = Result<T, String>;

/// 一个已登录的会话（界面上的一个标签页）
struct Session {
    conn: Arc<Connection>,
    /// SFTP 第一次用到时才打开
    sftp: tokio::sync::Mutex<Option<Arc<SftpSession>>>,
    shells: Mutex<Vec<u64>>,
    forwards: Mutex<Vec<RunningForward>>,
}

struct RunningForward {
    id: u64,
    rule: ForwardRule,
    forward: Forward,
}

#[derive(Serialize)]
struct ForwardInfo {
    id: u64,
    rule: ForwardRule,
    /// 实际监听的端口
    port: u16,
    /// 正在通过这个转发的连接数
    active: usize,
}

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum TransferState {
    Running,
    Done,
    Failed,
    Cancelled,
}

struct Transfer {
    id: u64,
    session: String,
    upload: bool,
    name: String,
    /// 上传时是远程文件夹，下载时是本地文件夹
    dest: String,
    progress: Arc<Progress>,
    state: TransferState,
    error: Option<String>,
    started: Instant,
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct TransferInfo {
    id: u64,
    session: String,
    upload: bool,
    name: String,
    dest: String,
    done: u64,
    total: u64,
    files_done: u64,
    files_total: u64,
    current: String,
    state: TransferState,
    error: Option<String>,
    elapsed_ms: u64,
}

/// 界面自己管理内容的数据文件（命令历史、命令片段），后端只负责读写
const DATA_FILES: &[&str] = &["history", "snippets"];

fn data_path(app: &AppHandle, name: &str) -> CmdResult<PathBuf> {
    if !DATA_FILES.contains(&name) {
        return Err(format!("unknown data file {name}"));
    }
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(dir.join(format!("{name}.json")))
}

#[tauri::command]
async fn data_read(app: AppHandle, name: String) -> CmdResult<Option<String>> {
    let p = data_path(&app, &name)?;
    Ok(tokio::fs::read_to_string(p).await.ok())
}

#[tauri::command]
async fn data_write(app: AppHandle, name: String, content: String) -> CmdResult<()> {
    let p = data_path(&app, &name)?;
    if let Some(dir) = p.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(|e| e.to_string())?;
    }
    // 先写临时文件再改名，写到一半退出也不会把文件弄坏
    let tmp = p.with_extension("json.tmp");
    tokio::fs::write(&tmp, content).await.map_err(|e| e.to_string())?;
    tokio::fs::rename(&tmp, &p).await.map_err(|e| e.to_string())
}

struct AppState {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    shells: Mutex<HashMap<u64, Arc<Shell>>>,
    /// 等界面回答“是否信任主机指纹”
    hostkey_waiters: Mutex<HashMap<u64, oneshot::Sender<bool>>>,
    /// 等界面回答登录时服务器提的问题（验证码等）
    prompt_waiters: Mutex<HashMap<u64, oneshot::Sender<Option<Vec<String>>>>>,
    edits: Mutex<Vec<edit::EditFile>>,
    transfers: Mutex<Vec<Transfer>>,
    next_id: AtomicU64,
    hosts: HostStore,
}

impl AppState {
    fn id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn session(&self, id: &str) -> CmdResult<Arc<Session>> {
        self.sessions.lock().unwrap().get(id).cloned().ok_or_else(|| tr!("连接已断开", "Disconnected"))
    }

    fn shell(&self, id: u64) -> CmdResult<Arc<Shell>> {
        self.shells.lock().unwrap().get(&id).cloned().ok_or_else(|| tr!("终端已关闭", "The terminal is closed"))
    }

    async fn sftp(&self, session: &str) -> CmdResult<Arc<SftpSession>> {
        let s = self.session(session)?;
        let mut slot = s.sftp.lock().await;
        if let Some(sftp) = slot.as_ref() {
            return Ok(sftp.clone());
        }
        let sftp = sftp::open(&s.conn).await?;
        *slot = Some(sftp.clone());
        Ok(sftp)
    }
}

#[derive(Serialize)]
struct HostList {
    saved: Vec<Host>,
    groups: Vec<String>,
    /// ~/.ssh/config 里的主机
    ssh_config: Vec<Host>,
}

#[tauri::command]
fn hosts_load(state: State<'_, AppState>) -> HostList {
    let f = state.hosts.load();
    HostList { saved: f.hosts, groups: f.groups, ssh_config: hosts::read_ssh_config() }
}

#[tauri::command]
fn hosts_save(state: State<'_, AppState>, hosts: Vec<Host>, groups: Vec<String>) -> CmdResult<()> {
    state.hosts.save(&hosts, &groups)
}

/// 保存或删除（secret 为空）钥匙串里的密码
#[tauri::command]
fn host_secret_set(id: String, secret: Option<String>) -> CmdResult<()> {
    match secret {
        Some(s) => hosts::set_secret(&id, &s),
        None => {
            hosts::delete_secret(&id);
            Ok(())
        }
    }
}

#[derive(Serialize, Clone)]
struct HostKeyPrompt {
    id: u64,
    info: HostKeyInfo,
}

#[derive(Serialize, Clone)]
struct AuthPrompt {
    id: u64,
    info: PromptInfo,
}

/// 连接经过的一台主机（跳板机或目标主机）
#[derive(Deserialize)]
struct Hop {
    params: ConnectParams,
    host_id: Option<String>,
    #[serde(default)]
    save_secret: bool,
}

/// 依次登录 chain 里的主机：前面的是跳板机，最后一个是目标主机。返回识别到的系统。
/// 跳板机需要密码时返回 `jump:序号:need-password` 这样的错误，界面据此问是哪台主机的密码
#[tauri::command]
async fn ssh_connect(app: AppHandle, state: State<'_, AppState>, session: String, mut chain: Vec<Hop>) -> CmdResult<Option<String>> {
    // 没有输入密码时用钥匙串里保存的
    for hop in &mut chain {
        let Some(id) = hop.host_id.as_deref() else { continue };
        let p = &mut hop.params;
        if p.auth == AuthKind::Key {
            if p.passphrase.is_none() {
                p.passphrase = hosts::get_secret(id);
            }
        } else if p.password.is_none() {
            p.password = hosts::get_secret(id);
        }
    }
    let handle = app.clone();
    let host_key: conn::HostKeyAsk = Arc::new(move |info| {
        let app = handle.clone();
        Box::pin(async move {
            let state = app.state::<AppState>();
            let id = state.id();
            let (tx, rx) = oneshot::channel();
            state.hostkey_waiters.lock().unwrap().insert(id, tx);
            if app.emit("hostkey", HostKeyPrompt { id, info }).is_err() {
                return false;
            }
            rx.await.unwrap_or(false)
        })
    });
    let handle = app.clone();
    let prompt: conn::PromptAsk = Arc::new(move |info| {
        let app = handle.clone();
        Box::pin(async move {
            let state = app.state::<AppState>();
            let id = state.id();
            let (tx, rx) = oneshot::channel();
            state.prompt_waiters.lock().unwrap().insert(id, tx);
            if app.emit("auth-prompt", AuthPrompt { id, info }).is_err() {
                return None;
            }
            rx.await.ok().flatten()
        })
    });
    let params: Vec<ConnectParams> = chain.iter().map(|h| h.params.clone()).collect();
    let c = conn::connect(&params, &conn::Callbacks { host_key, prompt: Some(prompt) }).await.map_err(|e| {
        if e.hop + 1 >= chain.len() {
            return e.message;
        }
        let codes = [NEED_PASSWORD, NEED_PASSPHRASE, AUTH_FAILED, BAD_PASSPHRASE];
        if codes.contains(&e.message.as_str()) {
            format!("jump:{}:{}", e.hop, e.message)
        } else if e.message == shelltree_core::CANCELLED {
            e.message
        } else {
            tr!("跳板机 {}：{}", "Jump host {}: {}", chain[e.hop].params.host, e.message)
        }
    })?;
    for hop in &chain {
        if !hop.save_secret {
            continue;
        }
        let Some(id) = hop.host_id.as_deref() else { continue };
        let p = &hop.params;
        let secret = if p.auth == AuthKind::Key { &p.passphrase } else { &p.password };
        if let Some(s) = secret {
            hosts::set_secret(id, s)?;
        }
    }
    let os = c.detect_os().await;
    let s = Arc::new(Session { conn: Arc::new(c), sftp: Default::default(), shells: Default::default(), forwards: Default::default() });
    let old = state.sessions.lock().unwrap().insert(session, s);
    if let Some(old) = old {
        // 重连时替换掉旧的连接
        close_session(&state, old).await;
    }
    Ok(os)
}

#[tauri::command]
fn auth_prompt_reply(state: State<'_, AppState>, id: u64, answers: Option<Vec<String>>) {
    if let Some(tx) = state.prompt_waiters.lock().unwrap().remove(&id) {
        let _ = tx.send(answers);
    }
}

#[tauri::command]
fn hostkey_reply(state: State<'_, AppState>, id: u64, accept: bool) {
    if let Some(tx) = state.hostkey_waiters.lock().unwrap().remove(&id) {
        let _ = tx.send(accept);
    }
}

#[tauri::command]
async fn ssh_disconnect(state: State<'_, AppState>, session: String) -> CmdResult<()> {
    let s = state.sessions.lock().unwrap().remove(&session);
    if let Some(s) = s {
        for t in state.transfers.lock().unwrap().iter() {
            if t.session == session {
                t.progress.cancelled.store(true, Ordering::Relaxed);
            }
        }
        close_session(&state, s).await;
    }
    Ok(())
}

/// 关掉一个会话的终端和端口转发，断开连接
async fn close_session(state: &AppState, s: Arc<Session>) {
    for id in s.shells.lock().unwrap().drain(..) {
        state.shells.lock().unwrap().remove(&id);
    }
    // 端口转发里的任务拿着连接，要先停掉；等端口释放后再返回，重连时才能重新监听同一个端口
    let forwards: Vec<_> = s.forwards.lock().unwrap().drain(..).collect();
    for f in forwards {
        f.forward.stop().await;
    }
    s.conn.disconnect().await;
}

/// 开启一个端口转发
#[tauri::command]
async fn forward_start(state: State<'_, AppState>, session: String, rule: ForwardRule) -> CmdResult<ForwardInfo> {
    let s = state.session(&session)?;
    let forward = forward::start(s.conn.clone(), &rule).await?;
    let id = state.id();
    let info = ForwardInfo { id, rule: rule.clone(), port: forward.port, active: 0 };
    s.forwards.lock().unwrap().push(RunningForward { id, rule, forward });
    Ok(info)
}

#[tauri::command]
async fn forward_stop(state: State<'_, AppState>, session: String, id: u64) -> CmdResult<()> {
    let s = state.session(&session)?;
    let f = {
        let mut list = s.forwards.lock().unwrap();
        list.iter().position(|f| f.id == id).map(|i| list.remove(i))
    };
    if let Some(f) = f {
        f.forward.stop().await;
    }
    Ok(())
}

#[tauri::command]
fn forwards_list(state: State<'_, AppState>, session: String) -> Vec<ForwardInfo> {
    let Ok(s) = state.session(&session) else { return vec![] };
    s.forwards
        .lock()
        .unwrap()
        .iter()
        .map(|f| ForwardInfo { id: f.id, rule: f.rule.clone(), port: f.forward.port, active: f.forward.active() })
        .collect()
}

#[tauri::command]
async fn shell_open(
    state: State<'_, AppState>,
    session: String,
    cols: u32,
    rows: u32,
    on_data: Channel<InvokeResponseBody>,
    on_exit: Channel<Option<u32>>,
) -> CmdResult<u64> {
    let s = state.session(&session)?;
    // 输出直接以二进制发给界面，不转成 JSON
    let sh = Shell::open(
        &s.conn,
        cols,
        rows,
        move |data| {
            let _ = on_data.send(InvokeResponseBody::Raw(data));
        },
        move |status| {
            let _ = on_exit.send(status);
        },
    )
    .await?;
    let id = state.id();
    state.shells.lock().unwrap().insert(id, Arc::new(sh));
    s.shells.lock().unwrap().push(id);
    Ok(id)
}

#[tauri::command]
async fn shell_write(state: State<'_, AppState>, id: u64, data: String) -> CmdResult<()> {
    state.shell(id)?.write(data.as_bytes()).await
}

#[tauri::command]
async fn shell_resize(state: State<'_, AppState>, id: u64, cols: u32, rows: u32) -> CmdResult<()> {
    state.shell(id)?.resize(cols, rows).await
}

#[tauri::command]
fn shell_ack(state: State<'_, AppState>, id: u64, bytes: usize) {
    if let Ok(sh) = state.shell(id) {
        sh.ack(bytes);
    }
}

#[tauri::command]
async fn shell_close(state: State<'_, AppState>, id: u64) -> CmdResult<()> {
    let sh = state.shells.lock().unwrap().remove(&id);
    if let Some(sh) = sh {
        sh.close().await;
    }
    Ok(())
}

#[tauri::command]
async fn sftp_home(state: State<'_, AppState>, session: String) -> CmdResult<String> {
    sftp::home(&*state.sftp(&session).await?).await
}

#[tauri::command]
async fn sftp_list(state: State<'_, AppState>, session: String, path: String) -> CmdResult<Vec<Entry>> {
    sftp::list(&*state.sftp(&session).await?, &path).await
}

#[tauri::command]
async fn sftp_mkdir(state: State<'_, AppState>, session: String, path: String) -> CmdResult<()> {
    sftp::mkdir(&*state.sftp(&session).await?, &path).await
}

#[tauri::command]
async fn sftp_rename(state: State<'_, AppState>, session: String, from: String, to: String) -> CmdResult<()> {
    sftp::rename(&*state.sftp(&session).await?, &from, &to).await
}

#[tauri::command]
async fn sftp_remove(state: State<'_, AppState>, session: String, path: String, is_dir: bool) -> CmdResult<()> {
    sftp::remove(&*state.sftp(&session).await?, &path, is_dir).await
}

#[tauri::command]
async fn sftp_chmod(state: State<'_, AppState>, session: String, path: String, perm: u32) -> CmdResult<()> {
    sftp::chmod(&*state.sftp(&session).await?, &path, perm).await
}

#[derive(Deserialize)]
struct Source {
    path: String,
    #[serde(default)]
    is_dir: bool,
}

/// 开始上传（本地路径 → 远程文件夹）或下载（远程路径 → 本地文件夹），每个来源一个任务
#[tauri::command]
async fn transfer_start(
    app: AppHandle,
    state: State<'_, AppState>,
    session: String,
    upload: bool,
    sources: Vec<Source>,
    dest: String,
) -> CmdResult<Vec<u64>> {
    let sftp = state.sftp(&session).await?;
    let mut ids = Vec::new();
    for src in sources {
        let id = state.id();
        let progress = Arc::new(Progress::default());
        let name = src.path.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or("").to_string();
        state.transfers.lock().unwrap().push(Transfer {
            id,
            session: session.clone(),
            upload,
            name,
            dest: dest.clone(),
            progress: progress.clone(),
            state: TransferState::Running,
            error: None,
            started: Instant::now(),
            elapsed_ms: 0,
        });
        let (sftp, dest, app) = (sftp.clone(), dest.clone(), app.clone());
        tauri::async_runtime::spawn(async move {
            let r = if upload {
                sftp::upload(sftp, PathBuf::from(src.path), dest, progress.clone()).await
            } else {
                sftp::download(sftp, src.path, src.is_dir, PathBuf::from(dest), progress.clone()).await
            };
            let state = app.state::<AppState>();
            let mut list = state.transfers.lock().unwrap();
            if let Some(t) = list.iter_mut().find(|t| t.id == id) {
                t.elapsed_ms = t.started.elapsed().as_millis() as u64;
                t.state = match &r {
                    Ok(()) => TransferState::Done,
                    Err(e) if e == sftp::CANCELLED => TransferState::Cancelled,
                    Err(_) => TransferState::Failed,
                };
                t.error = r.err().filter(|e| e != sftp::CANCELLED);
            }
            drop(list);
            let _ = app.emit("transfer-finished", id);
        });
        ids.push(id);
    }
    Ok(ids)
}

#[tauri::command]
fn transfers_list(state: State<'_, AppState>) -> Vec<TransferInfo> {
    state
        .transfers
        .lock()
        .unwrap()
        .iter()
        .map(|t| {
            let p = &t.progress;
            TransferInfo {
                id: t.id,
                session: t.session.clone(),
                upload: t.upload,
                name: t.name.clone(),
                dest: t.dest.clone(),
                done: p.done.load(Ordering::Relaxed),
                total: p.total.load(Ordering::Relaxed),
                files_done: p.files_done.load(Ordering::Relaxed),
                files_total: p.files_total.load(Ordering::Relaxed),
                current: p.current.lock().map(|c| c.clone()).unwrap_or_default(),
                state: t.state,
                error: t.error.clone(),
                elapsed_ms: if t.state == TransferState::Running {
                    t.started.elapsed().as_millis() as u64
                } else {
                    t.elapsed_ms
                },
            }
        })
        .collect()
}

#[tauri::command]
fn transfer_cancel(state: State<'_, AppState>, id: u64) {
    if let Some(t) = state.transfers.lock().unwrap().iter().find(|t| t.id == id) {
        t.progress.cancelled.store(true, Ordering::Relaxed);
    }
}

/// 清掉已经结束的传输记录
#[tauri::command]
fn transfers_clear(state: State<'_, AppState>) {
    state.transfers.lock().unwrap().retain(|t| t.state == TransferState::Running);
}

#[tauri::command]
fn set_lang(en: bool) {
    shelltree_core::set_english(en);
}

#[tauri::command]
fn local_home() -> String {
    local::home()
}

#[tauri::command]
fn local_roots() -> Vec<String> {
    local::roots()
}

#[tauri::command]
async fn local_list(path: String) -> CmdResult<Vec<Entry>> {
    tauri::async_runtime::spawn_blocking(move || local::list(&path)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn local_mkdir(path: String) -> CmdResult<()> {
    local::mkdir(&path)
}

#[tauri::command]
fn local_rename(from: String, to: String) -> CmdResult<()> {
    local::rename(&from, &to)
}

#[tauri::command]
async fn local_remove(path: String, is_dir: bool) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || local::remove(&path, is_dir)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn known_hosts_list() -> CmdResult<Vec<known_hosts::KnownHost>> {
    known_hosts::list()
}

#[tauri::command]
fn known_hosts_remove(line: usize, raw: String) -> CmdResult<()> {
    known_hosts::remove(line, &raw)
}

#[tauri::command]
fn download_dir(app: AppHandle) -> String {
    app.path()
        .download_dir()
        .or_else(|_| app.path().home_dir())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            app.manage(AppState {
                sessions: Default::default(),
                shells: Default::default(),
                hostkey_waiters: Default::default(),
                prompt_waiters: Default::default(),
                edits: Default::default(),
                transfers: Default::default(),
                next_id: AtomicU64::new(0),
                hosts: HostStore::new(&dir),
            });
            edit::start_watcher(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            hosts_load,
            hosts_save,
            host_secret_set,
            ssh_connect,
            hostkey_reply,
            auth_prompt_reply,
            ssh_disconnect,
            forward_start,
            forward_stop,
            forwards_list,
            edit::edit_open,
            edit::edit_upload,
            edit::edit_forget,
            shell_open,
            shell_write,
            shell_resize,
            shell_ack,
            shell_close,
            sftp_home,
            sftp_list,
            sftp_mkdir,
            sftp_rename,
            sftp_remove,
            sftp_chmod,
            transfer_start,
            transfers_list,
            transfer_cancel,
            transfers_clear,
            download_dir,
            set_lang,
            data_read,
            data_write,
            local_home,
            local_roots,
            local_list,
            local_mkdir,
            local_rename,
            local_remove,
            known_hosts_list,
            known_hosts_remove,
        ])
        .run(tauri::generate_context!())
        .expect("启动失败");
}
