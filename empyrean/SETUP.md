# Setting up an Empyrean server

This guide takes you from nothing to a running Empyrean shard that players can log in to, and
covers keeping it up to date and moving over from ACE.

Empyrean (`empyrean-server`) is a port of ACE, the Asheron's Call server emulator. It is one
executable with no database server to install: the world's content is a read-only file you build
once (`world.pack`), and accounts and characters live in two SQLite files the server creates.

Contents:

1. [What you need](#1-what-you-need)
2. [Build the world database](#2-build-the-world-database)
3. [Configure](#3-configure)
4. [Run](#4-run)
5. [Connect a client](#5-connect-a-client)
6. [Keeping up to date](#6-keeping-up-to-date)
7. [Migrating from ACE](#7-migrating-from-ace)
8. [Troubleshooting](#8-troubleshooting)

---

## 1. What you need

| What | Where it comes from |
| --- | --- |
| The retail dat files: `client_portal.dat`, `client_cell_1.dat`, `client_highres.dat`, `client_local_English.dat` | Your own Asheron's Call client install. The project ships none, and never will. |
| `empyrean-server` and `empyrean-import` (`.exe` on Windows) | A release package, or built from source (below). |
| ACE-World's world database | The SQL release of ACE-World-16PY-Patches, which `empyrean-import fetch` downloads for you (section 2). |

A release package holds the two binaries, this guide, `README.md`, `DIVERGENCES.md`, `ACE-BUGS.md`,
`empyrean.toml.example`, `LICENSE`, `NOTICE.txt` (which names the exact source commit the release
was built from) and `THIRD-PARTY-LICENSES.html`. It holds no game data. `empyrean-server --version`
names the version and commit of the binary you have.

### Building from source

The server's crates are members of a Cargo workspace. Install a current stable Rust toolchain
(https://rustup.rs), then from the workspace root (the folder holding the top-level `Cargo.toml`):

```
cargo build --profile server-release -p empyrean-server -p empyrean-import
```

The binaries land in `target/server-release/` under the workspace root. `--release` works too
(into `target/release/`), without the server's link-time optimisation.

On macOS, released binaries are not notarised. If Gatekeeper blocks them, run once:

```
xattr -d com.apple.quarantine empyrean-server empyrean-import
```

---

## 2. Build the world database

The server reads its world (every creature, item, spawn, quest and recipe) from `world.pack`,
which you build from ACE-World's SQL release. The project does not distribute packs: you download
the release and build your own. ACE-World's data is licensed AGPL-3.0 by its authors.

### 2.1 The one command

```
empyrean-import fetch --pack
```

This downloads the ACE-World release this server build was tested against from
https://github.com/ACEmulator/ACE-World-16PY-Patches/releases, checks the download's SHA-256
against the value built into `empyrean-import`, unzips it, keeps the dump in a cache folder, and
builds `world.pack` (and `world.pack.report.json`) in the current folder. It needs an internet
connection once; a release already in the cache is not downloaded again. A SHA-256 that does not
match stops it with an error, and nothing is cached.

The cache is a per-user folder, so every copy of the server on the machine shares it:

| System | Cache folder |
| --- | --- |
| Windows | `%LOCALAPPDATA%\Empyrean\world-database` |
| macOS | `~/Library/Caches/Empyrean/world-database` |
| Linux | `$XDG_CACHE_HOME/empyrean/world-database`, else `~/.cache/empyrean/world-database` |

It holds one `ACE-World-Database-v0.9.<n>.sql` per release fetched (about 150 MB each). The
options:

| Option | What it does |
| --- | --- |
| `--pack` | build `world.pack` from the dump once it is fetched |
| `--out <world.pack>`, `--report <report.json>` | with `--pack`: where the pack and its report go |
| `--version <n>` | fetch release `v<n>` (for example `--version 0.9.294`) instead |
| `--latest` | fetch the newest release, and print its tag and SHA-256 |
| `--dir <folder>` | cache the dump in `<folder>` instead |

Without `--pack` it only fetches, and prints where the dump is. For `--version` and `--latest`
the download is checked against the SHA-256 GitHub publishes for the asset.

**By hand**, if the machine cannot reach GitHub: on the releases page, expand "Assets" under the
release, download `ACE-World-Database-v0.9.<n>.sql.zip`, and unzip it (`unzip` on Linux and macOS,
`Expand-Archive` in PowerShell, or Explorer's "Extract all"). With the GitHub CLI
(https://cli.github.com), `gh release download --repo ACEmulator/ACE-World-16PY-Patches --pattern
"ACE-World-Database-*.sql.zip"` takes the latest release. You get one file,
`ACE-World-Database-v0.9.<n>.sql`; build the pack from it as in 2.2.

### 2.2 Build the pack

`fetch --pack` does this for you. By hand, or from a dump you already have:

```
empyrean-import --sql ACE-World-Database-v0.9.<n>.sql --out world.pack
```

It takes a few seconds and writes two files:

- `world.pack`: the world database the server reads. It never changes once built; the server
  checks its structure and content hash every time it starts.
- `world.pack.report.json`: what went in: rows read per table, records per pack table, the
  content hash, and (when you give content patches) what each input file added, replaced and
  deleted. `unknown_tables` and `unread_columns` should both be empty; if not, the release changed
  ACE's database schema and this server build does not read the new parts yet.

The build is deterministic: the same inputs always give the same pack and content hash.

**Your own content.** If you keep content changes as ACE-style SQL files (one object per file, a
`DELETE` then its `INSERT`s) or ACE's JSON exports, bake them in over the release; later inputs win
over earlier ones:

```
empyrean-import --sql ACE-World-Database-v0.9.<n>.sql --patches my-content/sql --json my-content/json --out world.pack
```

A folder means every `.sql` (or `.json`) file beneath it, in sorted path order. A statement that
fails stops the build and names the file and line; no pack is written.
`--now "YYYY-MM-DD HH:MM:SS"` sets the timestamp written wherever ACE would stamp the current
time (default 2000-01-01 00:00:00).

### 2.3 Verify it

The server corrects a small number of values in ACE's data as it reads them (each correction is
built into the server binary). Check that they all meet the pack:

```
empyrean-import --corrections world.pack
```

It lists every correction as `applies`, `stale` or `absent`, and exits with status 0 when every
entry applies. On the ACE-World release a server build was tested against, all of them should
apply. On a newer release, a `stale` or `absent` entry usually means ACE changed that value
itself; the server still starts, and logs the entry as a warning.

---

## 3. Configure

### 3.1 Lay out a folder

The simplest layout is one folder holding everything:

```
empyrean/
  empyrean-server        (empyrean-server.exe on Windows)
  empyrean-import
  empyrean.toml
  world.pack
```

The dat files can stay in your client install; `empyrean.toml` points at them.

### 3.2 Write `empyrean.toml`

`empyrean.toml` is the server's only configuration. Every key is optional; a key left out keeps its
default. A minimal file:

```toml
[server]
world_name = "My World"
# The folder that holds the four dat files.
dat_files_directory = 'C:\Games\AC'   # Windows (single quotes: backslashes as written)
# dat_files_directory = "~/ac-client"  # Linux or macOS
world_pack_path = "./world.pack"
log_level = "info"
# The status endpoint; leave it out to keep it off.
status_address = "127.0.0.1:9100"

[server.network]
host = "0.0.0.0"
port = 9000
```

Every key, with its default and the ACE setting it stands for (or "Empyrean only"), is in
`empyrean.toml.example`. The sections are `[server]`, `[server.network]`, `[server.accounts]`,
`[[server.preloaded_landblocks]]`, `[database]`, `[offline]` and `[ddd]`. Two ways to get a
complete, commented file to edit (either one):

```
cp empyrean.toml.example empyrean.toml           # the shipped example
empyrean-server --write-config empyrean.toml     # or: the configuration the server would use now
```

`--write-config` never overwrites an existing file.

In a double-quoted TOML string, write each backslash twice (`"C:\\Games\\AC"`); a single-quoted
string takes them as they are but cannot contain a `'` (for a folder like `Asheron's Call`, use
a double-quoted string: `"C:\\Games\\Asheron's Call"`). A key the server does not know is logged
as a warning and ignored.

### 3.3 How paths are read

Every path in `empyrean.toml` (`dat_files_directory`, `world_pack_path`, `world_overlay_path`,
`world_base_sql`, `world_base_patches`, `shard_db_path`, `auth_db_path`) follows one rule:

- a leading `~` is your home folder (`~user/...` is not expanded);
- an absolute path is used as written;
- a relative path is relative to **the folder `empyrean.toml` is in**, not to where the server was
  started from (running on the defaults, with no file, it is the working directory).

Two defaults also search: an empty `dat_files_directory` and the default `world_pack_path`
(`./world.pack`) are looked for beside `empyrean.toml`, then beside `empyrean-server`. At
`log_level = "info"` the server logs where every path resolved (`Paths: ...` lines).

### 3.4 Which file the server reads

- Without options, the server reads `empyrean.toml` from the working directory, else from beside
  `empyrean-server`, and logs which one it read. With neither, it runs on the defaults and warns.
- `--config <path>` names the file; it must exist.
- **No environment variables** are read. Everything is a key in `empyrean.toml`, apart from
  `--config`, `--status <ip>:<port>` (overrides `status_address`) and `--run-for <seconds>` on the
  command line.

### 3.5 Firewall

The server listens on **two UDP ports**: `port` and `port + 1` (9000 and 9001 by default). Open
both, inbound, for UDP. Nothing else needs to be open to players; the status endpoint in
particular should not be.

With the WebSocket endpoint on ([4.7](#47-websocket-browser-clients)), open its TCP port too when
it serves `wss://` itself. Behind a reverse proxy, open only the proxy's port (443), and keep the
endpoint on loopback or a private address that only the proxy can reach.

---

## 4. Run

### 4.1 First start

From the folder with `empyrean.toml`:

```
./empyrean-server            # Linux or macOS
.\empyrean-server.exe        # Windows
```

On its first start the server creates `shard.db` (characters, their items and the server
properties) and `auth.db` (accounts) at the paths in `[database]` (default: beside
`empyrean.toml`). They are SQLite files; SQLite also keeps `-wal` and `-shm` files beside them
while the server runs. With `log_level = "info"` the start-up lines show the paths, the world
pack's content hash and the corrections that apply.

The server refuses to start without usable dat files or a usable `world.pack`, and says where it
looked (see [Troubleshooting](#8-troubleshooting)).

### 4.2 Accounts and your admin account

Accounts are created on first login by default: a player logs in with a new name and password,
and the account is made with them (`[server.accounts] allow_auto_account_creation = true`). A new
account gets `default_access_level` (0, Player).

**The first account is the admin.** While `auth.db` holds no admin account, the server warns at
start-up, and the next account created becomes an Admin (level 5), however it is created. So on
a fresh server, the simplest route is to log in first with the account you want to be admin.

To create or promote accounts from the server console (console commands are typed without `@`):

```
accountcreate <name> <password> 5        # a new admin account
set-accountaccess <name> 5               # promote an existing account
set-accountpassword <name> <newpassword>
accountget <name>
```

Access levels are 0 Player, 1 Advocate, 2 Sentinel, 3 Envoy, 4 Developer, 5 Admin (a number or the
name). An admin can use the same commands in game with `@`. If you turn off
`allow_auto_account_creation`, create each player's account with `accountcreate`.

### 4.3 The console

When the server runs in a terminal, it reads commands at the `empyrean>> ` prompt:

- `empcommands` lists the commands (ACE's `acecommands` works too), and `emphelp` gives help
  (ACE's `acehelp`).
- `exit` shuts the server down cleanly. Ctrl-C does the same; a second Ctrl-C exits at once,
  **without saving**.
- `config-write [<path>] [-f]` writes the configuration in use as a commented `empyrean.toml`.

### 4.4 Logs

The server logs to standard error, at `[server] log_level`: `error`, `warn`, `info` (the default),
`debug` or `trace`. To keep a log file, redirect standard
error (`./empyrean-server 2>> empyrean.log`), or let a service manager collect it (below). Anything
the port does not do yet is logged once as `not ported: ACE: <name>`. The lines ACE writes straight
to its console (quest and contract bookkeeping, loot-table loading, cast records) are ordinary log
lines here, under the target `console`, so the level filters them like any other; at `debug` and
`trace`, where each line names its source, they read `console: ...`.

### 4.5 Running as a service

A service has no terminal, so turn the console off:

```toml
[server]
interactive_console = false
```

Stop the server with Ctrl-C, or on Linux and macOS with SIGINT, SIGTERM or SIGHUP; it then saves and shuts down.
A second interrupt exits at once.

**Linux (systemd).** Create the dedicated `empyrean` user and give it the install folder (it
writes the databases and their backups there, and an update replaces the binaries there):

```sh
sudo useradd --system --home-dir /opt/empyrean --shell /usr/sbin/nologin empyrean
sudo chown -R empyrean:empyrean /opt/empyrean
```

Without the user, the unit fails at once with `status=217/USER`. An example unit,
`/etc/systemd/system/empyrean.service`, for a server installed in `/opt/empyrean`:

```ini
[Unit]
Description=Empyrean server
After=network-online.target
Wants=network-online.target

[Service]
User=empyrean
ExecStart=/opt/empyrean/empyrean-server --config /opt/empyrean/empyrean.toml
TimeoutStopSec=120
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

Then `systemctl daemon-reload`, `systemctl enable --now empyrean`, and read the log with
`journalctl -u empyrean`. Because relative paths follow `empyrean.toml`, the service needs no
`WorkingDirectory`.

**Windows.** Windows cannot run a console program as a service by itself. Use a service wrapper
that starts `empyrean-server.exe --config <path>\empyrean.toml`, stops it with Ctrl-C, and captures
its standard error to a file; or run it in a terminal that stays open. Keep
`interactive_console = false` whenever there is no one at a console.

A server that updates itself (6.4) restarts into the new release inside the same service on
either system; nothing else needs configuring.

### 4.6 The status endpoint

With `status_address` set (or `--status <ip>:<port>`), the server answers:

- `GET /health`: `200 ok` while the world responds, otherwise `503`, for a monitor or a health
  check;
- `GET /status`: the same, with a JSON body (world name, version, uptime, connections, players
  online, the world data's content hash and corrections digest).

It has **no authentication**. Bind it to `127.0.0.1` (or a private network) and do not open its port
in the firewall.

With the WebSocket endpoint on, `/status` also reports `websocket_url`, the URL browser clients
connect to (`null` when the endpoint is off). A web page on an origin in
`server.websocket.allowed_origins` may read `/status` too, so it can find that URL; no other page can.

### 4.7 WebSocket: browser clients

A browser cannot send UDP. With `[server.websocket]` enabled, the server also accepts each game
datagram as one WebSocket message, so the web client connects to it directly. Nothing else changes:
the same packets, the same sessions and the same world. The frame is described in
`docs/networking/05-websocket-frame.md`. It is off by default, and a server without it behaves
exactly as before.

**The connection must be encrypted.** The game protocol is not: its key stream only keys the
checksums, and the login carries the account's password. A browser on an `https://` page also
cannot open a plain `ws://` connection to another machine. So:

- **`wss://` served by the server itself:** set `tls_certificate` and `tls_private_key` to PEM
  files (a certificate chain and its key, from any certificate authority). The server does not
  obtain certificates itself.
- **Behind a reverse proxy that terminates TLS** (nginx, Caddy, Cloudflare): the endpoint speaks
  plain `ws://` to the proxy. Keep it on loopback, or set `behind_tls_proxy = true` when the proxy
  is on another machine and only the proxy can reach the endpoint.
- **Plain `ws://` on any other address is refused:** the server stops at start-up and says why.

The settings:

| Key | Default | What it does |
|---|---|---|
| `enabled` | `false` | Listen for WebSocket connections. |
| `listen` | `"0.0.0.0:9443"` | The address and port. |
| `tls_certificate`, `tls_private_key` | empty | PEM files for `wss://`. |
| `behind_tls_proxy` | `false` | A proxy in front terminates TLS. |
| `trusted_proxies` | `[]` | Proxies whose `X-Forwarded-For` names the client. |
| `allowed_origins` | `[]` | The web pages allowed to connect, by origin; empty refuses every page. |
| `maximum_connections_per_ip_address` | `4` | Connections open at once from one client; `-1` is unlimited. |
| `idle_timeout` | `60` | Seconds without a message before a connection is closed. |
| `public_url` | empty | The URL reported on `/status`; empty derives it from `listen`. |

When a connection closes, however it closes, the log-off the client handed over in advance is
delivered for it, so a player who closes the tab leaves the world at once. The per-address session
limit of `[server.network]` applies to WebSocket players as to UDP ones.

#### Serving `wss://` directly

```toml
[server.websocket]
enabled = true
listen = "0.0.0.0:9443"
tls_certificate = "/etc/empyrean/fullchain.pem"
tls_private_key = "/etc/empyrean/privkey.pem"
allowed_origins = ["https://play.example.org"]
```

Players' pages connect to `wss://play.example.org:9443/`. Open TCP 9443 in the firewall.

#### Behind nginx

This is the recommended setup when nginx (or another web server) already serves your site with a
certificate. nginx terminates TLS and forwards the WebSocket to the endpoint on loopback:

```toml
[server.websocket]
enabled = true
listen = "127.0.0.1:9180"
trusted_proxies = ["127.0.0.1"]
allowed_origins = ["https://play.example.org"]
public_url = "wss://play.example.org/ws"
```

```nginx
map $http_upgrade $connection_upgrade {
    default upgrade;
    ''      close;
}

server {
    listen 443 ssl;
    server_name play.example.org;
    ssl_certificate     /etc/letsencrypt/live/play.example.org/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/play.example.org/privkey.pem;

    location /ws {
        proxy_pass http://127.0.0.1:9180;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection $connection_upgrade;
        proxy_set_header Host $host;
        # The client's address, for the per-address limits and the log. The server believes it
        # only from trusted_proxies, and takes the last address, the one nginx added.
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        # Longer than idle_timeout, so the server, not nginx, decides when a quiet connection ends.
        proxy_read_timeout 120s;
        proxy_send_timeout 120s;
        proxy_buffering off;
    }
}
```

Keep the proxy's read timeout longer than `idle_timeout`. Without `X-Forwarded-For` (or without
`trusted_proxies`) every player appears to come from nginx's address, and the per-address limits
count them all as one.

If nginx runs on another machine, listen on a private address, set `behind_tls_proxy = true` and
`trusted_proxies` to nginx's address, and let only nginx reach the port.

#### Behind Cloudflare

Cloudflare proxies WebSocket connections on every plan:
- Give the web client's host a **proxied** DNS record.
- Set SSL/TLS to **Full (strict)**, so the hop from Cloudflare to your server is encrypted and its
  certificate checked as well. The server, or the nginx in front of it, needs a certificate
  Cloudflare trusts: a Cloudflare Origin CA certificate, or one from a public authority.

Things to know:
- **Idle timeout.** Cloudflare closes a WebSocket that has been idle for about **100 seconds**.
  Keep `idle_timeout` below that; the default of 60 is. A playing client is never that quiet.
- **UDP is not proxied.** Cloudflare carries HTTP and WebSocket only, never the game's UDP. Players
  on the desktop client need a separate, **unproxied** DNS record (or the server's address) for UDP
  `port` and `port + 1`.
- **Client addresses.** The server trusts `X-Forwarded-For` only from the addresses in
  `trusted_proxies`, and Cloudflare connects from many. Put nginx in front and restore the
  visitor's address there (`real_ip_header CF-Connecting-IP;` with Cloudflare's ranges in
  `set_real_ip_from`). nginx then passes it on as above.

#### Finding the endpoint

A launcher or the web client finds the URL at `GET /status` (`websocket_url`). Without a status
endpoint, the URL is `wss://<host>:<port>/`, or wherever the proxy serves it (`public_url`).

---

## 5. Connect a client

Players need their own Asheron's Call client with the same dat files. Point the client at the
server's address and its **first** port (`port`, 9000 by default).

The retail client (`acclient.exe`) and the Dereth client take the account, the password and the
server on the command line:

```
acclient.exe -a <account> -v <password> -h <server address>:9000
```

Launchers made for ACE servers work the same way: add a server with the host and port.

- On the same machine, the address is `127.0.0.1`.
- On a home network, use the server's LAN address.
- Over the internet, use your public address or a DNS name, and forward UDP `port` and `port + 1`
  on your router to the server.

The first login with a new name creates the account (when auto-creation is on), as in
[4.2](#42-accounts-and-your-admin-account).

The web client (in a browser) connects to the server's WebSocket URL instead, when the server has
the endpoint on ([4.7](#47-websocket-browser-clients)). It reaches a server without the endpoint
(ACE, GDLE, or Empyrean with it off) through `dereth-web-relay`, which the player runs on their own
machine.

---

## 6. Keeping up to date

### 6.1 A new ACE-World release

ACE-World publishes new releases from time to time. Build a new pack beside the old one, compare,
and swap:

1. Fetch the new release: `empyrean-import fetch --latest` (section 2.1) prints its tag and where
   the dump is.
2. Build the new release alone, and with your content if you have any:
   ```
   empyrean-import --sql ACE-World-Database-v0.9.<new>.sql --out ace-new.pack
   empyrean-import --sql ACE-World-Database-v0.9.<new>.sql --patches my-content/sql --out world-new.pack
   ```
   Check that `unknown_tables` and `unread_columns` in the report are empty.
3. See what upstream changed, and which of those changes meet your own content or the server's
   corrections (`ace-old.pack` is the previous release built alone):
   ```
   empyrean-import --check ace-old.pack ace-new.pack --fields --overlap my-content/sql --sql ACE-World-Database-v0.9.<new>.sql --report upstream-diff.json
   ```
   For each overlap, decide whether your file still wins or should take upstream's fix.
4. Check the corrections against the new pack:
   ```
   empyrean-import --corrections world-new.pack
   ```
5. Stop the server, back up (6.3), point `world_pack_path` at `world-new.pack` (or rename it to
   `world.pack`), and start. Keep the old pack until you are happy with the new one.

Without your own content, steps 2 and 3 reduce to building `world-new.pack` and, if you like,
`empyrean-import --check world.pack world-new.pack` to see what changed.

Existing characters keep the items they already hold: a world change reaches new objects, not
copies already in the shard.

### 6.2 A new server release

Stop the server, back up (6.3), replace `empyrean-server` and `empyrean-import`, and start.
Database schema changes are applied automatically: each time the server opens `shard.db` and
`auth.db`, it applies any migrations the files have not had yet, each in its own transaction.
There is no separate upgrade step.

- **Automatic backups.** Before it migrates a file, the server copies it beside itself with
  SQLite's online backup, as `shard.db.backup-shard-v<from>-v<to>-<UTC timestamp>` (and
  `auth.db.backup-auth-v…`). If the copy fails (a full disk, no write permission), nothing is
  migrated and the server stops with the reason. Only the newest three backups of each file are
  kept. Nothing is copied when there is nothing to migrate.
- **Going back.** An older server refuses a database a newer release has upgraded, naming both
  schema versions and the newest backup. To go back, run the newer release again, or stop the
  server and copy the backup over `shard.db` (or `auth.db`), deleting its `-wal` and `-shm` files.
  Play since the upgrade is lost.
- **The world pack.** A release that changes the pack's format refuses the old `world.pack` and
  prints the exact command that rebuilds it (from the dump `empyrean-import fetch` cached, when it
  is there). Add the `--patches` and `--json` inputs you built the old pack with. The importer
  keeps the pack it replaces as `world.pack.backup-<UTC timestamp>` (the newest three).
- **The configuration.** A key `empyrean.toml` has that the server does not know is a warning
  naming it, not an error. A key a release renames keeps working under its old name, with a
  warning, for that release; rename it before the next.

Read the release notes for anything that needs a new pack (a new release is usually tested
against a particular ACE-World release).

After replacing the binaries, run `empyrean-import --corrections world.pack` again: the
corrections are part of the server binary and can change between releases.

Every release says what upgrading to it involves in its `release.json`: the database migrations,
whether `world.pack` must be rebuilt, configuration keys renamed, removed or newly required, and
the oldest version that can upgrade straight to it. A patch release (`0.1.0` to `0.1.1`) never
changes any of these. `empyrean-server update --check` reads them for you (6.4), and the server
can install releases by itself.

### 6.3 Backups

Everything that changes as people play is in `shard.db` and `auth.db`. `world.pack` can be rebuilt
from its inputs; keep the dump and your content files so you can.

- **Server stopped:** copy `shard.db` and `auth.db`, with any `-wal` and `-shm` files beside them.
- **Server running:** use SQLite's online backup, which is consistent while the server writes:
  ```
  sqlite3 shard.db ".backup 'shard-backup.db'"
  sqlite3 auth.db  ".backup 'auth-backup.db'"
  ```
  Copying the files alone while the server runs can give an inconsistent copy.

Take a backup before every upgrade of the server or the world pack.

### 6.4 Automatic updates

The server can keep itself up to date, within limits you set, and undo an update that does not
come up. It is off until you turn it on:

```toml
[server]
update = "patch"          # "off" (the default), "patch" or "minor"
update_check_hours = 6    # how often it looks
update_warning_seconds = 300   # the in-game countdown before it restarts
```

| `update` | What it installs by itself |
|---|---|
| `"off"` | Nothing. |
| `"patch"` | Patch releases of the version it runs (`0.1.0` to `0.1.3`). These change nothing but the binaries: no migration, no new pack, no configuration change. |
| `"minor"` | Also minor releases of the same major version (`0.1.3` to `0.2.0`), with their database migrations and world-pack rebuilds. |

Never installed by itself, only reported in the log: a major release, a pre-release, a release
that needs a configuration key you must set, a release that needs `world.pack` rebuilt when
`server.world_base_sql` does not name the dump it was built from (with `server.world_base_patches`
for your patches), a release that cannot be upgraded to straight from the version you run (the
intermediate one is installed first, on a later check), and a release that already failed here.

**What happens.** A minute after start-up, and then every `update_check_hours`, the server asks
GitHub for the releases of the repository it was built from (the address `--version` prints),
or of the one `update_source` names: another GitHub repository (`https://github.com/<owner>/<name>`),
or a mirror that serves GitHub's releases API for one (`https://<host>/repos/<owner>/<name>`).
When one qualifies, with the world still running, it:

1. downloads this platform's archive, `release.json` and `SHA256SUMS`, and refuses the release if
   a SHA-256 does not match. That is the only check of where the files came from: releases are
   not signed, so the log says the release was checked by SHA-256 only;
2. unpacks it into `.empyrean-update/` beside the server, runs the new `empyrean-server` once to
   confirm it runs on this machine and is what the release declares, and, for a release that needs
   one, builds the new `world.pack` with the new importer;
3. starts ACE's shutdown countdown (`update_warning_seconds`), which players see as the usual
   shutdown broadcasts (also written to the log). `@cancelshutdown` cancels the update too; the
   next check tries again;
4. when the countdown ends and everything is saved: backs up `shard.db` and `auth.db`
   (`shard.db.backup-update-v<old>-v<new>-<UTC timestamp>`, the newest three kept), moves each
   replaced file aside as `<name>.previous-v<old>`, puts the new files (and the new pack) in
   place, and starts the new server for a health check: it must open and migrate the databases,
   load the world, open its listeners and report itself ready;
5. passes: the new server starts for real. Fails: the old files, the old pack and the backed-up
   databases are put back, the failure is logged and recorded in `.empyrean-update/state.json`
   (so that release is not tried again automatically), and the old server starts again.

A server stopped with Ctrl-C or a signal while an update waits installs nothing.

**The folder must be writable** by the account the server runs as: the updater renames and
writes the binaries beside `empyrean-server`, and writes `.empyrean-update/`.

**Running as a service.** The server restarts itself into the new release; nothing needs to be
configured for it:

- **Linux and macOS (systemd, launchd, or by hand):** the server process replaces itself with the
  new binary (`exec`), keeping its process ID, so systemd sees the same main process keep running
  and `journalctl -u empyrean` shows the whole update. The unit in 4.5 works as it is.
- **Windows (a service wrapper, or a terminal):** Windows cannot replace a running process, so the
  old process starts the new server as its child, in the same console, and waits for it; the
  wrapper still sees its process running. Stopping the service stops the new server as usual (the
  Ctrl-C reaches it through the shared console; a wrapper that ends the process tree ends both).
  The waiting process holds no files, ports or databases, and is gone the next time the service is
  restarted. Console input reaches the new server: a terminal is read by the new server itself,
  and input from a pipe or a file is passed on to it by the waiting process.

**By hand**, with any `update` setting:

```
empyrean-server update --check [--config empyrean.toml] [--policy patch|minor]
empyrean-server update --apply [--config empyrean.toml] [--policy patch|minor]
```

`--check` lists every newer release, says why each would or would not be installed, and what
installing the chosen one involves (migrations, pack rebuild, configuration changes). `--apply`
does steps 1 to 5 on a **stopped** server (it refuses while the server's game port is in use) and
leaves the server stopped, so you or the service manager start it. Without `--policy`, the
policy is `update`, or `minor` when `update` is `"off"`. Majors and pre-releases are installed by
hand as in 6.2.

---

## 7. Migrating from ACE

### 7.1 Configuration: convert `Config.js` once

Empyrean does not read `Config.js`. Convert it once:

```
empyrean-server --write-config --from Config.js --out empyrean.toml
```

The converter writes a fully commented `empyrean.toml` with every setting your `Config.js` held.

**What carries over:** the world name, `DatFilesDirectory`, the network settings, account settings,
preloaded landblocks, the offline maintenance switches, DAT patching, shutdown interval and cache
times.

**What is dropped** (each is logged, and listed at the top of the new file): the MySQL
connections (`MySql.Authentication`, `MySql.Shard`, `MySql.World`), `Server.Threading`,
`Server.ModsDirectory` (mods are not ported), `Server.WorldDatabasePrecaching`, and the offline
update switches (`AutoUpdateWorldDatabase`, `AutoServerUpdateCheck`,
`AutoApplyWorldCustomizations`, `WorldCustomizationAddedPaths`,
`RecurseWorldCustomizationPaths`, `AutoApplyDatabaseUpdates`).

Then check the result:

- **Paths:** they are copied as written, but a relative path is now relative to `empyrean.toml`'s
  folder, not to ACE's working directory.
- **World name:** if your ACE server never set `WorldName`, it was `ACEmulator`; the retail client
  names its saved window layouts after the world, so set `world_name = "ACEmulator"` to keep
  players' layouts.
- **New keys:** add `world_pack_path` (section 2) and, if you want them elsewhere,
  `[database] shard_db_path` and `auth_db_path`.

### 7.2 World data and customisations

ACE's world database (`ace_world` in MySQL) is replaced by `world.pack`, built from the same
ACE-World release as in section 2. ACE's customisation SQL (the `Customizations` folders
`AutoApplyWorldCustomizations` applied, or content you exported with `export-sql` /
`export-json`) goes in with `--patches` and `--json`:

```
empyrean-import --sql ACE-World-Database-v0.9.<n>.sql --patches Customizations --json my-json --out world.pack
```

A folder means every `.sql` (or `.json`) file beneath it, in sorted path order, applied as ACE's
MySQL would run it. If you relied on a particular order across several folders, give them as
separate `--patches` in that order.

In-game content editing (`@import-sql`, `@createinst`, `@nudge` and the rest) works against a
content overlay: an SQLite file beside the pack that the world database reads first, while
`world.pack` itself never changes. Turn it on by naming the overlay and the inputs the pack was
built from:

```toml
[server]
world_overlay_path = "./overlay.sqlite"
world_base_sql = "./ACE-World-Database-v0.9.<n>.sql"
world_base_patches = ["./Customizations", "json:./my-json"]   # the pack's --patches, then --json as "json:<path>"
```

To bake the overlay into a new pack (never over the one the server is running on), then start the
server on it with a new, empty overlay:

```
empyrean-import --sql ACE-World-Database-v0.9.<n>.sql --patches Customizations --json my-json --overlay overlay.sqlite --out new-world.pack
```

### 7.3 Existing characters and accounts (an ACE MySQL shard)

**Not supported yet.** There is no tool that imports an ACE shard: neither `empyrean-server` nor
`empyrean-import` reads ACE's `ace_shard` or `ace_auth` databases or their SQL dumps (the importer
reads world-database tables only). An Empyrean server starts with an empty shard.

What exists that such a migration would build on:

- The SQLite shard and account schemas use ACE's own table and column names, deliberately, so
  that an ACE shard can be brought across.
- Account passwords are stored as ACE stores them (bcrypt, and ACE's older salted SHA-512, which
  is upgraded on the next login), so moved accounts would keep their passwords.
- Any shard brought in gets the same automatic migrations on first open as any other.

What an operator would need, and what a future tool has to provide: an export of each MySQL table
of `ace_shard` and `ace_auth`, loaded into the SQLite files with the value conversions MySQL and
SQLite differ on (dates and times, unsigned 64-bit columns, IP addresses). Until there is a
supported tool, do not hand-load a production shard; start fresh, or keep the ACE server for the
existing characters.

**Server properties** (the values set with `/modifybool`, `/modifylong`, `/modifydouble` and
`/modifystring`) are stored in the shard, as in ACE, so they would only come across with the shard.
On a fresh Empyrean shard, set them again; `/showprops` lists them.

### 7.4 What behaves differently from ACE

What an ACE shard notices when it moves over:

- **Commands.** `@emphelp`, `@empcommands` and `@empversion` are the help, command-list and version
  commands; ACE's `@acehelp`, `@acecommands` and `@aceversion` keep working. `@reportbug` (when
  `reportbug_enabled` is on) shows the project's issue tracker and sends nothing about the player.
- **Console.** The prompt is `empyrean>> ` (ACE's was `ACE >> `): a script that waits for ACE's
  prompt must change. `ACE_NONINTERACTIVE_CONSOLE` is not read: set `interactive_console = false`.
  A console command's reply goes to standard output whatever the log level (ACE logged it at Info).
- **No environment variables and no container special case.** `DOTNET_RUNNING_IN_CONTAINER` is not
  read and `content_folder` is no longer forced to `/ace/Content`: set it and the paths as on any
  install.
- **Paths** resolve against the folder `empyrean.toml` is in, not the working directory (3.3).
- **Stored text.** The first time the server opens a shard it points the DAT-warning messages
  (`dat_older_warning_msg`, `dat_newer_warning_msg`) at https://dereth.network and rewords two
  property descriptions, but only where the shard still holds ACE's own text.
- **IOUs** issued by ACE (signed `ACEmulator`) are still redeemed; new ones are signed `Empyrean`.
- **The source offer.** The login welcome and `@source` tell players where the server's source is.
  The AGPL asks whoever runs a *modified* version for network users to offer them the modified
  source: publish your changes and set `[server] source_url` to where they are. An unmodified
  release needs no setting.
- **Not ported:** mods, the threading options, and the automatic world-database and server updates.
  Anything else unported that the server reaches is logged once as `not ported: ACE: <name>`.
- **Deliberate differences** in game behaviour are listed, with their reasons, in
  `DIVERGENCES.md`.

---

## 8. Troubleshooting

Set `log_level = "info"` first: the start-up lines then show which configuration file was read and
where every path resolved.

| Message or symptom | Cause and fix |
| --- | --- |
| `DatManager initialization failed: <folder>: ...` | The dat files could not be opened in `<folder>`. Set `dat_files_directory` to the folder that holds all four (`client_portal.dat`, `client_cell_1.dat`, `client_highres.dat`, `client_local_English.dat`). An empty value means beside `empyrean.toml`, then beside the server. |
| `No usable world database at <path>` | There is no `world.pack` there, or it is damaged or from an incompatible build. Build it with `empyrean-import fetch --pack` (section 2), put it beside `empyrean.toml` or the server, or set `world_pack_path`. |
| `<path> is ACE's Config.js, which Empyrean does not read` | A `Config.js` was found and no `empyrean.toml`. Convert it (section 7.1). |
| `Configuration file <path> does not exist.` | `--config` names a missing file. |
| `empyrean-import fetch: ... could not reach ...` | No connection to GitHub (or a proxy in `HTTPS_PROXY` that does not answer). Try again, or download the release by hand (section 2.1). |
| `empyrean-import fetch: ... does not match ...` | The download's SHA-256 is not the expected one; nothing was cached. Try again; if it persists, the release asset changed, so report it. |
| `Configuration: unknown key ...` or `... is not read` | A misspelt key, or an ACE setting Empyrean does not have. The server ignores it and starts. |
| `Unable to open the database: ...` | `shard.db` or `auth.db` cannot be created or opened: check the folder exists and is writable by the server's user, and that no other server is using the same files. |
| `Unable to bind the listeners: ...` | The UDP port or `port + 1` is in use (another server running?) or `host` is not an address of this machine. |
| `Authentication Database does not contain any admin accounts...` | A warning, not an error: the next account created becomes Admin (section 4.2). |
| The server starts, but clients cannot connect | Check both UDP ports in the firewall and on the router, and that the client points at `port`, not `port + 1`. |
| `World database: corrections ... stale` | The pack is from a different ACE-World release than the server was tested with; the server runs. See 2.3. |
| `command prompt disabled - console input stream was closed` | The console is on but there is no terminal (a service, or input redirected). Harmless; set `interactive_console = false` to leave the console off. |

Logs go to standard error: the terminal, the file you redirect it to, or your service manager's
journal.
