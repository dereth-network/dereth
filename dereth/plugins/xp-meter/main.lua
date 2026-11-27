-- XP Meter: experience earned since login and the hourly rate, in a window and on /xp; the
-- session's kills, deaths and luminance; and what each monster fought does, in a second window.
--
-- The rate is measured from the first gain of the session, so the minutes before the first kill
-- do not dilute it. The lifetime total this plugin has watched is kept in the character's store,
-- and so are the monster statistics (monsters.lua).

local fmt = require("format")
local monsters = require("monsters")

local session = { earned = 0, started = nil, gains = 0 }
local tally = { kills = 0, deaths = 0, luminance = 0, since = nil }
local win = nil
local mon = nil -- the Monsters window
local book = nil -- the monster statistics, from login to logout
local picked = nil -- the monster whose figures the Monsters window shows

local function rate()
	if session.started == nil then
		return 0
	end
	local hours = (os.clock() - session.started) / 3600
	if hours <= 0 then
		return 0
	end
	return session.earned / hours
end

-- `n` per hour since the session's first kill, gain or death.
local function hourly(n)
	if tally.since == nil then
		return 0
	end
	local hours = (os.clock() - tally.since) / 3600
	if hours <= 0 then
		return 0
	end
	return n / hours
end

local function counted()
	if tally.since == nil then
		tally.since = os.clock()
	end
end

local function with_rate(n)
	return fmt.number(n) .. " (" .. fmt.number(hourly(n)) .. " / hour)"
end

-- A figure for the window, or a dash when there is none yet.
local function shown(v, unit)
	if v == nil then
		return "-"
	end
	return fmt.number(v) .. (unit or "")
end

local function seconds(v)
	if v == nil then
		return "-"
	end
	return string.format("%.1f s", v)
end

local function save()
	if book ~= nil and book.dirty then
		store.character.monsters = book.book
		book.dirty = false
	end
end

local function refresh_monsters()
	if mon == nil or book == nil then
		return
	end
	local names = monsters.names(book.book)
	-- Until the player picks one, the window shows the monster killed most.
	local shown_name = picked
	if shown_name == nil or book.book[shown_name] == nil then
		shown_name = names[1]
	end
	local rows = {}
	local selected = nil
	for i, name in ipairs(names) do
		local f = monsters.figures(book.book[name])
		table.insert(rows, {
			name,
			f.kills,
			shown(f.xp_per_kill),
			shown(f.it_evades, "%"),
			shown(f.you_evade, "%"),
			shown(f.dealt),
			shown(f.taken),
			seconds(f.seconds),
		})
		if name == shown_name then
			selected = i
		end
	end
	mon:get("list").rows = rows
	mon:get("list").selected = selected
	local have = shown_name ~= nil
	local f = have and monsters.figures(book.book[shown_name]) or {}
	mon:get("name").text = have and shown_name or "No monsters fought yet."
	mon:get("forget").enabled = have
	local function set(id, text)
		mon:get(id).text = have and text or "-"
	end
	set("kills", shown(f.kills))
	set("xp", shown(f.xp_per_kill))
	set("it_evades", shown(f.it_evades, "%"))
	set("you_evade", shown(f.you_evade, "%"))
	set("it_resists", shown(f.it_resists, "%"))
	set("you_resist", shown(f.you_resist, "%"))
	set("normal", shown(f.normal))
	set("critical", shown(f.critical))
	set("taken", shown(f.taken))
	set("spell_dealt", shown(f.spell_dealt))
	set("spell_taken", shown(f.spell_taken))
	set("seconds", seconds(f.seconds))
	set("casts", f.casts and string.format("%.1f", f.casts) or "-")
	set("corpse", seconds(f.corpse))
end

local function refresh()
	if win ~= nil then
		win:get("earned").text = fmt.number(session.earned)
		win:get("rate").text = fmt.number(rate()) .. " / hour"
		local xp = game.player.xp
		if xp ~= nil then
			win:get("unassigned").text = fmt.number(xp.unassigned)
			if xp.to_level ~= nil then
				win:get("level").text = xp.to_level > 0 and (fmt.number(xp.to_level) .. " to next level") or "at the level cap"
			end
		end
		win:get("kills").text = with_rate(tally.kills)
		win:get("deaths").text = with_rate(tally.deaths)
		win:get("luminance").text = with_rate(tally.luminance)
	end
	refresh_monsters()
	save()
end

local function reset()
	session = { earned = 0, started = nil, gains = 0 }
	tally = { kills = 0, deaths = 0, luminance = 0, since = nil }
	refresh()
end

-- The monster's name as its row shows it, for `text`: the same, or the same but for case.
local function named(text)
	if book.book[text] ~= nil then
		return text
	end
	local lower = text:lower()
	for name in pairs(book.book) do
		if name:lower() == lower then
			return name
		end
	end
	return nil
end

-- Forget the monster `name`. The window then shows the row that took its place, or the last
-- row, or nothing when none is left.
local function forget(name)
	local at = nil
	for i, n in ipairs(monsters.names(book.book)) do
		if n == name then
			at = i
		end
	end
	if at == nil or not book.forget_name(name) then
		return false
	end
	local rest = monsters.names(book.book)
	picked = rest[math.min(at, #rest)]
	refresh()
	return true
end

-- Forget the monster the Monsters window shows.
local function forget_shown()
	if book == nil then
		return
	end
	local name = picked
	if name == nil or book.book[name] == nil then
		name = monsters.names(book.book)[1]
	end
	if name ~= nil then
		forget(name)
	end
end

-- A label and its value, side by side.
local function pair(text, id)
	return ui.row({ ui.label({ text = text, width = 15 }), ui.label({ id = id, text = "-" }) })
end

if host.has("ui.window") then
	win = ui.window({
		id = "main",
		title = "XP Meter",
		width = 24,
		ui.row({ ui.label({ text = "This session:" }), ui.label({ id = "earned", text = "0" }) }),
		ui.row({ ui.label({ text = "Rate:" }), ui.label({ id = "rate", text = "0 / hour" }) }),
		ui.row({ ui.label({ text = "Unassigned:" }), ui.label({ id = "unassigned", text = "-" }) }),
		ui.label({ id = "level", text = "" }),
		ui.row({ ui.label({ text = "Kills:" }), ui.label({ id = "kills", text = "0 (0 / hour)" }) }),
		ui.row({ ui.label({ text = "Deaths:" }), ui.label({ id = "deaths", text = "0 (0 / hour)" }) }),
		ui.row({ ui.label({ text = "Luminance:" }), ui.label({ id = "luminance", text = "0 (0 / hour)" }) }),
		ui.row({ ui.button({ id = "reset", text = "Reset" }), ui.button({ id = "monsters", text = "Monsters" }) }),
	})
	mon = ui.window({
		id = "monsters",
		title = "Monsters",
		width = 64,
		resizable = true,
		min_width = 40,
		ui.list({
			id = "list",
			height = 8,
			grow = 1,
			columns = {
				{ title = "Monster", grow = 1, min_width = 12 },
				"Kills",
				"XP/kill",
				"Evades you",
				"You evade",
				"Hit",
				"Hit you",
				"Kill time",
			},
			rows = {},
		}),
		ui.row({
			ui.label({ id = "name", text = "No monsters fought yet.", grow = 1 }),
			ui.button({ id = "forget", text = "Forget", enabled = false }),
		}),
		ui.row({
			ui.column({
				pair("Kills", "kills"),
				pair("XP a kill", "xp"),
				pair("Evades you", "it_evades"),
				pair("You evade it", "you_evade"),
				pair("Resists you", "it_resists"),
				pair("You resist it", "you_resist"),
				pair("Seconds a kill", "seconds"),
			}),
			ui.column({
				pair("Your hit", "normal"),
				pair("Your critical", "critical"),
				pair("Its hit", "taken"),
				pair("Your spell", "spell_dealt"),
				pair("Its spell", "spell_taken"),
				pair("Casts a kill", "casts"),
				pair("Time to corpse", "corpse"),
			}),
		}),
	})
	on("ui.clicked", function(e)
		if e.id == "reset" then
			reset()
		elseif e.id == "monsters" then
			refresh_monsters()
			mon:show()
		elseif e.id == "forget" then
			forget_shown()
		end
	end)
	on("ui.selected", function(e)
		if e.window == "monsters" and e.id == "list" and book ~= nil then
			picked = monsters.names(book.book)[e.rows[1]]
			refresh_monsters()
		end
	end)
end

on("character.xp", function(e)
	if e.gained <= 0 then
		return
	end
	if session.started == nil then
		session.started = os.clock()
	end
	counted()
	session.earned = session.earned + e.gained
	session.gains = session.gains + 1
	store.character.lifetime = (store.character.lifetime or 0) + e.gained
	if book ~= nil then
		book.gained(e.gained)
	end
	refresh()
end)

on("character.luminance", function(e)
	if e.gained <= 0 then
		return
	end
	counted()
	tally.luminance = tally.luminance + e.gained
	refresh()
end)

on("character.death", function()
	counted()
	tally.deaths = tally.deaths + 1
	refresh()
end)

on("session.login", function()
	reset()
	book = monsters.new(store.character.monsters, os.clock)
	picked = nil
	refresh()
end)

on("session.logout", function()
	save()
	book = nil
end)

function on_unload()
	save()
end

-- Is `name` a monster: one counted already, or one the character can attack nearby?
local function monster(name)
	return book.knows(name) or game.world:first({ name = name, monster = true }) ~= nil
end

-- The names a kill's line may tell of: the monsters counted and those nearby.
local function candidates()
	local names = {}
	for name in pairs(book.book) do
		table.insert(names, name)
	end
	for _, o in ipairs(game.world:find({ monster = true })) do
		table.insert(names, o.name)
	end
	return names
end

if host.has("messages") then
	-- A combat notice's handler, for while a character is in the world.
	local function heard(f)
		return function(m)
			if book ~= nil and m.fields ~= nil then
				f(m.fields)
			end
		end
	end
	messages.on("Combat_HandleAttackerNotificationEvent", heard(function(f)
		book.hit(f.defender_name, f.damage, f.critical ~= 0)
	end))
	messages.on("Combat_HandleDefenderNotificationEvent", heard(function(f)
		book.struck(f.attacker_name, f.damage)
	end))
	messages.on("Combat_HandleEvasionAttackerNotificationEvent", heard(function(f)
		book.missed(f.defender_name)
	end))
	messages.on("Combat_HandleEvasionDefenderNotificationEvent", heard(function(f)
		book.evaded(f.attacker_name)
	end))
	messages.on("Combat_HandleVictimNotificationEventOther", heard(function(f)
		counted()
		tally.kills = tally.kills + 1
		local name = monsters.victim(f.message, candidates())
		if name ~= nil then
			book.kill(name)
		end
	end))
	messages.on("Magic_CastTargetedSpell", heard(function(f)
		local target = game.world:get(f.target)
		if target ~= nil and target.name ~= nil and monster(target.name) then
			book.cast(target.name)
		end
	end))
end

-- The kinds of line the server tells spells' outcomes in.
local SPELL_LINES = { magic = true, combat = true, combat_self = true, combat_enemy = true }

on("chat.line", function(e)
	if book == nil or not SPELL_LINES[e.kind] then
		return
	end
	local kind, name, n = monsters.spell_line(e.text)
	if kind == "dealt" then
		book.spell_hit(name, n)
	elseif kind == "taken" then
		book.spell_struck(name, n)
	elseif kind == "resisted" then
		book.resisted(name)
	elseif kind == "resist" then
		book.resist(name)
	elseif kind == "cast" then
		for _, maybe in ipairs(name) do
			if monster(maybe) then
				book.landed(maybe)
				break
			end
		end
	end
end)

on("world.object_created", function(e)
	if book ~= nil and e.name ~= nil then
		local name = e.name:match("^Corpse of (.+)$")
		if name ~= nil then
			book.corpse(name)
		end
	end
end)

task.every(1000, refresh)

-- One monster's line for /xp monsters.
local function monster_line(name)
	local f = monsters.figures(book.book[name])
	return string.format(
		"%s: %d kills, %s XP a kill, evades you %s, you evade it %s, resists you %s, %s a kill.",
		name,
		f.kills,
		shown(f.xp_per_kill),
		shown(f.it_evades, "%"),
		shown(f.you_evade, "%"),
		shown(f.it_resists, "%"),
		seconds(f.seconds)
	)
end

commands.add(
	"xp",
	"/xp shows experience this session and the rate; /xp reset starts again; /xp monsters shows the monsters fought; /xp forget [name] forgets one, or all of them",
	function(args)
		if args == "reset" then
			reset()
			chat.print("XP Meter reset.", "system")
			return
		end
		if args == "monsters" then
			if mon ~= nil then
				refresh_monsters()
				mon:show()
			end
			local names = book ~= nil and monsters.names(book.book) or {}
			if #names == 0 then
				chat.print("No monsters fought yet.", "system")
			end
			for i = 1, math.min(#names, 10) do
				chat.print(monster_line(names[i]), "system")
			end
			return
		end
		local one = args:match("^forget%s+(.-)%s*$")
		if one ~= nil and one ~= "" then
			local name = book ~= nil and named(one) or nil
			if name ~= nil and forget(name) then
				chat.print("XP Meter forgot " .. name .. ".", "system")
			else
				chat.print("No monster named " .. one .. ".", "system")
			end
			return
		end
		if args == "forget" then
			if book ~= nil then
				book.forget()
				picked = nil
				refresh()
			end
			chat.print("XP Meter forgot the monsters.", "system")
			return
		end
		if win ~= nil then
			refresh()
			win:show()
		end
		chat.print(
			string.format(
				"XP this session: %s in %d gains, %s per hour. Watched in all: %s.",
				fmt.number(session.earned),
				session.gains,
				fmt.number(rate()),
				fmt.number(store.character.lifetime or 0)
			),
			"system"
		)
		chat.print(
			string.format(
				"Kills: %s, deaths: %s, luminance: %s.",
				with_rate(tally.kills),
				with_rate(tally.deaths),
				with_rate(tally.luminance)
			),
			"system"
		)
	end
)
