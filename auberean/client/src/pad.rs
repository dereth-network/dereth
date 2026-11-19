//! The pads, read through the system's XInput on Windows: the one last used, as the shell's pad.
//! Elsewhere the desktop reads none yet.

use auberean_shell::pad::PadState;

/// The desktop's pad: on Windows the pad library, opened the first time it is read; elsewhere
/// nothing.
#[derive(Default)]
pub struct DesktopPad {
    #[cfg(windows)]
    pads: Option<Pads>,
}

impl std::fmt::Debug for DesktopPad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopPad").finish_non_exhaustive()
    }
}

/// The pad library, and the pad last used.
#[cfg(windows)]
struct Pads {
    /// `None` once it has failed to open, so it is not tried every frame.
    gilrs: Option<gilrs::Gilrs>,
    last: Option<gilrs::GamepadId>,
}

impl DesktopPad {
    /// The pad as it is now; none with none connected.
    #[cfg(windows)]
    pub fn poll(&mut self) -> Option<PadState> {
        let pads = self.pads.get_or_insert_with(|| Pads {
            gilrs: match gilrs::Gilrs::new() {
                Ok(g) => Some(g),
                Err(e) => {
                    tracing::warn!("no pads: {e}");
                    None
                }
            },
            last: None,
        });
        let gilrs = pads.gilrs.as_mut()?;
        // The library's state moves only as its events are taken; the pad last heard from is
        // the one read.
        while let Some(event) = gilrs.next_event() {
            pads.last = Some(event.id);
        }
        let id = pads
            .last
            .filter(|id| gilrs.connected_gamepad(*id).is_some())
            .or_else(|| gilrs.gamepads().next().map(|(id, _)| id))?;
        pads.last = Some(id);
        Some(read(&gilrs.gamepad(id)))
    }

    /// The pad as it is now: none, off Windows.
    #[cfg(not(windows))]
    #[allow(clippy::unused_self)]
    pub fn poll(&mut self) -> Option<PadState> {
        None
    }
}

/// One pad's state, as the shell names its controls.
#[cfg(windows)]
fn read(pad: &gilrs::Gamepad<'_>) -> PadState {
    use gilrs::{Axis, Button};
    let mut s = PadState {
        left: (pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY)),
        right: (pad.value(Axis::RightStickX), pad.value(Axis::RightStickY)),
        left_trigger: pad
            .button_data(Button::LeftTrigger2)
            .map_or(0.0, gilrs::ev::state::ButtonData::value),
        right_trigger: pad
            .button_data(Button::RightTrigger2)
            .map_or(0.0, gilrs::ev::state::ButtonData::value),
        ..PadState::default()
    };
    for &(ours, theirs) in BUTTONS {
        s.set(ours, pad.is_pressed(theirs));
    }
    s
}

/// Each of the shell's pad buttons and the pad library's button it is read from.
#[cfg(windows)]
const BUTTONS: &[(auberean_shell::pad::PadButton, gilrs::Button)] = {
    use auberean_shell::pad::PadButton as B;
    use gilrs::Button;
    &[
        (B::South, Button::South),
        (B::East, Button::East),
        (B::West, Button::West),
        (B::North, Button::North),
        (B::LeftBumper, Button::LeftTrigger),
        (B::RightBumper, Button::RightTrigger),
        (B::Back, Button::Select),
        (B::Start, Button::Start),
        (B::DPadUp, Button::DPadUp),
        (B::DPadDown, Button::DPadDown),
        (B::DPadLeft, Button::DPadLeft),
        (B::DPadRight, Button::DPadRight),
        (B::LeftStick, Button::LeftThumb),
        (B::RightStick, Button::RightThumb),
    ]
};

#[cfg(all(test, windows))]
mod tests {
    //! Behaviour: none (this client's own reading of the system's pads)
    use super::BUTTONS;
    use auberean_shell::pad::{PadButton, PadState};

    #[test]
    fn every_pad_button_is_read_from_one_button_of_the_pads_and_none_twice() {
        let mut ours = PadState::default();
        let mut theirs = Vec::new();
        for &(b, g) in BUTTONS {
            assert!(!ours.held(b), "{b:?} is read twice");
            ours.set(b, true);
            assert!(!theirs.contains(&g), "{g:?} is read into two buttons");
            theirs.push(g);
        }
        for b in PadButton::ALL {
            assert!(ours.held(b), "{b:?} is never read");
        }
    }
}
