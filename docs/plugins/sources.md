# Plugin sources: `plugins.json`

**Provenance: design.** Plugin sources are this client's own addition, not the original's; every statement here is what Dereth's launcher and its author's tool do, and what another host reading a source must do.

A **plugin source** is one file, `plugins.json`, at a stable `https://` address, listing many
plugins, each at its latest release, with where its zip is and the zip's SHA-256. Anyone can run
one: an author for their own plugins, a group for theirs, a guide site for the ones it recommends.
The launcher and the web client start with Dereth's own source and nothing installed; the player
adds others by pasting their address
([install-and-update.md](guide/install-and-update.md#the-launcher-plugin-sources)).
The author's tool writes and updates the file ([publishing.md](guide/publishing.md)).

The idea is Dalamud's: its plugin repositories are a hosted JSON list a launcher reads, added by
URL behind a warning, with an auto-update setting and a per-plugin override. This format adds a
SHA-256 for every zip and a check of each zip against its entry.

## An example

```json
{
  "format": 1,
  "name": "Someone's plugins",
  "plugins": [
    {
      "id": "io.github.someone.meter",
      "name": "Meter",
      "version": "2.1.0",
      "api": "0.7",
      "description": "Experience earned this session and per hour.",
      "authors": ["Someone"],
      "homepage": "https://github.com/someone/plugins",
      "permissions": ["chat.send"],
      "domains": [],
      "zip": "io.github.someone.meter/io.github.someone.meter-2.1.0.zip",
      "sha256": "5a9a144cc888df4685629b1adff34bd22c1dcc6cb28d0ffc19f5d27fcbe2fe64",
      "size": 48213,
      "notes": "- Counts luminance too.",
      "released": "2026-10-26"
    },
    {
      "id": "io.github.someone.prices",
      "name": "Prices",
      "version": "1.0.4",
      "api": "0.7",
      "permissions": ["net.http"],
      "domains": ["prices.example.org"],
      "zip": "https://github.com/someone/prices/releases/download/v1.0.4/io.github.someone.prices-1.0.4.zip",
      "sha256": "0ba0a1fdbdfd5a1514b3d522e1f3ad1c30af9275aee4163e033900274cf67181"
    }
  ]
}
```

## The list

| field | | what it is |
|---|---|---|
| `format` | required | the format's version, the integer `1`. A host refuses a list of a higher format whole ("update Dereth"), and one with none. |
| `name` | optional | the source's name, as a launcher shows it; else the address's host. |
| `plugins` | required | the plugins, one entry each. |

## An entry

| field | | what it is |
|---|---|---|
| `id` | required | the plugin's id, as its manifest has it ([manifest.md](manifest.md)) |
| `name` | optional | its name; else its id |
| `version` | required | the release's version, SemVer 2 |
| `api` | required | the lowest API version the release needs, `MAJOR.MINOR` |
| `description` | optional | what it does, for the player |
| `authors` | optional | a list of names |
| `homepage` | optional | an address for people |
| `permissions` | optional | the permissions its manifest names, a list; none when absent |
| `domains` | optional | the web domains its manifest names (`[net] domains`), a list; none when absent |
| `zip` | required | where the package's zip is: an `https://` address, or a path relative to the list's own address (`//host/...` and `/path` read as on the web). No other scheme, `http:` included. |
| `sha256` | required | the zip's SHA-256, 64 hexadecimal digits |
| `size` | optional | the zip's size in bytes. A host downloads no more than this, and refuses a zip of another size. |
| `notes` | optional | what changed in this release, for the player (Markdown) |
| `released` | optional | the day it was released, `YYYY-MM-DD` |

**One release per plugin.** A source lists each plugin at its latest release only, and a new
release replaces its entry. There is no list of older releases and no testing channel: a fix is
a newer release. A release whose `api` is newer than a client's is listed there but not
installable ("needs a newer Dereth"), and a plugin installed from that source stays at the release
it has until a release that client runs is listed.

**Growing the format.** A host ignores a field it does not know, so a field is added without a new
format. `format` changes only when an old host would read the new list wrongly.

**Reading leniently.** An entry that does not read (a field missing or of the wrong kind, an id
that is not one, an id in the `dereth.` namespace, a second entry for one id) is set aside with
the reason, and the others are read. The launcher shows how many were set aside.

## Checking a release

A host takes a zip only when all of these hold, and keeps nothing otherwise:

1. **It is the file the entry names:** its size is `size` (when given) and its SHA-256 is `sha256`.
2. **It is a package** whose manifest reads ([packages.md](packages.md)).
3. **Its manifest says what the entry says:** the same `id`, `version` and `api`, the same set of
   `permissions` and the same set of `domains`. So what a launcher shows before the download (what
   the plugin uses) is what the package holds.
4. **The client runs its API version** ([the rule](README.md)).

The launcher and the web client add two rules of their own:

- **Ids under `network.dereth.`** install only from Dereth's own source: an id says nothing about
  who made a plugin, and a plugin from elsewhere claiming one of Dereth's ids would take its place
  in the client, with its stores.
- **An installed plugin updates only from the source it came from**, and only to a release whose
  version takes precedence over the one installed (SemVer's precedence: a pre-release below its
  release, build metadata ignored). Another source listing the same id offers nothing for it.

## Hosting a list

Any static host that serves files over HTTPS will do for the launcher. The web client reads every
source from a page on another host, so a list and its zips need their host to send
`Access-Control-Allow-Origin: *`, as Cloudflare Pages with a `_headers` file, GitHub Pages, GitLab
Pages and `raw.githubusercontent.com` do. A page cannot tell a host that sends none from one that
is offline: the web client says it could not read that source there. GitHub's release downloads
send no such header, so a page cannot download a zip that is a release asset; the web client leaves
such a plugin to the launcher, which can.

The template's release workflow keeps the list and the zips on the repository's `gh-pages` branch,
published by GitHub Pages ([publishing.md](guide/publishing.md)). Dereth's own source has a host of
its own, `https://plugins.dereth.network/plugins.json`: `cargo xtask deploy plugins` builds it from
the samples in `dereth/plugins/` and puts it on its Cloudflare Pages project, with headers that let
any page read it, the list kept five minutes and each versioned zip a year.
