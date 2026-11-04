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

/// A turn too small to be a turn sends nothing: headings count as the same within 0.0002 radians,
/// the way round through north included.
///
/// Behaviour: session.position.a-turn-under-the-heading-tolerance-is-not-reported
#[test]
fn a_turn_under_the_heading_tolerance_is_not_reported() {
    let turned = |from: f32, to: f32| {
        let mut b = placed();
        let mut now = 0.0;
        let mut reports = Vec::new();
        b.ask(Motion {
            forward: 0.0,
            sideways: 0.0,
            heading: from,
            active: false,
        });
        run(&mut b, &mut now, 0.1, 0.01, &mut reports);
        b.report(now, 0.0);
        b.follow(
            0x7574_0031,
            Vec3 {
                x: 100.0,
                y: 100.0,
                z: 50.0,
            },
            to,
        );
        b.should_report(now + 1.0)
    };
    assert!(!turned(90.0, 90.01), "under the tolerance");
    assert!(turned(90.0, 90.02), "over the tolerance");
    assert!(!turned(359.999, 0.0), "over north");
}

/// While the body turns toward a heading the report says that heading, not where it faces yet, and
/// the check that follows compares where it faces with what was said, so it keeps reporting until
/// the body gets there.
///
/// Behaviour: session.position.a-report-says-where-the-body-is-turning-to
#[test]
fn a_report_says_where_the_body_is_turning_to() {
    let mut b = placed();
    let here = Vec3 {
        x: 100.0,
        y: 100.0,
        z: 50.0,
    };
    b.follow(0x7574_0031, here, 90.0);
    b.turning_to(Some(180.0));
    let r = b.report(0.0, 0.0);
    assert_eq!(r.heading_motion >> 24, heading_byte(180.0));
    assert!(b.should_report(1.0), "still facing 90");
    b.follow(0x7574_0031, here, 180.0);
    b.turning_to(None);
    assert!(!b.should_report(2.0), "there");
    assert_eq!(b.report(3.0, 0.0).heading_motion >> 24, heading_byte(180.0));
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

/// A move the server orders the player's body (walking it up to what it uses) keeps the order's
/// stamp, and the reports after it carry that stamp: when the player then moves by hand, the
/// report says so (the moving flag) with the walk's stamp, as the recorded 2017 sessions' reports
/// do after each order. An order for another object leaves the stamp as it is.
///
/// Behaviour: session.position.a-report-carries-the-stamp-of-the-last-move-ordered
#[test]
fn a_report_carries_the_stamp_of_the_last_move_ordered() {
    use auberean_client_runtime::data::{DataFiles, ENGLISH};
    use auberean_client_runtime::{Client, ClientData, ClientSettings};
    use auberean_protocol::login::PlayerObject;
    use auberean_protocol::physics::{MoveOrder, ObjectEvent};
    use auberean_protocol::types::{MoveTarget, MovementParams, ObjectStamp};
    use auberean_protocol::ServerMessage;
    const PLAYER: u64 = 0x2130_0000_0000_0042;
    const OTHER: u64 = 0x2130_0000_0000_0055;
    // Messages that take effect as they come, in order.
    const NOW: u64 = 3 << 56;
    let mut c = Client::new(
        ClientSettings {
            login_server: "127.0.0.1:9000".parse().expect("an address"),
            account: b"a".to_vec(),
            ticket: Vec::new(),
            timestamp: 0,
            session_key: [0; 64],
            script_sends: false,
        },
        ClientData {
            files: DataFiles {
                files: Vec::new(),
                language: ENGLISH,
            },
            scripts: None,
        },
    );
    c.take_message(
        NOW,
        ServerMessage::PlayerObject(PlayerObject {
            id: PLAYER,
            region: 1,
        }),
    );
    for id in [PLAYER, OTHER] {
        c.take_message(
            NOW,
            ServerMessage::CreateObject(auberean_protocol::physics::CreateObject {
                id,
                ..Default::default()
            }),
        );
    }
    let order = |id: u64, stamp: u16| {
        ServerMessage::MoveTo(ObjectEvent {
            sender: ObjectStamp {
                id,
                instance_stamp: 0,
                other_stamp: 0,
            },
            action: MoveOrder {
                params: MovementParams {
                    flags: 0x63,
                    desired_velocity: 1.0,
                    fail_distance: f32::MAX,
                    desired_distance: 2.0,
                    max_success_distance: 2.0,
                    min_success_distance: 0.0,
                    target: MoveTarget::Object(0x2130_0000_0000_0099),
                    offset: Vec3::default(),
                    offset_id: 0,
                    waypoints: Vec::new(),
                    context: 0,
                },
                stamp,
            },
        })
    };
    c.take_message(NOW, order(PLAYER, 7));
    c.take_message(NOW, order(OTHER, 9));
    c.body.place(
        0x7574_0031,
        Vec3 {
            x: 100.0,
            y: 100.0,
            z: 50.0,
        },
        90.0,
    );
    c.body.ask(Motion {
        forward: 1.0,
        sideways: 0.0,
        heading: 90.0,
        active: true,
    });
    let r = c.body.report(0.0, 0.0);
    assert_eq!(r.move_to_stamp, 7);
    assert_eq!(r.flags & MOVING, MOVING);
}
