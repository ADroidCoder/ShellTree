<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { Terminal, type ITheme } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebglAddon } from "@xterm/addon-webgl";
  import { Unicode11Addon } from "@xterm/addon-unicode11";
  import { WebLinksAddon } from "@xterm/addon-web-links";
  import { SearchAddon, type ISearchOptions } from "@xterm/addon-search";
  import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import ChevronUp from "@lucide/svelte/icons/chevron-up";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import X from "@lucide/svelte/icons/x";
  import {
    broadcastInput,
    closePane,
    connectTab,
    onShellExit,
    registerTerminal,
    splitFocused,
    toast,
    type Tab,
  } from "../lib/store.svelte";
  import { leaves } from "../lib/panes";
  import { isDark, settings } from "../lib/settings.svelte";
  import { hostLabel, isMac } from "../lib/util";
  import { CommandLine } from "../lib/cmdline";
  import { historyKey, loadHistory, recordCommand, suggest } from "../lib/history";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";

  /** active：标签页正显示着；pane：分屏里的哪一块；focused：输入焦点在这一块 */
  let { tab, active, pane, focused }: { tab: Tab; active: boolean; pane: string; focused: boolean } = $props();

  let el: HTMLDivElement;
  let term: Terminal;
  let fit: FitAddon;
  let search: SearchAddon;
  let shellId = 0;
  /** shell 正在打开、还不知道 id：这期间的输入先攒着（Windows 的 ConPTY 一启动就问光标位置，xterm 的回答不能丢） */
  let opening = false;
  let queued = "";
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let cmdline: CommandLine;
  /** 灰色提示：还没输入的后半截，以及显示的位置 */
  let ghost = $state<{ text: string; x: number; y: number; w: number; h: number } | null>(null);

  /** 用户的输入都走这里：打开了广播输入时发给所有广播中的终端 */
  function send(data: string) {
    // 终端自动回答的查询（光标位置、设备属性、焦点变化等）只发给自己的 shell，不广播
    if (/^\x1b(\[[\d;?>]*[RcnIO]|\]|P)/.test(data) || !broadcastInput(tab, data)) write(data);
  }

  /** 发给这个终端的 shell，顺便跟踪正在输入的命令 */
  function write(data: string) {
    if (!shellId) {
      if (opening) queued += data;
      return;
    }
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

  // 终端里查找（⌘F）
  let finding = $state(false);
  let query = $state("");
  let caseSensitive = $state(false);
  let regex = $state(false);
  let results = $state<{ index: number; count: number }>({ index: -1, count: 0 });
  let findInput = $state<HTMLInputElement>();

  export function find() {
    finding = true;
    // 选中了文字就直接查它
    const sel = term?.getSelection();
    if (sel && !sel.includes("\n")) query = sel;
    queueMicrotask(() => {
      findInput?.focus();
      findInput?.select();
      if (query) findNext(true);
    });
  }

  function searchOptions(incremental = false): ISearchOptions {
    const match = cssVar("--accent");
    return {
      caseSensitive,
      regex,
      incremental,
      decorations: {
        matchBackground: isDark() ? "#5a4a1c" : "#fff2a8",
        matchOverviewRuler: "#d7a700",
        activeMatchBackground: match,
        activeMatchColorOverviewRuler: match,
      },
    };
  }

  function findNext(incremental = false) {
    if (!query) {
      search.clearDecorations();
      results = { index: -1, count: 0 };
      return;
    }
    search.findNext(query, searchOptions(incremental));
  }

  function findPrevious() {
    if (query) search.findPrevious(query, searchOptions());
  }

  function closeFind() {
    finding = false;
    search.clearDecorations();
    results = { index: -1, count: 0 };
    term.focus();
  }

  function onFindKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      if (e.shiftKey) findPrevious();
      else findNext();
    } else if (e.key === "Escape") {
      e.preventDefault();
      closeFind();
    }
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

  let opened = false;

  // 会话日志：重连后接着写同一个文件
  let logPath = $state("");

  function logNote(what: string) {
    const h = tab.host;
    return `# ShellTree ${what} ${h.user}@${h.host}:${h.port}  ${new Date().toLocaleString()}`;
  }

  function stamp() {
    const d = new Date();
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
  }

  async function startLog() {
    const name = `${hostLabel(tab.host).replace(/[\\/:*?"<>|\s]+/g, "_")}-${stamp()}.log`;
    const dir = await api.downloadDir().catch(() => "");
    const path = await saveDialog({
      title: t("保存会话日志"),
      defaultPath: dir ? `${dir}/${name}` : name,
      filters: [{ name: t("日志"), extensions: ["log", "txt"] }],
    });
    if (!path) return;
    try {
      if (shellId) await api.shellLog(shellId, { path, note: logNote(t("会话日志")), append: false });
      logPath = path;
      toast(t("开始记录会话日志"));
    } catch (e) {
      toast(String(e), "error");
    }
  }

  function stopLog() {
    if (shellId) api.shellLog(shellId, null).catch(() => {});
    const path = logPath;
    logPath = "";
    toast(t("已停止记录会话日志"), "info", { label: t("显示文件"), action: () => revealItemInDir(path) });
  }

  async function openShell() {
    if (shellId) api.shellClose(shellId).catch(() => {});
    shellId = 0;
    opening = true;
    queued = "";
    // 重连时保留之前的输出，只退出全屏程序的画面、恢复终端模式
    if (opened) term.write("\x1b[?1049l\x1b[!p\x1b[0m\r\n");
    else term.reset();
    opened = true;
    cmdline.reset();
    ghost = null;
    // 输出可能在 shellOpen 返回之前就到了，那时还不知道 id，先记着
    let id = 0;
    let early = 0;
    const ack = (n: number) => (id ? api.shellAck(id, n) : (early += n));
    try {
      const onData = (data: Uint8Array) => {
        // xterm 处理完这批再告诉后端，后端据此控制发送速度
        term.write(data, () => ack(data.length));
      };
      const onExit = (status: number | null) => {
        if (shellId !== id) return;
        shellId = 0;
        // 分屏里的一块退出了（比如输入了 exit），只关这一块
        if (status !== null && leaves(tab.layout).length > 1) {
          closePane(tab, pane);
          return;
        }
        const msg =
          status === null
            ? t("连接已断开，正在自动重连…（按回车立即重连）")
            : tab.local
              ? t("已退出（退出码 {0}），按回车重新打开", status)
              : t("会话已结束（退出码 {0}），按回车重新连接", status);
        term.write(`\r\n\x1b[2m${msg}\x1b[0m\r\n`);
        onShellExit(tab, status === null);
      };
      const log = logPath ? { path: logPath, note: logNote(t("重新连接")), append: true } : null;
      const r = tab.local
        ? await api.localShellOpen(term.cols, term.rows, log, onData, onExit)
        : await api.shellOpen(tab.key, term.cols, term.rows, log, onData, onExit);
      id = r.id;
      shellId = id;
      if (early) api.shellAck(id, early);
      if (queued) api.shellWrite(id, queued).catch(() => {});
      if (r.log_error) {
        logPath = "";
        toast(r.log_error, "error");
      }
    } catch (e) {
      term.write(`\x1b[31m${e}\x1b[0m\r\n`);
    } finally {
      opening = false;
      queued = "";
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
    // 先读 focused：term 不是响应式的，第一次运行时还没创建，放在条件后面就不会被跟踪
    const f = focused;
    if (active && term) {
      queueMicrotask(() => {
        fit.fit();
        if (f) term.focus();
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
        { label: t("查找"), hint: isMac ? "⌘F" : "Ctrl+Shift+F", action: find },
        "sep",
        { label: t("清屏"), hint: isMac ? "⌘K" : "", action: () => term.clear() },
        "sep",
        { label: t("左右分屏"), hint: isMac ? "⌘D" : "Ctrl+Shift+D", action: () => splitFocused(tab, "row") },
        { label: t("上下分屏"), hint: isMac ? "⇧⌘D" : "Ctrl+Alt+Shift+D", action: () => splitFocused(tab, "col") },
        ...(leaves(tab.layout).length > 1 ? [{ label: t("关闭这块分屏"), action: () => closePane(tab, pane) }] : []),
        { label: tab.broadcast ? t("停止广播输入") : t("广播输入"), hint: isMac ? "⌘B" : "Ctrl+Shift+B", action: () => (tab.broadcast = !tab.broadcast) },
        "sep",
        logPath
          ? { label: t("停止记录日志"), action: stopLog }
          : { label: t("记录会话日志…"), disabled: !shellId, action: startLog },
        { label: tab.local ? t("重新打开") : t("重新连接"), action: () => connectTab(tab) },
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
    search = new SearchAddon();
    term.loadAddon(search);
    search.onDidChangeResults(({ resultIndex, resultCount }) => (results = { index: resultIndex, count: resultCount }));
    term.loadAddon(new WebLinksAddon((e, uri) => (isMac ? e.metaKey : e.ctrlKey) && openUrl(uri)));
    term.open(el);
    // 点到哪一块，输入焦点就在哪一块
    term.textarea?.addEventListener("focus", () => (tab.focused = pane));
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
      if (shellId || opening) send(d);
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
    const unregister = registerTerminal(pane, tab, write);

    let pending = 0;
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(pending);
      pending = requestAnimationFrame(() => {
        if (el.offsetWidth && el.offsetHeight) fit.fit();
      });
    });
    ro.observe(el);

    return () => {
      unregister();
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
  {#if logPath}
    <button class="rec" title={t("正在记录到 {0}，点击停止", logPath)} onclick={stopLog}>
      <span class="dot"></span>{t("记录中")}
    </button>
  {/if}
  {#if finding}
    <div class="find">
      <input
        bind:this={findInput}
        bind:value={query}
        oninput={() => findNext(true)}
        onkeydown={onFindKey}
        placeholder={t("查找")}
        spellcheck="false"
        autocomplete="off"
      />
      <span class="count" class:none={query && !results.count}>
        {#if query}{results.count ? `${results.index + 1}/${results.count}` : t("无结果")}{/if}
      </span>
      <button class="opt" class:on={caseSensitive} title={t("区分大小写")} onclick={() => ((caseSensitive = !caseSensitive), findNext(true))}>Aa</button>
      <button class="opt" class:on={regex} title={t("正则表达式")} onclick={() => ((regex = !regex), findNext(true))}>.*</button>
      <button title={t("上一个（Shift+回车）")} onclick={findPrevious}><ChevronUp size={15} /></button>
      <button title={t("下一个（回车）")} onclick={() => findNext()}><ChevronDown size={15} /></button>
      <button title={t("关闭（Esc）")} onclick={closeFind}><X size={15} /></button>
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
  .rec {
    position: absolute;
    right: 18px;
    bottom: 8px;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 22px;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: 11px;
    background: var(--bg-card);
    color: var(--text-dim);
    font-size: 11.5px;
    opacity: 0.85;
  }
  .rec:hover {
    opacity: 1;
    color: var(--text);
  }
  .rec .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--del);
    animation: blink 1.6s ease-in-out infinite;
  }
  @keyframes blink {
    50% {
      opacity: 0.3;
    }
  }
  .find {
    position: absolute;
    top: 8px;
    right: 18px;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px;
    border-radius: 9px;
    background: var(--bg-card);
    border: 1px solid var(--border);
    box-shadow: var(--shadow);
  }
  .find input {
    width: 190px;
    height: 26px;
    margin-right: 4px;
  }
  .find .count {
    min-width: 52px;
    color: var(--text-faint);
    font-size: 12px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .find .count.none {
    color: var(--del);
  }
  .find button {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--text-dim);
    font-size: 12px;
  }
  .find button:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .find button.on {
    background: var(--accent);
    color: #fff;
  }
  .find .opt {
    font-family: var(--mono);
  }
  .wrap :global(.xterm-viewport) {
    background: transparent !important;
  }
</style>
