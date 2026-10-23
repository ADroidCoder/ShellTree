<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import Unplug from "@lucide/svelte/icons/unplug";
  import Braces from "@lucide/svelte/icons/braces";
  import { app, closeTab, connectTab, type Tab } from "../lib/store.svelte";
  import SftpPanel from "./SftpPanel.svelte";
  import SnippetsSide from "./SnippetsSide.svelte";
  import Terminal from "./Terminal.svelte";

  let { tab, active }: { tab: Tab; active: boolean } = $props();

  let terminal = $state<ReturnType<typeof Terminal>>();
  const KEY = "shelltree.sftpWidth";
  let width = $state(Number(localStorage.getItem(KEY)) || 480);

  function startResize(e: PointerEvent) {
    const startX = e.clientX;
    const startW = width;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => {
      width = Math.max(320, Math.min(window.innerWidth - 360, startW - (ev.clientX - startX)));
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      try {
        localStorage.setItem(KEY, String(width));
      } catch {
        // 存不了就算了
      }
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  }

  export function toggleFiles() {
    toggle("files");
  }

  export function toggleSnippets() {
    toggle("snippets");
  }

  function toggle(p: "files" | "snippets") {
    tab.side = tab.side === p ? null : p;
    if (p === "files") filesMounted = true;
    if (!tab.side) terminal?.focus();
  }

  // 文件面板打开过一次后就一直保留（切到片段面板再切回来，目录还在）
  let filesMounted = $state(false);
  $effect(() => {
    if (tab.side === "files") filesMounted = true;
  });

  const running = $derived(app.transfers.filter((t) => t.session === tab.key && t.state === "running").length);
</script>

<div class="session">
  <div class="term">
    <Terminal bind:this={terminal} {tab} {active} />
  </div>
  {#if tab.side}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="splitter" onpointerdown={startResize}></div>
  {/if}
  <div class="files" style="width:{width}px" class:hidden={tab.side !== "files"}>
    {#if filesMounted || tab.side === "files"}
      {#if tab.generation > 0}
        <SftpPanel {tab} active={active && tab.side === "files"} onRun={(cmd) => terminal?.run(cmd)} />
      {:else}
        <div class="wait">{t("连接后显示远程文件")}</div>
      {/if}
    {/if}
  </div>
  {#if tab.side === "snippets"}
    <div class="files" style="width:{Math.min(width, 380)}px">
      <SnippetsSide onRun={(cmd) => terminal?.run(cmd)} onInsert={(text) => terminal?.insert(text)} />
    </div>
  {/if}
  <div class="rail">
    <button class:on={tab.side === "files"} onclick={toggleFiles} title={t("文件（⌘E）")}>
      <FolderOpen size={18} />
      {#if running}<span class="badge">{running}</span>{/if}
    </button>
    <button class:on={tab.side === "snippets"} onclick={toggleSnippets} title={t("命令片段（⌘J）")}>
      <Braces size={18} />
    </button>
    <span class="fill"></span>
    <button onclick={() => connectTab(tab)} title={t("重新连接")} disabled={tab.status === "connecting"}><RotateCw size={17} /></button>
    <button class="danger" onclick={() => closeTab(tab)} title={t("断开并关闭")}><Unplug size={17} /></button>
  </div>
</div>

<style>
  .session {
    display: flex;
    height: 100%;
    background: var(--bg-graph);
  }
  .term {
    flex: 1;
    min-width: 0;
  }
  .splitter {
    width: 5px;
    margin: 0 -2px;
    z-index: 1;
    cursor: col-resize;
    position: relative;
    flex-shrink: 0;
  }
  .splitter::after {
    content: "";
    position: absolute;
    left: 2px;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--border);
  }
  .splitter:hover::after {
    background: var(--accent);
  }
  .files {
    flex-shrink: 0;
    min-width: 0;
  }
  .files.hidden {
    display: none;
  }
  .wait {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    background: var(--bg-side);
    color: var(--text-faint);
  }
  .rail {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    width: 46px;
    flex-shrink: 0;
    padding: 8px 0;
    background: var(--bg-side);
    border-left: 1px solid var(--border);
  }
  .rail button {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    padding: 0;
    border: 0;
    border-radius: 9px;
    background: none;
    color: var(--text-dim);
  }
  .rail button:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text);
  }
  .rail button.on {
    background: var(--bg-card);
    color: var(--accent);
  }
  .rail button.danger:hover {
    color: var(--del);
  }
  .badge {
    position: absolute;
    top: 2px;
    right: 1px;
    min-width: 15px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    font-size: 10px;
    line-height: 15px;
  }
  .fill {
    flex: 1;
  }
</style>
