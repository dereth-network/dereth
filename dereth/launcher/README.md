# Dereth, the launcher

The launcher gets a player from "I have Asheron's Call" to "in the world I chose". It lists the
worlds, keeps the player's accounts (passwords only in the system's secret store), checks the
client and the data files against what a world expects before connecting, starts the client, and
keeps itself and the Dereth client up to date.

| folder | what |
|---|---|
| this folder | the launcher: a Tauri app (`Cargo.toml`, `src/`, `tauri.conf.json`), its page in plain HTML, CSS and JavaScript (`ui/`). A workspace of its own |
| [`crates/launch/`](crates/launch/) | `dereth-launch`, what the launcher knows, with no window and no operating system in it. A member of the main workspace |
| [`assets/`](assets/) | the background paintings and the two typefaces the page uses |

The Tauri app is kept out of the main workspace (it is no member there, and it has its own
`[workspace]` and lock file; `crates/launch` names the main workspace as its own), so nothing the main workspace builds or checks needs a web view
or, on Linux, WebKitGTK. `dereth-launch` is an ordinary workspace crate: `cargo test -p
dereth-launch` from the repository root.

## What a release is

| system | the launcher | the client |
|---|---|---|
| Windows | a zip with `dereth.exe`, no installer: unzip it anywhere | `dereth-client.exe`, beside `dereth.exe` |
| macOS | `Dereth.app` | inside it, in `Contents/MacOS` |
| Linux | an AppImage | inside it, beside the launcher |

## Clients and data files

- **The Dereth client** ships beside the launcher, so the launcher always knows it. A launcher built
  from the repository has none beside it and uses the workspace's own build,
  `target/release/dereth-client` (then `debug`); `DERETH_CLIENT` names another.
- **The retail client** (Windows only): the player can point the launcher at one retail install,
  the folder that holds `acclient.exe`. There is only ever one; choosing another replaces it. On
  macOS and Linux the launcher offers the Dereth client alone and never mentions retail.
- **Data sets**: a library of folders of data files, of two kinds, each with its own default.
  A **Modern** set is the `client_*.dat` files of a modern install: `client_portal.dat`,
  `client_cell_1.dat` and `client_local_English.dat`, with `client_highres.dat` when the folder has
  it (the client plays without it); a **Classic** set is
  both files of an older install, `portal.dat` and `cell.dat`. A folder with only some of either is
  refused, naming what it lacks. One folder may hold both kinds, and is then two sets. The retail
  client is `acclient.exe` with a Modern set beside it. The Library lists the two kinds apart. Adding the retail install
  adds its data files too.

On a world's page the player chooses **Dereth** or **Retail**. The retail client plays with the
data files beside it. The Dereth client plays with the Modern set and the Classic set the player
picks. Every world needs a Modern set. An era before Throne of Destiny needs a Classic set too,
which its world is drawn from, with the Modern set beside it for the interface; for any other era,
or when the era is not known, the Classic set is optional (beside the Modern one it gives the
classic interface and looks). The Modern set is `--dat-dir`, the Classic set `--classic-dat-dir`, and the world's era and systems `--era` and `--era-features` (the systems as
the shared bitfield, `<table version>:<hex>`: the era's table with what the world turned on or
off over it; neither switch for a world that names no era, which plays the end of retail). A world that patches data files over the network, or ships its own, gets a private copy, and only the
Dereth client can use one.

Both clients receive the account and password on their command line (`-a <account> -v <password>`).
The launcher shows and logs the command line with the password replaced by `***`.

## The world list

Worlds shows three sections, each opened and closed by its header: **Custom** (the servers the
player added, each with a bin to remove it), **Online** (listed worlds that are up, or not yet
asked) and **Offline** (closed at first). A search shows every match, closed section or not. Each
row names the world, its emulator and version (`ACE`, `Empyrean | v0.1.2`; the version only
when the world says it), its era (the one the world names, else `End of Retail`), its rules and
its players (`unknown` when nobody says). Clicking a row opens it, one at a time: its description,
website and Discord, how settled it says it is, and, for a world that names its era or systems,
those systems. Its PLAY opens the world's page, whose `<` comes back. A server added by hand is
given its name, host, port, emulator and rules; its era is what it says when asked.

## Join links

A host sends a friend one link for their world: `https://dereth.network/join#host=...&port=...&name=...`,
a page of the site that offers **Open in Dereth** and **Get Dereth**. The world's details are after
the `#`, which a browser never sends, so the site sees none of them. **Open in Dereth** opens
`dereth://join?host=...&port=...&name=...`, the launcher's own scheme: Worlds opens with Add world
filled in, and nothing more happens until the person presses Add. A link never connects, logs in,
or carries an account or a password; its host and port are checked as a world added by hand is, and
its name is cut to 80 characters (`dereth_launch::join`).

The system hands the launcher the link. On Windows and Linux the link is the program's argument:
a launcher started with one while another is open leaves the link in the data folder
(`join-request.txt`; `launcher-running` there says one is open) and ends, and the open launcher takes
it within a second and comes to the front. On macOS the system hands the open app the link through
Tauri's deep-link plugin.

Only a release registers the scheme, so a build from a checkout never takes it over. On Windows each
start writes `HKEY_CURRENT_USER\Software\Classes\dereth` (a URL protocol whose command is
`"<dereth.exe>" "%1"`), each value only when it differs; on Linux each start writes the launcher's
desktop entry with `MimeType=x-scheme-handler/dereth;` and `%u`, and makes it the scheme's default in
the user's `mimeapps.list`, every other line kept; on macOS the bundle's `Info.plist` names the
scheme (`plugins.deep-link` in `tauri.conf.json`, which packaging reads).

## The interface

The first time the launcher starts, while the Dereth client's preferences name no interface, it
asks which one to play in: **Horizon** (first, and chosen to begin with), **Modern** or
**Classic**, each with a picture of the screen and a line saying what it is. The answer goes into
the client's preferences file the way the client writes it, `Interface=<name>` under `[UI]`, every
other line kept, and the client starts in it. Once the file names an interface the launcher never
asks again. The file is the one the client reads when the launcher starts it: in the folder
`DERETH_SETTINGS_DIR` names, else the `UserPreferences.ini` beside the client (a portable
install), else in the client's settings folder (`%APPDATA%\Dereth\client` on Windows,
`~/Library/Application Support/Dereth/client` on macOS, `$XDG_CONFIG_HOME/dereth/client` on
Linux).

The client's own options change the interface, as before. The Library's Dereth client section
shows it and changes it too, except while a Dereth client the launcher started is running, which
saves its own when it closes. A client started without the launcher still starts a new player in
Horizon. On Windows the client's first run still copies the original game's settings in, with the
interface chosen here kept in them.

## A pad

On Windows the launcher can be played with a pad, read as the Dereth client reads it, while its
window is the one in front (a pad played in a game is not read here too):

| on the pad | on the page |
|---|---|
| d-pad, or the left stick | moves the focus to the control that way: a button, a row, a box or a list |
| bottom face button (A, Cross) | presses the focused control; in a text box, Enter; on a list, its next choice |
| right face button (B, Circle) | steps back: closes an open row or form, leaves a world's page, or goes back to Play |
| shoulder buttons | Play, Worlds, Accounts, Empyrean, Lyceum and Library, in turn, as the rail lists them |
| right stick | scrolls the list under the focus |
| left and right on a list | its previous and next choice |

A direction held moves once, then again after a pause, then steadily. While the pad is in use the
focus is drawn on every control, rows included; moving the pointer puts the page back as it was.
What each button does and which control a move reaches are `dereth-launch`'s (`pad`), so any
launcher can take a pad the same way. On macOS and Linux no pad is read yet, as in the client.

## Discord

While a game the launcher started is running, Discord shows it on the player's profile:
**Playing Dereth Network**, the world's name, then the client (`Dereth` or `Retail client`) and
the era (the one the world names, else `End of Retail`), and the time since the game started.
With several games running, it shows the last one started; when the games exit, it is cleared.
The character is not named, since the launcher does not know which one is played, and the account
never is.

It works for both clients on every world, with nothing to sign in to and no server involved: the
launcher talks to the Discord desktop app on the same computer, over the app's local socket
(`discord-ipc-0` to `discord-ipc-9`: a named pipe on Windows; elsewhere a Unix socket in the first
of `$XDG_RUNTIME_DIR`, `$TMPDIR`, `$TMP` and `$TEMP` that is set, else `/tmp`, and on Linux also
in the Flatpak and Snap app folders there), as the Dereth Network's Discord application,
`1557643227513757726`. With Discord not running, nothing happens and nothing is said; while a game
runs, the launcher looks for it again every 10 seconds, so a Discord started later, or restarted,
shows the game too. Closing the launcher clears it, even with the game still running.

The Library's **Show what I'm playing on Discord** turns it off (it is on to begin with, and kept
in `launcher-state.json`). `DERETH_DISCORD_APPLICATION_ID` names another Discord application, and
`DERETH_DISCORD_LARGE_IMAGE` the key of a picture among the application's uploaded art, shown
beside the activity; there is none by default.

The web client shows nothing on Discord: a page in a browser cannot reach the Discord app.

## Where things are

| what | where |
|---|---|
| settings: `launcher-state.json` (the library, accounts, favourites, recent worlds) | `%APPDATA%\Dereth\launcher` on Windows, `~/Library/Application Support/Dereth/launcher` on macOS, `$XDG_CONFIG_HOME/dereth/launcher` (`~/.config`) on Linux |
| data: private copies of data files, the web view's cache | `%LOCALAPPDATA%\Dereth\launcher` on Windows, the settings folder on macOS, `$XDG_DATA_HOME/dereth/launcher` (`~/.local/share`) on Linux |
| both, in one folder of your choosing | `DERETH_STATE_DIR` |
| passwords | Windows Credential Manager, the macOS login Keychain, or the Linux desktop's keyring (the Secret Service: GNOME Keyring, KWallet): one entry per account per world, named `dereth:world/<slug>/<account>` |
| the world list | the Dereth Network hub's list (`https://hub.dereth.network/v1/servers`, a hundred worlds a page), which imports the community's list, `Servers.xml` in [acresources/serverslist](https://github.com/acresources/serverslist), daily and adds the worlds registered at the hub (a world with no address yet is not shown, and a demonstration world is shown with the status `Test`); the community's own `Servers.xml` when the hub does not answer with its list; or the `Servers.xml` that `DERETH_SERVERS_LIST` names, read alone (`DERETH_HUB_LIST` names another hub's list, read in place of the network's: a release's trial runs on one). A world is kept under its name, whichever list it came from. Fetched at most once a day (the refresh button fetches it again) and kept in the data folder as `world-list.json`, with the sources it was read from: a launcher reading other sources (another hub's list, or a named one) reads its own list at once rather than show that copy; and the servers the player added, which are kept in the state. While the list shown is the hub's, the launcher also reads it again about once a minute for each world's status (below), kept in memory only, never from the day's copy |
| a world's era and systems | its Empyrean status document or status ping when it says them; else the launcher's table of worlds that run a client of their own; else the end of retail, with its systems. The player chooses neither; eras chosen in earlier versions are passed over |
| the interface the Dereth client starts in | the client's own preferences file, `UserPreferences.ini` (above, "The interface") |
| whether a world is up | its Empyrean status document when it has one; else, for a world that may run Empyrean, the status ping on its game port (`docs/networking/06-status-ping.md`), which also says how many are on, its era and systems, the server's version and its name, asked again every minute once answered; else, while the world list shown is the hub's, what the hub's list says of the world (matched by its name) from the world's own heartbeat to the hub, which every world linked to the Dereth Network sends, ACE worlds with the network's mod among them: up, starting (the hub's `maintenance`) or down, and how many are on, read about once a minute and with each fetch of the list, and taken only until the heartbeat expires; a read that fails changes nothing. Otherwise the server-tracker login (`acservertracker:jj9h26hcsggc`, no password), which ACE, GDLE and Empyrean all answer, sent when the list loads or is refreshed and when a world's page opens. It says up or down, never how many are on. A list `DERETH_SERVERS_LIST` names, and the community's own list read when the hub does not answer, bring no status: those worlds are asked themselves |

Nothing is kept beside the launcher, and the launcher writes nothing outside its own `launcher`
folders but the interface in the client's preferences: the rest of `Dereth` (`dereth` on Linux) is
the client's. Nothing is carried over from
where launchers before these folders kept their state.

The launcher has no settings screen: this table is where its folders are. Under the rail, the
footnote reads `Dereth <version> | dereth.network`, and on Windows and Linux a small shortcut
button follows, **Create desktop shortcut**: a `Dereth.lnk` (Windows) or `dereth.desktop` (Linux)
on the desktop, replaced when pressed again. macOS has no desktop shortcuts, so no button.

**On Linux**, every start makes sure of the launcher's entry in the desktop's application list, so
the window has its name and icon (the desktop matches a window to a `.desktop` file by its
application id): `$XDG_DATA_HOME/applications/network.dereth.dereth.desktop` and the icon
`$XDG_DATA_HOME/icons/hicolor/256x256/apps/network.dereth.dereth.png` (`~/.local/share` when
`XDG_DATA_HOME` is unset). The entry starts the AppImage (`$APPIMAGE`), or the launcher itself
outside one. Each file is written only when it differs, and nothing else is written. The window's
Wayland app id and X11 class are `network.dereth.dereth`, the entry's `StartupWMClass`.

## Updates

At start a release lists its repository's GitHub releases through GitHub's REST API
(`https://api.github.com/repos/<owner>/<name>/releases?per_page=100`, unauthenticated) and picks
the newest published Dereth release: tagged `dereth-v<MAJOR>.<MINOR>.<PATCH>`, not a draft, not a
pre-release, and newest by version compared as semver, never by date or by its place in the list.
Empyrean's releases, in the same repository, are passed over, and which release GitHub shows as
"latest" does not matter. The launcher then reads that release's `latest.json`. The repository is
the one the build names (`DERETH_BUILD_SOURCE_URL` at compile time, which `cargo xtask package
dereth` passes; a fork's build names its own), else `https://github.com/dereth-network/dereth`.
`DERETH_UPDATE_URL` overrides all of this and names a `latest.json` directly. When the list cannot
be fetched or read, or holds no such release, there is no update this time: the launcher logs it,
tells the player nothing, and tries again at the next start. `tauri.conf.json` names no updater
endpoint (`plugins.updater` holds only the signing key); the launcher hands the updater the address
it found.

When `latest.json` names a newer version, the launcher downloads that platform's file in the background and checks it
against the update-signing public key (`plugins.updater.pubkey`) before the page offers a restart.
A launcher built from the repository (no client beside it) never checks.

`latest.json` has one entry per platform under `platforms`, keyed `windows-x86_64`,
`darwin-aarch64` (and `darwin-x86_64` when there is an Intel build) and `linux-x86_64`, each a
`url` and the file's `signature` (minisign, as Tauri's signer writes it; `cargo xtask package
dereth` makes both, RELEASING.md):

- **Windows**: the release zip itself. The updater cannot install a zip, so the launcher does:
  it unpacks the zip into `.dereth-update` beside itself, renames each file it replaces aside
  (`dereth.exe` becomes `dereth.exe.dereth-old`; Windows allows renaming a running program),
  moves the new files into place, and starts the new `dereth.exe`. The next start removes the
  `.dereth-old` files. A failure part-way renames everything back.
- **macOS**: `Dereth.app` as a `.tar.gz`; the updater replaces the app bundle.
- **Linux**: the AppImage; the updater replaces it.

## WebView2

On Windows the page is drawn by the Microsoft Edge WebView2 runtime, which Windows 11 includes
and nearly every Windows 10 has. When it is missing the launcher says so, with the download page,
instead of failing without a word.

## Building

```sh
cargo test -p dereth-launch        # the logic crate, from the repository root
cd dereth/launcher
cargo run                          # the launcher, in a native window
cargo run -- --downloads           # the same, downloading Empyrean and Lyceum as a release does
cargo test                         # the app's own tests
cargo tauri build                  # the release build (`cargo install tauri-cli --version ^2 --locked`)
```

No Node, no npm and no bundler: `ui/` is plain HTML, CSS and JavaScript, embedded at build
time. `build.rs` copies the paintings and the typefaces from `assets/` into `ui/assets`,
and the emblem and the icons from the Dereth client's `dereth/client/assets` (all ignored by git).
On macOS it also compiles the client's layered `Dereth.icon` with Xcode's `actool`; the files that
makes (`Dereth.icns`, `Assets.car`) are named only in `tauri.macos.conf.json`, which only a
macOS build reads.

On Windows `cargo tauri build` leaves `target/release/dereth.exe` (the `app` and `appimage`
bundles are macOS's and Linux's). On Linux it needs Tauri's build dependencies (WebKitGTK 4.1 and
friends: see Tauri's prerequisites). A release, with the client inside, is built by
`cargo xtask package dereth` ([RELEASING.md](RELEASING.md)).

## Looking at the design without the app

The page also runs in a browser, against demo data (`ui/demo.js`), which is how its
screenshots are taken:

```sh
cd dereth/launcher && cargo build          # once, to copy the art into ui/assets
cd ui && python3 -m http.server 8765 --bind 127.0.0.1    # then open http://127.0.0.1:8765/
```

`#worlds`, `#worlds=add`, `#worlds=harvestgain` (that row open), `#world=eulmore`, `#library`, `#library=add`, `#accounts`,
`#first-run`, `#first-run=guide`, `#first-run=found` and `#interface` (the first start's
question) open a screen directly. The window opens
at, and cannot be made smaller than, 1100 x 700.

## The look

Two registers. What you press is soft pixel art: the rail, the buttons and the PLAY plate are
pixel-grid SVG frames with crisp edges and banded plates. What holds or shows something is quiet:
panels are dark glass over the painting with a thin gold line and rounded corners, text boxes and
drop-downs are plain wells, and a world's status is a plain dot with the state in words beside it.
The paintings are scaled with `image-rendering: pixelated`. Text stays type: Cinzel for display,
EB Garamond to read. The lockup keeps the painted emblem and DERETH NETWORK.

## Empyrean and Lyceum

The rail is in three parts, each apart from the next: **Play**, **Worlds** and **Accounts**; then
**Empyrean**, for hosting a world server of your own, and **Lyceum**, for building a world with
Dereth's world builder; then the **Library**. The two pages' headings are **Empyrean: Host a
World Server** and **Lyceum: Build a World**, each with its program's emblem. Most players never
open them, so neither program comes with the launcher.

**Getting them.** Each page names the program and the newest version GitHub has, in one line under
its title, and offers it: **Download Empyrean 0.3.3**, then, once it is here, `Empyrean v0.3.3`
with **Update to 0.3.4** once a newer one is out, and **Remove**. The launcher asks GitHub's
releases API once when either page opens (and at most every ten minutes), and takes the newest published release tagged `empyrean-v<version>` or
`lyceum-v<version>`: not a draft, not a pre-release, newest by version. It downloads that
release's file for this system, named as the release names it (`empyrean-<version>-<target>.zip`
or `.tar.gz`; `lyceum-<version>-windows-x86_64.zip`, `lyceum-<version>-linux-x86_64.tar.gz`,
`Lyceum-<version>-macos-<arch>.app.tar.gz`), and the release's `SHA256SUMS`, and checks one
against the other before unpacking it. Nothing is signed: the checksums come from the same
release, so they catch a damaged or altered download, as Empyrean's own updater does. Without a
connection, or while GitHub is limiting requests, the page says so and offers **Try again**;
what is already downloaded keeps working. A failed download changes nothing.

**A development build** (a launcher the release packaging did not make, which it knows because
packaging stamps the commit into the program) downloads neither program when its checkout has
built it: it uses the newest `empyrean-server` and `lyceum-editor` beside the launcher or in the
checkout's `target/release`, `target/test-release` or `target/debug`, and the line under the
title says its version and **(Local Build)**, `Empyrean v0.4.0 (Local Build)`, as the build says
it (`empyrean-server --release-facts`, `lyceum --version`; where it is, in its tooltip). A world
made then runs that build where it is, so each start runs the newest build; it is never updated from a release and
runs only while the launcher is open. A packaged release never looks for a checkout.
**`--downloads`** (`cargo run -- --downloads`) makes a development build behave as a release
here: it passes over its checkout's builds and downloads both programs, so the download flow can
be tried from a checkout.

**Your world servers.** **New world** asks for a name, the game's data files (one of the Library's
Modern sets, its default to begin with), a port (one no other world uses), **Automatically create
accounts** and **Allow other players**, both on to begin with; each check box's tooltip says what
it does. Each world's row says, in one line, its state, its Empyrean, who is online and how long it
has been up (`Running · Empyrean v0.3.3 · 2 online · up 1 h 4 min`), and under it only what it is
doing now or why it stopped. A world's page shows the same line under its name, then four sections
that fold:

- **Connect**: its **Local Address** (this computer's on its network) and **Public Address** (the
  router's on the internet, as the router or a check from outside says it), each with **Copy**; the
  public one says **Not available** when it cannot be found or other players are not allowed. Then
  whether friends elsewhere can reach it, as far as the router says (below), **Check from outside**,
  and **Copy link**, which copies the world's join link (above, "Join links").
- **Console**: its log, which a search box filters, and **Save log**. The log stays at its newest
  line as lines come, and stays where it is while the person reads further up or selects in it.
  Under it, while the world runs, the command line: a kind, its fields, and **Send**. The kinds
  are `empyrean>` (a command as typed at the world's console), **Add account...** (an account
  name and a password, sent as `accountcreate`, the password hidden in the log), **Message
  everyone...** (`gamecast`), **Save** (`save-world`: every player and the world saved now) and
  **Shut down in 5 minutes** (`set-shutdown-interval 300`, then `shutdown`), which **Cancel
  shutdown** stops.
- **Lyceum**: **World base**, the ACE world release its world database was made from (`ACE End of
  Retail v0.9.295`, `ACE Infiltration (2005) v0.8.8`: the release `empyrean-import fetch` named when
  Get world database fetched it, kept beside the world database as `<world database>.base`, or the
  one a published Lyceum world was built over), and **Running**, `Unmodified.` or the Lyceum world
  it plays and its version (`Lyceum world holtburg-hills version 3`). A world that plays its world
  base unmodified has **Publish Lyceum World** with a list of Lyceum's worlds (below, "Your Lyceum
  worlds") and **Other...**, which asks for a `world.toml`; one that plays a Lyceum world has
  **Publish Latest** with the version publishing makes, which is off while the saved world's sources
  are the ones published. **Restore World Base** puts the world back on its era's world database,
  and is off while it plays that already. The world must be stopped for any of them.
- **Advanced**: one line each, the button first and then what it says of the world. **Check for
  Updates**, the Empyrean the world runs, and what the last check found: a green dot and **Latest**,
  or an amber dot, the newer version and **Update to** it. **Open world folder**. **Run via System
  Service** (or **Disable System Service**), so the world keeps running when the launcher is closed
  (its tooltip says what setting it up takes), with a green dot and **Active** or a gray one and
  **Inactive**. **List on the Dereth Network** (the network's page for registering a world), with
  **Listed as** the world's slug, a link to its page on the network's website, once the slug its
  `[federation] slug` names is on the hub's list, else **Unlisted**. **Delete World**, which asks to
  be confirmed in place.

**Start** and **Stop** run it (Stop asks Empyrean to save first), **Play** plays on it, and
**Settings** opens a screen of its own for what matters in its `empyrean.toml`, each part a section
that folds:

- **World**: its name, its era, its game data, its **World Base**, the character screen's
  welcome, which may run to several lines (`\n` in the file), and the port, beside which the
  screen names the one after it, which the world takes too. The game data is one of the Library's
  sets of the kind the era is drawn from, by the client's own rule: an era before Throne of Destiny
  (Infiltration) plays the Classic data files, `portal.dat` and `cell.dat`; any other the Modern
  ones. The list names each set; the screen says nothing more unless there is none to choose, when
  it says which files to add (`Infiltration needs the 2005 game files. Add them in the Library.`).
  Saving refuses data files of the other kind. Each era plays its own world base: the end of
  retail's is `world.pack` (ACE's world), Infiltration's `world.pack.infiltration` (ACE's
  Infiltration world). The World Base names the release (as on the world's page), or **Not
  downloaded yet** with **Get world database**; after the era changes, it says `Saving will switch
  world base to ACE Infiltration (2005)`, and Get world database fetches that one once saved. A
  world whose configuration names a world database of its own keeps it.
- **Players**: **Allow other players**, **Automatically create accounts**, the most players at
  once and from each IP address, and the shutdown warning.
- **Era systems**: each of the era's systems as a check box, showing the era's own value until it
  is changed; one changed is written under `[era]`, and one given back to the era's value loses
  its key. **Use defaults for** the era gives every one back.
- **Websocket**: `[server.websocket]`. **Enable Websocket Connections** turns the endpoint on or
  off. **Behind a TLS proxy**, or, unchecked, the certificate and key; the address it listens on,
  the pages players may play from (**Allowed pages**, one a line: `https://play.dereth.network` to
  begin with, for a world that names none, written when the person saves, as anything else is) and
  the public address. Each says what it is for in its tooltip.
- **Dereth Network**: `[federation]` on or off, and the world's slug, its name on the network's
  website. Its key is pasted from the network's registration: **Save** keeps it in `network.key`
  in the world's folder (only its owner may read it on macOS and Linux; on Windows the folder is
  in the user's own profile) and names that file in `key_file`. From then on the screen says only
  **Key saved**, with **Replace**. A paste that is not a key is refused, the key is never logged,
  and the world links only once its key is kept. **Link this world** opens the network's page for
  operators, beside whether the world is listed.
- **Lyceum world**: what it plays, as what it is ("Plays the Lyceum world X version 3, published
  <date>") and the content overlay, with patching and its rates folded under it. A world that plays a
  Lyceum world keeps the era and game data it was published for, until it stops playing it.
- **Advanced**: the log level, as the file writes it (`error` to `trace`).

The launcher counts a world's players at its status endpoint on this computer
(`server.status_address`, on `127.0.0.1`). It chooses the port when it makes the world, the first
from 9100 that no other world names and nothing holds, and keeps it; the screen never shows it.

A key the file does not have is written only when it is not the server's default. Every other key
and comment in the file is kept, and **Open empyrean.toml** opens it for the rest. The world reads
its settings when it starts, which the screen says while it runs. Before its first start a world
needs its world database, which **Get world database** downloads once. Closing the launcher asks to
stop the worlds it started.

**Whether friends elsewhere can reach it.** The launcher asks the router over UPnP, on the local
network only, for its outside address and for which computer it passes each game port to; it changes
nothing on the router. The page says plainly what that means: friends elsewhere need the router to
pass the game port and the one after it (UDP) to this computer, which the router does or does not
list, or the launcher cannot tell; and an internet provider that shares one address among many
homes (a private or `100.64.0.0/10` outside address) keeps friends elsewhere out. Only something
outside can settle it, so **Check from outside** asks the Dereth Network's hub to try this computer
at the world's port (`POST /v1/probe`, the hub's list's hub when `DERETH_HUB_LIST` names another): the
hub tries the address the request comes from, never one the launcher names, and says in one line
whether it reached the world, and what the world said of itself. The hub allows a few checks an hour
from one address.

**Other worlds on this computer.** Host a world also lists the Empyrean, ACE and GDLE servers
already running here that the launcher did not make. It looks every few seconds while the page is
open: at each process whose program is a server's (`empyrean-server`, `ACE.Server` or `dotnet
ACE.Server.dll`, `GDLEnhanced`), its command line, the UDP ports it holds and what started it (a
Windows service, a systemd service, launchd, or a person). It asks each one how it is, at most
twice a minute: Empyrean with the status ping (its name, version and players), ACE and GDLE with
the server-tracker login (up or not). Each has a page: who runs it, Connect, its log, and under
Advanced its folders, opened in the system's file browser. Each is laid out as its own sources lay
it out:

| server | configuration | data | log |
|---|---|---|---|
| Empyrean | `--config <file>`, else `empyrean.toml` where it was started, else beside `empyrean-server` | its databases, beside the configuration unless it says otherwise | none of its own (it writes to standard error): the newest `.log` beside the configuration or the program, or in a `logs` folder there; or its admin channel (below) |
| ACE | `Config.js` where it was started, else beside the program | MySQL, so no folder; its game data is `DatFilesDirectory` | the file `log4net.config` names, else `ACE_Log.txt`, beside the program |
| GDLE | `-c` or `--config <file>`, else `server.cfg`, else `debug_server.cfg`, where it was started | `Data` where it was started | the file `GDLELogging.conf` names, else `server.log` |

**An Empyrean's admin channel.** An Empyrean world writes `admin-channel.json` in its folder at each
start: the loopback address of its admin channel, a new secret, and its process. The launcher reads
it for each Empyrean it finds running (a file another process left is passed over), connects, and
gives the secret: from then on the world's log comes over the channel (its recent lines, then each
new one, as its console shows them), its command line works as for the launcher's own worlds, and
**Stop** asks it to save and stop there (its console's `exit`). A world a service manager may start
again when it stops says so on its page.

**A world a service manager runs.** The launcher never stops a process it did not start any other
way. For a Windows service, **Let the launcher start and stop this service** asks once for an
administrator's permission, with Windows' own prompt, and adds the person's own account to the
service's permissions with start, stop and status only, as the launcher gives the person for a world
it sets to keep running; from then on **Start** and **Stop** use the service, which has the world
save and stop, admin channel or not, so a service that starts its world again when it exits cannot
undo a Stop. A systemd service of the person's own (`systemctl --user`) is started and stopped
directly. For a system service, the page shows the command to run.

An Empyrean the launcher has seen running from a folder, not as a service, is remembered; once it
stops, **Start** runs it again from that folder as the launcher's own, and then **Stop** asks it to
save and stop (its console's `exit`), **Stop now** ends it at once when asked a second time, and its
console takes commands as the launcher's own worlds' do. One whose console is off
(`interactive_console = false`) is not started, since it could not be stopped cleanly. **Forget this
world** forgets a remembered one.

**Your Lyceum worlds.** Lyceum's page lists the worlds in two kinds: **Your Worlds**, those in its
folder for worlds (the Lyceum folder in your Documents, which **Open Folder** shows), and **Opened
from elsewhere**, the others Lyceum or the launcher opened (Lyceum's own recent worlds, and the
launcher's list of the worlds it opened, so a world opened with **Open World** is listed at once).
Each world is listed once, however its path is spelled, with its folder under its name and
**Open**. A world whose files are in an older format than this Lyceum's (as `lyceum upgrade
<world.toml> --check` says, asked again whenever its files or Lyceum change) also has **Upgrade**,
which brings them up to this Lyceum's format, keeping a backup.

A row opens in place, one at a time, as a world's row on Worlds does: its era, when it was last
saved (its newest source file), and what it changes, counted, as `lyceum facts <world.toml>`
prints it (landblocks with terrain edits, statics, buildings, dungeons, rooms, spawns, weenies,
new textures and models; asked again whenever its sources or Lyceum change); then **Export For**
with Empyrean, ACE or GDLE, which writes the world for that server with `lyceum export` into a
folder you choose (that folder when it is empty or an earlier export, else `<world>-<server>` in
it), over the game data its era is built on; and **Publish To** with the worlds you host, which
publishes it to the one chosen, as its page's Lyceum section does.

**New World** asks for a name, a **Folder** (Lyceum's folder in your Documents unless you choose
another) and an **Era** (End of Retail or Infiltration), and **Start Building** makes an empty world
with the `lyceum new --empty` command beside the editor and opens the editor on it; the sample is
the `lyceum` command's (`lyceum new <folder>`). **Open World** opens any world, by its folder. The
editor opens straight on the world, drawn over the game's data files Lyceum used last, else the
Library's default. A Lyceum too old to make worlds this way opens its own start screen instead,
which can.

Publishing builds the saved Lyceum world with the `lyceum` command beside the editor, into the
world you host, which must be stopped, and that world plays it from then on. Each publish is
numbered, the version the world's page shows: the first is 1, and each after it the next when what
it builds differs from the last (`publication.json` keeps the number, and a digest of the world's
sources it was built from, which tells whether the saved world has changed since). Publishing again
makes a new snapshot, the next revision of the world's overlay, and keeps the three newest (and
always the one played), each holding its own world database and a link to the world database dump
it was built over. Close Lyceum first. A world with a content overlay (`world_overlay_path`) is
neither published nor put back on the standard world until the overlay is removed or built into its
world database. Playing a published world needs the Dereth client, which the world patches when it
joins: a player who has the last revision is sent what changed. The launcher's own client keeps its
patches in `client-overlays/` in the world's folder.

For scripts, with Lyceum closed: `dereth --publish-host <world-folder> <world.toml>
<lyceum-program>` publishes without opening a window and prints the result as JSON.

The browser demo opens these with `#server` and `#make` (nothing downloaded yet),
`#server=ready` and `#make=ready`, `#server=new` and `#make=new`, `#server=running` (a world's page,
checked from outside), `#server=settings` (its Settings), `#server=settings-open` (every section
open), `#server=settings-infiltration` (changing to Infiltration), `#server=settings-published` (a
world that plays a Lyceum world), `#server=console` (adding an account),
`#server=advanced`, `#server=delete` and `#server=update`, `#server=lyceum` (a world's Lyceum
section) and `#server=published` (one that plays a Lyceum world saved since), and `#server=others`
(the other worlds on this computer) with `#server=service`, `#server=folder` and `#server=channel`
(one of their pages, the last an Empyrean service whose admin channel is connected), `#make=open`
(a Lyceum world's row open), `#make=notes` (What's new, with its links), and `#join` (a join link
arriving); `?dev=1` shows a development build.

### Where they keep things

| what | where |
|---|---|
| Empyrean and Lyceum as downloaded | `components/empyrean/<version>` and `components/lyceum/<version>` in the launcher's data folder (above: `%LOCALAPPDATA%\Dereth\launcher` on Windows, `~/Library/Application Support/Dereth/launcher` on macOS, `$XDG_DATA_HOME/dereth/launcher` on Linux); `current.json` beside them names the version in use |
| each world you host | `hosts/<id>` in the launcher's data folder: `empyrean.toml`; the databases `auth.db` (accounts) and `shard.db` (characters); `world.pack` (the world database), with `world.pack.base` beside it naming the ACE release it was made from; `server.log`, with the one before as `server.log.old`; `console`, the commands waiting for the world; `runtime/<version>`, its own copy of Empyrean, with `.empyrean-update` inside for update downloads and recovery; `published/snapshot-<n>`, the Lyceum worlds published to it, the three newest; `client-overlays`, what the world patched the launcher's own Dereth client with; **Open world folder** on its page opens it |
| its backups | beside its databases, `auth.db.backup-update-v<old>-v<new>-<time>` and the same for `shard.db`, made before each update; the newest three are kept |
| ACE's world release, as downloaded | Empyrean's cache: `%LOCALAPPDATA%\Empyrean\world-database`, `~/Library/Caches/Empyrean/world-database`, `$XDG_CACHE_HOME/empyrean/world-database` |
| a world running in the background | Windows: the service `dereth-<id>`; Linux: `~/.config/systemd/user/dereth-<id>.service`; macOS: `~/Library/LaunchAgents/dereth-<id>.plist` |
| the other Empyrean worlds the launcher remembers | `local-worlds.json` in the launcher's data folder: each one's program and configuration, and the service it ran as, if any |
| the output of one of those the launcher started | `local-logs` in the launcher's data folder, one file each |
| a join link a second launcher left for the open one, and the mark that one is open | `join-request.txt` and `launcher-running` in the launcher's data folder |
| Lyceum's worlds | `Lyceum` in your Documents unless you chose another folder: on Windows the Documents folder wherever it is (OneDrive's among them), `~/Documents` on macOS, the XDG documents folder on Linux (`~/Documents`). A Lyceum world is a folder: `world.toml`, its packs (each a folder beside it), `world.lock` and what it builds |
| the Lyceum worlds opened last | `recent-worlds.toml` in Lyceum's settings: `%APPDATA%\Dereth\lyceum`, `~/Library/Application Support/Dereth/lyceum`, `$XDG_CONFIG_HOME/dereth/lyceum`; and the ones the launcher opened, `lyceum-worlds.json` in the launcher's data folder |

Removing Empyrean or Lyceum removes only the download: your worlds are kept. **Delete World**
deletes the world's folder, its databases included.
