-- A typed command walks the character east by three moves, each asked for when the one before
-- arrives: the run lock is kept across them, and let go after the last.
commands.add("go", "walk east in three moves", function()
	task.spawn(function()
		for i, ew in ipairs({ 33.82, 33.84, 33.86 }) do
			await(nav.move_to({ ns = 42.2, ew = ew }, 1))
			print("arrived", i)
		end
	end)
end)
