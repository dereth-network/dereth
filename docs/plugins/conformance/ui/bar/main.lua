-- The plugin bar: the plugin says what it is doing, and its main window opens and closes from the
-- bar, the plugin hearing each.
local win = ui.window{ id = "main", title = "Bar", ui.label{ text = "hello" } }
win:close()
ui.status("paused")
print("bad status", (pcall(ui.status, "asleep")))
on("ui.opened", function(e) print("opened", e.window) end)
on("ui.closed", function(e) print("closed", e.window) end)
