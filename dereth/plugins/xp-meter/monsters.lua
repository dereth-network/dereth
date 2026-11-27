-- Monster statistics: what each monster the character fights does to it and it to the monster,
-- learned from play and kept by the monster's name.
--
-- The server says each swing's outcome in a combat notice naming the other side: a hit with its
-- damage, or an evasion. A kill is a line naming the monster that died. Spells are told only in
-- chat lines: a spell's damage, a resist, a spell cast on a monster. The experience a kill gives
-- arrives with it or soon after, so a gain within a few seconds of a kill is that kill's.

local M = {}

-- How long a kill waits for its experience, and its corpse.
local XP_WAIT = 3
local CORPSE_WAIT = 30
-- A fight with a monster of a name ends when nothing has happened with one for this long.
local FIGHT_GAP = 60

-- The counts kept for each monster.
local FIELDS = {
	"kills", "xp", "hits", "dealt", "crits", "crit_dealt", "missed", "struck", "taken", "evaded",
	"spell_hits", "spell_dealt", "landed", "resisted", "spell_struck", "spell_taken", "resists",
	"casts", "fights", "fight_seconds", "corpses", "corpse_seconds",
}

local function blank()
	local m = {}
	for _, f in ipairs(FIELDS) do
		m[f] = 0
	end
	return m
end

-- A tracker over `book` (name to counts, as kept in the character's store), timed by `now`.
function M.new(book, now)
	local t = { book = book or {}, now = now, dirty = false }
	local fights = {} -- name to { start, last }
	local pending = {} -- kills waiting for their experience: { name, at }
	local bodies = {} -- kills waiting for their corpse: { name, at }

	local function of(name)
		local m = t.book[name]
		if m == nil then
			m = blank()
			t.book[name] = m
		end
		for _, f in ipairs(FIELDS) do
			m[f] = m[f] or 0
		end
		return m
	end

	local function engage(name)
		local at = t.now()
		local f = fights[name]
		if f == nil or at - f.last > FIGHT_GAP then
			fights[name] = { start = at, last = at }
		else
			f.last = at
		end
	end

	local function add(name, field, n)
		local m = of(name)
		m[field] = m[field] + (n or 1)
		t.dirty = true
		return m
	end

	-- The character hit `name` for `damage`.
	function t.hit(name, damage, critical)
		engage(name)
		add(name, "hits")
		add(name, "dealt", damage)
		if critical then
			add(name, "crits")
			add(name, "crit_dealt", damage)
		end
	end

	-- `name` hit the character for `damage`.
	function t.struck(name, damage)
		engage(name)
		add(name, "struck")
		add(name, "taken", damage)
	end

	-- `name` evaded the character's swing.
	function t.missed(name)
		engage(name)
		add(name, "missed")
	end

	-- The character evaded `name`'s swing.
	function t.evaded(name)
		engage(name)
		add(name, "evaded")
	end

	-- The character cast a spell at `name`.
	function t.cast(name)
		engage(name)
		add(name, "casts")
	end

	-- A spell of the character's hurt `name` for `damage`.
	function t.spell_hit(name, damage)
		engage(name)
		add(name, "spell_hits")
		add(name, "spell_dealt", damage)
	end

	-- A spell of the character's that does no damage took hold on `name`.
	function t.landed(name)
		engage(name)
		add(name, "landed")
	end

	-- `name` resisted the character's spell.
	function t.resisted(name)
		engage(name)
		add(name, "resisted")
	end

	-- A spell `name` cast hurt the character for `damage`.
	function t.spell_struck(name, damage)
		engage(name)
		add(name, "spell_struck")
		add(name, "spell_taken", damage)
	end

	-- The character resisted a spell `name` cast.
	function t.resist(name)
		engage(name)
		add(name, "resists")
	end

	-- Whether `name` is one the tracker has counted.
	function t.knows(name)
		return t.book[name] ~= nil
	end

	-- The character killed `name`.
	function t.kill(name)
		local at = t.now()
		local m = add(name, "kills")
		local f = fights[name]
		if f ~= nil and at - f.last <= FIGHT_GAP then
			m.fights = m.fights + 1
			m.fight_seconds = m.fight_seconds + (at - f.start)
		end
		fights[name] = nil
		table.insert(pending, { name = name, at = at })
		table.insert(bodies, { name = name, at = at })
	end

	-- The character gained `n` experience: the oldest kill still waiting for its experience gets it.
	function t.gained(n)
		local at = t.now()
		while #pending > 0 and at - pending[1].at > XP_WAIT do
			table.remove(pending, 1)
		end
		local k = table.remove(pending, 1)
		if k ~= nil then
			add(k.name, "xp", n)
		end
	end

	-- A corpse named `name` appeared: the oldest kill of that name still waiting for its corpse.
	function t.corpse(name)
		local at = t.now()
		for i, b in ipairs(bodies) do
			if at - b.at > CORPSE_WAIT then
				bodies[i] = false
			elseif b.name == name then
				add(name, "corpses")
				add(name, "corpse_seconds", at - b.at)
				bodies[i] = false
				break
			end
		end
		local kept = {}
		for _, b in ipairs(bodies) do
			if b then
				table.insert(kept, b)
			end
		end
		bodies = kept
	end

	-- Forget the monster `name`: whether there was one to forget.
	function t.forget_name(name)
		if t.book[name] == nil then
			return false
		end
		t.book[name] = nil
		fights[name] = nil
		local function others(list)
			local kept = {}
			for _, k in ipairs(list) do
				if k.name ~= name then
					table.insert(kept, k)
				end
			end
			return kept
		end
		pending, bodies = others(pending), others(bodies)
		t.dirty = true
		return true
	end

	-- Forget every monster.
	function t.forget()
		t.book = {}
		fights, pending, bodies = {}, {}, {}
		t.dirty = true
	end

	return t
end

-- `part` of `whole` as a whole percentage, or nil when there is nothing to divide.
local function percent(part, whole)
	if whole <= 0 then
		return nil
	end
	return math.floor(part * 100 / whole + 0.5)
end

-- `sum` per `count`, or nil when there is none.
local function per(sum, count)
	if count <= 0 then
		return nil
	end
	return sum / count
end

-- What the counts of one monster come to.
function M.figures(m)
	return {
		kills = m.kills,
		xp_per_kill = per(m.xp, m.kills),
		it_evades = percent(m.missed, m.hits + m.missed),
		you_evade = percent(m.evaded, m.evaded + m.struck),
		it_resists = percent(m.resisted, m.resisted + m.spell_hits + m.landed),
		you_resist = percent(m.resists, m.resists + m.spell_struck),
		dealt = per(m.dealt, m.hits),
		normal = per(m.dealt - m.crit_dealt, m.hits - m.crits),
		critical = per(m.crit_dealt, m.crits),
		taken = per(m.taken, m.struck),
		spell_dealt = per(m.spell_dealt, m.spell_hits),
		spell_taken = per(m.spell_taken, m.spell_struck),
		seconds = per(m.fight_seconds, m.fights),
		casts = per(m.casts, m.kills),
		corpse = per(m.corpse_seconds, m.corpses),
	}
end

-- The names in `book`, most killed first, then by name.
function M.names(book)
	local names = {}
	for name in pairs(book) do
		table.insert(names, name)
	end
	table.sort(names, function(a, b)
		local ka, kb = book[a].kills or 0, book[b].kills or 0
		if ka ~= kb then
			return ka > kb
		end
		return a < b
	end)
	return names
end

-- The name of the monster a kill's line tells of: the longest of `names` the line holds.
function M.victim(text, names)
	local best = nil
	for _, n in ipairs(names) do
		if n ~= "" and string.find(text, n, 1, true) and (best == nil or #n > #best) then
			best = n
		end
	end
	return best
end

-- A spell's line, read: `kind` (`dealt`, `taken`, `resisted`, `resist` or `cast`), the monster's
-- name (for `cast`, the names it may be: a spell's name can hold " on "), and the damage.
function M.spell_line(text)
	local s = text:match("^%s*(.-)%s*$")
	-- The words a line may start with, in any order: "Critical hit!", "Overpower!" and the like.
	while true do
		local word, rest = s:match("^(%a[%a ]-)!%s+(.*)$")
		if word ~= "Critical hit" and word ~= "Overpower" and word ~= "Sneak Attack" and word ~= "Recklessness" and word ~= "Reckless" then
			break
		end
		s = rest
	end
	local name, n = s:match("^You %S+ (.+) for (%d+) points? with ")
	if name then
		return "dealt", name, tonumber(n)
	end
	name, n = s:match("^(.+) %S+ you for (%d+) points? with ")
	if name then
		return "taken", name, tonumber(n)
	end
	name = s:match("^(.+) resists your spell")
	if name then
		return "resisted", name
	end
	name = s:match("^You resist the spell cast by (.-)%.?$")
	if name then
		return "resist", name
	end
	local first, last = s:match("^You cast .- on (.+)$"), s:match("^You cast .+ on (.+)$")
	if first then
		return "cast", { first, last }
	end
	return nil
end

return M
