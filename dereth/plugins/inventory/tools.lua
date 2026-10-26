-- The inventory's everyday tools, in the manner of Mag-Tools: partial stacks merged, items put in
-- the packs their pack rules name, sets of gear worn at once, and trading at a vendor by the
-- vendor's own loot profile.

local T = {}

-- The packs: { id, free } each, the main pack first.
local function packs()
	local out = {}
	for _, p in ipairs(game.inventory.packs()) do
		table.insert(out, { id = p.id, free = (p.capacity or 0) - (p.used or 0) })
	end
	return out
end

-- The items in the packs, in the order the packs show them (the main pack, then each side pack),
-- worn items and the packs themselves left out: { item, pack } each. Inside a task: a few dozen
-- items a frame.
local function in_packs()
	local order, ps = {}, packs()
	for i, p in ipairs(ps) do
		order[p.id] = i
	end
	local out = {}
	for i, it in ipairs(game.inventory.items()) do
		local worn = it.wielder ~= nil and it.wielder ~= 0
		if not worn and order[it.id] == nil and order[it.container] ~= nil then
			table.insert(out, { item = it, pack = order[it.container], at = i })
		end
		if i % 40 == 0 then
			task.sleep(0)
		end
	end
	table.sort(out, function(a, b)
		if a.pack ~= b.pack then
			return a.pack < b.pack
		end
		return a.at < b.at
	end)
	return out
end

-- Stacking ----------------------------------------------------------------------------------------

-- One merge of partial stacks of one name, the top-most into the bottom-most with room, so the one
-- partial stack left is the top-most; the names in `skip` are left. Answers a line when it merged
-- (or tried), and the name, nil when nothing was left to merge.
function T.stack_step(skip)
	local kinds, order = {}, {}
	for i, e in ipairs(in_packs()) do
		local it = e.item
		if i % 40 == 0 then
			task.sleep(0)
		end
		if (it.max_stack or 1) > 1 then
			local k = it.name or ""
			if kinds[k] == nil then
				kinds[k] = {}
				table.insert(order, k)
			end
			table.insert(kinds[k], it)
		end
	end
	for _, k in ipairs(order) do
		local list = (skip or {})[k] and {} or kinds[k]
		for to = #list, 2, -1 do
			local into = list[to]
			local room = (into.max_stack or 1) - (into.stack or 1)
			if room > 0 then
				local from = list[1]
				local n = math.min(room, from.stack or 1)
				local ok, why = pawait(items.merge(from.id, into.id, n))
				return ok and string.format("stacked %d %s", n, k) or string.format("stacking %s: %s", k, tostring(why)), k, ok
			end
		end
	end
	return nil
end

-- Pack rules --------------------------------------------------------------------------------------

-- The pack rules a text writes ("Scarab=2, Taper=2, Salvage=3"): an item whose name holds the
-- words goes to that pack (1 the main pack, 2 the first side pack). { words, pack } each.
function T.pack_rules(text)
	local out = {}
	for words, n in string.gmatch(tostring(text or ""), "%s*([^=,]-)%s*=%s*(%d+)%s*,?") do
		if words ~= "" then
			table.insert(out, { words = words, pack = tonumber(n) })
		end
	end
	return out
end

-- The pack a rule puts an item of `name` in, or nil.
local function rule_for(rules, name)
	local lower = string.lower(name or "")
	for _, r in ipairs(rules) do
		if string.find(lower, string.lower(r.words), 1, true) then
			return r.pack
		end
	end
	return nil
end

-- One item moved to the pack its rule names, when it is elsewhere and that pack has room; the
-- items in `skip` (by id) are left. Answers a line when it moved one (or tried), and the item's
-- id, nil when every item is where its rule puts it.
function T.pack_step(rules, skip)
	if #rules == 0 then
		return nil
	end
	local ps = packs()
	for i, e in ipairs(in_packs()) do
		if i % 60 == 0 then
			task.sleep(0)
		end
		local want = not (skip or {})[e.item.id] and rule_for(rules, e.item.name) or nil
		if want ~= nil and want ~= e.pack and ps[want] ~= nil and e.item.class ~= "Container" then
			if ps[want].free <= 0 then
				return nil
			end
			local ok, why = pawait(items.move(e.item.id, ps[want].id))
			return ok and string.format("put %s in pack %d", e.item.name, want)
				or string.format("putting %s in pack %d: %s", e.item.name, want, tostring(why)), e.item.id, ok
		end
	end
	return nil
end

-- Sets of gear -------------------------------------------------------------------------------------

-- The character's sets of gear: name -> { id, name } each, what it wore when the set was saved.
local function sets()
	return store.character.inv_sets or {}
end

function T.set_names()
	local out = {}
	for name in pairs(sets()) do
		table.insert(out, name)
	end
	table.sort(out)
	return out
end

function T.set_items(name)
	return sets()[string.lower(name or "")]
end

-- Keep what the character wears now as the set `name`; answers the line to tell.
function T.save_set(name)
	name = string.lower(string.match(name or "", "^%s*(.-)%s*$"))
	if name == "" then
		return "a set needs a name"
	end
	local me, worn = game.player.id, {}
	for _, it in ipairs(game.inventory.items()) do
		if it.wielder == me then
			table.insert(worn, { id = it.id, name = it.name })
		end
	end
	local all = sets()
	all[name] = worn
	store.character.inv_sets = all
	return string.format("set %s: %d items", name, #worn)
end

function T.remove_set(name)
	local all = sets()
	name = string.lower(name or "")
	if all[name] == nil then
		return "no set " .. name
	end
	all[name] = nil
	store.character.inv_sets = all
	return "removed the set " .. name
end

-- Wear the set `name`, an item at a time (inside a task); answers the line to tell.
function T.wear_set(name)
	local set = T.set_items(name)
	if set == nil then
		return "no set " .. tostring(name)
	end
	local held, me = {}, game.player.id
	for _, it in ipairs(game.inventory.items()) do
		held[it.id] = it
	end
	local worn, missing, failed = 0, {}, {}
	for _, s in ipairs(set) do
		local it = held[s.id]
		if it == nil then
			table.insert(missing, s.name)
		elseif it.wielder ~= me then
			local ok, why = pawait(items.wield(it.id))
			if ok then
				worn = worn + 1
			else
				table.insert(failed, string.format("%s (%s)", s.name, tostring(why)))
			end
		end
	end
	local line = string.format("set %s: %d put on", string.lower(name), worn)
	if #missing > 0 then
		line = line .. "; not held: " .. table.concat(missing, ", ")
	end
	if #failed > 0 then
		line = line .. "; not put on: " .. table.concat(failed, ", ")
	end
	return line
end

-- Trading by a vendor's profile --------------------------------------------------------------------

local lootlib = require("dereth.loot")

-- The pyreals the character carries (its integer property).
local COIN_VALUE = 20

-- How many of `name` the character holds, stacks counted whole, without case.
local function held(name)
	local want, n = string.lower(name or ""), 0
	for _, it in ipairs(game.inventory.items()) do
		if string.lower(it.name or "") == want then
			n = n + (it.stack or 1)
		end
	end
	return n
end

-- A vendor's own profile, `vendors/<its name>.utl` in the data folder, or nil (read again when its
-- text changes).
local read = {}
function T.vendor_profile(name)
	if name == nil or name == "" then
		return nil
	end
	local text = files.data("vendors/" .. name .. ".utl")
	if text == nil then
		return nil
	end
	local r = read[name]
	if r ~= nil and r.text == text then
		return r.profile
	end
	local ok, p = pcall(lootlib.load, text)
	read[name] = { text = text, profile = ok and p or nil }
	return ok and p or nil
end

-- The ids of the items worn in a saved set: never sold.
local function kept_ids()
	local out = {}
	for _, set in pairs(sets()) do
		for _, s in ipairs(set) do
			out[s.id] = true
		end
	end
	return out
end

-- What to sell: in the packs (not worn, not a pack, not in a saved set), that the profile's Sell
-- rules match. Their ids and names.
function T.to_sell(profile)
	local ids, names, kept = {}, {}, kept_ids()
	for i, e in ipairs(in_packs()) do
		local it = e.item
		if i % 20 == 0 then
			task.sleep(0)
		end
		if it.class ~= "Container" and not kept[it.id] then
			local hit = profile:classify(it)
			if hit ~= nil and hit.action == "sell" then
				table.insert(ids, it.id)
				table.insert(names, it.name)
			end
		end
	end
	return ids, names
end

-- What to buy from the stock: what a Keep # rule matches, up to its count, and one of what a Keep
-- rule matches that the character lacks; no more than `coins` pay for. { id, count, name } each.
function T.to_buy(stock, profile, coins)
	local rows, left = {}, coins
	for _, r in ipairs(stock or {}) do
		local o = game.world:get(r.id)
		local hit = o and profile:classify(o) or nil
		local need = 0
		if hit ~= nil and hit.action == "keep_up_to" then
			need = (hit.count or 0) - held(r.name)
		elseif hit ~= nil and hit.action == "keep" and held(r.name) == 0 then
			need = 1
		end
		if r.amount ~= nil and r.amount >= 0 then
			need = math.min(need, r.amount)
		end
		local price = r.price or 0
		if price > 0 then
			need = math.min(need, math.floor(left / price))
		end
		if need > 0 then
			table.insert(rows, { id = r.id, count = need, name = r.name })
			left = left - need * price
		end
	end
	return rows
end

-- Trade with the open vendor by its profile, inside a task: sell, then buy. Answers the line to
-- tell, or nil when no vendor is open or it has no profile.
function T.trade()
	local info = vendor.info()
	if info == nil then
		return nil
	end
	local shop = game.world:get(info.id)
	local name = shop and shop.name or ""
	local profile = T.vendor_profile(name)
	if profile == nil then
		return nil
	end
	local before = game.player_int(COIN_VALUE) or 0
	local sold, bought = 0, {}
	local ids = T.to_sell(profile)
	if #ids > 0 then
		local ok, why = pawait(vendor.sell(ids))
		if ok then
			sold = #ids
		else
			return string.format("trading with %s: selling: %s", name, tostring(why))
		end
	end
	info = vendor.info() or info
	local rows = T.to_buy(info.stock, profile, game.player_int(COIN_VALUE) or 0)
	if #rows > 0 then
		local list = {}
		for _, r in ipairs(rows) do
			table.insert(list, { id = r.id, count = r.count })
			table.insert(bought, string.format("%d %s", r.count, r.name))
		end
		local ok, why = pawait(vendor.buy(list))
		if not ok then
			return string.format("trading with %s: buying: %s", name, tostring(why))
		end
	end
	local after = game.player_int(COIN_VALUE) or 0
	return string.format("traded with %s: sold %d items, bought %s; pyreals %d to %d", name, sold,
		#bought > 0 and table.concat(bought, ", ") or "nothing", before, after)
end

return T
