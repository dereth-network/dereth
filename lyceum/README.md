# Lyceum

Lyceum is the Dereth Network's world builder. Its editor draws a world as the Dereth client draws
it and saves every change as the source of a content pack. The Foundry compiles those packs into a
world that Empyrean serves, or exports it for ACE or GDLE. The `lyceum` command runs the Foundry
from a terminal. (Maila was Master of the Lyceum at Knorr, where makers learned to forge with
pyreal in the foundries.)

| Folder | Package | What it is |
|---|---|---|
| [`editor/`](editor/) | `lyceum-editor` | the editor |
| [`crates/foundry/`](crates/foundry/) | `lyceum-foundry` | the Foundry: the pack format, the compiler and its checks, the edits, the indoor cells, bake and import |
| [`cli/`](cli/) | `lyceum-cli` | the `lyceum` command |
| [`sample/`](sample/) | | a one-pack world to start from |

Lyceum is built on the shared crates and the client's drawing. It runs Empyrean as a separate
program when you play a world.

## Getting started

A release has the editor, the `lyceum` command, and Empyrean and its import tool beside them. The
Dereth Network launcher can download it for you and manage your worlds (its Lyceum page).

Start the editor with no world to get its start screen: recent worlds, open a world, a new world
from the sample, and the client folder worlds are drawn over. Or name a world:

```text
lyceum-editor <world.toml> [--dats <client folder>] [--classic-dats <folder>] [--landblock <XXYY>]
cargo run -p lyceum-editor -- lyceum/sample/world.toml --dats <your client folder>
```

A world built on Infiltration's files is drawn from `portal.dat` and `cell.dat`, beside the
client's files or in the folder `--classic-dats` names.

To play a world, Play and Live need its world database. Download it once with the button at the
top (you can keep editing meanwhile). It goes into Empyrean's per-user cache, and an existing one
is reused. Development builds use the server programs built beside the editor, or the folder
`--empyrean` names.

The editor keeps its settings in its own folder (`Dereth/lyceum` beside the client's). It reopens
where you left off: the window, the panel layout (`lyceum-layout.json`), and each world's camera,
tool and dungeon (`.lyceum-place.toml` beside the world), unless the command line names a
landblock. To change the words it shows, put a `lyceum-text.toml` with just the lines you want in
its settings folder; `lyceum-editor --write-text` writes every line to start from, and
`--overwrite` replaces an existing file.

## The editor

The world view takes the left three quarters of the window. On the right are the Tool, Map,
Nearby, Hierarchy and Dungeons tabs, and below them Inspector, Definition (the selected weenie)
and Log (the checks' findings and every status message). Choosing a tool brings the Tool tab
forward. Any tab can be dragged elsewhere.

The top bar has Select, Place, Spawn and Terrain; Undo and Redo; Play, Live and Settings; Save;
and the flying speed. The camera keeps two speeds, 10 m/s indoors and 100 m/s outdoors, each as you
last set it with the buttons or the wheel. Settings picks the playtest's interface, walking or
flying, the key hints and the UI scale. The status bar shows the landblock and position, the
Collision and View switches, and the unsaved edits or when you last saved.

With Collision on, the camera stops at the ground, walls and objects, as the client's camera does,
but passes through doors. Ctrl+D (or Duplicate) copies the selection beside itself.

### Weenies

Definition shows the selected weenie without changing it: its kind and whose it is, its fields in
plain words, what it carries, casts, spawns and says, where it appears, and every property, folded
at the bottom. Click Edit, or double-click a value, to open it in the Editor tab. The Editor
floats over the world view, docks anywhere and closes with its ×. It stays on its weenie while the
selection moves, with Back and Forward through the weenies it has shown, and its menu makes new
weenies or puts one back as saved. Editing one of the server's weenies asks first: change it in
this world, so every copy changes, or make a copy of your own.

### Placing scenery

The scenery browser has Recent and Favorites shelves, a search box, a filter by kind of model and
one by your own tags. The star on a tile adds or removes a favorite, and right-clicking a tile
edits its tags; these are saved in `.lyceum-browser.toml` beside the world. Click to place the model
on the ground, or on top of whatever is under the pointer.

### Buildings

Buildings are one of the browser's kinds: every building the world places, most common first,
with houses named Cottage, Villa and Mansion. A building is dropped whole, with its rooms and doors.
Its card picks the layout, where there are two, the furniture (like a town's copy, or bare), and
for a house whether players can buy it.

The ghost snaps to where the retail copy stands in its land cell (Shift places it freely, R turns it
a quarter). The land cell is outlined green where the drop is allowed and red where it isn't, with
the reason. A mansion drops with the three buildings it always stands with. The ground under a drop
is leveled and the scenery inside it cleared. Moving or deleting a dropped building takes its
doors, steps, porches, contents and level ground with it, as one step.

### Dungeons

Dungeons are a mode. Click one in the Dungeons tab to go in, at the place its portals drop you. New
dungeon starts one on the open sea floor, near a landblock you type or near the camera. Leave goes
back. Inside, the world view draws only the dungeon. The Grid tab shows it one floor at a time, and
the Cross-section under the world view shows its floors from the south. Both zoom with the wheel
and pan with a right drag. The world's own dungeons sit on a grid of 10 m squares, in quarter turns,
with floors 6 m apart.

The dungeon's card heads the Tool tab: its name, Leave and Delete, and a new name and look (the set
of textures several of the world's dungeons share). With Select, the look's pieces are listed under
it, each a whole room, drawn as a floor plan and grouped by shape: dead ends, corridors, corners, T
junctions, crossings, rooms, stairs and ramps, shafts and ceilings. A pack's own pieces show in
every look.

Click a free doorway or an empty square on the grid to list the pieces that fit there, best first.
Hovering one shows its ghost on the grid and in the world view; click to set it. Or choose a piece
and click a green square (R turns it). Delete removes the selected room with what's in it, freeing
its doorways.

Choosing another look restyles the whole dungeon. A new name renames the portals that lead in too;
quests, books and dialogue keep the old name. Deleting a dungeon removes its rooms, their contents
and the portals to it, after you confirm.

Anything you place in a room belongs to it and moves with it. A door set by a doorway stands in it,
facing through; anything else lands on the floor, or against the wall clicked. A portal's card
sends it into a dungeon or to the camera's position.

### Play and Live

Start Live, then Play to take control of a character at the edit camera, in the editor's own window.
F8 goes back to editing at the same camera while the character stays connected; Escape still opens
the game's menus. Play resumes the character, and stopping Live ends the session.

While Live runs, each edit is published when the drag or brush stroke ends, and Undo publishes too.
The server and the client take the same numbered overlay, and only the affected landblocks are
rebuilt. A publish that fails reports its error and restores the previous content.

The playtest character's template is `characters/<name>.json` beside the world. A new character
uses it on first Play; an existing one keeps its progress. The server keeps its databases in
`.playtest/world` beside the world (`--live-dir` picks another), and a second editor can't open the
same playtest folder while its server runs.

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

`lyceum new <folder>` makes a world from the sample; `--empty` makes an empty one and
`--era infiltration` an Infiltration one.

A world is built on one era's files and refused over another's. An Infiltration world is built over
`portal.dat` and `cell.dat` from February 2005, in those files' own layout, and its content goes
over that era's world database. A bake, and so an ACE or GDLE export, writes only end-of-retail
files.

New ids are taken above the highest the files hold in each range. Weenie class ids start at
`[ids] wcid` (100,000 by default, above every class id in the end-of-retail world database), and
instance ids count down from the top of their landblock's range. `world.lock` keeps every id it
gives, so a rebuild gives the same ones: commit it with the world.

A world is a list of content packs, each a folder of text and pictures: new records and models,
changes to landblocks, and the server's content as ACE's SQL. [PACKS.md](PACKS.md) is the
reference for their format, for edits written as text, and for what happens when two packs set the
same thing differently (an `[[override]]` in `world.toml` says which wins).

### Upgrading

World and pack sources declare `format_version = 1`; unversioned sources still read. `lyceum
upgrade <world.toml> --check` previews an upgrade, and `lyceum upgrade <world.toml>` does it,
keeping comments and content. It validates every pack first and saves each original beside it as
`*.toml.before-upgrade` (and `world.lock.before-upgrade`). Assigned ids don't change. A source in
a newer format needs a newer Lyceum.

## The `lyceum` command

```text
lyceum new <folder> [--empty] [--era eor|infiltration]
lyceum upgrade <world.toml> [--check]
lyceum build <world.toml> --dats <dir> [--out <dir>] [--after <overlay>] [--check]
lyceum validate --dats <dir> [--overlay <dir>] [--sql <file or folder>] [--world <sql>] [--schema <json>]
lyceum edit <world.toml> --dats <dir> --edits <file> [--pack <name>] [--weenies <sql>] [--dry-run]
lyceum edit <world.toml> --dats <dir> --weenies <sql> add-building <model> <landblock> <x> <y> <heading>
            [--layout <n>] [--furnished <XXYY/n> | --bare] [--plain] [--pack <name>] [--dry-run]
lyceum template --world <sql> --class <n> --name <symbol> [--blank] [--out <file>]
lyceum bake --overlay <dir> --dats <dir> --to <dir> [--touched-only]
lyceum export <world.toml> --dats <dir> --for empyrean|ace|gdle --out <dir>
              [--era eor|infiltration] [--world-sql <dump>] [--empyrean <dir>] [--gdle-quests <file>]
lyceum import --base <dir> --world <dir> --out <dir> --world-key <key> [--iterations revision|world] [--verify]
lyceum facts <world.toml>
lyceum layer <manifest> [--out <container>] [--check]
lyceum fit --dats <dir> --model <id> [--overlay <dir>] [--against <id>] [--grid <n>] [--directions <n>]
lyceum --version
```

It exits 0 on success, 1 when `build --check` finds the build stale or `validate` finds a fault,
and 2 for a usage or input error.

- **`build`** compiles a world's packs over the era's data files in `--dats`. It writes the overlay
  (the records the world adds and changes, kept beside the data files, never in them) to
  `<out>/overlay`, the server's SQL to `<out>/sql`, and the lockfile beside `world.toml`. Nothing is
  written while there are conflicts or a record fails the checks. `--check` builds into a scratch
  folder and says whether the output is current. The same sources and files always give the same
  bytes. `--after` names the overlay the world was last published with, so the new one is its next
  revision and a server patches players with only what changed.
- **`validate`** runs the checks over a set of files, with or without an overlay. `--sql` also
  checks that the records an SQL file or folder names are in the files. A static placed inside a
  building is a warning.
- **`edit`** applies text edits ([PACKS.md](PACKS.md)) to a world's pack (`--pack`, the last
  by default) and saves them as its sources, as the editor would. `--weenies` names ACE SQL for
  the server's weenies. `--dry-run` saves nothing. `add-building` drops one building from the
  command line; `--plain` makes a house a building players can't buy.
- **`bake`** writes an overlay into copies of the files in `--dats`, in `--to`, checking each copy.
  `--touched-only` copies only the files the overlay changes. The originals are only read.
- **`export`** writes a world out for another server (below).
- **`import`** takes any modified set of data files (another editor's output, a custom world's
  files, of either era) apart against their base, into an overlay folder a world can list under
  `imports`.
- **`facts`** prints what a world changes as one line of JSON, from its sources alone: its key,
  era and packs, the landblocks with terrain edits, the statics, buildings, dungeons and rooms, the
  spawns and weenies, and the new pictures and models.
- **`layer`** packs Dereth's own client layer (below).
- **`fit`** compares a new model's collision with its drawing ([PACKS.md](PACKS.md), New models).
- **`template`** starts a new weenie's SQL from an existing class in the world database.

Try it on the sample, a world with one pack (statics west of Holtburg, a hill on a landblock edge,
a new texture and an NPC wearing it):

```text
cargo run -p lyceum-cli -- build lyceum/sample/world.toml --dats <your client folder>
```

## Exporting a world

`lyceum export` writes a world out for another server, into a folder that is new, empty or an
earlier export. A `README.txt` there says where each part goes and what players need.

| For | What it writes |
|---|---|
| Empyrean | `world.pack`, the era's world database with the world's content in it; `overlay/`, the world's changes to the data files; `sql/`, the world's content |
| ACE | `dats/`, the changed data files, baked; `sql/`, the world's content laid out as ACE's content folder |
| GDLE | `dats/`, as for ACE; `json/`, the world's weenies and spawn maps as GDLE's files; `quests.json`, the quests the world adds or changes |

Empyrean and GDLE exports run `empyrean-import` (from `--empyrean`, or beside `lyceum`) over the
era's world database: `--world-sql`, else the newest in the download cache, downloaded if there is
none. `--era` picks the end of retail's (`eor`, the default) or Infiltration's. Retail clients read
only plain files, so their players need the baked data files; Empyrean patches Dereth clients
itself.

GDLE reads all its quests from one `quests.json`. Give the server's own with `--gdle-quests` and
the export merges the world's quests into it, listing in its README which quests were added,
replaced and removed. Without it, `quests.json` holds only the world's quests, and the README says
how to merge them.

## Dereth's own records

The same tool packs Dereth's client layer, the pictures the client draws over the portal files
beneath any world's overlay:

```text
cargo run -p lyceum-cli -- layer core/client-runtime/assets/classic-portal/pack.tsv
cargo run -p lyceum-cli -- layer <manifest> --check
```

The client builds the classic portal's layer in from `core/client-runtime/assets/classic-portal/`,
and `cargo test -p lyceum-foundry` fails while that container doesn't match its manifest.

The opt-in connected-play test runs with `--features live-server-tests --test gpu`, with
`EMPYREAN_SERVER_BIN` naming the built server (its importer beside it), `EMPYREAN_WORLD_SQL` the
world SQL dump, and `DERETH_TEST_DAT_DIR` the data files.
