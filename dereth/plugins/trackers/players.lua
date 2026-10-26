-- The player tracker: the players seen this session, when and where each was last seen, and
-- where one was when it went out of view (a portal or a recall taking it away, or the character
-- moving on).

local util = require("util")

local M = {}

-- Id -> { name, first, last, place, here }
M.seen = {}
local KEEP = 100

-- A player came into view, or moved while in view.
function M.saw(id)
	local obj = game.world:get(id)
	if obj == nil then
		return
	end
	local p = M.seen[id]
	if p == nil then
		p = { name = obj.name or "?", first = os.clock() }
		M.seen[id] = p
	end
	p.last = os.clock()
	p.here = true
	p.place = util.place(obj.position) or p.place
	-- Only the most recently seen are kept.
	local count, oldest, oldest_at = 0, nil, math.huge
	for pid, q in pairs(M.seen) do
		count = count + 1
		if not q.here and q.last < oldest_at then
			oldest, oldest_at = pid, q.last
		end
	end
	if count > KEEP and oldest ~= nil then
		M.seen[oldest] = nil
	end
end

-- Every player in view now (at the start of a session, when nothing has come into view yet).
function M.scan()
	local me = game.player.id
	for _, obj in ipairs(game.world:find({ class = "Player" })) do
		if obj.id ~= me then
			M.saw(obj.id)
		end
	end
end

-- A player went out of view: kept where it was last seen.
function M.left(id)
	local p = M.seen[id]
	if p ~= nil then
		p.here = false
		p.last = os.clock()
		return p
	end
	return nil
end

-- The names of the players seen.
function M.names()
	local out = {}
	for _, p in pairs(M.seen) do
		out[p.name] = true
	end
	return out
end

-- The rows, most recently seen first: { player, seen, where, away }.
function M.rows()
	local list = {}
	for _, p in pairs(M.seen) do
		table.insert(list, p)
	end
	table.sort(list, function(a, b)
		if a.here ~= b.here then
			return a.here
		end
		return a.last > b.last or (a.last == b.last and a.name < b.name)
	end)
	local rows = {}
	for _, p in ipairs(list) do
		local when = p.here and "here" or (util.duration(os.clock() - p.last) .. " ago")
		table.insert(rows, { p.name, when, util.coords(p.place), p.here and util.away(p.place) or "-" })
	end
	return rows
end

return M
