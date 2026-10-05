use super::*;

// -------------------------------------------------------------------------------------------
// camera.view.slides-through-a-creature-standing-between-you-and-the-camera
// -------------------------------------------------------------------------------------------

/// The camera passes through a creature and is pulled in by the same body that is not one.
///
/// The candidate is a body the recorded corpus carries, re-created here with only its description
/// type changed -- so the creature and the solid control are geometrically identical and the type
/// is the only variable.
pub fn the_camera_slides_through_a_creature_but_not_through_a_solid_twin() {
    use dereth_client_net::client_session::testing::{Corpus, Direction};
    use dereth_client_net::client_session::SessionEvent;
    use dereth_client_runtime::camera::CameraInput;
    use dereth_physics::math::V3 as _;
    use dereth_primitives::{LocalTime, ObjectId, Position, Quat, Vec3};
    use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
    use dereth_protocol::types::physicsdesc::flags;
    use dereth_protocol::types::{PhysicsDesc, PositionWire};
    use dereth_protocol::{Message, Opcode};

    const CANDIDATE: ObjectId = ObjectId(0x8000_09E1);
    /// The recorded creature this scenario borrows a body from.
    const RECORDED: u32 = 0x8000_09D2;
    /// Anything without the creature bit; this one is the shipped "misc" type.
    const NOT_A_CREATURE: u32 = 0x0000_0080;

    /// The body this scenario measures with: no settle and no restrictions -- the camera path is
    /// measured from a body that has just been created, and settling it would move the path under
    /// the obstacle.
    fn plain_body(
        store: &std::sync::Arc<dereth_dat::RetailDatStore>,
    ) -> dereth_client_runtime::character::Character {
        let region =
            dereth_client_runtime::landblock::load_region(store).expect("the region decodes");
        dereth_client_runtime::character::Character::new(
            store,
            &region,
            dereth_client_runtime::landblock::DEFAULT_LANDBLOCK,
            (96.0, 96.0),
        )
        .expect("the ordinary local body is created")
    }

    /// The recorded create for the remote creature, as a payload.
    fn recorded_creature() -> ObjectCreatePayload {
        let corpus = Corpus::load("long-solo-play")
            .expect("the locked corpus decodes")
            .expect("the corpus carries that recording");
        let row = corpus
            .blobs
            .iter()
            .find(|r| {
                r.dir == Direction::ServerToClient
                    && r.opcode == Opcode::ITEM_CREATE_OBJECT.0
                    && r.payload.get(4..8) == Some(&RECORDED.to_le_bytes())
            })
            .expect("that recording carries the remote creature's create");
        ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
            .expect("the create decodes")
            .0
    }

    /// The free camera path a body with nothing around it has: from the pivot to the sought eye.
    fn free_camera_path(
        store: &std::sync::Arc<dereth_dat::RetailDatStore>,
    ) -> (Position, Position) {
        let mut character = plain_body(store);
        character.update_camera(CameraInput::default(), LocalTime(10.0), 1.0 / 30.0);
        let pivot = dereth_client_runtime::camera::pivot_state(&character.world, character.handle)
            .expect("the player has a body");
        let from = character.camera.manager.query_pivot_position(&pivot);
        let to = character.camera.sought;
        assert_eq!(
            from.cell, to.cell,
            "the focused camera path must stay in one outdoor cell"
        );
        assert!(
            to.frame.origin.sub(from.frame.origin).mag2().sqrt() > 1.0,
            "the default camera path is too short to hold a discriminating obstacle"
        );
        assert_eq!(
            character.camera.stats.sweeps_blocked, 0,
            "the open-air control was blocked"
        );
        (from, to)
    }

    /// Build the candidate's create, with `obj_type` the only thing that varies.
    fn create_blob(at: Position, obj_type: u32) -> Vec<u8> {
        let recorded = recorded_creature();
        let setup_id = recorded
            .physicsdesc
            .setup_id
            .expect("the recording names a body setup");
        let mut candidate = recorded;
        candidate.id = CANDIDATE;
        candidate.wdesc.obj_type = obj_type;
        candidate.physicsdesc = PhysicsDesc {
            bitfield: flags::POSITION | flags::SETUP,
            setup_id: Some(setup_id),
            position: Some(PositionWire {
                objcell_id: at.cell.raw(),
                frame: dereth_protocol::types::Frame {
                    origin: at.frame.origin.into(),
                    orientation: at.frame.rotation.into(),
                },
            }),
            state: dereth_physics::PhysicsState::REPORT_COLLISIONS_PS,
            ..Default::default()
        };
        let encoded =
            dereth_protocol::write_body(&ItemCreateObject(candidate)).expect("the create encodes");
        let decoded = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&encoded))
            .expect("the encoded create round trips")
            .0;
        assert_eq!(
            decoded.wdesc.obj_type, obj_type,
            "the description type was not encoded"
        );
        encoded
    }

    /// The top of the candidate's own collision geometry, read back off a placed body rather than
    /// decoded here: the point of the scenario is the body the client builds, not a second copy.
    fn upper_sphere_center(
        store: &std::sync::Arc<dereth_dat::RetailDatStore>,
        near: Position,
    ) -> Vec3 {
        let mut character = plain_body(store);
        character.land().load_block_cells(near.cell.landblock());
        let mut at = near;
        at.frame.origin.x += 10.0;
        at.frame.rotation = Quat::IDENTITY;
        let mut stream = dereth_client_runtime::objects::ObjectStream::new();
        stream.apply_event(
            &SessionEvent::WorldObject {
                opcode: ItemCreateObject::OPCODE,
                body: create_blob(at, CREATURE),
            },
            LocalTime(9.0),
        );
        stream.sync_physics(store, &mut character.world);
        let h = character
            .world
            .by_object_id(CANDIDATE)
            .expect("the candidate has a body");
        let o = character.world.get(h).expect("the candidate's body");
        o.geometry
            .spheres
            .iter()
            .max_by(|a, b| a.center.z.total_cmp(&b.center.z))
            .expect("the candidate's setup has a collision sphere")
            .center
    }

    /// `(sweeps blocked, how far short of the sought eye the camera stopped, it ended in a cell)`.
    fn camera_with_candidate(
        store: &std::sync::Arc<dereth_dat::RetailDatStore>,
        upper: Vec3,
        obj_type: u32,
    ) -> (u64, f32, bool) {
        let (from, to) = free_camera_path(store);
        // Put the upper body sphere exactly at the midpoint of the real pivot-to-eye path.
        let midpoint = from.frame.origin.add(to.frame.origin).mul(0.5);
        let mut at = from;
        at.frame.origin = midpoint.sub(upper);
        at.frame.rotation = Quat::IDENTITY;

        let mut stream = dereth_client_runtime::objects::ObjectStream::new();
        stream.apply_event(
            &SessionEvent::WorldObject {
                opcode: ItemCreateObject::OPCODE,
                body: create_blob(at, obj_type),
            },
            LocalTime(9.0),
        );
        assert_eq!(
            stream
                .world
                .weenie(CANDIDATE)
                .expect("the create made a weenie")
                .is_creature(),
            obj_type == CREATURE,
            "the accepted description type did not reach the game model"
        );

        let mut character = plain_body(store);
        character.land().load_block_cells(from.cell.landblock());
        stream.sync_physics(store, &mut character.world);
        let h = character
            .world
            .by_object_id(CANDIDATE)
            .expect("the candidate has a body");
        let body = character.world.get(h).expect("the candidate's body");
        assert!(
            body.cell.is_some(),
            "the candidate was not inserted into the cell graph"
        );
        assert!(
            !body.geometry.spheres.is_empty(),
            "the candidate has no collision geometry"
        );
        assert_eq!(
            body.weenie
                .as_ref()
                .expect("the candidate's weenie half")
                .is_creature,
            obj_type == CREATURE,
            "the candidate's description did not reach the collision body"
        );

        character.update_camera(CameraInput::default(), LocalTime(10.0), 1.0 / 30.0);
        let shortfall = character
            .camera
            .sought
            .frame
            .origin
            .sub(character.camera.viewer.frame.origin);
        (
            character.camera.stats.sweeps_blocked,
            shortfall.mag2().sqrt(),
            character.camera.viewer_cell.is_some(),
        )
    }

    let store = store();
    let (from, _) = free_camera_path(&store);
    let upper = upper_sphere_center(&store, from);
    let creature = camera_with_candidate(&store, upper, CREATURE);
    let solid = camera_with_candidate(&store, upper, NOT_A_CREATURE);
    println!("camera: creature {creature:?} vs the same body that is not one {solid:?}");

    let both_ended_somewhere = solid.2 && creature.2;
    let solid_obstructs = solid.0 > 0 && solid.1 > dereth_physics::globals::VIEWER_SPHERE_RADIUS;
    let creature_does_not =
        creature.0 == 0 && creature.1 <= dereth_physics::globals::VIEWER_SPHERE_RADIUS;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "camera.view.slides-through-a-creature-standing-between-you-and-the-camera",
        move |_| both_ended_somewhere && solid_obstructs && creature_does_not,
    );
}

// -------------------------------------------------------------------------------------------
// camera.wall.*
// -------------------------------------------------------------------------------------------

/// The camera backed into a wall settles in the same place however often the client ticks.
///
/// The measurement is the instrument's: the same wall-clock trajectory driven at two rates, with
/// the body's position a pure function of time in both, so the only difference between the arms
/// is the step. Over every wall approach the training academy offers, not one.
pub fn the_camera_against_a_wall_settles_the_same_at_any_tick_rate() {
    use dereth_physics::math::V3 as _;
    use dereth_primitives::{CellId, LandblockId};

    /// The client's own early-out distance: a camera that varies by less than this over a whole
    /// second is, by the client's own definition, not moving.
    const SETTLE_DISTANCE: f32 = 0.000_4;
    /// How far behind the pivot an unobstructed camera sits.
    const FREE_CAMERA_DISTANCE: f32 = 2.610_077;
    const RESIDUAL_WINDOW: f64 = 1.0;

    let store = std::sync::Arc::new(dereth_dat::testing::open_store_or_fail());
    let region = dereth_client_runtime::landblock::load_region(&store).expect("the region decodes");
    let src = std::sync::Arc::new(
        dereth_world_data::land_source::DatLandSource::new(std::sync::Arc::clone(&store), &region)
            .expect("the retail height table"),
    );
    src.load_block_cells(LandblockId(world_support::TRAINING_DUNGEON));

    let mut loader = dereth_world_data::env_cells::EnvCellLoader::new();
    let ids: Vec<CellId> = loader
        .load_block(&store, world_support::TRAINING_DUNGEON)
        .iter()
        .map(|d| d.id)
        .collect();
    assert!(!ids.is_empty(), "the training academy has interior cells");

    let walls = world_support::find_walls(&src, &ids, 8);
    // Four, deliberately: a single approach that happens to settle proves nothing about the
    // solver, and the shape this measures did not appear at every heading.
    let enough = walls.len() >= 4;

    let mut worst_slow = 0.0f32;
    let mut worst_fast = 0.0f32;
    let mut worst_separation = 0.0f32;
    let mut pressed = 0usize;
    let mut standing_off = 0usize;
    for (wall, slow) in &walls {
        let fast = world_support::run_camera(&src, wall, 1.0 / 144.0);
        let slow_pull = slow.max_pivot_distance(RESIDUAL_WINDOW);
        let fast_pull = fast.max_pivot_distance(RESIDUAL_WINDOW);
        // Calibration, before anything is concluded from a zero: the wall has to do the work.
        if slow_pull < FREE_CAMERA_DISTANCE * 0.5 && fast_pull < FREE_CAMERA_DISTANCE * 0.5 {
            pressed += 1;
        }
        // A camera collapsed onto the body is a legitimate state and a useless measurement: it
        // cannot move, so it cannot be unsteady.
        if slow_pull.max(fast_pull) > 0.2 {
            standing_off += 1;
        }
        worst_slow = worst_slow.max(slow.residual(RESIDUAL_WINDOW));
        worst_fast = worst_fast.max(fast.residual(RESIDUAL_WINDOW));
        worst_separation = worst_separation.max(slow.last().sub(fast.last()).mag2().sqrt());
    }
    println!(
        "camera wall: {} wall approaches, {pressed} pressed into geometry, {standing_off} holding \
         the camera clear -- slow {worst_slow:.6} m, fast {worst_fast:.6} m, separation \
         {worst_separation:.6} m",
        walls.len()
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "camera.wall.settles-in-the-same-place-however-often-the-client-ticks",
        move |_| {
            enough
                && pressed == walls.len()
                && standing_off >= 4
                && worst_slow < SETTLE_DISTANCE
                && worst_fast < SETTLE_DISTANCE
                && worst_separation < 0.01
        },
    );
}
