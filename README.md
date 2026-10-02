# dereth

**Dereth, rebuilt. A modern client, server and toolkit for Asheron's Call.**

**Dereth** is an open-source, from-scratch reimplementation of the Asheron's Call ecosystem. It includes
**Dereth**, a modern client built to behave exactly like the final retail client (acclient
00.00.11.6096), on the desktop and in a web browser; **Empyrean**, a Rust server descended from
ACEmulator, which web clients reach directly over WebSocket; and a shared SDK that anyone can build new
clients, bots and tools on. Retail behaviour is written down, tested and tied to evidence as it is
implemented, and every deliberate departure is listed ([client](dereth/DIVERGENCES.md),
[server](empyrean/DIVERGENCES.md)).

Everything builds on Windows, Linux and macOS; the client is tested on Windows and macOS.

**No retail data files ship with this repository.** The client and the server read the original
game's data files; obtain your own copy to run them.

## Download

The repository's [releases](https://github.com/dereth-network/dereth/releases) tagged
`dereth-v<version>` are Dereth, the launcher, with the client inside it: a zip for Windows (unzip
anywhere and run `dereth.exe`; no installer), `Dereth.app` for macOS and an AppImage for Linux.
Take the newest. The launcher keeps itself and the client up to date from the newest published
one. The builds are not code-signed yet: Windows SmartScreen asks once (**More info**, **Run
anyway**), and macOS once (**System Settings**, **Privacy & Security**, **Open Anyway**). How a
release is made:
[`dereth/launcher/RELEASING.md`](dereth/launcher/RELEASING.md). Empyrean's releases, tagged
`empyrean-v<version>`, are in the same list.

## Build and run

Rust 1.98 or newer. The default renderer is Vulkan, so one source tree serves all three platforms;
at run time a Vulkan 1.2 driver is required.

```
cargo build --workspace
cargo run   -p dereth-client
```

The server's own guide is [`empyrean/SETUP.md`](empyrean/SETUP.md). The web client builds and serves
locally with `cargo xtask web`; deploying it is [`dereth/web/DEPLOY.md`](dereth/web/DEPLOY.md), and
playing it on a server that speaks only UDP goes through
[`dereth-web-relay`](tools/web-relay/README.md).

## Why this exists

There are already good Asheron's Call servers, and people keep making more. Most players are still using ancient retail clients unsuitable for modern Windows or non-Windows. This isn't a replacement for any of them, and it isn't a bid to be the one true anything. I'm building Dereth and
Empyrean for me: to understand the game properly, top to bottom, and to have a modern client and server I can take wherever I want. If it's useful to you, use it. If it isn't, don't. Both are fine.

Where it's going:

- **Every era.** One client and one server that can play Asheron's Call as it was at any point from
  launch in 1999 to the end of retail in 2017, chosen by the data you load, not by which program you
  run.
- **The old UI,** (optionally) and the classic Asheron's Call experience more generally.
- **New ways to present it:** experiments with upgraded graphics, a stylized 2D client, Dereth on the web.
- **A plugin and automation API,** including bots. Picture a world populated by AI-driven characters.
- Whatever else turns out to be cool along the way.

## Layout

One workspace (`Cargo.toml`, one `Cargo.lock`), four folders. A crate's folder is its package name
without the `dereth-` or `empyrean-` prefix: `core/physics` is `dereth-physics`,
`dereth/client/crates/ui` is `dereth-ui`, `empyrean/crates/world` is `empyrean-world`.

| Folder | What it holds | Licence |
|---|---|---|
| `core/` | The shared crates: the data files, the engines, the wire and the game rules any AC client, server or tool shares, and the client SDK (`client-net`, `client-model`, `client-contract`, `client-runtime`, `audio`), with `dereth-client-sdk` its front door. | MIT |
| `dereth/` | The products: the 3D client (`dereth/client`, with its own crates under `dereth/client/crates/`, among them `scene`, the drawn world, and `shell`, the retail front end both clients share), the web client (`dereth/web`), the launcher (`dereth/launcher`: its Tauri app, a workspace of its own, and `dereth-launch`, what it knows), the headless client and the client's test kit, with [`DIVERGENCES.md`](dereth/DIVERGENCES.md), the places the Dereth client deliberately differs from the retail client. | MIT |
| `empyrean/` | Empyrean, the server: the ACE port under `empyrean/crates/`, the `empyrean-server` and `empyrean-import` binaries and the server's test kit, with its own [`LICENSE`](empyrean/LICENSE), [`README.md`](empyrean/README.md), [`SETUP.md`](empyrean/SETUP.md) and [`DIVERGENCES.md`](empyrean/DIVERGENCES.md). | AGPL-3.0-only |
| `tools/` | Research and build tools: the capture index, the corpus tool, the web client's local relay (`web-relay`) and developer runner (`web-dev`), and the `xtask` task runner; [`tools/README.md`](tools/README.md) says what each is for and how to run it. | MIT |

## Commands

```
cargo xtask check            # while working, before each commit: the crates you changed, and the fast rules
cargo xtask ci tier0         # before pushing: exactly what CI runs; needs no game data
cargo xtask ci tier1         # with your retail data: the data tiers and the server's real-content tier
cargo xtask ci tier2         # on hardware: the GPU tier and the retail comparisons
cargo xtask web              # build the web client and serve it on 127.0.0.1
cargo xtask                  # every single check and tool, grouped
```

Which to run when, and how to point the tests at your own data: [CONTRIBUTING.md](CONTRIBUTING.md).
Clippy, the lints and the tests cover the client and the server crates alike; `clippy.toml` is
shared, and all the code formats with rustfmt's defaults.

## Test tiers

A crate's integration tests are **one binary per tier**, not one binary per file. The tier says what
a test needs to run, and it is the only thing the tiers say:

| tier  | needs                                                | target              |
|-------|------------------------------------------------------|---------------------|
| `cpu` | nothing but the crate                                | `tests/cpu/main.rs` |
| `dat` | game data (`DERETH_TEST_DAT_DIR`)                    | `tests/dat/main.rs` |
| `gpu` | a graphics device and game data                      | `tests/gpu/main.rs` |
| `local` | explicitly supplied private inputs (e.g. unscrubbed recordings) | `tests/local/main.rs` |

The tests' and tools' own variables are `DERETH_*`, and those only tests read are `DERETH_TEST_*` (the server's, `EMPYREAN_TEST_*`). The inputs a run may be given:

| variable | what it points the tests at |
|---|---|
| `DERETH_TEST_DAT_DIR` | the game data (no default) |
| `EMPYREAN_TEST_WORLD_PACK` | the server's built world pack ([`empyrean/SETUP.md`](empyrean/SETUP.md)) |
| `DERETH_TEST_RENDERER` | `d3d12` or `wgpu` runs the GPU tiers on that backend instead of Vulkan |
| `DERETH_TEST_GPU` | `software` asks every test device for a software rasteriser; by default they use the hardware GPU, and fall back to software only where there is none |
| `EMPYREAN_WORLD_SQL` | ACE's world-database dump for the server's real-content tests, else the one `empyrean-import fetch` cached |
| `DERETH_TEST_FFMPEG` | the FFmpeg the intro-movie test decodes with, else `ffmpeg` on `PATH` |
| `DERETH_TEST_CAPTURES`, `DERETH_TEST_NETBLOBS`, `DERETH_TEST_CORPUS_ROOT` | recordings or a message corpus other than the ones in `fixtures/` |
| `DERETH_TEST_GOLDEN=1` | rewrite the golden files instead of comparing against them |

## Building on Windows, Linux and macOS

| | Windows | Linux | macOS |
|---|---|---|---|
| Vulkan at run time | any GPU driver from ~2019 on (`vulkan-1.dll` ships with the driver) | the GPU's Mesa or vendor driver; `lavapipe` (`mesa-vulkan-drivers`) works headless | the Vulkan SDK from LunarG, or `brew install molten-vk vulkan-loader` |
| native build inputs | MSVC, `rc.exe` (for the icon) | `libasound2-dev` (cpal), `pkg-config`; X11 and Wayland come through winit's pure-Rust bindings | Xcode command-line tools (cpal's CoreAudio bindings run bindgen against the SDK) |
| shaders | compiled from `legacy.wgsl` by `naga` at device creation; no toolchain | same | same |

On macOS `dereth_render::vulkan` finds the loader itself (a MoltenVK bundled in the app,
`Contents/Frameworks/libMoltenVK.dylib`, as a release carries it, then `$VULKAN_SDK/lib`,
`/opt/homebrew/lib` and `/usr/local/lib`), so nothing needs `DYLD_LIBRARY_PATH`. Device tests use the
machine's GPU, and a software rasteriser (`lavapipe`, SwiftShader, WARP) only where there is no GPU or
`DERETH_TEST_GPU=software` asks for one. Cross-checking from Windows (`cargo check -p dereth-render --target
x86_64-unknown-linux-gnu`, or `aarch64-apple-darwin`) needs only the rustup target.

**Three backends are built in by default:** Vulkan (the default), Direct3D 12 (Windows only) and
`wgpu` (Metal, Vulkan or Direct3D 12 underneath; `WGPU_BACKEND` picks which, and it is the backend the
web client uses in the browser).

```powershell
cargo run -p dereth-client -- --renderer d3d12
cargo run -p dereth-client -- --renderer wgpu
```

A run picks its backend from `--renderer vulkan|d3d12|wgpu`, then `Renderer=` in the preferences
file, then the default; a backend that is not built or cannot create a device prints a line and falls
back. The test-only `DERETH_TEST_RENDERER=d3d12|wgpu` runs the GPU test tiers on it.

## Where the client writes its settings

Everything the player saves (`UserPreferences.ini`, `dereth.keymap`, the UI layouts, screenshots
and journals) lives in one directory: a `UserPreferences.ini` in the working directory wins (a
portable install), and `-prefs <file>` (or `--prefs <file>`) moves all of them together. A
`--headless` run that names no file uses none of them: it reads the defaults and writes nothing
there, so a capture or a test run never changes the player's settings. Otherwise:

| platform | directory |
|---|---|
| Windows | `%APPDATA%\Dereth\client` |
| macOS | `~/Library/Application Support/Dereth/client` |
| Linux and other Unix | `$XDG_CONFIG_HOME/dereth/client`, or `~/.config/dereth/client` |

The launcher keeps its own settings beside it, in `launcher`. On Windows the first run **copies**
(never moves) the original game's `Documents\Asheron's Call` folder into it, once.
`Display.FullScreen` applies when the player enters the world; login and character select are
always windowed. Each run's crash log is `crash-logs/dereth-client-<pid>.log`
in the directory of the table above, whatever `-prefs` says.

## The specification

[`docs/`](docs/README.md) is the specification of what the client does: the numbers, the ordering,
the edge cases, the places where common reimplementations differ, and the test that pins each claim
where one exists. It is a **behaviour** specification, not a map of the original binary: every
statement is meant to be falsifiable, and each page says where its claims come from.

## License

Everything outside `empyrean/` is MIT; see [`LICENSE`](LICENSE). `empyrean/`, the server, is a port
of ACE (its earlier eras' rules of [ClassicACE](https://github.com/bDekaru/ClassicACE), bDekaru's
fork of ACE) and is therefore **AGPL-3.0-only**; see [`empyrean/LICENSE`](empyrean/LICENSE).

**Dereth is an independent project, not affiliated with or endorsed by Microsoft, Turbine or any current rights holder.**

**AI was used, and will be used, to assist with development on this project family.**
