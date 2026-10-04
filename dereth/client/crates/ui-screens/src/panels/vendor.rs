//! `VendorPanel` — the vendor window, its three tabs and its eleven buttons.
//!
//! This panel is the production consumer of `dereth-client-model`'s shop system — the buy price,
//! the sell price, the acceptability read and the four refusal strings — and of
//! `dereth_protocol::trade::{VendorInfo, VendorBuy, VendorSell}`.
//!
//! # Where the window actually lives, which is not where the layout says
//!
//! `classic_vendor` (`0x21000012`) declares `VendorPanel` as element **`0x100000B7`**, type
//! `0x10000017`. **`0x100000B7` is not in the live gameplay tree.** What *is* live, measured off
//! the shipped tree rather than assumed, is its subtree, incorporated under the environment
//! window:
//!
//! ```text
//! 0x10000495 > 0x100005FD <ENVP> > 0x10000062 > 0x100000B8 > {0x100000BC, 0x100000C4, 0x100000CD}
//! ```
//!
//! So the vendor window is a page of the floating environment panel, the `<ENVP>` window of
//! [`crate::hud::floaty::GAMEPLAY_WINDOWS`] — which is why retail groups the vendor panel with the
//! trade, salvage, housing, external-container and spellcasting panels as *environment windows*.
//! This panel therefore
//! binds off the screen **root** and finds `0x100000B8` wherever it is, rather than loading
//! `0x21000012` a second time.
//!
//! # The eleven buttons
//!
//! The vendor panel's button-click handling is one `switch` on the element id. The labels are
//! the shipped `StringTable 0x23000001` rows the layout points each button at, read back through
//! the live tree:
//!
//! | element | label | what it does |
//! |---|---|---|
//! | `0x100000B9` / `BA` / `BB` | Items / Buying / Selling | the three tabs |
//! | `0x100000C2` | **Buy** | buys the selected item alone — **sends `0x005F`** |
//! | `0x100000C3` | **Add to List** | adds it to the buy basket, at the split size for a stack of 2+, else 1 |
//! | `0x100000C9` | **Buy Item** | buys the selected basket row alone, then removes it from the basket |
//! | `0x100000CA` | **Buy All** | the two refusals, then a buy of the whole basket + flush |
//! | `0x100000CB` | **Clear Item** | removes the selected row from the buy basket |
//! | `0x100000CC` | **Clear List** | empties the buy basket |
//! | `0x100000D2` | **Sell Item** | sells the selected row alone — **sends `0x0060`** — then unmark and remove |
//! | `0x100000D3` | **Sell All** | sells the whole sell basket to the vendor, then flushes both |
//! | `0x100000D4` | **Clear Item** | clears the item's sell mark + removes it |
//! | `0x100000D5` | **Clear List** | clears every sell mark and empties the sell basket |
//! | `0x100000D6` | (close) | closes when both baskets are empty, else raises the confirmation |
//!
//! # What the corpus can and cannot witness
//!
//! **It witnesses the vendor, and of the vendor, trade, fellowship and housing windows it is the
//! only one it does.**
//! `long-solo-play` carries **8 `0x0062 Vendor_VendorInfo`**, **5 `0x005F Vendor_Buy`** and
//! **1 `0x0060 Vendor_Sell`** — two distinct vendors, real `VendorProfile`s (`buy_price` 0.9,
//! `sell_price` 1.0, `max_value` 10000) and real stock. The same scan finds **0** of trade
//! (`0x01C9`, `0x01CA`), **0** of fellowship (`0x02BE`), and for housing only
//! `0x021E House_QueryHouse` ×5 with `0x0226 House_HouseStatus` ×5 — no `HouseProfile` and no
//! `HouseData`.
//!
//! **It cannot witness the panel's own text.** The stock rows' names and icons come out of the
//! `PublicWeenieDesc` the `0x0062` carried, so those are real; the *prices* are this build's
//! arithmetic over real inputs, and no capture records what the retail window displayed. The two
//! things that are byte-compared are the requests (against the six recorded actions) and the six
//! refusal strings (against the client's own literals).
//!
//! **No live test.** Buying and selling move items and money on a live shard, so the requests
//! are asserted as bytes rather than sent.
//!
//! # The stock is decorated, and the numeral is not what it looks like
//!
//! [`ItemListWidget::decorate`] is the client's item-update tail, and it runs over the vendor's
//! lists as well as the pack's. Without it a stack of 250 arrows and a stack of one are the same
//! picture — no composite icon, no structure bar, no capacity meter, no sell marker and **no
//! tooltip**, which is where the client says how many (`"%d %s"` of the stack size and the plural
//! name).
//!
//! **The numeral over an icon is a different mechanism and a vendor-only one.** Only the vendor
//! items page writes it, and what it writes is the row's amount — the stock a vendor advertises —
//! never the stack size. It walks the stock list and stops at the first slot whose item id is the
//! selected object, so **at most one row** can carry a numeral, and an amount below 1 becomes -1,
//! the client's hide value. **29 of the corpus's 30** stock rows carry `amount == -1` (the
//! thirtieth row is a Sack with `amount == 1`), so in a
//! replay this pass draws nothing unless that one row is the selected one — hence
//! [`VendorPanel::quantity_overlays`], a counter and not a boolean.
//!
//! `decorate`'s input per slot is a [`SlotInfo`] — **four** things, not one — so
//! `VendorPanel::last_tiles` holds the three frozen-world ones per row, `cooldown_remaining` is
//! exempted, and `VendorPanel::last_selected` is the third thing the guard has to hold because
//! the numeral is a function of the selection.

use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::items::widget::{ItemListWidget, SlotInfo, TileInfo};
use crate::view::{GameView, ShopRow, ShopView, UiRequest};

/// The tile read for `id`, out of the two parallel vectors [`VendorPanel::update`] builds.
///
/// A **positional lookup**, not a mapping: `ids[i]` and `tiles[i]` are one read of the seam split
/// into a key and a value, so nothing here can be inconsistent with anything else — which is why it
/// is safe where a separately maintained id-to-tile map would not be. `None` covers both "not a
/// tile of this window" and "an id with no live client object yet", and `decorate` treats them
/// alike: that is the item update's no-object early-out, which leaves the slot as the fill left it.
/// A vendor's stock is exactly where that arm bites — the `0x0062`'s rows can name an object whose
/// `0xF745` has not landed.
fn tile_at<'a>(
    ids: &[dereth_primitives::ObjectId],
    tiles: &'a [Option<TileInfo>],
    id: dereth_primitives::ObjectId,
) -> Option<&'a TileInfo> {
    ids.iter()
        .zip(tiles)
        .find_map(|(tid, t)| (*tid == id).then_some(t.as_ref()))?
}

/// Set a text element's text through an optional handle. Returns whether there was one.
///
/// The client's four money writers each end in exactly this call and each of them reaches it
/// through an element that the vendor buy and vendor sell constructors may have left null — the
/// text is set unguarded there, so a missing element faults in retail and answers `false` here.
fn set_text(ui: &mut UiSystem, h: Option<ElemHandle>, text: &str) -> bool {
    let Some(h) = h else { return false };
    let Some(t) = ui.text_element_mut(h) else {
        return false;
    };
    t.set_text(text);
    true
}

pub use dereth_presentation::vendor::{
    item_cost_line, item_name_line, purse_line, transaction_line,
};

/// `VendorPanel`'s root in `classic_vendor` — declared, and **not instantiated** in the live
/// gameplay tree. Kept because it is the id the layout and the class table agree on and because a
/// reader who greps for it should find this note rather than nothing.
pub const VENDOR_ROOT: ElementId = ElementId(0x1000_00B7);
/// The tab element that *is* live, under `<ENVP>`. Everything below hangs off it.
pub const TABS: ElementId = ElementId(0x1000_00B8);

/// **`VendorPanel` itself** — the environment panel's vendor child, element class `0x10000017`, and
/// the fifth entry of [`crate::panels::catalogue::ENV_PANEL_PAGES`].
///
/// This is the element retail shows, and it is not [`TABS`]. The client hides all
/// five ENV pages during bring-up, and opening the vendor ends by making **this page itself**
/// visible, the same show the ground-object panel uses; closing hides it again. Showing only `TABS`
/// leaves this page and `<ENVP>` above it hidden, which is a shop that opens where nobody can see
/// it.
pub const PANEL: ElementId = ElementId(0x1000_0062);

/// The three tab buttons, in the order the `0x2E` tab array lists them: "Items", "Buying",
/// "Selling".
pub const TAB_ITEMS: ElementId = ElementId(0x1000_00B9);
/// See [`TAB_ITEMS`].
pub const TAB_BUYING: ElementId = ElementId(0x1000_00BA);
/// See [`TAB_ITEMS`].
pub const TAB_SELLING: ElementId = ElementId(0x1000_00BB);

/// The Items tab's page.
pub const PAGE_ITEMS: ElementId = ElementId(0x1000_00BC);
/// The Buying tab's page.
pub const PAGE_BUY: ElementId = ElementId(0x1000_00C4);
/// The Selling tab's page.
pub const PAGE_SELL: ElementId = ElementId(0x1000_00CD);

/// The shop's stock list. Carries `UI_ItemList_IsVendor` (`0x10000013`).
pub const STOCK_LIST: ElementId = ElementId(0x1000_00BD);
/// The item-type menu — the **type-filter strip**, a `Menu`. The vendor items panel's constructor
/// finds it by recursive lookup of `0x100000BF` and requires it to be a menu.
pub const TYPE_FILTER_MENU: ElementId = ElementId(0x1000_00BF);
/// The item-name and item-cost labels, bound with recursive child lookups for `0x100000C0` and
/// `…(0x100000C1)` off the `VendorPanel` root, like every other member of that constructor.
///
/// They are **not per-row**: there is one of each, they belong to the Items page beside the
/// list, and the client writes them from whichever row is **selected**.
pub const ITEM_NAME_TEXT: ElementId = ElementId(0x1000_00C0);
/// See [`ITEM_NAME_TEXT`].
pub const ITEM_COST_TEXT: ElementId = ElementId(0x1000_00C1);
/// The buy-list text — `L"Buying %d %s worth %hsp"`.
pub const BUY_LIST_TEXT: ElementId = ElementId(0x1000_00C7);
/// The buy-page purse text — `L"You have %hsp"`.
pub const BUY_PURSE_TEXT: ElementId = ElementId(0x1000_00C8);
/// The sell-list text — `L"Selling %d %s worth %hsp"`.
pub const SELL_LIST_TEXT: ElementId = ElementId(0x1000_00D0);
/// The sell-page purse text — `L"You have %hsp"`, the **same** number as [`BUY_PURSE_TEXT`] and a
/// different element.
pub const SELL_PURSE_TEXT: ElementId = ElementId(0x1000_00D1);
/// `UI_Vendor_ShopFilters` — the property the type-filter fill writes the `ITEM_TYPE` mask into on
/// each menu row it makes.
///
/// **It is an `Integer`, not an `Enum`**: `property_types.rs`'s row for `0x10000039`, taken from
/// the retail dat, is `T::Integer`. `UiSystem::set_attribute_enum`'s own doc records what the
/// wrong one costs — a value written through the other setter matches no arm and reads as absent.
pub const ATTR_SHOP_FILTER: u32 = 0x1000_0039;
/// The buy basket's list — the **one** `ItemListWidget` in the whole shipped tree that
/// carries `UI_ItemList_SingleSelection` (`0x10000052`); it is 1 of 65 item lists.
pub const BUY_LIST: ElementId = ElementId(0x1000_00C5);
/// The sell basket's list.
pub const SELL_LIST: ElementId = ElementId(0x1000_00CE);

/// The vendor panel's item list begin drag notice's partial-stack refusal.
pub const CANNOT_SPLIT_FROM_PANEL: &str = "You cannot split items from this panel";

/// "Buy" — buys the selected item alone.
pub const BTN_BUY: ElementId = ElementId(0x1000_00C2);
/// "Add to List" — adds the selected item to the buy basket.
pub const BTN_ADD_TO_LIST: ElementId = ElementId(0x1000_00C3);
/// "Buy Item".
pub const BTN_BUY_ITEM: ElementId = ElementId(0x1000_00C9);
/// "Buy All".
pub const BTN_BUY_ALL: ElementId = ElementId(0x1000_00CA);
/// "Clear Item", on the buy page.
pub const BTN_BUY_CLEAR_ITEM: ElementId = ElementId(0x1000_00CB);
/// "Clear List", on the buy page.
pub const BTN_BUY_CLEAR_LIST: ElementId = ElementId(0x1000_00CC);
/// "Sell Item".
pub const BTN_SELL_ITEM: ElementId = ElementId(0x1000_00D2);
/// "Sell All".
pub const BTN_SELL_ALL: ElementId = ElementId(0x1000_00D3);
/// "Clear Item", on the sell page.
pub const BTN_SELL_CLEAR_ITEM: ElementId = ElementId(0x1000_00D4);
/// "Clear List", on the sell page.
pub const BTN_SELL_CLEAR_LIST: ElementId = ElementId(0x1000_00D5);
/// The close button — `0x100000D6`, the only button that can raise a dialog.
pub const BTN_CLOSE: ElementId = ElementId(0x1000_00D6);

/// Which tab is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    /// The Items tab — the tab array's default (`Bool(true)` on the first row).
    #[default]
    Items,
    Buying,
    Selling,
}

/// `VendorPanel`, bound to a live tree.
#[derive(Debug, Default)]
pub struct VendorPanel {
    /// [`PANEL`] — the vendor panel's own element, shown at the client-side open edge; see
    /// [`PANEL`].
    pub root: Option<ElemHandle>,
    /// `0x100000B8`, the element every other binding is found under.
    pub tabs: Option<ElemHandle>,
    pub items_page: Option<ElemHandle>,
    pub buy_page: Option<ElemHandle>,
    pub sell_page: Option<ElemHandle>,
    /// The stock list.
    pub stock: Option<ItemListWidget>,
    /// The buy basket.
    pub buy: Option<ItemListWidget>,
    /// The sell basket.
    pub sell: Option<ItemListWidget>,
    /// **The four money elements, one per writer.** The buy-list text, the buy purse,
    /// the sell-list text and the sell purse, in that order. Separate handles rather than a pair,
    /// because the four writers are four functions and a station that could only see "some text
    /// changed" could not tell them apart.
    pub buy_list_text: Option<ElemHandle>,
    /// See [`Self::buy_list_text`].
    pub buy_purse_text: Option<ElemHandle>,
    /// See [`Self::buy_list_text`].
    pub sell_list_text: Option<ElemHandle>,
    /// See [`Self::buy_list_text`].
    pub sell_purse_text: Option<ElemHandle>,
    /// The item-name text (`0x100000C0`) and the item-cost text (`0x100000C1`) — the two elements
    /// the items-page update writes.
    pub item_name_text: Option<ElemHandle>,
    /// See [`Self::item_name_text`].
    pub item_cost_text: Option<ElemHandle>,
    /// The Buy button (`0x100000C2`) and the Add button (`0x100000C3`) — the same function's other
    /// two writes, state 1 with a selection and state `0x0D` without one. Bound as *elements*, not
    /// as the [`BTN_BUY`] / [`BTN_ADD_TO_LIST`] click ids, because this is the one place the panel
    /// writes to them rather than reads from them.
    pub buy_button: Option<ElemHandle>,
    /// See [`Self::buy_button`].
    pub add_button: Option<ElemHandle>,
    /// How many of the **four** writes the items-page update makes ran on the last rebuild, on the
    /// same terms as [`Self::money_writes`]: `4` whenever all four elements are bound.
    pub item_writes: usize,
    /// The item-type menu — the tab strip's `Menu` (`0x100000BF`).
    pub type_menu: Option<ElemHandle>,
    /// The type-filter count — how many rows the last [`Self::open_vendor_type_filters`] put in
    /// that menu.
    ///
    /// **Three states, not two.** A shop with stock and `0` here is the defect this row was filed
    /// for; a shop with *no* stock and `0` is correct; a menu the tree does not carry is
    /// [`Self::type_menu`] being `None`. The three must not read alike.
    pub num_type_filters: usize,
    /// How many of the **four** money writers ran on the last rebuild. `4` whenever all four
    /// elements are bound; anything less names a missing binding rather than a missing number.
    pub money_writes: usize,
    /// Which tab is up.
    pub tab: Tab,
    /// The snapshot the tree was last built for — the rebuild guard.
    last: Option<ShopView>,
    /// **The other half of that guard**, one entry per tile the three lists hold, in
    /// the order [`Self::update`] builds them: stock, then the buy basket, then the sell basket.
    ///
    /// [`ItemListWidget::decorate`]'s input per slot is a [`SlotInfo`], and a `SlotInfo` is
    /// **four** things: the whole [`crate::view::SlotDecoration`], the name, the plural name and
    /// `cooldown_remaining`. [`ShopView`] carries none of them — a [`ShopRow`] is an id, a name, an
    /// icon id, an amount, a price and a refusal — so a guard that compared only the snapshot would
    /// let a stack of arrows in a vendor's stock keep a stale quantity overlay for as long as the
    /// window stayed open. The first three are facts about a frozen world and live here; the fourth
    /// is a function of the clock and is exempted — see the note above the `info` closure in
    /// [`Self::update`].
    ///
    /// Held on the panel rather than inside [`ShopView`] for the reason
    /// [`crate::panels::inventory::InventoryPanels`] holds its own: a `TileInfo` owns two
    /// `String`s, and [`TileInfo::matches`] asks the view field by field, so a frame on which
    /// nothing moved allocates nothing at all.
    last_tiles: Vec<Option<TileInfo>>,
    /// **The selected object, the third thing this panel's rebuild reads and the third thing its
    /// guard therefore has to hold.**
    ///
    /// The quantity pass returns immediately when nothing is selected and otherwise writes the
    /// numeral on **exactly one** row: the stock slot whose item id equals it. So the quantity a
    /// player sees is a function of the selection, and a selection that moved with the stock and
    /// the decorations unchanged would leave the numeral on the row it was on. It is not in
    /// [`ShopView`] — the panel already takes it as an argument at [`Self::handle_button_click`] —
    /// so it is remembered here.
    last_selected: Option<dereth_primitives::ObjectId>,
    /// How many times [`Self::update`] rebuilt the panel. **Three states, not two**: this
    /// separates "rebuilt and the shop was closed" from "never ran", which is the difference
    /// between a shop with no stock and an unwired panel.
    pub rebuilds: u32,
    /// What the last rebuild put in each of the three lists, so a test can read back what a player
    /// would see without re-walking the tree.
    ///
    /// **Entry 0 is the FILTERED stock, not the whole of [`ShopView::stock`].**
    /// The vendor items panel's items-list update is the only thing in the client that ever inserts
    /// into the stock list (it is run from the component-list fill and from the vendor panel's
    /// element-message handler), and it inserts only the rows the current tab's mask keeps.
    rows: [Vec<ShopRow>; 3],
    /// The `ITEM_TYPE` mask the last [`Self::update_items_list`] filtered on.
    ///
    /// **Three states, not two**, and this is the field that separates them: `Some(m)` is a tab
    /// whose mask was read off the menu's selected row, `Some(0)` is retail's *no selected row*
    /// (the menu had no selected item, so the mask stayed 0 and **nothing** was inserted — an empty
    /// list, not an unfiltered one), and `None` is a pass that never ran.
    pub last_mask: Option<u32>,
    /// How many times an element message drove
    /// [`Self::update_items_list`], i.e. how often the *player* re-filtered rather than the
    /// rebuild. Separates "the arm exists and fired" from "the list happens to look right".
    pub filter_applications: u32,
    /// How many times the items-list update's tail found the stock list non-empty
    /// and scrolled it to show row 0.
    ///
    /// The count is diagnostic; acceptance checks the inherited offset and actual slot geometry.
    pub scroll_restores: u32,
    /// The type-filter open saves the list's horizontal scroll around the filter-menu callback. The
    /// screen's callback is queued, so retain this until that MENU_CHOSEN has been delivered.
    pending_open_scroll_x: Option<(i32, u32)>,
    /// How many stock rows the buy basket's subtraction dropped:
    ///
    /// The row does not appear greyed, zeroed or struck through — it **disappears**, and a
    /// player who adds a vendor's whole stock of tapers to the basket stops seeing tapers. The
    /// counter exists because "the row is absent" is also what a broken mask, an empty stock and a
    /// panel that never ran look like; three states, not two.
    pub basket_drops: u32,
    /// How many stock rows the containment gate dropped:
    /// either the contained-item count or the contained-container count is nonzero.
    ///
    /// **No recorded vendor sells a container**, so this counter is expected to read 0 against the
    /// corpus and the arm is reached only by a constructed station. Saying that with a number is
    /// the difference between *the gate did not fire* and *the gate is not there*.
    pub container_drops: u32,
    /// How many stack-size writes the last pass made, which is the rows whose max
    /// stack size is greater than 1.
    ///
    /// Counted as well as emitted because the two arms reach the same call from different
    /// arithmetic — the max stack size unclamped on the unlimited arm, `min(remaining, max stack
    /// size)` on the finite one — and because a row dropped by the containment gate below still has
    /// its stack size written, which no reading of the list can show.
    pub stack_size_writes: u32,
    /// Whether the last rebuild left the window visible.
    pub visible: bool,
    /// How many slots the last rebuild ran the client's tail over. The denominator for "the
    /// vendor's stock draws its quantities". **Three states, not two**: a shop with stock and
    /// `slots_decorated == 0` is a decoration failure, and it must not read the same as a vendor
    /// with an empty stock list.
    pub slots_decorated: usize,
    /// How many stock slots the last rebuild wrote a quantity numeral onto — the
    /// client's reach, which is **at most one**: it breaks out of its walk on the first slot whose
    /// item id is the selected object.
    ///
    /// Zero is the honest answer for "nothing is selected" *and* for "the selected row is not in
    /// the stock list", and the two are the same in the client. It is a counter rather than a
    /// boolean so that a station can tell "the pass ran and wrote nothing" from "the pass never
    /// ran".
    pub quantity_overlays: usize,
    /// A successful source drag whose partial split must reset the toolbar's shared split pair.
    /// The panel lives behind the HUD seam, so the host drains this after delivering the
    /// rejected-drag message and applies it to `GamePlayScreen`'s visible copy.
    pending_split_reset: bool,
}

impl VendorPanel {
    /// The panel's post-init, minus the notice registrations.
    ///
    /// Bound off the screen **root**: `0x100000B8` sits three levels inside `<ENVP>` and every id
    /// below it is unique in the shipped tree.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        let Some(tabs) = ui.get_child_recursive(root, TABS) else {
            *self = Self::default();
            return;
        };
        self.tabs = Some(tabs);
        // `VendorPanel` is the ENV page, not the tab control: bind it so the open
        // edge can show the element retail's vendor open actually shows.
        self.root = ui.get_child_recursive(root, PANEL);
        self.items_page = ui.get_child_recursive(tabs, PAGE_ITEMS);
        self.buy_page = ui.get_child_recursive(tabs, PAGE_BUY);
        self.sell_page = ui.get_child_recursive(tabs, PAGE_SELL);
        let list = |ui: &mut UiSystem, id| {
            ui.get_child_recursive(tabs, id)
                .map(|h| ItemListWidget::init(ui, h))
        };
        self.stock = list(ui, STOCK_LIST);
        self.buy = list(ui, BUY_LIST);
        self.sell = list(ui, SELL_LIST);
        // The two sub-UI constructors' own recursive child lookups, off the
        // *parent* exactly as they are: the vendor buy page's constructor takes `0x100000C7` and
        // `0x100000C8`, the vendor sell page's constructor takes `0x100000D0` and `0x100000D1`, and
        // the vendor items page's constructor takes the menu.
        self.buy_list_text = ui.get_child_recursive(tabs, BUY_LIST_TEXT);
        self.buy_purse_text = ui.get_child_recursive(tabs, BUY_PURSE_TEXT);
        self.sell_list_text = ui.get_child_recursive(tabs, SELL_LIST_TEXT);
        self.sell_purse_text = ui.get_child_recursive(tabs, SELL_PURSE_TEXT);
        // The client's other four children, in its own order: the two texts and then the two
        // buttons.
        self.item_name_text = ui.get_child_recursive(tabs, ITEM_NAME_TEXT);
        self.item_cost_text = ui.get_child_recursive(tabs, ITEM_COST_TEXT);
        self.buy_button = ui.get_child_recursive(tabs, BTN_BUY);
        self.add_button = ui.get_child_recursive(tabs, BTN_ADD_TO_LIST);
        self.item_writes = 0;
        self.type_menu = ui.get_child_recursive(tabs, TYPE_FILTER_MENU);
        self.num_type_filters = 0;
        self.money_writes = 0;
        self.tab = Tab::Items;
        self.last = None;
        self.last_tiles.clear();
        self.last_selected = None;
        self.rows = [Vec::new(), Vec::new(), Vec::new()];
        self.visible = false;
        self.slots_decorated = 0;
        self.quantity_overlays = 0;
        self.pending_split_reset = false;
        self.last_mask = None;
        self.filter_applications = 0;
        self.scroll_restores = 0;
        self.pending_open_scroll_x = None;
        self.basket_drops = 0;
        self.container_drops = 0;
        self.stack_size_writes = 0;
    }

    /// True once the stock list was found — the binding without which nothing can be drawn.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.stock.is_some()
    }

    /// The client's rejected-drag arm followed by the vendor panel's item-list begin-drag notice.
    ///
    /// The notice handler accepts only the sell basket. It clears the object's sell state and
    /// removes that profile first. If the split size is below the max split size, it then displays
    /// [`CANNOT_SPLIT_FROM_PANEL`], assigns the maximum to the current split, and asks the toolbar
    /// to refresh. No sale or other network request is made.
    pub fn begin_item_drag(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        split: u32,
        max: u32,
    ) -> Option<crate::items::widget::DragStart> {
        if m.id != dereth_ui::msg::element::id::DRAG_REJECTED {
            return None;
        }
        let started = {
            let list = self.sell.as_mut()?;
            crate::items::widget::begin_drag_from_rejected(
                ui,
                &mut [list],
                m.source,
                m.point.window.0,
                m.point.window.1,
            )
        };
        let item = started.as_ref().and_then(|drag| drag.item)?;
        ui.requests.emit(UiRequest::VendorClearList {
            sell: true,
            item: Some(item),
        });
        if split < max {
            ui.requests.emit(UiRequest::DisplayChatText {
                channel: 0x1A,
                text: CANNOT_SPLIT_FROM_PANEL.to_owned(),
            });
            self.pending_split_reset = true;
        }
        started
    }

    /// Take the toolbar refresh owed by a partial sell-row source drag.
    pub fn take_split_reset(&mut self) -> bool {
        std::mem::take(&mut self.pending_split_reset)
    }

    /// What the last rebuild wrote into the stock, buy and sell lists.
    #[must_use]
    pub fn rows(&self, which: Tab) -> &[ShopRow] {
        &self.rows[which as usize]
    }

    /// The panel's update, guarded on the snapshot.
    ///
    /// A shop that closes is **not** an early return: the three lists are flushed and the window
    /// hidden, because a player who walks away from a vendor must stop seeing its stock.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let s = view.shop();
        // **The ids the three lists will hold, in the order they will hold them.**
        //
        // Computed once and used three times: for the guard's memory, for the `info` closure the
        // decorate pass reads, and (positionally) to answer which tile belongs to which id. A
        // closed shop draws nothing, so it has no tiles.
        let tile_ids: Vec<dereth_primitives::ObjectId> = if s.open {
            s.stock
                .iter()
                .chain(s.buy_list.iter())
                .chain(s.sell_list.iter())
                .map(|r| r.item)
                .collect()
        } else {
            Vec::new()
        };
        // Asked without allocating: on a still frame this walks the ids and builds no `String`.
        //
        // The length term is kept for its reason: `zip` stops at the shorter side, so a
        // `last_tiles` shorter than `tile_ids` would make `any` answer *false* and the gate could
        // close over a fill that never happened. Every id in `tile_ids` comes out of `s`, which is
        // compared as a whole one line down, so the term is unfalsifiable on this host today —
        // containment through another field rather than by construction, which is why it stays.
        let tiles_changed = self.last_tiles.len() != tile_ids.len()
            || tile_ids
                .iter()
                .zip(&self.last_tiles)
                .any(|(id, last)| !TileInfo::matches(view, *id, last.as_ref()));
        // See [`Self::last_selected`]. Read here, before the early-out, so the guard
        // is a superset of what the quantity pass below reads.
        let selected = view.selected_object();
        if self.last.as_ref() == Some(&s) && !tiles_changed && self.last_selected == selected {
            return false;
        }
        // One read of the seam per tile, used by the `info` closure and stored as the next pass's
        // memory — so the value the guard compared and the value the screen draws are one read.
        let tiles: Vec<Option<TileInfo>> = tile_ids
            .iter()
            .map(|id| TileInfo::read(view, *id))
            .collect();
        // **The stock list is filled LAST, by the items-list update, because its contents are a
        // function of the filter strip that has not been rebuilt yet.**
        //
        // In the client the order is not a choice: the vendor open flushes the stock list, rebuilds
        // the menu, and ends by selecting a menu row with **broadcast on** — and that broadcast is
        // message `7`, which turns into the items-list update with mask 0 and select-first on. So
        // retail's stock list is filled *only* as a consequence of the strip choosing a row, and
        // the vendor open itself leaves it empty. That is why the two calls below are in this order
        // and why [`Self::open_vendor_type_filters`] does not fill the list itself. **The two
        // edges, computed once.** `self.last` is still the *previous* snapshot here; it is written
        // at the end of this function.
        //
        // **`opening` is this build's stand-in for the open-vendor notice, and it is keyed on the
        // strip's own inputs.** It is deliberately not *closed-to-open*: a player walking from one
        // grocer to the next overwrites the vendor id without it ever passing through 0. And it is
        // deliberately not *the vendor id alone*: a `0x0062` from the **same** vendor — a stock
        // refresh after a purchase — is a full vendor open in the client.
        //
        // So the key is the set of fields the two vendor-open bodies below actually read: `open`,
        // `vendor` and `type_filters` for [`Self::open_vendor_type_filters`], and `stock` for
        // [`Self::update_items_list`]. **What it is a superset of is therefore "a `0x0062`
        // arrived"** — every one of them changes at least one of those, and nothing else in
        // `ShopView` can: the money, the two baskets, the tiles and `last_selected` all sit
        // outside it. That sentence is the thing that can go stale when `ShopView` grows, so it is
        // written here rather than left implied.
        //
        // **Residual gap:** a `0x0062` that re-sends a **byte-identical** stock and strip is a
        // no-op here and would still move retail's selection back to row 0.
        let reopened = self.last.as_ref().is_none_or(|l| {
            l.vendor != s.vendor || l.type_filters != s.type_filters || l.stock != s.stock
        });
        let opening = s.open && reopened;
        let closing = !s.open && self.last.as_ref().is_some_and(|l| l.open);
        // A selection/decoration-only repaint is not an items-list update in retail, and it never
        // scrolls. Preserve the existing viewport when this host's broader repaint pass reprojects
        // otherwise-identical shop state.
        let repaint_scroll = (self.last.as_ref() == Some(&s))
            .then(|| self.stock.as_ref().map(|w| w.scroll(ui)))
            .flatten();
        self.fill(ui, Tab::Buying, &s.buy_list);
        self.fill(ui, Tab::Selling, &s.sell_list);
        // The client's tab strip, rebuilt only on the vendor-open edge and never on an ordinary
        // repaint.
        //
        // Building the strip is not just drawing it. The function's last statement selects a menu
        // row with broadcast on, and that broadcast is message `7` — which `GamePlayScreen`
        // forwards as `MENU_CHOSEN` to [`Self::on_element_message`]'s message-7 arm, running the
        // items-list update with mask 0 and select-first on, whose tail writes the selected
        // object. So rebuilding the strip on every repaint would raise a **selection write**
        // through the real message pump, even with no vendor open.
        //
        // In the client the whole function runs only inside the vendor open, under the vendor
        // panel's open, under the open-vendor notice — a `0x0062`. So it runs here on the same
        // edge, plus the close, which has to empty the strip a player walked away from; the close
        // passes `broadcast = false` for the reason at that line.
        if opening || closing {
            self.open_vendor_type_filters(ui, &s);
        }
        // …and then the consequence of its broadcast selection: the items-list update with mask 0,
        // which is what makes the mask come off the strip rather than out of a literal.
        //
        // **Select-first is the vendor-open edge, and it is NOT `true` on every rebuild.** The
        // items-list update's tail selects the first kept row when select-first is
        // set, so a `true` here **writes the player's selection**, and writes `0`, the client's own
        // clear, whenever the mask kept nothing. In the client select-first is `true` at exactly
        // **one** of the update's three call sites: the message-7 arm of the vendor panel's
        // element-message handler. The other two pass `false` — the component-list fill (mask
        // `0x1000`) and the `0x2C` arm (mask 0).
        //
        // **A repaint cannot reach that one site.** The only broadcasts of message `7` from
        // `0x100000BF` are a player's click on the filter strip and the trailing broadcast
        // selection that closes the vendor items page's open-vendor — and that runs only under the
        // vendor panel's own open-vendor, whose one caller is its open-vendor notice, i.e. a
        // `0x0062`.
        //
        // This build's rebuild gate is far wider than a `0x0062`: it opens on the tiles, on the
        // baskets, and — through `last_selected` — on `view.selected_object()` itself. So a
        // literal `true` here would make **every selection change clear the selection**, on a
        // screen with no vendor open at all (`s.open == false`, mask `0`, nothing kept, hence a
        // selection write of 0). The flag is therefore the open
        // edge, which is this build's stand-in for the open-vendor notice; the player's own
        // re-filter keeps its `true` in [`Self::on_element_message`], which is where retail puts
        // it. `self.last` is still the *previous* snapshot here — it is written at the end of this
        // function — so this reads the previous snapshot, as the tab choice below does.
        //
        // **The edge is a change of VENDOR, not merely closed-to-open.** The open-vendor notice
        // fires per `0x0062`, and a player who walks from one grocer to the next can produce a view
        // whose `open` never went false in between — the vendor id is simply overwritten.
        // `ShopView::vendor` is that id, so comparing it catches both the first open and the
        // switch, and `!l.open && s.open` would have missed the second.
        //
        // **Declared gap:** a *second* `0x0062` from the **same** vendor — a stock refresh after a
        // purchase — is a real vendor open in the client and is invisible here, because nothing in
        // `ShopView` distinguishes it from any other change to the stock. Retail would move the
        // selection to the first row again; this build will not.
        if !opening
            && self
                .last
                .as_ref()
                .is_some_and(|last| last.filter != s.filter)
        {
            if let Some(menu) = self.type_menu {
                let row = dereth_ui::widgets::menu::get_item(ui, menu, s.filter_index());
                dereth_ui::widgets::menu::set_selected_item(ui, menu, row, false);
            }
        }
        self.update_items_list(ui, &s, 0, opening);
        if let Some(w) = self.stock.as_ref() {
            if let Some((x, _)) = self.pending_open_scroll_x.filter(|_| opening) {
                // The vendor open's trailing horizontal scroll, after the synchronous projection
                // here.
                dereth_ui::widgets::listbox::set_scroll_offset(ui, w.handle, x, w.scroll(ui).1);
            } else if let Some((x, y)) = repaint_scroll {
                dereth_ui::widgets::listbox::set_scroll_offset(ui, w.handle, x, y);
            }
        }
        // Quantity decoration has exactly one caller, here, and is not part of the
        // item update. Thus the numeral over an icon is a vendor-only decoration, written from
        // amount — the stock the vendor advertises — and never from the stack size. A build that
        // painted the stack size there would look wrong against retail rather than right, which is
        // why `decorate` below leaves the slot's quantity alone and only re-renders it.
        //
        // Nothing happens with no selection. Otherwise the client walks the stock list for the slot
        // whose item is the selected object, returns if none matches, looks the object up in the
        // vendor's profile list for its amount, turns an amount below 1 into -1, and sets that as
        // the slot's quantity before re-running the item update.
        //
        // **One row, not all of them**: the walk stops on the first slot that matches, so only the
        // *selected* stock row can carry a numeral. And `amount < 1 -> -1` is the quantity
        // display's hide value, so a vendor with unlimited stock shows nothing — which is 29 of the
        // corpus's 30 stock rows (the thirtieth is `0x80000997` "Sack" with `amount == 1`). This
        // pass is therefore expected to draw a
        // numeral almost never, and `quantity_overlays` exists so a station can prove it ran
        // anyway.
        //
        // The flush inside `set_contents` has already put every slot's quantity back to `-1`, so
        // the non-selected rows need no write.
        self.quantity_overlays = 0;
        if let (Some(sel), Some(w)) = (selected, self.stock.as_mut()) {
            if let Some(slot) = w.slots.iter_mut().find(|slot| slot.item == Some(sel)) {
                let amount = s
                    .stock
                    .iter()
                    .find(|r| r.item == sel)
                    .map_or(0, |r| r.amount);
                slot.set_quantity(if amount < 1 { -1 } else { amount });
                self.quantity_overlays += 1;
            }
        }
        // **The item-update tail, on all three lists.**
        //
        // The client runs it per row as its list fills, so a stack in a vendor's stock draws its
        // quantity overlay, its structure bar, its capacity meter, its sell marker and its tooltip.
        // `set_contents` alone would leave a stack of 100 arrows and a stack of 1 the same picture,
        // with no tooltip.
        //
        // **`cooldown_remaining` is the one input above that the guard CANNOT hold**, and this is
        // why. Every field of a [`TileInfo`] is a fact about a frozen world: it changes when a
        // datagram lands and is otherwise constant, so *"the compared value did not move"* and
        // *"nothing happened"* are the same statement. A cooldown is `(duration + start time) -
        // now`, a function of the **clock**: it takes a new value every frame of its own accord,
        // with no datagram behind it. Folding it into `last_tiles` would make this early-out fire
        // **never** — three lists flushed, refilled and re-decorated on every frame for as long as
        // anything in the shop was on cooldown — and it would *look correct on screen*, which is
        // what makes it worse than the staleness it would be fixing. So it stays out, and the read
        // below is not the countdown mechanism: it is the fresh value the item update's own tail
        // would have used on a frame that really did rebuild.
        //
        // **The heartbeat that ticks it does not reach these three lists**, because
        // `GamePlayScreen::do_item_heartbeat` walks the inventory's lists and the eighteen quickbar
        // tiles only. That is a declared gap, not a claim that it is right — in the client every
        // item widget registers for global message 3 in its post-init, these three lists included.
        // The file that owns the walk is `screens/gameplay.rs`.
        let now = ui.now.0;
        let info = |id: dereth_primitives::ObjectId| -> Option<SlotInfo> {
            Some(tile_at(&tile_ids, &tiles, id)?.to_slot_info(view, now))
        };
        let mut decorated = 0;
        for w in [self.stock.as_mut(), self.buy.as_mut(), self.sell.as_mut()]
            .into_iter()
            .flatten()
        {
            decorated += w.decorate(ui, &info);
        }
        self.slots_decorated = decorated;
        // **The four money writers, called separately and in the client's own order.** The vendor
        // buy panel's update adopts the basket as its contents, updates the buy UI, then the
        // transaction value, then the total value, and the vendor sell page's update is the same
        // four with its own three functions. **Then the total-value update's own first callee, in
        // its own order.**
        //
        // It reads integer quality `0x14` into the purse total, updates the selected stock item's
        // name and cost, then updates the buy purse and the sell purse, in that order.
        //
        // It reads the purse total for the `(you have %hsp)` half of the cost line, so it has to
        // come after the shop's purse is settled and it may not come after the two purse writers
        // for a reason of its own — this is simply where the client puts it.
        self.item_writes = self.update_items_ui(ui, &s, selected, &info, view.split_size());
        self.money_writes = 0;
        self.money_writes += usize::from(self.update_buy_transaction_value(ui, &s));
        self.money_writes += usize::from(self.update_buy_total_value(ui, &s));
        self.money_writes += usize::from(self.update_sell_transaction_value(ui, &s));
        self.money_writes += usize::from(self.update_sell_total_value(ui, &s));
        // The open-vendor notice's `mode` picks the tab: the sell shop mode is the drag-onto-the-
        // vendor route, and it opens on Selling.
        let was_open = self.last.as_ref().is_some_and(|l| l.open);
        if !was_open && s.open {
            self.tab = if s.sell_mode {
                Tab::Selling
            } else {
                Tab::Items
            };
        }
        // **The window is toggled on the EDGE, not re-asserted on every pass.**
        //
        // Nothing in the update's three sub-UIs changes visibility. The window goes up in exactly
        // one place, the client's open-vendor notice — a `0x0062` off the wire — and it comes down
        // in the close-button handler and the object-range exit handler.
        //
        // Re-asserting it every time the snapshot moved would not be a harmless idempotent write,
        // because **`VendorPanel` shares `<ENVP>` with the corpse/chest window**: `0x10000062` and
        // `0x1000005D` are both entries of the environment panel stack's child table (the
        // environment panel's child set-up), and that stack shows **one page at a time** -- its
        // set-panel-visibility notice hides the page it covers. Opening a corpse correctly hides
        // the shop, and the very next thing that moves this panel's snapshot -- a **selection**,
        // which `last_selected` is deliberately part of -- would put the shop back up and hide the
        // corpse under it: the click that selects a loot row would send the next click to the
        // same slot id (`0x1000033A`) in the shop's stock list `0x100000BD` instead of in the loot
        // list under `0x1000006A`.
        if self.last.is_none() || was_open != s.open {
            self.set_visible(ui, s.open);
        }
        self.last_tiles = tiles;
        self.last_selected = selected;
        self.last = Some(s);
        self.rebuilds += 1;
        true
    }

    /// The client's tail, run again over whatever the three lists are holding
    /// **now**. Without it, tabbing off Items and back draws the stock with no background and
    /// white borders until an item is selected.
    ///
    /// **In the client there is no separate decorate pass to forget.** The vendor items panel's
    /// items-list update is a flush of the stock list plus one item-list insert per survivor, and
    /// every insert ends in the item-list add -> item init -> item update, which is where the
    /// weenie object's icon composite is put on the slot's icon element. So retail decorates on
    /// **every** call of the items-list update — the `0x2C` tab arm and the `7` filter arm as much
    /// as the open.
    ///
    /// On this host the fill and the decoration are two passes ([`Self::fill`]'s `set_contents`,
    /// which writes the row's raw icon id, and [`crate::items::widget::ItemListWidget::decorate`]),
    /// and [`Self::update`]'s guard — *"the snapshot, the tiles and the selection are all
    /// unmoved"* — is a statement about the **world**. A tab change moves none of them, so without
    /// this pass the freshly filled rows would keep their bare pictures until something the guard
    /// does watch moved (a selection, which is why clicking a row would put it right).
    ///
    /// Reads the ids off [`Self::rows`], which [`Self::fill`] has just written and which is
    /// positionally what the widgets hold. Returns how many slots were decorated.
    fn redecorate(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> usize {
        let ids: Vec<dereth_primitives::ObjectId> =
            self.rows.iter().flatten().map(|r| r.item).collect();
        let tiles: Vec<Option<TileInfo>> = ids.iter().map(|id| TileInfo::read(view, *id)).collect();
        let now = ui.now.0;
        let info = |id: dereth_primitives::ObjectId| -> Option<SlotInfo> {
            Some(tile_at(&ids, &tiles, id)?.to_slot_info(view, now))
        };
        let mut decorated = 0;
        for w in [self.stock.as_mut(), self.buy.as_mut(), self.sell.as_mut()]
            .into_iter()
            .flatten()
        {
            decorated += w.decorate(ui, &info);
        }
        self.slots_decorated = decorated;
        decorated
    }

    /// The update's fill for one of the three lists.
    fn fill(&mut self, ui: &mut UiSystem, which: Tab, rows: &[ShopRow]) {
        let w = match which {
            Tab::Items => self.stock.as_mut(),
            Tab::Buying => self.buy.as_mut(),
            Tab::Selling => self.sell.as_mut(),
        };
        let Some(w) = w else {
            self.rows[which as usize] = rows.to_vec();
            return;
        };
        let ids: Vec<dereth_primitives::ObjectId> = rows.iter().map(|r| r.item).collect();
        let icons: std::collections::BTreeMap<
            dereth_primitives::ObjectId,
            Option<dereth_primitives::DataId>,
        > = rows.iter().map(|r| (r.item, r.icon)).collect();
        // A vendor's list is unbounded: `UI_ItemList_FixedListSize` is `-1` on all three
        // (`0x0000005F` = `Integer(-1)` in the shipped layout), which is the empty-slot update's
        // unbounded arm.
        w.set_contents(ui, None, Some(-1), &ids, &|id| {
            icons.get(&id).copied().flatten()
        });
        self.rows[which as usize] = rows.to_vec();
    }

    /// The type-filter menu's selected row and the `0x10000039` on it — the items-list update's own
    /// mask-0 arm.
    ///
    /// The mask defaults to 0 (and that default bites); a non-zero argument is used as is;
    /// otherwise the menu's selected row is asked, and with no selected row the mask stays 0, else
    /// it is the row's `UI_Vendor_ShopFilters` value. This is one of only two places in the client
    /// that asks a menu what is chosen.
    ///
    /// Answers `None` for "there is no menu", which is not the same as `Some(0)`: the client cannot
    /// tell those apart because a missing menu has no selected row either, but a station has to,
    /// and [`Self::type_menu`] is the field that says which.
    #[must_use]
    pub fn selected_type_filter(&self, ui: &UiSystem) -> Option<u32> {
        let menu = self.type_menu?;
        // The menu's selected-item read is its list box's selected item; the index and the item are
        // the same read of the same list, and a selected index of -1 is "no item".
        let idx = dereth_ui::widgets::menu::selected_index(ui, menu);
        let row = usize::try_from(idx)
            .ok()
            .and_then(|i| dereth_ui::widgets::menu::get_item(ui, menu, i))?;
        // `Integer`, per the retail property schema — see [`ATTR_SHOP_FILTER`]. A row written
        // through the enum setter would answer `None` here and read as an unfiltered tab.
        #[allow(clippy::cast_sign_loss)]
        Some(
            ui.node(row)?
                .merged_properties()
                .get_int(ATTR_SHOP_FILTER)? as u32,
        )
    }

    /// The items-list update, `(mask, select_first)` — **the consumption of the filter strip.**
    ///
    /// Retail flushes the stock list unconditionally, then (unless no vendor is open) takes the
    /// mask from the argument or, when that is 0, from [`Self::selected_type_filter`], and walks
    /// the vendor's profile list. Each profile is skipped when it has no live object or when `mask
    /// & type` is 0; the first row past the mask is remembered as `first`; then come the stack-size
    /// write and the buy-basket subtraction, the examination refresh for the selected object, the
    /// containment gate, the sell mark (sell list only) and the insert. The tail selects `first`
    /// when `select_first` is set, and scrolls to row 0 when the list has rows.
    ///
    /// The mask test is a bit test, and not a comparison against the tab's index: a row survives
    /// when any bit of the chosen tab's mask is set in its `ITEM_TYPE`, which is what makes the
    /// five compound masks in `dereth_client_model::vendor::TYPE_FILTERS` mean anything.
    ///
    /// **`mask == 0` empties the list; it does not disable the filter.** `0 & type` is always 0, so
    /// every row is skipped and nothing is inserted. That is reachable in retail — the menu has no
    /// selected row whenever the strip is empty, which is exactly a shop none of the eighteen
    /// list-contains-type gates fired for — and it is why [`Self::last_mask`] has to distinguish
    /// `Some(0)` from `None`.
    ///
    /// **What is here and what is not.** **Four** gates decide membership and all four are
    /// here. The weenie test is the seam's own construction (a [`ShopRow`] exists only for a
    /// profile the `0x0062` carried a `PublicWeenieDesc` for, which is precisely the set for which
    /// the world constructs objects; see `dereth_client::vendor_view`'s `inq_type`). The mask test is
    /// the bit test above. The other two are:
    ///
    /// * the **buy-basket subtraction**, which drops a
    ///   finite stock row the basket already holds the whole of. That is corpus-reachable: the
    ///   eighth recorded `0x0062` sells `0x80000997` "Sack" with `amount == 1`.
    /// * the **containment gate**, which needed two new [`ShopRow`] fields, because an object's
    ///   inventory is not part of a `PublicWeenieDesc` and the answer exists only in the world.
    ///
    /// The client's stack-size write is decided here and applied by the host, because it writes a
    /// **desc** and not a widget — see [`UiRequest::VendorSetObjectStackSize`]. It is not a
    /// stack-size box and has nothing to do with `GameView::split_size`.
    ///
    /// Two statements are still **not** reproduced and are named rather than dropped:
    ///
    /// * When the changed id is selected, refresh the examination panel for that object. That
    ///   behavior is not implemented here; it is a known gap.
    /// * The sell-state check compares the list element id. This function is only ever called with
    ///   the stock list `0x100000BD`, so its `0x100000CE` arm is **dead for this list**.
    ///
    /// Returns how many rows were inserted, which is the stock list's row count.
    pub fn update_items_list(
        &mut self,
        ui: &mut UiSystem,
        s: &ShopView,
        mask: i32,
        select_first: bool,
    ) -> usize {
        // A zero argument takes the mask from the menu's selected row, and the `0` that survives a
        // missing selection is the client's own default, written *before* the branch.
        #[allow(clippy::cast_sign_loss)]
        let mask = if mask != 0 {
            mask as u32
        } else {
            self.selected_type_filter(ui).unwrap_or(0)
        };
        self.last_mask = Some(mask);
        let projection = dereth_client_contract::vendor::stock(s, mask);
        self.basket_drops += projection.basket_drops;
        self.container_drops += projection.container_drops;
        let kept = projection.rows;
        let sizes = projection.sizes;
        let first = projection.first;
        // A flush then one insert per survivor, which on this host is one `set_contents` of exactly
        // the survivors.
        self.fill(ui, Tab::Items, &kept);
        // When select-first is set, select `first`. The loop seeds `first` to zero and writes it
        // **once**, on the first row that passed both gates — so a filter that keeps nothing
        // selects **object 0**, which is the client's own clear and is passed as a literal here for
        // that reason. The vendor sub-UI's object stack-size write, once per row the loop reached
        // it for, in the client's own order and before the tail. It writes the object's stack size
        // and rescales its value, which is a **desc** and not a widget, so it is the host's to
        // apply — see [`UiRequest::VendorSetObjectStackSize`].
        self.stack_size_writes += u32::try_from(sizes.len()).unwrap_or(u32::MAX);
        for (item, size) in sizes {
            ui.requests
                .emit(UiRequest::VendorSetObjectStackSize { item, size });
        }
        if select_first {
            ui.requests.emit(UiRequest::Select(first));
        }
        // Scroll to row 0 when the list has rows — the guard counts UI rows, including any empty
        // padding created by the empty-slot update; it is not the number of stock profiles that
        // survived the filter.
        let has_rows = self
            .stock
            .as_ref()
            .map_or(!kept.is_empty(), |w| !w.slots.is_empty());
        if has_rows {
            self.scroll_restores += 1;
            if let Some(w) = self.stock.as_mut() {
                w.scroll_to_show(ui, 0);
            }
        }
        kept.len()
    }

    /// The vendor buy panel's transaction value update.
    ///
    /// For every basket row with a live object it adds the stack size (0 counts as 1) to the count
    /// and the vendor's sell price for that many to the value, then writes `L"Buying %d %s worth
    /// %hsp"` — or, for a vendor with a trade currency, `"Buying %d %s worth %d %s."` with the
    /// trade name.
    ///
    /// The list walk itself lives on the host side of the seam ([`ShopView::buy_transaction`] and
    /// [`ShopView::buy_items`] are its two accumulators); what is here is the **string**, which is
    /// the half a player reads and the half that separates this writer from the other three.
    /// Returns whether the element existed to write.
    /// The **name line, the cost line and the two
    /// buttons' states**, all four keyed on the selected stock row.
    ///
    /// With a selection, it walks the stock list for the selected object's slot. When found and the
    /// object is live, it writes [`item_name_line`] and [`item_cost_line`] from the split size, the
    /// vendor's sell price and the purse total, sets both buttons to state 1, and returns.
    /// Otherwise it clears both texts and sets both buttons to state `0x0D`.
    ///
    /// **Three things about this that are easy to get wrong.**
    ///
    /// 1. The two lines are **not per-row**. There is one of each, and it describes whatever is
    ///    selected. A build that wrote them into the slots would look wrong in a different way.
    /// 2. The walk is over **the stock list's own live items**, not over the shop's stock: a row
    ///    the
    ///    type filter is hiding cannot be the subject even while it is still selected. That is
    ///    [`Self::rows`] for [`Tab::Items`], which is what [`Self::update_items_list`] just filled.
    /// 3. Every input comes from the **manufactured live object** — the name and split-size
    ///    queries both use that object, and the vendor sell-price query uses its weenie
    ///    description. The `info(id)` guard below is the same missing-object test the client
    ///    early-outs on; it depends on `open_vendor_stock_objects` having manufactured the stock
    ///    rows' objects, without which it would answer `None` for every stock row.
    ///
    /// The split-size query is asked only about the **selected** object, and answers the current
    /// split size for exactly that case — so [`GameView::split_size`] is the whole of it and not an
    /// approximation of it.
    ///
    /// Returns how many of the four writes landed.
    fn update_items_ui(
        &mut self,
        ui: &mut UiSystem,
        s: &ShopView,
        selected: Option<dereth_primitives::ObjectId>,
        info: &dyn Fn(dereth_primitives::ObjectId) -> Option<SlotInfo>,
        split_size: i32,
    ) -> usize {
        let hit = selected.and_then(|sel| {
            let row = self.rows[Tab::Items as usize]
                .iter()
                .find(|r| r.item == sel)?;
            let i = info(sel)?;
            Some((row.price, i))
        });
        let (name, cost, state) = match hit {
            Some((price, i)) => {
                let n = u32::try_from(split_size).unwrap_or(1).max(1);
                (
                    item_name_line(&i.name, i.plural_name.as_deref(), n),
                    item_cost_line(price, s.total_value, n),
                    dereth_ui::StateId(1),
                )
            }
            // The text clear and state `0x0D`, the tail.
            None => (String::new(), String::new(), dereth_ui::StateId(0x0D)),
        };
        let mut wrote = 0;
        wrote += usize::from(set_text(ui, self.item_name_text, &name));
        wrote += usize::from(set_text(ui, self.item_cost_text, &cost));
        for h in [self.buy_button, self.add_button].into_iter().flatten() {
            ui.set_state(h, state);
            wrote += 1;
        }
        wrote
    }

    fn update_buy_transaction_value(&mut self, ui: &mut UiSystem, s: &ShopView) -> bool {
        let text = transaction_line("Buying", s.buy_items, s.buy_transaction);
        set_text(ui, self.buy_list_text, &text)
    }

    /// The vendor buy panel's total value update.
    ///
    /// It reads the *parent's* purse total and renders it. That is the whole
    /// totals-versus-transaction distinction, and it is why a station whose purse happens to equal
    /// its basket cannot tell this function from [`Self::update_buy_transaction_value`].
    fn update_buy_total_value(&mut self, ui: &mut UiSystem, s: &ShopView) -> bool {
        set_text(ui, self.buy_purse_text, &purse_line(s.total_value))
    }

    /// The vendor sell page's transaction value update.
    ///
    /// The same shape as the buy side with three differences, all of them observed in retail: the
    /// price is the vendor buy price, which takes **two** weenie descriptions, the vendor's and the
    /// item's; the whole body only runs when the vendor object is known, so an unknown vendor
    /// object leaves the text untouched; and there is **no alternate-currency fork at all**.
    ///
    /// **The vendor-object guard is not reproduced**, a known gap: this build
    /// always writes the element, where retail zeroes the transaction value and then returns
    /// without touching the sell-list text if the vendor object has not landed — so retail leaves
    /// the *previous* shop's line on screen and this one shows `Selling 0 items worth 0p`. The seam
    /// cannot represent the difference today, because [`ShopView`] carries no "the vendor object is
    /// unknown" state.
    fn update_sell_transaction_value(&mut self, ui: &mut UiSystem, s: &ShopView) -> bool {
        let text = transaction_line("Selling", s.sell_items, s.sell_transaction);
        set_text(ui, self.sell_list_text, &text)
    }

    /// The vendor sell page's total value update.
    ///
    /// [`Self::update_buy_total_value`] renders, into a **different** element. Two writers, one
    /// number — which is exactly why the four have to be asserted separately.
    fn update_sell_total_value(&mut self, ui: &mut UiSystem, s: &ShopView) -> bool {
        set_text(ui, self.sell_purse_text, &purse_line(s.total_value))
    }

    /// The vendor items panel's open vendor's type-filter strip, and
    /// one type-filter insert per surviving row.
    ///
    /// Retail remembers the stock list's horizontal scroll and the menu's selected index, flushes
    /// the stock list and the menu and zeroes the filter count, then for each of eighteen (label,
    /// mask) pairs adds a filter row when the vendor's stock contains that type. It then clamps the
    /// old index to the new last row (and to 0), selects that row with broadcast on, and restores
    /// the horizontal scroll.
    ///
    /// Stock projection is performed by the enclosing update and the queued menu callback.
    /// `pending_open_scroll_x` carries the trailing scroll restore across both, rather than
    /// restoring before the callback and losing the viewport again when it is delivered.
    ///
    /// Adding a filter row inserts a text item at the current filter count, sets its `0x10000039`
    /// to the mask, and increments the count — so the index it inserts at is the count so far, i.e.
    /// it **appends**, and each row carries its own mask.
    ///
    /// **The strip is a function of the stock, not a fixed eighteen.** The eighteen gated calls are
    /// the whole mechanism.
    ///
    /// **This builds the strip; it does not consume its selection.** The items-list update does
    /// that: it flushes the stock list, takes the mask from the selected menu row's property
    /// `0x10000039` when the argument is **0**, and then inserts only the stock rows whose object
    /// type has a bit in the mask. So the tabs *filter the stock list* — see
    /// [`Self::update_items_list`] and the message-7 arm of [`Self::on_element_message`].
    ///
    /// Returns how many rows were made, which is the filter count.
    pub fn open_vendor_type_filters(&mut self, ui: &mut UiSystem, s: &ShopView) -> usize {
        self.num_type_filters = 0;
        let prev_x = self.stock.as_ref().map_or(0, |w| w.scroll(ui).0);
        self.pending_open_scroll_x = None;
        let Some(menu) = self.type_menu else { return 0 };
        // The menu's popup construction and initialisation, lazily, exactly as init_talk_focus_menu
        // does it: in the client these run from the menu's initialisation, which has the element
        // manager to hand.
        if dereth_ui::widgets::menu::list_box_handle(ui, menu).is_none() {
            let made = ui.env().cloned().map(|e| {
                e.with_assets(|assets| dereth_ui::widgets::menu::make_popup(ui, assets, menu))
            });
            if made.flatten().is_none() {
                return 0;
            }
            dereth_ui::widgets::menu::initialize_popup(ui, menu);
        }
        let prev = i32::try_from(s.filter_index()).unwrap_or(i32::MAX);
        dereth_ui::widgets::menu::flush(ui, menu);
        // The masks come back beside the names so each row can carry its own `0x10000039`, which
        // is what makes a row a *filter* rather than a label.
        let rows: Vec<(&'static str, u32)> = if s.open {
            s.type_filters.clone()
        } else {
            Vec::new()
        };
        let made = ui.env().cloned().map(|e| {
            e.with_assets(|assets| {
                let mut out: Vec<(ElemHandle, u32)> = Vec::with_capacity(rows.len());
                for (label, mask) in &rows {
                    // Insert the text item at the filter count — the index is the count so far.
                    if let Some(h) = dereth_ui::widgets::menu::insert_text_item(
                        ui,
                        assets,
                        menu,
                        label,
                        out.len(),
                    ) {
                        out.push((h, *mask));
                    }
                }
                out
            })
        });
        let Some(made) = made else { return 0 };
        for (h, mask) in made {
            // Property `0x10000039` = mask on the row — `Integer`, per the retail property schema.
            // See [`ATTR_SHOP_FILTER`].
            #[allow(clippy::cast_possible_wrap)]
            ui.set_attribute_int(h, ATTR_SHOP_FILTER, mask as i32);
            self.num_type_filters += 1;
        }
        // The selection clamp, as retail does it: `idx = min(prev, last)`, then 0 if negative. A
        // menu that ended up empty has `last == -1`, so `idx` is 0 and item 0 is `None` — which is
        // the client's selection of no row, a cleared selection.
        let last = i32::try_from(dereth_ui::widgets::menu::num_items(ui, menu)).unwrap_or(0) - 1;
        let idx = if last <= prev { last } else { prev };
        let idx = if idx < 0 { 0 } else { idx };
        let item = usize::try_from(idx)
            .ok()
            .and_then(|i| dereth_ui::widgets::menu::get_item(ui, menu, i));
        // **The broadcast is the whole reason the vendor open fills the stock list — and it is a
        // selection write, so it is `s.open` and not a literal `true`.**
        //
        // A broadcast selection raises `MENU_CHOSEN` from `0x100000BF`, which
        // [`Self::on_element_message`] turns into the items-list update with select-first on, whose
        // tail selects `first`. On a **close** the strip is being emptied, so `item` is `None` and
        // `first` would be `0` — the client's own clear — and the player's selection would be wiped
        // by walking away from a shop. The client never gets there: this function runs only inside
        // the vendor open, which a close does not call at all. This build has to empty the strip on
        // the close, so it does that silently.
        dereth_ui::widgets::menu::set_selected_item(ui, menu, item, s.open);
        // Initialization can itself select a row. Keep the restore through every callback emitted
        // during this vendor open, not merely the first one delivered by UiFlow.
        self.pending_open_scroll_x = s.open.then_some((prev_x, ui.element_message_serial()));
        self.num_type_filters
    }

    /// The client's show on open and the button handler's hide on close.
    ///
    /// **[`PANEL`] is the element retail toggles.** `TABS` is kept in the loop because
    /// everything below hangs off it and it must not be left hidden by an earlier close, but on its
    /// own it is invisible: the environment panel hid the page above it during bring-up and only
    /// [`PANEL`] going visible lets the host raise `<ENVP>`.
    fn set_visible(&mut self, ui: &mut UiSystem, visible: bool) {
        self.visible = visible;
        for h in [self.root, self.tabs].into_iter().flatten() {
            ui.set_visible(h, visible);
        }
    }

    /// **Four arms, two of which make a tab do anything.**
    ///
    /// The listener handles these message arms: `0x1C` runs the mouse-press handling (when the
    /// items page exists); `1` runs the button-click handling on the element; `7` from `0x100000BF`
    /// runs the items-list update with mask 0 and select-first on; `0x2C` from `0x100000B8`
    /// switches on the open page — `0x100000BC` runs the items-list update with select-first off,
    /// `0x100000C4` and `0x100000CD` run the buy and sell page updates; and `0x15` runs the
    /// drop-release handling.
    ///
    /// **Message 7 is the whole of "the tabs filter the stock".** It is the strip's own
    /// `MENU_CHOSEN`, raised by a broadcast selection — which is how the vendor open's trailing
    /// broadcast selection fills the list in the first place, and how a player's click on a row
    /// re-fills it. It is keyed on the **element** (`0x100000BF`) and not on the message alone, so
    /// another menu's selection cannot reach it.
    ///
    /// **The two arms pass different select-first values.** Choosing a *filter* moves the selection
    /// to the first surviving row (`true`); merely coming back to the Items *page* does not
    /// (`false`). A build that passed one value for both would move the player's selection every
    /// time they looked at the buy basket and came back.
    ///
    /// The three tab ids are **not** button-handler cases; they appear only as
    /// arguments when opening a tab, and the panel raises
    /// `0x2C` from `0x100000B8` itself. [`Self::handle_button_click`] keeps its own tab arm
    /// because this build's [`Self::tab`] is what [`Self::rows`] is indexed by; the `0x2C` arm
    /// below is the client's mechanism and is what re-runs the sub-UI update.
    ///
    /// The client's **`0x3E` arm**, for the one list this window registers an item-list drag
    /// handler on: the drag-accept cursor over the sell basket.
    ///
    /// On `0x3E`, a zero first parameter or no drag element clears the tile's drag-accept state to
    /// `0x1000003F`; otherwise, when the tile's parent is an item list, the list's drag-over runs.
    /// That drag-over reads the drag source's drop-icon info (item, spell, flags) and asks the
    /// list's registered drag handler **before** its own default, returning if the handler took it.
    ///
    /// The vendor sell page's constructor registers a drag handler on the sell basket `0x100000CE`
    /// and on **nothing else**, so the stock list and the buy basket keep the item-list drag-over
    /// default (and the stock list carries `UI_ItemList_IsVendor`, which that default returns on).
    /// This method is therefore keyed on [`SELL_LIST`] alone.
    ///
    /// The handler itself is four lines and **always returns true** — so once a drag is over a
    /// sell-list tile the default three-way hint can never run: with an item and no alias flags it
    /// sets `0x10000040` when the vendor accepts the item and `0x10000041` when it does not;
    /// otherwise it writes nothing.
    ///
    /// **The alias mask is `14`, not a single bit** (the drop-item flags are `IS_CONTAINER 1,
    /// IS_VENDOR 2, IS_SHORTCUT 4, IS_SALVAGE 8, IS_ALIAS 14`), so the test is the same `flags &
    /// 0x0E` that the drop path refuses on — a spell, a shortcut or a salvage drag gets **no hint
    /// at all** and the tile keeps whatever it had.
    ///
    /// Returns true when the message was this window's, which is what
    /// `RemainingPanels::on_element_message`'s fan-out reads.
    pub fn on_drag_cursor_over(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        use crate::items::widget::{drag_accept_state, drag_flags, inq_drop_icon_info};
        if m.id != dereth_ui::msg::element::id::DRAG_CURSOR_OVER {
            return false;
        }
        let Some(list) = self.sell.as_mut() else {
            return false;
        };
        let Some(slot) = list.slot_of(m.source) else {
            return false;
        };
        // Both of the arm's clearing routes: a zero first parameter, and a non-zero one with no
        // drag element. Neither reaches the handler.
        let proxy = ui.drag_state().element.filter(|_| m.p1 != 0);
        let Some(proxy) = proxy else {
            list.slots[slot].set_drag_accept_state(ui, drag_accept_state::NONE);
            return true;
        };
        let info = inq_drop_icon_info(ui, proxy);
        // The alias mask = `IS_VENDOR | IS_SHORTCUT | IS_SALVAGE` = 0x0E.
        const IS_ALIAS: u32 =
            drag_flags::IS_VENDOR | drag_flags::IS_SHORTCUT | drag_flags::IS_SALVAGE;
        if let Some(item) = info.item.filter(|_| info.flags & IS_ALIAS == 0) {
            let s = if view.vendor_drag_item_accepted(item) {
                drag_accept_state::ACCEPT
            } else {
                drag_accept_state::REFUSE
            };
            list.slots[slot].set_drag_accept_state(ui, s);
        }
        true
    }

    /// The client's **`0x15` arm** — the clear that
    /// takes [`Self::on_drag_cursor_over`]'s hint back down. Without it the drag/drop indicators
    /// stay up after the drag stops.
    ///
    /// What the client does:
    ///
    /// On message `0x15`, inspect the catcher element. If present and a UI item (type
    /// `0x10000032`), clear its drag-accept state to `0x1000003F`. A missing catcher or a non-item
    /// skips that write. All three cases then handle the drop release with the original message;
    /// other messages leave this arm immediately.
    ///
    /// **It is the only producer of the clear on the drop path.** The drop raises no `0x3E` of its
    /// own, and the pointer does not leave the tile it was dropped on, so neither of the client's
    /// two clearing routes (a zero first parameter, and a non-zero one with no drag element) ever
    /// runs.
    ///
    /// `GamePlayScreen::on_element_message` has this arm for
    /// [`crate::panels::inventory::InventoryPanels`]' four lists only — the vendor's three hang off
    /// [`crate::panels::remaining::RemainingPanels`], so they are offered the message here.
    ///
    /// The clear is **unconditional**: the client's message-id check is the whole guard, so it runs
    /// whether the drop was taken or refused, and it runs *before* the drop is handled. All three
    /// lists are offered because the retail arm is on `ItemListWidget` and does not care which list
    /// it is on.
    ///
    /// Returns true when the drop landed on one of this window's tiles, which is what
    /// `RemainingPanels::on_element_message`'s fan-out reads. It is deliberately not a
    /// short-circuit for anybody else: no other panel owns these elements.
    pub fn on_drop_release(&mut self, ui: &mut UiSystem, m: &dereth_ui::ElementMessage) -> bool {
        use crate::items::widget::drag_accept_state;
        // The **catcher's** copy of the message, which is the one carrying the drag's owner in
        // `p2`; the owner's own copy has `p2 == 0` and names a slot in the pack.
        if m.id != dereth_ui::msg::element::id::DROP_FAILED || m.p2 == 0 {
            return false;
        }
        for w in [self.stock.as_mut(), self.buy.as_mut(), self.sell.as_mut()]
            .into_iter()
            .flatten()
        {
            if let Some(slot) = w.slot_of(m.source) {
                w.slots[slot].set_drag_accept_state(ui, drag_accept_state::NONE);
                return true;
            }
        }
        false
    }

    /// Uses the selected object and the current split size, both read off `view`.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        use dereth_ui::msg::element::id;
        // The whole view is passed because [`Self::redecorate`] needs it; the client reads the
        // selected object and the split size as its own second and third statements, which is
        // where they are used.
        let selected = view.selected_object();
        let split = view.split_size();
        // **The item list element's element-message handler's `0x1C` arm, on this window's own
        // three lists.** See [`Self::on_item_list_press`] for why it is here and
        // not on the screen.
        if m.id == id::MOUSE_PRESS && self.on_item_list_press(ui, m.source, m.p1, split) {
            return true;
        }
        if m.id == id::BUTTON_CLICKED {
            return self.handle_button_click(&mut ui.requests, m.source_id, selected, split);
        }
        // Message 7 from `0x100000BF`: the items-list update with mask 0 and select-first on.
        if m.id == id::MENU_CHOSEN && m.source_id == TYPE_FILTER_MENU {
            if let Some(menu) = self.type_menu {
                let index = dereth_ui::widgets::menu::selected_index(ui, menu).max(0) as usize;
                ui.requests.emit(UiRequest::VendorFilter(index));
            }

            let Some(s) = self.last.clone() else {
                return false;
            };
            self.update_items_list(ui, &s, 0, true);
            // Retail's insert decorates every row it inserts.
            self.redecorate(ui, view);
            if let Some((x, through)) = self.pending_open_scroll_x {
                if m.serial <= through {
                    if let Some(w) = self.stock.as_ref() {
                        dereth_ui::widgets::listbox::set_scroll_offset(
                            ui,
                            w.handle,
                            x,
                            w.scroll(ui).1,
                        );
                    }
                }
                if m.serial >= through {
                    self.pending_open_scroll_x = None;
                }
            }
            self.filter_applications += 1;
            return true;
        }
        // Message `0x2C` from `0x100000B8`: switch on the open page.
        if m.id == id::TAB_PAGE_CHANGED && m.source_id == TABS {
            let page = self.open_page(ui);
            let Some(s) = self.last.clone() else {
                return false;
            };
            match page {
                Some(PAGE_ITEMS) => {
                    self.tab = Tab::Items;
                    self.update_items_list(ui, &s, 0, false);
                    // Retail's insert decorates every row it inserts; without this, tabbing off
                    // Items and back draws the stock with no background.
                    self.redecorate(ui, view);
                    self.filter_applications += 1;
                }
                // The vendor buy panel's update / the vendor sell page's update are each four
                // steps: adopt the basket as contents, update the page UI, update the transaction
                // value, update the total value. The **last two** are the money writers this panel
                // already has; the first two are the basket's own list fill, which on this host is
                // [`Self::fill`] on the rebuild rather than on the page change — a declared gap,
                // and the reason a basket that changed while the Items page was up is redrawn by
                // `update` and not by this arm.
                Some(PAGE_BUY) => {
                    self.tab = Tab::Buying;
                    self.update_buy_transaction_value(ui, &s);
                    self.update_buy_total_value(ui, &s);
                }
                Some(PAGE_SELL) => {
                    self.tab = Tab::Selling;
                    self.update_sell_transaction_value(ui, &s);
                    self.update_sell_total_value(ui, &s);
                }
                // The client's page switch has no default: a fourth page would do nothing, and the
                // message is still this window's.
                _ => {}
            }
            return true;
        }
        false
    }

    /// The vendor panel's drop handling → the vendor sell page's drag-accept test.
    ///
    /// It forks on the splitter: with the whole stack dialled in (`split == max`) the item is
    /// offered as it is; with part of it, the split is asked for first and the part the server
    /// makes is what lands in the list. Both refusals -- the vendor not taking the item, or the
    /// split not being possible (*"Cannot split the stack to sell it"*) -- belong to the host,
    /// which holds the object table.
    ///
    /// **Waiting state 0 and not 1** — the same polarity the secure-trade panel's drag-accept has
    /// and the opposite of a pack move's. Nothing is on the wire yet for a whole stack, so there is
    /// nothing to wait for: the row goes into the basket at once and "Sell All" is what asks the
    /// shard. On this host that is the ghost `GamePlayScreen::handle_drop_release` already clears
    /// on the drop, so there is no write here.
    ///
    /// Returns whether a request was emitted.
    pub fn drop_item(
        &mut self,
        requests_out: &mut crate::requests::Outbox,
        item: dereth_primitives::ObjectId,
        split: u32,
        max: u32,
    ) -> bool {
        if split < max {
            requests_out.emit(UiRequest::VendorSplitToSell { item, split, max });
        } else {
            requests_out.emit(UiRequest::VendorAddToSell { item });
        }
        true
    }

    /// The client's message-`0x1C` arm, run on whichever of **this window's** three lists the press
    /// landed in.
    ///
    /// With an item under the mouse, the input action decides. Action 7 (left click) does a
    /// targeted use when a targeting mode is active; otherwise, on a non-empty slot, it applies
    /// single selection (if the list has it), selects the object, and opens a container when the
    /// list is a container list with a child list. Action 8 (right click) does the same single
    /// selection and select, then examines the object. Action 10 (double click) uses the object
    /// unless the list is a vendor list or a salvage list.
    ///
    /// **Why this lives on the panel and not on `GamePlayScreen`.** The client has no dispatch to
    /// do — the element-message handler runs *on* the list — so where the body sits is this build's
    /// question, not retail's. `GamePlayScreen::on_item_list_press` is the same logic for the pack
    /// and the quickbar, and it resolves the pressed element through `InventoryPanels::locate`; the
    /// vendor's `0x100000BD`, `0x100000C5` and `0x100000CE` are not in that map, because this whole
    /// window hangs off `RemainingPanels`. The press is therefore answered here, on the panel that
    /// owns the lists, rather than by widening `InventoryPanels`.
    ///
    /// Two of the client's branches are named and not taken, and both are named because they are
    /// *reached* and do nothing rather than being absent:
    ///
    /// * an active targeting mode — `GamePlayScreen::interaction_target_mode` is the screen's, and
    ///   a panel reached through `RemainingPanels::on_element_message` is handed the view, not the
    ///   screen. A targeted-use click on a vendor row therefore selects instead of executing.
    /// * the container-list open — `UI_ItemList_IsContainer` is false on all three of these lists
    ///   in the shipped tree, so the container open is unreachable from them in retail too. The
    ///   flag is read off the live widget rather than assumed.
    ///
    /// Action 10's guard is the interesting one and it is **not** a no-op: the vendor-list flag is
    /// [`crate::items::widget::attr::IS_VENDOR`], read off the live element, and it is what stops a
    /// double-click in a shop from *eating the bread you were about to buy*.
    ///
    /// Single selection resolves `true` on exactly one of the 65 item lists in the shipped gameplay
    /// tree — `0x100000C5`, this window's buy basket — so this is the only caller in the build for
    /// which [`ItemListWidget::handle_single_selection`] does anything at all.
    ///
    /// **The window's own double-click on the stock list buys.** Beside the list's arm, the vendor
    /// window answers the same press itself: a double-click on a stock row buys that item at once,
    /// exactly as "Buy" does with the row selected — one item, or the split amount for a stack,
    /// sent on its own and not added to the basket. `split` is the current split size, which the
    /// first click of the pair has just made the row's.
    ///
    /// Returns whether the press landed in one of this window's lists.
    fn on_item_list_press(
        &mut self,
        ui: &mut UiSystem,
        source: ElemHandle,
        action: u32,
        split: i32,
    ) -> bool {
        use dereth_ui::focus::action as act;
        let in_stock = self
            .stock
            .as_ref()
            .is_some_and(|w| w.slot_of(source).is_some());
        let Some(w) = [self.stock.as_mut(), self.buy.as_mut(), self.sell.as_mut()]
            .into_iter()
            .flatten()
            .find(|w| w.slot_of(source).is_some())
        else {
            return false;
        };
        let slot = w.slot_of(source).expect("just matched");
        // No item under the mouse is the guard above; an empty slot (item id 0) is this one — it
        // does nothing at all, not even a deselect. The message is still this window's, so it is
        // consumed either way.
        let Some(item) = w.item_at(slot) else {
            return true;
        };
        let single = w.single_selection;
        let vendor_list = w.vendor_list;
        let salvage_list = w.salvage_list;
        match action {
            act::PRIMARY_CLICK => {
                if single {
                    w.handle_single_selection(ui, slot);
                }
                ui.requests.emit(UiRequest::Select(item));
            }
            act::SECONDARY_CLICK => {
                if single {
                    w.handle_single_selection(ui, slot);
                }
                ui.requests.emit(UiRequest::Select(item));
                ui.requests.emit(UiRequest::Examine(item));
            }
            // Action 10 — `0x0A` is the left **double**-click.
            0x0A if !vendor_list && !salvage_list => {
                ui.requests.emit(UiRequest::Use(item));
            }
            // The window's arm: the stock list carries the vendor-list flag, so the list's own
            // use above never runs on it, and the double-click is the buy.
            0x0A if in_stock => {
                ui.requests.emit(UiRequest::VendorBuySingle { item, split });
            }
            _ => {}
        }
        true
    }

    /// The client's final open of the Buying tab (`0x100000BA`).
    pub fn open_buying(&mut self, ui: &mut UiSystem) -> bool {
        let Some(tabs) = self.tabs else { return false };
        let Some(tab) = ui.get_child_recursive(tabs, TAB_BUYING) else {
            return false;
        };
        // Behaviour borrowing is private to dereth-ui, so cross its public callback seam with
        // ACTIVATED, one of the two exact messages the panel's element-message handler maps to the
        // same tab-open the client calls directly.
        ui.broadcast_element_message(tab, dereth_ui::msg::element::id::ACTIVATED, 0, 0);
        self.open_page(ui) == Some(PAGE_BUY)
    }

    /// The open of the Selling tab (`0x100000BB`), by the same route as [`Self::open_buying`].
    pub fn open_selling(&mut self, ui: &mut UiSystem) -> bool {
        let Some(tabs) = self.tabs else { return false };
        let Some(tab) = ui.get_child_recursive(tabs, TAB_SELLING) else {
            return false;
        };
        ui.broadcast_element_message(tab, dereth_ui::msg::element::id::ACTIVATED, 0, 0);
        self.open_page(ui) == Some(PAGE_SELL)
    }

    /// Something carried over the shop window turns it to the Selling tab, on every frame, by
    /// itself: while the window is up and not already on the selling page, a drag whose pointer
    /// is strictly inside the window's own box opens the Selling tab. Anywhere inside the window
    /// does it, not only the tab, so whatever the player is carrying arrives at the sell list.
    ///
    /// Returns whether it opened the tab this frame.
    pub fn update_drag_over(&mut self, ui: &mut UiSystem) -> bool {
        let Some(root) = self.root else { return false };
        if !ui.is_visible(root) || !ui.is_dragging() {
            return false;
        }
        if self.tabs.is_none() || self.open_page(ui) == Some(PAGE_SELL) {
            return false;
        }
        let (x, y) = ui.mouse_pos();
        let b = ui.screen_box(root);
        if !(b.x0 < x && x < b.x1 && b.y0 < y && y < b.y1) {
            return false;
        }
        self.open_selling(ui)
    }

    /// The vendor panel's open page — which of the three pages `0x100000B8` is showing.
    ///
    /// The `0x2C` arm reads this rather than the message, because the tab open broadcasts `0x2C`
    /// with both parameters 0: the message says *a* page changed and never says which.
    #[must_use]
    pub fn open_page(&self, ui: &UiSystem) -> Option<ElementId> {
        let tabs = self.tabs?;
        let p = ui
            .node(tabs)?
            .behaviour
            .as_ref()?
            .as_any()?
            .downcast_ref::<dereth_ui::widgets::panel::Panel>()?;
        p.open_page
    }

    /// The element-message handler's button-click handling on the clicked element — the one
    /// `switch`, as requests.
    ///
    /// `selected` is the selected object, which every one of the eleven cases reads **before** the
    /// switch (reading it is the function's second statement). `split` is the current split size.
    /// Returns true when the click was one of the cases.
    pub fn handle_button_click(
        &mut self,
        requests_out: &mut crate::requests::Outbox,
        id: ElementId,
        selected: Option<dereth_primitives::ObjectId>,
        split: i32,
    ) -> bool {
        let mut emit = |r: UiRequest| {
            requests_out.emit(r);
            true
        };
        match id {
            TAB_ITEMS => {
                self.tab = Tab::Items;
                true
            }
            TAB_BUYING => {
                self.tab = Tab::Buying;
                true
            }
            TAB_SELLING => {
                self.tab = Tab::Selling;
                true
            }
            BTN_BUY | BTN_BUY_ITEM => match selected {
                Some(item) => emit(UiRequest::VendorBuySingle { item, split }),
                None => true,
            },
            BTN_ADD_TO_LIST => match selected {
                Some(item) => emit(UiRequest::VendorAddToBuyList { item, split }),
                None => true,
            },
            BTN_BUY_ALL => emit(UiRequest::VendorBuyAll),
            BTN_SELL_ITEM => match selected {
                Some(item) => emit(UiRequest::VendorSellSingle { item }),
                None => true,
            },
            BTN_SELL_ALL => emit(UiRequest::VendorSellAll),
            BTN_BUY_CLEAR_ITEM => emit(UiRequest::VendorClearList {
                sell: false,
                item: selected,
            }),
            BTN_BUY_CLEAR_LIST => emit(UiRequest::VendorClearList {
                sell: false,
                item: None,
            }),
            BTN_SELL_CLEAR_ITEM => emit(UiRequest::VendorClearList {
                sell: true,
                item: selected,
            }),
            BTN_SELL_CLEAR_LIST => emit(UiRequest::VendorClearList {
                sell: true,
                item: None,
            }),
            BTN_CLOSE => emit(UiRequest::VendorClose),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vendor button handler's eleven `case` labels, **as literals**.
    ///
    /// the stated testability rule: *"a test that reads a constant through the same symbol it writes it
    /// through cannot detect a wrong constant"*. Every other test here uses the symbols; this one
    /// states the numbers, from the `switch`'s own `case` list.
    #[test]
    fn the_button_ids_are_the_switch_cases_in_the_client() {
        let mut outbox = crate::requests::Outbox::owned();
        assert_eq!(BTN_BUY, ElementId(0x1000_00C2));
        assert_eq!(BTN_ADD_TO_LIST, ElementId(0x1000_00C3));
        assert_eq!(BTN_BUY_ITEM, ElementId(0x1000_00C9));
        assert_eq!(BTN_BUY_ALL, ElementId(0x1000_00CA));
        assert_eq!(BTN_BUY_CLEAR_ITEM, ElementId(0x1000_00CB));
        assert_eq!(BTN_BUY_CLEAR_LIST, ElementId(0x1000_00CC));
        assert_eq!(BTN_SELL_ITEM, ElementId(0x1000_00D2));
        assert_eq!(BTN_SELL_ALL, ElementId(0x1000_00D3));
        assert_eq!(BTN_SELL_CLEAR_ITEM, ElementId(0x1000_00D4));
        assert_eq!(BTN_SELL_CLEAR_LIST, ElementId(0x1000_00D5));
        assert_eq!(BTN_CLOSE, ElementId(0x1000_00D6));
        // The five ids the switch lists and **does nothing for** — `case 0x100000C4 … 0x100000C8`
        // and `0x100000CD … 0x100000D1` all fall into the empty `break`. They are the pages, the
        // lists and the scrollbars, and a click on one is not a button click.
        let mut p = VendorPanel::default();
        for id in [
            0x1000_00C4u32,
            0x1000_00C5,
            0x1000_00C6,
            0x1000_00CD,
            0x1000_00CE,
        ] {
            assert!(
                !p.handle_button_click(&mut outbox, ElementId(id), None, 1),
                "{id:#X} is not a button"
            );
        }
    }

    /// The three tab ids and the three page ids, as literals, from the shipped `0x100000B8`'s
    /// `0x2E` tab array — `{state 0x100000B9 -> page 0x100000BC, default}`,
    /// `{0x100000BA -> 0x100000C4}`, `{0x100000BB -> 0x100000CD}`.
    #[test]
    fn the_tab_array_pairs_the_ids_the_shipped_layout_pairs() {
        assert_eq!(
            (TAB_ITEMS, PAGE_ITEMS),
            (ElementId(0x1000_00B9), ElementId(0x1000_00BC))
        );
        assert_eq!(
            (TAB_BUYING, PAGE_BUY),
            (ElementId(0x1000_00BA), ElementId(0x1000_00C4))
        );
        assert_eq!(
            (TAB_SELLING, PAGE_SELL),
            (ElementId(0x1000_00BB), ElementId(0x1000_00CD))
        );
        assert_eq!(TABS, ElementId(0x1000_00B8));
        assert_eq!(VENDOR_ROOT, ElementId(0x1000_00B7));
        assert_eq!(
            (STOCK_LIST, BUY_LIST, SELL_LIST),
            (
                ElementId(0x1000_00BD),
                ElementId(0x1000_00C5),
                ElementId(0x1000_00CE)
            )
        );
        // The default tab is the first row of the array, the one with `Bool(true)`.
        assert_eq!(Tab::default(), Tab::Items);
    }

    /// The eleven cases route to the six requests the button handler distinguishes, and the two
    /// that **send on their own** are separate from the two that fill a basket.
    #[test]
    fn each_button_emits_the_request_its_case_performs() {
        let mut outbox = crate::requests::Outbox::owned();
        use dereth_primitives::ObjectId;
        let item = ObjectId(0x8000_0A6E);
        let mut p = VendorPanel::default();
        let one = |outbox: &mut crate::requests::Outbox, p: &mut VendorPanel, id, split| {
            outbox.clear();
            assert!(p.handle_button_click(outbox, id, Some(item), split));
            outbox.take()
        };
        assert_eq!(
            one(&mut outbox, &mut p, BTN_BUY, 2),
            vec![UiRequest::VendorBuySingle { item, split: 2 }]
        );
        assert_eq!(
            one(&mut outbox, &mut p, BTN_BUY_ITEM, 1),
            vec![UiRequest::VendorBuySingle { item, split: 1 }],
            "0x100000C9 buys the single item too; it differs only in the list edit that follows"
        );
        assert_eq!(
            one(&mut outbox, &mut p, BTN_ADD_TO_LIST, 7),
            vec![UiRequest::VendorAddToBuyList { item, split: 7 }],
            "Add to List does not send"
        );
        assert_eq!(
            one(&mut outbox, &mut p, BTN_BUY_ALL, 1),
            vec![UiRequest::VendorBuyAll]
        );
        assert_eq!(
            one(&mut outbox, &mut p, BTN_SELL_ITEM, 1),
            vec![UiRequest::VendorSellSingle { item }]
        );
        assert_eq!(
            one(&mut outbox, &mut p, BTN_SELL_ALL, 1),
            vec![UiRequest::VendorSellAll]
        );
        assert_eq!(
            one(&mut outbox, &mut p, BTN_BUY_CLEAR_ITEM, 1),
            vec![UiRequest::VendorClearList {
                sell: false,
                item: Some(item)
            }]
        );
        assert_eq!(
            one(&mut outbox, &mut p, BTN_BUY_CLEAR_LIST, 1),
            vec![UiRequest::VendorClearList {
                sell: false,
                item: None
            }],
            "Clear List ignores the selection"
        );
        assert_eq!(
            one(&mut outbox, &mut p, BTN_SELL_CLEAR_ITEM, 1),
            vec![UiRequest::VendorClearList {
                sell: true,
                item: Some(item)
            }]
        );
        assert_eq!(
            one(&mut outbox, &mut p, BTN_SELL_CLEAR_LIST, 1),
            vec![UiRequest::VendorClearList {
                sell: true,
                item: None
            }]
        );
        assert_eq!(
            one(&mut outbox, &mut p, BTN_CLOSE, 1),
            vec![UiRequest::VendorClose]
        );
        // The three tabs are local: they change what is showing and send nothing.
        outbox.clear();
        assert!(p.handle_button_click(&mut outbox, TAB_SELLING, Some(item), 1));
        assert_eq!(p.tab, Tab::Selling);
        assert_eq!(outbox.len(), 0, "a tab is not a message");
    }

    /// A button that needs a selection and has none is still **handled** — the client's object
    /// lookup for id 0 finds nothing and the case falls out of the switch — and sends nothing.
    #[test]
    fn a_button_with_no_selection_sends_nothing_and_is_still_a_button() {
        let mut outbox = crate::requests::Outbox::owned();
        let mut p = VendorPanel::default();
        outbox.clear();
        for id in [BTN_BUY, BTN_BUY_ITEM, BTN_ADD_TO_LIST, BTN_SELL_ITEM] {
            assert!(
                p.handle_button_click(&mut outbox, id, None, 1),
                "{id:?} is a case of the switch"
            );
        }
        assert_eq!(outbox.len(), 0);
        // …but the two whole-list buttons do not need one.
        assert!(p.handle_button_click(&mut outbox, BTN_BUY_ALL, None, 1));
        assert!(p.handle_button_click(&mut outbox, BTN_SELL_ALL, None, 1));
        assert_eq!(outbox.len(), 2);
        outbox.clear();
    }

    /// An unbound panel is honest about it: `bound()` is false, `update` still runs and reports a
    /// rebuild, and `rows` reports what it *would* have written.
    ///
    /// This is the third state the live-run evidence asks for — "no stock" and "the panel
    /// was never driven" must not look the same.
    #[test]
    fn an_unbound_panel_still_reports_whether_it_ran() {
        let mut ui = UiSystem::new((800, 600));
        let mut p = VendorPanel::default();
        assert!(!p.bound());
        assert_eq!(p.rebuilds, 0);
        struct V(ShopView);
        impl std::fmt::Debug for V {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("V")
            }
        }
        impl GameView for V {
            fn shop(&self) -> ShopView {
                self.0.clone()
            }
        }
        let v = V(ShopView {
            open: true,
            stock: vec![ShopRow {
                item: dereth_primitives::ObjectId(1),
                name: "Oil of Rendering".into(),
                amount: -1,
                obj_type: 0x0000_0080,
                price: 5,
                ..ShopRow::default()
            }],
            type_filters: vec![("Miscellaneous", 0x0000_0490)],
            ..ShopView::default()
        });
        assert!(p.update(&mut ui, &v), "the first drive is always a rebuild");
        assert_eq!(p.rebuilds, 1);
        assert_eq!(
            p.selected_type_filter(&ui),
            None,
            "no menu at all -- not merely no selection"
        );
        assert_eq!(
            p.last_mask,
            Some(0),
            "...and the pass still ran, with retail's default mask"
        );
        assert!(p.rows(Tab::Items).is_empty(), "mask 0 keeps nothing");
        // The two lists no mask gates still report what they would have written, which is the
        // third state this test exists for: "no stock" and "never driven" must not look alike.
        assert!(p.rows(Tab::Buying).is_empty());
        assert_eq!(p.num_type_filters, 0, "and no menu means no rows in it");
        assert!(
            !p.update(&mut ui, &v),
            "the snapshot guard holds the second frame off"
        );
        assert_eq!(p.rebuilds, 1);
    }
}
