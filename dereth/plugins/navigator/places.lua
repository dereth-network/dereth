-- Places: coordinates read and written, and how far and which way a place lies from the character.
--
-- A place is a table in either of the forms the host takes: map coordinates `{ ns, ew }` (south
-- and west negative, one unit 240 meters), or a cell and an offset `{ cell, x, y, z }` for a place
-- indoors. The character's own position (`game.player.position`) carries both outdoors.

local P = {}

local POINTS = { "N", "NE", "E", "SE", "S", "SW", "W", "NW" }

-- "12.3N, 45.6E" (or "12.3N 45.6E", "12.3 N 45.6 E") as a place, or nil.
function P.parse(text)
	local ns, nsd, ew, ewd = string.match(text or "", "(%d+%.?%d*)%s*([NnSs])[%s,]+(%d+%.?%d*)%s*([EeWw])")
	if ns == nil then
		return nil
	end
	ns, ew = tonumber(ns), tonumber(ew)
	if string.upper(nsd) == "S" then
		ns = -ns
	end
	if string.upper(ewd) == "W" then
		ew = -ew
	end
	return { ns = ns, ew = ew }
end

-- The landblock a cell belongs to (`0xXXYY`).
function P.landblock(cell)
	return math.floor((cell or 0) / 65536)
end

-- A place as the radar writes it outdoors, or by its cell indoors.
function P.show(p)
	if p == nil then
		return "nowhere"
	end
	if p.ns ~= nil then
		return string.format("%.1f%s %.1f%s", math.abs(p.ns), p.ns >= 0 and "N" or "S", math.abs(p.ew), p.ew >= 0 and "E" or "W")
	end
	return string.format("cell 0x%08X (%.1f, %.1f, %.1f)", p.cell or 0, p.x or 0, p.y or 0, p.z or 0)
end

-- A place (an object's or the character's position, say) kept to the form that names it: map
-- coordinates outdoors, the cell and offset indoors.
function P.plain(p)
	if p == nil then
		return nil
	end
	if p.ns ~= nil then
		return { ns = p.ns, ew = p.ew }
	end
	return { cell = p.cell, x = p.x, y = p.y, z = p.z }
end

-- Where the character is, as a place, or nil before the world.
function P.here()
	return P.plain(game.player.position)
end

-- Metres east, north and up from `a` to `b`: across the map when both are outdoors, within the
-- landblock's frame when both are in the same dungeon; nil when they cannot be compared.
local function offset(a, b)
	if a.ns ~= nil and b.ns ~= nil then
		return (b.ew - a.ew) * 240, (b.ns - a.ns) * 240, 0
	end
	if a.ns == nil and b.ns == nil and a.cell ~= nil and b.cell ~= nil
		and P.landblock(a.cell) == P.landblock(b.cell) then
		return (b.x or 0) - (a.x or 0), (b.y or 0) - (a.y or 0), (b.z or 0) - (a.z or 0)
	end
	return nil
end

-- Metres and the compass point from `from` (the character when nil) to `to`, or nil.
function P.reach(to, from)
	from = from or game.player.position
	if from == nil or to == nil then
		return nil
	end
	local de, dn, du = offset(from, to)
	if de == nil then
		return nil
	end
	local d = math.sqrt(de * de + dn * dn + du * du)
	local heading = math.deg(math.atan2(de, dn)) % 360
	return d, POINTS[math.floor((heading + 22.5) / 45) % 8 + 1]
end

function P.show_distance(d)
	if d == nil then
		return ""
	end
	if d >= 1000 then
		return string.format("%.1f km", d / 1000)
	end
	return string.format("%d m", math.floor(d + 0.5))
end

return P
