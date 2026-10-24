# Empyrean

Empyrean (`empyrean-server`) is an Asheron's Call server: a Rust port of
[ACE](https://github.com/ACEmulator/ACE) (ACEmulator), from the Dereth Network
(https://dereth.network). It is one program on Windows, Linux and macOS and needs no database
server. World content comes from a read-only `world.pack` built with `empyrean-import`, and
accounts and characters live in two SQLite files the server creates on first start.

The port is unfinished. Anything it can't do yet is logged once as `not ported: ACE: <member>`.

A world can play an earlier era than the end of retail. Set `[era] profile` in `empyrean.toml`:
`eor` (the default) or `infiltration` (February 2005). That era's rules are ported from
[ClassicACE](https://github.com/bDekaru/ClassicACE), bDekaru's fork of ACE: character creation,
combat and death formulas, starter gear and first login. Each such rule names its ClassicACE
source file, and `DIVERGENCES.md` lists them as the era rows.

An era also sets which systems the world has: ratings, aetheria, luminance, trade, housing,
tinkering, chess and so on. The `[era]` section can turn any of them on or off
(`aetheria = true`, `trade = false`). The server refuses what the world lacks, whatever a client
asks, and the Dereth client's screens follow the server's list.

- [SETUP.md](SETUP.md): setting up a server, running it, keeping it up to date and moving over
  from ACE.
- [DIVERGENCES.md](DIVERGENCES.md): where Empyrean deliberately differs from ACE, and why. The code
  names its rows by number (`V336`).
- [RELEASING.md](RELEASING.md): how a release is built and published.

This file is the reference for what the guide leaves out: the less common options, the settings
and properties Empyrean adds to ACE's, and the status endpoint.

## Command line

```
empyrean-server [--config <empyrean.toml>] [--run-for <seconds>] [--status <ip>:<port>]
empyrean-server --write-config [<path>] [--from <Config.js>] [--out <path>]
empyrean-server update --check|--apply [--config <empyrean.toml>] [--policy <off|patch|minor>]
empyrean-server --version
```

| Option | What it does |
|---|---|
| `--config` | The `empyrean.toml` to read. Without it the server looks in the working directory, then beside the program. The file must exist; a `.js` file is refused with the command that converts it. |
| `--run-for` | Shut down after this many seconds, for smoke tests. |
| `--status` | Serve the status endpoint on this address, over `server.status_address`. |
| `--write-config` | Write the configuration the server would use as a commented `empyrean.toml`, then exit. With `--from` it converts an ACE `Config.js` instead. It writes to `<path>` or `--out`, by default `empyrean.toml`, and never overwrites a file. |
| `update` | Check for or install a newer server release ([SETUP.md](SETUP.md), section 6.4). |
| `--version` | Print the version, the commit and time it was built from, its target and where its source is. `empyrean-import --version` does the same. |

The server reads no environment variables. Everything is set in `empyrean.toml`.

## empyrean-import

```
empyrean-import fetch [--world patches|16py] [--version <n> | --latest] [--dir <folder>]
                      [--pack [--out <world.pack>] [--report <report.json>]]
empyrean-import --sql <dump.sql> [--patches <dir|file>]... [--json <dir|file>]...
                [--now <YYYY-MM-DD HH:MM:SS>] [--era eor|infiltration]
                [--allow-skipped] [--allow-unknown-tables] [--allow-unknown-columns]
                --out <world.pack> [--report <report.json>]
empyrean-import --check <old.pack> (<new.pack> | --sql <dump.sql> [--patches ...] [--json ...])
                [--fields] [--overlap [<dir|file>]]... [--overlap-json <dir|file>]...
                [--overlap-overlay <overlay.sqlite>] [--report <diff.json>]
empyrean-import --corrections <world.pack> [--report <corrections.json>]
empyrean-import --sql <dump.sql> [--patches ...] [--json ...] --overlay <overlay.sqlite>
                [--overlay-journal <dir>] --out <world.pack> [--report <report.json>]
empyrean-import dat-overlay --base <dir> --world <dir> --out <dir> --world-key <name> [--era eor|infiltration]
empyrean-import gdle --sql <dump.sql> --patches <dir|file>... [--quests <quests.json>] --out <dir>
```

`empyrean-import --help` prints the full list, including `spawns` and `schema`.

### Fetching

`fetch` downloads an ACE-World release from GitHub and caches the unzipped dump in a per-user
folder (SETUP.md section 2.1 names it; `--dir` picks another). `--world` picks the world database:
`patches`, the end of retail (the default), or `16py`, February 2005. By default it fetches the
release this build pins and checks it against the pinned SHA-256. `--version <n>` fetches another
release and `--latest` the newest, each checked against the SHA-256 GitHub publishes. `--pack`
then builds `world.pack` from the dump.

### Building

- `--patches` takes SQL as ACE writes it, one object per file: a `DELETE` and its `INSERT`s, as in
  ACE-World-16PY-Patches or ACE's `export-sql`. `UPDATE` works too. The statements run as ACE's
  MySQL would run them: generated ids continue from the release's counters, deleting a weenie or
  landblock deletes its property rows and links, and a duplicate key or a missing parent is an
  error.
- `--json` takes ACE's JSON content (`export-json`: weenies, recipes, landblocks, quests),
  converted as ACE's `import-json` converts it. A file's kind comes from its folder, or else its
  contents.
- GDLE bulk files (spawn maps, spells, events, quest lists, wielded treasure, recipe and precursor
  lists) are converted by ACE's GDLE loaders. GDLE region and terrain files are refused.
- `--era` records which era the pack is for.
- A dump with statements, tables or columns the importer can't read is refused, naming each with
  its count. The `--allow-` flags build the pack anyway.

Inputs apply in command-line order. The report (`<out>.report.json` unless `--report` names one)
lists rows per table, records per pack table, the content hash, and what each input added,
replaced and deleted.

### Comparing

`--check` writes no pack. It prints each added (`+`), changed (`~`) and removed (`-`) record, such
as `~ weenie 1234 drudgeskulker (Drudge Skulker)`, and exits 0 with no differences, 1 with some and
2 on an error.

- `--fields` shows what changed inside each record, old -> new: a weenie property by property
  (`PropertyInt.Value: (none) -> 5`), its emote table by category
  (`Emote.Use#0.action[2].motion`), and its unkeyed child rows as rows removed and added. Row ids,
  which a dump renumbers freely, don't count as differences.
- `--overlap` adds the changed records your own content also sets: the server's built-in
  corrections, plus the SQL files given to `--overlap`, the JSON given to `--overlap-json` and the
  records of an `--overlap-overlay`. A content patch replaces its object whole, so where upstream
  fixed the same object your patch would undo the fix. Look at each overlap.
- With two packs, `--sql` names the dump the new pack was built from.

To review an upstream release, compare packs built from ACE's dumps alone, so the differences are
all upstream's.

### Corrections

The server corrects a few values in ACE's data as it reads them, where retail shows the stored
value is wrong. The corrections live in the server, not the pack, so the content hash alone doesn't
say what players see. At startup the server logs both:

```
World database: content hash 1b260e4e…a8a8 (verified), corrections v1:aea10e6ae7317bfe
World database: corrections v1:aea10e6ae7317bfe: 51 of 51 entries apply (0 stale, 0 absent); the rules change 132 default scripts and 4 emote motions
```

`@empversion` and the status endpoint report the same two values. Two servers serve the same world
data when both match. The digest is `v1:` and 16 hex digits of a BLAKE3 hash over every correction,
so it changes whenever one does.

`--corrections` prints the digest and every entry: its property, stored and corrected values,
DIVERGENCES row, and whether it `applies`, is `stale` (with the value the pack has instead) or is
`absent`. It also prints every value each rule changes. It exits 0 when every entry applies and 1
otherwise, which means ACE has fixed the value (the entry can go) or changed it (it needs another
look).

### The content overlay

ACE's developer content commands (`@import-sql`, `@import-json`, `@export-sql`, `@export-json`,
`@clearcache`, `@createinst`, `@removeinst`, `@addenc`, `@removeenc`, `@nudge`, `@rotate` and so
on) write to the overlay named by `server.world_overlay_path`. They read and write files under the
`content_folder` server property, as ACE does. The first write loads `server.world_base_sql` and
`server.world_base_patches`, which takes a few seconds. Each write then runs as an
`empyrean-import` patch would, and is kept in the overlay's journal.

`--overlay` bakes the journal into a new pack over the same base inputs, checked against the base
the overlay was made over. It never overwrites the pack the overlay sits on. `--overlay-journal
<dir>` also writes the journal as numbered SQL files, which rebuild the same pack as one more
`--patches <dir>`.

### Other commands

- `dat-overlay` takes a world's modified data files apart against their base files, into the
  overlay the server serves (`[dat_overlay] path`).
- `gdle` writes the weenies, spawns and quests the patches reach as GDLE's JSON, with `quests.json`
  merged into the `--quests` list.

## Settings Empyrean adds

`empyrean.toml.example` lists every setting with its default and the ACE `Config.js` setting it
replaces; Empyrean's own are marked `(Empyrean only)`. The main ones:

| Setting | Default | What it is |
|---|---|---|
| `server.world_pack_path` | `./world.pack` | The world database. Replaces ACE's `MySql.World`. |
| `server.world_overlay_path`, `world_base_sql`, `world_base_patches` | empty | The content overlay (above). |
| `server.log_level` | `info` | `error`, `warn`, `info`, `debug` or `trace`. Anything else logs a warning and uses `info`. |
| `server.status_address` | empty (off) | The status endpoint's `<ip>:<port>`. |
| `server.status_public_url` | empty | Where players reach `/status` from outside. With the Dereth Network on, the world tells its hub this and its WebSocket URL. |
| `server.interactive_console` | `true` | Read console commands from standard input. |
| `server.admin_channel` | `true` | A console for programs on this computer, on loopback. Its address and secret are in `admin-channel.json` in the server's folder (SETUP.md, "The console"). |
| `server.source_url` | empty (the repository the build came from) | Where players are told the source is (see [License](#license)). |
| `[server.websocket]` | off | The WebSocket endpoint for browser clients (SETUP.md section 4.7). |
| `database.shard_db_path`, `auth_db_path` | `./shard.db`, `./auth.db` | The SQLite files that replace ACE's `ace_shard` and `ace_auth`. |
| `[dat_overlay]` | off | A world's own data files, served to Dereth clients as an overlay, with patching rates. |
| `[status]` | on | The status ping on the game port, which launchers use, and its rate limits. |
| `[federation]` | off | The link to the Dereth Network, for cross-world tells, friends and chat. |
| `[era]` | `profile = "eor"` | The era and its systems (above). |

ACE settings Empyrean doesn't have still load, and each is logged once as not read:
`[server.threading]`, `server.mods_directory`, `server.world_database_precaching`, the `[mysql.*]`
connections and the `[offline]` update switches. The section earlier builds called `[mysql]` is
`[database]` now.

At the console, `config-write [<path>] [-f]` writes the running configuration as a commented
`empyrean.toml` (`-f` overwrites).

When you add or change a setting in the code, regenerate the example: run `empyrean-server
--write-config empyrean.toml.example` in an empty folder and copy the result to
`empyrean/server/empyrean.toml.example`. A test fails while the checked-in file is out of date.

## Server properties Empyrean adds

Set with `/modifybool`; `/showprops` lists them after ACE's.

`monster_ranged_closing` (default **on**, V336): archers and casters close in before attacking,
as in retail. When a monster picks a missile attack or a spell and its target is beyond that
attack's range R, it runs toward the target (a non-sticky chase), stops at a random distance
between 2/3 R and 16/15 R, turns and attacks, then decides again. R is a missile weapon's
`MaximumVelocity`² / 9.8, or a spell's range constant plus its range modifier times the monster's
skill in the spell's school, uncapped. A target already in range is only turned to, and melee
keeps the sticky chase.

Off gives ACE's behavior: a missile monster never moves toward its target (it turns, and switches
to melee out of range), a caster beyond range chases stickily and casts on the way, and spell range
is capped at 75 m.

## Status endpoint

ACE has none. It's for monitoring, and off unless `--status` or `server.status_address` names an
address. It has no authentication, so keep it on loopback or a private network.

- `GET /health`: `200 ok` while the world thread answers within 2 seconds, otherwise `503`.
- `GET /status` (also `GET /v1/world`): the same check, with a JSON body:

```
{"world_name":"Empyrean","version":"0.0.0","source_url":"https://github.com/dereth-network/dereth",
 "uptime_seconds":73,"world_open":true,
 "shutting_down":false,"connections":1,"authenticated_connections":1,"players_online":1,
 "landblocks_loaded":9,"content_hash":"1b260e4e…a8a8","corrections_digest":"v1:aea10e6ae7317bfe",
 "era":"eor","features":{"ratings":true,"consolidated_weapon_skills":true,…,"spell_research":false,
 "chess":true},"dats":{"portal":2072,"cell":982,"local":994,"highres":497,"patching":false},
 "client_versions":["1802"],"websocket_url":null,
 "not_ported":{"ACE: EventManager.Initialize":1}}
```

- `content_hash` is `null` without a pack.
- `era` is the `[era] profile`, and `features` lists every system and whether the world has it.
- `dats` gives the iterations of the server's data files, which it compares a client's against,
  and whether it patches them.
- `not_ported` lists each unported ACE member this process has reached, and how often.

## License

AGPL-3.0-only (see `LICENSE`). The server is derived from ACE, and its era rules from ClassicACE,
both AGPL-3.0. In a release, `NOTICE.txt` names the exact source commit it was built from, carries
the MIT notice for the Lifestoned data model names the server uses, and lists the crates it is
built from. `THIRD-PARTY-LICENSES.html` has each crate's license text.

**Running a modified server.** The login welcome, `@source`, the last line of `@empversion` and
the status endpoint's `source_url` all show `server.source_url`, or the build's repository when it
is empty. The AGPL asks anyone who runs a *modified* version for players over a network to offer
them the modified source: publish your changes (a public fork is enough) and set `source_url` to
point there. An unmodified release needs no setting. (This is a summary, not legal advice; the
license text is in `LICENSE`.)
