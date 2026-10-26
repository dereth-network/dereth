-- Locations: the places the Navigator knows, searched by name and type.
--
-- Four sources, merged each time they are asked for:
--   * the package's `locations.txt`: `name | coordinates | type` a line (`type` left out is a
--     town), approximate public coordinates;
--   * the client's own portal knowledge (`nav.known_portals()`): each portal where it stands, and a
--     dungeon for each portal that leads indoors, entered by that portal;
--   * the character's recalls, where each has landed it (`nav.recalls()`);
--   * what the character has seen (lifestones and portals coming into view) and the places the
--     player saved by name, both kept for this world in `store.world`.

local P = require("places")

local L = {}

-- Every type, in the order the window offers them.
L.TYPES = { "all", "town", "outpost", "dungeon", "portal", "lifestone", "landmark", "recall", "saved" }

local KNOWN_TYPES = {}
for _, t in ipairs(L.TYPES) do
	KNOWN_TYPES[t] = true
end

-- The most seen places kept for a world.
local MAX_SEEN = 500

-- The package's list.
local bundled = {}
for line in string.gmatch(files.package("locations.txt") or "", "[^\n]+") do
	if string.sub(line, 1, 1) ~= "#" then
		local name, rest = string.match(line, "^%s*(.-)%s*|%s*(.-)%s*$")
		local coords, kind = string.match(rest or "", "^(.-)%s*|%s*(%a+)%s*$")
		coords = coords or rest
		kind = kind and string.lower(kind) or "town"
		local p = P.parse(coords)
		if p and KNOWN_TYPES[kind] then
			table.insert(bundled, { name = name, type = kind, place = p, source = "list" })
		end
	end
end
L.bundled = bundled

-- The world's store, or nil before login.
local function world()
	local ok, s = pcall(function()
		return store.world
	end)
	return ok and s or nil
end

-- A portal's name as its dungeon's: "Portal to Drudge Hideout" and "Drudge Hideout Portal" both
-- name "Drudge Hideout".
local function dungeon_name(portal)
	local n = string.match(portal, "^[Pp]ortal [Tt]o (.+)$") or string.match(portal, "^(.+) [Pp]ortal$")
	return n or portal
end

-- The client's portal knowledge, read at most every 30 seconds (a world can publish thousands).
local portals_read, portals_at = {}, -math.huge
local function portal_places()
	if not host.has("nav.journey") then
		return {}
	end
	if os.clock() - portals_at < 30 then
		return portals_read
	end
	portals_at = os.clock()
	local out = {}
	for _, e in ipairs(nav.known_portals()) do
		if e.retired == nil and e.from ~= nil then
			table.insert(out, { name = e.name, type = "portal", place = e.from, source = e.source })
			if e.to ~= nil and e.to.ns == nil then
				-- It leads indoors: a dungeon, pointed at by its entrance and gone into through it.
				table.insert(out, { name = dungeon_name(e.name), type = "dungeon", place = e.from,
					target = { portal = e.name }, inside = e.to, source = e.source })
			end
		end
	end
	portals_read = out
	return out
end

-- Forget what was read, so the next ask reads the client's knowledge again.
function L.refresh()
	portals_at = -math.huge
end

local RECALL_NAMES = { lifestone = "Lifestone recall", marketplace = "Marketplace recall",
	house = "House recall", mansion = "Mansion recall", allegiance = "Allegiance hometown recall" }

local function recall_places()
	if not host.has("nav.journey") then
		return {}
	end
	local out = {}
	for _, r in ipairs(nav.recalls()) do
		local word = tostring(r.recall)
		table.insert(out, { name = RECALL_NAMES[word] or ("Recall " .. word), type = "recall", place = r.to,
			target = { recall = r.recall }, source = "learned" })
	end
	return out
end

-- The locations the plugin keeps itself: the bundled ones, and this world's seen and saved.
local function own_places()
	local out = {}
	for _, l in ipairs(bundled) do
		table.insert(out, l)
	end
	local w = world()
	if w ~= nil then
		for _, l in pairs(w.seen or {}) do
			table.insert(out, { name = l.name, type = l.type, place = l.place, source = "seen" })
		end
		for name, place in pairs(w.saved or {}) do
			table.insert(out, { name = name, type = "saved", place = place, source = "saved" })
		end
	end
	return out
end

-- Every location, from every source.
function L.all()
	local out = {}
	for _, l in ipairs(bundled) do
		table.insert(out, l)
	end
	for _, l in ipairs(portal_places()) do
		table.insert(out, l)
	end
	for _, l in ipairs(recall_places()) do
		table.insert(out, l)
	end
	local w = world()
	if w ~= nil then
		for _, l in pairs(w.seen or {}) do
			table.insert(out, { name = l.name, type = l.type, place = l.place, source = "seen" })
		end
		for name, place in pairs(w.saved or {}) do
			table.insert(out, { name = name, type = "saved", place = place, source = "saved" })
		end
	end
	return out
end

-- The locations whose name holds `text` (without case; any when empty) and of type `kind` ("all"
-- or nil for any), nearest first, then by name; each with `far`, its distance (nil when the
-- character cannot tell from where it stands).
function L.find(text, kind)
	local want = string.lower(text or "")
	local out = {}
	for _, l in ipairs(L.all()) do
		if (kind == nil or kind == "all" or l.type == kind)
			and (want == "" or string.find(string.lower(l.name), want, 1, true)) then
			l.far = P.reach(l.place)
			table.insert(out, l)
		end
	end
	table.sort(out, function(a, b)
		if (a.far ~= nil) ~= (b.far ~= nil) then
			return a.far ~= nil
		end
		if a.far ~= nil and a.far ~= b.far then
			return a.far < b.far
		end
		return a.name < b.name
	end)
	return out
end

-- The location named `text`: an exact name first, else the nearest whose name holds it. A name
-- of the plugin's own places is found without reading the client's portal knowledge, which a
-- world can publish by the thousand.
function L.named(text, kind)
	local want = string.lower(text or "")
	for _, l in ipairs(own_places()) do
		if string.lower(l.name) == want and (kind == nil or kind == "all" or l.type == kind) then
			return l
		end
	end
	local found = L.find(text, kind)
	for _, l in ipairs(found) do
		if string.lower(l.name) == want then
			return l
		end
	end
	return found[1]
end

-- The nearest location of a type the character can measure to, or nil.
function L.nearest(kind)
	local l = L.find("", kind)[1]
	if l ~= nil and l.far ~= nil then
		return l
	end
	return nil
end

-- Keep `place` under `name` for this world. Answers false before login.
function L.save(name, place)
	local w = world()
	if w == nil or place == nil then
		return false
	end
	local saved = w.saved or {}
	saved[name] = place
	w.saved = saved
	return true
end

-- Forget a saved place. Answers whether there was one.
function L.forget(name)
	local w = world()
	if w == nil or w.saved == nil then
		return false
	end
	for k in pairs(w.saved) do
		if string.lower(k) == string.lower(name) then
			local saved = w.saved
			saved[k] = nil
			w.saved = saved
			return true
		end
	end
	return false
end

-- A lifestone or a portal came into view: kept for this world, where it stands.
function L.seen(kind, name, place)
	local w = world()
	if w == nil or place == nil or name == nil then
		return
	end
	local seen = w.seen or {}
	local key = kind .. "|" .. name .. "|" .. P.show(place)
	if seen[key] ~= nil then
		return
	end
	local n = 0
	for _ in pairs(seen) do
		n = n + 1
	end
	if n >= MAX_SEEN then
		return
	end
	seen[key] = { name = name, type = kind, place = P.plain(place) }
	w.seen = seen
end

return L
