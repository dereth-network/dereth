-- With no gesture behind it an action needs the world's leave for its domain: this world allows
-- crafting only with a gesture and denies vendors outright, so both fail their promises. A
-- character's advancement is refused outright without a gesture, whatever the world says; a
-- command the player types opens a thirty-second activation in which actions are interactive.
on("session.login", function()
	local ok, err = pcall(character.raise_skill, 6)
	print("automated raise", ok, string.find(tostring(err), "click or command", 1, true) ~= nil)
	print("activation", permissions.activation())
	task.spawn(function()
		print("automated move", pawait(items.move(0x80000021, 0x50000001)))
	end)
	task.spawn(function()
		print("automated vendor", pawait(vendor.close()))
	end)
end)
commands.add("go", "move the stone", function()
	print("activation live", permissions.activation() > 29)
	task.spawn(function()
		print("interactive move", pawait(items.move(0x80000021, 0x50000001)))
	end)
	task.spawn(function()
		print("interactive vendor", pawait(vendor.close()))
	end)
end)
