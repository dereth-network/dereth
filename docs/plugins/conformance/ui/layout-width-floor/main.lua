-- In a row, a list bounded under its own floor keeps its bounds: one no wider than 5 cells is 5
-- cells wide, one that may be as narrow as 5 cells gives up its room as the window narrows, and one
-- without bounds keeps its 8 cells.
ui.window{ id = "main", title = "Floor", resizable = true,
	ui.row{
		ui.list{ id = "most", columns = { "A" }, rows = {}, height = 2, max_width = 5 },
		ui.list{ id = "least", columns = { "B" }, rows = {}, height = 2, min_width = 5, grow = 1 },
		ui.list{ id = "plain", columns = { "C" }, rows = {}, height = 2 },
	},
}
