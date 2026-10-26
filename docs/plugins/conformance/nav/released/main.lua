-- A long walk, and the player unloads the plugin halfway.
commands.add("go", "walk east", function()
	task.spawn(function()
		print("move", pawait(nav.move_to({ ns = 42.2, ew = 34.2 }, 1)))
	end)
end)
