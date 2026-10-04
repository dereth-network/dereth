//! An object placed below the terrain settles on it, however deep the request, and an object placed
//! above it stays where it was asked to be; the clamp does not depend on the ethereal state bit.
//!
//! This is retail's placement rule. The land-cell collision finds the terrain polygon under the
//! sphere's low point (no z gate) and, when `d = plane . (centre - (0,0,r)) + waterDepth` is below
//! `-0.0002`, lifts the checked position by `d / N.z`, unbounded. Placement with sliding accepts
//! the adjusted position; the non-sliding comparison checks x, y and cell but not z. Only hook,
//! storage and corpse objects take the wire z verbatim.
//!
//! Fixture: a never-degraded setup (`0x0200_004D`) placed through an encoded `ITEM_CREATE_OBJECT`
//! body (no socket) 300 m out on the Holtburg hill, on the retail dats and a software device. It
//! reads the settled physics position, not pixels.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::character::{CharacterInput, PLAYER_OBJECT_ID};
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_client::world::{SceneReads, SceneWrites};
use dereth_client_model::weenie::{bitfield, item_type};
use dereth_client_net::client_session::SessionEvent;
use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_primitives::{LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};
use dereth_render::device::Gpu;
use std::sync::Arc;

const W: u32 = 800;
const H: u32 = 600;
/// Holtburg.
const HOLTBURG: u16 = 0xA9B4;
/// An outdoor Holtburg station, block-local, the one `rendering::landscape_draw_distance` uses.
#[allow(clippy::excessive_precision)]
const STATION: (f32, f32) = (130.097_656, 24.053_997);
const TARGET: ObjectId = ObjectId(0x8300_0021);
/// A long-reach setup: 18 parts and no degrade terminator, so the object never goes dark at range
/// and a reading here is the placement rather than the degrade cutoff.
const NEVER_DARK: u32 = 0x0200_004D;
/// `ETHEREAL_PS`.
const ETHEREAL_PS: u32 = 0x0000_0004;
/// The bearing from the station up the hill.
const BEARING_DEGREES: f32 = 17.5;
/// Mid distance on the Holtburg hill.
const MID_DISTANCE: f32 = 300.0;

fn outdoor(x: f32, y: f32, z: f32) -> Position {
    let mut cell = LandblockId(HOLTBURG).cell(1);
    let mut o = Vec3::new(x, y, z);
    dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut o);
    Position::new(cell, dereth_primitives::Frame::new(o, Quat::IDENTITY))
}

struct Bench {
    gpu: Gpu,
    scene: WorldScene,
    store: Arc<RetailDatStore>,
    objects: ObjectStream,
    now: f64,
    instance: u16,
}

impl Bench {
    fn new(store: &Arc<RetailDatStore>, mut gpu: Gpu, land_radius: u32) -> Self {
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        let cfg = SceneConfig {
            landblock: HOLTBURG,
            land_radius,
            time_of_day: Some(0.78),
            particles: false,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
        scene
            .attach_character(store, &region, &mut gpu)
            .expect("the body is created");
        let mut objects = ObjectStream::new();
        objects.world.player = Some(PLAYER_OBJECT_ID);
        let mut me = dereth_client_model::Weenie::new(PLAYER_OBJECT_ID);
        me.valid = true;
        me.has_phys_obj = true;
        me.qualities = Some(dereth_client_model::qualities::Qualities::new());
        objects.world.tables.weenies.insert(PLAYER_OBJECT_ID, me);
        Self {
            gpu,
            scene,
            store: store.clone(),
            objects,
            now: 1.0,
            instance: 0,
        }
    }

    /// The landblock terrain triangle height at (x, y), through the same production lookup the
    /// placement clamp uses.
    fn height(&self, x: f32, y: f32) -> Option<f32> {
        self.scene
            .character
            .as_ref()?
            .world
            .terrain_height_at(&outdoor(x, y, 0.0))
    }

    fn stand_at_the_station(&mut self) {
        let z = self
            .height(STATION.0, STATION.1)
            .expect("the station is on terrain");
        let mut p = outdoor(STATION.0, STATION.1, z + 0.01);
        p.frame.rotation = Quat::IDENTITY; // yaw 0 = north
        self.scene.character.as_mut().expect("a body").teleport(p);
    }

    /// The point on the bearing this file measures at.
    fn target_xy(distance: f32) -> (f32, f32) {
        let (s, c) = math::sin_cosf(BEARING_DEGREES.to_radians());
        (STATION.0 + s * distance, STATION.1 + c * distance)
    }

    /// Feed a synthetic encoded create body through the application's object stream, then mark
    /// the resulting record as a visible attackable creature for scene synchronization. This is
    /// direct fixture construction rather than a network send or 3D-pick producer.
    /// `scale` is the `PhysicsDesc.object_scale` carried by that body.
    fn place(&mut self, at: Position, scale: f32, state: u32) {
        self.instance += 1;
        let payload = dereth_protocol::objects::ObjectCreatePayload {
            id: TARGET,
            objdesc: ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::POSITION | flags::SETUP | flags::OBJSCALE,
                object_scale: Some(scale),
                setup_id: Some(NEVER_DARK),
                state,
                position: Some(dereth_protocol::types::PositionWire {
                    objcell_id: at.cell.0,
                    frame: dereth_protocol::types::Frame {
                        origin: at.frame.origin.into(),
                        orientation: Quat::IDENTITY.into(),
                    },
                }),
                timestamps: dereth_protocol::types::PhysicsTimestamps {
                    instance: self.instance,
                    ..dereth_protocol::types::PhysicsTimestamps::default()
                },
                ..PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc::default(),
        };
        let body =
            dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload))
                .expect("encode");
        self.objects.apply_event(
            &SessionEvent::WorldObject {
                opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
                body,
            },
            LocalTime(self.now),
        );
        {
            let Self {
                store,
                objects,
                scene,
                ..
            } = self;
            let ch = scene.character.as_mut().expect("a body");
            objects.sync_physics(store, &mut ch.world);
        }
        let w = self
            .objects
            .world
            .tables
            .weenies
            .get_mut(TARGET)
            .expect("placed");
        w.pwd.obj_type |= item_type::CREATURE;
        w.pwd.bitfield |= bitfield::ATTACKABLE;
        w.pwd.radar_enum = Some(4);
        self.objects.world.update_visible_object_list();
    }

    /// The physics position at which the synchronized body settled.
    fn settled(&self) -> Option<Position> {
        let w = &self.scene.character.as_ref()?.world;
        let h = w.by_object_id(TARGET)?;
        w.get(h).map(|o| o.position)
    }

    fn draw(&mut self) {
        self.now += 1.0;
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
            CharacterInput::default(),
            LocalTime(self.now),
            1.0 / 30.0,
        );
        scene.stream(store, gpu).expect("stream");
        scene.reserve_upload_arena(gpu).expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
    }
}

/// A bench standing at the station with the hill in front of it, and the ground height at
/// the point this file measures.
fn bench(store: &Arc<RetailDatStore>) -> Option<(Bench, f32, f32, f32)> {
    let gpu = crate::common::test_gpu(W, H);
    let mut b = Bench::new(store, gpu, 8);
    b.stand_at_the_station();
    for _ in 0..12 {
        b.draw();
    }
    let (x, y) = Bench::target_xy(MID_DISTANCE);
    let ground = b.height(x, y).expect("the bearing has ground under it");
    Some((b, x, y, ground))
}

/// Behaviour: world.physics-script.a-body-walking-at-a-scaled-object-stops-at-the-scaled-distance
///
/// Placements requested at 0, 2, 60 and 2,000 metres below the terrain settle at the same z bits
/// as the on-ground request. The above-ground control must differ by more than 30 metres; the
/// test does not require an exact +40-metre result.
#[test]
fn a_downward_placement_is_clamped_to_the_surface_and_an_upward_one_is_honoured() {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let Some((mut b, x, y, ground)) = bench(&store) else {
        return;
    };

    let mut settled = Vec::new();
    for depth in [0.0f32, 2.0, 60.0, 2000.0, -40.0] {
        b.place(outdoor(x, y, ground - depth), 4.0, ETHEREAL_PS);
        for _ in 0..3 {
            b.draw();
        }
        let p = b.settled().expect("the object was placed at all");
        eprintln!(
            "placement clamp, Holtburg hill at {MID_DISTANCE:.0} m, terrain {ground:.5}: asked for z {:.5} \
             ({depth:7.1} m of burial) -> settled {:#010X} z {:.5}",
            ground - depth,
            p.cell.0,
            p.frame.origin.z
        );
        settled.push(p.frame.origin.z);
    }

    // **The positive control first.** Lifting the object 40 m must move it, or the equalities
    // below are a blind instrument: they would pass just as well against a `settled()` that never
    // changed at all.
    let on_ground = settled[0];
    let lifted = settled[4];
    assert!(
        (lifted - on_ground).abs() > 30.0,
        "lifting the object 40 m moved its settled z by only {:.4} m, so this instrument cannot \
         see the object move and proves nothing about a downward placement",
        (lifted - on_ground).abs()
    );
    // Retail's walkable-position lift is unbounded, so 2 m, 60 m and 2 000 m of burial all end
    // in the same place: on the terrain triangle, bit for bit, because the lift is one `d / N.z`
    // and lands on the plane exactly.
    for (depth, z) in [
        (2.0f32, settled[1]),
        (60.0, settled[2]),
        (2000.0, settled[3]),
    ] {
        assert_eq!(
            z.to_bits(),
            on_ground.to_bits(),
            "a request to bury the object {depth} m settled at {z} rather than {on_ground}, where an \
             on-ground placement settles. If a downward z is honoured now, the original \
             walkable-position rule `lift = d / N.z` and its unbounded vertical offset have \
             changed. Read that rule before relaxing this: the clamp is original behavior"
        );
    }
    // And it really is the terrain it landed on, rather than an arbitrary z the transition
    // invented: the settled point is within a sphere radius of the terrain triangle under that
    // (x, y).
    assert!(
        (on_ground - ground).abs() < 2.0,
        "an on-ground placement settled at {on_ground} where the terrain triangle at ({x:.2}, \
         {y:.2}) is {ground} — {:.3} m apart, so the clamp is not landing on the terrain and this \
         whole reading is about something else",
        (on_ground - ground).abs()
    );
}

/// The `ETHEREAL_PS` half is separate because physics-state bit 4 supplied here means ethereality
/// and gates object collisions. Retail's terrain collision separately tests bit 4 of its
/// collision-info state as the viewer flag; those are different state domains. The station places
/// the same body 60 metres below terrain with physics state 0 and state 4 and compares the settled
/// z bits.
#[test]
fn the_terrain_clamp_is_not_gated_on_ethereality() {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let Some((mut b, x, y, ground)) = bench(&store) else {
        return;
    };
    let mut z = Vec::new();
    for state in [0u32, ETHEREAL_PS] {
        b.place(outdoor(x, y, ground - 60.0), 4.0, state);
        for _ in 0..3 {
            b.draw();
        }
        let p = b.settled().expect("the object was placed at all");
        eprintln!(
            "placement clamp, state {state:#06x}: 60 m of burial settled at z {:.5}",
            p.frame.origin.z
        );
        z.push(p.frame.origin.z);
    }
    assert_eq!(
        z[0].to_bits(),
        z[1].to_bits(),
        "the same burial settled at {} with state 0 and {} with ethereality state bit 4; \
         terrain adjustment should not depend on that ethereality bit",
        z[0],
        z[1]
    );
    assert!(
        (z[0] - ground).abs() < 2.0,
        "neither arm reached the terrain at all ({} against a terrain height of {ground}), so \
         this test is comparing two failures rather than two clamps",
        z[0]
    );
}
