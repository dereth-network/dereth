-- The macro: one loop that, a few times a second, picks the most pressing thing to do and does a
-- step of it. In order: wait out portal space and death; stop or recall when told to; heal when
-- health is low; fight the best monster in range; loot the corpses of what it killed; rest when
-- stamina or mana is low and nothing is near; otherwise walk the route.
--
-- Every step is short (one swing, one heal, one corpse, one leg of the route), so a monster that
-- comes near while walking, or health that drops while fighting, is answered at the next step. A
-- leg of the route is stopped as soon as something more pressing appears.

local S = require("settings")
local monsters = require("monsters")
local spells = require("spells")
local buffs = require("buffs")
local recharge = require("recharge")
local vendors = require("vendor")
local pack = require("pack")
local fellows = require("fellows")
local pets = require("pets")
local bb = require("buffbot")
local fight = require("dereth.combat")
local navlib = require("dereth.nav")
local lootlib = require("dereth.loot")

local E = {}
fellows.E = E
pets.E = E
bb.E = E

-- How near a melee attack is made from, in metres.
local MELEE = 2.0
-- How far above or below the character a monster may be and still count as on its floor, in
-- metres.
local FLOOR = 4.0
-- Corpses this near are opened where the character stands, and of those the one it turns least
-- to face is looted first (VTank's corpse choice); further ones are walked to, nearest first.
local NEAR_CORPSE = 3.0

E.running = false
E.state = "Stopped"
E.route = nil
E.profile = nil
E.target = nil -- the monster being fought: { id, name }
E.why_stopped = nil

-- The run's tallies, and its log.
E.tally = {}
local function reset_tally()
	E.tally = {
		kills = 0, deaths = 0, looted = 0, kept = 0, stuck = 0, skipped = 0, waypoints = 0,
		heals = 0, recharges = 0, rests = 0, given_up = 0, swings = 0, buffs = 0, debuffs = 0, started = os.clock(), started_at = os.time(),
	}
end
reset_tally()
E.log = {}

-- Log lines not yet kept with the character.
E.unsaved = {}

-- Keep the run's log with the character (the last 1,000 lines), so it can be read after the
-- client has closed. Done now and then, not each line: a store write carries the whole table.
-- The log as kept with the character, read once a character (reading it back each time cost a
-- frame 14 ms live: a thousand lines).
local kept_log = nil
function E.forget_kept_log()
	kept_log = nil
end

function E.save_log()
	if #E.unsaved == 0 then
		return
	end
	local ok = pcall(function()
		local kept = kept_log or store.character.tank_log or {}
		for _, line in ipairs(E.unsaved) do
			table.insert(kept, line)
		end
		local drop = #kept - 1000
		if drop > 0 then
			local trimmed = {}
			for i = drop + 1, #kept do
				table.insert(trimmed, kept[i])
			end
			kept = trimmed
		end
		store.character.tank_log = kept
		kept_log = kept
	end)
	if ok then
		E.unsaved = {}
	end
end

-- The log's types, VTank's (its /vt log): each line is of one, and a type the player turns off
-- (the LogOff setting) is not logged.
E.LOG_TYPES = { "ActiveRule", "SalvageList", "SpellCast", "RuleInfo", "Timers", "CastInfo", "DebuffChoice",
	"Loot", "CharProps", "Misc" }

-- The log's type of that name, without case, or nil.
function E.log_type(name)
	for _, t in ipairs(E.LOG_TYPES) do
		if string.lower(t) == string.lower(tostring(name or "")) then
			return t
		end
	end
	return nil
end

-- Whether lines of `kind` are logged.
function E.logging(kind)
	return not S.listed("LogOff", kind)
end

-- A line of type `kind` in the macro's log, unless the type is off.
local write
function E.logk(kind, fmt, ...)
	if E.logging(kind) then
		write(fmt, ...)
	end
end

-- A line in the macro's log (of type Misc): kept for the window and `/tank log`, and printed to
-- the console.
function E.note(fmt, ...)
	E.logk("Misc", fmt, ...)
end

function write(fmt, ...)
	-- A missing name (an item not yet described, a rule with none) prints as "?", not an error.
	local args = table.pack(...)
	for i = 1, args.n do
		if args[i] == nil then
			args[i] = "?"
		end
	end
	local ok, text = pcall(string.format, fmt, table.unpack(args, 1, args.n))
	if not ok then
		text = fmt
	end
	local line = string.format("%7.1f %s", os.clock() - (E.tally.started or 0), text)
	table.insert(E.log, line)
	while #E.log > 300 do
		table.remove(E.log, 1)
	end
	print(line)
	table.insert(E.unsaved, line)
	if E.on_note then
		E.on_note(line)
	end
end
buffs.note = function(fmt, ...)
	E.logk("SpellCast", fmt, ...)
end

local function set_state(s)
	if E.state ~= s then
		E.state = s
		if E.on_state then
			E.on_state(s)
		end
	end
end
E.set_state = set_state

-- Vitals as shares of their maximum (nil where the host cannot tell).
local function share(vital)
	local v = game.player[vital]
	if v == nil or v.max == nil or v.max == 0 then
		return nil
	end
	return v.current / v.max
end
E.share = share

function E.vitals_text()
	local function pct(v)
		local s = share(v)
		return s and string.format("%d%%", math.floor(s * 100 + 0.5)) or "?"
	end
	return string.format("H %s S %s M %s", pct("health"), pct("stamina"), pct("mana"))
end

---------------------------------------------------------------------------------------------------
-- Monsters

-- Monsters left alone for a while (could not be reached, or would not die), by id: when.
local ignored = {}
-- The monsters attacked lately, by id: { name, at (clock), place } when last seen.
local attacked = {}
-- Kills waiting for their corpses: { name, at, place }.
local pending_kills = {}

-- How many times each monster has been left alone, so one that keeps failing is left longer.
local strikes = {}
-- The monsters known dead (their corpse seen) whose bodies have not gone yet.
local dead_ids = {}
-- The monsters taken for ghosts (there to the client, not to the server), by id, and the casts
-- at each that never began.
local ghosts = {}
local ghost_casts = {}

local function is_ignored(id)
	local t = ignored[id]
	return t ~= nil and os.clock() < t
end

-- Leave a monster alone: for the blacklist time, doubled each time it is left again (at most ten
-- minutes), so an unreachable one does not hold the macro.
local function blacklist(id, name, why)
	strikes[id] = (strikes[id] or 0) + 1
	local secs = math.min(S.get("BlacklistMonsterTimeoutSeconds") * 2 ^ (strikes[id] - 1), 600)
	ignored[id] = os.clock() + secs
	E.tally.given_up = E.tally.given_up + 1
	E.logk("Timers", "leaving %s alone for %d s (%s)", name or "a monster", math.floor(secs), why)
	if E.target ~= nil and E.target.id == id then
		E.target = nil
	end
end

-- A ghost: a monster the client shows that the server no longer has. It is left alone until the
-- client lets it go (a plugin cannot make the client forget an object, as VTank's deletion does).
local function ghost(id, name, why)
	if ghosts[id] then
		return
	end
	ghosts[id] = true
	ignored[id] = math.huge
	E.tally.given_up = E.tally.given_up + 1
	E.logk("Timers", "%s is a ghost (%s): left alone until it goes", name or "a monster", why)
	if E.target ~= nil and E.target.id == id then
		E.target = nil
	end
end

-- VTank's /vt deletemonster: the monster `o` taken for a ghost.
function E.ghost(o)
	ghost(o.id, o.name, "marked by you")
end

-- Whether a failed cast's reason says the target is not there: the client would not send it at
-- that target, or the server found no such target.
local function no_such_target(why)
	local w = tostring(why)
	return string.find(w, "refused by the client", 1, true) ~= nil or string.find(w, "0x0403", 1, true) ~= nil
end

-- A cast at `o` ended: one that never began because `o` is not there counts toward it being a
-- ghost (DeleteGhostMonsters, GhostMonsterSpellAttemptCount casts running); one that began clears
-- the count.
local function cast_at(o, ok, why)
	if ok then
		ghost_casts[o.id] = nil
		return
	end
	if not no_such_target(why) then
		return
	end
	ghost_casts[o.id] = (ghost_casts[o.id] or 0) + 1
	if S.get("DeleteGhostMonsters") and ghost_casts[o.id] >= S.get("GhostMonsterSpellAttemptCount") then
		ghost(o.id, o.name, string.format("%d casts at it never began", ghost_casts[o.id]))
	end
end

-- Lines of sight, the client's own (its `nav.path` capability): whether a straight line to a
-- monster is clear of walls, the ground and closed doors, what a bolt or an arrow would hit first.
-- Without the capability, every monster counts as in sight.

-- Lines of sight looked along lately, by monster id: { clear, at }.
local sights = {}
-- How long a line of sight is taken as still holding, seconds.
local SIGHT_KEPT = 1

-- Whether the character sees `o` along a straight line; with `fresh`, looked at again now. True
-- where the host cannot tell.
local function in_sight(o, fresh)
	if not host.has("nav.path") then
		return true
	end
	local c = sights[o.id]
	if c ~= nil and not fresh and os.clock() - c.at < SIGHT_KEPT then
		return c.clear
	end
	local clear = nav.sight(o)
	if clear == nil then
		clear = true
	end
	sights[o.id] = { clear = clear, at = os.clock() }
	return clear
end
E.in_sight = in_sight


-- Degrees the character turns to face `o` (0 to 180).
local function turn_to(o)
	local ax, ay = E.global_of(game.player.position)
	local bx, by = E.global_of(o.position)
	local h = game.player.heading
	if ax == nil or bx == nil or h == nil then
		return 0
	end
	local bearing = math.deg(math.atan2(bx - ax, by - ay))
	return math.abs(((bearing - h + 540) % 360) - 180)
end
E.turn_to = turn_to

-- Whether monster `a` (`da` metres off) is fought before `b` (`db` off), of one priority, by
-- TargetSelectMethod: 1 the nearer; 2 the one the character turns less to face; 3 by turning
-- within TargetSelectAngleRange and by distance beyond it, the near ones first.
local function before(a, da, b, db)
	local method = S.get("TargetSelectMethod")
	if method == 2 then
		return turn_to(a) < turn_to(b)
	elseif method == 3 then
		local near = S.metres("TargetSelectAngleRange")
		local an, bn = da <= near, db <= near
		if an ~= bn then
			return an
		end
		if an then
			return turn_to(a) < turn_to(b)
		end
	end
	return da < db
end

-- How near a monster must be to be engaged: the attack distance, or with navigation on the
-- approach distance (a monster that near is run toward).
local function engage_distance()
	local d = S.metres("AttackDistance")
	if S.get("EnableNav") then
		d = math.max(d, S.metres("ApproachDistance"))
	end
	return d
end

-- The monster to fight within `range` (the engage distance when not given) that is not being
-- left alone: the highest priority, of those one in a straight line of sight before one behind a
-- wall (a bolt or an arrow would hit the wall, and it is reached the long way round), and then
-- the first by TargetSelectMethod. The current target is kept while it is in range, unless one of
-- higher priority comes, or it has gone out of sight and one of its priority is in sight; with
-- TargetLock, the monster the player selects is fought first, unless one of higher priority is
-- about.
function E.choose_target(range)
	range = range or engage_distance()
	local min = S.metres("AttackMinimumDistance")
	local current = nil
	if E.target ~= nil then
		local o = game.world:get(E.target.id)
		local d = o and o:distance()
		if o ~= nil and d ~= nil and d <= range + 5 and not is_ignored(o.id) then
			current = o
		end
	end
	local selected = S.get("TargetLock") and game.selection() or nil
	local sel, sel_p = nil, nil
	local best, best_d, best_p, best_seen, best_vuln = nil, nil, nil, nil, false
	local peered = fellows.in_peer_fellow()
	local me = game.player.position
	for _, o in ipairs(game.world:find({ monster = true, within = range })) do
		local d = o:distance()
		-- A monster on another floor (well above or below) is not one to run at.
		local p = o.position
		local level = me == nil or p == nil or me.z == nil or p.z == nil or p.cell == nil
			or bit32.rshift(p.cell, 16) ~= bit32.rshift(me.cell, 16) or math.abs(p.z - me.z) <= FLOOR
		-- One right beside the character is fought whatever it was left for: it is attacking.
		-- (Once only: one that keeps failing from beside the character is out of reach after all,
		-- on a ledge or behind a rail. Never a ghost.)
		local free = not is_ignored(o.id)
			or (d ~= nil and d <= MELEE + 1 and dead_ids[o.id] == nil and not ghosts[o.id] and (strikes[o.id] or 0) < 2)
		-- The monster list: priority -1 is left alone, and a higher priority is fought first.
		local row = monsters.match(o, E.meta_state)
		local p = row.priority or 1
		if d ~= nil and d >= min and level and free and not ghosts[o.id] and p >= 0 then
			local seen = in_sight(o)
			-- In a fellow of the Tank's own (VI2's), a monster one of them has made vulnerable is
			-- fought first.
			local vuln = peered and E.vulned_by_any(o)
			if best == nil or p > best_p or (p == best_p and seen and not best_seen)
				or (p == best_p and seen == best_seen and vuln and not best_vuln)
				or (p == best_p and seen == best_seen and vuln == best_vuln and before(o, d, best, best_d)) then
				best, best_d, best_p, best_seen, best_vuln = o, d, p, seen, vuln
			end
			if selected ~= nil and o.id == selected.id then
				sel, sel_p = o, p
			end
		end
	end
	-- The player's selection, while it is one to fight and none of higher priority is about.
	if sel ~= nil and sel_p >= best_p then
		return sel
	end
	-- The current target stays unless a monster of higher priority has come, or it is out of
	-- sight and one of its priority is in sight.
	if current ~= nil and best ~= nil and best.id ~= current.id then
		local cp = monsters.match(current, E.meta_state).priority or 1
		if cp > best_p or (cp == best_p and (in_sight(current) or not best_seen)) then
			return current
		end
	end
	return best
end

local function monster_near(range)
	return S.get("EnableCombat") and E.choose_target(range) ~= nil
end

-- The distance within which a monster is fought where it stands (rather than run at).
local function attack_distance()
	return math.max(S.metres("AttackDistance"), MELEE + 1)
end

-- A monster close enough to fight now: what interrupts looting.
local function monster_close()
	return monster_near(attack_distance())
end

---------------------------------------------------------------------------------------------------
-- Equipment

local function wielded(class)
	local me = game.player.id
	for _, it in ipairs(game.inventory.items()) do
		if it.wielder == me and (class == nil or it.class == class) then
			return it
		end
	end
	return nil
end

local function is_ammo(it)
	local n = string.lower(it.name or "")
	return it.class == "MissileWeapon"
		and (n:find("arrow") or n:find("bolt") or n:find("quarrel") or n:find("dart")) ~= nil
end

-- A grenade: a thrown weapon the Consumables list names (VTank's alchemical grenades, which it
-- uses from its Consumables).
local function is_grenade(it)
	return it.class == "MissileWeapon" and not is_ammo(it) and S.listed("Consumables", it.name)
end

-- The weapon the macro fights with: one already wielded, else the first melee weapon, else the
-- first missile weapon (and its ammunition) from the pack.
function E.arm()
	local me = game.player.id
	local weapon = nil
	-- The weapon named in the settings, taken up in place of any other.
	local want = S.get("Weapon")
	if want ~= "" then
		local named, current = nil, nil
		for _, it in ipairs(game.inventory.items()) do
			if string.lower(it.name or "") == string.lower(want) and named == nil then
				named = it
			end
			if it.wielder == me and (it.class == "MeleeWeapon" or it.class == "MissileWeapon" or it.class == "WandStaffOrb") and not is_ammo(it) then
				current = it
			end
		end
		if named == nil then
			E.note("no %s in the pack", want)
		elseif named.wielder ~= me then
			if current ~= nil then
				pawait(items.unwield(current.id))
			end
			local ok, why = pawait(items.wield(named.id))
			E.note(ok and "wielding %s" or "could not wield %s: %s", named.name, tostring(why))
		end
	end
	for _, it in ipairs(game.inventory.items()) do
		if it.wielder == me and (it.class == "MeleeWeapon" or (it.class == "MissileWeapon" and not is_ammo(it))) then
			weapon = it
		end
	end
	if weapon == nil then
		for _, class in ipairs({ "MeleeWeapon", "MissileWeapon" }) do
			for _, it in ipairs(game.inventory.items()) do
				if weapon == nil and it.class == class and not is_ammo(it) and not is_grenade(it) then
					local ok, why = pawait(items.wield(it.id))
					if ok then
						weapon = it
						E.note("wielding %s", it.name)
					else
						E.note("could not wield %s: %s", it.name, tostring(why))
					end
				end
			end
		end
	end
	if weapon ~= nil and weapon.class == "MissileWeapon" then
		E.ready_ammo()
	end
	E.missile = weapon ~= nil and weapon.class == "MissileWeapon"
	return weapon
end

-- Ammunition readied for a missile weapon, when none is: taking up a caster puts it away.
function E.ready_ammo()
	local me = game.player.id
	local want = string.lower(S.get("Ammo") or "")
	for _, it in ipairs(game.inventory.items()) do
		if it.wielder == me and is_ammo(it) and (want == "" or string.lower(it.name or "") == want) then
			return true
		end
	end
	for _, it in ipairs(game.inventory.items()) do
		if is_ammo(it) and it.wielder ~= me and (want == "" or string.lower(it.name or "") == want) then
			local ok = pawait(items.wield(it.id))
			if ok then
				E.note("readying %s", it.name)
				return true
			end
		end
	end
	return false
end

---------------------------------------------------------------------------------------------------
-- Steps. Each answers after doing one short piece of work.

-- Walk to `where` (an object or a place) within `range`, stopping early when `interrupt()` says
-- something more pressing has come up. Answers ok, why.
local function walk(where, range, interrupt)
	local p = nav.move_to(where, range)
	local watch = nil
	if interrupt ~= nil then
		watch = task.every(300, function()
			if interrupt() then
				nav.stop()
			end
		end)
	end
	local ok, why = pawait(p)
	if watch ~= nil then
		task.cancel(watch)
	end
	return ok, why
end
E.walk = walk

-- Wait up to `secs` seconds for `cond()`; answers whether it came true.
local function wait_for(cond, secs)
	local until_t = os.clock() + secs
	while os.clock() < until_t do
		if cond() then
			return true
		end
		task.sleep(200)
	end
	return cond()
end

-- How far from its landblock's corner a place inside a dungeon lies at most, metres: a dungeon's
-- rooms spread a few hundred metres; a place further is on the land.
local DUNGEON_REACH = 1000

-- A place a route names by map coordinates, taken inside the dungeon the character stands in: the
-- character's own cell (which names the dungeon to the client's planner) and the place's offset in
-- the landblock, as a dungeon's own places are given. Map coordinates alone name the land above
-- it, and a journey there would leave the dungeon. Anything else is as it was.
local function in_dungeon(where)
	local me = game.player.position
	if type(where) ~= "table" or where.ns == nil or where.ew == nil or me == nil or me.ns ~= nil
		or me.cell == nil or bit32.band(me.cell, 0xFFFF) < 0x100 then
		return where
	end
	local bx = bit32.rshift(me.cell, 24)
	local by = bit32.band(bit32.rshift(me.cell, 16), 0xFF)
	local x = (where.ew + 101.9) * 240 - bx * 192
	local y = (where.ns + 101.9) * 240 - by * 192
	if math.abs(x) > DUNGEON_REACH or math.abs(y) > DUNGEON_REACH then
		return where
	end
	return { cell = me.cell, x = x, y = y, z = where.z or 0 }
end
E.in_dungeon = in_dungeon

-- The highest drop off a ledge a journey steps down when only that drop leads on, metres: what the
-- body steps down without falling. And how many it steps down on one journey.
local STEP_DOWN, STEP_DOWNS = 1.5, 3

-- The drop a journey's failure names (`only_by_drop`): its height, where to stand at the ledge's
-- edge and where the body lands, each a cell and offset; or nil.
local function drop_of(r)
	local n = "([%-%d.]+)"
	local h, c1, x1, y1, z1, c2, x2, y2, z2 = string.match(tostring(r),
		"^only_by_drop: a drop of ([%d.]+) m, off the ledge at (%x+) %(" .. n .. ", " .. n .. ", " .. n
		.. "%) to (%x+) %(" .. n .. ", " .. n .. ", " .. n .. "%)")
	if h == nil then
		return nil
	end
	local place = function(c, x, y, z)
		return { cell = tonumber(c, 16), x = tonumber(x), y = tonumber(y), z = tonumber(z) }
	end
	return tonumber(h), place(c1, x1, y1, z1), place(c2, x2, y2, z2)
end

-- A journey to `where` (a place, an object, or what `nav.path_to` takes), with its legs noted.
-- Answers ok, why. Without the host's journeys, or with them switched off, answers false. Where
-- only a drop off a ledge leads on (the client's journeys leave a drop to the player), one the body
-- steps down without falling is taken: walked to, stepped off, and the journey made again.
local function travel(where, why)
	if not S.get("NavJourneys") or not host.has("nav.journey") then
		return false, "no journeys"
	end
	E.note("travelling %s", why)
	local opts = {
		on_planned = function(e)
			local kinds = {}
			for i, leg in ipairs(e.legs) do
				kinds[i] = leg.kind == "portal" and ("portal " .. tostring(leg.name))
					or leg.kind == "recall" and ("recall " .. tostring(leg.recall))
					or leg.kind == "hand_back" and ("hand back to " .. tostring(leg.name))
					or leg.kind
			end
			E.note("journey%s: %s (%d s)", e.reason and (" again (" .. e.reason .. ")") or "",
				table.concat(kinds, ", "), math.floor(e.seconds or 0))
		end,
		on_learned = function(e)
			E.note("learned: %s %s", e.kind, tostring(e.name or e.recall))
		end,
	}
	local ok, r = pawait(nav.path_to(in_dungeon(where), opts))
	local downs = 0
	while not ok and downs < STEP_DOWNS do
		local h, stand, land = drop_of(r)
		if h == nil or h > STEP_DOWN then
			break
		end
		downs = downs + 1
		E.note("journey: only a drop of %.1f m off a ledge leads on; stepping down it", h)
		if pawait(nav.path_to(stand, { portals = false, recalls = false, show = false })) then
			pawait(nav.move_to(land, 1))
		end
		ok, r = pawait(nav.path_to(in_dungeon(where), opts))
	end
	if ok then
		E.tally.journeys = (E.tally.journeys or 0) + 1
		E.note("arrived %.1f m from it (%d portals, %d recalls)", r.distance or 0, r.portals or 0, r.recalls or 0)
	elseif r == "stopped" then
		E.note("journey stopped")
	else
		E.note("journey failed: %s", tostring(r))
	end
	return ok, r
end
E.travel = travel

-- Get unstuck: turn to one side, run a moment, and stop.
local stuck_side = 1
local function unstick()
	E.tally.stuck = E.tally.stuck + 1
	local h = game.player.heading or 0
	stuck_side = -stuck_side
	local away = (h + 180 + 70 * stuck_side) % 360
	E.note("stuck: backing off toward %d degrees", math.floor(away))
	pawait(nav.face(away))
	nav.autorun(true)
	task.sleep(1500)
	nav.autorun(false)
	task.sleep(300)
end

-- Doors (VTank's OpenDoors). Before a leg of the route is walked, a closed door standing across it
-- (within DoorOpenRange of the way) is opened: used, and waited for until it stands open
-- (ethereal), as the client's own journeys open the doors on their way. A door that does not open
-- is locked: its lock is looked at (an appraisal, from within DoorIDRange) and picked with a lockpick
-- the Consumables allow when the character's lockpicking and the pick's bonus beat the lock's
-- resistance by DoorLockpickDiffExcessThreshold (VTank's lockpicks for doors), then the door is used
-- again. One it cannot pick is left as it is: the leg is walked as before (its stuck recovery may
-- find a way round, or the waypoint is skipped), and the route says once that a locked door blocks
-- it. A walk to a monster or a corpse along the client's route opens the doors that route passes
-- the same way.

-- How long a used door is waited for, seconds; and how many times a lock is picked at.
local DOOR_WAIT, PICK_TRIES = 3, 3
-- The properties an appraisal tells: a lock's resistance to picking, a lockpick's bonus.
local RESIST_LOCKPICK, LOCKPICK_MOD = 173, 88
-- The doors found locked this run, by id.
local locked_doors = {}

function E.forget_doors()
	locked_doors = {}
end

-- The nearest closed door across the leg from the character to `to`, or nil.
local function door_across(to)
	local ax, ay = E.global_of(game.player.position)
	local bx, by = E.global_of(to)
	if ax == nil or bx == nil then
		return nil
	end
	local dx, dy = bx - ax, by - ay
	local len2 = dx * dx + dy * dy
	if len2 < 0.25 then
		return nil
	end
	if not S.get("OpenDoors") then
		return nil
	end
	local reach = S.metres("DoorOpenRange")
	local best, best_t = nil, nil
	for _, d in ipairs(game.world:find({ class = "Door", within = math.sqrt(len2) + reach + 2 })) do
		if d.open == false and not locked_doors[d.id] then
			local x, y = E.global_of(d.position)
			if x ~= nil then
				local t = ((x - ax) * dx + (y - ay) * dy) / len2
				if t > 0 and t < 1 then
					local off = math.sqrt((x - (ax + t * dx)) ^ 2 + (y - (ay + t * dy)) ^ 2)
					if off <= reach and (best_t == nil or t < best_t) then
						best, best_t = d, t
					end
				end
			end
		end
	end
	return best
end

-- Use `door` and wait for it to stand open. Answers whether it did, and the use's failure.
local function use_door(door)
	local ok, why = pawait(act.use(door.id))
	local opened = ok and wait_for(function()
		local d = game.world:get(door.id)
		return d == nil or d.open == true
	end, DOOR_WAIT)
	return opened, ok and nil or why
end

-- The character's lockpicking skill, or nil when it has none trained.
local function lockpicking()
	for _, sk in ipairs(game.player.skills or {}) do
		if sk.name == "Lockpick" then
			return sk.training ~= "untrained" and sk.current or nil
		end
	end
	return nil
end

-- The lockpick to pick with: of those the Consumables allow, the one of the highest bonus (each
-- appraised once), and its bonus. Or nil.
local pick_bonus = {}
local function lockpick()
	local best, best_b = nil, nil
	for _, it in ipairs(game.inventory.items()) do
		if it.class == "Lockpick" and recharge.consumable(it.name) and (it.uses == nil or it.uses > 0) then
			local b = pick_bonus[it.id]
			if b == nil then
				local ok, a = pawait(act.appraise(it.id))
				b = ok and a and a.ints and a.ints[LOCKPICK_MOD] or 0
				pick_bonus[it.id] = b
			end
			if best == nil or b > best_b then
				best, best_b = it, b
			end
		end
	end
	return best, best_b or 0
end

-- Pick the lock of `door` and open it, when the character can (VTank's
-- DoorLockpickDiffExcessThreshold): its lock looked at from within DoorIDRange, then picked at a
-- few times, the door used after each. Answers whether it opened, and why not.
local function pick_lock(door)
	local skill = lockpicking()
	if skill == nil then
		return false, "no lockpicking skill"
	end
	local pick, bonus = lockpick()
	if pick == nil then
		return false, "no lockpick"
	end
	local d = door:distance()
	if d ~= nil and d > S.metres("DoorIDRange") then
		walk(door, S.metres("DoorIDRange"))
	end
	local ok, a = pawait(act.appraise(door.id))
	local resist = ok and a and a.ints and a.ints[RESIST_LOCKPICK] or nil
	if resist ~= nil and skill + bonus < resist + S.get("DoorLockpickDiffExcessThreshold") then
		return false, string.format("its lock (%d) is beyond picking (%d)", resist, skill + bonus)
	end
	for i = 1, PICK_TRIES do
		local picked, why = pawait(act.use_on(pick.id, door.id))
		if not picked then
			E.note("picking %s with %s: %s", tostring(door.name), pick.name, tostring(why))
		end
		if use_door(door) then
			E.tally.picked = (E.tally.picked or 0) + 1
			E.note("picked the lock of %s (%d tries)", tostring(door.name), i)
			return true
		end
	end
	return false, string.format("its lock did not give in %d tries", PICK_TRIES)
end

-- Use `door` and wait for it to stand open, picking its lock when it is locked and the character
-- can. Answers true once it stands open; one that does not is taken for locked (false, and why).
local function open_door(door)
	local opened, why = use_door(door)
	if not opened then
		opened, why = pick_lock(door)
	end
	if opened then
		E.tally.doors = (E.tally.doors or 0) + 1
		return true
	end
	locked_doors[door.id] = true
	return false, why
end

-- Open the closed door across the leg to `to`, if there is one; answers when it stands open, or
-- once it is taken for locked.
local function open_door_across(to)
	local door = door_across(to)
	if door == nil then
		return
	end
	E.note("route: opening %s", tostring(door.name))
	local opened, why = open_door(door)
	if opened then
		E.note("route: %s is open", tostring(door.name))
		return
	end
	E.note("route: a locked door (%s) blocks the way to waypoint %d%s", tostring(door.name), E.route and E.route.at or 0,
		why and (" (" .. tostring(why) .. ")") or "")
end

-- Walking routes, the client's own (its `nav.path` capability): the way to walk to a monster or a
-- corpse round walls, through doors and across a dungeon's rooms. Without the capability, every
-- walk goes straight.

-- The walking routes to monsters and corpses, by id: { points, doors, at, gx, gy, next }, `next`
-- the corner walked to next and `gx`, `gy` where the object stood when it was planned.
local routes = {}
-- A route is planned again once its object has moved this far from where it stood, metres, or
-- once it is this old, seconds.
local REROUTE_METRES, REROUTE_SECONDS = 3, 10
-- How near a corner of a route counts as reaching it, metres.
local CORNER = 1

-- The route to `o`, planned again when it has moved or the route is old. Answers it, or nil and
-- why there is none (`no_route` when no way walks there).
local function route_to(o)
	local r = routes[o.id]
	local gx, gy = E.global_of(o.position)
	if r ~= nil and os.clock() - r.at < REROUTE_SECONDS
		and (gx == nil or r.gx == nil or math.sqrt((gx - r.gx) ^ 2 + (gy - r.gy) ^ 2) <= REROUTE_METRES) then
		return r
	end
	local ok, got = pawait(nav.path(o))
	if not ok then
		routes[o.id] = nil
		return nil, got
	end
	r = { points = got.points or {}, doors = got.doors or {}, at = os.clock(), gx = gx, gy = gy, next = 2 }
	routes[o.id] = r
	-- A route that bends or passes a door is told; a straight one is a plain walk.
	if #r.points > 2 or #r.doors > 0 then
		E.note("route to %s: %d corners, %.0f m%s", tostring(o.name), #r.points, got.length or 0,
			#r.doors > 0 and string.format(", %d doors", #r.doors) or "")
	end
	return r
end

-- People never fought: a melee charge stops against them as against a wall.
local PEOPLE = { Npc = true, Vendor = true, Player = true }
-- How near the straight line to a monster someone stands in its way, metres: the two bodies'
-- widths.
local BODY = 1.0

-- Who stands in the way of a melee attack on `o`: a person (a townsperson, a vendor, a player)
-- within a body's width of the straight line from the character to it, on its floor, or nil.
-- From beyond the server's own reach a melee attack is a charge straight at the monster, and a
-- body across the line stops the charge short: the server charges on into it, and no blow is
-- struck until the attack gives up. Lines of sight see through people.
local function in_the_way(o)
	local ax, ay, az = E.global_of(game.player.position)
	local bx, by = E.global_of(o.position)
	if ax == nil or bx == nil then
		return nil
	end
	local dx, dy = bx - ax, by - ay
	local len2 = dx * dx + dy * dy
	if len2 < 0.25 then
		return nil
	end
	for _, p in ipairs(game.world:find({ within = math.sqrt(len2) + BODY })) do
		if PEOPLE[p.class] and p.id ~= game.player.id and p.id ~= o.id then
			local x, y, z = E.global_of(p.position)
			if x ~= nil and math.abs(z - az) <= FLOOR then
				local t = ((x - ax) * dx + (y - ay) * dy) / len2
				if t > 0 and t < 1 and math.sqrt((x - (ax + t * dx)) ^ 2 + (y - (ay + t * dy)) ^ 2) <= BODY then
					return p
				end
			end
		end
	end
	return nil
end
E.in_the_way = in_the_way

-- Whether `o` is within `reach`, and with `sight` in a straight line of sight too, with nobody
-- standing in the way of a charge at it from beyond melee distance.
local function close_to(o, reach, sight)
	local d = o:distance()
	return d ~= nil and d <= reach
		and (not sight or (in_sight(o, true) and (d <= MELEE or in_the_way(o) == nil)))
end

-- Along the client's own walking route to `where` (an object or a place), round walls and through
-- doors, followed whole by its travel (a journey that only walks: no portal, no recall), until
-- `arrived()` says the character is there, `interrupt()` says something more pressing has come
-- up, or, for an object, it has moved off the route's end. Answers true once arrived; otherwise
-- false and why: `walking` when the journey ended short of it or it moved, `stopped` when
-- interrupted, else the journey's failure.
local function route_walk(where, arrived, interrupt)
	local gx, gy = nil, nil
	if type(where) == "table" and where.id ~= nil and where.position ~= nil then
		gx, gy = E.global_of(where.position)
	end
	local stopped = nil
	local p = nav.path_to(in_dungeon(where), { portals = false, recalls = false, show = false })
	local watch = task.every(250, function()
		if stopped ~= nil then
			return
		end
		if arrived() then
			stopped = "arrived"
		elseif interrupt ~= nil and interrupt() then
			stopped = "stopped"
		elseif gx ~= nil and where.position ~= nil then
			local x, y = E.global_of(where.position)
			if x ~= nil and math.sqrt((x - gx) ^ 2 + (y - gy) ^ 2) > REROUTE_METRES then
				stopped = "walking"
			end
		end
		if stopped ~= nil then
			nav.stop()
		end
	end)
	local ok, why = pawait(p)
	task.cancel(watch)
	if arrived() then
		return true
	end
	if stopped ~= nil and stopped ~= "arrived" then
		return false, stopped
	end
	if ok then
		return false, "walking"
	end
	return false, why
end
E.route_walk = route_walk

-- Whether walks may follow the client's journeys: it can plan them, NavJourneys is on and
-- NavStraightLegs (VTank's straight walks) is off.
local function journeys_walk()
	return not S.get("NavStraightLegs") and S.get("NavJourneys") and host.has("nav.path")
		and host.has("nav.journey")
end

-- Whether a straight walk to `where` (an object or a place) goes, as VTank walks: the client
-- cannot plan, NavStraightLegs asks for it or NavJourneys is off, or nothing stands in the way of
-- the straight line there.
local function straight_to(where)
	if not journeys_walk() then
		return true
	end
	local object = type(where) == "table" and where.id ~= nil
	return nav.sight(object and where or in_dungeon(where)) ~= false
end
E.straight_to = straight_to

-- One step toward `o`. Behind a wall (no straight line of sight), or with `sight` and someone in
-- the way of a charge at it, and no closed door on the way, along the client's whole route to it,
-- followed by its travel, until `o` is within `reach` (and in sight with nobody in the way, with
-- `sight`) or `interrupt()` says something more pressing has come up. Otherwise
-- (in sight, a door to open on the way, or no journeys) along the client's route to it a corner a
-- step: the doors the next leg passes opened, then on to the route's next corner (straight at `o`
-- past the last), stopping early as above. Answers true once `o` is within reach (and in sight);
-- otherwise false and why the step ended: `walking` when a corner was reached or the walk ended
-- short, else the walk's or the route's failure.
local function approach(o, reach, interrupt, sight)
	if close_to(o, reach, sight) then
		return true
	end
	if not host.has("nav.path") then
		return walk(o, reach, interrupt)
	end
	local r, why = route_to(o)
	if r == nil then
		return false, why
	end
	-- Round what stands in the way, with no closed door on the way (the Tank opens those itself,
	-- picking their locks): the whole route, followed by the client's travel, which goes round the
	-- people standing on it too.
	local person = sight and journeys_walk() and in_the_way(o) ~= nil
	if #r.doors == 0 and ((#r.points > 2 and not straight_to(o)) or person) then
		local ok, why2 = route_walk(o, function()
			return close_to(o, reach, sight)
		end, interrupt)
		if ok or why2 == "walking" or why2 == "stopped" then
			if not ok then
				routes[o.id] = nil
			end
			return ok, why2
		end
		-- No walking journey there: a corner a step, below.
		E.note("route to %s: %s; a corner at a time", tostring(o.name), tostring(why2))
	end
	local i = r.next
	for _, d in ipairs(r.doors) do
		-- A door walked to (to use a key on it) is the walk's end, not a door on the way.
		local door = d.at == i - 1 and d.object ~= o.id and game.world:get(d.object) or nil
		if door ~= nil and door.open == false and S.get("OpenDoors") then
			E.note("opening %s on the way to %s", tostring(door.name), tostring(o.name))
			if not open_door(door) then
				routes[o.id] = nil
				return false, "a locked door"
			end
		end
	end
	local stop = function()
		return close_to(o, reach, sight) or (interrupt ~= nil and interrupt())
	end
	local last = i >= #r.points
	-- The last leg, when `o` is to be in sight: on toward it until it is, as near as melee
	-- distance. Below the edge of a ramp the line to it is cut though the walk there is straight.
	local near = sight and math.min(reach, MELEE) or reach
	local ok, why2 = walk(last and o or r.points[i], last and near or CORNER, stop)
	if ok and not last then
		r.next = i + 1
		return false, "walking"
	end
	if close_to(o, reach, sight) then
		return true
	end
	if ok then
		-- At it, but no clear line yet: planned again from here.
		routes[o.id] = nil
		return false, "walking"
	end
	if why2 == "stuck" or why2 == "timeout" then
		routes[o.id] = nil
	end
	return false, why2
end
E.approach = approach

-- One fighting step on `target`: into combat, close in if needed, one attack.
-- The weapon wielded now (a melee or missile weapon, or a caster), or nil.
local function wielded_weapon()
	local me = game.player.id
	for _, it in ipairs(game.inventory.items()) do
		if it.wielder == me and (it.class == "MeleeWeapon" or it.class == "WandStaffOrb"
			or (it.class == "MissileWeapon" and not is_ammo(it))) then
			return it
		end
	end
	return nil
end
E.wielded_weapon = wielded_weapon

-- Take up the weapon named for a monster, if it is not already held. Answers false when it could
-- not be taken up.
local function take_up(name)
	if name == nil or name == "" then
		return true
	end
	local now = wielded_weapon()
	if now ~= nil and string.lower(now.name or "") == string.lower(name) then
		return true
	end
	local want = nil
	for _, it in ipairs(game.inventory.items()) do
		if string.lower(it.name or "") == string.lower(name) and it.wielder ~= game.player.id then
			want = it
		end
	end
	if want == nil then
		return false
	end
	-- Weapons are changed in peace mode, as a player changes them.
	if (game.player.combat_mode or 1) > 1 then
		combat.stop()
		pawait(combat.set_mode("peace"))
	end
	if now ~= nil then
		pawait(items.unwield(now.id))
	end
	local ok, why = pawait(items.wield(want.id))
	E.note(ok and "taking up %s" or "could not take up %s: %s", want.name, tostring(why))
	E.missile = ok and want.class == "MissileWeapon" or false
	-- A thrown weapon is its own ammunition.
	if E.missile and not is_grenade(want) then
		E.ready_ammo()
	end
	return ok
end
E.take_up = take_up

-- Spells are cast in magic stance. Into it for a spell: the caster (CasterWeapon, else the first
-- in the pack) taken up, and the weapon put away remembered so the next fight takes it up again.
-- With no caster to take up, spells wait a minute before it is tried again.
local magic_blocked_until = 0
function E.ready_magic()
	if os.clock() < magic_blocked_until then
		return false
	end
	local name = S.get("CasterWeapon")
	local ok, put = fight.ready_magic({ caster = name ~= "" and name or nil })
	if ok then
		if put ~= nil then
			E.put_away = put.name
		end
		return true
	end
	E.note("not in magic stance: %s", tostring(put))
	magic_blocked_until = os.clock() + 60
	return false
end
recharge.ready = E.ready_magic

-- The debuffs cast on each monster, by id: spell family -> { at, lasts, dot } when it landed
-- (`lasts`, its spell's duration, where the client knows it; `dot`, one that does its damage over
-- time), or { retry } after one that failed.
local debuffed = {}

-- The server's answer to a cast the character has too little mana for.
local function no_mana(why)
	return string.find(tostring(why), "0x0401", 1, true) ~= nil
end

-- Until when the caster waits for mana (after a cast it had too little for).
local mana_wait = 0

-- In a fight with mana under Recharge-Norm-Mana: VTank's list for mana in magic stance (a mana
-- kit, a mana food or potion, Stamina to Mana). Answers true when it used one.
local function restore_mana()
	if not recharge.low("mana", "Recharge-Norm-Mana") or os.clock() < mana_wait then
		return false
	end
	local ok, line = recharge.step("mana", "magic", false)
	if line == nil then
		return false
	end
	E.note("%s", line)
	if ok then
		E.tally.recharges = (E.tally.recharges or 0) + 1
	elseif no_mana(line) then
		mana_wait = os.clock() + 3
	end
	return ok
end

-- Whether a debuff landed as `on` wants casting again: it is not on, or it runs out within
-- DebuffPrecastSeconds (by its spell's duration; where the client does not know that, it is cast
-- again DebuffRecastSeconds after it landed). A damage-over-time spell runs its course first: it is
-- cast again once it has ended, as VTank's DebuffPrecastSeconds does not apply to those. One that
-- failed waits a little.
local function debuff_due(on)
	if on == nil then
		return true
	end
	local now = os.clock()
	if on.retry ~= nil then
		return now >= on.retry
	end
	if on.lasts ~= nil then
		return now >= on.at + on.lasts - (on.dot and 0 or S.get("DebuffPrecastSeconds"))
	end
	return now - on.at > S.get("DebuffRecastSeconds")
end

-- The damage types the character has an attack spell of, for the Random damage type.
local function known_types()
	local out = {}
	for _, t in ipairs(spells.AUTO_ORDER) do
		if spells.attack(t, S.get("MaxSpellLevel"), S.get("SpellDiffExcessThreshold-Hunt")) ~= nil then
			table.insert(out, t)
		end
	end
	return out
end

-- The damage types chosen at random, by monster id.
local random_types = {}

-- The damage a monster is fought with: its row's damage type or mode; for `random` (VTank's Random
-- damage type) a type chosen at random once a monster, of those the character has an attack spell
-- of; and for `auto` by a character with neither War Magic nor Void Magic trained, the drains (as
-- VTank chooses Drain Auto then), where it knows one.
local function damage_of(o, row)
	local d = row.damage or "auto"
	if d == "random" then
		local t = random_types[o.id]
		if t == nil then
			local known = known_types()
			t = #known > 0 and known[math.random(#known)] or "auto"
			random_types[o.id] = t
			E.logk("CastInfo", "%s: %s, at random", tostring(o.name), t)
		end
		return t
	end
	if d == "auto" and #(game.player.skills or {}) > 0 and not spells.trained("War Magic")
		and not spells.trained("Void Magic") then
		for _, f in pairs(spells.DRAINS) do
			if spells.best(f, S.get("MaxSpellLevel"), S.get("SpellDiffExcessThreshold-Hunt")) ~= nil then
				return "drain"
			end
		end
	end
	return d
end

-- VTank's /vt testpet: why no pet would be summoned now, or the device and element that would be,
-- and for which monster.
function E.test_pet()
	return pets.test(E.meta_state, damage_of)
end

-- The debuff families a monster's row asks for, in a fixed order: its debuffs, the vulnerability
-- of its damage type, and its extra vulnerability (VTank's Ex. Vuln: an element, or `auto` for the
-- vulnerability of its damage type).
local function wanted_debuffs(row, o)
	local wanted, seen = {}, {}
	local function add(f)
		if f ~= nil and not seen[f] then
			seen[f] = true
			table.insert(wanted, f)
		end
	end
	local damage = o ~= nil and damage_of(o, row) or row.damage
	if spells.MODES[damage] then
		damage = "auto"
	end
	local _, kind = spells.attack(damage, S.get("MaxSpellLevel"), S.get("SpellDiffExcessThreshold-Hunt"))
	for name, want in pairs(row.debuffs or {}) do
		if want then
			if name == "vuln" then
				add(spells.VULNS[kind or "fire"])
			elseif spells.DEBUFFS[name] ~= nil then
				add(spells.DEBUFFS[name])
			end
		end
	end
	local ex = row.exvuln
	if ex ~= nil and ex ~= "" and ex ~= "none" then
		add(spells.VULNS[ex == "auto" and (kind or "fire") or ex])
	end
	table.sort(wanted)
	return wanted
end

-- The first debuff `o`'s row wants that is due on it, as the spell to cast, or nil.
local function due_debuff(o, row)
	local on = debuffed[o.id] or {}
	for _, fam in ipairs(wanted_debuffs(row, o)) do
		local s = spells.best(fam, S.get("MaxSpellLevel"), S.get("SpellDiffExcessThreshold-Hunt"))
		-- A debuff a fellow of the Tank's own has put on it is not cast again (VI2's fellows do not
		-- double a vulnerability).
		if s ~= nil and debuff_due(on[fam]) and not fellows.peer_debuffed(o.id, fam) then
			return s, fam
		end
	end
	return nil
end

-- The debuffs the macro has on monsters it still sees, for its fellows: each { monster, family,
-- left } (seconds).
function E.debuff_list()
	local out = {}
	local now = os.clock()
	for id, on in pairs(debuffed) do
		local o = game.world:get(id)
		if o ~= nil then
			for fam, d in pairs(on) do
				if d.at ~= nil then
					local left = d.lasts ~= nil and (d.at + d.lasts - now) or (S.get("DebuffRecastSeconds") - (now - d.at))
					if left > 0 then
						table.insert(out, { monster = id, family = fam, left = math.floor(left) })
					end
				end
			end
		end
	end
	return out
end

-- One debuff on `o` that its row wants and it has not got (or will soon lose). Answers true when
-- it cast (or tried to).
local function debuff_step(o, row)
	local s, fam = due_debuff(o, row)
	if s == nil then
		return false
	end
	local on = debuffed[o.id] or {}
	debuffed[o.id] = on
	local ok, why = pawait(magic.cast(s.id, o.id))
	recharge.other_cast()
	cast_at(o, ok, why)
	if ok then
		local dot = spells.DOTS[fam] == true
		on[fam] = { at = os.clock(), lasts = dot and spells.dot_seconds(s.id) or spells.duration(s.id), dot = dot }
		E.tally.debuffs = (E.tally.debuffs or 0) + 1
		E.logk("DebuffChoice", "%s on %s", s.name, o.name)
	elseif no_mana(why) then
		mana_wait = os.clock() + 3
	else
		E.logk("CastInfo", "%s on %s: %s", s.name, o.name, tostring(why))
		-- Not retried at once: a resisted or fizzled debuff waits a little.
		on[fam] = { retry = os.clock() + 5 }
	end
	return true
end

-- With DebuffEachFirst 2 (Priority) or 3 (All): another monster in reach that its row wants
-- debuffed and is not yet, of the target's priority or of any; nearest first. Answers it and its
-- row, or nil.
local function debuff_first(target, row)
	local method = S.get("DebuffEachFirst")
	if method ~= 2 and method ~= 3 then
		return nil
	end
	local priority = row.priority or 1
	for _, o in ipairs(game.world:find({ monster = true, within = attack_distance(), sort = "distance" })) do
		if o.id ~= target.id and not is_ignored(o.id) then
			local r = monsters.match(o, E.meta_state)
			local p = r.priority or 1
			if p >= 0 and (method == 3 or p == priority) and due_debuff(o, r) ~= nil then
				return o, r
			end
		end
	end
	return nil
end

-- One cast of the debuffs a monster's row asks for: a debuff another monster is to have first
-- (DebuffEachFirst), else the first debuff not yet on it. Answers true when it cast (or tried to).
local function debuff_cast(target, row)
	local other, other_row = debuff_first(target, row)
	if other ~= nil and debuff_step(other, other_row) then
		return true
	end
	return debuff_step(target, row)
end

-- Whether a ring is to be cast now (VTank's R column): for a row that attacks too, when
-- MinimumRingTargets monsters or more are within RingDistance; for one that does not, when any
-- monster of a row marked for rings is.
local function ring_wanted(row)
	if not row.ring then
		return false
	end
	local n, marked = 0, 0
	for _, o in ipairs(game.world:find({ monster = true, within = S.metres("RingDistance") })) do
		if not is_ignored(o.id) and not ghosts[o.id] then
			local r = monsters.match(o, E.meta_state)
			if (r.priority or 1) >= 0 then
				n = n + 1
				if r.ring then
					marked = marked + 1
				end
			end
		end
	end
	if row.attack ~= false then
		return n >= S.get("MinimumRingTargets")
	end
	return marked > 0
end

-- The bearing from the character to `o`, degrees clockwise from north.
local function bearing_to(o)
	local ax, ay = E.global_of(game.player.position)
	local bx, by = E.global_of(o.position)
	if ax == nil or bx == nil then
		return game.player.heading or 0
	end
	return math.deg(math.atan2(bx - ax, by - ay)) % 360
end

-- VTank's DoJiggle ("spell dodge mode"): after a cast or a shot at a monster out of melee reach, a
-- short side-step, left and right in turn, so the bolts aimed where the character stood miss.
local JIGGLE_MS = 300
local jiggle_side = "strafe_right"
local function jiggle(target)
	if not S.get("DoJiggle") or not E.can("nav") then
		return
	end
	local d = target:distance()
	if d ~= nil and d <= MELEE + 1 then
		return
	end
	jiggle_side = jiggle_side == "strafe_left" and "strafe_right" or "strafe_left"
	pawait(nav.motion(jiggle_side, JIGGLE_MS))
end

-- The drain to cast (VTank's Drain Auto): the vital the character is short of taken from the
-- monster (mana under its top-off mark, Recharge-NoTarg-Mana, else stamina under
-- Recharge-NoTarg-Stam), else the monster's health.
local function drain_spell(max, excess)
	local order = { "health" }
	local m, st = share("mana"), share("stamina")
	if m ~= nil and m < S.share("Recharge-NoTarg-Mana") then
		table.insert(order, 1, "mana")
	elseif st ~= nil and st < S.share("Recharge-NoTarg-Stam") then
		table.insert(order, 1, "stamina")
	end
	for _, v in ipairs(order) do
		local s = spells.best(spells.DRAINS[v], max, excess)
		if s ~= nil then
			return s
		end
	end
	return nil
end

-- The attack spell for monster `o` of `row`, fought with `damage` (a damage type or a mode):
-- the drains, the harm spells, Tusker Fists (answered with "fists": cast untargeted, aimed by
-- facing), or the type's streak (the row's S column), arc (UseArcs, at ArcRange or more) or bolt.
local function attack_spell(o, row, damage)
	local max, excess = S.get("MaxSpellLevel"), S.get("SpellDiffExcessThreshold-Hunt")
	if damage == "drain" then
		return drain_spell(max, excess)
	elseif damage == "harm" then
		for _, f in ipairs(spells.HARMS) do
			local s = spells.best(f, max, excess)
			if s ~= nil then
				return s
			end
		end
		return nil
	elseif damage == "fists" then
		return spells.best(spells.FISTS, max, excess), "fists"
	end
	local kind = nil
	if row.streak then
		kind = "streak"
	elseif S.get("UseArcs") and (o:distance() or 0) >= S.metres("ArcRange") then
		kind = "arc"
	end
	return (spells.attack(damage, max, excess, kind))
end

-- How far off its heading the character turns to cast at a monster before a breakable turn looks
-- again, degrees.
local TURN_BREAK = 20

-- One cast of the spells a monster's row asks for: its debuffs (and those DebuffEachFirst wants
-- on other monsters first), a ring when the crowd for one is about, else the row's attack spell.
-- With BreakableTurn, a cast at a monster well off to the side turns to it first and looks again:
-- another monster to be fought first is fought instead. Answers true when it cast (or tried to).
local function cast_step(target, row)
	if restore_mana() then
		return true
	end
	-- Too little mana for the last cast: wait a moment for it rather than casting into nothing.
	if os.clock() < mana_wait then
		task.sleep(500)
		return true
	end
	if debuff_cast(target, row) then
		return true
	end
	local damage = damage_of(target, row)
	local max, excess = S.get("MaxSpellLevel"), S.get("SpellDiffExcessThreshold-Hunt")
	if ring_wanted(row) then
		local ring = spells.ring(spells.MODES[damage] and "auto" or damage, max, excess)
		if ring ~= nil then
			E.tally.swings = E.tally.swings + 1
			local ok, why = pawait(magic.cast(ring.id))
			recharge.other_cast()
			if ok then
				E.logk("SpellCast", "ring: %s", ring.name)
				jiggle(target)
			elseif no_mana(why) then
				mana_wait = os.clock() + 3
			else
				E.logk("CastInfo", "ring: %s: %s", ring.name, tostring(why))
				task.sleep(500)
			end
			return true
		end
	end
	-- Debuff only, unless the row casts streaks or rings (a ring row with none in reach of a
	-- ring is fought with the attack spell).
	if row.attack == false and not row.streak and not row.ring then
		return false
	end
	local s, how = attack_spell(target, row, damage)
	if s == nil then
		E.note("no attack spell known for %s", tostring(row.damage))
		return false
	end
	if S.get("BreakableTurn") and turn_to(target) > TURN_BREAK then
		pawait(nav.face(bearing_to(target)))
		local again = E.choose_target(attack_distance())
		if again ~= nil and again.id ~= target.id then
			E.note("turned to %s; %s comes first", tostring(target.name), tostring(again.name))
			E.target = nil
			return true
		end
	end
	if how == "fists" then
		-- The fists go down a straight path ahead: aimed by facing the monster.
		pawait(nav.face(bearing_to(target)))
	end
	E.tally.swings = E.tally.swings + 1
	local ok, why = pawait(magic.cast(s.id, how ~= "fists" and target.id or nil))
	recharge.other_cast()
	cast_at(target, ok, why)
	if not ok and no_mana(why) then
		-- Not the monster's doing: it is not given up for this.
		mana_wait = os.clock() + 3
	elseif not ok and why ~= "stopped" then
		E.logk("CastInfo", "%s on %s: %s", s.name, target.name, tostring(why))
		if E.target ~= nil then
			E.target.failed = (E.target.failed or 0) + 1
			-- A cast gets a try more than a swing: a fizzle is not the monster's doing.
			if E.target.failed >= S.get("BlacklistMonsterAttemptCount") + 1 then
				blacklist(target.id, target.name, "spells keep failing")
			end
		end
		task.sleep(500)
	elseif ok then
		E.logk("SpellCast", "%s at %s", s.name, target.name)
		if E.target ~= nil then
			E.target.failed = 0
		end
		jiggle(target)
	end
	return true
end

-- The game's properties a weapon's appraisal tells: its damage type and its imbued effects.
local DAMAGE_TYPE, IMBUED = 45, 179
-- The damage type bits, and the rending imbue of each type.
local DAMAGE_BITS = { slash = 0x1, pierce = 0x2, blunt = 0x4, cold = 0x8, fire = 0x10, acid = 0x20, lightning = 0x40, nether = 0x400 }
local REND_BITS = { slash = 0x8, pierce = 0x10, blunt = 0x20, acid = 0x40, cold = 0x80, lightning = 0x100, fire = 0x200, nether = 0x4000 }
-- The imbues that pay on a monster already made vulnerable: critical strike, crippling blow and
-- armor rending.
local STRIKES = 0x1 + 0x2 + 0x4
-- What each weapon strikes with, by id: { damage, imbued }; looked up once.
local weapon_facts = {}
local function facts_of(w)
	local f = weapon_facts[w.id]
	if f == nil then
		local dt, im = w:int(DAMAGE_TYPE), w:int(IMBUED)
		if dt == nil and im == nil then
			local ok, a = pawait(act.appraise(w.id))
			local ints = ok and a and a.ints or {}
			dt, im = ints[DAMAGE_TYPE], ints[IMBUED]
		end
		f = { damage = dt or 0, imbued = im or 0 }
		weapon_facts[w.id] = f
	end
	return f
end

-- Whether monster `o` has a vulnerability on it that the macro cast and that has not run out.
local function vulned(o)
	local on = o ~= nil and debuffed[o.id] or nil
	if on == nil then
		return false
	end
	for _, fam in pairs(spells.VULNS) do
		local v = on[fam]
		if v ~= nil and v.at ~= nil and (v.lasts == nil or os.clock() < v.at + v.lasts) then
			return true
		end
	end
	return false
end

-- Whether `o` is made vulnerable, by the macro (a vulnerability or Imperil) or by a fellow of its own.
function E.vulned_by_any(o)
	if vulned(o) or fellows.peer_vulned(o.id) then
		return true
	end
	local imp = debuffed[o.id] and debuffed[o.id][spells.DEBUFFS.imperil]
	return imp ~= nil and imp.at ~= nil and (imp.lasts == nil or os.clock() < imp.at + imp.lasts)
end

-- VTank's automatic weapon choice, for a row whose weapon is `auto`: of the weapons the Weapons
-- setting names (else every melee and missile weapon in the pack), the one that strikes with the
-- row's damage type; of those, a rending weapon of that type first, but on a monster made
-- vulnerable already, a critical-strike, crippling-blow or armor-rending one first.
function E.choose_weapon(o, row)
	local named = S.list("Weapons")
	local best, best_score = nil, -1
	local vul = vulned(o)
	for _, it in ipairs(game.inventory.items()) do
		if (#named == 0 or S.listed("Weapons", it.name))
			and (it.class == "MeleeWeapon" or (it.class == "MissileWeapon" and not is_ammo(it) and not is_grenade(it))) then
			local f = facts_of(it)
			local t = row.damage
			local score = 0
			if DAMAGE_BITS[t] ~= nil and bit32.band(f.damage, DAMAGE_BITS[t]) ~= 0 then
				score = score + 2
			end
			if REND_BITS[t] ~= nil and bit32.band(f.imbued, REND_BITS[t]) ~= 0 then
				score = score + (vul and 1 or 3)
			end
			if bit32.band(f.imbued, STRIKES) ~= 0 then
				score = score + (vul and 4 or 1)
			end
			if score > best_score then
				best, best_score = it, score
			end
		end
	end
	return best
end

-- The weapon a monster's row fights with: the row's (by name, or `auto`: the one VTank's
-- automatic choice picks), else the Weapon setting, else the weapon put away for a spell.
local function weapon_for(target, row)
	local name = row.weapon
	if name ~= nil and string.lower(name) == "auto" then
		local w = E.choose_weapon(target, row)
		name = w and w.name or ""
	end
	if name == nil or name == "" then
		name = S.get("Weapon") ~= "" and S.get("Weapon") or E.put_away
	end
	return name
end

-- Whether the pack holds a caster of that name.
local function caster_named(name)
	if name == nil or name == "" then
		return false
	end
	for _, it in ipairs(game.inventory.items()) do
		if it.class == "WandStaffOrb" and string.lower(it.name or "") == string.lower(name) then
			return true
		end
	end
	return false
end

-- The shields and offhand weapons found missing, so each is said once.
local offhand_missing = {}

-- The shield or offhand weapon of a melee fight (the row's Offhand, else the Offhand setting)
-- taken up beside the weapon, when it is not already.
local function ready_offhand(row)
	local name = row.offhand
	if name == nil or name == "" then
		name = S.get("Offhand")
	end
	if name == nil or name == "" then
		return
	end
	local me, item = game.player.id, nil
	for _, it in ipairs(game.inventory.items()) do
		if string.lower(it.name or "") == string.lower(name) then
			if it.wielder == me then
				return
			end
			item = item or it
		end
	end
	if item == nil then
		if not offhand_missing[name] then
			offhand_missing[name] = true
			E.note("no %s in the pack to take up beside the weapon", name)
		end
		return
	end
	local ok, why = pawait(items.wield(item.id))
	E.note(ok and "taking up %s beside the weapon" or "could not take up %s: %s", item.name, tostring(why))
end

-- The grenade to throw for `row`: one the Consumables list names, of the row's damage type where
-- one is (by the element its name gives), else any.
local GRENADE_WORDS = {
	fire = { "fire", "flame" }, cold = { "frost" }, acid = { "acid" }, lightning = { "lightning" },
	blunt = { "shock" }, pierce = { "force" }, slash = { "blade" },
}
local function grenade_for(row)
	local any = nil
	for _, it in ipairs(game.inventory.items()) do
		if is_grenade(it) then
			local n = string.lower(it.name or "")
			for _, word in ipairs(GRENADE_WORDS[row.damage] or {}) do
				if string.find(n, word, 1, true) then
					return it
				end
			end
			any = any or it
		end
	end
	return any
end

-- Throw `grenade` at `target`: taken up in place of the weapon (the shield or offhand weapon kept),
-- the weapon remembered to be taken up again, then a missile attack.
local function throw_grenade(grenade, target)
	local now = wielded_weapon()
	if now ~= nil and now.id ~= grenade.id and not is_grenade(now) and E.put_away == nil then
		E.put_away = now.name
	end
	E.note("throwing %s at %s", grenade.name, target.name)
	if not take_up(grenade.name) then
		return
	end
	if (game.player.combat_mode or 1) <= 1 then
		local ok, why = pawait(combat.set_mode("combat"))
		if not ok then
			E.note("could not enter combat mode: %s", tostring(why))
			return
		end
	end
	E.tally.swings = E.tally.swings + 1
	E.tally.grenades = (E.tally.grenades or 0) + 1
	local ok, why = pawait(combat.attack(target, { power = S.share("AttackPower") }))
	if not ok and why ~= "stopped" then
		E.note("throw at %s: %s", target.name, tostring(why))
	end
end

-- The weapon's attacks (an appraisal's attack type), by its id; looked up once.
local ATTACK_TYPE = 47
local attack_types = {}
local function attacks_of(weapon)
	local t = attack_types[weapon.id]
	if t == nil then
		t = weapon:int(ATTACK_TYPE)
		if t == nil then
			local ok, a = pawait(act.appraise(weapon.id))
			t = ok and a and a.ints and a.ints[ATTACK_TYPE] or 0
		end
		attack_types[weapon.id] = t
	end
	return t
end

-- The power bar's mark below which a weapon of both thrusts and slashes thrusts.
local THRUST_BELOW = 0.33
-- The attack type's thrusts and slashes (single, double and triple strikes).
local THRUSTS = 0x2 + 0x80 + 0x100
local SLASHES = 0x4 + 0x20 + 0x40
-- The band of the power bar Recklessness works in.
local RECKLESS_LOW, RECKLESS_HIGH = 0.2, 0.8

-- The power of a swing at a monster of `row`: AttackPower, or TiredAttackPower when stamina is
-- under Recharge-Norm-Stam. With AutoAttackPower, a melee weapon that both thrusts and slashes is
-- swung on the side of the thrust mark that strikes with the row's damage type (pierce or slash);
-- with UseRecklessness and Recklessness trained, the power stays in Recklessness's band.
function E.swing_power(weapon, row)
	local stamina = share("stamina") or 1
	local power = stamina > S.share("Recharge-Norm-Stam") and S.share("AttackPower") or S.share("TiredAttackPower")
	if S.get("AutoAttackPower") and weapon ~= nil and weapon.class == "MeleeWeapon" then
		local t = attacks_of(weapon)
		if bit32.band(t, THRUSTS) ~= 0 and bit32.band(t, SLASHES) ~= 0 then
			if row.damage == "pierce" then
				power = math.min(power, THRUST_BELOW - 0.01)
			elseif row.damage == "slash" then
				power = math.max(power, THRUST_BELOW)
			end
		end
	end
	if S.get("UseRecklessness") then
		for _, sk in ipairs(game.player.skills or {}) do
			if sk.name == "Recklessness" and sk.training ~= "untrained" then
				power = math.min(math.max(power, RECKLESS_LOW), RECKLESS_HIGH)
			end
		end
	end
	return power
end

local function fight_step(target)
	set_state("Fighting")
	if E.target == nil or E.target.id ~= target.id then
		E.target = { id = target.id, name = target.name, since = os.clock() }
		E.note("target %s (%.1f m)", target.name, target:distance() or -1)
	end
	local row = monsters.match(target, E.meta_state)
	attacked[target.id] = { name = target.name, at = os.clock(), place = target.position }
	if os.clock() - E.target.since > S.get("GiveUpSeconds") then
		combat.stop()
		blacklist(target.id, target.name, string.format("not killed in %d s", S.get("GiveUpSeconds")))
		return
	end
	-- The health tracker: a target whose health has not changed in GhostDeleteHPTrackerSeconds
	-- of fighting it is a ghost.
	if S.get("DeleteGhostMonstersByHPTracker") then
		local h = target:health()
		local f = h and h.fraction
		if E.target.hp_at == nil or f ~= E.target.hp then
			E.target.hp, E.target.hp_at = f, os.clock()
		elseif os.clock() - E.target.hp_at > S.get("GhostDeleteHPTrackerSeconds") then
			combat.stop()
			ghost(target.id, target.name,
				string.format("its health has not changed in %d s of fighting", S.get("GhostDeleteHPTrackerSeconds")))
			return
		end
	end
	local d = target:distance() or 0
	-- Within the attack distance, and in a straight line of sight, the attack is made at once: a
	-- melee attack's last steps are the server's (it moves the character into reach). Further
	-- off, in approach distance, or with a wall between (what a bolt or an arrow would hit), the
	-- character walks to the monster first, along the client's route round walls and through
	-- doors, a corner a step. One no route reaches is left alone a while. A wall or a ledge
	-- counts within melee distance too: the server's charge at a monster a step away across one
	-- goes the long way round and gives up ("You charged too far!"). Walked right up to it once
	-- and still out of sight, the monster is attacked: the charge decides. Someone standing across
	-- the line beyond melee distance (a townsperson the charge would run into) is walked round
	-- first, as a wall is.
	local reach = math.max(S.metres("AttackDistance"), MELEE)
	local close = d <= MELEE + 1
	if d > reach or (not in_sight(target) and not (close and E.target.walked_up))
		or (d > MELEE and in_the_way(target) ~= nil) then
		if close then
			E.target.walked_up = true
		end
		local ok, why = approach(target, math.max(reach - 1, MELEE), nil, true)
		-- Interrupted (the server moved the character, say) is tried again at the next step.
		if not ok and why ~= "walking" and why ~= "stopped" and why ~= "target_gone" and why ~= "interrupted" then
			if why == "stuck" then
				unstick()
			end
			blacklist(target.id, target.name, "cannot reach it: " .. tostring(why))
		end
		return
	end
	local weapon_name = weapon_for(target, row)
	-- VTank's SwitchWandsToDebuff: a monster the row fights with a weapon is debuffed with a
	-- caster first (CasterWeapon, else the first in the pack); the weapon is taken up again after.
	local held = wielded_weapon()
	if S.get("SwitchWandsToDebuff") and (held == nil or held.class ~= "WandStaffOrb") and not caster_named(weapon_name)
		and (due_debuff(target, row) ~= nil or debuff_first(target, row) ~= nil) then
		if E.ready_magic() then
			debuff_cast(target, row)
			return
		end
	end
	if weapon_name ~= nil and weapon_name ~= "" and not take_up(weapon_name) then
		E.note("no %s to fight %s with", weapon_name, target.name)
	end
	local weapon = wielded_weapon()
	if weapon ~= nil and weapon.class == "MeleeWeapon" then
		ready_offhand(row)
	end
	if game.player.combat_mode == nil or game.player.combat_mode <= 1 then
		local ok, why = pawait(combat.set_mode("combat"))
		if not ok then
			E.note("could not enter combat mode: %s", tostring(why))
			task.sleep(1000)
			return
		end
	end
	if weapon ~= nil and weapon.class == "WandStaffOrb" then
		cast_step(target, row)
		return
	end
	-- A row marked for rings, fought with a weapon: a grenade from the Consumables list, thrown
	-- when the crowd a ring would be cast for is about.
	if ring_wanted(row) then
		local g = grenade_for(row)
		if g ~= nil then
			throw_grenade(g, target)
			return
		end
	end
	if row.attack == false then
		-- Debuff only, and no caster held: nothing to do to it with this weapon.
		blacklist(target.id, target.name, "set to debuff only, and no caster is held")
		return
	end
	local power = E.swing_power(weapon, row)
	local heights = { "high", "medium", "low" }
	E.tally.swings = E.tally.swings + 1
	local ok, why = pawait(combat.attack(target, { power = power, height = heights[S.get("DefaultMeleeAttackHeight")] }))
	if ok and type(why) == "table" and why.ended == "cancelled" and E.target ~= nil then
		-- An attack cancelled at once, twice running: the monster has fallen (its body not yet
		-- gone) or cannot be fought from here.
		E.target.cancelled = (E.target.cancelled or 0) + 1
		if E.target.cancelled >= 2 then
			ignored[target.id] = os.clock() + 20
			dead_ids[target.id] = true
			E.target = nil
		end
		return
	end
	if not ok and why ~= "stopped" then
		E.note("attack on %s: %s", target.name, tostring(why))
		-- The monster fell meanwhile (its kill is counted, the target let go): nothing to keep.
		if E.target == nil then
			return
		end
		-- The client sends an attack only from the ready position: a character still moving (a
		-- leg of the route just stopped, a turn) has it dropped. It stops and settles first; only
		-- a run of these leaves the monster alone.
		if why == "refused by the client" then
			nav.stop()
			E.target.unready = (E.target.unready or 0) + 1
			if E.target.unready >= 8 then
				blacklist(target.id, target.name, "the client will not attack it")
			end
			task.sleep(700)
			return
		end
		-- A monster the attacks keep failing on (out of reach, behind a wall) is left alone a while.
		E.target.failed = (E.target.failed or 0) + 1
		-- An attack that times out never reached the monster (the server could not walk to it):
		-- it is out of reach from here. Other failures get BlacklistMonsterAttemptCount tries.
		if why == "timeout" or E.target.failed >= S.get("BlacklistMonsterAttemptCount") then
			blacklist(target.id, target.name, why == "timeout" and "out of reach" or "attacks keep failing")
		end
		task.sleep(500)
	elseif ok then
		if E.target ~= nil then
			E.target.failed = 0
		end
		if E.missile then
			jiggle(target)
		end
	end
end

-- VTank's /vt equipitemsfor: one step toward wielding what the monster list says to fight
-- `name` with (the row of that name, else the default row; a row that is an expression is not
-- looked at): to peace mode first, then the weapon. Answers what it did.
function E.equip_step_for(name)
	local row = monsters.find(name) or monsters.rows()[1]
	local want = row and row.weapon or ""
	if want ~= nil and string.lower(want) == "auto" then
		local w = E.choose_weapon(nil, row)
		want = w and w.name or ""
	end
	if want == nil or want == "" then
		want = S.get("Weapon")
	end
	if want == nil or want == "" then
		return "no weapon is named for " .. name
	end
	local now = wielded_weapon()
	if now ~= nil and string.lower(now.name or "") == string.lower(want) then
		return want .. " is wielded"
	end
	if (game.player.combat_mode or 1) > 1 then
		combat.stop()
		local ok, why = pawait(combat.set_mode("peace"))
		return ok and "to peace mode" or ("to peace mode: " .. tostring(why))
	end
	local item = nil
	for _, it in ipairs(game.inventory.items()) do
		if string.lower(it.name or "") == string.lower(want) and it.wielder ~= game.player.id then
			item = it
		end
	end
	if item == nil then
		return "no " .. want .. " in the pack"
	end
	local ok, why = pawait(items.wield(item.id))
	return ok and ("wielding " .. item.name) or ("could not wield " .. item.name .. ": " .. tostring(why))
end

-- Kill bookkeeping: a monster that was being fought and has gone is a kill once its corpse
-- appears. Called on object removal.
function E.object_removed(id)
	ghosts[id] = nil
	local a = attacked[id]
	if a ~= nil then
		attacked[id] = nil
		table.insert(pending_kills, { name = a.name, at = os.clock(), place = a.place })
		if E.target ~= nil and E.target.id == id then
			E.target = nil
		end
	end
end

-- Corpses of the macro's kills, waiting to be looted: id -> { name, at, opens, next_try, items },
-- `opens` the tries at opening it, `items` the tries at taking each of its items, by id.
local corpses = {}
-- Corpses done with: id -> true (looted, or left), or the clock time until which one that would
-- not open is left alone.
local looted = {}

function E.object_created(id)
	local o = game.world:get(id)
	if o == nil or o.class ~= "Corpse" then
		return
	end
	local name = o.name or ""
	local function kill(what)
		E.tally.kills = E.tally.kills + 1
		E.note("killed %s (%d so far)", what, E.tally.kills)
		corpses[id] = { name = name, at = os.clock(), opens = 0, items = {} }
	end
	for i, k in ipairs(pending_kills) do
		if name == "Corpse of " .. k.name and os.clock() - k.at < 15 then
			table.remove(pending_kills, i)
			kill(k.name)
			return
		end
	end
	-- The corpse can come before the monster leaves.
	for mid, a in pairs(attacked) do
		if name == "Corpse of " .. a.name and os.clock() - a.at < 20 then
			attacked[mid] = nil
			-- Dead, though its body has not left yet: not a target any more.
			ignored[mid] = os.clock() + 3600
			dead_ids[mid] = true
			if E.target ~= nil and E.target.id == mid then
				E.target = nil
			end
			kill(a.name)
			return
		end
	end
end

-- Whether a corpse is done with, or left alone for now.
local function corpse_done(id)
	local l = looted[id]
	return l == true or (type(l) == "number" and os.clock() < l)
end

-- Other corpses (not of the macro's own kills) and what each one's appraisal tells: who killed it
-- ("Killed by …" in its description) and whether it generated a rare. Asked for once a while, by id.
local LONG_DESC = 16
local corpse_asked = {}

-- A corpse's killer and whether it generated a rare, from its appraisal; nil before one.
local function corpse_facts(o)
	if not o.appraised then
		return nil
	end
	local d = o:str(LONG_DESC) or ""
	return { killer = string.match(d, "Killed by (.-)%."), rare = string.find(d, "generated a rare", 1, true) ~= nil }
end
E.corpse_facts = corpse_facts

-- A character's name as a comparison takes it: without case, and without the plus sign the game
-- puts before an administrator's name.
local function plain(name)
	return string.lower((string.gsub(tostring(name or ""), "^%+", "")))
end
E.plain_name = plain

-- Whether a name is the character's own, or a fellow's.
local function is_me(name)
	return plain(name) == plain(game.player.name)
end
local function is_fellow(name)
	local f = game.player.fellowship
	for _, m in ipairs(f and f.members or {}) do
		if plain(m.name) == plain(name) and not is_me(m.name) then
			return true
		end
	end
	return false
end

-- Whether corpse `o` is one to loot (`own`: of one of the macro's kills): with
-- LootOnlyRareCorpses only one that generated a rare; another's by who killed it, the
-- character's own, a fellow's with LootFellowCorpses, anyone else's with LootAllCorpses (VTank's
-- options, as its advanced options describe them).
local function corpse_wanted(o, own)
	local f = corpse_facts(o)
	if S.get("LootOnlyRareCorpses") and not (f ~= nil and f.rare) then
		return false
	end
	if own then
		return true
	end
	if f == nil or f.killer == nil then
		return false
	end
	if is_me(f.killer) then
		return true
	elseif is_fellow(f.killer) then
		return S.get("LootFellowCorpses") == true
	end
	return S.get("LootAllCorpses") == true
end

-- The corpses in reach whose appraisals the choice needs (others' with LootAllCorpses or
-- LootFellowCorpses; every one with LootOnlyRareCorpses) asked for, each once every ten seconds
-- at most. Called from the loop, as it can wait.
local function ask_corpses()
	local others = S.get("LootAllCorpses") or S.get("LootFellowCorpses")
	if not others and not S.get("LootOnlyRareCorpses") then
		return
	end
	for _, o in ipairs(game.world:find({ class = "Corpse", within = S.metres("CorpseApproachRange-Max") })) do
		local own = corpses[o.id] ~= nil
		if not o.appraised and looted[o.id] == nil and (own or others)
			and (corpse_asked[o.id] == nil or os.clock() - corpse_asked[o.id] > 10) then
			corpse_asked[o.id] = os.clock()
			act.appraise(o.id)
		end
	end
end

-- The corpse to loot next, within CorpseApproachRange-Max: of those near enough to open where the
-- character stands, the one it turns least to face; else the nearest (VTank's corpse choice).
-- A kill's corpse is remembered CorpseCacheTimeoutMinutes; another's corpse is taken in as the
-- loot options say.
local function next_corpse()
	local keep = S.get("CorpseCacheTimeoutMinutes") * 60
	local reach = S.metres("CorpseApproachRange-Max")
	if S.get("LootAllCorpses") or S.get("LootFellowCorpses") then
		for _, o in ipairs(game.world:find({ class = "Corpse", within = reach })) do
			if corpses[o.id] == nil and looted[o.id] == nil and corpse_wanted(o, false) then
				corpses[o.id] = { name = o.name or "", at = os.clock(), opens = 0, items = {}, other = true }
				E.logk("Loot", "%s: %s", o.name or "a corpse", "another's kill, to loot")
			end
		end
	end
	local near, near_turn, far, far_d = nil, nil, nil, nil
	for id, c in pairs(corpses) do
		local o = game.world:get(id)
		if o == nil or looted[id] == true or os.clock() - c.at > keep then
			corpses[id] = nil
		elseif not corpse_done(id) and (c.next_try == nil or os.clock() >= c.next_try) and corpse_wanted(o, true) then
			local d = o:distance()
			if d ~= nil and d <= reach then
				if d <= NEAR_CORPSE then
					local t = turn_to(o)
					if near == nil or t < near_turn then
						near, near_turn = o, t
					end
				elseif far == nil or d < far_d then
					far, far_d = o, d
				end
			end
		end
	end
	return near or far
end

local function pack_full()
	local free = game.inventory.free_slots
	return free ~= nil and free < S.get("MinFreeSlots")
end

-- Whether a rule the profile matched takes the item from the corpse. Every rule's action does, as
-- this plugin reads VTank's loot actions (VTank's own handling of Sell, Read and User rules is not
-- published): Keep and Keep # rules keep it; a Salvage rule takes it to be salvaged; a Sell rule
-- takes it to sell at a vendor; a Read rule takes it to read; a User rule takes it for the player.
local TAKES = { keep = true, keep_up_to = true, salvage = true, sell = true, read = true, user1 = true,
	user2 = true, user3 = true, user4 = true, user5 = true }
function E.takes(hit)
	return hit ~= nil and TAKES[hit.action] == true
end

-- How many items the pack holds that the profile's rule `rule` matches (stacks counted whole).
local function held_by_rule(rule)
	local n = 0
	for _, it in ipairs(game.inventory.items()) do
		if it.wielder == nil then
			local hit = E.profile:classify(it)
			if hit ~= nil and hit.name == rule then
				n = n + (it.stack or 1)
			end
		end
	end
	return n
end

-- A scroll's spell: the spell its properties name, else the first its appraisal lists.
local SPELL = 28
local function scroll_spell(item)
	local id = item:did(SPELL)
	if id == nil or id == 0 then
		local a = item:appraisal()
		id = a and a.spells and a.spells[1] or nil
	end
	return id
end

-- Whether the character can learn a scroll: it does not know the spell, and its buffed skill in
-- the spell's school is no more than 15 under the spell's difficulty (VTank's rule).
local function learnable(item)
	if item.class ~= "Scroll" then
		return false
	end
	local id = scroll_spell(item)
	local sp = id and magic.spell(id)
	if sp == nil or sp.known or sp.difficulty == nil then
		return false
	end
	local skill = spells.skill(sp.school)
	return skill ~= nil and skill >= sp.difficulty - 15, sp
end

-- Take a scroll the character can learn and read it.
local function read_scroll(item)
	local _, sp = learnable(item)
	local took, why = pawait(items.move(item.id))
	if not took then
		E.logk("Loot", "could not take %s to read it: %s", item.name, tostring(why))
		return
	end
	local ok, why2 = pawait(act.use(item.id))
	E.tally.scrolls = (E.tally.scrolls or 0) + 1
	E.logk("Loot", ok and "read %s (%s)" or "could not read %s (%s): %s", item.name, sp and sp.name or "?", tostring(why2))
end

local take_from
local far_step

-- Loot one corpse by the profile: walk to it, open it, take what the profile keeps (a keep-up-to
-- rule's items until the pack holds its count), then read the unknown scrolls the character can
-- learn (ReadUnknownScrolls) that the profile does not keep. A corpse that does not open is tried
-- again after CorpseOpenTimeoutSeconds, BlacklistCorpseOpenAttemptCount times; an item that will
-- not come is tried again, CorpseLootItemMaxAttempts times.
local function loot_step(corpse)
	set_state("Looting")
	local c = corpses[corpse.id]
	local ok, why = approach(corpse, math.max(S.metres("CorpseApproachRange-Min"), 1), monster_close)
	if not ok then
		if why ~= "stopped" and why ~= "walking" then
			E.logk("Loot", "cannot reach %s: %s", corpse.name or "the corpse", tostring(why))
			if why == "stuck" then
				unstick()
			end
			looted[corpse.id] = true
		end
		return
	end
	if game.inventory.open_container ~= corpse.id then
		ok, why = pawait(act.use(corpse.id))
		local opened = ok and wait_for(function()
			return game.inventory.open_container == corpse.id
		end, 2)
		if not opened then
			c.opens = c.opens + 1
			if c.opens >= S.get("BlacklistCorpseOpenAttemptCount") then
				looted[corpse.id] = os.clock() + S.get("BlacklistCorpseOpenTimeoutSeconds")
				E.logk("Loot", "%s would not open (%d tries): left for %d s", corpse.name or "the corpse", c.opens,
					S.get("BlacklistCorpseOpenTimeoutSeconds"))
			else
				c.next_try = os.clock() + S.get("CorpseOpenTimeoutSeconds")
				E.logk("Loot", "%s did not open (%s): tried again in %d s", corpse.name or "the corpse",
					ok and "no contents" or tostring(why), S.get("CorpseOpenTimeoutSeconds"))
			end
			return
		end
	end
	if not c.counted then
		c.counted = true
		E.tally.looted = E.tally.looted + 1
	end
	if take_from(corpse, c) then
		c.next_try = os.clock() + 1
	else
		looted[corpse.id] = true
	end
end

-- Take from open container `corpse` what the profile's rules take (`c` its record: the tries at
-- each item), then read the unknown scrolls the character can learn and what Read rules took.
-- Answers whether an item would not come and is to be tried again.
function take_from(corpse, c)
	-- The contents arrive with the opening: waited for until they stop coming (a moment with no
	-- more), CorpseItemAppearanceTimeoutSeconds at most.
	local count, steady = -1, os.clock()
	wait_for(function()
		local n = #game.world:find({ container = corpse.id })
		if n ~= count then
			count, steady = n, os.clock()
		end
		return os.clock() - steady >= 0.6
	end, S.get("CorpseItemAppearanceTimeoutSeconds"))
	local contents = game.world:find({ container = corpse.id })
	-- Their appraisals, all asked for at once and waited for together, CorpseItemIDTimeoutSeconds
	-- at most.
	local asked = {}
	for _, item in ipairs(contents) do
		if not item.appraised and not c.everything and E.profile ~= nil
			and (E.profile:needs_appraisal(item) or (item.class == "Scroll" and S.get("ReadUnknownScrolls"))) then
			act.appraise(item.id)
			table.insert(asked, item)
		end
	end
	if #asked > 0 then
		wait_for(function()
			for _, item in ipairs(asked) do
				if not item.appraised then
					return false
				end
			end
			return true
		end, S.get("CorpseItemIDTimeoutSeconds"))
	end
	local kept, again, to_read, to_use = 0, false, {}, {}
	-- The corpse of a character of the player's own name is never salvaged from (VTank's guard
	-- [log 0.3.1.37]).
	local own_body = corpse.name == "Corpse of " .. tostring(game.player.name)
	for _, item in ipairs(contents) do
		-- The character's own corpse gives back everything.
		local hit = c.everything and { keep = true, action = "keep", name = "the character's own" }
			or (E.profile ~= nil and E.profile:classify(item) or nil)
		if not E.takes(hit) and recharge.wants_stone(item) then
			hit = { keep = true, action = "keep", name = "mana stone" }
		end
		-- A keep-up-to rule keeps its items until the pack holds its count.
		if hit ~= nil and hit.action == "keep_up_to" and hit.count ~= nil then
			local held = held_by_rule(hit.name)
			if held >= hit.count then
				E.logk("RuleInfo", "left %s: %d held of %s's %d", item.name, held, hit.name, hit.count)
				hit = nil
			end
		end
		local take = E.takes(hit)
		if not take and S.get("ReadUnknownScrolls") and learnable(item) then
			table.insert(to_read, item)
		elseif not take then
			local line = recharge.drain_into_stone(item)
			if line ~= nil then
				E.note("%s", line)
			end
		end
		if take then
			if pack_full() then
				E.logk("Loot", "no room for %s", item.name)
			else
				local took, why2 = pawait(items.move(item.id))
				if took then
					kept = kept + 1
					E.tally.kept = E.tally.kept + 1
					E.logk("Loot", "%s %s (%s)", hit.action == "salvage" and "took to salvage" or hit.action == "sell"
						and "took to sell" or hit.action == "read" and "took to read" or "kept", item.name, hit.name)
					if hit.action == "salvage" and not own_body then
						-- Its appraisal says whether it was tinkered or inscribed, which are kept.
						if not item.appraised then
							pawait(act.appraise(item.id))
						end
						if pack.salvageable(item) then
							pack.take(item)
						else
							E.logk("SalvageList", "%s is tinkered or inscribed: kept, not salvaged", item.name)
						end
					elseif hit.action == "read" then
						table.insert(to_use, item)
					end
					if E.on_loot then
						E.on_loot(item.name, hit.name)
					end
				else
					local tries = (c.items[item.id] or 0) + 1
					c.items[item.id] = tries
					if tries < S.get("CorpseLootItemMaxAttempts") then
						again = true
						E.logk("Loot", "could not take %s (try %d): %s", item.name, tries, tostring(why2))
					else
						E.logk("Loot", "left %s after %d tries: %s", item.name, tries, tostring(why2))
					end
				end
			end
		end
	end
	E.logk("Loot", "looted %s: kept %d of %d", corpse.name or "corpse", kept, #contents)
	-- The scrolls once the corpse is done, as VTank reads them; and what Read rules took, read.
	for _, item in ipairs(to_read) do
		read_scroll(item)
	end
	for _, item in ipairs(to_use) do
		local ok, why3 = pawait(act.use(item.id))
		E.logk("Loot", ok and "read %s" or "could not read %s: %s", item.name, tostring(why3))
	end
	return again
end

-- VTank's "Loot this Container": the container open now (a chest, a corpse the player opened)
-- taken from by the loot profile, as a corpse the macro loots is. Answers whether it was looted, and
-- why not.
function E.loot_container()
	local id = game.inventory.open_container
	local o = id and game.world:get(id)
	if o == nil then
		return false, "no container is open"
	end
	if E.profile == nil then
		return false, "no loot profile is loaded"
	end
	take_from(o, { items = {} })
	return true
end

-- The stance VTank's recharge lists are chosen by.
local function stance()
	return game.player.combat_mode == 8 and "magic" or "other"
end

local function can_heal()
	local h = share("health")
	return recharge.can("health", stance(), h ~= nil and h < 0.15)
end

-- Heal up to `to` (a share) by VTank's list for health: a few tries at most, so a failed heal does
-- not empty the kit.
local function heal_step(to)
	set_state("Healing")
	-- A kit is not used while a swing is under way.
	if combat.fighting() then
		combat.stop()
		task.sleep(300)
	end
	local tries = 0
	while (share("health") or 1) < to and tries < 3 and E.running do
		tries = tries + 1
		local before = share("health") or 0
		local ok, line = recharge.step("health", stance(), before < 0.15)
		if line == nil then
			break
		end
		if ok then
			E.tally.heals = E.tally.heals + 1
			E.note("healed %d%% -> %d%% (%s)", math.floor(before * 100), math.floor((share("health") or 0) * 100), line)
		else
			E.note("heal failed: %s", line)
			task.sleep(1000)
		end
	end
	E.last_heal = os.clock()
end

-- Stamina or mana under the marks the two settings name: one use of VTank's list for it. Answers
-- true when it tried something.
local function recharge_step(mark_stam, mark_mana)
	for _, v in ipairs({ { "mana", mark_mana }, { "stamina", mark_stam } }) do
		if recharge.low(v[1], v[2]) and recharge.can(v[1], stance(), false) then
			set_state("Recharging")
			local ok, line = recharge.step(v[1], stance(), false)
			E.note("%s", line)
			if ok then
				E.tally.recharges = (E.tally.recharges or 0) + 1
			else
				task.sleep(1000)
			end
			return true
		end
	end
	return false
end

-- Worn items' mana, looked at every half minute with nothing to fight.
local worn_checked = nil
local function refill_step()
	if worn_checked ~= nil and os.clock() - worn_checked < 30 then
		return false
	end
	worn_checked = os.clock()
	local ok, line = recharge.refill_worn()
	if line ~= nil then
		E.note("%s", line)
	end
	return ok
end

-- With no monster near: stamina or mana under the top-off marks, to be waited back up.
local function needs_rest()
	if not S.get("RestWhenIdle") then
		return false
	end
	local s, m = share("stamina"), share("mana")
	return (s ~= nil and s < S.share("Recharge-NoTarg-Stam")) or (m ~= nil and m < S.share("Recharge-NoTarg-Mana"))
end

-- Rest in peace mode until stamina and mana are back, or a monster comes.
local function rest_step()
	set_state("Resting")
	E.tally.rests = E.tally.rests + 1
	E.note("resting at %s", E.vitals_text())
	nav.stop()
	if (game.player.combat_mode or 1) > 1 and E.can("combat") then
		pawait(combat.set_mode("peace"))
	end
	local started = os.clock()
	while E.running and not monster_near() and os.clock() - started < 120 do
		local s, m = share("stamina") or 1, share("mana") or 1
		if s >= S.share("Recharge-NoTarg-Stam") and m >= S.share("Recharge-NoTarg-Mana") then
			break
		end
		task.sleep(1000)
	end
	E.note("rested to %s", E.vitals_text())
end

-- One leg of the route: on to the current waypoint, stopping for anything more pressing.
local function interrupt_nav()
	if not E.running then
		return true
	end
	if monster_near() then
		return true
	end
	if S.get("EnableLooting") and E.profile ~= nil and not pack_full() and next_corpse() ~= nil then
		return true
	end
	local h = share("health")
	return h ~= nil and h < S.share("Recharge-Norm-HitP") and can_heal()
end

-- The object a waypoint names: by its id when the client still knows it, else the nearest one of
-- its name (or of `class` when it has no name) within `within` metres of the character.
local function waypoint_object(p, class, within)
	if p.id ~= nil and p.id ~= 0 then
		local o = game.world:get(p.id)
		if o ~= nil then
			return o
		end
	end
	local filter = { within = within, sort = "distance" }
	if p.name ~= nil and p.name ~= "" then
		filter.name = p.name
	else
		filter.class = class
	end
	return game.world:first(filter)
end

-- Whether the character is in portal space (between a teleport's start and its arrival).
local function teleporting()
	local info = session.info()
	return info ~= nil and info.portal_space == true
end

-- How many times an NPC Talk waypoint's NPC is used, waiting a while each time for its tell.
local NPC_TALK_TRIES, NPC_TALK_WAIT = 12, 5
-- How long a checkpoint waits for the server to place the character on it, and how long between
-- steps forward while it does not.
local CHECKPOINT_WAIT, CHECKPOINT_INCH = 20, 3

-- A checkpoint (VTank's): the character stands at the waypoint until the server tells it is
-- there, its own word on where the character stands (`Movement_PositionEvent`, which runs a
-- little behind the client's); while the server places it elsewhere, it steps forward now and
-- then. On a host without the message stream it waits a moment. Answers whether the server put
-- it there.
local function checkpoint(r, p)
	if not host.has("messages") then
		task.sleep(1000)
		return false
	end
	local wx, wy = E.global_of(p.position)
	local reach = math.max(S.metres("NavCloseStopRange"), 2)
	local there = false
	local function heard(m)
		local f = m.fields
		if m.dir ~= "received" or f == nil or f.id ~= game.player.id or f.position == nil then
			return
		end
		local o = f.position.origin
		local x, y = E.global_of({ cell = o and o.objcell_id, x = o and o.origin and o.origin.x,
			y = o and o.origin and o.origin.y })
		if x ~= nil and wx ~= nil and math.sqrt((x - wx) ^ 2 + (y - wy) ^ 2) <= reach then
			there = true
		end
	end
	messages.on("Movement_PositionEvent", heard)
	local started, steps = os.clock(), 0
	while E.running and not there and os.clock() - started < CHECKPOINT_WAIT do
		if os.clock() - started >= CHECKPOINT_INCH * (steps + 1) then
			steps = steps + 1
			pawait(nav.motion("forward", 250, { walk = true }))
		end
		task.sleep(200)
	end
	messages.off("Movement_PositionEvent", heard)
	E.note(there and "route: the server has the character at checkpoint %d" or "route: checkpoint %d not confirmed in %d s; going on",
		r.at, CHECKPOINT_WAIT)
	return there
end

-- An NPC Talk waypoint (VTank's): the NPC is used, again and again, until it tells the character
-- something (NPC_TALK_TRIES uses at most, then the route goes on, said).
local function npc_talk(r, npc)
	local name = npc.name or ""
	local told = false
	-- A tell names its teller as a link (`<Tell:…>Name<\Tell> tells you`), or plainly.
	local function heard(e)
		local text = e.text or ""
		if e.kind == "tell" and (string.find(text, name .. "<\\Tell> tells you", 1, true) ~= nil
			or string.sub(text, 1, #name + 10) == name .. " tells you") then
			told = true
		end
	end
	on("chat.line", heard)
	local tries = 0
	while E.running and not told and tries < NPC_TALK_TRIES do
		tries = tries + 1
		local ok, why = pawait(act.use(npc.id))
		if not ok then
			E.note("route: %s: %s", name, tostring(why))
		end
		wait_for(function()
			return told
		end, NPC_TALK_WAIT)
	end
	off("chat.line", heard)
	E.note(told and "route: %s told the character something (%d uses)" or "route: %s told nothing in %d uses; going on",
		name, tries)
end

-- What a waypoint asks once the character has reached it: a chat line typed, a jump, a portal
-- used and passed through (or an NPC of that name used), an NPC used until it speaks, a vendor
-- opened, a checkpoint confirmed. Answers whether the waypoint is done (false: try it again next
-- step).
local portal_tries = 0
local function at_waypoint(r, p)
	if p.kind == "chat" and p.text then
		-- A VTank command is answered as the meta's are; anything else is typed.
		if not (E.command ~= nil and E.command(p.text)) then
			chat.command(p.text)
		end
	elseif p.kind == "jump" then
		local ok, why = pawait(nav.jump({ heading = p.heading, walk = p.shift == true, ms = math.min(p.ms or 0, 1000), direction = p.direction }))
		if ok then
			E.tally.jumps = (E.tally.jumps or 0) + 1
			E.note("route: jumped at waypoint %d (heading %d, %d ms%s)", r.at, math.floor(p.heading or 0), math.floor(p.ms or 0), p.shift and ", walking" or "")
			-- The leap carries the character; a moment to land before the next leg.
			task.sleep(800)
		else
			E.note("route: jump at waypoint %d: %s", r.at, tostring(why))
		end
	elseif p.kind == "portal" then
		local portal = waypoint_object(p, "Portal", 15)
		if portal == nil then
			E.note("route: no portal %s near waypoint %d; going on", tostring(p.name or p.id), r.at)
			return true
		end
		-- VTank's Use Portal/NPC: what it names is an NPC (or anything but a portal), used once.
		if portal.class ~= "Portal" then
			local ok, why = pawait(act.use(portal.id))
			E.note(ok and "route: used %s" or "route: %s: %s", tostring(portal.name), tostring(why))
			task.sleep(1500)
			return true
		end
		portal_tries = portal_tries + 1
		E.note("route: using %s", tostring(portal.name))
		local ok, why = pawait(act.use(portal.id))
		if ok and wait_for(teleporting, 8) then
			wait_for(function()
				return not teleporting()
			end, 60)
			E.tally.portals = (E.tally.portals or 0) + 1
			portal_tries = 0
			E.note("route: through %s", tostring(portal.name))
			return true
		end
		if portal_tries >= 3 then
			E.note("route: %s did not take the character (%s); going on", tostring(portal.name), tostring(why or "no teleport"))
			portal_tries = 0
			return true
		end
		task.sleep(1000)
		return false
	elseif p.kind == "use_npc" then
		local npc = waypoint_object(p, "Npc", 15)
		if npc == nil then
			E.note("route: no %s near waypoint %d; going on", tostring(p.name), r.at)
		else
			npc_talk(r, npc)
		end
	elseif p.kind == "checkpoint" then
		checkpoint(r, p)
	elseif p.kind == "vendor" then
		local shop = waypoint_object(p, "Vendor", 15)
		if shop == nil then
			E.note("route: no vendor %s near waypoint %d; going on", tostring(p.name), r.at)
		else
			local ok, why = pawait(act.use(shop.id))
			if ok and wait_for(function()
				return vendor.info() ~= nil
			end, 5) then
				E.note("route: %s's wares are open", tostring(shop.name))
				vendors.from_waypoint(p)
				if (S.get("VendorSell") or S.get("VendorBuy")) and E.can("vendor") then
					-- The macro trades by profile, then closes the wares.
					vendors.trade(E, E.note)
					pawait(vendor.close())
				else
					-- Another plugin may trade; the route goes on once the window closes, or
					-- after a minute.
					wait_for(function()
						return vendor.info() == nil
					end, 60)
				end
			else
				E.note("route: %s did not open: %s", tostring(shop.name), tostring(why))
			end
		end
	end
	return true
end

-- Following a character: a follow route (VTank's, which follows another player; its MiniRemote's
-- FC follows the selected one). The follower closes to within NavCloseStopRange (VTank's
-- "Follow/Nav Min"; 2 m at least) of its leader and holds there, between its fights, loots and
-- heals: straight at the leader, or with FollowAroundCorners along the leader's own way, the
-- places it passed. A leader the client does not see is walked to where its own Tank on this
-- machine last said it was (a fellow on the machine's bus, see fellows.lua).

-- Where a fellow of the Tank's own on this machine last said it was.
E.heard_place = fellows.heard_place

-- The places the leader passed, oldest first, as global metres { x, y, place }: what the follower
-- walks around corners.
local crumbs = {}
-- How far apart the places kept are, metres, and the most kept.
local CRUMB, MAX_CRUMBS = 1.5, 400

function E.forget_crumbs()
	crumbs = {}
end

-- The leader a follow route names: by its object id while the client knows it (and it bears the
-- name), else the nearest of its name.
function E.follow_target(r)
	local o = r.target_id ~= nil and r.target_id ~= 0 and game.world:get(r.target_id) or nil
	if o ~= nil and r.target ~= nil and r.target ~= "" and not E.same_name(o.name, r.target) then
		o = nil
	end
	if o == nil and r.target ~= nil and r.target ~= "" then
		o = E.character_named(r.target)
	end
	if o ~= nil and o.id == game.player.id then
		return nil
	end
	return o
end

-- Whether two names are one character's.
function E.same_name(a, b)
	return E.plain_name(a) == E.plain_name(b)
end

-- The nearest character (or other object) of that name, or nil.
function E.character_named(name)
	local plain = E.plain_name(name)
	if plain == "" then
		return nil
	end
	for _, o in ipairs(game.world:find({ name_contains = plain, sort = "distance" })) do
		if E.plain_name(o.name) == plain and o.id ~= game.player.id then
			return o
		end
	end
	return nil
end

-- Where the leader is, a few times a second, kept as its way while the follower follows it round
-- corners.
task.every(250, function()
	local r = E.route
	if not E.running or r == nil or r.kind ~= "follow" or not S.get("FollowAroundCorners") then
		return
	end
	local o = E.follow_target(r)
	local x, y = nil, nil
	if o ~= nil then
		x, y = E.global_of(o.position)
	end
	if x == nil then
		return
	end
	local last = crumbs[#crumbs]
	if last == nil or math.sqrt((x - last.x) ^ 2 + (y - last.y) ^ 2) >= CRUMB then
		table.insert(crumbs, { x = x, y = y, place = o.position })
		if #crumbs > MAX_CRUMBS then
			table.remove(crumbs, 1)
		end
	end
end)

-- One step of following: to the leader, or along its way, stopping for anything more pressing.
local function follow_step(r)
	set_state("Following")
	local reach = math.max(S.metres("NavCloseStopRange"), 2)
	local o = E.follow_target(r)
	if o == nil then
		-- Out of the client's sight: where its own Tank last said it was.
		local heard = E.heard_place ~= nil and E.heard_place(r.target) or nil
		local d = heard and E.metres_to(heard)
		if d ~= nil and d > reach + 3 then
			if d > 150 and travel(heard, "after " .. tostring(r.target)) then
				return
			end
			walk(heard, reach, interrupt_nav)
			return
		end
		task.sleep(500)
		return
	end
	local d = o:distance()
	if d == nil or d <= reach then
		-- Caught up: the way behind is walked no more.
		crumbs = {}
		task.sleep(250)
		return
	end
	if S.get("FollowAroundCorners") then
		-- The places reached are dropped; the next is walked to, the leader's own way.
		local mx, my = E.global_of(game.player.position)
		while crumbs[1] ~= nil and mx ~= nil and math.sqrt((crumbs[1].x - mx) ^ 2 + (crumbs[1].y - my) ^ 2) <= CRUMB do
			table.remove(crumbs, 1)
		end
		local next_place = crumbs[1]
		if next_place ~= nil then
			local ok, why = walk(next_place.place, CRUMB / 2, interrupt_nav)
			if not ok and why ~= "stopped" then
				-- A place the follower cannot reach is passed over.
				table.remove(crumbs, 1)
				E.logk("Timers", "following %s: %s", tostring(o.name), tostring(why))
			end
			return
		end
	end
	local ok, why = walk(o, reach, interrupt_nav)
	if not ok and why ~= "stopped" then
		E.logk("Timers", "following %s: %s", tostring(o.name), tostring(why))
		task.sleep(500)
	end
end

local stuck_at, stuck_count = nil, 0
local function nav_step()
	local r = E.route
	if r.kind == "follow" then
		follow_step(r)
		return
	end
	set_state("Navigating")
	if r.at == nil then
		r.at = E.nearest_point() or 1
		E.note("route: starting at waypoint %d of %d%s", r.at, #r.points, r.reversed and ", reversed" or "")
	end
	-- Run too far off its route, the macro turns navigation off (VTank's NavFarStopRange).
	local far = E.route_distance()
	if far ~= nil and far > S.metres("NavFarStopRange") then
		S.set("EnableNav", false, false)
		E.note("route: %d m from the route, further than NavFarStopRange: navigation off", math.floor(far))
		return
	end
	local at = r.at
	local p = r.points[at]
	local done = false
	-- The route set again (a meta entering its state anew) or started over (a death, a vendor
	-- trip's end) while this step waited: the next step takes it up from its nearest waypoint.
	local function started_over()
		return E.route ~= r or r.at ~= at
	end
	if p.kind == "pause" then
		task.sleep(p.ms or 0)
		done = true
	elseif p.kind == "recall" then
		E.note("route: recalling (spell %d)", p.spell or 0)
		pawait(magic.cast(p.spell))
		done = true
	elseif p.position ~= nil then
		open_door_across(p.position)
		local reach = math.max(S.metres("NavCloseStopRange"), 2)
		local ok, why
		if straight_to(p.position) then
			-- In a straight line of sight (VTank's leg): walked straight at it.
			ok, why = walk(p.position, reach, interrupt_nav)
		else
			-- Round a corner or behind a wall: along the client's route there from the start,
			-- round the corner and through the doors on the way.
			if r.planned_note ~= at then
				r.planned_note = at
				E.note("route: waypoint %d is out of sight; walking the client's route to it", at)
			end
			ok, why = route_walk(p.position, function()
				local d = E.metres_to(p.position)
				return d ~= nil and d <= reach
			end, interrupt_nav)
			if started_over() then
				return
			end
			if not ok and why == "walking" then
				-- Short of it: what is left is walked straight, as a leg is.
				ok, why = walk(p.position, reach, interrupt_nav)
			elseif not ok and why ~= "stopped" then
				E.note("route: no walking route to waypoint %d (%s); straight at it", at, tostring(why))
				ok, why = walk(p.position, reach, interrupt_nav)
			end
		end
		if started_over() then
			return
		end
		if ok then
			stuck_at, stuck_count = nil, 0
			done = at_waypoint(r, p)
		elseif why == "stuck" or why == "timeout" then
			if stuck_at ~= at then
				stuck_at, stuck_count = at, 0
			end
			stuck_count = stuck_count + 1
			if stuck_count > S.get("StuckTries") then
				E.note("route: skipping waypoint %d (stuck %d times)", at, stuck_count - 1)
				E.tally.skipped = E.tally.skipped + 1
				stuck_at, stuck_count = nil, 0
				done = true
			else
				-- A journey finds a way round what stopped the straight walk; backing off is the
				-- fallback.
				E.tally.stuck = E.tally.stuck + 1
				local found = travel(p.position, string.format("round to waypoint %d", at))
				if found then
					done = true
					stuck_at, stuck_count = nil, 0
				else
					unstick()
				end
			end
		elseif why ~= "stopped" then
			E.note("route: waypoint %d: %s", at, tostring(why))
			task.sleep(1000)
		end
	else
		done = true
	end
	if started_over() then
		return
	end
	if done then
		E.tally.waypoints = E.tally.waypoints + 1
		-- The way the route is run: from first to last, or reversed (VTank's reverse route, which
		-- runs the points the other way without changing their order).
		local step = r.step or (r.reversed and -1 or 1)
		local nexti = r.at + step
		if nexti > #r.points or nexti < 1 then
			if r.kind == "circular" then
				nexti = step > 0 and 1 or #r.points
			elseif r.kind == "linear" then
				step = -step
				nexti = math.max(1, math.min(#r.points, r.at + step))
			else
				E.note("route: finished")
				E.route_done = true
				return
			end
		end
		r.step = step
		r.at = nexti
	end
end

---------------------------------------------------------------------------------------------------
-- The vendor trip: to the vendor, trading there by profile, and back (see vendor.lua).

-- The trip under way: { stage = "going" | "trading" | "returning", vendor, back, tries, full }:
-- `back` the place it left, `full` when a full pack sent it.
E.trip = nil

-- How near the vendor the character walks before using it, metres; how far it looks for the
-- vendor, metres; and how near it is to be walked to and used (the server's own approach walking
-- the character the last of the way), metres.
local VENDOR_REACH, VENDOR_SEEN, VENDOR_USE = 3, 60, 30

-- The vendor's object, when the client knows it (by its id, else the nearest of its name).
local function vendor_object(v)
	return waypoint_object({ id = v.id, name = v.name }, "Vendor", VENDOR_SEEN)
end

-- Metres from the character to `place`, or nil.
local function metres_to(place)
	local ax, ay = E.global_of(game.player.position)
	local bx, by = E.global_of(place)
	if ax == nil or bx == nil then
		return nil
	end
	return math.sqrt((bx - ax) ^ 2 + (by - ay) ^ 2)
end
E.metres_to = metres_to

-- Start the vendor trip, `why` for the log: to the vendor the macro was given, else to the
-- current route's first vendor waypoint's. Answers the line to tell.
function E.vendor_trip(why)
	if E.trip ~= nil then
		return "the vendor trip is under way"
	end
	local v = vendors.vendor()
	if v == nil and E.route ~= nil then
		for _, p in ipairs(E.route.points or {}) do
			if p.kind == "vendor" then
				v = vendors.from_waypoint(p)
				break
			end
		end
	end
	if v == nil then
		return "no vendor: select one and set it on the Vendor page (/tank vendor set)"
	end
	local p = game.player.position
	E.trip = { stage = "going", vendor = v, tries = 0,
		back = p and { cell = p.cell, x = p.x, y = p.y, z = p.z } or nil }
	E.tally.trips = (E.tally.trips or 0) + 1
	local seen = vendor_object(v)
	local d = seen and seen:distance() or (v.place and metres_to(v.place))
	E.note("vendor trip to %s (%s)%s", v.name, why, d and d < 5000 and string.format(", %d m away", math.floor(d)) or "")
	return "going to " .. v.name
end

-- The trip ends; the route carries on from its nearest waypoint.
local function end_trip(why)
	local t = E.trip
	E.trip = nil
	if t ~= nil and t.full then
		E.full_trip_done = true
	end
	if E.route ~= nil then
		E.route.at = nil
	end
	E.note("vendor trip: %s", why)
	if E.trip_only then
		E.stop("the vendor trip is done")
	end
end

-- To `place` by a journey, else (without journeys) on foot. Answers ok, why.
local function go_to(place, why)
	local ok, r = travel(place, why)
	if ok or r ~= "no journeys" then
		return ok, r
	end
	return walk(place, VENDOR_REACH)
end

-- One step of the trip: on the way, at the vendor, or on the way back.
local function trip_step()
	local t = E.trip
	set_state("Vendor trip")
	if t.stage == "going" then
		local shop = vendor_object(t.vendor)
		local d = shop and shop:distance()
		-- At the place kept for it (where the player stood when it was set), the vendor is walked
		-- to from there, however far it stands from it.
		local stand_d = t.vendor.stand and metres_to(t.vendor.stand)
		local at_stand = stand_d ~= nil and stand_d <= 10
		if d ~= nil and (d <= VENDOR_USE or at_stand) then
			-- Near: walked to as far as a walking route goes, then used, the server walking the
			-- character the rest of the way (to a vendor behind its counter, say).
			local ok, why = approach(shop, VENDOR_REACH)
			if why == "stuck" then
				unstick()
			end
			if ok or (why ~= "walking" and why ~= "stopped") then
				t.stage, t.tries = "trading", 0
			end
			return
		end
		-- Further off: a journey to where the player stood when the vendor was set (a place a body
		-- stands, near it), else to where the vendor stands.
		local where = t.vendor.stand or t.vendor.place
		if where == nil then
			end_trip(string.format("%s is not in sight and its place is not known", t.vendor.name))
			return
		end
		local ok, why = go_to(where, "to " .. t.vendor.name)
		if not ok or vendor_object(t.vendor) == nil then
			t.tries = t.tries + 1
			if t.tries >= 3 then
				end_trip(string.format("could not get to %s (%s)", t.vendor.name, ok and "not where it was" or tostring(why)))
			end
		end
	elseif t.stage == "trading" then
		local shop = vendor_object(t.vendor)
		if shop == nil then
			t.stage = "going"
			return
		end
		if not E.can("vendor") then
			end_trip("this world does not let the macro trade at a vendor")
			return
		end
		if vendor.info() == nil then
			local ok, why = pawait(act.use(shop.id))
			if not (ok and wait_for(function()
				return vendor.info() ~= nil
			end, 5)) then
				t.tries = t.tries + 1
				if t.tries >= 3 then
					end_trip(string.format("%s's wares did not open (%s)", t.vendor.name, tostring(why)))
				end
				return
			end
		end
		vendors.trade(E, E.note)
		pawait(vendor.close())
		-- In town, a buffbot trip under way (the vendor trip came up on its way) goes on from here,
		-- and buffs from the buffbot that would run low before the next trip are renewed now (under
		-- twice BuffbotMinutes left): the buffbot trip goes back in the vendor trip's place.
		if E.btrip ~= nil and not E.trip_only then
			end_trip("on to the buffbot")
			return
		end
		local why = not E.trip_only and bb.due(S.get("BuffbotMinutes") * 60) or nil
		if why ~= nil then
			end_trip("on to the buffbot")
			bb.start(why, t.back)
			return
		end
		t.stage, t.tries = "returning", 0
	else
		local d = t.back and metres_to(t.back)
		if t.back == nil or (d ~= nil and d <= 10) then
			end_trip("back from " .. t.vendor.name)
			return
		end
		local ok, why = go_to(t.back, "back from " .. t.vendor.name)
		if ok then
			end_trip("back from " .. t.vendor.name)
		else
			t.tries = t.tries + 1
			if t.tries >= 3 then
				end_trip(string.format("could not get back (%s)", tostring(why)))
			end
		end
	end
end

---------------------------------------------------------------------------------------------------
-- The loop

-- What the plugin bar shows for the Tank: running, paused (stopped by a meta, or a domain it may
-- not automate) or idle.
local shown = nil
function E.show_status()
	local s = "idle"
	if E.running then
		s = "running"
		if E.paused or next(E.paused_domains) ~= nil then
			s = "paused"
		end
	end
	if s ~= shown and ui.status ~= nil then
		shown = s
		ui.status(s)
	end
end

function E.stop(why)
	if not E.running then
		return
	end
	E.running = false
	E.trip, E.trip_only = nil, false
	E.btrip, E.btrip_only = nil, false
	E.show_status()
	E.why_stopped = why
	if E.on_stop then
		E.on_stop()
	end
	set_state("Stopped")
	combat.stop()
	nav.stop()
	E.note("stopped: %s", why)
	E.note("tally: %s", E.tally_text())
	E.save_log()
	-- The weapon put away for spells is taken up again, so the character is left as it was found.
	local weapon = E.put_away
	E.put_away = nil
	if weapon ~= nil and why ~= "left the world" then
		task.spawn(function()
			local ok, err = fight.take_up(weapon)
			E.note(ok and "took up %s again" or "could not take up %s again: %s", weapon, tostring(err))
			if ok and E.missile then
				E.ready_ammo()
			end
		end)
	end
end

-- Pause the run (VTank's Run Macro turned off): the macro does nothing until it is resumed, and
-- keeps what the run knows (its tally, the route's place, the monsters left alone, the corpses to
-- loot, a force buff under way), as VTank's switch works as a pause.
function E.pause(why)
	if not E.running or E.paused then
		return
	end
	E.paused = true
	combat.stop()
	nav.stop()
	set_state("Paused")
	E.note("paused: %s", why)
	E.show_status()
end

-- Resume a paused run where it left off; answers false when there was none to resume.
function E.resume(why)
	if not E.running or not E.paused then
		return false
	end
	E.paused = false
	E.note("resumed: %s", why)
	E.show_status()
	if E.on_resume then
		E.on_resume()
	end
	return true
end

function E.tally_text()
	local t = E.tally
	return string.format(
		"%d kills, %d deaths, %d corpses recovered, %d corpses looted, %d items kept, %d stuck recoveries, %d waypoints skipped, %d waypoints, %d heals, %d recharges, %d rests, %d given up, %d attacks, %d debuffs, %d buffs; %d vendor trips, %d sold; %d buffbot trips, %d buffs from it; %d pyreals; %s; %d free slots; %.1f min",
		t.kills, t.deaths, t.recovered or 0, t.looted, t.kept, t.stuck, t.skipped, t.waypoints, t.heals, t.recharges or 0, t.rests,
		t.given_up, t.swings, t.debuffs or 0, t.buffs or 0, t.trips or 0, t.sold or 0, t.buff_trips or 0, t.buffbot_buffs or 0,
		vendors.coins(), E.vitals_text(), game.inventory.free_slots or -1, (os.time() - (t.started_at or os.time())) / 60)
end

-- What to do when the pack is full, or the run has died too often: stop, or recall then stop.
local function give_in(why)
	local spell = S.get("RecallSpell")
	if spell ~= nil and spell > 0 then
		set_state("Recalling")
		E.note("%s: recalling (spell %d)", why, spell)
		combat.stop()
		nav.stop()
		pawait(combat.set_mode("peace"))
		local ok, err = pawait(magic.cast(spell))
		if not ok then
			E.note("recall failed: %s", tostring(err))
		end
	end
	E.stop(why)
end

-- A place in metres from the world's corner, from a position (cell and offset) or map
-- coordinates. Inside one landblock this holds indoors too, which is how routes name dungeon
-- places.
local function global_of(p)
	if p == nil then
		return nil
	end
	if p.cell ~= nil and p.x ~= nil then
		local bx = bit32.rshift(p.cell, 24)
		local by = bit32.band(bit32.rshift(p.cell, 16), 0xFF)
		return bx * 192 + p.x, by * 192 + p.y, p.z or 0
	end
	if p.ns ~= nil and p.ew ~= nil then
		return (p.ew + 101.9) * 240, (p.ns + 101.9) * 240, p.z or 0
	end
	return nil
end
E.global_of = global_of

-- Metres from the character to the route's nearest waypoint.
local function route_distance()
	local mx, my = global_of(game.player.position)
	if mx == nil or E.route == nil then
		return nil
	end
	local best = nil
	for _, p in ipairs(E.route.points) do
		local x, y = global_of(p.position)
		if x ~= nil then
			local d = math.sqrt((x - mx) ^ 2 + (y - my) ^ 2)
			if best == nil or d < best then
				best = d
			end
		end
	end
	return best
end
E.route_distance = route_distance

-- The index of the route's waypoint nearest the character (indoors too).
function E.nearest_point()
	local mx, my, mz = global_of(game.player.position)
	if mx == nil or E.route == nil then
		return nil
	end
	local best, best_d = nil, nil
	for i, p in ipairs(E.route.points) do
		local x, y, z = global_of(p.position)
		if x ~= nil then
			local d = (x - mx) ^ 2 + (y - my) ^ 2 + (z - mz) ^ 2
			if best_d == nil or d < best_d then
				best, best_d = i, d
			end
		end
	end
	return best
end

local function in_portal_space()
	local info = session.info()
	return info ~= nil and info.portal_space == true
end

-- Back to the route from afar (further than JourneyToRouteMetres, which a walk does not go): by a
-- journey to the hunting place when there is one (`/tank hunt`; a dungeon's entrance, say, which
-- the route's map coordinates cannot name from outside it), once, else to the route's nearest
-- waypoint; and to the hunting place at once when `/tank hunt` is typed while the macro runs.
-- Three journeys that fail in a row stop the macro. Answers true when it did something.
local far_fails, hunt_tried = 0, false
function far_step()
	if not E.can("nav") then
		return false
	end
	local d = (S.get("EnableNav") and E.route ~= nil and not E.route_done) and route_distance() or nil
	local far = d ~= nil and d > S.get("JourneyToRouteMetres")
	local hunt_now = E.hunt_due and E.hunt ~= nil
	if not hunt_now and not far then
		far_fails, hunt_tried = 0, false
		return false
	end
	set_state("Travelling")
	-- Told to hunt somewhere: there. Far from the route: to the hunting place, else (none, or still
	-- far once there) to the route's nearest waypoint.
	local where, why
	local to_hunt = hunt_now or (E.hunt ~= nil and not hunt_tried)
	if to_hunt then
		where, why = E.hunt, "to " .. tostring(E.hunt_name)
	else
		local p = E.route.points[E.nearest_point() or 1]
		where, why = p and p.position, "back to the route"
	end
	E.hunt_due = false
	local ok, r = false, "no place to go"
	if where ~= nil then
		ok, r = travel(where, why)
	end
	if ok then
		far_fails = 0
		hunt_tried = hunt_tried or to_hunt
		if E.route ~= nil then
			E.route.at = nil
		end
		return true
	end
	far_fails = far_fails + 1
	if far_fails >= 3 then
		E.stop(string.format("the route is %d m away and no journey reaches it (%s)", math.floor(d or -1), tostring(r)))
	end
	return true
end

-- How long a corpse is gone back for, seconds; how near it is looked for, meters; and how many
-- deaths in a row on the way back give the recovery up.
local RECOVER_GIVE_UP, CORPSE_SEEN, RECOVER_DEATHS = 1800, 60, 3
local metres3

local dead_at = nil
function E.died()
	E.tally.deaths = E.tally.deaths + 1
	local p = game.player.position
	E.note("died (%d so far)%s", E.tally.deaths,
		p and string.format(" at %08X (%.1f, %.1f, %.1f)", p.cell or 0, p.x or 0, p.y or 0, p.z or 0) or "")
	dead_at = os.clock()
	E.target = nil
	combat.stop()
	nav.stop()
	if E.route ~= nil then
		E.route.at = nil
	end
	bb.died()
	-- The corpse is where the character fell: gone back to once it is alive again (RecoverCorpse),
	-- unless the game says nothing was left in it.
	if S.get("RecoverCorpse") and p ~= nil then
		-- Dead again on the way back to a corpse: the new corpse is gone back for, and after
		-- RECOVER_DEATHS deaths in a row on the way the recovery is given up, so a place that kills
		-- the character each time does not hold it.
		local again = E.corpse ~= nil and (E.corpse.deaths or 0) + 1 or 0
		if again >= RECOVER_DEATHS then
			E.note("corpse: given up after dying %d times on the way back", again)
			E.set_corpse(nil)
			return
		end
		E.set_corpse({ at = { cell = p.cell, x = p.x, y = p.y, z = p.z }, died = os.time(),
			name = "Corpse of " .. tostring(game.player.name), deaths = again })
	end
end

-- The corpse to go back for, kept with the character too, so a client that logs in again still
-- goes back for it; nil when there is none.
function E.set_corpse(c)
	E.corpse = c
	if c ~= nil then
		c.tries = c.tries or 0
	end
	store.character.tank_corpse = c and { at = c.at, died = c.died, name = c.name, deaths = c.deaths } or nil
end

-- The corpse kept with the character, at login.
function E.load_corpse()
	local c = store.character.tank_corpse
	E.corpse = c ~= nil and { at = c.at, died = c.died, name = c.name, deaths = c.deaths, tries = 0 } or nil
end

-- Corpse recovery (RecoverCorpse). After a death that left a corpse, the macro travels back to
-- where the character fell (by a journey: portals and recalls too), finds the corpse of its own
-- name there, opens it and takes back everything in it, then carries on. A death that leaves
-- nothing ("You have retained all your items", the game says) leaves nothing to go back for. A
-- corpse not found, or not reached, after three tries, or after half an hour, is given up, and so
-- is the recovery after three deaths in a row on the way. In the corpse's landblock it walks, a
-- corner at a time, stopping to fight what comes near; from elsewhere a journey takes it there.

-- What the game says of the corpse: nothing in it, or (outdoors) where it lies.
function E.corpse_line(text)
	if E.corpse == nil then
		return
	end
	if string.find(text, "You do not need to recover your corpse", 1, true) then
		E.note("corpse: nothing was left in it")
		E.set_corpse(nil)
	end
end

-- Meters from the character to `place`, height counted (a dungeon's floors), or nil.
function metres3(place)
	local ax, ay, az = E.global_of(game.player.position)
	local bx, by, bz = E.global_of(place)
	if ax == nil or bx == nil then
		return nil
	end
	return math.sqrt((bx - ax) ^ 2 + (by - ay) ^ 2 + ((bz or 0) - (az or 0)) ^ 2)
end

-- The corpse of the character's own name nearest it, within CORPSE_SEEN meters.
local function own_corpse()
	local want = E.corpse and string.lower(E.corpse.name)
	return game.world:first({ class = "Corpse", within = CORPSE_SEEN, sort = "distance" }, function(o)
		return string.lower(o.name or "") == want
	end)
end

-- One step of the recovery: to the corpse, then everything in it taken.
local function recover_step()
	local c = E.corpse
	set_state("Recovering the corpse")
	if os.time() - (c.died or os.time()) > RECOVER_GIVE_UP then
		E.note("corpse: given up after %d min", math.floor(RECOVER_GIVE_UP / 60))
		E.set_corpse(nil)
		return
	end
	local o = own_corpse()
	if o == nil then
		-- Where the character fell, in three dimensions: in a dungeon a corpse a floor below is not
		-- seen from above.
		local d = metres3(c.at)
		if d ~= nil and d <= 3 then
			-- At the place, and no corpse of its name: looked for a moment, then given up.
			c.missing = (c.missing or 0) + 1
			if c.missing >= 10 then
				E.note("corpse: not where the character fell")
				E.set_corpse(nil)
			else
				task.sleep(1000)
			end
			return
		end
		local me = game.player.position
		local block = function(p)
			return p and p.cell and bit32.rshift(p.cell, 16) or nil
		end
		local ok, why
		if block(me) ~= nil and block(me) == block(c.at) then
			-- In the corpse's landblock (its dungeon): walked to, a corner at a time, stopping for
			-- each monster that comes near, as the hunt walks: the monsters that killed the
			-- character are likely still there.
			ok, why = route_walk(c.at, function()
				local dd = metres3(c.at)
				return own_corpse() ~= nil or (dd ~= nil and dd <= 3)
			end, monster_close)
			if not ok and (why == "walking" or why == "stopped") then
				return
			end
		else
			-- Elsewhere: a journey (through portals, by recalls), stopped once the character is in
			-- the corpse's landblock, from where it walks on as above.
			local watch = task.every(250, function()
				if block(game.player.position) == block(c.at) then
					nav.stop()
				end
			end)
			ok, why = travel(c.at, "back to the corpse")
			task.cancel(watch)
			if not ok and why == "no journeys" then
				ok, why = walk(c.at, 3, monster_close)
			end
			if not ok and block(game.player.position) == block(c.at) then
				return
			end
		end
		if not ok then
			c.tries = c.tries + 1
			if c.tries >= 3 then
				E.note("corpse: could not get to it (%s)", tostring(why))
				E.set_corpse(nil)
			end
		end
		return
	end
	local ok, why = approach(o, math.max(S.metres("CorpseApproachRange-Min"), 1), monster_close)
	if not ok then
		if why == "stuck" then
			unstick()
		elseif why ~= "walking" and why ~= "stopped" then
			c.tries = c.tries + 1
			if c.tries >= 3 then
				E.note("corpse: could not reach it (%s)", tostring(why))
				E.set_corpse(nil)
			end
		end
		return
	end
	if game.inventory.open_container ~= o.id then
		ok, why = pawait(act.use(o.id))
		if not (ok and wait_for(function()
			return game.inventory.open_container == o.id
		end, 3)) then
			c.tries = c.tries + 1
			E.note("corpse: did not open (%s)", tostring(why))
			if c.tries >= 3 then
				E.set_corpse(nil)
			end
			return
		end
	end
	local again = take_from(o, { items = {}, everything = true })
	if not again then
		local left = #game.world:find({ container = o.id })
		E.tally.recovered = (E.tally.recovered or 0) + 1
		E.note("corpse: recovered%s", left > 0 and string.format(", %d items left in it", left) or "")
		E.set_corpse(nil)
	end
end

-- One decision and one step.
-- What the macro does without a click of the player's needs, per domain, a world that allows the
-- domain without a gesture. A domain the world does not allow so is paused, and said once why; it
-- resumes on its own once allowed. (Within the 30 s after the player's own /tank start the
-- player's gesture covers every domain the world allows at all.)
local paused = {}
E.paused_domains = paused
local policy_seen, policy_at = nil, -100

local function rule_of(domain)
	if os.clock() - policy_at > 5 then
		local ok, info = pcall(policy.info)
		policy_seen = ok and info or nil
		policy_at = os.clock()
	end
	return policy_seen and policy_seen.rules and policy_seen.rules[domain] or "allow"
end

-- Why the macro may not do `domain` unattended, or nil when it may.
function E.missing(domain)
	local rule = rule_of(domain)
	if rule == "deny" then
		return string.format("this world does not allow %s automation", domain)
	end
	if rule ~= "allow" then
		return string.format("this world allows %s only with a click of yours", domain)
	end
	return nil
end

function E.can(domain)
	if (permissions.activation() or 0) > 0.5 then
		return true
	end
	local why = E.missing(domain)
	if why == nil then
		if paused[domain] then
			paused[domain] = nil
			E.note("%s resumes: allowed now", domain)
			E.show_status()
		end
		return true
	end
	if not paused[domain] then
		paused[domain] = true
		E.note("%s paused: %s", domain, why)
		E.show_status()
	end
	return false
end

-- The domains the settings use, and what each lacks, said at the start.
function E.check_domains()
	local want = { "heal", "buff" }
	if S.get("EnableCombat") then
		table.insert(want, "combat")
	end
	if S.get("EnableNav") then
		table.insert(want, "nav")
	end
	if S.get("EnableLooting") then
		table.insert(want, "loot")
	end
	if S.get("RandomHelperBuffs") then
		table.insert(want, "support")
	end
	local lacking = {}
	for _, d in ipairs(want) do
		local why = E.missing(d)
		if why ~= nil then
			table.insert(lacking, d .. ": " .. why)
		end
	end
	if #lacking > 0 then
		E.note("unattended, these wait (your /tank start covers 30 s): %s", table.concat(lacking, "; "))
	end
end

-- The longest the character moves backward for one fast-cast buff, milliseconds: let go when the
-- cast ends.
local FAST_CAST_MS = 5000

-- One buff step for `wanted` (a buff `buffs.needed` answered): into magic stance, the mana cycle
-- when there is too little for it, then the cast. Answers true when it did something.
local function buff_step(wanted)
	if wanted == nil or not E.can("buff") or not (game.player.combat_mode == 8 or E.can("combat")) or not E.ready_magic() then
		return false
	end
	set_state("Buffing")
	-- Too little mana for the buff: the mana cycle first. Stamina to Mana while there is the
	-- stamina for it, else Revitalize (stamina back for mana) while there is the mana for that.
	local mana = game.player.mana
	local need = (wanted.spell.mana or 0) + 5
	if mana ~= nil and mana.current ~= nil and mana.current < need then
		local stam = share("stamina") or 0
		local first, second = "mana", "stamina"
		if stam < 0.4 then
			first, second = "stamina", "mana"
		end
		for _, vital in ipairs({ first, second }) do
			local ok, line = recharge.step(vital, "magic", false)
			if line ~= nil then
				E.note("%s (for %s, %d mana wanted)", line, wanted.spell.name, need)
				if ok then
					E.tally.recharges = (E.tally.recharges or 0) + 1
					return true
				end
			end
		end
		-- Neither: a rest gets it back.
		rest_step()
		return true
	end
	-- Fast-cast buffs (VTank's): the character moves backward while it casts, which casts sooner.
	local backing = S.get("FastCastBuffs") and E.can("nav") and nav.motion("backward", FAST_CAST_MS) or nil
	local ok, line = buffs.cast(wanted)
	recharge.other_cast()
	if backing ~= nil then
		nav.stop()
	end
	if ok then
		E.tally.buffs = (E.tally.buffs or 0) + 1
	end
	E.logk("SpellCast", "buff: %s", line)
	return true
end

-- The harmful vulnerability of level 7 or more on the character (an element's vulnerability or
-- Imperil), or nil: what VTank dispels.
local function heavy_vuln()
	for _, e in ipairs(game.player.enchantments or {}) do
		local name = e.name or ""
		if e.beneficial == false and (string.find(name, "Vulnerability", 1, true) or string.find(name, "Imperil", 1, true)) then
			local _, level = spells.family(name)
			if level == nil then
				local sp = magic.spell(e.spell)
				level = sp and sp.level
			end
			if (level or 0) >= 7 then
				return e
			end
		end
	end
	return nil
end

-- The dispel items: Gems of Stillness, the dispel potions and gems, Runes of Dispel, Black Potions,
-- by what their names hold.
local DISPEL_WORDS = { "stillness", "dispel", "black potion" }

-- The dispel item in the pack the Consumables list allows, or nil.
local function dispel_item()
	for _, it in ipairs(game.inventory.items()) do
		local n = string.lower(it.name or "")
		if it.wielder == nil and recharge.consumable(it.name) then
			for _, w in ipairs(DISPEL_WORDS) do
				if string.find(n, w, 1, true) then
					return it
				end
			end
		end
	end
	return nil
end

-- When a dispel was last tried, so one that does not take is not tried again at once.
local dispel_at = nil
local DISPEL_AGAIN = 3

-- VTank's UseDispelItems and CastDispelSelf: with a vulnerability of level 7 or more on the
-- character, a dispel item is used, else Eradicate Life Magic Self cast. Answers true when it
-- tried one.
local function dispel_step()
	if not S.get("UseDispelItems") and not S.get("CastDispelSelf") then
		return false
	end
	if dispel_at ~= nil and os.clock() - dispel_at < DISPEL_AGAIN then
		return false
	end
	local v = heavy_vuln()
	if v == nil then
		return false
	end
	if S.get("UseDispelItems") then
		local it = dispel_item()
		if it ~= nil then
			dispel_at = os.clock()
			set_state("Dispelling")
			local ok, why = pawait(act.use(it.id))
			E.note(ok and "dispelling %s with %s" or "dispelling %s with %s: %s", tostring(v.name), it.name, tostring(why))
			return true
		end
	end
	if S.get("CastDispelSelf") then
		local s = spells.best(spells.DISPEL)
		if s ~= nil and E.ready_magic() then
			dispel_at = os.clock()
			set_state("Dispelling")
			local ok, why = pawait(magic.cast(s.id, nil, { purpose = "heal" }))
			E.note(ok and "dispelling %s with %s" or "dispelling %s with %s: %s", tostring(v.name), s.name, tostring(why))
			return true
		end
	end
	return false
end

-- VTank's RandomHelperBuffs: every RandomHelperIntervalSeconds, with nothing to fight, a
-- protection at random on a player at random within reach, remembering nothing of it.
local HELPER_PROTECTIONS = {
	"Blade Protection Other", "Piercing Protection Other", "Bludgeoning Protection Other", "Fire Protection Other",
	"Cold Protection Other", "Acid Protection Other", "Lightning Protection Other",
}
spells.want(HELPER_PROTECTIONS)
local HELPER_REACH = 20
local helped_at = nil
local function helper_step()
	if not S.get("RandomHelperBuffs") or not E.can("support") then
		return false
	end
	if helped_at ~= nil and os.clock() - helped_at < S.get("RandomHelperIntervalSeconds") then
		return false
	end
	helped_at = os.clock()
	local players = {}
	for _, o in ipairs(game.world:find({ class = "Player", within = HELPER_REACH })) do
		if o.id ~= game.player.id then
			table.insert(players, o)
		end
	end
	local known = {}
	for _, f in ipairs(HELPER_PROTECTIONS) do
		local s = spells.best(f, S.get("MaxSpellLevel"), S.get("SpellDiffExcessThreshold-Buff"))
		if s ~= nil then
			table.insert(known, s)
		end
	end
	if #players == 0 or #known == 0 or not E.ready_magic() then
		return false
	end
	local who, s = players[math.random(#players)], known[math.random(#known)]
	set_state("Buffing")
	local ok, why = pawait(magic.cast(s.id, who.id))
	E.logk("SpellCast", ok and "helper buff: %s on %s" or "helper buff: %s on %s: %s", s.name, who.name, tostring(why))
	return true
end

-- Salvage what the loot profile's Salvage rules took (VTank's auto-salvager). Answers true when it
-- did something.
local function salvage_step()
	set_state("Salvaging")
	local line = pack.salvage_step()
	if line ~= nil then
		E.logk("SalvageList", "%s", line)
		E.tally.salvaged = (E.tally.salvaged or 0) + 1
	end
	return line ~= nil
end

-- The pack kept, one step at a time, looked at every five seconds while there is nothing to do:
-- salvage bags combined by the profile's ranges (CombineSalvage), stacks merged (AutoStack), the
-- main pack crammed into the side packs (AutoCram). Answers true when it did something.
local pack_looked = nil
local function pack_step()
	if pack_looked ~= nil and os.clock() - pack_looked < 5 then
		return false
	end
	local line, kind = nil, "Misc"
	if S.get("EnableLooting") and E.profile ~= nil then
		line, kind = pack.combine_step(E.profile), "SalvageList"
	end
	if line == nil and S.get("AutoStack") then
		line, kind = pack.stack_step(), "Misc"
	end
	if line == nil and S.get("AutoCram") then
		line, kind = pack.cram_step(), "Misc"
	end
	if line == nil then
		pack_looked = os.clock()
		return false
	end
	set_state("Packing")
	E.logk(kind, "%s", line)
	return true
end

-- One use of a buff item from the Consumables that a buff wants (`idle`: the idle top-off's
-- margin). Answers true when it used one.
local function buff_item_step(idle)
	if not S.get("EnableBuffing") or not E.can("buff") or not E.can("nav") then
		return false
	end
	local margin = idle and S.get("IdleBuffTopoffTimeSeconds") or S.get("RebuffTimeRemainingSeconds")
	local item, spell = buffs.next_consumable(margin)
	if item == nil then
		return false
	end
	set_state("Buffing")
	local ok, why = pawait(act.use(item.id))
	buffs.used_consumable(item, ok)
	E.logk("SpellCast", ok and "buff: used %s (%s)" or "buff: could not use %s (%s): %s", item.name, spell.name, tostring(why))
	if ok then
		E.tally.buffs = (E.tally.buffs or 0) + 1
	end
	return true
end

local function tick()
	-- The meta is checked whenever the macro decides what to do, as well as on its timer (VTank
	-- checks it so).
	if E.before_tick ~= nil then
		E.before_tick()
	end
	-- Paused (by the player, or a meta's /vt stop): the meta runs on, the macro does nothing.
	if E.paused then
		set_state("Paused")
		task.sleep(500)
		return
	end
	local dead = dead_at ~= nil and (os.clock() - dead_at < 8 or (share("health") or 1) <= 0)
	if in_portal_space() or dead then
		set_state(dead_at ~= nil and "Dead" or "Portal")
		task.sleep(500)
		return
	end
	if dead_at ~= nil then
		dead_at = nil
		if S.get("StopMacroOnDeath") then
			E.stop("died")
			return
		end
		if S.get("StopAfterDeaths") > 0 and E.tally.deaths >= S.get("StopAfterDeaths") then
			give_in(string.format("died %d times", E.tally.deaths))
			return
		end
		local d = route_distance()
		if E.corpse ~= nil then
			E.note("alive again: back for the corpse first")
		else
			-- Far from the route: back to it by a journey (below), or the macro gives up.
			E.note("back from death %d m from the route", d and math.floor(d) or -1)
		end
	end
	local minutes = S.get("StopAfterMinutes")
	-- Run time by the game's clock (wall time), which a busy frame cannot stretch.
	if minutes > 0 and os.time() - E.tally.started_at > minutes * 60 then
		give_in(string.format("ran %d minutes", minutes))
		return
	end
	local h = share("health")
	if recharge.low("health", "Recharge-Norm-HitP") and can_heal() and E.can("heal") then
		heal_step(math.max(S.share("Recharge-Norm-HitP") + 0.1, S.share("Recharge-NoTarg-HitP")))
		return
	end
	-- A heavy vulnerability on the character is dispelled next.
	if E.can("heal") and dispel_step() then
		return
	end
	-- Then a fellow in need is helped (DoHelp).
	if fellows.help_step() then
		return
	end
	-- A force buff under way comes next: the player asked for the buffs now.
	if buffs.forcing() and buff_step(buffs.needed(false)) then
		return
	end
	-- A run started for the vendor trip alone ends with it.
	if E.trip_only and E.trip == nil then
		E.stop("the vendor trip is done")
		return
	end
	-- A run started for a force buff alone ends with it.
	if E.buff_only then
		if not buffs.forcing() then
			E.stop("the force buff is done")
		else
			set_state("Buffing")
			task.sleep(300)
		end
		return
	end
	-- Stamina and mana under the in-fight marks, with a monster about.
	if monster_near() and E.can("heal") and recharge_step("Recharge-Norm-Stam", "Recharge-Norm-Mana") then
		return
	end
	-- A full pack: to the vendor and back (OnFullPack vendor), once; still full after it, or told
	-- to, the macro stops (or recalls and stops).
	local full = pack_full()
	if not full then
		E.full_trip_done = false
	end
	if S.get("EnableLooting") and E.profile ~= nil and full and S.get("OnFullPack") ~= "keep" and E.trip == nil then
		if S.get("OnFullPack") == "vendor" and not E.full_trip_done then
			local line = E.vendor_trip("the pack is full")
			if E.trip == nil then
				give_in("the pack is full, and " .. line)
				return
			end
			E.trip.full = true
		else
			give_in(E.full_trip_done and "the pack is still full after the vendor" or "the pack is full")
			return
		end
	end
	-- The vendor trip, a monster close at hand fought first.
	if E.trip ~= nil then
		if S.get("EnableCombat") and E.can("combat") then
			local target = E.choose_target(attack_distance())
			if target ~= nil then
				fight_step(target)
				return
			end
		end
		if E.can("nav") then
			trip_step()
		else
			task.sleep(500)
		end
		return
	end
	-- The buffbot trip: due when a buff it gave runs low or is gone (a death takes them all), or
	-- started by a command; a monster close at hand fought first.
	if E.btrip == nil and not E.trip_only then
		local why = bb.due()
		if why ~= nil then
			bb.start(why)
		end
	end
	if E.btrip ~= nil then
		if S.get("EnableCombat") and E.can("combat") then
			local target = E.choose_target(attack_distance())
			if target ~= nil then
				fight_step(target)
				return
			end
		end
		if E.can("nav") then
			bb.step()
		else
			task.sleep(500)
		end
		return
	end
	-- A run started for the buffbot trip alone ends with it.
	if E.btrip_only then
		E.stop("the buffbot trip is done")
		return
	end
	-- Back to the corpse a death left, and everything in it taken again.
	if E.corpse ~= nil and E.can("nav") and E.can("loot") then
		if S.get("EnableCombat") and E.can("combat") then
			local target = E.choose_target(attack_distance())
			if target ~= nil then
				fight_step(target)
				return
			end
		end
		recover_step()
		return
	end
	-- Away from the route (back from a death, a vendor's or a buffbot's, a client come back in
	-- town), or told to hunt somewhere new: back by a journey.
	if far_step() then
		return
	end
	ask_corpses()
	-- What the loot profile's Salvage rules took is salvaged before the next corpse is looted, as
	-- VTank does.
	local salvage_due = S.get("EnableLooting") and #pack.waiting() > 0 and E.can("craft")
	if salvage_due and S.get("LootPriorityBoost") and salvage_step() then
		return
	end
	local corpse = S.get("EnableLooting") and E.profile ~= nil and not pack_full() and next_corpse() or nil
	if corpse ~= nil and not E.can("loot") then
		corpse = nil
	end
	if corpse ~= nil and S.get("LootPriorityBoost") then
		loot_step(corpse)
		return
	end
	if S.get("EnableNav") and E.route ~= nil and not E.route_done and S.get("NavPriorityBoost") and E.can("nav") then
		nav_step()
		return
	end
	-- A pet is summoned against the monsters about (SummonPets) before they are fought.
	if S.get("EnableCombat") and E.can("nav") and pets.step(E.meta_state, damage_of) then
		return
	end
	-- A monster close by is fought first, then the kills are looted, then monsters further off
	-- (within the approach distance) are run at.
	if S.get("EnableCombat") and E.can("combat") then
		local target = E.choose_target(attack_distance())
		if target ~= nil then
			fight_step(target)
			return
		end
	end
	if salvage_due and salvage_step() then
		return
	end
	if corpse ~= nil then
		loot_step(corpse)
		return
	end
	if S.get("EnableCombat") and E.can("combat") then
		local target = E.choose_target()
		if target ~= nil then
			fight_step(target)
			return
		end
	end
	if E.target ~= nil then
		E.target = nil
	end
	-- With nothing in range, stamina and mana are topped off with what is at hand first: buffs
	-- want the mana.
	if not monster_near() and E.can("heal") and recharge_step("Recharge-NoTarg-Stam", "Recharge-NoTarg-Mana") then
		return
	end
	-- Buffs: those running out at once, and with nothing in range, a top-off.
	if buff_step(buffs.needed(S.get("IdleBuffTopoff") and not monster_near())) then
		return
	end
	-- Buffs the Consumables' buff items give (beers, foods, gems).
	if buff_item_step(S.get("IdleBuffTopoff") and not monster_near()) then
		return
	end
	-- Nothing in range: a protection for a player nearby, now and then (RandomHelperBuffs).
	if not monster_near() and helper_step() then
		return
	end
	-- Nothing in range: top off.
	h = share("health")
	if h ~= nil and h < S.share("Recharge-NoTarg-HitP") and can_heal() and (E.last_heal == nil or os.clock() - E.last_heal > 5) and E.can("heal") then
		heal_step(S.share("Recharge-NoTarg-HitP"))
		return
	end
	-- Nothing in range: worn items refilled, the pack kept (salvage combined, stacks merged, the
	-- main pack crammed), and only then a rest.
	if E.can("craft") and refill_step() then
		return
	end
	if E.can("craft") and pack_step() then
		return
	end
	if needs_rest() then
		rest_step()
		return
	end
	if S.get("IdlePeaceMode") and (game.player.combat_mode or 1) > 1 and E.can("combat") then
		pawait(combat.set_mode("peace"))
	end
	if S.get("EnableNav") and E.route ~= nil and not E.route_done and E.can("nav") then
		nav_step()
		return
	end
	set_state("Idle")
	task.sleep(500)
end

local loop = nil
-- Start a run (`opts.buff_only`: a run for a force buff alone, which ends with it;
-- `opts.trip_only`: a run for the vendor trip alone, which ends with it). A paused run is resumed
-- instead.
function E.start(opts)
	if E.running then
		E.resume("started again")
		return
	end
	E.running = true
	E.paused = false
	E.buff_only = opts ~= nil and opts.buff_only == true
	E.trip_only = opts ~= nil and opts.trip_only == true
	E.btrip_only = opts ~= nil and opts.btrip_only == true
	-- A run for an errand (a force buff, the vendor trip) neither arms, nor travels to its route.
	local errand = E.buff_only or E.trip_only or E.btrip_only
	if not E.trip_only then
		E.trip = nil
	end
	if not E.btrip_only then
		E.btrip = nil
	end
	E.full_trip_done = false
	E.show_status()
	E.why_stopped = nil
	E.route_done = false
	reset_tally()
	E.forget_doors()
	pick_bonus = {}
	crumbs = {}
	pets.forget()
	ignored, attacked, pending_kills, corpses, looted, strikes, dead_ids, debuffed = {}, {}, {}, {}, {}, {}, {}, {}
	routes, sights, random_types, offhand_missing, dispel_at = {}, {}, {}, {}, nil
	-- A ghost stays one until it goes, run to run.
	ghost_casts = {}
	corpse_asked = {}
	dead_at = nil
	local function max(v)
		local x = game.player[v]
		return x and x.max or 0
	end
	E.note("started%s: level %s, health %d stamina %d mana %d (%s); route %s; loot profile %s",
		E.buff_only and " for a force buff" or E.trip_only and " for the vendor trip" or "", tostring(game.player.level), max("health"), max("stamina"), max("mana"),
		E.vitals_text(), S.get("Route") ~= "" and S.get("Route") or "none",
		S.get("LootProfile") ~= "" and S.get("LootProfile") or "none")
	loop = task.spawn(function()
		-- Starting took this frame's time (the settings, the route, the profile): the spell book
		-- is read from the next.
		task.sleep(0)
		buffs.want_settings()
		local read, known = pcall(spells.refresh, true)
		if not read then
			E.note("could not read the spell book: %s", tostring(known))
		elseif known > 0 then
			E.note("%d spells known", known)
		end
		E.check_domains()
		if not errand then
			local armed, err = pcall(E.arm)
			if not armed then
				E.note("could not arm: %s", tostring(err))
			end
		end
		-- A hunting place to go to first: travel there, then hunt.
		if E.hunt ~= nil and not errand then
			set_state("Travelling")
			local ok, why = travel(E.hunt, "to " .. tostring(E.hunt_name))
			if not ok then
				E.stop("could not travel to " .. tostring(E.hunt_name) .. ": " .. tostring(why))
				return
			end
			E.note("hunting at %s", tostring(E.hunt_name))
		end
		-- A route further off than JourneyToRouteMetres is travelled to (its first point); one
		-- further than NavFarStopRange that cannot be is not walked (it would be run at in a
		-- straight line).
		local d = route_distance()
		if d ~= nil and not errand and d > S.get("JourneyToRouteMetres") then
			local first = E.route.points[1]
			if first ~= nil and first.position ~= nil and travel(first.position, "to the route's first point") then
				E.route.at = 1
				d = route_distance()
			elseif d > S.metres("NavFarStopRange") then
				E.note("the route is %d m away: not walking it", math.floor(d))
				E.route_done = true
			end
		end
		if d ~= nil and not E.route_done and not errand then
			E.note("the route's nearest waypoint is %d m away", math.floor(d))
		end
		-- An error (an overrun of the frame's time too) is noted with where it happened, and the
		-- loop goes on a second later.
		local function where(e)
			return debug ~= nil and debug.traceback(tostring(e), 2) or tostring(e)
		end
		while E.running do
			local ok, err = xpcall(tick, where)
			if not ok then
				E.note("error: %s", tostring(err))
				task.sleep(1000)
			end
			task.sleep(100)
		end
	end)
end

-- A route the macro can walk: one of points, or a follow route naming the player it follows.
-- Answers nil and why for one it cannot.
local function walkable(r)
	if r ~= nil and r.kind == "follow" then
		if (r.target == nil or r.target == "") and (r.target_id or 0) == 0 then
			return nil, "a follow route names no one to follow"
		end
		r.points = r.points or {}
		return r
	end
	if r ~= nil and r.points == nil then
		return nil, "a route of no waypoints"
	end
	return r
end

-- A route in words, for the log: its kind and waypoints, or whom it follows.
local function route_words(r)
	if r.kind == "follow" then
		return "following " .. tostring(r.target ~= "" and r.target or r.target_id)
	end
	return string.format("%s, %d waypoints", tostring(r.kind), #(r.points or {}))
end
E.route_words = route_words

-- The files the settings name, read from the plugin's folders.
-- Use `r` as the route (a meta's embedded route, a route loaded by name; nil for none), from its
-- nearest waypoint.
function E.set_route(r, name)
	local why
	r, why = walkable(r)
	if why ~= nil then
		E.note("route %s: %s", name or "from the meta", why)
		return
	end
	E.route = r
	E.route_done = false
	crumbs = {}
	if r ~= nil then
		r.at = nil
		E.note("route %s: %s", name or "from the meta", route_words(r))
	else
		E.note("route cleared")
	end
end

-- A file by name: from the player's data folder (in `dir`, or at its top), else from the
-- plugin's own folder `dir`.
function E.read_file(dir, name)
	return files.data(dir .. "/" .. name) or files.data(name) or files.package(dir .. "/" .. name)
end

function E.load_files()
	-- A route made or changed in the game stays until it is saved or another is chosen.
	local edited = E.route_edited and E.route or nil
	E.route, E.profile = nil, nil
	local r = S.get("Route")
	if edited ~= nil then
		E.route = edited
	elseif r ~= "" then
		local text = E.read_file("routes", r)
		if text == nil then
			E.note("no route routes/%s", r)
		else
			local ok, route = pcall(navlib.load, text)
			local why = nil
			if ok then
				route, why = walkable(route)
			end
			if not ok then
				E.note("route %s does not read: %s", r, tostring(route))
			elseif why ~= nil then
				E.note("route %s: %s", r, why)
			else
				E.route = route
				E.note("route %s: %s", r, route_words(route))
			end
		end
	end
	if E.on_load_meta then
		E.on_load_meta(S.get("Meta"))
	end
	local l = S.get("LootProfile")
	if l ~= "" then
		local text = E.read_file("profiles", l)
		if text == nil then
			E.note("no loot profile profiles/%s", l)
		else
			local ok, profile = pcall(lootlib.load, text)
			if ok then
				E.profile = profile
				E.note("loot profile %s: %d rules", l, #profile:rules())
			else
				E.note("loot profile %s does not read: %s", l, tostring(profile))
			end
		end
	end
end

return E
