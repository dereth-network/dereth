//! The player's standing on the HUD, from a view: the experience band along the top, the link
//! line over the minimap and its lamp, and the vitae lamp beside the conflict lamp.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::notice::examination::kind as examine;
use auberean_client_sdk::contract::notice::{vital, Notice, PlayerNotice, Said};
use auberean_client_sdk::contract::session::Heartbeat;
use auberean_client_sdk::contract::view::{self as v, Progress, View};
use auberean_client_sdk::Action;
use auberean_horizon::draw::{Quad, Rect};
use auberean_horizon::state::{Frame, GameState};
use auberean_horizon::ui::hud::{target_gauge_rect, EXPERIENCE};
use auberean_horizon::ui::minimap;
use auberean_horizon::ui::status::{
    self, experience_rect, link_anchor, vitae_rect, vitae_words, VITAE_GROUND,
};
use dereth_ui_kit::hud::{LINK_GOOD, LINK_LOST, LINK_SLOW};

use crate::{centre, Harness, PLAYER};

/// The flat quads drawn in `colour`.
fn fills(h: &Harness, colour: u32) -> Vec<Rect> {
    h.list
        .quads
        .iter()
        .filter(|q: &&Quad| q.tex.is_none() && q.colour == colour)
        .map(|q| q.dst)
        .collect()
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

/// The player in the world as `view` gives it, with what the scripts said in `notices`.
fn follow(view: &View, notices: &[Notice]) -> GameState {
    let mut s = GameState::default();
    s.in_world = true;
    s.follow(&Frame {
        view: Some(view),
        notices,
        events: &[],
        told: &[],
        lines: &[],
        write: &|_| None,
        unknown_command: "",
    });
    s
}

/// A view of the player on the world `Ishgard` with `progress`, the link measured as `link`
/// says and the server last heard `since_heard` seconds before, at 9:57 in the evening.
fn view(progress: Progress, link: Option<Heartbeat>, since_heard: Option<f64>) -> View {
    View {
        player_id: Some(PLAYER),
        player: Some(v::Player {
            id: PLAYER,
            world_name: "Ishgard".into(),
            progress,
            ..v::Player::default()
        }),
        link,
        since_heard,
        local_clock: Some((21, 57)),
        ..View::default()
    }
}

fn measured(latency: f64, loss: f64) -> Heartbeat {
    Heartbeat {
        count: 3,
        latency,
        loss,
        ..Heartbeat::default()
    }
}

#[test]
fn the_link_line_reads_the_world_the_clock_and_the_round_trip_from_the_view() {
    let s = follow(
        &view(
            Progress::default(),
            Some(measured(0.1046, 0.031)),
            Some(1.3),
        ),
        &[],
    );
    let link = status::link(&s);
    assert_eq!(link.text(), "Ishgard  |  9:57 PM  |  105 ms");
    assert_eq!(
        link.tip_lines(),
        [
            "Round trip: 105 ms",
            "Packet loss: 3.1%",
            "Last heard: 1.3 s ago"
        ]
    );
    // Before a round trip was measured the line has no time for one.
    let s = follow(
        &view(Progress::default(), Some(measured(0.0, 0.0)), None),
        &[],
    );
    assert_eq!(status::link(&s).text(), "Ishgard  |  9:57 PM");
    // The clock at midnight and at noon.
    let mut v = view(Progress::default(), None, None);
    v.local_clock = Some((0, 5));
    assert_eq!(
        status::link(&follow(&v, &[])).text(),
        "Ishgard  |  12:05 AM"
    );
    v.local_clock = Some((12, 0));
    assert_eq!(
        status::link(&follow(&v, &[])).text(),
        "Ishgard  |  12:00 PM"
    );
}

#[test]
fn the_link_lamp_over_the_minimap_is_green_amber_then_red_as_the_server_falls_quiet() {
    let mut h = Harness::new();
    let map = minimap::parts(h.screen, h.k()).map;
    let (right, top) = link_anchor(h.screen, h.k());
    assert!(
        near(right - 15.0 * h.k(), map.right()),
        "the lamp ends where the map does"
    );
    assert!(top < map.y, "over the minimap");
    for (since, lamp) in [
        (Some(0.5), LINK_GOOD),
        (Some(3.0), LINK_SLOW),
        (Some(6.0), LINK_LOST),
        (None, LINK_LOST),
    ] {
        let s = follow(
            &view(Progress::default(), Some(measured(0.05, 0.0)), since),
            &[],
        );
        h.frame(&s);
        let lit = fills(&h, lamp);
        assert_eq!(lit.len(), 1, "{since:?}: {lit:?}");
        let l = lit[0];
        assert!(
            l.right() <= map.right() + 0.5 && l.bottom() < map.y,
            "{l:?}"
        );
        for other in [LINK_GOOD, LINK_SLOW, LINK_LOST]
            .into_iter()
            .filter(|c| *c != lamp)
        {
            assert!(fills(&h, other).is_empty(), "{since:?}: only one lamp");
        }
    }
    // Over the line, the connection's measures.
    let s = follow(
        &view(Progress::default(), Some(measured(0.05, 0.0)), Some(0.5)),
        &[],
    );
    h.input.mouse = (right - 20.0 * h.k(), top + 8.0 * h.k());
    h.frame(&s);
    let tip = h.ui.tip().expect("a tooltip");
    assert_eq!(tip.title, "Connection");
    assert_eq!(tip.lines[0], "Round trip: 50 ms");
}

#[test]
fn the_experience_band_along_the_top_fills_as_far_as_the_player_is_through_its_level() {
    let mut h = Harness::new();
    let progress = Progress {
        level: Some(21),
        total_experience: Some(9_000_000),
        available_experience: Some(138_195),
        ..Progress::default()
    };
    let v = view(progress, None, None);
    let s = follow(&v, &[]);
    h.frame(&s);
    assert!(
        fills(&h, EXPERIENCE).is_empty(),
        "not until the scripts say how far"
    );
    let s = follow(
        &v,
        &[Notice {
            about: Some(PLAYER),
            said: Said::Player(PlayerNotice::Vital {
                vital: vital::EXPERIENCE,
                current: 750,
                max: 3_000,
            }),
        }],
    );
    let e = status::experience(&s);
    assert_eq!(e.level_text().as_deref(), Some("Lv21"));
    assert_eq!(e.experience_text(), "EXP 750/3,000");
    assert_eq!(e.to_spend_text().as_deref(), Some("Available XP 138,195"));
    h.frame(&s);
    let band = experience_rect(h.screen, h.k());
    let filled = fills(&h, EXPERIENCE);
    assert_eq!(filled.len(), 1, "{filled:?}");
    assert!(near(filled[0].x, band.x) && near(filled[0].y, band.y));
    assert!(
        near(filled[0].w, band.w / 4.0),
        "a quarter: {:?}",
        filled[0]
    );
    assert!(
        near(band.x + band.w / 2.0, h.screen.0 / 2.0),
        "centered along the top"
    );
    let target = target_gauge_rect(h.screen, h.k());
    assert!(
        band.bottom() < target.y - 28.0 * h.k(),
        "over the target's name"
    );
    assert!(
        band.right() < vitae_rect(h.screen, h.k()).x,
        "clear of the lamps"
    );
    h.input.mouse = centre(band);
    h.frame(&s);
    let tip = h.ui.tip().expect("a tooltip");
    assert_eq!(tip.title, "Experience");
    assert_eq!(
        tip.lines,
        ["750 / 3,000", "Total 9,000,000", "Available XP 138,195"]
    );
}

/// The vitae lamp shows below full vitae only, with the game's own words for the vitae (to a whole
/// percent, cut down) and the experience to its next point, and the vitae under it; a click on it
/// examines the vitae in the examine window, the selection let go. At full vitae nothing of it is
/// drawn and a click there asks for nothing.
///
/// Behaviour: desktop.horizon.the-vitae-lamp-gives-the-game-s-words-and-examines-the-vitae
#[test]
fn the_vitae_lamp_gives_the_game_s_words_and_a_click_below_full_examines_the_vitae() {
    let mut h = Harness::new();
    let full = follow(&view(Progress::default(), None, None), &[]);
    assert_eq!(
        vitae_words(&full.progress),
        "You are currently at full vitae."
    );
    let lamp = vitae_rect(h.screen, h.k());
    let out = h.click(&full, centre(lamp));
    assert!(
        !out.acts.iter().any(|a| matches!(a, Action::Examine { .. })),
        "{:?}",
        out.acts
    );
    assert!(!h.ui.examine.is_open());
    let under = Rect::new(lamp.x - 8.0, lamp.y, lamp.w + 16.0, lamp.h + 20.0 * h.k());
    let drawn = |h: &Harness| -> Vec<Quad> {
        h.list
            .quads
            .iter()
            .filter(|q| q.dst.x < under.right() && under.x < q.dst.right())
            .filter(|q| q.dst.y < under.bottom() && under.y < q.dst.bottom())
            .copied()
            .collect()
    };
    assert!(drawn(&h).is_empty(), "nothing at full: {:?}", drawn(&h));
    assert_eq!(h.ui.tip(), None);
    assert!(!h.ui.covers(centre(lamp).0, centre(lamp).1));

    let lost = follow(
        &view(
            Progress {
                vitae: Some(95.6),
                vitae_experience: Some(1_234),
                ..Progress::default()
            },
            None,
            None,
        ),
        &[],
    );
    assert_eq!(
        vitae_words(&lost.progress),
        "Vitae: 95% XP to recover next 1 point: 1234."
    );
    let out = h.click(&lost, centre(lamp));
    assert!(
        out.acts.contains(&Action::Examine {
            kind: examine::VITAE,
            data_enum: 0,
            item: 0,
            data_id: 0,
            selection: 0,
        }),
        "{:?}",
        out.acts
    );
    assert!(h.ui.examine.is_open());
    assert_eq!(VITAE_GROUND >> 24, 0xFF, "an opaque ground");
    assert_eq!(fills(&h, VITAE_GROUND), [lamp], "the lamp below full");
    assert!(h.ui.covers(centre(lamp).0, centre(lamp).1));
}

/// The art's vitae lamp is the heart, drawn opaque, untinted, below full vitae with the vitae
/// written under it, and not at all at full.
#[test]
fn with_its_art_the_vitae_lamp_is_the_heart_below_full_vitae_and_hidden_at_full() {
    let mut h = Harness::with_built_in_art();
    let lamp = vitae_rect(h.screen, h.k());
    let heart = |h: &Harness| {
        h.list
            .quads
            .iter()
            .filter(|q| q.tex.is_some() && q.dst == lamp)
            .map(|q| q.colour)
            .collect::<Vec<_>>()
    };
    h.frame(&follow(&view(Progress::default(), None, None), &[]));
    assert!(heart(&h).is_empty(), "hidden at full");
    let lost = Progress {
        vitae: Some(80.0),
        ..Progress::default()
    };
    h.frame(&follow(&view(lost, None, None), &[]));
    assert_eq!(heart(&h), [0xFFFF_FFFF]);
    let written = h
        .list
        .quads
        .iter()
        .filter(|q| q.tex.is_some() && q.dst.y >= lamp.bottom() - 4.0)
        .filter(|q| q.dst.x >= lamp.x - 10.0 && q.dst.right() <= lamp.right() + 10.0)
        .filter(|q| q.dst.bottom() <= lamp.bottom() + 20.0)
        .count();
    assert!(written >= 3, "80% under it: {written} glyphs");
}

/// In the HUD Layout editor the lamps' box takes in the vitae lamp's place at full vitae, where it
/// shows nothing.
#[test]
fn the_layout_editor_outlines_the_vitae_lamps_place_at_full_vitae() {
    let mut h = Harness::new();
    let full = follow(&view(Progress::default(), None, None), &[]);
    h.ui.layout.open_editor();
    h.frame(&full);
    let lamp = vitae_rect(h.screen, h.k());
    let (_, outline) =
        h.ui.layout
            .outlines
            .iter()
            .find(|(n, _)| *n == auberean_horizon::ui::layout::element::LAMP)
            .expect("the lamps outlined");
    assert!(
        outline.x <= lamp.x && outline.right() >= lamp.right(),
        "{outline:?} holds {lamp:?}"
    );
    assert!(outline.y <= lamp.y && outline.bottom() >= lamp.bottom());
}
