// Auberean's worker, from a page's first play of it: loads the game's module, opens the player's data files, and
// brings the client up drawing into the canvas the page handed it. The rest of a play is what every
// game's worker does (`game-loop.js`). A browser cannot open the friends service's stream, so the
// player is without friends here.

import init, * as auberean from './pkg-auberean/auberean_web.js';
import { filesStamp, openAubereanFiles, readStats, releaseFiles } from './datfiles.js';
import { serve } from './game-loop.js';

const log = (text) => postMessage({ log: String(text) });
const params = new URLSearchParams(location.search);

let wasm = null;
const ready = init().then(async (exports) => {
  wasm = exports;
  // `?log=debug` (or `trace`, `warn`, ...) sets how much of the client's log reaches the console.
  auberean.start(params.get('log') || 'info');
  log(`Auberean ${auberean.version()}`);
  await openSettings();
});

const memoryMiB = () => (wasm?.memory?.buffer?.byteLength ?? 0) / 1048576;

// Auberean's own files (preferences, key maps, what its scripts keep), kept in one file in the
// origin's private storage, apart from Dereth's. Its access handle is synchronous, so a save is on
// disk before the client's call returns. Memory only where there is no such storage.
let settings = null;
async function openSettings() {
  try {
    const root = await navigator.storage.getDirectory();
    const dir = await root.getDirectoryHandle('auberean', { create: true });
    const file = await dir.getFileHandle('settings.bin', { create: true });
    settings = await file.createSyncAccessHandle();
  } catch (e) {
    log(`settings kept for this page only: ${e?.message ?? e}`);
  }
}
globalThis.aubereanSettingsLoad = () => {
  if (!settings) return new Uint8Array(0);
  const bytes = new Uint8Array(settings.getSize());
  settings.read(bytes, { at: 0 });
  return bytes;
};
globalThis.aubereanSettingsSave = (bytes) => {
  if (!settings) return;
  settings.truncate(0);
  settings.write(bytes, { at: 0 });
  settings.flush();
};

// The world opened over the data files, by the files: a later start on the same ones takes the files
// up again and has no need to open it anew.
let openKey = null;

// Bring the client up for the world `msg` names.
async function start(msg) {
  if (!settings) await openSettings();
  await openAubereanFiles(msg.dats, auberean.dataFiles());
  const key = filesStamp(msg.dats);
  if (openKey !== key) {
    openKey = null;
    const t0 = performance.now();
    try {
      log(`data files: ${auberean.openDats()}`);
    } catch (e) {
      throw new Error(`The data files did not open: ${e?.message ?? e}`);
    }
    openKey = key;
    log(`opened in ${(performance.now() - t0).toFixed(0)} ms: ${readStats()}`);
    log(`WebAssembly memory with the data files open: ${memoryMiB().toFixed(0)} MiB`);
  }
  // `?gpu=webgl` draws with WebGL 2 even where the browser has WebGPU.
  return auberean.WebPlay.create(msg.canvas, msg.pageArray, msg.account, msg.password, params.get('gpu') === 'webgl', ticketMayRide(msg.server));
}

// The friends service's stream rides the game's WebSocket with its ticket: over TLS, or to this
// computer.
function ticketMayRide(server) {
  try {
    const url = new URL(server);
    return url.protocol === 'wss:' || ['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname);
  } catch {
    return false;
  }
}

serve({
  ready,
  start,
  release: () => {
    releaseFiles();
    settings?.flush();
    settings?.close();
    settings = null;
  },
  rate: () => auberean.WebPlay.audioRate(),
  memory: memoryMiB,
});
