-- Does what each `/act <what>` names, on the first item in the pack that is neither money nor a
-- pack (or the character), naming the item, the character and the first side pack first, and says
-- how each ended: `failed <action> <reason>` or `done <action>`.
local function first(keep)
	for _, o in ipairs(game.inventory.items()) do
		if o.wielder == nil and keep(o) then
			return o.id
		end
	end
	return nil
end

local function item()
	return first(function(o) return o.class ~= "Money" and o.class ~= "Container" end)
end

local function pack()
	return first(function(o) return o.class == "Container" end)
end

on("action.failed", function(e) print("failed", e.action, e.reason) end)
on("action.completed", function(e) print("done", e.action) end)

local doings = {
	use = function() act.use(first(function(o) return o.class == "Food" end)) end,
	appraise = function() act.appraise(item()) end,
	select = function() act.select(item()) end,
	move = function() items.move(item(), pack()) end,
	drop = function() items.drop(item()) end,
	say = function() chat.say("hello from a plugin") end,
	face = function() nav.face(90) end,
	walk = function()
		local here = game.player.position
		nav.move_to({ cell = here.cell, x = here.x + 20, y = here.y, z = here.z }, 2)
	end,
	attack = function() combat.attack(game.player.id) end,
	strike = function() combat.attack(item(), { height = "high", power = 0.5 }) end,
	stop = function() combat.stop() end,
	split = function() items.split(item(), 1) end,
	jump = function() nav.jump{ ms = 500 } end,
	hold = function() nav.motion("forward", 3000) end,
	name = function() end,
}

commands.add("act", "does one thing", function(_, words)
	local what = words[1]
	local f = doings[what]
	if f then
		print("item", item())
		print("me", game.player.id)
		print("pack", pack())
		f()
	else
		print("no such thing", tostring(what))
	end
end)
