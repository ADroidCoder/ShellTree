export const isMac = typeof navigator !== "undefined" && /Mac/.test(navigator.platform);

/** macOS 用 ⌘，其他系统用 Ctrl */
export function modKey(e: KeyboardEvent | MouseEvent) {
  return isMac ? e.metaKey : e.ctrlKey;
}

const pad = (n: number) => String(n).padStart(2, "0");

export function formatDate(sec: number) {
  if (!sec) return "";
  const d = new Date(sec * 1000);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function formatSize(n: number) {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n;
  let i = -1;
  do {
    v /= 1024;
    i++;
  } while (v >= 1024 && i < units.length - 1);
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

/** 权限位转成 rwxr-xr-x */
export function formatPerm(perm: number) {
  let s = "";
  for (let i = 8; i >= 0; i--) s += perm & (1 << i) ? "rwx"[(8 - i) % 3] : "-";
  return s;
}

export function joinPath(dir: string, name: string) {
  return dir.endsWith("/") ? dir + name : `${dir}/${name}`;
}

export function parentPath(p: string) {
  if (p === "/" || !p) return "/";
  const i = p.replace(/\/+$/, "").lastIndexOf("/");
  return i <= 0 ? "/" : p.slice(0, i);
}

/** 给 shell 命令用的单引号转义 */
export function shellQuote(s: string) {
  return /^[\w./~-]+$/.test(s) ? s : `'${s.replace(/'/g, `'\\''`)}'`;
}

/** “root@10.0.0.1:2222” 这样的快速连接地址 */
export function parseAddress(s: string): { user: string; host: string; port: number } | null {
  const m = s.trim().match(/^(?:ssh\s+)?(?:([^@\s]+)@)?(\[[^\]]+\]|[^:\s]+)(?::(\d+))?$/);
  if (!m) return null;
  return { user: m[1] ?? "", host: m[2].replace(/^\[|\]$/g, ""), port: m[3] ? Number(m[3]) : 22 };
}

export function hostLabel(h: { name: string; user: string; host: string }) {
  return h.name || (h.user ? `${h.user}@${h.host}` : h.host);
}

export function uid() {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
}
