-- Keeping buffs up, as VTank does: by default every trained attribute buff, a buff for each trained
-- skill, the protections (those BuffProfile_Prots names) and the regeneration buffs, and the buffs
-- AddedBuffs adds (VTank's Buffs tab), each the best level the character knows, recast when it has
-- less than RebuffTimeRemainingSeconds left (with nothing to fight, from
-- IdleBuffTopoffTimeSeconds); buffs on items: the weapon held and the armour worn (the banes those
-- BuffProfile_Banes names); and the buffs of the buff items the Consumables name (beers, foods,
-- gems), used when their buff wants it.

local S = require("settings")
local spells = require("spells")

local B = {}

-- The self buff of each skill, by the skill's name.
B.SKILLS = {
	["Heavy Weapons"] = "Heavy Weapon Mastery Self",
	["Light Weapons"] = "Light Weapon Mastery Self",
	["Finesse Weapons"] = "Finesse Weapon Mastery Self",
	["Two Handed Combat"] = "Two Handed Combat Mastery Self",
	["Missile Weapons"] = "Missile Weapon Mastery Self",
	["Melee Defense"] = "Invulnerability Self",
	["Missile Defense"] = "Impregnability Self",
	["Magic Defense"] = "Magic Resistance Self",
	["War Magic"] = "War Magic Mastery Self",
	["Void Magic"] = "Void Magic Mastery Self",
	["Life Magic"] = "Life Magic Mastery Self",
	["Creature Enchantment"] = "Creature Enchantment Mastery Self",
	["Item Enchantment"] = "Item Enchantment Mastery Self",
	["Mana Conversion"] = "Mana Conversion Mastery Self",
	["Healing"] = "Healing Mastery Self",
	["Arcane Lore"] = "Arcane Enlightenment Self",
	["Run"] = "Sprint Self",
	["Jump"] = "Jumping Mastery Self",
	["Shield"] = "Shield Mastery Self",
	["Dual Wield"] = "Dual Wield Mastery Self",
	["Recklessness"] = "Recklessness Mastery Self",
	["Sneak Attack"] = "Sneak Attack Mastery Self",
	["Dirty Fighting"] = "Dirty Fighting Mastery Self",
	["Summoning"] = "Summoning Mastery Self",
}

-- Always wanted: the attributes, the protections and the regeneration buffs.
B.ALWAYS = {
	"Strength Self", "Endurance Self", "Coordination Self", "Quickness Self", "Focus Self",
	"Willpower Self", "Armor Self", "Blade Protection Self", "Piercing Protection Self",
	"Bludgeoning Protection Self", "Fire Protection Self", "Cold Protection Self", "Acid Protection Self",
	"Lightning Protection Self", "Regeneration Self", "Rejuvenation Self", "Mana Renewal Self",
}

-- Cast first, as players buff: the buffs that raise the casting skills (the magic masteries, and
-- Focus and Willpower, which the magic skills are reckoned from), so that the buffs after them are
-- cast at the higher levels the raised skill reaches ("pyramid" buffing).
B.FIRST = {
	"Creature Enchantment Mastery Self", "Focus Self", "Willpower Self", "Item Enchantment Mastery Self",
	"Life Magic Mastery Self", "Mana Conversion Mastery Self",
}

-- The buffs cast on the weapon held, and on each piece of armour worn.
B.WEAPON = { "Blood Drinker", "Swift Killer", "Heart Seeker", "Defender" }
-- The buffs cast on the caster held: its war and void damage, its mana conversion, defence.
B.CASTER = { "Spirit Drinker", "Hermetic Link", "Defender" }
-- Where the weapon buffs are auras instead (cast on the character, they work on whatever weapon
-- or caster it holds): those for a weapon, and those for a caster.
B.WEAPON_AURAS = {
	"Aura of Blood Drinker Self", "Aura of Swift Killer Self", "Aura of Heart Seeker Self", "Aura of Defender Self",
}
B.CASTER_AURAS = { "Aura of Spirit Drinker Self", "Aura of Hermetic Link Self", "Aura of Defender Self" }
spells.want(B.WEAPON_AURAS)
spells.want(B.CASTER_AURAS)
B.ARMOR = {
	"Impenetrability", "Blade Bane", "Piercing Bane", "Bludgeon Bane", "Flame Bane", "Frost Bane",
	"Acid Bane", "Lightning Bane",
}

spells.want(B.ALWAYS)
spells.want(B.WEAPON)
spells.want(B.CASTER)
spells.want(B.ARMOR)
for _, f in pairs(B.SKILLS) do
	spells.want({ f })
end

local function list_setting(name)
	return S.list(name)
end

-- The spell book keeps the families BuffSpells, AddedBuffs and ExcludedBuffs name too.
function B.want_settings()
	spells.want(list_setting("BuffSpells"))
	spells.want(list_setting("AddedBuffs"))
	spells.want(list_setting("ExcludedBuffs"))
end

-- VTank's buff profiles [faq]: which protections and banes are cast, by the letters of their
-- elements (B bludgeon, P pierce, S slash, A acid, L lightning, F fire, C cold).
B.PROTS = {
	B = "Bludgeoning Protection Self", P = "Piercing Protection Self", S = "Blade Protection Self",
	A = "Acid Protection Self", L = "Lightning Protection Self", F = "Fire Protection Self", C = "Cold Protection Self",
}
B.BANES = {
	B = "Bludgeon Bane", P = "Piercing Bane", S = "Blade Bane", A = "Acid Bane", L = "Lightning Bane",
	F = "Flame Bane", C = "Frost Bane",
}
-- The built-in selections, and Custom (its letters in BuffProfile-Prots or BuffProfile-Banes).
local PRESETS = { all = "BPSALFC", none = "", b = "B", bps = "BPS", bpsa = "BPSA", alfc = "ALFC" }

-- The element letters a buff profile names: `which` is "Prots" or "Banes".
function B.profile_letters(which)
	local mode = string.lower(tostring(S.get("BuffProfile_" .. which) or "all"))
	if PRESETS[mode] ~= nil then
		return PRESETS[mode]
	end
	if mode == "custom" then
		return string.upper(tostring(S.get("BuffProfile-" .. which) or ""))
	end
	-- Letters given in place of a selection are taken as the custom set.
	return string.upper(mode)
end

-- Whether a protection or bane family is one the profile casts (any other family is).
local function in_profile(family, which)
	local table_ = which == "Prots" and B.PROTS or B.BANES
	local letters = B.profile_letters(which)
	for letter, f in pairs(table_) do
		if string.lower(f) == string.lower(family) then
			return string.find(letters, letter, 1, true) ~= nil
		end
	end
	return true
end
B.in_profile = in_profile

-- The self buff families the set holds, and why each (lower-cased family -> "chosen" for the
-- BuffSpells setting's, "default", "added" for AddedBuffs', "aura" for an item buff cast as an
-- aura): the BuffSpells setting when it names any, else the defaults for this character and the
-- buffs added.
local function candidates()
	local fams, why = {}, {}
	for _, f in ipairs(list_setting("BuffSpells")) do
		table.insert(fams, f)
		why[string.lower(f)] = "chosen"
	end
	if #fams == 0 then
		for _, f in ipairs(B.ALWAYS) do
			-- The protections the buff profile names.
			if in_profile(f, "Prots") then
				table.insert(fams, f)
				why[string.lower(f)] = "default"
			end
		end
		for _, sk in ipairs(game.player.skills or {}) do
			if sk.training ~= "untrained" and B.SKILLS[sk.name] ~= nil then
				table.insert(fams, B.SKILLS[sk.name])
				why[string.lower(B.SKILLS[sk.name])] = "default"
			end
		end
		-- The buffs added to the set (VTank's Buffs tab: a skill not trained, say).
		for _, f in ipairs(list_setting("AddedBuffs")) do
			if why[string.lower(f)] == nil then
				why[string.lower(f)] = "added"
				table.insert(fams, f)
			end
		end
	end
	-- With BuffItems, the weapon auras: a weapon's for a character carrying a weapon, a caster's
	-- for one carrying a caster (each family once).
	if S.get("BuffItems") then
		local weapon, caster = false, false
		for _, it in ipairs(game.inventory.items()) do
			weapon = weapon or it.class == "MeleeWeapon" or it.class == "MissileWeapon"
			caster = caster or it.class == "WandStaffOrb"
		end
		local seen = {}
		for _, f in ipairs(fams) do
			seen[string.lower(f)] = true
		end
		local function add(list)
			for _, f in ipairs(list) do
				if not seen[string.lower(f)] then
					seen[string.lower(f)] = true
					why[string.lower(f)] = "aura"
					table.insert(fams, f)
				end
			end
		end
		if weapon then
			add(B.WEAPON_AURAS)
		end
		if caster then
			add(B.CASTER_AURAS)
		end
	end
	return fams, why
end

-- The buffs the Buffs page lists: the set's families, then those left out of it that the set does
-- not hold, each `{ family, how, spell }`: how it comes to be listed ("chosen", "default",
-- "added", "aura", or "excluded" for one ExcludedBuffs names) and the spell it would be cast with
-- (nil when the character knows none it casts reliably, or the spell book has not been read).
function B.listed()
	local fams, why = candidates()
	local out, seen = {}, {}
	local excess = S.get("SpellDiffExcessThreshold-Buff")
	local function best(f)
		return spells.ready() and spells.best(f, S.get("MaxSpellLevel"), excess) or nil
	end
	for _, f in ipairs(fams) do
		seen[string.lower(f)] = true
		local excluded = S.listed("ExcludedBuffs", f)
		table.insert(out, { family = f, how = excluded and "excluded" or why[string.lower(f)], spell = best(f) })
	end
	for _, f in ipairs(list_setting("ExcludedBuffs")) do
		if not seen[string.lower(f)] then
			table.insert(out, { family = f, how = "excluded", spell = best(f) })
		end
	end
	return out
end

-- The self buff families wanted, best first: the BuffSpells setting when it names any, else the
-- defaults for this character; only those it knows a spell of.
function B.wanted()
	local fams = candidates()
	-- The skill-raising buffs first, in B.FIRST's order; the rest as listed.
	local rank = {}
	for i, f in ipairs(B.FIRST) do
		rank[string.lower(f)] = i
	end
	local order = {}
	for i, f in ipairs(fams) do
		table.insert(order, { f = f, r = rank[string.lower(f)] or (100 + i) })
	end
	table.sort(order, function(a, b)
		return a.r < b.r
	end)
	local out = {}
	local excess = S.get("SpellDiffExcessThreshold-Buff")
	for _, o in ipairs(order) do
		-- A buff the player excluded is never cast.
		local s = not S.listed("ExcludedBuffs", o.f) and spells.best(o.f, S.get("MaxSpellLevel"), excess) or nil
		if s ~= nil then
			table.insert(out, { family = o.f, spell = s })
		end
	end
	return out
end

-- The character's enchantments by category and by family name: the longest time left of each
-- (math.huge for one that does not run out). Read once per look.
-- A spell's level, by id (the spell table does not change).
local level_of = {}
local function spell_level(id)
	local l = level_of[id]
	if l == nil then
		local sp = magic.spell(id)
		l = sp and sp.level or 0
		level_of[id] = l
	end
	return l
end

local function effect_index(effects)
	local by_cat, by_fam, lvl = {}, {}, {}
	for _, e in ipairs(effects) do
		local r = e.remaining or math.huge
		if e.category ~= nil then
			by_cat[e.category] = math.max(by_cat[e.category] or 0, r)
			lvl[e.category] = math.max(lvl[e.category] or 0, spell_level(e.spell))
		end
		local fam = string.lower(spells.family(e.name or ""))
		by_fam[fam] = math.max(by_fam[fam] or 0, r)
	end
	return { cat = by_cat, fam = by_fam, lvl = lvl }
end

-- The level of the family's buff on the character now, by its category (0 when none, nil when
-- the host gives no categories).
local function level_on(family, index)
	local cat = spells.category_of(family)
	if cat == nil then
		return nil
	end
	return index.lvl[cat] or 0
end

-- Seconds left on a buff of `family` on the character, 0 when none: by its category where the
-- spell table gives one (a higher spell of the family counts), else by its name.
local function left(family, index)
	local cat = spells.category_of(family)
	if cat ~= nil and index.cat[cat] ~= nil then
		return index.cat[cat]
	end
	return index.fam[string.lower(family)] or 0
end

-- Buffs skipped for a refusal that casting again cannot mend (the spell not known, a component
-- missing, a target it cannot take), by family or item key: until when.
local skipped = {}

-- A force buff under way (VTank's Force Buff): every buff is cast again, whatever time it has
-- left, each once; `done` holds those cast (or given up on) so far, so a paused run resumes the
-- sequence where it left off.
B.force = nil

-- Start a force buff; answers the line to log.
function B.start_force()
	B.force = { done = {}, cast = 0 }
	B.step = nil
	return "force buff: every buff is cast again"
end

-- Cancel the force buff under way; answers the line to log.
function B.cancel_force()
	if B.force == nil then
		return "no force buff to cancel"
	end
	local n = B.force.cast
	B.force = nil
	B.step = nil
	return string.format("force buff cancelled after %d buffs", n)
end

-- Whether a force buff is under way.
function B.forcing()
	return B.force ~= nil
end

-- When each buff was last cast (by family, or item and family), on the clock.
B.cast_times = {}

-- The while after a buff in which the others are recast earlier (BuffCastRecast_Seconds for
-- BuffCastRecastReset_Seconds): when it began, at the first cast after a while with none.
B.window = nil

-- Whether that while runs now.
local function in_window()
	return B.window ~= nil and os.clock() - B.window < S.get("BuffCastRecastReset_Seconds")
end

-- Whether buff `key` was cast in that while: one the earlier recast does not apply to, as it has
-- just been cast.
local function recent(key)
	local t = B.cast_times[key]
	return t ~= nil and B.window ~= nil and t >= B.window
end

-- The next self buff that wants casting: missing, with less than `margin` seconds left (`base`
-- for one cast lately, when given), or on below the level the skill reaches now (the climb).
function B.next_self(margin, base)
	local index = effect_index(game.player.enchantments or {})
	for _, w in ipairs(B.wanted()) do
		local sk = skipped[w.family]
		if (sk == nil or os.clock() >= sk) and not (B.force and B.force.done[w.family]) then
			local m = (base ~= nil and recent(w.family)) and base or margin
			local l = left(w.family, index)
			local on = level_on(w.family, index)
			local lower = l >= m and on ~= nil and on > 0 and on < w.spell.level
			if l < m or lower then
				w.upgrade = lower and on or nil
				-- Done once it has more than the usual margin left (not the earlier one).
				w.margin = base or m
				return w
			end
		end
	end
	return nil
end

-- Item buffs: when each family was last cast on each item, kept with the character: the
-- character's age then (seconds played, which logged-out time does not add to) and how long the
-- buff lasts. (A plain number is a time of day, as earlier versions kept it.)
local function item_log()
	local log = store.character.tank_item_buffs or {}
	return log
end

-- The character's age, seconds played (the game's own count, which the server keeps the client
-- told of), or nil where the client does not know it.
local AGE = 125
local function age()
	return game.player_int(AGE)
end

-- Seconds an item buff logged as `entry` has run, and how long it lasts (nil: the setting's).
local function item_elapsed(entry)
	if type(entry) == "table" then
		local now = age()
		if entry.age ~= nil and now ~= nil then
			return now - entry.age, entry.lasts
		end
		return os.time() - (entry.time or os.time()), entry.lasts
	end
	return os.time() - entry, nil
end

-- The log's entry for an item buff cast now, lasting `lasts` seconds (nil: the setting's).
local function item_entry(lasts)
	local a = age()
	if a ~= nil then
		return { age = a, lasts = lasts }
	end
	return { time = os.time(), lasts = lasts }
end

-- The next item buff that wants casting: `{ family, spell, item }` (`base` as `next_self`
-- takes it).
function B.next_item(margin, base)
	if not S.get("BuffItems") then
		return nil
	end
	local log = item_log()
	local me = game.player.id
	local function check(item, fams)
		for _, f in ipairs(fams) do
			local s = not S.listed("ExcludedBuffs", f) and in_profile(f, "Banes")
				and spells.best(f, S.get("MaxSpellLevel"), S.get("SpellDiffExcessThreshold-Buff")) or nil
			local key = tostring(item.id) .. ":" .. f
			local entry = log[key]
			local sk = skipped[key]
			local m = (base ~= nil and recent(key)) and base or margin
			local due = entry == nil
			if not due then
				local ran, lasts = item_elapsed(entry)
				due = ran > (lasts or S.get("ItemBuffMinutes") * 60) - m
			end
			if s ~= nil and (sk == nil or os.clock() >= sk) and due and not (B.force and B.force.done[key]) then
				return { family = f, spell = s, item = item, key = key, margin = m }
			end
		end
		return nil
	end
	for _, it in ipairs(game.inventory.items()) do
		-- An item the player listed as one never to buff is left as it is.
		if it.wielder == me and not S.listed("NoBuffItems", it.name) then
			local fams = nil
			if it.class == "MeleeWeapon" or it.class == "MissileWeapon" then
				fams = B.WEAPON
			elseif it.class == "WandStaffOrb" then
				fams = B.CASTER
			elseif it.class == "Armor" then
				fams = B.ARMOR
			end
			local w = fams and check(it, fams)
			if w ~= nil then
				return w
			end
		end
	end
	return nil
end

-- The buff being cast: a step is done only once its effect shows on its target (the character's
-- enchantments for a self buff, the item's appraised spells for an item buff). A fizzle, a cast
-- the server ended with nothing to show, or any other failure casts the same spell again, as soon
-- as the character can cast: a cast asked for while the last one is still finishing waits for the
-- server's end of it (its use-done, sent once the character is back in its ready stance), and
-- casting earlier would only be refused as too busy. Too little mana waits for the mana cycle and
-- casts it again. Only a refusal casting again
-- cannot mend skips it, said once; and a spell that fails TRIES times in a row pauses buffing a
-- while, said, and is then cast again.
B.step = nil
B.TRIES = 15
-- Seconds: after a failed cast, before the next is asked for (none: the cast waits its turn);
-- after a cast that ended well, for its effect to show; and buffing's pause when a spell has
-- failed TRIES times.
B.RETRY_PAUSE = 0
B.SHOW_WAIT = 4
B.CAP_PAUSE = 60

-- Where the buffing's lines go (the engine sets its log).
B.note = function(fmt, ...)
	print(string.format(fmt, ...))
end

-- Refusals casting again cannot mend: the spell not known (0x03FE), a target it cannot take
-- (0x03FF), a component missing (0x0400), the target gone (0x0403), a spell that cannot be cast
-- outside or inside (0x0407, 0x0408).
local HOPELESS = { [0x03FE] = "not known", [0x03FF] = "not on that target", [0x0400] = "a component missing",
	[0x0403] = "the target is missing", [0x0407] = "not cast outside", [0x0408] = "not cast inside" }

local function code_of(why)
	local hex = string.match(tostring(why), "0x(%x+)")
	return hex and tonumber(hex, 16) or nil
end

-- The level of a spell an appraisal lists as an enchantment on an item, by category.
local MASK = 0x80000000
local function item_levels(item)
	local a = item.appraisal and item:appraisal() or nil
	local out = {}
	for _, id in ipairs(a and a.spells or {}) do
		if id >= MASK then
			local sp = magic.spell(id - MASK)
			if sp ~= nil and sp.category ~= nil then
				out[sp.category] = math.max(out[sp.category] or 0, sp.level or 0)
			end
		end
	end
	return out
end

-- Whether the step's effect shows on its target, at its level or above.
local function confirmed(st)
	local cat = spells.category_of(st.family)
	if st.item ~= nil then
		-- Only an appraisal after a cast that ended well tells: one from before shows what was.
		if not st.ended_well then
			return false
		end
		local lv = cat and item_levels(st.item)[cat]
		return lv ~= nil and lv >= (st.spell.level or 0)
	end
	local index = effect_index(game.player.enchantments or {})
	local l = left(st.family, index)
	if st.margin == math.huge then
		-- Cast again by a force buff: done once the cast ended well and the buff's time has not
		-- fallen from what it had at the cast (a cast that took renews it).
		if not st.ended_well or l < (st.before or 0) then
			return false
		end
	elseif l < (st.margin or 0) then
		return false
	end
	local on = level_on(st.family, index)
	return on == nil or on >= (st.spell.level or 0)
end

local function label(st)
	return st.spell.name .. (st.item and (" on " .. st.item.name) or "")
end

-- The step in hand, carried on: done (its effect shows), waiting, or to be cast again. Answers
-- the step to cast, false while it waits, nil when there is none.
local function carry_on()
	local st = B.step
	if st == nil then
		return nil
	end
	if st.cast_at ~= nil and confirmed(st) then
		if st.item ~= nil then
			local log = item_log()
			log[st.key] = item_entry(spells.duration(st.spell.id))
			store.character.tank_item_buffs = log
		end
		if B.force ~= nil then
			B.force.done[st.key] = true
			B.force.cast = B.force.cast + 1
		end
		if st.item ~= nil then
			B.note("buff: %s confirmed in its appraisal%s", label(st),
				st.tries > 1 and string.format(" after %d casts", st.tries) or "")
		elseif st.tries > 1 then
			B.note("buff: %s confirmed after %d casts", label(st), st.tries)
		end
		B.step = nil
		return nil
	end
	if os.clock() < (st.wait_until or 0) then
		return false
	end
	if st.cast_at ~= nil and st.ended_well then
		-- Cast, ended well, and still nothing on the target: cast again.
		B.note("buff: %s: no effect showed; casting it again", label(st))
		st.ended_well = false
	end
	-- The level again, as the skill may have changed: lower when this one is no longer cast
	-- reliably, the family kept.
	local s = spells.best(st.family, S.get("MaxSpellLevel"), S.get("SpellDiffExcessThreshold-Buff"))
	if s == nil then
		B.note("buff: %s skipped: no level of it is cast reliably now", st.family)
		skipped[st.key] = os.clock() + 300
		if B.force ~= nil then
			B.force.done[st.key] = true
		end
		B.step = nil
		return nil
	end
	if s.id ~= st.spell.id then
		B.note("buff: %s now at level %d (was %d)", st.family, s.level or 0, st.spell.level or 0)
		st.spell = s
	end
	return st
end

-- Whether anything wants casting now (`idle`: nothing to fight, so the earlier top-off applies).
-- A new step is looked for at most every two seconds.
local last_look, last_answer = nil, nil

-- Until when buffing waits: for mana after a cast the character had too little for, or after a
-- spell failed TRIES times.
local mana_wait = 0

function B.needed(idle)
	if not S.get("EnableBuffing") or os.clock() < mana_wait then
		return nil
	end
	local st = carry_on()
	if st == false then
		return nil
	elseif st ~= nil then
		return st
	end
	if last_look ~= nil and os.clock() - last_look < 2 and B.force == nil then
		return last_answer
	end
	local margin = idle and S.get("IdleBuffTopoffTimeSeconds") or S.get("RebuffTimeRemainingSeconds")
	local base = nil
	-- For a while after a buff, the others are recast that much earlier (VTank's
	-- BuffCastRecast_Seconds for BuffCastRecastReset_Seconds); those cast in that while keep the
	-- usual margin.
	if in_window() then
		base = margin
		margin = margin + S.get("BuffCastRecast_Seconds")
	end
	-- A force buff casts every buff again, whatever it has left.
	if B.force ~= nil then
		margin, base = math.huge, nil
	end
	last_look = os.clock()
	local w = B.next_self(margin, base) or B.next_item(margin, base)
	if w ~= nil then
		w.key = w.key or w.family
		w.tries = 0
		B.step = w
	elseif B.force ~= nil then
		B.note("force buff: done, %d buffs cast", B.force.cast)
		B.force = nil
	end
	last_answer = w
	return w
end

-- Buff items -------------------------------------------------------------------------------------

-- VTank's Consumables include "items the macro uses to cast buffs on you such as beers and
-- foodstuffs", and buff items whose buff lasts under five minutes are not allowed [std]. A buff
-- item is one the Consumables name whose spell lasts five minutes or more; it is used when its
-- buff is missing or has less than the rebuff margin left, for a buff the character does not cast
-- itself (which buff VTank prefers when both serve is not published; this is the plugin's
-- reading).
local MIN_LASTS = 300
local SPELL_ID = 0x0D000008

-- Each item's buff spells, by id, from its appraisal and its own spell.
local item_spells = {}
-- When each buff item was last used, by id: it is not used again before its buff can show.
local used_at = {}

local function buff_spells_of(it)
	local known = item_spells[it.id]
	if known ~= nil then
		return known
	end
	if not it.appraised then
		pawait(act.appraise(it.id))
	end
	local ids, out = {}, {}
	for _, id in ipairs(it.spells or {}) do
		table.insert(ids, id)
	end
	local own = it:int(SPELL_ID)
	if own ~= nil and own > 0 then
		table.insert(ids, own)
	end
	for _, id in ipairs(ids) do
		local sp = magic.spell(id)
		if sp ~= nil and sp.duration ~= nil and sp.duration >= MIN_LASTS then
			table.insert(out, sp)
		end
	end
	item_spells[it.id] = out
	return out
end

-- The buff item to use now, and the buff it gives, or nil (looked for every two seconds).
local consumable_look = nil
function B.next_consumable(margin)
	if #S.list("Consumables") == 0 or (consumable_look ~= nil and os.clock() - consumable_look < 2) then
		return nil
	end
	consumable_look = os.clock()
	local cast = {}
	for _, w in ipairs(B.wanted()) do
		local cat = spells.category_of(w.family)
		if cat ~= nil then
			cast[cat] = true
		end
	end
	local index = effect_index(game.player.enchantments or {})
	for _, it in ipairs(game.inventory.items()) do
		local worn = it.wielder ~= nil and it.wielder ~= 0
		if not worn and S.listed("Consumables", it.name) and (used_at[it.id] == nil or os.clock() - used_at[it.id] > 2 * B.SHOW_WAIT) then
			for _, sp in ipairs(buff_spells_of(it)) do
				if sp.category ~= nil and not cast[sp.category] and (index.cat[sp.category] or 0) < margin then
					return it, sp
				end
			end
		end
	end
	return nil
end

-- A buff item was used (or tried).
function B.used_consumable(item)
	used_at[item.id] = os.clock()
	consumable_look = nil
end

-- Cast the step's spell. Answers whether the cast ended well, and the line to log.
function B.cast(w)
	last_look = nil
	local target = w.item and w.item.id or nil
	if w.item == nil then
		w.before = left(w.family, effect_index(game.player.enchantments or {}))
	end
	local ok, why = pawait(magic.cast(w.spell.id, target))
	local skill = spells.skill(w.spell.school)
	if ok then
		if not in_window() then
			B.window = os.clock()
		end
		B.cast_times[w.key or w.family] = os.clock()
		w.tries = (w.tries or 0) + 1
		w.cast_at = os.clock()
		w.ended_well = true
		w.wait_until = os.clock() + B.SHOW_WAIT
		if w.item ~= nil then
			-- The item's appraisal shows its enchantments.
			act.appraise(w.item.id)
		end
		return true, string.format("%s (level %d%s%s)", label(w), w.spell.level or 0,
			w.upgrade and (", up from " .. w.upgrade) or "",
			skill and (string.format(", %s %d", spells.SCHOOL_SKILL[w.spell.school] or "skill", skill)) or "")
	end
	local code = code_of(why)
	-- Too little mana: buffing waits for the mana cycle, then casts it again.
	if code == 0x0401 then
		mana_wait = os.clock() + 10
		return false, string.format("%s: too little mana", label(w))
	end
	if code ~= nil and HOPELESS[code] then
		skipped[w.key] = os.clock() + 600
		if B.force ~= nil then
			B.force.done[w.key] = true
		end
		B.step = nil
		return false, string.format("%s skipped: %s (%s)", label(w), HOPELESS[code], tostring(why))
	end
	-- The client's own checks stopped it and nothing was sent (a target the spell cannot take, a
	-- component missing): asking again at once would be stopped again. Left two minutes, said.
	if string.find(tostring(why), "refused by the client", 1, true) then
		skipped[w.key] = os.clock() + 120
		if B.force ~= nil then
			B.force.done[w.key] = true
		end
		B.step = nil
		return false, string.format("%s skipped for now: the client will not cast it (%s)", label(w), tostring(why))
	end
	-- A fizzle, an interrupted cast, a resist: the same spell again, as soon as it can be cast.
	w.tries = (w.tries or 0) + 1
	w.cast_at = os.clock()
	w.ended_well = false
	w.wait_until = os.clock() + B.RETRY_PAUSE
	if w.tries >= B.TRIES then
		w.tries = 0
		mana_wait = os.clock() + B.CAP_PAUSE
		return false, string.format("%s: failed %d times in a row (%s); buffing paused %d s, then it is cast again",
			label(w), B.TRIES, tostring(why), B.CAP_PAUSE)
	end
	return false, string.format("%s: %s; casting it again (try %d)", label(w), code == 0x0402 and "fizzled" or tostring(why), w.tries + 1)
end

return B
