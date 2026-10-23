// 全局界面状态：主机列表、会话标签页、对话框、传输任务
import { listen } from "@tauri-apps/api/event";
import { t } from "./i18n.svelte";
import {
  api,
  AUTH_FAILED,
  BAD_PASSPHRASE,
  NEED_PASSPHRASE,
  NEED_PASSWORD,
  type AuthPrompt,
  type EditEvent,
  type ForwardInfo,
  type ForwardRule,
  type Host,
  type HostKeyPrompt,
  type Hop,
  type TransferInfo,
} from "./api";
import { hostLabel, uid } from "./util";

export interface DialogSpec {
  title: string;
  message?: string;
  input?: { value?: string; placeholder?: string; password?: boolean };
  checkbox?: { label: string; value?: boolean };
  confirmText?: string;
  danger?: boolean;
  resolve: (r: { input: string; checkbox: boolean } | null) => void;
}

export type TabStatus = "connecting" | "connected" | "disconnected" | "error";

/** 一个会话标签页：一条 SSH 连接，上面有终端和文件面板 */
export class Tab {
  key = uid();
  host: Host;
  status = $state<TabStatus>("connecting");
  error = $state("");
  /** 每次连接成功加一，终端据此重新打开 shell */
  generation = $state(0);
  /** 终端右边打开的面板：文件、命令片段、端口转发，或者都不开 */
  side = $state<"files" | "snippets" | "forwards" | null>("files");
  /** 正在运行的端口转发 */
  forwards = $state<ForwardInfo[]>([]);
  /** 开启失败的端口转发和原因 */
  forwardErrors = $state<{ rule: ForwardRule; error: string }[]>([]);
  /** 断线后正在自动重连 */
  reconnecting = $state(false);
  /** 终端当前目录（来自 OSC 7，服务器没配置时为空） */
  cwd = $state("");
  // 这次运行里输入过的密码，重连时不用再问
  password: string | undefined;
  passphrase: string | undefined;
  saveSecret = false;
  /** 跳板机的密码，按在连接链里的位置 */
  jumpSecrets: Record<number, { password?: string; passphrase?: string; save?: boolean }> = {};

  constructor(host: Host) {
    this.host = host;
  }

  get title() {
    return hostLabel(this.host);
  }
}

export const app = $state({
  /** 顶部标签：主机库、SFTP 双栏、某个会话 */
  view: "hosts" as "hosts" | "sftp" | "session",
  /** 主机库左侧导航选中的页面 */
  page: "hosts" as "hosts" | "known" | "snippets" | "settings",
  /** SFTP 双栏里单独打开的连接（没有终端） */
  sftpConns: [] as Tab[],
  activeKey: "",
  tabs: [] as Tab[],
  hosts: [] as Host[],
  sshConfig: [] as Host[],
  /** 自己建的分组（包括还没有主机的空分组），按显示顺序 */
  groups: [] as string[],
  hostsLoaded: false,
  /** 正在编辑的主机，id 为空表示新建 */
  editing: null as Host | null,
  dialog: null as DialogSpec | null,
  hostKeys: [] as HostKeyPrompt[],
  toasts: [] as { id: number; text: string; kind: "info" | "error" }[],
  transfers: [] as TransferInfo[],
  /** 每个传输任务的速度（字节/秒） */
  speeds: {} as Record<number, number>,
});

/** 用户取消了连接（tab.error 里的特殊值） */
export const CANCELLED = "cancelled";

/** 后端返回的错误代码转成当前语言的文字，其他错误原样显示 */
export function errorText(e: string) {
  const known: Record<string, string> = {
    [CANCELLED]: "已取消",
    [AUTH_FAILED]: "用户名或密码错误",
    [BAD_PASSPHRASE]: "私钥密码不正确",
    [NEED_PASSWORD]: "需要输入密码",
    [NEED_PASSPHRASE]: "需要输入私钥密码",
  };
  return known[e] ? t(known[e]) : e;
}

let toastId = 0;
export function toast(text: string, kind: "info" | "error" = "info") {
  const id = ++toastId;
  app.toasts.push({ id, text, kind });
  setTimeout(() => (app.toasts = app.toasts.filter((t) => t.id !== id)), kind === "error" ? 6000 : 3000);
}

export function ask(spec: Omit<DialogSpec, "resolve">) {
  return new Promise<{ input: string; checkbox: boolean } | null>((resolve) => {
    app.dialog = { ...spec, resolve };
  });
}

export async function confirm(title: string, message: string, confirmText = t("确定"), danger = false) {
  return !!(await ask({ title, message, confirmText, danger }));
}

export function activeTab() {
  return app.view === "session" ? app.tabs.find((t) => t.key === app.activeKey) : undefined;
}

// ---------- 主机 ----------

export async function loadHosts() {
  try {
    const r = await api.hostsLoad();
    app.hosts = r.saved;
    app.groups = mergeGroups(r.groups, r.saved);
    // ~/.ssh/config 里和已保存主机同名的不重复显示
    const names = new Set(r.saved.map((h) => h.name));
    app.sshConfig = r.ssh_config.filter((h) => !names.has(h.name));
  } catch (e) {
    toast(String(e), "error");
  }
  app.hostsLoaded = true;
}

async function saveHosts() {
  try {
    app.groups = mergeGroups(app.groups, app.hosts);
    await api.hostsSave($state.snapshot(app.hosts), $state.snapshot(app.groups));
  } catch (e) {
    toast(String(e), "error");
  }
}

export function newHost(partial: Partial<Host> = {}): Host {
  return {
    id: "",
    name: "",
    group: "",
    host: "",
    port: 22,
    user: "root",
    auth: "auto",
    key_path: "",
    save_password: false,
    last_used: 0,
    os: "",
    jump: "",
    forwards: [],
    ...partial,
  };
}

/** 保存主机；secret 为 undefined 表示不修改钥匙串里的密码 */
export async function upsertHost(h: Host, secret?: string | null) {
  const host = { ...h, from_ssh_config: false };
  if (!host.id) host.id = uid();
  const i = app.hosts.findIndex((x) => x.id === host.id);
  if (i >= 0) app.hosts[i] = host;
  else app.hosts.push(host);
  if (secret !== undefined) {
    try {
      await api.hostSecretSet(host.id, host.save_password ? secret : null);
    } catch (e) {
      toast(String(e), "error");
    }
  }
  await saveHosts();
  app.sshConfig = app.sshConfig.filter((x) => x.name !== host.name);
  return host;
}

/** 已有的分组加上主机身上出现的分组，去重后保持原来的顺序 */
function mergeGroups(groups: string[], hosts: Host[]) {
  const out = [...new Set(groups.filter(Boolean))];
  for (const h of hosts) if (h.group && !out.includes(h.group)) out.push(h.group);
  return out;
}

export const SSH_CONFIG_GROUP = "~/.ssh/config";

function checkGroupName(name: string, except?: string) {
  if (!name) return false;
  if (name === SSH_CONFIG_GROUP || (name !== except && app.groups.includes(name))) {
    toast(t("已经有叫“{0}”的分组了", name), "error");
    return false;
  }
  return true;
}

/** 新建分组，返回分组名；取消时返回 null */
export async function createGroup() {
  const r = await ask({ title: t("新建分组"), input: { placeholder: t("分组名称") }, confirmText: t("创建") });
  const name = r?.input.trim() ?? "";
  if (!checkGroupName(name)) return null;
  app.groups.push(name);
  await saveHosts();
  return name;
}

/** 重命名分组，组里的主机一起改。返回新名字，取消时返回 null */
export async function renameGroup(old: string) {
  const r = await ask({ title: t("重命名分组"), input: { value: old }, confirmText: t("重命名") });
  const name = r?.input.trim() ?? "";
  if (!name || name === old || !checkGroupName(name, old)) return null;
  app.groups = app.groups.map((g) => (g === old ? name : g));
  for (const h of app.hosts) if (h.group === old) h.group = name;
  if (app.editing?.group === old) app.editing.group = name;
  await saveHosts();
  return name;
}

/** 删除分组：只是解散，里面的主机移到“未分组”，不会删除主机 */
export async function deleteGroup(name: string) {
  const n = app.hosts.filter((h) => h.group === name).length;
  const msg = n ? t("分组里的 {0} 台主机会移到“未分组”，主机本身不会被删除。", n) : t("这是一个空分组。");
  if (!(await confirm(t("删除分组“{0}”", name), msg, t("删除分组"), true))) return false;
  app.groups = app.groups.filter((g) => g !== name);
  for (const h of app.hosts) if (h.group === name) h.group = "";
  await saveHosts();
  return true;
}

export async function moveToGroup(h: Host, group: string) {
  const saved = app.hosts.find((x) => x.id === h.id);
  if (!saved) return;
  saved.group = group;
  await saveHosts();
}

export async function deleteHost(h: Host) {
  if (!(await confirm(t("删除主机"), t("确定删除“{0}”吗？", hostLabel(h)), t("删除"), true))) return;
  app.hosts = app.hosts.filter((x) => x.id !== h.id);
  api.hostSecretSet(h.id, null).catch(() => {});
  await saveHosts();
}

function touchHost(h: Host, os: string | null) {
  if (os) h.os = os;
  const saved = app.hosts.find((x) => x.id === h.id);
  if (saved) {
    saved.last_used = Math.floor(Date.now() / 1000);
    if (os) saved.os = os;
    saveHosts();
  }
}

// ---------- 会话 ----------

export function activate(tab: Tab) {
  app.activeKey = tab.key;
  app.view = "session";
}

export function showHosts() {
  app.view = "hosts";
}

export async function openSession(host: Host) {
  let h = { ...host };
  if (!h.user) {
    const r = await ask({ title: t("用户名"), message: t("登录 {0} 用的用户名", h.host), input: { value: "root" } });
    if (!r?.input.trim()) return;
    h.user = r.input.trim();
  }
  const tab = new Tab(h);
  app.tabs.push(tab);
  activate(tab);
  await connectTab(app.tabs[app.tabs.length - 1]);
}

export async function closeTab(tab: Tab) {
  const i = app.tabs.findIndex((t) => t.key === tab.key);
  if (i < 0) return;
  const running = app.transfers.some((t) => t.session === tab.key && t.state === "running");
  if (running && !(await confirm(t("关闭会话"), t("这个会话还有文件正在传输，关闭会取消传输。"), t("关闭"), true))) return;
  app.tabs.splice(i, 1);
  api.disconnect(tab.key).catch(() => {});
  api.editForget(tab.key).catch(() => {});
  if (app.activeKey === tab.key) {
    const next = app.tabs[Math.min(i, app.tabs.length - 1)];
    if (next) activate(next);
    else showHosts();
  }
}

/** 按 id、名字或者 user@host:port 找跳板机 */
function findJump(ref: string): Host | null {
  const all = [...app.hosts, ...app.sshConfig];
  const found = all.find((h) => h.id === ref) ?? all.find((h) => h.name === ref);
  if (found) return { ...found };
  const m = /^(?:([^@\s]+)@)?(\[[^\]]+\]|[^:\s]+)(?::(\d+))?$/.exec(ref);
  if (!m) return null;
  return newHost({ host: m[2].replace(/^\[|\]$/g, ""), user: m[1] ?? "", port: Number(m[3]) || 22 });
}

/** 连接要依次经过的主机：跳板机在前，目标主机在最后。跳板机自己也可以再经过跳板机（和 ssh 的 ProxyJump 一样）。
 *  出错时返回错误文字 */
export function hostChain(host: Host): Host[] | string {
  const chain: Host[] = [];
  const seen = new Set([host.id || `${host.user}@${host.host}:${host.port}`]);
  const visit = (h: Host, depth: number): string | null => {
    for (const ref of (h.jump ?? "").split(",").map((s) => s.trim()).filter(Boolean)) {
      if (depth > 8) return t("跳板机嵌套的层数太多了");
      const j = findJump(ref);
      if (!j) return t("找不到跳板机“{0}”", ref);
      const key = j.id || `${j.user}@${j.host}:${j.port}`;
      if (seen.has(key)) return t("跳板机“{0}”出现了循环", ref);
      seen.add(key);
      // 没写用户名时和目标主机一样
      if (!j.user) j.user = h.user;
      const err = visit(j, depth + 1);
      if (err) return err;
      chain.push(j);
    }
    return null;
  };
  const err = visit(host, 0);
  if (err) return err;
  chain.push(host);
  return chain;
}

const SECRET_CODES = [NEED_PASSWORD, AUTH_FAILED, NEED_PASSPHRASE, BAD_PASSPHRASE];

/** 连接（或重连）。缺密码或密码错误时问用户，问完再试。
 *  auto：断线后的自动重连，失败时不显示错误卡片，留给调用方继续重试 */
export async function connectTab(tab: Tab, auto = false) {
  const h = tab.host;
  const chain = hostChain(h);
  if (typeof chain === "string") {
    tab.status = "error";
    tab.error = chain;
    return;
  }
  // 没输入过密码时后端会用钥匙串里保存的
  tab.status = "connecting";
  tab.error = "";
  const hops: Hop[] = chain.map((c, i) => {
    const last = i === chain.length - 1;
    const sec = last ? { password: tab.password, passphrase: tab.passphrase, save: tab.saveSecret } : (tab.jumpSecrets[i] ?? {});
    return {
      params: {
        host: c.host,
        port: c.port || 22,
        user: c.user,
        auth: c.auth,
        key_path: c.key_path || undefined,
        password: sec.password,
        passphrase: sec.passphrase,
      },
      host_id: c.id || null,
      save_secret: !!sec.save,
    };
  });
  try {
    const os = await api.connect(tab.key, hops);
    tab.saveSecret = false;
    for (const s of Object.values(tab.jumpSecrets)) s.save = false;
    // 连接期间标签页被关掉了：这条连接没人用，断开
    if (!app.tabs.includes(tab) && !app.sftpConns.includes(tab)) {
      api.disconnect(tab.key).catch(() => {});
      return;
    }
    tab.status = "connected";
    tab.generation++;
    touchHost(h, os);
    // 只有会话标签页开端口转发，SFTP 双栏单独的连接不开
    if (app.tabs.includes(tab)) startForwards(tab);
  } catch (e) {
    // 标签页已经关了就不再提示或询问密码
    if (!app.tabs.includes(tab) && !app.sftpConns.includes(tab)) return;
    const msg = String(e);
    // 跳板机的错误是 jump:序号:代码
    const m = /^jump:(\d+):(.+)$/.exec(msg);
    const hop = m ? Number(m[1]) : chain.length - 1;
    const code = m ? m[2] : msg;
    if (SECRET_CODES.includes(code)) {
      const ph = chain[hop];
      const isPass = code === NEED_PASSPHRASE || code === BAD_PASSPHRASE;
      const saved = app.hosts.some((x) => x.id === ph.id);
      const who = `${ph.user}@${ph.host}`;
      const r = await ask({
        title: m ? (isPass ? t("跳板机的私钥密码") : t("跳板机的密码")) : isPass ? t("私钥密码") : t("密码"),
        message:
          code === AUTH_FAILED
            ? t("{0}：用户名或密码错误，请重新输入", who)
            : code === BAD_PASSPHRASE
              ? t("私钥密码不正确，请重新输入")
              : isPass
                ? t("私钥 {0} 有密码保护", ph.key_path)
                : t("{0} 的密码", who),
        input: { password: true },
        checkbox: saved ? { label: t("记住密码（保存在系统钥匙串）"), value: ph.save_password } : undefined,
        confirmText: t("连接"),
      });
      if (!r) {
        tab.status = "error";
        tab.error = CANCELLED;
        return;
      }
      if (m) {
        const s = (tab.jumpSecrets[hop] ??= {});
        if (isPass) s.passphrase = r.input;
        else s.password = r.input;
        s.save = r.checkbox;
      } else {
        if (isPass) tab.passphrase = r.input;
        else tab.password = r.input;
        tab.saveSecret = r.checkbox;
      }
      if (saved && r.checkbox !== ph.save_password) {
        const s = app.hosts.find((x) => x.id === ph.id)!;
        s.save_password = r.checkbox;
        if (!m) h.save_password = r.checkbox;
        if (!r.checkbox) api.hostSecretSet(ph.id, null).catch(() => {});
        saveHosts();
      }
      return connectTab(tab, auto);
    }
    tab.status = auto && msg !== CANCELLED ? "disconnected" : "error";
    tab.error = msg;
  }
}

// ---------- 端口转发 ----------

export function forwardLabel(r: ForwardRule) {
  const bind = `${r.bind_host || "127.0.0.1"}:${r.bind_port}`;
  if (r.kind === "dynamic") return `SOCKS5 ${bind}`;
  return `${bind} → ${r.dest_host}:${r.dest_port}`;
}

/** 连接成功后开启主机配置里的端口转发 */
async function startForwards(tab: Tab) {
  tab.forwards = [];
  tab.forwardErrors = [];
  for (const rule of tab.host.forwards ?? []) await startForward(tab, rule, false);
}

/** 开启一个转发，失败时记下原因（manual：用户手动添加的，失败时直接提示） */
export async function startForward(tab: Tab, rule: ForwardRule, manual = true) {
  try {
    const info = await api.forwardStart(tab.key, $state.snapshot(rule) as ForwardRule);
    tab.forwards.push(info);
    tab.forwardErrors = tab.forwardErrors.filter((f) => f.rule !== rule);
    return true;
  } catch (e) {
    if (manual) toast(String(e), "error");
    else {
      tab.forwardErrors.push({ rule, error: String(e) });
      toast(t("端口转发 {0} 开启失败：{1}", forwardLabel(rule), String(e)), "error");
    }
    return false;
  }
}

export async function stopForward(tab: Tab, id: number) {
  await api.forwardStop(tab.key, id).catch(() => {});
  tab.forwards = tab.forwards.filter((f) => f.id !== id);
}

/** 刷新转发的连接数 */
export async function refreshForwards(tab: Tab) {
  if (tab.status !== "connected") return;
  try {
    tab.forwards = await api.forwardsList(tab.key);
  } catch {
    // 下次再刷新
  }
}

/** 终端里的 shell 结束了（输入了 exit 或连接断开）。lost：连接意外断开，自动重连 */
export function onShellExit(tab: Tab, lost = false) {
  tab.status = "disconnected";
  tab.forwards = [];
  api.disconnect(tab.key).catch(() => {});
  if (lost) autoReconnect(tab);
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** 断线后自动重连，间隔越来越长；用户手动重连或关掉标签页就停止 */
async function autoReconnect(tab: Tab) {
  if (tab.reconnecting) return;
  tab.reconnecting = true;
  try {
    for (const delay of [1000, 3000, 5000, 10000, 20000, 30000]) {
      await sleep(delay);
      if (!app.tabs.includes(tab) || tab.status !== "disconnected") return;
      await connectTab(tab, true);
      if (tab.status !== "disconnected") return;
    }
    // 一直没连上，显示错误，让用户手动重试
    tab.status = "error";
  } finally {
    tab.reconnecting = false;
  }
}

export function showSftp() {
  app.view = "sftp";
}

/** SFTP 双栏里打开一台主机：已有会话就直接用，否则单独建一条连接 */
export async function sftpConnection(host: Host): Promise<Tab | null> {
  const existing =
    app.tabs.find((t) => t.host.id === host.id && t.status === "connected") ??
    app.sftpConns.find((t) => t.host.id === host.id && t.status !== "error");
  if (existing) return existing;
  const tab = new Tab({ ...host });
  tab.side = null;
  app.sftpConns.push(tab);
  await connectTab(tab);
  if (tab.status === "connected") return tab;
  app.sftpConns = app.sftpConns.filter((t) => t !== tab);
  if (tab.error && tab.error !== CANCELLED) toast(t("连接 {0} 失败：{1}", tab.title, errorText(tab.error)), "error");
  return null;
}

/** SFTP 双栏不再用这条连接时断开（会话标签页的连接不动） */
export function releaseSftpConnection(tab: Tab, stillUsed: (t: Tab) => boolean) {
  if (!app.sftpConns.includes(tab) || stillUsed(tab)) return;
  app.sftpConns = app.sftpConns.filter((t) => t !== tab);
  api.disconnect(tab.key).catch(() => {});
  api.editForget(tab.key).catch(() => {});
}

// ---------- 主机指纹 ----------

export function replyHostKey(p: HostKeyPrompt, accept: boolean) {
  app.hostKeys = app.hostKeys.filter((x) => x.id !== p.id);
  api.hostkeyReply(p.id, accept);
}

// ---------- 传输 ----------

let polling = 0;
const lastSample: Record<number, { done: number; t: number }> = {};

async function pollTransfers() {
  let list: TransferInfo[];
  try {
    list = await api.transfersList();
  } catch {
    return;
  }
  const now = performance.now();
  for (const t of list) {
    const prev = lastSample[t.id];
    if (t.state === "running" && prev && now > prev.t) {
      const inst = ((t.done - prev.done) * 1000) / (now - prev.t);
      // 平滑一下，数字不会跳得太厉害
      app.speeds[t.id] = app.speeds[t.id] ? app.speeds[t.id] * 0.6 + inst * 0.4 : inst;
    }
    lastSample[t.id] = { done: t.done, t: now };
  }
  app.transfers = list;
  if (!list.some((t) => t.state === "running")) stopPolling();
}

function stopPolling() {
  if (polling) clearInterval(polling);
  polling = 0;
}

export function watchTransfers() {
  if (!polling) polling = window.setInterval(pollTransfers, 400);
  pollTransfers();
}

export async function clearTransfers() {
  await api.transfersClear();
  pollTransfers();
}

// ---------- 登录时的验证码 ----------

let promptQueue = Promise.resolve();

/** 服务器要求输入验证码之类的。一次只弹一个对话框，同时来的排队 */
function onAuthPrompt(p: AuthPrompt) {
  promptQueue = promptQueue.then(async () => {
    const answers: string[] = [];
    for (const q of p.info.prompts) {
      const r = await ask({
        title: p.info.name || t("{0} 的登录验证", p.info.host),
        message: [p.info.instructions.trim(), q.prompt.trim()].filter(Boolean).join(" "),
        input: { password: !q.echo },
        confirmText: t("确定"),
      });
      if (!r) return api.authPromptReply(p.id, null).catch(() => {});
      answers.push(r.input);
    }
    await api.authPromptReply(p.id, answers).catch(() => {});
  });
}

// ---------- 编辑远程文件 ----------

async function onEdit(e: EditEvent) {
  const name = e.remote.split("/").pop() ?? e.remote;
  if (e.state === "saved") toast(t("已保存到服务器：{0}", name));
  else if (e.state === "failed") toast(t("{0} 上传失败：{1}", name, e.error ?? ""), "error");
  else if (await confirm(t("文件已被修改"), t("服务器上的 {0} 在你编辑期间被修改过，要用你的版本覆盖吗？", e.remote), t("覆盖"), true))
    api.editUpload(e.id).catch((err) => toast(String(err), "error"));
}

export function startListeners() {
  const un = [
    listen<HostKeyPrompt>("hostkey", (e) => app.hostKeys.push(e.payload)),
    listen<AuthPrompt>("auth-prompt", (e) => onAuthPrompt(e.payload)),
    listen<EditEvent>("edit", (e) => onEdit(e.payload)),
    listen<number>("transfer-finished", async () => {
      await pollTransfers();
    }),
  ];
  return () => un.forEach((p) => p.then((f) => f()));
}
