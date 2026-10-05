//! `ChatInterface`'s three vocabularies: the window ids, one delivered line, and the two opacity
//! attributes.
//!
//! Cut out of `dereth_ui_screens::chat::interface`, which keeps everything that routes, filters,
//! scrolls or fades. What is here is what crosses the seam: `dereth_client_shell::hud` composes a
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
    pub feedback: crate::feedback::Feedback,
    pub ty: u8,
    pub body: String,
    /// The optional timestamp prefix, drawn in colour index 12 whatever `ty` is.
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

/// The default 64-bit text-type filter per window id, set by the post-init's window-id switch.
///
/// Type `0x1A` is *not* a chat type — it is the over-head bubble channel, and the main window's
/// default filter excludes exactly that bit. `0xFBFFFFFF` is `!(1 << 26)` over the low 32.
#[must_use]
pub fn default_filter(window_id: u32) -> u64 {
    match window_id {
        window::MAIN_ALT | window::MAIN => 0xFBFF_FFFF,
        window::FLOATY_1 => 0x0000_101C,
        window::FLOATY_2 => 0x0004_0C00,
        window::FLOATY_3 => 0x0008_0000,
        window::FLOATY_4 => 0x7800_0000,
        // The `switch` has no default arm; an unknown window id leaves the filter at its
        // constructed value, which is zero — the window accepts nothing on a broadcast.
        _ => 0,
    }
}

/// One check box on the chat-options page: a label string id and the mask it owns.
///
/// The chat options panel's checkbox bitfield64 option insert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterGroup {
    pub label: &'static str,
    pub mask: u64,
}

const fn fg(label: &'static str, mask: u64) -> FilterGroup {
    FilterGroup { label, mask }
}

/// The thirteen filter groups, in the order the options page adds them.
///
/// **Twelve of the thirteen are always offered**; *Gameplay* is the one that is not. Which
/// windows get it is easy to get backwards — see [`filter_groups_for`].
pub const FILTER_GROUPS: [FilterGroup; 13] = [
    fg("ID_ChatOption_TextFilter_Gameplay", 0x8391_2021),
    fg("ID_ChatOption_TextFilter_Combat", 0x0060_0040),
    fg("ID_ChatOption_TextFilter_Magic", 0x0002_0080),
    fg("ID_ChatOption_TextFilter_AreaSpeech", 0x0000_1004),
    fg("ID_ChatOption_TextFilter_Tells", 0x0000_0018),
    fg("ID_ChatOption_TextFilter_Allegience", 0x0004_0C00),
    fg("ID_ChatOption_TextFilter_Fellowship", 0x0008_0000),
    fg("ID_ChatOption_TextFilter_General", 0x0800_0000),
    fg("ID_ChatOption_TextFilter_Trade", 0x1000_0000),
    fg("ID_ChatOption_TextFilter_LFG", 0x2000_0000),
    fg("ID_ChatOption_TextFilter_Roleplay", 0x4000_0000),
    fg("ID_ChatOption_TextFilter_Society", 0x1_0000_0000),
    fg("ID_ChatOption_TextFilter_Error", 0x0400_0000),
];

/// The groups offered for one window: all thirteen for **every floaty**, the last twelve
/// (starting at Combat) for the **main** window.
///
/// The client switches on the window id, and window **8** is the one case that skips the
/// *Gameplay* check box:
///
/// ```text
///   window 8:        default 0xFBFFFFFF; no Gameplay child
///   windows 2/3/4/5: default = that window's mask; add Gameplay (mask 0x83912021)
///   every window:    add Combat (mask 0x600040) and the eleven after it
/// ```
///
/// The chat-options page's two per-window blocks agree: the main-window block (window 8) goes
/// from setting the default straight to adding Combat (`0x600040`), while the floaty-4 block
/// (window 5) adds Gameplay with mask `0x83912021` first.
/// So the page shows **12 + 13 · 4 = 64** check boxes.
///
/// An unknown window id falls past the switch's range check to the common
/// tail, i.e. twelve groups and **no default value at all** — which is why
/// [`default_filter`] answers 0 there.
#[must_use]
pub fn filter_groups_for(window_id: u32) -> &'static [FilterGroup] {
    match window_id {
        window::MAIN | window::MAIN_ALT => &FILTER_GROUPS[1..],
        window::FLOATY_1 | window::FLOATY_2 | window::FLOATY_3 | window::FLOATY_4 => &FILTER_GROUPS,
        _ => &FILTER_GROUPS[1..],
    }
}

/// Trimming both ends for `L"\n"` is the client's first operation on every composed chat
/// line.
///
/// The composed line is trimmed using a temporary wide string containing only `\n`,
/// with both leading and trailing trimming enabled. The final-string notice carries
/// that trimmed string, not the original composed line.
///
/// `trim`'s trailing arm loops over wide characters, stopping on the first non-newline and
/// otherwise reducing the character count and moving the end pointer back two bytes.
/// Every trailing newline comes off, not merely one; the leading arm mirrors this behavior.
///
/// **Why it lives here rather than at each composer.** That one function is the *only* caller of
/// the display-final-string-info notice in the whole client, so the trim applies
/// to every line the chat log can ever receive; taking it at the notice boundary is the same
/// function on the same strings, and it cannot be forgotten by one of the hundred call sites that
/// compose a line.
///
/// **What it fixes.** A body that keeps its newline leaves the log's *last glyph* a newline glyph,
/// and the glyph list's recalculate then adds the empty line at the glyph count. That line is
/// real and correct for a text element in general — it must be unreachable for the chat log,
/// where it shows as a blank bottom row. Every `0xF7E0` body in the recorded error-condition
/// sweep ends in `'\n'`, so on an ACE shard it would be every line.
#[must_use]
pub fn add_text_to_scroll_trim(s: &str) -> &str {
    s.trim_matches('\n')
}

/// What one window did with one message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Routed {
    /// Appended to this window's log.
    Accepted,
    /// Addressed to a different window.
    OtherWindow,
    /// Broadcast, but this window filters the type out.
    FilteredOut,
}

/// Whether a broadcast type is included in a window's filter.
#[must_use]
pub const fn type_is_active(filter: u64, ty: u32) -> bool {
    ty < 64 && (filter >> ty) & 1 != 0
}

/// Addressed messages bypass filters; broadcasts must match the type mask.
#[must_use]
pub const fn route(window: u32, filter: u64, ty: u32, destination: u32) -> Routed {
    if destination == window {
        Routed::Accepted
    } else if destination != 0 {
        Routed::OtherWindow
    } else if type_is_active(filter, ty) {
        Routed::Accepted
    } else {
        Routed::FilteredOut
    }
}

#[cfg(test)]
mod shared_tests {
    use super::*;
    /// Behaviour: chat.routing
    #[test]
    fn addressed_messages_bypass_filters_and_large_broadcast_types_never_shift_the_mask() {
        for ty in [0, 26, 33, 63, 64, 256, u32::MAX] {
            assert_eq!(route(8, 0, ty, 8), Routed::Accepted);
            assert_eq!(route(8, u64::MAX, ty, 1), Routed::OtherWindow);
            assert_eq!(route(8, 0, ty, 0), Routed::FilteredOut);
            assert_eq!(
                route(8, u64::MAX, ty, 0),
                if ty < 64 {
                    Routed::Accepted
                } else {
                    Routed::FilteredOut
                }
            );
        }
    }
    /// Behaviour: chat.routing
    #[test]
    fn message_trim_removes_only_edge_newlines_including_newline_only_bodies() {
        for (input, expected) in [
            ("", ""),
            ("\n\n", ""),
            ("\nA\nB\n", "A\nB"),
            (" A ", " A "),
            ("\r\n", "\r"),
        ] {
            assert_eq!(add_text_to_scroll_trim(input), expected);
        }
    }
}
