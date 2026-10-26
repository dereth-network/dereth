-- The pack, kept as VTank keeps it: what the loot profile's Salvage rules took salvaged with the
-- salvaging tool (VTank's auto-salvager), salvage bags combined by the profile's ranges
-- (CombineSalvage), partial stacks merged (AutoStack) and the main pack emptied into the side
-- packs (AutoCram). Each step does one thing.

local S = require("settings")

local P = {}

-- The items taken from corpses to salvage, by id.
P.pending = {}

function P.forget()
	P.pending = {}
end

-- An item's properties that keep it from the salvager, as VTank keeps them [log 0.3.0.13,
-- 0.3.1.37]: tinkered (its tinkering count), or inscribed.
local TINKERED, INSCRIPTION = 171, 7

-- Whether `item` may be salvaged: neither tinkered nor inscribed, not worn.
function P.salvageable(item)
	if item == nil or (item.wielder ~= nil and item.wielder ~= 0) then
		return false
	end
	if (item:int(TINKERED) or 0) > 0 then
		return false
	end
	local text = item:str(INSCRIPTION)
	return text == nil or text == ""
end

-- Take `item` (in the pack now) for salvaging.
function P.take(item)
	P.pending[item.id] = true
end

-- The items waiting to be salvaged that are still in the pack.
function P.waiting()
	local out, held = {}, {}
	if next(P.pending) == nil then
		return out
	end
	for _, it in ipairs(game.inventory.items()) do
		held[it.id] = it
	end
	for id in pairs(P.pending) do
		local it = held[id]
		if it ~= nil and P.salvageable(it) then
			table.insert(out, it)
		else
			P.pending[id] = nil
		end
	end
	table.sort(out, function(a, b)
		return a.id < b.id
	end)
	return out
end

-- Whether the pack holds a salvaging tool.
local function has_ust()
	for _, it in ipairs(game.inventory.items()) do
		if it.class == "Ust" then
			return true
		end
	end
	return false
end

local said_no_ust = false

-- Salvage what waits, all at once. Answers a line when it did something (or could not).
function P.salvage_step()
	local waiting = P.waiting()
	if #waiting == 0 then
		return nil
	end
	if not has_ust() then
		if not said_no_ust then
			said_no_ust = true
			return "salvage: no salvaging tool (an Ust) in the pack; the salvage waits"
		end
		return nil
	end
	said_no_ust = false
	local ids, names = {}, {}
	for _, it in ipairs(waiting) do
		table.insert(ids, it.id)
		table.insert(names, it.name)
	end
	local ok, why = pawait(items.salvage(ids))
	for _, id in ipairs(ids) do
		P.pending[id] = nil
	end
	return ok and string.format("salvaged %s", table.concat(names, ", "))
		or string.format("salvage of %s: %s", table.concat(names, ", "), tostring(why))
end

-- Combining salvage bags --------------------------------------------------------------------------

-- A bag holds at most this many units.
local FULL = 100

-- The profile's ranges for `material`: its own, else the default; nil when it has none.
local function ranges_for(combine, material)
	for _, m in ipairs(combine.materials or {}) do
		if m.material == material then
			return m.ranges
		end
	end
	if #(combine.default or {}) > 0 then
		return combine.default
	end
	return nil
end

-- The range (its index) a bag's workmanship falls in, the workmanship taken as its whole number;
-- nil when none.
local function range_of(ranges, workmanship)
	local w = math.floor(workmanship or 0)
	for i, r in ipairs(ranges) do
		if w >= r[1] and w <= r[2] then
			return i
		end
	end
	return nil
end

-- Two bags of one material and one range of the profile's whose units together fill no more than
-- a bag, or nil.
function P.bags_to_combine(profile)
	if profile == nil then
		return nil
	end
	local combine = profile:salvage_combine()
	if combine == nil then
		return nil
	end
	local groups = {}
	for _, it in ipairs(game.inventory.items()) do
		if it.class == "Salvage" and it.material ~= nil and (it.uses or 0) < FULL then
			local ranges = ranges_for(combine, it.material)
			local r = ranges and range_of(ranges, it.workmanship)
			if r ~= nil then
				local key = it.material .. ":" .. r
				groups[key] = groups[key] or {}
				table.insert(groups[key], it)
			end
		end
	end
	local keys = {}
	for k in pairs(groups) do
		table.insert(keys, k)
	end
	table.sort(keys)
	for _, k in ipairs(keys) do
		local g = groups[k]
		table.sort(g, function(a, b)
			return (a.uses or 0) < (b.uses or 0)
		end)
		for i = 1, #g do
			for j = i + 1, #g do
				if (g[i].uses or 0) + (g[j].uses or 0) <= FULL then
					return g[i], g[j]
				end
			end
		end
	end
	return nil
end

-- Combine two bags the profile's ranges put together, by salvaging them together (which pours
-- them into one). Answers a line when it did.
function P.combine_step(profile)
	if not S.get("CombineSalvage") or not has_ust() then
		return nil
	end
	local a, b = P.bags_to_combine(profile)
	if a == nil then
		return nil
	end
	local ok, why = pawait(items.salvage({ a.id, b.id }))
	return ok and string.format("combined %s (%d) and %s (%d)", a.name, a.uses or 0, b.name, b.uses or 0)
		or string.format("combining %s and %s: %s", a.name, b.name, tostring(why))
end

-- Stacking and cramming ---------------------------------------------------------------------------

-- The packs: the main pack's id first, then the side packs', with each one's free slots.
local function packs()
	local out = {}
	for _, p in ipairs(game.inventory.packs()) do
		table.insert(out, { id = p.id, free = (p.capacity or 0) - (p.used or 0) })
	end
	return out
end

-- The items in the packs, in the order the pack shows them (the main pack, then each side pack),
-- worn items and packs left out.
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
	end
	table.sort(out, function(a, b)
		if a.pack ~= b.pack then
			return a.pack < b.pack
		end
		return a.at < b.at
	end)
	return out
end

-- VTank's AutoStack [adv, log 0.3.1.157]: stacks are merged toward the bottom-most stack of their
-- kind, the main pack's after the side packs' order, so the one partial stack left is the
-- top-most (which the game uses first). Items of one name that do not stack are left. One merge a
-- step. Answers a line when it merged.
function P.stack_step()
	local kinds, order = {}, {}
	for _, e in ipairs(in_packs()) do
		local it = e.item
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
		local list = kinds[k]
		-- The bottom-most stack with room, and the top-most stack above it.
		for to = #list, 2, -1 do
			local into = list[to]
			local room = (into.max_stack or 1) - (into.stack or 1)
			if room > 0 then
				local from = list[1]
				if from.id ~= into.id then
					local n = math.min(room, from.stack or 1)
					local ok, why = pawait(items.merge(from.id, into.id, n))
					return ok and string.format("stacked %d %s", n, k) or string.format("stacking %s: %s", k, tostring(why))
				end
			end
		end
	end
	return nil
end

-- VTank's AutoCram [std, adv]: an item of the main pack moved into the first side pack with room.
-- One move a step. Answers a line when it moved one.
function P.cram_step()
	local ps = packs()
	if #ps < 2 then
		return nil
	end
	local into = nil
	for i = 2, #ps do
		if ps[i].free > 0 then
			into = ps[i]
			break
		end
	end
	if into == nil then
		return nil
	end
	for _, e in ipairs(in_packs()) do
		if e.pack == 1 and e.item.class ~= "Container" then
			local ok, why = pawait(items.move(e.item.id, into.id))
			return ok and string.format("crammed %s into a side pack", e.item.name)
				or string.format("cramming %s: %s", e.item.name, tostring(why))
		end
	end
	return nil
end

return P
