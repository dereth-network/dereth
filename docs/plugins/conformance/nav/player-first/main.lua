-- A long walk the player cuts short with a movement key of their own.
commands.add("go", "walk east", function()
	task.spawn(function()
		print("move", pawait(nav.move_to({ ns = 42.2, ew = 34.2 }, 1)))
	end)
end)
