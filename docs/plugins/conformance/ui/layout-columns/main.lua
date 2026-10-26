-- A list's columns: a fixed time, a growing line; a growing name between fixed version and state
-- columns; a column of a least width.
ui.window{ id = "main", title = "Columns", resizable = true,
	ui.list{ id = "log", columns = { { title = "Time", width = 7 }, "Line" },
		rows = { { "12.5", "the first line of the log" } }, height = 3 },
	ui.list{ id = "plugins", columns = { { title = "Plugin", grow = 1 }, { title = "Version", width = 8 },
		{ title = "State", width = 10 } }, rows = { { "Tank", "0.2.0", "running" } }, height = 3 },
	ui.list{ id = "least", columns = { { title = "Name", grow = 1, min_width = 30 }, "Note" }, rows = {},
		height = 3 },
}
