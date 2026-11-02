// The front page: the desktop launcher's Dereth realm, in the browser. Its screens are the
// desktop's (Home, Worlds and a world's page, Accounts, Plugins, the Library, the first start's
// questions), with the same names, markup and words, drawn from one snapshot of the launcher's
// backend (`backend.js`) and acting through one command at a time, by the desktop's command names.
//
// The page keeps only what is on screen and being typed: which screen, the world page's choices,
// the search box. Where the browser differs (how a world is reached, its updates kept here, the
// files picked in the browser, passwords kept by the browser's password manager) the screens say
// so in the browser's words, and those parts are marked below.
//
// What the desktop launcher has and a browser cannot (hosting a world, building one, the retail
// client, downloads of the launcher's programs, Discord, a gamepad, a desktop shortcut) is not
// offered. The realm slot above the rail is kept, empty: Dereth's is the only realm here.

import { loginName, webBackend } from './backend.js';

const GUIDE = 'https://www.accpp.net/manual-installation';
const RELAY_README = 'https://github.com/dereth-network/dereth/blob/main/tools/web-relay/README.md';
// The desktop client, for a world a browser can't reach.
const DESKTOP_RELEASE = 'https://github.com/dereth-network/dereth/releases/latest';

// How long a notice stays before it goes by itself.
const NOTICE_MS = 7000;

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);

export function startFront({ call, start, params, log, scale }) {
  const backend = webBackend({ call, start, params, log, scale });
  const api = backend.api;

  const ui = {
    // The Plugins page: the source being added and whether adding it waits to be confirmed, the
    // search, the slow call under way, the one open row, and the closed sections (all but
    // Installed at first).
    plugin: { add: '', confirm: false, search: '', busy: null, open: null, folded: { installed: false, available: true, sources: true } },
    screen: 'home', // home | worlds | world | library | accounts | plugins | first-run | interface
    realm: 'dereth',
    snap: null,
    homeSel: 0,
    search: '',
    filter: null, // the Worlds list's one rules filter: null | "pve" | "pvp"
    expanded: null, // the Worlds list's one open row: a world's slug, or null
    folded: { favorites: false, custom: false, online: false, offline: true },
    addServer: null, // the Worlds list's "add a world" form, while it is open
    world: null, // the world page: { view, form, notice }
    firstRun: { step: 'ask', added: null, dismissed: false },
    iface: { pick: null, fromLibrary: false, dismissed: false },
    folds: {},
    playing: false,
  };

  // ----- small helpers -----------------------------------------------------------------------

  const worldBy = (slug) => ui.snap?.worlds.find((w) => w.slug === slug);
  const ROLES = ['portal', 'cell', 'local', 'highres'];
  const iterLabel = (it) => ROLES.map((r) => it?.[r] ?? '-').join('/');
  function setIterations(set) {
    const it = {};
    for (const f of set.files) it[f.role] = f.iterations ?? undefined;
    return it;
  }
  const isEndOfRetail = (it) => it.portal === 2072 && it.cell === 982 && it.local === 994 && (it.highres == null || it.highres === 497);
  function longDate(iso) {
    const [y, m, d] = String(iso || '').split('-').map(Number);
    const months = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
    return y && m && d ? `${d} ${months[m - 1]} ${y}` : iso;
  }
  function setName(s) {
    if (!s) return 'No data files';
    const it = setIterations(s);
    if (s.kind === 'classic') return `Classic  ${it.portal ?? '-'}/${it.cell ?? '-'}`;
    return `${isEndOfRetail(it) ? 'Modern' : 'Modern (older)'}  ${iterLabel(it)}`;
  }
  const eraLabel = (name) => (name ? ui.snap?.eras.find((e) => e.name === name)?.label ?? name : null);
  // The era a world plays, in words: the one it names, else the end of retail; "Unknown" for a
  // world that may play any era and has not said which (a browser can't ask it over UDP).
  const eraText = (w) => (w.era_state === 'checking' ? 'Checking…' : w.era_state === 'unknown' ? 'Unknown' : eraLabel(w.era || 'eor'));
  // The era among other facts, where "Unknown" alone would not say what is unknown.
  const eraFact = (w) => (w.era_state === 'checking' ? 'checking era…' : w.era_state === 'unknown' ? 'era unknown' : eraText(w));
  function emulatorLabel(w) {
    const name = { empyrean: 'Empyrean', ace: 'ACE', classic_ace: 'ClassicACE', gdle: 'GDLE', unknown: 'Unknown' }[w.emulator] ?? w.emulator;
    const v = (w.emulator_version ?? '').replace(/^v/i, '');
    return v ? `${name} | v${v}` : name;
  }
  function worldFeatures(w) {
    const era = ui.snap.eras.find((e) => e.name === (w.era || 'eor')) ?? ui.snap.eras[0];
    const base = era?.features ?? {};
    const said = {};
    for (const part of String(w.era_features ?? '').split(',')) {
      const [k, v] = part.split('=').map((x) => (x ?? '').trim().toLowerCase());
      if (k in base) said[k] = v === 'true' || v === 'on' || v === '1';
    }
    const f = { ...base, ...said };
    if (said.housing === false && !('apartments' in said)) f.apartments = false;
    if (f.apartments) f.housing = true;
    return f;
  }
  const stateLabel = (w) => ({ online: 'online', high: 'busy', offline: 'offline', starting: 'starting', unknown: 'status unknown' }[w.state] ?? w.state);
  const bead = (cls) => `<i class="dot ${esc(cls)}"></i>`;
  const mb = (n) => `${Math.max(1, Math.round(n / 1048576))} MB`;
  const kb = (n) => (n >= 1048576 ? mb(n) : `${Math.max(1, Math.round(n / 1024))} KB`);
  const size = (n) => (n >= 1073741824 ? `${(n / 1073741824).toFixed(1)} GB` : mb(n));
  const reachOf = (slug) => ui.snap?.web.reach[slug] ?? { direct: false };

  const CHEVRON_LEFT = '<svg viewBox="0 0 8 12" fill="currentColor" aria-hidden="true"><rect x="5" y="1" width="2" height="2"/><rect x="3" y="3" width="2" height="2"/><rect x="1" y="5" width="2" height="2"/><rect x="3" y="7" width="2" height="2"/><rect x="5" y="9" width="2" height="2"/></svg>';

  // ----- data from the backend ---------------------------------------------------------------

  // Redrawn only when something changed: an open drop-down would otherwise close under the player.
  let lastSnap = '';
  async function pull() {
    if (ui.playing) return;
    let snap;
    try {
      snap = await api('snapshot');
    } catch (e) {
      log(`front page: ${e?.message ?? e}`);
      return;
    }
    const json = JSON.stringify(snap);
    ui.snap = snap;
    // The first start asks which interface the client plays in, before anything else, while the
    // client's preferences name none.
    if ((ui.screen === 'home' || ui.screen === 'first-run') && interfaceAsked()) {
      ui.screen = 'interface';
      ui.iface = { ...ui.iface, pick: null, fromLibrary: false };
      lastSnap = '';
    }
    if (ui.screen === 'home' && snap.state.dat_sets.length === 0 && !ui.firstRun.dismissed && !snap.web.dev_dats) {
      ui.screen = 'first-run';
      lastSnap = '';
    }
    if (json === lastSnap) return;
    lastSnap = json;
    render();
  }

  async function act(cmd, args) {
    try {
      const r = await api(cmd, args);
      await pull();
      return r;
    } catch (e) {
      toast(String(e?.message ?? e), true);
      return undefined;
    }
  }

  let localToast = null;
  function toast(text, error) {
    localToast = { text, error };
    render();
    setTimeout(() => {
      if (localToast?.text === text) {
        localToast = null;
        render();
      }
    }, NOTICE_MS);
  }

  // The backend's message goes by itself too, a while after it first appears.
  let shownMessage = null;
  function expireMessage(msg) {
    const key = msg ? `${msg.error}:${msg.text}` : null;
    if (key === shownMessage) return;
    shownMessage = key;
    if (!key) return;
    setTimeout(() => {
      if (shownMessage === key) act('dismiss_message');
    }, NOTICE_MS);
  }

  // ----- rendering ---------------------------------------------------------------------------

  // Re-rendering replaces the screen, so the field being typed into is found again afterwards and
  // the caret put back where it was.
  function render() {
    if (!ui.snap) return;
    const active = document.activeElement;
    const keepFocus = active && active.id && $('launch').contains(active) ? { id: active.id, start: active.selectionStart, end: active.selectionEnd } : null;
    const railDrawn = rail();
    const railHost = $('rail');
    if (railHost.dataset.drawn !== railDrawn) {
      railHost.innerHTML = railDrawn;
      railHost.dataset.drawn = railDrawn;
    }
    const footnote = foot();
    const footHost = $('foot');
    if (footHost.dataset.drawn !== footnote) {
      footHost.innerHTML = footnote;
      footHost.dataset.drawn = footnote;
    }
    const notice = statusArea();
    const host = $('notice');
    if (host.dataset.drawn !== notice) {
      host.innerHTML = notice;
      host.dataset.drawn = notice;
    }
    expireMessage(ui.snap.message);
    const main = $('main');
    const screens = { home, worlds, world: worldPage, library, accounts, plugins, 'first-run': firstRun, interface: interfacePage };
    const sameScreen = main.dataset.screen === ui.screen;
    const openDetails = sameScreen ? [...main.querySelectorAll('details[open][id]')].map((el) => el.id) : [];
    const scrolled = sameScreen ? [...main.querySelectorAll('.scroll')].map((el) => el.scrollTop) : [];
    main.innerHTML = (screens[ui.screen] ?? home)();
    main.dataset.screen = ui.screen;
    for (const id of openDetails) {
      const detail = $(id);
      if (detail) detail.open = true;
    }
    main.querySelectorAll('.scroll').forEach((el, i) => {
      if (scrolled[i] !== undefined) el.scrollTop = scrolled[i];
    });
    if (keepFocus) {
      const el = $(keepFocus.id);
      if (el) {
        el.focus();
        if (keepFocus.start != null && el.setSelectionRange) {
          try {
            el.setSelectionRange(keepFocus.start, keepFocus.end);
          } catch {
            // Not a text field.
          }
        }
      }
    }
  }

  // A section that folds: open or shut as the player left it, else as the page opens it.
  function fold(id, title, body, open) {
    const isOpen = ui.folds[id] ?? open;
    return `<details class="fold" id="${esc(id)}" ${isOpen ? 'open' : ''}><summary><h2>${title}</h2></summary><div class="fold-body">${body}</div></details>`;
  }
  document.addEventListener('toggle', (ev) => {
    const d = ev.target;
    if (d.classList?.contains('fold') && d.id) ui.folds[d.id] = d.open;
  }, true);

  function rail() {
    const lit = { world: 'worlds', 'first-run': 'home', interface: ui.iface.fromLibrary ? 'library' : 'home' }[ui.screen] ?? ui.screen;
    const item = (id, label) => `<button class="${lit === id ? 'lit' : ''}" data-go="${id}">${label}</button>`;
    const divider = '<div class="rail-divider" role="separator"></div>';
    return [item('home', 'PLAY'), item('worlds', 'WORLDS'), item('accounts', 'ACCOUNTS'), item('plugins', 'PLUGINS'), divider, item('library', 'LIBRARY')].join('');
  }

  // The footnote under the rail: the client's version and the website.
  function foot() {
    return `<span>Dereth ${esc(ui.snap.version)}</span><span class="sep">|</span><a href="https://dereth.network" target="_blank" rel="noopener">dereth.network</a>`;
  }

  // The notice: what is under way and the last message, floating over the top right corner.
  function statusArea() {
    const s = ui.snap;
    const job = s.job;
    const msg = localToast ?? s.message;
    if (!job && !msg) return '';
    let html = '<div class="status panel">';
    if (job) {
      html += '<div class="bar sweep"><i style="width:33%"></i></div>';
      html += `<div class="line"><span>${esc(job.what)}...</span></div>`;
    }
    if (msg) html += `<div class="line ${msg.error ? 'error' : ''}"><span>${esc(msg.text)}</span><button data-act="dismiss" title="Dismiss">✕</button></div>`;
    return `${html}</div>`;
  }

  // ----- Home --------------------------------------------------------------------------------

  // Every favourite, then the recent combinations that are not favourites, as the launcher lists
  // them.
  const homeEntries = () => ui.snap.home;

  function home() {
    const entries = homeEntries();
    ui.homeSel = Math.min(ui.homeSel, Math.max(0, entries.length - 1));
    const sel = entries[ui.homeSel];
    const canPlay = !!(sel && sel.account && worldBy(sel.world_slug)?.endpoint);
    let list;
    if (entries.length === 0) {
      list = `<p class="muted">Nothing here yet. Pick a world to play, and it will be here next time.</p>
        <div class="btns"><button class="btn" data-go="worlds">Browse worlds</button></div>`;
    } else {
      list = `<div class="list">${entries.map((e, i) => {
        const w = worldBy(e.world_slug);
        const era = w ? eraFact(w) : 'era unknown';
        const end = !w ? 'not listed' : w.state === 'offline' ? 'offline' : w.players != null ? `${w.players} online` : 'players unknown';
        const pin = e.favourite
          ? `<span class="star" title="Favorite">★</span><button class="btn icon-btn" data-unfav="${esc(e.favourite)}" title="Remove from favorites">✕</button>`
          : `<button class="btn icon-btn" data-fav="${i}" title="Add to favorites">☆</button>`;
        return `<div class="row ${i === ui.homeSel ? 'sel' : ''} ${e.favourite ? 'fav' : ''}" data-home="${i}" data-dbl-home="${i}">
            ${bead(w?.state ?? 'unknown')}
            <div class="grow"><div class="name">${esc(w?.name ?? e.world_slug)}</div>
            <div class="sub">${esc(e.account || 'no account yet')} · ${esc(era)}</div></div>
            <div class="end">${esc(end)}</div>
            <div class="pin">${pin}</div>
          </div>`;
      }).join('')}</div>`;
    }
    return `<div class="column narrow">
        <section class="panel"><header><h1>PLAY AGAIN</h1></header>
          <div class="scroll">${list}</div></section>
        <button class="play" data-act="home-play" ${canPlay ? '' : 'disabled'}>PLAY</button>
      </div>`;
  }

  // ----- Worlds ------------------------------------------------------------------------------

  const FILTERS = [['pve', 'PvE'], ['pvp', 'PvP']];
  const EMULATORS = [['unknown', 'Unknown'], ['empyrean', 'Empyrean'], ['ace', 'ACE'], ['classic_ace', 'ClassicACE'], ['gdle', 'GDLE']];
  const isCustom = (w) => ui.snap.state.custom_worlds.some((c) => c.slug === w.slug);
  const SECTIONS = [['favorites', 'FAVORITES'], ['custom', 'CUSTOM'], ['online', 'ONLINE'], ['offline', 'OFFLINE']];
  const sectionOf = (w) => (w.favorite ? 'favorites' : isCustom(w) ? 'custom' : w.state === 'offline' ? 'offline' : 'online');

  // The favourites first, then the worlds the browser reaches directly, then by state and players.
  function visibleWorlds() {
    const q = ui.search.trim().toLowerCase();
    const rank = { online: 0, high: 0, starting: 1, unknown: 2, offline: 3 };
    const direct = (w) => (reachOf(w.slug).direct ? 1 : 0);
    return ui.snap.worlds
      .filter((w) => !q || w.name.toLowerCase().includes(q) || (w.description ?? '').toLowerCase().includes(q))
      .filter((w) => {
        const rs = (w.ruleset ?? '').toLowerCase();
        if (ui.filter === 'pve') return rs.includes('pve');
        if (ui.filter === 'pvp') return rs.includes('pvp') || rs.includes('pk');
        return true;
      })
      .sort((a, b) => (direct(b) - direct(a)) || (rank[a.state] - rank[b.state]) || ((b.players ?? 0) - (a.players ?? 0)) || a.name.localeCompare(b.name));
  }

  const playersLabel = (w) => (w.state === 'offline' ? 'offline' : w.players != null ? `${w.players}` : 'unknown');
  // How the browser reaches a world, in a few words.
  const reachText = (slug) => (reachOf(slug).direct ? 'Directly' : 'Not in a browser yet');

  // An open row: what the world says about itself, its era and systems, how it is reached, and the
  // way to its page.
  function worldDetail(w) {
    const links = [['Website', w.links.website], ['Discord', w.links.discord], ['Rules', w.links.rules]].filter(([, u]) => u);
    const named = w.era_source === 'world' || w.features_source === 'world';
    const on = worldFeatures(w);
    const boxes = named
      ? ui.snap.features.map((f) => `<label class="check small"><input type="checkbox" ${on[f.name] ? 'checked' : ''} disabled>${esc(f.label)}</label>`).join('')
      : '';
    const said = w.era_state === 'checking' ? 'Checking which era this world plays.'
      : w.era_state === 'unknown' ? 'This world has not said its era.'
      : w.era_state === 'assumed' ? "Assumed: the list doesn't say."
      : !named ? `This world does not name its era: it plays ${eraText(w)}.`
      : w.era_source !== 'world' ? `This world says the systems it has, and plays ${eraText(w)}.`
      : w.features_source === 'world' ? 'This world says its era and the systems it has.'
      : 'This world says its era, and has that era\'s systems.';
    const facts = [['EMULATOR', emulatorLabel(w)], ['STATUS', w.development_status ?? 'unknown'], ['RULES', w.ruleset ?? 'unknown'], ['PLAYERS', playersLabel(w)], ['REACHED', reachText(w.slug)]];
    return `<tr class="detail" data-stop><td></td><td colspan="6">
        <div class="detail-body">
          <div class="detail-top">
            <p class="${w.description ? '' : 'muted'}">${esc(w.description ?? 'No description.')}</p>
            <div class="detail-actions">
              <button class="play mini" data-open-world="${esc(w.slug)}" title="Play on ${esc(w.name)}">PLAY</button>
              ${isCustom(w) ? `<button class="btn" data-trash="${esc(w.slug)}" title="Remove ${esc(w.name)}">Remove</button>` : ''}
            </div>
          </div>
          <div class="strip">${facts.map(([k, v]) => `<span><b>${k}</b>${esc(v)}</span>`).join('')}</div>
          ${links.length ? `<div class="btns">${links.map(([l, u]) => `<button class="btn" data-url="${esc(u)}">${l}</button>`).join('')}</div>` : ''}
          <div class="era-line"><b>ERA</b><span>${esc(eraText(w))}</span></div>
          <p class="small muted">${esc(said)}</p>
          ${boxes ? `<div class="features">${boxes}</div>` : ''}
        </div></td></tr>`;
  }

  function worldRow(w) {
    const open = ui.expanded === w.slug;
    const pin = w.favorite
      ? `<span class="star" title="Favorite">★</span><button class="btn icon-btn" data-world-fav="${esc(w.slug)}" title="Remove from favorites">✕</button>`
      : `<button class="btn icon-btn" data-world-fav="${esc(w.slug)}" title="Add to favorites">☆</button>`;
    return `<tr data-world="${esc(w.slug)}" class="${w.state === 'offline' ? 'dim' : ''} ${open ? 'open' : ''}">
        <td>${bead(w.state)}</td><td class="name">${esc(w.name)}</td>
        <td>${esc(emulatorLabel(w))}</td><td>${esc(eraText(w))}</td>
        <td>${esc(w.ruleset ?? '')}</td><td>${esc(playersLabel(w))}</td><td class="end"><div class="pin">${pin}</div></td></tr>`
      + (open ? worldDetail(w) : '');
  }

  function worlds() {
    const rows = visibleWorlds();
    const s = ui.snap;
    const searching = ui.search.trim() !== '';
    let body;
    if (rows.length === 0) {
      const msg = s.list_state === 'loading' ? 'Fetching the world list...'
        : s.list_state === 'unavailable' && s.worlds.length === 0 ? `${s.list_error ?? ''} Add a world by its address.`
        : s.worlds.length === 0 ? 'No worlds are listed.' : 'No world matches these filters.';
      body = `<p class="muted">${esc(msg)}</p>`;
    } else {
      body = `<table class="worlds"><colgroup><col class="c-dot"><col class="c-world"><col class="c-emu"><col class="c-era"><col class="c-rules"><col class="c-players"><col class="c-end"></colgroup>
        <thead><tr><th></th><th>WORLD</th><th>EMULATOR</th><th>ERA</th><th>RULES</th><th>PLAYERS</th><th></th></tr></thead><tbody>${
        SECTIONS.map(([key, label]) => {
          const these = rows.filter((w) => sectionOf(w) === key);
          if (these.length === 0) return '';
          const open = searching || !ui.folded[key];
          return `<tr class="section ${open ? '' : 'shut'}" data-section="${key}"><td colspan="7"><span class="fold">${open ? '▾' : '▸'}</span>${label}<span class="muted small">${these.length}</span></td></tr>`
            + (open ? these.map(worldRow).join('') : '');
        }).join('')}</tbody></table>`;
    }
    const fetched = s.list_fetched_at ? new Date(s.list_fetched_at * 1000) : null;
    const stale = s.list_state === 'unavailable' && s.worlds.length
      ? `<p class="small warn">${esc(s.list_error ?? '')} Showing the list from ${esc(fetched ? fetched.toLocaleString() : 'before')}.</p>` : '';
    const a = ui.addServer;
    const emulatorOpts = EMULATORS.map(([v, l]) => `<option value="${v}" ${a?.emulator === v ? 'selected' : ''}>${l}</option>`).join('');
    const rulesOpts = [['', 'Unknown'], ['PvE', 'PvE'], ['PvP', 'PvP']].map(([v, l]) => `<option value="${v}" ${a?.ruleset === v ? 'selected' : ''}>${l}</option>`).join('');
    const form = a
      ? `<section class="panel pending"><header><h1>ADD A WORLD</h1></header>
          <div class="form">
            <label class="lab" for="srv-name">NAME</label><input id="srv-name" class="field" autocomplete="off" placeholder="optional" value="${esc(a.name)}">
            <label class="lab" for="srv-address">ADDRESS</label><input id="srv-address" class="field" autocomplete="off" spellcheck="false" placeholder="host:port, a wss:// address, or a status address" value="${esc(a.address)}">
            <label class="lab" for="srv-emulator">EMULATOR</label><select id="srv-emulator" class="field" style="max-width:220px">${emulatorOpts}</select>
            <label class="lab" for="srv-ruleset">RULES</label><select id="srv-ruleset" class="field" style="max-width:220px">${rulesOpts}</select>
            ${a.error ? `<p class="note bad">${esc(a.error)}</p>` : ''}
          </div>
          <div class="btns end" style="margin-top:12px"><button class="btn" data-act="server-cancel">Cancel</button><button class="btn" data-act="server-add">Add</button></div>
        </section>`
      : '';
    return `<div class="column wide" style="height:100%">
        ${form}
        <section class="panel grow"><header><h1>WORLDS</h1><span class="muted small">${rows.length} worlds</span>
          <button class="btn" data-act="server-open" title="Add a world by its address">+ Add world</button>
          <button class="btn icon-btn" data-act="refresh" title="Fetch the world list again">↻</button></header>
          <div class="btns" style="margin-bottom:12px">
            <input id="search" class="field" autocomplete="off" style="max-width:360px" placeholder="search" value="${esc(ui.search)}">
            <div class="toggle" role="group" aria-label="Rules">${FILTERS.map(([k, l]) => `<button class="${ui.filter === k ? 'on' : ''}" data-filter="${k}" aria-pressed="${ui.filter === k}">${l}</button>`).join('')}</div>
          </div>
          ${stale}
          <div class="scroll">${body}</div>
        </section>
      </div>`;
  }

  // ----- one world ---------------------------------------------------------------------------

  async function openWorld(slug, notice, preset) {
    let view;
    try {
      view = await api('world_view', { slug });
    } catch (e) {
      toast(String(e?.message ?? e), true);
      return;
    }
    const p = view.prefs;
    const known = view.accounts.map((a) => a.username);
    const account = preset?.account ?? p.account ?? known[0] ?? params.get('account') ?? '';
    const validSet = (id) => view.dat_sets.some((s) => s.id === id);
    const validClassic = (id) => view.classic_sets.some((s) => s.id === id);
    const fallback = view.dat_sets.find((s) => s.origin.kind === 'shared') ?? view.dat_sets[0];
    const set = [preset?.dat_set_id, p.dat_set_id].find(validSet) ?? fallback?.id ?? (ui.snap.web.dev_dats ? '__http' : null);
    const classicDefault = view.classic_sets.find((s) => s.origin.kind === 'shared');
    const classic = [preset?.classic_set_id, p.classic_set_id].find(validClassic) ?? classicDefault?.id ?? null;
    ui.world = {
      view,
      form: { account, password: preset?.password ?? '', dat_set_id: set, classic_set_id: classic },
      notice: notice ?? null,
    };
    ui.screen = 'world';
    render();
    focusSoon(account ? 'password' : 'acct-name');
  }

  function focusSoon(id) {
    setTimeout(() => $(id)?.focus(), 0);
  }

  function formChoice() {
    const w = ui.world;
    const f = w.form;
    if (!f.account.trim()) return null;
    const dats = f.dat_set_id === '__http' ? 'http' : f.dat_set_id === '__visit' ? 'visit' : undefined;
    return {
      world_slug: w.view.world.slug,
      account: f.account.trim(),
      password: f.password || null,
      dat_set_id: dats ? null : f.dat_set_id,
      classic_set_id: f.classic_set_id,
      dats,
    };
  }

  // Whether the client has the data files the world's era needs: the Modern files always, and
  // the Classic pair too for an era before Throne of Destiny.
  function hasRequiredSet() {
    const f = ui.world.form;
    // The dev runner serves its Classic files beside its Modern ones.
    return !!f.dat_set_id && (ui.world.view.requires !== 'classic' || f.dat_set_id === '__http' || !!f.classic_set_id);
  }

  function worldCanPlay() {
    const world = worldBy(ui.world.view.world.slug) ?? ui.world.view.world;
    return !!formChoice() && !!world.endpoint && !!ui.world.view.reach?.direct && hasRequiredSet();
  }
  function worldNotice() {
    const w = ui.world;
    const f = w.form;
    const world = worldBy(w.view.world.slug) ?? w.view.world;
    const creates = world.account_model === 'auto_create_on_first_login' && f.account.trim() && !w.view.accounts.some((a) => a.username.toLowerCase() === f.account.trim().toLowerCase());
    return creates ? 'New here? This world creates your account the first time you sign in, so check the spelling.' : w.notice ?? '';
  }
  function syncWorld() {
    const play = document.querySelector('[data-act="world-play"]');
    if (play) play.disabled = !worldCanPlay();
    const note = $('world-note');
    if (note) {
      const text = worldNotice();
      note.textContent = text;
      note.hidden = !text;
    }
  }

  // How the browser reaches the world (web only): directly, or not at all. A world whose server
  // takes no web connection is played with the desktop client; the web relay's own page is linked
  // for anyone who runs it by hand.
  function reachBlock(view) {
    const r = view.reach;
    if (r.direct) {
      return `<label class="lab">CONNECTION</label><div class="plain">Directly <span class="muted small">${esc(r.url)}</span></div>`;
    }
    return `<label class="lab">CONNECTION</label><div class="pair"><span class="small warn">This world can't be played in a browser yet.</span>
        <span class="btns"><button type="button" class="btn" data-url="${DESKTOP_RELEASE}">Get Dereth</button><button type="button" class="btn" data-url="${RELAY_README}">Web relay</button></span></div>`;
  }

  // The world's updates kept in this browser (web only), with the buttons that remove or refuse them.
  function updatesBlock(view) {
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

  function worldPage() {
    const w = ui.world;
    if (!w) return home();
    const view = w.view;
    const world = worldBy(view.world.slug) ?? view.world;
    const f = w.form;
    const facts = [stateLabel(world), world.players != null ? `${world.players} players` : 'players unknown', world.ruleset, eraFact(world), emulatorLabel(world)].filter(Boolean);
    const links = [['Website', world.links.website], ['Discord', world.links.discord], ['Rules', world.links.rules]].filter(([, u]) => u);
    const custom = isCustom(world);
    const note = worldNotice();
    const needs = view.requires;
    const option = (s, chosen) => `<option value="${esc(s.id)}" ${s.id === chosen ? 'selected' : ''}>${esc(setName(s))}${s.origin.kind === 'shared' ? '  (default)' : ''}</option>`;
    const none = (chosen) => `<option value="" ${chosen ? '' : 'selected'}>None</option>`;
    const picked = ui.snap.web.picked;
    const setOpts = none(f.dat_set_id) + view.dat_sets.map((s) => option(s, f.dat_set_id)).join('')
      + (ui.snap.web.dev_dats ? `<option value="__http" ${f.dat_set_id === '__http' ? 'selected' : ''}>The dev runner's files</option>` : '')
      + `<option value="__visit" ${f.dat_set_id === '__visit' ? 'selected' : ''}>${picked ? `Picked for this visit (${picked.length} files)` : 'Pick files for this visit only...'}</option>`
      + `<option value="__add">Add ${world.dats?.custom ? `${esc(world.name)}'s` : 'data'} files to this browser...</option>`;
    const classicOpts = none(f.classic_set_id) + view.classic_sets.map((s) => option(s, f.classic_set_id)).join('');
    const eraName = eraText(world);
    const dataNote = world.era_state === 'checking' || world.era_state === 'unknown'
      ? 'This world has not said its era yet. It needs Modern data files, and Classic ones too if it plays an era before Throne of Destiny.'
      : world.world_base === 'modern' && world.dats.custom
      ? `${world.name} plays ${eraName} over its own Modern data files. Classic ones are optional, for the classic interface and looks.`
      : needs === 'classic'
        ? `${eraName} is before Throne of Destiny: it needs Classic data files, which its world is drawn from, and Modern ones beside them for the interface.`
        : `${eraName} needs Modern data files. Classic ones are optional, for the classic interface and looks.`;
    const missing = !hasRequiredSet();
    const ownFiles = world.dats.custom && view.dat_sets.length === 0
      ? `<p class="note warn">${esc(view.refused_sets[0]?.why ?? world.dats.custom.license_note ?? 'This world plays with data files of its own.')}
           ${world.dats.custom.url ? `<button type="button" class="btn" data-url="${esc(world.dats.custom.url)}">Where to get them</button>` : ''}</p>`
      : '';
    const join = view.join_link
      ? `<label class="lab">JOIN LINK</label><div class="pair"><span class="small muted join">${esc(view.join_link)}</span><button type="button" class="btn mini-btn" data-copy-link="${esc(view.join_link)}">Copy link</button></div>`
      : '';
    return `<form class="column wide" id="world-form" autocomplete="on">
        <section class="panel"><header><button type="button" class="btn icon-btn back" data-go="worlds" title="Back to worlds">${CHEVRON_LEFT}</button><h1>${esc(world.name.toUpperCase())}</h1>
            ${links.map(([l, u]) => `<button type="button" class="btn" data-url="${esc(u)}">${l}</button>`).join('')}
            ${custom ? '<button type="button" class="btn" data-act="server-remove">Remove world</button>' : ''}</header>
          <div class="scroll">
            <div class="facts">${bead(world.state)}<span>${esc(facts.join('  ·  '))}</span></div>
            ${world.era_state === 'assumed' ? `<p class="muted small">Assumed: the list doesn't say.</p>` : ''}
            ${world.description ? `<p class="muted small">${esc(world.description)}</p>` : ''}
            <div class="form" style="margin-top:14px">
              <label class="lab" for="acct-name">ACCOUNT</label><input id="acct-name" name="account" class="field" autocomplete="off" autocapitalize="off" spellcheck="false" placeholder="account name" value="${esc(f.account)}">
              <input id="login-name" name="username" class="login-name" autocomplete="username" tabindex="-1" aria-hidden="true" value="${esc(loginName(f.account, world.name))}">
              <label class="lab" for="password">PASSWORD</label><input id="password" name="password" class="field" type="password" autocomplete="current-password" value="${esc(f.password)}">
              <p class="note muted">Your browser's password manager keeps your password.</p>
              <p class="note warn" id="world-note" ${note ? '' : 'hidden'}>${esc(note)}</p>
              <label class="lab" for="dats">MODERN DATA</label><select id="dats" class="field">${setOpts}</select>
              <label class="lab" for="classic">CLASSIC DATA</label><select id="classic" class="field">${classicOpts}</select>
              <p class="note ${missing ? 'warn' : 'muted'}">${esc(dataNote)}</p>${ownFiles}
              ${reachBlock(view)}
              ${updatesBlock(view)}
              ${join}
            </div>
          </div>
        </section>
        <button type="submit" class="play" data-act="world-play" ${worldCanPlay() ? '' : 'disabled'}>PLAY</button>
      </form>`;
  }

  async function play(choice) {
    let out;
    try {
      out = await api('launch', { choice });
    } catch (e) {
      toast(String(e?.message ?? e), true);
      return;
    }
    // The client takes the page; the screen it started from is there when it ends. Nothing is
    // asked of the backend while it runs.
    if (out.kind === 'launched') {
      ui.playing = true;
      ui.homeSel = 0;
      return;
    }
    await pull();
    if (out.kind === 'need_password') {
      if (ui.screen === 'world' && ui.world?.view.world.slug === choice.world_slug) {
        ui.world.notice = out.message;
        render();
        focusSoon('password');
      } else {
        await openWorld(choice.world_slug, out.message, { ...choice, password: '' });
      }
    } else {
      toast(out.message, true);
    }
  }

  // ----- the library -------------------------------------------------------------------------

  const SCALES = ['100', '85', '75', '67', '50'];

  function library() {
    const s = ui.snap.state;
    const kept = ui.snap.web.kept;
    const setRow = (d) => {
      const bytes = d.files.reduce((n, f) => n + (f.size ?? 0), 0);
      const pairBroken = d.kind === 'classic' && !(d.files.some((f) => f.role === 'portal') && d.files.some((f) => f.role === 'cell'));
      const facts = [size(bytes), d.files.some((f) => f.error) && 'some files unreadable', pairBroken && 'portal.dat or cell.dat is missing'].filter(Boolean);
      const isDefault = d.origin.kind === 'shared';
      const defaultBtn = isDefault ? '<span class="tag">DEFAULT</span>' : `<button class="btn" data-default-set="${esc(d.id)}">Set default</button>`;
      return `<div class="row static"><div class="grow"><div class="name">${esc(setName(d))} <span class="muted small">${esc(facts.join('; '))}</span></div>
          <div class="sub">Kept in this browser as ${esc(String(d.path).split('/').pop())}</div></div>
          <div class="btns">${defaultBtn}<button class="btn" data-delete-set="${esc(d.id)}">Delete</button></div></div>`;
    };
    const modern = s.dat_sets.filter((d) => d.kind !== 'classic').map(setRow).join('')
      || '<p class="muted">No Modern data files yet. Add the client_*.dat files.</p>';
    const classic = s.dat_sets.filter((d) => d.kind === 'classic').map(setRow).join('')
      || '<p class="muted">No Classic data files yet. Add portal.dat and cell.dat.</p>';
    const updates = kept.overlays.map((o) => `<div class="row static"><div class="grow"><div class="name">${esc(o.path.split('/').pop())}</div>
        <div class="sub">${esc(kb(o.bytes))}</div></div><div class="btns"><button class="btn" data-overlay-remove="${esc(o.path)}">Remove</button></div></div>`).join('')
      || '<p class="muted">No world has sent updates yet.</p>';
    const st = kept.storage ?? {};
    const used = st.usage != null ? `${size(st.usage)} used${st.quota ? ` of ${size(st.quota)}` : ''}` : 'Not known';
    const storage = `<div class="row static"><div class="grow"><div class="name">${esc(used)}</div>
        <div class="sub">${st.persisted ? 'This browser keeps these files until you remove them.' : 'This browser may clear these files when it runs short of space. Playing asks it to keep them.'}</div></div></div>`;
    const scaleOpts = SCALES.map((v) => `<option value="${v}" ${ui.snap.web.scale === v ? 'selected' : ''}>${v}%</option>`).join('');
    const dereth = `<div class="row static"><div class="grow"><div class="name">Dereth ${esc(ui.snap.version)}</div><div class="sub">Runs in this browser.</div></div></div>
      ${interfaceRow()}
      <div class="row static"><div class="grow"><div class="name">Render scale</div><div class="sub">Fewer pixels draw faster and look softer.</div></div>
        <div class="btns"><select id="render-scale" class="field mini" aria-label="Render scale">${scaleOpts}</select></div></div>`;
    return `<div class="column wide" style="height:100%">
        <section class="panel grow"><header><h1>LIBRARY</h1><button class="btn" data-act="add-dats">+ Add data files</button></header>
          <div class="scroll">
            <h2>DERETH CLIENT</h2><div class="list">${dereth}</div>
            <h2>MODERN DATA SETS</h2><p class="muted small">The files from a modern install: client_*.dat.</p><div class="list">${modern}</div>
            <h2>CLASSIC DATA SETS</h2><p class="muted small">The files from older installs: portal.dat and cell.dat.</p><div class="list">${classic}</div>
            <h2>WORLD UPDATES</h2><p class="muted small">What worlds have patched, kept apart from your files.</p><div class="list">${updates}</div>
            <h2>STORAGE</h2><div class="list">${storage}</div>
          </div>
        </section>
      </div>`;
  }

  // ----- accounts ----------------------------------------------------------------------------

  function accounts() {
    const s = ui.snap.state;
    const rows = s.accounts.map((a, i) => `<div class="row static"><div class="grow"><div class="name">${esc(worldBy(a.world_slug)?.name ?? a.world_slug)} · ${esc(a.username)}</div></div>
          <button class="btn" data-forget-account="${i}" title="Forget this account">Forget</button></div>`).join('')
      || '<p class="muted">No accounts yet. They are added the first time you sign in to a world.</p>';
    return `<div class="column wide" style="height:100%">
        <section class="panel grow"><header><h1>ACCOUNTS</h1></header>
          <div class="scroll"><div class="list">${rows}</div></div>
          <p class="small muted">Your browser's password manager keeps the passwords.</p>
          <div class="btns end" style="margin-top:12px"><button class="btn" data-act="forget-all-accounts" ${s.accounts.length ? '' : 'disabled'}>Forget all accounts</button></div>
        </section>
      </div>`;
  }

  // ----- plugins -----------------------------------------------------------------------------

  // Plugins for the client, from plugin sources, as the desktop launcher has them: Installed,
  // Available and Sources, each folding, and rows that open in place. Plugins the player adds
  // by hand (a folder or a zip, through the browser's pickers) are kept in this browser and listed
  // under Installed.

  const PLUGIN_UPDATES = [['none', 'None'], ['all', 'All'], ['first_party', 'Only Dereth']];
  const pluginUses = (permissions, domains) => [...permissions, ...domains].join(', ');
  // A plugin whose zip only the desktop launcher can fetch.
  const DESKTOP_ONLY = '<span class="muted small" title="It downloads from a GitHub release, which web pages can\'t fetch.">Desktop launcher only</span>';

  function pluginSection(key, label, count, body) {
    const open = !ui.plugin.folded[key];
    return `<h2 class="fold-head" data-plugin-fold="${key}"><span class="fold">${open ? '▾' : '▸'}</span>${label}<span class="muted small">${count}</span></h2>${open ? body : ''}`;
  }

  function pluginRow(key, p, buttons, extra, detail) {
    const open = ui.plugin.open === key;
    const row = `<div class="row plugin-row" data-plugin-row="${esc(key)}"><div class="grow">
        <div class="name">${esc(p.name)} ${esc(p.version)} <span class="muted small">${esc(p.source_name)}</span></div>
        ${p.description && !open ? `<div class="sub">${esc(p.description.split('\n')[0])}</div>` : ''}${extra}</div>
        ${buttons ? `<div class="btns">${buttons}</div>` : ''}</div>`;
    return open ? `<div class="plugin open">${row}<div class="plugin-detail" data-stop>${detail}</div></div>` : row;
  }

  function pluginDetail(p) {
    const facts = [['ID', p.id], ['VERSION', p.version], ['SOURCE', p.source_name], ['AUTHORS', (p.authors ?? []).join(', ')],
      ['RELEASED', p.released ? longDate(p.released) : ''], ['SIZE', p.size != null ? kb(p.size) : '']].filter(([, x]) => x);
    return `<p class="desc ${p.description ? '' : 'muted'}">${esc(p.description || 'No description.')}</p>
      <div class="strip">${facts.map(([k, x]) => `<span><b>${k}</b>${esc(x)}</span>`).join('')}</div>
      <div class="said"><b>USES</b>${esc(pluginUses(p.permissions, p.domains) || 'no permissions')}</div>
      ${p.notes ? `<div class="said"><b>WHAT CHANGED</b>${esc(p.notes)}</div>` : ''}
      ${p.homepage ? `<div class="btns"><button class="btn" data-url="${esc(p.homepage)}">Homepage</button></div>` : ''}`;
  }

  function updateLine(u) {
    if (u.needs_newer_dereth) return `<div class="sub warn">${esc(u.to)} needs a newer Dereth.</div>`;
    const asks = [...u.new_permissions, ...u.new_domains];
    return `<div class="sub">Update ${esc(u.to)} is ready.${asks.length ? ` <span class="warn">It also asks for ${esc(asks.join(', '))}.</span>` : ''}</div>`;
  }

  function installedRow(x, busy) {
    const u = x.update;
    const update = u && !u.needs_newer_dereth
      ? (x.desktop_only ? DESKTOP_ONLY : `<button class="btn" data-plugin-update="${esc(x.id)}" ${busy ? 'disabled' : ''}>Update</button>`)
      : '';
    const buttons = `${update}<button class="btn" data-plugin-remove="${esc(x.id)}">Remove</button>`;
    let detail = pluginDetail(x);
    if (x.source_removed) detail += '<p class="small warn">You removed its source, so it gets no updates.</p>';
    if (u) {
      const facts = [['UPDATE', u.to], ['RELEASED', u.released ? longDate(u.released) : ''], ['SIZE', u.size != null ? kb(u.size) : '']].filter(([, v]) => v);
      const asks = [...u.new_permissions, ...u.new_domains];
      detail += `<div class="plugin-update"><div class="strip">${facts.map(([k, v]) => `<span><b>${k}</b>${esc(v)}</span>`).join('')}</div>
        ${asks.length ? `<div class="said warn"><b>ALSO ASKS FOR</b>${esc(asks.join(', '))}</div>` : ''}
        ${u.notes ? `<div class="said"><b>WHAT CHANGED</b>${esc(u.notes)}</div>` : ''}</div>`;
    }
    return pluginRow(`i:${x.id}`, x, buttons, u ? updateLine(u) : '', detail);
  }

  // A plugin the player added by hand: its folder's or zip's name, marked Added, with Remove.
  function addedRow(a) {
    return `<div class="row static plugin-row"><div class="grow"><div class="name">${esc(a.name)} <span class="muted small">Added</span></div>
        <div class="sub">${a.folder ? 'A plugin folder' : 'A plugin zip'}, kept in this browser.</div></div>
        <div class="btns"><button class="btn" data-added-remove="${esc(a.name)}">Remove</button></div></div>`;
  }

  function availableRow(a, busy) {
    const button = a.state === 'install'
      ? (a.desktop_only ? DESKTOP_ONLY : `<button class="btn" data-plugin-install="${esc(a.source)}" data-plugin-id="${esc(a.id)}" ${busy ? 'disabled' : ''}>Install</button>`)
      : a.state === 'needs_newer_dereth' ? '<span class="muted small">Needs a newer Dereth</span>'
      : '<span class="muted small">Only from Dereth</span>';
    return pluginRow(`a:${a.source} ${a.id}`, a, button, '', pluginDetail(a));
  }

  function plugins() {
    const p = ui.plugin;
    const v = ui.snap.plugins;
    const added = ui.snap.web.kept.added ?? [];
    const busy = !!p.busy;
    const installed = v.installed.map((x) => installedRow(x, busy)).join('') + added.map(addedRow).join('')
      || '<p class="muted empty">No plugins installed. Install one from the list below.</p>';
    const addLine = `<div class="plugin-line add"><button class="btn" data-act="plugin-folder-add" ${busy ? 'disabled' : ''}>Add Plugin Folder</button>
        <button class="btn" data-act="plugin-zip-add" ${busy ? 'disabled' : ''}>Add Plugin Zip</button>
        ${v.installed.length + added.length ? `<button class="btn" data-act="plugins-remove-all" ${busy ? 'disabled' : ''}>Remove all</button>` : ''}</div>`;
    const updateOpts = PLUGIN_UPDATES.map(([val, l]) => `<option value="${val}" ${v.auto_update === val ? 'selected' : ''}>${l}</option>`).join('');
    const setting = `<div class="row static plugin-setting"><div class="grow"><div class="name">Automatically update plugins</div>
        <div class="sub">Plugin updates that ask for additional permissions will never automatically install.</div></div>
        <div class="btns"><select id="plugin-updates" class="field mini" aria-label="Automatically update plugins">${updateOpts}</select></div></div>`;
    const q = p.search.trim().toLowerCase();
    const shown = v.available.filter((a) => !q || [a.name, a.id, a.description, a.source_name].some((t) => String(t ?? '').toLowerCase().includes(q)));
    const listed = v.sources.some((s) => s.plugins > 0);
    const available = shown.map((a) => availableRow(a, busy)).join('')
      || `<p class="muted empty">${q ? 'No plugin matches.' : listed ? 'Every plugin the sources list is installed.' : 'No plugins listed yet. Refresh to read the sources.'}</p>`;
    const search = `<div class="plugin-line"><input id="plugin-search" class="field mini-field" type="text" placeholder="Search" spellcheck="false" autocomplete="off" value="${esc(p.search)}"></div>`;
    const sources = v.sources.map((s) => {
      const skipped = s.skipped?.length ? ` · <span class="warn" title="${esc(s.skipped.join('\n'))}">${s.skipped.length} could not be read</span>` : '';
      const problem = s.problem ?? s.error;
      return `<div class="row static source-row"><div class="grow">
          <div class="line"><span class="name">${esc(s.name)}</span><span class="muted small">${s.plugins} plugin${s.plugins === 1 ? '' : 's'}${skipped}</span><span class="muted small addr" title="${esc(s.url)}">${esc(s.url)}</span></div>
          ${problem ? `<div class="sub warn" title="${esc(problem)}">${esc(problem)}</div>` : ''}</div>
          ${s.first_party ? '' : `<div class="btns"><button class="btn" data-plugin-source-remove="${esc(s.url)}">Remove</button></div>`}</div>`;
    }).join('');
    const add = `<div class="plugin-line add"><input id="plugin-source" class="field mini-field" type="text" placeholder="https://…/plugins.json" spellcheck="false" autocomplete="off" aria-label="A source's address" value="${esc(p.add)}">
        <button class="btn" data-act="plugin-source-add" ${busy || !p.add.trim() ? 'disabled' : ''}>Add source</button></div>
      ${p.confirm ? `<div class="plugin-confirm"><span>Dereth does not review plugins from other sources.</span>
        <button class="btn mini-btn" data-act="plugin-source-confirm" ${busy ? 'disabled' : ''}>Add</button><button class="btn mini-btn" data-act="plugin-source-cancel">Cancel</button></div>` : ''}`;
    return `<div class="column wide" style="height:100%">
        <section class="panel grow"><header><h1>PLUGINS</h1><span class="tag">EXPERIMENTAL</span><button class="btn" data-act="plugins-refresh" ${busy ? 'disabled' : ''}>Refresh</button></header>
          <div class="scroll plugins-page">
            ${pluginSection('installed', 'INSTALLED', v.installed.length + added.length, `<div class="list">${installed}</div>${addLine}${setting}`)}
            ${pluginSection('available', 'AVAILABLE', shown.length, `${search}<div class="list">${available}</div>`)}
            ${pluginSection('sources', 'SOURCES', v.sources.length, `<div class="list">${sources}</div>${add}`)}
          </div>
        </section>
      </div>`;
  }

  // One of the Plugins page's slow calls (it fetches), with the page busy, and how it went said.
  async function pluginFetch(cmd, args) {
    ui.plugin.busy = cmd;
    render();
    let ok = false;
    try {
      const said = await api(cmd, args);
      toast(String(said || (cmd === 'refresh_plugins' ? 'Nothing new.' : 'Done.')).replace(/\n/g, ' '));
      ok = true;
    } catch (e) {
      toast(String(e?.message ?? e), true);
    }
    ui.plugin.busy = null;
    lastSnap = '';
    await pull();
    return ok;
  }

  async function addPluginSource(confirmed) {
    const url = ui.plugin.add.trim();
    if (!url) return;
    if (!confirmed && url !== ui.snap.plugins.first_party) {
      ui.plugin.confirm = true;
      render();
      return;
    }
    ui.plugin.confirm = false;
    if (await pluginFetch('add_plugin_source', { url })) ui.plugin.add = '';
    render();
  }

  // ----- first run ---------------------------------------------------------------------------

  // No data files kept yet: the desktop's first start, asking for the data files, which the
  // browser's picker reads and keeps here.
  function firstRun() {
    const fr = ui.firstRun;
    let body;
    if (fr.step === 'guide') {
      body = `<header><h1>GETTING THE GAME FILES</h1></header>
        <p>The community guide explains how to install Asheron's Call and get the final data files.</p>
        <div class="btns" style="margin:10px 0 16px"><button class="btn" data-url="${GUIDE}">Open community guide</button></div>
        <div class="btns" style="justify-content:space-between"><span>Then come back and choose the files.</span><button class="btn" data-act="fr-choose">Choose files</button></div>
        <p class="muted" style="margin-top:16px">We never download or host these files. They stay in this browser.</p>
        <div class="btns"><button class="btn" data-act="fr-skip">Skip for now</button></div>`;
    } else if (fr.step === 'found') {
      const sets = ui.snap.state.dat_sets.filter((s) => fr.added?.includes(s.id));
      const lines = sets.map((s) => {
        const it = setIterations(s);
        if (s.kind === 'classic') return `<p><span class="good">CLASSIC</span> — portal ${it.portal ?? '-'}, cell ${it.cell ?? '-'}</p>`;
        const eor = isEndOfRetail(it);
        return `<p><span class="${eor ? 'good' : 'warn'}">${eor ? 'MODERN' : 'MODERN, OLDER'}</span> — portal ${it.portal ?? '-'}, cell ${it.cell ?? '-'}, local ${it.local ?? '-'}, highres ${it.highres ?? '-'}${eor ? '' : ' (from before the final patch)'}</p>`;
      }).join('') || '<p class="warn">No data files among these.</p>';
      const older = sets.some((s) => s.kind !== 'classic' && !isEndOfRetail(setIterations(s)));
      body = `<header><h1>DATA FILES</h1></header>${lines}
        <div class="btns" style="margin-top:10px"><button class="btn" data-act="fr-continue" ${sets.length ? '' : 'disabled'}>Continue</button>
          <button class="btn" data-act="fr-choose">Choose other files</button>
          ${older ? `<button class="btn" data-url="${GUIDE}">Open community guide</button>` : ''}</div>`;
    } else {
      body = `<header><h1>WELCOME TO DERETH</h1></header>
        <p style="font-size:21px">Do you already have Asheron's Call's data files?</p>
        <div class="btns" style="margin:12px 0"><button class="btn" data-act="fr-choose">Yes - choose the files</button><button class="btn" data-act="fr-guide">No - how do I get them?</button></div>
        <p class="muted">The Dereth client needs the game's data (.dat) files, which we cannot ship. They are read in this browser and never uploaded.</p>
        <div class="btns"><button class="btn" data-act="fr-skip">Skip for now</button></div>`;
    }
    return `<div class="column wide"><section class="panel">${body}</section></div>`;
  }

  // ----- the interface -----------------------------------------------------------------------

  const interfaceAsked = () => {
    const i = ui.snap?.interface;
    return !!i && !i.chosen && !ui.iface.dismissed;
  };

  // A picture of each interface: the screen in miniature on a 64 by 40 grid, as the desktop draws it.
  const px = (x, y, w, h, fill) => `<rect x="${x}" y="${y}" width="${w}" height="${h}" fill="${fill}"/>`;
  const PIC = {
    gold: '#b58a3c', bright: '#e9c77a', dark: '#6b4e22', ink: '#0c0a10', stone: '#211c29', glass: 'rgba(12,10,16,0.74)',
    bronze: '#3a2f22', parchment: '#7d6a48', line: '#a99f8b', health: '#d8584a', stamina: '#e3b344', mana: '#4f7fd0', friend: '#5fcf55',
  };
  function dial(cx, cy, face) {
    const rows = [[-5, 2], [-4, 4], [-3, 5], [-2, 5], [-1, 5], [0, 5], [1, 5], [2, 5], [3, 5], [4, 4], [5, 2]];
    const disc = (inset, fill) => rows.filter(([dy]) => Math.abs(dy) <= 5 - inset)
      .map(([dy, half]) => px(cx - Math.max(0, half - inset), cy + dy, Math.max(0, half - inset) * 2 + 1, 1, fill)).join('');
    return disc(0, PIC.gold) + disc(1, face) + px(cx, cy, 1, 1, PIC.bright);
  }
  const pane = (x, y, w, h, fill) => px(x, y, w, h, PIC.gold) + px(x + 1, y + 1, w - 2, h - 2, fill);
  const textLines = (x, y, widths, fill, last = fill) => widths.map((w, i) => px(x, y + i * 2, w, 1, i === widths.length - 1 ? last : fill)).join('');
  const WORLD = px(0, 0, 64, 14, '#2a3a52') + px(0, 14, 64, 10, '#38506a')
    + px(0, 19, 15, 5, '#2f4a3a') + px(4, 17, 8, 2, '#2f4a3a') + px(40, 20, 24, 4, '#2f4a3a') + px(47, 18, 10, 2, '#2f4a3a')
    + px(0, 24, 64, 8, '#3b5a34') + px(0, 32, 64, 8, '#2e4829') + px(31, 21, 2, 7, PIC.ink);
  const INTERFACE_PICTURES = {
    Horizon: px(22, 2, 20, 3, PIC.gold) + px(23, 3, 18, 1, PIC.health)
      + px(45, 2, 2, 2, PIC.mana) + px(48, 2, 2, 2, PIC.friend)
      + dial(57, 7, '#16222c')
      + [8, 6, 7].map((w, i) => px(2, 10 + i * 4, 10, 3, PIC.glass) + px(3, 11 + i * 4, w, 1, PIC.friend)).join('')
      + px(2, 27, 12, 11, PIC.glass) + px(2, 27, 12, 1, PIC.dark) + textLines(3, 30, [9, 6, 10, 5], PIC.line, PIC.bright)
      + px(42, 23, 3, 3, PIC.ink) + px(40, 21, 7, 1, PIC.stamina)
      + px(17, 29, 10, 1, PIC.health) + px(28, 29, 10, 1, PIC.mana) + px(39, 29, 10, 1, PIC.stamina)
      + [31, 34].map((y) => Array.from({ length: 12 }, (_, i) => px(15 + i * 3, y, 2, 2, PIC.gold)).join('')).join('')
      + px(15, 38, 35, 1, PIC.dark) + px(15, 38, 22, 1, PIC.bright)
      + px(53, 35, 9, 3, PIC.ink) + [54, 56, 58, 60].map((x) => px(x, 36, 1, 1, PIC.bright)).join(''),
    Modern: pane(1, 1, 15, 9, PIC.stone) + px(3, 3, 11, 1, PIC.health) + px(3, 5, 9, 1, PIC.stamina) + px(3, 7, 10, 1, PIC.mana)
      + dial(57, 7, '#1d4a25')
      + pane(1, 27, 26, 12, PIC.stone) + textLines(3, 29, [20, 14, 18, 11], PIC.line, PIC.bright)
      + pane(46, 14, 17, 19, PIC.stone)
      + [17, 20, 23, 26, 29].map((y) => [48, 51, 54, 57].map((x) => px(x, y, 2, 2, PIC.dark)).join('')).join('')
      + pane(40, 34, 23, 5, PIC.stone) + Array.from({ length: 7 }, (_, i) => px(42 + i * 3, 36, 2, 1, PIC.bright)).join(''),
    Classic: px(0, 0, 64, 2, PIC.ink) + px(1, 0, 12, 2, PIC.health) + px(14, 0, 12, 2, PIC.stamina) + px(27, 0, 12, 2, PIC.mana)
      + [55, 58, 61].map((x) => px(x, 0, 2, 2, PIC.gold)).join('')
      + px(40, 2, 24, 32, PIC.bronze) + dial(51, 7, '#1d4a25')
      + px(41, 14, 22, 19, PIC.parchment) + textLines(43, 16, [16, 12, 15, 9, 14, 11, 13, 8], PIC.ink)
      + px(0, 33, 40, 7, PIC.ink) + textLines(2, 34, [30, 22, 26], PIC.line, PIC.bright)
      + px(40, 34, 24, 6, PIC.bronze) + Array.from({ length: 7 }, (_, i) => px(42 + i * 3, 36, 2, 2, PIC.gold)).join(''),
  };
  const interfacePicture = (name) => `<svg class="pic" viewBox="0 0 64 40" shape-rendering="crispEdges" aria-hidden="true">${WORLD}${INTERFACE_PICTURES[name] ?? ''}</svg>`;

  function pickedInterface() {
    const i = ui.snap.interface;
    return ui.iface.pick ?? i?.chosen ?? i?.offers[0]?.name ?? null;
  }

  function interfacePage() {
    const i = ui.snap.interface;
    if (!i) return home();
    const pick = pickedInterface();
    const fromLibrary = ui.iface.fromLibrary;
    const cards = i.offers.map((o) => `<button class="iface ${o.name === pick ? 'on' : ''}" data-iface="${esc(o.name)}" role="radio" aria-checked="${o.name === pick}">
        ${interfacePicture(o.name)}<span class="iname">${esc(o.name.toUpperCase())}</span><span class="iline">${esc(o.line)}</span></button>`).join('');
    const noClassic = pick === 'Classic' && !ui.snap.state.dat_sets.some((d) => d.kind === 'classic');
    const back = fromLibrary ? `<button class="btn icon-btn back" data-act="iface-cancel" title="Back to the library">${CHEVRON_LEFT}</button>` : '';
    return `<div class="column wide"><section class="panel">
        <header>${back}<h1>${fromLibrary ? 'INTERFACE' : 'CHOOSE YOUR INTERFACE'}</h1></header>
        <p style="font-size:21px">Which interface would you like to play Dereth in?</p>
        <div class="ifaces" role="radiogroup" aria-label="Interface">${cards}</div>
        ${noClassic ? '<p class="small warn">There are no Classic data files yet: add portal.dat and cell.dat in the Library before playing in the classic interface.</p>' : ''}
        <p class="muted small">You can change it whenever you like in the game's options, or here in the Library.</p>
        <div class="btns end" style="margin-top:12px">${fromLibrary ? '<button class="btn" data-act="iface-cancel">Cancel</button>' : ''}
          <button class="btn" data-act="iface-save">${fromLibrary ? 'Save' : 'Continue'}</button></div>
      </section></div>`;
  }

  function interfaceRow() {
    const i = ui.snap.interface;
    if (!i) return '';
    return `<div class="row static"><div class="grow"><div class="name">Interface: ${esc(i.chosen ?? 'not chosen yet')}</div>
        <div class="sub">The interface the client starts in. Its own options change it too.</div></div>
        <div class="btns"><button class="btn" data-act="iface-change">Change</button></div></div>`;
  }

  async function saveInterface() {
    const name = pickedInterface();
    const fromLibrary = ui.iface.fromLibrary;
    try {
      await api('set_interface', { name });
    } catch (e) {
      toast(String(e?.message ?? e), true);
      if (fromLibrary) return;
      ui.iface.dismissed = true;
    }
    ui.iface = { ...ui.iface, pick: null, fromLibrary: false };
    ui.screen = fromLibrary ? 'library' : 'home';
    lastSnap = '';
    await pull();
  }

  // ----- the browser's pickers (web only) ----------------------------------------------------

  // The data-file picker, for a set to keep (named `purpose.name`), or for this visit only.
  let picking = null;
  function pickDats(purpose) {
    picking = purpose;
    $('files').value = '';
    $('files').click();
  }
  $('files').addEventListener('change', async () => {
    const files = [...$('files').files];
    const purpose = picking;
    picking = null;
    if (!files.length || !purpose) return;
    if (purpose.visit) {
      backend.pickForVisit(files);
      if (ui.world) ui.world.form.dat_set_id = '__visit';
      lastSnap = '';
      await pull();
      return;
    }
    const keeping = backend.storeFiles(purpose.name, files);
    lastSnap = '';
    await pull();
    try {
      const ids = await keeping;
      if (purpose.firstRun) {
        ui.firstRun = { ...ui.firstRun, step: 'found', added: ids };
        ui.screen = 'first-run';
      }
      if (ui.screen === 'world' && ui.world) await openWorld(ui.world.view.world.slug, null, formChoice() ?? undefined);
      else if (!ids.length) toast('No data files among these.', true);
    } catch (e) {
      toast(String(e?.message ?? e), true);
    }
    lastSnap = '';
    await pull();
  });

  // A plugin's folder (the one holding its plugin.toml), or plugins' zips, kept in this browser;
  // the client finds them when it starts.
  $('plugin-folder').addEventListener('change', async () => {
    const picked = [...$('plugin-folder').files];
    $('plugin-folder').value = '';
    const files = await Promise.all(picked.map(async (f) => ({ path: f.webkitRelativePath || f.name, bytes: new Uint8Array(await f.arrayBuffer()) })));
    if (!files.length) return;
    if (!files.some((f) => f.path.endsWith('/plugin.toml'))) {
      toast('That folder has no plugin.toml: pick the plugin\'s own folder.', true);
      return;
    }
    await backend.addPlugins(files);
    toast('Added. Turn it on in the game with /plugin.');
    lastSnap = '';
    await pull();
  });
  $('plugin-zip').addEventListener('change', async () => {
    const picked = [...$('plugin-zip').files].filter((f) => f.name.toLowerCase().endsWith('.zip'));
    $('plugin-zip').value = '';
    const files = await Promise.all(picked.map(async (f) => ({ path: f.name, bytes: new Uint8Array(await f.arrayBuffer()) })));
    if (!files.length) return;
    await backend.addPlugins(files);
    toast('Added. Turn it on in the game with /plugin.');
    lastSnap = '';
    await pull();
  });

  async function copyText(text) {
    try {
      await navigator.clipboard.writeText(text);
      toast('Copied.');
    } catch {
      toast('The browser would not copy it.', true);
    }
  }

  // ----- events ------------------------------------------------------------------------------

  const launchHost = $('launch');
  launchHost.addEventListener('click', async (ev) => {
    const t = ev.target.closest('[data-go],[data-act],[data-url],[data-home],[data-world],[data-filter],[data-unfav],[data-fav],[data-world-fav],[data-default-set],[data-delete-set],[data-forget-account],[data-section],[data-open-world],[data-trash],[data-iface],[data-stop],[data-plugin-install],[data-plugin-update],[data-plugin-remove],[data-plugin-source-remove],[data-plugin-row],[data-plugin-fold],[data-added-remove],[data-overlay-remove],[data-block],[data-copy-link]');
    if (!t) return;
    if (t.dataset.stop !== undefined && !ev.target.closest('[data-act],[data-url]')) return;
    const d = t.dataset;
    // A form's own submit button submits it (below).
    if (t.tagName === 'BUTTON' && t.type === 'submit' && t.form) return;
    if (d.unfav) { ev.stopPropagation(); await act('remove_favourite', { id: d.unfav }); return; }
    if (d.worldFav) {
      ev.stopPropagation();
      await act('set_world_favorite', { slug: d.worldFav, favorite: !worldBy(d.worldFav)?.favorite });
      return;
    }
    if (d.fav) {
      ev.stopPropagation();
      const e = homeEntries()[Number(d.fav)];
      if (e) await act('save_favourite', { favourite: { id: '', world_slug: e.world_slug, account: e.account, client: 'dereth', dat_set_id: e.dat_set_id ?? null, classic_set_id: e.classic_set_id ?? null } });
      return;
    }
    if (d.go) {
      ui.screen = d.go;
      if (d.go === 'home') ui.firstRun.dismissed = ui.firstRun.dismissed || ui.snap.state.dat_sets.length > 0;
      render();
      return;
    }
    if (d.url) { ev.preventDefault(); await act('open_url', { url: d.url }); return; }
    if (d.home) { ui.homeSel = Number(d.home); render(); return; }
    if (d.world) { ui.expanded = ui.expanded === d.world ? null : d.world; render(); return; }
    if (d.openWorld) { await openWorld(d.openWorld); return; }
    if (d.section) { ui.folded[d.section] = !ui.folded[d.section]; render(); return; }
    if (d.trash) { if (ui.expanded === d.trash) ui.expanded = null; await act('remove_custom_world', { slug: d.trash }); return; }
    if (d.filter) { ui.filter = ui.filter === d.filter ? null : d.filter; render(); return; }
    if (d.defaultSet) { await act('set_default_set', { id: d.defaultSet }); return; }
    if (d.deleteSet) { await act('delete_set', { id: d.deleteSet }); return; }
    if (d.pluginInstall) { await pluginFetch('install_plugin', { source: d.pluginInstall, id: d.pluginId }); return; }
    if (d.pluginUpdate) { await pluginFetch('update_plugin', { id: d.pluginUpdate }); return; }
    if (d.pluginRemove) { await act('remove_plugin', { id: d.pluginRemove }); await backend.readKept(); lastSnap = ''; await pull(); return; }
    if (d.pluginSourceRemove) { await act('remove_plugin_source', { url: d.pluginSourceRemove }); return; }
    if (d.pluginRow) { ui.plugin.open = ui.plugin.open === d.pluginRow ? null : d.pluginRow; render(); return; }
    if (d.pluginFold) { ui.plugin.folded[d.pluginFold] = !ui.plugin.folded[d.pluginFold]; render(); return; }
    if (d.addedRemove) { await backend.removeAddedPlugin(d.addedRemove); lastSnap = ''; await pull(); return; }
    if (d.forgetAccount) { const a = ui.snap.state.accounts[Number(d.forgetAccount)]; await act('forget_account', { slug: a.world_slug, username: a.username }); return; }
    if (d.iface) { ui.iface.pick = d.iface; render(); return; }
    if (d.copyLink) { await copyText(d.copyLink); return; }
    if (d.overlayRemove || d.block) {
      try {
        if (d.overlayRemove) await backend.removeOverlay(d.overlayRemove);
        if (d.block) await backend.setBlocked(d.block, d.on === 'true');
      } catch (e) {
        toast(String(e?.message ?? e), true);
      }
      if (ui.screen === 'world' && ui.world) await openWorld(ui.world.view.world.slug, null, formChoice() ?? undefined);
      lastSnap = '';
      await pull();
      return;
    }

    switch (d.act) {
      case 'dismiss': localToast = null; await act('dismiss_message'); break;
      case 'refresh': await act('refresh'); break;
      case 'home-play': {
        const e = homeEntries()[ui.homeSel];
        if (e) await play({ world_slug: e.world_slug, account: e.account, password: null, dat_set_id: e.dat_set_id ?? null, classic_set_id: e.classic_set_id ?? null });
        break;
      }
      case 'add-dats': pickDats({ name: 'files' }); break;
      case 'forget-all-accounts': await act('forget_all_accounts'); break;
      case 'plugins-refresh': await pluginFetch('refresh_plugins', {}); break;
      case 'plugin-source-add': await addPluginSource(false); break;
      case 'plugin-source-confirm': await addPluginSource(true); break;
      case 'plugin-source-cancel': ui.plugin.confirm = false; render(); break;
      case 'plugin-folder-add': $('plugin-folder').click(); break;
      case 'plugin-zip-add': $('plugin-zip').click(); break;
      case 'plugins-remove-all': await backend.removePlugins(); lastSnap = ''; await pull(); break;
      case 'server-open': ui.addServer = { name: '', address: '', emulator: 'unknown', ruleset: '', error: null }; render(); focusSoon('srv-address'); break;
      case 'server-cancel': ui.addServer = null; render(); break;
      case 'server-add': await addServer(); break;
      case 'server-remove': {
        await act('remove_custom_world', { slug: ui.world.view.world.slug });
        ui.world = null;
        ui.screen = 'worlds';
        render();
        break;
      }
      case 'fr-guide': ui.firstRun.step = 'guide'; render(); break;
      case 'fr-choose': pickDats({ name: 'files', firstRun: true }); break;
      case 'fr-skip': ui.firstRun.dismissed = true; ui.screen = 'home'; render(); break;
      case 'fr-continue': ui.firstRun = { step: 'ask', added: null, dismissed: true }; ui.screen = 'home'; render(); break;
      case 'iface-change': ui.iface = { ...ui.iface, pick: null, fromLibrary: true }; ui.screen = 'interface'; render(); break;
      case 'iface-cancel': ui.iface = { ...ui.iface, pick: null, fromLibrary: false }; ui.screen = 'library'; render(); break;
      case 'iface-save': await saveInterface(); break;
      default: break;
    }
  });

  // PLAY on a world's page submits its form, so the browser's password manager sees the account
  // and the password go.
  launchHost.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    if (ev.target.id !== 'world-form' || !ui.world) return;
    if (!ui.world.form.password && !ui.world.form.account.trim()) { focusSoon('acct-name'); return; }
    const c = formChoice();
    if (c && worldCanPlay()) await play(c);
  });

  async function addServer() {
    const a = ui.addServer;
    try {
      const slug = await api('add_custom_world', { server: { name: a.name, address: a.address, emulator: a.emulator, ruleset: a.ruleset } });
      ui.addServer = null;
      lastSnap = '';
      await pull();
      await openWorld(slug);
    } catch (e) {
      a.error = String(e?.message ?? e);
      render();
    }
  }

  launchHost.addEventListener('dblclick', async (ev) => {
    const row = ev.target.closest('[data-dbl-home]');
    const e = row && homeEntries()[Number(row.dataset.dblHome)];
    if (e) await openWorld(e.world_slug, null, { account: e.account, dat_set_id: e.dat_set_id, classic_set_id: e.classic_set_id });
  });

  launchHost.addEventListener('input', (ev) => {
    const t = ev.target;
    if (t.id === 'search') { ui.search = t.value; render(); return; }
    if (ui.addServer && t.id.startsWith('srv-')) { ui.addServer[t.id.slice(4)] = t.value; return; }
    if (t.id === 'plugin-source') {
      const was = !ui.plugin.add.trim();
      const confirming = ui.plugin.confirm;
      ui.plugin.add = t.value;
      ui.plugin.confirm = false;
      if (confirming || was !== !ui.plugin.add.trim()) render();
      return;
    }
    if (t.id === 'plugin-search') { ui.plugin.search = t.value; render(); return; }
    if (!ui.world) return;
    const f = ui.world.form;
    if (t.id === 'acct-name') {
      f.account = t.value;
      const login = $('login-name');
      if (login) login.value = loginName(f.account, (worldBy(ui.world.view.world.slug) ?? ui.world.view.world).name);
      syncWorld();
    }
    if (t.id === 'password') { f.password = t.value; syncWorld(); }
  });

  launchHost.addEventListener('change', async (ev) => {
    const t = ev.target;
    if (ui.addServer && (t.id === 'srv-ruleset' || t.id === 'srv-emulator')) { ui.addServer[t.id.slice(4)] = t.value; return; }
    if (t.id === 'plugin-updates') { await act('set_plugin_updates', { setting: t.value }); return; }
    if (t.id === 'render-scale') { backend.setScale(t.value); lastSnap = ''; await pull(); return; }
    if (!ui.world) return;
    const f = ui.world.form;
    if (t.id === 'dats') {
      if (t.value === '__visit') pickDats({ visit: true });
      else if (t.value === '__add') { pickDats({ name: ui.world.view.world.dats?.custom ? ui.world.view.world.slug : 'files' }); t.value = f.dat_set_id ?? ''; return; }
      if (t.value !== '__visit' || ui.snap.web.picked) f.dat_set_id = t.value || null;
      render();
    }
    if (t.id === 'classic') { f.classic_set_id = t.value || null; render(); }
  });

  launchHost.addEventListener('keydown', async (ev) => {
    if (ev.key === 'Escape') {
      if (ui.screen === 'world') { ui.screen = 'worlds'; render(); }
      if (ui.screen === 'interface' && ui.iface.fromLibrary) { ui.iface = { ...ui.iface, pick: null, fromLibrary: false }; ui.screen = 'library'; render(); }
    }
    if (ev.key === 'Enter' && ui.addServer && ev.target.id?.startsWith('srv-')) { ev.preventDefault(); await addServer(); return; }
    if (ev.key === 'Enter' && ev.target.id === 'plugin-source') { await addPluginSource(false); return; }
    if (ev.key === 'Enter' && ev.target.id === 'acct-name' && !ui.world?.form.password) { ev.preventDefault(); focusSoon('password'); }
  });

  // ----- the painting ------------------------------------------------------------------------

  // Dereth's paintings, turning over every ten seconds.
  async function vista() {
    let sets = {};
    try {
      sets = await (await fetch('assets/backgrounds.json')).json();
    } catch {
      return;
    }
    const host = $('vista');
    const imgs = (sets[ui.realm] ?? []).map((n) => {
      const i = document.createElement('img');
      i.src = `assets/background/${n}`;
      i.alt = '';
      host.appendChild(i);
      return i;
    });
    let at = 0;
    imgs[0]?.classList.add('on');
    setInterval(() => {
      if (imgs.length < 2 || ui.playing) return;
      imgs[at].classList.remove('on');
      at = (at + 1) % imgs.length;
      imgs[at].classList.add('on');
    }, 10000);
  }

  // ----- start -------------------------------------------------------------------------------

  // `#worlds`, `#worlds=<slug>` (that row open), `#world=<slug>`, `#accounts`, `#plugins`,
  // `#library`, `#first-run` and `#interface` open that screen directly.
  async function jump() {
    const [what, arg] = location.hash.slice(1).split('=');
    if (!what) return;
    if (what === 'world' && arg) { await openWorld(decodeURIComponent(arg)); return; }
    if (what === 'worlds' && arg) ui.expanded = decodeURIComponent(arg);
    if (what === 'first-run') { ui.firstRun.step = arg || 'ask'; ui.firstRun.dismissed = false; }
    if (what === 'interface') ui.iface.dismissed = false;
    if (what !== 'first-run' && what !== 'interface') ui.firstRun.dismissed = true;
    ui.screen = what === 'worlds' ? 'worlds' : what;
    lastSnap = '';
    render();
  }

  async function begin() {
    requestAnimationFrame(() => requestAnimationFrame(() => document.documentElement.classList.remove('still')));
    vista();
    let linked = null;
    try {
      linked = await backend.begin();
    } finally {
      await pull();
    }
    if (location.hash) await jump();
    if (linked) {
      ui.firstRun.dismissed = true;
      await openWorld(linked);
    }
    // Read every plugin source as the page opens.
    api('refresh_plugins').then((said) => {
      if (said) toast(String(said).replace(/\n/g, ' '));
      lastSnap = '';
      return pull();
    }, (e) => log(`plugins: ${e?.message ?? e}`));
    // The hub's word on each world, while the page shows its worlds and is in sight.
    setInterval(async () => {
      if (ui.playing || document.hidden) return;
      try {
        await backend.readHub();
      } catch (e) {
        // A client started while the hub was read refuses the rest; that is no error.
        if (!ui.playing) log(`the hub's status: ${e?.message ?? e}`);
      }
      await pull();
    }, 5000);
  }

  return {
    begin,
    // The client ended (its own Exit, or a lost connection) or could not start: the front page
    // again, as it was, with what is kept read again and what went wrong said.
    async back(error) {
      ui.playing = false;
      await backend.readKept().catch(() => {});
      if (error) backend.say(error, true);
      lastSnap = '';
      if (ui.screen === 'world' && ui.world) await openWorld(ui.world.view.world.slug, null, formChoice() ?? undefined);
      await pull();
    },
  };
}
