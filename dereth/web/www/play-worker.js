// The Dereth client's worker, from a page's first play of it: loads the game's module, fetches the Horizon
// interface's art from beside the module, opens the player's data files and the world's overlay, and
// brings the client up drawing into the canvas the page handed it. The rest of a play is what every
// game's worker does (`game-loop.js`).

import init, * as dereth from './pkg/dereth_web.js';
import { filesStamp, flushOverlay, holdOverlay, openFiles, openStore, releaseFiles } from './datfiles.js';
import { serve } from './game-loop.js';
import { installPluginIo, fetchPluginPolicy } from './plugin-io.js';

installPluginIo();

const log = (text) => postMessage({ log: String(text) });
const params = new URLSearchParams(location.search);

let wasm = null;
const ready = init().then(async (exports) => {
  wasm = exports;
  // `?log=debug` (or `trace`, `warn`, ...) sets how much of the client's log reaches the console.
  dereth.start(params.get('log') || 'info');
  await openSettings();
  dereth.installSettings();
});

// The Horizon interface's art, which the module does not carry: its files are served beside the
// module, in pkg/horizon/, and fetched from when the worker starts. The client offers the Horizon
// interface once they are in (a choice of it before then waits), and a failure leaves the other
// interfaces as they are and says so.
const art = ready.then(async () => {
  try {
    await Promise.all(dereth.horizonFiles().map(async (name) => {
      const res = await fetch(new URL(`./pkg/horizon/${name}`, import.meta.url));
      if (!res.ok) throw new Error(`pkg/horizon/${name}: HTTP ${res.status}`);
      dereth.horizonFile(name, new Uint8Array(await res.arrayBuffer()));
    }));
    dereth.horizonLoaded();
    log("the Horizon interface's art is in");
  } catch (e) {
    const why = String(e?.message ?? e);
    dereth.horizonFailed(why);
    log(`the Horizon interface's art did not load (${why}): the Horizon interface is not offered`);
  }
});

const memoryMiB = () => (wasm?.memory?.buffer?.byteLength ?? 0) / 1048576;

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
function closeSettings() {
  settings?.flush();
  settings?.close();
  settings = null;
}

// Plugins the player added to this page: under `plugins/` in the origin's private storage, one
// folder per plugin or a plugin's zip file, read into one bundle before the client starts (each file's path and its
// bytes, each preceded by its length, as the settings file is laid out).
let pluginBundle = new Uint8Array(0);
async function pluginsDir(create) {
  const root = await navigator.storage.getDirectory();
  return root.getDirectoryHandle('plugins', { create });
}
// Files as one bundle: each path and its bytes, each preceded by its length.
function bundleOf(files) {
  const parts = [];
  const enc = new TextEncoder();
  const put = (bytes) => {
    const len = new Uint8Array(4);
    new DataView(len.buffer).setUint32(0, bytes.length, true);
    parts.push(len, bytes);
  };
  for (const f of files) {
    put(enc.encode(f.path));
    put(f.bytes);
  }
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}
async function readPlugins() {
  const files = [];
  const walk = async (dir, prefix) => {
    for await (const [name, handle] of dir.entries()) {
      const path = prefix ? `${prefix}/${name}` : name;
      if (handle.kind === 'directory') {
        await walk(handle, path);
      } else {
        const file = await handle.getFile();
        files.push({ path, bytes: new Uint8Array(await file.arrayBuffer()) });
      }
    }
  };
  try {
    await walk(await pluginsDir(false), '');
  } catch {
    return new Uint8Array(0);
  }
  return bundleOf(files);
}
globalThis.derethPluginsLoad = () => pluginBundle;

// The store opened over the data files, by the files and the set the world is drawn from: a later
// start on the same ones takes the files up again and has no need to open it anew.
let storeKey = null;

// Bring the client up for the world `msg` names.
async function start(msg) {
  if (!settings) await openSettings();
  dereth.installSettings();
  if (!(await openFiles(msg.dats))) {
    throw new Error('The data files did not open: choose a data set with the four client_*.dat files (and portal.dat and cell.dat for the classic interface and the early worlds).');
  }
  // The world's overlay, kept in this browser by its address, when it can be: another tab of this
  // site may hold it, and then the client keeps none this visit (and says so to the server).
  const overlay = await holdOverlay(msg.overlay || '');
  const args = msg.args || [];
  const key = `${filesStamp(msg.dats)}|${dereth.worldSet(args, overlay)}`;
  if (storeKey !== key) {
    storeKey = null;
    try {
      openStore(dereth, args, overlay);
    } catch (e) {
      // The files picked are not enough for this world: the front page offers others.
      throw new Error(`The data files did not open: ${e?.message ?? e}`);
    }
    storeKey = key;
    log(`WebAssembly memory with the data files open: ${memoryMiB().toFixed(0)} MiB`);
  }
  log(overlay ? `the world's overlay is kept in ${overlay}` : 'no overlay is kept for this world');
  // The art comes before a client that would start in the interface it dresses.
  if (dereth.horizonWanted()) await art;
  pluginBundle = await readPlugins();
  await fetchPluginPolicy(msg.pluginPolicy);
  const sequence = (Date.now() % 0x100000000) >>> 0;
  // `?gpu=webgl` draws with WebGL 2 even where the browser has WebGPU.
  return dereth.WebPlay.create(
    msg.canvas, msg.width, msg.height, msg.pageArray, msg.account,
    msg.password, sequence, params.get('gpu') === 'webgl', args, overlay,
  );
}

serve({
  ready,
  start,
  // What a patch wrote into the world's overlay this frame goes to disk.
  each: flushOverlay,
  release: () => {
    releaseFiles();
    closeSettings();
  },
  rate: () => dereth.WebPlay.audioRate(),
  memory: memoryMiB,
});
