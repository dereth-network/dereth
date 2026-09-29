// The player's data files, as the module reads them: a positional read the module calls
// synchronously, over one of three sources (the dev server's ranged reads, files the player picked,
// or the copy in the origin's private file system). The worker that loads the module imports it.

const NAMES = [
  'client_portal.dat',
  'client_cell_1.dat',
  'client_local_English.dat',
  'client_highres.dat',
];

const log = (text) => postMessage({ log: String(text) });

// ---- the data files --------------------------------------------------------------------------

// A reader is `{ size, read(offset, length) -> Uint8Array }`; the read may return fewer bytes only
// at the end of the file.
let readers = [null, null, null, null];
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
// server binds 127.0.0.1 and serves nothing but the four files from the developer's own folder.
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

// The Origin Private File System: the player's copy, kept by the browser for this origin.
function opfsReader(handle) {
  const size = handle.getSize();
  return {
    size,
    read(offset, length) {
      const buf = new Uint8Array(Math.min(length, size - offset));
      handle.read(buf, { at: offset });
      return buf;
    },
  };
}

async function opfsDir() {
  const root = await navigator.storage.getDirectory();
  return root.getDirectoryHandle('dats', { create: true });
}

async function opfsImport(files) {
  const dir = await opfsDir();
  for (const file of files) {
    if (!NAMES.includes(file.name)) continue;
    const t0 = performance.now();
    const fh = await dir.getFileHandle(file.name, { create: true });
    const h = await fh.createSyncAccessHandle();
    h.truncate(0);
    const CHUNK = 16 * 1024 * 1024;
    for (let at = 0; at < file.size; at += CHUNK) {
      const bytes = new Uint8Array(await file.slice(at, at + CHUNK).arrayBuffer());
      h.write(bytes, { at });
    }
    h.flush();
    h.close();
    log(`stored ${file.name} (${(file.size / 1048576).toFixed(0)} MiB) in ${((performance.now() - t0) / 1000).toFixed(1)} s`);
  }
}

async function opfsReaders() {
  const dir = await opfsDir();
  const out = [];
  for (const name of NAMES) {
    try {
      const fh = await dir.getFileHandle(name);
      out.push(opfsReader(await fh.createSyncAccessHandle()));
    } catch {
      out.push(null);
    }
  }
  return out;
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

export async function openFiles(msg, dereth) {
  const t0 = performance.now();
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
  } else if (msg.mode === 'opfs') {
    if (msg.files) await opfsImport(msg.files);
    readers = (await opfsReaders()).map((r) => r && paged(counted(r)));
  }
  const t1 = performance.now();
  log(`dats: ${msg.mode}, ${readers.filter(Boolean).length} of 4 present`);
  if (!readers[0]) return { open: false, ms: t1 - t0 };
  const report = dereth.openDats();
  const t2 = performance.now();
  log(report.trimEnd());
  log(`opened in ${(t2 - t1).toFixed(0)} ms (+${(t1 - t0).toFixed(0)} ms to reach the files): ` +
    `${stats.calls} reads from the module, ${stats.rawReads} from storage, ` +
    `${(stats.rawBytes / 1048576).toFixed(1)} MiB`);
  return { open: readers[0] !== null, ms: t2 - t0 };
}

