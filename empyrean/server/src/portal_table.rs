//! Not ACE: the world's portal table and its places, published by the status endpoint as
//! `GET /portals` and `GET /places` when `server.publish_portals` is on.
//!
//! ACE publishes nothing like them. A client that routes a character through portals needs to know
//! where each placed portal stands, where it leads and who may use it; a client that draws a map of
//! the world, or lists where to go, needs its lifestones, vendors and people too. These are those
//! lists, read from the world content the server loaded (its landblock instances and their
//! weenies), never from the live world, so no landblock is loaded to build them, and no player,
//! nothing a player carries and no house owner is ever in them.
//!
//! The documents, and how they are read from a world's content, are `dereth-world-places`'s
//! ([`dereth_world_places::documents`]): a launcher builds the same documents from the public
//! database of a world's era with the same code, so the two agree on the same content. This
//! module gives that builder the server's content ([`ServerContent`]) and builds the documents
//! off the endpoint's thread ([`PortalTable`]).
//!
//! The places say whether their towns and landmarks are the world's own names, which a client's
//! maps draw in place of the game's own map notes: they are when the world's overlay folder names
//! its own ([`with_own_places`]), or else when its era's world database names them, as the
//! Infiltration and Launch databases do and the end of retail's does not
//! ([`PortalTable::of_world`]).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use dereth_world_places::{GameEvent, Landmark, Placement, Sending, Spawn, Spot, WorldSource};
use empyrean_common::era::EraRules;
use empyrean_content::models::world::{LandblockInstance, Weenie};
use empyrean_content::WorldDatabase;

pub use dereth_world_places::documents::{HEADER, PLACES_HEADER};
pub use dereth_world_places::Documents;

/// The server's loaded content, as the places are read from it. Each weenie is read from the
/// content once.
pub struct ServerContent<'a> {
    content: &'a dyn WorldDatabase,
    weenies: Mutex<HashMap<u32, Option<Arc<dereth_world_places::Weenie>>>>,
}

impl std::fmt::Debug for ServerContent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerContent").finish_non_exhaustive()
    }
}

impl<'a> ServerContent<'a> {
    /// The places' view of `content`.
    #[must_use]
    pub fn new(content: &'a dyn WorldDatabase) -> Self {
        Self {
            content,
            weenies: Mutex::new(HashMap::new()),
        }
    }
}

impl WorldSource for ServerContent<'_> {
    fn placements(&self) -> Vec<Placement> {
        self.content
            .get_all_landblock_instances()
            .iter()
            .map(placement)
            .collect()
    }

    fn weenie(&self, wcid: u32) -> Option<Arc<dereth_world_places::Weenie>> {
        let mut weenies = self
            .weenies
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        weenies
            .entry(wcid)
            .or_insert_with(|| self.content.get_weenie(wcid).map(|w| Arc::new(weenie(&w))))
            .clone()
    }

    fn events(&self) -> Vec<GameEvent> {
        self.content
            .get_all_events()
            .iter()
            .map(|e| GameEvent {
                name: e.name.clone(),
                state: e.state,
                start: i64::from(e.start_time),
                end: i64::from(e.end_time),
            })
            .collect()
    }

    fn landmarks(&self) -> Vec<Landmark> {
        self.content.cache_all_points_of_interest();
        self.content
            .get_points_of_interest_cache()
            .into_iter()
            .filter_map(|(_, p)| {
                p.map(|p| Landmark {
                    name: p.name.clone(),
                    wcid: p.weenie_class_id,
                })
            })
            .collect()
    }

    fn sendings(&self) -> Vec<Sending> {
        self.content
            .get_all_spell_names()
            .keys()
            .filter_map(|&id| {
                let s = self.content.get_cached_spell(id)?;
                let cell = s.position_obj_cell_id.filter(|c| *c != 0)?;
                let f = |v: Option<f32>| v.unwrap_or(0.0);
                Some(Sending {
                    spell: id,
                    name: s.name.clone(),
                    to: Spot {
                        cell,
                        origin: [
                            f(s.position_origin_x),
                            f(s.position_origin_y),
                            f(s.position_origin_z),
                        ],
                        rotation: [
                            s.position_angles_w.unwrap_or(1.0),
                            f(s.position_angles_x),
                            f(s.position_angles_y),
                            f(s.position_angles_z),
                        ],
                    },
                })
            })
            .collect()
    }
}

fn placement(i: &LandblockInstance) -> Placement {
    Placement {
        guid: i.guid,
        wcid: i.weenie_class_id,
        at: Spot {
            cell: i.obj_cell_id,
            origin: [i.origin_x, i.origin_y, i.origin_z],
            rotation: [i.angles_w, i.angles_x, i.angles_y, i.angles_z],
        },
        links: i
            .landblock_instance_link
            .iter()
            .map(|l| l.child_guid)
            .collect(),
    }
}

#[allow(clippy::cast_sign_loss)]
fn weenie(w: &Weenie) -> dereth_world_places::Weenie {
    dereth_world_places::Weenie {
        class_id: w.class_id,
        class_name: w.class_name.clone(),
        weenie_type: w.r#type as u32,
        ints: w
            .weenie_properties_int
            .iter()
            .map(|p| (u32::from(p.r#type), p.value))
            .collect(),
        bools: w
            .weenie_properties_bool
            .iter()
            .map(|p| (u32::from(p.r#type), p.value))
            .collect(),
        floats: w
            .weenie_properties_float
            .iter()
            .map(|p| (u32::from(p.r#type), p.value))
            .collect(),
        strings: w
            .weenie_properties_string
            .iter()
            .map(|p| (u32::from(p.r#type), p.value.clone()))
            .collect(),
        dids: w
            .weenie_properties_did
            .iter()
            .map(|p| (u32::from(p.r#type), p.value))
            .collect(),
        positions: w
            .weenie_properties_position
            .iter()
            .map(|p| {
                (
                    u32::from(p.position_type),
                    Spot {
                        cell: p.obj_cell_id,
                        origin: [p.origin_x, p.origin_y, p.origin_z],
                        rotation: [p.angles_w, p.angles_x, p.angles_y, p.angles_z],
                    },
                )
            })
            .collect(),
        spawns: w
            .weenie_properties_generator
            .iter()
            .map(|g| Spawn {
                wcid: g.weenie_class_id,
                probability: g.probability,
                init_create: g.init_create,
                max_create: g.max_create,
                when_create: g.when_create,
                where_create: g.where_create,
                at: Spot {
                    cell: g.obj_cell_id.unwrap_or(0),
                    origin: [
                        g.origin_x.unwrap_or(0.0),
                        g.origin_y.unwrap_or(0.0),
                        g.origin_z.unwrap_or(0.0),
                    ],
                    rotation: [
                        g.angles_w.unwrap_or(1.0),
                        g.angles_x.unwrap_or(0.0),
                        g.angles_y.unwrap_or(0.0),
                        g.angles_z.unwrap_or(0.0),
                    ],
                },
            })
            .collect(),
    }
}

/// The documents for one world, built once, on a thread of their own, from the moment they are
/// made: the status endpoint's own thread never builds them, so `/status` and `/health` never wait
/// on them.
#[derive(Debug)]
pub struct PortalTable {
    documents: Arc<OnceLock<Documents>>,
}

impl PortalTable {
    /// The portal table and places of `content` for the world named `world_name`, their building
    /// begun.
    #[must_use]
    pub fn new(world_name: &str, content: Arc<dyn WorldDatabase>) -> Self {
        Self::with_own_places(world_name, content, None)
    }

    /// As [`Self::new`], with the places of the world's own places document `own` (its overlay
    /// folder's, [`crate::dat_directory::world_places`]) in place of the content's when there is
    /// one ([`with_own_places`]).
    #[must_use]
    pub fn with_own_places(
        world_name: &str,
        content: Arc<dyn WorldDatabase>,
        own: Option<String>,
    ) -> Self {
        Self::named(world_name, content, own, false)
    }

    /// As [`Self::with_own_places`], for a world playing the era `era`: a world with no places
    /// document of its own whose era's world database names the world's own towns and landmarks
    /// ([`EraRules::names_own_places`]) publishes the content's places marked as its own names
    /// ([`marked_own`]). The world's own document still wins over its era's names.
    #[must_use]
    pub fn of_world(
        world_name: &str,
        content: Arc<dyn WorldDatabase>,
        own: Option<String>,
        era: &EraRules,
    ) -> Self {
        Self::named(world_name, content, own, era.names_own_places)
    }

    fn named(
        world_name: &str,
        content: Arc<dyn WorldDatabase>,
        own: Option<String>,
        era_names_own: bool,
    ) -> Self {
        let world_name = world_name.to_owned();
        Self::building(move || {
            let mut d = Documents::of(&world_name, &ServerContent::new(&*content));
            if let Some(own) = &own {
                d.places = with_own_places(&d.places, own);
            } else if era_names_own {
                d.places = marked_own(&d.places);
            }
            d
        })
    }

    /// The documents `build` makes, their building begun on a thread of its own (or, should no
    /// thread start, here and now).
    #[must_use]
    pub fn building(build: impl FnOnce() -> Documents + Send + 'static) -> Self {
        let documents = Arc::new(OnceLock::new());
        let cell = Arc::clone(&documents);
        let build = std::sync::Mutex::new(Some(build));
        let run = move || {
            if let Some(f) = build.lock().ok().and_then(|mut b| b.take()) {
                let _ = cell.set(f());
            }
        };
        let run = Arc::new(run);
        let on_thread = Arc::clone(&run);
        let spawned = std::thread::Builder::new()
            .name("Portal Table".to_owned())
            .spawn(move || on_thread());
        if let Err(e) = spawned {
            log::warn!("The portal table is built in place: no thread started ({e})");
            run();
        }
        Self { documents }
    }

    /// The portal table once it is built; `None` while it is being built.
    #[must_use]
    pub fn ready(&self) -> Option<&str> {
        self.documents.get().map(|d| d.portals.as_str())
    }

    /// The places once they are built; `None` while they are being built.
    #[must_use]
    pub fn places_ready(&self) -> Option<&str> {
        self.documents.get().map(|d| d.places.as_str())
    }

    /// The portal table, waiting for it to be built.
    #[must_use]
    pub fn document(&self) -> &str {
        &self.documents.wait().portals
    }

    /// The places, waiting for them to be built.
    #[must_use]
    pub fn places(&self) -> &str {
        &self.documents.wait().places
    }
}

/// Both documents of `content` for the world named `world_name`.
#[must_use]
pub fn documents(world_name: &str, content: &dyn WorldDatabase) -> Documents {
    Documents::of(world_name, &ServerContent::new(content))
}

/// The portal table of `content` for the world named `world_name`.
#[must_use]
pub fn document(world_name: &str, content: &dyn WorldDatabase) -> String {
    dereth_world_places::documents::portal_table(world_name, &ServerContent::new(content))
}

/// The places of `content` for the world named `world_name`.
#[must_use]
pub fn places_document(world_name: &str, content: &dyn WorldDatabase) -> String {
    dereth_world_places::documents::places(world_name, &ServerContent::new(content))
}

/// `places` with a `naming own` line after its `world` line, saying its towns and landmarks are
/// the world's own names; any `naming` line it had is dropped. Its places are unchanged.
#[must_use]
pub fn marked_own(places: &str) -> String {
    let mut out = String::with_capacity(places.len() + 11);
    for line in places.lines() {
        if line.starts_with("naming\t") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
        if line.starts_with("world\t") {
            out.push_str("naming\town\n");
        }
    }
    out
}

/// `places` with its places replaced by those of a world's own places document `own`: its
/// header and its `world` line, a `naming own` line saying its towns and landmarks are the
/// world's own names, then each line of `own` after its own header, `world` and `naming` lines.
/// A document of another kind changes nothing.
#[must_use]
pub fn with_own_places(places: &str, own: &str) -> String {
    let Some(body) = own.strip_prefix(PLACES_HEADER) else {
        return places.to_owned();
    };
    let mut out = String::new();
    for line in places.lines() {
        if line.starts_with("place\t") || line.starts_with("naming\t") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
        if line.starts_with("world\t") {
            out.push_str("naming\town\n");
        }
    }
    for line in body.lines() {
        if line.is_empty() || line.starts_with("world\t") || line.starts_with("naming\t") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}
