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
  // The Plugins page: the source being added and whether adding it waits to be confirmed, the
  // search, the slow call under way, the one open row, the closed sections (all but Installed at
  // first), and the new plugin being made: its name, its GitHub repository and what the last one
  // made was.
  plugin: { add: "", confirm: false, search: "", busy: null, open: null, folded: { installed: false, available: true, sources: true, build: true }, make: { name: "", github: "", made: null } },
  screen: "home",          // home | worlds | world | library | accounts | plugins | first-run | interface
  snap: null,
  homeSel: 0,
  search: "",
  filter: null,            // the Worlds list's one rules filter: null | "pve" | "pvp"
  expanded: null,          // the Worlds list's one open row: a world's slug, or null
  folded: { custom: false, online: false, offline: true },   // the Worlds list's closed sections
  addServer: null,         // the Worlds list's "add a world" form, while it is open: { name, host, port, emulator, ruleset, error }
  world: null,             // the world page: { view, form, notice }
  firstRun: { step: "ask", find: null, dismissed: false },
  // The interface question: the interface picked so far (null: the one already saved, else the
  // first offered), whether it was opened from the Library, and whether the first start's question
  // was set aside after its answer could not be saved.
  iface: { pick: null, fromLibrary: false, dismissed: false },
  serverDraft: null,       // { settings }: a new server's form, until it is made
  serverId: null,          // the server whose page is open
  serverEdit: null,        // { id, settings }: that server's settings, open and as edited, until saved or closed
  keyDraft: null,          // { id, text, replacing }: a world's network key as pasted, until it is saved or its Settings close
  localKey: null,          // the other world on this computer whose page is open
  logFilter: {},           // per world ("host:<id>", "local:<key>"): what its log is searched for
  cmd: {},                 // per world: its command line, { kind, text, account, password }
  shutdownAt: {},          // per world: when a shutdown asked for from here ends, in ms
  folds: {},               // each folding section's state, by its id, once the person has opened or shut it
  confirmDelete: null,     // the world whose Delete world asks to be confirmed
  lyceumDraft: null,       // { name, parent, era }: a new Lyceum world's form, until it is made
  publishPick: {},         // per hosted world: the Lyceum world chosen to publish to it ("" for another)
  lyOpen: null,            // the Lyceum world whose row is open, by its world file
  lyExport: {},            // per Lyceum world: the server chosen to export it for
  lyPublish: {},           // per Lyceum world: the hosted world chosen to publish it to
  lyDelete: null,          // the Lyceum world whose Delete asks to be confirmed, by its world file
  addFind: null,           // { purpose: "retail" | "dats", find }: a folder read from Library, waiting to be confirmed
};

// ----- small helpers ---------------------------------------------------------------------------------

const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const worldBy = (slug) => ui.snap?.worlds.find((w) => w.slug === slug);
// The two clients: the Dereth client the launcher found beside itself, and the player's retail one.
const clientOf = (kind) => (kind === "retail" ? ui.snap?.state.retail : ui.snap?.dereth) ?? null;
const clients = () => [ui.snap?.dereth, ui.snap?.state.retail].filter(Boolean);
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
  if (s.kind === "classic") {
    const it = setIterations(s);
    return `Classic  ${it.portal ?? "-"}/${it.cell ?? "-"}`;
  }
  const it = setIterations(s);
  const o = s.origin;
  const what =
    o.kind === "world" ? `${worldBy(o.slug)?.name ?? o.slug} (private)`
    : o.kind === "custom" ? "Custom data files"
    : isEndOfRetail(it) ? "Modern" : "Modern (older)";
  return `${what}  ${iterLabel(it)}`;
}
// An era, in words; `null` for none.
function eraLabel(name) {
  if (!name) return null;
  return ui.snap?.eras.find((e) => e.name === name)?.label ?? name;
}
// The era a world plays, in words: the one it names, else the end of retail.
function eraText(w) {
  return eraLabel(w.era || "eor");
}
// The server software and, when the world says it, its version: "ACE | v1.77.4778".
function emulatorLabel(w) {
  const name = { empyrean: "Empyrean", ace: "ACE", classic_ace: "ClassicACE", gdle: "GDLE", unknown: "Unknown" }[w.emulator] ?? w.emulator;
  const v = (w.emulator_version ?? "").replace(/^v/i, "");
  return v ? `${name} | v${v}` : name;
}
// The systems a world has: its era's table (the end of retail's when no era is named), with what
// the world turned on or off over it, as the client reads them.
function worldFeatures(w) {
  const era = ui.snap.eras.find((e) => e.name === (w.era || "eor")) ?? ui.snap.eras[0];
  const base = era?.features ?? {};
  const said = {};
  for (const part of String(w.era_features ?? "").split(",")) {
    const [k, v] = part.split("=").map((x) => (x ?? "").trim().toLowerCase());
    if (k in base) said[k] = v === "true" || v === "on" || v === "1";
  }
  return systemsOver(base, said);
}
// An era's systems, `base`, with those a world turns on or off over them, `said`, as the server
// reads them: housing off takes apartments with it unless they are named, and apartments bring
// housing.
function systemsOver(base, said) {
  const f = { ...base, ...said };
  if (said.housing === false && !("apartments" in said)) f.apartments = false;
  if (f.apartments) f.housing = true;
  return f;
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
  // The first start asks which interface the Dereth client plays in, before anything else, while
  // the client's preferences name none.
  if ((ui.screen === "home" || ui.screen === "first-run") && interfaceAsked()) {
    ui.screen = "interface";
    ui.iface = { ...ui.iface, pick: null, fromLibrary: false };
    lastSnap = "";
  }
  if (ui.screen === "home" && snap.state.dat_sets.length === 0 && !ui.firstRun.dismissed) {
    ui.screen = "first-run";
    lastSnap = "";
  }
  if (snap.join_waiting) await takeJoin();
  if (json === lastSnap) return;
  lastSnap = json;
  render();
}

// A join link came in: Worlds opens with Add world filled in from it, for the person to check and
// add. Nothing connects until they do.
let takingJoin = false;
async function takeJoin() {
  if (takingJoin) return;
  takingJoin = true;
  try {
    const link = await api("take_join");
    if (link) {
      ui.addServer = { name: link.name, host: link.host, port: String(link.port), emulator: "unknown", ruleset: "", error: null, fromLink: true };
      ui.screen = "worlds";
      ui.world = null;
      render();
      focusSoon("srv-name");
    }
  } finally {
    takingJoin = false;
  }
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

// Each log's place before the page is drawn anew: at its newest line, or where the person left it
// (the line at its top, and how far into that line).
function logPlaces() {
  const places = {};
  for (const el of document.querySelectorAll("#main .log[data-log]")) {
    const atEnd = el.scrollHeight - el.scrollTop - el.clientHeight < 6;
    let anchor = null;
    if (!atEnd) {
      const lines = [...el.children];
      const top = lines.find((l) => l.offsetTop + l.offsetHeight > el.scrollTop);
      if (top) anchor = { text: top.textContent, index: lines.indexOf(top), offset: el.scrollTop - top.offsetTop };
    }
    places[el.dataset.log] = { atEnd, anchor, top: el.scrollTop };
  }
  return places;
}

// A log at its newest line stays there as lines come; one the person scrolled up stays on the line
// they were reading, found again however many old lines left its top.
function keepLogPlaces(places) {
  for (const el of document.querySelectorAll("#main .log[data-log]")) {
    const p = places[el.dataset.log];
    if (!p || p.atEnd) { el.scrollTop = el.scrollHeight; continue; }
    const lines = [...el.children];
    let line = null;
    if (p.anchor) {
      for (let i = Math.min(p.anchor.index, lines.length - 1); i >= 0; i -= 1) {
        if (lines[i].textContent === p.anchor.text) { line = lines[i]; break; }
      }
    }
    el.scrollTop = line ? line.offsetTop + p.anchor.offset : p.top;
  }
}

// Whether the person is selecting in a log: pressing in it, or with text selected there. The
// screen is not drawn anew under them meanwhile, and catches up once they let go.
let pressedInLog = false;
let heldForSelection = false;
function selectingInLog() {
  const sel = window.getSelection?.();
  const inLog = !!sel && !sel.isCollapsed && !!sel.anchorNode?.parentElement?.closest?.("#main .log");
  return pressedInLog || inLog;
}
function catchUp() {
  if (heldForSelection && !selectingInLog()) { heldForSelection = false; render(); }
}
document.addEventListener("pointerdown", (ev) => { pressedInLog = !!ev.target.closest?.(".log"); });
document.addEventListener("pointerup", () => { pressedInLog = false; catchUp(); });
document.addEventListener("selectionchange", catchUp);

// Re-rendering replaces the page, so the element being typed into is found again afterwards and the
// caret put back where it was.
function render() {
  if (!ui.snap) return;
  const active = document.activeElement;
  const keep = active && active.id ? { id: active.id, start: active.selectionStart, end: active.selectionEnd } : null;
  // While a pad moves about the page, the control it focused is focused again once the page is
  // drawn anew, whatever it is.
  const padKeep = !keep && document.body.classList.contains("pad") ? controlKey(active) : null;

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
  const screens = { server: serverList, "server-page": serverPage, "server-settings": settingsPage, "local-page": localPage, make: makePage, home, worlds, world: worldPage, library, accounts, plugins, "first-run": firstRun, interface: interfacePage };
  // A redraw of the same screen keeps where its lists were scrolled to: opening a row, or a status
  // update arriving, leaves the list where the player had it.
  const sameScreen = main.dataset.screen === ui.screen;
  if (sameScreen && selectingInLog()) { heldForSelection = true; return; }
  const openDetails = sameScreen ? [...main.querySelectorAll("details[open][id]")].map((el) => el.id) : [];
  const scrolled = sameScreen ? [...main.querySelectorAll(".scroll")].map((el) => el.scrollTop) : [];
  const logs = sameScreen ? logPlaces() : {};
  main.innerHTML = (screens[ui.screen] ?? home)();
  main.dataset.screen = ui.screen;
  for (const id of openDetails) { const detail = document.getElementById(id); if (detail) detail.open = true; }
  main.querySelectorAll(".scroll").forEach((el, i) => {
    if (scrolled[i] !== undefined) el.scrollTop = scrolled[i];
  });
  keepLogPlaces(logs);

  if (keep) {
    const el = document.getElementById(keep.id);
    if (el) {
      el.focus();
      if (keep.start != null && el.setSelectionRange) {
        try { el.setSelectionRange(keep.start, keep.end); } catch (_) { /* not a text input */ }
      }
    }
  }
  if (padKeep) padFocus(controlByKey(padKeep), false);
  else if (keep && document.body.classList.contains("pad")) document.getElementById(keep.id)?.classList.add("pad-focus");
}

// ----- Host a world and Build a world --------------------------------------------------------------
//
// Empyrean (the server) and Lyceum (the world builder) are downloaded only when asked, each its own
// newest release; a development build uses its checkout's own builds instead. Host a world lists
// the worlds the player hosts, each with a page of its own, and the other servers running on this
// computer; Build a world makes Lyceum worlds and opens them in the editor.

const comp = (product) => (ui.snap.components ?? []).find((c) => c.product === product);
const TITLES = { empyrean: "Empyrean", lyceum: "Lyceum" };
const SOFTWARE = { empyrean: "Empyrean", ace: "ACE", gdle: "GDLE" };
// Each program's own emblem, beside its page's title and nowhere else.
const BRAND = { empyrean: "assets/emblem/empyrean.webp", lyceum: "assets/emblem/lyceum.webp" };
const brand = (product) => `<img class="brand" src="${BRAND[product]}" alt="">`;
// Where a world's operator registers it on the Dereth Network.
const NETWORK_REGISTER = "https://dereth.network/operators/#h-register";
const CHEVRON_RIGHT = `<svg viewBox="0 0 8 12" fill="currentColor" aria-hidden="true"><rect x="1" y="1" width="2" height="2"/><rect x="3" y="3" width="2" height="2"/><rect x="5" y="5" width="2" height="2"/><rect x="3" y="7" width="2" height="2"/><rect x="1" y="9" width="2" height="2"/></svg>`;
const mb = (n) => `${Math.max(1, Math.round(n / 1048576))} MB`;
const serverBy = (id) => (ui.snap.hosts ?? []).find((h) => h.id === id);
const localBy = (key) => (ui.snap.local ?? []).find((w) => w.key === key);
// Whether the program is here to use: downloaded, or this checkout's build.
const ready = (product) => { const c = comp(product); return !!(c?.installed || c?.checkout); };
// A version as the page writes it: `v0.3.3`.
const vee = (version) => (/^v/i.test(String(version)) ? String(version) : `v${version}`);
// The program as it is here: its name and version, and whether it is a local build.
function programName(product) {
  const c = comp(product);
  const title = TITLES[product];
  if (c?.checkout) return `${title}${c.checkout_version ? ` ${vee(c.checkout_version)}` : ""} (Local Build)`;
  return c?.installed ? `${title} ${vee(c.installed.version)}` : title;
}
// The ACE world each era's world base is.
const BASE_NAMES = { eor: "ACE End of Retail", infiltration: "ACE Infiltration (2005)" };
// A world base in words: `ACE End of Retail v0.9.295`, or the era's world alone when its release
// is not known.
const baseName = (era, base) => `${BASE_NAMES[base?.era ?? era] ?? BASE_NAMES.eor}${base?.version ? ` ${base.version}` : ""}`;
// Where the Dereth Network's website shows a listed world.
const networkPage = (slug) => `https://dereth.network/worlds/?q=${encodeURIComponent(slug)}`;
// A world's name: the folder its world.toml is in.
const worldName = (path) => path.split(/[\\/]/).slice(-2, -1)[0] ?? path;
const parentOf = (path) => path.split(/[\\/]/).slice(0, -1).join(path.includes("\\") ? "\\" : "/");
// The Library's default Modern data files, which a new world and Lyceum are given.
const defaultDats = () => ui.snap.state.dat_sets.find((s) => s.kind !== "classic" && s.origin.kind === "shared")?.path ?? null;
const defaultClassicDats = () => ui.snap.state.dat_sets.find((s) => s.kind === "classic" && s.origin.kind === "shared")?.path ?? null;
// A new Lyceum world's folder name, as Lyceum names worlds: lower case, words joined by hyphens.
const folderName = (name) => String(name ?? "").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");

function progressBar(progress) {
  const [done, total] = progress;
  const frac = total ? Math.min(1, done / total) : null;
  return `<div class="bar ${frac == null ? "sweep" : ""}"><i style="width:${frac == null ? 33 : (frac * 100).toFixed(1)}%"></i></div>
    <div class="sub">${mb(done)}${total ? ` of ${mb(total)}` : ""}</div>`;
}

// Release notes as the page shows them: their text, with each web link (a Markdown link, or an
// address on its own) a link that opens in the system's browser.
function linkify(text) {
  const out = [];
  const re = /\[([^\]\n]+)\]\((https?:\/\/[^\s)]+)\)|(https?:\/\/[^\s<>()]*[^\s<>().,;:!?'"])/g;
  let at = 0;
  for (const m of String(text ?? "").matchAll(re)) {
    out.push(esc(text.slice(at, m.index)));
    const url = m[2] ?? m[3];
    out.push(`<a href="#" data-url="${esc(url)}" title="${esc(url)}">${esc(m[1] ?? url)}</a>`);
    at = m.index + m[0].length;
  }
  out.push(esc(String(text ?? "").slice(at)));
  return out.join("");
}

// The program, in one line under its page's title: what is here and the one thing to do about it.
// Where it is stays in the line's tooltip.
function programLine(product) {
  const c = comp(product);
  if (!c) return "";
  const title = TITLES[product];
  if (c.checkout) return `<div class="program-line"><span title="${esc(c.checkout)}">${esc(programName(product))}</span></div>`;
  const busy = c.busy;
  const btns = [];
  let what;
  if (c.installed) {
    what = esc(programName(product));
    if (c.offer.kind === "update") btns.push(`<button class="btn mini-btn" data-get="${product}" ${busy || c.running ? "disabled" : ""}>Update to ${esc(c.offer.version)}</button>`);
    btns.push(`<button class="btn mini-btn" data-remove="${product}" ${busy || c.running ? "disabled" : ""} title="Remove the download. Your worlds are kept.">Remove</button>`);
  } else {
    what = c.checking && !c.latest ? `Asking GitHub for the newest ${title}...` : `${title} is not downloaded yet${c.latest_size ? `: ${mb(c.latest_size)}` : ""}.`;
    btns.push(`<button class="btn" data-get="${product}" ${busy ? "disabled" : ""}>Download ${title}${c.latest ? ` ${esc(c.latest)}` : ""}</button>`);
  }
  const said = c.error
    ? `<span class="bad">${esc(c.message)}</span><button class="btn mini-btn" data-act="components-retry">Try again</button>`
    : busy ? `<span class="muted">${esc(c.message ?? "")}</span>`
    : c.running ? `<span class="muted">${title} is open.</span>`
    : c.offer.kind === "update" ? `<span class="warn">${title} ${esc(c.offer.version)} is out.</span>` : "";
  const notes = c.offer.kind !== "none" ? c.latest_notes : c.installed?.notes;
  return `<div class="program-line ${c.installed ? "" : "first"}"><span>${what}</span>${said}<span class="btns">${btns.join("")}</span></div>
    ${busy && c.progress ? progressBar(c.progress) : ""}
    ${notes ? `<details class="notes" id="notes-${product}"><summary>What's new${c.offer.kind !== "none" ? ` in ${esc(c.offer.version)}` : ""}</summary><pre>${linkify(notes)}</pre></details>` : ""}`;
}

const PHASES = { stopped: ["unknown", "Stopped"], starting: ["starting", "Starting"], running: ["online", "Running"], stopping: ["starting", "Stopping"], failed: ["offline", "Stopped with an error"] };
const isRunning = (h) => ["starting", "running", "stopping"].includes(h.status.phase);

// How long a world has been up, from when it started (seconds since 1970).
function since(started) {
  if (!started) return null;
  const s = Math.max(0, Math.floor(Date.now() / 1000) - started);
  if (s < 60) return "up under a minute";
  const m = Math.floor(s / 60);
  if (m < 60) return `up ${m} min`;
  const h = Math.floor(m / 60);
  if (h < 48) return `up ${h} h ${m % 60} min`;
  return `up ${Math.floor(h / 24)} days`;
}

// The Empyrean a world runs: its own copy's version, or this checkout's build.
function serverVersion(h) {
  if (!h.checkout) return `Empyrean ${vee(h.version)}`;
  const v = comp("empyrean")?.checkout_version ?? (/^\d/.test(h.version) ? h.version : null);
  return `Empyrean${v ? ` ${vee(v)}` : ""} (Local Build)`;
}

// A world in a line: its state, its version, who is on and how long it has been up.
function serverFacts(h) {
  const [, state] = PHASES[h.status.phase] ?? ["unknown", h.status.phase];
  const up = h.status.phase === "running";
  return [state, serverVersion(h), up && h.status.players != null ? `${h.status.players} online` : null,
    up ? since(h.status.started) : null].filter(Boolean).join(" · ");
}

// Start, Stop, or the world database a new server needs first.
function serverPrimary(h) {
  const busy = !!h.busy;
  if (isRunning(h)) return `<button class="btn" data-srv-stop="${h.id}" ${h.status.phase === "stopping" ? "disabled" : ""}>Stop</button>`;
  if (!h.prepared) return `<button class="btn" data-srv-prepare="${h.id}" ${busy ? "disabled" : ""}>Get world database</button>`;
  return `<button class="btn" data-srv-start="${h.id}" ${busy ? "disabled" : ""}>Start</button>`;
}

// A world on the list: its facts in a line, and under them only what it is doing now or why it
// stopped; everything else is on its page.
function serverRow(h) {
  const [dot] = PHASES[h.status.phase] ?? ["unknown"];
  const note = h.busy ?? (h.status.phase === "failed" ? h.status.message : null);
  return `<div class="row" data-srv-open="${h.id}">${bead(dot)}
      <div class="grow"><div class="name">${esc(h.settings.name)}</div><div class="sub">${esc(serverFacts(h))}</div>
      ${note ? `<div class="sub ${h.status.phase === "failed" ? "bad" : ""}">${esc(note)}</div>` : ""}</div>
      <div class="btns">${serverPrimary(h)}<button class="btn" data-srv-play="${h.id}" ${h.status.phase === "running" ? "" : "disabled"}>Play</button></div><span class="chev">${CHEVRON_RIGHT}</span></div>`;
}

// ----- the other worlds on this computer

const localDot = (w) => (!w.running ? "unknown" : { online: "online", starting: "starting", offline: "offline" }[w.state] ?? "unknown");

// Who started a world, in a word or two, for its row.
function managerShort(w) {
  const m = w.manager;
  if (!w.running || !m) return null;
  return { windows_service: "a Windows service", systemd: "a systemd service", launchd: "started by launchd", launcher: "started here" }[m.kind] ?? null;
}

function localFacts(w) {
  const state = !w.running ? "Stopped" : w.state === "starting" ? "Starting" : w.state === "offline" ? "Not answering" : "Running";
  return [state, w.software ? `${SOFTWARE[w.software]}${w.version ? ` ${w.version}` : ""}` : null,
    w.running && w.players != null ? `${w.players} playing` : null, w.running ? since(w.started) : null, managerShort(w)].filter(Boolean).join(" · ");
}

// Who starts and stops a world, said plainly: the launcher never stops a process it did not start.
function controlText(w) {
  if (!w.running) return w.cannot_start ?? "Stopped. The launcher can start it from its folder.";
  const m = w.manager ?? { kind: "elsewhere" };
  const mine = w.control?.can_control ? " The launcher starts and stops it." : "";
  const channel = w.channel ? " Its console takes commands from here." : "";
  switch (m.kind) {
    case "windows_service": return `Runs as the Windows service ${m.name}.${mine}${channel}`;
    case "systemd": return `Runs as the ${m.user ? "user's " : ""}systemd service ${m.unit}.${mine}${channel}`;
    case "launchd": return `Started by launchd.${channel}`;
    case "launcher": return null;
    default: return w.channel ? `Started outside the launcher.${channel} Stop asks it to save and stop.` : "Started outside the launcher, which leaves it alone. Stop it where it was started.";
  }
}

function localRow(w) {
  return `<div class="row" data-local-open="${esc(w.key)}">${bead(localDot(w))}
      <div class="grow"><div class="name">${esc(w.name)}</div><div class="sub">${esc(localFacts(w))}</div>
      ${w.message ? `<div class="sub bad">${esc(w.message)}</div>` : ""}</div>
      <span class="chev">${CHEVRON_RIGHT}</span></div>`;
}

// ----- folding sections

// A section that folds: open or shut as the person left it, else as the page opens it.
function fold(id, title, body, open) {
  const isOpen = ui.folds[id] ?? open;
  return `<details class="fold" id="${esc(id)}" ${isOpen ? "open" : ""}><summary><h2>${title}</h2></summary><div class="fold-body">${body}</div></details>`;
}
document.addEventListener("toggle", (ev) => {
  const d = ev.target;
  if (d.classList?.contains("fold") && d.id) ui.folds[d.id] = d.open;
}, true);

// ----- connecting to a world

// Whether friends elsewhere can reach the world, as far as the launcher can tell from inside.
function reachText(port) {
  const sh = ui.snap.sharing ?? {};
  const pair = `UDP ports ${port} and ${port + 1}`;
  if (sh.router && sh.shared_address) return "Your internet provider shares its address among many homes, so friends elsewhere can't reach this computer.";
  const f = (sh.forwards ?? []).find(([p]) => p === port);
  if (sh.router && f && f[1]) {
    return f[1] === sh.lan ? `Your router passes port ${port} to this computer.` : `Your router passes port ${port} to another computer (${f[1]}), not this one.`;
  }
  if (sh.router) return `Friends elsewhere need your router to pass ${pair} to this computer. The router lists no such rule.`;
  if (sh.checking) return "Asking your router...";
  return `Friends elsewhere need your router to pass ${pair} to this computer. The launcher can't see your router to check.`;
}

// What the Dereth Network found when it tried the world from outside, in a line.
function outsideText(o) {
  if (!o) return null;
  if (o.checking) return "Asking the Dereth Network to try it from outside...";
  const r = o.outcome;
  if (!r) return null;
  switch (r.kind) {
    case "answered": {
      const at = `${r.address}:${r.port}`;
      if (!r.reached) return `The Dereth Network could not reach it at ${at}. Friends elsewhere can't join yet.`;
      const said = r.world?.name ? `: ${r.world.name}${r.world.players != null ? `, ${r.world.players} playing` : ""}` : "";
      return `Reached from outside at ${at}${said}. Friends elsewhere can join.`;
    }
    case "too_soon": return `Checked a lot lately. Try again in ${Math.max(1, Math.ceil(r.seconds / 60))} min.`;
    default: return r.message;
  }
}

// A join link: the site's page for the world, which offers to open it in Dereth.
function joinLink(name, host, port) {
  const enc = (v) => encodeURIComponent(v).replace(/[!'()*]/g, (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`);
  return `https://dereth.network/join#host=${enc(host)}&port=${port}&name=${enc(name)}`;
}

// Where friends connect: this computer's address on its network and on the internet, each with
// Copy, whether friends elsewhere can reach it, a check from outside, and the link to send them.
// `o`: { kind: "host" | "local", id, name, port, bind }.
function connectBlock(o) {
  if (o.port == null) return `<p class="muted small">The launcher does not know this world's port.</p>`;
  const sh = ui.snap.sharing ?? {};
  const others = !(o.bind === "127.0.0.1" || o.bind === "::1");
  const local = others ? (sh.lan ? `${sh.lan}:${o.port}` : null) : `127.0.0.1:${o.port}`;
  const outside = others && sh.public && !sh.shared_address ? `${sh.public}:${o.port}` : null;
  const row = (label, value) => `<div class="share-row"><span class="lab">${label}</span>${value
    ? `<span class="addr">${esc(value)}</span><button class="btn mini-btn" data-copy="${esc(value)}">Copy</button>`
    : `<span class="muted">Not available</span>`}</div>`;
  const reach = others ? reachText(o.port) : `Only players on this computer can join.${o.kind === "host" ? " Allow other players in Settings to share it." : ""}`;
  const out = (sh.outside ?? {})[o.port];
  const found = outsideText(out);
  // What the Dereth Network found outside settles what the router could only suggest.
  const reached = out?.outcome?.kind === "answered" && out.outcome.reached;
  const host = outside ? sh.public : local ? local.split(":")[0] : null;
  const link = host ? joinLink(o.name, host, o.port) : null;
  return `${row("LOCAL ADDRESS", local)}${row("PUBLIC ADDRESS", outside)}
    ${reached ? "" : `<p class="small muted reach">${esc(reach)}</p>`}
    <div class="share-row"><span class="lab"></span>
      ${others ? `<button class="btn mini-btn" data-check-outside="${o.port}" ${out?.checking ? "disabled" : ""} title="The Dereth Network tries this computer at the world's port, as a friend elsewhere would">Check from outside</button>` : ""}
      <button class="btn mini-btn" data-copy-link="${esc(link ?? "")}" ${link ? "" : "disabled"} title="${esc(link ?? "")}">Copy link</button></div>
    ${found ? `<p class="small reach ${reached ? "good" : ""}">${esc(found)}</p>` : ""}`;
}

// ----- the console

// The command line's kinds: a plain command, and the few a host wants most.
const COMMAND_KINDS = [["command", "empyrean>"], ["account", "Add account..."], ["message", "Message everyone..."], ["save", "Save"], ["shutdown", "Shut down in 5 minutes"]];

// The world's log, searched, and under it, for a world that takes commands, its command line.
// `o`: { kind, id, logs, commands, exportable, empty }.
function consoleBlock(o) {
  const target = `${o.kind}:${o.id}`;
  const filter = ui.logFilter[target] ?? "";
  const lines = o.logs.filter((l) => l.toLowerCase().includes(filter.toLowerCase()));
  const log = `<div class="log" data-log="${esc(target)}">${lines.length ? lines.map((l) => `<div>${esc(l)}</div>`).join("") : `<span class="muted">${o.logs.length ? "Nothing matches." : esc(o.empty)}</span>`}</div>`;
  const tools = `<div class="btns log-tools"><input id="log-filter-${esc(o.id)}" class="field mini-field" data-log-filter="${esc(target)}" value="${esc(filter)}" placeholder="Search the log" aria-label="Search the log">
      ${o.exportable ? `<button class="btn mini-btn" data-srv-export="${esc(o.id)}">Save log</button>` : ""}</div>`;
  if (!o.commands) return tools + log;
  const c = ui.cmd[target] ?? { kind: "command", text: "", account: "", password: "" };
  const kinds = COMMAND_KINDS.map(([k, l]) => `<option value="${k}" ${c.kind === k ? "selected" : ""}>${esc(l)}</option>`).join("");
  const id = esc(o.id);
  const fields = {
    command: `<input id="cmd-${id}" class="field" data-cmd-field="text" data-cmd="${esc(target)}" value="${esc(c.text)}" placeholder="A command, as typed at the world's console" autocomplete="off">`,
    account: `<input id="cmd-acct-${id}" class="field" data-cmd-field="account" data-cmd="${esc(target)}" value="${esc(c.account)}" placeholder="Account name" autocomplete="off">
      <input id="cmd-pass-${id}" class="field" type="password" data-cmd-field="password" data-cmd="${esc(target)}" value="${esc(c.password)}" placeholder="Password" autocomplete="new-password">`,
    message: `<input id="cmd-msg-${id}" class="field" data-cmd-field="text" data-cmd="${esc(target)}" value="${esc(c.text)}" placeholder="What everyone playing reads" autocomplete="off">`,
    save: `<span class="muted small grow">Saves every player and the world now.</span>`,
    shutdown: `<span class="muted small grow">Warns everyone playing, then saves and stops in 5 minutes.</span>`,
  }[c.kind];
  const shutdown = ui.shutdownAt[target];
  const left = shutdown ? Math.max(0, Math.ceil((shutdown - Date.now()) / 60000)) : null;
  const pending = shutdown ? `<div class="btns"><span class="warn small">Shutting down${left ? ` in about ${left} min` : ""}.</span><button class="btn mini-btn" data-shutdown-cancel="${esc(target)}">Cancel shutdown</button></div>` : "";
  return `${tools}${log}
    <div class="console-line"><select class="field kind-field" data-cmd-kind="${esc(target)}" aria-label="Command">${kinds}</select>${fields}<button class="btn" data-send="${esc(target)}">Send</button></div>${pending}`;
}

// ----- Host a world's pages

// The game port a new world takes: the first no other world has. The game uses its port and the
// one after it.
function freePorts() {
  const hosts = ui.snap.hosts ?? [];
  let port = 9000;
  while (hosts.some((h) => Math.abs(h.settings.port - port) < 2) || (ui.snap.local ?? []).some((w) => w.port != null && Math.abs(w.port - port) < 2)) port += 2;
  return { port };
}

// The Library's data sets of one kind, `modern` or `classic`, which a world of an era that plays
// that kind may be given.
const librarySets = (kind) => (ui.snap.state.dat_sets ?? []).filter((s) => (s.kind ?? "modern") === kind && s.origin.kind !== "world");

const TIP_ACCOUNTS = "If checked, anyone who connects with an unknown account name will automatically have an account created for them with that account name and the provided password.";
const TIP_OTHERS = "If checked, anyone on your local network (or outside your local network if you have properly configured port forwarding) can connect to this world.";
const TIP_PER_ADDRESS = "The most players at once from each unique IP address. Empty is no limit.";
const TIP_WELCOME = "What the character screen says, line by line. Not every client shows it.";
const TIP_PLAY = "Players can play this world in their web browser at https://play.dereth.network. The world needs a secure connection for it: a certificate and its key, or a TLS proxy in front.";
const TIP_PROXY = "A proxy such as nginx or Cloudflare handles the secure connection. It must be the only way in.";
const TIP_LINK = "Linked, players here can talk with, befriend and tell characters on the other worlds of the Dereth Network.";
const TIP_SLUG = "The world's name on the Dereth Network's website, which can differ from its name here: 2 to 12 lowercase letters and digits, with hyphens between.";
const TIP_KEY = "The key the Dereth Network gave you when you registered the world. It is kept in the world's folder.";
const TIP_LISTEN = "The address and port browsers connect to.";
const TIP_ORIGINS = "The web pages players may play from, one a line. No other page can connect.";
const TIP_PUBLIC = "What browsers connect to, when a proxy changes it. Empty works it out from Listen.";
const TIP_PORT = "Each world consumes two consecutive ports.";

// The page Dereth is played from in a browser, which a world may let its players use.
const PLAY_PAGE = "https://play.dereth.network";

// A row of a form: its label on the left, its field, and a line under it when there is more to
// say than its tooltip. A label with a tooltip shows it, and so does its field.
const tipOf = (tip) => (tip ? ` title="${esc(tip)}"` : "");
const formRow = (id, label, field, { note, tip } = {}) => `<label class="lab" for="${id}"${tipOf(tip)}>${label}</label>${field}${note ? `<p class="note muted">${note}</p>` : ""}`;
const textField = (id, path, s, extra = "") => `<input id="${id}" class="field" data-srv-field="${path}" value="${esc(getField(s, path) ?? "")}" ${extra}>`;
// A check box: its label on the left like any other, the box where a value goes, and after it a
// few words of its own when it has them. A locked box can't be changed, and says why under it.
const checkRow = (id, label, path, on, { tip, trail, locked, note } = {}) => `<label class="lab" for="${id}"${tipOf(tip)}>${label}</label>
  <div class="pair box"><input type="checkbox" id="${id}" data-srv-field="${path}" ${on ? "checked" : ""}${locked ? " disabled" : ""}${tipOf(tip)}>${trail ? `<label class="trail" for="${id}"${tipOf(tip)}>${trail}</label>` : ""}</div>${note ? `<p class="note muted">${note}</p>` : ""}`;
// The game port, and the one after it that the world takes too.
function portTrail(port) {
  const p = Number(port);
  return Number.isInteger(p) && p >= 1 && p < 65535 ? `and ${p + 1}` : "";
}
const portRow = (id, s) => formRow(id, "PORT", `<div class="pair"${tipOf(TIP_PORT)}><input id="${id}" class="field narrow-field" type="number" min="1" max="65534" data-srv-field="port" value="${esc(s.port)}"${tipOf(TIP_PORT)}><span class="muted small">${esc(portTrail(s.port))}</span></div>`, { tip: TIP_PORT });
// Whether a world lets other players in: it listens on more than this computer.
const othersAllowed = (s) => s.bind !== "127.0.0.1" && s.bind !== "::1";

// A new world plays the Library's default data files and lets other players in.
function newServerDraft() {
  const dats = defaultDats() ?? librarySets("modern")[0]?.path ?? "";
  const names = new Set((ui.snap.hosts ?? []).map((h) => h.settings.name));
  let name = "My world";
  for (let n = 2; names.has(name); n += 1) name = `My world ${n}`;
  return { settings: { name, dats, bind: "0.0.0.0", accounts: true, era: "eor", ...freePorts() } };
}

// The game data a world plays: one of the Library's sets of the kind its era is drawn from, as
// the client's rule says (`needs`): Classic for an era before Throne of Destiny, else Modern.
function datsField(id, s, locked) {
  const era = eraOf(s.era);
  const kind = era?.needs ?? "modern";
  const sets = librarySets(kind);
  // Only when there is nothing to choose does the screen say anything: which files to add.
  const missing = kind === "classic" ? `${esc((era?.label ?? "This era").replace(/ \(.*\)$/, ""))} needs the 2005 game files. Add them in the Library.`
    : "You have no end-of-retail game files yet. Add them in the Library.";
  if (!sets.length && !locked) return `<div class="pair"><span class="warn small">${missing}</span><button class="btn" data-go="library">Library</button></div>`;
  const known = sets.some((set) => set.path === s.dats);
  const options = (known || !s.dats ? "" : `<option value="${esc(s.dats)}" selected>${esc(s.dats)}</option>`)
    + sets.map((set) => `<option value="${esc(set.path)}" ${set.path === s.dats ? "selected" : ""} title="${esc(set.path)}">${esc(setName(set))}</option>`).join("");
  return `<select id="${id}" class="field" data-srv-field="dats" ${locked ? `disabled${tipOf("Set by the Lyceum world it plays.")}` : ""}>${options}</select>`;
}

// What a new world asks: its name, its game data, its port, and two choices, laid out as its
// Settings lay them out.
function newWorldFields(s) {
  return `<div class="form long-labels">
      ${formRow("new-name", "NAME", `<input id="new-name" class="field" data-srv-field="name" value="${esc(s.name)}">`)}
      ${formRow("new-dats", "GAME DATA", datsField("new-dats", s))}
      ${portRow("new-port", s)}
      ${checkRow("new-others", "ALLOW OTHER PLAYERS", "others", othersAllowed(s), { tip: TIP_OTHERS })}
      ${checkRow("new-accounts", "AUTOMATICALLY CREATE ACCOUNTS", "accounts", s.accounts, { tip: TIP_ACCOUNTS })}
    </div>`;
}

function serverList() {
  const hosts = ui.snap.hosts ?? [];
  const others = ui.snap.local ?? [];
  const pending = ui.serverDraft
    ? `<section class="panel pending"><header><h1>NEW WORLD</h1></header>${newWorldFields(ui.serverDraft.settings)}
      <div class="btns end" style="margin-top:12px"><button class="btn" data-act="srv-new-cancel">Cancel</button><button class="btn" data-act="srv-create">Make world</button></div></section>`
    : "";
  const rows = hosts.map(serverRow).join("")
    || `<p class="muted">${ready("empyrean") ? "No world servers." : "Download Empyrean to host a world."}</p>`;
  const otherRows = others.length ? `<h2>OTHER WORLD SERVERS ON THIS COMPUTER</h2><div class="list">${others.map(localRow).join("")}</div>` : "";
  return `<div class="column wide" style="height:100%">${pending}
      <section class="panel grow"><header>${brand("empyrean")}<h1>EMPYREAN: HOST A WORLD SERVER</h1>${ready("empyrean") && !ui.serverDraft ? `<button class="btn" data-act="srv-new">+ New world</button>` : ""}</header>
        <div class="scroll">${ready("empyrean") ? "" : `<p class="muted small">Run a world of your own on this computer, for you and your friends.</p>`}
          ${programLine("empyrean")}
          <h2>YOUR WORLD SERVERS</h2><div class="list">${rows}</div>${otherRows}</div>
      </section>
    </div>`;
}

// A state in words beside its dot: green when all is well, amber when there is something to do,
// gray when it is off.
const stateMark = (dot, words) => `<span class="state">${bead(dot)}<span>${words}</span></span>`;

// Check for Updates, the Empyrean the world runs, and what the last check found: Latest, or a
// newer version to update to.
function updatesBlock(h) {
  const u = h.update ?? {};
  const offered = u.version && !u.pending;
  const running = isRunning(h);
  if (h.checkout) {
    return `<div class="share-row"><button class="btn mini-btn" disabled title="A local build is never updated from a release.">Check for Updates</button><span>${esc(serverVersion(h))}</span></div>`;
  }
  const found = offered ? stateMark("starting", `Empyrean ${esc(vee(u.version))} is out`)
    : u.pending ? stateMark("starting", esc(u.message ?? `Empyrean ${vee(u.version)} goes in at the next start`))
    : u.latest && u.latest === h.version ? stateMark("online", "Latest") : "";
  const said = !offered && !u.pending && !(u.latest && u.latest === h.version) && u.message ? `<p class="small muted">${esc(u.message)}</p>` : "";
  return `<div class="share-row"><button class="btn mini-btn" data-srv-check="${h.id}" ${h.busy ? "disabled" : ""}>Check for Updates</button><span>${esc(serverVersion(h))}</span>${found}
      ${offered ? `<button class="btn mini-btn" data-srv-approve="${h.id}" data-version="${esc(u.version)}" ${h.busy ? "disabled" : ""}>Update to ${esc(u.version)}</button>` : ""}
      ${u.pending ? `<button class="btn mini-btn" data-srv-cancel-update="${h.id}" ${h.busy ? "disabled" : ""}>Cancel update</button>` : ""}
      ${u.rollback ? `<button class="btn mini-btn" data-srv-rollback="${h.id}" ${h.busy || running ? "disabled" : ""}>Undo last update</button>` : ""}</div>
    ${said}
    ${offered ? `<details class="notes" id="srv-notes-${h.id}" open><summary>What's new in ${esc(u.version)}</summary><pre>${linkify(u.notes ?? "")}</pre></details>
      <p class="muted small">The databases are backed up first. If the new version does not start, everything is put back.</p>` : ""}`;
}

// Run via System Service, or Disable System Service, and whether it runs as one.
function serviceRow(h, running) {
  const why = h.checkout ? "A world on a local build runs only while the launcher is open."
    : running ? "Stop the world to change this."
    : !h.prepared ? "Get the world database first."
    : ui.snap.platform === "windows" ? "The world keeps running when the launcher is closed. Windows asks for administrator permission to set this up."
    : "The world keeps running when the launcher is closed, while you are logged in.";
  const off = running || h.busy || !h.prepared || h.checkout;
  return `<div class="share-row"><button class="btn mini-btn" data-srv-service="${h.id}" data-enabled="${h.service ? 0 : 1}" ${off ? "disabled" : ""} title="${esc(why)}">${h.service ? "Disable System Service" : "Run via System Service"}</button>
      ${h.service ? stateMark("online", "Active") : stateMark("unknown", "Inactive")}</div>`;
}

// List on the Dereth Network, and whether the world is listed, by its slug.
function listRow(h) {
  const slug = h.federation_slug;
  const state = h.listed && slug
    ? stateMark("online", `Listed as <a href="#" data-url="${esc(networkPage(slug))}" title="Its page on the Dereth Network">${esc(slug)}</a>`)
    : stateMark("unknown", "Unlisted");
  return `<div class="share-row"><button class="btn mini-btn" data-url="${NETWORK_REGISTER}" title="The Dereth Network's page for operators">List on the Dereth Network</button>${state}</div>`;
}

// Delete World, confirmed in place.
function deleteRow(h, running) {
  if (ui.confirmDelete === h.id) {
    return `<div class="share-row confirm"><span class="warn">Delete ${esc(h.settings.name)}? Its characters, accounts and world database go with it.</span>
      <button class="btn mini-btn" data-srv-delete="${h.id}">Delete World</button><button class="btn mini-btn" data-act="delete-cancel">Cancel</button></div>`;
  }
  const why = running ? "Stop the world first." : h.service ? "Disable the system service first." : "";
  return `<div class="share-row"><button class="btn mini-btn" data-confirm-delete="${h.id}" ${running || h.busy || h.service ? "disabled" : ""} title="${esc(why)}">Delete World</button></div>`;
}

// The world base a world plays, in words: ACE's world for its era and the release it was made
// from, its own world database, or nothing yet.
function worldBase(h) {
  const plays = h.plays ?? {};
  if (plays.database === "own") return `The world's own: <code>${esc(plays.pack)}</code>`;
  if (plays.database === "era" && !h.prepared) return "Not downloaded yet";
  return esc(baseName(h.settings.era, plays.base));
}

// The Lyceum world a world plays, or that it plays its world base unmodified; and the ways to
// publish one, publish the newest saved, or go back to the world base.
function lyceumBlock(h, running) {
  const p = h.publication;
  const lyceum = ready("lyceum");
  const busy = !!h.busy || !!comp("lyceum")?.busy;
  const playing = p
    ? `Lyceum world <b>${esc(p.key)}</b>${p.revision ? ` version <b>${p.revision}</b>` : ""}`
    : "Unmodified.";
  const why = !lyceum ? "Download Lyceum first." : running ? "Stop the world first." : "";
  let publish;
  if (p && h.source_gone) {
    publish = `<div class="share-row"><span class="muted small">The Lyceum world it was published from is gone.</span></div>`;
  } else if (p) {
    const latest = h.unpublished_changes === false;
    const next = latest ? `<b>${esc(p.key)}</b>${p.revision ? ` version <b>${p.revision}</b>` : ""} is the latest`
      : `<b>${esc(p.key)}</b>${p.revision ? ` version <b>${p.revision + 1}</b>` : ""}`;
    publish = `<div class="share-row"><button class="btn mini-btn" data-srv-republish="${h.id}" ${lyceum && !running && !busy && !latest ? "" : "disabled"} title="${esc(latest ? "The saved world is the one published." : why)}">Publish Latest</button><span>${next}</span></div>`;
  } else {
    const worlds = comp("lyceum")?.worlds ?? [];
    const pick = ui.publishPick[h.id] ?? worlds[0]?.path ?? "";
    const options = worlds.map((w) => `<option value="${esc(w.path)}" ${w.path === pick ? "selected" : ""} title="${esc(w.path)}">${esc(worldName(w.path))}</option>`).join("")
      + `<option value="" ${pick === "" ? "selected" : ""}>Other...</option>`;
    publish = `<div class="share-row"><button class="btn mini-btn" data-srv-publish="${h.id}" ${lyceum && !running && !busy ? "" : "disabled"} title="${esc(why)}">Publish Lyceum World</button>
      <select class="field mini" data-publish-pick="${h.id}" aria-label="The Lyceum world to publish" ${lyceum ? "" : "disabled"}>${options}</select></div>`;
  }
  return `<div class="aligned"><div class="share-row"><span class="lab">WORLD BASE</span><span>${worldBase(h)}</span></div>
    <div class="share-row"><span class="lab">RUNNING</span><span>${playing}</span></div>
    ${publish}
    <div class="share-row"><button class="btn mini-btn" data-srv-unpublish="${h.id}" ${p && !running && !h.busy ? "" : "disabled"} title="${p ? "The world goes back to its world base, and its era and game data may change again." : "It plays its world base already."}">Restore World Base</button></div>
    ${running ? `<p class="small muted">Stop the world to publish or restore its world base.</p>` : ""}</div>`;
}

function serverPage() {
  const h = serverBy(ui.serverId);
  if (!h) { ui.screen = "server"; return serverList(); }
  const [dot] = PHASES[h.status.phase] ?? ["unknown"];
  const u = h.update ?? {};
  const note = h.busy ?? h.status.message ?? h.message;
  const running = isRunning(h);
  const offered = u.version && !u.pending;
  const advanced = `<div class="aligned">${updatesBlock(h)}
    <div class="share-row"><button class="btn mini-btn" data-srv-folder="${h.id}" title="${esc(h.folder)}">Open world folder</button></div>
    ${serviceRow(h, running)}
    ${listRow(h)}
    ${deleteRow(h, running)}</div>`;
  return `<div class="column wide" style="height:100%">
      <section class="panel grow"><header><button class="btn icon-btn back" data-go="server" title="Back to your world servers">${CHEVRON_LEFT}</button><h1>${esc(h.settings.name.toUpperCase())}</h1>
          ${serverPrimary(h)}<button class="btn" data-srv-play="${h.id}" ${h.status.phase === "running" ? "" : "disabled"}>Play</button>
          <button class="btn" data-act="srv-settings">Settings</button></header>
        <div class="scroll">
          <div class="facts">${bead(dot)}<span>${esc(serverFacts(h))}</span></div>
          ${note ? `<p class="small ${h.status.phase === "failed" ? "bad" : ""}">${esc(note)}</p>` : ""}
          ${h.prepared ? "" : `<p class="small warn">Before its first start the world needs its world database, a one-time download. Get world database fetches it.</p>`}
          ${offered ? `<p class="small warn">Empyrean ${esc(u.version)} is out. It is under Advanced.</p>` : ""}
          ${fold(`fold-connect-${h.id}`, "CONNECT", connectBlock({ kind: "host", id: h.id, name: h.settings.name, port: h.settings.port, bind: h.settings.bind }), true)}
          ${fold(`fold-console-${h.id}`, "CONSOLE", consoleBlock({ kind: "host", id: h.id, logs: h.logs, commands: h.status.phase === "running", exportable: true, empty: "Nothing yet. Empyrean's output shows here while the world runs." }), true)}
          ${fold(`fold-lyceum-${h.id}`, "LYCEUM", lyceumBlock(h, running), !!h.publication)}
          ${fold(`fold-advanced-${h.id}`, "ADVANCED", advanced, !!offered)}
        </div>
      </section>
    </div>`;
}

// The log levels `server.log_level` takes, as the file writes them.
const LOG_LEVELS = ["error", "warn", "info", "debug", "trace"];

// A value written the same way whatever order its keys were set in, to tell edited settings from
// the saved ones.
const stable = (v) => Array.isArray(v) ? `[${v.map(stable).join(",")}]`
  : v && typeof v === "object" ? `{${Object.keys(v).sort().map((k) => `${JSON.stringify(k)}:${stable(v[k])}`).join(",")}}`
  : JSON.stringify(v);

// Settings as they are saved: the allowed pages one to a line, blank lines dropped.
function cleanSettings(s) {
  const c = structuredClone(s);
  if (c.websocket) c.websocket.allowed_origins = (c.websocket.allowed_origins ?? []).map((o) => o.trim()).filter(Boolean);
  return c;
}

// Sets a setting by its path in the form: `name`, or `websocket.listen`.
function setField(s, path, v) {
  const keys = path.split(".");
  let o = s;
  for (const k of keys.slice(0, -1)) o = o[k] ??= {};
  o[keys[keys.length - 1]] = v;
}
const getField = (s, path) => path.split(".").reduce((o, k) => o?.[k], s);

// A date from seconds since 1970, as a person writes it.
function dateOf(seconds) {
  const d = new Date(seconds * 1000);
  return longDate(`${d.getFullYear()}-${d.getMonth() + 1}-${d.getDate()}`);
}

// The era a world is set to play: its label, the data files it is drawn from (`needs`), and its
// own systems.
const eraOf = (name) => ui.snap.eras.find((e) => e.name === (name || "eor")) ?? ui.snap.eras[0];

// After the era changes, the game data follows it to the kind of data files the new era plays,
// unless the files chosen are of that kind already.
function eraChanged(s) {
  const sets = librarySets(eraOf(s.era)?.needs ?? "modern");
  if (!sets.some((set) => set.path === s.dats)) s.dats = (sets.find((set) => set.origin.kind === "shared") ?? sets[0])?.path ?? s.dats;
}

// A world's settings as its Settings screen edits them: a copy, with the page Dereth is played
// from among the allowed pages of a world that names none. Like any change, that is written only
// when the person saves.
function editCopy(h) {
  const s = structuredClone(h.settings);
  s.websocket ??= {};
  if (!(s.websocket.allowed_origins ?? []).some((o) => o.trim())) s.websocket.allowed_origins = [PLAY_PAGE];
  return s;
}

// The network key's row: a box to paste it into and Save, or, once it is kept, only that it is.
// A kept key is never drawn again.
function keyField(h) {
  const draft = ui.keyDraft?.id === h.id ? ui.keyDraft : null;
  if (h.plays?.network_key && !draft?.replacing) {
    return `<div class="pair"><span class="small fit">Key saved</span><button class="btn mini-btn" data-act="key-replace">Replace</button></div>`;
  }
  return `<div class="pair"><input id="set-fed-key" class="field" type="password" data-key-field autocomplete="off" spellcheck="false" placeholder="Paste the key from the Dereth Network" value="${esc(draft?.text ?? "")}"${tipOf(TIP_KEY)}>
      <button class="btn mini-btn" data-act="key-save" ${draft?.text?.trim() && !h.busy ? "" : "disabled"}>Save</button>
      ${draft?.replacing ? `<button class="btn mini-btn" data-act="key-cancel">Cancel</button>` : ""}</div>`;
}

// A world's Settings: everything that matters in its empyrean.toml, a folding section to each
// part. The rest stays in the file, which Open empyrean.toml opens.
function settingsPage() {
  const h = serverBy(ui.serverId);
  if (!h) { ui.screen = "server"; return serverList(); }
  if (ui.serverEdit?.id !== h.id) ui.serverEdit = { id: h.id, settings: editCopy(h) };
  const s = ui.serverEdit.settings;
  const changed = stable(cleanSettings(s)) !== stable(cleanSettings(editCopy(h)));
  const running = isRunning(h);
  const plays = h.plays ?? {};
  // A world that plays a Lyceum world keeps the era and data files it was published for, and a
  // published one sends its data to Dereth players.
  const locked = !!plays.dat_overlay;
  const published = !!plays.published;
  const era = eraOf(s.era);
  const id = h.id;

  const eras = ui.snap.eras.map((e) => `<option value="${esc(e.name)}" ${s.era === e.name ? "selected" : ""}>${esc(e.label)}</option>`).join("");
  // The world base: the one it plays, or the one saving switches it to.
  const eraChangedHere = s.era !== h.settings.era;
  const next = eraChangedHere && plays.database === "era" ? (plays.era_bases ?? []).find((b) => b.era === s.era) : null;
  const database = eraChangedHere && plays.database === "era" ? `Saving will switch world base to ${esc(baseName(s.era, next))}`
    : worldBase(h);
  const getDatabase = plays.database === "era" && !h.prepared && !eraChangedHere
    ? `<button class="btn mini-btn" data-srv-prepare="${id}" ${running || h.busy ? "disabled" : ""}>Get world database</button>` : "";
  const world = `<div class="form long-labels">
      ${formRow("set-name", "NAME", textField("set-name", "name", s))}
      ${formRow("set-era", "ERA", `<select id="set-era" class="field" data-srv-field="era" ${locked ? "disabled" : ""}${locked ? tipOf("Set by the Lyceum world it plays.") : ""}>${eras}</select>`)}
      ${formRow("set-dats", "GAME DATA", datsField("set-dats", s, locked))}
      <span class="lab">WORLD BASE</span><div class="pair"><span class="small">${database}</span>${getDatabase}</div>
      ${formRow("set-welcome", "WELCOME", `<textarea id="set-welcome" class="field area" rows="3" data-srv-field="welcome" placeholder="Welcome to ${esc(s.name)}!"${tipOf(TIP_WELCOME)}>${esc(s.welcome ?? "")}</textarea>`, { tip: TIP_WELCOME })}
      ${portRow("set-port", s)}
    </div>`;

  const players = `<div class="form long-labels">
      ${checkRow("set-others", "ALLOW OTHER PLAYERS", "others", othersAllowed(s), { tip: TIP_OTHERS })}
      ${s.bind !== "127.0.0.1" && s.bind !== "0.0.0.0" && s.bind !== "::1" ? `<p class="note muted">Listens on ${esc(s.bind)}, as empyrean.toml says.</p>` : ""}
      ${checkRow("set-accounts", "AUTOMATICALLY CREATE ACCOUNTS", "accounts", s.accounts, { tip: TIP_ACCOUNTS })}
      ${formRow("set-max", "MAXIMUM PLAYERS", `<input id="set-max" class="field narrow-field" type="number" min="1" max="10000" data-srv-field="max_players" value="${esc(s.max_players ?? 128)}">`)}
      ${formRow("set-per", "MAXIMUM PLAYERS FROM EACH ADDRESS", `<input id="set-per" class="field narrow-field" type="number" min="1" max="10000" data-srv-field="max_per_address" value="${s.max_per_address > 0 ? esc(s.max_per_address) : ""}" placeholder="No limit"${tipOf(TIP_PER_ADDRESS)}>`, { tip: TIP_PER_ADDRESS })}
      ${formRow("set-warn", "SHUTDOWN WARNING", `<div class="pair"><input id="set-warn" class="field narrow-field" type="number" min="0" max="99999" data-srv-field="shutdown_warning" value="${esc(s.shutdown_warning ?? 60)}"><span class="muted small">seconds</span></div>`)}
    </div>`;

  const said = s.systems ?? {};
  const on = systemsOver(era?.features ?? {}, said);
  const boxes = ui.snap.features.map((f) => {
    const mine = f.name in said;
    return `<label class="check small ${mine ? "changed" : ""}" title="${mine ? `Changed from ${esc(era?.label)}` : `As ${esc(era?.label)} has it`}"><input type="checkbox" data-srv-system="${esc(f.name)}" ${on[f.name] ? "checked" : ""}>${esc(f.label)}</label>`;
  }).join("");
  const systems = `<div class="features">${boxes}</div>
    <div class="btns" style="margin-top:8px"><button class="btn mini-btn" data-act="set-systems-reset" ${Object.keys(said).length ? "" : "disabled"}>Use defaults for ${esc(era?.label)}</button></div>`;

  // The endpoint on or off, and the pages that may connect to it, the page Dereth is played from
  // among them to begin with.
  const ws = s.websocket ?? {};
  const origins = ws.allowed_origins ?? [];
  const proxy = !!ws.behind_tls_proxy;
  const websocket = `<div class="form long-labels">
      ${checkRow("set-ws-enabled", "ENABLE WEBSOCKET CONNECTIONS", "websocket.enabled", !!ws.enabled, { tip: TIP_PLAY })}
      ${checkRow("set-ws-proxy", "BEHIND A TLS PROXY", "websocket.behind_tls_proxy", proxy, { tip: TIP_PROXY })}
      ${proxy ? "" : `${formRow("set-ws-cert", "CERTIFICATE", textField("set-ws-cert", "websocket.tls_certificate", s, `placeholder="The certificate file (PEM)"`))}
        ${formRow("set-ws-key", "KEY", textField("set-ws-key", "websocket.tls_private_key", s, `placeholder="Its private key file (PEM)"`))}`}
      ${formRow("set-ws-listen", "LISTEN", textField("set-ws-listen", "websocket.listen", s, `placeholder="0.0.0.0:9443"${tipOf(TIP_LISTEN)}`), { tip: TIP_LISTEN })}
      ${formRow("set-ws-origins", "ALLOWED PAGES", `<textarea id="set-ws-origins" class="field area" rows="2" data-srv-field="websocket.allowed_origins" placeholder="${PLAY_PAGE}"${tipOf(TIP_ORIGINS)}>${esc(origins.join("\n"))}</textarea>`, { tip: TIP_ORIGINS })}
      ${formRow("set-ws-public", "PUBLIC ADDRESS", textField("set-ws-public", "websocket.public_url", s, `placeholder="wss://play.example.org/ws"${tipOf(TIP_PUBLIC)}`), { tip: TIP_PUBLIC })}
    </div>`;

  const network = `<div class="form long-labels">
      ${checkRow("set-fed-on", "LINK TO THE DERETH NETWORK", "federation.enabled", !!s.federation?.enabled, { tip: TIP_LINK })}
      ${formRow("set-fed-slug", "SLUG", textField("set-fed-slug", "federation.slug", s, `placeholder="my-world"${tipOf(TIP_SLUG)}`), { tip: TIP_SLUG })}
      ${formRow("set-fed-key", "KEY", keyField(h), { tip: TIP_KEY })}
      <span></span><div class="pair"><button class="btn mini-btn" data-url="${NETWORK_REGISTER}" title="The Dereth Network's page for operators">Link this world</button>
        ${h.listed ? `<span class="good small">Listed on the Dereth Network.</span>` : `<span class="muted small">Not listed yet.</span>`}</div>
    </div>`;

  const patching = `<details class="notes" id="set-patching-${id}"><summary>Patching</summary>
      <div class="form long-labels" style="margin-top:8px">
        ${checkRow("set-patch-on", "SEND TO DERETH CLIENTS", "patching.enabled", published || (s.patching?.enabled ?? true), { tip: "Sends the Lyceum world's data to a Dereth client that joins without it. Off, such a client is turned away.", locked: published, note: published ? "This is how players get the Lyceum world." : "" })}
        ${formRow("set-patch-records", "RECORDS A MINUTE", `<input id="set-patch-records" class="field narrow-field" type="number" min="0" data-srv-field="patching.records_per_minute" value="${esc(s.patching?.records_per_minute ?? 60000)}">`)}
        ${formRow("set-patch-bytes", "BYTES A SECOND", `<input id="set-patch-bytes" class="field narrow-field" type="number" min="1" data-srv-field="patching.bytes_per_second" value="${esc(s.patching?.bytes_per_second ?? 1048576)}">`)}
      </div></details>`;
  const lyceum = `${h.publication ? `<p class="small" title="${esc(h.publication.source)}">Plays the Lyceum world <b>${esc(h.publication.key)}</b>${h.publication.revision ? ` version ${h.publication.revision}` : ""}${plays.published ? `, published ${esc(dateOf(plays.published))}` : ""}.</p>`
      : plays.dat_overlay ? `<p class="small">Plays the data overlay in <code>${esc(plays.dat_overlay)}</code>.</p>`
      : `<p class="small muted">Plays no Lyceum world. Publish one from the world's page, under Lyceum.</p>`}
    ${plays.content_overlay ? `<p class="small muted">Changes to its content made in the world are kept in <code>${esc(plays.content_overlay)}</code>.</p>` : ""}
    ${plays.dat_overlay ? patching : ""}`;

  const levels = LOG_LEVELS.map((v) => `<option value="${v}" ${s.log_level === v ? "selected" : ""}>${v}</option>`).join("");
  const advanced = `<div class="form long-labels">
      ${formRow("set-log", "LOG LEVEL", `<select id="set-log" class="field narrow-select" data-srv-field="log_level">${levels}</select>`)}
    </div>`;

  return `<div class="column wide" style="height:100%">
      <section class="panel grow"><header><button class="btn icon-btn back" data-act="set-close" title="Back to the world">${CHEVRON_LEFT}</button><h1>SETTINGS</h1>
          <span class="muted small">${esc(h.settings.name)}</span>
          <button class="btn" data-act="set-cancel" ${changed ? "" : "disabled"}>Undo changes</button>
          <button class="btn" data-act="srv-save" ${changed && !h.busy ? "" : "disabled"}>Save</button></header>
        <div class="scroll">
          ${running ? `<p class="small warn">The world reads its settings when it starts: changes take effect at its next start.</p>` : ""}
          ${h.message?.startsWith("Saved") || h.busy ? `<p class="small">${esc(h.busy ?? h.message)}</p>` : ""}
          ${fold(`set-world-${id}`, "WORLD", world, true)}
          ${fold(`set-players-${id}`, "PLAYERS", players, true)}
          ${fold(`set-systems-${id}`, "ERA SYSTEMS", systems, false)}
          ${fold(`set-websocket-${id}`, "WEBSOCKET", websocket, false)}
          ${fold(`set-network-${id}`, "DERETH NETWORK", network, false)}
          ${fold(`set-lyceum-${id}`, "LYCEUM WORLD", lyceum, locked)}
          ${fold(`set-advanced-${id}`, "ADVANCED", advanced, false)}
          <div class="btns" style="margin-top:14px"><button class="btn" data-srv-config="${id}" title="${esc(h.folder)}">Open empyrean.toml</button><span class="muted small">Everything else is in empyrean.toml.</span></div>
        </div>
      </section>
    </div>`;
}

// A world the launcher found running on this computer, or remembers: what it is, who runs it, its
// log and console, and its folders.
function localPage() {
  const w = localBy(ui.localKey);
  if (!w) { ui.screen = "server"; return serverList(); }
  const control = controlText(w);
  const c = w.control;
  const places = [["folder", "Folder", w.folder], ["config", w.config ? w.config.split(/[\\/]/).pop() : "Config", w.config], ["data", "Data", w.data], ["dats", "Game data", w.dats], ["log", "Log", w.log]]
    .filter(([, , path]) => path)
    .map(([place, label, path]) => `<button class="btn mini-btn" data-reveal-local="${esc(w.key)}" data-place="${place}" title="${esc(path)}">${esc(label)}</button>`).join("");
  const stoppable = w.running && (w.ours || w.channel || c?.can_control);
  const buttons = [
    !w.running && !w.cannot_start ? `<button class="btn" data-local-start="${esc(w.key)}">Start</button>` : "",
    stoppable ? `<button class="btn" data-local-stop="${esc(w.key)}" data-now="${w.stopping && w.ours ? 1 : 0}" title="${w.stopping && w.ours ? "Ends it at once, without saving" : "Asks it to save and stop"}">${w.stopping && w.ours ? "Stop now" : "Stop"}</button>` : "",
    `<button class="btn" data-local-play="${esc(w.key)}" ${w.running && w.port != null && w.state !== "offline" ? "" : "disabled"}>Play</button>`,
  ].join("");
  const service = !c ? "" : c.can_grant
    ? `<div class="share-row"><button class="btn mini-btn" data-allow-service="${esc(w.key)}" title="Windows asks for administrator permission once">Let the launcher start and stop this service</button></div>`
    : !c.can_control && c.start_command ? `<p class="small muted">Start it with <code>${esc(c.start_command)}</code>${w.channel ? "" : `, and stop it with <code>${esc(c.stop_command)}</code>`}.</p>` : "";
  const takes = (w.ours || w.channel) && !w.stopping;
  const advanced = `${places ? `<div class="share-row">${places}</div>` : ""}
    ${w.software === "empyrean" ? `<div class="share-row">${w.listed ? `<span class="good">Listed on the Dereth Network.</span>` : `<button class="btn mini-btn" data-url="${NETWORK_REGISTER}">List on the Dereth Network</button>`}</div>` : ""}
    ${!w.running ? `<div class="share-row"><button class="btn mini-btn" data-local-forget="${esc(w.key)}">Forget this world</button></div>` : ""}`;
  const empty = w.log || w.channel ? "The log is empty." : w.software === "empyrean" && !w.ours ? "Empyrean writes its log where it was started: the launcher can't see it." : "The launcher found no log.";
  return `<div class="column wide" style="height:100%">
      <section class="panel grow"><header><button class="btn icon-btn back" data-go="server" title="Back to your world servers">${CHEVRON_LEFT}</button><h1>${esc(w.name.toUpperCase())}</h1>${buttons}</header>
        <div class="scroll">
          <div class="facts">${bead(localDot(w))}<span>${esc(localFacts(w))}</span></div>
          ${control ? `<p class="small muted">${esc(control)}</p>` : ""}
          ${service}
          ${w.stopping ? `<p class="small warn">Saving and stopping.${w.ours ? " Stop now ends it at once, without saving." : ""}</p>` : ""}
          ${w.message ? `<p class="small bad">${esc(w.message)}</p>` : ""}
          ${fold(`fold-connect-${w.key}`, "CONNECT", connectBlock({ kind: "local", id: w.key, name: w.name, port: w.port, bind: w.host }), true)}
          ${fold(`fold-console-${w.key}`, takes ? "CONSOLE" : "LOG", consoleBlock({ kind: "local", id: w.key, logs: w.logs, commands: takes, exportable: false, empty }), true)}
          ${fold(`fold-advanced-${w.key}`, "ADVANCED", advanced, false)}
        </div>
      </section>
    </div>`;
}

// ----- Build a world

// What a Lyceum world changes, counted, as Lyceum says, in words: only what it changes at all.
const FACT_WORDS = [
  ["terrain", "landblock with terrain edits", "landblocks with terrain edits"],
  ["statics", "static", "statics"],
  ["buildings", "building", "buildings"],
  ["dungeons", "dungeon", "dungeons"],
  ["rooms", "room", "rooms"],
  ["spawns", "spawn", "spawns"],
  ["weenies", "weenie", "weenies"],
  ["textures", "new texture", "new textures"],
  ["models", "new model", "new models"],
];
function factsText(facts) {
  const said = FACT_WORDS.filter(([k]) => facts?.[k] > 0).map(([k, one, many]) => `${facts[k]} ${facts[k] === 1 ? one : many}`);
  return said.length ? said.join(" · ") : "Nothing changed yet.";
}

// The servers a Lyceum world is exported for.
const EXPORT_TARGETS = [["empyrean", "Empyrean"], ["ace", "ACE"], ["gdle", "GDLE"]];

// A Lyceum world's row: its name with where it is under it, and Open; opened, what it changes,
// and the ways to export it and to publish it to a world you host.
function lyceumRow(w, on, busy) {
  const c = comp("lyceum");
  const open = ui.lyOpen === w.path;
  const hosts = ui.snap.hosts ?? [];
  const btns = `<button class="btn" data-lyceum-world="${esc(w.path)}" ${on && !busy ? "" : "disabled"}>Open</button>
    ${w.outdated ? `<button class="btn" data-lyceum-upgrade="${esc(w.path)}" ${on && !busy && !c.running ? "" : "disabled"} title="Bring the world's files up to this Lyceum's format, keeping a backup">Upgrade</button>` : ""}`;
  const row = `<div class="row ly-row" data-ly-row="${esc(w.path)}"><span class="fold-mark">${open ? "▾" : "▸"}</span>
      <div class="grow"><div class="name">${esc(worldName(w.path))}</div><div class="sub" title="${esc(w.path)}">${esc(parentOf(w.path))}</div>
      ${w.outdated ? `<div class="sub warn">Its files are from an older Lyceum.</div>` : ""}</div>
      <div class="btns">${btns}</div></div>`;
  if (!open) return row;
  const target = ui.lyExport[w.path] ?? "empyrean";
  const exportOpts = EXPORT_TARGETS.map(([v, l]) => `<option value="${v}" ${v === target ? "selected" : ""}>${l}</option>`).join("");
  const hostId = hosts.some((h) => h.id === ui.lyPublish[w.path]) ? ui.lyPublish[w.path] : hosts[0]?.id;
  const host = hosts.find((h) => h.id === hostId);
  const hostOpts = hosts.map((h) => `<option value="${esc(h.id)}" ${h.id === hostId ? "selected" : ""}>${esc(h.settings.name)}</option>`).join("");
  const publishWhy = !hosts.length ? "Make a world in Empyrean first." : host && isRunning(host) ? `Stop ${host.settings.name} first.` : "";
  const facts = [["ERA", eraLabel(w.era) ?? w.era], ["SAVED", w.saved ? dateOf(w.saved) : "never"]];
  const detail = `<div class="ly-detail" data-stop>
      <div class="strip">${facts.map(([k, v]) => `<span><b>${k}</b>${esc(v)}</span>`).join("")}</div>
      ${w.facts ? `<p class="small">${esc(factsText(w.facts))}</p>` : ""}
      <div class="ly-actions"><div class="share-row"><button class="btn mini-btn" data-ly-export="${esc(w.path)}" ${on && !busy ? "" : "disabled"}>Export For</button>
        <select class="field mini" data-ly-export-pick="${esc(w.path)}" aria-label="The server to export for">${exportOpts}</select></div>
      <div class="share-row"><button class="btn mini-btn" data-ly-publish="${esc(w.path)}" ${on && !busy && hosts.length && !(host && isRunning(host)) && !host?.busy ? "" : "disabled"} title="${esc(publishWhy)}">Publish To</button>
        ${hosts.length ? `<select class="field mini" data-ly-publish-pick="${esc(w.path)}" aria-label="The world you host to publish it to">${hostOpts}</select>` : `<span class="muted small">You host no worlds yet.</span>`}</div>
      ${lyceumDeleteRow(w, busy, c?.running)}</div>
    </div>`;
  return `<div class="ly-world open">${row}${detail}</div>`;
}

// A Lyceum world's Delete, confirmed in place.
function lyceumDeleteRow(w, busy, running) {
  if (ui.lyDelete === w.path) {
    return `<div class="share-row confirm"><span class="warn">Delete the Lyceum world ${esc(worldName(w.path))}?</span>
      <button class="btn mini-btn" data-ly-delete="${esc(w.path)}">Delete</button><button class="btn mini-btn" data-act="ly-delete-cancel">Cancel</button></div>`;
  }
  return `<div class="share-row"><button class="btn mini-btn" data-ly-confirm-delete="${esc(w.path)}" ${busy || running ? "disabled" : ""} title="${running ? "Cannot delete while Lyceum is open" : ""}">Delete</button></div>`;
}

function makePage() {
  const c = comp("lyceum");
  const on = ready("lyceum");
  const worlds = c?.worlds ?? [];
  const busy = c?.busy;
  const mine = worlds.filter((w) => w.home);
  const elsewhere = worlds.filter((w) => !w.home);
  const rows = mine.map((w) => lyceumRow(w, on, busy)).join("")
    || `<p class="muted">${on ? "No worlds." : "Download Lyceum to build a world."}</p>`;
  const others = elsewhere.length
    ? `<h2>OTHER WORLDS</h2><div class="list">${elsewhere.map((w) => lyceumRow(w, on, busy)).join("")}</div>` : "";
  let pending = "";
  const d = ui.lyceumDraft;
  if (d) {
    const folder = folderName(d.name);
    const where = d.parent ?? c?.home ?? "";
    const eras = [["eor", "End of Retail"], ["infiltration", "Infiltration"]]
      .map(([v, l]) => `<option value="${v}" ${(d.era ?? "eor") === v ? "selected" : ""}>${l}</option>`).join("");
    pending = `<section class="panel pending"><header><h1>NEW WORLD</h1></header>
      <div class="form">
        <label class="lab" for="ly-name">NAME</label><input id="ly-name" class="field" data-ly-field="name" value="${esc(d.name)}">
        <label class="lab" for="ly-parent">FOLDER</label>
        <div class="pair"><input id="ly-parent" class="field" data-ly-field="parent" value="${esc(where)}"><button class="btn" data-act="ly-parent">Choose</button></div>
        <label class="lab" for="ly-era">ERA</label>
        <select id="ly-era" class="field" data-ly-field="era" style="max-width:260px">${eras}</select>
      </div>
      <div class="btns end" style="margin-top:12px"><button class="btn" data-act="ly-cancel">Cancel</button><button class="btn" data-act="ly-create" ${folder && !busy ? "" : "disabled"}>Start Building</button></div></section>`;
  }
  // What the last action said (an upgrade's or an export's result), but not that the download is
  // ready, which the program's line already shows.
  const said = c?.message && !busy && c.message !== `Lyceum ${c.installed?.version} is ready.` ? `<p class="small ${c.error ? "bad" : ""}">${esc(c.message)}</p>`
    : busy && c?.message ? `<p class="small muted">${esc(c.message)}</p>` : "";
  return `<div class="column wide" style="height:100%">${pending}
      <section class="panel grow"><header>${brand("lyceum")}<h1>LYCEUM: BUILD A WORLD</h1>
          ${on && !d ? `<button class="btn" data-act="ly-new" ${busy ? "disabled" : ""}>+ New world</button>` : ""}
          ${on ? `<button class="btn" data-act="choose-lyceum-world" ${busy ? "disabled" : ""} title="Open any world, by its folder">Open World</button>` : ""}</header>
        <div class="scroll">${on ? "" : `<p class="muted small">Make places, people and things with Lyceum, Dereth's world builder, then host them.</p>`}
          ${programLine("lyceum")}${said}
          <div class="h2-row"><h2>YOUR WORLDS</h2>${c?.home ? `<button class="btn mini-btn" data-act="lyceum-folder" title="${esc(c.home)}">Open Folder</button>` : ""}</div>
          <div class="list">${rows}</div>${others}</div>
      </section>
    </div>`;
}

// The settings the form on screen edits: a new server's, or the open server's, copied when its
// Settings open and kept until they are saved or closed.
function srvSettings() {
  if (ui.serverDraft && ui.screen === "server") return ui.serverDraft.settings;
  if (ui.screen !== "server-settings") return {};
  const h = serverBy(ui.serverId);
  if (!h) return {};
  if (ui.serverEdit?.id !== h.id) ui.serverEdit = { id: h.id, settings: editCopy(h) };
  return ui.serverEdit.settings;
}

// Ask GitHub for the newest releases when a page that offers them opens, unless it was asked a
// moment ago.
function checkComponents(force) {
  api("check_components", { force }).then(pull, () => {});
}

// While Host a world is open: a look at the servers on this computer every few seconds, and at
// this computer's address and its router for the worlds' ports now and then. The backend keeps each
// answer a while, so asking often costs nothing.
let lastLook = 0;
function lookAround() {
  if (!["server", "server-page", "local-page"].includes(ui.screen) || !ui.snap) return;
  if (Date.now() - lastLook < 8000) return;
  lastLook = Date.now();
  api("scan_local_worlds", { force: false }).catch(() => {});
  const ports = [...(ui.snap.hosts ?? []).map((h) => h.settings.port), ...(ui.snap.local ?? []).map((w) => w.port).filter((p) => p != null)];
  api("check_sharing", { ports: [...new Set(ports)].sort((a, b) => a - b), force: false }).catch(() => {});
}

// Pass `line` to a world's console: one the launcher hosts, one it started from a folder, or one
// it reaches through its admin channel.
async function sendCommand(target, line) {
  const [kind, ...rest] = target.split(":");
  const id = rest.join(":");
  if (kind === "host") return act("host_command", { id, line });
  return act("local_command", { key: id, line });
}

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
  } catch (_) {
    const area = document.createElement("textarea");
    area.value = text;
    document.body.appendChild(area);
    area.select();
    document.execCommand("copy");
    area.remove();
  }
  toast("Copied.");
}

function rail() {
  const lit = { world: "worlds", "first-run": "home", interface: ui.iface.fromLibrary ? "library" : "home", "server-page": "server", "server-settings": "server", "local-page": "server" }[ui.screen] ?? ui.screen;
  const item = (id, label) => `<button class="${lit === id ? "lit" : ""}" data-go="${id}">${label}</button>`;
  // Playing first, with the plugins the client plays with; then hosting a world (Empyrean) and
  // building one (Lyceum); then the Library, each part apart.
  const divider = '<div class="rail-divider" role="separator"></div>';
  return [
    item("home", "PLAY"),
    item("worlds", "WORLDS"),
    item("accounts", "ACCOUNTS"),
    item("plugins", "PLUGINS"),
    divider,
    item("server", "EMPYREAN"),
    item("make", "LYCEUM"),
    divider,
    item("library", "LIBRARY"),
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
  // A world the list no longer names is not offered; the backend forgets it once the list is read.
  const listed = (e) => ui.snap.worlds.length === 0 || !!worldBy(e.world_slug);
  return [...favs, ...recents].filter(listed);
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
      const era = w ? eraText(w) : "era unknown";
      const end = !w ? "not listed" : w.state === "offline" ? "offline" : w.players != null ? `${w.players} online` : "players unknown";
      const pin = e.fav
        ? `<span class="star" title="Favourite">★</span><button class="btn icon-btn mini" data-unfav="${esc(e.fav.id)}" title="Remove from favourites">✕</button>`
        : `<button class="btn icon-btn mini" data-fav="${i}" title="Add to favourites">☆</button>`;
      return `<div class="row ${i === ui.homeSel ? "sel" : ""} ${e.fav ? "fav" : ""}" data-home="${i}" data-dbl-home="${i}">
          ${bead(w?.state ?? "unknown")}
          <div class="grow"><div class="name">${esc(w?.name ?? e.world_slug)}</div>
          <div class="sub">${esc(e.account || "no account yet")} · ${esc(clientShort(e.client))} · ${esc(era)}</div></div>
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

// The emulators a player can name for a server they add, by the backend's names: Unknown, the
// default, then Empyrean first.
const EMULATORS = [["unknown", "Unknown"], ["empyrean", "Empyrean"], ["ace", "ACE"], ["classic_ace", "ClassicACE"], ["gdle", "GDLE"]];

const isCustom = (w) => ui.snap.state.custom_worlds.some((c) => c.slug === w.slug);

// The three sections of the list, in order: the player's own servers, the listed worlds that are
// up (or not yet asked), and the listed worlds that are down.
const SECTIONS = [
  ["custom", "CUSTOM"],
  ["online", "ONLINE"],
  ["offline", "OFFLINE"],
];
const sectionOf = (w) => (isCustom(w) ? "custom" : w.state === "offline" ? "offline" : "online");

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
    .sort((a, b) => (favs.has(b.slug) - favs.has(a.slug)) || (rank[a.state] - rank[b.state]) || ((b.players ?? 0) - (a.players ?? 0)) || a.name.localeCompare(b.name));
}

// A chevron on the same pixel grid as the rest of the chrome: "<" goes back.
const CHEVRON_LEFT = `<svg viewBox="0 0 8 12" fill="currentColor" aria-hidden="true"><rect x="5" y="1" width="2" height="2"/><rect x="3" y="3" width="2" height="2"/><rect x="1" y="5" width="2" height="2"/><rect x="3" y="7" width="2" height="2"/><rect x="5" y="9" width="2" height="2"/></svg>`;
// A bin, for taking a server of your own off the list.
const TRASH_ICON = `<svg viewBox="0 0 12 12" fill="currentColor" aria-hidden="true"><rect x="4" y="0" width="4" height="1"/><rect x="1" y="2" width="10" height="1"/><rect x="2" y="4" width="1" height="8"/><rect x="9" y="4" width="1" height="8"/><rect x="2" y="11" width="8" height="1"/><rect x="5" y="5" width="1" height="5"/><rect x="7" y="5" width="1" height="5"/></svg>`;

function playersLabel(w) {
  return w.state === "offline" ? "offline" : w.players != null ? `${w.players}` : "unknown";
}

// An open row: what the world says about itself, its era and systems, and the way to its page.
function worldDetail(w) {
  const links = [["Website", w.links.website], ["Discord", w.links.discord], ["Rules", w.links.rules]].filter(([, u]) => u);
  // The systems, shown for a world that names its era or systems; nothing here is chosen.
  const named = w.era_source === "world" || w.features_source === "world";
  const on = worldFeatures(w);
  const boxes = named
    ? ui.snap.features.map((f) => `<label class="check small"><input type="checkbox" ${on[f.name] ? "checked" : ""} disabled>${esc(f.label)}</label>`).join("")
    : "";
  const said = !named ? `This world does not name its era: it plays ${eraText(w)}.`
    : w.era_source !== "world" ? `This world says the systems it has, and plays ${eraText(w)}.`
    : w.features_source === "world" ? "This world says its era and the systems it has."
    : "This world says its era, and has that era's systems.";
  const facts = [
    ["EMULATOR", emulatorLabel(w)],
    ["STATUS", w.development_status ?? "unknown"],
    ["RULES", w.ruleset ?? "unknown"],
    ["PLAYERS", playersLabel(w)],
  ];
  return `<tr class="detail" data-stop><td></td><td colspan="6">
      <div class="detail-body">
        <div class="detail-top">
          <p class="${w.description ? "" : "muted"}">${esc(w.description ?? "No description.")}</p>
          <button class="play mini" data-open-world="${esc(w.slug)}" title="Play on ${esc(w.name)}">PLAY</button>
        </div>
        <div class="strip">${facts.map(([k, v]) => `<span><b>${k}</b>${esc(v)}</span>`).join("")}</div>
        ${links.length ? `<div class="btns">${links.map(([l, u]) => `<button class="btn" data-url="${esc(u)}">${l}</button>`).join("")}</div>` : ""}
        <div class="era-line"><b>ERA</b><span>${esc(eraText(w))}</span></div>
        <p class="small muted">${esc(said)}</p>
        ${boxes ? `<div class="features">${boxes}</div>` : ""}
      </div></td></tr>`;
}

function worldRow(w) {
  const open = ui.expanded === w.slug;
  const trash = isCustom(w) ? `<button class="btn icon-btn trash" data-trash="${esc(w.slug)}" title="Remove ${esc(w.name)}">${TRASH_ICON}</button>` : "";
  return `<tr data-world="${esc(w.slug)}" class="${w.state === "offline" ? "dim" : ""} ${open ? "open" : ""}">
      <td>${bead(w.state)}</td><td class="name">${esc(w.name)}</td>
      <td>${esc(emulatorLabel(w))}</td><td>${esc(eraText(w))}</td>
      <td>${esc(w.ruleset ?? "")}</td><td>${esc(playersLabel(w))}</td><td class="end">${trash}</td></tr>`
    + (open ? worldDetail(w) : "");
}

function worlds() {
  const rows = visibleWorlds();
  const s = ui.snap;
  const searching = ui.search.trim() !== "";
  let body;
  if (rows.length === 0) {
    const msg = s.list_state === "loading" ? "Fetching the world list..."
      : s.list_state === "unavailable" && s.worlds.length === 0 ? `The world list is unavailable: ${s.list_error ?? ""}`
      : s.worlds.length === 0 ? "No worlds are listed." : "No world matches these filters.";
    body = `<p class="muted">${esc(msg)}</p>`;
  } else {
    // The columns have fixed widths, so folding a section never moves them.
    body = `<table class="worlds"><colgroup><col class="c-dot"><col class="c-world"><col class="c-emu"><col class="c-era"><col class="c-rules"><col class="c-players"><col class="c-end"></colgroup>
      <thead><tr><th></th><th>WORLD</th><th>EMULATOR</th><th>ERA</th><th>RULES</th><th>PLAYERS</th><th></th></tr></thead><tbody>` +
      SECTIONS.map(([key, label]) => {
        const these = rows.filter((w) => sectionOf(w) === key);
        if (these.length === 0) return "";
        // A search shows every match, whether its section is folded or not.
        const open = searching || !ui.folded[key];
        return `<tr class="section ${open ? "" : "shut"}" data-section="${key}"><td colspan="7"><span class="fold">${open ? "▾" : "▸"}</span>${label}<span class="muted small">${these.length}</span></td></tr>`
          + (open ? these.map(worldRow).join("") : "");
      }).join("") + `</tbody></table>`;
  }
  const fetched = s.list_fetched_at ? new Date(s.list_fetched_at * 1000) : null;
  const stale = s.list_state === "unavailable" && s.worlds.length
    ? `<p class="small warn">${esc(s.list_error ?? "")} — showing the list from ${esc(fetched ? fetched.toLocaleString() : "before")}</p>` : "";
  const a = ui.addServer;
  const emulatorOpts = EMULATORS.map(([v, l]) => `<option value="${v}" ${a?.emulator === v ? "selected" : ""}>${l}</option>`).join("");
  const rulesOpts = [["", "Unknown"], ["PvE", "PvE"], ["PvP", "PvP"]].map(([v, l]) => `<option value="${v}" ${a?.ruleset === v ? "selected" : ""}>${l}</option>`).join("");
  const form = a
    ? `<section class="panel pending"><header><h1>ADD A WORLD</h1></header>
        ${a.fromLink ? `<p class="small muted">From a link a friend sent. Check it, then Add.</p>` : ""}
        <div class="form">
          <label class="lab" for="srv-name">NAME</label><input id="srv-name" class="field" autocomplete="off" placeholder="optional" value="${esc(a.name)}">
          <label class="lab" for="srv-host">HOST</label><input id="srv-host" class="field" autocomplete="off" placeholder="play.example.org or 203.0.113.7" value="${esc(a.host)}">
          <label class="lab" for="srv-port">PORT</label><input id="srv-port" class="field" autocomplete="off" inputmode="numeric" style="max-width:140px" value="${esc(a.port)}">
          <label class="lab" for="srv-emulator">EMULATOR</label><select id="srv-emulator" class="field" style="max-width:220px">${emulatorOpts}</select>
          <label class="lab" for="srv-ruleset">RULES</label><select id="srv-ruleset" class="field" style="max-width:220px">${rulesOpts}</select>
          ${a.error ? `<p class="note bad">${esc(a.error)}</p>` : ""}
        </div>
        <div class="btns end" style="margin-top:12px"><button class="btn" data-act="server-cancel">Cancel</button><button class="btn" data-act="server-add">Add</button></div>
      </section>`
    : "";
  return `<div class="column wide" style="height:100%">
      ${form}
      <section class="panel grow"><header><h1>WORLDS</h1><span class="muted small">${rows.length} worlds</span>
        <button class="btn" data-act="server-open" title="Add a world by its address">+ Add world</button>
        <button class="btn icon-btn" data-act="refresh" title="Fetch the world list again">↻</button></header>
        <div class="btns" style="margin-bottom:12px">
          <input id="search" class="field" autocomplete="off" style="max-width:360px" placeholder="search" value="${esc(ui.search)}">
          <div class="toggle" role="group" aria-label="Rules">${FILTERS.map(([k, l]) => `<button class="${ui.filter === k ? "on" : ""}" data-filter="${k}" aria-pressed="${ui.filter === k}">${l}</button>`).join("")}</div>
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
  const validClassic = (id) => view.classic_sets.some((s) => s.id === id);
  const client = [preset?.client, p.client, view.default_client].find((k) => k && offered(k)) ?? view.clients[0]?.kind ?? null;
  // Each kind starts on what was played here last, else its own default, else (for the Modern
  // kind, where a world's own copy may be the only one) the first there is.
  const fallback = view.dat_sets.find((s) => s.origin.kind === "shared") ?? view.dat_sets[0];
  const set = [preset?.dat_set_id, p.dat_set_id].find(validSet) ?? fallback?.id ?? null;
  const classicDefault = view.classic_sets.find((s) => s.origin.kind === "shared");
  const classic = [preset?.classic_set_id, p.classic_set_id].find(validClassic) ?? classicDefault?.id ?? null;
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
      classic_set_id: classic,
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
    classic_set_id: f.client === "dereth" ? f.classic_set_id : null,
  };
}

// Which kind of data files the world's era needs: the Classic pair for an era before Throne of
// Destiny, the Modern files for any other, and for an era nobody has named.
function requiredKind(world) {
  // A world that names the set it is drawn from needs that one, whatever its era.
  if (world.world_base === "modern" || world.world_base === "classic") return world.world_base;
  return ui.snap.eras.find((e) => e.name === world.era)?.needs ?? "modern";
}

// Whether the Dereth client has the data files the world's era needs: the Modern files always,
// and the Classic pair too for an era before Throne of Destiny.
function hasRequiredSet() {
  const w = ui.world;
  const world = worldBy(w.view.world.slug) ?? w.view.world;
  if (w.form.client !== "dereth") return true;
  return !!w.form.dat_set_id && (requiredKind(world) !== "classic" || !!w.form.classic_set_id);
}

// Whether PLAY can go, and what the account field warns about: redrawn in place while typing, so
// the field being typed in is never replaced under the player.
function worldCanPlay() {
  const w = ui.world;
  const world = worldBy(w.view.world.slug) ?? w.view.world;
  return !!formChoice() && !!world.endpoint && hasRequiredSet();
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

function worldPage() {
  const w = ui.world;
  if (!w) return home();
  const world = worldBy(w.view.world.slug) ?? w.view.world;
  const f = w.form;

  const facts = [stateLabel(world), world.players != null ? `${world.players} players` : "players unknown", world.ruleset, eraText(world), emulatorLabel(world)].filter(Boolean);
  const links = [["Website", world.links.website], ["Discord", world.links.discord], ["Rules", world.links.rules]].filter(([, u]) => u);
  const custom = ui.snap.state.custom_worlds.some((c) => c.slug === world.slug);

  const accountField = f.adding
    ? `<div class="pair"><input id="acct-name" class="field" autocomplete="off" placeholder="account name" value="${esc(f.account)}">
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
  const needs = requiredKind(world);
  const option = (s, chosen) => `<option value="${esc(s.id)}" ${s.id === chosen ? "selected" : ""}>${esc(setName(s))}${s.origin.kind === "shared" ? "  (default)" : ""}</option>`;
  const none = (chosen) => `<option value="" ${chosen ? "" : "selected"}>None</option>`;
  const setOpts = none(f.dat_set_id) + w.view.dat_sets.map((s) => option(s, f.dat_set_id)).join("")
    + (w.view.offers_private_copy ? `<option value="__private">Create a private copy (1.4 GB)</option>` : "")
    + (world.dats.custom ? `<option value="__custom">Add this world's downloaded data files...</option>` : "");
  const classicOpts = none(f.classic_set_id) + w.view.classic_sets.map((s) => option(s, f.classic_set_id)).join("");
  const eraName = eraText(world);
  const dataNote = world.world_base === "modern" && world.dats.custom
    ? `${world.name} plays ${eraName} over its own Modern data files. Classic ones are optional, for the classic interface and looks.`
    : needs === "classic"
    ? `${eraName} is before Throne of Destiny: it needs Classic data files, which its world is drawn from, and Modern ones beside them for the interface.`
    : `${eraName} needs Modern data files. Classic ones are optional, for the classic interface and looks.`;
  const missing = f.client === "dereth" && !hasRequiredSet();
  // A world that plays with its own files, and none of the player's sets is them: where they come
  // from, and the way to point at them.
  const ownFiles = world.dats.custom && w.view.dat_sets.length === 0 && f.client === "dereth"
    ? `<p class="note warn">${esc(world.dats.custom.license_note ?? "This world plays with data files of its own.")}
         <button class="btn" data-url="${esc(world.dats.custom.url)}">Where to get them</button></p>`
    : "";
  const dataRow = f.client === "retail"
    ? `<label class="lab">DATA</label><div class="plain">The data files beside the retail client <span class="muted small">${esc(ui.snap.state.retail?.path ?? "")}</span></div>`
    : `<label class="lab" for="dats">MODERN DATA</label><select id="dats" class="field">${setOpts}</select>
       <label class="lab" for="classic">CLASSIC DATA</label><select id="classic" class="field">${classicOpts}</select>
       <p class="note ${missing ? "warn" : "muted"}">${esc(dataNote)}</p>${ownFiles}`;
  const remembers = ui.snap.vault_name === "memory only" ? "Remember until the launcher closes" : "Save password securely";

  return `<div class="column wide">
      <section class="panel"><header><button class="btn icon-btn back" data-go="worlds" title="Back to worlds">${CHEVRON_LEFT}</button><h1>${esc(world.name.toUpperCase())}</h1>
          ${links.map(([l, u]) => `<button class="btn" data-url="${esc(u)}">${l}</button>`).join("")}
          ${custom ? `<button class="btn" data-act="server-remove">Remove world</button>` : ""}</header>
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
            ${dataRow}
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
  const dereth = (d
    ? `<div class="row static"><div class="grow"><div class="name">${esc(installName(d))}</div><div class="sub">${esc(d.path)}</div></div></div>`
    : `<div class="row static"><div class="grow"><div class="sub warn">The Dereth client was not found beside the launcher.</div></div></div>`)
    + interfaceRow();
  const setRow = (d) => {
    const size = d.files.reduce((n, f) => n + f.size, 0);
    const pairBroken = d.kind === "classic" && !(d.files.some((f) => f.role === "portal") && d.files.some((f) => f.role === "cell"));
    const facts = [d.created_by_launcher ? gb(size) : d.files.every((f) => f.read_only) ? "read-only" : null, d.last_patched_by_server && "patched by its world", d.files.some((f) => f.error) && "some files unreadable", pairBroken && "portal.dat or cell.dat is missing"].filter(Boolean);
    const isDefault = d.origin.kind === "shared";
    const defaultBtn = isDefault ? `<span class="tag">DEFAULT</span>`
      : d.origin.kind === "unassigned" ? `<button class="btn" data-default-set="${esc(d.id)}">Set default</button>` : "";
    return `<div class="row static"><div class="grow"><div class="name">${esc(setName(d))} <span class="muted small">${esc(facts.join("; "))}</span></div>
        <div class="sub">${esc(d.path)}</div></div>
        <div class="btns">${defaultBtn}${d.origin.kind === "world" ? `<button class="btn" data-reset-set="${esc(d.id)}">Reset</button>` : ""}<button class="btn" data-delete-set="${esc(d.id)}">${d.created_by_launcher ? "Delete" : "Remove"}</button></div></div>`;
  };
  const modern = s.dat_sets.filter((d) => d.kind !== "classic").map(setRow).join("")
    || `<p class="muted">No Modern data files yet. Add a folder that holds the client_*.dat files.</p>`;
  const classic = s.dat_sets.filter((d) => d.kind === "classic").map(setRow).join("")
    || `<p class="muted">No Classic data files yet. Add a folder that holds portal.dat and cell.dat.</p>`;
  // What a game the launcher started shows on Discord, on unless the player turns it off.
  const discord = `<div class="row static"><div class="grow">
      <label class="check"><input type="checkbox" id="discord-presence" ${s.settings.discord_presence !== false ? "checked" : ""}>Show what I'm playing on Discord</label>
      <div class="sub">While a game the launcher started is running, Discord on this computer shows its world, its client and its era.</div></div></div>`;

  let pending = "";
  if (ui.addFind) {
    const { purpose, find: f } = ui.addFind;
    // The retail client is acclient.exe with the Modern files beside it; data files are either kind.
    const ok = purpose === "retail" ? !!(f.retail && f.dats) : !!(f.dats || f.classic);
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
          <h2>MODERN DATA SETS</h2><p class="muted small">The files from a modern install: client_*.dat.</p><div class="list">${modern}</div>
          <h2>CLASSIC DATA SETS</h2><p class="muted small">The files from older installs: portal.dat and cell.dat.</p><div class="list">${classic}</div>
          <h2>DISCORD</h2><div class="list">${discord}</div></div>
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
    } else if (purpose === "retail") {
      // Only the retail client needs acclient.exe; a folder of data files never does.
      lines.push(`<p class="warn">No acclient.exe in this folder.</p>`);
    }
  }
  if (f.dats) {
    const it = setIterations(f.dats);
    lines.push(`<p><span class="${isEndOfRetail(it) ? "good" : "warn"}">${isEndOfRetail(it) ? "MODERN" : "MODERN, OLDER"}</span> — portal ${it.portal ?? "-"}, cell ${it.cell ?? "-"}, local ${it.local ?? "-"}, highres ${it.highres ?? "-"}${isEndOfRetail(it) ? "" : " (from before the final patch)"}</p>`);
  }
  if (f.classic) {
    const it = setIterations(f.classic);
    lines.push(`<p><span class="good">CLASSIC</span> — portal ${it.portal ?? "-"}, cell ${it.cell ?? "-"}</p>`);
  }
  if (!f.dats && !f.classic && !f.error) {
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

// ----- plugins -------------------------------------------------------------------------------------

// Plugins for the Dereth client, from plugin sources: each source is a list (`plugins.json`) of
// plugins at their latest release. Dereth's own source is there from the start; others are added
// by their list's address, once the player confirms it. The page has four sections, each folding
// as the Worlds list's do: the plugins installed, the ones the sources offer that are not, the
// sources, and Make Your Own, which makes a new plugin. A plugin's row opens in place, one at a
// time, to everything known about it. An
// installed plugin updates only from its own source. Plugins are experimental: an installed plugin
// is off in the client until the player turns it on there. Installed also lists the plugins in
// folders on this computer, marked Local: the client loads them as they are on disk, never updated,
// in place of an installed plugin of the same id. A folder is a plugin's own or a plugin repository
// (the folders directly in it hold plugins), added in one step; taking it off the list deletes
// nothing.

// Which plugins update on their own, in the order the page offers them.
const PLUGIN_UPDATES = [["none", "None"], ["all", "All"], ["first_party", "Only Dereth"]];

// A plugin's zip's size in words.
const kb = (n) => (n >= 1048576 ? mb(n) : `${Math.max(1, Math.round(n / 1024))} KB`);

// What a release says it uses, its permissions and the web domains it reaches; empty when none.
const pluginUses = (permissions, domains) => [...permissions, ...domains].join(", ");

// A section of the page: its heading, which folds and unfolds it with how many it holds, and what
// it holds while it is open.
function pluginSection(key, label, count, body) {
  const open = !ui.plugin.folded[key];
  return `<h2 class="fold-head" data-plugin-fold="${key}"><span class="fold">${open ? "▾" : "▸"}</span>${label}<span class="muted small">${count}</span></h2>`
    + (open ? body : "");
}

// A plugin's row: its name, version and source, and the first line of what it does, with `extra`
// under it and its `buttons`, if any; open, `detail` under the row in place of that first line.
function pluginRow(key, p, buttons, extra, detail) {
  const open = ui.plugin.open === key;
  const row = `<div class="row plugin-row" data-plugin-row="${esc(key)}"><div class="grow">
      <div class="name">${esc(p.name)} ${esc(p.version)} <span class="muted small">${esc(p.source_name)}</span></div>
      ${p.description && !open ? `<div class="sub">${esc(p.description.split("\n")[0])}</div>` : ""}${extra}</div>
      ${buttons ? `<div class="btns">${buttons}</div>` : ""}</div>`;
  return open ? `<div class="plugin open">${row}<div class="plugin-detail" data-stop>${detail}</div></div>` : row;
}

// Everything known of a release, for an open row: what it does in full, its facts, what it uses
// and what changed in it, and its homepage.
function pluginDetail(p) {
  const facts = [["ID", p.id], ["VERSION", p.version], ["SOURCE", p.source_name], ["AUTHORS", p.authors.join(", ")],
    ["RELEASED", p.released ? longDate(p.released) : ""], ["SIZE", p.size != null ? kb(p.size) : ""]].filter(([, x]) => x);
  return `<p class="desc ${p.description ? "" : "muted"}">${esc(p.description || "No description.")}</p>
    <div class="strip">${facts.map(([k, x]) => `<span><b>${k}</b>${esc(x)}</span>`).join("")}</div>
    <div class="said"><b>USES</b>${esc(pluginUses(p.permissions, p.domains) || "no permissions")}</div>
    ${p.notes ? `<div class="said"><b>WHAT CHANGED</b>${esc(p.notes)}</div>` : ""}
    ${p.homepage ? `<div class="btns"><button class="btn" data-url="${esc(p.homepage)}">Homepage</button></div>` : ""}`;
}

// An installed plugin's update, on its row: the version, and what it asks for that the installed
// one did not.
function updateLine(u) {
  if (u.needs_newer_dereth) return `<div class="sub warn">${esc(u.to)} needs a newer Dereth.</div>`;
  const asks = [...u.new_permissions, ...u.new_domains];
  return `<div class="sub">Update ${esc(u.to)} is ready.${asks.length ? ` <span class="warn">It also asks for ${esc(asks.join(", "))}.</span>` : ""}</div>`;
}

function installedRow(x, busy) {
  const u = x.update;
  const local = x.local_in_its_place ? `<div class="sub">The local one is loaded in its place.</div>` : "";
  const buttons = `${u && !u.needs_newer_dereth ? `<button class="btn" data-plugin-update="${esc(x.id)}" ${busy ? "disabled" : ""}>Update</button>` : ""}
    <button class="btn" data-plugin-remove="${esc(x.id)}">Remove</button>`;
  let detail = pluginDetail(x);
  if (x.source_removed) detail += `<p class="small warn">You removed its source, so it gets no updates.</p>`;
  if (u) {
    const facts = [["UPDATE", u.to], ["RELEASED", u.released ? longDate(u.released) : ""], ["SIZE", u.size != null ? kb(u.size) : ""]].filter(([, v]) => v);
    const asks = [...u.new_permissions, ...u.new_domains];
    detail += `<div class="plugin-update"><div class="strip">${facts.map(([k, v]) => `<span><b>${k}</b>${esc(v)}</span>`).join("")}</div>
      ${asks.length ? `<div class="said warn"><b>ALSO ASKS FOR</b>${esc(asks.join(", "))}</div>` : ""}
      ${u.notes ? `<div class="said"><b>WHAT CHANGED</b>${esc(u.notes)}</div>` : ""}</div>`;
  }
  return pluginRow(`i:${x.id}`, x, buttons, (u ? updateLine(u) : "") + local, detail);
}

// A plugin in a folder on this computer, marked Local, or a folder on the list that holds none,
// saying so. Its row has no buttons. Open, its location's line ends with Open, which shows the
// folder, and Remove, which takes it off the list and deletes nothing.
function localPluginRow(x) {
  const p = { ...x, source_name: "Local" };
  const line = x.problem ? `<div class="sub warn">${esc(x.problem)}</div>`
    : x.id_taken ? `<div class="sub warn">Not loaded: a plugin above has the same id.</div>`
    : x.needs_newer_dereth ? `<div class="sub warn">Needs a newer Dereth.</div>`
    : x.in_place_of_installed ? `<div class="sub">Loaded in place of the installed one.</div>` : "";
  // A folder Make Your Own wrote in, whose copy of dereth-plugin could not be brought up to date.
  const tool = x.tool_problem ? `<div class="sub warn">${esc(x.tool_problem)}</div>` : "";
  const location = `<div class="plugin-location"><b>LOCATION</b><span class="path">${esc(x.folder)}</span>
      <span class="btns"><button class="btn" data-plugin-open="${esc(x.folder)}">Open</button><button class="btn" data-plugin-folder-remove="${esc(x.listed)}">Remove</button></span></div>`;
  const detail = (x.problem ? "" : pluginDetail(p)) + location;
  return pluginRow(`l:${x.folder}`, p, "", line + tool, detail);
}

function availableRow(a, busy) {
  const button = a.state === "install" ? `<button class="btn" data-plugin-install="${esc(a.source)}" data-plugin-id="${esc(a.id)}" ${busy ? "disabled" : ""}>Install</button>`
    : a.state === "needs_newer_dereth" ? `<span class="muted small">Needs a newer Dereth</span>`
    : `<span class="muted small">Only from Dereth</span>`;
  return pluginRow(`a:${a.source} ${a.id}`, a, button, "", pluginDetail(a));
}

function plugins() {
  const p = ui.plugin;
  const v = ui.snap.plugins;
  const busy = !!p.busy;
  const installed = v.installed.map((x) => installedRow(x, busy)).join("") + v.local.map(localPluginRow).join("")
    || `<p class="muted empty">No plugins installed. Install one from the list below.</p>`;
  // A folder on this computer, chosen with the system's picker and added at once.
  const addFolder = `<div class="plugin-line add"><button class="btn" data-act="plugin-folder-add" ${busy ? "disabled" : ""}>Add Plugin Folder</button></div>`;
  const updateOpts = PLUGIN_UPDATES.map(([val, l]) => `<option value="${val}" ${v.auto_update === val ? "selected" : ""}>${l}</option>`).join("");
  const setting = `<div class="row static plugin-setting"><div class="grow"><div class="name">Automatically update plugins</div>
      <div class="sub">Plugin updates that ask for additional permissions will never automatically install.</div></div>
      <div class="btns"><select id="plugin-updates" class="field mini" aria-label="Automatically update plugins">${updateOpts}</select></div></div>`;

  const q = p.search.trim().toLowerCase();
  const shown = v.available.filter((a) => !q || [a.name, a.id, a.description, a.source_name].some((t) => t.toLowerCase().includes(q)));
  const listed = v.sources.some((s) => s.plugins > 0);
  const available = shown.map((a) => availableRow(a, busy)).join("")
    || `<p class="muted empty">${q ? "No plugin matches." : listed ? "Every plugin the sources list is installed." : "No plugins listed yet. Refresh to read the sources."}</p>`;
  const search = `<div class="plugin-line"><input id="plugin-search" class="field mini-field" type="text" placeholder="Search" spellcheck="false" autocomplete="off" value="${esc(p.search)}"></div>`;

  // A source on one line: its name, how many plugins it lists and its address; Dereth's own stays.
  const sources = v.sources.map((s) => {
    const skipped = s.skipped.length ? ` · <span class="warn" title="${esc(s.skipped.join("\n"))}">${s.skipped.length} could not be read</span>` : "";
    return `<div class="row static source-row"><div class="grow">
        <div class="line"><span class="name">${esc(s.name)}</span><span class="muted small">${s.plugins} plugin${s.plugins === 1 ? "" : "s"}${skipped}</span><span class="muted small addr" title="${esc(s.url)}">${esc(s.url)}</span></div>
        ${s.error ? `<div class="sub warn" title="${esc(s.error)}">Could not be read: ${esc(s.error)}</div>` : ""}</div>
        ${s.first_party ? "" : `<div class="btns"><button class="btn" data-plugin-source-remove="${esc(s.url)}">Remove</button></div>`}</div>`;
  }).join("");
  const add = `<div class="plugin-line add"><input id="plugin-source" class="field mini-field" type="text" placeholder="https://…/plugins.json" spellcheck="false" autocomplete="off" aria-label="A source's address" value="${esc(p.add)}">
      <button class="btn" data-act="plugin-source-add" ${busy || !p.add.trim() ? "disabled" : ""}>Add source</button></div>
    ${p.confirm ? `<div class="plugin-confirm"><span>Dereth does not review plugins from other sources.</span>
      <button class="btn mini-btn" data-act="plugin-source-confirm" ${busy ? "disabled" : ""}>Add</button><button class="btn mini-btn" data-act="plugin-source-cancel">Cancel</button></div>` : ""}`;

  // A new plugin: its name and, for the links and workflows, its GitHub repository. Create asks
  // for an empty folder, where it writes the repository the author's tool writes, or a plugin
  // repository, which gets the new plugin's folder; the line under it says what the last one made.
  const m = p.make;
  const build = `<div class="plugin-line"><input id="plugin-make-name" class="field mini-field" type="text" placeholder="Name" spellcheck="false" autocomplete="off" aria-label="The plugin's name" value="${esc(m.name)}">
      <input id="plugin-make-github" class="field mini-field" type="text" placeholder="GitHub username/repo (optional)" spellcheck="false" autocomplete="off" aria-label="Its GitHub repository" value="${esc(m.github)}">
      <button class="btn" data-act="plugin-create" ${busy || !m.name.trim() ? "disabled" : ""}>Create</button></div>
    ${m.made ? `<div class="plugin-made"><span title="${esc(m.made.folder)}">${esc(m.made.said)}</span>
      <button class="btn mini-btn" data-act="plugin-open-made">Open folder</button></div>` : ""}`;

  return `<div class="column wide" style="height:100%">
      <section class="panel grow"><header><h1>PLUGINS</h1><span class="tag">EXPERIMENTAL</span><button class="btn" data-act="plugins-refresh" ${busy ? "disabled" : ""}>Refresh</button></header>
        <div class="scroll plugins-page">
          ${pluginSection("installed", "INSTALLED", v.installed.length + v.local.length, `<div class="list">${installed}</div>${addFolder}${setting}`)}
          ${pluginSection("available", "AVAILABLE", shown.length, `${search}<div class="list">${available}</div>`)}
          ${pluginSection("sources", "SOURCES", v.sources.length, `<div class="list">${sources}</div>${add}`)}
          ${pluginSection("build", "MAKE YOUR OWN", "", build)}
        </div>
      </section>
    </div>`;
}

// One of the Plugins page's slow calls (it fetches), with the page busy, and how it went said.
// Answers whether it went through.
async function pluginFetch(cmd, args) {
  ui.plugin.busy = cmd;
  render();
  let ok = false;
  try {
    const said = await api(cmd, args);
    toast(String(said || (cmd === "refresh_plugins" ? "Nothing new." : "Done.")).replace(/\n/g, " "));
    ok = true;
  } catch (e) {
    toast(String(e), true);
  }
  ui.plugin.busy = null;
  await pull();
  render();
  return ok;
}

// Make a new plugin from the name and the repository typed in: the launcher checks them, asks for
// an empty folder or a plugin repository and writes the template there. Cancelling the folder
// changes nothing.
async function createPlugin() {
  const m = ui.plugin.make;
  const name = m.name.trim();
  if (!name || ui.plugin.busy) return;
  ui.plugin.busy = "create_plugin";
  render();
  try {
    const made = await api("create_plugin", { name, github: m.github.trim() || null });
    if (made) Object.assign(m, { name: "", github: "", made });
  } catch (e) {
    toast(String(e), true);
  }
  ui.plugin.busy = null;
  render();
}

// Add Plugin Folder: choose a folder on this computer with the system's picker, and it is added at
// once, the line saying what it added or why it could not; cancelling the picker changes nothing.
async function addPluginFolder() {
  if (ui.plugin.busy) return;
  let folder = null;
  try {
    folder = await api("pick_plugin_folder");
  } catch (e) {
    toast(String(e), true);
  }
  if (folder) await pluginFetch("add_plugin_folder", { folder });
}

// Add the source typed in. One other than Dereth's is added once the player confirms it, on the
// line under the address.
async function addPluginSource(confirmed) {
  const url = ui.plugin.add.trim();
  if (!url) return;
  if (!confirmed && url !== ui.snap.plugins.first_party) {
    ui.plugin.confirm = true;
    render();
    return;
  }
  ui.plugin.confirm = false;
  if (await pluginFetch("add_plugin_source", { url })) ui.plugin.add = "";
  render();
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
      <div class="btns" style="margin-top:10px"><button class="btn" data-act="fr-continue" ${f.retail || f.dats || f.classic ? "" : "disabled"}>Continue</button>
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

// ----- the interface ------------------------------------------------------------------------------

// Whether the first start's question is open: the Dereth client keeps preferences here, they name
// no interface yet, and the question was not set aside this session.
function interfaceAsked() {
  const i = ui.snap?.interface;
  return !!i && !i.chosen && !ui.iface.dismissed;
}

// Whether a Dereth client the launcher started is running: it saves its own interface when it
// closes, so the launcher does not change it under it.
const derethRunning = () => ui.snap.running.some((r) => r.kind === "dereth");

// A picture of each interface: the screen in miniature on a 64 by 40 grid, drawn as pixel art like
// the rest of the chrome, with each part where that interface puts it over the world.
const px = (x, y, w, h, fill) => `<rect x="${x}" y="${y}" width="${w}" height="${h}" fill="${fill}"/>`;
const PIC = {
  gold: "#b58a3c", bright: "#e9c77a", dark: "#6b4e22", ink: "#0c0a10", stone: "#211c29", glass: "rgba(12,10,16,0.74)",
  bronze: "#3a2f22", parchment: "#7d6a48", line: "#a99f8b", health: "#d8584a", stamina: "#e3b344", mana: "#4f7fd0", friend: "#5fcf55",
};
// A round dial (a radar or a minimap) centred on (cx, cy): a gold ring around a dark face, the
// player a bright dot in the middle.
function dial(cx, cy, face) {
  const rows = [[-5, 2], [-4, 4], [-3, 5], [-2, 5], [-1, 5], [0, 5], [1, 5], [2, 5], [3, 5], [4, 4], [5, 2]];
  const disc = (inset, fill) => rows.filter(([dy]) => Math.abs(dy) <= 5 - inset)
    .map(([dy, half]) => px(cx - Math.max(0, half - inset), cy + dy, Math.max(0, half - inset) * 2 + 1, 1, fill)).join("");
  return disc(0, PIC.gold) + disc(1, face) + px(cx, cy, 1, 1, PIC.bright);
}
// A window: a gold line around a filled body.
const pane = (x, y, w, h, fill) => px(x, y, w, h, PIC.gold) + px(x + 1, y + 1, w - 2, h - 2, fill);
// Lines of text in a window.
const textLines = (x, y, widths, fill, last = fill) => widths.map((w, i) => px(x, y + i * 2, w, 1, i === widths.length - 1 ? last : fill)).join("");
// The world every picture stands over: sky, hills and ground, and the character in the middle.
const WORLD = px(0, 0, 64, 14, "#2a3a52") + px(0, 14, 64, 10, "#38506a")
  + px(0, 19, 15, 5, "#2f4a3a") + px(4, 17, 8, 2, "#2f4a3a") + px(40, 20, 24, 4, "#2f4a3a") + px(47, 18, 10, 2, "#2f4a3a")
  + px(0, 24, 64, 8, "#3b5a34") + px(0, 32, 64, 8, "#2e4829") + px(31, 21, 2, 7, PIC.ink);
const INTERFACE_PICTURES = {
  // Horizon: the target bar at the top, the minimap at the top right, the party list at the left,
  // the log window at the bottom left, the parameter bar over two hotbars of twelve and the EXP bar
  // along the foot, the main menu at the bottom right, and nameplates over the creatures.
  Horizon: px(22, 2, 20, 3, PIC.gold) + px(23, 3, 18, 1, PIC.health)
    + px(45, 2, 2, 2, PIC.mana) + px(48, 2, 2, 2, PIC.friend)
    + dial(57, 7, "#16222c")
    + [8, 6, 7].map((w, i) => px(2, 10 + i * 4, 10, 3, PIC.glass) + px(3, 11 + i * 4, w, 1, PIC.friend)).join("")
    + px(2, 27, 12, 11, PIC.glass) + px(2, 27, 12, 1, PIC.dark) + textLines(3, 30, [9, 6, 10, 5], PIC.line, PIC.bright)
    + px(42, 23, 3, 3, PIC.ink) + px(40, 21, 7, 1, PIC.stamina)
    + px(17, 29, 10, 1, PIC.health) + px(28, 29, 10, 1, PIC.mana) + px(39, 29, 10, 1, PIC.stamina)
    + [31, 34].map((y) => Array.from({ length: 12 }, (_, i) => px(15 + i * 3, y, 2, 2, PIC.gold)).join("")).join("")
    + px(15, 38, 35, 1, PIC.dark) + px(15, 38, 22, 1, PIC.bright)
    + px(53, 35, 9, 3, PIC.ink) + [54, 56, 58, 60].map((x) => px(x, 36, 1, 1, PIC.bright)).join(""),
  // Modern: windows over the whole world: the vitals at the top left, the radar at the top right,
  // the chat at the bottom left, an open panel at the right and the toolbar under it.
  Modern: pane(1, 1, 15, 9, PIC.stone) + px(3, 3, 11, 1, PIC.health) + px(3, 5, 9, 1, PIC.stamina) + px(3, 7, 10, 1, PIC.mana)
    + dial(57, 7, "#1d4a25")
    + pane(1, 27, 26, 12, PIC.stone) + textLines(3, 29, [20, 14, 18, 11], PIC.line, PIC.bright)
    + pane(46, 14, 17, 19, PIC.stone)
    + [17, 20, 23, 26, 29].map((y) => [48, 51, 54, 57].map((x) => px(x, y, 2, 2, PIC.dark)).join("")).join("")
    + pane(40, 34, 23, 5, PIC.stone) + Array.from({ length: 7 }, (_, i) => px(42 + i * 3, 36, 2, 1, PIC.bright)).join(""),
  // Classic: the world in a frame: the vitals along the top, the radar and the side panel in the
  // column at the right, the chat under the world and the toolbar under the column.
  Classic: px(0, 0, 64, 2, PIC.ink) + px(1, 0, 12, 2, PIC.health) + px(14, 0, 12, 2, PIC.stamina) + px(27, 0, 12, 2, PIC.mana)
    + [55, 58, 61].map((x) => px(x, 0, 2, 2, PIC.gold)).join("")
    + px(40, 2, 24, 32, PIC.bronze) + dial(51, 7, "#1d4a25")
    + px(41, 14, 22, 19, PIC.parchment) + textLines(43, 16, [16, 12, 15, 9, 14, 11, 13, 8], PIC.ink)
    + px(0, 33, 40, 7, PIC.ink) + textLines(2, 34, [30, 22, 26], PIC.line, PIC.bright)
    + px(40, 34, 24, 6, PIC.bronze) + Array.from({ length: 7 }, (_, i) => px(42 + i * 3, 36, 2, 2, PIC.gold)).join(""),
};
function interfacePicture(name) {
  return `<svg class="pic" viewBox="0 0 64 40" shape-rendering="crispEdges" aria-hidden="true">${WORLD}${INTERFACE_PICTURES[name] ?? ""}</svg>`;
}

// The interface picked on the question: the player's pick, else the one saved, else the first
// offered (Horizon).
function pickedInterface() {
  const i = ui.snap.interface;
  return ui.iface.pick ?? i?.chosen ?? i?.offers[0]?.name ?? null;
}

// The question: which interface the Dereth client plays in. On the first start it opens by itself;
// from the Library it changes the one saved.
function interfacePage() {
  const i = ui.snap.interface;
  if (!i) return home();
  const pick = pickedInterface();
  const fromLibrary = ui.iface.fromLibrary;
  const cards = i.offers.map((o) => `<button class="iface ${o.name === pick ? "on" : ""}" data-iface="${esc(o.name)}" role="radio" aria-checked="${o.name === pick}">
      ${interfacePicture(o.name)}<span class="iname">${esc(o.name.toUpperCase())}</span><span class="iline">${esc(o.line)}</span></button>`).join("");
  const noClassic = pick === "Classic" && !ui.snap.state.dat_sets.some((d) => d.kind === "classic");
  const back = fromLibrary ? `<button class="btn icon-btn back" data-act="iface-cancel" title="Back to the library">${CHEVRON_LEFT}</button>` : "";
  return `<div class="column wide"><section class="panel">
      <header>${back}<h1>${fromLibrary ? "INTERFACE" : "CHOOSE YOUR INTERFACE"}</h1></header>
      <p style="font-size:21px">Which interface would you like to play Dereth in?</p>
      <div class="ifaces" role="radiogroup" aria-label="Interface">${cards}</div>
      ${noClassic ? `<p class="small warn">There are no Classic data files yet: add portal.dat and cell.dat in the Library before playing in the classic interface.</p>` : ""}
      <p class="muted small">You can change it whenever you like in the game's options, or here in the Library.</p>
      <div class="btns end" style="margin-top:12px">${fromLibrary ? `<button class="btn" data-act="iface-cancel">Cancel</button>` : ""}
        <button class="btn" data-act="iface-save" ${fromLibrary && derethRunning() ? `disabled title="Close the Dereth client first"` : ""}>${fromLibrary ? "Save" : "Continue"}</button></div>
    </section></div>`;
}

// The Library's line for the interface the Dereth client starts in, with the way to change it.
function interfaceRow() {
  const i = ui.snap.interface;
  if (!i) return "";
  const running = derethRunning();
  const sub = running ? "Close the Dereth client to change it here; its own options change it while it runs."
    : "The interface the Dereth client starts in. Its own options change it too.";
  return `<div class="row static"><div class="grow"><div class="name">Interface: ${esc(i.chosen ?? "not chosen yet")}</div>
      <div class="sub">${esc(sub)}</div></div>
      <div class="btns"><button class="btn" data-act="iface-change" ${running ? "disabled" : ""}>Change</button></div></div>`;
}

// Save the interface picked, and go back where the question came from. On the first start, an
// answer that could not be saved is not asked again until the launcher starts again.
async function saveInterface() {
  const name = pickedInterface();
  const fromLibrary = ui.iface.fromLibrary;
  try {
    await api("set_interface", { name });
  } catch (e) {
    toast(String(e), true);
    if (fromLibrary) return;
    ui.iface.dismissed = true;
  }
  ui.iface = { ...ui.iface, pick: null, fromLibrary: false };
  ui.screen = fromLibrary ? "library" : "home";
  lastSnap = "";
  await pull();
}

// ----- events -------------------------------------------------------------------------------------

document.addEventListener("click", async (ev) => {
  const t = ev.target.closest("[data-srv-start],[data-srv-stop],[data-srv-prepare],[data-srv-play],[data-srv-open],[data-srv-publish],[data-srv-republish],[data-srv-unpublish],[data-srv-service],[data-srv-config],[data-srv-folder],[data-srv-delete],[data-confirm-delete],[data-srv-export],[data-local-open],[data-local-start],[data-local-stop],[data-local-play],[data-local-forget],[data-reveal-local],[data-reveal-host],[data-copy],[data-copy-link],[data-check-outside],[data-allow-service],[data-send],[data-shutdown-cancel],[data-ly-row],[data-ly-export],[data-ly-publish],[data-ly-confirm-delete],[data-ly-delete],[data-srv-check],[data-srv-approve],[data-srv-cancel-update],[data-srv-rollback],[data-lyceum-world],[data-lyceum-upgrade],[data-get],[data-remove],[data-go],[data-act],[data-url],[data-home],[data-world],[data-filter],[data-unfav],[data-fav],[data-client],[data-default-set],[data-reset-set],[data-delete-set],[data-remember],[data-forget-account],[data-section],[data-open-world],[data-trash],[data-iface],[data-stop],[data-plugin-install],[data-plugin-update],[data-plugin-remove],[data-plugin-source-remove],[data-plugin-open],[data-plugin-folder-remove],[data-plugin-row],[data-plugin-fold]");
  if (!t) return;
  if (t.dataset.stop !== undefined && !ev.target.closest("[data-act]")) return;
  const d = t.dataset;
  if (d.unfav) { ev.stopPropagation(); await act("remove_favourite", { id: d.unfav }); return; }
  if (d.fav) {
    ev.stopPropagation();
    const e = homeEntries()[Number(d.fav)];
    if (e) await act("save_favourite", { favourite: { id: "", world_slug: e.world_slug, account: e.account, client: e.client, dat_set_id: e.client === "dereth" ? e.dat_set_id ?? null : null, classic_set_id: e.client === "dereth" ? e.classic_set_id ?? null : null } });
    return;
  }
  if (d.go) {
    ui.screen = d.go;
    if (d.go === "home") ui.firstRun.dismissed = ui.firstRun.dismissed || ui.snap.state.dat_sets.length > 0;
    if (d.go === "server" || d.go === "make") checkComponents(false);
    if (d.go === "server") lastLook = 0;
    render();
    return;
  }
  // Host a world: a world's row opens its page; its buttons act on it.
  if (d.srvOpen) { ui.serverId = d.srvOpen; ui.serverEdit = null; ui.keyDraft = null; ui.confirmDelete = null; ui.screen = "server-page"; lastLook = 0; render(); return; }
  // The other worlds on this computer.
  if (d.localOpen) { ui.localKey = d.localOpen; ui.screen = "local-page"; lastLook = 0; render(); return; }
  if (d.localStart) { await act("start_local_world", { key: d.localStart }); return; }
  if (d.localStop) { await act("stop_local_world", { key: d.localStop, now: d.now === "1" }); return; }
  if (d.localPlay) { const slug = await act("open_local_world", { key: d.localPlay }); if (slug) await openWorld(slug); return; }
  if (d.localForget) { await act("forget_local_world", { key: d.localForget }); ui.screen = "server"; render(); return; }
  if (d.revealLocal) { await act("reveal_local_world", { key: d.revealLocal, place: d.place }); return; }
  if (d.revealHost) { await act("reveal_host", { id: d.revealHost, place: d.place }); return; }
  // Connecting: an address, the link, and a check from outside.
  if (d.copy) { await copyText(d.copy); return; }
  if (d.copyLink) { await copyText(d.copyLink); return; }
  if (d.checkOutside) { await act("check_from_outside", { port: Number(d.checkOutside) }); return; }
  if (d.allowService) { await act("allow_service_control", { key: d.allowService }); return; }
  // The console of a world that takes commands.
  if (d.send) { await submitCommand(d.send); return; }
  if (d.shutdownCancel) { if ((await sendCommand(d.shutdownCancel, "cancel-shutdown")) !== undefined) { delete ui.shutdownAt[d.shutdownCancel]; render(); } return; }
  // Build a world: a world's row opens and shuts in place, one at a time.
  if (d.lyRow) { ui.lyOpen = ui.lyOpen === d.lyRow ? null : d.lyRow; ui.lyDelete = null; render(); return; }
  if (d.lyConfirmDelete) { ui.lyDelete = d.lyConfirmDelete; render(); return; }
  if (d.lyDelete) {
    ui.lyDelete = null;
    await act("delete_lyceum_world", { world: d.lyDelete });
    if (!(comp("lyceum")?.worlds ?? []).some((w) => w.path === d.lyDelete)) { ui.lyOpen = null; render(); }
    return;
  }
  if (d.lyExport) { await act("export_lyceum_world", { world: d.lyExport, target: ui.lyExport[d.lyExport] ?? "empyrean", dats: defaultDats(), classic: defaultClassicDats() }); return; }
  if (d.lyPublish) {
    const hosts = ui.snap.hosts ?? [];
    const id = hosts.some((h) => h.id === ui.lyPublish[d.lyPublish]) ? ui.lyPublish[d.lyPublish] : hosts[0]?.id;
    if (id) await act("publish_host_world", { id, world: d.lyPublish });
    return;
  }
  if (d.srvStart) { await act("start_host", { id: d.srvStart }); return; }
  if (d.srvStop) { await act("stop_host", { id: d.srvStop }); return; }
  if (d.srvPrepare) { await act("prepare_host", { id: d.srvPrepare }); return; }
  if (d.srvPlay) { const slug = await act("open_host_world", { id: d.srvPlay }); if (slug) await openWorld(slug); return; }
  if (d.srvConfig) { await act("open_host_config", { id: d.srvConfig }); return; }
  if (d.srvFolder) { await act("open_host_folder", { id: d.srvFolder }); return; }
  if (d.srvExport) { await act("export_host_logs", { id: d.srvExport }); return; }
  if (d.srvCheck) { await act("check_host_update", { id: d.srvCheck }); return; }
  if (d.srvCancelUpdate) { await act("cancel_host_update", { id: d.srvCancelUpdate }); return; }
  if (d.srvApprove) { await act("approve_host_update", { id: d.srvApprove, version: d.version }); return; }
  if (d.srvRollback) { await act("rollback_host_update", { id: d.srvRollback }); return; }
  if (d.confirmDelete) { ui.confirmDelete = d.confirmDelete; render(); return; }
  if (d.srvDelete) {
    ui.confirmDelete = null;
    await act("remove_host", { id: d.srvDelete });
    if (!serverBy(d.srvDelete)) { ui.screen = "server"; ui.serverId = null; render(); }
    return;
  }
  if (d.srvPublish) {
    // The Lyceum world chosen beside the button; Other... asks for one.
    const pick = ui.publishPick[d.srvPublish] ?? comp("lyceum")?.worlds?.[0]?.path ?? "";
    await act("publish_host_world", { id: d.srvPublish, world: pick || null });
    return;
  }
  if (d.srvService) { await act("set_host_service", { id: d.srvService, enabled: d.enabled === "1" }); return; }
  if (d.srvUnpublish) { await act("unpublish_host_world", { id: d.srvUnpublish }); return; }
  if (d.srvRepublish) { await act("publish_host_world", { id: d.srvRepublish, world: serverBy(d.srvRepublish)?.publication?.source ?? null }); return; }
  // Build a world.
  if (d.lyceumWorld) { await act("open_lyceum", { world: d.lyceumWorld, dats: defaultDats(), classic: defaultClassicDats() }); return; }
  if (d.lyceumUpgrade) { await act("upgrade_lyceum_world", { world: d.lyceumUpgrade }); return; }
  if (d.get) { await act("get_component", { product: d.get }); return; }
  if (d.remove) { await act("remove_component", { product: d.remove }); return; }
  if (d.url) { ev.preventDefault(); await act("open_url", { url: d.url }); return; }
  if (d.home) { ui.homeSel = Number(d.home); render(); return; }
  // A row opens in place, one at a time; its ">" goes to the world's page.
  if (d.world) { ui.expanded = ui.expanded === d.world ? null : d.world; if (ui.expanded) api("probe_world", { slug: d.world }).catch(() => {}); render(); return; }
  if (d.openWorld) { await openWorld(d.openWorld); return; }
  if (d.section) { ui.folded[d.section] = !ui.folded[d.section]; render(); return; }
  if (d.trash) { if (ui.expanded === d.trash) ui.expanded = null; await act("remove_custom_world", { slug: d.trash }); return; }
  if (d.filter) { ui.filter = ui.filter === d.filter ? null : d.filter; render(); return; }
  if (d.client && ui.world) { ui.world.form.client = d.client; render(); return; }
  if (d.defaultSet) { await act("set_default_set", { id: d.defaultSet }); return; }
  if (d.resetSet) { await act("reset_set", { id: d.resetSet }); return; }
  if (d.deleteSet) { await act("delete_set", { id: d.deleteSet }); return; }
  if (d.pluginInstall) { await pluginFetch("install_plugin", { source: d.pluginInstall, id: d.pluginId }); return; }
  if (d.pluginUpdate) { await pluginFetch("update_plugin", { id: d.pluginUpdate }); return; }
  if (d.pluginRemove) { await act("remove_plugin", { id: d.pluginRemove }); return; }
  if (d.pluginSourceRemove) { await act("remove_plugin_source", { url: d.pluginSourceRemove }); return; }
  if (d.pluginOpen) { await act("open_plugin_folder", { folder: d.pluginOpen }); return; }
  if (d.pluginFolderRemove) { await act("remove_plugin_folder", { folder: d.pluginFolderRemove }); return; }
  // A plugin's row opens and shuts in place, one at a time; a section's heading folds it.
  if (d.pluginRow) { ui.plugin.open = ui.plugin.open === d.pluginRow ? null : d.pluginRow; render(); return; }
  if (d.pluginFold) { ui.plugin.folded[d.pluginFold] = !ui.plugin.folded[d.pluginFold]; render(); return; }
  if (d.forgetAccount) { const a = ui.snap.state.accounts[Number(d.forgetAccount)]; await act("forget_account", { slug: a.world_slug, username: a.username }); return; }
  if (d.iface) { ui.iface.pick = d.iface; render(); return; }

  switch (d.act) {
    case "srv-new": ui.serverDraft = newServerDraft(); render(); focusSoon("new-name"); break;
    case "srv-settings": { const h = serverBy(ui.serverId); if (h) { ui.serverEdit = { id: h.id, settings: editCopy(h) }; ui.screen = "server-settings"; } render(); break; }
    case "set-close": ui.serverEdit = null; ui.keyDraft = null; ui.screen = "server-page"; render(); break;
    case "set-cancel": { const h = serverBy(ui.serverId); if (h) ui.serverEdit = { id: h.id, settings: editCopy(h) }; ui.keyDraft = null; render(); break; }
    case "key-replace": ui.keyDraft = { id: ui.serverId, text: "", replacing: true }; render(); focusSoon("set-fed-key"); break;
    case "key-cancel": ui.keyDraft = null; render(); break;
    case "key-save": await saveKey(); break;
    case "delete-cancel": ui.confirmDelete = null; render(); break;
    case "ly-delete-cancel": ui.lyDelete = null; render(); break;
    case "ly-new": ui.lyceumDraft = { name: "", parent: null, era: "eor" }; render(); focusSoon("ly-name"); break;
    case "ly-cancel": ui.lyceumDraft = null; render(); break;
    case "ly-parent": { const where = await act("choose_lyceum_parent"); if (where) { ui.lyceumDraft.parent = where; render(); } break; }
    case "ly-create": {
      const draft = ui.lyceumDraft;
      try {
        // A world made here starts from nothing; the sample is the lyceum command's.
        await api("new_lyceum_world", { name: draft.name, parent: draft.parent, empty: true, era: draft.era ?? "eor", dats: defaultDats(), classic: defaultClassicDats() });
        ui.lyceumDraft = null;
        await pull();
        render();
      } catch (error) { toast(String(error), true); await pull(); }
      break;
    }
    case "srv-new-cancel": ui.serverDraft = null; render(); break;
    case "srv-create": {
      try {
        const id = await api("create_host", { settings: ui.serverDraft.settings });
        ui.serverDraft = null;
        ui.serverId = id;
        ui.screen = "server-page";
        await pull();
        render();
      } catch (error) { toast(String(error), true); }
      break;
    }
    case "srv-save": {
      const edit = ui.serverEdit;
      if (!edit) break;
      await act("save_host", { id: edit.id, settings: cleanSettings(edit.settings) });
      const said = serverBy(edit.id)?.message;
      // What stopped the save, said where the person is looking; the changes stay to be fixed.
      if (!said?.startsWith("Saved")) { if (said) toast(said, true); break; }
      ui.serverEdit = { id: edit.id, settings: editCopy(serverBy(edit.id)) };
      render();
      break;
    }
    case "set-systems-reset": { const s = srvSettings(); s.systems = {}; render(); break; }
    case "components-retry": checkComponents(true); break;
    case "lyceum-folder": await act("open_worlds_folder"); break;
    case "choose-lyceum-world": await act("choose_lyceum_world", { dats: defaultDats(), classic: defaultClassicDats() }); break;
    case "dismiss": localToast = null; await act("dismiss_message"); break;
    case "refresh": await act("refresh"); break;
    case "home-play": {
      const e = homeEntries()[ui.homeSel];
      if (e) await play({ world_slug: e.world_slug, account: e.account, password: null, remember: true, client: e.client, dat_set_id: e.client === "dereth" ? e.dat_set_id ?? null : null, classic_set_id: e.client === "dereth" ? e.classic_set_id ?? null : null });
      break;
    }
    case "world-play": { const c = formChoice(); if (c) await play(c); break; }
    case "add-account": ui.world.form = { ...ui.world.form, adding: true, account: "", password: "", remembered: false }; render(); focusSoon("acct-name"); break;
    case "known-accounts": { const a = ui.world.view.accounts[0]; ui.world.form = { ...ui.world.form, adding: false, account: a.username, password: "", remember: a.remember }; await recallPassword(); render(); break; }
    case "choose-retail": { const f = await act("pick_folder", { title: "Choose the folder that holds acclient.exe" }); if (f) { ui.addFind = { purpose: "retail", find: f }; render(); } break; }
    case "add-dats": { const f = await act("pick_folder", { title: "Choose a folder of data files: the client_*.dat files, or portal.dat and cell.dat" }); if (f) { ui.addFind = { purpose: "dats", find: f }; render(); } break; }
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
    case "plugins-refresh": await pluginFetch("refresh_plugins", {}); break;
    case "plugin-source-add": await addPluginSource(false); break;
    case "plugin-source-confirm": await addPluginSource(true); break;
    case "plugin-source-cancel": ui.plugin.confirm = false; render(); break;
    case "plugin-folder-add": await addPluginFolder(); break;
    case "plugin-create": await createPlugin(); break;
    case "plugin-open-made": await act("open_plugin_folder", { folder: ui.plugin.make.made.folder }); break;
    case "restart-update": await act("restart_to_update"); break;
    case "shortcut": { const where = await act("create_desktop_shortcut"); if (where) toast("Desktop shortcut created."); break; }
    case "server-open": ui.addServer = { name: "", host: "", port: "9000", emulator: "unknown", ruleset: "", error: null }; render(); focusSoon("srv-host"); break;
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
    case "iface-change": ui.iface = { ...ui.iface, pick: null, fromLibrary: true }; ui.screen = "interface"; render(); break;
    case "iface-cancel": ui.iface = { ...ui.iface, pick: null, fromLibrary: false }; ui.screen = "library"; render(); break;
    case "iface-save": await saveInterface(); break;
    case "fr-skip": ui.firstRun.dismissed = true; ui.screen = "home"; render(); break;
    case "fr-choose": { const f = await act("pick_folder", { title: retailHere() ? "Choose your Asheron's Call folder" : "Choose the folder that holds the four .dat files" }); if (f) { ui.firstRun.find = f; ui.firstRun.step = "found"; render(); } break; }
    case "fr-continue": { const f = ui.firstRun.find; ui.firstRun = { ...ui.firstRun, step: "ask", find: null, dismissed: true }; ui.screen = "home"; await act("add_folder", { find: f }); break; }
  }
});

// Send what the world's command line holds, as its kind says: a command as typed, an account made
// (its password hidden in the log), a message to everyone, a save, or a shutdown with a warning.
async function submitCommand(target) {
  const c = ui.cmd[target] ?? { kind: "command", text: "", account: "", password: "" };
  const done = () => { ui.cmd[target] = { ...c, text: "", account: "", password: "" }; render(); };
  switch (c.kind) {
    case "account": {
      const account = c.account.trim();
      if (!/^\S{1,50}$/.test(account) || !/^\S{1,100}$/.test(c.password)) { toast("An account name and a password, each without spaces.", true); return; }
      if ((await sendCommand(target, `accountcreate ${account} ${c.password}`)) !== undefined) done();
      return;
    }
    case "message": {
      const text = c.text.trim();
      if (!text) { toast("Type the message first.", true); return; }
      if ((await sendCommand(target, `gamecast ${text}`)) !== undefined) done();
      return;
    }
    case "save": await sendCommand(target, "save-world"); return;
    case "shutdown": {
      if ((await sendCommand(target, "set-shutdown-interval 300")) === undefined) return;
      if ((await sendCommand(target, "shutdown")) !== undefined) { ui.shutdownAt[target] = Date.now() + 300000; render(); }
      return;
    }
    default: {
      const line = c.text.trim();
      if (!line) return;
      if ((await sendCommand(target, line)) !== undefined) done();
    }
  }
}

// Keep the network key pasted into a world's Settings. Once kept it is dropped from the page, which
// says only that it is kept; a paste that is not a key is refused in plain words and stays to be
// fixed.
async function saveKey() {
  const draft = ui.keyDraft;
  if (!draft?.text?.trim()) return;
  try {
    await api("save_host_key", { id: draft.id, key: draft.text });
    ui.keyDraft = null;
  } catch (error) { toast(String(error), true); }
  await pull();
  render();
}

// Add the server in the form, and open its page.
async function addServer() {
  const a = ui.addServer;
  try {
    const slug = await api("add_custom_world", { server: { name: a.name, host: a.host, port: a.port, emulator: a.emulator || "unknown", ruleset: a.ruleset || null } });
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
  if (e) await openWorld(e.world_slug, null, { account: e.account, client: e.client, dat_set_id: e.dat_set_id, classic_set_id: e.classic_set_id });
});

document.addEventListener("input", (ev) => {
  const t = ev.target;
  if (t.id === "search") { ui.search = t.value; render(); return; }
  if (ui.addServer && t.id.startsWith("srv-")) { ui.addServer[t.id.slice(4)] = t.value; return; }
  if (t.id === "plugin-source") {
    const was = !ui.plugin.add.trim();
    const confirming = ui.plugin.confirm;
    ui.plugin.add = t.value;
    ui.plugin.confirm = false;
    if (confirming || was !== !ui.plugin.add.trim()) render();
    return;
  }
  if (t.id === "plugin-search") { ui.plugin.search = t.value; render(); return; }
  if (t.id === "plugin-make-name" || t.id === "plugin-make-github") {
    const m = ui.plugin.make;
    const was = !m.name.trim();
    m[t.id === "plugin-make-name" ? "name" : "github"] = t.value;
    if (was !== !m.name.trim()) render();
    return;
  }
  if (!ui.world) return;
  const f = ui.world.form;
  if (t.id === "acct-name") { f.account = t.value; f.remembered = false; syncWorld(); }
  if (t.id === "password") { f.password = t.value; syncWorld(); }
});

document.addEventListener("change", async (ev) => {
  const t = ev.target;
  if (t.dataset?.cmdKind) {
    const target = t.dataset.cmdKind;
    ui.cmd[target] = { ...(ui.cmd[target] ?? { text: "", account: "", password: "" }), kind: t.value };
    render();
    return;
  }
  if (t.tagName === "SELECT" && t.dataset?.srvField) {
    const s = srvSettings();
    setField(s, t.dataset.srvField, t.value);
    if (t.dataset.srvField === "era") eraChanged(s);
    render();
    return;
  }
  if (ui.addServer && (t.id === "srv-ruleset" || t.id === "srv-emulator")) { ui.addServer[t.id.slice(4)] = t.value; return; }
  if (t.id === "discord-presence") { await act("set_discord_presence", { on: t.checked }); return; }
  if (t.id === "plugin-updates") { await act("set_plugin_updates", { setting: t.value }); return; }
  if (t.tagName === "SELECT" && t.dataset.lyField && ui.lyceumDraft) { ui.lyceumDraft[t.dataset.lyField] = t.value; return; }
  if (t.dataset.publishPick) { ui.publishPick[t.dataset.publishPick] = t.value; render(); return; }
  if (t.dataset.lyExportPick) { ui.lyExport[t.dataset.lyExportPick] = t.value; return; }
  if (t.dataset.lyPublishPick) { ui.lyPublish[t.dataset.lyPublishPick] = t.value; render(); return; }
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
    case "classic":
      f.classic_set_id = t.value || null;
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
    if (ui.screen === "server-settings") { ui.screen = "server-page"; ui.serverEdit = null; ui.keyDraft = null; render(); return; }
    if (ui.screen === "server-page") { ui.screen = "server"; ui.serverEdit = null; ui.confirmDelete = null; render(); }
    if (ui.screen === "local-page") { ui.screen = "server"; render(); }
    if (ui.screen === "interface" && ui.iface.fromLibrary) { ui.iface = { ...ui.iface, pick: null, fromLibrary: false }; ui.screen = "library"; render(); }
  }
  if (ev.key === "Enter" && ui.addServer && ev.target.id?.startsWith("srv-")) { await addServer(); return; }
  if (ev.key === "Enter" && ev.target.id === "plugin-source") { await addPluginSource(false); return; }
  if (ev.key === "Enter" && (ev.target.id === "plugin-make-name" || ev.target.id === "plugin-make-github")) { await createPlugin(); return; }
  if (ev.key === "Enter" && ev.target.dataset?.cmd) { await submitCommand(ev.target.dataset.cmd); return; }
  if (ev.key === "Enter" && ev.target.dataset?.keyField !== undefined) { await saveKey(); return; }
  if (ev.key === "Enter" && ui.screen === "world" && (ev.target.id === "password" || ev.target.id === "acct-name")) {
    if (ev.target.id === "acct-name" && !ui.world.form.password) { focusSoon("password"); return; }
    const c = formChoice();
    if (c && ui.world.view.world.endpoint) await play(c);
  }
});

// ----- the pad ------------------------------------------------------------------------------------
//
// A pad moves about the page as the keyboard's Tab and Enter would, and more: the launcher reads it
// and sends each move as a `pad` event, and the page asks it which control a move reaches. The
// d-pad or the left stick moves the focus, the bottom face button presses the focused control, the
// right one steps back, the shoulder buttons step through the rail's pages, and
// the right stick scrolls the list under the focus. While the pad is in use the focus is drawn on
// every control, rows included; the pointer moving puts the page back as it was.

// Every control a click acts on, in the page's order: a row is one, and so is a button on it.
const PAD_CONTROLS = "button, a[href], input:not([type=hidden]), select, textarea, [data-go], [data-act], [data-url], [data-home], [data-world], [data-filter], [data-section], [data-open-world], [data-iface], [data-srv-open], [data-local-open], [data-plugin-row], [data-plugin-fold]";

// The sections the shoulder buttons step through, as the rail lists them.
const PAD_SECTIONS = ["home", "worlds", "accounts", "plugins", "server", "make", "library"];

// The controls a pad may reach: drawn, and not disabled. The screen's own come first, so a move
// with nothing focused starts on the screen rather than on the rail.
function padControls() {
  const drawn = (el) => {
    if (el.disabled) return false;
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0;
  };
  const main = document.getElementById("main");
  const all = [...document.querySelectorAll(PAD_CONTROLS)].filter(drawn);
  return [...all.filter((el) => main.contains(el)), ...all.filter((el) => !main.contains(el))];
}

// Focuses a control (a row too, which takes the focus only when asked), marks it so the focus is
// drawn whether or not the window is in front, and brings it into view.
function padFocus(el, show = true) {
  if (!el) return;
  if (el.tabIndex < 0 && !el.hasAttribute("tabindex")) el.setAttribute("tabindex", "-1");
  document.querySelectorAll(".pad-focus").forEach((x) => x.classList.remove("pad-focus"));
  el.classList.add("pad-focus");
  el.focus({ preventScroll: true });
  if (show) el.scrollIntoView({ block: "nearest", inline: "nearest" });
}

// Where a control is found again once the page is drawn anew: its tag and data attributes, and
// which of the controls alike it is.
function controlKey(el) {
  if (!el || el === document.body || !el.tagName) return null;
  const attrs = [...el.attributes].filter((a) => a.name.startsWith("data-")).map((a) => `[${a.name}="${CSS.escape(a.value)}"]`).join("");
  const sel = el.tagName.toLowerCase() + attrs;
  return { sel, index: [...document.querySelectorAll(sel)].indexOf(el) };
}

function controlByKey(key) {
  const all = document.querySelectorAll(key.sel);
  return all[key.index] ?? all[0] ?? null;
}

// One of a list's choices to the side: the next or the previous, as the list itself would change.
function padChoose(el, by) {
  const i = el.selectedIndex + by;
  if (i < 0 || i >= el.options.length) return;
  el.selectedIndex = i;
  el.dispatchEvent(new Event("change", { bubbles: true }));
}

async function padMove(step) {
  const el = document.activeElement;
  if (el?.tagName === "SELECT" && (step === "left" || step === "right")) {
    padChoose(el, step === "right" ? 1 : -1);
    return;
  }
  const controls = padControls();
  const from = controls.indexOf(el);
  const rects = controls.map((c) => {
    const r = c.getBoundingClientRect();
    return { x: r.left, y: r.top, width: r.width, height: r.height };
  });
  const to = await api("pad_target", { rects, from: from < 0 ? null : from, step });
  if (to != null) padFocus(controls[to]);
}

// The focused control, pressed: a button or a row is clicked, a text box takes Enter as from the
// keyboard, and a list moves on to its next choice, the first after the last.
function padPress() {
  const el = document.activeElement;
  if (!el || el === document.body) return;
  if (el.tagName === "SELECT") {
    padChoose(el, el.selectedIndex + 1 < el.options.length ? 1 : -el.selectedIndex);
  } else if (el.matches("input:not([type=checkbox]):not([type=radio]), textarea")) {
    el.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  } else {
    el.click();
  }
}

// A step back: as Escape does where it does anything, else what is open is closed, else Worlds,
// Library, Accounts and Plugins go back to Play.
function padBack() {
  const before = ui.screen;
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  if (ui.screen !== before) return;
  if (ui.addFind) { ui.addFind = null; render(); return; }
  if (ui.addServer) { ui.addServer = null; render(); return; }
  if (ui.serverDraft || ui.lyceumDraft) { ui.serverDraft = null; ui.lyceumDraft = null; render(); return; }
  if (ui.screen === "worlds" && ui.expanded) {
    const row = ui.expanded;
    ui.expanded = null;
    render();
    padFocus(document.querySelector(`[data-world="${CSS.escape(row)}"]`));
    return;
  }
  if (ui.screen === "plugins" && ui.plugin.confirm) { ui.plugin.confirm = false; render(); return; }
  if (ui.screen === "plugins" && ui.plugin.open) {
    const row = ui.plugin.open;
    ui.plugin.open = null;
    render();
    padFocus(document.querySelector(`[data-plugin-row="${CSS.escape(row)}"]`));
    return;
  }
  if (PAD_SECTIONS.includes(ui.screen) && ui.screen !== "home") { ui.screen = "home"; render(); }
}

// The next or previous section, as its rail button opens it, with the focus on its first control.
// The first start's screens keep to themselves.
function padSection(by) {
  const sections = PAD_SECTIONS;
  const at = sections.indexOf({ world: "worlds", "server-page": "server", "server-settings": "server", "local-page": "server" }[ui.screen] ?? ui.screen);
  if (at < 0) return;
  const go = sections[(at + by + sections.length) % sections.length];
  document.querySelector(`#rail [data-go="${go}"]`)?.click();
  const first = padControls()[0];
  if (first) padFocus(first);
}

function padScroll(pixels) {
  const list = document.activeElement?.closest?.(".scroll") ?? document.querySelector("#main .scroll");
  list?.scrollBy(0, pixels);
}

function onPad(move) {
  document.body.classList.add("pad");
  switch (move.kind) {
    case "move": padMove(move.step); break;
    case "press": padPress(); break;
    case "back": padBack(); break;
    case "next_section": padSection(1); break;
    case "previous_section": padSection(-1); break;
    case "scroll": padScroll(move.pixels); break;
  }
}

document.addEventListener("mousemove", () => document.body.classList.remove("pad"));
// While the pad is in use, whatever takes the focus is drawn as focused: a control the page itself
// focuses (a world page's password box), or one focused again after the page is drawn anew.
document.addEventListener("focusin", (ev) => {
  if (!document.body.classList.contains("pad")) return;
  document.querySelectorAll(".pad-focus").forEach((x) => x.classList.remove("pad-focus"));
  ev.target.classList?.add("pad-focus");
});
if (window.__TAURI__?.event) window.__TAURI__.event.listen("pad", (e) => onPad(e.payload));

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
      if (!view.classic_sets.some((s) => s.id === ui.world.form.classic_set_id)) ui.world.form.classic_set_id = view.classic_sets.find((s) => s.origin.kind === "shared")?.id ?? null;
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

// In the browser demo, `#world=eulmore`, `#worlds`, `#worlds=add`, `#worlds=<slug>` (that row
// open), `#library`, `#library=add`,
// `#accounts`, `#first-run`, `#first-run=guide` and `#interface` (the first start's question) open that
// screen directly, for looking at designs.
async function demoJump() {
  const [what, arg] = location.hash.slice(1).split("=");
  if (!what) return;
  await pull();
  if (what === "server" || what === "make") {
    // `#server` and `#make` as a first-time player sees them; `=ready` with the program
    // downloaded; `#server=new` the new server's form; `#server=running` and `#server=update` a
    // server's page, `#server=settings` with its settings open and `#server=console` with a
    // message being written; `#server=others` the list with the other worlds on this computer, and
    // `#server=service` and `#server=folder` one of their pages; `#server=lyceum` a world's page with
    // its Lyceum section open, and `#server=published` one that plays a Lyceum world saved since;
    // `#make=new` a new Lyceum world's form, and `#make=open` a Lyceum world's row open.
    await api("check_components", { force: false });
    if (arg) await api("get_component", { product: what === "server" ? "empyrean" : "lyceum" });
    if (["lyceum", "published"].includes(arg)) await api("get_component", { product: "lyceum" });
    if (what === "make" && arg === "open") await api("get_component", { product: "empyrean" });
    // `#server=settings-published` a published world's settings, `#server=settings-infiltration`
    // one being changed to Infiltration, `#server=settings-open` one with every section open and
    // some of each filled in, and `#server=settings-plain` every section open and nothing filled.
    const settingsShot = arg?.startsWith("settings-");
    if (["running", "update", "published", "lyceum", "settings", "console", "others", "advanced", "delete"].includes(arg) || settingsShot || (what === "make" && arg === "open")) {
      const id = await api("create_host", { settings: newServerDraft().settings });
      await api("prepare_host", { id });
      if (arg === "published" || arg === "settings-published") await api("publish_host_world", { id, world: "C:\\Users\\you\\Documents\\Lyceum\\holtburg-hills\\world.toml" });
      if (!["published", "lyceum", "open"].includes(arg) && !settingsShot) await api("start_host", { id });
      if (arg === "update") await api("check_host_update", { id });
      if (arg === "advanced") { await api("check_host_update", { id, latest: true }); await window.demoBackend("demo_list", { id }); }
      ui.serverId = id;
    }
    if (["others", "service", "folder", "channel"].includes(arg)) await api("scan_local_worlds", { force: true });
    await api("check_sharing", { ports: [9000], force: false });
    if (arg === "running") await api("check_from_outside", { port: 9000 });
    await pull();
    ui.screen = what;
    if (["running", "update", "published", "lyceum", "console", "advanced", "delete"].includes(arg)) ui.screen = "server-page";
    if (["published", "lyceum"].includes(arg)) { ui.folds[`fold-connect-${ui.serverId}`] = false; ui.folds[`fold-console-${ui.serverId}`] = false; ui.folds[`fold-lyceum-${ui.serverId}`] = true; }
    // Saved in Lyceum since it was published: there is a newer version to publish.
    if (arg === "published") { await window.demoBackend("demo_saved", { id: ui.serverId }); await pull(); }
    if (what === "make" && arg === "open") { ui.screen = "make"; ui.lyOpen = comp("lyceum").worlds[0].path; }
    if (arg === "settings" || settingsShot) { ui.serverEdit = { id: ui.serverId, settings: editCopy(serverBy(ui.serverId)) }; ui.screen = "server-settings"; }
    if (arg === "settings-infiltration") { ui.serverEdit.settings.era = "infiltration"; eraChanged(ui.serverEdit.settings); }
    if (arg === "settings-open") {
      // A made-up key, kept as a pasted one is.
      await api("save_host_key", { id: ui.serverId, key: `drth_${"0".repeat(43)}` });
      await pull();
      const s = ui.serverEdit.settings;
      Object.assign(s, { welcome: "Welcome to My world!\nBe kind, and have fun.\nThe Holtburg lifestone is where we meet.\nEvents most Saturdays.", systems: { aetheria: false, spell_research: true } });
      Object.assign(s.websocket, { enabled: true, behind_tls_proxy: true, allowed_origins: [PLAY_PAGE, "https://play.example.org"] });
      Object.assign(s.federation, { enabled: true, slug: "my-world" });
    }
    if (arg === "settings-open" || arg === "settings-plain") {
      for (const part of ["world", "players", "systems", "websocket", "network", "lyceum", "advanced"]) ui.folds[`set-${part}-${ui.serverId}`] = true;
    }
    if (arg === "console") { const t = `host:${ui.serverId}`; ui.cmd[t] = { kind: "account", text: "", account: "aerbax", password: "hunter22" }; ui.folds[`fold-connect-${ui.serverId}`] = false; }
    if (arg === "advanced" || arg === "delete" || arg === "update") { ui.folds[`fold-connect-${ui.serverId}`] = false; ui.folds[`fold-console-${ui.serverId}`] = false; ui.folds[`fold-advanced-${ui.serverId}`] = true; }
    if (arg === "delete") ui.confirmDelete = ui.serverId;
    if (["service", "folder", "channel"].includes(arg)) { ui.localKey = (ui.snap.local ?? [])[{ service: 0, folder: 1, channel: 2 }[arg]]?.key; ui.screen = "local-page"; }
    if (arg === "new") ui.serverDraft = newServerDraft();
    if (what === "make" && arg === "new") ui.lyceumDraft = { name: "Holtburg Hills", parent: null, era: "eor" };
    if (what === "make" && arg === "notes") { render(); document.getElementById("notes-lyceum")?.setAttribute("open", ""); }
  }
  else if (what === "world") await openWorld(arg || "eulmore");
  else if (what === "worlds" && arg === "add") {
    ui.addServer = { name: "", host: "play.example.org", port: "9000", ruleset: "PvE", error: null };
    ui.filter = "pve";
    ui.screen = "worlds";
  }
  else if (what === "join") {
    // `#join`: a join link waiting, as the system hands one to the launcher; taking it opened
    // Worlds with Add world filled in.
  }
  else if (what === "worlds" && arg) {
    // `#worlds=<slug>`: the list with that world's row open.
    ui.expanded = arg;
    ui.screen = "worlds";
  }
  else if (what === "library" && arg === "add") {
    // The "change the retail client" panel, over a folder whose dats predate the final patch.
    const f = await api("pick_folder", {});
    for (const x of f.dats.files) if (x.role === "portal") x.iterations = 2000;
    ui.addFind = { purpose: "retail", find: f };
    ui.screen = "library";
  }
  else if (what === "interface") ui.screen = "interface";
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
let drawnMinute = 0;
(async function loop() {
  await pull();
  await watchLibrary();
  lookAround();
  // A world's page says how long it has been up, which changes by the minute.
  const minute = Math.floor(Date.now() / 60000);
  if (minute !== drawnMinute && (ui.screen === "server-page" || ui.screen === "local-page")) render();
  drawnMinute = minute;
  setTimeout(loop, 1000);
})();

// The new world's form and a world's Settings, its log's search and command line, and the new
// Lyceum world's form.
document.addEventListener("input", (event) => {
  const t = event.target;
  const field = t.dataset?.srvField;
  if (field === "others") {
    srvSettings().bind = t.checked ? "0.0.0.0" : "127.0.0.1";
    render();
  } else if (field === "max_per_address") {
    srvSettings().max_per_address = t.value.trim() === "" ? -1 : Number(t.value);
    render();
  } else if (field === "websocket.allowed_origins") {
    // A page a line, kept as typed until the settings are saved.
    srvSettings().websocket.allowed_origins = t.value.split("\n");
    render();
  } else if (field === "websocket.behind_tls_proxy") {
    // Behind a proxy the world serves no certificate of its own: its boxes go, and come back as
    // they were if the box is unchecked again.
    const edit = ui.serverEdit;
    const ws = edit.settings.websocket;
    ws.behind_tls_proxy = t.checked;
    if (t.checked) {
      edit.tls = { tls_certificate: ws.tls_certificate, tls_private_key: ws.tls_private_key };
      ws.tls_certificate = "";
      ws.tls_private_key = "";
    } else if (edit.tls) {
      Object.assign(ws, edit.tls);
      edit.tls = null;
    }
    render();
  } else if (field && t.tagName !== "SELECT") {
    const v = t.type === "checkbox" ? t.checked : t.type === "number" ? Number(t.value) : t.value;
    setField(srvSettings(), field, v);
    if (ui.screen === "server-settings" || t.type === "checkbox" || field === "port") render();
  }
  if (t.dataset?.keyField !== undefined) { ui.keyDraft = { ...(ui.keyDraft ?? {}), id: ui.serverId, text: t.value }; render(); }
  // An era system turned on or off: named in the world's settings while it differs from what
  // the era would give it.
  const system = t.dataset?.srvSystem;
  if (system) {
    const s = srvSettings();
    const said = { ...(s.systems ?? {}), [system]: t.checked };
    const without = { ...said };
    delete without[system];
    if (systemsOver(eraOf(s.era)?.features ?? {}, without)[system] === t.checked) delete said[system];
    s.systems = said;
    render();
  }
  const log = t.dataset?.logFilter;
  if (log) { ui.logFilter[log] = t.value; render(); }
  if (t.dataset?.cmd && t.dataset.cmdField) {
    const target = t.dataset.cmd;
    ui.cmd[target] = { ...(ui.cmd[target] ?? { kind: "command", text: "", account: "", password: "" }), [t.dataset.cmdField]: t.value };
  }
  if (t.dataset?.lyField && ui.lyceumDraft) { ui.lyceumDraft[t.dataset.lyField] = t.value; if (t.dataset.lyField === "name") render(); }
});
