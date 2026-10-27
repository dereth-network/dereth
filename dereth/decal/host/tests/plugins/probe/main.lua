-- Hears what a plugin hears of a session. Each kind of event is counted and every one is folded,
-- in the order it came, into a digest of its kind; and once a second, in the world, what the plugin
-- reads of it (every object it knows, the pack, the character) is folded into the `model` digest.
-- `/probe` prints what the character and its pack last read as, and a line for each kind: its
-- name, its count and its digest. Objects and items that arrive with the character, before the
-- first reading in the world, are the world as it was entered rather than events, so they are left
-- out of those two kinds: where a login's burst of messages ends a frame turns on how a host
-- batches them.
local kinds = {}
local order = {}

-- FNV-1a over the bytes of `s`, kept to 32 bits.
local function fold(h, s)
	for i = 1, #s do
		h = bit32.bxor(h, string.byte(s, i))
		h = (bit32.lshift(h, 24) + h * 403) % 4294967296
	end
	return h
end

local function seen(kind, ...)
	local parts = { kind }
	for i = 1, select("#", ...) do
		parts[#parts + 1] = tostring((select(i, ...)))
	end
	local line = table.concat(parts, "\t")
	local k = kinds[kind]
	if not k then
		k = { n = 0, h = 2166136261, lines = {} }
		kinds[kind] = k
		order[#order + 1] = kind
	end
	k.n = k.n + 1
	k.h = fold(k.h, line .. "\n")
	k.lines[k.n] = line
end

local function list(t)
	local out = {}
	for _, v in ipairs(t or {}) do
		out[#out + 1] = tostring(v)
	end
	return table.concat(out, ",")
end

-- Whether the world has been read once since the character entered it.
local entered, settled = false, false

on("session.pregame", function(e) seen("session.pregame", #e.characters, e.world) end)
on("session.login", function(e) seen("session.login", e.id, e.character) end)
on("session.enter_world", function(e)
	entered = true
	seen("session.enter_world", e.id)
end)
on("session.portal_enter", function() seen("session.portal_enter") end)
on("session.portal_exit", function() seen("session.portal_exit") end)
on("session.logout", function()
	entered, settled = false, false
	seen("session.logout")
end)
on("world.object_created", function(e)
	if settled then seen("world.object_created", e.id, e.name, e.class) end
end)
on("world.object_changed", function(e) seen("world.object_changed", e.id, list(e.changed)) end)
on("world.object_removed", function(e) seen("world.object_removed", e.id) end)
on("world.selection_changed", function(e) seen("world.selection_changed", e.id, e.previous) end)
on("character.vital", function(e) seen("character.vital", e.vital, e.current, e.max) end)
on("character.xp", function(e) seen("character.xp", e.total, e.unassigned, e.gained) end)
on("character.luminance", function(e) seen("character.luminance", e.earned, e.max, e.gained) end)
on("character.death", function(e) seen("character.death", e.deaths) end)
on("character.enchantment_added", function(e) seen("character.enchantment_added", e.spell, e.name) end)
on("character.enchantment_removed", function(e) seen("character.enchantment_removed", e.spell, e.name) end)
on("character.fellowship", function(e) seen("character.fellowship", e.change, e.name, list(e.members)) end)
on("inventory.container_opened", function(e) seen("inventory.container_opened", e.id) end)
on("inventory.container_closed", function(e) seen("inventory.container_closed", e.id) end)
on("inventory.item_added", function(e)
	if settled then seen("inventory.item_added", e.id, e.container) end
end)
on("inventory.item_removed", function(e) seen("inventory.item_removed", e.id) end)
on("inventory.item_moved", function(e) seen("inventory.item_moved", e.id, e.container, e.previous) end)
on("items.appraised", function(e) seen("items.appraised", e.id) end)
on("commerce.vendor_opened", function(e) seen("commerce.vendor_opened", e.id) end)
on("commerce.vendor_closed", function(e) seen("commerce.vendor_closed", e.id) end)
on("commerce.trade_opened", function(e) seen("commerce.trade_opened", e.partner) end)
on("commerce.trade_changed", function(e) seen("commerce.trade_changed", e.partner) end)
on("commerce.trade_closed", function() seen("commerce.trade_closed") end)
on("chat.line", function(e) seen("chat.line", e.line, e.kind, e.type, e.text) end)

-- What the plugin reads of the world, once a second while the character is in it.
local player, pack
task.every(1000, function()
	local p = game.player
	if not p.name then
		return
	end
	local function vital(v) return v and (v.current .. "/" .. v.max) or "nil" end
	player = table.concat({ "player", p.name, tostring(p.level), vital(p.health), vital(p.stamina),
		vital(p.mana), tostring(p.xp and p.xp.total), tostring(p.burden), tostring(p.combat_mode) }, "\t")
	local items = game.inventory.items()
	local objects = game.world:find{}
	pack = "pack\t" .. #items .. "\t" .. #objects
	local parts = { player, pack }
	for _, o in ipairs(objects) do
		parts[#parts + 1] = table.concat({ tostring(o.id), tostring(o.name), tostring(o.container),
			tostring(o.wielder), tostring(o.stack) }, ",")
	end
	seen("model", table.concat(parts, "\t"))
	if entered then
		settled = true
	end
end)

-- `/probe` alone: the report. `/probe digests <kind> <first> <count>`: each of those lines' own
-- digest. `/probe part <kind> <line> <offset>`: 200 characters of one line from an offset.
commands.add("probe", "says what the probe heard, or parts of it", function(_, words)
	if words[1] == "digests" then
		local k = kinds[words[2]]
		local from, count = tonumber(words[3]) or 1, tonumber(words[4]) or 10
		for i = from, math.min(from + count - 1, k and k.n or 0) do
			print(i, string.format("%08x", fold(2166136261, k.lines[i])))
		end
		return
	end
	if words[1] == "part" then
		local k = kinds[words[2]]
		local line = k and k.lines[tonumber(words[3]) or 1] or ""
		local off = tonumber(words[4]) or 1
		print(string.sub(line, off, off + 199))
		return
	end
	if player then
		print(player)
		print(pack)
	end
	for _, kind in ipairs(order) do
		local k = kinds[kind]
		print(kind, k.n, string.format("%08x", k.h))
	end
end)
