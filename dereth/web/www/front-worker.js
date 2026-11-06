// The front page's worker: answers the launcher's page with the launcher's own logic (a small module
// that holds neither game, over the player's files kept in this browser), and fetches what plugin
// sources offer. A game is played in a worker of its own, which takes over what the two would both
// hold in the browser's storage: a file there has one synchronous handle at a time, so this worker
// lets go of its handles for a play (`suspend`), and takes them again when it ends (`resume`).

import init, * as dereth from './pkg-front/dereth_web_front.js';
import {
  FULL, NAMES, holdOverlay, listOverlays, releaseFiles, removeOverlay, removeSet, removeSetFiles,
  scanAubereanSets, scanSets, storeAubereanSet, storeSet,
} from './datfiles.js';

const log = (text) => postMessage({ log: String(text) });

const ready = init().then(async () => {
  dereth.start();
  // The client's own files first: the overlay blocklist the front page shows is one of them.
  await openSettings();
  dereth.installSettings();
  // The plugin sources and what was installed from them, as this browser kept them.
  await openShelf();
});

// The client's own files (preferences, keymaps, layouts), kept in one file in the origin's private
// storage. Its access handle is synchronous, so a save is on disk before the client's call
// returns, which is what lets the page's close save them. Memory only where there is no such
// storage.
let settings = null;
async function openSettings(tries = 1) {
  for (let left = tries; left > 0; left -= 1) {
    try {
      const root = await navigator.storage.getDirectory();
      const file = await root.getFileHandle('settings.bin', { create: true });
      settings = await file.createSyncAccessHandle();
      return;
    } catch (e) {
      if (left === 1) log(`settings kept for this page only: ${e?.message ?? e}`);
      else await new Promise((resolve) => { setTimeout(resolve, 100); });
    }
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

// Plugins the player added to this page, under `plugins/` in the origin's private storage.
async function pluginsDir(create) {
  const root = await navigator.storage.getDirectory();
  return root.getDirectoryHandle('plugins', { create });
}
// Plugin sources, as the desktop launcher has them: the module keeps the launcher's own shelf (the
// sources, the last list each gave, what was installed from them and the update setting) and says
// what to keep; this worker fetches the lists and the zips, and keeps the files in the origin's
// private storage: the sources beside the settings file, and each installed plugin's zip, with the
// record of them, among the plugins the client finds (`plugins/`). The only requests are for the
// lists and zips of Dereth's source and the sources the player added, to their own hosts.
async function writeFile(dir, name, bytes) {
  const handle = await dir.getFileHandle(name, { create: true });
  const access = await handle.createSyncAccessHandle();
  access.truncate(0);
  access.write(bytes, { at: 0 });
  access.flush();
  access.close();
}
// The folder a path in the origin's private storage is in, and its name there.
async function placeOf(path, create) {
  const names = path.split('/').filter((n) => n && n !== '.' && n !== '..');
  let dir = await navigator.storage.getDirectory();
  for (const n of names.slice(0, -1)) dir = await dir.getDirectoryHandle(n, { create });
  return { dir, name: names[names.length - 1] };
}
async function readKept(path) {
  try {
    const { dir, name } = await placeOf(path, false);
    const file = await (await dir.getFileHandle(name)).getFile();
    return new Uint8Array(await file.arrayBuffer());
  } catch {
    return undefined;
  }
}
async function openShelf() {
  const [sources, installed] = dereth.frontPluginRecords();
  dereth.frontPluginsOpen(await readKept(sources), await readKept(installed));
}
// Make the writes the module asks for, in order. When one fails the rest are not made, and the
// shelf is read again from what is kept.
async function keepShelf() {
  try {
    for (const [path, bytes] of dereth.frontPluginWrites()) {
      const { dir, name } = await placeOf(path, true);
      if (bytes) await writeFile(dir, name, bytes);
      else await dir.removeEntry(name).catch(() => {});
    }
  } catch (e) {
    await openShelf();
    throw e;
  }
}
// One download for the plugins, of at most `limit` bytes: `{ body }`, `{ error }` with what its
// host answered, or `{}` when the page could not reach it at all: offline, or a host that does not
// let web pages read it, which a page cannot tell apart.
async function fetchCapped(url, limit) {
  let res;
  try {
    res = await fetch(url, { cache: 'no-cache' });
  } catch {
    return {};
  }
  if (!res.ok) return { error: `HTTP ${res.status}` };
  const tooLarge = { error: `${url} is larger than its source says` };
  if (Number(res.headers.get('content-length')) > limit) return tooLarge;
  try {
    const reader = res.body.getReader();
    const parts = [];
    let size = 0;
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.length;
      if (size > limit) {
        reader.cancel().catch(() => {});
        return tooLarge;
      }
      parts.push(value);
    }
    const body = new Uint8Array(size);
    let at = 0;
    for (const p of parts) {
      body.set(p, at);
      at += p.length;
    }
    return { body };
  } catch {
    return {};
  }
}
// One plugin call at a time: each reads and writes the same files.
let pluginTurn = Promise.resolve();
function inTurn(work) {
  const run = pluginTurn.then(work);
  pluginTurn = run.catch(() => {});
  return run;
}
// Read every source, and apply the updates that apply on their own: the launcher's walk, each
// download made here.
async function refreshPlugins() {
  let next = dereth.frontPluginRefreshStart();
  while (next) {
    const { url, limit } = JSON.parse(next);
    const got = await fetchCapped(url, limit);
    next = dereth.frontPluginRefreshNext(got.body, got.error);
    await keepShelf();
  }
  const done = JSON.parse(dereth.frontPluginRefreshDone());
  if (done.said) log(`plugins: ${done.said.replace(/\n/g, ' ')}`);
  return done;
}
async function addSource(url) {
  const list = JSON.parse(dereth.frontPluginAddSource(url));
  const got = await fetchCapped(list.url, list.limit);
  const said = dereth.frontPluginSourceRead(list.url, got.body, got.error);
  await keepShelf();
  return said;
}
// Install plugin `id` from `source`, or with no source update it from its own.
async function takePlugin(source, id) {
  const { url, limit } = JSON.parse(dereth.frontPluginPlan(source, id));
  const got = await fetchCapped(url, limit);
  const said = dereth.frontPluginTake(source, id, got.body, got.error);
  await keepShelf();
  log(said);
  return said;
}
async function changeShelf(change) {
  change();
  await keepShelf();
}
// Keep the files of one added plugin folder, or added zips: `files` is [{ path, bytes }], each
// path starting with the folder's name, or a zip's file name alone.
async function addPlugins(files) {
  const dir = await pluginsDir(true);
  for (const f of files) {
    const names = f.path.split('/').filter((n) => n && n !== '.' && n !== '..');
    let at = dir;
    for (const n of names.slice(0, -1)) at = await at.getDirectoryHandle(n, { create: true });
    const handle = await at.getFileHandle(names[names.length - 1], { create: true });
    const access = await handle.createSyncAccessHandle();
    access.truncate(0);
    access.write(f.bytes, { at: 0 });
    access.flush();
    access.close();
  }
  log(`kept ${files.length} plugin file(s); turn each on in the client with /plugin`);
}
// The plugins the player added by hand (a plugin's folder, or a zip), by their names in `plugins/`:
// every entry there but the record of what was installed from sources and those plugins' zips.
async function addedPlugins() {
  const kept = new Set([
    ...JSON.parse(dereth.frontPlugins()).installed.map((p) => dereth.frontPluginZipPath(p.id)),
    dereth.frontPluginRecords()[1],
  ].map((path) => path.split('/').pop()));
  const out = [];
  try {
    for await (const [name, handle] of (await pluginsDir(false)).entries()) {
      if (!kept.has(name)) out.push({ name, folder: handle.kind === 'directory' });
    }
  } catch {
    // No plugins kept.
  }
  return out.sort((a, b) => a.name.localeCompare(b.name));
}
async function removeAddedPlugin(name) {
  if (!name || name.includes('/')) return;
  await (await pluginsDir(false)).removeEntry(name, { recursive: true }).catch(() => {});
  log(`removed the plugin ${name}`);
}
async function removePlugins() {
  const root = await navigator.storage.getDirectory();
  await root.removeEntry('plugins', { recursive: true }).catch(() => {});
  // The record of what was installed from sources went with them; the sources stay.
  await openShelf();
  log('removed every added plugin');
}

// ---- the front page's launcher ---------------------------------------------------------------

// What the front page asks of the launcher: the module's own calls, and the files kept in this
// browser. Each answers a value, or throws with what to tell the player.
const calls = {
  listStart: (named, hub) => dereth.frontListStart(named || undefined, hub || undefined),
  listNext: (body, error) => dereth.frontListNext(body ?? undefined, error ?? undefined),
  listDone: (now) => JSON.parse(dereth.frontListDone(now)),
  listWanted: (now) => dereth.frontListWanted(now),
  listCopy: (copy, named, hub) => dereth.frontListCopy(copy, named || undefined, hub || undefined),
  hubStart: (named, hub, now) => dereth.frontHubStart(named || undefined, hub || undefined, now),
  hubNext: (body, error) => dereth.frontHubNext(body ?? undefined, error ?? undefined),
  hubDone: (now) => dereth.frontHubDone(now),
  setState: (json) => dereth.frontSetState(json),
  state: () => dereth.frontState(),
  status: (slug, text) => dereth.frontStatus(slug, new TextEncoder().encode(text)),
  statusFailed: (slug) => dereth.frontStatusFailed(slug),
  worlds: () => JSON.parse(dereth.frontWorlds()),
  world: (slug) => {
    const page = dereth.frontWorld(slug);
    return page ? JSON.parse(page) : null;
  },
  addWorld: (game, name, host, port, ruleset, emulator) => dereth.frontAddWorld(game, name, host, port, ruleset, emulator),
  addedBy: (slug, url) => dereth.frontAddedBy(slug, url),
  removeWorld: (slug) => dereth.frontRemoveWorld(slug),
  recordLaunch: (slug, prefs) => dereth.frontRecordLaunch(slug, JSON.stringify(prefs)),
  saveFavourite: (favourite) => dereth.frontSaveFavourite(JSON.stringify(favourite)),
  removeFavourite: (id) => dereth.frontRemoveFavourite(id),
  setWorldFavorite: (slug, favorite) => dereth.frontSetWorldFavorite(slug, favorite),
  forgetAccount: (slug, username) => dereth.frontForgetAccount(slug, username),
  forgetAllAccounts: () => dereth.frontForgetAllAccounts(),
  setDefaultSet: (id) => dereth.frontSetDefaultSet(id),
  interface: () => JSON.parse(dereth.frontInterface()),
  setInterface: (name) => dereth.frontSetInterface(name),
  version: () => dereth.version(),
  serverProblem: (url) => dereth.serverUrlProblem(url),
  setRealm: (realm) => dereth.frontSetRealm(realm),
  scanSets: (known, again) => scanSets(dereth, known, again),
  storeSet: (name, files) => storeSet(dereth, name, files),
  removeSet: (path) => removeSet(dereth, path),
  // One kind's files of a kept set: the later four, or the older pair.
  removeSetKind: (path, kind) => removeSetFiles(dereth, path, kind === 'classic' ? NAMES.slice(4) : NAMES.slice(0, 4)),
  // Asheron's Call 2's data sets, kept apart from Asheron's Call's: its portal file has the name of
  // Asheron's Call's older one.
  scanAubereanSets: (known, again) => scanAubereanSets(dereth, known, again),
  aubereanSetProblem: (names) => dereth.frontAubereanSetProblem(names),
  storeAubereanSet: (name, files) => storeAubereanSet(dereth, name, files),
  aubereanCheck: () => JSON.parse(dereth.frontAubereanCheck()),
  plugins: () => JSON.parse(dereth.frontPlugins()),
  pluginRefresh: () => inTurn(refreshPlugins),
  pluginAddSource: (url) => inTurn(() => addSource(url)),
  pluginRemoveSource: (url) => inTurn(() => changeShelf(() => dereth.frontPluginRemoveSource(url))),
  installPlugin: (source, id) => inTurn(() => takePlugin(source, id)),
  updatePlugin: (id) => inTurn(() => takePlugin(undefined, id)),
  removePlugin: (id) => inTurn(() => changeShelf(() => dereth.frontPluginRemove(id))),
  pluginUpdates: (setting) => inTurn(() => changeShelf(() => dereth.frontPluginUpdates(setting))),
  removePlugins: () => inTurn(removePlugins),
  addedPlugins: () => inTurn(addedPlugins),
  removeAddedPlugin: (name) => inTurn(() => removeAddedPlugin(name)),
  addPlugins: (files) => inTurn(() => addPlugins(files)),
  overlayFolder: (address) => dereth.overlayFolder(address),
  // The world's overlay as the folder holds it: held open for the world being looked at.
  overlay: async (path) => {
    const held = await holdOverlay(path);
    dereth.holdOverlay(held);
    return held ? { ...JSON.parse(dereth.overlayInfo()), held } : null;
  },
  overlays: () => listOverlays(),
  removeOverlay: (path) => removeOverlay(path),
  blocklist: () => dereth.overlayBlocklist(),
  setBlocked: (key, on) => dereth.setOverlayBlocked(key, on),
  storage: async () => {
    const e = await navigator.storage?.estimate?.().catch(() => null);
    const persisted = await navigator.storage?.persisted?.().catch(() => false);
    return { usage: e?.usage ?? null, quota: e?.quota ?? null, persisted: !!persisted };
  },
};

// A play is about to start, or has ended. A game ending lets go of what it held just before it
// says so, and a browser may take a moment more: the settings file is tried again for a while
// before it is taken to be another tab's.
let suspended = false;
function suspend() {
  suspended = true;
  releaseFiles();
  settings?.flush();
  settings?.close();
  settings = null;
}
async function resume() {
  await openSettings(20);
  dereth.installSettings();
  suspended = false;
}

onmessage = async (ev) => {
  await ready;
  const msg = ev.data;
  try {
    let value;
    if (msg.call === 'suspend') value = suspend();
    else if (msg.call === 'resume') value = await resume();
    else if (suspended) throw new Error('the client is running');
    else value = await calls[msg.call](...(msg.args ?? []));
    postMessage({ reply: msg.id, value });
  } catch (e) {
    const error = String(e?.message ?? e);
    postMessage({ reply: msg.id, error: e?.name === 'QuotaExceededError' ? FULL : error });
  }
};
