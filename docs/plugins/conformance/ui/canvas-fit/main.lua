-- A canvas that fits its room in a window the player sizes: it takes the room left under a label
-- and draws its square of pixels as large as fits that room, in its middle.
ui.window{ id = "main", title = "Fit", resizable = true,
	ui.label{ id = "head", text = "Map" },
	ui.canvas{ id = "map", width = 100, height = 100, fit = true, background = "#101010", draw = {
		{ op = "rect", x = 10, y = 10, w = 80, h = 80, color = "#FFFFFF" },
	} },
}
