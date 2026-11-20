-- A journey under way whose plugin draws its way itself: a path along the journey's way still to
-- walk, in place of the host's own trail.
commands.add("go", "travel to the hideout", function()
	nav.path_to({ cell = 0x019E0120, x = 20, y = -40, z = 0 })
end)
commands.add("draw", "draw the way", function()
	local way = nav.journey().way
	local points = { game.player.position }
	for _, p in ipairs(way) do
		table.insert(points, p)
	end
	draw.path({ points = points, look = "ribbon" })
end)
