-- A typed command walks the character to a place: it turns toward it, runs, and lets the run
-- lock go on arrival; the promise answers how far from the place it stopped.
commands.add("go", "walk east", function()
	task.spawn(function()
		local r = await(nav.move_to({ ns = 42.2, ew = 33.85 }, 2))
		print("arrived", r.distance <= 2)
		local p = game.player.position
		print("heading", math.floor(p.heading + 0.5))
	end)
end)
