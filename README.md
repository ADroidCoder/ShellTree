<div align="center">

# ShellTree

**A lightweight, free and open-source SSH / SFTP desktop client for macOS and Windows.**

Built with Rust and Tauri. Terminal, file transfer and host management in one window — quick to open, light on memory, fast to transfer.

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
![Platforms](https://img.shields.io/badge/platform-macOS%20%7C%20Windows-lightgrey)

English · [简体中文](README.zh-CN.md)

[**Download**](https://github.com/ADroidCoder/ShellTree/releases/latest) · [Features](#features) · [Performance](#performance) · [Building from source](#building-from-source)

</div>

<p align="center"><img src="doc/terminal.png" alt="ShellTree: terminal and file panel" width="100%"></p>

---

## Why ShellTree

Popular SSH clients are either paid and require an account, or built on Electron or Java — hundreds of megabytes to install, and memory climbs with every session. ShellTree aims to be a tool that **just opens, stays fast, and costs nothing**:

- **Lightweight** — uses the system WebView instead of bundling a browser engine; the installer is only a few MB.
- **Fast** — the SSH protocol is pure Rust ([russh](https://github.com/Eugeny/russh)) and the terminal is GPU-rendered. SFTP keeps many requests in flight per file and transfers several files of a folder in parallel.
- **Consistent with the command line** — hosts from `~/.ssh/config` show up automatically, host keys live in `~/.ssh/known_hosts`, and ssh-agent is supported. It shares the same setup as your `ssh` command.
- **Free and open source** — Apache 2.0, no account, no telemetry. Passwords are stored only in the system keychain.

## Download

Get the latest installer from the [**Releases page**](https://github.com/ADroidCoder/ShellTree/releases/latest). Until the first release is published, [build from source](#building-from-source).

| Platform | File |
|---|---|
| macOS (Apple Silicon) | `ShellTree_<version>_aarch64.dmg` |
| macOS (Intel) | `ShellTree_<version>_x64.dmg` |
| Windows 10 / 11 (x64) | `ShellTree_<version>_x64-setup.exe` or `ShellTree_<version>_x64_en-US.msi` |

> [!NOTE]
> The installers aren't code-signed yet.
> - **macOS** — if it says the app is "damaged" or "can't be opened", run `xattr -cr /Applications/ShellTree.app` once, or right-click the app and choose "Open".
> - **Windows** — if SmartScreen shows a warning, click "More info → Run anyway".

## Features

<p align="center">
  <img src="doc/hosts.png" alt="Hosts" width="49%">
  <img src="doc/host-details.png" alt="Host details" width="49%">
</p>

### Hosts
- **Groups and cards** — hosts are organized in groups, shown as cards or a list. Create empty groups, rename or dissolve them, and move hosts between groups from the context menu.
- **Quick connect** — type `root@10.0.0.1:22` in the search box and press Enter, no need to save first.
- **Reads `~/.ssh/config`** — hosts from your SSH config appear automatically and can be saved with one click.
- **OS detection** — Ubuntu, Debian, CentOS, Alpine and others are detected on first connect, and the host icon is colored by OS.
- **Authentication** — Auto (like `ssh`: tries ssh-agent and the default keys in `~/.ssh`, then falls back to a password), password, or private key (passphrase-protected keys supported). Passwords are stored in the system keychain (macOS Keychain / Windows Credential Manager).
- **Jump hosts** — reach internal servers through a bastion, like `ssh -J`, including multiple hops. `ProxyJump` in `~/.ssh/config` is recognized.
- **Verification codes** — when a server asks for a one-time code (Google Authenticator, etc.), you're prompted for it. Password + code and key + code both work.
- **Host key verification** — shows the fingerprint on first connect and a prominent warning if it ever changes, protecting against man-in-the-middle attacks.
- **Known hosts** — view and remove entries in `~/.ssh/known_hosts`, e.g. after a server is reinstalled; right-click an entry to add it as a host.

### Terminal
- **xterm.js + WebGL** — 256 colors and true color, correct widths for CJK and emoji, smooth under heavy output.
- **No freezing on floods** — when you `cat` a huge file or run `yes`, output is sent at the pace the UI can render, so the window stays responsive and Ctrl+C takes effect immediately.
- **History suggestions** — as you type, a gray suggestion from commands you've run on this host appears; press → or End to accept (like fish or zsh-autosuggestions). Passwords are never recorded, suggestions are off inside full-screen apps like vim or top, and history stays on your computer — clear it any time.
- **Snippets** — save frequently used commands and insert or run them from the snippet panel next to the terminal. Use `{{variables}}` to fill in values each time.
- **Session tabs** — one tab per session with a status color.
- **Split panes** — split a tab left/right or top/bottom (⌘D / ⇧⌘D). Each pane is its own shell on the same connection; drag the divider to resize and use ⌘[ / ⌘] to switch.
- **Broadcast input** — turn on broadcast (⌘B) in several tabs and whatever you type in one goes to all of them, handy for running the same commands on many servers.
- **Local terminal** — open a shell on your own computer (⌘L): your default shell on macOS, PowerShell on Windows.
- **Find in terminal** — ⌘F searches the output, with match-case and regex options.
- **Server status bar** — CPU, memory, disk, network and load below the terminal, read over the same connection with nothing to install (Linux only, can be turned off in Settings).
- **Session logs** — "Record session log" in the context menu saves the output as plain text (colors and control codes stripped) and keeps writing after a reconnect.
- **Auto-reconnect** — reconnects automatically when the network drops, keeping the previous output on screen; press Enter to reconnect right away.
- **Port forwarding** — local (`-L`, e.g. reach a database on the server from your computer), remote (`-R`) and SOCKS5 dynamic (`-D`). Configure them per host to start on every connect, or add one on the fly from the panel next to the terminal. `LocalForward` and friends in `~/.ssh/config` are recognized.
- Copy, paste and clear from the context menu. On Windows, copy/paste is Ctrl+Shift+C / V so Ctrl+C stays with the terminal.
- ⌘ / Ctrl-click a link to open it in your browser.

### SFTP
<p align="center"><img src="doc/sftp.png" alt="Dual-pane SFTP" width="100%"></p>

- **File panel next to the terminal** — shares the terminal's connection, so there's no second login; "Open in Terminal" `cd`s the shell into a folder.
- **Dual-pane SFTP** — pick Local or any host for each pane, and drag files between panes to upload or download. Between two servers, files are copied directly (relayed through memory, never written to your disk).
- **Drag-and-drop upload** — drop files or folders from Finder / Explorer; drop onto a folder to upload into it.
- **Transfer queue** — progress, speed, time remaining and cancel. Partial files are cleaned up on failure or cancel; downloads never overwrite a local file of the same name.
- **Edit remote files** — double-click a text file to open it in a local editor (the system text editor by default, or VS Code etc. from Settings); saving uploads it back automatically, with a warning if the file changed on the server in the meantime.
- Rename, delete, new folder, change permissions, copy path. The file list is virtualized, so folders with tens of thousands of files stay smooth.
- Executable permissions are kept when uploading scripts.

### App
- **Dark / Light / System** appearance.
- **English and Chinese UI** — follows the system language by default and can be switched in Settings at any time, without restarting or dropping open sessions.
- Adjustable terminal font size.
- **Auto-update** — checks GitHub Releases in the background, shows a button in the top-right corner when a new version is out, and installs it with one click. Updates are signed and verified before installing.

## Keyboard shortcuts

On Windows, use Ctrl+Shift instead of ⌘ (plain Ctrl+letter is left to the terminal, e.g. Ctrl+W deletes a word).

| Shortcut | Action |
|---|---|
| ⌘T | New connection (open host search) |
| ⌘N | New host |
| ⌘W | Close the current pane, or the session if it isn't split |
| ⌘L | Open a local terminal |
| ⌘1 – ⌘9 | Switch to session N |
| ⌘E | Show / hide the file panel |
| ⌘J | Show / hide snippets |
| ⌘F | Find in the terminal |
| ⌘D / ⇧⌘D | Split right / down (Windows: Ctrl+Shift+D / Ctrl+Alt+Shift+D) |
| ⌘[ / ⌘] | Previous / next pane |
| ⌘B | Toggle broadcast input |
| → / End | Accept the gray suggestion |
| ⌘C / ⌘V | Terminal copy / paste (Windows: Ctrl+Shift+C / V) |
| ⌘K | Clear the terminal |

## Performance

Measured against OpenSSH in Docker on the same machine (loopback):

| Scenario | Result |
|---|---|
| Connect and log in | ~50 ms |
| Upload a 200 MB file | ~89 MB/s |
| Download a 200 MB file | ~53 MB/s |
| Upload a folder of 501 small files | 0.46 s |

How:

- **SFTP pipelining** — up to 64 read/write requests in flight per file instead of waiting for each reply, so high-latency links still reach full bandwidth.
- **Parallel small files** — folders are transferred 4 files at a time, so per-file open/close round trips don't add up.
- **Batched terminal output with flow control** — output is sent to the UI as binary (no JSON), the next batch is sent only after the UI has rendered the previous one, and whatever arrives in between is merged into one chunk.
- **One connection, many channels** — the terminal and SFTP share a single SSH connection, so opening the file panel needs no new handshake or login.

Measure it yourself (you need an SSH server to test against):

```bash
cargo run --release -p shelltree-core --example smoke -- host port user password
```

## Building from source

**Requirements**

- [Rust](https://rustup.rs/) 1.85 or newer
- [Node.js](https://nodejs.org/) 22 or newer
- Windows only: [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/) (usually preinstalled on Windows 10/11) and the Visual Studio C++ Build Tools

```bash
git clone https://github.com/ADroidCoder/ShellTree.git
cd ShellTree
npm install

npm run tauri dev     # run in development mode
npm run tauri build   # build installers
```

Output:

- macOS: `target/release/bundle/dmg/*.dmg` and `target/release/bundle/macos/*.app`
- Windows: `target/release/bundle/msi/*.msi` and `target/release/bundle/nsis/*.exe`

**Tests**

```bash
cargo test -p shelltree-core   # core library tests
npm test                     # command-line detection (suggestions) and split-pane layout tests
cargo run -p shelltree-core --example otp   # verification-code login (starts a local test server)
npm run check                # frontend type check
```

**Releases** are built by [GitHub Actions](.github/workflows/build.yml):

1. Bump the version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json` (all three must match — CI checks).
2. Commit and push a tag: `git tag v0.2.0 && git push origin v0.2.0`.
3. CI builds the Windows and both macOS installers, publishes a Release with `latest.json`, and installed copies get an update prompt.

Before the first release, add the updater signing key under Settings → Secrets and variables → Actions: `TAURI_SIGNING_PRIVATE_KEY` (the private key file's contents) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. If the key is ever lost, installed versions can never be updated again — keep a backup.

Regular pushes only run tests. Run the workflow manually from the Actions page to build without releasing; the installers are in that run's Artifacts.

## Project layout

```
crates/shelltree-core/   Core library (UI-independent, testable on its own)
  src/conn.rs          Connecting, host key verification, login (password / key / ssh-agent)
  src/shell.rs         Terminals (remote shell and local pty), output batching and flow control
  src/log.rs           Session logs (control codes stripped)
  src/stats.rs         Server status (CPU, memory, disk, network)
  src/forward.rs       Port forwarding (local / remote / SOCKS5)
  src/sftp.rs          Remote file operations, upload and download
  src/local.rs         Local file operations (the Local pane in dual-pane SFTP)
  src/hosts.rs         Host list, ~/.ssh/config, keychain
  src/known_hosts.rs   Viewing and removing ~/.ssh/known_hosts entries
  examples/smoke.rs    End-to-end test against a real server
src-tauri/             Tauri desktop app exposing the core library to the UI
src/                   Svelte 5 UI
  lib/cmdline.ts       Detects the command being typed in the terminal (for suggestions)
  lib/panes.ts         Split-pane layout
tests/                 Frontend tests
```

**Tech stack:** Rust · [russh](https://github.com/Eugeny/russh) · [russh-sftp](https://github.com/AspectUnk/russh-sftp) · [Tauri 2](https://tauri.app/) · [Svelte 5](https://svelte.dev/) · [xterm.js](https://xtermjs.org/) · TypeScript

## Contributing

Issues and pull requests are welcome. Before opening a PR, please make sure `cargo test -p shelltree-core`, `npm test` and `npm run check` all pass.

## License

Licensed under the [Apache License 2.0](LICENSE): free for personal and commercial use, modification and redistribution, as long as you keep the copyright and license notices and mark what you changed.
