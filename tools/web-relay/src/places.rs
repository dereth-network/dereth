//! The place package of an era, served to the web client that plays through the relay: the
//! portal table and the places `dereth-places` builds from the era's public world database, at
//! `GET /places/<era>/portals` and `GET /places/<era>/places`.
//!
//! A browser can fetch neither the database (about 155 MB for the end of retail, from a host that
//! sends no CORS headers) nor a package anyone ships, so the relay builds it where it runs: beside
//! the world's server, or on the player's own machine. The first request for an era fetches that
//! era's database into the per-user cache Empyrean's import tool keeps, once, and builds the
//! package in memory; every later request is answered from memory. Requests for an era while its
//! package is being built wait for that one build. An era that failed is not tried again until
//! [`RETRY_AFTER`] has passed, so a failing download is never repeated request by request.
//!
//! `--no-places` turns it off: then nothing is fetched and the routes answer `404`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use dereth_primitives::EraId;
use dereth_world_places::eras::EraDatabase;
use dereth_world_places::{Source, WorldPlaces};

use crate::limits::Clock;
use crate::log::Log;

/// How long an era whose package failed to build is refused before it is tried again.
pub const RETRY_AFTER: Duration = Duration::from_secs(10 * 60);

/// The most requests that wait at once for packages being built; any more are told to come back.
pub const MAX_WAITING: usize = 64;

/// The name the documents give the world: the package is the era's, not any one world's.
const WORLD: &str = "Dereth";

/// One era's package: its two documents, as the client's stores read them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub portals: String,
    pub places: String,
}

/// Which of a package's documents a request asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Document {
    Portals,
    Places,
}

/// What a request's path asks of the places routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// A document of a known era's package.
    Document(EraId, Document),
    /// A places path naming an era or a document there is none of.
    Unknown,
}

/// The places route a request target asks for, or `None` for a target that is none of them. The
/// route is the target's last three segments (`places/<era>/<document>`), so a proxy that hands
/// the relay a path under its own prefix reaches it too; a query is ignored.
#[must_use]
pub fn route(target: &str) -> Option<Route> {
    let path = target.split(['?', '#']).next().unwrap_or("");
    let segments: Vec<&str> = path.trim_end_matches('/').split('/').collect();
    if !segments.contains(&"places") {
        return None;
    }
    // `/places`, `/places/<era>` and longer paths under it are places paths too, of nothing.
    let [.., places, era, document] = segments[..] else {
        return Some(Route::Unknown);
    };
    if places != "places" {
        return Some(Route::Unknown);
    }
    let document = match document {
        "portals" => Document::Portals,
        "places" => Document::Places,
        _ => return Some(Route::Unknown),
    };
    // Only the era's own spelling: an era is named in the address the client builds.
    match EraId::ALL.into_iter().find(|e| e.name() == era) {
        Some(era) => Some(Route::Document(era, document)),
        None => Some(Route::Unknown),
    }
}

/// How a package is built for an era: fetched and read where the relay runs, or a test's.
pub type Build = Arc<dyn Fn(EraId) -> Result<Package, String> + Send + Sync>;

/// One era's package as the relay holds it.
enum Held {
    Built(Arc<Package>),
    Failed { why: String, at: Duration },
}

/// The packages the relay serves, each built once, on the first request for its era.
pub struct Places {
    build: Build,
    clock: Clock,
    /// Each era's slot. A request holds the slot's lock while it builds the era's package, so
    /// the requests that come meanwhile wait on the lock for that one build.
    eras: Mutex<HashMap<EraId, Arc<Mutex<Option<Held>>>>>,
    waiting: AtomicUsize,
}

impl std::fmt::Debug for Places {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Places").finish_non_exhaustive()
    }
}

/// Why a request for a package has no answer yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unbuilt {
    /// The package could not be built; it is tried again after [`RETRY_AFTER`].
    Failed(String),
    /// Too many requests are already waiting for packages being built.
    Busy,
}

impl Places {
    /// Packages built by `build`, on `clock` (for when a failed era is tried again).
    #[must_use]
    pub fn new(build: Build, clock: Clock) -> Self {
        Self {
            build,
            clock,
            eras: Mutex::new(HashMap::new()),
            waiting: AtomicUsize::new(0),
        }
    }

    /// Packages built from each era's public database, fetched into `cache` (the per-user folder
    /// Empyrean's import tool keeps) unless it is there already. `log` hears each download.
    #[must_use]
    pub fn fetching(cache: PathBuf, clock: Clock, log: Log) -> Self {
        let build: Build = Arc::new(move |era| fetch_and_build(era, &cache, &log));
        Self::new(build, clock)
    }

    /// The package of `era`: the one built already, or built now. A request that comes while
    /// another builds it waits for that build.
    ///
    /// # Errors
    /// The package could not be built (now, or within [`RETRY_AFTER`]), or too many requests
    /// are waiting already.
    pub fn package(&self, era: EraId) -> Result<Arc<Package>, Unbuilt> {
        let slot = {
            let mut eras = self.eras.lock().unwrap_or_else(PoisonError::into_inner);
            Arc::clone(eras.entry(era).or_default())
        };
        // Answered at once when built; otherwise the request counts as waiting until it has the
        // slot (and then the build, when it is the first).
        if let Ok(held) = slot.try_lock() {
            if let Some(Held::Built(p)) = &*held {
                return Ok(Arc::clone(p));
            }
        }
        if self.waiting.fetch_add(1, Ordering::SeqCst) >= MAX_WAITING {
            self.waiting.fetch_sub(1, Ordering::SeqCst);
            return Err(Unbuilt::Busy);
        }
        let answer = self.package_in(&slot, era);
        self.waiting.fetch_sub(1, Ordering::SeqCst);
        answer
    }

    fn package_in(&self, slot: &Mutex<Option<Held>>, era: EraId) -> Result<Arc<Package>, Unbuilt> {
        let mut held = slot.lock().unwrap_or_else(PoisonError::into_inner);
        let now = (self.clock)();
        match &*held {
            Some(Held::Built(p)) => return Ok(Arc::clone(p)),
            Some(Held::Failed { why, at }) if now.saturating_sub(*at) < RETRY_AFTER => {
                return Err(Unbuilt::Failed(why.clone()));
            }
            _ => {}
        }
        match (self.build)(era) {
            Ok(p) => {
                let p = Arc::new(p);
                *held = Some(Held::Built(Arc::clone(&p)));
                Ok(p)
            }
            Err(why) => {
                *held = Some(Held::Failed {
                    why: why.clone(),
                    at: now,
                });
                Err(Unbuilt::Failed(why))
            }
        }
    }
}

/// The era's package from its public database: fetched into `cache` unless it is there, read
/// and built in memory.
fn fetch_and_build(era: EraId, cache: &std::path::Path, log: &Log) -> Result<Package, String> {
    let (path, tag) = dereth_places::fetch_database(era, cache, &Http::new(), &mut |line| {
        log.line(&format!("places {}: {line}", era.name()));
    })?;
    let content = dereth_places::read_database(EraDatabase::of(era), &path)?;
    let docs = WorldPlaces::of(&content).documents(WORLD, Source::Hint);
    log.line(&format!(
        "places {}: built from {tag} ({} and {} bytes)",
        era.name(),
        docs.portals.len(),
        docs.places.len()
    ));
    Ok(Package {
        portals: docs.portals,
        places: docs.places,
    })
}

/// The relay's HTTP client for the databases.
struct Http(ureq::Agent);

impl Http {
    fn new() -> Self {
        Self(
            ureq::Agent::config_builder()
                .user_agent(concat!("dereth-web-relay/", env!("CARGO_PKG_VERSION")))
                .timeout_global(Some(Duration::from_secs(600)))
                .build()
                .into(),
        )
    }
}

fn http(url: &str, e: &ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(c) => format!("{url} answered HTTP {c}"),
        other => format!("could not reach {url}: {other}"),
    }
}

impl dereth_places::Fetch for Http {
    fn text(&self, url: &str, accept: &str) -> Result<String, String> {
        self.0
            .get(url)
            .header("Accept", accept)
            .call()
            .and_then(|mut r| r.body_mut().read_to_string())
            .map_err(|e| http(url, &e))
    }

    fn bytes(&self, url: &str, limit: u64) -> Result<Vec<u8>, String> {
        self.0
            .get(url)
            .call()
            .and_then(|mut r| r.body_mut().with_config().limit(limit).read_to_vec())
            .map_err(|e| http(url, &e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn a_places_path_names_an_era_and_a_document_or_is_unknown() {
        let doc = |e, d| Some(Route::Document(e, d));
        assert_eq!(
            route("/places/eor/portals"),
            doc(EraId::Eor, Document::Portals)
        );
        assert_eq!(
            route("/places/infiltration/places?x=1"),
            doc(EraId::Infiltration, Document::Places)
        );
        assert_eq!(
            route("/relay/places/launch/portals/"),
            doc(EraId::Launch, Document::Portals),
            "under a proxy's prefix"
        );
        assert_eq!(route("/places/tod/portals"), Some(Route::Unknown));
        assert_eq!(route("/places/EOR/portals"), Some(Route::Unknown));
        assert_eq!(route("/places/eor/weenies"), Some(Route::Unknown));
        assert_eq!(route("/places/eor"), Some(Route::Unknown));
        assert_eq!(route("/places"), Some(Route::Unknown));
        assert_eq!(route("/"), None);
        assert_eq!(route("/ws"), None);
        assert_eq!(route("/eor/portals"), None);
    }

    fn still() -> Clock {
        Arc::new(|| Duration::ZERO)
    }

    #[test]
    fn requests_while_an_era_builds_wait_for_its_one_build() {
        let builds = Arc::new(AtomicUsize::new(0));
        let (started_tx, started) = mpsc::channel();
        let (go, go_rx) = mpsc::channel::<()>();
        let go_rx = Mutex::new(go_rx);
        let counted = Arc::clone(&builds);
        let build: Build = Arc::new(move |era| {
            counted.fetch_add(1, Ordering::SeqCst);
            let _ = started_tx.send(());
            go_rx.lock().expect("one build").recv().expect("let go");
            Ok(Package {
                portals: format!("portals of {}", era.name()),
                places: String::new(),
            })
        });
        let places = Arc::new(Places::new(build, still()));
        let first = {
            let p = Arc::clone(&places);
            std::thread::spawn(move || p.package(EraId::Eor))
        };
        started.recv().expect("the first build starts");
        let others: Vec<_> = (0..4)
            .map(|_| {
                let p = Arc::clone(&places);
                std::thread::spawn(move || p.package(EraId::Eor))
            })
            .collect();
        // The others are waiting on the slot, not building: once they are counted, let it go.
        while places.waiting.load(Ordering::SeqCst) < 5 {
            std::thread::yield_now();
        }
        go.send(()).expect("let the build go");
        for t in std::iter::once(first).chain(others) {
            let p = t.join().expect("no panic").expect("built");
            assert_eq!(p.portals, "portals of eor");
        }
        assert_eq!(builds.load(Ordering::SeqCst), 1, "one build for them all");
        // Built: later requests are answered from memory.
        assert!(places.package(EraId::Eor).is_ok());
        assert_eq!(builds.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_failed_era_is_refused_until_it_may_be_tried_again() {
        let builds = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&builds);
        let build: Build = Arc::new(move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
            Err("could not reach the release".to_owned())
        });
        let now = Arc::new(Mutex::new(Duration::ZERO));
        let at = Arc::clone(&now);
        let clock: Clock = Arc::new(move || *at.lock().expect("the time"));
        let places = Places::new(build, clock);
        let failed = Err(Unbuilt::Failed("could not reach the release".to_owned()));
        assert_eq!(places.package(EraId::Launch), failed);
        assert_eq!(places.package(EraId::Launch), failed);
        assert_eq!(builds.load(Ordering::SeqCst), 1, "not tried again at once");
        *now.lock().expect("the time") = RETRY_AFTER;
        assert_eq!(places.package(EraId::Launch), failed);
        assert_eq!(
            builds.load(Ordering::SeqCst),
            2,
            "tried again after a while"
        );
        // Each era is its own.
        let _ = places.package(EraId::Eor);
        assert_eq!(builds.load(Ordering::SeqCst), 3);
    }
}
