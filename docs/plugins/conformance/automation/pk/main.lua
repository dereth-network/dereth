-- A world that stops automation while the character is a player killer: an automated swing is
-- refused, a swing the player asked for is not.
on("session.login", function()
	task.spawn(function()
		print("automated", pawait(combat.set_mode("combat")))
	end)
end)
commands.add("fight", "enter combat", function()
	task.spawn(function()
		print("asked", pawait(combat.set_mode("combat")))
	end)
end)
