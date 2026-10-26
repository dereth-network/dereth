-- Objects: what changed about one, where one walked, every property type, what the client knows of
-- a creature's health, and finding objects by their properties.
local RING, DRUDGE = 0x80000011, 0x80000021
on("world.object_changed", function(e)
	print("changed", string.format("0x%08X", e.id), table.concat(e.changed, ","))
end)
on("world.object_moved", function(e)
	print("moved", string.format("0x%08X", e.id), e.moved >= 1)
end)
local function ids(list)
	local out = {}
	for _, o in ipairs(list) do
		table.insert(out, string.format("0x%08X", o.id))
	end
	return table.concat(out, ",")
end
commands.add("look", "appraise and read", function()
	task.spawn(function()
		await(act.appraise(RING))
		await(act.appraise(DRUDGE))
		local ring, drudge = game.world:get(RING), game.world:get(DRUDGE)
		print("int64", ring:int64(6), "float", ring:float(167), "string", ring:str(16))
		print("bool", ring:bool(69), "did", string.format("0x%08X", ring:did(50)), "iid", ring:iid(2))
		print("prop", ring:prop("int", 105), ring:prop("did", 50) == ring:did(50))
		local h = drudge:health()
		print("health", h.current, h.max, h.fraction)
		print("spells", #ring.spells, ring.appraised)
		print("where", ids(game.world:find{ where = { { type = "int", key = 105, op = ">=", value = 5 } } }))
		print("string", ids(game.world:find{ where = { { type = "string", key = 16, op = "contains", value = "NOTE" } } }))
		print("class", ids(game.world:find{ class = "Monster" }), ids(game.world:find{ class = 4 }))
		print("nearest", ids(game.world:find{ sort = "distance", limit = 1, monster = true }))
		print("pred", ids(game.world:find({ container = game.player.id }, function(o) return o.value > 100 end)))
		local ok, err = pcall(function() return game.world:find{ where = { { type = "colour", key = 1 } } } end)
		print("bad", ok, string.find(tostring(err), "type is one of", 1, true) ~= nil)
	end)
end)
