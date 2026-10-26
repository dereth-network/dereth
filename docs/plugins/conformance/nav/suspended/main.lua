-- A long walk, and then a handler that always fails: after ten failures within a minute the plugin
-- is suspended and can no longer stop its own walk.
commands.add("go", "walk east", function()
	task.spawn(function()
		print("move", pawait(nav.move_to({ ns = 42.2, ew = 34.2 }, 1)))
	end)
	task.after(1000, function()
		task.every(100, function()
			error("always")
		end)
	end)
end)
