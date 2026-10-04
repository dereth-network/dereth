//! A walking body stops at a ledge instead of walking off it: when the step down finds no walkable
//! ground, edge sliding undoes the sub-step and the body keeps its contact. Clearing the body's
//! edge-slide bit is retail's one exemption, and that body walks straight off.
//! Fixture: the retail dats' Holtburg block `0xA9B4`, a local body placed on a static ledge in land
//! cell `0xA9B4_0022` three and a half metres above the terrain, on a headless software GPU. No
//! socket is opened.
//!
//! # What the original client does at a ledge while walking
//!
//! A walking body that is `CONTACT | ON_WALKABLE` and finds nothing under its feet enters the
//! ground-transition arm:
//!
//! * The walkable allowance is `floor_z`, initialized as `cos(3437.746770784939)` and stored as
//!   **0.664 174 15** (`f32` `0x3F2A0751`), a **48.381°** slope limit. A surface is walkable when
//!   `normal.z >= floor_z`, with no tolerance.
//! * The initial step-down amount is the setup record's `step_down_height` times the object's
//!   scale. When a single-sphere path has a smaller diameter, the amount becomes `radius * 0.5`;
//!   when it still exceeds the diameter, the transition uses two half-sized probes.
//! * The step-down path drops the sphere by that amount and re-inserts it. It succeeds only when
//!   the landing plane is walkable and the ground there is more than a sliver.
//! * When step-down fails, edge sliding chooses among five arms. Only a body missing either
//!   `ON_WALKABLE (0x2)` or `EDGE_SLIDE (0x200)` takes the first arm, reports `OK_TS`, and walks
//!   off. The other outcomes cover a slope that is too steep, sliding along the walkable polygon's
//!   edge, a surviving contact plane, and **`COLLIDED_TS`** for a landscape ledge. Terrain reaches
//!   that last arm because it never marks the sphere path walkable.
//!
//! **The edge-slide result is carried through an out-parameter that aliases the transition loop's
//! running state.** If edge sliding does not finish directly, the loop reloads that state, carries
//! it through the loop tail, and returns it from either exit.
//!
//! Transition validation then takes the non-`OK_TS` arm for that `SLID_TS`: it stops velocity,
//! restores the **last known** contact plane when the body is still within
//! `radius + F_EPSILON`, puts the check position back at `curr_pos`, and only then reports
//! `OK_TS`. The sub-step is undone and **contact is kept**. That is the whole of "you cannot walk
//! off a ledge, you have to jump".
//!
//! If `transitional_insert` dropped `edge_slide`'s state and returned `OK_TS` for a sub-step in
//! which `edge_slide` had already cleared `contact_plane_valid`, position finalization would clear
//! `CONTACT_TS` and `ON_WALKABLE_TS`, the next frame would skip the ground arm, and the body would
//! fall: that is the failure these tests reject.
//!
//! The ledge here is a **static object**, not terrain, so the arm that holds the body is
//! `precipice_slide`'s and not the landscape's `COLLIDED_TS`. These fixtures do not execute every
//! arm or the jump path: the positive test exercises the static ledge and its precipice and
//! validation result, and the negative test clears the edge-slide bit to exercise the walk-off
//! exemption.
//!
//! **Fails** when the retail dats are absent or no headless software GPU device can be made.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::sync::Arc;

use dereth_client::character::{CharacterInput, PLAYER_OBJECT_ID};
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LocalTime, Position, Quat, Vec3};
use dereth_render::device::Gpu;

const W: u32 = 320;
const H: u32 = 240;
/// Holtburg, the ledge's block.
const HOLTBURG: u16 = 0xA9B4;

/// The ledge location: a body standing on a static object at the lip of a drop.
const LEDGE_CELL: u32 = 0xA9B4_0022;
#[allow(clippy::excessive_precision)]
const LEDGE_ORIGIN: Vec3 = Vec3::new(110.202_118, 42.349_392, 97.505_005);
#[allow(clippy::excessive_precision)]
const LEDGE_ROTATION: Quat = Quat::new(-0.857_244, 0.0, 0.0, 0.514_911);

/// How many frames the walk runs for. The body reaches the edge on frame 38 of a 30 Hz headless
/// run, so this is three times the distance to it.
const FRAMES: usize = 120;

/// The tolerance on "the body did not fall". The ledge is 3.5 m tall; a millimetre is three and a
/// half orders below that and the settled body's z does not move at all.
const NO_FALL: f32 = 0.001;

/// One frame's reading of position and footing. `contact` is `CONTACT_TS` and `walkable` is
/// `ON_WALKABLE_TS`: position finalization derives both from the transition's contact plane, and
/// the ground-transition arm requires contact on the following frame.
#[derive(Debug, Clone, Copy)]
struct Sample {
    origin: Vec3,
    cell: u32,
    contact: bool,
    walkable: bool,
}

struct Bench {
    gpu: Gpu,
    scene: WorldScene,
    store: Arc<RetailDatStore>,
    objects: ObjectStream,
    now: f64,
}

impl Bench {
    fn new(store: &Arc<RetailDatStore>, mut gpu: Gpu) -> Self {
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        let cfg = SceneConfig {
            landblock: HOLTBURG,
            land_radius: 2,
            particles: false,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
        scene
            .attach_character(store, &region, &mut gpu)
            .expect("the body is created");
        let mut objects = ObjectStream::new();
        objects.world.player = Some(PLAYER_OBJECT_ID);
        Self {
            gpu,
            scene,
            store: store.clone(),
            objects,
            now: 1.0,
        }
    }

    /// Put the body on the ledge, facing the drop.
    fn stand_at_the_ledge(&mut self) {
        let p = Position::new(CellId(LEDGE_CELL), Frame::new(LEDGE_ORIGIN, LEDGE_ROTATION));
        self.scene.character.as_mut().expect("a body").teleport(p);
    }

    fn step(&mut self, input: CharacterInput) -> Sample {
        self.now += 1.0 / 30.0;
        let Self {
            store,
            gpu,
            scene,
            objects,
            now,
        } = self;
        scene
            .sync_objects(store, gpu, objects)
            .expect("sync_objects");
        scene.update(
            dereth_client::camera::CameraInput::default(),
            input,
            LocalTime(*now),
            1.0 / 30.0,
        );
        let c = scene.character.as_ref().expect("a body");
        let o = c.world.get(c.handle).expect("the body is in the world");
        Sample {
            origin: c.position().frame.origin,
            cell: c.position().cell.0,
            contact: o.transient_state.in_contact(),
            walkable: o.transient_state.on_walkable(),
        }
    }

    fn settle(&mut self) -> Sample {
        let mut s = self.step(CharacterInput::default());
        for _ in 0..5 {
            s = self.step(CharacterInput::default());
        }
        s
    }

    fn walk(&mut self, frames: usize) -> Vec<Sample> {
        let input = CharacterInput {
            forward: true,
            ..CharacterInput::default()
        };
        (0..frames).map(|_| self.step(input)).collect()
    }

    /// The terrain triangle height at a block-local `(x, y)`.
    fn terrain(&self, x: f32, y: f32) -> Option<f32> {
        let mut cell = dereth_primitives::LandblockId(HOLTBURG).cell(1);
        let mut o = Vec3::new(x, y, 0.0);
        dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut o);
        let p = Position::new(cell, Frame::new(o, Quat::IDENTITY));
        self.scene.character.as_ref()?.world.terrain_height_at(&p)
    }
}

fn bench(store: &Arc<RetailDatStore>) -> Bench {
    let gpu = crate::common::test_gpu(W, H);
    let mut b = Bench::new(store, gpu);
    b.stand_at_the_ledge();
    b
}

/// The calibration premise: the terrain triangle under the ledge is at about 94.0 and the body
/// stands at 97.505. The settled body stays within one millimetre of the placed origin and more
/// than three metres above the terrain; without that, "the body's z did not change" could describe
/// flat ground and would prove nothing. Each test builds and settles its own bench.
///
/// It also reads the body's scaled step-down height and the shared walkable-slope threshold used
/// by the ledge rule rather than substituting fixture values.
#[test]
fn the_test_spot_is_a_static_ledge_with_three_and_a_half_metres_of_air_in_front_of_it() {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let mut b = bench(&store);
    let start = b.settle();

    let ground = b
        .terrain(LEDGE_ORIGIN.x, LEDGE_ORIGIN.y)
        .expect("the ledge's cell has terrain");
    let (step_down, radius, scale) = {
        let c = b.scene.character.as_ref().expect("a body");
        let o = c.world.get(c.handle).expect("the body");
        (o.step_down_height(), o.radius(), o.scale)
    };
    eprintln!(
        "ledge location {LEDGE_CELL:#010X} {LEDGE_ORIGIN:?}: body settled at z {:.6} \
         (contact {}, on_walkable {}), terrain triangle under it {ground:.3} -- {:.3} m of air. \
         step_down_height {step_down:.6} (scale {scale:.3}), radius {radius:.6}, \
         floor_z {:.8} = {:.3} degrees",
        start.origin.z,
        start.contact,
        start.walkable,
        start.origin.z - ground,
        dereth_physics::globals::FLOOR_Z,
        math::acos(f64::from(dereth_physics::globals::FLOOR_Z)).to_degrees(),
    );

    // The body did not settle onto the terrain: it kept the z it was placed at.
    assert!(
        (start.origin.z - LEDGE_ORIGIN.z).abs() < NO_FALL,
        "the body must stand on the ledge, not fall to the terrain -- got {:.6}",
        start.origin.z
    );
    assert!(
        start.contact && start.walkable,
        "the body is standing on something walkable"
    );
    assert!(
        start.origin.z - ground > 3.0,
        "the ledge location must be well above the terrain for this file to be about a ledge at \
         all -- body {:.3}, terrain {ground:.3}",
        start.origin.z
    );
    // A surface is walkable when `normal.z >= floor_z`; the stored threshold is
    // `cos(3437.746770784939)` rounded to `f32`.
    assert_eq!(dereth_physics::globals::FLOOR_Z.to_bits(), 0x3F2A_0751);
}

/// Behaviour: movement.contact.a-walking-body-stops-at-a-ledge-instead-of-walking-off
/// Walk forward from the ledge location toward the drop and measure the two things `edge_slide`
/// decides: the body's z, and `CONTACT_TS | ON_WALKABLE_TS`.
///
/// The original client holds both: the precipice arm slides the body along the walkable polygon's
/// edge, and when even that cannot recover, the transition returns `SLID_TS`; validation restores
/// the last known contact plane and undoes the sub-step. A body that walked off would end on the
/// terrain about 3.5 m lower and be off the ground for many frames.
#[test]
fn walking_forward_off_a_static_ledge_keeps_contact_and_does_not_fall() {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let mut b = bench(&store);
    let start = b.settle();
    assert!(
        start.contact && start.walkable,
        "the walk has to start on the ledge"
    );

    let path = b.walk(FRAMES);
    let lowest = path
        .iter()
        .map(|s| s.origin.z)
        .fold(f32::INFINITY, f32::min);
    let airborne = path.iter().filter(|s| !(s.contact && s.walkable)).count();
    let last = path.last().expect("frames were walked");
    let travelled = math::hypotf(
        last.origin.x - start.origin.x,
        last.origin.y - start.origin.y,
    );
    eprintln!(
        "walk of {FRAMES} frames from {:?}: ended [{:.3} {:.3} {:.3}] cell {:#010X}, \
         travelled {travelled:.3} m, lowest z {lowest:.6}, {airborne} frame(s) off the ground",
        start.origin, last.origin.x, last.origin.y, last.origin.z, last.cell
    );

    // The instrument has to have been pointed at the ledge: a body that never moved would pass
    // "it did not fall" for the wrong reason.
    assert!(
        travelled > 1.0,
        "the body must actually have walked to the edge -- it moved {travelled:.3} m"
    );
    assert!(
        lowest > LEDGE_ORIGIN.z - NO_FALL,
        "retail does not let a walking body leave the ledge: the lowest z over {FRAMES} frames \
         was {lowest:.6}, {:.3} m below the start",
        LEDGE_ORIGIN.z - lowest
    );
    assert_eq!(
        airborne, 0,
        "CONTACT_TS | ON_WALKABLE_TS must hold on every frame of the walk"
    );
    assert_eq!(
        last.cell, LEDGE_CELL,
        "the body stayed in the ledge's own land cell"
    );
}

/// Retail's one exemption, and the proof that the two assertions above can fail.
///
/// Edge sliding first tests `ON_WALKABLE (0x2)` and `EDGE_SLIDE (0x200)`. A body missing either
/// takes the exemption that restores the check position, clears the contact plane, marks the cell
/// array valid, and writes `OK_TS`. The edge-slide state comes from `EDGE_SLIDE_PS (0x400000)`,
/// which the original body constructor includes in its default `0x400C08` state.
///
/// So clearing that one bit on the same body at the same place walks it straight off, which is
/// both a transcription of the exemption and the red this file needs to be able to show.
#[test]
fn the_same_body_with_edge_slide_ps_cleared_walks_straight_off() {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let mut b = bench(&store);
    let start = b.settle();
    assert!(
        start.contact && start.walkable,
        "the walk has to start on the ledge"
    );
    {
        let c = b.scene.character.as_mut().expect("a body");
        let h = c.handle;
        let o = c.world.get_mut(h).expect("the body");
        o.state.set_edge_slide(false);
        assert!(!o.state.can_edge_slide());
    }

    let path = b.walk(FRAMES);
    let lowest = path
        .iter()
        .map(|s| s.origin.z)
        .fold(f32::INFINITY, f32::min);
    let airborne = path.iter().filter(|s| !(s.contact && s.walkable)).count();
    let ground = b.terrain(LEDGE_ORIGIN.x, LEDGE_ORIGIN.y).expect("terrain");
    eprintln!(
        "EDGE_SLIDE_PS cleared: lowest z {lowest:.6} against a terrain triangle of \
         {ground:.3}, {airborne} frame(s) off the ground out of {FRAMES}"
    );

    assert!(
        lowest < LEDGE_ORIGIN.z - 3.0,
        "without EDGE_SLIDE_PS the body must leave the ledge -- lowest z was {lowest:.6}"
    );
    assert!(
        airborne > 5,
        "and it must be measurably airborne on the way down -- {airborne} frame(s)"
    );
}
