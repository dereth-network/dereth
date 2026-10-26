-- A plugin loaded before login picks a character from the list, and tries again when the
-- server says the character is still in the world.
local tries = 0
local function enter()
	tries = tries + 1
	task.spawn(function()
		local ok, why = pawait(session.select_character("Bravo"))
		print("entry", tries, ok, ok and why.name or why)
	end)
end
on("session.pregame", function(e)
	print("pregame", #e.characters, e.characters[2].name, session.info().phase)
	if tries < 3 then
		task.after(1000, enter)
	end
end)
on("session.login_failed", function(e)
	print("refused", e.code, e.reason)
end)
on("session.login", function(e)
	print("in world as", e.character)
end)
