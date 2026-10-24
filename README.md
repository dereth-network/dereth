# Dereth Network

**Asheron's Call, rebuilt: a modern client, server and world builder.**

The Dereth Network is an open-source reimplementation of Asheron's Call:

- **Dereth**, a client that behaves like the final retail client, acclient 00.00.11.6096 (June
  2015), on the desktop and in a web browser. It comes with a launcher that finds worlds and keeps
  everything up to date.
- **Empyrean**, a server ported from [ACE](https://github.com/ACEmulator/ACE). Web clients reach it
  directly over WebSocket.
- **Lyceum**, a world builder that draws a world the way the client does and exports it for
  Empyrean, ACE or GDLE.
- **The Dereth Network** itself: cross-world tells, friends and chat between Empyrean and ACE
  worlds.
- A shared SDK for building other clients, bots and tools.

Retail behavior is written down, tested and tied to evidence, and every deliberate difference is
listed ([client](dereth/DIVERGENCES.md), [server](empyrean/DIVERGENCES.md)).

Everything builds on Windows, Linux and macOS. The client is tested on Windows and macOS.

**No game data ships with this repository.** The client and the server read the original game's
data files; you need your own copy.

## Download

Get the newest release tagged `dereth-v<version>` from
[releases](https://github.com/dereth-network/dereth/releases). It is the launcher with the client
inside:

- **Windows:** a zip. Unzip it anywhere and run `dereth.exe`; there is no installer.
- **macOS:** `Dereth.app`.
- **Linux:** an AppImage.

The launcher updates itself and the client, and downloads Empyrean and Lyceum when you want them.
Their own releases are in the same list, tagged `empyrean-v<version>` and `lyceum-v<version>`.

The builds aren't code-signed yet. Windows SmartScreen asks once (**More info**, then **Run
anyway**), and so does macOS (**System Settings**, **Privacy & Security**, **Open Anyway**).

## Why this exists

There are already good Asheron's Call servers, and people keep making more. Most players are still
using ancient retail clients unsuitable for modern Windows or non-Windows. This isn't a replacement
for any of them, and it isn't a bid to be the one true anything. I'm building Dereth and Empyrean
for me: to understand the game properly, top to bottom, and to have a modern client and server I
can take wherever I want. If it's useful to you, use it. If it isn't, don't. Both are fine.

Where it's going:

- **Every era.** One client and one server that can play Asheron's Call as it was at any point from
  launch in 1999 to the end of retail in 2017, chosen by the data you load, not by which program you
  run.
- **The old UI**, optionally, and the classic Asheron's Call experience more generally.
- **New ways to present it:** upgraded graphics, a stylized 2D client, Dereth on the web.
- **A plugin and automation API,** including bots. Picture a world populated by AI-driven characters.
- Whatever else turns out to be cool along the way.

## Guides

| Product | Guide |
|---|---|
| The launcher | [`dereth/launcher/README.md`](dereth/launcher/README.md) |
| The web client | [`dereth/web/README.md`](dereth/web/README.md); hosting it: [`dereth/web/DEPLOY.md`](dereth/web/DEPLOY.md) |
| The Horizon interface | [`dereth/client/crates/horizon/README.md`](dereth/client/crates/horizon/README.md) |
| Empyrean | [`empyrean/SETUP.md`](empyrean/SETUP.md) to run a server; [`empyrean/README.md`](empyrean/README.md) for reference |
| Lyceum | [`lyceum/README.md`](lyceum/README.md) |
| The Dereth Network | [`network/README.md`](network/README.md); for ACE worlds, [`network/ace-mod/README.md`](network/ace-mod/README.md) |

## Build and run

You need Rust 1.98 or newer. The default renderer is Vulkan, so a Vulkan 1.2 driver is needed at
run time.

```
cargo build --workspace
cargo run -p dereth-client
```

`cargo xtask web` builds the web client and serves it on `127.0.0.1`. To play it on a server that
speaks only UDP, run [`dereth-web-relay`](tools/web-relay/README.md).

## Layout

One Cargo workspace. A crate's folder is usually its package name without the `dereth-`,
`empyrean-` or `lyceum-` prefix: `core/physics` is `dereth-physics`, `empyrean/crates/world` is
`empyrean-world`.

| Folder | What it holds | License |
|---|---|---|
| `core/` | The shared crates: the data files, the engines, the wire protocol, the game rules and the client SDK (`dereth-client-sdk` is its front door). | MIT |
| `dereth/` | The client (`dereth/client`, with its crates under `dereth/client/crates/`), the web client (`dereth/web`), the launcher (`dereth/launcher`, a Tauri app with a workspace of its own), the headless client and the client's test kit. | MIT |
| `empyrean/` | The server: the ACE port under `empyrean/crates/`, the `empyrean-server` and `empyrean-import` programs and the server's test kit. | AGPL-3.0-only |
| `lyceum/` | The world builder: the editor, the Foundry library and the `lyceum` command. | MIT |
| `network/` | The Dereth Network: its protocol, hub, conformance suite and ACE mod. | MIT; the hub and the mod AGPL-3.0-only |
| `tools/` | Developer tools: the `xtask` task runner, the capture and corpus tools, and the web client's relay and dev server. [`tools/README.md`](tools/README.md) describes each. | MIT |
| `docs/` | The behavior specification (below). | |
| `fixtures/` | Recorded sessions and other shared test data. | |

## Commands

```
cargo xtask check            # while working, before each commit: the crates you changed and the fast rules
cargo xtask ci tier0         # before pushing: exactly what CI runs; needs no game data
cargo xtask ci tier1         # with your game data: the data tests and the server's real-content tests
cargo xtask ci tier2         # on hardware: the GPU tests and the retail comparisons
cargo xtask web              # build the web client and serve it on 127.0.0.1
cargo xtask                  # list every check and tool
```

[CONTRIBUTING.md](CONTRIBUTING.md) says which to run when and how to point the tests at your data.
Clippy, the lints and the tests cover the client and the server alike. `clippy.toml` is shared, and
the code is formatted with rustfmt's defaults.

## Test tiers

Each crate's integration tests build into one binary per tier. The tier says what the tests need:

| Tier | Needs | Target |
|---|---|---|
| `cpu` | nothing but the crate | `tests/cpu/main.rs` |
| `dat` | game data (`DERETH_TEST_DAT_DIR`) | `tests/dat/main.rs` |
| `gpu` | a graphics device and game data | `tests/gpu/main.rs` |
| `local` | private inputs you supply, such as unscrubbed recordings | `tests/local/main.rs` |

Variables only tests read start with `DERETH_TEST_` (`EMPYREAN_TEST_` for the server's).
CONTRIBUTING.md lists the ones that point at game data and world packs. The others:

| Variable | What it does |
|---|---|
| `DERETH_TEST_RENDERER` | `d3d12` or `wgpu` runs the GPU tests on that backend instead of Vulkan |
| `DERETH_TEST_GPU` | `software` asks for a software rasterizer. By default tests use the hardware GPU, and software only where there is none |
| `DERETH_TEST_DXC_DIR` | the folder holding DXC's `dxcompiler.dll`, for Direct3D 12 test devices; the GPU tier's DXC test needs it |
| `DERETH_TEST_FFMPEG` | the FFmpeg the intro-movie test decodes with, else `ffmpeg` on `PATH` |
| `DERETH_TEST_CAPTURES`, `DERETH_TEST_NETBLOBS`, `DERETH_TEST_CORPUS_ROOT` | recordings or a message corpus other than the ones in `fixtures/` |
| `DERETH_TEST_GOLDEN=1` | rewrite the golden files instead of comparing against them |

## Platforms

| | Windows | Linux | macOS |
|---|---|---|---|
| Vulkan at run time | any GPU driver from about 2019 on | the GPU's Mesa or vendor driver; `lavapipe` (`mesa-vulkan-drivers`) works headless | the Vulkan SDK from LunarG, or `brew install molten-vk vulkan-loader` |
| To build | MSVC, and `rc.exe` for the icon | `libasound2-dev` and `pkg-config` | Xcode command-line tools |

Shaders are compiled at run time, so there is no shader toolchain to install. On macOS the client
finds the Vulkan loader itself (the MoltenVK a release bundles, then `$VULKAN_SDK/lib`,
`/opt/homebrew/lib` and `/usr/local/lib`), so `DYLD_LIBRARY_PATH` isn't needed. To check the Linux
or macOS build from Windows, add the rustup target and run
`cargo check -p dereth-render --target x86_64-unknown-linux-gnu` (or `aarch64-apple-darwin`).

Three renderers are built in: Vulkan (the default), Direct3D 12 (Windows only) and `wgpu` (Metal,
Vulkan or Direct3D 12 underneath, chosen by `WGPU_BACKEND`; the web client uses it too). Pick one
with `--renderer vulkan|d3d12|wgpu` or `Renderer=` in the preferences file. One that isn't built
in or can't create a device falls back to the default and says so.

```
cargo run -p dereth-client -- --renderer d3d12
```

On Direct3D 12, `wgpu` compiles shaders with FXC, which comes with Windows. Put Microsoft's
`dxcompiler.dll`, 1.9 or newer (from `bin/x64` of a
[DXC release](https://github.com/microsoft/DirectXShaderCompiler/releases)), beside
`dereth-client.exe` and it uses DXC instead, which is much faster on Better Lighting's long shaders
and can trace rays. The startup log line says which compiler it used.

The client can also run without a window and replay recorded input:
[`dereth/client/INPUT-REPLAY.md`](dereth/client/INPUT-REPLAY.md).

## Where the client keeps its settings

Everything the player saves (`UserPreferences.ini`, key maps, UI layouts, screenshots and
journals) lives in one folder:

| Platform | Folder |
|---|---|
| Windows | `%APPDATA%\Dereth\client` |
| macOS | `~/Library/Application Support/Dereth/client` |
| Linux and other Unix | `$XDG_CONFIG_HOME/dereth/client`, or `~/.config/dereth/client` |

The launcher keeps its own settings in `launcher` beside it. On Windows the client's first run
copies (never moves) the original game's `Documents\Asheron's Call` folder in, once.

These win over the default, strongest first:

- `-prefs <file>` (or `--prefs`) moves everything to that file's folder.
- `DERETH_SETTINGS_DIR` names the folder outright. Scripts, tests and a second copy of the client
  use it to leave the player's own settings alone.
- A `UserPreferences.ini` in the working directory makes a portable install.

A `--headless` run that names no preferences file reads the defaults and writes nothing.

Full screen (`Display.FullScreen`) starts at the first screen in Horizon. The modern and classic
interfaces keep login and character select in a window and go full screen on entering the world.

A run that fails, panics or crashes leaves a log in `crash-logs/dereth-client-<pid>.log` in the
settings folder, with the account, password and ticket hidden. A clean exit removes its log, and
only the newest 20 are kept.

## The specification

[`docs/`](docs/README.md) specifies what the client does: the numbers, the ordering, the edge
cases, where other reimplementations differ, and the test that pins each claim. It describes
behavior, not the original program's code. Every statement is meant to be checkable, and each page
says where its claims come from.

## License

Everything is MIT except Empyrean (`empyrean/`) and the Dereth Network's hub and ACE mod, which
are AGPL-3.0-only. Empyrean is a port of ACE, with its earlier eras' rules from
[ClassicACE](https://github.com/bDekaru/ClassicACE), bDekaru's fork of ACE. See
[`LICENSE`](LICENSE) and [`empyrean/LICENSE`](empyrean/LICENSE).

**Dereth is an independent project, not affiliated with or endorsed by Microsoft, Turbine or any current rights holder.**

**AI was used, and will be used, to assist with development on this project family.**
