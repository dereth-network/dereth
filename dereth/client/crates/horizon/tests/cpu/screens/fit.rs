//! Every screen stands whole on the smallest windows each interface scale is offered on, and on
//! the monitors a player has it on, full screen and maximised: the HUD's parts apart and on the
//! screen with a whole fellowship (at rest, fighting with magic, and in gamepad mode with a cross
//! hotbar held up), character select's parts apart and on it, every window opened on it, and
//! nothing drawn off it. Drawn with the interface's own art, as a player sees it.
//!
//! Behaviour: none (experimental Horizon interface)

use dereth_horizon::draw::Rect;
use dereth_horizon::options::{HorizonOptions, StartScreen};
use dereth_horizon::scale;
use dereth_horizon::ui::game::{CharacterEntry, GameState};
use dereth_horizon::ui::panels::WindowId;
use dereth_primitives::ObjectId;

use std::sync::Arc;

use crate::Harness;

/// The windows checked at each step: the smallest it is offered on, a 16:9 one that short, and
/// the monitors a player has it on, full screen and maximised.
fn windows(step: f32) -> Vec<(f32, f32)> {
    let (w, h) = scale::smallest_window(step);
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (w as f32, h as f32);
    let mut out = vec![(w, h), ((h * 16.0 / 9.0).ceil(), h)];
    out.extend_from_slice(match step {
        s if s < 1.75 => &[
            (2560.0, 1440.0),
            (2560.0, 1377.0),
            (2560.0, 1600.0),
            (2736.0, 1824.0),
        ][..],
        s if s < 2.5 => &[(3840.0, 2160.0), (3840.0, 2097.0), (2880.0, 1800.0)][..],
        _ => &[(5120.0, 2880.0)][..],
    });
    out
}

/// A harness over the interface's own art, on a window of `screen` pixels.
fn harness(art: &Art, options: HorizonOptions, screen: (f32, f32)) -> Harness {
    let mut h = Harness::new(options.clone());
    h.ui = dereth_horizon::ui::HorizonUi::new(Arc::clone(art), options);
    h.screen = screen;
    h
}

/// The interface's own art, read once.
type Art = Arc<dereth_horizon::art::Art>;

fn apart(a: &Rect, b: &Rect) -> bool {
    a.intersect(b).is_none_or(|r| r.w <= 0.5 || r.h <= 0.5)
}

fn on_screen(r: &Rect, screen: (f32, f32)) -> bool {
    r.x >= -0.5 && r.y >= -0.5 && r.right() <= screen.0 + 0.5 && r.bottom() <= screen.1 + 0.5
}

/// That nothing of this frame is drawn off the screen, but light that spreads out and fades: the
/// glow of a cross hotbar held up.
fn assert_all_drawn_on_screen(h: &Harness, what: &str) {
    let glow =
        h.ui.art
            .piece("cross.glow", h.ui.layout_scale(h.screen))
            .expect("the cross hotbars' glow");
    let off: Vec<Rect> = h
        .list
        .quads
        .iter()
        .filter(|q| q.turn == 0.0)
        .filter(|q| !(q.tex == Some(glow.tex) && q.src == glow.src))
        .filter_map(|q| q.clip.map_or(Some(q.dst), |c| q.dst.intersect(&c)))
        .filter(|r| !on_screen(r, h.screen))
        .collect();
    assert!(off.is_empty(), "{what}: drawn off the screen: {off:?}");
}

/// That the parts named are apart and on the screen.
fn assert_apart_and_on_screen(parts: &[(&str, Rect)], screen: (f32, f32), what: &str) {
    for (i, (a, ra)) in parts.iter().enumerate() {
        assert!(on_screen(ra, screen), "{what}: {a} off the screen: {ra:?}");
        for (b, rb) in &parts[i + 1..] {
            assert!(apart(ra, rb), "{what}: {a} over {b}: {ra:?} {rb:?}");
        }
    }
}

/// The sample player in the world, with a whole fellowship of nine.
fn in_the_world() -> GameState {
    let mut state = GameState::preview();
    state.host = "127.0.0.1:9000".into();
    state.connected = true;
    state.in_world = true;
    while state.fellowship.len() < 9 {
        let mut fellow = state.fellowship[0].clone();
        fellow.0 = format!("Fellow {}", state.fellowship.len());
        state.fellowship.push(fellow);
    }
    state
}

/// An account of eleven characters, with news longer than its panel.
fn account() -> GameState {
    GameState {
        connected: true,
        connect_phase: dereth_horizon::ui::game::ConnectPhase::Ready,
        host: "127.0.0.1:9000".into(),
        world: Some("Test".into()),
        character_slots: 11,
        world_message: Some("Welcome to the world, with a long line of news. ".repeat(40)),
        characters: (0..11)
            .map(|n| CharacterEntry {
                id: ObjectId(0x5000_0001 + n),
                name: format!("Character {n}"),
                delete_seconds: 0,
            })
            .collect(),
        full_screen: Some(false),
        known_looks: vec![ObjectId(0x5000_0001)],
        ..GameState::default()
    }
}

#[test]
fn every_screen_stands_whole_on_the_smallest_windows_each_scale_is_offered_on() {
    let mut cases = vec![((1920.0, 1080.0), 1.0)];
    for step in [1.5, 2.0, 3.0] {
        cases.extend(windows(step).into_iter().map(|w| (w, step)));
    }
    let pieces = dereth_horizon::pieces::Pieces::built_in().expect("the built-in art");
    let art: Art = Arc::new(dereth_horizon::art::Art::new(Arc::new(pieces)));
    for (screen, step) in cases {
        let what = format!("{screen:?} at {}", scale::label(step));
        assert!(scale::offered(step, screen), "{what} is offered");
        let options = HorizonOptions {
            screen: StartScreen::Game,
            scale: Some(step),
            ..HorizonOptions::default()
        };
        let state = in_the_world();
        let mut magic = state.clone();
        magic.combat_mode = dereth_client_contract::combat_mode::MAGIC;
        magic.spell_tab_keys = ("Ctrl+Shift+[".into(), "Ctrl+Shift+]".into());

        // The HUD, at rest and fighting with magic.
        for (mode, state) in [("at rest", &state), ("with magic", &magic)] {
            let mut h = harness(&art, options.clone(), screen);
            for _ in 0..3 {
                h.frame(state);
            }
            assert!((h.ui.layout_scale(screen) - step).abs() < 1e-4, "{what}");
            let parts: Vec<(&str, Rect)> = h.ui.hud.layout.outlines.clone();
            assert!(parts.len() >= 10, "{what}: {parts:?}");
            assert_apart_and_on_screen(&parts, screen, &format!("{what}, the HUD {mode}"));
            assert_all_drawn_on_screen(&h, &format!("{what}, the HUD {mode}"));
        }

        // In gamepad mode, with the left cross hotbar held up: the cross hotbars stand clear of
        // the log window.
        let mut h = harness(&art, options.clone(), screen);
        for _ in 0..3 {
            h.input.pad.mode = Some(dereth_horizon::ui::input::PadHints::World);
            h.input.pad.set = Some(dereth_horizon::pad::CrossSet::Left);
            h.frame(&state);
        }
        let k = h.ui.layout_scale(screen);
        let slots: Vec<Rect> = h.ui.hud.cross.drawn.iter().map(|(r, _)| *r).collect();
        assert!(!slots.is_empty(), "{what}: the cross hotbars");
        let left = slots.iter().map(|r| r.x).fold(f32::MAX, f32::min) - 8.0 * k;
        let top = slots.iter().map(|r| r.y).fold(f32::MAX, f32::min) - 8.0 * k;
        let right = slots.iter().map(Rect::right).fold(f32::MIN, f32::max) + 8.0 * k;
        let bottom = slots.iter().map(Rect::bottom).fold(f32::MIN, f32::max) + 8.0 * k;
        let mut parts: Vec<(&str, Rect)> = h.ui.hud.layout.outlines.clone();
        parts.push(("cross", Rect::new(left, top, right - left, bottom - top)));
        let parts: Vec<(&str, Rect)> = parts
            .into_iter()
            .filter(|(name, _)| ["chat", "party", "menu", "cross"].contains(name))
            .collect();
        assert_apart_and_on_screen(&parts, screen, &format!("{what}, gamepad mode"));
        assert_all_drawn_on_screen(&h, &format!("{what}, gamepad mode"));

        // Every window opens on the screen.
        for window in [
            WindowId::Options,
            WindowId::Inventory,
            WindowId::Character,
            WindowId::Actions,
            WindowId::Map,
            WindowId::Social,
            WindowId::Journal,
            WindowId::Vitae,
        ] {
            let mut h = harness(&art, options.clone(), screen);
            h.frame(&state);
            h.ui.windows.open(window, -1.0);
            for _ in 0..3 {
                h.frame(&state);
            }
            for (id, r) in h.ui.windows.rects(k) {
                assert!(on_screen(&r, screen), "{what}: {id:?} at {r:?}");
            }
            assert_all_drawn_on_screen(&h, &format!("{what}, {window:?}"));
        }

        // Character select.
        let mut h = harness(
            &art,
            HorizonOptions {
                scale: Some(step),
                ..HorizonOptions::default()
            },
            screen,
        );
        let state = account();
        for _ in 0..3 {
            h.frame(&state);
        }
        let l = dereth_horizon::ui::pregame::LobbyLayout::new(screen, k);
        let switch = Rect::new(screen.0 - 190.0 * k, 22.0 * k, 150.0 * k, 30.0 * k);
        assert_apart_and_on_screen(
            &[
                ("news", l.news),
                ("list", l.list),
                ("log in", l.log_in),
                ("create", l.create),
                ("delete", l.delete),
                ("exit", l.exit),
                ("switch", switch),
            ],
            screen,
            &format!("{what}, character select"),
        );
        assert!(
            l.doll.w >= 300.0 * k && on_screen(&l.doll, screen),
            "{what}: the character's room {:?}",
            l.doll
        );
        assert_all_drawn_on_screen(&h, &format!("{what}, character select"));
    }
}
