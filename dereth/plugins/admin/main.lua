-- Admin: an administrator's chat commands as a window of forms, with the server's replies under
-- them. Only a character with the administrator flag (bool property 0x2C) has the window; for any
-- other character there is no window, no key and no button on the plugin bar, and /admin says so.
--
-- Each button sends the line an administrator would otherwise type, so the server checks it as it
-- checks a typed one. What is hard to undo takes a second press. A command that acts on the
-- selection first has the selection appraised, and is sent only once the server has appraised it,
-- so it acts on what is selected now and not on an older target.
--
-- /admin                opens or closes the window (Ctrl+Shift+A; /plugin key changes the key)

local L = require("lines")

local COMMANDS = L.read(files.package("commands.txt"))
local ADMIN_BOOL = 0x2C

-- How long a pressed dangerous button waits for the second press.
local ARMED_SECONDS = 4
-- How long a line of chat after a command or its last reply is taken as its reply.
local REPLY_SECONDS = 3
-- How long after a command with no reply the window says so.
local NO_REPLY_SECONDS = 5
-- How long the window waits for the list of players.
local LIST_SECONDS = 5
local MAX_REPLIES = 200
-- The kinds of line a server answers a command in.
local REPLY_KINDS = {
	default = true, system = true, magic = true, help = true, world_broadcast = true, error = true,
}

local LEVELS = { "Player", "Advocate", "Sentinel", "Envoy", "Developer", "Admin" }
local PROPERTY_KINDS = { Yes_No = "bool", Whole = "long", Decimal = "double", Text = "string" }
local PROPERTY_NAMES = { "Yes_No", "Whole", "Decimal", "Text" }
local ADJUST_KINDS = { Vital = "modifyvital", Skill = "modifyskill", Attribute = "modifyattr" }

local win = nil -- the window, once an administrator has had it
local full = false -- whether the window holds the forms (else only the notice)
local open = false
local armed = nil -- { id, key, until }
local sent = nil -- { line, at, last, replies }
local listing = nil -- { names, until } while @listplayers is awaited
local online = {}
local location_wanted = false
local recent = {}
local chosen = nil -- the Commands tab's command

-- Whether the character has the administrator flag.
local function is_admin()
	local id = game.player and game.player.id
	if id == nil then
		return false
	end
	local me = game.world:get(id)
	return me ~= nil and me:bool(ADMIN_BOOL) == true
end

-- Building the window --------------------------------------------------------------------------

local function b(id, text)
	return ui.button({ id = id, text = text })
end

local function input(id, text, placeholder, width)
	return ui.input({ id = id, text = text, value = "", placeholder = placeholder or "", width = width })
end

local function group_options()
	local o = { "All" }
	for _, g in ipairs(L.GROUPS) do
		table.insert(o, g)
	end
	return o
end

local function forms()
	return ui.window({
		id = "main",
		title = "Admin",
		width = 76,
		height = 34,
		resizable = true,
		ui.tabs({
			id = "tabs",
			ui.group({
				title = "Players",
				ui.row({ input("name", "Name", "Type a name, or pick one", 30), b("refresh", "Refresh"), b("use_selected", "Use Selected") }),
				ui.list({ id = "online", columns = { "Online" }, rows = {}, height = 5 }),
				ui.row({ b("finger", "Who Is This"), b("teleto", "Go To"), b("teletome", "Bring Here"), b("telereturn", "Send Back") }),
				ui.row({ b("buff", "Buff"), b("fellowbuff", "Fellowship Buff"), input("amount", "Amount", "", 10), b("grantxp", "Give XP"), b("grantluminance", "Give Luminance") }),
				ui.row({ ui.choice({ id = "level", text = "Access", options = LEVELS, value = "Player" }), b("set_access", "Set Access"), b("watchmen", "List Staff") }),
				ui.group({
					title = "Moderation",
					ui.row({ b("gag", "Gag"), b("ungag", "Ungag"), b("forcelogout", "Force Logoff") }),
					ui.row({ input("boot_reason", "Reason", "Reason for booting", 30), b("boot", "Boot") }),
					ui.row({ input("account", "Account", "", 14), input("days", "Days", "", 4), input("hours", "Hours", "", 4), input("minutes", "Mins", "", 4) }),
					ui.row({ input("ban_reason", "Reason", "Reason for banning", 30), b("ban", "Ban"), b("banlist", "Ban List"), b("unban", "Unban") }),
					ui.row({ input("new_name", "New name", "", 24), b("rename", "Rename") }),
				}),
			}),
			ui.group({
				title = "Travel",
				ui.row({ input("coords", "Coordinates", "37.3S, 67W", 24), b("tele", "Go") }),
				ui.row({ input("poi", "Point of interest", "A place's name", 24), b("telepoi", "Go"), b("telepoi_list", "List") }),
				ui.row({ input("dungeon", "Dungeon", "Name or landblock", 24), b("teledungeon", "Go") }),
				ui.row({ input("location", "Exact location", "0x7F0401AD [12.3 -28.4 0.0]", 30), b("teleloc", "Go"), b("loc", "Fill From Where I Am") }),
				ui.row({ b("teleallto", "Bring All Players Here") }),
				ui.group({
					title = "Recall",
					ui.row({ ui.label({ text = "Sanctuary" }), b("home", "Go"), b("save", "Set Here") }),
					ui.row({ ui.number({ id = "slot", text = "Slot", value = 1, min = 1, max = 9, step = 1 }), b("home_slot", "Go"), b("save_slot", "Set Here") }),
				}),
			}),
			ui.group({
				title = "Target",
				ui.label({ id = "selection", text = "Nothing is selected" }),
				ui.group({
					title = "Inspect",
					ui.row({ b("propertydump", "Properties"), b("getinfo", "Info"), b("showstats", "Stats"), b("targetloc", "Location"), b("dist", "Distance") }),
					ui.row({ b("getenchantments", "Enchantments"), b("resist-info", "Resistances"), b("generatordump", "Generator"), b("vendordump", "Vendor") }),
				}),
				ui.group({
					title = "Act On It",
					ui.row({ b("heal", "Heal"), b("turnto", "Turn To Me"), b("movetome", "Move To Me"), b("crack", "Crack"), b("regen", "Regen"), b("smite", "Smite"), b("delete", "Delete") }),
				}),
				ui.row({ input("property", "Property", "PropertyInt.Name", 24), b("getproperty", "Read") }),
				ui.row({ input("property_value", "Value", "", 24), b("setproperty", "Write") }),
				ui.row({
					ui.choice({ id = "adjust_kind", text = "Adjust", options = { "Vital", "Skill", "Attribute" }, value = "Vital" }),
					input("adjust_name", "Name", "Exact name", 16), input("adjust_by", "By", "", 6), b("adjust", "Apply"),
				}),
			}),
			ui.group({
				title = "Myself",
				ui.row({ ui.label({ text = "Cloak" }), b("cloak_on", "On"), b("cloak_off", "Off"), b("cloak_player", "Player"), b("cloak_creature", "Creature") }),
				ui.row({ ui.label({ text = "Never Die" }), b("neversaydie_on", "On"), b("neversaydie_off", "Off") }),
				ui.row({ ui.label({ text = "Attackable" }), b("attackable_on", "On"), b("attackable_off", "Off") }),
				ui.row({ ui.label({ text = "Vision" }), b("adminvision_on", "On"), b("adminvision_off", "Off") }),
				ui.row({ ui.label({ text = "Run Boost" }), b("run_on", "On"), b("run_off", "Off") }),
				ui.row({ ui.label({ text = "Bypass" }), b("portal_bypass", "Portals"), b("mrt", "Housing") }),
				ui.row({ b("god", "God"), b("ungod", "Ungod"), b("buff_me", "Buff Me"), b("heal_me", "Heal Me"), b("dispel", "Dispel") }),
				ui.row({ ui.label({ text = "PK State" }), b("pk_npk", "None"), b("pk_pk", "PK"), b("pk_pkl", "PKL"), b("pk_free", "Free") }),
				ui.row({ ui.label({ text = "Faction" }), input("faction_rank", "Rank", "", 4), b("faction_none", "None"), b("faction_ch", "CH"), b("faction_ew", "EW"), b("faction_rb", "RB") }),
				ui.row({ b("addallspells", "All Spells"), b("addalltitles", "All Titles"), input("level_to", "Level", "", 6), b("delevel", "Delevel") }),
			}),
			ui.group({
				title = "Spawn",
				ui.row({ input("weenie", "Object", "Weenie class name or id", 24), input("spawn_amount", "Amount", "", 6), input("palette", "Palette", "", 6), input("shade", "Shade", "", 6) }),
				ui.row({ b("create", "In The World"), b("ci", "In My Pack"), input("custom_name", "Name", "Custom name", 18), b("createnamed", "Spawn Named") }),
				ui.list({ id = "recent", columns = { "Recent" }, rows = {}, height = 3 }),
				ui.row({ input("tier", "Loot tier", "", 4), input("loot_items", "Items", "", 4), b("ciloot", "Roll Into Pack") }),
				ui.row({ input("material", "Salvage", "Material", 12), input("structure", "Size", "", 4), input("workmanship", "Work", "", 4), b("cm", "Make Bag") }),
				ui.row({ b("weapons", "Weapons"), b("inv", "Sample Items"), b("splits", "Stackables"), b("comps", "Components"), b("food", "Food"), b("currency", "Currency") }),
			}),
			ui.group({
				title = "World",
				ui.row({ input("message", "Message everyone", "What everyone playing reads", 40), b("gamecast", "Send") }),
				ui.row({ b("save-world", "Save World"), b("world_open", "Open"), b("world_close", "Close"), b("world_close_boot", "Close And Boot") }),
				ui.row({ input("shutdown_delay", "Seconds", "300", 6), input("shutdown_message", "Message", "Shutdown message", 24), b("shutdown", "Shut Down"), b("cancel-shutdown", "Cancel Shutdown") }),
				ui.row({ b("serverstatus", "Server"), b("serverperformance", "Performance"), b("landblockperformance", "Landblocks"), b("netstats", "Network"), b("time", "Game Time"), b("reload-landblock", "Reload This Landblock") }),
				ui.row({ input("environment", "Environment", "Effect", 14), b("setlbenviron_list", "List"), b("setlbenviron", "Here"), b("setlbenviron_reset", "Reset Here"), b("setglobalenviron", "Everywhere"), b("setglobalenviron_reset", "Reset All") }),
				ui.row({ input("event", "Event", "Event name", 18), b("event_status", "Status"), b("event_start", "Start"), b("event_stop", "Stop") }),
				ui.row({ ui.choice({ id = "property_kind", text = "Server property", options = PROPERTY_NAMES, value = "Yes_No" }), b("showprops", "List All") }),
				ui.row({ input("server_property", "Property", "", 20), b("fetch", "Read"), input("server_value", "Value", "", 12), b("modify", "Write") }),
			}),
			ui.group({
				title = "Commands",
				ui.row({ input("search", "Search", "Search the commands", 30), ui.choice({ id = "group", text = "Group", options = group_options(), value = "All" }) }),
				ui.list({ id = "commands", columns = { { title = "Command", width = 24 }, "What it does" }, rows = {}, grow = 1 }),
				ui.label({ id = "usage", text = "Choose a command to run it" }),
				ui.row({ input("arguments", "Arguments", "", 40), b("run", "Run") }),
			}),
		}),
		ui.label({ id = "sent", text = "Replies from the server show here" }),
		ui.list({ id = "replies", columns = { "Reply" }, rows = {}, height = 4 }),
		ui.hotkey({ id = "key", command = "admin", text = "Admin window", key = "Ctrl+Shift+A" }),
	})
end

-- The window as a character without the flag sees it, if an administrator had it earlier.
local function notice()
	return ui.window({
		id = "main",
		title = "Admin",
		ui.label({ text = "Only a character with the administrator flag has this window." }),
	})
end

local function v(id)
	return L.trim(win:get(id).value)
end

local function notify(text)
	win:get("sent").text = text
	win:get("replies").rows = {}
end

local function show_replies()
	if sent == nil then
		return
	end
	local rows = {}
	for _, r in ipairs(sent.replies) do
		table.insert(rows, { r })
	end
	win:get("replies").rows = rows
end

local function show_online()
	local wanted = string.lower(v("name"))
	local rows = {}
	for _, n in ipairs(online) do
		if string.find(string.lower(n), wanted, 1, true) then
			table.insert(rows, { n })
		end
	end
	win:get("online").rows = rows
end

local function show_recent()
	local rows = {}
	for _, r in ipairs(recent) do
		table.insert(rows, { r })
	end
	win:get("recent").rows = rows
end

-- The Commands tab's list: the commands the search and the group leave.
local function show_commands()
	local wanted = string.lower(v("search"))
	local group = win:get("group").value
	local rows = {}
	for _, c in ipairs(COMMANDS) do
		if (group == "All" or c.group == group)
			and (string.find(string.lower(c.verb), wanted, 1, true) or string.find(string.lower(c.about), wanted, 1, true)) then
			table.insert(rows, { "@" .. c.verb, c.about })
		end
	end
	win:get("commands").rows = rows
	return rows
end

local function show_selection()
	local s = game.selection()
	if s == nil then
		win:get("selection").text = "Nothing is selected"
	else
		win:get("selection").text = string.format("%s   0x%08X", s.name or "", s.id)
	end
end

-- Sending ----------------------------------------------------------------------------------------

-- Send `lines`, one after another, as the player would type them.
local function send(lines)
	for _, line in ipairs(lines) do
		-- A plugin sends one line a second, three at once.
		if not pcall(chat.command, line) then
			notify("Not sent: too many commands at once. Press again in a moment.")
			return
		end
	end
	sent = { line = table.concat(lines, "  then  "), at = os.clock(), last = os.clock(), replies = {} }
	win:get("sent").text = sent.line
	win:get("replies").rows = {}
	local this = sent
	task.after(NO_REPLY_SECONDS * 1000, function()
		if sent == this and #this.replies == 0 then
			win:get("replies").rows = { { "No reply from the server" } }
		end
	end)
end

-- Send `lines` about the selection: appraised first, and sent only if the server appraised it.
local function send_on(lines, target)
	act.appraise(target):next(function(a)
		if a ~= nil and a.success then
			send(lines)
		else
			notify("The server could not appraise the selection, so nothing was sent.")
		end
	end, function(why)
		notify("The selection was not appraised (" .. tostring(why) .. "), so nothing was sent.")
	end)
end

-- The buttons ------------------------------------------------------------------------------------

local function name()
	return v("name")
end

-- `@verb <name>`, given a name.
local function with_name(verb)
	return function()
		if name() == "" then
			return nil, "Name a player first."
		end
		return "@" .. verb .. " " .. name()
	end
end

local function fixed(line)
	return function()
		return line
	end
end

local function needs(id, what, build)
	return function()
		if v(id) == "" then
			return nil, what
		end
		return build(v(id))
	end
end

-- Each button: what it sends (a function answering the line, a list of lines, or nil and why
-- not), whether it takes a second press, and whether it acts on the selection ("always", or
-- "if-any" for a command that acts on the player without one).
local ACTIONS = {
	refresh = { build = fixed("@listplayers"), list = true },
	finger = { build = with_name("finger") },
	teleto = { build = with_name("teleto") },
	teletome = { build = with_name("teletome") },
	telereturn = { build = with_name("telereturn") },
	-- With a level, or a name ending in a digit would lose the digit to it.
	buff = { build = function()
		if name() == "" then return nil, "Name a player first." end
		return "@buff " .. name() .. " 8"
	end },
	fellowbuff = { build = with_name("fellowbuff") },
	grantxp = { build = function()
		if name() == "" or not L.is_count(v("amount")) then return nil, "Name a player and an amount." end
		return "@grantxp " .. name() .. " " .. v("amount")
	end },
	grantluminance = { build = function()
		if name() == "" or not L.is_count(v("amount")) then return nil, "Name a player and an amount." end
		return "@grantluminance " .. name() .. " " .. v("amount")
	end },
	set_access = { confirm = true, build = function()
		if name() == "" then return nil, "Name a player first." end
		return "@set-characteraccess " .. L.one_word(name()) .. " " .. win:get("level").value
	end },
	watchmen = { build = fixed("@watchmen") },
	gag = { build = with_name("gag") },
	ungag = { build = with_name("ungag") },
	forcelogout = { confirm = true, build = with_name("forcelogout") },
	boot = { confirm = true, build = function()
		if name() == "" then return nil, "Name a player first." end
		-- The comma against the name is how the server finds where the name ends.
		if v("boot_reason") == "" then
			return "@boot char " .. name()
		end
		return "@boot char " .. name() .. ", " .. v("boot_reason")
	end },
	ban = { confirm = true, build = function()
		local span, total = {}, 0
		for _, id in ipairs({ "days", "hours", "minutes" }) do
			local t = v(id)
			local n = tonumber(t == "" and "0" or t)
			if n == nil or n < 0 then return nil, "Days, hours and minutes are numbers." end
			total = total + n
			table.insert(span, t == "" and "0" or t)
		end
		if v("account") == "" or total <= 0 then return nil, "Name an account and how long." end
		return L.trim("@ban " .. L.one_word(v("account")) .. " " .. table.concat(span, " ") .. " " .. v("ban_reason"))
	end },
	banlist = { build = fixed("@banlist") },
	unban = { build = needs("account", "Name an account first.", function(a) return "@unban " .. L.one_word(a) end) },
	rename = { confirm = true, build = function()
		if name() == "" or v("new_name") == "" then return nil, "Name a player and the new name." end
		return "@rename " .. name() .. ", " .. v("new_name")
	end },

	tele = { build = needs("coords", "Give the coordinates first.", function(c) return "@tele " .. c end) },
	telepoi = { build = needs("poi", "Name a place first.", function(p) return "@telepoi " .. p end) },
	telepoi_list = { build = fixed("@telepoi list") },
	teledungeon = { build = needs("dungeon", "Name a dungeon first.", function(d) return "@teledungeon " .. d end) },
	teleloc = { build = needs("location", "Give a location first.", function(l) return "@teleloc " .. l end) },
	loc = { build = fixed("@loc"), location = true },
	teleallto = { confirm = true, build = fixed("@teleallto") },
	home = { build = fixed("@home") },
	save = { build = fixed("@save") },
	home_slot = { build = function() return "@home " .. math.floor(win:get("slot").value) end },
	save_slot = { build = function() return "@save " .. math.floor(win:get("slot").value) end },

	getproperty = { selection = "always", build = needs("property", "Name a property first.", function(p) return "@getproperty " .. p end) },
	setproperty = { selection = "always", confirm = true, build = function()
		if v("property") == "" or v("property_value") == "" then return nil, "Name a property and its value." end
		return "@setproperty " .. v("property") .. " " .. L.one_word(v("property_value"))
	end },
	adjust = { selection = "always", confirm = true, build = function()
		local what, by = v("adjust_name"), v("adjust_by")
		if what == "" or string.find(what, " ", 1, true) or not string.match(by, "^-?%d+$") then
			return nil, "Give the exact name, one word, and a whole number to change it by."
		end
		return "@" .. ADJUST_KINDS[win:get("adjust_kind").value] .. " " .. what .. " " .. by
	end },

	god = { build = fixed("@god") },
	ungod = { build = fixed("@ungod") },
	buff_me = { build = fixed("@buff") },
	-- With something selected, the server heals that instead.
	heal_me = { build = function()
		if game.selection() ~= nil then return nil, "Clear the selection first: with one, the server heals that." end
		return "@heal"
	end },
	dispel = { build = fixed("@dispel") },
	portal_bypass = { build = fixed("@portal_bypass") },
	mrt = { build = fixed("@mrt") },
	addallspells = { build = fixed("@addallspells") },
	addalltitles = { build = fixed("@addalltitles") },
	delevel = { confirm = true, build = function()
		if not L.is_count(v("level_to")) then return nil, "Give the level first." end
		return "@delevel " .. v("level_to")
	end },

	create = { spawn = true, build = function() return "create" end },
	ci = { spawn = true, build = function() return "ci" end },
	createnamed = { build = function()
		if v("weenie") == "" or v("custom_name") == "" then return nil, "Name the object and what to call it." end
		local count = v("spawn_amount") == "" and "1" or v("spawn_amount")
		return "@createnamed " .. v("weenie") .. " " .. count .. " " .. v("custom_name")
	end, remember = true },
	ciloot = { build = function()
		if not L.is_count(v("tier")) then return nil, "Give the loot tier first." end
		return L.trim("@ciloot " .. v("tier") .. " " .. v("loot_items"))
	end },
	cm = { build = function()
		local bag = L.in_order({ v("structure"), v("workmanship") })
		if v("material") == "" or bag == nil then return nil, "Name the material, and the size before the work." end
		return L.trim("@cm " .. v("material") .. " " .. bag)
	end },

	gamecast = { build = needs("message", "Write the message first.", function(m) return "@gamecast " .. m end), clears = "message" },
	world_open = { confirm = true, build = fixed("@world open") },
	world_close = { confirm = true, build = fixed("@world close") },
	world_close_boot = { confirm = true, build = fixed("@world close boot") },
	-- The delay is sent each time: the server keeps whatever it was last told.
	shutdown = { confirm = true, build = function()
		local delay = v("shutdown_delay") == "" and "300" or v("shutdown_delay")
		if not L.is_count(delay) or #delay > 5 then return nil, "The delay is a number of seconds." end
		return { "@set-shutdown-interval " .. delay, L.trim("@shutdown " .. v("shutdown_message")) }
	end },
	["reload-landblock"] = { confirm = true, build = fixed("@reload-landblock") },
	setlbenviron_list = { build = fixed("@setlbenviron list") },
	setlbenviron = { build = needs("environment", "Name the effect first.", function(e) return "@setlbenviron " .. e end) },
	setlbenviron_reset = { build = fixed("@setlbenviron") },
	setglobalenviron = { confirm = true, build = needs("environment", "Name the effect first.", function(e) return "@setglobalenviron " .. e end) },
	setglobalenviron_reset = { confirm = true, build = fixed("@setglobalenviron") },
	event_status = { build = needs("event", "Name the event first.", function(e) return "@event status " .. L.one_word(e) end) },
	event_start = { build = needs("event", "Name the event first.", function(e) return "@event start " .. L.one_word(e) end) },
	event_stop = { build = needs("event", "Name the event first.", function(e) return "@event stop " .. L.one_word(e) end) },
	showprops = { build = fixed("@showprops") },
	fetch = { build = needs("server_property", "Name the property first.", function(p)
		return "@fetch" .. PROPERTY_KINDS[win:get("property_kind").value] .. " " .. p
	end) },
	modify = { confirm = true, build = function()
		if v("server_property") == "" or v("server_value") == "" then return nil, "Name the property and its value." end
		return "@modify" .. PROPERTY_KINDS[win:get("property_kind").value] .. " " .. v("server_property") .. " " .. L.one_word(v("server_value"))
	end },
}

-- The buttons that send a command alone, by its verb: the Target tab's, the World tab's status
-- and the Spawn tab's kits.
for _, verb in ipairs({ "propertydump", "getinfo", "showstats", "targetloc", "dist", "getenchantments",
	"resist-info", "generatordump", "vendordump", "turnto", "movetome", "crack", "regen" }) do
	ACTIONS[verb] = { selection = "always", build = fixed("@" .. verb) }
end
ACTIONS.heal = { selection = "if-any", build = fixed("@heal") }
ACTIONS.smite = { selection = "always", confirm = true, build = fixed("@smite") }
ACTIONS.delete = { selection = "always", confirm = true, build = fixed("@delete") }
for _, verb in ipairs({ "save-world", "cancel-shutdown", "serverstatus", "serverperformance",
	"landblockperformance", "netstats", "time", "weapons", "inv", "splits", "comps", "food", "currency" }) do
	ACTIONS[verb] = { build = fixed("@" .. verb) }
end
-- The buttons for a word after a verb: `cloak_on` sends `@cloak on`.
for _, pair in ipairs({
	{ "cloak", { "on", "off", "player", "creature" } }, { "neversaydie", { "on", "off" } },
	{ "attackable", { "on", "off" } }, { "adminvision", { "on", "off" } }, { "run", { "on", "off" } },
	{ "pk", { "npk", "pk", "pkl", "free" } },
}) do
	for _, word in ipairs(pair[2]) do
		ACTIONS[pair[1] .. "_" .. word] = { build = fixed("@" .. pair[1] .. " " .. word) }
	end
end
for _, word in ipairs({ "none", "ch", "ew", "rb" }) do
	ACTIONS["faction_" .. word] = { build = function()
		if word == "none" or v("faction_rank") == "" then
			return "@faction " .. word
		end
		return "@faction " .. word .. " " .. v("faction_rank")
	end }
end

-- The Commands tab's Run: the chosen command with what was typed after it.
local function run_chosen()
	if chosen == nil then
		return nil, "Choose a command first."
	end
	return L.trim("@" .. chosen.verb .. " " .. v("arguments"))
end

-- The Spawn tab's two spawns: `@create` or `@ci` with the object and what follows it.
local function spawn_line(verb)
	local details = L.in_order({ v("spawn_amount"), v("palette"), v("shade") })
	if v("weenie") == "" or details == nil then
		return nil, "Name the object, and each detail before the next."
	end
	return L.trim("@" .. verb .. " " .. v("weenie") .. " " .. details)
end

local function remember_spawn()
	local w = v("weenie")
	for i = #recent, 1, -1 do
		if recent[i] == w then
			table.remove(recent, i)
		end
	end
	table.insert(recent, 1, w)
	while #recent > 5 do
		table.remove(recent)
	end
	show_recent()
end

local function disarm()
	if armed ~= nil then
		win:get(armed.id).text = armed.label
		armed = nil
	end
end

local function press(id)
	local a = ACTIONS[id]
	local selection, confirm = nil, false
	local lines, why
	if id == "run" then
		lines, why = run_chosen()
		if chosen ~= nil then
			confirm = chosen.confirm
			selection = chosen.selection
			-- With words after it, the command names what it acts on.
			if selection == "always" and v("arguments") ~= "" then
				selection = nil
			end
		end
	elseif a == nil then
		return
	elseif a.spawn then
		lines, why = spawn_line(a.build())
	else
		lines, why = a.build()
		confirm = a.confirm == true
		selection = a.selection
	end
	if lines == nil then
		disarm()
		notify(why or "")
		return
	end
	if type(lines) == "string" then
		lines = { lines }
	end
	local target = nil
	local s = game.selection()
	if selection == "always" then
		if s == nil then
			disarm()
			notify("Select something first: this command acts on the selection.")
			return
		end
		target = s.id
	elseif selection == "if-any" and s ~= nil then
		target = s.id
	end
	-- A dangerous button takes a second press of the same command on the same selection.
	if confirm then
		local key = table.concat(lines, "\n") .. "|" .. tostring(target)
		if armed == nil or armed.id ~= id or armed.key ~= key or os.clock() > armed["until"] then
			disarm()
			armed = { id = id, key = key, label = win:get(id).text, ["until"] = os.clock() + ARMED_SECONDS }
			win:get(id).text = "Press Again"
			local this = armed
			task.after(ARMED_SECONDS * 1000, function()
				if armed == this then
					disarm()
				end
			end)
			return
		end
	end
	disarm()
	if a ~= nil and a.list then
		listing = { names = {}, ["until"] = os.clock() + LIST_SECONDS }
	end
	if a ~= nil and a.location then
		location_wanted = true
	end
	if target ~= nil then
		send_on(lines, target)
	else
		send(lines)
	end
	if a ~= nil and (a.spawn or a.remember) then
		remember_spawn()
	end
	if a ~= nil and a.clears then
		win:get(a.clears).value = ""
	end
end

-- Having the window ------------------------------------------------------------------------------

local function ask_online()
	listing = { names = {}, ["until"] = os.clock() + LIST_SECONDS }
	send({ "@listplayers" })
end

-- The window as the flag says: the forms for an administrator, the notice for anyone else who
-- had them earlier in the session.
local function follow_flag()
	local admin = is_admin()
	if admin and not full then
		win = forms()
		full = true
		show_commands()
	elseif not admin and full then
		win = notice()
		full = false
		open = false
		win:close()
		armed = nil
	end
	if full then
		show_selection()
	end
end

commands.add("admin", "opens or closes the Admin window, for an administrator", function()
	follow_flag()
	if not full then
		print("Only a character with the administrator flag has the Admin window.")
		return
	end
	if open then
		win:close()
		open = false
	else
		win:show()
		open = true
		if listing == nil and #online == 0 then
			ask_online()
		end
	end
end)

task.every(1000, follow_flag)
on("context.begin", follow_flag)

on("ui.opened", function(e)
	if e.window ~= "main" then
		return
	end
	if not full then
		return
	end
	open = true
	if listing == nil and #online == 0 then
		ask_online()
	end
end)

on("ui.closed", function(e)
	if e.window == "main" then
		open = false
	end
end)

on("ui.clicked", function(e)
	if e.window ~= "main" or not full then
		return
	end
	if e.id == "use_selected" then
		local s = game.selection()
		if s ~= nil and s.class == "Player" then
			win:get("name").value = s.name
			show_online()
		end
		return
	end
	press(e.id)
end)

on("ui.changed", function(e)
	if e.window ~= "main" or not full then
		return
	end
	if e.id == "name" then
		show_online()
	elseif e.id == "search" or e.id == "group" then
		show_commands()
	end
end)

on("ui.selected", function(e)
	if e.window ~= "main" or not full then
		return
	end
	local row = e.rows and e.rows[1]
	if row == nil then
		return
	end
	if e.id == "online" then
		local cells = win:get("online").rows[row]
		if cells ~= nil then
			win:get("name").value = cells[1]
		end
	elseif e.id == "recent" then
		local r = recent[row]
		if r ~= nil then
			win:get("weenie").value = r
		end
	elseif e.id == "commands" then
		local cells = show_commands()[row]
		if cells ~= nil then
			chosen = L.find(COMMANDS, string.sub(cells[1], 2))
			win:get("arguments").value = ""
			if chosen ~= nil then
				win:get("usage").text = L.trim("@" .. chosen.verb .. " " .. chosen.usage) .. "   " .. chosen.about
			end
		end
	end
end)

-- The server's replies. Only the kinds of line a server answers in are taken, so what another
-- player says or emotes never fills the list of players or the location.
on("chat.line", function(e)
	if not full or not REPLY_KINDS[e.kind] then
		return
	end
	local now = os.clock()
	for line in string.gmatch(e.text or "", "[^\n]+") do
		line = L.without_stamp(line)
		if listing ~= nil then
			if now > listing["until"] then
				listing = nil
			elseif string.find(line, "Total connected Players:", 1, true) then
				online = listing.names
				listing = nil
				show_online()
			else
				local who = string.match(line, "^(.-) : %d+$")
				if who ~= nil and L.trim(who) ~= "" then
					table.insert(listing.names, L.trim(who))
				end
			end
		end
		if location_wanted then
			local at = string.match(line, "Your location is: (.*)$")
			if at ~= nil then
				win:get("location").value = L.trim(at)
				location_wanted = false
			end
		end
		if sent ~= nil and now - sent.last <= REPLY_SECONDS and #sent.replies < MAX_REPLIES then
			table.insert(sent.replies, line)
			sent.last = now
			show_replies()
		end
	end
end)
