-- The mana tracker: each item the character wears or wields, its mana, and how long that lasts.
--
-- An item's appraisal says its mana now and at most (integer properties 107 and 108), the mana it
-- burns (float property 5, points a second, negative while it drains) and its spells. Worn items
-- are appraised a few at a time, again every few minutes, and the mana between appraisals is
-- worked out from the burn. An item whose spells show among the character's enchantments is
-- active; one with spells and mana but none showing, inactive; one with spells and no mana, out;
-- one with no spells, none; one not yet appraised, unknown.

local M = {}

local CUR_MANA, MAX_MANA, MANA_RATE = 107, 108, 5
-- How long an appraisal stands before the item is appraised again.
local LOOK_AGAIN = 180

-- Item id -> { name, cur, max, rate, spells, at }
M.items = {}
-- Minutes left under which an item is warned of, and the items already warned of.
M.warn_minutes = 20
local warned = {}
-- The last "low on mana" lines the server sent, newest last.
M.notices = {}

-- The character's worn and wielded items.
local function worn()
	local me = game.player.id
	local out = {}
	for _, it in ipairs(game.inventory.items()) do
		if it.wielder == me then
			table.insert(out, it)
		end
	end
	return out
end

-- Appraise up to `count` worn items whose appraisal is missing or old; forget items no longer
-- worn. Answers the items to look at, for a task to appraise.
function M.due(count)
	local now = os.clock()
	local seen, due = {}, {}
	for _, it in ipairs(worn()) do
		seen[it.id] = true
		local m = M.items[it.id]
		if m == nil then
			M.items[it.id] = { name = it.name, at = nil }
			m = M.items[it.id]
		end
		if (m.at == nil or m.stale or now - m.at > LOOK_AGAIN) and #due < count and not m.asking then
			table.insert(due, it)
		end
	end
	for id in pairs(M.items) do
		if not seen[id] then
			M.items[id] = nil
			warned[id] = nil
		end
	end
	return due
end

-- Appraise one item and keep what it says. Inside a task.
function M.look(it)
	local m = M.items[it.id]
	if m == nil then
		return
	end
	m.asking = true
	local ok, a = pawait(act.appraise(it.id))
	m.asking = false
	if not ok or a == nil then
		return
	end
	local ints, floats = a.ints or {}, a.floats or {}
	m.cur, m.max, m.rate = ints[CUR_MANA], ints[MAX_MANA], floats[MANA_RATE]
	m.spells = a.spells or {}
	m.at = os.clock()
	m.stale = nil
end

-- An item's mana now: the last appraisal's less what it has burned since.
function M.mana_now(m)
	if m.cur == nil then
		return nil
	end
	local burn = math.abs(m.rate or 0)
	return math.max(0, m.cur - burn * (os.clock() - (m.at or os.clock())))
end

-- Seconds of mana an item has left at its burn, or nil when it burns none.
function M.time_left(m)
	local now = M.mana_now(m)
	local burn = math.abs(m.rate or 0)
	if now == nil or burn <= 0 then
		return nil
	end
	return now / burn
end

-- Whether any of the item's spells shows among the character's enchantments.
local function active(m)
	local on = {}
	for _, e in ipairs(game.player.enchantments or {}) do
		on[e.spell] = true
	end
	for _, s in ipairs(m.spells or {}) do
		if on[s] then
			return true
		end
	end
	return false
end

-- An item's state: active, inactive, out, none or unknown.
function M.state(m)
	if m.at == nil then
		return "unknown"
	end
	if #(m.spells or {}) == 0 then
		return "none"
	end
	if (M.mana_now(m) or 0) <= 0 then
		return "out"
	end
	return active(m) and "active" or "inactive"
end

-- The items now, least time left first: { name, mana, left, state } each, `left` in seconds.
function M.rows()
	local rows = {}
	for id, m in pairs(M.items) do
		local now = M.mana_now(m)
		table.insert(rows, {
			id = id,
			name = m.name,
			mana = now and m.max and string.format("%d / %d", math.floor(now + 0.5), m.max) or "-",
			left = M.time_left(m),
			state = M.state(m),
		})
	end
	table.sort(rows, function(a, b)
		local la, lb = a.left or math.huge, b.left or math.huge
		return la < lb or (la == lb and a.name < b.name)
	end)
	return rows
end

-- The items newly under the warning mark: each is answered once until it is over the mark again.
function M.newly_low()
	local out = {}
	for id, m in pairs(M.items) do
		local left = M.time_left(m)
		if left ~= nil and left < M.warn_minutes * 60 then
			if not warned[id] then
				warned[id] = true
				table.insert(out, { name = m.name, left = left })
			end
		else
			warned[id] = nil
		end
	end
	return out
end

-- A "Your <item> is low on mana." or "... is out of mana." line, noted; answers the item's name.
function M.notice(text)
	local name, how = string.match(text, "^Your (.-) is (%a+) on [Mm]ana")
	if name == nil then
		name, how = string.match(text, "^Your (.-) is (out) of [Mm]ana")
	end
	if name == nil then
		return nil
	end
	table.insert(M.notices, { name = name, how = how, at = os.clock() })
	while #M.notices > 10 do
		table.remove(M.notices, 1)
	end
	-- What it says is fresher than the last appraisal: look again at once.
	for _, m in pairs(M.items) do
		if m.name == name then
			m.stale = true
		end
	end
	return name
end

return M
