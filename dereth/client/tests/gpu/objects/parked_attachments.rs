//! A child attached to a holder before the client knows the child (a parked physical attachment)
//! keeps its link through refresh deadlines, a late create, failed re-parents, cell releases and
//! re-admission, expires on its own timer, and becomes real held geometry once it arrives. A
//! failed parent operation keeps the old link; a later valid one wins; a holder's deletion detaches
//! without a new deadline.
//!
//! Fixture: the retail dats (setups and an academy interior cell) with explicitly constructed
//! packet and time ordering; no capture timing is claimed. The one App case needs a GPU device.
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::objects::ObjectStream;
use dereth_primitives::{LocalTime, ObjectId, ServerTime};
use dereth_protocol::{
    objects::{ItemCreateObject, ItemDeleteObject, ItemParentEvent, ObjectCreatePayload},
    types::{
        physicsdesc::{flags, ChildLink},
        PhysicsDesc,
    },
    Message,
};
use std::sync::Arc;

const HOLDER: ObjectId = ObjectId(0x7000_0011);
const OTHER: ObjectId = ObjectId(0x7000_0012);
const CHILD: ObjectId = ObjectId(0x8000_0011);
const SETUP: u32 = 0x0200_0001;

fn stream() -> ObjectStream {
    ObjectStream::with_store(Arc::new(dereth_dat::testing::open_store().unwrap()))
}
fn payload(id: ObjectId, setup: u32, child: Option<(ObjectId, u32)>) -> ObjectCreatePayload {
    let mut physicsdesc = PhysicsDesc {
        bitfield: flags::SETUP,
        setup_id: Some(setup),
        ..Default::default()
    };
    if let Some((child_id, location_id)) = child {
        physicsdesc.bitfield |= flags::CHILDREN;
        physicsdesc.children = Some(vec![ChildLink {
            child_id,
            location_id,
        }]);
    }
    ObjectCreatePayload {
        id,
        physicsdesc,
        objdesc: Default::default(),
        wdesc: Default::default(),
    }
}
fn create(s: &mut ObjectStream, p: ObjectCreatePayload, now: f64) {
    s.apply_event(
        &SessionEvent::WorldObject {
            opcode: ItemCreateObject::OPCODE,
            body: dereth_protocol::write_body(&ItemCreateObject(p)).unwrap(),
        },
        LocalTime(now),
    );
}
fn tick(s: &mut ObjectStream, now: f64) {
    s.use_time::<dereth_client_net::client_session::testing::MockTransport>(ServerTime(now), None);
}

fn parent_event(s: &mut ObjectStream, parent: ObjectId, location: u32, stamp: u16, now: f64) {
    s.apply_event(
        &SessionEvent::WorldObject {
            opcode: ItemParentEvent::OPCODE,
            body: dereth_protocol::write_body(&ItemParentEvent {
                creature: parent,
                item: CHILD,
                location,
                placement_frame: 1,
                timestamps: dereth_protocol::types::PhysicsEventStamp {
                    instance: 0,
                    event: stamp,
                },
            })
            .unwrap(),
        },
        LocalTime(now),
    );
}

/// Behaviour: objects.parent.a-failed-link-keeps-the-old-parent
#[test]
fn actual_parent_and_pickup_acceptance_preserve_failed_link_and_cancel_only_after_success() {
    let mut s = stream();
    for id in [HOLDER, OTHER, CHILD] {
        create(&mut s, payload(id, SETUP, None), 1000.0);
    }
    parent_event(&mut s, HOLDER, 1, 1, 1001.0);
    assert_eq!(s.presence(CHILD).unwrap().parent, Some((HOLDER, 1)));
    s.world.schedule_destroy(CHILD, ServerTime(1002.0));
    parent_event(&mut s, OTHER, u32::MAX, 2, 1003.0);
    assert_eq!(s.presence(CHILD).unwrap().parent, Some((HOLDER, 1)));
    assert_eq!(
        s.presence(CHILD).unwrap().position_ts,
        2,
        "the accepted event stamp is stored even if attaching fails"
    );
    assert_eq!(
        s.presence(CHILD).unwrap().placement,
        1,
        "parent-event handling continues to placement on failure"
    );
    assert_eq!(s.world.tables.doomed.get(CHILD), Some(&ServerTime(1027.0)));
    parent_event(&mut s, OTHER, 1, 2, 1004.0);
    assert_eq!(
        s.presence(CHILD).unwrap().parent,
        Some((HOLDER, 1)),
        "duplicate refused before attachment"
    );
    parent_event(&mut s, OTHER, 1, 3, 1005.0);
    assert_eq!(s.presence(CHILD).unwrap().parent, Some((OTHER, 1)));
    assert_eq!(s.world.tables.doomed.get(CHILD), None);
    let pickup = dereth_protocol::objects::InventoryPickupEvent {
        id: CHILD,
        timestamps: dereth_protocol::types::PhysicsEventStamp {
            instance: 0,
            event: 4,
        },
    };
    s.apply_event(
        &SessionEvent::WorldObject {
            opcode: dereth_protocol::objects::InventoryPickupEvent::OPCODE,
            body: dereth_protocol::write_body(&pickup).unwrap(),
        },
        LocalTime(1006.0),
    );
    assert_eq!(s.world.physics_parent(CHILD), None);
    assert_eq!(s.presence(CHILD).unwrap().parent, None);
    parent_event(&mut s, HOLDER, 1, 5, 1007.0);
    assert_eq!(
        s.presence(CHILD).unwrap().parent,
        Some((HOLDER, 1)),
        "next operation remains live"
    );
}

#[test]
fn actual_session_parked_state_replays_once_on_late_physical_null_promotion() {
    use dereth_primitives::NetQueue;
    let mut s = stream();
    let mut session = dereth_client_net::client_session::Session::new(
        dereth_client_net::client_session::testing::MockTransport::new(),
    );
    session.transport.deliver_blob(
        NetQueue::WorldObjects,
        &dereth_protocol::write_blob(&ItemCreateObject(payload(HOLDER, SETUP, Some((CHILD, 1)))))
            .unwrap(),
    );
    session.transport.deliver_blob(
        NetQueue::WorldObjects,
        &dereth_protocol::write_blob(&dereth_protocol::objects::ItemSetState {
            id: CHILD,
            state: 0x4000,
            timestamps: dereth_protocol::types::PhysicsEventStamp {
                instance: 0,
                event: 7,
            },
        })
        .unwrap(),
    );
    session.tick(LocalTime(1000.0));
    s.pump_session(&mut session, LocalTime(1000.0));
    assert!(!session.instances_mut().knows(CHILD));
    assert_eq!(s.world.physics_parent(CHILD), Some((HOLDER, 1)));
    tick(&mut s, 1026.0);
    session.transport.deliver_blob(
        NetQueue::WorldObjects,
        &dereth_protocol::write_blob(&ItemCreateObject(payload(CHILD, SETUP, None))).unwrap(),
    );
    session.tick(LocalTime(1027.0));
    s.pump_session(&mut session, LocalTime(1027.0));
    assert!(session.instances_mut().knows(CHILD));
    assert_eq!(s.physics_state(CHILD), Some(0x4000));
    assert_eq!(s.presence(CHILD).unwrap().parent, Some((HOLDER, 1)));
    let count = s.stats.state_events;
    s.pump_session(&mut session, LocalTime(1028.0));
    assert_eq!(
        s.stats.state_events, count,
        "no double replay after promotion"
    );
    assert_eq!(count, 1);
}

#[test]
fn failed_init_destroys_only_physics_parked_owner_and_later_create_does_not_replay_it() {
    use dereth_primitives::NetQueue;
    use dereth_protocol::{
        events::pack_event,
        objects::{ItemServerSaysRemove, ItemSetState},
    };
    let mut s = stream();
    let mut session = dereth_client_net::client_session::Session::new(
        dereth_client_net::client_session::testing::MockTransport::new(),
    );
    // Give the independent Weenie stamper a discriminating value without publishing physics.
    assert!(session.stamper(CHILD).update(7, 20));
    session.transport.deliver_blob(
        NetQueue::WorldObjects,
        &dereth_protocol::write_blob(&ItemCreateObject(payload(HOLDER, SETUP, Some((CHILD, 1)))))
            .unwrap(),
    );
    session.transport.deliver_blob(
        NetQueue::WorldObjects,
        &dereth_protocol::write_blob(&ItemSetState {
            id: CHILD,
            state: 0x4000,
            timestamps: dereth_protocol::types::PhysicsEventStamp {
                instance: 0,
                event: 7,
            },
        })
        .unwrap(),
    );
    session.tick(LocalTime(1000.0));
    s.pump_session(&mut session, LocalTime(1000.0));
    // A UI owner or window is distinct from the physical parked list. This direct Session
    // adapter seeds its ordered gap; the phased App's Weenie-only publication is a separate
    // integration gate, not covered by this compatibility API test.
    for stamp in [1, 3] {
        session.transport.deliver_blob(
            NetQueue::UiQueue,
            &pack_event(CHILD, stamp, &ItemServerSaysRemove { object: CHILD }).unwrap(),
        );
    }
    session.transport.deliver_blob(
        NetQueue::WorldObjects,
        &dereth_protocol::write_blob(&ItemCreateObject(payload(CHILD, 0, None))).unwrap(),
    );
    session.tick(LocalTime(1001.0));
    s.pump_session(&mut session, LocalTime(1001.0));
    assert!(s.world.weenie(CHILD).is_some());
    assert!(!session.instances_mut().knows(CHILD));
    assert_eq!(
        session.stamper(CHILD).get(7),
        Some(20),
        "not broad Weenie deletion"
    );
    // Physical-only cleanup must preserve queued UI bytes and per-object ordering. Publish
    // this compatibility Session's eventual real arrival directly, before World replaces its
    // Weenie-only generation (which independently has full deletion semantics).
    session.object_arrived(CHILD, 0);
    let events: Vec<_> = session.drain_events().collect();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SessionEvent::WorldObject { .. })),
        "old physical state was destroyed"
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SessionEvent::UiEvent { .. }))
            .count(),
        1
    );
    session.transport.deliver_blob(
        NetQueue::UiQueue,
        &pack_event(CHILD, 2, &ItemServerSaysRemove { object: CHILD }).unwrap(),
    );
    session.tick(LocalTime(1002.0));
    assert_eq!(
        session
            .drain_events()
            .filter(|e| matches!(e, SessionEvent::UiEvent { .. }))
            .count(),
        2,
        "the preserved gap releases 2 and 3, not only the fresh 2"
    );
    session.transport.deliver_blob(
        NetQueue::WorldObjects,
        &dereth_protocol::write_blob(&ItemCreateObject(payload(CHILD, SETUP, None))).unwrap(),
    );
    session.tick(LocalTime(1003.0));
    s.pump_session(&mut session, LocalTime(1003.0));
    assert!(s.presence(CHILD).is_some());
    assert_eq!(
        s.physics_state(CHILD),
        Some(0),
        "failed null's state cannot replay on next object"
    );
    assert_eq!(s.stats.state_events, 0);
}

/// Behaviour: objects.parent.a-holder-holds-only-at-a-place-its-own-body-has
#[test]
fn real_dat_unknown_child_attachment_survives_refresh_deadline_and_late_parentless_init() {
    let mut s = stream();
    create(&mut s, payload(HOLDER, SETUP, Some((CHILD, 1))), 1000.0);
    assert_eq!(s.world.physics_parent(CHILD), Some((HOLDER, 1)));
    assert!(
        s.presence(CHILD).is_none(),
        "physical null is not a render description"
    );
    assert_eq!(s.world.tables.doomed.get(CHILD), None);
    assert_eq!(
        s.world.tables.null_physics.get(CHILD).unwrap().update_time,
        ServerTime(1000.0)
    );
    s.world.get_null_physics_object(CHILD, ServerTime(1019.0));
    assert_eq!(
        s.world.tables.null_physics.get(CHILD).unwrap().update_time,
        ServerTime(1000.0),
        "lookup is not refresh"
    );
    tick(&mut s, 1020.0);
    assert_eq!(
        s.world.tables.null_physics.get(CHILD).unwrap().update_time,
        ServerTime(1000.0),
        "the 20 s boundary is strict"
    );
    tick(&mut s, 1020.001);
    assert_eq!(
        s.world.tables.null_physics.get(CHILD).unwrap().update_time,
        ServerTime(1020.001)
    );
    tick(&mut s, 1025.001);
    assert_eq!(s.world.physics_parent(CHILD), Some((HOLDER, 1)));
    create(&mut s, payload(CHILD, SETUP, None), 1026.0);
    assert!(s.world.tables.null_physics.get(CHILD).is_none());
    assert!(s.world.weenie(CHILD).unwrap().has_phys_obj);
    assert_eq!(s.presence(CHILD).unwrap().parent, Some((HOLDER, 1)));
    // A duplicate no-parent descriptor is NOT an ungated pickup; a fresh one is.
    create(&mut s, payload(CHILD, SETUP, None), 1027.0);
    assert_eq!(s.presence(CHILD).unwrap().parent, Some((HOLDER, 1)));
    let mut pickup = payload(CHILD, SETUP, None);
    pickup.physicsdesc.timestamps.position = 1;
    create(&mut s, pickup, 1028.0);
    assert_eq!(s.world.physics_parent(CHILD), None);
    assert_eq!(s.presence(CHILD).unwrap().parent, None);
}

/// Behaviour: objects.parent.a-failed-link-keeps-the-old-parent
#[test]
fn failed_add_child_preserves_old_link_then_valid_reparent_rescues_only_that_null() {
    let mut s = stream();
    create(&mut s, payload(HOLDER, SETUP, Some((CHILD, 1))), 1000.0);
    create(
        &mut s,
        payload(OTHER, SETUP, Some((CHILD, u32::MAX))),
        1001.0,
    );
    assert_eq!(
        s.world.physics_parent(CHILD),
        Some((HOLDER, 1)),
        "failed add precedes old unlink"
    );
    assert!(!s
        .world
        .set_physics_parent(CHILD, CHILD, 1, ServerTime(1002.0)));
    assert!(!s
        .world
        .set_physics_parent(CHILD, ObjectId(999), 1, ServerTime(1002.0)));
    assert_eq!(s.world.physics_parent(CHILD), Some((HOLDER, 1)));
    // Explicit source timer station: parent leaving visibility schedules held/null children.
    s.world.schedule_destroy(CHILD, ServerTime(1003.0));
    let untouched = ObjectId(0x8000_0022);
    s.world
        .get_null_physics_object(untouched, ServerTime(1003.0));
    assert!(!s
        .world
        .set_physics_parent(CHILD, OTHER, u32::MAX, ServerTime(1004.0)));
    assert_eq!(s.world.tables.doomed.get(CHILD), Some(&ServerTime(1028.0)));
    assert!(s
        .world
        .set_physics_parent(CHILD, OTHER, 1, ServerTime(1005.0)));
    assert_eq!(
        s.world.tables.null_physics.get(CHILD).unwrap().update_time,
        ServerTime(1005.0)
    );
    assert_eq!(s.world.tables.doomed.get(CHILD), None);
    assert_eq!(
        s.world.tables.doomed.get(untouched),
        Some(&ServerTime(1028.0))
    );
    tick(&mut s, 1028.001);
    assert!(s.world.tables.null_physics.contains_key(CHILD));
    assert!(!s.world.tables.null_physics.contains_key(untouched));
}

#[test]
fn source_cell_transitions_enter_only_initialized_children_and_do_not_reenter_late_init() {
    use dereth_primitives::CellId;
    let mut s = stream();
    create(&mut s, payload(HOLDER, SETUP, Some((CHILD, 1))), 1000.0);
    // Explicit model-only physical-owner facts, not a claim that these cells are loaded by App.
    let a = Some(CellId(0xA9B4_0001));
    let b = Some(CellId(0xA9B4_0002));
    s.world.publish_physics_cell(HOLDER, a);
    assert!(s.world.tables.null_physics.contains_key(CHILD));
    assert!(
        s.world.physics(CHILD).is_none(),
        "entering a cell is a no-op without a part array"
    );
    create(&mut s, payload(CHILD, SETUP, None), 1001.0);
    assert_eq!(s.world.physics(CHILD).unwrap().cell, None);
    s.world.publish_physics_cell(HOLDER, a);
    assert_eq!(
        s.world.physics(CHILD).unwrap().cell,
        None,
        "an unchanged parent does not re-enter the cell"
    );
    s.world.publish_physics_cell(HOLDER, b);
    assert_eq!(
        s.world.physics(CHILD).unwrap().cell,
        b,
        "real parent transition now enters initialized child"
    );
    assert!(s.world.weenie(CHILD).unwrap().phys_has_cell);
    s.world.publish_physics_cell(HOLDER, None);
    assert_eq!(s.world.physics(CHILD).unwrap().cell, None);
    assert!(!s.world.weenie(CHILD).unwrap().phys_has_cell);
    s.world.publish_physics_cell(HOLDER, a);
    assert_eq!(s.world.physics(CHILD).unwrap().cell, a);
    parent_event(&mut s, HOLDER, 1, 1, 1002.0);
    assert_eq!(
        s.world.physics(CHILD).unwrap().cell,
        a,
        "the explicit initialized re-parent restores the loaded cell"
    );
}

#[test]
fn real_indoor_release_cannot_republish_stale_body_cell_and_same_position_reenters() {
    use dereth_physics::{EnvCellGeometry, LandSource, LandblockCollision, PhysicsWorld};
    use dereth_primitives::{CellId, LandblockId, Vec3};
    use dereth_protocol::types::PositionWire;
    use dereth_world_data::land_source::DatLandSource;
    use std::sync::atomic::{AtomicBool, Ordering};
    // Explicit synthetic cell-lifetime failure over the real dat source. One visibility lookup
    // succeeds, but releases that cell registry before placement's next lookup. This tests the
    // missing-cell tail, not collision rejection or capture timing.
    struct ReleaseAfterVisible {
        land: Arc<DatLandSource>,
        armed: AtomicBool,
    }
    impl LandSource for ReleaseAfterVisible {
        fn landblock(&self, id: LandblockId) -> Option<Arc<LandblockCollision>> {
            self.land.landblock(id)
        }
        fn height_table(&self) -> &[f32; dereth_physics::globals::LAND_HEIGHT_TABLE_LEN] {
            self.land.height_table()
        }
        fn env_cell(&self, cell: CellId) -> Option<Arc<EnvCellGeometry>> {
            let visible = self.land.env_cell(cell);
            if visible.is_some() && self.armed.swap(false, Ordering::SeqCst) {
                self.land.release_visible_cells(cell.landblock());
            }
            visible
        }
    }
    let store = Arc::new(dereth_dat::testing::open_store().unwrap());
    let region = dereth_world_data::landblock::load_region(&store).unwrap();
    let land = Arc::new(DatLandSource::new(Arc::clone(&store), &region).unwrap());
    let block = LandblockId(0x7F03);
    let cell = CellId(0x7F03_0100);
    land.load_block_cells(block);
    let geometry = land.env_cell(cell).expect("installed academy interior");
    let mut low = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut high = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for vertex in geometry.physics_polygons.iter().flat_map(|p| &p.vertices) {
        low = Vec3::new(
            low.x.min(vertex.x),
            low.y.min(vertex.y),
            low.z.min(vertex.z),
        );
        high = Vec3::new(
            high.x.max(vertex.x),
            high.y.max(vertex.y),
            high.z.max(vertex.z),
        );
    }
    assert!(high.x > low.x && high.y > low.y && high.z > low.z);
    let center = Vec3::new((low.x + high.x) * 0.5, (low.y + high.y) * 0.5, low.z + 1.0);
    let initial_origin = dereth_physics::math::localtoglobal(&geometry.frame, center);
    let source = Arc::new(ReleaseAfterVisible {
        land: Arc::clone(&land),
        armed: AtomicBool::new(false),
    });
    let mut physics = PhysicsWorld::new(Arc::clone(&source) as Arc<dyn LandSource>);
    let mut s = ObjectStream::with_store(Arc::clone(&store));
    let mut parent = payload(HOLDER, SETUP, None);
    parent.physicsdesc.bitfield |= flags::POSITION;
    parent.physicsdesc.position = Some(PositionWire {
        objcell_id: cell.0,
        frame: dereth_protocol::types::Frame {
            origin: initial_origin.into(),
            orientation: geometry.frame.rotation.into(),
        },
    });
    create(&mut s, parent.clone(), 1000.0);
    create(&mut s, payload(CHILD, SETUP, None), 1000.0);
    s.sync_physics_at(&store, &mut physics, LocalTime(1000.0));
    parent_event(&mut s, HOLDER, 1, 1, 1001.0);
    assert_eq!(s.world.physics(CHILD).unwrap().cell, Some(cell));
    let handle = s.physics.handle(HOLDER).unwrap();
    assert_eq!(physics.get(handle).unwrap().cell, Some(cell));
    assert!(
        !physics.get(handle).unwrap().shadow_objects.is_empty(),
        "actual initial shadow membership"
    );
    let wire = s.presence(HOLDER).unwrap().position.unwrap();
    {
        let body = physics.get_mut(handle).unwrap();
        body.set_active(false, 1001.0);
        body.set_velocity(Vec3::new(1.0, 0.0, 0.0), 1001.0);
    }
    assert!(physics.use_time(LocalTime(1001.1), false));
    let placed = Some(physics.get(handle).unwrap().position);
    assert_eq!(physics.get(handle).unwrap().cell, Some(cell));
    assert_ne!(
        placed,
        Some(wire),
        "actual physics moved before release without a fresh packet"
    );
    assert_eq!(s.presence(HOLDER).unwrap().position, Some(wire));
    let left = s.release_block_obj_cells_with_physics(block, LocalTime(1002.0), &mut physics);
    assert_eq!(
        left, 1,
        "held child is not an independent cell-release candidate"
    );
    assert_eq!(s.world.tables.doomed.get(HOLDER), Some(&ServerTime(1027.0)));
    assert_eq!(s.world.tables.doomed.get(CHILD), Some(&ServerTime(1027.0)));
    assert!(land.release_visible_cells(block).0 > 0);
    assert!(land.env_cell(cell).is_none());
    s.sync_physics_at(&store, &mut physics, LocalTime(1003.0));
    assert_eq!(
        s.world.physics(HOLDER).unwrap().cell,
        None,
        "publisher must not resurrect the released cell from a surviving remote body"
    );
    assert_eq!(s.world.physics(CHILD).unwrap().cell, None);
    assert!(!s.world.weenie(HOLDER).unwrap().phys_has_cell);
    assert!(!s.world.weenie(CHILD).unwrap().phys_has_cell);
    assert_eq!(
        s.physics.handle(HOLDER),
        Some(handle),
        "release is not object destruction"
    );
    assert_eq!(physics.get(handle).unwrap().cell, None);
    assert!(physics.get(handle).unwrap().shadow_objects.is_empty());
    assert_eq!(
        s.presence(HOLDER).unwrap().position,
        placed,
        "storing the position keeps the current physical pose, not the old wire pose"
    );
    assert_eq!(s.world.tables.doomed.get(HOLDER), Some(&ServerTime(1027.0)));
    s.sync_physics_at(&store, &mut physics, LocalTime(1004.0));
    assert_eq!(
        s.world.physics(HOLDER).unwrap().cell,
        None,
        "repeated unloaded sync cannot restore it"
    );
    land.load_block_cells(block);
    assert!(land.env_cell(cell).is_some());
    s.sync_physics_at(&store, &mut physics, LocalTime(1005.0));
    assert_eq!(s.physics.handle(HOLDER), Some(handle));
    // **Re-admission is a placement, so it consults the floor; `placed` never did.**
    //
    // `placed` is the pose the body reached by *integration*: the station set `active = false`
    // and gave it `velocity = (1, 0, 0)`, and one 0.1 s `use_time` slid it 0.1 m along +x at a
    // constant z. An inactive body runs no contact/gravity half, so it slid across the floor
    // rather than down it. Re-entry runs full position placement, including the floor check.
    // Moving or teleporting a body with no current cell takes this same world-entry path.
    //
    // Measured on this cell, re-placing the same body at a sweep of x along y = -230:
    //
    // ```text
    //   x = 150.000  ->  z = -11.883217      (the pose `placed` holds)
    //   x = 150.025  ->  z = -11.893214      (-0.009997)
    //   x = 150.050  ->  z = -11.903218      (-0.020001)
    //   x = 150.100  ->  z = -11.915229      (-0.032012)
    //   x = 150.200  ->  z = -11.915229      (flat past the edge)
    //   x = 151.000  ->  z = -11.915229
    // ```
    //
    // A 0.4-gradient ramp polygon that ends about 0.08 m along: the 0.032012 m the body drops
    // here is that ramp's whole height, and the 0.1 m of velocity carried it off the top. So a
    // floor-following placement cannot equal a pose that never touched the floor; the check is
    // the two facts that hold: x, y, cell and rotation are kept, and z settles by the ramp height.
    let body = physics.get(handle).unwrap().position;
    let want = placed.unwrap();
    assert_eq!(
        s.presence(HOLDER).unwrap().position,
        Some(body),
        "the presence is the body's own position, republished by publish_physics_cells"
    );
    assert_eq!(
        (
            body.cell,
            body.frame.origin.x,
            body.frame.origin.y,
            body.frame.rotation
        ),
        (
            want.cell,
            want.frame.origin.x,
            want.frame.origin.y,
            want.frame.rotation
        ),
        "the re-admission placement moved the body in z only"
    );
    let settled = want.frame.origin.z - body.frame.origin.z;
    assert!((0.031..=0.033).contains(&settled),
        "re-entry must settle the body DOWN onto the ramp it slid off, by that ramp's own height of 0.032012 m; it moved {settled} m. A zero here means the re-admission stopped consulting the floor.");
    assert_eq!(physics.get(handle).unwrap().cell, Some(cell));
    assert_eq!(
        s.world.physics(HOLDER).unwrap().cell,
        Some(cell),
        "unchanged position must retry real cell admission"
    );
    assert_eq!(s.world.physics(CHILD).unwrap().cell, Some(cell));
    assert!(s.world.weenie(HOLDER).unwrap().phys_has_cell);
    assert!(s.world.weenie(CHILD).unwrap().phys_has_cell);
    assert_eq!(physics.get(handle).unwrap().update_time, 1005.0);
    assert!(physics.get(handle).unwrap().transient_state.is_active());
    assert_eq!(s.world.tables.doomed.get(HOLDER), None);
    assert_eq!(s.world.tables.doomed.get(CHILD), None);
    assert!(s.world.tables.lost_cells.get(cell).is_none());
    let moves = s.physics.stats.moved;
    s.sync_physics_at(&store, &mut physics, LocalTime(1006.0));
    s.sync_physics(&store, &mut physics); // documented adapter is the same consumer, not a bypass
    assert_eq!(
        physics.get(handle).unwrap().update_time,
        1005.0,
        "loaded list drains exactly once"
    );
    assert_eq!(
        s.physics.stats.moved, moves,
        "ordinary identical placement retains its memo"
    );
    // No periodic visible-refresh rescue is allowed to mask stale destruction ownership.
    s.world.tables.visible.clear();
    tick(&mut s, 1027.001);
    assert!(s.world.physics(HOLDER).is_some());
    assert!(s.world.physics(CHILD).is_some());
    parent_event(&mut s, HOLDER, 1, 2, 1028.0);
    assert_eq!(
        s.presence(CHILD).unwrap().parent,
        Some((HOLDER, 1)),
        "next parent operation remains valid"
    );

    assert_eq!(
        s.release_block_obj_cells_with_physics(block, LocalTime(1030.0), &mut physics),
        1
    );
    land.release_visible_cells(block);
    land.load_block_cells(block);
    source.armed.store(true, Ordering::SeqCst);
    s.sync_physics_at(&store, &mut physics, LocalTime(1031.0));
    assert!(
        !source.armed.load(Ordering::SeqCst),
        "actual reentry resolver was reached"
    );
    assert_eq!(
        physics.get(handle).unwrap().cell,
        None,
        "placement really failed after admission"
    );
    assert!(!physics.get(handle).unwrap().transient_state.is_active());
    assert_eq!(s.world.physics(HOLDER).unwrap().cell, None);
    assert_eq!(
        s.world.tables.doomed.get(HOLDER),
        Some(&ServerTime(1056.0)),
        "missing-cell tail reschedules after preparation"
    );
    assert_eq!(s.world.tables.doomed.get(CHILD), Some(&ServerTime(1056.0)));
    assert_eq!(
        s.world.tables.lost_cells.get(cell).unwrap().objects,
        vec![HOLDER]
    );
    s.sync_physics_at(&store, &mut physics, LocalTime(1032.0));
    assert_eq!(
        s.world.tables.doomed.get(HOLDER),
        Some(&ServerTime(1056.0)),
        "unloaded retry does not refresh deadline"
    );
    land.load_block_cells(block);
    s.sync_physics_at(&store, &mut physics, LocalTime(1033.0));
    assert_eq!(physics.get(handle).unwrap().cell, Some(cell));
    assert_eq!(s.world.tables.doomed.get(HOLDER), None);
    assert_eq!(s.world.tables.doomed.get(CHILD), None);
    s.world.tables.visible.clear();
    tick(&mut s, 1056.001);
    assert!(s.world.physics(HOLDER).is_some());
    assert!(s.world.physics(CHILD).is_some());
}

#[cfg(gpu)]
#[test]
fn app_late_child_becomes_real_held_geometry_then_drop_and_next_parent_operation_work() {
    use dereth_assets::Decode;
    use dereth_primitives::{DataId, Quat, Vec3};
    use dereth_protocol::types::{Frame as WireFrame, PositionWire};
    use {
        dereth_client::app::App, dereth_client_runtime::config::Config,
        dereth_client_runtime::scene::SceneConfig,
    };
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: false,
        preferences_file: std::env::temp_dir()
            .join("dere-parked-attachments-not-created/prefs.ini"),
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .unwrap();
    app.load_static_scene(SceneConfig {
        character: true,
        cell_statics: false,
        mesh_collision: false,
        land_radius: 1,
        scenery_radius: 0,
        particles: false,
        ..Default::default()
    })
    .unwrap();
    let scene = app.world_scene().unwrap();
    let block = scene.viewer_block().unwrap();
    let origin =
        dereth_physics::math::localtoglobal(&scene.camera.frame(), Vec3::new(0.0, 10.0, -1.0));
    let position = PositionWire {
        objcell_id: ((block.0 as u32) << 24) | ((block.1 as u32) << 16) | 1,
        frame: WireFrame {
            origin: origin.into(),
            orientation: Quat::IDENTITY.into(),
        },
    };
    let mut parent = payload(HOLDER, SETUP, Some((CHILD, 1)));
    parent.physicsdesc.bitfield |= flags::POSITION | flags::ANIMFRAME;
    parent.physicsdesc.position = Some(position);
    parent.physicsdesc.animframe_id = Some(101);
    create(app.probe_mut().objects_mut(), parent, 1000.0);
    assert!(app.frame());
    assert!(app
        .world_state()
        .unwrap()
        .server_object_frame(HOLDER)
        .is_some());
    assert!(app
        .world_state()
        .unwrap()
        .server_object_frame(CHILD)
        .is_none());
    tick(app.probe_mut().objects_mut(), 1025.001);
    let mut child = payload(CHILD, 0x0200_1713, None); // recorded weapon setup; synthetic delayed delivery.
    child.physicsdesc.bitfield |= flags::ANIMFRAME;
    child.physicsdesc.animframe_id = Some(1);
    create(app.probe_mut().objects_mut(), child, 1026.0);
    assert!(app.frame());
    let scene = app.world_scene().unwrap();
    let parent_frame = scene.server_object_frame(HOLDER).unwrap();
    let parent_parts = scene.server_object_part_frames(HOLDER).unwrap();
    let child_frame = scene
        .server_object_frame(CHILD)
        .expect("late initialized child reaches actual renderer");
    let store = dereth_dat::testing::open_store().unwrap();
    let did = DataId(SETUP);
    let setup = dereth_assets::Setup::decode_payload(
        did,
        &store.read_typed(dereth_dat::DbType::Setup, did).unwrap(),
    )
    .unwrap();
    let holding = setup.holding_locations[&1];
    let expected = dereth_client_runtime::models::child_frame(
        &parent_frame,
        &parent_parts,
        &dereth_animation::data::LocationEntry {
            part_id: holding.part_id,
            frame: holding.frame,
        },
    );
    assert_eq!(
        child_frame, expected,
        "the real part 15 holding frame, not an injected scene position"
    );
    assert!(!scene.server_object_part_frames(CHILD).unwrap().is_empty());
    assert!(app.objects().world.weenie(CHILD).unwrap().has_phys_obj);
    assert_eq!(
        app.objects().world.physics(CHILD).unwrap().cell,
        None,
        "initializing the bodyless object does not re-enter the unchanged parent's cell"
    );
    assert!(
        app.objects().physics.handle(CHILD).is_none(),
        "held object has no independent collision body"
    );
    let held_count = scene.server_object_count();
    assert!(app.frame());
    assert_eq!(app.world_state().unwrap().server_object_count(), held_count);
    // Accepted position unparents; normal sync must create the independent physical body.
    let event = dereth_protocol::movement::MovementPositionEvent {
        id: CHILD,
        position: dereth_protocol::movement::PositionPack {
            origin: dereth_protocol::types::Origin {
                objcell_id: position.objcell_id,
                origin: position.frame.origin,
            },
            position_timestamp: 1,
            ..Default::default()
        },
    };
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: dereth_protocol::movement::MovementPositionEvent::OPCODE,
            body: dereth_protocol::write_body(&event).unwrap(),
        },
        LocalTime(1027.0),
    );
    assert!(app.frame());
    assert_eq!(app.objects().presence(CHILD).unwrap().parent, None);
    assert!(
        app.objects().physics.handle(CHILD).is_some(),
        "drop reaches real physics synchronization"
    );
    let handle = app.objects().physics.handle(CHILD).unwrap();
    let loaded_cell = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .world
        .get(handle)
        .unwrap()
        .cell;
    assert!(
        loaded_cell.is_some(),
        "drop fixture has an actual loaded cell"
    );
    assert_eq!(
        app.objects().world.physics(CHILD).unwrap().cell,
        loaded_cell
    );
    assert!(app.objects().world.weenie(CHILD).unwrap().phys_has_cell);
    app.probe_mut()
        .objects_mut()
        .world
        .update_visible_object_list();
    assert!(
        app.objects().world.tables.visible.contains(&CHILD),
        "drop regains actual visible/selection eligibility"
    );
    parent_event(app.probe_mut().objects_mut(), HOLDER, 1, 2, 1028.0);
    assert!(app.frame());
    assert!(app.objects().physics.handle(CHILD).is_none());
    assert_eq!(
        app.objects().presence(CHILD).unwrap().parent,
        Some((HOLDER, 1))
    );
    let holder_cell = app.objects().world.physics(HOLDER).unwrap().cell;
    assert!(holder_cell.is_some());
    assert_eq!(
        app.objects().world.physics(CHILD).unwrap().cell,
        holder_cell,
        "an explicit re-parent of an initialized child changes its cell, unlike a physical null"
    );
    assert!(app.objects().world.weenie(CHILD).unwrap().phys_has_cell);
    assert!(app
        .world_state()
        .unwrap()
        .server_object_frame(CHILD)
        .is_some());

    // A nonzero network cell ID is not evidence of an available cell object. This supplied-input
    // object deliberately names an unloaded interior; no expected model state is injected.
    let absent = ObjectId(0x7000_0033);
    let mut unloaded = payload(absent, SETUP, None);
    unloaded.physicsdesc.bitfield |= flags::POSITION;
    unloaded.physicsdesc.position = Some(PositionWire {
        objcell_id: 0xFFFF_FFFF,
        ..position
    });
    create(app.probe_mut().objects_mut(), unloaded, 1029.0);
    assert_eq!(app.objects().world.physics(absent).unwrap().cell, None);
    assert!(app.frame());
    let absent_handle = app
        .objects()
        .physics
        .handle(absent)
        .expect("unplaced object still owns initialized physics");
    assert_eq!(
        app.world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .world
            .get(absent_handle)
            .unwrap()
            .cell,
        None
    );
    assert_eq!(app.objects().world.physics(absent).unwrap().cell, None);
    assert!(!app.objects().world.weenie(absent).unwrap().phys_has_cell);
    app.probe_mut()
        .objects_mut()
        .world
        .update_visible_object_list();
    assert!(!app.objects().world.tables.visible.contains(&absent));

    // Supplied accepted placement plus a physical velocity, then the real App update: publication
    // in the same frame AFTER a moving body's cell transition, not only sync before the step.
    let holder_handle = app.objects().physics.handle(HOLDER).unwrap();
    let physical = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .world
        .get(holder_handle)
        .unwrap();
    let mut near_boundary = physical.position;
    let direction = if near_boundary.frame.origin.x < 168.0 {
        1.0
    } else {
        -1.0
    };
    near_boundary.frame.origin.x = if direction > 0.0 {
        (near_boundary.frame.origin.x / 24.0).floor() * 24.0 + 23.98
    } else {
        (near_boundary.frame.origin.x / 24.0).floor() * 24.0 + 0.02
    };
    near_boundary.frame.origin.z += 5.0;
    let event = dereth_protocol::movement::MovementPositionEvent {
        id: HOLDER,
        position: dereth_protocol::movement::PositionPack {
            origin: dereth_protocol::types::Origin {
                objcell_id: near_boundary.cell.0,
                origin: near_boundary.frame.origin.into(),
            },
            position_timestamp: 1,
            ..Default::default()
        },
    };
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: dereth_protocol::movement::MovementPositionEvent::OPCODE,
            body: dereth_protocol::write_body(&event).unwrap(),
        },
        LocalTime(1030.0),
    );
    assert!(app.frame());
    let before = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .world
        .get(holder_handle)
        .unwrap()
        .cell;
    assert!(before.is_some());
    {
        let body = app
            .probe_mut()
            .world_state_mut()
            .unwrap()
            .character
            .as_mut()
            .unwrap()
            .world
            .get_mut(holder_handle)
            .unwrap();
        body.set_velocity(Vec3::new(direction * 20.0, 0.0, 0.0), body.update_time);
    }
    let mut changed = false;
    for _ in 0..30 {
        assert!(app.frame());
        let actual = app
            .world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .world
            .get(holder_handle)
            .unwrap()
            .cell;
        assert_eq!(
            app.objects().world.physics(HOLDER).unwrap().cell,
            actual,
            "post-step publisher must not lag the moving actual body by a frame"
        );
        if actual != before {
            assert!(
                actual.is_some(),
                "physical fixture crosses between two loaded cells"
            );
            assert_eq!(app.objects().world.physics(CHILD).unwrap().cell, actual);
            changed = true;
            break;
        }
    }
    assert!(
        changed,
        "actual velocity integration must cross the prepared cell boundary"
    );
}

/// Behaviour: objects.maintenance.every-deadline-is-strict-and-a-child-that-left-is-spared
#[test]
fn invalid_location_expires_but_parent_delete_detaches_without_invented_new_deadline() {
    let mut s = stream();
    create(
        &mut s,
        payload(HOLDER, SETUP, Some((CHILD, u32::MAX))),
        1000.0,
    );
    assert_eq!(s.world.physics_parent(CHILD), None);
    tick(&mut s, 1025.0);
    assert!(
        s.world.tables.null_physics.contains_key(CHILD),
        "the 25 s boundary is strict"
    );
    tick(&mut s, 1025.001);
    assert!(!s.world.tables.null_physics.contains_key(CHILD));
    let mut fresh = payload(HOLDER, SETUP, Some((CHILD, 1)));
    fresh.physicsdesc.timestamps.instance = 1;
    create(&mut s, fresh, 1030.0);
    assert_eq!(s.world.physics_parent(CHILD), Some((HOLDER, 1)));
    s.apply_event(
        &SessionEvent::WorldObject {
            opcode: ItemDeleteObject::OPCODE,
            body: dereth_protocol::write_body(&ItemDeleteObject {
                id: HOLDER,
                instance_sequence: 1,
            })
            .unwrap(),
        },
        LocalTime(1031.0),
    );
    assert_eq!(s.world.physics_parent(CHILD), None);
    assert_eq!(
        s.world.tables.null_physics.get(CHILD).unwrap().update_time,
        ServerTime(1031.0)
    );
    assert_eq!(
        s.world.tables.doomed.get(CHILD),
        None,
        "detaching the children does not queue destruction"
    );
    create(&mut s, payload(HOLDER, SETUP, None), 1032.0);
    create(&mut s, payload(CHILD, SETUP, None), 1033.0);
    assert_eq!(
        s.presence(CHILD).unwrap().parent,
        None,
        "new same-id holder cannot inherit an old pointer edge"
    );
}

#[test]
fn failed_init_drops_physical_null_but_keeps_weenie_and_assetless_never_grants_parenting() {
    let mut s = stream();
    create(&mut s, payload(HOLDER, SETUP, Some((CHILD, 1))), 1000.0);
    create(&mut s, payload(CHILD, 0, None), 1001.0);
    assert!(
        s.world.weenie(CHILD).is_some(),
        "independent gameplay-object creation partially succeeds"
    );
    assert!(!s.world.weenie(CHILD).unwrap().has_phys_obj);
    assert!(s.world.physics(CHILD).is_none());
    assert!(s.presence(CHILD).is_none());
    assert!(!s.world.tables.null_physics.contains_key(CHILD));
    assert_eq!(s.world.tables.doomed.get(CHILD), None);
    create(&mut s, payload(CHILD, SETUP, None), 1002.0);
    assert!(
        s.presence(CHILD).is_some(),
        "next valid same-instance create replaces the Weenie-only partial success"
    );
    assert_eq!(
        s.presence(CHILD).unwrap().parent,
        None,
        "deleted null attachment is not resurrected"
    );
    let mut unresolved = ObjectStream::new();
    create(
        &mut unresolved,
        payload(HOLDER, SETUP, Some((CHILD, 1))),
        1000.0,
    );
    assert_eq!(unresolved.world.physics_parent(CHILD), None);
    assert_eq!(
        unresolved.world.tables.doomed.get(CHILD),
        Some(&ServerTime(1025.0))
    );
    create(
        &mut s,
        payload(ObjectId(33), 0x0200_ffff, Some((ObjectId(34), 1))),
        1002.0,
    );
    assert!(
        s.presence(ObjectId(33)).is_none(),
        "missing setup cannot create a parent part array"
    );
    assert!(
        !s.world.tables.null_physics.contains_key(ObjectId(34)),
        "child linking is skipped without a physics object"
    );
}
