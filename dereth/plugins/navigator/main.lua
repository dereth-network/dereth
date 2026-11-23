-- Navigator: gets you from point A to point B, in the vein of GoArrow for Decal. One "Where to?"
-- box finds places as the player types (dungeons, where portals send, portals, lifestones, shops,
-- people, houses, recalls and places of the player's own, by name and type, or coordinates typed
-- in), and one list holds them with the player's saved places, the coordinates seen in chat and
-- the spots picked on the game's map, a Filter choosing which. The place picked is the
-- destination; Go travels there (through portals and by recalls where the client knows a
-- journey, else on foot round what stands in the way), Stop stops, Clear forgets it. Dungeon Map
-- opens a map of the dungeon the character is in, with the way and the creatures on it.
--
-- Every place is one the client learned in this world or the player saved: the plugin brings none
-- of its own. The destination, the map's spots and the coordinates seen in chat are kept for the
-- world, so they are there again after a reload.
--
-- The commands are shortcuts to the same:
--
-- /nav                     open the window
-- /nav <place>             make a place (by name, of any type) or coordinates (12.3N, 45.6E) the
--                          destination
-- /nav find <text> [type]  find places whose name holds the text, nearest first (in the window too)
-- /nav nearest <type>      make the nearest place of a type the destination
-- /nav here                say where the character is
-- /nav save <name>         keep where the character stands as a place of the player's own
-- /nav forget <name>       forget one
-- /nav sel                 make the selected object the destination, following it as it moves
-- /nav obj <name>          make the nearest object whose name holds the text the destination
-- /nav last                make the last coordinates anyone wrote in chat the destination
-- /nav routes              the ways to the destination: best, without recalls, without portals,
--                          on foot
-- /nav go | stop           go to the destination (a gesture), or stop
-- /nav travel <where>      go somewhere by a journey: a place, coordinates, a cell (0x019E0140),
--                          or "portal <name>" for where a known portal sends (a dungeon, say)
-- /nav plan <where>        the journey there, without going
-- /nav route <file>        walk a VTank route from the plugin's data folder
-- /nav clear               forget the destination, and take its markers down

local navlib = require("dereth.nav")
local P = require("places")
local L = require("locations")
local C = require("chatcoords")
local R = require("route")
local M = require("map")

-- The destination: `{ name, place }` for a place, `{ name, object }` for an object it follows;
-- `travel` is what a journey goes to (a portal's destination for a dungeon, say).
local target = nil
local found = {} -- the list's rows, as locations
local clicks = {} -- the spots picked on the game's map, newest first
local journey = nil -- the legs of the journey under way, or nil
local moving = false -- whether a Go, a travel or a route of this plugin's is under way
local run = 0 -- which Go, travel or route is the latest: an older one's end changes nothing
local way = nil -- the walking route to the destination, as the dungeon map draws it: `{ points }`
local status = ""

local MAX_RESULTS = 200
local MAX_CLICKS = 10
-- The filter's choices, and the list's columns: the delete column is the fourth.
local FILTERS = { "Everything", "Saved", "Chat", "Map" }
local DELETE_COLUMN = 4
-- Two left clicks on the map this close in time (seconds) and place (map units) are a double-click.
local DOUBLE_SECS = 0.5
local DOUBLE_NEAR = 0.05

local function say(text)
	chat.print("[Nav] " .. text)
end

-- The world's store, or nil before login.
local function world()
	local ok, s = pcall(function()
		return store.world
	end)
	return ok and s or nil
end

-- The main window, about nine tenths the map window's width and under half its height: the list
-- shows seven places.
local win = ui.window({
	id = "main",
	title = "Navigator",
	width = 72,
	height = 17,
	ui.input({ id = "search", text = "Where to?", value = "", placeholder = "a name, a type (dungeon, portal...) or 12.3N 45.6E" }),
	ui.choice({ id = "filter", text = "Filter", options = FILTERS, value = FILTERS[1] }),
	ui.list({ id = "results", columns = { { title = "Name", grow = 1 }, { title = "Type", width = 12 },
		{ title = "Where", width = 11 }, { title = "", width = 2 } }, rows = {}, height = 7, grow = 1 }),
	ui.label({ id = "to", text = "Pick a place, then Go." }),
	ui.row({
		ui.button({ id = "go", text = "Go" }),
		ui.button({ id = "stop", text = "Stop", enabled = false }),
		ui.button({ id = "clear", text = "Clear", enabled = false }),
		ui.label({ text = "", grow = 1 }),
		ui.button({ id = "dungeon", text = "Dungeon Map", enabled = false, tooltip = "You're outdoors" }),
	}),
	ui.label({ id = "status", text = "" }),
	ui.row({
		ui.input({ id = "save_name", text = "Name", value = "", grow = 1 }),
		ui.label({ id = "here", text = "" }),
		ui.button({ id = "save", text = "Save" }),
	}),
})
win:close()
local shown = false -- whether the window is open

-- The dungeon map, in a window of its own.
local map_win = nil
if M.available() then
	map_win = ui.window({
		id = "dungeon",
		title = "Dungeon Map",
		ui.canvas({ id = "map", width = M.SIZE, height = M.SIZE, background = "#101010", draw = {} }),
		ui.label({ id = "map_note", text = "" }),
	})
	map_win:close()
end
local map_shown = false

local function set_status(text)
	status = text
	win:get("status").text = text
end

-- Stop is up only while going, Clear only while there is a destination.
local function sync_buttons()
	win:get("stop").enabled = moving
	win:get("clear").enabled = target ~= nil
end

-- Keep the destination (a place: an object's id means nothing after a reload), the map's spots
-- and the coordinates seen in chat for this world.
local function remember()
	local w = world()
	if w == nil then
		return
	end
	local t = nil
	if target ~= nil and target.object == nil and target.place ~= nil then
		t = { name = target.name, place = target.place, travel = target.travel }
	end
	w.nav = { target = t, clicks = clicks, chat = C.recent }
end

-- Stop what this plugin set going: a journey, a walk or a route.
local function halt()
	run = run + 1
	if moving then
		nav.stop()
		moving = false
		journey = nil
	end
	sync_buttons()
end

-- Start a Go, a travel or a route: its number, which `finished` takes.
local function start()
	halt()
	run = run + 1
	moving = true
	sync_buttons()
	return run
end

-- Whether run `id` is still the latest; when it is, it is over.
local function finished(id)
	if id ~= run then
		return false
	end
	moving = false
	journey = nil
	sync_buttons()
	return true
end

-- The words of a search, and a type named last ("hideout dungeon"), or a type alone ("dungeon").
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

-- How many places the client knows, and from where, as the status line says it before a search:
-- what the Where to? box has to go on.
local WHERE = { learned = "seen", published = "the server", hint = "the era's package", data = "the game's data" }
local function knowledge_line()
	if nav.knowledge == nil then
		return ""
	end
	local k = nav.knowledge()
	local function said(counts, what)
		local total, parts = 0, {}
		for _, source in ipairs({ "learned", "published", "hint", "data" }) do
			local n = counts[source]
			if n ~= nil and n > 0 then
				total = total + n
				table.insert(parts, string.format("%d from %s", n, WHERE[source]))
			end
		end
		if total == 0 then
			return "no " .. what
		end
		return string.format("%d %s (%s)", total, what, table.concat(parts, ", "))
	end
	return "Knows " .. said(k.portals or {}, "portals") .. " and " .. said(k.places or {}, "places") .. "."
end
local knowing = ""

-- Whether `name` holds the words of a search (any, when there are none).
local function matches(name, text)
	if text == nil or text == "" then
		return true
	end
	return string.find(string.lower(name or ""), string.lower(text), 1, true) ~= nil
end

-- The list's rows: the filter's places whose name holds the words typed. Everything is the
-- coordinates typed in, the spots picked on the map and seen in chat, then the places found (the
-- player's saved places among them), of the type named, nearest first. A row the player may
-- delete (a saved place, a line of chat, a spot on the map) has its mark in the delete column.
local function refresh_results()
	local typed = win:get("search").value or ""
	local filter = win:get("filter").value or FILTERS[1]
	-- Before a search, what there is to search; never over a journey's progress.
	if typed == "" and (status == "" or status == knowing) then
		knowing = knowledge_line()
		set_status(knowing)
	end
	found = {}
	local p = P.parse(typed)
	local text, kind = text_and_type(typed)
	if p ~= nil then
		text = ""
	end
	if filter == "Everything" and p ~= nil then
		table.insert(found, { name = P.show(p), type = "coordinates", place = p })
	end
	if filter == "Everything" or filter == "Map" then
		for i, c in ipairs(clicks) do
			local name = P.show(c)
			if matches(name, text) then
				table.insert(found, { name = name, type = "map", place = c, map = i })
			end
		end
	end
	if filter == "Everything" or filter == "Chat" then
		for i, c in ipairs(C.recent) do
			local name = c.who .. ": " .. P.show(c.place)
			if matches(name, text) then
				table.insert(found, { name = name, type = "chat", place = c.place, chat = i })
			end
		end
	end
	if p == nil and (filter == "Everything" or filter == "Saved") then
		for _, l in ipairs(L.find(text, filter == "Saved" and "saved" or kind)) do
			if #found >= MAX_RESULTS then
				break
			end
			if l.type == "saved" then
				l.saved = l.name
			end
			table.insert(found, l)
		end
	end
	local rows = {}
	for _, l in ipairs(found) do
		local deletable = l.saved ~= nil or l.chat ~= nil or l.map ~= nil
		table.insert(rows, { l.name, l.type, P.short(l.place), deletable and "X" or "" })
	end
	win:get("results").rows = rows
	win:get("results").selected = nil
end

-- The destination, in words.
local function show_target()
	local t = target
	if t == nil then
		win:get("to").text = "Pick a place, then Go."
		return
	end
	local where = t.object and "following it" or P.long(t.place)
	win:get("to").text = string.format("Destination: %s (%s)", t.name, where)
end

-- Put the markers on the destination: one in the world and one on the map, where the host draws
-- them.
local function mark()
	markers.clear()
	local t = target
	if t == nil then
		return
	end
	if host.has("overlay.world") then
		if t.object then
			markers.world:add({ object = t.object, label = t.name })
		else
			markers.world:add({ position = t.place, label = t.name })
		end
	end
	if host.has("overlay.map") and t.place and t.place.ns ~= nil then
		markers.map:add({ points = { t.place }, label = t.name })
	end
end

-- Make a place (`place`) or an object (`object`, an id) the destination. Whatever this plugin had
-- the character traveling to before stops: it was going somewhere else.
local function destine(name, place, travel, object)
	halt()
	way = nil
	target = { name = name, place = place, travel = travel or place, object = object }
	mark()
	show_target()
	sync_buttons()
	remember()
end

local function destine_at(l)
	destine(l.name, l.place, l.target)
	say("Destination: " .. l.name .. " (" .. P.long(l.place) .. ").")
end

local function clear()
	halt()
	target = nil
	way = nil
	markers.clear()
	show_target()
	sync_buttons()
	remember()
end

-- Where the destination is now: its place, or its object's.
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

-- Say `text` in the chat and on the status line.
local function tell_line(text)
	say(text)
	set_status(text)
end

-- The progress of journey `id`, on the window's status line; nothing once a later Go, travel or
-- route has started, or this one was stopped.
local function teller(id)
	return {
		line = function(text)
			if id == run then
				tell_line(text)
			end
		end,
		planned = function(legs)
			if id == run then
				journey = legs
			end
		end,
		leg = function(index, what)
			if id == run then
				set_status(string.format("Leg %d of %d: %s", index, journey and #journey or index, what))
			end
		end,
		ended = function()
			finished(id)
		end,
	}
end

-- The way ahead, drawn in the world while a journey of ours runs (with `overlay.world`): a faint
-- glow from the character along the journey's way still to walk. The host keeps the path, starts
-- it at the character, moves it, and cross-fades it with the one it replaces; it is handed over
-- again only when the way changes, not as the character passes its corners.
local trail = nil -- the path drawn: `{ handle, keys }`, the keys of the route's places
local TRAIL_EVERY = 0.5 -- how often the route is looked at, seconds

local function place_key(p)
	return string.format("%s %.2f %.2f %.2f %.3f %.3f", tostring(p.cell or ""), p.x or 0, p.y or 0,
		p.z or 0, p.ns or 0, p.ew or 0)
end

local function clear_trail()
	if trail ~= nil then
		trail.handle:remove()
		trail = nil
	end
end

-- Draw `route` (the way still to walk), unless it is the end of the way drawn already.
local function draw_trail(route)
	if route == nil or #route == 0 then
		clear_trail()
		return
	end
	local keys = {}
	for i, p in ipairs(route) do
		keys[i] = place_key(p)
	end
	if trail ~= nil then
		local skip = #trail.keys - #keys
		local same = skip >= 0
		for i = 1, #keys do
			if not same then
				break
			end
			same = trail.keys[skip + i] == keys[i]
		end
		if same then
			return
		end
	end
	clear_trail()
	local points = {}
	for _, p in ipairs(route) do
		table.insert(points, p)
	end
	if #points < 2 then
		table.insert(points, 1, game.player.position)
	end
	local ok, handle = pcall(draw.path, { points = points, look = "glow" })
	if ok then
		trail = { handle = handle, keys = keys }
	end
end

-- Keep run `id`'s way drawn while it is the latest and its journey lasts.
local function follow_trail(id)
	if not (host.has("overlay.world") and host.has("nav.journey")) then
		return
	end
	task.spawn(function()
		while id == run and moving do
			local j = nav.journey()
			draw_trail(j and j.way)
			task.sleep(TRAIL_EVERY * 1000)
		end
		if id == run or trail ~= nil then
			clear_trail()
		end
	end)
end

-- Walk to `to` with no journey to follow: along the client's walking route there when it plans
-- one (round walls and through doors), corner by corner, else straight at it.
local function walk_to(to, name)
	set_status("Walking to " .. name .. "...")
	local id = start()
	task.spawn(function()
		local corners = nil
		if host.has("nav.path") then
			local ok, r = pawait(nav.path(to))
			if ok and r ~= nil and r.points ~= nil then
				corners = r.points
			end
		end
		local ok, why = true, nil
		if corners ~= nil then
			for i = 2, #corners - 1 do
				ok, why = pawait(nav.move_to(corners[i], 1))
				if not ok then
					break
				end
			end
		end
		if ok then
			ok, why = pawait(nav.move_to(to, 3))
		end
		if finished(id) then
			tell_line(ok and ("Arrived at " .. name .. ".") or ("Stopped: " .. tostring(why) .. "."))
		end
	end)
end

-- Go to the destination: by a journey (with `opts`, a way of the route finder), otherwise walking.
local function go(opts)
	if target == nil then
		say("No destination.")
		set_status("Pick a place first.")
		return
	end
	local t = target
	local to = t.object or t.travel
	if host.has("nav.journey") then
		local id = start()
		set_status("Going to " .. t.name .. "...")
		R.go(to, t.name, opts, teller(id))
		follow_trail(id)
		return
	end
	if not host.has("nav.moveto") then
		say("This client cannot walk for you.")
		set_status("This client cannot walk for you.")
		return
	end
	walk_to(to, t.name)
end

-- The route finder: each way to the destination, in the chat.
local function find_routes()
	if target == nil then
		say("No destination.")
		return
	end
	if not host.has("nav.journey") then
		say("This client does not make journeys.")
		return
	end
	local t = target
	R.plan_all(t.object or t.travel, function(plans)
		for _, p in ipairs(plans) do
			say(string.format("%s to %s: %s (about %d s).", p.way.name, t.name, R.legs_text(p.legs),
				math.floor(p.seconds + 0.5)))
		end
		if #plans == 0 then
			say("No way to " .. t.name .. " is known.")
		end
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

-- What was kept for this world, read once the world's store is there: the map's spots, the
-- coordinates seen in chat and the destination, its markers put back.
local restored = false
local function restore()
	local w = world()
	if w == nil then
		return
	end
	restored = true
	local n = w.nav
	if n == nil then
		return
	end
	clicks = n.clicks or {}
	C.recent = n.chat or {}
	if n.target ~= nil and n.target.place ~= nil and target == nil then
		target = { name = n.target.name, place = n.target.place, travel = n.target.travel or n.target.place }
		mark()
		show_target()
		sync_buttons()
	end
end

-- Indoors or out, a few times a second: where the character stands, by the Name box, and the
-- Dungeon Map button, up only indoors.
task.every(250, function()
	if not restored then
		restore()
	end
	local here = P.here()
	win:get("here").text = here and P.long(here) or ""
	local inside = here ~= nil and here.ns == nil and map_win ~= nil
	local b = win:get("dungeon")
	b.enabled = inside
	b.tooltip = not inside and "You're outdoors" or nil
end)

-- The way to the destination as the dungeon map draws it: asked of the client once for each
-- destination, and again every few seconds as the character moves along it.
local WAY_EVERY = 3
local way_asked = -math.huge
local function refresh_way()
	if target == nil or not host.has("nav.path") or os.clock() - way_asked < WAY_EVERY then
		return
	end
	local at = target_place()
	if at == nil or at.ns ~= nil then
		way = nil
		return
	end
	way_asked = os.clock()
	local t = target
	task.spawn(function()
		local ok, r = pawait(nav.path(t.object or at))
		if target == t then
			way = ok and r ~= nil and { points = r.points } or nil
		end
	end)
end

-- The dungeon map, while its window is open: a few cells read a step, drawn again as the
-- character moves or turns, or the way or the creatures change.
local drawn_at = nil
if map_win ~= nil then
	task.every(100, function()
		if not map_shown then
			return
		end
		local done = M.step()
		local me = game.player.position
		if me == nil then
			return
		end
		refresh_way()
		local creatures = {}
		local ckey = {}
		if M.inside(me) ~= nil then
			for _, o in ipairs(game.world:find({ monster = true, within = 60 })) do
				if o.position ~= nil then
					table.insert(creatures, o.position)
					table.insert(ckey, string.format("%.0f,%.0f", o.position.x or 0, o.position.y or 0))
				end
			end
		end
		local key = string.format("%s %.1f %.1f %.1f %d %s %s %s %s", tostring(me.cell), me.x or 0, me.y or 0,
			me.z or 0, math.floor((me.heading or 0) / 5), tostring(done), tostring(way),
			tostring(target), table.concat(ckey, ";"))
		if key == drawn_at then
			return
		end
		drawn_at = key
		local cmds = M.draw({ target = target_place(), route = way and way.points, creatures = creatures })
		map_win:get("map").draw = cmds or {}
		local read, all = M.progress()
		map_win:get("map_note").text = cmds == nil and "Outdoors: no dungeon to map."
			or string.format("%d of %d rooms read.", read, all)
	end)
end

-- Open the window, its list read afresh.
local function open()
	L.refresh()
	refresh_results()
	show_target()
	sync_buttons()
	win:show()
	shown = true
end

-- Coordinates anyone writes in chat land in the list.
on("chat.line", function(e)
	if C.heard(e) then
		if shown then
			refresh_results()
		end
		remember()
	end
end)

-- A spot on the game's map, picked with a right-click or a double-click (a single click is the
-- map's own, for dragging it), is kept in the list and made the destination, where the host
-- tells plugins of clicks on its map.
local last_click = nil
local function picked_on_map(at)
	table.insert(clicks, 1, at)
	while #clicks > MAX_CLICKS do
		table.remove(clicks)
	end
	if shown then
		refresh_results()
	end
	destine(P.show(at), at)
	say("Destination: " .. P.show(at) .. ".")
end
if host.has("map.clicks") then
	on("map.clicked", function(e)
		local at = e.position
		if e.button == "right" then
			last_click = nil
			picked_on_map(at)
		elseif e.button == "left" then
			local now = os.clock()
			local l = last_click
			if l ~= nil and now - l.time <= DOUBLE_SECS and math.abs(at.ns - l.at.ns) <= DOUBLE_NEAR
				and math.abs(at.ew - l.at.ew) <= DOUBLE_NEAR then
				last_click = nil
				picked_on_map(at)
			else
				last_click = { time = now, at = at }
			end
		end
	end)
end

on("ui.closed", function(e)
	if e.window == "main" then
		shown = false
	elseif e.window == "dungeon" then
		map_shown = false
	end
end)

on("ui.opened", function(e)
	if e.window == "main" then
		shown = true
		open()
	elseif e.window == "dungeon" then
		map_shown = true
		drawn_at = nil
	end
end)

on("ui.changed", function(e)
	if e.window == "main" and (e.id == "search" or e.id == "filter") then
		refresh_results()
	end
end)

-- Delete a row's entry: a saved place forgotten, a line of chat or a spot on the map dropped.
local function delete(l)
	if l.saved ~= nil then
		L.forget(l.saved)
		say("Forgot " .. l.saved .. ".")
	elseif l.chat ~= nil then
		table.remove(C.recent, l.chat)
	elseif l.map ~= nil then
		table.remove(clicks, l.map)
	end
	remember()
	refresh_results()
end

on("ui.selected", function(e)
	if e.window ~= "main" or e.id ~= "results" then
		return
	end
	local row = e.rows and e.rows[1]
	local l = row and found[row]
	if l == nil then
		return
	end
	if e.column == DELETE_COLUMN and (l.saved ~= nil or l.chat ~= nil or l.map ~= nil) then
		delete(l)
		return
	end
	destine(l.name, l.place, l.target)
	if l.saved ~= nil then
		win:get("save_name").value = l.saved
	end
end)

on("ui.clicked", function(e)
	if e.window ~= "main" then
		return
	end
	if e.id == "go" then
		-- The row selected is where Go goes, whatever was the destination before.
		local sel = win:get("results").selected
		local l = type(sel) == "table" and found[sel[1]] or found[sel]
		if l ~= nil and (target == nil or target.name ~= l.name or target.place ~= l.place) then
			destine(l.name, l.place, l.target)
		end
		go()
	elseif e.id == "stop" then
		halt()
		nav.stop()
		set_status("Stopped.")
	elseif e.id == "clear" then
		clear()
		set_status("")
	elseif e.id == "dungeon" and map_win ~= nil then
		map_win:show()
		map_shown = true
		drawn_at = nil
	elseif e.id == "save" then
		local name = win:get("save_name").value
		if name ~= nil and name ~= "" and L.save(name, P.here()) then
			say("Saved " .. name .. " (" .. P.long(P.here()) .. ").")
			refresh_results()
		end
	end
end)

commands.add("nav", "get from point A to point B: /nav <place | coordinates | find <text> [type] | nearest <type> | here | known | save <name> | forget <name> | sel | obj <name> | last | routes | go | stop | travel <where> | plan <where> | route <file> | clear>", function(args, words)
	local verb = words[1] and string.lower(words[1])
	local rest = string.match(args, "^%s*%S+%s+(.-)%s*$") or ""
	if verb == nil then
		open()
	elseif verb == "go" then
		go()
	elseif verb == "stop" then
		halt()
		nav.stop()
		set_status("Stopped.")
	elseif verb == "clear" then
		clear()
		set_status("")
	elseif verb == "known" then
		say(knowledge_line())
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
		local list, total = L.find(text, kind, 8)
		if #list == 0 then
			say("No place matches " .. rest .. ".")
		end
		for i, l in ipairs(list) do
			if i > 8 then
				break
			end
			say(string.format("%s (%s) %s%s", l.name, l.type, P.show(l.place),
				l.far and (", " .. P.show_distance(l.far)) or ""))
		end
		-- How many more there are, of those found by name (a portal is one, however many rows).
		local said = math.min(#list, 8)
		if total > said then
			say(string.format("... and %d more.", total - said))
		end
		win:get("search").value = rest
		refresh_results()
	elseif verb == "nearest" and words[2] then
		L.refresh()
		local _, kind = text_and_type(rest)
		local l = kind and L.nearest(kind)
		if l then
			destine_at(l)
		else
			say("No " .. rest .. " known near here.")
		end
	elseif verb == "save" and rest ~= "" then
		local here = P.here()
		if L.save(rest, here) then
			say("Saved " .. rest .. " (" .. P.long(here) .. ").")
			refresh_results()
		else
			say("Not in the world.")
		end
	elseif verb == "forget" and rest ~= "" then
		say(L.forget(rest) and ("Forgot " .. rest .. ".") or ("No saved place " .. rest .. "."))
		refresh_results()
	elseif verb == "sel" then
		local o = game.selection()
		if o then
			destine(o.name, nil, nil, o.id)
			say("Destination: " .. o.name .. " (following it).")
		else
			say("Nothing is selected.")
		end
	elseif verb == "obj" and rest ~= "" then
		local o = game.world:first({ name_contains = rest, sort = "distance" })
		if o then
			destine(o.name, nil, nil, o.id)
			say("Destination: " .. o.name .. " (following it).")
		else
			say("Nothing named " .. rest .. " in sight.")
		end
	elseif verb == "last" then
		local c = C.recent[1]
		if c then
			destine(P.show(c.place), c.place)
			say("Destination: " .. P.show(c.place) .. ".")
		else
			say("No coordinates seen in chat yet.")
		end
	elseif verb == "routes" then
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
			local id = start()
			set_status("Going to " .. name .. "...")
			R.go(to, name, nil, teller(id))
			follow_trail(id)
		end
	elseif verb == "route" and words[2] then
		local text = files.data(words[2]) or files.data("routes/" .. words[2])
		if text == nil then
			say("No route " .. words[2] .. " in the Navigator's data folder.")
			return
		end
		local route = navlib.load(text)
		local id = start()
		set_status("Walking the route " .. words[2] .. ".")
		task.spawn(function()
			local ok, why = pcall(route.walk, route, {
				range = 3,
				on_point = function(i)
					set_status(string.format("Walking the route %s: point %d of %d.", words[2], i, #route.points))
				end,
			})
			if finished(id) then
				tell_line(ok and "Route done." or ("Route stopped: " .. tostring(why)))
			end
		end)
	else
		local p = P.parse(args)
		if p then
			destine(P.show(p), p)
			say("Destination: " .. P.show(p) .. ".")
		else
			local l = L.named(args)
			if l then
				destine_at(l)
			else
				say("No place or coordinates in " .. args .. ".")
			end
		end
	end
end)
