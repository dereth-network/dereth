-- A plugin active only in the world: at the character list it hears the session but no ticks, and
-- its window is hidden; in the world it is active; after it logs out it is idle again.
ui.window{ id = "main", title = "World only", ui.label{ id = "where", text = "here" } }
local entered = false
task.every(500, function()
	print("tick", session.info().context)
end)
on("context.begin", function(e)
	print("begin", e.context, e.previous)
end)
on("context.end", function(e)
	print("end", e.context, e.next)
end)
on("session.pregame", function()
	print("pregame", session.info().context)
	if not entered then
		entered = true
		session.select_character("Alpha")
	end
end)
on("session.login", function(e)
	print("in world as", e.character)
	task.after(1200, function()
		session.logout()
	end)
end)
on("session.logout", function()
	print("logged out")
end)
