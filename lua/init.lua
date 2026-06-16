-- kuroko default configuration
-- Copy to ~/.config/krk/init.lua and edit to customize.
-- Each option below is set to its default value.

--------------------------------------------------------------------------------
-- Options (krk.opt)
--------------------------------------------------------------------------------

-- Leader key. Prefix for "<leader>..." keybindings in the global mode.
krk.opt.leader = " "

-- Main pane shown on startup ("claude-code", "codex", "terminal").
krk.opt.main_pane = "claude-code"

-- Git panel tool ("lazygit", "tig", "gitui", etc.). Must be installed.
krk.opt.git_tool = "lazygit"

-- Editor for the file tree "e" key (opens in a floating dialog).
-- Args are allowed (e.g. "nvim -u NONE").
krk.opt.editor = os.getenv("EDITOR") or "vim"

-- File manager for the side panel. "builtin" uses the built-in tree.
-- Set to an external TUI file manager (e.g. "yazi", "lf"); args are allowed.
-- Note: replacing the built-in tree disables kuroko-specific keys
-- (send-to-agent, preview, in-app editor); the external tool handles its own keys.
krk.opt.file_manager = "builtin"

-- Desktop notification when an agent finishes and waits for your input.
-- Sends an OSC 9 escape to the outer terminal (iTerm2/WezTerm/Ghostty/kitty).
krk.opt.notify = true

-- Notification body template. "{title}" is replaced with the agent pane name.
krk.opt.notify_message = "{title}: waiting for your input"

-- Event loop tick rate in milliseconds. (Reserved: defined but not yet wired up.)
krk.opt.tick_rate = 50

--------------------------------------------------------------------------------
-- Keymaps (krk.keymap)
--------------------------------------------------------------------------------

-- Global mode toggle key (default: Ctrl+g).
krk.keymap.set_toggle_key("<C-g>")

-- krk.keymap.set(context, key, callback)
-- context "global": active inside the global mode (single keystrokes after Ctrl+g)
krk.keymap.set("global", "<leader>t", function()
    krk.pane.toggle("terminal")
end)

krk.keymap.set("global", "<leader>f", function()
    krk.pane.toggle("files")
end)

-- context "direct": intercepted before reaching the focused pane.
-- Use sparingly -- each binding steals that key from apps inside panes.
-- krk.keymap.set("direct", "<C-h>", function()
--     krk.pane.focus("left")
-- end)

--------------------------------------------------------------------------------
-- Pane API (krk.pane), usable inside keymap callbacks
--------------------------------------------------------------------------------

-- krk.pane.toggle(type)      -- Toggle a panel ("terminal", "files")
-- krk.pane.focus(direction)  -- Move focus ("next", "prev", "left", "right", "up", "down")
