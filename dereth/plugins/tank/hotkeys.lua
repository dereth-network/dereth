-- The Tank's hotkeys, as VTank has them: one that runs or pauses the macro, one for each melee
-- attack height, one that loots the open container by the loot profile, and one for each meta
-- state the player binds. Each is a key the player sets in the window (Ctrl or Alt held), bound
-- to the Tank's own command for it; a key that does not read is not bound.

local S = require("settings")

local H = {}

-- The hotkeys: the setting holding the key, what it does, and the command it runs.
H.KEYS = {
	{ "HotkeyRun", "Run or pause", "tank toggle" },
	{ "HotkeyHigh", "Attack high", "tank set DefaultMeleeAttackHeight 1" },
	{ "HotkeyMid", "Attack mid", "tank set DefaultMeleeAttackHeight 2" },
	{ "HotkeyLow", "Attack low", "tank set DefaultMeleeAttackHeight 3" },
	{ "HotkeyLootContainer", "Loot this container", "tank lootcontainer" },
}

-- A key as a hotkey takes it, `Ctrl+Shift+F5` or `Alt+K`: Ctrl or Alt held (Shift may be too),
-- and the key F1 to F24, A to Z or 0 to 9. Answers the key as it is bound (modifiers as written,
-- the key upper case), or nil.
function H.key(text)
	local parts = {}
	for p in string.gmatch(tostring(text or ""), "[^+]+") do
		table.insert(parts, string.match(p, "^%s*(.-)%s*$"))
	end
	if #parts < 2 then
		return nil
	end
	local held = { ctrl = false, shift = false, alt = false }
	for i = 1, #parts - 1 do
		local m = string.lower(parts[i])
		if m == "control" then
			m = "ctrl"
		end
		if held[m] == nil then
			return nil
		end
		held[m] = true
	end
	local key = string.upper(parts[#parts])
	local n = tonumber(string.match(key, "^F(%d%d?)$"))
	local ok = string.match(key, "^%w$") ~= nil or (n ~= nil and n >= 1 and n <= 24)
	if not ok or not (held.ctrl or held.alt) then
		return nil
	end
	local out = {}
	for _, m in ipairs({ "ctrl", "shift", "alt" }) do
		if held[m] then
			table.insert(out, ({ ctrl = "Ctrl", shift = "Shift", alt = "Alt" })[m])
		end
	end
	table.insert(out, key)
	return table.concat(out, "+")
end

-- The meta states bound to keys, `{ state, key }` each, from MetaStateHotkeys
-- ("Hunt=Ctrl+1, Vendor=Alt+V").
function H.states()
	local out = {}
	for _, part in ipairs(S.list("MetaStateHotkeys")) do
		local state, key = string.match(part, "^(.-)%s*=%s*(.+)$")
		if state ~= nil and state ~= "" then
			table.insert(out, { state = state, key = key })
		end
	end
	return out
end

local function save_states(list)
	local parts = {}
	for _, b in ipairs(list) do
		table.insert(parts, b.state .. "=" .. b.key)
	end
	S.set("MetaStateHotkeys", table.concat(parts, ", "))
end

-- Bind meta state `state` to `key` (in place of any key it had). Answers the line to log.
function H.bind_state(state, key)
	local k = H.key(key)
	if state == nil or state == "" then
		return "choose a state first"
	end
	if k == nil then
		return tostring(key) .. " is not a key a hotkey takes (Ctrl or Alt, then F1 to F24, A to Z or 0 to 9)"
	end
	local list = {}
	for _, b in ipairs(H.states()) do
		if b.state ~= state then
			table.insert(list, b)
		end
	end
	table.insert(list, { state = state, key = k })
	save_states(list)
	return string.format("meta state %s on %s", state, k)
end

-- Unbind the meta state bound `i`th. Answers the line to log.
function H.unbind_state(i)
	local list = H.states()
	local b = table.remove(list, i)
	if b == nil then
		return nil
	end
	save_states(list)
	return string.format("meta state %s unbound from %s", b.state, b.key)
end

-- The hotkey nodes for the keys set: `{ setting or state, node }` each, a key bound once (the
-- first to ask for it has it).
function H.nodes()
	local out, used = {}, {}
	local function add(id, text, key, command)
		local k = H.key(key)
		if k ~= nil and not used[k] then
			used[k] = true
			out[id] = ui.hotkey({ id = "hk:" .. id, command = command, text = text, key = k })
		end
	end
	for _, h in ipairs(H.KEYS) do
		add(h[1], h[2], S.get(h[1]), h[3])
	end
	for i, b in ipairs(H.states()) do
		add("state:" .. i, "State " .. b.state, b.key, "tank vt setmetastate " .. b.state)
	end
	return out
end

return H
