-- What stands in front, and a crowd: a ring and a world marker that stay on top, a ring that does
-- not, and a nametags layer that labels only its nearest two monsters, nearest first. `on_top`
-- that is not a boolean is refused, as is a limit on a layer that is not nametags.
on("session.login", function()
	draw.ring({ object = 0x80000021, radius = 2, on_top = true })
	draw.ring({ object = 0x80000022, radius = 2 })
	markers.world:add({ object = 0x80000023, label = "Far", on_top = true })
	draw.nametags({ monsters = true, within = 100, limit = 2 })
	local ok = pcall(draw.ring, { object = 0x80000021, limit = 3 })
	print("limit on a ring", ok)
	local ok2 = pcall(draw.ring, { object = 0x80000021, on_top = "yes" })
	print("on_top not a boolean", ok2)
end)
