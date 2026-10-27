-- A window with a button, a box to tick and a line of text: it says each click and change, and the
-- text counts the clicks.
local clicks = 0

local function draw()
	ui.window{ id = "main", title = "Window",
		ui.button{ id = "go", text = "Go" },
		ui.toggle{ id = "auto", text = "Auto", value = false },
		ui.label{ id = "count", text = "clicks " .. clicks },
	}
end

draw()
on("ui.clicked", function(e)
	clicks = clicks + 1
	print("clicked", e.id)
	draw()
end)
on("ui.changed", function(e) print("changed", e.id, tostring(e.value)) end)
on("ui.closed", function(e) print("closed", e.window) end)
on("ui.opened", function(e) print("opened", e.window) end)
