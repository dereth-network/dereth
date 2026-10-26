-- Chat Filter: rules for the chat the character sees. A rule matches a line by its words, its
-- channel or who said it, and hides the line, highlights it (shows it again in another style) or
-- keeps it in a log. Its window holds the rules (added, changed, removed, each on or off) and the
-- log; the commands are shortcuts:
--
--   /chatlog [n]                  the last n logged lines (10 when n is left out)
--   /chatfilter add <words>       hide incoming lines containing <words> (case does not matter)
--   /chatfilter highlight <words> highlight them
--   /chatfilter log <words>       keep them in the log
--   /chatfilter remove <words>    forget the rules on those words
--   /chatfilter list              the rules
--   /chatfilter on | off          switch the rules without forgetting them
--
-- It took the place of the Chat Logger: what that kept (its filter words, its log, its switch) is
-- read here as rules the first time, and since it logged every line, a rule logging every line
-- is among them.

local LOG_LINES = 300

-- The styles a highlighted line can be shown again in: chat kinds of their own colours.
local STYLES = { "help", "advancement", "magic", "fellowship", "world_broadcast", "system" }
local MATCHES = { "text", "channel", "sender" }
local ACTIONS = { "hide", "highlight", "log" }

local rules = {}
local log = {}
local filtering = true
local hidden = 0
local chosen = nil -- the rule chosen in the window's list
local win = nil
-- The character's store opens at login; lines that arrive before it (the server's welcome) are
-- matched by no rule.
local logged_in = false

-- A new character's rules: tells kept in the log.
local function default_rules()
	return { { match = "channel", value = "tell", action = "log", on = true } }
end

local function save()
	if not logged_in then
		return
	end
	store.character.rules = rules
	store.character.log = log
	store.character.filtering = filtering
end

local function load()
	local c = store.character
	rules = c.rules
	if rules == nil then
		if c.filters ~= nil or c.log ~= nil then
			-- The Chat Logger's: its words hid lines, and it kept every line.
			rules = {}
			for _, words in ipairs(c.filters or {}) do
				table.insert(rules, { match = "text", value = words, action = "hide", on = true })
			end
			table.insert(rules, { match = "text", value = "", action = "log", on = true })
			c.filters = nil
		else
			rules = default_rules()
		end
	end
	log = c.log or {}
	local f = c.filtering
	filtering = f == nil or f
	hidden = 0
	chosen = nil
	save()
end

-- Who said a chat line, as far as its words tell: a speaker before "says," or "tells you,", or
-- a channel's speaker after its name in brackets. A name the chat window makes a link of is read
-- without its link. "you" for the player's own.
local function speaker(text)
	text = string.gsub(text or "", "<[^>]*>", "")
	if string.match(text, "^You say") or string.match(text, "^%[.-%]%s*You say") or string.match(text, "^You tell ") then
		return "you"
	end
	return string.match(text, "^%[.-%]%s*(.-) says, ")
		or string.match(text, "^(.-) says on the .- channel, ")
		or string.match(text, "^(.-) says, ")
		or string.match(text, "^(.-) tells you, ")
		or string.match(text, "^(.-) yells, ")
end

-- Whether rule `r` matches line `e`, case aside: its words in the text, its channel by the line's
-- kind or the channel's name in brackets at the line's start, its sender by name.
local function matches(r, e)
	local want = string.lower(r.value or "")
	local text = string.lower(e.text or "")
	if r.match == "text" then
		return string.find(text, want, 1, true) ~= nil
	elseif r.match == "channel" then
		return string.lower(tostring(e.kind)) == want
			or string.sub(text, 1, #want + 2) == "[" .. want .. "]"
			or string.lower(e.prefix or "") == "[" .. want .. "]"
	elseif r.match == "sender" then
		local who = speaker(e.text)
		return who ~= nil and string.lower(who) == want
	end
	return false
end

local function rule_text(r)
	local value = r.value ~= "" and r.value or "(any line)"
	local action = r.action == "highlight" and ("highlight as " .. (r.style or "help")) or r.action
	return string.format("%s %s: %s", r.match, value, action)
end

local function refresh()
	if win == nil then
		return
	end
	local rows = {}
	for _, r in ipairs(rules) do
		table.insert(rows, {
			r.on and "on" or "off",
			r.match,
			r.value ~= "" and r.value or "(any line)",
			r.action == "highlight" and ("highlight: " .. (r.style or "help")) or r.action,
		})
	end
	win:get("rules").rows = rows
	if chosen ~= nil and rules[chosen] == nil then
		chosen = nil
	end
	win:get("rules").selected = chosen
	win:get("filtering").value = filtering
	win:get("hidden").text = string.format("Hidden this session: %d", hidden)
	local lines = {}
	for _, entry in ipairs(log) do
		table.insert(lines, { entry.time or "", tostring(entry.kind), entry.text })
	end
	win:get("log").rows = lines
	win:get("logged").text = string.format("%d lines kept (at most %d).", #log, LOG_LINES)
end

-- The rule the window's fields describe.
local function rule_from_fields()
	return {
		match = win:get("match").value,
		value = win:get("value").value or "",
		action = win:get("action").value,
		style = win:get("style").value,
		on = true,
	}
end

if host.has("ui.window") then
	win = ui.window({
		id = "main",
		title = "Chat Filter",
		width = 60,
		ui.tabs({
			id = "pages",
			selected = 1,
			ui.group({
				title = "Rules",
				ui.row({
					ui.toggle({ id = "filtering", text = "Rules on", value = true }),
					ui.label({ id = "hidden", text = "Hidden this session: 0", grow = 1 }),
				}),
				ui.list({ id = "rules", columns = { { title = "On", width = 4 }, { title = "Match", width = 8 }, "Value",
					{ title = "Action", width = 16 } }, rows = {}, height = 6 }),
				ui.choice({ id = "match", text = "Match", options = MATCHES, value = "text" }),
				ui.input({ id = "value", text = "Value", value = "", placeholder = "words, a channel (Trade, tell) or a name" }),
				ui.choice({ id = "action", text = "Action", options = ACTIONS, value = "hide" }),
				ui.choice({ id = "style", text = "Highlight as", options = STYLES, value = "help" }),
				ui.row({
					ui.button({ id = "add", text = "Add" }),
					ui.button({ id = "save", text = "Save" }),
					ui.button({ id = "remove", text = "Remove" }),
					ui.button({ id = "switch", text = "On / off" }),
				}),
			}),
			ui.group({
				title = "Log",
				ui.list({ id = "log", columns = { { title = "Time", width = 8 }, { title = "Kind", width = 10 }, "Line" },
					rows = {}, height = 12 }),
				ui.row({
					ui.label({ id = "logged", text = "", grow = 1 }),
					ui.button({ id = "clear", text = "Clear the log" }),
				}),
			}),
		}),
	})

	on("ui.changed", function(e)
		if e.window == "main" and e.id == "filtering" then
			filtering = e.value
			save()
			refresh()
		end
	end)

	on("ui.selected", function(e)
		if e.window ~= "main" or e.id ~= "rules" then
			return
		end
		chosen = e.rows and e.rows[1]
		local r = chosen and rules[chosen]
		if r ~= nil then
			win:get("match").value = r.match
			win:get("value").value = r.value
			win:get("action").value = r.action
			win:get("style").value = r.style or "help"
		end
	end)

	on("ui.clicked", function(e)
		if e.window ~= "main" then
			return
		end
		if e.id == "add" then
			table.insert(rules, rule_from_fields())
			chosen = #rules
		elseif e.id == "save" and chosen ~= nil then
			local r = rule_from_fields()
			r.on = rules[chosen].on
			rules[chosen] = r
		elseif e.id == "remove" and chosen ~= nil then
			table.remove(rules, chosen)
			chosen = nil
		elseif e.id == "switch" and chosen ~= nil then
			rules[chosen].on = not rules[chosen].on
		elseif e.id == "clear" then
			log = {}
		end
		save()
		refresh()
	end)
end

on("session.login", function()
	logged_in = true
	load()
	refresh()
end)

on("session.logout", function()
	logged_in = false
end)

on("chat.line", function(e)
	if not logged_in or not filtering then
		return
	end
	local hide, highlight, keep = false, nil, false
	for _, r in ipairs(rules) do
		if r.on and matches(r, e) then
			if r.action == "hide" then
				hide = true
			elseif r.action == "highlight" then
				highlight = highlight or (r.style or "help")
			elseif r.action == "log" then
				keep = true
			end
		end
	end
	if keep then
		table.insert(log, { time = os.date("%H:%M:%S"), kind = tostring(e.kind), text = e.text })
		while #log > LOG_LINES do
			table.remove(log, 1)
		end
		save()
	end
	if hide then
		chat.hide(e)
		hidden = hidden + 1
	elseif highlight then
		-- Shown again in the highlight's style, in its place.
		chat.hide(e)
		chat.print(e.text, highlight)
	end
	if hide or keep then
		refresh()
	end
end)

commands.add("chatlog", "/chatlog [n] shows the last n logged lines", function(args)
	local n = tonumber(args) or 10
	if #log == 0 then
		chat.print("The chat log is empty.", "system")
		return
	end
	for i = math.max(1, #log - n + 1), #log do
		chat.print(string.format("[%s] %s", log[i].kind, log[i].text), "system")
	end
end)

local VERBS = { add = "hide", highlight = "highlight", log = "log" }

commands.add("chatfilter", "/chatfilter add|highlight|log|remove <words>, /chatfilter list, /chatfilter on|off", function(args, words)
	local verb = words[1]
	local rest = string.match(args, "^%S+%s+(.+)$")
	if VERBS[verb] and rest then
		table.insert(rules, { match = "text", value = rest, action = VERBS[verb], style = "help", on = true })
		chat.print("Rule: " .. rule_text(rules[#rules]), "system")
	elseif verb == "remove" and rest then
		local gone = 0
		for i = #rules, 1, -1 do
			if rules[i].match == "text" and string.lower(rules[i].value) == string.lower(rest) then
				table.remove(rules, i)
				gone = gone + 1
			end
		end
		chat.print(gone > 0 and ("No longer matching: " .. rest) or ("No rule on: " .. rest), "system")
	elseif verb == "list" then
		if #rules == 0 then
			chat.print("No rules.", "system")
		end
		for i, r in ipairs(rules) do
			chat.print(string.format("%d. %s%s", i, rule_text(r), r.on and "" or " (off)"), "system")
		end
	elseif verb == "on" or verb == "off" then
		filtering = verb == "on"
		chat.print("The rules are " .. verb .. ".", "system")
	else
		chat.print("/chatfilter add|highlight|log|remove <words>, /chatfilter list, /chatfilter on|off", "system")
		return
	end
	save()
	refresh()
end)

-- Loaded with a character already in the world: its rules now.
if game.player.id ~= nil then
	logged_in = true
	load()
	refresh()
end
