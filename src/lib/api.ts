// 后端命令的类型化封装
import { Channel, invoke } from "@tauri-apps/api/core";

export type AuthKind = "auto" | "password" | "key";

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

export const api = {
  hostsLoad: () => invoke<{ saved: Host[]; groups: string[]; ssh_config: Host[] }>("hosts_load"),
  hostsSave: (hosts: Host[], groups: string[]) => invoke<void>("hosts_save", { hosts, groups }),
  hostSecretSet: (id: string, secret: string | null) => invoke<void>("host_secret_set", { id, secret }),

  connect: (session: string, params: ConnectParams, hostId: string | null, saveSecret: boolean) =>
    invoke<string | null>("ssh_connect", { session, params, hostId, saveSecret }),
  hostkeyReply: (id: number, accept: boolean) => invoke<void>("hostkey_reply", { id, accept }),
  disconnect: (session: string) => invoke<void>("ssh_disconnect", { session }),

  shellOpen: (
    session: string,
    cols: number,
    rows: number,
    onData: (data: Uint8Array) => void,
    onExit: (status: number | null) => void,
  ) => {
    const data = new Channel<ArrayBuffer | number[]>();
    data.onmessage = (m) => onData(m instanceof ArrayBuffer ? new Uint8Array(m) : Uint8Array.from(m));
    const exit = new Channel<number | null>();
    exit.onmessage = onExit;
    return invoke<number>("shell_open", { session, cols, rows, onData: data, onExit: exit });
  },
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
