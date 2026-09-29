//! `ContractsPanel` — the **Contracts tab** of the toolbar's quest page: every contract the
//! character holds, its stage, and the Abandon button.
//!
//! Four pieces make it up, and all four must be connected:
//!
//! | piece | where |
//! |---|---|
//! | `0x0314` / `0x0315` decoders | `dereth_protocol::social`, received by `Hud::ui_event` |
//! | the contract tracker table | `dereth_client_model::quests` |
//! | the 322-entry contract table | `dereth_assets::tables`, decoded |
//! | `ContractsPanel` | an element type, a shipped layout, a `panels::catalogue` row and this module |
//!
//! A receiver whose field nothing reads is as good as no receiver; this module is the reader.
//!
//! # The shipped tree, measured
//!
//! The panel instance is **`0x100005D4`**, the fifth child of `<QUES>` `0x10000559` and the third
//! tab of its `Panel`: `tab_to_page` is
//! `{0x10000560 -> 0x10000563 (Journal), 0x10000561 -> 0x10000564 (Page List),
//! 0x100005D3 -> 0x100005D4 (Contracts)}`. Its list box `0x100005CF` carries exactly one row
//! template, `(0x21000069, 0x100005D7)`, whose two text children `0x100005D1` and `0x100005D2`
//! resolve nowhere in the live tree until a row exists — the same shape [`super::titles`]
//! documents. `panels::catalogue::CONTRACTS_TEMPLATES` names those two children rather than the
//! template root `0x100005D7`, which is what actually
//! instantiates. [measured on the shipped layout]
//!
//! # Six retail behaviours that are easy to get wrong
//!
//!
//! 1. **A repeat timer that has run out reads `"Available"`, not `"Done"`.**
//! 2. **A repeatable contract with no timer running also reads `"Available"`**, not an empty
//!    string.
//! 3. **Stage ≥ 4 with an empty progress description reads `"In Progress"`.**
//! 4. **An unresolvable position reads `"Indoors"`, not `"None"`.** `"None"` is the *timer*
//!    line's no-flag answer; the two are easily conflated.
//! 5. **Both coordinate axes subtract `0x400` and add `0.5`** — each is scaled by `0.1` and
//!    then `0.5` is added.
//!    `dereth_physics::landdefs::cellid_to_coordinates`, the numeric half of a *different* function
//!    (the cell-id to coordinate-string conversion), subtracts `0x100` from the north/south axis
//!    and adds nothing. The two do not agree and this panel uses its own.
//! 6. **The double click does nothing.** The client ignores the double-click check's answer
//!    and just refreshes the detail fields. Double-clicking an entry does **not** set a map or
//!    radar destination; the check only records the last clicked row and its time.
//!
//! # The seven functions, and where they live here
//!
//! | client | here |
//! |---|---|
//! | — seven child lookups, two notices, a subscription to global message `3` | [`ContractsPanel::post_init`] |
//! | — the tracker table joined to the contract table | `ContractsPanel::rebuild_contract_list` |
//! | — four comparators, between two listbox rebuilds | [`ContractsPanel::sort_contract_list`] |
//! | — keep the index, flush, one add each, restore | [`ContractsPanel::rebuild_contract_listbox`] |
//! | — one row from the template | [`ContractsPanel::add_contract_to_listbox`] |
//! | — the half-second redraw | [`ContractsPanel::refresh_contract_listbox`] |
//! | — the six detail fields for the selected row | [`ContractsPanel::update_buttons`] |
//! | — two sort buttons, Abandon, `4`, `0x43` | [`ContractsPanel::on_element_message`] |
//!
//! # What drives it, and why it is not a notice
//!
//! The client has three triggers. The contract-tracker-table and contract-tracker notices both
//! rebuild the contract list and then sort it. The panel's shown edge runs the same pair. Global
//! message `3` — the per-frame broadcast — runs the listbox refresh at most twice a second
//! **while the panel is visible**.
//!
//! This build has no notice bus at that seam, so [`ContractsPanel::update`] takes the two model
//! triggers as a change in the snapshot's own key (the contract ids, stages and names) and keeps
//! the visibility edge and the half-second throttle exactly as they are. The net effect is the
//! client's: a tab nobody has opened costs one `Vec` comparison a frame.
//!
//! # Shard safety
//!
//! **No datagram leaves this process.** The Abandon arm appends a
//! [`UiRequest::AbandonContract`] to the thread-local outbox; the host is what puts `0x0316` on
//! the wire.

use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::panels::listbox::ListBoxWidget;
use crate::view::{ContractEntry, GameView, UiRequest};

/// `<QUES>` — the quest page of `PanelStack`'s stack, shared with [`super::journal`] and
/// [`super::pagelist`].
pub const PAGE: ElementId = ElementId(0x1000_0559);
/// The `ContractsPanel` sub-panel — the panel itself. [measured on the shipped layout]
pub const PANEL: ElementId = ElementId(0x1000_05D4);
/// The **Contracts tab**, paired with [`PANEL`] by the shipped `0x2E` table.
pub const TAB: ElementId = ElementId(0x1000_05D3);

/// The contracts list box, a `ListBox` looked up inside the panel.
pub const CONTRACTS_BOX: ElementId = ElementId(0x1000_05CF);
/// The notes text — the contract's description.
pub const NOTES_TEXT: ElementId = ElementId(0x1000_05DE);
/// The progress text — the selected row's progress string.
pub const PROGRESS_TEXT: ElementId = ElementId(0x1000_05DF);
/// The contact text — the start or end NPC's name.
pub const CONTACT_TEXT: ElementId = ElementId(0x1000_05E0);
/// The contact location text — that NPC's position.
pub const CONTACT_LOC_TEXT: ElementId = ElementId(0x1000_05E1);
/// The area text — the contract's quest-area location.
pub const AREA_TEXT: ElementId = ElementId(0x1000_05E2);
/// The timed text — the quest-flag timer countdown.
pub const TIMED_TEXT: ElementId = ElementId(0x1000_05E3);

/// The client's `0x100005CE` — *sort by name*.
pub const SORT_NAME_BUTTON: ElementId = ElementId(0x1000_05CE);
/// Its `0x100005D6` — *sort by status*.
pub const SORT_STATUS_BUTTON: ElementId = ElementId(0x1000_05D6);
/// Its `0x100005DC` — **Abandon**, the one element id this panel sends on.
pub const ABANDON_BUTTON: ElementId = ElementId(0x1000_05DC);

/// The row template the client adds each contract from, by id.
///
/// The list carries exactly one template, so adding it by id and
/// [`ListBoxWidget::add_from_template`]'s index 0 name the same row.
pub const ROW_TEMPLATE: ElementId = ElementId(0x1000_05D7);
/// Index of [`ROW_TEMPLATE`] in the list box's template list.
pub const ROW_TEMPLATE_INDEX: usize = 0;
/// The row's name child, looked up inside each row.
pub const ROW_NAME: u32 = 0x1000_05D1;
/// The row's status child, looked up inside each row.
pub const ROW_STATUS: u32 = 0x1000_05D2;

/// The redraw throttle: the next refresh is due half a second after the last.
pub const TICK_PERIOD: f64 = 0.5;

/// The sort criterion — name = 0, status = 1.
pub use dereth_client_contract::panels::contracts::ContractSort;

/// The selected index off the live list box, read back off the element rather than
/// mirrored — the same treatment `super::titles`' own note argues for.
fn selected_index(ui: &UiSystem, list: ElemHandle) -> Option<usize> {
    ui.node(list)
        .and_then(|n| n.behaviour.as_ref())
        .and_then(|b| (**b).as_any())
        .and_then(|a| a.downcast_ref::<dereth_ui::widgets::listbox::ListBox>())
        .and_then(|l| l.selected)
}

/// Writes the list box element's selected index, which is how the client puts the selection
/// back after a flush, and clears it (`None`) in between.
///
/// Written straight onto the behaviour, as [`ListBoxWidget`]'s own `mirror_items` does, rather
/// than through select: that path needs `UiSystem::take_behaviour`/`put_behaviour`,
/// which are `pub(crate)` in `dereth-ui` (see `panels::listbox::update_layout`'s note).
/// The only thing lost is the notify half, and `ContractsPanel`'s own `4` arm is
/// [`ContractsPanel::update_buttons`] — which the listbox rebuild calls on the next line anyway,
/// so the observable is identical. An index past the end **clears** the selection, exactly as
/// the client's lookup finding no row does.
fn set_selected_index(ui: &mut UiSystem, list: ElemHandle, index: Option<usize>) {
    let Some(l) = ui.node_mut(list).and_then(|n| {
        n.behaviour
            .as_mut()?
            .as_any_mut()?
            .downcast_mut::<dereth_ui::widgets::listbox::ListBox>()
    }) else {
        return;
    };
    l.selected = index.filter(|i| *i < l.items.len());
}

/// Lowercase both names, then compare.
fn cmp_name(a: &ContractEntry, b: &ContractEntry) -> std::cmp::Ordering {
    a.name.to_lowercase().cmp(&b.name.to_lowercase())
}

/// The status comparator.
///
/// A contract is *repeating* when it is at stage 3 with a server-update time above zero. When
/// both are repeating, the one with less time remaining sorts first; otherwise the lowercased
/// status texts are compared, and a tie falls through to the name comparator.
///
/// *Repeating* is [`ContractEntry::repeat_remaining`] being `Some`, computed host-side where the
/// two doubles live.
fn cmp_status(a: &ContractEntry, b: &ContractEntry) -> std::cmp::Ordering {
    if let (Some(x), Some(y)) = (a.repeat_remaining, b.repeat_remaining) {
        return x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal);
    }
    match a.status.to_lowercase().cmp(&b.status.to_lowercase()) {
        std::cmp::Ordering::Equal => cmp_name(a, b),
        other => other,
    }
}

/// `ContractsPanel`, bound to a live tree.
#[derive(Debug, Default)]
pub struct ContractsPanel {
    /// The `ContractsPanel` element itself — the subject of the shown edge.
    pub panel: Option<ElemHandle>,
    /// The contracts list box.
    pub list: Option<ListBoxWidget>,
    notes: Option<ElemHandle>,
    progress: Option<ElemHandle>,
    contact: Option<ElemHandle>,
    contact_loc: Option<ElemHandle>,
    area: Option<ElemHandle>,
    timed: Option<ElemHandle>,
    /// The contract list, in **display** order — i.e. after `ContractsPanel::sort_contract_list`.
    pub rows: Vec<ContractEntry>,
    /// The sort criterion.
    pub sort: ContractSort,
    /// Whether the sort is reversed.
    pub reverse: bool,
    /// The list box's selected row, as an index into [`ContractsPanel::rows`].
    pub selected: Option<usize>,
    /// How many times the contract-list rebuild and sort ran. **Three states, not two**:
    /// this is what separates "rebuilt and the character holds no contracts" from "never ran",
    /// which is the whole difference between a new character and an unwired panel.
    pub rebuilds: u32,
    /// How many times the half-second listbox refresh ran.
    pub refreshes: u32,
    /// Abandon presses that sent nothing — no row selected, or a row whose id is `0`. The two
    /// guards, counted so a silent button can say why.
    pub abandon_refusals: u32,
    /// The key [`ContractsPanel::update`] last rebuilt on — `(contract_id, stage, name)` per row,
    /// which is everything the two notices can change and nothing the clock can.
    last_key: Option<Vec<(u32, u32, String)>>,
    /// The panel's visibility on the previous frame, for the shown edge.
    was_visible: bool,
    /// When the next half-second refresh is due.
    time_next_update: f64,
    /// The last clicked row and its time — the client's whole double-click state.
    /// Kept because the check is real; **its answer is discarded by the caller** in the
    /// client too, so nothing acts on [`ContractsPanel::double_clicks`] but the tests.
    last_click: Option<(usize, f64)>,
    /// How many times the double-click check answered true. See `last_click`.
    pub double_clicks: u32,
}

impl ContractsPanel {
    /// The contracts panel's post-init, minus the two contract-tracker notice registrations. This
    /// build has no notice bus at this seam and both arrive as a change in [`Self::update`]'s
    /// snapshot.
    ///
    /// `root` is the gameplay root: the seven ids are unique in the shipped tree, and the child
    /// lookup is recursive, so binding from the root finds the same elements binding
    /// from the sub-panel would. Falling back to `root` when [`PANEL`] is missing keeps a
    /// differently-rooted tree bindable instead of silently blank.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        *self = Self::default();
        self.panel = ui.get_child_recursive(root, PANEL);
        let me = self.panel.unwrap_or(root);
        self.notes = ui.get_child_recursive(me, NOTES_TEXT);
        self.progress = ui.get_child_recursive(me, PROGRESS_TEXT);
        self.contact = ui.get_child_recursive(me, CONTACT_TEXT);
        self.contact_loc = ui.get_child_recursive(me, CONTACT_LOC_TEXT);
        self.area = ui.get_child_recursive(me, AREA_TEXT);
        self.timed = ui.get_child_recursive(me, TIMED_TEXT);
        self.list = ui
            .get_child_recursive(me, CONTRACTS_BOX)
            .map(|h| ListBoxWidget::bind(ui, h));
    }

    /// True once the list box was found — the binding without which nothing can be drawn.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.list.is_some()
    }

    /// How many row templates the bound list carries. `Self::add_contract_to_listbox` needs
    /// **one**.
    #[must_use]
    pub fn templates(&self) -> usize {
        self.list.as_ref().map_or(0, |l| l.templates.len())
    }

    /// The contract ids on screen, in list order — what a test asserts against instead of a pixel.
    #[must_use]
    pub fn shown(&self) -> Vec<u32> {
        self.rows.iter().map(|r| r.contract_id).collect()
    }

    /// How many rows the bound list box actually holds — the tree's own count, not this struct's.
    #[must_use]
    pub fn drawn(&self) -> usize {
        self.list.as_ref().map_or(0, |l| l.items.len())
    }

    /// Whether `ContractsPanel` itself is visible — the gate the shown edge and the
    /// global-message tick share.
    #[must_use]
    pub fn visible(&self, ui: &UiSystem) -> bool {
        self.panel
            .and_then(|h| ui.node(h))
            .is_some_and(|n| n.region.flags.visible)
    }

    /// One frame's drive, standing in for the two notices, and
    /// global message `3`.
    ///
    /// Returns whether the tree was rewritten.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        if !self.bound() {
            return false;
        }
        let visible = self.visible(ui);
        let shown_edge = visible && !self.was_visible;
        self.was_visible = visible;
        let entries = view.contracts();
        let key: Vec<(u32, u32, String)> = entries
            .iter()
            .map(|e| (e.contract_id, e.stage, e.name.clone()))
            .collect();
        // The two notices: the contract-tracker-table update and
        // the contract-tracker update are the same two calls, and neither is
        // gated on visibility — retail rebuilds a hidden panel too, which is why opening the tab
        // after a contract arrives shows it immediately.
        let changed = self.last_key.as_ref() != Some(&key);
        if changed || shown_edge {
            self.last_key = Some(key);
            self.rebuild_contract_list(entries);
            self.sort_contract_list(ui);
            self.rebuilds += 1;
            // The listbox refresh would be a no-op on this frame anyway; the throttle is
            // re-armed so the next redraw is half a second after the rebuild, not immediately.
            self.time_next_update = ui.now.0 + TICK_PERIOD;
            return true;
        }
        // The global-message handler acts only on message `3`, only while the panel is visible,
        // and only once the throttle has run out.
        if !visible || ui.now.0 < self.time_next_update {
            return false;
        }
        self.refresh_contract_listbox(ui, view);
        self.time_next_update = ui.now.0 + TICK_PERIOD;
        true
    }

    /// Clear the contract list, then one entry per tracker in the character's tracker table.
    ///
    /// The join against the contract table is the host's (see [`ContractEntry`]); what is left
    /// here is the clear and the copy. Retail uses the contract lookup's answer **without a null
    /// check**, so a tracker naming a contract the dat does not carry crashes it; the host drops
    /// such a tracker
    /// instead and the count of what it dropped is its own.
    fn rebuild_contract_list(&mut self, entries: Vec<ContractEntry>) {
        self.rows = entries;
    }

    /// Sort the contract list: rebuild the listbox, sort by the status comparator when the
    /// criterion is status and by the name comparator otherwise, then rebuild the listbox again.
    ///
    /// **The first listbox rebuild is not dead code**: it is what makes the *pre-sort* index the
    /// one the list box reports, so the selection the second rebuild restores is the row the
    /// player had picked. Both rebuilds are here. Note also that name is the client's default
    /// case, so an out-of-range criterion sorts by name.
    ///
    /// The reverse comparators are the same comparators with the final inequality flipped (the
    /// remaining-time arm flips too), which is a whole-order reversal and nothing subtler.
    fn sort_contract_list(&mut self, ui: &mut UiSystem) {
        self.rebuild_contract_listbox(ui);
        let picked = self
            .selected
            .and_then(|i| self.rows.get(i).map(|r| r.contract_id));
        match self.sort {
            ContractSort::Status => self.rows.sort_by(cmp_status),
            ContractSort::Name => self.rows.sort_by(cmp_name),
        }
        if self.reverse {
            self.rows.reverse();
        }
        // The client's selection survives because the list box selects a *row* by pointer, which
        // sorting the contract list does not touch; here it is an index into `rows`, which the
        // sort does, so it is carried across by contract id. Same observable, one representation
        // apart.
        self.selected = picked.and_then(|id| self.rows.iter().position(|r| r.contract_id == id));
        self.rebuild_contract_listbox(ui);
    }

    /// Remember the selected index, flush, clear the selection, one
    /// [`Self::add_contract_to_listbox`] per entry, then restore the index and
    /// [`Self::update_buttons`].
    fn rebuild_contract_listbox(&mut self, ui: &mut UiSystem) {
        let Some(mut list) = self.list.take() else {
            return;
        };
        let keep = self.selected;
        list.flush(ui);
        set_selected_index(ui, list.handle, None);
        let rows = std::mem::take(&mut self.rows);
        for e in &rows {
            Self::add_contract_to_listbox(ui, &mut list, e);
        }
        self.rows = rows;
        list.update_layout(ui);
        set_selected_index(ui, list.handle, keep);
        self.selected = selected_index(ui, list.handle).filter(|i| *i < self.rows.len());
        self.list = Some(list);
        self.update_buttons(ui);
    }

    /// Add one contract row from [`ROW_TEMPLATE`]: the name child [`ROW_NAME`] gets the contract's
    /// name and, when the contract resolved, the status child [`ROW_STATUS`] gets its progress
    /// string.
    ///
    /// The status child is written **only when the contract resolved**, which is why the host
    /// drops an unresolvable tracker rather than handing over an entry with an empty status.
    fn add_contract_to_listbox(ui: &mut UiSystem, list: &mut ListBoxWidget, e: &ContractEntry) {
        let Some(row) = list.add_from_template(ui, ROW_TEMPLATE_INDEX, None) else {
            return;
        };
        if let Some(t) = ui.get_child_recursive(row, ElementId(ROW_NAME)) {
            if let Some(el) = ui.text_element_mut(t) {
                el.set_text(&e.name);
            }
        }
        if let Some(t) = ui.get_child_recursive(row, ElementId(ROW_STATUS)) {
            if let Some(el) = ui.text_element_mut(t) {
                el.set_text(&e.status);
            }
        }
    }

    /// The contracts panel's refresh contract listbox — the half-second redraw.
    ///
    /// Its first check is a guard nothing else in this file needs: the listbox row count must
    /// equal the contract list's length, i.e. **do nothing at all while the rows and the model
    /// are out of step**. It re-sets each row's name and status text and writes the fresh status
    /// back into the contract list entry, which is what keeps [`cmp_status`]'s string arm in step
    /// with what is on screen, and it ends with [`Self::update_buttons`].
    fn refresh_contract_listbox(&mut self, ui: &mut UiSystem, view: &dyn GameView) {
        let Some(list) = self.list.as_ref() else {
            return;
        };
        if list.items.len() != self.rows.len() {
            return;
        }
        // The client re-looks-up each row's contract in the hash by contract id; this is the
        // same lookup against the host's freshly computed snapshot, which is where the display
        // clock has already been applied.
        let fresh = view.contracts();
        let items = list.items.clone();
        for (i, row) in self.rows.iter_mut().enumerate() {
            if let Some(f) = fresh.iter().find(|e| e.contract_id == row.contract_id) {
                row.status.clone_from(&f.status);
                row.repeat_remaining = f.repeat_remaining;
                row.timed.clone_from(&f.timed);
                row.contact_location.clone_from(&f.contact_location);
                row.area_location.clone_from(&f.area_location);
            }
            let Some(h) = items.get(i).copied() else {
                continue;
            };
            if let Some(t) = ui.get_child_recursive(h, ElementId(ROW_NAME)) {
                if let Some(el) = ui.text_element_mut(t) {
                    el.set_text(&row.name);
                }
            }
            if let Some(t) = ui.get_child_recursive(h, ElementId(ROW_STATUS)) {
                if let Some(el) = ui.text_element_mut(t) {
                    el.set_text(&row.status);
                }
            }
        }
        self.refreshes += 1;
        self.update_buttons(ui);
    }

    /// The six detail fields for the **selected** row.
    ///
    /// Despite the name it moves no button state at all. Its three early returns are worth
    /// keeping: an empty list, a list with no selection, and a contract the dat does not carry all
    /// leave every field **exactly as it was**, so the pane keeps showing the last contract the
    /// player looked at rather than blanking.
    ///
    /// The two `Position` fields have a fourth: a `Position` whose `objcell_id` is **zero** is
    /// skipped entirely, which is [`ContractEntry::contact_location`]
    /// being `None`.
    fn update_buttons(&mut self, ui: &mut UiSystem) {
        let Some(e) = self.selected.and_then(|i| self.rows.get(i)) else {
            return;
        };
        let set = |ui: &mut UiSystem, h: Option<ElemHandle>, s: &str| {
            if let Some(el) = h.and_then(|h| ui.text_element_mut(h)) {
                el.set_text(s);
            }
        };
        set(ui, self.notes, &e.description);
        set(ui, self.contact, &e.contact);
        if let Some(s) = e.contact_location.clone() {
            set(ui, self.contact_loc, &s);
        }
        set(ui, self.progress, &e.status);
        if let Some(s) = e.area_location.clone() {
            set(ui, self.area, &s);
        }
        set(ui, self.timed, &e.timed);
    }

    /// The contracts panel's double-click check. A click on the same row as the last one, no more
    /// than one second after it, answers true and clears the latch; any other click records this
    /// row and time and answers false.
    ///
    /// **The caller throws the answer away.** This build keeps the function because it is real
    /// and its state machine is testable; nothing acts on it, exactly as nothing does in retail.
    fn check_for_double_click(&mut self, now: f64, index: usize) -> bool {
        if self
            .last_click
            .is_some_and(|(i, t)| i == index && now <= t + 1.0)
        {
            self.last_click = None;
            self.double_clicks += 1;
            return true;
        }
        self.last_click = Some((index, now));
        false
    }

    /// The contracts panel's element-message handler, whole.
    ///
    /// On a button click (`1`): the name and status sort buttons each flip the reverse flag when
    /// their criterion is already active, and otherwise clear it and select their criterion, then
    /// re-sort and refresh the detail fields. Abandon, when a row is selected and its contract id
    /// is non-zero, sends the abandon request, refreshes the detail fields and stops; with no
    /// usable row it only refreshes the detail fields (it does not re-sort). Any other button only
    /// refreshes the detail fields. Selection changed (`4`) refreshes the detail fields; item
    /// activated (`0x43`) runs the double-click check when a row is selected and refreshes the
    /// detail fields either way. Anything else is ignored.
    ///
    /// Returns true when an arm consumed the message.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        _view: &dyn GameView,
    ) -> bool {
        use dereth_ui::msg::element::id as msg;
        if !self.bound() {
            return false;
        }
        if m.id == msg::BUTTON_CLICKED {
            if m.source_id == SORT_NAME_BUTTON {
                if self.sort == ContractSort::Name {
                    self.reverse = !self.reverse;
                } else {
                    self.reverse = false;
                    self.sort = ContractSort::Name;
                }
                self.sort_contract_list(ui);
                return true;
            }
            if m.source_id == SORT_STATUS_BUTTON {
                if self.sort == ContractSort::Status {
                    self.reverse = !self.reverse;
                } else {
                    self.reverse = false;
                    self.sort = ContractSort::Status;
                }
                self.sort_contract_list(ui);
                return true;
            }
            if m.source_id == ABANDON_BUTTON {
                return self.abandon_selected(ui);
            }
            return false;
        }
        // `4` and `0x43` both have to come from **this** panel's list box; every list box in the
        // tree raises them and a blanket forward would put a great many of them through a fan-out
        // that wants one. Same gate `super::titles` applies.
        let Some(list) = self.list.as_ref().map(|l| l.handle) else {
            return false;
        };
        if list != m.source {
            return false;
        }
        if m.id == msg::LIST_SELECTION_CHANGED {
            self.selected = selected_index(ui, list).filter(|i| *i < self.rows.len());
            self.update_buttons(ui);
            return true;
        }
        if m.id == msg::LIST_ITEM_ACTIVATED {
            self.selected = selected_index(ui, list).filter(|i| *i < self.rows.len());
            let Some(i) = self.selected else { return false };
            let now = ui.now.0;
            self.check_for_double_click(now, i);
            self.update_buttons(ui);
            return true;
        }
        false
    }

    /// The `0x100005DC` arm's body — the selected index, the no-selection guard, the
    /// `contract_id != 0` guard, then the abandon-contract request.
    fn abandon_selected(&mut self, ui: &mut UiSystem) -> bool {
        let list = self.list.as_ref().map(|l| l.handle);
        // Re-read off the element rather than trusting the mirror, because that is where
        // [`Self::update_buttons`] and this arm both read it from.
        self.selected = list
            .and_then(|h| selected_index(ui, h))
            .filter(|i| *i < self.rows.len());
        let Some(contract_id) = self
            .selected
            .and_then(|i| self.rows.get(i))
            .map(|e| e.contract_id)
        else {
            self.abandon_refusals += 1;
            return true;
        };
        if contract_id == 0 {
            self.abandon_refusals += 1;
            return true;
        }
        ui.requests.emit(UiRequest::AbandonContract { contract_id });
        // The tracker is **not** removed here: the table changes when the shard answers with a
        // `0x0315` carrying the delete-contract flag. The detail fields refresh on the press anyway.
        self.update_buttons(ui);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **No datagram leaves this process.** Nothing in this module opens a socket.
    ///
    /// Oracle: element ids from panel initialization, contract-row insertion
    /// and element-message handling, pinned as literals and
    /// cross-checked against the catalogue row that has been standing in for this panel.
    #[test]
    fn the_element_ids_are_the_ones_the_client_binds_and_reads() {
        assert_eq!(PANEL, ElementId(0x1000_05D4));
        assert_eq!(TAB, ElementId(0x1000_05D3));
        assert_eq!(ROW_TEMPLATE, ElementId(0x1000_05D7));
        assert_eq!(ROW_NAME, 0x1000_05D1);
        assert_eq!(ROW_STATUS, 0x1000_05D2);
        let spec = crate::panels::catalogue::spec("ContractsPanel").expect("catalogued");
        assert_eq!(spec.ty, crate::element_types::ty::CONTRACTS);
        for id in [
            CONTRACTS_BOX.0,
            NOTES_TEXT.0,
            PROGRESS_TEXT.0,
            CONTACT_TEXT.0,
            CONTACT_LOC_TEXT.0,
            AREA_TEXT.0,
            TIMED_TEXT.0,
        ] {
            assert!(spec.children.iter().any(|c| c.id.0 == id), "{id:#010X}");
        }
        // The catalogue names the template's two **text children**, not the template root —
        // `0x100005D7` is what adding a row instantiates and it is in neither
        // table. Asserted so the two cannot drift apart silently.
        assert_eq!(spec.templates.len(), 2);
        for t in spec.templates {
            assert!(
                t.id.0 == ROW_NAME || t.id.0 == ROW_STATUS,
                "{:#010X}",
                t.id.0
            );
        }
    }

    /// An unbound panel answers **not-bound, 0 templates, 0 rebuilds**, rather than looking like
    /// a character who holds no contracts.
    #[test]
    fn an_unbound_panel_says_so() {
        let p = ContractsPanel::default();
        assert!(!p.bound());
        assert_eq!(p.templates(), 0);
        assert_eq!(p.rebuilds, 0);
        assert_eq!(p.refreshes, 0);
        assert_eq!(p.drawn(), 0);
        assert!(p.shown().is_empty());
        assert_eq!(p.selected, None);
        assert_eq!(p.sort, ContractSort::Name);
        assert!(!p.reverse);
    }

    fn entry(id: u32, name: &str, status: &str) -> ContractEntry {
        ContractEntry {
            contract_id: id,
            name: name.into(),
            status: status.into(),
            ..ContractEntry::default()
        }
    }

    /// Oracle: retail lowercases both names before comparing, so
    /// `"aardvark"` sorts before `"Beacon"` — which a raw byte compare gets backwards.
    #[test]
    fn the_name_sort_is_case_insensitive() {
        let mut v = [entry(2, "Beacon", ""), entry(1, "aardvark", "")];
        v.sort_by(cmp_name);
        assert_eq!(
            v.iter().map(|e| e.contract_id).collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    /// Oracle: retail's status comparator — two repeating contracts sort by **remaining
    /// time, ascending**, and everything else by the status text with the name as the tie-break.
    #[test]
    fn the_status_sort_puts_the_soonest_repeat_first_and_ties_break_by_name() {
        let mut a = entry(1, "Zephyr", "Done (2m to Repeat)");
        a.repeat_remaining = Some(120.0);
        let mut b = entry(2, "Aegis", "Done (1m to Repeat)");
        b.repeat_remaining = Some(60.0);
        let mut v = [a, b];
        v.sort_by(cmp_status);
        assert_eq!(v[0].contract_id, 2, "60 s remaining sorts before 120 s");

        // Same status text, no repeat timers: the name decides.
        let mut v = [
            entry(1, "Zephyr", "In Progress"),
            entry(2, "Aegis", "In Progress"),
        ];
        v.sort_by(cmp_status);
        assert_eq!(v[0].contract_id, 2);

        // One repeating and one not: the string arm, not the number arm.
        let mut r = entry(1, "Aegis", "Done (1m to Repeat)");
        r.repeat_remaining = Some(60.0);
        let mut v = [r, entry(2, "Zephyr", "Available")];
        v.sort_by(cmp_status);
        assert_eq!(v[0].contract_id, 2, "\"available\" sorts before \"done (\"");
    }

    /// Oracle: the client's three-line state machine, including that the
    /// second click **clears** the latch so a third does not also count.
    #[test]
    fn the_double_click_latch_is_one_second_and_clears_itself() {
        let mut p = ContractsPanel::default();
        assert!(!p.check_for_double_click(10.0, 3));
        assert!(p.check_for_double_click(10.5, 3));
        assert!(!p.check_for_double_click(10.6, 3), "the latch was cleared");
        assert!(!p.check_for_double_click(20.0, 3));
        assert!(
            !p.check_for_double_click(21.5, 3),
            "1.5 s is outside the window"
        );
        // A different row never pairs with the one before it.
        assert!(!p.check_for_double_click(21.6, 4));
        assert_eq!(p.double_clicks, 1);
    }
}
