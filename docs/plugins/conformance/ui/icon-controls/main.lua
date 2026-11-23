-- A strip of toggles that are only game icons, two on and two off, each a control square; a
-- button with an icon and its text, the icon's square before the text; an icon toggle with its
-- text.
ui.window{ id = "main", title = "Icons",
	ui.row{ gap = 2,
		ui.toggle{ id = "a", icon = 0x06001234, color = "#40C0FF", value = true, tooltip = "One" },
		ui.toggle{ id = "b", icon = 0x06001235, color = "#40C0FF", value = false, tooltip = "Two" },
		ui.toggle{ id = "c", icon = 0x06001236, color = "#FFB040", value = true, tooltip = "Three" },
		ui.toggle{ id = "d", icon = 0x06001237, value = false, tooltip = "Four" },
	},
	ui.row{
		ui.button{ id = "go", icon = 0x06001238, text = "Go" },
		ui.toggle{ id = "e", icon = 0x06001239, text = "Loot" },
	},
}
