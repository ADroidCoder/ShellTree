<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import X from "@lucide/svelte/icons/x";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Settings from "@lucide/svelte/icons/settings";
  import Copy from "@lucide/svelte/icons/copy";
  import { onMount } from "svelte";
  import type { ForwardKind } from "../lib/api";
  import { app, forwardLabel, refreshForwards, startForward, stopForward, toast, type Tab } from "../lib/store.svelte";

  let { tab }: { tab: Tab } = $props();

  let kind = $state<ForwardKind>("local");
  let bind = $state("");
  let dest = $state("");

  // 定时刷新每个转发的连接数
  onMount(() => {
    refreshForwards(tab);
    const timer = setInterval(() => refreshForwards(tab), 2000);
    return () => clearInterval(timer);
  });

  async function add() {
    const b = /^(?:(\[[^\]]+\]|[^:\s]+):)?(\d+)$/.exec(bind.trim());
    if (!b || Number(b[2]) > 65535) return toast(t("端口转发的监听端口不正确：{0}", bind), "error");
    let destHost = "";
    let destPort = 0;
    if (kind !== "dynamic") {
      const d = /^(\[[^\]]+\]|[^:\s]+):(\d+)$/.exec(dest.trim());
      if (!d || !Number(d[2]) || Number(d[2]) > 65535) return toast(t("端口转发的目标地址不正确，格式是 主机:端口"), "error");
      destHost = d[1].replace(/^\[|\]$/g, "");
      destPort = Number(d[2]);
    }
    const rule = { kind, bind_host: (b[1] ?? "").replace(/^\[|\]$/g, ""), bind_port: Number(b[2]), dest_host: destHost, dest_port: destPort };
    if (await startForward(tab, rule)) {
      bind = "";
      dest = "";
    }
  }

  function editHost() {
    const saved = app.hosts.find((h) => h.id === tab.host.id);
    if (!saved) return;
    app.editing = { ...saved };
    app.page = "hosts";
    app.view = "hosts";
  }

  const kindName = (k: ForwardKind) => (k === "local" ? t("本地") : k === "remote" ? t("远程") : "SOCKS");
  const localAddr = (host: string, port: number) => `${host && host !== "0.0.0.0" ? host : "127.0.0.1"}:${port}`;
</script>

<div class="side">
  <div class="top">
    <span class="title">{t("端口转发")}</span>
    {#if app.hosts.some((h) => h.id === tab.host.id)}
      <button class="icon" title={t("在主机设置里管理（每次连接自动开启）")} onclick={editHost}><Settings size={15} /></button>
    {/if}
  </div>
  <div class="list">
    {#each tab.forwards as f (f.id)}
      <div class="item">
        <div class="row">
          <span class="kind {f.rule.kind}">{kindName(f.rule.kind)}</span>
          <code>{forwardLabel({ ...f.rule, bind_port: f.port })}</code>
          <button class="icon small" title={t("停止")} onclick={() => stopForward(tab, f.id)}><X size={14} /></button>
        </div>
        <div class="meta">
          <span class="dot"></span>
          {f.active ? t("{0} 个连接", f.active) : t("正在监听")}
          {#if f.rule.kind !== "remote"}
            <button class="link" onclick={() => navigator.clipboard.writeText(localAddr(f.rule.bind_host, f.port)).then(() => toast(t("已复制")))}>
              <Copy size={11} />
              {localAddr(f.rule.bind_host, f.port)}
            </button>
          {/if}
        </div>
      </div>
    {/each}
    {#each tab.forwardErrors as f (f)}
      <div class="item failed">
        <div class="row">
          <span class="kind {f.rule.kind}">{kindName(f.rule.kind)}</span>
          <code>{forwardLabel(f.rule)}</code>
          <button class="icon small" title={t("重试")} onclick={() => startForward(tab, f.rule)}><RefreshCw size={13} /></button>
        </div>
        <div class="meta err">{f.error}</div>
      </div>
    {/each}
    {#if !tab.forwards.length && !tab.forwardErrors.length}
      <div class="empty">{t("还没有端口转发")}</div>
    {/if}
  </div>

  <div class="add">
    <div class="seg">
      <button class:on={kind === "local"} onclick={() => (kind = "local")}>{t("本地")}</button>
      <button class:on={kind === "remote"} onclick={() => (kind = "remote")}>{t("远程")}</button>
      <button class:on={kind === "dynamic"} onclick={() => (kind = "dynamic")}>SOCKS</button>
    </div>
    <div class="inputs">
      <input bind:value={bind} placeholder={kind === "remote" ? t("服务器端口") : t("本机端口")} spellcheck="false" onkeydown={(e) => e.key === "Enter" && add()} />
      {#if kind !== "dynamic"}
        <span class="arrow">→</span>
        <input
          class="dest"
          bind:value={dest}
          placeholder={kind === "remote" ? "localhost:3000" : "127.0.0.1:3306"}
          spellcheck="false"
          onkeydown={(e) => e.key === "Enter" && add()}
        />
      {/if}
    </div>
    <button class="primary" onclick={add} disabled={tab.status !== "connected"}>{t("开启")}</button>
    <div class="hint">
      {kind === "local"
        ? t("访问本机端口，相当于在服务器上访问目标地址")
        : kind === "remote"
          ? t("访问服务器上的端口，相当于在本机访问目标地址")
          : t("在本机开一个 SOCKS5 代理，经服务器访问网络")}
    </div>
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
    align-items: center;
    gap: 6px;
    padding: 12px 10px 8px 14px;
  }
  .title {
    flex: 1;
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
    color: var(--text-dim);
  }
  .icon:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .icon.small {
    width: 24px;
    height: 24px;
    flex-shrink: 0;
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
    padding: 9px 10px 9px 12px;
    border-radius: 10px;
    background: var(--bg-card);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  code {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mono);
    font-size: 12px;
  }
  .kind {
    flex-shrink: 0;
    padding: 1px 6px;
    border-radius: 5px;
    background: var(--bg-selected);
    color: var(--accent);
    font-size: 11px;
    font-weight: 600;
  }
  .kind.remote {
    background: rgba(168, 85, 247, 0.16);
    color: #a855f7;
  }
  .kind.dynamic {
    background: rgba(34, 197, 94, 0.16);
    color: #16a34a;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 6px;
    color: var(--text-faint);
    font-size: 11.5px;
  }
  .meta.err {
    color: var(--del);
    word-break: break-all;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #22c55e;
  }
  .link {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 20px;
    margin-left: auto;
    padding: 0 6px;
    border: 0;
    background: none;
    color: var(--text-dim);
    font-family: var(--mono);
    font-size: 11px;
  }
  .link:hover {
    color: var(--accent);
  }
  .empty {
    padding: 40px 10px;
    color: var(--text-faint);
    text-align: center;
  }
  .add {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 10px;
    border-top: 1px solid var(--border);
  }
  .seg {
    display: flex;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--input-bg);
  }
  .seg button {
    flex: 1;
    height: 26px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--text-dim);
    font-size: 12px;
  }
  .seg button.on {
    background: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .inputs {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .inputs input {
    width: 84px;
    min-width: 0;
    font-family: var(--mono);
    font-size: 12px;
  }
  .inputs .dest {
    flex: 1;
  }
  .arrow {
    color: var(--text-faint);
  }
  .add .primary {
    height: 30px;
  }
  .hint {
    color: var(--text-faint);
    font-size: 11.5px;
    line-height: 1.5;
  }
</style>
