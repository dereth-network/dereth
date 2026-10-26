-- Markers of each kind: a world marker and the arrow on an object, a radar blip at a place, a
-- line on the map; the blip is removed by the plugin and a timed marker goes by itself.
on("session.login", function()
	markers.world:add({ object = 0x80000030, label = "Signpost", color = "#00FF00" })
	local blip = markers.radar:add({ position = { ns = 42.25, ew = 33.8 }, label = "Camp" })
	markers.map:add({ points = { { ns = 42.2, ew = 33.8 }, { ns = 42.3, ew = 33.9 } }, line = true, label = "Road" })
	markers.arrow:set({ object = 0x80000030, label = "Signpost" })
	markers.world:add({ position = { ns = 42.21, ew = 33.81 }, label = "Brief", expires = 1 })
	task.after(2000, function()
		print("removed", blip:remove())
	end)
	print("distance", game.world:get(0x80000030):distance())
end)
