-- ID Notices: appraises items as their containers open and tells the player about the ones that
-- matter, and notes players, portals and the names it watches for coming into view, in the manner
-- of Alinco. Appraisals need no gesture: they only read, and the host holds them to five a second.
--
-- What matters is kept in the character's store and changed in its window or with /idn: a value
-- above which any item is worth a notice; words that, found in a spell's name, make it one (Epic,
-- Legendary and Major by default); rares; the names it watches for (a creature, a person or an
-- item whose name holds one, or is it, for a name written in quotes: Alinco's lists of monsters
-- and trophies); and spell scrolls of a spell the character does not know, from a level up. Each
-- kind of notice has its alert: a chat line and the HUD (`chat`), those and a toast (`toast`), or
-- nothing (`off`). The recent notices stand on a HUD, each in its kind's color; the player moves
-- it where they like by unlocking the HUDs in the plugin panel.
--
--   /idn                       open the window, and say the settings
--   /idn value <n>             the value threshold
--   /idn word <text>           add a spell word
--   /idn unword <text>         remove one
--   /idn clear                 remove every spell word
--   /idn watch <name>          watch for a name ("Name" in quotes: that name only)
--   /idn unwatch <name>        stop watching for it
--   /idn scroll <level>        notice scrolls you don't know from this level (0: none)
--   /idn alert <kind> <how>    a kind's alert: chat, toast or off

-- The kinds of notice, in the order the window lists them: what each is called there, its
-- alert by default and its color on the HUD.
local KINDS = {
	{ id = "rare", text = "Rares", how = "toast", color = "#F0C040" },
	{ id = "value", text = "Worth the value", how = "chat", color = "#80E080" },
	{ id = "spell", text = "Spell words", how = "chat", color = "#C090F0" },
	{ id = "watch", text = "Watched for", how = "toast", color = "#F09040" },
	{ id = "scroll", text = "Scrolls you don't know", how = "chat", color = "#70C0F0" },
	{ id = "player", text = "Players", how = "chat", color = "#C0C0C0" },
	{ id = "portal", text = "Portals", how = "chat", color = "#A0A0E0" },
}
local KIND = {}
for i, k in ipairs(KINDS) do
	k.order = i
	KIND[k.id] = k
end
local HOWS = { "chat", "toast", "off" }

-- A scroll's spell (the figure outside the property tables).
local SCROLL_SPELL = 0x0D000008

local function settings()
	local ok, s = pcall(function()
		return store.character.idn
	end)
	s = ok and s or {}
	s = s or {}
	s.value = s.value or 5000
	s.words = s.words or { "Legendary", "Epic", "Major" }
	s.watch = s.watch or {}
	s.scroll = s.scroll or 6
	s.alerts = s.alerts or {}
	for _, k in ipairs(KINDS) do
		s.alerts[k.id] = s.alerts[k.id] or k.how
	end
	return s
end

local function save(s)
	pcall(function()
		store.character.idn = s
	end)
end

local chosen_word, chosen_watch = nil, nil
local recent = {}
-- What was noticed this session, by id: a container opened again says nothing new.
local noticed = {}

local hud = host.has("ui.hud") and ui.hud{ id = "notices", anchor = "left",
	ui.list{ id = "list", columns = { "Notice" }, rows = {}, height = 5, width = 32 },
} or nil

local alert_rows = {}
for _, k in ipairs(KINDS) do
	table.insert(alert_rows, ui.choice{ id = "alert_" .. k.id, text = k.text, options = HOWS, value = k.how })
end
local alerts_page = { title = "Alerts" }
for _, row in ipairs(alert_rows) do
	table.insert(alerts_page, row)
end

local win = host.has("ui.window") and ui.window{ id = "main", title = "ID Notices", width = 40,
	ui.tabs{ id = "tabs",
		ui.group{ title = "Items",
			ui.number{ id = "value", text = "Worth a notice from", value = 5000, min = 0, max = 100000000, step = 1000 },
			ui.group{ title = "Spell words",
				ui.list{ id = "words", columns = { "A spell's name holding" }, rows = {}, height = 5 },
				ui.row{
					ui.input{ id = "word", text = "Word", value = "", placeholder = "Epic", grow = 1 },
					ui.button{ id = "add", text = "Add" },
					ui.button{ id = "remove", text = "Remove" },
				},
			},
			ui.number{ id = "scroll", text = "Scrolls you don't know, from level", value = 6, min = 0, max = 8, step = 1 },
		},
		ui.group{ title = "Watch",
			ui.list{ id = "watch", columns = { "Name" }, rows = {}, height = 6 },
			ui.row{
				ui.input{ id = "watchname", text = "Name", value = "", placeholder = "Olthoi Queen", grow = 1 },
				ui.toggle{ id = "exact", text = "Only that name", value = false },
			},
			ui.row{
				ui.button{ id = "watchadd", text = "Add" },
				ui.button{ id = "watchremove", text = "Remove" },
			},
		},
		ui.group(alerts_page),
	},
	ui.label{ id = "summary", text = "" },
} or nil
if win then
	win:close()
end

-- The name a watched entry is limited to, for one written in quotes; nil for any name holding it.
local function exact_of(w)
	return string.match(w, '^"(.*)"$')
end

local function refresh()
	if win == nil then
		return
	end
	local s = settings()
	win:get("value").value = s.value
	win:get("scroll").value = s.scroll
	local rows = {}
	for _, w in ipairs(s.words) do
		table.insert(rows, { w })
	end
	win:get("words").rows = rows
	if chosen_word ~= nil and s.words[chosen_word] == nil then
		chosen_word = nil
	end
	win:get("words").selected = chosen_word
	rows = {}
	for _, w in ipairs(s.watch) do
		table.insert(rows, { w })
	end
	win:get("watch").rows = rows
	if chosen_watch ~= nil and s.watch[chosen_watch] == nil then
		chosen_watch = nil
	end
	win:get("watch").selected = chosen_watch
	for _, k in ipairs(KINDS) do
		win:get("alert_" .. k.id).value = s.alerts[k.id]
	end
	win:get("summary").text = string.format("Items worth %d or more, spells naming %s, and rares.", s.value,
		#s.words > 0 and table.concat(s.words, ", ") or "nothing")
end

local function change(f)
	local s = settings()
	f(s)
	save(s)
	refresh()
end

-- `w` added to the settings' list `key` (once, whatever its case).
local function add_to(key, w)
	if w == nil or string.match(w, "^%s*$") then
		return
	end
	change(function(s)
		for _, have in ipairs(s[key]) do
			if string.lower(have) == string.lower(w) then
				return
			end
		end
		table.insert(s[key], w)
	end)
end

local function remove_from(key, w)
	change(function(s)
		for i, have in ipairs(s[key]) do
			if string.lower(have) == string.lower(w) then
				table.remove(s[key], i)
				return
			end
		end
	end)
end

local function notice(kind, text)
	local how = settings().alerts[kind] or "chat"
	if how == "off" then
		return
	end
	chat.print("[ID] " .. text, "system")
	if how == "toast" then
		pcall(ui.toast, text, 6)
	end
	table.insert(recent, 1, { { text = text, color = KIND[kind].color } })
	while #recent > 5 do
		table.remove(recent)
	end
	if hud then
		hud:get("list").rows = recent
	end
end

-- The watched entry `name` answers to, or nil: one it holds (without case), or, for an entry in
-- quotes, the name itself.
local function watched(name)
	local lower = string.lower(name or "")
	if lower == "" then
		return nil
	end
	for _, w in ipairs(settings().watch) do
		local only = exact_of(w)
		if only ~= nil then
			if string.lower(only) == lower then
				return w
			end
		elseif string.find(lower, string.lower(w), 1, true) then
			return w
		end
	end
	return nil
end

-- Why an appraised item matters: each reason its kind and its words.
local function interesting(item, a)
	local s = settings()
	local why = {}
	local value = item:int(19) or 0
	if value >= s.value then
		table.insert(why, { "value", string.format("worth %d", value) })
	end
	for _, spell in ipairs(a.spells or {}) do
		local info = magic.spell(spell)
		local name = info and info.name or ""
		for _, w in ipairs(s.words) do
			if string.find(name, w, 1, true) then
				table.insert(why, { "spell", name })
				break
			end
		end
	end
	-- Rares carry a rare id (property 17).
	if (a.ints or {})[17] ~= nil then
		table.insert(why, { "rare", "a rare" })
	end
	return why
end

-- What an item says before it is appraised: a watched name, a scroll of a spell not known.
local function on_sight(item)
	local why = {}
	if watched(item.name) ~= nil then
		table.insert(why, { "watch", "watched for" })
	end
	local s = settings()
	local spell = item.class == "Scroll" and s.scroll > 0 and item:int(SCROLL_SPELL) or nil
	local info = spell and spell > 0 and magic.spell(spell) or nil
	if info ~= nil and not info.known and (info.level or 0) >= s.scroll then
		table.insert(why, { "scroll", string.format("%s, a level %d spell you don't know", info.name, info.level or 0) })
	end
	return why
end

-- One notice of an item's reasons, under the alert of the first whose alert is not off (the
-- rarest kind leads), its words those of every such reason.
local function notice_reasons(item, why)
	local s = settings()
	table.sort(why, function(x, y)
		return KIND[x[1]].order < KIND[y[1]].order
	end)
	local words, kind = {}, nil
	for _, w in ipairs(why) do
		if s.alerts[w[1]] ~= "off" then
			kind = kind or w[1]
			table.insert(words, w[2])
		end
	end
	if kind ~= nil then
		notice(kind, item.name .. ": " .. table.concat(words, ", "))
	end
end

local function check(item)
	if noticed[item.id] then
		return
	end
	local why = on_sight(item)
	local ok, a = pawait(act.appraise(item.id))
	if ok and a ~= nil then
		for _, w in ipairs(interesting(item, a)) do
			table.insert(why, w)
		end
	end
	if #why > 0 then
		noticed[item.id] = true
		notice_reasons(item, why)
	end
end

on("inventory.container_opened", function(e)
	task.spawn(function()
		for _, item in ipairs(game.world:find{ container = e.id }) do
			check(item)
		end
	end)
end)

on("world.object_created", function(e)
	if e.id == game.player.id or noticed[e.id] then
		return
	end
	local o = game.world:get(e.id)
	-- In view, not in a container (an item in a corpse is noticed when the corpse opens).
	local in_view = o ~= nil and (o.container == nil or o.container == 0)
	if in_view and e.class ~= "Portal" and watched(e.name) ~= nil then
		noticed[e.id] = true
		notice("watch", "Watched for: " .. tostring(e.name) .. " is here")
	elseif e.class == "Player" then
		notice("player", "Player nearby: " .. tostring(e.name))
	elseif e.class == "Portal" then
		notice("portal", "Portal: " .. tostring(e.name))
	end
end)

on("session.login", function()
	refresh()
end)

-- The HUD opens with the window: a window opens only when the player asks.
on("ui.opened", function(e)
	if e.window == "main" and hud then
		hud:show()
	end
end)

on("ui.changed", function(e)
	if e.window ~= "main" then
		return
	end
	if (e.id == "value" or e.id == "scroll") and tonumber(e.value) then
		change(function(s)
			s[e.id] = tonumber(e.value)
		end)
	elseif string.sub(e.id, 1, 6) == "alert_" and KIND[string.sub(e.id, 7)] ~= nil then
		change(function(s)
			s.alerts[string.sub(e.id, 7)] = e.value
		end)
	end
end)

on("ui.selected", function(e)
	if e.window ~= "main" then
		return
	end
	if e.id == "words" then
		chosen_word = e.rows and e.rows[1]
		local w = chosen_word and settings().words[chosen_word]
		if w then
			win:get("word").value = w
		end
	elseif e.id == "watch" then
		chosen_watch = e.rows and e.rows[1]
		local w = chosen_watch and settings().watch[chosen_watch]
		if w then
			win:get("watchname").value = exact_of(w) or w
			win:get("exact").value = exact_of(w) ~= nil
		end
	end
end)

-- The watched entry the window's box and toggle name.
local function typed_watch()
	local name = string.match(win:get("watchname").value or "", "^%s*(.-)%s*$")
	if name == "" then
		return nil
	end
	return win:get("exact").value and ('"' .. name .. '"') or name
end

on("ui.clicked", function(e)
	if e.window ~= "main" then
		return
	end
	if e.id == "add" then
		add_to("words", win:get("word").value)
		win:get("word").value = ""
	elseif e.id == "remove" then
		local s = settings()
		local w = (chosen_word and s.words[chosen_word]) or win:get("word").value
		if w ~= nil and w ~= "" then
			remove_from("words", w)
			chosen_word = nil
			win:get("word").value = ""
		end
	elseif e.id == "watchadd" then
		add_to("watch", typed_watch())
		win:get("watchname").value = ""
	elseif e.id == "watchremove" then
		local w = (chosen_watch and settings().watch[chosen_watch]) or typed_watch()
		if w ~= nil then
			remove_from("watch", w)
			chosen_watch = nil
			win:get("watchname").value = ""
		end
	end
end)

-- The text after a command's first word.
local function rest_of(args)
	return string.match(args, "^%s*%S+%s+(.-)%s*$")
end

commands.add("idn", "/idn [value <n> | word <text> | unword <text> | clear | watch <name> | unwatch <name> | scroll <level> | alert <kind> <chat|toast|off>]", function(args, words)
	local w = words[1] and string.lower(words[1]) or nil
	if w == "value" and tonumber(words[2]) then
		change(function(s)
			s.value = tonumber(words[2])
		end)
	elseif w == "word" and words[2] then
		add_to("words", rest_of(args))
	elseif w == "unword" and words[2] then
		remove_from("words", rest_of(args))
	elseif w == "clear" then
		change(function(s)
			s.words = {}
		end)
	elseif w == "watch" and words[2] then
		add_to("watch", rest_of(args))
	elseif w == "unwatch" and words[2] then
		remove_from("watch", rest_of(args))
	elseif w == "scroll" and tonumber(words[2]) then
		change(function(s)
			s.scroll = math.max(0, math.min(8, math.floor(tonumber(words[2]))))
		end)
	elseif w == "alert" and words[2] and words[3] then
		local kind, how = string.lower(words[2]), string.lower(words[3])
		if KIND[kind] == nil or (how ~= "chat" and how ~= "toast" and how ~= "off") then
			local ids = {}
			for _, k in ipairs(KINDS) do
				table.insert(ids, k.id)
			end
			chat.print("ID notices: /idn alert <" .. table.concat(ids, " | ") .. "> <chat | toast | off>")
		else
			change(function(s)
				s.alerts[kind] = how
			end)
		end
	elseif w == nil and win then
		refresh()
		win:show()
	end
	local s = settings()
	chat.print(string.format("ID notices: value %d, words %s", s.value, table.concat(s.words, ", ")))
	local alerts = {}
	for _, k in ipairs(KINDS) do
		table.insert(alerts, k.id .. " " .. s.alerts[k.id])
	end
	chat.print(string.format("ID notices: watching for %s; scrolls you don't know %s; alerts %s",
		#s.watch > 0 and table.concat(s.watch, ", ") or "nothing",
		s.scroll > 0 and ("from level " .. s.scroll) or "off", table.concat(alerts, ", ")))
end)

refresh()
