-- A declared window: a click and a toggle change come back as events, and the plugin updates a label.
local count = 0
local win = ui.window{ id = "main", title = "Conformance",
	ui.label{ id = "count", text = "0" },
	ui.button{ id = "add", text = "Add" },
	ui.toggle{ id = "auto", text = "Auto", value = false },
	ui.meter{ id = "bar", value = 1, max = 4, style = "mana" },
	ui.list{ id = "rows", columns = { "A", "B" }, rows = { { "x", 1 } } },
}
on("ui.clicked", function(e)
	if e.id == "add" then
		count = count + 1
		win:get("count").text = tostring(count)
		win:get("bar").value = count
	end
end)
on("ui.changed", function(e) print("changed", e.id, e.value) end)
local ok, err = pcall(ui.window, { id = "x", ui.label{ txt = "typo" } })
print("bad property refused", ok, string.find(tostring(err), "no property", 1, true) ~= nil)
ok, err = pcall(ui.window, { id = "y", ui.label{ text = string.rep("W", 4097) } })
print("long text refused", ok, string.find(tostring(err), "4096", 1, true) ~= nil)
local kids = { id = "z" }
for i = 1, 17 do kids[#kids + 1] = ui.label{ text = string.rep("W", 4000) } end
ok, err = pcall(ui.window, kids)
print("too much text refused", ok, string.find(tostring(err), "65536", 1, true) ~= nil)
local cmds = {}
for i = 1, 1001 do cmds[i] = { op = "circle", x = i, y = 0, r = 2 } end
ok, err = pcall(ui.window, { id = "c", ui.canvas{ draw = cmds } })
print("too many commands refused", ok, string.find(tostring(err), "1000", 1, true) ~= nil)
ok, err = pcall(function() win:get("count").text = string.rep("W", 4097) end)
print("long change refused", ok)
