# Lyceum: packs and edits

How to write a Lyceum content pack by hand: its files, the records and models it adds, its changes
to landblocks, its server content, edits written as text, and what happens when two packs
disagree. The editor writes all of this for you. For the editor, worlds and the `lyceum`
command, see [README.md](README.md).

## A pack

```text
mypack/
  pack.toml                    name, version, authors, license, description
  art/*.png                    pictures for new textures
  records/*.toml               new records: pictures, textures, surfaces, models, pieces
  edits/landblocks/XXYY.toml   changes to one landblock
  server/sql/**/*.sql          the server's content, as ACE's SQL
```

Only `pack.toml` is required; `name` is lowercase letters, digits and hyphens.

Everything a pack makes has a symbol instead of a number: `stripes` inside its own pack,
`mypack:stripes` from another. Ids are given per world when it is built.

### New records

In `records/*.toml`, each with `name` (a new record) or `replace` (an id the files already hold,
written over):

```toml
[[render-surface]]           # a picture
name = "stripes"
picture = "art/stripes.png"  # sides that are powers of two, up to 2048
format = "a8r8g8b8"          # or "r8g8b8"

[[surface-texture]]          # what a model's surface, or a texture change, names
name = "stripes"
levels = ["stripes"]         # pictures, largest first: symbols or ids

[[surface]]
name = "stripes"
type = 2                     # 2 or 4 lay a texture; otherwise give color = 0xAARRGGBB
texture = "stripes"
translucency = 0.0
luminosity = 0.0
diffuse = 1.0
```

### New models

```toml
[[model]]
name = "arch"
file = "art/arch.glb"        # .gltf with local buffers works too
collision = "mesh"           # mesh (default), cylinder, sphere, or none
# replace = 0x0200040B       # optionally replace an existing Setup
# distances = [40.0, 80.0]   # optional: one cutoff per detail level, in meters
```

Place it with `model = "arch"` in a landblock's `[[static]]`, or `"pack:arch"` from another pack.
Its parts, surfaces and pictures get stable ids too. A replacement keeps `name` for those records;
place it by its numeric Setup id.

Models are static glTF 2.0 triangle scenes in meters, Y up. The default scene is used, or the
first. Node transforms are kept, with scale baked into the vertices, and each mesh node becomes a
part. Missing normals are generated.

- **Detail levels:** name lower-detail nodes `arch_lod1`, `arch_lod2` and so on beside a base node
  `arch`, without gaps. The default cutoffs are 40, 80, 120 meters and so on, then nothing is
  drawn. Only the base level collides.
- **Collision:** a node named `arch_collision` is that part's collision mesh and is never drawn.
  `cylinder` and `sphere` enclose the drawn model; `none` makes scenery you walk through.
- **Materials:** base-color PNG pictures, embedded or in the pack, power-of-two sides up to 2048.
  Base-color factors tint pictures in linear color. Solid colors, opaque and blended alpha, the
  first UV set and double-sided materials work, and textures repeat. Other PBR channels are
  ignored.
- **Refused:** animations, skins, morphs, alpha cutoff, sparse accessors, required extensions, and
  resources outside the pack. Split meshes over 32,767 vertices or triangles per part and level.

To compare a new model's collision with its drawing, or with the model it replaces, build the world
and run:

```text
lyceum fit --dats <dir> --overlay build/overlay --model 0x02001C57 --against 0x0200040B
```

Take the new model's id from `world.lock`. `--against` always reads the original model from the
base files. The report gives the largest gap between drawing and collision, how much of the surface
is within 5 cm, and the excess collision volume (`--grid`, 4 to 64 cells per axis, 32 by default).
It also walks a player-sized sphere into both models from `--directions` approaches (4 to 128, 32
by default) and reports where each stops. The numbers are sampled, not a guarantee, and nothing in
them stops a build.

### Original dungeon pieces

A pack can add a convex indoor piece from a glTF or GLB scene:

```toml
[[piece]]
name = "hall"
file = "art/hall.glb"
```

Use meters and Y up, as for models. Walls, floor and ceiling face inward, and together with the
openings close the cell with matching edges. Name each opening's mesh `portal_0`, `portal_1` and so
on, without gaps: one flat convex shape, not drawn or collided with. Split a concave room into
convex pieces. Solid colors become small textures, and lights are placed in the room as in other
dungeons. Limits: 254 materials, 255 openings of up to 255 corners each, and under 32,768 vertices
and polygons.

The new piece shows in the editor's kit in every look, in its own materials, and is placed like any
other. The world's lockfile names its environment and surfaces, for command-line cell edits.

### Changes to a landblock

In `edits/landblocks/XXYY.toml`, one file per landblock. Positions are meters inside the landblock.

```toml
[[static]]                   # place a model
name = "stone-1"             # optional, so a later change can name it
model = 0x0200040B           # a GfxObj (0x01...) or a Setup (0x02...)
at = [186.0, 30.0, "ground"] # the height, or "ground"
heading = 90.0               # degrees clockwise from north (or rotation = [w, x, y, z])

[[move-static]]              # one of the landblock's own statics, by its index,
index = 2                    # or a pack's by name: static = "stone-1"
at = [190.0, 30.0, "ground"]

[[remove-static]]
index = 3
model = 0x02000BCD           # optional: what must be there

[[move-cell-static]]         # a static in a building's room or a dungeon cell, by the cell's
cell = 0x0105                # number (or a pack cell's name) and its index among the cell's statics
index = 2
at = [61.5, 40.2, 94.0]
heading = 90.0

[[remove-cell-static]]
cell = 0x0105
index = 3
model = 0x02000BCD           # optional: what must be there

[[add-cell-static]]          # a static added to one of the world's own rooms
cell = 0x0105
model = 0x0200040B
at = [61.5, 40.2, 94.0]
heading = 90.0

[dungeon]                    # the landblock's dungeon: its name, and the dungeon whose
name = "Drudge Warren"       # look its pieces are drawn in
look = 0x019E

[[replace-cell]]             # one of the landblock's own cells, another piece in its place;
cell = 0x0103                # it keeps its number and statics
environment = 0x0D000586
piece = 1
surfaces = [0x080000A1, 0x080000A2]
at = [0.0, -20.0, 0.0]
rotation = [1.0, 0.0, 0.0, 0.0]
portals = ["keep", ""]       # per doorway: "keep" the cell's, "" free, "outside",
                             # or "<new cell>:<doorway>"

[[remove-cell]]              # one of the landblock's own cells, with its statics
cell = 0x017D

[[cell]]                     # a new indoor cell (a room the editor set in a dungeon)
name = "hall-1"
environment = 0x0D000586
piece = 1
surfaces = [0x080000A1, 0x080000A2]
at = [0.0, -30.0, 0.0]
heading = 0.0
portals = ["outside", ""]

[[move-building]]            # one of the world's own buildings, by its index; its rooms,
index = 0                    # doors and the spawns inside go with it
at = [60.0, 40.0, 94.0]
heading = 90.0

[[add-building]]             # a copy of a retail building, with its rooms
name = "cottage-1"           # its rooms are cells named cottage-1-room-000 and on
model = 0x01002228           # a set that stands together lists its models and templates
template = "3B9C/3"          # the retail copy: its landblock, and its place among the buildings there
at = [61.3, 79.8, 78.0]      # its origin; the height, or "ground"
heading = 180.0
furnished = "bare"           # or another copy's furniture, "3DC7/1"; left out, the template's
house = "cottage-1-house"    # optional: a house players can buy, owned by this spawn

[[add-dungeon]]              # a copy of a retail dungeon, every cell of its landblock
name = "villa-1-basement"
template = 0x0398            # the retail dungeon's landblock
house = "villa-1-basement-house"  # optional: its cells belong to this spawn

[[vertex]]                   # one vertex, or every vertex from at to to
at = [4, 3]                  # 0 to 8 each
to = [5, 4]
height = 80                  # the height table's index, 0 to 255
terrain = 1                  # 0 to 31
scene = 0                    # 0 to 31
road = 0                     # 0 to 3
```

A dropped building is an exact copy of its template: the same rooms, ways and lists, moved to where
it stands and numbered after the landblock's own cells. A building whose origin is outside its
landblock, or in a land cell another building stands in, stops the build: the client draws one
building per land cell.

A vertex on a landblock's edge is shared with its neighbors, and the build changes every copy. Two
packs that set the same vertex differently stop the build, naming both files.

### The server's content

ACE's SQL, one file per object as ACE's `export-sql` writes it, with `{{kind:symbol}}` wherever an
id goes:

| Token | Becomes |
|---|---|
| `{{wcid:guide}}` | the weenie's class id (decimal) |
| `{{guid:guide@A9B4}}` | an instance's id in landblock A9B4 (hex) |
| `{{surface-texture:stripes}}` | a record's id (hex); also `render-surface`, `surface` |

Write a spawn as one instance (`DELETE FROM landblock_instance WHERE guid = ...`), not a whole
landblock, so the landblock's other instances stay. `lyceum template --world <sql> --class <n>
--name <symbol>` starts a new weenie's SQL from an existing class. The built SQL goes to Empyrean
(`empyrean-import --patches <out>/sql`) or into ACE's content folder.

## Edits as text

What `lyceum edit --edits` reads, and what the editor does:

```toml
[[edit]]
do = "place-static"              # also move-static, remove-static
landblock = "A9B4"
model = 0x0200040B
at = [30.0, 60.0, "ground"]
heading = 45.0

[[edit]]
do = "move-static"
landblock = "A9B4"
static = "stone-west"            # a pack's static by its symbol, or: index = 3 (the base's)
at = [20.0, 26.0, "ground"]

[[edit]]
do = "brush"                     # raise, lower, flatten, smooth, terrain, scene, road
brush = "raise"
amount = 3                       # raise and lower; flatten, terrain, scene and road take value
landblock = "A9B4"
at = [0.0, 72.0]                 # the center, landblock meters
radius = 30.0

[[edit]]
do = "spawn"
weenie = "guide"                 # a pack's weenie by its symbol, or a class id
landblock = "A8B4"
at = [170.0, 36.0, "ground"]
heading = 270.0

[[edit]]
do = "move-building"             # one of the world's own buildings, by its index
landblock = "A9B4"
index = 0
at = [60.0, 40.0, 94.0]
heading = 90.0

[[edit]]
do = "add-building"              # a copy of a retail building, from the building catalog
model = 0x01002228
landblock = "A9AD"
at = [61.3, 79.8]                # its origin, landblock meters
heading = 180.0
layout = 0                       # left out, the model's most common layout
furnished = "bare"               # or another copy's furniture, "3DC7/1"; left out, its own
house = false                    # a house as a plain building; left out, one players can buy
```

An edit changes the pack's files in place, keeping everything it doesn't touch, comments included.
A new static gets a name (`static-1`) so later edits can find it. A vertex goes into the file that
already sets it, else the landblock where it is 0 to 7 from the corner. A spawn is a file of its
own, `server/sql/landblocks/A8B4 guide.sql`.

A dropped building comes with its doors from the world database, the steps and porches most of its
copies have (statics named after it, `cottage-1-outdoor-0`), level ground at the height under its
origin, and the scenery inside it cleared. A drop that reaches further into the next land cell than
any retail copy of its model gets a warning. Moving a dropped building rewrites its
`[[add-building]]` row; removing it removes the row, and the ground goes back to what the world and
earlier packs make it.

## When packs disagree

Packs that change different things merge. Two that set one thing differently are a conflict, and
`build` lists every conflict, with each pack, file and line, and writes nothing. The editor lists
them with its checks and refuses an edit that would make a new one.

| Item | What |
|---|---|
| `record 0x0E000123` | a record of the files, replaced |
| `vertex 1354,1440 height` | one field of a terrain vertex (`height`, `terrain`, `scene`, `road`) |
| `static A9B4 12`, `static A9B4 mypack:post` | a static, the files' by index or a pack's by symbol |
| `building A9B4 2` | a building of the files, moved |
| `cell 019E 0103` | a cell of the files, replaced by another piece |
| `cell-static A9B4 0105 2` | a static in a cell, by its place among the cell's own |
| `weenie 1234` | a weenie class, compared row by row (its date aside) |
| `spawn 0x7A9B4001` | an instance and its links |
| `quest drudgekills` | a quest's registration |

A world says which pack wins:

```toml
[[override]]
pack = "other"                             # the pack that wins
over = "mypack"                            # the pack it wins over
items = ["weenie 1234", "landblock A9B4"]  # what it wins; without a list, everything both set
```

`landblock A9B4` covers that landblock's vertices, statics, buildings and spawns. Where overrides
in both directions cover one item, the most specific wins: the item, then its landblock, then an
override with no list. Two files of one pack that disagree must be fixed in the pack.
