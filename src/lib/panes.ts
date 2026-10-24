// 分屏布局：一棵二叉树，叶子是一个终端，中间节点把空间分成左右（row）或上下（col）两块。
// 界面不按树嵌套渲染（树一变终端组件就会重建、shell 就断了），而是把每个终端按算出来的位置平铺。

export type Pane = { kind: "leaf"; id: string } | { kind: "split"; id: string; dir: "row" | "col"; ratio: number; a: Pane; b: Pane };

/** 一块区域，取值 0–1，相对整个终端区域 */
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** 分隔条：拖动它改 split 的比例 */
export interface Divider {
  id: string;
  dir: "row" | "col";
  /** 这个 split 占的区域 */
  area: Rect;
  ratio: number;
}

/** 一个标签页最多分成几块 */
export const MAX_PANES = 8;

let seq = 0;
export const paneId = () => `p${++seq}`;

export function leaf(id = paneId()): Pane {
  return { kind: "leaf", id };
}

export function leaves(p: Pane): string[] {
  return p.kind === "leaf" ? [p.id] : [...leaves(p.a), ...leaves(p.b)];
}

/** 把 target 这一块分成两半，新的一块放在右边或下面 */
export function split(root: Pane, target: string, dir: "row" | "col", id = paneId()): Pane {
  if (root.kind === "leaf") return root.id === target ? { kind: "split", id: paneId(), dir, ratio: 0.5, a: root, b: leaf(id) } : root;
  return { ...root, a: split(root.a, target, dir, id), b: split(root.b, target, dir, id) };
}

/** 关掉一块，旁边那块占满它的位置。只剩这一块时返回 null */
export function remove(root: Pane, target: string): Pane | null {
  if (root.kind === "leaf") return root.id === target ? null : root;
  const a = remove(root.a, target);
  const b = remove(root.b, target);
  if (!a) return b;
  if (!b) return a;
  return a === root.a && b === root.b ? root : { ...root, a, b };
}

export function setRatio(root: Pane, id: string, ratio: number): Pane {
  if (root.kind === "leaf") return root;
  if (root.id === id) return { ...root, ratio: Math.min(0.9, Math.max(0.1, ratio)) };
  return { ...root, a: setRatio(root.a, id, ratio), b: setRatio(root.b, id, ratio) };
}

/** 算出每个终端的位置和每条分隔条 */
export function layout(root: Pane): { rects: Record<string, Rect>; dividers: Divider[] } {
  const rects: Record<string, Rect> = {};
  const dividers: Divider[] = [];
  const walk = (p: Pane, r: Rect) => {
    if (p.kind === "leaf") {
      rects[p.id] = r;
      return;
    }
    dividers.push({ id: p.id, dir: p.dir, area: r, ratio: p.ratio });
    if (p.dir === "row") {
      const w = r.w * p.ratio;
      walk(p.a, { ...r, w });
      walk(p.b, { ...r, x: r.x + w, w: r.w - w });
    } else {
      const h = r.h * p.ratio;
      walk(p.a, { ...r, h });
      walk(p.b, { ...r, y: r.y + h, h: r.h - h });
    }
  };
  walk(root, { x: 0, y: 0, w: 1, h: 1 });
  return { rects, dividers };
}
