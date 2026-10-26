-- Getting health, stamina and mana back, and keeping worn items' mana up, as VTank does.
--
-- VTank checks a list of ways in order and uses the first it can, the list chosen by the vital and
-- the stance (its FAQ, "What are Virindi Tank's priorities for which type of recharge to use?"):
--
--   health, magic stance:   under 15%: stamina to health, mana to health, heal self, a health
--                           food; then a kit (UseKitsInMagicMode), stamina to health, mana to
--                           health, heal self, a health food
--   health, other stances:  under 15%: a health food; then a kit, a health food, stamina to
--                           health, heal self
--   stamina or mana, magic: a kit of that vital (UseKitsInMagicMode), a food of that vital, the
--                           spell (Stamina to Mana Self, or Revitalize Self for stamina)
--   stamina or mana, other: a kit of that vital, a food of that vital, the spell
--
-- Kits, foods and potions are those the Consumables setting names (empty: any the pack holds). A
-- food's vital is the one its spell restores, read from an appraisal (else from its name). Stamina
-- to Health and Mana to Health are weighed against a heal (and the restam or the mana each costs to
-- get back), as VTank weighs them, and cast when they beat it by StaminaToHealthMultiplier or
-- ManaToHealthMultiplier. With GoToPeaceModeToUseKits a kit is used in peace mode. After switching to magic stance for a
-- restoring spell, the marks are raised by RechargeBoostAmount points for RechargeBoostTimeSeconds
-- (VTank's recharge boost: its options say it then prefers to recharge), until the next spell of
-- another kind with ClearLevelBoostFlagOnCast. A caster's own spell (the Casting Stein's Stamina
-- to Mana, any "X to Y" spell built into a wand) is a restoring spell too, checked with the
-- character's own of its kind and used before them (VTank's FAQ: wand spells are "treated as normal
-- recharge spells and checked along with them"): the caster magic stance takes up, used on the
-- character where it is held, its spell cast. Mana stones and charges refill the worn items under
-- RefillWornMana-Item-ManaPercent, used on the character; empty stones are filled from looted
-- items the loot profile does not keep.

local S = require("settings")
local spells = require("spells")

local R = {}

-- The spell for each vital, and the two transfers to health VTank weighs against a heal.
R.SPELLS = { health = "Heal Self", stamina = "Revitalize Self", mana = "Stamina to Mana Self" }
R.S2H, R.M2H = "Stamina to Health Self", "Mana to Health Self"
spells.want({ R.SPELLS.health, R.SPELLS.stamina, R.SPELLS.mana, R.S2H, R.M2H })

-- Int properties an appraisal tells of an item's mana.
local CUR_MANA, MAX_MANA = 107, 108
-- An item's own spell, as its description tells it.
local OWN_SPELL = 0x0D000008

-- The spell families a caster's own spell may be of, at each step of the lists that casts one: the
-- transfers to health, and the spell of each vital.
local WAND_FAMILIES = {
	s2h = { ["stamina to health self"] = true },
	m2h = { ["mana to health self"] = true },
	health = { ["heal self"] = true },
	stamina = { ["revitalize self"] = true, ["mana to stamina self"] = true },
	mana = { ["stamina to mana self"] = true },
}

-- The caster magic stance takes up (as the combat library's ready_magic chooses it): the one held
-- when CasterWeapon names none or names it, else the one CasterWeapon names, else the first in the
-- pack; or nil.
local function stance_caster()
	local me = game.player.id
	local want = string.lower(S.get("CasterWeapon") or "")
	local first = nil
	for _, it in ipairs(game.inventory.items()) do
		if it.class == "WandStaffOrb" then
			local named = want == "" or string.lower(it.name or "") == want
			if it.wielder == me and named then
				return it
			end
			if first == nil and named then
				first = it
			end
		end
	end
	return first
end

-- A caster's magic works for a character whose Arcane Lore is at least the caster's difficulty
-- (its appraisal tells it); one whose spell did nothing is left a while.
local ITEM_DIFFICULTY = 109
local wand_difficulty, wand_rest = {}, {}
local WAND_REST = 120

local function arcane_lore()
	for _, sk in ipairs(game.player.skills or {}) do
		if sk.name == "Arcane Lore" then
			return sk.current
		end
	end
	return nil
end

-- Whether the character can use `it`'s own spell now.
local function wand_usable(it)
	if (wand_rest[it.id] or 0) > os.clock() then
		return false
	end
	local d = wand_difficulty[it.id]
	if d == nil then
		local ok, a = pawait(act.appraise(it.id))
		d = ok and a and a.ints and a.ints[ITEM_DIFFICULTY] or 0
		wand_difficulty[it.id] = d
	end
	local lore = arcane_lore()
	return d == 0 or lore == nil or lore >= d
end

-- The caster whose own spell serves step `kind` (s2h, m2h) or the spell of `vital`, and its spell,
-- or nil.
local function wand_for(kind, vital)
	local families = WAND_FAMILIES[kind == "spell" and vital or kind]
	local it = families and stance_caster()
	local id = it and it:int(OWN_SPELL)
	local sp = id and id > 0 and magic.spell(id) or nil
	if sp ~= nil and families[string.lower(spells.family(sp.name))] and wand_usable(it) then
		return it, sp
	end
	return nil
end
R.wand_for = wand_for

local function consumable(name)
	local list = S.get("Consumables") or ""
	if not string.find(list, "%S") then
		return true
	end
	local want = string.lower(name or "")
	for part in string.gmatch(list, "[^,]+") do
		if string.lower(string.match(part, "^%s*(.-)%s*$")) == want then
			return true
		end
	end
	return false
end

R.consumable = consumable

-- A kit's vital, by its name (kits name what they restore).
local function kit_vital(name)
	local n = string.lower(name or "")
	if string.find(n, "stamina", 1, true) then
		return "stamina"
	elseif string.find(n, "mana", 1, true) then
		return "mana"
	end
	return "health"
end

-- The recharge boost: until when it lasts (nil: none).
R.boost_until = nil

-- Whether the recharge boost lasts.
function R.boosted()
	return R.boost_until ~= nil and os.clock() < R.boost_until
end

-- A spell of another kind than a restoring one was cast: with ClearLevelBoostFlagOnCast, the boost
-- ends.
function R.other_cast()
	if S.get("ClearLevelBoostFlagOnCast") then
		R.boost_until = nil
	end
end

-- Whether `vital` is under the mark the setting `setting` names (a percent of its maximum), the
-- mark raised by RechargeBoostAmount points while the boost lasts (a full vital is never low).
function R.low(vital, setting)
	local v = game.player[vital]
	if v == nil or v.max == nil or v.max == 0 or v.current == nil then
		return false
	end
	local mark = v.max * S.share(setting)
	if R.boosted() then
		mark = math.min(mark + S.get("RechargeBoostAmount"), v.max)
	end
	return v.current < mark
end

-- The vital a text names, or nil.
local function vital_in(text)
	local n = string.lower(text or "")
	if string.find(n, "mana", 1, true) then
		return "mana"
	elseif string.find(n, "stamina", 1, true) or string.find(n, "revitaliz", 1, true) then
		return "stamina"
	elseif string.find(n, "heal", 1, true) or string.find(n, "health", 1, true) then
		return "health"
	end
	return nil
end

-- Foods by name: the vital each restores (false: none of the three), from an appraisal.
local food_vitals = {}

local function food_vital(item)
	local known = food_vitals[item.name]
	if known ~= nil then
		return known or nil
	end
	local ok, a = pawait(act.appraise(item.id))
	local v = nil
	if ok and a ~= nil then
		for _, id in ipairs(a.spells or {}) do
			local sp = magic.spell(id)
			v = sp and vital_in(sp.name)
			if v ~= nil then
				break
			end
		end
	end
	v = v or vital_in(item.name)
	food_vitals[item.name] = v or false
	return v
end

function R.forget()
	food_vitals = {}
end

-- Kits by name: { boost, uses } from an appraisal (the kit's bonus to the skill, its uses left).
local kit_facts = {}
local BOOST, USES = 90, 92

local function facts(it)
	local f = kit_facts[it.id]
	if f == nil then
		local ok, a = pawait(act.appraise(it.id))
		local ints = ok and a and a.ints or {}
		f = { boost = ints[BOOST] or 0, uses = ints[USES] or 0 }
		kit_facts[it.id] = f
	end
	return f
end

-- A heal with a kit lands, as the game rolls it: the healing skill and the kit's bonus (half as
-- much again when specialised, a tenth when trained) against twice the health missing (a tenth
-- more in a combat stance), on the usual skill curve.
function R.kit_chance(boost)
	local h = game.player.health
	local skill, spec = nil, false
	for _, sk in ipairs(game.player.skills or {}) do
		if sk.name == "Healing" then
			skill, spec = sk.current, sk.training == "specialized"
		end
	end
	if h == nil or h.max == nil or skill == nil then
		return 1
	end
	local effective = math.floor((skill + (boost or 0)) * (spec and 1.5 or 1.1) + 0.5)
	local combat_mod = (game.player.combat_mode or 1) == 1 and 1.0 or 1.1
	local difficulty = math.floor((h.max - h.current) * 2 * combat_mod + 0.5)
	return 1 - 1 / (1 + math.exp(0.03 * (effective - difficulty)))
end

-- The kit of `vital` to use: as VTank chooses, of those likely enough to succeed
-- (MinimumHealKitSuccessChance, for health) the one with the highest bonus, and of equals the one
-- with the fewest uses left.
local function kit(vital)
	local best, best_f = nil, nil
	local min = S.share("MinimumHealKitSuccessChance")
	for _, it in ipairs(game.inventory.items()) do
		if it.class == "HealingKit" and kit_vital(it.name) == vital and consumable(it.name) then
			local f = facts(it)
			if vital ~= "health" or R.kit_chance(f.boost) >= min then
				if best == nil or f.boost > best_f.boost or (f.boost == best_f.boost and f.uses < best_f.uses) then
					best, best_f = it, f
				end
			end
		end
	end
	return best
end

-- The first food or potion that restores `vital`.
local function food(vital)
	for _, it in ipairs(game.inventory.items()) do
		if it.class == "Food" and consumable(it.name) and food_vital(it) == vital then
			return it
		end
	end
	return nil
end

-- The spell for `vital`: for health the HealSpells setting first, else the best Heal Self known.
local function spell(vital)
	if vital == "health" then
		for _, id in ipairs(S.numbers("HealSpells")) do
			local sp = magic.spell(id)
			if sp ~= nil then
				return { id = id, name = sp.name }
			end
		end
	end
	-- Cast when mana is low, so held to the hunting margin, as VTank holds them.
	return spells.best(R.SPELLS[vital], S.get("MaxSpellLevel"), S.get("SpellDiffExcessThreshold-Hunt"))
end

-- What a restoring spell gives, from its description: the average of "Restores A-B points", or for
-- a transfer the share it drains and the percentage it gives (with any maximum).
local told = {}
local function said(id)
	local t = told[id]
	if t == nil then
		local sp = magic.spell(id)
		local d = string.lower(sp and sp.description or "")
		t = {}
		local a, b = string.match(d, "restores (%d+)%-(%d+) points")
		if a ~= nil then
			t.amount = (tonumber(a) + tonumber(b)) / 2
		end
		local share = string.match(d, "drains (%S+) of")
		local shares = { ["one-half"] = 0.5, ["one-quarter"] = 0.25, ["one-third"] = 1 / 3 }
		t.share = shares[share or ""] or (share and tonumber(string.match(share, "(%d+)%%") or "") or 0) / 100
		t.pct = tonumber(string.match(d, "gives (%d+)%%") or "")
		t.max = tonumber(string.match(d, "maximum of (%d+)") or "")
		t.mana = sp and sp.mana or 0
		told[id] = t
	end
	return t
end

-- Health a mana spends, for a heal spell, and for Stamina to Health and Mana to Health as VTank
-- weighs them: a transfer must beat a heal by its multiplier, counting the restam (Revitalize) it
-- costs to get the stamina back, and for Mana to Health the mana it drains too.
local function per_mana(kind)
	local h = game.player.health
	local st, mn = game.player.stamina, game.player.mana
	if h == nil or st == nil or mn == nil then
		return nil
	end
	local function gain(t, from)
		local g = (from or 0) * (t.share or 0) * (t.pct or 0) / 100
		if t.max ~= nil then
			g = math.min(g, t.max)
		end
		return math.min(g, h.max - h.current)
	end
	if kind == "spell" then
		local s = spell("health")
		local t = s and said(s.id)
		return t and t.amount and (t.mana or 0) > 0 and t.amount / t.mana or nil, s
	end
	local revit = spells.best(R.SPELLS.stamina, S.get("MaxSpellLevel"), S.get("SpellDiffExcessThreshold-Hunt"))
	local rt = revit and said(revit.id)
	if kind == "s2h" then
		local s = spells.best(R.S2H, S.get("MaxSpellLevel"), S.get("SpellDiffExcessThreshold-Hunt"))
		if s == nil then
			return nil
		end
		local t = said(s.id)
		local drained = st.current * (t.share or 0)
		local restam = (rt and rt.amount and rt.amount > 0) and (drained / rt.amount) * (rt.mana or 0) or 0
		local g = gain(t, st.current)
		return g / math.max(1, (t.mana or 0) + restam), s
	end
	local s = spells.best(R.M2H, S.get("MaxSpellLevel"), S.get("SpellDiffExcessThreshold-Hunt"))
	if s == nil then
		return nil
	end
	local t = said(s.id)
	local drained = mn.current * (t.share or 0)
	return gain(t, mn.current) / math.max(1, (t.mana or 0) + drained), s
end

-- A transfer to health, when it beats a heal by its multiplier (StaminaToHealthMultiplier,
-- ManaToHealthMultiplier): the spell, or nil.
local function transfer(kind)
	local heal = per_mana("spell")
	local e, s = per_mana(kind)
	if e == nil or s == nil then
		return nil
	end
	local mult = kind == "s2h" and S.get("StaminaToHealthMultiplier") or S.get("ManaToHealthMultiplier")
	if heal == nil or e >= heal * mult then
		return s
	end
	return nil
end

-- VTank's list for `vital` in `stance` ("magic" or another), `low` when health is under 15%: the
-- ways to try, in order, each { kind = "kit" | "food" | "spell" | "s2h" | "m2h" }.
function R.order(vital, stance, low)
	local magic_stance = stance == "magic"
	local kits_ok = not magic_stance or S.get("UseKitsInMagicMode")
	local out = {}
	local function add(kind)
		table.insert(out, kind)
	end
	if vital == "health" then
		if magic_stance then
			if low then
				add("s2h")
				add("m2h")
				add("spell")
				add("food")
			end
			if kits_ok and S.get("UseHealingKits") then
				add("kit")
			end
			add("s2h")
			add("m2h")
			add("spell")
			add("food")
		else
			if low then
				add("food")
			end
			if S.get("UseHealingKits") then
				add("kit")
			end
			add("food")
			add("s2h")
			add("spell")
		end
	else
		if kits_ok then
			add("kit")
		end
		if vital == "stamina" and magic_stance then
			-- In magic stance VTank tries the spell before stamina foods.
			add("spell")
			add("food")
		else
			add("food")
			add("spell")
		end
	end
	return out
end

-- Until when the restoring spells wait, after one the character had too little mana for.
local spell_wait = 0

-- The first way `R.order` gives that is at hand: { kind, item | spell }, or nil.
function R.choose(vital, stance, low)
	for _, kind in ipairs(R.order(vital, stance, low)) do
		if kind == "kit" then
			local it = kit(vital)
			if it ~= nil then
				return { kind = kind, item = it }
			end
		elseif kind == "food" then
			local it = food(vital)
			if it ~= nil then
				return { kind = kind, item = it }
			end
		else
			-- A caster's own spell of the kind first: the caster pays its mana.
			local wand, wand_spell = wand_for(kind, vital)
			if wand ~= nil then
				return { kind = "wand", item = wand, spell = wand_spell }
			end
			if kind == "s2h" or kind == "m2h" then
				local sp = os.clock() >= spell_wait and transfer(kind) or nil
				if sp ~= nil then
					return { kind = kind, spell = sp }
				end
			elseif os.clock() >= spell_wait then
				local sp = spell(vital)
				if sp ~= nil then
					return { kind = kind, spell = sp }
				end
			end
		end
	end
	return nil
end

-- Use one way of getting `vital` back; answers ok, a line for the log (nil when none was at hand).
function R.step(vital, stance, low)
	local way = R.choose(vital, stance, low)
	if way == nil then
		return false, nil
	end
	local ok, why
	if way.kind == "kit" then
		-- With GoToPeaceModeToUseKits, a kit is used in peace mode.
		if S.get("GoToPeaceModeToUseKits") and (game.player.combat_mode or 1) > 1 then
			combat.stop()
			pawait(combat.set_mode("peace"))
		end
		ok, why = pawait(act.use_on(way.item.id, game.player.id))
	elseif way.kind == "food" then
		ok, why = pawait(act.use(way.item.id))
	elseif way.kind == "wand" then
		-- The caster's own spell: in magic stance with it in hand, it is used on the character.
		local was = game.player.combat_mode
		if R.ready ~= nil and not R.ready() then
			return false, string.format("%s: %s: not in magic stance", vital, way.spell.name)
		end
		if was ~= 8 and S.get("RechargeBoostTimeSeconds") > 0 then
			R.boost_until = os.clock() + S.get("RechargeBoostTimeSeconds")
		end
		local before = game.player[vital] and game.player[vital].current or 0
		ok, why = pawait(act.use_on(way.item.id, game.player.id, { purpose = "heal" }))
		local what = string.format("%s's %s", way.item.name, way.spell.name)
		if ok then
			-- A use the server takes without casting (the caster's magic beyond the character)
			-- gives nothing back: the caster is left a while.
			task.sleep(500)
			local now = game.player[vital] and game.player[vital].current or 0
			if now <= before then
				wand_rest[way.item.id] = os.clock() + WAND_REST
				return false, string.format("%s: %s did nothing; left %d s", vital, what, WAND_REST)
			end
		end
		return ok, ok and string.format("%s: %s", vital, what) or string.format("%s: %s: %s", vital, what, tostring(why))
	else
		-- Spells are cast in magic stance; the macro gets into it (R.ready). Switching to it for
		-- the spell starts the recharge boost.
		local was = game.player.combat_mode
		if R.ready ~= nil and not R.ready() then
			return false, string.format("%s: %s: not in magic stance", vital, way.spell.name)
		end
		if was ~= 8 and S.get("RechargeBoostTimeSeconds") > 0 then
			R.boost_until = os.clock() + S.get("RechargeBoostTimeSeconds")
		end
		ok, why = pawait(magic.cast(way.spell.id, nil, vital == "health" and { purpose = "heal" } or nil))
		-- Too little mana for the spell: the spells wait a while (a kit or a food may serve meanwhile,
		-- or a rest), rather than being tried again and again.
		if not ok and string.find(tostring(why), "0x0401", 1, true) then
			spell_wait = os.clock() + 15
		end
	end
	local what = way.item and way.item.name or way.spell.name
	return ok, ok and string.format("%s: %s", vital, what) or string.format("%s: %s: %s", vital, what, tostring(why))
end

-- Whether anything is at hand for `vital`.
function R.can(vital, stance, low)
	return R.choose(vital, stance, low) ~= nil
end

-- Worn items' mana --------------------------------------------------------------------------------

-- Item id -> { cur, max, at } from the last appraisal.
local mana_of = {}
local LOOK_AGAIN = 120

local function mana(it, fresh)
	local m = mana_of[it.id]
	if m ~= nil and not fresh and os.clock() - m.at < LOOK_AGAIN then
		return m
	end
	local ok, a = pawait(act.appraise(it.id))
	if not ok or a == nil or a.ints == nil then
		return m
	end
	m = { cur = a.ints[CUR_MANA], max = a.ints[MAX_MANA], at = os.clock() }
	mana_of[it.id] = m
	return m
end

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

-- Mana stones and charges in the pack, with what each holds (appraised).
local function stones()
	local out = {}
	for _, it in ipairs(game.inventory.items()) do
		if it.class == "ManaStone" and it.wielder == nil then
			table.insert(out, { item = it, mana = mana(it) })
		end
	end
	return out
end

-- The worn item lowest on mana under the setting's mark, or nil.
function R.worn_needing_mana()
	if not S.get("RefillWornMana") then
		return nil
	end
	local mark = S.share("RefillWornMana-Item-ManaPercent")
	local worst, share = nil, nil
	for _, it in ipairs(worn()) do
		local m = mana(it)
		if m ~= nil and m.max ~= nil and m.max > 0 and m.cur ~= nil then
			local s = m.cur / m.max
			if s < mark and (share == nil or s < share) then
				worst, share = it, s
			end
		end
	end
	return worst, share
end

-- Refill the worn items from a charged stone or a charge: answers ok and a line, or false, nil when
-- none needs it or nothing can fill it.
function R.refill_worn()
	local item, share = R.worn_needing_mana()
	if item == nil then
		return false, nil
	end
	for _, s in ipairs(stones()) do
		if consumable(s.item.name) and s.mana ~= nil and (s.mana.cur or 0) > 0 then
			local ok, why = pawait(act.use_on(s.item.id, game.player.id))
			mana_of[item.id] = nil
			mana_of[s.item.id] = nil
			if ok then
				return true, string.format("refilled worn mana (%s at %d%%) from %s", item.name, math.floor(share * 100), s.item.name)
			end
			return false, string.format("%s: %s", s.item.name, tostring(why))
		end
	end
	return false, nil
end

-- Mana stones the pack holds: charged and empty.
function R.stone_counts()
	local full, empty = 0, 0
	for _, s in ipairs(stones()) do
		if s.mana ~= nil and (s.mana.cur or 0) > 0 then
			full = full + 1
		else
			empty = empty + 1
		end
	end
	return full, empty
end

-- Whether a looted mana stone is to be kept (fewer than ManaStoneLootCount held).
function R.wants_stone(item)
	if item.class ~= "ManaStone" or S.get("ManaStoneLootCount") <= 0 or not consumable(item.name) then
		return false
	end
	local full, empty = R.stone_counts()
	return full + empty < S.get("ManaStoneLootCount")
end

-- A looted item the profile does not keep: when it holds ManaTankMinimumMana or more and an empty
-- stone is held, take it and drain it into the stone. Answers a line when it did.
function R.drain_into_stone(item)
	if S.get("ManaStoneLootCount") <= 0 then
		return nil
	end
	local empty_stone = nil
	for _, s in ipairs(stones()) do
		if consumable(s.item.name) and (s.mana == nil or (s.mana.cur or 0) == 0) then
			empty_stone = s.item
			break
		end
	end
	if empty_stone == nil then
		return nil
	end
	local m = mana(item, true)
	if m == nil or (m.cur or 0) < S.get("ManaTankMinimumMana") then
		return nil
	end
	local took, why = pawait(items.move(item.id))
	if not took then
		return string.format("could not take %s for its mana: %s", item.name, tostring(why))
	end
	local ok, why2 = pawait(act.use_on(empty_stone.id, item.id))
	mana_of[empty_stone.id] = nil
	if ok then
		return string.format("drained %s (%d mana) into %s", item.name, m.cur, empty_stone.name)
	end
	return string.format("could not drain %s: %s", item.name, tostring(why2))
end

return R
