-- An item's information in one line, in the manner of Mag-Tools' item information on
-- appraisal: its armor level or damage, its spells, its set, its workmanship and tinkers, its
-- ratings, what it takes to wield, its arcane lore, its value and burden.

local I = {}

-- The armor and jewelry sets, by the set number an item carries, as the game names them.
I.SETS = {
	[4] = "Carraida's Benediction", [5] = "Noble Relic", [6] = "Ancient Relic", [7] = "Alduressa Relic",
	[8] = "Shou-jen", [9] = "Empyrean Rings", [10] = "Arm, Mind, Heart", [11] = "Coat of Perfect Light",
	[12] = "Leggings of Perfect Light", [13] = "Soldier's", [14] = "Adept's", [15] = "Archer's",
	[16] = "Defender's", [17] = "Tinker's", [18] = "Crafter's", [19] = "Hearty", [20] = "Dexterous",
	[21] = "Wise", [22] = "Swift", [23] = "Hardened", [24] = "Reinforced", [25] = "Interlocking",
	[26] = "Flame Proof", [27] = "Acid Proof", [28] = "Cold Proof", [29] = "Lightning Proof",
	[30] = "Dedication", [31] = "Gladiatorial Clothing", [32] = "Ceremonial Clothing",
	[33] = "Protective Clothing", [35] = "Sigil of Defense", [36] = "Sigil of Destruction",
	[37] = "Sigil of Fury", [38] = "Sigil of Growth", [39] = "Sigil of Vigor", [40] = "Heroic Protector",
	[41] = "Heroic Destroyer", [89] = "Shou-jen Shozoku", [91] = "Shrouded Soul", [92] = "Darkened Mind",
	[93] = "Clouded Spirit",
}

-- A set's name, or "set <n>" for one this list does not name.
function I.set_name(n)
	if n == nil then
		return nil
	end
	return I.SETS[n] or ("set " .. n)
end

-- The ratings, by the record's key, as the line says them.
local RATING_TEXT = {
	{ "dmg", "Dmg" }, { "crit", "Crit" }, { "critdmg", "Crit Dmg" }, { "dmgres", "Dmg Resist" },
	{ "critres", "Crit Resist" }, { "critdmgres", "Crit Dmg Resist" }, { "heal", "Heal Boost" },
	{ "vit", "Vitality" },
}

-- The skills' names, by number, from the character's own list (every skill is in it).
local function skill_names()
	local out = {}
	for _, s in ipairs(game.player.skills or {}) do
		if s.id ~= nil then
			out[s.id] = s.name
		end
	end
	return out
end

-- What it takes to wield an item, said: "level 150", "Melee Defense 300".
function I.requirement(rq, skills)
	if rq == nil then
		return nil
	end
	local d = rq.d or 0
	if rq.t == 7 then
		return "level " .. d
	elseif rq.t == 1 or rq.t == 2 then
		skills = skills or skill_names()
		return string.format("%s %d%s", skills[rq.s or 0] or ("skill " .. tostring(rq.s)), d, rq.t == 2 and " base" or "")
	elseif rq.t == 8 then
		skills = skills or skill_names()
		return string.format("%s %s", d >= 3 and "specialized in" or "trained in", skills[rq.s or 0] or ("skill " .. tostring(rq.s)))
	end
	return string.format("requirement %d (%d)", rq.t or 0, d)
end

-- A number with its thousands marked: 12500 -> "12,500".
local function thousands(n)
	local s = tostring(math.floor(n or 0))
	local out = s:reverse():gsub("(%d%d%d)", "%1,"):reverse()
	return (string.gsub(out, "^,", ""))
end
I.thousands = thousands

-- The line for a record (as the log keeps one).
function I.describe(r, skills)
	local parts = {}
	if (r.al or 0) > 0 then
		table.insert(parts, "AL " .. r.al)
	end
	if (r.dmg or 0) > 0 then
		table.insert(parts, "Dmg " .. r.dmg)
	end
	if r.sp ~= nil and #r.sp > 0 then
		table.insert(parts, table.concat(r.sp, ", "))
	end
	if r.set ~= nil then
		table.insert(parts, "Set: " .. I.set_name(r.set))
	end
	if r.wk ~= nil and r.wk > 0 then
		table.insert(parts, string.format("Work %s", r.wk == math.floor(r.wk) and tostring(math.floor(r.wk)) or string.format("%.2f", r.wk)))
	end
	if (r.tk or 0) > 0 then
		table.insert(parts, "Tinkered " .. r.tk)
	end
	local ratings = {}
	for _, rt in ipairs(RATING_TEXT) do
		local v = r.r and r.r[rt[1]]
		if v ~= nil then
			table.insert(ratings, string.format("%s %d", rt[2], v))
		end
	end
	if #ratings > 0 then
		table.insert(parts, table.concat(ratings, ", "))
	end
	local req = I.requirement(r.rq, skills)
	if req ~= nil then
		table.insert(parts, "Wield: " .. req)
	end
	if (r.lore or 0) > 0 then
		table.insert(parts, "Lore " .. r.lore)
	end
	if r.att then
		table.insert(parts, "Attuned")
	end
	if (r.v or 0) > 0 then
		table.insert(parts, "Value " .. thousands(r.v))
	end
	if (r.b or 0) > 0 then
		table.insert(parts, "Burden " .. r.b)
	end
	return r.n .. (#parts > 0 and (": " .. table.concat(parts, "; ")) or "")
end

return I
