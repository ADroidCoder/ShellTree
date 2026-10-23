<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import Search from "@lucide/svelte/icons/search";
  import Plus from "@lucide/svelte/icons/plus";
  import Braces from "@lucide/svelte/icons/braces";
  import Pencil from "@lucide/svelte/icons/pencil";
  import X from "@lucide/svelte/icons/x";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import { onMount } from "svelte";
  import { confirm } from "../lib/store.svelte";
  import { deleteSnippet, loadSnippets, snippets, snippetVars, upsertSnippet, type Snippet } from "../lib/snippets.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";

  let query = $state("");
  let editing = $state<Snippet | null>(null);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let nameEl = $state<HTMLInputElement>();

  onMount(() => {
    loadSnippets();
  });

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? snippets.list.filter((s) => `${s.name} ${s.command}`.toLowerCase().includes(q)) : snippets.list;
  });

  function edit(s?: Snippet) {
    editing = s ? { ...s } : { id: "", name: "", command: "" };
    queueMicrotask(() => nameEl?.focus());
  }

  async function save() {
    if (!editing) return;
    const s = { ...editing, name: editing.name.trim(), command: editing.command.replace(/\s+$/, "") };
    if (!s.command) return;
    if (!s.name) s.name = s.command.split("\n")[0].slice(0, 40);
    await upsertSnippet(s);
    editing = null;
  }

  async function remove(s: Snippet) {
    if (!(await confirm(t("删除片段"), t("确定删除“{0}”吗？", s.name), t("删除"), true))) return;
    await deleteSnippet(s);
    if (editing?.id === s.id) editing = null;
  }

  function showMenu(e: MouseEvent, s: Snippet) {
    e.preventDefault();
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: t("编辑…"), action: () => edit(s) },
        { label: t("复制命令"), action: () => navigator.clipboard.writeText(s.command) },
        { label: t("复制一份…"), action: () => edit({ ...s, id: "", name: t("{0} 副本", s.name) }) },
        "sep",
        { label: t("删除"), danger: true, action: () => remove(s) },
      ],
    };
  }

  function onKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Escape") editing = null;
    // 命令框里回车是换行，⌘/Ctrl+回车保存
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      save();
    }
  }
</script>

<div class="wrap">
  <div class="page">
    <div class="head">
      <h2>{t("命令片段")}</h2>
      <div class="desc">{t("保存常用的命令，在终端右侧的片段面板里一键插入或执行。命令里可以写 {{变量}}，执行前会让你填写。")}</div>
    </div>
    <div class="bar">
      <button class="primary" onclick={() => edit()}><Plus size={15} /> {t("新建片段")}</button>
      <div class="search">
        <Search size={15} />
        <input bind:value={query} placeholder={t("筛选")} spellcheck="false" />
      </div>
    </div>
    <div class="scroll">
      {#each shown as s (s.id)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="card" class:editing={editing?.id === s.id} role="button" tabindex="-1" onclick={() => edit(s)} oncontextmenu={(e) => showMenu(e, s)}>
          <span class="ico"><Braces size={18} /></span>
          <span class="text">
            <span class="title">{s.name}</span>
            <code>{s.command}</code>
          </span>
          <button class="act" title={t("编辑")} onclick={(e) => (e.stopPropagation(), edit(s))}><Pencil size={14} /></button>
          <button class="act danger" title={t("删除")} onclick={(e) => (e.stopPropagation(), remove(s))}><Trash2 size={14} /></button>
        </div>
      {:else}
        {#if snippets.loaded}
          <div class="empty">
            {#if query}
              {t("没有匹配的片段")}
            {:else}
              <div class="big">{t("还没有命令片段")}</div>
              <div>{t("比如：tail -f /var/log/{{服务名}}.log")}</div>
            {/if}
          </div>
        {/if}
      {/each}
    </div>
  </div>

  {#if editing}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <aside onkeydown={onKey}>
      <header>
        <div class="title">{editing.id ? t("编辑片段") : t("新建片段")}</div>
        <button class="icon" title={t("关闭（Esc）")} onclick={() => (editing = null)}><X size={17} /></button>
      </header>
      <div class="body">
        <label>
          <span>{t("名称")}</span>
          <input bind:this={nameEl} bind:value={editing.name} placeholder={t("例如：查看 Nginx 日志")} spellcheck="false" />
        </label>
        <label class="grow">
          <span>{t("命令")}</span>
          <textarea bind:value={editing.command} spellcheck="false" placeholder={"tail -f /var/log/nginx/{{name}}.log"}></textarea>
        </label>
        {#if snippetVars(editing.command).length}
          <div class="vars">
            {t("执行时会询问：")}
            {#each snippetVars(editing.command) as v (v)}<code>{v}</code>{/each}
          </div>
        {:else}
          <div class="hint">{t("用 {{变量名}} 表示每次执行时要填的内容。多行命令会依次执行。")}</div>
        {/if}
      </div>
      <footer>
        <button onclick={() => (editing = null)}>{t("取消")}</button>
        <button class="primary" disabled={!editing.command.trim()} onclick={save}>{t("保存")}</button>
      </footer>
    </aside>
  {/if}
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
{/if}

<style>
  .wrap {
    display: flex;
    height: 100%;
  }
  .page {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .head {
    padding: 22px 24px 4px;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  .desc {
    margin-top: 6px;
    color: var(--text-faint);
    line-height: 1.5;
  }
  .bar {
    display: flex;
    gap: 8px;
    padding: 12px 24px;
  }
  .bar button {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    border-radius: 8px;
  }
  .search {
    flex: 1;
    max-width: 360px;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-card);
    color: var(--text-faint);
  }
  .search:focus-within {
    border-color: var(--accent);
  }
  .search input {
    flex: 1;
    height: 100%;
    padding: 0;
    border: 0;
    background: none;
  }
  .scroll {
    flex: 1;
    overflow: auto;
    padding: 0 24px 24px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .card {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 58px;
    padding: 8px 12px;
    border: 1px solid transparent;
    border-radius: var(--radius-lg);
    background: var(--bg-card);
    cursor: default;
  }
  .card:hover {
    background: var(--bg-card-hover);
  }
  .card.editing {
    border-color: var(--accent);
  }
  .ico {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    border-radius: 9px;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
    flex-shrink: 0;
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .title {
    font-weight: 600;
  }
  .text code {
    color: var(--text-dim);
    font-family: var(--mono);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .act {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--text-faint);
    visibility: hidden;
  }
  .card:hover .act {
    visibility: visible;
  }
  .act:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .act.danger:hover {
    color: var(--del);
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 70px 20px;
    color: var(--text-faint);
  }
  .big {
    color: var(--text);
    font-size: 16px;
    font-weight: 600;
  }
  aside {
    display: flex;
    flex-direction: column;
    width: 400px;
    flex-shrink: 0;
    background: var(--bg-side);
    border-left: 1px solid var(--border);
  }
  header {
    display: flex;
    align-items: center;
    padding: 18px 16px 10px 20px;
  }
  header .title {
    flex: 1;
    font-size: 16px;
    font-weight: 700;
  }
  .icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--text-faint);
  }
  .icon:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 4px 20px 16px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  label > span {
    color: var(--text-dim);
    font-size: 12.5px;
    font-weight: 600;
  }
  label.grow {
    flex: 1;
    min-height: 0;
  }
  textarea {
    flex: 1;
    min-height: 140px;
    resize: none;
    font-family: var(--mono);
    font-size: 12.5px;
  }
  .vars,
  .hint {
    color: var(--text-faint);
    font-size: 12px;
    line-height: 1.6;
  }
  .vars code {
    margin-left: 6px;
    padding: 1px 6px;
    border-radius: 5px;
    background: var(--bg-card);
    color: var(--accent);
    font-family: var(--mono);
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 16px 16px;
    border-top: 1px solid var(--border);
  }
  footer button {
    height: 34px;
    min-width: 80px;
    border-radius: 8px;
  }
</style>
