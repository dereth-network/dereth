//! `HousingPanel` — the **house purchase / maintenance window**, and the two requests it sends.
//!
//! Interacting with a slumlord opens this window.
//!
//! # What raises it, and what the client must send
//!
//! Both halves are in a recorded retail housing session:
//!
//! | recorded housing session | dir | blob |
//! |---|---|---|
//! | `t = 29.080` | c2s | `0xF7B1` ordered `0x0036 Inventory_UseEvent`, body `3af0da79` |
//! | `t = 29.090` | s2c | `0xF7B0` event `0x021D House_HouseProfile`, 260 bytes, first dword `3af0da79` |
//!
//! `0x79DAF03A` is the slumlord statue of the villa at `0x9DAF0029` — a landblock-static guid,
//! `0x7` + landblock `0x9DAF` + index `0x03A`. So:
//!
//! * **What makes retail RAISE the window** is the *arrival of `0x021D`*. The update-house-profile
//!   notice shows the housing page. No other path shows it, and the panel registers **no**
//!   open-panel notice — its five post-initialization registrations are the
//!   update-house-profile, failed-house-transaction, item-list-begin-drag, server-move-item and
//!   close-dialog notices.
//! * **What the client must SEND** is an ordinary **use** on the slumlord. The server answers a
//!   use on a slumlord with the house profile, and that is the *only* producer of `0x021D`.
//!
//! The use is `dereth_client_model::inventory::use_object`'s ordinary `0x0036`; this module and
//! the `0x021D` receiver are the rest.
//!
//! # The query-lord request (`0x0258`) is a retry, not an opener
//!
//! Retail sends the query-lord request from **one** place only: the slumlord panel's
//! failed-house-transaction notice, which, when a house profile is held, re-queries the slumlord
//! the profile came from. A refused `0x021C`/`0x0221` comes back as
//! `0x0226`/`0x0259`, and the window re-asks the lord so its paid counts come back in step.
//! Wiring `0x0258` to a *use* would be inventing a behaviour.
//!
//! # The shipped tree, and two corrections to `panels::catalogue`
//!
//! `HousingPanel` is element class `0x10000013` and the instance is
//! **`0x10000060`**, the *fourth* of `EnvironmentPanelStack`'s five pages
//! ([`crate::panels::catalogue::ENV_PANEL_PAGES`]) — the same family `ExternalContainerPanel`
//! (`0x1000005D`), `SalvagePanel` (`0x1000005E`) and `TradePanel` (`0x1000005F`) belong to.
//! [measured on the shipped layout]
//!
//! The client's eight child lookups, in its own order:
//!
//! | id | what | type |
//! |---|---|---|
//! | `0x10000091` | buy requirements text | `TextElement` |
//! | `0x10000093` | buy owner text | `TextElement` |
//! | `0x10000095` | buy item list | `ItemListWidget` (+ item-list drag handler) |
//! | `0x10000094` | Buy button | `Button` |
//! | `0x10000098` | rent requirements text | `TextElement` |
//! | `0x1000009A` | rent owner text | `TextElement` |
//! | `0x1000009C` | rent item list | `ItemListWidget` (+ handler) |
//! | `0x1000009B` | Rent button | `Button` |
//!
//! and `0x10000090` / `0x10000097` are the two **tab pages**, while `0x1000009E` is the close
//! button. The tab pages are used by the tab-switch messages, and the close button by the close
//! message handler.
//!
//! 1. **`panels::catalogue`'s row called `0x10000090` the close button.** It is not: the
//!    post-init never binds it, and the client's `0x18` arm treats it as the *Buy page*'s
//!    visibility. The close is `0x1000009E`. The row also had the four buy children under
//!    placeholder names; they have real roles and real types, above.
//! 2. **The row listed three notices and the post-init registers five**, and it listed element
//!    message `1` where the client switches on `1`, `0x15` and `0x18`. The two missing notices are
//!    the two that matter: the update-house-profile notice is what opens the window and the
//!    failed-house-transaction notice is what retries the query. The `SalvagePanel` row has the
//!    same shape.
//!
//! # The two tabs are a mode, and the mode is what every refusal turns on
//!
//! The current house operation is [`HouseOp`] `Undef` / `Buy` / `Rent`, written by the
//! element-message handler's **`0x18`** (visibility-changed) arm on `0x10000090` and
//! `0x10000097`. It selects which of `HouseProfile`'s two payment lists every operation is about
//! (`dereth_client_model::housing::HouseOp` carries the six dispatchers), and the payment-allowed test
//! adds the rule that makes the two tabs exclusive: with no profile, refuse; Buy allows only an
//! **unowned** house, Rent only an **owned** one; Undef takes **nothing**.
//!
//! So on a dwelling nobody owns the Rent tab refuses every drop, on an owned one the Buy tab does,
//! and with neither tab up the window refuses everything **silently** — the drag-acceptable
//! test's first check is that a current list exists, above the string.
//!
//! # Every refusal, with retail's own text
//!
//! All five strings are retail's exactly, with their lengths.
//! The two the player can provoke are [`NOT_CARRYING`] and [`CANNOT_SPLIT`]; both go out on
//! channel `0x1A` through the display-string notice.
//!
//! | width | chars | where |
//! |---|---|---|
//! | wide | 41 | the drag-acceptable test — not yours |
//! | wide | 41 | the drop acceptance — the split failed |
//! | wide | 47 | the drop acceptance — the split succeeded, `%s` is the name |
//! | narrow | 7 | the house refresh — `"Owner: "` |
//! | narrow | 4 | the same — `"None"` |
//!
//! # What this module does **not** do, named rather than left to be discovered
//!
//! * **The two confirmation dialogs.** The house-purchase prompt (*"When you buy
//!   a landscape house like this one…"*, 138 chars) and the proxy-payment prompt
//!   (*"You are paying maintenance on someone else's house…"*, 86 chars) both go through
//!   current-UI dialog creation, and their answers come back through
//!   the close-dialog notice → the buy-house confirmation close /
//!   the rent-payment-by-proxy confirmation close. **This build routes the confirmation
//!   arms straight to [`SlumlordPanel::make_payment`]** — i.e. the Yes arm — and says so in
//!   [`SlumlordPanel::on_element_message`]. The dialog itself is not implemented.
//! * **The stack split.** Drop acceptance forks on whether the split size equals the maximum
//!   split size; the *unequal* arm attempts to place the item in a container.
//!   The panel emits [`UiRequest::HouseSplitItem`] for
//!   that inventory operation. Its authoritative result is selected by the generic object-create
//!   path; retail does not automatically insert it here, so the player drops that whole result a
//!   second time.
//! * **Trade notes** are normalized through the retail `TradeNotes` two-way enum map by the host,
//!   and their face value is kept with the row so replay reaches the trade-note payment rather than
//!   mistaking a note for a separate payment class.
//! * **The nine-unit auto-close.** The house-profile notice handler ends by registering an
//!   object-range handler on the slumlord at 9.0 units, and the range-exit callback closes the
//!   window when the player walks away.
//!   `dereth_client_model::range` carries that row (`range.rs`'s table, entry 3); the profile
//!   receiver registers it and delivers its one-shot exit back to this panel.
//!
//! # Shard safety
//!
//! **No datagram leaves this process.** Both buttons append a [`UiRequest`] to the thread-local
//! outbox and the host is what puts `0x021C` / `0x0221` / `0x0258` on the wire.

use dereth_primitives::ObjectId;
use dereth_ui::{ElemHandle, ElementId, ElementMessage, UiSystem};

use crate::items::widget::{ItemListWidget, SlotInfo, TileInfo};
use crate::view::{GameView, SlumlordPayment, SlumlordView, UiRequest};

/// The registered element-class id, and what the element-type query returns.
pub const ELEMENT_CLASS: u32 = 0x1000_0013;

/// The `HousingPanel` instance in the shipped `classic_gameplay` tree — `EnvironmentPanelStack`'s fourth
/// page. [measured, `panels::house_purchase_window`]
pub const PANEL: ElementId = ElementId(0x1000_0060);

/// The **Buy** tab page. The client's `0x18` arm sets the current operation to
/// [`HouseOp::Buy`] when it becomes visible.
pub const BUY_PAGE: ElementId = ElementId(0x1000_0090);
/// The buy requirements text — the profile's buy requirements.
pub const BUY_REQUIREMENTS: ElementId = ElementId(0x1000_0091);
/// The buy owner text — `"Owner: "` + the name, or `"None"`.
pub const BUY_OWNER: ElementId = ElementId(0x1000_0093);
/// The Buy button.
pub const BUY_BUTTON: ElementId = ElementId(0x1000_0094);
/// The buy item list.
pub const BUY_LIST: ElementId = ElementId(0x1000_0095);
/// The **Rent** tab page.
pub const RENT_PAGE: ElementId = ElementId(0x1000_0097);
/// The rent requirements text — composed by the *second* text composer, **not** the buy one.
pub const RENT_REQUIREMENTS: ElementId = ElementId(0x1000_0098);
/// The rent owner text — the *same* string as [`BUY_OWNER`], written twice
/// (the client shares one string between them).
pub const RENT_OWNER: ElementId = ElementId(0x1000_009A);
/// The Rent button.
pub const RENT_BUTTON: ElementId = ElementId(0x1000_009B);
/// The rent item list.
pub const RENT_LIST: ElementId = ElementId(0x1000_009C);
/// The close X — the element-message handler's message-1 arm, a bare hide.
pub const CLOSE_BUTTON: ElementId = ElementId(0x1000_009E);

/// The display-string notice's channel `0x1A` — the channel both of this panel's refusals go
/// out on.
pub const NOTICE_CHANNEL: u32 = 0x1A;

/// The drag-acceptability refusal, **wide**, 41 characters.
///
/// It says *"trade"* and not *"house"*: the string is shared with the secure-trade window's own
/// sense of the rule. No full stop.
pub const NOT_CARRYING: &str = "You can only trade items you are carrying";

/// The drop-acceptance refusal, **wide**, 41 characters.
pub const CANNOT_SPLIT: &str = "Cannot split the stack for dwelling costs";

/// The drop-acceptance *success* line, **wide**, 47 characters,
/// with the object's wide name (name type 2) substituted.
pub const SPLITTING_THE: &str = "Splitting the %s before adding to housing panel";

/// The house refresh's owner line, narrow, 7 characters including the
/// trailing space.
pub const OWNER_PREFIX: &str = "Owner: ";
/// …and its empty-name arm, narrow, 4 characters. The test is that the profile's owner name
/// has length 1 counting the terminator, i.e. the string is empty.
pub const OWNER_NONE: &str = "None";

/// The house-purchase confirmation prompt, wide, 138 characters.
/// Not raised by this leg; see the module header.
pub const BUY_CONFIRMATION: &str = "When you buy a landscape house like this one, you are \
                                    restricted from buying another for 30 days. Are you sure you \
                                    want to buy this house?";
/// The rent payment by proxy confirmation prompt, wide, 86
/// characters. Note the apostrophe in *"else's"*.
pub const RENT_BY_PROXY_CONFIRMATION: &str =
    "You are paying maintenance on someone else's house. Are you sure you wish to continue?";

/// `HouseType::Apartment`, and the one value the Buy button branches on:
/// buying an apartment skips the thirty-day confirmation entirely.
pub const APARTMENT: u32 = 4;

/// The current house operation — which tab is up. The same value the housing model dispatches its
/// payment lists on; the contract owns it.
pub use dereth_client_contract::panels::slumlord::HouseOp;

/// The buy and rent buttons' two states: `1` for enabled and `0xD` for disabled, the same pair the
/// salvage panel uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonState {
    /// `0x0D` — disabled.
    #[default]
    Disabled = 0x0D,
    /// `1` — enabled.
    Enabled = 1,
}

/// One row the window holds, with the two numbers the payment replay needs.
///
/// Retail keeps only the object id in the list and re-reads the weenie for the rest; this keeps
/// the pair it read **at drop time**, because the client pays into its house profile
/// there and then, and a stack whose size changed afterwards must not silently re-price the
/// window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DroppedItem {
    pub id: ObjectId,
    /// The item's WCID.
    pub wcid: u32,
    /// The item's house-payment amount — its stack size, or 1 when it has none.
    pub amount: i32,
    /// The client's mapped face value, or `None` for an ordinary item.
    pub trade_note_value: Option<i32>,
}

/// The snapshot [`SlumlordPanel::update`] guards on, so an unchanged frame redraws nothing.
#[derive(Debug, Clone, PartialEq)]
struct Snapshot {
    profile: Option<SlumlordView>,
    op: HouseOp,
    buy: Vec<(
        ObjectId,
        Option<dereth_primitives::DataId>,
        Option<TileInfo>,
    )>,
    rent: Vec<(
        ObjectId,
        Option<dereth_primitives::DataId>,
        Option<TileInfo>,
    )>,
    buy_payment: SlumlordPayment,
    rent_payment: SlumlordPayment,
}

/// `HousingPanel`, bound to a live tree.
#[derive(Debug, Default)]
pub struct SlumlordPanel {
    /// The `HousingPanel` element — [`PANEL`], and the element that is shown and hidden.
    pub root: Option<ElemHandle>,
    pub buy_requirements: Option<ElemHandle>,
    pub buy_owner: Option<ElemHandle>,
    pub buy_button: Option<ElemHandle>,
    pub buy_list: Option<ItemListWidget>,
    pub rent_requirements: Option<ElemHandle>,
    pub rent_owner: Option<ElemHandle>,
    pub rent_button: Option<ElemHandle>,
    pub rent_list: Option<ItemListWidget>,
    /// Bound because the element-message handler switches on it; never written.
    pub close_button: Option<ElemHandle>,

    /// The slumlord's id + the profile, as [`GameView::slumlord`] last handed them over.
    pub profile: Option<SlumlordView>,
    /// The current house operation.
    pub op: HouseOp,
    /// The buy item list's contents.
    pub buy_items: Vec<DroppedItem>,
    /// The rent item list's contents.
    pub rent_items: Vec<DroppedItem>,
    /// The two buttons as this panel last wrote them.
    pub buy_button_state: ButtonState,
    pub rent_button_state: ButtonState,
    /// Whether the last [`Self::update`] left the window visible.
    pub visible: bool,
    /// Whether a house-purchase dialog is open, reduced to the one bit the panel itself needs.
    buy_confirmation_pending: bool,
    /// Whether a rent-by-proxy dialog is open, independent from the Buy one.
    rent_confirmation_pending: bool,
    /// The `0x021D` count this panel has already raised itself for.
    profile_notices_seen: u64,

    // ---- counters, three-state on purpose ---------------------------------------------------
    /// The update-house-profile notice's shows of the window.
    pub opens: u32,
    /// Times the window was hidden — the close button, and the hidden edge.
    pub closes: u32,
    /// Times the tree was rewritten.
    pub rebuilds: u32,
    /// Rows the item insert actually inserted.
    pub items_added: u32,
    /// Drops the drag-acceptable test or the payment insert refused.
    pub drops_refused: u32,
    /// Refusal strings this panel put in the scroll.
    pub refusals_spoken: u32,
    /// Payments that put a request in the outbox.
    pub payments: u32,
    /// The failed house transaction notice re-queries.
    pub requeries: u32,
    /// Slots the last rebuild decorated, over both lists.
    pub slots_decorated: usize,

    last: Option<Snapshot>,
}

impl SlumlordPanel {
    /// The slumlord panel's post-init, minus the five notice registrations.
    ///
    /// Bound off the screen **root** like every other `<ENVP>` window, and the window is then
    /// hidden: the environment panel's set-up ends by hiding all five pages, so a freshly built
    /// tree has no slumlord window up. That matters here more than it
    /// does for its neighbours, because *this* window's whole complaint was that it does not come
    /// up — an initially-visible one would hide the defect rather than fix it.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        *self = Self::default();
        let Some(page) = ui.get_child_recursive(root, PANEL) else {
            return;
        };
        if ui.node(page).is_some_and(|n| n.ty().0 != ELEMENT_CLASS) {
            return;
        }
        self.root = Some(page);
        self.buy_requirements = ui.get_child_recursive(page, BUY_REQUIREMENTS);
        self.buy_owner = ui.get_child_recursive(page, BUY_OWNER);
        self.buy_button = ui.get_child_recursive(page, BUY_BUTTON);
        self.buy_list = ui
            .get_child_recursive(page, BUY_LIST)
            .map(|h| ItemListWidget::init(ui, h));
        self.rent_requirements = ui.get_child_recursive(page, RENT_REQUIREMENTS);
        self.rent_owner = ui.get_child_recursive(page, RENT_OWNER);
        self.rent_button = ui.get_child_recursive(page, RENT_BUTTON);
        self.rent_list = ui
            .get_child_recursive(page, RENT_LIST)
            .map(|h| ItemListWidget::init(ui, h));
        self.close_button = ui.get_child_recursive(page, CLOSE_BUTTON);
        self.write_button_states(ui);
        ui.set_visible(page, false);
    }

    /// True once both item lists were found — the bindings without which nothing can be dropped.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.buy_list.is_some() && self.rent_list.is_some()
    }

    /// The list the current tab names — the current item list, which is written by the same
    /// `0x18` arm that writes the current operation and is empty for [`HouseOp::Undef`].
    #[must_use]
    pub fn current_items(&self) -> &[DroppedItem] {
        match self.op {
            HouseOp::Undef => &[],
            HouseOp::Buy => &self.buy_items,
            HouseOp::Rent => &self.rent_items,
        }
    }

    /// `(wcid, amount, trade-note value)` for one tab, which is what
    /// [`GameView::slumlord_payment`] replays.
    fn drops(items: &[DroppedItem]) -> Vec<(u32, i32, Option<i32>)> {
        items
            .iter()
            .map(|d| (d.wcid, d.amount, d.trade_note_value))
            .collect()
    }

    /// The rule that makes the two tabs exclusive: with no profile, refuse; Buy (1) allows only an
    /// unowned house, Rent (2) only an owned one; anything else refuses.
    #[must_use]
    pub fn is_payment_allowed(&self) -> bool {
        let Some(p) = self.profile.as_ref() else {
            return false;
        };
        match self.op {
            HouseOp::Undef => false,
            HouseOp::Buy => p.owner.0 == 0,
            HouseOp::Rent => p.owner.0 != 0,
        }
    }

    /// The house update **plus** the update-house-profile notice's show: store the slumlord id,
    /// copy the profile in, back up a pristine copy, refresh the house, and show the window.
    ///
    /// **The backup is why this build keeps the *pristine* profile and replays the drops.**
    /// Retail mutates its working profile as items go in and holds a backup as the
    /// untouched original; here the view hands over the untouched original every frame and the
    /// panel's own row lists are the mutation, which cannot drift apart the way two copies can.
    ///
    /// A **new** profile flushes both lists inside the house refresh, so every item the player had
    /// dropped is taken back out and both payment lists are cleared. Using a second slumlord therefore empties the window rather than
    /// carrying a half-paid basket across.
    pub fn recv_house_profile(&mut self, ui: &mut UiSystem, view: &dyn GameView) {
        self.profile = view.slumlord();
        self.clean_item_lists(ui);
        self.opens += 1;
        self.set_visible(ui, true);
    }

    /// Close only when the range handler names this window's slumlord. A stale watch for a previously opened slumlord is harmless.
    pub fn recv_object_range_exit(&mut self, ui: &mut UiSystem, slumlord: ObjectId) -> bool {
        if self.profile.as_ref().is_none_or(|p| p.slumlord != slumlord) {
            return false;
        }
        self.set_visible(ui, false);
        true
    }

    /// Both lists flushed, both buttons disabled, and both of the profile's payment lists
    /// cleared.
    ///
    /// The payment-clearing half is implicit here: the paid counts are derived from the row lists by
    /// [`GameView::slumlord_payment`], so emptying the rows *is* clearing the payments.
    pub fn clean_item_lists(&mut self, ui: &mut UiSystem) {
        self.buy_items.clear();
        self.rent_items.clear();
        self.last = None;
        if let Some(w) = self.buy_list.as_mut() {
            w.flush(ui);
        }
        if let Some(w) = self.rent_list.as_mut() {
            w.flush(ui);
        }
        self.buy_button_state = ButtonState::Disabled;
        self.rent_button_state = ButtonState::Disabled;
        self.write_button_states(ui);
    }

    /// The slumlord panel's failed house transaction notice — the `0x0258` retry: when a
    /// profile is held, query the slumlord it came from.
    ///
    /// **The word the notice carries is not read**, exactly as `HousePanel`'s and `MapPanel`'s
    /// handlers do not read it.
    pub fn recv_failed_house_transaction(
        &mut self,
        requests_out: &mut crate::requests::Outbox,
    ) -> bool {
        let Some(p) = self.profile.as_ref() else {
            return false;
        };
        requests_out.emit(UiRequest::HouseQueryLord {
            slumlord: p.slumlord,
        });
        self.requeries += 1;
        true
    }

    /// The drag-acceptability test, in retail's order: refuse **silently** with no tab up; refuse
    /// an unknown object; refuse an object the player does not own, printing [`NOT_CARRYING`] on
    /// channel `0x1A` unless `quiet`; refuse an item already listed; refuse when payment is not
    /// allowed; accept a container outright (it skips the test); otherwise answer whether the
    /// profile still needs more of the item's WCID.
    ///
    /// `quiet` is `true` for the hover and `false` for the drop, which is why hovering a stranger's
    /// item says nothing and dropping it complains once.
    ///
    /// **The first test is the mode, not the item**, and it is above the string: with neither tab
    /// up the window is inert and silent. That is the state a freshly opened window is in.
    #[must_use]
    pub fn drag_item_acceptable(
        &self,
        requests_out: &mut crate::requests::Outbox,
        id: ObjectId,
        view: &dyn GameView,
        quiet: bool,
    ) -> bool {
        let list = match self.op {
            HouseOp::Undef => return false,
            HouseOp::Buy => &self.buy_items,
            HouseOp::Rent => &self.rent_items,
        };
        if view.slot_decoration(id).is_none() {
            return false;
        }
        if !view.item_owned_by_player(id) {
            if !quiet {
                requests_out.emit(UiRequest::DisplayChatText {
                    channel: NOTICE_CHANNEL,
                    text: NOT_CARRYING.to_owned(),
                });
            }
            return false;
        }
        if list.iter().any(|d| d.id == id) {
            return false;
        }
        if !self.is_payment_allowed() {
            return false;
        }
        if view
            .slot_decoration(id)
            .is_some_and(|d| d.contained_items > 0)
        {
            return true;
        }
        view.slumlord_needs_more(
            self.op.is_rent(),
            &Self::drops(list),
            view.item_wcid(id),
            view.item_trade_note_value(id),
        )
    }

    /// The client's decision — which of the two states the tile
    /// under the drag cursor is given. The hover is the **quiet** one.
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

    /// The drop acceptance. Refuse unless the loud acceptability test passes, or the object is
    /// unknown. When the split size equals the maximum split size, clear the item's waiting state,
    /// add the object and update the buttons. Otherwise try to place the split part in the item's
    /// container: on success print [`SPLITTING_THE`] with the item's name on channel `0x1A` and
    /// accept; on failure print [`CANNOT_SPLIT`] and refuse.
    ///
    pub fn accept_drag_object(
        &mut self,
        ui: &mut UiSystem,
        id: ObjectId,
        view: &dyn GameView,
    ) -> bool {
        self.accept_drag_object_with_split(
            ui,
            id,
            view,
            u32::try_from(view.split_size()).unwrap_or(0),
            u32::try_from(view.max_split_size()).unwrap_or(0),
        )
    }

    /// The same entry with the toolbar's split-size pair captured by the pointer gesture.
    /// `HudView` deliberately does not own the toolbar splitter, so the gameplay screen carries
    /// these values across the one-frame panel-ownership seam.
    pub fn accept_drag_object_with_split(
        &mut self,
        ui: &mut UiSystem,
        id: ObjectId,
        view: &dyn GameView,
        split: u32,
        max: u32,
    ) -> bool {
        if !self.drag_item_acceptable(&mut ui.requests, id, view, false) {
            self.drops_refused += 1;
            return false;
        }
        if split != max {
            ui.requests.emit(UiRequest::HouseSplitItem {
                item: id,
                split,
                max,
            });
            return true;
        }
        self.add_object(ui, id, view, 0);
        self.update_buttons(ui, view);
        true
    }

    /// The slumlord panel's object insert — the container fork: an object with no contents goes
    /// to the item insert, anything else to the container insert.
    fn add_object(&mut self, ui: &mut UiSystem, id: ObjectId, view: &dyn GameView, depth: u32) {
        if view
            .slot_decoration(id)
            .is_some_and(|d| d.contained_items > 0)
        {
            self.add_container(ui, id, view, depth);
        } else {
            self.add_item(ui, id, view);
        }
    }

    /// Walk the contents and take every one the test
    /// admits, **quietly** (the acceptability test with `quiet` set).
    ///
    /// Note what is *not* here and is in `SalvagePanel`'s equivalent: no heading line. Dropping a
    /// pack onto the slumlord says nothing at all and simply takes the coins out of it.
    ///
    /// `depth` is this build's own guard; the client has none, because its containment graph is a
    /// tree by construction.
    fn add_container(&mut self, ui: &mut UiSystem, id: ObjectId, view: &dyn GameView, depth: u32) {
        if depth > 8 {
            return;
        }
        for child in view.container_contents(id).to_vec() {
            if self.drag_item_acceptable(&mut ui.requests, child, view, true) {
                self.add_object(ui, child, view, depth + 1);
            }
        }
    }

    /// The slumlord panel's item insert. Refuse a container, a missing current list or an item
    /// already listed. Build a payment of the item's WCID and house-payment amount (stack size, or
    /// 1); refuse if the payment insert does; otherwise set the item's trade state to 1, add its
    /// row and register the being-deleted notice on it.
    ///
    /// **The payment insert is a gate, not a side effect.** It is the profile's attempt to pay,
    /// which answers 0 for a row that is already paid in full — so an item the list no longer
    /// wants is refused *here*, after the drag-acceptable test let it through, and the row is
    /// never added.
    fn add_item(&mut self, ui: &mut UiSystem, id: ObjectId, view: &dyn GameView) -> bool {
        let rent = self.op.is_rent();
        let list = match self.op {
            HouseOp::Undef => return false,
            HouseOp::Buy => &self.buy_items,
            HouseOp::Rent => &self.rent_items,
        };
        if list.iter().any(|d| d.id == id) {
            return false;
        }
        let wcid = view.item_wcid(id);
        let amount = view.item_house_payment(id);
        let trade_note_value = view.item_trade_note_value(id);
        // The payment insert -> the house profile's attempt to pay, and the **gate** is that
        // boolean: an item the list no longer wants is refused here, after the drag-acceptable
        // test let it through, and the row is never added.
        if !view.slumlord_pay(rent, &Self::drops(list), wcid, amount, trade_note_value) {
            self.drops_refused += 1;
            return false;
        }
        match self.op {
            HouseOp::Undef => return false,
            HouseOp::Buy => self.buy_items.push(DroppedItem {
                id,
                wcid,
                amount,
                trade_note_value,
            }),
            HouseOp::Rent => self.rent_items.push(DroppedItem {
                id,
                wcid,
                amount,
                trade_note_value,
            }),
        }
        self.items_added += 1;
        self.last = None;
        let _ = ui;
        true
    }

    /// Take a row back out and undo its payment.
    ///
    /// The client walks the current item list for the row holding this object, clears the
    /// trade state, deletes the row and removes the payment. Here the payment is derived from the
    /// row list, so removing the row *is* the payment removal.
    pub fn remove_item(&mut self, _ui: &mut UiSystem, id: ObjectId) -> bool {
        let list = match self.op {
            HouseOp::Undef => return false,
            HouseOp::Buy => &mut self.buy_items,
            HouseOp::Rent => &mut self.rent_items,
        };
        let Some(i) = list.iter().position(|d| d.id == id) else {
            return false;
        };
        list.remove(i);
        self.last = None;
        true
    }

    /// Taking a row back out of the
    /// current payment list happens at pick-up, before the drop has a destination. The shared
    /// item-list producer returns the same list/index notice payload retail receives; this panel
    /// consumes only its current Buy or Rent list, removes the row, and immediately recomputes
    /// the payment buttons.
    fn begin_payment_drag(
        &mut self,
        ui: &mut UiSystem,
        source: ElemHandle,
        x: i32,
        y: i32,
        view: &dyn GameView,
    ) -> bool {
        let started = match self.op {
            HouseOp::Undef => None,
            HouseOp::Buy => self.buy_list.as_mut().and_then(|w| {
                (w.handle == source || w.slot_of(source).is_some())
                    .then(|| w.begin_drag(ui, x, y))?
            }),
            HouseOp::Rent => self.rent_list.as_mut().and_then(|w| {
                (w.handle == source || w.slot_of(source).is_some())
                    .then(|| w.begin_drag(ui, x, y))?
            }),
        };
        let Some(started) = started else { return false };
        let Some(item) = started.item else {
            return true;
        };
        if started.ghosted {
            ui.requests.emit(UiRequest::SetItemWaiting(item));
        }
        if self.remove_item(ui, item) {
            self.update_buttons(ui, view);
        }
        true
    }

    /// An authoritative move only
    /// removes an item already shown in the current payment list, and only after it is no longer
    /// owned by the player. A newly created split result is therefore not auto-inserted here.
    pub fn recv_server_says_move_item(
        &mut self,
        ui: &mut UiSystem,
        id: ObjectId,
        view: &dyn GameView,
    ) -> bool {
        let in_current_list = match self.op {
            HouseOp::Undef => false,
            HouseOp::Buy => self.buy_items.iter().any(|d| d.id == id),
            HouseOp::Rent => self.rent_items.iter().any(|d| d.id == id),
        };
        if !in_current_list || view.item_owned_by_player(id) {
            return false;
        }
        let removed = self.remove_item(ui, id);
        if removed {
            self.update_buttons(ui, view);
        }
        removed
    }

    /// The slumlord panel's buttons update, whole. With no tab up both buttons are disabled
    /// (`0x0D`). Otherwise Buy is enabled (1) only when a profile is held, the house is unowned and
    /// the buy price is paid in full; and Rent is enabled when the rent item list has any row,
    /// disabled otherwise.
    ///
    /// Three things worth being exact about, because each is a place the window looks broken if
    /// it is wrong:
    ///
    /// * **The Buy button needs the price paid IN FULL**, not merely a non-empty list. Dropping
    ///   half the purchase price leaves it grey, which is retail's whole protection against a
    ///   partial purchase.
    /// * **The Rent button needs only one row**, because maintenance is cumulative: a shard takes
    ///   whatever is offered and `HouseProfile.SetPaidItems` re-derives the rest.
    /// * **The Rent button is read off the rent item list whichever tab is up** — the client names
    ///   that list unconditionally — so with the Buy tab showing and rent items already dropped
    ///   the (hidden) Rent button is still enabled. Reproduced rather than tidied.
    pub fn update_buttons(&mut self, ui: &mut UiSystem, view: &dyn GameView) {
        if self.op == HouseOp::Undef {
            self.buy_button_state = ButtonState::Disabled;
            self.rent_button_state = ButtonState::Disabled;
            self.write_button_states(ui);
            return;
        }
        let unowned = self.profile.as_ref().is_some_and(|p| p.owner.0 == 0);
        let paid = unowned
            && view
                .slumlord_payment(false, &Self::drops(&self.buy_items))
                .paid_in_full;
        self.buy_button_state = if paid {
            ButtonState::Enabled
        } else {
            ButtonState::Disabled
        };
        self.rent_button_state = if self.rent_items.is_empty() {
            ButtonState::Disabled
        } else {
            ButtonState::Enabled
        };
        self.write_button_states(ui);
    }

    /// The one thing this window sends. With no tab up, no current list or no slumlord id it
    /// refuses. It collects every row's item id in list order; if there is at least one, it
    /// cleans both item lists, updates the buttons, sends the buy-house (`0x021C`) or rent-house
    /// (`0x0221`) request with the slumlord and the ids, and answers true.
    ///
    /// **The walk is forwards** — first row to last — where the salvage window walks backwards. The
    /// order is on the wire and a recorded session shows it preserved: `c2150080 c1150080 92150080` at `t = 144.458` is the drop order.
    ///
    /// **The window is emptied *before* the request goes out**,
    /// so a refusal leaves an empty basket and the items back in the pack — which is what makes
    /// the `0x0258` retry in [`Self::recv_failed_house_transaction`] the right repair.
    pub fn make_payment(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        if self.op == HouseOp::Undef {
            return false;
        }
        let Some(slumlord) = self
            .profile
            .as_ref()
            .map(|p| p.slumlord)
            .filter(|s| s.0 != 0)
        else {
            return false;
        };
        let items: Vec<ObjectId> = self.current_items().iter().map(|d| d.id).collect();
        if items.is_empty() {
            return false;
        }
        let rent = self.op.is_rent();
        self.clean_item_lists(ui);
        self.update_buttons(ui, view);
        ui.requests.emit(UiRequest::HousePayment {
            slumlord,
            rent,
            items,
        });
        self.payments += 1;
        true
    }

    /// The client's two dialog-context arms and their matching close handlers.
    /// Property `0x8E` selects the confirmation dialog; its Boolean answer is property `0x92`.
    pub fn close_payment_confirmation(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        rent: bool,
        confirmed: Option<bool>,
    ) {
        if rent {
            self.rent_confirmation_pending = false;
        } else {
            self.buy_confirmation_pending = false;
        }
        let Some(confirmed) = confirmed else {
            // A factory failure or framework teardown closes no retail question and therefore
            // means neither No nor Yes. It only releases the corresponding non-zero context.
            return;
        };
        if !confirmed {
            // A No answer hides the window, in both close handlers.
            self.set_visible(ui, false);
            return;
        }

        // Both Yes arms re-check their guard at close time. Buy asks whether the Buy profile is
        // paid in full; proxy rent asks whether the rent item list still has a row. The payment
        // then reads the current operation/list, exactly as retail's shared tail does.
        let allowed = if rent {
            !self.rent_items.is_empty()
        } else {
            self.profile.is_some()
                && view
                    .slumlord_payment(false, &Self::drops(&self.buy_items))
                    .paid_in_full
        };
        if allowed {
            self.make_payment(ui, view);
        }
    }

    /// All three message arms.
    ///
    /// * **Message `1`.** `0x1000009E` hides the window. `0x10000094` (Buy) raises the house-purchase
    ///   confirmation — and stops — unless a profile is held and the house type is 4 (apartment),
    ///   which pays straight through. `0x1000009B` (Rent) raises the by-proxy confirmation — and
    ///   stops — unless the player owns the house, which pays straight through.
    /// * **Message `0x15`.** A drop inside the current item list is handled as a drop release.
    /// * **Message `0x18`** (visibility changed). The window itself cleans both lists and updates
    ///   the buttons. The Buy page `0x10000090` becoming visible makes Buy the current operation
    ///   and list; becoming hidden while Buy is current resets to Undef with no list. The Rent page
    ///   `0x10000097` does the same for Rent.
    ///
    /// **Two branches are inverted from what the names suggest and both were read twice.**
    /// The Buy button raises the thirty-day confirmation for *every* dwelling **except** an
    /// apartment, and the Rent button raises the by-proxy confirmation when the player is **not**
    /// the owner. In both cases the confirmation is the *only* thing that happens on that click;
    /// the payment runs on the fall-through.
    ///
    /// **This module routes both confirmations straight to [`Self::make_payment`]** — the Yes arm of
    /// the buy-house confirmation close and the rent-payment-by-proxy confirmation close, each of
    /// which pays when the player agreed and the same guard the button had still holds. The
    /// dialog is named in the module header as not implemented; what is *not* skipped is either
    /// guard.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        use dereth_ui::msg::element::id as msg;
        if m.id == msg::VISIBILITY_CHANGED {
            let visible = m.p1 != 0;
            if Some(m.source) == self.root {
                self.clean_item_lists(ui);
                self.update_buttons(ui, view);
                if !visible {
                    ui.requests.emit(UiRequest::UnregisterSlumlordRange);
                }
                return true;
            }
            let page = match m.source_id {
                BUY_PAGE => HouseOp::Buy,
                RENT_PAGE => HouseOp::Rent,
                _ => return false,
            };
            if visible {
                self.op = page;
            } else if self.op == page {
                self.op = HouseOp::Undef;
            }
            self.last = None;
            self.update_buttons(ui, view);
            return true;
        }
        if m.id == msg::DRAG_REJECTED {
            let (x, y) = m.point.window;
            return self.begin_payment_drag(ui, m.source, x, y, view);
        }
        if m.id != msg::BUTTON_CLICKED {
            return false;
        }
        match m.source_id {
            CLOSE_BUTTON => {
                self.set_visible(ui, false);
                true
            }
            BUY_BUTTON => {
                // The client: every landscape house asks; only an apartment pays immediately.
                if self
                    .profile
                    .as_ref()
                    .is_none_or(|p| p.house_type != APARTMENT)
                {
                    if !self.buy_confirmation_pending {
                        self.buy_confirmation_pending = true;
                        ui.requests
                            .emit(UiRequest::HousePaymentConfirmation { rent: false });
                    }
                } else {
                    self.make_payment(ui, view);
                }
                true
            }
            RENT_BUTTON => {
                // The client: the owner pays immediately; anybody else gets the proxy warning.
                if self.profile.as_ref().is_none_or(|p| !p.am_i_the_owner) {
                    if !self.rent_confirmation_pending {
                        self.rent_confirmation_pending = true;
                        ui.requests
                            .emit(UiRequest::HousePaymentConfirmation { rent: true });
                    }
                } else {
                    self.make_payment(ui, view);
                }
                true
            }
            _ => false,
        }
    }

    /// The client's `0x3E` arm for **both** of this
    /// window's lists — the hover hint, the same delivery `SalvagePanel::on_drag_cursor_over`
    /// and `VendorPanel`'s have.
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
        let Some((rent, slot)) = self.slot_of(m.source) else {
            return false;
        };
        let Some(proxy) = ui.drag_state().element.filter(|_| m.p1 != 0) else {
            self.set_slot_state(ui, rent, slot, drag_accept_state::NONE);
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
        self.set_slot_state(ui, rent, slot, state);
        true
    }

    /// The client's `0x15` arm — the clear that takes
    /// the hover hint back down when the drop lands.
    pub fn on_drop_release(&mut self, ui: &mut UiSystem, m: &ElementMessage) -> bool {
        use crate::items::widget::drag_accept_state;
        if m.id != dereth_ui::msg::element::id::DROP_FAILED || m.p2 == 0 {
            return false;
        }
        let Some((rent, slot)) = self.slot_of(m.source) else {
            return false;
        };
        self.set_slot_state(ui, rent, slot, drag_accept_state::NONE);
        true
    }

    /// Which of the two lists a slot element belongs to, and its index.
    fn slot_of(&self, h: ElemHandle) -> Option<(bool, usize)> {
        if let Some(i) = self.buy_list.as_ref().and_then(|w| w.slot_of(h)) {
            return Some((false, i));
        }
        self.rent_list
            .as_ref()
            .and_then(|w| w.slot_of(h))
            .map(|i| (true, i))
    }

    fn set_slot_state(
        &mut self,
        ui: &mut UiSystem,
        rent: bool,
        slot: usize,
        s: dereth_ui::StateId,
    ) {
        let w = if rent {
            self.rent_list.as_mut()
        } else {
            self.buy_list.as_mut()
        };
        if let Some(w) = w {
            if let Some(sl) = w.slots.get_mut(slot) {
                sl.set_drag_accept_state(ui, s);
            }
        }
    }

    /// One frame's drive: the raise edge, the hidden edge, then the house refresh.
    ///
    /// The house refresh is four writes and two calls: the buy requirements text, the rent
    /// requirements text (from the second composer), the owner line `"Owner: "` + the name (or
    /// `"None"`) written to **both** owner texts, then cleaning the item lists and updating the
    /// buttons. It answers false when no profile is held.
    ///
    /// The list clean at the end is why a fresh profile empties the window, and it is
    /// [`Self::recv_house_profile`]'s, not this function's: calling it every frame would throw the
    /// player's basket away on every tick. This is the *redraw* half.
    ///
    /// Returns whether the tree was rewritten.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        // The raise edge — a count, so a second use of the same slumlord re-raises a window the
        // player closed. See `GameView::slumlord_notices`.
        let notices = view.slumlord_notices();
        if notices != self.profile_notices_seen {
            self.profile_notices_seen = notices;
            self.recv_house_profile(ui, view);
        }
        let live = self
            .root
            .and_then(|h| ui.node(h))
            .map(|n| n.region.flags.visible);
        if live == Some(false) && self.visible {
            self.closes += 1;
        }
        self.visible = live.unwrap_or(false);

        let buy_drops = Self::drops(&self.buy_items);
        let rent_drops = Self::drops(&self.rent_items);
        let buy_payment = view.slumlord_payment(false, &buy_drops);
        let rent_payment = view.slumlord_payment(true, &rent_drops);
        let tiles = |items: &[DroppedItem]| -> Vec<(
            ObjectId,
            Option<dereth_primitives::DataId>,
            Option<TileInfo>,
        )> {
            items
                .iter()
                .map(|d| (d.id, view.icon(d.id), TileInfo::read(view, d.id)))
                .collect()
        };
        let snapshot = Snapshot {
            profile: self.profile.clone(),
            op: self.op,
            buy: tiles(&self.buy_items),
            rent: tiles(&self.rent_items),
            buy_payment: buy_payment.clone(),
            rent_payment: rent_payment.clone(),
        };
        if self.last.as_ref() == Some(&snapshot) {
            return false;
        }

        let owner_line = self.profile.as_ref().map_or_else(String::new, |p| {
            format!(
                "{OWNER_PREFIX}{}",
                if p.owner_name.is_empty() {
                    OWNER_NONE
                } else {
                    p.owner_name.as_str()
                }
            )
        });
        for (h, text) in [
            (self.buy_requirements, buy_payment.requirements.as_str()),
            (self.rent_requirements, rent_payment.requirements.as_str()),
            (self.buy_owner, owner_line.as_str()),
            (self.rent_owner, owner_line.as_str()),
        ] {
            if let Some(h) = h {
                if let Some(t) = ui.text_element_mut(h) {
                    t.set_text(text);
                }
            }
        }

        let now = ui.now.0;
        let mut decorated = 0usize;
        for (rent, items) in [(false, &snapshot.buy), (true, &snapshot.rent)] {
            let ids: Vec<ObjectId> = items.iter().map(|t| t.0).collect();
            let w = if rent {
                self.rent_list.as_mut()
            } else {
                self.buy_list.as_mut()
            };
            let Some(w) = w else { continue };
            // Neither shipped list carries `UI_ItemList_FixedListSize`, so both are unbounded —
            // the same reading `TradePanel::fill`, `VendorPanel::fill` and `SalvagePanel::update`
            // take.
            w.set_contents(ui, None, Some(-1), &ids, &|id| view.icon(id));
            let info = |id: ObjectId| -> Option<SlotInfo> {
                items
                    .iter()
                    .find(|t| t.0 == id)
                    .and_then(|t| t.2.as_ref())
                    .map(|t| t.to_slot_info(view, now))
            };
            decorated += w.decorate(ui, &info);
        }
        self.slots_decorated = decorated;
        self.update_buttons(ui, view);
        self.last = Some(snapshot);
        self.rebuilds += 1;
        true
    }

    /// Write the Buy and Rent buttons' states.
    fn write_button_states(&mut self, ui: &mut UiSystem) {
        for (h, s) in [
            (self.buy_button, self.buy_button_state),
            (self.rent_button, self.rent_button_state),
        ] {
            if let Some(h) = h {
                ui.set_state(h, dereth_ui::StateId(s as u32));
            }
        }
    }

    fn set_visible(&mut self, ui: &mut UiSystem, visible: bool) {
        if !visible && self.visible {
            self.closes += 1;
        }
        self.visible = visible;
        if let Some(h) = self.root {
            ui.set_visible(h, visible);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A view with a slumlord profile and a stack slider **below** the whole stack.
    #[derive(Debug)]
    struct Splitting;

    impl GameView for Splitting {
        fn slumlord(&self) -> Option<SlumlordView> {
            Some(SlumlordView {
                slumlord: dereth_primitives::ObjectId(0x79DA_F03A),
                owner: dereth_primitives::ObjectId(0),
                owner_name: String::new(),
                house_type: 2,
                am_i_the_owner: false,
            })
        }
        fn slot_decoration(&self, _id: ObjectId) -> Option<crate::view::SlotDecoration> {
            Some(crate::view::SlotDecoration::default())
        }
        fn item_owned_by_player(&self, _id: ObjectId) -> bool {
            true
        }
        fn item_wcid(&self, _id: ObjectId) -> u32 {
            273
        }
        fn item_house_payment(&self, _id: ObjectId) -> i32 {
            10
        }
        fn slumlord_needs_more(
            &self,
            _rent: bool,
            _drops: &[(u32, i32, Option<i32>)],
            _wcid: u32,
            _trade_note_value: Option<i32>,
        ) -> bool {
            true
        }
        fn slumlord_pay(
            &self,
            _rent: bool,
            _drops: &[(u32, i32, Option<i32>)],
            _wcid: u32,
            _amount: i32,
            _trade_note_value: Option<i32>,
        ) -> bool {
            true
        }
        /// The split size — one of a stack of many.
        fn split_size(&self) -> i32 {
            1
        }
        /// The maximum split size.
        fn max_split_size(&self) -> i32 {
            10
        }
    }

    /// Oracle: the client's three drop-acceptance arms, described in
    /// [`SlumlordPanel::accept_drag_object`].
    #[test]
    fn a_partial_stack_requests_the_native_inventory_split() {
        let mut ui = dereth_ui::UiSystem::new((800, 600));
        let mut p = SlumlordPanel {
            op: HouseOp::Buy,
            ..SlumlordPanel::default()
        };
        p.profile = Splitting.slumlord();
        ui.requests.clear();

        let taken = p.accept_drag_object(&mut ui, ObjectId(0x8000_0001), &Splitting);
        assert!(taken, "the split placement is now reachable from the panel");
        assert!(p.buy_items.is_empty(), "and nothing goes in the window");
        assert_eq!(p.drops_refused, 0);
        assert_eq!(p.refusals_spoken, 0);
        assert_eq!(
            ui.requests.take(),
            vec![UiRequest::HouseSplitItem {
                item: ObjectId(0x8000_0001),
                split: 1,
                max: 10
            }]
        );
    }

    /// The other side of the same fork: slider at the top, whole stack, the item goes in.
    #[test]
    fn a_whole_stack_takes_the_add_object_arm() {
        #[derive(Debug)]
        struct Whole;
        impl GameView for Whole {
            fn slumlord(&self) -> Option<SlumlordView> {
                Splitting.slumlord()
            }
            fn slot_decoration(&self, id: ObjectId) -> Option<crate::view::SlotDecoration> {
                Splitting.slot_decoration(id)
            }
            fn item_owned_by_player(&self, id: ObjectId) -> bool {
                Splitting.item_owned_by_player(id)
            }
            fn item_wcid(&self, id: ObjectId) -> u32 {
                Splitting.item_wcid(id)
            }
            fn item_house_payment(&self, id: ObjectId) -> i32 {
                Splitting.item_house_payment(id)
            }
            fn slumlord_needs_more(
                &self,
                r: bool,
                d: &[(u32, i32, Option<i32>)],
                w: u32,
                n: Option<i32>,
            ) -> bool {
                Splitting.slumlord_needs_more(r, d, w, n)
            }
            fn slumlord_pay(
                &self,
                r: bool,
                d: &[(u32, i32, Option<i32>)],
                w: u32,
                a: i32,
                n: Option<i32>,
            ) -> bool {
                Splitting.slumlord_pay(r, d, w, a, n)
            }
            fn split_size(&self) -> i32 {
                10
            }
            fn max_split_size(&self) -> i32 {
                10
            }
        }
        let mut ui = dereth_ui::UiSystem::new((800, 600));
        let mut p = SlumlordPanel {
            op: HouseOp::Buy,
            ..SlumlordPanel::default()
        };
        p.profile = Whole.slumlord();
        ui.requests.clear();

        assert!(p.accept_drag_object(&mut ui, ObjectId(0x8000_0001), &Whole));
        assert_eq!(
            p.buy_items.len(),
            1,
            "the accepted item enters the buy list and refreshes the buttons"
        );
        assert_eq!(p.refusals_spoken, 0, "and it says nothing");
        assert!(ui.requests.take().is_empty());
    }
}
