-- The inventory item tracker: how many of each consumable the character holds now against the
-- session's start, the change and its rate per hour, and the value of it all. Spell components,
-- healing kits, mana stones, trade notes and salvage are counted, by name; a stack counts its
-- size. Components the player names get a warning when they fall under a count.

local M = {}

local KINDS = {
	SpellComponent = "component",
	HealingKit = "kit",
	ManaStone = "stone",
	TradeNote = "note",
	Salvage = "salvage",
}

-- Name -> { kind, start, now, value_start, value_now }
M.counts = {}
M.started = nil
-- Name -> count under which it is warned of (the player's, kept with the character).
M.low = {}
local warned = {}

-- What the pack holds now, by name.
local function census()
	local out = {}
	local me = game.player.id
	for _, it in ipairs(game.inventory.items()) do
		local kind = KINDS[it.class or ""]
		if kind ~= nil and it.wielder ~= me then
			local name = it.name or "?"
			local c = out[name]
			if c == nil then
				c = { kind = kind, n = 0, value = 0 }
				out[name] = c
			end
			c.n = c.n + (it.stack or 1)
			c.value = c.value + (it.value or 0)
		end
	end
	return out
end

-- Count the pack again. The first count of a session is its start.
function M.recount()
	local now = census()
	local first = M.started == nil
	M.started = M.started or os.clock()
	for name, c in pairs(now) do
		local row = M.counts[name]
		if row == nil then
			row = { kind = c.kind, start = first and c.n or 0, value_start = first and c.value or 0 }
			M.counts[name] = row
		end
		row.now, row.value_now = c.n, c.value
	end
	for name, row in pairs(M.counts) do
		if now[name] == nil then
			row.now, row.value_now = 0, 0
		end
	end
end

function M.reset()
	M.counts = {}
	M.started = nil
	warned = {}
	M.recount()
end

-- The rows, by kind and name: { name, kind, start, now, change, per_hour }.
function M.rows()
	local secs = M.started and os.clock() - M.started or 0
	local rows = {}
	for name, row in pairs(M.counts) do
		local change = row.now - row.start
		table.insert(rows, {
			name = name,
			kind = row.kind,
			start = row.start,
			now = row.now,
			change = change,
			per_hour = secs > 0 and change * 3600 / secs or 0,
		})
	end
	table.sort(rows, function(a, b)
		return a.kind < b.kind or (a.kind == b.kind and a.name < b.name)
	end)
	return rows
end

-- The change in what the counted things are worth, and that per hour.
function M.value()
	local change = 0
	for _, row in pairs(M.counts) do
		change = change + (row.value_now or 0) - (row.value_start or 0)
	end
	local secs = M.started and os.clock() - M.started or 0
	return change, secs > 0 and change * 3600 / secs or 0
end

-- The count of a thing by name, the name compared without case.
function M.count_of(name)
	local want = string.lower(name)
	for n, row in pairs(M.counts) do
		if string.lower(n) == want then
			return row.now
		end
	end
	return 0
end

-- The named things newly under their warning counts: each answered once until it is back over.
function M.newly_low()
	local out = {}
	for name, under in pairs(M.low) do
		local n = M.count_of(name)
		if n < under then
			if not warned[name] then
				warned[name] = true
				table.insert(out, { name = name, n = n, under = under })
			end
		else
			warned[name] = nil
		end
	end
	table.sort(out, function(a, b)
		return a.name < b.name
	end)
	return out
end

return M
