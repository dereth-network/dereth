// The front page: pick a world or type an address, choose or add the data files, connect. The
// worlds come from the community list, the launcher's table of worlds that run a client of their
// own, and the servers the player adds; a world's page names the data sets kept in this browser
// that it takes (matched by what the files report, as the desktop library matches them), its
// overlay, and how it is reached: directly, for an Empyrean world with its WebSocket endpoint on,
// or through dereth-web-relay on this machine for any other. What the client is told about the
// world (its era, base, systems, logon version and rules) is the launcher's one description of it. The logic is the launcher's own, in the module: this page asks the worker, which runs
// it, and keeps what the launcher keeps (its state record, the day's copy of the list) in the
// browser's local storage.
//
// A browser cannot ask a server over UDP whether it is up, so a world with no status document is
// shown with its state unknown. An Empyrean world added by its status address is asked over HTTP.

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);

const RELAY = 'ws://127.0.0.1:9180/';
const RELAY_README = 'https://github.com/dereth-network/dereth/blob/main/tools/web-relay/README.md';
const STATE_KEY = 'dereth.launcher';
const LIST_KEY = 'dereth.worldlist';
const CONNECT_KEY = 'dereth.connect';
const CHOSEN_KEY = 'dereth.world';

// Local storage may refuse (private browsing): the launcher then forgets between visits.
function load(key, fallback) {
  try {
    const v = localStorage.getItem(key);
    return v === null ? fallback : JSON.parse(v);
  } catch {
    return fallback;
  }
}
function keep(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Not kept; see `load`.
  }
}

const EMULATORS = { empyrean: 'Empyrean', ace: 'ACE', classic_ace: 'ClassicACE', gdle: 'GDLE', unknown: 'Unknown' };
const STATES = { online: 'up', high: 'busy', starting: 'starting', offline: 'down', unknown: 'unknown' };
const kib = (n) => (n >= 1048576 ? `${(n / 1048576).toFixed(n >= 1073741824 ? 0 : 1)} MiB` : `${Math.max(1, Math.round(n / 1024))} KiB`);
const setName = (path) => (path === 'dats' ? 'Kept before' : String(path).split('/').pop());
const iterLabel = (files) => files.map((f) => `${f.role} ${f.iterations ?? '?'}`).join(', ');

export function startFront({ call, launch, params, log }) {
  const ui = {
    worlds: [], eras: [], features: [], page: null, slug: load(CHOSEN_KEY, null),
    search: '', kept: [], overlays: [], blocklist: [], picked: null, adding: null, direct: {},
    connect: load(CONNECT_KEY, {}),
  };
  const save = async () => keep(STATE_KEY, await call('state'));
  const state = async () => JSON.parse(await call('state'));
  const eraLabel = (name) => ui.eras.find((e) => e.name === name)?.label ?? name;

  // ---- the world list -----------------------------------------------------------------------

  async function fetchList(force) {
    const cached = load(LIST_KEY, null);
    const now = Date.now() / 1000;
    if (!force && cached && (await call('listFresh', cached.fetched_at, now))) {
      await call('list', cached.list);
      return '';
    }
    const url = params.get('list') || (await call('listUrl'));
    try {
      const res = await fetch(url, { cache: 'no-store' });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const xml = await res.text();
      const n = await call('list', xml);
      keep(LIST_KEY, { fetched_at: Math.floor(now), list: xml });
      log(`world list: ${n} worlds from ${url}`);
      return '';
    } catch (e) {
      if (cached) {
        await call('list', cached.list).catch(() => {});
        return `The world list did not answer (${e?.message ?? e}); showing the copy from ${new Date(cached.fetched_at * 1000).toLocaleString()}.`;
      }
      return `The world list did not answer (${e?.message ?? e}). Add a server by its address.`;
    }
  }

  // A world added by a status address is asked for its status; its WebSocket URL comes from it.
  async function askStatus(slug) {
    const url = ui.connect[slug];
    if (!url || !/^https?:\/\//i.test(url)) return null;
    try {
      const target = new URL(url);
      if (target.pathname === '/' || target.pathname === '') target.pathname = '/status';
      const text = await (await fetch(target, { cache: 'no-store' })).text();
      await call('status', slug, text);
      const status = JSON.parse(text);
      ui.direct[slug] = !!status.websocket_url;
      return status;
    } catch (e) {
      log(`status of ${url}: ${e?.message ?? e}`);
      await call('statusFailed', slug);
      return null;
    }
  }

  async function refresh() {
    const all = await call('worlds');
    ui.worlds = all.worlds;
    ui.eras = all.eras;
    ui.features = all.features;
    renderList();
  }

  // Whether the browser reaches a world itself: an Empyrean world with its WebSocket endpoint on.
  // Every other world is reached through dereth-web-relay on this machine.
  const direct = (w) => !!ui.direct[w.slug] || /^wss?:\/\//i.test(ui.connect[w.slug] ?? '');

  function visible() {
    const q = ui.search.trim().toLowerCase();
    const rank = { online: 0, high: 0, starting: 1, unknown: 2, offline: 3 };
    return ui.worlds
      .filter((w) => !q || w.name.toLowerCase().includes(q) || (w.description ?? '').toLowerCase().includes(q))
      .sort((a, b) => (direct(b) - direct(a)) || (b.added - a.added) || (rank[a.state] - rank[b.state]) || ((b.players ?? 0) - (a.players ?? 0)) || a.name.localeCompare(b.name));
  }

  function renderList() {
    const rows = visible();
    $('count').textContent = `${rows.length} of ${ui.worlds.length}`;
    const row = (w) => `<tr class="world ${w.slug === ui.slug ? 'chosen' : ''}" data-slug="${esc(w.slug)}">
        <td><span class="bead ${esc(w.state)}" title="${esc(STATES[w.state] ?? w.state)}"></span></td>
        <td class="name">${esc(w.name)}</td><td>${esc(EMULATORS[w.emulator] ?? w.emulator)}</td>
        <td>${esc(w.era ? eraLabel(w.era) : 'unknown')}</td>
        <td>${direct(w) ? 'directly' : `<a href="${RELAY_README}" target="_blank" rel="noopener noreferrer">needs a local relay</a>`}</td></tr>`;
    const section = (label, these) => (these.length ? `<tr class="section"><td colspan="5">${label} · ${these.length}</td></tr>${these.map(row).join('')}` : '');
    $('worlds').tBodies[0].innerHTML = rows.length
      ? section('REACHED DIRECTLY', rows.filter(direct))
        + section('NEED A LOCAL RELAY', rows.filter((w) => !direct(w)))
      : `<tr><td colspan="5" class="muted">${ui.worlds.length ? 'No world matches these filters.' : 'No worlds yet.'}</td></tr>`;
  }

  $('worlds').addEventListener('click', (ev) => {
    if (ev.target.closest('a')) return;
    const tr = ev.target.closest('tr.world');
    if (tr) choose(tr.dataset.slug);
  });
  $('search').addEventListener('input', () => {
    ui.search = $('search').value;
    renderList();
  });
  $('refresh').addEventListener('click', async () => {
    $('list-note').textContent = await fetchList(true);
    await refresh();
  });

  // ---- adding a server ----------------------------------------------------------------------

  $('add-open').addEventListener('click', () => {
    $('add-form').classList.toggle('hidden');
    $('add-problem').textContent = '';
  });
  $('add-cancel').addEventListener('click', () => $('add-form').classList.add('hidden'));

  // The host and port a server is added by, from what the player typed: `host:port`, or a URL's.
  async function addServer(name, address, emulator, rules) {
    let host = address.trim();
    let port = '9000';
    let url = null;
    if (/^(wss?|https?):\/\//i.test(host)) {
      const u = new URL(host);
      url = u.href;
      host = u.hostname.replace(/^\[|\]$/g, '');
      port = u.port || (u.protocol === 'ws:' || u.protocol === 'http:' ? '80' : '443');
    } else if (/^[^:\s/]+:\d+$/.test(host)) {
      [host, port] = host.split(':');
    }
    const slug = await call('addWorld', name, host, port, rules, url && !/^ws/i.test(url) ? 'empyrean' : emulator);
    if (url) {
      ui.connect[slug] = url;
      keep(CONNECT_KEY, ui.connect);
    }
    await save();
    await askStatus(slug);
    return slug;
  }

  $('add-form').addEventListener('submit', async (ev) => {
    ev.preventDefault();
    try {
      const slug = await addServer($('add-name').value, $('add-address').value, 'unknown', '');
      $('add-form').classList.add('hidden');
      $('add-name').value = '';
      $('add-address').value = '';
      await refresh();
      await choose(slug);
    } catch (e) {
      $('add-problem').textContent = e?.message ?? String(e);
    }
  });

  // ---- a world's page -----------------------------------------------------------------------

  async function choose(slug) {
    ui.slug = slug;
    keep(CHOSEN_KEY, slug);
    if (ui.connect[slug]) {
      await askStatus(slug);
      await refresh();
    }
    renderList();
    await renderWorld();
  }

  // Where a world is reached: what the player last used there, else its status's WebSocket URL,
  // else the relay on this machine.
  function connectFor(w, status) {
    const saved = ui.connect[w.slug];
    if (saved && /^wss?:\/\//i.test(saved)) return saved;
    if (status?.websocket_url) return status.websocket_url;
    return RELAY;
  }

  async function renderWorld() {
    const page = ui.slug ? await call('world', ui.slug) : null;
    ui.page = page;
    $('no-world').classList.toggle('hidden', !!page);
    $('form').classList.toggle('hidden', !page);
    if (!page) return;
    const w = page.world;
    $('w-name').textContent = w.name;
    $('w-remove').classList.toggle('hidden', !ui.worlds.find((x) => x.slug === w.slug)?.added);
    $('w-desc').textContent = w.description ?? '';
    const facts = [
      ['STATE', STATES[w.state] ?? w.state], ['EMULATOR', EMULATORS[w.emulator] ?? w.emulator],
      ['STATUS', w.development_status ?? 'unknown'], ['RULES', w.ruleset ?? 'unknown'],
      ['PLAYERS', w.state === 'offline' ? 'down' : w.players ?? 'unknown'],
      ['ADDRESS', page.address ?? 'none'],
    ];
    if (w.logon_version) facts.push(['LOGON', w.logon_version]);
    if (w.world_profile) facts.push(['RULES OF ITS CLIENT', w.world_profile]);
    $('w-facts').innerHTML = facts.map(([k, v]) => `<span><b>${k}</b>${esc(v)}</span>`).join('');

    // The era and systems: the world's word when it says them, else the player's choice.
    if (w.era_source === 'world') {
      $('w-era').innerHTML = `<span id="w-era-select">${esc(eraLabel(w.era))}</span>`;
    } else {
      $('w-era').innerHTML = `<select id="w-era-select"><option value="">Unknown (the client reads it from the data files)</option>${
        ui.eras.map((e) => `<option value="${esc(e.name)}" ${w.era === e.name ? 'selected' : ''}>${esc(e.label)}</option>`).join('')}</select>`;
      $('w-era-select').addEventListener('change', async (ev) => {
        await call('setEra', w.slug, ev.target.value);
        await save();
        await refresh();
        await renderWorld();
      });
    }
    // The data sets: those this world takes, then how to get the ones it wants.
    const prefs = page.prefs ?? {};
    const st = await state();
    const setsAt = (kind) => st.dat_sets.filter((s) => s.kind === kind);
    const option = (s, chosen) => `<option value="${esc(s.id)}" ${s.id === chosen ? 'selected' : ''}>${esc(setName(s.path))} (${esc(iterLabel(s.files))})</option>`;
    const modernChosen = page.dat_sets.some((s) => s.id === prefs.dat_set_id) ? prefs.dat_set_id : page.dat_sets[0]?.id;
    $('w-set').innerHTML = (page.dat_sets.length ? '' : '<option value="">None of the kept sets</option>')
      + page.dat_sets.map((s) => option(s, modernChosen)).join('')
      + (params.get('dats') === 'http' ? `<option value="http" ${!page.dat_sets.length ? 'selected' : ''}>The dev runner's files (cargo xtask web --dat-dir)</option>` : '')
      + '<option value="visit">Pick files for this visit only…</option>'
      + `<option value="add">Add ${w.dats?.custom ? `${esc(w.name)}'s` : 'data'} files to this browser…</option>`;
    const classicChosen = prefs.classic_set_id ?? page.classic_sets[0]?.id ?? '';
    $('w-classic').innerHTML = '<option value="">None</option>' + page.classic_sets.map((s) => option(s, classicChosen)).join('');
    const notes = [];
    if (page.launch.set === 'classic') notes.push(`${eraLabel(w.era)} is before Throne of Destiny: its world is drawn from the older files, portal.dat and cell.dat, with the later files beside them for the interface.`);
    if (w.dats?.custom) {
      notes.push(page.dat_sets.length
        ? `${w.name} plays with its own data files: the sets here are those that report what its files report (${iterLabel(Object.entries(w.dats.custom.iterations).map(([role, iterations]) => ({ role, iterations })))}).`
        : (page.refused_sets[0]?.why ?? `${w.name} plays with its own data files. ${w.dats.custom.license_note ?? ''}`));
    } else if (!setsAt('modern').length || (page.launch.set === 'classic' && !setsAt('classic').length)) {
      notes.push('Add your data files to this browser: the four client_*.dat files, and portal.dat and cell.dat for the classic interface and the early worlds.');
    }
    $('w-files').textContent = notes.join('\n');

    // How it is reached.
    const status = ui.connect[w.slug] && /^https?:/i.test(ui.connect[w.slug]) ? await askStatus(w.slug) : null;
    $('server').value = ui.connect[`${w.slug}#used`] ?? connectFor(w, status);
    describeConnect(w);
    $('account').value = prefs.account ?? params.get('account') ?? '';
    await renderOverlay();
  }

  function describeConnect(w) {
    const v = $('server').value.trim();
    const origin = ['127.0.0.1', 'localhost', '[::1]'].includes(location.hostname) ? '' : ` --allow-origin ${location.origin}`;
    if (/^ws:\/\/(127\.0\.0\.1|localhost|\[::1\])/i.test(v) && ui.page?.address && !ui.connect[w.slug]) {
      $('w-connect').innerHTML = `${esc(w.name)} needs a <a href="${RELAY_README}" target="_blank" rel="noopener noreferrer">local web relay</a> on this machine. Start it first:\n<code>dereth-web-relay --server ${esc(ui.page.address)}${esc(origin)}</code>`;
    } else if (/^wss:\/\//i.test(v) || /^ws:\/\//i.test(v)) {
      $('w-connect').textContent = 'The server\'s own WebSocket endpoint (Empyrean with [server.websocket] on).';
    } else {
      $('w-connect').textContent = 'A wss:// URL, a status address (https://…/status), or ws://127.0.0.1:9180/ for dereth-web-relay.';
    }
  }
  $('server').addEventListener('input', () => ui.page && describeConnect(ui.page.world));

  $('w-remove').addEventListener('click', async () => {
    if (!ui.page) return;
    await call('removeWorld', ui.page.world.slug);
    delete ui.connect[ui.page.world.slug];
    keep(CONNECT_KEY, ui.connect);
    await save();
    ui.slug = null;
    await refresh();
    await renderWorld();
  });

  // ---- the world's overlay ------------------------------------------------------------------

  async function renderOverlay() {
    const page = ui.page;
    const btns = [];
    if (!page?.overlay_folder) {
      $('w-overlay').textContent = 'This world has no address, so no overlay is kept for it.';
      $('w-overlay-btns').innerHTML = '';
      return;
    }
    const path = `overlays/${page.overlay_folder}`;
    const info = await call('overlay', path).catch(() => null);
    ui.blocklist = await call('blocklist');
    ui.overlays = await call('overlays');
    const kept = ui.overlays.find((o) => o.path === path);
    const key = info?.world_key;
    const blocked = key && ui.blocklist.includes(key);
    let text;
    if (!info) {
      text = 'No overlay can be kept for this world this visit: another tab of this page holds it, or this browser keeps no files for the page. The client plays the data files alone and tells the server so.';
    } else if (info.error) {
      text = `No overlay is kept for this world: ${info.error}`;
    } else if (!kept || !info.containers?.length) {
      text = 'No overlay is kept for this world yet. When the world updates its data files, the update is kept here, for this world alone, and your data files are never written.';
    } else {
      text = `This world's overlay is kept in this browser (${kib(kept.bytes)}, ${info.containers.join(', ')}), for the world "${key}".`
        + (blocked ? ' You refuse it: it is not read, and the world\'s updates are not kept.' : '');
      btns.push(`<button type="button" class="btn" data-overlay-remove="${esc(path)}">Remove this world's overlay</button>`);
    }
    if (key) btns.push(`<button type="button" class="btn" data-block="${esc(key)}" data-on="${blocked ? 'false' : 'true'}">${blocked ? 'Accept this world\'s overlay again' : 'Refuse this world\'s overlay'}</button>`);
    $('w-overlay').textContent = text;
    $('w-overlay-btns').innerHTML = btns.join('');
  }

  async function onKeptClick(ev) {
    const t = ev.target.closest('[data-overlay-remove],[data-block],[data-set-remove]');
    if (!t) return;
    try {
      if (t.dataset.overlayRemove) await call('removeOverlay', t.dataset.overlayRemove);
      if (t.dataset.block) await call('setBlocked', t.dataset.block, t.dataset.on === 'true');
      if (t.dataset.setRemove) {
        await call('removeSet', t.dataset.setRemove);
        await save();
      }
    } catch (e) {
      $('problem').textContent = e?.message ?? String(e);
    }
    await renderKept();
    await renderWorld();
  }
  $('w-overlay-btns').addEventListener('click', onKeptClick);
  $('storage-panel').addEventListener('click', onKeptClick);

  // ---- what is kept in this browser ---------------------------------------------------------

  async function renderKept() {
    const st = await state();
    ui.kept = await call('scanSets', st.dat_sets.map((s) => s.path), false);
    await save();
    const after = await state();
    ui.overlays = await call('overlays');
    const store = await call('storage');
    $('storage-use').textContent = store.usage != null ? `${kib(store.usage)} used${store.quota ? ` of ${kib(store.quota)} this browser allows` : ''}` : '';
    $('persist-note').textContent = store.persisted
      ? 'This browser keeps these files until you remove them.'
      : 'This browser may clear these files when it runs short of space; playing asks it to keep them.';
    $('sets').innerHTML = ui.kept.length
      ? ui.kept.map((k) => {
        const sets = after.dat_sets.filter((s) => s.path === k.path);
        const what = sets.map((s) => `${s.kind === 'classic' ? 'older files' : 'later files'}: ${esc(iterLabel(s.files))}`).join('; ');
        return `<span class="what">Data files <b>${esc(setName(k.path))}</b> <span class="muted">${what}</span></span><span class="muted">${kib(k.bytes)}</span><button type="button" class="btn" data-set-remove="${esc(k.path)}">Remove</button>`;
      }).join('')
      : '<span class="what muted">No data files are kept in this browser yet.</span><span></span><span></span>';
    $('overlays').innerHTML = ui.overlays.map((o) => `<span class="what">Overlay <b>${esc(o.path.split('/').pop())}</b> <span class="muted">a world's updates, kept by its address</span></span><span class="muted">${kib(o.bytes)}</span><button type="button" class="btn" data-overlay-remove="${esc(o.path)}">Remove</button>`).join('');
  }

  // Ask the browser to keep the page's files: only a page the player acted on may ask.
  async function persist() {
    try {
      if (await navigator.storage?.persisted?.()) return;
      const kept = await navigator.storage?.persist?.();
      log(kept ? 'the browser keeps this page\'s files' : 'the browser may clear this page\'s files when it runs short of space');
    } catch {
      // Not offered here.
    }
  }

  // ---- adding data files --------------------------------------------------------------------

  // The picker, for a set to keep (named `adding`) or for this visit only.
  function pick(purpose) {
    ui.picked = null;
    ui.adding = purpose;
    $('files').value = '';
    $('files').click();
  }
  $('add-set').addEventListener('click', () => pick({ name: 'files' }));
  $('files').addEventListener('change', async () => {
    const files = [...$('files').files];
    const purpose = ui.adding;
    ui.adding = null;
    if (!files.length || !purpose) return;
    if (purpose.visit) {
      ui.picked = files;
      $('w-files').textContent = `For this visit: ${files.map((f) => f.name).join(', ')}`;
      return;
    }
    await persist();
    const st = await state();
    let name = purpose.name.toLowerCase().replace(/[^a-z0-9-]+/g, '-').replace(/^-|-$/g, '') || 'files';
    const taken = new Set(st.dat_sets.map((s) => setName(s.path)));
    if (taken.has(name) && !purpose.replace) for (let n = 2; ; n += 1) if (!taken.has(`${name}-${n}`)) { name = `${name}-${n}`; break; }
    $('problem').textContent = `Keeping ${files.length} file(s) in this browser…`;
    try {
      await call('storeSet', name, files);
      await save();
      $('problem').textContent = '';
    } catch (e) {
      $('problem').textContent = e?.message ?? String(e);
    }
    await renderKept();
    await renderWorld();
  });
  $('w-set').addEventListener('change', () => {
    const v = $('w-set').value;
    if (v === 'visit') pick({ visit: true });
    if (v === 'add') pick({ name: ui.page?.world.dats?.custom ? ui.page.world.slug : 'files' });
  });

  // ---- playing ------------------------------------------------------------------------------

  $('form').addEventListener('submit', async (ev) => {
    ev.preventDefault();
    const page = ui.page;
    if (!page) return;
    $('problem').textContent = '';
    let server = $('server').value.trim();
    if (/^https?:\/\//i.test(server)) {
      try {
        const target = new URL(server);
        if (target.pathname === '/' || target.pathname === '') target.pathname = '/status';
        const s = await (await fetch(target, { cache: 'no-store' })).json();
        if (!s.websocket_url) throw new Error(`${target}: the server has no WebSocket endpoint`);
        server = s.websocket_url;
      } catch (e) {
        $('problem').textContent = `The server's status did not answer: ${e?.message ?? e}`;
        return;
      }
    }
    const chosen = $('w-set').value;
    const st = await state();
    const pathOf = (id) => st.dat_sets.find((s) => s.id === id)?.path ?? null;
    let dats;
    if (chosen === 'http') dats = { mode: 'http' };
    else if (chosen === 'visit') {
      if (!ui.picked) {
        $('problem').textContent = 'Pick the files for this visit first.';
        return;
      }
      dats = { mode: 'files', files: ui.picked };
    } else if (!chosen || chosen === 'add') {
      if (page.launch.set !== 'classic' || !$('w-classic').value) {
        $('problem').textContent = page.world.dats?.custom ? $('w-files').textContent : 'Choose the data files to play with.';
        return;
      }
      dats = { mode: 'sets', modern: null, classic: pathOf($('w-classic').value) };
    } else {
      dats = { mode: 'sets', modern: pathOf(chosen), classic: pathOf($('w-classic').value) };
    }
    ui.connect[`${page.world.slug}#used`] = $('server').value.trim();
    keep(CONNECT_KEY, ui.connect);
    await call('remember', page.world.slug, {
      account: $('account').value.trim() || undefined,
      dat_set_id: dats.mode === 'sets' && chosen && chosen !== 'add' ? chosen : undefined,
      classic_set_id: $('w-classic').value || undefined,
      last_played: Math.floor(Date.now() / 1000),
    });
    await save();
    await persist();
    launch({
      dats,
      server,
      account: $('account').value.trim(),
      password: $('password').value,
      args: page.args,
      overlay: page.overlay_folder ? `overlays/${page.overlay_folder}` : '',
    });
  });

  // ---- start --------------------------------------------------------------------------------

  async function begin() {
    await call('setState', load(STATE_KEY, '')).catch(async (e) => {
      log(`the launcher's state did not read (${e?.message ?? e}); starting afresh`);
      await call('setState', '');
    });
    $('list-note').textContent = await fetchList(false);
    // `?server=<url>` adds that server (once) and chooses it, as an operator's link to the page.
    const linked = params.get('server');
    if (linked) {
      const found = Object.entries(ui.connect).find(([k, v]) => !k.includes('#') && v === linked);
      try {
        ui.slug = found ? found[0] : await addServer('', linked, 'unknown', '');
      } catch (e) {
        $('list-note').textContent = `?server=${linked}: ${e?.message ?? e}`;
      }
      const era = params.get('era');
      if (era && ui.slug) await call('setEra', ui.slug, era);
      for (const part of (params.get('features') || '').split(',')) {
        const [k, v] = part.split('=');
        if (k && v !== undefined && ui.slug) await call('setFeature', ui.slug, k.trim(), v.trim() === 'true');
      }
      await save();
    }
    for (const slug of Object.keys(ui.connect)) if (!slug.includes('#')) await askStatus(slug);
    await refresh();
    await renderKept();
    if (ui.slug) await choose(ui.slug);
  }

  return {
    begin,
    // The client ended: the front page again, as it was, with what is kept read again.
    async back() {
      await renderKept().catch(() => {});
      await renderWorld().catch(() => {});
    },
  };
}
