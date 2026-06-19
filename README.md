# 🥷 `kuroko`

English | [日本語](README.ja.md)

![kuroko demo](docs/images/demo.gif)

**kuroko** is a terminal-native IDE.

## ✨ Features

- **Built-in file tree, git client, and terminal**: just open kuroko for a seamless development workflow.
- **Multi-agent execution**: agent panes are tabbed, and each tab's status (waiting for input / done) is visible at a glance.
- **Lua customization**: customizable via `~/.config/krk/init.lua`.

## 📦 Installation

### Homebrew

```sh
brew install ysmb-wtsg/tap/kuroko
```

### Build from source

```sh
git clone https://github.com/ysmb-wtsg/kuroko.git
cd kuroko
cargo build --release
```

Rust 1.96.0 or later is required.

## 🚀 Usage

```sh
krk
```

Press `Ctrl+g` to enter **global mode**, where you can perform meta operations such as resizing panes and moving focus.
To leave global mode, press either `Ctrl+g` or `Esc`.

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
krk.opt.file_manager       -- File manager ("yazi", etc. Unset/"builtin" uses the built-in tree)
krk.opt.notify             -- Desktop notification when an agent waits for input (default: true)
krk.opt.notify_message     -- Notification body template ("{title}" = agent name)

-- Keybindings
-- context: "global" (inside the global mode) | "direct" (intercepted before the pane)
krk.keymap.set(context, key, callback)
krk.keymap.set_toggle_key(key)  -- Change the global mode toggle key (default: "<C-g>")
```

Example — direct `Ctrl+h/j/k/l` focus movement:

```lua
for key, dir in pairs({ ["<C-h>"] = "left", ["<C-j>"] = "down", ["<C-k>"] = "up", ["<C-l>"] = "right" }) do
  krk.keymap.set("direct", key, function() krk.pane.focus(dir) end)
end
```

## 🎨 Customization

The main panels can be replaced with third-party tools. All of them are configured in `~/.config/krk/init.lua`.

### File manager

Set a launch command in `krk.opt.file_manager` to replace the file tree panel with an external file manager.

```lua
krk.opt.file_manager = "yazi"
```

![yazi](docs/images/yazi.png)

When unset or `"builtin"`, the built-in file tree is used. If the specified command is not found, it falls back to the built-in tree.

### Git

You can specify a git client in `krk.opt.git_tool`.

```lua
krk.opt.git_tool = "tig"
```

![tig](docs/images/tig.png)

### Notifications

A desktop notification is shown when an agent is waiting for input.
Toggle it with `krk.opt.notify` (default: `true`), and set the body template with `krk.opt.notify_message` (`{title}` is replaced with the agent name).

![notification](docs/images/notification.png)

```lua
krk.opt.notify = true
krk.opt.notify_message = "{title}: waiting for input"
```

### Agents

Choose the agent to place in the main pane with `krk.opt.main_pane`.

![codex](docs/images/codex.png)

```lua
krk.opt.main_pane = "codex"
```

## 💡 Inspiration

AI-agent-driven vibe coding no longer requires viewing or editing files by hand.
Developers are shifting away from the traditional paradigm — the editor at the center, AI agents welcomed as guests — toward a new mindset that places AI agents at the center.
`kuroko` embodies this new paradigm: an IDE for the new era.

> [!NOTE]
> The name comes from the kabuki *kuroko* (黒子) — the black-clad stagehand the audience agrees to treat as invisible.
> kuroko is the stagehand on the "black screen" that supports your vibe coding.

## 📄 License

MIT
