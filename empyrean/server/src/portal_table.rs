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

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use dereth_world_places::{GameEvent, Landmark, Placement, Spawn, Spot, WorldSource};
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
        let world_name = world_name.to_owned();
        Self::building(move || Documents::of(&world_name, &ServerContent::new(&*content)))
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
