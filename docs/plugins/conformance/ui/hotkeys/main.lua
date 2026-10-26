-- A hotkey runs a plugin command as if the player typed it: the command hears its words, and its
-- actions follow the player's gesture. A hotkey with a plain key (no Ctrl or Alt) is refused, and
-- one naming the host's own command reaches nothing.
commands.add("hk", "run by a hotkey", function(args, words)
	print("ran", args, #words, permissions.activation() > 0)
end)
ui.window({ id = "main", title = "Hotkeys",
	ui.hotkey({ id = "go", command = "hk run fast", text = "Run fast", key = "Ctrl+Shift+F5" }),
	ui.hotkey({ id = "stray", command = "nothere", key = "Alt+K" }),
	ui.hotkey({ id = "host", command = "plugin unload conformance.hotkeys", key = "Alt+P" }),
})
local ok = pcall(ui.window, { id = "bad", ui.hotkey({ id = "plain", command = "hk", key = "F5" }) })
print("plain key", ok)
