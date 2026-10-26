-- The corpse tracker: every corpse seen this session, whose it is and where it lies, and the
-- character's own corpse, kept with the character so it can be found again after a death.

local util = require("util")

local M = {}

-- Newest first: { id, name, who, whose, place, at, gone }. At most KEEP of them.
M.list = {}
local KEEP = 50

-- Whose a corpse is: yours, a fellow's, a player's, your kill's or another's. `kinds` answers
-- what the other trackers know: { me, fellows (names), players (names), killed (name -> bool) }.
local function whose(who, kinds)
	if who == kinds.me then
		return "yours"
	elseif kinds.fellows[who] then
		return "fellow"
	elseif kinds.players[who] then
		return "player"
	elseif kinds.killed(who) then
		return "your kill"
	end
	return "other"
end

-- A corpse came into view: noted, and answered with whose it is.
function M.seen(id, name, kinds)
	local who = string.match(name or "", "^Corpse of (.+)$") or name or "?"
	local obj = game.world:get(id)
	local c = {
		id = id,
		name = name,
		who = who,
		whose = whose(who, kinds),
		place = util.place(obj and obj.position),
		at = os.clock(),
	}
	table.insert(M.list, 1, c)
	while #M.list > KEEP do
		table.remove(M.list)
	end
	if c.whose == "yours" then
		M.mine = { name = name, place = c.place, at = os.time() }
		pcall(function()
			store.character.corpse = M.mine
		end)
	end
	return c
end

-- A corpse left view: kept, with where it was.
function M.gone(id)
	for _, c in ipairs(M.list) do
		if c.id == id then
			c.gone = true
			return true
		end
	end
	return false
end

-- The character's own corpse from the store, after a login or a reload.
function M.load()
	pcall(function()
		M.mine = store.character.corpse
	end)
end

-- The rows: { corpse, whose, where, away }.
function M.rows()
	local rows = {}
	for _, c in ipairs(M.list) do
		table.insert(rows, { c.who, c.whose, util.coords(c.place), c.gone and "out of view" or util.away(c.place) })
	end
	return rows
end

return M
