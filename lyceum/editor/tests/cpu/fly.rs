//! Behaviour: none (tooling: the world editor's flying camera over the retail dats)
//! With Collision on, the flying camera flown low over Holtburg's slopes, level or diving
//! gently into the ground, at fixed frame times from 60 frames a second to four: each frame ends
//! where its flight took it, slid along the ground or stepped up a little, and the camera flies
//! on over the slopes and across landblock edges. No frame throws it along the ground to
//! somewhere far off, and none holds it at an edge.
//! Fixture: the retail dats in `$DERETH_TEST_DAT_DIR`, from the middle of Holtburg's landblock,
//! south, south-west and east, over the town's slopes and into the landblocks round it.

use std::collections::BTreeSet;
use std::sync::Arc;

use dereth_assets::{Decode, Region};
use dereth_client_runtime::scene::SceneConfig;
use dereth_client_runtime::world_build;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::frame::V3;
use dereth_primitives::{DataId, Vec3};
use lyceum_editor::app::{Collider, FLY_RADIUS};
use lyceum_editor::camera::{landblock_of, FlyCamera};

const HOLTBURG: u16 = 0xA9B4;

/// How far each flight goes, in meters.
const FAR: f32 = 250.0;

/// More than a frame's flight a step up may lift the camera: a walking body's step height and
/// a little.
const STEP_UP: f32 = 0.7;

fn region(store: &RetailDatStore) -> Region {
    let id = DataId(0x1300_0000);
    let bytes = store.read_typed(DbType::Region, id).expect("the region");
    Region::decode_payload(id, &bytes).expect("the region decodes")
}

/// The ground's height under world meters `p`.
fn ground(store: &RetailDatStore, region: &Region, p: Vec3) -> f32 {
    let (lb, x, y) = landblock_of(p).expect("in the world");
    let t = world_build::read_landblock(store, i32::from(lb >> 8), i32::from(lb & 0xFF))
        .expect("the landblock");
    lyceum_foundry::compile::ground(&t, region, lb, x, y)
}

/// One flight: from the middle of Holtburg's landblock, the camera's sphere's underside `over`
/// meters over the ground, turned `yaw` and pitched `pitch` (degrees), flying `speed` meters a
/// second in frames of `dt` seconds.
struct Flight {
    speed: f32,
    dt: f32,
    pitch: f32,
    over: f32,
    yaw: f32,
}

const FLIGHTS: [Flight; 6] = [
    Flight {
        speed: 100.0,
        dt: 1.0 / 60.0,
        pitch: 0.0,
        over: 0.5,
        yaw: 180.0,
    },
    Flight {
        speed: 100.0,
        dt: 1.0 / 60.0,
        pitch: -10.0,
        over: 0.05,
        yaw: 135.0,
    },
    Flight {
        speed: 10.0,
        dt: 0.25,
        pitch: 0.0,
        over: 0.05,
        yaw: 135.0,
    },
    Flight {
        speed: 250.0,
        dt: 0.25,
        pitch: 0.0,
        over: 0.5,
        yaw: 180.0,
    },
    Flight {
        speed: 10.0,
        dt: 1.0 / 60.0,
        pitch: -10.0,
        over: 0.05,
        yaw: 270.0,
    },
    Flight {
        speed: 250.0,
        dt: 1.0 / 60.0,
        pitch: -10.0,
        over: 0.05,
        yaw: 270.0,
    },
];

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats")]
fn with_collide_on_the_flying_camera_flown_low_over_holtburgs_slopes_ends_each_frame_where_its_flight_took_it_and_is_never_thrown_far_off(
) {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let region = region(&store);
    let terrain =
        |b: u16| world_build::read_landblock(&store, i32::from(b >> 8), i32::from(b & 0xFF));
    let o = lyceum_foundry::pick::block_origin(HOLTBURG);
    let mid = Vec3::new(o.x + 96.0, o.y + 96.0, 0.0);
    let g = ground(&store, &region, mid);
    for f in &FLIGHTS {
        let name = format!(
            "{} m/s, {} s frames, pitched {}, {} m over the ground, turned {}",
            f.speed, f.dt, f.pitch, f.over, f.yaw
        );
        let mut c = Collider::new(&store, &region, &SceneConfig::default(), BTreeSet::new())
            .expect("the collider");
        let start = Vec3::new(mid.x, mid.y, g + FLY_RADIUS + f.over);
        let mut cam = FlyCamera::new(start, f.yaw.to_radians(), f.pitch.to_radians());
        let step = cam.forward().mul(f.speed * f.dt);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = (FAR / (f.speed * f.dt)).ceil() as u32;
        let level = |v: Vec3| Vec3::new(v.x, v.y, 0.0).magnitude();
        let mut flown = 0.0;
        for i in 0..frames {
            let from = cam.position;
            let to = from.add(step);
            let (lb, ..) = landblock_of(from).expect("in the world");
            c.reach(&store, &region, &terrain, lb);
            let here = c.sweep(from, to, None);
            let flight = step.magnitude();
            let moved = here.sub(from).magnitude();
            assert!(
                moved <= flight + STEP_UP,
                "{name}: frame {i} flew {flight} m and was moved {moved} m, from {from:?} to {here:?}"
            );
            assert!(
                here.sub(to).magnitude() <= flight + STEP_UP,
                "{name}: frame {i} ended {} m from where it flew to, {to:?}: {here:?}",
                here.sub(to).magnitude()
            );
            cam.position = here;
            flown += level(step);
        }
        assert_eq!(c.thrown, 0, "{name}: no sweep is thrown far off");
        // It flew on: most of its flight, less what the slopes took into climbing them.
        let went = level(cam.position.sub(start));
        assert!(
            went > 0.9 * flown,
            "{name}: it went {went} m of the {flown} m it flew over the ground, to {:?}",
            cam.position
        );
    }
}
