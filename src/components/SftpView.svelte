<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Search from "@lucide/svelte/icons/search";
  import X from "@lucide/svelte/icons/x";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import type { Host } from "../lib/api";
  import { localSource, remoteSource, type FileSource } from "../lib/fs";
  import { app, releaseSftpConnection, sftpConnection, type Tab } from "../lib/store.svelte";
  import { hostLabel } from "../lib/util";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import FileBrowser from "./FileBrowser.svelte";
  import HostIcon from "./HostIcon.svelte";
  import Transfers from "./Transfers.svelte";

  let { active }: { active: boolean } = $props();

  type Pane = { kind: "local" } | { kind: "remote"; tab: Tab; source: FileSource } | { kind: "pick"; connecting?: Host };

  let panes = $state<Pane[]>([{ kind: "local" }, { kind: "pick" }]);
  let browsers = $state<(ReturnType<typeof FileBrowser> | undefined)[]>([]);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let query = $state(["", ""]);

  function sourceOf(p: Pane): FileSource | null {
    return p.kind === "local" ? localSource : p.kind === "remote" ? p.source : null;
  }

  function stillUsed(t: Tab) {
    return panes.some((p) => p.kind === "remote" && p.tab === t);
  }

  function setPane(i: number, p: Pane) {
    const old = panes[i];
    panes[i] = p;
    if (old.kind === "remote") releaseSftpConnection(old.tab, stillUsed);
  }

  async function pickHost(i: number, h: Host) {
    setPane(i, { kind: "pick", connecting: h });
    const tab = await sftpConnection(h);
    // 等连接的时候用户可能已经换了别的
    const cur = panes[i];
    if (cur.kind !== "pick" || cur.connecting !== h) {
      if (tab) releaseSftpConnection(tab, stillUsed);
      return;
    }
    panes[i] = tab ? { kind: "remote", tab, source: remoteSource(tab) } : { kind: "pick" };
  }

  function useTab(i: number, t: Tab) {
    setPane(i, { kind: "remote", tab: t, source: remoteSource(t) });
  }

  function showSourceMenu(e: MouseEvent, i: number) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const open = app.tabs.filter((t) => t.status === "connected");
    const items: MenuItem[] = [{ label: t("本地"), action: () => setPane(i, { kind: "local" }) }];
    if (open.length) {
      items.push("sep");
      for (const tb of open) items.push({ label: tb.title, hint: t("已打开的会话"), action: () => useTab(i, tb) });
    }
    items.push("sep", { label: t("选择主机…"), action: () => setPane(i, { kind: "pick" }) });
    menu = { x: r.left, y: r.bottom + 4, items };
  }

  function hostsFor(i: number) {
    const q = query[i].trim().toLowerCase();
    return [...app.hosts, ...app.sshConfig]
      .filter((h) => !q || `${h.name} ${h.user}@${h.host} ${h.group}`.toLowerCase().includes(q))
      .sort((a, b) => (b.last_used || 0) - (a.last_used || 0) || hostLabel(a).localeCompare(hostLabel(b)));
  }

  /** 给另一栏用的“对面现在在哪个目录” */
  function peerOf(i: number) {
    return () => {
      const j = 1 - i;
      const s = sourceOf(panes[j]);
      const dir = browsers[j]?.currentDir();
      return s && dir ? { source: s, dir } : null;
    };
  }
</script>

<div class="sftp">
  <div class="panes">
    {#each panes as p, i (i)}
      <section class="pane">
        <header>
          <button class="source" onclick={(e) => showSourceMenu(e, i)}>
            {#if p.kind === "local"}
              <HostIcon kind="local" size={24} />
              <span class="name">{t("本地")}</span>
            {:else if p.kind === "remote"}
              <HostIcon host={p.tab.host.host} os={p.tab.host.os} size={24} />
              <span class="name">{p.tab.title}</span>
              {#if p.tab.status !== "connected"}<span class="warn">{t("已断开")}</span>{/if}
            {:else}
              <span class="name dim">{t("选择主机")}</span>
            {/if}
            <ChevronDown size={15} />
          </button>
          {#if p.kind === "remote"}
            <button class="close" title={t("关闭这一栏的连接")} onclick={() => setPane(i, { kind: "pick" })}><X size={15} /></button>
          {/if}
        </header>
        <div class="content">
          {#if p.kind === "pick"}
            <div class="picker">
              {#if p.connecting}
                <div class="connecting">
                  <LoaderCircle size={22} class="spin" />
                  {t("正在连接 {0}…", hostLabel(p.connecting))}
                </div>
              {:else}
                <div class="search">
                  <Search size={15} />
                  <input bind:value={query[i]} placeholder={t("选择要浏览的主机")} spellcheck="false" />
                </div>
                <div class="hosts">
                  {#each hostsFor(i) as h (h.id || h.host)}
                    <button class="host" onclick={() => pickHost(i, h)}>
                      <HostIcon host={h.host} os={h.os} size={32} />
                      <span class="text">
                        <span class="t">{hostLabel(h)}</span>
                        <span class="s">{h.user}@{h.host}</span>
                      </span>
                    </button>
                  {:else}
                    <div class="none">{t("没有主机，先在“主机”页添加")}</div>
                  {/each}
                </div>
              {/if}
            </div>
          {:else}
            {@const s = sourceOf(p)!}
            {#key s.key}
              <FileBrowser bind:this={browsers[i]} source={s} {active} peer={peerOf(i)} />
            {/key}
          {/if}
        </div>
      </section>
    {/each}
  </div>
  <div class="transfers"><Transfers /></div>
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
{/if}

<style>
  .sftp {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-graph);
  }
  .panes {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    padding: 10px;
  }
  .pane {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    border-radius: var(--radius-lg);
    background: var(--bg-side);
    border: 1px solid var(--border);
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 8px 0;
  }
  .source {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 10px 0 6px;
    border: 0;
    border-radius: 9px;
    background: none;
    color: var(--text);
    font-size: 13.5px;
    font-weight: 600;
    min-width: 0;
  }
  .source:hover {
    background: var(--bg-hover);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dim {
    color: var(--text-dim);
    padding-left: 4px;
  }
  .warn {
    color: var(--del);
    font-size: 11.5px;
    font-weight: 400;
  }
  .close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    margin-left: auto;
    padding: 0;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--text-faint);
  }
  .close:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .content {
    flex: 1;
    min-height: 0;
  }
  .picker {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 10px 12px;
    gap: 10px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--input-bg);
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
  .hosts {
    flex: 1;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .host {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 50px;
    padding: 0 10px;
    border: 0;
    border-radius: 10px;
    background: var(--bg-card);
    text-align: left;
    flex-shrink: 0;
  }
  .host:hover {
    background: var(--bg-card-hover);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .t {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .s {
    color: var(--text-faint);
    font-size: 12px;
  }
  .none,
  .connecting {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 60px 0;
    color: var(--text-faint);
  }
  .connecting :global(.spin) {
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .transfers:empty {
    display: none;
  }
  .transfers {
    margin: 0 10px 10px;
    border-radius: var(--radius-lg);
    overflow: hidden;
  }
</style>
