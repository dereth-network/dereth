# Yalain

Yalain is the Dereth Network's Asheron's Call 2 server: it speaks to the final retail client,
ac2client 00.00.00.0930 (30 September 2013), and to [Auberean](../auberean/README.md). It shares
Asheron's Call 2's core (`core/auberean-*`) with the client and the world builder, and Dereth's
transport.

It is early work: accounts and characters, the character list, entering the world, other players in
view, chat, portals and lifestones, and a starting area to play in, with creatures that spawn, fight,
die and drop loot (some of them shooting from afar), experience and levels, skills to train and
raise, death and resurrection, a store, the start area's quests, and people who say their lines.
The world it serves is the Asheron's Call 2 world database (`yalain-world`): where people, objects
and camps stand, what drops and is sold, how quests move on, what people say, each row saying where
it came from, compiled at start against your install.

## Getting it and running it

You need your own Asheron's Call 2 install: the end-of-retail data files `portal.dat`,
`cell_1.dat` and `local_English.dat`. Nothing of the game ships with Yalain, and nothing of it is
downloaded; the world database is, once, into your user cache.

**From the launcher.** In the Dereth Network launcher, pick Auberean's portal above the rail and
open Yalain's page: download Yalain, choose your Asheron's Call 2 folder, and press Start. The
launcher fetches the world database, runs Yalain on your computer alone (logins on port 9200), and
shows its log; Play on the Play page starts it when it is not running and logs you in.

**By hand.** Each release has an archive for your system, `yalain-<version>-<target>.zip` on
Windows and `.tar.gz` elsewhere (Linux x86-64 and arm64 with glibc 2.28 or newer, macOS 11 or
newer). Unpack it anywhere, then:

```text
yalain world fetch
yalain --dats <install folder>
```

It listens on 127.0.0.1, logins on port 9000 and the world on 9002, and keeps its accounts and
characters in `yalain.sqlite` where it was started (`--data` names another file). `yalain --help`
lists every option; `yalain --version` says which build it is.

| Folder | Package | What it is |
|---|---|---|
| [`server/`](server/) | `yalain` | the `yalain` program: settings, the data it reads at start, the sockets and the loop |
| [`crates/world/`](crates/world/) | `yalain-world` | the game: its state and rules, sans-IO |
| [`crates/net/`](crates/net/) | `yalain-net` | the server's connections over the shared transport, sans-IO, with an in-memory network, UDP and the WebSocket endpoint |
| [`crates/store/`](crates/store/) | `yalain-store` | accounts and characters, in SQLite or in memory |
| [`crates/content/`](crates/content/) | `yalain-content` | the tables the world plays from: objects, camps, loot, stores, quests, what people say |
| [`crates/world-db/`](crates/world-db/) | `yalain-world-db` | the world database: read, checked, merged and compiled into those tables |
| [`testkit/`](testkit/) | `yalain-testkit` | a whole server on a virtual clock with clients that speak the wire, and the replay of recorded logins |

## Running it from the repository

You need your own Asheron's Call 2 install, one no retail client is running from, and the world
database this build pins, fetched once into your user cache.

```text
cargo run --profile test-release -p yalain -- world fetch
cargo run --profile test-release -p yalain -- --dats <install folder>
cargo run --profile test-release -p yalain -- --help
```

It listens on 127.0.0.1 by default (`--bind` to change it). `--world-db <folder>` serves a checkout
of the database (or a world of your own packs) instead, and a world [Athenaeum](../athenaeum/README.md)
made over the database (its `[base]`) is served over the release it names in the cache, or the
folder `--world-base` gives; `yalain world check` and `yalain world compile` check one and write the
tables it compiles to.

To pick up edits to the database without a restart, type `reload` into the running server's
console. It compiles the database again (the `--world-db` folder, or the pinned release) and
takes it while players are in the world: templates, loot, stores, quests, what people say, the
story's scenes, effects and crafting, the `--banned-names` file, the areas of player conflict,
placed objects, camps and veins (`SETUP.md` section 6 says how players see them change); the log
says which tables were reloaded. A database that does not compile leaves the world as it was, and
the log says why. `help` lists the console's commands.

`--banned-names <file>` refuses new character names holding the words it lists: one pattern a
line, `*` for any run of letters, matched against a name's letters in small letters. The list is
empty by default; which words the 2017 servers refused is not known.

## Listing it on the Dereth Network

A world can be listed on a Dereth Network hub, where the launcher's Auberean realm shows it with
how many are playing. Register it with the hub once, as an Asheron's Call 2 world (`"game":
"auberean"`), with its address and its login port as the endpoint: on the hub's site, or by the
hub's operator (`dereth-network-hub register world.json`). Keep the key the hub prints in a file of
its own, then start the server with:

```text
yalain --dats <install folder> --hub https://hub.dereth.network --hub-slug <slug> --hub-key-file network.key
```

It posts a heartbeat to the hub at start, once a minute, at once when a shutdown starts, and as it
stops, each naming the world's game, so a world registered without one is put right by its first
heartbeat. A key, slug or hub address it cannot use stops it at start, saying why; a hub that
refuses a heartbeat or cannot be reached is logged, and the world plays on. Plain `http://` is for
a hub on the same machine. Only the listing is carried: tells and friends between worlds are not.

## Testing

Its tests that need no data run with the workspace's (`cargo xtask check`, `cargo xtask ci tier0`);
its replay of recorded logins and world entries is Auberean's `auberean-corpus` tier (`cargo xtask
ci tier1`, or `ci tier1 --game auberean` alone; [Auberean's README](../auberean/README.md#testing) lists
the variables it reads). The
server's claims are rows of the same behaviour registry as the client's.
