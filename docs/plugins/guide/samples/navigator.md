# Sample: Navigator

**Provenance: design.** A walk-through of a sample plugin that ships with Dereth; its source is
[`dereth/plugins/navigator/`](../../../../dereth/plugins/navigator/).

| | |
|---|---|
| id | `network.dereth.navigator`, version 0.4.0, API 0.7 |
| files | `plugin.toml`, `main.lua` (commands, window, HUD), `places.lua` (coordinates and distances), `locations.lua` (the places it knows), `chatcoords.lua` (coordinates seen in chat), `route.lua` (the route finder), `map.lua` (the dungeon map), `locations.txt` (towns), `routes/holtburg-loop.nav` (a VTank route) |
| permissions | `act.interactive` ("Goes to the place you pick when you click Go or type /nav go or /nav travel, through portals and by your recalls where the client makes journeys."), `auto.nav` ("Carries on along a route between points, where the world allows it.") |
| capabilities | `ui.window` required; `ui.hud`, `overlay.arrow`, `overlay.world`, `overlay.map`, `map.clicks`, `nav.moveto`, `nav.journey` and `game.data` optional |

## What it does

It points the way, in the manner of GoArrow. Its window is one search: **Where to?** lists the
places it knows as the player types (a part of a name, a type, a name and a type, or coordinates);
a place picked in the list says where it is and how far; **Go** travels there (through portals and
by the character's recalls where the client makes journeys, else walking straight), **Point** only
puts the arrow on it, and **Stop** stops (**Clear** takes the markers down). A line under the
buttons tells the journey as it goes, leg by leg, and how it ended. Tabs below hold the player's
**Saved** places (kept by **Save here** under a name, let go by **Forget**), the coordinates seen
**From chat**, the spots clicked on the game's map (**Map clicks**), the **Ways** to the target,
and, in a dungeon with the game's data, a **Dungeon** map. A place picked on any tab is what Go and
Point act on.

The arrow HUD at the top of the screen shows where the character is (its coordinates outdoors, its
cell in a dungeon) and, with a target, the target's distance and compass direction; the arrow
points at the target, with a marker in the world and one on the map where the host draws them. A
target is a place or an object: the arrow on an object (the selected one, or the nearest of a
name) follows it as it moves. The HUD opens with the window. The player moves the HUD where they
like by unlocking the HUDs in the plugin panel ([ui.md](../../ui.md#huds)).

**Places.** The Navigator knows four kinds of place, searched together by a part of their name
and by type (`town`, `outpost`, `dungeon`, `portal`, `lifestone`, `landmark`, `recall`, `saved`:
`holt`, `dungeon`, `hideout dungeon`), nearest first:

- the towns of its own list, `locations.txt` (`name | coordinates | type`, approximate public
  coordinates);
- the portals the client knows (the ones the world publishes and the ones the character has
  used), each where it stands, and for each portal that leads indoors a dungeon, pointed at by its
  entrance and gone into through it;
- where each of the character's recalls has landed it;
- what the character has seen and what the player saved: each lifestone and portal that comes
  into view is kept for the world, and `/nav save <name>` keeps where the character stands (by its
  coordinates outdoors, by its cell in a dungeon).

**Coordinates in chat.** Every pair of coordinates anyone writes in chat lands on the From chat
tab, newest first with who wrote it (the last ten); picking one picks it for Go and Point.

**A click on the map.** A left click on the game's world map points at the spot clicked, as a
click on GoArrow's map of Dereth set its arrow, under every interface where the host tells
plugins of clicks on its map (`map.clicks`), and keeps it on the Map clicks tab (the last ten).
Other buttons leave the target as it was.

**The route finder.** The Ways tab asks the client for each way to the target: the best one, then
the same without the character's recalls, without portals, and on foot, a way the same as one
before shown once. Each lists its legs (walks with their lengths, the portals and recalls by name)
and the seconds it should take; **Go this way** goes the way picked, telling each leg in the chat
and on the page as it begins.

**The dungeon map.** In a dungeon, with the game's data (`game.data`), the Dungeon tab draws the
outline of the floors at the character's level around it, three pixels a meter, with the
character's dot and the way it faces. It reads the dungeon's rooms a few at a time while the page is
up, so no one step of the plugin costs much, and draws the map again as the character moves or
turns.

The commands are shortcuts to the window:

| command | does |
|---|---|
| `/nav` | opens the window |
| `/nav <place>` | points at a place by name (an exact name, else the nearest holding the words) |
| `/nav 12.3N, 45.6E` | points at coordinates |
| `/nav find <text> [type]` | lists the places whose name holds the text, of the type named last, nearest first; the window's search shows the same |
| `/nav nearest <type>` | points at the nearest place of a type: `/nav nearest lifestone` |
| `/nav here` | says where the character is |
| `/nav save <name>`, `/nav forget <name>` | keeps where the character stands as a place of the player's own, or forgets one |
| `/nav sel`, `/nav obj <name>` | points at the selected object, or the nearest whose name holds the words, following it |
| `/nav last` | points at the last coordinates anyone wrote in chat |
| `/nav routes` | the ways to the target, in the chat and on the Ways tab |
| `/nav go`, `/nav stop` | goes to the target (by a journey where the client makes them, else walking straight), or stops |
| `/nav travel <where>` | goes by a journey to a place, coordinates, a cell (`0x019E0140`) or `portal <name>`, where a known portal sends; each leg is said in chat |
| `/nav plan <where>` | says the journey there, without going |
| `/nav route <file>` | walks a route from `routes/` |
| `/nav clear` | takes the markers down |

Everything past the window is optional: in a host with no markers it still points (the HUD, the
window, the chat line), in one that cannot move the character Go says so, and with no game data
there is no Dungeon tab. GoArrow's own map of Dereth has no counterpart: the game's own map
takes its place, showing the target's marker and a journey's way and setting the target where it
is clicked.

## The source

**The window.** The search box is an `input`: each change of its text is a `ui.changed` event,
on which the results are made again; picking a row of any list is a `ui.selected` event, on which
the place becomes the one Go and Point act on:

```lua
on("ui.selected", function(e)
	if e.window ~= "main" then
		return
	end
	local row = e.rows and e.rows[1]
	if e.id == "results" and row and found[row] then
		pick(found[row])
	elseif e.id == "saved" and row and saved[row] then
		pick(saved[row])
	...
end)
```

Go is a click, a gesture, so the journey it starts runs with the player's leave: a world that
allows travel only with a gesture still lets it go.

**Places and distances** (`places.lua`). A place is `{ ns = …, ew = … }` in map coordinates, south
and west negative, or `{ cell = …, x = …, y = …, z = … }` for a place indoors, the two forms the
host takes. The distance and direction compare two places outdoors across the map (one map unit
is 240 meters), and two in the same dungeon within its landblock's frame:

```lua
local function offset(a, b)
	if a.ns ~= nil and b.ns ~= nil then
		return (b.ew - a.ew) * 240, (b.ns - a.ns) * 240, 0
	end
	if a.ns == nil and b.ns == nil and a.cell ~= nil and b.cell ~= nil
		and P.landblock(a.cell) == P.landblock(b.cell) then
		return (b.x or 0) - (a.x or 0), (b.y or 0) - (a.y or 0), (b.z or 0) - (a.z or 0)
	end
	return nil
end
```

**The places it knows** (`locations.lua`). The client's portal knowledge is a read,
`nav.known_portals()`; a world can publish thousands, so it is read at most every 30 seconds. A
portal whose destination has no map coordinates leads indoors, and becomes a dungeon too, whose
journey target is the portal itself (`{ portal = name }`, see [nav.md](../../nav.md#journeys)):

```lua
for _, e in ipairs(nav.known_portals()) do
	if e.retired == nil and e.from ~= nil then
		table.insert(out, { name = e.name, type = "portal", place = e.from, source = e.source })
		if e.to ~= nil and e.to.ns == nil then
			table.insert(out, { name = dungeon_name(e.name), type = "dungeon", place = e.from,
				target = { portal = e.name }, inside = e.to, source = e.source })
		end
	end
end
```

Lifestones and portals seen, and the player's saved places, are kept in `store.world`, so they are
this world's whichever character is playing. A lifestone is told by its object class:

```lua
on("world.object_created", function(e)
	if e.class == "Lifestone" or e.class == "Portal" then
		local o = game.world:get(e.id)
		if o ~= nil and o.position ~= nil then
			L.seen(string.lower(e.class), e.name, o.position)
		end
	end
end)
```

**Pointing.** An arrow, a world marker and a map marker, each only where the host has it; on an
object, the markers name the object and the host keeps them where it is:

```lua
if host.has("overlay.arrow") then
	if object then
		markers.arrow:set({ object = object, label = name })
	else
		markers.arrow:set({ position = place, label = name })
	end
end
```

**Chat coordinates** (`chatcoords.lua`). Each `chat.line` is read for a pair of coordinates and
for who wrote it: the words before `says,` (after a channel's name in brackets), `says on the`,
`tells you,` or `yells,`, without the link the chat window makes of a name.

**The route finder** (`route.lua`). `nav.plan` answers a journey without moving; asked once for
each way, with `portals = false` and `recalls = false` among its options, it gives the
alternatives, and the ones the same as a way before are left out:

```lua
R.WAYS = {
	{ name = "Best", opts = {} },
	{ name = "No recalls", opts = { recalls = false } },
	{ name = "No portals", opts = { portals = false } },
	{ name = "On foot", opts = { portals = false, recalls = false } },
}
```

Going a way is `nav.path_to(target, opts)` with the way's options and the journey's callbacks,
which tell each leg as it begins.

**The dungeon map** (`map.lua`). `game.data.landblock(id).cells` lists a dungeon's rooms and
`game.data.cell(id)` gives each one's floor as polygons in the landblock's frame. The map reads six
rooms a step, keeps them, and for the character's level (floors within 3 meters of its height)
draws the floors' edges that no other floor of the level shares: the outline of the rooms and
corridors, without the seams between them. A canvas holds at most 1,000 commands, so the edges
out of view are left out and no more than 900 drawn.

## How it is tested

Over a scripted world, in `dereth/testkit/tests/cpu/plugins/navigator.rs`:

- `the_navigator_finds_places_by_name_and_type_and_points_at_the_nearest`: `/nav find hideout`
  lists a published portal and the dungeon it leads into; `/nav find ar town` lists towns only,
  Arwic first at 5.9 km; `/nav nearest lifestone` points at a lifestone that came into view and
  `/nav nearest dungeon` at the dungeon's entrance, 12 meters away; `/nav go` goes into the
  dungeon through its portal; the window's search lists the same, by a part of a name or a type.
- `the_navigator_keeps_the_players_own_places_outdoors_and_in_a_dungeon`: a place saved outdoors
  is pointed at from 24 meters away; in a dungeon, `/nav here` names the cell and a place saved
  there is kept by its cell; `/nav forget` lets one go.
- `the_navigator_lists_coordinates_seen_in_chat_and_points_at_the_one_picked`: coordinates said on
  the fellowship and co-vassal channels are listed newest first with who said them, the older
  picked and Point putting the arrow on it, and `/nav last` points at the newest.
- `the_navigator_finds_each_way_to_a_place_and_goes_the_one_picked`: to Arwic, the best way goes
  through a portal and the way without portals walks; the way the same as the best is said once;
  going the best way tells the portal leg and arrives at Arwic.
- `the_navigators_arrow_follows_the_selected_object_and_its_hud_shows_where_the_character_is`: the
  HUD shows the character's coordinates; `/nav sel` puts the arrow on the selected guard 12 meters
  north, and both the arrow and the HUD say 24 meters once it walks away.

`dereth/testkit/tests/cpu/plugins.rs :: the_navigator_points_at_a_place_and_walks_there` points
at coordinates and walks there.
`dereth/testkit/tests/cpu/plugins/windows.rs :: the_navigator_finds_a_place_as_it_is_typed_and_goes_there`
types into Where to? (a name, then coordinates), picks the result, goes there by Go and reads
"Arrived" on the status line, saves a place and lists a map click on their tabs, and stops by
Stop. The headless client's
`dereth/headless/tests/dat/presentation/navigator_go.rs` runs Go in a session a player entered (a
recorded login, on the retail data): the journey sets out from where the character stands and
arrives. With the game's data,
`dereth/testkit/tests/dat/plugins.rs :: the_navigator_maps_the_dungeon_floor_around_the_character`
carries the character into a dungeon of 433 rooms and requires the Dungeon tab to draw exactly the
outline the dungeon's floors give at its level around it, worked out again by the test, with the
character's dot, once every room is read.
