# Deploying the web client

The web client is static files: the pages and scripts in `www/`, and the WebAssembly module and its
bindings in `www/pkg/`. Any static host serves it (Cloudflare Pages, GitHub Pages, nginx, an object
store). It has no server side of its own, and it never hosts a player's data files.

## Build

```text
cargo xtask web --build-only
```

This builds `dereth-web` for `wasm32-unknown-unknown` with the `web-release` profile and writes
the bindings into `www/pkg/`. It needs the target installed once
(`rustup target add wasm32-unknown-unknown`). Publish the whole `www/` folder, `pkg/` included:

| File | Bytes | Gzipped |
|---|---:|---:|
| `pkg/dereth_web_bg.wasm` | about 12.6 MB | about 4.4 MB |
| `pkg/dereth_web.js` | about 135 kB | about 22 kB |
| `index.html`, `play.js`, `front.js`, `play-worker.js`, `datfiles.js`, `audio-worklet.js` | about 76 kB | about 22 kB |

The build names the standard library's and the registry's sources `/rustc` and `/cargo` in the
module, so it carries no path from the building machine.

`cargo xtask web` without `--build-only` also serves the result on `http://127.0.0.1:8080/` for
development. See [Developing](#developing). A release is built differently, from a tagged
commit; see [Releases](#releases).

## Releases

Each release of the web client is a GitHub release of this repository, tagged
`dereth-web-v<version>`. The web client is the Dereth client in a browser, so it carries Dereth's
version: the one the client and the launcher carry, which `cargo xtask release dereth <version>`
sets for all three. A version is released for the web under its own tag, from the same commit as
Dereth's release of it or later.

A release carries three files:

| File | What |
|---|---|
| `dereth-web-<version>.zip` | the files to serve, under one folder `dereth-web-<version>/`: exactly the files listed in [Build](#build) and `_headers` |
| `web.json` | the release's manifest (below) |
| `SHA256SUMS` | the SHA-256 of both, for `sha256sum -c` |

**`web.json`** is the machine-readable description of the release:

```json
{
  "schema": 1,
  "product": "dereth-web",
  "version": "0.1.3",
  "tag": "dereth-web-v0.1.3",
  "prerelease": false,
  "commit": "<the full commit id>",
  "commit_time": "2026-10-05T07:00:00Z",
  "build_number": "<the commit count>",
  "source_url": "https://github.com/dereth-network/dereth",
  "entry": "index.html",
  "archive": {
    "file": "dereth-web-0.1.3.zip",
    "root": "dereth-web-0.1.3",
    "size": 4411007,
    "sha256": "<hex>",
    "url": "https://github.com/dereth-network/dereth/releases/download/dereth-web-v0.1.3/dereth-web-0.1.3.zip"
  },
  "size": 12807592,
  "files": [
    { "path": "index.html", "size": 3785, "sha256": "<hex>" },
    { "path": "pkg/dereth_web_bg.wasm", "size": 12638359, "sha256": "<hex>" }
  ]
}
```

`files` lists every file of the bundle (paths below `root`), and nothing else is in the archive.
A consumer refuses a `schema` it does not know; fields may be added without changing it.

### Updating a host from the releases

A host, the launcher, or any other tool finds the newest web client like this:

1. List the repository's releases: `GET https://api.github.com/repos/dereth-network/dereth/releases`
   (newest first; page with `?per_page=100&page=2` and on).
2. Take the first that is published (`"draft": false`), is not a pre-release
   (`"prerelease": false`, unless the host wants pre-releases), and whose `tag_name` starts with
   `dereth-web-v`. The repository's other releases (`dereth-v…`, `empyrean-v…`) are other
   products, and a web release is never marked as the repository's "Latest", so
   `releases/latest` does not find it.
3. Download its `web.json` asset (`browser_download_url`) and compare `version` with the one
   being served. A host keeps the `web.json` it last installed beside the files for that.
4. If it is newer, download `archive.url`, check its size and SHA-256 against `archive`, unpack
   it, check each file against `files`, and swap the folder being served for the new one in one
   step (a rename or a symbolic link), so no visitor is served half of each. Keep `pkg/` on a
   short cache (see [Headers](#headers)).

For example, with `curl` and `jq`:

```text
curl -s "https://api.github.com/repos/dereth-network/dereth/releases?per_page=100" \
  | jq -r '[.[] | select(.draft == false and .prerelease == false
                  and (.tag_name | startswith("dereth-web-v")))][0]
           | .assets[] | select(.name == "web.json") | .browser_download_url'
```

A release never carries game data: the bundle is built from an allowlist, and the packaging
refuses any file named like or holding the game's data files, a database or a world, any file
outside the list, any oversized file, and a module or script that names the building machine's
folders. Players' data files stay in their own browsers.

### Making a release

The owner of the repository runs releases:

1. Release the version for Dereth as usual (`cargo xtask release dereth <version>`), or pick a
   commit on `main` that carries a version not yet released for the web.
2. Tag it `dereth-web-v<version>` and push the tag. The `Release the web client` workflow builds
   the bundle (`cargo xtask package web --tag <tag>`), checks it against its manifest, and drafts
   the release with Dereth's notes for the version.
3. Review the draft and publish it. Hosts that follow the releases pick it up.

Without GitHub Actions, `cargo xtask publish web <version>` makes the same release from
`cargo xtask package web` run locally ([CONTRIBUTING.md, "Releasing without GitHub
Actions"](../../CONTRIBUTING.md#releasing-without-github-actions)).

Running the workflow by hand is a dry run: the same build and checks, the files kept as a
workflow artifact for a week. Locally, `cargo xtask package web` writes the same three files into
`target/package/dereth-web-<version>/` (`--out` names another folder; `--no-build` packages what
`cargo xtask web --build-only` last built). The CI workflow builds the bundle on every push, so a
broken web build is seen before a release.

## Headers

- **Content types.** Serve `.wasm` as `application/wasm` (the module is compiled while it
  downloads, which needs it) and `.js` as `text/javascript`. Most hosts already do.
- **Compression.** Serve `.wasm` compressed (gzip or Brotli); it shrinks to about a third.
- **No cross-origin isolation.** The client uses no `SharedArrayBuffer`, so it needs no
  `Cross-Origin-Opener-Policy` or `Cross-Origin-Embedder-Policy`.
- **Caching.** The files in `pkg/` keep their names from build to build, so give them a short cache
  (`Cache-Control: no-cache` revalidates each load), or a new deploy may meet an old module.
- **HTTPS.** Serve the page over `https://`. Browsers give WebGPU, the origin-private file system
  and the clipboard only to a secure page (`https://`, or `http://` on `127.0.0.1`/`localhost`).
- **Content Security Policy,** if you set one, must allow:
  - the module (`script-src 'self' 'wasm-unsafe-eval'`);
  - the workers (`worker-src 'self'`);
  - the page's inline styles (`style-src 'self' 'unsafe-inline'`);
  - the cursor images, which the page makes as `data:` URLs (`img-src 'self' data:`);
  - the connections: the server's `wss://` URL and its status address, and
    `ws://127.0.0.1:*` for players who use `dereth-web-relay`.

`www/_headers` sets these on Cloudflare Pages (other hosts ignore it; set the same headers in
their own configuration):

```text
/*
  X-Content-Type-Options: nosniff
  Referrer-Policy: no-referrer
  Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' wss: https: ws://127.0.0.1:* ws://localhost:*

/pkg/*
  Cache-Control: no-cache
```

## Browsers

The page needs a browser with WebAssembly, module workers, `OffscreenCanvas`, and WebGPU or
WebGL 2. It has been run in:

| Browser | Drawing | Run |
|---|---|---|
| Safari 27, macOS | WebGPU | logged in, made a character and entered the world |
| Microsoft Edge 154, Windows 11 | WebGPU; WebGL 2 with `?gpu=webgl` | logged in and entered the world on each (the character made on WebGPU) |
| Firefox 156, Windows 11 (headless) | WebGL 2 | logged in, made a character and entered the world |

Chrome shares Edge's engine but has not been run itself.

## The front page

The page opens on a launcher, the desktop launcher's own logic run in the browser (the module
carries it): pick a world or type an address, choose or add the data files, and play.

- **The worlds** are the community list (`acresources/serverslist`'s `Servers.xml`, fetched from
  the browser and kept for a day in its local storage), the launcher's table of worlds that run a
  client of their own (their logon version, rules, era and files), and the servers the player adds
  by address: `host:port`, a `wss://` URL, or an Empyrean status address (`https://…/status`).
- **What the client is told about the world** (its era, the set its world is drawn from, its
  systems, its logon version and the rules its client played by) is the launcher's one
  description of it: the words the desktop launcher puts on the client's command line, read by the
  client in the browser with the same parser. They come from the list, the table, the world's
  status document when it has one, and what the player chose for what the world does not say. A
  browser cannot ask a server over UDP whether it is up, so a world with no status document is
  shown with its state unknown.
- **Reaching a world.** The browser connects directly only to Empyrean servers that have their
  WebSocket endpoint on. Any other server (ACE, GDLE, ClassicACE, or an Empyrean without it) needs
  [`dereth-web-relay`](../../tools/web-relay/README.md) running on the player's machine; the page
  says so on each such world and gives the command line. Worlds reached directly are listed first.

## The player's data files

Every player uses their own Asheron's Call data files (`client_portal.dat`, `client_cell_1.dat`,
`client_local_English.dat`, `client_highres.dat`, and `portal.dat` and `cell.dat` from before
Throne of Destiny), read in their own browser. The player adds them on the front page as named
data sets, each kept in the browser's private storage (the origin-private file system, in
`sets/<name>/`) and opened from there on later visits; files may also be picked for one visit. A
world is offered the sets that report what its files report, as the desktop launcher matches them:
the end of retail's for most worlds, and a world's own files (named in the launcher's table, with
where they come from) for a world that ships them. The classic interface draws from the older
`portal.dat`; a world of an era before Throne of Destiny is drawn from the older pair, with the
later files beside it for the interface.

The files are never uploaded, and a deployment never hosts them. The dev runner's `--dat-dir` (and
`--classic-dat-dir`) stands in for this on a developer's machine, on loopback only.

## A world's overlay

A world that updates its data files (an Empyrean world with `[dat_overlay]`, for one) patches them
during login. The page keeps each world's patch in an overlay of its own in the browser's private
storage, as the desktop client keeps it in a folder per server, and never writes the player's data
files:

- **One folder per world**, `overlays/<host>-<port>`, named by the world's address as the desktop
  client names its per-server folder: the world's game address for a listed world or one added by
  `host:port`, and the URL's host and port for one added by URL.
- **Read at every start.** The overlay is laid over the data files when the client starts, and
  after a patch the client reads the patched records at once. A second visit downloads nothing the
  world already sent.
- **Persistent storage.** Playing or adding files asks the browser to keep the page's files
  (`navigator.storage.persist()`). A browser that declines may clear them when it runs short of
  space; the front page says which, and how much of the browser's allowance is used.
- **When the storage is full,** the browser refuses the write: the record is not kept, its revision
  is not recorded, and the world offers it again on the next visit. Adding data files that do not
  fit says so and keeps nothing of them.
- **Removing and refusing.** The world's page removes its overlay, or refuses it: a refused world is
  on the client's overlay blocklist (kept with its settings), its overlay is not read, and its
  updates are not kept. The front page lists every overlay kept, with its size.
- **One tab at a time.** A file in the private storage is open in one tab at a time. A second tab
  of the page plays with no overlay, and tells the server it keeps none.

The client tells the server it keeps an overlay only when it does, so a server that patches its
own way patches a page that keeps none as it would a retail client.

## The classic interface

The player chooses it as on the desktop (the Interface option, or `[UI] Interface=Classic` in the
client's preferences), and needs the older `portal.dat` among the picked files. Its text is drawn
with the Liberation fonts the module carries (Serif, Mono and Sans, under the SIL Open Font
Licence), sized and spaced as the Windows desktop's, so a host serves no fonts and needs no header
for them.

## Connecting to a server

The world's **Connect through** field takes a WebSocket URL:

- **An Empyrean server with its WebSocket endpoint on** (its `[server.websocket]`, described in
  its `SETUP.md`). Use its `wss://` URL, or its status address (`https://…/status`); the page reads
  the WebSocket URL from the status. The server must list the page's origin in `allowed_origins`.
- **Any other server** (ACE, GDLE, or an Empyrean with the endpoint off). The player runs
  `dereth-web-relay` on their own machine and uses `ws://127.0.0.1:9180/` (the field's default). For
  a page served from your site, the relay must be started with that site's origin:

  ```text
  dereth-web-relay --server <host:port> --allow-origin https://play.example.org
  ```

The page refuses plain `ws://` to anything but this machine: the game protocol is not encrypted,
and the login carries the password.

## The page's address

The query string may fill the form, so an operator can link players to the page with the server
chosen. The page takes only these:

| Parameter | What it does |
|---|---|
| `server=<url>` | adds that server (a `wss://` URL or a status address) and chooses it |
| `account=<name>` | fills the Account field |
| `era=<name>`, `features=<name=true,...>` | the era and systems of the server `server=` names, when its status does not say them |
| `list=<url>` | reads the world list from another address |
| `log`, `log=<level>` | shows the client's log and its frame rate over the canvas, and sets how much of the log reaches the browser console (`info` by default; `debug`, `trace`, `warn`, `error`), for reporting a problem |
| `gpu=webgl` | draws with WebGL 2 even where the browser has WebGPU, for a browser whose WebGPU misbehaves |
| `dats=http` | offers the dev runner's data files (development only; see below) |
| `echo=1` | on a page served from this machine only: posts the log to the dev runner, which prints it |

The password is never taken from the address. The page remembers the servers added, the account
and the data sets chosen per world, and the day's copy of the world list, in the browser's local
storage.

## Developing

```text
cargo xtask web [--dev] [--no-build | --build-only] [--port 8080] [--dat-dir <dir>] [--classic-dat-dir <dir>] [--server <url | host:port>]
```

- **What it does:** builds the client and serves `www/` on `127.0.0.1` only, and prints the page's
  address.
- **`--dat-dir`:** serves the retail data files from that folder to the page, with byte ranges,
  and the printed address selects them (`?dats=http`): the four later files, and `portal.dat` and
  `cell.dat` when the folder holds them. **`--classic-dat-dir`** names another folder for those two.
- **`--server`:** fills the page's server. A `ws://` or `wss://` URL is used as it is. A
  `host:port` builds and starts `dereth-web-relay` for it on `127.0.0.1:9180`, and stops it on
  Ctrl-C.
- **`--dev`:** a quicker build that keeps function names in stack traces.
- **`&echo=1`:** add it to the address, and the runner prints the page's log in its terminal.
