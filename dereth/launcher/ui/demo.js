// A stand-in backend, for opening the page in a browser without the app around it.
//
// It answers the same commands with the worlds and the library of the spec's wireframes, and a
// small copy of the pre-launch check. It exists for designing the page; the app never loads it
// when the real backend is there.

"use strict";

(function () {
  if (window.__TAURI__) return;

  // `?platform=macos` or `?platform=linux` shows the page as it is off Windows: Dereth only.
  const platform = new URLSearchParams(location.search).get("platform") ?? "windows";
  const EOR = { portal: 2072, cell: 982, local: 994, highres: 497 };
  const files = (it, ro, size = 360 * 1048576) =>
    ["portal", "cell", "local", "highres"].map((role) => ({ role, file_name: `client_${role}.dat`, size, modified: 0, read_only: ro, iterations: it[role] }));
  const classicFiles = (ro) => [
    { role: "portal", file_name: "portal.dat", size: 420 * 1048576, modified: 0, read_only: ro, iterations: 2112 },
    { role: "cell", file_name: "cell.dat", size: 480 * 1048576, modified: 0, read_only: ro, iterations: 1593 },
  ];

  // The client's era table, as the backend sends it.
  const NAMES = ["ratings", "consolidated_weapon_skills", "item_spell_auras", "assessed_armor_and_ratings", "swear_to_lower_level", "pre_order_items_and_rares", "dual_wield", "weapon_masteries", "innate_augmentations", "aetheria", "luminance", "contracts", "titles", "cloaks", "trinkets", "journal", "trade", "housing", "apartments", "tinkering", "cantrips", "spell_research", "chess", "swear_xp_cost"];
  const table = (on) => Object.fromEntries(NAMES.map((n) => [n, on(n)]));
  const eras = [
    { name: "eor", label: "End of retail", needs: "modern", features: table((n) => n !== "spell_research" && n !== "swear_xp_cost") },
    { name: "infiltration", label: "Infiltration (February 2005)", needs: "classic", features: table((n) => ["trade", "housing", "apartments", "tinkering", "cantrips", "chess", "swear_xp_cost"].includes(n)) },
  ];
  const features = NAMES.map((name) => {
    const special = { pre_order_items_and_rares: "Pre-order items and rares", swear_xp_cost: "Swearing costs experience", assessed_armor_and_ratings: "Assessing shows armor and ratings" }[name];
    const words = name.replace(/_/g, " ");
    return { name, label: special ?? words[0].toUpperCase() + words.slice(1) };
  });

  const world = (slug, name, emulator, state, players, ruleset, extra = {}) => ({
    slug, name, description: null, ruleset, emulator, state, players,
    era: null, era_features: null, era_source: null, features_source: null,
    emulator_version: null, list_id: null, development_status: "Stable",
    endpoint: { address: `${slug}.dereth.network`, port: 9000, transport: null },
    accepted_clients: [], preferred_client: null,
    dats: { expected: EOR, patches_over_wire: null, custom: null, highres_required: null },
    account_model: "auto_create_on_first_login", signup_url: null, reset_url: null,
    status_method: "none", status_url: null, links: { website: null, discord: null, rules: null, guide: null },
    operator: null, updated_at: null, schema: 2, ...extra,
  });

  const worlds = [
    world("eulmore", "Eulmore", "empyrean", "online", 12, "PvE", {
      description: "Internal dereth.network testbed world.",
      accepted_clients: ["dereth", "acclient-6096", "acclient-4186"], preferred_client: "dereth",
      links: { website: "https://dereth.network", discord: null, rules: null, guide: null },
      emulator_version: "0.1.2", development_status: "Development",
      era: "infiltration", era_source: "world", features_source: "world",
      era_features: "ratings=false,trade=true,aetheria=true,housing=true,apartments=true,tinkering=true,cantrips=true,chess=true,swear_xp_cost=true",
    }),
    world("harvestgain", "Harvestgain", "ace", "high", 342, "PvE", {
      description: "End of retail, as it was. Free buff bot at the Holtburg lifestone.",
      links: { website: "https://harvestgain.example", discord: "https://discord.gg/example", rules: null, guide: null },
      emulator_version: "1.77.4778",
    }),
    world("leafcull", "Leafcull", "ace", "online", 87, "PvE", { development_status: "Experimental" }),
    world("achard", "AChard", "ace", "unknown", null, "PvP", { description: "ACE EOR PVP Server/Custom PK Content/PK Kill Rankings", links: { website: "http://a-chard.ddns.net/", discord: "https://discord.gg/example", rules: null, guide: null } }),
    world("frostfell", "Frostfell", "ace", "unknown", null, "PvE", {
      dats: { expected: null, patches_over_wire: false, highres_required: null, custom: { url: "https://frostfell.example/data", sha256: null, size: null, iterations: { ...EOR, portal: 2090 }, license_note: null } },
    }),
    world("coldeve", "Coldeve", "gdle", "offline", null, "PvE", { dats: { expected: EOR, patches_over_wire: true, custom: null, highres_required: null } }),
    world("morningthaw", "Morningthaw", "ace", "unknown", null, "PvE"),
    world("darktide", "Darktide", "ace", "unknown", null, "PK"),
  ];

  const inst = (id, kind, client_id, version, date, path, extra = {}) => ({
    id, path, exe: kind === "dereth" ? "dereth-client" : "acclient.exe", kind, client_id, version, build_date: date,
    net_version: "1802", identified_by: "exe_sha256", modifications: [], multi_instance: true, ...extra,
  });
  const dereth = inst("dereth", "dereth", "dereth-0.4.0", "0.4.0", "2026-09-26", "C:\\Users\\you\\Games\\Dereth");

  const state = {
    schema: 1,
    retail: inst("retail", "retail", "acclient-6096", "00.00.11.6096", "2015-06-12", "C:\\Turbine\\Asheron's Call", { modifications: ["single-instance check removed"] }),
    dat_sets: [
      { id: "eor", kind: "modern", path: "C:\\Turbine\\Asheron's Call", origin: { kind: "shared" }, files: files(EOR, true), created_by_launcher: false },
      { id: "d2", kind: "modern", path: "C:\\Users\\you\\AppData\\Local\\Dereth\\launcher\\library\\datsets\\d2", origin: { kind: "world", slug: "coldeve" }, files: files(EOR, false), created_by_launcher: true, last_patched_by_server: 1 },
      { id: "c1", kind: "classic", path: "C:\\Games\\AC February 2005", origin: { kind: "shared" }, files: classicFiles(true), created_by_launcher: false },
    ],
    accounts: [
      { world_slug: "eulmore", username: "player", remember: true, created_by_launcher: false },
      { world_slug: "eulmore", username: "alt1", remember: false, created_by_launcher: false },
      { world_slug: "achard", username: "player2", remember: true, created_by_launcher: false },
    ],
    favourites: [
      { id: "f1", world_slug: "eulmore", account: "player", client: "dereth", dat_set_id: "eor" },
      { id: "f2", world_slug: "achard", account: "player2", client: "retail", dat_set_id: null },
      { id: "f3", world_slug: "coldeve", account: "player", client: "dereth", dat_set_id: "d2" },
    ],
    world_prefs: {
      leafcull: { account: "player", client: "dereth", dat_set_id: "eor", last_played: 20 },
      harvestgain: { account: "alt1", client: "retail", last_played: 10 },
    },
    recent: [
      { world_slug: "leafcull", account: "player", client: "dereth", dat_set_id: "eor", last_played: 20 },
      { world_slug: "eulmore", account: "player", client: "dereth", dat_set_id: "eor", last_played: 15 },
      { world_slug: "harvestgain", account: "alt1", client: "retail", last_played: 10 },
    ],
    settings: { allow_play_anyway: false, auto_update: true },
    custom_worlds: [{ slug: "custom-1", name: "My test server", host: "127.0.0.1", port: 9000, ruleset: "PvE" }],
    world_eras: { leafcull: { era: "eor", features: { trade: false } } },
  };
  const vault = new Set(["eulmore/player", "achard/player2"]);
  let message = null;
  let job = null;

  const accepts = (w, i) =>
    w.accepted_clients.length
      ? w.accepted_clients.some((c) => c === i.client_id || (c === "dereth" && i.client_id.startsWith("dereth-")))
      : i.client_id.startsWith("dereth") || (i.client_id.startsWith("acclient-") && i.net_version === "1802");
  const needsPrivate = (w) => !!w.dats.patches_over_wire || !!w.dats.custom;
  const setsFor = (w) => state.dat_sets.filter((s) => s.kind !== "classic").filter((s) => (s.origin.kind === "world" ? s.origin.slug === w.slug : s.origin.kind === "custom" ? !!w.dats.custom : !needsPrivate(w)));

  // What the world does not say, the player may have chosen; as the backend lays it over.
  const withChoice = (w) => {
    const c = state.world_eras[w.slug];
    if (!c) return w;
    const out = { ...w };
    if (!out.era && c.era) { out.era = c.era; out.era_source = "player"; }
    const text = Object.entries(c.features ?? {}).map(([k, v]) => `${k}=${v}`).join(",");
    if (!out.era_features && text) { out.era_features = text; out.features_source = "player"; }
    return out;
  };
  const allWorlds = () => [...worlds, ...state.custom_worlds.map((c) => ({
    ...world(c.slug, c.name, c.emulator ?? "unknown", "unknown", null, c.ruleset ?? null),
    endpoint: { address: c.host, port: c.port, transport: null },
    description: `Added by you: ${c.host}:${c.port}`, development_status: null,
  }))].map(withChoice);
  const choice = (slug) => (state.world_eras[slug] ??= { era: null, features: {} });
  const tidy = (slug) => { const c = state.world_eras[slug]; if (c && !c.era && !Object.keys(c.features ?? {}).length) delete state.world_eras[slug]; };

  const handlers = {
    snapshot: () => ({
      worlds: allWorlds(), list_state: "loaded", list_error: null, list_fetched_at: Math.floor(Date.now() / 1000) - 3600, eras, features,
      state: structuredClone(state), dereth, running: [], job, message,
      vault_name: "Windows Credential Manager", platform, library_dir: "C:\\Users\\you\\AppData\\Local\\Dereth\\launcher\\library",
      update: { kind: "none" }, version: "0.1.0", shortcut: platform !== "macos",
    }),
    refresh: () => {},
    set_world_era: ({ slug, era }) => {
      const c = choice(slug);
      if ((era ?? null) !== (c.era ?? null)) { c.era = era ?? null; c.features = {}; }
      tidy(slug);
    },
    set_world_feature: ({ slug, name, on }) => {
      const c = choice(slug);
      const base = eras.find((e) => e.name === (c.era || "eor")).features;
      if (base[name] === on) delete c.features[name]; else c.features[name] = on;
      tidy(slug);
    },
    world_view: ({ slug }) => {
      const w = allWorlds().find((x) => x.slug === slug);
      if (!w) throw "That world is no longer listed.";
      const clients = [dereth, platform === "windows" ? state.retail : null].filter(Boolean).map((i) => ({ kind: i.kind, install: i, accepted: accepts(w, i), runnable: true }));
      const last = state.world_prefs[slug]?.client;
      return {
        world: w, clients, dat_sets: setsFor(w),
        classic_sets: state.dat_sets.filter((s) => s.kind === "classic"),
        requires: eras.find((e) => e.name === w.era)?.needs ?? "modern",
        default_client: (last && clients.some((c) => c.kind === last) ? last : (clients.find((c) => c.accepted) ?? clients[0])?.kind) ?? null,
        offers_private_copy: needsPrivate(w) && !w.dats.custom && !state.dat_sets.some((s) => s.origin.kind === "world" && s.origin.slug === slug),
        accounts: state.accounts.filter((a) => a.world_slug === slug), prefs: state.world_prefs[slug] ?? {},
        live: null,
      };
    },
    has_password: ({ slug, username }) => vault.has(`${slug}/${username}`),
    launch: ({ choice }) => {
      if (!choice.password && !vault.has(`${choice.world_slug}/${choice.account}`)) return { kind: "need_password", message: `Enter the password for ${choice.account}.` };
      if (choice.remember) vault.add(`${choice.world_slug}/${choice.account}`);
      state.world_prefs[choice.world_slug] = { account: choice.account, client: choice.client, dat_set_id: choice.dat_set_id, last_played: Date.now() };
      const same = (r) => r.world_slug === choice.world_slug && r.account === choice.account && r.client === choice.client && (r.dat_set_id ?? null) === (choice.dat_set_id ?? null);
      state.recent = [{ world_slug: choice.world_slug, account: choice.account, client: choice.client, dat_set_id: choice.dat_set_id, last_played: Date.now() }, ...state.recent.filter((r) => !same(r))].slice(0, 5);
      return { kind: "launched", world: choice.world_slug, account: choice.account };
    },
    pick_folder: () => ({
      folder: "C:\\Turbine\\Asheron's Call",
      retail: state.retail ?? inst("retail", "retail", "acclient-6096", "00.00.11.6096", "2015-06-12", "C:\\Turbine\\Asheron's Call"),
      dats: { id: "d9", kind: "modern", path: "C:\\Turbine\\Asheron's Call", origin: { kind: "unassigned" }, files: files(EOR, true), created_by_launcher: false },
      classic: { id: "c9", kind: "classic", path: "C:\\Turbine\\Asheron's Call", origin: { kind: "unassigned" }, files: classicFiles(true), created_by_launcher: false },
      error: null,
    }),
    add_folder: ({ find }) => { if (find.retail) state.retail = find.retail; },
    set_default_set: ({ id }) => {
      const kind = state.dat_sets.find((s) => s.id === id)?.kind;
      for (const s of state.dat_sets) {
        if (s.kind === kind && s.origin.kind === "shared") s.origin = { kind: "unassigned" };
        if (s.id === id) s.origin = { kind: "shared" };
      }
    },
    add_custom_dats: () => false,
    create_private_copy: ({ slug }) => {
      job = { what: `Copying data files for ${slug}`, done: 0, total: 1400 };
      const t = setInterval(() => {
        job.done += 140;
        if (job.done >= job.total) {
          clearInterval(t);
          job = null;
          state.dat_sets.push({ id: `p-${slug}`, kind: "modern", path: `C:\\Users\\you\\AppData\\Local\\Dereth\\launcher\\library\\datsets\\p-${slug}`, origin: { kind: "world", slug }, files: files(EOR, false), created_by_launcher: true });
        }
      }, 300);
    },
    reset_set: () => {},
    delete_set: ({ id }) => { state.dat_sets = state.dat_sets.filter((s) => s.id !== id); },
    forget_retail: () => { state.retail = null; },
    verify_retail: () => { message = { text: "205 of 205 files match", error: false }; },
    save_favourite: ({ favourite }) => { state.favourites.push({ ...favourite, id: `f${Date.now()}` }); },
    remove_favourite: ({ id }) => { state.favourites = state.favourites.filter((f) => f.id !== id); },
    forget_account: ({ slug, username }) => { state.accounts = state.accounts.filter((a) => !(a.world_slug === slug && a.username === username)); vault.delete(`${slug}/${username}`); },
    set_remember: ({ slug, username, remember }) => { const a = state.accounts.find((x) => x.world_slug === slug && x.username === username); if (a) a.remember = remember; if (!remember) vault.delete(`${slug}/${username}`); },
    forget_all_passwords: () => { vault.clear(); state.accounts.forEach((a) => (a.remember = false)); },
    forget_all_accounts: () => { vault.clear(); state.accounts = []; state.favourites = []; state.recent = []; },
    add_custom_world: ({ server: { name, host, port, ruleset, era, emulator } }) => {
      if (!host.trim()) throw "Enter the server's host name or address.";
      const p = Number(port);
      if (!(p >= 1 && p <= 65535)) throw "The port must be a number from 1 to 65535.";
      const slug = `custom-${state.custom_worlds.length + 1}`;
      state.custom_worlds.push({ slug, name: name.trim() || host.trim(), host: host.trim(), port: p, ruleset: ruleset ?? null, emulator: emulator ?? "unknown" });
      if (era) state.world_eras[slug] = { era, features: {} };
      return slug;
    },
    probe_world: () => {},
    remove_custom_world: ({ slug }) => { state.custom_worlds = state.custom_worlds.filter((c) => c.slug !== slug); },
    dismiss_message: () => { message = null; },
    open_url: ({ url }) => { window.open(url, "_blank"); },
    create_desktop_shortcut: () => "C:\\Users\\you\\Desktop\\Dereth.lnk",
    restart_to_update: () => {},
  };

  window.demoBackend = async (cmd, args = {}) => {
    const h = handlers[cmd];
    if (!h) throw `demo: no ${cmd}`;
    return structuredClone(h(args));
  };
})();
