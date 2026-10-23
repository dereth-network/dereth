# Lyceum: building worlds

Lyceum is the world builder: an editor that draws a world as the client draws it and saves every
change as the sources of a content pack, the Foundry that compiles those packs into a world, and
the `lyceum` command that runs the Foundry from a terminal. Maila was Master of the Lyceum at
Knorr, where makers learned to forge with pyreal in the foundries.

| Folder | Package | What it is |
|---|---|---|
| [`editor/`](editor/) | `lyceum-editor` | the editor, a window on the client's own drawing |
| [`crates/foundry/`](crates/foundry/) | `lyceum-foundry` | the Foundry: the pack format, the compiler and its checks, the edits, brushes and picks, the indoor cells and what each sees, bake and import |
| [`cli/`](cli/) | `lyceum-cli` | the `lyceum` command |
| [`sample/`](sample/) | | a world of one pack, to start from |

Lyceum stands beside Dereth and Empyrean and depends on neither's application: it is built on the
shared crates and the client's drawing, and runs Empyrean as a program of its own when a world is
played.

## The editor

```text
cargo run -p lyceum-editor -- lyceum/sample/world.toml --dats <your client folder>
```

A world built on Infiltration's files is drawn from `portal.dat` and `cell.dat`: beside the client's
files, or in the folder `--classic-dats` names. Its server is that era's, over its world database.

Its settings are kept in its own settings folder (`Dereth/lyceum` beside the client's). To change
the words it shows, put a `lyceum-text.toml` there with only the lines you want changed; the rest
stay the editor's own. `lyceum-editor --write-text` writes every line there to start from (or to the
file named after it), and `--overwrite` replaces a file already there.

It opens where you left it: the window where it was (a 1080p window the first time), its panels
as you arranged them (in `lyceum-layout.json`), and each world with its camera, tool and dungeon as they were (kept beside
the world in `.lyceum-place.toml`), unless the command line names a landblock.

The world view takes the left three quarters of the window. On the right, the Tool tab shows what
the tool works with, beside Map, Nearby, Hierarchy and Dungeons, sized to fit the map; under them
are Inspector, Definition (the selected weenie's) and Log. Choosing a tool brings the Tool tab
forward. Log holds the checks' findings, under a header that folds, and every message the status
bar has shown. Any tab can be dragged elsewhere; a layout from an older Lyceum is replaced by this
one.

The bar along the top has Select, Place, Spawn and Terrain, then Undo and Redo, then Play, Live
and Settings, then Save, each an icon named by its tooltip, and last the flying speed. The camera
keeps two speeds, 10 m/s indoors (in a dungeon or a room) and 100 m/s outdoors, each as you last
changed it with its buttons or the wheel. Settings chooses the playtest's interface, walking or
flying, the key hints and the UI scale. The status bar shows the landblock and position (in a
dungeon, the dungeon's landblock), Collision (on to start with), View (the landscape grid, never
drawn in a dungeon, and the selection's bounds), and the edits not yet saved or when the world was
last saved. The map opens on the whole world, square and in the middle of its panel; the wheel
zooms in from there.

The scenery browser has Recent and Favorites shelves, a search box, a dropdown of the kinds of
model to show, as many as you tick (Clear shows every kind), and one for your tags. Choose a model to add it to Recent; the star in a tile's
corner adds or removes a favorite, and a right click on a tile gives or takes its tags. These
shelves are saved beside the world in `.lyceum-browser.toml`. A click places the model on the
ground, or on top of the static under the pointer.

Buildings are a kind of the dropdown: every building the world places, the most placed first,
houses named Cottage, Villa and Mansion. A building chosen is dropped whole, with its rooms and
doors. Its card chooses its layout, where it has two, and its furniture: like a town's copy, or
bare. Its ghost follows the pointer, snapped where its retail copy stands in its land cell (Shift
places it freely, R turns it a quarter); the land cell is outlined green where the drop is
allowed and red where it is not, with the reason, and what the drop warns of beside it. A
mansion drops as its four. Moved or turned, a dropped building takes its doors, its steps and
porches, what stands in its rooms and its level ground along, as one step; Delete takes it away
with all of that.

Dungeons are a mode. A click on one in the Dungeons tab goes in, at the place its portals
drop you; New dungeon starts one on the open sea floor, near a landblock you type or near the
camera; Leave goes back where you were. Until you leave, the world view draws the dungeon alone,
all of it when the camera flies outside its rooms; the Map and Dungeons tabs give way to the Grid
tab, the dungeon one floor at a time about the camera (the floor the camera is on, or one you
choose until the camera goes to another), and the Cross-section under the world view shows its
floors from the south. Both zoom with the wheel and pan with a right drag. The world's own dungeons stand on a grid of 10 m squares, turned in quarter turns,
with floors 6 m apart.

The dungeon's card heads the Tool tab: its name with Leave and Delete, and folded under them a
new name and its look (the textures a set of the world's dungeons share). With Select, under it
are the look's pieces, each a whole room set down at once,
drawn as a floor plan (its floors, outline, doorways, and openings up and down) and grouped,
each group folding, by what it is on the grid: dead ends, corridors, corners, T junctions, crossings, rooms by their
squares, stairs and ramps, shafts and ceilings. Every piece adds the rest of the world's pieces,
drawn in this look by what each surface covers; a pack's own pieces are offered in every look, in
their own materials.

Click a free doorway's dot or an empty square on the grid to list what fits there: the look's
pieces that meet every doorway around it, best first (those that join the most doorways, then
those the world's dungeons join to the neighbors most). The pointer over one shows its ghost on
the grid and, textured, in the world view; a click sets it in. Or choose a piece and click a
green square (R turns it). In the world view a click only selects. Delete takes the selected room
away whole, the world's own too, with what stands in it, as one step; the doorways into it are
free to build on again.

Another look chosen restyles the whole dungeon: each room as the world draws that piece in the
look, where it does, else each surface by what it covers (floor, ceiling, walls, trim). A new name
renames the portals that lead in too, the world's own by the pack's copy of their weenie; quests,
books and what characters say keep the old name. Delete takes it away with its rooms, what stands
in them and the portals to it, once you say yes.

What you place in a room belongs to it: the room's card lists it, with its place in the room, and
moving or removing the room carries it along. A door set by a doorway stands in it, facing
through; anything else lands on the floor, or against the wall clicked. Ctrl+D (or Duplicate)
copies what is selected beside itself and selects the copies. Portals are added like any object;
a portal's card sends it into a dungeon or to where the camera is. With Collision on, the flying
camera stops at the ground, walls and things, as the client's own camera does, but passes
through doors, open or closed; put inside something it cannot fly out of, it passes through
until it is clear, as a player does after a portal.

Releases include Empyrean and its import tool beside the editor. Play and Live use that
server. On first use, choose **Download world database**; the editor downloads it into
Empyrean's per-user cache while you keep editing. An existing cached database is reused.
Development builds use server programs built beside the editor, or the `--empyrean` folder.

## The Foundry and the `lyceum` command

World and pack sources declare `format_version = 1`. Unversioned sources remain readable.
`lyceum upgrade <world.toml> --check` previews changes without writing.
`lyceum upgrade <world.toml>` updates the world and its packs, preserving comments and content.
It validates every pack first and saves each original beside its source as
`*.toml.before-upgrade`, plus `world.lock.before-upgrade` when a lockfile exists.
Assigned IDs in `world.lock` stay unchanged. A newer source format
requires a newer Lyceum and is refused before editing or building.

The Foundry turns content packs into a world's overlay: the records a world adds to and changes
in the data files, kept beside them and never written into them. Empyrean serves an overlay to
Dereth and to the retail client; `bake` writes one into a copy of a player's own files for worlds
whose server or client reads only plain files.

```text
lyceum build <world.toml> --dats <dir> [--out <dir>] [--after <overlay>] [--check]
lyceum validate --dats <dir> [--overlay <dir>] [--sql <file or folder>]
lyceum edit <world.toml> --dats <dir> --edits <file> [--pack <name>] [--weenies <sql>] [--dry-run]
lyceum edit <world.toml> --dats <dir> --weenies <sql> add-building <model> <landblock> <x> <y> <heading>
            [--layout <n>] [--furnished <XXYY/n> | --bare] [--pack <name>] [--dry-run]
lyceum bake --overlay <dir> --dats <dir> --to <dir> [--touched-only]
lyceum export <world.toml> --dats <dir> --for empyrean|ace|gdle --out <dir>
              [--era eor|infiltration] [--world-sql <dump>] [--empyrean <dir>] [--gdle-quests <file>]
lyceum import --base <dir> --world <dir> --out <dir> --world-key <key> [--iterations revision|world] [--verify]
lyceum facts <world.toml>
```

- **`build`** compiles a world's packs over the data files in `--dats` of the era the world is
  built on: the end of retail's (`client_portal.dat` and its siblings) or Infiltration's
  (`portal.dat` and `cell.dat`), each written in its own layout. It writes the overlay to `<out>/overlay`, the server's SQL
  to `<out>/sql` and the world's lockfile beside `world.toml`. Nothing is written while two packs
  set one thing differently (below), or unless every record passes the checks. `--check` builds
  into a scratch folder and says whether what is there is current. The same sources and files give
  the same bytes every time. `--after` names the overlay the world was last published with: the
  new overlay is then its next revision, so a server patches a player who has the last one with
  what changed alone (a record that reads as it did keeps its revision, and one the world no
  longer has is deleted in the new one).
- **`validate`** runs the checks over a set of files, with an overlay over them or without.
  Over the end-of-retail or the Infiltration files alone every check passes. `--sql` also checks that the records an
  SQL file or folder names (setups, motion tables, palettes, parts, textures, cells) are in the
  files. A static an overlay places inside a building is a warning, which stops nothing.
- **`edit`** applies the edits in `--edits` to a world's pack (`--pack`, the last by default) and
  saves them as that pack's sources: the same edits the world editor makes, written as text
  (below). `--weenies` names ACE SQL (an export or a dump) for the server's own weenies' setups;
  `--dry-run` saves nothing. `add-building` is one edit given on the command line: a copy of a
  retail building (below), with its doors from the world database in `--weenies`.
- **`bake`** writes an overlay into copies of the files in `--dats`, in `--to`, and checks each
  copy before it takes its name. The files in `--dats` are only read. `--touched-only` copies only
  the files the overlay changes.
- **`export`** builds a world and writes it out for a server that runs it elsewhere (below).
- **`import`** takes any modified set of files (another editor's output, a custom world's files,
  of either era) apart against its base into an overlay folder. A world can lay that folder under
  its packs.
- **`facts`** prints what a world changes as one line of JSON, read from its sources alone: its
  key and era, its packs, the landblocks whose terrain it edits, the statics, buildings,
  dungeons and rooms it adds or changes, the spawns and weenies of its server content, and the
  pictures and models it brings.

The sample in [`sample/`](sample/) is a world with one pack: statics west of Holtburg, a hill on a
landblock edge, a new texture, and an NPC wearing it.

```text
cargo run -p lyceum-cli -- build lyceum/sample/world.toml --dats <your client folder>
```

## Exporting a world

`lyceum export` writes a world out for another server. The folder
it writes into is new, empty, or an earlier export, and it holds a `README.txt` saying where each
part goes and what players need.

| for | what it writes |
|---|---|
| Empyrean | `world.pack`, the era's world database with the world's content in it; `overlay/`, what the world changes in the data files; `sql/`, the world's content |
| ACE | `dats/`, the data files the world changes, baked; `sql/`, the world's content laid out as ACE's content folder |
| GDLE | `dats/`, as for ACE; `json/`, the world's weenies and spawn maps as GDLE's files, and `quests.json`, the quests the world adds or changes |

Empyrean and GDLE exports run Empyrean's `empyrean-import`, from the folder `--empyrean` names or
beside `lyceum`, over the era's world database: `--world-sql`, else the newest in its download
cache, downloaded when there is none. `--era` chooses the world database, the end of retail's
(`eor`, the default) or Infiltration's. A retail client reads only plain files, so its players
need the baked data files; Empyrean patches a Dereth client itself.

GDLE reads all its quests from one `quests.json`. `--gdle-quests` names the server's own: the export's `quests.json` is then that list with the
world's quests merged in, and the README names the quests the world adds and those whose entries
it replaces. A quest the world leaves as the world database has it keeps the server's entry, and
one the world removes loses it; the README lists those under a heading of their own. Without it,
`quests.json` holds the world's own quests, and the README says how to merge them.

## Original dungeon pieces

A pack can add a convex indoor piece from a glTF or GLB scene:

```toml
[[piece]]
name = "hall"
file = "art/hall.glb"
```

Use meters and glTF's Y-up axes, as for models. Walls, floor and ceiling face inward.
Name opening meshes `portal_0`, `portal_1`, and so on without gaps. Each opening is
one flat convex boundary triangulated without holes. Its triangles mark the opening;
they are not drawn or collided with. The walls and opening boundaries must close the
cell with matching edges. Split a concave room into several convex pieces.

The kit lists the new piece even before it is placed, as its floor plan, in every look and
in its imported materials. Place or join it like any other piece.
The world's lockfile names its environment and surfaces for command-line cell edits.
Each imported scene supplies piece `0` of its own environment. Solid colors become
small textures for indoor walls. Place lights in the room as in other dungeons. Keep
the generated opening surface when supplying a custom surface list. A piece has at
most 254 materials and 255 openings, with at most 255 corners per opening. Keep each
piece below 32768 vertices and polygons.

## A pack

```text
mypack/
  pack.toml                    name, version, authors, license, description
  art/*.png                    pictures for new textures
  records/*.toml               new records: pictures, image textures, surfaces
  edits/landblocks/XXYY.toml   changes to one landblock: statics and terrain
  server/sql/**/*.sql          the server's content, as ACE's SQL
```

Only `pack.toml` is required; `name` is lowercase letters, digits and hyphens.

**Names, not numbers.** Everything a pack makes has a symbol: `stripes` inside its own pack,
`mypack:stripes` from another. Ids are given per world when it is built, and the world's
`world.lock` keeps them, so a rebuild gives the same ones. Commit the lockfile with the world.

**New records**, in `records/*.toml`, each with `name` (a new record) or `replace` (an id the
files already hold, written over):

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

**New models**, also in `records/*.toml`:

```toml
[[model]]
name = "arch"
file = "art/arch.glb"        # .gltf with local buffers works too
collision = "mesh"          # mesh (default), cylinder, sphere, or none
# replace = 0x0200040B      # optionally replace an existing Setup
# distances = [40.0, 80.0]  # optional: one cutoff per detail level, in meters
```

Place it with `model = "arch"` in a landblock's `[[static]]`, or use `"pack:arch"`
from another pack. Its parts, surfaces and pictures receive stable world ids too.
For a replacement, keep `name` for those supporting records and place the numeric Setup id.

Import static glTF 2.0 triangle scenes in meters. The scene's default scene is used, or its
first scene. Coordinates `(x, y, z)` become `(x, -z, y)`: Y up becomes Z up. Node translation,
rotation, scale and parent transforms are preserved; scale is baked into the vertices.
Each mesh node becomes a part with its own placement frame. Missing normals are generated.

Name lower-detail nodes `arch_lod1`, `arch_lod2`, and so on beside a base node named `arch`.
Levels must be consecutive; the default cutoffs are 40, 80, 120 meters and so on, followed by
no drawing. Each part's levels use the base part's frame. Only the base level collides.
A node named `arch_collision` supplies that part's mesh collision and is never drawn.
Cylinder and sphere collision enclose the drawn model; `none` gives scenery without collision.

Base-color PNG pictures become the pack's picture, texture and surface records. Pictures may
be embedded or in the pack, with power-of-two dimensions up to 2048. Base-color factors tint
pictures in linear color space. Solid colors, opaque and blended alpha, the first UV set and
double-sided materials are supported. Textures repeat. Other PBR channels are not represented.
Animations, skins, morphs, alpha cutoff, sparse accessors and required extensions are refused.
Resources must stay inside the pack. Split meshes exceeding 32767 vertices or triangles per
part and level; a separate collision mesh shares the base part's vertex limit.

**Collision fit**, after building the world:

```text
lyceum fit --dats <dir> --overlay build/overlay --model 0x02001C57 --against 0x0200040B
```

Use the new model's id from `world.lock`. `--against` is optional and always reads the original
model from the base files, even when both ids are the same. The candidate reads through the overlay.
The report gives the largest sampled gap from drawing to collision and how many surface samples
are within 5 cm. It uses seven samples per triangle; these are sampled bounds, not a guarantee
about every point on the surface. Excess collision volume is estimated at grid-cell centers, with
closed drawn parts treated as a union. Open or non-manifold meshes report volume as unknown.
`--grid` chooses 4 to 64 cells per axis (32 by default).

The comparison walks a 0.48-meter-radius sphere through both models on flat ground, with the
client's transition code. `--directions` chooses 4 to 128 approaches (32 by default). Each approach
reports stopping outcomes and endpoint separation, including sliding and stepping. A stop means
at least 25 cm less forward progress than the empty control. Models extending more than 30 meters
horizontally from their origin are refused by this bounded walk test. The report is diagnostic;
a reported difference does not prevent building or replacing a model.

**Changes to a landblock**, in `edits/landblocks/XXYY.toml`, one file per landblock:

```toml
[[static]]                   # place a model
name = "stone-1"             # optional, so a change can name it later
model = 0x0200040B           # a GfxObj (0x01...) or a Setup (0x02...)
at = [186.0, 30.0, "ground"] # meters inside the landblock; the height, or "ground"
heading = 90.0               # degrees clockwise from north (or rotation = [w, x, y, z])

[[move-static]]              # one of the landblock's own statics, by its place in its list,
index = 2                    # or a pack's, by name: static = "stone-1"
at = [190.0, 30.0, "ground"]

[[remove-static]]
index = 3
model = 0x02000BCD           # optional: what must be there

[[move-cell-static]]         # a static standing in a building's room or a dungeon's cell,
cell = 0x0105                # by the cell's number (or a pack's cell by its name)
index = 2                    # and its place among the cell's own statics
at = [61.5, 40.2, 94.0]      # meters inside the landblock
heading = 90.0

[[remove-cell-static]]
cell = 0x0105
index = 3
model = 0x02000BCD           # optional: what must be there

[[add-cell-static]]          # a static added to one of the world's own rooms
cell = 0x0105
model = 0x0200040B
at = [61.5, 40.2, 94.0]      # meters inside the landblock
heading = 90.0

[dungeon]                    # the landblock's dungeon: its name, and the look of the world's
name = "Drudge Warren"       # dungeon whose look its pieces are drawn in
look = 0x019E

[[replace-cell]]             # one of the landblock's own cells with another piece in its place
cell = 0x0103                # it keeps its number and statics; dropped doorways produce warnings
environment = 0x0D000586
piece = 1
surfaces = [0x080000A1, 0x080000A2]
at = [0.0, -20.0, 0.0]
rotation = [1.0, 0.0, 0.0, 0.0]
portals = ["keep", ""]       # per doorway of the piece: "keep" one the cell had, "" free,
                             # "outside", or "<new cell>:<doorway>"

[[add-building]]             # a copy of a retail building, with its rooms
name = "cottage-1"           # its rooms are cells named cottage-1-room-000 and on
model = 0x01002228           # a set that stands together lists its models and templates
template = "3B9C/3"          # the retail copy: its landblock and its place in that landblock's buildings
at = [61.3, 79.8, 78.0]      # where its origin stands; the height, or "ground"
heading = 180.0
furnished = "bare"           # or another copy's furniture, "3DC7/1"; left out, the template's

[[vertex]]                   # one vertex, or every vertex from at to to
at = [4, 3]                  # 0 to 8 each
to = [5, 4]
height = 80                  # the height table's index, 0 to 255
terrain = 1                  # 0 to 31
scene = 0                    # 0 to 31
road = 0                     # 0 to 3
```

A dropped building is the template's copy: the same rooms, ways and lists, carried to where it
stands, numbered after the landblock's own cells, and belonging to no house. A building whose
origin is outside its landblock, or in a land cell another building stands in, stops the build:
the client draws only one building in each land cell.

A vertex on a landblock's edge is shared with the landblock beside it (and at a corner with
three); the build changes every copy. A static in a landblock that had none gets the landblock its
statics record. Two packs that set the same vertex differently stop the build, naming both files.

**The server's content** is ACE's SQL, one file per object as ACE's `export-sql` writes it, with
`{{kind:symbol}}` wherever an id goes:

| token | becomes |
|---|---|
| `{{wcid:guide}}` | the weenie's class id (decimal) |
| `{{guid:guide@A9B4}}` | an instance's id in landblock A9B4 (hex) |
| `{{surface-texture:stripes}}` | a record's id (hex); also `render-surface`, `surface` |

Write a spawn as one instance (`DELETE FROM landblock_instance WHERE guid = ...`), not a whole
landblock, so the landblock's other instances stay. The built files go to Empyrean
(`empyrean-import --patches <out>/sql`) or to ACE's content folder.

## Edits as text

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
do = "brush"                     # raise, lower (amount), flatten, terrain, scene, road (value), smooth
brush = "raise"
amount = 3
landblock = "A9B4"
at = [0.0, 72.0]                 # the center, landblock meters
radius = 30.0

[[edit]]
do = "spawn"
weenie = "guide"                 # a pack's weenie by its symbol, or a class id
landblock = "A8B4"
at = [170.0, 36.0, "ground"]
heading = 270.0
```

```toml
[[edit]]
do = "add-building"              # a copy of a retail building, from the building catalog
model = 0x01002228
landblock = "A9AD"
at = [61.3, 79.8]                # its origin, landblock meters
heading = 180.0
furnished = "bare"               # or another copy's furniture, "3DC7/1"; left out, its own
```

A dropped building comes from the world's own: its most common layout, its doors as the world
database stands them, and the steps and porches most of its copies have (statics named after it,
`cottage-1-outdoor-0`). The ground under it is made level at the height under its origin, and
scenery that would stand inside it is cleared (scenery is chosen by vertex, so a cleared vertex
grows nothing around it either). A mansion drops with the other three buildings it always stands
with. A drop that reaches further into the next land cell than any retail copy of its model is
warned of. A dropped building is moved by writing its `[[add-building]]` row anew, and taken
away with it; the ground it stood on goes back to what the world (and the packs before) make it,
but where another drop stands. A `[[move-building]]` row moves only the world's own buildings.

An edit changes the pack's files in place: the entries it does not touch, and their comments,
stay as they were. A base static is moved or removed by its index, a pack's by its symbol, and a
new one gets a name (`static-1`) so later edits can find it. A vertex goes into the file that
already sets it, else into the landblock where it is 0 to 7 from the corner. A spawn is a file of
its own, `server/sql/landblocks/A8B4 guide.sql`.

## A world

```toml
# world.toml
key = "my-world"             # the world's key; the overlay records it
era = "eor"                  # the files it is built on: eor (default) or infiltration
packs = ["mypack", "other"]  # the load order, folders relative to this file
imports = []                 # overlay folders made by import, laid under the packs
out = "build"                # where build writes, relative to this file
[ids]
wcid = 100000                # where new weenie class ids start
```

A world is built on one era's files and refused over another's. An Infiltration world is
built over `portal.dat` and `cell.dat` from February 2005: its records are written as those files
hold them (a new texture's pixels in the texture itself, 16 bits a pixel), its content goes over
that era's world database, and Empyrean serves it. A bake, and so an export for ACE or GDLE,
writes only the end-of-retail files. `lyceum new <folder> --era infiltration` makes one.

New ids are taken above the highest the files hold in each kind's range (pictures, image textures,
surfaces), weenie class ids from `[ids] wcid` (100,000 by default, above every class id of the
end-of-retail world database), and instance ids from the top of their landblock's range down.

### When packs disagree

Packs that change different things merge. Two that set one thing differently are a conflict:

| item | what |
|---|---|
| `record 0x0E000123` | a record of the files, replaced |
| `vertex 1354,1440 height` | one field of a terrain vertex (`height`, `terrain`, `scene`, `road`) |
| `static A9B4 12`, `static A9B4 mypack:post` | a static, the files' by its index or a pack's by its symbol: moved, removed or placed |
| `building A9B4 2` | a building of the files, moved |
| `cell 019E 0103` | a cell of the files, replaced by another piece |
| `cell-static A9B4 0105 2` | a static standing in a cell, by its place among the cell's own: moved or removed |
| `weenie 1234` | a weenie class, compared row by row (its date aside) |
| `spawn 0x7A9B4001` | an instance and its links |
| `quest drudgekills` | a quest's registration |

`build` lists every conflict at once, with each pack, file and line and what each sets, and writes
nothing. The editor lists them with its checks, and refuses an edit that would make a new one. A
world says which pack wins:

```toml
[[override]]
pack = "other"                             # the pack that wins
over = "mypack"                            # the pack it wins over
items = ["weenie 1234", "landblock A9B4"]  # what it wins; without a list, everything both set
```

`landblock A9B4` is everything of that landblock: its vertices, statics, buildings and spawns.
Where overrides both ways cover one item, the one that names it most closely wins (the item, then
its landblock, then a list-less override). The build takes the winner's, leaves the other's SQL
for that item out of its own, and lists what it overrode. Two files of one pack that disagree are
put right in the pack.

## Dereth's own records

The same tool packs Dereth's client layer, the pictures the client draws over the portal files
beneath any world's overlay:

```text
cargo run -p lyceum-cli -- layer core/client-runtime/assets/classic-portal/pack.tsv
cargo run -p lyceum-cli -- layer <manifest> --check
```

The client builds in the classic portal's layer from `core/client-runtime/assets/classic-portal/`;
`cargo test -p lyceum-foundry` fails while that container is not what its manifest makes.

### Play in the editor

Start Live, then choose Play to take control of a character at the edit camera.
The world view expands while playing. F8 returns to the same edit camera; Escape
continues to belong to the game's menus. The character stays connected while you
edit, and Play resumes it. Stopping Live ends that session.

The bundled server keeps the local character databases under `.playtest/world`
beside the world file. Opening the world again preserves them. A second editor
cannot start the same playtest folder while its server is running. Play uses the
editor's window and graphics device; no separate client installation is needed.

### Playtest characters

Play takes control of the playtest character, whose template is kept in `characters/<name>.json`
beside the world file. A new character uses its template on first Play; an existing one keeps
its progress.

### Editing a running playtest

Press F8 to return to editing while the character stays connected. Live publishes edits after
a drag or brush stroke ends. The server and embedded client adopt the same numbered overlay;
terrain and cell edits rebuild only their affected landblocks. Shared model changes also rebuild
resident blocks so their geometry cannot retain an old model. Play returns to the same character.
Undo publishes another revision in the same way.

Publication is serialized and waits for the server acknowledgement before refreshing the client.
A failed publication reports its error and restores the previous content; if recovery fails,
the owned server stops. Old overlays remain available for recovery. Every server start rebuilds
generated content in a fresh run folder while preserving the world's character databases.

The opt-in connected-play check uses `--features live-server-tests --test gpu` with
`EMPYREAN_SERVER_BIN` naming the built server (its importer beside it), `EMPYREAN_WORLD_SQL`
naming the world SQL dump, and `DERETH_TEST_DAT_DIR` naming the data files.
