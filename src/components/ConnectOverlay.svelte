<script lang="ts">
  // 连接中、连接失败时盖在终端区域上的卡片
  import { t } from "../lib/i18n.svelte";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import { app, closeTab, connectTab, errorText, type Tab } from "../lib/store.svelte";
  import { hostLabel } from "../lib/util";
  import HostIcon from "./HostIcon.svelte";

  let { tab }: { tab: Tab } = $props();
</script>

<div class="overlay">
  <div class="card">
    <div class="who">
      <HostIcon host={tab.host.host} os={tab.host.os} size={44} />
      <div>
        <div class="name">{hostLabel(tab.host)}</div>
        <div class="addr">{tab.host.user}@{tab.host.host}:{tab.host.port}</div>
      </div>
    </div>
    {#if tab.status === "connecting"}
      <div class="progress"><span></span></div>
      <div class="row">
        <span class="state">{t("正在连接…")}</span>
        <button onclick={() => closeTab(tab)}>{t("取消")}</button>
      </div>
    {:else}
      <div class="err"><CircleAlert size={16} /> <span>{errorText(tab.error)}</span></div>
      <div class="btns">
        <button onclick={() => closeTab(tab)}>{t("关闭")}</button>
        {#if tab.host.id && app.hosts.some((h) => h.id === tab.host.id)}
          <button
            onclick={() => {
              app.editing = { ...app.hosts.find((h) => h.id === tab.host.id)! };
              app.page = "hosts";
              app.view = "hosts";
            }}>{t("编辑主机")}</button
          >
        {/if}
        <button class="primary" onclick={() => connectTab(tab)}>{t("重试")}</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: absolute;
    inset: 0;
    z-index: 5;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-graph);
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: 420px;
    max-width: calc(100% - 32px);
    padding: 22px;
    border-radius: 14px;
    background: var(--bg-card);
    border: 1px solid var(--border);
    box-shadow: var(--shadow);
  }
  .who {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .name {
    font-size: 16px;
    font-weight: 700;
  }
  .addr {
    margin-top: 3px;
    color: var(--text-faint);
    font-family: var(--mono);
    font-size: 12px;
  }
  .progress {
    height: 4px;
    border-radius: 2px;
    background: var(--border);
    overflow: hidden;
  }
  .progress span {
    display: block;
    width: 35%;
    height: 100%;
    border-radius: 2px;
    background: var(--accent);
    animation: slide 1.1s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(290%);
    }
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .row button {
    height: 30px;
  }
  .state {
    color: var(--text-dim);
  }
  .err {
    display: flex;
    gap: 8px;
    color: var(--del);
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
  }
  .err :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }
  .btns {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .btns button {
    height: 32px;
  }
</style>
