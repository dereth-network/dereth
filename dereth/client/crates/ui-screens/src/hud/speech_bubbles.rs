//! `MessageLogPanel` — the short-lived speech and emote bubbles over heads.
//!
//! The over-head bubble system has exactly one knob (the maximum concurrent items, default 1) and
//! one behaviour (identical consecutive text replaces rather than stacks). **Reproduce both.**
//! Every line it draws is yellow.

use dereth_ui::framework::LayoutEnum;
use dereth_ui::ElementId;

/// The font colour each new line is given: RGBA `(1, 1, 0, 1)`, yellow, in the `0xAARRGGBB`
/// packing a `Color` property decodes into. It replaces the layout's own text colour.
pub const LINE_COLOR: u32 = 0xFFFF_FF00;

/// Give one new line its yellow font colour.
pub fn color_line(ui: &mut dereth_ui::UiSystem, h: dereth_ui::ElemHandle) {
    crate::screens::charmgmt::set_font_color(ui, h, LINE_COLOR);
}

/// The only chat type the panel accepts: **`0x1A` (26)**.
///
/// ACE marks chat type `0x1A` as "does nothing"; in the client it is exactly the *float-over-head*
/// channel, and `ChatInterface`'s default main-window filter masks it out.
pub const BUBBLE_CHAT_TYPE: u8 = 0x1A;

/// The attribute the maximum concurrent item count is read from.
pub const ATTR_MAX_CONCURRENT: u32 = 0x1000_0028;
/// The default when that attribute is absent.
pub const DEFAULT_MAX_CONCURRENT: usize = 1;

/// The list box that holds the bubbles.
pub const LIST_BOX: ElementId = ElementId(0x1000_0049);
/// The authored `MessageLogPanel` panel instance in `classic_gameplay`.
pub const PANEL: ElementId = ElementId(0x1000_0046);
/// Each bubble is created from **layout enum `0x10000012`, element id `0x1000004A`**
/// (created by layout enum).
pub const BUBBLE_LAYOUT: LayoutEnum = LayoutEnum(0x1000_0012);
/// See [`BUBBLE_LAYOUT`].
pub const BUBBLE_ELEMENT: ElementId = ElementId(0x1000_004A);
/// The element message a bubble sends to remove itself; its lifetime is driven by its own media
/// script (a timed fade in the layout), not by C++.
pub const MSG_BUBBLE_EXPIRED: dereth_ui::MessageId = dereth_ui::MessageId(0x1000_0003);

/// `MessageLogPanel`'s list, newest at index 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechBubbles {
    /// The maximum number of concurrent bubbles.
    pub max_concurrent: usize,
    /// The list box's items, index 0 newest.
    pub items: Vec<String>,
    /// The pending queue — accepted messages waiting for the next update.
    pub pending: Vec<String>,
}

impl Default for SpeechBubbles {
    fn default() -> Self {
        Self {
            max_concurrent: DEFAULT_MAX_CONCURRENT,
            items: Vec::new(),
            pending: Vec::new(),
        }
    }
}

impl SpeechBubbles {
    /// "Accepts **only chat type `0x1A` (26)**
    /// and appends the `StringInfo` to the pending queue".
    pub fn recv_display_final_string_info(&mut self, ty: u8, text: &str) -> bool {
        if ty != BUBBLE_CHAT_TYPE {
            return false;
        }
        self.pending.push(text.to_owned());
        true
    }

    /// The spew box's update, driven by global message 3 — drain the pending array.
    ///
    /// "For each entry it creates a child element …, sets the trimmed string, …, **deletes item 0
    /// first if the new text is identical to it** (duplicate suppression), inserts the new item at
    /// index 0, and drops the last item when the count exceeds the maximum. Finally it
    /// scrolls to item 0."
    pub fn update(&mut self) {
        for text in std::mem::take(&mut self.pending) {
            let text = text.trim().to_owned();
            if self.items.first().is_some_and(|t| *t == text) {
                self.items.remove(0);
            }
            self.items.insert(0, text);
            while self.items.len() > self.max_concurrent {
                self.items.pop();
            }
        }
    }
}

/// `MessageLogPanel` bound to a live gameplay tree — the model above plus the elements it draws.
///
/// [`SpeechBubbles`] is the model; this is the half that puts it on screen. Without it `0x1A` —
/// the only chat type it accepts, and the only type the main chat window's default filter drops —
/// has no destination at all, and every refusal, every *"Moving %s to your backpack"*, every
/// *"You can't put that item there"* is computed, raised and dropped.
///
/// The list box `0x10000049` is in the shipped gameplay layout at (175, 20)–(624, 91): the strip
/// across the top of the viewport, which is where retail draws these.
#[derive(Debug, Default)]
pub struct SpeechBubblePanel {
    /// `MessageLogPanel`'s own state.
    pub model: SpeechBubbles,
    /// The list box — the recursive child search for `0x10000049`.
    pub list: Option<crate::panels::listbox::ListBoxWidget>,
    /// How many bubble elements this panel has created since it was bound. Counted for the reason
    /// [`crate::panels::listbox::ListBoxWidget::created`] is: a panel that silently creates nothing
    /// is the defect, and a zero here with a non-zero `model.items` is exactly the shape of it.
    pub drawn: u64,
    /// How many bubbles [`Self::on_element_message`] has deleted since the panel was bound.
    ///
    /// Counted for the same reason [`Self::drawn`] is, and for a sharper one: this counter staying
    /// at zero while `drawn` climbs is the visible defect — a notice that goes up and never comes
    /// down — and a build in which the message never arrives looks exactly like a build in which
    /// the handler declines it.
    pub expired: u64,
}

impl SpeechBubblePanel {
    /// Bind the list box and read the maximum concurrent item count
    /// from attribute `0x10000028`, defaulting to 1 when the layout carries no such attribute.
    pub fn post_init(&mut self, ui: &mut dereth_ui::UiSystem, root: dereth_ui::ElemHandle) {
        let Some(panel) = ui.get_child_recursive(root, PANEL) else {
            return;
        };
        // Retail explicitly clears mouse visibility on both the panel and its list. The latter
        // matters: the list box's own mouse-visibility rule normally answers true, but this
        // explicit setter is the last word during post-init.  Without it the empty, transparent
        // strip across the top of the viewport intercepts controls in panels behind it.
        ui.set_mouse_visible(panel, false);
        let Some(h) = ui.get_child_recursive(panel, LIST_BOX) else {
            return;
        };
        if let Some(n) = crate::bind::attr_int(ui, h, ATTR_MAX_CONCURRENT) {
            self.model.max_concurrent = usize::try_from(n).unwrap_or(DEFAULT_MAX_CONCURRENT);
        }
        self.list = Some(crate::panels::listbox::ListBoxWidget::bind(ui, h));
        ui.set_mouse_visible(h, false);
    }

    /// `recv_display_final_string_info`, for a host that has the panel and not the model.
    pub fn recv_display_final_string_info(&mut self, ty: u8, text: &str) -> bool {
        self.model.recv_display_final_string_info(ty, text)
    }

    /// The element-message handler — **the clearer**.
    ///
    /// The decision has two gates in order: require the source element's bubble id, then require
    /// the expiration message. When both match, delete that source item from the list. The base
    /// element-message handler runs either way.
    ///
    /// **The gate is on the *source element's* id and not on the panel's**, so a bubble deletes
    /// itself and nothing else can ask this handler to delete anything.
    ///
    /// # Where the message comes from — no client code raises it
    ///
    /// [`MSG_BUBBLE_EXPIRED`]'s producer is the bubble's own element description, read out of the
    /// shipped `client_local_English.dat`: layout enum [`BUBBLE_LAYOUT`] resolves to
    /// `DataId(0x21000011)`, and element `0x1000004A`'s state `0x10000002` carries exactly two
    /// media entries —
    ///
    /// ```text
    /// MediaDesc { media_type: 8, Pause   { min_duration: 5.0, max_duration: 5.0 } }
    /// MediaDesc { media_type: 7, Message { message_id: 0x10000003, probability: 1.0 } }
    /// ```
    ///
    /// So a bubble lives **five seconds** and then broadcasts its own removal.
    /// `dereth_ui::media::MediaPlayback` runs both of those steps; this is the listener. A client
    /// test measures the five seconds against the tree.
    ///
    /// Deleting an item scans for its pointer, then removes it. Removal does not lay
    /// the list out itself: it sets bit `0x200` in `flags` and dirties the root,
    /// which *marks* the list for
    /// the next layout update. This build has no dirty pass, so the re-layout is done here — the
    /// same call [`Self::update`] makes after its own deletes, and for the same reason.
    ///
    /// Returns whether the message was one of ours, which is what a panel fan-out reads.
    pub fn on_element_message(
        &mut self,
        ui: &mut dereth_ui::UiSystem,
        m: &dereth_ui::ElementMessage,
    ) -> bool {
        if m.source_id != BUBBLE_ELEMENT || m.id != MSG_BUBBLE_EXPIRED {
            return false;
        }
        let Some(list) = self.list.as_mut() else {
            return false;
        };
        // The list's delete-item scan, and its own answer when the element is not in the list:
        // the client returns false and deletes nothing (the not-found index -1 fails the
        // remove's bounds test).
        let Some(i) = list.index_of(m.source) else {
            return false;
        };
        list.delete_item(ui, i);
        // The model's `items` and the list's are written by the same decisions in the same order,
        // exactly as `update` keeps them.
        if i < self.model.items.len() {
            self.model.items.remove(i);
        }
        list.update_layout(ui);
        self.expired += 1;
        true
    }

    /// The spew box's update, driven by global message 3.
    ///
    /// The client's loop, per pending string: create a child from layout enum `0x10000012`, element
    /// `0x1000004A`, set its text to the trimmed string, size it to the list box's width, recompute
    /// its glyphs, **delete item 0 first if its text is identical**, insert the new item at index 0,
    /// then delete the last item while the count exceeds the maximum, and finally scroll to item 0.
    ///
    /// \[update\] is that loop over the strings alone and stays the pure oracle; this runs
    /// it against the elements and keeps the two in step by construction — the model's `items` and
    /// the list's `items` are written by the same three decisions in the same order.
    ///
    /// Returns how many bubbles were created this frame, so a caller can count instead of assume.
    pub fn update(&mut self, ui: &mut dereth_ui::UiSystem) -> u32 {
        let Some(list) = self.list.as_mut() else {
            // No element to draw into: the strings still drain, exactly as the client's
            // no-list-box guard leaves them queued. Keeping them would grow an
            // unbounded queue in a headless run.
            self.model.pending.clear();
            return 0;
        };
        if self.model.pending.is_empty() {
            return 0;
        }
        let width = ui.node(list.handle).map_or(0, |n| n.region.box_.width());
        let mut created = 0u32;
        for text in std::mem::take(&mut self.model.pending) {
            let text = text.trim().to_owned();
            let Some(h) = list.add_from_layout_enum(ui, BUBBLE_LAYOUT, BUBBLE_ELEMENT, Some(0))
            else {
                continue;
            };
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&text);
            }
            // The text goes in first, then the colour.
            color_line(ui, h);
            if width > 0 {
                let height = ui.node(h).map_or(0, |n| n.region.box_.height());
                ui.resize_to(h, width, height);
            }
            // The duplicate suppression, which the client applies **after** the new element exists
            // and against the item that was at index 0 — now index 1, because the insert above has
            // already happened here.
            if self.model.items.first().is_some_and(|t| *t == text) {
                self.model.items.remove(0);
                list.delete_item(ui, 1);
            }
            self.model.items.insert(0, text);
            created += 1;
            while self.model.items.len() > self.model.max_concurrent {
                self.model.items.pop();
                let last = list.items.len().saturating_sub(1);
                list.delete_item(ui, last);
            }
        }
        list.update_layout(ui);
        self.drawn += u64::from(created);
        created
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered HUD behavior — the one accepted chat type, the layout enum and element
    /// id the bubbles are built from, and the self-removal message.
    #[test]
    fn the_bubble_channel_and_its_layout_are_the_documented_ones() {
        assert_eq!(BUBBLE_CHAT_TYPE, 26);
        assert_eq!(BUBBLE_LAYOUT, LayoutEnum(0x1000_0012));
        assert_eq!(BUBBLE_ELEMENT, ElementId(0x1000_004A));
        assert_eq!(LIST_BOX, ElementId(0x1000_0049));
        assert_eq!(MSG_BUBBLE_EXPIRED, dereth_ui::MessageId(0x1000_0003));
        assert_eq!(ATTR_MAX_CONCURRENT, 0x1000_0028);
        assert_eq!(DEFAULT_MAX_CONCURRENT, 1);
        // The main chat window's default filter is exactly what excludes this type (`13` §11).
        let w = crate::chat::interface::ChatInterface::new(crate::chat::interface::window::MAIN);
        assert!(!w.type_is_active(BUBBLE_CHAT_TYPE));
    }

    /// Oracle: — "accepts only chat type `0x1A`".
    #[test]
    fn only_type_twenty_six_reaches_the_pending_list() {
        let mut s = SpeechBubbles::default();
        assert!(!s.recv_display_final_string_info(2, "speech"));
        assert!(!s.recv_display_final_string_info(12, "emote"));
        assert!(s.recv_display_final_string_info(BUBBLE_CHAT_TYPE, "over head"));
        assert_eq!(s.pending, vec!["over head".to_owned()]);
    }

    /// Oracle: — the duplicate suppression and the concurrency cap, which
    /// §12 names as the two behaviours to reproduce.
    #[test]
    fn identical_consecutive_text_replaces_rather_than_stacks() {
        let mut s = SpeechBubbles {
            max_concurrent: 3,
            ..Default::default()
        };
        s.recv_display_final_string_info(BUBBLE_CHAT_TYPE, "  hello  ");
        s.update();
        assert_eq!(s.items, vec!["hello".to_owned()], "the string is trimmed");

        s.recv_display_final_string_info(BUBBLE_CHAT_TYPE, "hello");
        s.update();
        assert_eq!(
            s.items,
            vec!["hello".to_owned()],
            "the duplicate replaced item 0"
        );

        s.recv_display_final_string_info(BUBBLE_CHAT_TYPE, "world");
        s.update();
        assert_eq!(s.items, vec!["world".to_owned(), "hello".to_owned()]);

        // A repeat that is *not* consecutive stacks normally.
        s.recv_display_final_string_info(BUBBLE_CHAT_TYPE, "hello");
        s.update();
        assert_eq!(
            s.items,
            vec!["hello".to_owned(), "world".to_owned(), "hello".to_owned()]
        );
    }

    /// Oracle: the same function — "drops the last item when the count exceeds
    /// the maximum", whose default is 1.
    #[test]
    fn the_default_of_one_keeps_exactly_one_bubble() {
        let mut s = SpeechBubbles::default();
        for t in ["a", "b", "c"] {
            s.recv_display_final_string_info(BUBBLE_CHAT_TYPE, t);
        }
        s.update();
        assert_eq!(
            s.items,
            vec!["c".to_owned()],
            "newest at index 0, the rest dropped"
        );

        let mut s = SpeechBubbles {
            max_concurrent: 2,
            ..Default::default()
        };
        for t in ["a", "b", "c"] {
            s.recv_display_final_string_info(BUBBLE_CHAT_TYPE, t);
        }
        s.update();
        assert_eq!(s.items, vec!["c".to_owned(), "b".to_owned()]);
    }

    /// A new line's font colour is set to yellow, as the one-entry colour array the text element
    /// reads its font colour from.
    #[test]
    fn a_new_line_is_coloured_yellow() {
        use dereth_assets::ui::PropertyValue;
        assert_eq!(LINE_COLOR, 0xFFFF_FF00, "RGBA (1, 1, 0, 1)");
        let mut ui = dereth_ui::UiSystem::new((800, 600));
        let h = ui.create_hollow(Some(ui.root()));
        color_line(&mut ui, h);
        let Some(PropertyValue::Array(a)) = ui.node(h).and_then(|n| {
            n.instance_properties
                .get(dereth_ui::props::attr::TEXT_FONT_COLOR)
        }) else {
            panic!("no font colour array on the line");
        };
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].value, PropertyValue::Color(LINE_COLOR));
    }
}
