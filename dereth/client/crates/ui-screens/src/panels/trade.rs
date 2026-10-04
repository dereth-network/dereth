//! The secure-trade window, its two item lists, and its three buttons.
//!
//! This window is the display half of `dereth-client-model`'s `Trade` state machine and of
//! `dereth_protocol::trade`'s ten trade codecs; `Request::OpenTradeNegotiations` starts a trade,
//! and this module shows it.
//!
//! # The element ids
//!
//! The secure-trade panel's post-init is nine child lookups with literal ids, and its element
//! class is `0x10000012`. None of the numbers below is inferred from a name or from a layout
//! listing; they are the ids the client itself uses.
//!
//! | element | const | what it is |
//! |---|---|---|
//! | `0x10000012` | [`ELEMENT_CLASS`] | the element **class** id the registration installs |
//! | `0x10000085` | [`SELF_NAME`] | your name |
//! | `0x10000086` | [`BTN_TRADE`] | the accept/decline toggle |
//! | `0x10000087` | [`SELF_TOTAL`] | "N Items" for your side |
//! | `0x10000088` | [`SELF_LIST`] | **your** offer |
//! | `0x1000007E` | [`OTHER_NAME`] | the partner's name |
//! | `0x1000007F` | [`OTHER_STATUS`] | the partner's accept light |
//! | `0x10000080` | [`OTHER_TOTAL`] | "N Items" for the partner |
//! | `0x10000081` | [`OTHER_LIST`] | the partner's offer |
//! | `0x1000008A` | [`BTN_CLEAR_ALL`] | "Clear All Items" |
//! | `0x1000008B` | [`BTN_CLOSE`] | the close button; the element-message handler hides the window |
//!
//! # The button is a toggle, and its two arms read backwards until you see that
//!
//! The client's `0x10000086` case — confirmed against retail, because it does not read the way a
//! reader expects:
//!
//! The button's state 6 takes the accept path; state 1 takes the decline path.
//!
//! The accept path then writes **6** while the decline path writes **1**. Read as "press at
//! 6 to accept, then set 6" that is a no-op; read as a **toggle whose state has already flipped by
//! the time the message arrives** it is exactly right, and the two handlers are re-asserting the
//! state they observed. That is the reading this panel implements: a press moves
//! `Enabled -> Accepted` (accept) or `Accepted -> Enabled` (decline).
//!
//! # The optimistic row, and why it is drawn from `pending` rather than from the view
//!
//! Adding an item to your offer updates the display before sending the request: the
//! insert is its *first* effect and the wire is its last. In order, on the arm where the object
//! holds nothing:
//!
//! 1. an id already in your list returns `-1`;
//! 2. the object's trade state is set to `1`;
//! 3. the row is inserted — **the row is on screen here**;
//! 4. your "N Items" label moves;
//! 5. the trade button's state is updated — **the button enables here**;
//! 6. the row's position is read — the position the request carries;
//! 7. and only now the add-to-trade request goes on the wire.
//!
//! So the window's own list and the confirmed trade list legitimately disagree for one round
//! trip. The `0x0200` handler is written for exactly that: lookup finds the row already there and
//! only
//! clears its waiting state, and *only* an id it does not find is inserted.
//!
//! This build is snapshot-driven, so the two lists are [`TradeView::self_rows`] (the server's) and
//! [`TradePanel::displayed_self`] (the window's). [`TradePanel::pending`] is the difference: ids
//! [`TradePanel::drop_item`] inserted that the `0x0200` has not yet named. A rebuild takes the
//! server's rows and appends the pending ones, so a confirmation *removes* an id from `pending`
//! rather than duplicating its row — which is the add-my-item handler's two arms in one statement.
//!
//! **Name and icon cross the seam through [`GameView::name`] and [`GameView::icon`]**, the same
//! two accessors `InventoryPanels` fills a pack slot from. They are not on [`TradeRow`]'s path
//! because there is no `TradeRow` yet: the server has not sent one. In the client the equivalent
//! lookup is inside the item list — row insertion creates an item element from the id
//! and its update reads the corresponding object — so the object table is the source on both
//! sides, and this crate's view of the object table is those two methods.
//!
//! # Removing the optimistic row
//!
//! Retail removes the optimistic row in exactly two handlers:
//!
//! | site | the message behind it |
//! |---|---|
//! | remove-from-trade receiver's `side == 1` arm | `0x0201 Trade_RemoveFromTrade` |
//! | trade-failure receiver | `0x0207 Trade_TradeFailure` |
//!
//! **The stack-split completion notice is not one of them.** Its handler
//! only clears the pending split-object field.
//! It touches neither list, neither button, nor the
//! partner light. Tests pin the two real removal paths above.
//!
//! What carries the failure across the seam is [`TradeView::self_removed`], and **that is a
//! declared seam difference**: `dereth_client_model::trade::TradeSystem` has a `self_removed` set with no
//! counterpart in the original trade controller, because a notice-driven window needs no memory of a
//! refusal and a snapshot-driven one does. Its two writers are the two notices above and nobody
//! else; a `0x0200` on side 1 takes an id back out of it; the trade reset's two callers
//! and the end-of-character-session handler clear it wholesale.
//!
//! The remove-added-item handler's **first three steps run before it has looked the row up** —
//! button to `1`, the trade-button update, then light to `0x0D` — so a removal overrides the
//! mirror's acceptances on the frame it lands. That is
//! [`TradePanel::update`]'s `removed_now`.
//!
//! # The rows are decorated
//!
//! [`ItemListWidget::decorate`] is the client's tail, and it runs here as well as after
//! `set_contents`. Without it a row on the trade table draws no composite icon, no structure
//! bar, no capacity meter, no selection ring, no sell/trade marker and **no tooltip** — which is
//! where the client says how many of something you are being offered (`"%d %s"` of the stack size
//! and the plural name). There is deliberately **no quantity numeral** on these rows:
//! only the vendor-items panel writes the quantity overlay.
//!
//! `decorate`'s input per slot is a [`SlotInfo`] — **four** things, not one — so
//! `TradePanel::last_tiles` holds the three frozen-world ones per row and `cooldown_remaining`
//! is exempted. See `TradePanel::last_tiles` and the note above the `info` closure.
//!
//! # The six elements below the two lists, and the one retail never writes
//!
//! Each is written from its own source, and the sources are not interchangeable:
//!
//! | element | writer in the client | what this panel writes it from |
//! |---|---|---|
//! | `0x1000007E` partner name | the partner's wide object name, or `""` with no partner | [`TradeView::partner_name`] |
//! | `0x1000007F` partner light | state `6` / state `0x0D` — see below | [`TradeView::partner_accepted`] |
//! | `0x10000087` your "N Items" | the my-item-number write | the row count of your list |
//! | `0x10000080` partner "N Items" | the other-item-number write | the row count of the partner's list |
//! | `0x10000086` trade button | a state write from six call sites | [`TradePanel::button`] |
//! | `0x10000085` your name | **nothing** | **nothing — see below** |
//!
//! **The self-player-name handle is bound and never written in retail either, and that is measured.**
//! Construction clears the handle and post-initialization stores it, but no later path writes
//! text to it. The same check finds a writer for the partner name and for both item-total labels,
//! so it does detect label writers when they exist; it finds none for this field.
//! Writing the player's name here would therefore be a deviation, not a repair, and this
//! remains deliberately different from the three labels whose writers are observed.
//! The panel binds the self-name element and leaves it alone.
//!
//!
//! # The partner's accept light is a two-state bare element
//!
//! The accept trade notice and the decline trade notice are the panel's own notice handlers.
//! The two observable actions are:
//!
//! Accepting sets the partner light to state `6`.
//! Declining sets it to state `0x0D`.
//!
//! The affected field is the partner trade-status indicator.
//! The light has exactly two values, **`6` accepted** and **`0x0D` not**, and the two
//! partner-side arms do *not* run the trade-button update, because the partner's acceptance
//! changes neither list. The add-my-item, add-partner-item, remove-added-item and
//! remove-partner-item handlers all knock the light back to `0x0D`, which falls out here because any list
//! movement is a snapshot change and `partner_accepted` is cleared by the server in the same
//! `0x0200`.
//!
//! # What the corpus can and cannot witness: nothing, and that is measured
//!
//! A calibrated scan of all seven recorded captures — 994 server game events and 2,564 client
//! actions over 34 sub-types, with `Combat_QueryHealthResponse` ×151 and `MoveToState` ×1187 as
//! known-positives — finds **0 of every trade opcode in both directions**, against the vendor
//! family's `0x005F` ×5, `0x0060` ×1 and `0x0062` ×8 in the same scan. So this panel is
//! **transcribe-and-drive, not replay**: every message here is synthesised, the *route* is real,
//! and the request bodies are compared as literal bytes. A validating capture would have to contain
//! both sides of a successful trade; none was manufactured.
//!
//! # No live test
//!
//! A trade moves items between characters irreversibly. Nothing here was sent to a live
//! shard: no trade was opened, none was accepted, and every request is asserted as bytes from a
//! loopback the test process owns.
//! No external trade state was changed.

use dereth_primitives::DataId;
use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::items::widget::{ItemListWidget, SlotInfo, TileInfo};
use crate::view::{GameView, TradeRow, TradeView, UiRequest};

/// The secure trade panel's element class, `0x10000012`.
pub const ELEMENT_CLASS: u32 = 0x1000_0012;

/// The window element's own id in the shipped gameplay tree, **measured** rather than declared:
/// the 11,593-element live tree holds exactly one
/// element of type [`ELEMENT_CLASS`], at `0x1000005F`, with all ten of the post-init's children
/// under it. The post-init never names it (it is the panel itself), which is why [`TradePanel::post_init`]
/// finds the window by **type** and this constant is documentation rather than a lookup key.
pub const WINDOW: ElementId = ElementId(0x1000_005F);

/// The partner-name text.
pub const OTHER_NAME: ElementId = ElementId(0x1000_007E);
/// The partner trade-status indicator — the only one of the nine fetched as a basic element
/// rather than cast, because it is a light rather than a typed control.
pub const OTHER_STATUS: ElementId = ElementId(0x1000_007F);
/// The partner's "N Items" label.
pub const OTHER_TOTAL: ElementId = ElementId(0x1000_0080);
/// The partner's item list.
pub const OTHER_LIST: ElementId = ElementId(0x1000_0081);
/// Your name label.
pub const SELF_NAME: ElementId = ElementId(0x1000_0085);
/// The trade (accept/decline) button.
pub const BTN_TRADE: ElementId = ElementId(0x1000_0086);
/// Your "N Items" label.
pub const SELF_TOTAL: ElementId = ElementId(0x1000_0087);
/// Your item list — the one list the post-init registers a drag handler on, which is
/// why only your own side accepts a drop.
pub const SELF_LIST: ElementId = ElementId(0x1000_0088);
/// The "Clear All Items" button.
pub const BTN_CLEAR_ALL: ElementId = ElementId(0x1000_008A);
/// The close button. The third element-message case hides the panel,
/// by making it invisible; the *message* goes out because hiding raises the visibility-changed
/// notification.
pub const BTN_CLOSE: ElementId = ElementId(0x1000_008B);

/// The drag-accept-state setter's two arguments are `0x10000040` and `0x10000041`.
///
/// The drag-accept-state write is a state write: the two numbers are **states**, not elements,
/// and an `ElementId` could not be handed to `ItemSlot::set_drag_accept_state`. They are aliases
/// of [`crate::items::widget::drag_accept_state`]'s, which a test pins against retail, rather
/// than a second copy of the numbers.
pub const DRAG_ACCEPT: dereth_ui::StateId = crate::items::widget::drag_accept_state::ACCEPT;
/// See [`DRAG_ACCEPT`].
pub const DRAG_REFUSE: dereth_ui::StateId = crate::items::widget::drag_accept_state::REFUSE;
/// The UI-item element-message handler's leave arm and
/// the client's drop arm both write this one.
pub const DRAG_NONE: dereth_ui::StateId = crate::items::widget::drag_accept_state::NONE;

/// String table `0x10000001` — the table both total-items labels
/// resolve against, the same `DidMapper` group-4 entry as
/// [`super::allegiance::STRING_TABLE`].
pub const STRING_TABLE: DataId = DataId(0x2300_0001);

/// The one `StringInfo` row this window uses for both
/// the self and partner item-number writes.
///
/// The token is the string the client hashes: a static initialiser sets the
/// `ID_SecureTrade_TotalItemsLabel` id to the string hash of `"ID_SecureTrade_TotalItemsLabel"`, so
/// nothing here writes a hash down. Its single variable is an integer variable whose *name* is
/// the item-count variable id — and that name is the hash of `"ITEMS"`, not a hash of the id's
/// own spelling, which is worth knowing because the name and the token differ. The variable's
/// name does not reach the rendered string: `resolve_string_variants` returns the literal
/// pieces around it.
pub const ID_TOTAL_ITEMS_LABEL: &str = "ID_SecureTrade_TotalItemsLabel";

/// The partner light's two states, as the literals the client writes.
///
/// It is a basic element, so its state write uses ordinary state application rather than
/// button-specific disabled or toggle behavior. See
/// this module's header for the call sites.
pub mod status_state {
    /// The accept-trade notice's partner arm writes state `6`.
    pub const ACCEPTED: u32 = 6;
    /// The reset and decline-trade paths share this partner-state arm. Also what the
    /// add-my-item, add-partner-item, remove-added-item and remove-partner-item handlers write, because any list movement clears the partner's acceptance.
    pub const NOT_ACCEPTED: u32 = 0x0D;
}

pub use dereth_client_contract::view::TradeButtonState as ButtonState;

/// The secure-trade panel, bound to a live tree.
#[derive(Debug, Default)]
pub struct TradePanel {
    /// The secure-trade element itself — the nearest ancestor of [`SELF_LIST`] whose
    /// **type** is [`ELEMENT_CLASS`], which is the element that is shown and hidden.
    ///
    /// Found by type rather than by id because post-initialization never names the window's own
    /// id: it is the panel itself. Binding the screen root here instead would hide the whole HUD, which is
    /// what the first draft of this panel did.
    pub root: Option<ElemHandle>,
    /// Your item list.
    pub self_list: Option<ItemListWidget>,
    /// The partner's item list.
    pub other_list: Option<ItemListWidget>,
    /// Input-dispatch snapshot of the UI target mode, shared with ordinary item lists.
    pub target_mode_active: bool,
    pub trade_button: Option<ElemHandle>,
    pub clear_button: Option<ElemHandle>,
    pub other_status: Option<ElemHandle>,
    /// The partner-name text — the trade partner write's only receiver.
    pub other_name: Option<ElemHandle>,
    /// The partner's "N Items" label — the other item number write's receiver.
    pub other_total: Option<ElemHandle>,
    /// Your "N Items" label — the my item number write's receiver.
    pub self_total: Option<ElemHandle>,
    /// Your name label, **bound and deliberately never written**, because retail never writes
    /// it either: see this module's header. The binding
    /// is kept so that the question "is this element reachable" has an answer, and so that
    /// a future change with a reason to write it does not have to re-derive the id.
    pub self_name: Option<ElemHandle>,
    /// The trade button's state.
    pub button: ButtonState,
    /// **The window's own two lists**, which is what the accept path compares against the mirror.
    ///
    /// They are not a copy of the view: the drop path inserts a row
    /// **before** anything goes on the wire, and the server's `0x0200` then finds it already there
    /// (the client's found-row arm just clears the waiting state). The two can
    /// therefore legitimately disagree for a round trip, and that disagreement is exactly what the
    /// anti-scam comparison exists to catch.
    pub displayed_self: Vec<dereth_primitives::ObjectId>,
    /// The partner's side, which the window never inserts into optimistically.
    pub displayed_other: Vec<dereth_primitives::ObjectId>,
    /// Ids [`Self::drop_item`] inserted that the server's `0x0200` has not named
    /// yet — the difference between the displayed list and the shared trade model.
    ///
    /// A rebuild appends these to [`TradeView::self_rows`] and drops any the view now carries,
    /// which is the "add my item" path's two arms: an id already in the list is left alone (its
    /// waiting state cleared) and only an unknown one is inserted.
    pub pending: Vec<dereth_primitives::ObjectId>,
    /// The snapshot the tree was last built for — the rebuild guard.
    last: Option<TradeView>,
    /// **The other half of that guard**, one entry per tile the two lists hold, in
    /// the order [`Self::update`] builds them: your confirmed rows, then your pending ones, then
    /// the partner's.
    ///
    /// [`ItemListWidget::decorate`]'s input per slot is a [`SlotInfo`], and a `SlotInfo` is
    /// **four** things: the whole [`crate::view::SlotDecoration`], the name, the plural name and
    /// `cooldown_remaining`. [`TradeView`] carries none of them — its [`TradeRow`] holds an id, a
    /// name and an icon id — so a guard that compared only the snapshot would let the window
    /// through with a stale quantity overlay, a stale structure bar and a stale tooltip for the
    /// rest of the negotiation. The first three are facts about a frozen world and live here; the
    /// fourth is a function of the clock and is exempted — see the note above the `info` closure
    /// in [`Self::update`].
    ///
    /// Held here rather than inside [`TradeView`] for the reason
    /// [`crate::panels::inventory::InventoryPanels`] holds its own: a `TileInfo` owns two
    /// `String`s, and [`TileInfo::matches`] asks the view field by field, so a frame on which
    /// nothing moved allocates nothing at all.
    last_tiles: Vec<Option<TileInfo>>,
    /// How many times [`Self::update`] rebuilt. **Three states, not two**: "rebuilt and the window
    /// was closed" must not read the same as "never driven".
    pub rebuilds: u32,
    /// What the last rebuild put in each list, so a test can read back what a player would see.
    rows: [Vec<TradeRow>; 2],
    /// Whether the last rebuild left the window visible.
    pub visible: bool,
    /// How many slots the last rebuild ran the client's tail over — the denominator for "the
    /// trade table's quantities are drawn". **Three states, not two**: a window with two rows and
    /// `slots_decorated == 0` is a defect, and it must not read the same as a window with nothing
    /// on the table.
    pub slots_decorated: usize,

    // ---- what the last rebuild wrote into the four non-list elements --------------------------
    /// The text that went into [`OTHER_NAME`].
    pub partner_name_text: String,
    /// The text that went into [`SELF_TOTAL`].
    pub self_total_text: String,
    /// The text that went into [`OTHER_TOTAL`].
    pub other_total_text: String,
    /// The state written to [`OTHER_STATUS`] — one of [`status_state`]'s two values.
    ///
    /// **Not the same thing as [`Self::button`]**, and they move independently: the partner's
    /// light follows the *partner's* acceptance and the button follows *yours*, so a station that
    /// asserted one would pass with the other dead.
    ///
    /// **Zero means "never written"** — neither of the two real values is 0 — which is the same
    /// three-state discipline [`Self::rebuilds`] carries, one element down.
    pub partner_status: u32,
}

impl TradePanel {
    /// The secure-trade panel's post-init, minus the eleven notice registrations and the
    /// object-range handler.
    ///
    /// Bound off the screen **root** rather than off a declared window id, for the same reason
    /// [`crate::panels::vendor::VendorPanel::post_init`] is: secure trade is one of the
    /// *environment windows* incorporated under `<ENVP>` (retail groups it with vendor, salvage, housing, external-container, and spellcasting panels), so the ids
    /// below are found wherever they are.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        let Some(list) = ui.get_child_recursive(root, SELF_LIST) else {
            *self = Self::default();
            return;
        };
        self.self_list = Some(ItemListWidget::init(ui, list));
        self.other_list = ui
            .get_child_recursive(root, OTHER_LIST)
            .map(|h| ItemListWidget::init(ui, h));
        self.trade_button = ui.get_child_recursive(root, BTN_TRADE);
        self.clear_button = ui.get_child_recursive(root, BTN_CLEAR_ALL);
        self.other_status = ui.get_child_recursive(root, OTHER_STATUS);
        self.other_name = ui.get_child_recursive(root, OTHER_NAME);
        self.other_total = ui.get_child_recursive(root, OTHER_TOTAL);
        self.self_total = ui.get_child_recursive(root, SELF_TOTAL);
        // Bound and never written -- see this module's header. The post-init fetches it too.
        self.self_name = ui.get_child_recursive(root, SELF_NAME);
        self.root = window_of(ui, list);
        self.button = ButtonState::Disabled;
        self.displayed_self.clear();
        self.displayed_other.clear();
        self.pending.clear();
        self.last = None;
        self.last_tiles.clear();
        self.rows = [Vec::new(), Vec::new()];
        self.visible = false;
        self.slots_decorated = 0;
        self.partner_name_text = String::new();
        self.self_total_text = String::new();
        self.other_total_text = String::new();
        self.partner_status = 0;
        // The client's last step is the reset, which is what
        // puts the window in its empty state: partner name cleared, light to `0x0D`, both lists
        // flushed, both totals written and the button restated.
        self.reset(ui);
    }

    /// The secure-trade panel's reset, in its own order.
    ///
    /// The client guards the whole body on the panel's initialised flag bit; here the equivalent is that `post_init` has bound something, which the
    /// `let … else` above has already established.
    fn reset(&mut self, ui: &mut UiSystem) {
        self.set_partner_name(ui, "");
        self.set_partner_status(ui, false);
        self.flush(ui);
        self.set_item_numbers(ui);
        self.button = self.update_button_state();
        self.write_button_state(ui);
    }

    /// True once the self list was found — the binding without which nothing can be drawn.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.self_list.is_some()
    }

    /// What the last rebuild wrote into the two lists: index 0 is yours, 1 is the partner's.
    #[must_use]
    pub fn rows(&self, partner_side: bool) -> &[TradeRow] {
        &self.rows[usize::from(partner_side)]
    }

    /// The trade-list flush, on the window side.
    fn flush(&mut self, ui: &mut UiSystem) {
        self.displayed_self.clear();
        self.displayed_other.clear();
        // The flush empties the *window's* lists, so the optimistic rows go with them -- there is
        // nothing left for the `0x0200` to find, and the add-my-item handler would re-insert.
        self.pending.clear();
        self.fill(ui, false, &[]);
        self.fill(ui, true, &[]);
    }

    /// The per-frame rebuild, guarded on the snapshot.
    ///
    /// A window that closes is **not** an early return: both lists are flushed and the window
    /// hidden, because a player whose partner walked away must stop seeing the partner's items.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let v = view.trade();
        // The window's list is the
        // server's rows plus whatever `drop_item` inserted that the `0x0200` has not confirmed.
        // Retaining first is the client's item-list lookup arm: an id the server now
        // names is already on screen, so it leaves the row alone rather than inserting a second
        // one. The second term: an id the server has taken *off* your side
        // is gone from the window too, and it is never going to appear in `self_rows`.
        //
        // It sits above the guard because the guard is taken over the ids the fill will draw,
        // and computing that id list a second way would be a duplicated mapping. It is a mutation before an early-out and it is safe to be one: its result
        // depends only on `v` and on `pending`, `pending` grows only in [`Self::drop_item`] (which
        // clears `last`), and the previous frame ran the identical retain against the identical
        // `v` — so on any frame the early-out below can fire, this removes nothing. A test asserts
        // it.
        self.pending.retain(|id| {
            !v.self_rows.iter().any(|r| r.item == *id) && !v.self_removed.contains(id)
        });
        // **The ids the two lists will hold, in the order they will hold them.**
        //
        // This is the fill's own id list, computed once and used three times: for the guard's
        // memory, for the `info` closure the decorate pass reads, and for the rows themselves. A
        // closed window draws nothing, so it has no tiles.
        let tile_ids: Vec<dereth_primitives::ObjectId> = if v.open {
            v.self_rows
                .iter()
                .map(|r| r.item)
                .chain(self.pending.iter().copied())
                .chain(v.partner_rows.iter().map(|r| r.item))
                .collect()
        } else {
            Vec::new()
        };
        // Asked without allocating: on a still frame this walks the ids and builds no `String`.
        //
        // The length term stays, as it does in the inventory panel, for the same reason: `zip`
        // stops at the shorter side, so a `last_tiles` shorter than `tile_ids` would make `any`
        // answer *false* and the gate could close over a fill that never happened. Every id in
        // `tile_ids` comes either from `v` — already compared as a whole — or from `pending`,
        // which only [`Self::drop_item`] grows and which clears `last` when it does, so the term
        // is unfalsifiable on this host today. That is containment through another field rather
        // than by construction, so it stays.
        let tiles_changed = self.last_tiles.len() != tile_ids.len()
            || tile_ids
                .iter()
                .zip(&self.last_tiles)
                .any(|(id, last)| !TileInfo::matches(view, *id, last.as_ref()));
        if self.last.as_ref() == Some(&v) && !tiles_changed {
            return false;
        }
        // One read of the seam per tile, used by the rows below, by the `info` closure and stored
        // as the next pass's memory — so the value the guard compared and the value the screen
        // draws are one read — the inventory panel's shape, on this panel.
        let tiles: Vec<Option<TileInfo>> = tile_ids
            .iter()
            .map(|id| TileInfo::read(view, *id))
            .collect();
        let tile_of = |id: dereth_primitives::ObjectId| tile_at(&tile_ids, &tiles, id);
        // **The secure trade panel's added item removal's edge.**
        //
        // Its first three steps run before it has even found the row, and they are not
        // functions of the rebuilt lists: trade button to state `1`, the trade-button update,
        // partner light to state `0x0D`. So a
        // removal forces the button back to **Enabled** and the partner's light back to
        // **`0x0D`** whatever the mirror still says, and only then does it delete the row.
        //
        // The remove-added-item handler returns false without touching anything else when your
        // list has no row for the id — but the button and the light have already been written
        // by then, because those three steps come before that lookup.
        // The condition here is therefore *"the server named an id the window is showing on your
        // side"*, read off [`Self::displayed_self`] as it stood at the end of the previous
        // rebuild, which is the list the lookup would have been asked about.
        let removed_now = v.open
            && self
                .displayed_self
                .iter()
                .any(|id| v.self_removed.contains(id));
        // **The other three of the four, and the reset-on-change rule.**
        //
        // The my item insert, the partner item insert and the partner item removal
        // open with the identical pair the remove-added-item handler does — state `1` on the
        // button and `0xD` on the indicator, both before every lookup in the function — so **a row
        // appearing or leaving on either side darkens both lights**, and it stays dark until an
        // acceptance message arrives. The shard sends nothing that says so (a server clears its own
        // acceptance on an add and tells nobody), which is why this has to come off
        // `TradeView::acceptance_darkened` and cannot be read out of `accepted` /
        // `partner_accepted`: those two stay *set* in the mirror the whole time.
        //
        // `removed_now` stays beside it rather than being folded in, because the two cover
        // different cases: this flag is a fact about the model and moves only when a datagram
        // lands, while `removed_now` is about an **optimistic** row the model never held.
        if !v.open {
            // The close-trade notice is the reset and then one more empty-string write on the
            // partner name; the reset's own partner write has already emptied it, because the
            // trade partner id is 0 by then.
            // The button is *not* forced to `0x0D` here: the reset leaves that to the
            // trade-button update, which sees both lists empty and writes it.
            self.reset(ui);
        } else {
            let mut mine = v.self_rows.clone();
            for id in &self.pending {
                // Both fields come out of the tile rather than off the view a
                // second time, so every input of this fill is a member of the compared value **by
                // construction** and not through a producer that happens to agree. `TileInfo::read` fills `name` from `GameView::name` and
                // `decoration.icon_id` from the object's icon id, which is what
                // `GameView::icon` answers; a tile that is `None` is an id with no
                // weenie object, for which both accessors answer `None` anyway.
                let t = tile_of(*id);
                mine.push(TradeRow {
                    item: *id,
                    name: t.map(|t| t.name.clone()).unwrap_or_default(),
                    icon: t.and_then(|t| {
                        (t.decoration.icon_id != 0).then_some(DataId(t.decoration.icon_id))
                    }),
                });
            }
            self.displayed_self = mine.iter().map(|r| r.item).collect();
            self.displayed_other = v.partner_rows.iter().map(|r| r.item).collect();
            self.fill(ui, false, &mine);
            let other = v.partner_rows.clone();
            self.fill(ui, true, &other);
            // The trade-partner write and the accept light, from **two different fields**.
            self.set_partner_name(ui, &v.partner_name);
            // …and the "remove added item" path's `0xD` overrides the mirror's
            // flag on the frame the removal lands.
            let controls = v.controls(
                self.displayed_self.len(),
                self.displayed_other.len(),
                removed_now,
            );
            self.set_partner_status(ui, controls.partner_accepted);
            // The client writes 6 on the button when the accept was yours, and 1 when the server
            // cleared it; then the trade-button update runs. The remove-added-item handler's `1`
            // is the same write from the other
            // producer, and it wins for the same reason `0x0207` clears an acceptance at all.
            self.button = controls.button;
            // The my/other item-number writes run off the **lists**, so they go after the fill
            // and after the button, exactly as the add and the reset order them.
            self.set_item_numbers(ui);
            self.write_button_state(ui);
        }
        // **The item-update tail, on both lists.**
        //
        // The item-list insertion path runs it per row as the client's list fills, so a row on
        // the trade table has always had its composite icon, its structure bar, its capacity
        // meter, its selection ring, its sell/trade marker and its **tooltip** in retail;
        // `set_contents` alone draws none of that.
        //
        // **The tooltip is the one a player needs, and there is no numeral here.**
        // The UI item's quantity write has exactly one caller in the whole client —
        // inside the vendor items panel's quantity-overlay update — so the row's quantity
        // is `-1` on every trade row and the quantity display hides the text. What
        // says *how many* is the tooltip update's `"%d %s"` of the stack size and the plural
        // name. A build that painted the stack size over these rows would be a deviation, not a fix.
        //
        // **`cooldown_remaining` is the one input above that the guard CANNOT hold**, and this is
        // why. Every field of a [`TileInfo`] is a fact about a frozen world: it changes when a
        // datagram lands and is otherwise constant, so *"the compared value did not move"* and
        // *"nothing happened"* are the same statement. A cooldown is
        // `(duration + start time) - now`, a function of the **clock**: it takes a
        // new value every frame of its own accord, with no datagram behind it. Folding it into
        // `last_tiles` would make this early-out fire **never** — both lists flushed, refilled and
        // re-decorated every frame for as long as anything on the table was on cooldown — and it
        // would *look correct on screen*, which is what makes it worse than the defect it would be
        // fixing. So it stays out, and the read below is not the countdown mechanism: it is the
        // fresh value the UI item update's own tail would have used on a frame that really did
        // rebuild, so a row refilled mid-cooldown draws the right wedge at once.
        //
        // **The heartbeat that ticks it does not reach these two lists**, because
        // `GamePlayScreen::do_item_heartbeat` walks the inventory's lists and the eighteen
        // quickbar tiles only. That is a declared gap, not a claim that it is right —
        // in the client every displayed item element registers for global message 3 in
        // its post-init, these two lists included. The file that owns the walk is
        // `screens/gameplay.rs`.
        let now = ui.now.0;
        let info = |id: dereth_primitives::ObjectId| -> Option<SlotInfo> {
            Some(tile_of(id)?.to_slot_info(view, now))
        };
        let mut decorated = 0;
        if let Some(w) = self.self_list.as_mut() {
            decorated += w.decorate(ui, &info);
        }
        if let Some(w) = self.other_list.as_mut() {
            decorated += w.decorate(ui, &info);
        }
        self.slots_decorated = decorated;
        self.set_visible(ui, v.open);
        self.last_tiles = tiles;
        self.last = Some(v);
        self.rebuilds += 1;
        true
    }

    /// The secure-trade panel's trade-button state update, over the panel's own two lists.
    ///
    /// The count is the partner's row count plus yours — **both** lists, which is easy
    /// to get wrong: a build that counted only your own side would re-enable the button when the
    /// partner emptied theirs.
    #[must_use]
    pub fn update_button_state(&self) -> ButtonState {
        let n = self.displayed_self.len() + self.displayed_other.len();
        self.button.with_displayed_rows(n)
    }

    fn fill(&mut self, ui: &mut UiSystem, partner_side: bool, rows: &[TradeRow]) {
        let w = if partner_side {
            self.other_list.as_mut()
        } else {
            self.self_list.as_mut()
        };
        let Some(w) = w else {
            self.rows[usize::from(partner_side)] = rows.to_vec();
            return;
        };
        let ids: Vec<dereth_primitives::ObjectId> = rows.iter().map(|r| r.item).collect();
        let icons: std::collections::BTreeMap<
            dereth_primitives::ObjectId,
            Option<dereth_primitives::DataId>,
        > = rows.iter().map(|r| (r.item, r.icon)).collect();
        // Both lists are unbounded: the shipped layout gives neither a `UI_ItemList_FixedListSize`,
        // which is the empty-slot update's unbounded arm — the same reading `VendorPanel::fill` takes.
        w.set_contents(ui, None, Some(-1), &ids, &|id| {
            icons.get(&id).copied().flatten()
        });
        self.rows[usize::from(partner_side)] = rows.to_vec();
    }

    /// The row count of one of the two lists: the count used for both total labels and
    /// [`Self::update_button_state`].
    ///
    /// Falls back to what the last fill recorded when the list is not bound, so an unbound panel
    /// reports the number it *would* have drawn rather than a silent zero.
    #[must_use]
    fn num_ui_items(&self, partner_side: bool) -> usize {
        let w = if partner_side {
            self.other_list.as_ref()
        } else {
            self.self_list.as_ref()
        };
        w.map_or_else(
            || self.rows[usize::from(partner_side)].len(),
            ItemListWidget::num_ui_items,
        )
    }

    /// The secure trade panel's trade partner write — the partner name (`0x1000007E`).
    ///
    /// The client's own null-partner arm writes the empty
    /// string, rather than skipping the call; so does this.
    fn set_partner_name(&mut self, ui: &mut UiSystem, name: &str) {
        self.partner_name_text = name.to_owned();
        if let Some(t) = self.other_name.and_then(|h| ui.text_element_mut(h)) {
            t.set_text(name);
        }
    }

    /// The partner's accept light (`0x1000007F`).
    ///
    /// See this module's header for the state-write sites and their two literals. It is a bare
    /// basic element, so this is an ordinary state write and not the button override.
    fn set_partner_status(&mut self, ui: &mut UiSystem, accepted: bool) {
        let s = if accepted {
            status_state::ACCEPTED
        } else {
            status_state::NOT_ACCEPTED
        };
        self.partner_status = s;
        if let Some(h) = self.other_status {
            ui.set_state(h, dereth_ui::StateId(s));
        }
    }

    /// The "my item number" and "other item number" writes, which are the same
    /// function twice over a different list.
    ///
    /// **The two guard different things, and the asymmetry is the client's**: the my-item-number
    /// write tests your label and then reads your list, while the other-item-number write tests
    /// the partner's list and then writes the partner's label
    /// **with no null test at all** — a layout carrying the list but not the label would fault in
    /// retail. Here both sides are `if let`, so the shape is recorded rather than reproduced.
    fn set_item_numbers(&mut self, ui: &mut UiSystem) {
        let mine = self.num_ui_items(false);
        let theirs = self.num_ui_items(true);
        // An integer string-info variable, so the number is grouped by the client's number-to-string helper
        // and not by a bare `to_string` -- see `super::statmgmt::num`.
        self.self_total_text = fill(ui, ID_TOTAL_ITEMS_LABEL, &super::statmgmt::num(mine as i64));
        let text = self.self_total_text.clone();
        if let Some(t) = self.self_total.and_then(|h| ui.text_element_mut(h)) {
            t.set_text(&text);
        }
        self.other_total_text = fill(
            ui,
            ID_TOTAL_ITEMS_LABEL,
            &super::statmgmt::num(theirs as i64),
        );
        let text = self.other_total_text.clone();
        if let Some(t) = self.other_total.and_then(|h| ui.text_element_mut(h)) {
            t.set_text(&text);
        }
    }

    /// The trade button's own state on `0x10000086`, written through
    /// the button element's state behavior.
    ///
    /// Routed through [`dereth_ui::UiSystem::set_state`] rather than through
    /// [`super::statmgmt::set_button_state_at`] deliberately: a state write on a **toggle** button
    /// is not a state write at all, it is a write of boolean attribute `0x0E` and an early return, and only
    /// the full override decides which of the three arms runs. Writing the disabled attribute
    /// directly would answer `6` and `1` identically. This panel is outside the behaviour lift
    /// when it runs, which is the condition `Footer::set_button_state`'s note records.
    fn write_button_state(&mut self, ui: &mut UiSystem) {
        if let Some(h) = self.trade_button {
            ui.set_state(h, dereth_ui::StateId(self.button as u32));
        }
    }

    fn set_visible(&mut self, ui: &mut UiSystem, visible: bool) {
        self.visible = visible;
        for h in [self.root].into_iter().flatten() {
            ui.set_visible(h, visible);
        }
    }
}

/// Resolve [`ID_TOTAL_ITEMS_LABEL`] in [`STRING_TABLE`] and put `value` where its variable is.
///
/// The same shape as `allegiance`'s own resolver, and the same reason for the fall-back: with no
/// string table installed — every headless test — a label that silently drew nothing would be
/// indistinguishable from a label that was never written, so the token and the value are written
/// instead and a test can still see *which* string went in and *what* was substituted.
///
/// The substitution itself is [`UiSystem::resolve_string_rendered`] — the string lookup's
/// meta-language arm.
fn fill(ui: &UiSystem, token: &str, value: &str) -> String {
    let hash = dereth_primitives::num::hash::str_hash(token.as_bytes());
    match ui.resolve_string_variant_count(STRING_TABLE, hash) {
        Some(2) => ui
            .resolve_string_rendered(STRING_TABLE, hash, &[value.to_owned()])
            .unwrap_or_else(|| format!("{token} {value}")),
        _ => format!("{token} {value}"),
    }
}

/// The tile read for `id`, out of the two parallel vectors [`TradePanel::update`] builds.
///
/// A **positional lookup**, not a mapping: `ids[i]` and `tiles[i]` are one read of the seam split
/// into a key and a value, so nothing here can be inconsistent with anything else — which is why
/// it is not a duplicated mapping. `None` covers both "not a tile of this
/// window" and "an id with no weenie object yet", and `decorate` treats them alike: that is
/// the client's no-object early-out, which leaves the slot as the fill left it.
fn tile_at<'a>(
    ids: &[dereth_primitives::ObjectId],
    tiles: &'a [Option<TileInfo>],
    id: dereth_primitives::ObjectId,
) -> Option<&'a TileInfo> {
    ids.iter()
        .zip(tiles)
        .find_map(|(tid, t)| (*tid == id).then_some(t.as_ref()))?
}

/// The nearest ancestor of `h` whose type is [`ELEMENT_CLASS`] — the secure-trade panel.
///
/// The element class registration for `0x10000012` is what puts that type on the window, and the shipped tree instantiates it: the ten child ids are all live and each appears
/// exactly once.
fn window_of(ui: &UiSystem, mut h: ElemHandle) -> Option<ElemHandle> {
    loop {
        if ui.node(h).is_some_and(|n| n.ty().0 == ELEMENT_CLASS) {
            return Some(h);
        }
        h = ui.parent(h)?;
    }
}

impl TradePanel {
    /// The item list's element-message handler, delivered to the two lists
    /// owned here rather than by GamePlayScreen. Vendor lists still select/examine; only the
    /// double-click Use arm tests vendor/salvage. Empty slots do not deselect anything.
    pub fn on_item_list_press(&mut self, ui: &mut UiSystem, m: &dereth_ui::ElementMessage) -> bool {
        if m.id != dereth_ui::msg::element::id::MOUSE_PRESS {
            return false;
        }
        for list in [self.self_list.as_mut(), self.other_list.as_mut()]
            .into_iter()
            .flatten()
        {
            let Some(slot) = list.slot_of(m.source) else {
                continue;
            };
            let Some(item) = list.item_at(slot).filter(|id| id.0 != 0) else {
                return true;
            };
            match m.p1 {
                7 if self.target_mode_active => {
                    ui.requests.emit(UiRequest::ExecuteTargetItem(item))
                }
                7 | 8 => {
                    if list.single_selection {
                        list.handle_single_selection(ui, slot);
                    }
                    ui.requests.emit(UiRequest::Select(item));
                    if m.p1 == 8 {
                        ui.requests.emit(UiRequest::Examine(item));
                    }
                }
                10 if !list.vendor_list && !list.salvage_list => {
                    ui.requests.emit(UiRequest::Use(item))
                }
                _ => {}
            }
            return true;
        }
        false
    }

    /// The secure trade panel's element-message handler.
    ///
    /// **Two message ids, not one**: `1` is a button click and `0x15` is a drop release. Only the
    /// first is handled here; the drop arrives through the panel's drag handler.
    pub fn on_element_message(
        &mut self,
        requests_out: &mut crate::requests::Outbox,
        m: &dereth_ui::ElementMessage,
    ) -> bool {
        if m.id != dereth_ui::msg::element::id::BUTTON_CLICKED {
            return false;
        }
        self.handle_button_click(requests_out, m.source_id)
    }

    /// The three `case` labels of the element-message handler's message-`1` arm, as requests.
    ///
    /// See this module's header for why the trade button's two arms are the way round they are.
    pub fn handle_button_click(
        &mut self,
        requests_out: &mut crate::requests::Outbox,
        id: ElementId,
    ) -> bool {
        match id {
            BTN_TRADE => {
                if let Some(request) = self
                    .button
                    .press(self.displayed_self.len(), self.displayed_other.len())
                {
                    requests_out.emit(request);
                }
                true
            }
            BTN_CLEAR_ALL => {
                requests_out.emit(UiRequest::TradeReset);
                true
            }
            BTN_CLOSE => {
                requests_out.emit(UiRequest::TradeClose);
                true
            }
            _ => false,
        }
    }

    /// The secure trade panel's drag-accept test's whole-stack arm, plus the item insert's
    /// optimistic insert.
    ///
    /// The **position** the request carries is the row's index — where the row landed in the window's own list — which is the second dword of `0x01F8`. It is computed
    /// here because the list is here.
    ///
    /// Returns false when the item is already on the table, which is the add's `-1` return.
    pub fn drop_item(
        &mut self,
        requests_out: &mut crate::requests::Outbox,
        item: dereth_primitives::ObjectId,
    ) -> bool {
        self.drop_item_with_split(requests_out, item, 1, 1)
    }

    /// The pointer-drop entry point with the held item's quantity captured by that gesture.
    /// A partial stack asks the game to split first and deliberately inserts no optimistic source
    /// row; `ItemAttributesChanged` later returns the authoritative result through
    /// [`Self::offer_item_at`].
    pub fn drop_item_with_split(
        &mut self,
        requests_out: &mut crate::requests::Outbox,
        item: dereth_primitives::ObjectId,
        split: u32,
        max: u32,
    ) -> bool {
        if self.displayed_self.contains(&item) {
            return false;
        }
        if split < max {
            requests_out.emit(UiRequest::TradeSplitItem { item, split, max });
            return true;
        }
        let position = u32::try_from(self.displayed_self.len()).unwrap_or(u32::MAX);
        self.offer_item_at(requests_out, item, position)
    }

    /// The add, for callers whose native path supplies the position.
    pub fn offer_item_at(
        &mut self,
        requests_out: &mut crate::requests::Outbox,
        item: dereth_primitives::ObjectId,
        position: u32,
    ) -> bool {
        if self.displayed_self.contains(&item) {
            return false;
        }
        let at = usize::try_from(position)
            .unwrap_or(usize::MAX)
            .min(self.displayed_self.len());
        self.displayed_self.insert(at, item);
        // The row insert puts the row on screen *before* the wire, and the
        // next `update` is where this build's fill runs -- so the id is recorded as pending and
        // the snapshot guard is dropped, or the guard would hold the row off until the `0x0200`
        // moved `TradeView::self_rows` and the item would be invisible for a round trip.
        self.pending.push(item);
        self.last = None;
        requests_out.emit(UiRequest::TradeAddItem { item, position });
        self.button = ButtonState::Enabled;
        self.button = self.update_button_state();
        true
    }

    /// The secure-trade panel's drag-acceptable test, **both halves**, as the state the tile
    /// under the drag cursor is given. [`Self::on_drag_cursor_over`] is what calls it.
    ///
    /// Retail refuses, in order: an unknown object (silently); an object the player does not own
    /// (with `"You can only trade items you are carrying"`, type `0x1A`, unless quiet); an object
    /// already in your list (silently). It then counts the object's contained items, which is
    /// dead (see below), and accepts.
    ///
    /// [`Self::on_drag_cursor_over`] delivers the `0x3E` that reaches it. The ownership half comes
    /// across the seam through
    /// [`GameView::trade_drag_item_acceptable`], for the reason
    /// [`crate::panels::salvage::SalvagePanel::drag_accept_state`]'s does: the object being
    /// carried is in the **pack**, not in this window's list, so the panel cannot answer it.
    ///
    /// **`quiet` is on here**, so hovering an item you are not carrying
    /// over the table is refused **silently**; only the drop speaks, because
    /// the drop-time drag-accept test uses `quiet = 0`.
    ///
    /// **The contained-item count is dead code.** Its result is discarded and the function returns
    /// true regardless, so a **full pack** is acceptable to
    /// both the hint and the drop. Trading a container trades its
    /// contents.
    #[must_use]
    pub fn drag_accept_state(
        &self,
        item: dereth_primitives::ObjectId,
        view: &dyn GameView,
    ) -> dereth_ui::StateId {
        if !view.trade_drag_item_acceptable(item) || self.displayed_self.contains(&item) {
            DRAG_REFUSE
        } else {
            DRAG_ACCEPT
        }
    }

    /// The client's **`0x3E` arm**, for the two lists
    /// this window owns — the drag hints over the trade table.
    ///
    /// # The window offers exactly two drop targets, and they are not symmetric
    ///
    /// The client registers an item-list drag handler **once**, on the self list (`0x10000088`)
    /// it has just fetched, and not on the partner list (`0x10000081`) it fetches next.
    ///
    /// So:
    ///
    /// * **your side (`0x10000088`)** gets the panel handler, which
    ///   is [`Self::drag_accept_state`] and **always returns true** -- once the cursor is over one
    ///   of your tiles, the item list's three-way drag-over default can never run;
    /// * **the partner's side (`0x10000081`)** has no handler, so it gets the list widget's
    ///   **default** -- and that default's first test is the vendor-style flag.
    ///   **Both of this window's lists carry that flag** (attribute `0x10000013`; measured off the
    ///   live tree, because resolution through `base_element`/`base_layout` makes a raw layout
    ///   scan disagree).
    ///   So the default returns with no drag-accept write at all and the
    ///   partner's half of the table shows **nothing**: it does not lie green about a drop that
    ///   the drop handler's "is the catcher inside your list" test is going to discard. On your own side the same flag is invisible, because the handler above the
    ///   default always returns true.
    ///
    ///   The default is still *called* below rather than skipped, because the flag is a fact
    ///   about the layout and not about the trade panel: [`ItemListWidget::drag_over`] is
    ///   the shared default and reads that flag, so a layout that ever cleared it would get
    ///   retail's behaviour here without an edit.
    /// * **the window's background, the two names, the two totals, the partner's light and the
    ///   three buttons** are not item elements and carry no drag-catcher attribute.
    ///   The mouse-over path reads catcher attribute `0x36`, walking to the parent when unset; it
    ///   never resolves to one of those controls, so **no `0x3E` is
    ///   raised at all**. Carrying an item across the window chrome takes the last tile's hint
    ///   **down** -- the leave `0x3E` with `p1 == 0` -- and paints nothing.
    ///
    /// The handler itself, in full: a zero item id, or drop flags with any of `0x0E` set
    /// (the alias mask), returns true with no hint at all; otherwise the hovered tile is
    /// painted `0x10000040` if the item is acceptable (checked quietly) and `0x10000041` if not.
    /// It **always** returns true.
    ///
    /// The alias mask is `0x0E` -- three bits, not one -- and on that arm the tile keeps
    /// **whatever state it already had**: a shortcut, a spell or a salvage drag over the table
    /// gets no hint rather than a red one.
    ///
    /// Returns true when the message was one of this window's two lists, which is what
    /// `RemainingPanels::on_element_message`'s fan-out reads.
    pub fn on_drag_cursor_over(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        use crate::items::widget::{drag_flags, inq_drop_icon_info};
        if m.id != dereth_ui::msg::element::id::DRAG_CURSOR_OVER {
            return false;
        }
        let partner = if self
            .self_list
            .as_ref()
            .is_some_and(|w| w.slot_of(m.source).is_some())
        {
            false
        } else if self
            .other_list
            .as_ref()
            .is_some_and(|w| w.slot_of(m.source).is_some())
        {
            true
        } else {
            return false;
        };
        let Some(slot) = self.list_mut(partner).and_then(|w| w.slot_of(m.source)) else {
            return false;
        };
        // Both of the client's clearing routes, neither of which reaches the list at all:
        // `p1 == 0` (the cursor left), and `p1 != 0` with no drag element.
        let Some(proxy) = ui.drag_state().element.filter(|_| m.p1 != 0) else {
            if let Some(w) = self.list_mut(partner) {
                w.slots[slot].set_drag_accept_state(ui, DRAG_NONE);
            }
            return true;
        };
        let info = inq_drop_icon_info(ui, proxy);
        if partner {
            // No drag handler on `0x10000081`, so the client's own default runs. The empty-slot
            // count of the object in the slot under the pointer is read off that slot, which is the only object the default asks about.
            let under = self
                .other_list
                .as_ref()
                .and_then(|w| w.slots.get(slot))
                .and_then(|s| s.item);
            let free = self
                .other_list
                .as_ref()
                .and_then(|w| w.slots.get(slot))
                .map(crate::items::widget::ItemSlot::num_empty_item_slots);
            if let Some(w) = self.other_list.as_mut() {
                w.drag_over(ui, slot, info, &|id| {
                    if Some(id) == under {
                        free
                    } else {
                        None
                    }
                });
            }
            return true;
        }
        let Some(item) = info
            .item
            .filter(|_| info.flags & drag_flags::NOT_AN_INVENTORY_MOVE == 0)
        else {
            // The client: `true` with no drag-accept write at all.
            return true;
        };
        let state = self.drag_accept_state(item, view);
        if let Some(w) = self.self_list.as_mut() {
            w.slots[slot].set_drag_accept_state(ui, state);
        }
        true
    }

    /// The client's **`0x15` arm** -- the clear that
    /// takes [`Self::on_drag_cursor_over`]'s hint back down when the drop lands — the same half
    /// the vendor's three lists, the quickbar's eighteen and the salvage window's one have.
    ///
    /// On message `0x15`, clear the catcher element's drag-accept state to `0x1000003F`
    /// if it casts to UI-item type `0x10000032`, then handle the drop release. The catcher
    /// receives the state write, not the list.
    ///
    /// The arm belongs to the item-list widget, not the window, so it runs on *both* of this
    /// window's lists -- including the partner's, where the drop is then discarded by
    /// the client's ancestor test. It is
    /// unconditional and runs before the drop is handled, and since there is no `0x3F` drag-leave
    /// message in the client and the pointer never leaves the tile it was dropped on, it is
    /// the only producer of the clear on that path.
    pub fn on_drop_release(&mut self, ui: &mut UiSystem, m: &dereth_ui::ElementMessage) -> bool {
        // The **catcher's** copy of the message, which is the one carrying the drag's owner in
        // `p2`; the owner's own copy has `p2 == 0` and names the slot the drag started from. Same
        // reading `SalvagePanel::on_drop_release` and `VendorPanel::on_drop_release` take.
        if m.id != dereth_ui::msg::element::id::DROP_FAILED || m.p2 == 0 {
            return false;
        }
        let mut hit = false;
        for partner in [false, true] {
            let Some(slot) = self.list_mut(partner).and_then(|w| w.slot_of(m.source)) else {
                continue;
            };
            if let Some(w) = self.list_mut(partner) {
                w.slots[slot].set_drag_accept_state(ui, DRAG_NONE);
            }
            hit = true;
        }
        hit
    }

    /// Your item list or the partner's.
    fn list_mut(&mut self, partner_side: bool) -> Option<&mut ItemListWidget> {
        if partner_side {
            self.other_list.as_mut()
        } else {
            self.self_list.as_mut()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::ObjectId;

    #[derive(Debug, Default)]
    struct V(
        TradeView,
        std::collections::BTreeMap<ObjectId, Obj>,
        Option<(u32, f64, f64)>,
    );

    /// One game object, with the fields this seam exposes.
    #[derive(Debug, Clone, Default)]
    struct Obj {
        name: String,
        plural: String,
        decoration: crate::view::SlotDecoration,
    }

    impl V {
        fn new(v: TradeView) -> Self {
            Self(v, std::collections::BTreeMap::new(), None)
        }

        /// The same object table under a different snapshot — used to prove a station's snapshot
        /// really is identical rather than merely believed to be.
        fn clone_view(&self, v: TradeView) -> Self {
            Self(v, self.1.clone(), self.2)
        }

        /// A live cooldown: `(key, start_time, duration)`.
        fn with_cooldown(mut self, key: u32, start: f64, duration: f64) -> Self {
            self.2 = Some((key, start, duration));
            self
        }

        fn with_object(self, id: ObjectId, name: &str, icon: DataId) -> Self {
            self.with_stack(id, name, icon, 1)
        }

        /// The same object with a stack size, which is the field the tooltip draws and the one
        /// this row exists for.
        fn with_stack(mut self, id: ObjectId, name: &str, icon: DataId, stack: u32) -> Self {
            self.1.insert(
                id,
                Obj {
                    name: name.to_owned(),
                    plural: String::new(),
                    // `HudView::slot_decoration`'s own two fields for this shape: the icon id
                    // straight through, and the stack size with the tooltip update's
                    // zero-becomes-one already applied.
                    decoration: crate::view::SlotDecoration {
                        icon_id: icon.0,
                        stack_size: stack.max(1),
                        ..crate::view::SlotDecoration::default()
                    },
                },
            );
            self
        }

        /// Move one field of one object's decoration, leaving every other input still.
        fn mutate(mut self, id: ObjectId, f: impl FnOnce(&mut Obj)) -> Self {
            if let Some(o) = self.1.get_mut(&id) {
                f(o);
            }
            self
        }
    }

    impl GameView for V {
        fn trade(&self) -> TradeView {
            self.0.clone()
        }

        fn name(&self, id: ObjectId) -> Option<&str> {
            self.1.get(&id).map(|o| o.name.as_str())
        }

        fn plural_name(&self, id: ObjectId) -> Option<&str> {
            self.1.get(&id).map(|o| o.plural.as_str())
        }

        /// The client's ownership half. **True for every id here**, which is
        /// the fixture saying "the player is carrying it": these stations are about the
        /// already-in-list half, not inventory ownership.
        fn trade_drag_item_acceptable(&self, _item: ObjectId) -> bool {
            true
        }

        fn slot_decoration(&self, id: ObjectId) -> Option<crate::view::SlotDecoration> {
            self.1.get(&id).map(|o| o.decoration)
        }

        fn icon(&self, id: ObjectId) -> Option<DataId> {
            // `HudView::icon`'s own `(i != 0).then_some(...)`, off the same field
            // `slot_decoration` carries — one record, four accessors.
            self.1
                .get(&id)
                .and_then(|o| (o.decoration.icon_id != 0).then_some(DataId(o.decoration.icon_id)))
        }

        /// `(duration + start time) - now`, with the client's `<= 0` removal
        /// arm — **a function of the clock**, which is why it is exempted from the guard.
        fn cooldown_remaining(&self, cooldown_id: u32, now: f64) -> Option<f64> {
            let (key, start, duration) = self.2?;
            if key != cooldown_id {
                return None;
            }
            let left = (start + duration) - now;
            (left > 0.0).then_some(left)
        }
    }

    /// **The eleven ids, as literals**, from the post-init's child lookups, the element class
    /// registration, the element-message handler's
    /// three cases and the item-list drag-over's two accept states.
    ///
    /// the stated testability rule: *"a test that reads a constant through the same symbol it writes it
    /// through cannot detect a wrong constant"*. Everything else in this file uses the symbols;
    /// this states the numbers.
    #[test]
    fn the_element_ids_are_the_ones_the_binary_passes() {
        assert_eq!(ELEMENT_CLASS, 0x1000_0012);
        assert_eq!(WINDOW, ElementId(0x1000_005F));
        assert_eq!(OTHER_NAME, ElementId(0x1000_007E));
        assert_eq!(OTHER_STATUS, ElementId(0x1000_007F));
        assert_eq!(OTHER_TOTAL, ElementId(0x1000_0080));
        assert_eq!(OTHER_LIST, ElementId(0x1000_0081));
        assert_eq!(SELF_NAME, ElementId(0x1000_0085));
        assert_eq!(BTN_TRADE, ElementId(0x1000_0086));
        assert_eq!(SELF_TOTAL, ElementId(0x1000_0087));
        assert_eq!(SELF_LIST, ElementId(0x1000_0088));
        assert_eq!(BTN_CLEAR_ALL, ElementId(0x1000_008A));
        assert_eq!(BTN_CLOSE, ElementId(0x1000_008B));
        assert_eq!(DRAG_ACCEPT, dereth_ui::StateId(0x1000_0040));
        assert_eq!(DRAG_REFUSE, dereth_ui::StateId(0x1000_0041));
        assert_eq!(DRAG_NONE, dereth_ui::StateId(0x1000_003F));
        // The element-message handler's three cases are exactly 0x86, 0x8A and 0x8B — four apart,
        // then one apart at the end.
        assert_eq!(BTN_CLEAR_ALL.0 - BTN_TRADE.0, 4);
        assert_eq!(BTN_CLOSE.0 - BTN_CLEAR_ALL.0, 1);
        // …and the button's three states.
        assert_eq!(ButtonState::Disabled as u32, 0x0D);
        assert_eq!(ButtonState::Enabled as u32, 1);
        assert_eq!(ButtonState::Accepted as u32, 6);
    }

    fn panel_with(displayed_self: usize, displayed_other: usize) -> TradePanel {
        let ids = |base: u32, n: usize| {
            (0..n)
                .map(|i| ObjectId(base + u32::try_from(i).expect("a small list")))
                .collect()
        };
        TradePanel {
            displayed_self: ids(0x8000_0000, displayed_self),
            displayed_other: ids(0x9000_0000, displayed_other),
            ..TradePanel::default()
        }
    }

    ///  counts **both** lists. A build that counted only your
    /// own side would re-enable the button when the partner emptied theirs.
    #[test]
    fn the_button_state_counts_both_lists() {
        let mut p = panel_with(0, 0);
        p.button = ButtonState::Enabled;
        assert_eq!(p.update_button_state(), ButtonState::Disabled);

        let mut p = panel_with(0, 2);
        p.button = ButtonState::Enabled;
        assert_eq!(
            p.update_button_state(),
            ButtonState::Enabled,
            "the partner's rows count"
        );
        p.button = ButtonState::Disabled;
        assert_eq!(p.update_button_state(), ButtonState::Enabled);

        let mut p = panel_with(1, 0);
        p.button = ButtonState::Accepted;
        assert_eq!(
            p.update_button_state(),
            ButtonState::Accepted,
            "6 survives a non-empty table"
        );
        p.displayed_self.clear();
        assert_eq!(
            p.update_button_state(),
            ButtonState::Disabled,
            "…and not an empty one"
        );
    }

    /// The trade button is a **toggle**: `Enabled -> Accepted` sends the accept and
    /// `Accepted -> Enabled` sends the decline, and at `Disabled` the client's `if` chain has no
    /// arm at all so the press sends nothing.
    #[test]
    fn the_trade_button_toggles_between_accept_and_decline() {
        let mut outbox = crate::requests::Outbox::owned();
        let mut p = panel_with(2, 1);
        p.button = ButtonState::Disabled;
        outbox.clear();
        assert!(p.handle_button_click(&mut outbox, BTN_TRADE));
        assert_eq!(outbox.len(), 0, "0x0D has no case");

        p.button = ButtonState::Enabled;
        outbox.clear();
        assert!(p.handle_button_click(&mut outbox, BTN_TRADE));
        assert_eq!(
            outbox.take(),
            vec![UiRequest::TradeAccept {
                displayed_self: 2,
                displayed_partner: 1
            }]
        );
        assert_eq!(p.button, ButtonState::Accepted);

        outbox.clear();
        assert!(p.handle_button_click(&mut outbox, BTN_TRADE));
        assert_eq!(outbox.take(), vec![UiRequest::TradeDecline]);
        assert_eq!(p.button, ButtonState::Enabled);
    }

    /// The other two cases, and the ids that are **not** cases.
    #[test]
    fn the_other_two_buttons_emit_their_own_requests() {
        let mut outbox = crate::requests::Outbox::owned();
        let mut p = panel_with(1, 0);
        outbox.clear();
        assert!(p.handle_button_click(&mut outbox, BTN_CLEAR_ALL));
        assert_eq!(outbox.take(), vec![UiRequest::TradeReset]);
        outbox.clear();
        assert!(p.handle_button_click(&mut outbox, BTN_CLOSE));
        assert_eq!(outbox.take(), vec![UiRequest::TradeClose]);
        outbox.clear();
        for id in [
            SELF_LIST,
            OTHER_LIST,
            SELF_NAME,
            OTHER_STATUS,
            SELF_TOTAL,
            OTHER_TOTAL,
        ] {
            assert!(
                !p.handle_button_click(&mut outbox, id),
                "{id:?} is not one of the three cases"
            );
        }
        assert_eq!(outbox.len(), 0);
    }

    /// A drop inserts optimistically and carries the row's **position**, which is what `0x01F8`'s
    /// second dword is. A second drop of the same item is the add's `-1` return.
    #[test]
    fn a_drop_carries_the_position_the_row_landed_at() {
        let mut outbox = crate::requests::Outbox::owned();
        let mut p = TradePanel::default();
        let a = ObjectId(0x8000_0A6E);
        let b = ObjectId(0x8000_0A6F);
        outbox.clear();
        assert!(p.drop_item(&mut outbox, a));
        assert!(p.drop_item(&mut outbox, b));
        assert_eq!(
            outbox.take(),
            vec![
                UiRequest::TradeAddItem {
                    item: a,
                    position: 0
                },
                UiRequest::TradeAddItem {
                    item: b,
                    position: 1
                },
            ]
        );
        assert_eq!(p.displayed_self, vec![a, b]);
        assert_eq!(
            p.button,
            ButtonState::Enabled,
            "the first row enables the button"
        );

        outbox.clear();
        assert!(!p.drop_item(&mut outbox, a), "already on the table");
        assert_eq!(outbox.len(), 0);
        // The item list drag over handler's two accept states.
        // `V` answers `trade_drag_item_acceptable` for every id, so what is under test here is
        // the already-in-list half alone, not inventory ownership.
        let carried = V::new(TradeView::default());
        assert_eq!(p.drag_accept_state(a, &carried), DRAG_REFUSE);
        assert_eq!(
            p.drag_accept_state(ObjectId(0x8000_1111), &carried),
            DRAG_ACCEPT
        );
    }

    /// The guard holds when the retain above it is a no op.
    #[test]
    fn the_guard_holds_when_the_retain_above_it_is_a_no_op() {
        let mut ui = UiSystem::new((800, 600));
        let mut p = TradePanel::default();
        let key = ObjectId(0x8000_0A6E);
        let v = V::new(TradeView {
            open: true,
            ..TradeView::default()
        })
        .with_object(key, "Sturdy Iron Key", DataId(0x0600_103F));

        assert!(p.update(&mut ui, &v), "the window opens");
        assert!(p.drop_item(&mut ui.requests, key));
        assert!(
            p.update(&mut ui, &v),
            "the optimistic insert defeats the guard once"
        );
        assert_eq!(
            p.pending,
            vec![key],
            "the premise: there is something for the retain to eat"
        );

        let n = p.rebuilds;
        assert!(
            !p.update(&mut ui, &v),
            "and the identical frame is held off"
        );
        assert_eq!(p.rebuilds, n, "…really held off");
        assert_eq!(
            p.pending,
            vec![key],
            "and the retain above the guard removed nothing"
        );
    }

    /// Each decoration field alone defeats the trade guard.
    #[test]
    fn each_decoration_field_alone_defeats_the_trade_guard() {
        let mut ui = UiSystem::new((800, 600));
        let mut p = TradePanel::default();
        let arrow = ObjectId(0x8000_0A6E);
        let view = TradeView {
            open: true,
            partner_rows: vec![TradeRow {
                item: arrow,
                name: "Arrow".into(),
                icon: Some(DataId(0x0600_103F)),
            }],
            ..TradeView::default()
        };
        let base = V::new(view.clone()).with_stack(arrow, "Arrow", DataId(0x0600_103F), 250);
        assert!(
            p.update(&mut ui, &base),
            "the first drive is always a rebuild"
        );
        assert!(
            !p.update(&mut ui, &base),
            "the premise: an identical frame is held off"
        );

        let mut n = p.rebuilds;
        let mut station = |p: &mut TradePanel, ui: &mut UiSystem, v: &V, why: &str| {
            assert_eq!(
                v.0, view,
                "the snapshot is identical across the two frames: {why}"
            );
            assert!(p.update(ui, v), "{why}");
            n += 1;
            assert!(!p.update(ui, v), "and the new value settles: {why}");
            assert_eq!(p.rebuilds, n, "{why}");
        };

        let v = base
            .clone_view(view.clone())
            .mutate(arrow, |o| o.decoration.stack_size = 249);
        station(&mut p, &mut ui, &v, "the stack size alone");

        let v = v.mutate(arrow, |o| {
            o.decoration.icon_overlay_id = Some(DataId(0x0600_2222))
        });
        station(&mut p, &mut ui, &v, "the icon overlay alone");

        let v = v.mutate(arrow, |o| o.name = "Deadly Arrow".into());
        station(&mut p, &mut ui, &v, "the name alone");

        let v = v.mutate(arrow, |o| o.plural = "Deadly Arrows".into());
        station(&mut p, &mut ui, &v, "the plural name alone");
    }

    /// **The exemption.** `cooldown_remaining` is a function of the clock, so folding it into the
    /// guard would make the early-out fire **never** — and it would look correct on screen. The
    /// premise is asserted first: the view really does answer two different values at the two
    /// times, so a build that folded it in really would rebuild.
    #[test]
    fn a_running_cooldown_does_not_defeat_the_trade_guard() {
        let mut ui = UiSystem::new((800, 600));
        let mut p = TradePanel::default();
        let arrow = ObjectId(0x8000_0A6E);
        let v = V::new(TradeView {
            open: true,
            partner_rows: vec![TradeRow {
                item: arrow,
                name: "Arrow".into(),
                icon: Some(DataId(0x0600_103F)),
            }],
            ..TradeView::default()
        })
        .with_stack(arrow, "Arrow", DataId(0x0600_103F), 250)
        .mutate(arrow, |o| {
            o.decoration.cooldown_id = 77;
            o.decoration.cooldown_duration = 30.0;
        })
        .with_cooldown(77, 1000.0, 30.0);

        ui.now = dereth_primitives::LocalTime(1000.0);
        assert!(p.update(&mut ui, &v), "the first drive");
        let n = p.rebuilds;

        let before = v
            .cooldown_remaining(77, 1000.0)
            .expect("a live cooldown at t=1000");
        let after = v
            .cooldown_remaining(77, 1010.0)
            .expect("still live at t=1010");
        assert!(
            (before - after - 10.0).abs() < 1e-9,
            "the premise: the clock moves it by the elapsed time, {before} -> {after}"
        );

        ui.now = dereth_primitives::LocalTime(1010.0);
        assert!(
            !p.update(&mut ui, &v),
            "…and the guard does not notice, which is the point"
        );
        assert_eq!(p.rebuilds, n);
    }

    /// An unbound panel is honest about it, and a **closed** window is rendered rather than
    /// skipped: the third state the live-run evidence asks for.
    #[test]
    fn an_unbound_panel_still_reports_whether_it_ran_and_a_close_flushes() {
        let mut ui = UiSystem::new((800, 600));
        let mut p = TradePanel::default();
        assert!(!p.bound());
        assert_eq!(p.rebuilds, 0);

        let open = V::new(TradeView {
            open: true,
            partner: Some(ObjectId(0x5000_0002)),
            partner_name: "Alba".into(),
            self_rows: vec![TradeRow {
                item: ObjectId(1),
                name: "Sturdy Iron Key".into(),
                icon: None,
            }],
            partner_rows: vec![TradeRow {
                item: ObjectId(2),
                name: "Pyreal".into(),
                icon: None,
            }],
            accepted: false,
            partner_accepted: true,
            acceptance_darkened: false,
            self_removed: Vec::new(),
        });
        assert!(
            p.update(&mut ui, &open),
            "the first drive is always a rebuild"
        );
        assert_eq!(p.rebuilds, 1);
        assert_eq!(p.rows(false).len(), 1);
        assert_eq!(p.rows(true)[0].name, "Pyreal");
        assert_eq!(p.button, ButtonState::Enabled);
        assert!(p.visible);
        assert!(
            !p.update(&mut ui, &open),
            "the snapshot guard holds the second frame off"
        );
        assert_eq!(p.rebuilds, 1);

        let closed = V::new(TradeView::default());
        assert!(p.update(&mut ui, &closed));
        assert_eq!(p.rebuilds, 2);
        assert!(!p.visible);
        assert!(
            p.rows(false).is_empty() && p.rows(true).is_empty(),
            "a close flushes both lists"
        );
        assert!(p.displayed_self.is_empty() && p.displayed_other.is_empty());
        assert_eq!(p.button, ButtonState::Disabled);
    }

    /// The panel's own lists follow the **server's** confirmation, and the button follows
    /// _accepted — so an accept the server has not echoed does not survive a rebuild.
    #[test]
    fn a_rebuild_takes_the_confirmed_list_and_keeps_the_unconfirmed_drop() {
        let mut ui = UiSystem::new((800, 600));
        let mut p = TradePanel::default();
        ui.requests.clear();
        p.drop_item(&mut ui.requests, ObjectId(7));
        ui.requests.clear();
        assert_eq!(p.displayed_self, vec![ObjectId(7)]);
        assert_eq!(
            p.pending,
            vec![ObjectId(7)],
            "…and it is pending, not confirmed"
        );

        let v = V::new(TradeView {
            open: true,
            self_rows: vec![TradeRow {
                item: ObjectId(8),
                name: "n".into(),
                icon: None,
            }],
            accepted: true,
            ..TradeView::default()
        });
        p.update(&mut ui, &v);
        assert_eq!(
            p.displayed_self,
            vec![ObjectId(8), ObjectId(7)],
            "the server's rows first, then the drop it has not answered yet"
        );
        assert_eq!(p.pending, vec![ObjectId(7)]);
        assert_eq!(
            p.button,
            ButtonState::Accepted,
            "…and the server's accept flag wins"
        );

        // The confirmation. finds the row already in the list and leaves it
        // there, so the id leaves `pending` and **no second row appears**.
        let confirmed = V::new(TradeView {
            open: true,
            self_rows: vec![
                TradeRow {
                    item: ObjectId(8),
                    name: "n".into(),
                    icon: None,
                },
                TradeRow {
                    item: ObjectId(7),
                    name: "Sturdy Iron Key".into(),
                    icon: None,
                },
            ],
            accepted: true,
            ..TradeView::default()
        });
        p.update(&mut ui, &confirmed);
        assert_eq!(
            p.displayed_self,
            vec![ObjectId(8), ObjectId(7)],
            "no duplicate row"
        );
        assert!(
            p.pending.is_empty(),
            "the 0x0200 named it, so it is no longer optimistic"
        );
        assert_eq!(
            p.rows(false)[1].name,
            "Sturdy Iron Key",
            "and the server's row replaced it"
        );
    }

    /// A dropped item draws before any reply with its name and icon.
    #[test]
    fn a_dropped_item_draws_before_any_reply_with_its_name_and_icon() {
        let mut ui = UiSystem::new((800, 600));
        let mut p = TradePanel::default();
        let key = ObjectId(0x8000_0A6E);
        let open = V::new(TradeView {
            open: true,
            partner: Some(ObjectId(0x5000_0002)),
            partner_name: "Alba".into(),
            ..TradeView::default()
        })
        .with_object(key, "Sturdy Iron Key", DataId(0x0600_103F));

        assert!(p.update(&mut ui, &open), "the window opens");
        assert!(
            p.rows(false).is_empty(),
            "the premise: nothing on your side yet"
        );
        let before = p.rebuilds;

        ui.requests.clear();
        assert!(p.drop_item(&mut ui.requests, key));
        assert_eq!(ui.requests.len(), 1, "it does go on the wire");
        ui.requests.clear();

        // **The same view.** Nothing has come back; the server has said nothing at all.
        assert!(
            p.update(&mut ui, &open),
            "the panel's own insert must defeat the snapshot guard"
        );
        assert_eq!(p.rebuilds, before + 1);
        assert_eq!(p.rows(false).len(), 1, "the optimistic row is drawn");
        assert_eq!(p.rows(false)[0].item, key);
        assert_eq!(
            p.rows(false)[0].name,
            "Sturdy Iron Key",
            "GameView::name crossed the seam"
        );
        assert_eq!(
            p.rows(false)[0].icon,
            Some(DataId(0x0600_103F)),
            "…and GameView::icon"
        );
        // And the two things the add updates in the same breath as the insert.
        assert_eq!(p.self_total_text, "ID_SecureTrade_TotalItemsLabel 1");
        assert_eq!(p.button, ButtonState::Enabled);
    }

    /// Each bound element follows its own source and no other.
    #[test]
    fn each_bound_element_follows_its_own_source_and_no_other() {
        let mut ui = UiSystem::new((800, 600));
        let mut p = TradePanel::default();
        let a = ObjectId(1);
        let b = ObjectId(2);
        let base = TradeView {
            open: true,
            partner: Some(ObjectId(0x5000_0002)),
            partner_name: "Alba".into(),
            self_rows: vec![TradeRow {
                item: a,
                name: "Key".into(),
                icon: None,
            }],
            partner_rows: vec![],
            accepted: false,
            partner_accepted: false,
            self_removed: Vec::new(),
            acceptance_darkened: false,
        };
        p.update(&mut ui, &V::new(base.clone()));

        // 1. OTHER_NAME 0x1000007E <- TradeView::partner_name.
        assert_eq!(p.partner_name_text, "Alba");
        // 2. OTHER_STATUS 0x1000007F <- TradeView::partner_accepted.
        assert_eq!(p.partner_status, status_state::NOT_ACCEPTED);
        // 3. SELF_TOTAL 0x10000087 <- the row count of your list.
        assert_eq!(p.self_total_text, "ID_SecureTrade_TotalItemsLabel 1");
        // 4. OTHER_TOTAL 0x10000080 <- the row count of the partner's list, a **different** list.
        assert_eq!(p.other_total_text, "ID_SecureTrade_TotalItemsLabel 0");
        // 5. The trade button's own state.
        assert_eq!(p.button, ButtonState::Enabled);
        // 6. SELF_NAME 0x10000085 is bound and never written -- see the module header.
        assert!(
            p.self_name.is_none(),
            "nothing is bound in a bare UiSystem, and nothing writes it"
        );

        // The partner's name alone.
        let mut v = base.clone();
        v.partner_name = "Aldis".into();
        p.update(&mut ui, &V::new(v));
        assert_eq!(p.partner_name_text, "Aldis");
        assert_eq!(
            p.partner_status,
            status_state::NOT_ACCEPTED,
            "the light did not move"
        );
        assert_eq!(p.self_total_text, "ID_SecureTrade_TotalItemsLabel 1");

        // The partner's acceptance alone. **This is not the button**: `partner_accepted` is the
        // mirror's flag and `accepted` is yours, and only the light follows it.
        let mut v = base.clone();
        v.partner_accepted = true;
        p.update(&mut ui, &V::new(v));
        assert_eq!(p.partner_status, status_state::ACCEPTED);
        assert_eq!(p.button, ButtonState::Enabled, "your button did NOT accept");
        assert_eq!(p.partner_name_text, "Alba", "the name did not move");

        // Your own acceptance alone, which moves the button and leaves the light where it was.
        let mut v = base.clone();
        v.accepted = true;
        p.update(&mut ui, &V::new(v));
        assert_eq!(p.button, ButtonState::Accepted);
        assert_eq!(
            p.partner_status,
            status_state::NOT_ACCEPTED,
            "the partner has not accepted"
        );

        // The partner's list alone: only OTHER_TOTAL moves, and SELF_TOTAL does not.
        let mut v = base.clone();
        v.partner_rows = vec![
            TradeRow {
                item: b,
                name: "Pyreal".into(),
                icon: None,
            },
            TradeRow {
                item: ObjectId(3),
                name: "Pyreal".into(),
                icon: None,
            },
        ];
        p.update(&mut ui, &V::new(v));
        assert_eq!(p.other_total_text, "ID_SecureTrade_TotalItemsLabel 2");
        assert_eq!(
            p.self_total_text, "ID_SecureTrade_TotalItemsLabel 1",
            "your side is unmoved"
        );
    }

    /// A close writes every one of them back, which is the client's five statements.
    #[test]
    fn a_close_resets_the_partner_name_the_light_and_both_totals() {
        let mut ui = UiSystem::new((800, 600));
        let mut p = TradePanel::default();
        let open = V::new(TradeView {
            open: true,
            partner_name: "Alba".into(),
            self_rows: vec![TradeRow {
                item: ObjectId(1),
                name: "Key".into(),
                icon: None,
            }],
            partner_accepted: true,
            accepted: true,
            ..TradeView::default()
        })
        .with_object(ObjectId(2), "Arrow", DataId(0x0600_1040));
        p.update(&mut ui, &open);
        assert_eq!(p.partner_status, status_state::ACCEPTED);
        assert_eq!(p.self_total_text, "ID_SecureTrade_TotalItemsLabel 1");

        // **The premise for the `pending` assertion below**, and it is the whole reason this
        // block exists: a mutation that deleted `flush`'s `pending.clear()` SURVIVED an earlier
        // version of this test, because `pending` had never been non-empty in it. An assertion
        // that a collection is empty is satisfied by a collection that was never filled --
        // the stated testability rule, *"assert the premise, not only the difference"*.
        ui.requests.clear();
        assert!(p.drop_item(&mut ui.requests, ObjectId(2)));
        ui.requests.clear();
        p.update(&mut ui, &open);
        assert_eq!(
            p.pending,
            vec![ObjectId(2)],
            "an optimistic row is on the table"
        );
        assert_eq!(p.self_total_text, "ID_SecureTrade_TotalItemsLabel 2");

        p.update(&mut ui, &V::new(TradeView::default()));
        assert_eq!(p.partner_name_text, "");
        assert_eq!(p.partner_status, status_state::NOT_ACCEPTED);
        assert_eq!(p.self_total_text, "ID_SecureTrade_TotalItemsLabel 0");
        assert_eq!(p.other_total_text, "ID_SecureTrade_TotalItemsLabel 0");
        assert_eq!(p.button, ButtonState::Disabled);
        assert!(
            p.pending.is_empty(),
            "flushing the trade lists takes the optimistic rows with it"
        );

        // …and it stays gone across a reopen: a stale pending id would put a row back on a table
        // the server has emptied, which is the failure the clear exists to prevent.
        p.update(&mut ui, &open);
        assert_eq!(p.rows(false).len(), 1, "only the server's row");
        assert_eq!(p.self_total_text, "ID_SecureTrade_TotalItemsLabel 1");
    }
}
