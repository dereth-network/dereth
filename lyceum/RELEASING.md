# Releasing Lyceum

A Lyceum release includes the editor, the `lyceum` command, and the separate Empyrean server
and import tool, for Windows, macOS and
Linux, published as a GitHub release of this repository under a `lyceum-v<version>` tag. It is
released the way Dereth is ([`dereth/launcher/RELEASING.md`](../dereth/launcher/RELEASING.md)):
two commands and one review.

1. **`cargo xtask release lyceum <version>`** on a clean `main`: sets the version, dates the
   release's highlights in [`CHANGES.md`](CHANGES.md), runs the checks, packages the host's
   target as a smoke test and tags `lyceum-v<version>`.
2. **Pushing the tag** starts the release workflow on GitHub, which builds every target, checks
   everything and creates a **draft** release.
3. **A maintainer reviews the draft and publishes it.**

`cargo xtask package lyceum` is the build itself: the same command locally and in the workflow.

## Release notes

A release's description on GitHub starts with its notes: the release's section of
[`CHANGES.md`](CHANGES.md) as **Highlights**, then **All changes**, the subject of every commit
since the previous final `lyceum-v*` tag that changed a file under `lyceum/` other than the sample
world, a test or a document, grouped under Editor, Foundry and the lyceum command, Fixes and Other
changes. The first release has its highlights alone. To see the notes before releasing:

```
cargo xtask release-notes lyceum [--since <tag>] [--version <version>] [--out <file>]
```

## Versions and tags

The editor (`lyceum/editor`), the command (`lyceum/cli`) and the Foundry (`lyceum/crates/foundry`)
carry one version, their own rather than the workspace's, which must agree. A release version is
`MAJOR.MINOR.PATCH`, optionally with a pre-release suffix such as `-rc.1`, and the tag is
`lyceum-v<version>`: the workflow fails when the tag does not name the version the tagged commit
carries, or the commit is not on `main`. A pre-release is published as a GitHub pre-release.

Both programs answer `--version`: `lyceum-editor <version>` (or `lyceum <version>`), the commit
and the target they were built from.

## What a release holds

| system | the release file | its label |
|---|---|---|
| Windows (x86-64) | `lyceum-<v>-windows-x86_64.zip`: one folder, `lyceum-<v>/`, holding `lyceum-editor.exe` (with its icon), `lyceum.exe`, `empyrean-server.exe`, `empyrean-import.exe`, `README.md`, `CHANGES.md`, `LICENSE`, `NOTICE.txt`, `THIRD-PARTY-LICENSES.html`. No installer | Lyceum · Windows x64 (zip, no installer) |
| macOS (Apple silicon, Intel) | `Lyceum-<v>-macos-aarch64.app.tar.gz` (and `-x86_64`): `Lyceum.app`, with all four programs in `Contents/MacOS`, MoltenVK in `Contents/Frameworks`, and the icon, the documents and the notices in `Contents/Resources` | Lyceum · macOS Apple silicon, Lyceum · macOS Intel |
| Linux (x86-64) | `lyceum-<v>-linux-x86_64.tar.gz`: one folder, `lyceum-<v>/`, holding `lyceum-editor`, `lyceum`, `empyrean-server`, `empyrean-import`, the desktop entry `network.dereth.lyceum.desktop` and its icon `network.dereth.lyceum.png`, the documents and the notices | Lyceum · Linux x86-64 (glibc 2.35+) |

Every package also carries `EMPYREAN-LICENSE` and `LIFESTONED-LICENSE.txt`. Its notice
identifies Empyrean as a separate AGPL-3.0-only program and links the corresponding source
at the exact build commit. No world database is bundled; the editor offers to fetch it
with the bundled importer into the per-user cache.

Beside them: `MANIFEST.txt` (Release manifest: every file in every package, with its size, SHA-256
and what the header checks read from each program), `release.json` (Release facts (JSON)) and
`SHA256SUMS` (Checksums (SHA-256)).

| Target | Built on | Runs on |
|---|---|---|
| `x86_64-pc-windows-msvc` | Windows | Windows 10 or newer |
| `aarch64-apple-darwin`, `x86_64-apple-darwin` | macOS | macOS 11 or newer |
| `x86_64-unknown-linux-gnu` | Linux (the workflow uses Ubuntu 22.04) | glibc 2.35 or newer (Ubuntu 22.04 and later), with ALSA and a Vulkan driver |

**Not code-signed yet**, as Dereth's: Windows SmartScreen warns once, macOS refuses once (System
Settings, Privacy & Security, Open Anyway). The macOS bundle is signed ad hoc, without the
hardened runtime, so it runs on Apple silicon and the editor can load the bundled MoltenVK; with
`APPLE_SIGNING_IDENTITY` set to a Developer ID, MoltenVK, the three console programs and the bundle are signed
with it under the hardened runtime. Lyceum has no updater, so there is no update signature and no
`latest.json`: a release is downloaded by hand and checked with
`sha256sum -c SHA256SUMS --ignore-missing`.

**No game data, ever**, and **header checks**, as Dereth's: only named files go into a package; a
deny scan reads the staging folder and the written archive and refuses a `.dat`, anything named
like the game's files or carrying their header, a link, a stray file or one over the size cap.
Each program is read back: on Windows, x86-64 with no import of the Visual C++ runtime, the editor
windowed with its icon and the command a console program; on Linux the machine, the loader and no
glibc symbol newer than 2.35; on macOS only system libraries, macOS 11 and a code signature.
MoltenVK must be a Mach-O library.

## `cargo xtask package lyceum`

```
cargo xtask package lyceum [--target <triple>]... [--out <dir>] [--allow-dirty] [--moltenvk <dir>]
cargo xtask package lyceum --gather <dir>
```

Builds all four programs (`cargo build --locked --release -p lyceum-editor -p lyceum-cli
-p empyrean-server -p empyrean-import`) in the same job for each
`--target` (default: this host's), lays out, scans, checks and archives each release file, and
writes the index in `target/package/lyceum-<version>/` unless `--out` names another folder. It
refuses a tree with uncommitted changes unless `--allow-dirty`, and a shallow clone. `--gather
<dir>` builds nothing: it scans the release files already in `<dir>` and writes the index over
them; the workflow's gather job runs it on every runner's files.

What it needs, and says when missing, before building anything:

- **cargo-about**: `cargo install --locked cargo-about --features cli`.
- **Windows:** the MSVC build tools.
- **macOS:** Xcode's command-line tools, and **MoltenVK**'s macOS release unpacked, named with
  `--moltenvk <dir>` or `DERETH_MOLTENVK_DIR` (the same one Dereth's package takes). For the Intel
  target on an Apple-silicon Mac, `rustup target add x86_64-apple-darwin`.
- **Linux:** ALSA's headers, on Debian and Ubuntu `libasound2-dev build-essential pkg-config`.
  Build on Ubuntu 22.04: the build host's glibc is the oldest a release runs on.

`LYCEUM_BUILD_SOURCE_URL` names the repository the notices name as the source (default
`https://github.com/dereth-network/dereth`).

## `cargo xtask release lyceum <version>`

```
cargo xtask release lyceum <MAJOR.MINOR.PATCH[-pre]> [--push] [--remote <name>]
```

1. Checks: the checkout is on `main`; no tracked file has uncommitted changes; `<version>` is not
   older than the version the three crates carry; `lyceum-v<version>` exists neither here nor on
   the remote; and `CHANGES.md` has highlights for the release (its section, or Unreleased).
2. Sets the three crates' `version`, updates their entries in `Cargo.lock`, turns the Unreleased
   section of `CHANGES.md` into `## <version> (<date>)` under a new empty one (not for a
   pre-release), and commits exactly those files as `Lyceum <version>`.
3. Runs `cargo xtask ci tier0`, then `cargo xtask package lyceum` for the host's target.
4. Tags that commit `lyceum-v<version>` (annotated), and prints the release notes.
5. Prints `git push <remote> main` and `git push <remote> lyceum-v<version>`, and runs them only
   with `--push`.

If a check fails after the version commit, no tag is made; fix it and run the command again (it
does not bump twice), or drop the commit with `git reset --hard HEAD~1`.

## The release workflow

`.github/workflows/release-lyceum.yml` runs when a `lyceum-v*` tag reaches GitHub:

1. **version and tag:** the tag names the version the three crates carry, and the tagged commit is
   on `main`; then the release notes, kept as the `release-notes-lyceum` artifact.
2. **package**, one job per target on a runner of its own system (Windows; Ubuntu 22.04; macOS for
   both Apple silicon and Intel): installs ALSA's headers on Linux and cargo-about, fetches
   MoltenVK on macOS (pinned by SHA-256), runs `cargo xtask package lyceum --target <target>`, and
   smoke-runs the packaged programs: both answer `--version` with the release's version and the
   target, `lyceum --help` its commands, and `lyceum-editor --list-recent` what its start screen
   shows (the bundle's signature verified first; Intel macOS under Rosetta when the runner has
   it). The editor has no headless start screen, so its window is not opened there.
3. **gather the release:** every runner's files, scanned again, with the index written over them.
4. **draft the GitHub release** (tags only): `gh release create --draft --latest=false`
   (`--prerelease` for a pre-release version) with the notes, then the downloads, and every file
   with its label. It never becomes the repository's "Latest" release, which stays the launcher's
   and the server's. It is the only job that can write, and it runs in the `release` environment.

Running the workflow by hand (**Run workflow**) is a dry run: the same builds and checks, the
release files kept as an artifact for 7 days, and nothing published.

## Publishing

After `cargo xtask release lyceum <version> --push` (or the two push commands by hand), the tag
reaches GitHub with the mirror's next sync. Then, on the draft release: check the run is green and
the draft has the four release files, `SHA256SUMS`, `MANIFEST.txt` and `release.json`, each
labelled; download the release for each system you can and open a world with it; read the notes
and edit them if a line needs it (a missing highlight belongs in `CHANGES.md` too); publish.
