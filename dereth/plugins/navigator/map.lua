-- The dungeon map: the floors of the dungeon the character is in, drawn from the game's data on a
-- canvas, its own level bright and the levels just above and below dim, with the character's dot
-- and heading, the target, the way there, and the creatures in sight.
--
-- A dungeon is read a few cells a step and kept, so no one step costs much; the outline of a
-- level is the edges of its floors that no other floor of it shares, worked out once for each
-- height the character stands at and drawn in the part of it around the character.

local M = {}

-- Pixels a side of the canvas, and pixels a meter.
M.SIZE = 320
M.SCALE = 3
-- Cells read a step, and the most lines drawn.
local CELLS_A_STEP = 6
local MAX_LINES = 900
-- A cell is on the character's level when the character stands within its height (from a little
-- under its floor to its ceiling), or its floor is this near the character's feet: a ramp's cell
-- and the room it leads into both count.
local NEAR_FLOOR = 2
-- Cells whose floor is within this far above or below the character's level are drawn dim.
local NEAR_LEVELS = 8

local LEVEL_COLOR = "#C8B070"
local NEAR_COLOR = "#4A4230"
local ROUTE_COLOR = "#40C0FF"
local TARGET_COLOR = "#FF60FF"
local CREATURE_COLOR = "#FF4040"
local ME_COLOR = "#40FF40"

local dungeon = nil -- { lb, ids, next, cells = { [id] = { floor_z, top_z, floors } }, outlines = {} }

function M.available()
	return host.has("game.data")
end

-- The dungeon the character is in, as a landblock id, or nil outdoors.
local function inside(me)
	if me == nil or me.ns ~= nil or me.cell == nil then
		return nil
	end
	if me.cell % 65536 < 0x0100 then
		return nil
	end
	return math.floor(me.cell / 65536)
end
M.inside = inside

-- Read a few more of the dungeon's cells. Answers whether every cell is read.
function M.step()
	local me = game.player.position
	local lb = inside(me)
	if lb == nil then
		return true
	end
	if dungeon == nil or dungeon.lb ~= lb then
		local block = game.data.landblock(lb)
		dungeon = { lb = lb, ids = block and block.cells or {}, next = 1, cells = {}, outlines = {} }
	end
	local d = dungeon
	local read = 0
	while d.next <= #d.ids and read < CELLS_A_STEP do
		local c = game.data.cell(d.ids[d.next])
		d.next = d.next + 1
		read = read + 1
		if c ~= nil and c.bounds ~= nil and #c.floor > 0 then
			d.cells[c.id] = { floor_z = c.bounds.min.z, top_z = c.bounds.max.z, floors = c.floor }
		end
	end
	if read > 0 then
		d.outlines = {}
	end
	return d.next > #d.ids
end

-- How many of the dungeon's cells are read, and how many it has.
function M.progress()
	if dungeon == nil then
		return 0, 0
	end
	return dungeon.next - 1, #dungeon.ids
end

-- Whether a cell (`{ floor_z, top_z }`) is on the level of a character whose feet are at `z`.
function M.on_level(c, z)
	return (z >= c.floor_z - 0.5 and z <= c.top_z) or math.abs(c.floor_z - z) <= NEAR_FLOOR
end
local on_level = M.on_level

-- A floor corner as a number, to a tenth of a meter.
local function key(x, y)
	return (math.floor(x * 10 + 0.5) + 100000) * 1000000 + math.floor(y * 10 + 0.5) + 100000
end

-- The outline of the cells `keep` picks: their floors' edges no other floor of them shares.
local function outline_of(keep)
	local count, edges = {}, {}
	for _, c in pairs(dungeon.cells) do
		if keep(c) then
			for _, poly in ipairs(c.floors) do
				for i = 1, #poly do
					local a, b = poly[i], poly[i % #poly + 1]
					local ka, kb = key(a.x, a.y), key(b.x, b.y)
					if kb < ka then
						ka, kb = kb, ka
					end
					local row = count[ka]
					if row == nil then
						row = {}
						count[ka] = row
					end
					if row[kb] == nil then
						row[kb] = 0
						table.insert(edges, { ka = ka, kb = kb, x1 = a.x, y1 = a.y, x2 = b.x, y2 = b.y })
					end
					row[kb] = row[kb] + 1
				end
			end
		end
	end
	local out = {}
	for _, e in ipairs(edges) do
		if count[e.ka][e.kb] == 1 then
			table.insert(out, e)
		end
	end
	return out
end

-- The outlines for a character whose feet are at `z`: its level's, and the levels near it.
local function outlines(z)
	local at = math.floor(z * 2 + 0.5)
	local have = dungeon.outlines[at]
	if have ~= nil then
		return have
	end
	have = {
		level = outline_of(function(c)
			return on_level(c, z)
		end),
		near = outline_of(function(c)
			return not on_level(c, z) and math.abs(c.floor_z - z) <= NEAR_LEVELS
		end),
	}
	dungeon.outlines[at] = have
	return have
end

-- The canvas's draw commands for the character's place now, or nil outdoors. `extra` may hold
-- `target` (a place `{ cell, x, y, z }`), `route` (a list of such places) and `creatures`
-- (positions), each drawn where it is in this dungeon.
function M.draw(extra)
	extra = extra or {}
	local me = game.player.position
	local lb = inside(me)
	if lb == nil or dungeon == nil or dungeon.lb ~= lb then
		return nil
	end
	local half, s = M.SIZE / 2, M.SCALE
	local function at(x, y)
		return half + (x - me.x) * s, half - (y - me.y) * s
	end
	local function here(p)
		return p ~= nil and p.cell ~= nil and p.ns == nil and math.floor(p.cell / 65536) == lb
	end
	local function seen(x, y)
		return x >= 0 and x <= M.SIZE and y >= 0 and y <= M.SIZE
	end
	local o = outlines(me.z or 0)
	-- The character's own level takes the lines first; the levels near it what is left, drawn
	-- under it.
	local count = 0
	local function lines_of(edges, color)
		local out = {}
		for _, e in ipairs(edges) do
			local x1, y1 = at(e.x1, e.y1)
			local x2, y2 = at(e.x2, e.y2)
			if (seen(x1, y1) or seen(x2, y2)) and count < MAX_LINES then
				count = count + 1
				table.insert(out, { op = "line", x1 = x1, y1 = y1, x2 = x2, y2 = y2, color = color })
			end
		end
		return out
	end
	local level = lines_of(o.level, LEVEL_COLOR)
	local cmds = lines_of(o.near, NEAR_COLOR)
	for _, c in ipairs(level) do
		table.insert(cmds, c)
	end
	-- The way to the target, a line from corner to corner.
	local route = extra.route or {}
	for i = 2, #route do
		local a, b = route[i - 1], route[i]
		if here(a) and here(b) then
			local x1, y1 = at(a.x, a.y)
			local x2, y2 = at(b.x, b.y)
			table.insert(cmds, { op = "line", x1 = x1, y1 = y1, x2 = x2, y2 = y2, color = ROUTE_COLOR, width = 2 })
		end
	end
	for _, c in ipairs(extra.creatures or {}) do
		if here(c) then
			local x, y = at(c.x, c.y)
			if seen(x, y) then
				table.insert(cmds, { op = "circle", x = x, y = y, r = 3, color = CREATURE_COLOR, fill = true })
			end
		end
	end
	if here(extra.target) then
		-- Off the canvas, the target is drawn at its edge, the way it lies.
		local x, y = at(extra.target.x, extra.target.y)
		x = math.max(5, math.min(M.SIZE - 5, x))
		y = math.max(5, math.min(M.SIZE - 5, y))
		table.insert(cmds, { op = "circle", x = x, y = y, r = 5, color = TARGET_COLOR })
	end
	-- The character: a dot, and a line the way it faces (heading clockwise from north).
	local h = math.rad(me.heading or 0)
	table.insert(cmds, { op = "circle", x = half, y = half, r = 4, color = ME_COLOR, fill = true })
	table.insert(cmds, { op = "line", x1 = half, y1 = half, x2 = half + math.sin(h) * 14, y2 = half - math.cos(h) * 14,
		color = ME_COLOR, width = 2 })
	local read, all = M.progress()
	table.insert(cmds, { op = "text", x = 4, y = 4, color = "#FFFFFF",
		text = string.format("0x%04X%s", dungeon.lb, read < all and string.format(" (%d/%d)", read, all) or "") })
	return cmds
end

return M
