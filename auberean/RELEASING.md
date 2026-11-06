# Releasing Auberean, Yalain and Athenaeum

Asheron's Call 2's three products are released apart, each under a tag of its own, as GitHub
releases of this repository: Auberean, the client (`auberean-v<version>`); Yalain, the server
(`yalain-v<version>`); and Athenaeum, the world builder (`athenaeum-v<version>`), which carries
Yalain and Auberean beside its editor. Each is released the way Lyceum is
([`lyceum/RELEASING.md`](../lyceum/RELEASING.md)): two commands and one review.

**Auberean's release is the launcher's.** It is the same launcher Dereth's release carries, built
the same way from the Auberean release's own commit, with the Auberean client beside it instead of
Dereth's and a `launcher-defaults.json` that makes a first run open in Auberean's realm and the
launcher update from Auberean's releases. Whichever game's release a player downloads, they get
that game first and the other one click away: its client, Yalain and Athenaeum download when first
wanted. The launcher keeps its own version, Dereth's; how the two channels update, and why a
launcher never downgrades itself, is in
[`launcher/RELEASING.md`](../launcher/RELEASING.md) ("Two channels").

1. **`cargo xtask release auberean|yalain|athenaeum <version>`** on a clean `main`: sets the
   version, dates the release's highlights in the product's `CHANGES.md`, runs the checks, packages
   the host's target as a smoke test and tags `<product>-v<version>`.
2. **Pushing the tag** to this repository's remote; the mirror carries it to GitHub, where the
   product's release workflow builds every target, checks everything and creates a **draft**
   release.
3. **A maintainer reviews the draft and publishes it.**

`cargo xtask package auberean|yalain|athenaeum` is the build itself: the same command locally and
in the workflow. The world database Yalain serves is released apart, from its own repository
(`yalain world release`); Yalain pins the release it is built for.

## Release notes

A release's description on GitHub starts with its notes: the release's section of the product's
`CHANGES.md` (`auberean/`, `yalain/` or `athenaeum/`) as **Highlights**, then **All changes**, the
subject of every commit since the product's previous final tag that changed what it ships: for
Auberean a file under `auberean/` (but not its test kit or world viewer) or `core/auberean-*`; for
Yalain one under `yalain/` (but not its test kit) or a core crate it is built from; for Athenaeum
one under `athenaeum/`. A first release has its highlights alone, and a release without highlights
is refused. To see the notes before releasing:

```
cargo xtask release-notes auberean|yalain|athenaeum [--since <tag>] [--version <version>] [--out <file>]
```

## Versions and tags

Each product's crates carry one version of their own, rather than the workspace's, which must
agree: Auberean's client and headless client; every `yalain-*` crate and the `yalain` program;
Athenaeum's editor, command and Foundry. A release version is `MAJOR.MINOR.PATCH`, optionally with
a pre-release suffix such as `-rc.1`; the workflow fails when the tag does not name the version the
tagged commit carries, or the commit is not on `main`.

Every program answers `--version`: its name and version, the commit and the target it was built
from.

## What a release holds

| product | system | the release file |
|---|---|---|
| Auberean | Windows (x86-64) | `auberean-<v>-windows-x86_64.zip`: `auberean-<v>/` holding the launcher `dereth.exe`, `auberean.exe`, `launcher-defaults.json`, `LICENSE`, `NOTICE.txt`, `THIRD-PARTY-LICENSES.html`. No installer |
| | macOS (Apple silicon) | `Auberean-<v>-macos-aarch64.app.tar.gz`: `Dereth.app`, the launcher and `auberean` in `Contents/MacOS`, `launcher-defaults.json` and the notices in `Contents/Resources` |
| | Linux (x86-64) | `Auberean-<v>-linux-x86_64.AppImage`: the launcher's AppImage, with `auberean` and `launcher-defaults.json` in its `usr/bin` |
| Athenaeum | Windows | `athenaeum-<v>-windows-x86_64.zip`: `athenaeum-editor.exe`, `athenaeum.exe`, `yalain.exe`, `auberean.exe`, the documents and the notices |
| | macOS | `Athenaeum-<v>-macos-<arch>.app.tar.gz`: `Athenaeum.app`, the four programs in `Contents/MacOS` |
| | Linux | `Athenaeum-<v>-linux-x86_64.AppImage`: the editor, or the program its first argument names (`athenaeum`, `yalain`, `auberean`) |
| Yalain | every Empyrean target | `yalain-<v>-<target>.zip` on Windows, `.tar.gz` elsewhere: `yalain-<v>-<target>/` holding `yalain`, `README.md`, `CHANGES.md` and the notices |

Beside them: `MANIFEST.txt` (every file in every package, with its size, SHA-256 and what the
header checks read from each program), `release.json` and `SHA256SUMS`; and for Auberean, as for
Dereth, `latest.json`, the launcher's update manifest, whose version is the launcher's.

| Product | Target | Built on | Runs on |
|---|---|---|---|
| Auberean, Athenaeum | `x86_64-pc-windows-msvc` | Windows | Windows 10 or newer |
| | `aarch64-apple-darwin` | macOS | macOS 11 or newer |
| | `x86_64-unknown-linux-gnu` | Linux (the workflow uses Ubuntu 22.04) | glibc 2.35 or newer |
| Yalain | `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` | any host, with `cargo zigbuild` | glibc 2.28 or newer |
| | Windows, macOS | its own system | as Auberean |

Asheron's Call 2's programs draw through wgpu, which uses Metal on macOS: Athenaeum's bundle
carries no MoltenVK, and Auberean's has it taken out (the launcher's build takes it in, for
Dereth's client). **Not code-signed yet**, as Lyceum's: Windows SmartScreen warns once, macOS
refuses once. The macOS bundles are signed ad hoc, or with a Developer ID under the hardened runtime
when `APPLE_SIGNING_IDENTITY` names one. Auberean's release files are signed for the launcher's
updater with the launcher's update key, as Dereth's are; Yalain and Athenaeum have no updater of
their own: the launcher downloads a release, and checks it against `SHA256SUMS`.

**No game data, ever**: only named files go into a package; a deny scan reads the staging folder
and the written archive (for an AppImage, the folder it is packed from, and the AppImage itself as
one file) and refuses a `.dat`, anything named like either game's files or carrying either game's
data-file header, a database, a link, a stray file or one over the size cap. Each program is read
back: on Windows x86-64 with no import of the Visual C++ runtime, the windowed programs windowed
with their icons and the rest console programs (Yalain with its icon); on Linux the machine, the
loader and no glibc symbol newer than the floor; on macOS only system libraries, macOS 11 and a
code signature.

## `cargo xtask package`

```
cargo xtask package auberean --launcher <dir> [--target <triple>]... [--out <dir>] [--allow-dirty]
cargo xtask package athenaeum [--target <triple>]... [--out <dir>] [--allow-dirty]
cargo xtask package yalain [--target <triple>]... [--out <dir>] [--allow-dirty]
cargo xtask package <product> --gather <dir>
```

Builds the product's programs (Athenaeum's with `cargo build --locked --release`, Yalain in the
`server-release` profile, Linux with `cargo zigbuild`) for each `--target` (default: this host's),
lays out, scans, checks and archives each release file, and writes the index in
`target/package/<product>-<version>/` unless `--out` names another folder. It refuses a tree with
uncommitted changes unless `--allow-dirty`, and a shallow clone. `--gather <dir>` builds nothing:
it scans the release files already in `<dir>` and writes the index over them.

Auberean's takes the launcher's release file for the launcher version in the tree, from the folder
`--launcher` names (`cargo xtask package launcher --fetch <dir>`, launcher/RELEASING.md), so its
launcher is exactly that release; it never takes another. It builds the Auberean client and puts it,
`launcher-defaults.json` and Auberean's notices in. On macOS the bundle is signed again; on Linux
the AppImage is unpacked (`--appimage-extract`) and packed again with `appimagetool`. With
`TAURI_SIGNING_PRIVATE_KEY` set it signs each release file for the launcher's updater and writes
its piece of `latest.json`, as Dereth's package does.

What it needs, and says when missing, before building anything:

- **cargo-about**: `cargo install --locked cargo-about --features cli`.
- **Auberean:** the launcher's release files, and on Linux `appimagetool`.
- **Windows:** the MSVC build tools.
- **macOS:** Xcode's command-line tools; with Xcode itself, the layered icon is compiled too.
- **Linux, Auberean and Athenaeum:** `appimagetool` (`APPIMAGETOOL`, or on `PATH`), and the AppImage
  runtime it embeds named in `APPIMAGE_RUNTIME` (the workflow pins both by SHA-256); Auberean's also
  needs the launcher's web view libraries, as Dereth's. Build on Ubuntu 22.04.
- **Linux, Yalain:** `cargo zigbuild` and `zig`, as Empyrean's package needs.

`AUBEREAN_BUILD_SOURCE_URL`, `YALAIN_BUILD_SOURCE_URL` and `ATHENAEUM_BUILD_SOURCE_URL` name the
repository the notices name as the source (default `https://github.com/dereth-network/dereth`).
Auberean's launcher, as Dereth's, takes `DERETH_BUILD_SOURCE_URL`: the repository it updates from
and its release files are downloaded from.

## `cargo xtask release <product> <version>`

```
cargo xtask release auberean|yalain|athenaeum <MAJOR.MINOR.PATCH[-pre]> [--push] [--remote <name>]
```

1. Checks: the checkout is on `main`; no tracked file has uncommitted changes; `<version>` is not
   older than the version the product's crates carry; `<product>-v<version>` exists neither here
   nor on the remote; and the product's `CHANGES.md` has highlights for the release.
2. Sets the product's crates' `version`, updates their entries in `Cargo.lock`, turns the
   Unreleased section of `CHANGES.md` into `## <version> (<date>)` under a new empty one (not for a
   pre-release), and commits exactly those files as `<Product> <version>`.
3. Runs `cargo xtask ci tier0`, then `cargo xtask ci tier1 --game auberean` when Asheron's Call 2's
   data is found (`AUBEREAN_TEST_DAT_DIR` and `AUBEREAN_TEST_CAPTURE_INDEX`; it says so when it is
   not), then `cargo xtask package <product>` for the host's target.
4. Tags that commit `<product>-v<version>` (annotated), and prints the release notes.
5. Prints `git push <remote> main` and `git push <remote> <product>-v<version>`, and runs them only
   with `--push`. The remote is `origin` unless `--remote` names another.

If a check fails after the version commit, no tag is made; fix it and run the command again (it
does not bump twice), or drop the commit with `git reset --hard HEAD~1`. `cargo xtask ship` cuts
these releases, as it cuts Lyceum's, for a preflighted branch that changed them.

## The release workflows

`.github/workflows/release-auberean.yml`, `release-yalain.yml` and `release-athenaeum.yml` run when
the product's tag reaches GitHub: the tag names the version on `main`; each target is packaged on a
runner of its own system and its programs answer `--version` with the release's version and the
target (Yalain's in AlmaLinux 8 and Ubuntu 24.04 too, on Linux); every runner's files are gathered
and scanned again; and, for a tag, a draft release is created with the notes, the downloads and
every file labelled, never the repository's "Latest". Running a workflow by hand is a dry run.

Auberean's workflow waits for the launcher release the tree names, as Dereth's does, and builds with
the same source URL and the launcher's update-signing key; its smoke run checks that the launcher
answers `dereth <launcher version>`, the client `auberean <version>`, that the defaults file names
Auberean, that no Dereth client is in the package, and that the retail client's friends library came
with the launcher. A tag's run refuses
unsigned files and a missing `latest.json`.

## Publishing

On the draft release: check the run is green and the draft has every release file, `SHA256SUMS`,
`MANIFEST.txt` and `release.json` (and Auberean's `latest.json`), each labelled; try the release on
each system you can (a launcher from Dereth's package downloads the newest published Auberean
client, Yalain and Athenaeum); read the notes; publish. Once Auberean's release is published,
launchers on Auberean's channel update from it.
