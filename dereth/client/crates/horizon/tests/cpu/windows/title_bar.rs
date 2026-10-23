//! Every window's title bar keeps one measure at every scale: as tall as its title needs with a
//! little room, the window dragged by it, its close button taking a press over the bar's end. A
//! window is as tall as its contents with the bar's room over them and its frame under them.
//!
//! Behaviour: none (experimental Horizon interface)

use std::sync::Arc;

use dereth_horizon::art::Art;
use dereth_horizon::draw::Rect;
use dereth_horizon::options::HorizonOptions;
use dereth_horizon::pieces::Pieces;
use dereth_horizon::ui::game::GameState;
use dereth_horizon::ui::kit::{BODY_TOP, TITLE_BAR};
use dereth_horizon::ui::panels::WindowId;
use dereth_horizon::ui::HorizonUi;

use crate::Harness;

/// The scales checked, each with a window that offers it.
const SCALES: [(f32, (f32, f32)); 4] = [
    (1.0, (1920.0, 1080.0)),
    (1.25, (1920.0, 1080.0)),
    (1.5, (2560.0, 1440.0)),
    (2.0, (3840.0, 2160.0)),
];

fn world() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        ..GameState::default()
    }
}

/// The interface at `scale` on a window `screen` pixels, with its own art, the inventory open.
fn inventory_at(scale: f32, screen: (f32, f32)) -> (Harness, Rect) {
    let pieces = Pieces::built_in().expect("the pieces built in");
    let mut h = Harness {
        screen,
        ..Harness::new(HorizonOptions {
            scale: Some(scale),
            ..HorizonOptions::default()
        })
    };
    h.ui = HorizonUi::new(
        Arc::new(Art::new(Arc::new(pieces))),
        HorizonOptions {
            scale: Some(scale),
            ..HorizonOptions::default()
        },
    );
    h.move_to(-50.0, -50.0);
    let state = world();
    h.frame(&state);
    h.ui.windows.open(WindowId::Inventory, -10.0);
    h.frame(&state);
    assert!((h.ui.layout_scale(screen) - scale).abs() < 1e-6, "{scale}");
    let rect =
        h.ui.windows
            .rects(scale)
            .into_iter()
            .find(|(id, _)| *id == WindowId::Inventory)
            .expect("the inventory is open")
            .1;
    (h, rect)
}

#[test]
fn the_title_bar_is_one_measure_at_every_scale_and_holds_its_title() {
    for (scale, screen) in SCALES {
        let (h, rect) = inventory_at(scale, screen);
        let bar = TITLE_BAR * scale;
        // The frame's top-left corner piece: the title bar's height.
        let corner = h
            .list
            .quads
            .iter()
            .find(|q| {
                q.tex.is_some()
                    && (q.dst.x - rect.x).abs() < 0.5
                    && (q.dst.y - rect.y).abs() < 0.5
                    && q.dst.h < rect.h / 2.0
            })
            .unwrap_or_else(|| panic!("{scale}: the frame's corner"));
        assert!(
            (corner.dst.h - bar).abs() < 0.01,
            "{scale}: the title bar is {} pixels tall, not {bar}",
            corner.dst.h
        );
        // The title's letters, in the middle of the window's width, inside the bar.
        let middle = rect.x + rect.w / 2.0;
        let title: Vec<Rect> = h
            .list
            .quads
            .iter()
            .filter(|q| {
                q.tex.is_some()
                    && q.dst.y >= rect.y
                    && q.dst.bottom() <= rect.y + bar * 1.5
                    && (q.dst.x - middle).abs() < 60.0 * scale
                    && q.dst.w < 30.0 * scale
            })
            .map(|q| q.dst)
            .collect();
        assert!(title.len() >= 9, "{scale}: the title's letters");
        for g in &title {
            assert!(
                g.y >= rect.y && g.bottom() <= rect.y + bar,
                "{scale}: a letter at {g:?} outside the bar from {} to {}",
                rect.y,
                rect.y + bar
            );
        }
    }
}

#[test]
fn a_window_is_dragged_by_its_title_bar_and_not_from_below_it() {
    for (scale, screen) in SCALES {
        let bar = TITLE_BAR * scale;
        let state = world();
        // A press just inside the bar's foot takes the window with it.
        let (mut h, rect) = inventory_at(scale, screen);
        let at = (rect.x + rect.w * 0.3, rect.y + bar - 2.0);
        h.move_to(at.0, at.1);
        h.press();
        h.frame(&state);
        h.move_to(at.0 - 40.0, at.1 + 30.0);
        h.frame(&state);
        h.release();
        h.frame(&state);
        let moved = h.ui.windows.state(WindowId::Inventory).pos.expect("placed");
        assert_eq!(
            moved,
            (rect.x - 40.0, rect.y + 30.0),
            "{scale}: dragged by the bar"
        );
        // A press just under it does not.
        let (mut h, rect) = inventory_at(scale, screen);
        let at = (rect.x + rect.w * 0.3, rect.y + bar + 2.0);
        h.move_to(at.0, at.1);
        h.press();
        h.frame(&state);
        h.move_to(at.0 - 40.0, at.1 + 30.0);
        h.frame(&state);
        h.release();
        h.frame(&state);
        let still = h.ui.windows.state(WindowId::Inventory).pos.expect("placed");
        assert_eq!(
            still,
            (rect.x, rect.y),
            "{scale}: not dragged from below the bar"
        );
    }
}

/// The room the character window gives its contents, in layout units: the figure with its slots,
/// and the sheet beside them.
const CHARACTER_CONTENTS: f32 = 584.0;

/// The foot of the frame drawn with no art of its own, under a window's contents, in layout
/// units.
const PLAIN_FOOT: f32 = 10.0;

/// How far in from the left of the frame drawn with no art of its own a window's contents start,
/// and the width of the character window's column for the figure, in layout units.
const PLAIN_SIDE: f32 = 12.0;
const DOLL_COLUMN: f32 = 384.0;

#[test]
fn a_window_opens_as_tall_as_its_contents_with_its_title_bar_over_them_and_its_frame_under_them() {
    for (scale, screen) in SCALES {
        let mut h = Harness {
            screen,
            ..Harness::new(HorizonOptions {
                scale: Some(scale),
                ..HorizonOptions::default()
            })
        };
        h.move_to(-50.0, -50.0);
        let state = world();
        h.frame(&state);
        h.ui.windows.open(WindowId::Character, -10.0);
        h.frame(&state);
        assert!((h.ui.layout_scale(screen) - scale).abs() < 1e-6, "{scale}");
        let rect =
            h.ui.windows
                .rects(scale)
                .into_iter()
                .find(|(id, _)| *id == WindowId::Character)
                .expect("the character window is open")
                .1;
        let tall = (CHARACTER_CONTENTS + BODY_TOP + PLAIN_FOOT) * scale;
        assert!(
            (rect.h - tall).abs() < 0.01,
            "{scale}: the window is {} pixels tall, not {tall}",
            rect.h
        );
        // The contents fill the room between the two: the figure's column, its slots from the
        // row over the figure to what is held at its foot, stands in the middle of that room.
        let slots: Vec<Rect> = h
            .input
            .nav
            .last
            .targets
            .iter()
            .map(|t| t.rect)
            .filter(|r| {
                r.x >= rect.x
                    && r.right() <= rect.x + (PLAIN_SIDE + DOLL_COLUMN) * scale
                    && r.y >= rect.y + TITLE_BAR * scale
                    && r.bottom() <= rect.bottom()
            })
            .collect();
        assert!(slots.len() >= 10, "{scale}: the figure's slots");
        let top = slots.iter().map(|r| r.y).fold(f32::MAX, f32::min);
        let foot = slots.iter().map(Rect::bottom).fold(f32::MIN, f32::max);
        let above = top - (rect.y + BODY_TOP * scale);
        let below = rect.bottom() - PLAIN_FOOT * scale - foot;
        assert!(
            above > 0.0 && (above - below).abs() < 0.01,
            "{scale}: the figure's column has {above} pixels over it and {below} under it"
        );
    }
}

#[test]
fn the_close_button_takes_a_press_over_the_end_of_the_title_bar() {
    for (scale, screen) in SCALES {
        let bar = TITLE_BAR * scale;
        let state = world();
        // Its middle, and near the bar's top right corner.
        for (dx, dy) in [(bar / 2.0, bar / 2.0), (3.0 * scale, 3.0 * scale)] {
            let (mut h, rect) = inventory_at(scale, screen);
            h.move_to(rect.right() - dx, rect.y + dy);
            h.frame(&state);
            h.press();
            h.frame(&state);
            h.release();
            h.frame(&state);
            assert!(
                !h.ui.windows.is_open(WindowId::Inventory),
                "{scale}: a press {dx} from the right and {dy} down closes the window"
            );
        }
    }
}
