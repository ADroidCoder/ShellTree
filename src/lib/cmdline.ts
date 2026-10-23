// 识别终端里“正在输入的命令”。
//
// 终端里只有字符流，客户端并不知道 shell 的提示符在哪。做法是：在新的一行输入开始时
// （上一条命令回车之后按下的第一个键），记下光标位置作为起点；之后从屏幕上读起点到光标的文字。
// 读屏幕而不是自己拼按键，所以 ↑ 翻历史、Tab 补全、Ctrl+U 清行这些 shell 自己做的修改都能反映出来；
// 输密码时 shell 不回显，屏幕上读到的是空的，也就不会被当成命令记下来。
import type { IBufferLine, Terminal } from "@xterm/xterm";

type State = { kind: "idle" } | { kind: "track"; row: number; col: number } | { kind: "lost" };

export class CommandLine {
  private state: State = { kind: "idle" };
  private term: Terminal;

  constructor(term: Terminal) {
    this.term = term;
  }

  private get buf() {
    return this.term.buffer.active;
  }

  /** vim、top、less 这类全屏程序用的是备用屏幕，这时不是在输命令 */
  get fullscreen() {
    return this.buf.type === "alternate";
  }

  /**
   * 按键发给 shell 之前调用。按了回车时返回这次执行的命令（可能是空字符串），否则返回 null
   */
  beforeInput(data: string): string | null {
    if (this.fullscreen) {
      this.state = { kind: "idle" };
      return null;
    }
    if (this.state.kind === "idle") {
      this.state = { kind: "track", row: this.buf.baseY + this.buf.cursorY, col: this.buf.cursorX };
    }
    const enter = data.indexOf("\r");
    if (enter >= 0) {
      // 一次粘贴了多行：不知道哪些是命令，这次不记
      const single = enter === data.length - 1;
      const cmd = single && this.state.kind === "track" ? this.read()?.full ?? null : null;
      this.state = { kind: "idle" };
      return cmd;
    }
    // Ctrl+C / Ctrl+D 放弃这一行
    if (data === "\x03" || data === "\x04") this.state = { kind: "idle" };
    // Ctrl+L 清屏后 shell 会重画提示符，起点找不回来了，等下一次回车再开始
    else if (data === "\x0c") this.state = { kind: "lost" };
    return null;
  }

  /** 输出了新内容（比如清屏）后起点可能已经失效 */
  reset() {
    this.state = { kind: "idle" };
  }

  /**
   * 读当前这一行：光标前已经输入的部分、整行内容、光标是否在末尾。
   * 不在输入状态时返回 null
   */
  read(): { typed: string; full: string; atEnd: boolean } | null {
    if (this.state.kind !== "track" || this.fullscreen) return null;
    const { row, col } = this.state;
    const buf = this.buf;
    const cursorRow = buf.baseY + buf.cursorY;
    if (cursorRow < row) return null;
    const first = buf.getLine(row);
    if (!first) return null;

    let typed = "";
    let after = "";
    let r = row;
    let line: IBufferLine | undefined = first;
    while (line) {
      const startCol = r === row ? col : 0;
      if (r < cursorRow) typed += line.translateToString(false, startCol);
      else if (r === cursorRow) {
        typed += line.translateToString(false, startCol, Math.max(startCol, buf.cursorX));
        after += line.translateToString(true, Math.max(startCol, buf.cursorX));
      } else after += line.translateToString(true);
      const next = buf.getLine(r + 1);
      // 长命令会折到下一行（isWrapped），光标所在行之前的行也要接上
      if (!next || (!next.isWrapped && r + 1 > cursorRow)) break;
      line = next;
      r++;
    }
    const full = (typed + after).trimEnd();
    return { typed, full, atEnd: after.trim() === "" };
  }
}
