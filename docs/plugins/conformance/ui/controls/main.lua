-- Tabs, a number and a choice: each change comes back as an event and stays on the node; a tabs
-- node holds only group pages.
ui.window{ id = "main", title = "Controls",
	ui.tabs{ id = "pages", selected = 1,
		ui.group{ title = "One",
			ui.number{ id = "range", text = "Range", value = 8, min = 0, max = 10, step = 2 },
		},
		ui.group{ title = "Two",
			ui.choice{ id = "mode", text = "Mode", options = { "stop", "recall" }, value = "stop" },
		},
	},
}
on("ui.changed", function(e) print("changed", e.id, e.value) end)
on("ui.selected", function(e) print("selected", e.id, e.rows[1]) end)
local ok, err = pcall(ui.window, { id = "x", ui.tabs{ ui.label{ text = "not a page" } } })
print("tabs hold groups", ok, string.find(tostring(err), "only groups", 1, true) ~= nil)
