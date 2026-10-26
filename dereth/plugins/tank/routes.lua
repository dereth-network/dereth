-- Routes made and changed in the game, as VTank's Route tab and its /vt commands make them: points
-- added where the character stands (or at the selected portal, vendor or NPC), jumps, pauses,
-- chat lines, recalls and checkpoints; a route run reversed (its points in the order made); and a
-- route saved as a VTank .nav file any VTank-format reader takes.

local R = {}

-- The character's place as a route names it: map coordinates and a height in metres. Indoors the
-- map coordinates are worked out from the cell's landblock, as VTank names dungeon places.
function R.here()
	local p = game.player.position
	if p == nil then
		return nil
	end
	if p.ns ~= nil and p.ew ~= nil then
		return { ns = p.ns, ew = p.ew, z = p.z or 0 }
	end
	local bx = bit32.rshift(p.cell, 24)
	local by = bit32.band(bit32.rshift(p.cell, 16), 0xFF)
	return {
		ns = (by * 192 + p.y) / 240 - 101.9,
		ew = (bx * 192 + p.x) / 240 - 101.9,
		z = p.z or 0,
	}
end

-- An object's place, as `here` names the character's.
local function place_of(o)
	local p = o.position
	if p == nil then
		return nil
	end
	if p.ns ~= nil and p.ew ~= nil then
		return { ns = p.ns, ew = p.ew, z = p.z or 0 }
	end
	local bx = bit32.rshift(p.cell, 24)
	local by = bit32.band(bit32.rshift(p.cell, 16), 0xFF)
	return { ns = (by * 192 + p.y) / 240 - 101.9, ew = (bx * 192 + p.x) / 240 - 101.9, z = p.z or 0 }
end

-- A new, empty route of `kind` (circular, linear or once), or a follow route of the character
-- `who` (an object: the player it follows).
function R.new(kind, who)
	kind = kind or "circular"
	if kind == "follow" then
		assert(who ~= nil, "a follow route follows the selected character: select one first")
		return { kind = kind, target = who.name, target_id = who.id, points = {} }
	end
	assert(kind == "circular" or kind == "linear" or kind == "once", "a route is circular, linear, once or follow")
	return { kind = kind, points = {} }
end

-- The selected character, to follow, or nil and why.
function R.selected_character()
	local sel = game.selection()
	if sel == nil or sel.id == game.player.id then
		return nil, "select the character to follow first"
	end
	if sel.class ~= "Player" and sel.type ~= nil and bit32.band(sel.type, 0x10) == 0 then
		return nil, tostring(sel.name) .. " is not a character"
	end
	return sel
end

-- The waypoints that use the selected object, as VTank's Route tab's buttons make them: `portal`
-- (Add Portal/NPC: a portal used and passed through, or an NPC used once), `talk` (Add NPC Talk:
-- an NPC used until it tells the character something), `vendor` (Open Vendor). Answers the point
-- and what it is, or nil and why.
function R.point_using_selected(how)
	local sel = game.selection()
	local at = sel and place_of(sel)
	if at == nil then
		return nil, "select the portal, NPC or vendor first"
	end
	if how == "portal" and (sel.class == "Portal" or sel.class == "Npc" or sel.class == "Vendor") then
		local what = sel.class == "Portal" and "portal " or "use NPC "
		return { kind = "portal", position = at, name = sel.name, class = sel.class_id or 0, exit = at }, what .. tostring(sel.name)
	elseif how == "talk" and (sel.class == "Npc" or sel.class == "Vendor") then
		return { kind = "use_npc", position = at, at = at, name = sel.name, class = sel.class_id or 0 }, "NPC talk " .. tostring(sel.name)
	elseif how == "vendor" and sel.class == "Vendor" then
		return { kind = "vendor", position = at, id = sel.id, name = sel.name }, "vendor " .. tostring(sel.name)
	end
	return nil, string.format("%s is not a %s", tostring(sel.name), how == "vendor" and "vendor"
		or how == "talk" and "NPC" or "portal or NPC")
end

-- A point where the character stands, or at the selected object when it is one a route uses: a
-- portal (used on the way), a vendor (opened) or an NPC (talked to). Answers the point and what it
-- is.
function R.point_here_or_selected()
	local sel = game.selection()
	if sel ~= nil then
		local how = sel.class == "Portal" and "portal" or sel.class == "Vendor" and "vendor" or sel.class == "Npc" and "talk" or nil
		if how ~= nil then
			local p, what = R.point_using_selected(how)
			if p ~= nil then
				return p, what
			end
		end
	end
	local here = R.here()
	if here == nil then
		return nil, "no place"
	end
	return { kind = "point", position = here }, "point"
end

-- A point of `kind` with its own fields, where the character stands.
function R.point(kind, fields)
	local here = R.here()
	if here == nil then
		return nil
	end
	local p = { kind = kind, position = here }
	for k, v in pairs(fields or {}) do
		p[k] = v
	end
	return p
end

-- The route run the other way round, or as it was made (`reversed`; left out, turned round from
-- how it runs now): VTank's reverse route, which runs the points in reverse order without
-- changing it, so the route saves as it was made. A route under way turns back from where it is.
function R.reverse(route, reversed)
	if reversed == nil then
		reversed = not route.reversed
	end
	if (route.reversed == true) ~= reversed then
		route.reversed = reversed
		if route.step ~= nil then
			route.step = -route.step
		end
	end
	return route
end

local KIND = { circular = 1, linear = 2, follow = 3, once = 4 }
local TYPE = { point = 0, recall = 2, pause = 3, chat = 4, vendor = 5, portal = 6, use_npc = 7, checkpoint = 8, jump = 9 }

local function num(v)
	return string.format("%.9g", v or 0)
end

-- The route as a VTank .nav file's text (CRLF line ends, as VTank writes them). A follow route
-- holds the name and object id of the character it follows.
function R.write(route)
	if route.kind == "follow" then
		return table.concat({ "uTank2 NAV 1.2", "3", tostring(route.target or ""), tostring(math.floor(route.target_id or 0)) },
			"\r\n") .. "\r\n"
	end
	local lines = { "uTank2 NAV 1.2", tostring(KIND[route.kind] or 1), tostring(#route.points) }
	local function add(...)
		for _, v in ipairs({ ... }) do
			table.insert(lines, tostring(v))
		end
	end
	for _, p in ipairs(route.points) do
		local at = p.position or { ns = 0, ew = 0, z = 0 }
		local ty = TYPE[p.kind]
		if p.kind == "portal" and p.name == nil then
			-- A portal known by its object alone is written as a point: its id means nothing
			-- in another session.
			ty = 0
		end
		add(ty or 0, num(at.ew), num(at.ns), num((at.z or 0) / 240), 0)
		if ty == 6 then
			local exit = p.exit or at
			add(p.name, p.class or 0, 0, num(exit.ns), num(exit.ew), num((exit.z or 0) / 240))
		elseif ty == 2 then
			add(p.spell or 0)
		elseif ty == 3 then
			add(p.ms or 0)
		elseif ty == 4 then
			add(p.text or "")
		elseif ty == 5 then
			add(p.id or 0, p.name or "")
		elseif ty == 7 then
			local npc = p.at or at
			add(p.name or "", p.class or 0, 0, num(npc.ew), num(npc.ns), num((npc.z or 0) / 240))
		elseif ty == 9 then
			add(num(p.heading), p.shift and "True" or "False", math.floor(p.ms or 0))
		end
	end
	return table.concat(lines, "\r\n") .. "\r\n"
end

-- Map coordinates as typed ("42.2N, 33.8E"), or nil.
function R.coords(text)
	local ns, nsh, ew, ewh = string.match(text or "", "^%s*([%d%.]+)%s*([NnSs])[,%s]*([%d%.]+)%s*([EeWw])%s*$")
	if ns == nil then
		return nil
	end
	return {
		ns = tonumber(ns) * ((nsh == "S" or nsh == "s") and -1 or 1),
		ew = tonumber(ew) * ((ewh == "W" or ewh == "w") and -1 or 1),
		z = 0,
	}
end

local DIRECTIONS = { forward = "forward", strafeleft = "strafe_left", straferight = "strafe_right", backward = "backward", strafe_left = "strafe_left", strafe_right = "strafe_right" }

-- A jump as VTank's commands give it: heading, shift (true or false), milliseconds, and an
-- optional direction (forward, strafeleft, straferight). Answers the jump's fields, or nil.
function R.jump_of(words, from)
	local heading = tonumber(words[from])
	local shift = string.lower(words[from + 1] or "")
	local ms = tonumber(words[from + 2])
	if heading == nil or (shift ~= "true" and shift ~= "false") or ms == nil then
		return nil
	end
	local dir = words[from + 3] and DIRECTIONS[string.lower(words[from + 3])] or nil
	if words[from + 3] ~= nil and dir == nil then
		return nil
	end
	return { heading = heading % 360, shift = shift == "true", ms = math.max(0, math.min(1000, ms)), direction = dir }
end

-- A waypoint in words, for the window's list: what it is, and where or what it holds.
function R.describe(p)
	local at = p.position and string.format("%.2f%s %.2f%s", math.abs(p.position.ns or 0), (p.position.ns or 0) >= 0 and "N" or "S",
		math.abs(p.position.ew or 0), (p.position.ew or 0) >= 0 and "E" or "W") or ""
	if p.kind == "jump" then
		return string.format("heading %d, %d ms%s%s at %s", math.floor(p.heading or 0), math.floor(p.ms or 0),
			p.shift and ", walking" or "", p.direction and p.direction ~= "forward" and (", " .. p.direction) or "", at)
	elseif p.kind == "pause" then
		return string.format("%d ms", math.floor(p.ms or 0))
	elseif p.kind == "chat" then
		return tostring(p.text)
	elseif p.kind == "recall" then
		return string.format("spell %d", p.spell or 0)
	elseif p.kind == "portal" or p.kind == "vendor" or p.kind == "use_npc" then
		return string.format("%s at %s", tostring(p.name), at)
	end
	return at
end

local function route_of(E)
	if E.route == nil then
		E.route = R.new("circular")
		E.route_name = nil
	end
	return E.route
end

local function added(E, p, what)
	if p == nil then
		return "the character has no place yet"
	end
	local r = route_of(E)
	table.insert(r.points, p)
	E.route_edited = true
	return string.format("waypoint %d: %s", #r.points, what)
end

-- A plain point where the character stands, added to the macro's route (a new circular one when
-- it has none), as the recorder adds them. Answers what was done.
function R.add_point_here(E)
	return added(E, R.point("point"), "point")
end

-- Remove waypoint `i` of the macro's route. Answers what was done.
function R.remove(E, i)
	local r = E.route
	if r == nil or r.points[i] == nil then
		return "no such waypoint"
	end
	table.remove(r.points, i)
	r.at = nil
	E.route_edited = true
	return string.format("waypoint %d removed: %d left", i, #r.points)
end

-- Move waypoint `i` of the macro's route by `by` places (-1 up, 1 down). Answers its new index,
-- or nil when it cannot move.
function R.move(E, i, by)
	local r = E.route
	local j = i and i + by
	if r == nil or r.points[i] == nil or r.points[j] == nil then
		return nil
	end
	r.points[i], r.points[j] = r.points[j], r.points[i]
	r.at = nil
	E.route_edited = true
	return j
end

-- A change to the macro's route, as VTank's Route tab makes one: `verb` and its words. Answers
-- what was done, for the chat window.
function R.edit(E, verb, words)
	if verb == "new" then
		local kind = words[3] or "circular"
		if kind == "follow" then
			local who, why = R.selected_character()
			if who == nil then
				return why
			end
			E.set_route(R.new("follow", who), "made in the game")
			E.route_name, E.route_edited = nil, true
			return "a new route following " .. tostring(who.name)
		end
		local ok, r = pcall(R.new, kind)
		if not ok then
			return "a route is circular, linear, once or follow"
		end
		E.route, E.route_name, E.route_edited = r, nil, true
		return "a new " .. kind .. " route"
	elseif verb == "add" and E.route ~= nil and E.route.kind == "follow" then
		-- A follow route's Add sets whom it follows: the selected character (VTank's Route tab).
		local who, why = R.selected_character()
		if who == nil then
			return why
		end
		E.route.target, E.route.target_id = who.name, who.id
		E.route_edited = true
		return "following " .. tostring(who.name)
	elseif verb == "add" then
		local at = R.coords(table.concat(words, " ", 3))
		if at ~= nil then
			return added(E, { kind = "point", position = at }, "point")
		end
		local p, what = R.point_here_or_selected()
		return added(E, p, what)
	elseif verb == "portal" or verb == "talk" or verb == "vendor" then
		local p, what = R.point_using_selected(verb)
		if p == nil then
			return what
		end
		return added(E, p, what)
	elseif verb == "jump" then
		local j = R.jump_of(words, 3)
		if j == nil then
			return "/tank route jump <heading> <true|false> <ms> [forward|strafeleft|straferight]"
		end
		return added(E, R.point("jump", j), string.format("jump %d, %d ms", math.floor(j.heading), j.ms))
	elseif verb == "pause" and tonumber(words[3]) then
		return added(E, R.point("pause", { ms = tonumber(words[3]) }), "pause " .. words[3] .. " ms")
	elseif verb == "chat" and words[3] ~= nil then
		local text = table.concat(words, " ", 3)
		return added(E, R.point("chat", { text = text }), "chat " .. text)
	elseif verb == "recall" and tonumber(words[3]) then
		return added(E, R.point("recall", { spell = tonumber(words[3]) }), "recall " .. words[3])
	elseif verb == "checkpoint" then
		return added(E, R.point("checkpoint"), "checkpoint")
	elseif verb == "reverse" then
		if E.route == nil or E.route.kind == "follow" then
			return "no route of waypoints"
		end
		R.reverse(E.route)
		return string.format("%s: %d waypoints", E.route.reversed and "run reversed" or "run as made", #E.route.points)
	elseif verb == "save" and words[3] ~= nil then
		if E.route == nil then
			return "no route"
		end
		local name = table.concat(words, " ", 3)
		if not string.find(string.lower(name), "%.nav$") then
			name = name .. ".nav"
		end
		local ok, why = pcall(files.save_data, "routes/" .. name, R.write(E.route))
		if not ok then
			return "could not save " .. name .. ": " .. tostring(why)
		end
		E.route_name, E.route_edited = name, false
		if E.route.kind == "follow" then
			return string.format("saved routes/%s (following %s)", name, tostring(E.route.target))
		end
		return string.format("saved routes/%s (%d waypoints)", name, #E.route.points)
	elseif verb == "points" then
		if E.route == nil then
			return "no route"
		end
		if E.route.kind == "follow" then
			return E.route_words(E.route)
		end
		local kinds = {}
		for i, p in ipairs(E.route.points) do
			kinds[i] = p.kind
		end
		return string.format("%s%s, %d waypoints: %s", E.route.kind, E.route.reversed and " (run reversed)" or "",
			#E.route.points, table.concat(kinds, ", "))
	end
	return nil
end

return R
