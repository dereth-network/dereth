# The Dereth Network launcher

The launcher gets you from "I have Asheron's Call" to "in the world I chose". It lists the worlds,
keeps your accounts (passwords only in the system's secret store), checks that your client and data
files suit a world before connecting, starts the client, and keeps itself and the Dereth client up
to date. It can also host a world with Empyrean and build one with Lyceum.

| Folder | What |
|---|---|
| this folder | the launcher: a Tauri app (`Cargo.toml`, `src/`, `tauri.conf.json`) with its page in plain HTML, CSS and JavaScript (`ui/`). A workspace of its own |
| [`crates/launch/`](crates/launch/) | `dereth-launch`, the launcher's logic, with no window and no operating system in it. A member of the main workspace |
| [`assets/`](assets/) | the background paintings and the two typefaces the page uses |

## What a release is

| System | The launcher | The client |
|---|---|---|
| Windows | a zip with `dereth.exe`; unzip it anywhere, no installer | `dereth-client.exe`, beside `dereth.exe` |
| macOS | `Dereth.app` | inside it, in `Contents/MacOS` |
| Linux | an AppImage | inside it, beside the launcher |

## Clients and data files

- **The Dereth client** ships beside the launcher. A launcher built from the repository uses the
  workspace's build instead, `target/release/dereth-client` (then `debug`); `DERETH_CLIENT` names
  another.
- **The retail client** (Windows only): you can point the launcher at one retail install, the
  folder holding `acclient.exe`. Choosing another replaces it. On macOS and Linux the launcher
  offers only the Dereth client.
- **Data sets**, in the Library, come in two kinds, each with its own default:
  - **Modern**: `client_portal.dat`, `client_cell_1.dat` and `client_local_English.dat`, plus
    `client_highres.dat` when the folder has it.
  - **Classic**: `portal.dat` and `cell.dat` from an older install.

  A folder with only some of the files is refused, naming what's missing. A folder with both kinds
  is two sets. Adding the retail install adds its data files too.

On a world's page you choose **Dereth** or **Retail**. The retail client plays with the data files
beside it. The Dereth client plays with a Modern set, which every world needs, and a Classic set:
required for an era before Throne of Destiny (the world is drawn from it), optional otherwise (it
adds the classic interface). A world that patches data files, or ships its own, gets a private copy
that only the Dereth client can use.

The launcher starts the Dereth client with `--dat-dir` (the Modern set), `--classic-dat-dir`, and
`--era` and `--era-features` for a world that names its era. Both clients get the account and
password as `-a <account> -v <password>`. The launcher shows and logs that command line with the
password replaced by `***`.

## The world list

Worlds has three sections: **Custom** (servers you added), **Online** (listed worlds that are up,
or not asked yet) and **Offline** (closed at first). A search shows every match. Each row shows the
world's name, emulator and version (`Empyrean | v0.1.2`), era (`End of Retail` unless the world
says otherwise), rules and players. Click a row to open it: its description, website, Discord and
systems. **PLAY** opens the world's page.

A server you add by hand needs a name, host, port, emulator and rules; its era is whatever it says
when asked.

## Join links

A host can send a friend one link: `https://dereth.network/join#host=...&port=...&name=...`. The
page offers **Open in Dereth** and **Get Dereth**. The world's details are after the `#`, which a
browser never sends, so the site never sees them. **Open in Dereth** opens the launcher's own
`dereth://join?...` link: Worlds opens with Add world filled in, and nothing happens until you
press Add. A link never connects, logs in or carries an account or password. Its host and port are
checked like a world added by hand, and its name is cut to 80 characters.

Only a release registers the `dereth://` scheme (in `HKEY_CURRENT_USER\Software\Classes\dereth`
on Windows, the desktop entry and `mimeapps.list` on Linux, `Info.plist` on macOS), so a build from
a checkout never takes it over.

## The interface

On first start, while the client's preferences name no interface, the launcher asks which to play
in: **Horizon** (chosen to begin with), **Modern** or **Classic**. It writes the answer as
`Interface=<name>` under `[UI]` in the client's `UserPreferences.ini`, keeping every other line,
and never asks again. You can change it later in the client's options or in the Library, except
while a Dereth client the launcher started is running.

The file is the one the client reads: in `DERETH_SETTINGS_DIR` if set, else a
`UserPreferences.ini` beside the client, else the client's settings folder.

## A pad

On Windows the launcher can be driven with a pad while its window is in front:

| On the pad | On the page |
|---|---|
| d-pad or left stick | moves the focus that way |
| A (Cross) | presses the focused control; Enter in a text box; the next choice on a list |
| B (Circle) | steps back: closes a row or form, leaves a world's page, or goes back to Play |
| shoulder buttons | the rail's pages in turn: Play, Worlds, Accounts, Empyrean, Lyceum, Library |
| right stick | scrolls the list under the focus |
| left and right on a list | its previous and next choice |

Moving the pointer hides the pad's focus again. macOS and Linux read no pad yet.

## Discord

While a game the launcher started is running, Discord shows **Playing Dereth Network** on your
profile, with the world's name, the client (`Dereth` or `Retail client`), the era and the time
played. With several games running it shows the last one started. It never names the character
or the account.

It works with both clients on every world: the launcher talks to the Discord app on the same
computer, with nothing to sign in to. If Discord isn't running nothing happens, and the launcher
looks again every 10 seconds. The Library's **Show what I'm playing on Discord** turns it off.
`DERETH_DISCORD_APPLICATION_ID` names another Discord application, and
`DERETH_DISCORD_LARGE_IMAGE` a picture from its art.

## Where things are

| What | Where |
|---|---|
| settings: `launcher-state.json` (the library, accounts, favorites, recent worlds) | `%APPDATA%\Dereth\launcher` on Windows, `~/Library/Application Support/Dereth/launcher` on macOS, `$XDG_CONFIG_HOME/dereth/launcher` (`~/.config`) on Linux |
| data: private copies of data files, the web view's cache, the world list | `%LOCALAPPDATA%\Dereth\launcher` on Windows, the settings folder on macOS, `$XDG_DATA_HOME/dereth/launcher` (`~/.local/share`) on Linux |
| both, in one folder of your choosing | `DERETH_STATE_DIR` |
| passwords | Windows Credential Manager, the macOS login Keychain, or the Linux desktop's keyring (Secret Service), one entry per account per world: `dereth:world/<slug>/<account>` |

The launcher writes nothing outside its own `launcher` folders, except the interface in the
client's preferences.

**The world list** is the Dereth Network hub's (`https://hub.dereth.network/v1/servers`). The hub
imports the community list (`Servers.xml` from
[acresources/serverslist](https://github.com/acresources/serverslist)) daily and adds the worlds
registered with it. When the hub doesn't answer, the launcher reads the community list itself. It
fetches at most once a day (the refresh button fetches again) and keeps the copy as
`world-list.json`. `DERETH_SERVERS_LIST` names a `Servers.xml` to read instead, and
`DERETH_HUB_LIST` another hub's list.

**A world's era and systems** come from its Empyrean status, else the launcher's table of worlds
that run a client of their own, else the end of retail.

**Whether a world is up**, first answer wins:

1. its Empyrean status document, if it has one;
2. the status ping on its game port, which also gives the player count, era, systems and server
   version, asked again every minute;
3. what the hub's list says, from the heartbeat every world on the Dereth Network sends (ACE worlds
   with the network's mod included), read about once a minute;
4. the server-tracker login, which ACE, GDLE and Empyrean all answer: up or down, with no player
   count.

On Windows and Linux, a button beside the version under the rail creates a desktop shortcut. On
Linux each start also writes the launcher's application entry (`network.dereth.dereth.desktop`
and its icon under `$XDG_DATA_HOME`), so its window gets its name and icon.

## Updates

At start a release asks GitHub's API for the repository's releases and picks the newest Dereth
release: tagged `dereth-v<MAJOR>.<MINOR>.<PATCH>`, not a draft or pre-release, newest by version,
never by date. Empyrean's and Lyceum's releases are passed over, and GitHub's "latest" mark doesn't
matter. The repository is the one the build names (`DERETH_BUILD_SOURCE_URL` at compile time; a
fork's build names its own), else `https://github.com/dereth-network/dereth`. `DERETH_UPDATE_URL`
names a `latest.json` directly instead. If the check fails, the launcher logs it, says nothing and
tries again next start. A launcher built from the repository never checks.

When the release's `latest.json` names a newer version, the launcher downloads this platform's file
in the background and checks its signature against the update key (`plugins.updater.pubkey` in
`tauri.conf.json`) before offering a restart. `latest.json` has a `url` and a minisign `signature`
per platform (`windows-x86_64`, `darwin-aarch64`, `darwin-x86_64`, `linux-x86_64`); `cargo xtask
package dereth` makes both ([RELEASING.md](RELEASING.md)). On macOS and Linux the updater replaces
`Dereth.app` or the AppImage. On Windows the file is the release zip, which the launcher installs
itself: it unpacks it into `.dereth-update`, renames each file it replaces aside (`dereth.exe`
becomes `dereth.exe.dereth-old`), moves the new ones in and restarts. A failure part-way puts
everything back.

On Windows the page is drawn by the Microsoft Edge WebView2 runtime, which Windows 11 includes and
nearly every Windows 10 has. If it's missing, the launcher says so and links the download.

## Hosting a world: Empyrean

The rail has **Play**, **Worlds** and **Accounts**; then **Empyrean** and **Lyceum**; then the
**Library**. Most players never open the middle two, so neither program comes with the launcher.

**Getting them.** Each page shows the newest version and offers **Download**, then **Update to
<version>** when a newer one is out, and **Remove**. The launcher asks GitHub when either page opens
(at most every ten minutes) for the newest release tagged `empyrean-v<version>` or
`lyceum-v<version>`. It downloads this system's file and the release's `SHA256SUMS`, and checks one
against the other before unpacking. Offline, or while GitHub is limiting requests, the page says so
and offers **Try again**. Removing a program keeps your worlds.

A development build (one release packaging didn't make) uses the newest `empyrean-server` and
`lyceum-editor` beside the launcher or in its checkout's `target/release`, `target/test-release` or
`target/debug`, and shows `(Local Build)`. Its worlds run that build and never update from a
release. `cargo run -- --downloads` makes it download the programs as a release does.

**New world** asks for a name, a Modern data set, a port no other world uses, and two switches, both
on to begin with: **Automatically create accounts** and **Allow other players**. Each world's row
shows its state, Empyrean version, players online and uptime
(`Running · Empyrean v0.3.3 · 2 online · up 1 h 4 min`). A world's page has **Start**, **Stop**
(which saves first), **Play** and **Settings**, and four sections:

- **Connect:** its local and public addresses with **Copy**, whether friends elsewhere can reach it,
  **Check from outside**, and **Copy link** for its join link.
- **Console:** its log, with a search box and **Save log**, and, while it runs, a command line: a
  console command, **Add account...**, **Message everyone...**, **Save** or **Shut down in 5
  minutes** (which **Cancel shutdown** stops).
- **Lyceum:** the world base (the ACE world release its database came from, such as `ACE End of
  Retail v0.9.295`) and what it runs, `Unmodified.` or a Lyceum world and its version. From here
  you can publish a Lyceum world to it, publish the latest version, or restore the world base. The
  world must be stopped.
- **Advanced:** **Check for Updates**, **Open world folder**, **Run via System Service** (so it
  keeps running with the launcher closed), **List on the Dereth Network**, and **Delete World**.

**Settings** edits the parts of `empyrean.toml` that matter:

| Section | What |
|---|---|
| World | name, era, data set, world base, the character screen's welcome, and port (the world takes the one after it too). Infiltration needs a Classic set, any other era a Modern one. Each era has its own world base (`world.pack`, `world.pack.infiltration`), which **Get world database** downloads |
| Players | the two switches, player limits overall and per IP address, and the shutdown warning |
| Era systems | each of the era's systems; **Use defaults for** puts them back |
| Websocket | for browser players: on or off, a TLS proxy or a certificate and key, the address, the pages allowed to connect (`https://play.dereth.network` to begin with) and the public address |
| Dereth Network | on or off, the world's slug, and its key, pasted in and kept in `network.key`, never logged |
| Lyceum world | what it plays, and the overlay's patching rates |
| Advanced | the log level |

The launcher reads a world's player count from its status endpoint on `127.0.0.1`, on a port it
picks from 9100 up and never shows. Settings keeps every other key and comment in the file, and
**Open empyrean.toml** opens it for the rest. A world reads its settings when it starts. Closing the
launcher asks to stop the worlds it started.

**Can friends reach it?** The launcher asks the router over UPnP for its outside address and which
computer it forwards each game port to; it changes nothing on the router. Friends elsewhere need the
router to forward the game port and the one after it (UDP) to this computer. An internet provider
that shares one address among many homes (a private or `100.64.0.0/10` outside address) keeps them
out. **Check from outside** settles it: the Dereth Network's hub tries the address the request came
from at the world's port and says whether it reached the world. The hub allows a few checks an
hour from one address.

**Other worlds on this computer.** The page also lists Empyrean, ACE and GDLE servers already
running that the launcher didn't start, with their status, log and folders:

| Server | Configuration | Log |
|---|---|---|
| Empyrean | `--config <file>`, else `empyrean.toml` where it was started, else beside `empyrean-server` | the newest `.log` beside the configuration or program, or in a `logs` folder there; or its admin channel |
| ACE | `Config.js` where it was started, else beside the program | the file `log4net.config` names, else `ACE_Log.txt` beside the program |
| GDLE | `-c` or `--config <file>`, else `server.cfg`, else `debug_server.cfg`, where it was started | the file `GDLELogging.conf` names, else `server.log` |

For an Empyrean, the launcher reads `admin-channel.json` in its folder to stream its log, send
console commands and stop it cleanly. For a Windows service, **Let the launcher start and stop this
service** asks once for administrator permission; a `systemctl --user` service is started and
stopped directly, and for a system service the page shows the command. An Empyrean seen running
from a folder is remembered, so **Start** can run it again.

## Building a world: Lyceum

Lyceum's page lists **Your Worlds** (in the Lyceum folder in your Documents; **Open Folder** shows
it) and **Other Worlds** (others Lyceum or the launcher opened). A world in an older format gets
**Upgrade**, which brings it up to date and keeps a backup.

Opening a row shows the world's era, when it was last saved, and what it changes (landblocks,
statics, buildings, dungeons, rooms, spawns, weenies, textures and models). Then:

- **Export For** Empyrean, ACE or GDLE writes the world for that server (`lyceum export`) into a
  folder you choose.
- **Publish To** publishes it to one of your hosted worlds.
- **Delete** asks to confirm, then deletes the world's folder. It waits until Lyceum is closed. The
  worlds it was published to keep playing it.

**New World** asks for a name, a folder and an era (End of Retail or Infiltration), makes an empty
world and opens the editor on it. **Open World** opens any world by its folder.

Publishing builds the saved world with the `lyceum` command into the hosted world, which must be
stopped and plays it from then on. Close Lyceum first. Each publish that changes something gets
the next version number, and the world keeps the three newest. Players need the Dereth client,
which the world patches as it joins. A world with a content overlay (`world_overlay_path`) can't be
published to until the overlay is removed or built into its world database.

For scripts, with Lyceum closed, `dereth --publish-host <world-folder> <world.toml>
<lyceum-program>` publishes without a window and prints the result as JSON.

### Where hosting and building keep things

| What | Where |
|---|---|
| Empyrean and Lyceum as downloaded | `components/empyrean/<version>` and `components/lyceum/<version>` in the launcher's data folder, with `current.json` naming the version in use |
| each world you host | `hosts/<id>` in the launcher's data folder: `empyrean.toml`, `auth.db` (accounts), `shard.db` (characters), `world.pack` and `world.pack.base` (the ACE release it came from), `server.log` and `server.log.old`, `runtime/<version>` (its own Empyrean), `published/snapshot-<n>` (published Lyceum worlds), `client-overlays` (the launcher's client's patches) |
| backups | `auth.db.backup-update-v<old>-v<new>-<time>` and the same for `shard.db`, made before each update; the newest three are kept |
| ACE's world release, as downloaded | Empyrean's cache: `%LOCALAPPDATA%\Empyrean\world-database`, `~/Library/Caches/Empyrean/world-database`, `$XDG_CACHE_HOME/empyrean/world-database` |
| a world running in the background | Windows: the service `dereth-<id>`; Linux: `~/.config/systemd/user/dereth-<id>.service`; macOS: `~/Library/LaunchAgents/dereth-<id>.plist` |
| other Empyrean worlds the launcher remembers, and their output | `local-worlds.json` and `local-logs/` in the launcher's data folder |
| Lyceum's worlds | `Lyceum` in your Documents unless you chose another folder |
| the Lyceum worlds opened last | `recent-worlds.toml` in Lyceum's settings folder (`Dereth/lyceum`), and `lyceum-worlds.json` in the launcher's data folder |

**Delete World** deletes the world's folder, its databases included.

## Building

```sh
cargo test -p dereth-launch        # the logic crate, from the repository root
cd dereth/launcher
cargo run                          # the launcher, in a native window
cargo run -- --downloads           # the same, downloading Empyrean and Lyceum as a release does
cargo test                         # the app's own tests
cargo tauri build                  # the release build (`cargo install tauri-cli --version ^2 --locked`)
```

The Tauri app is kept out of the main workspace, so nothing there needs a web view or, on Linux,
WebKitGTK. There's no Node, npm or bundler: `ui/` is plain HTML, CSS and JavaScript, embedded at
build time. `build.rs` copies the paintings and typefaces from `assets/` and the emblem and icons
from `dereth/client/assets` into `ui/assets` (ignored by git). On macOS it also compiles the
client's `Dereth.icon` with Xcode's `actool`; only `tauri.macos.conf.json` names the results.

On Windows `cargo tauri build` leaves `target/release/dereth.exe`. On Linux it needs Tauri's build
dependencies (WebKitGTK 4.1 and friends). A release, with the client inside, is built by
`cargo xtask package dereth` ([RELEASING.md](RELEASING.md)).

### The page in a browser

The page also runs in a browser against demo data (`ui/demo.js`), which is how its screenshots are
taken:

```sh
cd dereth/launcher && cargo build          # once, to copy the art into ui/assets
cd ui && python3 -m http.server 8765 --bind 127.0.0.1    # then open http://127.0.0.1:8765/
```

A hash opens a screen directly, such as `#worlds`, `#world=eulmore`, `#library`, `#server=running`
or `#make=open`; `?dev=1` shows a development build. The full list is at the top of `demoJump` in
`ui/app.js`. The window opens at, and can't be made smaller than, 1100 by 700.

### The look

What you press is pixel art: the rail, the buttons and the PLAY plate are pixel-grid SVG frames.
What holds or shows something is quiet: dark glass panels with a thin gold line, plain wells for
inputs, and a dot with words for a world's status. The paintings are scaled with
`image-rendering: pixelated`. Text is Cinzel for display and EB Garamond to read.
