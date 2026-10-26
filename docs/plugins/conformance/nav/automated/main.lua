-- With no gesture a movement needs the world's leave for navigation.
on("session.login", function()
	task.spawn(function()
		print("automated", pawait(nav.move_to({ ns = 42.3, ew = 33.8 })))
	end)
end)
