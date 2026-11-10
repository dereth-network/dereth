-- The command table (commands.txt) and the words a line is built from.
--
-- Each row of commands.txt is a command a server of this project answers from chat: its verb, its
-- group, its flags (`confirm`: hard to undo or reaching other players, so it takes a second press;
-- `selection`: with nothing after it, it acts on what the server takes for the selection;
-- `selection-if-any`: it acts on the selection when there is one, else on you), what goes after
-- it and what it does. `-` stands for an empty field.

local M = {}

M.GROUPS = {
	"Travel", "Items", "Players", "Character", "Properties", "Environment",
	"Effects", "Messages", "World", "Housing", "Content", "Inspect",
}

local function field(v)
	if v == "-" then
		return ""
	end
	return v
end

-- The rows of commands.txt, in order: `{ verb, group, confirm, selection, usage, about }`, where
-- `selection` is "always", "if-any" or nil.
function M.read(text)
	local rows = {}
	for line in string.gmatch(text or "", "[^\r\n]+") do
		local f = {}
		for v in string.gmatch(line, "[^\t]+") do
			table.insert(f, v)
		end
		if #f == 5 then
			local flags = "," .. f[3] .. ","
			local selection = nil
			if string.find(flags, ",selection,", 1, true) then
				selection = "always"
			elseif string.find(flags, ",selection-if-any,", 1, true) then
				selection = "if-any"
			end
			table.insert(rows, {
				verb = f[1],
				group = f[2],
				confirm = string.find(flags, ",confirm,", 1, true) ~= nil,
				selection = selection,
				usage = field(f[4]),
				about = f[5],
			})
		end
	end
	return rows
end

-- The command `verb` names, ignoring case as the server does.
function M.find(rows, verb)
	local wanted = string.lower(verb)
	for _, c in ipairs(rows) do
		if string.lower(c.verb) == wanted then
			return c
		end
	end
	return nil
end

function M.trim(s)
	return (string.gsub(s or "", "^%s*(.-)%s*$", "%1"))
end

-- A value that must reach the server as one word: in double quotes when it has spaces. The
-- server's line has no way to hold a double quote, so none is kept.
function M.one_word(value)
	local words = {}
	for w in string.gmatch(string.gsub(value or "", '"', ""), "%S+") do
		table.insert(words, w)
	end
	local v = table.concat(words, " ")
	if string.find(v, " ", 1, true) then
		return '"' .. v .. '"'
	end
	return v
end

-- Optional words in order, as far as they go; nil for a later one given without an earlier.
function M.in_order(words)
	local given = 0
	for i, w in ipairs(words) do
		if w ~= "" then
			given = i
		end
	end
	for i = 1, given do
		if words[i] == "" then
			return nil
		end
	end
	return table.concat(words, " ", 1, given)
end

-- Whether `text` is a whole number no less than zero.
function M.is_count(text)
	return text ~= nil and string.match(text, "^%d+$") ~= nil
end

-- A chat line without the timestamp a player's options may put in front of it.
function M.without_stamp(line)
	local rest = string.match(line, "^%[[^%]]*%] (.*)$")
	return rest or line
end

return M
