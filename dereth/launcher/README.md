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
| right face button (B, Circle) | steps back: closes an open row or form, leaves a world's page, or goes back to Home |
| shoulder buttons | Home, Worlds, Library and Accounts, in turn |
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
| the world list | the Dereth Network hub's list (`https://hub.dereth.network/v1/servers`, a hundred worlds a page), which imports the community's list, `Servers.xml` in [acresources/serverslist](https://github.com/acresources/serverslist), daily and adds the worlds registered at the hub (a world with no address yet is not shown, and a demonstration world is shown with the status `Test`); the community's own `Servers.xml` when the hub does not answer with its list; or the `Servers.xml` that `DERETH_SERVERS_LIST` names, read alone. A world is kept under its name, whichever list it came from. Fetched at most once a day (the refresh button fetches it again) and kept in the data folder as `world-list.json`; and the servers the player added, which are kept in the state. While the list shown is the hub's, the launcher also reads it again about once a minute for each world's status (below), kept in memory only, never from the day's copy |
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
