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

-- The most of the client's places a search lists.
L.MAX = 50

L.RECALL_NAMES = { lifestone = "Lifestone recall", marketplace = "Marketplace recall",
	house = "House recall", mansion = "Mansion recall", allegiance = "Allegiance hometown recall",
	["spell 48"] = "Primary Portal Recall", ["spell 2647"] = "Secondary Portal Recall",
	["spell 2645"] = "Portal Recall", ["spell 1635"] = "Lifestone Recall" }

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

-- The kinds of the client's places other than portals, for a search of every type where the
-- portal knowledge lists the portals.
local NOT_PORTALS = { "town", "landmark", "lifestone", "vendor", "npc", "cottage", "villa", "mansion", "house" }

-- The places of the client's knowledge whose names have words beginning with `want`'s (any when
-- empty), of type `kind`: the nearest `limit` of them, into `out`. Answers how many there are.
-- One search of the client's index (`nav.search`) finds them, however many the world has.
local function client_places(want, kind, out, limit)
	local journeys = host.has("nav.journey")
	local all = kind == nil or kind == "all"
	-- The portal knowledge lists the portals, and the dungeons and destinations they lead to.
	local portals = journeys and (all or PORTAL_TYPES[kind] ~= nil)
	-- The client's places of the kinds asked for; where the portal knowledge is kept, the
	-- portals among them are listed from it rather than twice.
	local kinds, places = nil, true
	if all then
		kinds = journeys and NOT_PORTALS or nil
	elseif PLACE_KINDS[kind] ~= nil then
		kinds = PLACE_KINDS[kind]
	elseif kind == "portal" and not journeys then
		kinds = { "portal" }
	else
		places = false
	end
	local total = 0
	if portals or places then
		local r = nav.search(want, { portals = portals, places = places, kind = kinds, limit = limit })
		total = r.total or 0
		for _, e in ipairs(r.items or {}) do
			if e.list == "portal" then
				if e.retired == nil and e.from ~= nil then
					if wanted("portal", kind) then
						table.insert(out, { name = e.name, type = "portal", place = P.plain(e.from), source = e.source })
					end
					if e.to ~= nil then
						-- Where it sends: a dungeon when that is indoors, pointed at by its entrance
						-- and gone into through it; another place outdoors, by where it lands. A
						-- journey goes to where this portal sends, not to any portal of its name:
						-- two portals may share one (the Facility Hub's "Tusker Abode" sends to the
						-- land beside the dungeon's own).
						if e.to.ns == nil then
							if wanted("dungeon", kind) then
								table.insert(out, { name = destination_name(e.name), type = "dungeon", place = P.plain(e.from),
									target = P.plain(e.to), inside = e.to, source = e.source })
							end
						elseif wanted("destination", kind) then
							table.insert(out, { name = destination_name(e.name), type = "destination", place = P.plain(e.to),
								source = e.source })
						end
					end
				end
			else
				local t = TYPE_OF[e.kind]
				if t ~= nil and e.at ~= nil then
					local n = e.name
					if t == "house" and e.owner ~= nil then
						n = e.owner .. "'s " .. string.lower(e.name)
					end
					table.insert(out, { name = n, type = t, place = P.plain(e.at), source = e.source })
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
				total = total + 1
			end
		end
	end
	return total
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

-- How much each source is trusted, most first: of two rows for one place, the one kept.
local TRUST = { saved = 1, learned = 2, published = 3, data = 4, hint = 5 }

-- Where a place is, to the metre, for telling two rows of one place apart.
local function spot(p)
	if p == nil then
		return "?"
	end
	if p.ns ~= nil then
		-- A map unit is 240 metres.
		return string.format("%.0f,%.0f", p.ns * 240, p.ew * 240)
	end
	return string.format("%d:%.0f,%.0f,%.0f", p.cell or 0, p.x or 0, p.y or 0, p.z or 0)
end

-- One row for each name, type and spot: the same place known from several sources (seen, the
-- server's, the era's package, the game's data) is listed once, as the most trusted says it.
local function coalesce(out)
	local kept, at = {}, {}
	for _, l in ipairs(out) do
		local key = string.lower(l.name or "") .. "|" .. tostring(l.type) .. "|" .. spot(l.place)
		local i = at[key]
		if i == nil then
			table.insert(kept, l)
			at[key] = #kept
		elseif (TRUST[l.source] or 9) < (TRUST[kept[i].source] or 9) then
			kept[i] = l
		end
	end
	return kept
end

-- The locations whose names have words beginning with `text`'s (without case; any when empty)
-- and of type `kind` ("all" or nil for any): the nearest `limit` (L.MAX when not given) of the
-- client's, and the saved places, one row for each place; those named exactly as `text` first,
-- then nearest first, then by name; each with `far`, its distance (nil when the character cannot
-- tell from where it stands). And how many there are in all.
function L.find(text, kind, limit)
	local want = string.lower(text or "")
	local out = {}
	local total = 0
	if wanted("saved", kind) then
		for _, l in ipairs(saved_places()) do
			if want == "" or string.find(string.lower(l.name), want, 1, true) then
				table.insert(out, l)
				total = total + 1
			end
		end
	end
	if kind ~= "saved" then
		total = total + client_places(want, kind, out, limit or L.MAX)
	end
	out = coalesce(out)
	for _, l in ipairs(out) do
		l.far = P.reach(l.place)
	end
	table.sort(out, function(a, b)
		local ea, eb = string.lower(a.name) == want, string.lower(b.name) == want
		if ea ~= eb then
			return ea
		end
		if (a.far ~= nil) ~= (b.far ~= nil) then
			return a.far ~= nil
		end
		if a.far ~= nil and a.far ~= b.far then
			return a.far < b.far
		end
		return a.name < b.name
	end)
	return out, total
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
