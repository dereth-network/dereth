-- Trackers: the trackers of Mag-Tools, fed by the chat, the world and appraisals. Combat (damage
-- dealt and taken per creature and in all, by element, the share of attacks that landed, evaded
-- and resisted, critical hits and killing blows), experience and luminance per hour, the mana of
-- worn items and how long it lasts, the consumables held and how fast they go, the corpses seen
-- (your own kept, to find it again) and the players seen. One window with a page each, and a
-- headline on a HUD. It only reads.

local util = require("util")
local combat = require("combat")
local experience = require("experience")
local mana = require("mana")
local items = require("items")
local corpses = require("corpses")
local players = require("players")

local session = combat.tally()
local lifetime = combat.tally()
local lifetime_dirty = false
-- Creature name -> its tally, with `last_hit` (when your last damage to it landed) and `kills`.
local by_monster = {}
local kills = 0
-- A killing blow said by the server, waiting for its creature's corpse: { name, at }.
local pending_kb = nil
local items_dirty = true
local last_count = 0

local function monster(name)
	local m = by_monster[name]
	if m == nil then
		m = combat.tally()
		m.kills = 0
		by_monster[name] = m
	end
	return m
end

---------------------------------------------------------------------------------------------------
-- The window and the HUD

local function page(title, ...)
	return ui.group({ title = title, ... })
end

local win = host.has("ui.window") and ui.window({ id = "main", title = "Trackers", width = 64,
	ui.tabs({ id = "pages", selected = 1,
		page("Combat",
			ui.label({ id = "c_totals", text = "", width = 62 }),
			ui.label({ id = "c_rates", text = "", width = 62 }),
			ui.label({ id = "c_defense", text = "", width = 62 }),
			ui.label({ id = "c_taken_by", text = "", width = 62 }),
			ui.label({ id = "c_dealt_by", text = "", width = 62 }),
			ui.list({ id = "monsters", columns = { { title = "Creature", grow = 2 }, { title = "Dealt", width = 8 },
				{ title = "Taken", width = 8 }, { title = "Hit", width = 6 }, { title = "Evade", width = 6 },
				{ title = "Resist", width = 6 }, { title = "KBs", width = 4 }, { title = "Hits you with", grow = 1 } },
				rows = {}, height = 8, width = 62 }),
			ui.label({ id = "c_lifetime", text = "", width = 62 }),
			ui.button({ id = "reset", text = "Reset" })
		),
		page("Experience",
			ui.label({ id = "x_xp", text = "", width = 62 }),
			ui.label({ id = "x_xp_rate", text = "", width = 62 }),
			ui.label({ id = "x_level", text = "", width = 62 }),
			ui.label({ id = "x_lum", text = "", width = 62 }),
			ui.label({ id = "x_lum_rate", text = "", width = 62 })
		),
		page("Mana",
			ui.list({ id = "mana", columns = { "Item", { title = "Mana", width = 8 }, { title = "Left", width = 8 },
				{ title = "State", width = 10 } }, rows = {}, height = 8, width = 62 }),
			ui.number({ id = "warn", text = "Warn under (minutes)", value = 20, min = 1, max = 600, step = 5 }),
			ui.label({ id = "m_notice", text = "", width = 62 })
		),
		page("Items",
			ui.list({ id = "items", columns = { "Item", { title = "Start", width = 9 }, { title = "Now", width = 9 },
				{ title = "Change", width = 9 }, { title = "Per hour", width = 9 } }, rows = {}, height = 8, width = 62 }),
			ui.label({ id = "i_value", text = "", width = 62 }),
			ui.label({ id = "i_low", text = "", width = 62 }),
			ui.row({
				ui.input({ id = "low_name", text = "Warn when", value = "", placeholder = "Mana Stone", grow = 1 }),
				ui.number({ id = "low_n", text = "is under", value = 10, min = 1, max = 100000, step = 5 }),
				ui.button({ id = "low_set", text = "Warn" }),
				ui.button({ id = "low_off", text = "Stop" })
			})
		),
		page("Corpses",
			ui.label({ id = "k_mine", text = "", width = 62 }),
			ui.row({
				ui.button({ id = "point", text = "Point at my corpse" }),
				ui.button({ id = "clear_arrow", text = "Clear the arrow" })
			}),
			ui.list({ id = "corpses", columns = { "Corpse", { title = "Whose", width = 12 }, { title = "Where", width = 14 },
				{ title = "Away", width = 8 } }, rows = {}, height = 8, width = 62 })
		),
		page("Players",
			ui.list({ id = "players", columns = { "Player", { title = "Seen", width = 8 }, { title = "Where", width = 14 },
				{ title = "Away", width = 8 } }, rows = {}, height = 10, width = 62 })
		),
	}),
}) or nil

local hud = host.has("ui.hud") and ui.hud({ id = "hud", anchor = "top_right",
	ui.label({ id = "line", text = "0 dealt / 0 taken / 0 kills" }),
}) or nil

local function combat_lines(t, killed)
	local attacks = combat.attacks(t)
	return string.format("Dealt %s (%s critical), taken %s; %d killing blows%s.",
			util.number(t.dealt), util.percent(t.crits, t.hits), util.number(t.taken), t.kbs or 0,
			killed and string.format(", %d kills", killed) or ""),
		string.format("Your attacks: %d, %s landed (%d evaded, %d resisted).",
			attacks, util.percent(t.hits, attacks), t.misses, t.resisted),
		string.format("You evaded %s of %d melee and missile attacks; resisted %s of %d spells.",
			util.percent(t.evades, t.evades + t.hit_on_me), t.evades + t.hit_on_me,
			util.percent(t.resists, t.resists + t.magic_on_me), t.resists + t.magic_on_me)
end

local function monster_rows()
	local rows = {}
	for name, m in pairs(by_monster) do
		table.insert(rows, {
			name,
			m.dealt,
			m.taken,
			util.percent(m.hits, combat.attacks(m)),
			util.percent(m.evades, m.evades + m.hit_on_me),
			util.percent(m.resists, m.resists + m.magic_on_me),
			m.kbs,
			combat.elements(m.taken_by),
		})
	end
	table.sort(rows, function(a, b)
		return a[2] + a[3] > b[2] + b[3] or (a[2] + a[3] == b[2] + b[3] and a[1] < b[1])
	end)
	return rows
end

local function my_corpse_line()
	local mine = corpses.mine
	if mine == nil then
		return "Your corpse: none seen."
	end
	return string.format("Your corpse: %s, %s away.", util.coords(mine.place), util.away(mine.place))
end

local function refresh()
	if hud then
		hud:get("line").text = string.format("%d dealt / %d taken / %d kills", session.dealt, session.taken, kills)
	end
	if not win then
		return
	end
	local totals, rates, defense = combat_lines(session, kills)
	win:get("c_totals").text = totals
	win:get("c_rates").text = rates
	win:get("c_defense").text = defense
	win:get("c_taken_by").text = "Taken by element: " .. combat.elements(session.taken_by)
	win:get("c_dealt_by").text = "Dealt by element: " .. combat.elements(session.dealt_by)
	win:get("monsters").rows = monster_rows()
	win:get("c_lifetime").text = string.format("Lifetime: dealt %s, taken %s, %d killing blows.",
		util.number(lifetime.dealt), util.number(lifetime.taken), lifetime.kbs or 0)

	local xp, lum = experience.xp, experience.lum
	win:get("x_xp").text = string.format("Experience this session: %s", util.number(xp.earned))
	win:get("x_xp_rate").text = string.format("Per hour: %s; over the last 5 minutes: %s",
		util.number(experience.rate(xp)), util.number(experience.recent(xp)))
	win:get("x_level").text = "Next level in: " .. util.duration(experience.to_level())
	win:get("x_lum").text = string.format("Luminance this session: %s", util.number(lum.earned))
	win:get("x_lum_rate").text = string.format("Per hour: %s; over the last 5 minutes: %s",
		util.number(experience.rate(lum)), util.number(experience.recent(lum)))

	local mrows = {}
	for _, r in ipairs(mana.rows()) do
		table.insert(mrows, { r.name, r.mana, util.duration(r.left), r.state })
	end
	win:get("mana").rows = mrows
	local last = mana.notices[#mana.notices]
	win:get("m_notice").text = last and string.format("Last notice: %s is %s on mana.", last.name, last.how) or ""

	local irows = {}
	for _, r in ipairs(items.rows()) do
		table.insert(irows, { r.name, r.start, r.now, string.format("%+d", r.change), string.format("%+.0f", r.per_hour) })
	end
	win:get("items").rows = irows
	local change, per_hour = items.value()
	win:get("i_value").text = string.format("Worth: %s pyreals (%s per hour).", util.number(change), util.number(per_hour))
	local low = {}
	for name, under in pairs(items.low) do
		table.insert(low, string.format("%s under %d", name, under))
	end
	table.sort(low)
	win:get("i_low").text = #low > 0 and ("Warns: " .. table.concat(low, ", ")) or "Warns: none"

	win:get("k_mine").text = my_corpse_line()
	win:get("corpses").rows = corpses.rows()
	win:get("players").rows = players.rows()
end

---------------------------------------------------------------------------------------------------
-- What is kept with the character

local function load_character()
	local ok = pcall(function()
		local s = store.character
		lifetime = s.lifetime or combat.tally()
		items.low = s.low or {}
		mana.warn_minutes = s.mana_warn or mana.warn_minutes
	end)
	corpses.load()
	if ok and win then
		win:get("warn").value = mana.warn_minutes
	end
end

local function save_lifetime()
	if lifetime_dirty then
		lifetime_dirty = false
		pcall(function()
			store.character.lifetime = lifetime
		end)
	end
end

-- A new session: at login (and when the plugin is loaded in the world), with what is kept read
-- back; at a reset, with the lifetime kept as it stands.
local function start_session(reading)
	if reading then
		load_character()
	else
		save_lifetime()
	end
	session = combat.tally()
	by_monster = {}
	kills = 0
	pending_kb = nil
	experience.reset()
	items.reset()
	players.scan()
	refresh()
end

-- Warn when the pack holds fewer than `n` of `name`, or (with no `n`) no longer warn of it.
local function set_low(name, n)
	if name == nil or name == "" then
		return
	end
	items.low[name] = n
	pcall(function()
		store.character.low = items.low
	end)
	items_dirty = true
	refresh()
end

local function clear_arrow()
	if host.has("overlay.arrow") then
		markers.arrow:clear()
	end
end

local function point_at_my_corpse()
	local mine = corpses.mine
	if mine == nil or mine.place == nil then
		chat.print("[Track] No corpse of yours seen.")
		return
	end
	if host.has("overlay.arrow") then
		markers.arrow:set({ position = mine.place, label = "Your corpse" })
	end
	chat.print(string.format("[Track] Your corpse: %s, %s away.", util.coords(mine.place), util.away(mine.place)))
end

---------------------------------------------------------------------------------------------------
-- Events

on("chat.line", function(e)
	local text = e.text or ""
	local news = combat.read(text)
	if news ~= nil then
		combat.count(session, news)
		combat.count(lifetime, news)
		lifetime_dirty = true
		local m = monster(news.who)
		combat.count(m, news)
		if news.what == "dealt" then
			m.last_hit = os.clock()
		end
		return
	end
	if mana.notice(text) ~= nil then
		return
	end
	-- The server's word on a creature's death, to its killer only: a line naming a creature
	-- your last blow landed on a moment ago. It counts once the creature's corpse shows.
	if e.kind == "default" then
		for name, m in pairs(by_monster) do
			if m.last_hit ~= nil and os.clock() - m.last_hit < 3 and string.find(text, name, 1, true) then
				pending_kb = { name = name, at = os.clock() }
				break
			end
		end
	end
end)

local function killed_recently(name)
	local m = by_monster[name]
	return m ~= nil and m.last_hit ~= nil and os.clock() - m.last_hit < 60
end

on("world.object_created", function(e)
	if e.class == "Corpse" then
		local fellows = {}
		local f = game.player.fellowship
		for _, member in ipairs(f and f.members or {}) do
			fellows[member.name] = true
		end
		local c = corpses.seen(e.id, e.name, {
			me = game.player.name,
			fellows = fellows,
			players = players.names(),
			killed = killed_recently,
		})
		if c.whose == "your kill" then
			kills = kills + 1
			monster(c.who).kills = monster(c.who).kills + 1
		end
		if pending_kb ~= nil and pending_kb.name == c.who and os.clock() - pending_kb.at < 10 then
			pending_kb = nil
			session.kbs = session.kbs + 1
			lifetime.kbs = (lifetime.kbs or 0) + 1
			lifetime_dirty = true
			monster(c.who).kbs = monster(c.who).kbs + 1
		end
		if c.whose == "yours" then
			chat.print(string.format("[Track] Your corpse lies at %s. /track corpse points at it.", util.coords(c.place)))
		end
	elseif e.class == "Player" and e.id ~= game.player.id then
		players.saw(e.id)
	end
end)

on("world.object_moved", function(e)
	if players.seen[e.id] ~= nil then
		players.saw(e.id)
	end
end)

on("world.object_removed", function(e)
	if not corpses.gone(e.id) then
		players.left(e.id)
	end
end)

on("character.death", function()
	-- A death's own lines name its killer: no killing blow of yours.
	pending_kb = nil
end)

on("character.xp", function(e)
	experience.gain_xp(e.gained or 0)
end)

on("character.luminance", function(e)
	experience.gain_lum(e.gained or 0)
end)

for _, name in ipairs({ "inventory.item_added", "inventory.item_removed", "world.object_changed" }) do
	on(name, function()
		items_dirty = true
	end)
end

on("session.login", function()
	start_session(true)
end)

on("ui.clicked", function(e)
	if e.window ~= "main" then
		return
	end
	if e.id == "reset" then
		start_session(false)
		chat.print("Trackers reset.")
	elseif e.id == "point" then
		point_at_my_corpse()
	elseif e.id == "clear_arrow" then
		clear_arrow()
	elseif e.id == "low_set" then
		set_low(win:get("low_name").value, tonumber(win:get("low_n").value))
	elseif e.id == "low_off" then
		set_low(win:get("low_name").value, nil)
	end
end)

-- An item picked in the Items page's list is the one its warning is set for.
on("ui.selected", function(e)
	if e.window == "main" and e.id == "items" and e.rows and e.rows[1] then
		local r = items.rows()[e.rows[1]]
		if r ~= nil then
			win:get("low_name").value = r.name
			if items.low[r.name] ~= nil then
				win:get("low_n").value = items.low[r.name]
			end
		end
	end
end)

on("ui.changed", function(e)
	if e.window == "main" and e.id == "warn" then
		mana.warn_minutes = e.value
		pcall(function()
			store.character.mana_warn = e.value
		end)
	end
end)

---------------------------------------------------------------------------------------------------
-- Timers: worn items appraised a few at a time, the pack recounted when it changed, warnings
-- said, the lifetime saved, the window shown afresh.

task.every(1000, function()
	if game.player.id == nil then
		return
	end
	local due = mana.due(1)
	if #due > 0 then
		task.spawn(mana.look, due[1])
	end
	for _, low in ipairs(mana.newly_low()) do
		chat.print(string.format("[Track] %s has %s of mana left.", low.name, util.duration(low.left)))
	end
	refresh()
end)

task.every(2000, function()
	if game.player.id == nil then
		return
	end
	-- The pack is counted again when it changed, and at least every half minute.
	if items_dirty or os.clock() - last_count > 30 then
		items_dirty = false
		last_count = os.clock()
		items.recount()
		for _, low in ipairs(items.newly_low()) do
			chat.print(string.format("[Track] Low on %s: %d left (warned under %d).", low.name, low.n, low.under))
		end
	end
	save_lifetime()
end)

---------------------------------------------------------------------------------------------------
-- Commands

local HELP = "trackers: /track [reset|kills|xp|mana|warn <minutes>|items|low <name> <n>|corpse [clear]|players|monster <name>|lifetime]"

commands.add("track", HELP, function(args, words)
	local verb = words[1]
	if verb == "reset" then
		start_session(false)
		chat.print("Trackers reset.")
	elseif verb == "kills" then
		for name, m in pairs(by_monster) do
			if m.kills > 0 then
				chat.print(string.format("%s: %d", name, m.kills))
			end
		end
	elseif verb == "xp" then
		local xp, lum = experience.xp, experience.lum
		chat.print(string.format("[Track] Experience %s, %s per hour (%s over the last 5 minutes); luminance %s, %s per hour; next level in %s.",
			util.number(xp.earned), util.number(experience.rate(xp)), util.number(experience.recent(xp)),
			util.number(lum.earned), util.number(experience.rate(lum)), util.duration(experience.to_level())))
	elseif verb == "mana" then
		for _, r in ipairs(mana.rows()) do
			chat.print(string.format("[Track] %s: %s, %s left (%s).", r.name, r.mana, util.duration(r.left), r.state))
		end
	elseif verb == "items" then
		for _, r in ipairs(items.rows()) do
			chat.print(string.format("[Track] %s: %d (%+d, %+.0f per hour).", r.name, r.now, r.change, r.per_hour))
		end
	elseif verb == "low" then
		local name, n = string.match(args, "^low%s+(.-)%s+(%d+)%s*$")
		local off = string.match(args, "^low%s+(.-)%s+off%s*$")
		if name ~= nil then
			set_low(name, tonumber(n))
		elseif off ~= nil then
			set_low(off, nil)
		else
			chat.print("[Track] /track low <name> <count>, or /track low <name> off.")
			return
		end
		chat.print(string.format("[Track] Warning for %s %s.", name or off, name and ("under " .. n) or "off"))
	elseif verb == "corpse" then
		if words[2] == "clear" then
			clear_arrow()
			chat.print("[Track] Arrow cleared.")
		else
			point_at_my_corpse()
		end
	elseif verb == "warn" then
		local n = tonumber(words[2])
		if n == nil or n < 1 then
			chat.print("[Track] /track warn <minutes>: warn of worn items with less mana left.")
			return
		end
		mana.warn_minutes = n
		if win then
			win:get("warn").value = n
		end
		pcall(function()
			store.character.mana_warn = n
		end)
		chat.print(string.format("[Track] Warning of worn items with under %d minutes of mana.", n))
	elseif verb == "players" then
		for _, r in ipairs(players.rows()) do
			chat.print(string.format("[Track] %s: %s, at %s.", r[1], r[2], r[3]))
		end
	elseif verb == "monster" then
		local name = string.match(args, "^monster%s+(.-)%s*$") or ""
		local m = by_monster[name]
		if m == nil then
			chat.print("[Track] No fight with " .. name .. " this session.")
			return
		end
		local totals, rates, defense = combat_lines(m, m.kills)
		chat.print("[Track] " .. name .. ": " .. totals .. " " .. rates .. " " .. defense)
		chat.print("[Track] It hits you with " .. combat.elements(m.taken_by) .. ".")
	elseif verb == "lifetime" then
		local totals, rates, defense = combat_lines(lifetime)
		chat.print("[Track] Lifetime: " .. totals .. " " .. rates .. " " .. defense)
	else
		chat.print(string.format("Dealt %d, taken %d, %d kills.", session.dealt, session.taken, kills))
	end
	refresh()
end)

-- The HUD opens with the window: a window opens only when the player asks.
on("ui.opened", function(e)
	if e.window == "main" and hud then
		hud:show()
	end
end)

if game.player.id ~= nil then
	start_session(true)
else
	refresh()
end
