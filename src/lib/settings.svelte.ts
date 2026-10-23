// 外观设置，存在 localStorage 里
import { getCurrentWindow } from "@tauri-apps/api/window";
export type Theme = "dark" | "light" | "system";

function read(key: string, def: string) {
  try {
    return localStorage.getItem(key) ?? def;
  } catch {
    return def;
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // 存不了就只在这次生效
  }
}

export const settings = $state({
  theme: read("shelltree.theme", "dark") as Theme,
  fontSize: Number(read("shelltree.fontSize", "13")) || 13,
  /** 主机页用卡片还是列表 */
  hostView: read("shelltree.hostView", "grid") as "grid" | "list",
  /** 终端里根据历史命令显示灰色提示 */
  suggest: read("shelltree.suggest", "1") === "1",
  /** 系统是不是深色，主题为“跟随系统”时用 */
  systemDark: typeof matchMedia !== "undefined" && matchMedia("(prefers-color-scheme: dark)").matches,
});

export function isDark() {
  return settings.theme === "dark" || (settings.theme === "system" && settings.systemDark);
}

export function setTheme(t: Theme) {
  settings.theme = t;
  write("shelltree.theme", t);
  applyTheme();
}

/** 页面配色和窗口外观一起切换。窗口外观决定 macOS 红绿灯按钮的颜色：
 *  不同步的话，深色界面在窗口失去焦点时红绿灯会变成深灰，几乎看不见 */
export function applyTheme() {
  document.documentElement.dataset.theme = settings.theme;
  getCurrentWindow()
    .setTheme(settings.theme === "system" ? null : settings.theme)
    .catch(() => {});
}

export function setFontSize(n: number) {
  settings.fontSize = Math.max(9, Math.min(28, n));
  write("shelltree.fontSize", String(settings.fontSize));
}

export function setSuggest(on: boolean) {
  settings.suggest = on;
  write("shelltree.suggest", on ? "1" : "0");
}

export function setHostView(v: "grid" | "list") {
  settings.hostView = v;
  write("shelltree.hostView", v);
}

export function watchSystemTheme() {
  const mq = matchMedia("(prefers-color-scheme: dark)");
  const on = () => (settings.systemDark = mq.matches);
  mq.addEventListener("change", on);
  return () => mq.removeEventListener("change", on);
}
