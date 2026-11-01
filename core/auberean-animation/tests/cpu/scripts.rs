//! Effect scripts played on their own, on a made-up script.
//! Fixture: a two-second script with an effect at a quarter of the way, another at three quarters
//! and a push halfway; every effect certain and of full intensity.

use std::sync::Arc;

use auberean_animation::scripts::ScriptTracker;
use auberean_animation::triggers::FrameTriggers;
use auberean_animation::{Animations, BodyRecords};
use auberean_assets::animation::{AnimMap, Animation, BehaviorTable};
use auberean_assets::fx::{FxScript, ImpulseTrigger, Trigger};
use auberean_assets::setup::Setup;
use auberean_assets::visual::{Appearance, VisualDesc};
use auberean_primitives::{DataId, Vec3};
use auberean_world::chargen::Random;
use auberean_world::look::LookRecords;

const SCRIPT: u32 = 0x3300_0001;
const FIRST: u32 = 0x10;
const SECOND: u32 = 0x11;

struct Records(Arc<FxScript>);

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
    fn setup(&self, _id: DataId) -> Option<Arc<Setup>> {
        None
    }
    fn anim_map(&self, _id: DataId) -> Option<Arc<AnimMap>> {
        None
    }
    fn behavior_table(&self, _id: DataId) -> Option<Arc<BehaviorTable>> {
        None
    }
    fn fx_script(&self, id: DataId) -> Option<Arc<FxScript>> {
        (id.0 == SCRIPT).then(|| self.0.clone())
    }
}

fn effect(name: u32, time: f32) -> Trigger {
    Trigger {
        name,
        time,
        stop: false,
        probability: 1.0,
        intensity: 1.0,
        intensity_variance: 0.0,
        play: true,
    }
}

/// A script plays from its start to its end over its period: its effects start as it passes
/// them and its pushes fire once; at its end it stops every effect it started, and once past its
/// end it is dropped. A script not in the data is refused.
///
/// Behaviour: animation.scripts.a-script-plays-once-over-its-period
#[test]
fn a_script_plays_once_over_its_period() {
    let records = Records(Arc::new(FxScript {
        triggers: vec![effect(FIRST, 0.25), effect(SECOND, 0.75)],
        impulses: vec![ImpulseTrigger {
            jump: false,
            impulse: Some(Vec3::new(0.0, 0.0, 2.0)),
            number: 0,
            time: 0.5,
        }],
        period: 2.0,
    }));
    let mut tracker = ScriptTracker::default();
    let mut fx = FrameTriggers::default();
    let mut dice = Random::default();
    assert!(!tracker.do_script(&records, SCRIPT + 1), "not in the data");
    assert!(tracker.do_script(&records, SCRIPT));

    tracker.update(0.5, &records, &mut fx, &mut dice);
    assert!(fx.starting.get(FIRST).is_some() && fx.starting.get(SECOND).is_none());
    assert!(fx.impulses.is_empty());
    fx.clean_up();

    tracker.update(1.0, &records, &mut fx, &mut dice);
    assert!(fx.starting.get(SECOND).is_some());
    assert_eq!(fx.impulses.len(), 1, "the push fires once");
    fx.clean_up();

    tracker.update(0.5, &records, &mut fx, &mut dice);
    assert!(fx.stopping.get(FIRST).is_some() && fx.stopping.get(SECOND).is_some());
    assert!(fx.impulses.is_empty(), "and only once");
    assert_eq!(tracker.tracks.len(), 1, "at its end, not yet past it");
    fx.clean_up();

    tracker.update(0.1, &records, &mut fx, &mut dice);
    assert!(tracker.tracks.is_empty());
}
