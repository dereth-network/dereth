# Contributing

## Which checks to run, and when

There is one command for each moment, and each covers the client and the server alike:

| When | Command | Needs | Takes (warm) |
|---|---|---|---|
| While working, before each commit | `cargo xtask check` | nothing | 1-2 minutes |
| Before pushing or opening a PR | `cargo xtask ci tier0` | nothing | about 3 minutes |
| With the retail data | `cargo xtask ci tier1` | the dats, `world.pack` and ACE's world dump (below) | about 15 minutes |
| On hardware | `cargo xtask ci tier2` | a graphics device and the dats | -- |

- **`check`** runs clippy and the tests of the crates you changed since your branch left
  `origin/HEAD` (`--base <ref>` to compare with another branch, `-p <crate>` to name crates
  instead), clippy over the crates that depend on them, and the fast rules: formatting, the
  comment and test lints, the seams, the crate headers.
- **`ci tier0` is exactly what CI runs. Run it before you push.** It builds everything, runs every
  test that needs no game data, clippy with every warning an error, every rule and the
  documentation check.
- **`ci tier1`** runs every crate's data tier and the server's real-content tests.
- **`ci tier2`** runs the tests that draw on a real graphics device and the comparisons against
  the original game.

`cargo xtask` with no arguments lists these and every single check. Each prints a pass/fail table;
a check that could not run says why and is never counted as a pass.

## Tests

Every command builds and runs the tests in the **`test-release` profile**: optimised, with debug
assertions and overflow checks kept on, so the tests run at release speed and a `debug_assert!` or an
overflow still fails them. `--debug` on any `cargo xtask` command selects the unoptimised dev profile,
for investigating one failure. A hand-typed command takes the same profile:
`cargo test --profile test-release -p <crate>`.

A crate's integration tests are one binary per tier (`cpu`, `dat`, `gpu`); [README.md](README.md#test-tiers)
explains them.

**Your own retail data.** The game's data files and `world.pack` are never part of this repository
and never shipped with it; the tests that need them read your copies through variables that only
tests read (no program does):

- `DERETH_TEST_DAT_DIR`: the folder holding `client_portal.dat`, `client_cell_1.dat`,
  `client_highres.dat` and `client_local_English.dat`. There is no default. Set it in your environment,
  or once for every build in a checkout with an `[env]` entry in a `.cargo/config.toml` above the
  workspace (`DERETH_TEST_DAT_DIR = { value = "<folder>", relative = true }` for a path relative to
  the folder holding that `.cargo/`).
- `EMPYREAN_TEST_WORLD_PACK`: a `world.pack` built with `empyrean-import` (see
  [empyrean/SETUP.md](empyrean/SETUP.md); default: `world.pack` at the top of the checkout).
- `EMPYREAN_WORLD_SQL`: ACE's world-database dump, for the importer's real-dump tests (default: the
  pinned release in the per-user cache that `cargo run -p empyrean-import -- fetch` downloads it into).

The tests of the older eras read four more, also with no default unless noted:

- `DERETH_TEST_PRETOD_DAT_DIR`: the February 2005 `portal.dat` and `cell.dat`, from before Throne of
  Destiny.
- `DERETH_TEST_DAT_CAPTURES_DIR`: older data files, one folder per capture date (`1999-10-09/`, …),
  each holding that date's `portal.dat` and `cell.dat` or `client_*.dat`.
- `DERETH_CLASSIC_PORTAL`: a `portal.dat` from before Throne of Destiny (the February 2005 one does),
  which the classic interface's tests draw from.
- `EMPYREAN_TEST_INFILTRATION_PACK`: a `world.pack` for the Infiltration era, built with
  `cargo run -p empyrean-import -- fetch --world 16py --pack --out world.pack.infiltration` (default:
  `world.pack.infiltration` at the top of the checkout). Without it, `ci tier1` reports the
  real-content rows that read it as NO-ORACLE.

**Test data in the tree.** `fixtures/` holds the shared recordings, the packet captures and the
message corpus reassembled from them, which the client's and the server's tests both read.
`empyrean/fixtures/vectors/` holds ACE's answers to the server's test questions, produced by running
ACE's own code, so they stay with the server.

## Behaviours, divergences and test anchors

A claim about what the client does is a row of the **behaviour registry**
(`dereth/testkit/src/behaviours/`): an id, a sentence, and the evidence it rests on.
`dereth/DIVERGENCES.md` lists where this client knowingly differs from the original and is generated
from the registry; `empyrean/DIVERGENCES.md` lists where the server knowingly differs from ACE.

A **test anchor** says which claim a test proves, and `cargo xtask test-lint` requires one. A client
integration test carries `/// Behaviour: <id>` naming a registry row (a module that proves no
behaviour says `//! Behaviour: none (<why>)`); a server test module carries `ACE:` (the ACE source it
holds the port to), `Vectors:` (the recorded ACE answers it replays) or `Divergence:` (the numbered
divergence it pins). Tests are named for what they prove, and an `#[ignore]` says why.

## Releases

Empyrean is released with two commands: `cargo xtask release empyrean <version>` sets the version,
runs the checks and tags the release, and pushing the tag starts the release workflow, which drafts
the GitHub release. `cargo xtask package empyrean` builds the same archives locally.
[empyrean/RELEASING.md](empyrean/RELEASING.md) describes the whole flow.

Dereth (the launcher, with the client inside it) is released the same way:
`cargo xtask release dereth <version>` and a pushed `dereth-v<version>` tag, whose workflow drafts
the release; once published, it is the one launchers update from.
`cargo xtask package dereth` builds this host's release files locally.
[dereth/launcher/RELEASING.md](dereth/launcher/RELEASING.md) describes the whole flow.

## Releasing without GitHub Actions

When the release workflows cannot run, `cargo xtask publish` makes the same GitHub release from
builds made on the maintainers' own machines: one Windows, one macOS and one Linux machine. It
talks to GitHub's REST API over HTTPS itself; no `gh` CLI is needed. The release it makes is the
one the workflow drafts from the same tag: the same files, labels, `SHA256SUMS`, `MANIFEST.txt`,
`release.json`, `latest.json` and `web.json`, the same title and notes, and the same pre-release
and "Latest" rules.

```
cargo xtask publish dereth|empyrean|web <version> --upload <dir> [--repo <owner/name>] [--yes]
cargo xtask publish dereth|empyrean|web <version> --finish [--repo <owner/name>] [--yes]
cargo xtask publish dereth|empyrean|web <version> --publish [--repo <owner/name>] [--yes]
```

**Every run is a dry run unless it is given `--yes`:** it reads what it needs, prints what it
would create, upload, replace or remove, and changes nothing. Run each step once without `--yes`,
read what it says, then again with it. The repository is `dereth-network/dereth` unless `--repo`
(or `DERETH_BUILD_SOURCE_URL` / `EMPYREAN_BUILD_SOURCE_URL`, which the packages are built for)
names another; the files must have been built for the repository they are published to.

### The three steps

1. **`--upload <dir>`**, on each machine, after `cargo xtask package <product> --out <dir>`. It
   checks the folder first: every file in it belongs to the release, every release file passes the
   data guard again, every Dereth launcher file is signed and its signature verifies against the
   launcher's key, and the files were built from the commit the tag names on GitHub, which must be
   on `main`. Then it creates the **draft** release for the tag if there is none yet, and adds the
   folder's files to it, labelled as the workflow labels them. A file the draft already has with
   the same SHA-256 is skipped, so an upload can be run again after a failure. A file of the same
   name with other bytes refuses the whole upload before anything is sent: two machines built
   different files, or a file was rebuilt (delete the draft's copy on GitHub if the new one is
   right). The folder's own `SHA256SUMS`, `MANIFEST.txt`, `release.json` and `latest.json` describe
   only that machine's files and stay behind. Besides the downloads, each machine uploads each
   file's lines of `MANIFEST.txt` (`<file>.manifest`) and, for Dereth, its platform's entry of
   `latest.json` (`latest-<platform>.json`): the **staging** files, which the next step merges and
   the last removes.
2. **`--finish`**, once, on any one machine, from a clean checkout of the tag with the tags
   fetched. It checks the draft holds everything the workflow's release would, and if anything
   is missing it lists each missing file by name (and what it is, such as a platform's update
   signature) and stops. Otherwise it downloads every machine's files (into `publish/<tag>/files/`
   in the target folder), checks each against the SHA-256 GitHub states, writes the merged
   `SHA256SUMS`, `MANIFEST.txt`, `release.json` and `latest.json` over them with the code the
   workflow runs (`cargo xtask package <product> --gather`: every archive scanned again, every
   update signature verified again; for the web client, its bundle scanned and its `web.json`
   checked against it), and adds those to the draft, replacing any older copy. It sets the title
   and the description: the release notes (`cargo xtask release-notes`, kept beside the files as
   `publish/<tag>/release-notes.md`), then the workflow's downloads text. **The release stays a
   draft.** Running it again changes only what differs, the description included, so edit the
   notes on GitHub after the last `--finish`. A dry run downloads the files too, to say what it
   would write.
3. **`--publish`**, after reviewing the draft as each `RELEASING.md` says. It checks the draft is
   finished (every file there, its notes written, its `SHA256SUMS` listing exactly the release's
   files with the SHA-256 each has; a file uploaded after `--finish` asks for `--finish` again),
   removes the staging files, and publishes it: a version with a pre-release suffix (`-rc.1`) as
   a pre-release, which is never "Latest"; a final Dereth or Empyrean release as the
   repository's "Latest"; the web client's release never as "Latest". Nothing is published
   without this step.

Nothing here makes or pushes a tag. The tag comes first, as for the workflows
(`cargo xtask release <product> <version> --push`; the web client's `dereth-web-v<version>` by
hand), and every step refuses a tag that is not on GitHub.

### The token

Each step reads a GitHub token from `DERETH_PUBLISH_TOKEN` (or `GITHUB_TOKEN`). Make a
**fine-grained** personal access token (GitHub: Settings, Developer settings, Personal access
tokens, Fine-grained tokens) for the one repository, with **Contents: Read and write** and nothing
else, and a short expiry. Set it in the shell for the session only, never in a file in the
checkout:

```
# PowerShell
$env:DERETH_PUBLISH_TOKEN = [Net.NetworkCredential]::new("", (Read-Host "token" -AsSecureString)).Password
# bash or zsh
read -rs DERETH_PUBLISH_TOKEN && export DERETH_PUBLISH_TOKEN
```

The token is sent to `api.github.com` and `uploads.github.com` in the `Authorization` header and
nowhere else: it is never printed, logged or written, and a download's redirect to GitHub's file
storage does not carry it. Without a token, `--upload` checks the folder alone (a useful first
run), and `--finish` and `--publish` refuse.

### The launcher's update-signing key

Dereth's launcher files are signed on the machine that builds them, by `cargo xtask package
dereth`, from the same two variables the workflow's secrets fill: `TAURI_SIGNING_PRIVATE_KEY`
(the path of the key file `cargo tauri signer generate` wrote, or its base64) and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Keep the key file and its password in a password manager or
on an encrypted disk, outside every checkout; copy the file to a build machine only for the build
and set both variables for that shell only. Never commit it, and never paste it into a command
line that is logged. Every installed launcher trusts only this key's public half, so a lost key
means launchers stop updating at the last release it signed. `package` verifies each signature
against the launcher's public key as soon as it signs; `--upload` refuses an unsigned launcher
file, and `--finish` verifies every signature again before it writes `latest.json`.

### Each product, end to end

Before starting, on every machine: a checkout of the tag with its whole history and tags
(`git fetch --tags`, `git checkout <tag>`; a shallow clone is refused), the build tools its
`RELEASING.md` lists, and the token. Each `package` command below writes into `dist/`.

**Dereth** (tag `dereth-v<version>`), with the signing key set on each machine:

| machine | builds | command |
|---|---|---|
| Windows | `x86_64-pc-windows-msvc` | `cargo xtask package dereth --out dist` |
| macOS | `aarch64-apple-darwin`, `x86_64-apple-darwin` | `cargo xtask package dereth --target aarch64-apple-darwin --target x86_64-apple-darwin --moltenvk <dir> --out dist` |
| Linux (Ubuntu 22.04) | `x86_64-unknown-linux-gnu` | `cargo xtask package dereth --out dist` |

On each machine in turn, `cargo xtask publish dereth <version> --upload dist`, then the same with
`--yes`. Then on one of them, `cargo xtask publish dereth <version> --finish`, and with `--yes`;
review the draft (four launcher files, `latest.json`, `SHA256SUMS`, `MANIFEST.txt`,
`release.json`); then `cargo xtask publish dereth <version> --publish --yes`. The Linux build's
glibc is the oldest the release runs on, so build it on Ubuntu 22.04.

**Empyrean** (tag `empyrean-v<version>`): first, on each machine, the server's tests as the
workflow runs them, `cargo test --locked --profile test-release -p 'empyrean-*'`.

| machine | builds | command |
|---|---|---|
| Windows | `x86_64-pc-windows-msvc` | `cargo xtask package empyrean --out dist` |
| macOS | `aarch64-apple-darwin`, `x86_64-apple-darwin` | `cargo xtask package empyrean --target aarch64-apple-darwin --target x86_64-apple-darwin --out dist` |
| Linux | `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` | `cargo xtask package empyrean --target x86_64-unknown-linux-gnu --target aarch64-unknown-linux-gnu --out dist` |

Then `cargo xtask publish empyrean <version> --upload dist` (and `--yes`) on each, `--finish` on
one, review the draft (five archives, `SHA256SUMS`, `MANIFEST.txt`, `release.json`), and
`--publish --yes`. The Linux archives are linked against glibc 2.28 by `cargo zigbuild` on any
Linux host. `--finish` writes `release.json` with the upgrade declaration, which reads the earlier
`empyrean-v*` tags, so fetch them first.

**The web client** (tag `dereth-web-v<version>`, the version Dereth carries) is one machine's
work, any of the three with the `wasm32-unknown-unknown` target installed:

```
cargo xtask package web --tag dereth-web-v<version> --out dist-web
cargo xtask publish web <version> --upload dist-web --yes
cargo xtask publish web <version> --finish --yes
cargo xtask publish web <version> --publish --yes
```

(each first without `--yes`). Its description is Dereth's notes for the version.

The workflows also smoke-run each packaged program's `--version`; run them yourself on each
machine before uploading (unpack the archive and run `dereth-client --version`, `empyrean-server
--version`), as each `RELEASING.md` describes.

## Documentation

The crates are not published to crates.io, so there is no docs.rs. Build the API documentation
locally with `cargo doc --workspace --no-deps --document-private-items --open`. (`cargo xtask doc`
is a check that the documentation's links and comments stay correct, not a way to read it.)

## Commits

- Comments, messages and commit text describe behaviour. They carry no planning labels (task ids,
  ticket or milestone names) and no pointers to documents outside this repository;
  `cargo xtask comment-lint` and `test-lint` enforce it.
- The code is rustfmt-clean (`cargo fmt --all`).
- A behaviour change comes with the test that pins it; a refactor keeps the same tests green and
  says which.
