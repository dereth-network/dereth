-- Buffs from a buffbot: another player's character that buffs whoever tells it a word (the
-- Buffbot sample, or anyone buffing by hand).
--
-- The **buffbot trip** goes to the buffbot it was given (Set the selected buffbot, /tank buffbot
-- set [name]: its place, and where the character stood then, kept with the character), tells it
-- the request (BuffbotRequest), waits for the buffs to land, and goes back to where it left, as the
-- vendor trip does. While it waits it answers what the buffbot says: a place in line keeps it
-- waiting, "come within N m" walks it nearer, "Done" ends the wait; a refusal (its cap, a full
-- line, a word it does not know) or no answer at all (it asks three times, half a minute apart)
-- ends the trip, and so does a buffbot that is not where it was (a minute's wait) or a wait longer
-- than BuffbotWaitSeconds. After a trip that brought no buffs it asks again only after
-- BuffbotRetryMinutes, so an absent buffbot does not hold the macro.
--
-- The buffs that landed while it waited are the buffbot's, kept with the character; the trip
-- starts when one of them has under BuffbotMinutes left or is gone (a death takes them all), or
-- when it has none yet; and on /tank buffbot, a meta's or a route's line typing it.

local S = require("settings")

local T = {}
-- The engine, set by it.
T.E = nil

-- How near the buffbot the character walks before asking, meters; how far it looks for it; how
-- near its place counts as there.
local REACH, SEEN, THERE = 4, 60, 10
-- How long a buffbot not where it was is waited for, seconds; how long between asks with no
-- answer, and how many asks.
local ABSENT_WAIT, ASK_GAP, ASKS = 60, 30, 3
-- How long after "Done" buffs still landing are waited for, seconds.
local SETTLE = 3

-- The buffbot: { name, id, place, stand } kept with the character, or nil.
function T.buffbot()
	local name = S.get("Buffbot")
	if name == nil or name == "" then
		return nil
	end
	local kept = (store.character.tank_buffbots or {})[string.lower(name)] or {}
	return { name = name, id = kept.id, place = kept.place, stand = kept.stand }
end

local function place_of(p)
	return p and { cell = p.cell, x = p.x, y = p.y, z = p.z } or nil
end

-- The selected player (or the nearest in sight of `name`) becomes the buffbot, its place and
-- where the character stands kept; answers the line to tell.
function T.set_buffbot(o)
	if o == nil or o.class ~= "Player" or o.id == game.player.id then
		return "select the buffbot's character first"
	end
	local name = string.gsub(o.name or "", "^%+", "")
	local all = store.character.tank_buffbots or {}
	all[string.lower(name)] = { id = o.id, place = place_of(o.position), stand = place_of(game.player.position) }
	store.character.tank_buffbots = all
	S.set("Buffbot", name)
	return "buffs come from " .. name
end

-- The buffs the buffbot gave, kept with the character: { category, name } each.
local function given()
	return store.character.tank_buffbot_buffs or {}
end

-- The character's enchantment of `category`, or nil.
local function enchantment(category)
	for _, e in ipairs(game.player.enchantments or {}) do
		if e.category == category then
			return e
		end
	end
	return nil
end

-- When a trip that brought nothing may be tried again.
local retry_at = nil

-- Why the buffbot trip is due now, or nil; with `soon` (seconds), due too when a buff has less than
-- BuffbotMinutes and that left.
function T.due(soon)
	if T.buffbot() == nil or S.get("BuffbotMinutes") <= 0 then
		return nil
	end
	if retry_at ~= nil and os.clock() < retry_at then
		return nil
	end
	local kept = given()
	if #kept == 0 then
		return "no buffs from " .. S.get("Buffbot") .. " yet"
	end
	for _, b in ipairs(kept) do
		local e = enchantment(b.category)
		if e == nil then
			return b.name .. " is gone"
		end
		if e.remaining ~= nil and e.remaining < S.get("BuffbotMinutes") * 60 + (soon or 0) then
			return string.format("%s has %d min left", b.name, math.floor(e.remaining / 60))
		end
	end
	return nil
end

-- What the buffbot told the character, since the trip asked: { text, at } each.
local heard = {}

-- A tell from the buffbot, kept for the trip waiting on it.
function T.chat_line(e, parse_tell)
	if e.kind ~= "tell" or T.E == nil or T.E.btrip == nil then
		return
	end
	local name, _, said = parse_tell(e.text)
	if name ~= nil and string.lower(name) == string.lower(T.E.btrip.bot.name) then
		table.insert(heard, { text = said, at = os.clock() })
	end
end

-- Start the trip, `why` for the log, going back afterward to `back` (else to where it starts);
-- answers the line to tell.
function T.start(why, back)
	local E = T.E
	if E.btrip ~= nil then
		return "the buffbot trip is under way"
	end
	local bot = T.buffbot()
	if bot == nil then
		return "no buffbot: select its character and set it on the Buffs page (/tank buffbot set)"
	end
	local p = game.player.position
	E.btrip = { stage = "going", bot = bot, tries = 0, back = back or place_of(p) }
	E.tally.buff_trips = (E.tally.buff_trips or 0) + 1
	local seen = T.bot_object(bot)
	local d = seen and seen:distance() or (bot.place and E.metres_to(bot.place))
	E.note("buffbot trip to %s (%s)%s", bot.name, why,
		d and d < 5000 and string.format(", %d m away", math.floor(d)) or "")
	return "going to " .. bot.name
end

-- The trip ends, `landed` the buffs it brought; the route carries on from its nearest waypoint.
local function finish(t, why)
	local E = T.E
	t.stage, t.tries = "returning", 0
	t.why = why
	local landed = {}
	for cat, name in pairs(t.landed or {}) do
		table.insert(landed, { category = cat, name = name })
	end
	table.sort(landed, function(a, b)
		return a.name < b.name
	end)
	if #landed > 0 then
		store.character.tank_buffbot_buffs = landed
		retry_at = nil
		E.tally.buffbot_buffs = (E.tally.buffbot_buffs or 0) + #landed
	else
		retry_at = os.clock() + S.get("BuffbotRetryMinutes") * 60
	end
	E.note("buffbot: %s; %d buffs landed%s", why, #landed,
		#landed == 0 and string.format(", asking again in %d min at the soonest", S.get("BuffbotRetryMinutes")) or "")
end

local function done(why)
	local E = T.E
	E.btrip = nil
	if E.route ~= nil then
		E.route.at = nil
	end
	E.note("buffbot trip: %s", why)
	if E.btrip_only then
		E.stop("the buffbot trip is done")
	end
end

-- A death on the trip: the buffs that landed went with it. The trip asks again from where the
-- character comes back to life, and ends there: a corpse to go back for, or the route, is the
-- macro's next step.
function T.died()
	local t = T.E.btrip
	if t ~= nil then
		t.stage, t.tries, t.back, t.landed, t.done_at, t.answered, t.asks = "going", 0, nil, nil, nil, nil, 0
		t.absent_since = nil
	end
end

-- The buffbot's character, by its id, else the nearest of its name (an administrator's with its
-- plus sign), within SEEN meters; or nil.
function T.bot_object(bot)
	local o = bot.id and game.world:get(bot.id) or nil
	if o == nil or o.name == nil then
		local want = string.lower(bot.name)
		o = game.world:first({ class = "Player", within = SEEN, sort = "distance" }, function(p)
			return string.lower((string.gsub(p.name or "", "^%+", ""))) == want
		end)
	end
	if o ~= nil and o:distance() <= SEEN then
		return o
	end
	return nil
end

-- The remaining time of each beneficial enchantment, by category.
local function snapshot()
	local out = {}
	for _, e in ipairs(game.player.enchantments or {}) do
		if e.beneficial ~= false and e.category ~= nil then
			out[e.category] = { name = e.name, remaining = e.remaining or math.huge }
		end
	end
	return out
end

local function ask(t)
	local E = T.E
	t.asks = (t.asks or 0) + 1
	t.asked_at = os.clock()
	local line = string.format("/t %s, %s", t.bot.name, S.get("BuffbotRequest"))
	local ok, why = pcall(chat.command, line)
	E.note(ok and "buffbot: asked %s for %s" or "buffbot: could not ask %s: %s", t.bot.name,
		ok and S.get("BuffbotRequest") or tostring(why))
end

-- One step of the trip: on the way, asking and waiting, or on the way back.
function T.step()
	local E = T.E
	local t = E.btrip
	E.set_state("Buffbot trip")
	if t.stage == "going" then
		local bot = T.bot_object(t.bot)
		if bot ~= nil and bot:distance() <= 30 then
			local ok, why = E.approach(bot, REACH)
			if ok then
				t.stage, t.tries = "asking", 0
				return
			end
			if why ~= "walking" and why ~= "stopped" then
				t.tries = t.tries + 1
				if t.tries >= 3 then
					finish(t, string.format("could not get near %s (%s)", t.bot.name, tostring(why)))
				end
			end
			return
		end
		local where = t.bot.stand or t.bot.place
		if where == nil then
			finish(t, string.format("%s is not in sight and its place is not known", t.bot.name))
			return
		end
		local d = E.metres_to(where)
		if d ~= nil and d <= THERE then
			-- At its place, and it is not there: waited for a while.
			t.absent_since = t.absent_since or os.clock()
			if os.clock() - t.absent_since > ABSENT_WAIT then
				finish(t, string.format("%s is not where it was", t.bot.name))
			else
				task.sleep(1000)
			end
			return
		end
		local ok, why = E.travel(where, "to " .. t.bot.name)
		if not ok and why == "no journeys" then
			ok, why = E.walk(where, REACH)
		end
		if not ok then
			t.tries = t.tries + 1
			if t.tries >= 3 then
				finish(t, string.format("could not get to %s (%s)", t.bot.name, tostring(why)))
			end
		end
	elseif t.stage == "asking" then
		heard = {}
		t.before = snapshot()
		t.landed = {}
		t.deadline = os.clock() + S.get("BuffbotWaitSeconds")
		ask(t)
		t.stage = "waiting"
	elseif t.stage == "waiting" then
		-- What landed since the ask: a buff new, or longer than it was.
		for cat, e in pairs(snapshot()) do
			local was = t.before[cat]
			if was == nil or e.remaining > was.remaining + 5 then
				if t.landed[cat] == nil then
					E.logk("SpellCast", "buffbot: %s landed", tostring(e.name))
				end
				t.landed[cat] = e.name
				t.before[cat] = e
			end
		end
		local lines = heard
		heard = {}
		for _, l in ipairs(lines) do
			local text = tostring(l.text)
			t.answered = true
			local place = string.match(text, "number (%d+) in line")
			local closer = string.match(text, "come within (%d+)")
			if place ~= nil then
				E.note("buffbot: number %s in line", place)
				t.deadline = os.clock() + S.get("BuffbotWaitSeconds")
			elseif closer ~= nil then
				local bot = T.bot_object(t.bot)
				E.note("buffbot: asked to come within %s m", closer)
				if bot ~= nil then
					E.approach(bot, math.max(2, math.min(REACH, tonumber(closer) - 3)))
				end
			elseif string.find(text, "^Buffing you now") or string.find(text, "getting my mana back") then
				t.deadline = os.clock() + S.get("BuffbotWaitSeconds")
			elseif string.find(text, "^Done") then
				t.done_at = os.clock()
				E.note("buffbot: %s", text)
			elseif string.find(text, "ask again") or string.find(text, "line is full") or string.find(text, "I don't know")
				or string.find(text, "not buffing") or string.find(text, "near enough") then
				finish(t, string.format("%s said: %s", t.bot.name, text))
				return
			end
		end
		if t.done_at ~= nil and os.clock() - t.done_at >= SETTLE then
			finish(t, "buffed by " .. t.bot.name)
			return
		end
		if not t.answered and os.clock() - t.asked_at >= ASK_GAP then
			if t.asks >= ASKS then
				finish(t, string.format("%s did not answer", t.bot.name))
				return
			end
			ask(t)
		end
		if os.clock() > t.deadline then
			finish(t, string.format("waited %d s for %s", S.get("BuffbotWaitSeconds"), t.bot.name))
			return
		end
		task.sleep(500)
	else
		local d = t.back and E.metres_to(t.back)
		if t.back == nil or (d ~= nil and d <= THERE) then
			done(t.why or "back")
			return
		end
		local ok, why = E.travel(t.back, "back from " .. t.bot.name)
		if not ok and why == "no journeys" then
			ok, why = E.walk(t.back, REACH)
		end
		if ok then
			done(t.why or "back")
		else
			t.tries = t.tries + 1
			if t.tries >= 3 then
				done(string.format("could not get back (%s)", tostring(why)))
			end
		end
	end
end

return T
