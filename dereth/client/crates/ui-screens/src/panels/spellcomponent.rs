//! `SpellComponentPanel` — the spell-component panel: seven category headers and one row per
//! component the player holds, each with an owned count and an editable "keep this many" field.
//!
//! This panel is the UI consumer of `dereth_client_model::magic`'s `ComponentTracker` and of the
//! desired-components list, behind the bindings `panels::catalogue` records for
//! `SpellComponentPanel`.
//!
//! # Where the window lives
//!
//! `SpellComponentPanel` (type `0x1000002F`) is a sub-panel of the spell page `0x10000190`, beside
//! `SpellbookPanel` (`0x100002AC`). Its one binding is the list box **`0x10000464`**, which the
//! post-init finds by recursive child lookup. Everything below is that list's rows.
//!
//! # The two row templates
//!
//! The component update builds the list from the list box's own template list
//! (property `0x64`), **index 0 for a category header and index 1 for a component row**:
//!
//! | | element | carries | children |
//! |---|---|---|---|
//! | header | `0x10000466` | `Int` attribute `0x1000004D` = the category index | its own text |
//! | row | `0x10000467` | `DataID` attribute `0x1000004C` = the component **WCID** | `0x10000468` icon, `0x10000469` name, `0x1000046A` owned count, `0x1000046B` desired level |
//!
//! **`0x1000004C` is the component's WCID, not its icon.** Retail is unambiguous: the row's
//! `0x1000004C` attribute is the class id, the same value the icon build and the buy-rates
//! update are then given.
//!
//! # What this build does differently, said out loud
//!
//! Retail's component update is an **incremental diff**: it walks the seven lists and the existing rows in
//! lockstep, reading `0x1000004D` off each row to decide whether the category header it is looking
//! at is the one it wants, and `0x1000004C` to decide whether a component row is the one it wants,
//! inserting and deleting to make the list match. This panel flushes and rebuilds. The resulting
//! list is the same list — the tracker's own ordering is what decides it, and the tracker keeps
//! each category sorted by name — but a row's
//! *element identity* does not survive a rebuild here where it does in retail. Nothing in the
//! client depends on that identity across an update, and [`SpellComponentPanel::rebuilds`] counts
//! the rebuilds so a test can tell "rebuilt and there was nothing" from "never ran".
//!
//! # The bound, three times
//!
//! `0 <= level <= 5000` is enforced in three places in the client: twice in
//! the element-message handler and the buy-rates update as `(-1 < n) && (n < 0x1389)`
//! and once in the player module's desired-comp-level write as `(n < 0) || (5000 < n)`.
//! [`MAX_DESIRED_LEVEL_EXCLUSIVE`] is the panel's spelling; the host checks again with its own.

use dereth_primitives::ObjectId;
use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::panels::listbox::ListBoxWidget;
use crate::view::{ComponentCategory, ComponentRow, GameView, UiRequest};

/// The component list box — the post-init's one child lookup.
pub const COMPONENT_LIST: ElementId = ElementId(0x1000_0464);

/// The category header row's element id, template index [`HEADER_TEMPLATE`].
pub const HEADER_ELEMENT: ElementId = ElementId(0x1000_0466);
/// The component row's element id, template index [`ROW_TEMPLATE`].
pub const ROW_ELEMENT: ElementId = ElementId(0x1000_0467);

/// The list box template a category header is created from.
pub const HEADER_TEMPLATE: usize = 0;
/// The list box template a component row is created from.
pub const ROW_TEMPLATE: usize = 1;

/// The four children of a component row, in the order the component update writes them.
pub mod row {
    /// The icon build's target.
    pub const ICON: u32 = 0x1000_0468;
    /// The component's name.
    pub const NAME: u32 = 0x1000_0469;
    /// The owned count.
    pub const OWNED: u32 = 0x1000_046A;
    /// The client writes the desired component level here, and this is the one
    /// editable field in the panel — it gets a numeric input filter installed on creation.
    pub const DESIRED: u32 = 0x1000_046B;
}

/// `Int` attribute `0x1000004D` — the category index a header row stands for.
pub const ATTR_CATEGORY: u32 = 0x1000_004D;
/// `DataID` attribute `0x1000004C` — the component WCID a row stands for. See the module note: it
/// is the class id, not the icon.
pub const ATTR_COMPONENT_WCID: u32 = 0x1000_004C;

/// The category title tokens, in the client's own order, which is the order of the
/// spell-component category enum.
///
/// These are **string tokens**, hashed at use and resolved in string table `0x10000001`, the
/// same shape as `TitlesPanel`'s `ID_CharacterTitle_*`. The hash is
/// `dereth_primitives::num::hash::str_hash` and the table is [`crate::chat::mainchat::CAPTION_STRING_TABLE`].
pub const CATEGORY_TITLES: [&str; 7] = [
    "ID_SpellComp_Category_Scarabs",
    "ID_SpellComp_Category_Herbs",
    "ID_SpellComp_Category_Gems",
    "ID_SpellComp_Category_Alchemical",
    "ID_SpellComp_Category_Talismans",
    "ID_SpellComp_Category_Tapers",
    "ID_SpellComp_Category_Peas",
];

/// `(-1 < n) && (n < 0x1389)` — the panel's own spelling of the bound, exclusive.
pub const MAX_DESIRED_LEVEL_EXCLUSIVE: i32 = 0x1389;

/// One drawn component row, as this build keeps it.
///
/// Retail keeps the WCID on the element (`0x1000004C`) because it has nowhere else; this build has
/// no data-id attribute write at this seam, so the identity lives here — the same shortcut
/// [`crate::panels::skills::SkillRow`] takes for `0x1000023D`'s rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawnRow {
    pub wcid: u32,
    pub name: String,
    pub owned: i64,
    pub desired: i32,
    pub object: Option<ObjectId>,
    pub element: ElemHandle,
    /// The `0x1000046B` field's element, which is the one a text-commit message arrives from.
    pub desired_field: Option<ElemHandle>,
}

/// `SpellComponentPanel`, bound to a live tree.
#[derive(Debug)]
pub struct SpellComponentPanel {
    /// The component list box, `0x10000464`.
    pub list: Option<ListBoxWidget>,
    /// The category headers, in list order, with the category each stands for.
    pub headers: Vec<(u32, ElemHandle)>,
    /// Every component row on screen, in list order.
    pub rows: Vec<DrawnRow>,
    /// The snapshot the tree was last built for, so an unchanged frame rebuilds nothing.
    last: Option<Vec<ComponentCategory>>,
    /// How many times [`Self::update`] rebuilt. **Three states, not two**: this separates "rebuilt
    /// and the player holds nothing" from "never ran", which is the difference between an empty
    /// pack and an unwired panel.
    pub rebuilds: u32,
    /// The broadcast-selection flag: the constructor's `true`, cleared by
    /// [`Self::on_selection_changed`] for exactly one reflected `4` and restored by
    /// every `4` that reaches [`Self::on_element_message`] with a non-zero index. See there.
    pub broadcast_selection: bool,
    /// The object id [`Self::on_selection_changed`] remembers.
    ///
    /// It is the notice's **edge guard**, not a cache: an equality check returns
    /// without touching the list when the world selection has not moved since the last time this
    /// ran, and a second equality check makes "a non-component was selected, and a
    /// non-component was already selected" do nothing at all. Both arms matter to this build,
    /// because it polls the notice once a frame rather than being handed it.
    pub selected_object: Option<ObjectId>,
    /// How many times [`Self::on_selection_changed`] actually moved the list selection —
    /// matched a row, or cleared. **Three states, not two**, for the same reason
    /// [`Self::rebuilds`] is: zero after a session in which the player selected components means
    /// the notice is not reaching the panel.
    pub selection_notices: u32,
}

impl Default for SpellComponentPanel {
    /// SpellComponentPanel: null list, empty tables, and the broadcast-selection flag `true` —
    /// the one field a derived `Default` would get wrong.
    fn default() -> Self {
        Self {
            list: None,
            headers: Vec::new(),
            rows: Vec::new(),
            last: None,
            rebuilds: 0,
            broadcast_selection: true,
            selected_object: None,
            selection_notices: 0,
        }
    }
}

impl SpellComponentPanel {
    /// The spell-component panel's post-init, minus the notice registrations and the seven
    /// title-token assignments (which are [`CATEGORY_TITLES`], a constant).
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        let Some(h) = ui.get_child_recursive(root, COMPONENT_LIST) else {
            *self = Self::default();
            return;
        };
        self.list = Some(ListBoxWidget::bind(ui, h));
        self.headers.clear();
        self.rows.clear();
        self.last = None;
    }

    /// True once the list box was found — the binding without which nothing can be drawn.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.list.is_some()
    }

    /// How many templates the bound list box actually carries. The component update needs
    /// **two** (index 0 and index 1); a list with fewer draws nothing and
    /// must be able to say so rather than look like a player with no components.
    #[must_use]
    pub fn templates(&self) -> usize {
        self.list.as_ref().map_or(0, |l| l.templates.len())
    }

    /// The panel's update-spell-components notice -> its component update,
    /// guarded on the snapshot.
    ///
    /// Returns whether the tree was rebuilt.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let cats = view.spell_components();
        if self.last.as_ref() == Some(&cats) {
            return false;
        }
        self.rebuild(ui, &cats);
        self.last = Some(cats);
        self.rebuilds += 1;
        true
    }

    fn rebuild(&mut self, ui: &mut UiSystem, cats: &[ComponentCategory]) {
        self.headers.clear();
        self.rows.clear();
        // The flush clears the list's selected item, and this build flushes
        // where retail splices (see the module note). So a rebuild loses the highlight retail
        // would have kept, and the memo has to go with it or `on_selection_changed`'s guard
        // would refuse to put it back. Retail
        // never reaches this state; the clear is the cost of the flush, not a transcription.
        self.selected_object = None;
        let Some(mut list) = self.list.take() else {
            return;
        };
        list.flush(ui);
        for c in cats {
            // **Not seven headers, always.** Whether the category's list is empty is tested **before** either arm:
            // a non-empty list creates the header, and an empty one takes the delete arm, which
            // only ever deletes.
            //
            // Nothing is created on the empty path. So a category
            // the player holds nothing of gets **no header**, and one that has just emptied has its
            // header removed. Seven headers is what a player who carries at least one of every
            // category sees, and nobody else.
            if c.rows.is_empty() {
                continue;
            }
            if let Some(h) = list.add_from_template(ui, HEADER_TEMPLATE, None) {
                ui.set_attribute_int(h, ATTR_CATEGORY, i32::try_from(c.category).unwrap_or(0));
                let text = category_title(ui, c.category);
                if let Some(t) = ui.text_element_mut(h) {
                    t.set_text(&text);
                }
                self.headers.push((c.category, h));
            }
            for r in &c.rows {
                let Some(h) = list.add_from_template(ui, ROW_TEMPLATE, None) else {
                    continue;
                };
                write_row(ui, h, r);
                self.rows.push(DrawnRow {
                    wcid: r.wcid,
                    name: r.name.clone(),
                    owned: r.owned,
                    desired: r.desired,
                    object: r.object,
                    element: h,
                    desired_field: ui.get_child_recursive(h, ElementId(row::DESIRED)),
                });
            }
        }
        list.update_layout(ui);
        self.list = Some(list);
    }

    /// The component ids on screen, in list order — what a test asserts against instead of a pixel.
    #[must_use]
    pub fn shown(&self) -> Vec<u32> {
        self.rows.iter().map(|r| r.wcid).collect()
    }

    /// The spell component panel's element-message handler, both arms.
    ///
    /// On message `0x2F` with `p1 == 0` from the desired field `0x1000046B`: the parent must be a
    /// component row (`0x10000467`); its WCID is read from attribute `0x1000004C` and the field's
    /// text parsed as an integer. In `0..=5000` it sends the set-desired-component-level event
    /// and stores the level; otherwise it puts the stored level back in the field. The other arm
    /// is the selection one (see [`Self::on_element_message`]).
    ///
    /// The out-of-range case is **not** a request: the field is put back to the stored value. That
    /// is why this returns the text the field should now show, rather than a bool.
    pub fn on_desired_committed(
        &mut self,
        ui: &mut UiSystem,
        from: ElemHandle,
        typed: &str,
    ) -> bool {
        let Some(i) = self.rows.iter().position(|r| r.desired_field == Some(from)) else {
            return false;
        };
        let stored = self.rows[i].desired;
        let wcid = self.rows[i].wcid;
        // A plain integer parse, on a field a numeric input filter has
        // been keeping to digits. Anything it cannot parse is out of range by construction.
        let parsed: Option<i32> = typed.trim().parse().ok();
        match parsed.filter(|n| (0..MAX_DESIRED_LEVEL_EXCLUSIVE).contains(n)) {
            Some(n) => {
                self.rows[i].desired = n;
                set_child_text(ui, self.rows[i].element, row::DESIRED, &n.to_string());
                ui.requests
                    .emit(UiRequest::SetDesiredComponentLevel { wcid, level: n });
                true
            }
            None => {
                set_child_text(ui, self.rows[i].element, row::DESIRED, &stored.to_string());
                false
            }
        }
    }

    /// The selection arm of the element-message handler — the **`4`** arm, which is
    /// `LIST_SELECTION_CHANGED` and not `0x1C`.
    ///
    /// Message 4 carries a row index, not an element id. Index zero does nothing,
    /// including leaving the broadcast flag unchanged. For a nonzero index, a false
    /// broadcast flag skips selection and is then restored to true. Otherwise a header
    /// (`0x10000466`) clears world selection; a component row (`0x10000467`) reads its
    /// data id from attribute `0x1000004C`, finds the component region, and selects its
    /// object id without forcing. The flag is then restored to true. Every path ends in
    /// the base element handler. The only other local message arm is `0x2F`; there is no
    /// `0x43` arm.
    ///
    /// Three things worth being exact about:
    ///
    /// * **A header row clears the selection** — selecting object `0` is retail's deselect,
    ///   with both arguments zero, just as for the Escape key. Emitted here as
    ///   [`UiRequest::Select`] of `ObjectId(0)`, which the host maps to `None` the way the
    ///   `ulong` 0 is.
    /// * **Index 0 is skipped**, and index 0 is the first header (the component update emits a
    ///   header for every non-empty category), so pressing the first header leaves the old
    ///   selection alone while pressing any other header clears it. Transcribed, not corrected.
    /// * **There is no "player lacks this component" case.**
    ///   The first-object lookup is unguarded, and removal deletes a component entry whose last
    ///   object left, so a drawn row always has an object. The region lookup failing is the only
    ///   silent arm, and [`DrawnRow::object`] being
    ///   `None` is this build's spelling of it.
    ///
    /// The broadcast-selection flag is the one-shot suppressor the selection-changed notice
    /// clears (just before its own list selection with broadcast) so that the reflected `4` does
    /// not re-select the world object. The constructor sets it `true`. That notice is the other
    /// direction — see [`Self::on_selection_changed`].
    ///
    /// Returns whether the message was this list's `4`.
    pub fn on_element_message(&mut self, ui: &mut UiSystem, m: &dereth_ui::ElementMessage) -> bool {
        if m.id != dereth_ui::msg::element::id::LIST_SELECTION_CHANGED {
            return false;
        }
        let Some(list) = self.list.as_ref().map(|l| l.handle) else {
            return false;
        };
        if m.source != list {
            return false;
        }
        // Index 0 falls straight through to the base handler, and does not reach the store
        // that sets the broadcast flag back to `true` either.
        if m.p1 == 0 {
            return true;
        }
        if std::mem::replace(&mut self.broadcast_selection, true) {
            if let Some(item) = selected_item(ui, list) {
                self.select_for_item(ui, item);
            }
        }
        true
    }

    /// The two selected-item arms: a header clears, a row selects its region's object.
    fn select_for_item(&self, ui: &mut UiSystem, item: ElemHandle) {
        let Some(id) = ui.node(item).map(dereth_ui::ElementNode::element_id) else {
            return;
        };
        if id == HEADER_ELEMENT {
            ui.requests.emit(UiRequest::Select(ObjectId(0)));
        } else if id == ROW_ELEMENT {
            // Retail reads the row's `0x1000004C` and looks up the component region — this
            // build keeps the region on [`DrawnRow`], keyed by the row element.
            let Some(r) = self.rows.iter().find(|r| r.element == item) else {
                return;
            };
            let Some(obj) = r.object else { return };
            ui.requests.emit(UiRequest::Select(obj));
        }
    }

    /// **The other direction**: the world selection moving the list selection, through the
    /// [`Self::broadcast_selection`] suppressor.
    ///
    /// Retail's order, with the world selection read fresh each time:
    ///
    /// 1. nothing selected: **clear**, with no memo test at all;
    /// 2. no component tracker: return;
    /// 3. the selected object is not an owned component: if the memo is already `0` do
    ///    **nothing**; otherwise clear the list selection (with broadcast), clear the broadcast
    ///    flag and the memo;
    /// 4. the selection equals the memo: return;
    /// 5. walk the rows, skipping headers and rows without a `0x1000004C` attribute; on the first
    ///    row whose WCID matches, clear the broadcast flag, select that row (with broadcast),
    ///    remember the selection and return;
    /// 6. ran off the end: nothing happens, and the memo is **not** updated.
    ///
    /// Three things worth saying out loud:
    ///
    /// * **The match is on the WCID, not on the object.** The owned-component check covers
    ///   *every* object of every stack, and the row is then found by `0x1000004C`. Selecting any
    ///   one of five separate piles of Lead Scarabs highlights the one Lead Scarab row.
    ///   [`DrawnRow::object`] is the first object id, i.e. one of those five, and
    ///   matching on it would answer wrongly four times out of five — which is why this needs
    ///   [`GameView::object_is_owned_component`] and cannot be done from the snapshot.
    /// * **Clearing the broadcast flag is a one-shot**, done immediately before the list
    ///   selection whose `4` would otherwise bounce straight back into
    ///   [`Self::on_element_message`] and re-select the world object we are reacting to. That arm
    ///   restores the flag. Retail selects with broadcast on, so the `4` really is raised here
    ///   and really is swallowed there.
    /// * **The clear arm is unconditional when nothing is selected** (it skips the memo test), but conditional when something non-component is selected. Those are
    ///   different guards on the same block and this keeps both.
    ///
    /// This build polls once a frame instead of receiving the notice; the memo is exactly what
    /// makes that equivalent, because every arm above is a no-op while the selection has not moved.
    ///
    /// Returns whether the list selection was touched.
    pub fn on_selection_changed(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let Some(list) = self.list.as_ref().map(|l| l.handle) else {
            return false;
        };
        // The world selection — 0 is "nothing", the same spelling the deselect writes.
        let selected = view.selected_object().filter(|o| o.0 != 0);
        let Some(id) = selected else {
            // The client's clear is unconditional **per notice**. This build polls, so the call is
            // made only when it would change something: set_selected_item's unchanged-item
            // early return makes clearing an already-empty list a no-op
            // apart from one *queued element message*, and queueing that on every frame of a
            // session with nothing selected is a deviation the notice itself cannot produce.
            if self.selected_object.is_none() && selected_item(ui, list).is_none() {
                return false;
            }
            return self.clear_list_selection(ui, list);
        };
        let Some(wcid) = view.object_is_owned_component(id) else {
            // A non-component selected while the memo is already
            // empty leaves the list alone. Without this a click on scenery would clear a row the
            // player had just selected from the panel.
            if self.selected_object.is_none() {
                return false;
            }
            return self.clear_list_selection(ui, list);
        };
        // An unchanged world selection leaves the list untouched.
        if self.selected_object == Some(id) {
            return false;
        }
        // The client's walk. `self.rows` is the list's items filtered to `0x10000467` already, and
        // the header arm of the client's loop is a `continue`, so the two walks visit the same
        // rows in the same order and stop at the same one.
        let Some(row) = self.rows.iter().find(|r| r.wcid == wcid).map(|r| r.element) else {
            // Ran off the end: the component is owned but not drawn (the `Undef` bucket, or the
            // list has not been rebuilt since it arrived). Retail does nothing and leaves the
            // memo alone, so the next rebuild's poll tries again.
            return false;
        };
        // **The suppressor is one-shot on a `4` that is actually raised.** The list box's
        // selected-item write compares the new item with the current one: a change takes the
        // full path and raises `4`; an unchanged item broadcasts `0x43` and returns, with no `4`
        // at all.
        //
        // In retail the unchanged case is reached only from the panel's *own* press: the `4` arm
        // selects the world object, that raises this notice **re-entrantly**, the notice finds
        // the row already selected, gets `0x43`, and the flag is put back by the outer arm's own
        // store. Here messages are queued rather than re-entrant, so there is no outer frame to do it and the
        // flag would stay `false` for ever — swallowing the next genuine press if this were
        // unconditional.
        //
        // So the flag is cleared only when a `4` is going to come back for it, which is precisely
        // when the selection moves. Same observable as retail in both cases, no stuck state.
        if selected_item(ui, list) != Some(row) {
            self.broadcast_selection = false;
        }
        dereth_ui::widgets::listbox::set_selected_item_of(ui, list, Some(row), true);
        self.selected_object = Some(id);
        self.selection_notices += 1;
        true
    }

    /// The three steps both clear arms share.
    ///
    /// Only reached with something to clear (see the guard at the top of
    /// [`Self::on_selection_changed`]), so clearing the list selection always takes the full path
    /// and always raises the `4` that restores the suppressor — with index `0xFFFFFFFF`, not the
    /// 0 that would skip it.
    fn clear_list_selection(&mut self, ui: &mut UiSystem, list: ElemHandle) -> bool {
        if selected_item(ui, list).is_some() {
            self.broadcast_selection = false;
        }
        dereth_ui::widgets::listbox::set_selected_item_of(ui, list, None, true);
        self.selected_object = None;
        self.selection_notices += 1;
        true
    }
}

/// The component list box's selected item — read off the widget, because that is where retail
/// reads it and because a row `set_selected_item` could not find must not be pickable.
fn selected_item(ui: &UiSystem, list: ElemHandle) -> Option<ElemHandle> {
    ui.node(list)
        .and_then(|n| n.behaviour.as_ref())
        .and_then(|b| (**b).as_any())
        .and_then(|a| a.downcast_ref::<dereth_ui::widgets::listbox::ListBox>())
        .and_then(|l| l.selected.and_then(|i| l.get_item(i)))
}

/// The category header's text: the token resolved in string table `0x10000001`.
///
/// Falls back to the token itself when the string table is not installed, so a headless test reads
/// a stable, greppable value instead of an empty header — and so an unresolved token is visible
/// rather than silent.
fn category_title(ui: &UiSystem, category: u32) -> String {
    let token = CATEGORY_TITLES
        .get(category as usize)
        .copied()
        .unwrap_or("");
    ui.resolve_string(
        crate::chat::mainchat::CAPTION_STRING_TABLE,
        dereth_primitives::num::hash::str_hash(token.as_bytes()),
    )
    .unwrap_or_else(|| token.to_owned())
}

fn set_child_text(ui: &mut UiSystem, row: ElemHandle, child: u32, text: &str) {
    if let Some(t) = ui
        .get_child_recursive(row, ElementId(child))
        .and_then(|c| ui.text_element_mut(c))
    {
        t.set_text(text);
    }
}

/// The component update's four writes per row, in its own order: name, icon, owned count, desired.
fn write_row(ui: &mut UiSystem, h: ElemHandle, r: &ComponentRow) {
    set_child_text(ui, h, row::NAME, &r.name);
    if let Some(icon) = r.icon {
        if let Some(c) = ui.get_child_recursive(h, ElementId(row::ICON)) {
            if let Some(n) = ui.node_mut(c) {
                n.region.image = Some(component_icon(icon));
            }
        }
    }
    set_child_text(ui, h, row::OWNED, &r.owned.to_string());
    set_child_text(ui, h, row::DESIRED, &r.desired.to_string());
    // **The component update's own numeric input filter, installed per row.**
    //
    // Find child `0x1000046B` recursively under the row and cast it to type `0x0C`.
    // Install the numeric input filter only when the cast succeeds.
    //
    // It is inside the per-component loop, not in the post-init, because the rows are spliced from a
    // list-box template and a row that has just been created carries no filter.
    if let Some(c) = ui.get_child_recursive(h, ElementId(row::DESIRED)) {
        dereth_ui::text::set_input_filter(ui, c, dereth_ui::text::number_input_filter);
    }
}

/// A component's icon as the magic window draws it: the component table's icon, read from the
/// world's files, with its opaque white outline turned opaque black. Wherever the magic window
/// shows a component (the Components tab's rows, the Create Spell page's slots) it draws this.
#[must_use]
pub fn component_icon(icon: dereth_primitives::DataId) -> dereth_ui::GraphicRef {
    use dereth_ui::region::SurfaceOp;
    let mut g = dereth_ui::GraphicRef::world_surface(icon, 0, 0);
    g.op = Some(SurfaceOp::ReplaceColor {
        from: SurfaceOp::OPAQUE_WHITE,
        to: SurfaceOp::OPAQUE_BLACK,
    });
    g
}

/// The icon a row draws — the client's input.
///
/// Defined in [`dereth_client_contract::panels::spellcomponent`], because
/// `dereth_client_shell::hud` is what composes the rows.
pub use dereth_client_contract::panels::spellcomponent::row_icon;

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::DataId;

    /// Oracle: the client's seven category-title assignments, in order, and the
    /// spell-component category enum's order.
    ///
    /// The tokens are **literals** here rather than derived from the category names, because a
    /// test that generated them from the enum could not detect a wrong token.
    #[test]
    fn the_seven_category_titles_are_the_tokens_post_init_assigns() {
        assert_eq!(CATEGORY_TITLES.len(), 7);
        assert_eq!(CATEGORY_TITLES[0], "ID_SpellComp_Category_Scarabs");
        assert_eq!(CATEGORY_TITLES[1], "ID_SpellComp_Category_Herbs");
        assert_eq!(CATEGORY_TITLES[2], "ID_SpellComp_Category_Gems");
        assert_eq!(CATEGORY_TITLES[3], "ID_SpellComp_Category_Alchemical");
        assert_eq!(CATEGORY_TITLES[4], "ID_SpellComp_Category_Talismans");
        assert_eq!(CATEGORY_TITLES[5], "ID_SpellComp_Category_Tapers");
        assert_eq!(CATEGORY_TITLES[6], "ID_SpellComp_Category_Peas");
    }

    /// Oracle: the element ids read out of and
    /// The element-message handler, as literals.
    ///
    /// `0x1000004C` is asserted to be the **WCID** attribute; `panels::rows` used to describe it as
    /// the icon and the two must not drift apart again.
    #[test]
    fn the_row_element_ids_are_the_ones_the_panel_reads_and_writes() {
        assert_eq!(COMPONENT_LIST, ElementId(0x1000_0464));
        assert_eq!(HEADER_ELEMENT, ElementId(0x1000_0466));
        assert_eq!(ROW_ELEMENT, ElementId(0x1000_0467));
        assert_eq!(row::ICON, 0x1000_0468);
        assert_eq!(row::NAME, 0x1000_0469);
        assert_eq!(row::OWNED, 0x1000_046A);
        assert_eq!(row::DESIRED, 0x1000_046B);
        assert_eq!(ATTR_CATEGORY, 0x1000_004D);
        assert_eq!(ATTR_COMPONENT_WCID, 0x1000_004C);
        assert_eq!(
            crate::panels::rows::row_attribute("SpellComponentPanel"),
            Some(ATTR_COMPONENT_WCID)
        );
        assert_eq!(MAX_DESIRED_LEVEL_EXCLUSIVE, 5001, "0x1389");
        assert_eq!(HEADER_TEMPLATE, 0);
        assert_eq!(ROW_TEMPLATE, 1);
    }

    /// Oracle: — an id of 0 is no icon.
    #[test]
    fn a_zero_icon_id_is_no_icon() {
        assert_eq!(row_icon(0), None);
        assert_eq!(row_icon(0x0600_1234), Some(DataId(0x0600_1234)));
    }

    /// An unbound panel answers **0 templates and not-bound**, rather than looking like a player
    /// who owns nothing.
    #[test]
    fn an_unbound_panel_says_so() {
        let p = SpellComponentPanel::default();
        assert!(!p.bound());
        assert_eq!(p.templates(), 0);
        assert_eq!(p.rebuilds, 0);
        assert!(p.shown().is_empty());
    }
}
