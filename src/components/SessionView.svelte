<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import Unplug from "@lucide/svelte/icons/unplug";
  import Braces from "@lucide/svelte/icons/braces";
  import ArrowLeftRight from "@lucide/svelte/icons/arrow-left-right";
  import Columns2 from "@lucide/svelte/icons/columns-2";
  import Radio from "@lucide/svelte/icons/radio";
  import { app, closePane, closeTab, connectTab, cyclePane, setBroadcastAll, splitFocused, type Tab } from "../lib/store.svelte";
  import { layout, leaves, setRatio, type Divider } from "../lib/panes";
  import { isMac } from "../lib/util";
  import ConnectOverlay from "./ConnectOverlay.svelte";
  import SftpPanel from "./SftpPanel.svelte";
  import SnippetsSide from "./SnippetsSide.svelte";
  import ForwardsSide from "./ForwardsSide.svelte";
  import Terminal from "./Terminal.svelte";
  import StatusBar from "./StatusBar.svelte";
  import { settings } from "../lib/settings.svelte";

  let { tab, active }: { tab: Tab; active: boolean } = $props();

  // 分屏里的每个终端，输入、查找这些操作发给焦点所在的那个
  let terms: Record<string, ReturnType<typeof Terminal>> = $state({});
  const terminal = $derived(terms[tab.focused]);
  const panes = $derived(leaves(tab.layout));
  const geo = $derived(layout(tab.layout));
  let screen: HTMLDivElement;
  const broadcasting = $derived(app.tabs.filter((t) => t.broadcast).length);
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

  export function find() {
    terminal?.find();
  }

  export function split(dir: "row" | "col") {
    splitFocused(tab, dir);
  }

  /** ⌘W：分屏时关掉焦点所在的一块，否则关掉标签页 */
  export function closeFocused() {
    if (panes.length > 1) closePane(tab, tab.focused);
    else closeTab(tab);
  }

  export function cycle(step: number) {
    if (panes.length > 1) cyclePane(tab, step);
  }

  function dragDivider(e: PointerEvent, d: Divider) {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const box = screen.getBoundingClientRect();
    const move = (ev: PointerEvent) => {
      const r =
        d.dir === "row"
          ? ((ev.clientX - box.left) / box.width - d.area.x) / d.area.w
          : ((ev.clientY - box.top) / box.height - d.area.y) / d.area.h;
      tab.layout = setRatio(tab.layout, d.id, r);
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  }

  const pct = (n: number) => `${n * 100}%`;

  export function toggleSnippets() {
    toggle("snippets");
  }

  function toggle(p: "files" | "snippets" | "forwards") {
    // 本地终端没有文件面板和端口转发
    if (tab.local && p !== "snippets") return;
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
    <div class="screen" bind:this={screen}>
      {#each panes as id (id)}
        {@const r = geo.rects[id]}
        <div
          class="pane"
          class:dim={panes.length > 1 && tab.focused !== id}
          style="left:{pct(r.x)}; top:{pct(r.y)}; width:{pct(r.w)}; height:{pct(r.h)}"
        >
          <Terminal bind:this={terms[id]} {tab} {active} pane={id} focused={tab.focused === id} />
        </div>
      {/each}
      {#each geo.dividers as d (d.id)}
        {@const a = d.area}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="divider {d.dir}"
          style={d.dir === "row"
            ? `left:${pct(a.x + a.w * d.ratio)}; top:${pct(a.y)}; height:${pct(a.h)}`
            : `top:${pct(a.y + a.h * d.ratio)}; left:${pct(a.x)}; width:${pct(a.w)}`}
          onpointerdown={(e) => dragDivider(e, d)}
        ></div>
      {/each}
      {#if tab.broadcast}
        <div class="bcast">
          <Radio size={13} />
          {broadcasting > 1 ? t("广播输入：同时发给 {0} 个标签页", broadcasting) : t("广播输入：在其他标签页也打开后，输入会同时发过去")}
          <button onclick={() => (tab.broadcast = false)}>{t("停止")}</button>
          {#if app.tabs.length > broadcasting}
            <button onclick={() => setBroadcastAll(true)}>{t("全部标签页")}</button>
          {:else}
            <button onclick={() => setBroadcastAll(false)}>{t("全部停止")}</button>
          {/if}
        </div>
      {/if}
      {#if !tab.local && ((tab.status === "connecting" && tab.generation === 0) || tab.status === "error")}
        <ConnectOverlay {tab} />
      {/if}
    </div>
    {#if settings.statusBar && !tab.local}
      <StatusBar {tab} {active} />
    {/if}
  </div>
  {#if tab.side}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="splitter" onpointerdown={startResize}></div>
  {/if}
  <div class="files" style="width:{width}px" class:hidden={tab.side !== "files"}>
    {#if filesMounted || tab.side === "files"}
      {#if tab.generation > 0 && !tab.local}
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
  {#if tab.side === "forwards"}
    <div class="files" style="width:{Math.min(width, 380)}px">
      <ForwardsSide {tab} />
    </div>
  {/if}
  <div class="rail">
    {#if !tab.local}
      <button class:on={tab.side === "files"} onclick={toggleFiles} title={t("文件（⌘E）")}>
        <FolderOpen size={18} />
        {#if running}<span class="badge">{running}</span>{/if}
      </button>
    {/if}
    <button class:on={tab.side === "snippets"} onclick={toggleSnippets} title={t("命令片段（⌘J）")}>
      <Braces size={18} />
    </button>
    {#if !tab.local}
      <button class:on={tab.side === "forwards"} onclick={() => toggle("forwards")} title={t("端口转发")}>
        <ArrowLeftRight size={17} />
        {#if tab.forwards.length}<span class="badge">{tab.forwards.length}</span>{/if}
      </button>
    {/if}
    <button onclick={() => split("row")} title={isMac ? t("分屏（⌘D，上下分屏 ⇧⌘D）") : t("分屏（Ctrl+Shift+D）")}>
      <Columns2 size={17} />
    </button>
    <button class:on={tab.broadcast} onclick={() => (tab.broadcast = !tab.broadcast)} title={isMac ? t("广播输入：同时输入到多个标签页（⌘B）") : t("广播输入：同时输入到多个标签页（Ctrl+Shift+B）")}>
      <Radio size={17} />
    </button>
    <span class="fill"></span>
    <button onclick={() => connectTab(tab)} title={tab.local ? t("重新打开") : t("重新连接")} disabled={tab.status === "connecting"}>
      <RotateCw size={17} />
    </button>
    <button class="danger" onclick={() => closeTab(tab)} title={tab.local ? t("关闭") : t("断开并关闭")}><Unplug size={17} /></button>
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
    display: flex;
    flex-direction: column;
  }
  .screen {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .pane {
    position: absolute;
  }
  /* 分屏时没有焦点的那几块稍微暗一点 */
  .pane.dim::after {
    content: "";
    position: absolute;
    inset: 0;
    z-index: 3;
    background: var(--bg-graph);
    opacity: 0.3;
    pointer-events: none;
  }
  .divider {
    position: absolute;
    z-index: 4;
  }
  .divider.row {
    width: 7px;
    transform: translateX(-50%);
    cursor: col-resize;
  }
  .divider.col {
    height: 7px;
    transform: translateY(-50%);
    cursor: row-resize;
  }
  .divider::after {
    content: "";
    position: absolute;
    background: var(--border);
  }
  .divider.row::after {
    left: 3px;
    top: 0;
    bottom: 0;
    width: 1px;
  }
  .divider.col::after {
    top: 3px;
    left: 0;
    right: 0;
    height: 1px;
  }
  .divider:hover::after {
    background: var(--accent);
  }
  .bcast {
    position: absolute;
    top: 8px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 4;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px 4px 12px;
    border-radius: 14px;
    background: #d97706;
    color: #fff;
    font-size: 12px;
    white-space: nowrap;
    box-shadow: var(--shadow);
  }
  .bcast button {
    height: 22px;
    padding: 0 9px;
    border: 0;
    border-radius: 11px;
    background: rgba(255, 255, 255, 0.22);
    color: #fff;
    font-size: 12px;
  }
  .bcast button:hover {
    background: rgba(255, 255, 255, 0.35);
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
