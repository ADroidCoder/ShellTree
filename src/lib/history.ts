// 终端里执行过的命令，用来做灰色的补全提示。按主机分开记，只存在本机。
import { api } from "./api";

interface Item {
  /** 命令 */
  c: string;
  /** 执行次数 */
  n: number;
  /** 最后一次执行的时间（秒） */
  t: number;
}

/** 每台主机最多记这么多条，超出时丢掉最久没用的 */
const PER_HOST = 1000;

let data: Record<string, Item[]> = {};
let loaded: Promise<void> | null = null;
let saveTimer = 0;

export function loadHistory() {
  loaded ??= api
    .dataRead("history")
    .then((s) => {
      if (s) data = JSON.parse(s);
    })
    .catch(() => {});
  return loaded;
}

function scheduleSave() {
  clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => {
    api.dataWrite("history", JSON.stringify(data)).catch(() => {});
  }, 2000);
}

/** 用来区分主机的键：保存过的主机用 id，快速连接的用地址 */
export function historyKey(h: { id: string; user: string; host: string; port: number }) {
  return h.id || `${h.user}@${h.host}:${h.port}`;
}

/** 这些不值得记：太短、太长、以空格开头（和 bash 的 HISTCONTROL=ignorespace 一样，表示不想留记录） */
function worthKeeping(cmd: string) {
  return cmd.length >= 2 && cmd.length <= 1000 && !/^\s/.test(cmd) && !cmd.includes("\n");
}

export function recordCommand(key: string, raw: string) {
  if (!worthKeeping(raw)) return;
  const cmd = raw.trimEnd();
  const list = (data[key] ??= []);
  const now = Math.floor(Date.now() / 1000);
  const hit = list.find((x) => x.c === cmd);
  if (hit) {
    hit.n++;
    hit.t = now;
  } else {
    list.push({ c: cmd, n: 1, t: now });
    if (list.length > PER_HOST) {
      list.sort((a, b) => b.t - a.t);
      list.length = PER_HOST;
    }
  }
  scheduleSave();
}

/** 越近用过、用得越多的越靠前 */
function score(x: Item, now: number) {
  const days = (now - x.t) / 86400;
  return Math.log2(x.n + 1) - days / 7;
}

/**
 * 给正在输入的内容找一条补全，返回还没输入的后半截；找不到返回 ""。
 * 先找这台主机的历史，没有再找其他主机的
 */
export function suggest(key: string, typed: string): string {
  if (!typed.trim()) return "";
  const now = Date.now() / 1000;
  const pick = (list: Item[] | undefined) => {
    let best: Item | null = null;
    let bestScore = -Infinity;
    for (const x of list ?? []) {
      if (x.c.length <= typed.length || !x.c.startsWith(typed)) continue;
      const s = score(x, now);
      if (s > bestScore) {
        best = x;
        bestScore = s;
      }
    }
    return best;
  };
  let best = pick(data[key]);
  if (!best) {
    for (const k in data) {
      if (k === key) continue;
      const b = pick(data[k]);
      if (b && (!best || score(b, now) > score(best, now))) best = b;
    }
  }
  return best ? best.c.slice(typed.length) : "";
}

export function clearHistory(key?: string) {
  if (key) delete data[key];
  else data = {};
  scheduleSave();
}

export function historyCount() {
  return Object.values(data).reduce((n, l) => n + l.length, 0);
}
