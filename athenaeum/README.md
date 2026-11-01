# Athenaeum

Athenaeum is the Dereth Network's Asheron's Call 2 world builder. A world is a folder of packs laid
over the Asheron's Call 2 world database (`yalain-world`, which [Yalain](../yalain/README.md)
serves): where people, objects and camps stand, where portals lead, and the rest of the server's
rows. A world's rows replace the database's by key and take them away with `[[remove]]`, and Yalain
serves the two together, so even the retail client sees what a world places.

It is early work: the editor places, moves, turns, changes and takes away people, objects, camps
and places, outdoors. It shares Asheron's Call 2's core (`core/auberean-*`) with
[Auberean](../auberean/README.md) and Yalain, draws the world with Auberean's drawing crates, and
runs Yalain as a program.

## Getting it and running it

You need your own Asheron's Call 2 install: the end-of-retail data files `portal.dat`,
`cell_1.dat` and `local_English.dat`. Athenaeum reads them and never writes them; nothing of the
game ships with it, and nothing of it is downloaded. The world database a world lies over is
fetched once into your user cache; the editor fetches it when a world needs it.

**From the launcher.** In the Dereth Network launcher, pick Auberean's portal above the rail and
open Athenaeum's page: download Athenaeum, choose your Asheron's Call 2 folder, then New World
(made in an `Athenaeum` folder in your Documents) or Open World. The editor opens on it.

**By hand.** Each release has a file for your system: `athenaeum-<version>-windows-x86_64.zip`
(unzip it anywhere), `Athenaeum-<version>-macos-<arch>.app.tar.gz` (unpack it and move
`Athenaeum.app` to Applications) or `Athenaeum-<version>-linux-x86_64.AppImage` (make it
executable; give it a program's name first to run that program, as in `Athenaeum-...AppImage
athenaeum new <folder>`). Each carries the editor, the `athenaeum` command, and Yalain and
Auberean, which the editor serves and plays a world with:

```text
yalain world fetch
athenaeum new <folder>
athenaeum-editor <folder> --dats <install folder>
```

`athenaeum --version` and `athenaeum-editor --version` say which build they are.

| Folder | Package | What it is |
|---|---|---|
| [`crates/foundry/`](crates/foundry/) | `athenaeum-foundry` | the world machinery: a world over the database read, checked, built into the server's tables against an install, counted, and edited row by row with undo and redo |
| [`cli/`](cli/) | `athenaeum-cli` | the `athenaeum` command: worlds made, built, checked and counted; an install's data files read and counted |
| [`editor/`](editor/) | `athenaeum-editor` | the editor: the world drawn as the client draws it, its rows marked where they stand, edited, saved, and played on Yalain |

## Running it from the repository

You need your own Asheron's Call 2 install, and the world database Yalain pins, fetched once into
your user cache (`yalain world fetch`).

```text
cargo run --profile test-release -p athenaeum-cli -- new <folder>
cargo run --profile test-release -p athenaeum-editor -- <folder> --dats <install folder>
cargo run --profile test-release -p athenaeum-cli -- validate <folder> --dats <install folder>
cargo run --profile test-release -p athenaeum-cli -- build <folder> --dats <install folder>
cargo run --profile test-release -p yalain -- --dats <install folder> --world-db <folder>
```

`athenaeum new` makes a world over the database with one empty pack, `fill-<name>`, which the
editor's changes go into. `validate` checks a world without writing anything (with `--dats`, it also
compiles it against the install); `build` writes the server's tables as text into the world's
`build` folder for reading; `facts` counts what a world changes. `--base <folder>` uses a checkout
of the database in place of the cached release. The data commands (`info`, `ls`, `verify`, `show`,
`census` and the rest) only read: an install is never written to. `athenaeum` with no arguments
lists every command.

### The editor

The world is drawn as the client draws it, from a free camera: hold the right button to turn it,
W, A, S, D to fly, E and Q to rise and sink, the wheel to change speed. Every row near the camera is
marked where it stands, in its kind's color with the letter of where it came from: **C** recorded
sessions, **D** the data files, **R** a rule, **A** a person. People and objects stand as their
templates look, a camp's creature at its middle with its reach drawn round it, and a portal's way
to where it leads.

- Click a marker (or a row in the list on the left) to select it; its fields are on the right.
- Drag a marker to move its row over the ground; R turns it (Shift+R the other way); Delete takes
  it away; F flies to it.
- **Place**, with a kind and a template along the top, puts a new row at the next click on the
  ground.
- A row of the database is never written: changing one writes this world's own row of its key,
  shown as changed, with what it was and **Put back**; taking one away writes a `[[remove]]`.
- Ctrl+S saves, Ctrl+Z undoes, Ctrl+Y redoes. The world's problems are listed along the bottom as
  the server's check finds them.

The top of `editor/src/main.rs` lists every option, and `editor/src/script.rs` the steps a
`--script` plays.

### Live and Play

**Live** starts Yalain on this computer alone (127.0.0.1, on ports nothing else holds), serving
the world as it is in the editor, saved or not. While it runs, each change is sent to it once you
let go of what you changed, and Undo too. Yalain takes at once what can change with players in
the world (what people say, stores, loot, quests and more); for people, objects and camps it
starts again, the character staying where it stands. Turn that off in Characters to start it
again only when you press **Restart**. Live keeps its files in `.playtest` in the world's folder,
and a second editor can't play the same world while the first does.

**Play** starts our client logged in as the chosen character, where its recipe starts it
(`--retail <client.exe>` adds **Play retail**). Stopping Live stops both; Yalain keeps the
character where it stands, as it does when its operator stops it.

### Characters

**Characters** lists the world's playtest characters, one recipe each in `characters/<Name>.toml`
beside `world.toml`: its race, sex, where it starts (at the camera, in front of the selected row,
where it left the world, or at one of the world's places), and whether it is a hero. Its first
play makes it; after that it plays on as it was left, with what it found and learned, until its
race or sex changes or **Reset** makes it from its recipe again. **Delete** takes the recipe and
its character away. Each character is kept in a store of its own in the playtest folder.

## Testing

Its tests that need no data run with the workspace's (`cargo xtask check`, `cargo xtask ci tier0`).
Its data test, a world over the database built against the end-of-retail install, is in
Auberean's `auberean-dat` tier (`cargo xtask ci auberean`); it reads the database from
`YALAIN_TEST_WORLD_DB`, else the pinned release in the cache. Its playtest test, a world played
live as a character on Yalain with our client, both run as programs, is in the `auberean-live`
tier, which builds them first.
