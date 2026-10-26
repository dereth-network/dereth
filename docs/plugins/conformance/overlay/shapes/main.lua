-- Drawing in the world: a line from the character to a monster, a ring about the monster, a square
-- at a place, text floating over a place, and the nametags layer over monsters in range. A shape
-- that names no shape, and a line with no other end, are refused.
on("session.login", function()
	draw.line({ object = game.player.id, to_object = 0x80000021, label = "Target", width = 3 })
	draw.ring({ object = 0x80000021, radius = 3, color = "#FF0000" })
	draw.square({ position = { ns = 42.21, ew = 33.8 }, radius = 2 })
	draw.text({ position = { ns = 42.2, ew = 33.85 }, label = "Camp" })
	draw.nametags({ monsters = true, health = true, within = 40 })
	local ok = pcall(draw.line, { position = { ns = 42.2, ew = 33.8 } })
	print("line without an end", ok)
	local ok2 = pcall(markers.world.add, markers.world, { shape = "ring", position = { ns = 42.2, ew = 33.8 } })
	print("shape on a world marker", ok2)
end)
