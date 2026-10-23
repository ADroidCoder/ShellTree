<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { Terminal, type ITheme } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebglAddon } from "@xterm/addon-webgl";
  import { Unicode11Addon } from "@xterm/addon-unicode11";
  import { WebLinksAddon } from "@xterm/addon-web-links";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import { app, closeTab, connectTab, errorText, onShellExit, type Tab } from "../lib/store.svelte";
  import { isDark, settings } from "../lib/settings.svelte";
  import { hostLabel, isMac } from "../lib/util";
  import { CommandLine } from "../lib/cmdline";
  import { historyKey, loadHistory, recordCommand, suggest } from "../lib/history";
  import HostIcon from "./HostIcon.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";

  let { tab, active }: { tab: Tab; active: boolean } = $props();

  let el: HTMLDivElement;
  let term: Terminal;
  let fit: FitAddon;
  let shellId = 0;
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let cmdline: CommandLine;
  /** 灰色提示：还没输入的后半截，以及显示的位置 */
  let ghost = $state<{ text: string; x: number; y: number; w: number; h: number } | null>(null);

  /** 发给 shell 的输入都走这里，顺便跟踪正在输入的命令 */
  function send(data: string) {
    if (!shellId) return;
    ghost = null;
    const cmd = cmdline.beforeInput(data);
    if (cmd) recordCommand(historyKey(tab.host), cmd);
    api.shellWrite(shellId, data).catch(() => {});
  }

  /** 在终端里执行一条命令（文件面板、命令片段用） */
  export function run(cmd: string) {
    send(cmd + "\r");
    term?.focus();
  }

  /** 把文字放到命令行里，不执行 */
  export function insert(text: string) {
    send(text);
    term?.focus();
  }

  // 输出更新后重新算灰色提示，同一帧里只算一次
  let ghostFrame = 0;
  function updateGhost() {
    cancelAnimationFrame(ghostFrame);
    ghostFrame = requestAnimationFrame(() => {
      ghost = null;
      if (!settings.suggest || !shellId || !term) return;
      const buf = term.buffer.active;
      // 往上翻看历史输出时不提示
      if (buf.viewportY !== buf.baseY) return;
      const line = cmdline.read();
      if (!line || !line.atEnd || !line.typed.trim()) return;
      const rest = suggest(historyKey(tab.host), line.typed);
      if (!rest) return;
      const screen = el.querySelector<HTMLElement>(".xterm-screen");
      if (!screen) return;
      const w = screen.clientWidth / term.cols;
      const h = screen.clientHeight / term.rows;
      const room = term.cols - buf.cursorX;
      if (room <= 0) return;
      ghost = { text: rest, x: screen.offsetLeft + buf.cursorX * w, y: screen.offsetTop + buf.cursorY * h, w: room * w, h };
    });
  }

  function acceptGhost() {
    if (!ghost) return false;
    const text = ghost.text;
    send(text);
    return true;
  }

  export function focus() {
    term?.focus();
  }

  function cssVar(name: string) {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  function theme(): ITheme {
    const dark = isDark();
    const base: ITheme = {
      background: cssVar("--bg-graph"),
      foreground: cssVar("--text"),
      cursor: cssVar("--accent"),
      cursorAccent: cssVar("--bg-graph"),
      selectionBackground: dark ? "rgba(77, 136, 255, 0.35)" : "rgba(47, 111, 235, 0.25)",
    };
    // 深色用 One Dark 风格，浅色用 GitHub 风格
    return dark
      ? {
          ...base,
          black: "#3f4451", red: "#e05561", green: "#8cc265", yellow: "#d18f52",
          blue: "#4aa5f0", magenta: "#c162de", cyan: "#42b3c2", white: "#d7dae0",
          brightBlack: "#4f5666", brightRed: "#ff616e", brightGreen: "#a5e075", brightYellow: "#f0a45d",
          brightBlue: "#4dc4ff", brightMagenta: "#de73ff", brightCyan: "#4cd1e0", brightWhite: "#e6e6e6",
        }
      : {
          ...base,
          black: "#24292f", red: "#cf222e", green: "#116329", yellow: "#4d2d00",
          blue: "#0969da", magenta: "#8250df", cyan: "#1b7c83", white: "#6e7781",
          brightBlack: "#57606a", brightRed: "#a40e26", brightGreen: "#1a7f37", brightYellow: "#633c01",
          brightBlue: "#218bff", brightMagenta: "#a475f9", brightCyan: "#3192aa", brightWhite: "#8c959f",
        };
  }

  async function openShell() {
    if (shellId) api.shellClose(shellId).catch(() => {});
    shellId = 0;
    term.reset();
    cmdline.reset();
    ghost = null;
    // 输出可能在 shellOpen 返回之前就到了，那时还不知道 id，先记着
    let id = 0;
    let early = 0;
    const ack = (n: number) => (id ? api.shellAck(id, n) : (early += n));
    try {
      id = await api.shellOpen(
        tab.key,
        term.cols,
        term.rows,
        (data) => {
          // xterm 处理完这批再告诉后端，后端据此控制发送速度
          term.write(data, () => ack(data.length));
        },
        (status) => {
          if (shellId !== id) return;
          shellId = 0;
          const why = status === null ? t("连接已断开") : t("会话已结束（退出码 {0}）", status);
          term.write(`\r\n\x1b[2m${t("{0}，按回车重新连接", why)}\x1b[0m\r\n`);
          onShellExit(tab);
        },
      );
      shellId = id;
      if (early) api.shellAck(id, early);
    } catch (e) {
      term.write(`\x1b[31m${e}\x1b[0m\r\n`);
    }
  }

  // 每次连接成功打开一个新的 shell
  $effect(() => {
    if (tab.generation > 0 && tab.status === "connected" && term) openShell();
  });

  // 切换主题或字号时更新终端（等样式变量生效后再读）
  $effect(() => {
    isDark();
    const size = settings.fontSize;
    if (!term) return;
    requestAnimationFrame(() => {
      term.options.theme = theme();
      if (term.options.fontSize !== size) {
        term.options.fontSize = size;
        fit.fit();
      }
    });
  });

  $effect(() => {
    if (active && term) {
      queueMicrotask(() => {
        fit.fit();
        term.focus();
      });
    }
  });

  function copySelection() {
    const s = term.getSelection();
    if (s) navigator.clipboard.writeText(s);
  }

  async function paste() {
    const text = await navigator.clipboard.readText().catch(() => "");
    if (text) term.paste(text);
  }

  function showMenu(e: MouseEvent) {
    e.preventDefault();
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: t("复制"), hint: isMac ? "⌘C" : "Ctrl+Shift+C", disabled: !term.hasSelection(), action: copySelection },
        { label: t("粘贴"), hint: isMac ? "⌘V" : "Ctrl+Shift+V", action: paste },
        { label: t("全选"), action: () => term.selectAll() },
        "sep",
        { label: t("清屏"), hint: isMac ? "⌘K" : "", action: () => term.clear() },
        { label: t("重新连接"), action: () => connectTab(tab) },
      ],
    };
  }

  onMount(() => {
    term = new Terminal({
      fontFamily: cssVar("--mono"),
      fontSize: settings.fontSize,
      lineHeight: 1.15,
      cursorBlink: true,
      scrollback: 10000,
      allowProposedApi: true,
      macOptionIsMeta: false,
      theme: theme(),
    });
    fit = new FitAddon();
    term.loadAddon(fit);
    const unicode = new Unicode11Addon();
    term.loadAddon(unicode);
    term.unicode.activeVersion = "11";
    term.loadAddon(new WebLinksAddon((e, uri) => (isMac ? e.metaKey : e.ctrlKey) && openUrl(uri)));
    term.open(el);
    // GPU 渲染，大量输出时也流畅；显卡不支持时退回默认渲染
    try {
      const gl = new WebglAddon();
      gl.onContextLoss(() => gl.dispose());
      term.loadAddon(gl);
    } catch {
      // 用默认渲染
    }
    fit.fit();

    term.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown") return true;
      const k = e.key.toLowerCase();
      // → 或 End 接受灰色提示（和 fish、zsh-autosuggestions 一样）
      if ((e.key === "ArrowRight" || e.key === "End") && !e.shiftKey && !e.altKey && !e.metaKey && !e.ctrlKey && ghost) {
        acceptGhost();
        return false;
      }
      if (isMac) {
        if (e.metaKey && k === "k") {
          term.clear();
          return false;
        }
        // 其他 ⌘ 快捷键交给应用
        if (e.metaKey && k !== "c" && k !== "v" && k !== "a") return false;
        if (e.metaKey && k === "a") {
          term.selectAll();
          return false;
        }
      } else if (e.ctrlKey && e.shiftKey) {
        // Windows 上 Ctrl+C 要留给终端发中断信号，复制粘贴用 Ctrl+Shift
        if (k === "c") {
          copySelection();
          return false;
        }
        if (k === "v") {
          paste();
          return false;
        }
        return false;
      }
      return true;
    });

    cmdline = new CommandLine(term);
    loadHistory();
    term.onData((d) => {
      if (shellId) send(d);
      else if (d === "\r" && tab.status !== "connecting") connectTab(tab);
    });
    term.onWriteParsed(updateGhost);
    term.onCursorMove(updateGhost);
    term.onScroll(updateGhost);
    term.onResize(updateGhost);
    term.onResize(({ cols, rows }) => {
      if (shellId) api.shellResize(shellId, cols, rows).catch(() => {});
    });
    // 服务器配置了 OSC 7 时可以知道当前目录，文件面板可以跟过去
    term.parser.registerOscHandler(7, (data) => {
      try {
        tab.cwd = decodeURIComponent(new URL(data).pathname);
      } catch {
        // 格式不对就忽略
      }
      return true;
    });

    if (tab.generation > 0 && tab.status === "connected") openShell();

    let pending = 0;
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(pending);
      pending = requestAnimationFrame(() => {
        if (el.offsetWidth && el.offsetHeight) fit.fit();
      });
    });
    ro.observe(el);

    return () => {
      ro.disconnect();
      if (shellId) api.shellClose(shellId).catch(() => {});
      term.dispose();
    };
  });
</script>

<div class="wrap" oncontextmenu={showMenu} role="presentation">
  <div class="term" bind:this={el}>
    {#if ghost}
      <span
        class="ghost"
        style="left:{ghost.x}px; top:{ghost.y}px; max-width:{ghost.w}px; height:{ghost.h}px; line-height:{ghost.h}px; font-size:{settings.fontSize}px"
        >{ghost.text}</span
      >
    {/if}
  </div>
  {#if (tab.status === "connecting" && tab.generation === 0) || tab.status === "error"}
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
  {/if}
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
{/if}

<style>
  .wrap {
    position: relative;
    height: 100%;
    overflow: hidden;
    padding: 8px 0 0 12px;
    background: var(--bg-graph);
  }
  .term {
    height: 100%;
    /* xterm 内部有 z-index 到 11 的图层（滚动条、输入框等），关在自己的层里，
       不然会盖住下面的连接状态卡片，导致按钮点不了 */
    position: relative;
    isolation: isolate;
    z-index: 0;
    overflow: hidden;
  }
  .ghost {
    position: absolute;
    z-index: 20;
    overflow: hidden;
    white-space: pre;
    color: var(--text-faint);
    opacity: 0.75;
    font-family: var(--mono);
    pointer-events: none;
  }
  .overlay {
    position: absolute;
    inset: 0;
    z-index: 1;
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
  .wrap :global(.xterm-viewport) {
    background: transparent !important;
  }
</style>
