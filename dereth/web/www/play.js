// The playable client's page: the front page (the launcher, `front.js`), the canvas the worker
// draws into, and the page's input events forwarded to the worker in the client's terms.
//
// The server is a WebSocket URL: a server's own wss:// endpoint, or ws://127.0.0.1:<port>/ for
// dereth-web-relay on this machine. An http:// or https:// address is taken as the server's status
// endpoint, and the WebSocket URL it reports is used.
//
// The query string may choose the world, so a server's operator can link to the page with it
// chosen: `?server=<url>&account=<name>` adds that server and chooses it; `?era=<name>` (and
// `?features=<name=true,...>`) names the era and systems of a server reached by its WebSocket URL,
// whose status the page does not read. `?dats=http` offers the dev runner's data files
// (`cargo xtask web --dat-dir`), and `?list=<url>` reads the world list from another address.
// `?log` (or `?log=debug`, ...) shows the client's log and its frame rate over the canvas, for
// reporting a problem, and `?gpu=webgl` draws with WebGL 2 where a browser's WebGPU misbehaves.

import { startFront } from './front.js';

const $ = (id) => document.getElementById(id);
const t0 = performance.now();
const params = new URLSearchParams(location.search);

const logBox = $('log');
if (!params.has('log')) {
  logBox.classList.add('hidden');
  $('status').classList.add('hidden');
}
// A page the dev runner serves on this machine may echo its log to the runner's terminal
// (`?echo=1`); nowhere else is the log sent.
const echo = ['127.0.0.1', 'localhost', '[::1]'].includes(location.hostname) && params.has('echo');
function show(text) {
  const line = `[${((performance.now() - t0) / 1000).toFixed(2)}s] ${text}`;
  logBox.textContent += `${line}\n`;
  logBox.scrollTop = logBox.scrollHeight;
  console.info(line);
  if (echo) fetch('/log', { method: 'POST', body: line, keepalive: true }).catch(() => {});
}

const view = $('view');
const worker = new Worker(`play-worker.js${location.search}`, { type: 'module' });
const offscreen = view.transferControlToOffscreen();
worker.postMessage({ canvas: offscreen }, [offscreen]);

// The drawing buffer's size, which the client chooses; the page only scales it to fit.
let size = [800, 600];

function fit() {
  const scale = Math.min(innerWidth / size[0], innerHeight / size[1]);
  view.style.width = `${Math.floor(size[0] * scale)}px`;
  view.style.height = `${Math.floor(size[1] * scale)}px`;
}
addEventListener('resize', fit);
fit();

// The sound: an audio context the launch opens (a page may only start one from a click), playing
// what the worker mixes.
let audio = null;
async function openAudio(rate) {
  try {
    const ctx = new AudioContext({ sampleRate: rate, latencyHint: 'interactive' });
    await ctx.audioWorklet.addModule('audio-worklet.js');
    const node = new AudioWorkletNode(ctx, 'dereth-pcm', { outputChannelCount: [2] });
    node.connect(ctx.destination);
    audio = { ctx, node };
    if (ctx.state !== 'running') await ctx.resume();
    worker.postMessage({ cmd: 'audio', started: true });
  } catch (e) {
    show(`no sound: ${e?.message ?? e}`);
  }
}

// The front page's questions to the launcher in the worker, each answered once.
let nextCall = 1;
const pending = new Map();
function call(name, ...args) {
  return new Promise((resolve, reject) => {
    const id = nextCall;
    nextCall += 1;
    pending.set(id, { resolve, reject });
    worker.postMessage({ call: name, args, id });
  });
}

let front = null;

worker.onmessage = (ev) => {
  const m = ev.data;
  if (m.reply !== undefined) {
    const p = pending.get(m.reply);
    pending.delete(m.reply);
    if (m.error !== undefined) p?.reject(new Error(m.error));
    else p?.resolve(m.value);
    return;
  }
  if (m.pcm) audio?.node.port.postMessage(m.pcm, [m.pcm.buffer]);
  if (m.log) show(m.log);
  if (m.error) show(`error: ${m.error}`);
  if (m.status !== undefined) $('status').textContent = m.status;
  if (m.size) {
    show(`drawing at ${m.size[0]}x${m.size[1]}`);
    size = m.size;
    fit();
  }
  if (m.copied !== undefined) {
    show(`copied ${m.copied.length} character(s)`);
    navigator.clipboard?.writeText(m.copied).catch(() => {});
  }
  if (m.open) window.open(m.open, '_blank', 'noopener');
  // The interface's own cursor, at its own size.
  if (m.cursor) view.style.cursor = `url(${m.cursor.url}) ${m.cursor.hx} ${m.cursor.hy}, default`;
  // A camera drag holds the pointer, or has ended.
  if (m.pointerLock === true) lockPointer();
  if (m.pointerLock === false) unlockPointer();
  // The client ended (its own Exit, or a lost connection) or could not start: the front page
  // comes back, saying what went wrong. Data files already open stay open for the next start.
  if (m.ended) {
    $('go').disabled = false;
    $('go').textContent = 'Play again';
    $('launch').style.display = 'block';
    front?.back().then(() => {
      $('problem').textContent = m.error ?? '';
    });
  }
};
worker.onerror = (e) => show(`worker error: ${e.message}`);

// ---- the front page ------------------------------------------------------------------------------

// Start the client on what the front page chose: the data files, the server, the account, what
// the launcher says about the world, and the folder its overlay is kept in.
function launch(choice) {
  $('go').disabled = true;
  if (!audio) openAudio(48000);
  $('launch').style.display = 'none';
  worker.postMessage({ cmd: 'start', ...choice, width: size[0], height: size[1] });
  view.focus();
}

front = startFront({ call, launch, params, log: show });
front.begin().catch((e) => {
  show(`front page: ${e?.message ?? e}`);
  $('list-note').textContent = `The launcher did not start: ${e?.message ?? e}`;
});

// ---- input -----------------------------------------------------------------------------------

const send = (input) => worker.postMessage({ input });

// Canvas pixels, from the pointer's place in the scaled canvas.
function at(ev) {
  const r = view.getBoundingClientRect();
  return {
    x: ((ev.clientX - r.left) * size[0]) / r.width,
    y: ((ev.clientY - r.top) * size[1]) / r.height,
  };
}

// The mouse's own movement, in canvas pixels: what turns the camera while a drag holds the pointer.
function movement(ev) {
  const r = view.getBoundingClientRect();
  return { dx: (ev.movementX * size[0]) / r.width, dy: (ev.movementY * size[1]) / r.height };
}

// Each button's bit in a pointer event's `buttons`, by its number.
const BUTTON_BIT = [1, 4, 2, 8, 16];

// The pointer that last went down, kept for the canvas to keep its events after a lock ends.
let lastPointer = null;

view.addEventListener('pointermove', (ev) => {
  // A button pressed or let go of while another is held comes as a movement that names it.
  if (ev.button >= 0 && ev.button < BUTTON_BIT.length) {
    send({ kind: 'button', button: ev.button, pressed: (ev.buttons & BUTTON_BIT[ev.button]) !== 0 });
  }
  send({ kind: 'move', ...at(ev), ...movement(ev), buttons: ev.buttons });
});
view.addEventListener('pointerdown', (ev) => {
  view.focus();
  if (audio && audio.ctx.state !== 'running') audio.ctx.resume();
  lastPointer = ev.pointerId;
  view.setPointerCapture(ev.pointerId);
  send({ kind: 'move', ...at(ev) });
  send({ kind: 'button', button: ev.button, pressed: true });
  ev.preventDefault();
});
view.addEventListener('pointerup', (ev) => {
  send({ kind: 'move', ...at(ev) });
  send({ kind: 'button', button: ev.button, pressed: false });
  ev.preventDefault();
});
view.addEventListener('contextmenu', (ev) => ev.preventDefault());

// A camera drag hides the pointer and holds it still: the browser locks it to the canvas, if it
// will. It locks it only soon after a press on the page, which a drag follows; refused, the pointer
// stays shown and the drag still turns the camera.
function lockPointer() {
  if (document.pointerLockElement === view) return;
  try {
    const asked = view.requestPointerLock();
    if (asked && typeof asked.catch === 'function') asked.catch(() => {});
  } catch (_) {
    // Refused: the pointer stays shown.
  }
}
function unlockPointer() {
  if (document.pointerLockElement === view) document.exitPointerLock();
}
// A lock the browser ended with a button still down: the canvas keeps the pointer's events, so the
// button's release still reaches the client.
document.addEventListener('pointerlockchange', () => {
  if (document.pointerLockElement === view || lastPointer === null) return;
  try {
    view.setPointerCapture(lastPointer);
  } catch (_) {
    // The pointer is no longer down.
  }
});
view.addEventListener('wheel', (ev) => {
  // One detent is about 100 pixels, or three lines.
  const notches = ev.deltaMode === 1 ? -ev.deltaY / 3 : -ev.deltaY / 100;
  send({ kind: 'wheel', notches });
  ev.preventDefault();
}, { passive: false });
view.addEventListener('focus', () => send({ kind: 'focus', gained: true }));
view.addEventListener('blur', () => send({ kind: 'focus', gained: false }));

// The text a key types, as the host translates it: a printable character, the control character
// for Ctrl with a letter, and the few control keys that type one.
function typed(ev) {
  if (ev.ctrlKey && /^[a-z]$/i.test(ev.key)) return String.fromCharCode(ev.key.toUpperCase().charCodeAt(0) - 64);
  if (ev.key.length === 1) return ev.key;
  return { Enter: '\r', Backspace: '\b', Tab: '\t', Escape: '\x1b' }[ev.key];
}

// Keys the browser keeps: reload, full screen, developer tools, and the system's own shortcuts.
const KEPT = new Set(['F5', 'F11', 'F12']);

function key(ev, pressed) {
  if (ev.metaKey || KEPT.has(ev.code)) return;
  if (ev.code === 'AltLeft' || ev.code === 'AltRight') send({ kind: 'alt', held: pressed });
  // Ctrl+V lets the paste event through, which carries the text; the key follows it.
  if (pressed && ev.ctrlKey && ev.code === 'KeyV') return;
  send({ kind: 'key', code: ev.code, pressed, text: pressed ? typed(ev) : undefined });
  ev.preventDefault();
}
view.addEventListener('keydown', (ev) => key(ev, true));
view.addEventListener('keyup', (ev) => key(ev, false));
document.addEventListener('paste', (ev) => {
  // A paste into one of the page's own fields (the server, the account, the password) is that
  // field's; only one made while the game has the keyboard goes to the game.
  if (document.activeElement !== view) return;
  send({ kind: 'paste', text: ev.clipboardData?.getData('text/plain') ?? '' });
  // The client pastes on its own Ctrl+V, a frame later, once its copy of the clipboard has the
  // text: the page's paste (Cmd+V on a Mac) becomes that key chord.
  setTimeout(() => {
    send({ kind: 'key', code: 'ControlLeft', pressed: true });
    send({ kind: 'key', code: 'KeyV', pressed: true, text: '\x16' });
    send({ kind: 'key', code: 'KeyV', pressed: false });
    send({ kind: 'key', code: 'ControlLeft', pressed: false });
  }, 50);
  ev.preventDefault();
});

// Closing the page logs off, so the server does not hold the account until it times out. The
// worker is stopped once this handler returns, so it waits a moment for the worker's goodbye (the
// worker runs on its own thread meanwhile). The server, or the relay, also holds the connection's
// goodbye, which goes out if this one does not.
addEventListener('pagehide', () => {
  worker.postMessage({ cmd: 'logoff' });
  const until = performance.now() + 300;
  while (performance.now() < until) {
    // The worker's thread runs the log-off meanwhile; nothing on this one can.
  }
});
