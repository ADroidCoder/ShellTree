// 分屏布局的树操作。运行：npm test
import { test } from "node:test";
import assert from "node:assert/strict";
import { layout, leaf, leaves, remove, setRatio, split } from "../src/lib/panes.ts";

test("分屏、关闭后旁边一块占满", () => {
  let root = leaf("a");
  root = split(root, "a", "row", "b");
  root = split(root, "b", "col", "c");
  assert.deepEqual(leaves(root), ["a", "b", "c"]);
  const { rects, dividers } = layout(root);
  assert.deepEqual(rects.a, { x: 0, y: 0, w: 0.5, h: 1 });
  assert.deepEqual(rects.b, { x: 0.5, y: 0, w: 0.5, h: 0.5 });
  assert.deepEqual(rects.c, { x: 0.5, y: 0.5, w: 0.5, h: 0.5 });
  assert.equal(dividers.length, 2);

  const r2 = remove(root, "b")!;
  assert.deepEqual(leaves(r2), ["a", "c"]);
  assert.deepEqual(layout(r2).rects.c, { x: 0.5, y: 0, w: 0.5, h: 1 });
  assert.equal(remove(leaf("x"), "x"), null);
  // 不相关的节点不变
  assert.equal(remove(root, "zzz"), root);
});

test("调整比例有上下限", () => {
  let root = split(leaf("a"), "a", "row", "b");
  root = setRatio(root, root.id, 0.99);
  assert.equal(root.kind === "split" && root.ratio, 0.9);
  assert.equal(layout(root).rects.a.w, 0.9);
});
