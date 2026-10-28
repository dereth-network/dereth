-- Inventory: the inventory tools of Mag-Tools, for every character of the world.
--
-- - The log: what each character holds and wears, kept for the world (log.lua), and searched from
--   any of them: by words of an item's name, its spells or its set.
-- - Item information: when the player appraises what they selected, a line of what it is
--   (info.lua).
-- - Stacking (partial stacks merged), packing by rules (an item put in the pack its name's rule
--   names), sets of gear put on at once, and trading at a vendor by the vendor's own loot profile
--   (`vendors/<its name>.utl` in the data folder), as soon as its wares show (tools.lua).
-- - The suit builder: from every character's logged gear, the best suits for this character by
--   the spells and the set asked for (suits.lua).
--
-- /inv                       open the window
-- /inv find <words>          the items of every character holding the words
-- /inv log                   log this character now
-- /inv chars                 the characters logged
-- /inv forget <character>    forget a character's log
-- /inv info <item>           the information of a logged item, by part of its name
-- /inv stack                 merge partial stacks now
-- /inv pack                  put items in their rules' packs now
-- /inv trade                 trade with the open vendor by its profile now
-- /inv gear                  the sets of gear; /inv gear save|wear|remove <name>
-- /inv suit                  build the best suits by SuitSet, SuitSpells and SuitFrom
-- /inv suit lock <words>     keep a logged piece in every suit; /inv suit unlock [<words>]
-- /inv suit wear [n]         put on the pieces of suit n (1: the best) this character holds
-- /inv set <setting> <value> a setting; /inv settings lists them

local L = require("log")
local I = require("info")
local T = require("tools")
local Suits = require("suits")

-- The settings: name -> default.
local DEFAULTS = {
	Info = true,
	Appraise = true,
	AutoStack = false,
	AutoPack = false,
	PackRules = "",
	AutoTrade = false,
	SuitSet = "",
	SuitSpells = "",
	SuitFrom = "",
}

local function settings()
	local s = store.character.inv_settings or {}
	for k, v in pairs(DEFAULTS) do
		if type(s[k]) ~= type(v) then
			s[k] = v
		end
	end
	return s
end

local function get(name)
	return settings()[name]
end

local function setting_name(name)
	local want = string.lower(tostring(name or ""))
	for k in pairs(DEFAULTS) do
		if string.lower(k) == want then
			return k
		end
	end
	return nil
end

local refresh

-- Set a setting from a value or the text typed for it; answers the line to tell.
local function set(name, value)
	local n = setting_name(name)
	if n == nil then
		return "no setting " .. tostring(name)
	end
	local kind = type(DEFAULTS[n])
	if type(value) == "string" and kind == "boolean" then
		local v = string.lower(value)
		value = (v == "true" or v == "on" or v == "yes" or v == "1") and true
			or ((v == "false" or v == "off" or v == "no" or v == "0") and false or nil)
	end
	if type(value) ~= kind then
		return n .. " takes a " .. kind
	end
	local s = settings()
	s[n] = value
	store.character.inv_settings = s
	if refresh then
		refresh()
	end
	return string.format("%s = %s", n, tostring(value))
end

-- A line for the player: what the plugin prints is told in the chat under its name.
local function say(fmt, ...)
	print(string.format(fmt, ...))
end

local function where_text(w)
	if w == "worn" then
		return "worn"
	elseif w == 1 or w == nil then
		return "main pack"
	end
	return "pack " .. tostring(w)
end

---------------------------------------------------------------------------------------------------
-- The window.

local win = ui.window({
	id = "main",
	title = "Inventory",
	width = 64,
	height = 26,
	ui.tabs({
		id = "tabs",
		ui.group({
			title = "Find",
			ui.row({
				ui.input({ id = "find", text = "Find", value = "", placeholder = "epic strength", grow = 1 }),
				ui.button({ id = "search", text = "Search" }),
			}),
			ui.label({ id = "found", text = "" }),
			ui.list({ id = "results", columns = { "Item", "Count", "Character", "Where" }, rows = {}, grow = 1 }),
			ui.label({ id = "detail", text = "" }),
		}),
		ui.group({
			title = "Characters",
			ui.list({ id = "chars", columns = { "Character", "Items", "Level", "Logged" }, rows = {}, grow = 1 }),
			ui.row({
				ui.button({ id = "lognow", text = "Log this character now" }),
				ui.button({ id = "forget", text = "Forget the one chosen" }),
			}),
		}),
		ui.group({
			title = "Tools",
			ui.toggle({ id = "Info", text = "Say what an item is when you appraise it", value = true }),
			ui.toggle({ id = "Appraise", text = "Appraise your gear for the log", value = true }),
			ui.toggle({ id = "AutoStack", text = "Merge partial stacks as items come", value = false }),
			ui.toggle({ id = "AutoPack", text = "Put items in their packs as they come", value = false }),
			ui.input({ id = "PackRules", text = "Pack rules (words=pack, ...)", value = "", placeholder = "Scarab=2, Taper=2, Salvage=3" }),
			ui.toggle({ id = "AutoTrade", text = "Trade at a vendor by its profile when its wares show", value = false }),
			ui.row({
				ui.button({ id = "stack", text = "Stack now" }),
				ui.button({ id = "pack", text = "Pack now" }),
				ui.button({ id = "trade", text = "Trade now" }),
			}),
			ui.label({ id = "toolstate", text = "" }),
		}),
		ui.group({
			title = "Suits",
			ui.input({ id = "SuitSpells", text = "Spells", value = "", placeholder = "Strength, Invulnerability, Heavy Weapon" }),
			ui.input({ id = "SuitSet", text = "Set", value = "", placeholder = "Defender's" }),
			ui.input({ id = "SuitFrom", text = "From (empty: every character)", value = "", placeholder = "Alice, Bob" }),
			ui.row({
				ui.button({ id = "build", text = "Build" }),
				ui.button({ id = "suitwear", text = "Put on what is yours" }),
				ui.button({ id = "lock", text = "Lock the piece chosen" }),
				ui.button({ id = "unlockall", text = "Unlock all" }),
			}),
			ui.label({ id = "suitstate", text = "" }),
			ui.list({ id = "suits", columns = { "Suit", "What it holds" }, rows = {}, height = 5 }),
			ui.list({ id = "suitpieces", columns = { "Where", "Item", "Character" }, rows = {}, grow = 1 }),
		}),
		ui.group({
			title = "Gear",
			ui.list({ id = "gear", columns = { "Set", "Items" }, rows = {}, grow = 1 }),
			ui.input({ id = "gearname", text = "Name", value = "", placeholder = "melee" }),
			ui.row({
				ui.button({ id = "gearsave", text = "Keep what you wear" }),
				ui.button({ id = "gearwear", text = "Put it on" }),
				ui.button({ id = "gearremove", text = "Remove" }),
			}),
		}),
	}),
})
win:close()

local results = {}
local chosen_char = nil

local function show_results(text)
	results = L.find(text, I.set_name)
	local rows = {}
	for i, f in ipairs(results) do
		if i > 500 then
			break
		end
		table.insert(rows, { f.item.n, f.item.k or 1, f.character, where_text(f.item.w) })
	end
	win:get("results").rows = rows
	win:get("results").selected = nil
	win:get("found").text = string.format("%d found in %d characters' logs.", #results, #L.characters())
	win:get("detail").text = ""
	return results
end

refresh = function()
	local s = settings()
	for _, k in ipairs({ "Info", "Appraise", "AutoStack", "AutoPack", "PackRules", "AutoTrade", "SuitSet", "SuitSpells", "SuitFrom" }) do
		win:get(k).value = s[k]
	end
	local rows = {}
	for _, c in ipairs(L.summaries()) do
		table.insert(rows, { c.name, c.items or 0, c.level or "", c.at and os.date("%Y-%m-%d %H:%M", c.at) or "" })
	end
	win:get("chars").rows = rows
	local gear = {}
	for _, name in ipairs(T.set_names()) do
		local names = {}
		for _, it in ipairs(T.set_items(name)) do
			table.insert(names, it.name)
		end
		table.insert(gear, { name, table.concat(names, ", ") })
	end
	win:get("gear").rows = gear
end

---------------------------------------------------------------------------------------------------
-- Suits.

-- The suits found last, best first; the one chosen in the window; its piece chosen.
local suits, chosen_suit, chosen_piece = {}, nil, nil

-- The pieces locked into every suit, by key ("<character>:<item id>").
local function locks()
	return store.character.inv_suit_locks or {}
end

local function show_suit(n)
	chosen_suit = n
	chosen_piece = nil
	local suit = suits[n]
	local rows = {}
	for _, p in ipairs(suit and suit.pieces or {}) do
		local c = p.candidate
		table.insert(rows, { Suits.place_names(p.place), c.item.n .. (locks()[c.key] and " (locked)" or ""), c.owner })
	end
	win:get("suitpieces").rows = rows
end

-- Build the best suits by the settings (inside a task); answers the lines to say.
local function build()
	local records = {}
	for _, name in ipairs(L.characters()) do
		table.insert(records, L.record(name))
		task.sleep(0)
	end
	local from = nil
	if get("SuitFrom") ~= "" then
		from = {}
		for part in string.gmatch(get("SuitFrom"), "[^,]+") do
			from[string.lower(string.match(part, "^%s*(.-)%s*$"))] = true
		end
	end
	local set = Suits.wanted_set(get("SuitSet"), I.SETS)
	local opt = { me = game.player.name, level = game.player_int(25), from = from,
		spells = Suits.wanted_spells(get("SuitSpells")), set = set }
	local cands = Suits.candidates(records, opt)
	local found, tried, stopped, refused = Suits.search(cands, opt, locks())
	suits = found
	local rows = {}
	local set_name = set and I.set_name(set) or nil
	for i, suit in ipairs(suits) do
		table.insert(rows, { i, Suits.summary(suit, set_name) })
	end
	win:get("suits").rows = rows
	show_suit(#suits > 0 and 1 or nil)
	local lines = { string.format("%d pieces from %d characters; %d suits tried%s", #cands, #records, tried,
		stopped and " (the limit; the best found so far)" or "") }
	for _, c in ipairs(refused) do
		table.insert(lines, string.format("the locked %s (%s) does not fit with the other locked pieces", c.item.n, c.owner))
	end
	local best = suits[1]
	if best ~= nil then
		table.insert(lines, "best: " .. Suits.summary(best, set_name))
		for _, p in ipairs(best.pieces) do
			table.insert(lines, string.format("  %s: %s (%s)", Suits.place_names(p.place), p.candidate.item.n, p.candidate.owner))
		end
	end
	win:get("suitstate").text = lines[1]
	return lines
end

-- Lock the first logged piece holding `words` (inside a task); answers the line to say.
local function lock(words)
	local found = L.find(words, I.set_name)
	for _, f in ipairs(found) do
		if (f.item.loc or 0) ~= 0 then
			local all = locks()
			all[f.character .. ":" .. tostring(f.item.id)] = true
			store.character.inv_suit_locks = all
			return string.format("locked %s (%s) into every suit", f.item.n, f.character)
		end
	end
	return "no logged piece of gear holds " .. tostring(words)
end

local function unlock(words)
	if words == nil or words == "" then
		store.character.inv_suit_locks = {}
		return "no piece locked"
	end
	local all, n = locks(), 0
	for _, f in ipairs(L.find(words, I.set_name)) do
		local key = f.character .. ":" .. tostring(f.item.id)
		if all[key] then
			all[key] = nil
			n = n + 1
		end
	end
	store.character.inv_suit_locks = all
	return string.format("unlocked %d", n)
end

-- Put on the pieces of suit `n` this character holds (inside a task); answers the line to say.
local function wear_suit(n)
	local suit = suits[n or 1]
	if suit == nil then
		return "no suit built (/inv suit)"
	end
	local me = string.lower(game.player.name or "")
	local held, worn, others = {}, 0, {}
	for _, it in ipairs(game.inventory.items()) do
		held[it.id] = it
	end
	for _, p in ipairs(suit.pieces) do
		local c = p.candidate
		local it = string.lower(c.owner) == me and held[c.item.id] or nil
		if it == nil then
			table.insert(others, string.format("%s (%s)", c.item.n, c.owner))
		elseif it.wielder ~= game.player.id then
			local ok = pawait(items.wield(it.id))
			if ok then
				worn = worn + 1
			end
		end
	end
	local line = string.format("suit %d: %d put on", n or 1, worn)
	if #others > 0 then
		line = line .. "; held by others: " .. table.concat(others, ", ")
	end
	return line
end

---------------------------------------------------------------------------------------------------
-- The work: the log, appraisals, stacking and packing, trading. One task does it, a step every
-- quarter second (appraisals four a second, under the host's five).

-- The log is taken again five seconds after the first change since it was last taken.
local dirty, dirty_at = true, -100
local LOG_AFTER = 5
local stack_wanted, pack_wanted, trade_wanted = false, false, false
-- What stacking and packing leave this pass: a name or an item that failed three times.
local failures, skip_names, skip_items = {}, {}, {}

local function changed()
	if not dirty then
		dirty, dirty_at = true, os.clock()
	end
	stack_wanted = stack_wanted or get("AutoStack")
	pack_wanted = pack_wanted or get("AutoPack")
end

-- A stacking or packing step's outcome: three failures of one name or item leave it this pass.
local function failed(skip, what, ok)
	if ok or what == nil then
		return
	end
	failures[what] = (failures[what] or 0) + 1
	if failures[what] >= 3 then
		skip[what] = true
	end
end

local function step()
	if game.player.id == nil then
		return
	end
	if trade_wanted then
		trade_wanted = false
		local line = T.trade()
		if line ~= nil then
			say("%s", line)
		end
		return
	end
	if stack_wanted then
		local line, name, ok = T.stack_step(skip_names)
		if line ~= nil then
			print(line)
			failed(skip_names, name, ok)
			return
		end
		stack_wanted, skip_names = false, {}
	end
	if pack_wanted then
		local line, id, ok = T.pack_step(T.pack_rules(get("PackRules")), skip_items)
		if line ~= nil then
			print(line)
			failed(skip_items, id, ok)
			return
		end
		pack_wanted, skip_items = false, {}
	end
	if dirty and os.clock() - dirty_at >= LOG_AFTER then
		dirty = false
		local rec = L.take()
		print(string.format("logged %s: %d items", rec.name, #rec.items))
		refresh()
		return
	end
	if get("Appraise") then
		local it = L.next_unappraised()
		if it ~= nil then
			pawait(act.appraise(it.id))
			changed()
		end
	end
end

task.spawn(function()
	local function where(e)
		return debug ~= nil and debug.traceback(tostring(e), 2) or tostring(e)
	end
	while true do
		local ok, err = xpcall(step, where)
		if not ok then
			print("error: " .. tostring(err))
		end
		task.sleep(250)
	end
end)

---------------------------------------------------------------------------------------------------
-- Events.

on("context.begin", function()
	changed()
	refresh()
end)

for _, name in ipairs({ "inventory.item_added", "inventory.item_removed", "inventory.item_moved" }) do
	on(name, function()
		changed()
	end)
end

-- The changes to an item that its record shows.
local LOGGED = { name = true, stack = true, value = true, burden = true, appraisal = true }

on("world.object_changed", function(e)
	local logged = false
	for _, k in ipairs(e.changed or {}) do
		logged = logged or LOGGED[k] == true
	end
	if not logged then
		return
	end
	local o = game.world:get(e.id)
	if o ~= nil and (o.container ~= nil or o.wielder == game.player.id) then
		for _, it in ipairs(game.inventory.items()) do
			if it.id == e.id then
				if not dirty then
					dirty, dirty_at = true, os.clock()
				end
				return
			end
		end
	end
end)

on("items.appraised", function(e)
	if not get("Info") then
		return
	end
	local sel = game.selection()
	if sel == nil or sel.id ~= e.id then
		return
	end
	local o = game.world:get(e.id)
	if o ~= nil and L.GEAR[o.class] then
		chat.print("[Inventory] " .. I.describe(L.entry(o, nil)))
	end
end)

on("commerce.vendor_opened", function()
	if get("AutoTrade") then
		trade_wanted = true
	end
end)

on("ui.changed", function(e)
	if e.window == "main" and setting_name(e.id) ~= nil then
		print(set(e.id, e.value))
		if e.id == "AutoStack" and e.value then
			stack_wanted = true
		elseif e.id == "AutoPack" and e.value then
			pack_wanted = true
		end
	end
end)

on("ui.selected", function(e)
	if e.window ~= "main" or e.rows == nil then
		return
	end
	if e.id == "results" then
		local f = results[e.rows[1]]
		win:get("detail").text = f and (f.character .. ": " .. I.describe(f.item)) or ""
	elseif e.id == "chars" then
		chosen_char = L.characters()[e.rows[1]]
	elseif e.id == "gear" then
		local name = T.set_names()[e.rows[1]]
		if name then
			win:get("gearname").value = name
		end
	elseif e.id == "suits" then
		show_suit(e.rows[1])
	elseif e.id == "suitpieces" then
		chosen_piece = e.rows[1]
	end
end)

on("ui.clicked", function(e)
	if e.window ~= "main" then
		return
	end
	if e.id == "search" then
		local text = win:get("find").value
		task.spawn(function()
			show_results(text)
		end)
	elseif e.id == "lognow" then
		dirty, dirty_at = true, -100
	elseif e.id == "forget" and chosen_char ~= nil then
		L.forget(chosen_char)
		chosen_char = nil
		refresh()
	elseif e.id == "stack" then
		stack_wanted = true
	elseif e.id == "pack" then
		pack_wanted = true
	elseif e.id == "trade" then
		trade_wanted = true
	elseif e.id == "gearsave" then
		say("%s", T.save_set(win:get("gearname").value))
		refresh()
	elseif e.id == "gearwear" then
		local name = win:get("gearname").value
		task.spawn(function()
			say("%s", T.wear_set(name))
		end)
	elseif e.id == "gearremove" then
		say("%s", T.remove_set(win:get("gearname").value))
		refresh()
	elseif e.id == "build" then
		win:get("suitstate").text = "Building..."
		task.spawn(function()
			build()
		end)
	elseif e.id == "suitwear" then
		local n = chosen_suit
		task.spawn(function()
			say("%s", wear_suit(n))
		end)
	elseif e.id == "lock" then
		local suit = chosen_suit and suits[chosen_suit]
		local p = suit and chosen_piece and suit.pieces[chosen_piece]
		if p ~= nil then
			local all = locks()
			all[p.candidate.key] = true
			store.character.inv_suit_locks = all
			say("locked %s (%s) into every suit", p.candidate.item.n, p.candidate.owner)
			show_suit(chosen_suit)
		end
	elseif e.id == "unlockall" then
		say("%s", unlock(nil))
		show_suit(chosen_suit)
	end
end)

---------------------------------------------------------------------------------------------------
-- The command.

local function rest(args)
	return string.match(args or "", "^%s*%S+%s+(.-)%s*$")
end

local USAGE = "/inv [find <words> | log | chars | forget <character> | info <item> | stack | pack | trade | gear [save|wear|remove <name>] | suit [lock|unlock <words> | wear [n]] | set <setting> <value> | settings]"

commands.add("inv", USAGE, function(args, words)
	local w = string.lower(words[1] or "")
	if w == "" then
		refresh()
		win:show()
		say("%d characters logged", #L.characters())
	elseif w == "find" and words[2] ~= nil then
		local text = rest(args)
		task.spawn(function()
			local found = show_results(text)
			say("%d found", #found)
			for i, f in ipairs(found) do
				if i > 20 then
					say("... and %d more (the window lists them all)", #found - 20)
					break
				end
				say("%s: %s%s (%s)", f.character, f.item.n, (f.item.k or 1) > 1 and (" x" .. f.item.k) or "", where_text(f.item.w))
			end
		end)
	elseif w == "log" then
		dirty, dirty_at = true, -100
		say("logging this character")
	elseif w == "chars" then
		for _, c in ipairs(L.summaries()) do
			say("%s: %d items%s", c.name, c.items or 0, c.level and (", level " .. c.level) or "")
		end
	elseif w == "forget" and words[2] ~= nil then
		local name = rest(args)
		say(L.forget(name) and ("forgot " .. name) or ("no log of " .. name))
		refresh()
	elseif w == "info" and words[2] ~= nil then
		local text = rest(args)
		task.spawn(function()
			local found = L.find(text, I.set_name)
			for i, f in ipairs(found) do
				if i > 5 then
					break
				end
				say("%s: %s", f.character, I.describe(f.item))
			end
			if #found == 0 then
				say("no logged item holds %s", text)
			end
		end)
	elseif w == "stack" then
		stack_wanted = true
	elseif w == "pack" then
		pack_wanted = true
	elseif w == "trade" then
		trade_wanted = true
	elseif w == "gear" then
		local verb, name = string.lower(words[2] or ""), table.concat(words, " ", 3)
		if verb == "save" then
			say("%s", T.save_set(name))
		elseif verb == "wear" then
			task.spawn(function()
				say("%s", T.wear_set(name))
			end)
		elseif verb == "remove" then
			say("%s", T.remove_set(name))
		else
			for _, n in ipairs(T.set_names()) do
				local names = {}
				for _, it in ipairs(T.set_items(n)) do
					table.insert(names, it.name)
				end
				say("gear %s: %s", n, table.concat(names, ", "))
			end
		end
		refresh()
	elseif w == "suit" then
		local verb = string.lower(words[2] or "")
		local what = table.concat(words, " ", 3)
		task.spawn(function()
			if verb == "lock" then
				say("%s", lock(what))
			elseif verb == "unlock" then
				say("%s", unlock(what))
			elseif verb == "wear" then
				say("%s", wear_suit(tonumber(words[3]) or 1))
			else
				for _, l in ipairs(build()) do
					say("%s", l)
				end
			end
		end)
	elseif w == "set" and words[3] ~= nil then
		say("%s", set(words[2], table.concat(words, " ", 3)))
	elseif w == "settings" then
		local names = {}
		for k in pairs(DEFAULTS) do
			table.insert(names, k)
		end
		table.sort(names)
		for _, k in ipairs(names) do
			say("%s = %s", k, tostring(get(k)))
		end
	else
		say("%s", USAGE)
	end
end)
