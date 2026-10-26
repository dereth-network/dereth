//! Navigation: a walkable route from one position to another, in a dungeon or outdoors, through
//! the right corridors and streets and around the objects in them.
//!
//! **Depends on** `dereth-primitives` and `dereth-physics` (its shapes, its walkable slope and, for
//! checking a route, its world). **Used by** the client runtime (`dereth-client-runtime`, its
//! `--nav-proof` run).
//!
//! **Must never** read the data files, touch a device or keep a clock: cells, land, buildings and
//! setups arrive through [`NavSource`], and time arrives as an argument. It builds for
//! `wasm32-unknown-unknown`.
//!
//! # How a route is made
//!
//! 1. **Region.** A dungeon's cells reachable from the start through their portals are one
//!    region; outdoors, a window of landblocks round both ends (with every interior its buildings
//!    open onto) is one. A region is built the first time a route needs it and kept until
//!    [`NavWorld::forget_landblock`]. Floors are sampled into standing places a quarter metre
//!    apart, the terrain adding one per column; a place is clear when the body standing there,
//!    raised by the height it steps over, keeps its radius plus a margin from every wall,
//!    ceiling, building, tree and static object (a building's shell only from outdoors, as it
//!    stops only a body standing outside it), and terrain too steep to stand on or under water
//!    is left out. The clear places are merged into patches: rectangles of places on one floor,
//!    each a step from the next, that a body walks straight across. See [`Region`].
//! 2. **Search.** A* over the patches, from the place on a patch's edge where the way enters it
//!    to the place it leaves from, stepping out to a neighbouring column only where the floor
//!    rises or falls no more than a step, and never cutting a corner. The patches round a door, a
//!    creature or an object in the way, and round the route's ends, are searched place by place.
//! 3. **Pull.** The chain of places is straightened into legs a body can walk in a straight line,
//!    and each corner is tried for how far a body may cut across it.
//! 4. **Check.** [`NavWorld::plan_checked`] walks every leg with the body's own movement
//!    ([`PhysicsWalker`]); a leg that fails is walked again through its places, and where that
//!    fails too, the places there are taken out and the route planned again.
//!
//! The work can be done a slice at a time: a [`Task`] (a route, or the region round a place) is
//! worked on with [`NavWorld::advance`] for as many units of work as the caller spends, so it can
//! be spread over frames or run whole on a thread of the caller's.
//!
//! Doors, creatures and other objects the world places come and go through
//! [`NavWorld::set_obstacle`]. A closed door is a gate the route may pass at a cost, with a
//! [`WaypointAction::OpenDoor`] before it; a locked one, and any solid object, is not passed; a
//! creature is passed only where there is no way round.
//!
//! Routes through portals between places are not made yet: a dungeon and the outdoors are
//! [`NoPath::Disconnected`] from each other, and two ends farther apart than one window are
//! [`NoPath::TooFar`].

#![doc(html_no_source)]

mod export;
mod follow;
mod journey;
mod mesh;
mod path;
mod portals;
mod region;
mod search;
mod solids;
mod source;
mod task;
mod vmath;
mod walk;
mod world;

pub use export::{path_json, region_json, svg};
pub use follow::{FollowConfig, Follower, Steer};
pub use journey::{
    land_realm, plan_journey, plan_journey_with, CharacterState, HandBack, Journey, JourneyConfig,
    Leg, Realm, Teleport,
};
pub use path::{global, global_distance};
pub use path::{End, NavPath, NoPath, Waypoint, WaypointAction};
pub use portals::{
    EdgeSource, PortalEdge, PortalSighting, PortalStore, Recall, RecallEdge, StoreError,
    TeleportOutcome, TeleportWatch, STORE_VERSION,
};
pub use region::{BuildReport, Place, Region};
pub use source::{
    Agent, LandNavSource, LandStatics, NavConfig, NavSource, NoLandStatics, StaticPlacement,
};
pub use task::{Done, Task, Want};
pub use walk::{CheckReport, PhysicsWalker, SegmentCheck, WalkOutcome, WalkerConfig};
pub use world::{DoorState, NavWorld, Obstacle, ObstacleKind};
