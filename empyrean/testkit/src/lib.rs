//! The server's in-process test harness: a whole server on a virtual clock, with wire-speaking bots
//! over an in-memory network.
//!
//! **Depends on** the server crates it assembles (`empyrean-common`, `empyrean-entity`,
//! `empyrean-content`, `empyrean-store`, `empyrean-dat`, `empyrean-net`, `empyrean-world`) and the
//! shared `dereth-primitives`, `dereth-assets`, `dereth-physics`, `dereth-transport`,
//! `dereth-protocol` and `dereth-dat` (its test lookup of the retail dats). **Used by** the server crates' tests only (`empyrean-world` and
//! `empyrean-command` as a dev-dependency), and by its own scenario suites.
//!
//! **Must never** sleep or touch a real network: ten minutes of virtual time take well under a
//! second, and the bots speak to the server over [`MemoryNet`]. Nothing here is an ACE port; there
//! are no ACE anchors.
//!
//! A [`TestServer`] is a [`World`] on [`FakeDats`] (or data files you pass) with ACE's start-up
//! `Timers`; its transport on a [`MemoryNet`] with perfect links; a [`VirtualClock`] (2026-01-01
//! 00:00 UTC at start) and a fixed seed ([`TestServer::SEED`]); and the persistence stores in
//! memory. [`TestServer::shard`] and [`TestServer::auth`] reach the stores;
//! [`TestServer::with_setup`] seeds them before the start-up sequence (`ConfigManager`, the
//! property, player, world and event managers) runs.
//!
//! [`TestServer::advance`] runs world iterations until the given virtual time has passed. Each is
//! one `WorldManager.UpdateWorld`: the clients tick and send, the world ticks, the clients receive,
//! then the clock moves on by [`TestServer::TICK`] (one game-world update, 1/60 s rounded up to the
//! 100 ns clock tick), so a reply takes one tick. [`TestClient`] is a bot: the shared client
//! transport plus the retail login handshake; [`TestServer::connect`] adds one and runs until its
//! link is up. Each client keeps its own local clock from when it was added.
//! [`TestServer::take_not_ported`] returns the `not_ported!` sites the test thread hit since the
//! last call.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use dereth_primitives::{IncomingMessage, NetQueue};
use dereth_protocol::{self as proto, Message};
use empyrean_common::account_defaults::AccountDefaults;
use empyrean_common::clock::{Clock, ClockSnapshot, VirtualClock};
use empyrean_common::config_manager::ConfigManager;
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::master_configuration::MasterConfiguration;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_dat::{DatManager, FakeDats};
use empyrean_net::driver::memory::MemoryNet;
use empyrean_net::{NetConfig, PortKind, ServerNet};
use empyrean_store::{AuthDatabase, MemAuth, MemShard, ShardDatabase, ShardHandle};
use empyrean_world::entity::timers::{self, TimersState};
use empyrean_world::managers::world_manager::{NetDriver, UpdateWorldTick};
use empyrean_world::managers::{
    event_manager, house_manager, player_manager, property_manager, server_manager, world_manager,
};
use empyrean_world::world::AuthHandle;
use empyrean_world::World;

pub use empty_shard::EmptyShard;
pub use empyrean_net::testing::{ClientStatus, TestClient};

#[cfg(feature = "captures")]
pub mod capture_replay;
pub mod dats;
pub mod decode;
mod empty_shard;
pub mod land;
#[cfg(feature = "soak")]
pub mod soak;
/// The soak's relog and guid invariants: built without the `soak` feature too, so that the unit
/// tier tests them on synthetic content. `soak` re-exports it.
#[path = "soak/invariants.rs"]
pub mod soak_invariants;
/// The soak's measures and its host-relative profile: no real content needed, so they are
/// built without the `soak` feature and the unit tier tests them. `soak` re-exports both.
#[path = "soak/metrics.rs"]
pub mod soak_metrics;
#[path = "soak/profile.rs"]
pub mod soak_profile;

/// Which client of a [`TestServer`]: the index [`TestServer::connect`] returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClientId(pub usize);

/// The in-memory wire: a [`MemoryNet`] carrying the world's transport.
///
/// `MemoryNet` owns the `ServerNet` it drives, while the world owns its transport (`World.net`), so
/// for `DoSessionWork` the two are swapped in, pumped, and swapped back.
#[derive(Debug)]
struct MemoryWire {
    mem: MemoryNet,
}

impl NetDriver for MemoryWire {
    fn receive(&mut self, _net: &mut ServerNet, _now: ClockSnapshot) {
        // `MemoryNet::pump` delivers and polls in one step, in `do_session_work`.
    }

    fn do_session_work(&mut self, net: &mut ServerNet, now: ClockSnapshot) -> usize {
        std::mem::swap(net, &mut self.mem.server);
        self.mem.now = now;
        // the pass's DoSessionWork count, which includes the sessions it dropped.
        let session_count = self.mem.pump();
        std::mem::swap(net, &mut self.mem.server);
        session_count
    }
}

#[derive(Debug)]
struct Slot {
    client: TestClient,
    /// Every message the client received, in its queue-drain order.
    inbox: Vec<IncomingMessage>,
    /// The next game action's sequence (`OrderedActionHeader.stamp`).
    next_action_stamp: u32,
    /// The server's uptime (virtual seconds) when the client was added: the client's own local
    /// clock reads the uptime minus this.
    added_at: f64,
}

/// A whole server in-process on a virtual clock (see the crate docs).
#[derive(Debug)]
pub struct TestServer {
    /// The world. Tests may inspect and change it between iterations.
    pub world: World,
    /// The virtual clock the world reads.
    pub clock: Arc<VirtualClock>,
    wire: MemoryWire,
    clients: Vec<Slot>,
    /// Set when the shutdown sequence reached ACE's `Environment.Exit`.
    exited: bool,
    /// What the last world iteration reported.
    last_tick: Option<UpdateWorldTick>,
    /// The test's own configuration ([`with_config`](Self::with_config)), on this thread only.
    _config: Option<empyrean_common::config_manager::ConfigOverride>,
}

impl Default for TestServer {
    fn default() -> Self {
        Self::new()
    }
}

impl TestServer {
    /// The fixed seed of the world thread's RNG and of the transport's seeds and cookies.
    pub const SEED: u64 = 0x0DE8_E715;

    /// One iteration's virtual time: 1/60 s, rounded up to the next 100 ns.
    pub const TICK: Duration = Duration::from_nanos(16_666_700);

    /// How long [`connect`](Self::connect) waits for the link to come up.
    pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

    /// The server's address on the in-memory network (its port `P` is 9000).
    pub const SERVER_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));

    /// A server on [`FakeDats`] with only the neutral stat tables ([`dats::with_stat_tables`]).
    ///
    /// # Panics
    /// Never: empty fake dats always open.
    #[must_use]
    pub fn new() -> Self {
        Self::with_dats(
            dats::with_stat_tables(FakeDats::new())
                .build()
                .expect("fake dats"),
        )
    }

    /// A server on the given dats.
    #[must_use]
    pub fn with_dats(dats: Arc<DatManager>) -> Self {
        Self::with_setup(dats, |_| {})
    }

    /// A server on the given dats; `setup` runs on the world once its stores exist and before
    /// `Program.Main`'s start-up steps (see the crate docs), to seed the databases.
    #[must_use]
    pub fn with_setup(dats: Arc<DatManager>, setup: impl FnOnce(&mut World)) -> Self {
        Self::build(dats, None, setup)
    }

    /// A server whose `ConfigManager.Config` is `config`. The configuration applies to this
    /// test's thread only (`ConfigManager::override_for_thread`) for the server's lifetime, so tests
    /// running in parallel keep reading the default one. A test that needs a non-default
    /// configuration uses this, never `ConfigManager::initialize`.
    #[must_use]
    pub fn with_config(
        dats: Arc<DatManager>,
        config: MasterConfiguration,
        setup: impl FnOnce(&mut World),
    ) -> Self {
        Self::build(dats, Some(config), setup)
    }

    fn build(
        dats: Arc<DatManager>,
        config: Option<MasterConfiguration>,
        setup: impl FnOnce(&mut World),
    ) -> Self {
        let config_override = config.map(ConfigManager::override_for_thread);
        ThreadSafeRandom::seed(Self::SEED);
        // ConfigManager.Initialize(): the default configuration (every test server uses it).
        ConfigManager::initialize(MasterConfiguration::default());

        let clock = Arc::new(VirtualClock::default());
        let timers = TimersState::new(&*clock);
        let now = ClockSnapshot::take(&*clock, timers.portal_year_ticks);
        let mut world = World::new(now, dats);
        world.timers = timers;
        let shared_clock: Arc<dyn Clock> = Arc::<VirtualClock>::clone(&clock);
        world.shard =
            ShardHandle::synchronous(Box::new(MemShard::new()), Arc::clone(&shared_clock));
        world.auth = AuthHandle::new(Box::new(MemAuth::new(
            AccountDefaults::default(),
            shared_clock,
        )));
        property_manager::install_shard_config(
            &mut world,
            property_manager::shard_config_handle(Box::new(MemShard::new())),
        );

        let config = NetConfig {
            rng_seed: Self::SEED,
            ..NetConfig::default()
        };
        world.net = ServerNet::new(
            config.clone(),
            empyrean_world::network::game_messages::game_message::transport_messages(),
        );
        let mem = MemoryNet::new(
            ServerNet::new(
                config,
                empyrean_world::network::game_messages::game_message::transport_messages(),
            ),
            Self::SERVER_IP,
        );

        setup(&mut world);

        // Program.Main's start-up, in its order.
        property_manager::initialize(&mut world, true);
        player_manager::initialize(&mut world);
        house_manager::initialize(&mut world);
        world_manager::initialize(&world);
        event_manager::initialize(&mut world);

        // WorldManager.UpdateWorld sets this as its thread starts.
        world.world_manager.world_active = true;

        if !world_manager::property_manager_get_bool_world_closed(&world) {
            world_manager::open(&mut world, None);
        }

        Self {
            world,
            clock,
            wire: MemoryWire { mem },
            clients: Vec::new(),
            exited: false,
            last_tick: None,
            _config: config_override,
        }
    }

    /// `DatabaseManager.Authentication`, locked: seed accounts
    /// (`ts.auth().create_account(..)`) or inspect them.
    pub fn auth(&self) -> std::sync::MutexGuard<'_, Box<dyn AuthDatabase>> {
        self.world.auth.lock()
    }

    /// `DatabaseManager.Shard.BaseDatabase`: the shard itself, for seeding characters and biotas
    /// (`ts.shard().add_character_in_parallel(..)`) or inspecting what was saved.
    pub fn shard(&self) -> std::sync::MutexGuard<'_, Box<dyn ShardDatabase>> {
        self.world.shard.base_database()
    }

    /// Virtual seconds since the server started (the clock's monotonic time).
    #[must_use]
    pub fn seconds(&self) -> f64 {
        self.clock.monotonic().as_secs_f64()
    }

    /// Runs iterations until `secs` of virtual time have passed (at least one iteration).
    pub fn advance(&mut self, secs: f64) {
        let end = self.clock.monotonic() + Duration::from_secs_f64(secs);
        loop {
            self.step();
            if self.clock.monotonic() >= end {
                break;
            }
        }
    }

    /// Runs iterations until `until` holds (checked after each) or `max_secs` of virtual time have
    /// passed. Returns whether `until` held.
    pub fn run_until(&mut self, max_secs: f64, mut until: impl FnMut(&mut Self) -> bool) -> bool {
        let end = self.clock.monotonic() + Duration::from_secs_f64(max_secs);
        loop {
            self.step();
            if until(self) {
                return true;
            }
            if self.clock.monotonic() >= end {
                return false;
            }
        }
    }

    /// One iteration (see the crate docs). After the world has stopped, only the clients and the
    /// clock move.
    pub fn step(&mut self) {
        let now = ClockSnapshot::take(&*self.clock, self.world.timers.portal_year_ticks);
        let t = now.monotonic.as_secs_f64();
        self.wire.mem.now = now;

        for slot in &mut self.clients {
            slot.client.tick(t - slot.added_at);
            for (to, bytes) in slot.client.take_outgoing() {
                self.wire.mem.client_send(slot.client.addr, to, bytes);
            }
        }

        if self.world.world_manager.world_active {
            self.last_tick = Some(self.world.tick(now, &mut self.wire));
        }

        for slot in &mut self.clients {
            while let Some(d) = self.wire.mem.client_recv(slot.client.addr) {
                slot.client
                    .handle_datagram(d.from, &d.bytes, t - slot.added_at);
            }
            while let Some(m) = slot.client.poll() {
                // Every message dereth-protocol can read must write back to the bytes the server sent.
                decode::assert_reencodes(&m);
                slot.inbox.push(m);
            }
        }

        self.clock.advance(Self::TICK);
        if self.world.world_manager.world_active {
            timers::advance_portal_year_ticks(&mut self.world, TimeSpan::from_ticks(tick_ticks()));
        }

        // What ACE's shutdown thread does beside the loop; then the loop's `while` check.
        if !self.exited {
            self.exited = server_manager::shutdown_server(&mut self.world);
        }
        if self.world.world_manager.world_active && self.world.world_manager.pending_world_stop {
            self.world.world_manager.world_active = false;
            if !self.exited {
                self.exited = server_manager::shutdown_server(&mut self.world);
            }
        }
    }

    /// What the last world iteration reported (`UpdateGameWorld` ran; `DoSessionWork`'s session
    /// count). `None` before the first iteration.
    #[must_use]
    pub fn last_tick(&self) -> Option<UpdateWorldTick> {
        self.last_tick
    }

    /// Whether a shutdown ran to ACE's `Environment.Exit`.
    #[must_use]
    pub fn exited(&self) -> bool {
        self.exited
    }

    /// Adds a client logging in as `account`/`password` without running anything.
    pub fn add_client(&mut self, account: &str, password: &str) -> ClientId {
        let n = self.clients.len();
        let host = u8::try_from(n % 250 + 2).unwrap_or(2);
        let port = 50_000 + u16::try_from(n / 250).unwrap_or(0);
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 1, host)), port);
        let server = self.wire.mem.server_addr(PortKind::C2S);
        self.clients.push(Slot {
            client: TestClient::new(addr, server, account, password),
            inbox: Vec::new(),
            next_action_stamp: 1,
            added_at: self.seconds(),
        });
        ClientId(n)
    }

    /// Adds a client and runs until its link is up ([`ClientStatus::Connected`]) or
    /// [`CONNECT_TIMEOUT`](Self::CONNECT_TIMEOUT) of virtual time has passed.
    pub fn connect(&mut self, account: &str, password: &str) -> ClientId {
        let id = self.add_client(account, password);
        self.run_until(Self::CONNECT_TIMEOUT.as_secs_f64(), |ts| {
            ts.client(id).status() == ClientStatus::Connected
        });
        id
    }

    /// A client.
    ///
    /// # Panics
    /// When `id` is not a client of this server.
    #[must_use]
    pub fn client(&self, id: ClientId) -> &TestClient {
        &self.clients[id.0].client
    }

    /// A client, mutably (for example to log off).
    ///
    /// # Panics
    /// When `id` is not a client of this server.
    pub fn client_mut(&mut self, id: ClientId) -> &mut TestClient {
        &mut self.clients[id.0].client
    }

    /// Queues a game action (`0xF7B1`, the next sequence number, `M`'s opcode and body) on the
    /// client's `Weenie` queue; it leaves with the client's next tick.
    ///
    /// # Panics
    /// When `m` cannot be encoded, or `id` is not a client of this server.
    pub fn send_game_action<M: Message>(&mut self, id: ClientId, m: &M) {
        let slot = &mut self.clients[id.0];
        let data =
            proto::actions::pack_action(slot.next_action_stamp, m).expect("encodable game action");
        slot.next_action_stamp = slot.next_action_stamp.wrapping_add(1);
        slot.client.send(NetQueue::Weenie, &data);
    }

    /// Queues a message (`M`'s opcode and body) on one of the client's queues.
    ///
    /// # Panics
    /// When `m` cannot be encoded, or `id` is not a client of this server.
    pub fn send_message<M: Message>(&mut self, id: ClientId, queue: NetQueue, m: &M) {
        let data = proto::write_blob(m).expect("encodable message");
        self.clients[id.0].client.send(queue, &data);
    }

    /// Every message of type `M` the client has received, decoded, oldest first.
    ///
    /// # Panics
    /// When a message with `M`'s opcode does not decode as `M`, or `id` is not a client of this
    /// server.
    #[must_use]
    pub fn received<M: Message>(&self, id: ClientId) -> Vec<M> {
        self.clients[id.0]
            .inbox
            .iter()
            .filter(|m| m.opcode == M::OPCODE.0)
            .map(|m| proto::read_body_padded::<M>(&m.body).expect("a received message decodes"))
            .collect()
    }

    /// Takes every message the client has received so far, leaving its inbox empty (a long
    /// run drains its bots' inboxes so that the harness does not grow without bound).
    ///
    /// # Panics
    /// When `id` is not a client of this server.
    pub fn take_received(&mut self, id: ClientId) -> Vec<IncomingMessage> {
        std::mem::take(&mut self.clients[id.0].inbox)
    }

    /// How many clients have been added.
    #[must_use]
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// Every message the client has received, raw.
    ///
    /// # Panics
    /// When `id` is not a client of this server.
    #[must_use]
    pub fn received_raw(&self, id: ClientId) -> &[IncomingMessage] {
        &self.clients[id.0].inbox
    }

    /// The `not_ported!` sites this thread hit since the last call (or since it started), with
    /// their hit counts.
    #[must_use]
    pub fn take_not_ported() -> BTreeMap<&'static str, u64> {
        empyrean_common::not_ported::take_local()
    }
}

/// [`TestServer::TICK`] in 100 ns ticks.
fn tick_ticks() -> i64 {
    i64::try_from(TestServer::TICK.as_nanos() / 100).unwrap_or(i64::MAX)
}
