//! Long-session resource growth under portal and leave-return churn: every leave-return cycle
//! gives back everything it took, so no application or world table grows with the number of
//! cycles. A client that draws the returning player perfectly and keeps one row per departed
//! object in a process-wide table is correct on every frame and unusable after an evening of
//! portalling.
//!
//! Fixture: a headless App on the retail dats and a software device, driven socket-free through
//! the encrypted replay endpoint with encoded server messages. It runs three leave-return shapes
//! many times over on one long-lived `App` and, after
//! every cycle, samples **56 selected application and world resource gauges** -- the game model's
//! nine object-maintenance tables, the per-object memos beside the physics bodies, cell membership
//! and shadow lists, renderer caches and reserved upload bytes, land-source caches, GPU descriptor
//! and texture counts, the HUD radar, and the session's per-object tables. For each gauge, the
//! minimum over the final third must not exceed the maximum of three warm-up cycles plus its stated
//! allowance.
//!
//! # Why a baseline and not zero
//!
//! Three cycles of warm-up run **before** the baseline is taken, because several of these caches
//! are supposed to fill and stay full: `ObjectPhysics::geometry_cache_count` memoises a collision
//! hull per `(setup, placement)`, `WorldScene::live_appearances` memoises geometry per
//! `AppearanceKey`, the game world's physics-setup census memoises a setup's holding locations, and the
//! texture table is keyed by payload. None of those is keyed by object id, so they saturate on the
//! first pass and a test that demanded zero would be asserting the caches do not work.
//! These object-id- and cell-id-keyed gauges are checked against the warm-up ceiling and their
//! per-gauge allowance. The fresh-guid control arms never reuse retired ids, so a row left behind
//! for each departed creature remains observable across later cycles; the same-guid merge arms
//! exercise the complementary path.
//!
//! # The three shapes and the release path each one is supposed to reach
//!
//! | shape | wire | the release it must reach |
//! |---|---|---|
//! | A, the asymmetric expiry | silence, then ACE's re-track: `0xF745` + wielded `0xF745` + `0xF74B` | none on the merge arm; the newer-instance arm of `create_or_merge` deletes and rebuilds the old body |
//! | B, the interior departure | `0xF748` naming an env cell the block does not carry, `0xF74B HIDDEN_PS`, 25 s, then the return triple | failed placement removes the object and its children from cells, queues them for destruction, and the 25 s maintenance deadline deletes them |
//! | C, the landblock round trip | the local `0xF748` with an advanced `TELEPORT_TS`, out and back, 25 s away, NPCs re-tracked under the same guids and under new ones | the teleport releases the whole landscape window, clears each object's cell and queues its deadline; object maintenance deletes it after 25 s, and the return refills the window |
//!
//! Shape C alternates same-guid and new-guid returns because ACE's landblock unload reissues
//! dynamic guids (`Entity/Landblock.cs:118`, `WorldObjects/Creature.cs:95`), and a new guid is the
//! case that can grow: the old id is gone for ever, so anything still keyed by it is lost storage.
//! Both arms cross the client's 25 s deadline first, so the old set is always culled before the
//! new one arrives -- away for *less* than 25 s with new guids is the one shape that legitimately
//! doubles, and it is deliberately not run here.
//!
//! # What this test cannot see
//!
//! Most rows sample collection lengths; the upload-arena row samples reserved bytes. A `Vec` that
//! is drained but never shrinks, a freed arena slot kept for reuse, and allocator-retained pages
//! are outside those gauges. Process working set is not sampled.
//!
//! Socket-free: every message is encoded and admitted through the encrypted
//! replay endpoint and the ordinary `App::frame` order, and the 25 s deadline is crossed by a real
//! `TimeSync` optional header, which advances the application clock.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::{app::App, config::Config, world::SceneConfig};
use dereth_primitives::{CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::{
    movement::{position_flags, MovementPositionEvent, PositionPack},
    objects::{ItemCreateObject, ItemSetState, LoginCreatePlayer, ObjectCreatePayload},
    types::{
        physicsdesc::{flags, ChildLink},
        weeniedesc::header,
        ObjDesc, PhysicsDesc, PhysicsEventStamp, PhysicsTimestamps, PositionWire, PublicWeenieDesc,
    },
    Message,
};
use dereth_ui_screens::mapradar::radar::{inq_showable_on_radar, radar_enum};

const REMOTE: ObjectId = ObjectId(0x5000_1C95);
const WAND: ObjectId = ObjectId(0x5000_1C96);
const LOCAL: ObjectId = ObjectId(0x5000_1C01);
/// Shape C's NPC guids start here and advance by one block of three per cycle, because ACE's
/// landblock reload issues **fresh** dynamic guids and never reuses a retired one.
const NPC_BASE: u32 = 0x5000_2000;
/// Shape B's fresh `(holder, held)` pairs, on the same rule.
const RETURN_BASE: u32 = 0x5000_3000;

const PLAYER_SETUP: u32 = 0x0200_0001;
const PLAYER_MTABLE: u32 = 0x0900_0001;
const PLAYER_PSCRIPT_TABLE: u32 = 0x3400_0004;
const WAND_SETUP: u32 = 0x0200_1713;
const RIGHT_HAND: u32 = 1;

const HIDDEN: u32 = 0x0040_4410;
const VISIBLE: u32 = 0x0040_0408;
const PLAYER_BIT: u32 = 0x0000_0008;
/// The object-maintenance deadline is the current application time plus 25 seconds.
const DESTRUCTION_TIME: f64 = 25.0;

/// Cycles measured after the warm-up, for the two shapes that churn objects alone.
const CYCLES: usize = 30;
/// The same for shape C, which is the expensive one: every cycle rebuilds the whole nine-block
/// landblock window **twice** (out and back), several times the cost of a shape-A cycle. Nine
/// measured round trips give the tail three cycles to be flat over, and they alternate the
/// same-guid and new-guid returns as every other count here does. A leak of one row per departed
/// object would be +27 rows by the end of them, and a leak of any row per cycle in a gauge with no
/// allowance still sits above the baseline on every tail cycle.
const LANDBLOCK_CYCLES: usize = 9;
/// Cycles run before the baseline census, so the id-independent caches are saturated.
const WARMUP: usize = 3;

const NPC_NAMES: [&str; 3] = ["Aun Tigrana", "Baron Nerine", "Town Crier"];

// =================================================================================================
// Socket-free peer -- the encrypted replay endpoint
// =================================================================================================

struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
}

fn server_addr() -> std::net::SocketAddr {
    "127.0.0.1:19000".parse().expect("a literal address")
}

impl Peer {
    /// **A long-session test has to complete `ClientNetwork`'s own handshake.**
    ///
    /// A fixture that installs the connection directly leaves the client link idle, and the login
    /// cadence then runs against a server that will never answer: immediately, then every 2.0 s,
    /// for at most twenty attempts (more than `0x13` requests sent), after
    /// which it raises the client-timeout error and every later
    /// `receive_use_time` returns without delivering anything.
    ///
    /// **That budget is forty seconds of local wall-clock time, and under `--headless`'s fixed step
    /// the local clock follows wall time**: the network's cadences must not depend
    /// on the frame rate. A twenty-two-cycle run finishes inside it; a ninety-nine-cycle one does
    /// not, and the symptom is not a timeout message but creates that silently stop arriving
    /// mid-run: a missing body for the first tracked creature, with a timed-out-server link state.
    ///
    /// So the fixture answers with a real `ConnectRequest` through `ClientNetwork::feed`. That path
    /// builds the same receiver a direct connection would have (same recipient, iteration and
    /// seeds), records the local time of the last received data, and moves the state machine to
    /// its login-connecting state, whose
    /// `ConnectResponse` cadence has **no attempt limit**. Every later packet goes through the
    /// same entry point at the real local time, so the transport's 140 s silence clock receives
    /// the clock it actually compares against instead of a frozen zero.
    fn attach(app: &mut App) -> Self {
        let mut net = dereth_client::net::ClientNetwork::new(
            "127.0.0.1:19000",
            7304,
            "long-session-growth",
            "unused",
            0,
        )
        .unwrap();
        let mut hello = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            rec_id: 0xB,
            iteration: 1,
            ..Default::default()
        });
        hello
            .add_optional_header(
                dereth_transport::PacketFlags::CONNECT_REQUEST,
                dereth_transport::conn::ConnectRequest {
                    server_time: 0.0,
                    cookie: 7,
                    net_id: 0,
                    outgoing_seed: 0xDEAD_BEEF,
                    incoming_seed: 0x1234_5678,
                }
                .to_bytes()
                .to_vec(),
            )
            .unwrap();
        // `ConnectResponse` is disposable and unencrypted, so `serialize` takes no key.
        net.feed(
            &hello.serialize(None).unwrap(),
            server_addr(),
            LocalTime(app.clock().local_time),
        );
        assert_eq!(
            net.status(),
            dereth_client::net::LinkStatus::LoginConnecting,
            "the ConnectRequest took the login FSM out of its twenty-attempt state"
        );
        app.attach_replay_network(net).unwrap();
        Self {
            crypto: dereth_transport::CryptoSystem::new(0xDEAD_BEEF),
            sequence: 1,
            blob: 0,
        }
    }

    fn send<M: Message>(&mut self, app: &mut App, message: &M) {
        self.send_batch(app, &[dereth_protocol::write_blob(message).unwrap()], None);
    }

    fn send_batch(&mut self, app: &mut App, blobs: &[Vec<u8>], server_time: Option<f64>) {
        self.sequence += 1;
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: self.sequence,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        if let Some(t) = server_time {
            packet
                .add_optional_header(
                    dereth_transport::PacketFlags::TIME_SYNC,
                    t.to_le_bytes().to_vec(),
                )
                .unwrap();
        }
        for blob in blobs {
            self.blob += 1;
            packet
                .add_fragment(dereth_transport::Fragment::new(
                    dereth_transport::FragmentHeader {
                        blob_id_low: self.blob,
                        blob_id_high: 0x8000_0000,
                        num_frags: 1,
                        blob_frag_size: 0,
                        blob_num: 0,
                        queue_id: 10,
                    },
                    blob.clone(),
                ))
                .unwrap();
        }
        let raw = packet.serialize(Some(self.crypto.next())).unwrap();
        // Use `ClientNetwork::feed` rather than calling the transport directly, and supply the
        // current local time. See [`Self::attach`].
        let now = LocalTime(app.clock().local_time);
        let rejected = app.replay_network_mut().unwrap().rejected();
        app.replay_network_mut()
            .unwrap()
            .feed(&raw, server_addr(), now);
        assert_eq!(
            app.replay_network_mut().unwrap().rejected(),
            rejected,
            "the datagram was accepted by the production receive path"
        );
    }

    /// Advance the server clock while the blob beside the header is a state repeat the `STATE_TS`
    /// gate refuses because its timestamp comparison is strict. Normal frame lifecycle work still
    /// runs after the clock advances.
    fn advance_to(
        &mut self,
        app: &mut App,
        id: ObjectId,
        state: u32,
        stamp: u16,
        instance: u16,
        t: f64,
    ) {
        let repeat = dereth_protocol::write_blob(&set_state(id, state, stamp, instance)).unwrap();
        self.send_batch(app, &[repeat], Some(t));
    }

    /// Push the current application time past the client's destruction deadline.
    fn cross_deadline(&mut self, app: &mut App, id: ObjectId, state: u32, instance: u16) {
        let after = app.clock().cur_time + DESTRUCTION_TIME + 5.0;
        self.advance_to(app, id, state, 0, instance, after);
    }
}

// =================================================================================================
// Wire shapes (the leave-return shapes; this test varies the *number* of cycles, not the bytes)
// =================================================================================================

fn seen_wdesc(name: &str, player: bool) -> PublicWeenieDesc {
    PublicWeenieDesc {
        header: header::RADAR_ENUM,
        name: name.to_string(),
        wcid: 1,
        icon_id: 0x0600_1036,
        obj_type: if player { 0 } else { 0x10 },
        bitfield: if player { PLAYER_BIT } else { 0 },
        radar_enum: Some(radar_enum::SHOW_ALWAYS),
        ..PublicWeenieDesc::default()
    }
}

#[allow(clippy::too_many_arguments)]
fn create(
    id: ObjectId,
    wdesc: PublicWeenieDesc,
    at: Position,
    instance: u16,
    position_stamp: u16,
    state_stamp: u16,
    state: u32,
    children: Option<ObjectId>,
) -> ItemCreateObject {
    let mut bitfield = flags::POSITION | flags::SETUP | flags::MTABLE | flags::PETABLE;
    if children.is_some() {
        bitfield |= flags::CHILDREN;
    }
    ItemCreateObject(ObjectCreatePayload {
        id,
        objdesc: ObjDesc::default(),
        physicsdesc: PhysicsDesc {
            bitfield,
            state,
            setup_id: Some(PLAYER_SETUP),
            mtable_id: Some(PLAYER_MTABLE),
            phstable_id: Some(PLAYER_PSCRIPT_TABLE),
            position: Some(PositionWire {
                objcell_id: at.cell.0,
                frame: dereth_protocol::types::Frame {
                    origin: at.frame.origin.into(),
                    orientation: at.frame.rotation.into(),
                },
            }),
            children: children.map(|c| {
                vec![ChildLink {
                    child_id: c,
                    location_id: RIGHT_HAND,
                }]
            }),
            timestamps: PhysicsTimestamps {
                instance,
                position: position_stamp,
                state: state_stamp,
                ..Default::default()
            },
            ..Default::default()
        },
        wdesc,
    })
}

/// The wielded item's `0xF745`, as ACE's `TrackEquippedObject` writes it.
fn wand(id: ObjectId, parent: ObjectId, instance: u16) -> ItemCreateObject {
    ItemCreateObject(ObjectCreatePayload {
        id,
        objdesc: ObjDesc::default(),
        physicsdesc: PhysicsDesc {
            bitfield: flags::SETUP | flags::PARENT | flags::ANIMFRAME,
            setup_id: Some(WAND_SETUP),
            parent: Some((parent, RIGHT_HAND)),
            animframe_id: Some(1),
            timestamps: PhysicsTimestamps {
                instance,
                ..Default::default()
            },
            ..Default::default()
        },
        wdesc: seen_wdesc("Sceptre", false),
    })
}

fn remote_position(id: ObjectId, at: Position, stamp: u16) -> MovementPositionEvent {
    position_message(id, at, 1, stamp, 0)
}

fn position_message(
    id: ObjectId,
    at: Position,
    instance: u16,
    stamp: u16,
    teleport: u16,
) -> MovementPositionEvent {
    let flags = position_flags::IS_GROUNDED
        | position_flags::ORIENTATION_HAS_NO_W
        | position_flags::ORIENTATION_HAS_NO_X
        | position_flags::ORIENTATION_HAS_NO_Y
        | position_flags::ORIENTATION_HAS_NO_Z;
    MovementPositionEvent {
        id,
        position: PositionPack {
            flags,
            origin: dereth_protocol::types::Origin {
                objcell_id: at.cell.0,
                origin: dereth_protocol::types::Vec3 {
                    x: at.frame.origin.x,
                    y: at.frame.origin.y,
                    z: at.frame.origin.z,
                },
            },
            instance_timestamp: instance,
            position_timestamp: stamp,
            teleport_timestamp: teleport,
            ..PositionPack::default()
        },
    }
}

fn set_state(id: ObjectId, state: u32, event: u16, instance: u16) -> ItemSetState {
    ItemSetState {
        id,
        state,
        timestamps: PhysicsEventStamp { instance, event },
    }
}

// =================================================================================================
// Observers
// =================================================================================================

/// Count this id's entries in the current scene draw order. This observes submitted parts, not
/// hidden allocations elsewhere in the renderer.
fn drawn(app: &App, id: ObjectId) -> usize {
    app.world_scene()
        .unwrap()
        .drawn_part_order()
        .iter()
        .filter(|part| part.object == Some(id))
        .count()
}

fn doomed(app: &App, id: ObjectId) -> bool {
    app.objects().world.tables.doomed.get(id).is_some()
}

fn known(app: &App, id: ObjectId) -> bool {
    app.objects().presence(id).is_some()
}

/// Query the current HUD radar rows and their showability predicate.
fn radar_dot(app: &App, id: ObjectId) -> bool {
    app.hud()
        .radar
        .iter()
        .any(|b| b.id == id && inq_showable_on_radar(b))
}

/// Count the two indexed physics-body views: the tracked handle and `by_object_id`. This is not a scan of every arena slot.
fn bodies(app: &App, id: ObjectId) -> usize {
    let scene = app.world_scene().unwrap();
    let world = &scene.character.as_ref().unwrap().world;
    let tracked = app
        .objects()
        .physics
        .handle(id)
        .filter(|h| world.get(*h).is_some());
    let arena = world.by_object_id(id);
    usize::from(tracked.is_some()) + usize::from(arena.is_some() && arena != tracked)
}

fn drawn_named(app: &App, name: &str) -> usize {
    app.objects()
        .presences()
        .filter(|(id, _)| {
            app.objects()
                .world
                .weenie(*id)
                .is_some_and(|w| w.pwd.name == name)
                && drawn(app, *id) > 0
        })
        .count()
}

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        assert!(app.frame());
    }
}

// =================================================================================================
// The census
// =================================================================================================

/// One row of the census: a name, and how many of that thing are live **right now**.
type Row = (&'static str, usize);

/// The selected application and world counters that the leave/return cycles exercise.
///
/// Order is fixed and later censuses compare names positionally, so a row is declared here only.
/// `&mut App` rather than `&App` is required because the session tables are reachable only through
/// [`App::replay_network_mut`].
fn census(app: &mut App) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::with_capacity(56);
    {
        let objects = app.objects();
        let tables = &objects.world.tables;
        rows.push(("objects.presences", objects.len()));
        rows.push(("objects.position_entries", objects.position_entry_count()));
        rows.push(("objects.queued_edges", objects.queued_edge_count()));
        rows.push(("tables.weenies", tables.weenies.len()));
        rows.push(("tables.null_weenies", tables.null_weenies.len()));
        rows.push(("tables.physics", tables.physics.len()));
        rows.push(("tables.null_physics", tables.null_physics.len()));
        rows.push(("tables.inventories", tables.inventories.len()));
        rows.push(("tables.lost_cells", tables.lost_cells.len()));
        rows.push(("tables.visible", tables.visible.len()));
        rows.push(("tables.doomed", tables.doomed.len()));
        rows.push(("tables.doom_queue", tables.doom_queue.len()));
        rows.push(("world.physics_setups", objects.world.physics_setup_count()));

        let physics = &objects.physics;
        rows.push(("objphys.handles", physics.len()));
        rows.push(("objphys.geometry_cache", physics.geometry_cache_count()));
        rows.push(("objphys.posed", physics.posed_count()));
        rows.push(("objphys.placed", physics.placed_count()));
        rows.push(("objphys.stated", physics.stated_count()));
        rows.push(("objphys.restricted", physics.restricted_count()));
        rows.push(("objphys.lost", physics.lost_count()));
        rows.push(("objphys.retired", physics.retired_count()));
    }
    {
        let scene = app.world_scene().expect("a world scene");
        let character = scene.character.as_ref().expect("a body");
        let world = &character.world;
        rows.push(("phys.bodies", world.body_count()));
        rows.push(("phys.object_table", world.object_table_count()));
        rows.push(("phys.cells", world.cell_count()));
        rows.push(("phys.cell_objects", world.cell_object_count()));
        rows.push(("phys.cell_shadows", world.cell_shadow_count()));
        rows.push(("phys.static_animating", world.static_animating_count()));
        rows.push(("phys.notices", world.pending_notice_count()));

        rows.push(("scene.server_objects", scene.server_object_count()));
        rows.push(("scene.appearances", scene.live_appearances()));
        rows.push(("scene.resident_blocks", scene.resident_blocks()));
        rows.push(("scene.env_cells", scene.env_cell_counts().0));
        rows.push(("scene.cell_static_bodies", scene.cell_static_bodies()));
        rows.push(("scene.emitter_hosts", scene.draw.stats.emitter_hosts));
        rows.push(("scene.particle_meshes", scene.draw.stats.particles.meshes));
        rows.push(("scene.pending_slots", scene.pending_slot_count()));
        rows.push(("scene.released_blocks", scene.released_block_count()));
        rows.push(("scene.released_interiors", scene.released_interior_count()));
        rows.push(("scene.entering_world", scene.entering_world_count()));
        rows.push(("scene.pending_sound", scene.pending_sound_count()));
        rows.push((
            "scene.pending_restrictions",
            scene.pending_restriction_count(),
        ));
        rows.push(("scene.upload_bytes", scene.upload_reservation()));

        let land = character.land();
        rows.push(("land.collision_blocks", land.resident()));
        rows.push(("land.cached_blocks", land.cached_block_count()));
        rows.push(("land.env_cells", land.resident_cells()));
        rows.push(("land.buildings", land.resident_buildings()));
        rows.push(("land.building_cells", land.building_cell_count()));
    }
    rows.push((
        "gpu.descriptors_live",
        app.renderer().descriptor_usage().live as usize,
    ));
    rows.push(("gpu.textures", app.renderer().texture_keys().len()));
    rows.push(("gpu.ui_textures", app.renderer().ui_texture_count()));
    rows.push(("hud.radar", app.hud().radar.len()));
    {
        let session = &app
            .replay_network_mut()
            .expect("the replay endpoint")
            .session;
        rows.push(("session.instances", session.instance_count()));
        rows.push(("session.weenies", session.weenie_count()));
        rows.push(("session.stampers", session.stamper_count()));
        rows.push(("session.parked_blobs", session.parked_blob_count()));
        rows.push(("session.events", session.pending_event_count()));
    }
    rows
}

/// How much a counter may exceed its baseline without being called growth, and **why**.
///
/// Zero is the default and the interesting answer. The exceptions are the caches that are keyed by
/// something other than an object id, which are allowed to keep filling if the shape introduces a
/// new key after the warm-up -- but even those are capped, because "it is a cache" is not the same
/// claim as "it is bounded".
fn allowance(name: &str) -> usize {
    match name {
        // Keyed by `(setup, placement)` / `AppearanceKey` / setup id, never by object id. Every
        // shape here uses two setups (the player body and the wand), so after the warm-up these
        // are saturated; the slack is for a placement the warm-up happened not to reach.
        "objphys.geometry_cache" | "scene.appearances" | "world.physics_setups" => 2,
        // Keyed by texture payload. Shape C's teleport reloads a landblock window, and a block the
        // warm-up did not stream can add its terrain and scenery textures once. Bounded by the
        // distinct blocks the shape visits, not by the number of cycles.
        "gpu.textures" | "gpu.descriptors_live" | "gpu.ui_textures" => 64,
        // Bytes, not rows: the upload arena is reserved for whatever geometry is resident this
        // frame, and a window whose streaming has not finished settling reserves less, not more.
        // The cap is one landblock's worth of terrain and statics.
        "scene.upload_bytes" => 4 * 1024 * 1024,
        _ => 0,
    }
}

/// The element-wise **maximum** over a run of censuses.
///
/// This is what "the baseline" means here, and the maximum rather than the last census is
/// deliberate. Two of the three shapes alternate arms (shape A merges on an even cycle and takes
/// the create-or-merge path's delete-and-rebuild on an odd one; shape C re-tracks the same guids on an
/// even cycle and fresh ones on an odd one), and the body ends each cycle three metres to one side
/// or the other. A steady state reached by alternation genuinely has **two** values — the
/// shadow-object list is one entry longer when the body straddles a cell boundary — and the upper
/// one is the ceiling of the steady state, not growth. Taking the last warm-up census instead
/// would make the assertion depend on the parity of `WARMUP`.
fn ceiling(series: &[Vec<Row>]) -> Vec<Row> {
    let mut out = series.first().expect("at least one census").clone();
    for census in &series[1..] {
        for (i, row) in census.iter().enumerate() {
            assert_eq!(row.0, out[i].0, "the census changed shape between cycles");
            out[i].1 = out[i].1.max(row.1);
        }
    }
    out
}

/// Assert each counter's final-third floor is no greater than its post-warm-up ceiling plus the
/// counter's allowance, and print the table either way.
///
/// **The statistic is the minimum over the tail, and that is the whole argument.** A container
/// that retains one row for every fresh retired id never returns to where it started, so its
/// *floor* rises with its ceiling. A container that merely
/// blips — one deferred edge caught by the census between the frame that queued it and the frame
/// that drains it — returns to the floor on the very next cycle. Comparing peaks would call the
/// second one a leak. Requiring the minimum over the final third to remain within the warm-up
/// ceiling plus allowance permits such recovery while detecting a sustained increase across every
/// sampled tail cycle. It can miss intermittent growth and does not prove the absence of every
/// kind of long-session resource growth.
///
/// The peak is still printed, because a transient is worth seeing even when it is not a failure.
fn assert_no_growth(label: &str, warmup: &[Vec<Row>], series: &[Vec<Row>]) {
    let baseline = ceiling(warmup);
    assert!(
        series.len() >= 3,
        "{label}: too few measured cycles to have a tail"
    );
    let tail_from = series.len() - series.len() / 3;

    let mut report = String::new();
    let mut leaks: Vec<String> = Vec::new();
    for (i, (name, base)) in baseline.iter().enumerate() {
        let values: Vec<usize> = series
            .iter()
            .map(|c| {
                assert_eq!(
                    c[i].0, *name,
                    "{label}: the census changed shape between cycles"
                );
                c[i].1
            })
            .collect();
        let peak = *values.iter().max().expect("a measured cycle");
        let low = *values.iter().min().expect("a measured cycle");
        let tail_floor = *values[tail_from..].iter().min().expect("a tail cycle");
        let slack = allowance(name);
        let over = tail_floor.saturating_sub(base + slack);
        report.push_str(&format!(
            "  {name:<26} baseline {base:>10}  min {low:>10}  max {peak:>10}  \
             tail-floor {tail_floor:>10}{}\n",
            if over > 0 {
                format!("  <-- +{over} over baseline+{slack}")
            } else {
                String::new()
            }
        ));
        if over > 0 {
            // The first-difference series says *how* it grew: one step on one cycle is a different
            // defect from a row added every cycle, and the failure message has to tell them apart.
            #[allow(clippy::cast_possible_wrap)]
            // LINT-OK: container lengths, far below i64::MAX.
            let deltas: Vec<i64> = std::iter::once(*base as i64)
                .chain(values.iter().map(|v| *v as i64))
                .collect::<Vec<_>>()
                .windows(2)
                .map(|w| w[1] - w[0])
                .collect();
            leaks.push(format!(
                "{name}: baseline {base}, and over {} cycles it never came back below \
                 {tail_floor} (allowance {slack}); per-cycle delta {deltas:?}",
                series.len()
            ));
        }
    }
    eprintln!(
        "long-session growth {label} -- {} cycles after {} warm-up:\n{report}",
        series.len(),
        warmup.len()
    );
    assert!(
        leaks.is_empty(),
        "{label}: {} container(s) never returned to their post-warm-up baseline. A leave/return \
         cycle must give back everything it took; a row kept per departed object is storage this \
         session can never reclaim, because object ids are never reused.\n{}",
        leaks.len(),
        leaks.join("\n")
    );
}

// =================================================================================================
// The App and its fixture helpers
// =================================================================================================

fn app() -> App {
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        // `Hud::sync` -- and therefore the radar row -- runs inside `App::ui_use_time`'s shell
        // block, so the radar census needs the real shell.
        ui: true,
        preferences_file: std::env::temp_dir()
            .join("dereth-long-session-growth-not-created/prefs.ini"),
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .unwrap();
    app.start_shell().unwrap();
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    app.load_static_scene(SceneConfig {
        character: true,
        cell_statics: false,
        mesh_collision: false,
        land_radius: 1,
        scenery_radius: 0,
        particles: true,
        release_interiors: true,
        ..Default::default()
    })
    .unwrap();
    assert!(app.frame());
    app
}

fn in_view(app: &App, right: f32) -> Position {
    let scene = app.world_scene().unwrap();
    let character = scene.character.as_ref().unwrap();
    let mut position = character.world.get(character.handle).unwrap().position;
    position.frame.origin =
        dereth_physics::math::localtoglobal(&scene.camera.frame(), Vec3::new(right, 10.0, 0.0));
    position
}

fn beside(app: &App, id: ObjectId, dx: f32) -> Position {
    let handle = app.objects().physics.handle(id).expect("placed body");
    let scene = app.world_scene().unwrap();
    let mut position = scene
        .character
        .as_ref()
        .unwrap()
        .world
        .get(handle)
        .unwrap()
        .position;
    position.frame.origin.x += dx;
    position
}

fn near_player(app: &App, dx: f32, dy: f32) -> Position {
    let scene = app.world_scene().unwrap();
    let character = scene.character.as_ref().unwrap();
    let mut position = character.world.get(character.handle).unwrap().position;
    position.frame.origin.x += dx;
    position.frame.origin.y += dy;
    let land = character.land().clone();
    let block = position.cell.landblock();
    if let Some(z) = land.ground_height(block, position.frame.origin.x, position.frame.origin.y) {
        position.frame.origin.z = z + 0.5;
    }
    position
}

/// `0xF746` raises the login tunnel and skips the
/// whole world while it is up, so a fixture that measures the draw pass has to wait it out.
fn wait_until_drawn(app: &mut App, ids: &[ObjectId], budget: usize) -> usize {
    for n in 0..budget {
        if ids.iter().all(|id| drawn(app, *id) > 0) {
            return n;
        }
        assert!(app.frame());
    }
    panic!(
        "{ids:?} never reached the draw pass in {budget} frames -- if the world is still hidden, \
         the login tunnel's world-hidden gate never cleared"
    );
}

fn home_block(app: &App) -> LandblockId {
    app.world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position()
        .cell
        .landblock()
}

fn on_terrain(app: &App, block: LandblockId, x: f32, y: f32) -> Position {
    let land = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .land()
        .clone();
    let z = land.ground_height(block, x, y).unwrap_or(0.0) + 0.5;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: bounded to 0..8 by the callers' coordinates, which are inside one block.
    let index = ((x / 24.0) as u16) * 8 + ((y / 24.0) as u16) + 1;
    Position::new(
        block.cell(index),
        Frame::new(Vec3::new(x, y, z), Quat::IDENTITY),
    )
}

/// `LandSource::env_cell` only queries resident geometry; it never loads a cell.
fn env_cell_known(app: &App, cell: CellId) -> bool {
    let land = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .land()
        .clone();
    dereth_physics::LandSource::env_cell(&*land, cell).is_some()
}

/// An interior cell index of `block` that the client holds no geometry for -- the one reachable
/// form of "the departure cannot be placed".
fn undecoded_interior(app: &App, block: LandblockId) -> CellId {
    (0x0100..0x0200u16)
        .map(|i| block.cell(i))
        .find(|c| !env_cell_known(app, *c))
        .expect("an interior index the block does not carry")
}

/// The local player's own `0xF748` with an advanced `TELEPORT_TS` follows the received-position
/// path into local teleport handling. Its accepted teleport arm changes position and releases the
/// whole landscape block array.
fn teleport_local(app: &mut App, peer: &mut Peer, to: Position, stamp: &mut u16) {
    *stamp = stamp.wrapping_add(1);
    let applied = app.player_teleports_applied();
    peer.send(app, &position_message(LOCAL, to, 1, *stamp, *stamp));
    frames(app, 14);
    assert_eq!(
        app.player_teleports_applied(),
        applied + 1,
        "the local 0xF748 to {:#010X} was admitted",
        to.cell.0
    );
}

/// Player and wand on screen, through the actual draw pass. Shapes A and B start from here.
fn seen(app: &mut App, peer: &mut Peer) {
    let home = in_view(app, 0.0);
    peer.send(
        app,
        &create(
            REMOTE,
            seen_wdesc("Returning Player", true),
            home,
            1,
            0,
            0,
            VISIBLE,
            Some(WAND),
        ),
    );
    peer.send(app, &wand(WAND, REMOTE, 1));
    frames(app, 31);
    assert!(
        drawn(app, REMOTE) > 0,
        "the remote body starts through the actual draw pass"
    );
    assert!(
        drawn(app, WAND) > 0,
        "its wand starts through the same pass"
    );
    assert!(radar_dot(app, REMOTE), "and it starts with a radar blip");
}

// =================================================================================================
// Shape A -- ACE's asymmetric expiry, over and over
// =================================================================================================

/// One cycle of shape A: the server forgets him in silence, then re-tracks him.
///
/// `handle_visible_obj`'s occluded arm queues the object for destruction without removing it from
/// `VisibleObjects` (`PhysicsObj.cs:2830..2836`), and `AddVisibleObject` refuses to un-queue an id
/// already in that dictionary (`ObjectMaint.cs:435..436`), so the 25 s expiry fires on a player
/// standing in view and sends no `0xF747` at all. The re-track is `GameMessageCreateObject` for
/// the player with his `Children` list, one per wielded item, then the login-complete `0xF74B`
/// (`Player_Tracking.cs:51..75`).
///
/// `rebuild` takes `create_or_merge`'s **newer-instance** arm -- delete the body and
/// build a new one -- which is the arm that actually destroys something and therefore the arm a
/// per-object table can fail to clean up. Alternated with the merge arm so both are measured.
fn shape_a_cycle(app: &mut App, peer: &mut Peer, n: usize, stamp: &mut u16, instance: &mut u16) {
    // The silence. Nothing about the remote arrives; only the clock moves.
    let server_now = app.clock().cur_time + 30.0;
    peer.advance_to(app, REMOTE, VISIBLE, 0, *instance, server_now);
    frames(app, 3);

    let rebuild = n % 2 == 1;
    if rebuild {
        *instance = instance.wrapping_add(1);
    }
    *stamp = stamp.wrapping_add(4);
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    // LINT-OK: `n` is bounded by CYCLES + WARMUP, far inside f32's exact integer range.
    let back = beside(app, REMOTE, if n % 2 == 0 { 3.0 } else { -3.0 });
    peer.send(
        app,
        &create(
            REMOTE,
            seen_wdesc("Returning Player", true),
            back,
            *instance,
            *stamp,
            *stamp,
            HIDDEN,
            Some(WAND),
        ),
    );
    peer.send(app, &wand(WAND, REMOTE, *instance));
    frames(app, 2);
    // `Player_Tracking.cs:54..70` -- the login-complete `0xF74B` closes the re-track.
    peer.send(
        app,
        &set_state(REMOTE, VISIBLE, stamp.wrapping_add(1), *instance),
    );
    frames(app, 26);

    assert_eq!(
        bodies(app, REMOTE),
        1,
        "A cycle {n}: exactly one physics body answers to the id"
    );
    assert!(
        known(app, REMOTE),
        "A cycle {n}: and the re-track left him tracked"
    );
    assert!(!doomed(app, REMOTE), "A cycle {n}: on no destruction queue");
    assert!(
        drawn(app, REMOTE) > 0,
        "A cycle {n}: the completed unhide returned his parts to the draw pass"
    );
    assert_eq!(
        app.objects().presence(WAND).and_then(|p| p.parent),
        Some((REMOTE, RIGHT_HAND)),
        "A cycle {n}: the wand is attached to the returned player"
    );
    assert_eq!(
        bodies(app, WAND),
        0,
        "A cycle {n}: a held child owns no independent body"
    );
}

#[test]
fn thirty_asymmetric_expiry_cycles_return_every_container_to_baseline() {
    let mut app = app();
    let mut peer = Peer::attach(&mut app);
    seen(&mut app, &mut peer);

    let mut stamp: u16 = 1;
    let mut instance: u16 = 1;
    let mut warmup = Vec::with_capacity(WARMUP);
    for n in 0..WARMUP {
        shape_a_cycle(&mut app, &mut peer, n, &mut stamp, &mut instance);
        warmup.push(census(&mut app));
    }

    let mut series = Vec::with_capacity(CYCLES);
    for n in WARMUP..WARMUP + CYCLES {
        shape_a_cycle(&mut app, &mut peer, n, &mut stamp, &mut instance);
        series.push(census(&mut app));
    }

    assert_no_growth(
        "shape A (asymmetric expiry, merge and rebuild arms)",
        &warmup,
        &series,
    );
    app.shutdown();
}

// =================================================================================================
// Shape B -- the interior departure, the 25 s cull, and the return
// =================================================================================================

/// One cycle of shape B: the departure that cannot be placed, the deadline, and ACE's return.
///
/// The departure is a `0xF748` naming an interior cell the resident block does not carry, which is
/// the only reachable form of an unplaceable destination on this build (residency and env-cell
/// decoding are one latch). `ObjectPhysics::place` runs `enter_world`,
/// `LandSource::env_cell` answers nothing, and the null-cell arm takes the body out of every
/// cell list, stores the destination, clears `ACTIVE_TS` and latches `lost` -- which dooms the
/// wielder **and his wand** at +25 s. The `TimeSync` then crosses the deadline and
/// object maintenance deletes both when the `TimeSync` crosses the deadline.
///
/// **On an odd cycle the pair comes back under fresh guids, and that is what makes this test
/// able to see a leak at all.** Every table measured here is keyed by `ObjectId`, so a cycle that
/// deletes an id and re-creates the *same* id overwrites the row it failed to remove and nothing
/// grows however broken the cleanup is. An even cycle is still run, because the returning **player** really does keep his
/// guid; an odd cycle is what ACE sends for everything that is not a player, whose guid is dynamic
/// and reissued (`WorldObjects/Creature.cs:95`), and for the wielded item that came back with him.
///
/// Returns the pair now live, which the next cycle has to cull.
fn shape_b_cycle(
    app: &mut App,
    peer: &mut Peer,
    n: usize,
    stamp: &mut u16,
    away: CellId,
    live: (ObjectId, ObjectId),
) -> (ObjectId, ObjectId) {
    let (holder, held) = live;
    *stamp = stamp.wrapping_add(4);
    let departure = Position::new(away, Frame::new(Vec3::new(12.0, 12.0, 0.0), Quat::IDENTITY));
    peer.send(app, &remote_position(holder, departure, *stamp));
    peer.send(app, &set_state(holder, HIDDEN, *stamp, 1));
    frames(app, 3);
    assert!(
        doomed(app, holder),
        "B cycle {n}: the unplaceable departure doomed him"
    );
    assert!(doomed(app, held), "B cycle {n}: and his held wand with him");

    peer.cross_deadline(app, holder, HIDDEN, 1);
    frames(app, 3);
    assert!(
        !known(app, holder),
        "B cycle {n}: object maintenance culled him at his deadline"
    );
    assert!(
        !known(app, held),
        "B cycle {n}: and culled the wand with its wielder"
    );
    assert_eq!(
        bodies(app, holder),
        0,
        "B cycle {n}: and left no body behind"
    );

    // The return, in ACE's order: create with the `Children` list, the wielded item, `0xF74B`.
    let fresh = n % 2 == 1;
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: `n` is bounded by CYCLES + WARMUP; the product cannot approach u32's range.
    let (holder, held) = if fresh {
        (
            ObjectId(RETURN_BASE + (n as u32 + 1) * 0x10),
            ObjectId(RETURN_BASE + (n as u32 + 1) * 0x10 + 1),
        )
    } else {
        (holder, held)
    };
    *stamp = stamp.wrapping_add(4);
    let back = in_view(app, 2.0);
    peer.send(
        app,
        &create(
            holder,
            seen_wdesc("Returning Player", true),
            back,
            1,
            *stamp,
            *stamp,
            HIDDEN,
            Some(held),
        ),
    );
    peer.send(app, &wand(held, holder, 1));
    frames(app, 2);
    peer.send(app, &set_state(holder, VISIBLE, stamp.wrapping_add(1), 1));
    frames(app, 30);

    assert_eq!(
        bodies(app, holder),
        1,
        "B cycle {n}: one body for the returned id"
    );
    assert!(!doomed(app, holder), "B cycle {n}: on no destruction queue");
    assert!(drawn(app, holder) > 0, "B cycle {n}: he draws again");
    assert!(
        drawn(app, held) > 0,
        "B cycle {n}: and the wand draws attached to him"
    );
    if fresh {
        assert!(
            !known(app, live.0),
            "B cycle {n}: the retired guid stays retired"
        );
        assert_eq!(bodies(app, live.0), 0, "B cycle {n}: and owns no body");
    }
    (holder, held)
}

#[test]
fn thirty_interior_departure_cull_and_return_cycles_return_every_container_to_baseline() {
    let mut app = app();
    let mut peer = Peer::attach(&mut app);
    seen(&mut app, &mut peer);
    let away = undecoded_interior(&app, home_block(&app));
    assert!(
        !away.is_outdoor(),
        "an env cell index, not one of the 64 land cells"
    );

    let mut stamp: u16 = 1;
    let mut live = (REMOTE, WAND);
    let mut warmup = Vec::with_capacity(WARMUP);
    for n in 0..WARMUP {
        live = shape_b_cycle(&mut app, &mut peer, n, &mut stamp, away, live);
        warmup.push(census(&mut app));
    }

    let mut series = Vec::with_capacity(CYCLES);
    for n in WARMUP..WARMUP + CYCLES {
        live = shape_b_cycle(&mut app, &mut peer, n, &mut stamp, away, live);
        series.push(census(&mut app));
    }

    // **The release point, asserted directly and not only through its symptom.** Every fresh-guid
    // cycle leaves one object message addressed to an id this client has just culled. The stream
    // parks that blob on a null placeholder and queues the placeholder for destruction at +25 s.
    // If this reads zero, the deadline pass in
    // `ObjectStream::use_time` never fired and `session.parked_blobs` above stayed flat for some
    // other reason.
    assert!(
        app.objects().stats.parked_blob_owners_destroyed > 0,
        "object maintenance destroyed no expired null placeholder over {} cycles, so the parked-blob deadline is not the thing keeping session.parked_blobs flat",
        CYCLES
    );

    assert_no_growth(
        "shape B (interior departure, 25 s cull, return)",
        &warmup,
        &series,
    );
    app.shutdown();
}

// =================================================================================================
// Shape C -- the landblock round trip, with the same guids and with new ones
// =================================================================================================

/// One cycle of shape C: leave the landblock, stay away past the client's 25 s, come back, and let
/// the server re-track the NPCs.
///
/// The teleport out uses the real position-change arm and releases the whole landscape block
/// array. That release clears each NPC's cell and gives it a lifecycle destruction deadline;
/// crossing the deadline lets object maintenance delete all three. The teleport back re-streams
/// the window, and then the NPCs
/// arrive again.
///
/// On an even cycle they arrive under the **same** guids, which is what ACE sends when the
/// landblock stayed loaded. On an odd cycle they arrive under **new** ones, which is what ACE sends
/// after `UnloadInterval` (`Entity/Landblock.cs:118`) rebuilt the block's creatures
/// (`WorldObjects/Creature.cs:95`). The new-guid arm is the one that can grow: the retired ids are
/// never named again, so anything still keyed by them is unreachable storage.
///
/// Returns the ids now live, which the next cycle has to cull.
fn shape_c_cycle(
    app: &mut App,
    peer: &mut Peer,
    n: usize,
    stamp: &mut u16,
    live: &[ObjectId],
    home: Position,
    away: Position,
) -> Vec<ObjectId> {
    teleport_local(app, peer, away, stamp);
    for (id, name) in live.iter().zip(NPC_NAMES) {
        assert!(
            doomed(app, *id),
            "C cycle {n}: {name}'s landblock was released and the object has a lifecycle destruction deadline"
        );
    }
    peer.cross_deadline(app, LOCAL, VISIBLE, 1);
    frames(app, 3);
    for (id, name) in live.iter().zip(NPC_NAMES) {
        assert!(
            !known(app, *id),
            "C cycle {n}: object maintenance culled {name} at his deadline"
        );
    }

    teleport_local(app, peer, home, stamp);
    let fresh_guids = n % 2 == 1;
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: `n` is bounded by CYCLES + WARMUP; the product cannot approach u32's range.
    let base = if fresh_guids {
        NPC_BASE + (n as u32 + 1) * 0x10
    } else {
        live[0].0
    };
    let returned: Vec<ObjectId> = (0..3u32).map(|i| ObjectId(base + i)).collect();

    *stamp = stamp.wrapping_add(4);
    for (i, (id, name)) in returned.iter().zip(NPC_NAMES).enumerate() {
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        // LINT-OK: `i` is 0..3.
        let spot = near_player(app, (i as f32 - 1.0) * 3.0, 8.0);
        peer.send(
            app,
            &create(
                *id,
                seen_wdesc(name, false),
                spot,
                1,
                *stamp,
                *stamp,
                VISIBLE,
                None,
            ),
        );
    }
    frames(app, 30);

    for (id, name) in returned.iter().zip(NPC_NAMES) {
        assert_eq!(
            bodies(app, *id),
            1,
            "C cycle {n}: exactly one body answers to {name}'s id"
        );
        assert!(
            !doomed(app, *id),
            "C cycle {n}: {name} is on no destruction queue"
        );
        assert!(
            drawn(app, *id) > 0,
            "C cycle {n}: {name} draws again after the return"
        );
        assert_eq!(
            drawn_named(app, name),
            1,
            "C cycle {n}: exactly one {name} on screen -- a return that crossed the 25 s deadline \
             culls the old set before the new one arrives, whichever guids it carries"
        );
    }
    if fresh_guids {
        for (id, name) in live.iter().zip(NPC_NAMES) {
            assert!(
                !known(app, *id),
                "C cycle {n}: {name}'s retired guid stays retired"
            );
            assert_eq!(bodies(app, *id), 0, "C cycle {n}: and owns no body");
        }
    }
    returned
}

/// Behaviour: presentation.growth.leave-return-churn-returns-every-container-to-baseline
#[test]
fn nine_landblock_round_trips_return_every_container_to_baseline() {
    let mut app = app();
    let mut peer = Peer::attach(&mut app);

    // The local player, so the `0xF748` below is the real teleport path and not a remote move.
    let start = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position();
    peer.send(&mut app, &LoginCreatePlayer { player_id: LOCAL });
    let mut me = create(
        LOCAL,
        seen_wdesc("Survivor", true),
        start,
        1,
        0,
        0,
        VISIBLE,
        None,
    );
    me.0.physicsdesc.setup_id = Some(dereth_client::character::ALUVIAN_MALE_SETUP.0);
    peer.send(&mut app, &me);
    frames(&mut app, 4);
    assert_eq!(
        app.objects().player(),
        Some(LOCAL),
        "the local player id is the server's"
    );

    let block = home_block(&app);
    let far = LandblockId((u16::from(block.x().wrapping_add(5)) << 8) | u16::from(block.y()));
    let away = on_terrain(&app, far, 96.0, 96.0);

    let mut stamp: u16 = 1;
    let mut live: Vec<ObjectId> = (0..3u32).map(|i| ObjectId(NPC_BASE + i)).collect();
    for (i, (id, name)) in live.iter().zip(NPC_NAMES).enumerate() {
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        // LINT-OK: `i` is 0..3.
        let spot = near_player(&app, (i as f32 - 1.0) * 3.0, 8.0);
        peer.send(
            &mut app,
            &create(*id, seen_wdesc(name, false), spot, 1, 0, 0, VISIBLE, None),
        );
    }
    let tunnel = wait_until_drawn(&mut app, &live, 2000);
    frames(&mut app, 31);
    eprintln!(
        "long-session growth shape C -- the login tunnel hid the world for {tunnel} frame(s)"
    );
    for name in NPC_NAMES {
        assert_eq!(
            drawn_named(&app, name),
            1,
            "one {name} on screen to begin with"
        );
    }

    // The home position is taken once, from the body's own settled pose, so every cycle teleports
    // back to the same spot rather than drifting with the camera.
    let home = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position();

    let mut warmup = Vec::with_capacity(WARMUP);
    for n in 0..WARMUP {
        live = shape_c_cycle(&mut app, &mut peer, n, &mut stamp, &live, home, away);
        warmup.push(census(&mut app));
    }

    let mut series = Vec::with_capacity(LANDBLOCK_CYCLES);
    for n in WARMUP..WARMUP + LANDBLOCK_CYCLES {
        live = shape_c_cycle(&mut app, &mut peer, n, &mut stamp, &live, home, away);
        series.push(census(&mut app));
    }

    assert_no_growth(
        "shape C (landblock round trip, same and new guids)",
        &warmup,
        &series,
    );
    app.shutdown();
}
