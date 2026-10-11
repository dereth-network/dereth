# Releasing the hub and the ACE mod

The hub and the ACE mod are released on tags of their own, beside Dereth's, Empyrean's and the web
client's:

| tag | workflow | version from | release files |
|---|---|---|---|
| `network-hub-v<version>` | `.github/workflows/release-network-hub.yml` | `network/hub/Cargo.toml` | the hub for Linux (x86-64 and arm64, glibc 2.28+), Windows and macOS (Apple silicon and Intel), each with its licences, `README.md`, `DEPLOY.md` and `THIRD-PARTY-LICENSES.html`; the container image for `linux/amd64` and `linux/arm64` as an OCI archive; `SHA256SUMS` |
| `network-ace-v<version>` | `.github/workflows/release-network-ace.yml` | `network/ace-mod/Dereth.Network/Meta.json` (`Version`), which `Dereth.Network.csproj`'s `<Version>` must equal | `Dereth.Network-<version>.zip` (the `Dereth.Network` mod folder); `SHA256SUMS` |

## Cutting a release

1. On `main`, set the version: `version` in `network/hub/Cargo.toml` (then `cargo build` so
   `Cargo.lock` follows), or `Version` in `Meta.json` and `<Version>` in the mod's project. Commit.
2. Tag that commit and push the tag:

   ```sh
   git tag network-hub-v0.2.0
   git push origin network-hub-v0.2.0
   ```

3. The workflow checks that the tag names the version and is on `main`, builds and checks every
   file (the hub's tests on each platform, a smoke run of each binary and of the image; the mod's
   whole harness and every patch applied against the pinned ACE release), and drafts the GitHub
   release with its notes (the commits since the previous tag of the same kind).
4. Review the draft and publish it. Publishing a hub release pushes its image, the one that was
   checked, to `ghcr.io/<repository>/network-hub:<version>` (and `:latest`, unless the version is a
   pre-release, `0.2.0-rc.1`).

Running either workflow by hand (workflow_dispatch) is a dry run: the same builds and checks, the
files kept as a workflow artifact for 7 days, nothing published.

## Why tags alone

The rest of the repository cuts its releases with `cargo xtask release`. The network does not: this
folder is self-contained, so that it can become a repository of its own, and depends on no
workspace tool. A version is one line in one file (two for the mod, which the workflow holds equal),
and the workflow refuses a tag that does not name it, or that is not on `main`; there is no change
log to roll or declaration to write, so a command would do no more than the two steps above.

## The ACE release the mod is checked against

The CI and release workflows download one ACE release (`ACE_VERSION`) and check its digest
(`ACE_SHA256`). To move to a newer ACE, change both in both workflows, after the mod's harness and
`PatchApply` pass against it (`network/ace-mod/README.md`, *Checking a build*).
