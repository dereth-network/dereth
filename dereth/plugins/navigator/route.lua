-- The route finder: the client plans a journey to a place over walking, the portals it knows and
-- the character's recalls; this lists its legs, the alternatives with fewer means, and carries one
-- out, telling each leg as it begins.

local P = require("places")

local R = {}

-- The ways asked for, best first: the plan the client prefers, then the same without recalls,
-- without portals, and on foot alone. Equal plans are shown once.
R.WAYS = {
	{ name = "Best", opts = {} },
	{ name = "No recalls", opts = { recalls = false } },
	{ name = "No portals", opts = { portals = false } },
	{ name = "On foot", opts = { portals = false, recalls = false } },
}

-- One leg in words.
function R.leg_text(leg)
	if leg.kind == "portal" then
		return "portal " .. tostring(leg.name)
	elseif leg.kind == "recall" then
		return "recall " .. tostring(leg.name)
	elseif leg.kind == "door" then
		return "door"
	elseif leg.kind == "hand_back" then
		return "hand back to " .. tostring(leg.name)
	end
	return "walk"
end

-- A journey's legs, as one line.
function R.legs_text(legs)
	local out = {}
	for _, leg in ipairs(legs) do
		table.insert(out, R.leg_text(leg))
	end
	return table.concat(out, ", ")
end

-- A journey's legs as the window's rows: number, what, where to, how far a walk goes.
function R.leg_rows(legs)
	local rows = {}
	local at = P.here()
	for i, leg in ipairs(legs) do
		local far = ""
		if leg.kind == "walk" and at ~= nil and leg.to ~= nil then
			far = P.show_distance(P.reach(leg.to, at))
		end
		table.insert(rows, { i, R.leg_text(leg), leg.to and P.show(leg.to) or "", far })
		at = leg.to or at
	end
	return rows
end

-- The plans to `target` by each way, in a task: `done(plans)` with each `{ way, legs, seconds }`
-- the client found, best first, a plan equal to one before it left out.
function R.plan_all(target, done)
	task.spawn(function()
		local plans, seen = {}, {}
		for _, way in ipairs(R.WAYS) do
			local ok, r = pawait(nav.plan(target, way.opts))
			if ok and r ~= nil then
				local key = R.legs_text(r.legs)
				if not seen[key] then
					seen[key] = true
					table.insert(plans, { way = way, legs = r.legs, seconds = r.seconds })
				end
			end
		end
		done(plans)
	end)
end

-- Go to `target` by a journey with `opts`. `tell` hears its progress: `tell.line(text)` a line for
-- the chat, `tell.planned(legs)` each plan, `tell.leg(index, what)` each leg as it begins, and
-- `tell.ended()` the end.
function R.go(target, name, opts, tell)
	local o = {
		on_planned = function(e)
			tell.line(string.format("%s %s: %s (about %d s).", e.reason and "Again, by" or "Going by", name,
				R.legs_text(e.legs), math.floor(e.seconds + 0.5)))
			tell.planned(e.legs)
		end,
		on_leg = function(e)
			if e.kind == "portal" then
				tell.line("To the portal " .. tostring(e.name) .. ".")
			elseif e.kind == "recall" then
				tell.line("Recalling: " .. tostring(e.name) .. ".")
			elseif e.kind == "door" then
				tell.line("Opening a door.")
			end
			tell.leg(e.index, R.leg_text(e))
		end,
		on_learned = function(e)
			if e.kind == "refusal" then
				tell.line(tostring(e.name) .. " would not take us.")
			end
		end,
	}
	for k, v in pairs(opts or {}) do
		o[k] = v
	end
	task.spawn(function()
		local ok, r = pawait(nav.path_to(target, o))
		tell.line(ok and ("Arrived at " .. name .. ".") or ("Stopped: " .. tostring(r) .. "."))
		tell.ended()
	end)
end

return R
