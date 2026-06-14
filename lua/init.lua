-- kuroko default configuration
-- Copy to ~/.config/krk/init.lua to customize

-- Leader key
krk.opt.leader = " "

-- Main pane ("claude-code", "codex", "terminal")
-- krk.opt.main_pane = "claude-code"

-- Editor for the file tree "e" key (opens in a floating dialog).
-- Falls back to $EDITOR, then "vim", when unset. Args are allowed (e.g. "nvim -u NONE").
-- krk.opt.editor = "vim"

-- Global layer toggle key (default: Ctrl+g)
-- krk.keymap.set_toggle_key("<C-g>")

-- Keybinding examples
-- context "global": active inside the global layer (single keystrokes)
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
