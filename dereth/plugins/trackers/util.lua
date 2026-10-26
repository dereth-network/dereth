-- Small helpers the trackers share: numbers, durations, rates, places and distances.
local M = {}

-- 1234567 -> "1,234,567" (rounded).
function M.number(n)
	local s = tostring(math.floor((n or 0) + 0.5))
	local sign, digits = string.match(s, "^(-?)(%d+)$")
	if digits == nil then
		return s
	end
	local out = string.reverse((string.gsub(string.reverse(digits), "(%d%d%d)", "%1,")))
	if string.sub(out, 1, 1) == "," then
		out = string.sub(out, 2)
	end
	return sign .. out
end

-- A share as a whole percentage, or "-" when there is nothing to share.
function M.percent(part, whole)
	if whole == nil or whole <= 0 then
		return "-"
	end
	return string.format("%.0f%%", 100 * part / whole)
end

-- Seconds as "2h 05m", "4m 10s" or "12s"; nil or a negative time as "-".
function M.duration(s)
	if s == nil or s < 0 or s ~= s or s == math.huge then
		return "-"
	end
	s = math.floor(s + 0.5)
	if s >= 3600 then
		return string.format("%dh %02dm", s // 3600, (s % 3600) // 60)
	elseif s >= 60 then
		return string.format("%dm %02ds", s // 60, s % 60)
	end
	return string.format("%ds", s)
end

-- An amount over `seconds`, per hour; 0 before any time has passed.
function M.per_hour(amount, seconds)
	if seconds == nil or seconds <= 0 then
		return 0
	end
	return amount * 3600 / seconds
end

-- A place's map coordinates as "42.2N, 33.8E", or "indoors" for a place with none.
function M.coords(p)
	if p == nil then
		return "-"
	end
	if p.ns == nil or p.ew == nil then
		return "indoors"
	end
	return string.format("%.1f%s, %.1f%s", math.abs(p.ns), p.ns >= 0 and "N" or "S",
		math.abs(p.ew), p.ew >= 0 and "E" or "W")
end

-- The place a position table names, kept as plain values: map coordinates outdoors, the cell
-- and its offset indoors.
function M.place(pos)
	if pos == nil then
		return nil
	end
	return { cell = pos.cell, x = pos.x, y = pos.y, z = pos.z, ns = pos.ns, ew = pos.ew }
end

-- Metres between two places: by map coordinates outdoors, by the offsets inside one landblock
-- indoors; nil when neither says.
function M.distance(a, b)
	if a == nil or b == nil then
		return nil
	end
	if a.ns ~= nil and a.ew ~= nil and b.ns ~= nil and b.ew ~= nil then
		local dn, de = (a.ns - b.ns) * 240, (a.ew - b.ew) * 240
		return math.sqrt(dn * dn + de * de)
	end
	if a.cell ~= nil and b.cell ~= nil and a.cell // 0x10000 == b.cell // 0x10000 and a.x ~= nil and b.x ~= nil then
		local dx, dy, dz = a.x - b.x, a.y - b.y, (a.z or 0) - (b.z or 0)
		return math.sqrt(dx * dx + dy * dy + dz * dz)
	end
	return nil
end

-- Metres from the character to a place, as text.
function M.away(p)
	local d = M.distance(game.player.position, p)
	return d and string.format("%.0f m", d) or "-"
end

return M
