# Deploying the web client

The web client is static files: the pages and scripts in `www/`, and three WebAssembly modules, each
with its bindings: the launcher's in `www/pkg-front/`, Dereth's in `www/pkg/` with the Horizon
interface's art beside it in `www/pkg/horizon/`, and Auberean's in `www/pkg-auberean/`. The page
loads the launcher's when it opens and a game's only when a world of that game is played, so a
visit to the launcher fetches neither game and a play fetches only its own.
Any static host serves it (Cloudflare Pages, GitHub Pages, nginx, an object store). It has no
server side of its own, and it never hosts a player's data files.

## Build

```text
cargo xtask web --build-only
```

This builds `dereth-web-front`, `dereth-web` and `auberean-web` for `wasm32-unknown-unknown` with
the `web-release` profile, writes each one's bindings into its folder, copies the Horizon
interface's art into `www/pkg/horizon/`, and
copies the launcher's art the front page wears (both realms' paintings and portals, the emblems,
and the Cinzel and EB Garamond typefaces with their licences) from `launcher/assets/` into
`www/assets/`, and copies
the launcher's own page from `launcher/ui/` into `www/` (`app.js`, `style.css`, and its `index.html`
as `launcher.html`), all of which git ignores. It needs:

- the target, installed once: `rustup target add wasm32-unknown-unknown`;
- **zig 0.15 or later** on the path (or named by the `ZIG` environment variable). The plugin
  engine is the Luau VM, which is C++; the build compiles it for WebAssembly with zig's clang,
  with WebAssembly exception handling, against zig's copies of the C++ library headers, and links
  it with a small C and C++ runtime built the same way from the LLVM sources zig ships. The build
  runs without a compiler cache (`RUSTC_WRAPPER` is cleared for it), which cannot run zig.

Publish the whole `www/` folder, `pkg/` and `assets/` included:

| File | Bytes | Gzipped |
|---|---:|---:|
| `pkg-front/dereth_web_front_bg.wasm` | about 1.2 MB | about 0.45 MB |
| `pkg-front/dereth_web_front.js` | about 45 kB | about 8 kB |
| `pkg/dereth_web_bg.wasm` | about 22.9 MB | about 8.0 MB |
| `pkg/dereth_web.js` | about 190 kB | about 27 kB |
| `pkg-auberean/auberean_web_bg.wasm` | about 10.3 MB | about 3.7 MB |
| `pkg-auberean/auberean_web.js` | about 135 kB | about 23 kB |
| `pkg/horizon/atlas-1-0.png`, `atlas-2-0.png`, `atlas-3-0.png` | 1.2, 4.4 and 9.0 MB | the same (PNG is compressed) |
| `pkg/horizon/font-0.png` to `font-5.png` | about 3.4 MB in all | the same |
| `pkg/horizon/fonts.json`, `pkg/horizon/manifest.json` | about 2.4 MB and 39 kB | about 0.35 MB |
| `assets/background/dereth/*.webp` (ten paintings), `assets/background/auberean/*.webp` (six), `assets/backgrounds.json` | about 7 MB | the same (WebP is compressed) |
| `assets/fonts/Cinzel-var.ttf`, `EBGaramond-var.ttf`, `OFL-Cinzel.txt`, `OFL-EBGaramond.txt`; `assets/emblem/dereth.webp`, `network.webp`; `assets/portal/*.webp` | about 1.1 MB | about 0.6 MB |
| `index.html`, `play.js`, `host.js`, `backend.js`, `front-worker.js`, `play-worker.js`, `auberean-worker.js`, `game-loop.js`, `datfiles.js`, `plugin-io.js`, `audio-worklet.js`, `app.js`, `style.css`, `launcher.html`, `web.css` | about 400 kB | about 115 kB |

No file is larger than 25 MiB, the most Cloudflare Pages serves as one file; the packaging refuses
a bundle with a larger one. Each game's module is a file of its own for that reason too.

What a page fetches:

| When | It fetches |
|---|---|
| The page opens | the page's scripts, the launcher's page and module, and the paintings of the realm it opens in |
| A realm is first shown | that realm's paintings |
| A world of Asheron's Call's is played | `play-worker.js`, Dereth's module and the Horizon interface's art |
| A world of Asheron's Call 2's is played | `auberean-worker.js` and Auberean's module |

Each game plays in a worker of its own, made at its first play and kept for its next, each play with
a canvas of its own. A worker is not ended between plays: WebKit does not survive the end of a worker
that drew with WebGPU. So a page that has played both games keeps both modules' memory until it closes.

The plugin host and its Lua engine are about 2.5 MB of Dereth's module (0.8 MB gzipped); reading,
checking and rewriting compiled plugins about 1.2 MB more (0.5 MB gzipped).
`dereth.pluginEngineCheck()` in a worker that has loaded the module answers the engine's self-check,
one line a step; each ends in `ok` in a working build.

The module uses WebAssembly exception handling (in the legacy encoding), which every browser in
the table below runs.

The build names the standard library's and the registry's sources `/rustc` and `/cargo` in the
module, so it carries no path from the building machine.

`cargo xtask web` without `--build-only` also serves the result on `http://127.0.0.1:8080/` for
development. See [Developing](#developing). A release is built differently, from a tagged
commit; see [Releases](#releases).

## Releases

Each release of the web client is a GitHub release of this repository, tagged
`dereth-web-v<version>`. **One release carries both games:** the launcher's module, Dereth's and
Auberean's, with no release of its own for Asheron's Call 2. It takes Dereth's version, which
`cargo xtask release dereth <version>` sets. A version is released for the web under its own tag,
from the same commit as Dereth's release of it or later. Auberean's own version names its desktop
client's releases; the page says which it is playing, and no tag or file name carries it.

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

1. List the repository's releases:
   `GET https://api.github.com/repos/dereth-network/dereth/releases?per_page=100`, newest first, a
   hundred to a page. Every product's releases share the list, so the newest web release can be
   on a later page: when a page does not hold one, read the next, whose address is the
   `rel="next"` entry of the answer's `Link` header (the last page has none).
2. Take the first that is published (`"draft": false`), is not a pre-release
   (`"prerelease": false`, unless the host wants pre-releases), and whose `tag_name` starts with
   `dereth-web-v`. The repository's other releases (`dereth-v…`, `empyrean-v…`) are other
   products, and a web release is never marked as the repository's "Latest", so
   `releases/latest` does not find it.
3. Download its `web.json` asset (`browser_download_url`) and compare `version` with the one
   being served. A host keeps the `web.json` it last installed beside the files for that.
4. If it is newer, download `archive.url`, check its size and SHA-256 against `archive`, unpack
   it, check each file against `files`, and swap the folder being served for the new one in one
   step (a rename or a symbolic link), so no visitor is served half of each. Keep every file on
   a short cache (see [Headers](#headers)).

For example, with `curl` and `jq`, a page at a time:

```text
url="https://api.github.com/repos/dereth-network/dereth/releases?per_page=100"
while [ -n "$url" ]; do
  curl -s -D head.txt -o page.json "$url"
  web=$(jq -r '[.[] | select(.draft == false and .prerelease == false
                       and (.tag_name | startswith("dereth-web-v")))][0]
               | .assets[]? | select(.name == "web.json") | .browser_download_url' page.json)
  if [ -n "$web" ]; then echo "$web"; break; fi
  url=$(tr -d '\r' < head.txt | sed -n 's/^[Ll]ink:.*<\([^>]*\)>; rel="next".*/\1/p')
done
```

A release never carries game data: the bundle is built from an allowlist, and the packaging
refuses any file named like or holding the game's data files, a database or a world, any file
outside the list, any file over 25 MiB, and a module or script that names the building machine's
folders. The Horizon interface's art in `pkg/horizon/` is the client's own, not the game's.
Players' data files stay in their own browsers.

### Deploying to Cloudflare Pages

`cargo xtask deploy web` puts a published release on a Cloudflare Pages project:

```text
cargo xtask deploy web <version>|latest [--project <name>] [--branch <name>] [--zip <file>] [--dry-run]
```

- **Which release:** `latest` is the newest, found as in step 2 above; a version names
  `dereth-web-v<version>`. A draft cannot be deployed: it reads GitHub's public API, which shows
  no drafts.
- **What it does:** downloads `web.json` and the archive, checks the archive's size and SHA-256,
  unpacks it into a fresh temporary folder, checks every file against `files` (and that none is
  over 25 MiB), and runs `wrangler pages deploy <folder> --project-name <name>`, with
  `--branch <name>` when given.
- **`--zip <file>`** deploys a package made locally (`cargo xtask package web`) instead, checked
  against the `web.json` beside it.
- **`--dry-run`** downloads and checks, and prints the command without running it.

It needs:

- **wrangler**, on the `PATH` or through `npx`, signed in with `wrangler login`, or given
  `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` in the environment. xtask never reads them;
  wrangler does.
- **The project's name:** `--project my-pages-project`, or `DERETH_PAGES_PROJECT=my-pages-project`.

### Making a release

The owner of the repository runs releases:

1. Release the version for Dereth as usual (`cargo xtask release dereth <version>`), or pick a
   commit on `main` that carries a version not yet released for the web.
2. Tag it `dereth-web-v<version>` and push the tag. The `Release Dereth (Web)` workflow builds
   the bundle (`cargo xtask package web --tag <tag>`), checks it against its manifest, and drafts
   the release with Dereth's notes for the version.
3. Review the draft and publish it. Hosts that follow the releases pick it up.

`cargo xtask ship` (the network's [`RELEASING.md`](../../network/RELEASING.md), "Preflight, then
ship") does the first two steps: with each Dereth release it ships, it tags
`dereth-web-v<version>` on Dereth's release commit and pushes the tag. A change to the web client
alone, to Auberean's module or to what it is built from is a reason for a Dereth release, since
that one release carries both games. Its
preflight builds the bundle beforehand as a release candidate, and `cargo xtask preflight play
--web` serves it on `127.0.0.1` with the world list of the dev hub it tries the release on.

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
  All three modules are served alike: `www/_headers` needs nothing for any one of them.
- **No cross-origin isolation.** The client uses no `SharedArrayBuffer`, so it needs no
  `Cross-Origin-Opener-Policy` or `Cross-Origin-Embedder-Policy`.
- **Caching.** Every file keeps its name from build to build: the page, its scripts, the module
  and the art in `pkg/`. Give all of them `Cache-Control: no-cache`, which revalidates each load
  (an unchanged file costs a short "not modified" answer). Without it, after a deploy a browser
  may run an old script beside a new page, script or module, and the page fails to start.
- **Missing files.** A host that answers a missing file with a page of its own (a single-page
  application's fallback) is fine: the worker checks each of the Horizon interface's files is what
  its name says before it takes it.
- **HTTPS.** Serve the page over `https://`. Browsers give WebGPU, the origin-private file system
  and the clipboard only to a secure page (`https://`, or `http://` on `127.0.0.1`/`localhost`).
- **Content Security Policy,** if you set one, must allow:
  - the module (`script-src 'self' 'wasm-unsafe-eval'`);
  - the workers (`worker-src 'self'`);
  - the page's inline styles (`style-src 'self' 'unsafe-inline'`);
  - the cursor images, which the page makes as `data:` URLs (`img-src 'self' data:`);
  - the connections: the server's `wss://` URL and its status address, the plugin sources
    (Dereth's, `https://plugins.dereth.network`, and any other `https:` host a player adds), and
    `ws://127.0.0.1:*` for players who use `dereth-web-relay`.

`www/_headers` sets these on Cloudflare Pages (other hosts ignore it; set the same headers in
their own configuration):

```text
/*
  X-Content-Type-Options: nosniff
  Referrer-Policy: no-referrer
  Cache-Control: no-cache
  Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' wss: https: https://plugins.dereth.network ws://127.0.0.1:* ws://localhost:*
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

The page opens on the launcher: the desktop launcher's own page, in both its realms, Dereth's and
Auberean's, with its script, stylesheet, shell, paintings, emblems and typefaces, and with its own
logic run in the browser (the launcher's module carries it). What a browser cannot do, the page
leaves out because the browser's backend says it does not offer it. Pick a world or type an address,
choose or add the data files, and play. Its screens are the desktop's, by the same names:

- **Play** (Home): the favourites, then the combinations played recently, each a world and an
  account with its data files, and PLAY.
- **Worlds**: the list in three sections (the player's own, online, offline), with search, a
  PvE/PvP filter, rows that open in place to what each world says about itself and how the
  browser reaches it, and **Add world**.
- **A world's page**: the account and password, the Modern and Classic data sets it takes, how it
  is reached, its updates kept here (with **Remove updates** and **Refuse updates**), the link to
  send a friend (`https://dereth.network/join#host=…&port=…&name=…`, for a world with a game
  address), and PLAY.
- **Accounts**: the account names used on each world, with **Forget** and **Forget all accounts**.
  The page keeps no password. The page's form is marked for the browser's password manager
  (`autocomplete="username"` and `"current-password"`), which offers to keep the password when
  the player plays, and where the browser lets a page ask (`navigator.credentials`, in Chrome and
  Edge) the page offers the password to it and asks for it back when Home's PLAY needs one.
- **Plugins**: Installed, Available and Sources (see [Plugins](#plugins)).
- **Library**: the client (its version, the interface it starts in, and the render scale), the
  Modern and Classic data sets kept in this browser (**+ Add data files**, **Set default**,
  **Delete**), the world updates kept, and the storage used and whether the browser keeps it.
- **The first visit** asks which interface to play in, as the desktop launcher's first start
  does, and writes the answer into the client's preferences, kept in this browser; then, with no
  data files kept yet, it asks for them.

The address may open a screen directly: `#worlds`, `#worlds=<slug>` (that world's row open),
`#world=<slug>`, `#accounts`, `#plugins`, `#library`, `#first-run` and `#interface`. At phone width
the rail is a bar along the top.

What the desktop launcher has that a browser cannot (hosting a world, building one, the retail
client, downloading the launcher's programs, Discord, a gamepad, a desktop shortcut) is not on the
page.

**Auberean's realm** shows the worlds of Asheron's Call 2, its accounts, and its Library: the data
sets kept in this browser and their check. A world's page takes an account, a password and a set,
and says how the world is reached. A world with a WebSocket endpoint (Yalain's `[server.websocket]`)
is played directly; a world reachable only by UDP says it can't be played in a browser, and links
to the desktop client. `dereth-web-relay` carries two ports and a world of Asheron's Call 2's has
four, so the relay is no way to one.

What the launcher keeps (its state record: the accounts, favourites, recent combinations, the
servers added and the data sets) is kept in the browser's local storage, never sent anywhere.

- **The worlds** are the world list (the Dereth Network hub's, `/v1/servers`, which holds the
  community list, `acresources/serverslist`'s `Servers.xml`, and the worlds registered at the
  hub, or another hub's that the `hub` parameter names; that `Servers.xml` itself when the hub
  does not answer with its list; or the `Servers.xml` the `list` parameter names, read alone;
  fetched from the browser and kept for a day in its local storage, with where it was read from,
  so that a page opened with other parameters reads its own list at once), the launcher's table
  of worlds that run a client of their own (their logon version, rules, era and files), and the
  servers the player adds by address: `host:port`, a `wss://` URL, or an Empyrean status address
  (`https://…/status`).
- **What the client is told about the world** (its era, the set its world is drawn from, its
  systems, its logon version and the rules its client played by) is the launcher's one
  description of it: the words the desktop launcher puts on the client's command line, read by the
  client in the browser with the same parser. They come from the list, the table and the world's
  status document when it has one, else what an Empyrean world told the hub when the hub asked it
  (the hub's `statusPing`: its era, systems and version, which the desktop launcher asks the
  world itself over UDP). A world plays the era it says, else the one the table gives it, else
  the end of retail, which the client is not told and plays from the later files; the player
  chooses neither the era nor its systems. An Empyrean world, or one whose software is not named,
  that has said nothing to either shows its era as unknown.
- **Whether a world is up**, and how many are on, is its status document's word when it has one;
  else what it told the hub when asked; else, while the world list shown is the hub's, the hub's
  word from the world's own heartbeat (any world linked to the Dereth Network, ACE worlds with the
  network's mod among them), read from the hub's list about once a minute while the page shows its
  worlds and taken until the answer or the heartbeat expires, by the desktop launcher's rules. A browser cannot ask a server over UDP whether it is
  up, so any other world is shown with its state unknown. A list the `list` parameter names, and
  the community's own list read when the hub does not answer, bring no status.
- **Reaching a world.** The browser connects directly to a world whose WebSocket endpoint it
  knows: the one the hub's list names for a world linked to the Dereth Network (an Empyrean world
  with its endpoint on tells its hub), or the one a server the player added names (its `wss://`
  URL, or its status document's). Whoever names it, the page follows a `wss://` URL to any host
  and a `ws://` one only to this machine. Any other world (ACE, GDLE, ClassicACE, or an Empyrean
  without the endpoint, whose operator runs no web relay beside it) can't be played in a browser:
  its page says so, with links to the desktop client and to
  [`dereth-web-relay`](../../tools/web-relay/README.md) for anyone who runs it by hand. The page
  never starts or offers a relay of its own. Worlds reached directly are listed first. A server the
  player added that the hub's list names too, by its status address or its WebSocket URL, is shown
  once, as the hub's world.

## The picture

The page is the game's screen. In full screen (the Client Options page's Full Screen box, Horizon's
switch, or Alt+Enter in the world) the game draws at the page's own pixels, the device's rather than
the page's scaled ones, and follows the page as the window is resized or zoomed or moves to another
screen. Full screen is on at first in a browser, and a profile kept from before it filled the page
is turned on once.

- **Render scale.** The Library's Render scale draws a share of those pixels, from all of them
  down to half, and the browser enlarges the picture to fill the page: faster on a large or
  high-density screen, and softer. Every interface's Client Options page has it too, under Display,
  and a change there is drawn at once. The browser keeps the last choice for the next visit.
- **Out of full screen** the game draws at the resolution chosen in its options, and the page scales
  it to fit. The resolutions offered are the standard ones that fit the page, and the page's own
  size.
- **The modern and classic interfaces** draw their screens before the world at the fixed 800 by 600
  they were made for, scaled to fit the page, as they keep that size in a window on the desktop; in
  the world they fill the page. Horizon fills it from its first screen.
- **Never smaller than 800 by 600.** A smaller page, or a render scale that would make the picture
  smaller, is drawn at that size in the page's shape and scaled down to fit.

## Plugins

The Plugins page's **Add Plugin Folder** copies a plugin's folder (the one holding its
`plugin.toml`) into the browser's private storage for this site, and **Add Plugin Zip** keeps a
plugin's zip there as it is; each is listed under Installed as Added, with **Remove**, and
**Remove all** deletes every plugin kept. A compiled plugin (a
WebAssembly component, `docs/plugins/wasm.md`) runs in the browser's own WebAssembly, in the page's
worker. Kept plugins load when the client starts: each is on the first time the client finds it,
and stays as the player leaves it in the plugin panel (`/plugin`) after that.
Their stores are kept in the same private storage, with the client's settings.

**Plugin sources.** The page installs plugins from plugin sources (`docs/plugins/sources.md`) as
the desktop launcher does, with the launcher's own code: from Dereth's own source, which is not
part of the web release but has a host of its own, `https://plugins.dereth.network/`
([Deploying Dereth's plugins](#deploying-dereths-plugins)), and from the sources the player adds by
address, once the player confirms it. The Plugins page lists, as the desktop launcher's does, the
plugins installed, the ones the sources offer, and the sources, each plugin with what it does and
uses, and **Install**, **Update** or **Remove**. Every source is read
as the page opens and again on **Refresh**, and updates apply as the page's **Automatically update
plugins** setting says. Installing fetches the plugin's zip from its source's host and keeps it only when it
is the release the list names (its size, its SHA-256, and a manifest that says what its entry
says). The sources, the last list each gave and the setting are kept in the origin's private
storage beside the client's settings file (`plugin-sources.json`); each installed zip is kept with
the added plugins as `plugins/<id>.zip`, with the record of what was installed
(`plugins/installed.json`), so **Remove all** removes them too.

The page's policy admits any `https:` host (`connect-src`), but a host must also let any page read
its files (`Access-Control-Allow-Origin: *`): Dereth's does, as do GitHub Pages, GitLab Pages,
`raw.githubusercontent.com` and Cloudflare Pages with a `_headers` file. A page cannot tell a host
that does not from one that is offline; the page says it could not read that source, and the rest
work. GitHub's release downloads never let a page read them, so a plugin whose zip is one is left
to the desktop launcher.

Nothing about plugins leaves the player's browser but the requests for the lists and zips of
Dereth's source and the sources the player added, each to its own host.

### Deploying Dereth's plugins

`cargo xtask deploy plugins` puts Dereth's plugin source on a Cloudflare Pages project of its own:

```text
cargo xtask deploy plugins [--project <name>] [--force] [--dry-run]
```

- **What it deploys:** the source as `cargo xtask package plugins` builds it (`plugins.json`, and
  each plugin's zip at `<id>/<id>-<version>-<hash>.zip`) from the folders of `dereth/plugins/` that
  `dereth/plugins/published.toml` names, saying which it holds back, and a `_headers` file: every file readable from any page (`Access-Control-Allow-Origin: *`), the
  list kept five minutes, and each zip kept a year as immutable. A zip's name carries its version
  and the start of its SHA-256, so changed bytes are always a new name.
- **From what:** a checkout of a commit `main` has. It refuses uncommitted changes to tracked
  files, and a `HEAD` that is not an ancestor of `origin/main` (fetch first if it was pushed from
  elsewhere). **`--force`** deploys a branch's commit instead, to try plugins before they reach
  `main`; the next deploy from `main` replaces it.
- **Where:** `--project <name>`, or `DERETH_PLUGINS_PAGES_PROJECT`. wrangler deploys it as
  `deploy web` does, with its own login. `--dry-run` builds and stages it and prints the command.

Setting it up, once (the owner's): make a Cloudflare Pages project for it (for example
`dereth-plugins`, with no Git connection: deploys come from this command), add the custom domain
`plugins.dereth.network` to it, and check that `https://plugins.dereth.network/plugins.json`
answers once the first deploy is done. Deploy again whenever a published plugin changes its version, or
`published.toml` names another. Plugins are experimental: nothing is deployed until the owner decides.

## The player's data files for Auberean

A play of Asheron's Call 2 reads four of an install's files: `portal.dat` (1.5 GB), `cell_1.dat`
(220 MB) and `local_English.dat` (68 MB), which it needs, and `highres.dat` (1.7 GB), the
high-resolution pictures, when the player has it: 1.8 GB, or 3.5 GB with the pictures. The rest of
an install is not read, and the page does not keep it. The player adds them in Auberean's Library
or on a world's page, as a set kept in the browser's private storage in `auberean/sets/<name>/`, or
picks them for one visit. Files that lack one of the needed three are refused before any is copied.
A browser keeps no updates for these worlds: what a world sends is kept for the session in memory,
as the desktop client keeps it. Auberean's own files (its preferences and key maps) are kept apart
from Dereth's, in `auberean/settings.bin`.

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
during login. The page keeps each world's patch in an overlay of its own (the page calls it the
world's updates) in the browser's private storage, as the desktop client keeps it in a folder per
server, and never writes the player's data files:

- **One folder per world**, `overlays/<host>-<port>`, named by the world's address as the desktop
  client names its per-server folder: the world's game address for a listed world or one added by
  `host:port`, and the URL's host and port for one added by URL.
- **Read at every start.** The overlay is laid over the data files when the client starts, and
  after a patch the client reads the patched records at once. A second visit downloads nothing the
  world already sent.
- **Persistent storage.** Playing or adding files asks the browser to keep the page's files
  (`navigator.storage.persist()`). A browser that declines may clear them when it runs short of
  space; the Library says which, and how much of the browser's allowance is used.
- **When the storage is full,** the browser refuses the write: the record is not kept, its revision
  is not recorded, and the world offers it again on the next visit. Adding data files that do not
  fit says so and keeps nothing of them.
- **Removing and refusing.** The world's page removes its overlay, or refuses it: a refused world is
  on the client's overlay blocklist (kept with its settings), its overlay is not read, and its
  updates are not kept. The Library lists every overlay kept, with its size.
- **One tab at a time.** A file in the private storage is open in one tab at a time. A second tab
  of the page plays with no overlay, and tells the server it keeps none.

The client tells the server it keeps an overlay only when it does, so a server that patches its
own way patches a page that keeps none as it would a retail client.

## The Horizon interface

The player chooses it as on the desktop (the Interface option, or `[UI] Interface=Horizon`). The
module does not carry its art: the worker fetches the files in `pkg/horizon/` from beside the
module (same origin, so the policy's `connect-src 'self'` covers it) from when the page opens, and
the client does not wait for them. Until they are in, a choice of Horizon waits and the chat says
the art is loading; a player whose saved interface is Horizon starts in the modern one and is
switched to Horizon once the art is in. If the files could not be fetched, the page's log says
what failed and choosing Horizon is refused with the chat saying why. The modern and classic
interfaces work as ever throughout.

## The classic interface

The player chooses it as on the desktop (the Interface option, or `[UI] Interface=Classic` in the
client's preferences), and needs the older `portal.dat` among the picked files. Its text is drawn
with the Liberation fonts the module carries (Serif, Mono and Sans, under the SIL Open Font
Licence), sized and spaced as the Windows desktop's, so a host serves no fonts and needs no header
for them.

## What a browser's Auberean leaves out

- **Friends.** The friends service is a TCP stream, and a browser cannot open one, so the player is
  without friends, and at each entry the game says it could not reach the service.
- **Lens flares,** on WebGPU: the device cannot look for what stands in front of a flare there, so
  flares are not drawn. On WebGL 2 they are.
- **Language.** The strings are the English ones.

## Connecting to a server

A world is played at its WebSocket URL:

- **An Empyrean or Yalain server with its WebSocket endpoint on** (its `[server.websocket]`,
  described in its `SETUP.md`). The hub's list names the URL for a world linked to the network; otherwise the
  player adds the world by its `wss://` URL, or its status address (`https://…/status`), and the
  page reads the WebSocket URL from the status. The server must list the page's origin in
  `allowed_origins`.
- **Any other server** (ACE, GDLE, or an Empyrean with the endpoint off), when its operator runs
  `dereth-web-relay --public` beside it, for every player, behind TLS: its `wss://` URL is then
  used as an Empyrean's is, and the hub lists it for a world that announces it
  ([the relay's README](../../tools/web-relay/README.md#beside-your-server-operators)). Without
  one the world can't be played in a browser. A player may still run `dereth-web-relay` on their
  own machine by hand and add the world by `ws://127.0.0.1:9180/`; the page offers nothing for it.

The page refuses plain `ws://` to anything but this machine: the game protocol is not encrypted,
and the login carries the password.

## The page's address

The query string may fill the form, so an operator can link players to the page with the server
chosen. The page takes only these:

| Parameter | What it does |
|---|---|
| `server=<url>` | adds that server (a `wss://` URL or a status address) and chooses it |
| `account=<name>` | fills the Account field |
| `list=<url>` | reads the world list from another address, a `Servers.xml` read alone |
| `hub=<url>` | reads the world list from another hub, its `/v1/servers` address, in place of the Dereth Network hub's (a hub run to try a release on) |
| `log`, `log=<level>` | shows the client's log and its frame rate over the canvas, and sets how much of the log reaches the browser console (`info` by default; `debug`, `trace`, `warn`, `error`), for reporting a problem |
| `gpu=webgl` | draws with WebGL 2 even where the browser has WebGPU, for a browser whose WebGPU misbehaves |
| `dats=http` | offers the dev runner's data files (development only; see below) |
| `echo=1` | on a page served from this machine only: posts the log to the dev runner, which prints it |

The password is never taken from the address. The page remembers the servers added, the
accounts, the data sets chosen per world, the favourites and the recent combinations, and the
day's copy of the world list, in the browser's local storage.

## Developing

```text
cargo xtask web [--dev] [--no-build | --build-only] [--port 8080] [--dat-dir <dir>] [--classic-dat-dir <dir>] [--auberean-dat-dir <dir>] [--server <url | host:port>]
```

- **What it does:** builds the client and serves `www/` on `127.0.0.1` only, and prints the page's
  address.
- **`--dat-dir`:** serves the retail data files from that folder to the page, with byte ranges,
  and the printed address selects them (`?dats=http`): the four later files, and `portal.dat` and
  `cell.dat` when the folder holds them. **`--classic-dat-dir`** names another folder for those two.
  **`--auberean-dat-dir`** serves an Asheron's Call 2 install the same way, at `/auberean-dats/`.
- **`--server`:** fills the page's server. A `ws://` or `wss://` URL is used as it is. A
  `host:port` builds and starts `dereth-web-relay` for it on `127.0.0.1:9180`, and stops it on
  Ctrl-C.
- **`--dev`:** a quicker build that keeps function names in stack traces.
- **`&echo=1`:** add it to the address, and the runner prints the page's log in its terminal.
