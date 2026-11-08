// What a game's worker does whichever game it plays: carries datagrams between the client and the
// server's WebSocket (a server's own endpoint, or dereth-web-relay on this machine), runs one client
// frame per animation frame, plays the sound, shows the interface's cursor and hands the client the
// page's input. A worker's `start` opens its game's files and brings its client up; everything after
// that is here. When the client ends the worker lets go of what it holds in the browser's storage, for
// the launcher's worker and the other game's, and stays for the next play of its game.
//
// A module's client, `play`, answers `frame`, `receive`, `takeMessages`, `takeCopied`, `takeOpenedUrls`,
// `clientSize`, `renderScale`, `cursorChange`, `pointerLock`, `audio`, `backend`, the input calls below,
// `setPage`, `logOff` and `shutDown`, and `will` when it keeps the server a copy of its goodbye.

const log = (text) => postMessage({ log: String(text) });

// `start(msg)` brings the client up and answers it. `each` runs after every frame. `release` closes
// what the worker holds in the browser's storage, before the page is told the client ended.
// `rate()` is the sound's rate, `memory()` the module's memory in MiB.
export function serve({ ready, start, each = () => {}, release = () => {}, rate, memory }) {
  let canvas = null;
  let play = null;
  let socket = null;
  const marks = {};

  // Log off and run the client's cleanup, which saves what it keeps; the goodbye goes out before
  // the socket closes.
  function shutDown() {
    if (!play) return;
    play.logOff();
    const goodbye = play.takeMessages();
    if (socket?.readyState === WebSocket.OPEN) {
      for (const m of goodbye) socket.send(m);
      log(`logged off (${goodbye.length} datagram(s))`);
    }
    try {
      play.shutDown();
    } catch (e) {
      log(`cleanup: ${e?.message ?? e}`);
    }
    play.free();
    play = null;
    socket?.close();
    socket = null;
    canvas = null;
  }

  // The client ended, or could not start: what the worker holds is let go, and the page brings its
  // launcher back, saying what went wrong.
  function finish(error) {
    shutDown();
    try {
      release();
    } catch (e) {
      log(`release: ${e?.message ?? e}`);
    }
    postMessage(error === undefined ? { ended: true } : { ended: true, error });
  }

  function connect(msg) {
    counted = msg.counted ?? null;
    const ws = new WebSocket(msg.server);
    socket = ws;
    ws.binaryType = 'arraybuffer';
    let opened = false;
    ws.onmessage = (ev) => play?.receive(new Uint8Array(ev.data));
    ws.onerror = () => log(`${msg.server}: connection failed`);
    ws.onclose = () => {
      log('connection closed');
      // Never connected: the client has nothing to run, so the launcher comes back. A connection
      // lost later is the client's to notice, as it notices a silent server.
      if (!opened && play) finish(`${msg.server} did not accept the connection.`);
    };
    ws.onopen = () => {
      opened = true;
      log(`connected to ${msg.server}; logging in as ${msg.account}`);
      loop();
    };
  }

  // The world this play is counted under, and how many it said are on when it logged in: the hub
  // is told for the launcher's list, as the desktop client tells it. Nothing waits on the answer.
  let counted = null;
  function reportPlayers(players) {
    if (!counted) return;
    fetch(`${counted.hub}/v1/players`, {
      method: 'POST',
      credentials: 'omit',
      body: JSON.stringify({ host: counted.host, port: counted.port, players }),
    }).catch(() => {});
  }

  let lastWill = null;
  let lastSize = [0, 0];
  const sameBytes = (a, b) => b !== null && a.length === b.length && a.every((x, i) => x === b[i]);

  function loop() {
    lastWill = null;
    lastSize = [0, 0];
    const raf = self.requestAnimationFrame
      ? (f) => self.requestAnimationFrame(f)
      : (f) => setTimeout(() => f(performance.now()), 16);
    let frames = 0;
    let busy = 0;
    let worst = 0;
    let since = performance.now();
    const tick = (now) => {
      if (!play) return;
      const f0 = performance.now();
      let alive = true;
      try {
        alive = play.frame();
      } catch (e) {
        log(`frame: ${e?.message ?? e}`);
      }
      for (const m of play.takeMessages()) {
        if (socket.readyState === WebSocket.OPEN) socket.send(m);
      }
      const players = play.takeWorldPlayers?.();
      if (players !== undefined) reportPlayers(players);
      // The server's copy of the goodbye, renewed when the connections change, in case the page goes
      // away before the worker can log off.
      const will = play.will?.();
      if (will && socket.readyState === WebSocket.OPEN && !sameBytes(will, lastWill)) {
        socket.send(will);
        lastWill = will;
      }
      for (const text of play.takeCopied()) postMessage({ copied: text });
      // The client resizes its own canvas (entering and leaving the game); the page scales it and
      // maps the pointer by the new size.
      const size = play.clientSize();
      if (size.length === 2 && (size[0] !== lastSize[0] || size[1] !== lastSize[1])) {
        lastSize = [size[0], size[1]];
        postMessage({ size: lastSize });
      }
      // A Client Options page changed the render scale: the front page keeps it.
      const scale = play.renderScale();
      if (scale !== renderScale) {
        renderScale = scale;
        postMessage({ scale });
      }
      const cursor = play.cursorChange();
      if (cursor) showCursor(cursor);
      // A camera drag holds the pointer, or has ended: the page locks it to the canvas, or unlocks it.
      const lock = play.pointerLock();
      if (lock !== undefined) postMessage({ pointerLock: lock });
      sound(now);
      for (const url of play.takeOpenedUrls()) postMessage({ open: url });
      each();
      const spent = performance.now() - f0;
      if (frames === 0 && marks.firstFrame === undefined) {
        marks.firstFrame = performance.now();
        log(`first frame ${(marks.firstFrame - marks.start).toFixed(0)} ms after start`);
      }
      busy += spent;
      worst = Math.max(worst, spent);
      frames += 1;
      if (now - since >= 1000) {
        const fps = (frames * 1000) / (now - since);
        postMessage({
          status: `${play.backend()} · ${fps.toFixed(0)} fps · ${(busy / frames).toFixed(1)} ms a frame (worst ${worst.toFixed(1)}) · ${memory().toFixed(0)} MiB`,
        });
        frames = 0;
        busy = 0;
        worst = 0;
        since = now;
      }
      if (alive) {
        raf(tick);
      } else {
        log('the client shut down');
        finish();
      }
    };
    raf(tick);
  }

  // The sound the page's audio clock has used since the last frame, and a tenth of a second ahead,
  // mixed and posted to the page.
  const audioState = { started: false, t0: 0, sent: 0 };
  function sound(now) {
    if (!audioState.started) return;
    const r = rate();
    if (!audioState.t0) audioState.t0 = now;
    const due = Math.floor(((now - audioState.t0) / 1000 + 0.1) * r) - audioState.sent;
    if (due <= 0) return;
    const frames = Math.min(due, r / 4);
    const pcm = play.audio(frames);
    audioState.sent += frames;
    if (pcm.length) postMessage({ pcm }, [pcm.buffer]);
  }

  // The interface's cursor, as an image the page can show over the canvas: `[width, height, hot_x,
  // hot_y]` as little-endian words, then RGBA rows.
  async function showCursor(bytes) {
    const words = new DataView(bytes.buffer, bytes.byteOffset, 16);
    const [w, h, hx, hy] = [0, 4, 8, 12].map((at) => words.getUint32(at, true));
    if (!w || !h) return;
    const pixels = new Uint8ClampedArray(bytes.buffer, bytes.byteOffset + 16, w * h * 4);
    const image = new OffscreenCanvas(w, h);
    image.getContext('2d').putImageData(new ImageData(pixels, w, h), 0, 0);
    const blob = await image.convertToBlob({ type: 'image/png' });
    const url = new FileReaderSync().readAsDataURL(blob);
    postMessage({ cursor: { url, hx, hy } });
  }

  // The page's size in the device's own pixels, as the page last said: the client draws at it in
  // full screen, at the render scale.
  let pageNow = null;
  const pageArray = (p) => [p?.width ?? 800, p?.height ?? 600];
  function setPage(p) {
    pageNow = p ?? pageNow;
    if (play && pageNow) play.setPage(...pageArray(pageNow));
  }

  // The share of the page's pixels the client draws in full screen, in percent, as the front page
  // last heard it.
  let renderScale = 100;

  // One input event from the page, in the client's terms.
  function input(ev) {
    if (!play) return;
    switch (ev.kind) {
      case 'move': play.pointerMove(ev.x, ev.y, ev.dx ?? 0, ev.dy ?? 0, ev.buttons ?? -1); break;
      case 'button': play.pointerButton(ev.button, ev.pressed); break;
      case 'wheel': play.wheel(ev.notches); break;
      case 'key': play.key(ev.code, ev.pressed, ev.text ?? undefined); break;
      case 'alt': play.alt(ev.held); break;
      case 'focus': play.focus(ev.gained); break;
      case 'paste': play.paste(ev.text); break;
      default: break;
    }
  }

  async function begin(msg) {
    marks.start = performance.now();
    marks.firstFrame = undefined;
    pageNow = pageNow ?? msg.page;
    // The client starts at the front page's render scale.
    renderScale = msg.page?.scale ?? 100;
    play = await start({ ...msg, canvas, page: pageNow, pageArray: [...pageArray(pageNow), renderScale] });
    // The page may have changed while the client came up.
    setPage(pageNow);
    log(`client up on ${play.backend()} in ${(performance.now() - marks.start).toFixed(0)} ms`);
    connect(msg);
  }

  onmessage = async (ev) => {
    await ready;
    const msg = ev.data;
    try {
      if (msg.canvas) canvas = msg.canvas;
      if (msg.input) input(msg.input);
      if (msg.cmd === 'logoff') shutDown();
      if (msg.cmd === 'page') setPage(msg.page);
      if (msg.cmd === 'audio') audioState.started = true;
    } catch (e) {
      log(`error: ${e?.message ?? e}`);
    }
    if (msg.cmd === 'start') {
      try {
        await begin(msg);
      } catch (e) {
        // The client could not start: the page brings its launcher back with the reason.
        const error = String(e?.message ?? e);
        log(`error: ${error}`);
        finish(error);
      }
    }
  };
}
