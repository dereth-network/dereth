-- Locations: the places the Navigator knows, searched by name and type.
--
-- Nothing comes with the plugin: every place is one the client knows of the world, asked for as
-- the player searches:
--   * the client's places (`nav.known_places`), the same set its map draws: the towns, landmarks
--     and houses the game's data names, the lifestones, portals, shops and people the character
--     has seen, and those the server publishes;
--   * the client's portal knowledge (`nav.known_portals`): each portal where it stands, a dungeon
--     for each portal that leads indoors and a destination for each that leads to another place
--     outdoors, entered or reached by that portal;
--   * the character's recalls, where each has landed it (`nav.recalls`);
--   * the places the player saved by name, kept for this world in `store.world`.
-- The client filters and measures its lists itself, nearest first, so a world of thousands of
-- places and portals costs a search no more than the few it shows.

local P = require("places")

local L = {}

-- Every type, in the order the window offers them.
L.TYPES = { "all", "town", "landmark", "dungeon", "destination", "portal", "lifestone", "vendor", "npc",
	"house", "recall", "saved" }

-- The kinds of the client's places each type lists.
local PLACE_KINDS = {
	town = { "town" },
	landmark = { "landmark" },
	lifestone = { "lifestone" },
	vendor = { "vendor" },
	npc = { "npc" },
	house = { "cottage", "villa", "mansion", "house" },
}
-- The type each kind of the client's places is listed as.
local TYPE_OF = { town = "town", landmark = "landmark", lifestone = "lifestone", vendor = "vendor", npc = "npc",
	cottage = "house", villa = "house", mansion = "house", house = "house", portal = "portal" }
-- The types the portal knowledge lists.
local PORTAL_TYPES = { portal = true, dungeon = true, destination = true }

-- The most of each source a search lists.
L.MAX = 200

L.RECALL_NAMES = { lifestone = "Lifestone recall", marketplace = "Marketplace recall",
	house = "House recall", mansion = "Mansion recall", allegiance = "Allegiance hometown recall" }

-- The world's store, or nil before login.
local function world()
	local ok, s = pcall(function()
		return store.world
	end)
	return ok and s or nil
end

-- A portal's name as its destination's: "Portal to Drudge Hideout" and "Drudge Hideout Portal"
-- both name "Drudge Hideout".
local function destination_name(portal)
	local n = string.match(portal, "^[Pp]ortal [Tt]o (.+)$") or string.match(portal, "^(.+) [Pp]ortal$")
	return n or portal
end
L.destination_name = destination_name

-- Nothing kept between searches: the client is asked each time.
function L.refresh() end

-- Whether a place of type `t` is wanted for the type asked for (`kind`, nil or "all" for any).
local function wanted(t, kind)
	return kind == nil or kind == "all" or kind == t
end

-- The places of the client's knowledge whose name holds `want` (any when empty), of type `kind`.
local function client_places(want, kind, out)
	local name = want ~= "" and want or nil
	local journeys = host.has("nav.journey")
	if journeys and (kind == nil or kind == "all" or PORTAL_TYPES[kind]) then
		for _, e in ipairs(nav.known_portals({ name = name, limit = L.MAX })) do
			if e.retired == nil and e.from ~= nil then
				if wanted("portal", kind) then
					table.insert(out, { name = e.name, type = "portal", place = P.plain(e.from), source = e.source })
				end
				if e.to ~= nil then
					-- Where it sends: a dungeon when that is indoors, pointed at by its entrance and
					-- gone into through it; another place outdoors, by where it lands.
					if e.to.ns == nil then
						if wanted("dungeon", kind) then
							table.insert(out, { name = destination_name(e.name), type = "dungeon", place = P.plain(e.from),
								target = { portal = e.name }, inside = e.to, source = e.source })
						end
					elseif wanted("destination", kind) then
						table.insert(out, { name = destination_name(e.name), type = "destination", place = P.plain(e.to),
							target = { portal = e.name }, source = e.source })
					end
				end
			end
		end
	end
	if journeys and wanted("recall", kind) then
		for _, r in ipairs(nav.recalls()) do
			local word = tostring(r.recall)
			local n = L.RECALL_NAMES[word] or ("Recall " .. word)
			if want == "" or string.find(string.lower(n), want, 1, true) then
				table.insert(out, { name = n, type = "recall", place = r.to, target = { recall = r.recall },
					source = "learned" })
			end
		end
	end
	local kinds = nil
	if kind ~= nil and kind ~= "all" then
		kinds = PLACE_KINDS[kind]
		if kinds == nil and kind ~= "portal" then
			return
		end
		kinds = kinds or { "portal" }
		if kind == "portal" and journeys then
			-- The portal knowledge lists the portals.
			return
		end
	end
	for _, p in ipairs(nav.known_places({ name = name, kind = kinds, limit = L.MAX })) do
		local t = TYPE_OF[p.kind]
		-- Where the client keeps portal knowledge, its portals are listed from it, once.
		if t ~= nil and p.at ~= nil and not (t == "portal" and journeys) then
			local n = p.name
			if t == "house" and p.owner ~= nil then
				n = p.owner .. "'s " .. string.lower(p.name)
			end
			table.insert(out, { name = n, type = t, place = P.plain(p.at), source = p.source })
		end
	end
end

-- The places the player saved for this world.
local function saved_places()
	local out = {}
	local w = world()
	if w ~= nil then
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
	if wanted("saved", kind) then
		for _, l in ipairs(saved_places()) do
			if want == "" or string.find(string.lower(l.name), want, 1, true) then
				table.insert(out, l)
			end
		end
	end
	if kind ~= "saved" then
		client_places(want, kind, out)
	end
	for _, l in ipairs(out) do
		l.far = P.reach(l.place)
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

-- The location named `text`: an exact name first, else the nearest whose name holds it. A saved
-- place's name is found without reading the client's knowledge.
function L.named(text, kind)
	local want = string.lower(text or "")
	for _, l in ipairs(saved_places()) do
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

return L
