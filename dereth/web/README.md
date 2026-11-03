# Dereth in a web browser

The Dereth client, compiled to WebAssembly. It is the same application as the desktop client,
running in a Web Worker and drawing into a canvas with WebGPU, or WebGL 2 where the browser has no
WebGPU.

- **The launcher first.** The page is the desktop launcher's Dereth realm, in its own look: Home,
  the worlds, each world's page, accounts, plugins and the library. Pick a world or type an
  address, choose your data files, and play.
- **Your own data files.** You pick your four `client_*.dat` files, plus the older `portal.dat`
  and `cell.dat` for the classic interface and early-era worlds. The page reads them on your
  machine and can keep a copy in the browser's private storage. Nothing is uploaded.
- **All three interfaces:** Horizon, modern and classic.
- **World updates.** A world that patches its data files keeps its patch in the browser, one
  overlay per world, and never writes your files.
- **The page is the screen.** Full screen is on at first and draws at the page's own pixels.
  Render scale, on the front page and in Client Options, draws fewer of them for speed.
- **One WebSocket to the server:** an Empyrean server's own `wss://` endpoint, or the
  `dereth-web-relay` (`tools/web-relay`) its operator runs beside a server that speaks only UDP.
  A world with neither can't be played in a browser. Plain `ws://` is refused for anything but
  your own machine.
- **Static files.** Any static host can serve the page, its scripts and the module.

[`DEPLOY.md`](DEPLOY.md) covers building and hosting it, the headers a host must send, the page's
address parameters, and how each of the above works. `cargo xtask web` builds it and serves it on
`127.0.0.1` for development.

| File | What |
|---|---|
| `www/index.html`, `www/host.js` | the front page: the desktop launcher's own page, run over the browser's backend, and what a browser does its own way |
| `www/app.js`, `www/style.css`, `www/launcher.html` | the desktop launcher's script, stylesheet and shell, copied there by the build and not kept in git |
| `www/backend.js` | the front page's backend: the desktop launcher's commands, answered by the launcher logic in the module and by the browser's storage |
| `www/web.css` | the page's own rules, beside the launcher's stylesheet |
| `www/assets/` | the launcher's art the page wears (Dereth's paintings and emblem, the two typefaces), copied there by the build and not kept in git |
| `www/play.js` | the canvas, and the page's input forwarded to the worker |
| `www/play-worker.js` | the worker: loads the module, opens the data files, carries the WebSocket, runs a frame per animation frame |
| `www/datfiles.js` | reads the data files: picked files, the browser's copy, or the dev runner's |
| `www/audio-worklet.js` | plays the sound the worker mixes |
| `www/_headers` | the headers a host sends with the page |
| `src/browser.rs` | the module's bindings the worker calls |
| `src/front.rs` | the front page's launcher logic, shared with the desktop launcher |
| `src/play.rs` | the client shell on the browser's host |
| `src/host.rs`, `src/host/` | the browser as the shell's host: canvas, clock and time zone, clipboard, sound, links, and the classic interface's fonts |
| `src/dats.rs`, `src/overlay.rs`, `src/settings.rs` | the data files, a world's overlay, and the client's own files in browser storage |
| `src/server_url.rs`, `src/frame.rs`, `src/keycodes.rs` | the server URL rule, the WebSocket frame, the key names |
