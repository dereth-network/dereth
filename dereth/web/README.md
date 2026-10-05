# Dereth in a web browser

The Dereth client, compiled to WebAssembly: the same application as the desktop client (the retail
interface, the world drawing, the sound), running in a Web Worker and drawing into a canvas with
WebGPU, or WebGL 2 where the browser has no WebGPU.

- **The player's own data files.** The player picks their four `client_*.dat` files, and the
  `portal.dat` and `cell.dat` from before Throne of Destiny for the classic interface and the early
  worlds; the page reads them on the player's machine and can keep a copy in the browser's private
  storage. Nothing is uploaded.
- **Both interfaces.** The retail interface, and the classic one, whose text is drawn with the
  browser's own fonts.
- **One WebSocket to the server.** Either an Empyrean server's own `wss://` endpoint, or
  `dereth-web-relay` (`tools/web-relay`) on the player's machine for a server that speaks only UDP.
  Plain `ws://` is refused for anything but the player's own machine.
- **Static files.** The page, its scripts and the module are served by any static host.

`DEPLOY.md` says how to build and host it, the headers a host must send, and the page's address
parameters. `cargo xtask web` builds it and serves it on `127.0.0.1` for development.

| File | What |
|---|---|
| `www/index.html`, `www/play.js` | the page: the launch form, the canvas, the page's input forwarded to the worker |
| `www/play-worker.js` | the worker: loads the module, opens the data files, carries the WebSocket, runs a frame per animation frame |
| `www/datfiles.js` | the data files' readers: picked files, the browser's copy, or the dev runner's |
| `www/audio-worklet.js` | plays the sound the worker mixes |
| `src/browser.rs` | the module's bindings the worker calls |
| `src/play.rs` | the client shell's application on the browser's host |
| `src/host.rs`, `src/host/` | the browser as the shell's host: canvas, clock and time zone, clipboard, sound, links, and the classic interface's fonts (`host/fonts.rs`) |
| `src/dats.rs`, `src/settings.rs`, `src/server_url.rs`, `src/frame.rs`, `src/keycodes.rs` | the data store over the worker's reads, the client's own files in browser storage, the server URL rule, the WebSocket frame, the key names |
