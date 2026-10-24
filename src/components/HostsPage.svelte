<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import Search from "@lucide/svelte/icons/search";
  import Plus from "@lucide/svelte/icons/plus";
  import LayoutGrid from "@lucide/svelte/icons/layout-grid";
  import Rows3 from "@lucide/svelte/icons/rows-3";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import Pencil from "@lucide/svelte/icons/pencil";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Terminal from "@lucide/svelte/icons/terminal";
  import type { Host } from "../lib/api";
  import {
    app,
    createGroup,
    deleteGroup,
    deleteHost,
    moveToGroup,
    newHost,
    openLocal,
    openSession,
    renameGroup,
    SSH_CONFIG_GROUP as SSH_CONFIG,
  } from "../lib/store.svelte";
  import { setHostView, settings } from "../lib/settings.svelte";
  import { hostLabel, isMac, parseAddress } from "../lib/util";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import HostIcon from "./HostIcon.svelte";

  let query = $state("");
  let group = $state<string | null>(null);
  let selected = $state(0);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let inputEl = $state<HTMLInputElement>();

  export function focus() {
    inputEl?.focus();
  }

  const all = $derived([...app.hosts, ...app.sshConfig]);

  function matches(h: Host, q: string) {
    if (!q) return true;
    const s = `${h.name} ${h.user}@${h.host}:${h.port} ${h.group} ${h.os}`.toLowerCase();
    return q
      .toLowerCase()
      .split(/\s+/)
      .every((w) => s.includes(w));
  }

  // 搜索时不分组，直接在全部主机里找；没搜索时先显示分组，点进分组只看这一组
  const groups = $derived.by(() => {
    if (query.trim() || group !== null) return [];
    // 自己建的分组按保存的顺序（包括空分组），~/.ssh/config 放最后
    const m = new Map<string, number>(app.groups.map((g) => [g, 0]));
    for (const h of all) if (h.group) m.set(h.group, (m.get(h.group) ?? 0) + 1);
    return [...m.entries()].sort((a, b) => (a[0] === SSH_CONFIG ? 1 : b[0] === SSH_CONFIG ? -1 : 0));
  });

  const hosts = $derived.by(() => {
    const q = query.trim();
    const list = all.filter((h) => (q ? matches(h, q) : group === null ? !h.group : h.group === group));
    // 最近用过的排前面，其余按名字
    return list.sort((a, b) => (b.last_used || 0) - (a.last_used || 0) || hostLabel(a).localeCompare(hostLabel(b)));
  });

  const quick = $derived.by(() => {
    const q = query.trim();
    if (!q || /\s/.test(q.replace(/^ssh\s+/, ""))) return null;
    return parseAddress(q);
  });

  $effect(() => {
    query;
    selected = 0;
  });

  function connectQuick() {
    if (!quick) return;
    openSession(newHost({ host: quick.host, port: quick.port, user: quick.user }));
    query = "";
  }

  function onKey(e: KeyboardEvent) {
    if (e.isComposing) return;
    if (e.key === "ArrowDown" || e.key === "ArrowRight") {
      if (e.key === "ArrowRight" && (e.target as HTMLInputElement).selectionStart !== query.length) return;
      e.preventDefault();
      selected = Math.min(selected + 1, hosts.length - 1);
    } else if (e.key === "ArrowUp" || e.key === "ArrowLeft") {
      if (e.key === "ArrowLeft" && query) return;
      e.preventDefault();
      selected = Math.max(selected - 1, 0);
    } else if (e.key === "Enter") {
      e.preventDefault();
      // 有匹配的主机就连它，否则把输入当作地址直接连
      if (query.trim() && hosts[selected]) openSession(hosts[selected]);
      else connectQuick();
    } else if (e.key === "Escape") {
      if (query) query = "";
      else group = null;
    }
  }

  function edit(h: Host) {
    app.editing = h.from_ssh_config ? { ...h, id: "", from_ssh_config: false } : { ...h };
  }

  function showMenu(e: MouseEvent, h: Host) {
    e.preventDefault();
    const items: MenuItem[] = [
      { label: t("连接"), action: () => openSession(h) },
      "sep",
      { label: h.from_ssh_config ? t("保存为主机…") : t("编辑…"), action: () => edit(h) },
      { label: t("复制一份…"), action: () => (app.editing = { ...h, id: "", name: h.name ? t("{0} 副本", h.name) : "", from_ssh_config: false }) },
      { label: t("复制地址"), action: () => navigator.clipboard.writeText(`${h.user}@${h.host}${h.port === 22 ? "" : `:${h.port}`}`) },
    ];
    if (!h.from_ssh_config) {
      // 移到别的分组
      const targets = app.groups.filter((g) => g !== h.group);
      if (targets.length || h.group) items.push("sep");
      if (h.group) items.push({ label: t("移出分组"), action: () => moveToGroup(h, "") });
      for (const g of targets.slice(0, 12)) items.push({ label: t("移到“{0}”", g), action: () => moveToGroup(h, g) });
      items.push({
        label: t("移到新分组…"),
        action: async () => {
          const g = await createGroup();
          if (g) moveToGroup(h, g);
        },
      });
      items.push("sep", { label: t("删除"), danger: true, action: () => deleteHost(h) });
    }
    menu = { x: e.clientX, y: e.clientY, items };
  }

  async function doRename(name: string) {
    const n = await renameGroup(name);
    if (n && group === name) group = n;
  }

  async function doDeleteGroup(name: string) {
    if ((await deleteGroup(name)) && group === name) group = null;
  }

  function showGroupMenu(e: MouseEvent, name: string) {
    e.preventDefault();
    const own = name !== SSH_CONFIG;
    const items: MenuItem[] = [{ label: t("打开"), action: () => (group = name) }];
    if (own) {
      items.push(
        { label: t("在此分组中新建主机…"), action: () => (app.editing = newHost({ group: name })) },
        "sep",
        { label: t("重命名分组…"), action: () => doRename(name) },
        { label: t("删除分组…"), danger: true, action: () => doDeleteGroup(name) },
      );
    }
    menu = { x: e.clientX, y: e.clientY, items };
  }

  async function newGroup() {
    const g = await createGroup();
    if (g) group = g;
  }

  function subtitle(h: Host) {
    const parts = [`${h.user}@${h.host}${h.port !== 22 ? `:${h.port}` : ""}`];
    if (h.os) parts.push(h.os);
    return parts.join(" · ");
  }
</script>

<div class="page">
  <div class="top">
    <div class="search">
      <Search size={16} />
      <!-- svelte-ignore a11y_autofocus -->
      <input
        bind:this={inputEl}
        bind:value={query}
        onkeydown={onKey}
        placeholder={t("搜索主机，或输入 ssh user@host:port 直接连接")}
        spellcheck="false"
        autocomplete="off"
        autofocus
      />
      {#if quick}
        <button class="primary go" onclick={connectQuick}>{t("连接")}</button>
      {/if}
    </div>
  </div>
  <div class="actions">
    <button class="primary" onclick={() => (app.editing = newHost({ group: group && group !== SSH_CONFIG ? group : "" }))}>
      <Plus size={15} /> {t("新建主机")}
    </button>
    {#if group === null}
      <button onclick={newGroup}><FolderPlus size={15} /> {t("新建分组")}</button>
    {/if}
    <button onclick={openLocal} title={isMac ? "⌘L" : "Ctrl+Shift+L"}><Terminal size={15} /> {t("本地终端")}</button>
    <span class="fill"></span>
    <div class="seg">
      <button class:on={settings.hostView === "grid"} onclick={() => setHostView("grid")} title={t("卡片")}><LayoutGrid size={15} /></button>
      <button class:on={settings.hostView === "list"} onclick={() => setHostView("list")} title={t("列表")}><Rows3 size={15} /></button>
    </div>
  </div>

  <div class="scroll">
    {#if group !== null && !query.trim()}
      <div class="crumbs">
        <button onclick={() => (group = null)}>{t("主机")}</button>
        <ChevronRight size={14} />
        <span>{group}</span>
        {#if group !== SSH_CONFIG}
          {@const g = group}
          <button class="crumb-act" title={t("重命名分组")} onclick={() => doRename(g)}><Pencil size={14} /></button>
          <button class="crumb-act danger" title={t("删除分组")} onclick={() => doDeleteGroup(g)}><Trash2 size={14} /></button>
        {/if}
      </div>
    {/if}

    {#if groups.length}
      <h3>{t("分组")}</h3>
      <div class="grid {settings.hostView}">
        {#each groups as [name, count] (name)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div class="card" role="button" tabindex="-1" onclick={() => (group = name)} oncontextmenu={(e) => showGroupMenu(e, name)}>
            <HostIcon kind="group" />
            <span class="text">
              <span class="title">{name}</span>
              <span class="sub">{count ? t("{0} 台主机", count) : t("空分组")}</span>
            </span>
            {#if name !== SSH_CONFIG}
              <button
                class="edit"
                title={t("重命名分组")}
                onclick={(e) => {
                  e.stopPropagation();
                  doRename(name);
                }}><Pencil size={14} /></button
              >
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    {#if hosts.length}
      <h3>{query.trim() ? t("搜索结果") : t("主机")}</h3>
      <div class="grid {settings.hostView}">
        {#each hosts as h, i (h.id || h.host)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            class="card"
            class:sel={i === selected && query.trim() !== ""}
            class:editing={app.editing?.id === h.id && h.id !== ""}
            role="button"
            tabindex="-1"
            title={t("双击连接")}
            ondblclick={() => openSession(h)}
            onclick={() => (selected = i)}
            oncontextmenu={(e) => showMenu(e, h)}
          >
            <HostIcon host={h.host} os={h.os} />
            <span class="text">
              <span class="title">{hostLabel(h)}</span>
              <span class="sub">{subtitle(h)}</span>
            </span>
            <button
              class="edit"
              title={h.from_ssh_config ? t("保存为主机") : t("编辑")}
              onclick={(e) => {
                e.stopPropagation();
                edit(h);
              }}><Pencil size={14} /></button
            >
          </div>
        {/each}
      </div>
    {:else if group !== null && !query.trim()}
      <div class="empty">
        <div>{t("这个分组还没有主机")}</div>
        <button class="primary" onclick={() => (app.editing = newHost({ group: group ?? "" }))}><Plus size={15} /> {t("在此分组中新建主机")}</button>
        <div class="hint">{t("也可以在主机上右键，选择“移到”这个分组。")}</div>
      </div>
    {:else if app.hostsLoaded && !groups.length}
      <div class="empty">
        {#if query.trim()}
          {#if quick}
            {t("按回车连接")} <code>{quick.user || "root"}@{quick.host}{quick.port !== 22 ? `:${quick.port}` : ""}</code>
          {:else}
            {t("没有匹配的主机")}
          {/if}
        {:else}
          <div class="big">{t("还没有主机")}</div>
          <div>{t("点“新建主机”添加，或者在上面输入")} <code>root@192.168.1.10</code> {t("回车直接连接。")}</div>
          <div class="hint">{t("~/.ssh/config 里配置的主机会自动出现在这里。")}</div>
        {/if}
      </div>
    {/if}
  </div>
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
{/if}

<style>
  .page {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .top {
    padding: 16px 24px 0;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 44px;
    padding: 0 6px 0 14px;
    border: 1px solid var(--border);
    border-radius: 11px;
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
    font-size: 14px;
  }
  .go {
    height: 32px;
    border-radius: 8px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 24px 4px;
  }
  .actions button {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    border-radius: 8px;
  }
  .fill {
    flex: 1;
  }
  .seg {
    display: flex;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-card);
  }
  .seg button {
    width: 30px;
    height: 26px !important;
    padding: 0;
    justify-content: center;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--text-faint);
  }
  .seg button.on {
    background: var(--bg-hover);
    color: var(--text);
  }
  .scroll {
    flex: 1;
    overflow: auto;
    padding: 0 24px 32px;
  }
  .crumbs {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-top: 12px;
    color: var(--text-faint);
    font-size: 15px;
    font-weight: 600;
  }
  .crumbs button {
    height: auto;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font-size: inherit;
    font-weight: inherit;
  }
  .crumbs span {
    color: var(--text);
  }
  .crumbs .crumb-act {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    margin-left: 2px;
    border-radius: 6px;
    color: var(--text-faint);
  }
  .crumbs .crumb-act:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .crumbs .crumb-act.danger:hover {
    color: var(--del);
  }
  .empty button {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    border-radius: 8px;
  }
  h3 {
    margin: 18px 0 10px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-dim);
  }
  .grid {
    display: grid;
    gap: 10px;
  }
  .grid.grid {
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  }
  .grid.list {
    grid-template-columns: 1fr;
    gap: 4px;
  }
  .card {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    height: 64px;
    padding: 0 14px;
    border: 1px solid transparent;
    border-radius: var(--radius-lg);
    background: var(--bg-card);
    color: var(--text);
    text-align: left;
    cursor: default;
    font: inherit;
  }
  .list .card {
    height: 52px;
  }
  .card:hover {
    background: var(--bg-card-hover);
  }
  .card.sel,
  .card.editing {
    border-color: var(--accent);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
    flex: 1;
  }
  .title {
    font-size: 14px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    color: var(--text-faint);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .edit {
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
  .card:hover .edit {
    visibility: visible;
  }
  .edit:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 80px 20px;
    color: var(--text-dim);
    text-align: center;
    line-height: 1.6;
  }
  .big {
    color: var(--text);
    font-size: 16px;
    font-weight: 600;
  }
  .hint {
    color: var(--text-faint);
    font-size: 12px;
  }
  code {
    padding: 1px 6px;
    border-radius: 5px;
    background: var(--bg-card);
    font-family: var(--mono);
  }
</style>
