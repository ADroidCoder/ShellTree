<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { api, type StatsSample } from "../lib/api";
  import type { Tab } from "../lib/store.svelte";
  import { formatSize } from "../lib/util";

  let { tab, active }: { tab: Tab; active: boolean } = $props();

  const INTERVAL = 3000;

  interface View {
    cpu: number | null;
    mem: number;
    memText: string;
    swap: number;
    swapText: string;
    disk: number;
    diskText: string;
    rx: number | null;
    tx: number | null;
    load: number;
  }

  let view = $state<View | null>(null);
  /** 服务器不是 Linux，这次连接里不再查 */
  let unsupported = $state(false);
  let prev: { s: StatsSample; at: number } | null = null;

  const pct = (used: number, total: number) => (total ? (used / total) * 100 : 0);

  function update(s: StatsSample) {
    const now = performance.now();
    const p = prev;
    prev = { s, at: now };
    const dt = p ? (now - p.at) / 1000 : 0;
    // 第一次采样没有上一次可比，CPU 和网速先空着；计数器变小说明服务器重启过
    const ok = p && dt > 0 && s.cpu_total >= p.s.cpu_total && s.net_rx >= p.s.net_rx && s.net_tx >= p.s.net_tx;
    const busy = ok ? s.cpu_total - p.s.cpu_total - (s.cpu_idle - p.s.cpu_idle) : 0;
    const total = ok ? s.cpu_total - p.s.cpu_total : 0;
    view = {
      cpu: ok ? (total ? (busy / total) * 100 : 0) : (view?.cpu ?? null),
      mem: pct(s.mem_used, s.mem_total),
      memText: `${formatSize(s.mem_used)} / ${formatSize(s.mem_total)}`,
      swap: pct(s.swap_used, s.swap_total),
      swapText: s.swap_used ? `${formatSize(s.swap_used)} / ${formatSize(s.swap_total)}` : "",
      disk: pct(s.disk_used, s.disk_total),
      diskText: `${formatSize(s.disk_used)} / ${formatSize(s.disk_total)}`,
      rx: ok ? (s.net_rx - p.s.net_rx) / dt : (view?.rx ?? null),
      tx: ok ? (s.net_tx - p.s.net_tx) / dt : (view?.tx ?? null),
      load: s.load,
    };
  }

  // 每次重新连接后重新开始
  $effect(() => {
    tab.generation;
    unsupported = false;
    view = null;
    prev = null;
  });

  // 只在这个标签页显示着、已连接时查
  $effect(() => {
    if (!active || tab.status !== "connected" || tab.generation === 0 || unsupported) return;
    const session = tab.key;
    let stopped = false;
    let timer = 0;
    const tick = async () => {
      try {
        const s = await api.serverStats(session);
        if (stopped) return;
        if (s) update(s);
        else if (!view) unsupported = true;
      } catch {
        // 连接正在断开，下次再试
      }
      if (!stopped) timer = window.setTimeout(tick, INTERVAL);
    };
    // 隔了一段时间再切回来，用旧的采样算出来的是这段时间的平均值，不准，重新开始
    if (prev && performance.now() - prev.at > INTERVAL * 2) prev = null;
    tick();
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  });

  function speed(n: number | null) {
    return n === null ? "–" : `${formatSize(Math.round(n))}/s`;
  }

  const level = (p: number) => (p >= 90 ? "high" : p >= 75 ? "mid" : "");
</script>

{#if view && !unsupported}
  <div class="bar">
    <span class="item" title={t("CPU 使用率")}>
      <span class="k">CPU</span>
      <span class="meter {level(view.cpu ?? 0)}"><span style="width:{view.cpu ?? 0}%"></span></span>
      <span class="v num">{view.cpu === null ? "–" : `${Math.round(view.cpu)}%`}</span>
    </span>
    <span class="item" title={t("内存")}>
      <span class="k">{t("内存")}</span>
      <span class="meter {level(view.mem)}"><span style="width:{view.mem}%"></span></span>
      <span class="v">{view.memText}</span>
    </span>
    {#if view.swapText}
      <span class="item" title={t("交换分区")}>
        <span class="k">{t("交换")}</span>
        <span class="v">{view.swapText}</span>
      </span>
    {/if}
    <span class="item" title={t("根分区（/）")}>
      <span class="k">{t("磁盘")}</span>
      <span class="meter {level(view.disk)}"><span style="width:{view.disk}%"></span></span>
      <span class="v">{view.diskText}</span>
    </span>
    <span class="item" title={t("网速（所有网卡）")}>
      <span class="v num">↓ {speed(view.rx)}</span>
      <span class="v num">↑ {speed(view.tx)}</span>
    </span>
    <span class="item" title={t("1 分钟平均负载")}>
      <span class="k">{t("负载")}</span>
      <span class="v num">{view.load.toFixed(2)}</span>
    </span>
  </div>
{/if}

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 18px;
    height: 26px;
    flex-shrink: 0;
    padding: 0 12px;
    overflow: hidden;
    white-space: nowrap;
    background: var(--bg-side);
    border-top: 1px solid var(--border);
    color: var(--text-dim);
    font-size: 11.5px;
    user-select: none;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .k {
    color: var(--text-faint);
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
  .meter {
    width: 40px;
    height: 5px;
    border-radius: 3px;
    background: var(--border);
    overflow: hidden;
  }
  .meter span {
    display: block;
    height: 100%;
    border-radius: 3px;
    background: var(--accent);
    transition: width 0.4s;
  }
  .meter.mid span {
    background: var(--warn);
  }
  .meter.high span {
    background: var(--del);
  }
</style>
