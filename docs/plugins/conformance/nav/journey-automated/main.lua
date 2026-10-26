-- With no gesture a journey needs the world's leave for navigation.
on("session.login", function()
	task.spawn(function()
		print("automated", pawait(nav.path_to({ cell = 0x019E0120, x = 20, y = -40, z = 0 })))
	end)
end)
