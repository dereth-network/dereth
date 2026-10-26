-- The dungeon map: the floors of the dungeon the character is in, at its own level, drawn from the
-- game's data on a canvas with the character's dot and heading.
--
-- A dungeon is read a few cells a step and kept, so no one step costs much; the outline of a level
-- is the edges of its floors that no other floor shares, worked out once a level and drawn in the
-- part of it around the character.

local M = {}

-- Pixels a side of the canvas, and pixels a meter.
M.SIZE = 320
M.SCALE = 3
-- Cells read a step, and the most lines drawn.
local CELLS_A_STEP = 6
local MAX_LINES = 900
-- How far above or below the character's feet a floor still counts as its level, in meters.
local LEVEL = 3

local dungeon = nil -- { lb, ids, next, cells = { [id] = { floor_z, floors } }, outlines = {} }

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
			d.cells[c.id] = { floor_z = c.bounds.min.z, floors = c.floor }
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

-- A floor corner as a number, to a tenth of a meter.
local function key(x, y)
	return (math.floor(x * 10 + 0.5) + 100000) * 1000000 + math.floor(y * 10 + 0.5) + 100000
end

-- The outline of the level at height `z`: the floors' edges no other floor of the level shares.
local function outline(z)
	local level = math.floor(z / LEVEL + 0.5)
	local have = dungeon.outlines[level]
	if have ~= nil then
		return have
	end
	local count, edges = {}, {}
	for _, c in pairs(dungeon.cells) do
		if math.abs(c.floor_z - z) <= LEVEL then
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
	dungeon.outlines[level] = out
	return out
end

-- The canvas's draw commands for the character's place now, or nil outdoors.
function M.draw()
	local me = game.player.position
	if inside(me) == nil or dungeon == nil or dungeon.lb ~= inside(me) then
		return nil
	end
	local half, s = M.SIZE / 2, M.SCALE
	local function at(x, y)
		return half + (x - me.x) * s, half - (y - me.y) * s
	end
	local cmds = {}
	for _, e in ipairs(outline(me.z or 0)) do
		local x1, y1 = at(e.x1, e.y1)
		local x2, y2 = at(e.x2, e.y2)
		local seen = (x1 >= 0 and x1 <= M.SIZE and y1 >= 0 and y1 <= M.SIZE)
			or (x2 >= 0 and x2 <= M.SIZE and y2 >= 0 and y2 <= M.SIZE)
		if seen then
			table.insert(cmds, { op = "line", x1 = x1, y1 = y1, x2 = x2, y2 = y2, color = "#C8B070" })
			if #cmds >= MAX_LINES then
				break
			end
		end
	end
	-- The character: a dot, and a line the way it faces (heading clockwise from north).
	local h = math.rad(me.heading or 0)
	table.insert(cmds, { op = "circle", x = half, y = half, r = 4, color = "#40FF40", fill = true })
	table.insert(cmds, { op = "line", x1 = half, y1 = half, x2 = half + math.sin(h) * 14, y2 = half - math.cos(h) * 14,
		color = "#40FF40", width = 2 })
	local read, all = M.progress()
	table.insert(cmds, { op = "text", x = 4, y = 4, color = "#FFFFFF",
		text = string.format("0x%04X%s", dungeon.lb, read < all and string.format(" (%d/%d)", read, all) or "") })
	return cmds
end

return M
