//! `World`: the one value that owns all mutable world state.
//!
//! Not an ACE port in itself; it is where ACE's static managers live. Each ACE static class with
//! mutable state becomes one field, whose type is defined in that class's ported module. The
//! host-provided handles (content, shard, authentication, network) are fields too, given empty or
//! default values by `World::new` and replaced by the server binary or the test harness.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use empyrean_common::account_defaults::AccountDefaults;
use empyrean_common::clock::{Clock, ClockSnapshot, VirtualClock};
use empyrean_content::{MemContent, WorldDatabase};
use empyrean_dat::DatManager;
use empyrean_net::{NetConfig, ServerNet};
use empyrean_store::{AuthDatabase, MemAuth, MemShard, ShardHandle};

use crate::entity::mutations::mutation_cache::MutationCacheState;
use crate::entity::timers::TimersState;
use crate::managers::allegiance_manager::AllegianceManagerState;
use crate::managers::ddd_manager::DddManagerState;
use crate::managers::event_manager::EventManagerState;
use crate::managers::guid_manager::GuidManagerState;
use crate::managers::house_manager::HouseManagerState;
use crate::managers::landblock_manager::LandblockManagerState;
use crate::managers::player_manager::PlayerManagerState;
use crate::managers::property_manager::PropertyManagerState;
use crate::managers::recipe_manager::RecipeManagerState;
use crate::managers::server_manager::ServerManagerState;
use crate::managers::server_performance_monitor::ServerPerformanceMonitorState;
use crate::managers::world_manager::WorldManagerState;
use crate::object_store::ObjectStore;
use crate::physics::phys_ext::PhysExtState;
use crate::physics::server_object_manager::ServerObjectManagerState;
use crate::sessions::Sessions;
use dereth_physics::PhysicsWorld;

/// Everything the world thread mutates, plus shared immutable data.
#[derive(Debug)]
pub struct World {
    /// The clocks, read once at the top of each tick. Ported `Timers`/`Time`/`DateTime.UtcNow`
    /// calls read these.
    pub now: ClockSnapshot,
    /// The only owner of live world objects.
    pub objects: ObjectStore,

    // ---- ACE static managers (mutable state) ----
    pub timers: TimersState,
    pub world_manager: WorldManagerState,
    pub landblock_manager: LandblockManagerState,
    pub player_manager: PlayerManagerState,
    pub guid_manager: GuidManagerState,
    pub event_manager: EventManagerState,
    pub house_manager: HouseManagerState,
    pub allegiance_manager: AllegianceManagerState,
    pub recipe_manager: RecipeManagerState,
    pub property_manager: PropertyManagerState,
    pub ddd_manager: DddManagerState,
    pub server_manager: ServerManagerState,
    pub performance: ServerPerformanceMonitorState,
    pub mutation_cache: MutationCacheState,
    /// The static tables of `Factories/Tables` (GemCountChance, CantripChance) and
    /// `SpellLevelCache`, built on first use from this world's content, properties and dats.
    pub loot_tables: crate::factories::loot_generation_factory::tables_logic::LootTablesState,

    // ---- shared C# reference objects, by id ----
    /// Every `Fellowship`; players hold `FellowshipRef` ids (`Player.Fellowship`).
    pub fellowships: crate::entity::fellowship::FellowshipStore,
    /// Every `ChessMatch`; the board and its players hold `ChessMatchRef` ids.
    pub chess_matches: crate::entity::chess::chess_match::ChessMatchStore,

    /// The game half of each ACE `Session`, and the inbound message queue.
    pub sessions: Sessions,

    /// The transport: ACE's `NetworkManager` with the transport half of every `Session`.
    /// It is sans-IO, so the world owns it and every world action can reach it; a
    /// `world_manager::NetDriver` carries its datagrams to and from the wire. `World::new` gives
    /// it the default settings; the server and the test harness replace it with their own.
    pub net: ServerNet,

    /// The one shared-crate physics world, as ACE's `LScape` is one. Its land source is
    /// a `DatLandSource` over `dats` (tests may install a flat one with
    /// `physics::phys_ext::use_land_source`). `WorldObject.phys` handles name bodies in it.
    pub physics: PhysicsWorld,
    /// The server half of ACE's `PhysicsObj`s and physics landblocks: each body's `WeenieObject`,
    /// `ObjectMaint` and collision table, and the landblocks' server object lists.
    pub phys_ext: PhysExtState,
    /// ACE's static `ServerObjectManager`: body id to body.
    pub server_object_manager: ServerObjectManagerState,

    // ---- immutable shared data ----
    pub dats: Arc<DatManager>,
    /// `DatabaseManager.World`. Empty (`MemContent`) until the host sets it: the server binary
    /// installs `PackContent` over `world.pack`, tests install a `MemContent` built for the test.
    pub content: Arc<dyn WorldDatabase>,

    // ---- persistence ----
    /// `DatabaseManager.Shard`: the serialized shard database. World code hands it owned
    /// snapshots; each job's callback comes back to the world and runs on the world thread in the
    /// loop's `SerializedShardDatabase` callback stage (see `world_manager`). `World::new` gives it
    /// a synchronous handle over an empty `MemShard`; the server binary installs a threaded handle
    /// over `shard.db` (`DatabaseManager.Initialize`), tests install their own.
    pub shard: ShardHandle<World>,
    /// `DatabaseManager.Authentication` (with `DatabaseManager.AutoPromoteNextAccountToAdmin`).
    /// `World::new` gives it an empty `MemAuth`; the server binary installs `auth.db`.
    pub auth: AuthHandle,
    // Not held here: ACE's `ConfigManager.Config` (`MasterConfiguration`).
    /// Not ACE: the rules of the era the world plays (`[era] profile`). `World::new` reads the
    /// configuration's era (end of retail before one is loaded); a test sets its own.
    pub era: &'static empyrean_common::era::EraRules,
}

/// `DatabaseManager.Authentication` and `DatabaseManager.AutoPromoteNextAccountToAdmin`.
///
/// ACE's `AuthenticationDatabase` opens a new context per call and is used from the login task's
/// thread as well as the world thread, so the handle is shared (`Clone`) and each call locks the
/// database for its duration.
#[derive(Clone)]
pub struct AuthHandle {
    db: Arc<Mutex<Box<dyn AuthDatabase>>>,
    auto_promote_next_account_to_admin: Arc<AtomicBool>,
}

impl AuthHandle {
    /// A handle over `db`, with `AutoPromoteNextAccountToAdmin` false.
    #[must_use]
    pub fn new(db: Box<dyn AuthDatabase>) -> Self {
        Self {
            db: Arc::new(Mutex::new(db)),
            auto_promote_next_account_to_admin: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The database, locked for one or more calls (`DatabaseManager.Authentication.<Method>`).
    pub fn lock(&self) -> MutexGuard<'_, Box<dyn AuthDatabase>> {
        self.db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// `DatabaseManager.AutoPromoteNextAccountToAdmin` (get).
    #[must_use]
    pub fn auto_promote_next_account_to_admin(&self) -> bool {
        self.auto_promote_next_account_to_admin
            .load(Ordering::SeqCst)
    }

    /// `DatabaseManager.AutoPromoteNextAccountToAdmin` (set).
    pub fn set_auto_promote_next_account_to_admin(&self, value: bool) {
        self.auto_promote_next_account_to_admin
            .store(value, Ordering::SeqCst);
    }
}

impl std::fmt::Debug for AuthHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthHandle")
            .field(
                "auto_promote_next_account_to_admin",
                &self.auto_promote_next_account_to_admin(),
            )
            .finish_non_exhaustive()
    }
}

impl World {
    /// A world with empty managers. `now` is the first tick's clock reading.
    ///
    /// The default shard and auth stores are empty and in memory, on a clock frozen at `now`.
    pub fn new(now: ClockSnapshot, dats: Arc<DatManager>) -> Self {
        let clock: Arc<dyn Clock> = Arc::new(VirtualClock::new(now.utc));
        let (phys_ext, physics) = PhysExtState::new(&dats);
        World {
            now,
            objects: ObjectStore::new(),
            timers: TimersState::from_snapshot(&now),
            world_manager: WorldManagerState::default(),
            landblock_manager: LandblockManagerState::default(),
            player_manager: PlayerManagerState::default(),
            guid_manager: GuidManagerState::default(),
            event_manager: EventManagerState::default(),
            house_manager: HouseManagerState::default(),
            allegiance_manager: AllegianceManagerState::default(),
            recipe_manager: RecipeManagerState::default(),
            property_manager: PropertyManagerState::default(),
            ddd_manager: DddManagerState::default(),
            server_manager: ServerManagerState::default(),
            performance: ServerPerformanceMonitorState::default(),
            mutation_cache: MutationCacheState::default(),
            loot_tables: Default::default(),
            fellowships: crate::entity::fellowship::FellowshipStore::default(),
            chess_matches: crate::entity::chess::chess_match::ChessMatchStore::default(),
            sessions: Sessions::new(),
            net: ServerNet::new(
                NetConfig::default(),
                crate::network::game_messages::game_message::transport_messages(),
            ),
            physics,
            phys_ext,
            server_object_manager: ServerObjectManagerState::default(),
            dats,
            content: Arc::new(MemContent::new()),
            shard: ShardHandle::synchronous(Box::new(MemShard::new()), Arc::clone(&clock)),
            auth: AuthHandle::new(Box::new(MemAuth::new(AccountDefaults::default(), clock))),
            era: empyrean_common::era::current(),
        }
    }
}
