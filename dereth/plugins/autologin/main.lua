-- Auto-login: enters the world as the character you chose when the character list comes up.
--
-- Its window holds everything it does: the character to log in, whether it logs in at all, the
-- lines it types after login (added, removed and put in order), and "log in the next one". The
-- commands are shortcuts to the same:
--
-- /autologin                 open the window, and say the settings
-- /autologin set [name]      log this character (or the one named) in automatically
-- /autologin on | off        log the chosen character in, or stop logging in automatically
-- /autologin cmd add <line>  type <line> after each login (in order, a second apart)
-- /autologin cmd remove <n>  forget the nth line
-- /autologin cmd clear       forget the lines
-- /autologin next            log the next character on the list in, once (logging out first)
--
-- Settings are kept per world and account. When the server says the character's last session is
-- still in the world, it tries again every five seconds, for a minute.

local RETRY_MS = 5000
local MAX_TRIES = 12

local function key()
	local s = session.info()
	return "login:" .. tostring(s.world) .. ":" .. tostring(s.account)
end

-- The settings: the chosen character, whether it is logged in automatically, and the lines typed
-- after login. Settings kept before the switch existed name a character only when it was on.
local function settings()
	local s = store.global[key()] or {}
	s.commands = s.commands or {}
	if s.on == nil then
		s.on = s.character ~= nil
	end
	return s
end

local function save(s)
	store.global[key()] = s
end

-- The account's characters, by name, as the character list shows them.
local function characters()
	local out = {}
	for _, c in ipairs(session.info().characters or {}) do
		table.insert(out, c.name)
	end
	return out
end

local tries = 0
local once = nil -- a character to log in once, instead of the chosen one
local chosen_line = nil -- the line chosen in the window's list
local status = ""

local win = ui.window({
	id = "main",
	title = "Auto-login",
	width = 44,
	ui.toggle({ id = "on", text = "Log in automatically", value = false }),
	ui.choice({ id = "character", text = "Character", options = { "" }, value = "" }),
	ui.label({ id = "status", text = "" }),
	ui.group({
		title = "Typed after login, in order",
		ui.list({ id = "commands", columns = { { title = "#", width = 3 }, "Line" }, rows = {}, height = 5 }),
		ui.row({
			ui.input({ id = "line", text = "Line", value = "", placeholder = "/fellowship create Hunters", grow = 1 }),
			ui.button({ id = "add", text = "Add" }),
		}),
		ui.row({
			ui.button({ id = "remove", text = "Remove" }),
			ui.button({ id = "up", text = "Up" }),
			ui.button({ id = "down", text = "Down" }),
		}),
	}),
	ui.button({ id = "next", text = "Log in the next one" }),
})

-- The window shows the settings as they are now.
local function refresh()
	local s = settings()
	local options = { "" }
	local listed = s.character == nil
	for _, n in ipairs(characters()) do
		table.insert(options, n)
		listed = listed or string.lower(n) == string.lower(s.character)
	end
	if not listed then
		table.insert(options, s.character)
	end
	win:get("on").value = s.on and true or false
	win:get("character").options = options
	win:get("character").value = s.character or ""
	local rows = {}
	for i, line in ipairs(s.commands) do
		table.insert(rows, { i, line })
	end
	win:get("commands").rows = rows
	if chosen_line ~= nil and s.commands[chosen_line] == nil then
		chosen_line = nil
	end
	win:get("commands").selected = chosen_line
	local what
	if s.on and s.character then
		what = "Logs " .. s.character .. " in at the character list."
	elseif s.character then
		what = "Off: " .. s.character .. " is chosen."
	else
		what = "Off."
	end
	win:get("status").text = status ~= "" and (what .. " " .. status) or what
end

local function say(text)
	status = text
	chat.print("[Auto-login] " .. text)
	refresh()
end

local function enter(name)
	tries = tries + 1
	task.spawn(function()
		local ok, why = pawait(session.select_character(name))
		if ok then
			print("logged in as", name)
			return
		end
		print("login refused", name, why)
		if (why == "character_in_world" or why == "character_in_world_server") and tries < MAX_TRIES then
			task.after(RETRY_MS, function()
				enter(name)
			end)
		end
	end)
end

on("session.pregame", function(e)
	refresh()
	local s = settings()
	local want = once or (s.on and s.character) or nil
	once = nil
	if want == nil then
		return
	end
	for _, c in ipairs(e.characters) do
		if string.lower(c.name) == string.lower(want) then
			tries = 0
			task.after(1000, function()
				enter(c.name)
			end)
			return
		end
	end
	print("no character", want, "on this account")
end)

on("session.login", function()
	status = ""
	refresh()
	-- Each line waits a second after the one before it is typed, so a long first frame in the
	-- world cannot bring them all due at once (chat allows three lines at once, then one a second).
	local cmds = settings().commands
	local function send(i)
		if cmds[i] == nil then
			return
		end
		task.after(1000, function()
			chat.command(cmds[i])
			send(i + 1)
		end)
	end
	send(1)
end)

-- The settings changed by the window or a command: kept, and shown.
local function change(f)
	local s = settings()
	f(s)
	save(s)
	refresh()
end

-- Turn logging in on or off; on with no character chosen chooses the one in the world, or the
-- first on the list.
local function switch(on)
	change(function(s)
		s.on = on
		if on and s.character == nil then
			s.character = game.player.name or characters()[1]
		end
	end)
end

local function choose(name)
	change(function(s)
		s.character = name
		s.on = name ~= nil
	end)
end

local function add_line(line)
	if line == nil or string.match(line, "^%s*$") then
		return
	end
	change(function(s)
		table.insert(s.commands, line)
	end)
end

local function remove_line(i)
	if i == nil then
		return
	end
	if chosen_line == i then
		chosen_line = nil
	end
	change(function(s)
		table.remove(s.commands, i)
	end)
end

-- Move the chosen line by `by` (-1 up, 1 down), keeping it chosen.
local function move(by)
	change(function(s)
		local i = chosen_line
		local j = i and i + by
		if i == nil or s.commands[j] == nil then
			return
		end
		s.commands[i], s.commands[j] = s.commands[j], s.commands[i]
		chosen_line = j
	end)
end

-- The character after the one in the world (or after the chosen one, at the character list).
local function next_character()
	local list = characters()
	if #list == 0 then
		return nil
	end
	local current = game.player.name or settings().character
	for i, n in ipairs(list) do
		if current ~= nil and string.lower(n) == string.lower(current) then
			return list[i % #list + 1]
		end
	end
	return list[1]
end

local function log_in_next()
	local name = next_character()
	if name == nil then
		say("No characters are listed yet.")
		return
	end
	if session.info().phase == "character_select" then
		tries = 0
		say("Logging " .. name .. " in.")
		enter(name)
		return
	end
	once = name
	say("Switching to " .. name .. ".")
	session.logout()
end

on("ui.changed", function(e)
	if e.window ~= "main" then
		return
	end
	if e.id == "on" then
		switch(e.value)
	elseif e.id == "character" then
		choose(e.value ~= "" and e.value or nil)
	end
end)

on("ui.selected", function(e)
	if e.window == "main" and e.id == "commands" then
		chosen_line = e.rows and e.rows[1]
	end
end)

on("ui.clicked", function(e)
	if e.window ~= "main" then
		return
	end
	if e.id == "add" then
		add_line(win:get("line").value)
		win:get("line").value = ""
	elseif e.id == "remove" then
		remove_line(chosen_line)
	elseif e.id == "up" then
		move(-1)
	elseif e.id == "down" then
		move(1)
	elseif e.id == "next" then
		log_in_next()
	end
end)

local function show()
	local s = settings()
	chat.print("[Auto-login] " .. ((s.on and s.character) and ("Logs in " .. s.character) or "Off") .. "; " .. #s.commands .. " commands after login.")
	for i, line in ipairs(s.commands) do
		chat.print(string.format("[Auto-login] %d. %s", i, line))
	end
end

commands.add("autologin", "/autologin [set [name] | on | off | cmd add <line> | cmd remove <n> | cmd clear | next]", function(args, words)
	if words[1] == "set" then
		choose(words[2] and string.sub(args, 5) or game.player.name)
	elseif words[1] == "on" or words[1] == "off" then
		switch(words[1] == "on")
	elseif words[1] == "cmd" and words[2] == "add" then
		add_line(string.match(args, "^%s*cmd%s+add%s+(.+)$"))
	elseif words[1] == "cmd" and words[2] == "remove" then
		remove_line(tonumber(words[3]))
	elseif words[1] == "cmd" and words[2] == "clear" then
		change(function(s)
			s.commands = {}
		end)
	elseif words[1] == "next" then
		log_in_next()
		return
	elseif words[1] == nil then
		refresh()
		win:show()
	end
	show()
end)

refresh()
