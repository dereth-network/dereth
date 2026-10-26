-- Settings profiles in VTank's format (`.usd`): read one into the macro's settings, monster list,
-- combat items, consumables and buffs, and write the macro's own out the same way, so a profile
-- goes both ways between the two.
--
-- VTank's documentation gives the layout of the settings alone; what its other tables hold is read
-- from the profiles VTank's players published (the public meta repository's), each a table of
-- named, typed columns:
-- - Settings: the options, by name.
-- - MyMonsters: the monster list, a row each: the name, priority, damage type, weapon (the item's
--   id), the debuff, attack, ring and streak columns, the extra vulnerability, the offhand (the
--   item's id) and the pet's element. A row left at its default damage type holds 8, its extra
--   vulnerability 98 and its pet's element 101; the numbers of the others are not published, so
--   those are carried in columns of this plugin's own (`Dereth...`), and a VTank profile's are read
--   as their defaults.
-- - AssistItems: the kits, potions, foods, mana charges and stones the macro may use, by name and
--   kind: 0 a healing kit, 1 a health potion or food, 3 a stamina one, 6 a mana charge, 8 a mana
--   stone (the kinds the published profiles hold).
-- - GemFoodItems: the gems, foods and beers that buff, by name and the spell each casts.
-- - ExtraBuffSpells, AntiExtraBuffSpells: the buffs added to the default set and left out of it,
--   each by its family's first-level spell.
-- - BuffedItems: the combat items (the Items tab), by the item's id, each with the item buffs cast
--   on it by their families' first-level spells, or -1 for none (Add (No Buffs)).
-- An item is named in a profile by its id, so only the character's own items read back.

local S = require("settings")
local monsters = require("monsters")
local spells = require("spells")
local buffs = require("buffs")
local profiles = require("dereth.profiles")

local P = {}

-- The numbers the published profiles hold for a row left at its defaults.
local DAMAGE_DEFAULT, EXVULN_NONE, PETDMG_DEFAULT = 8, 98, 101

-- The monster-list columns that are debuffs, by the key a row of ours uses.
local DEBUFF_COLUMNS = {
	Imperil = "imperil", Vuln = "vuln", Yield = "yield", GravityW = "gravity", Broadside = "broadside",
	Fester = "fester", WeakeningCurse = "weaken", FesteringCurse = "festering", Corruption = "corruption",
	DestructiveCurse = "destructive", Corrosion = "corrosion",
}

-- The monster list's columns in the order the published profiles write them, then this plugin's
-- own.
local MONSTER_COLUMNS = {
	"MonsterName", "AttackPriority", "DamageType", "WeaponToUse", "Imperil", "Vuln", "Yield", "GravityW",
	"Attack", "Ring", "Broadside", "Fester", "WeakeningCurse", "FesteringCurse", "Corruption", "DestructiveCurse",
	"Corrosion", "Streak", "SecondaryVuln", "SecondaryEquip", "PetDamageType", "DerethDamage", "DerethWeapon",
	"DerethOffhand", "DerethExVuln", "DerethPetDamage",
}

-- The kinds AssistItems numbers.
local ASSIST = { kit = 0, health = 1, stamina = 3, charge = 6, stone = 8 }

-- An item the character holds by its id, or by its name (without case).
local function item_by_id(id)
	local o = id ~= nil and id ~= 0 and id ~= -1 and game.world:get(id % 4294967296) or nil
	return o
end
local function item_named(name)
	local want = string.lower(tostring(name or ""))
	for _, it in ipairs(game.inventory.items()) do
		if string.lower(it.name or "") == want then
			return it
		end
	end
	return nil
end

-- A list setting with names added (without case, each once).
local function add_to(setting, names)
	local list, seen, n = S.list(setting), {}, 0
	for _, p in ipairs(list) do
		seen[string.lower(p)] = true
	end
	for _, name in ipairs(names) do
		if name ~= "" and not seen[string.lower(name)] then
			seen[string.lower(name)] = true
			table.insert(list, name)
			n = n + 1
		end
	end
	S.set(setting, table.concat(list, ", "))
	return n
end

-- A spell's family, by its id, from the spell table.
local function family_of(id)
	local sp = magic.spell(id)
	return sp and (spells.family(sp.name)) or nil
end

-- Read a profile's text into the settings (those this macro has; the others are counted), the
-- monster list, the combat items, the consumables and the buffs. Answers a line saying what was
-- read.
function P.import(text)
	local t = profiles.usd(text)
	local set, unknown = 0, {}
	for _, row in ipairs(t.Settings or {}) do
		local name = row.Setting
		if name ~= nil and S.name(name) ~= nil then
			if S.set(name, row.Value) then
				set = set + 1
			end
		elseif name ~= nil then
			table.insert(unknown, name)
		end
	end
	local rows = {}
	for _, m in ipairs(t.MyMonsters or {}) do
		local debuffs = {}
		for col, key in pairs(DEBUFF_COLUMNS) do
			if m[col] == true then
				debuffs[key] = true
			end
		end
		local weapon, offhand = item_by_id(m.WeaponToUse), item_by_id(m.SecondaryEquip)
		table.insert(rows, {
			name = m.MonsterName or monsters.DEFAULT,
			priority = m.AttackPriority or 1,
			attack = m.Attack ~= false,
			ring = m.Ring == true,
			streak = m.Streak == true,
			damage = m.DerethDamage or "auto",
			weapon = m.DerethWeapon or (weapon and weapon.name) or "",
			offhand = m.DerethOffhand or (offhand and offhand.name) or "",
			exvuln = m.DerethExVuln or "none",
			petdmg = m.DerethPetDamage or "pauto",
			debuffs = debuffs,
		})
	end
	if #rows > 0 then
		monsters.save(rows)
	end
	-- The consumables: the kits, potions, foods, charges and stones, and the buff items.
	local names = {}
	for _, r in ipairs(t.AssistItems or {}) do
		table.insert(names, tostring(r.Object or ""))
	end
	for _, r in ipairs(t.GemFoodItems or {}) do
		table.insert(names, tostring(r.Name or ""))
	end
	local consumables = add_to("Consumables", names)
	-- The buffs added and left out, by their families.
	local function families(list)
		local out = {}
		for _, r in ipairs(list or {}) do
			local f = family_of(r.ExemplarId)
			if f ~= nil then
				table.insert(out, f)
			end
		end
		return out
	end
	local added = add_to("AddedBuffs", families(t.ExtraBuffSpells))
	local excluded = add_to("ExcludedBuffs", families(t.AntiExtraBuffSpells))
	buffs.want_settings()
	-- The combat items: those of the character's own (an item is named by its id), a pet to the
	-- pets and anything else to the weapons, never buffed where its only spell is -1.
	local spells_of, order = {}, {}
	for _, r in ipairs(t.BuffedItems or {}) do
		local id = r.Object
		if spells_of[id] == nil then
			spells_of[id] = {}
			table.insert(order, id)
		end
		table.insert(spells_of[id], r.Spell)
	end
	local items, elsewhere = 0, 0
	for _, id in ipairs(order) do
		local it = item_by_id(id)
		if it == nil then
			elsewhere = elsewhere + 1
		else
			local pet = string.find(string.lower(it.name or ""), "essence", 1, true) ~= nil
			items = items + add_to(pet and "Pets" or "Weapons", { it.name })
			local none = true
			for _, sp in ipairs(spells_of[id]) do
				none = none and sp == -1
			end
			if none then
				add_to("NoBuffItems", { it.name })
			end
		end
	end
	table.sort(unknown)
	return string.format("%d settings and %d monster rows read; %d combat items%s, %d consumables, %d buffs added, %d left out; %d settings this macro does not have%s",
		set, #rows, items, elsewhere > 0 and string.format(" (%d of another character's)", elsewhere) or "", consumables,
		added, excluded, #unknown,
		#unknown > 0 and (" (" .. table.concat(unknown, ", ", 1, math.min(#unknown, 12)) .. (#unknown > 12 and ", ..." or "") .. ")") or "")
end

-- A typed value as the format writes it.
local function cell(out, v)
	local kind = type(v)
	if kind == "boolean" then
		table.insert(out, "b")
		table.insert(out, v and "True" or "False")
	elseif kind == "number" and math.floor(v) == v and math.abs(v) < 2 ^ 31 then
		table.insert(out, "i")
		table.insert(out, string.format("%d", v))
	elseif kind == "number" then
		table.insert(out, "d")
		table.insert(out, string.format("%.10g", v))
	else
		table.insert(out, "s")
		table.insert(out, tostring(v))
	end
end

local function write_table(out, name, columns, rows)
	table.insert(out, name)
	table.insert(out, tostring(#columns))
	for _, c in ipairs(columns) do
		table.insert(out, c)
	end
	for i = 1, #columns do
		table.insert(out, i == 1 and "y" or "n")
	end
	table.insert(out, tostring(#rows))
	for _, r in ipairs(rows) do
		for i = 1, #columns do
			cell(out, r[i])
		end
	end
end

-- A profile's text with setting `name` set to `value` in its Settings table, its other tables as
-- they were (VTank's /vt opt setinall): the row of that setting changed, or one added. Answers the
-- text, or nil and why.
function P.set_in_text(text, name, value)
	local lines = {}
	for line in string.gmatch(text, "([^\n]*)\n?") do
		table.insert(lines, (string.gsub(line, "\r$", "")))
	end
	-- The Settings table: its name, then two columns, Setting and Value.
	local at = nil
	for i = 1, #lines - 3 do
		if lines[i] == "Settings" and lines[i + 1] == "2" and lines[i + 2] == "Setting" and lines[i + 3] == "Value" then
			at = i
			break
		end
	end
	if at == nil then
		return nil, "no Settings table"
	end
	-- After the names, a flag a column, then the rows' count and each row's two cells, a type and
	-- a value each.
	local count_at = at + 6
	local rows = tonumber(lines[count_at])
	if rows == nil then
		return nil, "the Settings table does not read"
	end
	local out = {}
	cell(out, value)
	local found = false
	for r = 0, rows - 1 do
		local base = count_at + 1 + r * 4
		if lines[base] ~= "s" then
			return nil, "the Settings table does not read"
		end
		if string.lower(lines[base + 1] or "") == string.lower(name) then
			lines[base + 2], lines[base + 3] = out[1], out[2]
			found = true
		end
	end
	if not found then
		local after = count_at + rows * 4
		lines[count_at] = tostring(rows + 1)
		table.insert(lines, after + 1, "s")
		table.insert(lines, after + 2, name)
		table.insert(lines, after + 3, out[1])
		table.insert(lines, after + 4, out[2])
	end
	-- The text's last line end, which the split leaves as an empty last line.
	if lines[#lines] == "" then
		table.remove(lines)
	end
	return table.concat(lines, "\r\n") .. "\r\n"
end

-- An item's id as the profile writes it: the signed 32-bit number.
local function signed(id)
	return id >= 2147483648 and id - 4294967296 or id
end

-- The spell an item casts when used, from what every client is told of it.
local SPELL_ID = 0x0D000008

-- What a consumable is, as the profile's tables keep it: `"assist", kind` or `"gemfood", spell`,
-- or nil for a kind the published profiles hold none of (it stays in the Consumables setting).
local function consumable_entry(name)
	local it = item_named(name)
	if it == nil then
		return nil
	end
	local n = string.lower(it.name or "")
	if it.class == "HealingKit" then
		-- A healing kit (a stamina or mana kit is of no kind the published profiles hold).
		if string.find(n, "stamina", 1, true) or string.find(n, "mana", 1, true) then
			return nil
		end
		return "assist", ASSIST.kit
	elseif it.class == "ManaStone" then
		return "assist", string.find(n, "charge", 1, true) and ASSIST.charge or ASSIST.stone
	end
	local id = it:int(SPELL_ID)
	local sp = id and id > 0 and magic.spell(id) or nil
	if sp ~= nil and sp.duration ~= nil and sp.duration >= 300 then
		return "gemfood", id
	end
	local what = string.lower(sp and sp.name or it.name or "")
	if string.find(what, "stamina", 1, true) then
		return "assist", ASSIST.stamina
	elseif string.find(what, "health", 1, true) or string.find(what, "heal", 1, true) then
		return "assist", ASSIST.health
	end
	return nil
end

-- The id of a family's first-level spell, from the character's spell book (nil when it knows
-- none).
local function level_one(family)
	return spells.level_one(family)
end

-- The macro's settings (those that differ from the defaults, as VTank writes only what was set),
-- its monster list, its consumables, buffs and combat items, as a profile's text. Inside a task
-- the spell book is read again first (a page a frame), for the first spells of the buffs' families;
-- elsewhere the book as last read serves.
function P.export()
	if coroutine.isyieldable() then
		buffs.want_settings()
		pcall(spells.refresh, true)
	end
	local out = { "8" }
	-- The buffs added and left out, by their families' first-level spells.
	local function exemplars(setting)
		local rows = {}
		for _, f in ipairs(S.list(setting)) do
			local id = level_one(f)
			if id ~= nil then
				table.insert(rows, { id })
			end
		end
		return rows
	end
	write_table(out, "AntiExtraBuffSpells", { "ExemplarId" }, exemplars("ExcludedBuffs"))
	-- The consumables, by kind.
	local assist, gemfood = {}, {}
	for _, name in ipairs(S.list("Consumables")) do
		local where, v = consumable_entry(name)
		if where == "assist" then
			table.insert(assist, { item_named(name).name, v })
		elseif where == "gemfood" then
			table.insert(gemfood, { item_named(name).name, v })
		end
	end
	write_table(out, "AssistItems", { "Object", "Type" }, assist)
	-- The combat items the character holds, each with the item buffs cast on it (a weapon's or a
	-- caster's), or none (-1) for one never buffed and for a pet.
	local buffed = {}
	local function item_rows(it, families)
		local id = signed(it.id)
		if S.listed("NoBuffItems", it.name) or families == nil then
			table.insert(buffed, { id, -1 })
			return
		end
		for _, f in ipairs(families) do
			local sp = level_one(f)
			if sp ~= nil then
				table.insert(buffed, { id, sp })
			end
		end
	end
	for _, name in ipairs(S.list("Weapons")) do
		local it = item_named(name)
		if it ~= nil and (it.class == "MeleeWeapon" or it.class == "MissileWeapon") then
			item_rows(it, buffs.WEAPON)
		elseif it ~= nil and it.class == "WandStaffOrb" then
			item_rows(it, buffs.CASTER)
		end
	end
	for _, name in ipairs(S.list("Pets")) do
		local it = item_named(name)
		if it ~= nil then
			item_rows(it, nil)
		end
	end
	write_table(out, "BuffedItems", { "Object", "Spell" }, buffed)
	write_table(out, "ExtraBuffSpells", { "ExemplarId" }, exemplars("AddedBuffs"))
	write_table(out, "GemFoodItems", { "Name", "Spell" }, gemfood)
	write_table(out, "ItemUseSpecifiers", { "Object", "Uses" }, {})
	-- The monster list: VTank's columns, its numbers where they are published (a row's defaults),
	-- the items by their ids, and this plugin's own columns for the rest.
	local rows = {}
	for _, m in ipairs(monsters.rows()) do
		local weapon, offhand = item_named(m.weapon), item_named(m.offhand)
		local r = {
			MonsterName = m.name, AttackPriority = m.priority or 1, DamageType = DAMAGE_DEFAULT,
			WeaponToUse = weapon and signed(weapon.id) or -1, Attack = m.attack ~= false, Ring = m.ring == true,
			Streak = m.streak == true, SecondaryVuln = EXVULN_NONE, SecondaryEquip = offhand and signed(offhand.id) or 0,
			PetDamageType = PETDMG_DEFAULT, DerethDamage = m.damage or "auto", DerethWeapon = m.weapon or "",
			DerethOffhand = m.offhand or "", DerethExVuln = m.exvuln or "none", DerethPetDamage = m.petdmg or "pauto",
		}
		for col, key in pairs(DEBUFF_COLUMNS) do
			r[col] = (m.debuffs or {})[key] == true
		end
		local row = {}
		for i, col in ipairs(MONSTER_COLUMNS) do
			row[i] = r[col]
		end
		table.insert(rows, row)
	end
	write_table(out, "MyMonsters", MONSTER_COLUMNS, rows)
	local settings = {}
	for _, name in ipairs(S.names()) do
		local v = S.get(name)
		if v ~= S.defaults[name] then
			table.insert(settings, { name, v })
		end
	end
	write_table(out, "Settings", { "Setting", "Value" }, settings)
	return table.concat(out, "\r\n") .. "\r\n"
end

return P
