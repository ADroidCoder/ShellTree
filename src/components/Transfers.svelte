<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { openPath } from "@tauri-apps/plugin-opener";
  import { api, type TransferInfo } from "../lib/api";
  import { app, clearTransfers } from "../lib/store.svelte";
  import { formatSize } from "../lib/util";

  /** 只显示这个会话的传输；不传则显示全部 */
  let { session }: { session?: string } = $props();

  let collapsed = $state(false);
  const list = $derived(app.transfers.filter((t) => !session || t.session === session).reverse());
  const running = $derived(list.filter((t) => t.state === "running"));
  const totalSpeed = $derived(running.reduce((s, t) => s + (app.speeds[t.id] ?? 0), 0));

  function percent(x: TransferInfo) {
    if (x.state === "done") return 100;
    return x.total ? Math.min(100, (x.done / x.total) * 100) : 0;
  }

  function detail(x: TransferInfo) {
    if (x.state === "failed") return x.error ?? t("失败");
    if (x.state === "cancelled") return t("已取消");
    const files = x.files_total > 1 ? " · " + t("{0}/{1} 个文件", x.files_done, x.files_total) : "";
    if (x.state === "done") {
      const sec = x.elapsed_ms / 1000;
      const avg = sec > 0.05 ? ` · ${formatSize(x.total / sec)}/s` : "";
      return `${formatSize(x.total)}${files}${avg}`;
    }
    const speed = app.speeds[x.id] ?? 0;
    const left = speed > 0 && x.total > x.done ? Math.ceil((x.total - x.done) / speed) : 0;
    const eta = left
      ? " · " + (left >= 60 ? t("剩余 {0} 分 {1} 秒", Math.floor(left / 60), left % 60) : t("剩余 {0} 秒", left))
      : "";
    return `${formatSize(x.done)} / ${formatSize(x.total)} · ${formatSize(speed)}/s${eta}${files}`;
  }

</script>

{#if list.length}
  <div class="transfers">
    <div class="title">
      <button class="toggle" onclick={() => (collapsed = !collapsed)}>
        {collapsed ? "▸" : "▾"} {t("传输")}
        {#if running.length}<span class="badge">{running.length}</span> <span class="speed">{formatSize(totalSpeed)}/s</span>{/if}
      </button>
      <span class="fill"></span>
      {#if list.length > running.length}<button class="link" onclick={clearTransfers}>{t("清除已完成")}</button>{/if}
    </div>
    {#if !collapsed}
      <div class="items">
        {#each list as x (x.id)}
          <div class="item {x.state}">
            <div class="line">
              <span class="dir">{x.upload ? "⬆" : "⬇"}</span>
              <span class="name" title={t(x.upload ? "上传到 {0}" : "下载到 {0}", x.dest)}>{x.name}</span>
              {#if x.state === "running"}
                <button class="link" onclick={() => api.transferCancel(x.id)}>{t("取消")}</button>
              {:else if x.state === "done" && !x.upload}
                <button class="link" onclick={() => openPath(x.dest)}>{t("打开文件夹")}</button>
              {/if}
            </div>
            <div class="bar"><div class="fillbar" style="width:{percent(x)}%"></div></div>
            <div class="detail" title={detail(x)}>{detail(x)}</div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .transfers {
    max-height: 40%;
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--border);
    background: var(--bg-toolbar);
  }
  .title {
    display: flex;
    align-items: center;
    padding: 4px 8px;
  }
  .toggle {
    height: 24px;
    padding: 0 4px;
    border: 0;
    background: none;
    color: var(--text-dim);
    font-size: 12px;
    font-weight: 600;
  }
  .badge {
    padding: 0 6px;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    font-size: 10.5px;
  }
  .speed {
    color: var(--text-faint);
    font-weight: 400;
  }
  .fill {
    flex: 1;
  }
  .link {
    height: 20px;
    padding: 0 4px;
    border: 0;
    background: none;
    color: var(--accent);
    font-size: 12px;
  }
  .items {
    overflow: auto;
    padding: 0 8px 8px;
  }
  .item {
    padding: 6px 4px;
  }
  .item + .item {
    border-top: 1px solid var(--border);
  }
  .line {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .dir {
    color: var(--accent);
    font-size: 11px;
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bar {
    height: 3px;
    margin: 5px 0 4px;
    border-radius: 2px;
    background: var(--border);
    overflow: hidden;
  }
  .fillbar {
    height: 100%;
    background: var(--accent);
    transition: width 0.3s;
  }
  .done .fillbar {
    background: var(--add);
  }
  .failed .fillbar,
  .cancelled .fillbar {
    background: var(--del);
  }
  .detail {
    color: var(--text-faint);
    font-size: 11.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .failed .detail {
    color: var(--del);
    white-space: normal;
    user-select: text;
  }
</style>
