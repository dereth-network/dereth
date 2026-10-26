-- An item used on a target: a wielded caster's own spell cast on the character (the caster kept
-- in hand), a healing kit from the pack used on the character, a lockpick used on a door, and the
-- caster used on what its spell does not take, which the client refuses with nothing sent.
local STEIN, KIT, PICK, DOOR = 0x80000040, 0x80000041, 0x80000042, 0x80000043
commands.add("go", "use items on targets", function()
	task.spawn(function()
		local me = game.player.id
		print("stein", pawait(act.use_on(STEIN, me, { purpose = "heal" })))
		print("still wielded", game.world:get(STEIN).wielder == me)
		print("kit", pawait(act.use_on(KIT, me)))
		print("pick", pawait(act.use_on(PICK, DOOR)))
		print("stein on the door", pawait(act.use_on(STEIN, DOOR)))
		local ok, why = pcall(act.use_on, STEIN, me, { purpose = "buff" })
		print("purpose", ok, string.find(tostring(why), "a use's purpose is \"heal\", not \"buff\"", 1, true) ~= nil)
	end)
end)
