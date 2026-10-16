# Releasing the hub and the ACE mod

The hub and the ACE mod are released on tags of their own, beside Dereth's, Empyrean's and the web
client's:

| tag | workflow | version from | release files |
|---|---|---|---|
| `network-hub-v<version>` | `.github/workflows/release-network-hub.yml` | `network/hub/Cargo.toml` | the hub for Linux (x86-64 and arm64, glibc 2.28+), Windows and macOS (Apple silicon and Intel), each with its licences, `README.md`, `DEPLOY.md` and `THIRD-PARTY-LICENSES.html`; the container image for `linux/amd64` and `linux/arm64` as an OCI archive; `SHA256SUMS` |
| `network-ace-v<version>` | `.github/workflows/release-network-ace.yml` | `network/ace-mod/Dereth.Network/Meta.json` (`Version`), which `Dereth.Network.csproj`'s `<Version>` must equal | `Dereth.Network-<version>.zip` (the `Dereth.Network` mod folder); `SHA256SUMS` |

## Cutting a release

1. **`cargo xtask release network-hub <version>`** or **`cargo xtask release network-ace
   <version>`** on a clean `main`: sets the version, runs the product's checks, commits and tags
   it (below).
2. Push `main` and the tag: the command prints the two commands, and runs them with `--push`.
3. The workflow checks that the tag names the version and is on `main`, builds and checks every
   file (the hub's tests on each platform, a smoke run of each binary and of the image; the mod's
   whole harness and every patch applied against the pinned ACE release), and drafts the GitHub
   release with its notes (the commits since the previous tag of the same kind).
4. Review the draft and publish it. Publishing a hub release pushes its image, the one that was
   checked, to `ghcr.io/<repository>/network-hub:<version>` (and `:latest`, unless the version is a
   pre-release, `0.2.0-rc.1`).

Running either workflow by hand (workflow_dispatch) is a dry run: the same builds and checks, the
files kept as a workflow artifact for 7 days, nothing published.

## What a hub release promises the hubs that install it

Running hubs install releases themselves (`dereth-network-hub self-update`,
[`hub/DEPLOY.md`](hub/DEPLOY.md) section 9), many of them unattended with `--policy patch`. They
rely on:

- **A patch release never changes the registry's schema** (`SCHEMA_VERSION` in
  `network/hub/src/registry.rs`). A change to it is a minor release at least. `self-update
  --policy patch` refuses a release whose binary reads another schema, so a slip here stops the
  automatic updates rather than migrating registries unattended.
- **The files and the answers `self-update` reads:** the tag `network-hub-v<version>`; the archive
  `dereth-network-hub-<version>-<target>.tar.gz` (`.zip` for Windows) with the binary in a folder of
  the archive's name; `SHA256SUMS`; and the binary's answers to `--version`
  (`dereth-network-hub <version>`) and `--registry-schema` (the schema it reads).
- **The source:** the workflow builds with `DERETH_HUB_BUILD_SOURCE_URL` set to the repository it
  runs in, which is where a release's binary later looks for newer releases. A fork's releases
  point at the fork.

## `cargo xtask release network-hub|network-ace <version>`

```
cargo xtask release network-hub|network-ace <MAJOR.MINOR.PATCH[-pre]> [--push] [--remote <name>]
```

1. Checks: the checkout is on `main`; no tracked file has uncommitted changes; `<version>` is a
   release version, not older than the one the tree carries, and its tag (`network-hub-v<version>`
   or `network-ace-v<version>`) exists neither here nor on the remote (default `origin`). For the
   mod, `dotnet` is on `PATH`, and `DERETH_ACE_DIR`, when it is set, names a folder holding
   `ACE.Server.dll` and `0Harmony.dll`. Every unmet check is named.
2. Sets the version and commits exactly those files as `Dereth Network Hub <version>` or
   `Dereth.Network ACE <version>`: for the hub, `version` in `network/hub/Cargo.toml` and its
   entry in `Cargo.lock` (the protocol crate keeps its own version, which the workflow does not
   read); for the mod, `Version` in `Meta.json` and `<Version>` in `Dereth.Network.csproj`,
   together. When the tree already carries `<version>` there is nothing to commit.
3. Runs the product's checks as CI does:
   - the hub: the network's crates' tests (`dereth-network-protocol`, `dereth-network-hub`,
     `dereth-network-conformance`) and the conformance vectors through the suite's binary; then
     builds the hub's release binary for this host, which must answer `--version` with the version;
   - the mod: builds the hub the harness links its worlds through, then the harness, and runs it
     against that hub. With `DERETH_ACE_DIR` set to an ACE server folder it also builds the mod
     against that ACE, runs the harness's `targets` section, applies every patch with ACE's own
     Harmony (`PatchApply`), and checks the mod's release folder holds what its archive needs.
     Without it, those are left out and the output says so; the release workflow runs them against
     its pinned ACE release.
4. Tags that commit (annotated), and prints the notes the workflow will write: the commits since
   the previous tag of the same kind that touch the product's paths. The network keeps no change
   log.
5. Prints `git push <remote> main` and `git push <remote> <tag>`, and runs them only with `--push`.

If a check fails after the version commit, no tag is made; fix the failure and run the command
again (it does not bump twice), or drop the commit with `git reset --hard HEAD~1`.

## By hand

The workflows use no workspace tool, so that this folder can become a repository of its own.
Without the command, a release is cut by hand: on `main`, set the version (`version` in
`network/hub/Cargo.toml`, then `cargo build` so `Cargo.lock` follows; or `Version` in `Meta.json`
and `<Version>` in the mod's project, the same), commit, then tag that commit and push the tag:

```sh
git tag network-hub-v0.2.0
git push origin network-hub-v0.2.0
```

The workflow refuses a tag that does not name the version, or that is not on `main`.

## The ACE release the mod is checked against

The CI and release workflows download one ACE release (`ACE_VERSION`) and check its digest
(`ACE_SHA256`). To move to a newer ACE, change both in both workflows, after the mod's harness and
`PatchApply` pass against it (`network/ace-mod/README.md`, *Checking a build*).

## Preflight, then ship

The branch you are releasing can be tried on your own dev setup before anything is pushed, and
shipped in one command once it does what you wanted. The dev worlds are worlds you upgrade anyway: the preflight's upgrade
is their upgrade.

```
cargo xtask preflight setup --out setup       # once: a script per machine, to read and run there
cargo xtask preflight [--with <world>]        # on that branch; --dry-run says what it would do
cargo xtask preflight play                    # the client and the launcher, on the dev hub
cargo xtask preflight play --web              # the web client's build, on 127.0.0.1, on the dev hub
cargo xtask preflight rollback                # every world as it was, the dev hub stopped
cargo xtask ship                              # main to the preflighted commit, released, pushed
cargo xtask preflight finish                  # the final builds in the dev worlds, the real hub
```

- **`preflight`** works out the products the branch changed since `main` and the release each takes,
  and says why (the hub's is a minor release when its registry's schema moves; Empyrean's when a
  patch would be refused; Dereth's for the web client's changes too, since the web client carries
  Dereth's version). Before anything is built or stopped, it checks what it needs and says all of
  what is missing at once: the tools its builds need here, the dev Discord application's secret
  files (there, and not empty), `site_origins` beside a `discord_client_id` (the hub refuses one
  without the other), and each ACE world's `key_file`, which must hold a registry key (looked at
  where it is; the key is never shown). It builds each product in a worktree of the commit,
  stamped `<version>-rc.N`, and the web client with Dereth. It backs the real hub's registry up
  on its machine over ssh, copies it down, and runs the candidate hub on the copy: it counts
  every table before and after the migration, and drops every Discord mirror and webhook, every
  sign-in and link code, and every person's links but your own (`admins`), before it serves, with
  the dev Discord application, behind the tunnel. The dev hub must then answer on `listen` within
  half a minute; if it does not, the run stops before any world is touched, with the end of the
  dev hub's `hub.log`. Then each dev world is stopped, backed up (its server or mod, its
  configuration and Empyrean's databases), given its candidate, pointed at the dev hub (the
  original kept), and started. A second preflight builds the next candidate and keeps the first
  backups.
- **Every world must link.** For up to three minutes after the worlds start, the preflight watches
  the dev hub for each of them, by the slug its configuration names. The summary says which
  linked and which did not, and for each that did not, the log to read: ACE's `ACE_Log.txt`
  beside its server, Empyrean's service output (`journalctl -u <unit>` on Linux), and the dev
  hub's `hub.log`.
- **`preflight play`** starts the launcher on the dev hub's world list. The launcher keeps its copy
  of the list with the sources it was read from, so it reads the dev hub's list at once, and the
  ordinary launcher afterwards reads the network's again. **`--web`** serves the web client the
  preflight built on `127.0.0.1` alone (port 8080, or `--port`), its world list the dev hub's,
  and prints the address to open.
- **`ship`** fast-forwards `main` to exactly the preflighted commit, refusing when the branch has
  moved since (here, or on the remote if it is there), and cuts each product's release itself, as
  `cargo xtask release <product> <version> --push` does, never by starting cargo (whose build of
  xtask could not replace the running one on Windows). With Dereth, it also tags
  `dereth-web-v<version>` on Dereth's release commit and pushes it, which drafts the web client's
  release ([`dereth/web/DEPLOY.md`](../dereth/web/DEPLOY.md#releases)); it refuses before anything
  moves if that tag is there already. The workflows then draft the releases; you publish them.
- What a run changed is kept in the work folder's `preflight.json`, saved as soon as each world's
  backup is whole and after every world, so a run cut short can still be rolled back.

The setup file is `%APPDATA%\Dereth\preflight.toml` (`DERETH_PREFLIGHT` names another), never the
repository's:

```toml
work_dir = 'D:\dereth-preflight'          # builds, the registry's copy, backups, preflight.json

[hub]
ssh = "you@hub-vps"                       # the real hub's machine (key-based ssh)
remote_db = "/srv/dereth-hub/hub.sqlite"  # and its registry there
public_url = "https://dev-hub.example.org"
site_origins = ["https://dev-site.example.org"]  # the hub refuses a Discord client id without it
admins = ["<your Discord id>"]
discord_client_id = "<the dev application's id>"
discord_client_secret_file = 'D:\dereth-dev\client-secret.txt'
discord_bot_token_file = 'D:\dereth-dev\bot-token.txt'
tunnel = "dev-hub"

[[machine]]                               # a machine worlds run on, other than this one
name = "world-box"
ssh = "you@world-box"                     # key-based ssh from here
os = "windows"

[[machine]]
name = "world-vm"
ssh = "you@world-vm"
os = "linux"
target = "x86_64-unknown-linux-gnu.2.28"  # what cargo zigbuild builds Empyrean for

[[world]]
name = "empyrean"
kind = "empyrean"
machine = "world-box"                     # leave it out for a world on this machine
service = "EmpyreanDev"                   # the nssm service
dir = 'D:\worlds\empyrean'                # as world-box spells it

[[world]]
name = "ace"
kind = "ace"
machine = "world-box"
service = "ACEDev"
dir = 'D:\ACE\Server'                     # the folder holding ACE.Server.dll
mods_dir = 'D:\ACE\Mods'                  # ACE's ModsDirectory, when it is not Mods inside dir
depends_on = "MariaDB"                    # the service ACE waits for
mariadb_dir = 'D:\ACE\mariadb'            # if MariaDB is not a service yet, setup makes it one
mariadb_ini = 'D:\ACE\my.ini'             # with nssm: mariadbd --defaults-file=<this> --console
key_file = 'D:\dereth-dev\ace.key'        # on world-box

[[world]]
name = "infiltration"
kind = "empyrean"
machine = "world-vm"
service = "empyrean"                      # the systemd unit there
dir = "/srv/empyrean"
optional = true                           # only with --with infiltration
```

The one-time setup writes a script for each machine, to read and run once there as an
administrator (or root):
- **`setup.ps1`, this machine:** your account may start and stop each world's service here
  (`sc sdset`) and change its folder, so the preflight never runs as an administrator; an ACE
  world is made a service that waits for its database (MariaDB too, from `mariadb_dir`, when it
  is not one yet); its key from `key_file` is put in its service's environment, whether the
  service was made now or was there before, its other environment entries kept; each world's
  program (Empyrean's server, and the `dotnet` an ACE world's service runs) gets an inbound
  firewall rule, `dereth-world-<name>`; cargo zigbuild, zig, each Linux machine's target and the
  web client's are installed; the tunnel and its address are created (after `cloudflared tunnel
  login`); and the dev application's secret files are made readable by you alone.
- **`setup-<machine>.ps1`, each Windows machine:** its OpenSSH server is turned on, this machine's
  public key (`ssh_key`, or your usual one) logs in as the account the ssh address names, that
  account may start and stop the worlds' services and change their folders, and an ACE world's
  service, its key and each world's firewall rule are made as above.
- **`setup-<machine>.sh`, each Linux machine:** the account may run `sudo -n systemctl stop` and
  `start` of its worlds' units without a password, and nothing more, and may change the worlds'
  folders (`setfacl`).

### Worlds on other machines

A world with a `machine` runs there, reached over ssh with a key; several worlds may share one.
Nothing is built there: everything is built here and copied over with scp. A Windows machine's
worlds take this machine's builds, Empyrean's server and the mod (built against that world's own
ACE server, whose assemblies are copied here first), and are driven with PowerShell. A Linux
machine's take Empyrean built here with `cargo zigbuild` for its `target`, and are driven with
systemd. Each world's files as they were, its databases' logs included, are kept beside it, in
its folder's `.preflight-backup`, and a rollback puts them back from there. The dev hub stays on
this machine: every world links to it at `public_url`, through the tunnel.
