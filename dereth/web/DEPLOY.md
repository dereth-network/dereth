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
| `pkg/dereth_web_bg.wasm` | about 10.0 MB | about 3.5 MB |
| `pkg/dereth_web.js` | about 130 kB | about 22 kB |
| `index.html`, `play.js`, `play-worker.js`, `datfiles.js`, `audio-worklet.js` | about 31 kB | about 12 kB |

The build names the standard library's and the registry's sources `/rustc` and `/cargo` in the
module, so it carries no path from the building machine.

`cargo xtask web` without `--build-only` also serves the result on `http://127.0.0.1:8080/` for
development. See [Developing](#developing).

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

## The player's data files

Every player uses their own Asheron's Call data files (`client_portal.dat`, `client_cell_1.dat`,
`client_local_English.dat`, `client_highres.dat`), read in their own browser:
- picked once and kept in the browser's private storage (the origin-private file system), then
  opened from there on later visits;
- or picked from disk for one visit.

They are never uploaded, and a deployment never hosts them. The dev runner's `--dat-dir` stands in
for this on a developer's machine, on loopback only.

## Connecting to a server

The page's **Server** field takes a WebSocket URL:

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
| `server=<url>` | fills the Server field (a `wss://` URL, a status address, or the relay's `ws://127.0.0.1:<port>/`) |
| `account=<name>` | fills the Account field |
| `log`, `log=<level>` | shows the client's log and its frame rate over the canvas, and sets how much of the log reaches the browser console (`info` by default; `debug`, `trace`, `warn`, `error`), for reporting a problem |
| `gpu=webgl` | draws with WebGL 2 even where the browser has WebGPU, for a browser whose WebGPU misbehaves |
| `dats=http` | offers the dev runner's data files (development only; see below) |
| `echo=1` | on a page served from this machine only: posts the log to the dev runner, which prints it |

The password is never taken from the address. The form remembers the server, the account and the
data source in the browser's local storage.

## Developing

```text
cargo xtask web [--dev] [--no-build | --build-only] [--port 8080] [--dat-dir <dir>] [--server <url | host:port>]
```

- **What it does:** builds the client and serves `www/` on `127.0.0.1` only, and prints the page's
  address.
- **`--dat-dir`:** serves the four retail data files from that folder to the page, with byte
  ranges, and the printed address selects them (`?dats=http`).
- **`--server`:** fills the page's server. A `ws://` or `wss://` URL is used as it is. A
  `host:port` builds and starts `dereth-web-relay` for it on `127.0.0.1:9180`, and stops it on
  Ctrl-C.
- **`--dev`:** a quicker build that keeps function names in stack traces.
- **`&echo=1`:** add it to the address, and the runner prints the page's log in its terminal.
