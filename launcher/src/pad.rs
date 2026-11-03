//! A pad on the launcher's page. On Windows the pad library reads it, as the client reads it, while
//! the launcher's window is the one in front; the launcher's own rules
//! ([`dereth_launch::pad`]) turn its readings into the page's moves, and each goes to the page as
//! a `pad` event. The page asks [`pad_target`] which control a move reaches. Elsewhere no pad is
//! read yet, as in the client.

use dereth_launch::pad::{reach, Rect, Step};

/// The event each of the page's moves goes to it as.
pub const EVENT: &str = "pad";

/// Which of the page's controls (where each is drawn, in the page's order) a move one `step` from
/// the focused one reaches; none when no control lies that way.
#[tauri::command]
pub fn pad_target(rects: Vec<Rect>, from: Option<usize>, step: Step) -> Option<usize> {
    reach(&rects, from, step)
}

/// Reads the pad on a thread of its own for as long as the launcher runs.
pub fn watch(app: tauri::AppHandle) {
    #[cfg(windows)]
    std::thread::spawn(move || pads::run(&app));
    #[cfg(not(windows))]
    let _ = app;
}

#[cfg(windows)]
mod pads {
    use std::time::{Duration, Instant};

    use dereth_launch::pad::{PadButton, PadNav, PadState};
    use tauri::{Emitter, Manager};

    /// How often the pad is read.
    const EVERY: Duration = Duration::from_millis(16);

    pub(super) fn run(app: &tauri::AppHandle) {
        let mut gilrs = match gilrs::Gilrs::new() {
            Ok(g) => g,
            Err(e) => {
                eprintln!("dereth-launcher: no pads: {e}");
                return;
            }
        };
        let start = Instant::now();
        let mut nav = PadNav::default();
        let mut last: Option<gilrs::GamepadId> = None;
        loop {
            // The library's state moves only as its events are taken; the pad last heard from is
            // the one read.
            while let Some(event) = gilrs.next_event() {
                last = Some(event.id);
            }
            let front = app
                .get_webview_window("main")
                .and_then(|w| w.is_focused().ok())
                .unwrap_or(false);
            let id = last
                .filter(|id| gilrs.connected_gamepad(*id).is_some())
                .or_else(|| gilrs.gamepads().next().map(|(id, _)| id));
            last = id;
            let state = id.filter(|_| front).map(|id| read(&gilrs.gamepad(id)));
            for command in nav.read(state.as_ref(), start.elapsed().as_secs_f64()) {
                let _ = app.emit(super::EVENT, command);
            }
            std::thread::sleep(EVERY);
        }
    }

    /// One pad's state, named as the client names its controls.
    fn read(pad: &gilrs::Gamepad<'_>) -> PadState {
        use gilrs::{Axis, Button};
        let mut s = PadState {
            left: (pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY)),
            right: (pad.value(Axis::RightStickX), pad.value(Axis::RightStickY)),
            ..PadState::default()
        };
        for (ours, theirs) in [
            (PadButton::South, Button::South),
            (PadButton::East, Button::East),
            (PadButton::West, Button::West),
            (PadButton::North, Button::North),
            (PadButton::LeftBumper, Button::LeftTrigger),
            (PadButton::RightBumper, Button::RightTrigger),
            (PadButton::Back, Button::Select),
            (PadButton::Start, Button::Start),
            (PadButton::DPadUp, Button::DPadUp),
            (PadButton::DPadDown, Button::DPadDown),
            (PadButton::DPadLeft, Button::DPadLeft),
            (PadButton::DPadRight, Button::DPadRight),
        ] {
            s.set(ours, pad.is_pressed(theirs));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use dereth_launch::pad::{PadCommand, Step};

    /// Every move the launcher sends the page is one the page acts on: it listens for the event,
    /// and handles each kind of move by its name on the wire.
    #[test]
    fn the_page_acts_on_every_move_the_pad_sends_it() {
        let page = include_str!("../ui/app.js");
        assert!(
            page.contains(&format!("listen(\"{}\"", super::EVENT)),
            "the page listens for the pad"
        );
        for command in [
            PadCommand::Move { step: Step::Up },
            PadCommand::Press,
            PadCommand::Back,
            PadCommand::NextSection,
            PadCommand::PreviousSection,
            PadCommand::Scroll { pixels: 1.0 },
        ] {
            let wire = serde_json::to_value(command).expect("encodes");
            let kind = wire["kind"].as_str().expect("a kind");
            assert!(
                page.contains(&format!("case \"{kind}\"")),
                "the page does nothing for {kind}"
            );
        }
        for step in ["up", "down", "left", "right"] {
            assert!(
                serde_json::from_str::<Step>(&format!("\"{step}\"")).is_ok(),
                "{step}"
            );
        }
    }
}
