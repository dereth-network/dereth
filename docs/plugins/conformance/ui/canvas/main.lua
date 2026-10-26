-- A free drawing and a chart in a HUD: the canvas keeps its draw commands and takes new ones; a
-- draw command of no known op is refused.
local hud = ui.hud({ id = "map", anchor = "top_left",
	ui.canvas({ id = "c", width = 40, height = 20, background = "#101010", draw = {
		{ op = "line", x1 = 0, y1 = 0, x2 = 39, y2 = 19, color = "#FF0000" },
		{ op = "text", x = 2, y = 2, text = "N" },
	} }),
	ui.chart({ id = "xp", width = 60, height = 30, series = { { values = { 1, 3, 2 }, label = "xp" } } }),
})
local ok = pcall(ui.hud, { id = "bad", ui.canvas({ id = "b", draw = { { op = "blob" } } }) })
print("bad op", ok)
hud:get("c").draw = { { op = "circle", x = 20, y = 10, r = 5, fill = true } }
print("redrawn", #hud:get("c").draw)
