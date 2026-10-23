<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import Search from "@lucide/svelte/icons/search";
  import Play from "@lucide/svelte/icons/play";
  import TextCursorInput from "@lucide/svelte/icons/text-cursor-input";
  import Settings from "@lucide/svelte/icons/settings";
  import { onMount } from "svelte";
  import { app } from "../lib/store.svelte";
  import { fillSnippet, loadSnippets, snippets, snippetVars, type Snippet } from "../lib/snippets.svelte";

  let { onRun, onInsert }: { onRun: (cmd: string) => void; onInsert: (text: string) => void } = $props();

  let query = $state("");

  onMount(() => {
    loadSnippets();
  });

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? snippets.list.filter((s) => `${s.name} ${s.command}`.toLowerCase().includes(q)) : snippets.list;
  });

  /** 多行命令逐行执行：换行换成回车 */
  const toTerminal = (cmd: string) => cmd.replace(/\r?\n/g, "\r");

  async function run(s: Snippet) {
    const cmd = await fillSnippet(s.command);
    if (cmd !== null) onRun(toTerminal(cmd));
  }

  async function insert(s: Snippet) {
    const cmd = await fillSnippet(s.command);
    if (cmd !== null) onInsert(toTerminal(cmd));
  }

  function manage() {
    app.page = "snippets";
    app.view = "hosts";
  }
</script>

<div class="side">
  <div class="top">
    <div class="search">
      <Search size={14} />
      <input bind:value={query} placeholder={t("搜索片段")} spellcheck="false" />
    </div>
    <button class="icon" title={t("管理命令片段")} onclick={manage}><Settings size={15} /></button>
  </div>
  <div class="list">
    {#each shown as s (s.id)}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="item" role="button" tabindex="-1" title={t("双击执行")} ondblclick={() => run(s)}>
        <div class="name">
          {s.name}
          {#if snippetVars(s.command).length}<span class="var">{"{{…}}"}</span>{/if}
        </div>
        <code>{s.command}</code>
        <div class="acts">
          <button onclick={() => insert(s)} title={t("放到命令行，不执行")}><TextCursorInput size={13} /> {t("插入")}</button>
          <button class="primary" onclick={() => run(s)}><Play size={12} /> {t("执行")}</button>
        </div>
      </div>
    {:else}
      {#if snippets.loaded}
        <div class="empty">
          {query ? t("没有匹配的片段") : t("还没有命令片段")}
          {#if !query}<button onclick={manage}>{t("新建片段")}</button>{/if}
        </div>
      {/if}
    {/each}
  </div>
</div>

<style>
  .side {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-side);
  }
  .top {
    display: flex;
    gap: 6px;
    padding: 10px;
  }
  .search {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--input-bg);
    color: var(--text-faint);
  }
  .search:focus-within {
    border-color: var(--accent);
  }
  .search input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    background: none;
  }
  .icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border-radius: 8px;
    color: var(--text-dim);
  }
  .list {
    flex: 1;
    overflow: auto;
    padding: 0 10px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .item {
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--bg-card);
    cursor: default;
  }
  .item:hover {
    background: var(--bg-card-hover);
  }
  .name {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
  }
  .var {
    color: var(--accent);
    font-family: var(--mono);
    font-size: 11px;
    font-weight: 400;
  }
  code {
    display: block;
    margin-top: 5px;
    color: var(--text-dim);
    font-family: var(--mono);
    font-size: 11.5px;
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 4.5em;
    overflow: hidden;
  }
  .acts {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    margin-top: 8px;
  }
  .acts button {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 10px;
    border-radius: 6px;
    font-size: 12px;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 50px 10px;
    color: var(--text-faint);
  }
</style>
