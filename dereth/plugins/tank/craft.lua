-- Missile ammunition and healing kits made from what the pack holds, as VTank makes them.
--
-- Ammunition: a bundle of arrowheads used on a bundle of shafts makes ammunition for the launcher
-- the shafts are for: arrowshafts for a bow, quarrelshafts for a crossbow, atlatl dart shafts for an
-- atlatl. "Bundle of Deadly Fire Arrowheads" goes on "Bundle of Arrowshafts" (100 arrows) and
-- "Wrapped Bundle of …" on "Wrapped Bundle of …" (1000), never one on the other; infinite arrowheads
-- on either. The ammunition is named for the heads and launcher, Broad heads making "Broadhead"
-- ammunition and some quarrels named "… Bolt". Where the world answers what the
-- pack's items make (`craft.recipes`), its answer decides: each combine's product and the
-- character's chance, one under CraftMinChance left alone. Elsewhere the heads are paired with
-- shafts by their names, and ranked by their words (Prismatic above all others, then Deadly above
-- Greater above plain, an element above none). Either way the pack says how each combine went. A
-- combine that failed (the bundles gone, nothing made) is not tried again this run.
--
-- Special ammunition, by VTank's UseSpecialAmmo: 1 Raider Lightning, 2 the three made from the
-- special currencies (Burning Sands, Gear Blade Slashing, Olthoi Acid), 3 both, 0 neither, made
-- nor used, unless the Ammo setting names it.
--
-- Healing kits: Treated Hyssop on Treated Mandrake makes Combined Hyssop and Mandrake, which on
-- Soft Bandages makes a Plentiful Healing Kit. Made with AutoCraftItems when the Consumables list
-- a Plentiful Healing Kit and none is carried, and up to IdleCraftCount_HealthKits when idle.
--
-- The chance of a combine, where something tells it: the world's answer (`craft.recipes`); else,
-- with the character's craft dialog on, the server asks before each combine, giving the chance, and
-- the Tank answers Yes at CraftMinChance or better and No below (a combine it said No to is not
-- tried again this run). The Tank never changes the option. With neither, each tier is made only at
-- its least buffed skill (C.DEFAULT_SKILLS, the player's own in CraftSkills).

local S = require("settings")

local C = {}

-- The ammunition type a launcher shoots, or an item of ammunition is (its public description's
-- int 50): an arrow, a bolt or a dart, each with its crystal and chorizite kinds.
local AMMO_TYPE = 50
local BY_TYPE = {
	[1] = "arrow", [8] = "arrow", [64] = "arrow",
	[2] = "bolt", [16] = "bolt", [128] = "bolt",
	[4] = "dart", [32] = "dart", [256] = "dart",
}

-- Each kind's shafts, by their names without case, spaces or hyphens, and whether wrapped; and
-- what its finished ammunition's name ends with.
local SHAFTS = {
	arrow = { bundleofarrowshafts = false, wrappedbundleofarrowshafts = true },
	bolt = { bundleofquarrelshafts = false, wrappedbundleofquarrelshafts = true },
	dart = { bundleofatlatldartshafts = false, wrappedbundleofatlatldartshafts = true },
}
local ENDINGS = { arrow = { "arrow" }, bolt = { "quarrel", "bolt" }, dart = { "atlatldart" } }

-- Special ammunition, by the bit of UseSpecialAmmo that allows it.
local SPECIAL = { ["raider lightning"] = 1, ["burning sands"] = 2, ["gear blade slashing"] = 2, ["olthoi acid"] = 2 }

-- The healing kit's two combines: what is used on what, and what it makes.
C.KIT = "Plentiful Healing Kit"
local KIT_STEPS = {
	{ use = "Combined Hyssop and Mandrake", on = "Soft Bandages", makes = C.KIT },
	{ use = "Treated Hyssop", on = "Treated Mandrake", makes = "Combined Hyssop and Mandrake" },
}

-- The least buffed skill each tier is made at when nothing tells the chance (no answer from the
-- world, the craft dialog off): where its skill check reaches 90%. Ammunition by its heads' words
-- ("Plain" for plain heads), " (wrapped)" for a wrapped bundle, the same on every launcher; the
-- healing kit's two combines by what they make. A check of skill against a recipe's difficulty
-- succeeds with chance 1 - 1 / (1 + e^(0.03 (skill - difficulty))), 90% from 74 above it, so each
-- tier's skill is its difficulty plus REACH. The Tank guide names the sources. A tier with none is
-- made only when the chance is told, or at the skill the player sets for it (CraftSkills).
local REACH = 74

-- Recipe difficulties the retail game showed: the skill check's difficulty it reported after a
-- combine of each.
local RETAIL_DIFFICULTY = {
	["Plain"] = 5,
	["Blunt"] = 10,
	["Broad"] = 10,
	["Blunt (wrapped)"] = 40,
	["Armor Piercing (wrapped)"] = 50,
	["Combined Hyssop and Mandrake"] = 100,
	["Greater Blunt (wrapped)"] = 170,
	["Greater Broad (wrapped)"] = 170,
	["Deadly Blunt (wrapped)"] = 180,
	["Deadly Armor Piercing (wrapped)"] = 200,
	["Deadly Frog Crotch (wrapped)"] = 200,
	["Plentiful Healing Kit"] = 200,
	["Deadly Acid (wrapped)"] = 250,
	["Deadly Fire (wrapped)"] = 250,
	["Deadly Lightning (wrapped)"] = 250,
	["Prismatic"] = 250,
	["Greater Deadly"] = 275,
	["Greater Deadly Blunt"] = 275,
	["Greater Deadly Broad"] = 275,
	["Prismatic (wrapped)"] = 275,
	["Burning Sands (wrapped)"] = 300,
	["Greater Deadly Armor Piercing"] = 300,
	["Greater Deadly Blunt (wrapped)"] = 300,
	["Greater Deadly Frog Crotch"] = 300,
	["Olthoi Acid (wrapped)"] = 300,
	["Greater Deadly Armor Piercing (wrapped)"] = 325,
	["Greater Deadly Frog Crotch (wrapped)"] = 325,
	["Greater Prismatic"] = 350,
	["Deadly Prismatic"] = 375,
	["Greater Prismatic (wrapped)"] = 375,
	["Deadly Prismatic (wrapped)"] = 400,
}

-- The rest, from the community wiki's fletching recipes.
local PUBLIC_DIFFICULTY = {
	["Armor Piercing"] = 20,
	["Frog Crotch"] = 20,
	["Acid"] = 60,
	["Fire"] = 60,
	["Frost"] = 60,
	["Lightning"] = 60,
	["Lightning (wrapped)"] = 90,
	["Greater"] = 120,
	["Greater Blunt"] = 140,
	["Greater Broad"] = 140,
	["Deadly"] = 150,
	["Deadly Blunt"] = 150,
	["Deadly Broad"] = 150,
	["Greater (wrapped)"] = 150,
	["Greater Armor Piercing"] = 160,
	["Greater Frog Crotch"] = 160,
	["Deadly (wrapped)"] = 175,
	["Deadly Armor Piercing"] = 175,
	["Deadly Frog Crotch"] = 175,
	["Greater Deadly (wrapped)"] = 175,
	["Deadly Broad (wrapped)"] = 180,
	["Greater Deadly Broad (wrapped)"] = 180,
	["Greater Frog Crotch (wrapped)"] = 190,
	["Greater Acid"] = 200,
	["Greater Armor Piercing (wrapped)"] = 200,
	["Greater Fire"] = 200,
	["Greater Frost"] = 200,
	["Greater Lightning"] = 200,
	["Deadly Acid"] = 225,
	["Deadly Fire"] = 225,
	["Deadly Frost"] = 225,
	["Deadly Lightning"] = 225,
	["Greater Acid (wrapped)"] = 230,
	["Greater Fire (wrapped)"] = 230,
	["Greater Frost (wrapped)"] = 230,
	["Greater Lightning (wrapped)"] = 230,
}

C.DEFAULT_SKILLS = {}
for _, t in ipairs({ PUBLIC_DIFFICULTY, RETAIL_DIFFICULTY }) do
	for tier, difficulty in pairs(t) do
		C.DEFAULT_SKILLS[tier] = difficulty + REACH
	end
end

-- How long a combine is waited on before it counts as unanswered, and how long the pack is
-- watched after its bundles changed for what they made.
local ANSWER_SECONDS = 15
local SETTLE_SECONDS = 1

-- The combines that failed this run, by their heads' and shafts' names; and the lines already
-- said, so each is said once.
C.failed = {}
local said = {}
-- The combine under way (its two items' ids), and whether its craft question was answered No.
local current, declined = nil, false

local function plain(s)
	return (string.gsub(string.lower(s or ""), "[^%w]", ""))
end

-- A line for the Tank's log, said as it happens (the engine sets C.note).
local function note(line)
	if C.note ~= nil then
		C.note("%s", line)
	end
end

local function once(line)
	if said[line] then
		return nil
	end
	said[line] = true
	return line
end

function C.forget()
	C.failed = {}
	said = {}
	C.arrived = false
end

-- The least chance a combine is made at, 0 to 1.
local function min_chance()
	return (S.get("CraftMinChance") or 90) / 100
end

-- The server's craft question about the combine under way: Yes at CraftMinChance or better, No
-- below (a question that gives no chance is answered Yes).
on("craft.confirm", function(e)
	local yes = e.chance == nil or e.chance >= min_chance()
	if current ~= nil and e.item == current[1] and e.target == current[2] and not yes then
		declined = true
	end
	task.spawn(function()
		pawait(act.confirm(e.question, yes))
	end)
	if not yes then
		note(string.format("declined a combine at a %d%% chance, under CraftMinChance", math.floor(e.chance * 100 + 0.5)))
	end
end)

-- The heads' own words ("Deadly Fire"), whether they are wrapped (nil: either), or nil for an item
-- that is not a bundle of arrowheads.
function C.heads(name)
	name = name or ""
	local k = string.match(name, "^Wrapped Bundle of (.-)%s*Arrowheads$")
	if k ~= nil then
		return k, true
	end
	k = string.match(name, "^Bundle of (.-)%s*Arrowheads$")
	if k ~= nil then
		return k, false
	end
	k = string.match(name, "^Infinite (.-)%s*Arrowheads$")
	if k ~= nil then
		return k, nil
	end
	return nil
end

-- Whether an item named `name` is a bundle of shafts, of any launcher's.
local function is_shafts(name)
	local n = plain(name)
	for _, kinds in pairs(SHAFTS) do
		if kinds[n] ~= nil then
			return true
		end
	end
	return false
end

-- Whether heads or shafts have come into the pack since the engine last looked (it clears this
-- when it does): what they make is a combine the pack did not hold before.
C.arrived = false
on("inventory.item_added", function(e)
	local it = game.world:get(e.id)
	if it ~= nil and (C.heads(it.name) ~= nil or is_shafts(it.name)) then
		C.arrived = true
	end
end)

-- What a name's words make of its quality: Prismatic first, then Deadly, Greater and an element.
function C.rank(name)
	local n = " " .. string.lower(name or "") .. " "
	local r = 0
	if n:find("prismatic", 1, true) then
		r = r + 100
	end
	if n:find(" deadly ", 1, true) then
		r = r + 4
	end
	if n:find(" greater ", 1, true) then
		r = r + 2
	end
	for _, e in ipairs({ "fire", "frost", "lightning", "acid" }) do
		if n:find(" " .. e .. " ", 1, true) then
			r = r + 1
			break
		end
	end
	return r
end

-- The bit of UseSpecialAmmo a name's ammunition needs, or nil for ammunition that is not special.
local function special_bit(name)
	local n = string.lower(name or "")
	for words, bit in pairs(SPECIAL) do
		if n:find(words, 1, true) then
			return bit
		end
	end
	return nil
end

local function special_allowed(name)
	local bit = special_bit(name)
	return bit == nil or (math.floor((S.get("UseSpecialAmmo") or 0) / bit) % 2) == 1
end

-- The kind of ammunition a launcher shoots: by its ammunition type, else by its name.
function C.launcher_kind(it)
	local t = it.int and it:int(AMMO_TYPE)
	if t ~= nil and BY_TYPE[t] ~= nil then
		return BY_TYPE[t]
	end
	local n = string.lower(it.name or "")
	if n:find("crossbow", 1, true) then
		return "bolt"
	elseif n:find("atlatl", 1, true) then
		return "dart"
	end
	return "arrow"
end

-- The kind of finished ammunition an item is, or nil (a bundle of heads or shafts is none).
function C.ammo_kind(it)
	if it.class ~= "MissileWeapon" then
		return nil
	end
	local n = plain(it.name)
	if n:find("arrowhead", 1, true) or n:find("shaft", 1, true) or n:find("bundle", 1, true) then
		return nil
	end
	local t = it.int and it:int(AMMO_TYPE)
	if t ~= nil and BY_TYPE[t] ~= nil then
		return BY_TYPE[t]
	end
	if n:find("quarrel", 1, true) or n:find("bolt", 1, true) then
		return "bolt"
	elseif n:find("dart", 1, true) then
		return "dart"
	elseif n:find("arrow", 1, true) then
		return "arrow"
	end
	return nil
end

-- Whether heads of these words make ammunition named `want` for `kind`: the name is the words, then
-- perhaps "head" or "tip" (Broadhead, Crystal-tip), then the kind's ending.
function C.makes(words, kind, want)
	local w, n = plain(want), plain(words)
	if string.sub(w, 1, #n) ~= n then
		return false
	end
	local rest = string.sub(w, #n + 1)
	rest = string.gsub(rest, "^head", "")
	rest = string.gsub(rest, "^tip", "")
	for _, e in ipairs(ENDINGS[kind]) do
		if rest == e then
			return true
		end
	end
	return false
end

-- The loose ammunition of `kind` the pack holds that may be readied (the one `want` names when it
-- names one), best first.
function C.loose(kind, want)
	local me = game.player.id
	local out = {}
	for _, it in ipairs(game.inventory.items()) do
		if it.wielder ~= me and C.ammo_kind(it) == kind then
			local named = want ~= nil and want ~= ""
			if (named and string.lower(it.name or "") == string.lower(want))
				or (not named and special_allowed(it.name)) then
				table.insert(out, it)
			end
		end
	end
	table.sort(out, function(a, b)
		local ra, rb = C.rank(a.name), C.rank(b.name)
		if ra ~= rb then
			return ra > rb
		end
		return a.id < b.id
	end)
	return out
end

-- How much loose ammunition of `kind` the pack holds, counting every stack.
local function loose_count(kind)
	local n = 0
	for _, it in ipairs(game.inventory.items()) do
		if C.ammo_kind(it) == kind then
			n = n + (it.stack or 1)
		end
	end
	return n
end

-- The combines the pack holds for `kind` (the heads of `want`'s ammunition, when it names one),
-- best first: each `{ heads, shafts, rank, words }`, its heads and shafts the items to combine.
function C.combines(kind, want)
	local shafts = {}
	for _, it in ipairs(game.inventory.items()) do
		local wrapped = SHAFTS[kind][plain(it.name)]
		if wrapped ~= nil and shafts[wrapped] == nil then
			shafts[wrapped] = it
		end
	end
	local out = {}
	for _, it in ipairs(game.inventory.items()) do
		local words, wrapped = C.heads(it.name)
		local named = want ~= nil and want ~= ""
		if words ~= nil and ((named and C.makes(words, kind, want)) or (not named and special_allowed(words))) then
			-- Wrapped heads go on wrapped shafts, the others on the others; infinite ones on either,
			-- wrapped first (they make more).
			local on = wrapped == nil and (shafts[true] or shafts[false]) or shafts[wrapped]
			if on ~= nil and not C.failed[it.name .. "|" .. on.name] then
				table.insert(out, { heads = it, shafts = on, rank = C.rank(words) + (special_bit(words) and 200 or 0), words = words, tier = C.tier(it.name) })
			end
		end
	end
	table.sort(out, function(a, b)
		if a.rank ~= b.rank then
			return a.rank > b.rank
		end
		return a.heads.id < b.heads.id
	end)
	return out
end

-- The tier a bundle of heads is: its words ("Plain" for none), " (wrapped)" when wrapped.
function C.tier(name)
	local words, wrapped = C.heads(name)
	if words == nil then
		return nil
	end
	words = words == "" and "Plain" or words
	return wrapped and (words .. " (wrapped)") or words
end

-- The player's own skills for tiers (CraftSkills: "Deadly Fire = 320, Plentiful Healing Kit =
-- 260"), by tier without case.
local function own_skills()
	local out = {}
	for part in string.gmatch(tostring(S.get("CraftSkills") or ""), "[^,]+") do
		local k, v = string.match(part, "^%s*(.-)%s*=%s*(%d+)%s*$")
		if k ~= nil and k ~= "" then
			out[string.lower(k)] = tonumber(v)
		end
	end
	return out
end

-- The least buffed skill `tier` is made at: the player's own, else the default; nil for none.
function C.skill_for(tier)
	local own = own_skills()[string.lower(tier)]
	if own ~= nil then
		return own, true
	end
	for k, v in pairs(C.DEFAULT_SKILLS) do
		if string.lower(k) == string.lower(tier) then
			return v, false
		end
	end
	return nil, false
end

-- `tier`'s skill set by the player, or put back to its default (`skill` nil).
function C.set_skill(tier, skill)
	local own = own_skills()
	own[string.lower(tier)] = skill
	local keep = {}
	-- The tier's own spelling, where the defaults have it.
	local spelled = {}
	for k in pairs(C.DEFAULT_SKILLS) do
		spelled[string.lower(k)] = k
	end
	spelled[string.lower(C.KIT)] = C.KIT
	for _, step in ipairs(KIT_STEPS) do
		spelled[string.lower(step.makes)] = step.makes
	end
	for k, v in pairs(own) do
		table.insert(keep, (spelled[k] or k) .. "=" .. v)
	end
	table.sort(keep)
	return S.set("CraftSkills", table.concat(keep, ", "))
end

-- Every tier with its skill and whether the player set it, for the Items page: the defaults,
-- the healing kit's combines, and any the player named.
function C.skill_rows()
	local seen, rows = {}, {}
	local function add(tier)
		if seen[string.lower(tier)] then
			return
		end
		seen[string.lower(tier)] = true
		local skill, own = C.skill_for(tier)
		table.insert(rows, { tier = tier, skill = skill, own = own })
	end
	for k in pairs(C.DEFAULT_SKILLS) do
		add(k)
	end
	for _, step in ipairs(KIT_STEPS) do
		add(step.makes)
	end
	for k in pairs(own_skills()) do
		add(k)
	end
	table.sort(rows, function(a, b)
		local x, y = a.skill or math.huge, b.skill or math.huge
		if x ~= y then
			return x < y
		end
		return a.tier < b.tier
	end)
	return rows
end

-- Whether the server tells the chance before a combine (the character's craft dialog is on).
local function chance_told()
	return (game.player.options or {}).use_craft_success_dialog == true
end

-- The skill `name` with its buffs, and whether it is untrained; nil when the host has no skills.
local function buffed(name)
	for _, sk in ipairs(game.player.skills or {}) do
		if sk.name == name then
			return sk.current, sk.training == "untrained"
		end
	end
	return nil
end

-- Whether a combine of `tier` may be made with `skill` when nothing tells the chance: at least
-- the tier's skill; a tier with none is said once and left.
local function at_skill(tier, skill)
	local need = C.skill_for(tier)
	local have, untrained = buffed(skill)
	-- No skills to read, or the skill untrained: the trained check says why, as for any combine.
	if have == nil or untrained then
		return true
	end
	if need == nil then
		local line = once(string.format("no %s skill is set for %s: set one on the Items page, or turn the craft dialog on", skill, tier))
		if line ~= nil then
			note(line)
		end
		return false
	end
	return have >= need
end

-- The skill `name`'s training, or nil when the host has no skills.
local function training(name)
	local skills = game.player.skills
	if skills == nil or #skills == 0 then
		return nil
	end
	for _, sk in ipairs(skills) do
		if sk.name == name then
			return sk.training
		end
	end
	return "untrained"
end

-- Whether combines with `skill` may be made now; else the line saying why (once), or nil.
function C.can(skill)
	if training(skill) == "untrained" then
		return false, once(string.format("crafting needs %s trained", skill))
	end
	return true
end

local function count_named(name)
	local n = 0
	for _, it in ipairs(game.inventory.items()) do
		if it.name == name and it.wielder ~= game.player.id then
			n = n + (it.stack or 1)
		end
	end
	return n
end

local function stack_of(id)
	local o = game.world:get(id)
	return o ~= nil and (o.stack or 1) or 0
end

-- Use `use` on `on`, in peace mode, and watch the pack: answers "made" when `made()` says the
-- product came, "failed" when the two items changed and nothing came, "unanswered" otherwise.
local function combine(use, on, made)
	if (game.player.combat_mode or 1) > 1 then
		pawait(combat.set_mode("peace"))
	end
	local before_use, before_on = stack_of(use.id), stack_of(on.id)
	current, declined = { use.id, on.id }, false
	local ok, why = pawait(act.use_on(use.id, on.id))
	if not ok then
		current = nil
		return "unanswered", tostring(why)
	end
	local until_t = os.clock() + ANSWER_SECONDS
	local changed_at = nil
	while os.clock() < until_t do
		if made() then
			current = nil
			return "made"
		end
		if declined then
			current = nil
			return "declined"
		end
		local changed = stack_of(use.id) < before_use or stack_of(on.id) < before_on
		if changed and changed_at == nil then
			changed_at = os.clock()
		end
		if changed_at ~= nil and os.clock() - changed_at > SETTLE_SECONDS then
			current = nil
			return made() and "made" or "failed"
		end
		task.sleep(250)
	end
	current = nil
	return "unanswered", "no answer"
end

-- Ammunition of `kind` made from the best combine the pack holds that outranks `best` (the best
-- loose ammunition carried, or nil); `fighting` says a fight is on, when carried ammunition is
-- readied at once instead. Answers whether a combine made some, and a line for the log, or nil.
-- The world's answer to what `items` make, or nil where it gives none (no capability, or a world
-- that offered no recipe answers).
local function recipes_for(items)
	if #items == 0 then
		return nil
	end
	local ok, p = pcall(function()
		return craft.recipes(items)
	end)
	if not ok then
		return nil
	end
	local done, answer = pawait(p)
	if not done then
		return nil
	end
	return answer
end

-- The combines the world's recipes answer for `kind` (the ammunition `want` names, when it names
-- one), best first, each `{ heads, shafts, rank, words, chance }`; nil where the world gives no
-- recipe answers. Those under CraftMinChance are left out.
function C.answered_combines(kind, want)
	local items, by_id = {}, {}
	for _, it in ipairs(game.inventory.items()) do
		if it.wielder ~= game.player.id and (C.heads(it.name) ~= nil or SHAFTS[kind][plain(it.name)] ~= nil) then
			if #items < 64 then
				table.insert(items, it.id)
				by_id[it.id] = it
			end
		end
	end
	local answer = recipes_for(items)
	if answer == nil then
		return nil
	end
	local out = {}
	for _, m in ipairs(answer) do
		local heads, shafts = by_id[m.item], by_id[m.target]
		local name = m.makes and m.makes.name or ""
		local named = want ~= nil and want ~= ""
		local fits = C.ammo_kind({ class = "MissileWeapon", name = name }) == kind
		if heads ~= nil and shafts ~= nil and fits and m.chance >= min_chance()
			and ((named and string.lower(name) == string.lower(want)) or (not named and special_allowed(name)))
			and not C.failed[heads.name .. "|" .. shafts.name] then
			table.insert(out, {
				heads = heads,
				shafts = shafts,
				rank = C.rank(name) + (special_bit(name) and 200 or 0),
				words = name,
				chance = m.chance,
			})
		end
	end
	table.sort(out, function(a, b)
		if a.rank ~= b.rank then
			return a.rank > b.rank
		end
		return a.heads.id < b.heads.id
	end)
	return out
end

function C.make_ammo(kind, want, best, fighting)
	if not S.get("CraftAmmo") then
		return false, nil
	end
	local by_world = C.answered_combines(kind, want)
	local function list()
		if by_world ~= nil then
			return C.answered_combines(kind, want)
		end
		local named = C.combines(kind, want)
		-- Nothing tells the chance: each tier at its skill.
		if chance_told() then
			return named
		end
		local out = {}
		for _, c in ipairs(named) do
			if at_skill(c.tier, "Fletching") then
				table.insert(out, c)
			end
		end
		return out
	end
	local all = by_world or list()
	local top = all[1]
	if top == nil then
		return false, nil
	end
	if best ~= nil and (top.rank <= C.rank(best.name) or fighting) then
		return false, nil
	end
	local ok, why = C.can("Fletching")
	if not ok then
		return false, why
	end
	-- A few tries at most, the pack read again for each: a failed combine used up its bundles.
	for _ = 1, 3 do
		local c = all[1]
		if c == nil then
			break
		end
		-- Read now: an item the combine uses up tells nothing once it is gone.
		local heads, shafts = c.heads.name, c.shafts.name
		local before = loose_count(kind)
		local outcome, err = combine(c.heads, c.shafts, function()
			return loose_count(kind) > before
		end)
		if outcome == "made" then
			return true, string.format("made ammunition from %s", heads)
		elseif outcome == "failed" then
			C.failed[heads .. "|" .. shafts] = true
			note(string.format("the combine of %s and %s failed; it is not tried again this run", heads, shafts))
			-- With ammunition carried, the failure leaves it to be readied; with none, the next
			-- combine is tried.
			if best ~= nil then
				return false, nil
			end
			all = list()
		elseif outcome == "declined" then
			C.failed[heads .. "|" .. shafts] = true
			if best ~= nil then
				return false, nil
			end
			all = list()
		else
			return false, string.format("crafting %s: %s", heads, err or "no answer")
		end
	end
	return false, "no combine made any ammunition"
end

-- How many healing kits are wanted made now (VTank's AutoCraftItems and
-- IdleCraftCount_HealthKits), 0 for none.
function C.kits_wanted(idle)
	local held = count_named(C.KIT)
	local want = 0
	if S.get("AutoCraftItems") and held == 0 and S.listed("Consumables", C.KIT) then
		want = 1
	end
	if idle then
		want = math.max(want, (S.get("IdleCraftCount_HealthKits") or 0) - held)
	end
	return want
end

-- One combine toward a healing kit, when one is wanted and the pack holds what it takes. Answers
-- whether it did one, and a line for the log, or nil.
function C.kit_step(idle)
	if C.kits_wanted(idle) <= 0 then
		return false, nil
	end
	-- A combine left for want of skill, so nothing more is said.
	local short = false
	for _, step in ipairs(KIT_STEPS) do
		local use, on = nil, nil
		for _, it in ipairs(game.inventory.items()) do
			if it.wielder ~= game.player.id then
				if use == nil and it.name == step.use then
					use = it
				elseif on == nil and it.name == step.on then
					on = it
				end
			end
		end
		-- Nothing tells the chance: the combine at its Healing skill.
		local skilled = chance_told() or (use ~= nil and on ~= nil and at_skill(step.makes, "Healing"))
		short = short or (use ~= nil and on ~= nil and not skilled)
		if use ~= nil and on ~= nil and not C.failed[step.use .. "|" .. step.on] and skilled then
			local ok, why = C.can("Healing")
			if not ok then
				return false, why
			end
			local before = count_named(step.makes)
			local outcome, err = combine(use, on, function()
				return count_named(step.makes) > before
			end)
			if outcome == "made" then
				return true, "made " .. step.makes
			elseif outcome == "failed" then
				C.failed[step.use .. "|" .. step.on] = true
				return true, string.format("the combine of %s and %s failed; it is not tried again this run", step.use, step.on)
			elseif outcome == "declined" then
				C.failed[step.use .. "|" .. step.on] = true
				return true, nil
			end
			return false, string.format("crafting %s: %s", step.makes, err or "no answer")
		end
	end
	if short then
		return false, nil
	end
	return false, once("a healing kit is wanted, but the pack holds nothing to make one from")
end

return C
