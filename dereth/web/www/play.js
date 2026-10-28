// The playable client's page: the front page (the launcher, `front.js`), the canvas the worker
// draws into, and the page's input events forwarded to the worker in the client's terms.
//
// The server is a WebSocket URL: a server's own wss:// endpoint, or ws://127.0.0.1:<port>/ for
// dereth-web-relay on this machine. An http:// or https:// address is taken as the server's status
// endpoint, and the WebSocket URL it reports is used.
//
// The query string may choose the world, so a server's operator can link to the page with it
// chosen: `?server=<url>&account=<name>` adds that server and chooses it; a server linked by its
// status address says its own era and systems. `?dats=http` offers the dev runner's data files
// (`cargo xtask web --dat-dir`), and `?list=<url>` reads the world list from another address (a
// `Servers.xml`, read alone).
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

// The drawing buffer's size, which the client chooses; the page only scales it to fit. In full
// screen the client draws at the page's own size (below), so it fills the page.
let size = [800, 600];

function fit() {
  const scale = Math.min(innerWidth / size[0], innerHeight / size[1]);
  view.style.width = `${Math.floor(size[0] * scale)}px`;
  view.style.height = `${Math.floor(size[1] * scale)}px`;
}

// The render scale: the share of the page's own pixels the client draws in full screen, which
// the front page chooses, the game's Client Options page changes, and this browser keeps. Fewer
// pixels draw faster and look softer.
const SCALE_KEY = 'dereth.renderScale';
// The key a choice was kept under before, read while no choice is kept under the new one.
const SCALE_KEY_BEFORE = 'dereth.drawingScale';
const scaleBox = $('scale');
function showScale(value) {
  if ([...scaleBox.options].some((o) => o.value === value)) scaleBox.value = value;
}
function keepScale(value) {
  try {
    localStorage.setItem(SCALE_KEY, value);
    localStorage.removeItem(SCALE_KEY_BEFORE);
  } catch (_) {
    // Kept for this visit only.
  }
}
try {
  const kept = localStorage.getItem(SCALE_KEY) ?? localStorage.getItem(SCALE_KEY_BEFORE);
  if (kept) showScale(kept);
} catch (_) {
  // No storage: every visit starts at the whole page.
}

// The page in the device's own pixels, and the render scale in percent.
function page() {
  const r = $('stage').getBoundingClientRect();
  const ratio = devicePixelRatio || 1;
  return {
    width: Math.round(r.width * ratio),
    height: Math.round(r.height * ratio),
    scale: Number(scaleBox.value) || 100,
  };
}

function pageChanged() {
  fit();
  worker.postMessage({ cmd: 'page', page: page() });
}
addEventListener('resize', pageChanged);
// Moving the window to a screen of another pixel ratio, or zooming the page, changes the page's
// pixels without always resizing it.
function watchRatio() {
  matchMedia(`(resolution: ${devicePixelRatio}dppx)`).addEventListener('change', () => {
    pageChanged();
    watchRatio();
  }, { once: true });
}
watchRatio();
scaleBox.addEventListener('change', () => keepScale(scaleBox.value));
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
  // The game's Client Options page changed the render scale: the next start begins there.
  if (m.scale !== undefined) {
    showScale(String(m.scale));
    keepScale(String(m.scale));
  }
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

// Plugins: a folder the player picks is copied into this browser's private storage by the worker
// and loads when the client starts; the client asks before granting what a plugin asks for.
$('add-plugin').addEventListener('click', () => $('plugin-folder').click());
$('plugin-folder').addEventListener('change', async () => {
  const picked = [...$('plugin-folder').files];
  const files = await Promise.all(picked.map(async (f) => ({
    path: f.webkitRelativePath || f.name,
    bytes: new Uint8Array(await f.arrayBuffer()),
  })));
  if (!files.some((f) => f.path.endsWith('/plugin.toml'))) {
    $('plugin-note').textContent = 'That folder has no plugin.toml: pick the plugin\'s own folder.';
    return;
  }
  $('plugin-note').textContent = '';
  worker.postMessage({ cmd: 'addPlugins', files });
  $('plugin-folder').value = '';
});
// A plugin's zip is kept as it is, beside the folders; the client reads it when it starts.
$('add-plugin-zip').addEventListener('click', () => $('plugin-zip').click());
$('plugin-zip').addEventListener('change', async () => {
  const picked = [...$('plugin-zip').files].filter((f) => f.name.toLowerCase().endsWith('.zip'));
  const files = await Promise.all(picked.map(async (f) => ({
    path: f.name,
    bytes: new Uint8Array(await f.arrayBuffer()),
  })));
  if (files.length) worker.postMessage({ cmd: 'addPlugins', files });
  $('plugin-zip').value = '';
});
$('remove-plugins').addEventListener('click', async () => {
  try {
    await call('removePlugins');
  } catch (e) {
    $('plugin-note').textContent = String(e?.message ?? e);
  }
  showSources();
});

// Plugin sources, as the desktop launcher has them (its own logic, in the worker): Dereth's own and
// the ones the player adds by address, each with the plugins installed from it and then the ones
// it offers that are not installed, each saying what it does and uses, with Install, Update and
// Remove. A plugin installed here is kept in this browser and found by the client when it starts,
// on the first time it is found, until the player turns it off there. Every source is read as the
// page opens and again with
// Refresh; updates apply as the setting says, and one that asks for more permissions waits for
// the player.
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
const uses = (list) => (list.length ? ` Uses ${list.join(', ')}.` : '');
const DESKTOP = '<span class="muted" title="It downloads from a GitHub release, which web pages can&#39;t fetch.">Desktop launcher only</span>';
let shelf = null;

// A plugin a source offers that is not installed.
function availableRow(a) {
  const action = a.state === 'install'
    ? (a.desktop_only ? DESKTOP : `<button type="button" class="btn" data-plugin-install="${esc(a.id)}" data-source="${esc(a.source)}">Install</button>`)
    : a.state === 'needs_newer_dereth' ? '<span class="muted">Needs a newer Dereth</span>' : '<span class="muted">Only from Dereth</span>';
  return `<span class="what" style="padding-left:14px">Plugin <b>${esc(a.name)}</b> <span class="muted">${esc(`${a.description}${uses([...a.permissions, ...a.domains])}`)}</span></span><span class="muted">${esc(a.version)}</span>${action}`;
}

// An installed plugin, with its update when one waits; `from` says where it came from, when its
// row is not under its source.
function installedRow(p, from = '') {
  const u = p.update;
  let what = esc(`${from}${p.description}${uses([...p.permissions, ...p.domains])}`);
  let update = '';
  if (u?.needs_newer_dereth) {
    what += ` <span class="bad">${esc(u.to)} needs a newer Dereth.</span>`;
  } else if (u) {
    const asks = [...u.new_permissions, ...u.new_domains];
    if (asks.length) what += ` <span class="bad">${esc(u.to)} also asks for ${esc(asks.join(', '))}.</span>`;
    update = p.desktop_only ? DESKTOP : `<button type="button" class="btn" data-plugin-update="${esc(p.id)}">Update to ${esc(u.to)}</button>`;
  }
  const remove = `<button type="button" class="btn" data-plugin-remove="${esc(p.id)}">Remove</button>`;
  return `<span class="what" style="padding-left:14px">Plugin <b>${esc(p.name)}</b> <span class="muted">${what}</span></span><span class="muted">${esc(p.version)}</span>${update ? `<span class="btns" style="margin:0">${update}${remove}</span>` : remove}`;
}

async function showSources() {
  try {
    shelf = await call('plugins');
  } catch (e) {
    $('source-note').textContent = String(e?.message ?? e);
    return;
  }
  $('plugin-updates').value = shelf.auto_update;
  const rows = [];
  for (const s of shelf.sources) {
    const read = s.problem ? `<span class="bad">${esc(s.problem)}</span>` : `${s.plugins} plugin${s.plugins === 1 ? '' : 's'}`;
    const end = s.first_party ? '<span class="muted">Dereth&#39;s own</span>' : `<button type="button" class="btn" data-source-remove="${esc(s.url)}">Remove</button>`;
    rows.push(`<span class="what">Source <b>${esc(s.name)}</b> <span class="muted">${esc(s.url)} · ${read}</span></span><span></span>${end}`);
    for (const p of shelf.installed.filter((x) => x.source === s.url)) rows.push(installedRow(p));
    for (const a of shelf.available.filter((x) => x.source === s.url)) rows.push(availableRow(a));
  }
  // Installed plugins whose source was removed.
  const gone = shelf.installed.filter((x) => x.source_removed);
  if (gone.length) rows.push('<span class="what muted">Installed, and listed by no source here</span><span></span><span></span>');
  for (const p of gone) rows.push(installedRow(p, `From ${p.source_name}, a source you removed: no updates. `));
  $('source-plugins').innerHTML = rows.join('');
}

// One of the plugin calls, with the plugins' buttons off while it runs, and what it came to said.
async function pluginCall(work) {
  const controls = [...document.querySelectorAll('#source-plugins button, #add-source, #refresh-sources, #source-confirm, #plugin-updates')];
  for (const c of controls) c.disabled = true;
  try {
    $('source-note').textContent = (await work()) || '';
  } catch (e) {
    $('source-note').textContent = String(e?.message ?? e);
  }
  for (const c of controls) c.disabled = false;
  await showSources();
}

$('source-plugins').addEventListener('click', (ev) => {
  const d = ev.target.closest('button')?.dataset;
  if (!d) return;
  if (d.pluginInstall) pluginCall(() => call('installPlugin', d.source, d.pluginInstall));
  if (d.pluginUpdate) pluginCall(() => call('updatePlugin', d.pluginUpdate));
  if (d.pluginRemove) pluginCall(() => call('removePlugin', d.pluginRemove));
  if (d.sourceRemove) pluginCall(() => call('pluginRemoveSource', d.sourceRemove));
});

// Add the source typed in. One other than Dereth's is added once the player confirms it, on the
// line under the address.
function addSource(confirmed) {
  const url = $('source-url').value.trim();
  if (!url) return;
  if (!confirmed && url !== shelf?.first_party) {
    $('source-warn').classList.remove('hidden');
    return;
  }
  $('source-warn').classList.add('hidden');
  pluginCall(async () => {
    const said = await call('pluginAddSource', url);
    $('source-url').value = '';
    return said;
  });
}
$('add-source').addEventListener('click', () => addSource(false));
$('source-url').addEventListener('keydown', (ev) => {
  if (ev.key === 'Enter') addSource(false);
});
$('source-url').addEventListener('input', () => $('source-warn').classList.add('hidden'));
$('source-confirm').addEventListener('click', () => addSource(true));
$('source-cancel').addEventListener('click', () => $('source-warn').classList.add('hidden'));
$('refresh-sources').addEventListener('click', () => pluginCall(async () => (await call('pluginRefresh')).said || 'Nothing new.'));
$('plugin-updates').addEventListener('change', () => pluginCall(() => call('pluginUpdates', $('plugin-updates').value)));

// As the page opens: what is kept, then every source read again, and what is new said.
showSources().then(() => pluginCall(async () => (await call('pluginRefresh')).said));

// Start the client on what the front page chose: the data files, the server, the account, what
// the launcher says about the world, and the folder its overlay is kept in.
function launch(choice) {
  $('go').disabled = true;
  if (!audio) openAudio(48000);
  $('launch').style.display = 'none';
  worker.postMessage({
    cmd: 'start',
    ...choice,
    width: size[0],
    height: size[1],
    page: page(),
    // `?plugin_policy=https://…`: the world's plugin policy document, from its world entry.
    pluginPolicy: params.get('plugin_policy') || undefined,
  });
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
