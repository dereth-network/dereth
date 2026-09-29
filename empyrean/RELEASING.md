# Releasing Empyrean

An Empyrean release is the two binaries, `empyrean-server` and `empyrean-import`, with their
guides and notices, for five targets, published as a GitHub release of this repository. There are
two commands and one review:

1. **`cargo xtask release empyrean <version>`** on a clean `main`: sets the version, runs the
   checks, packages the host's target as a smoke test and tags `empyrean-v<version>`.
2. **Pushing the tag** starts the release workflow on GitHub, which builds every target, checks
   everything again and creates a **draft** release.
3. **A maintainer reviews the draft and publishes it.**

`cargo xtask package empyrean` is the build itself: the same command locally and in the workflow.

## Versions and tags

Empyrean has one version, the `version` every `empyrean-*` crate carries (`empyrean/server/Cargo.toml`
and its siblings), separate from the client's and the shared crates'. A release version is
`MAJOR.MINOR.PATCH`, optionally with a pre-release suffix such as `-rc.1` (no `+build` suffix). The
server reports the numeric core to clients and in `@empversion`; a pre-release is published as a
GitHub pre-release. The tag is `empyrean-v<version>`, and it
must name exactly the version the tagged commit carries; the workflow fails otherwise, and when the
tagged commit is not on `main`.

## What a release holds

One archive per target, named `empyrean-<version>-<target>`, a `.zip` for Windows and a `.tar.gz`
elsewhere, each with one folder of the same name holding exactly:

| File | What |
|---|---|
| `empyrean-server`, `empyrean-import` (`.exe` on Windows) | The binaries. |
| `README.md`, `SETUP.md`, `DIVERGENCES.md`, `ACE-BUGS.md` | The guides and registers. |
| `empyrean.toml.example` | Every configuration key with its default. |
| `LICENSE` | AGPL-3.0-only. |
| `NOTICE.txt` | The exact source commit and where it is (the AGPL source offer), the ACE attribution, the Lifestoned data model's MIT notice, SQLite's public-domain statement, the shared crates' MIT licence and every crate the binaries are built from. |
| `THIRD-PARTY-LICENSES.html` | Every third-party crate's licence text, from cargo-about (`empyrean/about.toml`, `empyrean/about.hbs`). |

Beside the archives: `empyrean-<version>-source.tar.gz` (`git archive` of the tagged commit),
`MANIFEST.txt` (each file's size and SHA-256, and what the header checks read from each binary),
`release.json` (the same, machine-readable, with the upgrade declaration below) and `SHA256SUMS`.

## What upgrading involves: the upgrade declaration

Every release's `release.json` carries an `upgrade` object, which the server's updater
(`empyrean-server update`, `server.update`; SETUP.md 6.4) reads before it installs anything:

| Field | What | From |
|---|---|---|
| `kind` | `initial`, `patch`, `minor` or `major`, against the previous release | the tags |
| `previous` | the previous release: the newest `empyrean-v*` tag, not a pre-release, older than this version | the tags |
| `upgrades_from` | the oldest version that can upgrade straight to this one | `empyrean/releases.toml` |
| `databases` | the shard and authentication schema versions (the store's migration lists) | the code |
| `world_pack` | the pack format and record schema the server reads | the code |
| `migrations` | each schema that changed since the previous release, from and to | the code, here and at the previous tag |
| `world_pack_rebuild` | whether the pack format or record schema changed since the previous release | the code, here and at the previous tag |
| `config` | keys renamed (`toml_config::RENAMED` entries whose `since` is this version), removed or newly required | the code and `empyrean/releases.toml` |
| `notes` | anything else an operator should read first | `empyrean/releases.toml` |

`empyrean/releases.toml` declares what the code cannot say, one `[[release]]` per first, minor
and major release (a pre-release uses its release's). **A patch release declares nothing:** it has
no entry, and `cargo xtask package empyrean` (and `cargo xtask release empyrean`, before it changes
anything) refuses a patch release that has an entry, migrates a database, changes the pack's
format or record schema, or renames a configuration key. A minor or major release needs an entry,
and its `upgrades_from` may not be newer than the previous release; the first release upgrades
from itself only. So a change that needs a migration, a new pack or a configuration change goes
out as a minor release, with its entry.

| Target | Built on | How | Runs on |
|---|---|---|---|
| `x86_64-pc-windows-msvc` | Windows | `cargo build`, the C runtime linked statically | Windows 10 or newer |
| `x86_64-unknown-linux-gnu` | any host with zig | `cargo zigbuild`, against glibc 2.28 | glibc 2.28 or newer (Debian 10, Ubuntu 18.10, RHEL 8) |
| `aarch64-unknown-linux-gnu` | any host with zig | `cargo zigbuild`, against glibc 2.28 | glibc 2.28 or newer |
| `aarch64-apple-darwin` | macOS | `cargo build` (ad-hoc signed by the linker) | macOS 11 or newer |
| `x86_64-apple-darwin` | macOS | `cargo build` | macOS 11 or newer |

**No game data, ever.** Only the files above are copied, each from a named path, and a deny scan
reads the staging folder and then the written archive. A `.dat`, anything named like the game
client's files (`client_*`, `acclient*`), a `world.pack`, a database (`*.db`, `*.sqlite`), a
private `empyrean.toml`, a capture, a file carrying the game's data-file header whatever its name,
a link, a folder, any file over 64 MB or any file not on the list fails the package.

**Header checks.** Each binary is read back: the target's format and architecture; on Windows the
console subsystem, the server's icon and no import of the Visual C++ runtime; on Linux the target's
dynamic loader, only the C library's shared objects, and no glibc symbol newer than 2.28; on macOS
only system libraries, a minimum of macOS 11, and a code signature on Apple silicon.

**Reproducible where cheap.** The build time the binaries report is the commit's
(`SOURCE_DATE_EPOCH` when set), local paths are remapped out of the binaries, dependencies are
`--locked`, the Windows linker writes no timestamp, and the archives carry the commit's time,
fixed permissions and no owner names. The same commit and toolchain should give the same bytes.

## `cargo xtask package empyrean`

```
cargo xtask package empyrean [--target <triple>]... [--out <dir>] [--allow-dirty]
cargo xtask package empyrean --gather <dir>
```

Builds both binaries in the `server-release` profile for each `--target` (default: this host's),
stages, scans, checks and archives them, then writes the source tarball, `MANIFEST.txt`,
`release.json` and `SHA256SUMS` beside the archives, in `target/package/empyrean-<version>/` unless
`--out` names another folder. It refuses a tree with uncommitted changes (the binaries would not be
the commit they name) unless `--allow-dirty`, and a shallow clone (the build number is the commit
count). `--gather <dir>` builds nothing: it scans the archives already in `<dir>` and writes the
index over them; the workflow's last job runs it on every runner's archives.

What it needs, and says when missing, before building anything:

- **cargo-about**, for `THIRD-PARTY-LICENSES.html`: `cargo install --locked cargo-about --features cli`.
- **Linux targets:** `cargo install --locked cargo-zigbuild`, zig on `PATH` (or
  `pip install ziglang`), and `rustup target add <target>`. Any host can build them.
- **Windows and macOS targets:** that host, with its own toolchain (the MSVC build tools; Xcode's
  command-line tools). `rustup target add x86_64-apple-darwin` on an Apple-silicon Mac.

Each binary also answers `--version`: its version, commit, build time, target and source.

## `cargo xtask release empyrean <version>`

```
cargo xtask release empyrean <MAJOR.MINOR.PATCH[-pre]> [--push] [--remote <name>]
```

1. Checks: the checkout is on `main`; no tracked file has uncommitted changes; `<version>` is three
   numbers, not older than the version the crates carry, and `empyrean-v<version>` exists neither
   here nor on the remote (default `origin`); its upgrade declaration keeps the rules above.
2. Sets every `empyrean-*` crate's `version` to `<version>`, updates `Cargo.lock`'s entries for the
   workspace's own packages, and commits exactly those files as `Empyrean <version>`. When the
   crates already carry `<version>` (the first release of a version), there is nothing to commit.
3. Runs `cargo xtask ci tier0`, then `cargo xtask package empyrean` for the host's target.
4. Tags that commit `empyrean-v<version>` (annotated).
5. Prints the two push commands, `git push <remote> main` and `git push <remote> empyrean-v<version>`,
   and runs them only with `--push`.

If a check fails after the version commit, no tag is made; fix the failure and run the command
again (it does not bump twice), or drop the commit with `git reset --hard HEAD~1`.

Before running it, run what public CI cannot: `cargo xtask ci tier1` with the retail data, and the
tier-2 modules the release touches on a hardware GPU.

## The release workflow

`.github/workflows/release-empyrean.yml` runs when an `empyrean-v*` tag reaches GitHub. Its jobs:

1. **version and tag:** the tag names the version the crates carry, and the tagged commit is on
   `main`.
2. **package**, one job per target, each on a runner of its own operating system and
   architecture (Windows; macOS; Linux x86-64 and arm64 on Ubuntu, built with zigbuild): runs the server's tests natively once per operating system, runs
   `cargo xtask package empyrean --target <target>`, unpacks the archive and smoke-runs both
   binaries (`--version`, and `empyrean-server --write-config`): Linux, natively on its own
   architecture, inside an AlmaLinux 8 container (glibc 2.28, the floor) and an Ubuntu 24.04 one
   (a current distribution); Intel macOS under Rosetta when the runner has it.
3. **gather the release:** downloads every archive, runs `cargo xtask package empyrean --gather`,
   and keeps the release files as a workflow artifact.
4. **draft the GitHub release** (tags only): `gh release create --draft --latest=false` with the archives, the
   source tarball, `SHA256SUMS`, `MANIFEST.txt` and `release.json`. It is the only job that can write, and it runs
   in the `release` environment.

The CI workflow runs tier 0 on the tagged commit as it does on every push. Running the release
workflow by hand (**Run workflow**) is a dry run: the same builds and checks, the release files
kept as an artifact for 7 days, and nothing published.

## Publishing

This repository is mirrored to GitHub, and the workflow runs there. After `cargo xtask release
empyrean <version> --push` (or the two push commands by hand), the tag reaches GitHub with the
mirror's next sync; push the mirror at once if it does not sync on push.

Then, on the draft release:

1. Check the workflow run is green, and the draft has five archives, the source tarball,
   `SHA256SUMS`, `MANIFEST.txt` and `release.json`.
2. Download one archive per operating system, look inside (the ten files and nothing else), and
   run the server against your own data.
3. Edit the notes: what changed, and the ACE-World release this build was tested against
   (the usage `empyrean-import` prints with no arguments names it).
4. Publish, with **Set as the latest release** unticked. The repository's latest release is
   Dereth's: its launchers update from the latest release's `latest.json`, so an Empyrean
   release marked latest would leave them with nothing to read. The workflow creates the draft
   with `--latest=false`; the box on the publish page is the last word, so check it.

Anyone can verify a download with `sha256sum -c SHA256SUMS --ignore-missing`. Releases are not
signed: the checksums catch a damaged or altered download, not a release published by someone
else.

**What GitHub needs.** Nothing beyond the workflow's own `GITHUB_TOKEN`: no secrets. Actions must be
enabled on the repository, with the default token allowed to create releases (the draft job asks
for `contents: write` itself). The `release` environment is created on first use; adding the maintainers as its required reviewers makes the draft job wait for approval.
