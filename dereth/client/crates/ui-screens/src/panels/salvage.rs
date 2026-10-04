//! The **salvage window**: the item list a tinkering tool opens, the Salvage
//! button, and the `0x027D` it sends.
//!
//! Four pieces make the window, and this module joins them:
//!
//! | piece | where |
//! |---|---|
//! | the open-panel notice | `use_object`'s `UseResult::Salvage` arm emits `Notice::OpenSalvagePanel(tool)` |
//! | the salvage model | `dereth_client_model::inventory::salvage` |
//! | `0x02B4 Inventory_SalvageOperationsResultData` | decoded in `dereth_protocol::items`, received by `Hud::ui_event` |
//! | salvage panel | an element type, a shipped layout, a `panels::catalogue` row and this module |
//!
//! A receiver whose field nothing reads is as good as no receiver; this module is the reader.
//!
//! # The shipped tree, measured
//!
//! The salvage instance is **`0x1000005E`** — the **second** of the environment panel's five pages
//! ([`crate::panels::catalogue::ENV_PANEL_PAGES`]), sitting between the external-container page
//! (`0x1000005D`) and secure-trade page (`0x1000005F`), all five at the same box
//! `(100, 405)-(699, 514)`. Its three children are all live:
//!
//! | id | type | box | what |
//! |---|---|---|---|
//! | `0x10000074` | item-list type `0x10000031` | `(110,445)-(609,476)` | salvage list |
//! | `0x10000076` | `0x00000001` button | `(632,449)-(695,470)` | the Salvage button |
//! | `0x10000078` | `0x00000001` button | `(676,405)-(697,424)` | the close X in the frame corner |
//!
//! [measured on the shipped layout]
//!
//! # Two corrections to `panels::catalogue`, both verified against retail
//!
//! 1. **The panel registers FOUR notices, not three:** open the salvage panel, add a salvage
//!    item, **remove a salvage item** and the item list's begin-drag. An earlier reading of the
//!    panel listed the open, add and begin-drag notices and dropped remove-item, which is the one
//!    that takes a row back *out* of the window.
//! 2. **It switches on three element messages, not one:**
//!    `0x1C`, `1`, and `0x15`. The catalogue carried only
//!    `&[1]`.
//!
//! # Five more readings of retail's behaviour
//!
//! 3. **A row is removed by a *double* click, not by a click.** The matching action is
//!    the left **double**-click (input map 3 binds `DIMOFS_BUTTON0` again with activation
//!    `MouseDblClick`, and `is_better_match` prefers the larger activation). A
//!    single press on a row in the salvage window does **nothing at all**.
//! 4. **The button has two states, `1` and `0x0D`.** Opening, closing, or removing the last row
//!    sets `0x0D` (disabled); adding the *first* row sets
//!    **state `1`**. No other button state is used here.
//! 5. **Closing clears the rows and tool id.** A user close ends the shared session;
//!    rebuilding an interface only replaces its presentation.
//! 6. **The material is latched on the first row only.** The write occurs when the list holds
//!    exactly one item, so with *SalvageMultiple* off the window locks to
//!    whatever went in first and refuses every other material until it empties.
//! 7. **Dropping a container adds its contents recursively.** A nonempty container follows
//!    the contained-item path, which prints the
//!    container's own name on channel `0x1A` and then walks its contained-items list — the loose
//!    items, recursing into any that are themselves containers.
//!    The accept path lets a container through **without** the suitability test,
//!    because the suitability test belongs to the leaves.
//!
//! # What drives it, and why it is not a notice bus
//!
//! The client registers four triggers during post-initialization: open, add, remove, and
//! item-list-begin-drag notices.
//! This build has no notice bus at
//! that seam, so the first three arrive as [`SalvageNotice`] through
//! `dereth_client::hud::Hud::pending_salvage` — the same one-frame hop
//! [`crate::panels::external_container::ExternalContainerNotice`] takes, and for the same reason.
//!
//! # Shard safety
//!
//! **No datagram leaves this process.** The Salvage button appends a [`UiRequest::SalvageList`]
//! to the thread-local outbox and the host is what puts `0x027D` on the wire — and
//! `create_tinkering_tool` repeats both of the client's guards.

use dereth_primitives::ObjectId;
use dereth_ui::{ElemHandle, ElementId, ElementMessage, UiSystem};

use crate::items::widget::{ItemListWidget, SlotInfo, TileInfo};
use crate::view::{GameView, UiRequest};

/// The registered salvage element-class id,
/// also returned by the element-type query.
pub const ELEMENT_CLASS: u32 = 0x1000_0011;

/// The salvage instance in the shipped `classic_gameplay` tree — the environment panel's second
/// page. [measured on the shipped layout]
pub const PANEL: ElementId = ElementId(0x1000_005E);

/// The salvage list — recursive child `0x10000074`, an item-list element.
pub const LIST: ElementId = ElementId(0x1000_0074);
/// The Salvage button — recursive child `0x10000076`.
pub const SALVAGE_BUTTON: ElementId = ElementId(0x1000_0076);
/// The client's other message-1 id — the close X.
pub const CLOSE_BUTTON: ElementId = ElementId(0x1000_0078);

// The drag hint writes states `0x10000040` and `0x10000041`, which are
// [`crate::items::widget::drag_accept_state`]'s `StateId`s — the ones
// `ItemSlot::set_drag_accept_state` takes. There is deliberately no local copy: a second
// spelling of a constant is the same defect as a second copy of a function.

/// `p1 == 10` — the left **double**-click, the only gesture that takes a row
/// out of the window. See reading 3 in this module's header.
pub const REMOVE_ACTION: u32 = 0x0A;

/// The display-string notice's channel `0x1A` — the channel both of this panel's strings go
/// out on.
pub const NOTICE_CHANNEL: u32 = 0x1A;

/// The salvage drag-acceptability predicate's refusal string, as retail prints it.
pub const NOT_YOURS: &str = "You can only salvage items that you own!";

/// The Salvage button's two states. See reading 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonState {
    /// `0x0D` — no rows. Written by opening, closing and the item removal's emptied arm.
    #[default]
    Disabled = 0x0D,
    /// `1` — at least one row. Written on the transition to exactly one.
    Enabled = 1,
}

use dereth_client_contract::panels::salvage::SalvageAction;
/// The three notices the panel registers that carry an object, in registration order.
///
/// Defined in [`dereth_client_contract::panels::salvage`], because it is the value
/// `dereth_client::hud` queues and this panel drains.
pub use dereth_client_contract::panels::salvage::SalvageNotice;

/// The snapshot [`SalvagePanel::update`] guards on, so an unchanged frame redraws nothing.
#[derive(Debug, Clone, PartialEq)]
struct ListSnapshot {
    tiles: Vec<(
        ObjectId,
        Option<dereth_primitives::DataId>,
        Option<TileInfo>,
    )>,
}

/// The salvage panel, bound to a live tree.
#[derive(Debug, Default)]
pub struct SalvagePanel {
    /// The salvage panel element — [`PANEL`], and the element whose visibility is changed.
    ///
    /// Bound by **id** and then confirmed by type, rather than by walking up from the list to the
    /// first [`ELEMENT_CLASS`] ancestor the way [`crate::panels::trade`] must: the panel's
    /// post-init never names the window either, but unlike secure trade this one *is* one of
    /// the environment panel's five declared pages, so it has an id the client itself uses
    /// (the environment panel's child set-up).
    pub root: Option<ElemHandle>,
    /// The salvage list.
    pub list: Option<ItemListWidget>,
    /// The Salvage button.
    pub salvage_button: Option<ElemHandle>,
    /// The close X — bound because the element-message handler switches on it; never written.
    pub close_button: Option<ElemHandle>,
    /// The tool id in the last shared projection.
    pub tool: Option<ObjectId>,
    /// The shared material latch in the last projection.
    pub material: u32,
    /// The Salvage button's state as this panel last wrote it.
    pub button: ButtonState,
    /// The last displayed projection; gestures never mutate these rows.
    pub displayed: Vec<ObjectId>,
    /// Whether the last [`Self::update`] left the window visible.
    pub visible: bool,

    // ---- counters, each three-state on purpose: "never driven" must not read as "drove and did
    // ---- nothing". -------------------------------------------------------------------------
    /// How many open-salvage-panel notices were honoured.
    pub opens: u32,
    /// How many times the close ran — the hidden edge plus a re-open.
    pub closes: u32,
    /// How many times the list was refilled.
    pub rebuilds: u32,
    /// Rows the item add actually inserted.
    pub items_added: u32,
    /// Rows the item removal actually took out.
    pub items_removed: u32,
    /// Drops the drag-acceptable test refused.
    pub drops_refused: u32,
    /// Salvage-button presses that put a request in the outbox.
    pub salvages: u32,
    /// Slots the last rebuild ran the tile decoration over. A window with rows and
    /// `slots_decorated == 0` is a defect, and must not read the same as an empty window.
    pub slots_decorated: usize,

    last: Option<ListSnapshot>,
}

impl SalvagePanel {
    /// The salvage panel's post-init, minus the four notice registrations: find the item list
    /// `0x10000074` and register the panel's item-list drag handler on it, then find the Salvage
    /// button `0x10000076`.
    ///
    /// Bound off the screen **root**, like every other `<ENVP>` window, and the window is then
    /// hidden: the environment panel's set-up ends by hiding all five pages, so a freshly built
    /// tree has no salvage window up.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        *self = Self::default();
        let Some(page) = ui.get_child_recursive(root, PANEL) else {
            return;
        };
        // Confirm the id is the class the client registered, so a layout change that moved the
        // panel reads as unbound rather than as bound to the wrong element.
        if ui.node(page).is_some_and(|n| n.ty().0 != ELEMENT_CLASS) {
            return;
        }
        self.root = Some(page);
        self.list = ui
            .get_child_recursive(page, LIST)
            .map(|h| ItemListWidget::init(ui, h));
        self.salvage_button = ui.get_child_recursive(page, SALVAGE_BUTTON);
        self.close_button = ui.get_child_recursive(page, CLOSE_BUTTON);
        self.write_button_state(ui);
        ui.set_visible(page, false);
    }

    /// True once the item list was found — the binding without which nothing can be drawn.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.list.is_some()
    }

    /// The UI-item element-message handler's `0x21` arm, followed by
    /// the salvage panel's item-list-begin-drag notice.
    ///
    /// The first half asks the pressed slot's parent item list to begin its own drag;
    /// the second confirms that the notice came from the salvage list, resolves the dragged row's
    /// index back to the row, and calls [`Self::remove_item`]. The removal therefore happens only after
    /// the client has passed `UI_ItemList_AllowDragging` and prepared its
    /// proxy. A click removes nothing, because it never crosses the drag threshold and so raises
    /// no `0x21` at all.
    ///
    /// **It does not wait on drag startup succeeding.** The client's return is
    /// discarded and both notices go out regardless, so a start that the manager
    /// refused still removes the row and leaves nothing on the cursor. Nothing a player can do
    /// reaches that refusal; see [`crate::items::widget::ItemListWidget::begin_drag`] for the arm
    /// table.
    pub fn begin_item_drag(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
    ) -> Option<crate::items::widget::DragStart> {
        if m.id != dereth_ui::msg::element::id::DRAG_REJECTED {
            return None;
        }
        let started = {
            let list = self.list.as_mut()?;
            crate::items::widget::begin_drag_from_rejected(
                ui,
                &mut [list],
                m.source,
                m.point.window.0,
                m.point.window.1,
            )
        };
        if let Some(item) = started.as_ref().and_then(|drag| drag.item) {
            self.remove_item(ui, item);
        }
        started
    }

    /// The three notices, delivered synchronously as retail's are.
    pub fn recv_notice(&mut self, ui: &mut UiSystem, notice: SalvageNotice, view: &dyn GameView) {
        if matches!(notice, SalvageNotice::Open(_)) {
            self.opens += 1;
        }
        self.update(ui, view);
    }

    /// Raise the presentation after the shared model consumes the open notice.
    pub fn open_salvage_panel(&mut self, ui: &mut UiSystem, tool: ObjectId) {
        let _ = tool;
        self.opens += 1;
        self.set_visible(ui, true);
    }

    /// End the shared salvage session after an explicit close.
    pub fn close_salvage_panel(&mut self, ui: &mut UiSystem) {
        ui.requests
            .emit(UiRequest::SalvageList(SalvageAction::Close));
        self.closes += 1;
    }

    /// The new-item add — the container fork. See reading 7. An unknown object is refused; an
    /// object that contains items goes to `Self::add_contained_items`, anything else to the
    /// single-item add.
    pub fn add_new_item(&mut self, ui: &mut UiSystem, id: ObjectId, view: &dyn GameView) -> bool {
        if view.slot_decoration(id).is_none() {
            return false;
        }
        ui.requests
            .emit(UiRequest::SalvageList(SalvageAction::Add(id)));
        true
    }

    /// The item removal: clear the object's trade state, delete its row, and when the list is
    /// then empty zero the material and set the Salvage button to `0x0D`.
    ///
    /// **The material is cleared only when the list empties**, not on every removal — so taking
    /// one of two iron rings out leaves the window locked to iron, which is right.
    pub fn remove_item(&mut self, ui: &mut UiSystem, id: ObjectId) {
        ui.requests
            .emit(UiRequest::SalvageList(SalvageAction::Remove(id)));
    }

    /// The drag-acceptability test, in retail's order: refuse an unknown object; refuse an object
    /// the player does not own, printing [`NOT_YOURS`] on channel `0x1A` unless `quiet`; refuse
    /// with no list or an item already listed; accept a container outright (it skips the
    /// suitability test); otherwise answer the suitability test.
    ///
    /// `quiet` is `true` for the hover and `false` for the drop, which is why hovering a stranger's item over the
    /// window says nothing and dropping it complains once.
    #[must_use]
    pub fn drag_item_acceptable(
        &self,
        requests_out: &mut crate::requests::Outbox,
        id: ObjectId,
        view: &dyn GameView,
        quiet: bool,
    ) -> bool {
        if view.slot_decoration(id).is_none() {
            return false;
        }
        if !view.item_owned_by_player(id) {
            if !quiet {
                requests_out.emit(UiRequest::DisplayChatText {
                    feedback: dereth_client_contract::feedback::Feedback::WARNING,
                    channel: NOTICE_CHANNEL,
                    text: NOT_YOURS.to_owned(),
                });
            }
            return false;
        }
        if self.list.is_none() || self.displayed.contains(&id) {
            return false;
        }
        if view
            .slot_decoration(id)
            .is_some_and(|d| d.contained_items > 0)
        {
            return true;
        }
        view.salvage_item_suitable(id, self.material)
    }

    /// The client's decision — which of the two states the tile
    /// under the drag cursor is given. [`Self::on_drag_cursor_over`] is what calls it.
    ///
    /// `quiet` is `true` here, so a stranger's item hovered over the
    /// window is refused **silently**; the drop is the arm that speaks.
    #[must_use]
    pub fn drag_accept_state(
        &self,
        requests_out: &mut crate::requests::Outbox,
        id: ObjectId,
        view: &dyn GameView,
    ) -> dereth_ui::StateId {
        use crate::items::widget::drag_accept_state;
        if self.drag_item_acceptable(requests_out, id, view, true) {
            drag_accept_state::ACCEPT
        } else {
            drag_accept_state::REFUSE
        }
    }

    /// The client's **`0x3E` arm** for the one list
    /// registered with an item-list drag handler — the drag hint over the salvage window.
    ///
    /// This is what delivers a `0x3E` to [`Self::drag_accept_state`], in the same shape
    /// [`crate::panels::vendor::VendorPanel::on_drag_cursor_over`] has:
    ///
    /// on message `0x3E` the item tile clears its drag-accept state to `0x1000003F` when `p1` is 0
    /// or no drag is in progress; otherwise it hands the drag to its parent item list, which reads
    /// the drop icon's item id, spell id and flags and offers them to the list's drag handler.
    ///
    /// The salvage panel's own item-list drag-over then returns true with no hint when the item id
    /// is 0 or any of the not-an-inventory-move flags `0x0E` is set; otherwise it sets the tile to
    /// `0x10000040` when the quiet acceptability test passes and `0x10000041` when it does not, and
    /// **always** returns true.
    ///
    /// Three things worth being exact about:
    ///
    /// * **It always returns true**, so once the cursor is over a salvage-list tile
    ///   the item list's own three-way default (`ACCEPT` / `REFUSE` / `INTO_CONTAINER`) can
    ///   never run — which is what stops an empty salvage row painting the plain green cross for
    ///   an item the window is about to refuse.
    /// * **The alias mask is `0x0E`, not one bit** (`IS_VENDOR | IS_SHORTCUT | IS_SALVAGE`,
    ///   [`crate::items::widget::drag_flags::NOT_AN_INVENTORY_MOVE`]), and on that arm the tile
    ///   keeps **whatever state it already had**: a spell or a shortcut dragged over the window
    ///   gets no hint rather than a red one.
    /// * **The accept test is the drop's test**, so the hint and
    ///   the drop cannot disagree — including the material lock, which is why hovering a
    ///   second material over a locked window shows **red** and not green.
    ///
    /// Returns true when the message was this window's list, which is what
    /// `RemainingPanels::on_element_message`'s fan-out reads.
    pub fn on_drag_cursor_over(
        &mut self,
        ui: &mut UiSystem,
        m: &ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        use crate::items::widget::{drag_accept_state, drag_flags, inq_drop_icon_info};
        if m.id != dereth_ui::msg::element::id::DRAG_CURSOR_OVER {
            return false;
        }
        let Some(slot) = self.list.as_ref().and_then(|w| w.slot_of(m.source)) else {
            return false;
        };
        // Both of the client's clearing routes, neither of which reaches the handler: `p1 == 0`
        // (the cursor left), and `p1 != 0` with no drag element at all.
        let Some(proxy) = ui.drag_state().element.filter(|_| m.p1 != 0) else {
            if let Some(w) = self.list.as_mut() {
                w.slots[slot].set_drag_accept_state(ui, drag_accept_state::NONE);
            }
            return true;
        };
        let info = inq_drop_icon_info(ui, proxy);
        let Some(item) = info
            .item
            .filter(|_| info.flags & drag_flags::NOT_AN_INVENTORY_MOVE == 0)
        else {
            // The client returns true without setting any drag-accept state.
            return true;
        };
        let state = self.drag_accept_state(&mut ui.requests, item, view);
        if let Some(w) = self.list.as_mut() {
            w.slots[slot].set_drag_accept_state(ui, state);
        }
        true
    }

    /// The client's **`0x15` arm** — the clear that
    /// takes [`Self::on_drag_cursor_over`]'s hint back down when the drop lands — the same half
    /// the vendor's three lists and the quickbar's eighteen have.
    ///
    /// On message `0x15`, clear the catcher element's drag-accept state to `0x1000003F`
    /// if it casts to UI-item type `0x10000032`, then handle the drop release. The catcher
    /// receives the state write, not the list.
    ///
    /// It is **unconditional** — the message-id check is the whole guard — so it runs whether the
    /// drop was taken or refused, and it runs before the drop is handled. There is no `0x3F`
    /// drag-leave message in the client, and the pointer never leaves the
    /// tile it was dropped on, so neither of the client's clearing routes fires on a drop: this
    /// arm is the only producer of the clear on that path.
    pub fn on_drop_release(&mut self, ui: &mut UiSystem, m: &ElementMessage) -> bool {
        use crate::items::widget::drag_accept_state;
        // The **catcher's** copy of the message, which is the one carrying the drag's owner in
        // `p2`; the owner's own copy has `p2 == 0` and names a slot in the pack. Same reading
        // `VendorPanel::on_drop_release` takes.
        if m.id != dereth_ui::msg::element::id::DROP_FAILED || m.p2 == 0 {
            return false;
        }
        let Some(slot) = self.list.as_ref().and_then(|w| w.slot_of(m.source)) else {
            return false;
        };
        if let Some(w) = self.list.as_mut() {
            w.slots[slot].set_drag_accept_state(ui, drag_accept_state::NONE);
        }
        true
    }

    /// The drop acceptance: refuse unless the loud acceptability test passes; otherwise add the
    /// object through [`Self::add_new_item`] (when the list exists) and answer true.
    ///
    /// Note the **`true` with no insert**: an acceptable drop whose new-item add then refuses every
    /// leaf still answers accepted, which is what stops the list's own unghost arm from running.
    pub fn accept_drag_object(
        &mut self,
        ui: &mut UiSystem,
        id: ObjectId,
        view: &dyn GameView,
    ) -> bool {
        if !self.drag_item_acceptable(&mut ui.requests, id, view, false) {
            self.drops_refused += 1;
            return false;
        }
        if self.list.is_some() {
            self.add_new_item(ui, id, view);
        }
        true
    }

    /// Submit the shared rows. The model reverses their order and retains the tool.
    pub fn salvage(&mut self, ui: &mut UiSystem) -> bool {
        if self.list.is_none() || self.tool.is_none() || self.displayed.is_empty() {
            return false;
        }
        ui.requests
            .emit(UiRequest::SalvageList(SalvageAction::Submit));
        self.salvages += 1;
        true
    }

    /// The salvage panel's element-message handler — all three arms. On `0x1C` from inside the
    /// salvage list, a `p1 == 10` press on a UI-item row (type `0x10000032`) removes that row's
    /// item. On message `1`, `0x10000078` hides the window and `0x10000076` salvages. On `0x15`
    /// the drop release is handled.
    ///
    /// The `0x15` arm is not here: [`crate::screens::gameplay::GamePlayScreen`] is the element the
    /// drop is delivered to in this build and it records the drop for the host to hand to
    /// [`Self::accept_drag_object`], exactly as for the secure-trade and vendor panels.
    ///
    /// **The gate is narrow in both arms**, as its five predecessors' are: the `0x1C` arm refuses
    /// any source that is not one of this list's own slots, and the `1` arm refuses any id but
    /// `0x10000076` and `0x10000078`.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &ElementMessage,
        _view: &dyn GameView,
    ) -> bool {
        use dereth_ui::msg::element::id as msg;
        if m.id == msg::MOUSE_PRESS {
            if m.p1 != REMOVE_ACTION {
                return false;
            }
            let Some(item) = self.item_under(ui, m.source) else {
                return false;
            };
            self.remove_item(ui, item);
            return true;
        }
        if m.id != msg::BUTTON_CLICKED {
            return false;
        }
        match m.source_id {
            CLOSE_BUTTON => {
                self.close_salvage_panel(ui);
                self.set_visible(ui, false);
                true
            }
            SALVAGE_BUTTON => {
                self.salvage(ui);
                true
            }
            _ => false,
        }
    }

    /// The item under the press, plus the UI-item type check — which row of **this** list the
    /// pressed element belongs to.
    ///
    /// The handle walk is this build's and not the client's: the element-message handler runs *on* the
    /// list in retail, so "which list" is never a question there. It walks ancestors because a
    /// press is delivered from the deepest mouse-visible element, which for a filled slot is the
    /// slot itself and for some layouts a child of it.
    fn item_under(&self, ui: &UiSystem, source: ElemHandle) -> Option<ObjectId> {
        let w = self.list.as_ref()?;
        let mut h = Some(source);
        while let Some(cur) = h {
            if let Some(i) = w.slot_of(cur) {
                return w.item_at(i);
            }
            if w.handle == cur {
                return None;
            }
            h = ui.parent(cur);
        }
        None
    }

    /// Project the shared session and refresh changed item tiles.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let state = view.salvage_list();
        self.tool = state.tool;
        self.material = state.material;
        self.items_added += u32::try_from(
            state
                .items
                .iter()
                .filter(|id| !self.displayed.contains(id))
                .count(),
        )
        .unwrap_or(u32::MAX);
        self.items_removed += u32::try_from(
            self.displayed
                .iter()
                .filter(|id| !state.items.contains(id))
                .count(),
        )
        .unwrap_or(u32::MAX);
        self.displayed = state.items;
        self.button = if self.tool.is_some() && !self.displayed.is_empty() {
            ButtonState::Enabled
        } else {
            ButtonState::Disabled
        };
        self.write_button_state(ui);
        if self.visible != state.visible {
            self.set_visible(ui, state.visible);
        }
        let ids = self.displayed.clone();
        let same = self.last.as_ref().is_some_and(|s| {
            s.tiles.len() == ids.len()
                && s.tiles.iter().zip(&ids).all(|((old, icon, tile), id)| {
                    old == id
                        && *icon == view.icon(*id)
                        && TileInfo::matches(view, *id, tile.as_ref())
                })
        });
        if same {
            return false;
        }
        let snapshot = ListSnapshot {
            tiles: ids
                .iter()
                .map(|id| (*id, view.icon(*id), TileInfo::read(view, *id)))
                .collect(),
        };
        if let Some(w) = self.list.as_mut() {
            // The shipped layout gives `0x10000074` no `UI_ItemList_FixedListSize`, so the list is
            // unbounded -- the same reading `TradePanel::fill` and `VendorPanel::fill` take.
            w.set_contents(ui, None, Some(-1), &ids, &|id| view.icon(id));
            let now = ui.now.0;
            let info = |id: ObjectId| -> Option<SlotInfo> {
                snapshot
                    .tiles
                    .iter()
                    .find(|t| t.0 == id)
                    .and_then(|t| t.2.as_ref())
                    .map(|t| t.to_slot_info(view, now))
            };
            self.slots_decorated = w.decorate(ui, &info);
        }
        self.last = Some(snapshot);
        self.rebuilds += 1;
        true
    }

    /// Write the Salvage button's state. See reading 4.
    fn write_button_state(&mut self, ui: &mut UiSystem) {
        if let Some(h) = self.salvage_button {
            ui.set_state(h, dereth_ui::StateId(self.button as u32));
        }
    }

    fn set_visible(&mut self, ui: &mut UiSystem, visible: bool) {
        self.visible = visible;
        if let Some(h) = self.root {
            ui.set_visible(h, visible);
        }
    }
}
