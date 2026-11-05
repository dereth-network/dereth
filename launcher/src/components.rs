//! Empyrean and Lyceum, and Asheron's Call 2's Yalain and Athenaeum, fetched only when asked; and
//! the game client the launcher's package does not carry, Auberean beside Dereth's launcher or
//! Dereth beside Auberean's.
//!
//! **A game's client comes with its launcher.** Each game's package is the same launcher with that
//! game's client beside it, so the client already here is the packaged one ([`View::bundled`]).
//! The other game's client is fetched from that game's release, which is its launcher package:
//! the client is taken out of it (out of an AppImage, by having it unpack itself) and the rest
//! left unused.
//!
//! **The packaged client is copied out** at the first start ([`adopt_bundled`]) and run from the
//! components folder like any other. An update replaces the launcher's whole bundle on macOS and
//! Linux, and a launcher release holds no client, so a client left inside would be deleted by it.
//! A packaged client is used only while its copy can't be made.
//!
//! Each is its own newest GitHub release ([`dereth_launch::components`]): the page shows that
//! version before anything is downloaded, and the newer one when there is one. Getting it
//! downloads this system's file and the release's `SHA256SUMS`, checks one against the other,
//! unpacks it into a staging folder (an AppImage, which is the program itself, is put there as
//! it is) and moves that into `components/<product>/<version>` in the launcher's data folder.
//! `current.json` there names the version in use. A failed download or unpack leaves the version
//! in use as it was.
//!
//! A development build (one the release packaging did not make: [`DEVELOPMENT`]) downloads
//! nothing it can find built in its own checkout: it uses the newest build of the program beside
//! the launcher or in the checkout's `target/release`, `target/test-release` or `target/debug`.
//! Started with [`DOWNLOADS_SWITCH`], it looks for none and downloads as a release does.

use dereth_launch::components::{self, Offer, Product};
use dereth_launch::releases::{self, Need};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tauri::State;

/// Whether this launcher is a development build: the release packaging stamps the commit it built
/// from into every launcher it makes, and nothing else does.
pub const DEVELOPMENT: bool = option_env!("DERETH_BUILD_COMMIT").is_none();

/// The command-line switch that makes a development build fetch Empyrean and Lyceum as a release
/// does, passing over its checkout's builds, so the download flow can be tried from a checkout.
pub const DOWNLOADS_SWITCH: &str = "--downloads";

/// Whether the launcher uses its checkout's builds: a development build started without
/// [`DOWNLOADS_SWITCH`]. A release never does.
#[must_use]
pub fn uses_checkout(development: bool, args: &[String]) -> bool {
    development && !args.iter().any(|a| a == DOWNLOADS_SWITCH)
}

/// The folder in the person's Documents that Lyceum's new worlds go in.
pub const WORLDS_FOLDER: &str = "Lyceum";

/// How long a list of releases is reused before GitHub is asked again.
const LIST_KEPT: Duration = dereth_launch::releases::LIST_KEPT;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Installed {
    pub version: String,
    pub executable: PathBuf,
    #[serde(default)]
    pub notes: String,
}

#[derive(Clone, Serialize)]
pub struct View {
    pub product: Product,
    /// The downloaded copy, when there is one.
    pub installed: Option<Installed>,
    /// In a development build, this checkout's own build of the program, used instead.
    pub checkout: Option<PathBuf>,
    /// The newest release and its notes, once GitHub has been asked.
    pub latest: Option<String>,
    pub latest_notes: String,
    /// The size of its file for this system, when GitHub said.
    pub latest_size: Option<u64>,
    pub offer: Offer,
    pub checking: bool,
    pub busy: bool,
    /// Bytes downloaded so far, and of how many when GitHub said.
    pub progress: Option<(u64, Option<u64>)>,
    pub message: Option<String>,
    pub error: bool,
    pub running: bool,
    /// In a development build, the version of this checkout's build, once it has said.
    pub checkout_version: Option<String>,
    /// `installed` is the game client that came beside the launcher in its package, not a
    /// download: there is nothing to remove.
    pub bundled: bool,
    /// Lyceum's worlds: those in its folder for worlds, then the others it or the launcher
    /// opened, newest first, each once.
    pub worlds: Vec<LyceumWorld>,
    /// Lyceum's folder for new worlds.
    pub home: Option<PathBuf>,
}

/// One of Lyceum's worlds, as the page lists it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LyceumWorld {
    /// Its world file.
    pub path: PathBuf,
    /// It is in Lyceum's folder for worlds, rather than opened from somewhere else.
    pub home: bool,
    /// Its key, and the era whose files it is built on (`eor` or `infiltration`).
    pub key: Option<String>,
    pub era: String,
    /// When it was last saved, in seconds since 1970.
    pub saved: Option<u64>,
    /// Its files are in an older format than this Lyceum's, which Upgrade brings up to it.
    pub outdated: bool,
    /// What it changes, counted, as `lyceum facts` says; none until Lyceum has said, or from a
    /// Lyceum without the command.
    pub facts: Option<serde_json::Value>,
}

struct Entry {
    view: View,
    children: Vec<Child>,
}

pub struct Store {
    root: PathBuf,
    entries: Vec<Entry>,
    /// Where a development build looks for its checkout's programs; empty in a release.
    builds: Vec<PathBuf>,
    checked: Option<Instant>,
    formats: Formats,
    facts: Facts,
    /// What each checkout build said its version is, by the program and when it was written.
    versions: HashMap<PathBuf, (Option<SystemTime>, Option<String>)>,
    /// The game client that came beside the launcher in its package, by product; none in a
    /// development build.
    bundled: HashMap<Product, PathBuf>,
    versions_asking: bool,
    /// The worlds listed a moment ago, with their stamps: the list is read again every few
    /// seconds, or at once after the launcher opens or makes a world.
    listed: Option<(Instant, Vec<Listed>)>,
}

/// A world on the list, with when its sources last changed.
#[derive(Clone)]
struct Listed {
    world: PathBuf,
    home: bool,
    stamp: crate::lyceum_world::Stamp,
}

/// How long the list of Lyceum's worlds is reused before it is read again.
const LIST_REREAD: Duration = Duration::from_secs(3);

/// What `lyceum facts` said of each world, by its world file, with the stamp of the command and
/// the world's sources it was asked under.
#[derive(Default)]
struct Facts {
    said: HashMap<PathBuf, (FactsStamp, Option<serde_json::Value>)>,
    asking: bool,
}

/// When the `lyceum` command was written, and the world's sources' stamp.
type FactsStamp = (Option<SystemTime>, crate::lyceum_world::Stamp);

/// What `lyceum facts` said of each world it was asked about, by its world file.
type KnownFacts = HashMap<PathBuf, Option<serde_json::Value>>;

/// The launcher's own list of the Lyceum worlds it opened, newest first, in its data folder: a
/// world opened here is listed at once, whatever Lyceum itself remembers.
const OPENED_FILE: &str = "lyceum-worlds.json";

/// How many worlds the launcher's own list keeps.
const OPENED_KEPT: usize = 20;

/// When a world's files and the `lyceum` command last changed: the command, the world file, and
/// each of its packs' `pack.toml`.
type Stamp = Vec<Option<SystemTime>>;

/// What `lyceum upgrade --check` said of each recent world, by its world file, with the [`Stamp`]
/// it was asked under: a world is asked again once its files or the command change.
#[derive(Default)]
struct Formats {
    said: HashMap<PathBuf, (Stamp, bool)>,
    asking: bool,
}

pub type Shared = Arc<Mutex<Store>>;

fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, Store> {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn selection(root: &Path, product: Product) -> PathBuf {
    root.join("components")
        .join(product.name())
        .join("current.json")
}

fn read_installed(root: &Path, product: Product) -> Option<Installed> {
    let data = std::fs::read(selection(root, product)).ok()?;
    let installed: Installed = serde_json::from_slice(&data).ok()?;
    let dir = components::install_dir(root, product, &installed.version).ok()?;
    if !installed.executable.starts_with(&dir) || !installed.executable.is_file() {
        return None;
    }
    Some(installed)
}

/// The program's file name on this system.
fn program_file(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// The folders a development build looks in for its checkout's programs: the launcher's own
/// folder (`here`, the target folder when the launcher was built there too), then the checkout's
/// `target/release`, `target/test-release` and `target/debug`, the checkout being the folder above
/// `here` that holds the workspace.
pub fn build_folders(here: &Path) -> Vec<PathBuf> {
    let mut out = vec![here.to_path_buf()];
    if let Some(workspace) = here
        .ancestors()
        .find(|a| a.join("Cargo.toml").is_file() && a.join("dereth/client/Cargo.toml").is_file())
    {
        for profile in ["release", "test-release", "debug"] {
            let dir = workspace.join("target").join(profile);
            if !out.contains(&dir) {
                out.push(dir);
            }
        }
    }
    out
}

/// The newest build of `program` in `folders`, by when it was written.
pub fn newest_build(folders: &[PathBuf], program: &str) -> Option<PathBuf> {
    let file = program_file(program);
    folders
        .iter()
        .map(|d| d.join(&file))
        .filter_map(|p| {
            let modified = std::fs::metadata(&p).ok()?.modified().ok()?;
            p.is_file().then_some((modified, p))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, p)| p)
}

/// The game client that came beside the launcher in `here` in its package: a release's launcher
/// has its own game's beside it (in `Dereth.app`, in `Contents/MacOS`; in the AppImage, in its
/// `usr/bin`).
pub fn bundled_clients(here: &Path) -> HashMap<Product, PathBuf> {
    Product::ALL
        .into_iter()
        .filter(|p| p.is_game_client())
        .filter_map(|p| {
            let program = here.join(program_file(p.program()));
            program.is_file().then_some((p, program))
        })
        .collect()
}

/// The client to use of the packaged one (`bundled`, at `bundled_version` once it has said) and
/// the copy in the components folder (`installed`): the copy, unless the packaged one is newer.
fn in_use(
    installed: Option<&Installed>,
    bundled: Option<(&Path, Option<&str>)>,
) -> Option<(Installed, bool)> {
    let Some((path, version)) = bundled else {
        return installed.map(|i| (i.clone(), false));
    };
    let packaged = Installed {
        version: version.unwrap_or_default().to_owned(),
        executable: path.to_path_buf(),
        notes: String::new(),
    };
    // The packaged client is used only when it's newer than the copy, which it is for a moment
    // when a newer package has just started, or when its copy couldn't be made.
    let older = |i: &Installed| match (
        components::version(&i.version),
        version.and_then(components::version),
    ) {
        (Some(theirs), Some(ours)) => theirs < ours,
        _ => false,
    };
    match installed {
        Some(i) if !older(i) => Some((i.clone(), false)),
        _ => Some((packaged, true)),
    }
}

/// The store, over the launcher's data folder `root`. A launcher that uses its checkout's builds
/// ([`uses_checkout`]) looks for them from the launcher's folder `here`; a release finds its
/// packaged game client there.
pub fn new(root: PathBuf, here: &Path, checkout: bool) -> Shared {
    let entries = Product::ALL
        .into_iter()
        .map(|product| Entry {
            view: View {
                product,
                installed: read_installed(&root, product),
                checkout: None,
                latest: None,
                latest_notes: String::new(),
                latest_size: None,
                offer: Offer::None,
                checking: false,
                busy: false,
                progress: None,
                message: None,
                error: false,
                running: false,
                checkout_version: None,
                bundled: false,
                worlds: Vec::new(),
                home: None,
            },
            children: Vec::new(),
        })
        .collect();
    let (builds, bundled) = if checkout {
        (build_folders(here), HashMap::new())
    } else {
        (Vec::new(), bundled_clients(here))
    };
    Arc::new(Mutex::new(Store {
        root,
        entries,
        builds,
        checked: None,
        formats: Formats::default(),
        facts: Facts::default(),
        versions: HashMap::new(),
        versions_asking: false,
        bundled,
        listed: None,
    }))
}

/// Whether this launcher looks for its checkout's builds before downloading anything.
pub fn checkout_builds(shared: &Shared) -> bool {
    !lock(shared).builds.is_empty()
}

/// Lyceum's folder for new worlds, which the page shows and the world picker starts in.
pub fn set_worlds_home(shared: &Shared, home: Option<PathBuf>) {
    entry(&mut lock(shared), Product::Lyceum).view.home = home;
}

pub fn views(shared: &Shared) -> Vec<View> {
    let mut store = lock(shared);
    let builds = store.builds.clone();
    let lyceum = lyceum_command(&store).ok();
    let home = entry(&mut store, Product::Lyceum).view.home.clone();
    if !store
        .listed
        .as_ref()
        .is_some_and(|(at, _)| at.elapsed() < LIST_REREAD)
    {
        let opened = opened_worlds(&store.root);
        let listed = list_worlds(home.as_deref(), &opened)
            .into_iter()
            .map(|(world, home)| Listed {
                stamp: crate::lyceum_world::stamp(&world),
                world,
                home,
            })
            .collect();
        store.listed = Some((Instant::now(), listed));
    }
    let listed = store
        .listed
        .as_ref()
        .map(|(_, l)| l.clone())
        .unwrap_or_default();
    let mut ask = Vec::new();
    let mut ask_facts = Vec::new();
    let mut ask_versions = Vec::new();
    let Store {
        entries,
        formats,
        facts,
        versions,
        bundled,
        ..
    } = &mut *store;
    for e in entries.iter_mut() {
        e.children
            .retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));
        e.view.running = !e.children.is_empty();
        e.view.checkout = newest_build(&builds, e.view.product.program());
        e.view.checkout_version = e.view.checkout.as_ref().and_then(|program| {
            let written = modified(program);
            match versions.get(program) {
                Some((then, version)) if *then == written => version.clone(),
                _ => {
                    ask_versions.push((e.view.product, program.clone(), written));
                    None
                }
            }
        });
        // The packaged client's version: what it says, asked once per build of it.
        let packaged = bundled.get(&e.view.product).map(|program| {
            let written = modified(program);
            let version = match versions.get(program) {
                Some((then, version)) if *then == written => version.clone(),
                _ => {
                    ask_versions.push((e.view.product, program.clone(), written));
                    None
                }
            };
            (program.clone(), version)
        });
        let using = in_use(
            e.view.installed.as_ref(),
            packaged.as_ref().map(|(p, v)| (p.as_path(), v.as_deref())),
        );
        e.view.bundled = using.as_ref().is_some_and(|(_, b)| *b);
        let have = using.as_ref().map(|(i, _)| i.version.clone());
        e.view.offer = if e.view.checkout.is_some() {
            Offer::None
        } else {
            components::offer(
                have.as_deref().filter(|v| !v.is_empty()),
                e.view.latest.as_deref(),
            )
        };
        if e.view.product == Product::Lyceum {
            let paths: Vec<PathBuf> = listed.iter().map(|l| l.world.clone()).collect();
            let stamp = |w: &Path| lyceum.as_deref().map(|l| format_stamp(l, w));
            let (outdated, stale) = sort_formats(&paths, &formats.said, stamp);
            ask = stale;
            let command = lyceum.as_deref().map(modified);
            let (known, stale) = sort_facts(&listed, &facts.said, command);
            ask_facts = stale;
            e.view.worlds = listed
                .iter()
                .map(|l| {
                    let header = crate::lyceum_world::header(&l.world);
                    LyceumWorld {
                        path: l.world.clone(),
                        home: l.home,
                        key: header.key,
                        era: header.era,
                        saved: crate::lyceum_world::saved(&l.stamp),
                        outdated: outdated.contains(&l.world),
                        facts: known.get(&l.world).cloned().flatten(),
                    }
                })
                .collect();
        }
    }
    if let (Some(lyceum), false, false) = (lyceum.clone(), ask.is_empty(), formats.asking) {
        formats.asking = true;
        let shared = shared.clone();
        std::thread::spawn(move || {
            for (world, stamp) in ask {
                let outdated = command(&lyceum)
                    .arg("upgrade")
                    .arg(&world)
                    .arg("--check")
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .and_then(|o| needs_upgrade(&String::from_utf8_lossy(&o.stdout)))
                    .unwrap_or(false);
                lock(&shared).formats.said.insert(world, (stamp, outdated));
            }
            lock(&shared).formats.asking = false;
        });
    }
    if let (Some(lyceum), false, false) = (lyceum, ask_facts.is_empty(), facts.asking) {
        facts.asking = true;
        let shared = shared.clone();
        std::thread::spawn(move || {
            for (world, stamp) in ask_facts {
                let said = command(&lyceum)
                    .arg("facts")
                    .arg(&world)
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .and_then(|o| facts_from(&String::from_utf8_lossy(&o.stdout)));
                lock(&shared).facts.said.insert(world, (stamp, said));
            }
            lock(&shared).facts.asking = false;
        });
    }
    if !ask_versions.is_empty() && !store.versions_asking {
        store.versions_asking = true;
        let shared = shared.clone();
        std::thread::spawn(move || {
            for (product, program, written) in ask_versions {
                let version = program_version(product, &program);
                lock(&shared).versions.insert(program, (written, version));
            }
            lock(&shared).versions_asking = false;
        });
    }
    let bundled = store.bundled.clone();
    let versions = store.versions.clone();
    store
        .entries
        .iter()
        .map(|e| {
            let mut view = e.view.clone();
            if view.bundled {
                view.installed = packaged_view(&bundled, &versions, &view)
                    .map(|(i, _)| i)
                    .or(view.installed);
            }
            view
        })
        .collect()
}

/// The client `view`'s product runs as, of its packaged and downloaded ones ([`in_use`]), with
/// whether it is the packaged one.
fn packaged_view(
    bundled: &HashMap<Product, PathBuf>,
    versions: &HashMap<PathBuf, (Option<SystemTime>, Option<String>)>,
    view: &View,
) -> Option<(Installed, bool)> {
    let packaged = bundled.get(&view.product).map(|program| {
        let version = versions.get(program).and_then(|(_, v)| v.clone());
        (program.clone(), version)
    });
    in_use(
        view.installed.as_ref(),
        packaged.as_ref().map(|(p, v)| (p.as_path(), v.as_deref())),
    )
}

/// When `file` was last written.
fn modified(file: &Path) -> Option<SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}

/// What a checkout's build of `product` at `program` says its version is: Empyrean's server in
/// its release facts, Lyceum by its `lyceum` command beside the editor, and Asheron's Call 2's
/// programs by their own `--version`.
fn program_version(product: Product, program: &Path) -> Option<String> {
    let (program, arg) = match product {
        Product::Empyrean => (program.to_path_buf(), "--release-facts"),
        Product::Lyceum => (program.with_file_name(program_file("lyceum")), "--version"),
        Product::Dereth | Product::Auberean | Product::Yalain | Product::Athenaeum => {
            (program.to_path_buf(), "--version")
        }
    };
    let output = command(&program).arg(arg).output().ok()?;
    if !output.status.success() {
        return None;
    }
    version_from(product, &String::from_utf8_lossy(&output.stdout))
}

/// The version in what a program prints of itself: the `version` of Empyrean's release facts
/// (one line of JSON), or the first line of `lyceum --version` (`lyceum 0.1.0`), of the Dereth
/// client's (`dereth-client 0.4.9`) and of each of Asheron's Call 2's programs' (`yalain 0.1.0`).
fn version_from(product: Product, said: &str) -> Option<String> {
    let version = match product {
        Product::Empyrean => serde_json::from_str::<serde_json::Value>(said.trim())
            .ok()?
            .get("version")?
            .as_str()?
            .to_owned(),
        Product::Lyceum => said
            .lines()
            .next()?
            .trim()
            .strip_prefix("lyceum ")?
            .trim()
            .to_owned(),
        Product::Dereth | Product::Auberean | Product::Yalain | Product::Athenaeum => said
            .lines()
            .next()?
            .trim()
            .strip_prefix(product.program())?
            .trim()
            .to_owned(),
    };
    components::version(&version).map(|_| version)
}

/// What `lyceum facts` prints of a world: one JSON object of what it changes, counted.
pub(crate) fn facts_from(said: &str) -> Option<serde_json::Value> {
    said.lines()
        .rev()
        .find_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .filter(serde_json::Value::is_object)
}

/// Of `worlds`: what `lyceum facts` last said of each, under the stamp it has now; and those to
/// ask about, never asked or changed since, with their stamps. `command` is when the `lyceum`
/// command was written; with none (no command), nothing is asked.
fn sort_facts(
    worlds: &[Listed],
    said: &HashMap<PathBuf, (FactsStamp, Option<serde_json::Value>)>,
    command: Option<Option<SystemTime>>,
) -> (KnownFacts, Vec<(PathBuf, FactsStamp)>) {
    let mut known = HashMap::new();
    let mut ask = Vec::new();
    let Some(command) = command else {
        return (known, ask);
    };
    for w in worlds {
        let now = (command, w.stamp.clone());
        match said.get(&w.world) {
            Some((then, facts)) if *then == now => {
                known.insert(w.world.clone(), facts.clone());
            }
            _ => ask.push((w.world.clone(), now)),
        }
    }
    (known, ask)
}

/// What a world is, the same for every spelling of it: its canonical path where it is there.
fn identity(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// The worlds the page lists: those in Lyceum's folder `home` (each a folder of it holding a
/// `world.toml`), the ones opened lately first and the rest by name; then the others in
/// `opened`, the worlds Lyceum or the launcher opened, newest first. Each is listed once, as a
/// person writes it, with whether it is in `home`; a world no longer there is not.
pub(crate) fn list_worlds(home: Option<&Path>, opened: &[PathBuf]) -> Vec<(PathBuf, bool)> {
    let home_id = home.map(identity);
    let in_home = |world: &Path| {
        home_id
            .as_ref()
            .is_some_and(|h| identity(world).starts_with(h))
    };
    let mut seen = HashSet::new();
    let opened: Vec<PathBuf> = opened
        .iter()
        .map(|p| {
            if p.is_dir() {
                p.join("world.toml")
            } else {
                p.clone()
            }
        })
        .filter(|p| p.is_file())
        .filter(|p| seen.insert(identity(p)))
        .map(|p| plain_path(&p))
        .collect();
    let mut found: Vec<PathBuf> = home
        .and_then(|h| std::fs::read_dir(h).ok())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with(DELETING))
        .map(|e| e.path().join("world.toml"))
        .filter(|p| p.is_file())
        .filter(|p| seen.insert(identity(p)))
        .map(|p| plain_path(&p))
        .collect();
    found.sort();
    let (mut first, elsewhere): (Vec<PathBuf>, Vec<PathBuf>) =
        opened.into_iter().partition(|w| in_home(w));
    first.extend(found);
    first
        .into_iter()
        .map(|w| (w, true))
        .chain(elsewhere.into_iter().map(|w| (w, false)))
        .collect()
}

/// The worlds Lyceum and the launcher opened, newest first: Lyceum's own recent worlds, then any
/// others the launcher opened, from its list in its data folder `root`.
fn opened_worlds(root: &Path) -> Vec<PathBuf> {
    let mut worlds = recent_worlds();
    worlds.extend(launcher_opened(root));
    worlds
}

/// The launcher's own list of the worlds it opened in Lyceum, newest first.
fn launcher_opened(root: &Path) -> Vec<PathBuf> {
    std::fs::read(root.join(OPENED_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Keep `world` first on the launcher's own list of the worlds it opened, once.
fn remember_opened(root: &Path, world: &Path) {
    let mut worlds = launcher_opened(root);
    let same = identity(world);
    worlds.retain(|w| identity(w) != same);
    worlds.insert(0, plain_path(world));
    worlds.truncate(OPENED_KEPT);
    if let Ok(bytes) = serde_json::to_vec_pretty(&worlds) {
        let _ = std::fs::create_dir_all(root);
        let _ = std::fs::write(root.join(OPENED_FILE), bytes);
    }
}

/// When `world`'s files and the `lyceum` command last changed.
fn format_stamp(lyceum: &Path, world: &Path) -> Stamp {
    let folder = world.parent().unwrap_or(Path::new("."));
    let packs: Vec<PathBuf> = std::fs::read_to_string(world)
        .ok()
        .and_then(|t| t.parse::<toml::Table>().ok())
        .and_then(|t| t.get("packs")?.as_array().cloned())
        .into_iter()
        .flatten()
        .filter_map(|p| p.as_str().map(|p| folder.join(p).join("pack.toml")))
        .collect();
    [lyceum, world]
        .into_iter()
        .chain(packs.iter().map(PathBuf::as_path))
        .map(|f| std::fs::metadata(f).and_then(|m| m.modified()).ok())
        .collect()
}

/// Of `worlds`: those whose last check, under the stamp they have now, found their files older
/// than this Lyceum's format; and those to ask about, never asked or changed since, with their
/// stamps. With no `lyceum` command (no stamp), a world is neither.
fn sort_formats(
    worlds: &[PathBuf],
    said: &HashMap<PathBuf, (Stamp, bool)>,
    stamp: impl Fn(&Path) -> Option<Stamp>,
) -> (Vec<PathBuf>, Vec<(PathBuf, Stamp)>) {
    let mut outdated = Vec::new();
    let mut ask = Vec::new();
    for w in worlds {
        let Some(now) = stamp(w) else { continue };
        match said.get(w) {
            Some((then, old)) if *then == now => {
                if *old {
                    outdated.push(w.clone());
                }
            }
            _ => ask.push((w.clone(), now)),
        }
    }
    (outdated, ask)
}

/// Whether `lyceum upgrade --check` found files to upgrade, from what it prints: the number of
/// source files that need upgrading, first. `None` for anything else.
fn needs_upgrade(said: &str) -> Option<bool> {
    let count: usize = said.split_whitespace().next()?.parse().ok()?;
    said.contains("need upgrading").then_some(count > 0)
}

fn entry(store: &mut Store, product: Product) -> &mut Entry {
    store
        .entries
        .iter_mut()
        .find(|e| e.view.product == product)
        .expect("known component")
}

/// A failed request in words for a person: offline, rate-limited, or something GitHub said.
pub(crate) fn plain(error: &ureq::Error) -> String {
    match error {
        ureq::Error::StatusCode(403 | 429) => components::RATE_LIMITED.to_owned(),
        ureq::Error::StatusCode(404) => {
            "GitHub no longer has this file. Check again for the newest release.".to_owned()
        }
        ureq::Error::StatusCode(code) => {
            format!("GitHub answered with an error ({code}). Try again later.")
        }
        ureq::Error::BodyExceedsLimit(_) => {
            "The download was larger than expected, so it was stopped.".to_owned()
        }
        ureq::Error::Io(_)
        | ureq::Error::Timeout(_)
        | ureq::Error::HostNotFound
        | ureq::Error::ConnectionFailed
        | ureq::Error::Tls(_) => {
            "Could not reach GitHub. Check your internet connection and try again.".to_owned()
        }
        other => format!("The download failed: {other}"),
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        // Offline is said soon; a slow download is not cut short, but nothing waits forever.
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        .timeout_global(Some(Duration::from_secs(60 * 60)))
        .build()
        .into()
}

/// Download `url`, at most `limit` bytes, telling `progress` how far it has got.
pub(crate) fn download(
    url: &str,
    limit: u64,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<Vec<u8>, String> {
    use std::io::Read;
    if !url.starts_with("https://") {
        return Err("Downloads need HTTPS.".into());
    }
    let mut response = agent()
        .get(url)
        .header("User-Agent", "Dereth-launcher")
        .call()
        .map_err(|e| plain(&e))?;
    let total = response.body().content_length();
    if total.is_some_and(|t| t > limit) {
        return Err("The download was larger than expected, so it was stopped.".into());
    }
    let mut reader = response.body_mut().with_config().limit(limit).reader();
    let mut bytes = Vec::with_capacity(total.unwrap_or(0).min(limit) as usize);
    let mut buf = vec![0; 256 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|_| {
            "The download stopped part way. Check your internet connection and try again."
                .to_owned()
        })?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        progress(bytes.len() as u64, total);
    }
    Ok(bytes)
}

/// The repository's releases, as GitHub's releases API lists them, read as far as `needs` take
/// it: asked at most once in [`LIST_KEPT`] unless `fresh`, so opening a page, a download and every
/// server's update check share their requests ([`dereth_launch::releases::kept_list`]).
pub(crate) fn release_list(fresh: bool, needs: &[Need]) -> Result<Arc<Vec<u8>>, String> {
    static LIST: Mutex<Option<releases::Kept>> = Mutex::new(None);
    let mut kept = LIST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let api = releases::releases_api_url(releases::SOURCE_URL)
        .ok_or("This launcher names no GitHub repository to download from.")?;
    releases::kept_list(
        &mut kept,
        Instant::now(),
        fresh,
        &api,
        needs,
        &mut list_page,
    )
}

/// One page of the releases API's list at `url`, and the next page's address.
fn list_page(url: &str) -> Result<releases::Page, String> {
    if !url.starts_with("https://") {
        return Err("Downloads need HTTPS.".into());
    }
    let mut response = agent()
        .get(url)
        .header("User-Agent", "Dereth-launcher")
        .call()
        .map_err(|e| plain(&e))?;
    let next = response
        .headers()
        .get("link")
        .and_then(|l| l.to_str().ok())
        .and_then(releases::next_page);
    let body = response
        .body_mut()
        .with_config()
        .limit(16 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| plain(&e))?;
    Ok(releases::Page { body, next })
}

pub(crate) fn target() -> &'static str {
    env!("DERETH_COMPONENT_TARGET")
}

/// The one copy of `product`'s program in the unpacked folder `root`.
pub(crate) fn find_executable(root: &Path, product: Product) -> Result<PathBuf, String> {
    let name = program_file(product.program());
    let mut found = Vec::new();
    fn visit(dir: &Path, name: &str, depth: usize, found: &mut Vec<PathBuf>) -> Result<(), String> {
        if depth > 6 {
            return Ok(());
        }
        for e in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
            let e = e.map_err(|e| e.to_string())?;
            let kind = e.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                visit(&e.path(), name, depth + 1, found)?;
            } else if kind.is_file() && e.file_name() == name {
                found.push(e.path());
            }
        }
        Ok(())
    }
    visit(root, &name, 0, &mut found)?;
    if found.len() != 1 {
        return Err(format!("The download must hold one {name}."));
    }
    Ok(found.remove(0))
}

/// Ask GitHub for each product's newest release, for the page to offer, unless it was asked a
/// moment ago. The page asks as it opens Host a world or Build a world.
#[tauri::command]
pub fn check_components(shared: State<'_, Shared>, force: bool) {
    let shared = shared.inner().clone();
    {
        let mut store = lock(&shared);
        let recent = store.checked.is_some_and(|at| at.elapsed() < LIST_KEPT);
        if store.entries.iter().any(|e| e.view.checking) || (recent && !force) {
            return;
        }
        store.checked = Some(Instant::now());
        for e in &mut store.entries {
            e.view.checking = true;
        }
    }
    std::thread::spawn(move || {
        let list = release_list(force, &Product::ALL.map(Product::newest_release));
        let mut store = lock(&shared);
        for e in &mut store.entries {
            e.view.checking = false;
            let newest = list
                .as_deref()
                .map_err(Clone::clone)
                .and_then(|l| components::newest(l, e.view.product));
            match newest {
                Ok(newest) => {
                    if e.view.error {
                        e.view.message = None;
                        e.view.error = false;
                    }
                    e.view.latest_notes =
                        newest.as_ref().map(|r| r.notes.clone()).unwrap_or_default();
                    e.view.latest_size = newest
                        .as_ref()
                        .and_then(|r| r.archive_size(e.view.product, target()));
                    e.view.latest = newest.map(|r| r.version);
                }
                Err(error) => {
                    if !e.view.busy {
                        e.view.message = Some(error);
                        e.view.error = true;
                    }
                }
            }
        }
    });
}

/// Download, check and unpack the newest release of `product`, and use it from now on.
fn get(shared: &Shared, root: &Path, product: Product) -> Result<Installed, String> {
    let list = release_list(false, &[product.newest_release()])?;
    let release = components::newest(&list, product)?.ok_or_else(|| {
        format!(
            "{} has no published release on GitHub yet.",
            product.title()
        )
    })?;
    {
        let mut store = lock(shared);
        let e = entry(&mut store, product);
        e.view.latest = Some(release.version.clone());
        e.view.latest_notes = release.notes.clone();
        e.view.latest_size = release.archive_size(product, target());
        e.view.message = Some(format!(
            "Downloading {} {}...",
            product.title(),
            release.version
        ));
    }
    if let Some(installed) = read_installed(root, product).filter(|i| i.version == release.version)
    {
        return Ok(installed);
    }
    let (name, url) = release.archive(product, target())?;
    let sums = String::from_utf8(download(
        release.asset(components::SUMS_ASSET)?,
        1024 * 1024,
        &mut |_, _| {},
    )?)
    .map_err(|_| "The release's checksums could not be read.")?;
    let sums = components::parse_sums(&sums);
    let bytes = download(url, components::MAX_ARCHIVE_BYTES, &mut |done, total| {
        entry(&mut lock(shared), product).view.progress = Some((done, total));
    })?;
    components::check_sum(&name, &bytes, &sums)?;
    let installed = install_archive(
        root,
        product,
        &release.version,
        &release.notes,
        &name,
        &bytes,
    )?;
    remove_other_versions(root, product, &installed.version);
    Ok(installed)
}

fn install_archive(
    root: &Path,
    product: Product,
    version: &str,
    notes: &str,
    filename: &str,
    bytes: &[u8],
) -> Result<Installed, String> {
    install_staged(root, product, version, notes, |stage| {
        crate::component_archive::extract_program(
            bytes,
            filename,
            &program_file(product.program()),
            stage,
            product.is_game_client(),
        )
    })
}

/// Install `version` of `product`: `fill` writes its files into a new staging folder, which is
/// moved into `components/<product>/<version>` and named as the version in use. A failure leaves
/// the version in use as it was.
fn install_staged(
    root: &Path,
    product: Product,
    version: &str,
    notes: &str,
    fill: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<Installed, String> {
    let dest = components::install_dir(root, product, version)?;
    let parent = dest.parent().ok_or("Invalid component folder")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let guard = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(parent.join("install.lock"))
        .map_err(|e| e.to_string())?;
    guard
        .try_lock()
        .map_err(|_| "Another launcher is changing this right now.")?;
    if dest.exists() {
        if read_installed(root, product).is_some_and(|i| i.version == version) {
            return Err("This version is already here.".into());
        }
        // A crash after the folder was moved into place can leave a complete but unused version.
        std::fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
    }
    let stage = parent.join("staging");
    if stage.exists() {
        if std::fs::symlink_metadata(&stage)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("The staging folder must not be a link.".into());
        }
        std::fs::remove_dir_all(&stage).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir(&stage).map_err(|e| e.to_string())?;
    let prepared = (|| {
        fill(&stage)?;
        let executable = find_executable(&stage, product)?;
        // A preferences file a player keeps beside the client stays beside it, version to version.
        if product.is_game_client() {
            let name = dereth_launch::interface::PREFERENCES_FILE;
            if let Some(kept) = read_installed(root, product)
                .map(|i| i.executable.with_file_name(name))
                .filter(|f| f.is_file())
            {
                copy_plain(&kept, &executable.with_file_name(name))?;
            }
        }
        dress_client(&executable, product, version);
        let relative = executable.strip_prefix(&stage).map_err(|e| e.to_string())?;
        let installed = Installed {
            version: version.to_owned(),
            executable: dest.join(relative),
            notes: notes.to_owned(),
        };
        std::fs::rename(&stage, &dest).map_err(|e| e.to_string())?;
        let pointer = selection(root, product);
        let temp = pointer.with_extension("tmp");
        std::fs::write(
            &temp,
            serde_json::to_vec(&installed).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(temp, pointer).map_err(|e| e.to_string())?;
        Ok(installed)
    })();
    if stage.exists() {
        let _ = std::fs::remove_dir_all(&stage);
    }
    prepared
}

/// Whether this launcher updates itself: any built for release, and any with a packaged client. A
/// launcher that has been updated from the launcher's own releases has no client beside it, so
/// what's beside it can't be all that says so.
pub fn is_release(development: bool, packaged_client: bool) -> bool {
    !development || packaged_client
}

/// Copy `from` to `to`: the bytes and the permissions, not the attributes. A packaged client's copy
/// is then a file the launcher wrote, as a download is, whatever the bundle's own download left on
/// the bundle.
fn copy_plain(from: &Path, to: &Path) -> Result<(), String> {
    let copy = || -> std::io::Result<()> {
        let mut source = std::fs::File::open(from)?;
        let mut target = std::fs::File::create(to)?;
        std::io::copy(&mut source, &mut target)?;
        target.set_permissions(source.metadata()?.permissions())?;
        target.sync_all()
    };
    copy().map_err(|e| format!("{}: {e}", from.display()))
}

/// The files a packaged game client in the launcher's folder `here` needs, each with its path below
/// the component's folder: the program; the plugin author's tool, which the launcher finds beside
/// the client; a preferences file kept beside it; and on macOS the frameworks, which the program
/// finds at `../Frameworks` from its own folder, so they keep that place, in a bundle named for the
/// game ([`crate::client_bundle`]).
fn bundled_files(here: &Path, product: Product) -> Vec<(PathBuf, PathBuf)> {
    let macos = here.ends_with("Contents/MacOS");
    let app = PathBuf::from(format!("{}.app", product.title()));
    let below = if macos {
        app.join("Contents/MacOS")
    } else {
        PathBuf::new()
    };
    let mut files = Vec::new();
    for name in [
        program_file(product.program()),
        dereth_launch::plugins::TOOL_FILE.to_owned(),
        dereth_launch::interface::PREFERENCES_FILE.to_owned(),
    ] {
        let from = here.join(&name);
        if from.is_file() {
            files.push((from, below.join(name)));
        }
    }
    if macos {
        let frameworks = here.join("../Frameworks");
        for e in std::fs::read_dir(frameworks)
            .into_iter()
            .flatten()
            .flatten()
        {
            if e.file_type().is_ok_and(|t| t.is_file()) {
                files.push((
                    e.path(),
                    app.join("Contents/Frameworks").join(e.file_name()),
                ));
            }
        }
    }
    files
}

/// Whether a packaged client at `bundled` should be copied over the installed copy at `installed`:
/// there is none, or the packaged one is newer. Neither version reading means no copy.
fn bundled_is_newer(installed: Option<&str>, bundled: &str) -> bool {
    let Some(ours) = components::version(bundled) else {
        return false;
    };
    match installed {
        None => true,
        Some(i) => components::version(i).is_none_or(|theirs| ours > theirs),
    }
}

/// Copy the game client packaged beside the launcher in `here` into the components folder under
/// `root`, when it isn't there already: the first start of a package, or of a newer one. `said`
/// is the version the program says it is. A copy that fails leaves things as they were, and the
/// packaged client is used.
pub fn adopt_bundled(
    root: &Path,
    here: &Path,
    said: &dyn Fn(Product, &Path) -> Option<String>,
) -> Vec<Result<Installed, String>> {
    let mut out = Vec::new();
    for (product, program) in bundled_clients(here) {
        let Some(version) = said(product, &program) else {
            continue;
        };
        let installed = read_installed(root, product);
        if !bundled_is_newer(installed.as_ref().map(|i| i.version.as_str()), &version) {
            continue;
        }
        let files = bundled_files(here, product);
        let adopted = install_staged(root, product, &version, "", |stage| {
            for (from, below) in &files {
                let to = stage.join(below);
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                copy_plain(from, &to)?;
            }
            Ok(())
        });
        if adopted.is_ok() {
            remove_other_versions(root, product, &version);
        }
        out.push(adopted);
    }
    out
}

/// [`adopt_bundled`] for a starting launcher, asking each program its version, and each client
/// installed before its bundle was made its own ([`crate::client_bundle`]) made so.
pub fn adopt_packaged_client(root: &Path, here: &Path) {
    for failed in adopt_bundled(root, here, &program_version)
        .into_iter()
        .filter_map(Result::err)
    {
        eprintln!("dereth: the packaged client stays in the launcher's bundle: {failed}");
    }
    for product in Product::ALL.into_iter().filter(|p| p.is_game_client()) {
        if let Some(i) = read_installed(root, product) {
            if crate::client_bundle::needs_dressing(&i.executable, product) {
                dress_client(&i.executable, product, &i.version);
            }
        }
    }
}

/// On macOS, make the bundle around a game's client its own, so the Dock shows that game. A failure
/// is only that: the client runs, drawn wrongly.
fn dress_client(executable: &Path, product: Product, version: &str) {
    if !cfg!(target_os = "macos") || !product.is_game_client() {
        return;
    }
    let launcher = std::env::current_exe().ok();
    let Some(here) = launcher.as_deref().and_then(Path::parent) else {
        return;
    };
    if let Err(e) = crate::client_bundle::dress(executable, product, version, here) {
        eprintln!(
            "dereth: {} keeps the bundle it came in: {e}",
            product.title()
        );
    }
}

/// Whether the release at `latest` is newer than the client at `installed`. No copy of the client
/// means none is wanted: the player has removed it, or never got it.
fn client_is_behind(installed: Option<&str>, latest: &str) -> bool {
    match (
        installed.and_then(components::version),
        components::version(latest),
    ) {
        (Some(ours), Some(theirs)) => theirs > ours,
        _ => false,
    }
}

/// Fetch `product`'s newer release when one is published, in the background: how a game's client is
/// kept up to date, apart from the launcher. Asks GitHub, so this waits for it.
pub fn update_client(app: tauri::AppHandle, shared: Shared, product: Product) {
    let root = lock(&shared).root.clone();
    let Ok(list) = release_list(false, &[product.newest_release()]) else {
        return;
    };
    let Ok(Some(release)) = components::newest(&list, product) else {
        return;
    };
    let installed = read_installed(&root, product);
    if client_is_behind(
        installed.as_ref().map(|i| i.version.as_str()),
        &release.version,
    ) {
        let _ = start_get(app, shared, product);
    }
}

/// The downloaded copy of `product` in the launcher's data folder `root`, when there is one.
pub fn downloaded(root: &Path, product: Product) -> Option<PathBuf> {
    read_installed(root, product).map(|i| i.executable)
}

/// Remove every downloaded version of `product` but `keep`: a version no longer in use is only
/// disk space, since each server keeps its own copy of the server it runs.
fn remove_other_versions(root: &Path, product: Product, keep: &str) {
    let Ok(entries) = std::fs::read_dir(root.join("components").join(product.name())) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name != keep
            && components::version(&name).is_some()
            && e.file_type().is_ok_and(|t| t.is_dir())
        {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// Download the newest release of `product` and use it from now on: the first copy, or an
/// update. Nothing changes until the download is checked and unpacked.
#[tauri::command]
pub fn get_component(
    app: tauri::AppHandle,
    shared: State<'_, Shared>,
    product: Product,
) -> Result<(), String> {
    start_get(app, shared.inner().clone(), product)
}

/// [`get_component`], from the launcher itself: the update check fetches a game's newer client
/// this way when its release carries an older launcher.
///
/// # Errors
/// This checkout's build is used instead, or the product is busy or running.
pub fn start_get(app: tauri::AppHandle, shared: Shared, product: Product) -> Result<(), String> {
    let root = {
        let mut store = lock(&shared);
        let root = store.root.clone();
        let builds = store.builds.clone();
        let e = entry(&mut store, product);
        if newest_build(&builds, product.program()).is_some() {
            return Err("This launcher uses this checkout's build.".into());
        }
        if e.view.busy {
            return Err(format!("{} is already downloading.", product.title()));
        }
        if e.view.running {
            return Err(format!("Close {} before updating it.", product.title()));
        }
        e.view.busy = true;
        e.view.error = false;
        e.view.progress = None;
        e.view.message = Some(match &e.view.latest {
            Some(v) => format!("Downloading {} {v}...", product.title()),
            None => "Asking GitHub for the newest release...".to_owned(),
        });
        root
    };
    std::thread::spawn(move || {
        let result = get(&shared, &root, product);
        let mut store = lock(&shared);
        let e = entry(&mut store, product);
        e.view.busy = false;
        e.view.progress = None;
        match result {
            Ok(installed) => {
                e.view.message = Some(format!(
                    "{} {} is ready.",
                    product.title(),
                    installed.version
                ));
                e.view.error = false;
                // The Dereth client plays from the main page, which learns of it at once.
                if product == Product::Dereth {
                    use tauri::Manager;
                    if let Some(b) = app.try_state::<crate::backend::Shared>() {
                        crate::backend::lock(&b).use_dereth_client(&installed.executable);
                    }
                }
                e.view.installed = Some(installed);
            }
            Err(error) => {
                e.view.message = Some(error);
                e.view.error = true;
            }
        }
    });
    Ok(())
}

/// Remove the downloaded copy of `product`. Servers and worlds are kept.
#[tauri::command]
pub fn remove_component(shared: State<'_, Shared>, product: Product) -> Result<(), String> {
    let mut store = lock(&shared);
    let root = store.root.clone();
    let e = entry(&mut store, product);
    if e.view.busy || e.view.running {
        return Err(format!("Close {} before removing it.", product.title()));
    }
    let Some(installed) = e.view.installed.as_ref() else {
        return Ok(());
    };
    let dir = components::install_dir(&root, product, &installed.version)?;
    let parent = dir.parent().ok_or("Invalid component folder")?;
    let guard = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(parent.join("install.lock"))
        .map_err(|e| e.to_string())?;
    guard
        .try_lock()
        .map_err(|_| "Another launcher is changing this right now.")?;
    std::fs::remove_file(selection(&root, product)).map_err(|e| e.to_string())?;
    remove_other_versions(&root, product, "");
    e.view.installed = None;
    e.view.error = false;
    e.view.message = Some(match product {
        Product::Dereth | Product::Auberean => format!("Removed {}.", product.title()),
        _ => format!("Removed {}. Your worlds are kept.", product.title()),
    });
    Ok(())
}

pub fn command(program: &Path) -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    leave_the_mount(&mut command, program);
    command
}

/// A program that runs outside this launcher's AppImage (a copy in the components folder) starts
/// with none of the AppImage's paths: it uses this system's libraries, and outlives the mount. One
/// still inside the mount keeps them.
pub fn leave_the_mount(command: &mut Command, program: &Path) {
    let Some(appdir) = std::env::var_os("APPDIR").filter(|d| !d.is_empty()) else {
        return;
    };
    let appdir = Path::new(&appdir);
    if program.starts_with(appdir) {
        return;
    }
    for (name, value) in dereth_launch::desktop::outside_mount(appdir, std::env::vars_os()) {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
}

/// The program `product` runs as here: this checkout's build in a development build that has
/// one, else the packaged or the downloaded copy, whichever is newer ([`in_use`]).
fn program(store: &Store, product: Product) -> Option<PathBuf> {
    newest_build(&store.builds, product.program()).or_else(|| {
        let view = &store
            .entries
            .iter()
            .find(|e| e.view.product == product)?
            .view;
        packaged_view(&store.bundled, &store.versions, view).map(|(i, _)| i.executable)
    })
}

/// The program `product` runs as here, if it is here: this checkout's build in a development build
/// that has one, else the downloaded copy.
pub fn program_of(shared: &Shared, product: Product) -> Option<PathBuf> {
    program(&lock(shared), product)
}

/// The `lyceum` command beside the editor in use.
fn lyceum_command(store: &Store) -> Result<PathBuf, String> {
    let editor = program(store, Product::Lyceum).ok_or("Download Lyceum first.")?;
    let command = editor.with_file_name(program_file("lyceum"));
    if !command.is_file() {
        return Err(
            if newest_build(&store.builds, Product::Lyceum.program()).is_some() {
                "This checkout has no build of the lyceum command beside the editor.".to_owned()
            } else {
                "This Lyceum has no lyceum command beside its editor.".to_owned()
            },
        );
    }
    Ok(command)
}

/// Open Lyceum on `world` (its `world.toml`, or the folder that holds it), drawn over the game's
/// data files: the ones Lyceum used last, else `dats` (the launcher's own), with the Classic data
/// files `classic` beside them, which a world built on Infiltration's files is drawn from. With no
/// world, Lyceum's start screen.
#[tauri::command]
pub fn open_lyceum(
    shared: State<'_, Shared>,
    world: Option<PathBuf>,
    dats: Option<PathBuf>,
    classic: Option<PathBuf>,
) -> Result<(), String> {
    open_editor(&shared, world, dats, classic)
}

fn open_editor(
    shared: &Shared,
    world: Option<PathBuf>,
    dats: Option<PathBuf>,
    classic: Option<PathBuf>,
) -> Result<(), String> {
    let mut store = lock(shared);
    let editor = program(&store, Product::Lyceum).ok_or("Download Lyceum first.")?;
    if entry(&mut store, Product::Lyceum).view.busy {
        return Err("Lyceum is busy. Try again in a moment.".into());
    }
    let mut cmd = command(&editor);
    if let Some(world) = world {
        let world = if world.is_dir() {
            world.join("world.toml")
        } else {
            world
        };
        let world = world
            .canonicalize()
            .map(|w| plain_path(&w))
            .map_err(|_| "That folder holds no Lyceum world (no world.toml).".to_owned())?;
        if !world.is_file() {
            return Err("That folder holds no Lyceum world (no world.toml).".into());
        }
        cmd.arg(&world);
        // Listed at once, whatever Lyceum itself remembers of it.
        remember_opened(&store.root, &world);
        store.listed = None;
        if let Some(dats) = recent_dats().or(dats).filter(|d| d.is_dir()) {
            cmd.arg("--dats").arg(dats);
        }
        if let Some(classic) = classic.filter(|d| d.is_dir()) {
            cmd.arg("--classic-dats").arg(classic);
        }
    }
    cmd.current_dir(editor.parent().ok_or("Invalid editor path")?);
    let e = entry(&mut store, Product::Lyceum);
    e.children.push(
        cmd.spawn()
            .map_err(|e| format!("Could not open Lyceum: {e}"))?,
    );
    e.view.running = true;
    Ok(())
}

/// The editor's settings folder, where it keeps its recent worlds: `DERETH_SETTINGS_DIR` when
/// set, else `lyceum` beside the launcher's own settings folder.
fn lyceum_settings() -> Option<PathBuf> {
    let override_dir = std::env::var_os("DERETH_SETTINGS_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    override_dir.or_else(|| {
        dereth_launch::folders::resolve(dereth_launch::folders::System::HOST, &|name| {
            if name == "DERETH_STATE_DIR" {
                None
            } else {
                std::env::var_os(name)
            }
        })
        .and_then(|f| f.config.parent().map(|p| p.join("lyceum")))
    })
}

fn recent_worlds() -> Vec<PathBuf> {
    let Some(dir) = lyceum_settings() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(dir.join("recent-worlds.toml")) else {
        return Vec::new();
    };
    recent_from(&text)
}

/// The game data folder Lyceum drew its worlds over last.
fn recent_dats() -> Option<PathBuf> {
    let text = std::fs::read_to_string(lyceum_settings()?.join("recent-worlds.toml")).ok()?;
    dats_from(&text)
}

fn dats_from(text: &str) -> Option<PathBuf> {
    text.parse::<toml::Table>()
        .ok()?
        .get("dats")?
        .as_str()
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
}

/// A world's folder name from the name a person gives it: lower case, words joined by hyphens,
/// as Lyceum names worlds.
#[must_use]
pub fn world_folder_name(name: &str) -> Option<String> {
    let mut out = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_owned();
    (!out.is_empty()).then_some(out)
}

/// Make a new Lyceum world called `name` in `parent` (Lyceum's folder in Documents unless the
/// person chose another), from the sample or `empty`, built on the files of `era` (`eor`, the
/// default, or `infiltration`), and open the editor on it. A Lyceum whose `lyceum` command cannot
/// make worlds yet opens its own start screen instead, which can.
#[tauri::command]
pub async fn new_lyceum_world(
    shared: State<'_, Shared>,
    name: String,
    parent: Option<PathBuf>,
    empty: bool,
    era: Option<String>,
    dats: Option<PathBuf>,
    classic: Option<PathBuf>,
) -> Result<(), String> {
    let era = era.filter(|e| e != "eor");
    if era.as_deref().is_some_and(|e| e != "infiltration") {
        return Err("A world is built on the end-of-retail or the Infiltration files.".into());
    }
    let folder = world_folder_name(&name).ok_or("Give the world a name with letters or digits.")?;
    let (lyceum, parent) = {
        let mut store = lock(&shared);
        let lyceum = lyceum_command(&store)?;
        let e = entry(&mut store, Product::Lyceum);
        let parent = parent
            .or_else(|| e.view.home.clone())
            .ok_or("Choose where the world goes.")?;
        (lyceum, parent)
    };
    let dir = parent.join(&folder);
    if dir.exists() {
        return Err(format!(
            "There is already a {folder} folder there. Choose another name."
        ));
    }
    let output = tauri::async_runtime::spawn_blocking(move || {
        std::fs::create_dir_all(&parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        let mut cmd = command(&lyceum);
        cmd.arg("new").arg(&dir);
        if empty {
            cmd.arg("--empty");
        }
        if let Some(era) = &era {
            cmd.arg("--era").arg(era);
        }
        cmd.output().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    let shared = shared.inner();
    if output.status.success() {
        let world = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
        return open_editor(shared, Some(world), dats, classic);
    }
    let error = String::from_utf8_lossy(&output.stderr);
    if error.contains("new is not a command") {
        open_editor(shared, None, None, None)?;
        return Err("This Lyceum makes new worlds on its own start screen, which is open now. Update Lyceum to make them here.".into());
    }
    Err(error
        .trim()
        .trim_start_matches("lyceum: ")
        .lines()
        .next()
        .unwrap_or("Could not make the world.")
        .to_owned())
}

/// Choose the folder a new world goes in. `None` when the person cancels.
#[tauri::command]
pub async fn choose_lyceum_parent(
    app: tauri::AppHandle,
    shared: State<'_, Shared>,
) -> Result<Option<PathBuf>, String> {
    use tauri_plugin_dialog::DialogExt;
    let start = entry(&mut lock(&shared), Product::Lyceum)
        .view
        .home
        .clone()
        .filter(|d| d.is_dir());
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Choose where the new world goes");
    if let Some(start) = start {
        dialog = dialog.set_directory(start);
    }
    dialog
        .blocking_pick_folder()
        .map(|f| f.into_path().map_err(|e| e.to_string()))
        .transpose()
}

/// The recent worlds Lyceum's `recent-worlds.toml` names that are still there, newest first, each
/// once. The file may name one world under two spellings (Windows' `\\?\` form and the plain one,
/// or its folder and its `world.toml`); the world is kept once, as a person writes it.
fn recent_from(text: &str) -> Vec<PathBuf> {
    let Ok(table) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    table
        .get("worlds")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(PathBuf::from)
        .map(|p| if p.is_dir() { p.join("world.toml") } else { p })
        .filter(|p| p.is_file())
        .filter(|p| seen.insert(p.canonicalize().unwrap_or_else(|_| p.clone())))
        .map(|p| plain_path(&p))
        .take(10)
        .collect()
}

/// `path` without the `\\?\` prefix a canonical path carries on Windows: the spelling a person,
/// and the editor's own list, use for the same file.
#[must_use]
pub fn plain_path(path: &Path) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path.to_path_buf();
    };
    if let Some(share) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{share}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

/// Open any world folder in Lyceum, chosen in the system's folder picker.
#[tauri::command]
pub async fn choose_lyceum_world(
    app: tauri::AppHandle,
    shared: State<'_, Shared>,
    dats: Option<PathBuf>,
    classic: Option<PathBuf>,
) -> Result<(), String> {
    use tauri_plugin_dialog::DialogExt;
    let start = {
        let mut store = lock(&shared);
        let e = entry(&mut store, Product::Lyceum);
        e.view
            .worlds
            .first()
            .and_then(|w| w.path.parent()?.parent().map(Path::to_path_buf))
            .or_else(|| e.view.home.clone())
            .filter(|d| d.is_dir())
    };
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Open a Lyceum world's folder");
    if let Some(start) = start {
        dialog = dialog.set_directory(start);
    }
    let Some(folder) = dialog.blocking_pick_folder() else {
        return Ok(());
    };
    open_editor(
        &shared,
        Some(folder.into_path().map_err(|e| e.to_string())?),
        dats,
        classic,
    )
}

/// Open Lyceum's folder for new worlds in the system's file manager, making it first.
#[tauri::command]
pub fn open_worlds_folder(shared: State<'_, Shared>) -> Result<(), String> {
    let home = entry(&mut lock(&shared), Product::Lyceum)
        .view
        .home
        .clone()
        .ok_or("There is no Documents folder here.")?;
    std::fs::create_dir_all(&home).map_err(|e| format!("{}: {e}", home.display()))?;
    open_folder(&home)
}

/// Show `dir` in the system's file manager.
pub fn open_folder(dir: &Path) -> Result<(), String> {
    let opener = if cfg!(windows) {
        "explorer.exe"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Command::new(opener)
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open {}: {e}", dir.display()))
}

/// What a new server runs: its program and version, and whether that is this checkout's build
/// (which the server then runs in place, rather than a copy of its own).
pub struct Server {
    pub program: PathBuf,
    pub version: String,
    pub checkout: bool,
}

/// The server a new server is made with.
///
/// # Errors
/// There is none yet.
pub fn server(shared: &Shared) -> Result<Server, String> {
    server_of(shared, Product::Empyrean)
}

/// The server of `product` (Empyrean, or Yalain) a new world is made with: this checkout's build
/// in a development build that has one, else the download.
///
/// # Errors
/// There is none yet.
pub fn server_of(shared: &Shared, product: Product) -> Result<Server, String> {
    let store = lock(shared);
    if let Some(program) = newest_build(&store.builds, product.program()) {
        let version = program_version(product, &program).unwrap_or_else(|| "checkout".to_owned());
        return Ok(Server {
            program,
            version,
            checkout: true,
        });
    }
    store
        .entries
        .iter()
        .find(|e| e.view.product == product)
        .and_then(|e| e.view.installed.as_ref())
        .map(|i| Server {
            program: i.executable.clone(),
            version: i.version.clone(),
            checkout: false,
        })
        .ok_or_else(|| format!("Download {} first.", product.title()))
}

#[tauri::command]
pub async fn upgrade_lyceum_world(shared: State<'_, Shared>, world: PathBuf) -> Result<(), String> {
    let shared = shared.inner().clone();
    let executable = {
        let mut store = lock(&shared);
        let executable = lyceum_command(&store)?;
        let e = entry(&mut store, Product::Lyceum);
        e.children
            .retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));
        if e.view.busy || !e.children.is_empty() {
            return Err("Close Lyceum before upgrading a world.".into());
        }
        e.view.busy = true;
        e.view.error = false;
        e.view.message = Some("Upgrading the world...".into());
        executable
    };
    let result = tauri::async_runtime::spawn_blocking(move || {
        let world = world
            .canonicalize()
            .map_err(|_| "That world is no longer there.".to_owned())?;
        let output = command(&executable)
            .arg("upgrade")
            .arg(&world)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
        }
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    let mut store = lock(&shared);
    let e = entry(&mut store, Product::Lyceum);
    e.view.busy = false;
    e.view.error = result.is_err();
    e.view.message = Some(match result {
        Ok(message) | Err(message) => message,
    });
    Ok(())
}

/// Delete the Lyceum world `world` (its `world.toml`, or the folder that holds it) and everything
/// in its folder, while Lyceum is closed. The page has the person confirm it in place first. The
/// worlds it was published to keep playing it: a publish is a copy in the hosted world's folder.
#[tauri::command]
pub async fn delete_lyceum_world(shared: State<'_, Shared>, world: PathBuf) -> Result<(), String> {
    let shared = shared.inner().clone();
    let (root, home) = {
        let mut store = lock(&shared);
        let root = store.root.clone();
        let e = entry(&mut store, Product::Lyceum);
        e.children
            .retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));
        if e.view.busy || !e.children.is_empty() {
            return Err("Cannot delete while Lyceum is open".into());
        }
        e.view.busy = true;
        e.view.error = false;
        e.view.message = Some("Deleting the world...".into());
        (root, e.view.home.clone())
    };
    let result = tauri::async_runtime::spawn_blocking(move || {
        let recent = lyceum_settings().map(|d| d.join("recent-worlds.toml"));
        delete_world(&world, home.as_deref(), &root, recent.as_deref())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    let mut store = lock(&shared);
    store.listed = None;
    let e = entry(&mut store, Product::Lyceum);
    e.view.busy = false;
    e.view.message = None;
    result
}

/// Delete the world `world` ([`crate::lyceum_world::deletable`] says which folder, and refuses
/// any other), then forget it in the launcher's own list in its data folder `root` and in
/// Lyceum's recent worlds file `recent` ([`delete_folder`]).
fn delete_world(
    world: &Path,
    home: Option<&Path>,
    root: &Path,
    recent: Option<&Path>,
) -> Result<(), String> {
    let folder = crate::lyceum_world::deletable(world, home, "Lyceum")?;
    let file = folder.join("world.toml");
    // Which entries name it is read while it is there to compare.
    let names = |w: &Path| {
        let w = if w.is_dir() {
            w.join("world.toml")
        } else {
            w.to_path_buf()
        };
        w.canonicalize().is_ok_and(|w| w == file)
    };
    let mut opened = launcher_opened(root);
    let opened_before = opened.len();
    opened.retain(|w| !names(w));
    let recent_text = recent
        .and_then(|r| std::fs::read_to_string(r).ok())
        .and_then(|t| without_recent(&t, &names));
    delete_folder(&folder, "Lyceum", || {
        if opened.len() != opened_before {
            if let Ok(bytes) = serde_json::to_vec_pretty(&opened) {
                let _ = std::fs::write(root.join(OPENED_FILE), bytes);
            }
        }
        if let (Some(recent), Some(text)) = (recent, recent_text) {
            let _ = std::fs::write(recent, text);
        }
    })
}

/// Delete a world's folder `folder`, one [`crate::lyceum_world::deletable`] allowed, of the world
/// builder `builder` (Lyceum, Athenaeum). The folder is first moved aside to a hidden folder beside
/// it, which fails while a program holds one of its files, and then nothing is changed; only once
/// it is moved is the world forgotten (`forget`) and the moved folder deleted.
pub(crate) fn delete_folder(
    folder: &Path,
    builder: &str,
    forget: impl FnOnce(),
) -> Result<(), String> {
    let failed = || format!("Failed to delete {builder} world");
    let aside = aside_of(folder).ok_or_else(failed)?;
    std::fs::rename(folder, &aside)
        .map_err(|_| format!("Cannot delete while {builder} is open"))?;
    forget();
    std::fs::remove_dir_all(&aside).map_err(|e| {
        eprintln!(
            "dereth-launcher: what is left of a deleted world is in {}: {e}",
            plain_path(&aside).display()
        );
        failed()
    })
}

/// The start of the name of a world's folder while it is being deleted; such a folder is never
/// listed.
const DELETING: &str = ".deleting-";

/// A free name beside `folder` to move it to while it is deleted: `.deleting-<name>-<n>`.
fn aside_of(folder: &Path) -> Option<PathBuf> {
    let parent = folder.parent()?;
    let name = folder.file_name()?.to_string_lossy();
    (0..1000)
        .map(|n| parent.join(format!("{DELETING}{name}-{n}")))
        .find(|p| std::fs::symlink_metadata(p).is_err())
}

/// Lyceum's recent worlds file `text` without the worlds `names` matches, the rest of it as it
/// was; `None` when it names none of them, or does not read.
fn without_recent(text: &str, names: &dyn Fn(&Path) -> bool) -> Option<String> {
    let mut doc = text.parse::<toml_edit::DocumentMut>().ok()?;
    let worlds = doc.get_mut("worlds")?.as_array_mut()?;
    let before = worlds.len();
    worlds.retain(|w| !w.as_str().is_some_and(|w| names(Path::new(w))));
    (worlds.len() != before).then(|| doc.to_string())
}

/// Hold Lyceum while a world is published: its `lyceum` command builds the world, so the editor
/// must be closed and nothing else may use it meanwhile.
pub fn begin_build(shared: &Shared) -> Result<PathBuf, String> {
    let mut store = lock(shared);
    let program = lyceum_command(&store)?;
    let e = entry(&mut store, Product::Lyceum);
    e.children
        .retain_mut(|c| !matches!(c.try_wait(), Ok(Some(_))));
    if e.view.busy || !e.children.is_empty() {
        return Err("Save and close Lyceum before publishing.".into());
    }
    e.view.busy = true;
    e.view.message = Some("Publishing the world...".into());
    Ok(program)
}

pub fn finish_build(shared: &Shared) {
    let mut store = lock(shared);
    let e = entry(&mut store, Product::Lyceum);
    e.view.busy = false;
    e.view.message = None;
}

/// The servers a Lyceum world is exported for, by the word `lyceum export --for` takes, and
/// their names.
const EXPORT_TARGETS: [(&str, &str); 3] =
    [("empyrean", "Empyrean"), ("ace", "ACE"), ("gdle", "GDLE")];

/// The folder an export of the world `name` for `target` goes in, in the folder the person
/// chose: that folder itself when it is empty or holds an earlier export, else a folder named
/// for the world and the server inside it.
#[must_use]
pub fn export_folder(chosen: &Path, name: &str, target: &str) -> PathBuf {
    let empty = std::fs::read_dir(chosen).is_ok_and(|mut d| d.next().is_none());
    if empty || chosen.join(".lyceum-export").is_file() {
        chosen.to_path_buf()
    } else {
        chosen.join(format!("{name}-{target}"))
    }
}

/// Export the Lyceum world `world` for a server that runs it elsewhere, `target` (`empyrean`,
/// `ace` or `gdle`), into a folder the person chooses, with the `lyceum export` command beside
/// the editor, over the game data the world's era is built on: the Modern files `dats` for the
/// end of retail, the Classic files `classic` for Infiltration. What it did, or why not, is
/// Lyceum's line on the page.
#[tauri::command]
pub async fn export_lyceum_world(
    app: tauri::AppHandle,
    shared: State<'_, Shared>,
    world: PathBuf,
    target: String,
    dats: Option<PathBuf>,
    classic: Option<PathBuf>,
) -> Result<(), String> {
    use tauri_plugin_dialog::DialogExt;
    let (word, server) = EXPORT_TARGETS
        .into_iter()
        .find(|(w, _)| *w == target)
        .ok_or("A world is exported for Empyrean, ACE or GDLE.")?;
    if !world.is_file() {
        return Err("That world is no longer there.".into());
    }
    let infiltration = crate::lyceum_world::header(&world).era == "infiltration";
    let game = if infiltration {
        classic.ok_or("Infiltration needs the 2005 game files. Add them in the Library.")?
    } else {
        dats.ok_or("You have no end-of-retail game files yet. Add them in the Library.")?
    };
    let (lyceum, importer) = {
        let store = lock(&shared);
        let lyceum = lyceum_command(&store)?;
        // The server's import tool, which the export runs: beside the lyceum command, else
        // beside Empyrean's server.
        let import = program_file("empyrean-import");
        let importer = (!lyceum.with_file_name(&import).is_file())
            .then(|| program(&store, Product::Empyrean))
            .flatten()
            .and_then(|server| server.parent().map(Path::to_path_buf))
            .filter(|dir| dir.join(&import).is_file());
        (lyceum, importer)
    };
    let Some(chosen) = app
        .dialog()
        .file()
        .set_title(format!("Choose where the export for {server} goes"))
        .blocking_pick_folder()
    else {
        return Ok(());
    };
    let chosen = chosen.into_path().map_err(|e| e.to_string())?;
    let name = world
        .parent()
        .and_then(Path::file_name)
        .map_or_else(|| "world".to_owned(), |n| n.to_string_lossy().into_owned());
    let out = export_folder(&chosen, &name, word);
    {
        let mut store = lock(&shared);
        let e = entry(&mut store, Product::Lyceum);
        if e.view.busy {
            return Err("Lyceum is busy. Try again in a moment.".into());
        }
        e.view.busy = true;
        e.view.error = false;
        e.view.message = Some(format!("Exporting {name} for {server}..."));
    }
    let to = out.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut cmd = command(&lyceum);
        cmd.arg("export")
            .arg(&world)
            .arg("--dats")
            .arg(&game)
            .arg("--for")
            .arg(word)
            .arg("--out")
            .arg(&to);
        if let Some(dir) = importer {
            cmd.arg("--empyrean").arg(dir);
        }
        let output = cmd.output().map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            let said = String::from_utf8_lossy(&output.stderr);
            Err(said
                .trim()
                .lines()
                .next()
                .unwrap_or("The export failed.")
                .trim_start_matches("lyceum: ")
                .to_owned())
        }
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    let mut store = lock(&shared);
    let e = entry(&mut store, Product::Lyceum);
    e.view.busy = false;
    e.view.error = result.is_err();
    e.view.message = Some(match result {
        Ok(()) => format!("Exported {name} for {server} to {}.", out.display()),
        Err(error) => error,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn package() -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file(
            format!("package/{}", program_file("lyceum-editor")),
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"synthetic executable").unwrap();
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn a_failed_download_keeps_the_version_in_use_and_user_worlds() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let before = install_archive(
            root,
            Product::Lyceum,
            "1.2.3",
            "Changes",
            "test.zip",
            &package(),
        )
        .unwrap();
        let world = root.join("world.toml");
        std::fs::write(&world, b"user world").unwrap();
        assert!(
            install_archive(root, Product::Lyceum, "1.2.4", "", "test.zip", b"broken").is_err()
        );
        assert_eq!(
            read_installed(root, Product::Lyceum).unwrap().version,
            before.version
        );
        assert_eq!(std::fs::read(world).unwrap(), b"user world");
        assert!(!root.join("components/lyceum/staging").exists());
    }

    #[test]
    fn an_interrupted_unselected_version_is_unpacked_again_before_it_is_used() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let orphan = components::install_dir(root, Product::Lyceum, "1.2.3").unwrap();
        std::fs::create_dir_all(&orphan).unwrap();
        std::fs::write(orphan.join("partial"), b"incomplete").unwrap();
        let installed =
            install_archive(root, Product::Lyceum, "1.2.3", "", "test.zip", &package()).unwrap();
        assert_eq!(
            std::fs::read(&installed.executable).unwrap(),
            b"synthetic executable"
        );
        assert!(!orphan.join("partial").exists());
        assert_eq!(
            read_installed(root, Product::Lyceum).unwrap().version,
            "1.2.3"
        );
    }

    #[test]
    fn an_update_removes_the_version_it_replaces_and_nothing_else() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        install_archive(root, Product::Lyceum, "1.2.3", "", "test.zip", &package()).unwrap();
        let next =
            install_archive(root, Product::Lyceum, "1.3.0", "", "test.zip", &package()).unwrap();
        let folder = root.join("components/lyceum");
        std::fs::create_dir_all(folder.join("notes")).unwrap();
        remove_other_versions(root, Product::Lyceum, &next.version);
        assert!(!folder.join("1.2.3").exists());
        assert!(folder.join("1.3.0").is_dir());
        assert!(folder.join("notes").is_dir(), "only version folders go");
        assert_eq!(
            read_installed(root, Product::Lyceum).unwrap().version,
            "1.3.0"
        );
    }

    #[test]
    fn a_development_build_uses_the_newest_build_in_its_checkout() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = temp.path();
        std::fs::write(checkout.join("Cargo.toml"), b"[workspace]").unwrap();
        std::fs::create_dir_all(checkout.join("dereth/client")).unwrap();
        std::fs::write(checkout.join("dereth/client/Cargo.toml"), b"[package]").unwrap();
        let here = checkout.join("launcher/target/debug");
        std::fs::create_dir_all(&here).unwrap();
        let folders = build_folders(&here);
        assert_eq!(
            folders,
            [
                here.clone(),
                checkout.join("target/release"),
                checkout.join("target/test-release"),
                checkout.join("target/debug"),
            ]
        );
        assert_eq!(newest_build(&folders, "empyrean-server"), None);
        let file = program_file("empyrean-server");
        let mut at = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        for profile in ["release", "debug", "test-release"] {
            let dir = checkout.join("target").join(profile);
            std::fs::create_dir_all(&dir).unwrap();
            let program = std::fs::File::create(dir.join(&file)).unwrap();
            program.set_modified(at).unwrap();
            at += Duration::from_secs(60);
        }
        assert_eq!(
            newest_build(&folders, "empyrean-server"),
            Some(checkout.join("target/test-release").join(&file))
        );
        // An unpacked launcher is in no checkout: only its own folder is looked in.
        let elsewhere = tempfile::tempdir().unwrap();
        let outside = elsewhere.path().to_path_buf();
        assert_eq!(build_folders(&outside), [outside]);
    }

    #[test]
    fn recent_worlds_that_are_gone_are_not_offered() {
        let temp = tempfile::tempdir().unwrap();
        let kept = temp.path().join("kept/world.toml");
        std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
        std::fs::write(&kept, b"key = 'kept'").unwrap();
        let gone = temp.path().join("gone/world.toml");
        let mut table = toml::Table::new();
        table.insert(
            "worlds".into(),
            toml::Value::Array(vec![
                gone.display().to_string().into(),
                kept.display().to_string().into(),
            ]),
        );
        assert_eq!(recent_from(&table.to_string()), [kept]);
        assert!(recent_from("not toml [").is_empty());
    }

    /// A world Lyceum's list names more than once, under Windows' `\\?\` spelling and the plain
    /// one, by its folder, or by a path through `..`, is listed once, as a person writes it.
    #[test]
    fn a_world_named_under_several_spellings_is_listed_once() {
        let temp = tempfile::tempdir().unwrap();
        let world = temp.path().join("my-world").join("world.toml");
        let other = temp.path().join("other").join("world.toml");
        for w in [&world, &other] {
            std::fs::create_dir_all(w.parent().unwrap()).unwrap();
            std::fs::write(w, b"key = 'w'").unwrap();
        }
        let canonical = world.canonicalize().unwrap();
        let spellings = [
            canonical.clone(),
            world.clone(),
            world.parent().unwrap().to_path_buf(),
            temp.path()
                .join("other")
                .join("..")
                .join("my-world")
                .join("world.toml"),
            other.clone(),
        ];
        let mut table = toml::Table::new();
        table.insert(
            "worlds".into(),
            toml::Value::Array(
                spellings
                    .iter()
                    .map(|p| p.display().to_string().into())
                    .collect(),
            ),
        );
        let listed = recent_from(&table.to_string());
        assert_eq!(listed, [plain_path(&canonical), other]);
        assert!(!listed[0].display().to_string().starts_with(r"\\?\"));
    }

    /// A world in `parent`, named `name`.
    fn made_world(parent: &Path, name: &str) -> PathBuf {
        let world = parent.join(name).join("world.toml");
        std::fs::create_dir_all(world.parent().unwrap()).unwrap();
        std::fs::write(&world, format!("key = '{name}'\n")).unwrap();
        world
    }

    /// The name of a listed world's folder, and whether it is in Lyceum's folder for worlds.
    fn named(listed: &[(PathBuf, bool)]) -> Vec<(String, bool)> {
        listed
            .iter()
            .map(|(w, home)| {
                let folder = w.parent().unwrap().file_name().unwrap();
                (folder.to_string_lossy().into_owned(), *home)
            })
            .collect()
    }

    /// Lyceum's worlds are listed in two kinds: those in its folder for worlds, the ones opened
    /// lately first and then the rest by name, and the worlds opened from anywhere else, newest
    /// first. Each is listed once, and a world no longer there is not.
    #[test]
    fn worlds_in_lyceums_folder_come_first_then_the_others_opened_elsewhere() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("Lyceum");
        made_world(&home, "arwic");
        made_world(&home, "bandit-camp");
        let c = made_world(&home, "cragstone");
        std::fs::create_dir_all(home.join("not-a-world")).unwrap();
        let elsewhere = temp.path().join("code");
        let sample = made_world(&elsewhere, "sample");
        let mine = made_world(&elsewhere, "mine");
        let gone = elsewhere.join("gone/world.toml");
        let opened = [
            mine.clone(),
            c.clone(),
            gone,
            sample.parent().unwrap().to_path_buf(),
            c.canonicalize().unwrap(),
        ];
        let listed = list_worlds(Some(&home), &opened);
        let yes = |n: &str| (n.to_owned(), true);
        let no = |n: &str| (n.to_owned(), false);
        assert_eq!(
            named(&listed),
            [
                yes("cragstone"),
                yes("arwic"),
                yes("bandit-camp"),
                no("mine"),
                no("sample")
            ]
        );
        assert!(listed.iter().all(|(w, _)| w.is_file()));
        assert!(listed
            .iter()
            .all(|(w, _)| !w.display().to_string().starts_with(r"\\?\")));
        // No folder for worlds: every world opened is listed as opened elsewhere.
        assert_eq!(named(&list_worlds(None, &[mine])), [no("mine")]);
    }

    /// A world the launcher opens in Lyceum is first on its own list at once, once, and the list
    /// keeps its newest twenty; the page lists it with Lyceum's own recent worlds.
    #[test]
    fn a_world_the_launcher_opens_is_listed_at_once() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let first = made_world(temp.path(), "first");
        let second = made_world(temp.path(), "second");
        remember_opened(&root, &first);
        remember_opened(&root, &second);
        remember_opened(&root, &first);
        assert_eq!(
            launcher_opened(&root),
            [plain_path(&first), plain_path(&second)]
        );
        for n in 0..30 {
            remember_opened(&root, &temp.path().join(format!("w{n}/world.toml")));
        }
        assert_eq!(launcher_opened(&root).len(), OPENED_KEPT);
        assert!(launcher_opened(temp.path()).is_empty(), "no list yet");
    }

    /// A link at `link` to the folder `target`: a junction on Windows, which needs no rights.
    fn link_to(target: &Path, link: &Path) {
        #[cfg(windows)]
        assert!(Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .output()
            .unwrap()
            .status
            .success());
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
    }

    /// Deleting a world removes its whole folder and forgets it on the launcher's list and in
    /// Lyceum's recent worlds, keeping the other worlds and the rest of Lyceum's file.
    #[test]
    fn deleting_a_world_removes_its_folder_and_forgets_it() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let home = temp.path().join("Documents/Lyceum");
        let doomed = made_world(&home, "doomed");
        let kept = made_world(&home, "kept");
        std::fs::create_dir_all(home.join("doomed/sample/edits")).unwrap();
        std::fs::write(home.join("doomed/sample/pack.toml"), "name = 'sample'\n").unwrap();
        remember_opened(&root, &doomed);
        remember_opened(&root, &kept);
        let recent = temp.path().join("recent-worlds.toml");
        let entry = |w: &Path| toml::Value::from(w.display().to_string()).to_string();
        std::fs::write(
            &recent,
            format!(
                "# Lyceum's list.\ndats = 'C:/Games/AC'\nworlds = [{}, {}]\n",
                entry(doomed.parent().unwrap()),
                entry(&kept)
            ),
        )
        .unwrap();

        delete_world(&doomed, Some(&home), &root, Some(&recent)).unwrap();
        assert!(!home.join("doomed").exists());
        assert!(kept.is_file());
        assert_eq!(launcher_opened(&root), [plain_path(&kept)]);
        let text = std::fs::read_to_string(&recent).unwrap();
        assert_eq!(recent_from(&text), [plain_path(&kept)]);
        assert!(text.starts_with("# Lyceum's list.\ndats = 'C:/Games/AC'\n"));
        assert_eq!(
            list_worlds(Some(&home), &launcher_opened(&root)),
            [(plain_path(&kept), true)]
        );

        // A world named by its folder is deleted the same.
        delete_world(kept.parent().unwrap(), Some(&home), &root, None).unwrap();
        assert!(!home.join("kept").exists());
        assert!(home.is_dir(), "Lyceum's folder for worlds stays");
        assert_eq!(
            std::fs::read_dir(&home).unwrap().count(),
            0,
            "nothing is left aside"
        );

        // A folder left part deleted is never listed as a world.
        made_world(&home, ".deleting-gone-0");
        assert!(list_worlds(Some(&home), &[]).is_empty());
    }

    /// A world one of whose files a program holds is left whole: its folder, the launcher's list
    /// and Lyceum's recent worlds are as they were, and the reason is to close Lyceum.
    #[cfg(windows)]
    #[test]
    fn a_world_whose_file_is_held_open_is_not_deleted_at_all() {
        use std::os::windows::fs::OpenOptionsExt;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let home = temp.path().join("Documents/Lyceum");
        let world = made_world(&home, "held");
        std::fs::create_dir_all(home.join("held/sample")).unwrap();
        std::fs::write(home.join("held/sample/pack.toml"), "name = 'sample'\n").unwrap();
        remember_opened(&root, &world);
        let recent = temp.path().join("recent-worlds.toml");
        let text = format!(
            "worlds = [{}]\n",
            toml::Value::from(world.display().to_string())
        );
        std::fs::write(&recent, &text).unwrap();
        let opened = std::fs::read(root.join(OPENED_FILE)).unwrap();
        // Held as an editor holds a file it has open: others may read it, not move it.
        let _held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(home.join("held/sample/pack.toml"))
            .unwrap();
        assert_eq!(
            delete_world(&world, Some(&home), &root, Some(&recent)).unwrap_err(),
            "Cannot delete while Lyceum is open"
        );
        assert!(world.is_file());
        assert!(home.join("held/sample/pack.toml").is_file());
        assert_eq!(std::fs::read_dir(&home).unwrap().count(), 1);
        assert_eq!(std::fs::read(root.join(OPENED_FILE)).unwrap(), opened);
        assert_eq!(std::fs::read_to_string(&recent).unwrap(), text);
    }

    /// Only a world's own folder is deleted: never one without a world file, nor Lyceum's folder
    /// for worlds or the folders that hold it, nor a folder that holds a link, whose target is
    /// left as it was.
    #[test]
    fn deleting_refuses_a_folder_that_is_not_just_a_world() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let documents = temp.path().join("Documents");
        let home = documents.join("Lyceum");
        let delete = |w: &Path| delete_world(w, Some(&home), &root, None);

        let plain = home.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        std::fs::write(plain.join("notes.txt"), "mine").unwrap();
        assert!(delete(&plain).unwrap_err().contains("does not appear"));
        assert!(delete(&plain.join("world.toml")).is_err());
        assert!(plain.join("notes.txt").is_file());

        // A world file put straight in Documents, or in Lyceum's folder, leaves both alone.
        std::fs::write(documents.join("world.toml"), "key = 'loose'\n").unwrap();
        std::fs::write(home.join("world.toml"), "key = 'loose'\n").unwrap();
        for folder in [&documents, &home] {
            assert!(delete(&folder.join("world.toml"))
                .unwrap_err()
                .contains("Failed to delete"));
            assert!(folder.join("world.toml").is_file());
        }

        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("keep.txt"), "keep").unwrap();
        let linked = made_world(&home, "linked");
        link_to(&outside, &linked.parent().unwrap().join("art"));
        assert!(delete(&linked).unwrap_err().contains("Failed to delete"));
        assert!(linked.is_file());
        assert!(outside.join("keep.txt").is_file());

        // A world reached through a link is not deleted through it.
        let real = made_world(&outside, "real");
        let through = home.join("through");
        link_to(real.parent().unwrap(), &through);
        assert!(delete(&through).unwrap_err().contains("Failed to delete"));
        assert!(delete(&through.join("world.toml"))
            .unwrap_err()
            .contains("Failed to delete"));
        assert!(real.is_file());
    }

    /// What `lyceum facts` said of a world is shown while the world's sources and the command are
    /// as they were when it was asked; a world never asked, or changed since, is asked (again).
    /// With no lyceum command nothing is asked, and nothing shown.
    #[test]
    fn a_worlds_facts_are_asked_again_once_its_sources_change() {
        let then = Some(SystemTime::UNIX_EPOCH);
        let later = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(60));
        let listed = |world: &str, at| Listed {
            world: PathBuf::from(world),
            home: true,
            stamp: vec![(PathBuf::from(world), at, 10)],
        };
        let facts = serde_json::json!({ "terrain": 3 });
        let said = HashMap::from([
            (
                PathBuf::from("a/world.toml"),
                (
                    (then, listed("a/world.toml", then).stamp),
                    Some(facts.clone()),
                ),
            ),
            (
                PathBuf::from("b/world.toml"),
                ((then, listed("b/world.toml", then).stamp), None),
            ),
        ]);
        let worlds = [
            listed("a/world.toml", then),
            listed("b/world.toml", then),
            listed("c/world.toml", then),
        ];
        let (known, ask) = sort_facts(&worlds, &said, Some(then));
        assert_eq!(known.get(Path::new("a/world.toml")), Some(&Some(facts)));
        assert_eq!(
            known.get(Path::new("b/world.toml")),
            Some(&None),
            "a Lyceum without the command"
        );
        assert_eq!(ask.len(), 1);
        assert_eq!(ask[0].0, Path::new("c/world.toml"));
        // Saved since: asked again, and nothing shown meanwhile.
        let changed = [listed("a/world.toml", later)];
        let (known, ask) = sort_facts(&changed, &said, Some(then));
        assert!(known.is_empty() && ask.len() == 1);
        // Another lyceum command: asked again.
        let (_, ask) = sort_facts(&worlds[..1], &said, Some(later));
        assert_eq!(ask.len(), 1);
        assert_eq!(sort_facts(&worlds, &said, None).1.len(), 0);
        // What the command prints, read.
        assert_eq!(
            facts_from("{\"key\":\"k\",\"terrain\":1}\n").unwrap()["terrain"],
            1
        );
        assert_eq!(facts_from("lyceum: facts is not a command"), None);
        assert_eq!(facts_from("[1, 2]"), None);
    }

    /// A checkout's build says its version: Empyrean's server in its release facts, Lyceum's
    /// command on its first line; anything else is no version.
    #[test]
    fn a_checkout_builds_version_is_what_the_program_says_of_itself() {
        let facts = r#"{"version":"0.4.0","target":"x86_64-pc-windows-msvc","databases":{"shard":3,"auth":1},"world_pack":{"format":1,"schema":100}}"#;
        assert_eq!(
            version_from(Product::Empyrean, facts).as_deref(),
            Some("0.4.0")
        );
        assert_eq!(
            version_from(
                Product::Lyceum,
                "lyceum 0.1.1\ncommit: unknown\ntarget: unknown\n"
            )
            .as_deref(),
            Some("0.1.1")
        );
        assert_eq!(version_from(Product::Lyceum, "lyceum-editor\n"), None);
        assert_eq!(
            version_from(Product::Empyrean, r#"{"version":"checkout"}"#),
            None
        );
        assert_eq!(version_from(Product::Empyrean, ""), None);
    }

    /// An export goes in the folder chosen when it is empty or holds an earlier export, and in a
    /// folder named for the world and the server inside it otherwise.
    #[test]
    fn an_export_goes_in_a_folder_of_its_own() {
        let temp = tempfile::tempdir().unwrap();
        let empty = temp.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(export_folder(&empty, "hills", "ace"), empty);
        let earlier = temp.path().join("earlier");
        std::fs::create_dir_all(&earlier).unwrap();
        std::fs::write(earlier.join(".lyceum-export"), b"").unwrap();
        std::fs::write(earlier.join("README.txt"), b"").unwrap();
        assert_eq!(export_folder(&earlier, "hills", "ace"), earlier);
        let documents = temp.path().join("Documents");
        std::fs::create_dir_all(documents.join("letters")).unwrap();
        assert_eq!(
            export_folder(&documents, "hills", "gdle"),
            documents.join("hills-gdle")
        );
    }

    #[test]
    fn a_canonical_windows_path_is_given_its_plain_spelling() {
        assert_eq!(
            plain_path(Path::new(r"\\?\C:\Worlds\my-world\world.toml")),
            Path::new(r"C:\Worlds\my-world\world.toml")
        );
        assert_eq!(
            plain_path(Path::new(r"\\?\UNC\nas\worlds\world.toml")),
            Path::new(r"\\nas\worlds\world.toml")
        );
        for kept in [
            r"\\?\Volume{0}\world.toml",
            "/home/you/Lyceum/world.toml",
            r"C:\Worlds\world.toml",
        ] {
            assert_eq!(plain_path(Path::new(kept)), Path::new(kept));
        }
    }

    /// Upgrade is offered for a world only when `lyceum upgrade --check` found its files older
    /// than this Lyceum's format, and only while the world and the command are as they were when
    /// it was asked; a world never asked, or changed since, is asked (again) first.
    #[test]
    fn upgrade_is_offered_only_for_a_world_last_found_older_and_unchanged_since() {
        let then = Some(SystemTime::UNIX_EPOCH);
        let later = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(60));
        let (a, b, c) = (
            PathBuf::from("a/world.toml"),
            PathBuf::from("b/world.toml"),
            PathBuf::from("c/world.toml"),
        );
        let worlds = [a.clone(), b.clone(), c.clone()];
        let said = HashMap::from([
            (a.clone(), (vec![then, then], true)),
            (b.clone(), (vec![then, then], false)),
        ]);
        let (outdated, ask) = sort_formats(&worlds, &said, |_| Some(vec![then, then]));
        assert_eq!(outdated, std::slice::from_ref(&a));
        assert_eq!(ask, [(c.clone(), vec![then, then])]);
        // The world's file changed, an upgrade among other things: asked again, not offered meanwhile.
        let (outdated, ask) = sort_formats(&worlds, &said, |w| {
            Some(vec![then, if w == a { later } else { then }])
        });
        assert!(outdated.is_empty());
        assert_eq!(ask, [(a, vec![then, later]), (c, vec![then, then])]);
        // No lyceum command: nothing to ask, and nothing to upgrade with.
        assert_eq!(sort_formats(&worlds, &said, |_| None), (vec![], vec![]));
    }

    /// What `lyceum upgrade <world> --check` prints, read: the files that need upgrading.
    #[test]
    fn the_format_check_is_read_from_what_lyceum_prints() {
        let said =
            |n: usize| format!("{n} source file(s) need upgrading; world.lock is unchanged.\n");
        assert_eq!(needs_upgrade(&said(2)), Some(true));
        assert_eq!(needs_upgrade(&said(0)), Some(false));
        assert_eq!(
            needs_upgrade("2 source file(s) upgraded; world.lock is unchanged."),
            None,
            "an upgrade, not a check"
        );
        assert_eq!(needs_upgrade(""), None);
    }

    #[test]
    fn a_new_worlds_folder_is_named_as_lyceum_names_worlds() {
        assert_eq!(
            world_folder_name("Holtburg Hills").as_deref(),
            Some("holtburg-hills")
        );
        assert_eq!(
            world_folder_name("  Arwic's  Market!  ").as_deref(),
            Some("arwic-s-market")
        );
        assert_eq!(world_folder_name("\u{dc}nter 2").as_deref(), Some("nter-2"));
        assert_eq!(world_folder_name("!!!"), None);
        assert_eq!(
            dats_from("dats = 'C:/Games/AC'\nworlds = []\n"),
            Some(PathBuf::from("C:/Games/AC"))
        );
        assert_eq!(dats_from("worlds = []\n"), None);
    }

    #[test]
    fn only_a_development_build_started_without_the_switch_uses_its_checkouts_builds() {
        let args = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(uses_checkout(true, &args(&[])));
        assert!(!uses_checkout(true, &args(&["--downloads"])));
        assert!(!uses_checkout(false, &args(&[])));
        assert!(!uses_checkout(false, &args(&["--downloads"])));
    }

    #[test]
    fn a_failed_request_is_said_in_plain_words() {
        assert_eq!(
            plain(&ureq::Error::StatusCode(403)),
            components::RATE_LIMITED
        );
        assert!(plain(&ureq::Error::HostNotFound).contains("Could not reach GitHub"));
        assert!(plain(&ureq::Error::ConnectionFailed).contains("internet connection"));
        assert!(plain(&ureq::Error::StatusCode(502)).contains("502"));
    }

    /// A folder laid out as a package has the launcher's folder, with a client beside the launcher
    /// that says its version only through what the test passes as `said`.
    fn packaged_at(here: &Path, product: Product) {
        std::fs::create_dir_all(here).unwrap();
        std::fs::write(
            here.join(program_file(product.program())),
            b"packaged client",
        )
        .unwrap();
    }

    fn says(version: &'static str) -> impl Fn(Product, &Path) -> Option<String> {
        move |_, _| Some(version.to_owned())
    }

    /// The case the copy exists for. On macOS and Linux an update replaces the launcher's whole
    /// bundle, and a launcher release holds no client: the client must already be elsewhere.
    #[test]
    fn a_client_copied_out_outlasts_an_update_that_replaces_the_bundle() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let here = temp.path().join("Dereth.app/Contents/MacOS");
        packaged_at(&here, Product::Dereth);
        std::fs::write(here.join(dereth_launch::plugins::TOOL_FILE), b"tool").unwrap();
        let frameworks = here.join("../Frameworks");
        std::fs::create_dir_all(&frameworks).unwrap();
        std::fs::write(frameworks.join("libMoltenVK.dylib"), b"library").unwrap();

        let adopted = adopt_bundled(&root, &here, &says("0.4.9"));
        assert_eq!(adopted.len(), 1);
        adopted.into_iter().next().unwrap().unwrap();

        // The updater swaps in a bundle with only the launcher in it.
        std::fs::remove_dir_all(temp.path().join("Dereth.app")).unwrap();
        std::fs::create_dir_all(&here).unwrap();
        assert!(bundled_clients(&here).is_empty());
        assert!(adopt_bundled(&root, &here, &says("0.4.9")).is_empty());

        let client = downloaded(&root, Product::Dereth).expect("the copy is still the client");
        assert!(client.starts_with(root.join("components/dereth/0.4.9")));
        assert_eq!(std::fs::read(&client).unwrap(), b"packaged client");
        // The tool and the library stay where the launcher and the program look for them.
        assert_eq!(
            std::fs::read(client.with_file_name(dereth_launch::plugins::TOOL_FILE)).unwrap(),
            b"tool"
        );
        assert_eq!(
            std::fs::read(client.with_file_name("../Frameworks/libMoltenVK.dylib")).unwrap(),
            b"library"
        );
        // Nothing beside it says it's a release any more; it still updates.
        assert!(is_release(false, false));
        assert!(!is_release(true, false));
    }

    /// On macOS the copy is laid out as a bundle named for its game, so the Dock can draw that game,
    /// with its library where the program looks for it.
    #[test]
    fn a_client_in_the_launcher_s_bundle_is_copied_into_a_bundle_of_its_game() {
        let temp = tempfile::tempdir().unwrap();
        let here = temp.path().join("Launcher.app/Contents/MacOS");
        packaged_at(&here, Product::Auberean);
        let frameworks = here.join("../Frameworks");
        std::fs::create_dir_all(&frameworks).unwrap();
        std::fs::write(frameworks.join("libMoltenVK.dylib"), b"library").unwrap();
        let below: Vec<PathBuf> = bundled_files(&here, Product::Auberean)
            .into_iter()
            .map(|(_, below)| below)
            .collect();
        assert!(
            below.contains(
                &PathBuf::from("Auberean.app/Contents/MacOS").join(program_file("auberean"))
            ),
            "{below:?}"
        );
        assert!(
            below.contains(&PathBuf::from(
                "Auberean.app/Contents/Frameworks/libMoltenVK.dylib"
            )),
            "{below:?}"
        );
    }

    /// The copy lands where [`find_executable`] looks, at the same place in the version's folder as
    /// a downloaded package's client, on a layout with no `Contents/MacOS`.
    #[test]
    fn a_client_packaged_beside_the_launcher_alone_is_copied_to_the_top_of_its_version() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let here = temp.path().join("package");
        packaged_at(&here, Product::Dereth);
        adopt_bundled(&root, &here, &says("0.4.9"));
        assert_eq!(
            downloaded(&root, Product::Dereth).unwrap(),
            root.join("components/dereth/0.4.9")
                .join(program_file("dereth-client"))
        );
    }

    /// Starting the same package again does nothing, a newer package takes over and removes the
    /// older copy, and an older one (a player gone back to an old package, with a newer client
    /// already fetched) leaves the newer copy alone.
    #[test]
    fn only_a_newer_packaged_client_replaces_the_copy() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let here = temp.path().join("package");
        packaged_at(&here, Product::Dereth);

        adopt_bundled(&root, &here, &says("0.4.9"));
        let dest = root.join("components/dereth/0.4.9");
        let marker = dest.join("marker");
        std::fs::write(&marker, b"left alone").unwrap();
        assert!(adopt_bundled(&root, &here, &says("0.4.9")).is_empty());
        assert!(marker.is_file());
        assert!(adopt_bundled(&root, &here, &says("0.4.8")).is_empty());
        assert!(marker.is_file());

        adopt_bundled(&root, &here, &says("0.4.10"));
        assert_eq!(
            read_installed(&root, Product::Dereth).unwrap().version,
            "0.4.10"
        );
        assert!(!dest.exists());
    }

    /// A copy that can't be made says so and changes nothing: the packaged client is used, as
    /// [`in_use`] picks it with no copy to prefer, and a client that won't say its version isn't
    /// copied at all.
    #[test]
    fn a_failed_copy_leaves_the_packaged_client_in_use() {
        let temp = tempfile::tempdir().unwrap();
        let here = temp.path().join("package");
        packaged_at(&here, Product::Dereth);

        // The data folder is a file: nothing can be made under it.
        let root = temp.path().join("not-a-folder");
        std::fs::write(&root, b"").unwrap();
        let adopted = adopt_bundled(&root, &here, &says("0.4.9"));
        assert!(adopted[0].is_err());
        assert!(read_installed(&root, Product::Dereth).is_none());
        let program = here.join(program_file("dereth-client"));
        let (used, bundled) = in_use(None, Some((&program, Some("0.4.9")))).unwrap();
        assert!(bundled);
        assert_eq!(used.executable, program);

        let root = temp.path().join("data");
        assert!(adopt_bundled(&root, &here, &|_, _| None).is_empty());
        assert!(read_installed(&root, Product::Dereth).is_none());
    }

    /// The copy keeps the program runnable; a mutation of `copy_plain` that drops the permissions
    /// would leave a client that can't start.
    #[cfg(unix)]
    #[test]
    fn a_copied_client_can_still_be_run() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let here = temp.path().join("package");
        std::fs::create_dir_all(&here).unwrap();
        let program = here.join("dereth-client");
        std::fs::write(&program, "#!/bin/sh\necho 'dereth-client 0.4.9'\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();

        adopt_bundled(&root, &here, &program_version);
        std::fs::remove_file(&program).unwrap();
        let copy = downloaded(&root, Product::Dereth).unwrap();
        assert_eq!(
            program_version(Product::Dereth, &copy).as_deref(),
            Some("0.4.9")
        );
    }

    /// A bundle carried off the internet has its download's mark on every file, which would hold a
    /// client copied with it back from running. The copy is the launcher's own file, as a download
    /// the launcher makes is.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_copied_client_does_not_carry_the_bundle_s_download_mark() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let here = temp.path().join("package");
        packaged_at(&here, Product::Dereth);
        let program = here.join("dereth-client");
        let mark = "com.apple.quarantine";
        let set = std::process::Command::new("xattr")
            .args(["-w", mark, "0081;00000000;Test;"])
            .arg(&program)
            .status()
            .unwrap();
        assert!(set.success());

        adopt_bundled(&root, &here, &says("0.4.9"));
        let copy = downloaded(&root, Product::Dereth).unwrap();
        let listed = std::process::Command::new("xattr")
            .arg(&copy)
            .output()
            .unwrap();
        assert!(
            !String::from_utf8_lossy(&listed.stdout).contains(mark),
            "{}",
            String::from_utf8_lossy(&listed.stdout)
        );
    }

    /// A preferences file kept beside the client names how the player plays. It goes with the first
    /// copy, and from version to version after that, whether a package or a download brings it.
    #[test]
    fn preferences_beside_the_client_follow_it_to_each_new_version() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let here = temp.path().join("package");
        packaged_at(&here, Product::Dereth);
        let name = dereth_launch::interface::PREFERENCES_FILE;
        std::fs::write(here.join(name), b"from the package").unwrap();

        adopt_bundled(&root, &here, &says("0.4.9"));
        let first = downloaded(&root, Product::Dereth).unwrap();
        assert_eq!(
            std::fs::read(first.with_file_name(name)).unwrap(),
            b"from the package"
        );

        // Played since: what's beside the copy is the player's latest, over a stale one beside the
        // package.
        std::fs::write(first.with_file_name(name), b"played since").unwrap();
        adopt_bundled(&root, &here, &says("0.4.10"));
        let second = downloaded(&root, Product::Dereth).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            std::fs::read(second.with_file_name(name)).unwrap(),
            b"played since"
        );
    }

    /// The copy is preferred from the moment it's as new as the packaged client; a newer package
    /// whose copy has not been made yet is used directly.
    #[test]
    fn the_copy_is_used_unless_the_packaged_client_is_newer() {
        let copy = Installed {
            version: "0.4.9".to_owned(),
            executable: PathBuf::from("copy"),
            notes: String::new(),
        };
        let packaged = Path::new("packaged");
        let pick = |v| {
            let (used, bundled) = in_use(Some(&copy), Some((packaged, Some(v)))).unwrap();
            (used.executable, bundled)
        };
        assert_eq!(pick("0.4.9"), (PathBuf::from("copy"), false));
        assert_eq!(pick("0.4.8"), (PathBuf::from("copy"), false));
        assert_eq!(pick("0.4.10"), (PathBuf::from("packaged"), true));
    }

    /// A client is fetched only to replace one that's there and older: no copy means the player
    /// removed it or never got it, and that isn't this update's to undo.
    #[test]
    fn a_client_is_fetched_only_when_one_is_here_and_older() {
        assert!(client_is_behind(Some("0.4.9"), "0.4.10"));
        assert!(!client_is_behind(Some("0.4.9"), "0.4.9"));
        assert!(!client_is_behind(Some("0.4.10"), "0.4.9"));
        assert!(!client_is_behind(None, "0.4.10"));
        assert!(!client_is_behind(Some("what"), "0.4.10"));
    }
}
