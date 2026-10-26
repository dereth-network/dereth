-- Navigator: points the way, in the manner of GoArrow. One "Where to?" box finds places as the
-- player types (towns, dungeons, portals, lifestones, recalls and places of their own, by name and
-- type, or coordinates typed in); Go travels to the one picked (through portals and by recalls
-- where the client knows a journey, else on foot), Point only puts the arrow on it, and Stop
-- stops. An arrow HUD shows the character's own coordinates and the target's distance and way.
-- Tabs below hold the player's saved places, the coordinates seen in chat, the spots clicked on the
-- game's map (a left click there points at it), the ways to the target over portals and recalls,
-- and a map of the dungeon the character is in.
--
-- The commands are shortcuts to the same:
--
-- /nav                     open the window
-- /nav <place>             point at a place by name (any type) or at coordinates (12.3N, 45.6E)
-- /nav find <text> [type]  find places whose name holds the text, nearest first (in the window too)
-- /nav nearest <type>      point at the nearest place of a type (town, dungeon, portal, lifestone...)
-- /nav here                say where the character is
-- /nav save <name>         keep where the character stands as a place of the player's own
-- /nav forget <name>       forget one
-- /nav sel                 point at the selected object, following it as it moves
-- /nav obj <name>          point at the nearest object whose name holds the text
-- /nav last                point at the last coordinates anyone wrote in chat
-- /nav routes              the ways to the target: best, without recalls, without portals, on foot
-- /nav go | stop           go to the target (a gesture), or stop
-- /nav travel <where>      go somewhere by a journey: a place, coordinates, a cell (0x019E0140),
--                          or "portal <name>" for where a known portal sends (a dungeon, say)
-- /nav plan <where>        the journey there, without going
-- /nav route <file>        walk a VTank route from the plugin's routes/ folder
-- /nav clear               take the markers down

local navlib = require("dereth.nav")
local P = require("places")
local L = require("locations")
local C = require("chatcoords")
local R = require("route")
local M = require("map")

-- What the Navigator points at: `{ name, place }` for a place, `{ name, object }` for an object
-- it follows; `travel` is what a journey goes to (a portal's destination for a dungeon, say).
local target = nil
-- The place picked in the window (a search result, a saved place, a line of chat, a click on the
-- map), which Go and Point act on; the target when none is picked.
local picked = nil
local found = {} -- the search's results, as locations
local saved = {} -- the Saved page's rows, as locations
local clicks = {} -- the spots clicked on the game's map, newest first
local plans = {} -- the Ways page's ways
local journey = nil -- the legs of the journey under way, or nil
local status = ""

local MAX_RESULTS = 200
local MAX_CLICKS = 10

local function say(text)
	chat.print("[Nav] " .. text)
end

local hud = nil
if host.has("ui.hud") then
	hud = ui.hud({ id = "hud", anchor = "top",
		ui.label({ id = "here", text = "" }),
		ui.label({ id = "where", text = "" }),
	})
end

local pages = {
	ui.group({ title = "Saved",
		ui.list({ id = "saved", columns = { "Name", { title = "Where", width = 20 }, { title = "Far", width = 8 } },
			rows = {}, height = 5 }),
		ui.row({
			ui.input({ id = "save_name", text = "Name", value = "", grow = 1 }),
			ui.button({ id = "save", text = "Save here" }),
			ui.button({ id = "forget", text = "Forget" }),
		}),
	}),
	ui.group({ title = "From chat",
		ui.list({ id = "chat", columns = { "Who", { title = "Where", width = 14 }, { title = "Far", width = 8 } },
			rows = {}, height = 5 }),
	}),
	ui.group({ title = "Map clicks",
		ui.list({ id = "clicks", columns = { "Where", { title = "Far", width = 8 } }, rows = {}, height = 5 }),
	}),
	ui.group({ title = "Ways",
		ui.list({ id = "ways", columns = { "Way", { title = "Legs", width = 5 }, { title = "About", width = 10 } },
			rows = {}, height = 4 }),
		ui.list({ id = "legs", columns = { { title = "#", width = 3 }, { title = "Leg", grow = 1 }, { title = "To", grow = 1 },
			{ title = "Far", width = 8 } }, rows = {}, height = 6 }),
		ui.row({
			ui.button({ id = "routes", text = "Find the ways" }),
			ui.button({ id = "route_go", text = "Go this way" }),
		}),
	}),
}
if M.available() then
	table.insert(pages, ui.group({ title = "Dungeon",
		ui.canvas({ id = "map", width = M.SIZE, height = M.SIZE, background = "#101010", draw = {} }),
		ui.label({ id = "map_note", text = "" }),
	}))
end
local MAP_PAGE = M.available() and #pages or nil
local WAYS_PAGE = 4

local win = ui.window({
	id = "main",
	title = "Navigator",
	width = 56,
	ui.input({ id = "search", text = "Where to?", value = "", placeholder = "a name, a type (town, dungeon...) or 12.3N 45.6E" }),
	ui.list({ id = "results", columns = { "Place", { title = "Type", width = 10 }, { title = "Where", width = 14 },
		{ title = "Far", width = 8 } }, rows = {}, height = 6 }),
	ui.label({ id = "to", text = "Pick a place, then Go or Point." }),
	ui.row({
		ui.button({ id = "go", text = "Go" }),
		ui.button({ id = "point", text = "Point" }),
		ui.button({ id = "stop", text = "Stop" }),
		ui.button({ id = "clear", text = "Clear" }),
	}),
	ui.label({ id = "status", text = "" }),
	ui.tabs({ id = "pages", selected = 1, table.unpack(pages) }),
})
win:close()
local shown = false -- whether the window is open

local function page()
	return win:get("pages").selected or 1
end

local function set_status(text)
	status = text
	win:get("status").text = text
end

-- The words of a search, and a type named last ("holt town"), or a type alone ("dungeon").
local function text_and_type(rest)
	local text, kind = string.match(rest or "", "^(.-)%s+(%a+)$")
	for _, t in ipairs(L.TYPES) do
		if kind ~= nil and string.lower(kind) == t then
			return text, t
		end
	end
	for _, t in ipairs(L.TYPES) do
		if string.lower(rest or "") == t then
			return "", t
		end
	end
	return rest or "", nil
end

-- The search's results: coordinates typed in first, then the places whose name holds the words,
-- of the type named, nearest first.
local function refresh_results()
	local typed = win:get("search").value or ""
	found = {}
	local p = P.parse(typed)
	if p ~= nil then
		table.insert(found, { name = P.show(p), type = "coordinates", place = p, far = P.reach(p) })
	end
	local text, kind = text_and_type(typed)
	if p == nil then
		for _, l in ipairs(L.find(text, kind)) do
			if #found >= MAX_RESULTS then
				break
			end
			table.insert(found, l)
		end
	end
	local rows = {}
	for _, l in ipairs(found) do
		table.insert(rows, { l.name, l.type, P.show(l.place), P.show_distance(l.far) })
	end
	win:get("results").rows = rows
	win:get("results").selected = nil
end

local function refresh_saved()
	saved = L.find("", "saved")
	local rows = {}
	for _, l in ipairs(saved) do
		table.insert(rows, { l.name, P.show(l.place), P.show_distance(l.far) })
	end
	win:get("saved").rows = rows
end

local function refresh_chat()
	win:get("chat").rows = C.rows()
end

local function refresh_clicks()
	local rows = {}
	for _, c in ipairs(clicks) do
		table.insert(rows, { P.show(c), P.show_distance(P.reach(c)) })
	end
	win:get("clicks").rows = rows
end

-- What Go and Point act on, in words: the picked place, else the target.
local function show_picked()
	local t = picked or target
	if t == nil then
		win:get("to").text = "Pick a place, then Go or Point."
		return
	end
	local where = t.object and "following it" or P.show(t.place)
	local d, dir = P.reach(t.place)
	win:get("to").text = string.format("To: %s (%s)%s", t.name, where,
		d and (", " .. P.show_distance(d) .. " " .. dir) or "")
end

local function clear()
	target = nil
	picked = nil
	plans = {}
	markers.clear()
	win:get("ways").rows = {}
	win:get("legs").rows = {}
	show_picked()
end

-- Point at a place (`place`) or an object (`object`, an id): an arrow, a marker in the world and
-- one on the map, where the host draws them.
local function point(name, place, travel, object)
	markers.clear()
	plans = {}
	win:get("ways").rows = {}
	target = { name = name, place = place, travel = travel or place, object = object }
	picked = nil
	if host.has("overlay.arrow") then
		if object then
			markers.arrow:set({ object = object, label = name })
		else
			markers.arrow:set({ position = place, label = name })
		end
	end
	if host.has("overlay.world") then
		if object then
			markers.world:add({ object = object, label = name })
		else
			markers.world:add({ position = place, label = name })
		end
	end
	if host.has("overlay.map") and place and place.ns ~= nil then
		markers.map:add({ points = { place }, label = name })
	end
	local where = object and "following it" or P.show(place)
	show_picked()
	say("Pointing at " .. name .. " (" .. where .. ").")
end

local function point_at(l)
	point(l.name, l.place, l.target)
end

-- Pick a place in the window: what Go and Point act on next.
local function pick(l)
	if l == nil then
		return
	end
	picked = { name = l.name, place = l.place, travel = l.target or l.place }
	show_picked()
end

-- Where the target is now: its place, or its object's.
local function target_place()
	if target == nil then
		return nil
	end
	if target.object then
		local o = game.world:get(target.object)
		return o and P.plain(o.position)
	end
	return target.place
end

-- The journey's progress, on the window's status line and the Ways page.
local tell = {
	line = function(text)
		say(text)
		set_status(text)
	end,
	planned = function(legs)
		journey = legs
		win:get("legs").rows = R.leg_rows(legs)
	end,
	leg = function(index, what)
		set_status(string.format("Leg %d of %d: %s", index, journey and #journey or index, what))
	end,
	ended = function()
		journey = nil
	end,
}

-- Go to the target: by a journey (with `opts`, a way of the route finder), otherwise walking.
local function go(opts)
	if target == nil then
		say("No target.")
		set_status("Pick a place first.")
		return
	end
	local t = target
	local to = t.object or t.travel
	if host.has("nav.journey") then
		set_status("Going to " .. t.name .. "...")
		R.go(to, t.name, opts, tell)
		return
	end
	if not host.has("nav.moveto") then
		say("This client cannot walk for you.")
		set_status("This client cannot walk for you.")
		return
	end
	set_status("Walking to " .. t.name .. "...")
	task.spawn(function()
		local ok, why = pawait(nav.move_to(to, 3))
		tell.line(ok and ("Arrived at " .. t.name .. ".") or ("Stopped: " .. tostring(why) .. "."))
	end)
end

-- Point at the picked place (if one is picked), and go there when `going`.
local function point_picked(going)
	if picked ~= nil then
		local p = picked
		point(p.name, p.place, p.travel)
	end
	if going then
		go()
	elseif target == nil then
		set_status("Pick a place first.")
	end
end

-- The route finder: each way to the target, in the chat and on the Ways page.
local function find_routes()
	if target == nil then
		say("No target.")
		return
	end
	if not host.has("nav.journey") then
		say("This client does not make journeys.")
		return
	end
	local t = target
	R.plan_all(t.object or t.travel, function(found_plans)
		plans = found_plans
		local rows = {}
		for _, p in ipairs(plans) do
			table.insert(rows, { p.way.name, R.legs_text(p.legs), string.format("%d s", math.floor(p.seconds + 0.5)) })
			say(string.format("%s to %s: %s (about %d s).", p.way.name, t.name, R.legs_text(p.legs),
				math.floor(p.seconds + 0.5)))
		end
		if #plans == 0 then
			say("No way to " .. t.name .. " is known.")
		end
		win:get("ways").rows = rows
		win:get("ways").selected = #plans > 0 and 1 or nil
		win:get("legs").rows = #plans > 0 and R.leg_rows(plans[1].legs) or {}
	end)
end

-- What a journey goes to for the words `rest`: coordinates, a cell, a known portal's destination,
-- or a place by name.
local function journey_target(rest)
	local place = P.parse(rest)
	if place then
		return place, P.show(place)
	end
	local cell = string.match(rest, "^0[xX](%x+)$")
	if cell then
		return { cell = tonumber(cell, 16) }, rest
	end
	local portal = string.match(rest, "^portal%s+(.+)$")
	if portal then
		return { portal = portal }, rest
	end
	local l = L.named(rest)
	if l then
		return l.target or l.place, l.name
	end
	return nil
end

-- The arrow HUD: the character's own place, and the target's distance and direction.
task.every(250, function()
	if hud == nil then
		return
	end
	local here = P.here()
	hud:get("here").text = here and ("Here: " .. P.show(here)) or ""
	if target == nil then
		hud:get("where").text = ""
		return
	end
	local at = target_place()
	local d, dir = P.reach(at)
	if at == nil then
		hud:get("where").text = target.name .. ": out of sight"
	elseif d ~= nil then
		hud:get("where").text = string.format("%s: %s %s", target.name, P.show_distance(d), dir)
	else
		hud:get("where").text = target.name .. " (" .. P.show(at) .. ")"
	end
end)

-- The dungeon map, while its page is up: a few cells read a step, drawn again as the character
-- moves or turns.
local drawn_at = nil
if MAP_PAGE then
	task.every(100, function()
		if not shown or page() ~= MAP_PAGE then
			return
		end
		local done = M.step()
		local me = game.player.position
		if me == nil then
			return
		end
		local key = string.format("%s %.1f %.1f %.1f %d %s", tostring(me.cell), me.x or 0, me.y or 0, me.z or 0,
			math.floor((me.heading or 0) / 5), tostring(done))
		if key == drawn_at then
			return
		end
		drawn_at = key
		local cmds = M.draw()
		win:get("map").draw = cmds or {}
		local read, all = M.progress()
		win:get("map_note").text = cmds == nil and "Outdoors: no dungeon to map."
			or string.format("%d of %d rooms read.", read, all)
	end)
end

-- Open the window, its lists read afresh.
local function open()
	L.refresh()
	refresh_results()
	refresh_saved()
	refresh_chat()
	refresh_clicks()
	show_picked()
	win:show()
	shown = true
end

-- Coordinates anyone writes in chat land in the From chat page's list.
on("chat.line", function(e)
	if C.heard(e) then
		refresh_chat()
	end
end)

-- A left click on the game's map points at the spot clicked, as a click on GoArrow's map did, and
-- is kept on the Map clicks page, where the host tells plugins of clicks on its map.
if host.has("map.clicks") then
	on("map.clicked", function(e)
		if e.button == "left" then
			table.insert(clicks, 1, e.position)
			while #clicks > MAX_CLICKS do
				table.remove(clicks)
			end
			refresh_clicks()
			point(P.show(e.position), e.position)
		end
	end)
end

on("ui.closed", function(e)
	if e.window == "main" then
		shown = false
	end
end)

on("ui.opened", function(e)
	if e.window == "main" then
		shown = true
		open()
		-- The HUD opens with the window: a window opens only when the player asks.
		if hud then
			hud:show()
		end
	end
end)

-- Lifestones and portals coming into view are kept as places of this world.
on("world.object_created", function(e)
	if e.class == "Lifestone" or e.class == "Portal" then
		local o = game.world:get(e.id)
		if o ~= nil and o.position ~= nil then
			L.seen(string.lower(e.class), e.name, o.position)
		end
	end
end)

on("ui.changed", function(e)
	if e.window == "main" and e.id == "search" then
		refresh_results()
	end
end)

on("ui.selected", function(e)
	if e.window ~= "main" then
		return
	end
	local row = e.rows and e.rows[1]
	if e.id == "results" and row and found[row] then
		pick(found[row])
	elseif e.id == "saved" and row and saved[row] then
		pick(saved[row])
		win:get("save_name").value = saved[row].name
	elseif e.id == "chat" and row and C.recent[row] then
		local c = C.recent[row]
		pick({ name = P.show(c.place), place = c.place })
	elseif e.id == "clicks" and row and clicks[row] then
		pick({ name = P.show(clicks[row]), place = clicks[row] })
	elseif e.id == "pages" then
		if page() == 1 then
			refresh_saved()
		elseif page() == 2 then
			refresh_chat()
		elseif page() == 3 then
			refresh_clicks()
		end
	elseif e.id == "ways" and row and plans[row] then
		win:get("legs").rows = R.leg_rows(plans[row].legs)
	end
end)

on("ui.clicked", function(e)
	if e.window ~= "main" then
		return
	end
	if e.id == "go" then
		point_picked(true)
	elseif e.id == "point" then
		point_picked(false)
	elseif e.id == "stop" then
		nav.stop()
		set_status("Stopped.")
	elseif e.id == "clear" then
		clear()
		set_status("")
	elseif e.id == "routes" then
		find_routes()
	elseif e.id == "save" then
		local name = win:get("save_name").value
		if name ~= nil and name ~= "" and L.save(name, P.here()) then
			say("Saved " .. name .. " (" .. P.show(P.here()) .. ").")
			refresh_saved()
			refresh_results()
		end
	elseif e.id == "forget" then
		local name = win:get("save_name").value
		if name ~= nil and L.forget(name) then
			say("Forgot " .. name .. ".")
			refresh_saved()
			refresh_results()
		end
	elseif e.id == "route_go" then
		local row = win:get("ways").selected
		local p = row and plans[row]
		go(p and p.way.opts or nil)
	end
end)

commands.add("nav", "point the way: /nav <place | coordinates | find <text> [type] | nearest <type> | here | save <name> | forget <name> | sel | obj <name> | last | routes | go | stop | travel <where> | plan <where> | route <file> | clear>", function(args, words)
	local verb = words[1] and string.lower(words[1])
	local rest = string.match(args, "^%s*%S+%s+(.-)%s*$") or ""
	if verb == nil then
		open()
	elseif verb == "go" then
		go()
	elseif verb == "stop" then
		nav.stop()
		set_status("Stopped.")
	elseif verb == "clear" then
		clear()
		set_status("")
	elseif verb == "here" then
		local here = P.here()
		if here == nil then
			say("Not in the world.")
		else
			local me = game.player.position
			say(string.format("Here: %s%s.", P.show(here), here.ns ~= nil and string.format(" (cell 0x%08X)", me.cell) or ""))
		end
	elseif verb == "find" then
		L.refresh()
		local text, kind = text_and_type(rest)
		local list = L.find(text, kind)
		if #list == 0 then
			say("No place matches " .. rest .. ".")
		end
		for i, l in ipairs(list) do
			if i > 8 then
				say(string.format("... and %d more.", #list - 8))
				break
			end
			say(string.format("%s (%s) %s%s", l.name, l.type, P.show(l.place),
				l.far and (", " .. P.show_distance(l.far)) or ""))
		end
		win:get("search").value = rest
		refresh_results()
	elseif verb == "nearest" and words[2] then
		L.refresh()
		local _, kind = text_and_type(rest)
		local l = kind and L.nearest(kind)
		if l then
			point_at(l)
		else
			say("No " .. rest .. " known near here.")
		end
	elseif verb == "save" and rest ~= "" then
		local here = P.here()
		if L.save(rest, here) then
			say("Saved " .. rest .. " (" .. P.show(here) .. ").")
			refresh_saved()
		else
			say("Not in the world.")
		end
	elseif verb == "forget" and rest ~= "" then
		say(L.forget(rest) and ("Forgot " .. rest .. ".") or ("No saved place " .. rest .. "."))
		refresh_saved()
	elseif verb == "sel" then
		local o = game.selection()
		if o then
			point(o.name, nil, nil, o.id)
		else
			say("Nothing is selected.")
		end
	elseif verb == "obj" and rest ~= "" then
		local o = game.world:first({ name_contains = rest, sort = "distance" })
		if o then
			point(o.name, nil, nil, o.id)
		else
			say("Nothing named " .. rest .. " in sight.")
		end
	elseif verb == "last" then
		local c = C.recent[1]
		if c then
			point(P.show(c.place), c.place)
		else
			say("No coordinates seen in chat yet.")
		end
	elseif verb == "routes" then
		win:get("pages").selected = WAYS_PAGE
		find_routes()
	elseif (verb == "travel" or verb == "plan") and rest ~= "" then
		if not host.has("nav.journey") then
			say("This client does not make journeys.")
			return
		end
		local to, name = journey_target(rest)
		if to == nil then
			say("No place, coordinates, cell or portal in " .. rest .. ".")
		elseif verb == "plan" then
			task.spawn(function()
				local ok, r = pawait(nav.plan(to))
				local line = ok and string.format("To %s: %s (about %d s).", name, R.legs_text(r.legs), math.floor(r.seconds + 0.5))
					or ("No way to " .. name .. ": " .. tostring(r) .. ".")
				say(line)
				set_status(line)
			end)
		else
			set_status("Going to " .. name .. "...")
			R.go(to, name, nil, tell)
		end
	elseif verb == "route" and words[2] then
		local text = files.package("routes/" .. words[2])
		if text == nil then
			say("No route " .. words[2] .. ".")
			return
		end
		local route = navlib.load(text)
		set_status("Walking the route " .. words[2] .. ".")
		task.spawn(function()
			local ok, why = pcall(route.walk, route, {
				range = 3,
				on_point = function(i)
					set_status(string.format("Walking the route %s: point %d of %d.", words[2], i, #route.points))
				end,
			})
			tell.line(ok and "Route done." or ("Route stopped: " .. tostring(why)))
		end)
	else
		local p = P.parse(args)
		if p then
			point(P.show(p), p)
		else
			local l = L.named(args)
			if l then
				point_at(l)
			else
				say("No place or coordinates in " .. args .. ".")
			end
		end
	end
end)
