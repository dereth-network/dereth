use super::*;

/// The two players and the two items every trade scenario below uses.
const ME: ObjectId = ObjectId(0x5000_0001);
const PARTNER: ObjectId = ObjectId(0x5000_0002);
/// The item the shard has already confirmed, so the table is never empty.
const CONFIRMED: ObjectId = ObjectId(0x8000_0A6E);
/// The item that is dropped and then refused.
const REFUSED: ObjectId = ObjectId(0x8000_0A6F);
const KEY_ICON: DataId = DataId(0x0600_103F);
const ARROW_ICON: DataId = DataId(0x0600_1040);

/// The text an element of the live tree is actually carrying.
fn element_text(ui: &mut UiSystem, root: ElemHandle, id: ElementId) -> String {
    let h = ui
        .get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the tree"));
    ui.text_element_mut(h)
        .unwrap_or_else(|| panic!("{id:?} is a text element"))
        .glyphs
        .inq_text(false)
}

/// The state an element of the live tree is actually in.
fn element_state(ui: &UiSystem, root: ElemHandle, id: ElementId) -> u32 {
    let h = ui
        .get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the tree"));
    ui.node(h).expect("live").state.0
}

/// One merged boolean attribute of an element of the live tree.
fn element_attr_bool(ui: &UiSystem, root: ElemHandle, id: ElementId, attr: u32) -> Option<bool> {
    let h = ui.get_child_recursive(root, id)?;
    ui.node(h)?.merged_properties().get_bool(attr)
}

/// Whether the trade button is showing itself as agreed. An absent attribute is `false`, which is
/// the state the shipped layout is in before anything writes it.
fn toggled(ui: &UiSystem, root: ElemHandle) -> bool {
    element_attr_bool(ui, root, trade::BTN_TRADE, dereth_ui::props::attr::TOGGLED).unwrap_or(false)
}

/// The picture each filled slot of a live list is drawing, in slot order.
fn drawn_icons(ui: &UiSystem, w: &ItemListWidget) -> Vec<Option<DataId>> {
    w.slots
        .iter()
        .filter(|s| s.item.is_some())
        .map(|s| {
            s.icon
                .and_then(|h| ui.node(h))
                .and_then(|n| n.region.image.as_ref())
                .map(|g| g.did)
        })
        .collect()
}

/// What each filled slot of a live list says about itself on hover, in slot order.
fn tooltips(w: &ItemListWidget) -> Vec<Option<String>> {
    w.slots
        .iter()
        .filter(|s| s.item.is_some())
        .map(|s| s.tooltip.clone())
        .collect()
}

/// The numeral each filled slot is showing. `-1` is the client's "hidden".
fn quantities(w: &ItemListWidget) -> Vec<i32> {
    w.slots
        .iter()
        .filter(|s| s.item.is_some())
        .map(|s| s.quantity_value)
        .collect()
}

/// One object, as much of it as the panels' seam exposes.
///
/// It is **one record** on purpose: the production host answers `name`, `plural_name`, `icon` and
/// `slot_decoration` from one `weenie(id)?`, so a fixture that could answer one and not another
/// would represent a state the client cannot be in, and a guard written against it would measure
/// nothing.
#[derive(Debug, Clone, Default)]
struct Obj {
    name: String,
    plural: String,
    decoration: SlotDecoration,
}

fn obj(name: &str, plural: &str, icon: DataId, stack: u32) -> Obj {
    Obj {
        name: name.to_owned(),
        plural: plural.to_owned(),
        decoration: SlotDecoration {
            icon_id: icon.0,
            // A stack of none and a stack of one are the same thing at this seam.
            stack_size: stack.max(1),
            ..SlotDecoration::default()
        },
    }
}

/// One trade snapshot, one shop snapshot, one selection and one object table -- the view both
/// panels read. The clock is not in it: that reaches them as the UI system's own time.
#[derive(Debug, Default, Clone)]
pub(super) struct Fixture {
    trade: TradeView,
    pub(super) shop: ShopView,
    selected: Option<ObjectId>,
    objects: std::collections::BTreeMap<ObjectId, Obj>,
    /// A live cooldown that started at `start` and runs for `duration`, keyed by its own id.
    cooldown: Option<(u32, f64, f64)>,
}

impl Fixture {
    fn with(mut self, id: ObjectId, o: Obj) -> Self {
        self.objects.insert(id, o);
        self
    }

    /// A copy with exactly one object mutated, so that "one field alone" is enforced by
    /// construction rather than by care.
    fn clone_with(&self, id: ObjectId, f: impl FnOnce(&mut Obj)) -> Self {
        let mut c = self.clone();
        f(c.objects.get_mut(&id).expect("the scenario's own object"));
        c
    }
}

impl GameView for Fixture {
    fn trade(&self) -> TradeView {
        self.trade.clone()
    }

    fn shop(&self) -> ShopView {
        self.shop.clone()
    }

    fn selected_object(&self) -> Option<ObjectId> {
        self.selected
    }

    fn name(&self, id: ObjectId) -> Option<&str> {
        self.objects.get(&id).map(|o| o.name.as_str())
    }

    fn plural_name(&self, id: ObjectId) -> Option<&str> {
        self.objects.get(&id).map(|o| o.plural.as_str())
    }

    fn icon(&self, id: ObjectId) -> Option<DataId> {
        self.objects
            .get(&id)
            .and_then(|o| (o.decoration.icon_id != 0).then_some(DataId(o.decoration.icon_id)))
    }

    fn slot_decoration(&self, id: ObjectId) -> Option<SlotDecoration> {
        self.objects.get(&id).map(|o| o.decoration)
    }

    /// The window's own ownership test: an id this fixture knows about is one the player carries.
    fn trade_drag_item_acceptable(&self, item: ObjectId) -> bool {
        self.objects.contains_key(&item)
    }

    /// What is left of a cooldown at `now`. **A function of the clock**, which is the whole
    /// reason both guards exempt it.
    fn cooldown_remaining(&self, cooldown_id: u32, now: f64) -> Option<f64> {
        let (key, start, duration) = self.cooldown?;
        if key != cooldown_id {
            return None;
        }
        let left = (start + duration) - now;
        (left > 0.0).then_some(left)
    }
}

/// An open negotiation with `partner_name` across the table and nothing on it.
fn open_with(partner_name: &str) -> Fixture {
    Fixture {
        trade: TradeView {
            open: true,
            partner: Some(PARTNER),
            partner_name: partner_name.into(),
            ..TradeView::default()
        },
        ..Fixture::default()
    }
}

/// A stack of `stack` arrows on the **partner's** side: the row a player most needs to read,
/// because it is what they are being offered.
fn trade_with_stack(stack: u32) -> Fixture {
    let mut f = open_with("Alba");
    f.trade.partner_rows = vec![TradeRow {
        item: REFUSED,
        name: "Arrow".into(),
        icon: Some(ARROW_ICON),
    }];
    f.with(REFUSED, obj("Arrow", "Arrows", ARROW_ICON, stack))
}

/// A shop with one stock row: a stack of `stack`, advertised as `amount` of them.
pub(super) fn shop_with_stack(stack: u32, amount: i32) -> Fixture {
    Fixture {
        shop: ShopView {
            open: true,
            vendor: Some(ObjectId(0x8000_0001)),
            stock: vec![ShopRow {
                item: REFUSED,
                name: "Arrow".into(),
                icon: Some(ARROW_ICON),
                amount,
                // A weapon, with a "Weapons" tab below: the stock list draws only the rows the
                // chosen tab keeps, so a row with no type and a shop with no tab draws nothing.
                obj_type: 0x0000_0100,
                max_stack_size: 0,
                contained_items: 0,
                contained_containers: 0,
                price: 5,
                refusal: None,
            }],
            type_filters: vec![("Weapons", 0x0000_0101)],
            ..ShopView::default()
        },
        ..Fixture::default()
    }
    .with(REFUSED, obj("Arrow", "Arrows", ARROW_ICON, stack))
}

fn requests_len(requests: &mut dereth_ui_screens::requests::Outbox) -> usize {
    requests.len()
}

fn bound_trade(app: &mut App) -> TradePanel {
    let (ui, root) = gameplay_root(app);
    let mut p = TradePanel::default();
    p.post_init(ui, root);
    assert!(
        p.bound(),
        "the trade panel found its own item list in the shipped tree"
    );
    p
}

pub(super) fn bound_vendor(app: &mut App) -> VendorPanel {
    let (ui, root) = gameplay_root(app);
    let mut p = VendorPanel::default();
    p.post_init(ui, root);
    assert!(
        p.bound(),
        "the vendor panel found the stock list in the shipped tree"
    );
    p
}

// ---------------------------------------------------------------------------------------------
// trade.window.every-element-the-panel-binds-to-is-in-the-shipped-tree
// ---------------------------------------------------------------------------------------------

/// Every element the trade window reaches for is in the screen the client builds.
///
/// The instrument is calibrated both ways before it certifies anything: a live element of another
/// window must be found, and a declared-but-never-instantiated id of that same other window must
/// **not** be -- so a zero here cannot be the walk.
pub(super) fn every_trade_element_is_in_the_live_tree() {
    let mut c = a_gameplay_client();

    /// A window id that is live in the shipped tree: the positive control.
    const LIVE_CONTROL: u32 = 0x1000_00B8;
    /// A window id that is declared and never instantiated: the negative control.
    const ABSENT_CONTROL: u32 = 0x1000_00B7;

    let wanted: [(&str, u32); 10] = [
        ("the partner's name", trade::OTHER_NAME.0),
        ("the partner's light", trade::OTHER_STATUS.0),
        ("the partner's total", trade::OTHER_TOTAL.0),
        ("the partner's list", trade::OTHER_LIST.0),
        ("your name", trade::SELF_NAME.0),
        ("the trade button", trade::BTN_TRADE.0),
        ("your total", trade::SELF_TOTAL.0),
        ("your list", trade::SELF_LIST.0),
        ("the clear button", trade::BTN_CLEAR_ALL.0),
        ("the close button", trade::BTN_CLOSE.0),
    ];

    let (found, total) = {
        let shell = c.view().expect_app().ui().expect("the UI shell");
        let root = shell.flow.current().expect("a screen is up").roots()[0];
        let mut all = Vec::new();
        collect(&shell.ui, root, &mut all);
        let n = all.len();
        let mut found: std::collections::BTreeMap<u32, Vec<u32>> =
            std::collections::BTreeMap::new();
        for h in all {
            let Some(node) = shell.ui.node(h) else {
                continue;
            };
            let id = node.element_id().0;
            if wanted.iter().any(|(_, w)| *w == id) || id == LIVE_CONTROL || id == ABSENT_CONTROL {
                found.entry(id).or_default().push(node.ty().0);
            }
        }
        (found, n)
    };

    // The instrument can see, and it can also fail to see.
    assert!(
        total > 200,
        "only {total} elements were walked -- the tree did not build"
    );
    assert!(
        found.contains_key(&LIVE_CONTROL),
        "the positive control is missing"
    );
    assert!(
        !found.contains_key(&ABSENT_CONTROL),
        "the negative control is present"
    );

    // …and the panel really does bind to them, which is the join that matters.
    let bound = {
        let p = bound_trade(c.app_mut());
        p.other_name.is_some()
            && p.other_status.is_some()
            && p.other_total.is_some()
            && p.other_list.is_some()
            && p.self_name.is_some()
            && p.self_total.is_some()
            && p.self_list.is_some()
            && p.trade_button.is_some()
            && p.clear_button.is_some()
            && p.root.is_some()
    };

    c.assert_behaviour(
        "trade.window.every-element-the-panel-binds-to-is-in-the-shipped-tree",
        {
            move |_| {
                let each_once = wanted
                    .iter()
                    .all(|(_, id)| found.get(id).is_some_and(|v| v.len() == 1));
                // The two item lists are item lists, which is what makes them bindable at all.
                let lists_are_lists = found[&trade::SELF_LIST.0][0]
                    == found[&trade::OTHER_LIST.0][0]
                    && found[&trade::SELF_LIST.0][0] != found[&trade::BTN_TRADE.0][0];
                // …and the three buttons are buttons.
                let buttons_agree = found[&trade::BTN_TRADE.0][0]
                    == found[&trade::BTN_CLEAR_ALL.0][0]
                    && found[&trade::BTN_TRADE.0][0] == found[&trade::BTN_CLOSE.0][0];
                each_once && lists_are_lists && buttons_agree && bound
            }
        },
    );
    c.shutdown();
}

fn collect(ui: &UiSystem, h: ElemHandle, out: &mut Vec<ElemHandle>) {
    out.push(h);
    for child in ui.children(h) {
        collect(ui, child, out);
    }
}

// ---------------------------------------------------------------------------------------------
// trade.table.the-optimistic-row-is-drawn-before-the-shard-answers-and-is-not-doubled-by-it
// ---------------------------------------------------------------------------------------------

/// A drop is drawn at once, the shard's answer replaces it rather than doubling it, and dropping
/// the same thing twice does nothing.
///
/// The reply is deliberately withheld for the first half: a scenario that let it arrive could not
/// tell the optimistic row from the confirmed one and would pass on a client that drew neither
/// until the shard spoke.
pub(super) fn a_drop_is_drawn_before_the_shard_answers() {
    let mut c = a_gameplay_client();
    let mut p = bound_trade(c.app_mut());
    let (ui, root) = gameplay_root(c.app_mut());

    let open = open_with("Alba")
        .with(
            CONFIRMED,
            obj("Sturdy Iron Key", "Sturdy Iron Keys", KEY_ICON, 1),
        )
        .with(REFUSED, obj("Arrow", "Arrows", ARROW_ICON, 1));
    assert!(p.update(ui, &open), "the window opens");
    assert!(
        drawn_icons(ui, p.self_list.as_ref().expect("your list")).is_empty(),
        "the premise: your side of the table starts empty"
    );
    let empty_label = element_text(ui, root, trade::SELF_TOTAL);

    clear_requests(&mut ui.requests);
    assert!(
        p.drop_item(&mut ui.requests, CONFIRMED),
        "the table takes an item that is not already on it"
    );
    let went_out = requests_len(&mut ui.requests) == 1;
    clear_requests(&mut ui.requests);
    assert!(p.drop_item(&mut ui.requests, REFUSED));
    clear_requests(&mut ui.requests);

    // **Nothing has come back.** The identical view is offered again, which is exactly the frame
    // the client produces: a drop, then a redraw against an untouched snapshot.
    assert!(
        p.update(ui, &open),
        "the panel's own insert defeats the redraw guard"
    );
    let drawn_at_once = drawn_icons(ui, p.self_list.as_ref().expect("your list"))
        == vec![Some(KEY_ICON), Some(ARROW_ICON)]
        && p.rows(false)[0].name == "Sturdy Iron Key"
        && p.pending == vec![CONFIRMED, REFUSED];
    let label_moved = element_text(ui, root, trade::SELF_TOTAL) != empty_label;
    let button_woke = element_state(ui, root, trade::BTN_TRADE) == ButtonState::Enabled as u32;
    let partner_untouched =
        drawn_icons(ui, p.other_list.as_ref().expect("the partner's list")).is_empty();

    // The shard answers about **one** of them, and under a fuller name.
    let mut confirmed = open.clone();
    confirmed.trade.self_rows = vec![TradeRow {
        item: CONFIRMED,
        name: "Sturdy Iron Key of Marks".into(),
        icon: Some(KEY_ICON),
    }];
    assert!(p.update(ui, &confirmed));
    let not_doubled = p.displayed_self == vec![CONFIRMED, REFUSED]
        && p.pending == vec![REFUSED]
        && p.rows(false)[0].name == "Sturdy Iron Key of Marks"
        && drawn_icons(ui, p.self_list.as_ref().expect("your list")).len() == 2;

    // …and a second drop of something already on the table adds nothing and sends nothing.
    clear_requests(&mut ui.requests);
    let second_refused = !p.drop_item(&mut ui.requests, REFUSED)
        && requests_len(&mut ui.requests) == 0
        && p.drag_accept_state(REFUSED, &confirmed) == trade::DRAG_REFUSE
        && !p.update(ui, &confirmed)
        && drawn_icons(ui, p.self_list.as_ref().expect("your list")).len() == 2;
    clear_requests(&mut ui.requests);

    c.assert_behaviour(
        "trade.table.the-optimistic-row-is-drawn-before-the-shard-answers-and-is-not-doubled-by-it",
        move |_| {
            went_out
                && drawn_at_once
                && label_moved
                && button_woke
                && partner_untouched
                && not_doubled
                && second_refused
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// trade.window.the-partners-name-follows-the-partner-and-a-close-blanks-it
// ---------------------------------------------------------------------------------------------

/// The name at the top of the window is the partner's, nothing else moves it, and a close blanks
/// it rather than leaving it up for the next negotiation to be mistaken for.
pub(super) fn the_partner_name_follows_the_partner_alone() {
    let mut c = a_gameplay_client();
    let mut p = bound_trade(c.app_mut());
    let (ui, root) = gameplay_root(c.app_mut());

    assert!(p.update(ui, &open_with("Alba")));
    let first = element_text(ui, root, trade::OTHER_NAME);

    assert!(p.update(ui, &open_with("Aldis")));
    let second = element_text(ui, root, trade::OTHER_NAME);

    // A field that is not the name moves, and the name element does not follow it.
    let mut other = open_with("Aldis");
    other.trade.partner_accepted = true;
    assert!(p.update(ui, &other));
    let unmoved = element_text(ui, root, trade::OTHER_NAME);

    assert!(p.update(ui, &Fixture::default()));
    let blanked = element_text(ui, root, trade::OTHER_NAME);

    c.assert_behaviour(
        "trade.window.the-partners-name-follows-the-partner-and-a-close-blanks-it",
        {
            move |_| {
                first == "Alba" && second == "Aldis" && unmoved == "Aldis" && blanked.is_empty()
            }
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// trade.window.the-partners-light-and-your-own-button-are-two-different-things

// ---------------------------------------------------------------------------------------------
// trade.window.the-partners-light-and-your-own-button-are-two-different-things
// ---------------------------------------------------------------------------------------------

/// The partner's light and your own button are two things with two sources, and each is moved
/// with the other held still, in both directions.
pub(super) fn the_light_and_the_button_are_two_different_things() {
    let mut c = a_gameplay_client();
    let mut p = bound_trade(c.app_mut());
    let (ui, root) = gameplay_root(c.app_mut());

    // The button is a toggling one in the shipped layout, so agreeing writes an attribute and
    // the element's own state settles on a second pass. Both halves are read, because either
    // alone would pass with the other broken.
    let toggling = element_attr_bool(
        ui,
        root,
        trade::BTN_TRADE,
        dereth_ui::props::attr::TOGGLE_BUTTON,
    )
    .unwrap_or(false);
    assert!(
        toggling,
        "the premise: the shipped trade button is a toggling one"
    );
    let starts_disabled = element_state(ui, root, trade::BTN_TRADE) == ButtonState::Disabled as u32;

    // One row on your side, so the button is not held disabled by an empty table.
    let mut base = open_with("Alba").with(CONFIRMED, obj("Key", "Keys", KEY_ICON, 1));
    base.trade.self_rows = vec![TradeRow {
        item: CONFIRMED,
        name: "Key".into(),
        icon: Some(KEY_ICON),
    }];
    assert!(p.update(ui, &base));
    let neither = element_state(ui, root, trade::OTHER_STATUS) == status_state::NOT_ACCEPTED
        && p.button == ButtonState::Enabled;

    // **The partner agrees, and only the partner.**
    let mut theirs = base.clone();
    theirs.trade.partner_accepted = true;
    assert!(p.update(ui, &theirs));
    let theirs_only = element_state(ui, root, trade::OTHER_STATUS) == status_state::ACCEPTED
        && p.button == ButtonState::Enabled
        && !toggled(ui, root)
        && element_state(ui, root, trade::BTN_TRADE) == ButtonState::Enabled as u32;

    // **You agree, and only you.**
    let mut mine = base.clone();
    mine.trade.accepted = true;
    assert!(p.update(ui, &mine));
    let mine_only = p.button == ButtonState::Accepted
        && element_state(ui, root, trade::OTHER_STATUS) == status_state::NOT_ACCEPTED
        && toggled(ui, root)
        && element_state(ui, root, trade::BTN_TRADE) == ButtonState::Accepted as u32;

    // …and all the way back, so both writes are live edges rather than values the layout shipped.
    let mut off = base.clone();
    off.trade.accepted = false;
    assert!(p.update(ui, &off));
    let and_back = p.button == ButtonState::Enabled
        && !toggled(ui, root)
        && element_state(ui, root, trade::BTN_TRADE) == ButtonState::Enabled as u32;

    // And an empty table puts the button out of reach again.
    let mut empty = base.clone();
    empty.trade.self_rows.clear();
    assert!(p.update(ui, &empty));
    let disabled_again = p.button == ButtonState::Disabled
        && element_state(ui, root, trade::BTN_TRADE) == ButtonState::Disabled as u32;

    c.assert_behaviour(
        "trade.window.the-partners-light-and-your-own-button-are-two-different-things",
        move |_| {
            starts_disabled && neither && theirs_only && mine_only && and_back && disabled_again
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// trade.window.each-sides-total-counts-only-its-own-side
// ---------------------------------------------------------------------------------------------

/// The two counts at the foot of the window count two different lists.
pub(super) fn each_sides_total_counts_only_its_own_side() {
    let mut c = a_gameplay_client();
    let mut p = bound_trade(c.app_mut());
    let (ui, root) = gameplay_root(c.app_mut());

    let base = open_with("Alba")
        .with(CONFIRMED, obj("Key", "Keys", KEY_ICON, 1))
        .with(REFUSED, obj("Arrow", "Arrows", ARROW_ICON, 1));
    assert!(p.update(ui, &base));
    let zero_self = element_text(ui, root, trade::SELF_TOTAL);
    let zero_other = element_text(ui, root, trade::OTHER_TOTAL);
    assert!(
        !zero_self.is_empty(),
        "the premise: something was written at all"
    );

    // Your side alone.
    let mut mine = base.clone();
    mine.trade.self_rows = vec![TradeRow {
        item: CONFIRMED,
        name: "Key".into(),
        icon: Some(KEY_ICON),
    }];
    assert!(p.update(ui, &mine));
    let one_self = element_text(ui, root, trade::SELF_TOTAL);
    let other_held = element_text(ui, root, trade::OTHER_TOTAL) == zero_other;

    // The partner's side alone, with yours empty again.
    let mut theirs = base.clone();
    theirs.trade.partner_rows = vec![
        TradeRow {
            item: REFUSED,
            name: "Arrow".into(),
            icon: Some(ARROW_ICON),
        },
        TradeRow {
            item: ObjectId(0x9000_0002),
            name: "Arrow".into(),
            icon: None,
        },
    ];
    assert!(p.update(ui, &theirs));
    let self_back = element_text(ui, root, trade::SELF_TOTAL) == zero_self;
    let two_other = element_text(ui, root, trade::OTHER_TOTAL);

    // And an item the player has only just dropped counts too, because the count is of what is
    // on screen rather than of what the shard has agreed to.
    assert!(p.update(ui, &base));
    clear_requests(&mut ui.requests);
    assert!(p.drop_item(&mut ui.requests, REFUSED));
    clear_requests(&mut ui.requests);
    assert!(p.update(ui, &base));
    let optimistic_counts = element_text(ui, root, trade::SELF_TOTAL) == one_self;

    c.assert_behaviour(
        "trade.window.each-sides-total-counts-only-its-own-side",
        move |_| {
            zero_self == zero_other
                && one_self != zero_self
                && other_held
                && self_back
                && two_other != zero_other
                && two_other != one_self
                && optimistic_counts
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// trade.window.your-own-name-is-bound-and-never-written
// ---------------------------------------------------------------------------------------------

/// Your own name is a place in the window the client deliberately never writes.
///
/// This is a declared non-write and not a pinned defect: it goes red if a later change starts
/// writing it, which is the point -- that change is a deviation from the client being rebuilt and
/// needs its own justification rather than arriving as a rider on a wiring row.
pub(super) fn your_own_name_is_bound_and_never_written() {
    let mut c = a_gameplay_client();
    let mut p = bound_trade(c.app_mut());
    let (ui, root) = gameplay_root(c.app_mut());

    let before = element_text(ui, root, trade::SELF_NAME);

    // Drive the window through everything that writes any other element.
    let mut v = open_with("Alba")
        .with(CONFIRMED, obj("Key", "Keys", KEY_ICON, 1))
        .with(REFUSED, obj("Arrow", "Arrows", ARROW_ICON, 1));
    v.trade.self_rows = vec![TradeRow {
        item: REFUSED,
        name: "Arrow".into(),
        icon: Some(ARROW_ICON),
    }];
    v.trade.partner_rows = vec![TradeRow {
        item: ObjectId(0x9000_0002),
        name: "Pyreal".into(),
        icon: None,
    }];
    v.trade.accepted = true;
    v.trade.partner_accepted = true;
    assert!(p.update(ui, &v));
    clear_requests(&mut ui.requests);
    assert!(p.drop_item(&mut ui.requests, CONFIRMED));
    clear_requests(&mut ui.requests);
    assert!(p.update(ui, &v));

    // The premise: the drive really did write the other elements.
    assert_eq!(element_text(ui, root, trade::OTHER_NAME), "Alba");
    assert_eq!(
        element_state(ui, root, trade::OTHER_STATUS),
        status_state::ACCEPTED
    );
    assert_ne!(element_text(ui, root, trade::SELF_TOTAL), "");

    let after_everything = element_text(ui, root, trade::SELF_NAME);
    assert!(p.update(ui, &Fixture::default()));
    let after_close = element_text(ui, root, trade::SELF_NAME);

    c.assert_behaviour(
        "trade.window.your-own-name-is-bound-and-never-written",
        move |_| after_everything == before && after_close == before,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// The removal family: what takes a row back off the table, and what does not.
// ---------------------------------------------------------------------------------------------

/// The shard's messages, delivered the way the client receives them.
fn open_accepted_trade(c: &mut HeadlessClient) {
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeRegisterTrade {
            initiator: ME,
            partner: PARTNER,
            stamp: 4.25,
        },
    ))
    .when(Inbound::message(&dereth_protocol::trade::TradeOpenTrade {
        source: PARTNER,
    }))
    .when(Inbound::message(
        &dereth_protocol::trade::TradeAddToTradeRecv {
            item: CONFIRMED,
            side: 1,
            container_properties: 0,
        },
    ))
    .when(Inbound::message(
        &dereth_protocol::trade::TradeAcceptTradeRecv { source: ME },
    ))
    .when(Inbound::message(
        &dereth_protocol::trade::TradeAcceptTradeRecv { source: PARTNER },
    ));
    let t = &c.view().world().trade.trade;
    assert!(
        t.accepted && t.partner_accepted,
        "the premise: both sides have agreed"
    );
    assert_eq!(
        t.self_list.len(),
        1,
        "the premise: one confirmed row on your side"
    );
}

/// Put the player and the two items into the client's own world.
fn a_player_with_two_things(c: &mut HeadlessClient) {
    let w = c.world_mut();
    w.player = Some(ME);
    for (id, name) in [(CONFIRMED, "Sturdy Iron Key"), (REFUSED, "Prismatic Taper")] {
        let mut o = dereth_client_model::Weenie::new(id);
        o.pwd.name = name.into();
        o.pwd.icon_id = KEY_ICON.0;
        // Both are in the player's pack, so the table will take them.
        o.pwd.container_id = Some(ME);
        o.valid = true;
        w.tables.weenies.insert(id, o);
    }
    let mut partner = dereth_client_model::Weenie::new(PARTNER);
    partner.pwd.name = "Alba".into();
    partner.valid = true;
    w.tables.weenies.insert(PARTNER, partner);
}

/// The client's own trade snapshot and object records, taken as an owned fixture so that the
/// panel can be driven with it while the UI system is borrowed.
fn snapshot(c: &HeadlessClient) -> Fixture {
    let w = c.view().world();
    let mut f = Fixture {
        trade: dereth_client::trade_view::trade(w),
        ..Fixture::default()
    };
    for id in [CONFIRMED, REFUSED, PARTNER] {
        if let Some(o) = w.weenie(id) {
            f.objects.insert(
                id,
                Obj {
                    name: o.pwd.name.clone(),
                    plural: o
                        .pwd
                        .plural_name
                        .clone()
                        .unwrap_or_else(|| o.pwd.name.clone()),
                    decoration: SlotDecoration {
                        icon_id: o.pwd.icon_id,
                        stack_size: u32::from(o.pwd.stack_size.unwrap_or(0)).max(1),
                        trade_state: o.trade_state != 0,
                        ..SlotDecoration::default()
                    },
                },
            );
        }
    }
    f
}

/// The optimistic row is on screen and both indicators are showing agreement -- the control that
/// makes every removal assertion below a change rather than a value the panel writes anyway.
fn drop_and_draw(c: &mut HeadlessClient, p: &mut TradePanel) {
    let f = snapshot(c);
    let (ui, root) = gameplay_root(c.app_mut());
    assert!(p.update(ui, &f), "the window opens with the confirmed row");
    clear_requests(&mut ui.requests);
    assert!(
        p.drop_item(&mut ui.requests, REFUSED),
        "the table takes an item that is not already on it"
    );
    clear_requests(&mut ui.requests);
    assert!(
        p.update(ui, &f),
        "the optimistic insert defeats the redraw guard"
    );

    assert_eq!(
        p.rows(false).iter().map(|r| r.item).collect::<Vec<_>>(),
        vec![CONFIRMED, REFUSED],
        "the control: both rows are on your side"
    );
    assert_eq!(p.pending, vec![REFUSED], "…and one of them is unconfirmed");
    assert_eq!(
        element_state(ui, root, trade::BTN_TRADE),
        ButtonState::Accepted as u32,
        "the control: the button is showing agreement, so a later change is a change"
    );
    assert_eq!(
        element_state(ui, root, trade::OTHER_STATUS),
        status_state::ACCEPTED,
        "the control: the light is showing agreement"
    );
}

/// What the window looks like once a removal has been offered to it.
fn redraw(c: &mut HeadlessClient, p: &mut TradePanel) -> (Vec<ObjectId>, bool, u32, u32) {
    let f = snapshot(c);
    let (ui, root) = gameplay_root(c.app_mut());
    p.update(ui, &f);
    (
        p.rows(false).iter().map(|r| r.item).collect(),
        p.pending.is_empty(),
        element_state(ui, root, trade::BTN_TRADE),
        element_state(ui, root, trade::OTHER_STATUS),
    )
}

// ---------------------------------------------------------------------------------------------
// trade.removal.the-shards-failure-takes-the-optimistic-row-off-the-table
// ---------------------------------------------------------------------------------------------

/// The shard refusing an item the player has already been shown takes the row off, and puts both
/// indicators back.
///
/// This is the arm that matters most: the refused item was never in the client's own record of
/// the trade, so without this nothing in the client would ever mention it again and the player
/// would believe an item was committed when it was not.
pub(super) fn a_shard_failure_takes_the_optimistic_row_off() {
    let mut c = a_gameplay_client();
    let mut p = bound_trade(c.app_mut());
    a_player_with_two_things(&mut c);
    open_accepted_trade(&mut c);
    drop_and_draw(&mut c, &mut p);

    assert!(
        !c.view()
            .world()
            .trade
            .trade
            .self_list
            .iter()
            .any(|r| r.iid == REFUSED),
        "the premise: the optimistic row exists only in the window"
    );

    c.when(Inbound::message(
        &dereth_protocol::trade::TradeTradeFailure {
            item: REFUSED,
            reason: 9,
        },
    ));
    let carried = dereth_client::trade_view::trade(c.view().world()).self_removed == vec![REFUSED];
    let (rows, cleared, button, light) = redraw(&mut c, &mut p);

    c.assert_behaviour(
        "trade.removal.the-shards-failure-takes-the-optimistic-row-off-the-table",
        {
            move |_| {
                carried
                    && rows == vec![CONFIRMED]
                    && cleared
                    && button == ButtonState::Enabled as u32
                    && light == status_state::NOT_ACCEPTED
            }
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// trade.removal.the-shard-withdrawing-your-row-takes-it-off-and-the-partners-does-not
// ---------------------------------------------------------------------------------------------

/// A withdrawal names a side, and the client believes it.
pub(super) fn a_withdrawal_names_a_side() {
    let mut c = a_gameplay_client();
    let mut p = bound_trade(c.app_mut());
    a_player_with_two_things(&mut c);
    open_accepted_trade(&mut c);
    drop_and_draw(&mut c, &mut p);

    // The partner's side first: your own optimistic row must survive it.
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeRemoveFromTrade {
            item: REFUSED,
            side: 2,
        },
    ));
    let theirs_carried = dereth_client::trade_view::trade(c.view().world())
        .self_removed
        .is_empty();
    let (rows_after_theirs, ..) = redraw(&mut c, &mut p);

    // …and then your own.
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeRemoveFromTrade {
            item: REFUSED,
            side: 1,
        },
    ));
    let yours_carried =
        dereth_client::trade_view::trade(c.view().world()).self_removed == vec![REFUSED];
    let (rows, cleared, button, light) = redraw(&mut c, &mut p);

    c.assert_behaviour(
        "trade.removal.the-shard-withdrawing-your-row-takes-it-off-and-the-partners-does-not",
        move |_| {
            theirs_carried
                && rows_after_theirs == vec![CONFIRMED, REFUSED]
                && yours_carried
                && rows == vec![CONFIRMED]
                && cleared
                && button == ButtonState::Enabled as u32
                && light == status_state::NOT_ACCEPTED
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// trade.removal.a-plain-inventory-refusal-leaves-the-row-where-it-is
// ---------------------------------------------------------------------------------------------

/// An ordinary inventory refusal is not a trade refusal, and the row stays.
///
/// A negative assertion needs the subject alive enough to have failed, so the same scenario then
/// delivers the message that **is** a producer and requires the row to go. Apart, the first half
/// is a frozen window scoring as faithful.
pub(super) fn a_plain_inventory_refusal_leaves_the_row() {
    let mut c = a_gameplay_client();
    let mut p = bound_trade(c.app_mut());
    a_player_with_two_things(&mut c);
    open_accepted_trade(&mut c);
    drop_and_draw(&mut c, &mut p);

    c.when(Inbound::message(
        &dereth_protocol::objects::CharacterServerSaysAttemptFailed {
            object: REFUSED,
            reason: 0x01E,
        },
    ));
    let not_carried = dereth_client::trade_view::trade(c.view().world())
        .self_removed
        .is_empty();
    let (rows_after, ..) = redraw(&mut c, &mut p);

    // …and the subject is alive: the message that is a producer does take it off.
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeTradeFailure {
            item: REFUSED,
            reason: 9,
        },
    ));
    let (rows, cleared, button, light) = redraw(&mut c, &mut p);

    c.assert_behaviour(
        "trade.removal.a-plain-inventory-refusal-leaves-the-row-where-it-is",
        {
            move |_| {
                not_carried
                    && rows_after == vec![CONFIRMED, REFUSED]
                    && rows == vec![CONFIRMED]
                    && cleared
                    && button == ButtonState::Enabled as u32
                    && light == status_state::NOT_ACCEPTED
            }
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// trade.table.an-item-put-on-the-table-wears-the-traded-mark-on-its-slot

// ---------------------------------------------------------------------------------------------
// trade.table.an-item-put-on-the-table-wears-the-traded-mark-on-its-slot
// ---------------------------------------------------------------------------------------------

/// Putting an item on the table marks the item, and the mark is drawn on the slot.
///
/// Both halves are asserted, because a client that recorded the mark and drew nothing would pass
/// the first alone -- and that is exactly the state this chain found.
pub(super) fn an_item_on_the_table_wears_the_traded_mark() {
    let mut c = a_gameplay_client();
    let mut p = bound_trade(c.app_mut());
    a_player_with_two_things(&mut c);
    open_accepted_trade(&mut c);

    {
        let f = snapshot(&c);
        let (ui, _root) = gameplay_root(c.app_mut());
        assert!(p.update(ui, &f), "the window opens with the confirmed row");
    }
    assert_eq!(
        c.view().world().weenie(REFUSED).map(|o| o.trade_state),
        Some(0),
        "the premise: the item is not marked yet"
    );

    clear_requests(c.ui_outbox());
    assert!(p.drop_item(c.ui_outbox(), REFUSED));
    // The request really travels: the interaction layer is the production consumer.
    let raised = take_requests(c.ui_outbox());
    assert_eq!(raised.len(), 1, "one request left the panel");
    // The `App` backend queues a UI request and runs it on its own next frame, so the gesture
    // needs a frame to reach the production consumer -- which is the point of putting it through
    // one rather than calling the arm.
    c.when(Player::Ui(raised)).tick(1);
    let marked = c.view().world().weenie(REFUSED).map(|o| o.trade_state) == Some(1);

    let f = snapshot(&c);
    let (ui, _root) = gameplay_root(c.app_mut());
    assert!(
        p.update(ui, &f),
        "the mark moving is a change the redraw guard sees"
    );
    let slot = p
        .self_list
        .as_ref()
        .expect("your list")
        .slots
        .iter()
        .find(|s| s.item == Some(REFUSED))
        .expect("the optimistic row is filled");
    let on_the_slot = slot.trade_state;
    let on_the_screen = slot
        .trade_state_elem
        .and_then(|h| ui.node(h))
        .map(|n| n.region.flags.visible)
        == Some(true);

    c.assert_behaviour(
        "trade.table.an-item-put-on-the-table-wears-the-traded-mark-on-its-slot",
        { move |_| marked && on_the_slot && on_the_screen },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// trade.table.a-stack-tells-you-how-many-there-are-and-what-they-are-called
// ---------------------------------------------------------------------------------------------

/// A stack on the table says how many and what they are called, and one of them says neither.
pub(super) fn a_stack_on_the_trade_table_is_counted_and_pluralised() {
    let mut c = a_gameplay_client();
    let mut p = bound_trade(c.app_mut());
    let (ui, _root) = gameplay_root(c.app_mut());

    assert!(
        p.update(ui, &trade_with_stack(250)),
        "the first drive is always a redraw"
    );
    let many = {
        let list = p.other_list.as_ref().expect("the partner's list");
        assert_eq!(
            tooltips(list).len(),
            1,
            "the premise: exactly one row is filled"
        );
        assert_eq!(
            p.slots_decorated, 1,
            "the premise: the decoration pass ran over it"
        );
        (tooltips(list), quantities(list), drawn_icons(ui, list))
    };

    assert!(p.update(ui, &trade_with_stack(1)));
    let one = tooltips(p.other_list.as_ref().expect("the partner's list"));

    c.assert_behaviour(
        "trade.table.a-stack-tells-you-how-many-there-are-and-what-they-are-called",
        {
            move |_| {
                many.0 == vec![Some("250 Arrows".to_owned())]
                // …and no numeral is painted over it, because on this table none is.
                && many.1 == vec![-1]
                && many.2 == vec![Some(ARROW_ICON)]
                && one == vec![Some("Arrow".to_owned())]
            }
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// trade.table.the-tooltip-reaches-the-slot-itself-and-not-only-the-panels-record
// ---------------------------------------------------------------------------------------------

/// The text is on the element the pointer will hover over, and the element is told it has
/// something to say. Either alone shows nothing on hover.
pub(super) fn the_tooltip_reaches_the_slot_itself() {
    let mut c = a_gameplay_client();
    let mut p = bound_trade(c.app_mut());
    let (ui, _root) = gameplay_root(c.app_mut());

    assert!(p.update(ui, &trade_with_stack(250)));
    let h = p
        .other_list
        .as_ref()
        .expect("the partner's list")
        .slots
        .iter()
        .find(|s| s.item == Some(REFUSED))
        .expect("the row is filled")
        .handle;
    let text = ui.node(h).and_then(|n| n.tooltip_text.clone());
    let on = ui.node(h).and_then(|n| {
        n.merged_properties()
            .get_bool(dereth_ui::props::attr::TOOLTIP_ON)
    });

    c.assert_behaviour(
        "trade.table.the-tooltip-reaches-the-slot-itself-and-not-only-the-panels-record",
        { move |_| text == Some("250 Arrows".to_owned()) && on == Some(true) },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// vendor.stock.a-stack-tells-you-how-many-there-are-and-what-they-are-called
// ---------------------------------------------------------------------------------------------

/// The same, on a shop's stock -- asserted separately, because two panels asserted in aggregate
/// would pass with one of them dead.
pub(super) fn a_stack_in_vendor_stock_is_counted_and_pluralised() {
    let mut c = a_gameplay_client();
    let mut p = bound_vendor(c.app_mut());
    let (ui, _root) = gameplay_root(c.app_mut());

    assert!(
        p.update(ui, &shop_with_stack(250, -1)),
        "the first drive is always a redraw"
    );
    let stock = p.stock.as_ref().expect("the stock list");
    assert_eq!(
        tooltips(stock).len(),
        1,
        "the premise: exactly one stock row is filled"
    );
    assert_eq!(
        p.slots_decorated, 1,
        "the premise: the decoration pass ran over it"
    );
    let tips = tooltips(stock);
    let icons = drawn_icons(ui, stock);
    let rows = p.rows(Tab::Items).len();

    c.assert_behaviour(
        "vendor.stock.a-stack-tells-you-how-many-there-are-and-what-they-are-called",
        {
            move |_| {
                tips == vec![Some("250 Arrows".to_owned())]
                    && icons == vec![Some(ARROW_ICON)]
                    && rows == 1
            }
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// vendor.stock.the-numeral-is-the-amount-the-shop-advertises-on-the-row-the-player-picked
// ---------------------------------------------------------------------------------------------

/// The numeral is the shop's advertised amount, on the picked row only, and it hides for a shop
/// that is out of the thing as well as for one with an endless supply.
///
/// The stack is 250 in every arm, so the number that appears is shown to be the amount and not
/// the stack. Zero is the discriminating value: for an endless supply and for a real count the
/// comparison changes nothing, so an arm using only those two cannot see it at all.
pub(super) fn the_vendor_numeral_is_the_advertised_amount() {
    let mut c = a_gameplay_client();
    let mut p = bound_vendor(c.app_mut());
    let (ui, _root) = gameplay_root(c.app_mut());

    // 1. Nothing picked: the pass runs and writes nothing.
    let none = shop_with_stack(250, 7);
    assert!(p.update(ui, &none));
    let unpicked = p.quantity_overlays == 0
        && quantities(p.stock.as_ref().expect("the stock list")) == vec![-1];
    assert!(
        !p.update(ui, &none),
        "the premise: an identical frame is held off"
    );

    // 2. **The picked row alone.** Same shop, same stack, same everything else.
    let mut picked = shop_with_stack(250, 7);
    picked.selected = Some(REFUSED);
    assert_eq!(
        picked.shop, none.shop,
        "the premise: only the selection moved"
    );
    assert!(
        p.update(ui, &picked),
        "a selection change alone defeats the redraw guard"
    );
    let shown = p.quantity_overlays == 1
        && quantities(p.stock.as_ref().expect("the stock list")) == vec![7]
        // …and the tooltip is still the stack's, so the two numbers are two numbers.
        && tooltips(p.stock.as_ref().expect("the stock list"))
            == vec![Some("250 Arrows".to_owned())];

    // 3. Out of stock.
    let mut out = shop_with_stack(250, 0);
    out.selected = Some(REFUSED);
    assert!(p.update(ui, &out));
    let hidden_at_zero = p.quantity_overlays == 1
        && quantities(p.stock.as_ref().expect("the stock list")) == vec![-1];

    // 4. An endless supply.
    let mut unlimited = shop_with_stack(250, -1);
    unlimited.selected = Some(REFUSED);
    assert!(p.update(ui, &unlimited));
    let hidden_when_endless = p.quantity_overlays == 1
        && quantities(p.stock.as_ref().expect("the stock list")) == vec![-1];

    c.assert_behaviour(
        "vendor.stock.the-numeral-is-the-amount-the-shop-advertises-on-the-row-the-player-picked",
        move |_| unpicked && shown && hidden_at_zero && hidden_when_endless,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// The two redraw guards, one field at a time.
// ---------------------------------------------------------------------------------------------

/// Every frozen fact about an item moves the trade table, and the clock does not.
pub(super) fn the_trade_table_follows_every_frozen_fact_and_not_the_clock() {
    let mut c = a_gameplay_client();
    let mut p = bound_trade(c.app_mut());
    let (ui, _root) = gameplay_root(c.app_mut());

    let base = trade_with_stack(250);
    assert!(p.update(ui, &base), "the first drive is always a redraw");
    assert!(
        !p.update(ui, &base),
        "the premise: an identical frame is held off"
    );

    let each_field = {
        let p = &mut p;
        let mut rebuilds = 0;
        let mut ok = true;
        let mut n = p.rebuilds;
        let mut v = base.clone();
        for (why, f) in [
            ("how many there are", 0u8),
            ("the overlay", 1),
            ("the wear", 2),
            ("the name", 3),
            ("the plural name", 4),
        ] {
            v = v.clone_with(REFUSED, |o| match f {
                0 => o.decoration.stack_size = 249,
                1 => o.decoration.icon_overlay_id = Some(DataId(0x0600_2222)),
                2 => {
                    o.decoration.structure = 40;
                    o.decoration.max_structure = 100;
                }
                3 => o.name = "Deadly Arrow".into(),
                _ => o.plural = "Deadly Arrows".into(),
            });
            assert_eq!(
                v.trade, base.trade,
                "the snapshot is identical across the frames: {why}"
            );
            ok &= p.update(ui, &v);
            n += 1;
            ok &= p.rebuilds == n;
            ok &= !p.update(ui, &v);
            ok &= p.rebuilds == n;
            rebuilds += 1;
        }
        ok && rebuilds == 5
            && tooltips(p.other_list.as_ref().expect("the partner's list"))
                == vec![Some("249 Deadly Arrows".to_owned())]
    };

    // **The exemption.** A cooldown takes a new value every frame with no message behind it; if
    // it were in the guard the early-out would fire never, the table would be flushed every
    // frame, and the screen would look perfect.
    let mut live = trade_with_stack(250);
    live.cooldown = Some((77, 1000.0, 30.0));
    live.objects
        .get_mut(&REFUSED)
        .expect("the arrow")
        .decoration
        .cooldown_id = 77;
    live.objects
        .get_mut(&REFUSED)
        .expect("the arrow")
        .decoration
        .cooldown_duration = 30.0;
    ui.now = dereth_primitives::LocalTime(1000.0);
    assert!(p.update(ui, &live), "the first drive");
    let before = live
        .cooldown_remaining(77, ui.now.0)
        .expect("a live cooldown");
    ui.now = dereth_primitives::LocalTime(1010.0);
    let after = live.cooldown_remaining(77, ui.now.0).expect("still live");
    assert!(
        (before - after - 10.0).abs() < 1e-9,
        "the premise: the value really moved"
    );
    let n = p.rebuilds;
    let clock_exempt = !p.update(ui, &live) && p.rebuilds == n;
    // …and the frozen half is still live over the same two frames, so the exemption is an
    // exemption rather than a dead guard.
    let still_live = p.update(
        ui,
        &live.clone_with(REFUSED, |o| o.decoration.stack_size = 248),
    );

    c.assert_behaviour(
        "trade.table.what-is-drawn-follows-every-frozen-fact-about-an-item-and-not-the-clock",
        move |_| each_field && clock_exempt && still_live,
    );
    c.shutdown();
}

/// The same, on the shop's stock list. Asserted separately, because one guard fixed and one left
/// is exactly the state an audit of six panels found.
pub(super) fn the_vendor_stock_follows_every_frozen_fact_and_not_the_clock() {
    let mut c = a_gameplay_client();
    let mut p = bound_vendor(c.app_mut());
    let (ui, _root) = gameplay_root(c.app_mut());

    let base = shop_with_stack(250, -1);
    assert!(p.update(ui, &base), "the first drive is always a redraw");
    assert!(
        !p.update(ui, &base),
        "the premise: an identical frame is held off"
    );

    let each_field = {
        let p = &mut p;
        let mut ok = true;
        let mut n = p.rebuilds;
        let mut v = base.clone();
        for (why, f) in [
            ("how many there are", 0u8),
            ("the overlay", 1),
            ("the wear", 2),
            ("the name", 3),
            ("the plural name", 4),
        ] {
            v = v.clone_with(REFUSED, |o| match f {
                0 => o.decoration.stack_size = 249,
                1 => o.decoration.icon_overlay_id = Some(DataId(0x0600_2222)),
                2 => {
                    o.decoration.structure = 40;
                    o.decoration.max_structure = 100;
                }
                3 => o.name = "Deadly Arrow".into(),
                _ => o.plural = "Deadly Arrows".into(),
            });
            assert_eq!(
                v.shop, base.shop,
                "the snapshot is identical across the frames: {why}"
            );
            ok &= p.update(ui, &v);
            n += 1;
            ok &= p.rebuilds == n;
            ok &= !p.update(ui, &v);
            ok &= p.rebuilds == n;
        }
        ok && tooltips(p.stock.as_ref().expect("the stock list"))
            == vec![Some("249 Deadly Arrows".to_owned())]
    };

    let mut live = shop_with_stack(250, -1);
    live.cooldown = Some((77, 1000.0, 30.0));
    live.objects
        .get_mut(&REFUSED)
        .expect("the arrow")
        .decoration
        .cooldown_id = 77;
    live.objects
        .get_mut(&REFUSED)
        .expect("the arrow")
        .decoration
        .cooldown_duration = 30.0;
    ui.now = dereth_primitives::LocalTime(1000.0);
    assert!(p.update(ui, &live), "the first drive");
    let before = live
        .cooldown_remaining(77, ui.now.0)
        .expect("a live cooldown");
    ui.now = dereth_primitives::LocalTime(1010.0);
    let after = live.cooldown_remaining(77, ui.now.0).expect("still live");
    assert!(
        (before - after - 10.0).abs() < 1e-9,
        "the premise: the value really moved"
    );
    let n = p.rebuilds;
    let clock_exempt = !p.update(ui, &live) && p.rebuilds == n;
    let still_live = p.update(
        ui,
        &live.clone_with(REFUSED, |o| o.decoration.stack_size = 248),
    );

    c.assert_behaviour(
        "vendor.stock.what-is-drawn-follows-every-frozen-fact-about-an-item-and-not-the-clock",
        move |_| each_field && clock_exempt && still_live,
    );
    c.shutdown();
}

/// The text of one child of one row element.
fn child_text(ui: &mut UiSystem, row: ElemHandle, child: u32) -> Option<String> {
    let h = ui.get_child_recursive(row, ElementId(child))?;
    ui.text_element_mut(h).map(|t| t.glyphs.inq_text(false))
}

// ---------------------------------------------------------------------------------------------
// o487: five panels, one rule.
// ---------------------------------------------------------------------------------------------

/// A shop with one stock row, whose picture and price are the two fields the stations move.
fn shop_row(icon: Option<u32>, price: i32) -> Fixture {
    Fixture {
        shop: ShopView {
            open: true,
            vendor: Some(ObjectId(0x8000_0001)),
            stock: vec![ShopRow {
                item: REFUSED,
                name: "Oil of Rendering".into(),
                icon: icon.map(DataId),
                amount: -1,
                // A miscellaneous thing, with a tab that covers it: without a type and a tab the
                // stock list keeps no rows and there is no slot whose picture could resolve.
                obj_type: 0x0000_0080,
                max_stack_size: 0,
                contained_items: 0,
                contained_containers: 0,
                price,
                refusal: None,
            }],
            type_filters: vec![("Miscellaneous", 0x0000_0490)],
            ..ShopView::default()
        },
        ..Fixture::default()
    }
}

/// A shop's stock row follows its own picture and its own price arriving late.
pub(super) fn a_stock_row_follows_a_late_picture_and_a_new_price() {
    let mut c = a_gameplay_client();
    let mut p = bound_vendor(c.app_mut());
    let (ui, _root) = gameplay_root(c.app_mut());

    let blank = shop_row(None, 5);
    assert!(p.update(ui, &blank), "the first drive is always a redraw");
    let stock = p.stock.as_ref().expect("the stock list");
    assert_eq!(
        drawn_icons(ui, stock).len(),
        1,
        "the premise: one slot is filled"
    );
    let starts_blank = drawn_icons(ui, stock) == vec![None];
    // The gate is live: without this the station passes against a panel with no gate at all.
    let gate_live = !p.update(ui, &blank);

    let lit = shop_row(Some(0x0600_103F), 5);
    assert_eq!(
        lit.shop.stock.iter().map(|r| r.item).collect::<Vec<_>>(),
        blank.shop.stock.iter().map(|r| r.item).collect::<Vec<_>>(),
        "the premise: the list of things on offer is identical across the two frames"
    );
    let picture = p.update(ui, &lit)
        && drawn_icons(ui, p.stock.as_ref().expect("the stock list"))
            == vec![Some(DataId(0x0600_103F))]
        && p.rows(Tab::Items)[0].icon == Some(DataId(0x0600_103F));

    let repriced = shop_row(Some(0x0600_103F), 9);
    let price = p.update(ui, &repriced) && p.rows(Tab::Items)[0].price == 9;

    c.assert_behaviour(
        "panels.redraw.a-shops-stock-follows-a-picture-or-a-price-arriving-under-an-unchanged-row",
        move |_| starts_blank && gate_live && picture && price,
    );
    c.shutdown();
}

/// A partner's trade row that arrives before the thing it names is known.
fn partner_row(name: &str, icon: Option<u32>) -> Fixture {
    let mut f = open_with("Alba");
    f.trade.partner_rows = vec![TradeRow {
        item: REFUSED,
        name: name.to_owned(),
        icon: icon.map(DataId),
    }];
    f
}

/// The partner's side follows a row being filled in later.
pub(super) fn the_partners_side_follows_a_row_filled_in_later() {
    let mut c = a_gameplay_client();
    let mut p = bound_trade(c.app_mut());
    let (ui, _root) = gameplay_root(c.app_mut());

    let before = partner_row("", None);
    assert!(p.update(ui, &before), "the first drive is always a redraw");
    let other = p.other_list.as_ref().expect("the partner's list");
    let starts_bare = drawn_icons(ui, other) == vec![None] && p.rows(true)[0].name.is_empty();
    let gate_live = !p.update(ui, &before);

    let after = partner_row("Oil of Rendering", Some(0x0600_103F));
    assert_eq!(
        after
            .trade
            .partner_rows
            .iter()
            .map(|r| r.item)
            .collect::<Vec<_>>(),
        before
            .trade
            .partner_rows
            .iter()
            .map(|r| r.item)
            .collect::<Vec<_>>(),
        "the premise: the list of what is being offered is identical across the two frames"
    );
    let filled = p.update(ui, &after)
        && drawn_icons(ui, p.other_list.as_ref().expect("the partner's list"))
            == vec![Some(DataId(0x0600_103F))]
        && p.rows(true)[0].name == "Oil of Rendering";

    c.assert_behaviour(
        "panels.redraw.the-partners-side-of-the-trade-follows-a-row-being-filled-in-later",
        move |_| starts_bare && gate_live && filled,
    );
    c.shutdown();
}

#[derive(Debug)]
struct Roster(AllegianceRoster);
impl GameView for Roster {
    fn allegiance_roster(&self) -> AllegianceRoster {
        self.0.clone()
    }
}

fn roster_with(logged_in: bool, cp: u32) -> Roster {
    Roster(AllegianceRoster {
        allegiance_name: "The Hand of Dereth".into(),
        total_members: 6,
        total_vassals: 1,
        own_cp_tithed: 0,
        subject: None,
        player_rank_quality: 0,
        monarch: None,
        patron: None,
        vassals: vec![AllegianceEntry {
            id: ObjectId(10),
            full_name: "Yeoman Dee".into(),
            logged_in,
            rank: 1,
            cp_cached: cp,
        }],
    })
}

/// A vassal logging out and passing up experience redraws that row, under an unchanged roster.
pub(super) fn a_vassals_own_fields_redraw_the_row() {
    let mut c = a_gameplay_client();
    let (ui, root) = gameplay_root(c.app_mut());
    let mut p = AllegiancePanel::default();
    p.post_init(ui, root);
    assert!(p.bound(), "the shipped tree carries the vassal list");
    let list = ui
        .get_child_recursive(root, VASSAL_LIST)
        .expect("the vassal list");

    let before = roster_with(true, 100);
    assert!(p.update(ui, &before), "the first drive is always a redraw");
    assert_eq!(p.rows().len(), 1, "the premise: one row was drawn");
    let starts_online = !p.rows()[0].logged_out_marker && p.vassal_chat_enabled;
    let row = ui.children(list)[0];
    let cp_before = child_text(ui, row, ROW_EXPERIENCE.0).expect("the experience field");
    assert!(
        cp_before.contains("100"),
        "the premise: the element carries the number"
    );
    let gate_live = !p.update(ui, &before);

    let after = roster_with(false, 250);
    assert_eq!(
        after.0.vassals.iter().map(|v| v.id).collect::<Vec<_>>(),
        before.0.vassals.iter().map(|v| v.id).collect::<Vec<_>>(),
        "the premise: the roster names the same people"
    );
    let redrew = p.update(ui, &after) && p.rows()[0].logged_out_marker && !p.vassal_chat_enabled;
    let row = ui.children(list)[0];
    let cp_after = child_text(ui, row, ROW_EXPERIENCE.0).expect("the experience field");

    c.assert_behaviour(
        "panels.redraw.the-vassal-list-follows-a-members-own-fields-changing-under-an-unchanged-id",
        move |_| {
            starts_online
                && gate_live
                && redrew
                && cp_after.contains("250")
                && cp_after != cp_before
        },
    );
    c.shutdown();
}

#[derive(Debug)]
struct Comps(Vec<ComponentCategory>);
impl GameView for Comps {
    fn spell_components(&self) -> Vec<ComponentCategory> {
        self.0.clone()
    }
}

/// Lead Scarab, in the Scarab bucket.
const SCARAB_WCID: u32 = 33835;

fn comps_with(owned: i64, icon: Option<u32>) -> Comps {
    Comps(vec![ComponentCategory {
        category: 0,
        rows: vec![ComponentRow {
            wcid: SCARAB_WCID,
            name: "Lead Scarab".into(),
            icon: icon.map(DataId),
            owned,
            desired: 5,
            object: Some(REFUSED),
        }],
    }])
}

/// Spending three scarabs redraws the count, under an unchanged component list.
pub(super) fn an_owned_component_count_redraws_its_row() {
    let mut c = a_gameplay_client();
    let (ui, root) = gameplay_root(c.app_mut());
    let mut p = SpellComponentPanel::default();
    p.post_init(ui, root);
    assert!(p.bound(), "the shipped tree carries the component list");
    assert_eq!(
        p.templates(),
        2,
        "the premise: the header template and the row one"
    );

    let before = comps_with(12, None);
    assert!(p.update(ui, &before), "the first drive is always a redraw");
    assert_eq!(
        p.shown(),
        vec![SCARAB_WCID],
        "the premise: one row was drawn"
    );
    let row = p.rows[0].element;
    let starts_at_twelve = child_text(ui, row, spellcomponent::row::OWNED).as_deref() == Some("12");
    let gate_live = !p.update(ui, &before);

    let after = comps_with(7, Some(0x0600_103F));
    assert_eq!(
        after.0[0].rows[0].wcid, before.0[0].rows[0].wcid,
        "the premise: the same component, both frames"
    );
    let redrew = p.update(ui, &after) && p.shown() == vec![SCARAB_WCID];
    let row = p.rows[0].element;
    let on_screen = child_text(ui, row, spellcomponent::row::OWNED).as_deref() == Some("7")
        && p.rows[0].owned == 7;

    c.assert_behaviour(
        "panels.redraw.the-component-list-follows-a-count-changing-under-an-unchanged-component",
        move |_| starts_at_twelve && gate_live && redrew && on_screen,
    );
    c.shutdown();
}

#[derive(Debug)]
struct Skills(Vec<SkillEntry>);
impl GameView for Skills {
    fn skills(&self) -> &[SkillEntry] {
        &self.0
    }
}

fn skill(level: i32, effective: i32, vitae: i32) -> Skills {
    Skills(vec![SkillEntry {
        id: 6,
        name: "Melee Defense".into(),
        icon: None,
        min_level: 0,
        sac: skills::sac::TRAINED,
        level,
        effective,
        vitae,
    }])
}

/// A skill row follows the enchanted number and the death penalty, each on its own.
///
/// **The station order is load-bearing**: each frame differs from the one before it in exactly one
/// field, so a gate carrying one of the two and not the other cannot pass by accident.
pub(super) fn a_skill_row_follows_the_enchanted_number_and_the_penalty() {
    let mut c = a_gameplay_client();
    let (ui, root) = gameplay_root(c.app_mut());
    let mut p = SkillsPanel::default();
    p.post_init(ui, root);
    assert!(p.list.is_some(), "the shipped tree carries the skill list");

    // Plain: 72 raw, 72 after everything, no penalty.
    let plain = skill(72, 72, 0);
    assert!(p.update(ui, &plain), "the first drive is always a redraw");
    assert_eq!(p.rows.len(), 1, "the premise: one skill row was drawn");
    let row = p.rows[0].element;
    let starts_plain =
        child_text(ui, row, skills::row::VALUE).as_deref() == Some("72") && p.rows[0].font == 0;
    let gate_live = !p.update(ui, &plain);

    // A spell lands: the number after everything moves, the raw one does not.
    let buffed = skill(72, 77, 0);
    assert_eq!(
        buffed.0.iter().map(|s| s.id).collect::<Vec<_>>(),
        plain.0.iter().map(|s| s.id).collect::<Vec<_>>(),
        "the premise: the skill list is identical"
    );
    let buff = p.update(ui, &buffed);
    let row = p.rows[0].element;
    let buff_drawn =
        child_text(ui, row, skills::row::VALUE).as_deref() == Some("77") && p.rows[0].font == 1;

    // Back to plain, so the penalty station below is entered on one field.
    assert!(p.update(ui, &plain), "the spell expiring is a change too");
    assert!(!p.update(ui, &plain), "and the gate closes again behind it");

    // **The penalty, isolated**: the raw level and the number after everything are both held at
    // 72 and only the modifier moves. That is not a frame a player reaches -- a live penalty is
    // already inside the number after everything -- and it is deliberately so, because it is the
    // only shape that isolates the field. The one a player does reach is the frame after it.
    let penalty_isolated = skill(72, 72, -5);
    assert_eq!(
        penalty_isolated
            .0
            .iter()
            .map(|s| (s.id, s.level, s.effective))
            .collect::<Vec<_>>(),
        plain
            .0
            .iter()
            .map(|s| (s.id, s.level, s.effective))
            .collect::<Vec<_>>(),
        "the premise: only the modifier moves"
    );
    let penalty = p.update(ui, &penalty_isolated) && !p.update(ui, &penalty_isolated);
    let row = p.rows[0].element;
    let penalty_drawn =
        child_text(ui, row, skills::row::VALUE).as_deref() == Some("72") && p.rows[0].font == 1;

    // …and the frame a character actually carries: 100 raw, 95 after the penalty, drawn plain
    // because the penalty term adds it back.
    assert!(p.update(ui, &skill(100, 100, 0)));
    let real = p.update(ui, &skill(100, 95, -5));
    let row = p.rows[0].element;
    let real_drawn =
        child_text(ui, row, skills::row::VALUE).as_deref() == Some("95") && p.rows[0].font == 0;

    // The discriminator: a raw-level change was inside even the narrowest gate, so a failure
    // above reads as an omission rather than as a dead panel.
    let raised = p.update(ui, &skill(73, 78, 0));
    let row = p.rows[0].element;
    let raised_drawn = child_text(ui, row, skills::row::VALUE).as_deref() == Some("78");

    c.assert_behaviour(
        "panels.redraw.a-skill-row-follows-the-enchanted-number-and-the-death-penalty-each-on-their-own",
        move |_| {
            starts_plain
                && gate_live
                && buff
                && buff_drawn
                && penalty
                && penalty_drawn
                && real
                && real_drawn
                && raised
                && raised_drawn
        },
    );
    c.shutdown();
}

/// The six primaries only; the three vitals need a player and a vital, and the gate is the same
/// one either way.
#[derive(Debug)]
struct Attrs(Vec<(u32, i32)>);
impl GameView for Attrs {
    fn attribute(&self, id: u32) -> Option<i32> {
        self.0.iter().find(|(s, _)| *s == id).map(|(_, v)| *v)
    }
}

/// The attribute panel compares the very text it writes, so it cannot be looking at less than it
/// draws.
pub(super) fn the_attribute_rows_compare_the_text_they_write() {
    let mut c = a_gameplay_client();
    let (ui, root) = gameplay_root(c.app_mut());
    let mut p = AttributesPanel::default();
    p.post_init(ui, root);
    assert_eq!(
        p.rows.len(),
        9,
        "the premise: six primaries and three vitals"
    );

    let before = Attrs(vec![(1, 100)]);
    assert!(
        p.update(ui, &before),
        "the first drive writes the nine values"
    );
    let strength = p
        .rows
        .iter()
        .find(|r| r.stat == 1 && !r.secondary)
        .expect("Strength");
    let (elem, value) = (strength.element, strength.value.clone());
    let starts_at_100 =
        value == "100" && child_text(ui, elem, skills::row::VALUE).as_deref() == Some("100");
    let gate_live = !p.update(ui, &before);

    let redrew = p.update(ui, &Attrs(vec![(1, 130)]));
    let strength = p
        .rows
        .iter()
        .find(|r| r.stat == 1 && !r.secondary)
        .expect("Strength");
    let (elem, value) = (strength.element, strength.value.clone());
    let moved =
        value == "130" && child_text(ui, elem, skills::row::VALUE).as_deref() == Some("130");

    // …and one the shard has said nothing about reads as unknown rather than as blank, which is
    // what makes "not told yet" distinguishable from "never drawn".
    let unknown = p
        .rows
        .iter()
        .find(|r| r.stat == 4 && !r.secondary)
        .is_some_and(|r| r.value == "???");

    c.assert_behaviour(
        "panels.redraw.the-attribute-rows-compare-the-very-text-they-write",
        { move |_| starts_at_100 && gate_live && redrew && moved && unknown },
    );
    c.shutdown();
}
