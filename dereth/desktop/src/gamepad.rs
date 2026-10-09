//! The pads, read through `gilrs` on Windows: the one last used, as the shell's pad. Elsewhere the
//! desktop reads none yet.

use dereth_client_shell::gamepad::{HostGamepad, PadState};

/// The desktop's pad: on Windows the pad library, opened the first time it is read; elsewhere
/// nothing.
#[derive(Default)]
pub struct DesktopGamepad {
    #[cfg(windows)]
    pads: Option<Pads>,
}

impl std::fmt::Debug for DesktopGamepad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopGamepad").finish_non_exhaustive()
    }
}

/// The pad library, and the pad last used.
#[cfg(windows)]
struct Pads {
    /// `None` once it has failed to open, so it is not tried every frame.
    gilrs: Option<gilrs::Gilrs>,
    last: Option<gilrs::GamepadId>,
}

impl HostGamepad for DesktopGamepad {
    #[cfg(windows)]
    fn poll(&mut self) -> Option<PadState> {
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

    #[cfg(not(windows))]
    fn poll(&mut self) -> Option<PadState> {
        None
    }
}

/// One pad's state, as the shell names its controls.
#[cfg(windows)]
fn read(pad: &gilrs::Gamepad<'_>) -> PadState {
    use dereth_client_shell::gamepad::PadState as S;
    use dereth_input::pad::PadButton as B;
    use gilrs::{Axis, Button};
    let mut s = S {
        left: (pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY)),
        right: (pad.value(Axis::RightStickX), pad.value(Axis::RightStickY)),
        left_trigger: pad
            .button_data(Button::LeftTrigger2)
            .map_or(0.0, gilrs::ev::state::ButtonData::value),
        right_trigger: pad
            .button_data(Button::RightTrigger2)
            .map_or(0.0, gilrs::ev::state::ButtonData::value),
        ..S::default()
    };
    for (ours, theirs) in [
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
    ] {
        s.set(ours, pad.is_pressed(theirs));
    }
    s
}
