-- Fellows: helping them (VTank's DoHelp), the Tank's own fellow over this machine's clients (in the
-- manner of Virindi Integrator 2's fellows), and running a fellowship (VTank's
-- AutoFellowManagement).
--
-- Helping: a fellow under a helper mark (Recharge-Helper-HitP, -Stam, -Mana) and within its
-- distance (HelperDistanceHitP, -Stam, -Mana) is healed (Heal Other, or with UseHealersHeart a
-- Healer's Heart), given stamina (Revitalize Other) or given mana (Mana Boost Other), the lowest
-- first, health before stamina before mana. The fellows are the fellowship's, as the game keeps
-- their figures (which it does while its fellowship panel is open, as VTank says of its helper),
-- and the Tank's in the player's other clients on this machine in the same PeerFellow, which tell
-- their own.
--
-- The Tank's own fellow (PeerFellow, VI2's fellow): the Tanks of one name on this machine's clients
-- tell each other, every second, where their characters are, their vitals, and the debuffs each has
-- cast; so they heal each other without the fellowship panel, do not cast a debuff a fellow has
-- already put on a monster, and fight first a monster one of them has made vulnerable.
--
-- Running a fellowship, as its leader (VTank's fellowship commands): a player who tells the
-- character "xp" is recruited (asked to stand close first, and given one more chance twenty
-- seconds later when too far), or put in line when the fellowship is full; "line", "remove",
-- "leader", "startvote giveleader <name>", "vote <n> yes|no" and "help" are answered. At most
-- eight commands are answered in any three minutes.

local S = require("settings")
local spells = require("spells")

local F = {}

-- The macro (set by the engine).
F.E = nil

-- The spells that give a fellow back each vital.
F.SPELLS = { health = "Heal Other", stamina = "Revitalize Other", mana = "Mana Boost Other" }
spells.want({ F.SPELLS.health, F.SPELLS.stamina, F.SPELLS.mana })
local MARK = { health = "Recharge-Helper-HitP", stamina = "Recharge-Helper-Stam", mana = "Recharge-Helper-Mana" }
-- The server's word on a cast at a target beyond the spell's reach, and how near the helper then
-- walks, metres.
local OUT_OF_RANGE, NEAR_ENOUGH = "0x0550", 5
local REACH = { health = "HelperDistanceHitP", stamina = "HelperDistanceStam", mana = "HelperDistanceMana" }

---------------------------------------------------------------------------------------------------
-- The Tank's own fellow on this machine

-- What each fellow's Tank last told, by its character's name in lower case: { at, data }.
local heard = {}
-- How long a fellow's word holds, seconds.
local STALE = 5

local function fellow_name()
	return string.lower(string.match(S.get("PeerFellow") or "", "^%s*(.-)%s*$"))
end

-- Whether the Tank is in a fellow of its own on this machine.
function F.in_peer_fellow()
	return fellow_name() ~= "" and host.has("peers.local")
end

-- A message from the Tank in another client: kept when it is of the same fellow.
function F.heard(e)
	if e.channel ~= "fellow" or type(e.data) ~= "table" or not F.in_peer_fellow() then
		return
	end
	if string.lower(tostring(e.data.fellow or "")) ~= fellow_name() or e.data.name == nil then
		return
	end
	heard[F.E.plain_name(e.data.name)] = { at = os.clock(), data = e.data }
end

-- The fellows heard lately: each { at, data }.
local function fresh()
	local out = {}
	for _, h in pairs(heard) do
		if os.clock() - h.at <= STALE then
			table.insert(out, h)
		end
	end
	return out
end

-- Where the fellow of that name last said it was, or nil.
function F.heard_place(name)
	local h = name ~= nil and heard[F.E.plain_name(name)] or nil
	if h == nil or os.clock() - h.at > STALE * 6 then
		return nil
	end
	return h.data.position
end

-- Whether a fellow has put debuff family `family` on monster `id`, with more than
-- DebuffPrecastSeconds of it left.
function F.peer_debuffed(id, family)
	for _, h in ipairs(fresh()) do
		for _, d in ipairs(h.data.debuffs or {}) do
			if d.monster == id and d.family == family
				and (d.left or 0) - (os.clock() - h.at) > S.get("DebuffPrecastSeconds") then
				return true
			end
		end
	end
	return false
end

-- Whether a fellow has made monster `id` vulnerable (an element's vulnerability or Imperil).
function F.peer_vulned(id)
	for _, h in ipairs(fresh()) do
		for _, d in ipairs(h.data.debuffs or {}) do
			if d.monster == id and (d.left or 0) - (os.clock() - h.at) > 0
				and (string.find(d.family or "", "Vulnerability", 1, true) or string.find(d.family or "", "Imperil", 1, true)) then
				return true
			end
		end
	end
	return false
end

local function vitals(v)
	local x = game.player[v]
	return x and { current = x.current, max = x.max } or nil
end

-- Tell the fellow where the character is, its vitals and its debuffs, once a second.
task.every(1000, function()
	if not F.in_peer_fellow() or game.player.id == nil then
		return
	end
	local E = F.E
	pcall(peers.send, "fellow", {
		fellow = S.get("PeerFellow"),
		id = game.player.id,
		name = game.player.name,
		position = game.player.position,
		health = vitals("health"),
		stamina = vitals("stamina"),
		mana = vitals("mana"),
		target = E and E.target and E.target.id or nil,
		debuffs = E and E.debuff_list and E.debuff_list() or {},
	})
end)

---------------------------------------------------------------------------------------------------
-- Helping

-- The fellows to help: the fellowship's (but the character) and the fellow's on this machine, each
-- { id, name, health, stamina, mana }, the fellow's own word first.
function F.fellows()
	local out, seen = {}, {}
	for _, h in ipairs(fresh()) do
		local d = h.data
		if d.id ~= nil and d.id ~= game.player.id then
			seen[d.id] = true
			table.insert(out, { id = d.id, name = d.name, health = d.health, stamina = d.stamina, mana = d.mana })
		end
	end
	local f = game.player.fellowship
	for _, m in ipairs(f and f.members or {}) do
		if m.id ~= game.player.id and not seen[m.id] then
			table.insert(out, { id = m.id, name = m.name, health = m.health, stamina = m.stamina, mana = m.mana })
		end
	end
	return out
end

-- What the macro last did for each fellow's vital, by `id:vital`: { figure, at }. A figure that has
-- not changed since is waited on (the fellowship's figures run behind while its panel is shut),
-- twenty seconds at most.
local helped = {}
local HELP_AGAIN = 20

function F.forget()
	helped = {}
	heard = {}
end

-- The fellow most in need of help now, or nil: { fellow, object, vital }.
function F.needing_help()
	for _, vital in ipairs({ "health", "stamina", "mana" }) do
		local best, best_share, best_o = nil, nil, nil
		for _, m in ipairs(F.fellows()) do
			local v = m[vital]
			if v ~= nil and (v.max or 0) > 0 and v.current ~= nil and v.current < v.max * S.share(MARK[vital]) then
				local o = game.world:get(m.id)
				local d = o and o:distance()
				local last = helped[m.id .. ":" .. vital]
				local waiting = last ~= nil and last.figure == v.current and os.clock() - last.at < HELP_AGAIN
				if d ~= nil and d <= S.metres(REACH[vital]) and not waiting then
					local share = v.current / v.max
					if best == nil or share < best_share then
						best, best_share, best_o = m, share, o
					end
				end
			end
		end
		if best ~= nil then
			return { fellow = best, object = best_o, vital = vital }
		end
	end
	return nil
end

-- The Healer's Heart to heal with (UseHealersHeart; one the Weapons setting names, when it names
-- any), or nil.
local function heart()
	if not S.get("UseHealersHeart") then
		return nil
	end
	for _, it in ipairs(game.inventory.items()) do
		local n = string.lower(it.name or "")
		if it.class == "WandStaffOrb" and string.find(n, "healer's heart", 1, true)
			and (#S.list("Weapons") == 0 or S.listed("Weapons", it.name)) then
			return it
		end
	end
	return nil
end

-- One help for the fellow most in need. Answers true when it tried one.
function F.help_step()
	if not S.get("DoHelp") then
		return false
	end
	local E = F.E
	local need = F.needing_help()
	if need == nil or not E.can("heal") then
		return false
	end
	local m, vital = need.fellow, need.vital
	local v = m[vital]
	helped[m.id .. ":" .. vital] = { figure = v.current, at = os.clock() }
	E.set_state("Helping")
	local h = vital == "health" and heart() or nil
	if h ~= nil then
		-- A Healer's Heart heals from the hand: taken up, then used on the fellow, its own spell
		-- cast; with JumpOutWandCasting the character jumps at once, which sends the heal at once.
		if h.wielder ~= game.player.id and not E.take_up(h.name) then
			E.note("help: could not take up %s", h.name)
			return true
		end
		local p = act.use_on(h.id, m.id, { purpose = "heal" })
		if S.get("JumpOutWandCasting") and E.can("nav") then
			task.sleep(0)
			pawait(nav.jump({ ms = 0 }))
		end
		local ok, why = pawait(p)
		E.note(ok and "help: healed %s with %s" or "help: %s with %s: %s", m.name, h.name, tostring(why))
		if ok then
			E.tally.helps = (E.tally.helps or 0) + 1
		end
		return true
	end
	local s = spells.best(F.SPELLS[vital], S.get("MaxSpellLevel"), S.get("SpellDiffExcessThreshold-Hunt"))
	if s == nil or not E.ready_magic() then
		return false
	end
	local ok, why = pawait(magic.cast(s.id, m.id, { purpose = "heal" }))
	E.note(ok and "help: %s on %s (%s %d/%d)" or "help: %s on %s (%s %d/%d): %s", s.name, m.name, vital, v.current, v.max,
		tostring(why))
	if ok then
		E.tally.helps = (E.tally.helps or 0) + 1
	elseif string.find(tostring(why), OUT_OF_RANGE, 1, true) and need.object ~= nil and E.can("nav") then
		-- Beyond the spell's reach: closer, and the help tried again at once.
		helped[m.id .. ":" .. vital] = nil
		E.walk(need.object, NEAR_ENOUGH)
	end
	return true
end

---------------------------------------------------------------------------------------------------
-- Running a fellowship (AutoFellowManagement)

local SIGN = "[VT Fellow Manager] "
local TAIL = " -v-"
-- How near a player must stand to be recruited, metres, and how long one too far is given.
local RECRUIT_NEAR, RECRUIT_WAIT = 20, 20
-- The most fellows a fellowship holds.
local FULL = 9
-- How long a vote runs, seconds; the commands answered in a while, and the while.
local VOTE_SECONDS, COMMANDS, COMMAND_WINDOW = 120, 8, 180

-- The players waiting for a place, in order: { name, id }.
local line = {}
-- The players being recruited: name lower -> { name, id, chance, at }.
local recruiting = {}
-- The votes: n -> { by, what, target, yes = {}, no = {}, ends, done }.
local votes, vote_count = {}, 0
-- When each command answered was answered.
local answered = {}

local function tell(name, text)
	pcall(chat.command, "/t " .. name .. ", " .. SIGN .. text .. TAIL)
end

local function fellowship_say(text)
	pcall(chat.command, "/f " .. SIGN .. text .. TAIL)
end

local function leading()
	local f = game.player.fellowship
	return f ~= nil and f.leader == game.player.id, f
end

local function member(f, name)
	for _, m in ipairs(f and f.members or {}) do
		if F.E.same_name(m.name, name) then
			return m
		end
	end
	return nil
end

-- A tell to the character: its teller's name and object id (when the line names it), and its words.
function F.parse_tell(text)
	local id, name, said = string.match(text or "", "^<Tell:IIDString:(%d+):[^>]*>(.-)<\\Tell> tells you, \"(.*)\"$")
	if name == nil then
		name, said = string.match(text or "", "^(.-) tells you, \"(.*)\"$")
	end
	if name == nil then
		return nil
	end
	return name, tonumber(id), said
end

-- The object of the player named `name` (by `id` when the line gave it), or nil.
local function player_object(name, id)
	local o = id ~= nil and id ~= 0 and game.world:get(id) or nil
	if o == nil then
		o = F.E.character_named(name)
	end
	return o
end

-- Recruit the player when it stands near enough; else, once, say so and wait for it.
local function recruit(r)
	local E = F.E
	local o = player_object(r.name, r.id)
	local d = o and o:distance()
	if d == nil or d > RECRUIT_NEAR then
		if r.chance then
			recruiting[string.lower(r.name)] = nil
			E.note("fellow: %s did not come near enough to be recruited", r.name)
			return
		end
		r.chance, r.at = true, os.clock() + RECRUIT_WAIT
		E.logk("Timers", "fellow: %s is %s m off", r.name, d and string.format("%.1f", d) or "where the client does not know")
		tell(r.name, "You are too far away. I will wait " .. RECRUIT_WAIT .. " seconds and give you one more chance.")
		return
	end
	recruiting[string.lower(r.name)] = nil
	if not E.can("social") then
		return
	end
	task.spawn(function()
		local ok, why = pawait(social.fellowship_recruit(o.id))
		E.note(ok and "fellow: recruited %s" or "fellow: recruiting %s: %s", r.name, tostring(why))
	end)
end

-- A command a player told the character; answers whether it was one.
local function command(name, id, said)
	local words = {}
	for w in string.gmatch(string.lower(said or ""), "%S+") do
		table.insert(words, w)
	end
	local verb = words[1]
	if verb ~= "xp" and verb ~= "line" and verb ~= "remove" and verb ~= "leader" and verb ~= "startvote"
		and verb ~= "vote" and verb ~= "help" then
		return false
	end
	local lead, f = leading()
	if not lead then
		return true
	end
	-- At most eight answered in any three minutes.
	while answered[1] ~= nil and os.clock() - answered[1] > COMMAND_WINDOW do
		table.remove(answered, 1)
	end
	if #answered >= COMMANDS then
		return true
	end
	table.insert(answered, os.clock())
	local E = F.E
	E.note("fellow: %s says %s", name, said)
	if verb == "xp" then
		if member(f, name) then
			tell(name, "You are already in the fellowship.")
		elseif #(f.members or {}) >= FULL then
			for _, w in ipairs(line) do
				if string.lower(w.name) == string.lower(name) then
					tell(name, "You are already in line.")
					return true
				end
			end
			table.insert(line, { name = name, id = id })
			tell(name, string.format("The fellowship is full. You are number %d in line.", #line))
		else
			tell(name, "I will recruit you in a moment. Please stand close to me.")
			recruiting[string.lower(name)] = { name = name, id = id, chance = false, at = os.clock() + 3 }
		end
	elseif verb == "line" then
		local names = {}
		for _, w in ipairs(line) do
			table.insert(names, w.name)
		end
		tell(name, string.format("%d in the fellowship. Waiting: %s.", #(f.members or {}),
			#names > 0 and table.concat(names, ", ") or "no one"))
	elseif verb == "remove" then
		for i, w in ipairs(line) do
			if string.lower(w.name) == string.lower(name) then
				table.remove(line, i)
				tell(name, "You are out of line.")
				return true
			end
		end
		tell(name, "You are not in line.")
	elseif verb == "leader" then
		tell(name, "The fellowship leader is " .. tostring(game.player.name) .. ".")
	elseif verb == "startvote" then
		if words[2] ~= "giveleader" or words[3] == nil or not member(f, words[3]) then
			tell(name, "Start a vote with: startvote giveleader <a fellow's name>")
			return true
		end
		local target = member(f, words[3])
		vote_count = vote_count + 1
		votes[vote_count] = { by = name, what = "giveleader " .. target.name, target = target, yes = {}, no = {},
			ends = os.clock() + VOTE_SECONDS }
		fellowship_say(string.format("%s has called a new vote: giveleader %s! To vote either tell me 'vote %d yes' or 'vote %d no'. You have 2 minutes.",
			name, target.name, vote_count, vote_count))
	elseif verb == "vote" then
		local n = tonumber(words[2] or "")
		local v = n and votes[n]
		if v == nil or v.done or (words[3] ~= "yes" and words[3] ~= "no") or not member(f, name) then
			tell(name, "Vote with: vote <number> yes, or vote <number> no")
			return true
		end
		v.yes[string.lower(name)], v.no[string.lower(name)] = nil, nil
		v[words[3]][string.lower(name)] = true
		tell(name, string.format("Your vote on '%s' is %s.", v.what, words[3]))
	elseif verb == "help" then
		tell(name, "Available commands: xp, line, remove, leader, startvote, vote, help")
	end
	return true
end

-- A line of chat: a tell to the character is a fellow command when AutoFellowManagement is on.
function F.chat_line(e)
	if e.kind ~= "tell" or not S.get("AutoFellowManagement") then
		return
	end
	local name, id, said = F.parse_tell(e.text)
	if name ~= nil then
		command(name, id, said)
	end
end

-- The fellowship looked after, twice a second: the players asked to come near recruited, the next in
-- line recruited once there is a place, and the votes counted when they end.
task.every(500, function()
	if not S.get("AutoFellowManagement") then
		return
	end
	local lead, f = leading()
	if not lead then
		return
	end
	for _, r in pairs(recruiting) do
		if os.clock() >= r.at then
			recruit(r)
		end
	end
	if line[1] ~= nil and #(f.members or {}) < FULL and next(recruiting) == nil then
		local w = table.remove(line, 1)
		tell(w.name, "There is a place now. I will recruit you in a moment. Please stand close to me.")
		recruiting[string.lower(w.name)] = { name = w.name, id = w.id, chance = false, at = os.clock() + 3 }
	end
	for n, v in pairs(votes) do
		if not v.done and os.clock() >= v.ends then
			v.done = true
			local yes, all = 0, 0
			for _ in pairs(v.yes) do
				yes, all = yes + 1, all + 1
			end
			for _ in pairs(v.no) do
				all = all + 1
			end
			fellowship_say(string.format("Vote total for '%s' (%d): %d/%d", v.what, n, yes, all))
			if yes * 2 > all and F.E.can("social") then
				task.spawn(function()
					local ok, why = pawait(social.fellowship_leader(v.target.id))
					F.E.note(ok and "fellow: %s leads now" or "fellow: giving the lead to %s: %s", v.target.name, tostring(why))
				end)
			end
		end
	end
end)

return F
