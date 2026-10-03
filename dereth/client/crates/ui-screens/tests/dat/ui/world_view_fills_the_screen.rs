//! The 3D view fills any screen: on a 5120 by 2160 screen the world view covers the whole screen
//! and the root lets the world through, a resolution change keeps it whole, and its minimum size
//! and every other element's limits still hold.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_ui::framework::{Screen, ScreenCx};
use dereth_ui::UiSystem;
use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};

fn screen(display: (i32, i32)) -> (UiSystem, GamePlayScreen) {
    let (mut ui, _flow, _store) =
        crate::common::layout::load(display, RegistrationOrder::BeforeResolver);
    let mut s = GamePlayScreen::default();
    s.create(&mut ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    (ui, s)
}

/// Behaviour: ui.world-view.the-world-view-fills-any-screen
#[test]
fn the_world_view_fills_a_5120_by_2160_screen() {
    let (mut ui, s) = screen((5120, 2160));
    let root = *s.roots().first().expect("the gameplay screen has a root");
    let sbox = ui
        .get_child_recursive(root, window::SMART_BOX)
        .expect("the gameplay screen has its world view");
    let b = ui.screen_box(sbox);
    assert_eq!(
        (b.x0, b.y0, b.width(), b.height()),
        (0, 0, 5120, 2160),
        "the world view stops short of the screen"
    );
    assert!(
        ui.node(root).is_some_and(|n| n.region.flags.transparent),
        "a world view covering the screen lets the world through the root"
    );

    // A resolution change keeps it whole.
    ui.refresh_event((7680, 2160));
    let b = ui.screen_box(sbox);
    assert_eq!((b.x0, b.y0, b.width(), b.height()), (0, 0, 7680, 2160));

    // Its minimum size stays.
    ui.resize_to(sbox, 100, 100);
    let b = ui.screen_box(sbox);
    assert_eq!((b.width(), b.height()), (490, 300));

    // The lift is the world view's alone: a maximum width still binds any other element.
    let other = ui.create_hollow(Some(root));
    ui.set_attribute_int(other, dereth_ui::props::attr::MAX_WIDTH, 3000);
    ui.resize_to(other, 5120, 100);
    assert_eq!(ui.screen_box(other).width(), 3000);
}
