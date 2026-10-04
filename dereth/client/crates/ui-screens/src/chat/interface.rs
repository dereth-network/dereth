//! `ChatInterface` — routing, filters, scrollback, opacity and the input line.
//!
//! `ChatInterface` is never registered as an element type: it is the shared base of the main
//! (`0x10000041`) and floating (`0x10000040`) chat element types, and everything in this module is
//! what the five windows share. The command interpreter is **not** here — the host receives
//! the raw string together with the window id. This module emits [`UiRequest::ChatLine`]
//! and stops.

use crate::view::UiRequest;

/// The five window ids the client's window-id switch knows.
///
/// Defined in [`dereth_client_contract::chat::interface::window`], because `dereth_client::hud`
/// names `MAIN` when it seeds the placement blob.
pub use dereth_client_contract::chat::interface::window;

pub use dereth_client_contract::chat::interface::{
    default_filter, filter_groups_for, FilterGroup, FILTER_GROUPS,
};

/// The scrollback caps.
pub mod scrollback {
    /// The log is truncated once it exceeds this many glyphs.
    pub const MAX_GLYPHS: usize = 10_000;
    /// …and the truncation keeps `0x1D4C` = 7500 glyphs.
    pub const KEEP_GLYPHS: usize = 0x1D4C;
}

/// One message as the display-final-string-info notice (type, body, prefix, window id)
/// delivers it.
///
/// Defined in [`dereth_client_contract::chat::interface`]: that notice is where a chat line
/// *enters* the UI, so the value that carries it is the contract and not the drawing.
pub use dereth_client_contract::chat::interface::ChatMessage;

pub use dereth_client_contract::chat::interface::{add_text_to_scroll_trim, Routed};

/// `ChatInterface` — one chat window's state.
#[derive(Debug, Clone)]
pub struct ChatInterface {
    /// The window id, attribute `0x1000007E`.
    pub window_id: u32,
    /// The 64-bit text-type filter.
    pub filter: u64,
    /// The log, one entry per appended run. Glyph counting is by `char`, which is what the client's
    /// glyph list counts for the ASCII and Latin-1 text this cap ever sees.
    pub log: Vec<(u8, String)>,
    /// The live edit line.
    pub entry: String,
    /// The default and active opacity, attributes `0x10000080` / `0x10000081`.
    pub default_opacity: f32,
    /// See [`Self::default_opacity`].
    pub active_opacity: f32,
    /// The last value pushed through the window's material-opacity setter.
    /// It tracks the effective fade value separately from both endpoint settings.
    /// It is where the three setters write. [verified against retail]
    pub current_opacity: f32,
    /// Whether the window is registered for global message 3 — i.e. whether a fade is in flight.
    ///
    /// The global-message handler unregisters from global message 3 the moment the target is
    /// reached, and the element-message handler re-registers on the five focus / rollover messages. Fade is
    /// therefore free when nothing is happening; a window that stayed registered would pay for
    /// the fade every frame.
    pub fading: bool,
    /// The engagement the last [`Self::fade_tick`] saw, so a change can re-arm [`Self::fading`]
    /// the way the five element messages do. `None` before the first tick.
    pub was_engaged: Option<bool>,
    /// True once a message landed while the log was not scrolled to the bottom.
    pub new_non_visible_text: bool,
    /// Where the last [`Self::truncate_chat_log`] cut, so the log **element** can behead at the
    /// same glyph.
    last_truncation_cut: usize,
    /// The last chat-entry-toggle notice value — true after
    /// activating the chat entry, false after deactivating it and after
    /// the enter-key handler's focus drop.
    ///
    /// The notice's own receivers are outside this crate (gameplay uses it to decide whether
    /// a keystroke is chat or a hotkey), so the flag is kept rather than emitted; a host that wants
    /// the notice reads it.
    pub chat_entry_active: bool,
}

/// The per-frame fraction of `|active − default|` the fade moves. The frame-message handler
/// multiplies by the single-precision float 0.05.
pub const FADE_STEP_FRACTION: f32 = 0.05;

/// The chat interface's attribute-set handler's two attributes.
///
/// Defined, with the `1.0f` fallback and the transcribed `switch`, in
/// [`dereth_client_contract::chat::interface::opacity_attr`], because `dereth_client::hud` reads
/// both ids off the retained `PlayerModule`.
pub use dereth_client_contract::chat::interface::opacity_attr;

/// The client's **name lookup** — the half that
/// needs the dats.
///
/// The client's preference initialisation attaches `UI.ChatFontFace` to a static font-face setting
/// with the five font-face choices ([`chat_font::FACES`]) and the font-preference change callback,
/// and `UI.ChatFontSize` to a static font-size setting the same way.
///
/// The callback reads the current font-face and font-size preferences and broadcasts
/// them together in a font-settings-changed notice, with face first and size second.
///
/// and the chat interface's post-init ends by calling the same handler with the same two
/// statics, which is the startup half.
///
/// [`font_enum_name`] builds `"Chat_%s_%s"` and [`resolve_chat_font`] walks it to a font `DataID`.
pub mod chat_font {
    /// The five font-face choices, narrow string entries.
    pub const FACES: [&str; 5] = [
        "Arial",
        "CourierNew",
        "PalatinoLinotype",
        "Tahoma",
        "TimesNewRoman",
    ];
    /// The five font-size choices.
    pub const SIZES: [&str; 5] = ["Tiny", "Small", "Medium", "Large", "XL"];
    /// The DID-by-enum lookup `(0x10000001, 2, 0x26)`'s two arguments, in
    /// [`dereth_assets::did_by_enum`]'s `(group, value)` order. Group 2 is `UNIQUEDB`, and
    /// `0x25000002`'s row `0x10000001` is the font enum-id map `0x25000012` — measured against the
    /// shipped dats, which carry exactly the 25 `Chat_<Face>_<Size>` names this pair of arrays
    /// spells.
    pub const MAPPER_GROUP: u32 = 2;
    /// See [`MAPPER_GROUP`].
    pub const MAPPER_VALUE: u32 = 0x1000_0001;
}

/// The name is formatted as `"Chat_%s_%s"` from face and size — **face first**.
///
/// `None` for an index outside the five choices. Retail's own guards reject only values above 5,
/// i.e. they admit **5**, one past the end of both arrays; the index can only come from the font-choice menu
/// with five rows, so the read is unreachable, and refusing it here produces the same observable
/// (a name the name-to-enum lookup would not find) without reading past an array.
#[must_use]
pub fn font_enum_name(face: usize, size: usize) -> Option<String> {
    let f = chat_font::FACES.get(face)?;
    let s = chat_font::SIZES.get(size)?;
    Some(format!("Chat_{f}_{s}"))
}

///  then, against the mapper
/// the DID-by-enum lookup `(0x10000001, 2, 0x26)` resolves.
///
/// Returns the font `DataID` would be handed, or `None` if any hop misses —
/// which is retail's behaviour too: every failure arm leaves the log's font alone.
#[must_use]
pub fn resolve_chat_font(
    ui: &dereth_ui::UiSystem,
    face: usize,
    size: usize,
) -> Option<dereth_primitives::DataId> {
    let name = font_enum_name(face, size)?;
    let mapper = ui
        .env()
        .cloned()
        .and_then(|e| e.did_by_enum(chat_font::MAPPER_GROUP, chat_font::MAPPER_VALUE))?;
    ui.env().cloned().map(|e| {
        e.with_assets(|assets| {
            let bytes = assets.read(mapper).ok()?;
            let m = <dereth_assets::tables::DidMapper as dereth_assets::Decode>::decode_payload(
                mapper, &bytes,
            )
            .ok()?;
            let value = m
                .enum_to_name
                .iter()
                .find(|(_, n)| *n == name)
                .map(|(e, _)| *e)?;
            m.enum_to_id
                .iter()
                .find(|(e, _)| *e == value)
                .map(|(_, d)| dereth_primitives::DataId(*d))
                .filter(|d| d.0 != 0)
        })
    })?
}

impl ChatInterface {
    /// The post-init's window-id switch: the filter default comes from the id.
    #[must_use]
    pub fn new(window_id: u32) -> Self {
        Self {
            window_id,
            filter: default_filter(window_id),
            log: Vec::new(),

            entry: String::new(),
            default_opacity: 1.0,
            active_opacity: 1.0,
            current_opacity: 1.0,
            fading: false,
            was_engaged: None,
            new_non_visible_text: false,
            last_truncation_cut: 0,
            chat_entry_active: false,
        }
    }

    /// Whether the filter passes a chat type — `((filter >> type) & 1) != 0`.
    #[must_use]
    pub fn type_is_active(&self, ty: u8) -> bool {
        dereth_client_contract::chat::interface::type_is_active(self.filter, u32::from(ty))
    }

    /// The display final string info notice's routing test: a message whose window id is not
    /// this window's is dropped when it is addressed to a different window (non-zero id), and a
    /// broadcast (id 0) is dropped when this window filters the type out.
    ///
    /// Note what it does *not* do: a message addressed to this window by id bypasses the filter.
    #[must_use]
    pub fn route(&self, m: &ChatMessage) -> Routed {
        dereth_client_contract::chat::interface::route(
            self.window_id,
            self.filter,
            u32::from(m.ty),
            m.window,
        )
    }

    /// The rest of the display-final-string-info notice: the newline separator, the grey prefix,
    /// the body in its own colour, the 10000/7500 truncation and the "new text below" flag.
    ///
    /// `was_at_end` is whether the chat log was at its vertical end, read **before** the append.
    pub fn recv_display_final_string_info(&mut self, m: &ChatMessage, was_at_end: bool) -> Routed {
        let r = self.route(m);
        if r != Routed::Accepted {
            return r;
        }
        if !self.log.is_empty() {
            self.log.push((m.ty, "\n".to_owned()));
        }
        if let Some(p) = &m.prefix {
            self.log
                .push((super::colors::PREFIX_COLOR_INDEX, p.clone()));
        }
        self.log
            .push((m.ty, add_text_to_scroll_trim(&m.body).to_owned()));
        if self.glyph_count() > scrollback::MAX_GLYPHS {
            self.truncate_chat_log(scrollback::KEEP_GLYPHS);
        }
        if !was_at_end {
            self.new_non_visible_text = true;
        }
        r
    }

    /// The log's glyph count.
    #[must_use]
    pub fn glyph_count(&self) -> usize {
        self.log.iter().map(|(_, s)| s.chars().count()).sum()
    }

    /// The log as one string, which is what the text element holds underneath the runs.
    #[must_use]
    pub fn log_text(&self) -> String {
        self.log.iter().map(|(_, s)| s.as_str()).collect()
    }

    /// Truncate the chat log, keeping `keep` glyphs.
    ///
    /// "It does not cut blindly: it looks for a `'\n'` **backwards** from the cut point, and if that
    /// newline is more than *total/10* glyphs away it looks **forwards** instead; if that is also
    /// more than *total/10* away it cuts at the raw position. Then
    /// the log is beheaded there, dropping everything before it."
    pub fn truncate_chat_log(&mut self, keep: usize) {
        let Some(cut) = self.truncation_cut_point(keep) else {
            return;
        };
        self.last_truncation_cut = cut;
        self.behead_text(cut);
    }

    /// The glyph index [`Self::truncate_chat_log`] cuts at, without performing the cut.
    ///
    /// Split out so that the **element** and the model behead at the same place: the client has
    /// one glyph list and beheads it once, this build has the model's runs and the log element's
    /// `GlyphList`, and a cut computed twice from two different totals would drift.
    #[must_use]
    pub fn truncation_cut_point(&self, keep: usize) -> Option<usize> {
        let text: Vec<char> = self.log_text().chars().collect();
        let total = text.len();
        if total <= keep {
            return None;
        }
        let raw = total - keep;
        let slack = total / 10;
        let back = text[..raw].iter().rposition(|c| *c == '\n');
        Some(match back {
            Some(b) if raw - b <= slack => b,
            _ => match text[raw..].iter().position(|c| *c == '\n').map(|i| raw + i) {
                Some(f) if f - raw <= slack => f,
                _ => raw,
            },
        })
    }

    /// The glyph index the last [`Self::truncate_chat_log`] beheaded at, for the element to match.
    #[must_use]
    pub const fn last_truncation_cut(&self) -> usize {
        self.last_truncation_cut
    }

    /// Behead the coloured runs: drop the first `pos` glyphs.
    fn behead_text(&mut self, pos: usize) {
        let mut dropped = 0;
        let mut out: Vec<(u8, String)> = Vec::new();
        for (ty, s) in std::mem::take(&mut self.log) {
            let n = s.chars().count();
            if dropped + n <= pos {
                dropped += n;
                continue;
            }
            if dropped < pos {
                let keep: String = s.chars().skip(pos - dropped).collect();
                dropped = pos;
                if !keep.is_empty() {
                    out.push((ty, keep));
                }
            } else {
                out.push((ty, s));
            }
        }
        self.log = out;
    }

    /// The clear-chat-buffer notice `(window_id)` — clears the log when `window_id` is 0 or
    /// matches this window.
    pub fn recv_clear_chat_buffer(&mut self, window_id: u32) {
        if window_id == 0 || window_id == self.window_id {
            self.log.clear();
            self.new_non_visible_text = false;
        }
    }

    /// Process one submitted command, the six documented steps.
    ///
    /// Step 2: "Empty (length 1, i.e. just the terminator) → do nothing" — an empty entry submits
    /// nothing and is **not** appended to the history.
    #[must_use]
    pub fn process_command(&mut self) -> Option<UiRequest> {
        let text = std::mem::take(&mut self.entry);
        if text.is_empty() {
            self.entry = text;
            return None;
        }
        let req = UiRequest::ChatLine {
            text: text.clone(),
            window: self.window_id,
        };
        self.entry.clear();
        Some(req)
    }

    /// The client's **head** — "keep `default ≤ active`".
    ///
    /// The tail, which pushes the new value through the window's opacity setter when it is idle,
    /// needs the element and lives on `super::window::set_default_opacity`; without it the slider
    /// would move the target and leave the value to the fade, which is twenty frames of 5 % steps
    /// and at login never runs at all.
    pub fn set_default_opacity(&mut self, v: f32) {
        self.default_opacity = v;
        if self.default_opacity > self.active_opacity {
            self.active_opacity = self.default_opacity;
        }
    }

    /// See [`Self::set_default_opacity`].
    pub fn set_active_opacity(&mut self, v: f32) {
        self.active_opacity = v;
        if self.default_opacity > self.active_opacity {
            self.default_opacity = self.active_opacity;
        }
    }

    /// Handle the chat interface's gameplay-option-changed notice, raised
    /// with `(property, windowId)` every time a chat-window option is set.
    ///
    /// ```text
    /// property == 0x1000007F         // only the text-type filter
    /// window_id == self.window_id    // only this chat window
    /// ```
    ///
    /// Both tests matter: the notice is a broadcast, so without the window-id compare one
    /// window's filter would be written into all five. Returns whether this window took it.
    ///
    /// The opacity half of the same notice is **not** here: the floaty main chat window's own
    /// gameplay-option-changed notice intercepts `0x10000080` / `0x10000081` before
    /// chaining to this body, and it applies them with **no** window-id test — which is why one
    /// pair of sliders moves all five windows. That arm is
    /// [`Self::set_default_opacity`] / [`Self::set_active_opacity`], driven from the screen.
    pub fn on_gameplay_option_changed(&mut self, property: u32, window_id: u32, mask: u64) -> bool {
        if property != super::super::options::pages::CHAT_FILTER_PROPERTY
            || window_id != self.window_id
        {
            return false;
        }
        self.filter = mask;
        true
    }

    /// The filter third of
    /// The floaty chat panel's update from the player module's *"filter, position, opacity"*.
    ///
    /// It reads chat-window option `0x1000007F` for this window id.
    ///
    /// The zero-window-id guard is the client's and is reproduced: a window with no
    /// `0x1000007E` in its layout has no blob to read and keeps its constructed filter. Returns
    /// whether the filter moved.
    pub fn update_filter_from_player_module(&mut self, mask: Option<u64>) -> bool {
        if self.window_id == 0 {
            return false;
        }
        let Some(mask) = mask else { return false };
        if self.filter == mask {
            return false;
        }
        self.filter = mask;
        true
    }

    /// Handle global message 3 — one step of the opacity fade.
    ///
    /// **There is no minimum-opacity setting in the chat interface**, so the step is not
    /// `|default − minimum| * 0.05`. The measured fields are the
    /// default endpoint, active endpoint, and current effective opacity, in that order.
    /// Their behavior is:
    ///
    /// Only global message 3 advances the fade. A mouse-over or focused text entry
    /// makes the window engaged. Each tick steps by `abs(active - default) * 0.05f`:
    /// add while engaged, subtract while idle. Reaching or passing the destination
    /// clamps to it and unregisters message 3. The material opacity receives the result.
    ///
    /// So the step is `|active − default| * 0.05`, a twentieth of the window's **own** travel.
    /// A window whose two opacities are equal never fades at all, exactly what the
    /// shipped default (both `1.0`) means. Writing it against a minimum opacity would make the step
    /// depend on a number that does not exist and would fade a window that should not.
    ///
    /// The client's constant is the single-precision `0.05` (`0.05000000074505806`).
    ///
    /// Returns the new opacity and whether the fade has settled (i.e. whether to unsubscribe).
    #[must_use]
    pub fn fade_step(&self, current: f32, engaged: bool) -> (f32, bool) {
        let step = (self.active_opacity - self.default_opacity).abs() * FADE_STEP_FRACTION;
        if engaged {
            let next = current + step;
            if next >= self.active_opacity {
                return (self.active_opacity, true);
            }
            (next, false)
        } else {
            let next = current - step;
            if next <= self.default_opacity {
                return (self.default_opacity, true);
            }
            (next, false)
        }
    }

    /// One frame of the fade, as the global-message handler runs it — plus the
    /// re-subscription the element-message handler makes on messages `0x1B`, `0x1F`,
    /// `0x29`, `0x2A` and `0x2F`.
    ///
    /// Those five are *"focus taken / focus lost / mouse entered / mouse left"* and their whole
    /// job is to put the window back on global message 3; modelling them as **a change in
    /// `engaged`** is the same edge, computed from the same two sources the fade itself reads
    /// (whether the text entry is focused and the region's mouse-over bit) rather than from a
    /// message this build does not raise. Declared deviation.
    ///
    /// Returns `Some(opacity)` on any frame the caller must push through
    /// the material-opacity setter, and `None` when the window is idle — which is the
    /// point of the unsubscribe.
    pub fn fade_tick(&mut self, engaged: bool) -> Option<f32> {
        if self.was_engaged != Some(engaged) {
            self.was_engaged = Some(engaged);
            self.fading = true;
        }
        if !self.fading {
            return None;
        }
        let (next, settled) = self.fade_step(self.current_opacity, engaged);
        self.current_opacity = next;
        if settled {
            self.fading = false;
        }
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered chat behavior's default-mask table, whose main-window row states that the
    /// window's default excludes exactly the over-head bubble bit.
    #[test]
    fn the_five_default_filters_are_the_documented_masks() {
        assert_eq!(default_filter(window::MAIN), 0xFBFF_FFFF);
        assert_eq!(default_filter(window::MAIN_ALT), 0xFBFF_FFFF);
        assert_eq!(default_filter(window::FLOATY_1), 0x0000_101C);
        assert_eq!(default_filter(window::FLOATY_2), 0x0004_0C00);
        assert_eq!(default_filter(window::FLOATY_3), 0x0008_0000);
        assert_eq!(default_filter(window::FLOATY_4), 0x7800_0000);
        // "everything except bit 26 (the over-head bubble channel)".
        assert_eq!(default_filter(window::MAIN), 0xFFFF_FFFF & !(1u64 << 26));
    }

    /// Oracle: §4's "Types accepted" column, which names the types each floaty default admits.
    #[test]
    fn each_floaty_default_admits_exactly_the_documented_types() {
        let types = |id: u32| -> Vec<u8> {
            let f = default_filter(id);
            (0u8..64).filter(|t| (f >> t) & 1 != 0).collect()
        };
        assert_eq!(
            types(window::FLOATY_1),
            vec![2, 3, 4, 12],
            "Speech, Tell, OutgoingTell, Emote"
        );
        assert_eq!(
            types(window::FLOATY_2),
            vec![10, 11, 18],
            "Social, SocialSend, Allegiance"
        );
        assert_eq!(types(window::FLOATY_3), vec![19], "Fellowship");
        assert_eq!(
            types(window::FLOATY_4),
            vec![27, 28, 29, 30],
            "the global channels"
        );
    }

    /// Oracle: the client's mask table, and §8's
    /// "one of them (`Society`) uses bit 32; a 32-bit implementation silently drops it".
    #[test]
    fn the_filter_group_masks_are_the_documented_ones_and_society_needs_sixty_four_bits() {
        assert_eq!(FILTER_GROUPS.len(), 13);
        let by = |n: &str| {
            FILTER_GROUPS
                .iter()
                .find(|g| g.label.ends_with(n))
                .unwrap()
                .mask
        };
        assert_eq!(by("Gameplay"), 0x8391_2021);
        assert_eq!(by("Combat"), 0x0060_0040);
        assert_eq!(by("Magic"), 0x0002_0080);
        assert_eq!(by("AreaSpeech"), 0x0000_1004);
        assert_eq!(by("Tells"), 0x0000_0018);
        assert_eq!(by("Allegience"), 0x0004_0C00);
        assert_eq!(by("Fellowship"), 0x0008_0000);
        assert_eq!(by("General"), 0x0800_0000);
        assert_eq!(by("Trade"), 0x1000_0000);
        assert_eq!(by("LFG"), 0x2000_0000);
        assert_eq!(by("Roleplay"), 0x4000_0000);
        assert_eq!(by("Society"), 0x1_0000_0000);
        assert_eq!(by("Error"), 0x0400_0000);
        assert!(
            by("Society") > u64::from(u32::MAX),
            "bit 32 is above a 32-bit mask"
        );
        // The Allegience group and floaty 2's default are the same mask; §4 lists both.
        assert_eq!(by("Allegience"), default_filter(window::FLOATY_2));
        assert_eq!(by("Fellowship"), default_filter(window::FLOATY_3));
    }

    /// The main window is the one without the gameplay group.
    #[test]
    fn the_main_window_is_the_one_without_the_gameplay_group() {
        let g = filter_groups_for(window::MAIN);
        assert_eq!(g.len(), 12, "window 8 skips the Gameplay check box");
        assert!(
            g[0].label.ends_with("Combat"),
            "the main window starts at Combat"
        );
        assert_eq!(filter_groups_for(window::MAIN_ALT).len(), 12);
        for id in [
            window::FLOATY_1,
            window::FLOATY_2,
            window::FLOATY_3,
            window::FLOATY_4,
        ] {
            let g = filter_groups_for(id);
            assert_eq!(g.len(), 13, "floaty {id} adds the Gameplay check box");
            assert!(
                g[0].label.ends_with("Gameplay"),
                "every floaty starts at Gameplay"
            );
        }
        // The page's whole check-box count: 12 on the main window, 13 on each of four floaties.
        let total: usize = [
            window::MAIN,
            window::FLOATY_1,
            window::FLOATY_2,
            window::FLOATY_3,
            window::FLOATY_4,
        ]
        .into_iter()
        .map(|w| filter_groups_for(w).len())
        .sum();
        assert_eq!(total, 64);
    }

    /// Oracle: the client's opening test, quoted in §2.
    #[test]
    fn routing_follows_the_window_id_then_the_filter() {
        let w = ChatInterface::new(window::FLOATY_3); // accepts only type 19
                                                      // Addressed to this window: accepted whatever the type.
        assert_eq!(
            w.route(&ChatMessage {
                ty: 6,
                window: window::FLOATY_3,
                ..Default::default()
            }),
            Routed::Accepted,
            "an addressed message bypasses the filter"
        );
        // Addressed elsewhere: dropped.
        assert_eq!(
            w.route(&ChatMessage {
                ty: 19,
                window: window::MAIN,
                ..Default::default()
            }),
            Routed::OtherWindow
        );
        // Broadcast: the filter decides.
        assert_eq!(
            w.route(&ChatMessage {
                ty: 19,
                window: 0,
                ..Default::default()
            }),
            Routed::Accepted
        );
        assert_eq!(
            w.route(&ChatMessage {
                ty: 2,
                window: 0,
                ..Default::default()
            }),
            Routed::FilteredOut
        );
    }

    /// Oracle: §2's pseudocode — the newline separator, the grey prefix and the body colour.
    #[test]
    fn an_accepted_message_appends_a_grey_prefix_and_a_typed_body() {
        let mut w = ChatInterface::new(window::MAIN);
        let m = ChatMessage {
            feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
            ty: 2,
            body: "hello".into(),
            prefix: Some("Kupo says, ".into()),
            window: 0,
        };
        assert_eq!(w.recv_display_final_string_info(&m, true), Routed::Accepted);
        assert_eq!(
            w.log,
            vec![(12, "Kupo says, ".to_owned()), (2, "hello".to_owned())]
        );
        assert!(!w.new_non_visible_text, "was at the end, so no arrow");

        // A second message gets the separator first, and this one lands off-screen.
        assert_eq!(
            w.recv_display_final_string_info(&m, false),
            Routed::Accepted
        );
        assert_eq!(w.log[2], (2, "\n".to_owned()));
        assert!(w.new_non_visible_text);
    }

    /// Oracle: the over-head bubble channel (type 26) is the one thing the main window's default
    /// filter drops; the dedicated floating-spew window picks it up instead.
    #[test]
    fn the_main_window_drops_the_over_head_bubble_channel() {
        let w = ChatInterface::new(window::MAIN);
        assert!(!w.type_is_active(26));
        assert_eq!(
            w.route(&ChatMessage {
                ty: 26,
                window: 0,
                ..Default::default()
            }),
            Routed::FilteredOut
        );
    }

    /// Oracle:, §2.1 — cut on a line boundary when one is within
    /// `total/10` of the raw cut point, otherwise mid-line.
    #[test]
    fn the_truncation_prefers_a_line_boundary_within_a_tenth_of_the_log() {
        let mut w = ChatInterface::new(window::MAIN);
        // 100 glyphs: "aaaa\n" twenty times. Keeping 75 puts the raw cut at 25, and there is a
        // newline at 24 — one glyph back, well inside total/10 = 10.
        w.log = (0..20).map(|_| (2u8, "aaaa\n".to_owned())).collect();
        assert_eq!(w.glyph_count(), 100);
        w.truncate_chat_log(75);
        assert_eq!(w.glyph_count(), 100 - 24, "cut at the newline, not at 25");
        assert!(w.log_text().starts_with('\n'));

        // One 100-glyph line with no newline anywhere: the cut is raw.
        let mut w = ChatInterface::new(window::MAIN);
        w.log = vec![(2, "b".repeat(100))];
        w.truncate_chat_log(75);
        assert_eq!(w.glyph_count(), 75);
    }

    /// Oracle: §2.1 — the truncation fires only once the log exceeds 10000 glyphs, and keeps 7500.
    #[test]
    fn the_truncation_thresholds_are_ten_thousand_and_seven_thousand_five_hundred() {
        assert_eq!(scrollback::MAX_GLYPHS, 10_000);
        assert_eq!(scrollback::KEEP_GLYPHS, 7500);
        assert_eq!(scrollback::KEEP_GLYPHS, 0x1D4C);

        let mut w = ChatInterface::new(window::MAIN);
        w.log = vec![(2, "x".repeat(9_999))];
        let m = ChatMessage {
            feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
            ty: 2,
            body: "y".into(),
            window: 0,
            prefix: None,
        };
        w.recv_display_final_string_info(&m, true);
        // 9999 + 1 (separator) + 1 = 10001 > 10000, so it truncates to 7500.
        assert_eq!(w.glyph_count(), 7500);
    }

    /// The opacity pair stays ordered and the fade settles.
    #[test]
    fn the_opacity_pair_stays_ordered_and_the_fade_settles() {
        let mut w = ChatInterface::new(window::MAIN);
        w.set_active_opacity(0.5);
        w.set_default_opacity(0.9);
        assert!(
            w.default_opacity <= w.active_opacity,
            "the setter raises active to match"
        );
        assert_eq!(w.active_opacity, 0.9);

        w.default_opacity = 0.6;
        w.active_opacity = 1.0;
        // |1.0 - 0.6| * 0.05 = 0.02.
        let (v, done) = w.fade_step(0.6, true);
        assert!(
            (v - 0.62).abs() < 1e-6,
            "step is |1.0-0.6|*0.05 = 0.02, got {v}"
        );
        assert!(!done);
        let (v, done) = w.fade_step(0.995, true);
        assert_eq!(
            (v, done),
            (1.0, true),
            "past the target it clamps and unsubscribes"
        );
        // Idle: the same step downwards, clamped at the default.
        let (v, done) = w.fade_step(1.0, false);
        assert!((v - 0.98).abs() < 1e-6, "got {v}");
        assert!(!done);
        assert_eq!(w.fade_step(0.605, false), (0.6, true));

        // A window whose two opacities agree never moves and never stays subscribed.
        let flat = ChatInterface::new(window::MAIN);
        assert_eq!(flat.default_opacity, 1.0);
        assert_eq!(flat.active_opacity, 1.0);
        assert_eq!(flat.fade_step(1.0, false), (1.0, true));
        assert_eq!(flat.fade_step(1.0, true), (1.0, true));

        // `fade_tick` is the subscription: the first call arms it because engagement went from
        // "unknown" to a value, and it is off again as soon as the target is reached.
        let mut w = ChatInterface::new(window::MAIN);
        w.default_opacity = 0.6;
        w.active_opacity = 1.0;
        w.current_opacity = 1.0;
        assert_eq!(w.fade_tick(false), Some(0.98));
        assert!(w.fading, "still travelling");
        for _ in 0..64 {
            if w.fade_tick(false).is_none() {
                break;
            }
        }
        assert!(!w.fading, "the fade unsubscribed itself");
        assert!((w.current_opacity - 0.6).abs() < 1e-6);
        assert_eq!(
            w.fade_tick(false),
            None,
            "and an idle window costs nothing per frame"
        );
        // The mouse arrives: engagement changed, so the window re-subscribes.
        assert!(w.fade_tick(true).is_some());
        assert!(w.fading);
    }

    /// Oracle: — "clears the log when `windowId` is 0 or
    /// matches".
    #[test]
    fn clear_chat_buffer_takes_zero_or_this_window() {
        let mut w = ChatInterface::new(window::FLOATY_2);
        w.log = vec![(2, "x".into())];
        w.recv_clear_chat_buffer(window::FLOATY_1);
        assert_eq!(w.log.len(), 1, "another window's clear is ignored");
        w.recv_clear_chat_buffer(0);
        assert!(w.log.is_empty());
        w.log = vec![(2, "x".into())];
        w.recv_clear_chat_buffer(window::FLOATY_2);
        assert!(w.log.is_empty());
    }
}
