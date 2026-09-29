//! The five chat windows bind their elements and read layout opacities; the shipped layout carries
//! neither opacity attribute; stored chat options reach the windows' alpha and the fade settles;
//! mouse arrival fades up to the active opacity.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_primitives::LocalTime;
use dereth_ui::framework::Screen;
use dereth_ui::{Delivery, ElemHandle, UiSystem};

use dereth_ui_screens::chat::interface::{opacity_attr, window as chatwin, FADE_STEP_FRACTION};
use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};

// ---------------------------------------------------------------------------------------------
// Literals, not symbols
// ---------------------------------------------------------------------------------------------

/// The two opacity attribute ids a layout can carry, and the 1.0 fallback when it carries neither.
const DEFAULT_OPACITY_ATTR: u32 = 0x1000_0080;
const ACTIVE_OPACITY_ATTR: u32 = 0x1000_0081;
const FALLBACK: f32 = 1.0;
/// The fade moves 5 % of the travel per frame.
const STEP_FRACTION: f32 = 0.05;

fn env() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui
}

fn screen() -> (UiSystem, GamePlayScreen) {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    let _ = ui.drain_outbox();
    ui.requests.clear();
    (ui, s)
}

/// One production frame:, whose last step broadcasts
/// global message 3, then the queued external deliveries — which is `UiFlow::frame`'s own body.
///
/// Nothing here calls the fade directly: if the screen were not a registered message-3 listener,
/// no `Delivery::Global` would arrive and every assertion below would fail.
fn frame(ui: &mut UiSystem, s: &mut GamePlayScreen, t: f64) {
    ui.use_time(LocalTime(t), &mut dereth_ui::NullInputPump);
    for d in ui.drain_outbox() {
        match d {
            Delivery::Global { id, param, .. } => {
                s.on_global_message(&mut dereth_ui::framework::ScreenCx::new(ui), id, param)
            }
            Delivery::Element { msg, .. } => {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg)
            }
            Delivery::Notice { .. } => {}
        }
    }
}

fn alpha(ui: &UiSystem, h: ElemHandle) -> f32 {
    ui.material_opacity(h)
}

fn main_window(s: &GamePlayScreen) -> ElemHandle {
    s.chat_windows[0]
        .root
        .expect("the main chat window's element")
}

// ---------------------------------------------------------------------------------------------
// 1. The binding and `on_set_attribute`
// ---------------------------------------------------------------------------------------------

/// The five windows bind their own element and read the layout opacities.
#[test]
fn the_five_windows_bind_their_own_element_and_read_the_layout_opacities() {
    let (ui, s) = screen();
    assert_eq!(s.chat.len(), 5, "main plus four floaties");
    assert_eq!(s.chat_windows.len(), 5);
    assert_eq!(
        s.chat.iter().map(|c| c.window_id).collect::<Vec<_>>(),
        vec![
            chatwin::MAIN,
            chatwin::FLOATY_1,
            chatwin::FLOATY_2,
            chatwin::FLOATY_3,
            chatwin::FLOATY_4
        ]
    );

    // The attribute numbers the client compares against, written as literals.
    assert_eq!(opacity_attr::DEFAULT, DEFAULT_OPACITY_ATTR);
    assert_eq!(opacity_attr::ACTIVE, ACTIVE_OPACITY_ATTR);
    assert_eq!(opacity_attr::FALLBACK, FALLBACK);
    assert!((FADE_STEP_FRACTION - STEP_FRACTION).abs() < 1e-9);

    for (i, w) in s.chat_windows.iter().enumerate() {
        let root = w
            .root
            .unwrap_or_else(|| panic!("chat window {i} has no element"));
        let iface = &s.chat[i];
        // Whatever the shipped layout says, the invariant the two setters exist to keep must
        // hold and the element must carry the idle value.
        assert!(
            iface.default_opacity <= iface.active_opacity,
            "window {i}: the opacity setters keep default <= active, got {} / {}",
            iface.default_opacity,
            iface.active_opacity
        );
        assert!(
            (alpha(&ui, root) - iface.default_opacity).abs() < 1e-6,
            "window {i}: setting the default opacity must have reached the element"
        );
        assert!((iface.current_opacity - iface.default_opacity).abs() < 1e-6);
    }
    // The main chat window's element is the one the gameplay layout names, not some inner child.
    assert_eq!(
        ui.node(main_window(&s)).expect("live").element_id(),
        window::MAIN_CHAT
    );
}

/// The shipped layout carries neither opacity attribute and the arm still works.
#[test]
fn the_shipped_layout_carries_neither_opacity_attribute_and_the_arm_still_works() {
    let (mut ui, mut s) = screen();
    let root = main_window(&s);

    for (i, w) in s.chat_windows.iter().enumerate() {
        let h = w.root.expect("bound");
        let n = ui.node(h).expect("live");
        assert_eq!(
            n.merged_properties().get_float(DEFAULT_OPACITY_ATTR),
            None,
            "window {i} unexpectedly carries 0x10000080; the fallback claim below is then wrong"
        );
        assert_eq!(n.merged_properties().get_float(ACTIVE_OPACITY_ATTR), None);
        assert!((s.chat[i].default_opacity - FALLBACK).abs() < 1e-6);
        assert!((s.chat[i].active_opacity - FALLBACK).abs() < 1e-6);
    }

    // Now give the main window the attributes a layout would, and run `on_set_attribute`'s pair.
    if let Some(n) = ui.node_mut(root) {
        n.instance_properties.set(
            DEFAULT_OPACITY_ATTR,
            dereth_assets::ui::PropertyValue::Float(0.3),
        );
        n.instance_properties.set(
            ACTIVE_OPACITY_ATTR,
            dereth_assets::ui::PropertyValue::Float(0.8),
        );
    }
    let w = s.chat_windows[0];
    let (d, a) = w.read_opacity_attributes(&mut ui, &mut s.chat[0]);
    assert!(
        (d - 0.3).abs() < 1e-6,
        "the default-opacity attribute 0x10000080 was taken, got {d}"
    );
    assert!(
        (a - 0.8).abs() < 1e-6,
        "the active-opacity attribute 0x10000081 was taken, got {a}"
    );
    assert!(
        (alpha(&ui, root) - 0.3).abs() < 1e-6,
        "…and put the idle value on the element, got {}",
        alpha(&ui, root)
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The stored options, the fade, and the unsubscribe
// ---------------------------------------------------------------------------------------------

/// Behaviour: chat.window.how-solid-it-is-follows-the-slider-at-once-and-is-worn-from-login
/// The stored chat options reach the windows alpha and the fade settles.
#[test]
fn the_stored_chat_options_reach_the_windows_alpha_and_the_fade_settles() {
    let (mut ui, mut s) = screen();
    let root = main_window(&s);

    // The pointer over the window, and a frame so `do_mouse_update` has set the last-entered
    // element before the two setters ask `fade_engaged`.
    let b = ui.screen_box(root);
    let (cx, cy) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    ui.mouse_move(LocalTime(0.0), cx, cy);
    frame(&mut ui, &mut s, 0.0);

    // The floaty-chat update's opacity third, as the host hands it
    // down: idle 0.4, active 0.9.
    let n = s.chat_set_stored_opacity(&mut ui, Some(0.4), Some(0.9));
    assert_eq!(
        n, 5,
        "the two sliders are general, so every window takes them"
    );
    assert!((s.chat[0].default_opacity - 0.4).abs() < 1e-6);
    assert!((s.chat[0].active_opacity - 0.9).abs() < 1e-6);
    assert!(
        (alpha(&ui, root) - 0.9).abs() < 1e-6,
        "the active-opacity setter applies at once to an engaged window, got {}",
        alpha(&ui, root)
    );

    let step = (0.9f32 - 0.4) * STEP_FRACTION;
    assert!(
        (step - 0.025).abs() < 1e-6,
        "|0.9-0.4|*0.05 = 0.025, got {step}"
    );

    // The pointer leaves, so the window is idle and fades **down**.
    ui.mouse_move(LocalTime(1.0), 4, 4);
    let start = alpha(&ui, root);
    frame(&mut ui, &mut s, 1.0);
    let after_one = alpha(&ui, root);
    assert!(
        after_one < start,
        "one frame of global message 3 must move the element's alpha: {start} -> {after_one}"
    );
    assert!(
        (start - after_one - step).abs() < 1e-5,
        "one step is |active-default|*0.05 = {step}, moved {}",
        start - after_one
    );

    // …and it converges on the idle value and stops there.
    for i in 2..64 {
        frame(&mut ui, &mut s, f64::from(i));
    }
    assert!(
        (alpha(&ui, root) - 0.4).abs() < 1e-5,
        "the fade must settle on the default opacity, got {}",
        alpha(&ui, root)
    );
    assert!(
        !s.chat[0].fading,
        "unregistering global message 3 — an idle window costs nothing"
    );
    // Every window, not only the main one.
    for (i, w) in s.chat_windows.iter().enumerate() {
        let h = w.root.expect("bound");
        assert!(
            (alpha(&ui, h) - 0.4).abs() < 1e-5,
            "window {i} is at {}",
            alpha(&ui, h)
        );
    }

    // A further frame writes nothing at all — the observable form of the unsubscribe.
    let before = alpha(&ui, root);
    assert!(
        s.chat_fade_tick(&mut ui).is_empty(),
        "a settled window reports no step"
    );
    assert!((alpha(&ui, root) - before).abs() < 1e-9);
}

/// Behaviour: chat.window.the-mouse-arriving-fades-it-up-to-the-active-opacity
/// The mouse arriving fades the window back up to the active opacity.
#[test]
fn the_mouse_arriving_fades_the_window_back_up_to_the_active_opacity() {
    let (mut ui, mut s) = screen();
    let root = main_window(&s);
    s.chat_set_stored_opacity(&mut ui, Some(0.4), Some(1.0));

    // Settle it at the idle value first, with the pointer away.
    ui.mouse_move(LocalTime(0.0), 4, 4);
    for i in 1..64 {
        frame(&mut ui, &mut s, f64::from(i));
    }
    assert!((alpha(&ui, root) - 0.4).abs() < 1e-5);
    assert!(!s.chat[0].fading);

    // Now the pointer arrives over the window. `UiSystem::do_mouse_update` is what sets
    // the last-entered element, and it runs inside `use_time` — so the very same frame that
    // notices the pointer is the one that re-arms the fade.
    let b = ui.screen_box(root);
    let (cx, cy) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    ui.mouse_move(LocalTime(64.0), cx, cy);
    frame(&mut ui, &mut s, 64.0);
    let over = ui.mouse_over().expect("the pointer is over something");
    assert!(
        over == root || ui.is_ancestor_of(root, over),
        "the pointer must be over the chat window for this test to mean anything"
    );
    assert!(
        s.chat[0].fading,
        "the engagement edge re-subscribes to global message 3"
    );

    for i in 65..130 {
        frame(&mut ui, &mut s, f64::from(i));
    }
    assert!(
        (alpha(&ui, root) - 1.0).abs() < 1e-5,
        "an engaged window fades up to the active opacity, got {}",
        alpha(&ui, root)
    );
    assert!(!s.chat[0].fading, "and unsubscribes again");
}
