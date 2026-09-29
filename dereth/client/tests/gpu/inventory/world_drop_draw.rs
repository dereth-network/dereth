//! A pack item dropped into the world is drawn: an item held in a container (a setup id, neither
//! position nor parent) has no drawable, and the `0xF748 Movement_PositionEvent` that takes it out
//! of the container (the whole physical answer to a drop for the dropping client, which already
//! knows the item) must reach the renderer so the item's parts are submitted. The draw list
//! (`WorldScene::drawn_part_order`) is compared across three independent replays: the item left in
//! the pack twice (a noise floor that must be zero) and the drop (only the item's parts added).
//! Fixture: the `long-solo-play` recording replayed to the datagram where the player holds the most
//! such pack items, the retail dats, a software device; the `0xF748` is constructed from the
//! recorded player's position with resting placement and `POSITION_TS` advanced by one.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::collections::BTreeSet;
use std::sync::Arc;

use dereth_client::character::CharacterInput;
use dereth_client::models::PLACEMENT_RESTING;
use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_client_net::client_session::testing::shared_session;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_dat::RetailDatStore;
use dereth_primitives::{LocalTime, ObjectId, Vec3};
use dereth_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};
use dereth_protocol::types::space::{Origin, Quat as WireQuat, Vec3 as WireVec3};
use dereth_protocol::{write_body, Opcode};
use dereth_render::device::Gpu;

/// A recording with a populated outdoor landblock and a player with a full pack.
const SESSION: &str = "long-solo-play";

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn warp() -> Gpu {
    crate::common::software_gpu(800, 600)
}

// ---------------------------------------------------------------------------------------------
// The capture reader.
// ---------------------------------------------------------------------------------------------

/// Replay `long-solo-play` up to and including datagram `stop_at`, into a **fresh** stream.
///
/// `ObjectStream` is not `Clone` and `take_created` is a drain, so every arm below replays for
/// itself; handing one stream to two scenes would let the second see an empty queue.
fn replay(stop_at: usize) -> ObjectStream {
    let records = shared_session(SESSION);
    let seq = connection_sequence_number(records).expect("the capture has no LoginRequest");
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", seq).expect("host");
    let mut objects = ObjectStream::with_store(store());
    let mut entered = false;
    for r in records.iter().take(stop_at + 1) {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, r.peer(), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
        }
    }
    objects
}

/// The replay point: which datagram, which landblock, which player, and which of the player's pack
/// items is dropped.
struct Station {
    stop_at: usize,
    landblock: u16,
    player: ObjectId,
    /// The chosen inventory item has a setup ID and **neither** position nor parent: the
    /// object-maintenance representation of a contained object.
    item: ObjectId,
    /// Every such item at this datagram, for the census the drop test prints.
    pack: BTreeSet<ObjectId>,
    /// Where the drop puts it: the dropping player's own position.
    cell: u32,
    origin: Vec3,
}

/// Walk the whole recording once, and stop at the datagram at which the player owns the **most**
/// drawable-less pack items while standing somewhere with a position of his own.
fn station() -> Station {
    let records = shared_session(SESSION);
    let seq = connection_sequence_number(records).expect("the capture has no LoginRequest");
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", seq).expect("host");
    let mut objects = ObjectStream::with_store(store());
    let mut entered = false;
    let mut best: Option<(usize, usize)> = None;
    for (index, r) in records.iter().enumerate() {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, r.peer(), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
        }
        let Some(player) = objects.player() else {
            continue;
        };
        if objects.presence(player).and_then(|p| p.position).is_none() {
            continue;
        }
        let n = pack_items(&objects, player).len();
        if n > best.map_or(0, |(_, n)| n) {
            best = Some((index, n));
        }
    }
    let (stop_at, _) =
        best.expect("long-solo-play never has the player positioned with a pack item");
    let objects = replay(stop_at);
    let player = objects.player().expect("the station has a player");
    let pos = objects
        .presence(player)
        .and_then(|p| p.position)
        .expect("and a position");
    let pack = pack_items(&objects, player);
    let item = *pack
        .iter()
        .next()
        .expect("the station has at least one pack item");
    let b = pos.cell.landblock();
    Station {
        stop_at,
        landblock: (u16::from(b.x()) << 8) | u16::from(b.y()),
        player,
        item,
        pack,
        cell: pos.cell.0,
        origin: pos.frame.origin,
    }
}

/// The player's own **pack**, restricted to what a drop could possibly draw.
///
/// Four conditions, and every one of them is load-bearing:
///
/// * a setup ID, or there is nothing to draw whatever else is true;
/// * the contained physical representation — **neither** position nor parent, which is the
///   physical state that makes `prepare_object_dispatch` skip it;
/// * owned by the player, so the item is one he could drop;
/// * a zero location quality and nonzero container ID, selecting `InContainer` rather than
///   `Wielded`. A **worn** item passes the first three — clothing has a setup and no physics
///   parent of its own, because it is drawn out of the wearer's `ObjDesc` and not as a child —
///   and it is not a pack item.
fn pack_items(s: &ObjectStream, player: ObjectId) -> BTreeSet<ObjectId> {
    s.presences()
        .filter(|(_, p)| p.position.is_none() && p.parent.is_none() && p.setup_id.is_some())
        .map(|(id, _)| id)
        .filter(|id| *id != player && s.world.is_owned_by_object(*id, player))
        .filter(|id| {
            s.world.weenie(*id).is_some_and(|w| {
                w.pwd.location.unwrap_or(0) == 0 && w.pwd.container_id.unwrap_or_default().0 != 0
            })
        })
        .collect()
}

/// What the server answers a successful drop with, applied to a stream **in ACE's own order**.
///
/// `HandleActionDropItem`'s success arm is three messages, and two of them reach this crate:
///
/// 1. `GameMessagePublicUpdateInstanceID(item, Container, ObjectGuid.Invalid)` — a quality
///    update, and not this seam's;
/// 2. `GameEventItemServerSaysMoveItem(Session, item)` clears the item's container and wielder
///    in the **game-object** half;
/// 3. `GameMessageUpdatePosition(item)` — `0xF748`, which is the **physical** half:
///    received-position handling clears the physical parent and supplies a position of its own.
///
/// The position is the player's own cell and origin, with identity orientation, grounded and
/// resting flags and no velocity; the timestamps are the item's own with `POSITION_TS` advanced.
fn drop_into_the_world(s: &mut ObjectStream, st: &Station, now: LocalTime) {
    let mut sink = dereth_client_model::RecordingSink::default();
    s.world
        .server_says_move_item(st.item, ObjectId(0), 0, ObjectId(0), 0, false, &mut sink);
    let p = s
        .presence(st.item)
        .expect("the item is in the stream")
        .clone();
    let m = MovementPositionEvent {
        id: st.item,
        position: PositionPack {
            // `TryDropItem` leaves the item resting on the ground and sends no velocity.
            flags: position_flags::IS_GROUNDED | position_flags::HAS_PLACEMENT_ID,
            origin: Origin {
                objcell_id: st.cell,
                origin: WireVec3 {
                    x: st.origin.x,
                    y: st.origin.y,
                    z: st.origin.z,
                },
            },
            orientation: WireQuat {
                w: 1.0,
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            velocity: None,
            // `item.Placement = ACE.Entity.Enum.Placement.Resting` — `TryDropItem`'s second line.
            placement_id: Some(PLACEMENT_RESTING),
            instance_timestamp: p.instance,
            // The position-event gate applies and stores only a strictly newer timestamp.
            position_timestamp: p.position_ts.wrapping_add(1),
            teleport_timestamp: p.teleport_ts,
            force_position_timestamp: p.force_position_ts,
        },
    };
    let body = write_body(&m).expect("the position event encodes");
    s.apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::MOVEMENT_POSITION_EVENT,
            body,
        },
        now,
    );
}

/// A scene over the capture's own landblock, with **no local body**: the objects the server owns
/// are the subject and a chase camera would move on its own.
///
/// `object_viewcone` is off because the camera is parked without being aimed, and the cull
/// correctly refuses objects behind the view. The measurement is whether a drawable exists at
/// all, not whether it is in frame.
fn scene_for(st: &Station) -> SceneConfig {
    SceneConfig {
        landblock: st.landblock,
        character: false,
        land_radius: 1,
        scenery_radius: 0,
        part_degrade_levels: true,
        part_billboards: true,
        part_depth_sort: true,
        part_alpha_lists: true,
        object_viewcone: false,
        ..SceneConfig::default()
    }
}

fn one_frame(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scene: &mut WorldScene,
    s: &mut ObjectStream,
    t: f64,
) {
    scene.sync_objects(store, gpu, s).expect("sync_objects");
    scene.update(
        dereth_client::camera::CameraInput::default(),
        CharacterInput::default(),
        LocalTime(t),
        1.0 / 30.0,
    );
    scene.stream(store, gpu).expect("stream");
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
}

/// One part of the draw list, reduced to what two arms are compared on.
type DrawKey = Vec<(Option<ObjectId>, usize, usize)>;

/// One arm: replay, optionally drop, park the camera over the **player** — who is drawn in every
/// arm, so the view is identical in all of them — and read the draw list.
///
/// Returns `(parts of the item, parts of everything else, the whole list as a comparable key)`.
fn arm(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    st: &Station,
    drop: bool,
) -> (usize, usize, DrawKey) {
    let mut s = replay(st.stop_at);
    let mut scene = WorldScene::load(store, gpu, scene_for(st)).expect("a scene");
    scene.set_weather_enabled(false);
    // The streaming window re-centres when the camera moves, which re-derives every
    // object frame against a new origin. Try up to six move/step/re-measure iterations, stopping
    // early when the displacement is below 0.5 m; this loop does not assert convergence. It uses
    // the **player's** frame, which is present and identical in every arm.
    let mut last = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    for i in 0..6 {
        one_frame(store, gpu, &mut scene, &mut s, 0.1 * f64::from(i));
        let here = scene
            .server_object_frame(st.player)
            .unwrap_or_else(|| panic!("the recorded player is not drawn at frame {i}"))
            .origin;
        let moved =
            ((here.x - last.x).powi(2) + (here.y - last.y).powi(2) + (here.z - last.z).powi(2))
                .sqrt();
        last = here;
        if moved < 0.5 {
            break;
        }
        scene.camera.position = Vec3::new(here.x, here.y, here.z + 20.0);
    }
    // **The drop happens here, after the settle.** The settle above is what
    // drains `take_created` for the entire replay -- every create the recording made, the pack
    // item among them -- exactly as a running client drains them as they arrive. The item is
    // skipped there for want of a position, so by this line the scene has been built and has no
    // drawable for it, which is precisely the state a real drop lands in. Dropping *before* the
    // first `sync_objects` would hand the create loop an item that already has a position and
    // would measure nothing at all.
    if drop {
        drop_into_the_world(&mut s, st, LocalTime(1_000.0));
    }
    // One more frame at the settled view, and that is the frame every number below comes from.
    one_frame(store, gpu, &mut scene, &mut s, 1.0);
    let order = scene.drawn_part_order();
    let key: DrawKey = order.iter().map(|d| (d.object, d.part, d.subset)).collect();
    let mine = order.iter().filter(|d| d.object == Some(st.item)).count();
    (mine, order.len() - mine, key)
}

// =============================================================================================
// The drop is drawn
// =============================================================================================

/// Behaviour: inventory.world-drop.a-dropped-item-is-drawn-in-the-world
///
/// Every created object gets its drawable from its setup, even one inside a container; received
/// position handling only clears the parent and places it in a cell. So the `0xF748` that takes a
/// pack item out of its container is what must offer it to the renderer: after it, the item's
/// parts are in the draw list and nothing else's changed, against a zero noise floor.
#[test]
fn an_item_dropped_out_of_the_pack_is_drawn_in_the_world() {
    let store = store();
    let mut gpu = warp();
    let st = station();
    println!(
        "{SESSION} datagram {} -- landblock 0x{:04X}, player {:#010X}, \
         {} pack item(s) with a setup record and no drawable, dropping {:#010X} at cell {:#010X} \
         ({:.2}, {:.2}, {:.2})",
        st.stop_at,
        st.landblock,
        st.player.0,
        st.pack.len(),
        st.item.0,
        st.cell,
        st.origin.x,
        st.origin.y,
        st.origin.z,
    );

    // ---- Arm A: the control. The item stays in the pack.
    let (a_item, a_other, a_key) = arm(&store, &mut gpu, &st, false);
    // ---- Arm A': the noise floor, driven independently and differing in nothing at all.
    let (a2_item, a2_other, a2_key) = arm(&store, &mut gpu, &st, false);
    // ---- Arm B: the drop.
    let (b_item, b_other, b_key) = arm(&store, &mut gpu, &st, true);

    let floor = a_key
        .iter()
        .zip(a2_key.iter())
        .filter(|(x, y)| x != y)
        .count()
        + a_key.len().abs_diff(a2_key.len());
    let moved = b_key.len().abs_diff(a_key.len());
    println!(
        "control {a_item} part(s) of the item, {a_other} of everything else; \
         control again {a2_item} and {a2_other}; drop {b_item} and {b_other}. \
         noise floor {floor} part(s); the drop moved {moved}."
    );

    // 1. The instrument can see a part at all. Without this every zero below is a silence.
    assert!(
        a_other > 0,
        "the scene submitted nothing: this frame proves nothing either way"
    );
    // 2. **The noise floor, first, and it must be zero.** Two independently driven runs of the
    //    identical arm must produce the identical draw list, or nothing measured between A and B
    //    is attributable to the drop.
    assert_eq!(
        floor,
        0,
        "two identical arms produced different draw lists ({} vs {} part(s)): this scene moves \
         on its own and the differential below is void",
        a_key.len(),
        a2_key.len()
    );
    // 3. The control really is the defect's precondition: an item in a pack is not drawn. That is
    //    correct in retail too -- it is in no cell object list -- and it is what makes the arm a
    //    control rather than a second copy of the subject.
    assert_eq!(
        a_item, 0,
        "the item is in the pack and must not be drawn before it is dropped"
    );
    // 4. After the one message a world drop consists of, the item is drawn.
    assert!(
        b_item > 0,
        "the dropped item contributed {b_item} part(s) to the draw list. It is in the model -- \
         `dereth_client_model::World` has it, and `ObjectStream` has its position and no parent, which is \
         what `the_dropped_item_was_in_the_model_the_whole_time` below asserts -- and no part of \
         it has ever reached the device, because nothing offered it to the renderer when `0xF748` \
         took it out of the container."
    );
    // 5. And the drop added **only** the item. The rest of the scene is untouched, which is what
    //    separates "the drop is drawn" from "the object pass changed".
    assert_eq!(b_other, a_other, "no other object lost or gained a part");
}

/// The model half, asserted separately: the pack item has a presence and no position before the
/// drop, and after it the player's cell, no parent, the 3d-view position state and no parent edge
/// in object maintenance. This half holds even when the renderer never learns of the drop, which
/// is why the drop test measures the draw list instead.
#[test]
fn the_dropped_item_was_in_the_model_the_whole_time() {
    let st = station();
    let mut s = replay(st.stop_at);
    assert!(
        s.presence(st.item).is_some(),
        "the pack item has a Presence before the drop"
    );
    assert!(
        s.presence(st.item).and_then(|p| p.position).is_none(),
        "and no position, which is what makes it a pack item"
    );
    drop_into_the_world(&mut s, &st, LocalTime(1_000.0));
    let p = s.presence(st.item).expect("still there");
    assert_eq!(
        p.position.map(|q| q.cell.0),
        Some(st.cell),
        "the drop gave it the player's cell"
    );
    assert!(p.parent.is_none(), "the parent was cleared");
    assert_eq!(
        s.world.weenie(st.item).map(|w| w.current_state),
        Some(dereth_client_model::weenie::PositionState::In3dView),
        "position-state classification agrees it is in the 3d view"
    );
    assert!(
        s.world.physics(st.item).is_some_and(|q| q.parent.is_none()),
        "and object maintenance holds no parent edge for it either"
    );
    // The **body** and the loaded cell belong to `ObjectStream::sync_physics_at`, which needs a
    // `PhysicsWorld` and a landscape; the drop test above drives the whole scene and is where
    // that half is exercised.
}
