// 命令片段：保存常用的命令，一键发到终端。命令里可以写 {{变量}}，执行前会询问
import { api } from "./api";
import { t } from "./i18n.svelte";
import { ask, toast } from "./store.svelte";
import { uid } from "./util";

export interface Snippet {
  id: string;
  name: string;
  command: string;
}

export const snippets = $state({ list: [] as Snippet[], loaded: false });

export async function loadSnippets() {
  if (snippets.loaded) return;
  try {
    const s = await api.dataRead("snippets");
    if (s) snippets.list = JSON.parse(s).snippets ?? [];
  } catch (e) {
    toast(String(e), "error");
  }
  snippets.loaded = true;
}

async function save() {
  try {
    await api.dataWrite("snippets", JSON.stringify({ snippets: $state.snapshot(snippets.list) }, null, 2));
  } catch (e) {
    toast(String(e), "error");
  }
}

export async function upsertSnippet(s: Snippet) {
  const item = { ...s, id: s.id || uid() };
  const i = snippets.list.findIndex((x) => x.id === item.id);
  if (i >= 0) snippets.list[i] = item;
  else snippets.list.push(item);
  await save();
  return item;
}

export async function deleteSnippet(s: Snippet) {
  snippets.list = snippets.list.filter((x) => x.id !== s.id);
  await save();
}

/** 命令里的变量名，按出现顺序去重 */
export function snippetVars(cmd: string) {
  return [...new Set([...cmd.matchAll(/\{\{\s*([^{}]+?)\s*\}\}/g)].map((m) => m[1]))];
}

/** 把变量替换成用户填的值；用户取消时返回 null */
export async function fillSnippet(cmd: string): Promise<string | null> {
  let out = cmd;
  for (const v of snippetVars(cmd)) {
    const r = await ask({ title: v, message: t("填写命令里的 {0}", `{{${v}}}`), input: { placeholder: v }, confirmText: t("确定") });
    if (!r) return null;
    out = out.replace(new RegExp(`\\{\\{\\s*${v.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*\\}\\}`, "g"), r.input);
  }
  return out;
}
