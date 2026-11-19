-- A journey through a locked door: it stops at the door, saying which and why; the plugin looks at
-- the lock, picks it, opens the door and goes again, through it.
local DOOR, PICK = 0x80000050, 0x80000051
local BEYOND = { ns = 42.2, ew = 33.758333 }

commands.add("go", "travel through the door", function()
	task.spawn(function()
		local p = nav.path_to(BEYOND)
		local ok = pawait(p)
		local door = p.door or {}
		print("stopped", ok, door.object == DOOR, door.why, door.name, door.resist)
		local a = await(act.appraise(DOOR))
		print("appraised", a.ints[173])
		print("picked", (pawait(act.use_on(PICK, DOOR))))
		await(act.use(DOOR))
		for _ = 1, 25 do
			if game.world:get(DOOR).open then
				break
			end
			task.sleep(200)
		end
		print("opened", game.world:get(DOOR).open)
		local again = await(nav.path_to(BEYOND))
		print("through", again.distance <= 1)
	end)
end)
