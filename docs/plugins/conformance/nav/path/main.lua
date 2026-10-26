-- Asks the client for walking routes, without moving: round a wall to a monster behind it, through
-- a closed door in a gap of another wall, and to a place walled in; then looks along straight lines:
-- through the wall, through the closed door, and across open ground.
local MONSTER, DOOR = 0x80000030, 0x80000031

commands.add("route", "ask for routes and lines of sight", function()
	task.spawn(function()
		local round = await(nav.path(MONSTER))
		print("round", #round.points >= 3, round.length > 12, round.length < 20, round.complete, #round.doors)
		local door = await(nav.path({ ns = 42.2, ew = 33.758333 }))
		print("door", #door.points, #door.doors, door.doors[1].object == DOOR, door.doors[1].at)
		print("walled in", pawait(nav.path({ ns = 42.283333, ew = 33.8 })))
		print("sight", nav.sight(MONSTER), nav.sight({ ns = 42.2, ew = 33.758333 }), nav.sight({ ns = 42.158333, ew = 33.8 }))
		local p = game.player.position
		print("still", math.abs(p.ns - 42.2) < 0.0001, math.abs(p.ew - 33.8) < 0.0001)
	end)
end)
