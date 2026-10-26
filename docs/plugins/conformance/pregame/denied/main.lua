-- A world whose policy denies session control refuses a plugin's entry.
on("session.pregame", function()
	print("control", permissions.has("session.control"))
	task.spawn(function()
		print("entry", pawait(session.select_character("Alpha")))
	end)
end)
