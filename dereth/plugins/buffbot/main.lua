-- Buffbot: buffs other players on request. A player tells the character the name of one of its
-- buff profiles ("buffs", "melee", "missile", "mage"); the request joins a line, the player is told
-- its place, and its buffs are cast on it in turn (see bot.lua for how).
--
-- Tells it answers: help (what to tell it), list (each profile's spells), cancel (leave the line)
-- and a profile's name (its buffs).
--
-- /buffbot on | off            answer requests, or stop (kept with the character, so a client
--                              that logs in again carries on)
-- /buffbot status              what it is doing, the line, and the counts
-- /buffbot profile <name> <spell family, ...>   set a profile (with no families: remove it)
-- /buffbot profiles            each profile and the spells it casts now
-- /buffbot set <setting> <value>   change a setting; /buffbot settings lists them
-- /buffbot components          the components held
-- /buffbot restock             buy what the Restock setting lists now, and come back
-- /buffbot post                its post is where it stands (else where it was first turned on)
-- /buffbot clear               empty the line
-- /buffbot self [profile]      cast your own buffs now, by a self profile (else SelfProfile's)
-- /buffbot self stop           stop casting them
-- /buffbot selfprofile <name> <spell family, ...>   set a self profile (with none: remove it)
-- /buffbot selfprofiles        each self profile, what it casts now and the time left on each
--
-- Everything is in its window too: Run (on and off, the line, the log), Profiles, Self (your own
-- buffs: their profiles, and the time left on each), Settings and Components.

local B = require("bot")

local function settings_rows()
	local names = {}
	for k in pairs(B.DEFAULTS) do
		table.insert(names, k)
	end
	table.sort(names)
	return names
end

local win = ui.window({
	id = "main",
	title = "Buffbot",
	width = 56,
	height = 26,
	ui.tabs({
		id = "tabs",
		ui.group({
			title = "Run",
			ui.row({
				ui.toggle({ id = "on", text = "Answer requests", value = false }),
				ui.button({ id = "clear", text = "Empty the line" }),
				ui.button({ id = "post", text = "Keep this place as its post" }),
			}),
			ui.label({ id = "state", text = "Off." }),
			ui.label({ id = "counts", text = "" }),
			ui.list({ id = "queue", columns = { "Waiting", "Profile" }, rows = {}, height = 4 }),
			ui.list({ id = "log", columns = { "Log" }, rows = {}, grow = 1 }),
		}),
		ui.group({
			title = "Profiles",
			ui.list({ id = "profiles", columns = { "Profile", "Spells" }, rows = {}, grow = 1 }),
			ui.input({ id = "pname", text = "Name", value = "", placeholder = "buffs" }),
			ui.input({ id = "pspells", text = "Spells", value = "", placeholder = "Strength Other, Armor Other" }),
			ui.row({
				ui.button({ id = "psave", text = "Save" }),
				ui.button({ id = "premove", text = "Remove" }),
				ui.button({ id = "pread", text = "Read the spell book again" }),
			}),
		}),
		ui.group({
			title = "Self",
			ui.row({
				ui.input({ id = "SelfProfile", text = "Profile", value = "", placeholder = "mage", grow = 1 }),
				ui.toggle({ id = "SelfBuff", text = "Keep them up", value = false }),
			}),
			ui.number({ id = "SelfRebuffMinutes", text = "Cast again under this many minutes left", value = 5, min = 0, max = 60, step = 1 }),
			ui.row({
				ui.button({ id = "selfnow", text = "Buff now" }),
				ui.button({ id = "selfstop", text = "Stop" }),
			}),
			ui.label({ id = "selfstate", text = "" }),
			ui.list({ id = "selfbuffs", columns = { "Buff", "Left" }, rows = {}, grow = 1 }),
			ui.list({ id = "selfprofiles", columns = { "Self profile", "Buffs" }, rows = {}, height = 4 }),
			ui.input({ id = "sname", text = "Name", value = "", placeholder = "mage" }),
			ui.input({ id = "sspells", text = "Buffs", value = "", placeholder = "Focus Self, Willpower Self, Defender" }),
			ui.row({
				ui.button({ id = "ssave", text = "Save" }),
				ui.button({ id = "sremove", text = "Remove" }),
			}),
		}),
		ui.group({
			title = "Settings",
			ui.number({ id = "Range", text = "Range (m)", value = 20, min = 1, max = 60, step = 1 }),
			ui.number({ id = "ComeCloserSeconds", text = "Wait for one out of range (s)", value = 60, min = 5, max = 600, step = 5 }),
			ui.number({ id = "QueueMax", text = "Longest line", value = 10, min = 1, max = 50, step = 1 }),
			ui.number({ id = "PerPlayerCap", text = "Requests a player may make an hour", value = 4, min = 1, max = 60, step = 1 }),
			ui.number({ id = "Retries", text = "Tries a spell gets", value = 3, min = 1, max = 10, step = 1 }),
			ui.number({ id = "MaxLevel", text = "Highest spell level", value = 7, min = 1, max = 8, step = 1 }),
			ui.number({ id = "SkillMargin", text = "Skill over a spell's difficulty", value = 10, min = 0, max = 100, step = 5 }),
			ui.number({ id = "ManaRest", text = "Rest under this mana (%)", value = 30, min = 0, max = 100, step = 5 }),
			ui.number({ id = "ManaReady", text = "Rest up to this mana (%)", value = 90, min = 0, max = 100, step = 5 }),
			ui.number({ id = "ComponentWarn", text = "Warn under this many of a component", value = 10, min = 0, max = 500, step = 5 }),
			ui.input({ id = "Caster", text = "Caster (empty: the first)", value = "" }),
			ui.input({ id = "Restock", text = "Restock (item=count@vendor, ...)", value = "", placeholder = "Prismatic Taper=200@Archmage Cindrue" }),
			ui.number({ id = "RestockAt", text = "Restock under this share of a count (%)", value = 25, min = 0, max = 100, step = 5 }),
		}),
		ui.group({
			title = "Components",
			ui.list({ id = "components", columns = { "Component", "Held" }, rows = {}, grow = 1 }),
		}),
	}),
})

local Self = B.self

local function refresh()
	win:get("on").value = B.get("On") == true
	win:get("state").text = B.state
	win:get("counts").text = string.format("%d buffed, %d casts, %d fizzles, %d spells not cast",
		B.served, B.casts, B.fizzles, B.failures)
	local q = {}
	if B.current ~= nil then
		table.insert(q, { B.current.name .. " (now)", B.current.profile })
	end
	for _, r in ipairs(B.queue) do
		table.insert(q, { r.name, r.profile })
	end
	win:get("queue").rows = q
	local log = {}
	for _, l in ipairs(B.log) do
		table.insert(log, { l })
	end
	win:get("log").rows = log
	local p = {}
	for _, name in ipairs(B.profile_names()) do
		table.insert(p, { name, table.concat(B.profiles[name], ", ") })
	end
	win:get("profiles").rows = p
	for _, k in ipairs(settings_rows()) do
		if k ~= "On" then
			win:get(k).value = B.get(k)
		end
	end
	local c = {}
	for _, it in ipairs(B.component_list()) do
		table.insert(c, { it.name, it.count })
	end
	win:get("components").rows = c
	local sp = {}
	for _, name in ipairs(Self.names()) do
		table.insert(sp, { name, table.concat(Self.profiles[name], ", ") })
	end
	win:get("selfprofiles").rows = sp
end

-- The Self tab's buffs: the chosen profile's, each with its time left; read again every couple
-- of seconds, as the time runs.
local function refresh_self()
	local profile = Self.asked and Self.asked.profile or Self.chosen()
	local rows = {}
	local state
	if profile == "" then
		state = "No self profile chosen."
	elseif Self.profiles[profile] == nil then
		state = "No self profile " .. profile .. "."
	elseif not B.book_ready() then
		state = "The spell book is read when your buffs are first cast."
	else
		local steps, missing = Self.plan(profile)
		for _, st in ipairs(steps) do
			table.insert(rows, { st.spell.name .. (st.item and (" on " .. tostring(st.item.name)) or ""), Self.left_text(st) })
		end
		for _, m in ipairs(missing) do
			table.insert(rows, { m, "" })
		end
		if Self.asked then
			state = "Casting " .. profile .. "."
		elseif B.get("SelfBuff") then
			state = "Keeping " .. profile .. " up."
		else
			state = profile .. "."
		end
	end
	win:get("selfbuffs").rows = rows
	win:get("selfstate").text = state
end

-- Changes reach the window a few times a second at most.
local dirty = true
B.on_change = function()
	dirty = true
end
local self_tick = 0
task.every(250, function()
	B.send_tells()
	if dirty then
		dirty = false
		refresh()
	end
	self_tick = self_tick + 1
	if self_tick % 8 == 1 then
		refresh_self()
	end
end)

local function status_lines()
	local lines = { "[Buffbot] " .. B.state }
	table.insert(lines, string.format("[Buffbot] %d buffed, %d casts, %d fizzles, %d spells not cast; %d waiting",
		B.served, B.casts, B.fizzles, B.failures, #B.queue))
	for i, r in ipairs(B.queue) do
		table.insert(lines, string.format("[Buffbot] %d. %s (%s)", i, r.name, r.profile))
	end
	return lines
end

local function turn(on)
	if on and B.restock.post() == nil then
		B.note("%s", B.restock.set_post())
	end
	B.note("%s", B.set("On", on))
	B.note(on and "answering requests" or "not answering requests")
end

local function profiles_lines()
	local out = {}
	for _, name in ipairs(B.profile_names()) do
		local spells, missing = B.plan(name)
		local names = {}
		for _, s in ipairs(spells) do
			table.insert(names, s.name)
		end
		table.insert(out, string.format("[Buffbot] %s: %s%s", name, #names > 0 and table.concat(names, ", ") or "nothing it can cast",
			#missing > 0 and (" (not: " .. table.concat(missing, ", ") .. ")") or ""))
	end
	return out
end

local function select_profile(row)
	local name = B.profile_names()[row]
	if name ~= nil then
		win:get("pname").value = name
		win:get("pspells").value = table.concat(B.profiles[name], ", ")
	end
end

local function select_self_profile(row)
	local name = Self.names()[row]
	if name ~= nil then
		win:get("sname").value = name
		win:get("sspells").value = table.concat(Self.profiles[name], ", ")
	end
end

local function self_profiles_lines()
	local out = {}
	for _, name in ipairs(Self.names()) do
		local parts = {}
		if B.book_ready() then
			local steps, missing = Self.plan(name)
			for _, st in ipairs(steps) do
				table.insert(parts, string.format("%s%s %s", st.spell.name, st.item and (" on " .. tostring(st.item.name)) or "", Self.left_text(st)))
			end
			for _, m in ipairs(missing) do
				table.insert(parts, "not: " .. m)
			end
		else
			parts = { table.concat(Self.profiles[name], ", ") }
		end
		table.insert(out, string.format("[Buffbot] self %s: %s", name, #parts > 0 and table.concat(parts, ", ") or "nothing"))
	end
	return out
end

on("ui.clicked", function(e)
	if e.window ~= "main" then
		return
	end
	if e.id == "clear" then
		B.queue = {}
		B.note("the line emptied")
	elseif e.id == "post" then
		B.note("%s", B.restock.set_post())
	elseif e.id == "psave" then
		B.note("%s", B.set_profile(win:get("pname").value, win:get("pspells").value))
	elseif e.id == "premove" then
		B.note("%s", B.set_profile(win:get("pname").value, ""))
	elseif e.id == "pread" then
		B.forget_book()
		B.note("the spell book is read again")
	elseif e.id == "selfnow" then
		B.note("%s", Self.ask(nil))
	elseif e.id == "selfstop" then
		B.note("%s", Self.stop())
	elseif e.id == "ssave" then
		B.note("%s", Self.set_profile(win:get("sname").value, win:get("sspells").value))
	elseif e.id == "sremove" then
		B.note("%s", Self.set_profile(win:get("sname").value, ""))
	end
	refresh()
	refresh_self()
end)

on("ui.changed", function(e)
	if e.window ~= "main" then
		return
	end
	if e.id == "on" then
		turn(e.value == true)
	elseif B.setting_name(e.id) ~= nil then
		B.note("%s", B.set(e.id, e.value))
	end
end)

on("ui.selected", function(e)
	if e.window == "main" and e.id == "profiles" and e.rows ~= nil then
		select_profile(e.rows[1])
	elseif e.window == "main" and e.id == "selfprofiles" and e.rows ~= nil then
		select_self_profile(e.rows[1])
	end
end)

on("chat.line", function(e)
	B.chat_line(e)
end)

on("context.begin", function()
	B.load()
	B.forget_book()
	refresh()
	if B.get("On") then
		B.note("answering requests (it was on when this character left)")
	end
end)

local USAGE = "/buffbot on | off | status | profiles | profile <name> <families> | self [profile | stop] | selfprofiles | selfprofile <name> <families> | set <setting> <value> | settings | components | restock | post | clear"

commands.add("buffbot", USAGE, function(args, words)
	local w = string.lower(words[1] or "status")
	if w == "on" or w == "off" then
		turn(w == "on")
	elseif w == "status" then
		for _, l in ipairs(status_lines()) do
			chat.print(l)
		end
	elseif w == "profiles" then
		for _, l in ipairs(profiles_lines()) do
			chat.print(l)
		end
	elseif w == "profile" then
		local name, families = string.match(args, "^%s*profile%s+(%S+)%s*(.*)$")
		chat.print("[Buffbot] " .. B.set_profile(name, families))
	elseif w == "set" and words[3] ~= nil then
		chat.print("[Buffbot] " .. B.set(words[2], table.concat(words, " ", 3)))
	elseif w == "settings" then
		for _, k in ipairs(settings_rows()) do
			chat.print(string.format("[Buffbot] %s = %s", k, tostring(B.get(k))))
		end
	elseif w == "components" then
		for _, c in ipairs(B.component_list()) do
			chat.print(string.format("[Buffbot] %s: %d", c.name, c.count))
		end
	elseif w == "post" then
		local line = B.restock.set_post()
		chat.print("[Buffbot] " .. line)
		B.note("%s", line)
	elseif w == "restock" then
		B.restock_asked = true
		chat.print("[Buffbot] restocking between players: " .. (B.get("Restock") ~= "" and B.get("Restock") or "nothing listed (/buffbot set Restock ...)"))
	elseif w == "clear" then
		B.queue = {}
		B.note("the line emptied")
	elseif w == "self" then
		local line
		if words[2] ~= nil and string.lower(words[2]) == "stop" then
			line = Self.stop()
		else
			line = Self.ask(words[2])
		end
		chat.print("[Buffbot] " .. line)
		B.note("%s", line)
	elseif w == "selfprofiles" then
		for _, l in ipairs(self_profiles_lines()) do
			chat.print(l)
		end
	elseif w == "selfprofile" then
		local name, families = string.match(args, "^%s*selfprofile%s+(%S+)%s*(.*)$")
		chat.print("[Buffbot] " .. Self.set_profile(name, families))
	else
		chat.print("[Buffbot] " .. USAGE)
	end
	refresh()
	refresh_self()
end)

task.spawn(B.run)
