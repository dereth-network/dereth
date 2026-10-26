-- Number formatting shared by the XP Meter's window and its chat line.

local M = {}

-- 1234567 -> "1,234,567"; fractions are rounded to whole numbers.
function M.number(n)
	local s = tostring(math.floor(n + 0.5))
	local sign, digits = string.match(s, "^(-?)(%d+)$")
	if digits == nil then
		return s
	end
	local out = digits:reverse():gsub("(%d%d%d)", "%1,"):reverse()
	if out:sub(1, 1) == "," then
		out = out:sub(2)
	end
	return sign .. out
end

return M
