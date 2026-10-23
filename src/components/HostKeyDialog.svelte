<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { app, replyHostKey } from "../lib/store.svelte";

  const p = $derived(app.hostKeys[0]);
  const addr = $derived(p.info.port === 22 ? p.info.host : `[${p.info.host}]:${p.info.port}`);

  function onKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Escape") replyHostKey(p, false);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onkeydown={onKey}>
  <div class="dialog" class:danger={p.info.changed} role="dialog" aria-modal="true">
    {#if p.info.changed}
      <div class="title">{t("⚠ 主机指纹变了")}</div>
      <div class="message">
        {t("{0} 的主机指纹和上次记录的不一样。可能是服务器重装了系统，也可能有人在中间拦截你的连接（中间人攻击）。如果不确定，请先向服务器管理员确认。", addr)}
      </div>
    {:else}
      <div class="title">{t("第一次连接这台主机")}</div>
      <div class="message">{t("无法确认 {0} 的身份。确认指纹无误后再继续，指纹会记录到 ~/.ssh/known_hosts，和命令行 ssh 共用。", addr)}</div>
    {/if}
    <div class="fp">
      <div class="alg">{p.info.algorithm}</div>
      <code>{p.info.fingerprint}</code>
    </div>
    <div class="buttons">
      <button onclick={() => replyHostKey(p, false)}>{t("取消")}</button>
      <!-- svelte-ignore a11y_autofocus -->
      <button class={p.info.changed ? "danger-solid" : "primary"} autofocus={!p.info.changed} onclick={() => replyHostKey(p, true)}>
        {p.info.changed ? t("仍然信任并连接") : t("信任并连接")}
      </button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 110;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 15vh;
    background: rgba(0, 0, 0, 0.45);
  }
  .dialog {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 480px;
    max-width: calc(100vw - 32px);
    padding: 18px 20px;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow);
  }
  .dialog.danger {
    border-top: 3px solid var(--del);
  }
  .title {
    font-size: 15px;
    font-weight: 600;
  }
  .danger .title {
    color: var(--del);
  }
  .message {
    color: var(--text-dim);
    line-height: 1.55;
  }
  .fp {
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--input-bg);
  }
  .alg {
    color: var(--text-faint);
    font-size: 12px;
    margin-bottom: 4px;
  }
  code {
    font-family: var(--mono);
    font-size: 12.5px;
    word-break: break-all;
    user-select: text;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
