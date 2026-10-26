-- The inventory log, in the manner of Mag-Tools' inventory logger: what each character holds (its
-- packs and what it wears), kept for the world, so that any character of it searches them all.
--
-- A character's log is taken a few seconds after it enters the world, and again a few seconds
-- after its inventory changes. Each item is kept as a record: its name, class, count, where it is
-- (worn, or which pack), value and burden, and, once it has been appraised, what its appraisal
-- tells (armor level, spells, set, workmanship, ratings, what it takes to wield, arcane lore,
-- tinkers, whether it is attuned). With Appraise on, the gear held that has not been appraised is
-- appraised, four a second, so its record is whole.

local L = {}

-- Property numbers.
local P = {
	VALUE = 19, BURDEN = 5, STACK = 12, LOCATIONS = 9, ARMOR = 28, WORK = 105, SET = 265,
	REQ = 158, REQ_SKILL = 159, REQ_DIFF = 160, LORE = 109, TINKERS = 171, ATTUNED = 114,
	DAMAGE = 44, LEVEL = 25,
}
-- The ratings an item may carry: its property, and the record's key.
L.RATINGS = {
	{ 370, "dmg" }, { 371, "dmgres" }, { 372, "crit" }, { 373, "critres" }, { 374, "critdmg" },
	{ 375, "critdmgres" }, { 376, "heal" }, { 379, "vit" },
}
-- The figure outside the property tables of where an item may be worn.
local VALID_LOCATIONS = 0x0D00000E
-- An item's spell list marks an enchantment cast on it (not its own) with the high bit.
local ENCHANTMENT = 0x80000000

-- The classes of gear: what is worn or wielded, appraised for the log.
L.GEAR = { Armor = true, Clothing = true, Jewelry = true, MeleeWeapon = true, MissileWeapon = true,
	WandStaffOrb = true }

-- The store's key of a character's log, and of the list of characters logged.
local function key(name)
	return "inv:" .. string.lower(name)
end
local INDEX = "inv_characters"

-- A spell's name, by id (the spell table does not change).
local spell_names = {}
local function spell_name(id)
	local n = spell_names[id]
	if n == nil then
		local sp = magic.spell(id)
		n = sp and sp.name or ("spell " .. id)
		spell_names[id] = n
	end
	return n
end

-- The record of one item: `where` is "worn", or the pack it is in (1 the main pack).
function L.entry(item, where)
	local props = item:properties() or {}
	local ints = props.ints or {}
	local r = {
		id = item.id,
		n = item.name or (props.strings or {})[1] or "?",
		c = item.class,
		k = item.stack or ints[P.STACK] or 1,
		w = where,
		v = item.value or ints[P.VALUE],
		b = item.burden or ints[P.BURDEN],
		loc = ints[P.LOCATIONS] or item:int(VALID_LOCATIONS),
	}
	local wk = item.workmanship or ints[P.WORK]
	if wk ~= nil then
		r.wk = math.floor(wk * 100 + 0.5) / 100
	end
	if item.appraised then
		r.ap = true
		r.al = ints[P.ARMOR]
		r.set = ints[P.SET]
		r.lore = ints[P.LORE]
		r.tk = ints[P.TINKERS]
		r.dmg = ints[P.DAMAGE]
		if (ints[P.ATTUNED] or 0) > 0 then
			r.att = true
		end
		if ints[P.REQ] ~= nil and ints[P.REQ] > 0 then
			r.rq = { t = ints[P.REQ], s = ints[P.REQ_SKILL], d = ints[P.REQ_DIFF] }
		end
		for _, rt in ipairs(L.RATINGS) do
			local v = ints[rt[1]]
			if v ~= nil and v ~= 0 then
				r.r = r.r or {}
				r.r[rt[2]] = v
			end
		end
		local sp = {}
		for _, id in ipairs(item.spells or {}) do
			if id < ENCHANTMENT then
				table.insert(sp, spell_name(id))
			end
		end
		if #sp > 0 then
			r.sp = sp
		end
	end
	return r
end

-- Where each item the character holds is: "worn", or its pack's place (1 the main pack).
local function places()
	local me = game.player.id
	local order = {}
	for i, p in ipairs(game.inventory.packs()) do
		order[p.id] = i
	end
	return function(item)
		if item.wielder == me then
			return "worn"
		end
		return order[item.container] or 1
	end
end

-- This character's log, taken now (inside a task: a few items a frame).
function L.take()
	local where = places()
	local items = {}
	for i, it in ipairs(game.inventory.items()) do
		table.insert(items, L.entry(it, where(it)))
		if i % 5 == 0 then
			task.sleep(0)
		end
	end
	local name = game.player.name
	local rec = { name = name, at = os.time(), level = game.player_int(P.LEVEL), items = items }
	task.sleep(0)
	store.world[key(name)] = rec
	task.sleep(0)
	-- The index: each character's name, level, count of items and when it was logged.
	local kept = {}
	for _, c in ipairs(store.world[INDEX] or {}) do
		if type(c) == "table" and string.lower(c.name or "") ~= string.lower(name) then
			table.insert(kept, c)
		end
	end
	table.insert(kept, { name = name, level = rec.level, items = #items, at = rec.at })
	table.sort(kept, function(a, b)
		return a.name < b.name
	end)
	store.world[INDEX] = kept
	return rec
end

-- The characters logged, sorted: { name, level, items, at } each.
function L.summaries()
	local out = {}
	for _, c in ipairs(store.world[INDEX] or {}) do
		if type(c) == "table" then
			table.insert(out, c)
		end
	end
	return out
end

-- The names of the characters logged, sorted.
function L.characters()
	local out = {}
	for _, c in ipairs(L.summaries()) do
		table.insert(out, c.name)
	end
	return out
end

-- A character's log, or nil.
function L.record(name)
	return store.world[key(name)]
end

-- Forget a character's log; answers whether there was one.
function L.forget(name)
	local kept, found = {}, false
	for _, c in ipairs(L.summaries()) do
		if string.lower(c.name) == string.lower(name) then
			found = true
		else
			table.insert(kept, c)
		end
	end
	if found then
		store.world[key(name)] = nil
		store.world[INDEX] = kept
	end
	return found
end

-- The items known appraised, and those known not to be gear, by id: each is looked at once.
local appraised, not_gear = {}, {}

-- The first piece of gear held that has not been appraised, or nil (inside a task: a few dozen
-- items a frame).
function L.next_unappraised()
	for i, it in ipairs(game.inventory.items()) do
		local id = it.id
		if not appraised[id] and not not_gear[id] then
			if it.appraised then
				appraised[id] = true
			elseif L.GEAR[it.class] then
				return it
			else
				not_gear[id] = true
			end
		end
		if i % 40 == 0 then
			task.sleep(0)
		end
	end
	return nil
end

-- What a record is found by: its name, its spells and its set's name, lower case.
function L.text_of(r, set_name)
	local parts = { r.n }
	for _, s in ipairs(r.sp or {}) do
		table.insert(parts, s)
	end
	if r.set ~= nil and set_name ~= nil then
		table.insert(parts, set_name(r.set) or "")
	end
	return string.lower(table.concat(parts, " | "))
end

-- Every logged item, of every character, whose text holds each word of `text` (without case):
-- { character, item } each, by character and then name. Inside a task: a few hundred items a
-- frame.
function L.find(text, set_name)
	local words = {}
	for w in string.gmatch(string.lower(text or ""), "%S+") do
		table.insert(words, w)
	end
	local out, looked = {}, 0
	for _, name in ipairs(L.characters()) do
		local rec = L.record(name)
		task.sleep(0)
		for _, r in ipairs(rec and rec.items or {}) do
			looked = looked + 1
			if looked % 300 == 0 then
				task.sleep(0)
			end
			local t = L.text_of(r, set_name)
			local all = true
			for _, w in ipairs(words) do
				if not string.find(t, w, 1, true) then
					all = false
					break
				end
			end
			if all then
				table.insert(out, { character = rec.name or name, item = r })
			end
		end
	end
	table.sort(out, function(a, b)
		if a.character ~= b.character then
			return a.character < b.character
		end
		return a.item.n < b.item.n
	end)
	return out
end

return L
