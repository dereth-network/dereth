//! Text: the glyph model, line breaking, editing and the tag system.
//!
//! All text in the interface — labels, captions, chat history, the
//! chat entry line, book pages, the character name field — is rendered by one class,
//! [`element_text::TextElement`] (`TextElement`, element type 0x0C).
//!
//! **IME is the one functional loss** of dropping Keystone. The client has no IME implementation of
//! its own: every `WM_IME_*` message goes to `Keystone.dll`, whose companion `KeystoneIMEUI.dll`
//! draws the composition and candidate windows, and the client only ever sees finished characters
//! as `WM_CHAR`. The IME hook does nothing but hand
//! the element's screen clip box to the IME font wrapper so the composition window can be placed over the
//! edit box — and returns false immediately when the element has the no-IME bit (attribute 0x1F).
//! See [`ime_composition_rect`].

pub mod compose;
pub mod edit;
/// The string-table escape pass. It lives beside the string table in `dereth_assets` so a lookup
/// made with no UI unescapes the same way; re-exported here at its old path.
pub use dereth_assets::escape;
pub mod element_text;
pub mod glyph;
pub mod linebreak;
pub use dereth_text::{metalanguage, string_table};
pub mod tag;

pub use compose::{
    place, place_caret, place_selection, FontProvider, GlyphCell, PlacedGlyph, StringResolver,
};
pub use edit::{
    name_input_filter, number_input_filter, CursorMovementFlags, CursorTravelMode, InputFilter,
    Selection,
};
pub use element_text::{TextBits, TextElement, CARET_BLINK_PERIOD};
pub use escape::{un_escaped_char, unescape};
pub use glyph::{
    calc_justification, justification_extent, FixedMetrics, FontMetrics, Glyph, GlyphLine,
    GlyphList,
};
pub use metalanguage::render;
pub use string_table::{render_named, render_positional, render_token, DatStringResolver};
pub use tag::{TagKind, TagSpan, TaggedText, TextTag};

use crate::{Box2D, ElemHandle, UiSystem};

/// The IME composition rectangle: the rectangle to place the composition window over, or
/// `None` when the element has the *no-IME* bit.
///
/// That is the whole of the client's IME involvement: position the composition window, which is
/// all a rebuild without Keystone can do anyway. A rebuild must supply composition itself and
/// feed the committed string through the same character path so element message 0x12 still fires.
#[must_use]
pub fn ime_composition_rect(ui: &UiSystem, h: ElemHandle) -> Option<Box2D> {
    let n = ui.node(h)?;
    let t = n.behaviour.as_ref()?;
    // A non-text element has no composition rectangle either.
    let _ = t;
    let no_ime = ui
        .node(h)
        .map(|n| {
            n.merged_properties()
                .get_bool(crate::props::attr::TEXT_NO_IME)
        })
        .unwrap_or(None)
        .unwrap_or(false);
    if no_ime {
        return None;
    }
    Some(ui.screen_clip_box(h))
}

/// Install a character filter on a text element, matching all four observed client roles.
///
/// Retail has **no setter for it**: every observed site stores the filter directly after
/// finding and type-checking the text child. The four roles are the character-generation summary
/// page, profession page, spell-component panel, and toolbar. Retail uses the name and number
/// filters at exactly those four sites and no others, so this function has
/// four callers by construction, and a fifth would be a divergence rather than a fix.
///
/// Returns whether the element was a `TextElement` at all. Two of retail's four sites store
/// through an unchecked pointer and would fault on a missing child; this
/// reports instead, which is the only difference.
pub fn set_input_filter(ui: &mut UiSystem, h: ElemHandle, f: InputFilter) -> bool {
    let Some(t) = ui.text_element_mut(h) else {
        return false;
    };
    t.filter = Some(f);
    true
}
