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
    { name: "eor", label: "End of Retail", needs: "modern", features: table((n) => n !== "spell_research" && n !== "swear_xp_cost") },
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
    settings: { allow_play_anyway: false, auto_update: true, discord_presence: true },
    custom_worlds: [{ slug: "custom-1", name: "My test world", host: "127.0.0.1", port: 9000, ruleset: "PvE" }],
  };
  const vault = new Set(["eulmore/player", "achard/player2"]);
  // The interface the Dereth client starts in; `#interface` shows the first start, with none saved.
  let chosenInterface = location.hash === "#interface" ? null : "Horizon";
  const interfaceOffers = [
    { name: "Horizon", line: "Dereth's own interface, with hotbars, a minimap and nameplates." },
    { name: "Modern", line: "The interface the game had from its 2005 redesign to the end of retail." },
    { name: "Classic", line: "The interface the game had before its 2005 redesign; it needs Classic data files." },
  ];
  let message = null;
  let job = null;

  const accepts = (w, i) =>
    w.accepted_clients.length
      ? w.accepted_clients.some((c) => c === i.client_id || (c === "dereth" && i.client_id.startsWith("dereth-")))
      : i.client_id.startsWith("dereth") || (i.client_id.startsWith("acclient-") && i.net_version === "1802");
  const needsPrivate = (w) => !!w.dats.patches_over_wire || !!w.dats.custom;
  const setsFor = (w) => state.dat_sets.filter((s) => s.kind !== "classic").filter((s) => (s.origin.kind === "world" ? s.origin.slug === w.slug : s.origin.kind === "custom" ? !!w.dats.custom : !needsPrivate(w)));

  const allWorlds = () => [...worlds, ...state.custom_worlds.map((c) => ({
    ...world(c.slug, c.name, c.emulator ?? "unknown", "unknown", null, c.ruleset ?? null),
    endpoint: { address: c.host, port: c.port, transport: null },
    description: `Added by you: ${c.host}:${c.port}`, development_status: null,
  }))];

  // Empyrean and Lyceum as GitHub lists them, and what is here. `?dev=1` is a development build with
  // its checkout's own builds.
  const development = new URLSearchParams(location.search).has("dev");
  const RELEASES = {
    empyrean: { version: "0.3.3", size: 16352529, notes: "Worlds keep running through a lost connection to the hub.\n\nFixed corpses sometimes landing under the floor." },
    lyceum: { version: "0.1.0", size: 13587452, notes: "The first release: open a world over your own data files, edit it in place, and play it.\n\nRead [the guide](https://dereth.network/lyceum/) first, or the notes at https://dereth.network/changes." },
  };
  const checkout = { empyrean: "C:\\code\\dereth\\target\\release\\empyrean-server.exe", lyceum: "C:\\code\\dereth\\target\\release\\lyceum-editor.exe" };
  const components = ["empyrean", "lyceum"].map((product) => ({
    product, installed: null, checkout: development ? checkout[product] : null, checkout_version: development ? { empyrean: "0.4.0", lyceum: "0.1.1" }[product] : null,
    latest: null, latest_notes: "", latest_size: null,
    offer: { kind: "none" }, checking: false, busy: false, progress: null, message: null, error: false, running: false, worlds: [],
    home: product === "lyceum" ? "C:\\Users\\you\\Documents\\Lyceum" : null,
  }));
  // A Lyceum world as the backend lists it.
  const lyWorld = (path, home, extra = {}) => ({ path, home, key: path.split("\\").slice(-2, -1)[0], era: "eor", saved: Math.floor(Date.now() / 1000) - 3 * 86400, outdated: false, facts: null, ...extra });
  // The ACE world each era's world database is made from, as Get world database records it.
  const BASES = { eor: { era: "eor", version: "v0.9.295" }, infiltration: { era: "infiltration", version: "v0.8.8" } };
  const offerOf = (c) => c.checkout || !c.latest ? { kind: "none" } : !c.installed ? { kind: "download", version: c.latest } : c.installed.version !== c.latest ? { kind: "update", version: c.latest } : { kind: "none" };
  const hosts = [];
  const hostBy = (id) => hosts.find((h) => h.id === id);
  // The other worlds on this computer, once the page has looked: an ACE world a Windows service
  // runs, an Empyrean world the launcher has seen run from a folder, stopped now, and an Empyrean
  // world a Windows service runs, whose admin channel the launcher is connected to.
  let looked = false;
  const local = [
    { key: "ace:c:\\ace\\config.js", software: "ace", name: "Frostfell", running: true, pid: 4120, state: "online", version: null, players: null, host: "0.0.0.0", port: 9050,
      started: Math.floor(Date.now() / 1000) - 3 * 86400 - 7200, manager: { kind: "windows_service", name: "ACEmulator" }, ours: false, stopping: false, cannot_start: null,
      folder: "C:\\ACE", config: "C:\\ACE\\Config.js", data: null, dats: "C:\\ACE\\Dats", log: "C:\\ACE\\ACE_Log.txt", open_accounts: true, message: null,
      channel: false, federation_slug: null, listed: false, control: { can_control: false, can_grant: true, start_command: "sc start ACEmulator", stop_command: "sc stop ACEmulator" },
      logs: ["2026-10-21 20:14:02 [INFO] Server is now online.", "2026-10-21 21:02:11 [INFO] Player Dalton logged in.", "2026-10-21 21:40:55 [INFO] Player Dalton logged off."] },
    { key: "empyrean:c:\\worlds\\lakeside\\empyrean.toml", software: "empyrean", name: "Lakeside", running: false, pid: null, state: "offline", version: null, players: null, host: "0.0.0.0", port: 9010,
      started: null, manager: null, ours: false, stopping: false, cannot_start: null,
      folder: "C:\\Worlds\\Lakeside", config: "C:\\Worlds\\Lakeside\\empyrean.toml", data: "C:\\Worlds\\Lakeside", dats: "C:\\Turbine\\Asheron's Call", log: null, open_accounts: false, message: null, logs: [],
      channel: false, federation_slug: null, listed: false, control: null },
    { key: "empyrean:c:\\worlds\\coldeve\\empyrean.toml", software: "empyrean", name: "Coldeve", running: true, pid: 5212, state: "online", version: "0.3.3", players: 4, host: "0.0.0.0", port: 9020,
      started: Math.floor(Date.now() / 1000) - 26 * 3600, manager: { kind: "windows_service", name: "Empyrean" }, ours: false, stopping: false, cannot_start: null,
      folder: "C:\\Worlds\\Coldeve", config: "C:\\Worlds\\Coldeve\\empyrean.toml", data: "C:\\Worlds\\Coldeve", dats: "C:\\Turbine\\Asheron's Call", log: null, open_accounts: true, message: null,
      channel: true, federation_slug: "coldeve", listed: true, control: { can_control: false, can_grant: true, start_command: "sc start Empyrean", stop_command: "sc stop Empyrean" },
      logs: ["2026-10-22 09:00:01.204 INFO  [World Manager] Dereth Network: linked to the hub as \"coldeve\"", "2026-10-22 09:14:37.881 INFO  [World Manager] Login: player Mirelle entered the world.", "2026-10-22 09:20:02.017 INFO  [World Manager] Login: player Torvald entered the world."] },
  ];
  const localBy = (key) => local.find((w) => w.key === key);
  // This computer's address, and a router that answers with documentation addresses.
  let sharing = { lan: null, router: false, public: null, shared_address: false, forwards: [], checking: false, outside: {} };
  // `#join`: a join link waiting, as the system hands one to the launcher.
  let join = location.hash === "#join" ? { host: "203.0.113.7", port: 9000, name: "Kazin's world" } : null;
  const handlers = {
    choose_host_dats: () => "C:\\Turbine\\Asheron's Call",
    create_host: ({ settings }) => {
      const id = `host-${hosts.length + 1}`;
      const c = components[0];
      hosts.push({ id, settings: { era: "eor", welcome: "", max_players: 128, max_per_address: -1, shutdown_warning: 60, log_level: "info", systems: {},
        websocket: { enabled: false, listen: "0.0.0.0:9443", tls_certificate: "", tls_private_key: "", behind_tls_proxy: false, allowed_origins: [], public_url: "" },
        federation: { enabled: false, slug: "" }, patching: { enabled: true, records_per_minute: 60000, bytes_per_second: 1048576 }, ...settings },
        version: c.installed?.version ?? "0.3.3", checkout: !!c.checkout, status: { phase: "stopped", players: null, started: null }, logs: [], prepared: false, service: false, update: {}, publication: null, folder: `C:\\Users\\you\\AppData\\Local\\Dereth\\launcher\\hosts\\${id}`, busy: null, message: null, federation_slug: null, listed: false,
        plays: { database: "era", pack: "world.pack", dat_overlay: null, content_overlay: null, published: null, network_key: false, base: null, era_bases: [] }, unpublished_changes: null });
      return id;
    },
    save_host: ({ id, settings }) => {
      const h = hostBy(id);
      // As the backend: the game data is the kind the era plays, and a published world keeps its
      // era and game data.
      const needs = eras.find((e) => e.name === settings.era)?.needs ?? "modern";
      if (h.plays.dat_overlay && (settings.era !== h.settings.era || settings.dats !== h.settings.dats)) { h.message = "The Lyceum world this world plays was built for its era and game data, so those stay as they are."; return; }
      if (settings.federation?.enabled && !h.plays.network_key) { h.message = "Save the world's key before linking it to the Dereth Network."; return; }
      if (!state.dat_sets.some((s) => s.path === settings.dats && s.kind === needs)) { h.message = needs === "classic" ? "Infiltration (February 2005) is drawn on the land from before Throne of Destiny: choose Classic data files (portal.dat and cell.dat) for it." : "End of Retail plays the Modern data files (client_portal.dat and the rest): choose those for it."; return; }
      if (settings.era !== h.settings.era && h.plays.database === "era") {
        h.plays.pack = settings.era === "infiltration" ? "world.pack.infiltration" : "world.pack";
        h.plays.base = h.plays.era_bases.find((b) => b.era === settings.era) ?? null;
        h.prepared = !!h.plays.base;
      }
      // A published Lyceum world sends its data to Dereth players.
      if (h.plays.published) settings = { ...settings, patching: { ...settings.patching, enabled: true } };
      h.settings = settings;
      h.message = h.status.phase === "running" ? "Saved. Restart the world to use the new settings." : "Saved.";
    },
    // As the backend: a key of the shape Empyrean reads is kept, and never sent back.
    save_host_key: ({ id, key }) => {
      const k = String(key ?? "").trim();
      if (!k) throw "Paste the world's key from the Dereth Network first.";
      if (!/^drth_[A-Za-z0-9_-]{43}$/.test(k)) throw "That is not a Dereth Network key. Paste the whole key the network gave you: it starts with drth_ and is 48 characters long.";
      hostBy(id).plays.network_key = true;
    },
    prepare_host: ({ id }) => {
      const h = hostBy(id);
      h.prepared = true;
      h.plays.base = BASES[h.settings.era];
      h.plays.era_bases = [...h.plays.era_bases.filter((b) => b.era !== h.settings.era), h.plays.base];
      h.message = "The world database is ready.";
    },
    start_host: ({ id }) => { const h = hostBy(id); h.status = { phase: "running", players: 2, started: Math.floor(Date.now() / 1000) - 3840 }; h.message = null; h.logs.push("Empyrean 0.3.3 starting.", "Loaded 7,421 landblocks.", "World ready: My world on port 9000.", "Login: player Aerbax entered the world.", "Login: player Kazin entered the world."); },
    stop_host: ({ id }) => { const h = hostBy(id); h.status = { phase: "stopped", players: null, started: null }; h.logs.push("Saving characters...", "World saved and stopped."); },
    open_host_config: () => {},
    host_command: ({ id, line }) => {
      const h = hostBy(id);
      if (h.status.phase !== "running") throw "Start the world first.";
      const said = line.startsWith("pop") ? ["Current world population: 2"] : line === "save-world" ? ["Saving 2 players and 41 landblocks...", "Saved."] : ["Done."];
      h.logs.push(`> ${line.replace(/^(accountcreate \S+) \S+/, "$1 ***")}`, ...said);
    },
    reveal_host: () => {},
    scan_local_worlds: ({ force }) => { if (force) looked = true; },
    start_local_world: ({ key }) => { const w = localBy(key); Object.assign(w, { running: true, ours: true, state: "online", players: 0, version: "0.3.3", started: Math.floor(Date.now() / 1000), manager: { kind: "launcher" }, logs: ["Empyrean 0.3.3 starting.", "World ready: Lakeside on port 9010."] }); },
    stop_local_world: ({ key, now }) => {
      const w = localBy(key);
      // A service the launcher may control is stopped by its service manager; any other world
      // with a channel is asked over it.
      if (w.control?.can_control && !w.ours) { w.stopping = true; w.logs.push("Shutting down..."); return; }
      if (w.channel && !w.ours) { w.stopping = true; w.logs.push("> exit", "Shutting down..."); return; }
      if (!w.ours) throw "The launcher did not start this world, so it leaves it alone. Stop it where it was started.";
      if (w.stopping && now) Object.assign(w, { running: false, ours: false, stopping: false, state: "offline", manager: null }); else w.stopping = true;
    },
    local_command: ({ key, line }) => { const w = localBy(key); if (!w.ours && !w.channel) throw "This world takes no commands from here."; w.logs.push(`> ${line}`, line === "save-world" ? "Saved." : "Done."); },
    allow_service_control: ({ key }) => { const w = localBy(key); w.control = { can_control: true, can_grant: false, start_command: null, stop_command: null }; },
    check_from_outside: ({ port }) => { sharing.outside[port] = { checking: false, outcome: { kind: "answered", address: "203.0.113.7", port, reached: true, world: { name: "My world", players: 2 } } }; },
    take_join: () => { const j = join; join = null; return j; },
    forget_local_world: ({ key }) => { local.splice(local.findIndex((w) => w.key === key), 1); },
    reveal_local_world: () => {},
    open_local_world: () => "custom-1",
    check_sharing: () => { sharing = { lan: "192.168.1.20", router: true, public: "203.0.113.7", shared_address: false, forwards: [[9000, null]], checking: false, outside: sharing.outside ?? {} }; },
    choose_lyceum_parent: () => "D:\\Worlds",
    new_lyceum_world: ({ name, era }) => { const c = components.find((c) => c.product === "lyceum"); c.worlds.unshift(lyWorld(`C:\\Users\\you\\Documents\\Lyceum\\${name.toLowerCase().replace(/[^a-z0-9]+/g, "-")}\\world.toml`, true, { era, saved: Math.floor(Date.now() / 1000), facts: {} })); message = { kind: "info", text: "The app opens Lyceum here." }; },
    open_host_folder: () => {},
    remove_host: ({ id }) => { hosts.splice(hosts.findIndex((h) => h.id === id), 1); },
    set_host_service: ({ id, enabled }) => { const h = hostBy(id); h.service = enabled; h.message = enabled ? "Runs in the background from now on." : "Runs only while the launcher is open from now on."; },
    export_host_logs: () => {},
    publish_host_world: ({ id, world }) => {
      const h = hostBy(id);
      const source = world ?? "C:\\Users\\you\\Documents\\Lyceum\\holtburg-hills\\world.toml";
      const key = source.split("\\").slice(-2, -1)[0];
      // As the backend: each publish of the same Lyceum world is the next version.
      const revision = h.publication?.key === key ? h.publication.revision + 1 : 3;
      h.publication = { source, key, packs: ["holtburg"], build: "0123456789abcdef", revision, sources: "fedcba", base: "ACE-World-Database-v0.9.295.sql" };
      h.unpublished_changes = false;
      h.plays.base = BASES[h.settings.era];
      h.plays = { ...h.plays, database: "lyceum", pack: `${h.folder}\\published\\snapshot-0\\world.pack`, dat_overlay: `${h.folder}\\published\\snapshot-0\\overlay`, published: Math.floor(Date.now() / 1000) - 2 * 86400 };
      // As the backend: publishing turns patching on.
      h.settings.patching = { ...h.settings.patching, enabled: true };
      h.prepared = true;
      h.message = null;
    },
    unpublish_host_world: ({ id }) => {
      const h = hostBy(id);
      h.publication = null;
      h.plays = { ...h.plays, database: "era", pack: "world.pack", dat_overlay: null, published: null, base: BASES[h.settings.era] };
      h.unpublished_changes = null;
      h.message = null;
    },
    open_host_world: () => "eulmore",
    // `latest`: the demo's check finds nothing newer.
    check_host_update: ({ id, latest }) => {
      const h = hostBy(id);
      h.update = latest ? { latest: h.version, message: `Empyrean ${h.version} is the newest.`, pending: false, rollback: false }
        : { version: "0.4.0", notes: "Housing and allegiance houses.\n\nThe databases gain two tables, which the update adds.", message: "Empyrean 0.4.0 is out. Read what changed, then update.", pending: false, rollback: false };
    },
    // The demo's stand-in for saving the published Lyceum world in Lyceum.
    demo_saved: ({ id }) => { hostBy(id).unpublished_changes = true; },
    // The demo's stand-in for the hub listing a world.
    demo_list: ({ id }) => { const h = hostBy(id); h.listed = true; h.federation_slug = "my-world"; h.settings.federation = { enabled: true, slug: "my-world" }; },
    cancel_host_update: ({ id }) => { const h = hostBy(id); h.update.pending = false; h.update.message = "Update canceled."; },
    approve_host_update: ({ id }) => { const h = hostBy(id); h.update.pending = true; h.update.message = `Empyrean ${h.update.version} is downloaded. It goes in the next time this world stops or starts.`; },
    rollback_host_update: ({ id }) => { hostBy(id).update = { message: "Back on Empyrean 0.3.3, with the databases as they were before the update." }; },
    check_components: () => {
      for (const c of components) { const r = RELEASES[c.product]; c.latest = r.version; c.latest_notes = r.notes; c.latest_size = r.size; c.offer = offerOf(c); }
    },
    get_component: ({ product }) => {
      const c = components.find((c) => c.product === product);
      const r = RELEASES[product];
      c.latest = r.version;
      c.installed = { version: r.version, notes: r.notes };
      c.message = `${product === "empyrean" ? "Empyrean" : "Lyceum"} ${r.version} is ready.`;
      c.offer = offerOf(c);
      if (product === "lyceum") {
        // Two in Lyceum's folder, one made with an older Lyceum, which Upgrade brings up to date;
        // and the sample, opened from a checkout.
        c.worlds = [
          lyWorld("C:\\Users\\you\\Documents\\Lyceum\\holtburg-hills\\world.toml", true, { facts: { terrain: 4, statics: 23, buildings: 2, dungeons: 1, rooms: 14, spawns: 6, weenies: 3, textures: 1, models: 0 } }),
          lyWorld("C:\\Users\\you\\Documents\\Lyceum\\arwic-markets\\world.toml", true, { outdated: true, era: "infiltration", facts: { terrain: 0, statics: 0 } }),
          lyWorld("C:\\code\\dereth\\lyceum\\sample\\world.toml", false, { key: "sample-world", facts: { terrain: 1, statics: 6, spawns: 1, weenies: 1, textures: 1 } }),
        ];
      }
    },
    remove_component: ({ product }) => { const c = components.find((c) => c.product === product); c.installed = null; c.offer = offerOf(c); c.message = product === "empyrean" ? "Removed Empyrean. Your worlds are kept." : "Removed Lyceum. Your worlds are kept."; },
    open_lyceum: () => { message = { kind: "info", text: "The app opens Lyceum here." }; },
    open_worlds_folder: () => {},
    choose_lyceum_world: () => {},
    upgrade_lyceum_world: ({ world }) => { const c = components.find((c) => c.product === "lyceum"); c.worlds.find((w) => w.path === world).outdated = false; c.message = "2 source file(s) upgraded; world.lock is unchanged."; },
    delete_lyceum_world: ({ world }) => { const c = components.find((c) => c.product === "lyceum"); c.worlds = c.worlds.filter((w) => w.path !== world); },
    export_lyceum_world: ({ world, target }) => {
      const c = components.find((c) => c.product === "lyceum");
      const name = world.split("\\").slice(-2, -1)[0];
      const server = { empyrean: "Empyrean", ace: "ACE", gdle: "GDLE" }[target];
      c.message = `Exported ${name} for ${server} to C:\\Users\\you\\Documents\\${name}-${target}.`;
      c.error = false;
    },
    snapshot: () => ({
      hosts, components, local: looked ? local : [], sharing, join_waiting: !!join, worlds: allWorlds(), list_state: "loaded", list_error: null, list_fetched_at: Math.floor(Date.now() / 1000) - 3600, eras, features,
      state: structuredClone(state), dereth, running: [], job, message,
      vault_name: "Windows Credential Manager", platform, library_dir: "C:\\Users\\you\\AppData\\Local\\Dereth\\launcher\\library",
      update: { kind: "none" }, version: "0.1.0", shortcut: platform !== "macos", development,
      interface: { chosen: chosenInterface, offers: interfaceOffers, file: "C:\\Users\\you\\AppData\\Roaming\\Dereth\\client\\UserPreferences.ini" },
      plugins: [],
    }),
    refresh: () => {},
    install_plugin: () => { throw "The demo installs no plugins."; },
    update_plugins: () => "No plugins are installed.",
    accept_plugin: () => { throw "The demo has no plugins."; },
    discard_plugin_release: () => { throw "The demo has no plugins."; },
    remove_plugin: () => { throw "The demo has no plugins."; },
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
    add_custom_world: ({ server: { name, host, port, ruleset, emulator } }) => {
      if (!host.trim()) throw "Enter the world's host name or address.";
      const p = Number(port);
      if (!(p >= 1 && p <= 65535)) throw "The port must be a number from 1 to 65535.";
      const slug = `custom-${state.custom_worlds.length + 1}`;
      state.custom_worlds.push({ slug, name: name.trim() || host.trim(), host: host.trim(), port: p, ruleset: ruleset ?? null, emulator: emulator ?? "unknown" });
      return slug;
    },
    probe_world: () => {},
    remove_custom_world: ({ slug }) => { state.custom_worlds = state.custom_worlds.filter((c) => c.slug !== slug); },
    set_interface: ({ name }) => {
      const offer = interfaceOffers.find((o) => o.name.toLowerCase() === String(name).toLowerCase());
      if (!offer) throw `${name} is not an interface the Dereth client has.`;
      chosenInterface = offer.name;
    },
    set_discord_presence: ({ on }) => { state.settings.discord_presence = on; },
    dismiss_message: () => { message = null; },
    open_url: ({ url }) => { window.open(url, "_blank"); },
    create_desktop_shortcut: () => "C:\\Users\\you\\Desktop\\Dereth.lnk",
    restart_to_update: () => {},
    // No pad moves about the demo page.
    pad_target: () => null,
  };

  // Every command the page sends, in order, for the page's own tests to read.
  window.demoCalls = [];
  window.demoBackend = async (cmd, args = {}) => {
    window.demoCalls.push([cmd, structuredClone(args)]);
    const h = handlers[cmd];
    if (!h) throw `demo: no ${cmd}`;
    return structuredClone(h(args));
  };
})();
