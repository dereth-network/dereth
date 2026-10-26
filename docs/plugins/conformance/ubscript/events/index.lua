-- UtilityBelt's events over the plugin interface's: objects coming and going, the selection, a
-- container opened, chat (a tell, speech near by, an emote) with the speaker and the words, a line
-- eaten, a handler that waits, `Once` and `Until`, and the world's tick.
local CORPSE = 0x80000041
game.World.OnObjectCreated.Add(function(e)
	local o = game.World.Get(e.ObjectId)
	print("created", o.Name, o.ObjectClass)
end)
game.World.OnObjectReleased.Add(function(e)
	print("released", string.format("0x%08X", e.ObjectId))
end)
game.World.OnObjectSelected.Add(function(e)
	print("selected", string.format("0x%08X", e.ObjectId), game.World.Selected.Name)
end)
game.World.OnContainerOpened.Add(function(e)
	print("opened", e.Container.Name, #e.Container.Items)
end)
game.World.OnTell.Add(function(e)
	print("tell", e.SenderName, e.Message, e.Type, string.format("0x%08X", e.SenderId))
	if e.Message == "hush" then
		e.Eat = true
	end
end)
game.World.OnLocalMessage.Add(function(e)
	print("local", e.SenderName, e.Message)
end)
game.World.OnEmote.Add(function(e)
	print("emote", e.SenderName, e.Message)
end)
local lines = 0
game.World.OnChatText.Add(function(e)
	lines = lines + 1
end)
game.World.OnTell.Once(function(e)
	print("once", e.Message)
end)
game.World.OnTell.Until(function(e)
	print("until", e.Message)
	return e.Message == "stop"
end)
-- A handler that waits: the next tell is heard while it sleeps.
game.World.OnTell.Add(function(e)
	if e.Message == "wait" then
		sleep(1000)
		print("waited", e.Message)
	end
end)
local ticks = 0
game.World.OnTick.Add(function()
	ticks = ticks + 1
end)
game.OnScriptEnd.Add(function()
	print("end")
end)
sleep(9000)
print("lines", lines, ticks >= 8)
game.Actions.ObjectUse(CORPSE)
