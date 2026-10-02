// The playable client's page: the launch form, the canvas the worker draws into, and the page's
// input events forwarded to the worker in the client's terms.
//
// The server is a WebSocket URL: a server's own wss:// endpoint, or ws://127.0.0.1:<port>/ for
// dereth-web-relay on this machine. An http:// or https:// address is taken as the server's status
// endpoint, and the WebSocket URL it reports is used.
//
// The query string may fill the form, so a server's operator can link to the page with the server
// chosen: `?server=<url>&account=<name>`; `?era=<name>` names the era of a server reached by its
// WebSocket URL, whose status the page does not read. `?dats=http` offers the dev runner's data files
// (`cargo xtask web --dat-dir`). `?log` (or `?log=debug`, ...) shows the client's log and its frame
// rate over the canvas, for reporting a problem, and `?gpu=webgl` draws with WebGL 2 where a
// browser's WebGPU misbehaves.

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

worker.onmessage = (ev) => {
  const m = ev.data;
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
  // The client ended (its own Exit, or a lost connection) or could not start: the form comes back,
  // saying what went wrong. Data files already open stay open for the next start.
  if (m.ended) {
    if (m.datsOpen) {
      $('source').disabled = true;
      $('files').style.display = 'none';
    }
    $('go').disabled = false;
    $('go').textContent = 'Play again';
    $('problem').textContent = m.error ?? '';
    $('launch').style.display = 'grid';
  }
};
worker.onerror = (e) => show(`worker error: ${e.message}`);

// ---- the launch form -------------------------------------------------------------------------

// The dev runner's data files are offered only to a page that asks for them.
if (params.get('dats') === 'http') {
  $('source').add(new Option('From the dev runner (cargo xtask web --dat-dir)', 'http'), 0);
}

// The form as the player last filled it. Private browsing may refuse storage; the form is then
// simply not remembered.
const FIELDS = ['source', 'server', 'account'];
function remembered(f) {
  try {
    return localStorage.getItem(`dereth.${f}`);
  } catch {
    return null;
  }
}
for (const f of FIELDS) {
  const saved = params.get(f === 'source' ? 'dats' : f) ?? remembered(f);
  const offered = f !== 'source' || [...$('source').options].some((o) => o.value === saved);
  if (saved && offered) $(f).value = saved;
}
$('source').onchange = () => {
  $('files').style.display = ['files', 'store'].includes($('source').value) ? 'block' : 'none';
};
$('source').onchange();

// The era the world plays: the one its status names, else `?era=<name>`, else none (the client
// reads it from the data files). With it, the systems the world has: its status's `features`
// object as `name=true,...`, else `?features=<name=true,...>`, else none (the era's own table).
let era = params.get('era') || '';
let features = params.get('features') || '';

// The WebSocket URL a status address reports, filled into the server field; the era and the
// systems it names are kept for the client.
async function fromStatus(address) {
  const url = new URL(address);
  if (url.pathname === '/' || url.pathname === '') url.pathname = '/status';
  const status = await (await fetch(url, { cache: 'no-store' })).json();
  if (!status.websocket_url) throw new Error(`${url}: the server has no WebSocket endpoint`);
  if (status.era) era = status.era;
  if (status.features && typeof status.features === 'object') {
    features = Object.entries(status.features)
      .filter(([, on]) => typeof on === 'boolean')
      .map(([name, on]) => `${name}=${on}`)
      .join(',');
  }
  return status.websocket_url;
}

async function launch(ev) {
  ev?.preventDefault();
  if (/^https?:\/\//i.test($('server').value.trim())) {
    try {
      $('server').value = await fromStatus($('server').value.trim());
      show(`server from its status: ${$('server').value}`);
    } catch (e) {
      $('problem').textContent = `The server's status did not answer: ${e?.message ?? e}`;
      show(`status: ${e?.message ?? e}`);
      return;
    }
  }
  $('problem').textContent = '';
  for (const f of FIELDS) {
    try {
      localStorage.setItem(`dereth.${f}`, $(f).value);
    } catch {
      // Not remembered; see `remembered`.
    }
  }
  const source = $('source').value;
  const files = [...$('files').files];
  const dats = source === 'store'
    ? { mode: 'opfs', files }
    : { mode: source, files };
  $('go').disabled = true;
  if (!audio) openAudio(48000);
  $('launch').style.display = 'none';
  worker.postMessage({
    cmd: 'start',
    dats,
    width: size[0],
    height: size[1],
    server: $('server').value.trim(),
    account: $('account').value.trim(),
    password: $('password').value,
    era,
    features,
  });
  view.focus();
}
$('form').onsubmit = launch;

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

view.addEventListener('pointermove', (ev) => send({ kind: 'move', ...at(ev) }));
view.addEventListener('pointerdown', (ev) => {
  view.focus();
  if (audio && audio.ctx.state !== 'running') audio.ctx.resume();
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
