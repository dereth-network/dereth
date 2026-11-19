//! The party list and the conflict lamp: which rows, in which order, what a press asks, and the
//! lamp for the place's conflict.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::view::{conflict, kingdom};
use auberean_client_sdk::Action;
use auberean_horizon::draw::Rect;
use auberean_horizon::social::{Fellow, Party};
use auberean_horizon::state::{GameState, Vital};
use auberean_horizon::ui::hud::{effect_rect, EFFECTS_PER_ROW};
use auberean_horizon::ui::party::{
    conflict_words, kingdom_words, lamp_rect, row_rect, rows, TAKEN,
};

use crate::{centre, drew_piece, in_world, Harness, PLAYER};

pub const CARA: u64 = 0x2130_0000_0000_00B2;
pub const DOV: u64 = 0x2130_0000_0000_00C3;

pub fn fellow(id: u64, name: &str, health: u64) -> Fellow {
    Fellow {
        id,
        name: name.into(),
        level: 20,
        health: Vital {
            current: health,
            max: 100,
        },
        vigor: Vital {
            current: 40,
            max: 80,
        },
        ..Fellow::default()
    }
}

/// The player in a fellowship Cara leads, with Dov.
pub fn in_party() -> GameState {
    let mut s = in_world();
    s.health = Vital {
        current: 99,
        max: 100,
    };
    s.social.party = Some(Party {
        name: "The Hunt".into(),
        leader: CARA,
        shares_experience: true,
        looting: 1,
        fellows: vec![
            fellow(CARA, "Cara", 80),
            fellow(PLAYER, "Me", 10),
            fellow(DOV, "Dov", 60),
        ],
    });
    s
}

#[test]
fn the_party_list_shows_the_player_first_with_its_own_vitals_then_the_fellows_in_order() {
    let s = in_party();
    let shown: Vec<(u64, u64)> = rows(&s).iter().map(|f| (f.id, f.health.current)).collect();
    assert_eq!(shown, [(PLAYER, 99), (CARA, 80), (DOV, 60)]);
    assert!(rows(&in_world()).is_empty(), "no fellowship, no rows");

    let mut h = Harness::new();
    h.frame(&s);
    let k = h.k();
    let (x, y) = centre(row_rect(k, 2));
    assert!(h.ui.covers(x, y), "the third row answers the pointer");
    let (x, y) = centre(row_rect(k, 3));
    assert!(!h.ui.covers(x, y), "there is no fourth");
    h.frame(&in_world());
    let (x, y) = centre(row_rect(k, 0));
    assert!(!h.ui.covers(x, y), "out of a fellowship the list is gone");
}

#[test]
fn a_row_pressed_selects_its_fellow_and_pressed_again_at_once_starts_a_tell() {
    let s = in_party();
    let mut h = Harness::new();
    h.frame(&s);
    let at = centre(row_rect(h.k(), 1));
    let first = h.click(&s, at);
    assert_eq!(first.acts, [Action::SetSelection { target: CARA }]);
    let again = h.click(&s, at);
    assert_eq!(
        again.acts,
        [
            Action::SetSelection { target: CARA },
            Action::AssistedTell {
                name: "Cara".into()
            },
        ]
    );
    for _ in 0..60 {
        h.frame(&s);
    }
    assert_eq!(
        h.click(&s, at).acts,
        [Action::SetSelection { target: CARA }],
        "a second press a second later selects again"
    );
}

#[test]
fn a_fellows_health_shows_what_other_players_took_past_what_is_left() {
    let mut s = in_party();
    if let Some(p) = &mut s.social.party {
        p.fellows[0].health_taken = 10;
    }
    let mut h = Harness::new();
    h.frame(&s);
    let row = row_rect(h.k(), 1);
    let taken: Vec<Rect> = h
        .list
        .quads
        .iter()
        .filter(|q| q.tex.is_none() && q.colour == TAKEN && row.contains(q.dst.x, q.dst.y))
        .map(|q| q.dst)
        .collect();
    assert_eq!(taken.len(), 1, "{taken:?}");
    let health = h
        .list
        .quads
        .iter()
        .find(|q| {
            q.tex.is_none()
                && q.colour == auberean_horizon::ui::hud::HEALTH
                && row.contains(q.dst.x, q.dst.y)
        })
        .expect("the health drawn")
        .dst;
    assert!(
        (taken[0].w - health.w * 90.0 / 80.0).abs() < 0.5,
        "80 left and 10 taken of 100: {} against {}",
        taken[0].w,
        health.w
    );
}

#[test]
fn the_conflict_lamp_shows_the_places_conflict_at_the_status_rows_left_end() {
    let mut h = Harness::new();
    let mut s = in_world();
    h.frame(&s);
    let k = h.k();
    let lamp = lamp_rect(h.screen, k);
    let last = effect_rect(h.screen, k, EFFECTS_PER_ROW - 1);
    assert!(
        lamp.right() < last.x && lamp.y == last.y,
        "left of the effects"
    );
    let lit = |h: &Harness, colour| {
        h.list
            .quads
            .iter()
            .any(|q| q.tex.is_none() && q.colour == colour && lamp.contains(q.dst.x, q.dst.y))
    };
    let (_, red) = conflict_words(conflict::KINGDOM_AGAINST_KINGDOM).expect("a kind");
    assert!(!lit(&h, red), "nothing told, no lamp");
    s.social.conflict = Some(conflict::KINGDOM_AGAINST_KINGDOM);
    h.frame(&s);
    assert!(lit(&h, red));
    s.social.conflict = Some(conflict::PEACE);
    h.frame(&s);
    assert!(!lit(&h, red), "a peace area's lamp is another color");
    assert_eq!(conflict_words(0), None);
    assert_eq!(
        conflict_words(conflict::PEACE).map(|c| c.0),
        Some("Peace Area")
    );
    assert_eq!(kingdom_words(Some(kingdom::ORDER)), "The Order");
    assert_eq!(kingdom_words(None), "No Kingdom");
}

#[test]
fn with_its_art_each_kind_of_conflict_draws_its_own_lamp_piece() {
    let mut h = Harness::with_built_in_art();
    let mut s = in_world();
    let lamp = lamp_rect(h.screen, h.k());
    let kinds = [
        (conflict::PEACE, "lamp.peace"),
        (conflict::CONTROLLED, "lamp.controlled"),
        (conflict::KINGDOM_AGAINST_KINGDOM, "lamp.kingdom"),
        (conflict::KINGDOM_AGAINST_KINGDOM_ANYWHERE, "lamp.kingdom"),
        (conflict::FREE_FOR_ALL, "lamp.free-for-all"),
    ];
    for (kind, piece) in kinds {
        s.social.conflict = Some(kind);
        h.frame(&s);
        assert_eq!(drew_piece(&h, piece), vec![lamp], "{piece} for {kind}");
        for (_, other) in kinds.iter().filter(|(_, p)| *p != piece) {
            assert!(drew_piece(&h, other).is_empty(), "not {other} for {kind}");
        }
    }
}

#[test]
fn a_fellows_key_selects_the_party_lists_row_of_its_place() {
    let mut h = Harness::new();
    let s = in_party();
    assert_eq!(
        h.ui.keyed(auberean_horizon::keys::Keyed::Fellow(0), &s),
        [Action::SetSelection { target: PLAYER }]
    );
    assert_eq!(
        h.ui.keyed(auberean_horizon::keys::Keyed::Fellow(2), &s),
        [Action::SetSelection { target: DOV }]
    );
    assert!(h
        .ui
        .keyed(auberean_horizon::keys::Keyed::Fellow(3), &s)
        .is_empty());
}
