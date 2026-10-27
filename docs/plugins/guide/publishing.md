# Publishing a plugin

**Provenance: design.** How a plugin reaches players: the author's tool, the template repository,
and a plugin source. The source's format is in [sources.md](../sources.md).

Players install plugins from **plugin sources**: lists, each one `plugins.json` at an `https://`
address, naming plugins at their latest release. To publish a plugin is to put its zip somewhere
and list it in a source. The author's tool, `dereth-plugin`, does every step, and the template it
writes does them on GitHub: each push checks the plugin, and each release tag packs it, makes a
GitHub release with the zip, and lists it in the repository's own `plugins.json`, published on the
repository's GitHub Pages. Players add that address in the launcher once and get every release
after it.

## The tool

| command | what it does |
|---|---|
| `dereth-plugin new <folder> [--id <id>] [--name <name>] [--github <owner>/<repo>]` | a new plugin repository, from the template |
| `dereth-plugin check <folder>` | every check a client makes when it loads the plugin, and every script compiled; a folder of plugins checks each one |
| `dereth-plugin pack <folder> <out.zip>` | the plugin as a zip, and `<out.zip>.sha256` beside it; the same folder always gives the same bytes |
| `dereth-plugin release <folder> <version> [--push]` | sets the version, moves the change log's notes under it, commits and tags; pushes with `--push` |
| `dereth-plugin notes <folder> <version>` | that version's notes from the change log |
| `dereth-plugin find <repository> <tag>` | the plugin a release tag names: its folder, id, version and name |
| `dereth-plugin add <plugins.json> <zip> <url> [--notes <file>] [--name <name>]` | lists the zip's release in a source, in place of the plugin's entry; `<url>` is where players download it, an `https://` address or a path beside the list |
| `dereth-plugin hash <zip>` | a zip's id, version and SHA-256 |
| `dereth-plugin build <crate> <folder>` | a Rust plugin built as a component ([getting-started-wasm.md](getting-started-wasm.md)) |

The tool carries Dereth's version, and its rules are those of the client released with it: the
API version a plugin may ask for, the permissions and capabilities it may name, the files a package
may hold. Each Dereth release carries it for every platform it ships, beside the launcher:
`dereth-plugin-<version>-windows-x86_64.zip`, or `-linux-x86_64.tar.gz`, `-macos-aarch64.tar.gz`
and `-macos-x86_64.tar.gz`, listed in the release's `SHA256SUMS`. Unpack it and run
`dereth-plugin`. In a Dereth checkout it also builds from source:

```text
cargo install --locked --path tools/plugin
```

## A new plugin repository

```text
dereth-plugin new xp-tracker --name "XP Tracker" --github you/xp-tracker
```

writes:

```text
xp-tracker/
  README.md                      what it is, the source's address to add, how to release
  .gitattributes, .gitignore
  .github/workflows/check.yml    every push and pull request: each plugin checked
  .github/workflows/release.yml  a release tag: packed, released, listed in plugins.json
  xp-tracker/                    the plugin: exactly what goes in its zip
    plugin.toml                  id io.github.you.xp-tracker, version 0.1.0, homepage the repository
    main.lua                     a window that says hello, and /xptracker
    CHANGES.md                   ## Unreleased
```

The plugin's id is `io.github.<you>.<name>` unless `--id` names another: an id is a reverse domain
name, and an author without a domain of their own uses their GitHub address's. Ids under
`network.dereth.` are Dereth's own and install only from Dereth's source.

Make the folder a git repository and push it to GitHub. The workflows download the tool of the
Dereth release the template was made with (`DERETH` in each workflow) from that release, and check
it against the release's `SHA256SUMS` before they run it: moving that line to a newer release moves
the repository to that release's rules.

## Working on it

Load the plugin into the client as you edit it, reloaded on every save:

```text
dereth-client --plugins xp-tracker --plugin-dev
```

or copy its folder into the client's plugins folder ([install-and-update.md](install-and-update.md#the-desktop-client-its-plugins-folder)).
`dereth-plugin check xp-tracker` runs what the client checks at load, and compiles every script, so
a syntax error fails there and in the check workflow rather than in a player's client.

## Releasing

Write what changed under `## Unreleased` in the plugin's `CHANGES.md`:

```markdown
# Changes

## Unreleased

- Counts luminance too.
```

then

```text
dereth-plugin release xp-tracker 0.2.0 --push
```

It refuses a version that is not newer than the plugin's, uncommitted changes, an existing tag and
an empty `## Unreleased`; checks the plugin; sets `version` in `plugin.toml`; moves the notes under
`## 0.2.0 (<today>)`; commits `XP Tracker 0.2.0`; tags `v0.2.0`; and pushes the commit and the tag.
Without `--push` it prints the two `git push` commands.

The tag starts the release workflow, which:

1. downloads the tool, checks it, and finds the plugin the tag names (`dereth-plugin find`), refusing a tag whose
   version is not the plugin's;
2. checks it, packs `io.github.you.xp-tracker-0.2.0.zip` with its `.sha256`, and takes the
   version's notes;
3. makes the GitHub release `v0.2.0` with the zip, the sum and the notes;
4. on the `gh-pages` branch (made on the first run), puts the zip in a folder named for the
   plugin, replacing the release before it, lists it in `plugins.json` (`dereth-plugin add`), and
   pushes the branch.

Once, after the first release, turn on GitHub Pages for the repository: **Settings → Pages**,
publish from the `gh-pages` branch. The source's address is then
`https://you.github.io/xp-tracker/plugins.json`, which the README gives players. Pages serves the
list and the zips to any page, so the web client can read them too.

A release is a fix forward: the list holds each plugin's latest release, and a launcher offers it to
everyone who installed the plugin from this source. A release that asks for a permission or a web
domain the one before did not waits for each player to accept it, whatever their update setting. A
release that needs a newer plugin API than a player's client is listed but not installable there
until they update Dereth.

## Many plugins in one repository

A repository may hold any number of plugins, each a folder of its own with its own `plugin.toml`
and `CHANGES.md`. Then each release is tagged `<id>-v<version>` (`dereth-plugin release` makes the
right tag), the workflow finds the plugin by its id, and every plugin is listed in the same
`plugins.json`: one source for all of them.

## A source of other repositories' releases

A source need not hold its zips. An index, run by a group or a guide site, can list plugins whose
zips are in their authors' own releases:

```text
curl -LO https://github.com/someone/prices/releases/download/v1.0.4/io.github.someone.prices-1.0.4.zip
dereth-plugin add plugins.json io.github.someone.prices-1.0.4.zip \
  https://github.com/someone/prices/releases/download/v1.0.4/io.github.someone.prices-1.0.4.zip
```

`add` reads the zip for what its entry says (id, version, API version, permissions, web domains,
SHA-256 and size), so the list matches the file players will download. The launcher downloads
release assets from GitHub, but a browser cannot (GitHub sends no cross-origin header for them), so
such an index works for the desktop and not for the web client.

## Without GitHub

Any host that serves files over HTTPS works: pack the plugin, list it, and upload the zip and
`plugins.json` together.

```text
dereth-plugin pack xp-tracker site/io.github.you.xp-tracker-0.2.0.zip
dereth-plugin add site/plugins.json site/io.github.you.xp-tracker-0.2.0.zip io.github.you.xp-tracker-0.2.0.zip --name "XP Tracker"
```

and upload `site/`. The list's address is wherever `plugins.json` lands; the zip's path is read
beside it.
