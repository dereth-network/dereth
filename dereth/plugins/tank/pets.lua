-- Summoning, as VTank summons (its SummonPets and pet options, its monster list's PetDmg column):
-- with SummonPets on and no pet of the character's out, once PetMonsterDensity monsters that their
-- rows summon for (PetDmg not none, priority 0 or more) are within the pet range (PetRangeMode 0:
-- AttackDistance; 1: PetCustomRange), a summoning device is used: one the Pets setting names (VTank's
-- Items tab; empty: any in the pack), of the character's summoning mastery, its skill enough for it,
-- with charges left; its element chosen by the row of the highest-priority monster in range:
-- `pauto` (VTank's PAuto, the default) the element the character fights it with, else the first of
-- the auto list it has a device of; `auto` that first; or an element. How VTank refills a device's
-- charges (PetRefillCount-Idle, -Normal) is not published, so it is not carried.

local S = require("settings")
local monsters = require("monsters")
local spells = require("spells")

local P = {}

-- The macro (set by the engine).
P.E = nil

-- The property numbers a device's appraisal tells: the mastery it needs, the skill and level of
-- it, and its icon effects (its element).
local MASTERY, NEEDS_SKILL_LEVEL, EFFECTS = 362, 367, 0x0D000010
-- The character's own mastery.
local PLAYER_MASTERY = 362
-- The elements, by a device's effect bits.
local ELEMENT_BITS = { fire = 0x20, lightning = 0x40, cold = 0x80, acid = 0x100, blunt = 0x200, slash = 0x400,
	pierce = 0x800, nether = 0x1000 }

-- How long after a summon (or a try) before another is tried, seconds.
local AGAIN, AFTER_FAILURE = 10, 30
local tried_at, wait = nil, 0

function P.forget()
	tried_at, wait = nil, 0
end

-- The pet range, metres.
function P.range()
	if S.get("PetRangeMode") == 1 then
		return S.metres("PetCustomRange")
	end
	return S.metres("AttackDistance")
end

-- A device's element, from its effects, or nil.
function P.element(it)
	local fx = it:int(EFFECTS) or 0
	for name, bit in pairs(ELEMENT_BITS) do
		if bit32.band(fx, bit) ~= 0 then
			return name
		end
	end
	return nil
end

-- Whether an item is a summoning device (an essence).
local function is_device(it)
	return it.wielder == nil and string.find(string.lower(it.name or ""), "essence", 1, true) ~= nil
		and (#S.list("Pets") == 0 or S.listed("Pets", it.name))
end

-- The character's summoning skill, or nil.
local function summoning()
	for _, sk in ipairs(game.player.skills or {}) do
		if sk.name == "Summoning" then
			return sk.current, sk.training
		end
	end
	return nil
end

-- The devices the character may summon from now: of its mastery, its skill enough, with charges
-- left. Each is appraised once for what it needs.
function P.devices()
	local out = {}
	local mine = game.player_int(PLAYER_MASTERY)
	local skill = summoning()
	for _, it in ipairs(game.inventory.items()) do
		if is_device(it) then
			if not it.appraised then
				pawait(act.appraise(it.id))
			end
			local needs = it:int(MASTERY)
			local level = it:int(NEEDS_SKILL_LEVEL)
			local charged = it.uses == nil or it.uses > 0
			if charged and (needs == nil or needs == 0 or mine == nil or needs == mine)
				and (level == nil or skill == nil or skill >= level) then
				table.insert(out, it)
			end
		end
	end
	return out
end

-- Whether a pet of the character's is out ("<name>'s ..." near it).
function P.pet_out()
	local me = game.player.name
	if me == nil then
		return false
	end
	local prefix = string.lower(me .. "'s ")
	for _, o in ipairs(game.world:find({ within = 60 })) do
		if string.sub(string.lower(o.name or ""), 1, #prefix) == prefix and o.class ~= "Corpse" then
			return true
		end
	end
	return false
end

-- The monsters in pet range that their rows summon for, the highest priority first: each { o, row }.
function P.targets(meta_state)
	local out = {}
	for _, o in ipairs(game.world:find({ monster = true, within = P.range(), sort = "distance" })) do
		local row = monsters.match(o, meta_state)
		if (row.priority or 1) >= 0 and (row.petdmg or "pauto") ~= "none" then
			table.insert(out, { o = o, row = row })
		end
	end
	table.sort(out, function(a, b)
		return (a.row.priority or 1) > (b.row.priority or 1)
	end)
	return out
end

-- The device to summon for a monster's row: its PetDmg's element (pauto: the element the
-- character fights it with), else the first of the auto list a device has.
function P.choose(devices, row, damage)
	local by = {}
	for _, it in ipairs(devices) do
		local e = P.element(it) or "none"
		by[e] = by[e] or it
	end
	local want = row.petdmg or "pauto"
	if want == "pauto" then
		if damage ~= nil and by[damage] ~= nil then
			return by[damage], damage
		end
		want = "auto"
	end
	if want == "auto" then
		for _, t in ipairs(spells.auto_order()) do
			if by[t] ~= nil then
				return by[t], t
			end
		end
		local any = devices[1]
		return any, any and P.element(any)
	end
	return by[want], want
end

-- Why no pet would be summoned now, or nil and the device and element that would be (`damage`, the
-- element the macro fights the first monster with). VTank's /vt testpet.
function P.test(meta_state, damage_of)
	if not S.get("SummonPets") then
		return "SummonPets is off"
	end
	if P.pet_out() then
		return "a pet of yours is out"
	end
	local targets = P.targets(meta_state)
	if #targets < math.max(1, S.get("PetMonsterDensity")) then
		return string.format("%d monsters to summon for within %d m, %d wanted", #targets, math.floor(P.range()),
			S.get("PetMonsterDensity"))
	end
	local devices = P.devices()
	if #devices == 0 then
		return "no summoning device of your mastery, with charges, that your skill uses"
	end
	local first = targets[1]
	local it, element = P.choose(devices, first.row, damage_of and damage_of(first.o, first.row) or nil)
	if it == nil then
		return "no device of the element " .. tostring(first.row.petdmg) .. " for " .. tostring(first.o.name)
	end
	return nil, it, element, first.o
end

-- One summon, when one is wanted. Answers true when it used a device.
function P.step(meta_state, damage_of)
	if not S.get("SummonPets") or os.clock() < wait then
		return false
	end
	if tried_at ~= nil and os.clock() - tried_at < AGAIN then
		return false
	end
	local E = P.E
	local why, it, element, monster = P.test(meta_state, damage_of)
	if why ~= nil then
		return false
	end
	tried_at = os.clock()
	E.set_state("Summoning")
	local ok, err = pawait(act.use(it.id))
	E.note(ok and "summoning from %s (%s) for %s" or "summoning from %s (%s) for %s: %s", it.name, tostring(element),
		monster.name, tostring(err))
	if ok then
		E.tally.summons = (E.tally.summons or 0) + 1
	else
		wait = os.clock() + AFTER_FAILURE
	end
	return true
end

return P
