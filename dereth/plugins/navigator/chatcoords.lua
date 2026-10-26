-- Coordinates seen in chat: every pair anyone writes lands in a short list of the latest, with who
-- wrote it, ready to be pointed at.

local P = require("places")

local C = {}

-- The most kept, newest first.
C.MAX = 10
C.recent = {}

-- Who wrote a chat line, as far as its words tell: a speaker before "says," or "tells you,", a
-- channel's speaker after its name in brackets or before "says on the", or the line's kind. A
-- name the chat window makes a link of is read without its link.
local function speaker(e)
	local text = string.gsub(e.text or "", "<[^>]*>", "")
	if string.match(text, "^You say") or string.match(text, "^%[.-%]%s*You say") or string.match(text, "^You tell ") then
		return "you"
	end
	local who = string.match(text, "^%[.-%]%s*(.-) says, ")
		or string.match(text, "^(.-) says on the .- channel, ")
		or string.match(text, "^(.-) says, ")
		or string.match(text, "^(.-) tells you, ")
		or string.match(text, "^(.-) yells, ")
	return who or tostring(e.kind)
end

-- A chat line arrived: answers the place it names, kept in the list, or nil. The Navigator's own
-- lines are left out.
function C.heard(e)
	local text = e.text or ""
	if string.sub(text, 1, 5) == "[Nav]" then
		return nil
	end
	local p = P.parse(text)
	if p == nil then
		return nil
	end
	local who = speaker(e)
	local first = C.recent[1]
	if first ~= nil and first.who == who and P.show(first.place) == P.show(p) then
		return p
	end
	table.insert(C.recent, 1, { who = who, place = p, text = text })
	while #C.recent > C.MAX do
		table.remove(C.recent)
	end
	return p
end

-- The list's rows: who, where, how far.
function C.rows()
	local rows = {}
	for _, c in ipairs(C.recent) do
		table.insert(rows, { c.who, P.show(c.place), P.show_distance(P.reach(c.place)) })
	end
	return rows
end

return C
