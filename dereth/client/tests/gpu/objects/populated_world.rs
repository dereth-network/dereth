//! A populated world: server objects appear, move, animate and are drawn.
//!
//! Fixture: every recorded session (datagrams between the retail client and a local ACE server),
//! fed through the *whole* application stack: `dereth_client_net`'s transport,
//! `dereth_client_net::client_session`'s four queue dispatchers, `dereth_client_model`'s tables
//! and, for the drawing tests, the render device over the retail dats. Every `0xF745`, `0xF748`
//! and `0xF74C` these tests handle is traffic a real client received from a real server; the
//! recordings hold more of each kind than any single live minute and are the same bytes every
//! time.
//!
//! What is asserted:
//!
//! 1. **Appear.** Every `0xF745` in the capture reaches the object tables, and the count of
//!    creates + merges + stale-instance refusals equals the number of `0xF745` blobs in the
//!    capture. A create for an unknown object is created, not parked on the object it is
//!    creating.
//! 2. **The instance sequence.** The player's own `PhysicsDesc.timestamps.instance` is read off
//!    the wire and is what the session's instance table ends up holding.
//! 3. **Move.** Objects change position from `0xF748`, and the change reaches the renderer's
//!    world-space frame.
//! 4. **Animate.** Applying a movement buffer moves the object's motion interpreter off its
//!    default state and the animation sequence then advances with the clock; every buffer a create
//!    carries decodes. (That every recorded `0xF74C` buffer decodes exactly is the cpu tier's
//!    `movement_corpus_decode`.)
//! 5. **Drawn.** The objects are submitted through the real device and the frame is not black.

#![cfg(gpu)]

use super::common::{
    addr, connection_sequence_number, corpus_sessions, load, retail_store, test_gpu,
};
use crate::common::{recorded_enter_world_requests, recorded_world_sessions};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::{LocalTime, ObjectId, Vec3};
use dereth_protocol::objects::ItemCreateObject;
use dereth_protocol::Message;
use dereth_render::device::Gpu;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

/// The captures whose character actually reaches the world, **measured rather than named**.
///
/// A login-only recording (the account authenticates and disconnects without a character entering
/// the world) carries no `0xF745`, no `0x0013` and no movement.
fn world_sessions() -> &'static [String] {
    static CACHE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let world: Vec<String> = corpus_sessions()
            .into_iter()
            .filter(|s| server_blob_count(s, 0xF745) > 0)
            .collect();
        // As many as carry a `0x0013`, read by `dereth_client_net::client_session::testing`.
        assert_eq!(
            world.len(),
            recorded_world_sessions(),
            "the captures that enter the world, as many as carry a recorded 0x0013"
        );
        world
    })
}

/// How many blobs of one opcode the server sent, counted **at the transport seam** rather than by
/// re-implementing the packet format: this is what `dereth_client_net` handed the session.
fn server_blob_count(session: &str, opcode: u32) -> usize {
    let records = load(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(&records),
    )
    .expect("host");
    for r in records.iter().filter(|r| !r.c2s) {
        net.feed(&r.raw, addr(r.pair), LocalTime(r.t));
    }
    let mut n = 0;
    while let Some(m) = dereth_primitives::Transport::poll(&mut net.session.transport) {
        if m.opcode == opcode {
            n += 1;
        }
    }
    n
}

/// Everything one replayed session produced.
struct Replayed {
    net: ClientNetwork,
    objects: ObjectStream,
    events: Vec<SessionEvent>,
    /// Every distinct position each object was seen at, in order.
    tracks: BTreeMap<ObjectId, Vec<Vec3>>,
    /// The most objects the client held at once. The recorded sessions all end with a clean
    /// logout, which tears the object model down, so the terminal count is 0 by design.
    max_objects: usize,
    /// False if a render presence was ever held for an object with no weenie in the object table.
    consistent: bool,
    /// The index of the **last** datagram after which the client still held objects. Replaying only
    /// that far leaves the stream in world, which the render tests need: every recording ends with
    /// a clean logout, and the end-of-session teardown empties the object model.
    last_populated: usize,
    /// The player's id and its `0xF745`, captured when they arrive rather than read at the end.
    player: Option<(ObjectId, dereth_client_runtime::objects::Presence)>,
}

/// Drive a whole capture through the transport, the session and the object stream — the same three
/// calls `App::frame` makes, minus the renderer.
fn replay(session: &str) -> Replayed {
    replay_upto(session, usize::MAX)
}

/// Replay at most `limit` datagrams. `usize::MAX` is the whole capture.
fn replay_upto(session: &str, limit: usize) -> Replayed {
    let records = load(session);
    assert!(!records.is_empty(), "{session} is empty");
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(&records),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut events = Vec::new();
    let mut tracks: BTreeMap<ObjectId, Vec<Vec3>> = BTreeMap::new();
    let mut entries = recorded_enter_world_requests(
        records
            .iter()
            .enumerate()
            .filter(|(_, r)| r.c2s)
            .map(|(i, r)| (i, r.raw.as_slice())),
    )
    .into_iter()
    .peekable();
    let mut max_objects = 0usize;
    let mut last_populated = 0usize;
    let mut consistent = true;
    let mut player: Option<(ObjectId, dereth_client_runtime::objects::Presence)> = None;

    for (index, r) in records.iter().enumerate() {
        if index >= limit {
            break;
        }
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, addr(r.pair), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        events.extend(objects.pump(&mut net, now));
        // Enter the world where the recording's client did, as the character it named, on every
        // cycle it did (`common::enter_world`).
        if let Some(e) = entries.next_if(|e| e.record == index) {
            net.enter_world(e.character, &e.account);
        }
        max_objects = max_objects.max(objects.len());
        if !objects.is_empty() {
            last_populated = index;
        }
        if player.is_none() {
            if let Some(id) = objects.player() {
                if let Some(p) = objects.presence(id) {
                    player = Some((id, p.clone()));
                }
            }
        }
        for (id, p) in objects.presences() {
            if objects.world.weenie(id).is_none() {
                consistent = false;
            }
            if let Some(pos) = p.position {
                let v = tracks.entry(id).or_default();
                if v.last() != Some(&pos.frame.origin) {
                    v.push(pos.frame.origin);
                }
            }
        }
    }
    Replayed {
        net,
        objects,
        events,
        tracks,
        max_objects,
        consistent,
        last_populated,
        player,
    }
}

// ---------------------------------------------------------------------------------------------
// 1. Appear.
// ---------------------------------------------------------------------------------------------

/// Behaviour: objects.create.every-create-reaches-the-object-tables
///
/// **Every `0xF745` the server sent reaches `dereth_client_model`'s tables**, through the real
/// WorldObjects dispatcher.
///
/// Fixture: every in-world recording. The count is taken at the transport (how many `0xF745`
/// blobs `dereth_client_net` handed up) and compared against what came out of the far end, so
/// the assertion cannot be satisfied by a decoder that silently drops messages.
///
/// A create for an unknown object takes the dispatcher's create path; sending it through the
/// instance gate instead would park it on the object it is creating, and `creates` would be 0.
#[test]
fn every_create_the_server_sent_reaches_the_object_tables() {
    for name in world_sessions() {
        let sent = server_blob_count(name, 0xF745);
        assert!(sent > 0, "{name}: the capture has no creates at all");

        let r = replay(name);
        let s = r.objects.stats;
        assert!(s.creates > 0, "{name}: not one object was created");
        assert_eq!(
            u64::try_from(sent).unwrap(),
            s.creates + s.merges + s.stale_instances,
            "{name}: {sent} creates on the wire, {} created + {} merged + {} stale",
            s.creates,
            s.merges,
            s.stale_instances
        );
        // The tables agree with the stream at every step: an object with a render presence always
        // has a weenie in the object table. Checked during the replay, because every recording
        // ends with a clean logout and the end-of-session teardown tears the object model down.
        assert!(
            r.consistent,
            "{name}: a render presence outlived its weenie"
        );
        assert!(
            r.max_objects > 1,
            "{name}: the client never held more than {} object(s) at once",
            r.max_objects
        );
        // **`0xF747` is not the only message that removes an object.** `0xF7DB
        // Item_UpdateObject` always deletes and rebuilds (`dereth_protocol::objects`), and
        // `house-purchase-and-trade` carries four, so the removes are the sum of both.
        assert_eq!(
            u64::try_from(server_blob_count(name, 0xF747) + server_blob_count(name, 0xF7DB))
                .unwrap(),
            s.removes,
            "{name}: every 0xF747 and 0xF7DB removes an object"
        );
    }
}

/// Datagrams `ClientNetwork` refuses, per capture.
///
/// Zero everywhere except `fellowship-one-vassal`, which is one: the shard re-sent server seq 74
/// after the proxy had already forwarded the first copy, so the same datagram arrives twice;
/// `ClientNetwork` draws a fresh ISAAC key for a sequence it has already consumed, the checksum
/// fails, and the duplicate is dropped. The blob stream is right, and it is right by rejection
/// rather than by sequence number, which is why this is a named expectation rather than a
/// tolerance. `connected.rs` carries the same function for the same reason.
fn expected_rejections(name: &str) -> u32 {
    u32::from(name == "fellowship-one-vassal")
}

/// **The player's instance sequence comes off the wire.** The player's own
/// `PhysicsDesc.timestamps.instance` is the character's **total login count**, it is never 0, and
/// it is what the session's instance table ends up holding.
///
/// Fixture: every in-world recording, and ACE, which sets the object-instance sequence to the
/// character's total logins. The three `fellowship-*` players were created in the recorded
/// session itself, so their first login is the one being recorded and the wire carries 1.
///
/// A client that assumed `0` here would be worse than arbitrary: with 0 in the table,
/// `is_newer(0, 90)` is true and **every** later WorldObjects message about the player parks for
/// ever.
#[test]
fn the_players_own_instance_sequence_comes_off_the_wire() {
    for name in world_sessions() {
        let full = replay(name);
        let mut r = replay_upto(name, full.last_populated + 1);
        let (id, p) = r
            .player
            .clone()
            .unwrap_or_else(|| panic!("{name}: the player has no 0xF745"));
        // The recording's own creates of the player, decoded apart from the client: the first is
        // the one the client latched, the last is the newest the session's gate has seen. A
        // recording that enters the world more than once as the same character re-creates him
        // with a rising total-logins count each time.
        let wire: Vec<u16> = Corpus::shared(name)
            .blobs
            .iter()
            .filter(|b| {
                b.dir == Direction::ServerToClient
                    && b.opcode == 0xF745
                    && b.payload.get(4..8) == Some(id.0.to_le_bytes().as_slice())
            })
            .map(|b| {
                ItemCreateObject::read(&mut dereth_protocol::Reader::new(&b.payload[4..]))
                    .expect("a recorded player create decodes")
                    .0
                    .physicsdesc
                    .timestamps
                    .instance
            })
            .collect();
        let (first, newest) = (wire[0], wire[wire.len() - 1]);
        assert_eq!(p.instance, first, "{name}: the player's instance sequence");
        assert_ne!(p.instance, 0, "{name}: never 0");
        let gate = r.net.session.instances_mut().get(id);
        assert_eq!(
            gate,
            Some(newest),
            "{name}: `object_arrived` put the newest wire value in the session's gate"
        );
        assert!(
            p.position.is_some(),
            "{name}: the player's create carries a position"
        );
        assert!(
            p.setup_id.is_some(),
            "{name}: the player's create carries a setup"
        );
        // And `0x0013` still arrived: it is parked on the player and released by that same call.
        assert!(
            full.events
                .iter()
                .any(|e| matches!(e, SessionEvent::PlayerDescription(_))),
            "{name}: no 0x0013"
        );
        assert_eq!(full.net.rejected(), expected_rejections(name), "{name}");
    }
}

// ---------------------------------------------------------------------------------------------
// 2. Move.
// ---------------------------------------------------------------------------------------------

/// Behaviour: movement.remote.position-events-move-objects
///
/// Objects move: `0xF748 Movement_PositionEvent` changes an object's position, more than once, for
/// more than one object.
///
/// Fixture: the recorded sessions, whose position events are the retail player and the NPCs
/// around him walking about. Each session's own wire count is the expectation here, so the
/// assertion is per capture and cannot be satisfied by a scan that stopped.
#[test]
fn objects_move_from_position_events() {
    let mut total_movers = 0;
    for name in world_sessions() {
        let r = replay(name);
        let sent = server_blob_count(name, 0xF748);
        // **`house-purchase-refused` is the corpus's in-world recording with no `0xF748` at all**,
        // and the zero is measured rather than a scan that stopped: it is five minutes standing
        // at a slumlord with nothing else moving in the cell. Named rather than floored, so a
        // session that *should* carry position events and stops still reddens.
        if name == "house-purchase-refused" {
            assert_eq!(sent, 0, "house carries no 0xF748 at all");
        } else {
            assert!(sent > 0, "{name}: no position events in the capture");
        }
        assert_eq!(
            u64::try_from(sent).unwrap(),
            r.objects.stats.position_updates + r.objects.stats.stale_positions,
            "{name}: every 0xF748 is either applied or refused by the POSITION_TS gate"
        );
        if name == "house-purchase-refused" {
            assert_eq!(
                r.objects.stats.position_updates, 0,
                "house has none to apply"
            );
        } else {
            assert!(
                r.objects.stats.position_updates > 0,
                "{name}: not one position event was applied"
            );
        }

        let movers: Vec<ObjectId> = r
            .tracks
            .iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|(k, _)| *k)
            .collect();
        if name == "house-purchase-refused" {
            assert!(movers.is_empty(), "house records no object moving twice");
        } else {
            assert!(!movers.is_empty(), "{name}: nothing moved");
        }
        total_movers += movers.len();

        // A mover really moved: the two extreme samples are metres apart, not float noise.
        let furthest = movers
            .iter()
            .map(|id| {
                let v = &r.tracks[id];
                let (a, b) = (v[0], v[v.len() - 1]);
                ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
            })
            .fold(0.0f32, f32::max);
        // **The floor is keyed by recording, as `house-purchase-refused`'s zero is.**
        // `post-relog-attribute-training`'s furthest-travelling repeated mover covers
        // **0.887 m**: a real walk, three orders of magnitude above the float noise this guard
        // exists to exclude, but under the metre the others clear. That recording is 470 s of
        // standing still training attributes after a relog, so the only objects that move twice
        // barely move at all. Keyed rather than relaxed: the others still have to clear a metre.
        let floor = if name == "post-relog-attribute-training" {
            0.5
        } else {
            1.0
        };
        if name != "house-purchase-refused" {
            assert!(
                furthest > floor,
                "{name}: the furthest an object travelled was {furthest} m"
            );
        }
    }
    assert!(total_movers >= 3, "only {total_movers} objects ever moved");
}

// ---------------------------------------------------------------------------------------------
// 3. Animate.
// ---------------------------------------------------------------------------------------------

/// A server movement buffer changes what the object's motion interpreter is doing, and the
/// animation then advances with the clock.
///
/// Fixture: the capture's own buffers, applied through `dereth_animation`'s
/// `MotionDriver::unpack_interpreted_movement` (the movement manager's `Invalid` arm) against the
/// retail motion table the object's `PhysicsDesc` names.
///
/// Asserted through the application's own update path with no window and no screenshot:
/// `WorldScene::update` advances every object's sequence, and `curr_frame_number` reports the
/// active sequence frame.
#[test]
fn a_movement_buffer_animates_the_object_it_names() {
    let store = retail_store();
    let mut gpu = test_gpu(800, 600);
    let in_world = replay("first-login-walk-jump").last_populated;
    let mut r = replay_upto("first-login-walk-jump", in_world + 1);
    let (mut scene, _) = scene_for(&store, &mut gpu, &r);

    scene
        .sync_objects(&store, &mut gpu, &mut r.objects)
        .expect("objects sync");
    assert!(
        scene.server_object_count() > 0,
        "no object from the capture became drawable"
    );
    assert!(
        scene.draw.stats.server_objects_animated > 0,
        "no drawable object has a motion table"
    );

    // Every animating object is on some animation frame. Advance the clock a second and at least
    // one of them must be on a different one: a standing cycle plays whether or not anything moves.
    let animated: Vec<ObjectId> = r
        .objects
        .presences()
        .map(|(id, _)| id)
        .filter(|id| scene.server_object_motion(*id).is_some())
        .collect();
    let before: Vec<(ObjectId, i32)> = animated
        .iter()
        .filter_map(|id| scene.server_object_motion(*id).map(|(f, _)| (*id, f)))
        .collect();

    let input = dereth_client_runtime::camera::CameraInput::default();
    let ci = dereth_client_runtime::character::CharacterInput::default();
    let mut t = 0.0f64;
    for _ in 0..60 {
        t += 1.0 / 30.0;
        scene.update(input, ci, LocalTime(t), 1.0 / 30.0);
    }
    let after: Vec<(ObjectId, i32)> = animated
        .iter()
        .filter_map(|id| scene.server_object_motion(*id).map(|(f, _)| (*id, f)))
        .collect();
    assert!(
        before.iter().zip(&after).any(|(a, b)| a.1 != b.1),
        "not one object's animation frame advanced over two simulated seconds"
    );

    // The two stamp gates and the autonomy rule, on the capture's own buffers.
    //
    // Some are the server echoing back the **retail player's own** autonomous movement, and the
    // client's object-movement handler drops those outright: "the player ignores autonomous
    // movement events about himself". Getting that rule backwards is what makes a rebuilt client
    // stutter as it replays its own walk. The rest are other objects and are applied.
    let full = replay("first-login-walk-jump");
    let dispatched = full
        .events
        .iter()
        .filter(|e| matches!(e, SessionEvent::WorldObject { opcode, .. } if opcode.0 == 0xF74C))
        .count();
    let st = full.objects.stats;
    assert_eq!(
        dispatched,
        server_blob_count("first-login-walk-jump", 0xF74C)
    );
    assert_eq!(
        u64::try_from(dispatched).unwrap(),
        st.movement_updates + st.movement_own_echo,
        "every buffer is applied, or dropped as the player's own echo"
    );
    assert!(st.movement_updates > 0, "movement buffers applied");
    assert!(
        st.movement_own_echo > 0,
        "the player's own autonomous echoes, dropped"
    );
    assert_eq!(st.movement_stale, 0);
    assert_eq!(st.movement_old_control, 0);

    // A create carries its own movement buffer — the object's opening animation — and the three
    // header fields come from the descriptor's timestamps rather than from the buffer. Every one of
    // them must decode with its bytes consumed exactly, across every in-world capture.
    let mut embedded = 0u64;
    for name in world_sessions() {
        let full = replay(name);
        assert_eq!(
            full.objects.stats.create_movement_undecodable, 0,
            "{name}: a create's embedded movement buffer failed to decode"
        );
        embedded += full.objects.stats.create_movements;
    }
    assert!(
        embedded > 0,
        "the corpus's creates with an embedded movement buffer"
    );
    assert_eq!(
        r.objects.stats.movement_undecodable, 0,
        "a movement buffer failed to decode, which is a layout bug"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. Drawn.
// ---------------------------------------------------------------------------------------------

/// **The capture's objects are drawn.** They are given geometry, placed where the server said,
/// submitted through the real device, and the frame is not black.
///
/// Fixture: the retail dats for every setup, gfxobj, surface and texture; the capture for which
/// objects exist and where. WARP so the pixels do not depend on the machine's GPU.
#[test]
fn the_captures_objects_are_drawn_where_the_server_put_them() {
    let store = retail_store();
    let mut gpu = test_gpu(800, 600);
    let in_world = replay("first-login-walk-jump").last_populated;
    let mut r = replay_upto("first-login-walk-jump", in_world + 1);
    let (mut scene, landblock) = scene_for(&store, &mut gpu, &r);
    eprintln!("the capture's player is in landblock {landblock:#06X}");

    scene
        .sync_objects(&store, &mut gpu, &mut r.objects)
        .expect("objects sync");
    let s = scene.draw.stats;
    assert!(s.server_objects > 0, "nothing to draw");
    assert!(
        s.server_object_setups > 0 && s.server_object_setups <= s.server_objects,
        "geometry is shared per setup: {} setups for {} objects",
        s.server_object_setups,
        s.server_objects
    );
    assert!(
        s.server_object_triangles > 0,
        "the objects have no geometry"
    );

    // Placed, not piled at the origin: the objects are spread over the landblock.
    let frames: Vec<Vec3> = r
        .objects
        .presences()
        .filter_map(|(id, _)| scene.server_object_frame(id))
        .map(|f| f.origin)
        .collect();
    assert!(frames.len() > 1);
    let spread = frames
        .iter()
        .map(|a| {
            frames
                .iter()
                .map(|b| (a.x - b.x).abs())
                .fold(0.0f32, f32::max)
        })
        .fold(0.0f32, f32::max);
    assert!(spread > 1.0, "every object is at the same x");

    // Draw one frame and look at it. "It rendered" and "it rendered something" are different
    // claims, and the second is the one that catches a scene that silently fails to submit.
    scene
        .reserve_upload_arena(&mut gpu)
        .expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(&mut gpu).expect("draw");
    gpu.end_frame().expect("end");
    let image = gpu.capture().expect("capture");
    let rgba = image.to_rgba();
    let lit = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 8 || p[1] > 8 || p[2] > 8)
        .count();
    assert!(
        lit > (image.width as usize * image.height as usize) / 10,
        "only {lit} lit pixels: the world did not draw"
    );
}

// ---------------------------------------------------------------------------------------------
// Shared setup.
// ---------------------------------------------------------------------------------------------

/// Build the landscape around wherever the capture's server put the player — which is what
/// `App::load_pending_scene` does with a live one.
fn scene_for(store: &Arc<RetailDatStore>, gpu: &mut Gpu, r: &Replayed) -> (WorldScene, u16) {
    let (_, p) = r.player.as_ref().expect("the capture creates a player");
    let pos = p.position.expect("the player has a position");
    let block = pos.cell.landblock();
    let landblock = (u16::from(block.x()) << 8) | u16::from(block.y());
    let cfg = SceneConfig {
        landblock,
        // No body: this module is about the objects the *server* owns, and the local character
        // costs a second of setup that proves nothing here.
        character: false,
        // One ring of blocks. The objects are all in the player's own block and its neighbours, and
        // the default three rings is 49 texture-merged blocks for a test that draws one frame.
        land_radius: 1,
        scenery_radius: 0,
        ..SceneConfig::default()
    };
    let scene = WorldScene::load(store, gpu, cfg).expect("the landscape loads");
    (scene, landblock)
}
