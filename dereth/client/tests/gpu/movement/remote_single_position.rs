//! A remote object has one position, not two: selection, the range watch, the radar blip and the
//! draw all read the physics body, mid-walk and after it, and an update without the contact bit
//! moves neither the body nor any reader. Fixture: the retail dats, a headless software GPU, and a
//! created creature (setup `0x02000001`) driven by encoded `0xF745 Item_CreateObject` and `0xF748
//! Movement_PositionEvent` messages; no window and no injected input.
//!
//! # The original rule
//!
//! The original client keeps exactly one position per object. Selection, radar, displayed names,
//! hearing, the selection range watch, distance calculations that account for object radii, sound
//! emitters, and drawing all read that same stored position. Drawing passes its frame to the
//! object's part array.
//!
//! **And the `0xF748` handler stores the position it was sent nowhere.**
//! It builds the decoded wire position as a stack local and passes that local to movement or
//! teleport handling. A false return ends the handler without further work. A true return makes
//! the handler read the body's stored position back for its constraint step, rather than reading
//! the wire local. The local dies with the handler. Selection state likewise stores only the
//! current and previous selected IDs and the object's selected flag, with no second position for
//! readers to disagree about.
//!
//! A presence record that kept the *wire's* position beside the body's would let readers disagree
//! by the whole correction: the `!contact` arm repositions nothing, and an interpolated walk moves
//! the body over the following sub-steps while the wire already names the destination.
//!
//! # The bench
//!
//! The same shape `selection::selection_persistence` uses. Each frame runs
//! the three production steps `App::frame` runs in its order — `WorldScene::sync_objects`,
//! `WorldScene::update` (the physics sub-steps and `finish_object_physics`), and then
//! `ObjectStream::publish_physics_cells` — and nothing else. Every `0xF745`/`0xF748` is encoded
//! and decoded so the flags really go over the wire. The movement arm selected for each message is
//! asserted from `ObjectPhysics::stats` rather than assumed.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::sync::Arc;

use dereth_client::character::PLAYER_OBJECT_ID;
use dereth_client::hud::{Hud, ViewerFrame};
use dereth_client::object_range::SceneRangeGeometry;
use dereth_client::objects::ObjectStream;
use dereth_client::selection_geometry::SceneSelectionPhysics;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_client_model::range::ObjectRangeGeometry;
use dereth_client_model::weenie::{bitfield, item_type};
use dereth_client_net::client_session::SessionEvent;
use dereth_dat::RetailDatStore;
use dereth_physics::pmanager::CLOSE_ENOUGH;
use dereth_primitives::{LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::types::physicsdesc::flags;
use dereth_protocol::types::{PhysicsDesc, PositionWire, PublicWeenieDesc};
use dereth_protocol::Message;
use dereth_render::device::{DeviceConfig, Gpu};

/// `0x02000001`, the Aluvian male setup — a creature body, which is what `SceneRangeGeometry`
/// needs on both ends of its measurement and what gives the walk a real collision radius.
const MONSTER_SETUP: u32 = 0x0200_0001;
const MOVER: ObjectId = ObjectId(0x8400_0001);

/// The frame's own step, 30 Hz — over `MIN_QUANTUM`, so the physics-time gate opens every frame,
/// and well under `MAX_QUANTUM`, so each frame is exactly one physics sub-step.
const DT: f64 = 1.0 / 30.0;

/// How far the object is asked to walk. It is comfortably inside the movement handler's 96 m
/// interpolation threshold and the 100 m outdoor autonomy-blip threshold, so the arm under test is
/// interpolation rather than a snap or blip.
const WALK: f32 = 3.0;

/// How far apart two points are. `dereth_primitives::Vec3` carries no `Sub`, and every measurement below
/// is a gap rather than a coordinate, so this is the whole of the station's own arithmetic.
fn gap(a: Vec3, b: Vec3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

struct Bench {
    gpu: Gpu,
    scene: WorldScene,
    store: Arc<RetailDatStore>,
    objects: ObjectStream,
    hud: Hud,
    now: f64,
    stamp: u16,
}

impl Bench {
    fn new() -> Self {
        let store = Arc::new(
            dereth_dat::testing::open_store()
                .expect("the retail dats are the station's own argument: set DERETH_TEST_DAT_DIR"),
        );
        let cfg = DeviceConfig {
            width: 800,
            height: 600,
            ..DeviceConfig::default()
        };
        let mut gpu = Gpu::new(None, &cfg).expect("a D3D12 WARP device");
        let region = dereth_client::world::load_region(&store).expect("the region decodes");
        let scfg = SceneConfig {
            cell_statics: false,
            mesh_collision: false,
            land_radius: 1,
            scenery_radius: 0,
            particles: false,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(&store, &mut gpu, scfg).expect("the scene loads");
        scene
            .attach_character(&store, &region, &mut gpu)
            .expect("the body is created");
        assert!(
            dereth_physics::landdefs::is_outdoors(
                scene.character.as_ref().expect("a body").position().cell
            ),
            "the premise: the body is outdoors, so the walk is under the outdoor blip distance"
        );

        let mut objects = ObjectStream::new();
        objects.world.player = Some(PLAYER_OBJECT_ID);
        let mut me = dereth_client_model::Weenie::new(PLAYER_OBJECT_ID);
        me.valid = true;
        me.has_phys_obj = true;
        me.qualities = Some(dereth_client_model::qualities::Qualities::new());
        objects.world.tables.weenies.insert(PLAYER_OBJECT_ID, me);

        let mut b = Self {
            gpu,
            scene,
            store,
            objects,
            hud: Hud::new(),
            now: 1.0,
            stamp: 0,
        };
        // Let the local body settle on the terrain before anything is measured from it: every
        // reader below measures *from* this body.
        for _ in 0..90 {
            b.frame();
        }
        assert!(
            b.scene.character.as_ref().expect("a body").on_ground(),
            "the local body must settle before anything is measured: {:?}",
            b.player()
        );
        b
    }

    /// The local player's live body position — the origin every reader below converts into.
    fn player(&self) -> Position {
        self.scene.character.as_ref().expect("a body").position()
    }

    /// One frame, in `App::frame`'s own order and with nothing else in it.
    fn frame(&mut self) {
        self.now += DT;
        let now = self.now;
        let Self {
            store,
            gpu,
            scene,
            objects,
            ..
        } = self;
        scene
            .sync_objects(store, gpu, objects)
            .expect("sync_objects");
        scene.update(
            dereth_client::camera::CameraInput::default(),
            dereth_client::character::CharacterInput::default(),
            LocalTime(now),
            DT as f32,
        );
        // `App::frame` runs this immediately after `world.update`, which is the physics step and
        // `finish_object_physics`. It publishes the body's cell *and* its `position` onto the
        // presence, so the record every reader shares is the body's.
        if let Some(c) = scene.character.as_ref() {
            objects.publish_physics_cells(&c.world);
        }
    }

    /// `ObjectPhysics::sync` — `App::sync_objects`' step, which gives a created object the physics
    /// body every reader below measures and applies its received movement.
    fn sync_bodies(&mut self) {
        let Self {
            store,
            objects,
            scene,
            ..
        } = self;
        let ch = scene.character.as_mut().expect("a body");
        objects.sync_physics(store, &mut ch.world);
    }

    /// A position a body can rest at: `x`/`y` in the player's own block, `z` on the terrain.
    ///
    /// `LandSource::ground_height` is the same table `Character::new` drops the local body onto,
    /// and a settled body stands about 5 mm above it.
    fn ground_at(&self, dx: f32, dy: f32) -> Position {
        let p = self.player();
        let block = p.cell.landblock();
        let (x, y) = (p.frame.origin.x + dx, p.frame.origin.y + dy);
        let z = self
            .scene
            .character
            .as_ref()
            .expect("a body")
            .land()
            .ground_height(block, x, y)
            .expect("the player's own block has a height table");
        Position::new(
            p.cell,
            dereth_primitives::Frame::new(Vec3::new(x, y, z + 0.005), Quat::IDENTITY),
        )
    }

    /// An encoded and decoded `0xF745 Item_CreateObject`, following the original object's creation
    /// and enter-world placement path.
    fn create(&mut self, id: ObjectId, at: &Position) {
        let payload = ObjectCreatePayload {
            id,
            objdesc: Default::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::POSITION | flags::SETUP,
                setup_id: Some(MONSTER_SETUP),
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
            wdesc: PublicWeenieDesc {
                name: "Mover".to_owned(),
                obj_type: item_type::CREATURE,
                bitfield: bitfield::ATTACKABLE,
                ..Default::default()
            },
        };
        let body = dereth_protocol::write_body(&ItemCreateObject(payload)).expect("F745 encodes");
        self.objects.apply_event(
            &SessionEvent::WorldObject {
                opcode: ItemCreateObject::OPCODE,
                body,
            },
            LocalTime(self.now),
        );
        self.sync_bodies();
        // The original radar-eligibility check reads the public description's optional radar enum.
        // This bench sets the corresponding current weenie field rather than the wire field, so
        // the blip is drawn.
        if let Some(w) = self.objects.world.tables.weenies.get_mut(id) {
            w.pwd.radar_enum = Some(4); // ShowAlways
        }
        self.objects.world.update_visible_object_list();
        assert!(
            self.objects.physics.handle(id).is_some(),
            "premise: {id:?} has a physics object, so every reader can measure to it"
        );
    }

    /// One **ordinary** `0xF748 Movement_PositionEvent` — a newer `POSITION_TS`, no teleport
    /// stamp, `IS_GROUNDED` set or clear as asked. The arm it takes is asserted, not assumed.
    fn position_event(&mut self, id: ObjectId, at: &Position, contact: bool) {
        self.stamp += 1;
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
                    objcell_id: at.cell.raw(),
                    origin: dereth_protocol::types::Vec3 {
                        x: at.frame.origin.x,
                        y: at.frame.origin.y,
                        z: at.frame.origin.z,
                    },
                },
                position_timestamp: self.stamp,
                teleport_timestamp: 0,
                ..PositionPack::default()
            },
        };
        let body = dereth_protocol::write_body(&msg).expect("F748 encodes");
        let decoded = MovementPositionEvent::read(&mut dereth_protocol::Reader::new(&body))
            .expect("the encoded F748 round trips");
        assert_eq!(
            decoded.position.has_contact(),
            contact,
            "the contact flag did not encode"
        );
        self.objects.apply_event(
            &SessionEvent::WorldObject {
                opcode: MovementPositionEvent::OPCODE,
                body,
            },
            LocalTime(self.now),
        );
        assert_eq!(
            self.objects.stats.stale_positions, 0,
            "every 0xF748 here is newer"
        );
        self.sync_bodies();
    }

    // -- the five readers, each exactly as production reaches it ---------------------------------

    /// The body's one authoritative position, and the answer every reader below must give.
    fn body(&self, id: ObjectId) -> Position {
        let h = self.objects.physics.handle(id).expect("the body exists");
        let ch = self.scene.character.as_ref().expect("a body");
        ch.world.get(h).expect("the body is in the world").position
    }

    /// Selection's per-candidate player-space conversion, through `SceneSelectionPhysics`.
    fn selection(&self, id: ObjectId) -> Vec3 {
        let at = self.player();
        let v = SceneSelectionPhysics::new(Some(&at), &self.objects)
            .get(id)
            .expect("the selection seam can answer for it")
            .player_space;
        Vec3::new(v.0, v.1, v.2)
    }

    /// The selection **range watch**, through `SceneRangeGeometry`, which the production range
    /// polling path uses.
    fn watch(&self, id: ObjectId) -> f32 {
        SceneRangeGeometry::new(
            self.scene.character.as_ref(),
            &self.objects.physics,
            Some(PLAYER_OBJECT_ID),
        )
        .expect("the geometry has a local body")
        .distance(id, PLAYER_OBJECT_ID, false, false)
        .expect("both ends resolve to a physics object")
    }

    /// The radar blip's player-space conversion, through `Hud::sync`.
    /// This is also the table `Hud::speaker_player_space` reads back for hearing range, so a sound
    /// emitter's range is the same measurement.
    fn radar(&mut self, id: ObjectId) -> Vec3 {
        let at = self.player();
        self.hud.sync(
            &self.objects,
            Some(ViewerFrame {
                position: at,
                heading_degrees: 0.0,
            }),
        );
        let e = self
            .hud
            .radar
            .iter()
            .find(|e| e.id == id)
            .expect("the radar walk reached it");
        assert!(
            e.in_world,
            "the blip is drawn, so its player space is the one on screen"
        );
        Vec3::new(e.player_space.0, e.player_space.1, e.player_space.2)
    }

    /// The draw — `SceneObject::position`, which `WorldScene::render_frame_of` turns into the
    /// frame the object's parts are submitted at.
    fn drawn(&self, id: ObjectId) -> Position {
        self.scene
            .server_object_position(id)
            .expect("the object is in the scene")
    }

    /// Every reader, against one position: the whole claim in one assertion.
    fn assert_everything_agrees_with(&mut self, id: ObjectId, truth: &Position, label: &str) {
        let at = self.player();
        let want = dereth_physics::math::localtolocal(&at, truth, Vec3::ZERO);
        let want_distance = dereth_physics::math::distance(truth, &at);

        let sel = self.selection(id);
        assert!(
            gap(sel, want) < 1e-3,
            "{label}: selection says player-space {sel:?}, the body is at {want:?} ({} m apart)",
            gap(sel, want)
        );

        let watch = self.watch(id);
        assert!(
            (watch - want_distance).abs() < 1e-3,
            "{label}: the range watch says {watch} m, the body is {want_distance} m away"
        );

        let radar = self.radar(id);
        assert!(
            gap(radar, want) < 1e-3,
            "{label}: the radar blip is at player-space {radar:?}, the body is at {want:?} \
             ({} m apart)",
            gap(radar, want)
        );

        let drawn = self.drawn(id);
        assert_eq!(drawn.cell, truth.cell, "{label}: the drawn cell");
        let d = gap(drawn.frame.origin, truth.frame.origin);
        assert!(
            d < 1e-3,
            "{label}: the draw is at {drawn:?}, the body is {d} m away"
        );
    }
}

/// A settled mover 2 m from the player, and the target 3 m beyond it.
fn one_mover() -> (Bench, Position) {
    let mut b = Bench::new();
    let start = b.ground_at(2.0, 0.0);
    b.create(MOVER, &start);
    // The create's enter-world placement drops the body onto the terrain;
    // let it settle so the walk below starts from a body at rest.
    for _ in 0..60 {
        b.frame();
    }
    let target = b.ground_at(2.0 + WALK, 0.0);
    (b, target)
}

// =================================================================================================
// 1. Mid-flight: every reader is the body, and the wire position is only a target
// =================================================================================================

/// Behaviour: objects.position.every-reader-sees-a-remote-objects-one-position
/// An ordinary `0xF748` within 96 m queues an interpolated walk; a couple of frames
/// later the body is part of the way along it. Selection, the range watch, the radar blip and the
/// drawn position must all say where the **body** is, and none of them may say where the server
/// said it would be.
#[test]
fn mid_walk_every_reader_is_the_body_and_not_the_wire_target() {
    let (mut b, target) = one_mover();

    let before = b.objects.physics.stats.interpolate_arm;
    b.position_event(MOVER, &target, true);
    assert_eq!(
        b.objects.physics.stats.interpolate_arm,
        before + 1,
        "premise: the movement handler selected the interpolation arm"
    );
    assert_eq!(
        b.objects.physics.stats.teleport_arm, 0,
        "no teleport stamp was sent"
    );
    assert_eq!(
        b.objects.physics.stats.no_contact_arm, 0,
        "the contact bit was set"
    );

    // The update moves nothing immediately because it only queues an interpolation node; two
    // physics frames then walk the body part of the way.
    b.frame();
    b.frame();

    let body = b.body(MOVER);
    let left = gap(body.frame.origin, target.frame.origin);
    assert!(
        left > 0.5,
        "the station has to be *mid*-flight: the body is {left} m from the target"
    );
    assert!(
        left < WALK - 0.1,
        "and the body has to have moved: it is still {left} m from the target"
    );

    b.assert_everything_agrees_with(MOVER, &body, "mid-walk");
}

/// And the negative: once the walk is over, every reader agrees with the target too — which is
/// what makes the assertion above a statement about *when*, and not a tolerance.
#[test]
fn once_the_walk_completes_every_reader_agrees_with_the_target_as_well() {
    let (mut b, target) = one_mover();
    b.position_event(MOVER, &target, true);

    // 3 m at the 7.5 m/s fallback is a dozen sub-steps; 150 is room to spare, and
    // the node is asserted completed rather than waited on blindly.
    for _ in 0..150 {
        b.frame();
    }
    let h = b.objects.physics.handle(MOVER).expect("the body exists");
    assert!(
        !b.scene
            .character
            .as_ref()
            .expect("a body")
            .world
            .is_interpolating(h),
        "the walk has to be over before this test means anything"
    );

    let body = b.body(MOVER);
    let d = gap(body.frame.origin, target.frame.origin);
    assert!(
        d < CLOSE_ENOUGH * 4.0,
        "the body stopped {d} m from the target, which is no arrival"
    );
    b.assert_everything_agrees_with(MOVER, &body, "after the walk");
}

// =================================================================================================
// 2. The `!contact` arm — the body is not moved at all, so neither is anything that reads it
// =================================================================================================

/// Behaviour: movement.correction.an-update-that-does-not-say-the-body-is-on-the-ground-moves-it-nowhere
/// The original movement handler returns false for a pack with no `has_contact` bit, after which
/// the outer position handler does **nothing at all**: however far the update names, the body
/// does not move, and every reader must stay on the body.
#[test]
fn an_update_with_no_contact_bit_moves_nothing_and_no_reader_follows_it() {
    let (mut b, target) = one_mover();
    let before = b.body(MOVER);

    b.position_event(MOVER, &target, false);
    assert_eq!(
        b.objects.physics.stats.no_contact_arm, 1,
        "premise: the movement handler took the no-contact arm"
    );
    assert_eq!(
        b.objects.physics.stats.interpolate_arm, 0,
        "and queued no walk"
    );
    b.frame();

    let body = b.body(MOVER);
    let moved = gap(body.frame.origin, before.frame.origin);
    assert!(
        moved < 1e-2,
        "the body must not have been repositioned, and it moved {moved} m"
    );
    let asked = gap(target.frame.origin, before.frame.origin);
    assert!(
        asked > 2.0,
        "premise: the refused update named a position {asked} m away"
    );

    b.assert_everything_agrees_with(MOVER, &body, "the refused update");
}
