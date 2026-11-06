// The launcher's backend in the browser. The launcher's own page (`host.js` loads it) draws every
// screen from one snapshot and sends every action as one command, so this answers by the desktop
// launcher's command names and in the shape of its snapshot. The answers come from the
// launcher's logic in its module (its worker runs it: the world list, the worlds, the accounts,
// favourites and recent combinations, the data sets and which of them a world takes, plugins) and
// from this browser's storage.
//
// The launcher's state record and the day's copy of the world list are kept in the browser's
// local storage, as the desktop keeps them in its files. No password is kept here: the browser's
// password manager keeps them (`navigator.credentials` where the browser has it, and the form's
// own markup everywhere).
//
// What only a browser needs is kept apart, at the end: how a world is reached (directly at its
// WebSocket endpoint, or not at all), a world's updates (its overlay), the files kept in the
// browser's private storage, and the file pickers' results.

const STATE_KEY = 'dereth.launcher';
const LIST_KEY = 'dereth.worldlist';
const CONNECT_KEY = 'dereth.connect';

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

const now = () => Math.floor(Date.now() / 1000);

// The name a world's password is kept under in the browser: the account and the world, so the same
// account name on two worlds keeps two passwords.
export const loginName = (account, world) => `${account.trim()} on ${world}`;

// `call(name, ...args)` asks the launcher in the worker; `start(choice)` starts the client;
// `scale` is the render scale the page keeps (`get`, `set`).
export function webBackend({ call, start, params, log, scale }) {
  const web = {
    list: { state: 'loading', error: null, fetched: null },
    message: null,
    job: null,
    version: '',
    // Where each server the player added by a URL was added from (by slug).
    connect: load(CONNECT_KEY, {}),
    // Each such server's status document, as last read.
    status: {},
    kept: { sets: [], overlays: [], storage: {}, added: [] },
    // Files picked for one visit.
    picked: null,
    // A client is running: it has the launcher in the worker, so nothing is asked of it.
    playing: false,
    // The last snapshot, the answer while a client runs.
    snapshot: null,
  };

  const save = async () => keep(STATE_KEY, await call('state'));
  const state = async () => JSON.parse(await call('state'));
  const say = (text, error = false) => {
    web.message = { text, error };
  };

  // ---- the world list -------------------------------------------------------------------------

  // Read the world list when the page opens and when asked, by the desktop launcher's rules (the
  // logic in the worker names each address to fetch): the hub's list (the one the page's `hub`
  // parameter names, else the network's) a page at a time, then the community's own when the hub
  // does not answer with its list; a list the page's `list` parameter names is read alone. The
  // day's copy stands in for it until it is not the day's.
  async function fetchList(force) {
    let cached = load(LIST_KEY, null);
    // A copy that does not read, or was read from other sources than the page's, is no copy.
    if (cached && !(await call('listCopy', JSON.stringify(cached), params.get('list'), params.get('hub'))
      .then(() => true, () => false))) {
      cached = null;
    }
    if (cached) web.list = { state: 'loaded', error: null, fetched: cached.fetched_at };
    if (!force && !(await call('listWanted', now()))) return;
    if (!cached) web.list = { state: 'loading', error: null, fetched: null };
    let url = await call('listStart', params.get('list'), params.get('hub'));
    while (url) {
      let body = null;
      let error = null;
      try {
        const res = await fetch(url, { cache: 'no-store' });
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        body = await res.text();
      } catch (e) {
        error = String(e?.message ?? e);
      }
      url = await call('listNext', body, error);
    }
    try {
      const done = await call('listDone', now());
      keep(LIST_KEY, done.copy);
      web.list = { state: 'loaded', error: null, fetched: done.copy.fetched_at };
      log(`world list: ${done.worlds} worlds from ${done.from}`);
    } catch (e) {
      const why = String(e?.message ?? e);
      log(`world list: ${why}`);
      web.list = { state: 'unavailable', error: `The world list did not answer (${why}).`, fetched: cached?.fetched_at ?? null };
    }
  }

  // The hub's word on each world: the launcher in the worker says when a read of the hub's list is
  // due (about a minute after the last, while the worlds shown are the hub's) and names each page
  // to fetch. Answers whether it read.
  let readingHub = false;
  async function readHub() {
    if (readingHub) return false;
    readingHub = true;
    try {
      let url = await call('hubStart', params.get('list'), params.get('hub'), now());
      if (!url) return false;
      while (url) {
        let body = null;
        let error = null;
        try {
          const res = await fetch(url, { cache: 'no-store' });
          if (!res.ok) throw new Error(`HTTP ${res.status}`);
          body = await res.text();
        } catch (e) {
          error = String(e?.message ?? e);
        }
        url = await call('hubNext', body, error);
      }
      if (!(await call('hubDone', now()))) log('the hub\'s status did not answer');
      return true;
    } finally {
      readingHub = false;
    }
  }

  // ---- the snapshot ---------------------------------------------------------------------------

  // What a browser cannot do, for the page to leave out.
  const OFFERS = {
    hosting: false,
    building: false,
    retail: false,
    downloads: false,
    discord: false,
    client_choice: false,
    make_plugin: false,
    kept_passwords: false,
    art_packs: false,
    realms: ['dereth', 'auberean'],
  };

  // The client here, as the page shows one it found.
  const client = () => ({ kind: 'dereth', version: web.version, shown_name: `Dereth ${web.version}`, path: 'Runs in this browser.' });

  async function snapshot() {
    if (web.playing && web.snapshot) return web.snapshot;
    try {
      web.snapshot = await read();
    } catch (e) {
      // A client that started meanwhile refuses the rest; that is no error.
      if (!web.playing || !web.snapshot) throw e;
    }
    return web.snapshot;
  }

  async function read() {
    const all = await call('worlds');
    const st = await state();
    const sets = [...st.dat_sets, ...(st.auberean?.dat_sets ?? [])];
    for (const set of sets) set.shown_path = `Kept in this browser as ${String(set.path).split('/').pop()}`;
    return {
      offers: OFFERS,
      dereth: client(),
      // Auberean's realm: its client is part of this page, and its files' check is the launcher's.
      auberean: {
        here: { shown_name: 'Auberean', path: 'Runs in this browser.' },
        check: await call('aubereanCheck'),
      },
      worlds: all.worlds.map((w) => {
        const reach = reachOf(w);
        return { ...w, reach: { ...reach, text: reach.direct ? 'Directly' : 'Not in a browser yet' } };
      }),
      eras: all.eras,
      features: all.features,
      state: st,
      list_state: web.list.state,
      list_error: web.list.error,
      list_fetched_at: web.list.fetched,
      plugins: { ...(await call('plugins')), local: web.kept.added },
      interface: await call('interface'),
      running: [],
      job: web.job,
      message: web.message,
      update: { kind: 'none' },
      version: web.version,
      platform: 'web',
      // What only the browser has.
      web: {
        kept: web.kept,
        scale: scale.get(),
        picked: web.picked ? web.picked.map((f) => f.name) : null,
        dev_dats: params.get('dats') === 'http',
      },
    };
  }

  // ---- the commands, by the desktop launcher's names ------------------------------------------

  const handlers = {
    snapshot,
    refresh: async () => {
      await fetchList(true);
    },
    dismiss_message: () => {
      web.message = null;
    },
    // A world's page: the world, the data sets it takes, its accounts and the player's last
    // choices there, and what the browser adds: how it is reached and its updates.
    world_view: async ({ slug }) => {
      if (web.connect[slug] && /^https?:/i.test(web.connect[slug])) await askStatus(slug);
      const page = await call('world', slug);
      if (!page) throw new Error('That world is no longer listed.');
      const auberean = page.world.game === 'auberean';
      return {
        ...page,
        prefs: page.prefs ?? {},
        clients: auberean ? [] : [{ kind: 'dereth', install: client(), runnable: true }],
        default_client: auberean ? 'auberean' : 'dereth',
        // `?account=<name>`: the account to start from where the world knows none.
        account_hint: params.get('account'),
        reach: reachOf(page.world),
        overlay: await overlayOf(page),
      };
    },
    // The browser's password manager may have it; the launch asks, and says when it has none.
    has_password: () => true,
    // Nothing is asked of a world here beyond what the hub says of it.
    worlds_shown: () => {},
    probe_world: () => {},
    // The client is part of this page: there is nothing to download.
    check_components: () => {},
    set_realm: async ({ realm }) => {
      await call('setRealm', realm);
      await save();
    },
    launch: ({ choice }) => launch(choice),
    save_favourite: async ({ favourite }) => {
      await call('saveFavourite', favourite);
      await save();
    },
    remove_favourite: async ({ id }) => {
      await call('removeFavourite', id);
      await save();
    },
    set_world_favorite: async ({ slug, favorite }) => {
      await call('setWorldFavorite', slug, favorite);
      await save();
    },
    forget_account: async ({ slug, username }) => {
      await call('forgetAccount', slug, username);
      await save();
    },
    forget_all_accounts: async () => {
      await call('forgetAllAccounts');
      await save();
    },
    // Add a world by what the player typed: `host:port`, a `wss://` URL, or an Empyrean status
    // address. Answers its slug.
    add_custom_world: async ({ server }) => addServer(server),
    remove_custom_world: async ({ slug }) => {
      await call('removeWorld', slug);
      delete web.connect[slug];
      delete web.connect[`${slug}#used`];
      keep(CONNECT_KEY, web.connect);
      await save();
    },
    set_default_set: async ({ id }) => {
      await call('setDefaultSet', id);
      await save();
    },
    // A set of one kind, its files taken out of the folder it is kept in.
    delete_set: async ({ id }) => {
      const st = await state();
      const set = st.dat_sets.find((s) => s.id === id);
      const ac2 = st.auberean?.dat_sets?.find((s) => s.id === id);
      if (!set && !ac2) return;
      if (set) await call('removeSetKind', set.path, set.kind);
      else await call('removeSet', ac2.path);
      await save();
      await readKept();
    },
    set_interface: ({ name }) => call('setInterface', name),
    // Plugins, from plugin sources, with the launcher's own shelf in the worker.
    refresh_plugins: async () => (await call('pluginRefresh')).said,
    add_plugin_source: ({ url }) => call('pluginAddSource', url),
    remove_plugin_source: ({ url }) => call('pluginRemoveSource', url),
    install_plugin: ({ source, id }) => call('installPlugin', source, id),
    update_plugin: ({ id }) => call('updatePlugin', id),
    remove_plugin: async ({ id }) => {
      await call('removePlugin', id);
      await readKept();
    },
    set_plugin_updates: ({ setting }) => call('pluginUpdates', setting),
    open_url: ({ url }) => {
      window.open(url, '_blank', 'noopener');
    },
  };

  // Start the client on `choice`: `{ world_slug, account, password, dat_set_id, classic_set_id }`,
  // `dat_set_id` being '__http' for the dev runner's files or '__visit' for files picked for this
  // visit. Answers as the desktop's launch does: launched, or the password it needs, or why not.
  // The client takes this page, which stays as it is for its return.
  async function launch(choice) {
    const page = await call('world', choice.world_slug);
    if (!page) return { kind: 'failed', message: 'That world is no longer listed.' };
    const account = (choice.account ?? '').trim();
    if (!account) return { kind: 'need_password', message: 'Enter your account name and password.' };
    const login = loginName(account, page.world.name);
    const password = choice.password || (await recallPassword(login));
    if (!password) return { kind: 'need_password', message: `Enter the password for ${account}.` };

    const game = page.world.game === 'auberean' ? 'auberean' : 'dereth';
    // Where to connect: the world's own WebSocket endpoint. A world with none can't be played in a
    // browser.
    const reach = reachOf(page.world);
    if (!reach.direct) return { kind: 'failed', message: `${page.world.name} can't be played in a browser yet.` };
    let server = reach.url;
    if (/^https?:\/\//i.test(server)) {
      try {
        const s = await statusDocument(server);
        if (!s.websocket_url) throw new Error('the server has no WebSocket endpoint');
        server = s.websocket_url;
      } catch (e) {
        return { kind: 'failed', message: `Couldn't reach the server's status: ${e?.message ?? e}` };
      }
    }
    // Refused here, before anything of the game is fetched.
    const refused = await call('serverProblem', server);
    if (refused) return { kind: 'failed', message: refused };

    // The data files.
    const st = await state();
    const pathOf = (id) => [...st.dat_sets, ...(st.auberean?.dat_sets ?? [])].find((s) => s.id === id)?.path ?? null;
    let dats;
    if (choice.dat_set_id === '__http') {
      dats = { mode: 'http' };
    } else if (choice.dat_set_id === '__visit') {
      if (!web.picked) return { kind: 'failed', message: 'Pick the files for this visit first.' };
      dats = { mode: 'files', files: web.picked };
    } else if (game === 'auberean') {
      const set = choice.dat_set_id ? pathOf(choice.dat_set_id) : null;
      if (!set) return { kind: 'failed', message: 'Choose the data files to play with.' };
      dats = { mode: 'sets', set };
    } else {
      const modern = choice.dat_set_id ? pathOf(choice.dat_set_id) : null;
      const classic = choice.classic_set_id ? pathOf(choice.classic_set_id) : null;
      if (!modern && !(page.launch.set === 'classic' && classic)) {
        return { kind: 'failed', message: 'Choose the data files to play with.' };
      }
      dats = { mode: 'sets', modern, classic };
    }

    await call('recordLaunch', page.world.slug, {
      account,
      dat_set_id: dats.mode === 'sets' ? choice.dat_set_id ?? undefined : undefined,
      classic_set_id: dats.mode === 'sets' ? choice.classic_set_id ?? undefined : undefined,
    });
    await save();
    await persist();
    await storePassword(login, password);
    web.playing = true;
    start({
      game,
      dats,
      server,
      account,
      password,
      args: page.args,
      overlay: page.overlay_folder ? `overlays/${page.overlay_folder}` : '',
    });
    return { kind: 'launched', stays: true, world: page.world.name, account };
  }

  // ---- passwords --------------------------------------------------------------------------------

  // The browser's own password kept under `login` ([`loginName`]: the account and the world) on
  // this site, where the browser offers one to the page (`navigator.credentials`; it may ask the
  // player which). Null elsewhere: there the form's markup lets the browser fill the field itself.
  async function recallPassword(login) {
    if (!('PasswordCredential' in window) || !navigator.credentials) return null;
    try {
      const c = await navigator.credentials.get({ password: true, mediation: 'optional' });
      return c && c.id.toLowerCase() === login.toLowerCase() ? c.password : null;
    } catch {
      return null;
    }
  }
  // Offer the password to the browser to keep under `login`, where it takes one from the page.
  async function storePassword(login, password) {
    if (!('PasswordCredential' in window) || !navigator.credentials) return;
    try {
      await navigator.credentials.store(new PasswordCredential({ id: login, password, name: login }));
    } catch {
      // Not kept; the browser may still offer to keep it from the form.
    }
  }

  // ---- the browser's own: reaching a world ------------------------------------------------------

  // How the browser reaches a world: directly at a WebSocket endpoint the launcher knows (the
  // hub's list names it, the player added the world by it, or the world's status names it), or
  // not at all.
  function reachOf(w) {
    const saved = web.connect[w.slug];
    if (saved && /^wss?:\/\//i.test(saved)) return { direct: true, url: saved };
    const url = w.websocket_url ?? web.status[w.slug]?.websocket_url;
    return url ? { direct: true, url } : { direct: false, url: null };
  }

  async function statusDocument(url) {
    const target = new URL(url);
    if (target.pathname === '/' || target.pathname === '') target.pathname = '/status';
    return (await fetch(target, { cache: 'no-store' })).json();
  }

  // A world added by a status address is asked for its status, which names its WebSocket URL.
  async function askStatus(slug) {
    const url = web.connect[slug];
    if (!url || !/^https?:\/\//i.test(url)) return;
    try {
      const target = new URL(url);
      if (target.pathname === '/' || target.pathname === '') target.pathname = '/status';
      const text = await (await fetch(target, { cache: 'no-store' })).text();
      await call('status', slug, text);
      web.status[slug] = JSON.parse(text);
    } catch (e) {
      log(`status of ${url}: ${e?.message ?? e}`);
      await call('statusFailed', slug);
      delete web.status[slug];
    }
  }

  // Add a server from what the player typed: `{ name, host, port, emulator, ruleset }`, `host` being
  // a bare host, `host:port`, a `wss://` URL or a status address.
  async function addServer({ name = '', host: address, port: typed, emulator = 'unknown', ruleset = '', game = 'dereth' }) {
    let host = String(address ?? '').trim();
    let port = typed || '9000';
    let url = null;
    if (/^(wss?|https?):\/\//i.test(host)) {
      const u = new URL(host);
      url = u.href;
      host = u.hostname.replace(/^\[|\]$/g, '');
      port = u.port || (u.protocol === 'ws:' || u.protocol === 'http:' ? '80' : '443');
    } else if (/^[^:\s/]+:\d+$/.test(host)) {
      [host, port] = host.split(':');
    }
    const server = game === 'auberean' ? 'yalain' : 'empyrean';
    const slug = await call('addWorld', game, name, host, port, ruleset ?? '', url && !/^ws/i.test(url) ? server : emulator || 'unknown');
    if (url) {
      web.connect[slug] = url;
      keep(CONNECT_KEY, web.connect);
      await call('addedBy', slug, url);
    }
    await save();
    await askStatus(slug);
    return slug;
  }

  // ---- the browser's own: a world's updates -----------------------------------------------------

  // A world's overlay as this browser keeps it: `{ path, state, bytes, containers, key, blocked,
  // error }`. `state` is `none` (the world has no address), `busy` (another tab has it open, or the
  // browser keeps no files), `error`, `empty` or `kept`.
  async function overlayOf(page) {
    if (!page.overlay_folder) return { state: 'none' };
    const path = `overlays/${page.overlay_folder}`;
    const info = await call('overlay', path).catch(() => null);
    const blocklist = await call('blocklist');
    const overlays = await call('overlays');
    const kept = overlays.find((o) => o.path === path);
    const key = info?.world_key ?? null;
    const out = { path, key, blocked: !!key && blocklist.includes(key), bytes: kept?.bytes ?? 0, containers: info?.containers ?? [] };
    if (!info) return { ...out, state: 'busy' };
    if (info.error) return { ...out, state: 'error', error: info.error };
    return { ...out, state: kept && info.containers?.length ? 'kept' : 'empty' };
  }

  // ---- the browser's own: what is kept here -----------------------------------------------------

  // The sets, the world updates and the plugins kept in the browser's private storage, how much of
  // the browser's allowance they use, and whether the browser keeps them.
  async function readKept() {
    const st = await state();
    const sets = await call('scanSets', st.dat_sets.map((s) => s.path), false);
    const ac2Sets = await call('scanAubereanSets', (st.auberean?.dat_sets ?? []).map((s) => s.path), false);
    await save();
    web.kept = {
      sets: [...sets, ...ac2Sets],
      overlays: await call('overlays'),
      storage: await call('storage'),
      added: await call('addedPlugins'),
    };
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

  // Keep the data files the player picked as a set of `game`'s in this browser, named `name` (a word
  // for its folder). Answers the ids of the sets read from them. Files that are no set of Asheron's
  // Call 2's are refused before any is copied.
  async function storeFiles(name, files, game = 'dereth') {
    const auberean = game === 'auberean';
    if (auberean) {
      const problem = await call('aubereanSetProblem', files.map((f) => f.name));
      if (problem) throw new Error(problem);
    }
    await persist();
    const st = await state();
    let folder = String(name).toLowerCase().replace(/[^a-z0-9-]+/g, '-').replace(/^-|-$/g, '') || 'files';
    const kept = auberean ? st.auberean?.dat_sets ?? [] : st.dat_sets;
    const taken = new Set(kept.map((s) => String(s.path).split('/').pop()));
    if (taken.has(folder)) for (let n = 2; ; n += 1) if (!taken.has(`${folder}-${n}`)) { folder = `${folder}-${n}`; break; }
    web.job = { what: `Keeping ${files.length} file${files.length === 1 ? '' : 's'} in this browser` };
    try {
      const ids = await call(auberean ? 'storeAubereanSet' : 'storeSet', folder, files);
      await save();
      return ids;
    } finally {
      web.job = null;
      await readKept();
    }
  }

  // Each `{ path, bytes }` of a plugin folder or zips the player picked, kept in this browser.
  async function addPlugins(files) {
    await call('addPlugins', files);
    await readKept();
  }

  // The client's own render scale, which the Library sets.
  const setScale = (value) => scale.set(String(value));

  // ---- start ------------------------------------------------------------------------------------

  // As the page opens: the launcher's state as the browser kept it, the world list, the servers
  // the player added by a URL asked for their status, and what is kept here. Answers the slug of
  // the server the page's `server` parameter names, which it adds once.
  async function begin() {
    await call('setState', load(STATE_KEY, '')).catch(async (e) => {
      log(`the launcher's state did not read (${e?.message ?? e}); starting afresh`);
      await call('setState', '');
    });
    web.version = await call('version');
    await fetchList(false);
    let linked = null;
    const named = params.get('server');
    if (named) {
      const found = Object.entries(web.connect).find(([k, v]) => !k.includes('#') && v === named);
      try {
        linked = found ? found[0] : await addServer({ host: named });
      } catch (e) {
        say(`?server=${named}: ${e?.message ?? e}`, true);
      }
    }
    for (const [slug, url] of Object.entries(web.connect)) {
      if (slug.includes('#')) continue;
      // What the launcher keeps of a server added by its URL, for one added before it kept it.
      await call('addedBy', slug, url).catch(() => {});
      await askStatus(slug);
    }
    await save();
    await readKept();
    return linked;
  }

  return {
    // A command by the desktop launcher's name: its answer, or what to say, as the desktop's answers
    // are. It runs at once up to its first wait, inside the click that asked.
    async api(cmd, args = {}) {
      const h = handlers[cmd];
      if (!h) throw `no ${cmd} here`;
      try {
        return await h(args);
      } catch (e) {
        throw String(e?.message ?? e);
      }
    },
    begin,
    playing: () => web.playing,
    // The client ended: the launcher in the worker is the page's again.
    ended() {
      web.playing = false;
    },
    readHub,
    readKept,
    storeFiles,
    addPlugins,
    setScale,
    persist,
    say,
    // Files picked for this visit only, or none.
    pickForVisit(files) {
      web.picked = files?.length ? files : null;
    },
    removeOverlay: async (path) => {
      await call('removeOverlay', path);
      await readKept();
    },
    setBlocked: (key, on) => call('setBlocked', key, on),
    removeAddedPlugin: async (name) => {
      await call('removeAddedPlugin', name);
      await readKept();
    },
    removePlugins: async () => {
      await call('removePlugins');
      await readKept();
    },
  };
}
