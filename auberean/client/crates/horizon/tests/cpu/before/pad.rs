//! The pad on the screens before the world: the focus moved over character select's cards and
//! controls by the d-pad, A pressing what it is on, Start as Enter.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_horizon::gamepad::PadMode;
use auberean_horizon::ui::pregame::{Confirm, SelectLayout};
use auberean_shell::pad::{PadButton as B, PadState};

use super::{frame, listed, ROWAN};
use crate::{centre, Harness};

const FRAME: f64 = 1.0 / 60.0;

/// One frame of the pad, then the screen's.
fn step(h: &mut Harness, mode: &mut PadMode, pad: PadState) {
    mode.screens(Some(pad), FRAME, &mut h.input);
    frame(h, &listed());
}

#[test]
fn the_d_pad_moves_the_focus_onto_a_card_a_selects_it_and_start_asks_to_enter() {
    let mut h = Harness::new();
    let mut mode = PadMode::default();
    frame(&mut h, &listed());
    step(&mut h, &mut mode, PadState::holding(&[B::South]));
    assert!(mode.on, "a press switched gamepad mode on");
    step(&mut h, &mut mode, PadState::default());
    let l = SelectLayout::new(h.screen, h.k());
    let elm = l.card(1, 0.0, h.k());
    let rowan = l.card(0, 0.0, h.k());
    // Down and up over the screen until the focus rests on Elm's card.
    let mut reached = false;
    for dir in [B::DPadDown, B::DPadUp, B::DPadLeft, B::DPadRight]
        .iter()
        .cycle()
        .take(40)
    {
        if elm.contains(h.input.mouse.0, h.input.mouse.1) {
            reached = true;
            break;
        }
        step(&mut h, &mut mode, PadState::holding(&[*dir]));
        step(&mut h, &mut mode, PadState::default());
    }
    assert!(reached, "the focus reached a card: at {:?}", h.input.mouse);
    step(&mut h, &mut mode, PadState::holding(&[B::South]));
    step(&mut h, &mut mode, PadState::default());
    assert_eq!(
        h.ui.pregame.selected, 1,
        "A selected the card under the focus"
    );
    // Up to Rowan's card, A on it, then Start asks to enter with it.
    step(&mut h, &mut mode, PadState::holding(&[B::DPadUp]));
    step(&mut h, &mut mode, PadState::default());
    assert!(
        rowan.contains(h.input.mouse.0, h.input.mouse.1),
        "up from Elm is Rowan: {:?}",
        centre(rowan)
    );
    step(&mut h, &mut mode, PadState::holding(&[B::South]));
    step(&mut h, &mut mode, PadState::default());
    assert_eq!(h.ui.pregame.selected, 0);
    step(&mut h, &mut mode, PadState::holding(&[B::Start]));
    assert!(matches!(
        h.ui.pregame.confirm,
        Some(Confirm::Enter(ROWAN, _))
    ));
}
