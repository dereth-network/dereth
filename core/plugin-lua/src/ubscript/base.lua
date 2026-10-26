-- `dereth.ubscript`: scripts written for UtilityBelt's scripting, run over the plugin interface.
--
-- A script sees UtilityBelt's globals (`game`, `await`, `sleep`, the enumerations, the action
-- classes, `json` and the rest) in place of the plugin interface's, and each thing it asks of the
-- game goes through the plugin interface: its actions are the plugin's actions, held to the
-- world's policy as any other. What the plugin interface cannot do is refused with an error that
-- says so. `run(entry)` runs the script: when a character enters the world (or at once, for a
-- plugin active before the world), and again, fresh, at each entry after the script ended with a
-- log-off.

local _, compile, entry = ...

-- The plugin interface, as the library reaches it; a script's globals shadow these names.
local D = {
	game = game,
	act = act,
	items = items,
	vendor = vendor,
	trade = trade,
	social = social,
	character = character,
	magic = magic,
	combat = combat,
	nav = nav,
	session = session,
	chat = chat,
	commands = commands,
	task = task,
	on = on,
	off = off,
	pawait = pawait,
	print = print,
	ui = ui,
	markers = markers,
	draw = draw,
	messages = messages,
	peers = peers,
	net = net,
	files = files,
	store = store,
	host = host,
	regex = regex,
	services = services,
	os = os,
	require = require,
}

local U = { D = D, compile = compile }

-- What the library read of the game in this entry into the VM: the game is the frame's for the
-- whole entry, so a table built once (an object's properties, the character's skills) serves
-- every read of it until the next.
local kept, kept_entry = {}, -1
local function keep(key, make)
	local e = entry()
	if e ~= kept_entry then
		kept, kept_entry = {}, e
	end
	local v = kept[key]
	if v == nil then
		v = make()
		kept[key] = v
	end
	return v
end
U.keep = keep

-- Names ------------------------------------------------------------------------------------------

-- A member is found whatever the case of its name and its underscores, as UtilityBelt's script
-- engine finds one (`GetLandscape`, `getLandscape` and `get_landscape` are one member).
local normcache = {}
local function norm(k)
	local n = normcache[k]
	if n == nil then
		n = string.lower((string.gsub(k, "_", "")))
		normcache[k] = n
	end
	return n
end
U.norm = norm

-- The key under which an object keeps its own state; a script cannot name it.
local S = {}
U.S = S

local function refusal(what, why)
	return string.format("%s is not available here: %s", what, why)
end
U.refusal = refusal

-- Classes ----------------------------------------------------------------------------------------

-- Every class, by name, for the library's own checks of what it answers.
U.classes = {}

-- A class of objects a script reads: getters, setters and methods by name, called with `.` or `:`
-- alike (a method called with `:` is handed the object twice; the first is dropped).
local function class(name)
	local c = { name = name, get = {}, set = {}, call = {}, refused = {} }
	U.classes[name] = c
	function c.define(t)
		for k, f in pairs(t.get or {}) do
			c.get[norm(k)] = f
		end
		for k, f in pairs(t.set or {}) do
			c.set[norm(k)] = f
		end
		for k, f in pairs(t.call or {}) do
			c.call[norm(k)] = f
		end
		for k, why in pairs(t.refused or {}) do
			c.refused[norm(k)] = why
		end
		return c
	end
	c.meta = {
		__index = function(obj, key)
			if type(key) ~= "string" then
				if c.index ~= nil then
					return c.index(obj, key)
				end
				return nil
			end
			local n = norm(key)
			local g = c.get[n]
			if g ~= nil then
				return g(obj)
			end
			local m = c.call[n]
			if m ~= nil then
				local st = rawget(obj, S)
				local bound = st.bound
				if bound == nil then
					bound = {}
					st.bound = bound
				end
				local b = bound[n]
				if b == nil then
					b = function(first, ...)
						if rawequal(first, obj) then
							return m(obj, ...)
						end
						return m(obj, first, ...)
					end
					bound[n] = b
				end
				return b
			end
			local why = c.refused[n]
			if why ~= nil then
				error(refusal(name .. "." .. key, why), 2)
			end
			if c.fallback ~= nil then
				local found, v = c.fallback(obj, n)
				if found then
					return v
				end
			end
			if n == "getpropertykeys" then
				return function()
					local keys = {}
					for k in pairs(c.get) do
						table.insert(keys, k)
					end
					table.sort(keys)
					return U.list(keys)
				end
			end
			error(string.format("%s has no member %s", name, key), 2)
		end,
		__newindex = function(obj, key, value)
			local f = type(key) == "string" and c.set[norm(key)] or nil
			if f ~= nil then
				f(obj, value)
				return
			end
			error(string.format("%s.%s cannot be set", name, tostring(key)), 2)
		end,
		__tostring = function(obj)
			if c.tostring ~= nil then
				return c.tostring(obj)
			end
			return name
		end,
	}
	-- A new object of the class, with its state.
	function c.new(state)
		state = state or {}
		return setmetatable({ [S] = state }, c.meta)
	end
	return c
end
U.class = class

local function state(obj)
	return rawget(obj, S)
end
U.state = state

-- Whether `v` is an object of class `c`.
local function is(v, c)
	return type(v) == "table" and getmetatable(v) == c.meta
end
U.is = is

-- Collections ------------------------------------------------------------------------------------

-- A list as a script reads one: indexed from 1, `#` and `ipairs` as a table, and `for v in list`
-- walking its values, as UtilityBelt's lists are walked; `Count` its length.
local LIST = {}
LIST.__iter = function(t)
	local i = 0
	return function()
		i = i + 1
		return rawget(t, i)
	end
end
LIST.__index = function(t, k)
	if type(k) == "string" then
		local n = norm(k)
		if n == "count" or n == "length" then
			return #t
		elseif n == "contains" then
			return function(a, b)
				local v = b
				if not rawequal(a, t) then
					v = a
				end
				for _, x in ipairs(t) do
					if x == v then
						return true
					end
				end
				return false
			end
		elseif n == "toarray" or n == "tolist" then
			return function()
				return t
			end
		end
	end
	return nil
end
local function list(t)
	return setmetatable(t or {}, LIST)
end
U.list = list

-- A dictionary: keys of a typed enumeration (or numbers), `pairs` giving each, and an index by
-- the enumeration's value or by its number alike; `Count`, `ContainsKey`, `Keys`, `Values`.
local function dict(t, keyof)
	return setmetatable(t, {
		__index = function(d, k)
			if keyof ~= nil and type(k) == "number" then
				local tk = keyof(k)
				if tk ~= nil then
					return rawget(d, tk)
				end
			end
			if type(k) == "string" then
				local n = norm(k)
				if n == "count" then
					local c = 0
					for _ in pairs(d) do
						c = c + 1
					end
					return c
				elseif n == "containskey" then
					return function(a, b)
						local key = b
						if not rawequal(a, d) then
							key = a
						end
						return d[key] ~= nil
					end
				elseif n == "keys" or n == "values" then
					local out = {}
					for kk, v in pairs(d) do
						table.insert(out, n == "keys" and kk or v)
					end
					return list(out)
				end
			end
			return nil
		end,
	})
end
U.dict = dict

-- Enumerations -------------------------------------------------------------------------------------

local ENUMS = require("dereth.ubscript.enums")

-- The enumerations whose values the script API hands out typed, as UtilityBelt's are: printed by
-- name, compared with each other, and telling which kind of property or class an argument names
-- (`Value(IntId.Level)` reads an integer, `Value(StringId.Name)` a string). Every other
-- enumeration's values are plain numbers, compared with the numbers properties hold.
local TYPED = {
	IntId = true,
	Int64Id = true,
	BoolId = true,
	FloatId = true,
	StringId = true,
	DataId = true,
	InstanceId = true,
	ObjectClass = true,
	SkillId = true,
	AttributeId = true,
	VitalId = true,
	ActionError = true,
	ActionType = true,
	ClientState = true,
	CombatMode = true,
	ChatMessageType = true,
	ChatChannel = true,
	AddRemoveEventType = true,
	MotionStance = true,
	SkillTrainingType = true,
	MagicSchool = true,
	DistanceType = true,
	UseActionType = true,
	CastEventType = true,
	FellowshipEventType = true,
	ClientConfirmationType = true,
	EndTradeReason = true,
	TradeSide = true,
}

local ENUMV = {}
local enum_tables = {}
local enum_info = {}

-- An enumeration's names and values, read the first time it is named: each name's value as
-- written, and the names in declaration order.
local function info(name)
	local i = enum_info[name]
	if i ~= nil then
		return i
	end
	local make = ENUMS[name]
	if make == nil then
		return nil
	end
	local flat = make()
	local exact, order = {}, {}
	for at = 1, #flat, 2 do
		local k, n = flat[at], flat[at + 1]
		if exact[k] == nil then
			exact[k] = n
		end
		order[#order + 1] = { k, n }
	end
	i = { name = name, exact = exact, order = order, typed = TYPED[name] == true, objects = {} }
	enum_info[name] = i
	return i
end

-- Its names without case, and the first name of each value, made the first time one is needed.
local function parsed(i)
	if i.byname == nil then
		i.byname, i.byvalue = {}, {}
		for _, kv in ipairs(i.order) do
			i.byname[norm(kv[1])] = kv
			if i.byvalue[kv[2]] == nil then
				i.byvalue[kv[2]] = kv[1]
			end
		end
	end
	return i
end

-- The value of one name: as written, or else whatever its case.
local function lookup(i, key)
	local v = i.exact[key]
	if v ~= nil then
		return v
	end
	local kv = parsed(i).byname[norm(key)]
	if kv ~= nil then
		return kv[2]
	end
	return nil
end

-- The typed value of enumeration `i` for number `n`: one object for each value, so `==` and table
-- keys hold.
local function typed(i, n)
	local o = i.objects[n]
	if o == nil then
		o = setmetatable({ [S] = { enum = i, value = n } }, ENUMV)
		i.objects[n] = o
	end
	return o
end

-- `v` as a number, whether it is a typed value or a plain one.
local function num(v)
	if type(v) == "table" then
		local m = getmetatable(v)
		if m == ENUMV then
			return rawget(v, S).value
		end
	end
	return v
end
U.num = num

-- The enumeration a typed value belongs to, by name, or nil.
local function enum_of(v)
	if type(v) == "table" and getmetatable(v) == ENUMV then
		return rawget(v, S).enum.name
	end
	return nil
end
U.enum_of = enum_of

-- A value of enumeration `name` from a number: the typed value for a typed enumeration, else the
-- number.
local function ev(name, n)
	if n == nil then
		return nil
	end
	local i = info(name)
	if i == nil or not i.typed then
		return n
	end
	return typed(i, n)
end
U.ev = ev

local function enum_name(i, n)
	local direct = parsed(i).byvalue[n]
	if direct ~= nil then
		return direct
	end
	-- A combination of flags, as .NET names one: its flags' names joined by ", ".
	local parts, rest = {}, n
	for _, kv in ipairs(i.order) do
		local f = kv[2]
		if f > 0 and bit32.band(rest, f) == f then
			table.insert(parts, kv[1])
			rest = bit32.band(rest, bit32.bnot(f))
		end
	end
	if #parts > 0 and rest == 0 then
		return table.concat(parts, ", ")
	end
	return tostring(n)
end
U.enum_name = function(name, n)
	local i = info(name)
	if i == nil then
		return tostring(n)
	end
	return enum_name(i, num(n))
end

local function band(a, b)
	return bit32.band(a % 4294967296, b % 4294967296)
end
local function bor(a, b)
	return bit32.bor(a % 4294967296, b % 4294967296)
end

local function same_enum_number(a, b)
	if type(a) == "table" and type(b) == "table" then
		local sa, sb = rawget(a, S), rawget(b, S)
		return sa.enum == sb.enum and sa.value == sb.value
	end
	return false
end

local enum_methods = {
	tonumber = function(v)
		return rawget(v, S).value
	end,
	hasflags = function(v, ...)
		local n = rawget(v, S).value
		for _, f in ipairs({ ... }) do
			local fv = num(f)
			if band(n, fv) ~= fv then
				return false
			end
		end
		return true
	end,
	addflags = function(v, ...)
		local s = rawget(v, S)
		local n = s.value
		for _, f in ipairs({ ... }) do
			n = bor(n, num(f))
		end
		return typed(s.enum, n)
	end,
	removeflags = function(v, ...)
		local s = rawget(v, S)
		local n = s.value
		for _, f in ipairs({ ... }) do
			n = band(n, bit32.bnot(num(f) % 4294967296))
		end
		return typed(s.enum, n)
	end,
	tostring = function(v)
		local s = rawget(v, S)
		return enum_name(s.enum, s.value)
	end,
	equals = function(v, o)
		return num(v) == num(o)
	end,
}
enum_methods.hasflag = enum_methods.hasflags

ENUMV.__index = function(v, key)
	if type(key) ~= "string" then
		return nil
	end
	local f = enum_methods[norm(key)]
	if f == nil then
		error("an enumeration value has no member " .. key, 2)
	end
	return function(first, ...)
		if rawequal(first, v) then
			return f(v, ...)
		end
		return f(v, first, ...)
	end
end
ENUMV.__newindex = function()
	error("an enumeration value cannot be changed", 2)
end
ENUMV.__tostring = function(v)
	local s = rawget(v, S)
	return enum_name(s.enum, s.value)
end
ENUMV.__eq = same_enum_number
ENUMV.__lt = function(a, b)
	return num(a) < num(b)
end
ENUMV.__le = function(a, b)
	return num(a) <= num(b)
end
-- `+` combines flags, as UtilityBelt's enumerations do; `..` with text joins it to the value's
-- name.
ENUMV.__add = function(a, b)
	local s = type(a) == "table" and rawget(a, S) or rawget(b, S)
	return typed(s.enum, bor(num(a), num(b)))
end
ENUMV.__sub = function(a, b)
	local s = type(a) == "table" and rawget(a, S) or rawget(b, S)
	return typed(s.enum, band(num(a), bit32.bnot(num(b) % 4294967296)))
end
ENUMV.__concat = function(a, b)
	if type(a) == "string" or type(b) == "string" or type(a) == "number" or type(b) == "number" then
		return tostring(a) .. tostring(b)
	end
	local s = rawget(a, S)
	return typed(s.enum, bor(num(a), num(b)))
end

-- The table a script names an enumeration by: `IntId.Level`, `IntId.GetValues()`,
-- `IntId.FromValue(25)`.
local function enum(name)
	local t = enum_tables[name]
	if t ~= nil then
		return t
	end
	local i = info(name)
	if i == nil then
		return nil
	end
	local function value_of(n)
		if i.typed then
			return typed(i, n)
		end
		return n
	end
	t = setmetatable({}, {
		__index = function(_, key)
			if type(key) == "number" then
				return value_of(key)
			end
			if type(key) ~= "string" then
				return nil
			end
			local v = lookup(i, key)
			if v ~= nil then
				return value_of(v)
			end
			local n = norm(key)
			if n == "getvalues" then
				return function()
					local out = {}
					for _, e in ipairs(parsed(i).order) do
						table.insert(out, value_of(e[2]))
					end
					return list(out)
				end
			elseif n == "getnames" then
				return function()
					local out = {}
					for _, e in ipairs(parsed(i).order) do
						table.insert(out, e[1])
					end
					return list(out)
				end
			elseif n == "fromvalue" then
				return function(a, b)
					return value_of(num(b ~= nil and b or a))
				end
			elseif n == "getname" or n == "tostring" then
				return function(a, b)
					return enum_name(i, num(b ~= nil and b or a))
				end
			end
			error(name .. " has no member " .. key, 2)
		end,
		__newindex = function()
			error(name .. " is read-only", 2)
		end,
		__tostring = function()
			return name
		end,
	})
	enum_tables[name] = t
	return t
end
U.enum = enum
U.enum_names = ENUMS

-- Values of plain enumerations by name, for the library's own use: `E.ObjectType.Creature`.
U.E = setmetatable({}, {
	__index = function(_, name)
		return enum(name)
	end,
})

-- Ids ----------------------------------------------------------------------------------------------

-- An object id from what a script passes for one: a number, a world object, or nil.
local function id_of(v)
	if v == nil then
		return nil
	end
	if type(v) == "number" then
		return v
	end
	if type(v) == "table" then
		local s = rawget(v, S)
		if s ~= nil and s.id ~= nil then
			return s.id
		end
		if getmetatable(v) == ENUMV then
			return rawget(v, S).value
		end
	end
	error("an object id or a world object is needed, not " .. tostring(v), 3)
end
U.id_of = id_of

-- Events -----------------------------------------------------------------------------------------

-- A script's events: `Add(fn)`, `Remove(fn)`, `Once(fn)` (heard once) and `Until(fn)` (heard
-- until `fn` answers true). Each handler runs as a coroutine of its own, so it may `await` and
-- `sleep`. An event that begins listening to the game only when its first handler is added names
-- `start`.
local Event = class("Event")
U.Event = Event

local run -- the script running now, or nil

local function add_handler(ev, fn, mode)
	assert(type(fn) == "function", "an event's handler must be a function")
	local st = state(ev)
	if st.refused ~= nil then
		error(refusal(st.name, st.refused), 3)
	end
	for _, h in ipairs(st.handlers) do
		if h.fn == fn then
			return
		end
	end
	table.insert(st.handlers, { fn = fn, mode = mode })
	if not st.started and st.start ~= nil then
		st.started = true
		st.start()
	end
end

Event.define({
	call = {
		Add = function(ev, fn)
			add_handler(ev, fn, "add")
		end,
		Once = function(ev, fn)
			add_handler(ev, fn, "once")
		end,
		Until = function(ev, fn)
			add_handler(ev, fn, "until")
		end,
		Remove = function(ev, fn)
			local st = state(ev)
			for i, h in ipairs(st.handlers) do
				if h.fn == fn then
					table.remove(st.handlers, i)
					return
				end
			end
		end,
	},
	get = {
		Count = function(ev)
			return #state(ev).handlers
		end,
	},
})
Event.tostring = function(ev)
	return "Event " .. tostring(state(ev).name)
end

-- A new event, `name` for messages; `start` runs when its first handler is added; `refused` makes
-- adding a handler an error that says why.
local function event(name, start, refused)
	return Event.new({ name = name, handlers = {}, start = start, refused = refused })
end
U.event = event

-- Running script code ----------------------------------------------------------------------------

local first_error = nil

-- Runs `fn(...)` as a coroutine of the script's; answers whether it ran without error, and what
-- it first answered (the value of a handler that finished without waiting). A failure is kept, and
-- raised once the host's event has been handed to every handler ([`U.raise`]).
local function spawn(fn, ...)
	local co = coroutine.create(fn)
	local ok, v = coroutine.resume(co, ...)
	if not ok then
		if first_error == nil then
			first_error = tostring(v)
		end
		return false, nil
	end
	if coroutine.status(co) == "dead" then
		return true, v, true
	end
	return true, nil, false
end
U.spawn = spawn

-- Raise the first failure of the script's code since the last call: the host counts it, as it
-- counts a plugin's.
local function raise()
	local e = first_error
	first_error = nil
	if e ~= nil then
		error(e, 0)
	end
end
U.raise = raise

-- Resume a coroutine of the script's that waits.
local function resume(co, ...)
	local ok, err = coroutine.resume(co, ...)
	if not ok and first_error == nil then
		first_error = tostring(err)
	end
end
U.resume = resume

-- Hand `arg` to every handler of `ev`, each as its own coroutine.
local function fire(ev, arg)
	local st = state(ev)
	if #st.handlers == 0 then
		return
	end
	for _, h in ipairs(table.clone(st.handlers)) do
		local ok, v, done = spawn(h.fn, arg)
		if h.mode == "once" or (h.mode == "until" and ok and done and v == true) then
			for i, x in ipairs(st.handlers) do
				if x == h then
					table.remove(st.handlers, i)
					break
				end
			end
		end
	end
end
U.fire = fire

-- Listen to a plugin-interface event for the script that runs now; it ends with the script.
local function listen(name, fn)
	local r = run
	local wrapped = function(e)
		if run ~= r then
			return
		end
		fn(e)
		raise()
	end
	D.on(name, wrapped)
	table.insert(r.listeners, { name, wrapped })
	return wrapped
end
U.listen = listen

-- Every `ms` milliseconds while the script runs.
local function every(ms, fn)
	local r = run
	local id
	id = D.task.every(ms, function()
		if run ~= r then
			D.task.cancel(id)
			return
		end
		fn()
		raise()
	end)
	table.insert(r.timers, id)
	return id
end
U.every = every

-- Things to do when the script ends.
local function at_end(fn)
	table.insert(run.ends, fn)
end
U.at_end = at_end

function U.current()
	return run
end

-- Waiting -----------------------------------------------------------------------------------------

-- Wait inside a script coroutine until `ready()` is true, looking again every `ms`.
local function wait_until(ready, ms)
	while not ready() do
		D.task.sleep(ms or 50)
	end
end
U.wait_until = wait_until

local function sleep(ms)
	ms = tonumber(num(ms)) or 0
	if not coroutine.isyieldable() then
		error("sleep can only be called from a script's own code, which runs as a coroutine", 2)
	end
	D.task.sleep(math.max(50, math.floor(ms)))
end
U.sleep = sleep

-- Time ---------------------------------------------------------------------------------------------

-- UtilityBelt's times are .NET dates; here a date is a Unix time in seconds, counted on the host's
-- frame clock from the game's time when the script first asked.
local clock0, unix0
local function now()
	local c = D.os.clock()
	if clock0 == nil then
		clock0, unix0 = c, D.os.time()
	end
	return unix0 + (c - clock0)
end
U.now = now

-- Positions and distances ------------------------------------------------------------------------

-- A place in meters, as the plugin interface gives one: { cell, x, y, z, ns, ew }. Two places in
-- different landblocks are measured by their landblocks' offsets (192 meters a landblock).
local function global_xyz(p)
	if p == nil then
		return nil
	end
	local cell = p.cell or 0
	local lbx = math.floor(cell / 16777216) % 256
	local lby = math.floor(cell / 65536) % 256
	local indoors = (cell % 65536) >= 256
	if indoors then
		return (p.x or 0), (p.y or 0), (p.z or 0), cell - cell % 65536
	end
	return lbx * 192 + (p.x or 0), lby * 192 + (p.y or 0), (p.z or 0), nil
end

local function distance(a, b, flat)
	if a == nil or b == nil then
		return math.huge
	end
	-- Map coordinates alone: a map unit is 240 meters.
	if (a.cell == nil or b.cell == nil) and a.ns ~= nil and b.ns ~= nil then
		local dx, dy, dz = (a.ew - b.ew) * 240, (a.ns - b.ns) * 240, (a.z or 0) - (b.z or 0)
		if flat then
			return math.sqrt(dx * dx + dy * dy)
		end
		return math.sqrt(dx * dx + dy * dy + dz * dz)
	end
	local ax, ay, az, ablock = global_xyz(a)
	local bx, by, bz, bblock = global_xyz(b)
	if ablock ~= bblock then
		if ablock ~= nil and bblock ~= nil then
			return math.huge
		end
		if a.ns ~= nil and b.ns ~= nil then
			local dx, dy = (a.ew - b.ew) * 240, (a.ns - b.ns) * 240
			return math.sqrt(dx * dx + dy * dy)
		end
		return math.huge
	end
	local dx, dy, dz = ax - bx, ay - by, az - bz
	if flat then
		return math.sqrt(dx * dx + dy * dy)
	end
	return math.sqrt(dx * dx + dy * dy + dz * dz)
end
U.distance = distance

-- World objects ------------------------------------------------------------------------------------

local WorldObject = class("WorldObject")
U.WorldObject = WorldObject
local wo_cache = setmetatable({}, { __mode = "v" })

-- The world object with id `id` as a script reads it, or nil when the client does not know it.
local function wo(id)
	if id == nil or id == 0 then
		return nil
	end
	local o = wo_cache[id]
	if o ~= nil then
		return o
	end
	if not D.game.world:get(id) then
		return nil
	end
	o = WorldObject.new({ id = id, h = D.game.world:get(id) })
	wo_cache[id] = o
	return o
end
U.wo = wo

local function handle(o)
	return state(o).h
end

local function list_of_ids(ids)
	local out = {}
	for _, id in ipairs(ids) do
		local o = wo(id)
		if o ~= nil then
			table.insert(out, o)
		end
	end
	return list(out)
end
U.list_of_ids = list_of_ids

local function objects(handles)
	local out = {}
	for _, h in ipairs(handles) do
		local o = wo(h.id)
		if o ~= nil then
			table.insert(out, o)
		end
	end
	return list(out)
end
U.objects = objects

-- What the client knows of an object's properties of one type, by number.
local function props(id, ty)
	return keep("props:" .. tostring(id) .. ":" .. tostring(ty), function()
		local h = D.game.world:get(id)
		return h and h:properties(ty) or {}
	end)
end
U.props = props

local PROP_ENUMS = {
	IntId = "int",
	Int64Id = "int64",
	FloatId = "float",
	StringId = "string",
	BoolId = "bool",
	DataId = "did",
	InstanceId = "iid",
}
local PROP_DEFAULTS = { int = 0, int64 = 0, float = 0, string = "", bool = false, did = 0, iid = 0 }

-- A property table as a script reads one: keyed by the typed id, also found by its number.
local function prop_table(id, ename)
	return keep("table:" .. tostring(id) .. ":" .. ename, function()
		local ty = PROP_ENUMS[ename]
		local i = info(ename)
		local out = {}
		for k, v in pairs(props(id, ty)) do
			out[typed(i, k)] = v
		end
		return dict(out, function(n)
			return typed(i, n)
		end)
	end)
end

-- One property of an object by its typed id (or a plain number, read as an integer).
local function value(id, key, default)
	local ename = enum_of(key) or "IntId"
	local ty = PROP_ENUMS[ename]
	if ty == nil then
		error("Value needs a property id (IntId, StringId, FloatId, BoolId, Int64Id, DataId or InstanceId), not " .. tostring(key), 3)
	end
	local v = props(id, ty)[num(key)]
	if v == nil then
		if default ~= nil then
			return default
		end
		return PROP_DEFAULTS[ty]
	end
	return v
end
U.value = value

local function has_value(id, key)
	local ename = enum_of(key) or "IntId"
	local ty = PROP_ENUMS[ename]
	if ty == nil then
		error("HasValue needs a property id, not " .. tostring(key), 3)
	end
	return props(id, ty)[num(key)] ~= nil
end

-- A position as UtilityBelt describes one: its landcell and its frame's origin (meters within the
-- landblock) and orientation.
local Position = class("Position")
U.Position = Position
local Frame = class("Frame")
local Vector3 -- set below

Position.define({
	get = {
		Landcell = function(p)
			return state(p).at.cell or 0
		end,
		Frame = function(p)
			return Frame.new({ at = state(p).at })
		end,
		NS = function(p)
			return state(p).at.ns
		end,
		EW = function(p)
			return state(p).at.ew
		end,
		Heading = function(p)
			return state(p).at.heading
		end,
	},
	call = {
		DistanceTo2D = function(p, o)
			return distance(state(p).at, state(o).at, true)
		end,
		DistanceTo3D = function(p, o)
			return distance(state(p).at, state(o).at, false)
		end,
		ToString = function(p)
			return U.coords_text(state(p).at)
		end,
	},
})
Position.tostring = function(p)
	return U.coords_text(state(p).at)
end

-- The arithmetic of map coordinates and landblocks: a landblock is 192 meters a side, a map unit
-- 240 meters, and the map's origin 101.9 units from the world's corner.
local MAP_ORIGIN, MAP_UNIT, LANDBLOCK = 101.9, 240, 192
local function block_x(cell)
	return math.floor(num(cell) / 16777216) % 256
end
local function block_y(cell)
	return math.floor(num(cell) / 65536) % 256
end
Position.define({
	get = {
		MaxCoord = function()
			return MAP_ORIGIN
		end,
	},
	call = {
		GetLandblockFromCoordinates = function(_, ew, ns)
			local gx, gy = (num(ew) + MAP_ORIGIN) * MAP_UNIT, (num(ns) + MAP_ORIGIN) * MAP_UNIT
			return math.floor(gx / LANDBLOCK) * 16777216 + math.floor(gy / LANDBLOCK) * 65536
		end,
		LandblockXDifference = function(_, origin, block)
			return block_x(block) - block_x(origin)
		end,
		LandblockYDifference = function(_, origin, block)
			return block_y(block) - block_y(origin)
		end,
		NSToLandblock = function(_, cell, ns)
			return (num(ns) + MAP_ORIGIN) * MAP_UNIT - block_y(cell) * LANDBLOCK
		end,
		EWToLandblock = function(_, cell, ew)
			return (num(ew) + MAP_ORIGIN) * MAP_UNIT - block_x(cell) * LANDBLOCK
		end,
		LandblockToNS = function(_, cell, y)
			return (block_y(cell) * LANDBLOCK + num(y)) / MAP_UNIT - MAP_ORIGIN
		end,
		LandblockToEW = function(_, cell, x)
			return (block_x(cell) * LANDBLOCK + num(x)) / MAP_UNIT - MAP_ORIGIN
		end,
	},
	refused = {
		C = "a position here is a landcell and an offset in it",
	},
})
Frame.define({
	get = {
		Origin = function(f)
			local a = state(f).at
			return Vector3.new(a.x or 0, a.y or 0, a.z or 0)
		end,
		Orientation = function(f)
			-- A heading in degrees clockwise from north, as a rotation about the vertical.
			local h = math.rad(state(f).at.heading or 0)
			return U.Quaternion(math.cos(-h / 2), 0, 0, math.sin(-h / 2))
		end,
	},
})

function U.coords_text(a)
	if a == nil then
		return ""
	end
	if a.ns ~= nil and a.ew ~= nil then
		return string.format(
			"%.2f%s, %.2f%s",
			math.abs(a.ns),
			a.ns >= 0 and "N" or "S",
			math.abs(a.ew),
			a.ew >= 0 and "E" or "W"
		)
	end
	return string.format("0x%08X [%.2f %.2f %.2f]", a.cell or 0, a.x or 0, a.y or 0, a.z or 0)
end

local function position(at)
	if at == nil then
		return nil
	end
	return Position.new({ at = at })
end
U.position = position

local function player_id()
	return D.game.player.id
end
U.player_id = player_id

local function place_of(id)
	local h = D.game.world:get(id)
	return h and h.position or nil
end
U.place_of = place_of

-- The items an object holds directly, in the order the world lists them (a container's own, not
-- its packs'), and the packs it holds.
local function contents(id)
	local items, packs = {}, {}
	for _, h in ipairs(D.game.world:find({ container = id })) do
		if h.class_id == 10 then
			table.insert(packs, h.id)
		else
			table.insert(items, h.id)
		end
	end
	return items, packs
end
U.contents = contents

local function equipment_ids(id)
	local out = {}
	for _, h in ipairs(D.game.world:find({ wielder = id })) do
		table.insert(out, h.id)
	end
	return out
end

local function all_item_ids(id)
	local items, packs = contents(id)
	local all = table.clone(items)
	for _, p in ipairs(packs) do
		local inner = contents(p)
		for _, i in ipairs(inner) do
			table.insert(all, i)
		end
	end
	return all
end
U.all_item_ids = all_item_ids

-- The character's own figures, which a script reads on its character's world object.
-- Every skill the game has is listed, as UtilityBelt lists them: one the character does not have is
-- untrained, at 0.
local function skill_objects()
	local out = {}
	local i = info("SkillId")
	for _, kv in ipairs(parsed(i).order) do
		if kv[2] ~= 0 then
			out[typed(i, kv[2])] = U.Skill.new({ s = { id = kv[2], name = kv[1], training = "untrained", base = 0, current = 0 } })
		end
	end
	for _, s in ipairs(D.game.player.skills or {}) do
		out[ev("SkillId", s.id)] = U.Skill.new({ s = s })
	end
	return dict(out, function(n)
		return ev("SkillId", n)
	end)
end
local function attribute_objects()
	local out = {}
	for _, a in ipairs(D.game.player.attributes or {}) do
		out[ev("AttributeId", a.id)] = U.Attribute.new({ a = a })
	end
	return dict(out, function(n)
		return ev("AttributeId", n)
	end)
end
local function vital_objects()
	local out = {}
	local p = D.game.player
	local stats = {}
	for _, v in ipairs(p.vital_stats or {}) do
		stats[v.id] = v
	end
	for id, field in pairs({ [1] = "health", [3] = "stamina", [5] = "mana" }) do
		local cur = p[field]
		out[ev("VitalId", id)] = U.Vital.new({ id = id, cur = cur, stat = stats[id] })
	end
	return dict(out, function(n)
		return ev("VitalId", n)
	end)
end

local function layered(spells)
	local out = {}
	for _, s in ipairs(spells or {}) do
		table.insert(out, U.LayeredSpellId.new({ id = s, layer = 0 }))
	end
	return list(out)
end

-- UtilityBelt's stance and combat mode of the character, from the combat mode the client tells.
local function combat_mode_of(id)
	if id ~= player_id() then
		return 1
	end
	local m = D.game.player.combat_mode or 1
	if m == 0 then
		m = 1
	end
	return m
end
local STANCE_OF_MODE = { [1] = 61, [2] = 62, [4] = 63, [8] = 73 }

WorldObject.define({
	get = {
		Id = function(o)
			return state(o).id
		end,
		Name = function(o)
			return handle(o).name or ""
		end,
		WeenieClassId = function(o)
			return handle(o).wcid or 0
		end,
		ClassId = function(o)
			return handle(o).wcid or 0
		end,
		ObjectClass = function(o)
			return ev("ObjectClass", handle(o).class_id or 0)
		end,
		ObjectType = function(o)
			return handle(o).type or 0
		end,
		IntValues = function(o)
			return prop_table(state(o).id, "IntId")
		end,
		Int64Values = function(o)
			return prop_table(state(o).id, "Int64Id")
		end,
		StringValues = function(o)
			return prop_table(state(o).id, "StringId")
		end,
		BoolValues = function(o)
			return prop_table(state(o).id, "BoolId")
		end,
		FloatValues = function(o)
			return prop_table(state(o).id, "FloatId")
		end,
		InstanceValues = function(o)
			return prop_table(state(o).id, "InstanceId")
		end,
		DataValues = function(o)
			return prop_table(state(o).id, "DataId")
		end,
		Skills = function(o)
			if state(o).id ~= player_id() then
				return dict({})
			end
			return keep("skills", skill_objects)
		end,
		Attributes = function(o)
			if state(o).id ~= player_id() then
				return dict({})
			end
			return keep("attributes", attribute_objects)
		end,
		Vitals = function(o)
			if state(o).id ~= player_id() then
				return dict({})
			end
			return keep("vitals", vital_objects)
		end,
		SpellIds = function(o)
			return layered(handle(o).spells)
		end,
		EnchantmentIds = function(o)
			if state(o).id ~= player_id() then
				return list({})
			end
			local ids = {}
			for _, e in ipairs(D.game.player.enchantments or {}) do
				table.insert(ids, e.spell)
			end
			return layered(ids)
		end,
		SpellId = function(o)
			return value(state(o).id, ev("DataId", 28), 0)
		end,
		ItemIds = function(o)
			return list((contents(state(o).id)))
		end,
		Items = function(o)
			return list_of_ids((contents(state(o).id)))
		end,
		AllItemIds = function(o)
			return list(all_item_ids(state(o).id))
		end,
		AllItems = function(o)
			return list_of_ids(all_item_ids(state(o).id))
		end,
		ContainerIds = function(o)
			local _, packs = contents(state(o).id)
			return list(packs)
		end,
		Containers = function(o)
			local _, packs = contents(state(o).id)
			return list_of_ids(packs)
		end,
		EquipmentIds = function(o)
			return list(equipment_ids(state(o).id))
		end,
		Equipment = function(o)
			return list_of_ids(equipment_ids(state(o).id))
		end,
		ContainerId = function(o)
			return handle(o).container or 0
		end,
		Container = function(o)
			return wo(handle(o).container)
		end,
		WielderId = function(o)
			return handle(o).wielder or 0
		end,
		Wielder = function(o)
			return wo(handle(o).wielder)
		end,
		ClothingPriority = function(o)
			return value(state(o).id, ev("IntId", 4), 0)
		end,
		RadarColor = function(o)
			return value(state(o).id, ev("IntId", 95), 0)
		end,
		RadarBehavior = function(o)
			return value(state(o).id, ev("IntId", 133), 0)
		end,
		Material = function(o)
			return handle(o).material or 0
		end,
		CurrentWieldedLocation = function(o)
			return handle(o).slot or 0
		end,
		ValidWieldedLocations = function(o)
			return value(state(o).id, ev("IntId", 9), 0)
		end,
		AmmoType = function(o)
			return value(state(o).id, ev("IntId", 50), 0)
		end,
		TargetType = function(o)
			return value(state(o).id, ev("IntId", 94), 0)
		end,
		IconEffects = function(o)
			return value(state(o).id, ev("IntId", 18), 0)
		end,
		CombatUse = function(o)
			return value(state(o).id, ev("IntId", 51), 0)
		end,
		Burden = function(o)
			return handle(o).burden or 0
		end,
		PhysicsState = function(o)
			return value(state(o).id, ev("IntId", 93), 0)
		end,
		HasAppraisalData = function(o)
			return handle(o).appraised == true
		end,
		LastAppraisalResponse = function(o)
			return U.DateTime(U.appraised_at(state(o).id) or 0)
		end,
		ServerPosition = function(o)
			return position(handle(o).position) or position({ cell = 0, x = 0, y = 0, z = 0 })
		end,
		CooldownTimeoutMilliseconds = function()
			return 0
		end,
		Stance = function(o)
			return ev("MotionStance", STANCE_OF_MODE[combat_mode_of(state(o).id)] or 61)
		end,
		CombatMode = function(o)
			return ev("CombatMode", combat_mode_of(state(o).id))
		end,
		CreatedAt = function(o)
			return U.DateTime(U.created_at(state(o).id) or now())
		end,
		LastAccessTime = function()
			return U.DateTime(now())
		end,
		ObjectDescriptionFlag = function(o)
			return value(state(o).id, ev("IntId", 0x0D00001A - 0x0D000000), 0)
		end,
		CreateFlags = function()
			return 0
		end,
		CreateFlags2 = function()
			return 0
		end,
		ContainerProperties = function(o)
			return handle(o).class_id == 10 and 1 or 0
		end,
		OnPositionChanged = function(o)
			return U.object_event(state(o).id, "moved")
		end,
		OnDestroyed = function(o)
			return U.object_event(state(o).id, "removed")
		end,
	},
	call = {
		HasValue = function(o, key)
			return has_value(state(o).id, key)
		end,
		Value = function(o, key, default)
			return value(state(o).id, key, default)
		end,
		DistanceTo3D = function(o, other)
			return distance(place_of(state(o).id), place_of(id_of(other)), false)
		end,
		DistanceTo2D = function(o, other)
			return distance(place_of(state(o).id), place_of(id_of(other)), true)
		end,
		GetTopmostParent = function(o)
			local id = state(o).id
			for _ = 1, 8 do
				local h = D.game.world:get(id)
				local up = h and (h.container or h.wielder)
				if up == nil or D.game.world:get(up) == nil then
					break
				end
				id = up
			end
			return wo(id)
		end,
		IsOnSharedCooldown = function()
			return false
		end,
		Wield = function(o, slot, options, cb)
			return U.Actions.ObjectWield(state(o).id, slot, options, cb)
		end,
		Drop = function(o, options, cb)
			return U.Actions.ObjectDrop(state(o).id, options, cb)
		end,
		Give = function(o, target, options, cb)
			return U.Actions.ObjectGive(state(o).id, target, options, cb)
		end,
		Move = function(o, target, slot, stack, options, cb)
			return U.Actions.ObjectMove(state(o).id, target, slot, stack, options, cb)
		end,
		Appraise = function(o, options, cb)
			return U.Actions.ObjectAppraise(state(o).id, options, cb)
		end,
		AddToSalvage = function(o, options, cb)
			return U.Actions.SalvageAdd(state(o).id, options, cb)
		end,
		Select = function(o, options, cb)
			return U.Actions.ObjectSelect(state(o).id, options, cb)
		end,
		Use = function(o, options, cb)
			return U.Actions.ObjectUse(state(o).id, 0, options, cb)
		end,
		UseOn = function(o, target, options, cb)
			return U.Actions.ObjectUse(state(o).id, target, options, cb)
		end,
		AddToTrade = function(o, options, cb)
			return U.Actions.TradeAdd(state(o).id, options, cb)
		end,
		Inscribe = function(o, text, options, cb)
			return U.Actions.Inscribe(state(o).id, text, options, cb)
		end,
		Salvage = function(o, options, cb)
			return U.Actions.RunAllOrdered({
				U.new_action("SalvageAddAction", state(o).id),
				U.new_action("SalvageAction"),
			}, options, cb)
		end,
		ToString = function(o)
			return string.format("%s (0x%08X)", handle(o).name or "?", state(o).id)
		end,
	},
	refused = {
		ObjectDescription = "the client keeps an object's look, not as the old client's description",
		PhysicsDesc = "the client keeps an object's physics, not as the old client's description",
	},
})
WorldObject.tostring = function(o)
	local h = handle(o)
	return string.format("%s (0x%08X)", h and h.name or "?", state(o).id)
end

-- The character's skills, attributes and vitals as UtilityBelt describes them.
local Skill = class("Skill")
U.Skill = Skill
local TRAINING = { untrained = 1, trained = 2, specialized = 3 }
Skill.define({
	get = {
		Type = function(s)
			return ev("SkillId", state(s).s.id)
		end,
		Base = function(s)
			return state(s).s.base or 0
		end,
		Current = function(s)
			return state(s).s.current or 0
		end,
		Training = function(s)
			return ev("SkillTrainingType", TRAINING[state(s).s.training] or 1)
		end,
		Name = function(s)
			return state(s).s.name
		end,
		PointsRaised = function()
			return 0
		end,
		Experience = function()
			return 0
		end,
	},
})
Skill.tostring = function(s)
	return tostring(state(s).s.name)
end

local Attribute = class("Attribute")
U.Attribute = Attribute
Attribute.define({
	get = {
		Type = function(a)
			return ev("AttributeId", state(a).a.id)
		end,
		Base = function(a)
			return state(a).a.base or 0
		end,
		Current = function(a)
			return state(a).a.current or 0
		end,
		Name = function(a)
			return state(a).a.name
		end,
		CostToRaise = function(a)
			return state(a).a.cost
		end,
		PointsRaised = function()
			return 0
		end,
		Experience = function()
			return 0
		end,
	},
})

local Vital = class("Vital")
U.Vital = Vital
Vital.define({
	get = {
		Type = function(v)
			return ev("VitalId", state(v).id)
		end,
		Base = function(v)
			local s = state(v).stat
			return s and s.base or 0
		end,
		Max = function(v)
			local c = state(v).cur
			return c and c.max or 0
		end,
		Current = function(v)
			local c = state(v).cur
			return c and c.current or 0
		end,
		PointsRaised = function()
			return 0
		end,
		Experience = function()
			return 0
		end,
	},
})

local SKILL_ONLY = "the plugin interface reads a skill's training and its base and current values"
Skill.define({
	refused = {
		AdjustXP = SKILL_ONLY,
		InitLevel = SKILL_ONLY,
		ResistanceOfLastCheck = SKILL_ONLY,
		LastUsedTime = SKILL_ONLY,
		Formula = SKILL_ONLY,
		Dat = SKILL_ONLY,
		MinTraining = SKILL_ONLY,
		MaxTraining = SKILL_ONLY,
		CostToIncreaseTraining = SKILL_ONLY,
		TrainedCost = SKILL_ONLY,
		SpecializedCost = SKILL_ONLY,
		CanLower = SKILL_ONLY,
		CanRaise = SKILL_ONLY,
		CreditsSpent = SKILL_ONLY,
		MeleeSkills = SKILL_ONLY,
		MissileSkills = SKILL_ONLY,
		MagicSkills = SKILL_ONLY,
		AlwaysTrained = SKILL_ONLY,
		AugSpecSkills = SKILL_ONLY,
	},
})
local STAT_ONLY = "the plugin interface reads an attribute's or a vital's base and current values"
Attribute.define({ refused = { InitLevel = STAT_ONLY } })
Vital.define({ refused = { InitLevel = STAT_ONLY, Formula = STAT_ONLY } })

local LayeredSpellId = class("LayeredSpellId")
U.LayeredSpellId = LayeredSpellId
LayeredSpellId.define({
	get = {
		Id = function(l)
			return state(l).id
		end,
		Layer = function(l)
			return state(l).layer
		end,
	},
})
LayeredSpellId.tostring = function(l)
	return tostring(state(l).id)
end

-- Vectors ------------------------------------------------------------------------------------------

local function vector(name, fields)
	local c = class(name)
	local get, set = {}, {}
	for _, f in ipairs(fields) do
		local lf = string.lower(f)
		get[f] = function(v)
			return state(v)[lf]
		end
		set[f] = function(v, x)
			state(v)[lf] = tonumber(x) or 0
		end
	end
	c.define({ get = get, set = set })
	c.tostring = function(v)
		local parts = {}
		for _, f in ipairs(fields) do
			table.insert(parts, tostring(state(v)[string.lower(f)]))
		end
		return "<" .. table.concat(parts, ", ") .. ">"
	end
	local function make(...)
		local args = { ... }
		local s = {}
		for i, f in ipairs(fields) do
			s[string.lower(f)] = tonumber(args[i]) or 0
		end
		return c.new(s)
	end
	local ops = {
		__add = function(a, b)
			local s = {}
			for _, f in ipairs(fields) do
				local lf = string.lower(f)
				s[lf] = state(a)[lf] + state(b)[lf]
			end
			return c.new(s)
		end,
		__sub = function(a, b)
			local s = {}
			for _, f in ipairs(fields) do
				local lf = string.lower(f)
				s[lf] = state(a)[lf] - state(b)[lf]
			end
			return c.new(s)
		end,
		__mul = function(a, b)
			local v, k = a, b
			if type(a) == "number" then
				v, k = b, a
			end
			local s = {}
			for _, f in ipairs(fields) do
				local lf = string.lower(f)
				s[lf] = state(v)[lf] * k
			end
			return c.new(s)
		end,
		__eq = function(a, b)
			for _, f in ipairs(fields) do
				local lf = string.lower(f)
				if state(a)[lf] ~= state(b)[lf] then
					return false
				end
			end
			return true
		end,
	}
	for k, f in pairs(ops) do
		c.meta[k] = f
	end
	local static = setmetatable({}, {
		__index = function(_, key)
			local n = norm(key)
			if n == "new" then
				return make
			elseif n == "zero" then
				return make()
			elseif n == "one" then
				return make(1, 1, 1, 1)
			end
			error(name .. " has no member " .. tostring(key), 2)
		end,
		__call = function(_, ...)
			return make(...)
		end,
	})
	c.make = make
	return c, static
end
local Vector2c, Vector2s = vector("Vector2", { "X", "Y" })
local Vector3c, Vector3s = vector("Vector3", { "X", "Y", "Z" })
local Vector4c, Vector4s = vector("Vector4", { "X", "Y", "Z", "W" })
local Quaternionc = vector("Quaternion", { "W", "X", "Y", "Z" })
Vector3 = { new = Vector3c.make }
U.Vector2 = Vector2c.make
U.Vector3 = Vector3c.make
U.Vector4 = Vector4c.make
U.Quaternion = Quaternionc.make
U.is_vector2 = function(v)
	return is(v, Vector2c)
end
U.vec = function(v)
	if type(v) == "table" and rawget(v, S) ~= nil then
		local s = rawget(v, S)
		return s.x or 0, s.y or 0, s.z or 0, s.w or 0
	end
	return 0, 0, 0, 0
end

-- Dates and spans ----------------------------------------------------------------------------------

-- .NET's `DateTime` and `TimeSpan` as scripts use them: the time now, its parts, adding to it, and
-- the span between two (`(DateTime.UtcNow - started).TotalSeconds`).
local DateTimec = class("DateTime")
local TimeSpanc = class("TimeSpan")
local function datetime(t, utc)
	return DateTimec.new({ t = t, utc = utc ~= false })
end
local function timespan(s)
	return TimeSpanc.new({ s = s })
end
U.DateTime = datetime
U.TimeSpan = timespan
local function dt_part(fmt)
	return function(d)
		return tonumber(D.os.date((state(d).utc and "!" or "") .. fmt, math.floor(state(d).t)))
	end
end
DateTimec.define({
	get = {
		Year = dt_part("%Y"),
		Month = dt_part("%m"),
		Day = dt_part("%d"),
		Hour = dt_part("%H"),
		Minute = dt_part("%M"),
		Second = dt_part("%S"),
		Millisecond = function(d)
			return math.floor((state(d).t % 1) * 1000)
		end,
		DayOfYear = dt_part("%j"),
		Ticks = function(d)
			return (state(d).t + 62135596800) * 10000000
		end,
		UnixTime = function(d)
			return math.floor(state(d).t)
		end,
	},
	call = {
		AddSeconds = function(d, n)
			return datetime(state(d).t + n, state(d).utc)
		end,
		AddMilliseconds = function(d, n)
			return datetime(state(d).t + n / 1000, state(d).utc)
		end,
		AddMinutes = function(d, n)
			return datetime(state(d).t + n * 60, state(d).utc)
		end,
		AddHours = function(d, n)
			return datetime(state(d).t + n * 3600, state(d).utc)
		end,
		AddDays = function(d, n)
			return datetime(state(d).t + n * 86400, state(d).utc)
		end,
		Add = function(d, span)
			return datetime(state(d).t + state(span).s, state(d).utc)
		end,
		Subtract = function(d, o)
			if is(o, TimeSpanc) then
				return datetime(state(d).t - state(o).s, state(d).utc)
			end
			return timespan(state(d).t - state(o).t)
		end,
		ToString = function(d, fmt)
			return D.os.date((state(d).utc and "!" or "") .. "%Y-%m-%d %H:%M:%S", math.floor(state(d).t))
		end,
		ToLocalTime = function(d)
			return datetime(state(d).t, false)
		end,
		ToUniversalTime = function(d)
			return datetime(state(d).t, true)
		end,
	},
})
DateTimec.tostring = function(d)
	return D.os.date((state(d).utc and "!" or "") .. "%Y-%m-%d %H:%M:%S", math.floor(state(d).t))
end
DateTimec.meta.__sub = function(a, b)
	if is(b, TimeSpanc) then
		return datetime(state(a).t - state(b).s, state(a).utc)
	end
	return timespan(state(a).t - state(b).t)
end
DateTimec.meta.__add = function(a, b)
	return datetime(state(a).t + state(b).s, state(a).utc)
end
DateTimec.meta.__lt = function(a, b)
	return state(a).t < state(b).t
end
DateTimec.meta.__le = function(a, b)
	return state(a).t <= state(b).t
end
DateTimec.meta.__eq = function(a, b)
	return state(a).t == state(b).t
end
TimeSpanc.define({
	get = {
		TotalSeconds = function(s)
			return state(s).s
		end,
		TotalMilliseconds = function(s)
			return state(s).s * 1000
		end,
		TotalMinutes = function(s)
			return state(s).s / 60
		end,
		TotalHours = function(s)
			return state(s).s / 3600
		end,
		TotalDays = function(s)
			return state(s).s / 86400
		end,
		Seconds = function(s)
			return math.floor(state(s).s) % 60
		end,
		Minutes = function(s)
			return math.floor(state(s).s / 60) % 60
		end,
		Hours = function(s)
			return math.floor(state(s).s / 3600) % 24
		end,
		Days = function(s)
			return math.floor(state(s).s / 86400)
		end,
		Milliseconds = function(s)
			return math.floor(state(s).s * 1000) % 1000
		end,
		Ticks = function(s)
			return state(s).s * 10000000
		end,
	},
	call = {
		ToString = function(s)
			local t = math.floor(math.abs(state(s).s))
			return string.format("%s%02d:%02d:%02d", state(s).s < 0 and "-" or "", t // 3600, (t // 60) % 60, t % 60)
		end,
	},
})
TimeSpanc.tostring = function(s)
	local t = math.floor(math.abs(state(s).s))
	return string.format("%s%02d:%02d:%02d", state(s).s < 0 and "-" or "", t // 3600, (t // 60) % 60, t % 60)
end
TimeSpanc.meta.__add = function(a, b)
	return timespan(state(a).s + state(b).s)
end
TimeSpanc.meta.__sub = function(a, b)
	return timespan(state(a).s - state(b).s)
end
TimeSpanc.meta.__lt = function(a, b)
	return state(a).s < state(b).s
end
TimeSpanc.meta.__le = function(a, b)
	return state(a).s <= state(b).s
end
TimeSpanc.meta.__eq = function(a, b)
	return state(a).s == state(b).s
end
local function static(name, members)
	local m = {}
	for k, f in pairs(members) do
		m[norm(k)] = f
	end
	return setmetatable({}, {
		__index = function(_, key)
			local f = m[norm(key)]
			if f == nil then
				error(name .. " has no member " .. tostring(key), 2)
			end
			if type(f) == "function" and string.sub(key, 1, 1) ~= "!" then
				return f
			end
			return f
		end,
		__newindex = function()
			error(name .. " is read-only", 2)
		end,
	})
end
U.static = static
local DateTimeStatic = setmetatable({}, {
	__index = function(_, key)
		local n = norm(key)
		if n == "utcnow" then
			return datetime(now(), true)
		elseif n == "now" then
			return datetime(now(), false)
		elseif n == "minvalue" then
			return datetime(-62135596800, true)
		elseif n == "maxvalue" then
			return datetime(253402300799, true)
		elseif n == "fromunixtime" or n == "fromunixtimeseconds" then
			return function(t)
				return datetime(t, true)
			end
		end
		error("DateTime has no member " .. tostring(key), 2)
	end,
})
local TimeSpanStatic = static("TimeSpan", {
	FromSeconds = function(n)
		return timespan(n)
	end,
	FromMilliseconds = function(n)
		return timespan(n / 1000)
	end,
	FromMinutes = function(n)
		return timespan(n * 60)
	end,
	FromHours = function(n)
		return timespan(n * 3600)
	end,
	FromDays = function(n)
		return timespan(n * 86400)
	end,
	Zero = timespan(0),
})

-- JSON -----------------------------------------------------------------------------------------------

local function json_escape(s)
	return (
		string.gsub(s, '[%c"\\]', function(c)
			local m = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
			return m[c] or string.format("\\u%04x", string.byte(c))
		end)
	)
end
local function json_encode(v, out, seen)
	local t = type(v)
	if v == nil then
		table.insert(out, "null")
	elseif t == "boolean" then
		table.insert(out, v and "true" or "false")
	elseif t == "number" then
		if v ~= v or v == math.huge or v == -math.huge then
			table.insert(out, "null")
		elseif math.floor(v) == v and math.abs(v) < 1e15 then
			table.insert(out, string.format("%d", v))
		else
			table.insert(out, string.format("%.17g", v))
		end
	elseif t == "string" then
		table.insert(out, '"' .. json_escape(v) .. '"')
	elseif t == "table" then
		if getmetatable(v) == ENUMV then
			table.insert(out, string.format("%d", num(v)))
			return
		end
		if seen[v] then
			error("json.serialize: a table that holds itself", 0)
		end
		seen[v] = true
		local n = #v
		local count = 0
		for _ in pairs(v) do
			count = count + 1
		end
		if n > 0 and count == n then
			table.insert(out, "[")
			for i = 1, n do
				if i > 1 then
					table.insert(out, ",")
				end
				json_encode(v[i], out, seen)
			end
			table.insert(out, "]")
		else
			table.insert(out, "{")
			local keys = {}
			for k in pairs(v) do
				if type(k) == "string" or type(k) == "number" then
					table.insert(keys, k)
				end
			end
			table.sort(keys, function(a, b)
				return tostring(a) < tostring(b)
			end)
			for i, k in ipairs(keys) do
				if i > 1 then
					table.insert(out, ",")
				end
				table.insert(out, '"' .. json_escape(tostring(k)) .. '":')
				json_encode(v[k], out, seen)
			end
			table.insert(out, "}")
		end
		seen[v] = nil
	else
		error("json.serialize: a " .. t .. " has no JSON form", 0)
	end
end
local function json_decode(s)
	local pos = 1
	local function ws()
		pos = string.find(s, "[^ \t\r\n]", pos) or #s + 1
	end
	local value
	local function str()
		local out = {}
		pos = pos + 1
		while true do
			local c = string.sub(s, pos, pos)
			if c == "" then
				error("json.parse: an unterminated string", 0)
			elseif c == '"' then
				pos = pos + 1
				return table.concat(out)
			elseif c == "\\" then
				local e = string.sub(s, pos + 1, pos + 1)
				local map = { b = "\b", f = "\f", n = "\n", r = "\r", t = "\t", ['"'] = '"', ["\\"] = "\\", ["/"] = "/" }
				if e == "u" then
					local code = tonumber(string.sub(s, pos + 2, pos + 5), 16) or 63
					table.insert(out, utf8.char(code))
					pos = pos + 6
				else
					table.insert(out, map[e] or e)
					pos = pos + 2
				end
			else
				table.insert(out, c)
				pos = pos + 1
			end
		end
	end
	value = function()
		ws()
		local c = string.sub(s, pos, pos)
		if c == "{" then
			local t = {}
			pos = pos + 1
			ws()
			if string.sub(s, pos, pos) == "}" then
				pos = pos + 1
				return t
			end
			while true do
				ws()
				if string.sub(s, pos, pos) ~= '"' then
					error("json.parse: a key must be a string at " .. pos, 0)
				end
				local k = str()
				ws()
				if string.sub(s, pos, pos) ~= ":" then
					error("json.parse: ':' expected at " .. pos, 0)
				end
				pos = pos + 1
				t[k] = value()
				ws()
				local d = string.sub(s, pos, pos)
				pos = pos + 1
				if d == "}" then
					return t
				elseif d ~= "," then
					error("json.parse: ',' or '}' expected at " .. (pos - 1), 0)
				end
			end
		elseif c == "[" then
			local t = {}
			pos = pos + 1
			ws()
			if string.sub(s, pos, pos) == "]" then
				pos = pos + 1
				return t
			end
			while true do
				table.insert(t, value())
				ws()
				local d = string.sub(s, pos, pos)
				pos = pos + 1
				if d == "]" then
					return t
				elseif d ~= "," then
					error("json.parse: ',' or ']' expected at " .. (pos - 1), 0)
				end
			end
		elseif c == '"' then
			return str()
		elseif string.sub(s, pos, pos + 3) == "true" then
			pos = pos + 4
			return true
		elseif string.sub(s, pos, pos + 4) == "false" then
			pos = pos + 5
			return false
		elseif string.sub(s, pos, pos + 3) == "null" then
			pos = pos + 4
			return nil
		else
			local n = string.match(s, "^-?%d+%.?%d*[eE]?[-+]?%d*", pos)
			if n == nil or n == "" then
				error("json.parse: unexpected text at " .. pos, 0)
			end
			pos = pos + #n
			return tonumber(n)
		end
	end
	local v = value()
	ws()
	if pos <= #s then
		error("json.parse: text after the value at " .. pos, 0)
	end
	return v
end
local json = {
	parse = function(text)
		return json_decode(tostring(text))
	end,
	serialize = function(v)
		local out = {}
		json_encode(v, out, {})
		return table.concat(out)
	end,
}
json.stringify = json.serialize
json.encode = json.serialize
json.decode = json.parse
U.json = json

-- Regular expressions --------------------------------------------------------------------------------

-- .NET's `Regex` as scripts use it, in the plugin interface's dialect: `IsMatch`, `Match` and
-- `Matches` (a match's `Success`, `Value` and `Groups`). `Replace` and `Split` are refused.
local Matchc = class("Match")
local function match_of(m)
	return Matchc.new({ m = m })
end
local Groupc = class("Group")
Groupc.define({
	get = {
		Value = function(g)
			return state(g).v or ""
		end,
		Success = function(g)
			return state(g).v ~= nil
		end,
	},
})
Groupc.tostring = function(g)
	return state(g).v or ""
end
Matchc.define({
	get = {
		Success = function(m)
			return state(m).m ~= nil
		end,
		Value = function(m)
			local r = state(m).m
			return r and r.text or ""
		end,
		Groups = function(m)
			local r = state(m).m
			local out = {}
			if r ~= nil then
				out[0] = Groupc.new({ v = r.text })
				for i, g in ipairs(r.groups or {}) do
					out[i] = Groupc.new({ v = g ~= false and g or nil })
				end
				for k, g in pairs(r.named or {}) do
					out[k] = Groupc.new({ v = g ~= false and g or nil })
				end
			end
			return out
		end,
	},
})
Matchc.tostring = function(m)
	local r = state(m).m
	return r and r.text or ""
end
local function ignore_case(opts)
	return opts ~= nil and band(num(opts), 1) ~= 0
end
local RegexStatic = static("Regex", {
	IsMatch = function(text, pattern, opts)
		return D.regex.match(pattern, tostring(text), ignore_case(opts)) ~= nil
	end,
	Match = function(text, pattern, opts)
		return match_of(D.regex.match(pattern, tostring(text), ignore_case(opts)))
	end,
	Matches = function(text, pattern, opts)
		-- Each match in turn, the text after one searched for the next.
		local out, rest = {}, tostring(text)
		for _ = 1, 1000 do
			local r = D.regex.match(pattern, rest, ignore_case(opts))
			if r == nil or r.text == "" then
				break
			end
			table.insert(out, match_of(r))
			local at = string.find(rest, r.text, 1, true)
			if at == nil then
				break
			end
			rest = string.sub(rest, at + #r.text)
		end
		return list(out)
	end,
	Escape = function(text)
		return (string.gsub(tostring(text), "[%^%$%(%)%%%.%[%]%*%+%-%?%{%}%|\\]", "\\%0"))
	end,
	Replace = function()
		error(refusal("Regex.Replace", "the plugin interface's patterns match; they do not replace"), 2)
	end,
	Split = function()
		error(refusal("Regex.Split", "the plugin interface's patterns match; they do not split"), 2)
	end,
})

-- Player and session state -------------------------------------------------------------------------

-- UtilityBelt's client state, from the session's phase and context.
local STATE = {
	offline = 0,
	connecting = 1,
	character_select = 2,
	entering = 4,
	in_world = 6,
	disconnected = 8,
}
local function client_state()
	local s = D.session.info()
	if s.context == "chargen" then
		return 3
	end
	return STATE[s.phase] or 0
end
U.client_state = client_state

local function in_world()
	return client_state() == 6
end
U.in_world = in_world

-- The script's environment ---------------------------------------------------------------------------

-- The globals every script starts with, besides the enumerations and the plugin interface's own
-- libraries (`string`, `math`, `table`, `os`, ...).
local function script_globals(r)
	local g = {}
	g.game = r.game
	g.await = U.await
	g.sleep = sleep
	g.json = json
	g.DateTime = DateTimeStatic
	g.TimeSpan = TimeSpanStatic
	g.Regex = RegexStatic
	g.Vector2 = Vector2s
	g.Vector3 = Vector3s
	g.Vector4 = Vector4s
	g.print = function(...)
		local parts = table.pack(...)
		for i = 1, parts.n do
			parts[i] = tostring(parts[i])
		end
		local line = table.concat(parts, " ", 1, parts.n)
		D.print(line)
		-- UtilityBelt shows what a script prints in the chat too, under the script's name.
		pcall(D.chat.print, "[" .. r.name .. "] " .. line)
	end
	g.Enum = static("Enum", {
		GetName = function(e, v)
			return U.enum_name(tostring(e), num(v))
		end,
		GetValues = function(e)
			return enum(tostring(e)).GetValues()
		end,
	})
	g.require = function(name)
		return U.require(r, name)
	end
	for k, v in pairs(U.globals) do
		g[k] = v
	end
	return g
end

-- Libraries and installers of the script API's other parts, filled in as they load, and what
-- each part sets going as a script starts.
U.globals = {}
U.modules = {}
U.on_start = {}

-- `require` as a script uses it: UtilityBelt's modules by name (`utilitybelt.views`, `imgui`,
-- `acclient`, `filesystem`, `ubnet`, `http`, `dats`), then the script's own files, each loaded
-- once with the script's globals.
function U.require(r, name)
	assert(type(name) == "string" and name ~= "", "require() needs a module's name")
	local key = string.lower(name)
	local cached = r.loaded[key]
	if cached ~= nil then
		return cached
	end
	local m = U.modules[key]
	if m ~= nil then
		local v = m(r)
		r.loaded[key] = v
		return v
	end
	local path = string.gsub(name, "\\", "/")
	path = string.gsub(path, "^/+", "")
	local source = D.files.package(path)
	if source == nil and not string.find(path, "%.lua$") then
		source = D.files.package(path .. ".lua")
		if source == nil then
			source = D.files.package((string.gsub(path, "%.", "/")) .. ".lua")
		end
	end
	if source == nil then
		error("module " .. name .. " is not in the script's folder", 2)
	end
	local fn = compile(path, source, r.env)
	local v = fn(name)
	if v == nil then
		v = true
	end
	r.loaded[key] = v
	return v
end

local function make_env(r)
	local g = script_globals(r)
	local env = {}
	setmetatable(env, {
		__index = function(_, k)
			local v = g[k]
			if v ~= nil then
				return v
			end
			if type(k) == "string" then
				local e = enum(k)
				if e ~= nil then
					g[k] = e
					return e
				end
				local a = U.action_class and U.action_class(k)
				if a ~= nil then
					g[k] = a
					return a
				end
			end
			-- The plugin interface's own globals, and the safe libraries.
			return _G[k]
		end,
	})
	return env
end

-- The parts of the script API, each a library that installs itself.
require("dereth.ubscript.actions")(U)
require("dereth.ubscript.world")(U)
require("dereth.ubscript.views")(U)
require("dereth.ubscript.modules")(U)

-- Running the script ---------------------------------------------------------------------------------

local M = {}

local entry_path

local function stop()
	local r = run
	if r == nil then
		return
	end
	-- OnScriptEnd, then everything the script set going goes with it.
	if r.game ~= nil then
		fire(U.game_event(r.game, "OnScriptEnd"), nil)
	end
	for _, f in ipairs(r.ends) do
		pcall(f)
	end
	for _, l in ipairs(r.listeners) do
		D.off(l[1], l[2])
	end
	for _, id in ipairs(r.timers) do
		D.task.cancel(id)
	end
	run = nil
end

local function start()
	if run ~= nil then
		return
	end
	local name = string.match(entry_path, "([^/]+)%.lua$") or entry_path
	local pkg = D.files.package_list and D.files.package_list() or {}
	local r = {
		name = U.script_name or name,
		listeners = {},
		timers = {},
		ends = {},
		loaded = {},
	}
	run = r
	r.game = U.new_game(r)
	r.env = make_env(r)
	for _, f in ipairs(U.on_start) do
		f()
	end
	local source = D.files.package(entry_path)
	if source == nil then
		run = nil
		error("the script's entry " .. entry_path .. " is not in its package", 0)
	end
	local fn = compile(entry_path, source, r.env)
	spawn(fn)
	raise()
	return pkg
end

-- Run the script at `entry` (a file of the package), named `name` where it prints: now, when a
-- character is in the world or the plugin is active before the world; else when a character
-- enters it. A log-off ends it (its `OnScriptEnd` heard), and the next entry runs it afresh.
function M.run(entry, name)
	assert(type(entry) == "string", "run(entry) needs the script's entry file")
	entry_path = entry
	U.script_name = name
	D.on("context.begin", function()
		if run == nil then
			start()
		end
	end)
	D.on("context.end", function()
		stop()
		raise()
	end)
	-- The plugin's own lifecycle hook (the library runs in the plugin's globals): the script ends
	-- before the plugin goes.
	on_unload = function()
		stop()
	end
	local ctx = D.session.info().context
	if ctx == "world" and in_world() then
		start()
	end
end

-- For the library's own tests: the script running now.
function M.running()
	return run
end

M.U = U
return M
