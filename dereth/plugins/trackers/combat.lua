-- The combat tracker: who hit whom, for how much, with what element, from the combat lines.
--
-- The lines read, each perhaps after "Critical hit!", "Overpower!", "Sneak Attack!" or
-- "Recklessness!":
--   "You impale Drudge Skulker for 8 points of piercing damage!"          (your melee or missile hit)
--   "You scorch Drudge Skulker for 22 points with Flame Bolt I."           (your spell)
--   "Drudge Skulker grazes your lower leg for 3 points of bludgeoning damage!"  (a hit on you)
--   "Drudge Skulker singes you for 9 points with Flame Bolt I."            (a spell on you)
--   "You evaded Drudge Skulker!"  "Drudge Skulker evaded your attack."
--   "Drudge Skulker resists your spell"  "You resist the spell cast by Drudge Skulker"
-- The element is the line's own damage word where it has one, else the verb's: each element has
-- four verbs, from a scratch to a mangle.

local M = {}

local VERBS = {
	Slashing = { "scratch", "cut", "slash", "mangle" },
	Piercing = { "nick", "stab", "impale", "gore" },
	Bludgeoning = { "graze", "bash", "smash", "crush" },
	Cold = { "numb", "chill", "frost", "freeze" },
	Fire = { "singe", "scorch", "burn", "incinerate" },
	Acid = { "blister", "sear", "corrode", "dissolve" },
	Electric = { "spark", "shock", "jolt", "blast" },
	Health = { "drain", "exhaust", "siphon", "deplete" },
	Nether = { "scar", "twist", "wither", "eradicate" },
}

-- Each verb, in both of its forms ("graze" and "grazes"), to its element.
local by_verb = {}
for element, verbs in pairs(VERBS) do
	for _, v in ipairs(verbs) do
		by_verb[v] = element
		by_verb[v .. "s"] = element
		by_verb[v .. "es"] = element
	end
end

-- A line's own damage word to its element.
local WORDS = {
	slashing = "Slashing", piercing = "Piercing", bludgeoning = "Bludgeoning", cold = "Cold",
	fire = "Fire", acid = "Acid", electrical = "Electric", nether = "Nether", prismatic = "Prismatic",
}

local function element(verb, word)
	if word ~= nil then
		local first = string.match(string.lower(word), "^(%a+)")
		if first ~= nil and WORDS[first] ~= nil then
			return WORDS[first]
		end
	end
	return by_verb[string.lower(verb or "")] or "Other"
end

-- What a line says: { what, who, damage, crit, element, magic }, or nil for a line that is none
-- of the combat lines. `what` is dealt, taken, evaded (you evaded it), missed (it evaded you),
-- resisted (it resisted your spell) or resist (you resisted its spell).
function M.read(text)
	text = string.match(text, "^%s*(.-)%s*$")
	local crit = false
	-- The prefixes, in any order.
	while true do
		local word, rest = string.match(text, "^(%a[%a ]-)!%s+(.*)$")
		if word == "Critical hit" then
			crit = true
		elseif word ~= "Overpower" and word ~= "Sneak Attack" and word ~= "Recklessness" and word ~= "Reckless" then
			break
		end
		text = rest
	end
	local who = string.match(text, "^You evaded (.-)!")
	if who ~= nil then
		return { what = "evaded", who = who }
	end
	who = string.match(text, "^You resist the spell cast by (.-)%.?$")
	if who ~= nil then
		return { what = "resist", who = who, magic = true }
	end
	local verb, n, word
	verb, who, n, word = string.match(text, "^You (%a+) (.-) for (%d+) points? of (.-)damage!")
	if verb ~= nil then
		return { what = "dealt", who = who, damage = tonumber(n), crit = crit, element = element(verb, word) }
	end
	verb, who, n = string.match(text, "^You (%a+) (.-) for (%d+) points? with .+$")
	if verb ~= nil then
		return { what = "dealt", who = who, damage = tonumber(n), crit = crit, element = element(verb), magic = true }
	end
	who, verb, n, word = string.match(text, "^(.-) (%a+) your [%a ]- for (%d+) points? of (.-)damage!")
	if who ~= nil then
		return { what = "taken", who = who, damage = tonumber(n), crit = crit, element = element(verb, word) }
	end
	who, verb, n = string.match(text, "^(.-) (%a+) you for (%d+) points? with .+$")
	if who ~= nil then
		return { what = "taken", who = who, damage = tonumber(n), crit = crit, element = element(verb), magic = true }
	end
	who = string.match(text, "^(.-) evaded your attack")
	if who ~= nil then
		return { what = "missed", who = who }
	end
	who = string.match(text, "^(.-) resists your spell")
	if who ~= nil then
		return { what = "resisted", who = who, magic = true }
	end
	return nil
end

local COUNTS = { "dealt", "taken", "hits", "crits", "misses", "resisted", "hit_on_me", "magic_on_me", "evades", "resists", "kbs" }

-- A fresh tally: for the whole fight, or for one creature.
function M.tally()
	local t = { dealt_by = {}, taken_by = {} }
	for _, k in ipairs(COUNTS) do
		t[k] = 0
	end
	return t
end

-- Count one line's news into a tally.
function M.count(t, news)
	local w = news.what
	if w == "dealt" then
		t.dealt = t.dealt + news.damage
		t.hits = t.hits + 1
		if news.crit then
			t.crits = t.crits + 1
		end
		t.dealt_by[news.element] = (t.dealt_by[news.element] or 0) + news.damage
	elseif w == "taken" then
		t.taken = t.taken + news.damage
		if news.magic then
			t.magic_on_me = t.magic_on_me + 1
		else
			t.hit_on_me = t.hit_on_me + 1
		end
		t.taken_by[news.element] = (t.taken_by[news.element] or 0) + news.damage
	elseif w == "missed" then
		t.misses = t.misses + 1
	elseif w == "resisted" then
		t.resisted = t.resisted + 1
	elseif w == "evaded" then
		t.evades = t.evades + 1
	elseif w == "resist" then
		t.resists = t.resists + 1
	end
end

-- Your attacks: those that landed and those that did not.
function M.attacks(t)
	return t.hits + t.misses + t.resisted
end

-- The elements of a tally's damage table, most damage first, as "Fire 60%, Cold 40%".
function M.elements(by)
	local rows, total = {}, 0
	for e, n in pairs(by or {}) do
		table.insert(rows, { e, n })
		total = total + n
	end
	table.sort(rows, function(a, b)
		return a[2] > b[2] or (a[2] == b[2] and a[1] < b[1])
	end)
	local out = {}
	for i, r in ipairs(rows) do
		if i > 3 then
			break
		end
		table.insert(out, string.format("%s %.0f%%", r[1], 100 * r[2] / total))
	end
	return #out > 0 and table.concat(out, ", ") or "-"
end

-- Add one tally into another (the session into the lifetime).
function M.add(into, t)
	for _, k in ipairs(COUNTS) do
		into[k] = (into[k] or 0) + (t[k] or 0)
	end
	for _, side in ipairs({ "dealt_by", "taken_by" }) do
		into[side] = into[side] or {}
		for e, n in pairs(t[side] or {}) do
			into[side][e] = (into[side][e] or 0) + n
		end
	end
end

return M
