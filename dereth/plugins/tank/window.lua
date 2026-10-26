-- The Tank's window, in the manner of VTank's: tabs for the run, combat, the monster list, the
-- items (the combat items and the consumables), navigation, the route, looting, vitals, buffing,
-- the vendor, the meta (its rules edited in the game), and the settings (when to stop, the
-- hotkeys, then every setting by name), each a control; and VTank's remote control, a small bar on
-- the screen. Everything a /tank command changes shows here too. Distances show in meters (the
-- settings keep VTank's map units of 240 m).

local S = require("settings")
local monsters = require("monsters")
local spells = require("spells")
local routes = require("routes")
local metas = require("metas")
local vendors = require("vendor")
local metaedit = require("metaedit")
local hotkeys = require("hotkeys")
local buffs = require("buffs")

local W = {}

-- The route recorder: on, and where the last point it added stands.
W.recording = false
W.recorded_at = nil

-- The rows chosen in the window's lists, by list.
local chosen = { points = nil, monsters = nil, all = nil, buylist = nil, items = nil, consumables = nil,
	buffs = nil, keys = nil }

-- The files of the data folder under `dir` with extension `ext`, and the plugin's own under it:
-- a choice's options, the empty one first.
local function files_in(dir, ext)
	return function()
		local out, seen = { "" }, {}
		local function add(name)
			if string.lower(string.sub(name, -#ext)) == ext and not seen[name] then
				seen[name] = true
				table.insert(out, name)
			end
		end
		for _, f in ipairs(files.data_list()) do
			add(string.match(f, "^" .. dir .. "/(.+)$") or f)
		end
		for _, f in ipairs(files.package_list()) do
			local name = string.match(f, "^" .. dir .. "/(.+)$")
			if name ~= nil then
				add(name)
			end
		end
		return out
	end
end

-- The settings each page shows: { setting, label, kind, extra }. Kinds: toggle; number (with min,
-- max, step); metres (a distance setting, shown and stepped in metres); choice (with options, or
-- a function answering them); pick (a number setting's values, each `{ value, label }`); input
-- (a setting typed in). A group with `into` has no tab of its own: it stands on that page.
W.PAGES = {
	{ "Combat", {
		{ "EnableCombat", "Combat", "toggle" },
		{ "AttackDistance", "Attack range (m)", "metres", { 1, 60, 1 } },
		{ "ApproachDistance", "Approach range (m)", "metres", { 0, 120, 5 } },
		{ "AttackPower", "Power (%)", "number", { 0, 100, 10 } },
		{ "AutoAttackPower", "Power by damage type", "toggle" },
		{ "UseRecklessness", "Recklessness", "toggle" },
		{ "UseArcs", "Arcs for bolts", "toggle" },
		{ "ArcRange", "Arcs from (m)", "metres", { 0, 60, 1 } },
		{ "RingDistance", "Ring range (m)", "metres", { 1, 30, 1 } },
		{ "MinimumRingTargets", "Ring at (monsters)", "number", { 1, 20, 1 } },
		{ "SwitchWandsToDebuff", "Wand to debuff", "toggle" },
		{ "DoJiggle", "Dodge spells", "toggle" },
		{ "BreakableTurn", "Look again after turning", "toggle" },
		{ "DefaultMeleeAttackHeight", "Height (1 high, 3 low)", "number", { 1, 3, 1 } },
		{ "TargetLock", "Attack the selected first", "toggle" },
		{ "TargetSelectMethod", "Targets", "pick", { { 1, "nearest" }, { 2, "least turn" }, { 3, "turn near, then nearest" } } },
		{ "TargetSelectAngleRange", "By turning within (m)", "metres", { 0, 60, 1 } },
		{ "DebuffEachFirst", "Debuff first", "pick", { { 1, "the target" }, { 2, "its priority" }, { 3, "all" } } },
		{ "DebuffPrecastSeconds", "Recast debuffs before (s)", "number", { 0, 600, 5 } },
		{ "GiveUpSeconds", "Give up after (s)", "number", { 10, 600, 10 } },
		{ "BlacklistMonsterAttemptCount", "Failed attacks to leave", "number", { 1, 100, 1 } },
		{ "DeleteGhostMonsters", "Find ghosts by casts", "toggle" },
		{ "GhostMonsterSpellAttemptCount", "Casts that never begin", "number", { 1, 50, 1 } },
		{ "DeleteGhostMonstersByHPTracker", "Find ghosts by health", "toggle" },
		{ "GhostDeleteHPTrackerSeconds", "Health unchanged (s)", "number", { 5, 600, 5 } },
		{ "IdlePeaceMode", "Peace mode when idle", "toggle" },
		{ "SummonPets", "Summon pets", "toggle" },
		{ "PetRangeMode", "Pet range", "pick", { { 0, "attack range" }, { 1, "custom" } } },
		{ "PetCustomRange", "Custom pet range (m)", "metres", { 1, 60, 1 } },
		{ "PetMonsterDensity", "Pet at (monsters)", "number", { 1, 20, 1 } },
	} },
	{ "Items", {
		{ "Weapon", "Weapon", "text" },
		{ "CasterWeapon", "Wand", "text" },
		{ "Ammo", "Ammo", "text" },
		{ "Offhand", "Offhand", "text" },
	} },
	{ "Nav", {
		{ "EnableNav", "Navigation", "toggle" },
		{ "Route", "Route", "choice", files_in("routes", ".nav") },
		{ "NavPriorityBoost", "Walk before fighting", "toggle" },
		{ "NavCloseStopRange", "Arrived (and follow) within (m)", "metres", { 1, 10, 0.5 } },
		{ "FollowAroundCorners", "Follow around corners", "toggle" },
		{ "OpenDoors", "Open doors", "toggle" },
		{ "DoorOpenRange", "Doors within (m) of the way", "metres", { 0.5, 10, 0.5 } },
		{ "DoorIDRange", "Look at locks within (m)", "metres", { 1, 60, 1 } },
		{ "DoorLockpickDiffExcessThreshold", "Pick locks beaten by", "number", { -100, 500, 5 } },
		{ "NavFarStopRange", "Off the route, stop at (m)", "metres", { 50, 999999 * 240, 50 } },
		{ "JourneyToRouteMetres", "Travel to a route past (m)", "number", { 50, 100000, 50 } },
		{ "StuckTries", "Unstick tries", "number", { 0, 10, 1 } },
		{ "NavJourneys", "Journeys (portals, recalls)", "toggle" },
	} },
	{ "Loot", {
		{ "EnableLooting", "Looting", "toggle" },
		{ "LootProfile", "Loot profile", "choice", files_in("profiles", ".utl") },
		{ "LootPriorityBoost", "Loot before fighting", "toggle" },
		{ "CorpseApproachRange-Max", "Corpses within (m)", "metres", { 2, 60, 2 } },
		{ "CorpseApproachRange-Min", "Walk to within (m)", "metres", { 0.5, 10, 0.5 } },
		{ "ReadUnknownScrolls", "Read unknown scrolls", "toggle" },
		{ "LootFellowCorpses", "Loot fellows' kills", "toggle" },
		{ "LootAllCorpses", "Loot others' kills", "toggle" },
		{ "LootOnlyRareCorpses", "Only corpses with a rare", "toggle" },
		{ "CombineSalvage", "Combine salvage bags", "toggle" },
		{ "AutoStack", "Stack", "toggle" },
		{ "AutoCram", "Cram into side packs", "toggle" },
		{ "MinFreeSlots", "Pack full below (slots)", "number", { 0, 50, 1 } },
		{ "OnFullPack", "When the pack is full", "choice", { "stop", "recall", "vendor", "keep" } },
		{ "CorpseOpenTimeoutSeconds", "Open again after (s)", "number", { 1, 120, 1 } },
		{ "BlacklistCorpseOpenAttemptCount", "Tries at opening", "number", { 1, 100, 1 } },
		{ "BlacklistCorpseOpenTimeoutSeconds", "Then leave it (s)", "number", { 10, 3600, 10 } },
		{ "CorpseItemAppearanceTimeoutSeconds", "Wait for items (s)", "number", { 1, 60, 1 } },
		{ "CorpseItemIDTimeoutSeconds", "Wait for appraisals (s)", "number", { 1, 60, 1 } },
		{ "CorpseLootItemMaxAttempts", "Tries at an item", "number", { 1, 100, 1 } },
		{ "CorpseCacheTimeoutMinutes", "Remember corpses (min)", "number", { 1, 60, 1 } },
	} },
	{ "Vitals", {
		{ "Recharge-Norm-HitP", "Heal at (%)", "number", { 0, 100, 5 } },
		{ "Recharge-NoTarg-HitP", "Top up to (%)", "number", { 0, 100, 5 } },
		{ "Recharge-Norm-Stam", "Tired below (%)", "number", { 0, 100, 5 } },
		{ "Recharge-NoTarg-Stam", "Rest stamina to (%)", "number", { 0, 100, 5 } },
		{ "Recharge-Norm-Mana", "Low mana below (%)", "number", { 0, 100, 5 } },
		{ "Recharge-NoTarg-Mana", "Rest mana to (%)", "number", { 0, 100, 5 } },
		{ "UseHealingKits", "Healing kits", "toggle" },
		{ "UseKitsInMagicMode", "Kits in magic stance", "toggle" },
		{ "GoToPeaceModeToUseKits", "Peace mode for kits", "toggle" },
		{ "RechargeBoostTimeSeconds", "Recharge boost (s)", "number", { 0, 600, 5 } },
		{ "RechargeBoostAmount", "Boost by (points)", "number", { 0, 1000, 10 } },
		{ "ClearLevelBoostFlagOnCast", "Boost ends at another spell", "toggle" },
		{ "RestWhenIdle", "Rest when idle", "toggle" },
		{ "RefillWornMana", "Refill worn mana", "toggle" },
		{ "ManaChargesWhenOff", "Refill while stopped", "toggle" },
		{ "RefillWornMana-Item-ManaPercent", "Refill items under (%)", "number", { 0, 100, 5 } },
		{ "ManaStoneLootCount", "Mana stones to keep", "number", { 0, 20, 1 } },
		{ "ManaTankMinimumMana", "Drain items with (mana)", "number", { 0, 10000, 100 } },
		{ "UseDispelItems", "Dispel items when vulned", "toggle" },
		{ "CastDispelSelf", "Dispel spell when vulned", "toggle" },
		{ "DoHelp", "Help fellows", "toggle" },
		{ "Recharge-Helper-HitP", "Heal fellows at (%)", "number", { 0, 100, 5 } },
		{ "Recharge-Helper-Stam", "Stamina to fellows at (%)", "number", { 0, 100, 5 } },
		{ "Recharge-Helper-Mana", "Mana to fellows at (%)", "number", { 0, 100, 5 } },
		{ "HelperDistanceHitP", "Heal fellows within (m)", "metres", { 1, 60, 1 } },
		{ "HelperDistanceStam", "Stamina within (m)", "metres", { 1, 60, 1 } },
		{ "HelperDistanceMana", "Mana within (m)", "metres", { 1, 60, 1 } },
		{ "UseHealersHeart", "Healer's Heart", "toggle" },
		{ "JumpOutWandCasting", "Jump out of the heart's cast", "toggle" },
		{ "AutoFellowManagement", "Run the fellowship (xp tells)", "toggle" },
		{ "PeerFellow", "Own fellow on this machine", "input" },
	} },
	{ "Buffs", {
		{ "EnableBuffing", "Buffing", "toggle" },
		{ "RebuffTimeRemainingSeconds", "Rebuff under (s)", "number", { 0, 3600, 30 } },
		{ "IdleBuffTopoff", "Top up when idle", "toggle" },
		{ "IdleBuffTopoffTimeSeconds", "Idle top-up under (s)", "number", { 0, 3600, 60 } },
		{ "BuffCastRecast_Seconds", "After a buff, others early by (s)", "number", { 0, 3600, 30 } },
		{ "BuffCastRecastReset_Seconds", "for (s)", "number", { 0, 3600, 30 } },
		{ "BuffItems", "Buff weapon and armour", "toggle" },
		{ "FastCastBuffs", "Fast-cast buffs", "toggle" },
		{ "MaxSpellLevel", "Highest spell level", "number", { 1, 8, 1 } },
		{ "BuffProfile_Prots", "Protections", "choice", { "All", "None", "B", "BPS", "BPSA", "ALFC", "Custom" } },
		{ "BuffProfile-Prots", "Custom protections (BPSALFC)", "input" },
		{ "BuffProfile_Banes", "Banes", "choice", { "All", "None", "B", "BPS", "BPSA", "ALFC", "Custom" } },
		{ "BuffProfile-Banes", "Custom banes (BPSALFC)", "input" },
		{ "BlacklistedSpellComps", "Excluded components", "input" },
		{ "NoBuffItems", "Items never buffed", "input" },
		{ "RandomHelperBuffs", "Random helper buffs", "toggle" },
		{ "RandomHelperIntervalSeconds", "Every (s)", "number", { 5, 3600, 5 } },
		{ "Buffbot", "Buffbot", "input" },
		{ "BuffbotRequest", "Tell it", "input" },
		{ "BuffbotMinutes", "Go with its buffs under (min)", "number", { 0, 120, 1 } },
		{ "BuffbotWaitSeconds", "Wait there for buffs (s)", "number", { 30, 3600, 30 } },
		{ "BuffbotRetryMinutes", "Ask again after none (min)", "number", { 1, 240, 1 } },
	} },
	{ "Vendor", {
		{ "VendorSell", "Sell by the Sell rules", "toggle" },
		{ "VendorBuy", "Buy the buy list", "toggle" },
		{ "Vendor", "Vendor", "text" },
	} },
	{ "Stop", {
		{ "StopMacroOnDeath", "Stop when dead", "toggle" },
		{ "RecoverCorpse", "Go back for the corpse", "toggle" },
		{ "StopAfterDeaths", "Stop after deaths", "number", { 0, 100, 1 } },
		{ "StopAfterMinutes", "Stop after (min)", "number", { 0, 1440, 15 } },
		{ "RecallSpell", "Recall spell", "number", { 0, 100000, 1 } },
		{ "StartOnLogin", "Start at login", "toggle" },
	}, into = "Settings" },
	{ "Meta", {
		{ "EnableMeta", "Meta", "toggle" },
		{ "Meta", "Meta file", "choice", files_in("metas", ".met") },
	}, into = "Meta" },
	{ "Remote", {
		{ "MiniRemote", "Remote control (F, CF, FC, C, lock)", "toggle" },
	}, into = "Run" },
}

-- The damage types and modes a monster row may name, and its extra vulnerabilities (VTank's Ex.
-- Vuln).
local DAMAGE = { "auto", "fire", "lightning", "cold", "acid", "slash", "pierce", "blunt", "nether", "drain", "harm",
	"fists", "random" }
local EXVULN = { "none", "auto", "fire", "lightning", "cold", "acid", "slash", "pierce", "blunt" }
-- The elements of the pet a monster row summons (VTank's PetDmg).
local PETDMG = { "pauto", "auto", "none", "fire", "lightning", "cold", "acid", "slash", "pierce", "blunt", "nether" }

-- The meta's states, for the state to set (the one it is in first), or none.
local function meta_states()
	local m = metas.meta
	if m == nil then
		return { "" }
	end
	local out = {}
	for name in pairs(m.states) do
		if name ~= m.state then
			table.insert(out, name)
		end
	end
	table.sort(out)
	table.insert(out, 1, m.state)
	return out
end
-- The debuffs a monster row may switch, by key: a vulnerability and the debuff spells.
local function debuff_keys()
	local out = { "vuln" }
	for k in pairs(spells.DEBUFFS) do
		table.insert(out, k)
	end
	table.sort(out)
	return out
end

-- Control ids: "s:<setting>" for a setting; the other pages' own ids are plain.
local function control(entry)
	local name, label, kind, extra = entry[1], entry[2], entry[3], entry[4]
	local id = "s:" .. name
	local v = S.get(name)
	if kind == "toggle" then
		return ui.toggle({ id = id, text = label, value = v == true })
	elseif kind == "metres" then
		return ui.number({ id = id, text = label, value = math.floor(v * 240 * 10 + 0.5) / 10,
			min = extra[1], max = extra[2], step = extra[3] })
	elseif kind == "number" then
		return ui.number({ id = id, text = label, value = v, min = extra[1], max = extra[2], step = extra[3] })
	elseif kind == "pick" then
		local options, shown = {}, tostring(v)
		for _, o in ipairs(extra) do
			table.insert(options, o[2])
			if o[1] == v then
				shown = o[2]
			end
		end
		return ui.choice({ id = id, text = label, options = options, value = shown })
	elseif kind == "input" then
		return ui.input({ id = id, text = label, value = tostring(v), grow = 1 })
	elseif kind == "choice" then
		local options = type(extra) == "function" and extra() or extra
		local have = false
		for _, o in ipairs(options) do
			have = have or o == v
		end
		if not have then
			table.insert(options, tostring(v))
		end
		return ui.choice({ id = id, text = label, options = options, value = tostring(v) })
	end
	return ui.label({ id = id, text = label .. ": " .. tostring(v) })
end

local kinds, picks = {}, {}
for _, page in ipairs(W.PAGES) do
	for _, entry in ipairs(page[2]) do
		kinds[entry[1]] = entry[3]
		if entry[3] == "pick" then
			picks[entry[1]] = entry[4]
		end
	end
end

local function monster_rows()
	local rows = {}
	for _, r in ipairs(monsters.rows()) do
		local d = {}
		for k, on in pairs(r.debuffs or {}) do
			if on then
				table.insert(d, k)
			end
		end
		table.sort(d)
		local does = r.attack == false and "debuff" or "attack"
		if r.ring then
			does = does .. " R"
		end
		if r.streak then
			does = does .. " S"
		end
		table.insert(rows, { r.name, r.priority or 1, does, r.damage or "auto", r.weapon or "", table.concat(d, " "),
			r.exvuln or "none", r.petdmg or "pauto" })
	end
	return rows
end

-- The settings the Settings page lists: every one, or those whose name holds the filter typed
-- (VTank's advanced options, filterable), without case.
W.filter = ""
local function setting_names()
	local out, want = {}, string.lower(W.filter)
	for _, name in ipairs(S.names()) do
		if want == "" or string.find(string.lower(name), want, 1, true) then
			table.insert(out, name)
		end
	end
	return out
end

local function setting_rows()
	local rows = {}
	for _, name in ipairs(setting_names()) do
		local v = S.get(name)
		if kinds[name] == "metres" then
			v = string.format("%s (%.1f m)", tostring(v), v * 240)
		end
		table.insert(rows, { name, tostring(v) })
	end
	return rows
end

local function route_rows(E)
	local rows = {}
	for i, p in ipairs(E.route and E.route.points or {}) do
		table.insert(rows, { i, p.kind, routes.describe(p) })
	end
	return rows
end

local function route_title(E)
	local r = E.route
	if r == nil then
		return "No route: New makes one, Load reads one."
	end
	if r.kind == "follow" then
		return string.format("Route %s: following %s%s", E.route_name or S.get("Route") ~= "" and S.get("Route") or "(new)",
			tostring(r.target), S.get("FollowAroundCorners") and ", around corners" or "")
	end
	return string.format("Route %s: %s%s, %d waypoints%s%s", E.route_name or S.get("Route") ~= "" and S.get("Route") or "(new)",
		r.kind, r.reversed and " (run reversed)" or "", #r.points, E.route_edited and ", changed since it was saved" or "",
		W.recording and string.format("; recording every %d m", S.get("RouteRecordMetres")) or "")
end

-- The controls of the groups that stand on another page, by that page.
local function guests()
	local out = {}
	for _, page in ipairs(W.PAGES) do
		if page.into ~= nil then
			local controls = {}
			for _, entry in ipairs(page[2]) do
				table.insert(controls, control(entry))
			end
			out[page.into] = controls
		end
	end
	return out
end

local function buy_rows()
	local rows = {}
	for _, r in ipairs(vendors.buy_list()) do
		table.insert(rows, { r.name, r.count, vendors.held(r.name) })
	end
	return rows
end

-- The log's types, as the engine names them (set by `W.build`).
local log_types = {}

-- Toggles for the log's types `from` to `to`, each on while its lines are logged.
local function log_toggles(from, to)
	local out = {}
	for i = from, to do
		local t = log_types[i]
		if t ~= nil then
			table.insert(out, ui.toggle({ id = "log:" .. t, text = t, value = not S.listed("LogOff", t) }))
		end
	end
	return out
end

-- An item of the pack by name, without case, and how many of it the pack holds.
local function in_pack(name)
	local want, first, n = string.lower(name or ""), nil, 0
	for _, it in ipairs(game.inventory.items()) do
		if string.lower(it.name or "") == want then
			first = first or it
			n = n + (it.stack or 1)
		end
	end
	return first, n
end

-- What an item is, as the Items page's lists show it.
local KINDS = { MeleeWeapon = "melee weapon", MissileWeapon = "missile weapon", WandStaffOrb = "caster",
	Armor = "shield", Food = "food or potion", HealingKit = "kit", ManaStone = "mana stone or charge",
	Lockpick = "lockpick", Gem = "gem", SpellComponent = "component" }
local function item_kind(it)
	if it == nil then
		return "not in the pack"
	end
	if string.find(string.lower(it.name or ""), "essence", 1, true) then
		return "pet"
	end
	return KINDS[it.class] or string.lower(tostring(it.class))
end

-- The combat items (VTank's Items tab): the weapons, casters and shields `auto` rows choose from
-- (Weapons), then the pets (Pets): `{ list, name }` each.
local function combat_items()
	local out = {}
	for _, list in ipairs({ "Weapons", "Pets" }) do
		for _, name in ipairs(S.list(list)) do
			table.insert(out, { list = list, name = name })
		end
	end
	return out
end

local function item_rows()
	local rows = {}
	for _, e in ipairs(combat_items()) do
		table.insert(rows, { e.name, item_kind((in_pack(e.name))), S.listed("NoBuffItems", e.name) and "no" or "yes" })
	end
	return rows
end

local function consumable_rows()
	local rows = {}
	for _, name in ipairs(S.list("Consumables")) do
		local it, n = in_pack(name)
		table.insert(rows, { name, item_kind(it), n })
	end
	return rows
end

local function buff_rows()
	local rows = {}
	for _, b in ipairs(buffs.listed()) do
		table.insert(rows, { b.family, b.spell and b.spell.name or (spells.ready() and "none known" or "?"), b.how })
	end
	return rows
end

-- Row `row` of `rows` when it is one of them, else nil (a list's chosen row, after the list
-- changed).
local function within(row, rows)
	return row ~= nil and rows[row] ~= nil and row or nil
end

-- A list setting with `name` added to it, or taken out (without case).
local function list_add(setting, name)
	if not S.listed(setting, name) then
		local list = S.list(setting)
		table.insert(list, name)
		S.set(setting, table.concat(list, ", "))
	end
end
local function list_remove(setting, name)
	local out = {}
	for _, p in ipairs(S.list(setting)) do
		if string.lower(p) ~= string.lower(name) then
			table.insert(out, p)
		end
	end
	S.set(setting, table.concat(out, ", "))
end
W.list_add, W.list_remove = list_add, list_remove

-- The route files an embedded route is chosen from.
local function route_files()
	local out = {}
	for _, f in ipairs(files_in("routes", ".nav")()) do
		if f ~= "" then
			table.insert(out, f)
		end
	end
	return out
end

-- The settings profiles kept for this character (VTank's [Char] profiles).
local function char_profiles()
	local out, prefix = {}, "characters/" .. tostring(game.player.name or "unknown") .. "/"
	for _, f in ipairs(files.data_list()) do
		local name = string.sub(f, 1, #prefix) == prefix and string.sub(f, #prefix + 1) or nil
		if name ~= nil and string.lower(string.sub(name, -4)) == ".usd" then
			table.insert(out, (string.gsub(name, "%.[Uu][Ss][Dd]$", "")))
		end
	end
	table.sort(out)
	return out
end

-- The Meta page: the meta and its file, its state set by hand and bound to keys, its rules with
-- the editor under them, an expression run, and the meta saved as another file or a new one made.
local function meta_page(guest, keys)
	local nodes = { title = "Meta" }
	for _, c in ipairs(guest.Meta or {}) do
		table.insert(nodes, c)
	end
	local m = metas.meta
	-- The state, set by hand (VTank's dropdown) or bound to a key; a death faked.
	table.insert(nodes, ui.row({
		ui.choice({ id = "meta_state", text = "State", options = meta_states(), value = meta_states()[1], grow = 1 }),
		ui.button({ id = "meta_set_state", text = "Set" }),
		ui.button({ id = "vt:fakedeath", text = "Fake death" }),
	}))
	table.insert(nodes, ui.row({
		ui.input({ id = "meta_key", text = "Key", value = "", placeholder = "Ctrl+1", grow = 1 }),
		ui.button({ id = "meta_bind", text = "Bind the state", tooltip = "The state chosen above, entered at the key" }),
	}))
	for i, b in ipairs(hotkeys.states()) do
		table.insert(nodes, ui.row({
			keys["state:" .. i] or ui.label({ text = string.format("State %s: %s is not a key", b.state, b.key) }),
			ui.button({ id = "meta_unbind:" .. i, text = "Unbind" }),
		}))
	end
	-- The rules: those that ran in this stay in the state in red; a rule chosen fills the editor.
	table.insert(nodes, ui.label({ id = "meta_now", text = m == nil and "No meta: choose its file, or make a new one below."
		or string.format("%s, in state %s: the rules that ran in it are red.", tostring(metas.name), tostring(m.state)) }))
	local rules = metaedit.rows(metas.file, m and m.state, m and m.ran)
	table.insert(nodes, ui.list({ id = "meta_rules", columns = { { title = "#", width = 4 }, { title = "State", width = 10 },
		{ title = "Condition", width = 22, grow = 1 }, { title = "Action", width = 18, grow = 1 } }, rows = rules, height = 8,
		selected = within(metaedit.chosen, rules) }))
	if metas.file ~= nil then
		for _, node in ipairs(metaedit.nodes(route_files())) do
			table.insert(nodes, node)
		end
	end
	table.insert(nodes, ui.separator({}))
	table.insert(nodes, ui.row({
		ui.input({ id = "mexec", text = "Expression", value = "", grow = 1 }),
		ui.button({ id = "mexec_go", text = "Run" }),
		ui.button({ id = "vt:dumpmetavars", text = "Variables" }),
	}))
	table.insert(nodes, ui.row({
		ui.input({ id = "meta_save_name", text = "Meta file", value = "", placeholder = "hunting.met", grow = 1 }),
		ui.button({ id = "meta_save", text = "Save as", tooltip = "The meta saved under this name too" }),
		ui.button({ id = "meta_new", text = "New meta", tooltip = "A meta of this name with no rules, run from now" }),
	}))
	return ui.group(nodes)
end

-- What a monster row's Weapon (or Offhand) may name, as VTank's column offers the Items tab's: none,
-- `auto` (a weapon), the combat items, the pack's weapons and casters (shields and melee weapons),
-- and whatever a row names already.
local function row_items(field)
	local out, seen = { "" }, { [""] = true }
	local function add(name)
		if name ~= nil and not seen[string.lower(name)] then
			seen[string.lower(name)] = true
			table.insert(out, name)
		end
	end
	if field == "weapon" then
		add("auto")
	end
	for _, name in ipairs(S.list("Weapons")) do
		add(name)
	end
	if field == "offhand" then
		add(S.get("Offhand") ~= "" and S.get("Offhand") or nil)
	end
	local fits = field == "weapon" and { MeleeWeapon = true, MissileWeapon = true, WandStaffOrb = true }
		or { Armor = true, MeleeWeapon = true }
	for _, it in ipairs(game.inventory.items()) do
		-- A shield is armor worn in the shield's place (its equip places, property 9).
		local shield = it.class == "Armor" and (bit32.band(it:int(9) or 0, 0x00200000) ~= 0
			or string.find(string.lower(it.name or ""), "shield", 1, true) ~= nil)
		if fits[it.class] and (field == "weapon" or it.class == "MeleeWeapon" or shield) then
			add(it.name)
		end
	end
	for _, r in ipairs(monsters.rows()) do
		add(r[field] ~= "" and r[field] or nil)
	end
	return out
end

-- The pages of the window, built from the settings as they are now.
local function pages(E)
	local guest = guests()
	local keys = hotkeys.nodes()
	local out = {
		ui.group({ title = "Run",
			ui.row({
				ui.button({ id = "start", text = "Start", tooltip = "Run the macro, or resume it where it was paused" }),
				ui.button({ id = "pause", text = "Pause", tooltip = "Pause the run, keeping what it knows" }),
				ui.button({ id = "stop", text = "Stop" }),
				ui.label({ id = "state", text = "Stopped", width = 30 }),
			}),
			ui.row({
				ui.input({ id = "hunt_at", text = "Hunt at", value = E.hunt_name or "", placeholder = "42.2N 33.8E, 0x019E0140, portal <name>, lifestone",
					grow = 1 }),
				ui.button({ id = "hunt", text = "Hunt" }),
				ui.button({ id = "go", text = "Go", tooltip = "Travel there by a journey, the macro stopped" }),
			}),
			ui.row(guest.Run or {}),
			ui.label({ id = "run_meta", text = metas.meta and string.format("Meta %s, in state %s", tostring(metas.name),
				tostring(metas.meta.state)) or "No meta", width = 50 }),
			ui.label({ id = "vitals", text = "", width = 50 }),
			ui.label({ id = "tally", text = "", width = 50 }),
			-- The log takes the rest of the page: its time a fixed column, its line the rest.
			ui.list({ id = "log", columns = { { title = "Time", width = 7 }, "Line" }, rows = {}, height = 10,
				grow = 1 }),
		}),
	}
	for _, page in ipairs(W.PAGES) do
		if page.into == nil then
			local nodes = { title = page[1] }
			for _, entry in ipairs(page[2]) do
				table.insert(nodes, control(entry))
			end
			if page[1] == "Items" then
				-- The item selected in the game, taken for a slot at a click.
				table.insert(nodes, ui.label({ text = "Select an item in the game, then:" }))
				table.insert(nodes, ui.row({
					ui.button({ id = "use:Weapon", text = "Weapon" }),
					ui.button({ id = "use:Bow", text = "Bow" }),
					ui.button({ id = "use:Wand", text = "Wand" }),
					ui.button({ id = "use:Ammo", text = "Ammo" }),
					ui.button({ id = "use:Offhand", text = "Offhand", tooltip = "A shield or offhand weapon, taken up beside a melee weapon" }),
					ui.button({ id = "use:NoBuff", text = "Never buff", tooltip = "Added to the items never buffed" }),
				}))
				-- VTank's Items tab: the weapons, casters and shields `auto` rows choose from
				-- (none listed: every one in the pack), and the pets, each buffed or not.
				table.insert(nodes, ui.label({ text = "Combat items (none listed: any weapon in the pack):" }))
				local items = item_rows()
				chosen.items = within(chosen.items, items)
				table.insert(nodes, ui.list({ id = "items", columns = { "Item", { title = "Kind", width = 15 },
					{ title = "Buffed", width = 7 } }, rows = items, height = 5, selected = chosen.items }))
				table.insert(nodes, ui.row({
					ui.button({ id = "item_add", text = "Add", tooltip = "The item selected in the game" }),
					ui.button({ id = "item_add_nobuff", text = "Add (No Buffs)", tooltip = "The item selected in the game, never buffed" }),
					ui.button({ id = "item_buff", text = "Buff or not", tooltip = "The item chosen: buffed, or never" }),
					ui.button({ id = "item_remove", text = "Remove" }),
				}))
				-- VTank's Consumables tab: the kits, foods, potions, gems, stones, charges,
				-- grenades and lockpicks the macro may use.
				table.insert(nodes, ui.label({ id = "cons_note", text = #S.list("Consumables") == 0
					and "Consumables (none listed: any the pack holds):" or "Consumables:" }))
				local cons = consumable_rows()
				chosen.consumables = within(chosen.consumables, cons)
				table.insert(nodes, ui.list({ id = "consumables", columns = { "Item", { title = "Kind", width = 15 },
					{ title = "Held", width = 6 } }, rows = cons, height = 5, selected = chosen.consumables }))
				table.insert(nodes, ui.row({
					ui.input({ id = "cons_name", text = "Item", value = "", grow = 1 }),
					ui.button({ id = "cons_add", text = "Add", tooltip = "The item named, else the item selected in the game" }),
					ui.button({ id = "cons_remove", text = "Remove" }),
				}))
			elseif page[1] == "Combat" then
				-- The selected monster or item looked at (VTank's /vt testmonster, propertydump),
				-- or taken for a ghost (its /vt deletemonster).
				table.insert(nodes, ui.row({
					ui.button({ id = "vt:testmonster", text = "Test monster" }),
					ui.button({ id = "vt:propertydump", text = "Properties" }),
					ui.button({ id = "vt:deletemonster", text = "Ghost" }),
				}))
				-- A step toward the weapons for a monster (its /vt equipitemsfor).
				table.insert(nodes, ui.row({
					ui.input({ id = "equip_for", text = "Equip for", value = "", placeholder = "a monster's name", grow = 1 }),
					ui.button({ id = "equip_go", text = "Step" }),
				}))
			elseif page[1] == "Loot" then
				-- The selected item against the profile (VTank's /vt testitem), and the profile
				-- saved as another file (its /vt loot save).
				table.insert(nodes, ui.row({
					ui.button({ id = "vt:testitem", text = "Test the selected item" }),
					-- VTank's "Loot this Container": the open container taken from by the profile.
					ui.button({ id = "loot_container", text = "Loot the open container", tooltip = "By the loot profile" }),
				}))
				table.insert(nodes, ui.row({
					ui.input({ id = "loot_save_name", text = "Save profile as", value = "", grow = 1 }),
					ui.button({ id = "loot_save", text = "Save" }),
				}))
			elseif page[1] == "Vendor" then
				-- The vendor the trip goes to, the trip, a trade at the open vendor, and the buy
				-- list (the Item Tool's: what to buy up to how many).
				table.insert(nodes, ui.row({
					ui.button({ id = "vendor_set", text = "Set the selected vendor" }),
					ui.button({ id = "vendor_go", text = "Go to the vendor", tooltip = "Sell and buy there, then come back" }),
					ui.button({ id = "vendor_trade", text = "Sell and buy now", tooltip = "At the vendor whose wares are open" }),
				}))
				table.insert(nodes, ui.list({ id = "buylist", columns = { "Buy", { title = "Up to", width = 7 },
					{ title = "Held", width = 6 } }, rows = buy_rows(), height = 6 }))
				table.insert(nodes, ui.row({
					ui.input({ id = "buy_name", text = "Item", value = "", grow = 1 }),
					ui.number({ id = "buy_count", text = "Up to", value = 10, min = 0, max = 100000, step = 1 }),
				}))
				table.insert(nodes, ui.row({
					ui.button({ id = "buy_set", text = "Set" }),
					ui.button({ id = "buy_selected", text = "The selected item", tooltip = "Its name in the item field" }),
					ui.button({ id = "buy_remove", text = "Remove" }),
				}))
			elseif page[1] == "Buffs" then
				W.BUFFS_PAGE = #out + 1
				-- VTank's Buffs tab: the buffs kept up, a buff added (a skill not trained, say)
				-- and one left out.
				local listed = buff_rows()
				chosen.buffs = within(chosen.buffs, listed)
				table.insert(nodes, ui.list({ id = "buffs", columns = { { title = "Buff", width = 24, grow = 1 },
					{ title = "Cast", width = 20, grow = 1 }, { title = "Why", width = 9 } },
					rows = listed, height = 6, selected = chosen.buffs }))
				table.insert(nodes, ui.row({
					ui.input({ id = "buff_name", text = "Buff", value = "", placeholder = "Leadership Mastery Self", grow = 1 }),
					ui.button({ id = "buff_add", text = "Also cast" }),
				}))
				table.insert(nodes, ui.row({
					ui.button({ id = "buff_exclude", text = "Never cast", tooltip = "The buff chosen" }),
					ui.button({ id = "buff_include", text = "Cast again", tooltip = "The buff chosen, left out before" }),
					ui.button({ id = "buff_remove", text = "Take off", tooltip = "The buff chosen, added before" }),
				}))
				-- VTank's Force Buff and Cancel Force Buff, and its /vt testspell.
				table.insert(nodes, ui.row({
					ui.button({ id = "forcebuff", text = "Force buff", tooltip = "Cast every buff again now" }),
					ui.button({ id = "cancelforcebuff", text = "Cancel force buff" }),
				}))
				table.insert(nodes, ui.row({
					ui.number({ id = "testspell_id", text = "Spell", value = 0, min = 0, max = 100000, step = 1 }),
					ui.button({ id = "testspell_go", text = "Test" }),
				}))
				-- The buffbot the macro asks for buffs, and the trip to it now.
				table.insert(nodes, ui.row({
					ui.button({ id = "buffbot_set", text = "Set the selected buffbot" }),
					ui.button({ id = "buffbot_go", text = "Go to the buffbot", tooltip = "Ask for buffs, then come back" }),
				}))
			end
			table.insert(out, ui.group(nodes))
			if page[1] == "Combat" then
				-- The monster list, after the combat page: a row chosen fills the fields below it.
				local debuffs = {}
				for _, k in ipairs(debuff_keys()) do
					table.insert(debuffs, ui.toggle({ id = "monster_debuff:" .. k, text = k, value = false }))
				end
				table.insert(out, ui.group({ title = "Monsters",
					ui.list({ id = "monsters", columns = { "Monster", { title = "P", width = 3 }, { title = "Do", width = 10 },
						{ title = "Damage", width = 9 }, "Weapon", "Debuffs", { title = "Ex. vuln", width = 9 }, { title = "Pet", width = 8 } },
						rows = monster_rows(), height = 6 }),
					ui.row({
						ui.input({ id = "monster_name", text = "Monster", value = "", placeholder = monsters.DEFAULT, grow = 1 }),
						ui.button({ id = "monster_selected", text = "The selected", tooltip = "The monster selected in the game" }),
					}),
					ui.number({ id = "monster_priority", text = "Priority", value = 1, min = -1, max = 4, step = 1 }),
					ui.choice({ id = "monster_damage", text = "Damage", options = DAMAGE, value = "auto" }),
					ui.choice({ id = "monster_weapon", text = "Weapon", options = row_items("weapon"), value = "",
						tooltip = "A combat item (the Items page's, or the pack's), or auto: chosen by the damage type" }),
					ui.choice({ id = "monster_offhand", text = "Offhand", options = row_items("offhand"), value = "" }),
					ui.choice({ id = "monster_exvuln", text = "Extra vuln", options = EXVULN, value = "none" }),
					ui.choice({ id = "monster_petdmg", text = "Pet", options = PETDMG, value = "pauto", tooltip = "The element of the pet summoned for it" }),
					ui.toggle({ id = "monster_attack", text = "Attack it (off: debuff only)", value = true }),
					ui.row({
						ui.toggle({ id = "monster_ring", text = "Rings", value = false }),
						ui.toggle({ id = "monster_streak", text = "Streaks", value = false }),
					}),
					ui.label({ text = "Debuffs:" }),
					ui.row({ table.unpack(debuffs, 1, 4) }),
					ui.row({ table.unpack(debuffs, 5, 8) }),
					ui.row({ table.unpack(debuffs, 9) }),
					ui.row({
						ui.button({ id = "monster_save", text = "Save the row" }),
						ui.button({ id = "monster_remove", text = "Remove it" }),
					}),
				}))
			elseif page[1] == "Nav" then
				-- The route, made and changed in the game as VTank's Route tab makes it.
				W.ROUTE_PAGE = #out + 1
				table.insert(out, ui.group({ title = "Route",
					ui.label({ id = "r_title", text = route_title(E) }),
					ui.list({ id = "r_points", columns = { { title = "#", width = 4 }, { title = "Kind", width = 10 }, "Where, what" },
						rows = route_rows(E), height = 8 }),
					ui.row({
						ui.button({ id = "r_up", text = "Up" }),
						ui.button({ id = "r_down", text = "Down" }),
						ui.button({ id = "r_remove", text = "Remove" }),
						ui.button({ id = "r_reverse", text = "Reverse" }),
					}),
					ui.row({
						ui.choice({ id = "r_kind", text = "New", options = { "circular", "linear", "once", "follow" }, value = "circular" }),
						ui.button({ id = "r_new", text = "New route" }),
						ui.button({ id = "r_follow", text = "Follow the selected", tooltip = "Follow the character selected in the game (VTank's FC)" }),
					}),
					ui.row({
						ui.button({ id = "r_add", text = "Add point", tooltip = "Where you stand, or at the selected portal, vendor or NPC" }),
						ui.button({ id = "r_portal", text = "Portal/NPC", tooltip = "The selected portal, passed through, or NPC, used once" }),
						ui.button({ id = "r_talk", text = "NPC talk", tooltip = "The selected NPC, used until it tells you something" }),
						ui.button({ id = "r_vendor", text = "Vendor", tooltip = "The selected vendor, opened" }),
					}),
					ui.row({
						ui.button({ id = "r_checkpoint", text = "Checkpoint" }),
						ui.toggle({ id = "r_record", text = "Record while walking", value = W.recording }),
						ui.number({ id = "s:RouteRecordMetres", text = "every (m)", value = S.get("RouteRecordMetres"), min = 1, max = 100, step = 1 }),
					}),
					ui.row({
						ui.number({ id = "r_charge", text = "Jump charge (ms)", value = 500, min = 0, max = 1000, step = 50 }),
						ui.toggle({ id = "r_walk", text = "Walk", value = false }),
						ui.choice({ id = "r_way", text = "Way", options = { "forward", "strafeleft", "straferight" }, value = "forward" }),
					}),
					ui.row({
						ui.button({ id = "r_jump", text = "Add jump" }),
						ui.button({ id = "r_jump_now", text = "Jump now" }),
						ui.label({ text = "at the heading you face" }),
					}),
					ui.row({
						ui.number({ id = "r_pause", text = "Pause (ms)", value = 1000, min = 0, max = 600000, step = 500 }),
						ui.button({ id = "r_add_pause", text = "Add pause" }),
					}),
					ui.row({
						ui.input({ id = "r_chat", text = "Chat", value = "", grow = 1 }),
						ui.button({ id = "r_add_chat", text = "Add chat" }),
					}),
					-- Quest steps (the plugin's own): an item handed to the selected person, the
					-- selected item picked up, an item used on the selected object; now, or as a
					-- waypoint whose chat line the macro carries out.
					ui.row({
						ui.input({ id = "r_item", text = "Item", value = "", grow = 1 }),
						ui.button({ id = "q_give", text = "Give", tooltip = "Hand the item to the selected person now" }),
						ui.button({ id = "q_pickup", text = "Pick up", tooltip = "Pick up the selected item (or the item named) now" }),
						ui.button({ id = "q_useon", text = "Use on", tooltip = "Use the item on the selected object now" }),
					}),
					ui.row({
						ui.button({ id = "r_give", text = "Add give", tooltip = "A waypoint that hands the item to the selected person" }),
						ui.button({ id = "r_pickup", text = "Add pick up", tooltip = "A waypoint that picks up the selected item (or the item named)" }),
						ui.button({ id = "r_useon", text = "Add use on", tooltip = "A waypoint that uses the item on the selected object" }),
					}),
					ui.row({
						ui.number({ id = "r_recall", text = "Recall spell", value = 0, min = 0, max = 100000, step = 1 }),
						ui.button({ id = "r_add_recall", text = "Add recall" }),
					}),
					ui.row({
						ui.input({ id = "r_save_name", text = "Save as", value = "", grow = 1 }),
						ui.button({ id = "r_save", text = "Save" }),
					}),
					ui.row({
						ui.choice({ id = "r_load", text = "Load", options = files_in("routes", ".nav")(), value = "", grow = 1 }),
						ui.button({ id = "r_load_go", text = "Load" }),
					}),
				}))
			end
		end
	end
	W.META_PAGE = #out + 1
	table.insert(out, meta_page(guest, keys))
	-- When to stop, the hotkeys, then every setting by name, as /tank set changes it, and settings
	-- profiles read and written.
	local settings = { title = "Settings" }
	for _, c in ipairs(guest.Settings or {}) do
		table.insert(settings, c)
	end
	table.insert(settings, ui.separator({}))
	table.insert(settings, ui.label({ text = "Hotkeys (Ctrl or Alt, then F1 to F24, A to Z or 0 to 9):" }))
	for _, h in ipairs(hotkeys.KEYS) do
		local v = S.get(h[1])
		table.insert(settings, ui.row({
			ui.input({ id = "s:" .. h[1], text = h[2], value = tostring(v), placeholder = "Ctrl+Shift+F1", grow = 1 }),
			keys[h[1]] or ui.label({ text = v ~= "" and "not a key" or "none" }),
		}))
	end
	table.insert(settings, ui.row({ ui.button({ id = "hk_apply", text = "Bind the keys" }) }))
	table.insert(settings, ui.separator({}))
	for _, node in ipairs({
		ui.input({ id = "all_filter", text = "Find", value = W.filter, placeholder = "corpse", grow = 1 }),
		ui.list({ id = "all", columns = { "Setting", { title = "Value", width = 18 } }, rows = setting_rows(), height = 10 }),
		ui.row({
			ui.input({ id = "all_value", text = "Value", value = "", grow = 1 }),
			ui.button({ id = "all_set", text = "Set" }),
			ui.button({ id = "all_default", text = "Default" }),
			ui.button({ id = "all_setinall", text = "Set in all profiles" }),
		}),
		ui.label({ id = "all_note", text = "Pick a setting, then type its value." }),
		ui.separator({}),
		-- The log's types (VTank's /vt log), and its dumps of the game's tables.
		ui.label({ text = "Log:" }),
		ui.row(log_toggles(1, 5)),
		ui.row(log_toggles(6, #log_types)),
		ui.row({
			ui.button({ id = "vt:dumpspells", text = "Dump spells" }),
			ui.button({ id = "vt:dumpspecies", text = "Species" }),
			ui.button({ id = "vt:dumpmats", text = "Materials" }),
			ui.button({ id = "vt:dumpskills", text = "Skills" }),
		}),
		ui.separator({}),
		ui.row({
			ui.choice({ id = "import_file", text = "Settings profile", options = files_in("", ".usd")(), value = "", grow = 1 }),
			ui.button({ id = "import", text = "Import" }),
		}),
		ui.row({
			ui.input({ id = "export_name", text = "Export as", value = "", placeholder = "hunter.usd", grow = 1 }),
			ui.button({ id = "export", text = "Export" }),
		}),
		-- This character's own settings profiles (VTank's [Char] profiles: its /vt settings
		-- savechar and loadchar).
		ui.row({
			ui.choice({ id = "char_pick", text = "This character's", options = (function()
				local o = char_profiles()
				table.insert(o, 1, "")
				return o
			end)(), value = "", grow = 1 }),
			ui.button({ id = "loadchar", text = "Load" }),
		}),
		ui.row({
			ui.input({ id = "char_profile", text = "Save as its", value = "", placeholder = "hunter", grow = 1 }),
			ui.button({ id = "savechar", text = "Save" }),
		}),
	}) do
		table.insert(settings, node)
	end
	table.insert(out, ui.group(settings))
	return out
end

-- The monster page's fields show row `row` of the monster list.
local function fill_monster(row)
	local win = W.win
	local r = row and monsters.rows()[row]
	if r == nil then
		return
	end
	win:get("monster_name").value = r.name
	win:get("monster_priority").value = r.priority or 1
	win:get("monster_damage").value = r.damage or "auto"
	win:get("monster_weapon").value = r.weapon or ""
	win:get("monster_attack").value = r.attack ~= false
	win:get("monster_ring").value = r.ring == true
	win:get("monster_streak").value = r.streak == true
	win:get("monster_offhand").value = r.offhand or ""
	win:get("monster_exvuln").value = r.exvuln or "none"
	win:get("monster_petdmg").value = r.petdmg or "pauto"
	for _, k in ipairs(debuff_keys()) do
		win:get("monster_debuff:" .. k).value = (r.debuffs or {})[k] == true
	end
end

-- The settings page's field shows setting number `row`.
local function fill_setting(row)
	local win = W.win
	local name = row and setting_names()[row]
	if name == nil then
		return
	end
	win:get("all_value").value = tostring(S.get(name))
	win:get("all_note").text = name .. ": " .. tostring(S.get(name)) .. " (default " .. tostring(S.defaults[name]) .. ")"
end

-- Build the window (again): settings read now.
function W.build(E)
	E = E or {}
	log_types = E.LOG_TYPES or {}
	local selected = W.win and W.win:get("pages").selected or 1
	W.win = ui.window({ id = "main", title = "Tank", width = 64,
		ui.tabs({ id = "pages", selected = selected, table.unpack(pages(E)) }),
	})
	if chosen.points ~= nil then
		W.win:get("r_points").selected = chosen.points
	end
	if chosen.monsters ~= nil then
		W.win:get("monsters").selected = chosen.monsters
		fill_monster(chosen.monsters)
	end
	if chosen.all ~= nil then
		W.win:get("all").selected = chosen.all
		fill_setting(chosen.all)
	end
	W.build_remote()
end

-- VTank's remote control (its Virindi HUDs' VTank Remote Control), shown while MiniRemote is on: F
-- (Force Buff), CF (its cancel), FC (follow the selected character), C (follow around corners) and
-- the lock (target lock), the last two switches showing the settings.
function W.build_remote()
	if not S.get("MiniRemote") or not host.has("ui.hud") then
		if W.remote ~= nil then
			W.remote:close()
		end
		return
	end
	W.remote = ui.hud({ id = "remote", anchor = "right",
		ui.row({
			ui.button({ id = "F", text = "F", tooltip = "Force buff" }),
			ui.button({ id = "CF", text = "CF", tooltip = "Cancel force buff" }),
			ui.button({ id = "FC", text = "FC", tooltip = "Follow the selected character" }),
		}),
		ui.row({
			ui.toggle({ id = "C", text = "C", value = S.get("FollowAroundCorners") == true, tooltip = "Follow around corners" }),
			ui.toggle({ id = "lock", text = "Lock", value = S.get("TargetLock") == true, tooltip = "Target lock: the selected monster first" }),
		}),
	})
	W.remote:show()
end

-- The route's page shown afresh: its title and waypoints.
function W.refresh_route(E)
	local win = W.win
	if win == nil then
		return
	end
	win:get("r_title").text = route_title(E)
	local rows = route_rows(E)
	win:get("r_points").rows = rows
	if chosen.points ~= nil and rows[chosen.points] == nil then
		chosen.points = nil
	end
	win:get("r_points").selected = chosen.points
	win:get("r_record").value = W.recording
end

-- The run page's live lines.
function W.refresh(E)
	local win = W.win
	if win == nil then
		return
	end
	local page = win:get("pages").selected or 1
	if page == W.ROUTE_PAGE then
		W.refresh_route(E)
	end
	local m = metas.meta
	if page == W.META_PAGE then
		-- The meta's state, and the rules that ran in it, as the meta checks them.
		win:get("meta_now").text = m == nil and "No meta: choose its file, or make a new one below."
			or string.format("%s, in state %s: the rules that ran in it are red.", tostring(metas.name), tostring(m.state))
		local rules = metaedit.rows(metas.file, m and m.state, m and m.ran)
		win:get("meta_rules").rows = rules
		win:get("meta_rules").selected = within(metaedit.chosen, rules)
	end
	if page ~= 1 then
		return
	end
	win:get("run_meta").text = m and string.format("Meta %s, in state %s", tostring(metas.name), tostring(m.state)) or "No meta"
	win:get("state").text = E.state .. (E.target and (" " .. E.target.name) or "")
	win:get("vitals").text = E.vitals_text()
	local t = E.tally
	win:get("tally").text = string.format("Kills %d  Deaths %d  Looted %d  Kept %d  Stuck %d  Buffs %d", t.kills,
		t.deaths, t.looted, t.kept, t.stuck, t.buffs or 0)
	local rows = {}
	for i = math.max(1, #E.log - 99), #E.log do
		local time, line = string.match(E.log[i], "^%s*(%S+)%s(.*)$")
		table.insert(rows, { time or "", line or E.log[i] })
	end
	win:get("log").rows = rows
end

-- The item selected in the game taken for a slot (`Weapon`, `Bow`, `Wand`, `Ammo`): answers the
-- line to log.
local function ammo(it)
	local n = string.lower(it.name or "")
	return it.class == "MissileWeapon" and (n:find("arrow") or n:find("bolt") or n:find("quarrel") or n:find("dart")) ~= nil
end

function W.use_selected(slot)
	local it = game.selection()
	if it == nil then
		return "select an item in the game first"
	end
	if slot == "NoBuff" then
		-- An item the macro uses but never buffs (VTank's Add (No Buffs)).
		if not S.listed("NoBuffItems", it.name) then
			local list = S.list("NoBuffItems")
			table.insert(list, it.name)
			S.set("NoBuffItems", table.concat(list, ", "))
		end
		return string.format("never buffed: %s", it.name)
	end
	local fits = {
		Weapon = { "Weapon", it.class == "MeleeWeapon" or (it.class == "MissileWeapon" and not ammo(it)), "a weapon" },
		Bow = { "Weapon", it.class == "MissileWeapon" and not ammo(it), "a bow, crossbow or atlatl" },
		Wand = { "CasterWeapon", it.class == "WandStaffOrb", "a wand, staff or orb" },
		Ammo = { "Ammo", ammo(it), "arrows, bolts or darts" },
		Offhand = { "Offhand", it.class == "Armor" or it.class == "MeleeWeapon", "a shield or an offhand weapon" },
	}
	local f = fits[slot]
	if f == nil then
		return nil
	end
	if not f[2] then
		return string.format("%s is not %s", it.name or "that", f[3])
	end
	S.set(f[1], it.name)
	return string.format("%s: %s", slot == "Bow" and "Weapon" or (slot == "Wand" and "Wand" or slot), it.name)
end

-- A control changed: the setting it stands for takes the new value. Answers the setting's name
-- when it was one. The filter of the settings list narrows it as it is typed.
function W.changed(e)
	if e.id == "all_filter" then
		W.filter = tostring(e.value or "")
		chosen.all = nil
		W.win:get("all").rows = setting_rows()
		W.win:get("all").selected = nil
		return nil
	end
	local name = string.match(e.id or "", "^s:(.+)$")
	if name == nil then
		return nil
	end
	local value = e.value
	if kinds[name] == "metres" then
		value = (tonumber(value) or 0) / 240
	elseif kinds[name] == "pick" then
		for _, o in ipairs(picks[name]) do
			if o[2] == value then
				value = o[1]
			end
		end
	end
	S.set(name, value)
	return name
end

-- A row chosen in one of the window's lists: the fields that edit it show what it holds.
function W.selected(E, e)
	local win = W.win
	local row = e.rows and e.rows[1]
	if e.id == "r_points" then
		chosen.points = row
	elseif e.id == "monsters" then
		chosen.monsters = row
		fill_monster(row)
	elseif e.id == "all" then
		chosen.all = row
		fill_setting(row)
	elseif e.id == "buylist" then
		chosen.buylist = row
		local r = row and vendors.buy_list()[row]
		if r ~= nil then
			win:get("buy_name").value = r.name
			win:get("buy_count").value = r.count
		end
	elseif e.id == "items" or e.id == "consumables" or e.id == "buffs" then
		chosen[e.id] = row
	elseif e.id == "meta_rules" then
		-- A rule chosen: the editor holds it.
		metaedit.choose(metas.file, row)
		W.rebuild = true
	elseif e.id == "mr_cparts" or e.id == "mr_aparts" then
		metaedit.part_chosen(e.id, row)
	end
end

-- A field of the meta editor changed: the rule it holds follows. Answers whether it was one.
function W.meta_changed(e)
	if string.sub(e.id or "", 1, 3) ~= "mr_" then
		return false
	end
	local rebuild, why = metaedit.changed(e.id, e.value, function(name)
		return metas.E.read_file("routes", name)
	end)
	W.rebuild = rebuild
	W.note = why
	return true
end

-- A button of the monster, route or settings pages clicked: answers the line to log, or nil when
-- the click was none of theirs. `rebuild` is set when the window must be built again.
function W.clicked(E, e)
	local win = W.win
	local id = e.id or ""
	local route_words = function(...)
		return { "route", ... }
	end
	if id == "monster_selected" then
		-- VTank's monster list adds the selected monster by its name.
		local it = game.selection()
		if it == nil then
			return "select the monster in the game first"
		end
		win:get("monster_name").value = it.name or ""
		return nil
	elseif id == "item_add" or id == "item_add_nobuff" then
		-- VTank's Items tab: the selected item added, a pet to the pets, anything else it fights or
		-- casts with (or a shield) to the weapons.
		local it = game.selection()
		if it == nil then
			return "select the item in the game first"
		end
		local pet = string.find(string.lower(it.name or ""), "essence", 1, true) ~= nil
		local fights = it.class == "MeleeWeapon" or it.class == "MissileWeapon" or it.class == "WandStaffOrb" or it.class == "Armor"
		if not pet and not fights then
			return string.format("%s is not a weapon, caster, shield or pet", tostring(it.name))
		end
		list_add(pet and "Pets" or "Weapons", it.name)
		if id == "item_add_nobuff" then
			list_add("NoBuffItems", it.name)
		end
		W.rebuild = true
		return string.format("combat item %s%s", it.name, id == "item_add_nobuff" and ", never buffed" or "")
	elseif id == "item_remove" or id == "item_buff" then
		local e2 = chosen.items and combat_items()[chosen.items]
		if e2 == nil then
			return "choose an item first"
		end
		W.rebuild = true
		if id == "item_buff" then
			if S.listed("NoBuffItems", e2.name) then
				list_remove("NoBuffItems", e2.name)
				return e2.name .. ": buffed"
			end
			list_add("NoBuffItems", e2.name)
			return e2.name .. ": never buffed"
		end
		list_remove(e2.list, e2.name)
		list_remove("NoBuffItems", e2.name)
		chosen.items = nil
		return "combat item " .. e2.name .. " removed"
	elseif id == "cons_add" then
		-- VTank's Consumables tab: the item named, else the one selected in the game.
		local name = win:get("cons_name").value or ""
		if name == "" then
			local it = game.selection()
			name = it and it.name or ""
		end
		if name == "" then
			return "name the item, or select it in the game"
		end
		list_add("Consumables", name)
		W.rebuild = true
		return "consumable " .. name
	elseif id == "cons_remove" then
		local name = chosen.consumables and S.list("Consumables")[chosen.consumables]
		if name == nil then
			return "choose a consumable first"
		end
		list_remove("Consumables", name)
		chosen.consumables = nil
		W.rebuild = true
		return "consumable " .. name .. " removed"
	elseif id == "buff_add" then
		-- VTank's Buffs tab: a buff added to the default set (a skill not trained, say).
		local name = win:get("buff_name").value or ""
		if name == "" then
			return "name the buff (Leadership Mastery Self, say)"
		end
		list_add("AddedBuffs", name)
		list_remove("ExcludedBuffs", name)
		buffs.want_settings()
		W.rebuild = true
		return "buff " .. name .. " added"
	elseif id == "buff_exclude" or id == "buff_include" or id == "buff_remove" then
		local b = chosen.buffs and buffs.listed()[chosen.buffs]
		if b == nil then
			return "choose a buff first"
		end
		W.rebuild = true
		if id == "buff_exclude" then
			list_add("ExcludedBuffs", b.family)
			return "buff " .. b.family .. " never cast"
		elseif id == "buff_include" then
			list_remove("ExcludedBuffs", b.family)
			return "buff " .. b.family .. " cast again"
		end
		list_remove("AddedBuffs", b.family)
		list_remove("BuffSpells", b.family)
		chosen.buffs = nil
		return "buff " .. b.family .. " taken off"
	elseif id == "meta_bind" then
		W.rebuild = true
		return hotkeys.bind_state(win:get("meta_state").value, win:get("meta_key").value)
	elseif string.sub(id, 1, 12) == "meta_unbind:" then
		W.rebuild = true
		return hotkeys.unbind_state(tonumber(string.sub(id, 13)))
	elseif id == "hk_apply" then
		local bad = {}
		for _, h in ipairs(hotkeys.KEYS) do
			local v = S.get(h[1])
			if v ~= "" and hotkeys.key(v) == nil then
				table.insert(bad, h[2] .. " (" .. v .. ")")
			end
		end
		W.rebuild = true
		return #bad > 0 and ("not keys a hotkey takes: " .. table.concat(bad, ", ")) or "hotkeys bound"
	elseif id == "meta_new" then
		local name = win:get("meta_save_name").value
		if name == nil or name == "" then
			return "name the meta first"
		end
		metaedit.new_rule("Default")
		W.rebuild = true
		return metas.new_meta(name)
	elseif string.sub(id, 1, 3) == "mr_" then
		-- The meta editor: its parts, then the rules it keeps.
		local done, line = metaedit.part_clicked(id)
		if done then
			W.rebuild = true
			return line
		end
		if metas.file == nil then
			return "no meta is loaded"
		end
		local file, line2 = metaedit.rules_clicked(id, metas.file)
		if file ~= nil then
			line2 = line2 .. "; " .. metas.apply(file)
		end
		W.rebuild = true
		return line2
	elseif id == "monster_save" then
		local name = win:get("monster_name").value
		if name == nil or name == "" then
			return "a monster row needs a name"
		end
		local d = {}
		for _, k in ipairs(debuff_keys()) do
			d[k] = win:get("monster_debuff:" .. k).value == true
		end
		monsters.set(name, {
			priority = tonumber(win:get("monster_priority").value) or 1,
			damage = win:get("monster_damage").value,
			weapon = win:get("monster_weapon").value or "",
			attack = win:get("monster_attack").value == true,
			ring = win:get("monster_ring").value == true,
			streak = win:get("monster_streak").value == true,
			offhand = win:get("monster_offhand").value or "",
			exvuln = win:get("monster_exvuln").value,
			petdmg = win:get("monster_petdmg").value,
			debuffs = d,
		})
		chosen.monsters = select(2, monsters.find(name))
		W.rebuild = true
		return "monster row " .. name .. " saved"
	elseif id == "buy_set" or id == "buy_remove" then
		local name = win:get("buy_name").value or ""
		if name == "" then
			return "name the item first"
		end
		local count = id == "buy_remove" and 0 or math.floor(tonumber(win:get("buy_count").value) or 0)
		chosen.buylist = nil
		W.rebuild = true
		return vendors.set_buy(name, count)
	elseif id == "buy_selected" then
		local it = game.selection()
		if it == nil then
			return "select an item in the game first"
		end
		win:get("buy_name").value = it.name or ""
		return nil
	elseif id == "monster_remove" then
		local name = win:get("monster_name").value
		if name == nil or name == "" or not monsters.remove(name) then
			return "no monster row " .. tostring(name)
		end
		chosen.monsters = nil
		W.rebuild = true
		return "monster row " .. name .. " removed"
	elseif id == "r_up" or id == "r_down" then
		local j = routes.move(E, chosen.points, id == "r_up" and -1 or 1)
		if j ~= nil then
			chosen.points = j
		end
		return nil
	elseif id == "r_remove" then
		local line = routes.remove(E, chosen.points)
		chosen.points = nil
		return "route: " .. line
	elseif id == "r_reverse" then
		return "route: " .. tostring(routes.edit(E, "reverse", route_words("reverse")))
	elseif id == "r_new" then
		chosen.points = nil
		return "route: " .. tostring(routes.edit(E, "new", route_words("new", win:get("r_kind").value)))
	elseif id == "r_follow" then
		chosen.points = nil
		return "route: " .. tostring(E.follow())
	elseif id == "r_add" then
		return "route: " .. tostring(routes.edit(E, "add", route_words("add")))
	elseif id == "r_portal" or id == "r_talk" or id == "r_vendor" then
		local verb = string.sub(id, 3)
		return "route: " .. tostring(routes.edit(E, verb, route_words(verb)))
	elseif id == "r_checkpoint" then
		return "route: " .. tostring(routes.edit(E, "checkpoint", route_words("checkpoint")))
	elseif id == "r_jump" or id == "r_jump_now" then
		local heading = math.floor((game.player.heading or 0) + 0.5) % 360
		local words = route_words("jump", tostring(heading), tostring(win:get("r_walk").value == true),
			tostring(math.floor(tonumber(win:get("r_charge").value) or 0)), win:get("r_way").value)
		if id == "r_jump" then
			return "route: " .. tostring(routes.edit(E, "jump", words))
		end
		local j = routes.jump_of(words, 3)
		W.jump_now = j
		return nil
	elseif id == "r_add_pause" then
		return "route: " .. tostring(routes.edit(E, "pause", route_words("pause", tostring(math.floor(tonumber(win:get("r_pause").value) or 0)))))
	elseif id == "r_add_chat" then
		local text = win:get("r_chat").value
		if text == nil or text == "" then
			return "a chat waypoint needs its line"
		end
		local words = route_words("chat")
		for w in string.gmatch(text, "%S+") do
			table.insert(words, w)
		end
		win:get("r_chat").value = ""
		return "route: " .. tostring(routes.edit(E, "chat", words))
	elseif id == "q_give" or id == "q_pickup" or id == "q_useon" or id == "r_give" or id == "r_pickup"
		or id == "r_useon" then
		-- A quest step: the item named, and the selection (the person, the item, the target).
		local verb = string.sub(id, 3)
		local item = win:get("r_item").value or ""
		local sel = game.selection()
		local line
		if verb == "give" or verb == "useon" then
			if item == "" or sel == nil then
				return verb == "give" and "give: name the item and select the person"
					or "use on: name the item and select what it is used on"
			end
			line = verb == "give" and string.format("/tank give %s to %s", item, tostring(sel.name))
				or string.format("/tank useon %s on %s", item, tostring(sel.name))
		else
			local what = item ~= "" and item or (sel and sel.name)
			if what == nil or what == "" then
				return "pick up: select the item or name it"
			end
			line = "/tank pickup " .. what
		end
		if string.sub(id, 1, 2) == "q_" then
			W.quest = line
			return nil
		end
		local words = route_words("chat")
		for w in string.gmatch(line, "%S+") do
			table.insert(words, w)
		end
		return "route: " .. tostring(routes.edit(E, "chat", words))
	elseif id == "r_add_recall" then
		return "route: " .. tostring(routes.edit(E, "recall", route_words("recall", tostring(math.floor(tonumber(win:get("r_recall").value) or 0)))))
	elseif id == "r_save" then
		local name = win:get("r_save_name").value
		if name == nil or name == "" then
			return "a route is saved under a name"
		end
		local words = route_words("save")
		for w in string.gmatch(name, "%S+") do
			table.insert(words, w)
		end
		W.rebuild = true
		return "route: " .. tostring(routes.edit(E, "save", words))
	elseif string.sub(id, 1, 3) == "vt:" then
		W.vt = "/vt " .. string.sub(id, 4)
		return nil
	elseif id == "meta_set_state" then
		W.vt = "/vt setmetastate " .. tostring(win:get("meta_state").value or "")
		return nil
	elseif id == "mexec_go" then
		W.vt = "/vt mexec " .. tostring(win:get("mexec").value or "")
		return nil
	elseif id == "equip_go" then
		W.vt = "/vt equipitemsfor " .. tostring(win:get("equip_for").value or "")
		return nil
	elseif id == "testspell_go" then
		W.vt = "/vt testspell " .. tostring(math.floor(tonumber(win:get("testspell_id").value) or 0))
		return nil
	elseif id == "loot_save" or id == "meta_save" then
		local name = win:get(id .. "_name").value
		if name == nil or name == "" then
			return "a file is saved under a name"
		end
		W.vt = "/vt " .. (id == "loot_save" and "loot" or "meta") .. " save " .. name
		W.rebuild = true
		return nil
	elseif id == "savechar" or id == "loadchar" then
		local name = win:get(id == "loadchar" and "char_pick" or "char_profile").value
		if name == nil or name == "" then
			return id == "loadchar" and "choose the profile first" or "a settings profile is saved under a name"
		end
		W.vt = "/vt settings " .. id .. " " .. name
		W.rebuild = true
		return nil
	elseif id == "all_setinall" then
		local name = chosen.all and setting_names()[chosen.all]
		if name == nil then
			return "pick a setting first"
		end
		W.vt = "/vt opt setinall " .. name .. " " .. tostring(win:get("all_value").value or "")
		W.rebuild = true
		return nil
	elseif id == "all_set" or id == "all_default" then
		local name = chosen.all and setting_names()[chosen.all]
		if name == nil then
			return "pick a setting first"
		end
		local value = id == "all_default" and S.defaults[name] or win:get("all_value").value
		local ok, why = S.set(name, value)
		W.rebuild = true
		W.files_changed = name == "Route" or name == "LootProfile" or name == "Meta"
		return ok and string.format("%s = %s", name, tostring(S.get(name))) or why
	end
	return nil
end

return W
