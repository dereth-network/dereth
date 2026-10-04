//! An object the server marks `NODRAW_PS` contributes no submitted parts, and a `HIDDEN_PS`
//! transition on a holder suppresses its direct children's parts and restores them on unhide.
//!
//! Drawing does not test `HIDDEN_PS` on the object's own parts. `NODRAW_PS` (`0x20`) sets the
//! part-array no-draw state, which sets each part's draw-state bit 0, and that bit suppresses
//! drawing. A hidden holder therefore stays drawn while its hide transition suppresses its direct
//! children's parts.
//!
//! Fixture: public captures replayed through transport, session and `ObjectStream`, measured by
//! `WorldScene::drawn_part_order` over retail dat geometry; nothing is hand-built and no datagram
//! leaves the process. Each case runs both arms in one process with independent streams from the
//! same recording: unrelated parts are submitted, the affected parts appear with the consumer
//! disabled, and disappear with it enabled. These are submission counts, not pixel comparisons.
//! Missing fixtures and GPU creation failures are hard failures; there is no skip.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use super::common::{addr, connection_sequence_number, load, retail_store, test_gpu};
use dereth_client::world::{SceneReads, SceneWrites};

use std::collections::BTreeSet;
use std::sync::Arc;

use dereth_client::character::CharacterInput;
use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_client_net::client_session::SessionEvent;
use dereth_dat::RetailDatStore;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_render::device::Gpu;

/// Independent literal no-draw mask, kept separate from the crate's state symbol.
const NODRAW_PS: u32 = 0x0000_0020;

/// The session whose `NODRAW_PS` objects this bench can actually see drawn.
///
/// This no-body outdoor bench draws no interior cells: only reached interior cells enter the cell
/// draw list, and it reaches none (as `WorldScene::drawn_cells()` reports). So a marked object is
/// scored only if it is outdoors. Across the recordings, at the datagram with the most
/// `NODRAW_PS` presences:
///
/// ```text
/// session                       outdoor  indoor
/// first-login-walk-jump               0       0
/// early-inventory-and-casting         1       0
/// short-second-connection             0       0
/// login-account-booted                0       0
/// ddd-interrogation-only              0       0
/// long-solo-play                      0       4
/// short-play-with-training            0       0
/// house-purchase-refused              0       1
/// house-purchase-and-trade           14      59
/// ```
///
/// long-solo-play carries 19 of the 20 no-draw-setting `Item_SetState 0xF74B` messages, but its
/// player is in a dungeon when they arrive and all 98 scene objects at its peak are indoors.
/// house-purchase-and-trade alone has marked outdoor objects in the player's own landblock; the
/// bench scores that subset rather than all marked presences.
const SESSION: &str = "house-purchase-and-trade";

// ---------------------------------------------------------------------------------------------
// The replay: the capture through the client's network and object stream.
// ---------------------------------------------------------------------------------------------

struct Replayed {
    landblock: u16,
    /// Objects whose state word carries `NODRAW_PS` at the datagram this stopped at, read from
    /// `dereth_client_model::objects::PhysicsPresence::state` through
    /// `ObjectStream::physics_state`, the one place the word lives.
    marked: BTreeSet<ObjectId>,
    /// The datagram to stop at, so each arm can build its own stream: [`ObjectStream`] is not
    /// `Clone` and handing the same one to two scenes would let the second see an empty
    /// `take_created`.
    stop_at: usize,
}

/// Is a presence at a position this bench's frame would submit at all?
///
/// This no-body frame traverses no interior cells. It draws outdoor cells of resident blocks;
/// the scoring predicate deliberately selects only the player's own landblock, a subset of
/// the radius-one window. Being eligible here is not proof of successful part submission.
fn drawable_here(position: Option<dereth_primitives::Position>, block: Option<u16>) -> bool {
    let (Some(pos), Some(block)) = (position, block) else {
        return false;
    };
    if !dereth_physics::landdefs::is_outdoors(pos.cell) {
        return false;
    }
    let b = pos.cell.landblock();
    (u16::from(b.x()) << 8) | u16::from(b.y()) == block
}

/// Replay [`SESSION`] and stop at the datagram at which the **most** objects that this frame can
/// submit are simultaneously carrying `NODRAW_PS`.
///
/// Measure at the transient population peak: the recording ends with logout and an empty object
/// model. The marked objects are short-lived spell projectiles (state `0x00128374`: MISSILE,
/// CLOAKED, NODRAW, ETHEREAL, IGNORE_COLLISIONS, ALIGNPATH, PATHCLIPPED, SCRIPTED_COLLISION and
/// INELASTIC), destroyed within a second or two, so the last populated datagram misses them.
/// Strictly larger scores select the first tied peak; a second replay stops at that index and
/// independently checks the drawable-marked count.
fn most_marked() -> Replayed {
    let records = load(SESSION);
    let seq = connection_sequence_number(&records);
    let run = |limit: usize| -> (ObjectStream, usize, usize, Option<u16>) {
        let mut net =
            ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", seq).expect("host");
        let mut objects = ObjectStream::with_store(retail_store());
        let mut entered = false;
        let (mut best, mut best_at) = (0usize, 0usize);
        let mut block = None;
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
            // **The player's block *now*, not the first one he ever stood in.**
            // The scene below is built over this block at `land_radius: 1`, and the station is
            // chosen by what is drawable in it, so the two have to be the same datagram.
            if let Some(p) = objects.player().and_then(|id| objects.presence(id)) {
                if let Some(pos) = p.position {
                    let b = pos.cell.landblock();
                    block = Some((u16::from(b.x()) << 8) | u16::from(b.y()));
                }
            }
            // Score marked outdoor objects in the player's current landblock. Interior objects
            // cannot enter this no-body frame's traversal, and a population elsewhere would
            // not calibrate the selected window. Submission itself is checked after drawing.
            let marked = objects
                .presences()
                .filter(|(id, _)| objects.physics_state(*id).unwrap_or(0) & NODRAW_PS != 0)
                .filter(|(_, p)| drawable_here(p.position, block))
                .count();
            if marked > best {
                best = marked;
                best_at = index;
            }
        }
        (objects, best, best_at, block)
    };
    let (_, peak, at, _) = run(usize::MAX);
    assert!(
        peak > 0,
        "{SESSION} never carries a NODRAW_PS presence this window can draw: no 0xF74B, or every \
         marked object is in a cell the portal view's cell draw list would not hold"
    );
    let (objects, _, _, block) = run(at + 1);
    let marked: BTreeSet<ObjectId> = objects
        .presences()
        .filter(|(id, _)| objects.physics_state(*id).unwrap_or(0) & NODRAW_PS != 0)
        .map(|(id, _)| id)
        .collect();
    let drawable = objects
        .presences()
        .filter(|(id, _)| objects.physics_state(*id).unwrap_or(0) & NODRAW_PS != 0)
        .filter(|(_, p)| drawable_here(p.position, block))
        .count();
    assert_eq!(
        drawable, peak,
        "the second pass stopped where the first said to"
    );
    println!(
        "no-draw: {SESSION} datagram {at} of {} -- {} object(s) present, {} carrying NODRAW_PS \
         ({drawable} of them outdoors in landblock {:#06X})",
        records.len(),
        objects.presences().count(),
        marked.len(),
        block.unwrap_or(0),
    );
    Replayed {
        landblock: block.expect("the capture's player has a position"),
        marked,
        stop_at: at,
    }
}

/// The same replay again, for the second arm.
fn replay_again(stop_at: usize) -> ObjectStream {
    let records = load(SESSION);
    let seq = connection_sequence_number(&records);
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", seq).expect("host");
    let mut objects = ObjectStream::with_store(retail_store());
    let mut entered = false;
    for r in records.iter().take(stop_at + 1) {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, addr(r.pair), now);
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

/// A scene over the capture's own landblock, with **no local body**: the objects the server owns
/// are this module's subject and a chase camera would move on its own.
fn scene_for(r: &Replayed, on: bool) -> SceneConfig {
    SceneConfig {
        landblock: r.landblock,
        character: false,
        land_radius: 1,
        scenery_radius: 0,
        // The part-drawing extensions stay on in both arms.
        part_degrade_levels: true,
        part_billboards: true,
        part_depth_sort: true,
        part_alpha_lists: true,
        // The bench parks above the objects without aiming yaw/pitch, so many objects are below
        // or behind the camera. View-cone rejection is disabled explicitly to isolate the no-draw
        // consumer; the cone predicate still runs and `WorldScene::drawn_object_cone` exposes its
        // answers, but this test does not assert that trace. The explicit opt-out holds whatever
        // the default is.
        object_viewcone: false,
        // **The one thing that differs between the arms.**
        object_state_draw: on,
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

/// Parts submitted **for the marked objects** and **for everything else**, this frame.
fn submitted(scene: &WorldScene, marked: &BTreeSet<ObjectId>) -> (usize, usize) {
    let order = scene.drawn_part_order();
    let mine = order
        .iter()
        .filter(|d| d.object.is_some_and(|o| marked.contains(&o)))
        .count();
    (mine, order.len() - mine)
}

/// Behaviour: objects.nodraw.a-server-marked-object-submits-no-parts
///
/// **The acceptance.**
#[test]
fn an_object_the_server_marks_nodraw_is_not_submitted() {
    let store = retail_store();
    let mut gpu = test_gpu(800, 600);
    let r = most_marked();

    // The two arms differ only in `object_state_draw`, and each gets its own copy of the replayed
    // stream so that neither can see the other's `take_created`.
    let mut off_stream = replay_again(r.stop_at);
    let mut off = WorldScene::load(&store, &mut gpu, scene_for(&r, false)).expect("a scene");
    one_frame(&store, &mut gpu, &mut off, &mut off_stream, 0.0);
    let (off_marked, off_other) = submitted(&off, &r.marked);

    let mut on_stream = replay_again(r.stop_at);
    let mut on = WorldScene::load(&store, &mut gpu, scene_for(&r, true)).expect("a scene");
    one_frame(&store, &mut gpu, &mut on, &mut on_stream, 0.0);
    let (on_marked, on_other) = submitted(&on, &r.marked);

    println!(
        "no-draw: consumer off -- {off_marked} part(s) of the marked object(s), {off_other} of \
         everything else; consumer on -- {on_marked} and {on_other}. \
         stats: nodraw_set={} nodraw_cleared={} hidden_changed={}",
        on.draw.stats.object_nodraw_set,
        on.draw.stats.object_nodraw_cleared,
        on.draw.stats.object_hidden_changed,
    );

    // 1. The instrument can see a part at all. Without this the zero below is a silence.
    assert!(
        off_other > 0,
        "the scene submitted nothing: this frame proves nothing either way"
    );
    // 2. With the consumer off, the marked objects are drawn: the consumer is what removes them.
    assert!(
        off_marked > 0,
        "the marked object(s) contributed no parts even with the consumer off, so switching it \
         on cannot be what removes them -- this differential is void"
    );
    // 3. With the consumer on, none of their parts is drawn.
    assert_eq!(
        on_marked, 0,
        "with the consumer on, not one part of a NODRAW_PS object is drawn"
    );
    // 4. The unrelated-part COUNT is unchanged. This guards against globally suppressing
    //    the pass; it does not compare the identities of every unrelated submission.
    assert_eq!(on_other, off_other, "no other object lost or gained a part");
    // 5. One no-draw-setting event per marked object; none is a clearing transition.
    assert_eq!(
        usize::try_from(on.draw.stats.object_nodraw_set).expect("a count"),
        r.marked.len(),
        "the no-draw consumer ran once per marked object"
    );
    assert_eq!(
        on.draw.stats.object_hidden_changed, 0,
        "no HIDDEN_PS object in this frame"
    );
}

// =============================================================================================
// Hidden-state transitions suppress direct children, then restore them on unhide.
// =============================================================================================

/// Independent literal hidden-state mask.
const HIDDEN_PS: u32 = 0x0000_4000;

/// The session in which a **hidden** object is holding something at the same moment.
///
/// All four hidden creates and all six hidden-setting `Item_SetState 0xF74B` messages in the
/// recordings concern player objects carrying the teleport-hide word. Only two sessions overlap a
/// hidden holder with an already-created child: short-second-connection at datagram 40
/// (`0x50000003` holding one item) and long-solo-play at 1654 (`0x5000000A` holding one item). The
/// others hide the player before a held item exists. short-second-connection is the shorter one.
const HELD_SESSION: &str = "short-second-connection";

struct HiddenHolder {
    landblock: u16,
    holder: ObjectId,
    /// The holder's direct children present while it is hidden.
    children: BTreeSet<ObjectId>,
    /// The datagram at which the holder is hidden and holding the most.
    while_hidden: usize,
    /// The datagram at which it stops being hidden while still holding them.
    after_unhide: usize,
}

/// Find the two stations this test needs in one pass: the datagram at which a **hidden** holder
/// has the most children present, and the later one at which it becomes **unhidden** while still
/// holding them.
///
/// Hidden-state effects run on bit transitions, not continuously while the bit is set. A single
/// replay and sync collapses the intervening transitions into one state and cannot test
/// restoration, so the same scene is synced at the hidden and then the unhidden station, in
/// order. The later station must still contain the measured direct children.
fn hidden_holder_with_children() -> HiddenHolder {
    let session = HELD_SESSION;
    let records = load(session);
    let seq = connection_sequence_number(&records);
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", seq).expect("host");
    let mut objects = ObjectStream::with_store(retail_store());
    let mut entered = false;
    let mut best: Option<(usize, ObjectId, BTreeSet<ObjectId>)> = None;
    let mut unhide: Option<usize> = None;
    let mut block = None;
    let mut previously_hidden: BTreeSet<ObjectId> = BTreeSet::new();
    for (index, r) in records.iter().enumerate() {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, addr(r.pair), now);
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
        let hidden: BTreeSet<ObjectId> = objects
            .presences()
            .filter(|(id, _)| objects.physics_state(*id).unwrap_or(0) & HIDDEN_PS != 0)
            .map(|(id, _)| id)
            .collect();
        let kids_of = |h: ObjectId| -> BTreeSet<ObjectId> {
            objects
                .presences()
                .filter(|(_, p)| p.parent.is_some_and(|(pid, _)| pid == h))
                .map(|(id, _)| id)
                .collect()
        };
        for h in &hidden {
            let kids = kids_of(*h);
            if !kids.is_empty() && best.as_ref().is_none_or(|(_, _, b)| kids.len() > b.len()) {
                best = Some((index, *h, kids));
            }
        }
        // The unhide: a holder that was hidden on the previous datagram and is not now, while
        // the children being measured are still attached to it.
        if unhide.is_none() {
            if let Some((_, holder, kids)) = &best {
                if previously_hidden.contains(holder)
                    && !hidden.contains(holder)
                    && kids_of(*holder).is_superset(kids)
                {
                    unhide = Some(index);
                }
            }
        }
        previously_hidden = hidden;
        if block.is_none() {
            if let Some(p) = objects.player().and_then(|id| objects.presence(id)) {
                if let Some(pos) = p.position {
                    let b = pos.cell.landblock();
                    block = Some((u16::from(b.x()) << 8) | u16::from(b.y()));
                }
            }
        }
    }
    let (while_hidden, holder, children) =
        best.unwrap_or_else(|| panic!("{session} never carries a hidden object holding anything"));
    let after_unhide = unhide.unwrap_or_else(|| {
        panic!("{session} never unhides {holder:?} while it is still holding those children")
    });
    println!(
        "hidden holder: {session} -- {holder:?} is HIDDEN_PS and holds {} at datagram {while_hidden}, \
         and is unhidden while still holding them at datagram {after_unhide}",
        children.len()
    );
    HiddenHolder {
        landblock: block.expect("the capture's player has a position"),
        holder,
        children,
        while_hidden,
        after_unhide,
    }
}

/// What one station saw: parts of the children, of the holder, and of everything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Station {
    children: usize,
    holder: usize,
    other: usize,
}

/// Replay `HELD_SESSION` on **one** scene and one stream, stopping to draw a frame at each of the
/// two stations, so that the scene sees `HIDDEN_PS` go up and then come down.
fn two_stations(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    h: &HiddenHolder,
    on: bool,
) -> (Station, Station, u64, u64, u64) {
    let records = load(HELD_SESSION);
    let seq = connection_sequence_number(&records);
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", seq).expect("host");
    let mut objects = ObjectStream::with_store(Arc::clone(store));
    let mut entered = false;
    let mut scene = WorldScene::load(
        store,
        gpu,
        SceneConfig {
            landblock: h.landblock,
            character: false,
            land_radius: 1,
            scenery_radius: 0,
            part_degrade_levels: true,
            part_billboards: true,
            part_depth_sort: true,
            part_alpha_lists: true,
            // Same explicit un-aimed-camera opt-out as `scene_for`: isolate state-driven child
            // submission, not cone rejection. `WorldScene::drawn_object_cone` still exposes the
            // evaluated predicate, but this test does not assert its trace.
            object_viewcone: false,
            object_state_draw: on,
            ..SceneConfig::default()
        },
    )
    .expect("a scene");
    let mut seen: Vec<Station> = Vec::new();
    for (index, r) in records.iter().enumerate() {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, addr(r.pair), now);
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
        if index == h.while_hidden || index == h.after_unhide {
            // LINT-OK: a station index in a capture of a few hundred datagrams.
            #[allow(clippy::cast_precision_loss)]
            let t = index as f64 * 0.1;
            one_frame(store, gpu, &mut scene, &mut objects, t);
            let order = scene.drawn_part_order();
            let children = order
                .iter()
                .filter(|d| d.object.is_some_and(|o| h.children.contains(&o)))
                .count();
            let holder = order.iter().filter(|d| d.object == Some(h.holder)).count();
            seen.push(Station {
                children,
                holder,
                other: order.len() - children - holder,
            });
        }
        if index >= h.after_unhide {
            break;
        }
    }
    assert_eq!(seen.len(), 2, "both stations were drawn");
    (
        seen[0],
        seen[1],
        scene.draw.stats.object_nodraw_set,
        scene.draw.stats.object_nodraw_cleared,
        scene.draw.stats.object_hidden_changed,
    )
}

/// Hiding a holder suppresses its measured direct children; unhiding restores them.
///
/// The client visits direct children only, not a whole tree, and the holder's own `HIDDEN_PS`
/// bit does not suppress its parts. Below, the holder keeps a positive part count while the
/// children disappear and later return; unrelated counts hold.
///
/// This recording hides the player's object, but `character: false` routes it as a remote scene
/// object, so this covers only the remote-object path, not the local body's.
#[test]
fn a_hidden_holder_takes_its_children_with_it_and_gives_them_back() {
    let store = retail_store();
    let mut gpu = test_gpu(800, 600);
    let h = hidden_holder_with_children();

    let (off_hidden, off_after, ..) = two_stations(&store, &mut gpu, &h, false);
    let (on_hidden, on_after, set, cleared, changed) = two_stations(&store, &mut gpu, &h, true);

    println!(
        "hidden holder -- off: {off_hidden:?} then {off_after:?}; \
         on: {on_hidden:?} then {on_after:?}. \
         stats: nodraw_set={set} nodraw_cleared={cleared} hidden_changed={changed}"
    );

    // 1. The instrument can see parts at all, at both stations.
    assert!(
        off_hidden.other > 0 && off_after.other > 0,
        "the scene submitted nothing"
    );
    // 2. Without the consumer, the held object is drawn on a hidden holder.
    assert!(
        off_hidden.children > 0,
        "the child contributed no parts with the consumer off: void"
    );
    // 3. While hidden, the children go.
    assert_eq!(
        on_hidden.children, 0,
        "the holder's hidden transition suppresses every measured direct child"
    );
    // 4. The holder stays: HIDDEN_PS alone does not suppress its own part submissions.
    //    Equal counts are required below, with a separate positive assertion.
    assert_eq!(
        on_hidden.holder, off_hidden.holder,
        "a HIDDEN_PS holder keeps the same submitted-part count"
    );
    assert!(
        on_hidden.holder > 0,
        "and it really was drawn, not absent for some other reason"
    );
    // 5. Nothing else moved, at either station.
    assert_eq!(
        on_hidden.other, off_hidden.other,
        "no unrelated object moved while hidden"
    );
    assert_eq!(on_after.other, off_after.other, "and none after the unhide");
    // 6. The unhide clears each direct child's forced no-draw state. Check restoration
    //    against the disabled consumer and require a positive count, not two equal zeros.
    assert_eq!(
        on_after.children, off_after.children,
        "after the unhide the child draws again, exactly as it did with the consumer off"
    );
    assert!(
        on_after.children > 0,
        "and it is a real number of parts, not zero on both sides"
    );
    // 7. One hide and one unhide; at least one child no-draw setting and clearing event.
    assert_eq!(
        changed, 2,
        "the hidden-state consumer observed the hide and the unhide"
    );
    assert!(set >= 1, "NODRAW_PS was forced on at least once");
    assert!(cleared >= 1, "and taken off again");
}
