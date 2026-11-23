-- A typed command runs the character through places that bend gently north-east: each is told
-- in order as it is reached, the run goes on through the bends and into the next movement with
-- the forward key held, never the run lock, and one that something stands before fails.
local points = {
	{ ns = 42.24, ew = 33.8 },
	{ ns = 42.28, ew = 33.81 },
	{ ns = 42.32, ew = 33.83 },
	{ ns = 42.36, ew = 33.86 },
}
commands.add("go", "run through four places", function()
	print("none", (pcall(nav.move_along, {})))
	task.spawn(function()
		local r = await(nav.move_along(points, {
			range = 1,
			on_point = function(i, p)
				local me = game.player.position
				-- Within range, and what the event's frame and the place's rounding add.
				local near = math.sqrt(((me.ns - p.ns) * 240) ^ 2 + ((me.ew - p.ew) * 240) ^ 2) <= 1.5
				print("reached", i, near)
			end,
		}))
		print("arrived", r.points, r.distance <= 1)
		print("beyond the wall", pawait(nav.move_along({ { ns = 42.4, ew = 33.875 } }, { range = 1 })))
	end)
end)
