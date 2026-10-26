# Packages

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

## A package

A package is a folder, or a zip of one, with `plugin.toml` at its root (see
[the manifest](manifest.md)), or a script written for UtilityBelt's scripting, its folder holding
`index.lua` and no manifest ([ubscript.md](ubscript.md)). A zip may instead hold one folder with everything in it, as an
archiver makes of a folder; that folder is then the root. A package holds at most 2,000 files and
64 MiB unpacked; a zip that says it unpacks past that is refused before it is unpacked. Only text,
Lua, TOML, JSON, XML, image, font, VTank profile and route files, a compiled plugin's WebAssembly
([wasm.md](wasm.md)) may be in it: no native code. A package loads by its manifest and its
files alone; no file in it is a signature the host reads.

A host lists packages from its own places: the desktop client its per-user plugins folder (folders
and `.zip` files in it), the headless client `--plugins <folder or zip>` (a package, or a folder of
them), the web client the folders and zips the player added to the page.

## Its hash

A zip file's SHA-256 identifies the download, and is what a release page lists beside it.

## Installing from a release: the launcher

The launcher, started with `--plugins`, installs a plugin for the Dereth client from the URL of its
release's zip. Without `--plugins` it has no Plugins page and offers the client none. There is no
index: the player brings the URL, and an update fetches the same URL again, so a release's
"latest" link keeps a plugin current.

- **The hash.** The zip's SHA-256 is read from beside it, `<url>.sha256` (a `sha256sum` line, or
  the hash alone), or given by the player. A zip whose SHA-256 differs, or with no hash from
  either, is not kept.
- **The checks.** The zip must be a package with a manifest that reads. An update must be the
  same plugin id as the installed release.
- **Staged.** A release that passes is kept beside the installed one, not loaded, and the
  launcher shows the permissions its manifest names. The player enables it. Nothing changes for a
  release whose zip is the one installed or already staged.
- **Where.** Under the launcher's data folder, `plugins/plugins.json` and one folder per plugin
  with `current.zip` and `staged.zip`. The launcher offers the client each installed plugin's
  `current.zip` (`--offer-plugins`), which lists it off until the player ticks it; the retail
  client gets none.

## The author's tool

`dereth-plugin` (in the workspace's `tools/plugin`):

```text
dereth-plugin pack <folder> <out.zip>     the folder as a zip; prints the zip's SHA-256
dereth-plugin hash <zip>                  a package zip's id, version and SHA-256
dereth-plugin build <crate> <package>     a Rust plugin built as a component: <package>/plugin.wasm
```
