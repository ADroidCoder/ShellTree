<script lang="ts">
  import { i18n, setLang, t, type Lang } from "../lib/i18n.svelte";
  import Moon from "@lucide/svelte/icons/moon";
  import Sun from "@lucide/svelte/icons/sun";
  import MonitorCog from "@lucide/svelte/icons/monitor-cog";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { setEditor, setFontSize, setSuggest, setTheme, settings, type Theme } from "../lib/settings.svelte";
  import { clearHistory, historyCount, loadHistory } from "../lib/history";
  import { confirm, toast } from "../lib/store.svelte";
  import { onMount } from "svelte";
  import { checkUpdate, updater } from "../lib/updater.svelte";

  let count = $state(0);
  onMount(() => {
    loadHistory().then(() => (count = historyCount()));
  });

  async function clear() {
    if (!(await confirm(t("清除命令历史"), t("清除所有主机的命令历史？灰色提示会从头开始学习。"), t("清除"), true))) return;
    clearHistory();
    count = 0;
    toast(t("已清除命令历史"));
  }

  const isMac = navigator.userAgent.includes("Mac");

  async function pickEditor() {
    const f = await openDialog({
      title: t("选择编辑器"),
      multiple: false,
      directory: false,
      defaultPath: isMac ? "/Applications" : undefined,
      filters: isMac ? undefined : [{ name: t("程序"), extensions: ["exe", "cmd", "bat"] }],
    });
    if (typeof f === "string") setEditor(f);
  }

  const editorName = $derived(
    settings.editor
      .split(/[\\/]/)
      .pop()
      ?.replace(/\.(app|exe|cmd|bat)$/i, "") ?? "",
  );

  const langs: { id: Lang | null; label: string }[] = [
    { id: null, label: "跟随系统" },
    { id: "zh", label: "简体中文" },
    { id: "en", label: "English" },
  ];

  const themes: { id: Theme; label: string; icon: typeof Moon }[] = [
    { id: "dark", label: "深色", icon: Moon },
    { id: "light", label: "浅色", icon: Sun },
    { id: "system", label: "跟随系统", icon: MonitorCog },
  ];
</script>

<div class="page">
  <h2>{t("设置")}</h2>

  <section>
    <div class="label">{t("外观")}</div>
    <div class="themes">
      {#each themes as th (th.id)}
        <button class="theme" class:on={settings.theme === th.id} onclick={() => setTheme(th.id)}>
          <th.icon size={18} />
          {t(th.label)}
        </button>
      {/each}
    </div>
  </section>

  <section>
    <div class="label">{t("语言")}</div>
    <div class="langs">
      {#each langs as l (l.id)}
        <button class:on={i18n.choice === l.id} onclick={() => setLang(l.id)}>{l.id ? l.label : t(l.label)}</button>
      {/each}
    </div>
  </section>

  <section>
    <div class="label">{t("终端字号")}</div>
    <div class="font">
      <button onclick={() => setFontSize(settings.fontSize - 1)}>−</button>
      <span class="size">{settings.fontSize}</span>
      <button onclick={() => setFontSize(settings.fontSize + 1)}>＋</button>
      <span class="preview" style="font-size:{settings.fontSize}px">root@server:~$ ls -la</span>
    </div>
  </section>

  <section>
    <div class="label">{t("命令提示")}</div>
    <label class="switch">
      <input type="checkbox" checked={settings.suggest} onchange={(e) => setSuggest((e.target as HTMLInputElement).checked)} />
      {t("根据执行过的命令显示灰色提示，按 → 或 End 接受")}
    </label>
    <div class="hist">
      <span>{t("已记录 {0} 条命令，只保存在本机", count)}</span>
      <button onclick={clear} disabled={!count}>{t("清除历史")}</button>
    </div>
  </section>

  <section>
    <div class="label">{t("编辑远程文件")}</div>
    <div class="hist">
      <span>
        {settings.editor ? t("用 {0} 打开", editorName) : isMac ? t("用系统的文本编辑器打开") : t("用记事本打开")}
      </span>
      <span class="btns">
        {#if settings.editor}<button onclick={() => setEditor("")}>{t("恢复默认")}</button>{/if}
        <button onclick={pickEditor}>{t("选择编辑器…")}</button>
      </span>
    </div>
    <div class="note">{t("在文件面板里双击文本文件或右键“编辑”，保存后自动传回服务器。可以选 VS Code、Sublime Text 等")}</div>
  </section>

  <section>
    <div class="label">{t("关于")}</div>
    <div class="hist">
      <span>ShellTree {updater.version}{updater.update ? " · " + t("新版本 {0}", updater.update.version) : ""}</span>
      {#if updater.update}
        <button class="primary" onclick={() => (updater.open = true)}>{t("查看更新")}</button>
      {:else}
        <button onclick={() => checkUpdate(true)} disabled={updater.status === "checking"}>
          {updater.status === "checking" ? t("正在检查…") : t("检查更新")}
        </button>
      {/if}
    </div>
  </section>

  <section>
    <div class="label">{t("快捷键")}</div>
    <div class="keys">
      <span>{t("新建连接（打开主机搜索）")}</span><kbd>⌘T / Ctrl+Shift+T</kbd>
      <span>{t("新建主机")}</span><kbd>⌘N / Ctrl+Shift+N</kbd>
      <span>{t("关闭会话")}</span><kbd>⌘W / Ctrl+Shift+W</kbd>
      <span>{t("显示 / 隐藏文件面板")}</span><kbd>⌘E / Ctrl+Shift+E</kbd>
      <span>{t("显示 / 隐藏命令片段")}</span><kbd>⌘J / Ctrl+Shift+J</kbd>
      <span>{t("接受灰色提示")}</span><kbd>→ / End</kbd>
      <span>{t("切换到第 N 个会话")}</span><kbd>⌘1…9 / Ctrl+Shift+1…9</kbd>
      <span>{t("终端复制 / 粘贴")}</span><kbd>⌘C ⌘V / Ctrl+Shift+C V</kbd>
    </div>
  </section>
</div>

<style>
  .page {
    height: 100%;
    overflow: auto;
    padding: 22px 24px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 760px;
  }
  h2 {
    margin: 0 0 4px;
    font-size: 18px;
  }
  section {
    padding: 16px;
    border-radius: var(--radius-lg);
    background: var(--bg-card);
  }
  .label {
    margin-bottom: 12px;
    color: var(--text-dim);
    font-weight: 600;
  }
  .themes {
    display: flex;
    gap: 10px;
  }
  .theme {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    width: 110px;
    height: 72px;
    border-radius: 10px;
    background: var(--input-bg);
  }
  .theme.on {
    border-color: var(--accent);
    color: var(--accent);
    font-weight: 600;
  }
  .langs {
    display: flex;
    gap: 8px;
  }
  .langs button {
    height: 34px;
    min-width: 110px;
    border-radius: 8px;
    background: var(--input-bg);
  }
  .langs button.on {
    border-color: var(--accent);
    color: var(--accent);
    font-weight: 600;
  }
  .switch {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .hist {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 12px;
    color: var(--text-faint);
    font-size: 12.5px;
  }
  .hist button {
    height: 28px;
  }
  .btns {
    display: flex;
    gap: 6px;
  }
  .note {
    margin-top: 8px;
    color: var(--text-faint);
    font-size: 12px;
  }
  .font {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .font button {
    width: 30px;
    padding: 0;
  }
  .size {
    width: 28px;
    text-align: center;
    font-weight: 600;
  }
  .preview {
    margin-left: 12px;
    padding: 6px 10px;
    border-radius: 6px;
    background: var(--bg-graph);
    font-family: var(--mono);
  }
  .keys {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 8px 16px;
    color: var(--text-dim);
  }
  kbd {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text);
  }
</style>
