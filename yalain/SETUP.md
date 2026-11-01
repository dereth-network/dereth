# Setting up a Yalain world

This guide takes you from nothing to a running Yalain world that players can log in to, and covers
listing it on the Dereth Network.

Yalain (`yalain`) is the Dereth Network's Asheron's Call 2 server. It is one program with no
database server to install. At each start it reads your own Asheron's Call 2 install, compiles the
world database (`yalain-world`) against it, and keeps accounts and characters in one SQLite file
it makes itself. Everything is set on the command line; there is no configuration file.

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
database that does not check, stops it with an error, and nothing is cached.

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

- **Nothing, or `--world-db pinned`:** the release this build pins, from the cache. If it has not
  been fetched, the server stops and says to run `yalain world fetch`. A release fetched with
  `--dir` is not looked for there: serve it as a folder (`--world-db <folder>/<tag>`).
- **`--world-db <folder>`:** the database in that folder, as it is. This is how to serve an
  unreleased checkout of the database's repository, or a release you unpacked yourself.
- **A world made in [Athenaeum](../athenaeum/README.md)** names a base database (its `[base]`) and
  holds only its own changes. Give its folder to `--world-db`, and the server lays it over the
  release its `[base]` names (or the pinned one, if it names none), from the cache. `--world-base
  <folder>` takes the base from that folder instead.

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
| `--advertise <ip>` | the IPv4 address clients are sent to for the world (default: `--bind`; required when `--bind` is 0.0.0.0 or an IPv6 address) |
| `--no-referral` | keep each client on its login connection instead of sending it to the world's port |
| `--listed-accounts` | admit only accounts made with `--add-account` (section 5) |
| `--add-account <name>:<ticket>` | make an account with that ticket, then carry on (section 5) |
| `--secret-cost <n>` | the bcrypt cost tickets are kept at, 4 to 31 (default 8; section 5) |
| `--save-interval <s>` | save each character in the world every that many seconds, 1 or more (default 300; section 3.3) |
| `--shutdown-interval <s>` | how long `shutdown` counts down, 0 to 99999 seconds (default 120; section 3.3) |
| `--banned-names <file>` | refuse new character names holding the words it lists (below) |
| `--no-friends` | serve no friends service (section 4) |
| `--hub`, `--hub-slug`, `--hub-key-file`, `--federation` | the Dereth Network (section 8) |
| `--verbose` | log at the debug level (section 7) |
| `--seconds <s>` | stop after that many seconds |

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
players the last line and stops the server at once.

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

---

## 4. Ports and the firewall

The server listens on:

| Protocol | Port | What |
| --- | --- | --- |
| UDP | `--login-port` and the port above it (9000 and 9001) | the login: the character list, and the world too with `--no-referral` |
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
account name is at most 50 characters. There is no command to change a ticket or remove an
account.

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

---

## 6. The console

When the server runs in a terminal, each line typed into it is a command. The first word is the
command, in any case. Answers appear in the log (section 7).

| Command | What it does |
| --- | --- |
| `reload` | compile the world database again and take what can change while players are in the world |
| `shutdown` or `shutdown <message>` | count down (two minutes unless set otherwise), telling every player in the world, then stop (section 3.3) |
| `cancel-shutdown` | stop the countdown, and tell the players |
| `set-shutdown-interval <s>` | how long the next `shutdown` counts down, 0 to 99999 seconds |
| `stop-now` or `stop-now <message>` | stop the server at once, saving everyone in the world |
| `save-world` | save every character in the world now; the world goes on |
| `hero <name>` | make a character a hero |
| `hero <name> off` | make it no longer one |
| `dn`, `dn block ...`, `dn unblock ...`, `dn help` | the Dereth Network (section 8.3) |
| `help` or `?` | list the commands |

A word that is no command is answered with a warning that lists them.

**`reload`** compiles the database the server was started with (the `--world-db` folder, or the
pinned release) on a thread of its own, so the world plays on meanwhile, then takes what can change
with players in the world: templates, loot, stores, quests, what people say, the story's scenes,
effects and crafting, and the `--banned-names` file. Placed objects (the market's brokers among
them) and camps change only at the next start; the log says which tables were reloaded and which
need a restart. A database that does not compile leaves the world as it was, and the log says why.
A second `reload` while one runs is refused.

**`hero <name>`** makes a character a hero. Only a hero earns focus, its third vital, by killing
creatures worth it; a hero's greatest focus is 100 and whatever its own focus adds. Made a hero,
the character's focus is set to its greatest at once. Nothing in play makes a hero: this command
does. It works on a character in the world (its player sees the change at once) and on one out of
it (the change is stored for its next entry); the log says which. `hero <name> off` takes it away,
leaving its focus as it was.

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

**Verify it.** A newly registered world is pending, and its key works only to verify it. Yalain
has no in-game verification, so verify with a page on the endpoint's own host, as section 2.5 of
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
  race, sex, health and vigor, and its allegiance.

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
| `@dn help` | the commands |

The line is heard by nobody, and the world answers in the player's chat window. The setting is
kept per character. Friends, presence and tells within the world need none of this: they work for
every player with the friends service.

### 8.3 The console's `dn`

| Command | What it does |
| --- | --- |
| `dn` | the world's link (connected, connecting or not connected), how many worlds are online, how many messages wait for the hub, and what is blocked |
| `dn block <world>` or `dn block <Name@world>` | block a world, or one character of it, for everyone on this world |
| `dn unblock <world>` or `dn unblock <Name@world>` | lift a block |
| `dn help` | list these commands |

A block works both ways and takes hold at once: what the blocked world or character sends is
refused (its tells are answered as if everyone here were offline), and what this world's
characters send it is refused too. Lifting it gives back what it took. Blocks are kept in the
store, across restarts.

Without `--federation`, every `dn` command, `dn help` among them, answers *This world is not on
the Dereth Network.*

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
| `yalain: the world database: the world database <tag> is not in <folder>: run yalain world fetch ...` | The pinned release is not in the cache. Run `yalain world fetch` as the user the server runs as, or give `--world-db <folder>`. |
| `yalain: the world database: ... problems in <folder>` | The database does not check or merge. Run `yalain world check <folder>` for the list. |
| `yalain: binding the ports: ...` | A port, or the port above it, is in use (another server running?), or `--bind` is not an address of this machine. |
| `the ports are even: each service uses its port and the one above` | Give `--login-port` and `--world-port` even numbers. |
| `yalain: the friends service on TCP <address>: ...` | The TCP port is in use. Free it, choose another `--login-port`, or give `--no-friends`. |
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
