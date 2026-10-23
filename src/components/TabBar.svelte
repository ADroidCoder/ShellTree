<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import Server from "@lucide/svelte/icons/server";
  import FolderSync from "@lucide/svelte/icons/folder-sync";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import Plus from "@lucide/svelte/icons/plus";
  import X from "@lucide/svelte/icons/x";
  import ArrowUpCircle from "@lucide/svelte/icons/circle-arrow-up";
  import { updater } from "../lib/updater.svelte";
  import { activate, app, closeTab, showHosts, showSftp } from "../lib/store.svelte";
  import { isMac } from "../lib/util";

  let { onNew }: { onNew: () => void } = $props();

  const statusText = { connecting: "正在连接", connected: "已连接", disconnected: "已断开", error: "连接失败" };
</script>

<!-- macOS 上标题栏和标签栏合在一起，左边给红绿灯按钮留位置；整条可以拖动窗口 -->
<div class="tabbar" class:mac={isMac} data-tauri-drag-region>
  <button class="pill fixed" class:active={app.view === "hosts"} onclick={showHosts}>
    <Server size={15} /> {t("主机")}
  </button>
  <button class="pill fixed" class:active={app.view === "sftp"} onclick={showSftp}>
    <FolderSync size={15} /> SFTP
  </button>
  {#if app.tabs.length}<span class="divider"></span>{/if}
  {#each app.tabs as tb (tb.key)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="pill tab"
      class:active={app.view === "session" && tb.key === app.activeKey}
      role="tab"
      tabindex="-1"
      aria-selected={tb.key === app.activeKey}
      title="{tb.host.user}@{tb.host.host}:{tb.host.port} · {t(statusText[tb.status])}"
      onclick={() => activate(tb)}
      onauxclick={(e) => e.button === 1 && closeTab(tb)}
    >
      <span class="ico {tb.status}"><SquareTerminal size={15} /></span>
      <span class="name">{tb.title}</span>
      <button
        class="close"
        title={t("关闭会话")}
        onclick={(e) => {
          e.stopPropagation();
          closeTab(tb);
        }}><X size={13} /></button
      >
    </div>
  {/each}
  <button class="add" title={t("新建连接")} onclick={onNew}><Plus size={16} /></button>
  <div class="fill" data-tauri-drag-region></div>
  {#if updater.update}
    <button class="update" onclick={() => (updater.open = true)} title={t("查看更新内容并安装")}>
      <ArrowUpCircle size={14} /> {t("新版本 {0}", updater.update.version)}
    </button>
  {/if}
</div>

<style>
  .tabbar {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 46px;
    padding: 0 10px;
    background: var(--bg-tabbar);
    user-select: none;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }
  .tabbar::-webkit-scrollbar {
    display: none;
  }
  .tabbar.mac {
    padding-left: 84px;
  }
  .pill {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 12px;
    border: 1px solid transparent;
    border-radius: 9px;
    background: none;
    color: var(--text-dim);
    font-size: 12.5px;
    flex-shrink: 0;
  }
  .fixed {
    font-weight: 600;
  }
  .pill:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .pill.active {
    background: var(--bg-card);
    border-color: var(--border);
    color: var(--text);
  }
  .tab {
    min-width: 130px;
    max-width: 230px;
    padding-right: 6px;
    cursor: default;
  }
  .ico {
    display: flex;
    color: var(--text-faint);
  }
  .ico.connected {
    color: var(--add);
  }
  .ico.connecting {
    color: var(--warn);
    animation: blink 1s ease-in-out infinite;
  }
  .ico.error,
  .ico.disconnected {
    color: var(--del);
  }
  @keyframes blink {
    50% {
      opacity: 0.35;
    }
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: 5px;
    background: none;
    color: var(--text-faint);
    visibility: hidden;
  }
  .tab:hover .close,
  .tab.active .close {
    visibility: visible;
  }
  .close:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .divider {
    width: 1px;
    height: 20px;
    margin: 0 2px;
    background: var(--border);
    flex-shrink: 0;
  }
  .add {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--text-dim);
    flex-shrink: 0;
  }
  .add:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .update {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
    height: 26px;
    padding: 0 11px;
    border: 0;
    border-radius: 13px;
    background: var(--accent);
    color: #fff;
    font-size: 12px;
    font-weight: 600;
  }
  .fill {
    flex: 1;
    align-self: stretch;
    min-width: 40px;
  }
</style>
