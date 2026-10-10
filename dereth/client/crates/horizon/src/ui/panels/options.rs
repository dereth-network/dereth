//! Settings: the pages both other interfaces show, from the shared options sheet
//! ([`dereth_client_contract::options::sheet`]) as this interface sees it, then this interface's
//! own controls and the key bindings.
//!
//! * **Character**: the character's own options, which the server keeps.
//! * **Chat**: the log window's opacity, active and inactive; then each tab's name and, but for
//!   the main tab, its opacity popped out, kept with this interface's settings, and its chat
//!   window's message filter, which the server keeps: the same filter the tab's own menu sets.
//! * **Client**: the profile's settings: display, graphics, sound, camera and the interface choice,
//!   with this interface's scale beside the interface choice and its smooth animation last under
//!   the graphics quality; in a build with the experimental
//!   rendering effects, their boxes last, under a heading of their own with a warning, the
//!   status of the device they draw on, and a greyed box with its reason where what it needs is
//!   off. Before them, wherever there are renderers to choose from (not in the browser), the
//!   renderer the next start comes up on, with the one in use now under it.
//! * **Controls**: how the movement keys move the character, which way the pointer turns the
//!   camera, its speeds and limits and how quickly it comes round behind the character, and the
//!   pad: whether it is read, its sticks' dead zone and its camera speed.
//! * **Key Bindings**: every bindable action, rebound by pressing the new key.
//!
//! Every sheet row, its caption and where its value is kept is the sheet's; every value shown is
//! the shared store's or the game's, so a change made in another interface shows here. A choice of
//! several is a dropdown box whose list opens over the window.

#[cfg(feature = "hifi")]
use dereth_client_contract::options::fidelity;
use dereth_client_contract::options::interface::Interface;
use dereth_client_contract::options::renderer;
use dereth_client_contract::options::sheet::{self, PageId, Row, Value};
use dereth_client_contract::options::store;
use dereth_client_contract::view::PrefValue;
use dereth_client_contract::UiRequest;

use super::Windows;
use crate::art::Family;
use crate::draw::Rect;
use crate::ui::game::{GameState, KeyRequest, KEY_GROUPS};
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

/// The window's tabs, in order.
const TABS: [&str; 5] = ["Character", "Chat", "Client", "Controls", "Key Bindings"];

/// The tab index of each page.
pub mod tab {
    pub const CHARACTER: usize = 0;
    pub const CHAT: usize = 1;
    pub const CLIENT: usize = 2;
    pub const CONTROLS: usize = 3;
    pub const KEYS: usize = 4;
}

/// How many keys a row of the key bindings page shows.
pub const KEY_SLOTS: usize = 3;

/// The interface scales offered, as the choice reads and as the scale is kept.
pub const SCALES: [(&str, f32); 4] = [("100%", 1.0), ("150%", 1.5), ("200%", 2.0), ("300%", 3.0)];

/// The caption of the interface scale's row, on the Client page under the interface choice.
const SCALE_CAPTION: &str = "Interface Scale";

/// The caption of the smooth animation's row, on the Client page last under the graphics quality,
/// and what it does, for its tooltip.
const SMOOTH_CAPTION: &str = "Smooth Animation";
const SMOOTH_NOTE: &str = "Draws every body between its animation's frames, at any frame rate, \
                           rather than stepping from one frame to the next. Only the drawing \
                           changes.";

/// The heading the smooth animation's row closes.
const SMOOTH_HEADING: &str = "Graphics Quality";

/// A dropdown box's list, open over the window.
#[derive(Debug, Clone)]
pub struct OpenMenu {
    /// The caption of the row whose box it opened from.
    pub key: String,
    /// The box, where it was last drawn.
    pub anchor: Rect,
    pub names: Vec<String>,
    /// The choice the box shows.
    pub at: usize,
    /// The first row shown of a long list.
    pub top: usize,
}

/// The window's own state.
#[derive(Debug, Default)]
pub struct OptionsState {
    pub tab: usize,
    /// Each page's scroll.
    pub scroll: [f32; TABS.len()],
    /// The key bindings page's group.
    key_group: usize,
    /// This interface's scale as it stands.
    pub scale: f32,
    /// The camera's movement scheme and pointer directions as they stand.
    pub orbit: dereth_client_runtime::orbit::OrbitSettings,
    /// The pad's settings as they stand.
    pub pad: crate::pad::PadSettings,
    /// Whether the minimap turns with the character.
    pub minimap_rotates: bool,
    /// Whether bodies are drawn between their animations' keyframes.
    pub smooth_animation: bool,
    /// The log window's tab names as they stand.
    pub chat_tabs: crate::options::TabNames,
    /// The log window's opacities as they stand.
    pub chat_opacity: crate::options::ChatOpacity,
    /// The interface chosen on the Interface row and not applied yet: switching interfaces
    /// waits for Apply.
    pub pending_interface: Option<i32>,
    /// The dropdown list open over the window, if one is.
    pub menu: Option<OpenMenu>,
    /// A choice made in a dropdown's list this frame, by its row's caption, for its row to act
    /// on.
    menu_choice: Option<(String, usize)>,
    /// Where each control was drawn this frame, by its caption: what a scripted run or a test
    /// clicks.
    pub controls: Vec<(String, Rect)>,
}

impl OptionsState {
    /// Where the control captioned `label` was drawn this frame.
    #[must_use]
    pub fn control(&self, label: &str) -> Option<Rect> {
        self.controls
            .iter()
            .find(|(l, _)| l == label)
            .map(|(_, r)| *r)
    }
}

impl Windows {
    pub(super) fn options(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        self.options_page.controls.clear();
        // An open dropdown list takes the presses over it before anything under it is drawn,
        // and a press anywhere else closes it.
        let occluders = ctx.input.occluders.len();
        if let Some(menu) = &self.options_page.menu {
            if kit::dropdown_dismissed(p, ctx, menu.anchor, menu.names.len()) {
                self.options_page.menu = None;
            } else {
                let r = kit::dropdown_list_rect(p, menu.anchor, menu.names.len());
                ctx.input.occluders.push(r);
            }
        }
        let tabs = Rect::new(body.x, body.y, body.w, 26.0 * k);
        for (label, r) in TABS.iter().zip(kit::tab_rects(p, tabs, &TABS)) {
            self.options_page.controls.push(((*label).to_owned(), r));
        }
        if kit::tabs(p, ctx, tabs, &TABS, &mut self.options_page.tab) {
            self.options_page.menu = None;
        }
        let area = Rect::new(body.x, body.y + 36.0 * k, body.w, body.h - 40.0 * k);
        match self.options_page.tab {
            tab::CHARACTER => self.character_options_tab(p, ctx, state, body, out),
            tab::CHAT => self.chat_options(p, ctx, state, area, out),
            tab::CLIENT => self.client_options(p, ctx, state, area, out),
            tab::CONTROLS => self.controls_options(p, ctx, area, out),
            tab::KEYS => self.key_bindings(p, ctx, state, area, out),
            _ => {}
        }
        ctx.input.occluders.truncate(occluders);
        // A choice its row was not drawn to take (its page left) is dropped.
        self.options_page.menu_choice = None;
        // The open list, over the page, its choice acted on by its row next frame.
        if let Some(menu) = self.options_page.menu.as_mut() {
            let names: Vec<&str> = menu.names.iter().map(String::as_str).collect();
            if let Some(i) = kit::dropdown_list(p, ctx, menu.anchor, &names, menu.at, &mut menu.top)
            {
                let key = menu.key.clone();
                self.options_page.menu = None;
                self.options_page.menu_choice = Some((key, i));
            }
        }
    }

    /// A dropdown box over `r` for the row captioned `key`, showing `names[at]`: a click opens
    /// its list over the window (or closes it). The choice made in its list, the frame after,
    /// when it is not the one shown.
    fn dropdown(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        key: &str,
        r: Rect,
        names: &[&str],
        at: usize,
    ) -> Option<usize> {
        let page = &mut self.options_page;
        let open = page.menu.as_ref().is_some_and(|m| m.key == key);
        if open {
            if let Some(m) = page.menu.as_mut() {
                // The box moves with its page's scroll; its list goes with it.
                m.anchor = r;
            }
        }
        let shown = names.get(at).copied().unwrap_or("");
        if kit::dropdown_box(p, ctx, r, shown, open, !names.is_empty()) {
            page.menu = Some(OpenMenu {
                key: key.to_owned(),
                anchor: r,
                names: names.iter().map(|n| (*n).to_owned()).collect(),
                at,
                top: at.saturating_sub(kit::DROPDOWN_ROWS / 2),
            });
        }
        // Choosing what it already shows changes nothing.
        page.menu_choice
            .take_if(|(k, _)| k == key)
            .map(|(_, i)| i)
            .filter(|i| *i < names.len() && *i != at)
    }

    /// A button, its place noted under its caption.
    fn button(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        r: Rect,
        label: &str,
        enabled: bool,
    ) -> bool {
        self.options_page.controls.push((label.to_owned(), r));
        kit::button(p, ctx, r, label, enabled)
    }

    /// Note where the control captioned `label` is.
    fn mark(&mut self, label: &str, r: Rect) {
        self.options_page.controls.push((label.to_owned(), r));
    }

    /// Whether a question of the settings' is up, which takes every press and key: the key
    /// bindings page's, while it waits for a key.
    #[must_use]
    pub fn options_modal(&self, state: &GameState) -> bool {
        state
            .key_bindings
            .as_ref()
            .and_then(|k| k.capture.as_ref())
            .is_some_and(|c| c.question.is_some())
    }

    /// The question the key bindings page asks over everything while it waits for a key.
    pub fn options_questions(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        let Some(c) = state.key_bindings.as_ref().and_then(|v| v.capture.as_ref()) else {
            return;
        };
        let (text, buttons): (String, &[&str]) = match &c.question {
            Some(q) => (q.clone(), &["Yes", "No"]),
            None => (
                format!(
                    "Press the key or button for {}.

Press Escape to leave it as it is.",
                    c.caption
                ),
                &["Cancel"],
            ),
        };
        // Waiting for a key, the box takes no key itself, so Enter and Escape can be what is
        // pressed; asking, it is the player's answer it waits for.
        let asking = c.question.is_some();
        if let Some(b) =
            crate::ui::pregame::dialog_box(p, ctx, "Key Bindings", &text, buttons, asking, 0)
        {
            out.key_requests.push(if asking {
                KeyRequest::Answer(b == 0)
            } else {
                KeyRequest::Cancel
            });
        }
    }

    // -----------------------------------------------------------------------------------------
    // Chat
    // -----------------------------------------------------------------------------------------

    fn chat_options(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        area: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let rows = chat_lines(state);
        let row_h = 30.0 * k;
        let label = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let heading =
            TextStyle::new(Family::Heading, 18.4, ctx.colours.heading()).edge(ctx.colours.edge());
        let list = Rect::new(area.x, area.y, area.w, area.h - 40.0 * k);
        #[allow(clippy::cast_precision_loss)]
        let content = row_h * rows.len() as f32;
        self.mark("list", list);
        let offset = kit::scroll(
            p,
            ctx,
            list,
            content,
            &mut self.options_page.scroll[tab::CHAT],
        );
        p.list.push_clip(list);
        let mut y = list.y - offset;
        for line in &rows {
            if y + row_h >= list.y && y <= list.bottom() {
                match line {
                    Line::Heading(title) => {
                        p.text_in(
                            &heading,
                            Rect::new(list.x + 8.0 * k, y, list.w * 0.5, row_h),
                            Align::Left,
                            title,
                        );
                    }
                    Line::TabHeading(slot) => {
                        p.text_in(
                            &heading,
                            Rect::new(list.x + 8.0 * k, y, list.w * 0.5, row_h),
                            Align::Left,
                            TAB_HEADINGS[*slot],
                        );
                    }
                    Line::TabName(slot) => {
                        let slot = *slot;
                        p.text_in(
                            &label,
                            Rect::new(list.x + 16.0 * k, y, list.w * 0.5, row_h),
                            Align::Left,
                            "Name",
                        );
                        let field =
                            Rect::new(list.x + list.w * 0.55, y + 2.0 * k, list.w * 0.42, 26.0 * k);
                        self.mark(&format!("Name {}", slot + 1), field);
                        let id = TAB_NAME_FIELD + u32::try_from(slot).unwrap_or(0);
                        let kept = self.options_page.chat_tabs.0[slot].clone();
                        let mut text = kept.clone();
                        kit::text_box(p, ctx, field, &mut text, &mut self.typing_field, id);
                        let key = crate::options::TabNames::key(slot);
                        if text != kept {
                            out.settings.push((key, text));
                        } else if self.typing_field != id && kept.trim().is_empty() {
                            // A name left blank is the tab's own again once the box lets go.
                            out.settings
                                .push((key, crate::options::DEFAULT_TAB_NAMES[slot].to_owned()));
                        }
                    }
                    Line::Opacity(slot, active) => {
                        let caption = opacity_caption(*slot, *active);
                        p.text_in(
                            &label,
                            Rect::new(list.x + 16.0 * k, y, list.w * 0.5, row_h),
                            Align::Left,
                            caption,
                        );
                        let control =
                            Rect::new(list.x + list.w * 0.55, y + 2.0 * k, list.w * 0.42, 26.0 * k);
                        self.mark(&opacity_mark(*slot, *active), control);
                        // Shown in percent.
                        let value = self.options_page.chat_opacity.get(*slot, *active) * 100.0;
                        if let Some(v) = kit::slider(p, ctx, control, value, 0.0, 100.0) {
                            out.settings.push((
                                crate::options::ChatOpacity::key(*slot, *active),
                                (v / 100.0).to_string(),
                            ));
                        }
                    }
                    Line::Scale | Line::SmoothAnimation | Line::Renderer | Line::RendererNotice => {
                    }
                    #[cfg(feature = "hifi")]
                    Line::Notice(_) | Line::EffectsStatus => {}
                    Line::Row(row) => match row.value {
                        Value::Opacity(property) => {
                            p.text_in(
                                &label,
                                Rect::new(list.x + 16.0 * k, y, list.w * 0.5, row_h),
                                Align::Left,
                                row.caption,
                            );
                            let control = Rect::new(
                                list.x + list.w * 0.55,
                                y + 2.0 * k,
                                list.w * 0.42,
                                26.0 * k,
                            );
                            self.mark(row.caption, control);
                            let at = usize::from(property == 0x1000_0081);
                            let value = state.chat_opacity[at].unwrap_or(1.0);
                            if let Some(v) = kit::slider(p, ctx, control, value, 0.0, 1.0) {
                                out.requests
                                    .push(UiRequest::SetChatOpacity { property, value: v });
                            }
                        }
                        Value::Filter { window, mask } => {
                            let filter = crate::ui::chat::window_filter(state, window);
                            let cb = Rect::new(list.x + 16.0 * k, y, list.w - 32.0 * k, row_h);
                            self.mark(
                                &format!("{}@{window:x}", row.caption),
                                Rect::new(cb.x, cb.y, 20.0 * k, cb.h),
                            );
                            let on = filter & mask == mask;
                            if let Some(on) = kit::check_row(p, ctx, cb, on, &label, row.caption) {
                                out.requests.push(UiRequest::SetChatWindowFilter {
                                    window,
                                    mask: if on { filter | mask } else { filter & !mask },
                                });
                            }
                        }
                        _ => {}
                    },
                }
            }
            y += row_h;
        }
        p.list.pop_clip();
        let defaults = Rect::new(
            area.x + 12.0 * k,
            area.bottom() - 34.0 * k,
            140.0 * k,
            30.0 * k,
        );
        // Every window's filter back to the one this interface starts a character with, and the
        // tabs' names and the opacities to their first.
        if self.button(p, ctx, defaults, "Defaults", true) {
            for window in crate::ui::chat::WINDOWS {
                out.requests.push(UiRequest::SetChatWindowFilter {
                    window,
                    mask: crate::ui::chat::default_filter(window),
                });
            }
            for (slot, name) in crate::options::DEFAULT_TAB_NAMES.iter().enumerate() {
                out.settings
                    .push((crate::options::TabNames::key(slot), (*name).to_owned()));
            }
            for line in chat_lines(state) {
                if let Line::Opacity(slot, active) = line {
                    out.settings.push((
                        crate::options::ChatOpacity::key(slot, active),
                        crate::options::DEFAULT_CHAT_OPACITY.to_string(),
                    ));
                }
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Client
    // -----------------------------------------------------------------------------------------

    fn client_options(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        area: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let rows = sheet_lines(PageId::Client, state);
        let row_h = 34.0 * k;
        let label = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let heading =
            TextStyle::new(Family::Heading, 18.4, ctx.colours.heading()).edge(ctx.colours.edge());
        let list = Rect::new(area.x, area.y, area.w, area.h - 40.0 * k);
        #[allow(clippy::cast_precision_loss)]
        let content = row_h * rows.len() as f32;
        self.mark("list", list);
        let offset = kit::scroll(
            p,
            ctx,
            list,
            content,
            &mut self.options_page.scroll[tab::CLIENT],
        );
        p.list.push_clip(list);
        let mut y = list.y - offset;
        for line in &rows {
            if y + row_h >= list.y && y <= list.bottom() {
                match line {
                    Line::Heading(title) => {
                        p.text_in(
                            &heading,
                            Rect::new(list.x + 8.0 * k, y, list.w * 0.5, row_h),
                            Align::Left,
                            title,
                        );
                    }
                    Line::Row(row) => {
                        let caption = row.caption_for(Interface::Horizon);
                        p.text_in(
                            &label,
                            Rect::new(list.x + 16.0 * k, y, list.w * 0.5, row_h),
                            Align::Left,
                            caption,
                        );
                        let control =
                            Rect::new(list.x + list.w * 0.55, y + 3.0 * k, list.w * 0.42, 28.0 * k);
                        self.mark(caption, control);
                        #[cfg(feature = "hifi")]
                        {
                            self.client_row(p, ctx, row, control, state, out);
                            // The effect's note, over its caption where the list shows it, is the
                            // windows' tooltip: drawn after them all, over every row of the page.
                            let at = Rect::new(list.x + 16.0 * k, y, list.w * 0.5, row_h)
                                .intersect(&list);
                            if let Some(note) = row.note.filter(|_| effect_box(row).is_some()) {
                                if at.is_some_and(|at| ctx.over(&at)) {
                                    self.tip = Some((caption.to_owned(), vec![note.to_owned()]));
                                }
                            }
                        }
                        #[cfg(not(feature = "hifi"))]
                        self.client_row(p, ctx, row, control, out);
                    }
                    #[cfg(feature = "hifi")]
                    Line::Notice(text) => {
                        let dim = TextStyle::new(Family::Body, 13.0, ctx.colours.dim())
                            .edge(ctx.colours.edge());
                        p.text_in(
                            &dim,
                            Rect::new(list.x + 16.0 * k, y, list.w - 32.0 * k, row_h),
                            Align::Left,
                            text,
                        );
                    }
                    #[cfg(feature = "hifi")]
                    Line::EffectsStatus => {
                        if let Some(status) = effects_status(state) {
                            let shown = format!("Status: {status}");
                            let at = Rect::new(list.x + 16.0 * k, y, list.w - 32.0 * k, row_h);
                            p.text_in(&label, at, Align::Left, &shown);
                            self.mark(&shown, at);
                        }
                    }
                    Line::TabHeading(_) | Line::TabName(_) | Line::Opacity(..) => {}
                    Line::Renderer => {
                        p.text_in(
                            &label,
                            Rect::new(list.x + 16.0 * k, y, list.w * 0.5, row_h),
                            Align::Left,
                            renderer::CAPTION,
                        );
                        let control =
                            Rect::new(list.x + list.w * 0.55, y + 3.0 * k, list.w * 0.42, 28.0 * k);
                        self.mark(renderer::CAPTION, control);
                        self.renderer_row(p, ctx, state, control, out);
                    }
                    Line::RendererNotice => {
                        if let Some(text) = state.renderers.notice(renderer::stored()) {
                            let dim = TextStyle::new(Family::Body, 13.0, ctx.colours.dim())
                                .edge(ctx.colours.edge());
                            let at = Rect::new(list.x + 16.0 * k, y, list.w - 32.0 * k, row_h);
                            p.text_in(&dim, at, Align::Left, &text);
                            // Marked by its words, so a run can read what the page says.
                            self.mark(&text, at);
                        }
                    }
                    Line::SmoothAnimation => {
                        let at = Rect::new(list.x + 16.0 * k, y, list.w * 0.5, row_h);
                        p.text_in(&label, at, Align::Left, SMOOTH_CAPTION);
                        let control =
                            Rect::new(list.x + list.w * 0.55, y + 3.0 * k, list.w * 0.42, 28.0 * k);
                        self.mark(SMOOTH_CAPTION, control);
                        let on = self.options_page.smooth_animation;
                        if let Some(on) = kit::checkbox(p, ctx, control, on) {
                            out.settings
                                .push(("smooth-animation".into(), on.to_string()));
                        }
                        if at.intersect(&list).is_some_and(|at| ctx.over(&at)) {
                            self.tip =
                                Some((SMOOTH_CAPTION.to_owned(), vec![SMOOTH_NOTE.to_owned()]));
                        }
                    }
                    Line::Scale => {
                        p.text_in(
                            &label,
                            Rect::new(list.x + 16.0 * k, y, list.w * 0.5, row_h),
                            Align::Left,
                            SCALE_CAPTION,
                        );
                        let control =
                            Rect::new(list.x + list.w * 0.55, y + 3.0 * k, list.w * 0.42, 28.0 * k);
                        self.mark(SCALE_CAPTION, control);
                        let names: Vec<&str> = SCALES.iter().map(|(n, _)| *n).collect();
                        let now = self.options_page.scale;
                        let at = SCALES.iter().position(|(_, v)| (v - now).abs() < 0.01);
                        if let Some(i) = kit::radios(p, ctx, control, &names, at) {
                            out.settings
                                .push(("scale".into(), format!("{:.2}", SCALES[i].1)));
                        }
                    }
                }
            }
            y += row_h;
        }
        p.list.pop_clip();
        let defaults = Rect::new(
            area.x + 12.0 * k,
            area.bottom() - 34.0 * k,
            140.0 * k,
            30.0 * k,
        );
        if self.button(p, ctx, defaults, "Defaults", true) {
            for (name, value) in sheet::defaults(PageId::Client, Interface::Horizon) {
                set_preference(name, PrefValue::from(value), out);
            }
        }
    }

    /// One Client page row's control, and what a change to it asks for.
    fn client_row(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        row: &Row,
        control: Rect,
        #[cfg(feature = "hifi")] state: &GameState,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        match row.value {
            Value::Check(name) => {
                if let Some(PrefValue::Bool(on)) = store::inq_value(name) {
                    // An effect whose prerequisite is off keeps its tick, greyed, and cannot be
                    // changed until it can draw.
                    #[cfg(feature = "hifi")]
                    if effect_box(row).is_some() && effect_greyed(name, state) {
                        kit::checkbox_greyed(p, control, on);
                        return;
                    }
                    if let Some(on) = kit::checkbox(p, ctx, control, on) {
                        set_preference(name, PrefValue::Bool(on), out);
                    }
                }
            }
            Value::Sound { on, volume } => {
                // The sound's switch is "on" when the preference says the sound is on.
                if let Some(PrefValue::Bool(lit)) = store::inq_value(on) {
                    let cb = Rect::new(control.x, control.y, 28.0 * k, control.h);
                    if let Some(lit) = kit::checkbox(p, ctx, cb, lit) {
                        set_preference(on, PrefValue::Bool(lit), out);
                    }
                }
                if let Some(PrefValue::Float(v)) = store::inq_value(volume) {
                    let s = Rect::new(
                        control.x + 36.0 * k,
                        control.y,
                        control.w - 36.0 * k,
                        control.h,
                    );
                    if let Some(v) = kit::slider(p, ctx, s, v, 0.0, 1.0) {
                        set_preference(volume, PrefValue::Float(v), out);
                    }
                }
            }
            Value::Slider(name) => {
                let (lo, hi) = self.range_of(name);
                match store::inq_value(name) {
                    Some(PrefValue::Float(v)) => {
                        if let Some(v) = kit::slider(p, ctx, control, v, lo, hi) {
                            set_preference(name, PrefValue::Float(v), out);
                        }
                    }
                    Some(PrefValue::Int(v)) => {
                        #[allow(clippy::cast_precision_loss)]
                        let f = v as f32;
                        if let Some(f) = kit::slider(p, ctx, control, f, lo, hi) {
                            let v = dereth_primitives::num::to_i32(f.round());
                            set_preference(name, PrefValue::Int(v), out);
                        }
                    }
                    _ => {}
                }
            }
            Value::Menu(name) => {
                let Some(PrefValue::Int(v)) = store::inq_value(name) else {
                    return;
                };
                let choices = self.choices_of(name);
                if choices.is_empty() {
                    return;
                }
                let names: Vec<&str> = choices.iter().map(|(_, n)| n.as_str()).collect();
                if name == dereth_client_contract::options::interface::INTERFACE {
                    self.interface_row(p, ctx, control, &choices, v, out);
                    return;
                }
                let at = choices.iter().position(|(cv, _)| *cv == v).unwrap_or(0);
                if let Some(i) = self.dropdown(p, ctx, name, control, &names, at) {
                    let chosen = choices[i].0;
                    if row.confirm_change {
                        // A screen size is tried first and kept only when the player says so,
                        // as the other interfaces' pages ask.
                        let packed = chosen.cast_unsigned();
                        out.requests.push(UiRequest::Resolution(
                            dereth_client_contract::resolution::ResolutionAction::Begin {
                                size: (packed >> 16, packed & 0xffff),
                                policy:
                                    dereth_client_contract::resolution::ResolutionPolicy::Modern,
                                persist: false,
                            },
                        ));
                    } else {
                        set_preference(name, PrefValue::Int(chosen), out);
                    }
                }
            }
            _ => {}
        }
    }

    /// The Renderer row: the renderers this build can create, showing the one the next start
    /// comes up on. A choice is the `Renderer=` preference, kept by the save at exit and read at
    /// the next start; the device is not remade.
    fn renderer_row(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        control: Rect,
        out: &mut Outcome,
    ) {
        let offered = &state.renderers.offered;
        let names: Vec<&str> = offered.iter().map(|c| c.label()).collect();
        let next = state.renderers.next(renderer::stored());
        let at = offered.iter().position(|c| Some(*c) == next).unwrap_or(0);
        if let Some(i) = self.dropdown(p, ctx, renderer::RENDERER, control, &names, at) {
            set_preference(
                renderer::RENDERER,
                PrefValue::Int(renderer::value(offered[i])),
                out,
            );
        }
    }

    /// The Interface row: the interface is chosen here and switched to only on Apply, since the
    /// switch replaces this whole interface.
    fn interface_row(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        control: Rect,
        choices: &[(i32, String)],
        current: i32,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let names: Vec<&str> = choices.iter().map(|(_, n)| n.as_str()).collect();
        let pending = self
            .options_page
            .pending_interface
            .filter(|v| *v != current);
        let shown = pending.unwrap_or(current);
        let at = choices.iter().position(|(cv, _)| *cv == shown).unwrap_or(0);
        let apply_w = 80.0 * k;
        let menu = Rect::new(
            control.x,
            control.y,
            control.w - apply_w - 8.0 * k,
            control.h,
        );
        let key = dereth_client_contract::options::interface::INTERFACE;
        if let Some(i) = self.dropdown(p, ctx, key, menu, &names, at) {
            self.options_page.pending_interface = Some(choices[i].0);
        }
        let apply = Rect::new(menu.right() + 8.0 * k, control.y, apply_w, control.h);
        self.mark("Apply", apply);
        if kit::button(p, ctx, apply, "Apply", pending.is_some()) {
            if let Some(v) = pending {
                set_preference(
                    dereth_client_contract::options::interface::INTERFACE,
                    PrefValue::Int(v),
                    out,
                );
            }
            self.options_page.pending_interface = None;
        }
    }

    /// A slider's ends: the registered preference's, else the shared table's.
    fn range_of(&self, name: &str) -> (f32, f32) {
        self.options
            .iter()
            .find(|o| o.name == name)
            .and_then(|o| o.range)
            .unwrap_or_else(|| sheet::preference_range(name))
    }

    /// A menu's choices: the registered preference's, in the game's words, else the store's own
    /// list (the display's modes, the interface choice).
    fn choices_of(&self, name: &str) -> Vec<(i32, String)> {
        if let Some(o) = self.options.iter().find(|o| o.name == name) {
            if !o.choices.is_empty() {
                return o.choices.clone();
            }
        }
        store::choice_rows(name)
            .unwrap_or_default()
            .into_iter()
            .map(|c| (c.value, store::choice_caption(&c.label).to_owned()))
            .collect()
    }

    // -----------------------------------------------------------------------------------------
    // Key bindings
    // -----------------------------------------------------------------------------------------

    fn key_bindings(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        area: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let label = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let Some(view) = state.key_bindings.as_ref() else {
            p.text(
                &dim,
                area.x + 8.0 * k,
                area.y + 8.0 * k,
                "The key bindings are not loaded.",
            );
            return;
        };
        // The groups the rows fall in, in their order.
        let groups: Vec<usize> = (0..KEY_GROUPS.len())
            .filter(|g| view.rows.iter().any(|r| r.group == *g))
            .collect();
        if groups.is_empty() {
            return;
        }
        let names: Vec<&str> = groups.iter().map(|g| KEY_GROUPS[*g]).collect();
        let at = groups
            .iter()
            .position(|g| *g == self.options_page.key_group)
            .unwrap_or(0);
        let picker = Rect::new(area.x + 8.0 * k, area.y, 260.0 * k, 28.0 * k);
        self.mark("Key Group", picker);
        if let Some(i) = self.dropdown(p, ctx, "Key Group", picker, &names, at) {
            self.options_page.key_group = groups[i];
            self.options_page.scroll[tab::KEYS] = 0.0;
        } else {
            self.options_page.key_group = groups[at];
        }
        let group = self.options_page.key_group;
        let rows: Vec<_> = view.rows.iter().filter(|r| r.group == group).collect();
        let row_h = 30.0 * k;
        let list = Rect::new(area.x, area.y + 36.0 * k, area.w, area.h - 80.0 * k);
        #[allow(clippy::cast_precision_loss)]
        let content = row_h * rows.len() as f32;
        self.mark("list", list);
        let offset = kit::scroll(
            p,
            ctx,
            list,
            content,
            &mut self.options_page.scroll[tab::KEYS],
        );
        p.list.push_clip(list);
        let mut y = list.y - offset;
        let key_w = (list.w * 0.55 - 16.0 * k) / 3.0;
        let capturing = view.capture.is_some();
        for row in rows {
            if y + row_h >= list.y && y <= list.bottom() {
                let style = if row.changed { &label } else { &dim };
                p.text_in(
                    style,
                    Rect::new(list.x + 12.0 * k, y, list.w * 0.42, row_h),
                    Align::Left,
                    &row.caption,
                );
                for slot in 0..KEY_SLOTS {
                    let r = Rect::new(
                        list.x
                            + list.w * 0.44
                            + (key_w + 4.0 * k) * f32::from(u8::try_from(slot).unwrap_or(0)),
                        y + 2.0 * k,
                        key_w,
                        row_h - 4.0 * k,
                    );
                    self.mark(&format!("{}#{slot}", row.caption), r);
                    let name = row.keys.get(slot).map_or("", String::as_str);
                    if self.button(p, ctx, r, name, !capturing) {
                        out.key_requests.push(KeyRequest::Capture {
                            map: row.map,
                            action: row.action,
                            slot: (slot < row.keys.len()).then_some(slot),
                        });
                    }
                    // A right click takes the key off the action.
                    if !capturing
                        && slot < row.keys.len()
                        && ctx.input.hover(&r)
                        && ctx.input.pressed[1]
                    {
                        ctx.input.pressed[1] = false;
                        out.key_requests.push(KeyRequest::Clear {
                            map: row.map,
                            action: row.action,
                            slot,
                        });
                    }
                }
            }
            y += row_h;
        }
        p.list.pop_clip();
        let defaults = Rect::new(
            area.x + 12.0 * k,
            area.bottom() - 34.0 * k,
            180.0 * k,
            30.0 * k,
        );
        if self.button(p, ctx, defaults, "Restore Defaults", !capturing) {
            out.key_requests.push(KeyRequest::RestoreDefaults);
        }
        if let Some(why) = &view.refused {
            p.text_in(
                &dim,
                Rect::new(
                    defaults.right() + 12.0 * k,
                    defaults.y,
                    area.w - 220.0 * k,
                    defaults.h,
                ),
                Align::Left,
                why,
            );
        }
    }

    // -----------------------------------------------------------------------------------------
    // Controls
    // -----------------------------------------------------------------------------------------

    /// The camera: how the movement keys move the character, which way the pointer turns it, its
    /// speeds and limits, and how quickly it comes round behind the character; then the pad.
    fn controls_options(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        area: Rect,
        out: &mut Outcome,
    ) {
        use dereth_client_runtime::orbit::MovementMode;
        let k = p.scale;
        let label = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let row_h = 28.0 * k;
        let row = |i: f32| {
            let y = area.y + 8.0 * k + i * row_h;
            (
                Rect::new(area.x + 16.0 * k, y, area.w * 0.5, row_h),
                Rect::new(area.x + area.w * 0.55, y + 2.0 * k, area.w * 0.42, 26.0 * k),
            )
        };
        let orbit = self.options_page.orbit;
        let (l, c) = row(0.0);
        self.mark("Movement", c);
        p.text_in(&label, l, Align::Left, "Movement");
        let schemes = [MovementMode::Camera, MovementMode::Character];
        let at = schemes
            .iter()
            .position(|m| *m == orbit.movement)
            .unwrap_or(0);
        let names = ["Camera-based", "Character-based"];
        if let Some(i) = self.dropdown(p, ctx, "Movement", c, &names, at) {
            out.settings.push((
                "movement".into(),
                crate::options::movement_word(schemes[i]).into(),
            ));
        }
        for (i, (caption, name, on)) in [
            ("Reverse Horizontal Turning", "reverse-x", orbit.reverse_x),
            ("Reverse Vertical Turning", "reverse-y", orbit.reverse_y),
            ("Sidestep Instead of Running", "sidestep", orbit.sidestep),
        ]
        .into_iter()
        .enumerate()
        {
            #[allow(clippy::cast_precision_loss)]
            let (l, c) = row(1.0 + i as f32);
            self.mark(caption, c);
            p.text_in(&label, l, Align::Left, caption);
            if let Some(v) = kit::checkbox(p, ctx, c, on) {
                out.settings.push((name.into(), v.to_string()));
            }
        }
        // The camera's own speeds and limits, each a slider over its range, its value beside it.
        {
            use dereth_client_runtime::orbit::limits;
            let value_style =
                TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
            let sliders: [CameraSlider; 6] = [
                (
                    "Mouse Turning Speed",
                    "mouse-turn",
                    orbit.mouse_turn,
                    limits::MOUSE_TURN,
                    |v| format!("{:.0}%", v / 0.005 * 100.0),
                    false,
                ),
                (
                    "Key Turning Speed",
                    "key-turn",
                    orbit.key_turn,
                    limits::KEY_TURN,
                    |v| format!("{:.0}%", v / 1.6 * 100.0),
                    false,
                ),
                (
                    "Lowest Tilt",
                    "tilt-min",
                    orbit.pitch_min,
                    limits::PITCH_MIN,
                    |v| format!("{:.0}°", v.to_degrees()),
                    false,
                ),
                (
                    "Highest Tilt",
                    "tilt-max",
                    orbit.pitch_max,
                    limits::PITCH_MAX,
                    |v| format!("{:.0}°", v.to_degrees()),
                    false,
                ),
                (
                    "Camera Height",
                    "camera-height",
                    orbit.height,
                    limits::HEIGHT,
                    |v| format!("{v:+.1} m"),
                    false,
                ),
                // The time it takes, as a speed: quicker to the right, at once at the right end.
                (
                    "Camera Recentre Speed",
                    "camera-recentre",
                    orbit.recentre,
                    limits::RECENTRE,
                    |v| {
                        if v < 0.005 {
                            "Instant".to_owned()
                        } else {
                            format!("{v:.2} s")
                        }
                    },
                    true,
                ),
            ];
            for (i, (caption, name, v, (lo, hi), show, quicker_right)) in
                sliders.into_iter().enumerate()
            {
                #[allow(clippy::cast_precision_loss)]
                let (l, c) = row(4.0 + i as f32);
                self.mark(caption, c);
                p.text_in(&label, l, Align::Left, caption);
                let track = Rect::new(c.x, c.y, c.w - 64.0 * k, c.h);
                let flip = |t: f32| if quicker_right { 1.0 - t } else { t };
                if let Some(t) = kit::track_slider(p, ctx, track, flip((v - lo) / (hi - lo))) {
                    out.settings
                        .push((name.into(), (lo + flip(t) * (hi - lo)).to_string()));
                }
                p.text_in(
                    &value_style,
                    Rect::new(track.right() + 8.0 * k, c.y, 56.0 * k, c.h),
                    Align::Right,
                    &show(v),
                );
            }
        }
        // The minimap: north up (the arrow showing the character's facing), or turning with them.
        let (l, c) = row(10.0);
        self.mark("Lock Minimap North", c);
        p.text_in(&label, l, Align::Left, "Lock Minimap North");
        if let Some(v) = kit::checkbox(p, ctx, c, !self.options_page.minimap_rotates) {
            out.settings
                .push(("minimap-rotates".into(), (!v).to_string()));
        }
        // The pad, under a heading of its own.
        let heading =
            TextStyle::new(Family::Heading, 18.4, ctx.colours.heading()).edge(ctx.colours.edge());
        let (l, _) = row(11.2);
        p.text_in(&heading, l, Align::Left, "Gamepad (Experimental)");
        let pad = self.options_page.pad;
        let (l, c) = row(12.2);
        self.mark("Gamepad Mode", c);
        p.text_in(&label, l, Align::Left, "Gamepad Mode");
        if let Some(v) = kit::checkbox(p, ctx, c, pad.enabled) {
            out.settings.push(("gamepad".into(), v.to_string()));
        }
        {
            let (l, c) = row(13.2);
            self.mark("Movement in Gamepad Mode", c);
            p.text_in(&label, l, Align::Left, "Movement in Gamepad Mode");
            let schemes = [MovementMode::Camera, MovementMode::Character];
            let at = schemes.iter().position(|m| *m == pad.movement).unwrap_or(0);
            let names = ["Camera-based", "Character-based"];
            if let Some(i) = self.dropdown(p, ctx, "Movement in Gamepad Mode", c, &names, at) {
                out.settings.push((
                    "gamepad-movement".into(),
                    crate::options::movement_word(schemes[i]).into(),
                ));
            }
        }
        for (i, (caption, name, value, (lo, hi))) in [
            (
                "Stick Dead Zone",
                "gamepad-dead-zone",
                pad.dead_zone,
                crate::pad::DEAD_ZONE_RANGE,
            ),
            (
                "Camera Speed",
                "gamepad-camera-speed",
                pad.camera_speed,
                crate::pad::CAMERA_SPEED_RANGE,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            #[allow(clippy::cast_precision_loss)]
            let (l, c) = row(14.2 + i as f32);
            self.mark(caption, c);
            p.text_in(&label, l, Align::Left, caption);
            if let Some(v) = kit::slider(p, ctx, c, value, lo, hi) {
                out.settings.push((name.into(), format!("{v:.2}")));
            }
        }
    }
}

/// A camera setting's slider: its caption, its settings name, its value, its range, how its
/// value is shown beside it, and whether its value runs the other way, from the top of its range
/// at the left (a time shown as a speed).
type CameraSlider = (
    &'static str,
    &'static str,
    f32,
    (f32, f32),
    fn(f32) -> String,
    bool,
);

/// A line of a page: a heading, or a row under it.
#[derive(Debug, Clone, Copy)]
enum Line {
    Heading(&'static str),
    Row(&'static Row),
    /// A line of plain words under a heading or a row: the experimental effects' warning, or why
    /// an effect's box is greyed.
    #[cfg(feature = "hifi")]
    Notice(&'static str),
    /// Where the experimental effects stand on the device, when there is something to say.
    #[cfg(feature = "hifi")]
    EffectsStatus,
    /// This interface's scale, under the interface choice.
    Scale,
    /// Whether bodies are drawn between their animations' keyframes, last under the graphics
    /// quality.
    SmoothAnimation,
    /// The renderer the next start comes up on.
    Renderer,
    /// The renderer in use now, and the one the next start takes when that is another.
    RendererNotice,
    /// The heading over a log window tab's name and its chat window's filter, by tab.
    TabHeading(usize),
    /// A log window tab's name, in a text box.
    TabName(usize),
    /// An opacity of the log window, active or inactive: the docked window's (`None`) or a tab's
    /// popped out.
    Opacity(Option<usize>, bool),
}

/// The caption of an opacity's row: the docked window's, or a tab's popped out.
const fn opacity_caption(slot: Option<usize>, active: bool) -> &'static str {
    match (slot, active) {
        (None, true) => "Active Opacity",
        (None, false) => "Inactive Opacity",
        (Some(_), true) => "Undocked Opacity (Active)",
        (Some(_), false) => "Undocked Opacity (Inactive)",
    }
}

/// What an opacity's slider is marked by: its caption, and a popped-out tab's number after it.
fn opacity_mark(slot: Option<usize>, active: bool) -> String {
    let caption = opacity_caption(slot, active);
    slot.map_or_else(|| caption.to_owned(), |s| format!("{caption} {}", s + 1))
}

/// The headings of the Chat page's tabs, in the log window's order.
const TAB_HEADINGS: [&str; 5] = ["Main Tab", "Tab #2", "Tab #3", "Tab #4", "Tab #5"];

/// The id of the first tab name's text box; the others follow it.
const TAB_NAME_FIELD: u32 = 90;

/// The Chat page's lines: the log window's heading with its two opacities, then each chat
/// window's heading is its tab's, with the tab's name and (but for the main tab) its two
/// opacities popped out under it before its filter's groups.
fn chat_lines(state: &GameState) -> Vec<Line> {
    let lines = sheet_lines(PageId::Chat, state);
    let mut out = Vec::with_capacity(lines.len() + 16);
    out.push(Line::Heading("Chat Window"));
    out.push(Line::Opacity(None, true));
    out.push(Line::Opacity(None, false));
    for (n, line) in lines.iter().enumerate() {
        let slot = match (line, lines.get(n + 1)) {
            (Line::Heading(_), Some(Line::Row(row))) => match row.value {
                Value::Filter { window, .. } => {
                    crate::ui::chat::WINDOWS.iter().position(|w| *w == window)
                }
                _ => None,
            },
            _ => None,
        };
        match slot {
            Some(slot) => {
                out.push(Line::TabHeading(slot));
                out.push(Line::TabName(slot));
                if slot > 0 {
                    out.push(Line::Opacity(Some(slot), true));
                    out.push(Line::Opacity(Some(slot), false));
                }
            }
            None => out.push(*line),
        }
    }
    out
}

/// The game's settings this interface leaves off its pages: what its own HUD, camera and chat
/// do their own way, so the setting would change nothing here.
const HIDDEN_ROWS: [&str; 10] = [
    "Keep Combat Targets in View",
    "Vivid Targeting Indicator",
    "Display Tooltips",
    "Show Coordinates By the Radar",
    "Side By Side Vitals",
    "Display Spell Durations",
    "Stay in Chat Mode After Sending a Message",
    "Chat Font",
    "Chat Font Size",
    "Advanced Combat Interface",
];

/// The game camera's heading; this interface has its own camera, with its own settings.
const HIDDEN_HEADINGS: [&str; 1] = ["Camera and Mouse"];

/// Whether this interface leaves `row` off its pages: a row of [`HIDDEN_ROWS`], or a chat window's
/// opacity, which its chat does not draw with.
pub(crate) fn hidden_row(row: &Row) -> bool {
    HIDDEN_ROWS.contains(&row.caption) || matches!(row.value, Value::Opacity(_))
}

/// A page's headings and rows as this interface shows them, leaving out what the world's era does
/// not have and what this interface does its own way.
fn sheet_lines(page: PageId, state: &GameState) -> Vec<Line> {
    let mut out = Vec::new();
    // The renderer choice comes last on the Client page, before the experimental effects.
    #[cfg_attr(not(feature = "hifi"), allow(unused_mut))]
    let mut renderer_lines = (page == PageId::Client && state.renderers.shown()).then_some([
        Line::Heading(renderer::HEADING),
        Line::Renderer,
        Line::RendererNotice,
    ]);
    for (heading, rows) in sheet::headings_for(page, Interface::Horizon) {
        if HIDDEN_HEADINGS.contains(&heading.title) {
            continue;
        }
        let rows: Vec<_> = rows
            .into_iter()
            .filter(|r| r.needs.met(state.era.as_ref()) && !hidden_row(r))
            .collect();
        if rows.is_empty() {
            continue;
        }
        let smooth = page == PageId::Client && heading.title == SMOOTH_HEADING;
        #[cfg(feature = "hifi")]
        if heading.title == fidelity::HEADING {
            out.extend(renderer_lines.take().into_iter().flatten());
        }
        out.push(Line::Heading(heading.title));
        #[cfg(feature = "hifi")]
        if heading.title == fidelity::HEADING {
            out.push(Line::Notice(fidelity::WARNING));
            if effects_status(state).is_some() {
                out.push(Line::EffectsStatus);
            }
        }
        for row in rows {
            out.push(Line::Row(row));
            if matches!(row.value, Value::Menu(n) if n == dereth_client_contract::options::interface::INTERFACE)
            {
                out.push(Line::Scale);
            }
            #[cfg(feature = "hifi")]
            if let Some(why) = effect_box(row)
                .filter(|_| state.hifi.offered())
                .and_then(|name| effect_blocked(name, state))
            {
                out.push(Line::Notice(why.reason()));
            }
        }
        if smooth {
            out.push(Line::SmoothAnimation);
        }
    }
    out.extend(renderer_lines.into_iter().flatten());
    out
}

/// The preference of `row` when it is one of the experimental effects' boxes.
#[cfg(feature = "hifi")]
fn effect_box(row: &Row) -> Option<&'static str> {
    match row.value {
        Value::Check(name) if fidelity::find(name).is_some() => Some(name),
        _ => None,
    }
}

/// Whether the box `name` is ticked.
#[cfg(feature = "hifi")]
fn ticked(name: &str) -> bool {
    matches!(store::inq_value(name), Some(PrefValue::Bool(true)))
}

/// Why the effect `name` draws nothing however it is ticked: what it needs is off, or the
/// device does not trace rays.
#[cfg(feature = "hifi")]
fn effect_blocked(name: &str, state: &GameState) -> Option<fidelity::Blocked> {
    fidelity::blocked(name, ticked, state.hifi.rays)
}

/// Whether the effect's box `name` is shown greyed, its tick kept but not changeable: the client
/// draws with a renderer the effects are not offered on (the status line says how to get them),
/// or what the effect needs is off.
#[cfg(feature = "hifi")]
fn effect_greyed(name: &str, state: &GameState) -> bool {
    !state.hifi.offered() || effect_blocked(name, state).is_some()
}

/// The experimental effects' status line, when there is something to say.
#[cfg(feature = "hifi")]
fn effects_status(state: &GameState) -> Option<String> {
    // Where the renderer choice above leaves wgpu, the renderer the effects draw on.
    fidelity::Availability {
        wgpu_next: fidelity::wgpu_next(&state.renderers, renderer::stored()),
        ..state.hifi.clone()
    }
    .status(fidelity::OPTIONS.iter().any(|o| ticked(o.name)))
}

/// Set a profile preference: shown at once, applied and kept by the game's own preference chain.
fn set_preference(name: &'static str, value: PrefValue, out: &mut Outcome) {
    store::set_value(name, value.clone());
    out.requests.push(UiRequest::SetPreference(name, value));
}

#[cfg(test)]
mod hidden_tests {
    //! Behaviour: none (this client's own settings window)
    use super::*;

    #[test]
    fn every_setting_left_off_is_one_the_pages_have_so_none_comes_back_renamed() {
        let pages = [PageId::Character, PageId::Chat, PageId::Client];
        for caption in HIDDEN_ROWS {
            assert!(
                pages
                    .iter()
                    .flat_map(|p| sheet::rows_for(*p, Interface::Horizon))
                    .any(|r| r.caption == caption),
                "{caption}"
            );
        }
        for title in HIDDEN_HEADINGS {
            assert!(
                pages
                    .iter()
                    .flat_map(|p| sheet::headings_for(*p, Interface::Horizon))
                    .any(|(h, _)| h.title == title),
                "{title}"
            );
        }
    }
}
