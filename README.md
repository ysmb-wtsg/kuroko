# 🥷 `kuroko`

![kuroko demo](docs/images/demo.gif)

**kuroko** is a terminal-native IDE for the AI agent era.

It brings the three things you reach for most — a **file tree**, **git**, and a **terminal** — into one integrated workspace, with AI agents at the center. No window juggling, no context switching: browse files, run git, drive a shell, and direct agents side by side on a single screen.

It's not an editor with AI bolted on — it's an environment built around agents, with the customizability and extensibility of Neovim.

The command name is `krk`.

> [!NOTE]
> The name comes from the kabuki *kuroko* (黒子) — the black-clad stagehand the audience agrees not to see. AI agents stand between you and the code, veiling the source like a kuroko — on the "black screen" of your terminal.

## ✨ Features

kuroko unifies the **file tree**, **git**, and the **terminal** into one screen, with AI agents driving the workflow:

- **File tree**: gitignore-aware navigation, file operations (create / rename / delete), preview, and inline editing in your `$EDITOR` — always one keystroke away
- **Git**: embed lazygit / tig / gitui in the right panel for staging, committing, and history without leaving the workspace
- **Terminal**: full PTY-backed shells running side by side, so your tools (vim, fzf, ...) behave exactly as in a plain terminal
- **AI agent integration**: embed Claude Code, Codex, and custom agents via PTY, right next to your files and git
- **Agent status at a glance**: each tab shows a status dot — working / ready / exited — and the status bar summarizes how many agents are waiting, so you can tell which agent needs you without switching tabs
- **Panel management**: toggle and resize the file tree, terminal, and git panels to compose the layout you want
- **Tab system**: agent tabs and terminal tabs managed independently
- **Conflict-free input**: all keys go straight to the focused pane; pane management lives in a global mode behind `Ctrl+g`
- **Lua customization**: configure via `~/.config/krk/init.lua`

## ⚡️ Requirements

- A terminal emulator (kitty keyboard protocol recommended for `Shift+Enter`)
- For building from source: Rust 1.96.0 or later

## 📦 Installation

### Homebrew

```sh
brew install ysmb-wtsg/tap/kuroko
```

### Build from source

Requires Rust 1.96.0 or later (managing with [mise](https://mise.jdx.dev/) is recommended).

```sh
git clone https://github.com/ysmb-wtsg/kuroko.git
cd kuroko
cargo build --release
```

The binary is generated at `target/release/krk`.

### Development

Enable the git hooks to run `cargo fmt --check` and `cargo clippy` before every push:

```sh
git config core.hooksPath .githooks
```

## 🚀 Usage

```sh
krk
```

By default every key — including `Esc` and `Ctrl` combinations — goes straight to the focused pane, so tools running inside (vim, Claude Code, fzf, ...) behave exactly as they would in a plain terminal.

Press `Ctrl+g` to enter the **global mode**, where single keystrokes manage panes. Press `Ctrl+g` or `Esc` to go back to direct input. The status bar shows a `GLOBAL` badge while the mode is active.

> [!TIP]
> In an agent / terminal pane, `Enter` submits and **`Ctrl+j` inserts a newline** for multi-line input. `Shift+Enter` and `Alt+Enter` also insert a newline on terminals that report them (kitty keyboard protocol); since many terminals cannot distinguish `Shift+Enter` from `Enter`, `Ctrl+j` is the portable shortcut.

### Global mode keybindings

| Key | Action |
|------|------|
| `h/j/k/l` | Directional focus movement |
| `Tab` / `Shift+Tab` | Cycle focus forward / backward |
| `H/J/K/L` | Resize panes |
| `t` | Toggle terminal panel |
| `f` | Toggle file tree panel |
| `g` | Toggle git panel |
| `Enter` | Copy mode (terminal / agent pane) |
| `:` | Command palette |
| `q` | Quit |

### Tab operations (global mode, act on the focused panel)

| Key | Action |
|------|------|
| `n` | Add a new tab |
| `x` / `w` | Close the active tab |
| `[` / `]` | Switch to previous / next tab |
| `1-9` | Select tab by number |
| `r` | Rename tab |

## ⚙️ Configuration

Place a config file at `~/.config/krk/init.lua` and it is loaded on startup.

```lua
-- Available APIs
krk.pane.toggle(type)      -- Toggle a panel ("terminal", "files")
krk.pane.focus(direction)  -- Move focus ("next", "prev", "left", "right", "up", "down")
krk.opt.leader             -- Leader key
krk.opt.main_pane          -- Main pane type ("claude-code", "codex", "terminal")
krk.opt.git_tool           -- Git panel tool ("lazygit", "tig", "gitui", etc.)

-- Keybindings
-- context: "global" (inside the global mode) | "direct" (intercepted before the pane)
krk.keymap.set(context, key, callback)
krk.keymap.set_toggle_key(key)  -- Change the global mode toggle (default: "<C-g>")
```

Example — direct `Ctrl+h/j/k/l` focus movement:

```lua
for key, dir in pairs({ ["<C-h>"] = "left", ["<C-j>"] = "down", ["<C-k>"] = "up", ["<C-l>"] = "right" }) do
  krk.keymap.set("direct", key, function() krk.pane.focus(dir) end)
end
```

> [!WARNING]
> Binding keys in the `direct` context steals them from apps running inside panes. Prefer the `global` context unless you specifically want a key to bypass the focused tool.

## 📄 License

MIT
