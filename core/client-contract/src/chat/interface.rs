//! `ChatInterface`'s three vocabularies: the window ids, one delivered line, and the two opacity
//! attributes.
//!
//! Cut out of `dereth_ui_screens::chat::interface`, which keeps everything that routes, filters,
//! scrolls or fades. What is here is what crosses the seam: `dereth_client::hud` composes a
//! `ChatMessage` for every inbound final display string (the notice that carries a finished chat
//! line into the UI is one of the three process globals the client owns), names `window::MAIN` when it seeds a placement, and
//! reads `opacity_attr`'s two property ids off the retained `PlayerModule`. None of the three
//! draws anything.
//!
//! `dereth_ui_screens::chat::interface::{window, ChatMessage, opacity_attr}` resolve through
//! `pub use` lines at the lines the definitions were on.

/// The five window ids the client's switch on the window id knows.
///
/// The client's table gives ids **1 and 8** for the main window; 8 is the one `ChatOptionsPanel`
/// uses as the user data for the main window's filter control.
pub mod window {
    /// The main chat window's alternate id.
    pub const MAIN_ALT: u32 = 1;
    /// Floaty chat 1.
    pub const FLOATY_1: u32 = 2;
    /// Floaty chat 2.
    pub const FLOATY_2: u32 = 3;
    /// Floaty chat 3.
    pub const FLOATY_3: u32 = 4;
    /// Floaty chat 4.
    pub const FLOATY_4: u32 = 5;
    /// The main chat window.
    pub const MAIN: u32 = 8;
}

/// One message as the final-string-info notice delivers it: type, body, prefix, and window id.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChatMessage {
    pub ty: u8,
    pub body: String,
    /// The speaker prefix, drawn in colour index 12 whatever `ty` is.
    pub prefix: Option<String>,
    /// 0 = broadcast, subject to the per-window filter.
    pub window: u32,
}

/// The chat interface's two opacity attributes, and the value it substitutes when the layout does
/// not carry one: `1.0f`, written before the property is read and left in place when the property
/// is null.
///
/// The dispatch is a two-way branch on the attribute id: the default-opacity id sets the idle
/// value, the next id up sets the active value, and every other id is ignored outright.
pub mod opacity_attr {
    /// The default opacity — the idle value.
    pub const DEFAULT: u32 = 0x1000_0080;
    /// The active opacity — focused or moused-over.
    pub const ACTIVE: u32 = 0x1000_0081;
    /// What the attribute setter uses when the property is absent.
    pub const FALLBACK: f32 = 1.0;
}
