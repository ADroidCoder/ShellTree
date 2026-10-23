// 文件列表之间的拖拽（本地栏 ↔ 远程栏）。
// 不用 HTML5 拖放：开启系统文件拖入后，Windows 上的 WebView 里 HTML5 拖放会失效，所以自己用指针事件实现。
import type { Entry } from "./api";
import type { FileSource } from "./fs";

export interface DragPayload {
  pane: string;
  source: FileSource;
  dir: string;
  entries: Entry[];
}

type Receiver = (from: DragPayload, dir: string) => void;

export const drag = $state({
  from: null as DragPayload | null,
  x: 0,
  y: 0,
  /** 鼠标下面的栏和文件夹 */
  over: null as { pane: string; dir: string } | null,
});

const receivers = new Map<string, { receive: Receiver; dir: () => string; join: (d: string, n: string) => string }>();

export function registerPane(id: string, r: { receive: Receiver; dir: () => string; join: (d: string, n: string) => string }) {
  receivers.set(id, r);
  return () => receivers.delete(id);
}

function hit(x: number, y: number) {
  const el = document.elementFromPoint(x, y);
  const pane = el?.closest<HTMLElement>("[data-pane]")?.dataset.pane;
  if (!pane) return null;
  const r = receivers.get(pane);
  if (!r) return null;
  const sub = el?.closest<HTMLElement>("[data-dir]")?.dataset.dir;
  return { pane, dir: sub ? r.join(r.dir(), sub) : r.dir() };
}

/** 在一行上按下鼠标后调用；移动超过几个像素才算开始拖 */
export function beginDrag(e: PointerEvent, payload: () => DragPayload | null) {
  if (e.button !== 0) return;
  const sx = e.clientX;
  const sy = e.clientY;
  let started = false;
  const move = (ev: PointerEvent) => {
    if (!started) {
      if (Math.abs(ev.clientX - sx) + Math.abs(ev.clientY - sy) < 6) return;
      const p = payload();
      if (!p) return cleanup();
      started = true;
      drag.from = p;
    }
    drag.x = ev.clientX;
    drag.y = ev.clientY;
    const h = hit(ev.clientX, ev.clientY);
    // 拖到自己选中的文件夹上没有意义
    drag.over = h && !(h.pane === drag.from!.pane && h.dir === drag.from!.dir) ? h : null;
  };
  const up = () => {
    const from = drag.from;
    const over = drag.over;
    cleanup();
    if (started && from && over) {
      const isSelf = over.pane === from.pane && from.entries.some((x) => from.source.join(from.dir, x.name) === over.dir);
      if (!isSelf) receivers.get(over.pane)?.receive(from, over.dir);
    }
  };
  const cleanup = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    drag.from = null;
    drag.over = null;
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}
