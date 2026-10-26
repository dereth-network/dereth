-- Nothing says grow: on each tab's page the last list or text box takes the room left, and the
-- tabs, holding them, take the window's.
ui.window{ id = "main", title = "Auto", resizable = true,
	ui.tabs{ id = "pages", selected = 2,
		ui.group{ title = "One", ui.list{ id = "only", columns = { "Only" }, rows = {}, height = 2 } },
		ui.group{ title = "Two",
			ui.list{ id = "first", columns = { "First" }, rows = {}, height = 2 },
			ui.textbox{ id = "notes", value = "", height = 2 },
			ui.label{ id = "foot", text = "the end" },
		},
	},
}
