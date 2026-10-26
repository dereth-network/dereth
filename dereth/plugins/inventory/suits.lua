-- The suit builder, in the manner of SuitBuilder: from the gear every character of the world has
-- logged, the best suit for the character playing: armor and underclothes over each part of the
-- body, a necklace, two bracelets, two rings, a trinket and a cloak, no two pieces over one place.
--
-- What matters, in this order: the spells asked for (each family, "Strength" or "Invulnerability",
-- counted once at the best tier the suit holds: Legendary 5, Epic 4, Major 3, Moderate 2, Minor 1)
-- and the pieces of the set asked for (up to five); then the items' ratings; then the armor level
-- over each part of the body. The pieces locked are in every suit. An item is left out when it
-- takes a higher level to wield than the character has, when it is attuned to another character
-- (it cannot be handed over), or when it comes from a character the search was not asked to use.
--
-- The search tries the suits in turn, the most promising pieces first, and leaves a branch as
-- soon as nothing in it could beat the best suit found; it keeps that and the four best met on the
-- way. It runs in a task, a hundred or so suits a frame, and stops at its limit with the best
-- found.

local S = {}

-- The places on the body, by the bits of where an item may be worn.
S.PLACES = {
	{ 0x1, "Head" }, { 0x200, "Chest" }, { 0x400, "Abdomen" }, { 0x800, "Upper arms" },
	{ 0x1000, "Lower arms" }, { 0x20, "Hands" }, { 0x2000, "Upper legs" }, { 0x4000, "Lower legs" },
	{ 0x100, "Feet" }, { 0x2, "Shirt: chest" }, { 0x8, "Shirt: upper arms" }, { 0x10, "Shirt: lower arms" },
	{ 0x4, "Pants: abdomen" }, { 0x40, "Pants: upper legs" }, { 0x80, "Pants: lower legs" },
	{ 0x8000, "Neck" }, { 0x10000, "Left wrist" }, { 0x20000, "Right wrist" }, { 0x40000, "Left finger" },
	{ 0x80000, "Right finger" }, { 0x4000000, "Trinket" }, { 0x8000000, "Cloak" },
}
local band, bor, bnot = bit32.band, bit32.bor, bit32.bnot
local ALL = 0
for _, p in ipairs(S.PLACES) do
	ALL = bor(ALL, p[1])
end
-- The places a piece of armor protects (its armor level counts on each).
local ARMOR_PLACES = bor(0x1, 0x200, 0x400, 0x800, 0x1000, 0x20, 0x2000, 0x4000, 0x100)
-- The pairs of places an item takes either of: the wrists, the fingers.
local EITHER = { bor(0x10000, 0x20000), bor(0x40000, 0x80000) }

-- The weight of each thing that matters.
local SPELL, SET, SET_CAP, RATING, ARMOR = 1000, 2500, 5, 200, 0.2
-- The cantrip tiers, by the word that opens a spell's name.
local TIERS = { Minor = 1, Moderate = 2, Major = 3, Epic = 4, Legendary = 5 }
-- The most candidates a place is tried with (the locked pieces aside), and the suits tried at
-- most, a few dozen a frame.
local PER_PLACE, LIMIT, PER_FRAME = 8, 60000, 150

local function bits(m)
	local n = 0
	while m ~= 0 do
		m = band(m, m - 1)
		n = n + 1
	end
	return n
end

-- The names of the places a mask covers.
function S.place_names(mask)
	local out = {}
	for _, p in ipairs(S.PLACES) do
		if band(mask, p[1]) ~= 0 then
			table.insert(out, p[2])
		end
	end
	return table.concat(out, ", ")
end

-- A spell's family and tier: "Epic Strength" -> "strength", 4; nil for a spell with no tier word.
local function cantrip(name)
	local word, rest = string.match(name or "", "^(%a+)%s+(.+)$")
	local tier = word and TIERS[word]
	if tier == nil then
		return nil
	end
	return string.lower(rest), tier
end

-- The families asked for, lower case, from their text ("Strength, Invulnerability").
function S.wanted_spells(text)
	local out = {}
	for part in string.gmatch(text or "", "[^,]+") do
		local f = string.lower(string.match(part, "^%s*(.-)%s*$"))
		if f ~= "" then
			table.insert(out, f)
		end
	end
	return out
end

-- The set asked for, by its name (part of it, without case) or number; nil for none.
function S.wanted_set(text, sets)
	text = string.match(text or "", "^%s*(.-)%s*$")
	if text == "" then
		return nil
	end
	local n = tonumber(text)
	if n ~= nil then
		return n
	end
	local lower = string.lower(text)
	for id, name in pairs(sets) do
		if string.find(string.lower(name), lower, 1, true) then
			return id
		end
	end
	return nil
end

-- The candidates: each logged, appraised item wearable in the suit's places, that the character
-- may use. `opt`: { me (the character's name), level, from (the characters to draw from, lower
-- case; nil for all), spells (the families asked for), set (the set asked for) }.
function S.candidates(records, opt)
	local out = {}
	for _, rec in ipairs(records) do
		local owner = rec.name or "?"
		local mine = string.lower(owner) == string.lower(opt.me or "")
		if opt.from == nil or opt.from[string.lower(owner)] or mine then
			for _, r in ipairs(rec.items or {}) do
				local cover = band(r.loc or 0, ALL)
				local too_high = r.rq ~= nil and r.rq.t == 7 and opt.level ~= nil and (r.rq.d or 0) > opt.level
				if r.ap and cover ~= 0 and not too_high and (mine or not r.att) then
					local c = { owner = owner, item = r, cover = cover, key = owner .. ":" .. tostring(r.id) }
					for _, pair in ipairs(EITHER) do
						if cover == pair then
							c.either = pair
						end
					end
					c.spells = {}
					for _, name in ipairs(r.sp or {}) do
						local fam, tier = cantrip(name)
						if fam ~= nil then
							for i, want in ipairs(opt.spells or {}) do
								if string.find(fam, want, 1, true) then
									table.insert(c.spells, { i, tier })
								end
							end
						end
					end
					c.set = opt.set ~= nil and r.set == opt.set
					local rating = 0
					for _, v in pairs(r.r or {}) do
						rating = rating + v
					end
					c.rating = rating
					c.armor = (r.al or 0) * bits(band(cover, ARMOR_PLACES))
					-- Its worth alone: what it adds to an empty suit.
					local alone = rating * RATING + c.armor * ARMOR + (c.set and SET or 0)
					for _, s in ipairs(c.spells) do
						alone = alone + s[2] * SPELL
					end
					c.alone = alone
					table.insert(out, c)
				end
			end
		end
	end
	return out
end

-- The best suits from `cands`, the pieces `locked` (candidate keys) in each: { score, pieces
-- ({ candidate, place }), spells (family -> tier), set (pieces), rating, armor } each, best first;
-- and how many suits were tried, and whether the search stopped at its limit. Inside a task.
function S.search(cands, opt, locked)
	locked = locked or {}
	local nwant = #(opt.spells or {})
	-- The candidates of each place, best alone first.
	local by_place = {}
	for _, p in ipairs(S.PLACES) do
		local list = {}
		for _, c in ipairs(cands) do
			if band(c.cover, p[1]) ~= 0 and not locked[c.key] then
				table.insert(list, c)
			end
		end
		table.sort(list, function(a, b)
			if a.alone ~= b.alone then
				return a.alone > b.alone
			end
			return a.key < b.key
		end)
		while #list > PER_PLACE do
			table.remove(list)
		end
		by_place[p[1]] = list
	end
	-- The most each place could add, for the bound: the best ratings and armor of its candidates
	-- spread over the places each covers; whether a piece of the set could go there; and the best
	-- tier of each family asked for.
	local share, set_at, tier_at = {}, {}, {}
	for _, p in ipairs(S.PLACES) do
		local m, has_set, tiers_here = 0, false, {}
		for _, c in ipairs(by_place[p[1]]) do
			local rest = c.rating * RATING + c.armor * ARMOR
			m = math.max(m, c.either and rest or rest / bits(c.cover))
			has_set = has_set or c.set
			for _, sp in ipairs(c.spells) do
				tiers_here[sp[1]] = math.max(tiers_here[sp[1]] or 0, sp[2])
			end
		end
		share[p[1]], set_at[p[1]], tier_at[p[1]] = m, has_set, tiers_here
	end

	local covered, used = 0, {}
	local pieces = {}
	-- Each family's tiers held now (counts by tier), and the set's pieces, the ratings, the armor.
	local tiers = {}
	for i = 1, nwant do
		tiers[i] = {}
	end
	local set_n, rating, armor = 0, 0, 0
	local kept = {}
	local tried, stopped = 0, false

	local function best_tier(i)
		local b = 0
		for t, n in pairs(tiers[i]) do
			if n > 0 and t > b then
				b = t
			end
		end
		return b
	end

	local function score()
		local s = math.min(set_n, SET_CAP) * SET + rating * RATING + armor * ARMOR
		for i = 1, nwant do
			s = s + best_tier(i) * SPELL
		end
		return s
	end

	-- A branch is left once it cannot beat the best suit found: the best is the best there is,
	-- and the others kept are the best met on the way to it.
	local function floor_kept()
		return #kept == 0 and -1 or kept[1].score
	end

	-- The suits kept, by their pieces: one suit is kept once (two rings swapped are one suit).
	local kept_keys = {}

	local function keep()
		local s = score()
		if #kept >= 5 and s <= kept[#kept].score then
			return
		end
		local keys = {}
		for _, p in ipairs(pieces) do
			table.insert(keys, p.candidate.key)
		end
		table.sort(keys)
		local sig = table.concat(keys, ",")
		if kept_keys[sig] then
			return
		end
		local spells = {}
		for i = 1, nwant do
			spells[opt.spells[i]] = best_tier(i)
		end
		local copy = {}
		for _, p in ipairs(pieces) do
			table.insert(copy, { candidate = p.candidate, place = p.place })
		end
		table.insert(kept, { score = s, pieces = copy, spells = spells, set = set_n, rating = rating, armor = armor, sig = sig })
		kept_keys[sig] = true
		table.sort(kept, function(a, b)
			return a.score > b.score
		end)
		while #kept > 5 do
			local gone = table.remove(kept)
			kept_keys[gone.sig] = nil
		end
	end

	local function put(c, place)
		used[c.key] = true
		covered = bor(covered, place)
		table.insert(pieces, { candidate = c, place = place })
		for _, s in ipairs(c.spells) do
			tiers[s[1]][s[2]] = (tiers[s[1]][s[2]] or 0) + 1
		end
		set_n = set_n + (c.set and 1 or 0)
		rating = rating + c.rating
		armor = armor + c.armor
	end

	local function take_off(c, place)
		used[c.key] = nil
		covered = band(covered, bnot(place))
		table.remove(pieces)
		for _, s in ipairs(c.spells) do
			tiers[s[1]][s[2]] = tiers[s[1]][s[2]] - 1
		end
		set_n = set_n - (c.set and 1 or 0)
		rating = rating - c.rating
		armor = armor - c.armor
	end

	-- Where a candidate goes now, covering `place`: all its places, or for one worn on either
	-- wrist or finger, `place` alone; nil when it does not fit.
	local function fit(c, place)
		if c.either ~= nil then
			return band(covered, place) == 0 and place or nil
		end
		return band(covered, c.cover) == 0 and c.cover or nil
	end

	-- The locked pieces first: each where it fits.
	local refused = {}
	for _, c in ipairs(cands) do
		if locked[c.key] then
			local place = nil
			if c.either ~= nil then
				for _, p in ipairs(S.PLACES) do
					if place == nil and band(c.either, p[1]) ~= 0 and band(covered, p[1]) == 0 then
						place = p[1]
					end
				end
			elseif band(covered, c.cover) == 0 then
				place = c.cover
			end
			if place ~= nil then
				put(c, place)
			else
				table.insert(refused, c)
			end
		end
	end

	-- The most the places still free could add: their ratings and armor, the set's pieces up to
	-- its cap, and each family's rise to the best tier among them.
	local function bound()
		local b, sets, top = 0, 0, {}
		for _, p in ipairs(S.PLACES) do
			if band(covered, p[1]) == 0 then
				b = b + share[p[1]]
				if set_at[p[1]] then
					sets = sets + 1
				end
				for i, t in pairs(tier_at[p[1]]) do
					top[i] = math.max(top[i] or 0, t)
				end
			end
		end
		b = b + math.max(0, math.min(sets, SET_CAP - set_n)) * SET
		for i, t in pairs(top) do
			b = b + math.max(0, t - best_tier(i)) * SPELL
		end
		return b
	end

	local function walk()
		if stopped then
			return
		end
		tried = tried + 1
		if tried % PER_FRAME == 0 then
			task.sleep(0)
		end
		if tried >= LIMIT then
			stopped = true
			return
		end
		local place = nil
		for _, p in ipairs(S.PLACES) do
			if band(covered, p[1]) == 0 then
				place = p[1]
				break
			end
		end
		if place == nil then
			keep()
			return
		end
		if score() + bound() <= floor_kept() then
			return
		end
		for _, c in ipairs(by_place[place]) do
			if not used[c.key] then
				local at = fit(c, place)
				if at ~= nil then
					put(c, at)
					walk()
					take_off(c, at)
					if stopped then
						return
					end
				end
			end
		end
		-- The place left empty.
		covered = bor(covered, place)
		walk()
		covered = band(covered, bnot(place))
	end

	walk()
	if #kept == 0 then
		keep()
	end
	return kept, tried, stopped, refused
end

-- A suit said in one line: its set's pieces, its spells, its ratings and armor.
function S.summary(suit, set_name)
	local parts = {}
	if set_name ~= nil then
		table.insert(parts, string.format("%d of %s", suit.set, set_name))
	end
	local spells = {}
	local names = {}
	for fam in pairs(suit.spells) do
		table.insert(names, fam)
	end
	table.sort(names)
	local WORD = {}
	for w, t in pairs(TIERS) do
		WORD[t] = w
	end
	for _, fam in ipairs(names) do
		local t = suit.spells[fam]
		local title = (string.gsub(fam, "(%a)([%w']*)", function(a, b)
			return string.upper(a) .. b
		end))
		table.insert(spells, t > 0 and (WORD[t] .. " " .. title) or ("no " .. title))
	end
	if #spells > 0 then
		table.insert(parts, table.concat(spells, ", "))
	end
	table.insert(parts, string.format("ratings %d", suit.rating))
	table.insert(parts, string.format("armor %d", math.floor(suit.armor)))
	return table.concat(parts, "; ")
end

return S
