-- The meta editor, as VTank's Meta tab has one: the meta's rules listed (those that ran in this
-- stay in the meta's state in red), a rule chosen filled into the fields below the list, its state,
-- its condition and its action, each kind under VTank's name with its own fields; and the rule
-- saved, added as a new one, removed or moved. A condition or action that holds others (All, Any,
-- Not; All) lists its parts, and a part opened is edited the same way. Each change is the meta's at
-- once, and saved to its file in the data folder.

local navlib = require("dereth.nav")
local metalib = require("dereth.meta")

local ME = {}

-- The conditions and actions by VTank's names, each with its fields `{ key, label, type }`: key
-- "" where the data is the value itself; type `s` text, `i` a whole number, `d` a number, `x` a
-- landblock or cell (written in hexadecimal), `route` an embedded route chosen from the route
-- files. `parts` marks one that holds others (`one`: exactly one).
ME.CONDITIONS = {
	{ "never", "Never" },
	{ "always", "Always" },
	{ "all", "All", parts = true },
	{ "any", "Any", parts = true },
	{ "no", "Not", parts = true, one = true },
	{ "chat_match", "Chat Message", { { "", "Pattern", "s" } } },
	{ "chat_capture", "Chat Message Capture", { { "p", "Pattern", "s" }, { "c", "Colors", "s" } } },
	{ "pack_slots_le", "Pack Slots <=", { { "", "Slots", "i" } } },
	{ "seconds_in_state_ge", "Seconds in State >=", { { "", "Seconds", "i" } } },
	{ "seconds_in_state_persist_ge", "Seconds in state (P) >=", { { "", "Seconds", "i" } } },
	{ "route_empty", "Navroute Empty" },
	{ "died", "Character Death" },
	{ "vendor_open", "Any Vendor Open" },
	{ "vendor_closed", "Vendor Closed" },
	{ "item_count_le", "Inventory Item Count <=", { { "n", "Item", "s" }, { "c", "Count", "i" } } },
	{ "item_count_ge", "Inventory Item Count >=", { { "n", "Item", "s" }, { "c", "Count", "i" } } },
	{ "monster_count_within", "Monster Name Count Within Distance",
		{ { "n", "Name", "s" }, { "c", "Count", "i" }, { "r", "Distance", "d" } } },
	{ "monster_priority_count_within", "Monster Priority Count Within Distance",
		{ { "p", "Priority", "i" }, { "c", "Count", "i" }, { "r", "Distance", "d" } } },
	{ "need_to_buff", "Need to Buff" },
	{ "no_monsters_within", "No Monsters Within Distance", { { "r", "Distance", "d" } } },
	{ "landblock", "Landblock ==", { { "", "Landblock", "x" } } },
	{ "landcell", "Landcell ==", { { "", "Landcell", "x" } } },
	{ "portal_entered", "Portalspace Entered" },
	{ "portal_exited", "Portalspace Exited" },
	{ "spell_time_ge", "Time Left On Spell >=", { { "sid", "Spell id", "i" }, { "sec", "Seconds", "i" } } },
	{ "burden_ge", "Burden Percent >=", { { "", "Percent", "i" } } },
	{ "route_distance_ge", "Dist Any Route Pt >=", { { "dist", "Distance", "d" } } },
	{ "expr", "Expression", { { "e", "Expression", "s" } } },
}
ME.ACTIONS = {
	{ "none", "None" },
	{ "set_state", "Set Meta State", { { "", "State", "s" } } },
	{ "chat", "Chat Command", { { "", "Command", "s" } } },
	{ "multiple", "All", parts = true },
	{ "walk", "Load Embedded Navroute", { { "", "Route", "route" } } },
	{ "call_state", "Call Meta State", { { "st", "State", "s" }, { "ret", "Return to", "s" } } },
	{ "return_state", "Return From Call" },
	{ "expr", "Expression Action", { { "e", "Expression", "s" } } },
	{ "chat_expr", "Chat Expression", { { "e", "Expression", "s" } } },
	{ "watchdog_set", "Set Watchdog", { { "s", "State", "s" }, { "r", "Distance", "d" }, { "t", "Seconds", "d" } } },
	{ "watchdog_clear", "Clear Watchdog" },
	{ "get_option", "Get VT Option", { { "o", "Option", "s" }, { "v", "Variable", "s" } } },
	{ "set_option", "Set VT Option", { { "o", "Option", "s" }, { "v", "Value", "s" } } },
	{ "create_view", "Create View", { { "n", "Name", "s" }, { "x", "View", "s" } } },
	{ "destroy_view", "Destroy View", { { "n", "Name", "s" } } },
	{ "destroy_all_views", "Destroy All Views" },
}

local function kind_of(kinds, kind)
	for _, k in ipairs(kinds) do
		if k[1] == kind then
			return k
		end
	end
	return nil
end

-- The names a kind choice offers, and the kind a name picks.
local function names(kinds)
	local out = {}
	for _, k in ipairs(kinds) do
		table.insert(out, k[2])
	end
	return out
end
local function by_name(kinds, name)
	for _, k in ipairs(kinds) do
		if k[2] == name then
			return k
		end
	end
	return nil
end

local function copy(v)
	if type(v) ~= "table" then
		return v
	end
	local out = {}
	for k, x in pairs(v) do
		out[k] = copy(x)
	end
	return setmetatable(out, getmetatable(v))
end

-- A field's value in node `n`, and set.
local function get(n, f)
	if f[1] == "" then
		return n.data
	end
	return type(n.data) == "table" and n.data[f[1]] or nil
end
local function put(n, f, v)
	if f[1] == "" then
		n.data = v
	else
		if type(n.data) ~= "table" then
			n.data = {}
		end
		n.data[f[1]] = v
	end
end

-- A field's value as the editor shows it, and the value typed for it (nil when it does not read).
local function shown(f, v)
	if f[3] == "x" then
		return string.format("0x%08X", (tonumber(v) or 0) % 4294967296)
	elseif f[3] == "route" then
		return type(v) == "table" and tostring(v.name or "") or ""
	elseif f[3] == "i" or f[3] == "d" then
		return tostring(tonumber(v) or 0)
	end
	return tostring(v or "")
end
local function typed(f, text)
	text = tostring(text or "")
	if f[3] == "s" then
		return text
	elseif f[3] == "x" then
		local n = tonumber(text)
		if n == nil then
			return nil
		end
		n = math.floor(n) % 4294967296
		return n >= 2147483648 and n - 4294967296 or n
	elseif f[3] == "i" then
		local n = tonumber(text)
		return n and math.floor(n) or nil
	elseif f[3] == "d" then
		return tonumber(text)
	end
	return nil
end

-- A new node of a kind: its data at the fields' defaults (an empty list of parts; Not holding
-- Never).
local function fresh(kinds, kind)
	local k = kind_of(kinds, kind)
	if k.parts then
		return { kind = kind, data = k.one and { { kind = "never", data = 0 } } or {} }
	end
	local n = { kind = kind, data = 0 }
	if k[3] ~= nil and k[3][1][1] ~= "" then
		n.data = {}
	end
	for _, f in ipairs(k[3] or {}) do
		if f[3] ~= "route" then
			put(n, f, f[3] == "s" and "" or 0)
		end
	end
	return n
end

-- A one-line account of a node: its kind's name and its values, a holder's parts in braces.
local function summary(kinds, n)
	local k = kind_of(kinds, n.kind)
	if k == nil then
		return tostring(n.kind) .. " (as read)"
	end
	if k.parts then
		local parts = {}
		for _, p in ipairs(type(n.data) == "table" and n.data or {}) do
			table.insert(parts, summary(kinds, p))
		end
		return k[2] .. " { " .. table.concat(parts, "; ") .. " }"
	end
	local vals = {}
	for _, f in ipairs(k[3] or {}) do
		local v = shown(f, get(n, f))
		table.insert(vals, f[3] == "s" and ("\"" .. v .. "\"") or v)
	end
	return #vals > 0 and (k[2] .. ": " .. table.concat(vals, ", ")) or k[2]
end
ME.summary = summary

-- The editor: the rule chosen (its number, or nil for a rule not yet added), the rule as the fields
-- hold it, and for its condition and its action the part being edited (a path of part numbers)
-- and the part chosen in its parts list.
ME.chosen = nil
ME.draft = nil
ME.path = { c = {}, a = {} }
ME.part = { c = nil, a = nil }

local SIDES = { c = { "condition", ME.CONDITIONS, "If" }, a = { "action", ME.ACTIONS, "Then" } }

local function focus(side)
	local n = ME.draft and ME.draft[SIDES[side][1]]
	for _, i in ipairs(ME.path[side]) do
		n = n and type(n.data) == "table" and n.data[i] or nil
	end
	return n
end

-- A rule of the meta's state `state`, never and none: a new one to fill in.
function ME.new_rule(state)
	ME.chosen = nil
	ME.draft = { state = state or "Default", condition = { kind = "never", data = 0 }, action = { kind = "none", data = 0 } }
	ME.path = { c = {}, a = {} }
	ME.part = { c = nil, a = nil }
end

-- Rule number `i` of `file` chosen: the fields hold it.
function ME.choose(file, i)
	local r = file and file.rules[i]
	if r == nil then
		return
	end
	ME.chosen = i
	ME.draft = copy(r)
	ME.path = { c = {}, a = {} }
	ME.part = { c = nil, a = nil }
end

-- The rules of `file` as the list shows them, `ran` saying which ran in this stay in `state`.
function ME.rows(file, state, ran)
	local rows, seen = {}, {}
	for i, r in ipairs(file and file.rules or {}) do
		local s = tostring(r.state)
		seen[s] = (seen[s] or 0) + 1
		local row = { i, s, summary(ME.CONDITIONS, r.condition), summary(ME.ACTIONS, r.action) }
		if s == state and ran ~= nil and ran[seen[s]] then
			for j, c in ipairs(row) do
				row[j] = { text = string.sub(tostring(c), 1, 300), color = "#FF6060" }
			end
		else
			for j = 3, 4 do
				row[j] = string.sub(row[j], 1, 300)
			end
		end
		table.insert(rows, row)
	end
	return rows
end

-- One side's fields: where the part being edited stands, its kind, its fields, and a holder's
-- parts with their buttons.
local function side_nodes(side, route_files)
	local name, kinds, title = SIDES[side][1], SIDES[side][2], SIDES[side][3]
	local n = focus(side)
	local out = {}
	if n == nil then
		return out
	end
	local where = title
	local walk = ME.draft[name]
	for _, i in ipairs(ME.path[side]) do
		local k = kind_of(kinds, walk.kind)
		where = where .. " > " .. (k and k[2] or walk.kind) .. " part " .. i
		walk = walk.data[i]
	end
	local k = kind_of(kinds, n.kind)
	local opts = names(kinds)
	if k == nil then
		table.insert(opts, tostring(n.kind))
	end
	local head = { ui.label({ id = "mr_" .. side .. "where", text = where .. ":" }),
		ui.choice({ id = "mr_" .. side .. "kind", text = "", options = opts, value = k and k[2] or tostring(n.kind), grow = 1 }) }
	if #ME.path[side] > 0 then
		table.insert(head, ui.button({ id = "mr_" .. side .. "back", text = "Back", tooltip = "To what holds this part" }))
	end
	table.insert(out, ui.row(head))
	for j, f in ipairs(k and k[3] or {}) do
		local id = "mr_" .. side .. j
		if f[3] == "route" then
			local options = { "" }
			for _, r in ipairs(route_files) do
				table.insert(options, r)
			end
			local v = shown(f, get(n, f))
			local have = v == ""
			for _, o in ipairs(options) do
				have = have or o == v
			end
			if not have then
				table.insert(options, v)
			end
			table.insert(out, ui.choice({ id = id, text = f[2], options = options, value = v, grow = 1,
				tooltip = "A route file, embedded in the meta" }))
		else
			table.insert(out, ui.input({ id = id, text = f[2], value = shown(f, get(n, f)), grow = 1 }))
		end
	end
	if k ~= nil and k.parts then
		local rows = {}
		for i, p in ipairs(type(n.data) == "table" and n.data or {}) do
			table.insert(rows, { i, string.sub(summary(kinds, p), 1, 300) })
		end
		table.insert(out, ui.list({ id = "mr_" .. side .. "parts", columns = { { title = "#", width = 4 }, "Part" },
			rows = rows, height = 3, selected = ME.part[side] }))
		table.insert(out, ui.row({
			ui.button({ id = "mr_" .. side .. "part_add", text = "Add part", enabled = not k.one }),
			ui.button({ id = "mr_" .. side .. "part_open", text = "Edit part" }),
			ui.button({ id = "mr_" .. side .. "part_remove", text = "Remove part", enabled = not k.one }),
		}))
	end
	return out
end

-- The editor's nodes, under the rules list: the state, the condition and the action, and the
-- buttons that keep the rule.
function ME.nodes(route_files)
	local out = {}
	if ME.draft == nil then
		ME.new_rule("Default")
	end
	table.insert(out, ui.row({
		ui.input({ id = "mr_state", text = "State", value = tostring(ME.draft.state or ""), grow = 1 }),
		ui.label({ id = "mr_which", text = ME.chosen and ("rule " .. ME.chosen) or "a new rule" }),
	}))
	for _, side in ipairs({ "c", "a" }) do
		for _, node in ipairs(side_nodes(side, route_files)) do
			table.insert(out, node)
		end
	end
	table.insert(out, ui.row({
		ui.button({ id = "mr_save", text = "Save the rule", tooltip = "In place of the rule chosen (or added, for a new one)" }),
		ui.button({ id = "mr_add", text = "Add as new", tooltip = "After the rule chosen" }),
		ui.button({ id = "mr_new", text = "New rule" }),
	}))
	table.insert(out, ui.row({
		ui.button({ id = "mr_remove", text = "Remove" }),
		ui.button({ id = "mr_up", text = "Up" }),
		ui.button({ id = "mr_down", text = "Down" }),
	}))
	return out
end

-- A field or kind changed: the rule as the fields hold it follows. Answers true when the window
-- must be built again (a kind changed), and a line to log when the value does not read.
function ME.changed(id, value, read_route)
	if id == "mr_state" then
		ME.draft.state = tostring(value or "")
		return false
	end
	local side, rest = string.match(id, "^mr_([ca])(.+)$")
	if side == nil then
		return false
	end
	local kinds = SIDES[side][2]
	local n = focus(side)
	if n == nil then
		return false
	end
	if rest == "kind" then
		local k = by_name(kinds, value)
		if k == nil or k[1] == n.kind then
			return false
		end
		local was = kind_of(kinds, n.kind)
		local new = fresh(kinds, k[1])
		-- A holder becoming another holder keeps its parts.
		if was ~= nil and was.parts and k.parts and not k.one then
			new.data = n.data
		end
		n.kind, n.data = new.kind, new.data
		ME.part[side] = nil
		return true
	end
	local j = tonumber(rest)
	local k = kind_of(kinds, n.kind)
	local f = j and k and k[3] and k[3][j]
	if f == nil then
		return false
	end
	if f[3] == "route" then
		if value == nil or value == "" then
			put(n, f, nil)
			return false
		end
		local text = read_route(value)
		local ok, r = pcall(navlib.load, text or "")
		if text == nil or not ok then
			return false, "the route " .. tostring(value) .. " does not read"
		end
		n.data = { name = string.gsub(tostring(value), "%.[Nn][Aa][Vv]$", ""), route = r, text = text }
		return false
	end
	local v = typed(f, value)
	if v == nil then
		return false, string.format("%s: %s is not a %s", f[2], tostring(value), f[3] == "s" and "text" or "number")
	end
	put(n, f, v)
	return false
end

-- A part chosen in a parts list.
function ME.part_chosen(id, row)
	local side = string.match(id, "^mr_([ca])parts$")
	if side ~= nil then
		ME.part[side] = row
	end
end

-- A button of the editor's parts: answers whether it was one, and a line to log.
function ME.part_clicked(id)
	local side, what = string.match(id, "^mr_([ca])(.+)$")
	if side == nil then
		return false
	end
	local kinds = SIDES[side][2]
	local n = focus(side)
	if what == "back" then
		table.remove(ME.path[side])
		ME.part[side] = nil
		return true
	elseif what == "part_add" and n ~= nil then
		table.insert(n.data, fresh(kinds, side == "c" and "never" or "none"))
		ME.part[side] = #n.data
		return true
	elseif what == "part_open" and n ~= nil then
		local i = ME.part[side]
		if i == nil or n.data[i] == nil then
			return true, "choose a part first"
		end
		table.insert(ME.path[side], i)
		ME.part[side] = nil
		return true
	elseif what == "part_remove" and n ~= nil then
		local i = ME.part[side]
		if i == nil or n.data[i] == nil then
			return true, "choose a part first"
		end
		table.remove(n.data, i)
		ME.part[side] = nil
		return true
	end
	return false
end

-- Whether the rule the fields hold can be kept: every embedded route chosen. Answers nil, or why
-- not.
local function incomplete(kinds, n)
	if n.kind == "walk" and (type(n.data) ~= "table" or type(n.data.text) ~= "string") then
		return "choose the route to embed"
	end
	local k = kind_of(kinds, n.kind)
	if k ~= nil and k.parts then
		for _, p in ipairs(n.data) do
			local why = incomplete(kinds, p)
			if why ~= nil then
				return why
			end
		end
	end
	return nil
end

-- A button that changes the meta's rules (`file`'s): answers the rules changed (nil when the click
-- changed none) and the line to log.
function ME.rules_clicked(id, file)
	local rules = file.rules
	local i = ME.chosen
	if id == "mr_new" then
		ME.new_rule(ME.draft and ME.draft.state)
		return nil, nil
	elseif id == "mr_save" or id == "mr_add" then
		if tostring(ME.draft.state or "") == "" then
			return nil, "a rule needs a state"
		end
		local why = incomplete(ME.CONDITIONS, ME.draft.condition) or incomplete(ME.ACTIONS, ME.draft.action)
		if why ~= nil then
			return nil, why
		end
		local rule = copy(ME.draft)
		if id == "mr_save" and i ~= nil and rules[i] ~= nil then
			rules[i] = rule
			return file, "rule " .. i .. " saved"
		end
		local at = (id == "mr_add" and i ~= nil) and i + 1 or #rules + 1
		table.insert(rules, at, rule)
		ME.chosen = at
		return file, "rule " .. at .. " added"
	elseif id == "mr_remove" then
		if i == nil or rules[i] == nil then
			return nil, "choose a rule first"
		end
		table.remove(rules, i)
		ME.new_rule(ME.draft.state)
		return file, "rule " .. i .. " removed"
	elseif id == "mr_up" or id == "mr_down" then
		local j = i and (id == "mr_up" and i - 1 or i + 1)
		if i == nil or rules[i] == nil or rules[j] == nil then
			return nil, nil
		end
		rules[i], rules[j] = rules[j], rules[i]
		ME.chosen = j
		return file, string.format("rule %d moved to %d", i, j)
	end
	return nil, nil
end

-- A rule's identity for keeping what ran: its text as the file writes it.
function ME.key(rule)
	local ok, text = pcall(metalib.write_met, { rules = { rule } })
	return ok and text or tostring(rule)
end

return ME
