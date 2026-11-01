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

| Folder | Package | What it is |
|---|---|---|
| [`server/`](server/) | `yalain` | the `yalain` program: settings, the data it reads at start, the sockets and the loop |
| [`crates/world/`](crates/world/) | `yalain-world` | the game: its state and rules, sans-IO |
| [`crates/net/`](crates/net/) | `yalain-net` | the server's connections over the shared transport, sans-IO, with an in-memory network and UDP |
| [`crates/store/`](crates/store/) | `yalain-store` | accounts and characters, in SQLite or in memory |
| [`crates/content/`](crates/content/) | `yalain-content` | the tables the world plays from: objects, camps, loot, stores, quests, what people say |
| [`crates/world-db/`](crates/world-db/) | `yalain-world-db` | the world database: read, checked, merged and compiled into those tables |
| [`testkit/`](testkit/) | `yalain-testkit` | a whole server on a virtual clock with clients that speak the wire, and the replay of recorded logins |

## Running it

You need your own Asheron's Call 2 install, one no client is running from, and the world database
this build pins, fetched once into your user cache.

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
takes what can change while players are in the world: templates, loot, stores, quests, what
people say, the story's scenes, effects and crafting, and the `--banned-names` file. Placed
objects (the market's brokers among them) and camps change only at the next start; the log says
which tables were reloaded and which need a restart. A database that does not compile leaves the
world as it was, and the log says why. `help` lists the console's commands.

`--banned-names <file>` refuses new character names holding the words it lists: one pattern a
line, `*` for any run of letters, matched against a name's letters in small letters. The list is
empty by default; which words the 2017 servers refused is not known.

## Testing

Its tests that need no data run with the workspace's (`cargo xtask check`, `cargo xtask ci tier0`);
its replay of recorded logins and world entries is Auberean's `auberean-corpus` tier (`cargo xtask
ci tier1`, or `ci tier1 --game auberean` alone; [Auberean's README](../auberean/README.md#testing) lists
the variables it reads). The
server's claims are rows of the same behaviour registry as the client's.
