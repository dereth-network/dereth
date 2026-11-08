# Art pack sources: `art.json`

**Provenance: design.** Art pack sources are this client's own addition, not the original's; every statement here is what Dereth's launcher and its tools do, and what another reader of a source must do.

An **art pack source** is one file, `art.json`, at a stable `https://` address, listing many art
packs, each at its latest version, with where its packed file is, the file's size and its SHA-256.
It is shaped like a plugin source ([plugins/sources.md](../plugins/sources.md)), and anyone can run
one: an artist for their own packs, a group for theirs.

The launcher starts with Dereth's own source, `https://art.dereth.network/art.json`, and nothing
installed. In the Library the player adds other sources by their address, installs, updates and
removes packs, and turns the installed ones on and puts them in an order: the higher one in the
Library's list wins where two change the same thing ([README.md](README.md#several-packs-at-once)).
A pack just installed goes to the top. Installed packs update on their own while the
Library's setting to update added components is on, and wait for the player's Update otherwise.

## An example

```json
{
  "format": 1,
  "name": "Someone's packs",
  "packs": [
    {
      "id": "com.example.holtburg-houses",
      "name": "Holtburg houses",
      "version": "1.2.0",
      "description": "Holtburg's cottages and their roofs, made again.",
      "authors": ["Someone"],
      "license": "CC-BY-4.0",
      "homepage": "https://example.com/holtburg-houses",
      "made_for": ["end-of-retail", "1999"],
      "file": "com.example.holtburg-houses/com.example.holtburg-houses-1.2.0-5a9a144cc888.dat",
      "sha256": "5a9a144cc888df4685629b1adff34bd22c1dcc6cb28d0ffc19f5d27fcbe2fe64",
      "size": 48213504,
      "hash": "1a2b3c4d5e6f7a8b",
      "notes": "- New roofs.",
      "released": "2026-11-08"
    }
  ]
}
```

## The list

| field | | what it is |
|---|---|---|
| `format` | required | the format's version, the integer `1`. A reader refuses a list of a higher format whole ("update Dereth"), and one with none. |
| `name` | optional | the source's name, as the launcher shows it; else the address's host. |
| `packs` | required | the packs, one entry each. |

A key a reader does not know is ignored, so fields can be added without a new format.

## An entry

| field | | what it is |
|---|---|---|
| `id` | required | the pack's id, as its manifest's `[pack] id`: lowercase words of letters, digits and `-`, joined by dots, at least two words, at most 128 bytes. Use your own domain reversed (`com.example.holtburg-houses`). |
| `version` | required | `MAJOR.MINOR.PATCH`. |
| `file` | required | where the packed file is: an `https://` URL, or a path relative to the list's own address (`/` at its start for the site's root). |
| `sha256` | required | the file's SHA-256, 64 hexadecimal digits. |
| `size` | required | the file's size in bytes, from 1 to 2,147,483,648 (2 GiB). |
| `name` | optional | the pack's name; else its id. |
| `description`, `authors`, `license`, `homepage` | optional | as the manifest's `[pack]` keys. |
| `made_for` | optional | the data its ids are chosen from: `end-of-retail`, `1999`. Other values are dropped. |
| `hash` | optional | the hash of the folder the pack was packed from, as the packed file names it. |
| `notes` | optional | what changed in this version, for the player. |
| `released` | optional | the day it was published, `YYYY-MM-DD`. |

An entry that does not read is set aside with the reason, and the others are kept. An id listed
twice keeps its first entry.

## How a download is checked

The launcher downloads a pack's file to a file beside where it is kept, hashing it as it comes,
after checking the disk has room. A download that stops part way goes on from where it stopped the
next time, when the host answers ranges (`Range: bytes=<n>-`). The file is taken only when:

1. its size and SHA-256 are the entry's;
2. it is a packed art pack, of a layout and manifest format this client reads;
3. the pack in it says what the entry says: its manifest's id and version where it names them,
   and the hash of what it was packed from where the entry names one. A pack whose manifest names
   neither id nor version is taken only by that hash.

Anything else is refused, and the version installed before, if any, stays. The client checks each
record of the file against its own SHA-256 as it reads it, and refuses a damaged pack whole.

A pack updates only from the source it was installed from, and only to a higher version. Ids under
`network.dereth.` install only from Dereth's own source.

## Publishing your own source

1. **Make each pack** and give its manifest's `[pack]` an `id` under your own domain and a
   `version` ([README.md](README.md#the-manifest)). Pack it with `dereth-art pack`.
2. **Host the files over HTTPS**, each at a name of its own for each version and content: the
   name Dereth's own source uses is `<id>/<id>-<version>-<the first 12 digits of its SHA-256>.dat`,
   so a changed file never takes an old one's name, and a host can keep each a year as immutable.
3. **Write `art.json`** beside them, each entry's `file` relative to it, with the size and SHA-256
   of the file as hosted. Keep the list itself short-lived in caches (a few minutes), so players
   see a new version soon. Serve every file readable from any page
   (`Access-Control-Allow-Origin: *`), so a web client can read it too.
4. **Tell players the address** of your `art.json`. They add it in the Library.

To publish a new version, raise `version`, pack it again, put the new file beside the old and
change its entry. A player who has the pack is offered the update.

Some hosts serve no file larger than a limit (Cloudflare Pages, 25 MiB). A larger pack's `file`
can be an `https://` URL anywhere else: an object store, a release download.

## Dereth's own source

Dereth's packs are listed at `https://art.dereth.network/art.json`, its front page this guide. It
is built with `cargo xtask package art`, from a folder of packed packs and an optional
`art-source.toml` naming the source, the files published and, for a pack packed before its
manifest named them, its listing keys; and put on its host with `cargo xtask deploy art`. A pack
too large for that host is listed under the address `art-source.toml` names as `files_url`, and
uploaded there first.
