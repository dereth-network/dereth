-- The plugin API's Lua-level bindings: every namespace a plugin sees, written over one host call.
--
-- This chunk runs once per VM, before the sandbox is sealed. It receives the host call, a compiler
-- for the plugin's own modules, the traceback function and the sources of the host's libraries
-- written in Lua, and returns the API table (whose entries become read-only globals) and the event
-- dispatcher (which the engine keeps and never exposes).

local hostcall, compile, traceback, libraries = ...

local api = {}
local handlers = {} -- event name -> list of functions, in registration order
local timers = {} -- timer id -> { fn = function } or { co = coroutine }, and repeats
local commands = {} -- command name -> function
local modules = {} -- module name -> loaded value
local promises = {} -- action id -> its promise, until it settles
local services = {} -- service name -> the function that answers it

-- Events -----------------------------------------------------------------------------------------

function api.on(name, fn)
	assert(type(name) == "string", "on(name, fn): name must be a string")
	assert(type(fn) == "function", "on(name, fn): fn must be a function")
	local list = handlers[name]
	if list == nil then
		hostcall("events.subscribe", name)
		list = {}
		handlers[name] = list
	end
	table.insert(list, fn)
	return fn
end

function api.off(name, fn)
	local list = handlers[name]
	if list == nil then
		return false
	end
	for i, f in ipairs(list) do
		if f == fn then
			table.remove(list, i)
			if #list == 0 then
				handlers[name] = nil
				hostcall("events.unsubscribe", name)
			end
			return true
		end
	end
	return false
end

-- Output -----------------------------------------------------------------------------------------

function api.print(...)
	local parts = table.pack(...)
	for i = 1, parts.n do
		parts[i] = tostring(parts[i])
	end
	hostcall("console.print", table.concat(parts, "\t", 1, parts.n))
end

api.chat = {
	print = function(text, style)
		hostcall("chat.print", tostring(text), style)
	end,
	-- Say a line, or type a command, as the player would in the chat bar (`chat.send`).
	say = function(text)
		hostcall("chat.send", tostring(text))
	end,
	command = function(line)
		line = tostring(line)
		if string.sub(line, 1, 1) ~= "/" and string.sub(line, 1, 1) ~= "@" then
			line = "/" .. line
		end
		hostcall("chat.send", line)
	end,
	-- Hide an incoming line: `chat.hide(e)` or `chat.hide(e.line)` in a `chat.line` handler.
	hide = function(line)
		if type(line) == "table" then
			line = line.line
		end
		hostcall("chat.hide", line)
	end,
}

-- Commands ---------------------------------------------------------------------------------------

api.commands = {
	add = function(name, help, fn)
		assert(type(name) == "string", "commands.add(name, help, fn): name must be a string")
		assert(type(fn) == "function", "commands.add(name, help, fn): fn must be a function")
		hostcall("commands.add", name, help or "")
		commands[name] = fn
	end,
	remove = function(name)
		hostcall("commands.remove", name)
		commands[name] = nil
	end,
}

-- Tasks ------------------------------------------------------------------------------------------

local function report(err)
	hostcall("task.error", tostring(err))
end

local function resume(co, ...)
	local ok, err = coroutine.resume(co, ...)
	if not ok then
		report(traceback(co, tostring(err)))
	end
end

api.task = {
	every = function(ms, fn)
		assert(type(fn) == "function", "task.every(ms, fn): fn must be a function")
		local id = hostcall("task.timer", ms, true)
		timers[id] = { fn = fn, again = true }
		return id
	end,
	after = function(ms, fn)
		assert(type(fn) == "function", "task.after(ms, fn): fn must be a function")
		local id = hostcall("task.timer", ms, false)
		timers[id] = { fn = fn }
		return id
	end,
	cancel = function(id)
		if timers[id] ~= nil then
			timers[id] = nil
			hostcall("task.cancel", id)
		end
	end,
	spawn = function(fn, ...)
		assert(type(fn) == "function", "task.spawn(fn, ...): fn must be a function")
		local co = coroutine.create(fn)
		resume(co, ...)
		return co
	end,
	sleep = function(ms)
		assert(coroutine.isyieldable(), "task.sleep can only be called inside task.spawn")
		local co = coroutine.running()
		local id = hostcall("task.timer", ms, false)
		timers[id] = { co = co }
		coroutine.yield()
	end,
}

-- The store --------------------------------------------------------------------------------------

local function scope(name)
	return setmetatable({}, {
		__index = function(_, key)
			return hostcall("store.get", name, key)
		end,
		__newindex = function(_, key, value)
			hostcall("store.set", name, key, value)
		end,
	})
end

api.store = { character = scope("character"), world = scope("world"), global = scope("global") }

-- The plugin's own files, read-only.
api.files = {
	package = function(path)
		assert(type(path) == "string", "files.package(path): path must be a string")
		return hostcall("files.package", path)
	end,
	-- Every file of the plugin's own package, by path.
	package_list = function()
		return hostcall("files.package_list")
	end,
	-- The plugin's data folder: files the player puts there, and files it saves there.
	data = function(path)
		assert(type(path) == "string", "files.data(path): path must be a string")
		return hostcall("files.data", path)
	end,
	data_list = function()
		return hostcall("files.data_list")
	end,
	save_data = function(path, text)
		assert(type(path) == "string" and type(text) == "string", "files.save_data(path, text) takes two strings")
		return hostcall("files.save_data", path, text)
	end,
}

-- Promises -----------------------------------------------------------------------------------------

-- A property table keyed by number, from the host's text keys.
local function numbered(t)
	if t == nil then
		return nil
	end
	local out = {}
	for k, v in pairs(t) do
		out[tonumber(k) or k] = v
	end
	return out
end

local promise_methods = {}
local promise_meta = {
	__index = promise_methods,
	__tostring = function(p)
		return "promise " .. tostring(p.action) .. " (" .. p.state .. ")"
	end,
}

local function new_promise(id, action, transform)
	local p = setmetatable(
		{ id = id, action = action, state = "pending", waiters = {}, callbacks = {}, transform = transform },
		promise_meta
	)
	promises[id] = p
	return p
end

-- `p:next(on_done, on_failed)`: call one of them when the action ends (at once if it has).
function promise_methods:next(on_done, on_failed)
	if self.state == "completed" then
		if on_done then
			on_done(self.value)
		end
	elseif self.state == "failed" then
		if on_failed then
			on_failed(self.reason)
		end
	else
		table.insert(self.callbacks, { on_done, on_failed })
	end
	return self
end

-- Waits for a promise inside `task.spawn`: answers its value, or raises its failure.
function api.await(p)
	assert(getmetatable(p) == promise_meta, "await(p): p must be a promise")
	if p.state == "pending" then
		assert(coroutine.isyieldable(), "await can only be called inside task.spawn")
		table.insert(p.waiters, coroutine.running())
		coroutine.yield()
	end
	if p.state == "completed" then
		return p.value
	end
	error(tostring(p.action) .. " failed: " .. tostring(p.reason), 2)
end

-- As `await`, but answers `true, value` or `false, reason` instead of raising.
function api.pawait(p)
	local ok, v = pcall(api.await, p)
	if ok then
		return true, v
	end
	return false, p.reason
end

local function request(name, args, transform)
	return new_promise(hostcall("act.request", name, args or {}), name, transform)
end

-- An appraisal's property tables keyed by number.
local function appraisal_value(a)
	if type(a) ~= "table" then
		return a
	end
	for _, k in ipairs({ "ints", "int64s", "floats", "strings", "bools" }) do
		a[k] = numbered(a[k])
	end
	return a
end

api.act = {
	select = function(id)
		return request("act.select", { id = id })
	end,
	use = function(id)
		return request("act.use", { id = id })
	end,
	-- `act.use_on(item, target, { purpose = "heal" })`: a wielded item's own spell used as a heal
	-- counts as one.
	use_on = function(item, target, opts)
		opts = opts or {}
		return request("act.use_on", { item = item, target = target, purpose = opts.purpose })
	end,
	appraise = function(id)
		return request("act.appraise", { id = id }, appraisal_value)
	end,
}

api.items = {
	move = function(id, container, slot)
		return request("items.move", { id = id, container = container, slot = slot })
	end,
	split = function(id, count, container)
		return request("items.split", { id = id, count = count, container = container })
	end,
	merge = function(id, target, count)
		return request("items.merge", { id = id, target = target, count = count })
	end,
	drop = function(id)
		return request("items.drop", { id = id })
	end,
	give = function(id, target, count)
		return request("items.give", { id = id, target = target, count = count })
	end,
	wield = function(id, opts)
		return request("items.wield", { id = id, purpose = opts and opts.purpose })
	end,
	unwield = function(id, opts)
		return request("items.unwield", { id = id, purpose = opts and opts.purpose })
	end,
	inscribe = function(id, text)
		return request("items.inscribe", { id = id, text = text })
	end,
	salvage = function(ids, tool)
		return request("items.salvage", { ids = ids, tool = tool })
	end,
}

api.vendor = {
	-- `vendor.buy{ {id = …, count = …}, … }`
	buy = function(items)
		return request("vendor.buy", { items = items })
	end,
	sell = function(ids)
		return request("vendor.sell", { ids = ids })
	end,
	close = function()
		return request("vendor.close")
	end,
	fill_components = function(max_price)
		return request("vendor.fill_components", { max_price = max_price })
	end,
	info = function()
		return hostcall("vendor.info")
	end,
}

api.trade = {
	open = function(player)
		return request("trade.open", { id = player })
	end,
	add = function(id)
		return request("trade.add", { id = id })
	end,
	accept = function()
		return request("trade.accept")
	end,
	decline = function()
		return request("trade.decline")
	end,
	reset = function()
		return request("trade.reset")
	end,
	close = function()
		return request("trade.close")
	end,
	info = function()
		return hostcall("trade.info")
	end,
}

api.social = {
	fellowship_create = function(name, share_xp)
		return request("social.fellowship_create", { name = name, share_xp = share_xp })
	end,
	fellowship_quit = function(disband)
		return request("social.fellowship_quit", { disband = disband })
	end,
	fellowship_recruit = function(id)
		return request("social.fellowship_recruit", { id = id })
	end,
	fellowship_dismiss = function(id)
		return request("social.fellowship_dismiss", { id = id })
	end,
	fellowship_leader = function(id)
		return request("social.fellowship_leader", { id = id })
	end,
	fellowship_openness = function()
		return request("social.fellowship_openness")
	end,
	friend_add = function(name)
		return request("social.friend_add", { name = name })
	end,
	friend_remove = function(id)
		return request("social.friend_remove", { id = id })
	end,
}

api.character = {
	raise_skill = function(id)
		return request("character.raise_skill", { id = id })
	end,
	raise_attribute = function(id)
		return request("character.raise_attribute", { id = id })
	end,
	raise_vital = function(id)
		return request("character.raise_vital", { id = id })
	end,
	set_option = function(name, value)
		return request("character.set_option", { name = name, value = value })
	end,
}

api.magic = {
	-- `magic.cast(spell, target, { purpose = "heal" })`: a Life spell cast as a heal counts as one.
	cast = function(spell, target, opts)
		opts = opts or {}
		return request("magic.cast", { spell = spell, target = target, purpose = opts.purpose })
	end,
	spell = function(id)
		return hostcall("magic.spell", id)
	end,
	known = function(from, count)
		return hostcall("magic.known", from, count)
	end,
}

-- An object handle or id as an id; anything else unchanged.
local function id_or(v)
	if type(v) == "table" and rawget(v, "id") ~= nil and getmetatable(v) ~= nil then
		return rawget(v, "id")
	end
	return v
end

-- A movement target: an object (handle or id) or a place table.
local function target_of(t)
	t = id_or(t)
	if type(t) == "number" then
		return { object = t }
	end
	return { position = t }
end

-- A journey's target: an object, a place, a cell alone, a known portal's destination, a recall's
-- landing.
local function journey_target(t)
	t = id_or(t)
	if type(t) == "number" then
		return { object = t }
	end
	if type(t) == "table" and (t.portal ~= nil or t.recall ~= nil) then
		return { portal = t.portal, recall = t.recall }
	end
	if type(t) == "table" and t.cell ~= nil and t.x == nil and t.ns == nil then
		return { cell = t.cell }
	end
	return { position = t }
end

api.combat = {
	-- "peace", or "combat": the mode the held equipment calls for.
	set_mode = function(mode, opts)
		return request("combat.set_mode", { mode = mode, purpose = opts and opts.purpose })
	end,
	-- `combat.attack(target, { height = "medium", power = 0.5 })`: one swing (or shot) at target.
	attack = function(target, opts)
		opts = opts or {}
		return request("combat.attack", { target = id_or(target), height = opts.height, power = opts.power })
	end,
	-- Stop attacking: the attack being charged or repeated is cancelled, and this plugin's
	-- attacks queued or in flight fail as "stopped".
	stop = function()
		return request("combat.stop")
	end,
	-- Whether the character is in a combat mode.
	fighting = function()
		return hostcall("player.get", "combat_mode") ~= 1
	end,
}

api.nav = {
	move_to = function(target, range)
		local args = target_of(target)
		args.range = range
		return request("nav.move_to", args)
	end,
	follow = function(object, range)
		return request("nav.follow", { object = id_or(object), range = range })
	end,
	face = function(heading)
		return request("nav.face", { heading = heading })
	end,
	-- `nav.jump{ ms, heading, walk, direction }`: face `heading` (when given), charge the jump for
	-- `ms` (0 to 1000; the power bar is full at 1000) holding the walk key when `walk` and the
	-- `direction` it leaps in (`forward`, `backward`, `strafe_left`, `strafe_right`), then leap.
	jump = function(opts)
		opts = opts or {}
		return request("nav.jump", {
			ms = opts.ms or 0,
			heading = opts.heading,
			walk = opts.walk and true or false,
			direction = opts.direction,
		})
	end,
	-- `nav.motion(kind, ms, { walk })`: hold one movement control (`forward`, `backward`,
	-- `strafe_left`, `strafe_right`, `turn_left`, `turn_right`) for `ms`, then let go.
	motion = function(kind, ms, opts)
		return request("nav.motion", {
			kind = kind,
			ms = ms,
			walk = opts ~= nil and opts.walk and true or false,
		})
	end,
	autorun = function(on)
		return request("nav.autorun", { on = on and true or false })
	end,
	stop = function()
		hostcall("act.request", "nav.stop", {})
	end,
	-- The id of the plugin moving the character, or nil.
	owner = function()
		return hostcall("nav.owner")
	end,
	-- `nav.path_to(target, opts)`: a journey there, by walking, portals and recalls, as one
	-- promise. `target` is an object, a place, `{ cell = 0x… }`, `{ portal = "name" }` (where a
	-- known portal sends) or `{ recall = "lifestone" | "marketplace" | spell }` (where a recall
	-- landed). `opts`: `portals`, `recalls` (false leaves them out), `show` (false: the host does
	-- not draw it), and `on_planned`, `on_leg`, `on_learned`, called with this journey's events.
	path_to = function(target, opts)
		opts = opts or {}
		local p = request("nav.path_to", {
			target = journey_target(target),
			opts = { portals = opts.portals, recalls = opts.recalls, show = opts.show },
		})
		local heard = {}
		for name, fn in pairs({
			["nav.journey_planned"] = opts.on_planned,
			["nav.journey_leg"] = opts.on_leg,
			["nav.journey_learned"] = opts.on_learned,
		}) do
			local h = function(e)
				if e.id == p.id then
					fn(e)
				end
			end
			api.on(name, h)
			table.insert(heard, { name, h })
		end
		local function done()
			for _, h in ipairs(heard) do
				api.off(h[1], h[2])
			end
		end
		p:next(done, done)
		return p
	end,
	-- `nav.plan(target, opts)`: the journey `nav.path_to` would take, without moving: a promise of
	-- `{ legs, seconds }`.
	plan = function(target, opts)
		opts = opts or {}
		return request("nav.plan", {
			target = journey_target(target),
			opts = { portals = opts.portals, recalls = opts.recalls },
		})
	end,
	-- `nav.path(target)`: the walking route there as the client plans it, round walls and
	-- through doors, without moving: a promise of `{ points, doors, length, complete }`. `target`
	-- is an object, a place or `{ cell = 0x… }`.
	path = function(target)
		return request("nav.path", { target = journey_target(target) })
	end,
	-- `nav.sight(target [, from])`: whether a straight line from the character (or `from`) to an
	-- object or a place is clear, a metre above each end; nil where the host cannot tell.
	sight = function(target, from)
		return hostcall("nav.sight", id_or(target), from ~= nil and id_or(from) or nil)
	end,
	-- The journey this plugin has under way, or nil.
	journey = function()
		return hostcall("nav.journey")
	end,
	-- Show or hide the host's drawing of this plugin's journey.
	show_journey = function(on)
		return hostcall("nav.show_journey", on and true or false)
	end,
	-- The portals this world's store knows.
	known_portals = function()
		return hostcall("nav.known_portals")
	end,
	-- Where the character's recalls have sent it.
	recalls = function()
		return hostcall("nav.recalls")
	end,
}

api.session = {
	info = function()
		return hostcall("session.info")
	end,
	select_character = function(character)
		return request("session.select_character", { character = character })
	end,
	logout = function()
		return request("session.logout")
	end,
	quit = function()
		return request("session.quit")
	end,
}

api.peers = {
	send = function(channel, data)
		hostcall("peers.send", tostring(channel), data)
	end,
}

api.net = {
	-- `net.fetch(url)` or `net.fetch{ url = …, method = …, headers = { … }, body = …, as = "image" }`
	fetch = function(req)
		if type(req) == "string" then
			req = { url = req }
		end
		return request("net.fetch", req)
	end,
}

-- Regular expressions in the dialect VTank metas are written in: `regex.match(pattern, text,
-- ignore_case)` answers { text, groups = { … }, named = { … } } for the first match, or nil.
api.regex = {
	match = function(pattern, text, ignore_case)
		return hostcall("text.match", tostring(pattern), tostring(text), ignore_case and true or false)
	end,
}

api.policy = {
	info = function()
		return hostcall("policy.info")
	end,
}

-- Markers ------------------------------------------------------------------------------------------

local marker_meta = {
	__index = {
		remove = function(self)
			return hostcall("overlay.remove", self.id)
		end,
	},
}

local function marker_fields(t)
	local out = {}
	for k, v in pairs(t or {}) do
		out[k] = v
	end
	if out.object ~= nil then
		out.object = id_or(out.object)
	end
	if out.to_object ~= nil then
		out.to_object = id_or(out.to_object)
	end
	return out
end

local function marker_kind(kind)
	return {
		add = function(_, t)
			local id = hostcall("overlay.add", kind, marker_fields(t))
			return setmetatable({ id = id }, marker_meta)
		end,
		clear = function(_)
			hostcall("overlay.clear", kind)
		end,
	}
end

api.markers = {
	world = marker_kind("world"),
	radar = marker_kind("radar"),
	map = marker_kind("map"),
	-- One arrow per plugin: `markers.arrow:set{ object = … }` or `{ position = … }`.
	arrow = {
		set = function(_, t)
			local id = hostcall("overlay.add", "arrow", marker_fields(t))
			return setmetatable({ id = id }, marker_meta)
		end,
		clear = function(_)
			hostcall("overlay.clear", "arrow")
		end,
	},
	clear = function()
		hostcall("overlay.clear")
	end,
}

-- World drawing: lines, rings, squares and floating text in the 3-D view (`overlay.world`).
-- `draw.line{ object|position, to_object|to_position, color, width, height }`,
-- `draw.ring{ object|position, radius, … }`, `draw.square{ object|position, radius, … }`,
-- `draw.text{ object|position, label, height, color }`; each answers a handle with `:remove()`.
-- What stands in front of a shape hides it unless it says `on_top = true`.
local function shape(name)
	return function(t)
		local f = marker_fields(t)
		f.shape = name
		local id = hostcall("overlay.add", "shape", f)
		return setmetatable({ id = id }, marker_meta)
	end
end
api.draw = {
	line = shape("line"),
	ring = shape("ring"),
	square = shape("square"),
	text = shape("text"),
	clear = function()
		hostcall("overlay.clear", "shape")
	end,
	-- The nametags layer: a label over every player, monster, NPC or portal (as named) within
	-- `within` metres, the nearest `limit` of them, with its health when `health` is true. One a
	-- plugin; nil turns it off.
	nametags = function(t)
		hostcall("overlay.clear", "nametags")
		if t == nil then
			return nil
		end
		local id = hostcall("overlay.add", "nametags", marker_fields(t))
		return setmetatable({ id = id }, marker_meta)
	end,
}

-- Game state -------------------------------------------------------------------------------------

local object_methods = {
	distance = function(self)
		return hostcall("object.distance", rawget(self, "id"))
	end,
	int = function(self, key)
		return hostcall("object.int", rawget(self, "id"), key)
	end,
	float = function(self, key)
		return hostcall("object.float", rawget(self, "id"), key)
	end,
	str = function(self, key)
		return hostcall("object.string", rawget(self, "id"), key)
	end,
	bool = function(self, key)
		return hostcall("object.bool", rawget(self, "id"), key)
	end,
	int64 = function(self, key)
		return hostcall("object.int64", rawget(self, "id"), key)
	end,
	did = function(self, key)
		return hostcall("object.did", rawget(self, "id"), key)
	end,
	iid = function(self, key)
		return hostcall("object.iid", rawget(self, "id"), key)
	end,
	-- What the client knows of its health: { current, max, fraction }, { fraction } for the
	-- selected object's bar alone, or nil.
	health = function(self)
		return hostcall("object.health", rawget(self, "id"))
	end,
	-- Any property by its type's name: "int", "int64", "float", "string", "bool", "did", "iid".
	prop = function(self, ty, key)
		local op = ({ int = "object.int", int64 = "object.int64", float = "object.float",
			string = "object.string", bool = "object.bool", did = "object.did", iid = "object.iid" })[ty]
		assert(op ~= nil, "prop(type, key): type is int, int64, float, string, bool, did or iid")
		return hostcall(op, rawget(self, "id"), key)
	end,
	-- The last appraisal, or nil: { success, ints, int64s, floats, strings, bools, spells }.
	appraisal = function(self)
		return appraisal_value(hostcall("object.appraisal", rawget(self, "id")))
	end,
	-- Every property the client knows of it, by number: { ints, int64s, floats, strings, bools,
	-- dids, iids }; with `ty` ("int", "string", ...), that type's table alone; nil once it is gone.
	properties = function(self, ty)
		local t = hostcall("object.properties", rawget(self, "id"), ty)
		if t == nil then
			return nil
		end
		if ty ~= nil then
			return numbered(t)
		end
		local out = {}
		for k, v in pairs(t) do
			out[k] = numbered(v)
		end
		return out
	end,
}

local object_meta = {}
object_meta.__index = function(obj, key)
	local m = object_methods[key]
	if m ~= nil then
		return m
	end
	return hostcall("object.get", rawget(obj, "id"), key)
end
object_meta.__newindex = function()
	error("an object handle is read-only", 2)
end
object_meta.__tostring = function(obj)
	return string.format("object 0x%08X", rawget(obj, "id"))
end

local function object(id)
	if id == nil then
		return nil
	end
	return setmetatable({ id = id }, object_meta)
end

-- What `game.player` has answered in this entry: the game is the frame's for the whole entry, so
-- a field read again is not asked of the host again. The host empties it as each entry begins.
local player_seen = {}
local NOTHING = {}
-- Entries into the VM so far: what a library reads of the game can be kept for one entry.
local entries = 0
local function new_entry()
	entries = entries + 1
	table.clear(player_seen)
end
local function entry()
	return entries
end

api.game = {
	player = setmetatable({}, {
		__index = function(_, key)
			local v = player_seen[key]
			if v == nil then
				v = hostcall("player.get", key)
				player_seen[key] = v == nil and NOTHING or v
				return v
			end
			if v == NOTHING then
				return nil
			end
			return v
		end,
		__newindex = function()
			error("game.player is read-only", 2)
		end,
	}),
	world = {
		get = function(_, id)
			if hostcall("object.exists", id) then
				return object(id)
			end
			return nil
		end,
		-- `find(filter, pred)`: the objects the filter matches (see `api.md`), kept only where
		-- `pred(object)` is true when a predicate is given.
		find = function(_, filter, pred)
			if type(filter) == "function" then
				filter, pred = nil, filter
			end
			local found = {}
			for _, id in ipairs(hostcall("world.find", filter or {})) do
				local o = object(id)
				if pred == nil or pred(o) then
					table.insert(found, o)
				end
			end
			return found
		end,
		-- The first object `find` answers, or nil.
		first = function(self, filter, pred)
			return self:find(filter, pred)[1]
		end,
	},
	selection = function()
		return object(hostcall("world.selection"))
	end,
	-- The character's own integer property `key` (a game property number), or nil.
	player_int = function(key)
		return hostcall("player.int", key)
	end,
	inventory = setmetatable({
		-- Every item the player holds, as handles.
		items = function()
			local found = {}
			for _, id in ipairs(hostcall("inventory.items")) do
				table.insert(found, object(id))
			end
			return found
		end,
		-- The packs, the main pack first and then the side packs in the order it shows them:
		-- `{ id, name, capacity, used }` each.
		packs = function()
			return hostcall("inventory.packs") or {}
		end,
	}, {
		__index = function(_, key)
			if key == "open_container" then
				return hostcall("inventory.open_container")
			elseif key == "free_slots" then
				return hostcall("inventory.free_slots")
			end
			return nil
		end,
	}),
}

-- The game's own data, read only (`game.data`): a landblock's terrain and cells, a dungeon cell's
-- place, portals and floor, for maps.
api.game.data = {
	landblock = function(id)
		return hostcall("data.landblock", id)
	end,
	cell = function(id)
		return hostcall("data.cell", id)
	end,
}

-- A file the player picks for the plugin, each time in a dialog the host owns (`files.import`):
-- `files.import{ prompt = "…", extensions = { "met" } }` is a promise of `{ name, text }`, failed
-- `refused` when the player declines.
api.files.import = function(t)
	return request("files.import", t or {})
end

-- The game's own panels (`retail_ui`): hide, show or move one; `reset` gives it back as the
-- player had it.
api.retail_ui = {
	panels = { "chat", "vitals", "side_vitals", "toolbar", "radar", "panels", "environment", "combat",
		"power_bar", "examination" },
	hide = function(panel)
		hostcall("retail_ui.set", panel, { hidden = true })
	end,
	show = function(panel)
		hostcall("retail_ui.set", panel, { hidden = false })
	end,
	move = function(panel, x, y)
		hostcall("retail_ui.set", panel, { x = x, y = y })
	end,
	set = function(panel, t)
		hostcall("retail_ui.set", panel, t)
	end,
	reset = function(panel)
		hostcall("retail_ui.reset", panel)
	end,
}

-- Declared UI ------------------------------------------------------------------------------------

local function declare(kind)
	return function(t)
		t = t or {}
		local node = { kind = kind, children = {} }
		for _, child in ipairs(t) do
			table.insert(node.children, child)
		end
		for k, v in pairs(t) do
			if type(k) ~= "number" then
				node[k] = v
			end
		end
		return node
	end
end

local node_meta = {}
node_meta.__index = function(h, key)
	return hostcall("ui.get", rawget(h, "window"), rawget(h, "node"), key)
end
node_meta.__newindex = function(h, key, value)
	hostcall("ui.set", rawget(h, "window"), rawget(h, "node"), key, value)
end

local function node_handle(window, id)
	return setmetatable({ window = window, node = id }, node_meta)
end

local window_methods = {}
function window_methods:get(id)
	return node_handle(self.id, id)
end
function window_methods:close()
	hostcall("ui.close", self.id)
end
function window_methods:show()
	hostcall("ui.show", self.id)
end
local window_meta = { __index = window_methods }

api.ui = {}
for _, kind in ipairs({ "row", "column", "group", "separator", "label", "button", "toggle", "meter", "list", "image", "tabs", "number", "choice", "canvas", "chart", "input", "textbox", "slider", "hotkey" }) do
	api.ui[kind] = declare(kind)
end
api.ui.window = function(t)
	local node = declare("window")(t)
	assert(type(node.id) == "string", "ui.window{...}: a window needs an id")
	hostcall("ui.window", node)
	return setmetatable({ id = node.id }, window_meta)
end
-- A line shown briefly at the top of the screen, with the plugin's name.
api.ui.toast = function(text, seconds)
	return hostcall("ui.toast", text, seconds)
end
-- What the plugin bar shows the plugin doing: "running", "paused" or "idle".
api.ui.status = function(s)
	hostcall("ui.status", s)
end

-- An always-on-screen HUD: a window with no frame, placed by `anchor` (`top_left` … `bottom_right`).
api.ui.hud = function(t)
	local node = declare("hud")(t)
	assert(type(node.id) == "string", "ui.hud{...}: a HUD needs an id")
	hostcall("ui.hud", node)
	return setmetatable({ id = node.id }, window_meta)
end
-- `ui.get("window.node")`: a node of one of the plugin's windows.
api.ui.get = function(path)
	local window, id = string.match(path, "^([^.]+)%.(.+)$")
	assert(window ~= nil, "ui.get(path): path is window.node")
	return node_handle(window, id)
end

-- The message stream: each message the client received or sent, decoded, read only (needs the
-- `messages` permission and a host with the `messages` capability). `messages.on(name, fn)` hears
-- one message by the name the opcode table gives it (`Item_SetAppraiseInfo`), by its type
-- (`0x02CD`), or every one (`any`); the handler gets `{ name, opcode, dir, recipient, codec,
-- fields }`. There is no way to send one.
api.messages = setmetatable({
	on = function(name, fn)
		assert(type(name) == "string", "messages.on(name, fn): name must be a string")
		return api.on("messages." .. name, fn)
	end,
	off = function(name, fn)
		return api.off("messages." .. name, fn)
	end,
}, {
	-- `messages.version`: the stream's own version, which follows the decoders.
	__index = function(_, key)
		if key == "version" then
			return hostcall("host.info").messages
		end
		return nil
	end,
})

-- Services: what this plugin offers the others in this client, and calls to theirs.
-- `services.offer(name, fn [, { allow = { plugin ids } }])`: `fn(args, call)` answers each call
-- (`call.from` the calling plugin, `call.gesture` whether it followed the player's gesture); what
-- it returns is the answer, and an error it raises fails the call.
-- `services.call(plugin, name, args)`: a promise of the answer (needs `plugins.call`).
api.services = {
	offer = function(name, fn, opts)
		assert(type(fn) == "function", "services.offer(name, fn): fn must be a function")
		hostcall("services.offer", name, opts and opts.allow or nil)
		services[name] = fn
	end,
	withdraw = function(name)
		services[name] = nil
		hostcall("services.withdraw", name)
	end,
	call = function(plugin, name, args)
		return request("services.call", { plugin = plugin, service = name, args = args })
	end,
	list = function()
		return hostcall("services.list")
	end,
}

-- The host ---------------------------------------------------------------------------------------

api.permissions = {
	has = function(name)
		return hostcall("permissions.has", name)
	end,
	-- Seconds left of the activation the player's last gesture gave (0 when none is live).
	activation = function()
		return hostcall("permissions.activation")
	end,
}

api.host = setmetatable({
	has = function(capability)
		return hostcall("host.has", capability)
	end,
}, {
	-- `api`, `name` and `version`, read from the host each time.
	__index = function(_, key)
		return hostcall("host.info")[key]
	end,
})

-- `os` keeps what a sandbox may have, on the host's clock.
local os_date, os_difftime = os.date, os.difftime
api.os = {
	clock = function()
		return hostcall("time.clock")
	end,
	time = function(t)
		if t ~= nil then
			error("os.time(table) is not available to plugins", 2)
		end
		return hostcall("time.unix")
	end,
	date = function(fmt, t)
		return os_date(fmt, t or hostcall("time.unix"))
	end,
	difftime = os_difftime,
}

-- The host's own libraries, which `require` finds before the plugin's package.
local builtin = {}

-- `dereth.loot`: loot profiles (`.utl`) read and matched by the host.
builtin["dereth.loot"] = function()
	local profile_methods = {}
	local function id_of(item)
		if type(item) == "table" then
			return item.id
		end
		return item
	end
	-- The rule `item` meets first, as { name, action, keep, count }, or nil.
	function profile_methods:classify(item)
		return hostcall("loot.classify", self.handle, id_of(item))
	end
	profile_methods.match = profile_methods.classify
	-- Whether the profile needs `item` appraised before it can decide it.
	function profile_methods:needs_appraisal(item)
		return hostcall("loot.needs_appraisal", self.handle, id_of(item))
	end
	function profile_methods:rules()
		return hostcall("loot.rules", self.handle)
	end
	-- The profile's salvage combining: { default, materials } of workmanship ranges, or nil.
	function profile_methods:salvage_combine()
		return hostcall("loot.salvage_combine", self.handle)
	end
	local profile_meta = { __index = profile_methods }
	return {
		load = function(text)
			assert(type(text) == "string", "loot.load(text): text must be the profile's contents")
			return setmetatable({ handle = hostcall("loot.load", text) }, profile_meta)
		end,
	}
end

-- `dereth.nav`: routes of waypoints, walked one `nav.move_to` at a time; VTank `.nav` files read.
builtin["dereth.nav"] = function()
	local route_methods = {}
	local route_meta = { __index = route_methods }

	-- Walk the route inside `task.spawn`: each point in turn, pauses slept, chat lines sent, jumps
	-- jumped. A circular route starts over, a linear one turns back, a once route ends.
	-- `opts.range` is how close counts as arrived (3 m); `opts.on_point(i, point)` hears each
	-- point reached. Answers when a once route ends, or raises the movement's failure.
	function route_methods:walk(opts)
		opts = opts or {}
		local range = opts.range or 3
		-- A follow route follows its player: by id while the client knows it, else the nearest
		-- of its name, held within range; lost, it is looked for again a second later.
		if self.kind == "follow" then
			while true do
				local o = self.target_id and api.game.world:get(self.target_id) or nil
				if o == nil or (self.target ~= nil and o.name ~= self.target) then
					o = self.target and api.game.world:first({ name = self.target, sort = "distance" }) or nil
				end
				if o ~= nil then
					api.pawait(api.nav.follow(o.id, range))
				end
				api.task.sleep(1000)
			end
		end
		local points = self.points
		local step = 1
		local i = self.at or 1
		while true do
			if #points == 0 then
				return
			end
			local p = points[i]
			self.at = i
			if p.kind == "pause" then
				api.task.sleep(p.ms or 0)
			elseif p.kind == "chat" then
				api.await(api.nav.move_to(p.position, range))
				api.chat.command(p.text)
			elseif p.kind == "recall" then
				api.await(api.magic.cast(p.spell))
			elseif p.kind == "jump" then
				api.await(api.nav.move_to(p.position, range))
				api.await(api.nav.jump({ heading = p.heading, walk = p.shift, ms = p.ms }))
			else
				api.await(api.nav.move_to(p.position, range))
			end
			if opts.on_point then
				opts.on_point(i, p)
			end
			local nexti = i + step
			if nexti > #points or nexti < 1 then
				if self.kind == "circular" then
					nexti = 1
				elseif self.kind == "linear" then
					step = -step
					nexti = i + step
					if nexti < 1 or nexti > #points then
						return
					end
				else
					self.at = nil
					return
				end
			end
			i = nexti
		end
	end

	-- The index of the point nearest the character (by position), to start a walk there.
	function route_methods:nearest()
		local best, best_d = nil, nil
		local me = api.game.player.position
		if me == nil then
			return nil
		end
		for i, p in ipairs(self.points or {}) do
			local at = p.position
			if at ~= nil and at.ns ~= nil and me.ns ~= nil then
				local d = (at.ns - me.ns) ^ 2 + (at.ew - me.ew) ^ 2
				if best_d == nil or d < best_d then
					best, best_d = i, d
				end
			end
		end
		return best
	end

	return {
		-- A route from a VTank `.nav` file's text.
		load = function(text)
			assert(type(text) == "string", "nav.load(text): text must be the route's contents")
			return setmetatable(hostcall("nav.parse", text), route_meta)
		end,
		-- A route from places: `nav.route({ place, … }, "circular" | "linear" | "once")`.
		route = function(places, kind)
			local points = {}
			for _, p in ipairs(places) do
				table.insert(points, { kind = "point", position = p })
			end
			return setmetatable({ kind = kind or "once", points = points }, route_meta)
		end,
	}
end

-- `dereth.profiles`: VTank settings profiles (`.usd`) read by the host.
builtin["dereth.profiles"] = function()
	return {
		-- `{ <table name> = { row, … }, … }`, each row a map of its columns.
		usd = function(text)
			assert(type(text) == "string", "profiles.usd(text): text must be the profile's contents")
			return hostcall("profile.usd", text)
		end,
	}
end

-- `dereth.meta`: state machines of condition -> action rules, in the manner of VTank's metas.
builtin["dereth.meta"] = function()
	local M = {}
	local meta_methods = {}
	local meta_meta = { __index = meta_methods }
	local X = api.require("dereth.expr")

	local function count_items(name)
		local n = 0
		for _, it in ipairs(api.game.inventory.items()) do
			if it.name == name then
				n = n + (it.stack or 1)
			end
		end
		return n
	end

	local function vital_fraction(v)
		local x = api.game.player[v]
		if x == nil or x.max == nil or x.max == 0 then
			return nil
		end
		return x.current / x.max
	end

	local function time_left(spell)
		for _, e in ipairs(api.game.player.enchantments or {}) do
			if e.spell == spell then
				return e.remaining or math.huge
			end
		end
		return nil
	end

	-- Conditions: each a function(ctx) answering true or false.
	function M.always()
		return function()
			return true
		end
	end
	function M.never()
		return function()
			return false
		end
	end
	function M.all(...)
		local cs = table.pack(...)
		return function(ctx)
			for i = 1, cs.n do
				if not cs[i](ctx) then
					return false
				end
			end
			return true
		end
	end
	function M.any(...)
		local cs = table.pack(...)
		return function(ctx)
			for i = 1, cs.n do
				if cs[i](ctx) then
					return true
				end
			end
			return false
		end
	end
	function M.no(c)
		return function(ctx)
			return not c(ctx)
		end
	end
	-- A chat line since the last check matching a Lua pattern; its captures are kept in
	-- ctx.captures for the rule's action.
	function M.chat_match(pattern)
		return function(ctx)
			for _, line in ipairs(ctx.chat) do
				local caps = table.pack(string.match(line, pattern))
				if caps[1] ~= nil then
					ctx.captures = caps
					return true
				end
			end
			return false
		end
	end
	-- A chat line since the last check matching a regular expression (see `regex.match`).
	function M.chat_regex(pattern)
		return function(ctx)
			for _, line in ipairs(ctx.chat) do
				local m = api.regex.match(pattern, line)
				if m ~= nil then
					ctx.captures = m.groups
					return true
				end
			end
			return false
		end
	end
	-- As `chat_regex`, and each named group `name` is kept in the meta's variable
	-- `capturegroup_name` for expressions to read. With `colors` (a list of chat line types, or
	-- their numbers in text separated by semicolons, as VTank's color string), only lines of those
	-- types match; the type of the line matched is kept in `capturecolor`.
	function M.chat_capture(pattern, colors)
		local only = nil
		if type(colors) == "string" then
			for n in string.gmatch(colors, "%d+") do
				only = only or {}
				only[tonumber(n)] = true
			end
		elseif type(colors) == "table" and #colors > 0 then
			only = {}
			for _, n in ipairs(colors) do
				only[n] = true
			end
		end
		return function(ctx)
			for i, line in ipairs(ctx.chat) do
				local ty = ctx.chat_types and ctx.chat_types[i]
				if only == nil or (ty ~= nil and only[ty]) then
					local m = api.regex.match(pattern, line)
					if m ~= nil then
						ctx.captures = m.groups
						for k, v in pairs(m.named) do
							ctx.meta.vars["capturegroup_" .. k] = v
						end
						ctx.meta.vars.capturecolor = ty
						return true
					end
				end
			end
			return false
		end
	end
	function M.pack_slots_le(n)
		return function()
			local free = api.game.inventory.free_slots
			return free ~= nil and free <= n
		end
	end
	function M.item_count_le(name, n)
		return function()
			return count_items(name) <= n
		end
	end
	function M.item_count_ge(name, n)
		return function()
			return count_items(name) >= n
		end
	end
	-- At least `n` monsters within `metres`; with `name`, only those whose name matches it (a
	-- regular expression).
	function M.monsters_within(n, metres, name)
		return function()
			local found = api.game.world:find({ monster = true, within = metres })
			if name == nil then
				return #found >= n
			end
			local k = 0
			for _, o in ipairs(found) do
				if api.regex.match(name, o.name or "", true) ~= nil then
					k = k + 1
				end
			end
			return k >= n
		end
	end
	function M.no_monsters_within(metres)
		return function()
			return #api.game.world:find({ monster = true, within = metres }) == 0
		end
	end
	function M.landblock(id)
		return function()
			local p = api.game.player.position
			return p ~= nil and math.floor(p.cell / 65536) == id
		end
	end
	function M.landcell(id)
		return function()
			local p = api.game.player.position
			return p ~= nil and p.cell == id
		end
	end
	function M.vendor_open()
		return function()
			return api.vendor.info() ~= nil
		end
	end
	function M.vendor_closed()
		return function()
			return api.vendor.info() == nil
		end
	end
	-- A vital (health, stamina, mana) at or below a fraction of its maximum.
	function M.vital_le(vital, fraction)
		return function()
			local f = vital_fraction(vital)
			return f ~= nil and f <= fraction
		end
	end
	-- A spell on the character with at most `seconds` left, or not on it at all.
	function M.spell_time_le(spell, seconds)
		return function()
			local left = time_left(spell)
			return left == nil or left <= seconds
		end
	end
	-- A spell on the character with at least `seconds` left.
	function M.spell_time_ge(spell, seconds)
		return function()
			local left = time_left(spell)
			return left ~= nil and left >= seconds
		end
	end
	-- The character's burden at or above `percent` of its capacity.
	function M.burden_ge(percent)
		return function()
			local b = api.game.player.burden
			return b ~= nil and b * 100 >= percent
		end
	end
	-- The meta has been in its state at least `seconds`, counted since it entered the state or was
	-- last started (`m:restart_clock()` too); `seconds_in_state_persist_ge` counts since it entered
	-- the state, however often it was stopped and started (VTank's "(P)" condition).
	function M.seconds_in_state_ge(seconds)
		return function(ctx)
			return ctx.state_seconds >= seconds
		end
	end
	function M.seconds_in_state_persist_ge(seconds)
		return function(ctx)
			return ctx.state_seconds_persist >= seconds
		end
	end
	function M.navigating()
		return function()
			return api.nav.owner() ~= nil
		end
	end
	-- No route is being walked by this meta (or, driving an engine, by the engine).
	function M.route_empty()
		return function(ctx)
			local e = ctx.meta.engine
			if e ~= nil and e.route_empty ~= nil then
				return e.route_empty() == true
			end
			return not ctx.meta.walking
		end
	end
	-- The character is at least `metres` from every point of the route this meta walks (true
	-- when it walks none).
	function M.route_distance_ge(metres)
		return function(ctx)
			local e = ctx.meta.engine
			if e ~= nil and e.route_distance ~= nil then
				local d = e.route_distance()
				return d == nil or d >= metres
			end
			local r = ctx.meta.route
			local me = api.game.player.position
			if r == nil or me == nil or me.ns == nil then
				return true
			end
			for _, pt in ipairs(r.points or {}) do
				local at = pt.position
				if at ~= nil and at.ns ~= nil then
					local d = math.sqrt(((at.ns - me.ns) * 240) ^ 2 + ((at.ew - me.ew) * 240) ^ 2)
					if d < metres then
						return false
					end
				end
			end
			return true
		end
	end
	-- The character entered, or left, portal space since the last check; or is in it now.
	function M.portal_entered()
		return function(ctx)
			return ctx.events.portal_enter
		end
	end
	function M.portal_exited()
		return function(ctx)
			return ctx.events.portal_exit
		end
	end
	function M.in_portal()
		return function()
			local s = api.session.info()
			return s ~= nil and s.portal_space == true
		end
	end
	-- The character died since the last check.
	function M.died()
		return function(ctx)
			return ctx.events.death
		end
	end
	-- At least `count` monsters within `metres` whose priority (as `priority(obj)` answers) is at
	-- least `min`.
	function M.monsters_with_priority(count, metres, min, priority)
		return function()
			local n = 0
			for _, o in ipairs(api.game.world:find({ monster = true, within = metres })) do
				if (priority(o) or 0) >= min then
					n = n + 1
				end
			end
			return n >= count
		end
	end

	-- The meta's `need_to_buff` function answers true (none set: never).
	function M.need_to_buff()
		return function(ctx)
			local f = ctx.meta.need_to_buff
			return f ~= nil and f(ctx) == true
		end
	end
	-- Any Lua function of the context, or an expression's text (true when its value is).
	function M.expr(fn)
		if type(fn) == "string" then
			local e = X.compile(fn)
			return function(ctx)
				return X.truthy(e(ctx.meta.env))
			end
		end
		return fn
	end

	-- Actions: each a function(ctx).
	function M.none()
		return function() end
	end
	function M.set_state(name)
		return function(ctx)
			ctx.meta:set_state(name)
		end
	end
	function M.call_state(name, back)
		return function(ctx)
			table.insert(ctx.meta.stack, back or ctx.meta.state)
			ctx.meta:set_state(name)
		end
	end
	function M.return_state()
		return function(ctx)
			local back = table.remove(ctx.meta.stack)
			if back ~= nil then
				ctx.meta:set_state(back)
			end
		end
	end
	-- Type a command (a line beginning with / or @), or say the line (`chat.send`). Driving an
	-- engine, a command the engine answers (VTank's own, say) goes to it instead.
	function M.chat(line)
		return function(ctx)
			local e = ctx and ctx.meta.engine
			if e ~= nil and e.command ~= nil and e.command(line) == true then
				return
			end
			if string.sub(line, 1, 1) == "/" or string.sub(line, 1, 1) == "@" then
				api.chat.command(line)
			else
				api.chat.say(line)
			end
		end
	end
	-- As `chat`, the line being an expression's value.
	function M.chat_expr(text)
		local e = X.compile(text)
		return function(ctx)
			M.chat(X.str(e(ctx.meta.env)))(ctx)
		end
	end
	-- Run an expression for what it does.
	function M.run(text)
		local e = X.compile(text)
		return function(ctx)
			e(ctx.meta.env)
		end
	end
	function M.print(text)
		return function()
			api.chat.print(text)
		end
	end
	-- Walk a route (from dereth.nav) until it ends or the state changes; driving an engine, hand
	-- the route to the engine, which walks it between its fights.
	function M.walk(route, opts)
		return function(ctx)
			local m = ctx.meta
			if m.engine ~= nil and m.engine.route ~= nil then
				m.route = route
				m.engine.route(route)
				return
			end
			if m.walking then
				return
			end
			m.walking = true
			m.route = route
			m.walk_task = api.task.spawn(function()
				local ok = pcall(route.walk, route, opts)
				if m.route == route then
					m.walking = false
				end
				if ok and route.kind == "once" then
					m.route = nil
				end
			end)
		end
	end
	function M.stop_nav()
		return function(ctx)
			if ctx.meta.engine ~= nil and ctx.meta.engine.route ~= nil then
				ctx.meta.engine.route(nil)
				ctx.meta.route = nil
				return
			end
			api.nav.stop()
			ctx.meta.walking = false
			ctx.meta.route = nil
		end
	end
	-- If the character has not moved `metres` within `seconds`, enter `state` (re-entering it
	-- when it is the current one). The watchdog lasts until cleared, fired, or the state changes.
	function M.watchdog_set(state, metres, seconds)
		return function(ctx)
			local w = ctx.meta.watchdog
			if w ~= nil and w.state == state and w.metres == metres and w.seconds == seconds then
				return
			end
			local p = api.game.player.position
			ctx.meta.watchdog = {
				state = state,
				metres = metres,
				seconds = seconds,
				at = p,
				since = api.os.clock(),
				owner = ctx.meta.state,
			}
		end
	end
	function M.watchdog_clear()
		return function(ctx)
			ctx.meta.watchdog = nil
		end
	end
	-- The meta's options (VTank's settings, by name), which a plugin reads to steer itself.
	function M.set_option(name, value)
		return function(ctx)
			ctx.meta:set_option(name, value)
		end
	end
	function M.get_option(name, var)
		return function(ctx)
			local e = ctx.meta.engine
			if e ~= nil and e.option ~= nil then
				ctx.meta.vars[var] = e.option(name)
				return
			end
			ctx.meta.vars[var] = ctx.meta.options[string.lower(name)]
		end
	end
	-- A meta view: VTank's view markup shown as a plugin window (see `meta.view`).
	function M.create_view(name, xml)
		return function(ctx)
			ctx.meta:create_view(name, xml)
		end
	end
	function M.destroy_view(name)
		return function(ctx)
			ctx.meta:destroy_view(name)
		end
	end
	function M.destroy_all_views()
		return function(ctx)
			for name in pairs(ctx.meta.views) do
				ctx.meta:destroy_view(name)
			end
		end
	end
	function M.multiple(...)
		local as = table.pack(...)
		return function(ctx)
			for i = 1, as.n do
				as[i](ctx)
			end
		end
	end
	function M.lua(fn)
		return fn
	end

	-- A meta: `meta.new{ states = { Default = { { cond, action }, ... } }, start = "Default",
	-- every = 293, once = false }`. Each check runs, in order, the action of every rule of the
	-- current state whose condition holds; with `once = true` (VTank's rule) each rule runs at most
	-- once per stay in its state, and setting a meta's state to the state it is in enters it again.
	-- A rule whose action changes or re-enters the state ends the check.
	function M.new(t)
		assert(type(t) == "table" and type(t.states) == "table", "meta.new{ states = { … } }")
		local m = setmetatable({
			states = t.states,
			state = t.start or "Default",
			every = t.every or 293,
			once = t.once == true,
			ran = {},
			entries = 0,
			stops = 0,
			stack = {},
			since = 0,
			since_persist = api.os.clock(),
			chat = {},
			chat_types = {},
			events = {},
			listeners = {},
			option_listeners = {},
			options = {},
			vars = {},
			views = {},
			need_to_buff = t.need_to_buff,
			engine = t.engine,
		}, meta_meta)
		assert(m.states[m.state] ~= nil, "meta.new: no state " .. tostring(m.state))
		local function kept(scope)
			local key = "dereth.meta." .. scope
			return setmetatable({}, {
				__index = function(_, k)
					local ok, s = pcall(function()
						return (scope == "pvars" and api.store.character or api.store.global)[key]
					end)
					return ok and s ~= nil and s[k] or nil
				end,
				__clear = function()
					pcall(function()
						local st = scope == "pvars" and api.store.character or api.store.global
						st[key] = nil
					end)
				end,
				__newindex = function(_, k, v)
					pcall(function()
						local st = scope == "pvars" and api.store.character or api.store.global
						local s = st[key] or {}
						if type(v) == "table" and getmetatable(v) ~= nil then
							v = X.str(v)
						end
						s[k] = v
						st[key] = s
					end)
				end,
			})
		end
		m.env = X.env({ vars = m.vars, pvars = kept("pvars"), gvars = kept("gvars") })
		m.env.meta = m
		return m
	end

	function meta_methods:set_state(name, again)
		assert(self.states[name] ~= nil, "meta: no state " .. tostring(name))
		if name ~= self.state or again or self.once then
			local old = self.state
			self.state = name
			self.entries = self.entries + 1
			self.ran = {}
			self.since = api.os.clock()
			self.since_persist = self.since
			self.watchdog = nil
			for _, fn in ipairs(self.listeners) do
				fn(name, old)
			end
		end
	end

	function meta_methods:on_state(fn)
		table.insert(self.listeners, fn)
	end

	function meta_methods:set_option(name, value)
		name = string.lower(name)
		self.options[name] = value
		for _, fn in ipairs(self.option_listeners) do
			fn(name, value)
		end
	end

	-- `m:on_option(function(name, value) … end)`: hear the meta setting an option (names lower
	-- case, values as the meta gave them).
	function meta_methods:on_option(fn)
		table.insert(self.option_listeners, fn)
	end

	-- The watchdog: moved far enough (it starts over from here) or out of time (it fires).
	function meta_methods:watch()
		local w = self.watchdog
		if w == nil then
			return
		end
		local p = api.game.player.position
		local moved = 0
		if p ~= nil and w.at ~= nil and p.ns ~= nil and w.at.ns ~= nil then
			moved = math.sqrt(((p.ns - w.at.ns) * 240) ^ 2 + ((p.ew - w.at.ew) * 240) ^ 2)
		end
		local now = api.os.clock()
		if moved >= w.metres then
			w.at, w.since = p, now
		elseif now - w.since >= w.seconds then
			self:set_state(w.state, true)
		end
	end

	-- One check (the timer calls it; a test may too, and a plugin driving an engine calls it as its
	-- macro decides what to do).
	function meta_methods:check()
		self:watch()
		local now = api.os.clock()
		local ctx = {
			meta = self,
			chat = self.chat,
			chat_types = self.chat_types,
			events = self.events,
			state_seconds = now - self.since,
			state_seconds_persist = now - self.since_persist,
		}
		self.chat = {}
		self.chat_types = {}
		self.events = {}
		local state, entries, stops = self.state, self.entries, self.stops
		for i, rule in ipairs(self.states[state]) do
			if not (self.once and self.ran[i]) then
				local ok, hit = pcall(rule[1], ctx)
				if not ok then
					api.print("meta: a condition failed in " .. state .. ": " .. tostring(hit))
					hit = false
				end
				if hit then
					if self.once then
						self.ran[i] = true
					end
					rule[2](ctx)
					-- A state changed, or the meta stopped (another taking its place), ends the check.
					if self.entries ~= entries or self.stops ~= stops then
						break
					end
				end
			end
		end
	end

	-- Whether the meta's timer runs (between `m:start()` and `m:stop()`).
	function meta_methods:running()
		return self.timer ~= nil
	end

	-- "Seconds in state" counts from now, as on a start.
	function meta_methods:restart_clock()
		self.since = api.os.clock()
	end

	function meta_methods:start()
		if self.timer ~= nil then
			return
		end
		self.since = api.os.clock()
		self.handlers = {
			["chat.line"] = function(e)
				if #self.chat < 256 then
					table.insert(self.chat, e.text)
					self.chat_types[#self.chat] = e.type
				end
			end,
			["session.portal_enter"] = function()
				self.events.portal_enter = true
			end,
			["session.portal_exit"] = function()
				self.events.portal_exit = true
			end,
			["character.death"] = function()
				self.events.death = true
			end,
			["ui.clicked"] = function(e)
				self:view_clicked(e)
			end,
		}
		for name, fn in pairs(self.handlers) do
			api.on(name, fn)
		end
		self.timer = api.task.every(self.every, function()
			self:check()
		end)
	end

	function meta_methods:stop()
		self.stops = self.stops + 1
		if self.timer ~= nil then
			api.task.cancel(self.timer)
			for name, fn in pairs(self.handlers) do
				api.off(name, fn)
			end
			self.timer = nil
			self.watchdog = nil
		end
	end

	-- Meta views ---------------------------------------------------------------------------------

	local function unescape(s)
		return (string.gsub(s, "&(%a+);", { quot = '"', amp = "&", lt = "<", gt = ">", apos = "'" }))
	end

	local function attributes(tag)
		local a = {}
		for k, v in string.gmatch(tag, '([%w_]+)%s*=%s*"([^"]*)"') do
			a[string.lower(k)] = unescape(v)
		end
		return a
	end

	-- A view's markup as { title, controls = { { type, name, text, top, left, … }, … } }: every
	-- control, in document order, whatever layout holds it.
	function M.parse_view(xml)
		local view = { controls = {} }
		local head = string.match(xml, "<view(.-)>")
		assert(head ~= nil, "a meta view begins with <view …>")
		local va = attributes(head)
		view.title = va.title or "View"
		for tag in string.gmatch(xml, "<control(.-)/?>") do
			local a = attributes(tag)
			a.type = string.lower(a.type or "")
			if a.type ~= "layout" and a.type ~= "" then
				a.top = tonumber(a.top) or 0
				a.left = tonumber(a.left) or 0
				table.insert(view.controls, a)
			end
		end
		return view
	end

	-- The UI tree for a parsed view: controls at the same height share a row, rows top to bottom,
	-- each row left to right. Buttons, check boxes and text become buttons, toggles and labels; a
	-- control of another kind shows as a label naming it.
	function M.view_tree(name, view)
		local sorted = table.clone(view.controls)
		table.sort(sorted, function(a, b)
			if a.top ~= b.top then
				return a.top < b.top
			end
			return a.left < b.left
		end)
		local rows, row, top = {}, nil, nil
		for i, c in ipairs(sorted) do
			local id = "c" .. i .. ":" .. (c.name or "")
			c.node = id
			local node
			if c.type == "button" then
				node = api.ui.button({ id = id, text = c.text or c.name or "" })
			elseif c.type == "checkbox" then
				node = api.ui.toggle({ id = id, text = c.text or c.name or "", value = c.checked == "true" })
			elseif c.type == "statictext" or c.type == "text" or c.type == "label" then
				node = api.ui.label({ id = id, text = c.text or "" })
			else
				node = api.ui.label({ id = id, text = "[" .. c.type .. "] " .. (c.text or c.name or "") })
			end
			if row == nil or math.abs(c.top - top) > 4 then
				row = { kind = "row", children = {} }
				top = c.top
				table.insert(rows, row)
			end
			table.insert(row.children, node)
		end
		local win = api.ui.column(rows)
		return { id = "view:" .. name, title = view.title, win }
	end

	function meta_methods:create_view(name, xml)
		local ok, view = pcall(M.parse_view, xml)
		if not ok then
			api.print("meta: the view " .. name .. " does not read: " .. tostring(view))
			return
		end
		local tree = M.view_tree(name, view)
		local by_node = {}
		for _, c in ipairs(view.controls) do
			by_node[c.node] = c
		end
		self.views[name] = { window = api.ui.window(tree), controls = by_node }
	end

	function meta_methods:destroy_view(name)
		local v = self.views[name]
		if v ~= nil then
			v.window:close()
			self.views[name] = nil
		end
	end

	-- A click on a view's control: its `setstate`, then its `actionexpr`.
	function meta_methods:view_clicked(e)
		local name = string.match(e.window or "", "^view:(.*)$")
		local v = name and self.views[name]
		local c = v and v.controls[e.id]
		if c == nil then
			return
		end
		if c.setstate ~= nil and c.setstate ~= "" and self.states[c.setstate] ~= nil then
			self:set_state(c.setstate)
		end
		if c.actionexpr ~= nil and c.actionexpr ~= "" then
			local ok, err = pcall(function()
				X.compile(c.actionexpr)(self.env)
			end)
			if not ok then
				api.print("meta: a view's action failed: " .. tostring(err))
			end
		end
	end

	-- VTank's `.met` files ---------------------------------------------------------------------------

	-- `meta.read_met(text)`: a VTank `.met` file's rules as data, `{ rules = { { state,
	-- condition = { kind, data }, action = { kind, data } }, … } }`, in file order.
	function M.read_met(text)
		assert(type(text) == "string", "meta.read_met(text): text must be the meta's contents")
		return hostcall("meta.parse", text)
	end

	-- The kinds as the file numbers them, and how each one's data is written: `none` (no data),
	-- `s` or `i` (a text or a whole number), `list` (nested kinds), `route` (an embedded route), or
	-- named fields, each `{ key, type }` (`s` text, `i` a whole number, `d` a number, `ba` a block
	-- of bytes).
	local CONDITION_KINDS = {
		never = { 0, "none" }, always = { 1, "none" }, all = { 2, "list" }, any = { 3, "list" },
		chat_match = { 4, "s" }, pack_slots_le = { 5, "i" }, seconds_in_state_ge = { 6, "i" },
		route_empty = { 7, "none" }, died = { 8, "none" }, vendor_open = { 9, "none" },
		vendor_closed = { 10, "none" }, item_count_le = { 11, { { "n", "s" }, { "c", "i" } } },
		item_count_ge = { 12, { { "n", "s" }, { "c", "i" } } },
		monster_count_within = { 13, { { "n", "s" }, { "c", "i" }, { "r", "d" } } },
		monster_priority_count_within = { 14, { { "p", "i" }, { "c", "i" }, { "r", "d" } } },
		need_to_buff = { 15, "none" }, no_monsters_within = { 16, { { "r", "d" } } }, landblock = { 17, "i" },
		landcell = { 18, "i" }, portal_entered = { 19, "none" }, portal_exited = { 20, "none" }, no = { 21, "list" },
		seconds_in_state_persist_ge = { 22, "i" }, spell_time_ge = { 23, { { "sid", "i" }, { "sec", "i" } } },
		burden_ge = { 24, "i" }, route_distance_ge = { 25, { { "dist", "d" } } }, expr = { 26, { { "e", "s" } } },
		chat_capture = { 28, { { "p", "s" }, { "c", "s" } } }, dialog_popup = { 27, "raw" },
	}
	local ACTION_KINDS = {
		none = { 0, "none" }, set_state = { 1, "s" }, chat = { 2, "s" }, multiple = { 3, "list" }, walk = { 4, "route" },
		call_state = { 5, { { "st", "s" }, { "ret", "s" } } }, return_state = { 6, "none" },
		expr = { 7, { { "e", "s" } } }, chat_expr = { 8, { { "e", "s" } } },
		watchdog_set = { 9, { { "s", "s" }, { "r", "d" }, { "t", "d" } } }, watchdog_clear = { 10, {} },
		get_option = { 11, { { "o", "s" }, { "v", "s" } } }, set_option = { 12, { { "o", "s" }, { "v", "s" } } },
		create_view = { 13, { { "n", "s" }, { "x", "ba" } } }, destroy_view = { 14, { { "n", "s" } } },
		destroy_all_views = { 15, {} },
	}
	M.CONDITION_KINDS, M.ACTION_KINDS = CONDITION_KINDS, ACTION_KINDS

	-- A number as the file writes it: a whole one bare, any other in its shortest form.
	local function number_text(v)
		v = tonumber(v) or 0
		if v == math.floor(v) and math.abs(v) < 2 ^ 53 then
			return string.format("%d", v)
		end
		for digits = 1, 17 do
			local s = string.format("%." .. digits .. "g", v)
			if tonumber(s) == v then
				return s
			end
		end
		return string.format("%.17g", v)
	end

	-- `meta.write_met(file)`: the text of a VTank `.met` file holding `file`'s rules (the shape
	-- `read_met` answers), lines ended `\r\n` as VTank writes them. An embedded route is written
	-- from its `text` (a `.nav` file's text) under its `name`.
	function M.write_met(file)
		assert(type(file) == "table" and type(file.rules) == "table", "meta.write_met{ rules = { … } }")
		local out = {}
		-- A line that runs straight into the next one (a view's markup, as VTank writes it).
		local joined = {}
		local function line(s)
			table.insert(out, tostring(s))
		end
		local function cell(ty, v)
			line(ty)
			if ty == "i" or ty == "d" then
				line(number_text(v))
			elseif ty == "b" then
				line(v and "True" or "False")
			elseif ty == "ba" then
				-- Counted as the file's own line ends, `\r\n`.
				local text = string.gsub((string.gsub(tostring(v or ""), "\r\n", "\n")), "\n", "\r\n")
				line(#text)
				line(text)
				joined[#out] = true
			else
				line(tostring(v or ""))
			end
		end
		local function table_head(names, rows)
			line("TABLE")
			line(#names)
			for _, n in ipairs(names) do
				line(n)
			end
			for _ in ipairs(names) do
				line("n")
			end
			line(rows)
		end
		-- Data of a kind known only by its number, written back by its own types: a number, a text, a
		-- truth, or named fields of them.
		local function raw(d)
			if type(d) == "number" then
				cell(d == math.floor(d) and "i" or "d", d)
			elseif type(d) == "boolean" then
				cell("b", d)
			elseif type(d) == "table" then
				local keys = {}
				for k in pairs(d) do
					table.insert(keys, tostring(k))
				end
				table.sort(keys)
				table_head({ "k", "v" }, #keys)
				for _, k in ipairs(keys) do
					cell("s", k)
					raw(d[k])
				end
			else
				cell("s", d)
			end
		end
		-- A kind's number and how its data is written; a kind read as `unknown:<n>` keeps its number.
		local function kind_of(kinds, kind)
			local k = kinds[kind]
			if k ~= nil then
				return k[1], k[2]
			end
			local n = tonumber(string.match(tostring(kind), "^unknown:(%-?%d+)$"))
			assert(n ~= nil, "meta.write_met: no kind " .. tostring(kind))
			return n, "raw"
		end
		local entry
		local function data(kinds, kind, d)
			local _, how = kind_of(kinds, kind)
			if how == "raw" then
				raw(d)
			elseif how == "none" then
				cell("i", 0)
			elseif how == "s" or how == "i" then
				cell(how, d)
			elseif how == "list" then
				local items = type(d) == "table" and d or {}
				table_head({ "K", "V" }, #items)
				for _, x in ipairs(items) do
					entry(kinds, x)
				end
			elseif how == "route" then
				assert(type(d) == "table" and type(d.text) == "string", "an embedded route is written from its text")
				local lines = {}
				local name = (d.name == nil or d.name == "") and "[None]" or tostring(d.name)
				table.insert(lines, name)
				local count = 0
				if type(d.route) == "table" then
					count = d.route.kind == "follow" and 1 or #(d.route.points or {})
				end
				table.insert(lines, tostring(count))
				for l in string.gmatch(string.gsub(d.text, "\r\n", "\n"), "([^\n]*)\n?") do
					table.insert(lines, l)
				end
				while lines[#lines] == "" do
					table.remove(lines)
				end
				local bytes = 0
				for _, l in ipairs(lines) do
					bytes = bytes + #l + 2
				end
				line("ba")
				line(bytes)
				for _, l in ipairs(lines) do
					line(l)
				end
			else
				local fields = type(d) == "table" and d or {}
				table_head({ "k", "v" }, #how)
				for _, f in ipairs(how) do
					cell("s", f[1])
					cell(f[2], fields[f[1]])
				end
			end
		end
		function entry(kinds, e)
			cell("i", (kind_of(kinds, e.kind)))
			data(kinds, e.kind, e.data)
		end
		for _, l in ipairs({ "1", "CondAct", "5", "CType", "AType", "CData", "AData", "State", "n", "n", "n", "n", "n" }) do
			line(l)
		end
		line(#file.rules)
		for _, r in ipairs(file.rules) do
			cell("i", (kind_of(CONDITION_KINDS, r.condition.kind)))
			cell("i", (kind_of(ACTION_KINDS, r.action.kind)))
			data(CONDITION_KINDS, r.condition.kind, r.condition.data)
			data(ACTION_KINDS, r.action.kind, r.action.data)
			cell("s", r.state)
		end
		local text = {}
		for i, l in ipairs(out) do
			table.insert(text, l)
			if not joined[i] then
				table.insert(text, "\r\n")
			end
		end
		return table.concat(text)
	end

	-- `meta.load_met(text or file, opts)`: a meta from a VTank `.met` file's text (or its rules as
	-- `read_met` answers them), checked by VTank's rule (each rule runs at most once per stay in its
	-- state). `opts` go to `meta.new` (`every`, `need_to_buff`, `engine`: the macro the meta drives,
	-- see `automation.md`); `opts.start` picks the first state (else `Default`, else the first
	-- rule's). What the import could not carry is listed in the meta's `unsupported`, each once.
	function M.load_met(text, opts)
		opts = opts or {}
		local navlib = api.require("dereth.nav")
		local file = type(text) == "table" and text or hostcall("meta.parse", text)
		local unsupported, seen = {}, {}
		local function missing(what)
			if not seen[what] then
				seen[what] = true
				table.insert(unsupported, what)
			end
		end
		local function field(d, ...)
			for _, k in ipairs({ ... }) do
				if type(d) == "table" and d[k] ~= nil then
					return d[k]
				end
			end
			return nil
		end
		local function compiled(kind, builder, arg)
			local ok, f = pcall(builder, arg)
			if ok then
				return f
			end
			missing(kind .. ": " .. tostring(f))
			return nil
		end
		local cond, act
		function cond(e)
			local k, d = e.kind, e.data
			if k == "never" then
				return M.never()
			elseif k == "always" then
				return M.always()
			elseif k == "all" or k == "any" then
				local parts = {}
				for _, x in ipairs(d or {}) do
					table.insert(parts, cond(x))
				end
				return (k == "all" and M.all or M.any)(table.unpack(parts))
			elseif k == "no" then
				return M.no(cond((d or {})[1] or { kind = "never" }))
			elseif k == "chat_match" then
				return M.chat_regex(tostring(d))
			elseif k == "chat_capture" then
				return M.chat_capture(tostring(field(d, "p") or ""), field(d, "c") and tostring(field(d, "c")) or nil)
			elseif k == "pack_slots_le" then
				return M.pack_slots_le(tonumber(d) or 0)
			elseif k == "seconds_in_state_ge" then
				return M.seconds_in_state_ge(tonumber(d) or 0)
			elseif k == "seconds_in_state_persist_ge" then
				return M.seconds_in_state_persist_ge(tonumber(d) or 0)
			elseif k == "route_empty" then
				return M.route_empty()
			elseif k == "died" then
				return M.died()
			elseif k == "vendor_open" then
				return M.vendor_open()
			elseif k == "vendor_closed" then
				return M.vendor_closed()
			elseif k == "item_count_le" then
				return M.item_count_le(field(d, "n") or "", tonumber(field(d, "c")) or 0)
			elseif k == "item_count_ge" then
				return M.item_count_ge(field(d, "n") or "", tonumber(field(d, "c")) or 0)
			elseif k == "monster_count_within" then
				return M.monsters_within(tonumber(field(d, "c")) or 1, tonumber(field(d, "r")) or 0, field(d, "n"))
			elseif k == "monster_priority_count_within" then
				if opts.engine == nil or opts.engine.priority == nil then
					missing("monster priorities (counted as any monster)")
					return M.monsters_within(tonumber(field(d, "c")) or 1, tonumber(field(d, "r")) or 0)
				end
				return M.monsters_with_priority(tonumber(field(d, "c")) or 1, tonumber(field(d, "r")) or 0,
					tonumber(field(d, "p")) or 0, opts.engine.priority)
			elseif k == "no_monsters_within" then
				return M.no_monsters_within(tonumber(field(d, "r")) or 0)
			elseif k == "need_to_buff" then
				return M.need_to_buff()
			elseif k == "landblock" then
				return M.landblock(math.floor(((tonumber(d) or 0) % 4294967296) / 65536))
			elseif k == "landcell" then
				return M.landcell((tonumber(d) or 0) % 4294967296)
			elseif k == "portal_entered" then
				return M.portal_entered()
			elseif k == "portal_exited" then
				return M.portal_exited()
			elseif k == "spell_time_ge" then
				return M.spell_time_ge(tonumber(field(d, "sid", "s")) or 0, tonumber(field(d, "sec", "t")) or 0)
			elseif k == "burden_ge" then
				return M.burden_ge(tonumber(d) or 0)
			elseif k == "route_distance_ge" then
				return M.route_distance_ge(tonumber(field(d, "dist", "d")) or tonumber(d) or 0)
			elseif k == "expr" then
				return compiled("expression", M.expr, tostring(field(d, "e") or "")) or M.never()
			end
			missing("condition " .. tostring(k))
			return M.never()
		end
		function act(e)
			local k, d = e.kind, e.data
			if k == "none" then
				return M.none()
			elseif k == "set_state" then
				return M.set_state(tostring(d))
			elseif k == "chat" then
				return M.chat(tostring(d))
			elseif k == "multiple" then
				local parts = {}
				for _, x in ipairs(d or {}) do
					table.insert(parts, act(x))
				end
				return M.multiple(table.unpack(parts))
			elseif k == "walk" then
				if type(d) == "table" and d.route ~= nil then
					local route = setmetatable(d.route, getmetatable(navlib.route({})))
					if route.kind ~= "follow" and #route.points == 0 then
						return M.stop_nav()
					end
					return M.walk(route, { range = opts.route_range or 3 })
				end
				missing("embedded route " .. tostring(type(d) == "table" and (d.error or d.name) or ""))
				return M.none()
			elseif k == "call_state" then
				return M.call_state(tostring(field(d, "st") or ""), field(d, "ret"))
			elseif k == "return_state" then
				return M.return_state()
			elseif k == "expr" then
				return compiled("expression", M.run, tostring(field(d, "e") or "")) or M.none()
			elseif k == "chat_expr" then
				return compiled("expression", M.chat_expr, tostring(field(d, "e") or "")) or M.none()
			elseif k == "watchdog_set" then
				return M.watchdog_set(tostring(field(d, "s") or ""), tonumber(field(d, "r")) or 0,
					tonumber(field(d, "t")) or 0)
			elseif k == "watchdog_clear" then
				return M.watchdog_clear()
			elseif k == "set_option" then
				return M.set_option(tostring(field(d, "o") or ""), field(d, "v"))
			elseif k == "get_option" then
				return M.get_option(tostring(field(d, "o") or ""), tostring(field(d, "v") or ""))
			elseif k == "create_view" then
				return M.create_view(tostring(field(d, "n") or "view"), tostring(field(d, "x") or ""))
			elseif k == "destroy_view" then
				return M.destroy_view(tostring(field(d, "n") or ""))
			elseif k == "destroy_all_views" then
				return M.destroy_all_views()
			end
			missing("action " .. tostring(k))
			return M.none()
		end
		local states, first_state = {}, nil
		for _, r in ipairs(file.rules) do
			local s = tostring(r.state)
			first_state = first_state or s
			states[s] = states[s] or {}
			table.insert(states[s], { cond(r.condition), act(r.action) })
		end
		-- A state named only by an action still exists, with no rules.
		local function touch(e)
			if e == nil then
				return
			end
			if e.kind == "set_state" or e.kind == "call_state" then
				local s = e.kind == "set_state" and tostring(e.data) or tostring(field(e.data, "st"))
				states[s] = states[s] or {}
				if e.kind == "call_state" and field(e.data, "ret") ~= nil then
					states[tostring(field(e.data, "ret"))] = states[tostring(field(e.data, "ret"))] or {}
				end
			elseif e.kind == "watchdog_set" then
				local s = tostring(field(e.data, "s") or "")
				states[s] = states[s] or {}
			elseif e.kind == "multiple" then
				for _, x in ipairs(e.data or {}) do
					touch(x)
				end
			end
		end
		for _, r in ipairs(file.rules) do
			touch(r.action)
		end
		local start = opts.start or (states.Default and "Default") or first_state or "Default"
		states[start] = states[start] or {}
		local m = M.new({
			states = states,
			start = start,
			every = opts.every,
			once = true,
			need_to_buff = opts.need_to_buff,
			engine = opts.engine,
		})
		m.unsupported = unsupported
		return m
	end

	return M
end

-- `dereth.expr`: expressions in the language VTank metas use (`getvar[x]==1`, `$n=$n+1`,
-- `chatbox[\/say hi]`), compiled once and evaluated against the game and a set of variables.
builtin["dereth.expr"] = function()
	local X = {}

	-- A parse error, without a place in this file.
	local function need(ok, msg)
		if not ok then
			error(msg, 0)
		end
		return ok
	end

	local list_meta = { __name = "list" }
	local coord_meta = { __name = "coordinates" }
	local dict_meta = { __name = "dictionary" }
	local watch_meta = { __name = "stopwatch" }
	X.dict_meta, X.watch_meta = dict_meta, watch_meta
	X.control_meta = { __name = "control" }

	local function is_object(v)
		return type(v) == "table" and getmetatable(v) ~= nil and getmetatable(v) ~= list_meta
			and getmetatable(v) ~= coord_meta and rawget(v, "id") ~= nil
	end

	local function truthy(v)
		if v == nil or v == false then
			return false
		elseif type(v) == "number" then
			return v ~= 0
		elseif type(v) == "string" then
			local l = string.lower(v)
			return v ~= "" and l ~= "false" and v ~= "0"
		end
		return true
	end
	X.truthy = truthy

	local function num(v)
		if v == true then
			return 1
		elseif v == false or v == nil then
			return 0
		elseif type(v) == "number" then
			return v
		elseif type(v) == "string" then
			return tonumber(v)
		end
		return nil
	end

	local function str(v)
		if v == nil then
			return ""
		elseif v == true then
			return "True"
		elseif v == false then
			return "False"
		elseif type(v) == "number" then
			if v == math.floor(v) and math.abs(v) < 2 ^ 53 then
				return string.format("%d", v)
			end
			return tostring(v)
		elseif is_object(v) then
			return tostring(rawget(v, "id"))
		elseif getmetatable(v) == coord_meta then
			return string.format("%.3f%s, %.3f%s", math.abs(v.ns), v.ns >= 0 and "N" or "S", math.abs(v.ew),
				v.ew >= 0 and "E" or "W")
		elseif getmetatable(v) == list_meta then
			local parts = {}
			for i, x in ipairs(v) do
				parts[i] = str(x)
			end
			return "[" .. table.concat(parts, ",") .. "]"
		elseif getmetatable(v) == dict_meta then
			local parts = {}
			for i, k in ipairs(v.keys) do
				parts[i] = k .. ":" .. str(v.values[k])
			end
			return "{" .. table.concat(parts, ",") .. "}"
		elseif getmetatable(v) == watch_meta then
			return tostring(v.elapsed)
		end
		return tostring(v)
	end
	X.str = str

	local function equal(a, b)
		if is_object(a) and is_object(b) then
			return rawget(a, "id") == rawget(b, "id")
		end
		local na, nb = num(a), num(b)
		if na ~= nil and nb ~= nil then
			return na == nb
		end
		return str(a) == str(b)
	end

	-- Tokens ------------------------------------------------------------------------------------

	local two = {
		["&&"] = true, ["||"] = true, ["=="] = true, ["!="] = true, ["<="] = true, [">="] = true,
		["<<"] = true, [">>"] = true,
	}
	local single = {}
	for c in string.gmatch("+-*/%=!<>#()[],;{}~^|", ".") do
		single[c] = true
	end

	local function tokenize(src)
		local toks = {}
		local i, n = 1, #src
		local braces = 0
		local function at(k)
			return string.sub(src, k, k)
		end
		-- Whether the identifier starting at `k` is a function call (its name then `[`).
		local function call_at(k)
			local j = k
			while j <= n and string.match(at(j), "[%w_]") do
				j = j + 1
			end
			return j > k and at(j) == "["
		end
		while i <= n do
			local c = at(i)
			local pair = string.sub(src, i, i + 1)
			if string.match(c, "%s") then
				i = i + 1
			elseif c == "`" then
				local buf = {}
				i = i + 1
				while i <= n and at(i) ~= "`" do
					if at(i) == "\\" and i < n then
						i = i + 1
					end
					table.insert(buf, at(i))
					i = i + 1
				end
				need(i <= n, "an unclosed ` string")
				i = i + 1
				table.insert(toks, { t = "str", v = table.concat(buf) })
			elseif two[pair] then
				table.insert(toks, { t = pair })
				i = i + 2
			elseif c == ":" and braces > 0 then
				table.insert(toks, { t = ":" })
				i = i + 1
			elseif c == "&" and (not string.match(at(i + 1), "[%a_]") or call_at(i + 1)) then
				-- A bitwise and: `1&getcharintprop[322]`.
				table.insert(toks, { t = "&" })
				i = i + 1
			elseif (c == "$" or c == "&" or c == "@") and string.match(at(i + 1), "[%w_]") then
				local j = i + 1
				while j <= n and (string.match(at(j), "[%w_%.]")
					or (at(j) == "-" and string.match(at(j + 1), "[%a_]"))) do
					j = j + 1
				end
				-- `$` this run's variables, `@` the character's kept ones, `&` the machine's.
				local scope = c == "$" and "vars" or (c == "@" and "pvars" or "gvars")
				table.insert(toks, { t = "var", scope = scope, v = string.sub(src, i + 1, j - 1) })
				i = j
			elseif single[c] then
				if c == "{" then
					braces = braces + 1
				elseif c == "}" then
					braces = math.max(0, braces - 1)
				end
				table.insert(toks, { t = c })
				i = i + 1
			else
				-- A bare run: words, numbers and escaped characters up to the next operator.
				local buf, escaped = {}, false
				while i <= n do
					local d = at(i)
					if d == "\\" and i < n then
						escaped = true
						table.insert(buf, at(i + 1))
						i = i + 2
					elseif d == "`" or two[string.sub(src, i, i + 1)] or (d == ":" and braces > 0)
						or (d == "&" and (not string.match(at(i + 1), "[%a_]") or call_at(i + 1))) then
						break
					elseif single[d] then
						-- A hyphen between letters belongs to the word (`qnxc-chars`).
						if d == "-" and #buf > 0 and string.match(buf[1], "[%a_]") and string.match(buf[#buf], "[%w_]")
							and string.match(at(i + 1), "[%a_]") then
							table.insert(buf, d)
							i = i + 1
						else
							break
						end
					else
						table.insert(buf, d)
						i = i + 1
					end
				end
				local raw = table.concat(buf)
				local trimmed = string.match(raw, "^%s*(.-)%s*$")
				if escaped then
					table.insert(toks, { t = "str", v = raw })
				elseif trimmed ~= "" then
					local number = tonumber(trimmed)
					if number ~= nil then
						table.insert(toks, { t = "num", v = number })
					elseif string.match(trimmed, "^[%a_][%w_]*$") and at(i) == "[" then
						table.insert(toks, { t = "fn", v = string.lower(trimmed) })
					elseif string.match(trimmed, "^[%w_%.%-]+$") then
						table.insert(toks, { t = "word", v = trimmed })
					else
						table.insert(toks, { t = "str", v = raw })
					end
				end
			end
		end
		return toks
	end

	-- Parsing into closures ------------------------------------------------------------------------

	local fns = {}
	X.functions = fns
	-- Functions whose arguments are evaluated only as they need them (`ifthen`).
	local lazy_fns = {}
	X.lazy_functions = lazy_fns

	local function compile(src)
		local toks = tokenize(src)
		local p = 1
		local function peek()
			return toks[p] and toks[p].t
		end
		local function take(t)
			local tok = toks[p]
			-- A bracket left open at the very end is closed there, as the metas' authors relied on.
			if tok == nil and (t == "]" or t == ")" or t == "}") then
				return { t = t }
			end
			need(tok ~= nil and tok.t == t, "expected " .. t .. " in " .. src)
			p = p + 1
			return tok
		end
		local expr
		local primary
		-- `v{i}`, `v{key}`, `v{from:to}` (either end may be left out; a negative counts from the
		-- end): an item of a list or a string, a dictionary's value, or a slice.
		local function postfix(node)
			while peek() == "{" do
				p = p + 1
				local from, to, slice = nil, nil, false
				if peek() ~= ":" then
					from = expr()
				end
				if peek() == ":" then
					p = p + 1
					slice = true
					if peek() ~= "}" then
						to = expr()
					end
				end
				take("}")
				local base, a, b = node, from, to
				node = function(env)
					return X.index(base(env), a and a(env), b and b(env), slice)
				end
			end
			return node
		end

		local function atom()
			local tok = toks[p]
			need(tok ~= nil, "an expression ends early: " .. src)
			p = p + 1
			if tok.t == "num" then
				local v = tok.v
				return function()
					return v
				end
			elseif tok.t == "str" then
				local v = tok.v
				return function()
					return v
				end
			elseif tok.t == "word" then
				local w = tok.v
				local l = string.lower(w)
				if l == "true" then
					return function()
						return true
					end
				elseif l == "false" then
					return function()
						return false
					end
				end
				return function()
					return w
				end
			elseif tok.t == "var" then
				local scope, name = tok.scope, tok.v
				if peek() == "=" then
					p = p + 1
					local value = expr()
					return function(env)
						local v = value(env)
						env.set(scope, name, v)
						return v
					end
				end
				return function(env)
					return env.get(scope, name)
				end
			elseif tok.t == "fn" then
				local name = tok.v
				take("[")
				local args = {}
				if peek() ~= "]" then
					table.insert(args, expr())
					while peek() == "," do
						p = p + 1
						table.insert(args, expr())
					end
				end
				take("]")
				local lazy = lazy_fns[name]
				if lazy ~= nil then
					return lazy(args)
				end
				local f = fns[name]
				if f == nil then
					error("the expression function " .. name .. "[] is not supported", 0)
				end
				return function(env)
					local vals = {}
					for i, a in ipairs(args) do
						vals[i] = a(env)
					end
					return f(env, table.unpack(vals, 1, #args))
				end
			elseif tok.t == "(" then
				local inner = expr()
				take(")")
				return inner
			elseif tok.t == "!" then
				local inner = primary()
				return function(env)
					return not truthy(inner(env))
				end
			elseif tok.t == "-" then
				local inner = primary()
				return function(env)
					return -(num(inner(env)) or 0)
				end
			elseif tok.t == "~" then
				local inner = primary()
				return function(env)
					return bit32.bnot(math.floor(num(inner(env)) or 0) % 4294967296)
				end
			end
			error("unexpected " .. tok.t .. " in " .. src, 0)
		end
		primary = function()
			return postfix(atom())
		end

		local function binary(next_level, ops, apply)
			return function()
				local left = next_level()
				while ops[peek() or ""] do
					local op = toks[p].t
					p = p + 1
					local right = next_level()
					left = apply(op, left, right)
				end
				return left
			end
		end

		local mul = binary(primary, { ["*"] = true, ["/"] = true, ["%"] = true }, function(op, l, r)
			return function(env)
				local a, b = num(l(env)) or 0, num(r(env)) or 0
				if op == "*" then
					return a * b
				elseif op == "/" then
					return a / b
				end
				return a % b
			end
		end)
		local add = binary(mul, { ["+"] = true, ["-"] = true }, function(op, l, r)
			return function(env)
				local a, b = l(env), r(env)
				local na, nb = num(a), num(b)
				if op == "-" then
					return (na or 0) - (nb or 0)
				end
				if type(a) == "number" and type(b) == "number" then
					return a + b
				end
				if na ~= nil and nb ~= nil and type(a) ~= "string" and type(b) ~= "string" then
					return na + nb
				end
				return str(a) .. str(b)
			end
		end)
		local shift = binary(add, { ["<<"] = true, [">>"] = true }, function(op, l, r)
			return function(env)
				local a, b = math.floor(num(l(env)) or 0) % 4294967296, math.floor(num(r(env)) or 0)
				if op == "<<" then
					return bit32.lshift(a, b)
				end
				return bit32.rshift(a, b)
			end
		end)
		local match = binary(shift, { ["#"] = true }, function(_, l, r)
			return function(env)
				return api.regex.match(str(l(env)), str(r(env)), true) ~= nil
			end
		end)
		local cmp = binary(match, {
			["=="] = true, ["!="] = true, ["<"] = true, [">"] = true, ["<="] = true, [">="] = true,
		}, function(op, l, r)
			return function(env)
				local a, b = l(env), r(env)
				if op == "==" then
					return equal(a, b)
				elseif op == "!=" then
					return not equal(a, b)
				end
				local na, nb = num(a), num(b)
				if na == nil or nb == nil then
					na, nb = str(a), str(b)
				end
				if op == "<" then
					return na < nb
				elseif op == ">" then
					return na > nb
				elseif op == "<=" then
					return na <= nb
				end
				return na >= nb
			end
		end)
		local function bits(next_level, op, f)
			return binary(next_level, { [op] = true }, function(_, l, r)
				return function(env)
					return f(math.floor(num(l(env)) or 0) % 4294967296, math.floor(num(r(env)) or 0) % 4294967296)
				end
			end)
		end
		local band = bits(cmp, "&", bit32.band)
		local bxor = bits(band, "^", bit32.bxor)
		local bor = bits(bxor, "|", bit32.bor)
		local function logic(next_level, op_name, short)
			return function()
				local left = next_level()
				while peek() == op_name do
					p = p + 1
					local l, r = left, next_level()
					left = function(env)
						local a = l(env)
						if truthy(a) == short then
							return short
						end
						return truthy(r(env))
					end
				end
				return left
			end
		end
		local conj = logic(bor, "&&", false)
		local disj = logic(conj, "||", true)
		expr = function()
			local first = disj()
			if peek() ~= ";" then
				return first
			end
			local seq = { first }
			while peek() == ";" do
				p = p + 1
				if peek() ~= nil and peek() ~= ")" and peek() ~= "]" and peek() ~= "," then
					table.insert(seq, disj())
				end
			end
			return function(env)
				local v
				for _, e in ipairs(seq) do
					v = e(env)
				end
				return v
			end
		end
		if #toks == 0 then
			return function()
				return ""
			end
		end
		local whole = expr()
		-- A stray closing bracket at the very end is let be, as the metas' authors relied on.
		while toks[p] ~= nil and (toks[p].t == "]" or toks[p].t == ")") do
			p = p + 1
		end
		need(p > #toks, "unexpected " .. tostring(toks[p] and toks[p].t) .. " in " .. src)
		return whole
	end

	-- The functions ---------------------------------------------------------------------------------

	local function list(...)
		return setmetatable({ ... }, list_meta)
	end

	local function obj(id)
		if id == nil or id == 0 then
			return 0
		end
		return api.game.world:get(id) or 0
	end

	local function id_of(v)
		if is_object(v) then
			return rawget(v, "id")
		end
		return num(v)
	end

	local function coords_of(pos)
		if pos == nil or pos.ns == nil then
			return 0
		end
		return setmetatable({ ns = pos.ns, ew = pos.ew, z = (pos.z or 0) / 240 }, coord_meta)
	end

	local function nearest(filter, pred)
		local best, bd = nil, nil
		for _, o in ipairs(api.game.world:find(filter or {})) do
			if pred == nil or pred(o) then
				local d = o:distance()
				if d ~= nil and (bd == nil or d < bd) then
					best, bd = o, d
				end
			end
		end
		return best or 0
	end

	for _, scope in ipairs({ { "", "vars" }, { "p", "pvars" }, { "g", "gvars" } }) do
		local s, key = scope[1], scope[2]
		fns["get" .. s .. "var"] = function(env, name)
			return env.get(key, str(name))
		end
		fns["set" .. s .. "var"] = function(env, name, v)
			env.set(key, str(name), v)
			return v
		end
		fns["test" .. s .. "var"] = function(env, name)
			return env.get(key, str(name)) ~= nil
		end
		fns["touch" .. s .. "var"] = function(env, name)
			if env.get(key, str(name)) == nil then
				env.set(key, str(name), 0)
			end
			return true
		end
		fns["clear" .. s .. "var"] = function(env, name)
			env.set(key, str(name), nil)
			return true
		end
	end
	fns.clearallvars = function(env)
		env.clear("vars")
		return true
	end
	fns.istrue = function(_, v)
		return truthy(v)
	end
	fns.isfalse = function(_, v)
		return not truthy(v)
	end
	fns.iif = function(_, c, a, b)
		if truthy(c) then
			return a
		end
		return b
	end
	fns.cstr = function(_, v)
		return str(v)
	end
	fns.cnumber = function(_, v)
		return num(v) or 0
	end
	fns.strlen = function(_, v)
		return #str(v)
	end
	fns.echo = function(_, text, _color)
		api.chat.print(str(text))
		return true
	end
	fns.chatbox = function(_, text)
		local line = str(text)
		if string.sub(line, 1, 1) == "/" or string.sub(line, 1, 1) == "@" then
			api.chat.command(line)
		else
			api.chat.say(line)
		end
		return true
	end
	fns.getcharintprop = function(_, key)
		return api.game.player_int(num(key)) or 0
	end
	fns.getcharstringprop = function(_, key)
		if num(key) == 1 then
			return api.game.player.name or ""
		end
		return ""
	end
	fns.getplayerlandcell = function()
		local p = api.game.player.position
		return p and p.cell or 0
	end
	fns.getplayercoordinates = function()
		return coords_of(api.game.player.position)
	end
	fns.coordinatedistancewithz = function(_, a, b)
		if getmetatable(a) ~= coord_meta or getmetatable(b) ~= coord_meta then
			return 0
		end
		return math.sqrt(((a.ns - b.ns) * 240) ^ 2 + ((a.ew - b.ew) * 240) ^ 2 + ((a.z - b.z) * 240) ^ 2)
	end
	fns.coordinatedistanceflat = function(_, a, b)
		if getmetatable(a) ~= coord_meta or getmetatable(b) ~= coord_meta then
			return 0
		end
		return math.sqrt(((a.ns - b.ns) * 240) ^ 2 + ((a.ew - b.ew) * 240) ^ 2)
	end
	fns.coordinatetostring = function(_, c)
		return str(c)
	end
	fns.coordinategetns = function(_, c)
		return getmetatable(c) == coord_meta and c.ns or 0
	end
	fns.coordinategetwe = function(_, c)
		return getmetatable(c) == coord_meta and c.ew or 0
	end
	fns.isportaling = function()
		local s = api.session.info()
		return s ~= nil and s.portal_space == true
	end
	fns.getcharvital_current = function(_, which)
		local v = ({ "health", "stamina", "mana" })[num(which) or 0]
		local x = v and api.game.player[v]
		return x and x.current or 0
	end
	fns.getcharvital_base = function(_, which)
		local v = ({ "health", "stamina", "mana" })[num(which) or 0]
		local x = v and api.game.player[v]
		return x and x.max or 0
	end
	fns.getcharburden = function()
		return math.floor((api.game.player.burden or 0) * 100 + 0.5)
	end
	fns.getinventorycountbyname = function(_, name)
		local n = 0
		for _, it in ipairs(api.game.inventory.items()) do
			if it.name == str(name) then
				n = n + (it.stack or 1)
			end
		end
		return n
	end
	fns.wobjectgetplayer = function()
		return obj(api.game.player.id)
	end
	fns.wobjectgetselection = function()
		return api.game.selection() or 0
	end
	fns.wobjectgetopencontainer = function()
		return obj(api.game.inventory.open_container)
	end
	fns.wobjectfindbyid = function(_, id)
		return obj(num(id))
	end
	fns.wobjectgetname = function(_, o)
		return is_object(o) and (o.name or "") or ""
	end
	fns.wobjectgetid = function(_, o)
		return is_object(o) and rawget(o, "id") or 0
	end
	fns.wobjectgetobjectclass = function(_, o)
		return is_object(o) and (o.class_id or 0) or 0
	end
	fns.wobjectgetphysicscoordinates = function(_, o)
		return is_object(o) and coords_of(o.position) or 0
	end
	fns.wobjectfindnearestmonster = function()
		return nearest({ monster = true })
	end
	fns.wobjectfindnearestbyobjectclass = function(_, class)
		local c = num(class)
		return nearest({}, function(o)
			return o.class_id == c and rawget(o, "id") ~= api.game.player.id
		end)
	end
	fns.wobjectfindnearestbynameandobjectclass = function(_, class, name)
		local c, n = num(class), str(name)
		return nearest({ name = n }, function(o)
			return o.class_id == c
		end)
	end
	fns.wobjectfindininventorybyname = function(_, name)
		for _, it in ipairs(api.game.inventory.items()) do
			if it.name == str(name) then
				return it
			end
		end
		return 0
	end
	fns.actiontryselect = function(_, o)
		local id = id_of(o)
		if id == nil or id == 0 then
			return false
		end
		api.act.select(id)
		return true
	end
	fns.actiontryuseitem = function(_, o)
		local id = id_of(o)
		if id == nil or id == 0 then
			return false
		end
		api.act.use(id)
		return true
	end
	fns.actiontryapplyitem = function(_, a, b)
		local x, y = id_of(a), id_of(b)
		if not x or x == 0 or not y or y == 0 then
			return false
		end
		api.act.use_on(x, y)
		return true
	end
	fns.actiontrycastbyid = function(_, spell)
		local s = num(spell)
		if s == nil or s == 0 then
			return false
		end
		api.magic.cast(s)
		return true
	end
	fns.actiontrycastbyidontarget = function(_, spell, o)
		local s, t = num(spell), id_of(o)
		if s == nil or s == 0 or t == nil or t == 0 then
			return false
		end
		api.magic.cast(s, t)
		return true
	end
	fns.listcreate = function(_, ...)
		return list(...)
	end
	fns.listadd = function(_, l, v)
		if getmetatable(l) == list_meta then
			table.insert(l, v)
		end
		return l
	end
	fns.listcontains = function(_, l, v)
		if getmetatable(l) ~= list_meta then
			return false
		end
		for _, x in ipairs(l) do
			if equal(x, v) then
				return true
			end
		end
		return false
	end
	fns.listcount = function(_, l)
		return getmetatable(l) == list_meta and #l or 0
	end
	fns.listgetitem = function(_, l, i)
		return getmetatable(l) == list_meta and l[(num(i) or 0) + 1] or 0
	end
	fns.listremove = function(_, l, v)
		if getmetatable(l) == list_meta then
			for i, x in ipairs(l) do
				if equal(x, v) then
					table.remove(l, i)
					break
				end
			end
		end
		return l
	end
	fns.listclear = function(_, l)
		if getmetatable(l) == list_meta then
			table.clear(l)
		end
		return l
	end
	fns.getgametime = function()
		return api.os.time()
	end
	-- UtilityBelt's additions to the language ---------------------------------------------------
	-- Dictionaries, the list and string library, math, stopwatches, the character's and the
	-- world's reads, quests, the calendar, and the macro's own state, as UtilityBelt names them.

	local dict_meta, watch_meta = X.dict_meta, X.watch_meta
	local function is_list(v)
		return getmetatable(v) == list_meta
	end
	local function is_dict(v)
		return getmetatable(v) == dict_meta
	end
	local function new_dict()
		return setmetatable({ keys = {}, values = {} }, dict_meta)
	end
	local function dict_set(d, k, v)
		k = str(k)
		if d.values[k] == nil then
			table.insert(d.keys, k)
		end
		d.values[k] = v
	end
	local function dict_remove(d, k)
		k = str(k)
		if d.values[k] == nil then
			return false
		end
		d.values[k] = nil
		for i, x in ipairs(d.keys) do
			if x == k then
				table.remove(d.keys, i)
				break
			end
		end
		return true
	end
	local function list_of(t)
		return setmetatable(t, list_meta)
	end
	local function copy(l)
		return list_of(table.clone(l))
	end
	-- A list index from 0, a negative one counting from the end: the Lua index, or nil outside.
	local function at_index(n, i, len)
		i = math.floor(num(i) or 0)
		if i < 0 then
			i = len + i
		end
		if i < 0 or i >= len then
			return nil
		end
		return i + 1
	end
	local function slice_bounds(len, a, b)
		local function norm(v, d)
			if v == nil then
				return d
			end
			v = math.floor(num(v) or 0)
			if v < 0 then
				v = len + v
			end
			return math.max(0, math.min(len, v))
		end
		return norm(a, 0), norm(b, len)
	end

	-- `v{i}`, `v{key}`, `v{from:to}`.
	function X.index(v, a, b, slice)
		if is_list(v) then
			if slice then
				local from, to = slice_bounds(#v, a, b)
				local out = {}
				for k = from + 1, to do
					table.insert(out, v[k])
				end
				return list_of(out)
			end
			local k = at_index(nil, a, #v)
			return k and v[k] or 0
		elseif is_dict(v) then
			local x = v.values[str(a)]
			if x == nil then
				return 0
			end
			return x
		elseif type(v) == "string" or type(v) == "number" then
			local s = str(v)
			if slice then
				local from, to = slice_bounds(#s, a, b)
				return string.sub(s, from + 1, to)
			end
			local k = at_index(nil, a, #s)
			return k and string.sub(s, k, k) or ""
		end
		return 0
	end

	-- Evaluate `text` once per item with `$0` the count and `$1` the item (and `$2`), keeping the
	-- variables they shadow.
	local function with_items(env, text)
		local f = compile(str(text))
		return function(i, a, b)
			local saved = { env.get("vars", "0"), env.get("vars", "1"), env.get("vars", "2") }
			env.set("vars", "0", i)
			env.set("vars", "1", a)
			env.set("vars", "2", b)
			local ok, v = pcall(f, env)
			env.set("vars", "0", saved[1])
			env.set("vars", "1", saved[2])
			env.set("vars", "2", saved[3])
			if not ok then
				error(v, 0)
			end
			return v
		end
	end

	-- Lists -------------------------------------------------------------------------------------

	fns.listinsert = function(_, l, v, i)
		if is_list(l) then
			local k = math.floor(num(i) or #l)
			if k < 0 then
				k = #l + k + 1
			end
			table.insert(l, math.max(1, math.min(#l + 1, k + 1)), v)
		end
		return l
	end
	fns.listremoveat = function(_, l, i)
		if is_list(l) then
			local k = at_index(nil, i, #l)
			if k then
				table.remove(l, k)
			end
		end
		return l
	end
	fns.listindexof = function(_, l, v)
		if is_list(l) then
			for i, x in ipairs(l) do
				if equal(x, v) then
					return i - 1
				end
			end
		end
		return -1
	end
	fns.listlastindexof = function(_, l, v)
		if is_list(l) then
			for i = #l, 1, -1 do
				if equal(l[i], v) then
					return i - 1
				end
			end
		end
		return -1
	end
	fns.listcopy = function(_, l)
		return is_list(l) and copy(l) or list()
	end
	fns.listreverse = function(_, l)
		local out = {}
		if is_list(l) then
			for i = #l, 1, -1 do
				table.insert(out, l[i])
			end
		end
		return list_of(out)
	end
	fns.listpop = function(_, l, i)
		if not is_list(l) or #l == 0 then
			return 0
		end
		local k = at_index(nil, i == nil and -1 or i, #l)
		if k == nil then
			return 0
		end
		return table.remove(l, k)
	end
	fns.listfilter = function(env, l, text)
		local f, out = with_items(env, text), {}
		for i, x in ipairs(is_list(l) and l or {}) do
			if truthy(f(i - 1, x)) then
				table.insert(out, x)
			end
		end
		return list_of(out)
	end
	fns.listmap = function(env, l, text)
		local f, out = with_items(env, text), {}
		for i, x in ipairs(is_list(l) and l or {}) do
			table.insert(out, f(i - 1, x))
		end
		return list_of(out)
	end
	fns.listreduce = function(env, l, text)
		local f, acc = with_items(env, text), 0
		for i, x in ipairs(is_list(l) and l or {}) do
			acc = f(i - 1, x, acc)
		end
		return acc
	end
	fns.listsort = function(env, l, text)
		local out = is_list(l) and copy(l) or list()
		if text == nil then
			table.sort(out, function(a, b)
				local na, nb = num(a), num(b)
				if na ~= nil and nb ~= nil then
					return na < nb
				end
				return str(a) < str(b)
			end)
			return out
		end
		local f = with_items(env, text)
		table.sort(out, function(a, b)
			local v = f(0, a, b)
			if type(v) == "boolean" then
				return v
			end
			return (num(v) or 0) < 0
		end)
		return out
	end
	fns.listfromrange = function(_, a, b)
		local out = {}
		local from, to = math.floor(num(a) or 0), math.floor(num(b) or 0)
		local step = from <= to and 1 or -1
		for i = from, to, step do
			table.insert(out, i)
		end
		return list_of(out)
	end

	-- Dictionaries ------------------------------------------------------------------------------

	fns.dictcreate = function(_, ...)
		local d, args = new_dict(), table.pack(...)
		for i = 1, args.n - 1, 2 do
			dict_set(d, args[i], args[i + 1])
		end
		return d
	end
	fns.dictgetitem = function(_, d, k)
		return X.index(d, k)
	end
	fns.dictadditem = function(_, d, k, v)
		if is_dict(d) then
			dict_set(d, k, v)
		end
		return d
	end
	fns.dicthaskey = function(_, d, k)
		return is_dict(d) and d.values[str(k)] ~= nil
	end
	fns.dictremovekey = function(_, d, k)
		return is_dict(d) and dict_remove(d, k)
	end
	fns.dictkeys = function(_, d)
		return list_of(is_dict(d) and table.clone(d.keys) or {})
	end
	fns.dictvalues = function(_, d)
		local out = {}
		for _, k in ipairs(is_dict(d) and d.keys or {}) do
			table.insert(out, d.values[k])
		end
		return list_of(out)
	end
	fns.dictsize = function(_, d)
		return is_dict(d) and #d.keys or 0
	end
	fns.dictclear = function(_, d)
		if is_dict(d) then
			table.clear(d.keys)
			table.clear(d.values)
		end
		return d
	end
	fns.dictcopy = function(_, d)
		local out = new_dict()
		for _, k in ipairs(is_dict(d) and d.keys or {}) do
			dict_set(out, k, d.values[k])
		end
		return out
	end

	-- Strings and math --------------------------------------------------------------------------

	fns.tostring = function(_, v)
		return str(v)
	end
	fns.getregexmatch = function(_, text, pattern)
		local m = api.regex.match(str(pattern), str(text), true)
		return m and m.text or ""
	end
	-- A number's digits grouped by thousands ("1234567" -> "1,234,567"), a sign kept.
	local function grouped(int)
		local sign, digits = string.match(int, "^(-?)(%d*)$")
		digits = string.reverse((string.gsub(string.reverse(digits or ""), "(%d%d%d)", "%1,")))
		return (sign or "") .. string.gsub(digits, "^,", "")
	end
	-- A number in .NET's custom format (`0.00`, `#,##0.0`, `0%`, with text around it): `0` a digit
	-- always shown, `#` one shown when it counts, `,` in the whole part grouping by thousands, `%`
	-- the number as a percentage.
	local function custom(n, f)
		local first = string.find(f, "[0#]")
		if first == nil then
			return f
		end
		local last = first
		for i = first, #f do
			if string.find(string.sub(f, i, i), "[0#,%.]") then
				last = i
			else
				break
			end
		end
		local prefix, pattern, suffix = string.sub(f, 1, first - 1), string.sub(f, first, last), string.sub(f, last + 1)
		if string.find(prefix .. suffix, "%%") then
			n = n * 100
		end
		local whole, frac = string.match(pattern, "^([^%.]*)%.?(.*)$")
		local max_frac = #(string.gsub(frac, "[^0#]", ""))
		local min_frac = #(string.match(frac, "^0*"))
		local min_int = #(string.gsub(string.match(whole, "0[0,]*$") or "", ",", ""))
		local s = string.format("%." .. max_frac .. "f", n)
		local int, dec = string.match(s, "^(-?%d+)%.?(%d*)$")
		while #dec > min_frac and string.sub(dec, -1) == "0" do
			dec = string.sub(dec, 1, -2)
		end
		local neg = string.sub(int, 1, 1) == "-"
		int = neg and string.sub(int, 2) or int
		if int == "0" and min_int == 0 then
			int = ""
		end
		while #int < min_int do
			int = "0" .. int
		end
		if string.find(whole, ",") then
			int = grouped(int)
		end
		return prefix .. (neg and "-" or "") .. int .. (#dec > 0 and ("." .. dec) or "") .. suffix
	end
	-- A number formatted as .NET formats it: the standard formats `C`, `D`, `E`, `F`, `G`, `N`,
	-- `P`, `R` and `X` (with a precision, as `N2`), or a custom one (`0.00`, `#,##0`, `0%`).
	fns.cstrf = function(_, v, fmt)
		local n, f = num(v) or 0, str(fmt)
		local letter, digits = string.match(f, "^([CcDdEeFfGgNnPpRrXx])(%d*)$")
		digits = tonumber(digits or "")
		if letter == nil then
			return custom(n, f)
		end
		local l = string.lower(letter)
		if l == "f" or l == "n" or l == "c" or l == "p" then
			local x = l == "p" and n * 100 or n
			local s = string.format("%." .. (digits or 2) .. "f", math.abs(x))
			if l ~= "f" then
				local int, frac = string.match(s, "^(%d+)(.*)$")
				s = grouped(int) .. frac
			end
			if l == "c" then
				s = "$" .. s
			elseif l == "p" then
				s = s .. " %"
			end
			return (x < 0 and "-" or "") .. s
		elseif l == "d" then
			local s = tostring(math.abs(math.floor(n)))
			while #s < (digits or 0) do
				s = "0" .. s
			end
			return (n < 0 and "-" or "") .. s
		elseif l == "x" then
			local s = string.format(letter == "X" and "%X" or "%x", math.floor(n) % 4294967296)
			while #s < (digits or 0) do
				s = "0" .. s
			end
			return s
		elseif l == "e" then
			local s = string.format("%." .. (digits or 6) .. "e", n)
			local mant, sign, exp = string.match(s, "^(.-)e([%+%-])(%d+)$")
			while #exp < 3 do
				exp = "0" .. exp
			end
			return mant .. (letter == "E" and "E" or "e") .. sign .. exp
		end
		-- G and R: the shortest form.
		return str(n)
	end
	fns.ord = function(_, c)
		local s = str(c)
		return #s > 0 and utf8.codepoint(s, 1) or 0
	end
	fns.chr = function(_, n)
		return utf8.char(math.floor(num(n) or 0))
	end
	fns.hexstr = function(_, n)
		return string.format("0x%X", math.floor(num(n) or 0) % 4294967296)
	end
	for _, name in ipairs({ "floor", "abs", "sqrt", "sin", "cos", "tan", "sinh", "cosh", "tanh", "asin", "acos", "atan" }) do
		local f = math[name]
		fns[name] = function(_, v)
			return f(num(v) or 0)
		end
	end
	fns.ceiling = function(_, v)
		return math.ceil(num(v) or 0)
	end
	fns.round = function(_, v)
		local n = num(v) or 0
		return n >= 0 and math.floor(n + 0.5) or -math.floor(-n + 0.5)
	end
	fns.atan2 = function(_, y, x)
		return math.atan2(num(y) or 0, num(x) or 0)
	end
	fns.randint = function(_, a, b)
		local lo, hi = math.floor(num(a) or 0), math.floor(num(b) or 0)
		if hi <= lo then
			return lo
		end
		return math.random(lo, hi - 1)
	end

	-- Logic and evaluation ----------------------------------------------------------------------

	lazy_fns.ifthen = function(args)
		local c, a, b = args[1], args[2], args[3]
		return function(env)
			if c ~= nil and truthy(c(env)) then
				return a and a(env) or 0
			end
			return b and b(env) or 0
		end
	end
	fns.exec = function(env, text)
		return compile(str(text))(env)
	end
	fns.delayexec = function(env, ms, text)
		local f = compile(str(text))
		api.task.after(math.max(0, num(ms) or 0), function()
			local ok, err = pcall(f, env)
			if not ok then
				api.print("delayexec: " .. tostring(err))
			end
		end)
		return true
	end
	fns.clearallpvars = function(env)
		env.clear("pvars")
		return true
	end
	fns.clearallgvars = function(env)
		env.clear("gvars")
		return true
	end

	-- Stopwatches -------------------------------------------------------------------------------

	fns.stopwatchcreate = function()
		return setmetatable({ elapsed = 0, since = nil }, watch_meta)
	end
	fns.stopwatchstart = function(_, w)
		if getmetatable(w) == watch_meta and w.since == nil then
			w.since = api.os.clock()
		end
		return w
	end
	fns.stopwatchstop = function(_, w)
		if getmetatable(w) == watch_meta and w.since ~= nil then
			w.elapsed = w.elapsed + api.os.clock() - w.since
			w.since = nil
		end
		return w
	end
	fns.stopwatchelapsedseconds = function(_, w)
		if getmetatable(w) ~= watch_meta then
			return 0
		end
		return w.elapsed + (w.since and api.os.clock() - w.since or 0)
	end

	-- The character -----------------------------------------------------------------------------

	local function me()
		local id = api.game.player.id
		return id and api.game.world:get(id)
	end
	local function prop_of(o, ty, key)
		if not is_object(o) then
			return 0
		end
		local v = o:prop(ty, num(key) or 0)
		if v == nil then
			return ty == "string" and "" or 0
		end
		return v
	end
	fns.getchardoubleprop = function(_, k)
		return prop_of(me(), "float", k)
	end
	fns.getcharquadprop = function(_, k)
		return prop_of(me(), "int64", k)
	end
	fns.getcharboolprop = function(_, k)
		return prop_of(me(), "bool", k)
	end
	fns.getcharstringprop = function(_, k)
		if num(k) == 1 then
			return api.game.player.name or ""
		end
		return prop_of(me(), "string", k)
	end
	local VITALS = { "health", "stamina", "mana" }
	fns.getcharvital_buffedmax = function(_, which)
		local x = VITALS[num(which) or 0] and api.game.player[VITALS[num(which)]]
		return x and x.max or 0
	end
	fns.getcharvital_base = function(_, which)
		local name = VITALS[num(which) or 0]
		for _, v in ipairs(name and api.game.player.vital_stats or {}) do
			if v.name == name and v.base ~= nil then
				return v.base
			end
		end
		local x = name and api.game.player[name]
		return x and x.max or 0
	end
	local function skill(id)
		for _, s in ipairs(api.game.player.skills or {}) do
			if s.id == num(id) then
				return s
			end
		end
		return nil
	end
	fns.getcharskill_traininglevel = function(_, id)
		local s = skill(id)
		return s and ({ untrained = 1, trained = 2, specialized = 3 })[s.training] or 0
	end
	fns.getcharskill_base = function(_, id)
		local s = skill(id)
		return s and s.base or 0
	end
	fns.getcharskill_buffed = function(_, id)
		local s = skill(id)
		return s and s.current or 0
	end
	local function attribute(id)
		for _, a in ipairs(api.game.player.attributes or {}) do
			if a.id == num(id) then
				return a
			end
		end
		return nil
	end
	fns.getcharattribute_base = function(_, id)
		local a = attribute(id)
		return a and a.base or 0
	end
	fns.getcharattribute_buffed = function(_, id)
		local a = attribute(id)
		return a and a.current or 0
	end
	fns.vitae = function()
		local v = api.game.player.vitae
		if v == nil then
			return 0
		end
		return math.floor((1 - v) * 100 + 0.5)
	end
	fns.getcombatstate = function()
		return ({ [1] = "Peace", [2] = "Melee", [4] = "Missile", [8] = "Magic" })[api.game.player.combat_mode] or "Peace"
	end
	fns.setcombatstate = function(_, state)
		local s = string.lower(str(state))
		api.combat.set_mode((s == "peace" or s == "noncombat") and "peace" or "combat")
		return true
	end
	fns.getbusystate = function()
		return api.game.player.busy and 1 or 0
	end
	fns.getequippedweapontype = function()
		local id = api.game.player.id
		for _, it in ipairs(api.game.inventory.items()) do
			local c = it.class_id
			if it.wielder == id and (c == 1 or c == 9 or c == 31) then
				return c
			end
		end
		return 0
	end

	-- Places and movement -----------------------------------------------------------------------

	fns.getplayerlandblock = function()
		local p = api.game.player.position
		return p and p.cell and (p.cell - p.cell % 65536) or 0
	end
	fns.coordinategetz = function(_, c)
		return getmetatable(c) == coord_meta and c.z or 0
	end
	fns.coordinateparse = function(_, text)
		local s = string.upper(str(text))
		local a, ns, b, ew = string.match(s, "([%d%.]+)%s*([NS])[%s,]*([%d%.]+)%s*([EW])")
		if a == nil then
			return 0
		end
		local n, e = tonumber(a), tonumber(b)
		return setmetatable({ ns = ns == "S" and -n or n, ew = ew == "W" and -e or e, z = 0 }, coord_meta)
	end
	fns.getheading = function(_, o)
		local p = is_object(o) and o.position
		return p and p.heading or 0
	end
	fns.getheadingto = function(_, o)
		local a, b = api.game.player.position, is_object(o) and o.position
		if a == nil or b == nil or a.ns == nil or b.ns == nil then
			return 0
		end
		local d = math.deg(math.atan2(b.ew - a.ew, b.ns - a.ns))
		return d % 360
	end
	-- The run lock is the one motion a plugin holds (it never holds a key): `Forward` sets it.
	fns.setmotion = function(_, motion, state)
		if string.lower(str(motion)) ~= "forward" then
			return false
		end
		api.nav.autorun(truthy(state))
		return true
	end
	fns.getmotion = function(_, motion)
		if string.lower(str(motion)) ~= "forward" then
			return 0
		end
		return api.game.player.autorun and 1 or 0
	end
	fns.clearmotion = function()
		if api.game.player.autorun then
			api.nav.autorun(false)
		end
		return true
	end

	-- Time and the world ------------------------------------------------------------------------

	local function game_time(field)
		local t = api.game.player.game_time
		return t and t[field] or 0
	end
	fns.getgameyear = function()
		return game_time("year")
	end
	fns.getgamemonth = function()
		return game_time("month")
	end
	fns.getgameday = function()
		return game_time("day")
	end
	fns.getgamehour = function()
		return game_time("hour")
	end
	fns.getgameticks = function()
		return game_time("ticks")
	end
	fns.getisday = function()
		return game_time("is_day") == true
	end
	fns.getisnight = function()
		return game_time("is_day") == false
	end
	fns.getminutesuntilday = function()
		return game_time("minutes_until_day")
	end
	fns.getminutesuntilnight = function()
		return game_time("minutes_until_night")
	end
	local MONTHS = { "Morningthaw", "Solclaim", "Seedsow", "Leafdawning", "Verdantine", "Thistledown",
		"Harvestgain", "Leafcull", "Frostfell", "Snowreap", "Coldeve", "Wintersebb" }
	local HOURS = { "Darktide", "Darktide-and-Half", "Foredawn", "Foredawn-and-Half", "Dawnsong",
		"Dawnsong-and-Half", "Morntide", "Morntide-and-Half", "Midsong", "Midsong-and-Half", "Warmtide",
		"Warmtide-and-Half", "Evensong", "Evensong-and-Half", "Gloaming", "Gloaming-and-Half" }
	fns.getgamemonthname = function(_, i)
		return MONTHS[(num(i) or 0) + 1] or ""
	end
	fns.getgamehourname = function(_, i)
		return HOURS[(num(i) or 0) + 1] or ""
	end
	fns.getworldname = function()
		local s = api.session.info()
		return s and s.world or ""
	end
	fns.getaccounthash = function()
		local s = api.session.info()
		local h = 2166136261
		for c in string.gmatch((s and s.account) or "", ".") do
			h = (bit32.bxor(h, string.byte(c)) * 16777619) % 4294967296
		end
		return string.format("%08X", h)
	end
	local function date(utc, fmt)
		local f = str(fmt)
		if not string.find(f, "%", 1, true) then
			f = "%Y-%m-%d %H:%M:%S"
		end
		return api.os.date((utc and "!" or "") .. f)
	end
	fns.getdatetimelocal = function(_, fmt)
		return date(false, fmt)
	end
	fns.getdatetimeutc = function(_, fmt)
		return date(true, fmt)
	end
	fns.getunixtime = function()
		return api.os.time()
	end

	-- Spells ------------------------------------------------------------------------------------

	fns.getisspellknown = function(_, id)
		local s = api.magic.spell(num(id) or 0)
		return s ~= nil and s.known == true
	end
	-- Whether the character knows a spell and casts it reliably: its buffed skill in the spell's
	-- school at least the spell's difficulty and the threshold more (VTank's
	-- SpellDiffExcessThreshold-Hunt or -Buff, as the engine a meta drives has it, else VTank's
	-- defaults, 25 and 5). Without a skill or a difficulty to compare, a known spell counts.
	local function can_cast(env, id, setting, default)
		local s = api.magic.spell(num(id) or 0)
		if s == nil or s.known ~= true then
			return false
		end
		local school_skill = ({ "War Magic", "Life Magic", "Item Enchantment", "Creature Enchantment", "Void Magic" })[s.school or 0]
		local skill = nil
		for _, sk in ipairs(api.game.player.skills or {}) do
			if sk.name == school_skill then
				skill = sk.current
			end
		end
		if skill == nil or s.difficulty == nil then
			return true
		end
		local e = env.meta and env.meta.engine
		local excess = e and e.option and tonumber(e.option(setting)) or default
		return skill >= s.difficulty + excess
	end
	fns.getcancastspell_hunt = function(env, id)
		return can_cast(env, id, "SpellDiffExcessThreshold-Hunt", 25)
	end
	fns.getcancastspell_buff = function(env, id)
		return can_cast(env, id, "SpellDiffExcessThreshold-Buff", 5)
	end
	local function remaining(pred)
		local best = 0
		for _, e in ipairs(api.game.player.enchantments or {}) do
			if pred(e) then
				best = math.max(best, e.remaining or math.huge)
			end
		end
		return best == math.huge and 999999 or best
	end
	fns.getspellexpiration = function(_, id)
		local want = num(id)
		return remaining(function(e)
			return e.spell == want
		end)
	end
	fns.getspellexpirationbyname = function(_, name)
		local want = string.lower(str(name))
		return remaining(function(e)
			return string.lower(e.name or "") == want
		end)
	end
	fns.spellname = function(_, id)
		local s = api.magic.spell(num(id) or 0)
		return s and s.name or ""
	end
	fns.spelldata = function(_, id)
		local s, d = api.magic.spell(num(id) or 0), new_dict()
		for _, k in ipairs({ "id", "name", "school", "level", "category", "difficulty", "mana", "description" }) do
			if s and s[k] ~= nil then
				dict_set(d, k, s[k])
			end
		end
		return d
	end
	fns.getknownspells = function()
		local out = {}
		for _, s in ipairs(api.magic.known()) do
			table.insert(out, s.id)
		end
		return list_of(out)
	end

	-- Quests: the server's quest list (`/myquests`), read from its lines as they arrive ---------------

	local quests = { flags = {}, refreshing = false, listening = false }
	X.quests = quests
	local function listen_quests()
		if quests.listening then
			return
		end
		quests.listening = true
		api.on("chat.line", function(e)
			local name, solves, last, rest = string.match(e.text or "", '^(%S+) %- (%d+) solves %((%d+)%)(.*)$')
			if name ~= nil then
				local max, delta = string.match(rest, '"[^"]*"%s+(%-?%d+)%s+(%d+)')
				quests.flags[string.lower(name)] = {
					solves = tonumber(solves),
					last = tonumber(last),
					max = tonumber(max or "0"),
					delta = tonumber(delta or "0"),
				}
				quests.refreshing = false
			elseif e.text == "Quest list is empty." then
				quests.refreshing = false
			end
		end)
	end
	-- `expr.refresh_quests()`: ask the server for the character's quest list (needs `chat.send`).
	function X.refresh_quests()
		listen_quests()
		quests.refreshing = true
		api.chat.command("/myquests")
	end
	local function quest(flag)
		listen_quests()
		return quests.flags[string.lower(str(flag))]
	end
	fns.testquestflag = function(_, flag)
		return quest(flag) ~= nil
	end
	fns.getqueststatus = function(_, flag)
		local q = quest(flag)
		if q == nil then
			return 1
		end
		return (q.last + q.delta <= api.os.time()) and 1 or 0
	end
	fns.getquestktprogress = function(_, flag)
		local q = quest(flag)
		return q and q.solves or 0
	end
	fns.getquestktrequired = function(_, flag)
		local q = quest(flag)
		return q and q.max or 0
	end
	fns.isrefreshingquests = function()
		listen_quests()
		return quests.refreshing
	end

	-- World objects -----------------------------------------------------------------------------

	local function ids_list(objs)
		return list_of(objs)
	end
	local function rx(pattern)
		local p = str(pattern)
		return function(o)
			return api.regex.match(p, o.name or "", true) ~= nil
		end
	end
	local function by_class(c)
		local want = num(c)
		return function(o)
			return o.class_id == want
		end
	end
	local function by_template(t)
		local want = num(t)
		return function(o)
			return o.wcid == want
		end
	end
	local function inventory(pred)
		local out = {}
		for _, it in ipairs(api.game.inventory.items()) do
			if pred == nil or pred(it) then
				table.insert(out, it)
			end
		end
		return out
	end
	local function landscape(o)
		return o.container == nil and o.wielder == nil and rawget(o, "id") ~= api.game.player.id
	end
	local function both(a, b)
		return function(o)
			return a(o) and b(o)
		end
	end
	fns.wobjectfindnearestdoor = function()
		return nearest({ class = "Door" })
	end
	fns.wobjectfindnearestbytemplatetype = function(_, t)
		return nearest({ wcid = num(t) or 0 })
	end
	fns.wobjectfindininventorybytemplatetype = function(_, t)
		return inventory(by_template(t))[1] or 0
	end
	fns.wobjectfindininventorybynamerx = function(_, p)
		return inventory(rx(p))[1] or 0
	end
	fns.wobjectfindallinventory = function()
		return ids_list(inventory())
	end
	fns.wobjectfindallinventorybyobjectclass = function(_, c)
		return ids_list(inventory(by_class(c)))
	end
	fns.wobjectfindallinventorybytemplatetype = function(_, t)
		return ids_list(inventory(by_template(t)))
	end
	fns.wobjectfindallinventorybynamerx = function(_, p)
		return ids_list(inventory(rx(p)))
	end
	fns.wobjectfindall = function()
		return ids_list(api.game.world:find({}))
	end
	fns.wobjectfindallbyobjectclass = function(_, c)
		return ids_list(api.game.world:find({ class = num(c) or 0 }))
	end
	fns.wobjectfindallbytemplatetype = function(_, t)
		return ids_list(api.game.world:find({ wcid = num(t) or 0 }))
	end
	fns.wobjectfindallbynamerx = function(_, p)
		return ids_list(api.game.world:find({}, rx(p)))
	end
	fns.wobjectfindalllandscape = function()
		return ids_list(api.game.world:find({}, landscape))
	end
	fns.wobjectfindalllandscapebyobjectclass = function(_, c)
		return ids_list(api.game.world:find({ class = num(c) or 0 }, landscape))
	end
	fns.wobjectfindalllandscapebytemplatetype = function(_, t)
		return ids_list(api.game.world:find({ wcid = num(t) or 0 }, landscape))
	end
	fns.wobjectfindalllandscapebynamerx = function(_, p)
		return ids_list(api.game.world:find({}, both(landscape, rx(p))))
	end
	fns.wobjectfindallbycontainer = function(_, c)
		return ids_list(api.game.world:find({ container = id_of(c) or 0 }))
	end
	fns.wobjectgettemplatetype = function(_, o)
		return is_object(o) and (o.wcid or 0) or 0
	end
	fns.wobjectgetintprop = function(_, o, k)
		return prop_of(o, "int", k)
	end
	fns.wobjectgetdoubleprop = function(_, o, k)
		return prop_of(o, "float", k)
	end
	fns.wobjectgetboolprop = function(_, o, k)
		return prop_of(o, "bool", k)
	end
	fns.wobjectgetstringprop = function(_, o, k)
		if num(k) == 1 then
			return is_object(o) and (o.name or "") or ""
		end
		return prop_of(o, "string", k)
	end
	fns.wobjectgethealth = function(_, o)
		local h = is_object(o) and o:health()
		return h and h.fraction or -1
	end
	fns.wobjectgetspellids = function(_, o)
		return list_of(is_object(o) and table.clone(o.spells or {}) or {})
	end
	fns.wobjectgetactivespellids = function(_, o)
		local out = {}
		if is_object(o) and rawget(o, "id") == api.game.player.id then
			for _, e in ipairs(api.game.player.enchantments or {}) do
				table.insert(out, e.spell)
			end
		end
		return list_of(out)
	end
	fns.wobjecthasdata = function(_, o)
		return is_object(o) and o.appraised == true
	end
	fns.wobjectrequestdata = function(_, o)
		local id = id_of(o)
		if id == nil or id == 0 then
			return false
		end
		api.act.appraise(id)
		return true
	end
	fns.wobjectgetisdooropen = function(_, o)
		return is_object(o) and o.open == true
	end
	-- When its last appraisal arrived, in seconds on the plugin's clock (`os.clock`); 0 when none.
	fns.wobjectlastidtime = function(_, o)
		local t = is_object(o) and o.appraised_at
		if t == nil then
			return 0
		end
		return t
	end
	fns.wobjectisvalid = function(_, o)
		return is_object(o) and api.game.world:get(rawget(o, "id")) ~= nil
	end
	fns.getobjectinternaltype = function(_, v)
		if v == nil then
			return 0
		elseif type(v) == "number" or type(v) == "boolean" then
			return 1
		elseif type(v) == "string" then
			return 3
		elseif is_object(v) then
			return 7
		elseif is_list(v) then
			return 8
		elseif is_dict(v) then
			return 10
		elseif getmetatable(v) == coord_meta then
			return 6
		elseif getmetatable(v) == watch_meta then
			return 9
		end
		return 0
	end

	-- Inventory ---------------------------------------------------------------------------------

	local function count(items)
		local n = 0
		for _, it in ipairs(items) do
			n = n + (it.stack or 1)
		end
		return n
	end
	fns.getitemcountininventorybyname = function(_, name)
		local want = str(name)
		return count(inventory(function(o)
			return o.name == want
		end))
	end
	fns.getitemcountininventorybynamerx = function(_, p)
		return count(inventory(rx(p)))
	end
	fns.getinventorycountbytemplatetype = function(_, t)
		return count(inventory(by_template(t)))
	end
	fns.getfreeitemslots = function()
		return api.game.inventory.free_slots or 0
	end
	fns.getfreecontainerslots = function()
		local p = me()
		if not p then
			return 0
		end
		local cap = p:int(0x0D000005) or 0
		local packs = 0
		for _, it in ipairs(inventory(function(o)
			return o.container == api.game.player.id and o.class_id == 10
		end)) do
			packs = packs + (it and 1 or 0)
		end
		return math.max(0, cap - packs)
	end
	fns.getcontaineritemcount = function()
		local c = api.game.inventory.open_container
		if c == nil then
			return 0
		end
		return #api.game.world:find({ container = c })
	end

	-- Actions -----------------------------------------------------------------------------------

	fns.actiontrygiveitem = function(_, item, target)
		local a, b = id_of(item), id_of(target)
		if not a or a == 0 or not b or b == 0 then
			return false
		end
		api.items.give(a, b)
		return true
	end
	fns.actiontryequipanywand = function()
		local id = api.game.player.id
		for _, it in ipairs(api.game.inventory.items()) do
			if it.class_id == 31 then
				if it.wielder == id then
					return true
				end
				api.items.wield(rawget(it, "id"))
				return true
			end
		end
		return false
	end

	-- Fellowship --------------------------------------------------------------------------------

	local function fellowship()
		return api.game.player.fellowship
	end
	local function members_sorted(by)
		local f, out = fellowship(), {}
		for _, m in ipairs(f and f.members or {}) do
			table.insert(out, m)
		end
		table.sort(out, function(a, b)
			return a[by] < b[by]
		end)
		return out
	end
	fns.getfellowshipstatus = function()
		return fellowship() ~= nil
	end
	fns.getfellowshipname = function()
		local f = fellowship()
		return f and f.name or ""
	end
	fns.getfellowshipcount = function()
		local f = fellowship()
		return f and #f.members or 0
	end
	fns.getfellowshipleaderid = function()
		local f = fellowship()
		return f and f.leader or 0
	end
	fns.getfellowid = function(_, i)
		local m = members_sorted("name")[(num(i) or 0) + 1]
		return m and m.id or 0
	end
	fns.getfellowname = function(_, i)
		local m = members_sorted("name")[(num(i) or 0) + 1]
		return m and m.name or ""
	end
	fns.getfellowshiplocked = function()
		local f = fellowship()
		return f ~= nil and f.locked == true
	end
	fns.getfellowshipisleader = function()
		local f = fellowship()
		return f ~= nil and f.leader == api.game.player.id
	end
	fns.getfellowshipisopen = function()
		local f = fellowship()
		return f ~= nil and f.open == true
	end
	fns.getfellowshipisfull = function()
		local f = fellowship()
		return f ~= nil and #f.members >= 9
	end
	fns.getfellowshipcanrecruit = function()
		local f = fellowship()
		return f ~= nil and #f.members < 9 and (f.open == true or f.leader == api.game.player.id)
	end
	fns.getfellownames = function()
		local out = {}
		for _, m in ipairs(members_sorted("name")) do
			table.insert(out, m.name)
		end
		return list_of(out)
	end
	fns.getfellowids = function()
		local out = {}
		for _, m in ipairs(members_sorted("id")) do
			table.insert(out, m.id)
		end
		return list_of(out)
	end

	-- A status HUD: lines of key and value -------------------------------------------------------

	local status = { keys = {}, values = {} }
	local function show_status()
		local rows = {}
		for _, k in ipairs(status.keys) do
			table.insert(rows, { k, str(status.values[k]) })
		end
		api.ui.hud({ id = "expr:status", anchor = "top_right",
			api.ui.list({ id = "rows", columns = { "", "" }, rows = rows, height = math.max(1, #rows) }) })
	end
	fns.statushud = function(_, key, value)
		local k = str(key)
		if status.values[k] == nil then
			table.insert(status.keys, k)
		end
		status.values[k] = value
		show_status()
		return true
	end
	fns.statushudcolored = function(env, key, value)
		return fns.statushud(env, key, value)
	end

	-- Meta views --------------------------------------------------------------------------------

	local function view_of(env, name)
		local m = env.meta
		return m and m.views and m.views[str(name)]
	end
	fns.uiviewexists = function(env, name)
		return view_of(env, name) ~= nil
	end
	fns.uiviewvisible = function(env, name)
		return view_of(env, name) ~= nil
	end
	fns.uigetcontrol = function(env, name, control)
		local v = view_of(env, name)
		if v == nil then
			return 0
		end
		for node, c in pairs(v.controls) do
			if c.name == str(control) then
				return setmetatable({ view = str(name), node = node }, X.control_meta)
			end
		end
		return 0
	end
	fns.uisetlabel = function(env, c, text)
		if getmetatable(c) ~= X.control_meta then
			return false
		end
		local v = view_of(env, c.view)
		if v == nil then
			return false
		end
		v.window:get(c.node).text = str(text)
		return true
	end

	-- The macro's own state: VTank's and UtilityBelt's settings, the meta's state -----------------

	fns.vtgetmetastate = function(env)
		return env.meta and env.meta.state or ""
	end
	fns.vtsetmetastate = function(env, name)
		local m, s = env.meta, str(name)
		if m == nil or m.states[s] == nil then
			return false
		end
		m:set_state(s)
		return true
	end
	fns.vtgetsetting = function(env, name)
		local m = env.meta
		local v = m and m.options[string.lower(str(name))]
		-- Driving an engine, the engine's own setting (what a meta set reaches it too).
		local e = m and m.engine
		if e ~= nil and e.option ~= nil then
			local ev = e.option(str(name))
			if ev ~= nil then
				v = ev
			end
		end
		if v == nil then
			return 0
		end
		return v
	end
	fns.vtsetsetting = function(env, name, value)
		if env.meta == nil then
			return false
		end
		env.meta:set_option(str(name), value)
		return true
	end
	fns.uboptget = function(env, name)
		local m = env.meta
		local v = m and m.options["ub:" .. string.lower(str(name))]
		if v == nil then
			return 0
		end
		return v
	end
	fns.uboptset = function(env, name, value)
		if env.meta == nil then
			return false
		end
		env.meta:set_option("ub:" .. str(name), value)
		return true
	end

	-- The API -------------------------------------------------------------------------------------

	-- `expr.compile(text)`: a function of an environment (see `expr.env`) answering the value.
	-- Raises when the text does not parse, or names a function this host does not support.
	X.compile = compile

	-- An environment over plain tables: `vars` (this run's), `pvars` (kept per character) and
	-- `gvars` (kept on this machine), with hooks the meta fills in.
	function X.env(t)
		t = t or {}
		local stores = { vars = t.vars or {}, pvars = t.pvars or {}, gvars = t.gvars or {} }
		local e = {}
		function e.get(scope, name)
			return stores[scope][name]
		end
		function e.set(scope, name, v)
			stores[scope][name] = v
			if t.on_set then
				t.on_set(scope, name, v)
			end
		end
		function e.clear(scope)
			local s = stores[scope]
			local mt = getmetatable(s)
			if mt ~= nil and mt.__clear ~= nil then
				mt.__clear()
			else
				table.clear(s)
			end
		end
		e.stores = stores
		return e
	end

	-- `expr.eval(text, env)`: compile and run once.
	function X.eval(text, env)
		return compile(text)(env or X.env())
	end

	return X
end

-- `dereth.combat`: helpers for the parts of a fight that repeat, healing and debuffing, in the
-- manner of VTank's.
builtin["dereth.combat"] = function()
	local C = {}

	local function fraction(vital)
		local x = api.game.player[vital]
		if x == nil or x.max == nil or x.max == 0 then
			return nil
		end
		return x.current / x.max
	end

	local function known(id)
		local s = api.magic.spell(id)
		return s ~= nil and s.known == true
	end

	local MAGIC = 8

	-- The weapon held (melee, missile other than ammunition, or a caster), or nil.
	local function held_weapon()
		local me = api.game.player.id
		for _, it in ipairs(api.game.inventory.items()) do
			local n = string.lower(it.name or "")
			local ammo = n:find("arrow") or n:find("bolt") or n:find("quarrel") or n:find("dart")
			if it.wielder == me and (it.class == "MeleeWeapon" or it.class == "WandStaffOrb"
				or (it.class == "MissileWeapon" and not ammo)) then
				return it
			end
		end
		return nil
	end

	-- `combat.take_up(item [, opts])`, inside a task: wield `item` (an object, or a name in the
	-- pack) in place of the weapon held, in peace stance; `opts.purpose` (`buff`, `heal` or
	-- `support`) is a caster's, for the spells of that domain. Answers true, or false and why.
	function C.take_up(item, opts)
		local purpose = opts and { purpose = opts.purpose } or nil
		if type(item) == "string" then
			local want = string.lower(item)
			local found = nil
			for _, it in ipairs(api.game.inventory.items()) do
				if string.lower(it.name or "") == want then
					found = it
				end
			end
			if found == nil then
				return false, "not_held"
			end
			item = found
		end
		local me = api.game.player
		if item.wielder == me.id then
			return true
		end
		if (me.combat_mode or 1) > 1 then
			local ok, why = pawait(api.combat.set_mode("peace", purpose))
			if not ok then
				return false, why
			end
		end
		local held = held_weapon()
		if held ~= nil then
			local ok, why = pawait(api.items.unwield(held.id, purpose))
			if not ok then
				return false, why
			end
		end
		return pawait(api.items.wield(item.id, purpose))
	end

	-- `combat.ready_magic{ caster = name, purpose = domain }`, inside a task: into magic stance,
	-- where spells are cast. A caster already held is kept; otherwise the one named (else the first
	-- in the pack) is taken up in place of the weapon held, and the combat stance entered; with a
	-- purpose (`buff`, `heal` or `support`), each step is that domain's. Answers true and the
	-- weapon put away (nil when none was), or false and why (`no_caster`, or the failed step's).
	function C.ready_magic(opts)
		opts = opts or {}
		local me = api.game.player
		if me.combat_mode == MAGIC then
			return true
		end
		local held = held_weapon()
		local caster = nil
		local want = opts.caster and string.lower(opts.caster) or nil
		if held ~= nil and held.class == "WandStaffOrb" and (want == nil or string.lower(held.name or "") == want) then
			caster = held
		end
		for _, it in ipairs(api.game.inventory.items()) do
			if caster == nil and it.class == "WandStaffOrb" and (want == nil or string.lower(it.name or "") == want) then
				caster = it
			end
		end
		if caster == nil then
			return false, "no_caster"
		end
		local put_away = nil
		local purpose = opts.purpose and { purpose = opts.purpose } or nil
		if caster.wielder ~= me.id then
			local ok, why = C.take_up(caster, purpose)
			if not ok then
				return false, why
			end
			put_away = held
		end
		local ok, why = pawait(api.combat.set_mode("combat", purpose))
		if not ok then
			return false, why
		end
		if api.game.player.combat_mode ~= MAGIC then
			return false, "not_in_magic_mode"
		end
		return true, put_away
	end

	-- `combat.heal{ vital = "health", below = 0.5, spells = { id, … }, kits = { name, … } }`: when
	-- the vital is at or below `below` of its maximum, cast the first of `spells` the character
	-- knows on itself as a heal, or else use the first of `kits` it holds on itself. Answers the
	-- action's promise, or nil when the vital is above the mark or nothing can heal it.
	function C.heal(opts)
		opts = opts or {}
		local f = fraction(opts.vital or "health")
		if f == nil or f > (opts.below or 0.5) then
			return nil
		end
		for _, id in ipairs(opts.spells or {}) do
			if known(id) then
				return api.magic.cast(id, nil, { purpose = "heal" })
			end
		end
		for _, name in ipairs(opts.kits or {}) do
			for _, it in ipairs(api.game.inventory.items()) do
				if it.name == name then
					return api.act.use_on(it.id, api.game.player.id)
				end
			end
		end
		return nil
	end

	local debuffer_methods = {}
	local debuffer_meta = { __index = debuffer_methods }

	-- `combat.debuffer{ spells = { id, … }, recast = 60 }`: a debuffer remembers what it cast on
	-- which target. `d:next(target)` casts, on the target, the first of `spells` the character knows
	-- and has not landed on it in the last `recast` seconds (a spell's own `recast` may be given as
	-- `{ id = …, recast = … }`), and answers the cast's promise; nil when all are up. A cast that
	-- fails is not remembered, so the next call tries it again.
	function C.debuffer(opts)
		opts = opts or {}
		return setmetatable({ spells = opts.spells or {}, recast = opts.recast or 60, landed = {} },
			debuffer_meta)
	end

	function debuffer_methods:next(target)
		local tid = type(target) == "table" and target.id or target
		if tid == nil or api.game.world:get(tid) == nil then
			self.landed[tid or 0] = nil
			return nil
		end
		local now = api.os.clock()
		local on = self.landed[tid] or {}
		self.landed[tid] = on
		for _, s in ipairs(self.spells) do
			local id = type(s) == "table" and s.id or s
			local recast = type(s) == "table" and s.recast or self.recast
			if known(id) and (on[id] == nil or now - on[id] >= recast) then
				local p = api.magic.cast(id, tid)
				p:next(function()
					on[id] = api.os.clock()
				end)
				return p
			end
		end
		return nil
	end

	-- Forget a target (it died or left), or every target.
	function debuffer_methods:forget(target)
		if target == nil then
			self.landed = {}
		else
			self.landed[type(target) == "table" and target.id or target] = nil
		end
	end

	return C
end

-- `require` resolves the host's libraries, then only inside the plugin's own package.
function api.require(name)
	assert(type(name) == "string", "require(name): name must be a string")
	local loaded = modules[name]
	if loaded ~= nil then
		return loaded
	end
	if builtin[name] ~= nil then
		loaded = builtin[name]()
		modules[name] = loaded
		return loaded
	end
	-- The host's libraries written in Lua, compiled the first time one is named; each is handed
	-- the compiler, for the modules of the package it runs, and the count of entries into the VM.
	local library = libraries[name]
	if library ~= nil then
		loaded = compile(name, library)(name, compile, entry)
		if loaded == nil then
			loaded = true
		end
		modules[name] = loaded
		return loaded
	end
	local source = hostcall("module.source", name)
	if source == nil then
		error("module " .. name .. " is not in this plugin's package", 2)
	end
	local fn = compile(name, source)
	local value = fn(name)
	if value == nil then
		value = true
	end
	modules[name] = value
	return value
end

-- The dispatcher ---------------------------------------------------------------------------------

-- The message of an error raised by a host call is not a string; the traceback needs one.
local function handler(err)
	return traceback(tostring(err), 2)
end

-- Runs every handler of one event; answers the first failure's message, or nil.
local function dispatch(name, data)
	local first_error = nil
	local function call(fn, ...)
		local ok, err = xpcall(fn, handler, ...)
		if not ok and first_error == nil then
			first_error = tostring(err)
		end
	end
	if name == "task.timer" then
		local t = timers[data.id]
		if t ~= nil then
			if not t.again then
				timers[data.id] = nil
			end
			if t.co ~= nil then
				resume(t.co)
			else
				call(t.fn)
			end
		end
		return first_error
	elseif name == "command" then
		local fn = commands[data.name]
		if fn ~= nil then
			call(fn, data.args, data.words)
		end
		return first_error
	elseif name == "service.call" then
		local fn = services[data.service]
		if fn == nil then
			hostcall("services.reply", data.id, nil, "no_service")
			return nil
		end
		local ok, v = pcall(fn, data.args, { from = data.from, gesture = data.gesture })
		if ok then
			hostcall("services.reply", data.id, v, nil)
		else
			hostcall("services.reply", data.id, nil, tostring(v))
		end
		return nil
	elseif name == "action.completed" or name == "action.failed" then
		local p = promises[data.id]
		if p ~= nil then
			promises[data.id] = nil
			if name == "action.completed" then
				p.state, p.value = "completed", data.value
				if p.transform ~= nil then
					p.value = p.transform(p.value)
				end
			else
				p.state, p.reason = "failed", data.reason
			end
			for _, cb in ipairs(p.callbacks) do
				local f = cb[1]
				local arg = p.value
				if p.state == "failed" then
					f, arg = cb[2], p.reason
				end
				if f ~= nil then
					call(f, arg)
				end
			end
			for _, co in ipairs(p.waiters) do
				resume(co)
			end
		end
	end
	local list = handlers[name]
	if list ~= nil then
		-- A handler may remove itself or another; the copy keeps the walk stable.
		local copy = table.clone(list)
		for _, fn in ipairs(copy) do
			call(fn, data)
		end
	end
	return first_error
end

return api, dispatch, new_entry
