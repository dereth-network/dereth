-- Into combat and a swing at the target; the player unloads the plugin while the swing is out.
commands.add("fight", "swing at the target", function()
	task.spawn(function()
		pawait(combat.set_mode("combat"))
		combat.attack(0x80000050, { power = 0.2 })
	end)
end)
