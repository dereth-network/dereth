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

Its settings, and `lyceum-text.toml`, every word it shows, which you can change, are kept in its
own settings folder (`Dereth/lyceum` beside the client's).

The dungeon kit's pieces are every piece the world's own cells are built of, each shown in a
dungeon that uses it. They group and filter by look (the textures a set of dungeons share, named
after the dungeons drawn so), by shape (room, corridor, junction, stairs, dead end), by the
dungeons that use them and by the pieces the world's dungeons join a chosen piece to. Inside a
dungeon, its cells are listed with their pieces; a cell can be replaced by another piece that
keeps its doorways, and built on from the doorways the new piece adds. With Collide on, the flying
camera stops at the ground, walls and things, as the client's own camera does.

## The Foundry and the `lyceum` command

The Foundry turns content packs into a world's overlay: the records a world adds to and changes
in the data files, kept beside them and never written into them. Empyrean serves an overlay to
Dereth and to the retail client; `bake` writes one into a copy of a player's own files for worlds
whose server or client reads only plain files.

```text
lyceum build <world.toml> --dats <dir> [--out <dir>] [--check]
lyceum validate --dats <dir> [--overlay <dir>] [--sql <file or folder>]
lyceum edit <world.toml> --dats <dir> --edits <file> [--pack <name>] [--weenies <sql>] [--dry-run]
lyceum bake --overlay <dir> --dats <dir> --to <dir> [--touched-only]
lyceum import --base <dir> --world <dir> --out <dir> --world-key <key> [--iterations revision|world] [--verify]
```

- **`build`** compiles a world's packs over the data files in `--dats` (the later set,
  `client_portal.dat` and its siblings). It writes the overlay to `<out>/overlay`, the server's SQL
  to `<out>/sql` and the world's lockfile beside `world.toml`. Nothing is written while two packs
  set one thing differently (below), or unless every record passes the checks. `--check` builds
  into a scratch folder and says whether what is there is current. The same sources and files give
  the same bytes every time.
- **`validate`** runs the checks over a set of files, with an overlay over them or without.
  Over the end-of-retail files alone every check passes. `--sql` also checks that the records an
  SQL file or folder names (setups, motion tables, palettes, parts, textures, cells) are in the
  files. A static an overlay places inside a building is a warning, which stops nothing.
- **`edit`** applies the edits in `--edits` to a world's pack (`--pack`, the last by default) and
  saves them as that pack's sources: the same edits the world editor makes, written as text
  (below). `--weenies` names ACE SQL (an export or a dump) for the server's own weenies' setups;
  `--dry-run` saves nothing.
- **`bake`** writes an overlay into copies of the files in `--dats`, in `--to`, and checks each
  copy before it takes its name. The files in `--dats` are only read. `--touched-only` copies only
  the files the overlay changes.
- **`import`** takes any modified set of files (another editor's output, a custom world's files)
  apart against its base into an overlay folder. A world can lay that folder under its packs.

The sample in [`sample/`](sample/) is a world with one pack: statics west of Holtburg, a hill on a
landblock edge, a new texture, and an NPC wearing it.

```text
cargo run -p lyceum-cli -- build lyceum/sample/world.toml --dats <your client folder>
```

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

[[replace-cell]]             # one of the landblock's own cells with another piece in its place
cell = 0x0103                # it keeps its number, its statics and every doorway it had
environment = 0x0D000586
piece = 1
surfaces = [0x080000A1, 0x080000A2]
at = [0.0, -20.0, 0.0]
rotation = [1.0, 0.0, 0.0, 0.0]
portals = ["keep", ""]       # per doorway of the piece: "keep" one the cell had, "" free,
                             # "outside", or "<new cell>:<doorway>"

[[vertex]]                   # one vertex, or every vertex from at to to
at = [4, 3]                  # 0 to 8 each
to = [5, 4]
height = 80                  # the height table's index, 0 to 255
terrain = 1                  # 0 to 31
scene = 0                    # 0 to 31
road = 0                     # 0 to 3
```

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

An edit changes the pack's files in place: the entries it does not touch, and their comments,
stay as they were. A base static is moved or removed by its index, a pack's by its symbol, and a
new one gets a name (`static-1`) so later edits can find it. A vertex goes into the file that
already sets it, else into the landblock where it is 0 to 7 from the corner. A spawn is a file of
its own, `server/sql/landblocks/A8B4 guide.sql`.

## A world

```toml
# world.toml
key = "my-world"             # the world's key; the overlay records it
packs = ["mypack", "other"]  # the load order, folders relative to this file
imports = []                 # overlay folders made by import, laid under the packs
out = "build"                # where build writes, relative to this file
[ids]
wcid = 100000                # where new weenie class ids start
```

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
