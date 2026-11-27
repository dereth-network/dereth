-- The dungeon map: the floors of the dungeon the character is in, drawn from the game's data on a
-- canvas, its own level bright and the levels just above and below dim, with the character's dot
-- and heading, the target, the way there, the creatures in sight, where portals into the dungeon
-- set a character down, and the portals inside it, each named; the dungeon's own name over it.
--
-- A dungeon is read a few cells a step and kept, so no one step costs much; the outline of a
-- level is the edges of its floors that no other floor of it shares, worked out once for each
-- height the character stands at and drawn in the part of it in view. A bar says how much is
-- built while it is read. The view follows the character, zoomed by the wheel and dragged aside
-- by the pointer; Reset puts it back.

local M = {}

-- Pixels a side of the canvas, and pixels a meter.
M.SIZE = 320
M.SCALE = 3
-- Cells read a step.
local CELLS_A_STEP = 6
-- While the dungeon is read, its outlines are worked out again at most this often, seconds (and
-- once all are read). They are worked out apart from drawing, this many cells a turn, so no one
-- turn of the plugin's costs much; the map is drawn with the last outlines until the new are done.
local OUTLINE_EVERY = 2
local CELLS_A_TURN = 100
-- The most lines drawn: every command a canvas is handed costs the plugin time, and a whole
-- dungeon zoomed out has thousands of edges. Zoomed out, an edge shorter than `SHORTEST` pixels is
-- left out: it would draw as a dot.
local MAX_LINES = 500
local SHORTEST = 1.5
-- The most commands a canvas holds.
local MAX_COMMANDS = 1000
-- A cell is on the character's level when the character stands within its height (from a little
-- under its floor to its ceiling), or its floor is this near the character's feet: a ramp's cell
-- and the room it leads into both count.
local NEAR_FLOOR = 2
-- Cells whose floor is within this far above or below the character's level are drawn dim.
local NEAR_LEVELS = 8

-- How far one notch of the wheel zooms, and the least and most zoom.
local ZOOM_STEP = 1.25
local ZOOM_MIN, ZOOM_MAX = 0.25, 8

local LEVEL_COLOR = "#C8B070"
local NEAR_COLOR = "#4A4230"
local ROUTE_COLOR = "#40C0FF"
local TARGET_COLOR = "#FF60FF"
local CREATURE_COLOR = "#FF4040"
local ME_COLOR = "#40FF40"
local DROP_COLOR = "#40E0E0"
local PORTAL_COLOR = "#C080FF"
local BAR_COLOR = "#C8B070"

local dungeon = nil -- { lb, ids, next, cells = { [id] = { floor_z, top_z, floors } }, outlines = {} }

-- The view: how far it is zoomed, and how far its middle stands from the character (meters east and
-- north).
local view = { zoom = 1, east = 0, north = 0 }

-- Back to the character, unzoomed.
function M.reset()
	view = { zoom = 1, east = 0, north = 0 }
end

-- Zoom by `notches` of the wheel (up in) about the canvas point `x`, `y`, which keeps the place under
-- it.
function M.zoom_at(notches, x, y)
	local before = view.zoom
	local after = math.max(ZOOM_MIN, math.min(ZOOM_MAX, before * ZOOM_STEP ^ notches))
	local half = M.SIZE / 2
	-- The place under the pointer, meters from the view's middle, at each zoom.
	local ex, nx = (x - half) / (M.SCALE * before), (half - y) / (M.SCALE * before)
	view.east = view.east + ex - ex * before / after
	view.north = view.north + nx - nx * before / after
	view.zoom = after
end

-- Drag the map by `dx`, `dy` canvas pixels: the floors follow the pointer.
function M.pan(dx, dy)
	local s = M.SCALE * view.zoom
	view.east = view.east - dx / s
	view.north = view.north + dy / s
end

-- The view as a string, which changes whenever it or the outlines drawn do.
function M.view_key()
	return string.format("%.3f %.2f %.2f %d", view.zoom, view.east, view.north, dungeon and dungeon.version or 0)
end

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
		d.stale = true
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

-- The outline of those of `cells` that `keep` picks: their floors' edges no other floor of them
-- shares. `turn()` is called after each cell.
local function outline_of(cells, keep, turn)
	local count, edges = {}, {}
	for _, c in ipairs(cells) do
		turn()
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

local NO_OUTLINES = { level = {}, near = {} }

-- Work out the outlines of dungeon `d` at height `z` (kept as `at`) in a task of their own, a few
-- cells a turn, over the cells read when it starts.
local function work_out(d, z, at)
	d.working = true
	local cells = {}
	for _, c in pairs(d.cells) do
		table.insert(cells, c)
	end
	task.spawn(function()
		local n = 0
		local function turn()
			n = n + 1
			if n % CELLS_A_TURN == 0 then
				task.sleep(0)
			end
		end
		local got = {
			level = outline_of(cells, function(c)
				return on_level(c, z)
			end, turn),
			near = outline_of(cells, function(c)
				return not on_level(c, z) and math.abs(c.floor_z - z) <= NEAR_LEVELS
			end, turn),
		}
		d.outlines[at], d.last = got, got
		d.version = (d.version or 0) + 1
		d.working = false
	end)
end

-- The outlines for a character whose feet are at `z`: its level's, and the levels near it; the
-- last worked out while new ones are.
local function outlines(z)
	local d = dungeon
	-- Cells read since they were worked out: worked out again once all are read, or a while after.
	if d.stale and not d.working and (d.next > #d.ids or os.clock() - (d.outlined or -math.huge) >= OUTLINE_EVERY) then
		d.outlines, d.stale, d.outlined = {}, false, os.clock()
	end
	local at = math.floor(z * 2 + 0.5)
	local have = d.outlines[at]
	if have == nil and not d.working then
		work_out(d, z, at)
	end
	return have or d.last or NO_OUTLINES
end

-- The canvas's draw commands for the character's place now, or nil outdoors. `extra` may hold
-- `target` (a place `{ cell, x, y, z }`), `route` (a list of such places), `creatures`
-- (positions), `marks` (`{ kind = "drop" | "portal", name, at }`, a place each) and `name` (the
-- dungeon's), each drawn where it is in this dungeon.
function M.draw(extra)
	extra = extra or {}
	local me = game.player.position
	local lb = inside(me)
	if lb == nil or dungeon == nil or dungeon.lb ~= lb then
		return nil
	end
	local half, s = M.SIZE / 2, M.SCALE * view.zoom
	local cx, cy = me.x + view.east, me.y + view.north
	local function at(x, y)
		return half + (x - cx) * s, half - (y - cy) * s
	end
	-- A point off the canvas, drawn at its edge, the way it lies.
	local function edge(x, y)
		return math.max(5, math.min(M.SIZE - 5, x)), math.max(5, math.min(M.SIZE - 5, y))
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
	local shortest = view.zoom < 1 and SHORTEST or 0
	local function lines_of(edges, color)
		local out = {}
		for _, e in ipairs(edges) do
			local x1, y1 = at(e.x1, e.y1)
			local x2, y2 = at(e.x2, e.y2)
			if (seen(x1, y1) or seen(x2, y2)) and count < MAX_LINES
				and math.abs(x2 - x1) + math.abs(y2 - y1) >= shortest then
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
	-- Where portals into the dungeon set a character down (a square), and the portals inside it
	-- (a ring), each with its name.
	for _, m in ipairs(extra.marks or {}) do
		if here(m.at) then
			local x, y = at(m.at.x, m.at.y)
			if seen(x, y) then
				local color = m.kind == "drop" and DROP_COLOR or PORTAL_COLOR
				if m.kind == "drop" then
					table.insert(cmds, { op = "rect", x = x - 4, y = y - 4, w = 8, h = 8, color = color, fill = true })
				else
					table.insert(cmds, { op = "circle", x = x, y = y, r = 5, color = color })
					table.insert(cmds, { op = "circle", x = x, y = y, r = 2, color = color, fill = true })
				end
				table.insert(cmds, { op = "text", x = x + 8, y = y - 6, text = m.name, color = color })
			end
		end
	end
	if here(extra.target) then
		local x, y = edge(at(extra.target.x, extra.target.y))
		table.insert(cmds, { op = "circle", x = x, y = y, r = 5, color = TARGET_COLOR })
	end
	-- The character: a dot, and a line the way it faces (heading clockwise from north); a ring at
	-- the canvas's edge when the view is dragged away from it.
	local h = math.rad(me.heading or 0)
	local mx, my = at(me.x, me.y)
	if seen(mx, my) then
		table.insert(cmds, { op = "circle", x = mx, y = my, r = 4, color = ME_COLOR, fill = true })
		table.insert(cmds, { op = "line", x1 = mx, y1 = my, x2 = mx + math.sin(h) * 14, y2 = my - math.cos(h) * 14,
			color = ME_COLOR, width = 2 })
	else
		mx, my = edge(mx, my)
		table.insert(cmds, { op = "circle", x = mx, y = my, r = 4, color = ME_COLOR })
	end
	table.insert(cmds, { op = "text", x = 4, y = 4, color = "#FFFFFF", text = extra.name or string.format("0x%04X", dungeon.lb) })
	-- While the dungeon is read: a bar of how much is built.
	local read, all = M.progress()
	if read < all then
		local w = M.SIZE - 16
		table.insert(cmds, { op = "rect", x = 8, y = M.SIZE - 20, w = w, h = 10, color = "#000000", fill = true })
		table.insert(cmds, { op = "rect", x = 8, y = M.SIZE - 20, w = math.floor(w * read / math.max(1, all)), h = 10,
			color = BAR_COLOR, fill = true })
		table.insert(cmds, { op = "rect", x = 8, y = M.SIZE - 20, w = w, h = 10, color = BAR_COLOR })
		table.insert(cmds, { op = "text", x = 8, y = M.SIZE - 36, text = "Building...", color = "#FFFFFF" })
	end
	-- Within what a canvas holds: the dimmest lines, drawn first, give way.
	local over = #cmds - MAX_COMMANDS
	if over > 0 then
		table.move(cmds, over + 1, #cmds, 1)
		for i = #cmds, #cmds - over + 1, -1 do
			cmds[i] = nil
		end
	end
	return cmds
end

return M
