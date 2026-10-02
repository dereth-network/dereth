# Empyrean

Empyrean (`empyrean-server`) is a Rust port of [ACE](https://github.com/ACEmulator/ACE) (ACEmulator), the
Asheron's Call server emulator, from the Dereth project (https://dereth.network). It runs as one
executable on Windows, Linux and macOS and needs no database server: world content comes from a
read-only `world.pack` built with `empyrean-import`, and accounts and characters are kept in two
SQLite files the server creates on first start.

The port is unfinished. Anything it cannot do yet is logged once as `not ported: ACE: <member>`.

- **Setting up a server**, running it, keeping it up to date or moving over from ACE:
  [SETUP.md](SETUP.md).
- **Where Empyrean deliberately behaves differently from ACE**, and why:
  [DIVERGENCES.md](DIVERGENCES.md). The code names its rows by number (`V336`).
- **How a release is built and published** (`cargo xtask package empyrean`,
  `cargo xtask release empyrean`): [RELEASING.md](RELEASING.md).

This README is the reference for what the guide does not need: the tools' less common options, the
keys and properties Empyrean adds to ACE's, and the status endpoint.

## Command line

```
empyrean-server [--config <empyrean.toml>] [--run-for <seconds>] [--status <ip>:<port>]
empyrean-server --write-config [<path>] [--from <Config.js>] [--out <path>]
empyrean-server --version
```

- `--config`: the `empyrean.toml` to read instead of looking for one (the working directory, then
  beside the executable). The file must exist; a `.js` file is refused with the converter's command.
- `--run-for`: shut down after this many seconds (for smoke tests).
- `--status`: serve the status endpoint on this address; it wins over `server.status_address`.
- `--write-config`: write the configuration the server would use, or with `--from` the converted
  `Config.js`, as a commented `empyrean.toml` at `<path>` (or `--out <path>`; default
  `empyrean.toml`), then exit. It never overwrites a file.
- `--version`: print the version, the commit and time the binary was built from, its target and
  where its source is, then exit. `empyrean-import --version` does the same.

The server reads no environment variable. Earlier builds read a dat-folder variable, `EMPYREAN_WORLD_PACK`,
`EMPYREAN_SHARD_DB`, `EMPYREAN_AUTH_DB`, `EMPYREAN_STATUS`, `EMPYREAN_LOG` and
`EMPYREAN_NONINTERACTIVE_CONSOLE`; they are ignored now in favour of `server.dat_files_directory`,
`server.world_pack_path`, `database.shard_db_path`, `database.auth_db_path`,
`server.status_address`, `server.log_level` and `server.interactive_console`.

## empyrean-import

```
empyrean-import fetch [--version <n> | --latest] [--dir <folder>] [--pack [--out <world.pack>] [--report <report.json>]]
empyrean-import --sql <dump.sql> [--patches <dir|file>]... [--json <dir|file>]... [--now <YYYY-MM-DD HH:MM:SS>]
                --out <world.pack> [--report <report.json>]
empyrean-import --check <old.pack> (<new.pack> | --sql <dump.sql> [--patches ...] [--json ...])
                [--fields] [--overlap <dir|file>]... [--overlap-json <dir|file>]... [--overlap-overlay <overlay.sqlite>]
                [--report <diff.json>]
empyrean-import --corrections <world.pack> [--report <corrections.json>]
empyrean-import --sql <dump.sql> [--patches ...] [--json ...] --overlay <overlay.sqlite>
                [--overlay-journal <dir>] --out <world.pack> [--report <report.json>]
```

**Fetching.** `fetch` downloads the ACE-World release this build pins (`--version <n>` another,
`--latest` the newest, printing its tag and SHA-256) from GitHub, checks the zip's SHA-256 (the pinned
value, or the one GitHub publishes), and caches the unzipped dump in a per-user folder (`--dir` another).
`--pack` then builds `world.pack` from it, as `--sql <dump> --out <world.pack>` does. SETUP.md section 2.1
names the folder on each system.

**Building.** `--patches` takes SQL as ACE writes it, one object per file (a `DELETE` then its
`INSERT`s, as in ACE-World-16PY-Patches or ACE's `export-sql`); `UPDATE` works too. The statements
run as ACE's MySQL would run them: generated ids continue from the release's counters, deleting a
weenie or landblock deletes its property rows and links, and a duplicate key or a missing parent is
an error. `--json` takes ACE's JSON content (`export-json`: weenies, recipes, landblocks, quests),
converted as ACE's `import-json` converts it; a file's kind comes from its folder or else its
contents. GDLE bulk files (spawn maps, spells, events, quest lists, wielded treasure, recipe and
precursor lists) are converted by ACE's GDLE loaders; GDLE region and terrain files are refused.
Inputs apply in command-line order. The report (`<out>.report.json` unless `--report` names one)
lists rows per table, records per pack table, the content hash and what each input added, replaced
and deleted.

**Comparing** (`--check`) writes no pack. It prints each added (`+`), changed (`~`) and removed
(`-`) record, such as `~ weenie 1234 drudgeskulker (Drudge Skulker)`, and exits 0 with no
differences, 1 with some, 2 on an error. `--fields` prints what changed inside each record, old ->
new: a weenie property by property (`PropertyInt.Value: (none) -> 5`), its emote table by category
(`Emote.Use#0.action[2].motion`), and its unkeyed child rows as rows removed and added; row ids,
which a dump renumbers freely, are not differences. `--overlap` adds the changed records that your
own content also sets: the server's built-in corrections, the files given to `--overlap` (SQL) or
`--overlap-json`, and the records of an `--overlap-overlay`. A content patch replaces its object
whole, so where upstream fixed the same object the patch would keep the old version; each overlap
needs a look. With two packs, `--sql` names the dump the new pack was built from. Compare packs
built from ACE's dumps alone when reviewing an upstream release, so the differences are upstream's.

**Corrections.** The server corrects a few values of ACE's data as it reads them, where retail
shows the stored value is wrong. They live in the server binary, not the pack, so the content hash
alone does not say what players see; at start-up the server logs both:

```
World database: content hash 1b260e4e…a8a8 (verified), corrections v1:aea10e6ae7317bfe
World database: corrections v1:aea10e6ae7317bfe: 51 of 51 entries apply (0 stale, 0 absent); the rules change 132 default scripts and 4 emote motions
```

`@empversion` and the status endpoint report the same two values; two servers serve the same world
data when both match. The digest is `v1:` and 16 hex digits of the BLAKE3 hash of a canonical
listing of every entry and rule, so it changes whenever a correction does. `--corrections` prints
the digest and every entry (`applies`; `stale`, with the value the pack stores instead; or
`absent`) with its property, stored and corrected values and DIVERGENCES row, and every value each
rule changes. It exits 0 when every entry applies and 1 otherwise: ACE may have fixed the value (the
entry can go) or changed it (it needs another look).

**The content overlay.** ACE's developer content commands (`@import-sql`, `@import-json`,
`@export-sql`, `@export-json`, `@clearcache`, `@createinst`, `@removeinst`, `@addenc`, `@removeenc`,
`@nudge`, `@rotate`, ...) write to the overlay named by `server.world_overlay_path`, reading and
writing files under the `content_folder` server property as ACE does. The first write loads
`server.world_base_sql` and `server.world_base_patches` (a few seconds); each write then runs as an
`empyrean-import` patch would and is kept in the overlay's journal. `--overlay` bakes the journal
into a new pack over the same base inputs (checked against the base the overlay was made over) and
never overwrites the pack the overlay sits on. `--overlay-journal <dir>` also writes the journal as
numbered SQL files, which rebuild the same pack as one more `--patches <dir>`.

## Configuration keys Empyrean adds

`empyrean.toml.example` lists every key with its default and the ACE `Config.js` setting it stands
for. Keys that are Empyrean's own:

- `[server] world_pack_path` (default `./world.pack`): replaces ACE's `MySql.World` connection.
- `[server] world_overlay_path`, `world_base_sql`, `world_base_patches`: the content overlay.
- `[server] log_level` (default `info`): `error`, `warn`, `info`, `debug` or `trace`; any other value
  is a start-up warning and the server logs at `info`.
- `[server] status_address` (default empty, off): the status endpoint's `<ip>:<port>`.
- `[server] interactive_console` (default `true`): read console commands from standard input.
- `[server] source_url` (default empty: the repository the build came from): where players are told
  the server's source is (see [Licence](#licence)).
- `[database] shard_db_path` and `auth_db_path` (defaults `./shard.db` and `./auth.db`): the SQLite
  files that replace ACE's `ace_shard` and `ace_auth` databases.

ACE settings Empyrean does not have load without error and are each logged once as not read:
`[server.threading]`, `server.mods_directory`, `server.world_database_precaching`, the
`[mysql.*]` connections and the `[offline]` update switches. The section earlier builds called
`[mysql]` is `[database]` now.

At the console, `config-write [<path>] [-f]` writes the running configuration as a commented
`empyrean.toml` (`-f` overwrites). After a configuration key changes, regenerate the example with
`empyrean-server --write-config empyrean.toml.example` in an empty directory and copy it to
`empyrean/server/empyrean.toml.example`; a test fails while the checked-in file is out of date.

**Server properties Empyrean adds** (set with `/modifybool`; `/showprops` lists them after ACE's):

- `monster_ranged_closing` (bool, default **true**): ranged and casting monsters close to a range
  before they attack, as retail's did (V336). When a monster decides on a missile shot or a spell
  and its target is beyond that attack's range R (a missile weapon's `MaximumVelocity`² / 9.8; a
  spell's range constant plus its range modifier times the monster's skill in the spell's school,
  uncapped), it runs toward the target with a non-sticky chase, stops at a distance drawn uniformly
  from 2/3 R to 16/15 R, turns and attacks, and decides again after each attack. A target already
  in range is only turned to, and melee keeps the sticky chase. **Off** gives ACE's behaviour: a
  missile monster never moves toward its target (it turns, and switches to melee out of range), a
  caster beyond range chases stickily and casts on the way, and spell range is capped at 75 m.

## Status endpoint

ACE has none; this one is for monitoring, off unless `--status` or `server.status_address` names an
address. It has no authentication: keep it on loopback or a private network.

- `GET /health`: `200 ok` while the world thread answers within 2 seconds, otherwise `503`.
- `GET /status` (also at `GET /v1/world`): the same check, with a JSON body:

```
{"world_name":"Empyrean","version":"0.0.0","source_url":"https://github.com/dereth-network/dereth",
 "uptime_seconds":73,"world_open":true,
 "shutting_down":false,"connections":1,"authenticated_connections":1,"players_online":1,
 "landblocks_loaded":9,"content_hash":"1b260e4e…a8a8","corrections_digest":"v1:aea10e6ae7317bfe",
 "era":"eor","dats":{"portal":2072,"cell":982,"local":994,"highres":497,"patching":false},
 "client_versions":["1802"],"websocket_url":null,
 "not_ported":{"ACE: EventManager.Initialize":1}}
```

`content_hash` is `null` without a pack; `era` is the configured `[era] profile`; `dats` are the
iterations of the server's dats, which it compares a client's against, and whether it patches them;
`not_ported` lists each unported ACE member this process has reached, with how often.

## Licence

AGPL-3.0-only (see `LICENSE`); the server is derived from ACE, which is AGPL-3.0 too. `NOTICE.txt`
in a release package names the release's source (the repository at the exact commit it was built
from), carries the MIT notice of the Lifestoned data model names the server uses, and lists the
crates it is built from; `THIRD-PARTY-LICENSES.html` holds each third-party crate's licence text.

**Running a modified server.** The login welcome, `@source`, the last line of `@empversion` and the
status endpoint's `source_url` all name `[server] source_url`, or the repository the build came
from when that key is empty. The AGPL asks whoever runs a *modified* version for users over a
network to offer those users the modified source: publish your changes (a public fork is enough)
and set `source_url` to where they are. An unmodified release needs no setting. (This is a summary,
not legal advice; the licence text is in `LICENSE`.)
