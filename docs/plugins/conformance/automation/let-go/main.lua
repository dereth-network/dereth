-- Stopping is always allowed: after the player's command has run out, in a world that stops a
-- player killer's automation, the plugin still stops the attack it started and lets go of the run
-- lock it set. Starting again is an action like any other, and the world refuses it.
commands.add("fight", "fight the skulker and run", function()
	task.spawn(function()
		await(combat.set_mode("combat"))
		print("attack", combat.attack(0x80000050, { power = 0 }) ~= nil)
		print("run", pawait(nav.autorun(true)))
	end)
	task.after(31000, function()
		task.spawn(function()
			print("activation", permissions.activation())
			print("stop", pawait(combat.stop()))
			print("run off", pawait(nav.autorun(false)))
			print("run again", pawait(nav.autorun(true)))
		end)
	end)
end)
