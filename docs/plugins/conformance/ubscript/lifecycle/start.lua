-- A script written for UtilityBelt's scripting in a package that names it (`ubscript = true`),
-- active before the world: it starts at the character list, logs a character in, hears the
-- client's state change, and hears its own end when it is unloaded.
local helper = require("helper")
while #game.Characters == 0 do
	sleep(100)
end
print("state", game.State, #game.Characters, game.Characters[2].Name, game.Character == nil)
game.OnStateChanged.Add(function(e)
	print("now", e.NewState)
end)
game.OnScriptEnd.Add(function()
	print("end", helper.name())
end)
local res = await(game.Actions.Login(game.Characters[2].Id))
print("login", res.Success, game.State, game.Character.Weenie.Name)
