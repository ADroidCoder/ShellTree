// 用无界面的 xterm 模拟 shell 的回显，测试命令行识别。运行：node --test tests/
import { test } from "node:test";
import assert from "node:assert/strict";
import xterm from "@xterm/headless";
import { CommandLine } from "../src/lib/cmdline.ts";

const { Terminal } = xterm;

function setup(cols = 40) {
  const term = new Terminal({ cols, rows: 10, allowProposedApi: true });
  const cl = new CommandLine(term as any);
  const write = (s: string) => new Promise<void>((r) => term.write(s, r));
  /** 模拟用户按键：先给 CommandLine 看，再像 shell 一样回显 */
  const type = async (keys: string, echo = keys) => {
    const cmd = cl.beforeInput(keys);
    await write(echo);
    return cmd;
  };
  return { term, cl, write, type };
}

test("普通输入和回车", async () => {
  const { cl, write, type } = setup();
  await write("root@srv:~# ");
  await type("l");
  await type("s -la");
  assert.deepEqual(cl.read(), { typed: "ls -la", full: "ls -la", atEnd: true });
  const cmd = await type("\r", "\r\n");
  assert.equal(cmd, "ls -la");
  assert.equal(cl.read(), null);
});

test("光标不在末尾时 atEnd 为 false", async () => {
  const { cl, write, type } = setup();
  await write("$ ");
  await type("echo hi");
  await type("\x1b[D", "\x1b[D"); // ← 左移一格
  const r = cl.read()!;
  assert.equal(r.typed, "echo h");
  assert.equal(r.full, "echo hi");
  assert.equal(r.atEnd, false);
});

test("↑ 翻历史：shell 重画整行，读到的是屏幕上的内容", async () => {
  const { cl, write, type } = setup();
  await write("$ ");
  await type("\x1b[A", "git status"); // shell 回显历史里的命令
  assert.equal(cl.read()!.typed, "git status");
  assert.equal(await type("\r", "\r\n"), "git status");
});

test("长命令折行", async () => {
  const { cl, write, type } = setup(20);
  await write("$ ");
  const long = "echo 0123456789abcdefghijklmnopqrstuvwxyz";
  await type(long);
  const r = cl.read()!;
  assert.equal(r.typed, long);
  assert.equal(r.atEnd, true);
  assert.equal(await type("\r", "\r\n"), long);
});

test("密码不回显，不会被当成命令", async () => {
  const { write, type } = setup();
  await write("[sudo] password for root: ");
  await type("secret", "");
  assert.equal(await type("\r", "\r\n"), "");
});

test("Ctrl+C 放弃这一行", async () => {
  const { cl, write, type } = setup();
  await write("$ ");
  await type("rm -rf /tmp/x");
  await type("\x03", "^C\r\n$ ");
  await type("pwd");
  assert.equal(cl.read()!.typed, "pwd");
});

test("vim 这类全屏程序里不识别", async () => {
  const { cl, write, type } = setup();
  await write("$ ");
  await write("\x1b[?1049h"); // 进入备用屏幕
  assert.equal(await type(":wq\r", ""), null);
  assert.equal(cl.read(), null);
  await write("\x1b[?1049l$ ");
  await type("ls");
  assert.equal(cl.read()!.typed, "ls");
});

test("一次粘贴多行不记录", async () => {
  const { write, type } = setup();
  await write("$ ");
  assert.equal(await type("echo a\recho b\r", "echo a\r\n"), null);
});
