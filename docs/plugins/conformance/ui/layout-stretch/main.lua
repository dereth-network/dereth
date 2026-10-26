-- Across a column each node takes the column's width but a picture, which keeps its own size; in a
-- row a list takes the row's height and a button stands in its middle; a row's justify puts the
-- room left before its nodes, and a column's align keeps its nodes at their own width.
ui.window{ id = "main", title = "Stretch", resizable = true,
	ui.button{ id = "wide", text = "Wide" },
	ui.canvas{ id = "pic", width = 50, height = 20 },
	ui.row{ id = "pair", ui.button{ id = "go", text = "Go" }, ui.list{ id = "rows", columns = { "Row" }, rows = {}, height = 3 } },
	ui.row{ id = "right", justify = "end", ui.button{ id = "ok", text = "OK" }, ui.button{ id = "cancel", text = "Cancel" } },
	ui.column{ id = "middle", align = "center", ui.button{ id = "mid", text = "Middle" } },
}
