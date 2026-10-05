// The playable client's worker. It opens the player's data files, brings the client up drawing
// into the canvas the page handed it, carries datagrams between the client and the server's
// WebSocket (a server's own endpoint, or dereth-web-relay on this machine), runs
// one client frame per animation frame, and hands the client the page's input events.

import init, * as dereth from './pkg/dereth_web.js';
import { openFiles, openStore } from './datfiles.js';

const log = (text) => postMessage({ log: String(text) });
const params = new URLSearchParams(location.search);

let wasm = null;
const ready = init().then((exports) => {
  wasm = exports;
  // `?log=debug` (or `trace`, `warn`, ...) sets how much of the client's log reaches the console.
  dereth.start(params.get('log') || 'info');
});

let canvas = null;
let play = null;
let socket = null;
const marks = {};

const memoryMiB = () => (wasm?.memory?.buffer?.byteLength ?? 0) / 1048576;

// The page is going away: say goodbye now, or the server holds the account until it times out.
function logOff() {
  shutDown();
}

// Log off and run the client's cleanup, which saves its keymap and preferences; the goodbye goes
// out before the socket closes. A later start opens another.
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
}

// The client's own files (preferences, keymaps, layouts), kept in one file in the origin's private
// storage. Its access handle is synchronous, so a save is on disk before the client's call
// returns, which is what lets the page's close save them. Memory only where there is no such
// storage.
let settings = null;
async function openSettings() {
  try {
    const root = await navigator.storage.getDirectory();
    const file = await root.getFileHandle('settings.bin', { create: true });
    settings = await file.createSyncAccessHandle();
  } catch (e) {
    log(`settings kept for this page only: ${e?.message ?? e}`);
  }
}
globalThis.derethSettingsLoad = () => {
  if (!settings) return new Uint8Array(0);
  const bytes = new Uint8Array(settings.getSize());
  settings.read(bytes, { at: 0 });
  return bytes;
};
globalThis.derethSettingsSave = (bytes) => {
  if (!settings) return;
  settings.truncate(0);
  settings.write(bytes, { at: 0 });
  settings.flush();
};

let datsOpen = false;
// The era the store was opened for: a later start for another era opens it again.
let storeEra = null;

async function start(msg) {
  marks.start = performance.now();
  marks.firstFrame = undefined;
  // The server first: a refused URL is known before the data files take their time to open.
  const refused = dereth.serverUrlProblem(msg.server);
  if (refused) throw new Error(refused);
  // A second start (after the client ended) keeps the data files and the settings file open.
  if (!datsOpen) {
    if (!(await openFiles(msg.dats))) {
      throw new Error(msg.dats.mode === 'opfs' && !msg.dats.files?.length
        ? 'This browser has no copy of the data files yet: pick them and keep a copy.'
        : 'The data files did not open: pick the four client_*.dat files (and portal.dat and cell.dat for the classic interface).');
    }
    datsOpen = true;
  }
  const era = msg.era || '';
  if (storeEra !== era) {
    try {
      openStore(dereth, era);
    } catch (e) {
      // The files picked are not enough for this world: the form offers the picker again.
      datsOpen = false;
      storeEra = null;
      throw new Error(`The data files did not open: ${e?.message ?? e}`);
    }
    storeEra = era;
    log(`WebAssembly memory with the data files open: ${memoryMiB().toFixed(0)} MiB`);
  }
  marks.dats = performance.now();
  if (!settings) await openSettings();
  const sequence = (Date.now() % 0x100000000) >>> 0;
  // `?gpu=webgl` draws with WebGL 2 even where the browser has WebGPU.
  play = await dereth.WebPlay.create(
    canvas, msg.width, msg.height, msg.account, msg.password, sequence,
    params.get('gpu') === 'webgl', era, msg.features || '',
  );
  marks.up = performance.now();
  log(`client up on ${play.backend()} in ${(marks.up - marks.dats).toFixed(0)} ms`);

  const ws = new WebSocket(msg.server);
  socket = ws;
  ws.binaryType = 'arraybuffer';
  let opened = false;
  ws.onmessage = (ev) => play?.receive(new Uint8Array(ev.data));
  ws.onerror = () => log(`${msg.server}: connection failed`);
  ws.onclose = () => {
    log('connection closed');
    // Never connected: the client has nothing to run, so the form comes back. A connection lost
    // later is the client's to notice, as it notices a silent server.
    if (!opened && play) {
      shutDown();
      postMessage({ ended: true, datsOpen, error: `${msg.server} did not accept the connection.` });
    }
  };
  ws.onopen = () => {
    opened = true;
    log(`connected to ${msg.server}; logging in as ${msg.account}`);
    loop();
  };
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
    // The server's copy of the goodbye, renewed when the connections change, in case the page goes
    // away before the worker can log off.
    const will = play.will();
    if (socket.readyState === WebSocket.OPEN && !sameBytes(will, lastWill)) {
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
    const cursor = play.cursorChange();
    if (cursor) showCursor(cursor);
    sound(now);
    for (const url of play.takeOpenedUrls()) postMessage({ open: url });
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
        status: `${play.backend()} · ${fps.toFixed(0)} fps · ${(busy / frames).toFixed(1)} ms a frame (worst ${worst.toFixed(1)}) · ${memoryMiB().toFixed(0)} MiB`,
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
      shutDown();
      postMessage({ ended: true, datsOpen });
    }
  };
  raf(tick);
}

// The sound the page's audio clock has used since the last frame, and a tenth of a second ahead,
// mixed and posted to the page.
const audioState = { started: false, t0: 0, sent: 0 };
function sound(now) {
  if (!audioState.started) return;
  const rate = dereth.WebPlay.audioRate();
  if (!audioState.t0) audioState.t0 = now;
  const due = Math.floor(((now - audioState.t0) / 1000 + 0.1) * rate) - audioState.sent;
  if (due <= 0) return;
  const frames = Math.min(due, rate / 4);
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
  const canvas = new OffscreenCanvas(w, h);
  canvas.getContext('2d').putImageData(new ImageData(pixels, w, h), 0, 0);
  const blob = await canvas.convertToBlob({ type: 'image/png' });
  const url = new FileReaderSync().readAsDataURL(blob);
  postMessage({ cursor: { url, hx, hy } });
}

// One input event from the page, in the client's terms.
function input(ev) {
  if (!play) return;
  switch (ev.kind) {
    case 'move': play.pointerMove(ev.x, ev.y); break;
    case 'button': play.pointerButton(ev.button, ev.pressed); break;
    case 'wheel': play.wheel(ev.notches); break;
    case 'key': play.key(ev.code, ev.pressed, ev.text ?? undefined); break;
    case 'alt': play.alt(ev.held); break;
    case 'focus': play.focus(ev.gained); break;
    case 'paste': play.paste(ev.text); break;
    default: break;
  }
}

onmessage = async (ev) => {
  await ready;
  const msg = ev.data;
  try {
    if (msg.canvas) canvas = msg.canvas;
    if (msg.input) input(msg.input);
    if (msg.cmd === 'logoff') logOff();
    if (msg.cmd === 'audio') audioState.started = true;
  } catch (e) {
    log(`error: ${e?.message ?? e}`);
  }
  if (msg.cmd === 'start') {
    try {
      await start(msg);
    } catch (e) {
      // The client could not start: the page brings its form back with the reason.
      const error = String(e?.message ?? e);
      log(`error: ${error}`);
      shutDown();
      postMessage({ ended: true, datsOpen, error });
    }
  }
};
