//! `TitlesPanel` — the Titles tab: every title the character has earned, the one currently
//! worn, and the button that changes it.
//!
//! The panel has a registered element type ([`crate::element_types::ty::CHARACTER_TITLE`]), a
//! shipped layout and a descriptive row in [`crate::panels::catalogue`]; a catalogue row is not a
//! panel, and this is the behaviour behind it.
//!
//! # The title list
//!
//! `0x0029 Social_CharacterTitleTable` carries both the display title and the title list, and
//! both must be kept: `dereth_client_shell::hud` keeps the list and it reaches this module through
//! [`GameView::character_titles`].
//!
//! # The four functions, and where they live here
//!
//! | client | here |
//! |---|---|
//! | post-init — three child lookups | [`TitlesPanel::post_init`] |
//! | refresh — display text, flush, one title insert per id, the button update | [`TitlesPanel::update`] |
//! | title insert — name lookup, sorted insert, text + `0x1000008E` | `TitlesPanel::rebuild` |
//! | button update — state `1` / state `0xD` on the button | `TitlesPanel::update_buttons` |
//! | element-message handler — `1` on `0x10000535`, `4`/`0x43` | [`TitlesPanel::on_element_message`] |
//!
//! # The list, out of the shipped tree
//!
//! The `TitlesPanel` instance in `classic_gameplay` is **`0x10000539`**, on the character
//! page beside `AttributesPanel` and `SkillsPanel`. Its list box `0x10000532` carries exactly one row
//! template, `(0x2100005E, 0x10000536)`, whose text child is `0x10000537` — which is why
//! `0x10000537` resolves nowhere in the live tree until a row exists. [measured on the shipped
//! layout]
//!
//! `catalogue::CHARACTER_TITLE_TEMPLATES` names `0x10000537`, the template's **text child**,
//! rather than `0x10000536`, the template's root. Both are "a root or a root's child elsewhere in
//! the same layout", so the catalogue's own acceptance test is satisfied either way; the id
//! actually instantiated is the one in the list box's template list and is read from the tree
//! here rather than named.
//!
//! # Sorted by **name**, not by id
//!
//! The client walks the existing rows and reads each one's `0x10000537`
//! text back and `wcscmp`s it against the incoming name, returning
//! the first row that sorts after it. So the list is alphabetical by the *resolved display
//! string*, and a title whose name does not resolve is not in it at all — the title insert
//! returns false before it creates anything when the title-name lookup fails, and when the id
//! is `0`.
//!
//! # The pick is two gestures, not one
//!
//! A click on a row **selects**; it sends nothing. The client's only
//! sending arm is message `1` from `0x10000535` — the *"Set as Display Title"*
//! button — which reads `0x1000008E` off the currently selected row and sends the
//! set-display-character-title event. Between the two, the client puts the button in state `1` only while a row is selected whose
//! title differs from the one already worn, and in state `0x0D` (disabled) otherwise.
//!
//! # Where the selection comes from — the *list box*, and nowhere else
//!
//! The client has **no `0x1C` arm at all**: its whole
//! body is the `1` arm for `0x10000535` and the button update on `4` or `0x43`. The
//! selecting is the list box's — it answers `0x1C` with `p1 == 7` (and `10`) by selecting the
//! item under the mouse — and the panel then reads the selected item straight back off the
//! list box.
//!
//! **`dereth_ui::widgets::listbox` carries both press sites and the selected-item write whole**,
//! so this panel has exactly its two arms, and [`selected_index`] is
//! the button update's own walk rather than a mirror this panel maintains — so a selection made by
//! anything else (a keyboard, a programmatic selection, another panel) moves the button too.
//!
//! The one wire that makes the `4` reachable is `GamePlayScreen::panel_messages`, which forwards
//! it keyed on [`TITLE_LIST`]; `0x04` is raised by every list box in the tree and a blanket
//! forward would put a great many of them through a fan-out that wants one.

use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::panels::listbox::ListBoxWidget;
use crate::view::{CharacterTitles, GameView, UiRequest};

/// The shipped `TitlesPanel` instance — the panel element itself.
pub const PANEL: ElementId = ElementId(0x1000_0539);
/// The display-title text, looked up under the panel.
pub const DISPLAY_TITLE_TEXT: ElementId = ElementId(0x1000_052F);
/// The title list box, looked up under the panel.
pub const TITLE_LIST: ElementId = ElementId(0x1000_0532);
/// The display button, looked up under the panel, and the one element id the element-message
/// handler sends on.
pub const DISPLAY_BUTTON: ElementId = ElementId(0x1000_0535);
/// The row template's text child, looked up under each new row.
pub const ROW_TEXT: u32 = 0x1000_0537;
/// The list box template a row is created from — the list carries exactly one template.
pub const ROW_TEMPLATE: usize = 0;
/// `Enum` attribute `0x1000008E` (`UI_Social_CharacterTitleID`) — the title id a row stands for,
/// written by the title insert and read back by the button update and by the send arm.
pub const ATTR_TITLE_ID: u32 = 0x1000_008E;

/// The client's failed-title-lookup arm: the display text is the literal
/// `L"Unknown"`, not an empty field.
pub const UNKNOWN_TITLE: &str = "Unknown";

/// The selected item's index, read off the live list box, as an index into
/// [`TitlesPanel::rows`].
///
/// The client does this walk itself — finding the selected item among the list's items — so the
/// selection this panel shows is the **element's** and not a mirror of its
/// own.
fn selected_index(ui: &UiSystem, list: ElemHandle) -> Option<usize> {
    ui.node(list)
        .and_then(|n| n.behaviour.as_ref())
        .and_then(|b| (**b).as_any())
        .and_then(|a| a.downcast_ref::<dereth_ui::widgets::listbox::ListBox>())
        .and_then(|l| l.selected)
}

/// Whether the Display button is currently offered — the observable half of
/// [`TitlesPanel::update_buttons`], read back off the element rather than off a mirror.
///
/// A free function taking the handle rather than a method taking `&self` and `&UiSystem`, because
/// a caller that holds the panel through `&App` cannot then borrow the `UiSystem` out of the same
/// `App`; the handle is `Copy` and the borrow ends with it.
#[must_use]
pub fn button_enabled(ui: &UiSystem, button: ElemHandle) -> bool {
    ui.node(button)
        .is_some_and(|n| n.state != dereth_ui::widgets::button::state::DISABLED)
}

/// One drawn row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleRow {
    /// The `0x1000008E` attribute's value.
    pub id: u32,
    /// The title-name lookup's answer, which is also the sort key.
    pub name: String,
    pub element: ElemHandle,
}

/// `TitlesPanel`, bound to a live tree.
#[derive(Debug, Default)]
pub struct TitlesPanel {
    /// The title list box.
    pub list: Option<ListBoxWidget>,
    /// The display-title text.
    pub display_text: Option<ElemHandle>,
    /// The display button.
    pub button: Option<ElemHandle>,
    /// The rows on screen, in list order — i.e. alphabetically.
    pub rows: Vec<TitleRow>,
    /// The list box's selected item, as an index into [`Self::rows`].
    pub selected: Option<usize>,
    /// The snapshot [`Self::update`] last drew, so an unchanged frame redraws nothing.
    last: Option<CharacterTitles>,
    /// How many times [`Self::update`] rebuilt. **Three states, not two**: this is what separates
    /// "rebuilt and the character has earned nothing" from "never ran", which is the difference
    /// between a new character and an unwired panel.
    pub rebuilds: u32,
    /// Ids the refresh walked past because the title-name lookup refused them — id `0`, or a
    /// token the shipped `EnumMapper`/`StringTable` pair does not carry. A denominator, so an
    /// empty list can say *why* it is empty.
    pub unresolved: u32,
}

impl TitlesPanel {
    /// The titles panel's post-init, minus the three notice registrations (title-table update,
    /// set display title, and add title) — this
    /// build has no notice bus at this seam and the same
    /// three writes arrive as the per-frame [`Self::update`] snapshot.
    ///
    /// `root` is the character page. The three children are looked up from the panel element
    /// itself when it is there, which is what the client looks them up under; falling back to `root`
    /// keeps a differently-rooted tree bindable instead of silently blank.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        *self = Self::default();
        let me = ui.get_child_recursive(root, PANEL).unwrap_or(root);
        self.display_text = ui.get_child_recursive(me, DISPLAY_TITLE_TEXT);
        self.button = ui.get_child_recursive(me, DISPLAY_BUTTON);
        self.list = ui
            .get_child_recursive(me, TITLE_LIST)
            .map(|h| ListBoxWidget::bind(ui, h));
    }

    /// True once the list box was found — the binding without which nothing can be drawn.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.list.is_some()
    }

    /// How many row templates the bound list carries. The title insert needs **one**; a list with
    /// none draws nothing and must be able to say so rather than look like a character who has
    /// earned no titles.
    #[must_use]
    pub fn templates(&self) -> usize {
        self.list.as_ref().map_or(0, |l| l.templates.len())
    }

    /// The title ids on screen, in list order — what a test asserts against instead of a pixel.
    #[must_use]
    pub fn shown(&self) -> Vec<u32> {
        self.rows.iter().map(|r| r.id).collect()
    }

    /// The titles panel's refresh, guarded on the snapshot.
    ///
    /// Returns whether the tree was rewritten.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let t = view.character_titles();
        if self.last.as_ref() == Some(&t) {
            return false;
        }
        // The display title's name goes into the display text, and the `L"Unknown"` literal on
        // failure. `display_title()` is the same lookup and answers `None` for the
        // same two reasons (`id == 0`, or the token missing).
        let shown = view
            .display_title()
            .unwrap_or_else(|| UNKNOWN_TITLE.to_owned());
        if let Some(e) = self.display_text.and_then(|h| ui.text_element_mut(h)) {
            e.set_text(&shown);
        }
        self.rebuild(ui, &t);
        self.update_buttons(ui, &t);
        self.last = Some(t);
        self.rebuilds += 1;
        true
    }

    /// The refresh's middle third: flush, clear the selection, then one title insert per id in
    /// the title list.
    fn rebuild(&mut self, ui: &mut UiSystem, t: &CharacterTitles) {
        self.rows.clear();
        self.selected = None;
        self.unresolved = 0;
        let Some(mut list) = self.list.take() else {
            return;
        };
        list.flush(ui);
        let rows = dereth_presentation::stats::title_rows(&t.titles);
        self.unresolved = u32::try_from(t.titles.len() - rows.len()).unwrap_or(u32::MAX);
        for (id, name) in &rows {
            let Some(h) = list.add_from_template(ui, ROW_TEMPLATE, None) else {
                continue;
            };
            let Some(text) = ui.get_child_recursive(h, ElementId(ROW_TEXT)) else {
                // The title insert returns false without writing the attribute when the text
                // child is missing, so the row exists and carries no identity. Counted, not
                // silently kept: a row with no `0x1000008E` can never be picked.
                self.unresolved += 1;
                continue;
            };
            if let Some(e) = ui.text_element_mut(text) {
                e.set_text(name);
            }
            ui.set_attribute_enum(h, ATTR_TITLE_ID, *id);
            let at = self.rows.len();
            self.rows.insert(
                at,
                TitleRow {
                    id: *id,
                    name: name.clone(),
                    element: h,
                },
            );
        }
        list.update_layout(ui);
        self.list = Some(list);
    }

    /// The character title panel's buttons update.
    ///
    /// State `1` for a different title, state `0x0D` (disabled) otherwise.
    ///
    /// The `1` arm is reached **only** when a row is selected *and* its `0x1000008E` differs from
    /// the display title; no selection, or the worn title re-selected, both fall to `0x0D`.
    fn update_buttons(&mut self, ui: &mut UiSystem, t: &CharacterTitles) {
        let Some(b) = self.button else { return };
        let selected = self.selected.and_then(|i| self.rows.get(i)).map(|r| r.id);
        let enabled = dereth_presentation::stats::can_set_title(selected, t.display, &t.titles);
        ui.set_state(
            b,
            if enabled {
                dereth_ui::widgets::button::state::NORMAL
            } else {
                dereth_ui::widgets::button::state::DISABLED
            },
        );
    }

    /// The titles panel's element-message handler, plus the list box's own selection
    /// arm — see the module note for why that one is here.
    ///
    /// On message `1` from `0x10000535`, with a row selected, read its `0x1000008E` enum attribute
    /// and send [`UiRequest::SetDisplayCharacterTitle`] with it. On `4` or `0x43`, run the button update.
    ///
    /// Returns true when the message was consumed.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        use dereth_ui::msg::element::id as msg;
        if m.id == msg::LIST_SELECTION_CHANGED || m.id == msg::LIST_ITEM_ACTIVATED {
            let Some(list) = self.list.as_ref().map(|l| l.handle) else {
                return false;
            };
            if list != m.source {
                return false;
            }
            // The button update re-reads the list box's selected index; this panel's own
            // `selected` is that read, cached for the `1` arm and for the tests.
            self.selected = selected_index(ui, list).filter(|i| *i < self.rows.len());
            self.update_buttons(ui, &view.character_titles());
            return true;
        }
        if m.id == msg::BUTTON_CLICKED && m.source_id == DISPLAY_BUTTON {
            return self.set_display_title(ui);
        }
        false
    }

    /// The `0x10000535` arm's body: read `0x1000008E` off the **selected** row and
    /// send it.
    ///
    /// The id comes off the element, not out of [`Self::rows`], because that is where the client
    /// reads it from and because a row whose attribute never got written must not be pickable —
    /// see [`Self::rebuild`]'s missing-text-child arm.
    fn set_display_title(&mut self, ui: &mut UiSystem) -> bool {
        let Some(row) = self.selected.and_then(|i| self.rows.get(i)) else {
            return false;
        };
        let Some(title_id) = crate::bind::attr_enum(ui, row.element, ATTR_TITLE_ID) else {
            return false;
        };
        ui.requests
            .emit(UiRequest::SetDisplayCharacterTitle { title_id });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **No datagram leaves this process.** Nothing in this module opens a socket; the send arm
    /// appends a [`UiRequest`] to the thread-local outbox and the host is what puts a message on
    /// the wire.
    ///
    /// Oracle: element ids from panel initialization, title insertion and
    /// element-message handling, pinned as literals.
    #[test]
    fn the_element_ids_are_the_ones_the_client_binds_and_reads() {
        assert_eq!(DISPLAY_TITLE_TEXT, ElementId(0x1000_052F));
        assert_eq!(TITLE_LIST, ElementId(0x1000_0532));
        assert_eq!(DISPLAY_BUTTON, ElementId(0x1000_0535));
        assert_eq!(ROW_TEXT, 0x1000_0537);
        assert_eq!(ATTR_TITLE_ID, 0x1000_008E);
        assert_eq!(ROW_TEMPLATE, 0);
        // `panels::rows` recovered the same attribute from the layout description independently; the two must
        // not drift apart.
        assert_eq!(
            crate::panels::rows::row_attribute("TitlesPanel"),
            Some(ATTR_TITLE_ID)
        );
        // And the catalogue agrees about the panel's own three bindings.
        let spec = crate::panels::catalogue::spec("TitlesPanel").expect("catalogued");
        assert_eq!(spec.ty, crate::element_types::ty::CHARACTER_TITLE);
        for id in [DISPLAY_TITLE_TEXT.0, TITLE_LIST.0, DISPLAY_BUTTON.0] {
            assert!(spec.children.iter().any(|c| c.id.0 == id), "{id:#010X}");
        }
    }

    /// An unbound panel answers **not-bound, 0 templates, 0 rebuilds**, rather than looking like a
    /// character who has earned no titles.
    #[test]
    fn an_unbound_panel_says_so() {
        let p = TitlesPanel::default();
        assert!(!p.bound());
        assert_eq!(p.templates(), 0);
        assert_eq!(p.rebuilds, 0);
        assert_eq!(p.unresolved, 0);
        assert!(p.shown().is_empty());
        assert_eq!(p.selected, None);
    }
}
