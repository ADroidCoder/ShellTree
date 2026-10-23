<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import Server from "@lucide/svelte/icons/server";
  import FingerprintPattern from "@lucide/svelte/icons/fingerprint-pattern";
  import Settings from "@lucide/svelte/icons/settings";
  import Braces from "@lucide/svelte/icons/braces";
  import { app } from "../lib/store.svelte";
  import HostsPage from "./HostsPage.svelte";
  import HostPanel from "./HostPanel.svelte";
  import KnownHostsPage from "./KnownHostsPage.svelte";
  import SettingsPage from "./SettingsPage.svelte";
  import SnippetsPage from "./SnippetsPage.svelte";

  let hostsPage = $state<ReturnType<typeof HostsPage>>();

  export function focusSearch() {
    app.page = "hosts";
    queueMicrotask(() => hostsPage?.focus());
  }

  const nav = [
    { id: "hosts", label: "主机", icon: Server },
    { id: "snippets", label: "命令片段", icon: Braces },
    { id: "known", label: "已知主机", icon: FingerprintPattern },
  ] as const;
</script>

<div class="vault">
  <nav>
    {#each nav as n (n.id)}
      <button class:on={app.page === n.id} onclick={() => (app.page = n.id)}>
        <n.icon size={17} />
        <span>{t(n.label)}</span>
        {#if n.id === "hosts"}<span class="count">{app.hosts.length + app.sshConfig.length}</span>{/if}
      </button>
    {/each}
    <span class="fill"></span>
    <button class:on={app.page === "settings"} onclick={() => (app.page = "settings")}>
      <Settings size={17} />
      <span>{t("设置")}</span>
    </button>
  </nav>
  <main>
    {#if app.page === "hosts"}
      <HostsPage bind:this={hostsPage} />
    {:else if app.page === "known"}
      <KnownHostsPage />
    {:else if app.page === "snippets"}
      <SnippetsPage />
    {:else}
      <SettingsPage />
    {/if}
  </main>
  {#if app.editing && app.page === "hosts"}
    <HostPanel />
  {/if}
</div>

<style>
  .vault {
    display: flex;
    height: 100%;
    background: var(--bg-graph);
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 196px;
    flex-shrink: 0;
    padding: 12px 10px;
    background: var(--bg-side);
    border-right: 1px solid var(--border);
  }
  nav button {
    display: flex;
    align-items: center;
    gap: 11px;
    height: 36px;
    padding: 0 12px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--text-dim);
    font-size: 13.5px;
    text-align: left;
  }
  nav button:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  nav button.on {
    background: var(--bg-card);
    color: var(--text);
    font-weight: 600;
  }
  nav button span:first-of-type {
    flex: 1;
  }
  .count {
    color: var(--text-faint);
    font-size: 11.5px;
    font-weight: 400;
  }
  .fill {
    flex: 1;
  }
  main {
    flex: 1;
    min-width: 0;
  }
  @media (max-width: 980px) {
    /* 窗口窄时导航只显示图标 */
    nav {
      width: 56px;
      padding: 12px 8px;
    }
    nav button {
      justify-content: center;
      padding: 0;
    }
    nav button span {
      display: none;
    }
  }
</style>
