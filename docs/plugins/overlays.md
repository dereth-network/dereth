# Markers

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

A plugin marks things in the world rather than in a window with **markers**. There are four
kinds, each a host capability:

| kind | capability | what it is |
|---|---|---|
| world | `overlay.world` | a mark and a label on an object or a place, where it falls in the view |
| radar | `overlay.radar` | a blip on the radar |
| map | `overlay.map` | points, or a line through them, on the map |
| arrow | `overlay.arrow` | one direction indicator per plugin, toward an object or a place, with its distance |

```lua
local m = markers.world:add{ object = id, label = "Signpost", color = "#00FF00" }
markers.radar:add{ position = { ns = 42.25, ew = 33.8 }, label = "Camp" }
markers.map:add{ points = { { ns = 42.2, ew = 33.8 }, { ns = 42.3, ew = 33.9 } }, line = true }
markers.arrow:set{ object = id, label = "Signpost" }  -- replaces this plugin's arrow
m:remove()
markers.world:clear()  -- this plugin's world markers; markers.clear() takes down all of its markers
```

A world, radar or arrow marker marks an `object` (an id or an object handle) or a `position` (a
place, see [places](nav.md#places)), never both. A map marker has `points`, and `line = true` joins
them. Every marker may have a `label`, a `color` (`"#RRGGBB"` or `0xRRGGBB`; each kind has its own
default) and `expires`, seconds after which it goes by itself; a world or radar marker may name an
`icon` from the game's data. A misspelt field is an error.

The host keeps each plugin's markers (at most 256) and resolves them each frame: a marker on an
object is where the object is, and is not drawn while the object is out of sight. A plugin's
markers go when it unloads. The arrow's direction is relative to the character's heading, and its
distance is from the character.

**How each host draws them.** The desktop and browser clients draw a world marker as a dot and its
label where the place falls in the game view, hidden where the world stands in front of it unless
the marker says `on_top = true`; a radar marker as a blip on the radar (heading up, at
the radar's range), a map marker on the map when the map is open, and the arrow at the top of the
screen as a ring with its lit point toward the target, with the label and distance under it. The
dot, the labels and the arrow are drawn under every interface, and the radar and map marks on each
interface's own radar and map. The headless client draws nothing and offers none of the four; the conformance driver keeps them and
lists them, one a line:

```text
world <plugin> #<id> label="…" color=#RRGGBB object=0x… at=42.20N 33.90E
arrow <plugin> #<id> … turn=<degrees clockwise from ahead> distance=<metres>
map <plugin> #<id> … line=[42.20N 33.80E, 42.30N 33.90E]
```

A place indoors is listed by cell and offset (`at=0x01D90108 10.0 20.0 0.0`).

**Clicks on the map.** Where the host offers `map.clicks`, a plugin hears each press on the game's
world map as `map.clicked` ([events](events.md#clicks-on-the-map)), its `position` in the
coordinates a map marker takes: a map marker there is drawn on the pixel pressed, and the arrow set
there points at the spot clicked.

## Drawing in the world

With `overlay.world` a plugin also draws **shapes** in the 3-D view, and a **nametags** layer:

```lua
draw.line{ object = game.player.id, to_object = target, color = "#FF0000", width = 3, label = "Target" }
draw.ring{ object = target, radius = 3 }          -- a circle on the ground about it
draw.square{ position = { ns = 42.2, ew = 33.8 }, radius = 2 }   -- radius: centre to side
draw.text{ position = place, label = "Camp", height = 2 }        -- text floating there
draw.nametags{ players = true, monsters = true, health = true, within = 40, limit = 30 }
draw.nametags(nil)   -- takes the layer down
draw.clear()         -- this plugin's shapes
```

A shape is anchored as a world marker is (`object` or `position`); a line also names its other
end (`to_object` or `to_position`). `radius` and `height` (above the anchor) are metres, `height`
0.1 by default and 2 for text; `width` is the line's width in pixels, 1 to 6 (2 by default). A
shape is a marker like any other: it counts toward the 256, takes `label`, `color` and `expires`,
answers a handle with `:remove()`, and goes when the plugin unloads. The host resolves each shape
every frame to its outline in the world (a line's two ends, a ring's 32 segments, a square's
corners), where its object is now; a shape on an object out of sight is not drawn.

The **nametags** layer (one a plugin; a new one replaces the old) labels every object of the kinds
it names (`players`, `monsters`, `npcs` with vendors, `portals`) within `within` metres (35 by
default, 250 at most), the nearest first and at most `limit` of them (20 by default, 100 at most),
with its name, and with its health in percent where the client knows it (`health = true`: a fellow,
a creature it has appraised, the selected object). A tag fades out evenly over the last 10 metres of
the range (over its last half, for a range under 20 metres).

**What stands in front.** Shapes, text, world markers and nametags are part of the scene: a wall, a
building or a hill between the camera and a mark hides it. A line or an outline is hidden where it
passes behind something; a text, a label or a nametag is tested once, at the point it stands on, and
is shown or hidden whole, never cut. A shape, a world marker or a nametags layer with
`on_top = true` is drawn over everything instead, for a mark that must stay in sight.

The desktop and browser clients draw an outline in the 3-D view itself, under every interface,
after the world and tested against its depth: a band facing the camera that keeps its width in
pixels at any distance, lifted by half that width so a ring on the ground sits on it. Text,
labels and nametags are drawn where
their place falls on the screen, under the windows and panels, each shown only when nothing in the
drawn world stands nearer the camera at its place (a nametag's place counts as 3 metres nearer, so
the thing it names does not hide it); the test reads the frame already drawn, so a label follows a
frame or two behind what hides or uncovers it. So that a crowd stays readable, the labels
are placed nearest first, a plugin's own text before nametags: a label that would lie mostly under
ones already placed is left out, and one that only clips them is drawn at half strength. The
conformance driver lists them:

```text
shape <plugin> #<id> … at=42.20N 33.90E shape=ring path=33 span=0.6
nametag <plugin> #<id> label="Drudge Skulker" color=#FFFFFF object=0x… at=42.20N 33.90E
```

(`path` is how many points the outline has, `span` the metres between its first two; a mark drawn
on top ends its line with `on_top`.)
