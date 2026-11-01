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

| Folder | Package | What it is |
|---|---|---|
| [`crates/foundry/`](crates/foundry/) | `athenaeum-foundry` | the world machinery: a world over the database read, checked, built into the server's tables against an install, counted, and edited row by row with undo and redo |
| [`cli/`](cli/) | `athenaeum-cli` | the `athenaeum` command: worlds made, built, checked and counted; an install's data files read and counted |
| [`editor/`](editor/) | `athenaeum-editor` | the editor: the world drawn as the client draws it, its rows marked where they stand, edited, saved, and played on Yalain |

## Running it

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
- **Serve** starts Yalain on this machine serving the world as saved; **Play** starts our client
  into it, standing where the camera is (`--retail <client.exe>` adds **Play retail**). Saving
  while it serves starts Yalain again on what was saved.

The top of `editor/src/main.rs` lists every option, and `editor/src/script.rs` the steps a
`--script` plays.

## Testing

Its tests that need no data run with the workspace's (`cargo xtask check`, `cargo xtask ci tier0`).
Its data test, a world over the database built against the end-of-retail install, is in
Auberean's `auberean-dat` tier (`cargo xtask ci auberean`); it reads the database from
`YALAIN_TEST_WORLD_DB`, else the pinned release in the cache.
