// 终端配色。auto 跟着界面的深色 / 浅色走，其余是常见的配色方案，颜色取自各方案官方的终端配置
import type { ITheme } from "@xterm/xterm";

export interface TermTheme {
  id: string;
  name: string;
  dark: boolean;
  colors: ITheme;
}

function ansi(c: string[]) {
  const keys = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
    "brightBlack", "brightRed", "brightGreen", "brightYellow", "brightBlue", "brightMagenta", "brightCyan", "brightWhite",
  ] as const;
  return Object.fromEntries(keys.map((k, i) => [k, c[i]])) as ITheme;
}

function theme(id: string, name: string, dark: boolean, bg: string, fg: string, cursor: string, selection: string, c: string[]): TermTheme {
  return { id, name, dark, colors: { background: bg, foreground: fg, cursor, cursorAccent: bg, selectionBackground: selection, ...ansi(c) } };
}

/** 跟随界面时用的两套：深色是 One Dark 风格，浅色是 GitHub 风格。背景和界面一致 */
export const ONE_DARK = ansi([
  "#3f4451", "#e05561", "#8cc265", "#d18f52", "#4aa5f0", "#c162de", "#42b3c2", "#d7dae0",
  "#4f5666", "#ff616e", "#a5e075", "#f0a45d", "#4dc4ff", "#de73ff", "#4cd1e0", "#e6e6e6",
]);
export const GITHUB_LIGHT = ansi([
  "#24292f", "#cf222e", "#116329", "#4d2d00", "#0969da", "#8250df", "#1b7c83", "#6e7781",
  "#57606a", "#a40e26", "#1a7f37", "#633c01", "#218bff", "#a475f9", "#3192aa", "#8c959f",
]);

export const THEMES: TermTheme[] = [
  theme("dracula", "Dracula", true, "#282a36", "#f8f8f2", "#f8f8f2", "rgba(68, 71, 90, 0.9)", [
    "#21222c", "#ff5555", "#50fa7b", "#f1fa8c", "#bd93f9", "#ff79c6", "#8be9fd", "#f8f8f2",
    "#6272a4", "#ff6e6e", "#69ff94", "#ffffa5", "#d6acff", "#ff92df", "#a4ffff", "#ffffff",
  ]),
  theme("tokyo-night", "Tokyo Night", true, "#1a1b26", "#c0caf5", "#c0caf5", "rgba(51, 70, 124, 0.7)", [
    "#15161e", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#a9b1d6",
    "#414868", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#c0caf5",
  ]),
  theme("catppuccin-mocha", "Catppuccin Mocha", true, "#1e1e2e", "#cdd6f4", "#f5e0dc", "rgba(88, 91, 112, 0.6)", [
    "#45475a", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#f5c2e7", "#94e2d5", "#bac2de",
    "#585b70", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#f5c2e7", "#94e2d5", "#a6adc8",
  ]),
  theme("nord", "Nord", true, "#2e3440", "#d8dee9", "#d8dee9", "rgba(67, 76, 94, 0.9)", [
    "#3b4252", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#88c0d0", "#e5e9f0",
    "#4c566a", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#8fbcbb", "#eceff4",
  ]),
  theme("gruvbox-dark", "Gruvbox Dark", true, "#282828", "#ebdbb2", "#ebdbb2", "rgba(80, 73, 69, 0.9)", [
    "#282828", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#a89984",
    "#928374", "#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#ebdbb2",
  ]),
  theme("monokai", "Monokai", true, "#272822", "#f8f8f2", "#f8f8f0", "rgba(73, 72, 62, 0.9)", [
    "#272822", "#f92672", "#a6e22e", "#f4bf75", "#66d9ef", "#ae81ff", "#a1efe4", "#f8f8f2",
    "#75715e", "#f92672", "#a6e22e", "#f4bf75", "#66d9ef", "#ae81ff", "#a1efe4", "#f9f8f5",
  ]),
  theme("solarized-dark", "Solarized Dark", true, "#002b36", "#839496", "#93a1a1", "rgba(7, 54, 66, 0.9)", [
    "#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5",
    "#586e75", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4", "#93a1a1", "#fdf6e3",
  ]),
  theme("solarized-light", "Solarized Light", false, "#fdf6e3", "#657b83", "#586e75", "rgba(238, 232, 213, 0.9)", [
    "#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5",
    "#002b36", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4", "#93a1a1", "#fdf6e3",
  ]),
  theme("one-light", "One Light", false, "#fafafa", "#383a42", "#526eff", "rgba(229, 229, 230, 0.9)", [
    "#383a42", "#e45649", "#50a14f", "#c18401", "#4078f2", "#a626a4", "#0184bc", "#a0a1a7",
    "#696c77", "#e45649", "#50a14f", "#c18401", "#4078f2", "#a626a4", "#0184bc", "#ffffff",
  ]),
];

export function findTheme(id: string) {
  return THEMES.find((t) => t.id === id);
}
