-- A canvas that takes the pointer: the wheel over it and a drag across it are told to the plugin,
-- in the canvas's own pixels.
ui.window({ id = "map", title = "Map",
	ui.canvas({ id = "c", width = 100, height = 80, pointer = true }),
})
on("ui.canvas", function(e)
	print(e.window, e.id, e.x, e.y, e.wheel, e.dx, e.dy)
end)
