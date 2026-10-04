//! `TextElement` (0x0C) — the text and edit control.
//!
//! Text input, selection, caret, clipboard, and layout behavior live here.
//!
//! The original text control combines scrolling, element, and character-input behavior. This
//! rebuild composes the shared scrolling state and delivers characters through the element API.

use std::sync::Arc;

use crate::element::{Element, ElementMessageListenResult as R};
use crate::focus::{action, InputEvent};
use crate::msg::element::id as msgid;
use crate::props::{attr, PropertyValue};
use crate::text::edit::{
    move_cursor, CursorMovementFlags, CursorTravelMode, InputFilter, Selection,
};
use crate::text::glyph::{FixedMetrics, FontMetrics, GlyphList};
use crate::text::tag::TextTag;
use crate::{ElemCtx, ElementMessage, MessageId};

/// The text element's behaviour bits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextBits(pub u32);

macro_rules! bit {
    ($get:ident, $set:ident, $mask:expr, $doc:expr) => {
        #[doc = $doc]
        #[must_use]
        pub const fn $get(self) -> bool {
            self.0 & $mask != 0
        }
        #[doc = $doc]
        pub fn $set(&mut self, v: bool) {
            if v {
                self.0 |= $mask;
            } else {
                self.0 &= !$mask;
            }
        }
    };
}

impl TextBits {
    bit!(
        editable,
        set_editable,
        0x0001,
        "0x0001 — editable, attribute 0x16."
    );
    bit!(
        one_line,
        set_one_line,
        0x0002,
        "0x0002 — one line only, attribute 0x20."
    );
    bit!(
        selectable,
        set_selectable,
        0x0004,
        "0x0004 — selectable, attribute 0x27."
    );
    bit!(
        no_ime,
        set_no_ime,
        0x0008,
        "0x0008 — **no IME**, attribute 0x1F."
    );
    bit!(
        outline,
        set_outline,
        0x0010,
        "0x0010 — draw an outline around glyphs, attribute 0x21."
    );
    bit!(
        selection_from_press,
        set_selection_from_press,
        0x0040,
        "0x0040 — the live selection was anchored by a **mouse press** rather than by \
         select-all. Set immediately after the selecting bit is raised on a press, \
         and cleared by every path that clears 0x0080 \
         (set-selecting, deselect and delete-selection)."
    );
    bit!(
        selecting,
        set_selecting,
        0x0080,
        "0x0080 — a selection drag is in progress."
    );
    bit!(dirty, set_dirty, 0x0100, "0x0100 — needs re-layout.");
    bit!(
        fit_to_text,
        set_fit_to_text,
        0x0400,
        "0x0400 — fit the element to the text, attribute 0x29."
    );
    bit!(
        truncate,
        set_truncate,
        0x0800,
        "0x0800 — truncate text to fit."
    );
    bit!(
        lose_focus_on_escape,
        set_lose_focus_on_escape,
        0x1000,
        "0x1000 — lose focus on Escape."
    );
    bit!(
        lose_focus_on_accept,
        set_lose_focus_on_accept,
        0x2000,
        "0x2000 — lose focus on Accept (Enter)."
    );
}

/// The caret blink period.
///
/// The caret's half-period in seconds — the default for [`crate::UiSystem::caret_blink_time`].
///
/// It is not a constant in
/// retail at all: the text element's per-frame loop — the body its global-message listener
/// runs for global message 3 — reads **`USER32.dll!GetCaretBlinkTime`** and multiplies by
/// 0.001. So retail blinks at the player's own Windows
/// caret rate, in milliseconds, read afresh on every tick. 530 ms is that setting's default; the
/// client sets the field from the OS when it can.
pub const CARET_BLINK_PERIOD: f64 = 0.53;

/// `TextElement`.
#[derive(Debug)]
pub struct TextElement {
    pub glyphs: GlyphList,
    /// The caret position, as a glyph index.
    pub cursor: usize,
    /// The selection start and end.
    pub selection: Selection,
    pub bits: TextBits,
    /// The horizontal and vertical justification.
    pub h_justify: u32,
    pub v_justify: u32,
    /// The input filter.
    pub filter: Option<InputFilter>,
    /// The current font colour.
    pub font_color: u32,
    /// The outline colour — opaque black by default.
    pub outline_color: u32,
    /// The up, down, left and right margins.
    pub margins: (i32, i32, i32, i32),
    /// When `caret_visible` last changed.
    /// The blink update stamps it whenever it writes bit `0x200`.
    pub last_flash_flip: f64,
    /// Bit `0x200` — the caret is on this half-period. The constructor sets
    /// the bits to `0x300`, so it starts **on**.
    pub caret_visible: bool,
    /// "The last cursor move is newer than the last flip", kept as the predicate rather than the
    /// time.
    ///
    /// The cursor-position setter and the text-insert path stamp
    /// the current time; the global loop compares it with the last flip and,
    /// when the move is newer, forces the caret on and restarts the half-period. Neither writer
    /// has a clock here, but the only reader is the tick — which is also the only writer of the
    /// flip time — so "moved since the last flip" is exactly the comparison retail makes.
    pub caret_moved: bool,
    /// Whether a delete operation has removed a glyph since the last time the element told
    /// anybody — the delete-section's return value, carried to the one
    /// place in this build that can raise a message.
    ///
    /// After deleting a section, the client broadcasts message `0x44` when at
    /// least one glyph was removed.
    ///
    /// so **every** deletion that removed something raises `0x44 TEXT_CHANGED`, and all five of
    /// the client's delete entry points funnel through it: delete-char,
    /// delete-selection, behead-text, cut and
    /// paste. Raising `0x44` from the **insert** path only would mean a box
    /// emptied with backspace tells nobody: `SquelchPanel`'s `0x12`/`0x44` arm would never run and
    /// its two buttons would stay lit over an empty name box. Every screen that keys on `0x44`
    /// depends on this.
    ///
    /// A flag rather than a return, because [`Self::delete_char`] and [`Self::delete_selection`]
    /// are also reached from `insert_at_cursor`, `cut` and `paste`, none of which has an
    /// `ElemCtx` to broadcast through; [`Self::flush_text_deleted`] is the one drain, at the one
    /// entry point that does.
    text_deleted: bool,
    /// The font metrics used to measure new glyphs. The renderer supplies the real one.
    ///
    /// `Arc` rather than `Box` because one dat `Font` is shared by many elements: the data-patch
    /// screen alone puts `0x40000001` on two labels, and the metrics table is 210 records.
    pub metrics: Arc<dyn FontMetrics>,
    /// The current font's DataID, plus whatever else attribute **0x1A** names.
    ///
    /// The attribute is an **array**. Initialization selects element **0**, and later calls reach
    /// other entries by index, which is what each glyph's font field stores. Empty until the
    /// layout sets one.
    pub fonts: Vec<dereth_primitives::DataId>,
    /// The tag font color, initialized from element 0 of attribute **0x1D** — the colour a
    /// `Tell` tag run is drawn in, instead of the ordinary [`Self::font_color`] / `0x1B` one.
    /// [`Self::stamp_tag_spans`] is where it reaches a glyph.
    ///
    /// **The shipped chat log authors it**: `0x10000011`'s `0x1D` is a **one**-entry `Color` array
    /// holding `#00B200`, and so is `0x10000016`'s, on each of the five chat windows — fourteen
    /// elements of the gameplay tree carry `0x1D` and all fourteen carry that same green (the
    /// others are the `<EXAM>` panes `0x1000013C`/`0x1000013E`/`0x1000013F` and `0x10000163`).
    ///
    /// One entry is what makes the colour constant across channels. The append-with-font
    /// re-reads `0x1D` per run with the *same* index it uses for `0x1B`,
    /// but the font-colour helper bounds-checks that index against
    /// the array's own count and returns without writing when it is past the end.
    /// Chat type `0x1B` = 27 is past the end of a
    /// one-entry array, so the field keeps element 0 — while `0x1B` itself is a 34-entry array by
    /// then, because the chat colour table replaced the layout's single-entry one
    /// and builds no `0x1D`.
    ///
    /// With no `0x1D` at all the original field stays white, which is this field's default here.
    ///
    pub tag_font_color: u32,
    /// The tag applied to newly appended glyphs.
    pub current_tag: Option<Arc<TextTag>>,
    /// The truncation suffix (attribute 0xC7), typically "…".
    pub truncation_suffix: String,
    /// The suffix's own measured width.
    ///
    /// The client's `0xC7` arm builds the suffix's glyph list from the resolved
    /// `StringInfo` and measures it as one line with unbounded width, storing its width and
    /// height. It zeroes both when the
    /// `StringInfo` is not valid.
    pub cx_trailer: i32,
    /// The glyph index the suffix replaces, or `-1` for *"nothing
    /// is truncated"*. The draw swaps glyph lists here.
    pub truncation_pos: i32,
    /// The last line drawn, or `-1`.
    pub cx_adjusted_line_number: i32,
    /// That line's width once the suffix has replaced its tail.
    pub cx_adjusted_line_size: i32,
    /// The shared scrolling state embedded by this rebuild; the original text control inherited
    /// the same six pieces of scrolling state.
    ///
    /// Without it a chat log would draw its text from the top of the box and pin
    /// it there for ever: the newest line sliced off by the pane edge, the scrollbar with no
    /// travel to report and its arrows with nothing to move. See [`crate::scrollable::Scrollable`].
    pub scroll: crate::scrollable::Scrollable,
    /// The per-chat-type colour table hands
    /// this element, and the two append-with-font forms index into.
    ///
    /// [`Self::set_default_chat_color`] fills all `0x22` entries;
    /// [`Self::set_chat_color`]`(type, colour)` overrides one.
    /// Empty until somebody builds it, in which case the element's plain `font_color` is used —
    /// which is exactly what every non-chat text element wants.
    pub chat_colors: Vec<u32>,
    /// **The whole `0x1B` colour array**, which is the table
    /// the font-colour helper indexes — not just element 0, which is all
    /// [`Self::font_color`] keeps.
    ///
    /// The append-with-font's *third* argument is an index into this array (its
    /// second is an index into the `0x1A` font array, [`Self::fonts`]); the helper bounds-checks
    /// it against the array's own count and leaves the current font colour alone when it is out of
    /// range, which is why an element with a one-entry array is unaffected by a non-zero index.
    ///
    /// It is the same mechanism the chat log uses — the chat colour table
    /// builds property **`0x1B`** as
    /// an array of `0x22` colours on the chat log — and [`Self::chat_colors`] is this build's
    /// separate model of that same array. Authored arrays land here; `set_default_chat_color` and
    /// `set_chat_color` still write `chat_colors`, and that copy still wins, so nothing the chat
    /// does changes.
    pub font_colors: Vec<u32>,
}

impl Default for TextElement {
    fn default() -> Self {
        Self {
            glyphs: GlyphList::default(),
            cursor: 0,
            selection: Selection::default(),
            bits: TextBits::default(),
            h_justify: 0,
            v_justify: 0,
            filter: None,
            font_color: 0xFFFF_FFFF,
            outline_color: 0xFF00_0000,
            margins: (0, 0, 0, 0),
            last_flash_flip: 0.0,
            caret_visible: true,
            caret_moved: false,
            text_deleted: false,
            metrics: Arc::new(FixedMetrics {
                advance: 8,
                height: 16,
            }),
            fonts: Vec::new(),
            tag_font_color: 0xFFFF_FFFF,
            current_tag: None,
            truncation_suffix: String::new(),
            // The retail constructor leaves the truncation position and the adjusted line
            // number at `-1` and the suffix width at `0`.
            cx_trailer: 0,
            truncation_pos: -1,
            cx_adjusted_line_number: -1,
            cx_adjusted_line_size: 0,
            scroll: crate::scrollable::Scrollable::default(),
            chat_colors: Vec::new(),
            font_colors: Vec::new(),
        }
    }
}

pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
    Box::new(TextElement::default())
}

impl TextElement {
    /// Behavior: clears first, which is what attribute 0x17 does.
    pub fn set_text(&mut self, s: &str) {
        self.glyphs.flush();
        self.append_text(s);
        self.cursor = self.glyphs.len();
        self.selection = Selection::default();
    }

    /// Append text. Tags in the string are parsed and applied per glyph, which is how a
    /// chat line's clickable names survive into the glyph list.
    pub fn append_text(&mut self, s: &str) {
        let parsed = crate::text::tag::parse(s);
        let base = self.glyphs.len();
        self.glyphs.add_text(
            base,
            &parsed.text,
            self.metrics.as_ref(),
            self.font_color,
            0,
            self.current_tag.as_ref(),
        );
        self.stamp_tag_spans(base, &parsed.spans);
        self.bits.set_dirty(true);
        // The text setter stamps the cursor-move time on **both** its arms, so setting text is a
        // caret move: the next tick shows the caret solid.
        self.caret_moved = true;
    }

    /// Behavior: append a run in one **chat colour index** rather than in
    /// the element's own font colour.
    ///
    /// The chat-line append path is the only caller that
    /// matters and it makes exactly three of these per line: the `"\n"` separator and the body in
    /// the message's own type, and the speaker prefix in **`0xC`** whatever the type is. The
    /// colour comes from [`Self::chat_colors`]; with no table the element's `font_color` is used,
    /// which is what every other text element gets.
    /// **This is the two-argument form and it means "font 0".** The client's function
    /// takes *three* arguments and this one is
    /// [`Self::append_text_with_font_and_color`]`(s, 0, color_index)` — which is exactly what
    /// every caller means, because each stands in for a call site that
    /// pushes a literal `0` for the font.
    pub fn append_text_with_font(&mut self, s: &str, color_index: u8) {
        self.append_text_with_font_and_color(s, 0, color_index);
    }

    /// The append-with-font, **whole** — a run in its own font *and*
    /// its own colour.
    ///
    /// The second argument selects font array `0x1A`; the third selects both ordinary color array
    /// `0x1B` and tag color array `0x1D`. The client then appends without deleting a selection.
    ///
    /// So **arg2 indexes the font array `0x1A` and arg3 indexes the colour array `0x1B`**, and
    /// the per-run state is the element's current font / font colour at the moment
    /// the text insert stamps the new glyphs — which is what [`crate::text::glyph::Glyph`]'s
    /// own `font` and `color` already are here. `font_index` reaches [`Self::fonts`] through
    /// the font lookup and [`crate::text::place`], so it arrives at the draw data as a `DataId`;
    /// `color_index` is resolved by [`Self::font_color_at`].
    ///
    /// An index past the end of either array leaves that run on the element's current value,
    /// which is the bounds check both the font and the font-colour helper make before reading.
    /// **For the font that clamp is not
    /// cosmetic**: the font travels here as a per-glyph *index*, and
    /// [`crate::text::place`] resolves an index the array does not have to `DataId(0)`, which
    /// rasterises nothing at all -- so "leave it alone" has to become "element 0", which is
    /// what the current font was last set to by the font reset.
    ///
    /// `dereth_ui_screens::panels::statmgmt::resolve_font_and_color` is a **local copy of this
    /// same rule** that reads the arrays off the element's merged properties instead. The two
    /// agree, deliberately, and could be folded together; see [`Self::font_color_at`].
    pub fn append_text_with_font_and_color(&mut self, s: &str, font_index: u32, color_index: u8) {
        let color = self.font_color_at(color_index);
        let font_index = if (font_index as usize) < self.fonts.len() {
            font_index
        } else {
            0
        };
        let parsed = crate::text::tag::parse(s);
        let base = self.glyphs.len();
        self.glyphs.add_text(
            base,
            &parsed.text,
            self.metrics.as_ref(),
            color,
            font_index,
            self.current_tag.as_ref(),
        );
        self.stamp_tag_spans(base, &parsed.spans);
        self.bits.set_dirty(true);
        // The same text-insert tail as `append_text`: its call stamps the move.
        self.caret_moved = true;
    }

    /// The glyph builder's tail — the open tag is stamped on every
    /// glyph of its run **and chooses which of the element's two colours that glyph is drawn in**.
    ///
    /// For each glyph, the client uses tag color attribute `0x1D` only when the open tag's type is
    /// `Tell` (`0x10000001`); every other glyph uses ordinary font color attribute `0x1B`. It then
    /// stores the chosen color, current font, and open tag on the glyph.
    ///
    /// Three things the branch settles:
    ///
    /// 1. **Only type `0x10000001` (`Tell`)** takes the tag colour. A `DID`, `IID` or
    ///    `IIDEnum` run — the tag *kind* is the second word, the type is the first — is drawn in
    ///    the line's own colour like any other text.
    /// 2. It is the whole run, not the markup: the `<…>` characters are already gone by here
    ///    (see [`crate::text::tag::parse`]), so what changes colour is exactly the glyphs the tag covers.
    /// 3. It is per **glyph**, decided as the glyph is built, so a run that closes mid-string
    ///    puts the following text back on the ordinary colour — no tag is open any more by the
    ///    time the branch reads it.
    ///
    /// So a chat line's clickable name is drawn in [`Self::tag_font_color`] — `#00B200` on the
    /// shipped log — while the rest of the line keeps its channel colour. Without this the name
    /// takes the line colour and nothing but the cursor marks it as clickable.
    fn stamp_tag_spans(&mut self, base: usize, spans: &[crate::text::tag::TagSpan]) {
        let tag_color = self.tag_font_color;
        for span in spans {
            // The client compares the tag's type against `0x10000001` -- the *first* mapped word.
            let tell = span.tag.type_id() == Some(crate::text::tag::TELL);
            let tag = Arc::new(span.tag.clone());
            for i in span.start..span.end {
                if let Some(g) = self.glyphs.glyphs.get_mut(base + i) {
                    g.tag = Some(Arc::clone(&tag));
                    if tell {
                        g.color = tag_color;
                    }
                }
            }
        }
    }

    /// Fill the whole chat colour table with one colour.
    ///
    /// Chat initialization calls it with green and a loop count of `0x22`, then overrides
    /// fourteen groups with [`Self::set_chat_color`].
    pub fn set_default_chat_color(&mut self, color: u32, count: usize) {
        self.chat_colors = vec![color; count];
    }

    /// Set one entry of the chat colour table.
    pub fn set_chat_color(&mut self, ty: u8, color: u32) {
        if let Some(slot) = self.chat_colors.get_mut(ty as usize) {
            *slot = color;
        }
    }

    /// The colour one chat type is drawn in, or the element's own `font_color` when it has no
    /// table — a text element that is not a chat log has none and must not be changed by this.
    #[must_use]
    pub fn chat_color(&self, ty: u8) -> u32 {
        self.font_color_at(ty)
    }

    /// Read element `n` of color-array attribute `0x1B`,
    /// which is what the append-with-font's third argument selects.
    ///
    /// Three sources, in this order:
    ///
    /// 1. [`Self::chat_colors`], the chat log's own model of the same array. It wins.
    /// 2. [`Self::font_colors`], the array the **layout** authored at `0x1B`.
    ///    For `n == 0` it is the same value `font_color` holds (both are element 0
    ///    after the font reset).
    /// 3. `font_color`, which is the helper's own out-of-range behaviour: it leaves
    ///    the current font colour where it was.
    #[must_use]
    pub fn font_color_at(&self, n: u8) -> u32 {
        self.chat_colors
            .get(n as usize)
            .or_else(|| self.font_colors.get(n as usize))
            .copied()
            .unwrap_or(self.font_color)
    }

    /// Behavior: insert at the caret and advance it.
    ///
    /// **An editable box deletes its selection first**, which is the function's own second step:
    /// an editable box deletes the selection, and otherwise, unless flag 1 is passed, selecting is
    /// turned off. The place it shows is a box whose whole content is selected
    /// — which is exactly what [`Self::select_all`] is for. The character-generation screen puts
    /// `ID_CharGen_NamePrompt` in the empty name box and selects all, so in retail the first
    /// keystroke replaces the placeholder; without this the name a player types comes out as
    /// `"[ Name ]Tarinell"`.
    pub fn insert_at_cursor(&mut self, ch: u16) {
        if self.bits.editable() {
            self.delete_selection();
        }
        self.glyphs.add_units(
            self.cursor,
            [ch],
            self.metrics.as_ref(),
            self.font_color,
            0,
            self.current_tag.as_ref(),
        );
        self.cursor += 1;
        self.bits.set_dirty(true);
        // The insert path stamps the cursor-move time: the caret stays
        // solid while the player is typing.
        self.caret_moved = true;
    }

    /// Change existing text to a new font — swap the font every existing
    /// glyph was measured with, **keeping each glyph's colour, font index and tag**.
    ///
    /// The client changes the glyph-list font, sets its re-layout bit, and dirties the root.
    ///
    /// The chat interface's font-settings-changed notice calls this and
    /// then [`Self::set_font_did_without_changing_existing_text`], in that order — the first
    /// re-measures the backlog that is already there, the second makes the *next* appended line
    /// take the same font without re-running the first.
    pub fn change_existing_text_to_new_font(&mut self, m: Arc<dyn FontMetrics>) {
        self.glyphs.change_font(m.as_ref());
        self.metrics = m;
        self.bits.set_dirty(true);
    }

    /// Set the font `DataID` **without** re-measuring the text already in the list.
    ///
    /// The client clears a global "change text in the font reset" flag, sets the font `DataID`,
    /// and sets the flag again.
    ///
    /// The global is the one [`Self::do_font_reset`]'s tail reads, so this is the font setter with
    /// the re-set of the tagged text suppressed — the element's font-0 `DataID` moves and the
    /// glyphs already in the list are left exactly as they are.
    pub fn set_font_did_without_changing_existing_text(&mut self, did: dereth_primitives::DataId) {
        if self.fonts.is_empty() {
            self.fonts.push(did);
        } else {
            self.fonts[0] = did;
        }
        self.bits.set_dirty(true);
    }

    /// Delete the character at the caret.
    pub fn delete_char(&mut self) {
        if self.cursor < self.glyphs.len() {
            self.glyphs.delete(self.cursor, self.cursor + 1);
            self.bits.set_dirty(true);
            // The client's tail; see [`Self::text_deleted`].
            self.text_deleted = true;
        }
    }

    /// Delete the selection.
    ///
    /// When there is a selection the client deletes that section and then does what turning
    /// selecting off does: `0x80` and `0x40` cleared, both endpoints 0.
    ///
    /// **Gated on [`Self::get_selection`], not on the endpoints**: a pair of stale
    /// endpoints with the selecting bit clear is not a selection and the client does not delete
    /// it. The caret landing on `a` is the section delete's, and it is what
    /// [`Self::insert_at_cursor`] then inserts at.
    pub fn delete_selection(&mut self) {
        let Some((a, b)) = self.get_selection() else {
            return;
        };
        if a == b {
            return;
        }
        self.glyphs.delete(a, b);
        self.cursor = a;
        self.set_selecting(false);
        self.bits.set_dirty(true);
        // The client's tail; see [`Self::text_deleted`].
        self.text_deleted = true;
    }

    /// The client's tail, raised where this build can raise it.
    /// See [`Self::text_deleted`] for the rule and for why it is a flag.
    ///
    /// Broadcast rather than queued, as the client does it: the section delete broadcasts the
    /// element message inside its own frame, and the listener this exists for is an
    /// **ancestor** (`SquelchPanel` is the panel above the edit box), which the bubble reaches
    /// whether or not this element's own slot is lifted.
    fn flush_text_deleted(&mut self, ctx: &mut ElemCtx<'_>) {
        if std::mem::take(&mut self.text_deleted) {
            ctx.ui
                .broadcast_element_message(ctx.me, msgid::TEXT_CHANGED, 0, 0);
        }
    }

    /// Behavior: the untagged text of the selection when there is one, so a box
    /// that is not selecting copies **nothing**.
    #[must_use]
    pub fn selected_text(&self) -> String {
        let Some((a, b)) = self.get_selection() else {
            return String::new();
        };
        String::from_utf16_lossy(
            &self.glyphs.glyphs[a.min(self.glyphs.len())..b.min(self.glyphs.len())]
                .iter()
                .map(|g| g.data)
                .collect::<Vec<_>>(),
        )
    }

    /// Behavior: **without** tags, so a link is copied as its display text.
    ///
    /// The client sends selected text to the device clipboard only while selection bit `0x80` is set.
    ///
    /// The destination is the device clipboard and not the element's — see
    /// [`crate::UiSystem::clipboard`], which carries the Win32 format the client writes. A
    /// `String` on this element would put text copied out of the
    /// chat log somewhere only the chat log could reach.
    ///
    /// The `0x80` gate is inside [`Self::selected_text`], which is where retail's selected-text
    /// read puts it; an unselecting box therefore clears nothing and writes nothing.
    ///
    /// The clipboard is passed in rather than reached through `ElemCtx` so that the two callers
    /// that are *not* an element message — a test, and whatever host operation writes the
    /// device clipboard — can drive it without an arena.
    ///
    /// Returns whether the clipboard write was reached — `false` is the no-selection arm,
    /// where the client neither reads the selection nor writes the clipboard. The caller needs it
    /// to decide whether to disturb the *system* clipboard.
    pub fn copy(&mut self, clipboard: &mut String) -> bool {
        if self.get_selection().is_none() {
            return false;
        }
        *clipboard = self.selected_text();
        true
    }

    /// Behavior: copy, then delete the selection.
    pub fn cut(&mut self, clipboard: &mut String) -> bool {
        let sent = self.copy(clipboard);
        self.delete_selection();
        sent
    }

    /// Paste.
    ///
    /// The entire paste operation is gated on editable bit 1. Once clipboard text is available and
    /// nonempty, selection bit `0x80` alone decides whether to delete the current selection first.
    ///
    /// **The `editable` gate matters.** Without it a *selectable* box takes
    /// a paste, and the shipped chat log `0x10000011` is exactly that: it carries `0x27` and not
    /// `0x16`, and its input-map registration gives it input map **8**, which is where the
    /// paste action `0x24` lives. So `Ctrl+V` over the chat scrollback would insert the
    /// clipboard into the conversation history.
    ///
    /// The delete gate is the selecting bit **alone** and not "the range is non-empty" — an empty
    /// live selection still runs the selection delete, which is a no-op, and the difference is only
    /// visible as which function the client entered. It is written the client's way.
    pub fn paste(&mut self, clipboard: &str) {
        self.paste_with(clipboard, |this, ch| {
            this.character_handler(ch);
        });
    }

    /// The paste replaces CRLF with LF, then feeds every
    /// UTF-16 unit (including the terminating NUL) to the character handler, so
    /// one-line rules and the input filter apply to paste too. The UI caller supplies the full
    /// handler including caret scrolling and element notifications; standalone callers use its
    /// model.
    fn paste_with(&mut self, clipboard: &str, mut character: impl FnMut(&mut Self, u16)) {
        if !self.bits.editable() || clipboard.is_empty() {
            return;
        }
        if self.bits.selecting() {
            self.delete_selection();
        }
        let normalized = clipboard.replace("\r\n", "\n");
        for ch in normalized.encode_utf16().chain(std::iter::once(0)) {
            character(self, ch);
        }
    }

    /// Behavior: the caret, the selection anchor and the
    /// select-all-on-first-click shortcut.
    ///
    /// The client snapshots focus, runs the inherited mouse-down behavior, and then handles a
    /// primary press over the element. A newly focused `0xD1` box selects all. Otherwise it maps
    /// the pointer to a glyph, moves the caret without selecting, and starts a selectable drag;
    /// Shift preserves the old anchor and extends to the new caret.
    ///
    /// **The focus take in the middle is `UiSystem::mouse_down`'s step 7 here, not this
    /// function's**, because it must run *after* the 0x1C broadcast this arm is delivered by:
    /// whether it had focus is read before the chain and this arm therefore still sees the old answer, which
    /// is exactly what the select-all shortcut turns on. Both halves are in the same press and no
    /// frame passes between them.
    ///
    /// **The shift-key test is [`crate::InputPump::shift_key_down`], latched once a frame into
    /// `UiSystem::shift_key_down`.** With shift held the press *extends* the existing selection instead of starting
    /// a new one, and the anchor is read **before** [`Self::set_cursor_position`] moves the
    /// caret: the selection start while the selecting bit (0x80) is up, and the old caret when it
    /// is not.
    /// Reading it afterwards would anchor every shift-click on itself and select nothing, which is
    /// exactly the shape of defect this file's other comments warn about — it still "works", it
    /// just always produces an empty range.
    ///
    /// `set_selecting(true)` and the `0x40` press-anchored flag run on **both** legs: the shift leg
    /// is an extension of a selection, not a bypass of the machinery that makes one legible.
    fn mouse_down(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) {
        let Some(n) = ctx.ui.node(ctx.me) else { return };
        if n.flags.is_moving() || n.flags.is_resizing() {
            return;
        }
        let a = m.p1;
        if !(a == action::PRIMARY_CLICK || a == action::WHEEL_UP || a == action::WHEEL_DOWN) {
            return;
        }
        if !n.region.flags.mouse_over_top {
            return;
        }
        if a != action::PRIMARY_CLICK || !(self.bits.editable() || self.bits.selectable()) {
            return;
        }
        let had_focus = ctx.ui.focus_element() == Some(ctx.me);
        let select_all = n
            .merged_properties()
            .get_bool(crate::props::attr::TEXT_SELECT_ALL_ON_FOCUS)
            .unwrap_or(false);
        if !had_focus && select_all {
            self.select_all();
            return;
        }
        let screen = ctx.ui.screen_box(ctx.me);
        let (x, y) = m.point.window;
        let pos = self.position_from_xy(screen, x, y);
        // The anchor is the selection start while 0x80 is set and the caret otherwise, read
        // **before** `set_cursor_position` moves the caret.
        let anchor = if self.bits.selecting() {
            self.selection.start
        } else {
            self.cursor
        };
        let shift = ctx.ui.shift_key_down;
        // **`DontSelectText`, and it is not decoration.** The caret move is what
        // drops whatever the *previous* gesture selected: the client's `else`
        // arm turns selecting off when 0x80 is set, which zeroes both endpoints.
        // The shift leg below re-arms from `anchor`, which is why the anchor is read first.
        self.set_cursor_position(ctx, pos, CursorMovementFlags::DontSelectText);
        if self.bits.selectable() {
            // Selecting is turned on and 0x40 is set — and 0x40 is what marks the live
            // selection as one a *press* anchored, so that `set_selecting(false)` can clear it
            // again.
            self.set_selecting(true);
            self.bits.set_selection_from_press(true);
            if shift {
                // With shift down the start goes to the anchor, the end to the caret, and the
                // handler returns — the range grows from where it
                // already began to where the player just clicked.
                self.set_selection_start(anchor);
                self.set_selection_end(self.cursor);
                return;
            }
            self.set_selection_start(self.cursor);
        }
    }

    /// Behavior: **the drag half of a text selection**.
    ///
    /// With press-selection bit `0x40` clear, the inherited mouse-move behavior runs and text adds
    /// nothing. With it set, the point is mapped into text coordinates and the caret setter extends
    /// the selection in its default mode.
    ///
    /// That is the whole function. It carries **no** extension arithmetic of its own — the range
    /// grows inside [`Self::set_cursor_position`], whose `Default` arm tests exactly the
    /// `0x40` this one gates on. Writing the extension here instead would work for the sweep and
    /// silently lose shift-arrow selection, which shares the same tail.
    ///
    /// The early return is a **plain return**, not a fallthrough: the base owns window dragging and
    /// the text element adds nothing to it, so with `0x40` clear
    /// this element has no interest in the pointer at all.
    fn mouse_move(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) {
        if !self.bits.selection_from_press() {
            return;
        }
        let screen = ctx.ui.screen_box(ctx.me);
        let (x, y) = m.point.window;
        let pos = self.position_from_xy(screen, x, y);
        self.set_cursor_position(ctx, pos, CursorMovementFlags::Default);
    }

    /// The client's **`0x40` half** — the release that ends the sweep.
    ///
    /// The client remembers whether this action began here, calls the base mouse-up handler, then
    /// continues when selection-from-press bit `0x40` is set or the press began here. It maps the
    /// release point into text coordinates, extends the selection and clears `0x40` when needed,
    /// and invokes the glyph tag when the press began on this element.
    ///
    /// **`0x80` is deliberately left alone.** The release clears only the *press-anchored* bit, so
    /// the selecting bit and both endpoints survive it — which is the entire reason a retail
    /// selection stays highlighted after the button comes up and is still there for a copy. A
    /// release that also turned selecting off would produce a highlight that flickers for the
    /// duration of the drag and vanishes on release, which is a *worse* failure than none at all because it
    /// looks like it nearly works.
    ///
    /// **The pressed-here half** is the hyperlink dispatch — a
    /// clicked `<Tell:...>` name — and it fires the glyph's own tag object, not the selection. The
    /// caller tests only that this action existed in the element's mouse-down table; there is no
    /// action/button discriminator before the tag raises `(type, id, payload)`.
    fn mouse_up(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) {
        // p2 is the manager's synchronous adapter for retail's pre-base mouse-down-table
        // snapshot. It cannot outlive an authoritative press or survive a cancelled drag.
        let was_pressed_here = m.p2 != 0;
        if self.bits.selection_from_press() {
            let screen = ctx.ui.screen_box(ctx.me);
            let (x, y) = m.point.window;
            let pos = self.position_from_xy(screen, x, y);
            self.set_cursor_position(ctx, pos, CursorMovementFlags::Default);
            self.bits.set_selection_from_press(false);
        }
        if !was_pressed_here {
            return;
        }

        let screen = ctx.ui.screen_box(ctx.me);
        let (x, y) = m.point.window;
        let pos = self.position_from_xy(screen, x, y);
        let Some(tag) = self.tag_at(pos).cloned() else {
            return;
        };
        let Some(id) = crate::text::tag::click_notice(tag.kind) else {
            return;
        };
        let Some(tag_type) = tag.type_id() else {
            return;
        };
        ctx.ui.send_notice(
            id,
            &crate::NoticePayload {
                a: tag_type,
                b: tag.id().unwrap_or(0),
                text: tag.payload().map(str::to_owned),
            },
        );
    }

    /// The cursor-position setter — **every caret move in the control goes
    /// through here, and this is where a selection is extended.** Nothing may assign the caret
    /// directly instead.
    ///
    /// Explicit select mode, or default mode while shift or press-selection bit `0x40` is active,
    /// starts a selection at the old caret when the element is selectable. Other moves clear a
    /// stale selection. The new position is clamped to the glyph count, scrolled into view when
    /// appropriate, stamped as a caret move, and installed as the selection end. A focused,
    /// editable element is dirtied when needed.
    ///
    /// **Three things here are easy to get wrong and each is observable:**
    ///
    /// * the anchor is the caret **before** the assignment three lines below, so it is where
    ///   the caret *was*. Anchoring on the new position selects nothing, for ever;
    /// * `Default` is conditional on `0x40 || shift`, so the *same call* both extends a live
    ///   sweep and leaves a bare click alone. Making it unconditional turns every click into a
    ///   selection that grows from wherever the caret happened to be;
    /// * the `else` arm drops a stale selection. Without it a fresh click somewhere else leaves the
    ///   old highlight on screen and a copy still yields the old text.
    ///
    /// The setter's final current-time stamp is [`Self::caret_moved`], which the caret blink reads.
    pub(crate) fn set_cursor_position(
        &mut self,
        ctx: &mut ElemCtx<'_>,
        pos: usize,
        mode: CursorMovementFlags,
    ) -> bool {
        let shift = ctx.ui.shift_key_down;
        let extends = mode == CursorMovementFlags::SelectText
            || ((self.bits.selection_from_press() || shift)
                && mode == CursorMovementFlags::Default);
        if extends {
            if !self.bits.selecting() && self.bits.selectable() {
                // The bit is raised **first**, because the selection-start setter refuses to
                // write without it.
                self.bits.set_selecting(true);
                self.set_selection_start(self.cursor);
            }
        } else if self.bits.selecting() {
            self.set_selecting(false);
        }
        self.cursor = pos.min(self.glyphs.len());
        self.caret_moved = true;

        // When the caret is not in view and the element has a bar or is editable or selectable
        // (bits 5), the client scrolls to it — this is what makes a sweep that runs off the
        // bottom of the chat log scroll it.
        //
        // Both of those calls open with a glyph-list recalculation —
        // the in-view test and the scroll-to-position both do it —
        // so the content extent the clamp below is measured against is refreshed
        // *here*, before either is asked. Without it the scrollable width is whatever the last
        // sweep left, and for a one-line entry box that nothing else re-measures it is the empty
        // box `post_init` sized: travel is negative, `set_scrollable_xy` clamps the offset to 0,
        // and the caret walks straight out of the element.
        self.recalculate_glyph_list(ctx);
        let screen = ctx.ui.screen_box(ctx.me);
        let has_bar = self.scroll.scrollbar(ctx.ui, ctx.me, true).is_some()
            || self.scroll.scrollbar(ctx.ui, ctx.me, false).is_some();
        if !self.is_position_in_view(screen, self.cursor)
            && (has_bar || self.bits.editable() || self.bits.selectable())
        {
            // The scroll-to-position, inlined rather than called: this element is lifted out
            // of the arena for the duration of its own handler, so the helper that reads it back
            // out (`chat::window::scroll_to_position`) would find an empty slot.
            if let Some((x, y)) = self.scroll_offset_for_position(screen, self.cursor) {
                let mut scroll = self.scroll;
                let moved = scroll.set_scrollable_xy(ctx.ui, ctx.me, x, y, false);
                self.scroll = scroll;
                if moved {
                    // The scrollable-offset change is adjusted for here.
                    self.bits.set_dirty(true);
                }
            }
        }

        if self.bits.selecting() {
            self.set_selection_end(self.cursor);
        }
        if ctx.ui.focus_element() == Some(ctx.me) && self.bits.editable() {
            self.bits.set_dirty(true);
        }
        true
    }

    /// Behavior: the **master switch** for the selection.
    ///
    /// Only an edge does anything. Turning on sets `0x80`. Turning off clears `0x80` and `0x40`
    /// and zeroes both endpoints.
    ///
    /// In the client `0x80` gates
    /// *every* reader of the range: [`Self::get_selection`] returns `(0, 0)` and **false** without
    /// it, and the selection start and end setters refuse to write. A pair of endpoints left over
    /// from an earlier gesture is therefore not a selection.
    pub fn set_selecting(&mut self, on: bool) {
        if on == self.bits.selecting() {
            return;
        }
        if on {
            self.bits.set_selecting(true);
            return;
        }
        self.bits.set_selecting(false);
        self.bits.set_selection_from_press(false);
        self.selection = Selection { start: 0, end: 0 };
    }

    /// Behavior: the range in ascending order, or **nothing at
    /// all** when the selecting bit (0x80) is clear. The client zeroes both outputs, and fills
    /// them with the min and max of the endpoints only while 0x80 is set.
    ///
    /// The draw path asks this function before it inverts a glyph's pixels, the copy path asks it
    /// before copying, and the delete path asks it
    /// before it deletes — so it is the one place the gate belongs.
    #[must_use]
    pub fn get_selection(&self) -> Option<(usize, usize)> {
        if !self.bits.selecting() {
            return None;
        }
        Some(self.selection.ordered())
    }

    /// Set the selection start. Refuses while the selecting bit is clear,
    /// clamps to the glyph count, and — this is easy to miss — **collapses the end onto the
    /// start**: the clamped start is also written as the end.
    pub fn set_selection_start(&mut self, pos: usize) -> bool {
        if !self.bits.selecting() {
            return false;
        }
        let p = pos.min(self.glyphs.len());
        self.selection = Selection { start: p, end: p };
        self.bits.set_dirty(true);
        true
    }

    /// Behavior: the same refusal and the same clamp.
    pub fn set_selection_end(&mut self, pos: usize) -> bool {
        if !self.bits.selecting() {
            return false;
        }
        self.selection.end = pos.min(self.glyphs.len());
        self.bits.set_dirty(true);
        true
    }

    /// Select all.
    ///
    /// The whole body is inside the selectable (`4`) gate: it sets `0x80`, sets the start to 0
    /// and the end to the glyph count, and sets the dirty bit `0x100` and dirties the root.
    ///
    /// **Three lines are each load-bearing:**
    ///
    /// * the **`selectable` gate** — a box that cannot be selected selects nothing, which is what
    ///   makes select-all safe to call on any element;
    /// * setting **the selecting bit**, which has to happen *first* because the two setters below
    ///   refuse to write without it. Without it this function is a no-op in the client;
    /// * the **dirty flag**, which is how the selection reaches the screen at all —
    ///   the draw inverts the selected glyphs' pixels.
    ///
    /// The character-generation summary page is the caller the wizard's name box depends on:
    /// it sets `ID_CharGen_NamePrompt` and then selects all, which is why the first character
    /// typed on the Summary page replaces the whole `[ Name ]` prompt.
    pub fn select_all(&mut self) {
        if !self.bits.selectable() {
            return;
        }
        self.set_selecting(true);
        let n = self.glyphs.len();
        self.set_selection_start(0);
        self.set_selection_end(n);
        self.bits.set_dirty(true);
    }

    /// Deselect / clear the selection.
    ///
    /// While `0x80` is set the client clears `0x80` and `0x40` and zeroes both endpoints; either
    /// way it then sets the dirty bit `0x100` and dirties the root.
    ///
    /// It does not collapse the range onto the *cursor* and leave
    /// the selecting bit alone. The client zeroes both endpoints and clears the bit, and the
    /// difference is observable — a deselected box in the client reports **no** selection, where
    /// one collapsed onto the caret reports an empty one that the selection query would still have
    /// answered `true` for.
    pub fn deselect(&mut self) {
        if self.bits.selecting() {
            self.bits.set_selecting(false);
            self.bits.set_selection_from_press(false);
            self.selection = Selection { start: 0, end: 0 };
        }
        self.bits.set_dirty(true);
    }

    /// Returns which glyph a point lands on, in **absolute screen
    /// coordinates**.
    ///
    /// The client is given the point less the left/up margins and the screen origin, i.e. the
    /// point made relative to the content box; [`Self::content_box`] applies the same two margins, so this
    /// takes the screen point and the element's screen box instead of pre-subtracting them.
    ///
    /// The layout is the one [`Self::compose`] draws — the same `wrap` and the same
    /// `calc_justification` — so the caret lands under the pointer wherever the text is justified
    /// and however it wrapped. A point past the end of a line answers that line's end, which is
    /// what makes clicking in the empty part of a one-line box put the caret after the text.
    ///
    /// **The scroll offset is part of the mapping.**
    /// The client adds the vertical scroll offset before selecting a line and adds the horizontal
    /// scroll offset before finding the position within that line.
    ///
    /// [`Self::compose`] and [`Self::selection_boxes`] both move the placed run by **minus**
    /// the scroll X/Y; this walk does not move, so the *pointer* must move into unscrolled
    /// content space instead — which is what the two additions above are. Without them the two
    /// halves disagree by exactly the scroll offset, and a chat log is always scrolled: the log
    /// follows the conversation, so by the second screenful every click lands on a glyph some
    /// hundreds of pixels above the one under the pointer. That is invisible in a test whose text
    /// fits the box, and it is the ordinary case in the running client.
    #[must_use]
    pub fn position_from_xy(&self, screen: crate::Box2D, px: i32, py: i32) -> usize {
        let content = self.content_box(screen);
        let n = self.glyphs.len();
        if !content.is_valid() || n == 0 {
            return 0;
        }
        let (px, py) = (px + self.scroll.x, py + self.scroll.y);
        let g = &self.glyphs.glyphs;
        let lines = crate::text::glyph::wrap(g, content.width(), self.glyphs.one_line);
        let text_h: i32 = lines.iter().map(|l| l.height).sum();
        // The same `max(box, content)` available extent [`crate::text::compose`] uses, matching
        // the client's two max comparisons. The two walks have to agree or
        // the caret lands on a different glyph than the one under the pointer, and they come
        // apart exactly when the text overflows its box.
        let max_w: i32 = lines.iter().map(|l| l.width).max().unwrap_or(0);
        let avail_h = crate::text::glyph::justification_extent(content.height(), text_h);
        let avail_w = crate::text::glyph::justification_extent(content.width(), max_w);
        let mut y = content.y0
            + crate::text::glyph::calc_justification(avail_h, text_h, 0, 0, self.v_justify);
        let last = lines.len().saturating_sub(1);
        for (i, line) in lines.iter().enumerate() {
            if py >= y + line.height && i != last {
                y += line.height;
                continue;
            }
            let mut x = content.x0
                + crate::text::glyph::calc_justification(avail_w, line.width, 0, 0, self.h_justify);
            let end = line.end.min(n);
            for (j, glyph) in g[line.start.min(n)..end].iter().enumerate() {
                // The half-width rule: the caret goes to whichever side of the glyph the pointer
                // is nearer, which is what makes a click between two letters unambiguous.
                if px < x + glyph.width / 2 {
                    return line.start + j;
                }
                x += glyph.width;
            }
            return end;
        }
        n
    }

    /// The tag under a glyph index, for the click path (the position-from-XY lookup, then the
    /// click handler).
    #[must_use]
    pub fn tag_at(&self, pos: usize) -> Option<&Arc<TextTag>> {
        self.glyphs.glyphs.get(pos).and_then(|g| g.tag.as_ref())
    }

    /// The character handler, the six documented steps.
    ///
    /// Step 6 is the one panels rely on: element message **0x12** fires for *every* character,
    /// including ones that were not inserted.
    pub fn character_handler(&mut self, ch: u16) -> bool {
        let mut inserted = false;
        if self.bits.editable() {
            // 3. tab and escape are ignored
            if ch != 0x09 && ch != 0x1B {
                if ch == 0x0A || ch == 0x0D {
                    // 4. newline, unless one-line
                    if !self.bits.one_line() {
                        self.insert_at_cursor(0x0A);
                        inserted = true;
                    }
                } else if ch >= 0x20 && ch != 0x7F {
                    // 5. anything below U+0020, and U+007F, is ignored
                    if self.filter.is_none_or(|f| f(ch)) {
                        self.insert_at_cursor(ch);
                        inserted = true;
                    }
                }
            }
        }
        inserted
    }

    fn lines_per_page(&self) -> usize {
        1
    }

    /// The client's tail: when the global "change text in the font reset" flag is set, re-set the
    /// text from its tagged form.
    ///
    /// Re-setting the text is what re-measures every glyph against the new font and re-stamps the
    /// new colour, and it is why the attribute order does not matter: a layout stream applies 0x17
    /// (the text) before 0x1A (the font) because the ids sort that way, and the client repairs it
    /// here. The text is re-set from the tagged form, so links survive the round trip — exactly
    /// what the glyph list's `inq_text` operation reconstructs when tags are enabled.
    pub fn do_font_reset(&mut self) {
        if self.glyphs.is_empty() {
            return;
        }
        let pre_parsed = self.glyphs.inq_text(true);
        self.set_text(&pre_parsed);
    }

    /// The element's content rectangle: its screen box less the four margins
    /// (left, up, right, down).
    #[must_use]
    pub fn content_box(&self, screen: crate::Box2D) -> crate::Box2D {
        let (up, down, left, right) = self.margins;
        crate::Box2D {
            x0: screen.x0 + left,
            y0: screen.y0 + up,
            x1: screen.x1 - right,
            y1: screen.y1 - down,
        }
    }

    /// The total height of the wrapped lines, which dialog sizing compares against
    /// the element's own height.
    ///
    /// The same `wrap` the composer runs, and the same `sum` `crate::text::place` takes for its
    /// vertical justification — shared rather than re-derived, so a dialog can never be sized
    /// against a layout the draw does not perform.
    #[must_use]
    pub fn text_height(&self, screen: crate::Box2D) -> i32 {
        let content = self.content_box(screen);
        if !content.is_valid() || self.glyphs.is_empty() {
            return 0;
        }
        crate::text::glyph::wrap(&self.glyphs.glyphs, content.width(), self.glyphs.one_line)
            .iter()
            .map(|l| l.height)
            .sum()
    }

    // ---- TextElement's share of Scrollable ----------------------------------------

    /// The wrapped lines against the content width — the client's answer,
    /// without the write, so a `&self` reader gets the same layout the composer performs.
    fn wrapped(&self, screen: crate::Box2D) -> Vec<crate::text::glyph::GlyphLine> {
        let content = self.content_box(screen);
        if !content.is_valid() {
            return Vec::new();
        }
        crate::text::glyph::wrap(&self.glyphs.glyphs, content.width(), self.glyphs.one_line)
    }

    /// The glyph-list recalculation — the **dirty-gated re-measure** every
    /// reader of the layout runs before it reads it.
    ///
    /// When re-layout bit `0x100` is clear it returns. Otherwise it measures within the element
    /// width minus horizontal margins, adds all margins to the result, recalculates truncation when
    /// bit `0x800` is set (or clears both truncation positions), resizes the scrollable area,
    /// reapplies the current scroll offset with force enabled, and clears the dirty bit.
    ///
    /// Its five callers are the draw start, the position-from-XY lookup,
    /// **the in-view test**, **the scroll-to-position** and
    /// the delete-section — so the client never asks a question about the layout without
    /// first bringing it up to date, the caret path included.
    ///
    /// [`crate::scrollable::recalculate_dirty_text`] is the draw-start sweep of the same
    /// tail. The two do the same work and
    /// clear the same bit, so whichever runs first satisfies the other.
    ///
    /// The final `set_scrollable_xy` is `force = true` — no clamp — because its job is to re-push
    /// the *existing* offset through the scrollbar-position update now that the content has a new
    /// extent, not to move it.
    ///
    /// Returns whether anything was re-measured.
    ///
    /// **This is the whole function, not only its tail.** Both halves matter:
    ///
    /// * **The glyph-list recalculation runs.** An extent from a throwaway
    ///   `wrap` would leave the line table with whatever the last unrelated caller left in it, so
    ///   `find_current_line`, `find_xy_from_position` and the caret would read a stale line table.
    /// * **The `0x800` arm runs**, so the truncation position and the adjusted line
    ///   number are always written — at least to the `-1` the `else` arm writes.
    pub(crate) fn recalculate_glyph_list(&mut self, ctx: &mut ElemCtx<'_>) -> bool {
        if !self.bits.dirty() {
            return false;
        }
        let screen = ctx.ui.screen_box(ctx.me);
        let (w, h) = self.recalculate_layout(screen);
        let mut scroll = self.scroll;
        // Cleared **before** the writes, as the client clears it at the end of a body that cannot
        // re-enter: the scrollable-area resize broadcasts an element message and the bit must
        // already be down when a listener asks.
        self.bits.set_dirty(false);
        scroll.resize_scrollable_area(ctx.ui, ctx.me, w, h);
        let (x, y) = (scroll.x, scroll.y);
        scroll.set_scrollable_xy(ctx.ui, ctx.me, x, y, true);
        self.scroll = scroll;
        true
    }

    /// The recalculation's body **up to** the scrollable-area resize — the half that
    /// touches only this element, so it can run without the arena.
    ///
    /// It measures within the element width minus horizontal margins, adds all four margins, and
    /// either recalculates truncation when bit `0x800` is set or clears both truncation positions.
    ///
    /// Returns the `(width, height)` the client hands the scrollable-area resize.
    pub fn recalculate_layout(&mut self, screen: crate::Box2D) -> (i32, i32) {
        let (up, down, left, right) = self.margins;
        let content_w = screen.width() - left - right;
        // Recalculation stores the new line table, which is the point.
        self.glyphs.recalculate(content_w);
        let mut w: i32 =
            self.glyphs.lines.iter().map(|l| l.width).max().unwrap_or(0) + left + right;
        let mut h: i32 = self.glyphs.lines.iter().map(|l| l.height).sum::<i32>() + up + down;
        if self.bits.truncate() {
            self.recalculate_truncation(screen, &mut w, &mut h);
        } else {
            self.truncation_pos = -1;
            self.cx_adjusted_line_number = -1;
        }
        (w, h)
    }

    /// The truncation recalculation's **geometry**, as read from retail.
    ///
    /// The extent compared is the width for a one-line element and the height otherwise; when the
    /// text needs no more than the box, both truncation fields go to `-1`. Otherwise the last line
    /// is 0 for one line, or the last complete line above the box height. The truncation position
    /// is how many whole glyphs of that line fit in the width less both horizontal margins and the
    /// suffix width. A one-line element then sets the adjusted line to 0, its size to the prefix
    /// width plus the suffix width, and the width to that plus both margins. A multi-line element
    /// sums lines `0..=last_line` (the last one at its adjusted size), sets the adjusted line to
    /// `last_line`, and sets width and height to the widest line and the total height plus margins.
    ///
    /// **The tooltip half is not here.** The boolean `0xD0` probe and the
    /// tooltip set / clear it gates need the arena, and they are
    /// [`crate::UiSystem::recalculate_truncation_tooltips`], which runs a step earlier in
    /// the frame — at the top of `use_time`, one line before the tooltip check reads the flag it
    /// writes. Splitting it that way keeps **one** copy of each half: the geometry writes the
    /// three fields the draw reads, the sweep writes the tooltip the hover reads, and neither
    /// repeats the other's work.
    ///
    /// **The `0x800` path is live.** `0xC7` is authored on 32 elements, but the live tree has
    /// **106 instances (30 distinct) arming it**, 105 of them one-line, and **not one of them
    /// names a scrollbar** [measured over the shipped layouts]. So the arm that runs is the
    /// one-line arm, which rewrites the width only — and the description panes with scrollbars
    /// cannot be reached by it.
    fn recalculate_truncation(&mut self, screen: crate::Box2D, w: &mut i32, h: &mut i32) {
        let one_line = self.bits.one_line();
        let (up, down, left, right) = self.margins;
        let box_extent = if one_line {
            screen.width()
        } else {
            screen.height()
        };
        let needed = if one_line { *w } else { *h };
        if needed <= box_extent {
            self.truncation_pos = -1;
            self.cx_adjusted_line_number = -1;
            return;
        }
        let last_line = if one_line {
            0
        } else {
            self.glyphs
                .find_complete_line_from_y(box_extent)
                .unwrap_or(0)
        };
        let budget = screen.width() - right - left - self.cx_trailer;
        let Some(pos) = self
            .glyphs
            .find_pos_from_line_and_pixels_rounded(last_line, budget, false)
        else {
            self.truncation_pos = -1;
            self.cx_adjusted_line_number = -1;
            return;
        };
        self.truncation_pos = i32::try_from(pos).unwrap_or(i32::MAX);
        if one_line {
            let px = self.glyphs.find_pixels_from_pos(pos);
            self.cx_adjusted_line_number = 0;
            self.cx_adjusted_line_size = px + self.cx_trailer;
            *w = right + left + self.cx_adjusted_line_size;
            return;
        }
        let mut max_w = 0;
        let mut sum_h = 0;
        // `for (i = 0; i <= last_line; ++i)`. The complete-line-from-y fall-through hands back
        // the line count, so the client reads one `GlyphLine` past the end of its array — see
        // the glyph list's `find_complete_line_from_y` operation. The bug is transcribed as the
        // index it returns; the read is not, because there is no garbage here to read.
        for i in 0..=last_line {
            let Some(l) = self.glyphs.lines.get(i) else {
                break;
            };
            let lw = if i == last_line {
                self.cx_adjusted_line_size =
                    self.glyphs.find_pixels_from_pos(pos) + self.cx_trailer;
                self.cx_adjusted_line_size
            } else {
                l.width
            };
            max_w = max_w.max(lw);
            sum_h += l.height;
        }
        self.cx_adjusted_line_number = i32::try_from(last_line).unwrap_or(i32::MAX);
        *w = right + left + max_w;
        *h = down + up + sum_h;
    }

    /// Returns whether glyph `pos` is inside the visible band.
    ///
    /// A glyph with no position is not in view. For a multi-line element the test is vertical:
    /// with `view` the height less both vertical margins and `step` the smaller of `view` and the
    /// font's maximum character height (0 with no font), the glyph is in view when
    /// `scroll_y <= y && y + step <= scroll_y + view`. A one-line element tests horizontally
    /// instead, over the width less both horizontal margins: `scroll_x <= x && x + 2 <= scroll_x +
    /// view`.
    #[must_use]
    pub fn is_position_in_view(&self, screen: crate::Box2D, pos: usize) -> bool {
        let lines = self.wrapped(screen);
        let Some((x, y)) = xy_from(&lines, &self.glyphs, pos) else {
            return false;
        };
        let content = self.content_box(screen);
        if self.bits.one_line() {
            let view = content.width();
            return self.scroll.x <= x && x + 2 <= self.scroll.x + view;
        }
        let view = content.height();
        let step = self.metrics.height().min(view);
        self.scroll.y <= y && y + step <= self.scroll.y + view
    }

    /// Behavior: "an empty log is at the end, and otherwise the **last
    /// glyph** must be in view". This is the flag the final-string display handler reads
    /// *before* it appends, and the whole of the difference between a log that follows the
    /// conversation and one that raises the "new text below" arrow.
    #[must_use]
    pub fn is_at_vertical_end(&self, screen: crate::Box2D) -> bool {
        if self.glyphs.is_empty() {
            return true;
        }
        self.is_position_in_view(screen, self.glyphs.len() - 1)
    }

    /// The client's arithmetic, without the final offset write — the offset the
    /// element wants so that glyph `pos` is visible.
    ///
    /// Both views are the box less its margins. Multi-line: `x` becomes 0 unless the scrollable
    /// width exceeds the view width, and with `step` the smaller of the font's maximum character
    /// height and the view height, `y += step - view_h` when `scroll_y + view_h < step + y`. One
    /// line: `y` is `max(scrollable_height - view_h, 0)`, and `x += 2 - view_w` when
    /// `scroll_x + view_w < x + 2`. The result is then written unforced.
    ///
    /// Note what the multi-line arm does when the glyph is **above** the view: it scrolls to put
    /// it at the top. When it is below, it scrolls just far enough to put it at the bottom. That
    /// is the "pin to the bottom" behaviour the retail chat window shows and ours did not.
    #[must_use]
    pub fn scroll_offset_for_position(
        &self,
        screen: crate::Box2D,
        pos: usize,
    ) -> Option<(i32, i32)> {
        let lines = self.wrapped(screen);
        let (mut x, mut y) = xy_from(&lines, &self.glyphs, pos)?;
        let content = self.content_box(screen);
        let (view_w, view_h) = (content.width(), content.height());
        if self.bits.one_line() {
            y = (self.scroll.height - view_h).max(0);
            if self.scroll.x + view_w < x + 2 {
                x += 2 - view_w;
            }
        } else {
            if self.scroll.width - view_w < 1 {
                x = 0;
            }
            let step = self.metrics.height().min(view_h);
            if self.scroll.y + view_h < step + y {
                y += step - view_h;
            }
        }
        Some((x, y))
    }

    /// Behavior: the scroll-delta query, the step in **pixels** that the scrollable adds for one
    /// arrow click or one track
    /// click. This is the number that makes a scrollbar arrow do anything.
    ///
    /// The client takes the glyph at the view's top-left, its line, and that line's width
    /// (horizontal) or height (vertical); `step` is the smaller of that and the view's width or
    /// height. For a page, `step` becomes `view - step` when `step <= view - step`. The answer is
    /// negated when `negative`.
    ///
    /// This implementation uses `view - step` for page scrolling.
    #[must_use]
    pub fn inq_scroll_delta(
        &self,
        screen: crate::Box2D,
        horizontal: bool,
        negative: bool,
        page: bool,
    ) -> i32 {
        let lines = self.wrapped(screen);
        if lines.is_empty() {
            return 0;
        }
        // The position at `(0, 0)` is the glyph under the view's own origin, which with the
        // scroll offset applied is the first glyph of the line the offset currently sits on.
        let top = if horizontal {
            self.scroll.x
        } else {
            self.scroll.y
        };
        let mut acc = 0;
        let mut line = 0;
        for (i, l) in lines.iter().enumerate() {
            if acc + l.height > top {
                line = i;
                break;
            }
            acc += l.height;
            line = i;
        }
        let size = if horizontal {
            lines[line].width
        } else {
            lines[line].height
        };
        let content = self.content_box(screen);
        let view = if horizontal {
            content.width()
        } else {
            content.height()
        };
        let mut step = size.min(view);
        if page {
            step = if step <= view - step {
                view - step
            } else {
                step
            };
        }
        if negative {
            -step
        } else {
            step
        }
    }

    /// The content extent is given after every text change —
    /// the widest wrapped line and the sum of their heights, **plus the four margins**.
    ///
    /// The glyph-list recalculation measures text width and height, adds the horizontal margins to
    /// width and vertical margins to height, and gives that complete extent to the scrollable area.
    ///
    /// **The margins matter because the scrollbar-size update compares this against
    /// the element's height, the *whole* box** — not against the content box the lines were wrapped
    /// into. Leaving them out understates the content by exactly the two margins, which is enough
    /// to disable a bar whose content overflows by less than that.
    #[must_use]
    pub fn scrollable_extent(&self, screen: crate::Box2D) -> (i32, i32) {
        let lines = self.wrapped(screen);
        let (up, down, left, right) = self.margins;
        let w: i32 = lines.iter().map(|l| l.width).max().unwrap_or(0);
        let h: i32 = lines.iter().map(|l| l.height).sum();
        (w + left + right, h + up + down)
    }

    /// The size query with the max-width flag (flag 0) — **how big
    /// would this element have to be to show that string?**
    ///
    /// It measures a *throwaway* glyph list, so it answers for a string the element does not hold
    /// yet and leaves the element alone. The client measures a temporary list within the maximum
    /// width minus horizontal margins, using the element's one-line bit, then adds all four
    /// margins.
    ///
    /// The maximum width for flag 0 is attribute `0x3D` (`max_width`) if the element has one, else
    /// the display width — the caller passes whichever applies.
    ///
    /// The manager's start-tooltip is this method's only
    /// caller and could not size a tooltip without it; [`Self::scrollable_extent`] is the same
    /// arithmetic but only against the box the element already has, which for a 26-pixel-wide
    /// tooltip child is not the question.
    #[must_use]
    pub fn inq_size_with_margins(&self, s: &str, max_width: i32) -> (i32, i32) {
        let mut tmp = crate::text::GlyphList {
            one_line: self.glyphs.one_line,
            ..Default::default()
        };
        tmp.add_text(0, s, self.metrics.as_ref(), self.font_color, 0, None);
        let (up, down, left, right) = self.margins;
        let lines = crate::text::glyph::wrap(&tmp.glyphs, max_width - left - right, tmp.one_line);
        let w: i32 = lines.iter().map(|l| l.width).max().unwrap_or(0) + left + right;
        let h: i32 = lines.iter().map(|l| l.height).sum::<i32>() + up + down;
        (w, h)
    }

    /// The resize-to-paper's middle step — re-flow **this element's own**
    /// glyph list against `max_width` and answer the box the element would have to be.
    ///
    /// It recalculates the stored glyph list within `max_width` minus horizontal margins, adds all
    /// four margins to the measured box, and resizes the element to that result.
    ///
    /// It differs from [`Self::inq_size_with_margins`] in the one way that matters: that one
    /// measures a **throwaway** list built from a string the element does not hold, for the
    /// tooltip sizer. This one mutates the glyph list's wrap, because the recalculation is what
    /// fills its line table and every later walk — the caret, the click and the composer — reads
    /// the lines it left.
    ///
    /// **The glyph-list recalculation always returns true** — it has no failing exit. So the
    /// resize-to-paper's success test is always taken and this returns the
    /// pair unconditionally rather than an `Option`.
    pub fn recalculate_to_paper(&mut self, max_width: i32) -> (i32, i32) {
        let (up, down, left, right) = self.margins;
        self.glyphs.recalculate(max_width - left - right);
        let (w, h) = self.glyphs.extent();
        (w + left + right, h + up + down)
    }

    /// Lay the text out inside `screen` and place every glyph.
    ///
    /// This is the text element's share of step 6: the client composes the glyphs into its own
    /// surface and draws that surface as one quad; this emits the same placements as draw data. See
    /// [`crate::text::compose`].
    #[must_use]
    pub fn compose(&self, screen: crate::Box2D) -> Vec<crate::text::PlacedGlyph> {
        let content = self.content_box(screen);
        if !content.is_valid() || self.glyphs.is_empty() {
            return Vec::new();
        }
        // The draw's glyph-list swap: at the truncation position the walk switches to the
        // suffix list — from there onwards the pen draws the suffix (the ellipsis) and
        // nothing else, and the walk stops after the adjusted line number.
        //
        // The one deviation is that this rebuilds the run and re-wraps it rather than swapping
        // heads mid-walk, because the composer is a function of a glyph slice. The prefix was
        // chosen by the truncation recalculation precisely to fit the width less margins and
        // suffix, so prefix + suffix re-wraps to the same single line it replaced.
        let truncated: Option<Vec<crate::text::Glyph>> = (self.truncation_pos >= 0)
            .then(|| usize::try_from(self.truncation_pos).unwrap_or(0))
            .filter(|p| *p < self.glyphs.glyphs.len())
            .map(|p| {
                let mut v: Vec<crate::text::Glyph> = self.glyphs.glyphs[..p].to_vec();
                let mut tail = crate::text::GlyphList::default();
                tail.add_text(
                    0,
                    &self.truncation_suffix,
                    self.metrics.as_ref(),
                    self.font_color,
                    0,
                    None,
                );
                v.extend(tail.glyphs);
                v
            });
        let glyphs: &[crate::text::Glyph] = truncated.as_deref().unwrap_or(&self.glyphs.glyphs);
        let lines = crate::text::glyph::wrap(glyphs, content.width(), self.glyphs.one_line);
        let lines = match usize::try_from(self.cx_adjusted_line_number) {
            // The adjusted line number is the **last** line drawn, so the walk keeps `n + 1`.
            Ok(n) if truncated.is_some() && n + 1 < lines.len() => lines[..=n].to_vec(),
            _ => lines,
        };
        let mut placed = crate::text::place(
            glyphs,
            &lines,
            &self.fonts,
            content,
            self.h_justify,
            self.v_justify,
        );
        // The scrollable X/Y pair is a pixel offset into the
        // content, so the whole placed run moves by minus it. This is the one line that makes a
        // chat log show its *end* instead of its beginning.
        if self.scroll.x != 0 || self.scroll.y != 0 {
            for g in &mut placed {
                g.x -= self.scroll.x;
                g.y -= self.scroll.y;
            }
        }
        placed
    }

    /// The client's **selection** arm — the rectangles the client
    /// inverts with, one per selected glyph, in absolute
    /// screen coordinates with an inclusive far edge.
    ///
    /// This is the draw's reader of the selection *state*: `compose` reads the glyph list and
    /// never the range, so without this a char-gen name box that really is selected draws exactly
    /// like one that is not.
    ///
    /// The inversion flips the RGB of every pixel in the rectangle and **leaves alpha alone**,
    /// so on the client's own UI surface the highlight is the
    /// element's blit and its glyphs inverted together — not a filled block painted over them.
    ///
    /// The same `wrap`, `content_box` and scroll offset as [`Self::compose`], because the two are
    /// one walk: see [`crate::text::compose::place_selection`].
    #[must_use]
    pub fn selection_boxes(&self, screen: crate::Box2D) -> Vec<crate::Box2D> {
        let Some((a, b)) = self.get_selection() else {
            return Vec::new();
        };
        // There is deliberately **no** `a == b` early-out here. The retail draw has none either:
        // its only test is the per-glyph `sel_start <= i < sel_end`, which an empty range fails for
        // every glyph. One was written and mutation-tested: changing it to `a > b` **survived**,
        // because the inner test subsumes it exactly. A redundant branch that no test can
        // falsify is a place for a wrong assumption to hide, so it is gone.
        let content = self.content_box(screen);
        if !content.is_valid() || self.glyphs.is_empty() {
            return Vec::new();
        }
        let lines =
            crate::text::glyph::wrap(&self.glyphs.glyphs, content.width(), self.glyphs.one_line);
        let mut boxes = crate::text::place_selection(
            &self.glyphs.glyphs,
            &lines,
            content,
            self.h_justify,
            self.v_justify,
            (a, b),
        );
        // The same scroll X/Y shift `compose` applies, for the same reason: the client
        // composes into a surface whose origin has already been moved by the scroll.
        if self.scroll.x != 0 || self.scroll.y != 0 {
            for r in &mut boxes {
                r.x0 -= self.scroll.x;
                r.x1 -= self.scroll.x;
                r.y0 -= self.scroll.y;
                r.y1 -= self.scroll.y;
            }
        }
        boxes
    }

    /// The client's **caret** arm — the inclusive screen rectangle
    /// the client fills with the current font colour after its glyph loop; see
    /// [`crate::text::place_caret`] for the geometry. `None` unless the element is editable
    /// (bit `1`) and the caret is on this half-period (bit `0x200`); the third
    /// gate, focus, is the manager's to answer and [`crate::UiSystem::draw`] applies it.
    ///
    /// This is the draw's reader of [`Self::caret_visible`]; without it the blink state machine
    /// ticks and no text box ever shows a caret.
    #[must_use]
    pub fn caret_box(&self, screen: crate::Box2D) -> Option<crate::Box2D> {
        if !(self.bits.editable() && self.caret_visible) {
            return None;
        }
        let content = self.content_box(screen);
        if !content.is_valid() {
            return None;
        }
        let lines =
            crate::text::glyph::wrap(&self.glyphs.glyphs, content.width(), self.glyphs.one_line);
        let mut b = crate::text::place_caret(
            &self.glyphs.glyphs,
            &lines,
            content,
            self.h_justify,
            self.v_justify,
            self.cursor,
            self.metrics.height(),
        );
        // The same scroll X/Y shift `compose` applies.
        if self.scroll.x != 0 || self.scroll.y != 0 {
            b.x0 -= self.scroll.x;
            b.x1 -= self.scroll.x;
            b.y0 -= self.scroll.y;
            b.y1 -= self.scroll.y;
        }
        Some(b)
    }

    /// The editable attribute — attribute `0x16`'s arm of the attribute setter,
    /// and **not** a bare bit-write.
    ///
    /// An unchanged value returns immediately. Clearing editable while focused relinquishes focus
    /// only when selectable is also clear; becoming editable takes the ordinary redraw path. The
    /// editable bit is then set or cleared.
    ///
    /// This is retail's target-change focus edge on the identify window's inscription box.
    /// The item-examine window's inscription editable-state setter — the last line of its
    /// set-inscription, which runs on **every** appraisal reply — writes `0x16` and
    /// `0x27`, so a target the player may not write on takes the caret
    /// away from him. The unchanged-value early-out is just as load-bearing: re-writing the
    /// property with
    /// the value it already holds is not a transition, so switching to *another* item he may
    /// inscribe leaves the caret exactly where it was.
    ///
    /// The redraw leg (clear `0x800`, set `0x100`) is
    /// this crate's `dirty` bit and is left to the ordinary invalidation the setters already do.
    fn set_editable(&mut self, ctx: &mut ElemCtx<'_>, v: bool) {
        if v == self.bits.editable() {
            return;
        }
        if !v && !self.bits.selectable() && ctx.ui.focus_element() == Some(ctx.me) {
            ctx.ui.queue_relinquish_focus(ctx.me);
        }
        self.bits.set_editable(v);
    }

    /// The selectable attribute — attribute `0x27`'s arm, the same
    /// shape with the sibling bit swapped.
    ///
    /// An unchanged value returns immediately. Clearing selectable while focused relinquishes
    /// focus only when editable is also clear; becoming selectable takes the redraw path.
    ///
    /// **This is the call that actually fires** on the identify panel, because
    /// the inscription editable-state setter clears `0x16` first: the editable arm then sees
    /// `selectable` still set and keeps the caret, and this arm sees `editable` already cleared and drops it.
    fn set_selectable(&mut self, ctx: &mut ElemCtx<'_>, v: bool) {
        if v == self.bits.selectable() {
            return;
        }
        if !v && !self.bits.editable() && ctx.ui.focus_element() == Some(ctx.me) {
            ctx.ui.queue_relinquish_focus(ctx.me);
        }
        self.bits.set_selectable(v);
    }
}

/// An element-id-valued attribute: the layouts store them as enums.
fn element_id_value(v: Option<&PropertyValue>) -> Option<crate::ElementId> {
    match v {
        Some(PropertyValue::Enum(e)) => Some(crate::ElementId(*e)),
        Some(PropertyValue::Integer(i)) => u32::try_from(*i).ok().map(crate::ElementId),
        _ => None,
    }
}

/// Find a glyph position against a line list the caller wrapped, so the
/// scroll arithmetic and the composer share one layout rather than each re-deriving it.
fn xy_from(
    lines: &[crate::text::glyph::GlyphLine],
    glyphs: &GlyphList,
    pos: usize,
) -> Option<(i32, i32)> {
    if lines.is_empty() {
        return None;
    }
    let mut line = lines.len() - 1;
    for (i, l) in lines.iter().enumerate() {
        if pos < l.end {
            line = i;
            break;
        }
    }
    let y = lines[..line].iter().map(|l| l.height).sum();
    let l = &lines[line];
    let x = glyphs.glyphs[l.start..pos.clamp(l.start, l.end)]
        .iter()
        .map(|g| g.width)
        .sum();
    Some((x, y))
}

/// Element `n` of an array-typed property value, which is the shape every one of the font
/// attributes has: the font and font-colour helpers are given an index and bounds-check it
/// against the array's own count before reading.
fn array_element(v: Option<&PropertyValue>, n: usize) -> Option<&PropertyValue> {
    match v {
        Some(PropertyValue::Array(a)) => a.get(n).map(|e| &e.value),
        // A layout that writes a bare value where the client expects a one-element array is not a
        // shape any shipped file has, but reading it is free and loses nothing.
        Some(other) if n == 0 => Some(other),
        _ => None,
    }
}

/// Every `Color` entry of an array-typed property, in order — the element's colour set, which is
/// what indexes on attributes `0x1B` and `0x1D`.
fn array_colors(v: Option<&PropertyValue>) -> Vec<u32> {
    match v {
        Some(PropertyValue::Array(a)) => a
            .iter()
            .filter_map(|e| match &e.value {
                PropertyValue::Color(c) => Some(*c),
                _ => None,
            })
            .collect(),
        Some(PropertyValue::Color(c)) => vec![*c],
        _ => Vec::new(),
    }
}

/// Every `DataFile` entry of an array-typed property, in order — the element's font set.
fn array_data_files(v: Option<&PropertyValue>) -> Vec<dereth_primitives::DataId> {
    match v {
        Some(PropertyValue::Array(a)) => a
            .iter()
            .filter_map(|e| match &e.value {
                PropertyValue::DataFile(d) => Some(*d),
                _ => None,
            })
            .collect(),
        Some(PropertyValue::DataFile(d)) => vec![*d],
        _ => Vec::new(),
    }
}

/// Resolve `StringInfo` into the text the client would show.
///
/// `override_flag == 1` carries the literal in the file and needs no table; otherwise the pair
/// `(string id, table DataID)` is looked up in `client_local_<Language>.dat` through the host's
/// [`crate::text::StringResolver`]. With no resolver installed there is nothing to show and the
/// existing text is left alone — which is what a `UiSystem` built for a layout-only test wants.
pub(crate) fn resolve_string_info(
    ctx: &mut ElemCtx<'_>,
    si: &dereth_assets::ui::StringInfo,
) -> Option<String> {
    if let Some(lit) = &si.literal {
        return Some(lit.clone());
    }
    let (id, table) = (si.string_id?, si.table_id?);
    ctx.ui.resolve_string(table, id)
}

impl Element for TextElement {
    /// Behavior: true when the element is editable or
    /// selectable. That is what makes a label non-hittable but an edit box hittable.
    fn should_be_mouse_visible(&self) -> bool {
        self.bits.editable() || self.bits.selectable()
    }

    fn measured_text_height(&self, screen: crate::Box2D) -> Option<i32> {
        Some(self.text_height(screen))
    }

    fn as_text_mut(&mut self) -> Option<&mut TextElement> {
        Some(self)
    }

    fn compose_text(&self, screen: crate::Box2D) -> Vec<crate::text::PlacedGlyph> {
        self.compose(screen)
    }

    /// See [`Self::selection_boxes`].
    fn selection_boxes(&self, screen: crate::Box2D) -> Vec<crate::Box2D> {
        TextElement::selection_boxes(self, screen)
    }

    /// See [`Self::caret_box`]. The colour is the current font colour, the one
    /// the fill is handed.
    fn caret(&self, screen: crate::Box2D) -> Option<(crate::Box2D, u32)> {
        self.caret_box(screen).map(|b| (b, self.font_color))
    }

    /// **The reader of `TextBits::outline` and `outline_color`.**
    ///
    /// The glyph loop starts at pass 0 when bit `0x10` is set and at pass 1 when it is not, and
    /// runs to pass 2 either way
    /// — so the bit's only effect is whether the outline pass happens, and the colour it uses is
    /// the outline colour. Both are exactly what this returns.
    fn text_outline_color(&self) -> Option<u32> {
        self.bits.outline().then_some(self.outline_color)
    }

    /// The text element's attribute setter.
    fn on_set_attribute(&mut self, ctx: &mut ElemCtx<'_>, id: u32, v: Option<&PropertyValue>) {
        let b = matches!(v, Some(PropertyValue::Bool(true)));
        let i = match v {
            Some(PropertyValue::Integer(x)) => *x,
            _ => 0,
        };
        let e = match v {
            Some(PropertyValue::Enum(x)) => *x,
            _ => 0,
        };
        match id {
            attr::TEXT_H_JUSTIFY => self.h_justify = e,
            attr::TEXT_V_JUSTIFY => self.v_justify = e,
            attr::TEXT_EDITABLE => {
                self.set_editable(ctx, b);
                // Both text-capability arms share the attribute setter's tail:
                // the should-be-mouse-visible query, then the mouse-visible write. The
                // behaviour is lifted while this callback runs, so UiSystem defers the
                // query until `put_behaviour` restores it.
                ctx.ui.update_mouse_visibility(ctx.me);
            }
            attr::TEXT_STRING => {
                if let Some(PropertyValue::StringInfo(si)) = v {
                    // A `StringInfo` is resolved through the string
                    // tables **at the moment it is set**. The literal override (`override_flag == 1`)
                    // wins outright; everything else is `(string id, table DataID)` and needs the
                    // host's resolver, because this crate has no asset source.
                    if let Some(text) = resolve_string_info(ctx, si) {
                        self.set_text(&text);
                    }
                } else if let Some(PropertyValue::String(s)) = v {
                    let s = s.clone();
                    self.set_text(&s);
                }
            }
            attr::TEXT_MAX_CHARACTERS => {
                self.glyphs.max_characters = usize::try_from(i.max(0)).unwrap_or(0);
            }
            attr::TEXT_NO_IME => self.bits.set_no_ime(b),
            attr::TEXT_ONE_LINE => {
                self.bits.set_one_line(b);
                self.glyphs.one_line = b;
            }
            attr::TEXT_OUTLINE => self.bits.set_outline(b),
            attr::TEXT_OUTLINE_COLOR => {
                if let Some(PropertyValue::Color(c)) = v {
                    self.outline_color = *c;
                }
            }
            attr::TEXT_MARGIN_LEFT => self.margins.2 = i,
            attr::TEXT_MARGIN_RIGHT => self.margins.3 = i,
            attr::TEXT_MARGIN_TOP => self.margins.0 = i,
            attr::TEXT_MARGIN_BOTTOM => self.margins.1 = i,
            attr::TEXT_SELECTABLE => {
                self.set_selectable(ctx, b);
                ctx.ui.update_mouse_visibility(ctx.me);
            }
            attr::TEXT_TRIM_FROM_TOP => self.glyphs.trim_from_top = b,
            attr::TEXT_FIT_TO_TEXT => self.bits.set_fit_to_text(b),
            // Both arms default the value to false when the attribute carries none.
            // These are policies for the existing action-handler arms, not immediate focus changes.
            attr::TEXT_LOSE_FOCUS_ON_ESCAPE => self.bits.set_lose_focus_on_escape(b),
            attr::TEXT_LOSE_FOCUS_ON_ACCEPT => self.bits.set_lose_focus_on_accept(b),
            // The `0xC7` arm reads the value's `StringInfo` (if any), sets truncate-to-fit to
            // whether it is valid, and flushes the suffix list.
            //
            // **This arm must not require a *literal*: not one shipped `0xC7` is one.**
            // All 32 of them are `StringInfo { string_id, table_id: 0x23000001 }`, and
            // the string-info validity test is satisfied by that pair; it does not need the
            // table to be open. Requiring a literal would leave the truncate bit unset on every
            // element and the truncation recalculation — the generic tooltip site — could never run.
            //
            // The truncate-to-fit setter also **refuses** on an editable or selectable
            // element: it forces its argument to false
            // when either `0x1` (editable) or `0x4` (selectable) is set. An edit box does not
            // truncate.
            attr::TEXT_TRUNCATION_SUFFIX => {
                let valid = match v {
                    Some(PropertyValue::StringInfo(s)) => {
                        s.literal.is_some() || (s.string_id.is_some() && s.table_id.is_some())
                    }
                    _ => false,
                };
                if let Some(PropertyValue::StringInfo(s)) = v {
                    if let Some(text) = resolve_string_info(ctx, s) {
                        self.truncation_suffix = text;
                    }
                }
                let refused = self.bits.editable() || self.bits.selectable();
                self.bits.set_truncate(valid && !refused);
                // The case-199 tail clears the truncation list, resolves the suffix, selects the
                // element's **font 0**, and measures the suffix as one line with unbounded width.
                // That measurement is
                // the suffix width, and the recalculation subtracts it from the
                // budget so the ellipsis has somewhere to go. Without the width the budget is one
                // ellipsis too wide.
                self.cx_trailer = if valid && !refused {
                    let mut g = crate::text::GlyphList {
                        one_line: true,
                        ..Default::default()
                    };
                    g.add_text(
                        0,
                        &self.truncation_suffix,
                        self.metrics.as_ref(),
                        self.font_color,
                        0,
                        None,
                    );
                    g.recalculate(i32::MAX);
                    g.lines.iter().map(|l| l.width).max().unwrap_or(0)
                } else {
                    0
                };
            }
            // The font reset uses the font from 0x1A, the font colour from
            // 0x1B and the tag font colour from 0x1D, each at index 0; then, when the global
            // "change text in the font reset" flag is set, the text is re-set from its tagged form.
            //
            // **0x1B is not "font index/size" and 0x1D is not "font colour"**: the client reads
            // 0x1B as the *font colour* and 0x1D as the *tag* colour.
            //
            // Every shipped layout carries a `Color` array at 0x1B, and
            // fourteen elements of the gameplay tree also author 0x1D, all `#00B200`, the chat log
            // `0x10000011` among them. See [`Self::tag_font_color`].
            //
            // All three are **arrays** indexed by font number, and the helpers read element 0.
            attr::TEXT_FONT_DID => {
                if let Some(did) = array_element(v, 0).and_then(|e| match e {
                    PropertyValue::DataFile(d) => Some(*d),
                    _ => None,
                }) {
                    self.fonts = array_data_files(v);
                    if let Some(m) = ctx.ui.font_metrics(did) {
                        self.metrics = m;
                    }
                    self.do_font_reset();
                }
            }
            attr::TEXT_FONT_COLOR => {
                if let Some(PropertyValue::Color(c)) = array_element(v, 0) {
                    self.font_color = *c;
                    // The helpers read element 0; the append-with-font's third
                    // argument reads element *n*, so the whole array has to be kept. The shipped
                    // `<EXAM>` description pane `0x1000013C` authors **three** entries --
                    // `0xFFFFFFFF`, `0xFF00FF00`, `0xFFFF0000` -- which are exactly
                    // the item-examine window's plain font and its two modifier fonts (high and
                    // low).
                    self.font_colors = array_colors(v);
                    self.do_font_reset();
                }
            }
            attr::TEXT_TAG_FONT_COLOR => {
                if let Some(PropertyValue::Color(c)) = array_element(v, 0) {
                    self.tag_font_color = *c;
                }
            }
            // Behavior: the base class's own two arms,
            // which reach here because `TextElement : Scrollable` and this is the
            // only override in the chain. Each one re-sizes the bar it just learned about.
            crate::scrollable::attr::H_SCROLLBAR | crate::scrollable::attr::V_SCROLLBAR => {
                let bar = element_id_value(v);
                let horizontal = id == crate::scrollable::attr::H_SCROLLBAR;
                if horizontal {
                    self.scroll.h_scrollbar = bar;
                } else {
                    self.scroll.v_scrollbar = bar;
                }
                if bar.is_some() {
                    self.scroll
                        .update_scrollbar_size(ctx.ui, ctx.me, horizontal);
                }
            }
            // Ids 0x18, 0x19 and 0x2A-0xCA fall through to a no-op here: they belong to other
            // element types (section 3.7).
            _ => {}
        }
    }

    /// Behavior: chains to the base first, then acts **only on the
    /// press edge**, when the event's `start` field is true.
    fn on_action(&mut self, ctx: &mut ElemCtx<'_>, e: &InputEvent) -> bool {
        if !e.start {
            return false;
        }
        // Every one of the twelve arms of the client's own action handler lands the caret through
        // the cursor-position setter in `Default` mode, and the flag is a literal 0 at each of
        // the six call sites.
        // That routing *is* keyboard selection: `Default` extends the range when `0x40` or
        // shift is live and drops a stale one otherwise. Assigning the caret directly here
        // would make shift+arrow select nothing and leave a bare arrow
        // dragging an old highlight along behind it.
        if let Some(mode) = CursorTravelMode::from_action(e.action) {
            let pos = move_cursor(&self.glyphs, self.cursor, mode, self.lines_per_page());
            self.set_cursor_position(ctx, pos, CursorMovementFlags::Default);
            return true;
        }
        // The arms below are the client's five delete entry points minus the behead: `CUT`,
        // `PASTE`, `DELETE` and `BACKSPACE` all end in section deletion, whose tail
        // raises `0x44` when it removed anything. Bound and drained once rather than repeated in
        // four arms.
        let handled = match e.action {
            // The three clipboard actions reach the device's one buffer, not this element's — see
            // [`crate::UiSystem::clipboard`].
            // Copy reads selected text and sends it to the device clipboard. The second half is
            // the host hop, deferred through `UiSystem::pending_clipboard`
            // because this crate has no window. Only when there is a
            // selection: a Copy with nothing selected must not empty the user's real clipboard.
            action::COPY => {
                if self.copy(&mut ctx.ui.clipboard) {
                    ctx.ui.pending_clipboard = Some(ctx.ui.clipboard.clone());
                }
                true
            }
            action::CUT => {
                if self.cut(&mut ctx.ui.clipboard) {
                    ctx.ui.pending_clipboard = Some(ctx.ui.clipboard.clone());
                }
                true
            }
            action::PASTE => {
                let s = std::mem::take(&mut ctx.ui.clipboard);
                self.paste_with(&s, |this, ch| this.character(ctx, ch));
                ctx.ui.clipboard = s;
                true
            }
            action::ACCEPT => {
                if self.bits.lose_focus_on_accept() {
                    ctx.ui.relinquish_focus(ctx.me);
                }
                true
            }
            action::DELETE => {
                if self.get_selection().is_some_and(|(a, b)| a != b) {
                    self.delete_selection();
                } else {
                    self.delete_char();
                }
                true
            }
            action::ESCAPE => {
                if self.bits.lose_focus_on_escape() {
                    ctx.ui.relinquish_focus(ctx.me);
                }
                true
            }
            action::BACKSPACE => {
                if self.get_selection().is_some_and(|(a, b)| a != b) {
                    self.delete_selection();
                } else if self.cursor > 0 {
                    self.cursor -= 1;
                    self.delete_char();
                }
                true
            }
            _ => false,
        };
        self.flush_text_deleted(ctx);
        handled
    }

    /// The character input entry point. Element message **0x12** is broadcast for every character,
    /// inserted or not.
    fn character(&mut self, ctx: &mut ElemCtx<'_>, ch: u16) {
        let inserted = self.character_handler(ch);
        if inserted {
            // **The text-insert path advances through the cursor-position setter.**
            // It inserts at the old caret, adds the inserted count, and passes the resulting
            // position with `DontSelectText`.
            //
            // That call is the *only* thing that brings the view to a caret a keystroke has moved:
            // the cursor-position setter's middle scrolls to the caret when it is not in view and
            // the element has a bar or bits 5. Advancing
            // `self.cursor` inside [`Self::insert_at_cursor`] and stopping there would let typing
            // past the right-hand edge of a one-line box leave the caret drawn outside the element
            // — on the shipped chat entry `0x10000016`, 306 px wide, the caret would run past the
            // content box with the scroll X still 0.
            //
            // `insert_at_cursor` has already advanced the caret, so the position handed over is
            // the same caret-plus-count the client computes. Mode 2 is `DontSelectText`,
            // which is why a keystroke never extends a selection.
            let pos = self.cursor;
            self.set_cursor_position(ctx, pos, CursorMovementFlags::DontSelectText);
        }
        ctx.ui
            .broadcast_element_message(ctx.me, msgid::CHARACTER, u32::from(ch), 0);
        if inserted {
            ctx.ui
                .broadcast_element_message(ctx.me, msgid::TEXT_CHANGED, 0, 0);
        }
    }

    /// The global-message listener runs the element's global loop for message 3,
    /// and **that loop is the blink**. It runs only for a focused editable or selectable
    /// element, converts the system blink interval to seconds, forces the caret on after a newer
    /// cursor move, otherwise toggles it once the interval elapses, stamps the flip time, and
    /// dirties the previous caret rectangle.
    ///
    /// Flipping on **every** tick with no clock would be a flicker at frame rate,
    /// not a blink. [`Self::caret_moved`] is
    /// the "moved since the last flip" predicate; a caret being moved by typing
    /// therefore never blinks off under the keys, which is retail's feel.
    ///
    /// The element registers for message 3 **only while it is focused** (and editable or
    /// selectable, the element-message listener's `0x2F` arm) and unregisters otherwise.
    fn listen_to_global_message(&mut self, ctx: &mut ElemCtx<'_>, id: MessageId, _p: u32) {
        if id != crate::msg::global::TICK {
            return;
        }
        if !((self.bits.editable() || self.bits.selectable())
            && ctx.ui.focus_element() == Some(ctx.me))
        {
            ctx.ui.want_tick(ctx.me, false);
            self.caret_visible = true;
            return;
        }
        let now = ctx.ui.now.0;
        let on = if self.caret_moved {
            true
        } else {
            if now - self.last_flash_flip < ctx.ui.caret_blink_time {
                return;
            }
            !self.caret_visible
        };
        self.caret_moved = false;
        self.last_flash_flip = now;
        self.caret_visible = on;
    }

    /// Behavior: bind to whichever bars `0x71` and `0x72`
    /// name, and size them once against the content that already exists.
    ///
    /// The registration is what makes the messages arrive at all: the chat log `0x10000011` and
    /// its bar `0x10000012` are **siblings**, so nothing the bar broadcasts would ever bubble
    /// through the log.
    fn post_init(&mut self, ctx: &mut ElemCtx<'_>) {
        for horizontal in [true, false] {
            if let Some(bar) = self.scroll.scrollbar(ctx.ui, ctx.me, horizontal) {
                ctx.ui
                    .register_for_element_messages(bar, crate::ListenerId::Element(ctx.me));
            }
        }
        let screen = ctx.ui.screen_box(ctx.me);
        let (w, h) = self.scrollable_extent(screen);
        self.scroll.resize_scrollable_area(ctx.ui, ctx.me, w, h);
        self.scroll.update_scrollbar_size(ctx.ui, ctx.me, true);
        self.scroll.update_scrollbar_size(ctx.ui, ctx.me, false);
    }

    fn listen_to_element_message(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) -> R {
        // The client's first arm, and the reason a
        // scrollbar arrow moves anything: the **owner** of the bar consumes the gesture and
        // scrolls its own content. Answered before the `m.source != me` gate below, because the
        // whole point is that the message comes from somewhere else.
        if let Some(horizontal) =
            self.scroll
                .message_is_from_my_bar(ctx.ui, ctx.me, m.source_id, m.source)
        {
            let screen = ctx.ui.screen_box(ctx.me);
            let delta = if crate::scrollable::Scrollable::is_step_message(m.id) {
                // **The two negative ids are `0x0D` and `0x0F`**, not `0x0E` and `0x0F` as where
                // the arrows sit might suggest. The client builds
                // the scroll-delta query's arguments as `negative = (id == 0x0D || id == 0x0F)` —
                // the **negate** flag, which the query applies as
                // its last act — and `page = (id == 0x0F || id == 0x10)`, the page flag.
                //
                // `0x0F` is the unambiguous half: the track click raises
                // `0x10 - (clicked before the thumb)`, so `0x0F` is a click on the track *above*
                // the thumb and must page up. It groups with `0x0D`, which the scrollbar raises
                // for `delta = +1`, i.e. the **increment** arrow — the one moved to `(0, 0)`, the
                // top.
                //
                // Getting this backwards is invisible until you watch a real log: both arrows
                // still move the view, they just move it the wrong way, and the scrollbar's own
                // thumb follows the offset either way. The chat tests assert the direction.
                let negative = m.id.0 == 0x0D || m.id.0 == 0x0F;
                let page = m.id.0 == 0x0F || m.id.0 == 0x10;
                self.inq_scroll_delta(screen, horizontal, negative, page)
            } else {
                0
            };
            if self
                .scroll
                .handle_scrollbar_message(ctx.ui, ctx.me, horizontal, m.id, delta)
            {
                // Behavior: mark the glyph layout dirty.
                self.bits.set_dirty(true);
            }
            return R::StopProcessing;
        }
        // The scrollable's message handler, **second** arm: a wheel
        // click on this element is reflected off its own vertical bar as the same arrow message
        // the arm above consumes. The reasoning is on
        // [`crate::scrollable::Scrollable::wheel_target`].
        //
        // **The reflected message is applied here rather than waited for.** The bar does broadcast
        // it, and this element is registered for the bar's messages — but this element's own
        // behaviour is lifted out of the arena for the duration of this call, so that delivery
        // reaches an empty slot and is dropped in silence. The message is sent but never received,
        // because the widget cannot be read from the arena inside its own handler.
        if let Some(bar) = self.scroll.wheel_target(ctx.ui, ctx.me, m.id, m.p1) {
            let up = m.p1 == crate::focus::action::WHEEL_UP;
            if let Some(id) = crate::widgets::scrollbar::wheel(ctx.ui, bar, up) {
                let screen = ctx.ui.screen_box(ctx.me);
                // The same group as the arm above.
                let negative = id.0 == 0x0D || id.0 == 0x0F;
                let page = id.0 == 0x0F || id.0 == 0x10;
                let delta = self.inq_scroll_delta(screen, false, negative, page);
                if self
                    .scroll
                    .handle_scrollbar_message(ctx.ui, ctx.me, false, id, delta)
                {
                    self.bits.set_dirty(true);
                }
            }
            return R::StopProcessing;
        }
        if m.source != ctx.me {
            return R::Default;
        }
        if m.id == msgid::MOUSE_PRESS {
            self.mouse_down(ctx, m);
        }
        // **The two arms that make a selection possible.** `TextElement` overrides
        // all three mouse entry points in the client; with only the press, a
        // sweep would anchor an empty range and nothing would ever move its end. `0x1E` and `0x1D` are
        // the original element manager's mouse-move and mouse-up; both carry the pointer, and
        // `focus::UiSystem::mouse_move` already delivers `0x1E` to whoever holds the press
        // (`!self.mouse.pressed_on.is_empty()`), which is the condition a sweep runs under.
        if m.id == msgid::MOUSE_MOVE {
            self.mouse_move(ctx, m);
        }
        if m.id == msgid::MOUSE_RELEASE {
            self.mouse_up(ctx, m);
        }
        if m.id == msgid::FOCUS_CHANGED {
            let gained = m.p1 != 0;
            // The focus edge: the tick registration is
            // `if (editable || selectable)`, and the losing edge also deselects.
            ctx.ui.want_tick(
                ctx.me,
                gained && (self.bits.editable() || self.bits.selectable()),
            );
            if !gained {
                self.bits.set_selecting(false);
                // Deselect on the losing edge: otherwise a box that loses focus with a live
                // selection keeps it highlighted for ever, and the next `delete` would eat it.
                self.deselect();
            }
        }
        R::Default
    }

    /// **Every `TextElement` takes focus on a press, editable or not.**
    ///
    /// The client first snapshots focus and then runs the inherited mouse-down; the base is where
    /// the focus take lives. The bits-5 (editable or selectable) test
    /// sits *below* that call and guards the **caret/selection** arm only, not the focus. A plain
    /// label therefore takes focus in retail too, and the focus as it was *before* the
    /// base ran is exactly what the `0xD1` select-all-on-focus arm reads, which is only
    /// meaningful because the base has already changed it.
    ///
    /// See [`crate::element::Element::takes_focus_on_press`].
    fn takes_focus_on_press(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the text element's behavior-bit table.
    #[test]
    fn every_documented_text_bit_sits_on_its_documented_mask() {
        type Row = (u32, fn(&mut TextBits, bool), fn(TextBits) -> bool);
        let cases: &[Row] = &[
            (0x0001, TextBits::set_editable, TextBits::editable),
            (0x0002, TextBits::set_one_line, TextBits::one_line),
            (0x0004, TextBits::set_selectable, TextBits::selectable),
            (0x0008, TextBits::set_no_ime, TextBits::no_ime),
            (0x0010, TextBits::set_outline, TextBits::outline),
            (0x0080, TextBits::set_selecting, TextBits::selecting),
            (0x0100, TextBits::set_dirty, TextBits::dirty),
            (0x0400, TextBits::set_fit_to_text, TextBits::fit_to_text),
            (0x0800, TextBits::set_truncate, TextBits::truncate),
            (
                0x1000,
                TextBits::set_lose_focus_on_escape,
                TextBits::lose_focus_on_escape,
            ),
            (
                0x2000,
                TextBits::set_lose_focus_on_accept,
                TextBits::lose_focus_on_accept,
            ),
        ];
        for (mask, set, get) in cases {
            let mut b = TextBits::default();
            set(&mut b, true);
            assert_eq!(b.0, *mask);
            assert!(get(b));
        }
    }

    /// Oracle: the text element's character handler, steps 2–5.
    #[test]
    fn the_character_handler_follows_its_six_steps() {
        let mut t = TextElement::default();
        // 2. not editable -> nothing is inserted
        assert!(!t.character_handler(b'a' as u16));
        assert_eq!(t.glyphs.inq_text(false), "");

        t.bits.set_editable(true);
        assert!(t.character_handler(b'a' as u16));
        // 3. tab and escape are ignored
        assert!(!t.character_handler(0x09));
        assert!(!t.character_handler(0x1B));
        // 4. newline is ignored when one-line
        t.bits.set_one_line(true);
        assert!(!t.character_handler(0x0A));
        t.bits.set_one_line(false);
        assert!(t.character_handler(0x0A));
        // 5. control characters and DEL are ignored
        assert!(!t.character_handler(0x01));
        assert!(!t.character_handler(0x7F));
        assert!(t.character_handler(b'b' as u16));
        assert_eq!(t.glyphs.inq_text(false), "a\nb");
    }

    /// the input filter gates insertion; the description step 6 says the message fires anyway,
    /// which the manager-level test in `crate::tests` covers.
    #[test]
    fn the_filter_rejects_without_stopping_the_handler() {
        let mut t = TextElement::default();
        t.bits.set_editable(true);
        t.filter = Some(crate::text::edit::number_input_filter);
        assert!(!t.character_handler(b'a' as u16));
        assert!(t.character_handler(b'7' as u16));
        assert_eq!(t.glyphs.inq_text(false), "7");
    }

    /// Cut copy paste move the text and the caret.
    #[test]
    fn cut_copy_paste_move_the_text_and_the_caret() {
        let mut t = TextElement::default();
        t.bits.set_editable(true);
        t.bits.set_selectable(true);
        t.set_text("hello world");

        let mut clip = String::new();

        // (a) endpoints without the selecting bit are not a selection.
        t.selection = Selection { start: 0, end: 5 };
        assert_eq!(
            t.get_selection(),
            None,
            "0x80 clear -> get_selection returns None"
        );
        t.copy(&mut clip);
        assert_eq!(clip, "", "and the selected-text read copies nothing");

        // (b) the client's own way in: set_selecting first, then the two setters.
        t.set_selecting(true);
        assert!(t.set_selection_start(0));
        assert!(t.set_selection_end(5));
        assert_eq!(t.get_selection(), Some((0, 5)));
        t.copy(&mut clip);
        assert_eq!(clip, "hello");
        t.cut(&mut clip);
        assert_eq!(t.glyphs.inq_text(false), " world");
        assert_eq!(t.cursor, 0);
        // The client's tail clears 0x80 and zeroes both endpoints.
        assert_eq!(t.get_selection(), None, "the cut ended the selection");
        t.cursor = 6;
        t.paste(&clip);
        assert_eq!(t.glyphs.inq_text(false), " worldhello");
        assert_eq!(t.cursor, 11);
    }

    /// A selectable but uneditable box refuses a paste.
    #[test]
    fn a_selectable_but_uneditable_box_refuses_a_paste() {
        let mut t = TextElement::default();
        t.bits.set_selectable(true);
        t.set_text("log");
        t.paste("junk");
        assert_eq!(
            t.glyphs.inq_text(false),
            "log",
            "a chat log is not a paste target"
        );

        t.bits.set_editable(true);
        t.paste("junk");
        assert_eq!(
            t.glyphs.inq_text(false),
            "logjunk",
            "and an editable one is"
        );
    }

    /// Paste uses character rules and normalizes windows newlines.
    #[test]
    fn paste_uses_character_rules_and_normalizes_windows_newlines() {
        let mut t = TextElement::default();
        t.bits.set_editable(true);
        t.bits.set_one_line(true);
        t.paste("first\r\nsecond\nthird\rfourth\t\u{7f}");
        assert_eq!(t.glyphs.inq_text(false), "firstsecondthirdfourth");
        assert_eq!(t.cursor, 22, "only inserted code units advance the caret");

        t.set_text("");
        t.bits.set_one_line(false);
        t.paste("a\r\nb\nc\rd");
        assert_eq!(
            t.glyphs.inq_text(false),
            "a\nb\nc\nd",
            "CRLF is one newline"
        );

        t.set_text("");
        t.bits.set_one_line(true);
        t.filter = Some(crate::text::edit::number_input_filter);
        t.paste("1x2\r\n3!");
        assert_eq!(
            t.glyphs.inq_text(false),
            "123",
            "the character handler reads the filter"
        );

        t.set_text("");
        t.filter = None;
        t.paste("\u{4e2d}\u{1f642}");
        assert_eq!(
            t.glyphs.inq_text(false),
            "\u{4e2d}\u{1f642}",
            "preserve UTF-16 units"
        );
    }

    /// Select all needs selectable and turns the selecting bit on first.
    #[test]
    fn select_all_needs_selectable_and_turns_the_selecting_bit_on_first() {
        let mut t = TextElement::default();
        t.set_text("[ Name ]");

        // Not selectable: the whole body of select-all is inside the gate.
        t.bits.set_dirty(false);
        t.select_all();
        assert_eq!(
            t.get_selection(),
            None,
            "a box that is not selectable selects nothing"
        );
        assert!(!t.bits.selecting());
        assert!(!t.bits.dirty(), "and nothing was repainted");

        t.bits.set_selectable(true);
        t.select_all();
        assert!(
            t.bits.selecting(),
            "0x80 is set, and it has to be set FIRST"
        );
        assert_eq!(t.get_selection(), Some((0, 8)), "the whole glyph list");
        assert!(
            t.bits.dirty(),
            "the root is dirtied — this is how the inversion reaches the screen"
        );
        assert!(
            !t.bits.selection_from_press(),
            "select-all is not a press-anchored selection"
        );

        // Deselecting zeroes both endpoints rather than collapsing onto the caret.
        t.cursor = 4;
        t.deselect();
        assert_eq!(t.get_selection(), None);
        assert_eq!(t.selection, Selection { start: 0, end: 0 });
    }

    /// The selection boxes are the selected glyphs own cells.
    #[test]
    fn the_selection_boxes_are_the_selected_glyphs_own_cells() {
        let screen = crate::Box2D::new(100, 50, 299, 99);
        let mut t = TextElement::default();
        t.bits.set_selectable(true);
        t.set_text("[ Name ]");

        // (a) the gate: no selection, no rectangles — and this is the calibration for the zeroes.
        assert!(
            t.selection_boxes(screen).is_empty(),
            "nothing selected, nothing inverted"
        );
        // Endpoints written behind the bit's back are not a selection in the client either.
        t.selection = Selection { start: 0, end: 8 };
        assert!(
            t.selection_boxes(screen).is_empty(),
            "0x80 is clear, so the selection query refuses and the draw inverts nothing"
        );

        // (b) a partial selection: glyphs 2..6 of "[ Name ]".
        t.set_selecting(true);
        assert!(t.set_selection_start(2));
        assert!(t.set_selection_end(6));
        let glyphs = t.compose(screen);
        let r = t.selection_boxes(screen);
        assert_eq!(
            r.len(),
            4,
            "four glyphs are selected, so four cells are inverted"
        );
        for (i, cell) in r.iter().enumerate() {
            let g = glyphs[i + 2];
            assert_eq!(
                (cell.x0, cell.y0),
                (g.x, g.y),
                "cell {i} starts at glyph {}'s pen",
                i + 2
            );
            assert_eq!(
                cell.x1 - cell.x0 + 1,
                8,
                "cell {i} is the glyph's advance wide"
            );
            assert_eq!(cell.y1 - cell.y0 + 1, 16, "cell {i} is the whole line tall");
        }

        // (c) the whole box, which is what select-all leaves behind on the char-gen name field.
        t.select_all();
        assert_eq!(
            t.selection_boxes(screen).len(),
            8,
            "select-all inverts every glyph"
        );
        // (d) and an empty range inverts nothing, however the endpoints got there.
        assert!(t.set_selection_start(3));
        assert!(
            t.selection_boxes(screen).is_empty(),
            "start == end is not a highlight"
        );

        // (e) the scroll X/Y moves the highlight with the text it is on, because in the
        // client both are composed into a surface whose origin the scroll has already moved.
        // The char-gen name box never scrolls, so this is the only place the shift is
        // observable -- without it the mutation that deletes it survives against every suite.
        t.set_selecting(true);
        assert!(t.set_selection_start(0));
        assert!(t.set_selection_end(8));
        let unscrolled = t.selection_boxes(screen);
        t.scroll.x = 5;
        t.scroll.y = 3;
        let scrolled = t.selection_boxes(screen);
        assert_eq!(scrolled.len(), unscrolled.len());
        for (a, b) in scrolled.iter().zip(&unscrolled) {
            assert_eq!(
                (a.x0, a.y0),
                (b.x0 - 5, b.y0 - 3),
                "the cell moved with the glyph"
            );
            assert_eq!(
                (a.x1 - a.x0, a.y1 - a.y0),
                (b.x1 - b.x0, b.y1 - b.y0),
                "and kept its size"
            );
        }
        assert_eq!(
            scrolled.iter().map(|r| (r.x0, r.y0)).collect::<Vec<_>>(),
            t.compose(screen)
                .iter()
                .map(|g| (g.x, g.y))
                .collect::<Vec<_>>(),
            "and still starts where the scrolled glyph is drawn"
        );
    }

    /// Pinned behavior: this is what makes a label non-hittable but
    /// an edit box hittable".
    #[test]
    fn a_label_is_not_mouse_visible_but_an_edit_box_is() {
        let mut t = TextElement::default();
        assert!(!t.should_be_mouse_visible());
        t.bits.set_selectable(true);
        assert!(t.should_be_mouse_visible());
        t.bits.set_selectable(false);
        t.bits.set_editable(true);
        assert!(t.should_be_mouse_visible());
    }

    /// Appended tagged text puts the tag on every covered glyph.
    #[test]
    fn appended_tagged_text_puts_the_tag_on_every_covered_glyph() {
        let mut t = TextElement::default();
        t.append_text("give <Tell:IIDString:0x50001234:Shard>Shard<\\Tell> to");
        assert_eq!(t.glyphs.inq_text(false), "give Shard to");
        assert!(t.tag_at(4).is_none());
        for i in 5..10 {
            assert!(t.tag_at(i).is_some(), "glyph {i}");
        }
        assert!(t.tag_at(10).is_none());
        assert_eq!(t.tag_at(5).unwrap().id(), Some(0x5000_1234));
        assert_eq!(t.tag_at(9).unwrap().payload(), Some("Shard"));
        assert_eq!(
            t.tag_at(5).unwrap().type_id(),
            Some(0x1000_0001),
            "the Tell type"
        );
        assert_eq!(
            t.tag_at(5).unwrap().kind,
            crate::text::tag::TagKind::IidString
        );

        // The tag factory rejects it -> the glyph builder breaks out: every character of the run
        // is an ordinary glyph.
        let raw = "give <IIDString:Name:0x50001234:x>Shard<\\IIDString> to";
        let mut u = TextElement::default();
        u.append_text(raw);
        assert_eq!(
            u.glyphs.inq_text(false),
            raw,
            "an unmapped Format word is not markup"
        );
        assert!(
            u.glyphs.glyphs.iter().all(|g| g.tag.is_none()),
            "and tags nothing"
        );
    }

    /// A tell run takes the tag colour and nothing else on the line does.
    #[test]
    fn a_tell_run_takes_the_tag_colour_and_nothing_else_on_the_line_does() {
        let line = 0xFFB4_DCF0; // the blue-grey, chat type 0x1B's entry in the 0x1B array
        let tag = 0xFF00_B200; // the chat log's authored attribute 0x1D, element 0

        let mut t = TextElement {
            font_color: line,
            tag_font_color: tag,
            ..Default::default()
        };
        t.append_text("give <Tell:IIDString:0x50001234:Shard>Shard<\\Tell> to");
        assert_eq!(t.glyphs.inq_text(false), "give Shard to");
        assert_ne!(
            line, tag,
            "the two colours have to differ for this to measure anything"
        );
        for (i, g) in t.glyphs.glyphs.iter().enumerate() {
            let want = if (5..10).contains(&i) { tag } else { line };
            assert_eq!(
                g.color,
                want,
                "glyph {i} ({:?})",
                char::from_u32(u32::from(g.data))
            );
        }

        // A type other than `Tell` takes the font colour. The tag is still stamped.
        let mut u = TextElement {
            font_color: line,
            tag_font_color: tag,
            ..Default::default()
        };
        u.append_text("give <DID:DID:0x06000001>Shard<\\DID> to");
        assert_eq!(u.glyphs.inq_text(false), "give Shard to");
        for i in 5..10 {
            assert!(
                u.tag_at(i).is_some(),
                "glyph {i} is still a clickable DID run"
            );
        }
        assert_eq!(u.tag_at(5).unwrap().type_id(), Some(1), "DID, not Tell");
        assert!(
            u.glyphs.glyphs.iter().all(|g| g.color == line),
            "a DID run is not recoloured"
        );
    }
}
