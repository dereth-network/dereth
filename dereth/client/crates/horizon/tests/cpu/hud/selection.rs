//! The selection is marked in the world wherever it is: beyond the range names are shown in, and
//! whatever it is. The mark on screen is its name and the marker over it; the ring round it lies
//! on the ground, drawn with the world. The server is told what is selected through the health it
//! is asked for.
//!
//! Behaviour: none (experimental Horizon interface)

use dereth_client_contract::UiRequest;
use dereth_horizon::draw::Rect;
use dereth_horizon::ui::game::{Blip, BlipKind, GameState, Nameplate, Relation, Target};
use dereth_horizon::ui::Outcome;
use dereth_primitives::ObjectId;

use crate::Harness;

const FAR_CHEST: ObjectId = ObjectId(0x8000_0042);

fn blip(id: ObjectId, dx: f32, kind: BlipKind) -> Blip {
    Blip {
        id,
        dx,
        dy: 0.0,
        kind,
        colour: 0,
        shape: 0,
        name: String::new(),
    }
}

#[test]
fn a_selection_out_of_name_range_or_of_any_kind_is_still_marked() {
    let state = GameState {
        blips: vec![
            blip(FAR_CHEST, 80.0, BlipKind::Selected),
            blip(ObjectId(7), 80.0, BlipKind::Creature),
            blip(ObjectId(8), 5.0, BlipKind::Item),
        ],
        ..GameState::default()
    };
    assert_eq!(
        dereth_horizon::state::nameplate_candidates(&state),
        [FAR_CHEST],
        "only the far selection; a far creature and a near thing are not named"
    );
}

#[test]
fn the_selection_is_named_on_screen_without_brackets_round_it() {
    let colour = 0xFFF3_D36C;
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        target: Some(Target {
            id: FAR_CHEST,
            name: "Chest".into(),
            relation: Relation::Hostile,
            ..Target::default()
        }),
        nameplates: vec![Nameplate {
            id: FAR_CHEST,
            name: "Chest".into(),
            x: 960.0,
            y: 500.0,
            kind: BlipKind::Selected,
            selected: true,
            bounds: Rect::new(900.0, 420.0, 120.0, 160.0),
            distance: None,
            nearness: 1.0,
        }],
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    let strokes = h
        .list
        .quads
        .iter()
        .filter(|q| {
            q.tex.is_none()
                && (q.colour == colour
                    || q.colour == dereth_horizon::draw::with_alpha(colour, 0.8)
                    || q.colour == 0x9000_0000)
        })
        .count();
    assert_eq!(strokes, 0, "no corner strokes round the selection");
}

/// The objects this frame asked the server for the health of, in order.
fn health_queries(out: &Outcome) -> Vec<ObjectId> {
    out.requests
        .iter()
        .filter_map(|r| match r {
            UiRequest::QueryHealth(id) => Some(*id),
            _ => None,
        })
        .collect()
}

#[test]
fn the_server_is_asked_for_the_health_of_each_new_selection_and_of_none_when_it_goes() {
    let raver = ObjectId(0x8000_033F);
    let selected = |id| GameState {
        in_world: true,
        name: "Tester".into(),
        target: Some(Target {
            id,
            name: "Banderling Raver".into(),
            relation: Relation::Hostile,
            ..Target::default()
        }),
        ..GameState::default()
    };
    let nothing = GameState {
        in_world: true,
        name: "Tester".into(),
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    assert_eq!(
        health_queries(&h.frame(&nothing)),
        [],
        "nothing selected yet"
    );
    assert_eq!(health_queries(&h.frame(&selected(raver))), [raver]);
    assert_eq!(
        health_queries(&h.frame(&selected(raver))),
        [],
        "once per selection"
    );
    assert_eq!(
        health_queries(&h.frame(&nothing)),
        [ObjectId(0)],
        "a selection that goes (the creature killed) asks for no one's health"
    );
    assert_eq!(health_queries(&h.frame(&nothing)), [], "and only once");
    let next = ObjectId(0x8000_0341);
    assert_eq!(health_queries(&h.frame(&selected(next))), [next]);
}
