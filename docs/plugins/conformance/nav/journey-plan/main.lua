-- Plans, without moving: the journey into a dungeon, then the same with portals left out.
commands.add("plan", "plan a journey", function()
	task.spawn(function()
		local goal = { cell = 0x019E0120, x = 20, y = -40, z = 0 }
		local plan = await(nav.plan(goal))
		local kinds = {}
		for i, leg in ipairs(plan.legs) do
			kinds[i] = leg.kind
		end
		print("legs", table.concat(kinds, " "), plan.seconds > 0)
		print("without portals", pawait(nav.plan(goal, { portals = false })))
	end)
end)
