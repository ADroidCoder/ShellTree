<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { openPath } from "@tauri-apps/plugin-opener";
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import House from "@lucide/svelte/icons/house";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import Upload from "@lucide/svelte/icons/upload";
  import Folder from "@lucide/svelte/icons/folder";
  import File from "@lucide/svelte/icons/file";
  import Search from "@lucide/svelte/icons/search";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import { onMount, untrack } from "svelte";
  import { api, type Entry } from "../lib/api";
  import { beginDrag, drag, registerPane, type DragPayload } from "../lib/drag.svelte";
  import type { FileSource } from "../lib/fs";
  import { app, ask, confirm, toast, watchTransfers } from "../lib/store.svelte";
  import { formatDate, formatPerm, formatSize, modKey, shellQuote, uid } from "../lib/util";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";

  let {
    source,
    active,
    onRun,
    peer,
  }: {
    source: FileSource;
    active: boolean;
    /** 在终端里执行命令（会话侧栏才有） */
    onRun?: (cmd: string) => void;
    /** 双栏时另一栏当前的位置，用于“传到另一侧” */
    peer?: () => { source: FileSource; dir: string } | null;
  } = $props();

  const ROW = 28;
  const paneId = uid();

  let path = $state("");
  let pathInput = $state("");
  let editingPath = $state(false);
  let entries = $state<Entry[]>([]);
  let loading = $state(false);
  let error = $state("");
  let filter = $state("");
  let selected = $state(new Set<string>());
  let anchor = -1;
  let sortKey = $state<"name" | "size" | "mtime">("name");
  let sortDesc = $state(false);
  let showHidden = $state(true);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  /** 从系统拖文件进来时鼠标下的目标文件夹 */
  let osDrop = $state<string | null>(null);
  let history: string[] = [];

  let listEl = $state<HTMLDivElement>();
  let rootEl = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let viewHeight = $state(400);

  const visible = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    let list = entries.filter((e) => (showHidden || !e.name.startsWith(".")) && (!q || e.name.toLowerCase().includes(q)));
    const dir = sortDesc ? -1 : 1;
    return list.slice().sort((a, b) => {
      if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
      const c =
        sortKey === "size"
          ? a.size - b.size
          : sortKey === "mtime"
            ? a.mtime - b.mtime
            : a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
      return c * dir;
    });
  });

  // 只渲染看得见的行，几万个文件的目录也不卡
  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW) - 10));
  const end = $derived(Math.min(visible.length, Math.ceil((scrollTop + viewHeight) / ROW) + 10));
  const crumbs = $derived(path ? source.crumbs(path) : []);
  const dropTarget = $derived(drag.over?.pane === paneId ? drag.over.dir : osDrop);

  export async function load(p: string, remember = true) {
    loading = true;
    error = "";
    try {
      const list = await source.list(p);
      if (remember && path && path !== p) history.push(path);
      path = p;
      pathInput = p;
      entries = list;
      selected = new Set();
      anchor = -1;
      filter = "";
      if (listEl) listEl.scrollTop = 0;
    } catch (e) {
      if (!path) error = String(e);
      else toast(t("打开 {0} 失败：{1}", p, e), "error");
    } finally {
      loading = false;
    }
  }

  export async function refresh() {
    if (!path) return;
    const keep = new Set(selected);
    await load(path, false);
    selected = new Set([...keep].filter((n) => entries.some((e) => e.name === n)));
  }

  export function currentDir() {
    return path;
  }

  // 第一次打开主目录；换了数据来源（双栏切换主机）时重新开始
  let loadedKey = "";
  $effect(() => {
    const key = source.key;
    untrack(() => {
      if (key === loadedKey) return;
      loadedKey = key;
      path = "";
      entries = [];
      history = [];
      source
        .home()
        .then((h) => load(h))
        .catch((e) => (error = String(e)));
    });
  });

  function selectedEntries() {
    return visible.filter((e) => selected.has(e.name));
  }

  function clickRow(e: MouseEvent, idx: number) {
    const name = visible[idx].name;
    if (e.shiftKey && anchor >= 0) {
      const [a, b] = anchor < idx ? [anchor, idx] : [idx, anchor];
      selected = new Set(visible.slice(a, b + 1).map((x) => x.name));
    } else if (modKey(e)) {
      const s = new Set(selected);
      if (s.has(name)) s.delete(name);
      else s.add(name);
      selected = s;
      anchor = idx;
    } else {
      selected = new Set([name]);
      anchor = idx;
    }
  }

  function openEntry(e: Entry) {
    if (e.is_dir) load(source.join(path, e.name));
    else if (!source.remote) openPath(source.join(path, e.name)).catch((err) => toast(String(err), "error"));
    else {
      const p = peer?.();
      sendTo([e], p && !p.source.remote ? p.dir : undefined);
    }
  }

  function goUp() {
    const up = source.parent(path);
    if (up !== path) load(up);
  }

  function goBack() {
    const p = history.pop();
    if (p) load(p, false);
  }

  // ---------- 传输 ----------

  let downloadDir = "";

  /** 把远程文件下载到本地文件夹（默认是“下载”文件夹） */
  async function sendTo(list: Entry[], localDir?: string, pick = false) {
    if (!list.length || !source.remote) return;
    let dir = localDir ?? (downloadDir || (downloadDir = await api.downloadDir()));
    if (pick) {
      const d = await openDialog({ directory: true, title: t("下载到") });
      if (typeof d !== "string") return;
      dir = d;
    }
    try {
      await api.transferStart(source.key, false, list.map((e) => ({ path: source.join(path, e.name), is_dir: e.is_dir })), dir);
      watchTransfers();
    } catch (e) {
      toast(String(e), "error");
    }
  }

  /** 把本地文件上传到这一栏（远程）的某个文件夹 */
  async function upload(paths: string[], dest = path) {
    if (!paths.length || !source.remote) return;
    if (dest === path) {
      const names = new Set(entries.map((e) => e.name));
      const dup = paths.map((p) => p.split(/[\\/]/).pop()!).filter((n) => names.has(n));
      if (dup.length) {
        const list = dup.slice(0, 5).join("\n") + (dup.length > 5 ? "\n" + t("…等 {0} 个", dup.length) : "");
        if (!(await confirm(t("覆盖文件"), t("{0} 里已经有同名的文件或文件夹：\n{1}\n\n覆盖它们吗？", dest, list), t("覆盖"), true))) return;
      }
    }
    try {
      await api.transferStart(source.key, true, paths.map((p) => ({ path: p })), dest);
      watchTransfers();
    } catch (e) {
      toast(String(e), "error");
    }
  }

  async function pickUpload(directory: boolean) {
    const r = await openDialog({ multiple: true, directory, title: directory ? t("上传文件夹") : t("上传文件") });
    if (r) upload(Array.isArray(r) ? r : [r]);
  }

  /** 别的栏（或本栏）拖过来的文件 */
  async function receive(from: DragPayload, dir: string) {
    const names = from.entries.map((e) => e.name);
    if (from.source.key === source.key) {
      // 同一个来源：移动到文件夹里
      if (!(await confirm(t("移动"), t("把 {0} 移动到 {1}？", names.length > 1 ? t("{0} 项", names.length) : `“${names[0]}”`, dir), t("移动")))) return;
      for (const e of from.entries) {
        try {
          await source.rename(source.join(from.dir, e.name), source.join(dir, e.name));
        } catch (err) {
          toast(t("移动 {0} 失败：{1}", e.name, err), "error");
          break;
        }
      }
      refresh();
      return;
    }
    if (!from.source.remote && source.remote) {
      upload(from.entries.map((e) => from.source.join(from.dir, e.name)), dir);
    } else if (from.source.remote && !source.remote) {
      try {
        await api.transferStart(from.source.key, false, from.entries.map((e) => ({ path: from.source.join(from.dir, e.name), is_dir: e.is_dir })), dir);
        watchTransfers();
      } catch (err) {
        toast(String(err), "error");
      }
    } else {
      toast(from.source.remote ? t("暂不支持两台服务器之间直接传输，可以先下载到本地") : t("本地之间的复制请用系统的文件管理器"), "error");
    }
  }

  // 传输结束后，涉及当前目录的就刷新列表
  let lastFinished = 0;
  $effect(() => {
    const n = app.transfers.filter(
      (t) => t.state !== "running" && t.dest === path && (t.upload ? t.session === source.key : !source.remote),
    ).length;
    if (n > lastFinished) untrack(refresh);
    lastFinished = n;
  });

  // ---------- 文件操作 ----------

  async function mkdir() {
    const r = await ask({ title: t("新建文件夹"), input: { placeholder: t("文件夹名称") }, confirmText: t("创建") });
    const name = r?.input.trim();
    if (!name) return;
    try {
      await source.mkdir(source.join(path, name));
      await refresh();
      selected = new Set([name]);
    } catch (e) {
      toast(String(e), "error");
    }
  }

  async function rename(e: Entry) {
    const r = await ask({ title: t("重命名"), input: { value: e.name }, confirmText: t("重命名") });
    const name = r?.input.trim();
    if (!name || name === e.name) return;
    try {
      await source.rename(source.join(path, e.name), source.join(path, name));
      await refresh();
      selected = new Set([name]);
    } catch (err) {
      toast(String(err), "error");
    }
  }

  async function remove(list: Entry[]) {
    if (!list.length) return;
    const what =
      list.length === 1
        ? t(list[0].is_dir ? "“{0}”文件夹及其中所有内容" : "“{0}”", list[0].name)
        : t("选中的 {0} 项", list.length);
    if (!(await confirm(t("删除"), t("确定要删除{0}吗？此操作无法撤销。", what), t("删除"), true))) return;
    for (const e of list) {
      try {
        await source.remove(source.join(path, e.name), e.is_dir && !e.is_link);
      } catch (err) {
        toast(t("删除 {0} 失败：{1}", e.name, err), "error");
        break;
      }
    }
    refresh();
  }

  async function chmod(e: Entry) {
    if (!source.chmod) return;
    const r = await ask({ title: t("修改权限"), message: e.name, input: { value: (e.perm & 0o777).toString(8) }, confirmText: t("修改") });
    const v = r?.input.trim();
    if (!v) return;
    if (!/^[0-7]{3,4}$/.test(v)) return toast(t("请输入八进制权限，例如 755"), "error");
    try {
      await source.chmod(source.join(path, e.name), parseInt(v, 8));
      refresh();
    } catch (err) {
      toast(String(err), "error");
    }
  }

  function showMenu(ev: MouseEvent, e?: Entry) {
    ev.preventDefault();
    ev.stopPropagation();
    if (e && !selected.has(e.name)) selected = new Set([e.name]);
    const sel = e ? selectedEntries() : [];
    const one = sel.length === 1 ? sel[0] : null;
    const other = peer?.();
    const items: MenuItem[] = [];
    if (sel.length) {
      items.push({ label: one?.is_dir ? t("打开") : source.remote ? t("下载") : t("用默认程序打开"), action: () => openEntry(one ?? sel[0]) });
      if (other && other.source.remote !== source.remote) {
        items.push({
          label: source.remote ? t("下载到左侧文件夹") : t("上传到右侧文件夹"),
          hint: other.dir,
          action: () => receiveInto(other, sel),
        });
      }
      if (source.remote) {
        if (!one || one.is_dir) items.push({ label: t("下载"), action: () => sendTo(sel) });
        items.push({ label: t("下载到…"), action: () => sendTo(sel, undefined, true) });
      }
      items.push("sep");
      if (one) {
        items.push({ label: t("重命名…"), hint: "F2", action: () => rename(one) });
        if (source.chmod) items.push({ label: t("修改权限…"), hint: formatPerm(one.perm), action: () => chmod(one) });
        items.push({ label: t("复制路径"), action: () => navigator.clipboard.writeText(source.join(path, one.name)) });
        if (one.is_dir && onRun) items.push({ label: t("在终端中打开"), action: () => onRun(`cd ${shellQuote(source.join(path, one.name))}`) });
        items.push("sep");
      }
      items.push({ label: sel.length > 1 ? t("删除 {0} 项", sel.length) : t("删除"), danger: true, action: () => remove(sel) });
    } else {
      if (source.remote) {
        items.push({ label: t("上传文件…"), action: () => pickUpload(false) }, { label: t("上传文件夹…"), action: () => pickUpload(true) });
      }
      items.push(
        { label: t("新建文件夹…"), action: mkdir },
        "sep",
        { label: t("刷新"), action: () => refresh() },
        { label: showHidden ? t("不显示隐藏文件") : t("显示隐藏文件"), action: () => (showHidden = !showHidden) },
        { label: t("复制当前路径"), action: () => navigator.clipboard.writeText(path) },
      );
      if (!source.remote) items.push({ label: t("在系统文件管理器中打开"), action: () => openPath(path) });
      if (onRun) items.push({ label: t("在终端中打开"), action: () => onRun(`cd ${shellQuote(path)}`) });
    }
    menu = { x: ev.clientX, y: ev.clientY, items };
  }

  /** 通过右键菜单把选中的传到另一栏 */
  function receiveInto(other: { source: FileSource; dir: string }, list: Entry[]) {
    const payload: DragPayload = { pane: paneId, source, dir: path, entries: list };
    if (!source.remote && other.source.remote) {
      api
        .transferStart(other.source.key, true, list.map((e) => ({ path: source.join(path, e.name) })), other.dir)
        .then(watchTransfers)
        .catch((e) => toast(String(e), "error"));
    } else if (source.remote && !other.source.remote) {
      sendTo(payload.entries, other.dir);
    }
  }

  function onKey(e: KeyboardEvent) {
    if ((e.target as HTMLElement).tagName === "INPUT") return;
    const sel = selectedEntries();
    if (e.key === "Enter" && sel.length === 1) openEntry(sel[0]);
    else if ((e.key === "Delete" || (e.key === "Backspace" && modKey(e))) && sel.length) remove(sel);
    else if (e.key === "Backspace") goUp();
    else if (e.key === "F2" && sel.length === 1) rename(sel[0]);
    else if (modKey(e) && e.key.toLowerCase() === "a") {
      selected = new Set(visible.map((x) => x.name));
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      const next = Math.max(0, Math.min(visible.length - 1, anchor + (e.key === "ArrowDown" ? 1 : -1)));
      if (!visible[next]) return;
      selected = new Set([visible[next].name]);
      anchor = next;
      // 让选中的行保持在可视范围内
      if (listEl) {
        const top = next * ROW;
        if (top < listEl.scrollTop) listEl.scrollTop = top;
        else if (top + ROW > listEl.scrollTop + listEl.clientHeight) listEl.scrollTop = top + ROW - listEl.clientHeight;
      }
    } else return;
    e.preventDefault();
    e.stopPropagation();
  }

  function setSort(k: typeof sortKey) {
    if (sortKey === k) sortDesc = !sortDesc;
    else {
      sortKey = k;
      sortDesc = k !== "name";
    }
  }

  function rowPointerDown(e: PointerEvent, entry: Entry) {
    beginDrag(e, () => {
      if (!selected.has(entry.name)) selected = new Set([entry.name]);
      const list = selectedEntries();
      return list.length ? { pane: paneId, source, dir: path, entries: list } : null;
    });
  }

  onMount(() => {
    const unreg = registerPane(paneId, { receive, dir: () => path, join: (d, n) => source.join(d, n) });
    // 从系统拖文件进来上传（只有远程栏接收）
    const un = getCurrentWebview().onDragDropEvent((ev) => {
      if (!active || !path || !source.remote) return;
      const p = ev.payload;
      if (p.type === "leave") {
        osDrop = null;
        return;
      }
      const scale = window.devicePixelRatio || 1;
      const x = p.position.x / scale;
      const y = p.position.y / scale;
      const r = rootEl?.getBoundingClientRect();
      let target: string | null = null;
      if (r && x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) {
        const sub = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-dir]")?.dataset.dir;
        target = sub ? source.join(path, sub) : path;
      }
      if (p.type === "over" || p.type === "enter") osDrop = target;
      else if (p.type === "drop") {
        osDrop = null;
        if (target) upload(p.paths, target);
      }
    });
    return () => {
      unreg();
      un.then((f) => f());
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="fb" bind:this={rootEl} onkeydown={onKey} data-pane={paneId} class:drop={dropTarget === path && !!path}>
  <div class="bar">
    <button class="ib" title={t("返回")} onclick={goBack}><ArrowLeft size={15} /></button>
    <button class="ib" title={t("上一级（Backspace）")} onclick={goUp}><ArrowUp size={15} /></button>
    <button class="ib" title={t("主目录")} onclick={() => source.home().then((h) => load(h))}><House size={15} /></button>
    {#if editingPath}
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="path"
        bind:value={pathInput}
        spellcheck="false"
        autofocus
        onkeydown={(e) => {
          if (e.key === "Enter" && !e.isComposing) {
            editingPath = false;
            load(pathInput.trim() || "/");
          }
          if (e.key === "Escape") editingPath = false;
        }}
        onblur={() => (editingPath = false)}
      />
    {:else}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="crumbs" role="button" tabindex="-1" title={t("点击空白处输入路径")} onclick={(e) => e.target === e.currentTarget && ((pathInput = path), (editingPath = true))}>
        {#each crumbs as c, i (c.path)}
          {#if i > 0 && !/[\\/]$/.test(crumbs[i - 1].name)}<span class="sep">/</span>{/if}
          <button class="crumb" onclick={() => load(c.path)}>{c.name}</button>
        {/each}
      </div>
    {/if}
    <button class="ib" title={t("刷新")} onclick={() => refresh()}><RefreshCw size={14} /></button>
  </div>
  <div class="bar sub">
    <div class="filter">
      <Search size={13} />
      <input bind:value={filter} placeholder={t("筛选")} spellcheck="false" />
    </div>
    <span class="fill"></span>
    {#if onRun}
      <button class="ib" title={t("在终端中打开当前目录")} onclick={() => onRun(`cd ${shellQuote(path)}`)}><SquareTerminal size={15} /></button>
    {/if}
    <button class="ib" title={t("新建文件夹")} onclick={mkdir}><FolderPlus size={15} /></button>
    {#if source.remote}
      <button class="up" title={t("上传文件（也可以直接把文件拖进来）")} onclick={() => pickUpload(false)}><Upload size={14} /> {t("上传")}</button>
    {/if}
  </div>
  <div class="head">
    <button class="c-name" onclick={() => setSort("name")}>{t("名称")} {sortKey === "name" ? (sortDesc ? "↓" : "↑") : ""}</button>
    <button class="c-size" onclick={() => setSort("size")}>{t("大小")} {sortKey === "size" ? (sortDesc ? "↓" : "↑") : ""}</button>
    <button class="c-time" onclick={() => setSort("mtime")}>{t("修改时间")} {sortKey === "mtime" ? (sortDesc ? "↓" : "↑") : ""}</button>
    <span class="c-perm">{t("权限")}</span>
  </div>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="list"
    tabindex="0"
    role="listbox"
    bind:this={listEl}
    bind:clientHeight={viewHeight}
    onscroll={(e) => (scrollTop = (e.target as HTMLElement).scrollTop)}
    oncontextmenu={(e) => showMenu(e)}
    onclick={(e) => e.target === listEl && (selected = new Set())}
  >
    <div class="spacer" style="height:{visible.length * ROW}px">
      {#each visible.slice(start, end) as e, i (e.name)}
        {@const idx = start + i}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="row"
          class:sel={selected.has(e.name)}
          class:target={e.is_dir && dropTarget === source.join(path, e.name)}
          style="top:{idx * ROW}px"
          role="option"
          tabindex="-1"
          aria-selected={selected.has(e.name)}
          data-dir={e.is_dir ? e.name : undefined}
          onpointerdown={(ev) => rowPointerDown(ev, e)}
          onclick={(ev) => clickRow(ev, idx)}
          ondblclick={() => openEntry(e)}
          oncontextmenu={(ev) => showMenu(ev, e)}
        >
          <span class="c-name">
            {#if e.is_dir}<span class="fi dir"><Folder size={15} fill="currentColor" fill-opacity="0.18" /></span>
            {:else}<span class="fi"><File size={15} /></span>{/if}
            <span class="n" class:link={e.is_link}>{e.name}</span>
          </span>
          <span class="c-size">{e.is_dir ? "" : formatSize(e.size)}</span>
          <span class="c-time">{formatDate(e.mtime)}</span>
          <span class="c-perm">{formatPerm(e.perm)}</span>
        </div>
      {/each}
    </div>
    {#if error}
      <div class="msg err">{error}</div>
    {:else if loading && !entries.length}
      <div class="msg">{t("正在加载…")}</div>
    {:else if !loading && path && !visible.length}
      <div class="msg">{filter ? t("没有匹配的文件") : source.remote ? t("空文件夹，可以把文件拖到这里上传") : t("空文件夹")}</div>
    {/if}
  </div>
</div>

{#if drag.from?.pane === paneId}
  <div class="ghost" style="left:{drag.x + 14}px; top:{drag.y + 10}px">
    {drag.from.entries.length > 1 ? t("{0} 项", drag.from.entries.length) : drag.from.entries[0].name}
    {#if drag.over && drag.over.pane !== paneId}<span class="to">→ {drag.over.dir}</span>{/if}
  </div>
{/if}

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
{/if}

<style>
  .fb {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    outline: 2px solid transparent;
    outline-offset: -2px;
    border-radius: inherit;
  }
  .fb.drop {
    outline-color: var(--accent);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 8px 8px 0;
  }
  .bar.sub {
    gap: 4px;
    padding: 6px 8px 8px;
  }
  .ib {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--text-dim);
    flex-shrink: 0;
  }
  .ib:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .up {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 28px;
    padding: 0 10px;
    border-radius: 7px;
    font-size: 12px;
  }
  .crumbs,
  .path {
    flex: 1;
    min-width: 0;
    height: 30px;
    margin: 0 4px;
    border-radius: 7px;
  }
  .crumbs {
    display: flex;
    align-items: center;
    padding: 0 6px;
    background: var(--input-bg);
    border: 1px solid var(--border);
    overflow: hidden;
    white-space: nowrap;
    cursor: text;
  }
  .path {
    font-family: var(--mono);
    font-size: 12px;
  }
  .crumb {
    height: 22px;
    padding: 0 3px;
    border: 0;
    background: none;
    color: var(--text-dim);
    font-size: 12px;
    flex-shrink: 0;
  }
  .crumb:last-child {
    color: var(--text);
    font-weight: 600;
  }
  .crumb:hover {
    color: var(--accent);
  }
  .crumbs .sep {
    color: var(--text-faint);
    font-size: 11px;
  }
  .filter {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 150px;
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--input-bg);
    color: var(--text-faint);
  }
  .filter:focus-within {
    border-color: var(--accent);
  }
  .filter input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    background: none;
    font-size: 12px;
  }
  .fill {
    flex: 1;
  }
  .head,
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 72px 120px 78px;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
  }
  .head {
    height: 28px;
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    color: var(--text-faint);
    font-size: 11.5px;
  }
  .head button {
    height: 24px;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font-size: inherit;
    text-align: left;
  }
  .head button:hover {
    color: var(--text);
  }
  .list {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: auto;
    outline: none;
  }
  .spacer {
    position: relative;
  }
  .row {
    position: absolute;
    left: 4px;
    right: 4px;
    height: 28px;
    padding: 0 8px;
    border-radius: 6px;
    cursor: default;
  }
  .row:hover {
    background: var(--bg-hover);
  }
  .row.sel {
    background: var(--bg-selected);
  }
  .row.target {
    background: var(--bg-selected);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .c-name {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .n {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .n.link {
    font-style: italic;
  }
  .fi {
    display: flex;
    color: var(--text-faint);
    flex-shrink: 0;
  }
  .fi.dir {
    color: var(--accent);
  }
  .c-size,
  .c-time,
  .c-perm {
    color: var(--text-dim);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
  }
  .c-size {
    text-align: right;
  }
  .head .c-size {
    text-align: right;
  }
  .c-perm {
    font-family: var(--mono);
    font-size: 11px;
  }
  .msg {
    position: absolute;
    top: 40px;
    left: 0;
    right: 0;
    padding: 0 20px;
    color: var(--text-faint);
    text-align: center;
    pointer-events: none;
  }
  .msg.err {
    color: var(--del);
    user-select: text;
  }
  .ghost {
    position: fixed;
    z-index: 300;
    max-width: 360px;
    padding: 6px 10px;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    font-size: 12px;
    font-weight: 600;
    pointer-events: none;
    box-shadow: var(--shadow);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .to {
    margin-left: 6px;
    font-weight: 400;
    opacity: 0.85;
  }
</style>
