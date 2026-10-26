-- A VTank meta (`.met`) run on top of the macro, as VTank runs one: its rules are checked a few
-- times a second and whenever the macro decides what to do; what it says to do with the macro
-- reaches the macro. An embedded route becomes the macro's route (walked between its fights), "set
-- option" changes the macro's settings for this run (one of VTank's this macro has not got is kept
-- for the run, so "get option" reads it back), "get option" reads them, the "need to buff"
-- condition asks the macro, monster priorities are the macro's monster list's, and the VTank
-- commands it types (VTank's /vt commands; see `M.command`) are answered by the macro.

local S = require("settings")
local monsters = require("monsters")
local buffs = require("buffs")
local spells = require("spells")
local names = require("names")
local metalib = require("dereth.meta")
local navlib = require("dereth.nav")
local X = require("dereth.expr")
local routes = require("routes")

local M = {}

M.meta = nil
M.name = nil

-- The macro the meta drives: set by main (the engine and the profile reader and writer), and the
-- macro's own `/tank` commands, which a meta's chat action or a route's chat waypoint may type.
M.E = nil
M.tank_line = nil
M.import = nil
M.export = nil
M.set_in_text = nil

local function engine()
	local E = M.E
	return {
		route = function(r)
			E.set_route(r)
		end,
		route_empty = function()
			return E.route == nil or E.route_done == true
		end,
		route_distance = function()
			return E.route_distance()
		end,
		option = function(name)
			return S.get(name)
		end,
		priority = function(o)
			return monsters.match(o, M.meta and M.meta.state).priority or 1
		end,
		command = function(line)
			return M.command(line)
		end,
	}
end

-- A line the player reads: in the chat window and the macro's log.
local function say(fmt, ...)
	local ok, line = pcall(string.format, fmt, ...)
	line = ok and line or fmt
	chat.print("[Tank] " .. line)
	M.E.note("%s", line)
end

-- Lines the player reads, forty a frame (within what a plugin may print in one).
local function say_all(lines)
	task.spawn(function()
		for i, line in ipairs(lines) do
			chat.print("[Tank] " .. line)
			if i % 40 == 0 then
				task.sleep(0)
			end
		end
	end)
end

-- A value as the expression language writes it (`True`, a list's entries).
local function show(v)
	local ok, s = pcall(X.str, v)
	return ok and s or tostring(v)
end

-- A typed value from the text typed for it: true or false, a number, else the text.
local function typed(text)
	local l = string.lower(text)
	if l == "true" or l == "false" then
		return l == "true"
	end
	return tonumber(text) or text
end

-- A file name with an extension it lacks added.
local function with_ext(name, ext)
	if string.find(string.lower(name), "%" .. ext .. "$") then
		return name
	end
	return name .. ext
end

-- A file of the macro's (its data folder first, then its own) saved again under another name in
-- the data folder: the meta and the loot profile are the macro's to use, not to change, so saving
-- one is saving its file.
local function save_copy(dir, current, name, ext)
	if current == nil or current == "" then
		return "nothing to save"
	end
	local text = M.E.read_file(dir, current)
	if text == nil then
		return "no " .. current
	end
	name = with_ext(name, ext)
	local ok, why = pcall(files.save_data, dir .. "/" .. name, text)
	return ok and ("saved " .. dir .. "/" .. name) or ("could not save " .. name .. ": " .. tostring(why))
end

-- The folder a character's own settings profiles are kept in (VTank's [By char] profiles).
local function char_folder()
	return "characters/" .. tostring(game.player.name or "unknown") .. "/"
end

-- What VTank's /vt testitem says of the selected item: what the loot profile does with it.
local function test_item(o)
	local E = M.E
	if E.profile == nil then
		return "no loot profile is loaded"
	end
	local hit = E.profile:classify(o)
	if hit == nil then
		return string.format("%s: no rule; left on the corpse", tostring(o.name))
	end
	local does = ({ keep = "kept", salvage = "taken and salvaged", sell = "taken to sell at a vendor",
		read = "taken and read" })[hit.action] or (hit.action .. ": taken for you")
	if hit.action == "keep_up_to" then
		does = string.format("kept up to %d", hit.count or 0)
	end
	return string.format("%s: rule %s, %s%s", tostring(o.name), tostring(hit.name), does,
		(not o.appraised and E.profile:needs_appraisal(o)) and " (it needs an appraisal to decide)" or "")
end

-- What VTank's /vt testmonster says of the selected monster: the row that decides, and the
-- expression variables.
local function test_monster(o)
	local r = monsters.match(o, M.meta and M.meta.state)
	local v = monsters.variables(o, M.meta and M.meta.state)
	local d = {}
	for k, on in pairs(r.debuffs or {}) do
		if on then
			table.insert(d, k)
		end
	end
	table.sort(d)
	return {
		string.format("%s: row %s, priority %d, %s, damage %s%s%s%s", tostring(o.name), r.name, r.priority or 1,
			r.attack == false and "debuff only" or "attack", r.damage or "auto",
			(r.weapon or "") ~= "" and (", weapon " .. r.weapon) or "",
			(r.exvuln or "") ~= "" and r.exvuln ~= "none" and (", extra vuln " .. r.exvuln) or "",
			#d > 0 and (", debuffs " .. table.concat(d, " ")) or ""),
		string.format("name %s, typeid %d, species %s, maxhp %d, range %.1f, hasshield %s, metastate %s", v.name,
			v.typeid, v.species, v.maxhp, v.range, tostring(v.hasshield), tostring(v.metastate)),
	}
end

-- What VTank's /vt testspell says: whether the character knows the spell, and whether it casts it
-- reliably by the hunting and buffing thresholds.
local function test_spell(id)
	local sp = magic.spell(id)
	if sp == nil then
		return string.format("no spell %d", id)
	end
	local skill = spells.skill(sp.school)
	local function verdict(setting)
		if sp.difficulty == nil or skill == nil then
			return "castable (no skill or difficulty known)"
		end
		local need = sp.difficulty + S.get(setting)
		return string.format("%s (%d of %d)", skill >= need and "castable" or "too hard", skill, need)
	end
	return string.format("%s (%d): %s; hunting %s, buffing %s%s", sp.name, id, sp.known and "known" or "not known",
		verdict("SpellDiffExcessThreshold-Hunt"), verdict("SpellDiffExcessThreshold-Buff"),
		spells.excluded(sp) and "; its scarab is excluded" or "")
end

-- VTank's /vt propertydump: the selected object's properties, from its last appraisal.
local function property_dump(o)
	local out = { string.format("%s: id 0x%08X, class %s, kind %s", tostring(o.name), o.id, tostring(o.class),
		tostring(o.wcid)) }
	local a = o:appraisal()
	if a == nil then
		table.insert(out, "no appraisal yet")
		return out
	end
	for _, t in ipairs({ { "int", a.ints }, { "int64", a.int64s }, { "float", a.floats }, { "string", a.strings },
		{ "bool", a.bools } }) do
		local keys = {}
		for k in pairs(t[2] or {}) do
			table.insert(keys, k)
		end
		table.sort(keys)
		for _, k in ipairs(keys) do
			table.insert(out, string.format("%s %d = %s", t[1], k, tostring(t[2][k])))
		end
	end
	if a.spells ~= nil and #a.spells > 0 then
		local ids = {}
		for _, s in ipairs(a.spells) do
			table.insert(ids, tostring(s))
		end
		table.insert(out, "spells " .. table.concat(ids, ", "))
	end
	return out
end

-- VTank's dump commands: a table of the game's written to the data folder, `dumps/<what>.txt`.
local function dump(what)
	local E = M.E
	task.spawn(function()
		local lines = {}
		if what == "spells" then
			for id = 1, 8000 do
				local sp = magic.spell(id)
				if sp ~= nil then
					table.insert(lines, string.format("%d\t%s\tschool %s\tlevel %s\tdifficulty %s\tmana %s", id, sp.name,
						tostring(sp.school), tostring(sp.level), tostring(sp.difficulty), tostring(sp.mana)))
				end
				if id % 250 == 0 then
					task.sleep(0)
				end
			end
		elseif what == "species" or what == "mats" then
			local t = what == "species" and names.SPECIES or names.MATERIALS
			local ids = {}
			for id in pairs(t) do
				table.insert(ids, id)
			end
			table.sort(ids)
			for _, id in ipairs(ids) do
				table.insert(lines, string.format("%d\t%s", id, t[id]))
			end
		else
			for _, sk in ipairs(game.player.skills or {}) do
				table.insert(lines, string.format("%d\t%s\t%s\tbase %s\tcurrent %s", sk.id or 0, sk.name, sk.training,
					tostring(sk.base), tostring(sk.current)))
			end
		end
		local name = "dumps/" .. what .. ".txt"
		local ok, why = pcall(files.save_data, name, table.concat(lines, "\r\n") .. "\r\n")
		say(ok and "%s: %d lines in %s" or "%s: could not save: %s", what, ok and #lines or tostring(why), name)
		if E.on_files then
			E.on_files()
		end
	end)
end

-- A VTank command a meta (or the player, by `/tank vt`) types, or one of the macro's own `/tank`
-- commands. Answers true when it was one.
--
-- Profiles: `opt set|get|list|setinall`, `nav load|save|clear`, `loot load|save`, `meta load|save`,
-- `settings load|save|loadchar|savechar`, `addnavpt`, `addnavjump`, `addnavcheckpoint`,
-- `reverseroute [true|false]`, `reverseroutequery`. Actions: `start`, `stop` (a pause),
-- `forcebuff`, `cancelforcebuff`, `setmetastate`, `fakedeath`, `deletemonster`, `equipitemsfor`,
-- `mexec`, `echo`, `jump`, `tapjump`, `setattackbar`, `refresh`. Information: `testitem`,
-- `propertydump`, `testmonster`, `testspell`, `testpet`, `listmonstervariables`, `dumpmetavars`,
-- `listmetafunctions`, `metafunchelp`, `dumpspells`, `dumpspecies`, `dumpmats`, `dumpskills`,
-- `log [type] [on|off]`.
function M.command(line)
	local words = {}
	for w in string.gmatch(line, "%S+") do
		table.insert(words, w)
	end
	local head = string.lower(words[1] or "")
	if (head == "/tank" or head == "@tank") and M.tank_line ~= nil then
		M.tank_line(line)
		return true
	end
	if head ~= "/vt" and head ~= "@vt" then
		return false
	end
	local E = M.E
	local verb = string.lower(words[2] or "")
	local sub = string.lower(words[3] or "")
	local rest3 = table.concat(words, " ", 3)
	local rest4 = table.concat(words, " ", 4)
	if verb == "opt" and sub == "set" and words[4] ~= nil then
		local value = table.concat(words, " ", 5)
		local ok, why = S.set(words[4], value, false)
		E.logk("ActiveRule", ok and "meta: %s = %s%s" or "meta: %s: %s", words[4],
			ok and tostring(S.get(words[4])) or tostring(why), ok and why and (" (" .. why .. ")") or "")
	elseif verb == "opt" and sub == "get" and words[4] ~= nil then
		local v = S.get(words[4])
		say(v == nil and "no option %s" or "%s = %s", words[4], tostring(v))
	elseif verb == "opt" and sub == "list" then
		local lines = {}
		for _, name in ipairs(S.names()) do
			table.insert(lines, string.format("%s = %s", name, tostring(S.get(name))))
		end
		for _, e in pairs(S.extra) do
			table.insert(lines, string.format("%s = %s (kept; this macro has no such option)", e.name, tostring(e.value)))
		end
		say_all(lines)
	elseif verb == "opt" and sub == "setinall" and words[4] ~= nil and words[5] ~= nil then
		say("%s", M.set_in_all(words[4], table.concat(words, " ", 5)))
	elseif verb == "nav" and sub == "load" and words[4] ~= nil then
		local text = E.read_file("routes", rest4) or E.read_file("routes", rest4 .. ".nav")
		if text == nil then
			E.note("meta: no route %s", rest4)
		else
			local ok, r = pcall(navlib.load, text)
			if ok then
				E.set_route(r, rest4)
			else
				E.note("meta: route %s does not read: %s", rest4, tostring(r))
			end
		end
	elseif verb == "nav" and sub == "clear" then
		E.set_route(nil)
	elseif verb == "nav" and sub == "save" and words[4] ~= nil then
		E.note("meta: %s", routes.edit(E, "save", { "", "", rest4 }))
	elseif verb == "loot" and sub == "load" and words[4] ~= nil then
		S.set("LootProfile", with_ext(rest4, ".utl"))
		E.load_files()
		say("loot profile %s", S.get("LootProfile"))
	elseif verb == "loot" and sub == "save" and words[4] ~= nil then
		say("%s", save_copy("profiles", S.get("LootProfile"), rest4, ".utl"))
	elseif verb == "meta" and sub == "load" and words[4] ~= nil then
		M.switch(with_ext(rest4, ".met"))
	elseif verb == "meta" and sub == "save" and words[4] ~= nil then
		say("%s", save_copy("metas", S.get("Meta"), rest4, ".met"))
	elseif verb == "addnavpt" then
		E.note("meta: %s", routes.edit(E, "add", { "", "", rest3 }))
	elseif verb == "addnavcheckpoint" then
		E.note("meta: %s", routes.edit(E, "checkpoint", {}))
	elseif verb == "addnavjump" then
		local w = { "", "" }
		for i = 3, #words do
			table.insert(w, words[i])
		end
		E.note("meta: %s", routes.edit(E, "jump", w))
	elseif verb == "reverseroute" then
		-- With true or false, the route runs reversed or as it was made; alone, turned round.
		if E.route ~= nil then
			if sub == "true" or sub == "false" then
				routes.reverse(E.route, sub == "true")
			else
				routes.reverse(E.route)
			end
		end
		E.note("meta: route reversed %s", tostring(E.route ~= nil and E.route.reversed == true))
	elseif verb == "reverseroutequery" then
		say("route reversed %s", tostring(E.route ~= nil and E.route.reversed == true))
	elseif verb == "jump" or verb == "tapjump" then
		local j = verb == "tapjump" and { heading = nil, shift = false, ms = 0 } or routes.jump_of(words, 3)
		if j == nil then
			E.note("meta: /vt jump <heading> <true|false> <ms> [forward|strafeleft|straferight]")
			return true
		end
		task.spawn(function()
			local ok, why = pawait(nav.jump({ heading = j.heading, walk = j.shift, ms = j.ms, direction = j.direction }))
			E.note(ok and "meta: jumped" or "meta: jump: %s", tostring(why))
		end)
	elseif verb == "settings" and sub == "load" and words[4] ~= nil then
		local text = E.read_file("profiles", rest4) or E.read_file("profiles", rest4 .. ".usd")
		if text == nil or M.import == nil then
			E.note("meta: no settings profile %s", rest4)
		else
			E.note("meta: settings %s: %s", rest4, M.import(text))
		end
	elseif verb == "settings" and sub == "loadchar" and words[4] ~= nil then
		local name = char_folder() .. with_ext(rest4, ".usd")
		local text = files.data(name)
		if text == nil then
			say("no settings profile %s", name)
		else
			say("settings %s: %s", name, M.import(text))
		end
	elseif verb == "settings" and (sub == "save" or sub == "savechar") and words[4] ~= nil then
		-- Written in a task: the spell book is read first where it has not been, for the buffs.
		local name = (sub == "savechar" and char_folder() or "") .. with_ext(rest4, ".usd")
		task.spawn(function()
			local ok, why = pcall(function()
				return files.save_data(name, M.export())
			end)
			say(ok and "saved %s" or "could not save %s: %s", name, tostring(why))
			if E.on_files then
				E.on_files()
			end
		end)
	elseif verb == "start" then
		-- The macro's own on and off: a pause that keeps the run, the meta running on, as VTank's
		-- switch does.
		if not E.resume("a meta's /vt start") and not E.running and E.on_start ~= nil then
			E.on_start()
		end
	elseif verb == "stop" then
		E.pause("a meta's /vt stop")
	elseif verb == "forcebuff" then
		E.force_buff()
	elseif verb == "cancelforcebuff" then
		E.note("%s", buffs.cancel_force())
	elseif verb == "setmetastate" and words[3] ~= nil then
		say("%s", M.set_state(rest3))
	elseif verb == "fakedeath" then
		-- The meta's Character Death condition, as though the character had died.
		if M.meta ~= nil then
			M.meta.events.death = true
		end
		E.logk("ActiveRule", "meta: a death, faked")
	elseif verb == "deletemonster" then
		local o = game.selection()
		if o == nil then
			say("select the monster first")
		else
			E.ghost(o)
			say("%s is left alone until the client lets it go", tostring(o.name))
		end
	elseif verb == "equipitemsfor" and words[3] ~= nil then
		task.spawn(function()
			say("%s", E.equip_step_for(rest3))
		end)
	elseif verb == "setattackbar" and tonumber(words[3]) ~= nil then
		-- The power bar's place, kept as the client keeps its bar's.
		local v = math.max(0, math.min(1, tonumber(words[3])))
		S.set("AttackPower", math.floor(v * 100 + 0.5))
		E.note("attack bar at %d%%", S.get("AttackPower"))
	elseif verb == "mexec" then
		say("%s", M.exec(rest3))
	elseif verb == "echo" then
		chat.print(rest3)
	elseif verb == "refresh" then
		if E.on_files then
			E.on_files()
		end
	elseif verb == "testitem" or verb == "propertydump" then
		local o = game.selection()
		if o == nil then
			say("select the item first")
			return true
		end
		task.spawn(function()
			if not o.appraised then
				pawait(act.appraise(o.id))
			end
			if verb == "testitem" then
				say("%s", test_item(o))
			else
				say_all(property_dump(o))
			end
		end)
	elseif verb == "testmonster" then
		local o = game.selection()
		if o == nil then
			say("select the monster first")
		else
			say_all(test_monster(o))
		end
	elseif verb == "testspell" and tonumber(words[3]) ~= nil then
		say("%s", test_spell(tonumber(words[3])))
	elseif verb == "testpet" then
		-- Whether a pet would be summoned here now, and from what.
		task.spawn(function()
			local why, it, element, monster = M.E.test_pet()
			if why ~= nil then
				say("no pet now: %s", why)
			else
				say("would summon from %s (%s) for %s", it.name, tostring(element), monster.name)
			end
		end)
	elseif verb == "dumpspells" or verb == "dumpspecies" or verb == "dumpmats" or verb == "dumpskills" then
		dump(string.sub(verb, 5))
	elseif verb == "listmonstervariables" then
		say("monster variables: %s", table.concat(monsters.VARIABLES, ", "))
	elseif verb == "dumpmetavars" then
		local lines = {}
		for k, v in pairs(M.meta and M.meta.vars or {}) do
			table.insert(lines, string.format("%s = %s", tostring(k), show(v)))
		end
		table.sort(lines)
		if #lines == 0 then
			lines = { "no meta variables are set" }
		end
		say_all(lines)
	elseif verb == "listmetafunctions" then
		local fns = {}
		for k in pairs(X.functions) do
			table.insert(fns, k)
		end
		table.sort(fns)
		local lines = {}
		for i = 1, #fns, 8 do
			table.insert(lines, table.concat(fns, ", ", i, math.min(i + 7, #fns)))
		end
		table.insert(lines, 1, string.format("%d meta functions:", #fns))
		say_all(lines)
	elseif verb == "metafunchelp" and words[3] ~= nil then
		local fn = string.lower(words[3])
		say(X.functions[fn] ~= nil and "%s is a meta function this client answers (its arguments: see the plugin guide's automation page)"
			or "%s is not a meta function this client answers", fn)
	elseif verb == "log" then
		local kind = E.log_type(words[3])
		local turn = string.lower(words[4] or "")
		if kind == nil and words[3] ~= nil then
			say("no log type %s", words[3])
		elseif kind ~= nil and (turn == "on" or turn == "off") then
			local off = {}
			for _, t in ipairs(E.LOG_TYPES) do
				if (t == kind and turn == "off") or (t ~= kind and not E.logging(t)) then
					table.insert(off, t)
				end
			end
			S.set("LogOff", table.concat(off, ", "))
		end
		local states = {}
		for _, t in ipairs(E.LOG_TYPES) do
			table.insert(states, t .. (E.logging(t) and " on" or " off"))
		end
		say("log: %s", table.concat(states, ", "))
	else
		E.note("meta: a VTank command this macro does not answer: %s", line)
	end
	return true
end

-- VTank's /vt opt setinall: the option set now and in every settings profile in the data folder
-- (its own row of each one's settings changed, or added, the rest of the file as it was).
-- Answers what was done.
function M.set_in_all(name, text)
	local ok, why = S.set(name, text)
	if not ok then
		return tostring(why)
	end
	local value = S.name(name) and S.get(name) or typed(text)
	local done, failed = 0, {}
	for _, f in ipairs(files.data_list()) do
		if string.find(string.lower(f), "%.usd$") then
			local new, err = M.set_in_text(files.data(f) or "", S.name(name) or name, value)
			local saved = new ~= nil and pcall(files.save_data, f, new)
			if saved then
				done = done + 1
			else
				table.insert(failed, f .. (err and (" (" .. err .. ")") or ""))
			end
		end
	end
	return string.format("%s = %s, and in %d settings profiles%s", S.name(name) or name, tostring(S.get(name)), done,
		#failed > 0 and ("; not in " .. table.concat(failed, ", ")) or "")
end

-- VTank's /vt mexec: an expression run with the meta's variables (a meta's own, when one runs).
-- Answers its value, or the error.
local plain_env = nil
function M.exec(text)
	local env = M.meta and M.meta.env
	if env == nil then
		plain_env = plain_env or X.env()
		env = plain_env
	end
	local ok, v = pcall(function()
		return X.compile(text)(env)
	end)
	return ok and ("mexec: " .. show(v)) or ("mexec: error: " .. tostring(v))
end

-- VTank's /vt setmetastate: the meta into a state of its. Answers what was done.
function M.set_state(state)
	if M.meta == nil then
		return "no meta is loaded"
	end
	if M.meta.states[state] == nil then
		return "the meta has no state " .. state
	end
	M.meta:set_state(state)
	return "meta state " .. state
end

-- VTank's /vt meta load: the meta named in place of the one running; the old one's rules stop at
-- once, even mid-check.
function M.switch(name)
	S.set("Meta", name)
	M.E.load_files()
	if M.E.running then
		M.start()
	end
	if M.E.on_files then
		M.E.on_files()
	end
end

-- The options a meta of the macro's is loaded with.
local function load_opts()
	return {
		engine = engine(),
		need_to_buff = function()
			return buffs.needed(false) ~= nil
		end,
	}
end

-- The meta's rules as data (`dereth.meta`'s `read_met`), as the meta editor changes them.
M.file = nil

-- Load a meta's text (`name` for the log) to run with the macro; or, with `text` a table, a meta's
-- rules as data.
function M.load(text, name)
	M.stop()
	local ok, file = true, text
	if type(text) == "string" then
		ok, file = pcall(metalib.read_met, text)
	end
	if not ok then
		return nil, tostring(file)
	end
	local m
	ok, m = pcall(metalib.load_met, file, load_opts())
	if not ok then
		return nil, tostring(m)
	end
	M.meta, M.name, M.file = m, name, file
	-- The editor starts afresh on the meta loaded.
	require("metaedit").new_rule(m.state)
	m:on_option(function(option, value)
		local done, why = S.set(option, value, false)
		M.E.logk("ActiveRule", done and "meta: %s = %s%s" or "meta: %s: %s", option,
			done and tostring(S.get(option)) or tostring(why), done and why and (" (" .. why .. ")") or "")
	end)
	m:on_state(function(new, old)
		M.E.meta_state = new
		M.E.logk("ActiveRule", "meta: %s -> %s", old, new)
	end)
	M.E.meta_state = m.state
	return m
end

-- The rules of state `state` in `file`, in order (as the meta numbers them within the state).
local function rules_of(file, state)
	local out = {}
	for _, r in ipairs(file and file.rules or {}) do
		if tostring(r.state) == state then
			table.insert(out, r)
		end
	end
	return out
end

-- The meta editor's change: `file`'s rules become the meta's at once, its state, variables, views
-- and watchdog kept, and the rules of its state that already ran in this stay (those unchanged)
-- still counted as run; and the meta is saved, `metas/<its name>` in the data folder. Answers
-- what was done.
function M.apply(file)
	local ME = require("metaedit")
	local name = M.name
	if name == nil or name == "" then
		return "no meta is loaded"
	end
	local ok, text = pcall(metalib.write_met, file)
	if not ok then
		return "the meta does not write: " .. tostring(text)
	end
	local m2
	ok, m2 = pcall(metalib.load_met, file, load_opts())
	if not ok then
		return "the meta does not load: " .. tostring(m2)
	end
	local m = M.meta
	if m == nil then
		M.load(file, name)
	else
		local ran = {}
		for i, r in ipairs(rules_of(M.file, m.state)) do
			if m.ran[i] then
				ran[ME.key(r)] = true
			end
		end
		m.states = m2.states
		m.states[m.state] = m.states[m.state] or {}
		m.ran = {}
		for i, r in ipairs(rules_of(file, m.state)) do
			if ran[ME.key(r)] then
				m.ran[i] = true
			end
		end
		m.unsupported = m2.unsupported
		M.file = file
	end
	local saved, why = pcall(files.save_data, "metas/" .. name, text)
	if #m2.unsupported > 0 then
		M.E.note("meta %s: not carried: %s", name, table.concat(m2.unsupported, "; "))
	end
	return saved and ("saved metas/" .. name) or ("could not save metas/" .. name .. ": " .. tostring(why))
end

-- A new meta, `name` (a `.met` file in the data folder), with no rules: the macro's meta from now.
function M.new_meta(name)
	name = with_ext(name, ".met")
	local file = { rules = {} }
	local ok, why = pcall(files.save_data, "metas/" .. name, metalib.write_met(file))
	if not ok then
		return "could not save metas/" .. name .. ": " .. tostring(why)
	end
	S.set("Meta", name)
	M.load(file, name)
	if M.E.running then
		M.start()
	end
	return "a new meta, metas/" .. name
end

function M.start()
	if M.meta ~= nil and S.get("EnableMeta") then
		M.meta:start()
		-- A first check at once, before the macro acts.
		M.meta:check()
	end
end

function M.stop()
	if M.meta ~= nil then
		M.meta:stop()
	end
end

-- A check as the macro decides what to do (the meta's timer checks it too), at most every tenth
-- of a second.
local decided = nil
function M.decide()
	if M.meta ~= nil and M.meta:running() and (decided == nil or os.clock() - decided >= 0.1) then
		decided = os.clock()
		M.meta:check()
	end
end

-- The macro resumed: "seconds in state" counts from now, as VTank's resets when the macro is
-- turned off and on ("seconds in state (P)" runs on).
function M.resumed()
	if M.meta ~= nil then
		M.meta:restart_clock()
	end
end

return M
