//! The HUD's elements kept apart: at the default layout, on the screens the interface is drawn
//! on and at each scale step, no element's outline meets another's (the chat entry closed or
//! open), the log window's lines stand whole within it, and the HUD layout editor writes each
//! element's name whole on the screen, clear of the elements.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::view::conflict;
use auberean_horizon::draw::Rect;
use auberean_horizon::state::pets::Pet;
use auberean_horizon::state::{Bar, Effect, GameState, Link, Slot, Target, Vital};
use auberean_horizon::ui::chat::Chat;
use auberean_horizon::ui::layout::element;

use crate::play::party::in_party;
use crate::Harness;

/// The screens each check is drawn on, and the scale step chosen there (none: Auto): a full HD
/// window, the 1280 by 960 window the interface was looked at in, and the smallest window that
/// offers each larger step, at that step.
pub fn screens() -> Vec<((f32, f32), Option<f32>)> {
    let mut out = vec![((1920.0, 1080.0), None), ((1280.0, 960.0), None)];
    for step in auberean_horizon::scale::STEPS {
        let (w, h) = auberean_horizon::scale::smallest_window(step);
        #[allow(clippy::cast_precision_loss)]
        out.push(((w as f32, h as f32), Some(step)));
    }
    out
}

/// A harness with the built-in art on `screen` at `step`.
pub fn on(screen: (f32, f32), step: Option<f32>) -> Harness {
    let mut h = Harness::with_built_in_art();
    h.screen = screen;
    h.ui.settings.own.scale = step;
    h
}

/// Everything the HUD shows at once: three in the fellowship, a target with its health, the
/// status row full, the conflict lamp and the vitae lamp below full, the experience band, the link
/// line, a four-row hotbar, the pack known, two pets and a mount, and a
/// log full of long lines.
pub fn full_hud() -> GameState {
    let mut s = in_party();
    s.hero = true;
    let full = Vital {
        current: 2000,
        max: 2000,
    };
    (s.health, s.vigor, s.focus) = (full, full, full);
    s.target = Some(Target {
        id: 7,
        name: "Wolfgang the Trader".into(),
        health: Some(full),
    });
    s.effects = (0..30)
        .map(|n| Effect {
            id: n,
            name: format!("Effect {n}"),
            icon: 0,
            left: Some(3599.0),
        })
        .collect();
    s.social.conflict = Some(conflict::KINGDOM_AGAINST_KINGDOM);
    s.bar = Some(Bar {
        version: 1,
        slots: vec![Slot::default(); 100],
        set: 0,
        rows: 4,
        locked: false,
        numbers: true,
    });
    s.goods.inventory = Some(auberean_client_sdk::contract::view::Inventory {
        version: 1,
        player: crate::PLAYER,
        money: 1_234_567,
        ..Default::default()
    });
    s.pets.pets = vec![
        Pet {
            id: 1,
            name: "Wolf".into(),
            ..Pet::default()
        },
        Pet {
            id: 2,
            name: "Bear".into(),
            ..Pet::default()
        },
    ];
    s.pets.mounted = true;
    s.experience = Vital {
        current: 218_435,
        max: 274_170,
    };
    s.progress = auberean_client_sdk::contract::view::Progress {
        level: Some(21),
        total_experience: Some(9_876_543),
        available_experience: Some(138_195),
        vitae: Some(95.0),
        vitae_experience: Some(1_234),
    };
    s.link = Link {
        world: Some("Ishgard".into()),
        clock: Some((21, 57)),
        round_trip_ms: Some(105.0),
        loss: 0.0,
        since_heard: Some(0.5),
        ..Link::default()
    };
    s.radar.coordinates = Some((32.1, -13.4));
    s.radar.show_coordinates = true;
    for n in 0..60 {
        s.say(
            0x10,
            format!(
                "Line {n}: you cannot use your basic attacks on this target, for 10 points of damage."
            ),
        );
    }
    s
}

/// Whether `a` and `b` share any area.
pub fn meet(a: &Rect, b: &Rect) -> bool {
    a.intersect(b).is_some()
}

#[test]
fn no_hud_element_meets_another_at_the_default_layout_on_any_screen_or_step() {
    for ((screen, step), typing) in screens()
        .into_iter()
        .flat_map(|at| [(at, false), (at, true)])
    {
        let mut h = on(screen, step);
        let s = full_hud();
        if typing {
            h.ui.chat.open("");
        }
        h.frame(&s);
        h.frame(&s);
        let outlines: Vec<(&str, Rect)> =
            h.ui.layout
                .outlines
                .iter()
                .filter(|(n, _)| *n != element::CROSS)
                .copied()
                .collect();
        for name in [
            element::PARAMETER,
            element::TARGET,
            element::STATUS,
            element::LAMP,
            element::PARTY,
            element::MENU,
            element::CHAT,
            element::HOTBAR,
            element::LINK,
            element::FOCUS,
            element::MINIMAP,
            element::PETS,
            element::DISMOUNT,
            element::EXPERIENCE,
        ] {
            assert!(
                outlines.iter().any(|(n, _)| *n == name),
                "{name} drawn on {screen:?}"
            );
        }
        let mut met = Vec::new();
        for (i, (a, ra)) in outlines.iter().enumerate() {
            for (b, rb) in &outlines[i + 1..] {
                if meet(ra, rb) {
                    met.push(format!("{a} {ra:?} meets {b} {rb:?}"));
                }
            }
        }
        assert!(
            met.is_empty(),
            "on {screen:?} at {step:?}, typing {typing}: {met:#?}"
        );
    }
}

#[test]
fn the_log_windows_lines_stand_whole_within_it_however_far_it_is_scrolled() {
    for (screen, step) in screens() {
        let mut h = on(screen, step);
        let s = full_hud();
        let k = h.ui.layout_scale(screen);
        let log = Chat::rect(screen, k);
        h.frame(&s);
        let panel = h.ui.chat.dock;
        for notches in [0.0, 1.0, 2.0, 5.0] {
            h.input.mouse = (log.x + log.w / 2.0, log.y + log.h / 2.0);
            h.input.wheel = notches;
            h.frame(&s);
            let lines: Vec<_> = h
                .list
                .quads
                .iter()
                .filter(|q| {
                    q.clip
                        .is_some_and(|c| meet(&c, &panel) && c.y < panel.y + 20.0 * k)
                })
                .collect();
            assert!(lines.len() > 50, "the lines drawn: {}", lines.len());
            // No line stands part out at the top; at the foot, the newest line's lowest letters'
            // edge may reach under the cut by a pixel or two, as Dereth's Horizon draws its log.
            for q in lines {
                let c = q.clip.expect("clipped");
                assert!(
                    q.dst.y > c.y - 1.0 && q.dst.bottom() < c.bottom() + 2.0,
                    "{:?} cut by {c:?} on {screen:?}, {notches} notches back",
                    q.dst
                );
            }
        }
    }
}

#[test]
fn the_layout_editor_names_each_element_whole_on_the_screen_and_clear_of_every_element() {
    for (screen, step) in screens() {
        let mut h = on(screen, step);
        let s = full_hud();
        h.ui.layout.open_editor();
        h.frame(&s);
        h.frame(&s);
        let outlines = h.ui.layout.outlines.clone();
        let labels = h.ui.layout.labels.clone();
        assert_eq!(labels.len(), outlines.len(), "every element named");
        for (name, l) in &labels {
            assert!(
                l.x >= 0.0 && l.y >= 0.0 && l.right() <= screen.0 && l.bottom() <= screen.1,
                "{name}'s name {l:?} off the screen {screen:?}"
            );
            for (other, r) in &outlines {
                assert!(
                    !meet(l, r),
                    "{name}'s name {l:?} over {other} {r:?} on {screen:?}"
                );
            }
        }
    }
}
