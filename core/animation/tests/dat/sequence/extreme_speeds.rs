//! A body the server tells to walk, turn or step at a speed no motion means (the received
//! movement's speeds are the wire's own floats) is still advanced: its animation's update returns,
//! forwards and backwards, and stands inside the animation it plays.
//! Fixture: the shipped retail DAT records.

use super::common;

use std::sync::Arc;

use dereth_animation::data::AnimAssets;
use dereth_animation::motion::InterpretedMotionState;
use dereth_animation::{MotionCommand, MotionDriver};
use dereth_primitives::{DataId, Frame};

const ALUVIAN_MALE_SETUP: DataId = DataId(0x0200_0001);
const ALUVIAN_MALE_MOTION_TABLE: DataId = DataId(0x0900_0001);

fn driver() -> MotionDriver {
    let assets = Arc::new(common::open());
    let setup = AnimAssets::setup(assets.as_ref(), ALUVIAN_MALE_SETUP).expect("the setup decodes");
    let mut d = MotionDriver::new(assets as Arc<dyn AnimAssets>);
    assert!(d.set_setup(setup), "the part array is built");
    assert!(
        d.set_motion_table(ALUVIAN_MALE_MOTION_TABLE),
        "the motion table loads"
    );
    d
}

/// Run `f` on a thread of its own; whether it returned within ten seconds, so that an update
/// that never returns fails the test rather than holding the suite.
fn returns(f: impl FnOnce() + Send + 'static) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        f();
        let _ = tx.send(());
    });
    rx.recv_timeout(std::time::Duration::from_secs(10)).is_ok()
}

#[test]
fn a_received_motion_at_any_speed_is_advanced_and_its_update_returns() {
    for speed in [1.0e20_f32, -1.0e20, 1.0e9, -0.65] {
        for (forward, sidestep, turn) in [
            (
                MotionCommand::WALK_FORWARD,
                MotionCommand::NONE,
                MotionCommand::NONE,
            ),
            (
                MotionCommand::READY,
                MotionCommand::SIDE_STEP_RIGHT,
                MotionCommand::NONE,
            ),
            (
                MotionCommand::READY,
                MotionCommand::NONE,
                MotionCommand::TURN_RIGHT,
            ),
        ] {
            let mut d = driver();
            d.move_to_interpreted_state(
                &InterpretedMotionState {
                    forward_command: forward,
                    forward_speed: speed,
                    sidestep_command: sidestep,
                    sidestep_speed: speed,
                    turn_command: turn,
                    turn_speed: speed,
                    ..InterpretedMotionState::default()
                },
                false,
            );
            let seq = d.sequence.clone();
            let ok = returns(move || {
                let mut seq = seq;
                let mut out = Vec::new();
                for _ in 0..30 {
                    seq.update(1.0 / 30.0, Some(&mut Frame::default()), &mut out);
                    if let Some(i) = seq.curr() {
                        let n = &seq.nodes()[i];
                        let (lo, hi) =
                            (n.low_frame.min(n.high_frame), n.low_frame.max(n.high_frame));
                        let at = seq.curr_frame_number();
                        assert!(
                            (lo..=hi + 1).contains(&at),
                            "frame {at} outside {lo}..={hi}"
                        );
                    }
                }
            });
            assert!(
                ok,
                "{forward:?} {sidestep:?} {turn:?} at {speed}: the update never returned"
            );
        }
    }
}
