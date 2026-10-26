-- XP Meter: experience earned since login and the hourly rate, in a window and on /xp.
--
-- The rate is measured from the first gain of the session, so the minutes before the first kill
-- do not dilute it. The lifetime total this plugin has watched is kept in the character's store.

local fmt = require("format")

local session = { earned = 0, started = nil, gains = 0 }
local win = nil

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

local function refresh()
	if win == nil then
		return
	end
	win:get("earned").text = fmt.number(session.earned)
	win:get("rate").text = fmt.number(rate()) .. " / hour"
	local xp = game.player.xp
	if xp ~= nil then
		win:get("unassigned").text = fmt.number(xp.unassigned)
		if xp.to_level ~= nil then
			win:get("level").text = xp.to_level > 0 and (fmt.number(xp.to_level) .. " to next level") or "at the level cap"
		end
	end
end

local function reset()
	session = { earned = 0, started = nil, gains = 0 }
	refresh()
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
		ui.button({ id = "reset", text = "Reset" }),
	})
	on("ui.clicked", function(e)
		if e.id == "reset" then
			reset()
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
	session.earned = session.earned + e.gained
	session.gains = session.gains + 1
	store.character.lifetime = (store.character.lifetime or 0) + e.gained
	refresh()
end)

on("session.login", function()
	reset()
end)

task.every(1000, refresh)

commands.add("xp", "/xp shows experience this session and the rate; /xp reset starts again", function(args)
	if args == "reset" then
		reset()
		chat.print("XP Meter reset.", "system")
		return
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
end)
