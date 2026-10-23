<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import Search from "@lucide/svelte/icons/search";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import FingerprintPattern from "@lucide/svelte/icons/fingerprint-pattern";
  import Plus from "@lucide/svelte/icons/plus";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import { onMount } from "svelte";
  import { api, type KnownHost } from "../lib/api";
  import { app, confirm, newHost, toast } from "../lib/store.svelte";

  let list = $state<KnownHost[]>([]);
  let query = $state("");
  let loaded = $state(false);

  async function load() {
    try {
      list = await api.knownHostsList();
    } catch (e) {
      toast(String(e), "error");
    }
    loaded = true;
  }

  onMount(load);

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? list.filter((k) => `${k.hosts} ${k.algorithm} ${k.fingerprint}`.toLowerCase().includes(q)) : list;
  });

  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  /** known_hosts 里的主机写法：host、[host]:port，多个名字用逗号分开，取第一个 */
  function parseHost(k: KnownHost): { host: string; port: number } | null {
    if (k.hashed) return null;
    const first = k.hosts.split(",")[0]?.trim();
    if (!first) return null;
    const m = first.match(/^\[(.+)\]:(\d+)$/);
    return m ? { host: m[1], port: Number(m[2]) } : { host: first, port: 22 };
  }

  function savedHost(k: KnownHost) {
    const p = parseHost(k);
    return p && app.hosts.find((h) => h.host === p.host && (h.port || 22) === p.port);
  }

  function addAsHost(k: KnownHost) {
    const p = parseHost(k);
    if (!p) return;
    app.page = "hosts";
    app.editing = newHost({ host: p.host, port: p.port });
  }

  function showMenu(e: MouseEvent, k: KnownHost) {
    e.preventDefault();
    const saved = savedHost(k);
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        saved
          ? { label: t("编辑已保存的主机…"), action: () => ((app.page = "hosts"), (app.editing = { ...saved })) }
          : { label: t("添加为主机…"), disabled: !parseHost(k), hint: k.hashed ? t("主机名已加密") : "", action: () => addAsHost(k) },
        { label: t("复制指纹"), action: () => navigator.clipboard.writeText(k.fingerprint) },
        "sep",
        { label: t("删除"), danger: true, action: () => remove(k) },
      ],
    };
  }

  async function remove(k: KnownHost) {
    const name = k.hosts || t("这条记录");
    if (!(await confirm(t("删除指纹"), t("删除 {0} 的主机指纹？下次连接时会重新询问是否信任。", name), t("删除"), true))) return;
    try {
      await api.knownHostsRemove(k.line, k.raw);
    } catch (e) {
      toast(String(e), "error");
    }
    load();
  }
</script>

<div class="page">
  <div class="head">
    <div>
      <h2>{t("已知主机")}</h2>
      <div class="desc">{t("~/.ssh/known_hosts 里信任过的主机指纹，和命令行 ssh 共用。服务器重装后指纹会变，可以在这里删掉旧的。")}</div>
    </div>
  </div>
  <div class="bar">
    <div class="search">
      <Search size={15} />
      <input bind:value={query} placeholder={t("筛选主机或指纹")} spellcheck="false" />
    </div>
    <button class="icon" title={t("刷新")} onclick={load}><RefreshCw size={15} /></button>
  </div>
  <div class="scroll">
    {#each shown as k (k.line)}
      <div class="row" role="listitem" oncontextmenu={(e) => showMenu(e, k)}>
        <span class="ico"><FingerprintPattern size={18} /></span>
        <span class="text">
          <span class="title">{k.hashed ? t("（主机名已加密）") : k.hosts}</span>
          <span class="sub"><span class="alg">{k.algorithm}</span> {k.fingerprint}</span>
        </span>
        {#if savedHost(k)}
          <span class="saved">{t("已在主机列表")}</span>
        {:else if parseHost(k)}
          <button class="add" title={t("添加为主机")} onclick={() => addAsHost(k)}><Plus size={14} /> {t("添加为主机")}</button>
        {/if}
        <button class="icon del" title={t("删除")} onclick={() => remove(k)}><Trash2 size={15} /></button>
      </div>
    {:else}
      {#if loaded}<div class="empty">{query ? t("没有匹配的记录") : t("还没有信任过任何主机")}</div>{/if}
    {/each}
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
  .search {
    flex: 1;
    max-width: 420px;
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
  .icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    padding: 0;
    border-radius: 8px;
  }
  .scroll {
    flex: 1;
    overflow: auto;
    padding: 0 24px 24px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 56px;
    padding: 8px 12px;
    border-radius: var(--radius-lg);
    background: var(--bg-card);
  }
  .ico {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    border-radius: 9px;
    background: color-mix(in srgb, var(--add) 18%, transparent);
    color: var(--add);
    flex-shrink: 0;
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .title {
    font-weight: 600;
    word-break: break-all;
  }
  .sub {
    color: var(--text-faint);
    font-family: var(--mono);
    font-size: 11.5px;
    word-break: break-all;
    user-select: text;
  }
  .alg {
    color: var(--text-dim);
  }
  .saved {
    color: var(--text-faint);
    font-size: 12px;
    white-space: nowrap;
  }
  .add {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 28px;
    padding: 0 10px;
    border-radius: 7px;
    font-size: 12px;
    white-space: nowrap;
    visibility: hidden;
  }
  .row:hover .add {
    visibility: visible;
  }
  .del {
    width: 30px;
    height: 30px;
    border: 0;
    background: none;
    color: var(--text-faint);
    visibility: hidden;
  }
  .row:hover .del {
    visibility: visible;
  }
  .del:hover {
    color: var(--del);
    background: var(--bg-hover);
  }
  .empty {
    padding: 60px;
    color: var(--text-faint);
    text-align: center;
  }
</style>
