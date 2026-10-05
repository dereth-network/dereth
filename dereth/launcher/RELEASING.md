# Releasing Dereth

A Dereth release is the launcher, with the Dereth client inside it, for Windows, macOS and Linux,
published as a GitHub release of this repository. Every installed
launcher lists the repository's releases, picks the newest published `dereth-v<version>` release
and reads its `latest.json` to update itself and the client ("Updates" below). There are two
commands and one review:

1. **`cargo xtask release dereth <version>`** on a clean `main`: sets the version, dates the
   release's highlights in [`CHANGES.md`](../CHANGES.md), runs the checks, packages the host's
   target as a smoke test and tags `dereth-v<version>`.
2. **Pushing the tag** starts the release workflow on GitHub, which builds every target, checks and
   signs everything, and creates a **draft** release.
3. **A maintainer reviews the draft and publishes it.**

`cargo xtask package dereth` is the build itself: the same command locally and in the workflow.

## Release notes

A release's description on GitHub starts with its notes, in two parts:

- **Highlights:** the release's section of [`dereth/CHANGES.md`](../CHANGES.md), a few
  player-facing lines written by hand. Add a line under `## Unreleased` when a change lands that a
  player would notice. The release command turns that section into the release's own,
  `## <version> (<date>)`, and opens a new empty Unreleased section, in the version commit. A
  pre-release (`-rc.1`) leaves the section alone: its notes show the Unreleased highlights, and
  the final release takes them.
- **All changes:** the subject of every commit since the previous final `dereth-v*` tag
  (pre-release tags are passed over) that changed what a release ships: a file under `dereth/` or
  `core/`, or the web client's relay, that is not a test, a fixture or a document. Empyrean's
  commits, CI's, the tools', the version commits, and commits that change only comments and blank
  lines in those files are left out. Each subject goes under one heading (Gameplay, Graphics & sound, Interface, Launcher,
  Fixes), chosen from its words and then from where its files are; a subject the rules cannot
  place goes under "Other changes". Subjects only, without hashes, and one compare link at the end.

`main` gets one commit per finished piece of work, with a subject that says what the client now
does, so the subjects are the changelog. To see the notes before releasing:

```
cargo xtask release-notes dereth [--since <tag>] [--version <version>] [--out <file>]
```

It prints the notes for the commits since the newest final `dereth-v*` tag (or `--since`) up to
`HEAD`, with the Unreleased section as the highlights. With `--version` of a release that is
tagged, the range ends at its tag and the highlights are its own section (a pre-release's are the
Unreleased section as its tag has it). `--out` writes the notes to a file instead. The release workflow runs the same command on the tag (below), so the preview is what the
release gets.

## Versions and tags

The launcher and the client are released together, with one version: the `version` in
`dereth/client/Cargo.toml` and in `dereth/launcher/Cargo.toml`, which must agree. A release
version is `MAJOR.MINOR.PATCH`, optionally with a pre-release suffix such as `-rc.1`. The tag is
`dereth-v<version>`, and it must name exactly the version the tagged commit carries; the workflow
fails otherwise, and when the tagged commit is not on `main`. A pre-release is published as a
GitHub pre-release, and launchers do not update to it.

Both programs answer `--version`: `dereth <version>` (or `dereth-client <version>`), the commit
and the target they were built from.

## What a release holds

| system | the release file | its label |
|---|---|---|
| Windows (x86-64) | `dereth-<v>-windows-x86_64.zip`: one folder, `dereth-<v>/`, holding `dereth.exe`, `dereth-client.exe`, `LICENSE`, `NOTICE.txt`, `THIRD-PARTY-LICENSES.html`. No installer: unzip anywhere | Dereth · Windows x64 (zip, no installer) |
| macOS (Apple silicon, Intel) | `Dereth-<v>-macos-aarch64.app.tar.gz` (and `-x86_64`): `Dereth.app`, with the client in `Contents/MacOS`, MoltenVK in `Contents/Frameworks` and the notices in `Contents/Resources` | Dereth · macOS Apple silicon, Dereth · macOS Intel |
| Linux (x86-64) | `Dereth-<v>-linux-x86_64.AppImage`, the client inside it | Dereth · Linux x86-64 AppImage (glibc 2.35+) |

Every download holds the client, so the client is not published on its own. Beside them:

- `latest.json` (Updater manifest): what launchers read, one entry per platform
  (`windows-x86_64`, `darwin-aarch64`, `darwin-x86_64`, `linux-x86_64`), each the download
  address and its update signature. The signatures are published there only, not as separate
  `.sig` files: the updater reads nothing else;
- `MANIFEST.txt` (Release manifest): every file in every package, with its size, SHA-256 and what
  the header checks read from each program;
- `release.json` (Release facts (JSON)): the same, machine-readable;
- `SHA256SUMS` (Checksums (SHA-256)).

The labels are what GitHub shows on the release page; the file names are unchanged.

| Target | Built on | Runs on |
|---|---|---|
| `x86_64-pc-windows-msvc` | Windows | Windows 10 or newer, with the WebView2 runtime (Windows 11 has it; the launcher says where to get it when it is missing) |
| `aarch64-apple-darwin`, `x86_64-apple-darwin` | macOS | macOS 11 or newer |
| `x86_64-unknown-linux-gnu` | Linux (the workflow uses Ubuntu 22.04) | glibc 2.35 or newer (Ubuntu 22.04 and later), with WebKitGTK 4.1 and a Vulkan driver |

Each target builds on its own operating system: the launcher links that system's web view.

**Not code-signed yet.** Windows SmartScreen warns once (**More info**, **Run anyway**); macOS
refuses once (**System Settings**, **Privacy & Security**, **Open Anyway**). The macOS bundles are
signed ad hoc, which Apple silicon needs to run them at all, and without the hardened runtime: an
ad-hoc signature carries no developer team, so the hardened runtime's library validation would
refuse the bundled MoltenVK. With `APPLE_SIGNING_IDENTITY` set to a Developer ID, the package signs
the bundle and MoltenVK with it under the hardened runtime, as notarisation requires.

**No game data, ever.** Only named files are copied into a package, and a deny scan reads the
staging folder and then the written archive: a `.dat`, anything named like the game client's files
(`client_*`, `acclient*`), a file carrying the game's data-file header whatever its name, a link,
a file not on the package's list or one over the size cap fails the package. An AppImage is one
file that only the launcher's own build fills; its programs are checked as they were built.

**Header checks.** Each program is read back: on Windows the target's machine, the windowed
subsystem, the icon, and no import of the Visual C++ runtime (it is linked in); on Linux the
target's machine and dynamic loader and no glibc symbol newer than 2.35; on macOS only system
libraries, a minimum of macOS 11 and a code signature. MoltenVK must be a Mach-O library.

## Updates: which release, signing and `latest.json`

At start a launcher lists its repository's releases through GitHub's REST API and takes the newest
**published** release tagged `dereth-v<MAJOR>.<MINOR>.<PATCH>`: drafts, pre-releases (by GitHub's
flag or by a `-pre` suffix), Empyrean's releases and other tags are passed over, and versions are
compared as semver, never by date or by their order in the list. It reads that release's
`latest.json`. Which release GitHub marks "latest" plays no part, so the box of that name on the
publish page can be left as GitHub sets it, for Dereth's releases and Empyrean's alike. The API
answers the 100 most recently created releases, which is where the newest Dereth release is. The
repository is the one the build names (`DERETH_BUILD_SOURCE_URL`, below). When the list cannot be
fetched (no network, GitHub's unauthenticated rate limit) or holds no such release, the launcher
skips the check quietly and tries again at its next start.

The launcher accepts an update only with a signature from the update-signing key whose public
half is `plugins.updater.pubkey` in `dereth/launcher/tauri.conf.json`. The signature is
minisign's, in the form Tauri's signer writes (`cargo tauri signer sign`): base64 of the signature
file, whose trusted comment names the file and the version. The updater refuses a signature for
another version than the one `latest.json` announces.

`cargo xtask package dereth` signs the launcher file itself when `TAURI_SIGNING_PRIVATE_KEY` is set
(the key file's base64, as `cargo tauri signer generate` writes it, or a path to it) with
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, and checks the signature at once the way the updater checks
it, against the launcher's public key: a key that is not the launcher's fails the package. It
writes `latest-<platform>.json`, that platform's entry, which carries the signature. Without a
key the package is built **unsigned** and says so, and no `latest.json` is written; a tag's
workflow run refuses that. The Windows entry is the zip itself: the launcher's own update unpacks it and swaps the files
in (README.md, "Updates"); macOS's is the `.app.tar.gz` and Linux's the AppImage, which the updater
installs itself.

To make a new key pair (only if the old one is lost; every launcher already installed trusts the
old public key, so it would not update past that release):

```
cargo tauri signer generate -w dereth-update.key
```

and put the printed public key in `plugins.updater.pubkey`.

## `cargo xtask package dereth`

```
cargo xtask package dereth [--target <triple>]... [--out <dir>] [--allow-dirty] [--moltenvk <dir>]
cargo xtask package dereth --gather <dir>
```

Builds the client (`cargo build --release -p dereth-client`) and the launcher (`cargo tauri build`
in `dereth/launcher`, with the client, the notices and on macOS MoltenVK added to the bundle)
for each `--target` (default: this host's), stages, scans, checks, archives and signs the
launcher's file, and writes the index (`MANIFEST.txt`, `release.json`, `latest.json` when
signed, `SHA256SUMS`) in `target/package/dereth-<version>/` unless `--out` names another folder. It refuses a tree with
uncommitted changes (the programs would not be the commit they name) unless `--allow-dirty`, and a
shallow clone. `--gather <dir>` builds nothing: it scans the files already in `<dir>`, verifies
every signature again, and writes the index over them; the workflow's last job runs it on every
runner's files.

What it needs, and says when missing, before building anything:

- **cargo-about**, for `THIRD-PARTY-LICENSES.html`: `cargo install --locked cargo-about --features cli`.
- **The Tauri CLI**: `cargo install --locked tauri-cli --version ^2`.
- **Windows:** the MSVC build tools.
- **macOS:** Xcode's command-line tools (Xcode itself for the layered icon), and **MoltenVK**:
  its macOS release, `MoltenVK-macos.tar` from
  <https://github.com/KhronosGroup/MoltenVK/releases>, unpacked, named with `--moltenvk <dir>` or
  `DERETH_MOLTENVK_DIR`. The workflow pins the version and its SHA-256. For the Intel target on an
  Apple-silicon Mac, `rustup target add x86_64-apple-darwin`.
- **Linux:** Tauri's build dependencies and ALSA's headers, on Debian and Ubuntu
  `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev
  libasound2-dev build-essential pkg-config curl wget file patchelf`. The glibc of the build host
  is the oldest a release runs on, so a release is built on Ubuntu 22.04.

`DERETH_BUILD_SOURCE_URL` names the repository the release files are downloaded from, the
notices name as the source, and the launcher looks for its updates in (default
`https://github.com/dereth-network/dereth`). The package passes it to the build, where the
launcher compiles it in; a fork's release names its own repository and updates from it.

## `cargo xtask release dereth <version>`

```
cargo xtask release dereth <MAJOR.MINOR.PATCH[-pre]> [--push] [--remote <name>]
```

1. Checks: the checkout is on `main`; no tracked file has uncommitted changes; `<version>` is three
   numbers, not older than the version the client and the launcher carry, and `dereth-v<version>`
   exists neither here nor on the remote (default `origin`).
2. Sets the client's and the launcher's `version` to `<version>`, updates both lock files' entries
   for their workspaces' own packages (`Cargo.lock`, `dereth/launcher/Cargo.lock`), turns the
   Unreleased section of `dereth/CHANGES.md` into `## <version> (<date>)` under a new empty one
   (not for a pre-release), and commits exactly those files as `Dereth <version>`. When they already carry `<version>`,
   there is nothing to commit, except the highlights when `CHANGES.md` has no section for
   `<version>` yet (`Dereth <version>: its changes`).
3. Runs `cargo xtask ci tier0`, then `cargo xtask package dereth` for the host's target (on macOS
   with `DERETH_MOLTENVK_DIR` set).
4. Tags that commit `dereth-v<version>` (annotated), and prints the release notes the workflow
   will write from it.
5. Prints the two push commands, `git push <remote> main` and `git push <remote> dereth-v<version>`,
   and runs them only with `--push`.

If a check fails after the version commit, no tag is made; fix the failure and run the command
again (it does not bump twice), or drop the commit with `git reset --hard HEAD~1`.

Before running it, run what public CI cannot: `cargo xtask ci tier1` with the retail data, and the
tier-2 modules the release touches on a hardware GPU.

## The release workflow

`.github/workflows/release-dereth.yml` runs when a `dereth-v*` tag reaches GitHub. Its jobs:

1. **version and tag:** the tag names the version the client and the launcher carry, and the
   tagged commit is on `main`. It then writes the release notes from the tagged tree
   (`cargo xtask release-notes dereth --version <version>`; on a dry run, the notes the next
   release would get) and keeps them as the `release-notes-dereth` artifact; they are on the run's
   summary page too.
2. **package**, one job per target on a runner of its own system (Windows; Ubuntu 22.04; macOS for
   both Apple silicon and Intel): installs the build dependencies, cargo-about and the Tauri CLI
   (pinned), fetches MoltenVK on macOS (pinned by SHA-256, never from Homebrew), runs
   `cargo xtask package dereth --target <target>` with the signing secrets, and smoke-runs the
   packaged programs' `--version` (the AppImage unpacked; the macOS bundle's signature verified;
   Intel macOS under Rosetta when the runner has it). The launcher has no headless mode, so its
   window is not opened there. A tag's run fails if the launcher file is not signed.
3. **gather the release:** downloads every runner's files, runs `cargo xtask package dereth --gather`
   (every file scanned and every signature verified again; `latest.json` merged), and keeps the
   release files as a workflow artifact.
4. **draft the GitHub release** (tags only): `gh release create --draft` (`--prerelease` for a
   pre-release version) with the release notes, then the downloads, as its description, and every
   release file, `SHA256SUMS`, `MANIFEST.txt`, `release.json` and
   `latest.json`, each with its display label (the table above). The job computes each label from
   the file's name and fails if any file has none. It is the only job that can write, and it runs
   in the `release` environment.

Running the workflow by hand (**Run workflow**) is a dry run: the same builds and checks, the
release files kept as an artifact for 7 days, and nothing published.

## Publishing

After `cargo xtask release dereth <version> --push` (or the two push commands by hand), the tag
reaches GitHub with the mirror's next sync; push the mirror at once if it does not sync on push.

Then, on the draft release:

1. Check the workflow run is green, and the draft has four release files (the launcher for each
   of the four targets), `latest.json`, `SHA256SUMS`, `MANIFEST.txt` and `release.json`, each
   labelled.
2. Download the launcher for each system you can, run it, and play with it against a world.
3. Read the notes (Highlights, then All changes) and edit them if a line needs it; a highlight
   that was missing belongs in `CHANGES.md` too, under the release's section.
4. Publish. Launchers see the release from their next start once it is published (a draft is
   never read), whether or not GitHub shows it as the latest release.

Anyone can verify a download with `sha256sum -c SHA256SUMS --ignore-missing`.

**What GitHub needs:**

- Actions enabled on the repository, with the default token allowed to create releases (the draft
  job asks for `contents: write` itself).
- Two repository secrets: `TAURI_SIGNING_PRIVATE_KEY` (the update-signing key file's base64, as
  `cargo tauri signer generate` wrote it) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (its password).
- The `release` environment, created on first use; adding the maintainers as its required
  reviewers makes the draft job wait for approval.

**Without GitHub Actions.** `cargo xtask publish dereth <version>` makes the same draft from
`cargo xtask package dereth` run on a Windows, a macOS and a Linux machine, each signing its own
launcher files with the key above, then finishes and publishes it:
[CONTRIBUTING.md, "Releasing without GitHub Actions"](../../CONTRIBUTING.md#releasing-without-github-actions).
