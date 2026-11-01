//! When the client reports where the player is, and what a report says.

use auberean_client_contract::Motion;
use auberean_client_runtime::motion::{heading_byte, motion_byte, Body, JUMP, MOVING};
use auberean_primitives::Vec3;

fn placed() -> Body {
    let mut b = Body::default();
    b.place(
        0x7574_0031,
        Vec3 {
            x: 100.0,
            y: 100.0,
            z: 50.0,
        },
        90.0,
    );
    b
}

fn run(b: &mut Body, now: &mut f64, until: f64, frame: f64, reports: &mut Vec<f64>) {
    while *now + frame <= until + 1e-9 {
        *now += frame;
        #[allow(clippy::cast_possible_truncation)]
        b.step(frame as f32);
        if b.should_report(*now) {
            b.report(*now, 0.0);
            reports.push(*now);
        }
    }
}

/// The client reports at once when the asked motion changes or the player jumps; while the body
/// moves or turns it reports again only after more than a fifth of a second; a body standing still
/// sends nothing more.
///
/// Behaviour: session.position.reports-on-change-jump-and-every-fifth-of-a-second
#[test]
fn reports_go_on_a_change_a_jump_and_every_fifth_of_a_second_while_moving() {
    let mut b = placed();
    let mut now = 0.0;
    let mut reports = Vec::new();
    // Standing: the first report, then nothing.
    run(&mut b, &mut now, 1.0, 0.01, &mut reports);
    assert_eq!(reports.len(), 1);
    // Running forward: at once, then every first frame past 0.2 s.
    b.ask(Motion {
        forward: 1.0,
        sideways: 0.0,
        heading: 90.0,
        active: true,
    });
    run(&mut b, &mut now, 2.0, 0.01, &mut reports);
    let running: Vec<f64> = reports[1..].windows(2).map(|w| w[1] - w[0]).collect();
    assert!(
        (reports[1] - 1.01).abs() < 1e-6,
        "the change is reported in its own frame"
    );
    assert!(running.iter().all(|g| *g > 0.2 && *g < 0.22), "{running:?}");
    // Stopping is a change; standing after it sends nothing.
    let before = reports.len();
    b.ask(Motion {
        forward: 0.0,
        sideways: 0.0,
        heading: 90.0,
        active: false,
    });
    run(&mut b, &mut now, 3.0, 0.01, &mut reports);
    assert_eq!(reports.len(), before + 1);
    // A jump goes at once.
    b.jump(Vec3 {
        x: 0.0,
        y: 0.0,
        z: 6.0,
    });
    assert!(b.should_report(now));
    let r = b.report(now, 0.0);
    assert_eq!(r.flags & JUMP, JUMP);
    assert_eq!(r.jump_velocity.z, 6.0);
    assert!(!b.should_report(now + 0.01));
}

/// A report carries the heading in its top byte (255 to a full turn), the asked motion as bytes
/// (sideways, forward, up; 127 for none), the moving flag while the player moves or turns, and the
/// body's stamps.
///
/// Behaviour: session.position.a-report-packs-heading-motion-flags-and-stamps
#[test]
fn a_report_packs_heading_motion_flags_and_stamps() {
    let mut b = placed();
    b.teleport_stamp = 3;
    b.instance_stamp = 36;
    b.ask(Motion {
        forward: 1.0,
        sideways: 0.0,
        heading: 273.9,
        active: true,
    });
    let r = b.report(0.0, 1.25e8);
    assert_eq!(r.heading_motion >> 24, 194);
    assert_eq!(r.heading_motion.to_le_bytes()[..3], [0x7F, 0xFF, 0x7F]);
    assert_eq!(r.flags, MOVING);
    assert_eq!((r.instance_stamp, r.teleport_stamp), (36, 3));
    assert_eq!(r.time, 1.25e8);
    assert_eq!(heading_byte(360.0), 0);
    assert_eq!(motion_byte(0.0), 0x7F);
    assert_eq!(motion_byte(-1.0), 0);
}
