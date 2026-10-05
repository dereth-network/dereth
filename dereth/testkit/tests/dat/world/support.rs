use std::sync::Arc;

use dereth_client_net::client_session::SessionEvent;
use dereth_dat::RetailDatStore;
use dereth_physics::pmanager::FALLBACK_SPEED;
use dereth_primitives::{LocalTime, ObjectId, Position, Vec3};
use dereth_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::types::physicsdesc::flags;
use dereth_protocol::types::{PhysicsDesc, PositionWire, PublicWeenieDesc};
use dereth_protocol::Message;
use {
    dereth_client_runtime::character::Character,
    dereth_client_runtime::character::ALUVIAN_MALE_SETUP,
};
use {
    dereth_client_runtime::landblock::load_region,
    dereth_client_runtime::landblock::DEFAULT_LANDBLOCK,
};

/// The description type every body below carries. A player and a monster both have it.
pub const CREATURE: u32 = 0x0000_0010;
/// The description bit that says a body is a player.
pub const PLAYER: u32 = 0x0000_0008;
/// The bit that says a player is marked for player combat.
pub const PK: u32 = 0x0000_0020;
/// The lighter form of the same marking.
pub const PK_LITE: u32 = 0x0200_0000;
/// The bit that makes a player collide with everything regardless.
pub const IMPENETRABLE: u32 = 0x0020_0000;

/// The sub-step these scenarios drive physics at: over the client's own minimum so the gate
/// opens every time, and under its maximum so each tick is exactly one sub-step.
pub const DT: f64 = 1.0 / 20.0;
/// How far a glide covers in one of those sub-steps for a body with no movement of its own.
#[allow(clippy::cast_possible_truncation)]
pub const STEP: f32 = FALLBACK_SPEED * DT as f32;

pub fn store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store_or_fail())
}

/// A settled local body that carries its own "this is a player" answer, exactly as the object
/// stream gives it on the connected path.
///
/// The settle is not a nicety: an unsettled body has no contact plane, and a measurement taken
/// over one would be measuring the drop.
pub fn fresh_local_body(store: &Arc<RetailDatStore>) -> Character {
    let region = load_region(store).expect("the region decodes");
    let mut c = Character::new(store, &region, DEFAULT_LANDBLOCK, (96.0, 96.0))
        .expect("the ordinary local body is created");
    let start = c.position();
    c.land().load_block_cells(start.cell.landblock());
    c.teleport(start);
    for i in 0..=60 {
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    assert!(
        c.on_ground(),
        "the outdoor start must support the body: {:?}",
        c.position()
    );
    c.world.set_weenie_restrictions(
        c.handle,
        Some(dereth_physics::obj::WeenieRestrictions {
            is_player: true,
            is_creature: true,
            ..Default::default()
        }),
    );
    c
}

/// Put the local body at a point in its own landblock, let it settle, and answer where it came
/// to rest. Remote bodies wear the same setup, so anything this returns is standable.
pub fn settled_at(c: &mut Character, x: f32, y: f32, t: &mut f64) -> Position {
    let mut p = c.position();
    p.frame.origin.x = x;
    p.frame.origin.y = y;
    p.frame.origin.z += 2.0;
    c.teleport(p);
    for _ in 0..90 {
        *t += DT;
        c.update(LocalTime(*t));
    }
    assert!(
        c.on_ground(),
        "({x}, {y}) does not support a body: {:?}",
        c.position()
    );
    c.position()
}

/// One create for a body wearing the local body's own setup, encoded and decoded so the
/// description bits really go over the wire.
///
/// `at == None` is the player's own create, which carries a description and no position: the
/// object stream excludes the player from placement because the local body already owns it.
pub fn create_event(id: ObjectId, at: Option<Position>, bitfield: u32, name: &str) -> SessionEvent {
    let payload = ObjectCreatePayload {
        id,
        objdesc: Default::default(),
        physicsdesc: match at {
            Some(at) => PhysicsDesc {
                bitfield: flags::POSITION | flags::SETUP,
                setup_id: Some(ALUVIAN_MALE_SETUP.0),
                position: Some(PositionWire {
                    objcell_id: at.cell.raw(),
                    frame: dereth_protocol::types::Frame {
                        origin: at.frame.origin.into(),
                        orientation: at.frame.rotation.into(),
                    },
                }),
                state: dereth_physics::PhysicsState::REPORT_COLLISIONS_PS,
                ..Default::default()
            },
            None => PhysicsDesc::default(),
        },
        wdesc: PublicWeenieDesc {
            name: name.to_owned(),
            obj_type: CREATURE,
            bitfield,
            ..Default::default()
        },
    };
    let body = dereth_protocol::write_body(&ItemCreateObject(payload)).expect("the create encodes");
    let decoded = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&body))
        .expect("the encoded create round trips")
        .0;
    assert_eq!(
        decoded.wdesc.bitfield, bitfield,
        "the description bits did not encode"
    );
    SessionEvent::WorldObject {
        opcode: ItemCreateObject::OPCODE,
        body,
    }
}

/// One position update, encoded and decoded so the ground-contact flag really goes over the
/// wire. A non-zero `teleport` stamp is what makes it a teleport rather than a correction.
pub fn position_event(
    id: ObjectId,
    cell: u32,
    origin: Vec3,
    stamp: u16,
    teleport: u16,
    contact: bool,
) -> SessionEvent {
    let mut f = position_flags::ORIENTATION_HAS_NO_W
        | position_flags::ORIENTATION_HAS_NO_X
        | position_flags::ORIENTATION_HAS_NO_Y
        | position_flags::ORIENTATION_HAS_NO_Z;
    if contact {
        f |= position_flags::IS_GROUNDED;
    }
    let msg = MovementPositionEvent {
        id,
        position: PositionPack {
            flags: f,
            origin: dereth_protocol::types::Origin {
                objcell_id: cell,
                origin: dereth_protocol::types::Vec3 {
                    x: origin.x,
                    y: origin.y,
                    z: origin.z,
                },
            },
            instance_timestamp: 0,
            position_timestamp: stamp,
            teleport_timestamp: teleport,
            ..PositionPack::default()
        },
    };
    let body = dereth_protocol::write_body(&msg).expect("the update encodes");
    let decoded = MovementPositionEvent::read(&mut dereth_protocol::Reader::new(&body))
        .expect("the encoded update round trips");
    assert_eq!(
        decoded.position.has_contact(),
        contact,
        "the contact flag did not encode"
    );
    SessionEvent::WorldObject {
        opcode: MovementPositionEvent::OPCODE,
        body,
    }
}

/// Where a remote body's **collision** body is standing, which is what these claims are about.
pub fn body_origin(c: &Character, id: ObjectId) -> Option<Vec3> {
    body_position(c, id).map(|p| p.frame.origin)
}

/// The same, with the cell the body is in.
pub fn body_position(c: &Character, id: ObjectId) -> Option<Position> {
    let h = c.world.by_object_id(id)?;
    let o = c.world.get(h)?;
    o.cell.map(|_| o.position)
}

/// Give a remote body a part array, which is what every drawn object in the world has.
///
/// A body with no movement of its own and standing on walkable ground is put back to sleep at
/// the end of every sub-step, so it runs a sub-step only every other tick and a glide over it
/// advances at half rate. In a whole client this never arises: the scene hangs a motion driver
/// on every animated remote body before the physics sweep. A scenario driving the object
/// stream alone has to say so itself.
pub fn give_part_array(c: &mut Character, id: ObjectId) {
    let h = c.world.by_object_id(id).expect("the body exists");
    c.world
        .get_mut(h)
        .expect("the body exists")
        .set_motion(Box::new(dereth_physics::NullMotion::with_geometry()));
}

/// How far apart two positions are horizontally, across a landblock seam.
pub fn xy_gap(a: &Position, b: &Position) -> f32 {
    let d = dereth_physics::math::get_offset(a, b);
    (d.x * d.x + d.y * d.y).sqrt()
}

use dereth_client_net::client_session::testing::{Corpus, Direction, MockTransport};
use dereth_client_net::client_session::{PositionReporter, Session};
use dereth_client_runtime::character::CharacterInput;
use dereth_protocol::actions::unpack_action;

/// `0xF61C`, the state edge a client reports when what it is doing changes.
pub const MOVE_TO_STATE: u32 = 0xF61C;
/// `0xF753`, the periodic "this is where I am".
pub const AUTONOMOUS_POSITION: u32 = 0xF753;
/// `0xF61B`, the jump.
pub const JUMP: u32 = 0xF61B;

/// The middle of Holtburg's own landblock, which is where every movement scenario spawns.
const SPAWN: (f32, f32) = (96.0, 96.0);

/// A body standing still on Holtburg's terrain, settled for two seconds of frames.
///
/// The settle is not a nicety: an unsettled body has no contact plane, and a measurement
/// taken over one would be measuring the drop.
pub fn settled_body(store: &Arc<RetailDatStore>) -> Character {
    let region = dereth_client_runtime::landblock::load_region(store).expect("the region decodes");
    let mut c =
        Character::new(store, &region, DEFAULT_LANDBLOCK, SPAWN).expect("the character is created");
    for i in 1..=60 {
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    assert!(
        c.on_ground(),
        "the body must settle before anything is measured"
    );
    c
}

/// The production position reporter, active, over a transport that binds nothing.
pub fn reporter() -> (PositionReporter, Session<MockTransport>) {
    let mut r = PositionReporter::new(0.0);
    r.active = true;
    (r, Session::new(MockTransport::new()))
}

/// Every blob the session emitted, as `(sub_type, payload)`.
pub fn emitted(session: &Session<MockTransport>) -> Vec<(u32, Vec<u8>)> {
    session
        .transport
        .sent
        .iter()
        .map(|b| {
            let a = unpack_action(&b.payload).expect("the producer emits game actions");
            (a.sub_type.0, b.payload.clone())
        })
        .collect()
}

/// One emitted blob, through the production reader.
pub fn decode<M: Message>(payload: &[u8]) -> M {
    let mut a = unpack_action(payload).expect("a game action unpacks");
    let m = M::read(&mut a.body).expect("the body decodes");
    a.body
        .expect_exhausted()
        .expect("the body is fully consumed");
    m
}

/// Sample the ground edge, run one frame with `input`, then ask the client what the jump did.
pub fn jump_frame(
    c: &mut Character,
    now: f64,
    input: CharacterInput,
) -> (bool, dereth_protocol::types::Vec3) {
    let left_before = dereth_client_runtime::app::jump_edge_sample(c);
    c.input = input;
    c.update(LocalTime(now));
    dereth_client_runtime::app::body_jump(c, left_before)
}

/// Every jump the recordings carry, as `(extent, upward speed)`, read off the corpus.
///
/// Reading them rather than writing pairs into the source means that promoting another
/// recording extends the measurement instead of leaving it quietly measuring a fraction of what
/// its message claims.
pub fn recorded_jumps() -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    for name in dereth_client_net::client_session::testing::session_names() {
        let Some(corpus) = Corpus::load(name).expect("the recording parses") else {
            continue;
        };
        for b in &corpus.blobs {
            if b.dir != Direction::ClientToServer {
                continue;
            }
            let Ok(mut a) = unpack_action(&b.payload) else {
                continue;
            };
            if a.sub_type.0 != JUMP {
                continue;
            }
            let Ok(m) = <dereth_protocol::movement::MovementJump as Message>::read(&mut a.body)
            else {
                continue;
            };
            out.push((m.0.extent, m.0.velocity.z));
        }
    }
    assert!(
        !out.is_empty(),
        "the recordings carry no jump at all; this scenario would be measuring nothing"
    );
    out
}

// ---------------------------------------------------------------------------------------
// The scene-less frame
// ---------------------------------------------------------------------------------------

/// The channel the client's own refusals are written to.
pub const FEEDBACK_CHANNEL: u32 = 0x1A;
/// What a client with no body of its own says when a drop takes the ground leg.
pub const MID_AIR: &str = "You cannot do that in mid air";

/// A client that draws no world: the object stream, the interaction layer and the retail
/// data, with the frame's own interaction step driven with no world under it.
pub struct SceneLess {
    store: Arc<RetailDatStore>,
    pub objects: dereth_client_runtime::objects::ObjectStream,
    pub inter: dereth_client_runtime::interaction::Interaction,
    player: dereth_primitives::ObjectId,
    pub clock: f64,
}

impl SceneLess {
    pub fn new(store: &Arc<RetailDatStore>) -> Self {
        use dereth_primitives::ObjectId;
        let mut objects = dereth_client_runtime::objects::ObjectStream::new();
        let player = ObjectId(0x5000_0415);
        objects.world.player = Some(player);
        objects.world.tables.inventories.insert(
            player,
            dereth_client_model::objects::ObjectInventory::new(player),
        );
        let mut pw = dereth_client_model::Weenie::new(player);
        pw.pwd.items_capacity = Some(0xFF);
        pw.pwd.containers_capacity = Some(0xFF);
        objects.world.tables.weenies.insert(player, pw);
        let mut h = Self {
            store: Arc::clone(store),
            objects,
            inter: dereth_client_runtime::interaction::Interaction::new(),
            player,
            clock: 1.0,
        };
        h.drive();
        h
    }

    /// One interaction step with **no world**, which is what a frame that drew nothing runs.
    pub fn drive(&mut self) {
        self.clock += 1.0;
        let _ = dereth_client_runtime::interaction::use_time(
            &mut self.inter,
            &self.store,
            None,
            &mut self.objects,
            None,
            Vec::new(),
            false,
            (1024, 768),
            LocalTime(self.clock),
        );
    }

    /// An item in the player's own pack.
    pub fn carry(&mut self, id: dereth_primitives::ObjectId) -> dereth_primitives::ObjectId {
        let mut w = dereth_client_model::Weenie::new(id);
        w.pwd.container_id = Some(self.player);
        w.pwd.stack_size = Some(1);
        w.pwd.max_stack_size = Some(1);
        w.pwd.name = format!("thing {:X}", id.0);
        w.waiting = true;
        w.determine_position_state();
        self.objects.world.tables.weenies.insert(id, w);
        assert!(
            self.objects.world.is_owned_by_player(id),
            "the fixture must be carried"
        );
        id
    }

    /// A left press in the middle of the viewport, and how many picks it armed.
    pub fn viewport_left_press(&mut self) -> u64 {
        use dereth_client_shell::ui::UiMouseEvent;
        use dereth_ui_screens::screens::gameplay::window;
        let armed = self.inter.pick.stats.requests;
        self.inter.wrapper_mouse(
            UiMouseEvent {
                action: dereth_ui::focus::action::PRIMARY_CLICK,
                start: true,
                x: 512,
                y: 384,
                over: Some(window::SMART_BOX),
            },
            (1024, 768),
            true,
        );
        self.inter.pick.stats.requests - armed
    }

    /// Every line the client has written for the player to read, with its channel.
    pub fn lines(&self) -> Vec<(u32, String)> {
        self.objects
            .world
            .scroll
            .pending()
            .iter()
            .map(|f| (f.chat_type, f.body.clone()))
            .collect()
    }
}

// ---------------------------------------------------------------------------------------
// The physics scripts
// ---------------------------------------------------------------------------------------

/// The client's own epsilon: a pause below it is no pause at all.
pub const HOOK_EPSILON: f32 = 0.0002;
/// Hook kinds, by the number the shipped data carries.
pub const CREATE_PARTICLE: u32 = 13;
pub const CALL_PES: u32 = 19;

/// One of the four shipped ambient scripts that chase each other round a ring: each makes an
/// emitter at its start and, part of a second later, calls the next one with a matching
/// pause. Named rather than searched for, because the scenario asserts its *shape* -- so a
/// data set that does not carry it fails loudly instead of silently measuring nothing.
pub const RING_PARENT: dereth_primitives::DataId = dereth_primitives::DataId(0x3300_11C3);
pub const RING_CHILD: dereth_primitives::DataId = dereth_primitives::DataId(0x3300_11C4);
/// The shipped script whose only hook doubles the object it is played on.
pub const DOUBLING: dereth_primitives::DataId = dereth_primitives::DataId(0x3300_0117);

/// A motion driver over the retail data, for an object that is in a room -- which is what
/// both arms of a delayed call test.
pub fn script_driver(store: &Arc<RetailDatStore>) -> dereth_animation::MotionDriver {
    let assets: Arc<dyn dereth_animation::AnimAssets> = Arc::new(
        dereth_world_data::anim_assets::DatAnimAssets::new(Arc::clone(store)),
    );
    let mut d = dereth_animation::MotionDriver::new(assets);
    d.env.in_cell = true;
    d
}

/// One step of what the client runs for an animated static object, in its own order: the
/// scripts, then the timers. Answers what the step raised.
pub fn script_tick(
    d: &mut dereth_animation::MotionDriver,
    now: f64,
) -> Vec<dereth_animation::AnimEvent> {
    d.cur_time = dereth_primitives::ServerTime(now);
    d.update_scripts();
    d.update_fp_hooks();
    d.take_events()
}

/// Play the shipped doubling script and answer the scale it asked for, the way the scene
/// drains it. The premise -- that it doubles, on the spot -- is asserted here.
pub fn scale_the_shipped_script_asks_for(store: &Arc<RetailDatStore>) -> f32 {
    use dereth_assets::{Decode, HookData, PhysicsScript};
    use dereth_dat::DbType;

    let bytes = store
        .read_typed(DbType::PhysicsScript, DOUBLING)
        .expect("the script is shipped");
    let s = PhysicsScript::decode_payload(DOUBLING, &bytes).expect("it decodes");
    let (end, time) = s
        .script_data
        .iter()
        .find_map(|st| match st.hook.data {
            HookData::Scale { end, time } => Some((end, time)),
            _ => None,
        })
        .expect("the script carries a scale hook");
    assert!(
        (end - 2.0).abs() < 1e-6,
        "the shipped script doubles: end = {end}"
    );
    assert!(
        time < HOOK_EPSILON,
        "and it does it on the spot: time = {time}"
    );

    let mut d = script_driver(store);
    d.cur_time = dereth_primitives::ServerTime(0.0);
    assert!(
        d.play_script_internal(DOUBLING),
        "the shipped script queues"
    );

    let mut out = None;
    let mut t = 0.0;
    for _ in 0..30 {
        for e in script_tick(&mut d, t) {
            if let dereth_animation::AnimEvent::SetScale(s) = e {
                assert!(out.is_none(), "the on-the-spot arm raised the event twice");
                out = Some(s);
            }
        }
        t += 1.0 / 30.0;
    }
    let s = out.expect("the shipped scale hook raised no event at all");
    assert!(
        (d.scale - s).abs() < 1e-6,
        "the part-array half and the event disagree"
    );
    s
}

/// The ground everything in the scale scenarios stands on.
pub const GROUND: f32 = 20.0;
const SCALE_BLOCK: dereth_primitives::LandblockId = dereth_primitives::LandblockId(0xA9B4);

pub fn flat_world() -> dereth_physics::PhysicsWorld {
    let mut land = dereth_physics::StaticLandSource::linear();
    for dx in -1_i32..=1 {
        for dy in -1_i32..=1 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let id = dereth_primitives::LandblockId::new((0xA9 + dx) as u8, (0xB4 + dy) as u8);
            land.add_flat_block(id, 10);
        }
    }
    dereth_physics::PhysicsWorld::new(Arc::new(land))
}

fn cell_at(p: dereth_primitives::Vec3) -> dereth_primitives::CellId {
    let mut c = SCALE_BLOCK.cell(1);
    let mut o = p;
    assert!(dereth_physics::landdefs::adjust_to_outside(&mut c, &mut o));
    c
}

fn walker_geometry() -> Arc<dereth_physics::SetupGeometry> {
    use dereth_physics::geom::Sphere;
    use dereth_primitives::Vec3;
    Arc::new(dereth_physics::SetupGeometry {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 0.5), 1.0),
        step_up_height: 0.3,
        step_down_height: 0.3,
        radius: 0.5,
        height: 1.0,
        ..dereth_physics::SetupGeometry::default()
    })
}

/// The obstacle: one sphere sitting on the ground. Its sorting sphere is deliberately
/// generous and the **same** in both arms -- an object's cell shadows are registered from it
/// and scaling does not re-register them, so letting it differ would make the differential
/// about bookkeeping rather than about the collision radius.
fn obstacle_geometry() -> Arc<dereth_physics::SetupGeometry> {
    use dereth_physics::geom::Sphere;
    use dereth_primitives::Vec3;
    Arc::new(dereth_physics::SetupGeometry {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.6), 0.6)],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 0.6), 4.0),
        radius: 0.6,
        height: 1.2,
        ..dereth_physics::SetupGeometry::default()
    })
}

pub fn spawn_walker(
    w: &mut dereth_physics::PhysicsWorld,
    at: dereth_primitives::Vec3,
    per_substep: dereth_primitives::Vec3,
) -> dereth_physics::PhysHandle {
    use dereth_primitives::{Frame, ObjectId, Position, Quat};
    let h = w.create(ObjectId(1), walker_geometry(), true);
    let cell = cell_at(at);
    w.enter_cell(h, cell);
    {
        let o = w.get_mut(h).expect("the walker is live");
        o.position = Position::new(cell, Frame::new(at, Quat::IDENTITY));
        o.set_motion(Box::new(dereth_physics::ScriptedMotion::new(
            vec![Frame::new(per_substep, Quat::IDENTITY); 400],
            true,
        )));
        o.transient_state.set_active_bit(true);
        o.calc_acceleration();
        o.update_time = 0.0;
    }
    w.calc_cross_cells(h, false);
    h
}

pub fn place_obstacle(
    w: &mut dereth_physics::PhysicsWorld,
    at: dereth_primitives::Vec3,
) -> dereth_physics::PhysHandle {
    use dereth_primitives::{Frame, ObjectId, Position, Quat};
    let h = w.create(ObjectId(9), obstacle_geometry(), false);
    let cell = cell_at(at);
    w.enter_cell(h, cell);
    let frame = Frame::new(at, Quat::IDENTITY);
    if let Some(o) = w.get_mut(h) {
        o.set_frame(frame);
        o.position = Position::new(cell, frame);
    }
    w.calc_cross_cells(h, true);
    h
}

// ---------------------------------------------------------------------------------------
// The camera against a wall
// ---------------------------------------------------------------------------------------

/// The retail training academy, whose interior rooms are the walls the camera is backed into.
pub const TRAINING_DUNGEON: u16 = 0x8602;

const BACK_UP_SPEED: f32 = 0.6;
const BACK_UP_SECONDS: f64 = 1.5;
const RUN_SECONDS: f64 = 3.5;

/// Whether a point of an interior cell is inside the room and outside its masonry.
///
/// Two lines, copied from `dereth/client/tests/dat/common/collision_probe`, which the scenario
/// crate cannot reach; this is the smallest use of it. The probe module's own calibration for
/// the predicate stays where it is.
fn in_the_room(
    cell: &dereth_physics::source::EnvCellGeometry,
    local: dereth_primitives::Vec3,
) -> bool {
    cell.cell_bsp
        .as_ref()
        .is_some_and(|b| b.point_inside_cell_bsp(local))
        && !cell
            .physics_bsp
            .as_ref()
            .is_some_and(|b| b.point_intersects_solid(local))
}

/// A point of an interior cell a body can stand at, in the block's own space.
fn a_standable_point(
    cell: &dereth_physics::source::EnvCellGeometry,
) -> Option<dereth_primitives::Vec3> {
    use dereth_primitives::Vec3;
    cell.cell_bsp.as_ref()?;
    for &z in &[0.5f32, 1.0] {
        for i in -12i8..=12 {
            for j in -12i8..=12 {
                let local = Vec3::new(f32::from(i) * 0.5, f32::from(j) * 0.5, z);
                if in_the_room(cell, local) {
                    return Some(dereth_physics::math::localtoglobal(&cell.frame, local));
                }
            }
        }
    }
    None
}

/// A wall to back into: a standable point, a heading, and the way the body walks so that the
/// camera behind it is driven into geometry.
pub struct Wall {
    pub cell: dereth_primitives::CellId,
    start: dereth_primitives::Vec3,
    heading: dereth_primitives::Quat,
    back: dereth_primitives::Vec3,
}

/// One camera run: every camera origin and its distance from the pivot, one per step.
pub struct CameraRun {
    origins: Vec<dereth_primitives::Vec3>,
    pivot_distance: Vec<f32>,
    dt: f64,
}

impl CameraRun {
    /// Peak-to-peak spread of the camera origin over the last `seconds`. It does not depend
    /// on the rate: a settled camera reads about zero and a cycle reads its own size however
    /// finely it is sampled.
    pub fn residual(&self, seconds: f64) -> f32 {
        use dereth_physics::math::V3 as _;
        let tail = self.tail(seconds);
        let mut worst = 0.0f32;
        for a in tail {
            for b in tail {
                worst = worst.max(a.sub(*b).mag2().sqrt());
            }
        }
        worst
    }

    fn tail(&self, seconds: f64) -> &[dereth_primitives::Vec3] {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = ((seconds / self.dt).ceil() as usize).min(self.origins.len());
        &self.origins[self.origins.len() - n..]
    }

    /// The furthest the camera got from the pivot over the last `seconds` -- the calibration:
    /// it must be well under the free-space stand-off or the camera met no wall.
    pub fn max_pivot_distance(&self, seconds: f64) -> f32 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = ((seconds / self.dt).ceil() as usize).min(self.pivot_distance.len());
        self.pivot_distance[self.pivot_distance.len() - n..]
            .iter()
            .copied()
            .fold(0.0f32, f32::max)
    }

    pub fn last(&self) -> dereth_primitives::Vec3 {
        *self.origins.last().expect("the run produced steps")
    }
}

pub fn run_camera(
    src: &Arc<dereth_world_data::land_source::DatLandSource>,
    wall: &Wall,
    dt: f64,
) -> CameraRun {
    use dereth_client_runtime::character::PLAYER_OBJECT_ID;
    use dereth_physics::math::V3 as _;
    use dereth_primitives::{Frame, Position, Vec3};
    use {
        dereth_client_runtime::camera::CameraControl, dereth_client_runtime::camera::CameraInput,
    };

    let mut w =
        dereth_physics::PhysicsWorld::new(Arc::clone(src) as Arc<dyn dereth_physics::LandSource>);
    let h = w.create(PLAYER_OBJECT_ID, walker_geometry(), true);
    w.enter_cell(h, wall.cell);
    {
        let o = w.get_mut(h).expect("the body is live");
        o.position = Position::new(wall.cell, Frame::new(wall.start, wall.heading));
        o.update_time = 0.0;
    }
    w.calc_cross_cells(h, false);

    let mut cam = CameraControl::new(PLAYER_OBJECT_ID);
    let mut origins = Vec::new();
    let mut pivot_distance = Vec::new();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let steps = (RUN_SECONDS / dt).round() as usize;
    for k in 1..=steps {
        #[allow(clippy::cast_precision_loss)]
        let t = k as f64 * dt;
        // The body's position is a pure function of wall-clock time: it backs toward the
        // wall and then stands still, identically in both arms at equal `t`.
        #[allow(clippy::cast_possible_truncation)]
        let travelled = (t.min(BACK_UP_SECONDS) as f32) * BACK_UP_SPEED;
        let origin = wall.start.add(wall.back.mul(travelled));
        {
            let o = w.get_mut(h).expect("the body is live");
            o.position = Position::new(wall.cell, Frame::new(origin, wall.heading));
        }
        // Every step writes the body's position on the line above, so every step is a tick
        // by construction and the smoother must run on all of them.
        cam.update(
            &mut w,
            h,
            PLAYER_OBJECT_ID,
            CameraInput::default(),
            t,
            dt,
            true,
        );
        let pivot = dereth_physics::math::localtoglobal(
            &Frame::new(origin, wall.heading),
            Vec3::new(0.0, 0.0, 1.5),
        );
        origins.push(cam.viewer.frame.origin);
        pivot_distance.push(cam.viewer.frame.origin.sub(pivot).mag2().sqrt());
    }
    CameraRun {
        origins,
        pivot_distance,
        dt,
    }
}

/// Every interior room of the block that has an approach whose control run really presses
/// the camera into a wall -- at most one heading per room, and at most `want` rooms.
pub fn find_walls(
    src: &Arc<dereth_world_data::land_source::DatLandSource>,
    cells: &[dereth_primitives::CellId],
    want: usize,
) -> Vec<(Wall, CameraRun)> {
    use dereth_physics::LandSource as _;
    use dereth_primitives::{Quat, Vec3};

    /// The free-space stand-off, halved: a camera that never gets even this far from the
    /// pivot is pressed hard against something.
    const HALF_FREE: f32 = 2.610_077 / 2.0;

    let mut out = Vec::new();
    for &id in cells {
        if out.len() >= want {
            break;
        }
        let Some(geom) = src.as_ref().env_cell(id) else {
            continue;
        };
        let Some(at) = a_standable_point(&geom) else {
            continue;
        };
        let start = Vec3::new(at.x, at.y, at.z - 0.5);
        for k in 0..16_i8 {
            let a = f32::from(k) * std::f32::consts::TAU / 16.0;
            let half = a * 0.5;
            let heading = Quat::new(
                dereth_primitives::num::math::cosf(half),
                0.0,
                0.0,
                dereth_primitives::num::math::sinf(half),
            );
            let back = Vec3::new(
                dereth_primitives::num::math::sinf(a),
                -dereth_primitives::num::math::cosf(a),
                0.0,
            );
            let wall = Wall {
                cell: id,
                start,
                heading,
                back,
            };
            let r = run_camera(src, &wall, 1.0 / 30.0);
            if r.max_pivot_distance(1.0) < HALF_FREE {
                out.push((wall, r));
                break;
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------
// The shard's own movements, and the body they are applied to
// ---------------------------------------------------------------------------------------

/// A running body is above this and a stopped one below it. Both are stated rather than
/// implied, and they are far enough apart that neither is the other's rounding.
pub const RUNNING: f32 = 3.0;
pub const STOPPED: f32 = 0.5;

/// The recording whose ordinary play carries the shard-authored movements these scenarios
/// are driven from. It is loaded through the decoded corpus, which reassembles the blobs, so
/// nothing here re-does that by hand.
const CONTROL_SESSION: &str = "combat-mode-while-moving";

/// Every movement the recorded shard sent about this session's own character that it authored
/// rather than echoed -- which is every one that takes control of the body away from the
/// player.
fn server_control_buffers() -> Vec<dereth_protocol::movement::MovementBuffer> {
    use dereth_protocol::movement::MovementSetObjectMovement;
    use dereth_protocol::Opcode;

    let corpus = Corpus::load(CONTROL_SESSION)
        .expect("the recording parses")
        .expect("the recording is committed to the repository");
    // The player's own identity, out of the recording's own login.
    let me = corpus
        .blobs
        .iter()
        .find(|b| b.dir == Direction::ServerToClient && b.opcode == 0xF746)
        .map(|b| {
            dereth_primitives::ObjectId(u32::from_le_bytes([
                b.payload[4],
                b.payload[5],
                b.payload[6],
                b.payload[7],
            ]))
        })
        .expect("the recording carries its own login");

    let mut out = Vec::new();
    let mut about_others = 0usize;
    for b in &corpus.blobs {
        if b.dir != Direction::ServerToClient || b.opcode != Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0
        {
            continue;
        }
        let mut r = dereth_protocol::Reader::new(&b.payload[4..]);
        let Ok(msg) = MovementSetObjectMovement::read(&mut r) else {
            continue;
        };
        if msg.id != me {
            about_others += 1;
            continue;
        }
        let buf = msg.decoded_movement().expect("its movement buffer decodes");
        if buf.autonomous {
            continue;
        }
        out.push(buf);
    }
    // The filter must be able to exclude something, or what it answers is a count of
    // everything.
    assert!(
        about_others > 0,
        "the identity filter excluded nothing at all"
    );
    assert!(
        !out.is_empty(),
        "the recording carries no shard-authored movement of the player"
    );
    out
}

/// Turn a recorded movement's interpreted half into the client's own.
fn interpreted_state(
    wire: &dereth_protocol::movement::InterpretedMotionState,
) -> dereth_animation::motion::InterpretedMotionState {
    use dereth_animation::motion::{ActionNode, InterpretedMotionState};
    use dereth_animation::MotionCommand;
    let cmd = |i: Option<u16>, fallback: MotionCommand| {
        i.and_then(MotionCommand::from_index).unwrap_or(fallback)
    };
    let base = InterpretedMotionState::default();
    InterpretedMotionState {
        current_style: cmd(wire.current_style, base.current_style),
        forward_command: cmd(wire.forward_command, base.forward_command),
        forward_speed: wire.forward_speed.unwrap_or(base.forward_speed),
        sidestep_command: cmd(wire.sidestep_command, base.sidestep_command),
        sidestep_speed: wire.sidestep_speed.unwrap_or(base.sidestep_speed),
        turn_command: cmd(wire.turn_command, base.turn_command),
        turn_speed: wire.turn_speed.unwrap_or(base.turn_speed),
        actions: wire
            .actions
            .iter()
            .filter_map(|a| {
                Some(ActionNode {
                    action: MotionCommand::from_index(a.command_index)?,
                    speed: a.speed,
                    stamp: u32::from(a.stamp()),
                    autonomous: a.autonomous(),
                })
            })
            .collect(),
    }
}

/// `Ready` is the command the shard's own stop carries.
const READY_COMMAND: u16 = 3;

/// The **pure** stops among the shard's movements: the ones that say stop and carry no action
/// of their own.
///
/// Both halves of that filter are load-bearing. A stop that also carries an action leaves the
/// body standing still for a reason that has nothing to do with control -- the action's own
/// animation does it -- and folding the two together would make a scenario green or red for
/// the wrong reason.
fn recorded_stops() -> Vec<dereth_protocol::movement::MovementBuffer> {
    server_control_buffers()
        .into_iter()
        .filter(|b| {
            b.body
                .interpreted
                .as_ref()
                .is_some_and(|s| s.forward_command == Some(READY_COMMAND) && s.actions.is_empty())
        })
        .collect()
}

/// One of them.
pub fn a_recorded_stop() -> dereth_protocol::movement::MovementBuffer {
    recorded_stops().swap_remove(0)
}

/// Two of them, for a scenario that needs a departure and an arrival.
pub fn two_recorded_stops() -> (
    dereth_protocol::movement::MovementBuffer,
    dereth_protocol::movement::MovementBuffer,
) {
    let mut stops = recorded_stops();
    assert!(
        stops.len() >= 2,
        "the recording carries fewer than two pure stops"
    );
    let a = stops.swap_remove(0);
    let b = stops.swap_remove(0);
    (a, b)
}

/// The stop that **does** carry a shard-authored action: the discriminating half of the pair,
/// and the one an action scenario is about.
pub fn a_recorded_stop_with_an_action() -> dereth_protocol::movement::MovementBuffer {
    let b = server_control_buffers()
        .into_iter()
        .find(|b| {
            b.body
                .interpreted
                .as_ref()
                .is_some_and(|s| s.forward_command == Some(READY_COMMAND) && !s.actions.is_empty())
        })
        .expect("the recording carries a stop with an action of its own");
    assert_eq!(
        b.body.interpreted.as_ref().map(|s| s.actions.len()),
        Some(1),
        "the premise: exactly one shard-authored action"
    );
    b
}

/// What one driven run produced.
pub struct Run {
    pub retakes: u32,
    at: Vec<(f32, f32)>,
}

/// The frame, from the client's own retake through the body's own step, thirty times a
/// second.
pub fn drive(
    c: &mut Character,
    mc: &mut dereth_client_runtime::character::MovementCommands,
    input: &mut CharacterInput,
    from: u32,
    frames: u32,
) -> Run {
    let mut r = Run {
        retakes: 0,
        at: Vec::with_capacity(frames as usize),
    };
    for i in 0..frames {
        let (pending, moving) = {
            let d = c.driver();
            (d.movement.motions_pending(), d.movement.is_moving_to())
        };
        if mc.use_time(pending, moving, input) {
            r.retakes += 1;
            c.take_control_from_server();
        }
        c.input = *input;
        c.update(LocalTime(f64::from(from + i + 1) / 30.0));
        let o = c.position().frame.origin;
        r.at.push((o.x, o.y));
    }
    r
}

/// Ground speed over a window of a run, measured across the straight line between its ends.
pub fn over(r: &Run, a: u32, b: u32) -> f32 {
    let (a, b) = (a as usize, b as usize);
    assert!(
        b > a && b <= r.at.len(),
        "window {a}..{b} is outside a {}-step run",
        r.at.len()
    );
    let (p, q) = (r.at[a], r.at[b - 1]);
    #[allow(clippy::cast_precision_loss)]
    let seconds = (b - a) as f32 / 30.0;
    ((q.0 - p.0).powi(2) + (q.1 - p.1).powi(2)).sqrt() / seconds
}

/// Ground speed **along the path**, summed step by step.
///
/// [`over`] measures the straight line between the ends, which is the right question for a
/// body running in one direction and the wrong one for a body running *and turning*: the
/// chord of an arc is shorter than the arc, and a genuinely running body measured that way
/// reads about half what it is doing.
pub fn along(r: &Run, a: u32, b: u32) -> f32 {
    let (ai, bi) = (a as usize, b as usize);
    assert!(
        bi > ai && bi <= r.at.len(),
        "window outside a {}-step run",
        r.at.len()
    );
    let d: f32 = r.at[ai..bi]
        .windows(2)
        .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let seconds = (bi - ai) as f32 / 30.0;
    d / seconds
}

fn movement_event(
    a: dereth_input::ActionId,
    start: bool,
) -> dereth_client_runtime::actions::Action {
    dereth_client_runtime::actions::Action {
        id: a,
        phase: if start {
            dereth_client_runtime::actions::ActionPhase::Begin
        } else {
            dereth_client_runtime::actions::ActionPhase::End
        },
        extent: 1.0,
        repeats: 0,
    }
}

/// A settled body that runs rather than walks, plus the command interpreter that drives it.
/// Running is the default on every shipped character, so the key walks you.
pub fn running_body() -> (
    Character,
    dereth_client_runtime::character::MovementCommands,
    CharacterInput,
) {
    use dereth_client_runtime::actions::movement::{action, on_action};
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let c = settled_body(&store);
    let mut mc = dereth_client_runtime::character::MovementCommands::default();
    let mut input = CharacterInput::default();
    mc.ui_toggles_run = true;
    assert!(mc.on_action(
        on_action(&movement_event(action::TOGGLE_RUN_WALK, false), |_| None),
        &mut input
    ));
    assert!(
        input.run,
        "running is the default state and the key walks you"
    );
    let _ = mc.take_control_retake_pending();
    (c, mc, input)
}

/// One key event through the application's own path: the interpreter, then its drain of the
/// body-side half of taking control back. Both halves, in that order, are what a frame does.
pub fn key(
    c: &mut Character,
    mc: &mut dereth_client_runtime::character::MovementCommands,
    input: &mut CharacterInput,
    a: dereth_input::ActionId,
    down: bool,
) {
    use dereth_client_runtime::actions::movement::on_action;
    assert!(
        mc.on_action(on_action(&movement_event(a, down), |_| None), input),
        "the interpreter consumes every movement action it is handed"
    );
    if mc.take_control_retake_pending() {
        c.take_control_from_server();
    }
    c.input = *input;
}

/// A shard-authored movement, applied exactly as the object stream applies one: the style
/// word first, then the interpreted state, then the latch the frame drains into losing
/// control. Nothing is synthesised; the buffer came off a recording.
pub fn server_takes_control(
    c: &Character,
    mc: &mut dereth_client_runtime::character::MovementCommands,
    input: &mut CharacterInput,
    buf: &dereth_protocol::movement::MovementBuffer,
) {
    let state = buf.body.interpreted.as_ref().map(interpreted_state);
    let style = dereth_animation::MotionCommand::from_index(buf.body.current_style)
        .or_else(|| state.as_ref().map(|s| s.current_style));
    if let Some(style) = style {
        c.driver_mut().apply_movement_style(style);
    }
    if let Some(state) = state.as_ref() {
        c.driver_mut().move_to_interpreted_state(state, true);
    }
    mc.lose_control_to_server(input);
}

/// Teleport the body `dz` upwards and let it fall back.
///
/// The fall is asserted as well as the landing: a body that kept the contact of wherever it
/// had been standing would hang in the air for ever while still answering that it is on the
/// ground, and a landing assertion alone cannot tell that apart from a real fall.
pub fn hop(
    c: &mut Character,
    mc: &mut dereth_client_runtime::character::MovementCommands,
    input: &mut CharacterInput,
    dz: f32,
) {
    use dereth_primitives::{Frame, Position, Vec3};
    let here = c.position();
    c.teleport(Position::new(
        here.cell,
        Frame::new(
            Vec3 {
                x: here.frame.origin.x,
                y: here.frame.origin.y,
                z: here.frame.origin.z + dz,
            },
            here.frame.rotation,
        ),
    ));
    let up = c.position().frame.origin.z;
    assert!(
        up > here.frame.origin.z + dz * 0.5,
        "the teleport put him up there: {up}"
    );
    let _ = drive(c, mc, input, 60, 90);
    assert!(c.on_ground(), "he landed");
    let down = c.position().frame.origin.z;
    assert!(
        down < up - dz * 0.5,
        "...and he really fell to get there: {up} -> {down}"
    );
}

// ---------------------------------------------------------------------------------------
// A body walking an approach
// ---------------------------------------------------------------------------------------

/// A settled body with a frame counter, for the scenarios about approaches and follows.
pub struct Approaching {
    pub c: Character,
    frame: u32,
}

impl Approaching {
    pub fn new(store: &Arc<RetailDatStore>) -> Self {
        Self {
            c: settled_body(store),
            frame: 60,
        }
    }

    pub fn frames(&mut self, count: u32) {
        for _ in 0..count {
            self.frame += 1;
            self.c.update(LocalTime(f64::from(self.frame) / 30.0));
        }
    }

    /// Start walking to a point twenty metres away and get properly under way, so that what
    /// a scenario interrupts is a live approach and not one that has already finished.
    pub fn approach(&mut self) {
        use dereth_animation::motion::{MoveToRequest, MovementParameters};
        let before = self.c.position();
        let mut goal = before;
        goal.frame.origin.y += 20.0;
        self.c.perform_move_to(
            &MoveToRequest::MoveToPosition { pos: goal },
            &MovementParameters::default(),
            Some(1.0),
        );
        self.frames(20);
        assert!(
            self.c.is_moving_to(),
            "the approach must still be unfinished"
        );
        assert!(
            dereth_animation::motion::moveto::distance(&before, &self.c.position()) > 0.1,
            "and the body must really be walking it"
        );
        assert_ne!(
            self.c
                .driver()
                .movement
                .interp
                .interpreted_state
                .forward_command,
            dereth_animation::MotionCommand::READY,
            "the real animation interpreter must be walking"
        );
    }

    /// The body is free afterwards: it walks when the player asks, stops when he lets go, and
    /// stays stopped. Answers whether all of that held.
    pub fn next_input_and_release(&mut self) -> bool {
        use dereth_animation::MotionCommand;
        self.c.input = CharacterInput::default();
        self.frames(90);
        let before = self.c.position();
        self.c.input.forward = true;
        self.frames(45);
        let walked = dereth_animation::motion::moveto::distance(&before, &self.c.position()) > 0.5
            && self.c.driver().movement.interp.raw_state.forward_command
                == MotionCommand::WALK_FORWARD;
        self.c.input = CharacterInput::default();
        self.frames(60);
        let stopped = self.c.position();
        let released = !self.c.is_moving_to()
            && self
                .c
                .driver()
                .movement
                .interp
                .interpreted_state
                .forward_command
                == MotionCommand::READY;
        self.frames(60);
        let stayed =
            dereth_animation::motion::moveto::distance(&stopped, &self.c.position()) < 0.05;
        walked && released && stayed
    }
}

// ---------------------------------------------------------------------------------------
// Replaying a recording for its teleports
// ---------------------------------------------------------------------------------------

/// One teleport the object stream reported.
#[derive(Debug, Clone, Copy)]
pub struct Teleported {
    pub pos: dereth_primitives::Position,
}

/// What one recording's replay measured.
pub struct Replayed {
    /// Position messages about the player -- the denominator.
    pub player_positions: u64,
    /// The teleports the client applied, in order.
    pub teleports: Vec<Teleported>,
    /// The body, when one was asked for: stood up the way the client stands one up at world
    /// entry, at the first position the shard gives the player, and moved by nothing except
    /// the client's own teleport step.
    pub body: Option<Character>,
    /// Where that body was stood up, i.e. the shard's first word on where the player is.
    ///
    /// Answered rather than logged: a body built at the destination would satisfy every
    /// assertion about a teleport without one ever happening.
    pub first_position: Option<dereth_primitives::Position>,
}

/// Replay `session` through the production endpoint, object stream and teleport step,
/// stopping after the `stop_after`th teleport when one is named.
///
/// **The feed loop is the one in `dereth_testkit::login`, narrowed.** That module is private to
/// the crate, so this is the smallest copy of it rather than a call. Nothing here binds a
/// socket: the endpoint is the replay one and only the recording's server-to-client datagrams
/// are fed.
pub fn replay_teleports(
    session: &str,
    store: Option<&Arc<RetailDatStore>>,
    stop_after: Option<usize>,
) -> Replayed {
    use dereth_client_net::client_session::testing::capture::peer;
    use dereth_client_net::client_session::SessionEvent;

    // The recording and the endpoint are `dereth_testkit::replay`'s. What stays
    // here is the loop *body*, which accumulates this scenario's own state and is not the
    // shared one.
    let recs = dereth_testkit::replay::records(session);
    let mut net = dereth_testkit::replay::recorded_endpoint(&recs);
    let mut objects = dereth_client_runtime::objects::ObjectStream::new();
    let mut entered = false;
    let mut out: Vec<Teleported> = Vec::new();
    let mut body: Option<Character> = None;
    let mut first_position = None;

    for r in &recs {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, peer(r.pair), now);
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
        // The client's own world entry: stand the body where the shard says the player is,
        // once, as soon as it says it.
        if let (Some(store), None) = (store, body.as_ref()) {
            let start = objects
                .player()
                .and_then(|id| objects.presence(id))
                .and_then(|p| p.position);
            if let Some(pos) = start {
                body = Some(body_at(store, pos));
                first_position = Some(pos);
            }
        }
        // The client's own teleport step, once per frame.
        let applied = match body.as_mut() {
            Some(c) => dereth_client_runtime::app::apply_player_teleport(&mut objects, c),
            None => objects.take_player_teleport(),
        };
        if let Some(pos) = applied {
            out.push(Teleported { pos });
            if stop_after == Some(out.len()) {
                break;
            }
        }
    }
    Replayed {
        player_positions: objects.stats.player_positions,
        teleports: out,
        body,
        first_position,
    }
}

/// Stand a body up the way the client does at world entry: build it in the piece of land the
/// shard named and put it on the shard's own position.
fn body_at(store: &Arc<RetailDatStore>, pos: dereth_primitives::Position) -> Character {
    let region = dereth_client_runtime::landblock::load_region(store).expect("the region decodes");
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: a cell id's top sixteen bits are its piece of land. Not a float cast.
    let landblock = (pos.cell.0 >> 16) as u16;
    let mut c =
        Character::new(store, &region, landblock, (96.0, 96.0)).expect("the character is created");
    c.teleport(pos);
    c
}

/// The cell a position report names, whichever of the two kinds of report it is.
pub fn reported_cell(payload: &[u8]) -> u32 {
    let a = unpack_action(payload).expect("the producer emits game actions");
    match a.sub_type.0 {
        AUTONOMOUS_POSITION => {
            let mut body = a.body;
            <dereth_protocol::movement::MovementAutonomousPosition as Message>::read(&mut body)
                .expect("the report decodes")
                .0
                .position
                .objcell_id
        }
        MOVE_TO_STATE => {
            let mut body = a.body;
            <dereth_protocol::movement::MovementMoveToState as Message>::read(&mut body)
                .expect("the report decodes")
                .0
                .position
                .objcell_id
        }
        other => panic!("a position report of an unexpected kind: {other:#06X}"),
    }
}

/// The first recording that carries a teleport, and how many it carries. Searched rather than
/// named, so that a corpus that grows is measured rather than ignored.
pub fn a_recording_with_a_teleport() -> (&'static str, usize) {
    for name in dereth_client_net::client_session::testing::session_names() {
        let n = replay_teleports(name, None, None).teleports.len();
        if n > 0 {
            return (name, n);
        }
    }
    panic!("no recording carries a teleport of the player at all")
}

/// The recording that carries the most of them.
pub fn the_recording_with_the_most_teleports() -> &'static str {
    let mut best = ("", 0usize);
    for name in dereth_client_net::client_session::testing::session_names() {
        let n = replay_teleports(name, None, None).teleports.len();
        if n > best.1 {
            best = (name, n);
        }
    }
    assert!(
        best.1 > 0,
        "no recording carries a teleport of the player at all"
    );
    best.0
}

/// A recording and the ordinal of one of its teleports whose destination is **inside a
/// room** -- a cell of a building rather than a patch of open land. That is the case the
/// destination's geometry has to be fetched for.
pub fn a_recorded_teleport_into_a_room() -> (&'static str, usize) {
    for name in dereth_client_net::client_session::testing::session_names() {
        let r = replay_teleports(name, None, None);
        for (i, t) in r.teleports.iter().enumerate() {
            if t.pos.cell.0 & 0xFFFF >= 0x100 {
                return (name, i + 1);
            }
        }
    }
    panic!("no recording teleports the player into a room")
}

// ---------------------------------------------------------------------------------------
// The training-dungeon door
// ---------------------------------------------------------------------------------------

/// The door the recording's own player opens.
const DOOR: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x77f0_3033);
/// The recording the door journey is taken from.
const DOOR_SESSION: &str = "early-inventory-and-casting";
/// The shipped setup the door's body is built from.
const DOOR_SETUP: dereth_primitives::DataId = dereth_primitives::DataId(0x0200_05da);
/// The open state the recording's own create carries.
const DOOR_OPEN_STATE: u32 = 0x0001_001c;
/// The piece of land the academy's rooms live in.
const ACADEMY: u16 = 0x7f03;

fn wire_position(p: &dereth_protocol::types::PositionWire) -> dereth_primitives::Position {
    use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};
    Position::new(
        CellId(p.objcell_id),
        Frame::new(
            Vec3::new(p.frame.origin.x, p.frame.origin.y, p.frame.origin.z),
            Quat::new(
                p.frame.orientation.w,
                p.frame.orientation.x,
                p.frame.orientation.y,
                p.frame.orientation.z,
            ),
        ),
    )
}

/// The recorded player's own place, the recorded door's place, and the movement the shard
/// sent when the player used it -- all three out of the recording, none of them written here.
fn recorded_door() -> (
    dereth_primitives::Position,
    dereth_primitives::Position,
    dereth_protocol::movement::MovementBuffer,
) {
    let rows = Corpus::load(DOOR_SESSION)
        .expect("the recording parses")
        .expect("the recording is committed to the repository")
        .blobs;
    let row = |i: usize| {
        rows.iter()
            .find(|r| r.idx == i)
            .unwrap_or_else(|| panic!("the recording has no {i}"))
    };
    let mut a = dereth_protocol::actions::unpack_action(&row(143).payload)
        .expect("the recorded placement unpacks");
    let state = <dereth_protocol::movement::MovementMoveToState as Message>::read(&mut a.body)
        .expect("it decodes")
        .0;
    let create = <dereth_protocol::objects::ItemCreateObject as Message>::read(
        &mut dereth_protocol::Reader::new(&row(52).payload[4..]),
    )
    .expect("the recorded door create decodes")
    .0;
    assert_eq!(create.id, DOOR, "the recorded create is the door's");
    let m = <dereth_protocol::movement::MovementSetObjectMovement as Message>::read(
        &mut dereth_protocol::Reader::new(&row(147).payload[4..]),
    )
    .expect("the recorded answer decodes");
    let buf = m.decoded_movement().expect("its movement buffer decodes");
    (
        wire_position(&state.position),
        wire_position(&create.physicsdesc.position.expect("the door has a place")),
        buf,
    )
}

/// A body standing where the recording's own player stood, in the academy, with the command
/// interpreter that drives it.
pub struct DoorRig {
    pub c: Character,
    pub mc: dereth_client_runtime::character::MovementCommands,
    pub input: CharacterInput,
    t: u32,
    pub door_pos: dereth_primitives::Position,
}

impl DoorRig {
    pub fn new() -> Self {
        use dereth_client_runtime::actions::movement::{action, on_action};
        let (player, door_pos, _) = recorded_door();
        let store = Arc::new(dereth_dat::testing::open_store_or_fail());
        let region =
            dereth_client_runtime::landblock::load_region(&store).expect("the region decodes");
        let mut c =
            Character::new(&store, &region, ACADEMY, (96.0, 96.0)).expect("the body is created");
        c.land().load_block_cells(player.cell.landblock());
        c.teleport(player);
        c.stop_completely_from_action();
        for i in 1..=90 {
            c.update(LocalTime(f64::from(i) / 30.0));
        }
        assert!(
            c.on_ground(),
            "the recorded spot the player stood at must support a body"
        );
        let mut mc = dereth_client_runtime::character::MovementCommands::default();
        let mut input = CharacterInput::default();
        mc.ui_toggles_run = true;
        assert!(mc.on_action(
            on_action(&movement_event(action::TOGGLE_RUN_WALK, false), |_| None),
            &mut input
        ));
        let _ = mc.take_control_retake_pending();
        Self {
            c,
            mc,
            input,
            t: 90,
            door_pos,
        }
    }

    /// One key edge, through the application's own two halves.
    pub fn key(&mut self, a: dereth_input::ActionId, down: bool) {
        use dereth_client_runtime::actions::movement::on_action;
        assert!(self.mc.on_action(
            on_action(&movement_event(a, down), |_| None),
            &mut self.input
        ));
        if self.mc.take_control_retake_pending() {
            self.c.take_control_from_server();
        }
        self.c.input = self.input;
    }

    /// One frame: the retake, then the body. `deliver` says where the door is whenever the
    /// client asks, the way the object table answers it.
    fn frame(&mut self, deliver: Option<bool>) {
        let (pending, moving) = {
            let d = self.c.driver();
            (d.movement.motions_pending(), d.movement.is_moving_to())
        };
        if self.mc.use_time(pending, moving, &mut self.input) {
            self.c.take_control_from_server();
        }
        self.c.input = self.input;
        if let Some(ok) = deliver {
            if self.c.wanted_target().is_some() {
                let p = self.door_pos;
                self.c.update_target(p, dereth_primitives::Vec3::ZERO, ok);
            }
        }
        self.t += 1;
        self.c.update(LocalTime(f64::from(self.t) / 30.0));
    }

    pub fn run(&mut self, n: u32, deliver: Option<bool>) {
        for _ in 0..n {
            self.frame(deliver);
        }
    }

    /// The shard's recorded answer to using the door, applied the way the object stream
    /// applies one, followed by the handover of control.
    pub fn door_turn(&mut self) {
        use dereth_animation::motion::{MoveToRequest, MovementParameters};
        let (_, _, buf) = recorded_door();
        let style = dereth_animation::MotionCommand::from_index(buf.body.current_style)
            .expect("the recorded answer names a way of carrying the body");
        self.c.apply_movement_style(style);
        let Some(dereth_protocol::movement::MoveToArm::TurnToObject {
            target,
            desired_heading,
            params,
        }) = buf.body.decode_move_to().expect("the recorded arm decodes")
        else {
            panic!("the recorded answer is not the door's turn")
        };
        let dereth_protocol::movement::MovementParameters::TurnTo {
            bitfield, speed, ..
        } = params
        else {
            panic!("the recorded turn carries the wrong parameters")
        };
        let p = MovementParameters {
            flags: bitfield,
            speed,
            desired_heading,
            ..MovementParameters::default()
        };
        self.c.perform_move_to(
            &MoveToRequest::TurnToObject {
                object_id: target,
                top_level_id: target,
            },
            &p,
            None,
        );
        self.lose_control();
    }

    /// The shard-directed approach, which is how a player reaches a door out of reach.
    pub fn door_approach(&mut self, at: dereth_primitives::Position, radius: f32) {
        use dereth_animation::motion::{flags, MoveToRequest, MovementParameters};
        self.door_pos = at;
        let p = MovementParameters {
            distance_to_object: 0.6,
            flags: flags::STICKY,
            ..MovementParameters::default()
        };
        self.c.perform_move_to(
            &MoveToRequest::MoveToObject {
                object_id: DOOR,
                top_level_id: DOOR,
                radius,
                height: 2.0,
            },
            &p,
            Some(1.0),
        );
        self.lose_control();
        assert!(self.c.is_moving_to(), "the approach was installed");
        // The client asks where the door is on a half-second gate, so the walk waits a good
        // fifteen frames for its first answer before it can begin.
        for _ in 0..600 {
            self.frame(Some(true));
            if !self.c.is_moving_to() {
                break;
            }
        }
    }

    fn lose_control(&mut self) {
        let Self { c, mc, input, .. } = self;
        mc.lose_control_to_server_with_finish(input, || c.finish_jump());
        self.c.input = self.input;
    }

    /// A shard-authored action, the way the object stream delivers one.
    pub fn server_action(&mut self, act: dereth_animation::MotionCommand) {
        use dereth_animation::motion::{ActionNode, InterpretedMotionState};
        let state = InterpretedMotionState {
            actions: vec![ActionNode {
                action: act,
                speed: 1.0,
                stamp: 1,
                autonomous: false,
            }]
            .into(),
            ..InterpretedMotionState::default()
        };
        self.c.move_to_interpreted_state(&state);
        self.lose_control();
    }

    fn heading(&self) -> f32 {
        dereth_physics::math::get_heading(&self.c.position().frame)
    }

    /// A fresh forward press held for `n` frames: how far did the body actually go?
    pub fn probe_translate(&mut self, n: u32) -> f32 {
        use dereth_client_runtime::actions::movement::action;
        let from = self.c.position();
        self.key(action::MOVE_FORWARD, true);
        self.run(n, Some(true));
        let d = dereth_animation::motion::moveto::distance(&from, &self.c.position());
        self.key(action::MOVE_FORWARD, false);
        self.run(15, Some(true));
        d
    }

    /// Forward, and if that does not move him, backward. A body that can go **neither** way
    /// is the report; one that is merely walled in front of him is not.
    pub fn probe_translate_either_way(&mut self, n: u32) -> f32 {
        use dereth_client_runtime::actions::movement::action;
        let f = self.probe_translate(n);
        if f > 0.25 {
            return f;
        }
        let from = self.c.position();
        self.key(action::MOVE_BACKWARD, true);
        self.run(n, Some(true));
        let b = dereth_animation::motion::moveto::distance(&from, &self.c.position());
        self.key(action::MOVE_BACKWARD, false);
        self.run(15, Some(true));
        f.max(b)
    }

    /// A fresh turn press held for `n` frames: how far did the heading actually go?
    pub fn probe_turn(&mut self, n: u32) -> f32 {
        use dereth_client_runtime::actions::movement::action;
        let from = self.heading();
        self.key(action::TURN_RIGHT, true);
        self.run(n, Some(true));
        let d = dereth_animation::motion::heading_diff(
            self.heading(),
            from,
            dereth_animation::MotionCommand::TURN_RIGHT,
        );
        self.key(action::TURN_RIGHT, false);
        self.run(15, Some(true));
        d.min(360.0 - d).abs().max(d.min(360.0 - d))
    }

    /// Point the body at the door and hold forward until it is in the door's own room.
    pub fn walk_into_the_doorway(&mut self) {
        use dereth_client_runtime::actions::movement::action;
        let to = self.door_pos;
        let want = dereth_animation::motion::moveto::position_heading(&self.c.position(), &to);
        let mut here = self.c.position();
        dereth_physics::math::set_heading(&mut here.frame, want);
        self.c.teleport(here);
        self.run(10, Some(true));
        self.key(action::MOVE_FORWARD, true);
        for _ in 0..120 {
            self.frame(Some(true));
            if self.c.position().cell == to.cell {
                break;
            }
        }
        self.key(action::MOVE_FORWARD, false);
        self.run(20, Some(true));
    }

    /// The recorded door as a body in the player's own world, so he can collide with it.
    pub fn register_door(&mut self, at: dereth_primitives::Position) -> dereth_physics::PhysHandle {
        use dereth_assets::{Decode, Setup};
        let store = Arc::new(dereth_dat::testing::open_store_or_fail());
        let bytes = store
            .read_typed(dereth_dat::DbType::Setup, DOOR_SETUP)
            .expect("the door's shipped setup");
        let s = Setup::decode_payload(DOOR_SETUP, &bytes).expect("it decodes");
        let mut stats = dereth_client_runtime::object_physics::SetupPartStats::default();
        let g = Arc::new(
            dereth_client_runtime::object_physics::setup_geometry_with_parts(
                &store, &s, &mut stats,
            ),
        );
        let h = self.c.world.create(DOOR, g, true);
        self.c.world.enter_cell(h, at.cell);
        if let Some(o) = self.c.world.get_mut(h) {
            o.state = dereth_physics::PhysicsState(o.state.0 | DOOR_OPEN_STATE);
            o.set_frame(at.frame);
            o.position = at;
        }
        assert!(
            self.c
                .world
                .get(h)
                .expect("the door is live")
                .state
                .has_physics_bsp(),
            "the door keeps the shape it is collided against"
        );
        self.c.world.calc_cross_cells(h, true);
        h
    }
}

// ---------------------------------------------------------------------------------------
// The shipped gameplay screen, and the map window on it
// ---------------------------------------------------------------------------------------

/// The real gameplay screen, built from the shipped layout, with no client around it.
///
/// The claims in this section are about what a panel writes into the element tree, which the
/// screen does for itself; building a whole client around it would only make the run slower.
pub fn shipped_gameplay() -> (dereth_ui::UiSystem, Box<dyn dereth_ui::framework::Screen>) {
    use dereth_ui::framework::DidMapperResolver;
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;

    let dir = dereth_dat::testing::dat_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the shipped layouts are this scenario's oracle and are absent at {}",
        dir.display()
    );
    let store = RetailDatStore::open_dir(&dir).expect("the retail data files open");
    let master_id = dereth_primitives::DataId(0x3900_0001);
    let bytes = dereth_primitives::AssetSource::read(&store, master_id)
        .expect("the shipped property table");
    let master =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
            .expect("it decodes");

    let mut ui = dereth_ui::UiSystem::new((800, 600));
    ui.property_types = master.property_types();
    let mut flow = dereth_ui::UiFlow::new();
    dereth_ui_screens::register_all(&mut ui, &mut flow);
    let store = std::rc::Rc::new(store);
    let resolver = std::rc::Rc::new(
        DidMapperResolver::load_via_master(store.as_ref()).expect("the id mapper"),
    );
    dereth_ui_screens::env::install(&mut ui, store, resolver);

    let mut screen = GamePlayScreen::create_screen();
    screen
        .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen creates from its real layout");
    ui.requests.clear();
    (ui, screen)
}

pub fn as_gameplay(
    screen: &mut Box<dyn dereth_ui::framework::Screen>,
) -> &mut dereth_ui_screens::screens::gameplay::GamePlayScreen {
    let any: &mut dyn std::any::Any = &mut **screen;
    any.downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
        .expect("the gameplay screen")
}

/// Whether the player is outside, where he is and what the clock says -- and nothing else.
/// Every other question the panels can ask keeps its default answer.
#[derive(Debug, Clone)]
pub struct MapView {
    pub outside: bool,
    pub coords: Option<(f32, f32)>,
    pub date_time: Option<(String, String)>,
}

impl MapView {
    /// Holtburg's own reading, which is what the coordinate line is calibrated against.
    pub const NORTH: f32 = 42.2;
    pub const EAST: f32 = 33.8;
    pub const EXPECTED_COORD_LINE: &'static str = "42.2N, 33.8E";
    pub const EXPECTED_DATE_LINE: &'static str =
        "Date: Morningthaw 14, 10 P.Y.\nTime: Late Morning";
    pub const EXPECTED_LATER_DATE_LINE: &'static str = "Date: Morningthaw 15, 10 P.Y.\nTime: Night";

    pub fn outdoors() -> Self {
        Self {
            outside: true,
            coords: Some((Self::NORTH, Self::EAST)),
            date_time: Some((
                "Morningthaw 14, 10 P.Y.".to_owned(),
                "Late Morning".to_owned(),
            )),
        }
    }

    /// In a room, a game-day later. The client cannot work out coordinates for a room, so
    /// they are unavailable as well as not asked for -- both halves, as in the client.
    pub fn indoors_later() -> Self {
        Self {
            outside: false,
            coords: None,
            date_time: Some(("Morningthaw 15, 10 P.Y.".to_owned(), "Night".to_owned())),
        }
    }
}

impl dereth_ui_screens::view::GameView for MapView {
    fn player(&self) -> Option<dereth_primitives::ObjectId> {
        Some(dereth_primitives::ObjectId(0x5000_0001))
    }
    fn player_outside(&self) -> bool {
        self.outside
    }
    fn player_coords(&self) -> Option<(f32, f32)> {
        self.coords
    }
    fn game_date_time(&self) -> Option<(String, String)> {
        self.date_time.clone()
    }
}

/// The whole observable surface of the map window's three elements, off the live tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapPixels {
    pub date_text: String,
    pub coord_text: String,
    pub icon_box: (i32, i32, i32, i32),
    pub icon_visible: bool,
    pub house_visible: bool,
}

fn text_of(ui: &mut dereth_ui::UiSystem, h: Option<dereth_ui::ElemHandle>) -> String {
    match h.and_then(|h| ui.text_element_mut(h)) {
        Some(t) => t.glyphs.inq_text(false),
        None => String::new(),
    }
}

pub fn element_is_visible(ui: &dereth_ui::UiSystem, h: Option<dereth_ui::ElemHandle>) -> bool {
    h.and_then(|h| ui.node(h))
        .is_some_and(|n| n.region.flags.visible)
}

pub fn sample_map(
    ui: &mut dereth_ui::UiSystem,
    g: &dereth_ui_screens::screens::gameplay::GamePlayScreen,
) -> MapPixels {
    let b = g
        .map
        .player_icon
        .and_then(|h| ui.node(h))
        .map(|n| n.region.box_);
    MapPixels {
        date_text: text_of(ui, g.map.date_time_text),
        coord_text: text_of(ui, g.map.coordinate_text),
        icon_box: b.map_or((0, 0, 0, 0), |b| (b.x0, b.y0, b.x1, b.y1)),
        icon_visible: element_is_visible(ui, g.map.player_icon),
        house_visible: element_is_visible(ui, g.map.house_icon),
    }
}

// ---------------------------------------------------------------------------------------
// The world-map page
// ---------------------------------------------------------------------------------------

/// The same shipped gameplay screen as [`shipped_gameplay`], as the concrete screen and with
/// the layout assets installed, which is what the tooltip window needs to exist at all.
///
/// Without them the shell answers "no tooltip" and a scenario about a town's name cannot
/// tell that apart from a machine with no data files.
pub fn shipped_map_page() -> (
    dereth_ui::UiSystem,
    dereth_ui_screens::screens::gameplay::GamePlayScreen,
) {
    use dereth_ui::framework::{DidMapperResolver, Screen as _};
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;

    let dir = dereth_dat::testing::dat_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the shipped layouts are this scenario's oracle and are absent at {}",
        dir.display()
    );
    let store = RetailDatStore::open_dir(&dir).expect("the retail data files open");
    let master_id = dereth_primitives::DataId(0x3900_0001);
    let bytes = dereth_primitives::AssetSource::read(&store, master_id)
        .expect("the shipped property table");
    let master =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
            .expect("it decodes");

    let mut ui = dereth_ui::UiSystem::new((800, 600));
    ui.property_types = master.property_types();
    let mut flow = dereth_ui::UiFlow::new();
    dereth_ui_screens::register_all(&mut ui, &mut flow);
    let store = std::rc::Rc::new(store);
    let resolver = std::rc::Rc::new(
        DidMapperResolver::load_via_master(store.as_ref()).expect("the id mapper"),
    );
    dereth_ui_screens::env::install(
        &mut ui,
        std::rc::Rc::clone(&store) as std::rc::Rc<dyn dereth_primitives::AssetSource>,
        resolver,
    );
    ui.assets = Some(std::rc::Rc::clone(&store) as std::rc::Rc<dyn dereth_ui::LayoutAssets>);

    let mut screen = GamePlayScreen::default();
    screen
        .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen creates from its real layout");
    pump_screen(&mut ui, &mut screen);
    ui.requests.clear();
    (ui, screen)
}

/// The shell's own message delivery, bounded.
pub fn pump_screen(
    ui: &mut dereth_ui::UiSystem,
    s: &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
) {
    use dereth_ui::framework::Screen as _;
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            return;
        }
        for d in batch {
            if let dereth_ui::Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            }
        }
    }
}

/// The panel the map page lives on, read off the shipped layout rather than written down.
fn maps_panel_id(g: &dereth_ui_screens::screens::gameplay::GamePlayScreen) -> u32 {
    g.panels
        .pages
        .iter()
        .find(|p| p.element == dereth_ui::ElementId(0x1000_018C))
        .expect("the maps page is one of the panel's own pages")
        .panel_id
}

/// Open the map page the way a toolbar button does.
pub fn open_the_map_page(
    ui: &mut dereth_ui::UiSystem,
    g: &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
) {
    let panel = maps_panel_id(g);
    g.recv_set_panel_visibility(ui, panel, true);
    pump_screen(ui, g);
}

/// ...and close it again.
pub fn close_the_map_page(
    ui: &mut dereth_ui::UiSystem,
    g: &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
) {
    let panel = maps_panel_id(g);
    g.recv_set_panel_visibility(ui, panel, false);
    pump_screen(ui, g);
}

/// One town on the map, as the element tree holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TownNote {
    pub box_: (i32, i32, i32, i32),
    pub tip: Option<String>,
}

pub fn map_notes(
    ui: &dereth_ui::UiSystem,
    g: &dereth_ui_screens::screens::gameplay::GamePlayScreen,
) -> Vec<TownNote> {
    g.map_notes
        .iter()
        .filter_map(|h| ui.node(*h))
        .map(|n| TownNote {
            box_: (
                n.region.box_.x0,
                n.region.box_.y0,
                n.region.box_.x1,
                n.region.box_.y1,
            ),
            tip: n.tooltip_text.clone(),
        })
        .collect()
}

/// Where the client puts the player's own mark, written out from the instructions rather
/// than by calling the thing under test -- so the assertion is not that function spelled
/// twice.
///
/// The coordinate is scaled by ten and biased by a thousand and twenty-four, multiplied by
/// the marker box's own size, divided by two thousand and forty-eight and truncated towards
/// zero; the north arm subtracts the biased value from two thousand and forty-seven first.
/// The mark's own size -- one more than the difference of its edges -- is halved and taken
/// off. That last term is the one the client and the measurement of it once both had one
/// short, where the error cancels sideways and does not vertically.
pub fn marker_oracle(
    area: (i32, i32, i32, i32),
    east: f32,
    north: f32,
    w: i32,
    h: i32,
) -> (i32, i32) {
    let (x0, x1, y0, y1) = area;
    let aw = f64::from(x1 - x0 + 1);
    let ah = f64::from(y1 - y0 + 1);
    let biased = |v: f32| f64::from(v) * 10.0 + 1024.0;
    #[allow(clippy::cast_possible_truncation)]
    let trunc = |v: f64| v.trunc() as i32;
    (
        x0 - w / 2 + trunc(biased(east) * aw / 2048.0),
        y0 - h / 2 + trunc((2047.0 - biased(north)) * ah / 2048.0),
    )
}

// ---------------------------------------------------------------------------------------
// Replaying a recording for the world it built
// ---------------------------------------------------------------------------------------

/// The object stream a recording had built at its **busiest** instant.
///
/// The peak and not the end: every recording finishes with a logout that destroys every
/// object, so a question about what the client had to draw has to be asked while it had it.
///
/// **The feed loop is [`replay_teleports`]'s**, which is `dereth_testkit::login`'s, narrowed
/// again. Nothing here binds a socket.
pub fn replay_objects_at_peak(session: &str) -> dereth_client_runtime::objects::ObjectStream {
    let peak = feed(session, None).1;
    feed(session, Some(peak)).0
}

/// The instants of a recording a scenario may want to ask a question at.
#[derive(Debug, Clone, Copy, Default)]
pub struct Stations {
    /// `(record index, how many things the world held)` -- the peak, wherever the player was.
    pub busiest: (usize, usize),
    /// The busiest instant at which the player's own room was a piece of open land.
    pub outdoor: Option<(usize, dereth_primitives::CellId, usize)>,
    /// The busiest instant at which it was inside a building.
    pub indoor: Option<(usize, dereth_primitives::CellId, usize)>,
}

/// The three instants above, for one recording.
pub fn radar_stations(session: &str) -> Stations {
    feed(session, None).3
}

/// The world a recording had built at `stop`, with a display synced to the recording's **own**
/// recorded player position -- which is where every room id a scenario reads comes from.
pub fn scene_at(
    session: &str,
    stop: usize,
) -> (
    dereth_client_runtime::objects::ObjectStream,
    dereth_client_shell::hud::Hud,
) {
    let objects = feed(session, Some(stop)).0;
    let player = objects.world.player.expect("the recording named a player");
    let pos = objects
        .presence(player)
        .and_then(|p| p.position)
        .expect("the player has a place");
    let mut hud = dereth_client_shell::hud::Hud::new();
    hud.sync(
        &objects,
        Some(dereth_client_runtime::hud::ViewerFrame {
            position: pos,
            heading_degrees: 0.0,
        }),
    );
    (objects, hud)
}

/// Replay `session` up to `stop` records, and answer the stream, the index and size of the
/// busiest instant it passed through, and the stations it found on the way.
fn feed(
    session: &str,
    stop: Option<usize>,
) -> (
    dereth_client_runtime::objects::ObjectStream,
    usize,
    usize,
    Stations,
) {
    use dereth_client_net::client_session::testing::capture::peer;
    use dereth_client_net::client_session::SessionEvent;

    // See `replay_teleports`: the loader and the endpoint are the harness's, the loop is
    // this scenario's.
    let recs = dereth_testkit::replay::records(session);
    let mut net = dereth_testkit::replay::recorded_endpoint(&recs);
    let mut objects = dereth_client_runtime::objects::ObjectStream::new();
    let mut entered = false;
    let mut best = (0usize, 0usize);
    let mut st = Stations::default();
    let limit = stop.unwrap_or(recs.len());
    for (i, r) in recs.iter().take(limit).enumerate() {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, peer(r.pair), now);
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
        let n = objects.world.tables.weenies.len();
        if n > best.1 {
            best = (i + 1, n);
        }
        st.busiest = best;
        // A question about the radar indoors has to be asked at an instant the player really
        // was indoors, which is never the busiest instant of any of these recordings.
        let here = objects.presences().count();
        if let Some(cell) = objects
            .world
            .player
            .and_then(|id| objects.presence(id))
            .and_then(|p| p.position)
            .map(|p| p.cell)
        {
            let slot = if dereth_physics::landdefs::is_outdoors(cell) {
                &mut st.outdoor
            } else {
                &mut st.indoor
            };
            if slot.is_none_or(|(_, _, most)| here > most) {
                *slot = Some((i + 1, cell, here));
            }
        }
    }
    (objects, best.0, best.1, st)
}

/// The recording that had the most objects in the world at once.
///
/// Searched rather than named, so that a corpus that grows is measured rather than ignored.
pub fn the_busiest_recording() -> &'static str {
    let mut best = ("", 0usize);
    for name in dereth_client_net::client_session::testing::session_names() {
        let n = feed(name, None).2;
        if n > best.1 {
            best = (name, n);
        }
    }
    assert!(
        best.1 > 0,
        "no recording puts an object in the world at all"
    );
    best.0
}

// ---------------------------------------------------------------------------------------
// The radar
// ---------------------------------------------------------------------------------------

/// Every recording that puts an object in the world and names a player.
///
/// Searched rather than listed, so a corpus that grows is measured rather than ignored. It is
/// cached because replaying eighteen recordings to find out is the expensive half of every
/// radar scenario.
pub fn recordings_with_a_world() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        let mut out = Vec::new();
        for name in dereth_client_net::client_session::testing::session_names() {
            let objects = replay_objects_at_peak_uncached(name);
            if objects.world.player.is_some() && objects.presences().count() > 1 {
                out.push(*name);
            }
        }
        assert!(
            !out.is_empty(),
            "no recording builds a world with a player in it"
        );
        out
    })
}

fn replay_objects_at_peak_uncached(session: &str) -> dereth_client_runtime::objects::ObjectStream {
    let peak = feed(session, None).1;
    feed(session, Some(peak)).0
}

/// A geometry to project into. Nothing asserted about the radar depends on the numbers --
/// every claim is about *which* things appear -- but they are the shipped ring's, so the
/// projection runs at its real scale.
pub fn radar_geometry() -> dereth_ui_screens::mapradar::radar::RadarGeometry {
    dereth_ui_screens::mapradar::radar::RadarGeometry {
        radius: 50,
        center: (60.0, 60.0),
    }
}

/// The radar list a recorded scene produces, with the player's own place as the viewer.
pub fn radar_list(
    objects: &dereth_client_runtime::objects::ObjectStream,
) -> Vec<dereth_ui_screens::view::RadarEntry> {
    let player = objects.world.player.expect("the recording named a player");
    let pos = objects
        .presence(player)
        .and_then(|p| p.position)
        .expect("the player has a place");
    let mut hud = dereth_client_shell::hud::Hud::new();
    hud.sync(
        objects,
        Some(dereth_client_runtime::hud::ViewerFrame {
            position: pos,
            heading_degrees: 0.0,
        }),
    );
    hud.radar.clone()
}

/// The colour a thing on the radar should take, worked out from the recording's own
/// description rather than by asking the client -- so the two can disagree.
///
/// The order is the client's own decision list: the shard's override first, then a portal,
/// then a trader, then a creature, then the several kinds of player, then the colour that
/// means nothing matched. The overrides a fellowship or an allegiance would apply are not
/// modelled, because no recording in the corpus has the player in one.
pub fn expected_blip_colour(pwd: &dereth_protocol::types::PublicWeenieDesc) -> u32 {
    use dereth_ui_screens::mapradar::radar::{
        semantic, BLUE, BRIGHT_GREEN, CYAN, GOLD, GREEN, PINK, PURPLE, RED, WHITE, YELLOW,
    };
    // A thing the shard hides from the interface takes no colour of its own.
    if pwd.bitfield & 0x80 != 0 {
        return semantic::DEFAULT.hex;
    }
    if let Some(v) = pwd.blip_color.filter(|v| *v != 0) {
        // The order the client's own switch reaches these in, which is **not** the order the
        // ten colours sit in the shipped data. Writing out the data's order instead is the
        // mistake this function exists to be able to make on its own, and it once did.
        return match v {
            1 => BLUE.hex,
            2 => GOLD.hex,
            3 => WHITE.hex,
            4 => PURPLE.hex,
            5 => RED.hex,
            6 => PINK.hex,
            7 => GREEN.hex,
            8 => YELLOW.hex,
            9 => CYAN.hex,
            10 => BRIGHT_GREEN.hex,
            _ => semantic::DEFAULT.hex,
        };
    }
    if pwd.bitfield & 0x0004_0000 != 0 {
        return semantic::PORTAL.hex;
    }
    if pwd.bitfield & 0x0000_0200 != 0 {
        return semantic::VENDOR.hex;
    }
    let is_player = pwd.bitfield & 0x0000_0008 != 0;
    let is_creature = pwd.obj_type & 0x0000_0010 != 0;
    if pwd.bitfield & 0x0000_0010 != 0 && is_creature && !is_player {
        return semantic::CREATURE.hex;
    }
    if !is_player {
        return semantic::DEFAULT.hex;
    }
    if pwd.bitfield & 0x0010_0000 != 0 && pwd.bitfield & 0x0000_0040 == 0 {
        semantic::ADMIN.hex
    } else if pwd.bitfield & 0x0000_0020 != 0 {
        semantic::PLAYER_KILLER.hex
    } else if pwd.bitfield & 0x0200_0000 != 0 {
        semantic::PK_LITE.hex
    } else if pwd.bitfield & 0x0020_0000 != 0 {
        semantic::CREATURE.hex
    } else {
        semantic::DEFAULT.hex
    }
}

// ---------------------------------------------------------------------------------------
// The radar's range
// ---------------------------------------------------------------------------------------

/// How far the radar reaches out of doors, and inside. Written out as numbers rather than
/// fetched through the same call the client makes: a scenario that read both sides through
/// one symbol could not see a wrong one.
pub const OUTDOOR_RANGE: f32 = 75.0;
pub const INDOOR_RANGE: f32 = 25.0;
/// The first room number that means "inside a building".
pub const ENV_CELL_FLOOR: u32 = 0x100;

/// The first recording that builds a world with a player in it.
pub fn a_recording_with_a_world() -> &'static str {
    recordings_with_a_world()[0]
}

/// A recording, an instant of it, and the room -- the busiest moment at which some recorded
/// player was inside a building.
pub fn a_recorded_indoor_place() -> (&'static str, usize, dereth_primitives::CellId) {
    let mut best: Option<(&'static str, usize, dereth_primitives::CellId, usize)> = None;
    for session in recordings_with_a_world() {
        if let Some((at, cell, here)) = radar_stations(session).indoor {
            if best.is_none_or(|(_, _, _, most)| here > most) {
                best = Some((session, at, cell, here));
            }
        }
    }
    let (s, at, cell, _) = best.expect("no recording ever puts the player inside a building");
    (s, at, cell)
}

/// The shell's own message delivery for a boxed screen, bounded.
pub fn pump_boxed(
    ui: &mut dereth_ui::UiSystem,
    screen: &mut Box<dyn dereth_ui::framework::Screen>,
) {
    for _ in 0..16 {
        let out = ui.drain_outbox();
        if out.is_empty() {
            return;
        }
        for d in out {
            match d {
                dereth_ui::Delivery::Element { msg, .. } => {
                    screen.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg)
                }
                dereth_ui::Delivery::Global { id, param, .. } => {
                    screen.on_global_message(
                        &mut dereth_ui::framework::ScreenCx::new(ui),
                        id,
                        param,
                    );
                }
                dereth_ui::Delivery::Notice { id, payload, .. } => {
                    screen.on_notice(&mut dereth_ui::framework::ScreenCx::new(ui), id, &payload);
                }
            }
        }
    }
    panic!("the message pump did not settle in sixteen rounds");
}

/// The picture one of an element's own states carries, read out of the shipped layout.
pub fn state_image(
    ui: &dereth_ui::UiSystem,
    h: dereth_ui::ElemHandle,
    state: u32,
) -> Option<dereth_primitives::DataId> {
    let sd = ui
        .node(h)
        .expect("the element is alive")
        .desc
        .access_state(dereth_ui::StateId(state))
        .unwrap_or_else(|| panic!("the shipped layout gives it no state {state:#x}"));
    match sd.media.first().map(|m| &m.fields) {
        Some(dereth_ui::desc::state_desc::MediaFields::Image { file, .. }) => Some(*file),
        other => panic!("that state carries {other:?}, not a picture"),
    }
}

// ---------------------------------------------------------------------------------------
// The radar's own controls
// ---------------------------------------------------------------------------------------

/// The radar window on a shipped gameplay screen.
#[allow(clippy::borrowed_box)]
pub fn radar_element(
    ui: &dereth_ui::UiSystem,
    screen: &Box<dyn dereth_ui::framework::Screen>,
) -> dereth_ui::ElemHandle {
    let root = *screen.roots().first().expect("the screen has a root");
    ui.get_child_recursive(root, dereth_ui_screens::screens::gameplay::window::RADAR)
        .expect("the radar")
}

/// One press and release of the left button at a point.
pub fn click_at(ui: &mut dereth_ui::UiSystem, x: i32, y: i32) {
    ui.mouse_move(dereth_primitives::LocalTime(1.0), x, y);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
}

/// One pass of the radar's own drawing, and what it drew.
pub fn radar_blips(
    ui: &mut dereth_ui::UiSystem,
    screen: &mut Box<dyn dereth_ui::framework::Screen>,
    objects: &dereth_client_runtime::objects::ObjectStream,
    hud: &dereth_client_shell::hud::Hud,
) -> Vec<dereth_ui_screens::mapradar::radar::Blip> {
    let g = as_gameplay(screen);
    let view = hud.view(objects);
    assert!(g.update_radar(ui, &view), "the radar wrote nothing at all");
    let geom = dereth_ui_screens::mapradar::radar::RadarGeometry {
        radius: g.radar.radius,
        center: g.radar.center,
    };
    dereth_ui_screens::mapradar::radar::draw_objects(
        &hud.radar,
        hud.radar.iter().find(|o| o.is_self),
        geom,
        dereth_ui_screens::mapradar::radar::radar_range(true),
        None,
        false,
    )
}

/// Where the client would put every thing on the radar, and which thing it is -- worked out
/// here from the recording's own descriptions rather than by asking the client, so that the
/// two can disagree.
pub fn expected_blip_map(
    objects: &dereth_client_runtime::objects::ObjectStream,
    hud: &dereth_client_shell::hud::Hud,
    screen: &mut Box<dyn dereth_ui::framework::Screen>,
) -> Vec<((i32, i32), dereth_primitives::ObjectId)> {
    let g = as_gameplay(screen);
    let (radius, cx, cy) = (g.radar.radius, g.radar.center.0, g.radar.center.1);
    let range = OUTDOOR_RANGE;
    let range_sq = (range - 1.0) * (range - 1.0);
    #[allow(clippy::cast_precision_loss)]
    let scale = radius as f32 / range;
    let player = objects.world.player.expect("the recording named a player");
    #[allow(clippy::cast_possible_truncation)]
    let trunc = |v: f32| v as i32;
    let mut out = Vec::new();
    for o in &hud.radar {
        let w = objects.world.weenie(o.id);
        let showable = w.is_some_and(|w| matches!(w.pwd.radar_enum.unwrap_or(0), 2..=4));
        if o.id == player || !showable || !o.in_world {
            continue;
        }
        let (px, py, _) = o.player_space;
        if px * px + py * py >= range_sq {
            continue;
        }
        let sx = trunc(px * scale + cx);
        let sy = trunc(cy - py * scale);
        let (bx, by) = (trunc(cx), trunc(cy));
        if sx < bx - radius || sx > bx + radius || sy < by - radius || sy > by + radius {
            continue;
        }
        out.push(((sx, sy), o.id));
    }
    out
}

/// Everything a point on the radar is within the client's own reach of.
///
/// In a thin scene there is exactly one and "the thing that mark stands for" is unambiguous.
/// In a crowded one there are several within a few pixels of each other, and which of them
/// the client names is its own tie-breaking; the scenario asserts that it names one of
/// these, and asserts the exact answer where there is only one to give.
pub fn things_within_reach(
    map: &[((i32, i32), dereth_primitives::ObjectId)],
    at: (i32, i32),
) -> Vec<dereth_primitives::ObjectId> {
    let mut out = Vec::new();
    for &((x, y), id) in map {
        let (dx, dy) = (at.0 - x, at.1 - y);
        if dx * dx + dy * dy < dereth_ui_screens::mapradar::radar::HOVER_DIST_SQ {
            out.push(id);
        }
    }
    out
}
