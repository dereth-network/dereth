-- The character's spells by family: "Flame Bolt" for Flame Bolt I to VII and Incantation of Flame
-- Bolt, so the macro can ask for the best one it knows of a kind.

local S = require("settings")

local M = {}

local ROMAN = { I = 1, II = 2, III = 3, IV = 4, V = 5, VI = 6, VII = 7, VIII = 8 }

-- A spell name's family and the level its name says (nil when the name carries none).
function M.family(name)
	name = name or ""
	local base, numeral = string.match(name, "^(.-)%s+([IVX]+)$")
	if base ~= nil and ROMAN[numeral] ~= nil then
		return base, ROMAN[numeral]
	end
	local inc = string.match(name, "^Incantation of (.+)$")
	if inc ~= nil then
		return inc, 8
	end
	return name, nil
end

-- Known spells by family name ("strength self": Strength Self I to VI, and the family's higher
-- spells, whose names are their own: "Might of the Lugians" is its level 7).
local by_family = nil
-- Every known spell with a category, as flat lists by field (rings are found by category).
local book = nil
-- Each wanted family's category in the spell table, where it gives one.
local family_category = {}

-- Spells read per frame: a character that knows every spell knows thousands, more than one
-- frame's time allows.
local PAGE = 100

-- The families the macro casts from (lower case); the others in the book are not kept, so a book
-- of thousands costs the plugin no memory to speak of.
local wanted = {}

-- Whether the spell book has been read.
function M.ready()
	return by_family ~= nil
end

-- Adds families to those kept at the next read of the book.
function M.want(families)
	for _, f in ipairs(families) do
		wanted[string.lower(f)] = true
	end
end

-- A spell description with its numbers taken out: the levels of one family say the same thing
-- by different amounts ("Increases the caster's Strength by # points.").
local function shape(text)
	return (string.gsub(string.lower(text or ""), "%d+", "#"))
end

-- Reads the spell book again, answering how many spells it holds. Inside a task (`in_task`) it is
-- read a page a frame.
--
-- A family's higher spells are found in the spell table: of the spells known in the family's
-- category above its highest named level, those whose description reads as that level's does
-- (a category also holds a family's "Other" form, its fellowship and item kin, and some kindred
-- families). Where several do, the earliest (lowest id) is the family's own.
function M.refresh(in_task)
	local fresh = {}
	local n = 0
	-- Every known spell with a category, in flat lists (no table a spell).
	local cats, ids, levels, names, diffs, manas, schools = {}, {}, {}, {}, {}, {}, {}
	local function take(list)
		for _, s in ipairs(list) do
			local fam, level = M.family(s.name)
			local key = string.lower(fam)
			if wanted[key] then
				fresh[key] = fresh[key] or {}
				table.insert(fresh[key], { id = s.id, name = s.name, level = level or s.level or 0, school = s.school,
					category = s.category, difficulty = s.difficulty, mana = s.mana })
			end
			if s.category ~= nil then
				local i = #cats + 1
				cats[i], ids[i], levels[i], names[i] = s.category, s.id, s.level or 0, s.name
				diffs[i], manas[i], schools[i] = s.difficulty or false, s.mana or false, s.school or false
			end
		end
		n = n + #list
	end
	if in_task then
		while true do
			local page = magic.known(n + 1, PAGE) or {}
			take(page)
			if #page < PAGE then
				break
			end
			task.sleep(0)
		end
	else
		take(magic.known() or {})
	end
	family_category = {}
	-- Each family's highest named spell, by category.
	local tops = {}
	for key, list in pairs(fresh) do
		-- The highest of the numbered levels (I to VI): the levels above them are named for
		-- themselves, but for the Incantation (level 8), which names its family.
		local top, named = nil, {}
		for _, sp in ipairs(list) do
			named[sp.level] = true
			if sp.category ~= nil and sp.level <= 6 and (top == nil or sp.level > top.level) then
				top = sp
			end
		end
		if top ~= nil then
			family_category[key] = top.category
			tops[top.category] = tops[top.category] or {}
			table.insert(tops[top.category], { key = key, top = top, named = named })
		end
	end
	local looked = 0
	local function look(id)
		looked = looked + 1
		if in_task and looked % 50 == 0 then
			task.sleep(0)
		end
		local sp = magic.spell(id)
		return sp and shape(sp.description) or ""
	end
	local shapes = {}
	for i = 1, #cats do
		local fams = tops[cats[i]]
		if fams ~= nil then
			for _, f in ipairs(fams) do
				if levels[i] > f.top.level and not f.named[levels[i]] and string.lower(M.family(names[i])) ~= f.key then
					shapes[f.top.id] = shapes[f.top.id] or look(f.top.id)
					if look(ids[i]) == shapes[f.top.id] then
						local list = fresh[f.key]
						local have = nil
						for _, sp in ipairs(list) do
							if sp.level == levels[i] and sp.higher then
								have = sp
							end
						end
						if have == nil then
							table.insert(list, { id = ids[i], name = names[i], level = levels[i], category = cats[i], higher = true,
								difficulty = diffs[i] or nil, mana = manas[i] or nil, school = schools[i] or nil })
						elseif ids[i] < have.id then
							have.id, have.name = ids[i], names[i]
							have.difficulty, have.mana = diffs[i] or nil, manas[i] or nil
						end
					end
				end
			end
		end
	end
	-- Each list is put in order when first asked for.
	by_family = fresh
	book = { cats = cats, ids = ids, levels = levels, names = names, diffs = diffs, manas = manas, schools = schools }
	return n
end

-- The id of a family's first-level spell in the character's book, or nil (the book as last read).
function M.level_one(family)
	for _, sp in ipairs(by_family and by_family[string.lower(family)] or {}) do
		if sp.level == 1 then
			return sp.id
		end
	end
	return nil
end

-- The spell table's category of a family, or nil.
function M.category_of(family)
	return family_category[string.lower(family)]
end

local function by_level(a, b)
	return a.level > b.level
end

-- The skill each school is cast with.
M.SCHOOL_SKILL = {
	[1] = "War Magic",
	[2] = "Life Magic",
	[3] = "Item Enchantment",
	[4] = "Creature Enchantment",
	[5] = "Void Magic",
}

-- The character's buffed skill in a spell's school, or nil where the host does not say.
function M.skill(school)
	local name = M.SCHOOL_SKILL[school or 0]
	if name == nil then
		return nil
	end
	for _, sk in ipairs(game.player.skills or {}) do
		if sk.name == name then
			return sk.current
		end
	end
	return nil
end

-- Whether the character casts `s` reliably: as VTank reckons it, its buffed skill in the spell's
-- school at least the spell's difficulty and `excess` more (VTank's SpellDiffExcessThreshold).
-- With no difficulty or no skill known, it is taken as castable.
function M.castable(s, excess)
	if excess == nil or s.difficulty == nil then
		return true
	end
	local skill = M.skill(s.school)
	return skill == nil or skill >= s.difficulty + excess
end

-- The level of spell each scarab is the power component of.
M.SCARABS = {
	["lead scarab"] = 1, ["iron scarab"] = 2, ["copper scarab"] = 3, ["silver scarab"] = 4,
	["gold scarab"] = 5, ["pyreal scarab"] = 6, ["diamond scarab"] = 6, ["platinum scarab"] = 7,
	["dark scarab"] = 7, ["mana scarab"] = 8,
}

-- The spell levels the BlacklistedSpellComps setting leaves out, by the scarabs it names.
local function excluded_levels()
	local out = {}
	for _, name in ipairs(S.list("BlacklistedSpellComps")) do
		local level = M.SCARABS[string.lower(name)]
		if level ~= nil then
			out[level] = true
		end
	end
	return out
end

-- Whether a spell is left out by the scarabs excluded.
function M.excluded(s, levels)
	levels = levels or excluded_levels()
	return s ~= nil and levels[s.level or 0] == true
end

-- A spell's duration in seconds, from the spell table where the client knows it, or nil.
local durations = {}
function M.duration(id)
	local d = durations[id]
	if d == nil then
		local sp = magic.spell(id)
		d = sp and sp.duration or false
		durations[id] = d
	end
	return d or nil
end

-- The best known spell of a family, no higher than `max_level` when given and, with `excess`, one
-- the character casts reliably now: `{ id, name, level, difficulty, mana }`. Asked again as the
-- skill rises (a mastery buff landed), it answers the higher level: buffing climbs.
function M.best(family, max_level, excess)
	if by_family == nil then
		M.refresh()
	end
	local list = by_family[string.lower(family)]
	if list == nil then
		return nil
	end
	if not list.sorted then
		table.sort(list, by_level)
		list.sorted = true
	end
	local levels = excluded_levels()
	for _, s in ipairs(list) do
		if (max_level == nil or s.level <= max_level) and M.castable(s, excess) and not levels[s.level] then
			return s
		end
	end
	return nil
end

-- The war (and void) bolts, streaks and arcs by damage type.
M.BOLTS = {
	fire = "Flame Bolt", cold = "Frost Bolt", acid = "Acid Stream", lightning = "Lightning Bolt",
	slash = "Whirling Blade", pierce = "Force Bolt", blunt = "Shock Wave", nether = "Nether Bolt",
}
M.STREAKS = {
	fire = "Flame Streak", cold = "Frost Streak", acid = "Acid Streak", lightning = "Lightning Streak",
	slash = "Whirling Blade Streak", pierce = "Force Streak", blunt = "Shock Wave Streak", nether = "Nether Streak",
}
M.ARCS = {
	fire = "Flame Arc", cold = "Frost Arc", acid = "Acid Arc", lightning = "Lightning Arc", slash = "Blade Arc",
	pierce = "Force Arc", blunt = "Shock Arc", nether = "Nether Arc",
}
-- The attack families by damage type: the bolt, else the streak.
M.ATTACKS = {}
for t, bolt in pairs(M.BOLTS) do
	M.ATTACKS[t] = { bolt, M.STREAKS[t] }
end
-- The rings by damage type: their names are each their own (Searing Disc, Halo of Frost, ...), so
-- they are found by the spell table's category of each element's rings.
M.RING_CATEGORIES = { acid = 222, blunt = 223, cold = 224, lightning = 225, fire = 226, pierce = 227, slash = 228, nether = 641 }
-- The drains, by the vital each takes from the monster and gives to the caster.
M.DRAINS = { health = "Drain Health Other", stamina = "Drain Stamina Other", mana = "Drain Mana Other" }
-- The harm spells: Harm Other, else the martyr's spell that spends the caster's own health.
M.HARMS = { "Harm Other", "Martyr's Hecatomb" }
-- Tusker Fists: a hail of fists down a straight path ahead of the caster, aimed by facing.
M.FISTS = "Tusker Fists"
-- The spell that dispels harmful life magic of level 7 and under from the caster.
M.DISPEL = "Eradicate Life Magic Self"
-- Damage types tried in this order when a monster's says auto.
M.AUTO_ORDER = { "fire", "lightning", "cold", "acid", "slash", "pierce", "blunt", "nether" }
-- The damage modes a monster row may name besides the damage types: drain spells, harm spells,
-- Tusker Fists, and a damage type at random.
M.MODES = { drain = true, harm = true, fists = true, random = true }

-- The vulnerability spell of each damage type.
M.VULNS = {
	fire = "Fire Vulnerability Other",
	cold = "Cold Vulnerability Other",
	acid = "Acid Vulnerability Other",
	lightning = "Lightning Vulnerability Other",
	slash = "Blade Vulnerability Other",
	pierce = "Piercing Vulnerability Other",
	blunt = "Bludgeoning Vulnerability Other",
}

-- The other debuffs, by the names a monster's settings use: VTank's columns I, Y, F, B, G and WC,
-- and its void curses and damage over time, FC, Cp, DC and Cs.
M.DEBUFFS = {
	imperil = "Imperil Other",
	yield = "Magic Yield Other",
	fester = "Fester Other",
	broadside = "Broadside of a Barn",
	gravity = "Gravity Well",
	weaken = "Weakening Curse",
	festering = "Festering Curse",
	corruption = "Corruption",
	destructive = "Destructive Curse",
	corrosion = "Corrosion",
}
-- The debuffs that do their damage over time (VTank's Cp, DC and Cs): each is cast again when it
-- has run its course, not before it runs out.
M.DOTS = { ["Corruption"] = true, ["Destructive Curse"] = true, ["Corrosion"] = true }

-- How long a damage-over-time spell runs, seconds: its duration, else the time its description
-- gives ("... over 15 seconds"), else nil.
function M.dot_seconds(id)
	local d = M.duration(id)
	if d ~= nil then
		return d
	end
	local sp = magic.spell(id)
	local over = sp and string.match(sp.description or "", "over (%d+) seconds")
	return over and tonumber(over) or nil
end

for _, set in ipairs({ M.BOLTS, M.STREAKS, M.ARCS }) do
	for _, f in pairs(set) do
		M.want({ f })
	end
end
for _, f in pairs(M.DRAINS) do
	M.want({ f })
end
M.want(M.HARMS)
M.want({ M.FISTS, M.DISPEL })
-- Mana back in a fight.
M.want({ "Stamina to Mana Self" })
for _, set in ipairs({ M.VULNS, M.DEBUFFS }) do
	for _, f in pairs(set) do
		M.want({ f })
	end
end

-- Whether the character has a skill trained (or specialized).
function M.trained(name)
	for _, sk in ipairs(game.player.skills or {}) do
		if sk.name == name then
			return sk.training ~= "untrained"
		end
	end
	return false
end

-- The damage types "auto" tries, in order: as VTank picks them, nether first for a character with
-- Void Magic trained and War Magic not.
function M.auto_order()
	if not M.trained("War Magic") and M.trained("Void Magic") then
		local out = { "nether" }
		for _, t in ipairs(M.AUTO_ORDER) do
			if t ~= "nether" then
				table.insert(out, t)
			end
		end
		return out
	end
	return M.AUTO_ORDER
end

-- The best attack spell for a damage type ("auto" picks the first type it knows a spell of),
-- answering the spell and the type used. `kind` asks for the streak (falling back to the bolt), or
-- the arc, which is taken over the bolt where it is known at the bolt's level or above (VTank's
-- UseArcs).
function M.attack(damage, max_level, excess, kind)
	local types = (damage == nil or damage == "" or damage == "auto") and M.auto_order() or { damage }
	for _, t in ipairs(types) do
		local bolt = M.BOLTS[t] and M.best(M.BOLTS[t], max_level, excess)
		local streak = M.STREAKS[t] and M.best(M.STREAKS[t], max_level, excess)
		local s = bolt or streak
		if kind == "streak" then
			s = streak or bolt
		elseif kind == "arc" then
			local arc = M.ARCS[t] and M.best(M.ARCS[t], max_level, excess)
			if arc ~= nil and (bolt == nil or (arc.level or 0) >= (bolt.level or 0)) then
				s = arc
			end
		end
		if s ~= nil then
			return s, t
		end
	end
	return nil
end

-- The best known ring of a damage type ("auto": the first type it knows one of), no higher than
-- `max_level` and, with `excess`, one it casts reliably; and the type.
function M.ring(damage, max_level, excess)
	if book == nil then
		M.refresh()
	end
	local types = (damage == nil or damage == "" or damage == "auto") and M.auto_order() or { damage }
	local levels = excluded_levels()
	for _, t in ipairs(types) do
		local cat, best = M.RING_CATEGORIES[t], nil
		for i = 1, #book.cats do
			if book.cats[i] == cat then
				local s = { id = book.ids[i], name = book.names[i], level = book.levels[i], difficulty = book.diffs[i] or nil,
					mana = book.manas[i] or nil, school = book.schools[i] or nil }
				if (max_level == nil or s.level <= max_level) and M.castable(s, excess) and not levels[s.level]
					and (best == nil or s.level > best.level) then
					best = s
				end
			end
		end
		if best ~= nil then
			return best, t
		end
	end
	return nil
end

return M
