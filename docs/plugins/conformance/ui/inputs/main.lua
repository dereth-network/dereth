-- Text the player types, a slider, a text box, list cells with their own colour and icon, a
-- tooltip, and a toast. Typed text and a slid value reach the plugin as changes.
ui.window({ id = "main", title = "Inputs",
	ui.input({ id = "name", text = "Name", value = "", placeholder = "who", tooltip = "Whom to follow" }),
	ui.slider({ id = "vol", text = "Volume", value = 5, min = 0, max = 10, step = 1 }),
	ui.textbox({ id = "notes", value = "line one\nline two", height = 3 }),
	ui.list({ id = "loot", columns = { "Item", "Value" }, rows = {
		{ { text = "Gold Ring", color = "#FFD700", icon = 0x06001234 }, 900 },
		{ "Rusty Dagger", 5 },
	} }),
	ui.button({ id = "go", text = "Go", tooltip = "Start following" }),
})
on("ui.changed", function(e)
	print("changed", e.id, e.value)
	if e.id == "name" then
		ui.toast("following " .. e.value, 4)
	end
end)
local ok = pcall(ui.window, { id = "bad", ui.list({ id = "l", rows = { { { colour = "#FFFFFF" } } } }) })
print("bad cell", ok)
