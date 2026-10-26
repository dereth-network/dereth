-- Quest steps, as a route's chat waypoint or a meta's chat action types them (and the player may
-- type them too): giving an item to a person, picking an item up off the ground, using an item on
-- something, and travelling somewhere with the macro stopped. Typed by a route, each is finished
-- before the route goes on.
--
-- VTank has no such waypoints: it gives and uses items only through its meta expressions
-- (`actiontrygiveitem`, `actiontryapplyitem`, `actiontryuseitem`), which fire and forget. These
-- are the plugin's own, and wait for the game to say the step was done:
--
-- /tank give <item> to <person>   walk to the nearest person of that name and hand them the item
--                                  from the pack; done once it has left the pack
-- /tank pickup <item>              walk to the nearest of that item lying on the ground and pick it
--                                  up; done once it is in the pack
-- /tank useon <item> on <target>   use an item from the pack on the nearest object of that name, or
--                                  on an item the character holds (a key on a door, an oil on a
--                                  weapon); done once the use is acknowledged, and a wielded
--                                  weapon it remakes taken up again
-- /tank go <place>                 travel there by a journey, the macro stopped (a place as
--                                  /tank hunt takes it)
--
-- A name ending in `*` names whatever begins with the rest: `Training *` is the character's
-- training weapon, whichever it is.

local Q = {}

-- How near the person or the item the character walks before it gives or picks up, metres (the
-- server walks it the last of the way); how far it looks for them, metres.
local REACH, SEEN = 3, 60
-- How near a thing an item is used on (a key on a door) the character walks first, metres: within
-- the thing's reach for a use, so the use is made where the character stands and the server has
-- no walk of its own to make first.
local USE_REACH = 1.5
-- How many times a give is tried, and how long between tries (a person still talking takes
-- nothing until they have done), milliseconds.
local GIVE_TRIES, GIVE_WAIT = 4, 2500
-- The longest a walk to a person or an item goes on, seconds.
local WALK_SECONDS = 60
-- The longest a wielded weapon a use remakes takes to come back into the pack, seconds.
local REMADE_SECONDS = 3

-- Whether `name` is the name asked for, without case: the whole of it, or with a trailing `*`
-- whatever begins with the rest.
local function named(name, want)
	name, want = string.lower(name or ""), string.lower(want or "")
	if string.sub(want, -1) == "*" then
		local stem = string.sub(want, 1, -2)
		return string.sub(name, 1, #stem) == stem
	end
	return name == want
end

-- An item of that name the character holds (in the pack, a side pack or worn); or nil.
function Q.held(name)
	for _, it in ipairs(game.inventory.items()) do
		if named(it.name, name) then
			return it
		end
	end
	return nil
end

-- The nearest object in the world of that name (not in a container), within SEEN metres; with
-- `ground`, only items lying on the ground.
local function nearest(name, ground)
	return game.world:first({ within = SEEN, sort = "distance" }, function(o)
		return named(o.name, name) and (o.container == nil or o.container == 0)
			and (not ground or o.position ~= nil) and o.id ~= game.player.id
	end)
end

-- Walk to within `reach` of `o`, along the client's route to it (round walls, through doors).
-- Answers ok, why.
local function walk_to(E, o, reach)
	local started = os.clock()
	while os.clock() - started < WALK_SECONDS do
		local ok, why = E.approach(o, reach)
		if ok then
			return true
		end
		if why ~= "walking" then
			return false, why
		end
	end
	return false, "timeout"
end

-- `/tank give <item> to <person>`. Answers ok and the line to tell.
function Q.give(E, item_name, person_name)
	local item = Q.held(item_name)
	if item == nil then
		return false, string.format("give: no %s in the pack", item_name)
	end
	local person = nearest(person_name)
	if person == nil then
		return false, string.format("give: no %s in sight", person_name)
	end
	local ok, why = walk_to(E, person, REACH)
	if not ok then
		return false, string.format("give: could not reach %s (%s)", person_name, tostring(why))
	end
	for try = 1, GIVE_TRIES do
		item = Q.held(item_name)
		if item == nil then
			return true, string.format("gave %s to %s", item_name, person_name)
		end
		local gave, reason = pawait(items.give(item.id, person.id, 1))
		if gave then
			E.tally.given = (E.tally.given or 0) + 1
			return true, string.format("gave %s to %s", item_name, person_name)
		end
		E.note("give: %s did not take %s (%s), try %d", person_name, item_name, tostring(reason), try)
		task.sleep(GIVE_WAIT)
	end
	return false, string.format("give: %s would not take %s", person_name, item_name)
end

-- `/tank pickup <item>`. Answers ok and the line to tell.
function Q.pickup(E, item_name)
	local item = nearest(item_name, true)
	if item == nil then
		return false, string.format("pick up: no %s lying in sight", item_name)
	end
	local ok, why = walk_to(E, item, REACH)
	if not ok then
		return false, string.format("pick up: could not reach %s (%s)", item_name, tostring(why))
	end
	local took, reason = pawait(items.move(item.id))
	if not took then
		return false, string.format("pick up: %s (%s)", item_name, tostring(reason))
	end
	E.tally.picked = (E.tally.picked or 0) + 1
	return true, string.format("picked up %s", item_name)
end

-- `/tank useon <item> on <target>`. Answers ok and the line to tell.
function Q.use_on(E, item_name, target_name)
	local item = Q.held(item_name)
	if item == nil then
		return false, string.format("use: no %s in the pack", item_name)
	end
	local target = Q.held(target_name)
	if target == nil then
		target = nearest(target_name)
		if target == nil then
			return false, string.format("use: no %s in sight", target_name)
		end
		local ok, why = walk_to(E, target, USE_REACH)
		if not ok then
			return false, string.format("use: could not reach %s (%s)", target_name, tostring(why))
		end
	end
	-- What the target is, read now: a target the use remakes is gone once it is used.
	local what, on, class = tostring(item.name), tostring(target.name), target.class
	local wielded = target.wielder ~= nil and target.wielder == game.player.id
	-- A trade skill (an oil on a weapon) is used at rest, in peace mode, as a player uses one.
	if (game.player.combat_mode or 1) > 1 then
		combat.stop()
		pawait(combat.set_mode("peace"))
	end
	local before = {}
	for _, it in ipairs(game.inventory.items()) do
		before[it.id] = true
	end
	local ok, why = pawait(act.use_on(item.id, target.id))
	if not ok then
		return false, string.format("use %s on %s: %s", what, on, tostring(why))
	end
	if wielded then
		-- A weapon the use remakes (an oil on the training weapon) comes back as a new item in
		-- the pack, nothing wielded: the new one is taken up.
		local started, remade = os.clock(), nil
		while remade == nil and os.clock() - started < REMADE_SECONDS do
			for _, it in ipairs(game.inventory.items()) do
				if not before[it.id] and it.class == class then
					remade = it
				end
			end
			if remade == nil then
				task.sleep(250)
			end
		end
		if remade ~= nil then
			local took, reason = pawait(items.wield(remade.id))
			E.note(took and "wielding %s" or "could not wield %s: %s", tostring(remade.name), tostring(reason))
		end
	end
	return true, string.format("used %s on %s", what, on)
end

-- The words of a quest step: `give <item> to <person>`, `pickup <item>` or
-- `useon <item> on <target>`, as { verb, a, b }; or nil.
function Q.parse(words)
	local verb = string.lower(words[1] or "")
	local rest = table.concat(words, " ", 2)
	if verb == "give" then
		local a, b = string.match(rest, "^(.-)%s+to%s+(.+)$")
		if a ~= nil and a ~= "" then
			return { verb = verb, a = a, b = b }
		end
	elseif verb == "pickup" and rest ~= "" then
		return { verb = verb, a = rest }
	elseif verb == "useon" then
		local a, b = string.match(rest, "^(.-)%s+on%s+(.+)$")
		if a ~= nil and a ~= "" then
			return { verb = verb, a = a, b = b }
		end
	end
	return nil
end

-- Carry out a parsed step. Answers ok and the line to tell.
function Q.run(E, step)
	if step.verb == "give" then
		return Q.give(E, step.a, step.b)
	elseif step.verb == "pickup" then
		return Q.pickup(E, step.a)
	end
	return Q.use_on(E, step.a, step.b)
end

return Q
