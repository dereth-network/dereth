-- More controls than a short window holds: its body scrolls through them, laid out at their least
-- height and as wide as the body less the scrollbar's room.
local nodes = { id = "main", title = "Scroll", resizable = true }
for i = 1, 8 do
	nodes[#nodes + 1] = ui.number{ id = "n" .. i, text = "Setting " .. i, value = i }
end
nodes[#nodes + 1] = ui.list{ id = "log", columns = { "Line" }, rows = {}, height = 6 }
ui.window(nodes)
