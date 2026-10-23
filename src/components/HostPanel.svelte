<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import X from "@lucide/svelte/icons/x";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import User from "@lucide/svelte/icons/user";
  import Lock from "@lucide/svelte/icons/lock";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import Tag from "@lucide/svelte/icons/tag";
  import Layers from "@lucide/svelte/icons/layers";
  import type { Host } from "../lib/api";
  import { app, deleteHost, openSession, toast, upsertHost } from "../lib/store.svelte";
  import HostIcon from "./HostIcon.svelte";

  // 切换编辑别的主机时重新初始化
  let h = $state<Host>({ ...app.editing! });
  let secret = $state("");
  let portText = $state("");
  let addrEl = $state<HTMLInputElement>();
  let editingId = $state("");
  let lastSeen: Host | null = null;

  $effect(() => {
    const e = app.editing;
    if (!e || e === lastSeen) return;
    lastSeen = e;
    h = { ...e };
    secret = "";
    portText = String(e.port || 22);
    editingId = e.id;
    if (!e.id) queueMicrotask(() => addrEl?.focus());
  });

  const isNew = $derived(!editingId);
  const groups = $derived(app.groups);

  async function pickKey() {
    const f = await openDialog({ title: t("选择私钥文件"), multiple: false, directory: false });
    if (typeof f === "string") h.key_path = f;
  }

  async function save(connect: boolean) {
    h.host = h.host.trim();
    h.user = h.user.trim();
    const port = Number(portText);
    if (!h.host) {
      addrEl?.focus();
      return toast(t("请填写主机地址"), "error");
    }
    if (!Number.isInteger(port) || port < 1 || port > 65535) return toast(t("端口不正确"), "error");
    if (h.auth === "key" && !h.key_path.trim()) return toast(t("请选择私钥文件"), "error");
    h.port = port;
    const changed = secret !== "" || !h.save_password;
    const saved = await upsertHost($state.snapshot(h), changed ? secret || null : undefined);
    app.editing = null;
    if (connect) openSession(saved);
  }

  function onKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Escape") app.editing = null;
    if (e.key === "Enter" && !e.isComposing && (e.target as HTMLElement).tagName === "INPUT") {
      e.preventDefault();
      save(false);
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<aside onkeydown={onKey}>
  <header>
    <div>
      <div class="title">{isNew ? t("新建主机") : t("主机详情")}</div>
      <div class="sub">{isNew ? t("保存后出现在主机列表里") : h.os ? t("系统：{0}", h.os) : t("连接后会自动识别系统")}</div>
    </div>
    {#if !isNew}
      <button class="icon danger" title={t("删除主机")} onclick={() => deleteHost(h).then(() => !app.hosts.some((x) => x.id === h.id) && (app.editing = null))}>
        <Trash2 size={16} />
      </button>
    {/if}
    <button class="icon" title={t("关闭（Esc）")} onclick={() => (app.editing = null)}><X size={17} /></button>
  </header>

  <div class="body">
    <section>
      <div class="label">{t("地址")}</div>
      <div class="addr">
        <HostIcon host={h.host} os={h.os} size={40} />
        <input bind:this={addrEl} bind:value={h.host} placeholder={t("IP 或域名")} spellcheck="false" />
      </div>
    </section>

    <section>
      <div class="label">{t("常规")}</div>
      <label class="field">
        <Tag size={15} />
        <input bind:value={h.name} placeholder={t("名称（可选）")} spellcheck="false" />
      </label>
      <label class="field">
        <Layers size={15} />
        <input bind:value={h.group} list="hp-groups" placeholder={t("分组（可选）")} spellcheck="false" />
      </label>
      <datalist id="hp-groups">
        {#each groups as g (g)}<option value={g}></option>{/each}
      </datalist>
    </section>

    <section>
      <div class="inline">
        <span>{t("SSH 端口")}</span>
        <input class="port" bind:value={portText} spellcheck="false" />
      </div>
      <div class="label">{t("认证")}</div>
      <div class="seg">
        <button class:on={h.auth === "auto"} onclick={() => (h.auth = "auto")} title={t("先试 ssh-agent 和 ~/.ssh 里的密钥，不行再用密码")}>{t("自动")}</button>
        <button class:on={h.auth === "password"} onclick={() => (h.auth = "password")}>{t("密码")}</button>
        <button class:on={h.auth === "key"} onclick={() => (h.auth = "key")}>{t("私钥")}</button>
      </div>
      <label class="field">
        <User size={15} />
        <input bind:value={h.user} placeholder={t("用户名")} spellcheck="false" />
      </label>
      {#if h.auth === "key"}
        <div class="field">
          <KeyRound size={15} />
          <input bind:value={h.key_path} placeholder={t("私钥文件，例如 ~/.ssh/id_ed25519")} spellcheck="false" />
          <button class="mini" onclick={pickKey}>{t("选择")}</button>
        </div>
      {/if}
      <label class="field">
        <Lock size={15} />
        <input
          type="password"
          bind:value={secret}
          autocomplete="off"
          placeholder={h.save_password && !isNew
            ? t("已保存，留空不修改")
            : h.auth === "key"
              ? t("私钥密码（没有就留空）")
              : t("密码（留空则连接时询问）")}
        />
      </label>
      <label class="check"><input type="checkbox" bind:checked={h.save_password} /> {t("记住密码（保存在系统钥匙串）")}</label>
    </section>
  </div>

  <footer>
    <button class="ghost" onclick={() => save(false)}>{t("保存")}</button>
    <button class="primary connect" onclick={() => save(true)}>{t("保存并连接")}</button>
  </footer>
</aside>

<style>
  aside {
    display: flex;
    flex-direction: column;
    width: 360px;
    flex-shrink: 0;
    background: var(--bg-side);
    border-left: 1px solid var(--border);
    animation: slide 0.16s ease-out;
  }
  @keyframes slide {
    from {
      transform: translateX(24px);
      opacity: 0;
    }
  }
  header {
    display: flex;
    align-items: flex-start;
    gap: 4px;
    padding: 18px 16px 10px 20px;
  }
  header > div {
    flex: 1;
  }
  .title {
    font-size: 16px;
    font-weight: 700;
  }
  .sub {
    margin-top: 3px;
    color: var(--text-faint);
    font-size: 12px;
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
    color: var(--text-faint);
  }
  .icon:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .icon.danger:hover {
    color: var(--del);
  }
  .body {
    flex: 1;
    overflow: auto;
    padding: 4px 16px 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px;
    border-radius: var(--radius-lg);
    background: var(--bg-card);
  }
  .label {
    color: var(--text-dim);
    font-size: 12.5px;
    font-weight: 600;
  }
  .addr {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .addr input {
    flex: 1;
    min-width: 0;
    height: 40px;
    font-size: 14px;
  }
  .field {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 4px 0 11px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--input-bg);
    color: var(--text-faint);
  }
  .field:focus-within {
    border-color: var(--accent);
  }
  .field input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    background: none;
  }
  .mini {
    height: 26px;
    padding: 0 9px;
    font-size: 12px;
  }
  .inline {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 4px;
    color: var(--text-dim);
    font-size: 12.5px;
    font-weight: 600;
  }
  .port {
    width: 80px;
    height: 32px;
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
    height: 28px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--text-dim);
  }
  .seg button.on {
    background: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--text-dim);
    font-size: 12px;
  }
  footer {
    display: flex;
    gap: 8px;
    padding: 12px 16px 16px;
    border-top: 1px solid var(--border);
  }
  footer button {
    height: 38px;
    border-radius: 9px;
  }
  .ghost {
    width: 90px;
  }
  .connect {
    flex: 1;
    font-size: 14px;
  }
</style>
