-- The character's own buffs, in the manner of Alinco's buffing plugin. A self profile names its
-- buffs by family, as the profiles for others do: buffs cast on the character, and item spells
-- cast on what it wields (Blood Drinker, Heart Seeker and Swift Killer on its weapon, Spirit
-- Drinker and Hermetic Link on its caster, Defender on either; the weapon it put away to cast
-- counts) and on each piece of armor it wears (Impenetrability and the banes). Each is cast at
-- the best level the character knows and casts reliably (as for others: MaxLevel, SkillMargin),
-- in the profile's order, when it is missing, has less than SelfRebuffMinutes left, or is on at a
-- lower level than the one it would be cast at. A buff on the character is read from its
-- enchantments; one on an item from when it was cast there, kept with the character by the
-- character's age (time logged out does not count).
--
-- /buffbot self [profile] casts them now; with SelfBuff on, the SelfProfile's are kept up between
-- players, whether or not the buffbot answers requests.

local S = {}
local B = nil

-- The self profiles a new buffbot has.
S.PROFILES = {
	mage = { "Creature Enchantment Mastery Self", "Focus Self", "Willpower Self", "Life Magic Mastery Self",
		"Item Enchantment Mastery Self", "Mana Conversion Mastery Self", "Arcane Enlightenment Self",
		"Mana Renewal Self", "Endurance Self", "Magic Resistance Self", "Armor Self",
		"Spirit Drinker", "Hermetic Link", "Defender", "Impenetrability" },
	melee = { "Creature Enchantment Mastery Self", "Item Enchantment Mastery Self", "Strength Self",
		"Endurance Self", "Coordination Self", "Quickness Self", "Invulnerability Self",
		"Heavy Weapon Mastery Self", "Armor Self", "Regeneration Self",
		"Blood Drinker", "Heart Seeker", "Swift Killer", "Defender", "Impenetrability" },
	missile = { "Creature Enchantment Mastery Self", "Item Enchantment Mastery Self", "Coordination Self",
		"Quickness Self", "Endurance Self", "Invulnerability Self", "Missile Weapon Mastery Self",
		"Armor Self", "Regeneration Self",
		"Blood Drinker", "Heart Seeker", "Swift Killer", "Defender", "Impenetrability" },
}

-- The item spells, by family (lower case): what each is cast on.
S.ITEM = {
	["blood drinker"] = "weapon", ["heart seeker"] = "weapon", ["swift killer"] = "weapon",
	["spirit drinker"] = "caster", ["hermetic link"] = "caster", ["defender"] = "held",
	["impenetrability"] = "armor", ["blade bane"] = "armor", ["piercing bane"] = "armor",
	["bludgeon bane"] = "armor", ["acid bane"] = "armor", ["flame bane"] = "armor",
	["frost bane"] = "armor", ["lightning bane"] = "armor",
}

-- What an item spell is cast on, said.
local ON = { weapon = "a weapon", caster = "a caster", held = "a weapon or caster", armor = "armor" }

-- The character's age (seconds played), where the client knows it.
local AGE = 125
-- A buff cast well is not cast again this many seconds, while its effect comes.
local SHOW_WAIT = 5
-- A buff casting cannot mend is left this many seconds.
local SKIP = 600

S.profiles = S.PROFILES

-- The buffs asked for now: { profile, cast, failed } (nil when none were).
S.asked = nil
-- The buff being cast, for the window.
S.casting = nil
-- The tries of each buff cast lately, by its key; until when each buff casting cannot mend is
-- left; when each was last cast well.
local tries, skipped, cast_at = {}, {}, {}
-- When the kept-up buffs are looked at next.
local next_look = 0

function S.init(bot)
	B = bot
end

function S.load()
	S.profiles = {}
	for name, families in pairs(store.character.buffbot_self_profiles or S.PROFILES) do
		S.profiles[name] = families
	end
end

-- The self profiles' names, sorted.
function S.names()
	local out = {}
	for name in pairs(S.profiles) do
		table.insert(out, name)
	end
	table.sort(out)
	return out
end

-- Every family the self profiles name.
function S.families()
	local out = {}
	for _, fams in pairs(S.profiles) do
		for _, f in ipairs(fams) do
			table.insert(out, f)
		end
	end
	return out
end

-- A self profile set (its families, comma separated) or, with no families, removed.
function S.set_profile(name, families)
	local why
	name, why = B.profile_name(name)
	if name == nil then
		return why
	end
	local list = B.families_of(families)
	B.forget_book()
	if #list == 0 then
		if S.profiles[name] == nil then
			return "no self profile " .. name
		end
		S.profiles[name] = nil
		store.character.buffbot_self_profiles = S.profiles
		return "removed the self profile " .. name
	end
	S.profiles[name] = list
	store.character.buffbot_self_profiles = S.profiles
	return string.format("self profile %s: %s", name, table.concat(list, ", "))
end

---------------------------------------------------------------------------------------------------
-- Time left.

-- A spell's category and level, by id (the spell table does not change).
local spell_info = {}
local function info(id)
	local i = spell_info[id]
	if i == nil then
		local sp = magic.spell(id)
		i = { category = sp and sp.category, level = sp and sp.level or 0, duration = sp and sp.duration }
		spell_info[id] = i
	end
	return i
end

-- Seconds left on the character's buff of spell `s` (family `family`): the longest of its
-- enchantments in the spell's category (another level of the family, or its form cast by another,
-- counts), else of the family by name; 0 when none, math.huge for one that does not run out. And
-- the highest level of them on now.
local function self_left(s, family, effects)
	local cat = s.category or info(s.id).category
	local left, level = 0, 0
	for _, e in ipairs(effects) do
		local same
		if cat ~= nil then
			same = e.category == cat
		else
			same = string.lower((B.family(e.name or ""))) == string.lower(family)
		end
		if same then
			left = math.max(left, e.remaining or math.huge)
			level = math.max(level, info(e.spell).level or 0)
		end
	end
	return left, level
end

local function age()
	return game.player_int(AGE)
end

-- The item buffs cast, by key ("<item id>:<family>"): { age, time, lasts } each.
local function item_log()
	return store.character.buffbot_item_buffs or {}
end

-- Seconds left on an item buff, by when it was cast there: 0 when it was not.
local function item_left(key)
	local e = item_log()[key]
	if e == nil then
		return 0
	end
	local now = age()
	local ran
	if e.age ~= nil and now ~= nil then
		ran = now - e.age
	else
		ran = os.time() - (e.time or os.time())
	end
	return math.max(0, (e.lasts or 0) - ran)
end

-- An item buff cast now, kept with the character; those run out are let go.
local function item_cast(key, s)
	local log = {}
	for k, e in pairs(item_log()) do
		if item_left(k) > 0 then
			log[k] = e
		end
	end
	log[key] = { age = age(), time = os.time(), lasts = info(s.id).duration or 0 }
	store.character.buffbot_item_buffs = log
end

---------------------------------------------------------------------------------------------------
-- The plan.

local function weapon_class(c)
	return c == "MeleeWeapon" or c == "MissileWeapon"
end

-- The items a kind of item spell goes on: those the character wields, and the weapon or caster it
-- wielded last while it holds none (still carried): a fighter takes up its caster to cast, and
-- its weapon is buffed in its pack. The last ones are kept with the character.
local function items_for(kind)
	local me = game.player.id
	local out, carried, held = {}, {}, {}
	for _, it in ipairs(game.inventory.items()) do
		carried[it.id] = it
		if it.wielder == me then
			local weapon, caster = weapon_class(it.class), it.class == "WandStaffOrb"
			if weapon then
				held.weapon = it.id
			elseif caster then
				held.caster = it.id
			end
			if (kind == "weapon" and weapon) or (kind == "caster" and caster)
				or (kind == "held" and (weapon or caster)) or (kind == "armor" and it.class == "Armor") then
				table.insert(out, it)
			end
		end
	end
	local last = store.character.buffbot_self_held or {}
	if (held.weapon and last.weapon ~= held.weapon) or (held.caster and last.caster ~= held.caster) then
		last.weapon = held.weapon or last.weapon
		last.caster = held.caster or last.caster
		store.character.buffbot_self_held = last
	end
	local function add_last(which)
		local it = held[which] == nil and last[which] and carried[last[which]] or nil
		if it ~= nil then
			table.insert(out, it)
		end
	end
	if kind == "weapon" or kind == "held" then
		add_last("weapon")
	end
	if kind == "caster" or kind == "held" then
		add_last("caster")
	end
	return out
end

-- A profile's buffs now, in its order: { family, spell, item (nil for one on the character), key,
-- left (seconds), level_on } each; and the families it cannot cast, each with why.
function S.plan(profile)
	local steps, missing = {}, {}
	local effects = game.player.enchantments or {}
	for _, fam in ipairs(S.profiles[profile] or {}) do
		local s = B.best(fam)
		local kind = S.ITEM[string.lower(fam)]
		if s == nil then
			table.insert(missing, fam .. " (not known)")
		elseif kind == nil then
			local left, level = self_left(s, fam, effects)
			table.insert(steps, { family = fam, spell = s, key = string.lower(fam), left = left, level_on = level })
		else
			local items = items_for(kind)
			if #items == 0 then
				table.insert(missing, string.format("%s (no %s wielded)", fam, ON[kind]))
			end
			for _, it in ipairs(items) do
				local key = tostring(it.id) .. ":" .. string.lower(fam)
				table.insert(steps, { family = fam, spell = s, item = it, key = key, left = item_left(key) })
			end
		end
	end
	return steps, missing
end

-- Whether a step wants casting: missing, running out, or on below the level it is cast at.
local function due(st)
	if os.clock() < (skipped[st.key] or 0) or os.clock() - (cast_at[st.key] or -100) < SHOW_WAIT then
		return false
	end
	if st.left < B.get("SelfRebuffMinutes") * 60 then
		return true
	end
	return st.item == nil and st.level_on > 0 and st.level_on < (st.spell.level or 0)
end

-- The time left, said: "12:05", "none", "lasting".
function S.left_text(st)
	if st.left == math.huge then
		return "lasting"
	elseif st.left <= 0 then
		return "none"
	end
	local secs = math.floor(st.left)
	return string.format("%d:%02d", secs // 60, secs % 60)
end

---------------------------------------------------------------------------------------------------
-- Casting.

-- The self profile chosen (SelfProfile, lower case; "" for none).
function S.chosen()
	return string.lower(B.get("SelfProfile") or "")
end

-- Whether the character's own buffs want the bot's time: asked for, or kept up.
function S.wanted()
	return S.asked ~= nil or (B.get("SelfBuff") and S.chosen() ~= "")
end

-- Buff the character now by `profile`, which becomes the one chosen (the one chosen when nil);
-- answers the line to tell.
function S.ask(profile)
	local name = profile and string.lower(profile) or S.chosen()
	if name == "" then
		return "no self profile chosen: /buffbot self <profile>, or set SelfProfile"
	end
	if S.profiles[name] == nil then
		return "no self profile " .. name .. " (" .. table.concat(S.names(), ", ") .. ")"
	end
	if name ~= S.chosen() then
		B.set("SelfProfile", name)
	end
	S.asked = { profile = name, cast = 0, failed = {} }
	next_look = 0
	return "buffing yourself: " .. name
end

function S.stop()
	if S.asked == nil then
		return "not buffing yourself"
	end
	S.asked = nil
	S.casting = nil
	return "stopped buffing yourself"
end

local function label(st)
	return st.spell.name .. (st.item and (" on " .. tostring(st.item.name)) or "")
end

-- Cast one step: answers whether it landed, and why not.
local function cast(st)
	local need = math.min(1, ((st.spell.mana or 10) + 5) / math.max(1, game.player.mana and game.player.mana.max or 1))
	if not B.mana_step(need, "buff") then
		return nil, "getting mana back"
	end
	if not B.ready("buff") then
		return false, "no magic stance"
	end
	S.casting = st
	B.set_state("Buffing yourself: " .. label(st))
	local ok, why = pawait(magic.cast(st.spell.id, st.item and st.item.id or nil))
	B.casts = B.casts + 1
	S.casting = nil
	if ok then
		return true
	end
	return false, why
end

-- One turn of the character's own buffs: the next one due cast (or tried, or mana got back for
-- it). Answers whether it spent the turn.
function S.step()
	local a = S.asked
	if a == nil then
		if not (B.get("SelfBuff") and S.chosen() ~= "") or os.clock() < next_look then
			return false
		end
		next_look = os.clock() + 5
		if S.profiles[S.chosen()] == nil then
			return false
		end
	end
	if not B.book_ready() then
		return false
	end
	local profile = a and a.profile or S.chosen()
	local st = nil
	for _, s in ipairs(S.plan(profile)) do
		if due(s) and not (a and a.failed[s.key]) then
			st = s
			break
		end
	end
	if st == nil then
		if a ~= nil then
			local failed = {}
			for _, f in pairs(a.failed) do
				table.insert(failed, f)
			end
			table.sort(failed)
			B.note("buffed yourself: %s, %d cast%s", profile, a.cast,
				#failed > 0 and ("; not cast: " .. table.concat(failed, ", ")) or "")
			pcall(chat.print, string.format("[Buffbot] Your buffs (%s): %d cast%s.", profile, a.cast,
				#failed > 0 and ("; not cast: " .. table.concat(failed, ", ")) or ""))
			S.asked = nil
		end
		return false
	end
	local ok, why = cast(st)
	if ok == nil then
		return true
	end
	if ok then
		tries[st.key] = nil
		cast_at[st.key] = os.clock()
		if st.item ~= nil then
			item_cast(st.key, st.spell)
		end
		if a ~= nil then
			a.cast = a.cast + 1
		end
		B.note("cast %s on %s", st.spell.name, st.item and tostring(st.item.name) or "yourself")
		return true
	end
	local code = B.code_of(why)
	local give_up = nil
	if string.find(tostring(why), "refused by the client", 1, true) then
		-- The client's own checks stopped it (a component missing, a target it cannot take):
		-- asking again would be stopped again.
		give_up = tostring(why)
		B.check_components()
	elseif code == B.FIZZLED or code == nil or code == B.TOO_LITTLE_MANA then
		if code == B.FIZZLED then
			B.fizzles = B.fizzles + 1
		end
		tries[st.key] = (tries[st.key] or 0) + 1
		B.note("%s: %s (try %d of %d)", label(st), code == B.FIZZLED and "fizzled" or tostring(why), tries[st.key], B.get("Retries"))
		if tries[st.key] >= B.get("Retries") then
			give_up = code == B.FIZZLED and ("fizzled " .. tries[st.key] .. " times") or tostring(why)
		end
	elseif code == B.NO_COMPONENT or B.HOPELESS[code] ~= nil then
		give_up = B.HOPELESS[code] or "a component missing"
		if code == B.NO_COMPONENT then
			B.check_components()
		end
	else
		tries[st.key] = (tries[st.key] or 0) + 1
		B.note("%s: %s (try %d of %d)", label(st), tostring(why), tries[st.key], B.get("Retries"))
		if tries[st.key] >= B.get("Retries") then
			give_up = tostring(why)
		end
	end
	if give_up ~= nil then
		tries[st.key] = nil
		skipped[st.key] = os.clock() + SKIP
		B.failures = B.failures + 1
		B.note("not cast: %s: %s", label(st), give_up)
		if a ~= nil then
			a.failed[st.key] = label(st)
		end
	end
	return true
end

return S
