-- Into combat and a full-power swing at the first monster; while the power bar charges, the
-- second is selected. The swing is the plugin's only while its target is selected.
commands.add("fight", "swing at the first monster", function()
	task.spawn(function()
		pawait(combat.set_mode("combat"))
		print("attack", pawait(combat.attack(0x80000050, { power = 1.0 })))
	end)
end)
