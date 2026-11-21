-- The plugin API's Lua-level bindings every game shares: events, output and chat, commands, tasks,
-- the store and the plugin's files, promises, the session, peers, HTTPS, the world's policy,
-- declared UI, the message stream, services, permissions, the host, `os`, `require` and the event
-- dispatcher, all written over one host call.
--
-- This chunk runs once per VM, before the game's own layer and before the sandbox is sealed. It
-- receives the host call, a compiler for the plugin's own modules, the traceback function, the
-- sources of the host's libraries written in Lua and a function answering the share of its budget
-- the entry in progress has spent, and returns the API table (whose entries become read-only
-- globals), the event dispatcher and the function run as each entry begins (which the engine
-- keeps and never exposes), and the kit the game's layer builds its namespaces from.

local hostcall, compile, traceback, libraries, budget_spent = ...

local api = {}
local handlers = {} -- event name -> list of functions, in registration order
local timers = {} -- timer id -> { fn = function } or { co = coroutine }, and repeats
local commands = {} -- command name -> function
local modules = {} -- module name -> loaded value
local promises = {} -- action id -> its promise, until it settles
local services = {} -- service name -> the function that answers it
local tasks = setmetatable({}, { __mode = "k" }) -- the coroutines `task.spawn` made

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
		tasks[co] = true
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
-- `on_failed` is called with the reason, and for a journey a door stopped, that door (`p.door`).
function promise_methods:next(on_done, on_failed)
	if self.state == "completed" then
		if on_done then
			on_done(self.value)
		end
	elseif self.state == "failed" then
		if on_failed then
			on_failed(self.reason, self.door)
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

api.policy = {
	info = function()
		return hostcall("policy.info")
	end,
}

-- Entries into the VM so far: what a library reads of the game can be kept for one entry.
local entries = 0
-- What the game's layer forgets as each entry begins (what it read of the game in the last).
local entry_hooks = {}
local function new_entry()
	entries = entries + 1
	for _, forget in ipairs(entry_hooks) do
		forget()
	end
end
local function entry()
	return entries
end

-- A file the player picks for the plugin, each time in a dialog the host owns (`files.import`):
-- `files.import{ prompt = "…", extensions = { "met" } }` is a promise of `{ name, text }`, failed
-- `refused` when the player declines.
api.files.import = function(t)
	return request("files.import", t or {})
end

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
				p.state, p.reason, p.door = "failed", data.reason, data.door
			end
			for _, cb in ipairs(p.callbacks) do
				local f = cb[1]
				local arg = p.value
				if p.state == "failed" then
					f, arg = cb[2], p.reason
				end
				if f ~= nil then
					call(f, arg, p.door)
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

-- Whether the code running is a task's own (not a coroutine of the plugin's inside one), where a
-- library may sleep without its caller noticing.
local function in_task()
	local co = coroutine.running()
	return co ~= nil and tasks[co] == true and coroutine.isyieldable()
end

-- What the game's layer is given besides the API table: the host call and the shared pieces it
-- builds its namespaces from. It never reaches a plugin.
local kit = {
	hostcall = hostcall,
	-- `request(name, args, transform)`: an action asked of the host, as a promise.
	request = request,
	-- A property table keyed by number, from the host's text keys.
	numbered = numbered,
	-- The host's own libraries, which `require` finds before the plugin's package, by name.
	builtin = builtin,
	-- `on_entry(fn)`: run `fn` as each entry begins.
	on_entry = function(fn)
		table.insert(entry_hooks, fn)
	end,
	-- `in_task()`: whether a library's long work may go on in the entries after this one
	-- (`checkpoint`).
	in_task = in_task,
	-- `checkpoint()`, between the pieces of a library's long work: in a task, once the entry has
	-- spent a quarter of its budget, the work sleeps and goes on in the next entry, leaving the
	-- rest for a piece that runs long; elsewhere it does nothing, and the work is the entry's.
	checkpoint = function()
		if in_task() and budget_spent() >= 0.25 then
			api.task.sleep(0)
		end
	end,
}

return api, dispatch, new_entry, kit
