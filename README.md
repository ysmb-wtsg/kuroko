# kuroko

A terminal TUI application aiming to be the home base that replaces the editor in the AI agent era. The command name is `krk`.

It keeps the customizability and extensibility of Neovim, while putting AI agent operation at the center.

The name comes from the kabuki *kuroko* (黒子) — the black-clad stagehand the audience agrees not to see. AI agents stand between you and the code, veiling the source like a kuroko — on the "black screen" of your terminal.

## Features

- **AI agent integration**: Embed Claude Code, Codex, and custom agents via PTY
- **Panel management**: Toggle and resize file tree, terminal, and git panels
- **Tab system**: Agent tabs and terminal tabs managed independently
- **Modal operation**: Normal / Insert / Select modes (Vim-like)
- **Lua customization**: Configure via `~/.config/krk/init.lua`
- **Git panel**: Embed external tools such as lazygit / tig / gitui in the right panel
- **File tree**: gitignore-aware, file operations (create / rename / delete), preview

## Installation

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

## Usage

```sh
krk
```

On startup, the agent pane is shown in Insert mode.

### Switching modes

| Key | Action |
|------|------|
| `Esc` | Switch to Normal mode |
| `i` | Back to Insert mode |

### Normal mode keybindings

| Key | Action |
|------|------|
| `h/j/k/l` | Directional focus movement |
| `Tab` / `Shift+Tab` | Cycle focus forward / backward |
| `H/J/K/L` | Resize panes |
| `t` | Toggle terminal panel |
| `f` | Toggle file tree panel |
| `g` | Toggle git panel |
| `q` | Quit |

### Tab operations (act on the focused panel)

| Key | Action |
|------|------|
| `n` | Add a new tab |
| `x` / `w` | Close the active tab |
| `[` / `]` | Switch to previous / next tab |
| `1-9` | Select tab by number |
| `r` | Rename tab |

## Lua customization

Place a config file at `~/.config/krk/init.lua` and it is loaded on startup.

```lua
-- Available APIs
krk.pane.toggle(type)      -- Toggle a panel ("terminal", "files")
krk.pane.focus(direction)  -- Move focus ("next", "prev", "left", "right", "up", "down")
krk.opt.leader             -- Leader key
krk.opt.main_pane          -- Main pane type ("claude-code", "codex", "terminal")
krk.opt.git_tool           -- Git panel tool ("lazygit", "tig", "gitui", etc.)
```

## Tech stack

- **Language**: Rust (edition 2024)
- **TUI**: ratatui 0.30 + crossterm
- **PTY**: portable-pty + vt100
- **Plugins**: Lua 5.4 (mlua, vendored)

## Status

v0.1.0-alpha — basic pane management, agent integration, and Lua configuration are working.

Planned:
- Command palette
- Custom keybindings (`krk.keymap.set`)
- Terminal copy mode (scrollback)
- Session persistence
- Theme customization

## License

MIT
