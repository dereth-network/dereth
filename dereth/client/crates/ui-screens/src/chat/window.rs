//! One chat window's **elements** — the log, the entry line, the "new text below" arrow — and the
//! four things `ChatInterface` does to them.
//!
//! [`super::interface::ChatInterface`] is the model: routing, the filter, the history, the
//! truncation arithmetic. Nothing in it touches an element. This module carries a routed line onto
//! the log element with its own colour, scroll offset and scrollbar, rather than writing the whole
//! log with one `set_text` in one `font_color`.
//!
//! # The three children, and which is which
//!
//! The client binds three independent children by id: the log, the editable entry and the
//! new-text-below indicator. The main window's text-type filter separately defaults to
//! `0xfbffffff`, so that filter value cannot identify any of the three child roles. Their ids,
//! attributes and behavior do, as the shipped layout read from the live tree at 800×600 shows:
//!
//! | id | type | box | attributes it carries | what it is |
//! |---|---|---|---|---|
//! | `0x10000011` | text element | 368 × 73 | `0x27` selectable, **`0x72` = `0x10000012`** | the **log** |
//! | `0x10000016` | text element | 306 × 17 | **`0x16` editable**, `0x20` one-line, `0x27` selectable, `0x1E` = 255 | the **entry** |
//! | `0x1000048C` | button | 16 × 16, inside the log | `0x3B` = false | the **new-text-below arrow** |
//!
//! The `0x72` on the log is its vertical-scrollbar id, and it names the bar
//! `0x10000012` — which is a **sibling**, not a child, so nothing the bar broadcasts could reach
//! the log without that binding. See [`dereth_ui::scrollable`].
//!
//! # The entry is editable as shipped
//!
//! Measuring `0x10000011` suggests the chat entry carries `Selectable` (`0x27`) and **not**
//! `Editable` (`0x16`). It does not: that element is the *log*, which is correctly selectable-and-not-editable because a player must be able to sweep a
//! selection over the backlog and must not be able to type into it. The entry `0x10000016` ships
//! with `0x16 = Bool(true)`. Nothing in the client writes the bit and nothing bypasses it, which
//! is why the only caller of the editability setter outside the set-attribute hook's
//! own case `0x16` is the book panel (and the examination panel, by the same property-setting
//! route) — a book page is the one text element in the client whose editability *changes*.

use dereth_ui::{ElemHandle, ElementId, UiSystem};

use super::colors;
use super::interface::{ChatInterface, ChatMessage, Routed};
use crate::view::UiRequest;

/// The chat log — a 368 × 73 text element, vertical scrollbar `0x10000012` via attribute
/// `0x72`. This is where a routed line is appended.
pub const LOG: ElementId = ElementId(0x1000_0011);
/// The input line — a 306 × 17 text element, **editable**, one-line, 255 characters, between
/// the "Chat" menu `0x10000014` and the "Send" button `0x10000019`.
pub const ENTRY: ElementId = ElementId(0x1000_0016);
/// The "new text below" arrow — a 16 × 16 button at the log's bottom centre.
pub const NEW_TEXT_BELOW: ElementId = ElementId(0x1000_048C);

/// The arrow is driven by state changes at two call sites:
///
/// ```text
///   display-final-string-info notice: set state 1
///                                      (text exists below the current view)
///   arrow action after scrolling to the end: set state 0x0D
///                                      (nothing remains below the view)
/// ```
///
/// `0x0D` is the same "disabled" state the main-chat talk-focus rows use
/// ([`super::mainchat::STATE_DISABLED`]), and `1` is its "available" partner — so the arrow is
/// *disabled*, not hidden, when there is nothing below.
pub const NEW_TEXT_BELOW_ON: dereth_ui::StateId = super::mainchat::STATE_ENABLED;
/// See [`NEW_TEXT_BELOW_ON`].
pub const NEW_TEXT_BELOW_OFF: dereth_ui::StateId = super::mainchat::STATE_DISABLED;
/// The "Send" button. the client's message-1 arm.
pub const SEND: ElementId = ElementId(0x1000_0019);
/// The log's vertical scrollbar.
pub const SCROLLBAR: ElementId = ElementId(0x1000_0012);

/// The client's separator test: a log that already holds any glyph gets a `"\n"` before the
/// next line.
///
/// A body never ends in a newline when it reaches the log, because the one scroll-submission path
/// trims its leading and trailing newlines first (see
/// [`super::interface::add_text_to_scroll_trim`]). So the separator never doubles a newline the
/// message carried, and the log's last glyph is not a newline that would open an empty line below
/// the view and defeat the at-the-end test.
fn needs_separator(t: &dereth_ui::text::TextElement) -> bool {
    !t.glyphs.glyphs.is_empty()
}

/// The three children one chat window owns, resolved once at post-init.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ChatWindow {
    /// The window root through which the chat interface's opacity write reaches the owned
    /// render object and all descendants sharing it.
    /// Without this handle the opacity has nowhere to land.
    pub root: Option<ElemHandle>,
    /// The chat log — `0x10000011`.
    pub log: Option<ElemHandle>,
    /// The chat entry — `0x10000016`.
    pub entry: Option<ElemHandle>,
    /// The new-non-visible-text indicator — `0x1000048C`.
    pub new_text_below: Option<ElemHandle>,
    /// The "Send" button — `0x10000019`.
    pub send: Option<ElemHandle>,
}

impl ChatWindow {
    /// The client's three recursive child lookups, plus handing the log its 34-entry colour
    /// table.
    #[must_use]
    pub fn post_init(ui: &mut UiSystem, root: ElemHandle) -> Self {
        let w = Self {
            root: Some(root),
            log: ui.get_child_recursive(root, LOG),
            entry: ui.get_child_recursive(root, ENTRY),
            new_text_below: ui.get_child_recursive(root, NEW_TEXT_BELOW),
            send: ui.get_child_recursive(root, SEND),
        };
        w.build_chat_color_lookup_table(ui);
        // The two states are one property each, read off the live tree: state `1` is
        // `0x3B = false` (shown) and state `0x0D` is `0x3B = true` (hidden) — so the arrow's "up"
        // and "down" *are* the hide flag, and the base record's `0x3B = false` starts it shown.
        //
        // **Nothing in the client takes it down except the click arm.** Post-init binds it and
        // sets no state, the element's description has **no state 0** to fall back to, and the
        // display-final-string-info notice only ever raises it. So the client starts it *up* on
        // the base record and relies on never being away from the end — which is why retail's
        // in-game screen shows a chat window full of text and no arrow, and why without this line
        // the arrow would show permanently.
        //
        // Putting it into `0x0D` here makes it down on a chat window scrolled to its end and up
        // when there is text below the view, driven by the at-vertical-end test rather than by a
        // constant, with the raise and the click-to-lower as the client has them. It is an
        // addition, not a transcription.
        if let Some(arrow) = w.new_text_below {
            ui.set_state(arrow, NEW_TEXT_BELOW_OFF);
        }
        w
    }

    /// The chat interface's opacity write stores the current value, then applies it to
    /// the render object shared by the window subtree.
    /// This is the UI-surface object's material-opacity write.
    ///
    /// **Not the region's alpha-blend modifier**: that is the wrong one of the client's two alphas. The region alpha is per
    /// *region* and rides on that region's own Draw blit; the material alpha is per
    /// render object, and that object belongs to the nearest ancestor configured to own one and
    /// is **shared by every descendant without one**. Measured on the shipped tree: the chat
    /// window root `0x10000601` is the owner (`should_own_object`), and its own draw command
    /// carries `image: None`, `fills: []`, `glyphs: 0` — it paints nothing. Its background
    /// `0x10000010`, the eight frame pieces `0x1000069B`…`0x100006A2`, the log `0x10000011` and
    /// the scrollbar `0x10000012` are all children and all own no object. So writing the region
    /// alpha would land the whole travel of both opacity sliders on a region with no pixels, and
    /// the window would never change.
    ///
    /// [`dereth_ui::UiSystem::set_material_opacity`] is the opacity setter's walk and the draw carries the
    /// product down the subtree, so this fades the window a player can see.
    ///
    /// Returns whether there was an element to write to.
    pub fn set_opacity(&self, ui: &mut UiSystem, iface: &mut ChatInterface, v: f32) -> bool {
        iface.current_opacity = v;
        let Some(root) = self.root else { return false };
        ui.set_material_opacity(root, v).is_some()
    }

    /// The chat interface's default-opacity write, **whole**: the clamp and the tail, and the tail
    /// is the half a player sees.
    ///
    /// Store the default opacity first. If the active opacity is smaller, raise it
    /// through the active-opacity setter. Apply the default immediately only when
    /// the mouse is outside the window and its own chat entry is not focused.
    /// No focused element, or focus elsewhere, takes that immediate-apply arm.
    ///
    /// The two guards are exactly [`Self::fade_engaged`]'s two terms — the region's mouse-over bit
    /// and the "is the text entry focused" test — so the whole tail is *"if the window is not
    /// engaged, go to the idle value now"*. That is a **snap**, not a fade: the twenty-frame ramp is
    /// what the global-message handler does when the *engagement* changes, and it is not what
    /// happens when the value itself changes under an idle window.
    ///
    /// Returns whether the value was pushed to the element.
    pub fn set_default_opacity(
        &self,
        ui: &mut UiSystem,
        iface: &mut ChatInterface,
        v: f32,
    ) -> bool {
        let raise_active = iface.active_opacity < v;
        iface.set_default_opacity(v);
        if raise_active {
            // The active-opacity setter's own body, including *its* tail.
            self.set_active_opacity(ui, iface, v);
        }
        if self.fade_engaged(ui) {
            return false;
        }
        self.set_opacity(ui, iface, iface.default_opacity)
    }

    /// The mirror image, and its tail applies when
    /// the window **is** engaged.
    ///
    /// Store the active opacity first. If the default is larger, lower it through
    /// the default-opacity setter. Apply the active opacity immediately when the
    /// window is moused over or its own chat entry is focused; otherwise return.
    pub fn set_active_opacity(&self, ui: &mut UiSystem, iface: &mut ChatInterface, v: f32) -> bool {
        let lower_default = iface.default_opacity > v;
        iface.set_active_opacity(v);
        if lower_default {
            self.set_default_opacity(ui, iface, v);
        }
        if !self.fade_engaged(ui) {
            return false;
        }
        self.set_opacity(ui, iface, iface.active_opacity)
    }

    /// The chat interface's font-settings-changed notice's **element half**.
    ///
    /// Reject either unsigned choice index above 5. Fetch the enum map with enum
    /// `0x10000001`, value 2 and database type `0x26`; format `Chat_%s_%s` from the
    /// face choice followed by the size choice. Resolve the name to an enum, then a
    /// data id, and load the font. Change existing text in the chat log to that font,
    /// then install the font id for subsequent text without another conversion.
    ///
    /// **It is the log and only the log.** The field is the chat log: this handler is entered
    /// through the notice handler, then resolves the same child that post-initialization binds as
    /// the log. The child binding settles the field identity independently:
    /// child id `0x10000011` is the log, while `0x10000016` is the entry, and
    /// font changes follow the former handle only. Accordingly,
    /// the child lookup for `0x10000011` stores the same field. The entry keeps whatever
    /// font its layout record gives it, which is why changing the chat font never changes the
    /// input line.
    ///
    /// The name lookup is the caller's; this takes the resolved `DataID` and its metrics, because
    /// a `ChatWindow` has no `AssetSource`. Returns whether the log took it.
    pub fn set_chat_font(
        &self,
        ui: &mut UiSystem,
        did: dereth_primitives::DataId,
        metrics: std::sync::Arc<dyn dereth_ui::text::FontMetrics>,
    ) -> bool {
        let Some(log) = self.log else { return false };
        let Some(t) = ui.text_element_mut(log) else {
            return false;
        };
        // The client's order, and it matters: the first re-measures the glyphs already in the
        // list (keeping each one's channel colour), the second moves the element's font id so the
        // *next* appended line is measured the same way without re-running the first.
        t.change_existing_text_to_new_font(metrics);
        t.set_font_did_without_changing_existing_text(did);
        // Both calls end by marking the root dirty; the extent the scrollbar reads has moved with
        // the glyph heights, so the log is re-measured the way retail re-measures it.
        self.update_scrollable_area(ui);
        true
    }

    /// The layout's own two opacity attributes,
    /// applied once at post-init because that is when this build merges an element's properties.
    ///
    /// `0x10000080` goes to the default opacity and `0x10000081` to the active opacity; an
    /// attribute the layout does not carry is `1.0f`, which the client uses as the value when the
    /// read finds nothing.
    ///
    /// Returns `(default, active)` as they ended up.
    pub fn read_opacity_attributes(
        &self,
        ui: &mut UiSystem,
        iface: &mut ChatInterface,
    ) -> (f32, f32) {
        use super::interface::opacity_attr;
        let Some(root) = self.root else {
            return (iface.default_opacity, iface.active_opacity);
        };
        let d = crate::bind::attr_float(ui, root, opacity_attr::DEFAULT)
            .unwrap_or(opacity_attr::FALLBACK);
        let a = crate::bind::attr_float(ui, root, opacity_attr::ACTIVE)
            .unwrap_or(opacity_attr::FALLBACK);
        // The client's order: the set-attribute hook runs once per attribute and the layout writes
        // `0x10000080` first, so the default lands first and the default setter's "raise active
        // to match" clause can fire before the active value arrives.
        //
        // Both go through the window's own setters, which is what the set-attribute hook calls:
        // each carries the tail that pushes the value to the element, gated on engagement the way
        // the client gates it.
        self.set_default_opacity(ui, iface, d);
        self.set_active_opacity(ui, iface, a);
        (iface.default_opacity, iface.active_opacity)
    }

    /// The fade's *engaged* test — the client's first two lines:
    /// `(flags >> 4) & 1` (the region's mouse-over bit) **or**
    /// the chat interface's text-entry-focused test.
    ///
    /// The mouse-over half is read off [`dereth_ui::UiSystem::mouse_over`] and accepts any
    /// descendant, because the client's bit is on the window's own region and a region covers
    /// its children.
    #[must_use]
    pub fn fade_engaged(&self, ui: &UiSystem) -> bool {
        if self.is_text_entry_focused(ui) {
            return true;
        }
        let (Some(root), Some(over)) = (self.root, ui.mouse_over()) else {
            return false;
        };
        over == root || ui.is_ancestor_of(root, over)
    }

    /// Fill all `0x22` entries with
    /// [`colors::GREEN`] and then override the fourteen groups.
    ///
    /// The table lives on the **log element**, which is what the colour argument of a font-string
    /// append indexes into. `colors::build_chat_color_lookup_table` builds the table and is
    /// asserted slot by slot; this is the one line that carries it onto the element a player
    /// actually reads.
    pub fn build_chat_color_lookup_table(&self, ui: &mut UiSystem) {
        let table = colors::build_chat_color_lookup_table();
        let Some(log) = self.log else { return };
        let Some(t) = ui.text_element_mut(log) else {
            return;
        };
        t.set_default_chat_color(argb(colors::GREEN.hex), colors::CHAT_COLOR_COUNT);
        for (i, c) in table.iter().enumerate() {
            let Ok(i) = u8::try_from(i) else { continue };
            t.set_chat_color(i, argb(c.hex));
        }
    }

    ///  on the log — read **before** an append, which
    /// is what decides between following the conversation and raising the arrow.
    #[must_use]
    pub fn is_at_vertical_end(&self, ui: &mut UiSystem) -> bool {
        let Some(log) = self.log else { return true };
        let screen = ui.screen_box(log);
        ui.text_element_mut(log)
            .is_none_or(|t| t.is_at_vertical_end(screen))
    }

    /// Put glyph `pos` in view.
    pub fn scroll_to_position(&self, ui: &mut UiSystem, pos: usize) {
        let Some(log) = self.log else { return };
        let screen = ui.screen_box(log);
        let (want, mut scroll) = {
            let Some(t) = ui.text_element_mut(log) else {
                return;
            };
            (t.scroll_offset_for_position(screen, pos), t.scroll)
        };
        let Some((x, y)) = want else { return };
        let moved = scroll.set_scrollable_xy(ui, log, x, y, false);
        if let Some(t) = ui.text_element_mut(log) {
            t.scroll = scroll;
            if moved {
                // The adjust to scrollable xy change.
                t.bits.set_dirty(true);
            }
        }
    }

    /// **A window that was following the conversation is
    /// still following it after it changes size.**
    ///
    /// It notes whether the log is at its vertical end (false with no log), resizes, and if it was
    /// at the end scrolls the log to its last glyph. The element it tests is `0x10000011`, the
    /// log.
    ///
    /// **Why it matters.** Window resizing goes through this resize override, so growing the
    /// chat window is precisely the path this guards: the log's view height
    /// goes from 73 px to 373 px in one call and the offset that was "the end" a moment ago is a
    /// long way past it. Without this the newest line leaves the pane the instant the window is
    /// maximised. The same guard is what a placement restore from `PlayerModule` wants; that call
    /// site is `GamePlayScreen::update_from_player_module`'s.
    pub fn resize_to(&self, ui: &mut UiSystem, window: ElemHandle, w: i32, h: i32) {
        let was_at_end = self.is_at_vertical_end(ui);
        ui.resize_to(window, w, h);
        if was_at_end {
            self.scroll_to_end(ui);
        }
    }

    /// The end of the log — scroll to the position one past its last glyph.
    pub fn scroll_to_end(&self, ui: &mut UiSystem) {
        let n = self
            .log
            .and_then(|h| ui.text_element_mut(h))
            .map_or(0, |t| t.glyphs.len());
        self.scroll_to_position(ui, n);
    }

    /// Recompute the scrollable area for the log, against the extent its
    /// own wrapped glyphs now occupy. This is what gives the scrollbar a travel and therefore a
    /// thumb that is smaller than its track.
    pub fn update_scrollable_area(&self, ui: &mut UiSystem) {
        let Some(log) = self.log else { return };
        let screen = ui.screen_box(log);
        let ((w, h), mut scroll) = {
            let Some(t) = ui.text_element_mut(log) else {
                return;
            };
            (t.scrollable_extent(screen), t.scroll)
        };
        scroll.resize_scrollable_area(ui, log, w, h);
        if let Some(t) = ui.text_element_mut(log) {
            t.scroll = scroll;
        }
    }

    /// The chat interface's display final string info notice, whole:
    ///
    /// 1. Route the message (see [`ChatInterface::route`]); a rejected one stops here.
    /// 2. If the log is not empty, append `"\n"` in the message's type.
    /// 3. Note whether the log is at its vertical end.
    /// 4. If the prefix is valid, append it in type `0xC`.
    /// 5. Append the body in the message's type.
    /// 6. If the log holds more than 10000 glyphs, truncate it to `0x1D4C`.
    /// 7. If the log was not at the end, show the new-text-below indicator and stop; otherwise
    ///    scroll the log to its end.
    ///
    /// Note the order the client actually has: the **separator goes in before** the at-end test
    /// is read, and the prefix is `0xC` — grey — whatever the message's own type is.
    pub fn recv_display_final_string_info(
        &self,
        ui: &mut UiSystem,
        iface: &mut ChatInterface,
        m: &ChatMessage,
    ) -> Routed {
        if iface.route(m) != Routed::Accepted {
            return iface.route(m);
        }
        let Some(log) = self.log else {
            return iface.recv_display_final_string_info(m, true);
        };
        let screen = ui.screen_box(log);
        if let Some(t) = ui.text_element_mut(log) {
            if needs_separator(t) {
                t.append_text_with_font("\n", m.ty);
            }
        }
        let was_at_end = ui
            .text_element_mut(log)
            .is_none_or(|t| t.is_at_vertical_end(screen));
        if let Some(t) = ui.text_element_mut(log) {
            if let Some(p) = &m.prefix {
                t.append_text_with_font(p, colors::PREFIX_COLOR_INDEX);
            }
            // The sole scroll-submission path
            // trims the line's leading and
            // trailing newlines before it builds the body `StringInfo`, so no body
            // that reaches this function ends in LF. Without it the log's last glyph is a newline
            // glyph and opens the empty line at the glyph
            // count, leaving a blank bottom row.
            // See [`super::interface::add_text_to_scroll_trim`].
            t.append_text_with_font(super::interface::add_text_to_scroll_trim(&m.body), m.ty);
        }
        // The model keeps the same runs, so the routing, the history and the truncation
        // arithmetic stay in one place and the element is only ever the picture of it.
        let routed = iface.recv_display_final_string_info(m, was_at_end);
        let over = ui
            .text_element_mut(log)
            .is_some_and(|t| t.glyphs.len() > super::interface::scrollback::MAX_GLYPHS);
        if over {
            let cut = iface.last_truncation_cut();
            if let Some(t) = ui.text_element_mut(log) {
                t.glyphs.behead(cut);
                t.bits.set_dirty(true);
            }
        }
        self.update_scrollable_area(ui);
        if was_at_end {
            self.scroll_to_end(ui);
        } else if let Some(arrow) = self.new_text_below {
            ui.set_state(arrow, NEW_TEXT_BELOW_ON);
        }
        routed
    }

    /// The entry's text goes to the communication
    /// system, then into the history, then the box is cleared.
    ///
    /// The pre-parsed text read is the **tagged** form, which is what a `/tell` built from a
    /// clicked name needs; `dereth_ui::text::glyph::inq_text` with `with_tags` is the
    /// same function.
    pub fn process_command(
        &self,
        ui: &mut UiSystem,
        iface: &mut ChatInterface,
    ) -> Option<UiRequest> {
        let entry = self.entry?;
        let text = ui
            .text_element_mut(entry)
            .map(|t| t.glyphs.inq_text(true))?;
        iface.entry = text;
        let req = iface.process_command();
        // Unconditional, even for the empty line the
        // step above declines to send.
        if let Some(t) = ui.text_element_mut(entry) {
            t.set_text("");
        }
        req
    }

    /// The client's **message 1** arm, which is the whole
    /// of what a click on this window's own buttons does:
    ///
    /// a click on "Send" (`0x10000019`) processes the command line; a click on the arrow
    /// (`0x1000048C`) scrolls the log to its end and turns the arrow off.
    ///
    /// Returns the request the Send button raised, if any.
    pub fn listen_to_element_message(
        &self,
        ui: &mut UiSystem,
        iface: &mut ChatInterface,
        source: ElementId,
        id: dereth_ui::MessageId,
    ) -> Option<UiRequest> {
        if id != dereth_ui::msg::element::id::BUTTON_CLICKED {
            return None;
        }
        if source == SEND {
            return self.process_command(ui, iface);
        }
        if source == NEW_TEXT_BELOW {
            self.scroll_to_end(ui);
            if let Some(arrow) = self.new_text_below {
                ui.set_state(arrow, NEW_TEXT_BELOW_OFF);
            }
            iface.new_non_visible_text = false;
        }
        None
    }

    /// The client's **message `0x12`** arm — the one
    /// element message this window takes that is not a click.
    ///
    /// When the message comes from the chat entry and its character is a space (`0x20`), the
    /// window runs its text replacements.
    ///
    /// `dereth_ui::text::TextElement` already broadcasts `msgid::CHARACTER` (`0x12`) with the
    /// character in `p1` for every key it inserts; this is its listener.
    ///
    /// The guard is the *space*, not the alias: the expansion happens when the space after `r` is
    /// typed, which is why the composed `@tell <name>,` carries no trailing space of its own. See
    /// [`super::interface::handle_text_replacements`] for the arithmetic.
    ///
    /// Returns what was expanded, for a caller that wants to say so; `None` is the ordinary case.
    pub fn on_entry_character(
        &self,
        ui: &mut UiSystem,
        iface: &mut ChatInterface,
        source: ElemHandle,
        ch: char,
        _targets: &ReplyTargets,
    ) -> bool {
        let Some(entry) = self.entry.filter(|entry| *entry == source) else {
            return false;
        };
        let Some(text) = ui.text_element_mut(entry).map(|t| t.glyphs.inq_text(true)) else {
            return false;
        };
        iface.entry.clone_from(&text);
        ui.requests.emit(UiRequest::ChatEntry {
            window: iface.window_id,
            text,
            action: if ch == ' ' {
                dereth_client_contract::chat::entry::EntryAction::ExpandAlias
            } else {
                dereth_client_contract::chat::entry::EntryAction::Draft
            },
        });
        true
    }
}

// ---------------------------------------------------------------------------------------------
// The keyboard half of `ChatInterface`
// ---------------------------------------------------------------------------------------------
//
// Everything above this line is reached from a **click**. Everything below it is reached from a
// key, through the element base's action handler and its bubble to the parent's
// child-action handler. Without that bubble the chat interface's child-action handler's eight
// arms and its own action handler's six are unreachable and **Enter does nothing**.
// [`dereth_ui::UiSystem::dispatch_child_action`] is the bubble.
//
// # Reading the two functions through their child bindings
//
// The child ids settle which field is which: each id has one shipped role and one binding
// destination.
//
// | child lookup | role |
// |---|---|
// | `0x10000011` | log |
// | `0x1000048C` | new-text-below arrow |
// | `0x10000016` | entry |
//
// The main window's filter default is `0xFBFFFFFF`; its behavior confirms that the text-type
// filter is a separate field from the arrow. The enter-key helper acts on the actual entry.
//
// # The order, and why it matters
//
// The text element's action handler runs the base handler first and only switches on the action
// itself when the base declined it and the action is a key-down. The base — the bubble — runs
// **first**. So `0x25` in the entry reaches the child-action handler's enter-key arm and never
// reaches the text element's own
// `0x25`, which would merely relinquish focus; `0x1E`/`0x1F` browse the history instead of moving
// the caret; and `0x1C`/`0x1D`/`0x20`/`0x21` scroll **the log** instead of the entry's caret.

/// The three names the reply aliases can address.
///
/// Defined in [`dereth_client_contract::chat::window`], because `dereth_client_shell::hud` is what
/// fills it.
pub use dereth_client_contract::chat::window::ReplyTargets;

/// `StringInfo(ID_AssistedTell, table 6)` with `PREFIX` = `ID_CmdPrefix`, rendered.
///
/// The two string ids are the literal arguments of the two
/// string-table lookups at the start of reply-key handling. The **text** those
/// ids resolve to is not compiled into the client; it is a row of string table enum 6 in the dat,
/// so this constant is the fallback a host uses when it has no string table, and
/// `GamePlayScreen::reply_template` is where the resolved row goes when it has one.
///
/// `ID_CmdPrefix` is the client's own command prefix. Command normalization rewrites a
/// leading `/` to `@` before it does anything else, so the prefix a composed command must carry is
/// `@`.
pub const ASSISTED_TELL_FALLBACK: &str = "@tell ";

/// The client's stored format string is `L"@tell %s, "`:
/// the `%s` is the name and the `, ` is part of the
/// literal, so the composed line is `@tell <name>, `. Not a string-table lookup: unlike
/// the reply key's `ID_AssistedTell`, this one is a literal in the client.
pub const START_TELL_PREFIX: &str = "@tell ";

/// What one child-action / action arm did.
#[derive(Debug, Default, PartialEq)]
pub struct ChatAction {
    /// Whether the arm consumed the event, which is what stops the focused text element from
    /// also acting on it.
    pub consumed: bool,
    /// The request the arm raised, if any — the command processor's line, in practice.
    pub request: Option<UiRequest>,
}

impl ChatAction {
    /// Consumed, nothing raised.
    const fn took() -> Self {
        Self {
            consumed: true,
            request: None,
        }
    }
}

impl ChatWindow {
    /// The chat interface's is-text-entry-focused test: the UI's active element exists, is this
    /// window's root, and its focused descendant is the chat entry.
    ///
    /// **The "active element is the root" half is dropped here and the focus half is kept.** In the
    /// client the test picks *which* chat window answers when five of them hear the same action;
    /// the focus element is already global and already single, so
    /// `focus_element() == entry` answers the same question with the same precision. The
    /// dropped half would additionally require the window to have been activated, and nothing in
    /// this build registers a chat window as activatable — keeping it would make every arm below
    /// unreachable for a second time.
    #[must_use]
    pub fn is_text_entry_focused(&self, ui: &UiSystem) -> bool {
        self.entry.is_some() && ui.focus_element() == self.entry
    }

    /// The chat interface's chat-entry activation — activate the entry, take focus, and raise the
    /// toggle-chat-entry notice with true.
    ///
    /// The client's activation returns immediately for an element without the activatable
    /// flag, and the entry does not carry it, so the observable half is taking focus.
    pub fn activate_chat_entry(&self, ui: &mut UiSystem, iface: &mut ChatInterface) {
        let Some(entry) = self.entry else { return };
        ui.take_focus(entry);
        iface.chat_entry_active = true;
    }

    /// The chat interface's chat-entry deactivation — the entry relinquishes focus and is
    /// deactivated, and the toggle-chat-entry notice is raised with false.
    pub fn deactivate_chat_entry(&self, ui: &mut UiSystem, iface: &mut ChatInterface) {
        let Some(entry) = self.entry else { return };
        ui.relinquish_focus(entry);
        iface.chat_entry_active = false;
    }

    /// The same two halves, chosen by whether the
    /// entry already has focus. Always returns true (it is [`Self::on_action`]'s `0x10000024` arm).
    pub fn on_toggle_chat_entry(&self, ui: &mut UiSystem, iface: &mut ChatInterface) -> bool {
        if self.is_text_entry_focused(ui) {
            self.deactivate_chat_entry(ui, iface);
        } else {
            self.activate_chat_entry(ui, iface);
        }
        true
    }

    /// The main chat panel's start-tell notice, which does nothing but hand over to the chat
    /// interface's own start-tell. What it does:
    ///
    /// Format the UTF-16 literal `@tell %s, ` with the name; activate the entry,
    /// take focus, then send the toggle-chat-entry notice with true.
    /// Set the formatted text, move the cursor to its end, and clear the selection, in that order.
    ///
    /// The first three calls are [`Self::activate_chat_entry`]
    /// inlined. Setting the text **replaces** — a half-typed line is gone, not prefixed — and the
    /// caret lands after the comma-space so the player types the message straight away. Only the
    /// main chat panel (and its floating-main-chat subclass, through the same method) handles
    /// this notice, so the four floaty windows never receive this and the screen calls it on the
    /// **main** window only.
    ///
    /// The three raisers in retail: the Friends tab's Send Tell button
    /// (the friends panel's element-message handler, `0x10000516`), the `0x10000119` action
    /// in [`Self::on_action`] and a click on a `<Tell:IIDString>` run in the log
    /// (the main chat panel's text tag iid string click notice, tag type `0x10000001`,
    /// guarded by the entry not being focused).
    pub fn on_start_tell(&self, ui: &mut UiSystem, iface: &mut ChatInterface, name: &str) {
        if self.entry.is_none() {
            return;
        }
        ui.requests.emit(UiRequest::ChatEntry {
            window: iface.window_id,
            text: iface.entry.clone(),
            action: dereth_client_contract::chat::entry::EntryAction::StartTell {
                name: name.to_owned(),
            },
        });
    }

    /// **The Enter key**: if the text entry is not focused, nothing happens. Unless the player's
    /// stay-in-chat-mode option is set, the entry relinquishes focus, is deactivated and the
    /// toggle-chat-entry notice is raised with false. Then the command line is processed.
    ///
    /// Note the order: the box loses focus **before** the line is taken out of it, and
    /// [`Self::process_command`] runs either way. `stay_in_chat_mode` is that player option.
    pub fn handle_enter_key(
        &self,
        ui: &mut UiSystem,
        iface: &mut ChatInterface,
        stay_in_chat_mode: bool,
    ) -> Option<UiRequest> {
        if !self.is_text_entry_focused(ui) {
            return None;
        }
        if !stay_in_chat_mode {
            self.deactivate_chat_entry(ui, iface);
        }
        self.process_command(ui, iface)
    }

    /// Page the **log** up or down: move its scroll y by the vertical page delta in that
    /// direction.
    pub fn scroll_page(&self, ui: &mut UiSystem, up: bool) {
        let Some(log) = self.log else { return };
        let screen = ui.screen_box(log);
        let (delta, mut scroll) = {
            let Some(t) = ui.text_element_mut(log) else {
                return;
            };
            (t.inq_scroll_delta(screen, false, up, true), t.scroll)
        };
        let (x, y) = (scroll.x, scroll.y);
        let moved = scroll.set_scrollable_xy(ui, log, x, y + delta, false);
        if let Some(t) = ui.text_element_mut(log) {
            t.scroll = scroll;
            if moved {
                t.bits.set_dirty(true);
            }
        }
    }

    /// [`ChatInterface::select_command_from_history`] with its element half: the model picks
    /// the entry, setting the entry's text writes it and scrolling the entry to its last glyph
    /// puts the caret at the end.
    pub fn select_command_from_history(
        &self,
        ui: &mut UiSystem,
        iface: &mut ChatInterface,
        back: bool,
    ) {
        let Some(entry) = self.entry else { return };
        let text = ui
            .text_element_mut(entry)
            .map(|t| t.glyphs.inq_text(true))
            .unwrap_or_default();
        ui.requests.emit(UiRequest::ChatEntry {
            window: iface.window_id,
            text,
            action: if back {
                dereth_client_contract::chat::entry::EntryAction::Previous
            } else {
                dereth_client_contract::chat::entry::EntryAction::Next
            },
        });
    }

    /// Handle a reply-key action by composing `ID_AssistedTell` for one of
    /// the three remembered names and put it in the entry.
    ///
    /// The client builds `ID_AssistedTell` (table enum 6) with `PREFIX` set to `ID_CmdPrefix`,
    /// picks the name by action, returns if it is empty, sets it as `TARGET`, and — when the
    /// string is valid and the entry exists — activates the entry, sets the string, deselects and
    /// moves the cursor to the end.
    ///
    /// **The three actions are not in the order the names suggest.** `0x10000020` is the
    /// *monarch*, `0x10000021` the *patron* and `0x10000022` the last teller — read off what the
    /// client does per action, not off the constant names.
    pub fn handle_reply_key(
        &self,
        ui: &mut UiSystem,
        iface: &mut ChatInterface,
        action: u32,
        targets: &ReplyTargets,
        template: &str,
    ) -> bool {
        let _ = targets;
        let Some(entry) = self.entry else {
            return true;
        };
        let Some(target) = dereth_client_contract::chat::entry::ReplyTarget::from_action(
            dereth_client_contract::actions::ActionId(action),
        ) else {
            return false;
        };
        let text = ui
            .text_element_mut(entry)
            .map_or_else(String::new, |t| t.glyphs.inq_text(true));
        ui.requests.emit(UiRequest::ChatEntry {
            window: iface.window_id,
            text,
            action: dereth_client_contract::chat::entry::EntryAction::Reply {
                target,
                prefix: template.to_owned(),
            },
        });
        true
    }

    /// The eight arms, behind the two guards
    /// "the action is a key-down" and "the child that raised it is **this window's entry**".
    ///
    /// ```text
    /// 0x1C Home      scroll the log to position 0
    /// 0x1D End       scroll the log to its end
    /// 0x1E Up        select_command_from_history(back)
    /// 0x1F Down      select_command_from_history(forward)
    /// 0x20 PageUp    scroll_page(up) on the log
    /// 0x21 PageDown  scroll_page(down) on the log
    /// 0x25 Enter     handle_enter_key
    /// 0x27 Escape    deactivate_chat_entry
    /// ```
    ///
    /// All eight consume the action. Anything else falls through to the base child-action
    /// handler, which is the walk further up the chain.
    pub fn on_child_action(
        &self,
        ui: &mut UiSystem,
        iface: &mut ChatInterface,
        child: ElemHandle,
        e: &dereth_ui::focus::InputEvent,
        stay_in_chat_mode: bool,
    ) -> ChatAction {
        use dereth_ui::focus::action as act;
        if !e.start || self.entry != Some(child) {
            return ChatAction::default();
        }
        match e.action {
            act::CURSOR_HOME => self.scroll_to_position(ui, 0),
            act::CURSOR_END => self.scroll_to_end(ui),
            act::CURSOR_UP => self.select_command_from_history(ui, iface, true),
            act::CURSOR_DOWN => self.select_command_from_history(ui, iface, false),
            act::CURSOR_PAGE_UP => self.scroll_page(ui, true),
            act::CURSOR_PAGE_DOWN => self.scroll_page(ui, false),
            act::ACCEPT => {
                let r = self.handle_enter_key(ui, iface, stay_in_chat_mode);
                return ChatAction {
                    consumed: true,
                    request: r,
                };
            }
            act::ESCAPE => self.deactivate_chat_entry(ui, iface),
            _ => return ChatAction::default(),
        }
        ChatAction::took()
    }

    /// The six arms that are **not** keyed on the focused
    /// element, because they are the ones that give it focus.
    ///
    /// ```text
    /// 0x10000020/21/22  handle_reply_key                                  consumed
    /// 0x10000023        activate_chat_entry, then select all in the entry consumed
    /// 0x10000024        on_toggle_chat_entry                              its answer
    /// 0x10000028        the command-or-alias key                          consumed
    /// 0x10000119        if the selected id is in (0x50000000, 0x70000000),
    ///                   raise start-tell with its object name              consumed
    /// anything else     the base action handler (the bubble)
    /// ```
    ///
    /// The command-or-alias action (`0x10000028`) builds its `StringInfo` with table enum 6 and
    /// **no string id at all**, so the string is never valid for any key in
    /// this build and the arm consumes the action and writes nothing. It is written as that rather
    /// than guessed at.
    ///
    /// `selection` is the selected id paired with its object name. The client's `0x10000119` arm
    /// raises the start-tell notice with that name, whose receiver is the main chat panel's start-tell
    /// notice — [`Self::on_start_tell`], reached through
    /// [`UiRequest::StartTell`]. The host supplies that pair from its existing object-name table;
    /// keeping it together here prevents a selected id from being combined with a later name.
    pub fn on_action(
        &self,
        ui: &mut UiSystem,
        iface: &mut ChatInterface,
        e: &dereth_ui::focus::InputEvent,
        targets: &ReplyTargets,
        template: &str,
        selection: Option<(u32, &str)>,
    ) -> ChatAction {
        use super::mainchat::action as a;
        match e.action {
            a::REPLY | a::REPLY_MONARCH | a::REPLY_PATRON => {
                self.handle_reply_key(ui, iface, e.action, targets, template);
                ChatAction::took()
            }
            a::ACTIVATE_ENTRY => {
                self.activate_chat_entry(ui, iface);
                if let Some(t) = self.entry.and_then(|h| ui.text_element_mut(h)) {
                    t.select_all();
                }
                ChatAction::took()
            }
            a::TOGGLE_ENTRY => {
                self.on_toggle_chat_entry(ui, iface);
                ChatAction::took()
            }
            x if x == a::COMMAND_OR_ALIAS[0] => ChatAction::took(),
            x if x == a::COMMAND_OR_ALIAS[1] => {
                if e.start {
                    if let Some((_, name)) =
                        selection.filter(|(id, _)| super::mainchat::selection_is_a_player(*id))
                    {
                        return ChatAction {
                            consumed: true,
                            request: Some(UiRequest::StartTell {
                                name: name.to_owned(),
                            }),
                        };
                    }
                }
                ChatAction::took()
            }
            _ => ChatAction::default(),
        }
    }
}

/// The 32-bit colour — the `0xRRGGBB` the colour table tabulates, opaque.
fn argb(hex: u32) -> u32 {
    0xFF00_0000 | hex
}
