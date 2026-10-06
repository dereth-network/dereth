// The player's files in this browser, as the module reads them: the data files (a positional read
// the module calls synchronously, over the dev server's ranged reads, files picked for one visit,
// or a data set kept in the origin's private file system), the data sets kept there, and a world's
// overlay kept there too. The worker that loads the module imports it.
//
// The six data files: the four of the later set, then the two of the set from before Throne of
// Destiny (`portal.dat` and `cell.dat`), which the classic interface and the early worlds draw from.
// The module's numbering is this order.
//
// The origin's private file system holds:
//   sets/<name>/       one data set the player added: any of the six files
//   dats/              the files an earlier version of this page kept (read as a set like the rest)
//   overlays/<folder>/ one world's overlay: overlay_portal.dat, overlay_cell.dat, ... (the folder
//                      is the world's address, as the desktop client names its per-server folder)
//   settings.bin       the client's own files (preferences, keymaps, the overlay blocklist)

export const NAMES = [
  'client_portal.dat',
  'client_cell_1.dat',
  'client_local_English.dat',
  'client_highres.dat',
  'portal.dat',
  'cell.dat',
];

const OVERLAY_NAMES = [
  'overlay_portal.dat',
  'overlay_cell.dat',
  'overlay_local.dat',
  'overlay_highres.dat',
];

const log = (text) => postMessage({ log: String(text) });

// A storage write the browser refused for want of room.
const isFull = (e) => e?.name === 'QuotaExceededError';
export const FULL = "The browser's storage for this site is full: remove a data set or an overlay you no longer need, or free space on this machine.";

// ---- the data files --------------------------------------------------------------------------

// A reader is `{ size, read(offset, length) -> Uint8Array }`; the read may return fewer bytes only
// at the end of the file.
let readers = NAMES.map(() => null);
const stats = { calls: 0, rawReads: 0, rawBytes: 0 };

// Pages keep the many small reads of a directory walk or a block chain from each becoming a
// request. Only the slow sources use them.
// A directory walk touches many small nodes spread through the file, so a local source reads
// small pages; the dev server's pages are larger, because each is a request.
const PAGE_LOCAL = 16 * 1024;
const PAGE_HTTP = 64 * 1024;
const CACHE_BYTES = 32 * 1024 * 1024;

function paged(reader, PAGE = PAGE_LOCAL) {
  const MAX_PAGES = Math.max(64, Math.floor(CACHE_BYTES / PAGE));
  const pages = new Map();
  return {
    size: reader.size,
    read(offset, length) {
      const out = new Uint8Array(length);
      let done = 0;
      while (done < length) {
        const at = offset + done;
        const index = Math.floor(at / PAGE);
        let page = pages.get(index);
        if (page) {
          pages.delete(index);
        } else {
          page = reader.read(index * PAGE, Math.min(PAGE, reader.size - index * PAGE));
          if (pages.size >= MAX_PAGES) pages.delete(pages.keys().next().value);
        }
        pages.set(index, page);
        const from = at - index * PAGE;
        const take = Math.min(length - done, page.length - from);
        if (take <= 0) break;
        out.set(page.subarray(from, from + take), done);
        done += take;
      }
      return done === length ? out : out.subarray(0, done);
    },
  };
}

function counted(reader) {
  return {
    size: reader.size,
    read(offset, length) {
      stats.rawReads += 1;
      stats.rawBytes += length;
      return reader.read(offset, length);
    },
  };
}

// The dev server's ranged GETs, synchronous because a worker may block. Development only: the
// server binds 127.0.0.1 and serves nothing but the data files from the developer's own folder.
function httpReader(name) {
  const head = new XMLHttpRequest();
  head.open('HEAD', `/dats/${name}`, false);
  head.send();
  if (head.status !== 200) return null;
  const size = Number(head.getResponseHeader('Content-Length'));
  return {
    size,
    read(offset, length) {
      const x = new XMLHttpRequest();
      x.open('GET', `/dats/${name}`, false);
      x.responseType = 'arraybuffer';
      x.setRequestHeader('Range', `bytes=${offset}-${offset + length - 1}`);
      x.send();
      if (x.status !== 206) throw new Error(`${name}: HTTP ${x.status}`);
      return new Uint8Array(x.response);
    },
  };
}

// The files the player picked, read in place with no copy. Lost when the page closes.
function fileReader(file) {
  const fr = new FileReaderSync();
  return {
    size: file.size,
    read(offset, length) {
      return new Uint8Array(fr.readAsArrayBuffer(file.slice(offset, offset + length)));
    },
  };
}

// A file kept in the origin's private file system, through its synchronous access handle.
function handleReader(handle) {
  const size = handle.getSize();
  return {
    size,
    read(offset, length) {
      const buf = new Uint8Array(Math.max(0, Math.min(length, size - offset)));
      handle.read(buf, { at: offset });
      return buf;
    },
  };
}

// A folder of the origin's private file system by its path (`sets/eor`), made when `create`.
async function folder(path, create = false) {
  let dir = await navigator.storage.getDirectory();
  for (const part of path.split('/').filter(Boolean)) {
    dir = await dir.getDirectoryHandle(part, { create });
  }
  return dir;
}

async function folderOrNull(path) {
  try {
    return await folder(path);
  } catch {
    return null;
  }
}

// A file takes one synchronous handle at a time, so the ones the data files are read through are
// closed before the files are reached again (another start in the same page, or storing over them),
// or that would be refused.
let held = [];
function close(handles) {
  for (const h of handles) {
    try {
      h.flush?.();
      h.close();
    } catch {
      // already closed
    }
  }
}
function releaseHeld() {
  close(held);
  held = [];
}

// A synchronous handle on `name` in `dir`, or null when the folder has no such file. A file another
// tab of this site holds open cannot be opened here.
async function handleOn(dir, name, create = false) {
  let fh;
  try {
    fh = await dir.getFileHandle(name, { create });
  } catch {
    return null;
  }
  try {
    return await fh.createSyncAccessHandle();
  } catch (e) {
    throw new Error(`${name} is open in another tab of this page; close it and try again (${e?.name ?? e})`);
  }
}

globalThis.derethDatPresent = (file) => readers[file] !== null;
globalThis.derethDatRead = (file, offset, buf) => {
  stats.calls += 1;
  const r = readers[file];
  if (!r || offset + buf.length > r.size) return false;
  const bytes = r.read(offset, buf.length);
  if (bytes.length !== buf.length) return false;
  buf.set(bytes);
  return true;
};

// Reach the files `msg` names: `{ mode: 'http' }` (the dev runner's), `{ mode: 'files', files }`
// (picked for this visit), or `{ mode: 'sets', modern, classic }` (the later files from the kept
// set in folder `modern`, the older pair from the one in `classic`; either may be null). `true`
// when there is a world to open: the later set's portal, or the older set's.
export async function openFiles(msg) {
  const t0 = performance.now();
  releaseHeld();
  if (msg.mode === 'http') {
    readers = NAMES.map((n) => {
      const r = httpReader(n);
      return r && paged(counted(r), PAGE_HTTP);
    });
  } else if (msg.mode === 'files') {
    readers = NAMES.map((n) => {
      const f = msg.files.find((x) => x.name === n);
      return f ? paged(counted(fileReader(f))) : null;
    });
  } else if (msg.mode === 'sets') {
    readers = NAMES.map(() => null);
    const from = async (path, indices) => {
      const dir = path && (await folderOrNull(path));
      if (!dir) return;
      for (const i of indices) {
        const h = await handleOn(dir, NAMES[i]);
        if (!h) continue;
        held.push(h);
        readers[i] = paged(counted(handleReader(h)));
      }
    };
    await from(msg.modern, [0, 1, 2, 3]);
    await from(msg.classic, [4, 5]);
  }
  const present = NAMES.filter((_, i) => readers[i]);
  log(`dats: ${msg.mode}, ${present.length} of ${NAMES.length} present (${present.join(', ')}); ` +
    `${(performance.now() - t0).toFixed(0)} ms to reach them`);
  return readers[0] !== null || readers[4] !== null;
}

// Open the files the world is drawn from as the module's store: `args` are what the launcher says
// about the world, and `overlay` the folder its overlay is held in ('' for none). Throws when a file
// the world needs is missing.
export function openStore(dereth, args, overlay) {
  const t0 = performance.now();
  const calls = stats.calls;
  const report = dereth.openDats(args, overlay);
  log(report.trimEnd());
  log(`opened in ${(performance.now() - t0).toFixed(0)} ms: ${stats.calls - calls} reads from the ` +
    `module, ${stats.rawReads} from storage in all, ${(stats.rawBytes / 1048576).toFixed(1)} MiB`);
}

// ---- the data sets kept in this browser ------------------------------------------------------

// The files a set's folder holds, read for what each reports: the module reads each through
// `derethProbeRead`, by its place in the list.
let probing = [];
globalThis.derethProbeRead = (file, offset, buf) => {
  const h = probing[file];
  if (!h || offset + buf.length > h.getSize()) return false;
  return h.read(buf, { at: offset }) === buf.length;
};

// Read the set kept in folder `path` into the launcher's state: answers its sets' ids.
async function readSet(dereth, path) {
  const dir = await folderOrNull(path);
  if (!dir) return [];
  const names = [];
  const sizes = [];
  probing = [];
  try {
    for await (const [name, entry] of dir.entries()) {
      if (entry.kind !== 'file' || !name.toLowerCase().endsWith('.dat')) continue;
      const h = await handleOn(dir, name);
      if (!h) continue;
      probing.push(h);
      names.push(name);
      sizes.push(h.getSize());
    }
    return dereth.frontAddSets(path, names, sizes);
  } finally {
    close(probing);
    probing = [];
  }
}

// Every set kept in this browser, read: the folders under `sets/`, and `dats/` from before.
// Folders the launcher's state already knows are not read again unless `again`. Answers
// `[{ path, bytes }]`.
export async function scanSets(dereth, known, again = false) {
  const out = [];
  const visit = async (path, dir) => {
    let bytes = 0;
    let any = false;
    for await (const [name, entry] of dir.entries()) {
      if (entry.kind !== 'file') continue;
      if (NAMES.includes(name)) any = true;
      bytes += (await entry.getFile()).size;
    }
    if (!any) return;
    if (again || !known.includes(path)) await readSet(dereth, path);
    out.push({ path, bytes });
  };
  const legacy = await folderOrNull('dats');
  if (legacy) await visit('dats', legacy);
  const sets = await folderOrNull('sets');
  if (sets) {
    for await (const [name, entry] of sets.entries()) {
      if (entry.kind === 'directory') await visit(`sets/${name}`, entry);
    }
  }
  return out;
}

// Keep the files the player picked as a set in `sets/<name>`, and read it: answers its sets' ids.
export async function storeSet(dereth, name, files) {
  releaseHeld();
  const path = `sets/${name}`;
  const dir = await folder(path, true);
  for (const file of files) {
    if (!NAMES.includes(file.name)) continue;
    const t0 = performance.now();
    const h = await handleOn(dir, file.name, true);
    try {
      h.truncate(0);
      const CHUNK = 16 * 1024 * 1024;
      for (let at = 0; at < file.size; at += CHUNK) {
        const bytes = new Uint8Array(await file.slice(at, at + CHUNK).arrayBuffer());
        h.write(bytes, { at });
      }
      h.flush();
    } catch (e) {
      close([h]);
      if (isFull(e)) {
        await dir.removeEntry(file.name).catch(() => {});
        throw new Error(FULL);
      }
      throw e;
    }
    close([h]);
    log(`stored ${file.name} (${(file.size / 1048576).toFixed(0)} MiB) in ${path} in ${((performance.now() - t0) / 1000).toFixed(1)} s`);
  }
  return readSet(dereth, path);
}

// Remove the set kept in folder `path`, files and all.
export async function removeSet(dereth, path) {
  releaseHeld();
  const parts = path.split('/');
  const parent = parts.length > 1 ? await folder(parts.slice(0, -1).join('/')) : await navigator.storage.getDirectory();
  await parent.removeEntry(parts[parts.length - 1], { recursive: true }).catch(() => {});
  dereth.frontRemoveSets(path);
}

// ---- a world's overlay -----------------------------------------------------------------------

// The world's overlay folder the worker holds open: its path and its four containers' handles. A
// container of no length is no container.
let overlay = { path: '', handles: {}, dirty: false };

function overlayHandle(name) {
  return overlay.handles[name] ?? null;
}

globalThis.derethOverlaySize = (name) => overlayHandle(name)?.getSize() ?? 0;
globalThis.derethOverlayRead = (name, offset, buf) => {
  const h = overlayHandle(name);
  if (!h || offset + buf.length > h.getSize()) return false;
  return h.read(buf, { at: offset }) === buf.length;
};
globalThis.derethOverlayWrite = (name, offset, bytes) => {
  const h = overlayHandle(name);
  if (!h) return false;
  try {
    const n = h.write(bytes, { at: offset });
    overlay.dirty = true;
    return n === bytes.length;
  } catch (e) {
    log(`the overlay could not be written: ${isFull(e) ? FULL : e?.message ?? e}`);
    return false;
  }
};
globalThis.derethOverlayTruncate = (name) => {
  const h = overlayHandle(name);
  if (!h) return false;
  try {
    h.truncate(0);
    overlay.dirty = true;
    return true;
  } catch (e) {
    log(`the overlay could not be written: ${isFull(e) ? FULL : e?.message ?? e}`);
    return false;
  }
};

// Put what was written since the last call on disk. The worker calls it once a frame.
export function flushOverlay() {
  if (!overlay.dirty) return;
  overlay.dirty = false;
  for (const h of Object.values(overlay.handles)) {
    try {
      h.flush();
    } catch {
      // closed
    }
  }
}

function closeOverlay() {
  flushOverlay();
  close(Object.values(overlay.handles));
  overlay = { path: '', handles: {}, dirty: false };
}

// Hold the overlay folder `path` (`overlays/<address>`) open for the client: answers the path, or
// '' when no overlay can be kept (no such storage here, or another tab of this site holds it).
export async function holdOverlay(path) {
  if (overlay.path === path) return path;
  closeOverlay();
  if (!path) return '';
  try {
    const dir = await folder(path, true);
    const handles = {};
    for (const name of OVERLAY_NAMES) {
      handles[name] = await handleOn(dir, name, true);
    }
    overlay = { path, handles, dirty: false };
    return path;
  } catch (e) {
    log(`no overlay is kept for this world this visit: ${e?.message ?? e}`);
    closeOverlay();
    return '';
  }
}

// Every world's overlay kept in this browser: `[{ path, bytes }]`, empty folders left out.
export async function listOverlays() {
  const out = [];
  const top = await folderOrNull('overlays');
  if (!top) return out;
  for await (const [name, entry] of top.entries()) {
    if (entry.kind !== 'directory') continue;
    let bytes = 0;
    for await (const [, f] of entry.entries()) {
      if (f.kind === 'file') bytes += (await f.getFile()).size;
    }
    if (bytes > 0) out.push({ path: `overlays/${name}`, bytes });
  }
  return out;
}

// Remove the world's overlay kept in `path`, containers and all.
export async function removeOverlay(path) {
  if (overlay.path === path) closeOverlay();
  const top = await folderOrNull('overlays');
  await top?.removeEntry(path.split('/').pop(), { recursive: true }).catch(() => {});
}
