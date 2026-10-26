-- The buffbot's work: the line of players waiting, each one's buffs cast in turn, and the
-- character's own mana and components looked after between them.
--
-- A player tells the character a profile's name ("buffs", "melee"): the request joins the line,
-- and the player is told its place. One at a time, from the front: a player further than Range is
-- asked to come closer and waited for ComeCloserSeconds, then skipped; otherwise each spell of the
-- profile is cast on them, at the highest level the character knows and casts reliably (its skill
-- beats the spell's difficulty by SkillMargin, at most MaxLevel). A fizzle casts the spell again,
-- Retries times at most; too little mana gets mana back first. When the profile is done the player
-- is told "Done". Between players, with mana under ManaRest, the character gets it back (Stamina
-- to Mana, a mana potion, else resting in peace) up to ManaReady. Each component it carries that
-- it is using up and has fewer than ComponentWarn of is warned of, once. What the Restock setting
-- lists it buys again from the vendors it names when it runs low (restock.lua). Its own buffs it
-- casts by its self profiles, now or kept up between players (selfbuffs.lua).

local B = {}
local R = require("restock")
local Self = require("selfbuffs")

-- The settings: name -> default.
B.DEFAULTS = {
	On = false,
	Range = 20,
	ComeCloserSeconds = 60,
	QueueMax = 10,
	PerPlayerCap = 4,
	Retries = 3,
	MaxLevel = 7,
	SkillMargin = 10,
	ManaRest = 30,
	ManaReady = 90,
	ComponentWarn = 10,
	Caster = "",
	Restock = "",
	RestockAt = 25,
	SelfProfile = "",
	SelfBuff = false,
	SelfRebuffMinutes = 5,
}

-- The profiles a new buffbot has: name -> spell families (a family is a spell's name without
-- its level's numeral).
B.PROFILES = {
	buffs = { "Strength Other", "Endurance Other", "Coordination Other", "Quickness Other",
		"Invulnerability Other", "Heavy Weapon Mastery Other", "Armor Other", "Regeneration Other" },
	melee = { "Strength Other", "Endurance Other", "Coordination Other", "Quickness Other",
		"Invulnerability Other", "Heavy Weapon Mastery Other", "Light Weapon Mastery Other",
		"Finesse Weapon Mastery Other", "Armor Other" },
	missile = { "Coordination Other", "Quickness Other", "Endurance Other", "Invulnerability Other",
		"Impregnability Other", "Missile Weapon Mastery Other", "Armor Other" },
	mage = { "Focus Other", "Willpower Other", "Endurance Other", "Mana Renewal Other",
		"War Magic Mastery Other", "Life Magic Mastery Other", "Creature Enchantment Mastery Other",
		"Magic Resistance Other", "Armor Other" },
}

-- The failures a cast ends with, by the code the game gives.
local TOO_LITTLE_MANA, FIZZLED, NO_COMPONENT, OUT_OF_RANGE = 0x0401, 0x0402, 0x0400, 0x0550
B.TOO_LITTLE_MANA, B.FIZZLED, B.NO_COMPONENT = TOO_LITTLE_MANA, FIZZLED, NO_COMPONENT
-- Failures casting again cannot mend: the spell not known, not on that target, the target gone.
local HOPELESS = { [0x03FE] = "not known", [0x03FF] = "not on that target", [0x0403] = "the target is missing" }
B.HOPELESS = HOPELESS
-- The magic schools' skills, by school number.
local SCHOOL_SKILL = { [1] = "War Magic", [2] = "Life Magic", [3] = "Item Enchantment",
	[4] = "Creature Enchantment", [5] = "Void Magic" }
local ROMAN = { I = 1, II = 2, III = 3, IV = 4, V = 5, VI = 6, VII = 7, VIII = 8 }
-- The stance spells are cast in, and the spell that gives mana for stamina.
local MAGIC = 8
local CONVERSION = "Stamina to Mana Self"
-- How long a request counts against its player's cap, seconds.
local CAP_WINDOW = 3600
-- Tells are sent one at a time, this far apart (seconds): the game takes a few lines a second.
local TELL_GAP = 1.1

B.settings = {}
for k, v in pairs(B.DEFAULTS) do
	B.settings[k] = v
end
B.profiles = B.PROFILES
-- The line: { name, id, profile, at } each; the one being buffed; each player's requests (lower
-- name -> times asked).
B.queue = {}
B.current = nil
B.asked = {}
-- What the window shows and the log keeps.
B.state = "Off."
B.log = {}
B.served, B.casts, B.fizzles, B.failures = 0, 0, 0, 0

local fight = require("dereth.combat")
local outbox = {}
local last_tell = -100

-- A line for the log: the plugin's console (the daemon's log file keeps it) and the window's.
function B.note(fmt, ...)
	local line = string.format(fmt, ...)
	print(line)
	table.insert(B.log, string.format("%6.1f  %s", os.clock(), line))
	if #B.log > 200 then
		table.remove(B.log, 1)
	end
	if B.on_change then
		B.on_change()
	end
end

local function set_state(s)
	if s ~= B.state then
		B.state = s
		if B.on_change then
			B.on_change()
		end
	end
end
B.set_state = set_state

---------------------------------------------------------------------------------------------------
-- Settings and profiles, kept with the character.

function B.load()
	local saved = store.character.buffbot_settings or {}
	for k, v in pairs(B.DEFAULTS) do
		B.settings[k] = v
		if type(saved[k]) == type(v) then
			B.settings[k] = saved[k]
		end
	end
	local profiles = store.character.buffbot_profiles
	B.profiles = {}
	for name, families in pairs(profiles or B.PROFILES) do
		B.profiles[name] = families
	end
	Self.load()
end

local function save()
	store.character.buffbot_settings = B.settings
end

-- The setting's own spelling, found without case, or nil.
function B.setting_name(name)
	local want = string.lower(tostring(name or ""))
	for k in pairs(B.DEFAULTS) do
		if string.lower(k) == want then
			return k
		end
	end
	return nil
end

-- Set a setting from a value or the text typed for it; answers the line to tell.
function B.set(name, value)
	local n = B.setting_name(name)
	if n == nil then
		return "no setting " .. tostring(name)
	end
	local kind = type(B.DEFAULTS[n])
	if type(value) == "string" and kind == "number" then
		value = tonumber(value)
	elseif type(value) == "string" and kind == "boolean" then
		local v = string.lower(value)
		value = (v == "true" or v == "on" or v == "yes" or v == "1") and true
			or ((v == "false" or v == "off" or v == "no" or v == "0") and false or nil)
	end
	if type(value) ~= kind then
		return n .. " takes a " .. kind
	end
	B.settings[n] = value
	save()
	if B.on_change then
		B.on_change()
	end
	return string.format("%s = %s", n, tostring(value))
end

function B.get(name)
	return B.settings[B.setting_name(name) or name]
end

-- A profile's name as kept (lower case), or nil and why it cannot be one.
function B.profile_name(name)
	name = string.lower(string.match(name or "", "^%s*(.-)%s*$"))
	if name == "" or string.find(name, "%s") then
		return nil, "a profile's name is one word"
	end
	return name
end

-- The families a profile's text names, comma separated.
function B.families_of(text)
	local list = {}
	for part in string.gmatch(text or "", "[^,]+") do
		local f = string.match(part, "^%s*(.-)%s*$")
		if f ~= "" then
			table.insert(list, f)
		end
	end
	return list
end

-- A profile set (its families, comma separated) or, with no families, removed.
function B.set_profile(name, families)
	local why
	name, why = B.profile_name(name)
	if name == nil then
		return why
	end
	if name == "help" or name == "list" or name == "cancel" then
		return "a profile cannot be called " .. name
	end
	local list = B.families_of(families)
	-- The book is read again for the families named now.
	book = nil
	if #list == 0 then
		if B.profiles[name] == nil then
			return "no profile " .. name
		end
		B.profiles[name] = nil
		store.character.buffbot_profiles = B.profiles
		return "removed the profile " .. name
	end
	B.profiles[name] = list
	store.character.buffbot_profiles = B.profiles
	return string.format("profile %s: %s", name, table.concat(list, ", "))
end

-- The profiles' names, sorted.
function B.profile_names()
	local out = {}
	for name in pairs(B.profiles) do
		table.insert(out, name)
	end
	table.sort(out)
	return out
end

---------------------------------------------------------------------------------------------------
-- The spell book, by family.

-- A spell name's family and level ("Strength Other III": "Strength Other", 3; "Incantation of
-- Strength Other": "Strength Other", 8).
function B.family(name)
	local base, numeral = string.match(name or "", "^(.-)%s+([IVX]+)$")
	if base ~= nil and ROMAN[numeral] ~= nil then
		return base, ROMAN[numeral]
	end
	local inc = string.match(name or "", "^Incantation of (.+)$")
	if inc ~= nil then
		return inc, 8
	end
	return name, nil
end

-- Known spells by lower-cased family: { id, name, level, school, difficulty, mana, category }
-- each.
local book = nil

-- A spell description with its numbers taken out: the levels of one family say the same thing
-- by different amounts ("Increases the caster's Strength by # points.").
local function shape(text)
	return (string.gsub(string.lower(text or ""), "%d+", "#"))
end

-- The families the profiles name, for others and for the character itself (lower case).
local function named_families()
	local out = {}
	for _, fams in pairs(B.profiles) do
		for _, f in ipairs(fams) do
			out[string.lower(f)] = true
		end
	end
	for _, f in ipairs(B.self and B.self.families() or {}) do
		out[string.lower(f)] = true
	end
	return out
end

-- Reads the spell book, a page a frame, answering how many spells it holds. A family's levels
-- above VI that carry names of their own ("Might of the Lugians" is Strength Self's level 7) are
-- found for the families the profiles name: of the spells known in the category of the family's
-- highest numbered level and above it, the one whose description reads as that level's does (a
-- category also holds a family's other forms, cast on another or on the caster).
function B.read_book()
	local fresh, n = {}, 0
	-- The spells with a category, by category.
	local by_cat = {}
	while true do
		local page = magic.known(n + 1, 100) or {}
		for _, s in ipairs(page) do
			local fam, level = B.family(s.name)
			local key = string.lower(fam)
			local sp = { id = s.id, name = s.name, level = level or s.level or 0, school = s.school,
				difficulty = s.difficulty, mana = s.mana, category = s.category }
			fresh[key] = fresh[key] or {}
			table.insert(fresh[key], sp)
			if s.category ~= nil then
				by_cat[s.category] = by_cat[s.category] or {}
				table.insert(by_cat[s.category], sp)
			end
		end
		n = n + #page
		if #page < 100 then
			break
		end
		task.sleep(0)
	end
	local looked, shapes = 0, {}
	local function shape_of(id)
		if shapes[id] == nil then
			looked = looked + 1
			if looked % 50 == 0 then
				task.sleep(0)
			end
			local sp = magic.spell(id)
			shapes[id] = shape(sp and sp.description)
		end
		return shapes[id]
	end
	for key in pairs(named_families()) do
		local list = fresh[key] or {}
		local top, named = nil, {}
		for _, sp in ipairs(list) do
			named[sp.level] = true
			if sp.category ~= nil and sp.level <= 6 and (top == nil or sp.level > top.level) then
				top = sp
			end
		end
		local higher = {}
		for _, sp in ipairs(top and by_cat[top.category] or {}) do
			if sp.level > top.level and not named[sp.level] and string.lower((B.family(sp.name))) ~= key
				and shape_of(sp.id) == shape_of(top.id) then
				local have = higher[sp.level]
				if have == nil or sp.id < have.id then
					higher[sp.level] = sp
				end
			end
		end
		for _, sp in pairs(higher) do
			table.insert(list, sp)
		end
	end
	book = fresh
	return n
end

-- Whether the spell book has been read.
function B.book_ready()
	return book ~= nil
end

-- The character's skill in a school, or nil when the client does not tell it.
local function skill(school)
	local name = SCHOOL_SKILL[school]
	for _, s in ipairs(game.player.skills or {}) do
		if s.name == name then
			return s.current
		end
	end
	return nil
end

-- The spell of `family` to cast: the highest level known, at most MaxLevel, whose difficulty the
-- skill of its school beats by SkillMargin (any known level where the client tells no skill or
-- difficulty); nil when none is.
function B.best(family)
	local best = nil
	for _, s in ipairs((book or {})[string.lower(family)] or {}) do
		local sk = skill(s.school)
		local reliable = sk == nil or s.difficulty == nil or sk >= s.difficulty + B.get("SkillMargin")
		if s.level <= B.get("MaxLevel") and reliable and (best == nil or s.level > best.level) then
			best = s
		end
	end
	return best
end

-- The spells a profile casts now, in its order, and the families it cannot cast.
function B.plan(profile)
	local spells, missing = {}, {}
	for _, fam in ipairs(B.profiles[profile] or {}) do
		local s = B.best(fam)
		if s ~= nil then
			table.insert(spells, s)
		else
			table.insert(missing, fam)
		end
	end
	return spells, missing
end

---------------------------------------------------------------------------------------------------
-- Tells.

-- A tell to the character: its teller's name and object id (when the line names it), and its words.
function B.parse_tell(text)
	local id, name, said = string.match(text or "", "^<Tell:IIDString:(%d+):[^>]*>(.-)<\\Tell> tells you, \"(.*)\"$")
	if name == nil then
		name, said = string.match(text or "", "^(.-) tells you, \"(.*)\"$")
	end
	if name == nil then
		return nil
	end
	return (string.gsub(name, "^%+", "")), tonumber(id), said
end

-- Tell `name` a line, in turn with the others waiting to go.
function B.tell(name, text)
	table.insert(outbox, { name = name, text = text })
end

-- One waiting tell sent, when the last went long enough ago.
function B.send_tells()
	if #outbox == 0 or os.clock() - last_tell < TELL_GAP then
		return
	end
	local t = table.remove(outbox, 1)
	last_tell = os.clock()
	local ok, why = pcall(chat.command, "/t " .. t.name .. ", " .. t.text)
	if not ok then
		B.note("could not tell %s: %s", t.name, tostring(why))
	end
end

local function same(a, b)
	return string.lower(a or "") == string.lower(b or "")
end

-- The place of `name` in the line (1 the next to be buffed), or nil.
local function place_of(name)
	for i, r in ipairs(B.queue) do
		if same(r.name, name) then
			return i
		end
	end
	return nil
end

-- The times `name` asked within the cap's hour, the old ones let go.
local function asked(name)
	local key = string.lower(name)
	local kept = {}
	for _, t in ipairs(B.asked[key] or {}) do
		if os.clock() - t < CAP_WINDOW then
			table.insert(kept, t)
		end
	end
	B.asked[key] = kept
	return kept
end

-- What a player's tell asks for: help, the profiles, leaving the line, or a profile's buffs.
function B.heard(name, id, said)
	local word = string.lower(string.match(said or "", "^%s*(%S+)") or "")
	if word == "" then
		return
	end
	if word == "help" then
		B.tell(name, string.format("Tell me a profile's name for its buffs: %s. Tell me list for each profile's spells, or cancel to leave the line.",
			table.concat(B.profile_names(), ", ")))
		return
	end
	if word == "list" then
		for _, p in ipairs(B.profile_names()) do
			local spells, missing = B.plan(p)
			local names = {}
			for _, s in ipairs(spells) do
				table.insert(names, s.name)
			end
			B.tell(name, string.format("%s: %s%s", p, #names > 0 and table.concat(names, ", ") or "nothing I can cast",
				#missing > 0 and (" (not: " .. table.concat(missing, ", ") .. ")") or ""))
		end
		return
	end
	if word == "cancel" then
		local at = place_of(name)
		if at ~= nil then
			table.remove(B.queue, at)
			B.tell(name, "You have left the line.")
		elseif B.current ~= nil and same(B.current.name, name) then
			B.current.canceled = true
			B.tell(name, "Stopping your buffs.")
		else
			B.tell(name, "You are not in the line.")
		end
		return
	end
	if B.profiles[word] == nil then
		B.tell(name, string.format("I don't know '%s'. Tell me help.", word))
		return
	end
	if not B.get("On") then
		B.tell(name, "I am not buffing right now.")
		return
	end
	local at = place_of(name)
	if at ~= nil or (B.current ~= nil and same(B.current.name, name)) then
		B.tell(name, at and string.format("You are already number %d in line.", at) or "I am buffing you now.")
		return
	end
	local times = asked(name)
	if #times >= B.get("PerPlayerCap") then
		local wait = math.ceil((CAP_WINDOW - (os.clock() - times[1])) / 60)
		B.tell(name, string.format("Your requests this hour: %d of %d; ask again in %d minutes.", #times, B.get("PerPlayerCap"), wait))
		B.note("%s refused: %d requests this hour", name, #times)
		return
	end
	if #B.queue >= B.get("QueueMax") then
		B.tell(name, "The line is full; ask again in a few minutes.")
		B.note("%s refused: the line is full", name)
		return
	end
	table.insert(times, os.clock())
	table.insert(B.queue, { name = name, id = id, profile = word, at = os.clock() })
	B.note("request: %s asks for %s (number %d in line)", name, word, #B.queue)
	if B.restocking then
		B.tell(name, string.format("I am buying components and will be back shortly; you are number %d in line.", #B.queue))
	elseif B.current ~= nil or #B.queue > 1 then
		B.tell(name, string.format("I am busy; you are number %d in line.", #B.queue))
	end
	if B.on_change then
		B.on_change()
	end
end

-- Whether a tell is from a player: a shopkeeper's welcome or another creature speaking by tell
-- is not a request, and an answer to it would only be refused (no one by that name to tell).
local function from_player(name, id)
	local o = (id and game.world:get(id)) or game.world:first({ name = name, sort = "distance" })
	return o == nil or o.class == "Player"
end

-- A line of chat: a tell to the character from a player is a request.
function B.chat_line(e)
	if e.kind ~= "tell" then
		return
	end
	local name, id, said = B.parse_tell(e.text)
	if name ~= nil and not same(name, game.player.name) and from_player(name, id) then
		B.heard(name, id, said)
	end
end

---------------------------------------------------------------------------------------------------
-- Casting.

local function code_of(why)
	local hex = string.match(tostring(why), "0x(%x+)")
	return hex and tonumber(hex, 16) or nil
end
B.code_of = code_of

local function share(vital)
	local v = game.player[vital]
	if v == nil or v.max == nil or v.max == 0 then
		return nil
	end
	return v.current / v.max
end
B.share = share

-- The requester's character, by its id, else the nearest player of its name.
local function requester(r)
	local o = r.id and game.world:get(r.id) or nil
	if o == nil then
		o = game.world:first({ class = "Player", name = r.name, sort = "distance" })
	end
	return o
end

local function in_range(r)
	local o = requester(r)
	return o ~= nil and o:distance() <= B.get("Range"), o
end

-- Wait up to `secs` for `cond()`, a quarter second at a time; answers whether it came true.
local function wait_for(cond, secs)
	local until_t = os.clock() + secs
	while os.clock() < until_t do
		if cond() then
			return true
		end
		task.sleep(250)
	end
	return cond()
end

-- Into the magic stance, for `purpose` ("support" when not given; "buff" for the character's own
-- buffs).
local function ready(purpose)
	if game.player.combat_mode == MAGIC then
		return true
	end
	local caster = B.get("Caster")
	local ok, why = fight.ready_magic({ purpose = purpose or "support", caster = caster ~= "" and caster or nil })
	if not ok then
		B.note("could not enter the magic stance: %s", tostring(why))
	end
	return ok
end
B.ready = ready

-- The first potion or food held that restores mana.
local function mana_item()
	for _, it in ipairs(game.inventory.items()) do
		if it.class == "Food" and string.find(string.lower(it.name or ""), "mana", 1, true) then
			return it
		end
	end
	return nil
end

-- One step toward `need` mana (a share of the most): Stamina to Mana while the stamina allows,
-- else a mana potion, else a rest in peace. Answers once the mana is there, or false when nothing
-- helped this time. `purpose` is the stance's, as `ready` takes it.
function B.mana_step(need, purpose)
	local m = share("mana")
	if m == nil or m >= need then
		return true
	end
	local conv = B.best(CONVERSION)
	local s = share("stamina")
	if conv ~= nil and s ~= nil and s > 0.3 and game.player.mana.current >= (conv.mana or 10) and ready(purpose) then
		set_state("Getting mana back: " .. conv.name)
		local ok, why = pawait(magic.cast(conv.id))
		B.note(ok and "mana: %s" or "mana: %s: %s", conv.name, tostring(why))
		return false
	end
	local potion = mana_item()
	if potion ~= nil then
		set_state("Getting mana back: " .. tostring(potion.name))
		local ok, why = pawait(act.use(potion.id))
		B.note(ok and "mana: drank %s" or "mana: %s: %s", tostring(potion.name), tostring(why))
		return false
	end
	set_state("Resting for mana")
	if (game.player.combat_mode or 1) > 1 then
		pawait(combat.set_mode("peace", { purpose = "support" }))
	end
	task.sleep(2000)
	return false
end

-- Rest, between players, until the mana reaches ManaReady (two minutes at most).
local function rest()
	local m = share("mana")
	if m == nil or m >= B.get("ManaRest") / 100 then
		return
	end
	B.note("resting: mana %d%%", math.floor(m * 100))
	local first = B.queue[1]
	if first ~= nil then
		B.tell(first.name, "I am getting my mana back; you are next.")
	end
	local until_t = os.clock() + 120
	while os.clock() < until_t and B.get("On") and not B.mana_step(B.get("ManaReady") / 100) do
		task.sleep(250)
	end
end

-- The components held, by name: { count } each, with what they were at their most.
local components = {}

-- Counts the components held, and warns of each the character is using up and has few of left.
function B.check_components()
	local now = {}
	for _, it in ipairs(game.inventory.items()) do
		if it.class == "SpellComponent" then
			now[it.name] = (now[it.name] or 0) + (it.stack or 1)
		end
	end
	for name, n in pairs(now) do
		local c = components[name] or { most = n, warned = false }
		c.count = n
		if n > c.most then
			c.most, c.warned = n, false
		end
		if n < c.most and n < B.get("ComponentWarn") and not c.warned then
			c.warned = true
			B.note("components low: %d %s left", n, name)
			pcall(chat.print, string.format("[Buffbot] Running low: %d %s left.", n, name))
		end
		components[name] = c
	end
	for name, c in pairs(components) do
		if now[name] == nil then
			if c.count ~= 0 then
				B.note("components: out of %s", name)
				pcall(chat.print, string.format("[Buffbot] Out of %s.", name))
			end
			c.count = 0
		end
	end
end

-- The components held, sorted: { name, count } each.
function B.component_list()
	local out = {}
	for name, c in pairs(components) do
		table.insert(out, { name = name, count = c.count or 0 })
	end
	table.sort(out, function(a, b)
		return a.name < b.name
	end)
	return out
end

-- One spell cast on the player: cast again on a fizzle (Retries times at most), after mana when
-- there was too little, after the player came closer when it was out of reach. Answers true when
-- it landed, or false and why.
local function cast_one(r, s)
	local tries = 0
	while true do
		if r.canceled or not B.get("On") then
			return false, "stopped"
		end
		local near, o = in_range(r)
		if not near then
			return false, "out of range"
		end
		local need = math.min(1, ((s.mana or 10) + 5) / math.max(1, game.player.mana and game.player.mana.max or 1))
		while not B.mana_step(need) do
			if r.canceled or not B.get("On") then
				return false, "stopped"
			end
		end
		if not ready() then
			return false, "no magic stance"
		end
		set_state(string.format("Buffing %s: %s", r.name, s.name))
		local ok, why = pawait(magic.cast(s.id, o.id))
		B.casts = B.casts + 1
		if ok then
			return true
		end
		local code = code_of(why)
		if code == TOO_LITTLE_MANA then
			B.note("%s on %s: too little mana", s.name, r.name)
		elseif code == FIZZLED then
			B.fizzles = B.fizzles + 1
			tries = tries + 1
			B.note("%s on %s: fizzled (try %d of %d)", s.name, r.name, tries, B.get("Retries"))
			if tries >= B.get("Retries") then
				return false, "fizzled " .. tries .. " times"
			end
		elseif code == OUT_OF_RANGE then
			B.note("%s on %s: out of range", s.name, r.name)
			return false, "out of range"
		elseif code == NO_COMPONENT or string.find(tostring(why), "refused by the client", 1, true) then
			B.check_components()
			B.note("%s on %s: a component missing (%s)", s.name, r.name, tostring(why))
			pcall(chat.print, string.format("[Buffbot] Missing components for %s.", s.name))
			return false, "a component missing"
		elseif code ~= nil and HOPELESS[code] then
			return false, HOPELESS[code]
		else
			tries = tries + 1
			B.note("%s on %s: %s (try %d of %d)", s.name, r.name, tostring(why), tries, B.get("Retries"))
			if tries >= B.get("Retries") then
				return false, tostring(why)
			end
		end
	end
end

-- Wait for an out-of-reach player to come within Range: asked once, waited for ComeCloserSeconds.
local function come_closer(r)
	B.tell(r.name, string.format("Please come within %d m of me; I'll wait %d seconds.", B.get("Range"), B.get("ComeCloserSeconds")))
	set_state("Waiting for " .. r.name .. " to come closer")
	B.note("%s is out of range: waiting %d s", r.name, B.get("ComeCloserSeconds"))
	return wait_for(function()
		return r.canceled or in_range(r)
	end, B.get("ComeCloserSeconds")) and not r.canceled
end

-- Buff one player: its profile's spells in turn, then "Done".
function B.serve(r)
	B.current = r
	if B.on_change then
		B.on_change()
	end
	if not in_range(r) and not come_closer(r) then
		if not r.canceled then
			B.tell(r.name, "You didn't come near enough; tell me again when you're close.")
			B.note("skipped %s: not near enough", r.name)
		end
		B.current = nil
		return
	end
	local spells, missing = B.plan(r.profile)
	B.tell(r.name, string.format("Buffing you now: %s, %d spells.", r.profile, #spells))
	B.note("buffing %s: %s, %d spells%s", r.name, r.profile, #spells,
		#missing > 0 and (" (not known: " .. table.concat(missing, ", ") .. ")") or "")
	local landed, failed = 0, {}
	for _, s in ipairs(spells) do
		local ok, why = cast_one(r, s)
		if ok then
			landed = landed + 1
			B.note("cast %s on %s", s.name, r.name)
		elseif why == "out of range" then
			if come_closer(r) then
				ok, why = cast_one(r, s)
				if ok then
					landed = landed + 1
					B.note("cast %s on %s", s.name, r.name)
				end
			end
			if not ok then
				table.insert(failed, s.name)
				B.failures = B.failures + 1
				B.note("failed %s on %s: %s", s.name, r.name, tostring(why))
				if why == "out of range" then
					break
				end
			end
		else
			table.insert(failed, s.name)
			B.failures = B.failures + 1
			B.note("failed %s on %s: %s", s.name, r.name, tostring(why))
			if why == "stopped" then
				break
			end
		end
		B.check_components()
	end
	B.served = B.served + 1
	if r.canceled then
		B.note("stopped buffing %s", r.name)
	elseif #failed == 0 then
		B.tell(r.name, string.format("Done: %d buffs cast on you.", landed))
		B.note("done: %s, %d buffs", r.name, landed)
	else
		B.tell(r.name, string.format("Done: %d of %d buffs; not cast: %s.", landed, #spells, table.concat(failed, ", ")))
		B.note("done: %s, %d of %d buffs (not cast: %s)", r.name, landed, #spells, table.concat(failed, ", "))
	end
	B.current = nil
	if B.on_change then
		B.on_change()
	end
end

-- One turn of the bot's loop: the next player, after a rest when the mana is low.
local function turn()
	if (B.get("On") or Self.wanted()) and book == nil then
		local ok, n = pcall(B.read_book)
		B.note(ok and "%s spells known" or "could not read the spell book: %s", tostring(n))
	end
	B.check_components()
	-- The character's own buffs first, between players.
	if Self.step() then
		return
	end
	if not B.get("On") then
		set_state("Off.")
		task.sleep(500)
		return
	end
	rest()
	local now = B.restock_asked
	B.restock_asked = false
	R.step(now)
	local r = table.remove(B.queue, 1)
	if r ~= nil then
		B.serve(r)
	else
		set_state("Waiting for requests.")
		task.sleep(250)
	end
end

-- The bot's loop. An error is noted with where it happened, and the loop goes on a second later.
function B.run()
	local function where(e)
		return debug ~= nil and debug.traceback(tostring(e), 2) or tostring(e)
	end
	while true do
		local ok, err = xpcall(turn, where)
		if not ok then
			B.current = nil
			B.note("error: %s", tostring(err))
			task.sleep(1000)
		end
	end
end

-- The book read again (a spell learned, a skill raised).
function B.forget_book()
	book = nil
end

B.restocking = false
B.restock_asked = false
B.restock = R
R.init(B)
B.self = Self
Self.init(B)

return B
