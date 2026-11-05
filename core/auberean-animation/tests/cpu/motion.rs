//! The locomotion sliders a body's motion steers.
//! Fixture: a body put together from a one-bone made-up setup with no animations; its sliders
//! are stepped by hand.

use std::sync::Arc;

use auberean_animation::body::slider;
use auberean_animation::{Animations, Body, BodyRecords};
use auberean_assets::animation::{AnimMap, Animation, BehaviorTable};
use auberean_assets::setup::{Bone, Setup};
use auberean_assets::visual::{Appearance, VisualDesc};
use auberean_primitives::{DataId, Vec3};
use auberean_world::look::{Group, Look, LookRecords};

const SETUP: u32 = 0x0400_0001;

struct Records(Arc<Setup>, f32);

impl Animations for Records {
    fn animation(&self, _id: DataId) -> Option<Arc<Animation>> {
        None
    }
}

impl LookRecords for Records {
    fn visual(&self, _id: DataId) -> Option<Arc<VisualDesc>> {
        None
    }
    fn appearance(&self, _id: DataId) -> Option<Arc<Appearance>> {
        None
    }
}

impl BodyRecords for Records {
    fn setup(&self, id: DataId) -> Option<Arc<Setup>> {
        (id.0 == SETUP).then(|| self.0.clone())
    }
    fn wind(&self) -> f32 {
        self.1
    }
    fn anim_map(&self, _id: DataId) -> Option<Arc<AnimMap>> {
        None
    }
    fn behavior_table(&self, _id: DataId) -> Option<Arc<BehaviorTable>> {
        None
    }
}

fn body() -> Body {
    let setup = Setup {
        bones: vec![Bone {
            parent: u32::MAX,
            id: 1,
        }],
        ..Setup::default()
    };
    let records = Records(Arc::new(setup), 0.0);
    let mut look = Look::default();
    look.add_group(&Group {
        key: 1,
        setup: DataId(SETUP),
        ..Group::default()
    })
    .expect("the group");
    Body::from_look(&records, &look).expect("the body")
}

/// A body drawn at 0.95 of its size, running at 6.5 m/s with 0.5 m/s to the side, has its forward
/// slider go to 6.5 / 0.95 and its sideways slider to 0.5 / 0.95: its legs cycle as a full-sized
/// body's would at a speed that much higher, so they cover the ground it covers.
///
/// Behaviour: animation.body.a-smaller-bodys-legs-cycle-faster-at-one-speed
#[test]
fn a_smaller_bodys_legs_cycle_faster_at_one_speed() {
    let mut b = body();
    let size = Vec3::new(0.95, 0.95, 0.95);
    b.set_motion(Vec3::new(0.5, 6.5, 0.0), 0.0, size);
    b.sliders.update(1.0);
    let forward = b.sliders.get(slider::FORWARD);
    let sideways = b.sliders.get(slider::SIDEWAYS);
    assert!((forward - 6.5 / 0.95).abs() < 1e-4, "forward {forward}");
    assert!((sideways - 0.5 / 0.95).abs() < 1e-4, "sideways {sideways}");
}

/// A slider standing just past still (under 0.05 m/s) toward the side it is told to go moves there
/// in 0.05 s however far that is; from still, or across a change of more than 0.75 m/s otherwise,
/// it takes 0.3 s; a small change takes 0.1 s.
///
/// Behaviour: animation.body.a-slider-just-past-still-moves-at-once
#[test]
fn a_slider_just_past_still_moves_at_once() {
    let one = Vec3::new(1.0, 1.0, 1.0);
    let mut b = body();
    b.sliders.set(slider::FORWARD, 0.02);
    b.set_motion(Vec3::new(0.0, 6.5, 0.0), 0.0, one);
    b.sliders.update(0.05);
    let f = b.sliders.get(slider::FORWARD);
    assert!((f - 6.5).abs() < 1e-4, "from just past still: {f}");

    let mut b = body();
    b.set_motion(Vec3::new(0.0, 6.5, 0.0), 0.0, one);
    b.sliders.update(0.15);
    let f = b.sliders.get(slider::FORWARD);
    assert!(
        (f - 3.25).abs() < 1e-3,
        "from still, halfway in 0.15 s: {f}"
    );

    let mut b = body();
    b.sliders.set(slider::FORWARD, 6.0);
    b.set_motion(Vec3::new(0.0, 6.5, 0.0), 0.0, one);
    b.sliders.update(0.05);
    let f = b.sliders.get(slider::FORWARD);
    assert!(
        (f - 6.25).abs() < 1e-3,
        "a small change, halfway in 0.05 s: {f}"
    );
}

/// Each body's wind slider follows how hard the wind blows, reaching a new strength over a second.
///
/// Behaviour: animation.body.its-wind-slider-follows-the-wind-over-a-second
#[test]
fn its_wind_slider_follows_the_wind_over_a_second() {
    let mut b = body();
    let blowing = Records(Arc::new(Setup::default()), 4.0);
    b.advance(&blowing, 0.5);
    assert!((b.sliders.current(slider::WIND) - 2.0).abs() < 1e-4);
    b.advance(&blowing, 0.5);
    assert!((b.sliders.current(slider::WIND) - 4.0).abs() < 1e-4);
    b.advance(&blowing, 0.5);
    assert!((b.sliders.current(slider::WIND) - 4.0).abs() < 1e-4);
    let still = Records(Arc::new(Setup::default()), 0.0);
    b.advance(&still, 0.25);
    assert!((b.sliders.current(slider::WIND) - 3.0).abs() < 1e-4);
}
