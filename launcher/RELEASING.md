# Releasing Dereth

A Dereth release is the launcher, with the Dereth client inside it, for Windows, macOS and Linux,
published as a GitHub release of this repository. The launcher in it is the release of the launcher
version in the tree ("The launcher's own release" below): every installed launcher updates itself
from those, and fetches its game's newer client from that game's releases ("Updates" below). There
are two commands and one review:

1. **`cargo xtask release dereth <version>`** on a clean `main`: sets the version, dates the
   release's highlights in [`CHANGES.md`](../dereth/CHANGES.md), runs the checks, packages the host's
   target as a smoke test and tags `dereth-v<version>`.
2. **Pushing the tag** starts the release workflow on GitHub, which builds every target, checks and
   signs everything, and creates a **draft** release.
3. **A maintainer reviews the draft and publishes it.**

`cargo xtask package dereth` is the build itself: the same command locally and in the workflow.

## Release notes

A release's description on GitHub starts with its notes, in two parts:

- **Highlights:** the release's section of [`dereth/CHANGES.md`](../dereth/CHANGES.md), a few
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
`dereth/client/Cargo.toml` and in `launcher/Cargo.toml`, which must agree. A release
version is `MAJOR.MINOR.PATCH`, optionally with a pre-release suffix such as `-rc.1`. The tag is
`dereth-v<version>`, and it must name exactly the version the tagged commit carries; the workflow
fails otherwise, and when the tagged commit is not on `main`. A pre-release is published as a
GitHub pre-release, and launchers do not update to it.

The programs answer `--version`: `dereth <version>` (or `dereth-client <version>`, or
`dereth-plugin <version>`), the commit and the target they were built from.

## What a release holds

| system | the release file | its label |
|---|---|---|
| Windows (x86-64) | `dereth-<v>-windows-x86_64.zip`: one folder, `dereth-<v>/`, holding `dereth.exe`, `dereth-client.exe`, `dereth-plugin.exe`, `LICENSE`, `NOTICE.txt` and `THIRD-PARTY-LICENSES.html`. No installer: unzip anywhere | Dereth · Windows x64 (zip, no installer) |
| macOS (Apple silicon) | `Dereth-<v>-macos-aarch64.app.tar.gz`: `Dereth.app`, with the client and `dereth-plugin` in `Contents/MacOS`, MoltenVK in `Contents/Frameworks` and the notices in `Contents/Resources` | Dereth · macOS Apple silicon |
| Linux (x86-64) | `Dereth-<v>-linux-x86_64.AppImage`, the client and `dereth-plugin` inside it | Dereth · Linux x86-64 AppImage (glibc 2.35+) |
| each of the three, for plugin authors | `dereth-plugin-<v>-<platform>.zip` (Windows) or `.tar.gz` (macOS and Linux): one folder, `dereth-plugin-<v>/`, holding `dereth-plugin`, `LICENSE`, `NOTICE.txt` and `THIRD-PARTY-LICENSES.html`. The plugin author's tool of this release on its own (each release file above carries it too); plugin repositories made from its template download it from here and check it against `SHA256SUMS` | Plugin author's tool · Windows x64, · macOS Apple silicon, · Linux x86-64 (glibc 2.35+) |

Every launcher download holds the client, so the client is not published on its own. A release
carries no plugins: players install Dereth's from its plugin source. Beside them:

- `latest.json` (Updater manifest): what launchers read, one entry per platform
  (`windows-x86_64`, `darwin-aarch64`, `linux-x86_64`), each the download
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
| `aarch64-apple-darwin` | macOS | macOS 11 or newer |
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
**published** release tagged `launcher-v<MAJOR>.<MINOR>.<PATCH>`: drafts, pre-releases (by GitHub's
flag or by a `-pre` suffix), every other product's releases and other tags are passed over, and
versions are compared as semver, never by date or by their order in the list. It reads that release's
`latest.json`. Which release GitHub marks "latest" plays no part, so the box of that name on the
publish page can be left as GitHub sets it. The list is read a page at a time, as far as the newest
launcher release. The
repository is the one the build names (`DERETH_BUILD_SOURCE_URL`, below). When the list cannot be
fetched (no network, GitHub's unauthenticated rate limit) or holds no such release, the launcher
skips the check quietly and tries again at its next start.

The launcher accepts an update only with a signature from the update-signing key whose public
half is `plugins.updater.pubkey` in `launcher/tauri.conf.json`. The signature is
minisign's, in the form Tauri's signer writes (`cargo tauri signer sign`): base64 of the signature
file, whose trusted comment names the file and the version. The updater refuses a signature for
another version than the one `latest.json` announces.

`cargo xtask package launcher` and `cargo xtask package dereth` sign their launcher files themselves when `TAURI_SIGNING_PRIVATE_KEY` is set
(the key file's base64, as `cargo tauri signer generate` writes it, or a path to it) with
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, and checks the signature at once the way the updater checks
it, against the launcher's public key: a key that is not the launcher's fails the package. It
writes `latest-<platform>.json`, that platform's entry, which carries the signature. Without a
key the package is built **unsigned** and says so, and no `latest.json` is written; a tag's
workflow run refuses that. The Windows entry is the zip itself: the launcher's own update unpacks it and swaps the files
in (README.md, "Updates"); macOS's is the `.app.tar.gz` and Linux's the AppImage, which the updater
installs itself.

### Every package's launcher, and each game's client

Every package carries the same launcher. Dereth's release is the launcher with the Dereth client;
Auberean's release (`auberean-v<version>`, auberean/RELEASING.md) is the same launcher, the release
its tree names, with the Auberean client beside it and a small file,
`launcher-defaults.json`:

```json
{"realm": "auberean", "updates": "auberean"}
```

It sits beside the launcher (in `Dereth.app`, in `Contents/Resources`; in the AppImage, in
`usr/bin`). It names the realm a first run opens in, before the launcher has a state file (after
that the realm the player last showed wins), and the game whose client the launcher keeps up to
date. Dereth's package has no such file, which reads as Dereth. The launcher keeps a copy in its data
folder, since an update replaces the bundle it's in.

**The client is a component.** At its first start a package's launcher copies its client to the
components folder and runs it from there. On macOS it's in a bundle of its own, `<game>.app`, which
the launcher writes wherever it installs a client, with that game's name and icon; the launcher
carries both games' icons for this, compiled from each client's `assets/<game>.icon`. It's then updated from its game's own newest release (the
release file for this system, checked against that release's `SHA256SUMS`, with the client taken out
of it; an AppImage unpacks itself to give it up), apart from the launcher: its own game's as soon as
one is published, the other game's when first wanted and then from the page, as Yalain, Athenaeum,
Empyrean and Lyceum are.

**Every launcher updates from the launcher's own releases** (below), whichever package it came in.
A game's release still has a `latest.json`, whose version is the launcher it holds, signed for that
version. Installs from before those releases read it, install that package, and from then on read the
launcher's. Nothing else does.

### The signing key

To make a new key pair (only if the old one is lost; every launcher already installed trusts the
old public key, so it would not update past that release):

```
cargo tauri signer generate -w dereth-update.key
```

and put the printed public key in `plugins.updater.pubkey`.

## `cargo xtask package dereth`

```
cargo xtask package dereth --launcher <dir> [--target <triple>]... [--out <dir>] [--allow-dirty] [--moltenvk <dir>]
cargo xtask package dereth --gather <dir>
```

Builds the client (`cargo build --release -p dereth-client`) and puts it, the tool, the notices and
on macOS MoltenVK into the launcher's release file for each `--target` (default: this host's), then
stages, scans, checks, archives and signs the result (on macOS the bundle is signed again; on Linux
the AppImage is unpacked and packed again). `--launcher <dir>` names the folder of the launcher's
release files, and only the launcher version in the tree is taken from it, checked against its
`SHA256SUMS`. It writes the index (`MANIFEST.txt`, `release.json`, `latest.json` when
signed, `SHA256SUMS`) in `target/package/dereth-<version>/` unless `--out` names another folder. It refuses a tree with
uncommitted changes (the programs would not be the commit they name) unless `--allow-dirty`, and a
shallow clone. `--gather <dir>` builds nothing: it scans the files already in `<dir>`, verifies
every signature again, and writes the index over them; the workflow's last job runs it on every
runner's files.

What it needs, and says when missing, before building anything:

- **cargo-about**, for `THIRD-PARTY-LICENSES.html`: `cargo install --locked cargo-about --features cli`.
- **The launcher's release files**: `cargo xtask package launcher --fetch <dir>` gets the release of
  the version in the tree, or `cargo xtask package launcher --out <dir>` builds one to try locally.
- **Windows:** the MSVC build tools.
- **macOS:** Xcode's command-line tools (Xcode itself for the layered icon), and **MoltenVK**:
  its macOS release, `MoltenVK-macos.tar` from
  <https://github.com/KhronosGroup/MoltenVK/releases>, unpacked, named with `--moltenvk <dir>` or
  `DERETH_MOLTENVK_DIR`. The workflow pins the version and its SHA-256.
- **Linux:** `appimagetool`, Tauri's build dependencies and ALSA's headers, on Debian and Ubuntu
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
   numbers, not older than the version the client carries, and `dereth-v<version>` exists neither
   here nor on the remote (default `origin`); and the launcher version in the tree has its tag, with
   the launcher unchanged since ("The launcher's own release" below).
2. Sets `version` in Dereth's crates to `<version>`, updates `Cargo.lock`'s entries for the
   workspace's own packages, turns the
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

1. **version and tag:** the tag names the version the client carries, and the tagged commit is on
   `main`. It then writes the release notes from the tagged tree
   (`cargo xtask release-notes dereth --version <version>`; on a dry run, the notes the next
   release would get) and keeps them as the `release-notes-dereth` artifact; they are on the run's
   summary page too.
2. **the launcher release this tree names:** `cargo xtask package launcher --fetch <dir>`, kept as an
   artifact. A tag's run passes `--wait`: it stays until the `launcher-v<version>` release of the
   version in the tree is whole, draft or published, and fails naming it if that doesn't happen in
   time. No other launcher is ever used, and this workflow has nothing to build one with. A dry run
   looks once, so the release must already be there.
   **package**, one job per target on a runner of its own system (Windows; Ubuntu 22.04; macOS):
   installs the build dependencies and cargo-about (pinned), fetches MoltenVK on macOS (pinned by
   SHA-256, never from Homebrew) and `appimagetool` on Linux, runs
   `cargo xtask package dereth --target <target> --launcher <the release>` with the signing
   secrets, and smoke-runs the
   packaged programs' `--version`, the plugin author's tool's too (the AppImage unpacked; the
   macOS bundle's signature verified). The launcher has no headless mode, so its
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

1. Check the workflow run is green, and the draft has eight release files (the launcher and the
   plugin author's tool for each of the four targets), `latest.json`, `SHA256SUMS`, `MANIFEST.txt`
   and `release.json`, each labelled.
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
[CONTRIBUTING.md, "Releasing without GitHub Actions"](../CONTRIBUTING.md#releasing-without-github-actions).

## The launcher's own release

The launcher has releases of its own, `launcher-v<version>`, from
`.github/workflows/release-launcher.yml`, and a version of its own, in `launcher/Cargo.toml`.
`cargo xtask package launcher` builds it (the Tauri CLI, with the friends library and on macOS
MoltenVK as that build's inputs) and takes the client out, so each file holds the launcher alone:

```
cargo xtask package launcher [--target <triple>]... [--out <dir>] [--allow-dirty] [--moltenvk <dir>] [--friends-dll <file>]
cargo xtask package launcher --gather <dir>
cargo xtask package launcher --fetch <dir> [--wait] [--repo <owner/name>]
cargo xtask package friends-dll [--out <dir>] [--allow-dirty]
```

It needs what Dereth's package does, and **the Tauri CLI** (`cargo install --locked tauri-cli
--version ^2`) and **the retail Asheron's Call 2 client's friends library** (`auberean_friends.dll`),
which it carries beside the launcher (on macOS in `Contents/Resources`): a 32-bit Windows DLL, which
only Windows links. A Windows host builds it itself, with `rustup target add i686-pc-windows-msvc`;
another host is given the file `cargo xtask package friends-dll` wrote on Windows, with
`--friends-dll <file>` or `DERETH_FRIENDS_DLL`. The workflow builds it once and gives every job the
same file.

**Dereth's and Auberean's packages hold it.** Each takes the launcher version in the tree at its own
commit, never "the newest". So the launcher is cut first: `cargo xtask ship` releases it ahead of the
games whenever it changed, and `cargo xtask release dereth` or `auberean` refuses while that version
has no tag or the launcher has changed since it. `cargo xtask publish drafts` publishes its draft
before any game's. Its highlights are in [`CHANGES.md`](CHANGES.md).

Every launcher updates from the newest published one.

### Trying a game's package and a launcher update on a Mac

A build needs no signing key, and a local build takes an update by hand: replacing the bundle is what
the updater would do. It needs cargo-about, the Tauri CLI and a checkout with no uncommitted changes.

MoltenVK, and the friends library, which only Windows links, so it comes from a published release:

```sh
mkdir -p ~/swap-test && cd ~/swap-test
curl -fsSLO https://github.com/KhronosGroup/MoltenVK/releases/download/v1.4.2/MoltenVK-macos.tar
echo "f95765a6229cb7b915990a2890ce12ebe36a730b021545d3d52ae69ce4c4024e  MoltenVK-macos.tar" | shasum -a 256 -c -
tar -xf MoltenVK-macos.tar
export DERETH_MOLTENVK_DIR=$PWD/MoltenVK

v=0.4.12 # any published Dereth release
curl -fsSLO https://github.com/dereth-network/dereth/releases/download/dereth-v$v/Dereth-$v-macos-aarch64.app.tar.gz
tar -xzf Dereth-$v-macos-aarch64.app.tar.gz --strip-components 3 Dereth.app/Contents/Resources/auberean_friends.dll
export DERETH_FRIENDS_DLL=$PWD/auberean_friends.dll
```

The launcher's release files, then Dereth's package around them, from the checkout:

```sh
cargo xtask package launcher --target aarch64-apple-darwin --out ~/swap-test/launcher
cargo xtask package dereth --launcher ~/swap-test/launcher --target aarch64-apple-darwin --out ~/swap-test/dereth
```

The first start of the package copies its client out. Both folders below keep the test apart from your
own launcher and client settings, and the program is run directly so that they reach it. On Linux also
set `XDG_DATA_HOME=$PWD/xdg`, or the test's launcher rewrites your desktop's Dereth entry and `dereth://`
handler to point at the test's AppImage. Close the window when it has opened:

```sh
cd ~/swap-test && rm -rf Dereth.app
export DERETH_STATE_DIR=$PWD/state DERETH_SETTINGS_DIR=$PWD/settings
tar -xzf dereth/Dereth-*.app.tar.gz
Dereth.app/Contents/MacOS/dereth
ls state/components/dereth/*/Contents/MacOS   # dereth-client and dereth-plugin
```

Then the update: the launcher's own bundle in place of the package's, so no client is left in it. Play a
world from it:

```sh
rm -rf Dereth.app
tar -xzf launcher/Launcher-*.app.tar.gz
ls Dereth.app/Contents/MacOS                  # dereth alone
Dereth.app/Contents/MacOS/dereth
```

The client that starts is the copy in `state/components/dereth`. The launcher logs that there is no
published launcher release until there is one. If a newer Dereth has been published than the checkout's,
it fetches that client by itself, and the copy is then that version.

