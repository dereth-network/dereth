//! The names over people and creatures: each stands on the top of the body it names, and the
//! interface, drawn before the world moves for the frame, moves them to where the world is drawn
//! on that frame, at the depth of the top of the body, so the world hides them where it stands
//! in front.
//!
//! Behaviour: none (experimental Horizon interface)

use std::sync::Arc;

use dereth_client_contract::overlay::{
    OverlayDepth, OverlayItem, OverlaySampler, OverlayTexture, OverlayVertex,
};
use dereth_client_runtime::present::NullPresentation;
use dereth_horizon::art::Art;
use dereth_horizon::draw::{Anchor, DrawList, Overlay, Quad, Rect, Spot};
use dereth_horizon::ui::game::{BlipKind, GameState, Nameplate, Relation, Target};
use dereth_horizon::ui::input::InputFrame;
use dereth_horizon::ui::HorizonUi;
use dereth_primitives::ObjectId;

use crate::Harness;

const NEAR: ObjectId = ObjectId(0x8300_0601);
const FAR: ObjectId = ObjectId(0x8300_0602);
const CHOSEN: ObjectId = ObjectId(0x8300_0603);
const INSIDE: ObjectId = ObjectId(0x8300_0604);

/// The pieces and fonts the client carries.
fn built_in_art() -> Arc<Art> {
    Arc::new(Art::new(Arc::new(
        dereth_horizon::pieces::Pieces::built_in().expect("the pieces built in"),
    )))
}

/// The interface at 1920x1080 with the pieces and fonts the client carries.
fn harness() -> Harness {
    harness_with(built_in_art())
}

/// The interface at 1920x1080 drawing from `art`.
fn harness_with(art: Arc<Art>) -> Harness {
    Harness {
        ui: HorizonUi::new(art, Default::default()),
        list: DrawList::default(),
        input: InputFrame::default(),
        screen: (1920.0, 1080.0),
    }
}

/// A name `distance` meters from a player outdoors.
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
        nearness: dereth_horizon::state::nearness(distance, true),
    }
}

/// The game screen with names standing on the tops at `at`: a creature five meters away, one
/// fifty meters away (its name shrunk and faded), and the selection.
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
            plate(FAR, "Gotrok Lugian", at[1], 50.0),
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
            .map(|n| Spot::at(MOVED[n].0, MOVED[n].1))
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
fn a_name_thirty_meters_off_outdoors_is_drawn_as_large_as_one_close_by_and_indoors_is_not_drawn() {
    let mut h = harness();
    let mut state = names_at(DRAWN);
    state.target = None;
    state.nameplates = vec![
        plate(NEAR, "Drudge", DRAWN[0], 5.0),
        plate(FAR, "Drudge", DRAWN[1], 30.0),
        Nameplate {
            nearness: dereth_horizon::state::nearness(30.0, false),
            ..plate(INSIDE, "Drudge", DRAWN[2], 30.0)
        },
    ];
    h.frame(&state);
    let drawn = names(&h.list);
    let size = |id: ObjectId| {
        let (_, quads) = drawn.iter().find(|(i, _)| *i == id).expect("drawn");
        let b = bounds(quads);
        (b.w, b.h)
    };
    let ((fw, fh), (nw, nh)) = (size(FAR), size(NEAR));
    assert!(
        (fw - nw).abs() < 0.01 && (fh - nh).abs() < 0.01,
        "thirty meters off outdoors: whole, {fw}x{fh} against {nw}x{nh}"
    );
    assert!(
        drawn.iter().all(|(id, _)| *id != INSIDE),
        "thirty meters off indoors: not drawn"
    );
}

#[test]
fn a_name_whose_body_is_not_on_screen_when_the_world_is_drawn_is_not_drawn() {
    let mut h = harness();
    h.frame(&names_at(DRAWN));
    h.list
        .follow(|id, _| (id != FAR).then_some(Spot::at(MOVED[0].0, MOVED[0].1)));
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
    h.list.follow(|id, anchor| {
        (id == NEAR && anchor == Anchor::Origin).then_some(Spot::at(520.0, 390.0))
    });
    for (q, was) in h.list.quads[ring.quads].iter().zip(&before) {
        assert_eq!(q.dst, was.offset(20.0, -10.0));
    }
}

/// The depth of the top of each body, nearest first: as the world's projection gives it.
const DEPTHS: [f32; 3] = [0.98, 0.997, 0.99];

/// The names drawn where the bodies stand, followed to the depth of the top of each.
fn names_at_depth(h: &mut Harness) {
    h.frame(&names_at(DRAWN));
    h.list.follow(|id, _| {
        let n = [NEAR, FAR, CHOSEN].iter().position(|i| *i == id)?;
        Some(Spot {
            x: DRAWN[n].0,
            y: DRAWN[n].1,
            depth: Some(DEPTHS[n]),
        })
    });
}

#[test]
fn a_name_and_the_selections_marker_stand_at_the_depth_of_the_top_of_its_body() {
    let mut h = harness();
    names_at_depth(&mut h);
    let drawn = names(&h.list);
    assert_eq!(drawn.len(), 3);
    for (id, quads) in &drawn {
        let n = [NEAR, FAR, CHOSEN].iter().position(|i| i == id).unwrap();
        assert!(
            quads.iter().all(|q| q.depth == Some(DEPTHS[n])),
            "{id:?}: every quad of the name stands at its body's depth"
        );
    }
    let (_, chosen) = drawn.iter().find(|(id, _)| *id == CHOSEN).unwrap();
    assert!(
        chosen.iter().any(marker),
        "the selection's marker is among them"
    );
    // Nothing else the interface draws stands in the world.
    let named: Vec<usize> = h
        .list
        .anchored
        .iter()
        .flat_map(|a| a.quads.clone())
        .collect();
    let rest: Vec<&Quad> = h
        .list
        .quads
        .iter()
        .enumerate()
        .filter(|(i, _)| !named.contains(i))
        .map(|(_, q)| q)
        .collect();
    assert!(!rest.is_empty(), "the interface draws more than the names");
    assert!(rest.iter().all(|q| q.depth.is_none()));
}

#[test]
fn the_ring_round_the_pads_other_choice_is_drawn_over_the_world() {
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
    h.list.follow(|_, _| {
        Some(Spot {
            x: 520.0,
            y: 390.0,
            depth: Some(0.9),
        })
    });
    let ring = h
        .list
        .anchored
        .iter()
        .find(|a| a.id == NEAR && a.anchor == Anchor::Origin)
        .expect("the ring stands over its object")
        .clone();
    assert!(!ring.in_world);
    assert!(h.list.quads[ring.quads].iter().all(|q| q.depth.is_none()));
}

/// The overlay's triangles, in drawing order: their texture, how they are sampled and their
/// vertices, and whether each is hidden behind the world.
fn triangles(
    items: &[OverlayItem],
) -> Vec<(OverlayTexture, OverlaySampler, OverlayDepth, OverlayVertex)> {
    items
        .iter()
        .flat_map(|item| match item {
            OverlayItem::Triangles {
                texture,
                sampler,
                depth,
                vertices,
                ..
            } => vertices
                .iter()
                .map(|v| (*texture, *sampler, *depth, *v))
                .collect(),
            OverlayItem::Preview { .. } => Vec::new(),
        })
        .collect()
}

#[test]
fn the_overlay_hides_only_the_names_behind_the_world_and_draws_the_rest_as_before() {
    // The interface as it was drawn before names stood in the world: the same frame, with no
    // depth for any name.
    let art = built_in_art();
    let mut before = harness_with(Arc::clone(&art));
    before.frame(&names_at(DRAWN));
    before.list.follow(|id, _| {
        let n = [NEAR, FAR, CHOSEN].iter().position(|i| *i == id)?;
        Some(Spot::at(DRAWN[n].0, DRAWN[n].1))
    });
    let mut now = harness_with(Arc::clone(&art));
    names_at_depth(&mut now);
    // Each drawn as the first frame of an overlay of its own, so both are drawn alike.
    let mut present = NullPresentation::new(1920, 1080);
    let mut overlay = Overlay::default();
    overlay.compose(&mut present, &art, &before.list, false, &[]);
    let was = triangles(overlay.items());
    assert_eq!(overlay.stats.upload_failures, 0, "every picture is drawn");
    let mut present = NullPresentation::new(1920, 1080);
    let mut overlay = Overlay::default();
    overlay.compose(&mut present, &art, &now.list, false, &[]);
    let is = triangles(overlay.items());
    assert_eq!(overlay.stats.upload_failures, 0, "every picture is drawn");
    assert!(
        was.iter()
            .all(|t| t.2 == OverlayDepth::Over && t.3.position[2] == 0.5),
        "before, everything was drawn over the world"
    );
    assert_eq!(was.len(), is.len(), "the same triangles");
    let quads: usize = names(&now.list).iter().map(|(_, q)| q.len()).sum();
    let in_world = is.iter().filter(|t| t.2 == OverlayDepth::World).count();
    assert!(quads > 0);
    assert_eq!(
        in_world,
        quads * 6,
        "every quad of every name, and only those"
    );
    for (a, b) in was.iter().zip(&is) {
        match b.2 {
            // A name's: the very triangle, at the depth of its body's top.
            OverlayDepth::World => {
                assert!(DEPTHS.contains(&b.3.position[2]), "{b:?}");
                let mut v = b.3;
                v.position[2] = 0.5;
                assert_eq!((a.0, a.1, a.3), (b.0, b.1, v));
            }
            // The rest: exactly as before.
            OverlayDepth::Over => assert_eq!(a, b),
        }
    }
}
