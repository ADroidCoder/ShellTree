<script lang="ts" module>
  // 常见系统的品牌色；没识别出系统时按主机名算一个稳定的颜色
  const OS: Record<string, { color: string; label: string }> = {
    ubuntu: { color: "#e95420", label: "U" },
    debian: { color: "#c70036", label: "D" },
    centos: { color: "#932279", label: "C" },
    rocky: { color: "#10b981", label: "R" },
    almalinux: { color: "#0f4266", label: "A" },
    rhel: { color: "#ee0000", label: "R" },
    fedora: { color: "#51a2da", label: "F" },
    alpine: { color: "#0d597f", label: "A" },
    arch: { color: "#1793d1", label: "A" },
    opensuse: { color: "#73ba25", label: "S" },
    "opensuse-leap": { color: "#73ba25", label: "S" },
    amzn: { color: "#ff9900", label: "A" },
    openwrt: { color: "#00b5e2", label: "O" },
    kali: { color: "#367bf0", label: "K" },
    darwin: { color: "#8e8e93", label: "" },
    freebsd: { color: "#ab2b28", label: "F" },
    anolis: { color: "#ff6a00", label: "A" },
    openeuler: { color: "#002fa7", label: "E" },
    kylin: { color: "#2a64e0", label: "K" },
    uos: { color: "#0081ff", label: "U" },
  };
  const PALETTE = ["#4d88ff", "#7c5cff", "#16a394", "#e5a50a", "#e0607e", "#3fa34d", "#d9622b", "#2f9fd0"];

  export function hostColor(key: string, os: string) {
    if (OS[os]) return OS[os].color;
    let h = 0;
    for (let i = 0; i < key.length; i++) h = (h * 31 + key.charCodeAt(i)) | 0;
    return PALETTE[Math.abs(h) % PALETTE.length];
  }
</script>

<script lang="ts">
  import Server from "@lucide/svelte/icons/server";
  import Folder from "@lucide/svelte/icons/folder";
  import HardDrive from "@lucide/svelte/icons/hard-drive";

  let {
    host,
    os = "",
    kind = "host",
    size = 36,
  }: { host?: string; os?: string; kind?: "host" | "group" | "local"; size?: number } = $props();

  const color = $derived(kind === "group" ? "var(--accent)" : kind === "local" ? "#6b7385" : hostColor(host ?? "", os));
  const letter = $derived(kind === "host" ? (OS[os]?.label ?? "") : "");
</script>

<span class="icon" style="--c:{color}; width:{size}px; height:{size}px; border-radius:{Math.round(size * 0.28)}px">
  {#if kind === "group"}
    <Folder size={size * 0.5} strokeWidth={2.2} />
  {:else if kind === "local"}
    <HardDrive size={size * 0.5} strokeWidth={2.2} />
  {:else if letter}
    <span class="letter" style="font-size:{size * 0.44}px">{letter}</span>
  {:else}
    <Server size={size * 0.5} strokeWidth={2.2} />
  {/if}
</span>

<style>
  .icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    background: var(--c);
    color: #fff;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.12);
  }
  .letter {
    font-weight: 800;
    line-height: 1;
  }
</style>
