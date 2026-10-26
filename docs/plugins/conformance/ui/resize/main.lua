-- A resizable window takes the player's size within its bounds and keeps it when declared again.
local function declare()
	ui.window{ id = "main", title = "Resize", resizable = true, min_width = 20, max_width = 60,
		min_height = 4, max_height = 30, ui.label{ text = "hello" } }
end
declare()
on("ui.resized", function(e)
	print("resized", e.window, e.width, e.height)
	declare()
end)
