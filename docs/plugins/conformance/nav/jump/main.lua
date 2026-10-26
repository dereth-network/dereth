-- Jumps: one charged half a second toward the east, walking and forward; one straight up at once;
-- one the player cuts short while it charges; one charged longer than the power bar runs.
local function moved(a, b)
	local dn, de = (b.ns - a.ns) * 240, (b.ew - a.ew) * 240
	return math.sqrt(dn * dn + de * de)
end

commands.add("hop", "a jump to the east", function()
	task.spawn(function()
		local from = game.player.position
		local ok, r = pawait(nav.jump({ heading = 90, ms = 500, walk = true, direction = "forward" }))
		local d = moved(from, game.player.position)
		print("hop", ok, r.held >= 500 and r.held < 600, r.heading, d > 2 and d < 3)
	end)
end)

commands.add("up", "a jump straight up", function()
	task.spawn(function()
		local from = game.player.position
		local ok = pawait(nav.jump())
		print("up", ok, moved(from, game.player.position) < 0.1)
	end)
end)

commands.add("charge", "a full charge", function()
	task.spawn(function()
		print("charge", pawait(nav.jump({ ms = 1000 })))
	end)
end)

commands.add("long", "a charge past the bar", function()
	print("long", (pcall(nav.jump, { ms = 1500 })))
	print("sideways", (pcall(nav.jump, { direction = "up" })))
end)
