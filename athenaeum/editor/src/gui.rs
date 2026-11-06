//! The panels' toolkit: egui, fed the window's events as egui's own.
//!
//! The window's events in pixels become egui's in points ([`Gui::scale`] pixels each): the
//! pointer, its buttons and wheel, the keys with the text they type, the modifiers held. What egui
//! takes (a click on a panel, typing in a field) the world does not; the editor asks after each
//! pass. Text is the carried Liberation Sans (and Mono for numbers), so no font of another licence
//! is linked in.

use std::sync::Arc;

use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// The carried sans, for the panels' text.
static SANS: &[u8] =
    include_bytes!("../../../dereth/client/crates/classic-fonts/fonts/LiberationSans-Bold.ttf");
/// The carried mono, for numbers.
static MONO: &[u8] =
    include_bytes!("../../../dereth/client/crates/classic-fonts/fonts/LiberationMono-Bold.ttf");

/// The carried fonts as egui's.
fn fonts() -> egui::FontDefinitions {
    let mut f = egui::FontDefinitions::empty();
    f.font_data
        .insert("sans".into(), Arc::new(egui::FontData::from_static(SANS)));
    f.font_data
        .insert("mono".into(), Arc::new(egui::FontData::from_static(MONO)));
    f.families.insert(
        egui::FontFamily::Proportional,
        vec!["sans".into(), "mono".into()],
    );
    f.families.insert(
        egui::FontFamily::Monospace,
        vec!["mono".into(), "sans".into()],
    );
    f
}

/// Whether the key shortcuts are held for is down: Cmd on macOS as well as Ctrl.
#[must_use]
pub fn command_key(m: winit::keyboard::ModifiersState) -> bool {
    m.control_key() || (cfg!(target_os = "macos") && m.super_key())
}

/// A physical key by its name (`KeyR`, `Delete`, `Digit1`), for the keys [`key_of`] knows.
#[must_use]
pub fn key_named(name: &str) -> Option<KeyCode> {
    use KeyCode as C;
    let letters = [
        C::KeyA,
        C::KeyB,
        C::KeyC,
        C::KeyD,
        C::KeyE,
        C::KeyF,
        C::KeyG,
        C::KeyH,
        C::KeyI,
        C::KeyJ,
        C::KeyK,
        C::KeyL,
        C::KeyM,
        C::KeyN,
        C::KeyO,
        C::KeyP,
        C::KeyQ,
        C::KeyR,
        C::KeyS,
        C::KeyT,
        C::KeyU,
        C::KeyV,
        C::KeyW,
        C::KeyX,
        C::KeyY,
        C::KeyZ,
    ];
    let digits = [
        C::Digit0,
        C::Digit1,
        C::Digit2,
        C::Digit3,
        C::Digit4,
        C::Digit5,
        C::Digit6,
        C::Digit7,
        C::Digit8,
        C::Digit9,
    ];
    let others = [
        C::Escape,
        C::Enter,
        C::Tab,
        C::Space,
        C::Delete,
        C::Backspace,
        C::Equal,
        C::Minus,
        C::ArrowUp,
        C::ArrowDown,
        C::ArrowLeft,
        C::ArrowRight,
    ];
    letters
        .into_iter()
        .chain(digits)
        .chain(others)
        .find(|c| format!("{c:?}").eq_ignore_ascii_case(name))
}

/// egui's key for a physical key, for the keys the panels use.
#[must_use]
pub fn key_of(code: KeyCode) -> Option<egui::Key> {
    use egui::Key as K;
    use KeyCode as C;
    Some(match code {
        C::ArrowDown => K::ArrowDown,
        C::ArrowLeft => K::ArrowLeft,
        C::ArrowRight => K::ArrowRight,
        C::ArrowUp => K::ArrowUp,
        C::Escape => K::Escape,
        C::Tab => K::Tab,
        C::Backspace => K::Backspace,
        C::Enter | C::NumpadEnter => K::Enter,
        C::Space => K::Space,
        C::Insert => K::Insert,
        C::Delete => K::Delete,
        C::Home => K::Home,
        C::End => K::End,
        C::PageUp => K::PageUp,
        C::PageDown => K::PageDown,
        C::Minus | C::NumpadSubtract => K::Minus,
        C::Equal | C::NumpadAdd => K::Plus,
        C::Digit0 | C::Numpad0 => K::Num0,
        C::Digit1 | C::Numpad1 => K::Num1,
        C::Digit2 | C::Numpad2 => K::Num2,
        C::Digit3 | C::Numpad3 => K::Num3,
        C::Digit4 | C::Numpad4 => K::Num4,
        C::Digit5 | C::Numpad5 => K::Num5,
        C::Digit6 | C::Numpad6 => K::Num6,
        C::Digit7 | C::Numpad7 => K::Num7,
        C::Digit8 | C::Numpad8 => K::Num8,
        C::Digit9 | C::Numpad9 => K::Num9,
        C::KeyA => K::A,
        C::KeyB => K::B,
        C::KeyC => K::C,
        C::KeyD => K::D,
        C::KeyE => K::E,
        C::KeyF => K::F,
        C::KeyG => K::G,
        C::KeyH => K::H,
        C::KeyI => K::I,
        C::KeyJ => K::J,
        C::KeyK => K::K,
        C::KeyL => K::L,
        C::KeyM => K::M,
        C::KeyN => K::N,
        C::KeyO => K::O,
        C::KeyP => K::P,
        C::KeyQ => K::Q,
        C::KeyR => K::R,
        C::KeyS => K::S,
        C::KeyT => K::T,
        C::KeyU => K::U,
        C::KeyV => K::V,
        C::KeyW => K::W,
        C::KeyX => K::X,
        C::KeyY => K::Y,
        C::KeyZ => K::Z,
        C::F1 => K::F1,
        C::F2 => K::F2,
        C::F3 => K::F3,
        C::F4 => K::F4,
        C::F5 => K::F5,
        C::F6 => K::F6,
        C::F7 => K::F7,
        C::F8 => K::F8,
        C::F9 => K::F9,
        C::F10 => K::F10,
        C::F11 => K::F11,
        C::F12 => K::F12,
        _ => return None,
    })
}

/// The panels' context and the input it is given.
pub struct Gui {
    pub ctx: egui::Context,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
    pointer: egui::Pos2,
    /// Pixels to the point.
    pub scale: f32,
}

impl std::fmt::Debug for Gui {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gui").field("scale", &self.scale).finish()
    }
}

impl Gui {
    #[must_use]
    pub fn new(scale: f32) -> Self {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts());
        ctx.set_visuals(egui::Visuals::dark());
        Self {
            ctx,
            events: Vec::new(),
            modifiers: egui::Modifiers::default(),
            pointer: egui::Pos2::ZERO,
            scale,
        }
    }

    /// The pointer, in pixels.
    #[must_use]
    pub fn pointer_pixels(&self) -> [f32; 2] {
        [self.pointer.x * self.scale, self.pointer.y * self.scale]
    }

    /// The pointer to a pixel of the window, as a script moves it.
    pub fn point(&mut self, px: [f32; 2]) {
        self.pointer = egui::pos2(px[0] / self.scale, px[1] / self.scale);
        self.events.push(egui::Event::PointerMoved(self.pointer));
    }

    /// A button down or up, as a script presses it.
    pub fn button(&mut self, button: egui::PointerButton, pressed: bool) {
        self.events.push(egui::Event::PointerButton {
            pos: self.pointer,
            button,
            pressed,
            modifiers: self.modifiers,
        });
    }

    /// A key pressed and let go, as a script presses it.
    pub fn key(&mut self, code: KeyCode, modifiers: egui::Modifiers) {
        if let Some(k) = key_of(code) {
            for pressed in [true, false] {
                self.events.push(egui::Event::Key {
                    key: k,
                    physical_key: Some(k),
                    pressed,
                    repeat: false,
                    modifiers,
                });
            }
        }
    }

    /// Text typed, as a script types it.
    pub fn wheel(&mut self, notches: f32) {
        self.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, notches),
            phase: egui::TouchPhase::Move,
            modifiers: self.modifiers,
        });
    }

    /// Text typed, as a script types it.
    pub fn type_text(&mut self, text: &str) {
        self.events.push(egui::Event::Text(text.to_string()));
    }

    /// A window event, kept for the next pass.
    pub fn event(&mut self, e: &WindowEvent) {
        let s = self.scale;
        match e {
            WindowEvent::CursorMoved { position, .. } => {
                #[allow(clippy::cast_possible_truncation)]
                let p = egui::pos2(position.x as f32 / s, position.y as f32 / s);
                self.pointer = p;
                self.events.push(egui::Event::PointerMoved(p));
            }
            WindowEvent::CursorLeft { .. } => self.events.push(egui::Event::PointerGone),
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    MouseButton::Left => egui::PointerButton::Primary,
                    MouseButton::Right => egui::PointerButton::Secondary,
                    MouseButton::Middle => egui::PointerButton::Middle,
                    _ => return,
                };
                self.events.push(egui::Event::PointerButton {
                    pos: self.pointer,
                    button,
                    pressed: *state == ElementState::Pressed,
                    modifiers: self.modifiers,
                });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                #[allow(clippy::cast_possible_truncation)]
                let (unit, d) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        (egui::MouseWheelUnit::Line, egui::vec2(*x, *y))
                    }
                    MouseScrollDelta::PixelDelta(p) => (
                        egui::MouseWheelUnit::Point,
                        egui::vec2(p.x as f32 / s, p.y as f32 / s),
                    ),
                };
                self.events.push(egui::Event::MouseWheel {
                    unit,
                    delta: d,
                    phase: egui::TouchPhase::Move,
                    modifiers: self.modifiers,
                });
            }
            WindowEvent::ModifiersChanged(m) => {
                let st = m.state();
                self.modifiers = egui::Modifiers {
                    alt: st.alt_key(),
                    ctrl: st.control_key(),
                    shift: st.shift_key(),
                    mac_cmd: cfg!(target_os = "macos") && st.super_key(),
                    command: command_key(st),
                };
                self.events
                    .push(egui::Event::ModifiersChanged(self.modifiers));
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key {
                    if let Some(k) = key_of(code) {
                        self.events.push(egui::Event::Key {
                            key: k,
                            physical_key: Some(k),
                            pressed,
                            repeat: event.repeat,
                            modifiers: self.modifiers,
                        });
                    }
                }
                if pressed && !self.modifiers.ctrl && !self.modifiers.command {
                    if let Some(t) = event
                        .text
                        .as_ref()
                        .filter(|t| !t.is_empty() && t.chars().all(|c| !c.is_control()))
                    {
                        self.events.push(egui::Event::Text(t.to_string()));
                    }
                }
            }
            WindowEvent::Focused(f) => self.events.push(egui::Event::WindowFocused(*f)),
            _ => {}
        }
    }

    /// The input for one pass over a window of `size` pixels at `time` seconds.
    pub fn input(&mut self, size: (u32, u32), time: f64) -> egui::RawInput {
        let s = self.scale;
        #[allow(clippy::cast_precision_loss)]
        let screen = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(size.0 as f32 / s, size.1 as f32 / s),
        );
        let mut raw = egui::RawInput {
            screen_rect: Some(screen),
            time: Some(time),
            events: std::mem::take(&mut self.events),
            max_texture_side: Some(8192),
            focused: true,
            ..egui::RawInput::default()
        };
        if let Some(root) = raw.viewports.get_mut(&raw.viewport_id) {
            root.native_pixels_per_point = Some(s);
        }
        raw
    }
}
