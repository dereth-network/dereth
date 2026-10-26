-- A journey under way, drawn by the host, and read back.
commands.add("go", "travel to the hideout", function()
	nav.path_to({ cell = 0x019E0120, x = 20, y = -40, z = 0 })
end)
commands.add("look", "read the journey", function()
	local j = nav.journey()
	print("leg", j.leg, j.legs[j.leg].kind)
end)
