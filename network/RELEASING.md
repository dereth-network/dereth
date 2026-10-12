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

## `cargo xtask release network-hub|network-ace <version>`

```
cargo xtask release network-hub|network-ace <MAJOR.MINOR.PATCH[-pre]> [--push] [--remote <name>]
```

1. Checks: the checkout is on `main`; no tracked file has uncommitted changes; `<version>` is a
   release version, not older than the one the tree carries, and its tag (`network-hub-v<version>`
   or `network-ace-v<version>`) exists neither here nor on the remote (default `origin`). For the
   mod, `dotnet` is on `PATH`, and `DERETH_ACE_DIR`, when it is set, names a folder holding
   `ACE.Server.dll` and `0Harmony.dll`. Every unmet check is named.
2. Sets the version and commits exactly those files as `Dereth Network hub <version>` or
   `Dereth Network ACE mod <version>`: for the hub, `version` in `network/hub/Cargo.toml` and its
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
