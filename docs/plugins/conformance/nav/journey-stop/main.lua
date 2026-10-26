-- Two journeys: the plugin stops the first, the player's own movement ends the second.
local n = 0
commands.add("go", "travel to the hideout", function()
	n = n + 1
	local which = n == 1 and "first" or "second"
	task.spawn(function()
		print(which, pawait(nav.path_to({ cell = 0x019E0120, x = 20, y = -40, z = 0 })))
	end)
	if n == 1 then
		print("owner", nav.owner())
	end
end)
commands.add("halt", "stop", function()
	nav.stop()
	print("owner", nav.owner() or "none")
end)
