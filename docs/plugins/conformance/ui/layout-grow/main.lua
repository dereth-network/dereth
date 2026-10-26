-- Two lists sharing the room left down a window in proportion to their shares, under a label and
-- a row that keep their height (the row's field grows across it alone); across, each as wide as
-- the window.
ui.window{ id = "main", title = "Grow", resizable = true,
	ui.label{ id = "head", text = "Two logs" },
	ui.row{ ui.input{ id = "find", grow = 1 }, ui.button{ id = "go", text = "Go" } },
	ui.list{ id = "a", columns = { "A" }, rows = {}, height = 2, grow = 1 },
	ui.list{ id = "b", columns = { "B" }, rows = {}, height = 2, grow = 3 },
}
