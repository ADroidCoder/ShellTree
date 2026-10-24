// 后端命令的类型化封装
import { Channel, invoke } from "@tauri-apps/api/core";

export type AuthKind = "auto" | "password" | "key";

export type ForwardKind = "local" | "remote" | "dynamic";

export interface ForwardRule {
  kind: ForwardKind;
  /** 空表示 127.0.0.1 */
  bind_host: string;
  bind_port: number;
  dest_host: string;
  dest_port: number;
}

export interface ForwardInfo {
  id: number;
  rule: ForwardRule;
  port: number;
  active: number;
}

/** 会话日志：写到哪个文件，开头写一行说明，是否接着原文件写 */
export interface LogStart {
  path: string;
  note: string;
  append: boolean;
}

/** 服务器状态的一次采样，CPU 和网络是累计值 */
export interface StatsSample {
  cpu_total: number;
  cpu_idle: number;
  mem_total: number;
  mem_used: number;
  swap_total: number;
  swap_used: number;
  net_rx: number;
  net_tx: number;
  disk_total: number;
  disk_used: number;
  load: number;
}

export interface Host {
  id: string;
  name: string;
  group: string;
  host: string;
  port: number;
  user: string;
  auth: AuthKind;
  key_path: string;
  save_password: boolean;
  last_used: number;
  os: string;
  /** 跳板机：主机 id 或名字，或者 user@host:port，多个用逗号隔开 */
  jump: string;
  forwards: ForwardRule[];
  from_ssh_config?: boolean;
}

export interface ConnectParams {
  host: string;
  port: number;
  user: string;
  auth: AuthKind;
  password?: string;
  key_path?: string;
  passphrase?: string;
}

export interface HostKeyInfo {
  host: string;
  port: number;
  algorithm: string;
  fingerprint: string;
  changed: boolean;
}

export interface HostKeyPrompt {
  id: number;
  info: HostKeyInfo;
}

/** 登录时服务器提的问题，例如验证码 */
export interface AuthPrompt {
  id: number;
  info: {
    host: string;
    name: string;
    instructions: string;
    prompts: { prompt: string; echo: boolean }[];
  };
}

/** 连接经过的一台主机 */
export interface Hop {
  params: ConnectParams;
  host_id: string | null;
  save_secret: boolean;
}

export interface EditEvent {
  id: number;
  session: string;
  remote: string;
  state: "saved" | "failed" | "conflict";
  error: string | null;
}

export interface Entry {
  name: string;
  is_dir: boolean;
  is_link: boolean;
  size: number;
  mtime: number;
  perm: number;
}

export type TransferState = "running" | "done" | "failed" | "cancelled";

export interface TransferInfo {
  id: number;
  session: string;
  /** 服务器之间复制时的源会话 */
  from: string | null;
  upload: boolean;
  name: string;
  dest: string;
  done: number;
  total: number;
  files_done: number;
  files_total: number;
  current: string;
  state: TransferState;
  error: string | null;
  elapsed_ms: number;
}

export interface KnownHost {
  line: number;
  hosts: string;
  hashed: boolean;
  algorithm: string;
  fingerprint: string;
  raw: string;
}

/** 后端用这两个字符串表示需要用户输入密码 / 私钥密码 */
export const NEED_PASSWORD = "need-password";
export const NEED_PASSPHRASE = "need-passphrase";
export const AUTH_FAILED = "auth-failed";
export const BAD_PASSPHRASE = "bad-passphrase";

function openShell(cmd: string, args: Record<string, unknown>, onData: (data: Uint8Array) => void, onExit: (status: number | null) => void) {
  const data = new Channel<ArrayBuffer | number[]>();
  data.onmessage = (m) => onData(m instanceof ArrayBuffer ? new Uint8Array(m) : Uint8Array.from(m));
  const exit = new Channel<number | null>();
  exit.onmessage = onExit;
  return invoke<{ id: number; log_error: string | null }>(cmd, { ...args, onData: data, onExit: exit });
}

export const api = {
  hostsLoad: () => invoke<{ saved: Host[]; groups: string[]; ssh_config: Host[] }>("hosts_load"),
  hostsSave: (hosts: Host[], groups: string[]) => invoke<void>("hosts_save", { hosts, groups }),
  hostSecretSet: (id: string, secret: string | null) => invoke<void>("host_secret_set", { id, secret }),

  /** 依次登录 chain 里的主机，最后一个是目标主机，前面的是跳板机 */
  connect: (session: string, chain: Hop[]) => invoke<string | null>("ssh_connect", { session, chain }),
  hostkeyReply: (id: number, accept: boolean) => invoke<void>("hostkey_reply", { id, accept }),
  authPromptReply: (id: number, answers: string[] | null) => invoke<void>("auth_prompt_reply", { id, answers }),

  forwardStart: (session: string, rule: ForwardRule) => invoke<ForwardInfo>("forward_start", { session, rule }),
  forwardStop: (session: string, id: number) => invoke<void>("forward_stop", { session, id }),
  forwardsList: (session: string) => invoke<ForwardInfo[]>("forwards_list", { session }),
  serverStats: (session: string) => invoke<StatsSample | null>("server_stats", { session }),

  editOpen: (session: string, path: string, editor: string | null) => invoke<void>("edit_open", { session, path, editor }),
  editUpload: (id: number) => invoke<void>("edit_upload", { id }),
  editForget: (session: string) => invoke<void>("edit_forget", { session }),
  disconnect: (session: string) => invoke<void>("ssh_disconnect", { session }),

  shellOpen: (
    session: string,
    cols: number,
    rows: number,
    log: LogStart | null,
    onData: (data: Uint8Array) => void,
    onExit: (status: number | null) => void,
  ) => openShell("shell_open", { session, cols, rows, log }, onData, onExit),
  /** 本机的终端 */
  localShellOpen: (
    cols: number,
    rows: number,
    log: LogStart | null,
    onData: (data: Uint8Array) => void,
    onExit: (status: number | null) => void,
  ) => openShell("local_shell_open", { cols, rows, log }, onData, onExit),
  shellLog: (id: number, log: LogStart | null) => invoke<void>("shell_log", { id, log }),
  shellWrite: (id: number, data: string) => invoke<void>("shell_write", { id, data }),
  shellResize: (id: number, cols: number, rows: number) => invoke<void>("shell_resize", { id, cols, rows }),
  shellAck: (id: number, bytes: number) => invoke<void>("shell_ack", { id, bytes }),
  shellClose: (id: number) => invoke<void>("shell_close", { id }),

  sftpHome: (session: string) => invoke<string>("sftp_home", { session }),
  sftpList: (session: string, path: string) => invoke<Entry[]>("sftp_list", { session, path }),
  sftpMkdir: (session: string, path: string) => invoke<void>("sftp_mkdir", { session, path }),
  sftpRename: (session: string, from: string, to: string) => invoke<void>("sftp_rename", { session, from, to }),
  sftpRemove: (session: string, path: string, isDir: boolean) => invoke<void>("sftp_remove", { session, path, isDir }),
  sftpChmod: (session: string, path: string, perm: number) => invoke<void>("sftp_chmod", { session, path, perm }),

  transferStart: (session: string, upload: boolean, sources: { path: string; is_dir?: boolean }[], dest: string) =>
    invoke<number[]>("transfer_start", { session, upload, sources, dest }),
  transferCopy: (from: string, sources: { path: string; is_dir?: boolean }[], session: string, dest: string) =>
    invoke<number[]>("transfer_copy", { from, sources, session, dest }),
  transfersList: () => invoke<TransferInfo[]>("transfers_list"),
  transferCancel: (id: number) => invoke<void>("transfer_cancel", { id }),
  transfersClear: () => invoke<void>("transfers_clear"),
  downloadDir: () => invoke<string>("download_dir"),

  localHome: () => invoke<string>("local_home"),
  localRoots: () => invoke<string[]>("local_roots"),
  localList: (path: string) => invoke<Entry[]>("local_list", { path }),
  localMkdir: (path: string) => invoke<void>("local_mkdir", { path }),
  localRename: (from: string, to: string) => invoke<void>("local_rename", { from, to }),
  localRemove: (path: string, isDir: boolean) => invoke<void>("local_remove", { path, isDir }),

  dataRead: (name: "history" | "snippets") => invoke<string | null>("data_read", { name }),
  dataWrite: (name: "history" | "snippets", content: string) => invoke<void>("data_write", { name, content }),

  knownHostsList: () => invoke<KnownHost[]>("known_hosts_list"),
  knownHostsRemove: (line: number, raw: string) => invoke<void>("known_hosts_remove", { line, raw }),
};
