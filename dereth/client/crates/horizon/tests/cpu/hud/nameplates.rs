//! The names over people and creatures: each stands on the top of the body it names, and the
//! interface, drawn before the world moves for the frame, moves them to where the world is drawn
//! on that frame.
//!
//! Behaviour: none (experimental Horizon interface)

use std::sync::Arc;

use dereth_horizon::art::Art;
use dereth_horizon::draw::{Anchor, DrawList, Quad, Rect};
use dereth_horizon::ui::game::{BlipKind, GameState, Nameplate, Relation, Target};
use dereth_horizon::ui::input::InputFrame;
use dereth_horizon::ui::HorizonUi;
use dereth_primitives::ObjectId;

use crate::Harness;

const NEAR: ObjectId = ObjectId(0x8300_0601);
const FAR: ObjectId = ObjectId(0x8300_0602);
const CHOSEN: ObjectId = ObjectId(0x8300_0603);

/// The interface at 1920x1080 with the pieces and fonts the client carries.
fn harness() -> Harness {
    let art = Arc::new(Art::new(Arc::new(
        dereth_horizon::pieces::Pieces::built_in().expect("the pieces built in"),
    )));
    Harness {
        ui: HorizonUi::new(art, Default::default()),
        list: DrawList::default(),
        input: InputFrame::default(),
    }
}

fn plate(id: ObjectId, name: &str, (x, y): (f32, f32), distance: f32) -> Nameplate {
    Nameplate {
        id,
        name: name.into(),
        x,
        y,
        kind: if id == CHOSEN {
            BlipKind::Selected
        } else {
            BlipKind::Creature
        },
        selected: id == CHOSEN,
        bounds: Rect::new(x - 40.0, y, 80.0, 160.0),
        distance: Some(distance),
        nearness: dereth_horizon::state::nearness(distance),
    }
}

/// The game screen with names standing on the tops at `at`: a creature five metres away, one
/// twenty-five metres away (its name shrunk and faded), and the selection.
fn names_at(at: [(f32, f32); 3]) -> GameState {
    GameState {
        in_world: true,
        connected: true,
        host: "test".into(),
        name: "Tester".into(),
        radar_range: 75.0,
        target: Some(Target {
            id: CHOSEN,
            name: "Lugian".into(),
            relation: Relation::Hostile,
            ..Target::default()
        }),
        nameplates: vec![
            plate(NEAR, "Drudge Skulker", at[0], 5.0),
            plate(FAR, "Gotrok Lugian", at[1], 25.0),
            plate(CHOSEN, "Lugian", at[2], 9.0),
        ],
        ..GameState::default()
    }
}

/// The quads each name was drawn with, by what it names.
fn names(list: &DrawList) -> Vec<(ObjectId, Vec<Quad>)> {
    list.anchored
        .iter()
        .filter(|a| a.anchor == Anchor::Top)
        .map(|a| (a.id, list.quads[a.quads.clone()].to_vec()))
        .collect()
}

/// Whether `q` is the selection's marker over its name, which is 26 layout units tall.
fn marker(q: &Quad) -> bool {
    (q.dst.h - 26.0).abs() < 1e-3
}

/// The box round `quads`.
fn bounds(quads: &[Quad]) -> Rect {
    let x0 = quads.iter().map(|q| q.dst.x).fold(f32::MAX, f32::min);
    let y0 = quads.iter().map(|q| q.dst.y).fold(f32::MAX, f32::min);
    let x1 = quads.iter().map(|q| q.dst.right()).fold(f32::MIN, f32::max);
    let y1 = quads
        .iter()
        .map(|q| q.dst.bottom())
        .fold(f32::MIN, f32::max);
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

const DRAWN: [(f32, f32); 3] = [(600.0, 400.0), (1300.0, 450.0), (960.0, 300.0)];
/// Where the bodies are drawn a frame on: moved by parts of a pixel and by many pixels.
const MOVED: [(f32, f32); 3] = [(637.3, 391.6), (1288.6, 452.2), (955.5, 312.4)];

#[test]
fn a_name_stands_centred_on_the_top_of_its_body_just_clear_of_it() {
    let mut h = harness();
    h.frame(&names_at(DRAWN));
    let drawn = names(&h.list);
    assert_eq!(drawn.len(), 3, "every name is drawn over the world");
    for (id, quads) in &drawn {
        let at = DRAWN[[NEAR, FAR, CHOSEN].iter().position(|i| i == id).unwrap()];
        // The name's letters, without the selection's marker over them.
        let letters: Vec<Quad> = quads.iter().filter(|q| !marker(q)).copied().collect();
        let b = bounds(&letters);
        let middle = b.x + b.w / 2.0;
        assert!(
            (middle - at.0).abs() <= 1.5,
            "{id:?}: the name's middle at {middle} against the top at {}",
            at.0
        );
        assert!(
            b.bottom() <= at.1 && b.bottom() > at.1 - 12.0,
            "{id:?}: the name's foot at {} against the top at {}",
            b.bottom(),
            at.1
        );
    }
}

#[test]
fn a_name_drawn_before_the_world_moved_is_moved_where_it_is_drawn_near_and_far() {
    // Drawn where the bodies were, then followed to where they are drawn a frame on...
    let mut followed = harness();
    followed.frame(&names_at(DRAWN));
    followed.list.follow(|id, anchor| {
        assert_eq!(anchor, Anchor::Top);
        [NEAR, FAR, CHOSEN]
            .iter()
            .position(|i| *i == id)
            .map(|n| MOVED[n])
    });
    // ...stands exactly where it would have been drawn there to begin with.
    let mut fresh = harness();
    fresh.frame(&names_at(MOVED));
    let (followed, fresh) = (names(&followed.list), names(&fresh.list));
    assert_eq!(followed.len(), 3);
    for ((id, a), (_, b)) in followed.iter().zip(&fresh) {
        assert_eq!(a.len(), b.len(), "{id:?}");
        // The letters and the selection's marker land on the very pixels.
        for (qa, qb) in a.iter().zip(b) {
            assert_eq!(qa.dst, qb.dst, "{id:?}: followed, against drawn there");
        }
    }
}

#[test]
fn a_name_whose_body_is_not_on_screen_when_the_world_is_drawn_is_not_drawn() {
    let mut h = harness();
    h.frame(&names_at(DRAWN));
    h.list.follow(|id, _| (id != FAR).then_some(MOVED[0]));
    for (id, quads) in names(&h.list) {
        let shown = quads.iter().any(|q| q.dst.w > 0.0 && q.dst.h > 0.0);
        assert_eq!(shown, id != FAR, "{id:?}");
    }
}

#[test]
fn the_ring_round_the_pads_other_choice_is_moved_where_its_object_is_drawn() {
    let mut h = Harness::new(Default::default());
    h.ui.hud.alt = Some(NEAR);
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        alt_at: Some((500.0, 400.0)),
        targets: vec![dereth_horizon::ui::game::Blip {
            id: NEAR,
            dx: 0.0,
            dy: 5.0,
            kind: BlipKind::Creature,
            colour: 0,
            shape: 0,
            name: "Drudge Skulker".into(),
        }],
        ..GameState::default()
    };
    h.frame(&state);
    let ring = h
        .list
        .anchored
        .iter()
        .find(|a| a.id == NEAR && a.anchor == Anchor::Origin)
        .expect("the ring stands over its object")
        .clone();
    let before: Vec<Rect> = h.list.quads[ring.quads.clone()]
        .iter()
        .map(|q| q.dst)
        .collect();
    assert!(!before.is_empty());
    h.list
        .follow(|id, anchor| (id == NEAR && anchor == Anchor::Origin).then_some((520.0, 390.0)));
    for (q, was) in h.list.quads[ring.quads].iter().zip(&before) {
        assert_eq!(q.dst, was.offset(20.0, -10.0));
    }
}
