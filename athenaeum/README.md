# Athenaeum

Athenaeum is the Dereth Network's Asheron's Call 2 world builder. A world is a folder of packs laid
over the Asheron's Call 2 world database (`yalain-world`, which [Yalain](../yalain/README.md)
serves): where people, objects and camps stand, where portals lead, and the rest of the server's
rows. A world's rows replace the database's by key and take them away with `[[remove]]`, and Yalain
serves the two together, so even the retail client sees what a world places.

It is early work: the editor places, moves, turns, changes and takes away people, objects, camps
and places, outdoors and in rooms and dungeons. It shares Asheron's Call 2's core (`core/auberean-*`) with
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
editor's changes go into, and a `.gitignore` that keeps what playing and building it leave
(`.playtest`, `build`) out of a repository. `validate` checks a world without writing anything
(with `--dats`, it also compiles it against the install); `build` writes the server's tables as
text into the world's `build` folder for reading; `facts` counts what a world changes.
`--base <folder>` uses a checkout of the database in place of the cached release. The data
commands (`info`, `ls`, `verify`, `show`, `census` and the rest) only read: an install is never
written to. `athenaeum` with no arguments lists every command.

### The editor

The world is drawn as the client draws it, from a free camera: hold the right button to turn it,
W, A, S, D to fly, E and Q to rise and sink, the wheel to change speed. Every row near the camera is
marked where it stands, in its kind's color with the letter of where it came from: **C** recorded
sessions, **D** the data files, **R** a rule, **A** a person. People and objects stand as their
templates look, a camp's creature at its middle with its reach drawn round it, and a portal's way
to where it leads.

- Click a marker, what a row looks like (or a row in the list on the left) to select it; its fields are on the right.
- Fly into a building or a dungeon, or Find a row in one and click it, and the rooms are drawn
  around the camera as the client draws them. Placing and dragging go by the floor under the pointer,
  and a row keeps track of the room it is in.
- The selected row has handles: red, green and blue arrows move it east, north and up, and the
  ring turns it. **Snap** along the top makes moves and turns go by steps, R landing on them too;
  Shift while dragging does the other.
- Drag a marker to move its row over the ground; R turns it (Shift+R the other way); Delete or
  Backspace takes it away; F flies to it.
- **Place**, with a kind and a template along the top, shows the new row under the pointer and puts
  it at the next click: on the ground, or on the stones, platforms, buildings and objects
  standing there. R turns it first; Shift+click keeps placing; Esc stops.
- Shift+click adds a row to the selection. Then dragging, the handles, R, Duplicate and Delete take
  them all together, keeping how they stand to each other, and one Undo takes it back.
- A place's **key** on the right renames it, and what leads to it follows.
- An object's **Lead to a place** list makes it a portal: this world's places first, then the
  nearest others. A camp's creatures are listed by name, with how often each comes and **Add** for
  the template along the top.
- **Find** shows the templates as pictures, by name or id. Click one and click the ground to place
  it; Shift+click keeps the window open for the next, and it opens again on the same search.
  Things to carry stay out of the way until **Items** is ticked, or **Worn**, which shows what can
  be worn on a body. **no picture** is something that shows only as effects, which play once it stands in the world.
- **Map** or M shows the whole world, this world's rows on it and the camera as a cross. Click
  to go there.
- Ctrl+F or **Find rows** finds any row of the world by its name, template, kind or key, nearest
  or not, and goes to a landblock (`5766` or `5766:123,45`) or a room (`03BB022C`). Enter takes
  the first thing it offers.
- G stands the selected rows on what is under them; Shift+G also leans objects with the ground.
- Ctrl+C keeps the selected rows, and Ctrl+V puts copies down at the pointer, in any landblock,
  standing as they stood to each other.
- `=` and `-` make the selected objects larger and smaller, and **scale** on the right sets it; an
  empty one goes back to their template's.
- Ctrl+D or **Duplicate** copies the selected row beside it and selects the copy, whatever it is:
  a person with their name and place, a camp with its creatures, a portal with where it leads.
- A row of the database is never written: changing one writes this world's own row of its key,
  shown as changed, with what it was and **Put back**; taking one away writes a `[[remove]]`.
- Ctrl+S saves, Ctrl+Z undoes, Ctrl+Y redoes (Cmd on macOS, as well). The world's problems are listed along the bottom as
  the server's check finds them.
- T sends the character playing on Live to the camera.

The top of `editor/src/main.rs` lists every option, and `editor/src/script.rs` the steps a
`--script` plays.

### Live and Play

**Live** starts Yalain on this computer alone (127.0.0.1, on ports nothing else holds; taken by
another program as it starts, it moves on to the next), serving the world as it is in the editor,
saved or not. While it runs, each change is sent to it once you let go of what you changed, and
Undo too. Yalain takes every change at once with players in the world: what people say, stores,
loot, quests, people, objects, camps, veins and more. **Restart** starts it again, the character
staying where it stands. Live keeps its files in `.playtest` in the world's folder, and a second
editor can't play the same world while the first does. An editor that closes or crashes takes its
Yalain with it, which keeps every character where it stands.

**Play** starts our client logged in as the chosen character, where its recipe starts it
(`--retail <client.exe>` adds **Play retail**), starting Live first when it is off. One Yalain
plays every character: choosing another, or changing the one you play, never restarts it. When
our client plays that character already, it is sent to its start through portal space; otherwise
the character is made, given its build or put at its start before it enters (a character still in
the world is taken out of it first, at once). **To camera** (or T) sends the character in
the world to the camera. Stopping Live stops both; Yalain keeps the character where it stands, as
it does when its operator stops it.

### Characters

**Characters** lists the world's playtest characters, one recipe each in `characters/<Name>.toml`
beside `world.toml`: its race, sex, where it starts (at the camera, in front of the selected row,
where it left the world, or at one of the world's places), whether it is a hero, and what it has
reached: a level, money, skills raised to a rating, items in its pack, and quests done or under
way. Its first play makes it as a player reaching that would be; Yalain refuses a build no player
could hold, and says why. After that it plays on as it was left, with what it found and learned.
A recipe whose build changes gives it what changed at its next play, keeping what it found and
learned: its level and skills are raised (never lowered), items it lacks are added, and its money
and quests are set as the recipe now says. A new race or sex makes it anew, and so does **Reset**,
at once when Live runs. **Delete** takes the recipe and its character
away. **Export** writes the character to `build/characters/<Name>.json` in the world's folder, and
**Import** brings a character from such a file (another world's) in as a new playtest character,
with a recipe of what it holds: its level, money and hero, the skills it trained with their
ratings, its items and its quests, starting where it stood. Every character lives in one store in
the playtest folder.

## Testing

Its tests that need no data run with the workspace's (`cargo xtask check`, `cargo xtask ci tier0`).
Its data test, a world over the database built against the end-of-retail install, is in
Auberean's `auberean-dat` tier (`cargo xtask ci auberean`); it reads the database from
`YALAIN_TEST_WORLD_DB`, else the pinned release in the cache. Its playtest tests, a world played
live as characters on Yalain with our client, both run as programs, a Yalain moved past ports
another program holds, and one stopped by the end of its console, are in the `auberean-live` tier,
which builds them first.
