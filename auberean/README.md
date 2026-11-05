# Auberean

Auberean is the Dereth Network's Asheron's Call 2 client: a rebuild of the final retail client,
ac2client 00.00.00.0930 (30 September 2013). Its server is [Yalain](../yalain/README.md) and its world
builder [Athenaeum](../athenaeum/README.md); all three share the Asheron's Call 2 core in
`core/auberean-*`, and that core shares the engine's common crates with Dereth (the data-file
container, the transport, the connection manager, the landscape, the swept-sphere transition).

It is early work, released for the first time as 0.1.0. The desktop client opens a window with the
game's own screens: make a character, enter the world, walk, fight and talk on Yalain. The world
viewer draws the world offline to a picture, and the headless client logs in, enters the world and
plays a script of actions.

## Playing it

You need your own Asheron's Call 2 install: the end-of-retail data files `portal.dat`,
`cell_1.dat` and `local_English.dat` (and `highres.dat`, when the install has it). Nothing of the
game ships with Auberean, and nothing of it is downloaded.

**From the launcher.** Auberean's release is the Dereth Network launcher with the Auberean client
beside it: `auberean-<version>-windows-x86_64.zip` (unzip it anywhere and run `dereth.exe`),
`Auberean-<version>-macos-<arch>.app.tar.gz` (unpack it and move `Dereth.app` to Applications) or
`Auberean-<version>-linux-x86_64.AppImage` (make it executable and run it). It opens in
Auberean's realm. On Play, choose your Asheron's Call 2 folder (the launcher checks its data files)
and press Play: it starts a world on your computer with Yalain (downloaded on Yalain's page) and
logs you in with the account name you give. A world on the Dereth Network opens from Worlds, with
its account and password. The launcher keeps itself and the client up to date. Dereth's release
carries the same launcher: in it, pick Auberean's portal above the rail and download Auberean on
Play.

**In a browser.** [The web page](../dereth/web/README.md) offers Auberean's realm beside Dereth's.
Add your data files in its Library, and play any world with a WebSocket endpoint
([Yalain's guide](../yalain/SETUP.md#41-websocket-connections) says how to turn one on).

**By hand.** The client is `auberean` beside the launcher in the release (in `Dereth.app`, in
`Contents/MacOS`; in the AppImage, `Auberean-<version>-linux-x86_64.AppImage --appimage-extract`
unpacks it to `squashfs-root/usr/bin`). Start a world with Yalain, then:

```text
auberean --dats <install folder> --server 127.0.0.1:9000 --account <name>:<password>
```

`--server` is the world's login address and port. The client keeps its settings in the folder
`AUBEREAN_SETTINGS_DIR` names, else in your application data's `Auberean`, and its screenshots
(Ctrl+P) there in `screenshots`. `auberean --version` says which build it is. The original client's
own options work too (`-h` and `-p` for the server, `-a` for the account, `-z` for the ticket,
`--prefs` for the preferences file); `auberean -?` lists them all.

| Folder | Package | What it is |
|---|---|---|
| [`client/`](client/) | `auberean-client` | the desktop client (the `auberean` program): the window, the device, the network, the settings folder |
| [`client/crates/shell/`](client/crates/shell/) | `auberean-shell` | the application over a host: input to the interface and the world's controls, the session, the settings |
| [`client/crates/scene/`](client/crates/scene/) | `auberean-scene` | the world a window shows: the player's body, the bodies around it, the chase camera, the frame |
| [`client/crates/draw/`](client/crates/draw/) | `auberean-draw` | what a frame of the world draws, with no device |
| [`client/crates/gpu/`](client/crates/gpu/) | `auberean-gpu` | the wgpu device that draws it |
| [`client/crates/present/`](client/crates/present/) | `auberean-present` | a frame composed and put on a surface, a window's or a canvas's |
| [`web/`](web/) | `auberean-web` | the client in a web browser: the module [the web page](../dereth/web/README.md) loads for a world of Asheron's Call 2's |
| [`viewer/`](viewer/) | `auberean-viewer` | the offline world viewer: the world at a spot and time of day, to a PNG |
| [`headless/`](headless/) | `auberean-headless` | the client with no window, on the SDK alone |
| [`testkit/`](testkit/) | `auberean-testkit` | the behaviour registry, where the tests find their data, and a server double |

The core, in [`core/`](../core/):

| Package | What it is |
|---|---|
| `auberean-primitives` | ids, vectors, rotations and frames, the string scramble |
| `auberean-dat` | the data files: the shared container in this game's layout, the type table, the record reader |
| `auberean-assets` | one decoder per record type, each held to exact consumption |
| `auberean-script` | the script machine: the compiled library, the interpreter, the packed object graph |
| `auberean-glue` | the natives scripts call |
| `auberean-protocol` | every message both ways, typed, exact over the recorded sessions |
| `auberean-world` | the terrain and its looks, the sky, light and fog, placed objects |
| `auberean-physics` | bodies that move: the shared transition with this world's rules and cells |
| `auberean-animation` | bodies that animate: skeletons, clips, the blend graph, skinning |
| `auberean-port` | the marker for a retail function not ported yet |
| `auberean-client-net`, `-model`, `-contract`, `-runtime`, `-sdk` | the client without a window: the session, the model, the actions, the runtime and the SDK over them |

## Running it from the repository

You need your own Asheron's Call 2 install; nothing of the game ships here.

```text
cargo run --profile test-release -p auberean-viewer -- --install <install folder> --block 6FA9 --at 45.2,90.5,85.3 --heading 178 --out view.png
cargo run --profile test-release -p yalain -- --dats <install folder>
cargo run --profile test-release -p auberean-headless -- --dats <install folder> --account me --ticket t
cargo run --profile test-release -p auberean-client -- --dats <install folder> --dev-login me:t --dev-character Name
```

The desktop client keeps its settings in the folder `AUBEREAN_SETTINGS_DIR` names, else in your
application data's `Auberean`. It moves with the game's default keys: W, S and X, A and D, Z and C,
Q to run on, Space to jump, the keypad for the camera, the middle button for mouse look.

## Testing

`cargo xtask check` and `cargo xtask ci tier0` run Auberean's tests that need no game data with the
rest of the workspace. `cargo xtask ci tier1` runs them, then its data tiers, after the first game's
(`--game auberean` runs Asheron's Call 2's alone; `cargo xtask ci auberean` is the same). The data tiers
read the data files and a capture index of recorded sessions through two variables:

| Variable | What |
|---|---|
| `AUBEREAN_TEST_DAT_DIR` | the end-of-retail data files, as retail left them in January 2017 (which the September 2013 client reads) |
| `AUBEREAN_TEST_CAPTURE_INDEX` | the capture index |

| Tier | Command | Reads |
|---|---|---|
| no data | `cargo test --profile test-release -p <crate>` | nothing |
| `auberean-dat` | `cargo xtask gate auberean-dat` | the installs: every record of every file decodes, the world builds |
| `auberean-script` | `cargo xtask gate auberean-script` | the installs: the script library and saved states |
| `auberean-corpus` | `cargo xtask gate auberean-corpus` | the installs and the capture index: every recorded message and script call, the recorded logins replayed |

A test whose data is not found fails, naming the variable to set.

## The rules the code keeps

- **A portable core.** The core crates do no I/O, read no clock and spawn no thread outside the files a
  feature gates for native hosts and tests, and build for `wasm32-unknown-unknown`.
- **One place finds the data**: `auberean_dat::locate` and, for tests, `auberean_dat::testing`.
- **Tests are claims.** Each test carries `/// Behaviour: <id>` naming a row of the registry in
  `auberean-testkit`, or its file says `//! Behaviour: none (why)`. A row is stamped `RETAIL` (what the
  September 2013 client does), `THIS_CLIENT` (a deliberate divergence) or `TOOLING`, and names its
  evidence by an opaque handle, `AC2-EVID-...`. Tests never depend on timing.
- **Port anchors.** A Rust item that ports a function of the retail client carries a comment line
  that is only that function's opaque id, `// D2-04211`; a function not ported yet is
  `not_ported!("D2-04211")`.
- **Behavior only.** Names are this code's own; no comment, string or test names the retail program's
  internals or its addresses.

`cargo xtask seams`, `test-lint` and `comment-lint` hold the code to these.
