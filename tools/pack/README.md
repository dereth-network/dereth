# `dereth-pack`: building content for a world

`dereth-pack` turns content packs into a world's overlay: the records a world adds to and changes
in the data files, kept beside them and never written into them. Empyrean serves an overlay to
Dereth and to the retail client; `bake` writes one into a copy of a player's own files for worlds
whose server or client reads only plain files.

```text
dereth-pack build <world.toml> --dats <dir> [--out <dir>] [--check]
dereth-pack validate --dats <dir> [--overlay <dir>]
dereth-pack bake --overlay <dir> --dats <dir> --to <dir> [--touched-only]
dereth-pack import --base <dir> --world <dir> --out <dir> --world-key <key> [--iterations revision|world] [--verify]
```

- **`build`** compiles a world's packs over the data files in `--dats` (the later set,
  `client_portal.dat` and its siblings). It writes the overlay to `<out>/overlay`, the server's SQL
  to `<out>/sql` and the world's lockfile beside `world.toml`. Nothing is written unless every
  record passes the checks. `--check` builds into a scratch folder and says whether what is there
  is current. The same sources and files give the same bytes every time.
- **`validate`** runs the checks over a set of files, with an overlay over them or without.
  Over the end-of-retail files alone every check passes.
- **`bake`** writes an overlay into copies of the files in `--dats`, in `--to`, and checks each
  copy before it takes its name. The files in `--dats` are only read. `--touched-only` copies only
  the files the overlay changes.
- **`import`** takes any modified set of files (another editor's output, a custom world's files)
  apart against its base into an overlay folder. A world can lay that folder under its packs.

The sample in [`sample/`](sample/) is a world with one pack: statics west of Holtburg, a hill on a
landblock edge, a new texture, and an NPC wearing it.

```text
cargo run -p dereth-pack -- build tools/pack/sample/world.toml --dats <your client folder>
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

## Dereth's own records

The same tool packs Dereth's client layer, the pictures the client draws over the portal files
beneath any world's overlay:

```text
cargo run -p dereth-pack -- core/client-runtime/assets/classic-portal/pack.tsv
cargo run -p dereth-pack -- <manifest> --check
```

The client builds in the classic portal's layer from `core/client-runtime/assets/classic-portal/`;
`cargo test -p dereth-pack` fails while that container is not what its manifest makes.
