<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { app } from "../lib/store.svelte";

  const d = $derived(app.dialog!);
  let input = $state("");
  let checkbox = $state(false);
  let inputEl = $state<HTMLInputElement>();
  let confirmEl = $state<HTMLButtonElement>();

  $effect(() => {
    input = d.input?.value ?? "";
    checkbox = d.checkbox?.value ?? false;
    // 打开后聚焦输入框，没有输入框时聚焦确认按钮
    queueMicrotask(() => {
      (inputEl ?? confirmEl)?.focus();
      inputEl?.select();
    });
  });

  function close(ok: boolean) {
    const spec = app.dialog;
    app.dialog = null;
    spec?.resolve(ok ? { input, checkbox } : null);
  }

  function onKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Escape") close(false);
    if (e.key === "Enter" && !e.isComposing) {
      e.preventDefault();
      close(true);
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onkeydown={onKey}>
  <div class="dialog" role="dialog" aria-modal="true">
    <div class="title">{d.title}</div>
    {#if d.message}<div class="message">{d.message}</div>{/if}
    {#if d.input}
      <input
        bind:this={inputEl}
        bind:value={input}
        type={d.input.password ? "password" : "text"}
        placeholder={d.input.placeholder}
        spellcheck="false"
        autocomplete="off"
      />
    {/if}
    {#if d.checkbox}
      <label class="check"><input type="checkbox" bind:checked={checkbox} /> {d.checkbox.label}</label>
    {/if}
    <div class="buttons">
      <button onclick={() => close(false)}>{t("取消")}</button>
      <button bind:this={confirmEl} class={d.danger ? "danger-solid" : "primary"} onclick={() => close(true)}>
        {d.confirmText ?? t("确定")}
      </button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 100;
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
    width: 420px;
    max-width: calc(100vw - 32px);
    padding: 18px 20px;
    background: var(--bg-menu);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow);
  }
  .title {
    font-size: 15px;
    font-weight: 600;
  }
  .message {
    color: var(--text-dim);
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
</style>
