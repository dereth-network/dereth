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

`dereth-plugin` comes with Dereth: `dereth-plugin.exe` beside `dereth.exe` on Windows,
`Dereth.app/Contents/MacOS/dereth-plugin` on macOS, and inside the AppImage on Linux
(`--appimage-extract usr/bin/dereth-plugin` takes it out). Each Dereth release also has it as a file
of its own, `dereth-plugin-<version>-<platform>`, which the template's workflows download.

| command | what it does |
|---|---|
| `dereth-plugin new <folder> [--id <id>] [--name <name>] [--github <username>/<repo>]` | a new plugin repository, from the template; in a plugin repository, a new plugin's folder added to it |
| `dereth-plugin check <folder>` | every check a client makes when it loads the plugin, and every script compiled; a folder of plugins checks each one |
| `dereth-plugin pack <folder> <out.zip>` | the plugin as a zip, and `<out.zip>.sha256` beside it; the same folder always gives the same bytes |
| `dereth-plugin release <folder> <version> [--push]` | sets the version, moves the change log's notes under it, commits and tags; pushes with `--push` |
| `dereth-plugin notes <folder> <version>` | that version's notes from the change log |
| `dereth-plugin find <repository> <tag>` | the plugin a release tag names: its folder, id, version and name |
| `dereth-plugin add <plugins.json> <zip> <url> [--notes <file>] [--name <name>]` | lists the zip's release in a source, in place of the plugin's entry; `<url>` is where players download it, an `https://` address or a path beside the list |
| `dereth-plugin latest <repository> <releases.json>` | the zip of each of the repository's plugins' latest GitHub release, one `<tag>` and `<zip>` to a line, to download |
| `dereth-plugin site <repository> <releases.json> <zips> <out> [--name <name>]` | the repository's whole source made from its GitHub releases: `<out>/plugins.json`, and each plugin's zip in `<out>/<id>/` |
| `dereth-plugin hash <zip>` | a zip's id, version and SHA-256 |
| `dereth-plugin build <crate> <folder>` | a Rust plugin built as a component ([getting-started-wasm.md](getting-started-wasm.md)) |

The tool carries Dereth's version, and its rules are those of the client released with it: the
API version a plugin may ask for, the permissions and capabilities it may name, the files a package
may hold. Each Dereth release carries it for every platform it ships, beside the launcher:
`dereth-plugin-<version>-windows-x86_64.zip`, `-linux-x86_64.tar.gz` or `-macos-aarch64.tar.gz`,
listed in the release's `SHA256SUMS`. Unpack it and run
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
  .github/workflows/release.yml  a release tag: packed and released, then Pages started
  .github/workflows/pages.yml    after a release, or a push changing a plugin.toml: the source made
                                 from the releases and deployed to GitHub Pages
  xp-tracker/                    the plugin: exactly what goes in its zip
    plugin.toml                  id io.github.you.xp-tracker, version 0.1.0, homepage the repository
    main.lua                     a window that says hello, and /xptracker
    CHANGES.md                   ## Unreleased
```

The launcher makes the same repository: on its **Plugins** page, **Make Your Own** takes the name
and the GitHub repository and writes it into an empty folder you choose, for the launcher's own
Dereth release. It also puts a copy of the tool it comes with at the repository's root,
`dereth-plugin.exe` on Windows and `dereth-plugin` elsewhere, which the `.gitignore` keeps out of
git. Each time the launcher starts it makes that copy its own tool again, so after a Dereth update
the copy is the new release's. Only the folders Make Your Own wrote in get one: a folder added with
**Add Plugin Folder** does not, and taking a folder off the launcher's list stops the updates and
leaves the copy where it is. `dereth-plugin new` from the command line makes no copy of itself.

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
4. starts the **Pages** workflow on the default branch.

The Pages workflow makes the source afresh from the repository's GitHub releases and deploys it to
GitHub Pages, with no branch to keep: it lists the releases (`gh api`), downloads the zip of each
plugin's latest release (`dereth-plugin latest`), and writes `plugins.json` with each zip in a
folder named for its plugin (`dereth-plugin site`). A plugin's latest release is the highest
version among the releases, not drafts or pre-releases, that hold its zip; each entry carries that
release's notes and day. Deployments run one at a time, and each one makes the whole source again,
so none is lost to another. It runs on the default branch only, and can be run by hand from the
**Actions** tab.

Once, before the first release, turn on GitHub Pages for the repository: **Settings → Pages**, set
**Source** to **GitHub Actions**. (A release made before that fails its Pages run; run it again
afterwards.) The source's address is then `https://you.github.io/xp-tracker/plugins.json`, which the
README gives players. Pages serves the list and the zips to any page, so the web client can read
them too.

A release is a fix forward: the list holds each plugin's latest release, and a launcher offers it to
everyone who installed the plugin from this source. A release that asks for a permission or a web
domain the one before did not waits for each player to accept it, whatever their update setting. A
release that needs a newer plugin API than a player's client is listed but not installable there
until they update Dereth.

## Many plugins in one repository

A repository may hold any number of plugins, each a folder of its own, directly in the
repository, with its own `plugin.toml` and `CHANGES.md`. Then each release is tagged
`<id>-v<version>` (`dereth-plugin release` makes the right tag), the workflow finds the plugin by
its id, and every plugin is listed in the same `plugins.json`: one source for all of them. A plugin
further down than the repository's own folders is not one of its plugins: `check`, `find` and
`release` leave it out or refuse it, and the launcher does not add it.

To add one, run `new` on the repository itself, or choose it in the launcher's **Make Your Own**
(which also puts its copy of the tool there, and the `.gitignore` lines for it the repository
lacks):

```text
dereth-plugin new xp-tracker --name "Loot Helper" --github you/xp-tracker
```

In a plugin repository, `new` writes only the new plugin's folder (`loot-helper/`, with its
`plugin.toml`, `main.lua` and `CHANGES.md`) beside the others, and touches nothing else: the README
and the workflows serve every plugin. It refuses a folder name or an id the repository has already,
and a plugin's own folder. Any other folder that is not empty is refused too, one whose plugins are
all further down among them.

To remove one, delete its folder and push to the default branch. A push that adds, changes or
removes a `plugin.toml` runs the **Pages** workflow, and the source it makes lists only the plugins
the repository holds, so the removed plugin and its zip leave it; its GitHub releases stay.
Launchers that installed it keep it, with no more updates. A repository that holds no plugin at
all, or one whose `plugin.toml` does not read, fails the run, and the source deployed before stays
as it was.

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
