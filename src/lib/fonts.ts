// 终端字体：从常见的等宽字体里挑出本机装了的
const CANDIDATES = [
  "SF Mono",
  "Menlo",
  "Monaco",
  "Consolas",
  "Cascadia Code",
  "Cascadia Mono",
  "JetBrains Mono",
  "JetBrainsMono Nerd Font",
  "Fira Code",
  "FiraCode Nerd Font",
  "Hack",
  "Hack Nerd Font",
  "MesloLGS NF",
  "Source Code Pro",
  "IBM Plex Mono",
  "Roboto Mono",
  "Ubuntu Mono",
  "DejaVu Sans Mono",
  "Liberation Mono",
  "Noto Sans Mono",
  "Inconsolata",
  "Victor Mono",
  "Iosevka",
  "Maple Mono",
  "Maple Mono NF CN",
  "Sarasa Mono SC",
  "Sarasa Term SC",
  "Courier New",
];

/** 用画布量文字宽度判断字体有没有装：装了的字体和后备字体宽度不一样 */
function installed(font: string, ctx: CanvasRenderingContext2D) {
  const text = "mmmmmmmmmmlli1WW@#";
  return ["monospace", "serif", "sans-serif"].some((fallback) => {
    ctx.font = `72px ${fallback}`;
    const base = ctx.measureText(text).width;
    ctx.font = `72px "${font}", ${fallback}`;
    return ctx.measureText(text).width !== base;
  });
}

let cache: string[] | null = null;

export function installedFonts() {
  if (cache) return cache;
  const ctx = document.createElement("canvas").getContext("2d");
  cache = ctx ? CANDIDATES.filter((f) => installed(f, ctx)) : [];
  return cache;
}

/** 终端用的字体：选的字体在前，装不上时退回默认的等宽字体 */
export function fontStack(font: string, fallback: string) {
  return font ? `"${font.replace(/"/g, "")}", ${fallback}` : fallback;
}
