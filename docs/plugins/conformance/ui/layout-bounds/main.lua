-- A growing list stops at its most height and the other takes the rest; across the window, a list
-- stops at its most width.
ui.window{ id = "main", title = "Bounds", resizable = true,
	ui.list{ id = "short", columns = { "Short" }, rows = {}, height = 2, grow = 1, max_height = 6 },
	ui.list{ id = "tall", columns = { "Tall" }, rows = {}, height = 2, grow = 1 },
	ui.list{ id = "narrow", columns = { "Narrow" }, rows = {}, height = 2, max_width = 20 },
}
