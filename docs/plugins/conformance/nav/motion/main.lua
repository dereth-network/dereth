-- Held motions: two seconds stepping to the right, a second walking forward, and a motion this
-- host does not have.
local function moved(a, b)
	local dn, de = (b.ns - a.ns) * 240, (b.ew - a.ew) * 240
	return math.sqrt(dn * dn + de * de)
end

commands.add("side", "two seconds to the right", function()
	task.spawn(function()
		local from = game.player.position
		local ok, r = pawait(nav.motion("strafe_right", 2000))
		local d = moved(from, game.player.position)
		print("side", ok, r.held >= 2000 and r.held < 2100, d > 2 and d < 3)
	end)
end)

commands.add("stroll", "a second's walk forward", function()
	task.spawn(function()
		local from = game.player.position
		local ok = pawait(nav.motion("forward", 1000, { walk = true }))
		local d = moved(from, game.player.position)
		print("stroll", ok, d > 1 and d < 1.6)
	end)
end)

commands.add("fly", "no such motion", function()
	print("fly", (pcall(nav.motion, "fly", 100)))
	print("forever", (pcall(nav.motion, "forward", 60000)))
end)
