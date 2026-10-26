-- A window with tabs too short for its page: the row of tabs stays where it is and the page alone
-- scrolls, its nodes laid out at their least height and as wide as the page less the scrollbar's
-- room.
local settings = { title = "Settings" }
for i = 1, 8 do
	settings[#settings + 1] = ui.number{ id = "n" .. i, text = "Setting " .. i, value = i }
end
ui.window{ id = "main", title = "Tabs", resizable = true,
	ui.label{ id = "status", text = "Running" },
	ui.tabs{ id = "pages", selected = 1,
		ui.group(settings),
		ui.group{ title = "Log", ui.list{ id = "log", columns = { "Line" }, rows = {} } },
	},
}
