<script lang="ts">
  import { onMount } from "svelte";
  import { activate, activeTab, app, closeTab, loadHosts, newHost, showHosts, startListeners } from "./lib/store.svelte";
  import { applyTheme, watchSystemTheme } from "./lib/settings.svelte";
  import { syncBackendLang } from "./lib/i18n.svelte";
  import { isMac } from "./lib/util";
  import TabBar from "./components/TabBar.svelte";
  import VaultView from "./components/VaultView.svelte";
  import SftpView from "./components/SftpView.svelte";
  import HostKeyDialog from "./components/HostKeyDialog.svelte";
  import SessionView from "./components/SessionView.svelte";
  import Dialog from "./components/Dialog.svelte";
  import UpdateDialog from "./components/UpdateDialog.svelte";
  import { startUpdater, updater } from "./lib/updater.svelte";

  let vault = $state<ReturnType<typeof VaultView>>();
  let views: Record<string, ReturnType<typeof SessionView>> = $state({});

  onMount(() => {
    applyTheme();
    syncBackendLang();
    loadHosts();
    startUpdater();
    const stopTheme = watchSystemTheme();
    const stop = startListeners();
    return () => {
      stopTheme();
      stop();
    };
  });

  function newConnection() {
    showHosts();
    queueMicrotask(() => vault?.focusSearch());
  }

  function onGlobalKey(e: KeyboardEvent) {
    const key = e.key.toLowerCase();
    // 重新加载页面会断开所有连接
    if (e.key === "F5" || ((isMac ? e.metaKey : e.ctrlKey) && key === "r")) e.preventDefault();
    // Windows 上 Ctrl+字母 要留给终端（Ctrl+W 删词、Ctrl+T 交换字符），应用快捷键加 Shift
    const mod = isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && e.shiftKey;
    if (!mod) return;
    const tab = activeTab();
    if (key === "t") newConnection();
    else if (key === "n") {
      showHosts();
      app.page = "hosts";
      app.editing = newHost();
    }
    else if (key === "w" && tab) closeTab(tab);
    else if (key === "e" && tab) views[tab.key]?.toggleFiles();
    else if (key === "j" && tab) views[tab.key]?.toggleSnippets();
    else if (/^[1-9]$/.test(e.key) && app.tabs[Number(e.key) - 1]) activate(app.tabs[Number(e.key) - 1]);
    else return;
    e.preventDefault();
  }
</script>

<svelte:window onkeydown={onGlobalKey} />

<div class="app">
  <TabBar onNew={newConnection} />
  <div class="body">
    <div class="view" class:hidden={app.view !== "hosts"}>
      <VaultView bind:this={vault} />
    </div>
    <div class="view" class:hidden={app.view !== "sftp"}>
      <SftpView active={app.view === "sftp"} />
    </div>
    <!-- 切到别的标签页时会话保持挂载，终端内容和滚动位置都还在 -->
    {#each app.tabs as t (t.key)}
      <div class="view" class:hidden={app.view !== "session" || t.key !== app.activeKey}>
        <SessionView bind:this={views[t.key]} tab={t} active={app.view === "session" && t.key === app.activeKey} />
      </div>
    {/each}
  </div>
</div>

<div class="toasts">
  {#each app.toasts as t (t.id)}
    <div class="toast {t.kind}">{t.text}</div>
  {/each}
</div>
{#if app.hostKeys.length}
  <HostKeyDialog />
{/if}
{#if updater.open && updater.update}
  <UpdateDialog />
{/if}
{#if app.dialog}
  <Dialog />
{/if}

<style>
  .app {
    display: grid;
    grid-template-rows: auto 1fr;
    grid-template-columns: minmax(0, 1fr);
    height: 100vh;
  }
  .body {
    position: relative;
    min-height: 0;
  }
  .view {
    position: absolute;
    inset: 0;
  }
  .view.hidden {
    visibility: hidden;
    pointer-events: none;
  }
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    z-index: 200;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 460px;
  }
  .toast {
    padding: 10px 14px;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-left: 4px solid var(--accent);
    border-radius: 6px;
    box-shadow: var(--shadow);
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 240px;
    overflow: auto;
    user-select: text;
  }
  .toast.error {
    border-left-color: var(--del);
  }
</style>
