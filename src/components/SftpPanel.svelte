<script lang="ts">
  import { untrack } from "svelte";
  import { remoteSource } from "../lib/fs";
  import type { Tab } from "../lib/store.svelte";
  import FileBrowser from "./FileBrowser.svelte";
  import Transfers from "./Transfers.svelte";

  let { tab, active, onRun }: { tab: Tab; active: boolean; onRun: (cmd: string) => void } = $props();

  const source = $derived(remoteSource(tab));
  let fb = $state<ReturnType<typeof FileBrowser>>();

  // 重连后刷新当前目录（第一次由 FileBrowser 自己打开主目录）
  let seen = 0;
  $effect(() => {
    const g = tab.generation;
    if (tab.status === "connected" && g !== seen) {
      const first = seen === 0;
      seen = g;
      if (first) return;
      untrack(() => fb?.refresh());
    }
  });
</script>

<div class="panel">
  <FileBrowser bind:this={fb} {source} {active} {onRun} />
  <Transfers session={tab.key} />
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-side);
  }
</style>
