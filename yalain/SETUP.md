# Setting up a Yalain world

This guide takes you from nothing to a running Yalain world that players can log in to, and covers
listing it on the Dereth Network.

Yalain (`yalain`) is the Dereth Network's Asheron's Call 2 server. It is one program with no
database server to install. At each start it reads your own Asheron's Call 2 install, compiles the
world database (`yalain-world`) against it, and keeps accounts and characters in one SQLite file
it makes itself. Everything is set on the command line, or in a configuration file
(`yalain.toml`, section 3.6).

Contents:

1. [What you need](#1-what-you-need)
2. [The world database](#2-the-world-database)
3. [Run](#3-run)
4. [Ports and the firewall](#4-ports-and-the-firewall)
5. [Accounts](#5-accounts)
6. [The console](#6-the-console)
7. [Logs](#7-logs)
8. [The Dereth Network](#8-the-dereth-network)
9. [Connect a client](#9-connect-a-client)
10. [Troubleshooting](#10-troubleshooting)

---

## 1. What you need

| What | Where it comes from |
| --- | --- |
| An Asheron's Call 2 install: the September 2013 client's folder, with `portal.dat`, `cell_1.dat` and `local_English.dat` | Your own install. The project ships no game data, and never will. |
| `yalain` (`yalain.exe` on Windows) | A release package, or built from source (below). |
| The world database | Its release, which `yalain world fetch` downloads for you (section 2). |

The server needs `portal.dat` and `cell_1.dat`; it reads `local_English.dat` too when it is there.
Clients ask the world for records their own data files lack, and the world sends them from these
files. Point the server at a copy no client is running from: a running client opens its data files
for itself alone.

### Building from source

The server is a member of a Cargo workspace. Install a current stable Rust toolchain
(https://rustup.rs), then from the workspace root (the folder holding the top-level `Cargo.toml`):

```
cargo build --profile server-release -p yalain
```

The binary lands in `target/server-release/` under the workspace root. `--release` works too
(into `target/release/`), without the link-time optimization. To try it straight from the source
tree, `cargo run --profile server-release -p yalain -- <options>` runs it with the options after
`--`.

---

## 2. The world database

The world database is the world's content: where people, objects and camps stand, what drops and
is sold, how quests move on, and what people say. It is a folder of text files, released as a zip
archive. The server compiles it against your install at every start (a second or two), and logs
how much of each kind it compiled.

### 2.1 The one command

```
yalain world fetch
```

This downloads the release this build of the server pins from
https://github.com/dereth-network/yalain-world/releases, checks the archive's SHA-256 against the
value built into `yalain`, checks the database, and unpacks it into a per-user cache folder, one
folder per release. It needs an internet connection once. A release already in the cache is not
downloaded again (`<tag> is already cached`). An archive whose SHA-256 does not match, or a
database that does not check, stops it with an error, and nothing is cached. A folder under the
pinned release's name whose files are not that release is fetched again, and the old folder is
kept beside it as `<tag>.replaced`.

The cache is a per-user folder:

| System | Cache folder |
| --- | --- |
| Windows | `%LOCALAPPDATA%\Yalain\world-database` |
| macOS | `~/Library/Caches/Yalain/world-database` |
| Linux | `$XDG_CACHE_HOME/yalain/world-database`, else `~/.cache/yalain/world-database` |

Because the cache belongs to a user, fetch as the user the server runs as. The options:

| Option | What it does |
| --- | --- |
| `--version <tag>` | fetch release `<tag>` (`v0.3.0`, or `0.3.0`) instead of the pinned one |
| `--sha256 <hex>` | the SHA-256 to check the archive against. Without it, a release other than the pinned one is checked against the `.sha256` file published beside its archive |
| `--from <address>` | where to download from: a URL, a `file://` URL, a file, or a folder holding the archive. `{tag}` in it stands for the release's tag |
| `--dir <folder>` | unpack into `<folder>` instead of the cache |

The `YALAIN_WORLD_URL` environment variable sets the address as `--from` does; `--from` wins.

**By hand**, if the machine cannot reach GitHub: download `yalain-world-<tag>.zip` from the
release's page on another machine, copy it over, and give it to fetch, which checks and unpacks it
as before:

```
yalain world fetch --from yalain-world-<tag>.zip
```

### 2.2 Which database the server serves

`--world-db` chooses:

- **Nothing, or `--world-db pinned`:** the release this build pins, from the cache. If it is not
  there, the server fetches it as it starts, as `yalain world fetch` does, and a world that lies
  over a release does the same for that one. If that fails it stops and says why, and nothing is
  changed. A release fetched with `--dir` is not looked for there: serve it as a folder (`--world-db <folder>/<tag>`).
- **`--world-db <folder>`:** the database in that folder, as it is. This is how to serve an
  unreleased checkout of the database's repository, or a release you unpacked yourself.
- **A world made in [Athenaeum](../athenaeum/README.md)** names a base database (its `[base]`) and
  holds only its own changes. Give its folder to `--world-db`, and the server lays it over the
  release its `[base]` names (or the pinned one, if it names none), from the cache. `--world-base
  <folder>` takes the base from that folder instead.
  If the world changes the game's own records, `athenaeum build` writes them to its `overlay`
  folder (`--dat-overlay` names another), and the server reads its data files through that. An
  overlay built against another install, or another world's, stops the server: build it again
  against this one's `--dats`. Auberean is sent what it lacks as it logs in. A client holding other
  data files is refused, and so is a retail client unless you turn on `[ddd] enable_dat_patching`:
  then it must run with `-rodat off`, on a copy of its files kept for this world.

Two commands help when you change a database:

```
yalain world check <folder>
yalain world compile --dats <install folder> --world-db <folder> --out <folder>
```

`check` checks a database without an install: the schema, keys, references, how its packs merge,
and every row's provenance. `compile` compiles it against the install as the server would, and
writes the tables it makes as text files (`content.tsv`, `spawns.tsv`, `loot.tsv`, `stores.tsv`,
`quests.tsv`, `talk.tsv`) for reading and comparing.

To take changes to the database without a restart, use the console's `reload` (section 6).

---

## 3. Run

### 3.1 First start

```
yalain --dats <install folder>
```

The start takes a few seconds: the server reads the install, compiles the world database, and logs
what it read. Then it logs where it listens, for example:

```
listening on 127.0.0.1:9000/9001 (login) and 127.0.0.1:9002/9003 (world, referred as 127.0.0.1)
friends service listening on TCP 127.0.0.1:9000
```

On its first start the server creates `yalain.sqlite` in the working directory. It keeps the
accounts, the characters, their friends and ignore lists, and the Dereth Network's settings and
blocks. While the server runs, `yalain.sqlite-wal` and `yalain.sqlite-shm` sit beside it: they are
part of the database (section 3.5 says how to back it up).

By default the server listens on 127.0.0.1, so only this machine can reach it. Section 4 opens it
to others.

### 3.2 The options

| Option | What it does |
| --- | --- |
| `--dats <folder>` | the install to read (required) |
| `--world-db <folder>` or `pinned` | the world database to serve (default `pinned`; section 2.2) |
| `--world-base <folder>` | the base database an Athenaeum world lies over, instead of the release in the cache |
| `--data <file>` | the database of accounts and characters (default `yalain.sqlite`) |
| `--memory` | keep nothing: accounts and characters live in memory and are gone when the server stops |
| `--world-name <name>` | the world's name, shown at login (default `Local World`) |
| `--bind <ip>` | the address to listen on (default `127.0.0.1`) |
| `--login-port <port>` | the login's port, an even number (default 9000; the port above it is used too) |
| `--world-port <port>` | the world's port, an even number (default 9002; the port above it is used too) |
| `--advertise <ip>` | the IPv4 address players from elsewhere are sent to for the world; players on this computer or its network get the nearby address (default: `--bind`; required when `--bind` is 0.0.0.0 or an IPv6 address) |
| `--no-referral` | keep each client on its login connection instead of sending it to the world's port |
| `--listed-accounts` | admit only accounts made with `--add-account` (section 5) |
| `--add-account <name>:<ticket>` | make an account with that ticket, then carry on (section 5) |
| `--admin <name>` | make an account an admin's when this start makes it with `--add-account` (section 5) |
| `--admin-command <command>:<group,group>` | keep an account-level admin command to named groups (section 5) |
| `--serve-admin-library` | serve admin accounts' retail clients the library their admin commands need (section 5) |
| `--secret-cost <n>` | the bcrypt cost tickets are kept at, 4 to 31 (default 8; section 5) |
| `--save-interval <s>` | save each character in the world every that many seconds, 1 or more (default 300; section 3.3) |
| `--watchdog <s>` | stop with a failure if the world runs no pass for that many seconds, 0 for never (default 300) |
| `--shutdown-interval <s>` | how long `shutdown` counts down, 0 to 99999 seconds (default 120; section 3.3) |
| `--max-sessions <n>` | how many logins the world holds at once (default 128; section 3.7) |
| `--max-sessions-per-address <n>` | how many logins one address holds at once (default `unlimited`; section 3.7) |
| `--unlimited-sessions-from <ip>` | an address the per-address limit does not hold back; may be given many times (section 3.7) |
| `--download-rate <n>` | the bytes a second a client is sent a large data download at, 4096 to 262144 (default 65536, the retail servers' rate: about two minutes over the largest record) |
| `--motd <text>` | a message shown to each player as its character enters the world; `\n` starts a new line (default: none) |
| `--status <ip>:<port>` | serve the status endpoint at this address (default: off; section 3.8) |
| `--websocket` and the `--websocket-…` options | also take WebSocket connections, for clients in a browser (default: off; section 4.1) |
| `--config <file>` | read the options from this configuration file (section 3.6) |
| `--banned-names <file>` | refuse new character names holding the words it lists (below) |
| `--no-friends` | serve no friends service (section 4) |
| `--hub`, `--hub-slug`, `--hub-key-file`, `--federation`, `--global-room`, `--min-character-age` | the Dereth Network (section 8) |
| `--verbose` | log at the debug level (section 7) |
| `--seconds <s>` | stop after that many seconds |
| `--stop-at-console-end` | stop, saving everyone, when the console's input ends (section 3.3) |
| `--update <policy>`, `--update-check-hours <h>`, `--update-warning-seconds <s>`, `--update-source <url>` | which new releases the server installs by itself, and how (section 3.10) |
| `--no-status-ping`, `--status-ping-hellos <n>`, `--status-ping-asks <n>`, `--status-ping-replies <n>` | turn the status ping off, or set its limits (section 3.9) |

`yalain --version` prints the version, `yalain --release-facts` what an update reads, and
`yalain update` checks for and installs a new release (section 3.10). The server exits with 0 when it stops as asked, 1 when it fails, and **3** when
another program holds one of its ports (section 6.3).

**Banned names.** The file holds one pattern a line, in small letters; `*` stands for any run of
letters. Blank lines and lines starting with `#` are skipped. A new character's name is refused
when its letters, in small letters, hold a pattern. The list is empty by default. `reload` reads
the file again.

**Options for testing.** These set up characters and the clock for trials; you do not need them to
run a world.

| Option | What it does |
| --- | --- |
| `--add-character <account>:<name>:<race>:<sex>` | make a character (sex 4096 or 8192) with a middling physique, then carry on; a missing account is made on an open world only (section 5) |
| `--place <account>:<name>:<cell>:<x>:<y>:<z>:<heading>` | put a character at a place (the cell in hex, the heading in degrees) before it next enters |
| `--money <account>:<name>:<amount>` | set the money of a character that has played, before it next enters |
| `--make-character <file>` | make a character with a level, skills, money, items and quests before it first plays (section 6.2) |
| `--game-time <s>` | start the game clock at that many seconds after 1 December 2012 (default: now); a game day is 7,347 seconds |
| `--hold-clock` | hold the game clock where it starts: the sky and the time of day stand still, and nothing timed in game time runs out |
| `--no-echo` | do not send players their own positions back |

### 3.3 Stopping

Type `shutdown` at the console (section 6), or `shutdown <message>` to show every player in the
world your message first. Every player in the world is told the world is shutting down, two minutes
ahead and then every thirty seconds; twenty seconds after the last line, the server stops. With
`--hub`, the hub shows the world in maintenance during the countdown, and offline once it stops.

The countdown is two minutes unless you give another length, with `--shutdown-interval <s>` or the
console's `set-shutdown-interval <s>` (0 to 99999 seconds). A longer countdown shows its first
line at once, then a line at two hours, one hour, 45, 30, 15, 10 and 5 minutes before (those still
ahead), and from two minutes on as above. `cancel-shutdown` stops a countdown and tells the
players the world is no longer shutting down. `stop-now` (with a message if you like) shows the
players the last line and stops the server at once; `stop` and `exit` do the same. A program that
runs Yalain (an editor, a launcher) can stop it by typing `stop` into its standard input and
waiting for it to end.

**When the console's input ends.** By default the end of the console's input (Ctrl-D in a
terminal, a closed pipe, or no input at all, as a service has) ends only the console: the log says
so, and the world goes on. A program that runs Yalain and holds its console should start it with
`--stop-at-console-end`: then, if the program goes away (it crashed, or was killed) and its end of
the console closes, Yalain stops at once, saving everyone, and frees its ports. Do not give it to
a server run as a service, whose input ends as soon as it starts.

Ctrl-C, and on Linux and macOS SIGTERM and SIGHUP (on Windows, Ctrl-Break too), stop the server
at once, without the countdown. `--seconds` stops it when its time is up.

However the server stops, it first saves every character still in the world, as if its player had
left, and logs how many. Closing the console window on Windows ends the server before it can save;
stop it with Ctrl-C instead.

**Saves while the server runs.** Every character in the world is saved every five minutes
(`--save-interval <s>` sets another interval), as well as when it leaves the world and when some
things change (its equipment, what it crafts, among others); a save at one of those puts the next
one five minutes after it. So a crash loses at most the last few minutes of play. When many
characters come due at once, they are saved one at a time over the next moments, not all at once.
`save-world` at the console saves everyone in the world now.

### 3.4 Running as a service

Run the server under a service manager like any program that writes to a terminal. A server
started with no standard input (as a service is) has no console; everything else runs as before.
Without a console there is no `shutdown` countdown: stopping the service (SIGTERM) saves everyone
in the world and stops it, as in section 3.3. On Windows, use a service wrapper that stops the
server with Ctrl-C.

Give `--data` an absolute path, or start the server in the folder that holds `yalain.sqlite`, and
fetch the world database as the service's user (section 2.1).

An example systemd unit, `/etc/systemd/system/yalain.service`, for a server in `/opt/yalain`
running as the user `yalain`:

```ini
[Unit]
Description=Yalain world
After=network-online.target
Wants=network-online.target

[Service]
User=yalain
WorkingDirectory=/opt/yalain
ExecStart=/opt/yalain/yalain --dats /opt/ac2 --data /opt/yalain/yalain.sqlite --bind 0.0.0.0 --advertise 203.0.113.10
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

Then `systemctl daemon-reload`, `systemctl enable --now yalain`, and read the log with
`journalctl -u yalain`.

---

### 3.5 Backups

Everything that changes as people play is in `yalain.sqlite` (or your `--data` file).

- **Server stopped:** copy `yalain.sqlite`, with `yalain.sqlite-wal` and `yalain.sqlite-shm` if
  they are beside it.
- **Server running:** use SQLite's online backup, which gives a whole copy while the server writes:
  ```
  sqlite3 yalain.sqlite ".backup 'yalain-backup.sqlite'"
  ```
  Copying the files while the server runs can give a copy that does not open.

**Automatic backups.** When a new release of the server changes the database's layout, it first
copies the file beside itself, as `yalain.sqlite.backup-v<from>-v<to>-<UTC time>`, and logs it. If
the copy fails (a full disk, no permission to write the folder), nothing is changed and the server
stops with the reason. Only the newest three of these copies are kept. The first start of this
release makes one (`...backup-v0-v1-...`) of a database an earlier Yalain kept.

To go back to a copy, stop the server, delete `yalain.sqlite-wal` and `yalain.sqlite-shm`, and copy
the backup over `yalain.sqlite`. Play since the copy is lost. A database a newer release has
upgraded is refused by an older one, which names the versions.

### 3.6 The configuration file

Every setting can be kept in a configuration file instead of on the command line. The server
reads `yalain.toml` from the working folder, else from beside `yalain`; `--config <file>` names
another (which must exist). The log's first lines say which file it read. Without one, the command
line is all.

The file takes the form of Empyrean's `empyrean.toml`: the same sections and key names, with the
same meanings and units, wherever Yalain has the setting, so a host who runs worlds of both games
keeps one kind of file:

```toml
[server]
world_name = "Dawnsong"
dat_files_directory = "/opt/ac2"
message_of_the_day = "Welcome to Dawnsong. Be kind."

[server.network]
host = "0.0.0.0"
port = 9000
advertised_address = "203.0.113.10"
maximum_allowed_sessions = 64

[server.accounts]
allow_auto_account_creation = false
accounts = ["kara:secret", "elara:another"]

[database]
shard_db_path = "yalain.sqlite"
```

| Key | Option | Notes |
| --- | --- | --- |
| `[server] world_name` | `--world-name` | |
| `[server] dat_files_directory` | `--dats` | empty: the file's folder |
| `[server] message_of_the_day` | `--motd` | Yalain only |
| `[server] world_database_path` | `--world-db` | Yalain only; empty or `pinned`: the pinned release |
| `[server] world_base_path` | `--world-base` | Yalain only |
| `[server] shutdown_interval` | `--shutdown-interval` | seconds |
| `[server] player_save_interval` | `--save-interval` | Yalain only; seconds |
| `[server] watchdog_seconds` | `--watchdog` | Yalain only; seconds, 0 for never |
| `[server] log_level` | `--verbose` | `debug` (or `trace`) logs at the debug level; others at `info` |
| `[server] status_address` | `--status` | empty: off |
| `[server] stop_at_console_end` | `--stop-at-console-end` | Yalain only |
| `[server] update` | `--update` | default `"off"`; `"patch"` or `"minor"` (section 3.10) |
| `[server] update_check_hours` | `--update-check-hours` | default 6 |
| `[server] update_warning_seconds` | `--update-warning-seconds` | default 300 |
| `[server] update_source` | `--update-source` | default empty: the repository the build came from |
| `[server.network] host` | `--bind` | |
| `[server.network] port` | `--login-port` | |
| `[server.network] world_port` | `--world-port` | Yalain only; default `port` + 2 |
| `[server.network] advertised_address` | `--advertise` | Yalain only |
| `[server.network] world_referral` | `--no-referral` when false | Yalain only |
| `[server.network] friends_service` | `--no-friends` when false | Yalain only |
| `[server.network] maximum_allowed_sessions` | `--max-sessions` | |
| `[server.network] maximum_allowed_sessions_per_ip_address` | `--max-sessions-per-address` | -1: unlimited |
| `[server.network] allow_unlimited_sessions_from_ip_addresses` | `--unlimited-sessions-from` | a list |
| `[server.network] download_bytes_per_second` | `--download-rate` | Yalain only; default 65536, 4096 to 262144. The floor hasn't been tried against a retail client, and at the top about a quarter of the packets were sent twice |
| `[server.accounts] allow_auto_account_creation` | `--listed-accounts` when false | |
| `[server.accounts] password_hash_work_factor` | `--secret-cost` | |
| `[server.accounts] accounts` | `--add-account` | Yalain only; a list of `name:ticket` |
| `[server.accounts] admins` | `--admin` | Yalain only; a list of account names, each raised only as `accounts` makes it |
| `[server.accounts] admin_commands` | `--admin-command` | Yalain only; a list of `command:group,group` |
| `[server.accounts] serve_admin_library` | `--serve-admin-library` | Yalain only |
| `[server.accounts] banned_names_path` | `--banned-names` | Yalain only |
| `[status] udp_ping` | `--no-status-ping` when false | the status ping (section 3.9) |
| `[status] hellos_per_minute`, `asks_per_minute`, `replies_per_second` | `--status-ping-hellos`, `--status-ping-asks`, `--status-ping-replies` | default 4, 4 and 50 |
| `[database] shard_db_path` | `--data` | accounts and characters in one file |
| `[database] in_memory` | `--memory` | Yalain only |
| `[federation] enabled` | `--federation`, with the hub's options | linked, and listed |
| `[federation] listed` | `--hub`, `--hub-slug`, `--hub-key-file` | Yalain only: listed alone |
| `[federation] hub_url` | `--hub` | `wss://` (the heartbeat goes to its `https://`), or `ws://` on this machine |
| `[federation] slug` | `--hub-slug` | |
| `[federation] key_file` | `--hub-key-file` | default `network.key` |
| `[federation] min_character_age_seconds` | `--min-character-age` | 0: seconds a character must have played before it sends to the network |
| `[federation] global_room` | `--global-room` | `general` (the default), `trade`, `lfg`, `roleplay`, or empty for none (section 8.4) |
| `[dat_overlay] path` | `--dat-overlay` | empty: the world's own `overlay` folder |
| `[dat_overlay] patching` | `--no-dat-overlay-patching` when false | `true`: a client that keeps overlays is sent the world's records |
| `[dat_overlay] records_per_minute` | `--dat-overlay-records-per-minute` | 60000; 0 for the original servers' 1,000 a minute, shared |
| `[ddd] enable_dat_patching` | `--enable-dat-patching` | `false`: a retail client lacking a world's overlay is refused |
| `[testing] add_characters`, `place_characters`, `character_money`, `make_characters` | `--add-character`, `--place`, `--money`, `--make-character` | Yalain only; lists |
| `[testing] game_time`, `hold_clock`, `echo_positions`, `run_seconds` | `--game-time`, `--hold-clock`, `--no-echo` when false, `--seconds` | Yalain only |

A relative path is read from the file's folder, not the working folder. Empyrean's keys that
Yalain has no setting for (its world pack and content overlay, its engine's caches and preloaded
landblocks, its WebSocket endpoint, its admin channel, the era, and the rest) are logged as warnings and ignored, so a file copied from an Empyrean world reads;
so is any key the server does not know.

The command line is read after the file, so an option given in both takes the command line's
value; an option given many times takes the file's values and the command line's. A setting the
file turns on cannot be turned off from the command line: change the file. `yalain.toml.example`
in the source tree's `yalain/server/` lists every key with its default.

### 3.7 How many can log in

The world holds at most 128 logins at once, as Empyrean does; `--max-sessions <n>` sets another
number. A login past them is refused before its account is checked: the client shows *The logon
server is full. Try again later.*, the September 2013 client's own line, and the connection is
closed. A login that leaves frees its place at once. An admin's account with its right ticket is
let in past this limit.

`--max-sessions-per-address <n>` limits the logins one address holds at once in the same way
(Empyrean's default, and Yalain's, is no limit). Players behind one router share an address, so a
limit of one keeps a household to one player. `--unlimited-sessions-from <ip>` exempts an
address (yours, say) from that limit, but not from the world's.

Any refused login, a wrong ticket among them, is answered and its connection closed at once.

### 3.8 The status endpoint

With `--status <ip>:<port>` (for example `--status 127.0.0.1:9100`), the server answers over HTTP:

- `GET /health`: `ok`, for a monitor or a health check;
- `GET /status` (also `GET /v1/world`): the world as JSON: `world_name`, `server` (`yalain`),
  `version`, `game` (`auberean`), `uptime_seconds`, `state` (`online`, or `maintenance` while a
  shutdown counts down), `players_online` (characters in the world), `logins`, `max_logins`,
  `connections`, `saves_failing` (true while the database will not take saves: watch this one) and
  `websocket_url` (section 4.1; `null` with the endpoint off).

**If saves fail** (the disk is full, the file is read-only or another program has it locked), nothing is
lost while the server runs: it holds what it could not write and tries again every five seconds. The log
says `SAVES ARE FAILING` at once and every minute, `saves_failing` is true, and every player is told. Fix
the cause and it catches up by itself. A server told to stop while saves fail tries for 30 seconds more,
then names the characters it lost and exits with a failure, and no staged update is installed.

**If the world panics** (a bug in the server), it stops. The log says `THE WORLD PANICKED`, with the place
and the calls above it; every player is told. What finished before the panic is written, and nothing after:
each character goes back to its last save, and the server exits with a failure, so a service with
`Restart=on-failure` comes back by itself. A staged update isn't installed. Please report the log's lines.

**A character that will not read** is refused at the door and never written over; the log names it. The
player reads the client's own words for why. Its rows in `world_state` and `play_state` stay as they were,
for you to restore a backup or to look at. A world record that will not read (`consignments`,
`allegiances`) is copied to `<name>.unreadable` in `world_record` before the world starts it empty.

It has **no authentication**. Keep it on `127.0.0.1` or a private network, and do not open its
port in the firewall. It is off by default. A web page on an origin in
`server.websocket.allowed_origins` may read `/status` too, so it can find the WebSocket address; no
other page can.

### 3.9 The status ping

The login port also answers Empyrean's status ping: a launcher, or the Dereth Network's check from
outside, asks in two short UDP steps, with no login, and learns whether the world is open (or
shutting down), how many are in it, the server (`Yalain`) and its version, and the world's name.
It needs no port of its own. It is on by default; `[status] udp_ping = false` (or
`--no-status-ping`) turns it off. Each address gets four first steps and four second steps a
minute, and the server sends at most 50 answers a second in all (`[status]`'s
`hellos_per_minute`, `asks_per_minute` and `replies_per_second`); past them nothing is answered,
and the log counts what was dropped once a minute. The first step's answer is smaller than the
question, so the ping cannot be used to flood someone else.

### 3.10 Updates

Each Yalain release says what upgrading to it involves, in its `release.json`'s `upgrade`: the
oldest version that can upgrade straight to it, whether the database's layout changes, whether it
serves a newer world database, configuration keys it removes or needs, and notes. A launcher
hosting the world reads it before updating, and so does the server's own updater. `yalain
--release-facts` prints what a program is:

```
{"version":"0.1.0","target":"x86_64-unknown-linux-gnu","databases":{"store":2},"world_database":"v0.23.0"}
```

A patch release never changes the database's layout or the configuration.

**Automatic updates.** The server can keep itself up to date, within limits you set, and undo an
update that does not come up. It is off until you turn it on:

```toml
[server]
update = "patch"          # "off" (the default), "patch" or "minor"
update_check_hours = 6    # how often it looks
update_warning_seconds = 300   # the in-game countdown before it restarts
```

| `update` | What it installs by itself |
|---|---|
| `"off"` | Nothing. |
| `"patch"` | Patch releases of the version it runs (`0.1.0` to `0.1.3`). These change the program alone: no migration and no configuration change (the world database's own patches are fetched). |
| `"minor"` | Also minor releases of the same major version (`0.1.3` to `0.2.0`), with their database migrations and newer world databases. |

Never installed by itself, only reported in the log: a major release, a pre-release, a release that
needs a configuration key you must set, a release that cannot be upgraded to straight from the
version you run (the intermediate one is installed first, on a later check), and a release that
already failed here.

**What happens.** A minute after start-up, and then every `update_check_hours`, the server asks
GitHub for the releases of the repository it was built from, or of the one `update_source` names:
another GitHub repository (`https://github.com/<owner>/<name>`), or a mirror that serves GitHub's
releases API for one (`https://<host>/repos/<owner>/<name>`). When one qualifies, with the world
still running, it:

1. downloads this platform's archive, `release.json` and `SHA256SUMS`, and refuses the release if a
   SHA-256 does not match. That is the only check of where the files came from: releases are not
   signed, so the log says the release was checked by SHA-256 only;
2. unpacks it into `.yalain-update/` beside the server, runs the new `yalain` once to confirm it
   runs on this machine and is what the release declares, and, for a release that serves a newer
   world database, has the new `yalain` fetch it (`yalain world fetch`);
3. starts the shutdown countdown (`update_warning_seconds`), which players see as the usual shutdown
   lines (also written to the log). `cancel-shutdown` cancels the update too; the next check tries
   again;
4. when the countdown ends and everything is saved: backs up the database
   (`yalain.sqlite.backup-update-v<old>-v<new>-<UTC timestamp>`, the newest three kept), moves each
   replaced file aside as `<name>.previous-v<old>`, puts the new files in place, and starts the new
   server for a health check: it must open and migrate the database, load the world, open its
   listeners and report itself ready;
5. passes: the new server starts for real. Fails: the old files and the backed-up database are put
   back, the failure is logged and recorded in `.yalain-update/state.json` (so that release is not
   tried again automatically), and the old server starts again.

A server stopped with Ctrl-C or a signal while an update waits installs nothing.

**The folder must be writable** by the account the server runs as: the updater renames and writes
the files beside `yalain`, and writes `.yalain-update/`.

**Running as a service.** The server restarts itself into the new release; nothing needs to be
configured for it. On Linux and macOS the process replaces itself with the new program, keeping its
process ID, so a service manager sees the same main process keep running. On Windows, which cannot
replace a running process, the old process starts the new server as its child, in the same console,
and waits for it, holding no files, ports or databases; what is typed at the console is passed on
to the new server.

**By hand**, with any `update` setting:

```
yalain update --check [--config yalain.toml] [--policy patch|minor]
yalain update --apply [--config yalain.toml] [--policy patch|minor]
```

`--check` lists every newer release, says why each would or would not be installed, and what
installing the chosen one involves (migrations, the world database's fetch, configuration changes).
`--apply` does steps 1 to 5 on a **stopped** server (it refuses while the server's login port is in
use) and leaves the server stopped, so you or the service manager start it. It needs the world's
`yalain.toml` to start the new server for its health check. Without `--policy`, the policy is
`update`, or `minor` when `update` is `"off"`.

**Going back.** A release that fails its health check is undone by itself. To go back later: stop
the server, put `yalain.previous-v<old>` back as `yalain`, and put the backup made before the update
back as the database (remove its `-wal` and `-shm` files first). What was played since the update
is lost. A release that needs another first, or a major release, is installed the same way by hand:
stop the server, replace `yalain`, run `yalain world fetch` if the release serves a newer world
database, and start it.

**A world the launcher hosts** keeps `update = "off"`: the launcher updates it, and refuses to start
it otherwise.

---

## 4. Ports and the firewall

The server listens on:

| Protocol | Port | What |
| --- | --- | --- |
| UDP | `--login-port` and the port above it (9000 and 9001) | the login: the character list, and the world too with `--no-referral`; the status ping (section 3.9) on the first |
| UDP | `--world-port` and the port above it (9002 and 9003) | the world, where the login sends each client |
| TCP | `--login-port` (9000) | the friends service: friends, ignore lists, away, and tells to `Name@world` |

Both ports must be even. Open all five inbound for players on other machines, and forward them on
your router for players over the internet. With `--no-referral` no client is sent to the world's
ports, though the server still listens on them.

To serve other machines, listen on every address and tell clients where the world is:

```
yalain --dats <install folder> --bind 0.0.0.0 --advertise <your public IPv4 address>
```

The login sends each client to the world at the advertised address. A server listening on every
address cannot tell which one players reach it by, so with `--bind 0.0.0.0` (or an IPv6 address)
it will not start without `--advertise`. Behind a router, advertise the public address players
use; on a home network, the server's LAN address. Only an IPv4 address can be advertised. With
`--no-referral`, nothing is advertised and `--advertise` is not needed.

**The friends service** is on unless you give `--no-friends`. Clients reach it at the login's
address and port number, over TCP. With `--no-friends` nothing listens on TCP, and players are told
at each entry that the friends list cannot be reached; nobody is online to friends, on this world
or another. If something else already holds the TCP port, the server stops at start and says so.

The Dereth Network needs no inbound port: the heartbeat and the link both go out from the server
to the hub.

With the WebSocket endpoint on (section 4.1), open its TCP port too when it serves `wss://` itself.
Behind a reverse proxy, open only the proxy's port (443), and keep the endpoint on loopback or a
private address that only the proxy can reach.

### 4.1 WebSocket connections

A browser cannot send UDP. With `[server.websocket]` enabled, the server also accepts each game
datagram as one WebSocket message: the same packets, the same sessions and the same world. It is
off by default. The settings are Empyrean's `[server.websocket]`, key for key, read by the same code,
so [Empyrean's guide](../empyrean/SETUP.md#47-websocket-browser-clients) serves for this one: its
reverse proxy and Cloudflare recipes apply as they stand, on the same keys.

| Key | Default | Option | What it does |
|---|---|---|---|
| `enabled` | `false` | `--websocket` | Listen for WebSocket connections. |
| `listen` | `"0.0.0.0:9443"` | `--websocket-listen` | The address and port. |
| `tls_certificate`, `tls_private_key` | empty | `--websocket-tls-certificate`, `--websocket-tls-private-key` | PEM files for `wss://`. |
| `behind_tls_proxy` | `false` | `--websocket-behind-tls-proxy` | A proxy in front terminates TLS. |
| `trusted_proxies` | `[]` | `--websocket-trusted-proxy` | Proxies whose `X-Forwarded-For` names the client. |
| `allowed_origins` | `[]` | `--websocket-allowed-origin` | The web pages allowed to connect, by origin; empty refuses every page. |
| `maximum_connections_per_ip_address` | `4` | `--websocket-max-per-address` | Connections open at once from one client; `-1` is unlimited. |
| `idle_timeout` | `60` | `--websocket-idle-timeout` | Seconds without a message before a connection is closed. |
| `public_url` | empty | `--websocket-public-url` | The URL reported on `/status` and told to the hub; empty derives it from `listen`. |

**The connection must be encrypted.** The game protocol is not, and the login carries the account's
password. So set `tls_certificate` and `tls_private_key`, or run behind a reverse proxy that
terminates TLS, on loopback or with `behind_tls_proxy = true`. Plain `ws://` on any other address is
refused: the server stops at its start and says why, as it does for half a certificate, a `listen`
that is no address or a trusted proxy that is none. Each stops it with exit code 1, a `listen` port
another program holds included: no other game port would help.

**One connection reaches the whole world.** A player's page opens one connection and needs one URL.
It logs in and is sent to the world over that same connection, so the world's UDP ports do not have
to be open for it, and the limit counts a player once. Each player's address is the connection's, or
behind a proxy in `trusted_proxies` the one the proxy names.

**The friends service rides the WebSocket too,** for a player in a browser on `wss://` or on this
computer: no port of its own to open. On a plain `ws://` from elsewhere its ticket would go in the
clear, so that player goes without friends.

With the Dereth Network linked (section 8.2), the world tells its hub the URL too, and the hub lists
it while the world is linked. The hub takes only a `wss://` URL, or a plain one to `127.0.0.1` for a
world and its players on one machine; any other is left out, and the log says why.

---

## 5. Accounts

A client logs in with an account name and a ticket, which stands for the password. The world
admits it by one of two rules:

- **Open accounts** (the default). Any new name logs in, and its account is made the first time
  it does, with that ticket. From then on the account logs in with that ticket only; a login with
  another is refused. An account made with no ticket takes the first one it logs in with.
- **Listed accounts** (`--listed-accounts`). Only accounts that exist and have a ticket log in,
  and only with their own ticket. Make each one with `--add-account <name>:<ticket>` (the option
  may be given many times; the server makes them and carries on running). An account that exists
  already is left as it is, with a warning in the log.

The store keeps a bcrypt hash of each account's ticket, never the ticket, at cost 8, as Empyrean
keeps its passwords. `--secret-cost <n>` sets another cost, from 4 to 31: each step up doubles the
time a login's check takes. An account kept at another cost, or kept as an earlier Yalain kept it
(a salted SHA-256), is kept again at the world's cost the next time it logs in with its ticket,
and the log says so. A world that turns `--listed-accounts` on later admits its players with the
tickets they already use. Account and character names are matched without regard to case, and an
account name is at most 50 characters. The console makes accounts and changes their tickets while
the world runs (section 6.1); there is no command to remove an account.

Five refused logins from one address within a minute (a wrong ticket, or a name a world of listed
accounts does not have) make the world refuse that address's logins for a minute, unchecked, with
the same answer as any refusal. Other addresses are not affected. An IPv6 address counts by its
/64. The friends service holds back refused tickets the same way. Players who share one address (a
household behind one router, say) share the lockout: one player's wrong tickets hold back the others
for that minute.

An account holds up to six characters. A deleted character stays in the list, grayed out, for an
hour.

The friends service checks its tickets by the same rule. A client's friends ticket carries the
login's account and ticket, so a player who can log in is admitted to the friends service too, and
one who cannot is refused there as well.

`--add-character` (a testing option) makes the account it names when it is missing, with no
ticket: it takes the first ticket it logs in with. A world of listed accounts makes no account
for it; make the account first with `--add-account`, in the same command if you like.

**Admins.** Asheron's Call 2 has one kind of admin and no levels. An account either is an admin's
or is a player's; every new account is a player's, and only you raise one: `set-accountadmin <name>`
at the console, `accountcreate <name> <ticket> admin`, or `--add-account <name>:<ticket> --admin <name>`
(`[server.accounts] accounts` and `admins`) at start. No account is made an admin for being the first.

`--admin` raises a name only on the start that makes its account from a ticket in the same
configuration: only then is the account's secret surely yours. It does nothing for an account already
there, and logs a warning, because on an open world anyone may have logged in with that name first and
holds it with their own ticket. That goes for a name whose account a later start lists, and for one
named ahead of its account. To raise an account that already exists, check it is yours and use
`set-accountadmin <name>` at the console. So the list never undoes the console: `set-accountadmin <name>
off` lasts through restarts, and taking a name off the list changes nothing.

**Admin characters.** An admin plays an ordinary character like anyone else, and keeps a separate
admin character for admin work: `set-characteradmin <name>` at the console makes one of an admin
account's characters an admin character (`off` undoes it), and it takes effect when the character
next enters the world. A client that can ask for one at creation gets one on an admin's account; any
other account is refused with the client's own answer. An admin character is drawn from the game's
admin template, so the client gives it the game's admin commands. It stays an admin character only
while its account is an admin's: on any other account it enters as a player.

What makes an admin stays yours: `set-accountadmin`, `set-characteradmin`, `set-accountgroups` and
`accountcreate ... admin` work only at the console, and an admin in game cannot give another admin's
account a new password.

**Named groups.** Eight commands act on accounts or the whole server: `ban`, `boot`, `fingerchar`,
`fingerid`, `fingeraccount`, `serverparam`, `userlimit` and `dumpadmintable`. Each is open to every
admin unless you keep it to named groups with `--admin-command ban:moderators,leads` (`[server.accounts]
admin_commands`). `set-accountgroups <account> moderators,leads` puts an account in its groups, and
`set-accountgroups <account>` takes it out of them all. What does the same work follows the same
groups: `set-accountpassword`, `unban` and `banlist` follow `ban`, and `accountget` and `accountlist`
follow `fingeraccount`. The console is never refused.

**A retail client's admin commands.** The original client sends its admin commands encrypted, under a
key only the original servers could read. `--serve-admin-library` (`[server.accounts]
serve_admin_library`) serves an admin account's client a script library that sends them in the clear,
through the login's data download: about 7 MB, once, which holds its login for about two minutes at
the default `--download-rate`. That client must run with `-rodat off` and on a copy of the install kept
for this world, since the download writes into its data files. Auberean sends them in the clear already,
so it tells the server it holds the library and is sent none of it; a release before this one takes the
download at every login. Without the option, the `@` commands work from any client.

**Upgrading from access levels.** A database from before this keeps its accounts. One at the highest
level (5, Admin) with a ticket becomes an admin. One in between (Advocate, Sentinel, Envoy or Developer),
or at the highest with no ticket yet, becomes a player and is named in a warning at the first start, for
you to raise if you want it to be an admin.
The file is backed up first, as for any upgrade.

**Bans.** A banned account cannot log in until its ban ends: the client says the account is
banned, at its friends login and at its game login. A ban that has run out is lifted at the
account's next login.

---

## 6. The console

When the server runs in a terminal, each line typed into it is a command. The first word is the
command, in any case. Answers appear in the log (section 7).

| Command | What it does |
| --- | --- |
| `reload` | compile the world database again and take it while players are in the world |
| `shutdown` or `shutdown <message>` | count down (two minutes unless set otherwise), telling every player in the world, then stop (section 3.3) |
| `cancel-shutdown` | stop the countdown, and tell the players |
| `set-shutdown-interval <s>` | how long the next `shutdown` counts down, 0 to 99999 seconds |
| `stop-now` or `stop-now <message>` (or `stop`, `exit`) | stop the server at once, saving everyone in the world |
| `save-world` | save every character in the world now; the world goes on |
| `hero <name>` | make a character a hero |
| `hero <name> off` | make it no longer one |
| `teleport <name> to <where>` | send a character in the world through portal space (section 6.2) |
| `place <name> to <where>` | set where a character out of the world enters next (section 6.2) |
| `make-character <file>`, `apply-build <file>`, `deletecharacter <name> [now]`, `forcelogoff <name>`, `export-character <name> <file>`, `import-character <file> <account> [new name]` | the operator's characters (section 6.2) |
| `dn`, `dn block ...`, `dn unblock ...`, `dn verify <token>`, `dn help` | the Dereth Network (section 8.3) |
| `accountcreate`, `accountget`, `accountlist`, `set-accountpassword`, `set-accountadmin`, `set-characteradmin`, `set-accountgroups`, `ban`, `unban`, `banlist`, `gag`, `ungag`, `boot`, `gamecast` | accounts, moderation and messages to everyone (section 6.1) |
| `help` or `?` | list the commands |

A word that is no command is answered with a warning that lists them. Each command's answer is
one line of the log, ending in a tag a program can read (section 6.3).

A character's name with spaces is written as it is (`hero Hand Of Doom`), or between double
quotes where the command needs to tell it apart from what follows (`hero "Kara Off"`,
`teleport "Walks To Arwic" to Kara`). A file's path with spaces goes between double quotes too.

**`reload`** compiles the database the server was started with (the `--world-db` folder, or the
pinned release) on a thread of its own, so the world plays on meanwhile, then takes it with players
in the world: templates, loot, stores, quests, what people say, the story's scenes, effects and
crafting, the `--banned-names` file, the areas of player conflict, placed objects, camps and veins.
Players see a new object appear, a moved or changed one appear where it now stands, and a removed
one vanish. An object a player is using is let go first: a chest open is closed, a use of a door
or chest is answered. A camp's creatures stay when only how many it keeps or how soon it replaces
them changed; otherwise they leave and the camp makes new ones. A creature fighting a player
finishes its fight first, then leaves, and nothing replaces it. A vein left as it was keeps its
uses, or its coming back once spent; one taken away while a player mines it answers the player.
What players carry, corpses, pets and anything lying on the ground are never touched. The log says
which tables were reloaded, with a line each on the placed objects, camps and veins. A database
that does not compile leaves the world as it was, and the log says why.
A second `reload` while one runs is refused.

**`hero <name>`** makes a character a hero. Only a hero earns focus, its third vital, by killing
creatures worth it; a hero's greatest focus is 100 and whatever its own focus adds. Made a hero,
the character's focus is set to its greatest at once. Nothing in play makes a hero: this command
does. It works on a character in the world (its player sees the change at once) and on one out of
it (the change is stored for its next entry); the log says which. `hero <name> off` takes it away,
leaving its focus as it was.

### 6.1 Accounts and moderation

These are Empyrean's console commands, with its answers. Each change is saved at once, and every
command's answer is logged (in the form of section 6.3 at the console, with who gave it in game); a
ticket typed into one is never logged.

| Command | What it does |
| --- | --- |
| `accountcreate <name> <ticket> [admin]` | make an account, a player's unless given the word `admin` |
| `accountget <name>` | an account's id, whether it is an admin's, and any ban |
| `accountlist` | every account, the same |
| `set-accountpassword <name> <ticket>` | give an account a new ticket; the old one no longer logs in |
| `set-accountadmin <name> [on\|off]` | make an account an admin's, or a player's again; a logged-in account has it at once |
| `set-characteradmin <name> [on\|off]` | make a character of an admin's account an admin character, or a player again |
| `set-accountgroups <name> [group,group...]` | put an account in its named groups (none when none are given) |
| `ban <account> <days> <hours> <minutes> [reason]` | keep an account out for that long; a logged-in account is booted |
| `unban <account>` | lift a ban |
| `banlist` | the accounts banned now, who banned them, until when and why |
| `gag <character>` | stop a character's chat for five minutes of its time in the world |
| `ungag <character>` | let it talk again |
| `boot char <name>`, `boot account <name>`, `boot iid <0x id>`, each with `, <reason>` if you like | end a login; its character leaves the world as when a client goes away |
| `gamecast <message>` | show every player in the world `Broadcast from System> <message>` |

A gagged character is told so, and then says, tells (in the game and through the friends
service), emotes and speaks into rooms to nobody. Its gag counts down only while it is in the
world; one gagged out of the world, or that leaves, keeps the rest for when it comes back. The
player is told when the gag ends.

A booted player's client shows that its character was booted from the world, and its connection
closes a second later. Its character lingers as when a client goes away; to take it out of the
world at once, use `forcelogoff` (section 6.2).

`gamecast` said in game by an admin shows the message as from their character
(`Broadcast from Kara> Back in five minutes.`).

**In game.** Any character of an admin's account can say the same commands after `@` (`@boot char
Kara, spamming`), aloud or into a chat room. Nobody hears the line, and the answer appears in the
speaker's chat window. A player's line beginning with `@ban` is just said, and the attempt is logged. Any player can say
`@passwd <old> <new>` to change their own account's password, at most once every five seconds; the
old one must match. An admin character's own commands (`/boot`, `/ban`) reach Yalain from Auberean, and from
a retail client given the library above; a retail client without it sends them encrypted, and they are
not read.

### 6.2 Characters

These commands make, move and carry characters for testing a world, as Empyrean's console does for
Lyceum. Each answer appears in the log.

**`teleport <name> to <where>`** sends a character in the world through portal space, as a recall
goes. `<where>` is another character in the world (it arrives beside it, facing it), `place <name>`
(a place the world database names, such as a recall's destination), or a cell in hexadecimal and a
point in it with a heading in degrees if you like (`teleport Kara to 77B80006 4.4 100.8 182.5 90`).

**`place <name> to <where>`** sets where a character out of the world enters next, with the same
`<where>` as `teleport` (beside a character in the world, `place <name>`, or a cell and point).
It works on a character that has played and on one that has not. A character in the world is
refused: teleport it instead.

**`make-character <file>`** (or `--make-character <file>` at start) makes the character a recipe
file describes, with a build, before it first plays:

```toml
account = "builder"
name = "Hand Of Doom"
race = 1            # 1 human, 2 lugian, 4 tumerok, 8 drudge, 16 empyrean
sex = 4096          # 4096 or 8192
level = 20          # or experience = 250000
money = 5000
hero = true
place = { cell = 0x77B80006, x = 4.4, y = 100.8, z = 182.5, heading = 90 }
skills = [{ skill = 15, rating = 10 }]
items = [{ template = 0x4700051C, quantity = 1 }]
quests = [{ quest = 0x800000E0, phase = "done" }, { quest = 0x800000E1, phase = 2 }]
```

Only `account`, `name`, `race` and `sex` are needed. The character is made as a player reaching
the build would be: the level's experience and the skill credits its levels give; each skill
trained with those credits and raised to its rating with the experience, by the same rules as a
player's; the money, the items in its pack (each a template the world serves), and the quests done
or under way at a phase. A missing account is made on an open world, as with `--add-character`.
A build that does not hold (a skill the level may not train, too little experience to raise one,
an unknown item or quest) is refused and nothing is kept. A character made before and never played
is given its build again from the start; one that has played is refused: give it the build with
`apply-build`, or delete it now and make it again.

**`apply-build <file>`** gives a build to a character out of the world, played or not, keeping
everything the recipe does not name. Its experience is raised to the recipe's level or
experience, with the skill credits and health and vigor of the levels it gains, as a player
reaching them gets; each skill is trained and raised to its rating by a player's rules; its pack
holds at least the recipe's quantity of each item (what it lacks is added); its money, quests,
hero and place are set as the recipe says. Nothing is lowered: a level or skill already above the
recipe's stays. Given again, the same recipe adds nothing. `race` and `sex` must be the
character's own. A character in the world is refused: it is given its build out of the world.

**`deletecharacter <name>`** deletes a character as its player would: it stays in its account's
list, grayed out, for an hour, then goes. A character in the world is booted first.
**`deletecharacter <name> now`** deletes it at once, with no hour: its name is free for a new
character straight away. A character in the world is taken out of it at once and its player
booted; a character already deleted and grayed out goes at once too. A player's own deletion
always keeps the hour.

**`forcelogoff <name>`** (or `forcelogout`, Empyrean's other word) takes a character out of the
world at once, without the leaving a logging off plays: it is saved where it stands, those beside
it see it go, and its player's login is ended. Nothing lingers, even a character a crashed client
left in the world, so its account can enter again straight away. A character not in the world is
refused. Use it to start a playtest over without deleting the character.

**`export-character <name> <file>`** writes a character to a new file (never over another), to
carry it to another world. Beside the character itself, the file holds its build, readable, in a
recipe's own words (no account): its level (or its experience, past a level's), money, hero and
place, every skill it has trained with its rating now, its items and its quests:

```json
"build": {
  "name": "Hand Of Doom", "race": 4, "sex": 8192, "level": 20, "money": 5000, "hero": true,
  "place": { "cell": 2008547334, "x": 4.4, "y": 100.8, "z": 182.5, "heading": 90.0 },
  "skills": [{ "skill": 15, "rating": 10 }],
  "items": [{ "template": 1191183644, "quantity": 1 }],
  "quests": [{ "quest": 2147483872, "phase": "done" }, { "quest": 2147483873, "phase": 2 }]
}
```

Given an account, it reads as a recipe for `make-character`. Importing reads the character, not
the build. **`import-character <file> <account> [new name]`** brings one onto an
account as a new character, with what it held: its level, skills, items, quests and money. It
keeps its name unless you give another, and a name that is taken is refused. Its friends, its
allegiance and its place on the Dereth Network stay behind.

### 6.3 Answers for programs

A program that drives the console (a world builder's playtest, a launcher) reads the answers from
the log. These lines end in a tag of a fixed form, which this section documents as an interface:
the words before the tag are for people and may change; the tags do not.

```
<time>  INFO yalain: <what was done> [ok <command>]
<time>  WARN yalain: <why nothing was done> [refused <command>]
<time>  INFO yalain: in the world at <cell> ... [entered "<name>" <id>]
<time>  INFO yalain: left the world at <cell> ... [left "<name>" <id>]
<time>  INFO yalain: listening on ... [listening]
<time> ERROR yalain: binding the ports: ... [ports-taken]
```

- Each of these lines comes from `yalain` (` yalain: ` after its time and level) and ends with
  `]`; its tag runs from the line's last ` [` to that `]`. The words may hold brackets of their
  own (`Kara is on its way to 77B80006 [4.4, 100.8, 182.5] [ok teleport]`), never after the tag.
- `<command>` is the command's own name, in small letters, whichever word was typed: `stop` and
  `exit` answer as `stop-now`, `?` as `help`; a word that is no command answers as itself,
  refused.
- `ok` means the command did what it says; `refused` means it changed nothing (or, for a line
  that does not read, it shows how the command is written).
- Every console command answers with one such line, except `dn`. The commands of section 6.1
  answer in Empyrean's words under their own word (`Booting character Kara. [ok boot]`,
  `Cannot boot "Kara" because that character is not currently online ... [refused boot]`,
  `Broadcast from System> Back soon. (shown to 2 players in the world) [ok gamecast]`); one whose
  answer is a list (`accountlist`, `banlist`) tags each line.
  `forcelogoff` answers `<name> taken out of the world and booted [ok forcelogoff]`, then the
  character's `[left ...]` line has already been logged. `reload` answers when the database has been taken
  or refused (`world database ...: reloaded: ...; needs a restart: ...; unchanged: ... [ok
  reload]`); the lines after it, about placed objects, camps and veins, are details.
  `--make-character` at start answers as `make-character` does.
- `<name>` is the character's name as the world shows it, between double quotes (a name never holds
  a quote or a bracket), and `<id>` its id in hexadecimal (`0x` and small letters).
- `[listening]` comes once, when the server is ready for clients and its console commands; the
  answers of `--make-character` come before it.
- `[ports-taken]` comes when another program holds one of the server's ports (a login or world
  UDP port, or the friends service's TCP port): the server stops before it starts, with **exit
  code 3**, so a program can try other ports. Every other failure at start exits with 1.

---

## 7. Logs

The server writes its log to **standard output**, one line each, with the time in UTC, the level
and where it came from. Errors that stop it at start go to standard error. To keep a log file,
redirect both:

```
yalain --dats <install folder> > yalain.log 2>&1
```

or let a service manager collect them (section 3.4).

The level is `info`. `--verbose` raises it to `debug`, which adds every connection event and much
more. No environment variable changes it.

At `info`, the start-up lines show what the server read from the install, which world database it
compiled and how much of each kind, where it listens, and, with the Dereth Network on, the hub it
reports to and links to. What the world
does not handle yet is logged as `not handled yet: ...` as it happens, and counted again as the
server stops. No line holds the hub's key.

---

## 8. The Dereth Network

A world can join the Dereth Network in two steps:

- **Listed** (`--hub`): the hub lists the world, with how many are playing, where the launcher's
  Asheron's Call 2 worlds are shown.
- **Linked** (`--federation`, with `--hub`): the world also holds a link to the hub, so its players
  can reach characters on other worlds of either game: tells, and friends who show online.

The hub's own rules, for operators, are in [the hub's specification](../docs/network/05-hub.md).

### 8.1 Listing the world

**Register it** with the hub once, as an Asheron's Call 2 world: on the hub's site
(https://dereth.network), or by the hub's operator (`dereth-network-hub register world.json`). The
registration names the world's slug and name, its game, `"game": "auberean"`, and its endpoint: the
address players reach and the **login port**. Section 2.4 of
[the hub's specification](../docs/network/05-hub.md#24-registering-and-claiming) lists every field.

**Keep the key.** The hub answers with the world's key, `drth_` and 43 more characters, shown
once. Put it alone in a file (whitespace around it is ignored) that only the server's user can
read. The key is the world's identity at the hub: whoever holds it can speak for the world.

**Verify it.** A newly registered world is pending, and its key works only to verify it. Verify it
with `dn verify` (section 8.3), or with a page on the endpoint's own host, as section 2.5 of
[the hub's specification](../docs/network/05-hub.md#25-proving-the-operator-runs-the-world) says.

**Start the server** with the three options:

```
yalain --dats <install folder> --bind 0.0.0.0 --advertise <address> \
  --hub https://hub.dereth.network --hub-slug dawnsong --hub-key-file network.key
```

| Option | What it is |
| --- | --- |
| `--hub <url>` | the hub's address: `https://`, or plain `http://` only for a hub on this machine (the key goes with every post) |
| `--hub-slug <slug>` | the world's slug, as it was registered: 2 to 40 small letters, digits and single inner hyphens |
| `--hub-key-file <file>` | the file holding the key |

The three go together. A key, slug or address the server cannot use stops it at start, saying why.

The server posts a heartbeat to the hub at start, then once a minute, at once when a shutdown
starts (the world is then in maintenance), and as it stops (offline). Each says how many characters
are in the world, the server's version and its game, so a world registered without a game is put
right by its first heartbeat. A heartbeat the hub refuses, or that does not arrive, is logged, and
the world plays on; the next goes a minute later.

### 8.2 Linking the world

Add `--federation`:

```
yalain --dats <install folder> ... --hub https://hub.dereth.network --hub-slug dawnsong \
  --hub-key-file network.key --federation
```

The world then holds a WebSocket link to the hub (`wss://` for an `https://` hub), beside the
heartbeat. Over it:

- characters on the network tell, and are told by, characters on other worlds (`/tell
  Name@world, text`);
- they list friends on other worlds (`/friend add Name@world`) and see them come online and go;
- each character on the network is listed in the network's character directory, with its level,
  race, sex, health and vigor, and its allegiance;
- the network's Global room is carried in one of the world's chat rooms (section 8.4).

What linking needs:

- **The world's federation on at the hub.** The hub turns it on when the world is verified, if
  its slug can join the network; its operator can turn it off and on again. While it is off, the
  hub refuses the link: the server logs that and tries again a minute later.
- **A slug that can join the network:** 2 to 12 small letters, digits and inner hyphens, and not
  one of the network's reserved names. A longer or reserved slug stops the server at start with
  `--federation`; it can still be listed without it.
- **The friends service** (no `--no-friends`). A character is online to other worlds only while it
  is in the world with the friends service connected.

The link reconnects by itself, with growing waits, when it drops. The world plays on without it.

**Characters start off the network.** Each player puts a character on by saying, in chat, without
a slash:

| Said | Does |
| --- | --- |
| `@dn on` | put this character on the network, as `Name@slug` |
| `@dn off` | take it off |
| `@dn` | its status: on or off, the hub connected or not, its friends on other worlds |
| `@dn link`, `@dn unlink` | link it to the player's Discord account, or undo that |
| `@dn friends`, `@dn friends add <Name@world>`, `@dn friends remove <Name@world>` | its friends on other worlds |
| `@dn squelch <world>`, `@dn unsquelch <world>` | squelch a whole world for it, or undo that |
| `@dng <text>`, `@dn global <text>` | say a line in the Global room, from wherever it is talking |
| `@dn channel create`, `invite`, `join`, `leave`, `members`, `list` | channels across worlds |
| `@dn help` | the commands |

The line is heard by nobody, and the world answers in the player's chat window. The setting is
kept per character. Friends, presence and tells within the world need none of this: they work for
every player with the friends service.

### 8.3 The console's `dn`

| Command | What it does |
| --- | --- |
| `dn` | the world's link (connected, connecting or not connected), how many worlds are online, how many messages wait for the hub, and what is blocked |
| `dn block <world>` or `dn block <Name@world>` | block a world, or one character of it, for everyone on this world (`dn admin block ...` too; `@dn admin block ...` in game, from an admin) |
| `dn unblock <world>` or `dn unblock <Name@world>` | lift a block |
| `dn admin global`, `dn admin global <general\|trade\|lfg\|roleplay\|off>` | where the world carries the Global room, or ask the hub to carry it there (`@dn admin global ...` in game, from an admin; section 8.4) |
| `dn verify <token>` | verify this world with the token the site gave you |
| `dn help` | list these commands |

A block works both ways and takes hold at once: what the blocked world or character sends is
refused (its tells are answered as if everyone here were offline), and what this world's
characters send it is refused too. Lifting it gives back what it took. Blocks are kept in the
store, across restarts.

**Verifying the world.** The site gives the world's operator a token (`dnv_...`) to prove the
world is theirs. Type `dn verify <token>` at the console, or say `@dn verify <token>` in game from
an admin's account. The world
asks the hub, which sends a challenge to the world's login port; the world answers it over
HTTPS, so the login port must be reachable from the internet. One line then says how it went: the
world is verified and on the network, or why not. A second `dn verify` before the hub answers is
refused.

Without `--federation`, every `dn` command, `dn help` among them, answers *This world is not on
the Dereth Network.*

### 8.4 The Global room

The Dereth Network's Global room is one room shared by every world that carries it, of either game.
A world carries it in one of its own chat rooms, whose players then hear the other worlds' lines
there as that room's own. Each line in that room from a character on the network goes to the other
worlds. It is on from the start, in General; turn it off, or move it, as you like.

`[federation] global_room` (`--global-room`) names the room:

| `global_room` | carried in |
| --- | --- |
| left out, or `"general"` | General |
| `"trade"` | Trade |
| `"roleplay"`, `"lfg"` | PvP |
| `""` | none: the world carries no Global room |

Anything else stops the server at start. The names are the same on every world of the network, and
Asheron's Call 2 has no LFG or Roleplay room, so both mean PvP, the room nobody here needs. No
recorded session of the game ever said a line there. General is where a world's players already
talk, so another world's lines mix with theirs and are shown as `[General]`. A quiet world may
like that; a busy one may prefer `"roleplay"`, where they read `[PvP]`, and its players can filter
them apart. New characters see all of these rooms in their main chat window.

**The hub has the last word.** The setting is read until the hub keeps a room for the world: a hub
that keeps none is told this one, once, and keeps it. After that, the setting is not read (the log
says so, once, if it differs), so to move the room, say `@dn admin global trade` in game as an
admin, type `dn admin global trade` at the console, or use the hub's operator page. `off` carries it
nowhere. The world keeps the hub's last word, so it carries the room there while the hub is away.

**What leaves the game.** A hub may mirror the Global room, and the world's own rooms, into Discord,
and a linked person may say a line there as its own character; the world says that line only under
the rules a character's own line meets. Tell your players, since their lines in that room leave the
world and stay in those channels.

---

## 9. Connect a client

Players need their own Asheron's Call 2 install. Point the client at the server's address and its
**login port** (9000 by default).

- **Auberean**, the Dereth Network's client:
  `auberean --dats <install folder> --server <ip>:9000 --account <name>:<ticket>`. It reaches the
  friends service at the same address and port. See [its README](../auberean/README.md).
- **The September 2013 client** takes `--host <ip> --port 9000 --account <name>
  --glsticketdirect <ticket>`. It reaches the friends service through the Dereth Network's friends
  library, which the launcher sets up for it.

On an open world (section 5), the first login with a new name makes the account.

---

## 10. Troubleshooting

| Message or symptom | Cause and fix |
| --- | --- |
| `yalain: --dats names the install to read the data from` | Give `--dats <install folder>`. |
| `yalain: reading the character data: no portal data file in <folder>` | `--dats` names a folder without `portal.dat`. Name the install's folder, the one holding the `.dat` files. |
| `yalain: the world database: could not download <tag>, which this world needs: ...` | The release is not in the cache and the server could not fetch it: the rest of the line says why. Run `yalain world fetch` as the user the server runs as when it can reach it, or give `--world-db <folder>`. |
| `yalain: the world database: ... problems in <folder>` | The database does not check or merge. Run `yalain world check <folder>` for the list. |
| `yalain: --config <file>: the configuration file does not exist` | `--config` names a missing file. |
| `yalain: yalain.toml: ...` | The configuration file is not TOML, or a key's value is not one its setting takes (section 3.6). |
| `yalain.toml: <key>: not a setting of the server; ignored` | A misspelled key, or one the server does not have. The server starts without it. |
| `yalain.toml: <key>: Empyrean's setting, which Yalain does not have; ignored` | A key of `empyrean.toml` Yalain has no setting for (section 3.6). The server starts without it. |
| A player's client says the logon server is full | The world holds `--max-sessions` logins (an admin's account passes this one), or the player's address holds `--max-sessions-per-address` (section 3.7). |
| `yalain: --status <address>: ...` | The status endpoint's port is in use, or the address is not one of this machine. |
| `yalain: binding the ports: ... [ports-taken]`, exit code 3 | A port, or the port above it, or the friends service's TCP port, is in use (another server running?). |
| `yalain: binding the ports: ...`, exit code 1 | `--bind` is not an address of this machine, or the system refused the port. |
| `the ports are even: each service uses its port and the one above` | Give `--login-port` and `--world-port` even numbers. |
| `yalain: binding the ports: the friends service on TCP <address> ...` | The TCP port is in use. Free it, choose another `--login-port`, or give `--no-friends`. |
| `yalain: --bind 0.0.0.0 listens on every address, so give --advertise ...` | Give `--advertise <address>` (section 4). |
| Players on other machines log in but never reach the world | The advertised address is not one they can reach, or the world's ports are closed. Check `--advertise` (section 4), and that all four UDP ports are open and forwarded. |
| The friends list is out of reach for every player | The TCP login port is blocked, or the server runs with `--no-friends`. |
| `--hub-key-file <file>: no hub key in it` | The file must hold the key alone: `drth_` and 43 more characters. |
| `--hub <url>: plain http:// is for a hub on this machine only; use https://` | Use the hub's `https://` address. |
| `--federation: the slug <slug> cannot join the Dereth Network ...` | The slug is longer than 12 or reserved. Register a shorter one, or run without `--federation`. |
| `Dereth Network: the hub refused the link (it does not know the world's key)` | The key file holds another world's key, or the key was replaced at the hub. |
| `Dereth Network: the hub refused the link (the world's federation is not on at the hub)` | Verify the world, or turn its federation on at the hub (section 8.2). |
| `the hub could not be reached: ...` | The hub is down, or the machine cannot reach it. The world plays on and tries again a minute later. |
| `the world database did not compile; the world is unchanged: ...` | After `reload`: fix what it names and `reload` again. |
| `yalain: <file>: the backup before upgrading the schema ... failed, so nothing was upgraded: ...` | Free disk space or let the server write the database's folder, then start it again (section 3.5). |
| `yalain: <file>: the database is at schema version <n>, newer than this Yalain's ...` | A newer release upgraded the database. Run that release, or put back a backup (section 3.5). |
| `yalain world: yalain-world-<tag>.zip does not match: expected SHA-256 ...` | The archive is not the release this build expects. Nothing was cached. Download it again; with `--version`, check `--sha256`. |
| `yalain world: could not reach ...` | No connection to GitHub. Try again, or fetch by hand (section 2.1). |
