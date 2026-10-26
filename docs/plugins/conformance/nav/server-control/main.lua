-- A long walk during which the server takes control of the character (to walk it to something the
-- player used, say).
commands.add("go", "walk east", function()
	task.spawn(function()
		print("move", pawait(nav.move_to({ ns = 42.2, ew = 34.2 }, 1)))
	end)
end)
