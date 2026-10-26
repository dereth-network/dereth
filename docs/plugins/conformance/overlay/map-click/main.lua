-- A press on the game's map is heard with the place under the pointer, the button and the
-- modifiers held, and is no gesture. A left press marks the place on the map and points the arrow
-- at it.
on("map.clicked", function(e)
	local m = e.modifiers
	print(e.button, e.position.ns, e.position.ew, m.ctrl, m.shift, m.alt, permissions.activation() > 0)
	if e.button == "left" then
		markers.map:add({ points = { e.position }, label = "Here" })
		markers.arrow:set({ position = e.position, label = "Clicked" })
	end
end)
