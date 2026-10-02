// The launcher's page.
//
// The page keeps only what is on screen and being typed: which screen, the world page's choices,
// the search box. Everything the launcher knows comes from the backend's snapshot, read once a
// second, and every action is one call to it. The rules (which clients there are, which data files a
// world may be given) are the backend's.

"use strict";

const api = window.__TAURI__
  ? (cmd, args) => window.__TAURI__.core.invoke(cmd, args)
  : window.demoBackend;

const GUIDE = "https://www.accpp.net/manual-installation";

// How long a notice stays before it goes by itself.
const NOTICE_MS = 7000;

const ui = {
  screen: "home",          // home | worlds | world | library | accounts | first-run
  snap: null,
  homeSel: 0,
  search: "",
  filter: null,            // the Worlds list's one rules filter: null | "pve" | "pvp"
  addServer: null,         // the Worlds list's "add a server" form, while it is open: { name, host, port, error }
  world: null,             // the world page: { view, form, notice }
  firstRun: { step: "ask", find: null, dismissed: false },
  addFind: null,           // { purpose: "retail" | "dats", find }: a folder read from Library, waiting to be confirmed
};

// ----- small helpers ---------------------------------------------------------------------------------

const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const worldBy = (slug) => ui.snap?.worlds.find((w) => w.slug === slug);
// The two clients: the Dereth client the launcher found beside itself, and the player's retail one.
const clientOf = (kind) => (kind === "retail" ? ui.snap?.state.retail : ui.snap?.dereth) ?? null;
const clients = () => [ui.snap?.dereth, ui.snap?.state.retail].filter(Boolean);
const setBy = (id) => ui.snap?.state.dat_sets.find((s) => s.id === id);
// The retail client is a Windows program. Elsewhere the page offers the Dereth client alone and
// never mentions retail.
const retailHere = () => ui.snap?.platform === "windows";
const ROLES = ["portal", "cell", "local", "highres"];

function iterLabel(it) {
  return ROLES.map((r) => it?.[r] ?? "-").join("/");
}
function setIterations(set) {
  const it = {};
  for (const f of set.files) it[f.role] = f.iterations ?? undefined;
  return it;
}
function isEndOfRetail(it) {
  return it.portal === 2072 && it.cell === 982 && it.local === 994 && (it.highres == null || it.highres === 497);
}
function longDate(iso) {
  const [y, m, d] = String(iso || "").split("-").map(Number);
  const months = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
  return y && m && d ? `${d} ${months[m - 1]} ${y}` : iso;
}
function installName(i) {
  if (!i) return "No client";
  const family = i.kind === "dereth" ? "Dereth client" : "acclient";
  if (i.version && i.build_date) return `${family} ${i.version} (${longDate(i.build_date)})`;
  if (i.version) return `${family} ${i.version}`;
  return i.kind === "dereth" ? family : `${family} (unknown build)`;
}
function setName(s) {
  if (!s) return "No data files";
  const it = setIterations(s);
  const o = s.origin;
  const what =
    o.kind === "world" ? `${worldBy(o.slug)?.name ?? o.slug} (private)`
    : o.kind === "custom" ? "Custom data files"
    : isEndOfRetail(it) ? "End-of-retail" : "Data files";
  return `${what}  ${iterLabel(it)}`;
}
function stateLabel(w) {
  return { online: "online", high: "busy", offline: "offline", starting: "starting", unknown: "status unknown" }[w.state] ?? w.state;
}
function bead(cls) {
  return `<i class="dot ${esc(cls)}"></i>`;
}
function clientShort(kind) {
  return kind === "retail" ? "Retail" : "Dereth";
}
function gb(n) {
  return `${(n / 1073741824).toFixed(1)} GB`;
}

// ----- data from the backend ------------------------------------------------------------------------

// Redrawn only when something changed: an open drop-down would otherwise close under the player
// once a second.
let lastSnap = "";
async function pull() {
  let snap;
  try {
    snap = await api("snapshot");
  } catch (e) {
    console.error(e);
    return;
  }
  const json = JSON.stringify(snap);
  ui.snap = snap;
  if (ui.screen === "home" && snap.state.dat_sets.length === 0 && !ui.firstRun.dismissed) {
    ui.screen = "first-run";
    lastSnap = "";
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
    toast(String(e), true);
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
    if (shownMessage === key) api("dismiss_message").then(pull, () => {});
  }, NOTICE_MS);
}

// ----- rendering -----------------------------------------------------------------------------------

// Re-rendering replaces the page, so the element being typed into is found again afterwards and the
// caret put back where it was.
function render() {
  if (!ui.snap) return;
  const active = document.activeElement;
  const keep = active && active.id ? { id: active.id, start: active.selectionStart, end: active.selectionEnd } : null;

  document.getElementById("rail").innerHTML = rail();
  // The footnote is redrawn only when it changes, so the button under the pointer stays put.
  const footnote = foot();
  const footHost = document.getElementById("foot");
  if (footHost.dataset.drawn !== footnote) {
    footHost.innerHTML = footnote;
    footHost.dataset.drawn = footnote;
  }
  // The notice is redrawn only when it changes, so it does not pop in again on every screen.
  const notice = statusArea();
  const host = document.getElementById("notice");
  if (host.dataset.drawn !== notice) {
    host.innerHTML = notice;
    host.dataset.drawn = notice;
  }
  expireMessage(ui.snap.message);
  const main = document.getElementById("main");
  const screens = { home, worlds, world: worldPage, library, accounts, "first-run": firstRun };
  main.innerHTML = (screens[ui.screen] ?? home)();

  if (keep) {
    const el = document.getElementById(keep.id);
    if (el) {
      el.focus();
      if (keep.start != null && el.setSelectionRange) {
        try { el.setSelectionRange(keep.start, keep.end); } catch (_) { /* not a text input */ }
      }
    }
  }
}

function rail() {
  const lit = { world: "worlds", "first-run": "home" }[ui.screen] ?? ui.screen;
  const item = (id, label) => `<button class="${lit === id ? "lit" : ""}" data-go="${id}">${label}</button>`;
  return [
    item("home", "HOME"),
    item("worlds", "WORLDS"),
    item("library", "LIBRARY"),
    item("accounts", "ACCOUNTS"),
  ].join("");
}

// A shortcut: a curved arrow leaving a box, drawn on the same pixel grid as the rest of the chrome.
const SHORTCUT_ICON = `<svg viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
  <rect x="0" y="3" width="6" height="1"/><rect x="0" y="3" width="1" height="9"/>
  <rect x="0" y="11" width="9" height="1"/><rect x="8" y="8" width="1" height="4"/>
  <rect x="3" y="7" width="1" height="3"/><rect x="4" y="6" width="1" height="1"/>
  <rect x="5" y="5" width="3" height="1"/><rect x="8" y="3" width="1" height="5"/>
  <rect x="9" y="4" width="1" height="3"/><rect x="10" y="5" width="1" height="1"/></svg>`;

// The footnote under the rail: the launcher's version, the website, and where the system has such a
// thing (Windows and Linux), a button that puts a shortcut to the launcher on the desktop.
function foot() {
  const s = ui.snap;
  const shortcut = s.shortcut
    ? `<span class="sep">|</span><button class="iconbtn" data-act="shortcut" title="Create desktop shortcut" aria-label="Create desktop shortcut">${SHORTCUT_ICON}</button>`
    : "";
  return `<span>Dereth ${esc(s.version)}</span><span class="sep">|</span><a href="#" data-url="https://dereth.network">dereth.network</a>${shortcut}`;
}

// The notice: a job's progress and the last message, floating over the top right corner rather than
// taking room in any screen, so changing screens never moves it and it never moves them.
function statusArea() {
  const s = ui.snap;
  const job = s.job;
  const msg = localToast ?? s.message;
  // A launcher update: its download, then the offer to restart, which stays until taken.
  const up = s.update ?? { kind: "none" };
  const updating = up.kind === "downloading" || up.kind === "ready";
  if (!job && !msg && !updating) return "";
  let html = `<div class="status panel">`;
  if (up.kind === "downloading") {
    const frac = up.total ? Math.min(1, up.done / up.total) : null;
    html += `<div class="bar ${frac == null ? "sweep" : ""}"><i style="width:${frac == null ? 33 : (frac * 100).toFixed(1)}%"></i></div>`;
    html += `<div class="line"><span>Downloading Dereth ${esc(up.version)}${frac == null ? "..." : `  ${Math.round(frac * 100)}%`}</span></div>`;
  } else if (up.kind === "ready") {
    html += `<div class="line"><span>Dereth ${esc(up.version)} is ready.</span><button class="btn" data-act="restart-update">Restart to update</button></div>`;
  }
  if (job) {
    const frac = job.total ? Math.min(1, job.done / job.total) : null;
    html += `<div class="bar ${frac == null ? "sweep" : ""}"><i style="width:${frac == null ? 33 : (frac * 100).toFixed(1)}%"></i></div>`;
    html += `<div class="line"><span>${esc(job.what)}${frac == null ? "..." : `  ${Math.round(frac * 100)}%`}</span></div>`;
  }
  if (msg) {
    html += `<div class="line ${msg.error ? "error" : ""}"><span>${esc(msg.text)}</span><button data-act="dismiss" title="Dismiss">✕</button></div>`;
  }
  return html + `</div>`;
}

// ----- Home ---------------------------------------------------------------------------------------

// Two combinations are the same when the world, the account, the client and (for the Dereth
// client) the data files are.
function sameCombo(a, b) {
  return a.world_slug === b.world_slug && (a.account ?? "").toLowerCase() === (b.account ?? "").toLowerCase()
    && a.client === b.client && (a.client !== "dereth" || (a.dat_set_id ?? null) === (b.dat_set_id ?? null));
}

// Every favourite, then the recent combinations that are not favourites.
function homeEntries() {
  const s = ui.snap.state;
  const favs = s.favourites.map((f) => ({ ...f, fav: f }));
  const recents = s.recent.filter((r) => !s.favourites.some((f) => sameCombo(f, r))).map((r) => ({ ...r, fav: null }));
  return [...favs, ...recents];
}

function home() {
  const entries = homeEntries();
  ui.homeSel = Math.min(ui.homeSel, Math.max(0, entries.length - 1));
  const sel = entries[ui.homeSel];
  const canPlay = !!(sel && sel.account && sel.client && clientOf(sel.client) && worldBy(sel.world_slug)?.endpoint);
  const running = ui.snap.running;

  let list;
  if (entries.length === 0) {
    list = `<p class="muted">Nothing here yet. Pick a world to play, and it will be here next time.</p>
      <div class="btns"><button class="btn" data-go="worlds">Browse worlds</button></div>`;
  } else {
    list = `<div class="list">` + entries.map((e, i) => {
      const w = worldBy(e.world_slug);
      const set = setBy(e.dat_set_id);
      const data = e.client === "retail" ? "its own dats" : !set ? "no data" : set.origin.kind === "world" ? "private dats" : set.origin.kind === "custom" ? "custom dats" : setName(set).split("  ")[0];
      const end = !w ? "not listed" : w.state === "offline" ? "offline" : `${w.players ?? "-"} online`;
      const pin = e.fav
        ? `<span class="star" title="Favourite">★</span><button class="btn icon-btn mini" data-unfav="${esc(e.fav.id)}" title="Remove from favourites">✕</button>`
        : `<button class="btn icon-btn mini" data-fav="${i}" title="Add to favourites">☆</button>`;
      return `<div class="row ${i === ui.homeSel ? "sel" : ""} ${e.fav ? "fav" : ""}" data-home="${i}" data-dbl-home="${i}">
          ${bead(w?.state ?? "unknown")}
          <div class="grow"><div class="name">${esc(w?.name ?? e.world_slug)}</div>
          <div class="sub">${esc(e.account || "no account yet")} · ${esc(clientShort(e.client))} · ${esc(data)}</div></div>
          <div class="end">${esc(end)}</div>
          <div class="pin">${pin}</div>
        </div>`;
    }).join("") + `</div>`;
  }
  const runningLine = running.length
    ? `<p class="small good">Running: ${running.map((r) => `${esc(worldBy(r.world_slug)?.name ?? r.world_slug)} / ${esc(r.account)}`).join(", ")}</p>`
    : "";
  return `<div class="column narrow">
      <section class="panel"><header><h1>PLAY AGAIN</h1></header>
        <div class="scroll">${list}</div>${runningLine}</section>
      <button class="play" data-act="home-play" ${canPlay ? "" : "disabled"}>PLAY</button>
    </div>`;
}

// ----- Worlds -------------------------------------------------------------------------------------

const FILTERS = [
  ["pve", "PvE"],
  ["pvp", "PvP"],
];

function visibleWorlds() {
  const q = ui.search.trim().toLowerCase();
  const favs = new Set(ui.snap.state.favourites.map((f) => f.world_slug));
  const rank = { online: 0, high: 0, starting: 1, unknown: 2, offline: 3 };
  return ui.snap.worlds
    .filter((w) => !q || w.name.toLowerCase().includes(q) || (w.description ?? "").toLowerCase().includes(q))
    .filter((w) => {
      const rs = (w.ruleset ?? "").toLowerCase();
      if (ui.filter === "pve") return rs.includes("pve");
      if (ui.filter === "pvp") return rs.includes("pvp") || rs.includes("pk");
      return true;
    })
    .sort((a, b) => (favs.has(b.slug) - favs.has(a.slug)) || (rank[a.state] - rank[b.state]) || ((b.players ?? 0) - (a.players ?? 0)));
}

function worlds() {
  const rows = visibleWorlds();
  const s = ui.snap;
  let body;
  if (rows.length === 0) {
    const msg = s.list_state === "loading" ? "Fetching the world list..."
      : s.list_state === "unavailable" ? `The world list is unavailable: ${s.list_error ?? ""}`
      : s.worlds.length === 0 ? "No worlds are listed." : "No world matches these filters.";
    body = `<p class="muted">${esc(msg)}</p>`;
  } else {
    body = `<table class="worlds"><thead><tr><th></th><th>WORLD</th><th>RULES</th><th>PLAYERS</th></tr></thead><tbody>` +
      rows.map((w) => {
        const players = w.state === "offline" ? "offline" : w.players != null ? `${w.players}` : "--";
        return `<tr data-world="${esc(w.slug)}" class="${w.state === "offline" ? "dim" : ""}">
          <td>${bead(w.state)}</td><td class="name">${esc(w.name)}</td>
          <td>${w.slug.startsWith("custom-") && ui.snap.state.custom_worlds.some((c) => c.slug === w.slug) ? `<span class="muted">your server</span>` : esc(w.ruleset ?? "")}</td><td>${esc(players)}</td></tr>`;
      }).join("") + `</tbody></table>`;
  }
  const stale = s.list_state === "unavailable" && s.worlds.length ? `<p class="small warn">${esc(s.list_error ?? "")} — showing the last list</p>` : "";
  const a = ui.addServer;
  const form = a
    ? `<section class="panel pending"><header><h1>ADD A SERVER</h1></header>
        <div class="form">
          <label class="lab" for="srv-name">NAME</label><input id="srv-name" class="field" placeholder="optional" value="${esc(a.name)}">
          <label class="lab" for="srv-host">HOST</label><input id="srv-host" class="field" placeholder="play.example.org or 203.0.113.7" value="${esc(a.host)}">
          <label class="lab" for="srv-port">PORT</label><input id="srv-port" class="field" inputmode="numeric" style="max-width:140px" value="${esc(a.port)}">
          ${a.error ? `<p class="note bad">${esc(a.error)}</p>` : ""}
        </div>
        <div class="btns end" style="margin-top:12px"><button class="btn" data-act="server-cancel">Cancel</button><button class="btn" data-act="server-add">Add</button></div>
      </section>`
    : "";
  return `<div class="column wide" style="height:100%">
      ${form}
      <section class="panel grow"><header><h1>WORLDS</h1><span class="muted small">${rows.length} worlds</span>
        <button class="btn" data-act="server-open" title="Add a server of your own">+ Add server</button>
        <button class="btn icon-btn" data-act="refresh" title="Refresh">↻</button></header>
        <div class="btns" style="margin-bottom:12px">
          <input id="search" class="field" style="max-width:360px" placeholder="search" value="${esc(ui.search)}">
          ${FILTERS.map(([k, l]) => `<button class="chip ${ui.filter === k ? "on" : ""}" data-filter="${k}">${l}</button>`).join("")}
        </div>
        ${stale}
        <div class="scroll">${body}</div>
      </section>
    </div>`;
}

// ----- one world ------------------------------------------------------------------------------------

async function openWorld(slug, notice, preset) {
  api("probe_world", { slug }).catch(() => {});
  let view;
  try {
    view = await api("world_view", { slug });
  } catch (e) {
    toast(String(e), true);
    return;
  }
  const p = view.prefs;
  const known = view.accounts.map((a) => a.username);
  const account = preset?.account ?? (known.find((a) => a.toLowerCase() === (p.account ?? "").toLowerCase()) || known[0] || "");
  const offered = (k) => view.clients.some((c) => c.kind === k);
  const validSet = (id) => view.dat_sets.some((s) => s.id === id);
  const client = [preset?.client, p.client, view.default_client].find((k) => k && offered(k)) ?? view.clients[0]?.kind ?? null;
  const fallback = view.dat_sets.find((s) => s.origin.kind === "shared") ?? view.dat_sets[0];
  const set = [preset?.dat_set_id, p.dat_set_id].find(validSet) ?? fallback?.id ?? null;
  const acct = view.accounts.find((a) => a.username === account);
  ui.world = {
    view,
    form: {
      account,
      adding: known.length === 0,
      password: preset?.password ?? "",
      remembered: false,
      remember: acct ? acct.remember : true,
      client,
      dat_set_id: set,
    },
    notice: notice ?? null,
  };
  ui.screen = "world";
  await recallPassword();
  render();
  focusSoon(ui.world.form.adding ? "acct-name" : "password");
}

function focusSoon(id) {
  setTimeout(() => document.getElementById(id)?.focus(), 0);
}

async function recallPassword() {
  const w = ui.world;
  if (!w || !w.form.account) return;
  w.form.remembered = await api("has_password", { slug: w.view.world.slug, username: w.form.account });
}

function formChoice() {
  const w = ui.world;
  const f = w.form;
  if (!f.account.trim() || !f.client) return null;
  if (!f.password && !f.remembered) return null;
  return {
    world_slug: w.view.world.slug,
    account: f.account.trim(),
    password: f.password || null,
    remember: f.remember,
    client: f.client,
    dat_set_id: f.client === "dereth" ? f.dat_set_id : null,
  };
}

// Whether PLAY can go, and what the account field warns about: redrawn in place while typing, so
// the field being typed in is never replaced under the player.
function worldCanPlay() {
  const w = ui.world;
  const world = worldBy(w.view.world.slug) ?? w.view.world;
  return !!formChoice() && !!world.endpoint;
}
function worldNotice() {
  const w = ui.world;
  const f = w.form;
  const world = worldBy(w.view.world.slug) ?? w.view.world;
  const creates = world.account_model === "auto_create_on_first_login" && f.account.trim() && !w.view.accounts.some((a) => a.username.toLowerCase() === f.account.trim().toLowerCase());
  return creates ? "New here? This world creates your account the first time you sign in, so check the spelling." : w.notice ?? "";
}
function syncWorld() {
  const play = document.querySelector('[data-act="world-play"]');
  if (play) play.disabled = !worldCanPlay();
  const note = document.getElementById("world-note");
  if (note) {
    const text = worldNotice();
    note.textContent = text;
    note.hidden = !text;
  }
}

// The era a world plays, for the world's facts; nothing for the end of retail, which is every
// world's unless it says otherwise.
function eraLabel(era) {
  if (!era || era === "eor") return null;
  const names = { infiltration: "Infiltration era (February 2005)" };
  return names[era] ?? `${era} era`;
}

function worldPage() {
  const w = ui.world;
  if (!w) return home();
  const world = worldBy(w.view.world.slug) ?? w.view.world;
  const f = w.form;

  const facts = [stateLabel(world), world.players != null ? `${world.players} players` : null, world.ruleset, eraLabel(world.era)].filter(Boolean);
  const links = [["Website", world.links.website], ["Discord", world.links.discord], ["Rules", world.links.rules]].filter(([, u]) => u);
  const custom = ui.snap.state.custom_worlds.some((c) => c.slug === world.slug);

  const accountField = f.adding
    ? `<div class="pair"><input id="acct-name" class="field" placeholder="account name" value="${esc(f.account)}">
        ${w.view.accounts.length ? `<button class="btn" data-act="known-accounts">Known accounts</button>` : ""}</div>`
    : `<div class="pair"><select id="account" class="field">${w.view.accounts.map((a) => `<option ${a.username === f.account ? "selected" : ""}>${esc(a.username)}</option>`).join("")}</select>
        <button class="btn" data-act="add-account">+ Add account</button></div>`;
  const note = worldNotice();

  // The client: the Dereth client or the retail one, as two buttons.
  const clientBtn = (kind) => {
    const c = w.view.clients.find((x) => x.kind === kind);
    const why = !c ? (kind === "retail" ? "No retail client yet: add one in the Library" : "The Dereth client was not found")
      : !c.runnable ? "Needs Windows" : installName(c.install);
    return `<button class="btn seg ${f.client === kind ? "on" : ""}" data-client="${kind}" ${c ? "" : "disabled"} title="${esc(why)}">${kind === "retail" ? "RETAIL" : "DERETH"}</button>`;
  };
  const chosen = w.view.clients.find((x) => x.kind === f.client);
  const clientRow = retailHere()
    ? null
    : `<div class="plain">${chosen ? esc(installName(chosen.install)) : `<span class="warn">The Dereth client was not found beside the launcher.</span>`}</div>`;
  const clientNote = !chosen ? "" : !chosen.runnable ? `<span class="warn small">needs Windows</span>` : `<span class="muted small">${esc(installName(chosen.install))}</span>`;
  const setOpts = (w.view.dat_sets.length ? "" : `<option value="" selected>No data files for this world yet</option>`) + w.view.dat_sets.map((s) => `<option value="${esc(s.id)}" ${s.id === f.dat_set_id ? "selected" : ""}>${esc(setName(s))}${s.origin.kind === "shared" ? "  (default)" : ""}</option>`).join("")
    + (w.view.offers_private_copy ? `<option value="__private">Create a private copy (1.4 GB)</option>` : "")
    + (world.dats.custom ? `<option value="__custom">Add this world's downloaded data files...</option>` : "");
  const dataRow = f.client === "retail"
    ? `<div class="plain">The data files beside the retail client <span class="muted small">${esc(ui.snap.state.retail?.path ?? "")}</span></div>`
    : `<select id="dats" class="field">${setOpts}</select>`;
  const remembers = ui.snap.vault_name === "memory only" ? "Remember until the launcher closes" : "Save password securely";

  return `<div class="column wide">
      <section class="panel"><header><h1>${esc(world.name.toUpperCase())}</h1>
          ${links.map(([l, u]) => `<button class="btn" data-url="${esc(u)}">${l}</button>`).join("")}
          ${custom ? `<button class="btn" data-act="server-remove">Remove server</button>` : ""}
          <button class="btn icon-btn" data-go="worlds" title="Back to worlds">✕</button></header>
        <div class="scroll">
          <div class="facts">${bead(world.state)}<span>${esc(facts.join("  ·  "))}</span></div>
          ${world.description ? `<p class="muted small">${esc(world.description)}</p>` : ""}
          <div class="form" style="margin-top:14px">
            <label class="lab" for="${f.adding ? "acct-name" : "account"}">ACCOUNT</label>${accountField}
            <label class="lab" for="password">PASSWORD</label>
            <div class="pair"><input id="password" class="field" type="password" autocomplete="off"
                placeholder="${f.remembered && !f.password ? "saved" : ""}" value="${esc(f.password)}">
              <label class="check"><input type="checkbox" id="remember" ${f.remember ? "checked" : ""}>${esc(remembers)}</label></div>
            <p class="note warn" id="world-note" ${note ? "" : "hidden"}>${esc(note)}</p>
            <label class="lab">CLIENT</label>${clientRow ?? `<div class="pair">${clientBtn("dereth")}${clientBtn("retail")}${clientNote}</div>`}
            <label class="lab" for="dats">DATA</label>${dataRow}
          </div>
        </div>
      </section>
      <button class="play" data-act="world-play" ${worldCanPlay() ? "" : "disabled"}>PLAY</button>
    </div>`;
}

async function play(choice) {
  const out = await act("launch", { choice });
  if (!out) return;
  if (out.kind === "launched") {
    toast(`Launching ${out.world} | ${out.account}...`, false);
    ui.screen = "home";
    const at = homeEntries().findIndex((e) => sameCombo(e, choice));
    ui.homeSel = at < 0 ? 0 : at;
    render();
  } else if (out.kind === "need_password") {
    await openWorld(choice.world_slug, out.message, { ...choice, password: "" });
  } else {
    toast(out.message, true);
  }
}

// ----- the library ---------------------------------------------------------------------------------

function library() {
  const s = ui.snap.state;
  const r = s.retail;
  let retail;
  if (r) {
    const tags = [r.modifications.length ? "modified" : "stock", r.multi_instance ? "multi-client" : "one at a time", r.manifest_result && `${r.manifest_result.matched} of ${r.manifest_result.total} files match`].filter(Boolean);
    const warn = r.client_id === "unknown" ? "not a build we know; it may still work" : !r.net_version ? "logon version unknown; no listed world may accept it" : ui.snap.platform !== "windows" ? "needs Windows to run" : "";
    retail = `<div class="row static"><div class="grow"><div class="name">${esc(installName(r))} <span class="muted small">${esc(tags.join("; "))}</span></div>
        <div class="sub">${esc(r.path)}${warn ? ` <span class="warn">— ${esc(warn)}</span>` : ""}</div></div>
        <div class="btns"><button class="btn" data-act="verify-retail">Verify</button><button class="btn" data-act="choose-retail">Change</button><button class="btn" data-act="forget-retail">Remove</button></div></div>`;
  } else {
    retail = `<div class="row static"><div class="grow"><div class="sub">No retail client. Point the launcher at the folder that holds acclient.exe; it plays with the data files beside it.</div></div>
        <div class="btns"><button class="btn" data-act="choose-retail">Choose folder</button></div></div>`;
  }
  const d = ui.snap.dereth;
  const dereth = d
    ? `<div class="row static"><div class="grow"><div class="name">${esc(installName(d))}</div><div class="sub">${esc(d.path)}</div></div></div>`
    : `<div class="row static"><div class="grow"><div class="sub warn">The Dereth client was not found beside the launcher.</div></div></div>`;
  const sets = s.dat_sets.map((d) => {
    const size = d.files.reduce((n, f) => n + f.size, 0);
    const facts = [d.created_by_launcher ? gb(size) : d.files.every((f) => f.read_only) ? "read-only" : null, d.last_patched_by_server && "patched by its world", d.files.some((f) => f.error) && "some files unreadable"].filter(Boolean);
    const isDefault = d.origin.kind === "shared";
    const defaultBtn = isDefault ? `<span class="tag">DEFAULT</span>`
      : d.origin.kind === "unassigned" ? `<button class="btn" data-default-set="${esc(d.id)}">Set default</button>` : "";
    return `<div class="row static"><div class="grow"><div class="name">${esc(setName(d))} <span class="muted small">${esc(facts.join("; "))}</span></div>
        <div class="sub">${esc(d.path)}</div></div>
        <div class="btns">${defaultBtn}${d.origin.kind === "world" ? `<button class="btn" data-reset-set="${esc(d.id)}">Reset</button>` : ""}<button class="btn" data-delete-set="${esc(d.id)}">${d.created_by_launcher ? "Delete" : "Remove"}</button></div></div>`;
  }).join("") || `<p class="muted">No data files yet. Add a folder that holds the four .dat files.</p>`;

  let pending = "";
  if (ui.addFind) {
    const { purpose, find: f } = ui.addFind;
    const ok = purpose === "retail" ? !!f.retail : !!f.dats;
    pending = `<section class="panel pending"><header><h1>${purpose === "retail" ? "RETAIL CLIENT" : "DATA FILES"}</h1></header>
      <p class="muted small">${esc(f.folder)}</p>${findSummary(f, purpose)}
      <div class="btns end" style="margin-top:10px">
        <button class="btn" data-act="add-cancel">Cancel</button><button class="btn" data-act="add-confirm" ${ok ? "" : "disabled"}>Add</button></div></section>`;
  }
  return `<div class="column wide" style="height:100%">
      ${pending}
      <section class="panel grow"><header><h1>LIBRARY</h1><button class="btn" data-act="add-dats">+ Add data files</button></header>
        <div class="scroll">${retailHere() ? `<h2>RETAIL CLIENT</h2><div class="list">${retail}</div>` : ""}
          <h2>DERETH CLIENT</h2><div class="list">${dereth}</div>
          <h2>DATA SETS</h2><p class="muted small">The Dereth client plays with any of these; you choose one per world, and new worlds start on the default.</p><div class="list">${sets}</div></div>
      </section>
    </div>`;
}

// What a folder holds. `purpose` says what it was chosen for: the retail client, or data files (in
// which case any client in it is not taken).
function findSummary(f, purpose) {
  if (!retailHere()) purpose = "dats";
  const lines = [];
  if (f.error) lines.push(`<p class="bad">${esc(f.error)}</p>`);
  if (purpose !== "dats") {
    if (f.retail) {
      const known = f.retail.client_id !== "unknown";
      lines.push(`<p><span class="${known ? "good" : "warn"}">${known ? "IDENTIFIED" : "UNKNOWN BUILD"}</span> — ${esc(installName(f.retail))}${f.retail.modifications.length ? ` <span class="muted">(modified: ${esc(f.retail.modifications.join(", "))})</span>` : ""}</p>`);
    } else {
      lines.push(`<p class="warn">No acclient.exe in this folder.</p>`);
    }
  }
  if (f.dats) {
    const it = setIterations(f.dats);
    lines.push(`<p><span class="${isEndOfRetail(it) ? "good" : "warn"}">${isEndOfRetail(it) ? "MATCHES" : "OLDER"}</span> — portal ${it.portal ?? "-"}, cell ${it.cell ?? "-"}, local ${it.local ?? "-"}, highres ${it.highres ?? "-"}${isEndOfRetail(it) ? " = end of retail" : " (from before the final patch)"}</p>`);
  } else {
    lines.push(`<p class="warn">No data files in this folder.</p>`);
  }
  return lines.join("");
}

// ----- accounts ------------------------------------------------------------------------------------

function accounts() {
  const s = ui.snap.state;
  const rows = s.accounts.map((a, i) => {
    return `<div class="row static"><div class="grow"><div class="name">${esc(worldBy(a.world_slug)?.name ?? a.world_slug)} · ${esc(a.username)}</div></div>
        <button class="btn icon-btn" data-forget-account="${i}" title="Forget this account">✕</button></div>`;
  }).join("") || `<p class="muted">No accounts yet. They are added the first time you sign in to a world.</p>`;
  return `<div class="column wide" style="height:100%">
      <section class="panel grow"><header><h1>ACCOUNTS</h1></header>
        <div class="scroll"><div class="list">${rows}</div></div>
        <div class="btns end" style="margin-top:12px"><button class="btn" data-act="forget-all-accounts" ${s.accounts.length ? "" : "disabled"}>Forget all accounts</button></div>
      </section>
    </div>`;
}

// ----- first run -----------------------------------------------------------------------------------

function firstRun() {
  const fr = ui.firstRun;
  let body;
  if (fr.step === "guide") {
    body = `<header><h1>GETTING THE GAME FILES</h1></header>
      <p>The community guide explains how to install Asheron's Call and get the final data files.</p>
      <div class="btns" style="margin:10px 0 16px"><button class="btn" data-url="${GUIDE}">Open community guide</button></div>
      <div class="btns" style="justify-content:space-between"><span>Then come back and point us at the folder.</span><button class="btn" data-act="fr-choose">Choose folder</button></div>
      <p class="muted" style="margin-top:16px">We never download or host these files. We only check them.</p>
      <div class="btns"><button class="btn" data-act="fr-skip">Skip for now</button></div>`;
  } else if (fr.step === "found") {
    const f = fr.find;
    body = `<header><h1>${esc(f.folder)}</h1></header>${findSummary(f, "both")}
      <div class="btns" style="margin-top:10px"><button class="btn" data-act="fr-continue" ${f.retail || f.dats ? "" : "disabled"}>Continue</button>
        <button class="btn" data-act="fr-choose">Choose another folder</button>
        ${f.dats && !isEndOfRetail(setIterations(f.dats)) ? `<button class="btn" data-url="${GUIDE}">Open community guide</button>` : ""}</div>`;
  } else {
    body = `<header><h1>WELCOME TO DERETH</h1></header>
      <p style="font-size:21px">${retailHere() ? "Do you already have Asheron's Call installed?" : "Do you already have Asheron's Call's data files?"}</p>
      <div class="btns" style="margin:12px 0"><button class="btn" data-act="fr-choose">Yes - show me the folder</button><button class="btn" data-act="fr-guide">No - how do I get it?</button></div>
      <p class="muted">The Dereth client needs the game's data (.dat) files, which we cannot ship. Either way, we will check what you have.</p>
      <div class="btns"><button class="btn" data-act="fr-skip">Skip for now</button></div>`;
  }
  return `<div class="column wide"><section class="panel">${body}</section></div>`;
}

// ----- events -------------------------------------------------------------------------------------

document.addEventListener("click", async (ev) => {
  const t = ev.target.closest("[data-go],[data-act],[data-url],[data-home],[data-world],[data-filter],[data-unfav],[data-fav],[data-client],[data-default-set],[data-reset-set],[data-delete-set],[data-remember],[data-forget-account],[data-stop]");
  if (!t) return;
  if (t.dataset.stop !== undefined && !ev.target.closest("[data-act]")) return;
  const d = t.dataset;
  if (d.unfav) { ev.stopPropagation(); await act("remove_favourite", { id: d.unfav }); return; }
  if (d.fav) {
    ev.stopPropagation();
    const e = homeEntries()[Number(d.fav)];
    if (e) await act("save_favourite", { favourite: { id: "", world_slug: e.world_slug, account: e.account, client: e.client, dat_set_id: e.client === "dereth" ? e.dat_set_id ?? null : null } });
    return;
  }
  if (d.go) { ui.screen = d.go; if (d.go === "home") ui.firstRun.dismissed = ui.firstRun.dismissed || ui.snap.state.dat_sets.length > 0; render(); return; }
  if (d.url) { ev.preventDefault(); await act("open_url", { url: d.url }); return; }
  if (d.home) { ui.homeSel = Number(d.home); render(); return; }
  if (d.world) { await openWorld(d.world); return; }
  if (d.filter) { ui.filter = ui.filter === d.filter ? null : d.filter; render(); return; }
  if (d.client && ui.world) { ui.world.form.client = d.client; render(); return; }
  if (d.defaultSet) { await act("set_default_set", { id: d.defaultSet }); return; }
  if (d.resetSet) { await act("reset_set", { id: d.resetSet }); return; }
  if (d.deleteSet) { await act("delete_set", { id: d.deleteSet }); return; }
  if (d.forgetAccount) { const a = ui.snap.state.accounts[Number(d.forgetAccount)]; await act("forget_account", { slug: a.world_slug, username: a.username }); return; }

  switch (d.act) {
    case "dismiss": localToast = null; await act("dismiss_message"); break;
    case "refresh": await act("refresh"); break;
    case "home-play": {
      const e = homeEntries()[ui.homeSel];
      if (e) await play({ world_slug: e.world_slug, account: e.account, password: null, remember: true, client: e.client, dat_set_id: e.client === "dereth" ? e.dat_set_id ?? null : null });
      break;
    }
    case "world-play": { const c = formChoice(); if (c) await play(c); break; }
    case "add-account": ui.world.form = { ...ui.world.form, adding: true, account: "", password: "", remembered: false }; render(); focusSoon("acct-name"); break;
    case "known-accounts": { const a = ui.world.view.accounts[0]; ui.world.form = { ...ui.world.form, adding: false, account: a.username, password: "", remember: a.remember }; await recallPassword(); render(); break; }
    case "choose-retail": { const f = await act("pick_folder", { title: "Choose the folder that holds acclient.exe" }); if (f) { ui.addFind = { purpose: "retail", find: f }; render(); } break; }
    case "add-dats": { const f = await act("pick_folder", { title: "Choose a folder that holds the four .dat files" }); if (f) { ui.addFind = { purpose: "dats", find: f }; render(); } break; }
    case "add-cancel": ui.addFind = null; render(); break;
    case "add-confirm": {
      const { purpose, find } = ui.addFind;
      ui.addFind = null;
      // Data files chosen as data files never make their folder's client the retail one.
      await act("add_folder", { find: purpose === "dats" ? { ...find, retail: null } : find });
      break;
    }
    case "verify-retail": await act("verify_retail"); break;
    case "forget-retail": await act("forget_retail"); break;
    case "forget-all-accounts": await act("forget_all_accounts"); break;
    case "restart-update": await act("restart_to_update"); break;
    case "shortcut": { const where = await act("create_desktop_shortcut"); if (where) toast("Desktop shortcut created."); break; }
    case "server-open": ui.addServer = { name: "", host: "", port: "9000", error: null }; render(); focusSoon("srv-host"); break;
    case "server-cancel": ui.addServer = null; render(); break;
    case "server-add": await addServer(); break;
    case "server-remove": {
      const slug = ui.world.view.world.slug;
      await act("remove_custom_world", { slug });
      ui.world = null;
      ui.screen = "worlds";
      render();
      break;
    }
    case "fr-guide": ui.firstRun.step = "guide"; render(); break;
    case "fr-skip": ui.firstRun.dismissed = true; ui.screen = "home"; render(); break;
    case "fr-choose": { const f = await act("pick_folder", { title: retailHere() ? "Choose your Asheron's Call folder" : "Choose the folder that holds the four .dat files" }); if (f) { ui.firstRun.find = f; ui.firstRun.step = "found"; render(); } break; }
    case "fr-continue": { const f = ui.firstRun.find; ui.firstRun = { ...ui.firstRun, step: "ask", find: null, dismissed: true }; ui.screen = "home"; await act("add_folder", { find: f }); break; }
  }
});

// Add the server in the form, and open its page.
async function addServer() {
  const a = ui.addServer;
  try {
    const slug = await api("add_custom_world", { name: a.name, host: a.host, port: a.port });
    ui.addServer = null;
    await pull();
    await openWorld(slug);
  } catch (e) {
    a.error = String(e);
    render();
  }
}

document.addEventListener("dblclick", async (ev) => {
  const row = ev.target.closest("[data-dbl-home]");
  const e = row && homeEntries()[Number(row.dataset.dblHome)];
  if (e) await openWorld(e.world_slug, null, { account: e.account, client: e.client, dat_set_id: e.dat_set_id });
});

document.addEventListener("input", (ev) => {
  const t = ev.target;
  if (t.id === "search") { ui.search = t.value; render(); return; }
  if (ui.addServer && t.id.startsWith("srv-")) { ui.addServer[t.id.slice(4)] = t.value; return; }
  if (!ui.world) return;
  const f = ui.world.form;
  if (t.id === "acct-name") { f.account = t.value; f.remembered = false; syncWorld(); }
  if (t.id === "password") { f.password = t.value; syncWorld(); }
});

document.addEventListener("change", async (ev) => {
  const t = ev.target;
  if (!ui.world) return;
  const f = ui.world.form;
  switch (t.id) {
    case "account": {
      const a = ui.world.view.accounts.find((x) => x.username === t.value);
      Object.assign(f, { account: t.value, password: "", remembered: false, remember: a?.remember ?? true });
      await recallPassword();
      break;
    }
    case "remember": f.remember = t.checked; return;
    default: return;
    case "dats":
      if (t.value === "__private") { await act("create_private_copy", { slug: ui.world.view.world.slug }); ui.world.notice = "Copying data files for this world. PLAY when it is done."; }
      else if (t.value === "__custom") { if (await act("add_custom_dats", { slug: ui.world.view.world.slug })) await openWorld(ui.world.view.world.slug, null, formChoice() ?? undefined); return; }
      else f.dat_set_id = t.value || null;
      break;
  }
  render();
});

// The web view's own right-click menu (Back, Reload, Inspect) is not the launcher's, so a release
// build swallows every right-click outside a text box. A debug build, whose snapshot says
// `devtools`, keeps it everywhere for Inspect, and so does the demo page in an ordinary browser.
document.addEventListener("contextmenu", (ev) => {
  // A text box keeps its own menu (cut, copy, paste); without devtools it offers no Inspect.
  const editable = ev.target.closest?.("input, textarea, [contenteditable='true']");
  if (window.__TAURI__ && !ui.snap?.devtools && !editable) ev.preventDefault();
});

// The web view's own page shortcuts do nothing in a release build either: reloading would throw
// away the launcher's state for nothing, and there is nothing to print or search. F5, Ctrl+R
// (with or without Shift), Ctrl+P and Ctrl+F; on macOS the same with Cmd.
document.addEventListener("keydown", (ev) => {
  if (!window.__TAURI__ || ui.snap?.devtools) return;
  const mod = ev.ctrlKey || ev.metaKey;
  const key = ev.key.toLowerCase();
  if (ev.key === "F5" || (mod && (key === "r" || key === "p" || key === "f"))) {
    ev.preventDefault();
    ev.stopImmediatePropagation();
  }
}, true);

document.addEventListener("keydown", async (ev) => {
  if (ev.key === "Escape") {
    if (ui.screen === "world") { ui.screen = "worlds"; render(); }
  }
  if (ev.key === "Enter" && ui.addServer && ev.target.id?.startsWith("srv-")) { await addServer(); return; }
  if (ev.key === "Enter" && ui.screen === "world" && (ev.target.id === "password" || ev.target.id === "acct-name")) {
    if (ev.target.id === "acct-name" && !ui.world.form.password) { focusSoon("password"); return; }
    const c = formChoice();
    if (c && ui.world.view.world.endpoint) await play(c);
  }
});

// A world page whose world's library changed underneath it (a private copy finished) reads its
// options again, so the new set appears without the player having to reopen the page.
let lastSets = "";
async function watchLibrary() {
  const sets = JSON.stringify(ui.snap?.state.dat_sets.map((d) => d.id) ?? []);
  if (sets !== lastSets && ui.screen === "world" && ui.world) {
    lastSets = sets;
    const view = await api("world_view", { slug: ui.world.view.world.slug }).catch(() => null);
    if (view) {
      ui.world.view = view;
      if (!view.dat_sets.some((s) => s.id === ui.world.form.dat_set_id)) ui.world.form.dat_set_id = view.dat_sets[0]?.id ?? null;
      render();
    }
  }
  lastSets = sets;
}

// ----- the painting ---------------------------------------------------------------------------------

async function vista() {
  let names = [];
  try {
    names = await (await fetch("assets/backgrounds.json")).json();
  } catch (_) {
    return;
  }
  const host = document.getElementById("vista");
  const imgs = names.map((n) => {
    const i = document.createElement("img");
    i.src = `assets/background/${n}`;
    i.alt = "";
    host.appendChild(i);
    return i;
  });
  let at = 0;
  if (imgs[0]) imgs[0].classList.add("on");
  setInterval(() => {
    if (imgs.length < 2) return;
    imgs[at].classList.remove("on");
    at = (at + 1) % imgs.length;
    imgs[at].classList.add("on");
  }, 10000);
}

// ----- start --------------------------------------------------------------------------------------

// In the browser demo, `#world=eulmore`, `#worlds`, `#worlds=add`, `#library`, `#library=add`,
// `#accounts`, `#first-run` and `#first-run=guide` open that screen directly, for looking at designs.
async function demoJump() {
  const [what, arg] = location.hash.slice(1).split("=");
  if (!what) return;
  await pull();
  if (what === "world") await openWorld(arg || "eulmore");
  else if (what === "worlds" && arg === "add") {
    ui.addServer = { name: "", host: "play.example.org", port: "9000", error: null };
    ui.filter = "pve";
    ui.screen = "worlds";
  }
  else if (what === "library" && arg === "add") {
    // The "change the retail client" panel, over a folder whose dats predate the final patch.
    const f = await api("pick_folder", {});
    for (const x of f.dats.files) if (x.role === "portal") x.iterations = 2000;
    ui.addFind = { purpose: "retail", find: f };
    ui.screen = "library";
  }
  else if (what === "first-run") { ui.screen = "first-run"; ui.firstRun.step = arg || "ask"; if (arg === "found") ui.firstRun.find = await api("pick_folder", {}); }
  else if (what === "notice") { await api("create_private_copy", { slug: "coldeve" }); toast("Launching Eulmore...", false); }
  else ui.screen = what;
  render();
}

vista();
if (!window.__TAURI__) {
  document.body.insertAdjacentHTML("beforeend", `<div class="demo-flag">DEMO DATA — NOT CONNECTED</div>`);
  demoJump();
}
(async function loop() {
  await pull();
  await watchLibrary();
  setTimeout(loop, 1000);
})();
