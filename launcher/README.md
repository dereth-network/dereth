# The Dereth Network launcher

The launcher gets you from "I have Asheron's Call" to "in the world I chose". It lists the worlds,
keeps your accounts (passwords only in the system's secret store), checks that your client and data
files suit a world before connecting, starts the client, and keeps itself and the Dereth client up
to date. It can also host a world with Empyrean and build one with Lyceum. A portal above the rail
turns the page to Asheron's Call 2: Auberean, its client; Yalain, a world on this computer; and
Athenaeum, its world builder.

One launcher serves both games. Dereth's release carries it with the Dereth client, and Auberean's
with the Auberean client and a small `launcher-defaults.json` that opens a first run in Auberean's
realm and keeps Auberean's client up to date ([RELEASING.md](RELEASING.md)). The other game's client
is downloaded the first time it is wanted. The window is titled Dereth Network, and its icon is the network's emblem; the game clients keep their own.

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

The plugin author's tool, `dereth-plugin`, comes with each one beside the client.

## Clients and data files

- **The Dereth client** comes in Dereth's package, beside the launcher, and is copied to the
  components folder at the first start (`components/dereth/<version>`, with the plugin tool, MoltenVK
  and any `UserPreferences.ini` beside it). It runs from there: an update replaces the launcher's
  whole bundle on macOS and Linux. It's updated from its game's own releases, apart from the
  launcher. A launcher built from the repository uses the workspace's build instead, `target/release/dereth-client` (then `debug`); `DERETH_CLIENT` names
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
| shoulder buttons | the rail's pages in turn: Play, Worlds, Accounts, Plugins with `--plugins`, Empyrean, Lyceum and Library |
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
| settings: `launcher-state.json` (the library, accounts, favorites, recent worlds), and Auberean's realm's own in `auberean-state.json` beside it (its Library, the worlds added for it and everything played there), which a launcher from before the realm never reads or writes | `%APPDATA%\Dereth\launcher` on Windows, `~/Library/Application Support/Dereth/launcher` on macOS, `$XDG_CONFIG_HOME/dereth/launcher` (`~/.config`) on Linux |
| data: private copies of data files, the web view's cache, the world list | `%LOCALAPPDATA%\Dereth\launcher` on Windows, the settings folder on macOS, `$XDG_DATA_HOME/dereth/launcher` (`~/.local/share`) on Linux |
| both, in one folder of your choosing | `DERETH_STATE_DIR` |
| passwords | Windows Credential Manager, the macOS login Keychain, or the Linux desktop's keyring (Secret Service), one entry per account per world: `dereth:world/<slug>/<account>`. With no keyring, the passwords earlier launchers made up for Asheron's Call 2 accounts are in the data folder's `auberean/secrets.json`, readable by you alone, and still play those accounts; no launcher makes one now |

The launcher writes nothing outside its own `launcher` folders, except the interface in the
client's preferences.

**The world list** is the Dereth Network hub's (`https://hub.dereth.network/v1/servers`). The hub
imports the community list (`Servers.xml` from
[acresources/serverslist](https://github.com/acresources/serverslist)) daily and adds the worlds
registered with it. When the hub doesn't answer, the launcher reads the community list itself. It
fetches at most once a day (the refresh button fetches again) and keeps the copy as
`world-list.json`. `DERETH_SERVERS_LIST` names a `Servers.xml` to read instead, and
`DERETH_HUB_LIST` another hub's list.

**A world played through a relay** speaks another wire format than Dereth's. The launcher keeps
these in a table of its own, and lists them. Play starts `empyrean-relay`, which comes with Dereth
beside the client, on this computer's loopback address and starts the client towards it; the relay
goes when the client does. Otherwise the launcher only asks such a world whether it is up, as it asks any
world.

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

A world with no count of its own takes the hub's, which may come from a player's Dereth client or
from TreeStats. The count grows dimmer as it ages, and hovering over it says where it came from and
how old it is.

On Windows and Linux, a button beside the version under the rail creates a desktop shortcut. On
Linux each start also writes the launcher's application entry (`network.dereth.dereth.desktop`
and its icon under `$XDG_DATA_HOME`), and the Dereth client window's hidden entry with the client's
own icon, so each window gets its name and icon.

## Plugins

Plugins are experimental. The **Plugins** screen installs plugins for the Dereth client from plugin
sources, each a `plugins.json` at an `https://` address listing plugins at their latest release ([`docs/plugins/sources.md`](../docs/plugins/sources.md)).
Dereth's own source is there from the start, with nothing installed; the player adds others by
their address, once they confirm it, and removes them. The screen has four sections that fold,
only the first open at first: the plugins installed; the ones the sources offer that are not
installed, each with its source, what it does and what it uses; the sources; and **Make Your Own**,
which makes a new plugin repository in an empty folder the author chooses, or adds a new plugin to
a plugin repository, as `dereth-plugin new` does
([`docs/plugins/guide/publishing.md`](../docs/plugins/guide/publishing.md)). A plugin's row
opens to everything known about it, and each plugin is installed by itself. A zip is kept only when it is the file its source lists and
its manifest says what its entry says; an id under `network.dereth.` installs only from Dereth's
source.

The launcher reads every source at every start, and again on **Refresh**, which says what is new. An installed plugin updates
only from its own source, to a newer release: by hand, or on its own as **Automatically update
plugins** says (none, all, or only Dereth's, the setting at first). A release that asks for a
permission or a web domain the installed one did not always waits for the player.
Installed also lists the plugins in **folders on this computer** (**Add Plugin Folder**, which
adds the folder chosen at once), marked **Local**; an open Local row's **Location** line has **Open**
and **Remove**, which deletes nothing. A folder is one plugin's own or a plugin repository, whose
plugins are the folders directly in it as `dereth-plugin` has them; any other is refused. Make Your
Own adds the folder it wrote in. They go to the client as they are on disk, never updated; one with an
installed plugin's id is loaded in its place, and the installed one keeps updating.
A folder Make Your Own wrote in also gets a copy of the `dereth-plugin` beside the launcher at its
root, with the `.gitignore` lines that keep it out of git; at every start the launcher makes each
such copy its own tool again, so an update reaches it. Its row says so when the copy could not be
written. A folder added any other way gets none, and one taken off the list keeps its copy.

Each such folder, then each installed plugin's zip, goes to the client with `--offer-plugins`, and
the client loads each the first time it sees it, until the player unticks it in the plugin panel. A source that cannot be read at start says so on its row, not in the notice. The
logic is `dereth-launch`'s `plugins` module; the files are under the launcher's data folder, in
`plugins` (`sources.json`, which also keeps the folders on this computer and those it keeps the
tool in, `installed.json`, a folder per plugin).

## Updates

At start a release asks GitHub's API for the repository's releases and picks the newest launcher
release: tagged `launcher-v<MAJOR>.<MINOR>.<PATCH>`, not a draft or pre-release, newest by version,
never by date. Every other product's releases are passed over, and GitHub's "latest" mark doesn't
matter. It also fetches its game's newer client from that game's own newest release, when it has a
client to replace. The repository is the one the build names (`DERETH_BUILD_SOURCE_URL` at compile time; a
fork's build names its own), else `https://github.com/dereth-network/dereth`. `DERETH_UPDATE_URL`
names a `latest.json` directly instead. If the check fails, the launcher logs it, says nothing and
tries again next start. A launcher built from the repository never checks.

When the release's `latest.json` names a newer version, the launcher downloads this platform's file
in the background and checks its signature against the update key (`plugins.updater.pubkey` in
`tauri.conf.json`) before offering a restart. `latest.json` has a `url` and a minisign `signature`
per platform (`windows-x86_64`, `darwin-aarch64`, `linux-x86_64`); `cargo xtask
package dereth` makes both ([RELEASING.md](RELEASING.md)). On macOS and Linux the updater replaces
`Dereth.app` or the AppImage. On Windows the file is the release zip, which the launcher installs
itself: it unpacks it into `.dereth-update`, renames each file it replaces aside (`dereth.exe`
becomes `dereth.exe.dereth-old`), moves the new ones in and restarts. A failure part-way puts
everything back.

On Windows the page is drawn by the Microsoft Edge WebView2 runtime, which Windows 11 includes and
nearly every Windows 10 has. If it's missing, the launcher says so and links the download.

## Hosting a world: Empyrean

The rail has **Play**, **Worlds**, **Accounts** and, started with `--plugins`, **Plugins**; then
**Empyrean** and **Lyceum**; then the **Library**. Most players never open the middle two, so neither program comes
with the launcher.

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
| World | name, era, data set, world base, the character screen's welcome, and port (the world takes the one after it too). Launch needs the 1999 pair, Infiltration a Classic set, the end of retail a Modern one. Each era has its own world base (`world.pack`, `world.pack.infiltration`, `world.pack.launch`), which **Get world database** downloads |
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
| Auberean, Yalain and Athenaeum as downloaded | `components/auberean/<version>`, `components/yalain/<version>` and `components/athenaeum/<version>` there, the same way |
| the Dereth client as downloaded (by a launcher from Auberean's release) | `components/dereth/<version>` there, the same way |
| the world on this computer (Auberean's realm) | `auberean/yalain.sqlite` (accounts and characters) and `auberean/yalain.log` (its log, the one before it as `yalain.log.old`) in the launcher's data folder |
| Athenaeum's worlds | `Athenaeum` in your Documents; the ones the launcher made or opened, in `athenaeum-worlds.json` in its data folder |
| what Auberean says as it plays | `auberean/auberean-<account>.log` in the launcher's data folder, written again at each start; when Auberean stops with an error as it starts, the page shows its last line |
| each world you host | `hosts/<id>` in the launcher's data folder: `empyrean.toml`, `auth.db` (accounts), `shard.db` (characters), `world.pack` and `world.pack.base` (the ACE release it came from), `server.log` and `server.log.old`, `runtime/<version>` (its own Empyrean), `published/snapshot-<n>` (published Lyceum worlds), `client-overlays` (the launcher's client's patches); a Yalain world's, in `hosts-auberean/<id>` beside it, where a launcher from before Auberean's realm never looks (one an earlier build made in `hosts` moves there at start while it is stopped and runs no system service): `yalain.toml`, `yalain.sqlite` (accounts and characters) and its update backups, its log, `runtime/<version>` (its own Yalain), and `published/snapshot-<n>` (published Athenaeum worlds) |
| backups | `auth.db.backup-update-v<old>-v<new>-<time>` and the same for `shard.db`, made before each update; the newest three are kept |
| ACE's world release, as downloaded | Empyrean's cache: `%LOCALAPPDATA%\Empyrean\world-database`, `~/Library/Caches/Empyrean/world-database`, `$XDG_CACHE_HOME/empyrean/world-database` |
| a world running in the background | Windows: the service `dereth-<id>`; Linux: `~/.config/systemd/user/dereth-<id>.service`; macOS: `~/Library/LaunchAgents/dereth-<id>.plist` |
| other Empyrean worlds the launcher remembers, and their output | `local-worlds.json` and `local-logs/` in the launcher's data folder |
| Lyceum's worlds | `Lyceum` in your Documents unless you chose another folder |
| the Lyceum worlds opened last | `recent-worlds.toml` in Lyceum's settings folder (`Dereth/lyceum`), and `lyceum-worlds.json` in the launcher's data folder |

**Delete World** deletes the world's folder, its databases included.

## Asheron's Call 2: Auberean's realm

The two portals above the rail pick the game the page is about: Dereth's, or Auberean's. The page
opens in the one it last showed; a first run opens in the one its release names (Auberean's for the
launcher from Auberean's release). In Auberean's realm the rail's Empyrean and Lyceum are **Yalain**
and **Athenaeum**, and Plugins (Dereth's alone) folds away.

**The Library.** Nothing of Asheron's Call 2 comes with the launcher or is downloaded. The realm's
Library is Dereth's, with the game's own things in it:

- **Retail client** (Windows only): the folder that holds `AC2client.exe`, which plays with the
  data files beside it. **Verify** checks every file of the folder against the September 2013
  client's list of files (its data files and settings left out), **Change** points at another,
  **Remove** forgets it.
- **Auberean client:** its version and the folder it is in, with its update and Remove.
- **Data sets:** the `*.dat` files from an install: `portal.dat`, `cell_1.dat` and
  `local_English.dat`, with `highres.dat` when the folder has it. Each set shows its files'
  iterations, portal/cell/local/highres (`End of Retail 137/7/9/7`, what every client told the
  servers at the end; any other is `Older`). **+ Add data files** adds a folder; a folder with only
  some of the files is refused, naming what's missing. The first set is the default, the one
  Auberean, Yalain and Athenaeum are given unless another is chosen; **Set default** makes another
  the default, and **Remove** forgets one (its files stay). Adding the retail client adds its data
  files too.

Each file's header must say it is that file, written at the size it is (a copy cut short is
caught). The launcher keeps the Library in `auberean-state.json`, apart from Asheron's Call's; it
never copies or changes a file in it. With no set yet, Play, Yalain's and Athenaeum's pages offer
**Choose Your Asheron's Call 2 Folder**, which adds an install's retail client and data set at
once. A launcher that kept one install folder before takes it into the Library at its next start.

**The programs.** The launcher from Auberean's release has the Auberean client beside it. Yalain
and Athenaeum, and the Auberean client for a launcher from Dereth's release, are downloaded,
updated and removed as Empyrean and Lyceum are, each its own newest release (`auberean-v<version>`,
`yalain-v<version>`, `athenaeum-v<version>`), checked against the release's `SHA256SUMS`. The
Auberean client comes out of its release, which is the launcher with the client beside it (on
Linux the AppImage unpacks itself to give it up); Athenaeum's AppImage is kept as it is. A
client packaged beside the launcher is copied out as Dereth's is, and the package's own
`launcher-defaults.json` is kept in the data folder, so the game it named outlasts an update. In the
same way a launcher from Auberean's release offers the Dereth client on Play when it is first wanted, from the
newest `dereth-v<version>` release. A development build uses its checkout's builds, as for
Empyrean and Lyceum.

**Join links** name their game: `dereth://join?host=...&port=...&name=...&game=auberean` (and
the site's `https://dereth.network/join#...&game=auberean`) adds the world to Auberean's realm. A
link without `game` is Dereth's, as every link was before; a game this launcher does not know is
refused. Links the launcher writes name the game only for Auberean.

**Play** opens a world's page, a world you host as a listed one: Auberean is started with `--dats
<data set> --host <address> --port <port> --account <name> --glsticketdirect <password>`, the
retail client's own options. A world of Asheron's Call 2's asks, as Asheron's Call's do, for an
account and a password of your own, with the same field and **Save password securely** box; the
world makes the account the first time it logs in, with that password. The password goes to the
client, and is kept in the system's secret store only when the box is checked. **Accounts** shows
an Asheron's Call 2 account's saved password when you ask. The launcher logs the client's command
line with the password replaced by `***`.

**Yalain's page** hosts worlds as Empyrean's page does: **New World** (a name, a data set from the
Library, a port: the world takes it and the three after it, and the two switches), each world in
its own folder under `hosts-auberean/<id>` with its settings in `yalain.toml` (in `empyrean.toml`'s
form), **Start** (which fetches the world database its Yalain pins into your user cache the first time,
`yalain world fetch`), **Stop**, **Play**, **Settings**, its console and log. A world's page has
the same **Websocket** settings, and **Athenaeum** where Empyrean's has **Lyceum**: its world base (`Yalain v0.21.0`, the world
database its Yalain pins) and what it runs, `Unmodified.` or an Athenaeum world and its version;
**Publish Athenaeum World** with a picker of Athenaeum's worlds (or Other...), **Publish Latest**
once the world was saved since, and **Restore World Base**. Its Settings say `Plays the Athenaeum world <name> version <n>,
published <date>.` The world must be stopped. Its console has Empyrean's presets (**Message
everyone...** is `gamecast`). **Advanced** has **Check for Updates** as Empyrean's has, over
`yalain-v<version>` releases and the `upgrade` their `release.json` declares: a patch downloads by
itself, anything else waits for **Update to <version>**, and the download goes in at the next stop
or start: the store backed up (`yalain.sqlite.backup-update-v<old>-v<new>-<time>`, the newest three
kept), the world database the release pins fetched first when it pins another, then the program
replaced in `runtime/<version>`. A release that cannot go in leaves the world on the one it ran.
**Undo last update** puts the previous program and the store from before the update back. A world
that finds its ports taken says so on its page. **Run via System Service** works as Empyrean's; on
Windows the service's account cannot read your user cache, so a Yalain world set to run as one
keeps its world database in its own folder (`world-database/<tag>`, which `[server]
world_database_path`, or for a published Athenaeum world `world_base_path`, names) from then on.

Publishing copies the saved world's own files (`world.toml`, `world.lock` and its packs; not
`.playtest`, `build` or `characters`) into the next snapshot, `published/snapshot-<n>/world`,
fetches the world database release the world names in its `[base]` (if it names one), compiles the
copy against the world's data set as the server will (`yalain world compile`; a world that does
not compile is refused and the world keeps what it played), then points `[server]
world_database_path` at it. Each publish that changes something is the next version; the three
newest snapshots are kept, and the one the world plays always. An Athenaeum world changes only the
server's rows (people, objects, camps, places, talk, stores, quests), which every client is told of
as it plays, so nothing is sent to players' data files and the retail client sees it too.

**Athenaeum's page** makes and opens worlds. **New world** makes one with `athenaeum new` in an
`Athenaeum` folder in your Documents and opens the editor on it; **Open World** opens one by its
folder. The editor is started with `<world> --dats <default data set>`, and finds Yalain and Auberean
beside itself. A world's row opens to when it was saved, what it changes, **Export For** and
**Publish To**, which publishes it to one of your Yalain worlds, and **Delete**.

**Export For** writes the world for a Yalain that serves it elsewhere, as a world database package,
in a folder you choose (that folder when it is empty or an earlier export, else `<world>-yalain` in
it): its `world.toml`, naming the release of the world database it lies over (the one it names, else
the one Yalain pins), `world.lock` and its packs, checked with `yalain world compile` against your
default data set, and a `README.txt` saying how to serve it (`[server] world_database_path`) and
which release to fetch. Athenaeum changes only the server's rows, never the game's data files, so
every client, retail too, sees its changes from the server.

Started with `DERETH_SETTINGS_DIR` set, the launcher gives Auberean `AUBEREAN_SETTINGS_DIR` as a
folder `auberean` in it, so a trial run keeps away from the player's own settings.

## Building

```sh
cargo test -p dereth-launch        # the logic crate, from the repository root
cd launcher
cargo run                          # the launcher, in a native window
cargo run -- --downloads           # the same, downloading the servers and builders as a release does
cargo test                         # the app's own tests
cargo tauri build                  # the release build (`cargo install tauri-cli --version ^2 --locked`)
```

The Tauri app is kept out of the main workspace, so nothing there needs a web view or, on Linux,
WebKitGTK. There's no Node, npm or bundler: `ui/` is plain HTML, CSS and JavaScript, embedded at
build time. `build.rs` copies the paintings, emblems and typefaces from `assets/` into `ui/assets`
(ignored by git), and the launcher's own icon, the network's emblem, from `assets/icon` into
`icons/`: a 256-pixel PNG, the Windows icon (16 to 256 pixels) and the macOS `Network.icns` (16 to
1024), which only `tauri.macos.conf.json` names. All three are drawn down from the emblem's layered
source, `assets/Network.icon`, which a macOS build also compiles with Xcode's `actool` into the
`Assets.car` macOS 26 draws the icon from.

On Windows `cargo tauri build` leaves `target/release/dereth.exe`. On Linux it needs Tauri's build
dependencies (WebKitGTK 4.1 and friends). A release, with the client inside, is built by
`cargo xtask package dereth` ([RELEASING.md](RELEASING.md)).

### The page in a browser

The page also runs in a browser against demo data (`ui/demo.js`), which is how its screenshots are
taken:

```sh
cd launcher && cargo build          # once, to copy the art into ui/assets
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
