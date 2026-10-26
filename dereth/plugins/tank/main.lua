-- Tank: an unattended macro in the manner of VTank. It walks a route (a VTank .nav file), fights
-- the monsters it meets along the way, loots their corpses by a loot profile (a VTank .utl file),
-- heals and rests as it needs to, gets itself unstuck, and stops or recalls when the pack is full,
-- it has died too often, or it has run long enough.
--
-- /tank start | pause | stop    run the macro (or resume it), pause it keeping the run, or stop it
-- /tank toggle                  run (or resume) the macro, or pause it when it runs: VTank's
--                               Run Macro switch, for a hotkey
-- /tank forcebuff | cancelforcebuff   cast every buff again now (VTank's Force Buff), or stop doing so
-- /tank status                  what it is doing, and the run's tally
-- /tank log [n]                 the last n lines of the run's log
-- /tank set <setting> <value>   change a setting (/tank settings lists them)
-- /tank route <file>            the route, from the routes folder ("none" for none)
-- /tank route new [kind]        a new route (circular, linear or once), made in the game:
-- /tank route add [coords]      a point here (or at the selected portal, vendor or NPC, which the
--                               route then uses), or at coordinates
-- /tank route portal | talk | vendor   a waypoint using the selected portal or NPC (used once), the
--                               selected NPC (used until it tells the character something), or
--                               the selected vendor (opened)
-- /tank route jump <heading> <true|false> <ms> [forward|strafeleft|straferight]
--                               a jump here: faces the heading, holds shift (walks) or not, and
--                               charges the jump that long before leaping
-- /tank route pause <ms> | chat <text> | recall <spell id> | checkpoint
-- /tank route reverse | points | save <file>   run it the other way round, list it, save it as a
--                               VTank .nav
-- /tank route new follow        a follow route of the selected character (route add sets whom
--                               it follows to the selection)
-- /tank follow [name]           follow the selected character (or the one named): VTank's FC
-- /tank jump <heading> <true|false> <ms> [direction]   one jump now
-- /tank hunt <place>            travel there (walking, portals, recalls), then run the macro; a
--                               place is "42.2N 33.8E", a cell "0x019E0140", "portal <name>" (where
--                               a known portal sends), or "lifestone" / "marketplace"
-- /tank go <place>              travel there, the macro stopped (a place as hunt takes it)
-- /tank give <item> to <person> | pickup <item> | useon <item> on <target>
--                               quest steps: hand an item to a person, pick one up off the ground,
--                               use one on something (a route waiting for each, see quest.lua)
-- /tank profile <file>          the loot profile, from the profiles folder ("none" for none)
-- /tank vendor [set [name]]     go to the vendor, sell and buy there, and come back; with set, the
--                               selected vendor (or the nearest of that name) becomes the one it
--                               goes to
-- /tank buffbot [set [name]]    go to the buffbot, ask it for buffs, and come back; with set, the
--                               selected player (or the nearest of that name) becomes the buffbot
-- /tank sell                    sell and buy at the vendor whose wares are open
-- /tank lootcontainer           take from the open container what the loot profile takes (VTank's
--                               "Loot this Container")
-- /tank buy [<count> <item>]    buy the item up to the count at vendors (0 takes it off); alone,
--                               the buy list
-- /tank monsters                the monster list; /tank monster <field> <value> <name> sets a row
--                               (priority, attack, ring, streak, damage, weapon, offhand, exvuln,
--                               petdmg, or a debuff on/off)
-- /tank import <file.usd>       read a VTank settings profile from the data folder
-- /tank export <file.usd>       save the settings and monster list as one
-- /tank files                   what is in the data folder
-- /tank meta <file.met>         run a VTank meta with the macro ("none" for none)
-- /tank vt <command>            a VTank command, as a meta types it (/vt opt set, /vt nav load, ...)
--
-- Routes and profiles are looked for in the plugin's data folder (where the player puts files
-- for it) before its own routes/ and profiles/ folders.
--
-- Each thing it does without a click of yours (combat, nav, loot, heal, buff, craft) needs a world
-- that allows it.

local S = require("settings")
local monsters = require("monsters")
local spells = require("spells")
local profile = require("profile")
local metas = require("metas")
local window = require("window")
local buffs = require("buffs")
local recharge = require("recharge")
local routes = require("routes")
local vendors = require("vendor")
local fellows = require("fellows")
local quest = require("quest")
local bb = require("buffbot")

-- The words after /tank route that change the route rather than load one.
local ROUTE_EDITS = { new = true, add = true, jump = true, pause = true, chat = true, recall = true,
	checkpoint = true, reverse = true, points = true, save = true, portal = true, talk = true, vendor = true }
local E = require("engine")

metas.E = E
metas.import = profile.import
metas.export = profile.export
metas.set_in_text = profile.set_in_text
E.command = metas.command
-- The meta is checked whenever the macro decides what to do; a resumed run starts its "seconds in
-- state" again.
E.before_tick = metas.decide
E.on_resume = metas.resumed
E.on_stop = function()
	metas.stop()
end
-- The meta the settings name, read with the other files.
E.on_load_meta = function(name)
	if name == nil or name == "" then
		metas.stop()
		metas.meta, metas.name = nil, nil
		return
	end
	if metas.name == name and metas.meta ~= nil then
		return
	end
	local text = E.read_file("metas", name)
	if text == nil then
		E.note("no meta %s", name)
		return
	end
	local m, why = metas.load(text, name)
	if m == nil then
		E.note("meta %s does not read: %s", name, tostring(why))
	else
		E.note("meta %s: %d states%s", name, (function()
			local n = 0
			for _ in pairs(m.states) do
				n = n + 1
			end
			return n
		end)(), #m.unsupported > 0 and ("; not carried: " .. table.concat(m.unsupported, "; ")) or "")
	end
end

window.build(E)

-- Services for the other plugins in this client, as VTank answered UtilityBelt: the meta's state
-- and the settings, read and set; and whether the macro is running, which a looter asks so that it
-- waits while the macro loots by its own profile.
services.offer("running", function()
	return E.running == true
end)
-- The run's tally in words, for a plugin that logs the run.
services.offer("tally", function()
	return { running = E.running, state = E.state, tally = E.tally_text() }
end)
services.offer("get_meta_state", function()
	return metas.meta and metas.meta.state or nil
end)
services.offer("set_meta_state", function(args)
	local state = type(args) == "table" and args.state or args
	assert(metas.meta ~= nil, "no meta is loaded")
	assert(metas.meta.states[tostring(state)] ~= nil, "the meta has no state " .. tostring(state))
	metas.meta:set_state(tostring(state))
	return metas.meta.state
end)
services.offer("get_setting", function(args)
	local name = type(args) == "table" and args.name or args
	return S.get(tostring(name))
end)
services.offer("set_setting", function(args)
	assert(type(args) == "table", "set_setting takes { name, value }")
	local done, why = S.set(tostring(args.name), args.value, false)
	assert(done, tostring(why))
	return S.get(tostring(args.name))
end)

local function refresh()
	window.refresh(E)
end

E.on_state = function()
	refresh()
end
E.on_note = function()
	refresh()
end

-- Start the macro, or resume it where it was paused (`opts` as `E.start` takes them).
local function start(opts)
	if E.running then
		E.resume("by you")
		refresh()
		return
	end
	S.load()
	E.load_files()
	-- The meta is checked before the macro first acts, as VTank checks it before the macro's own
	-- work.
	metas.start()
	E.start(opts)
	refresh()
end
E.on_start = start

-- VTank's Run Macro switch, as its hotkey works it: run (or resume), or pause the run under way.
local function toggle()
	if E.running and not E.paused then
		E.pause("by you")
	else
		E.hunt, E.hunt_name = nil, nil
		start()
	end
end

-- VTank's "Loot this Container", now: the open container taken from by the loot profile.
local function loot_container()
	if game.inventory.open_container == nil then
		return "no container is open"
	end
	task.spawn(function()
		local ok, why = E.loot_container()
		if not ok then
			E.note("%s", why)
		end
		refresh()
	end)
	return "looting the open container"
end

-- VTank's Force Buff: every buff cast again now, whatever time it has left; a pause holds it and
-- a resume carries it on. With the macro stopped, a run for the buffs alone, which ends with them.
function E.force_buff()
	E.note("%s", buffs.start_force())
	if not E.running then
		start({ buff_only = true })
	end
end

-- What changed the files or the run's state shows in the window.
E.on_files = function()
	window.build(E)
	refresh()
end

-- The character's settings come with it; the macro starts by itself if told to.
on("context.begin", function()
	monsters.forget()
	E.forget_kept_log()
	S.load()
	E.load_corpse()
	E.load_files()
	window.build(E)
	refresh()
	if S.get("StartOnLogin") then
		-- A moment in, so what other plugins type at login is done first.
		task.after(10000, function()
			if not E.running then
				E.note("starting by itself (StartOnLogin)")
				start()
			end
		end)
	end
end)

on("context.end", function()
	E.stop("left the world")
end)

on("world.object_created", function(e)
	if E.running then
		E.object_created(e.id)
	end
end)

-- An object gone: a kill counted while running, and a ghost forgotten whenever.
on("world.object_removed", function(e)
	E.object_removed(e.id)
end)

-- The Tank in the player's other clients: a fellow of its own on this machine (PeerFellow).
on("peers.message", function(e)
	fellows.heard(e)
end)

-- Tells to the character: VTank's fellowship commands, with AutoFellowManagement; the buffbot's
-- answers while the macro waits on it; and what the game says of a corpse a death left.
on("chat.line", function(e)
	fellows.chat_line(e)
	bb.chat_line(e, fellows.parse_tell)
	E.corpse_line(tostring(e.text or ""))
end)

on("character.death", function()
	if E.running then
		E.died()
	end
end)

-- A hunting place as typed: map coordinates, a cell, a known portal's destination, a recall's.
local function hunting_place(words)
	local rest = table.concat(words, " ", 2)
	local ns, nsh, ew, ewh = string.match(rest, "^([%d%.]+)%s*([NnSs])[,%s]*([%d%.]+)%s*([EeWw])$")
	if ns ~= nil then
		local n = tonumber(ns) * ((nsh == "S" or nsh == "s") and -1 or 1)
		local e = tonumber(ew) * ((ewh == "W" or ewh == "w") and -1 or 1)
		return { ns = n, ew = e }
	end
	local cell = string.match(rest, "^0[xX](%x+)$")
	if cell ~= nil then
		return { cell = tonumber(cell, 16) }
	end
	local portal = string.match(rest, "^portal%s+(.+)$")
	if portal ~= nil then
		return { portal = portal }
	end
	if rest == "lifestone" or rest == "marketplace" then
		return { recall = rest }
	end
	return nil
end

-- The Tank goes hunting: travel to `place`, then run the macro there.
local function hunt(place, name)
	E.hunt, E.hunt_name = place, name
	-- A run under way travels there at its next step (a meta's line, say); else the run starts
	-- by traveling there.
	E.hunt_due = E.running
	start()
	chat.print("[Tank] Hunting at " .. name .. ".")
end

-- Travel to `place` by a journey, the macro stopped: a run's last step (a meta's leaving of a
-- dungeon), or the player's. The journey is the client's, through portals and by recalls, or by
-- handing a token back where a person sends the character on (asked first).
local function go(place, name)
	if E.running then
		E.stop("going to " .. name)
	end
	E.note("going to %s", name)
	task.spawn(function()
		local ok, r = E.travel(place, "to " .. name)
		E.note(ok and "arrived at %s" or "did not get to %s: %s", name, tostring(r))
		E.save_log()
		refresh()
	end)
end

-- A quest step (see quest.lua), finished before what typed it goes on when it can wait (a route's
-- chat waypoint), else in a task of its own.
local function quest_step(step)
	local function run()
		local ok, line = quest.run(E, step)
		E.note("%s", line)
		chat.print("[Tank] " .. line)
		return ok
	end
	if coroutine.isyieldable() then
		return run()
	end
	task.spawn(run)
	return true
end

-- The vendor trip, now: within the run, or a run for it alone. Answers the line to tell.
local function vendor_go(why)
	local line = E.vendor_trip(why)
	if E.trip ~= nil and not E.running then
		start({ trip_only = true })
	end
	return line
end

-- The buffbot trip, now: within the run, or a run for it alone. Answers the line to tell.
local function buffbot_go(why)
	local line = bb.start(why)
	if E.btrip ~= nil and not E.running then
		start({ btrip_only = true })
	end
	return line
end

-- Trade with the open vendor by profile, now.
local function trade_now()
	if vendor.info() == nil then
		return "no vendor's wares are open"
	end
	task.spawn(function()
		vendors.trade(E, E.note)
		refresh()
	end)
	return "selling and buying"
end

-- One jump now, as the route's jump waypoint would make it.
local function jump_now(j)
	task.spawn(function()
		local ok, why = pawait(nav.jump({ heading = j.heading, walk = j.shift, ms = j.ms, direction = j.direction }))
		E.note(ok and "jumped" or "jump: %s", tostring(why))
	end)
end

-- Follow a character, as VTank's MiniRemote FC follows the selected one: the macro's route
-- becomes a follow route of it (or of the character named, near or not). Answers the line to
-- tell.
local function follow(name)
	local who = nil
	if name ~= nil and name ~= "" then
		who = E.character_named(name) or { name = name, id = 0 }
	else
		local why
		who, why = routes.selected_character()
		if who == nil then
			return why
		end
	end
	E.set_route(routes.new("follow", who), "made in the game")
	E.route_edited, E.route_name = true, nil
	return "following " .. tostring(who.name) .. (E.running and "" or " once the macro runs")
end
E.follow = follow

-- Read a VTank settings profile from the data folder; answers the line to tell.
local function import_settings(name)
	local text = files.data(name) or files.data("profiles/" .. name)
	if text == nil then
		return "No " .. name .. " in the data folder."
	end
	local ok, line = pcall(profile.import, text)
	E.load_files()
	return ok and line or ("could not read " .. name .. ": " .. tostring(line))
end

-- Save the settings, the monster list, the items, consumables and buffs as a VTank settings
-- profile, in a task (the spell book read first where it has not been); `tell` hears the line
-- to tell.
local function export_settings(name, tell)
	if not string.find(string.lower(name), "%.usd$") then
		name = name .. ".usd"
	end
	task.spawn(function()
		local ok, why = pcall(function()
			return files.save_data(name, profile.export())
		end)
		tell(ok and ("Saved " .. name .. ".") or ("Could not save " .. name .. ": " .. tostring(why)))
		window.build(E)
		refresh()
	end)
end

on("ui.clicked", function(e)
	if e.window == "remote" then
		-- VTank's remote control: F, CF and FC.
		if e.id == "F" then
			E.force_buff()
		elseif e.id == "CF" then
			E.note("%s", buffs.cancel_force())
		elseif e.id == "FC" then
			E.note("route: %s", follow(nil))
			window.build(E)
		end
		refresh()
		return
	end
	if e.window ~= "main" then
		return
	end
	local win = window.win
	if e.id == "loot_container" then
		E.note("%s", loot_container())
	elseif e.id == "start" then
		E.hunt, E.hunt_name = nil, nil
		start()
	elseif e.id == "stop" then
		E.stop("stopped by you")
	elseif e.id == "pause" then
		E.pause("by you")
	elseif e.id == "forcebuff" then
		E.force_buff()
	elseif e.id == "cancelforcebuff" then
		E.note("%s", buffs.cancel_force())
	elseif e.id == "vendor_set" then
		E.note("%s", vendors.set_vendor(game.selection()))
		window.build(E)
	elseif e.id == "vendor_go" then
		E.note("%s", vendor_go("by you"))
	elseif e.id == "vendor_trade" then
		E.note("%s", trade_now())
	elseif e.id == "buffbot_set" then
		E.note("%s", bb.set_buffbot(game.selection()))
		window.build(E)
	elseif e.id == "buffbot_go" then
		E.note("%s", buffbot_go("by you"))
	elseif e.id == "hunt" or e.id == "go" then
		local text = win:get("hunt_at").value or ""
		local place = hunting_place({ "hunt", text })
		if place == nil then
			E.note("%s where? 42.2N 33.8E, 0x019E0140, portal <name>, lifestone or marketplace", e.id)
		elseif e.id == "go" then
			go(place, text)
		else
			hunt(place, text)
		end
	elseif string.sub(e.id or "", 1, 4) == "use:" then
		local line = window.use_selected(string.sub(e.id, 5))
		if line ~= nil then
			E.note("%s", line)
			window.build(E)
		end
	elseif e.id == "r_load_go" then
		local name = win:get("r_load").value or ""
		S.set("Route", name)
		E.route_edited = false
		E.route_name = nil
		E.load_files()
		window.build(E)
	elseif e.id == "import" then
		local name = win:get("import_file").value or ""
		if name ~= "" then
			E.note("%s", import_settings(name))
			window.build(E)
		end
	elseif e.id == "export" then
		local name = win:get("export_name").value or ""
		if name ~= "" then
			export_settings(name, function(line)
				E.note("%s", line)
			end)
		end
	else
		window.rebuild, window.files_changed, window.jump_now, window.vt = false, false, nil, nil
		window.quest = nil
		local was_meta = S.get("Meta")
		local line = window.clicked(E, e)
		if line ~= nil then
			E.note("%s", line)
		end
		-- A quest step's button does it now, as the player's /tank line does.
		if window.quest ~= nil then
			metas.command(window.quest)
		end
		-- A control that stands for one of VTank's commands runs it, as `/tank vt` does.
		if window.vt ~= nil then
			metas.command(window.vt)
		end
		if window.files_changed then
			E.load_files()
		end
		-- A new meta made: the meta the settings name now.
		if S.get("Meta") ~= was_meta then
			E.load_files()
		end
		if window.jump_now ~= nil then
			jump_now(window.jump_now)
		end
		if window.rebuild then
			window.build(E)
		else
			window.refresh_route(E)
		end
	end
	refresh()
end)

-- A setting changed in the window: kept, and the files it names read again.
on("ui.changed", function(e)
	if e.window == "remote" then
		-- VTank's remote control: C (follow around corners) and the target lock.
		local name = ({ C = "FollowAroundCorners", lock = "TargetLock" })[e.id]
		if name ~= nil then
			S.set(name, e.value == true)
			E.note("%s %s", name, e.value == true and "on" or "off")
			window.build(E)
		end
		return
	end
	if e.window ~= "main" then
		return
	end
	-- The meta editor's fields: the rule they hold follows (the window built again for a kind).
	window.rebuild, window.note = false, nil
	if window.meta_changed(e) then
		if window.note ~= nil then
			E.note("meta: %s", window.note)
		end
		if window.rebuild then
			window.build(E)
		end
		return
	end
	if e.id == "r_record" then
		window.recording = e.value == true
		window.recorded_at = nil
		E.note(window.recording and "route: recording while walking" or "route: recording stopped")
		window.refresh_route(E)
		return
	end
	-- A log type switched on or off, as VTank's /vt log <type> on|off.
	local kind = string.match(e.id or "", "^log:(.+)$")
	if kind ~= nil then
		metas.command("/vt log " .. kind .. (e.value == true and " on" or " off"))
		return
	end
	local name = window.changed(e)
	-- The remote control shown or hidden, and its switches kept with the settings they show.
	if name == "MiniRemote" or name == "FollowAroundCorners" or name == "TargetLock" then
		window.build_remote()
	end
	if name == "Route" or name == "LootProfile" or name == "Meta" then
		if name == "Route" then
			E.route_edited = false
			E.route_name = nil
		end
		E.load_files()
		-- A meta read: its states to set in the window.
		if name == "Meta" then
			window.build(E)
		end
		refresh()
	end
end)

-- The spell book read for the Buffs page's list, a page a frame, once.
local reading_book = false
local function read_book()
	if spells.ready() or reading_book then
		return
	end
	reading_book = true
	task.spawn(function()
		buffs.want_settings()
		pcall(spells.refresh, true)
		reading_book = false
		window.build(E)
	end)
end

-- The remote control opens with the window, while MiniRemote is on: a window opens only when the
-- player asks.
on("ui.opened", function(e)
	if e.window == "main" then
		window.build_remote()
	end
end)

on("ui.selected", function(e)
	if e.window ~= "main" then
		return
	end
	if e.id == "pages" then
		if e.rows and e.rows[1] == window.BUFFS_PAGE then
			read_book()
		end
		refresh()
	else
		window.rebuild = false
		window.selected(E, e)
		if window.rebuild then
			window.build(E)
		end
		refresh()
	end
end)

-- Recording a route while walking: a point each time the character has gone RouteRecordMetres
-- from the last one the recorder added.
task.every(250, function()
	if not window.recording then
		return
	end
	local x, y = E.global_of(game.player.position)
	if x == nil then
		return
	end
	local last = window.recorded_at
	if last ~= nil and math.sqrt((x - last[1]) ^ 2 + (y - last[2]) ^ 2) < S.get("RouteRecordMetres") then
		return
	end
	window.recorded_at = { x, y }
	E.note("%s", "route: " .. routes.add_point_here(E))
	window.refresh_route(E)
end)

-- With ManaChargesWhenOff, worn items are kept filled from stones and charges while the macro is
-- stopped too (VTank's option), once a minute.
task.every(60000, function()
	if not E.running and S.get("ManaChargesWhenOff") and E.can("craft") then
		task.spawn(function()
			local ok, line = recharge.refill_worn()
			if line ~= nil then
				E.note("%s", line)
			end
		end)
	end
end)

-- A heartbeat in the log: vitals and the tally once a minute while running.
task.every(60000, function()
	if E.running then
		E.note("%s: %s", E.state, E.tally_text())
	end
	refresh()
	E.save_log()
end)

local function tank_command(args, words)
	local w = words[1]
	if w == "start" then
		E.hunt, E.hunt_name = nil, nil
		start()
		chat.print("[Tank] Started.")
	elseif (w == "hunt" or w == "go") and words[2] ~= nil then
		local place = hunting_place(words)
		if place == nil then
			chat.print("[Tank] /tank " .. w .. " 42.2N 33.8E | 0x019E0140 | portal <name> | lifestone | marketplace")
		elseif w == "go" then
			-- Its notes show in the window; the window is not built again for it.
			go(place, table.concat(words, " ", 2))
			return
		else
			hunt(place, table.concat(words, " ", 2))
		end
	elseif quest.parse(words) ~= nil then
		-- As for a journey: the step's note is all it changes in the window, and the window is
		-- not built again in the moment the step ends, which would make that moment long.
		quest_step(quest.parse(words))
		return
	elseif w == "stop" then
		E.stop("stopped by you")
		chat.print("[Tank] Stopped.")
	elseif w == "pause" then
		E.pause("by you")
		chat.print("[Tank] Paused.")
	elseif w == "toggle" then
		toggle()
	elseif w == "lootcontainer" then
		chat.print("[Tank] " .. loot_container())
	elseif w == "forcebuff" then
		E.force_buff()
	elseif w == "cancelforcebuff" then
		chat.print("[Tank] " .. buffs.cancel_force())
	elseif w == "status" or w == nil then
		chat.print(string.format("[Tank] %s%s%s. %s", E.state, E.target and (" on " .. E.target.name) or "",
			metas.meta and (", meta " .. tostring(metas.name) .. " in " .. tostring(metas.meta.state)) or "", E.tally_text()))
	elseif w == "log" then
		local n = tonumber(words[2]) or 10
		for i = math.max(1, #E.log - n + 1), #E.log do
			chat.print("[Tank] " .. E.log[i])
		end
	elseif w == "settings" then
		-- Forty lines a frame, within what a plugin may print in one.
		task.spawn(function()
			for i, name in ipairs(S.names()) do
				chat.print(string.format("[Tank] %s = %s", name, tostring(S.get(name))))
				if i % 40 == 0 then
					task.sleep(0)
				end
			end
		end)
	elseif w == "set" and words[2] ~= nil and words[3] ~= nil then
		local value = table.concat(words, " ", 3)
		local ok, why = S.set(words[2], value)
		chat.print(ok and string.format("[Tank] %s = %s", words[2], tostring(S.get(words[2]))) or ("[Tank] " .. why))
		if S.name(words[2]) == "Route" or S.name(words[2]) == "LootProfile" then
			E.load_files()
		end
	elseif w == "meta" and words[2] ~= nil then
		local name = words[2] == "none" and "" or table.concat(words, " ", 2)
		S.set("Meta", name)
		E.load_files()
		if E.running then
			metas.start()
		end
		chat.print("[Tank] Meta: " .. (name ~= "" and name or "none"))
	elseif w == "vt" then
		-- VTank's commands, typed by hand as a meta types them.
		metas.command("/vt " .. args:gsub("^%s*vt%s*", ""))
	elseif w == "import" and words[2] ~= nil then
		-- A VTank settings profile from the data folder.
		chat.print("[Tank] " .. import_settings(table.concat(words, " ", 2)))
	elseif w == "export" and words[2] ~= nil then
		-- The settings and monster list, as a VTank settings profile in the data folder.
		export_settings(table.concat(words, " ", 2), function(line)
			chat.print("[Tank] " .. line)
		end)
	elseif w == "files" then
		for _, f in ipairs(files.data_list()) do
			chat.print("[Tank] " .. f)
		end
	elseif w == "monsters" then
		for _, r in ipairs(monsters.rows()) do
			local d = {}
			for k, on in pairs(r.debuffs or {}) do
				if on then
					table.insert(d, k)
				end
			end
			table.sort(d)
			chat.print(string.format("[Tank] %s: priority %d, %s%s%s, damage %s%s%s%s%s", r.name, r.priority or 1,
				r.attack == false and "debuff only" or "attack", r.ring and ", rings" or "", r.streak and ", streaks" or "",
				r.damage or "auto",
				(r.weapon or "") ~= "" and (", weapon " .. r.weapon) or "",
				(r.offhand or "") ~= "" and (", offhand " .. r.offhand) or "",
				#d > 0 and (", debuffs " .. table.concat(d, " ")) or "",
				(r.exvuln or "none") ~= "none" and (", extra vuln " .. r.exvuln) or "") ..
				((r.petdmg or "pauto") ~= "pauto" and (", pet " .. r.petdmg) or ""))
		end
	elseif w == "monster" and words[2] ~= nil and words[3] ~= nil then
		-- /tank monster <field> <value> <name ...>, or /tank monster remove <name ...>
		local field = string.lower(words[2])
		if field == "remove" then
			local name = table.concat(words, " ", 3)
			chat.print(monsters.remove(name) and ("[Tank] Removed " .. name .. ".") or ("[Tank] No row " .. name .. "."))
		else
			local value = words[3]
			local name = table.concat(words, " ", 4)
			if name == "" then
				name = monsters.DEFAULT
			end
			local row = monsters.find(name) or {}
			local fields = {}
			if field == "priority" and tonumber(value) then
				fields.priority = tonumber(value)
			elseif field == "attack" or field == "ring" or field == "streak" then
				-- VTank's A, R and S columns.
				fields[field] = value == "on" or value == "true"
			elseif field == "damage" then
				fields.damage = string.lower(value)
			elseif field == "weapon" or field == "offhand" then
				-- A weapon's name may have spaces: /tank monster weapon "Long Sword" <name> is not
				-- possible, so the weapon is named with underscores for spaces.
				fields[field] = (string.gsub(value, "_", " "))
			elseif field == "exvuln" then
				-- VTank's Ex. Vuln: an element's vulnerability cast as well, `auto` for the damage
				-- type's, `none` for none.
				fields.exvuln = string.lower(value)
			elseif field == "petdmg" then
				-- VTank's PetDmg: the element of the pet summoned for it (pauto, auto, none, or an
				-- element).
				fields.petdmg = string.lower(value)
			elseif spells.DEBUFFS[field] ~= nil or field == "vuln" then
				local d = row.debuffs or {}
				d[field] = value == "on" or value == "true"
				fields.debuffs = d
			else
				chat.print("[Tank] /tank monster priority|attack|ring|streak|damage|weapon|offhand|exvuln|petdmg|<debuff> <value> <name>")
				return
			end
			monsters.set(name, fields)
			chat.print("[Tank] " .. name .. ": " .. field .. " " .. value)
		end
	elseif w == "follow" then
		-- VTank's MiniRemote FC: a follow route of the selected character (or the one named).
		chat.print("[Tank] " .. follow(table.concat(words, " ", 2)))
	elseif w == "route" and ROUTE_EDITS[string.lower(words[2] or "")] then
		chat.print("[Tank] Route: " .. tostring(routes.edit(E, string.lower(words[2]), words)))
	elseif w == "jump" then
		local j = routes.jump_of(words, 2)
		if j == nil then
			chat.print("[Tank] /tank jump <heading> <true|false> <ms> [forward|strafeleft|straferight]")
		else
			task.spawn(function()
				local ok, why = pawait(nav.jump({ heading = j.heading, walk = j.shift, ms = j.ms, direction = j.direction }))
				chat.print(ok and "[Tank] Jumped." or ("[Tank] Jump: " .. tostring(why)))
			end)
		end
	elseif w == "vendor" then
		if string.lower(words[2] or "") == "set" then
			-- The selected vendor, or the nearest one in sight of the name given.
			local name = table.concat(words, " ", 3)
			local o = name ~= "" and game.world:first({ name = name, sort = "distance" }) or game.selection()
			chat.print("[Tank] " .. vendors.set_vendor(o))
		else
			chat.print("[Tank] Vendor trip: " .. vendor_go("by a command"))
		end
	elseif w == "buffbot" then
		if string.lower(words[2] or "") == "set" then
			-- The selected player, or the nearest one in sight of the name given.
			local name = string.lower(table.concat(words, " ", 3))
			local o = name ~= "" and game.world:first({ class = "Player", sort = "distance" }, function(p)
				return string.lower((string.gsub(p.name or "", "^%+", ""))) == name
			end) or game.selection()
			chat.print("[Tank] " .. bb.set_buffbot(o))
		else
			chat.print("[Tank] Buffbot trip: " .. buffbot_go("by a command"))
		end
	elseif w == "sell" then
		chat.print("[Tank] " .. trade_now())
	elseif w == "buy" then
		local n = tonumber(words[2] or "")
		if n ~= nil and words[3] ~= nil then
			chat.print("[Tank] " .. vendors.set_buy(table.concat(words, " ", 3), math.floor(n)))
		else
			for _, r in ipairs(vendors.buy_list()) do
				chat.print(string.format("[Tank] buy %s up to %d (%d held)", r.name, r.count, vendors.held(r.name)))
			end
		end
	elseif (w == "route" or w == "profile") and words[2] ~= nil then
		local name = words[2] == "none" and "" or words[2]
		S.set(w == "route" and "Route" or "LootProfile", name)
		if w == "route" then
			E.route_edited = false
		end
		E.load_files()
		chat.print("[Tank] " .. (w == "route" and "Route: " or "Loot profile: ") .. (name ~= "" and name or "none"))
	else
		chat.print("[Tank] /tank start | pause | stop | toggle | forcebuff | status | log [n] | settings | set <setting> <value> | route <file> | profile <file>")
	end
	-- What a command changed shows in the window.
	if w ~= "status" and w ~= "log" and w ~= "settings" and w ~= "monsters" and w ~= "files" and w ~= nil then
		window.build(E)
	end
	refresh()
end

commands.add("tank", "/tank start | pause | stop | toggle | forcebuff | status | log [n] | settings | set <setting> <value> | route <file> | profile <file> | hunt <place> | go <place> | give <item> to <person> | pickup <item> | useon <item> on <target> | vendor | buffbot | sell | buy", tank_command)

-- A `/tank` line a meta's chat action or a route's chat waypoint types is answered as the player's
-- would be (a line a plugin types never reaches its own command).
metas.tank_line = function(line)
	local rest = string.gsub(line, "^%s*[/@][Tt][Aa][Nn][Kk]%s*", "")
	local words = {}
	for word in string.gmatch(rest, "%S+") do
		table.insert(words, word)
	end
	tank_command(rest, words)
end
