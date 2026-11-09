# Art pack sources: `art.json`

An **art pack source** is a single file, `art.json`, hosted at a stable `https://` address. It lists a set of art packs, each at its latest version, along with where to download the packed file, how big it is, and its SHA-256 hash. Anyone can host one: an artist can publish their own packs, or a group can publish a shared collection.

Out of the box, the launcher knows about Dereth's own source, `https://art.dereth.network/art.json`, and has no packs installed. From the Library, players can:

- add other sources by pasting their address;
- install, update and remove packs;
- turn installed packs on or off, and drag them into order.

When two packs change the same thing, the one higher in the Library's list wins. A newly installed pack goes to the top. If **Automatically update components** is turned on in the Library, installed packs update by themselves; otherwise they wait until the player clicks Update.

## Example

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

## Top-level fields

| Field | Required? | Description |
|---|---|---|
| `format` | Yes | The version of this file format. Currently always the integer `1`. Readers reject a file with no `format`, or with a higher number than they understand (and tell the player to update Dereth). |
| `name` | No | The source's display name in the launcher. If missing, the launcher shows the host name from the address. |
| `packs` | Yes | A list of packs, one entry per pack. |

Readers ignore any keys they don't recognise. That lets new fields be added later without bumping the format version.

## Pack entries

| Field | Required? | Description |
|---|---|---|
| `id` | Yes | The pack's unique ID, matching `[pack] id` in its manifest. It's made of two or more dot-separated words using lowercase letters, digits and hyphens, up to 128 bytes total. Use your own domain name, reversed, as a prefix (e.g. `com.example.holtburg-houses`). |
| `version` | Yes | A version number in `MAJOR.MINOR.PATCH` form. |
| `file` | Yes | Where to download the packed file. Either a full `https://` URL, or a path relative to the `art.json` file itself (start it with `/` to make it relative to the site root). |
| `sha256` | Yes | The file's SHA-256 hash, as 64 hex digits. |
| `size` | Yes | The file's size in bytes, between 1 and 2,147,483,648 (2 GiB). |
| `name` | No | The pack's display name. Defaults to its `id`. |
| `description`, `authors`, `license`, `homepage` | No | Same meaning as the matching keys in the manifest's `[pack]` section. |
| `made_for` | No | Which game data the pack's IDs are based on: `end-of-retail` and/or `1999`. Any other values are ignored. |
| `hash` | No | The hash of the folder the pack was built from, as it appears in the packed file's name. |
| `notes` | No | Release notes for this version, shown to the player. |
| `released` | No | The release date, as `YYYY-MM-DD`. |

If an entry can't be read, the launcher skips it, records why, and keeps the rest of the list. If the same `id` appears more than once, only the first entry counts.

## Publishing your own source

1. **Build each pack.** In its manifest's `[pack]` section, set an `id` based on your own domain and a `version` ([README.md](README.md#the-manifest)). Then pack it with `dereth-art pack`.
2. **Host the files over HTTPS.** Give each version its own unique filename. Dereth's own source uses `<id>/<id>-<version>-<first 12 hex digits of the SHA-256>.dat`. That way a changed file never reuses an old name, and your host can safely cache each file as immutable for a year.
3. **Write `art.json`** in the same place. Make each entry's `file` path relative to it, and fill in the size and SHA-256 of each file exactly as hosted. Give `art.json` itself a short cache lifetime (a few minutes) so players see new versions quickly. Serve all files with `Access-Control-Allow-Origin: *` so the web client can read them too.
4. **Share your `art.json` address** with players. They add it in the Library.

To release a new version: bump `version`, repack, upload the new file alongside the old one, and update the entry. Anyone who has the pack installed will be offered the update.

Some hosts cap file sizes (Cloudflare Pages, for example, has a 25 MiB limit). For larger packs, point `file` at a full `https://` URL somewhere else, such as an object storage bucket or a release download.

## Using a GitHub repository as a source

If you publish a pack through a GitHub repository's releases, you don't need an `art.json` at all. Players add the repository's address (`https://github.com/<owner>/<repo>`) as a source, and the desktop launcher reads its releases through GitHub's API. Each release is one version of one pack:

- **Latest version:** the newest release that isn't a draft or pre-release and is tagged with a version number (`v1.2.0` or `1.2.0`).
- **Files:** the release must contain exactly one `*.dat` asset (the pack), plus a `<file>.sha256` file next to it containing a single line in `sha256sum` format (`<hex>  <file>`). If GitHub also reports a digest for the asset, the two must match.
- **Description and name:** the release body is used as the pack's description. The release title, minus the version number at the end, is used as its name until the pack is installed.
- **ID:** the pack's manifest should set its `id` (and may set `version`, which must match the tag). Until it's installed, the pack is listed as `com.github.<owner>.<repo>`; if the manifest doesn't set an `id`, it keeps that one permanently.

Downloads are verified the same way as described above, except for the source-folder hash, since GitHub releases don't provide one.

The launcher only checks a repository's releases at startup and when the player clicks Refresh. It sends the tag from the previous response (`If-None-Match`), so unchanged results don't count against GitHub's rate limit. Because web pages can't download GitHub release files, the web client only supports `art.json` sources.
