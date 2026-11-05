// The front page: the desktop launcher's own page, running in the browser over the browser's
// backend (`backend.js`). The build brings the launcher's script, stylesheet and shell here as
// `app.js`, `style.css` and `launcher.html`; nothing of them is kept in this folder.
//
// This file is only what a browser does its own way, handed to the launcher's page as
// `window.launcherHost`: the backend, the browser's words where the page's would be wrong, the sign-in
// its password manager watches, how a world is reached, a world's updates, the files kept here and
// the browser's file pickers. What a browser cannot do at all (hosting a world, building one, the
// retail client, downloads, Discord) the backend's snapshot says it does not offer, and the page
// leaves out.

import { loginName, webBackend } from './backend.js';

const RELAY_README = 'https://github.com/dereth-network/dereth/blob/main/tools/web-relay/README.md';
// The desktop client, for a world a browser can't reach.
const DESKTOP_RELEASE = 'https://github.com/dereth-network/dereth/releases/latest';
const AUBEREAN_README = 'https://github.com/dereth-network/dereth/blob/main/auberean/README.md';

const SCALES = ['100', '85', '75', '67', '50'];

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
const mb = (n) => `${Math.max(1, Math.round(n / 1048576))} MB`;
const kb = (n) => (n >= 1048576 ? mb(n) : `${Math.max(1, Math.round(n / 1024))} KB`);
const size = (n) => (n >= 1073741824 ? `${(n / 1073741824).toFixed(1)} GB` : mb(n));

const row = (name, sub, buttons = '') => `<div class="row static"><div class="grow"><div class="name">${name}</div><div class="sub">${sub}</div></div>${buttons ? `<div class="btns">${buttons}</div>` : ''}</div>`;

// A file picker's answer: what the player chose, or nothing when they closed it. It opens at once,
// inside the click that asked: a browser refuses a picker later.
function chooseFiles(input) {
  return new Promise((resolve) => {
    input.value = '';
    const done = (files) => {
      input.onchange = null;
      input.oncancel = null;
      resolve(files);
    };
    input.onchange = () => done([...input.files]);
    input.oncancel = () => done([]);
    input.click();
  });
}

export function startHost({ call, start, params, log, scale }) {
  const backend = webBackend({ call, start, params, log, scale });
  // The launcher's page, once it has started: its state and what it does.
  let page = null;
  // The page's first look at the backend waits until the backend has begun.
  let begun;
  const ready = new Promise((resolve) => {
    begun = resolve;
  });

  // ----- the browser's pickers -------------------------------------------------------------------

  // What the page asks for by the desktop's commands that open the system's folder picker. The files
  // a browser picks are kept as they are chosen, so there is nothing left to add.
  const pickers = {
    async pick_folder({ game } = {}) {
      const files = await chooseFiles($('files'));
      if (!files.length) return null;
      const ids = await backend.storeFiles('files', files, game);
      if (!ids.length) backend.say('No data files among these.', true);
      const { state } = await backend.api('snapshot');
      const kept = game === 'auberean' ? state.auberean?.dat_sets ?? [] : state.dat_sets;
      const sets = kept.filter((s) => ids.includes(s.id));
      return {
        kept: true,
        folder: 'DATA FILES',
        retail: null,
        error: null,
        dats: sets.find((s) => s.kind !== 'classic') ?? null,
        classic: sets.find((s) => s.kind === 'classic') ?? null,
      };
    },
    add_folder: () => {},
    // A plugin's folder (the one holding its plugin.toml), kept in this browser; the client finds it
    // when it starts. Answers no folder: the desktop's next step has nothing to do here.
    async pick_plugin_folder() {
      const picked = await chooseFiles($('plugin-folder'));
      if (!picked.length) return null;
      const files = await Promise.all(picked.map(async (f) => ({ path: f.webkitRelativePath || f.name, bytes: new Uint8Array(await f.arrayBuffer()) })));
      if (!files.some((f) => f.path.endsWith('/plugin.toml'))) throw new Error('That folder has no plugin.toml: pick the plugin\'s own folder.');
      await backend.addPlugins(files);
      page.toast('Added. Turn it on in the game with /plugin.');
      return null;
    },
  };

  // Plugins' zips, kept in this browser.
  async function addPluginZips() {
    const picked = (await chooseFiles($('plugin-zip'))).filter((f) => f.name.toLowerCase().endsWith('.zip'));
    const files = await Promise.all(picked.map(async (f) => ({ path: f.name, bytes: new Uint8Array(await f.arrayBuffer()) })));
    if (!files.length) return;
    await backend.addPlugins(files);
    page.toast('Added. Turn it on in the game with /plugin.');
    await page.pull();
  }

  // The data files a world's page chose to pick: for this visit only, or to keep (named after a world
  // with files of its own). The page's other choices stay.
  async function pickForWorld(visit) {
    const world = page.ui.world;
    const files = await chooseFiles($('files'));
    if (!files.length) return;
    if (visit) {
      backend.pickForVisit(files);
      world.form.dat_set_id = '__visit';
      await page.pull();
      page.render();
      return;
    }
    const custom = world.view.world.dats?.custom;
    try {
      const ids = await backend.storeFiles(custom ? world.view.world.slug : 'files', files, world.view.world.game);
      if (!ids.length) page.toast('No data files among these.', true);
    } catch (e) {
      page.toast(String(e?.message ?? e), true);
    }
    // A world of Asheron's Call 2's offers what the snapshot lists, so that comes first.
    await page.pull();
    await page.reopenWorld();
    await page.pull();
  }

  // ----- what the launcher's page asks of its host ------------------------------------------------

  const host = {
    api(cmd, args) {
      if (cmd === 'snapshot') return ready.then(() => backend.api(cmd, args));
      // A picker runs at once, inside the click that asked, and answers as the backend does.
      if (pickers[cmd]) return Promise.resolve(pickers[cmd](args)).catch((e) => { throw String(e?.message ?? e); });
      return backend.api(cmd, args);
    },

    // Another realm's paintings are fetched only once that realm is shown.
    paintingsWhenShown: true,

    // Where the browser's words are not the desktop's.
    words: {
      'foot.version': (version) => `Dereth ${version}`,
      'worlds.unavailable': (error) => `${error} Add a world by its address.`,
      'worlds.stale': (error, from) => `${error} Showing the list from ${from}.`,
      'library.no-modern': 'No Modern data files yet. Add the client_*.dat files.',
      'library.no-classic': 'No Classic data files yet. Add portal.dat and cell.dat.',
      'library.no-auberean': 'No data sets yet. Add portal.dat, cell_1.dat and local_English.dat, and highres.dat when you have it.',
      'find.none': 'No data files among these.',
      'accounts.note': 'Your browser\'s password manager keeps the passwords.',
      'first-run.then': 'Then come back and choose the files.',
      'first-run.choose': 'Choose files',
      'first-run.never': 'We never download or host these files. They stay in this browser.',
      'first-run.other': 'Choose other files',
      'first-run.yes': 'Yes - choose the files',
      'first-run.no': 'No - how do I get them?',
      'first-run.why': 'The Dereth client needs the game\'s data (.dat) files, which we cannot ship. They are read in this browser and never uploaded.',
    },

    // A world's address: a browser takes what it can connect to.
    addWorldWhere: (a) => `<label class="lab" for="srv-host">ADDRESS</label><input id="srv-host" class="field" autocomplete="off" spellcheck="false" placeholder="host:port, a wss:// address, or a status address" value="${esc(a.host)}">`,

    // The sign-in, in the form the browser's password manager watches, under a name that holds the
    // account and the world, so one account name on two worlds keeps two passwords.
    worldLogin: (world, f) => `<label class="lab" for="acct-name">ACCOUNT</label><input id="acct-name" name="account" class="field" autocomplete="off" autocapitalize="off" spellcheck="false" placeholder="account name" value="${esc(f.account)}">
      <input id="login-name" name="username" class="login-name" autocomplete="username" tabindex="-1" aria-hidden="true" data-login-world="${esc(world.name)}" value="${esc(loginName(f.account, world.name))}">
      <label class="lab" for="password">PASSWORD</label><input id="password" name="password" class="field" type="password" autocomplete="current-password" value="${esc(f.password)}">
      <p class="note muted">Your browser's password manager keeps your password.</p>`,

    // The ways to data files a world's page has besides the sets kept here.
    moreSets(world) {
      const web = page.ui.snap.web;
      return [
        ...(web.dev_dats ? [{ id: '__http', label: 'The dev runner\'s files', both_kinds: true, fallback: true }] : []),
        { id: '__visit', label: web.picked ? `Picked for this visit (${web.picked.length} files)` : 'Pick files for this visit only...' },
        { id: '__add', label: `Add ${world.dats?.custom ? `${world.name}'s` : 'data'} files to this browser...` },
      ];
    },

    // The world page's data-file choice: whether it was one of the pickers.
    datsChosen(select, form) {
      if (select.value === '__add') {
        pickForWorld(false);
        select.value = form.dat_set_id ?? '';
        return true;
      }
      if (select.value === '__visit') {
        pickForWorld(true);
        // Files already picked stay chosen, whatever this picker brings.
        if (page.ui.snap.web.picked) form.dat_set_id = '__visit';
        else select.value = form.dat_set_id ?? '';
        return true;
      }
      return false;
    },

    // How the browser reaches the world, and the updates it keeps of it.
    worldRows: (view) => `${reachRows(view)}${updatesRows(view)}${view.join_link
      ? `<label class="lab">JOIN LINK</label><div class="pair"><span class="small muted join">${esc(view.join_link)}</span><button type="button" class="btn mini-btn" data-copy-link="${esc(view.join_link)}">Copy link</button></div>`
      : ''}`,

    libraryClientRows: (snap) => row('Render scale', 'Fewer pixels draw faster and look softer.',
      `<select id="render-scale" class="field mini" aria-label="Render scale">${SCALES.map((v) => `<option value="${v}" ${snap.web.scale === v ? 'selected' : ''}>${v}%</option>`).join('')}</select>`),

    // What this browser keeps: worlds' updates, and how much of its allowance they use.
    librarySections(snap) {
      const kept = snap.web.kept;
      const updates = kept.overlays.map((o) => row(esc(o.path.split('/').pop()), esc(kb(o.bytes)),
        `<button class="btn" data-overlay-remove="${esc(o.path)}">Remove</button>`)).join('')
        || '<p class="muted">No world has sent updates yet.</p>';
      const st = kept.storage ?? {};
      const used = st.usage != null ? `${size(st.usage)} used${st.quota ? ` of ${size(st.quota)}` : ''}` : 'Not known';
      const storage = row(esc(used), st.persisted ? 'This browser keeps these files until you remove them.' : 'This browser may clear these files when it runs short of space. Playing asks it to keep them.');
      // Asheron's Call 2's worlds patch nothing a browser keeps.
      const patched = page.ui.realm === 'auberean' ? ''
        : `<h2>WORLD UPDATES</h2><p class="muted small">What worlds have patched, kept apart from your files.</p><div class="list">${updates}</div>`;
      return `${patched}<h2>STORAGE</h2><div class="list">${storage}</div>`;
    },

    // A plugin added by hand: its folder's or zip's name, marked Added, with Remove.
    localPluginRow: (a) => `<div class="row static plugin-row"><div class="grow"><div class="name">${esc(a.name)} <span class="muted small">Added</span></div>
        <div class="sub">${a.folder ? 'A plugin folder' : 'A plugin zip'}, kept in this browser.</div></div>
        <div class="btns"><button class="btn" data-added-remove="${esc(a.name)}">Remove</button></div></div>`,

    pluginAddButtons: (plugins, busy) => `<button class="btn" data-act="plugin-zip-add" ${busy ? 'disabled' : ''}>Add Plugin Zip</button>
      ${plugins.installed.length + plugins.local.length ? `<button class="btn" data-act="plugins-remove-all" ${busy ? 'disabled' : ''}>Remove all</button>` : ''}`,

    // A plugin whose zip only the desktop launcher can fetch.
    desktopOnly: '<span class="muted small" title="It downloads from a GitHub release, which web pages can\'t fetch.">Desktop launcher only</span>',

    attach(launcher) {
      page = launcher;
    },
  };

  // How the browser reaches the world: directly, or not at all. A world whose server takes no web
  // connection is played with the desktop client; the web relay's own page is linked for anyone who
  // runs it by hand.
  function reachRows(view) {
    const r = view.reach;
    if (r.direct) return `<label class="lab">CONNECTION</label><div class="plain">Directly <span class="muted small">${esc(r.url)}</span></div>`;
    // The web relay carries two ports, and a world of Asheron's Call 2's has four.
    if (view.world.game === 'auberean') {
      return `<label class="lab">CONNECTION</label><div class="pair"><span class="small warn">This world can't be played in a browser yet.</span>
        <span class="btns"><button type="button" class="btn" data-url="${AUBEREAN_README}">Get Auberean</button></span></div>`;
    }
    return `<label class="lab">CONNECTION</label><div class="pair"><span class="small warn">This world can't be played in a browser yet.</span>
        <span class="btns"><button type="button" class="btn" data-url="${DESKTOP_RELEASE}">Get Dereth</button><button type="button" class="btn" data-url="${RELAY_README}">Web relay</button></span></div>`;
  }

  // The world's updates kept in this browser, with the buttons that remove or refuse them.
  function updatesRows(view) {
    const o = view.overlay;
    if (!o || o.state === 'none') return '';
    const text = o.state === 'busy' ? 'This world\'s updates can\'t be kept right now: another tab has them open, or this browser keeps no files.'
      : o.state === 'error' ? `This world's updates can't be kept: ${o.error}`
      : o.state === 'empty' ? 'No updates kept for this world yet.'
      : `${kb(o.bytes)} kept (${o.containers.join(', ')}).${o.blocked ? ' Refused.' : ''}`;
    const btns = [];
    if (o.state === 'kept') btns.push(`<button type="button" class="btn" data-overlay-remove="${esc(o.path)}">Remove updates</button>`);
    if (o.key) btns.push(`<button type="button" class="btn" data-block="${esc(o.key)}" data-on="${o.blocked ? 'false' : 'true'}">${o.blocked ? 'Accept updates' : 'Refuse updates'}</button>`);
    return `<label class="lab">UPDATES</label><div class="pair"><span class="small ${o.state === 'busy' || o.state === 'error' ? 'warn' : 'muted'}">${esc(text)}</span>${btns.length ? `<span class="btns">${btns.join('')}</span>` : ''}</div>`;
  }

  // ----- events on what only this page draws -----------------------------------------------------

  document.addEventListener('click', async (ev) => {
    const t = ev.target.closest('[data-overlay-remove],[data-block],[data-added-remove],[data-act="plugin-zip-add"],[data-act="plugins-remove-all"]');
    if (!t) return;
    const d = t.dataset;
    try {
      if (d.act === 'plugin-zip-add') await addPluginZips();
      if (d.act === 'plugins-remove-all') await backend.removePlugins();
      if (d.addedRemove) await backend.removeAddedPlugin(d.addedRemove);
      if (d.overlayRemove) await backend.removeOverlay(d.overlayRemove);
      if (d.block) await backend.setBlocked(d.block, d.on === 'true');
    } catch (e) {
      page.toast(String(e?.message ?? e), true);
    }
    await page.reopenWorld();
    await page.pull();
  });

  document.addEventListener('input', (ev) => {
    if (ev.target.id !== 'acct-name') return;
    const login = $('login-name');
    if (login) login.value = loginName(ev.target.value, login.dataset.loginWorld);
  });

  document.addEventListener('change', async (ev) => {
    if (ev.target.id !== 'render-scale') return;
    backend.setScale(ev.target.value);
    await page.pull();
  });

  // ----- start ------------------------------------------------------------------------------------

  // The launcher's shell (everything in its page's body but its scripts), under `#launch`.
  async function loadShell() {
    const shell = new DOMParser().parseFromString(await (await fetch('launcher.html')).text(), 'text/html');
    for (const el of [...shell.body.children]) {
      if (el.tagName !== 'SCRIPT') $('launch').append(document.importNode(el, true));
    }
  }

  function loadPage() {
    return new Promise((resolve, reject) => {
      const script = document.createElement('script');
      script.src = 'app.js';
      script.onload = resolve;
      script.onerror = () => reject(new Error('the launcher\'s page did not load'));
      document.body.append(script);
    });
  }

  // `#worlds`, `#worlds=<slug>` (that row open), `#world=<slug>`, `#accounts`, `#plugins`,
  // `#library`, `#first-run` and `#interface` open that screen directly. A screen the backend does
  // not offer is not drawn, whatever the address says.
  function jump() {
    const [what, arg] = location.hash.slice(1).split('=');
    if (!what) return null;
    const { ui } = page;
    if (what === 'world' && arg) return decodeURIComponent(arg);
    if (what === 'worlds' && arg) ui.expanded = decodeURIComponent(arg);
    if (what === 'first-run') {
      ui.firstRun.step = arg === 'guide' ? 'guide' : 'ask';
      ui.firstRun.dismissed = false;
    }
    if (what === 'interface') ui.iface.dismissed = false;
    if (what !== 'first-run' && what !== 'interface') ui.firstRun.dismissed = true;
    ui.screen = what;
    return null;
  }

  async function begin() {
    await loadShell();
    window.launcherHost = host;
    await loadPage();
    // The dev runner's files need no first start.
    if (params.get('dats') === 'http') page.ui.firstRun.dismissed = true;
    const linked = await backend.begin().catch((e) => {
      log(`front page: ${e?.message ?? e}`);
      return null;
    });
    if (linked) page.ui.firstRun.dismissed = true;
    const direct = location.hash ? jump() : null;
    begun();
    await page.pull();
    if (linked || direct) await page.openWorld(linked ?? direct);
    // Read every plugin source as the page opens.
    backend.api('refresh_plugins').then((said) => {
      if (said) page.toast(String(said).replace(/\n/g, ' '));
      return page.pull();
    }, (e) => log(`plugins: ${e?.message ?? e}`));
    // The hub's word on each world, while the page is in sight.
    setInterval(async () => {
      if (backend.playing() || document.hidden) return;
      try {
        await backend.readHub();
      } catch (e) {
        // A client started while the hub was read refuses the rest; that is no error.
        if (!backend.playing()) log(`the hub's status: ${e?.message ?? e}`);
      }
    }, 5000);
  }

  return {
    begin,
    // The client ended (its own Exit, or a lost connection) or could not start: the launcher's page
    // again, as it was, with what is kept read again and what went wrong said.
    async back(error) {
      backend.ended();
      await backend.readKept().catch(() => {});
      if (error) backend.say(error, true);
      await page.reopenWorld();
      await page.pull();
    },
  };
}
