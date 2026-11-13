//! Explore mode's front end (`--explore`): the keys and the mouse that fly the camera, and the
//! line in the corner that says how fast it flies, where it is and which keys do what. No
//! interface is shown and none takes the keys.
//!
//! The right button held turns the camera with the mouse; W, A, S and D (or the arrows) fly it,
//! E up and Q down; the wheel, or + and -, step its speed; C copies where it is as a location line
//! and R the visual report's whole line.

use std::sync::Arc;

use dereth_client_contract::overlay::OverlayItem;
use dereth_client_runtime::explore::Fly;
use dereth_input::host::HostEvent;
use dereth_input::keys::MouseButton;
use dereth_ui_kit::art::{Art, Family};
use dereth_ui_kit::draw::{DrawList, Overlay, Rect};
use dereth_ui_kit::paint::{Painter, TextStyle};

use super::{ClientShell, Cx};
use crate::platform::host::Host;

/// The keys the corner line's pictures are uploaded under: a range of their own.
const KEY_BASE: u64 = 0x4558_5000_0000_0000;

/// How long the line saying what was copied stays, seconds.
const NOTE_SECONDS: f64 = 3.0;

/// What the keys do, as the corner line says it.
pub(crate) const KEYS: &str = "Right mouse: look   W A S D: fly   E, Q: up, down   \
                               Wheel or + -: speed   C: copy location   R: copy report";

/// Virtual keys.
mod vk {
    pub const A: usize = 0x41;
    pub const C: usize = 0x43;
    pub const D: usize = 0x44;
    pub const E: usize = 0x45;
    pub const Q: usize = 0x51;
    pub const R: usize = 0x52;
    pub const S: usize = 0x53;
    pub const W: usize = 0x57;
    pub const LEFT: usize = 0x25;
    pub const UP: usize = 0x26;
    pub const RIGHT: usize = 0x27;
    pub const DOWN: usize = 0x28;
    pub const ADD: usize = 0x6B;
    pub const SUBTRACT: usize = 0x6D;
    pub const PLUS: usize = 0xBB;
    pub const MINUS: usize = 0xBD;
}

/// What a key does in explore mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExploreKey {
    Fly(Fly),
    Faster,
    Slower,
    CopyLocation,
    CopyReport,
}

/// The key `virtual_key` does, if any.
pub(crate) fn key_of(virtual_key: usize) -> Option<ExploreKey> {
    Some(match virtual_key {
        vk::W | vk::UP => ExploreKey::Fly(Fly::Forward),
        vk::S | vk::DOWN => ExploreKey::Fly(Fly::Back),
        vk::A | vk::LEFT => ExploreKey::Fly(Fly::Left),
        vk::D | vk::RIGHT => ExploreKey::Fly(Fly::Right),
        vk::E => ExploreKey::Fly(Fly::Up),
        vk::Q => ExploreKey::Fly(Fly::Down),
        vk::PLUS | vk::ADD => ExploreKey::Faster,
        vk::MINUS | vk::SUBTRACT => ExploreKey::Slower,
        vk::C => ExploreKey::CopyLocation,
        vk::R => ExploreKey::CopyReport,
        _ => return None,
    })
}

/// What explore mode's front end keeps from one frame to the next.
#[derive(Debug, Default)]
pub(crate) struct ExploreFace {
    /// Whether the mode runs: set as each frame's line is made.
    pub on: bool,
    /// The art the line is drawn from, and the reading of the data files it was made for.
    art: Option<(u64, Arc<Art>)>,
    overlay: Option<Overlay>,
    /// This frame's line as overlay items.
    pub items: Vec<OverlayItem>,
    /// What was last copied, and when.
    note: Option<(&'static str, f64)>,
    /// Wheel notches not yet taken as a step of speed.
    wheel: f32,
}

impl ExploreFace {
    /// One device event: the camera's keys and the mouse. Whether it was theirs.
    fn device_event<H: Host>(
        cx: &mut Cx<'_, H>,
        shell: &mut ClientShell<H>,
        event: &HostEvent,
    ) -> bool {
        match event {
            HostEvent::KeyboardInput { key, pressed, .. } => match key_of(key.virtual_key) {
                Some(ExploreKey::Fly(way)) => cx.explore_fly(way, *pressed),
                Some(ExploreKey::Faster) if *pressed => cx.explore_speed(1),
                Some(ExploreKey::Slower) if *pressed => cx.explore_speed(-1),
                Some(ExploreKey::CopyLocation) if *pressed => {
                    let line = cx.camera_location_line();
                    shell.explore_copy(cx, line, "Location copied.");
                }
                Some(ExploreKey::CopyReport) if *pressed => {
                    let line = cx.visual_report_line(shell);
                    shell.explore_copy(cx, Some(line), "Report copied.");
                }
                Some(_) => {}
                None => return false,
            },
            HostEvent::MouseInput {
                button: MouseButton::Right,
                pressed,
            } => cx.mouse_look_button(*pressed),
            HostEvent::CursorMoved { x, y } => cx.cursor_moved(*x, *y),
            HostEvent::MouseWheel { notches } => {
                let face = &mut shell.explore;
                face.wheel += notches;
                // LINT-OK: whole notches of a wheel, a handful at most.
                #[allow(clippy::cast_possible_truncation)]
                let steps = face.wheel.trunc() as i32;
                face.wheel -= face.wheel.trunc();
                if steps != 0 {
                    cx.explore_speed(steps);
                }
            }
            _ => return false,
        }
        true
    }

    /// Make this frame's line: the speed and the camera's spot, the keys, and what was copied
    /// while that is new.
    fn compose<H: Host>(&mut self, cx: &mut Cx<'_, H>) {
        let Some(status) = cx.explore_status() else {
            self.items.clear();
            return;
        };
        let art = self.art(cx);
        let now = cx.now();
        let mut lines = vec![format!(
            "Explore   {}   {}",
            speed_text(status.meters_per_second),
            status.location.unwrap_or_default()
        )];
        lines.push(KEYS.to_owned());
        if let Some((note, at)) = self.note {
            if now - at < NOTE_SECONDS {
                lines.push(note.to_owned());
            } else {
                self.note = None;
            }
        }
        let size = cx.present().size();
        #[allow(clippy::cast_precision_loss)]
        let screen = (size.0 as f32, size.1 as f32);
        let mut list = DrawList::default();
        let mut painter = Painter {
            list: &mut list,
            art: &art,
            scale: 1.0,
            screen,
            fade: 1.0,
        };
        let style = TextStyle::new(Family::Body, 14.0, 0xFFF0_E6D2).edge(0xFF00_0000);
        let line_height = painter.line_height(&style).ceil();
        let width = lines
            .iter()
            .map(|l| painter.measure(&style, l))
            .fold(0.0_f32, f32::max);
        let (pad, x, y) = (6.0, 8.0, 8.0);
        #[allow(clippy::cast_precision_loss)]
        let height = line_height * lines.len() as f32;
        painter.fill(
            Rect::new(x, y, width + 2.0 * pad, height + 2.0 * pad),
            0x9000_0000,
        );
        for (i, line) in lines.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let row = i as f32;
            painter.text(&style, x + pad, y + pad + row * line_height, line);
        }
        let overlay = self.overlay.get_or_insert_with(|| Overlay::new(KEY_BASE));
        let mut sink = super::plugin_layer::Sink(cx.present_mut());
        overlay.compose(&mut sink, &art, &list, None, &[]);
        self.items = overlay.items().to_vec();
    }

    /// The art the line is drawn from: the game's label font, made again when the data files
    /// change.
    fn art<H: Host>(&mut self, cx: &mut Cx<'_, H>) -> Arc<Art> {
        let generation = cx.store_generation();
        if let Some((made_for, art)) = &self.art {
            if *made_for == generation {
                return Arc::clone(art);
            }
        }
        if let Some(overlay) = self.overlay.as_mut() {
            overlay.release(&mut super::plugin_layer::Sink(cx.present_mut()));
        }
        let font = super::plugin_layer::retail_font(cx.store());
        let art = Arc::new(dereth_plugin_ui::skin::retail(font, None, &|_| None));
        self.art = Some((generation, Arc::clone(&art)));
        art
    }
}

/// A speed as the corner line says it.
fn speed_text(meters_per_second: f32) -> String {
    format!("{meters_per_second:.0} m/s")
}

impl<H: Host> ClientShell<H> {
    /// One of the window's events in explore mode: a lifecycle event as any interface routes it,
    /// a device event to the camera, and one the camera does not take to the window procedure
    /// alone (Alt+Enter still makes the window full screen), never to the game's keys.
    pub(super) fn route_explore_event(
        &mut self,
        cx: &mut Cx<'_, H>,
        event: &HostEvent,
        time_ms: u32,
    ) {
        if crate::platform::window::lifecycle(event).is_some() {
            super::route_host_event(cx, self, event, time_ms);
            return;
        }
        if ExploreFace::device_event(cx, self, event) {
            return;
        }
        for m in self.shared.devices.map_device_event(event, time_ms) {
            cx.window_message(crate::pump::window_message(m), m.time_ms);
        }
    }

    /// This frame's corner line, in explore mode.
    pub(super) fn compose_explore(&mut self, cx: &mut Cx<'_, H>) {
        self.explore.on = true;
        self.explore.compose(cx);
    }

    /// Copy `text` to the clipboard, and say so on the screen.
    fn explore_copy(&mut self, cx: &Cx<'_, H>, text: Option<String>, copied: &'static str) {
        use dereth_client_runtime::shell::Shell;
        let Some(text) = text else {
            return;
        };
        tracing::info!("{text}");
        let note = if self.copy_text(&text) {
            copied
        } else {
            "The clipboard did not take it."
        };
        self.explore.note = Some((note, cx.now()));
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a developer's camera flown through the world with no server).
    use super::*;

    #[test]
    fn the_keys_fly_the_camera_step_its_speed_and_copy() {
        assert_eq!(key_of(0x57), Some(ExploreKey::Fly(Fly::Forward)));
        assert_eq!(key_of(0x53), Some(ExploreKey::Fly(Fly::Back)));
        assert_eq!(key_of(0x41), Some(ExploreKey::Fly(Fly::Left)));
        assert_eq!(key_of(0x44), Some(ExploreKey::Fly(Fly::Right)));
        assert_eq!(key_of(0x45), Some(ExploreKey::Fly(Fly::Up)));
        assert_eq!(key_of(0x51), Some(ExploreKey::Fly(Fly::Down)));
        assert_eq!(key_of(0xBB), Some(ExploreKey::Faster));
        assert_eq!(key_of(0xBD), Some(ExploreKey::Slower));
        assert_eq!(key_of(0x43), Some(ExploreKey::CopyLocation));
        assert_eq!(key_of(0x52), Some(ExploreKey::CopyReport));
        assert_eq!(key_of(0x0D), None);
    }

    #[test]
    fn the_speed_reads_in_whole_meters_a_second() {
        assert_eq!(speed_text(5000.0), "5000 m/s");
        assert_eq!(speed_text(2.0), "2 m/s");
    }
}
