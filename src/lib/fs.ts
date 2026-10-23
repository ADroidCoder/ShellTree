// 文件浏览的数据来源：本地或某个 SSH 会话。文件列表组件只跟这个接口打交道，
// 所以本地栏和远程栏用的是同一个组件。
import { api, type Entry } from "./api";
import { hostLabel } from "./util";
import type { Tab } from "./store.svelte";

export interface FileSource {
  /** "local" 或会话的 key */
  key: string;
  remote: boolean;
  label: string;
  home(): Promise<string>;
  list(path: string): Promise<Entry[]>;
  mkdir(path: string): Promise<void>;
  rename(from: string, to: string): Promise<void>;
  remove(path: string, isDir: boolean): Promise<void>;
  chmod?(path: string, perm: number): Promise<void>;
  join(dir: string, name: string): string;
  parent(path: string): string;
  crumbs(path: string): { name: string; path: string }[];
}

// ---------- 路径：远程总是 /，本地在 Windows 上是 \ ----------

const isWinPath = (p: string) => /^[A-Za-z]:/.test(p);

function posixJoin(dir: string, name: string) {
  return dir.endsWith("/") ? dir + name : `${dir}/${name}`;
}

function posixParent(p: string) {
  if (p === "/" || !p) return "/";
  const i = p.replace(/\/+$/, "").lastIndexOf("/");
  return i <= 0 ? "/" : p.slice(0, i);
}

function posixCrumbs(p: string) {
  const parts = p.split("/").filter(Boolean);
  return [{ name: "/", path: "/" }, ...parts.map((n, i) => ({ name: n, path: "/" + parts.slice(0, i + 1).join("/") }))];
}

function localJoin(dir: string, name: string) {
  if (!isWinPath(dir)) return posixJoin(dir, name);
  return /[\\/]$/.test(dir) ? dir + name : `${dir}\\${name}`;
}

function localParent(p: string) {
  if (!isWinPath(p)) return posixParent(p);
  const t = p.replace(/[\\/]+$/, "");
  const i = Math.max(t.lastIndexOf("\\"), t.lastIndexOf("/"));
  // C:\ 已经是根目录
  return i <= 2 ? t.slice(0, 2) + "\\" : t.slice(0, i);
}

function localCrumbs(p: string) {
  if (!isWinPath(p)) return posixCrumbs(p);
  const parts = p.split(/[\\/]/).filter(Boolean);
  return parts.map((n, i) => ({ name: i === 0 ? n + "\\" : n, path: i === 0 ? n + "\\" : parts.slice(0, i + 1).join("\\") }));
}

export function baseName(p: string) {
  return p.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? p;
}

export const localSource: FileSource = {
  key: "local",
  remote: false,
  label: "本地",
  home: () => api.localHome(),
  list: (p) => api.localList(p),
  mkdir: (p) => api.localMkdir(p),
  rename: (a, b) => api.localRename(a, b),
  remove: (p, d) => api.localRemove(p, d),
  join: localJoin,
  parent: localParent,
  crumbs: localCrumbs,
};

export function remoteSource(tab: Tab): FileSource {
  const s = tab.key;
  return {
    key: s,
    remote: true,
    label: hostLabel(tab.host),
    home: () => api.sftpHome(s),
    list: (p) => api.sftpList(s, p),
    mkdir: (p) => api.sftpMkdir(s, p),
    rename: (a, b) => api.sftpRename(s, a, b),
    remove: (p, d) => api.sftpRemove(s, p, d),
    chmod: (p, m) => api.sftpChmod(s, p, m),
    join: posixJoin,
    parent: posixParent,
    crumbs: posixCrumbs,
  };
}
