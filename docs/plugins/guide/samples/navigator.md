# Sample: Navigator

**Provenance: design.** A walk-through of a sample plugin that ships with Dereth; its source is
[`dereth/plugins/navigator/`](../../../../dereth/plugins/navigator/).

| | |
|---|---|
| id | `network.dereth.navigator`, version 0.5.0, API 0.7 |
| files | `plugin.toml`, `main.lua` (commands and windows), `places.lua` (coordinates and distances), `locations.lua` (the places it knows), `chatcoords.lua` (coordinates seen in chat), `route.lua` (the route finder), `map.lua` (the dungeon map) |
| permissions | `act.interactive` ("Goes to the place you pick when you click Go or type /nav go or /nav travel, through portals and by your recalls where the client makes journeys."), `auto.nav` ("Carries on along a route between points, where the world allows it.") |
| capabilities | `ui.window` required; `overlay.world`, `overlay.map`, `map.clicks`, `nav.moveto`, `nav.journey`, `nav.path` and `game.data` optional |

## What it does

Navigator helps you get from point A to point B; it is an autonomous navigation plugin in the
vein of GoArrow for Decal. Its window is one search: **Where to?** lists the places it knows as the
player types (a part of a name, a type, a name and a type, or coordinates), in one list with the
player's saved places, the coordinates seen in chat and the spots picked on the game's map. Each
row gives the place's name, its type and where it is: its coordinates outdoors, and indoors the
landblock it is in (`in 0x01AE`), which names the dungeon or building. The **Filter** beside the
search shows **Everything**, or only what was **Saved**, seen in **Chat** or picked on the **Map**.
A saved place, a line of chat and a spot on the map carry a mark in the list's narrow last column,
and a click on it deletes the entry; a click elsewhere on a row makes the place the destination.

The line under the list names it, **Destination:** with where it is (indoors, by its whole cell:
`in cell 0x01AE0361`), and it is marked in the world and on the map where the host draws markers.
**Go** travels there (through portals and by the character's recalls where the client makes
journeys, else walking along the client's route round what stands in the way), **Stop** stops (it
is up only while going) and **Clear** lets the destination go (up only while there is one). A new
destination, or clearing, stops a journey under way: it was going to the old one. **Dungeon Map**,
at the right, opens the dungeon map in a window of its own; it is up only indoors, and outdoors
says "You're outdoors". The status line below tells the journey as it goes, leg by leg, and how it
ended. At the bottom, a **Name** box, where the character is (its coordinates outdoors, its cell
indoors) and **Save**, which keeps where the character stands under the name.

A destination is a place or an object: the marker on an object (the selected one, or the nearest of
a name) follows it as it moves.

The destination, the spots picked on the map and the coordinates seen in chat are kept for the world,
so a reload of the plugin, or the next session in that world, has them again.

**Places.** The Navigator comes with no places of its own: every place it finds is one the client
knows of the world, or one the player saved. They are searched together by a part of their name and
by type (`town`, `landmark`, `dungeon`, `destination`, `portal`, `lifestone`, `vendor`, `npc`,
`house`, `recall`, `saved`: `holt`, `dungeon`, `hideout dungeon`), nearest first:

- the client's places of the world, the same set its map draws (`nav.known_places`): the towns,
  landmarks and houses the game's data names, read from the player's own files; the lifestones,
  portals, shops and people the character has seen; and what the world's server publishes;
- the portals the client knows (the ones the world publishes and the ones the character has
  used), each where it stands; for each portal that leads indoors a dungeon, pointed at by its
  entrance and gone into through it, and for each that leads elsewhere outdoors a destination,
  where it sends;
- where each of the character's recalls has landed it;
- what the player saved: `/nav save <name>` keeps where the character stands (by its coordinates
  outdoors, by its cell in a dungeon).

The client filters and measures these lists itself, so a world that publishes thousands of places
and portals costs a search no more than the few it shows.

**Coordinates in chat.** Every pair of coordinates anyone writes in chat lands in the list, typed
`chat`, newest first with who wrote it (the last ten).

**A spot on the map.** A right click on the game's world map, or two left clicks on the same spot in
a row, makes the spot the destination, as a click on GoArrow's map of Dereth set its arrow, under
every interface where the host tells plugins of clicks on its map (`map.clicks`), and keeps it in
the list, typed `map` (the last ten). A single left click is the map's own, for dragging it, and
leaves the destination as it was.

**The route finder.** `/nav routes` asks the client for each way to the destination: the best one,
then the same without the character's recalls, without portals, and on foot, a way the same as one
before said once. Each is said in chat with its legs (walks with their lengths, the portals and
recalls by name) and the seconds it should take; Go goes the best way, telling each leg in the chat
as it begins.

**The dungeon map.** In a dungeon, with the game's data (`game.data`), the Dungeon Map window draws
the outline of the floors at the character's level around it, three pixels a meter, and dim the levels
just above and below, with the character's dot and the way it faces, the destination (at the edge
of the map, the way it lies, when it is farther), the way there as the client plans it, and the
creatures in sight. A room is on the character's level when the character stands within its height
or its floor is within 2 meters of the character's feet, so a ramp and the rooms it joins are drawn
together. It reads the dungeon's rooms a few at a time while the window is
up, so no one step of the plugin costs much, and draws the map again as the character moves or
turns.

The commands are shortcuts to the window:

| command | does |
|---|---|
| `/nav` | opens the window |
| `/nav <place>` | makes a place the destination, by name (an exact name, else the nearest holding the words) |
| `/nav 12.3N, 45.6E` | makes coordinates the destination |
| `/nav find <text> [type]` | lists the places whose name holds the text, of the type named last, nearest first; the window's search shows the same |
| `/nav nearest <type>` | makes the nearest place of a type the destination: `/nav nearest lifestone` |
| `/nav here` | says where the character is |
| `/nav save <name>`, `/nav forget <name>` | keeps where the character stands as a place of the player's own, or forgets one |
| `/nav sel`, `/nav obj <name>` | makes the selected object the destination, or the nearest whose name holds the words, following it |
| `/nav last` | makes the last coordinates anyone wrote in chat |
| `/nav routes` | says the ways to the destination in chat |
| `/nav go`, `/nav stop` | goes to the destination (by a journey where the client makes them, else walking along the client's route), or stops |
| `/nav travel <where>` | goes by a journey to a place, coordinates, a cell (`0x019E0140`) or `portal <name>`, where a known portal sends; each leg is said in chat |
| `/nav plan <where>` | says the journey there, without going |
| `/nav route <file>` | walks one of the player's VTank routes from the Navigator's data folder, running through its plain waypoints: straight to a waypoint in sight, round the walls to one out of sight |
| `/nav clear` | lets the destination go, and its markers |

Everything past the window is optional: in a host with no markers it still names the destination
(the window, the chat line), in one that cannot move the character Go says so, and with no game data
the Dungeon Map shows nothing. GoArrow's own map of Dereth has no counterpart: the game's own map
takes its place, showing the destination's marker and a journey's way and setting the destination
where it is picked.

## The source

**The window.** The search box is an `input`: each change of its text is a `ui.changed` event,
on which the list is made again, as is each change of the Filter, a `choice`. Picking a row is a
`ui.selected` event, whose `column` says which column the pointer chose it in, so a click in the
last column deletes the entry and one elsewhere makes the place the destination:

```lua
on("ui.selected", function(e)
	if e.window ~= "main" or e.id ~= "results" then
		return
	end
	local row = e.rows and e.rows[1]
	local l = row and found[row]
	if l == nil then
		return
	end
	if e.column == DELETE_COLUMN and (l.saved ~= nil or l.chat ~= nil or l.map ~= nil) then
		delete(l)
		return
	end
	destine(l.name, l.place, l.target)
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

**The places it knows** (`locations.lua`). Each search asks the client, which filters and
measures its lists itself: `nav.known_places{ name = ..., kind = ..., limit = 200 }` and
`nav.known_portals{ name = ..., limit = 200 }` answer the nearest that match. A portal whose
destination has no map coordinates leads indoors, and becomes a dungeon too, whose journey target is
the portal itself (`{ portal = name }`, see [nav.md](../../nav.md#journeys)):

```lua
for _, e in ipairs(nav.known_portals({ name = name, limit = L.MAX })) do
	if e.retired == nil and e.from ~= nil then
		...
		if e.to ~= nil and e.to.ns == nil then
			table.insert(out, { name = destination_name(e.name), type = "dungeon", place = P.plain(e.from),
				target = { portal = e.name }, inside = e.to, source = e.source })
		end
	end
end
```

The player's saved places are kept in `store.world`, so they are this world's whichever character
is playing.

**Marking the destination.** A world marker and a map marker, each only where the host has it; on
an object, the world marker names the object and the host keeps it where it is:

```lua
if host.has("overlay.world") then
	if t.object then
		markers.world:add({ object = t.object, label = t.name })
	else
		markers.world:add({ position = t.place, label = t.name })
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
rooms a step, keeps them, and for the character's level (and, dim, the levels within 8 meters
above and below) draws the floors' edges that no other floor of the level shares: the outline of the rooms and
corridors, without the seams between them. A canvas holds at most 1,000 commands, so the edges
out of view are left out and no more than 900 drawn.

## How it is tested

Over a scripted world, in `dereth/testkit/tests/cpu/plugins/navigator.rs`:

- `the_navigator_finds_places_by_name_and_type_and_points_at_the_nearest`: `/nav find hideout`
  lists a published portal and the dungeon it leads into; `/nav find ar destination` lists where
  the portal to Arwic sends, at 5.9 km; `/nav find vendor` lists a shop the server published;
  `/nav nearest lifestone` makes a lifestone the client learned the destination and
  `/nav nearest dungeon` the dungeon by its entrance; `/nav go` goes into the dungeon through its
  portal; the window's search lists the same, by a part of a name or a type.
- `the_navigator_brings_no_places_of_its_own`: the package holds only its code and manifest;
  nothing is found before the client knows the world; a town and a house the game's data names are
  found once the client has read them, and a saved place is found.
- `the_navigator_stops_going_to_a_target_it_was_pointed_away_from_or_cleared`: on the way east,
  `/nav clear` stops the character, and so does another destination.
- `the_navigators_buttons_are_up_only_when_they_can_act`: Stop is down until Go and after Stop,
  Clear is down without a destination, and outdoors Dungeon Map is down with "You're outdoors".
- `the_navigators_one_list_holds_saved_chat_and_map_places_under_its_filter_and_deletes_them`: a
  saved place, a line of chat and a spot on the map are in one list, each alone under its filter; a
  click on a row picks it, and a click in its last column deletes it.
- `the_navigator_keeps_its_target_map_clicks_and_chat_coordinates_across_a_reload`: after
  `/plugin reload`, the destination is marked again and the chat's coordinates and the map's spot
  are listed.
- `the_navigator_walks_round_a_wall_where_the_client_makes_no_journeys`: with no journeys, Go
  follows the client's route round a wall to the place behind it.
- `a_route_walks_round_a_wall_to_a_waypoint_out_of_sight`: a VTank route of the player's own, its
  first waypoint in sight and its second behind a wall, is walked to its end round the wall.
- `the_navigator_keeps_the_players_own_places_outdoors_and_in_a_dungeon`: a place saved outdoors
  is the destination from 24 meters away; in a dungeon, `/nav here` names the cell and a place
  saved there is kept by its cell; `/nav forget` lets one go.
- `the_navigator_lists_coordinates_seen_in_chat_and_points_at_the_one_picked`: coordinates said on
  the fellowship and co-vassal channels are listed newest first with who said them, the older
  picked as the destination, and `/nav last` makes the newest it.
- `the_navigator_says_each_way_to_a_place_in_chat_and_goes_the_best`: to Arwic, the best way goes
  through a portal and the way without portals walks; the way the same as the best is said once;
  Go tells the portal leg and arrives at Arwic.
- `the_navigators_destination_follows_the_selected_object_and_the_window_shows_where_the_character_is`:
  the window shows the character's coordinates; `/nav sel` marks the selected guard, with no arrow,
  and the marker stays on it as it walks away.
- `the_navigator_takes_a_spot_picked_on_the_map_by_a_right_or_double_click`: a single left click
  picks nothing; a right click, and then a double click, each make the spot the destination.

`dereth/testkit/tests/cpu/plugins.rs :: the_navigator_points_at_a_place_and_walks_there` makes
coordinates the destination, marked without an arrow, and walks there.
`dereth/testkit/tests/cpu/plugins/windows.rs :: the_navigator_finds_a_place_as_it_is_typed_and_goes_there`
types into Where to? (a name, then coordinates), picks the result, reads the Destination line, goes
there by Go and reads "Arrived" on the status line, saves a place and finds it under the Saved
filter, right-clicks the map and finds the spot under the Map filter, and stops by Stop. The
headless client's
`dereth/headless/tests/dat/presentation/navigator_go.rs` runs Go in a session a player entered (a
recorded login, on the retail data): the journey sets out from where the character stands and
arrives. With the game's data,
`dereth/testkit/tests/dat/plugins.rs :: the_navigator_maps_the_dungeon_floor_around_the_character`
carries the character into a dungeon of 433 rooms, opens the Dungeon Map by its button, and
requires it to draw exactly the outline the dungeon's floors give at its level around it, worked
out again by the test, with the character's dot, once every room is read.
