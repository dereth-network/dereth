-- Trading at a vendor by profile, and the trip to the vendor and back.
--
-- With a vendor's wares open (at a route's vendor waypoint, on the vendor trip, or by the player's
-- /tank sell), the macro sells and buys, in that order:
--
-- - **Sells** (VendorSell) what the pack holds that the loot profile's Sell rules match, and what
--   the vendor's own profile's Sell rules match. Nothing worn or wielded is sold, nor a pack, nor
--   an item the settings name for the macro's own use (its weapons, caster, ammunition, offhand,
--   consumables).
-- - **Buys** (VendorBuy) each item of the buy list the vendor stocks, up to its count, counting
--   what the pack holds (the Virindi Item Tool's buy list), and by the vendor's own profile: what
--   a Keep # rule matches, up to its count, and one of what a Keep rule matches that the pack
--   lacks (the vendor profiles Mag-Tools reads, named after the vendor). It buys no more than the
--   pyreals carried pay for.
--
-- A vendor's own profile is `vendors/<its name>.utl` in the data folder.
--
-- The **vendor trip** goes to the vendor the macro was given (its place, and where the character
-- stood when it was given, kept with the character): from afar by a journey (portals and recalls
-- too) to where the character stood, then on foot, the vendor used from near and the server
-- walking the character the last of the way; it trades, and goes back to where it left by a
-- journey, and the route carries on from its nearest waypoint. It starts on the player's
-- `/tank vendor`, a meta's or a route's `/tank vendor` line, or a full pack (OnFullPack vendor).

local S = require("settings")
local lootlib = require("dereth.loot")

local V = {}

-- The player's integer property of the pyreals it carries.
local COIN_VALUE = 20

-- The pyreals the character carries.
function V.coins()
	return game.player_int(COIN_VALUE) or 0
end

-- The buy list: { name, count } each, as the BuyList setting writes it ("Prismatic Taper=50,
-- Bread=5"). A name may hold a comma ("Trade Note (250,000)=4"): each entry ends at its count.
function V.buy_list()
	local out = {}
	for name, count in string.gmatch(tostring(S.get("BuyList") or ""), "%s*(.-)%s*=%s*(%d+)%s*,?") do
		if name ~= "" then
			table.insert(out, { name = name, count = tonumber(count) })
		end
	end
	return out
end

local function write_list(list)
	local parts = {}
	for _, r in ipairs(list) do
		table.insert(parts, string.format("%s=%d", r.name, r.count))
	end
	S.set("BuyList", table.concat(parts, ", "))
end

-- Buy `name` up to `count` (0 takes it off the list); answers the line to tell.
function V.set_buy(name, count)
	local list, found = {}, false
	for _, r in ipairs(V.buy_list()) do
		if string.lower(r.name) == string.lower(name) then
			found = true
			if count > 0 then
				table.insert(list, { name = r.name, count = count })
			end
		else
			table.insert(list, r)
		end
	end
	if not found and count > 0 then
		table.insert(list, { name = name, count = count })
	end
	write_list(list)
	if count <= 0 then
		return found and ("no longer bought: " .. name) or ("not on the buy list: " .. name)
	end
	return string.format("bought up to %d: %s", count, name)
end

-- How many of `name` the pack holds, stacks counted whole, without case.
function V.held(name)
	local want, n = string.lower(name or ""), 0
	for _, it in ipairs(game.inventory.items()) do
		if string.lower(it.name or "") == want then
			n = n + (it.stack or 1)
		end
	end
	return n
end

-- A vendor's own profile, `vendors/<name>.utl` in the data folder, read once a text; or nil.
local read = {}
function V.vendor_profile(name)
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

-- The names the settings keep for the macro's own use, lower-cased.
local function own_items()
	local out = {}
	for _, s in ipairs({ "Weapon", "CasterWeapon", "Ammo", "Offhand" }) do
		local v = S.get(s)
		if v ~= nil and v ~= "" then
			out[string.lower(v)] = true
		end
	end
	for _, s in ipairs({ "Weapons", "Consumables", "NoBuffItems" }) do
		for _, n in ipairs(S.list(s)) do
			out[string.lower(n)] = true
		end
	end
	return out
end

-- The items to sell: in the pack (not worn, not a pack, not one of the macro's own), matched by a
-- Sell rule of the loot profile or the vendor's profile. Answers their ids and names.
function V.to_sell(profile, vendor_profile)
	local packs, own = {}, own_items()
	for _, p in ipairs(game.inventory.packs()) do
		packs[p.id] = true
	end
	local ids, names = {}, {}
	for _, it in ipairs(game.inventory.items()) do
		local worn = it.wielder ~= nil and it.wielder ~= 0
		if not worn and not packs[it.id] and not own[string.lower(it.name or "")] then
			local hit = profile and profile:classify(it) or nil
			local vhit = vendor_profile and vendor_profile:classify(it) or nil
			if (hit ~= nil and hit.action == "sell") or (vhit ~= nil and vhit.action == "sell") then
				table.insert(ids, it.id)
				table.insert(names, it.name)
			end
		end
	end
	return ids, names
end

-- The rows to buy from the vendor's stock: the buy list's, and the vendor profile's Keep # and
-- Keep rules', each { id, count, name, price }; no more than `coins` pay for, in stock order.
function V.to_buy(stock, vendor_profile, coins)
	local want = {}
	for _, r in ipairs(V.buy_list()) do
		want[string.lower(r.name)] = r.count
	end
	local rows, left = {}, coins
	for _, r in ipairs(stock or {}) do
		local need = 0
		local listed = want[string.lower(r.name or "")]
		if listed ~= nil then
			need = listed - V.held(r.name)
		elseif vendor_profile ~= nil then
			local o = game.world:get(r.id)
			local hit = o and vendor_profile:classify(o) or nil
			if hit ~= nil and hit.action == "keep_up_to" then
				need = (hit.count or 0) - V.held(r.name)
			elseif hit ~= nil and hit.action == "keep" and V.held(r.name) == 0 then
				need = 1
			end
		end
		if r.amount ~= nil and r.amount >= 0 then
			need = math.min(need, r.amount)
		end
		local price = r.price or 0
		if price > 0 then
			need = math.min(need, math.floor(left / price))
		end
		if need > 0 then
			table.insert(rows, { id = r.id, count = need, name = r.name, price = price })
			left = left - need * price
		end
	end
	return rows
end

-- Trade with the open vendor by profile, inside a task: sell, then buy. `note` takes the lines.
-- Answers a summary { sold, bought, before, after }, or nil when no vendor is open.
function V.trade(E, note)
	local info = vendor.info()
	if info == nil then
		return nil
	end
	local shop = game.world:get(info.id)
	local name = shop and shop.name or "the vendor"
	local vp = V.vendor_profile(name)
	local before = V.coins()
	local sold, bought = {}, {}
	if S.get("VendorSell") then
		local ids, names = V.to_sell(E.profile, vp)
		if #ids > 0 then
			local ok, why = pawait(vendor.sell(ids))
			local still = {}
			for _, it in ipairs(game.inventory.items()) do
				still[it.id] = true
			end
			for i, id in ipairs(ids) do
				if not still[id] then
					table.insert(sold, names[i])
				end
			end
			if not ok then
				note("vendor: selling: %s", tostring(why))
			end
		end
	end
	if S.get("VendorBuy") then
		info = vendor.info() or info
		local rows = V.to_buy(info.stock, vp, V.coins())
		if #rows > 0 then
			local held = {}
			for _, r in ipairs(rows) do
				held[r.name] = V.held(r.name)
			end
			local list = {}
			for _, r in ipairs(rows) do
				table.insert(list, { id = r.id, count = r.count })
			end
			local ok, why = pawait(vendor.buy(list))
			for _, r in ipairs(rows) do
				local got = V.held(r.name) - (held[r.name] or 0)
				if got > 0 then
					table.insert(bought, string.format("%d %s", got, r.name))
				end
			end
			if not ok then
				note("vendor: buying: %s", tostring(why))
			end
		end
	end
	local after = V.coins()
	E.tally.sold = (E.tally.sold or 0) + #sold
	E.tally.bought = (E.tally.bought or 0) + #bought
	note("vendor: at %s: sold %d (%s); bought %s; pyreals %d -> %d", name, #sold,
		#sold > 0 and table.concat(sold, ", ") or "nothing", #bought > 0 and table.concat(bought, ", ") or "nothing",
		before, after)
	return { sold = sold, bought = bought, before = before, after = after }
end

-- The vendor the trip goes to: { name, id, place, stand }, kept with the character, or nil:
-- `place` where it stands, `stand` where the character stood when it was set.
function V.vendor()
	local name = S.get("Vendor")
	if name == nil or name == "" then
		return nil
	end
	local kept = (store.character.tank_vendors or {})[name] or {}
	return { name = name, id = kept.id, place = kept.place, stand = kept.stand }
end

local function place_of(p)
	return p and { cell = p.cell, x = p.x, y = p.y, z = p.z } or nil
end

-- The selected vendor becomes the one the trip goes to, its place and where the character stands
-- kept (a place a body stands near it, which the trip's journey goes to); answers the line to
-- tell.
function V.set_vendor(o)
	if o == nil or o.class ~= "Vendor" then
		return "select a vendor first"
	end
	local all = store.character.tank_vendors or {}
	all[o.name] = { id = o.id, place = place_of(o.position), stand = place_of(game.player.position) }
	store.character.tank_vendors = all
	S.set("Vendor", o.name)
	return "the vendor trip goes to " .. tostring(o.name)
end

-- A vendor waypoint's vendor taken for the trip's, its place kept (when none was set).
function V.from_waypoint(p)
	if p == nil or p.kind ~= "vendor" or p.name == nil then
		return nil
	end
	local all = store.character.tank_vendors or {}
	if all[p.name] == nil then
		all[p.name] = { id = p.id, place = p.position }
		store.character.tank_vendors = all
	end
	if S.get("Vendor") == "" then
		S.set("Vendor", p.name)
	end
	return V.vendor()
end

return V
