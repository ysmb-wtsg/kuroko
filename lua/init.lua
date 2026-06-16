-- kuroko default configuration
-- Copy to ~/.config/krk/init.lua to customize

-- Leader key
krk.opt.leader = " "

-- Main pane ("claude-code", "codex", "terminal")
-- krk.opt.main_pane = "claude-code"

-- Editor for the file tree "e" key (opens in a floating dialog).
-- Falls back to $EDITOR, then "vim", when unset. Args are allowed (e.g. "nvim -u NONE").
-- krk.opt.editor = "vim"

-- File manager for the side panel. Uses the built-in tree when unset or "builtin".
-- Set to an external TUI file manager (e.g. "yazi", "lf"). Args are allowed.
-- Note: replacing the built-in tree disables kuroko-specific keys
-- (send-to-agent, preview, in-app editor); the external tool handles its own keys.
-- krk.opt.file_manager = "yazi"

-- Desktop notification when an agent finishes and waits for your input.
-- Sends an OSC 9 escape to the outer terminal (iTerm2/WezTerm/Ghostty/kitty).
-- Set false to disable.
-- krk.opt.notify = true

-- Notification body template. "{title}" is replaced with the agent pane name.
-- krk.opt.notify_message = "{title}: waiting for your input"

-- Global mode toggle key (default: Ctrl+g)
-- krk.keymap.set_toggle_key("<C-g>")

-- Keybinding examples
-- context "global": active inside the global mode (single keystrokes)
-- krk.keymap.set("global", "<leader>t", function()
--     krk.pane.toggle("terminal")
-- end)
--
-- krk.keymap.set("global", "<leader>f", function()
--     krk.pane.toggle("files")
-- end)
--
-- context "direct": intercepted before reaching the focused pane.
-- Use sparingly -- each binding steals that key from apps inside panes.
-- krk.keymap.set("direct", "<C-h>", function()
--     krk.pane.focus("left")
-- end)
