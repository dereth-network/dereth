// The Library's downloads, the worker's half: gets a data set from the community's archive into
// this browser's private storage, as the launcher's module's steps say (`getDats*`). The module
// decrypts, checks and unpacks; this does the requests, the reads and the writes. The front page's
// worker imports it.
//
// An archive comes down over several connections at once, each getting the ranges the module hands
// it; the module answers what to write, in order, so the archive kept is always its first bytes.
// The archives are kept in `downloads/` until each is unpacked, so a download stopped part way is
// continued the next time. The set's files are written to its folder (`sets/<id>/`, or
// `auberean/sets/<id>/`), which holds `.incomplete` until every file is in and checked; the page
// never offers such a folder.
//
// One set is got at a time. Its state, as the front page polls it (`status()`):
//   { stage: 'idle' }
//   { stage: 'asking' | 'downloading' | 'checking' | 'unpacking', id, label, archive, archives,
//     done, total }                      (`done`/`total` in bytes of the stage's work)
//   { stage: 'done', id, label, folder, ids, message? }
//      (`ids`: the launcher's sets it made; `message`: the optional files it is without)
//   { stage: 'failed', id, label, reason, message }
//      (`reason`: 'gone', 'over-quota', 'network', 'damaged', 'unsupported', 'space' or 'storage')
//   { stage: 'cancelled', id, label }

import { INCOMPLETE, probeFolder, readSet, releaseHeld } from './datfiles.js';

const log = (text) => postMessage({ log: String(text) });

let job = null;

async function folder(path, create = false) {
  let dir = await navigator.storage.getDirectory();
  for (const part of path.split('/').filter(Boolean)) {
    dir = await dir.getDirectoryHandle(part, { create });
  }
  return dir;
}

async function keptBytes(dir, name) {
  try {
    return (await (await dir.getFileHandle(name)).getFile()).size;
  } catch {
    return 0;
  }
}

function closeAll(handles) {
  for (const h of Object.values(handles)) {
    try {
      h.flush();
      h.close();
    } catch {
      // already closed
    }
  }
}

// The sets on offer, each with what this browser already holds of it: `kept`, the bytes of its
// archives kept from a try before, and `have`, whether its folder holds the set whole.
export async function offers(dereth) {
  const o = JSON.parse(dereth.getDatsOffers());
  const downloads = await folder('downloads', true);
  for (const s of o.sets) {
    s.kept = 0;
    for (const a of s.archives) s.kept += await keptBytes(downloads, a);
    let have = false;
    try {
      const dir = await folder(s.folder);
      have = (await keptBytes(dir, s.files[0])) > 0;
      for await (const [name] of dir.entries()) if (name === INCOMPLETE) have = false;
    } catch {
      // no such folder
    }
    s.have = have;
  }
  return o;
}

export function status() {
  return job ? { ...job.status } : { stage: 'idle' };
}

// Begin getting set `id`. Answers at once; the page polls `status()`.
export async function start(dereth, id) {
  if (job && !job.over) throw new Error(`${job.status.label} is being downloaded already`);
  const o = await offers(dereth);
  const offer = o.sets.find((s) => s.id === id);
  if (!offer) throw new Error(`no data set "${id}" is on offer`);
  if (offer.have) throw new Error(`${offer.label} is in your Library already.`);
  job = { over: false, cancel: false, abort: null, status: { stage: 'asking', id, label: offer.label } };
  const est = await navigator.storage.estimate?.().catch(() => null);
  const need = offer.peak - offer.kept;
  if (est?.quota != null && est.quota - (est.usage ?? 0) < need) {
    const gb = (n) => `${(n / 1e9).toFixed(1)} GB`;
    job.over = true;
    job.status = {
      stage: 'failed', id, label: offer.label, reason: 'space',
      message: `${offer.label} needs ${gb(need)} of this browser's storage; it allows this page ${gb(est.quota - (est.usage ?? 0))} more.`,
    };
    return status();
  }
  run(dereth, job, offer).catch((e) => {
    job.over = true;
    job.status = {
      stage: 'failed', id, label: offer.label, reason: 'storage',
      message: String(e?.message ?? e),
    };
    log(`getting ${offer.label} stopped: ${job.status.message}`);
  });
  return status();
}

// Stop getting the set. The archives downloaded so far are kept, for the next try.
export function cancel() {
  if (!job || job.over) return status();
  job.cancel = true;
  job.abort?.abort();
  return status();
}

// Remove the folder `path` (a set's), and all in it.
async function removeFolder(path) {
  const parts = path.split('/');
  const parent = await folder(parts.slice(0, -1).join('/'), true);
  await parent.removeEntry(parts[parts.length - 1], { recursive: true }).catch(() => {});
}

const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// The archive host's API, answering its proof-of-work challenge when it sets one.
async function api(dereth, url, body, signal) {
  let answer = null;
  for (let i = 0; i < 3; i += 1) {
    const headers = { 'Content-Type': 'application/json' };
    if (answer) headers['X-Hashcash'] = answer;
    const res = await fetch(url, { method: 'POST', body, headers, signal });
    const challenge = res.headers.get('X-Hashcash');
    if (challenge && !answer) {
      log('the archive host set a proof-of-work challenge; answering it');
      answer = dereth.getDatsHashcash(challenge);
      if (!answer) return { error: 'the archive host set a challenge this page cannot answer' };
      continue;
    }
    if (!res.ok) return { error: `HTTP ${res.status}` };
    return { body: await res.text() };
  }
  return { error: 'the archive host kept setting challenges' };
}

async function run(dereth, j, offer) {
  const downloads = await folder('downloads', true);
  const kept = [];
  for (const a of offer.archives) kept.push(await keptBytes(downloads, a));
  dereth.getDatsBegin(offer.id, kept);
  releaseHeld();
  const setDir = await folder(offer.folder, true);
  await setDir.getFileHandle(INCOMPLETE, { create: true });
  const archives = {};
  const outputs = {};
  const archive = async (name) => {
    archives[name] ??= await (await downloads.getFileHandle(name, { create: true })).createSyncAccessHandle();
    return archives[name];
  };
  const progress = () => {
    const p = JSON.parse(dereth.getDatsProgress() ?? 'null');
    if (p) j.status = { ...p, id: offer.id, label: offer.label };
  };
  const t0 = performance.now();
  let steps = 0;
  try {
    for (;;) {
      if (j.cancel) {
        j.status = { stage: 'cancelled', id: offer.id, label: offer.label };
        log(`getting ${offer.label} stopped; what was downloaded is kept for the next try`);
        break;
      }
      const step = JSON.parse(dereth.getDatsNext());
      progress();
      steps += 1;
      // Let the page's questions in now and then: reads and writes are synchronous.
      if (steps % 8 === 0) await pause(0);
      if (step.kind === 'api') {
        if (step.after_ms) await pause(step.after_ms);
        j.abort = new AbortController();
        let reply;
        try {
          reply = await api(dereth, step.url, step.body, j.abort.signal);
        } catch (e) {
          reply = { error: String(e?.message ?? e) };
        }
        if (j.cancel) continue;
        dereth.getDatsApiReply(reply.body, reply.error);
      } else if (step.kind === 'download') {
        const h = await archive(step.archive);
        h.truncate(step.from);
        j.abort = new AbortController();
        log(`downloading ${step.archive} from byte ${step.from} of ${step.to}, over ${step.connections} connections`);
        let sinceFlush = 0;
        let pieces = 0;
        // One connection: it gets the ranges the module hands it until none is left. Each piece's
        // writes are made before the next piece is handed over, so the archive is written in order.
        const connection = async () => {
          for (;;) {
            if (j.cancel) return;
            const next = JSON.parse(dereth.getDatsNextRange());
            if (next.kind === 'finished') return;
            if (next.kind === 'wait') {
              await pause(50);
              continue;
            }
            let status = 200;
            let error;
            try {
              const res = await fetch(next.url, { signal: j.abort.signal });
              status = res.status;
              if (res.status === 200) {
                const reader = res.body.getReader();
                for (;;) {
                  const { done, value } = await reader.read();
                  if (done) break;
                  const writes = dereth.getDatsRangePiece(next.id, value);
                  if (!writes) {
                    await reader.cancel().catch(() => {});
                    break;
                  }
                  for (const [at, bytes] of writes) {
                    h.write(bytes, { at });
                    sinceFlush += bytes.length;
                  }
                  if (sinceFlush > 64 * 1024 * 1024) {
                    h.flush();
                    sinceFlush = 0;
                  }
                  pieces += 1;
                  if (pieces % 16 === 0) progress();
                }
              } else {
                await res.body?.cancel().catch(() => {});
              }
            } catch (e) {
              error = String(e?.message ?? e);
            }
            if (j.cancel) return;
            dereth.getDatsRangeEnded(next.id, status, error);
          }
        };
        const connections = [];
        for (let i = 0; i < Math.max(1, step.connections); i += 1) connections.push(connection());
        await Promise.all(connections);
        h.flush();
      } else if (step.kind === 'read') {
        const h = await archive(step.archive);
        const buf = new Uint8Array(step.len);
        h.read(buf, { at: step.offset });
        for (const [file, offset, bytes] of dereth.getDatsRead(buf)) {
          outputs[file].write(bytes, { at: offset });
        }
      } else if (step.kind === 'prepare') {
        for (const name of step.files) {
          outputs[name] ??= await (await setDir.getFileHandle(name, { create: true })).createSyncAccessHandle();
          outputs[name].truncate(0);
        }
        dereth.getDatsDone();
      } else if (step.kind === 'skip') {
        log(`${offer.label} goes on without ${step.files.join(', ')}: ${step.message}`);
        archives[step.archive]?.close();
        delete archives[step.archive];
        await downloads.removeEntry(step.archive).catch(() => {});
        for (const name of step.files) {
          outputs[name]?.close();
          delete outputs[name];
          await setDir.removeEntry(name).catch(() => {});
        }
        dereth.getDatsDone();
      } else if (step.kind === 'remove') {
        archives[step.archive]?.close();
        delete archives[step.archive];
        await downloads.removeEntry(step.archive).catch(() => {});
        dereth.getDatsDone();
      } else if (step.kind === 'done') {
        closeAll(outputs);
        const why = await probeFolder(step.folder, (names, sizes) => dereth.getDatsCheck(names, sizes));
        if (why) {
          j.status = { stage: 'failed', id: offer.id, label: offer.label, reason: 'damaged', message: why };
          break;
        }
        await setDir.removeEntry(INCOMPLETE);
        const add = offer.game === 'auberean' ? dereth.frontAddAubereanSets : dereth.frontAddSets;
        const ids = await readSet(add, step.folder);
        j.status = { stage: 'done', id: offer.id, label: offer.label, folder: step.folder, ids, message: step.note ?? undefined };
        log(`${offer.label} is in, checked, in ${((performance.now() - t0) / 1000).toFixed(0)} s`);
        break;
      } else if (step.kind === 'failed') {
        j.status = { stage: 'failed', id: offer.id, label: offer.label, reason: step.reason, message: step.message };
        log(`getting ${offer.label} failed (${step.reason}): ${step.message}`);
        break;
      }
    }
  } finally {
    closeAll(archives);
    closeAll(outputs);
    dereth.getDatsEnd();
    if (j.status.stage !== 'done') {
      // A folder not whole is never offered; its files are written again next time.
      await removeFolder(offer.folder);
    }
    j.over = true;
    j.abort = null;
  }
}
