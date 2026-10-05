use super::meter::{direction, Meter, CHILD_IMAGE};
use crate::region::Box2D;
use crate::ElementId;

/// The meter fill uses the client's four-arm direction switch, evaluated on the box
/// the retail `0x21000005` gives the health bar's fill child `0x00000002` — 150 x 16,
/// read off the live tree decoded from `client_local_English.dat`.
///
/// The proportion is the whole point: a meter at 22/30 must show 22/30 of its width, not "not
/// zero". `fixtures/packet-captures/first-login-walk-jump.jsonl` is where 22/30 comes from — the stamina the
/// recorded ACE server sent after the player jumped.
#[test]
fn the_fill_is_clipped_to_its_own_fraction_of_the_box() {
    let box_ = Box2D::new(325, 5, 474, 20); // 150 wide, 16 high
    let clip = |l: f32| {
        let m = Meter {
            position: l,
            ..Meter::default()
        };
        m.child_clip(CHILD_IMAGE, box_)
    };
    // Direction 1 is the constructor's default and what the three vitals bars use.
    assert_eq!(
        clip(1.0),
        Some(box_),
        "a full meter shows the whole graphic"
    );
    assert_eq!(clip(0.5).unwrap().width(), 75);
    assert_eq!(
        clip(0.5).unwrap().x0,
        box_.x0,
        "it grows from the left edge"
    );
    // 22/30: the stamina in first-login-walk-jump after the jump.
    assert_eq!(clip(22.0 / 30.0).unwrap().width(), 110);
    assert_eq!(clip(28.0 / 30.0).unwrap().width(), 140);
    // Empty is *invalid*, which is how `draw_region` drops the fill entirely.
    assert!(!clip(0.0).unwrap().is_valid());
    // `cur > max` happens on a live ACE server (early-inventory-and-casting reaches mana
    // 111/100) and neither the client nor this clamps the ratio; the intersection with the
    // child's own box does.
    assert_eq!(clip(1.11), Some(box_));
}

/// Oracle: the same switch's other three arms.
#[test]
fn each_direction_grows_the_fill_from_its_own_edge() {
    let box_ = Box2D::new(0, 0, 99, 39); // 100 x 40
    let at = |d: u32, l: f32| {
        Meter {
            position: l,
            direction: d,
            ..Meter::default()
        }
        .child_clip(CHILD_IMAGE, box_)
    };
    assert_eq!(at(direction::LEFT, 0.25), Some(Box2D::new(0, 0, 24, 39)));
    assert_eq!(at(direction::TOP, 0.25), Some(Box2D::new(0, 0, 99, 9)));
    assert_eq!(at(direction::RIGHT, 0.25), Some(Box2D::new(75, 0, 99, 39)));
    assert_eq!(at(direction::BOTTOM, 0.25), Some(Box2D::new(0, 30, 99, 39)));
    // …and the bottom arm's y0 is `y0 + h*(1-l)`, i.e. 30, with the full width kept.
    let b = at(direction::BOTTOM, 0.25).unwrap();
    assert_eq!((b.x0, b.x1, b.y0, b.y1), (0, 99, 30, 39));
    // An unrecognised direction narrows nothing, as the switch's default does.
    assert_eq!(at(0, 0.25), None);
    assert_eq!(at(5, 0.25), None);
}

/// Oracle: the child draw's guard — the child image is the id-2 descendant, the `0x68`
/// arm draws the child unclipped, and a frame meter has no child image at all.
#[test]
fn only_the_id_2_child_of_a_clipping_meter_is_narrowed() {
    let box_ = Box2D::new(0, 0, 99, 15);
    let m = Meter {
        position: 0.5,
        ..Meter::default()
    };
    assert!(m.child_clip(CHILD_IMAGE, box_).is_some());
    assert_eq!(
        m.child_clip(ElementId(0x1000_00E7), box_),
        None,
        "the trough is never clipped"
    );
    assert_eq!(
        m.child_clip(ElementId(0x1000_00EB), box_),
        None,
        "nor the label"
    );
    let moved = Meter {
        position: 0.5,
        move_fill: true,
        ..Meter::default()
    };
    assert_eq!(
        moved.child_clip(CHILD_IMAGE, box_),
        None,
        "0x68 moves the child instead"
    );
    let framed = Meter {
        position: 0.5,
        frame_meter: true,
        ..Meter::default()
    };
    assert_eq!(
        framed.child_clip(CHILD_IMAGE, box_),
        None,
        "a frame meter has no child image"
    );
}

/// Oracle: the meter's constructor and its member initialisers.
#[test]
fn the_constructed_meter_is_the_clients_constructed_meter() {
    let m = Meter::default();
    assert_eq!(m.direction, direction::LEFT, "the direction starts at 1");
    assert_eq!(m.current_frame, -1);
    assert!(!m.frame_meter && !m.animating);
    assert_eq!(
        m.anim_start_time, -1.0,
        "the animation start time starts at -1.0"
    );
    assert_eq!(m.anim_end_time, -1.0);
    assert_eq!(m.position, 0.0);
}
