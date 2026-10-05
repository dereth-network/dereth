use dereth_input::keys::MouseButton;
use dereth_primitives::{DataId, ObjectId};
use dereth_testkit::{ClientSpec, HeadlessClient, Inbound};
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::panels::examination;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::SpellEntry;
use {dereth_desktop::pump::Pump, dereth_input::win32::Win32Message};

pub const PRIMARY: MouseButton = MouseButton::Left;
/// The button the whole of this family is about. `dereth_testkit::Player::Click` is the primary
/// one only; see this section's header.
pub const SECONDARY: MouseButton = MouseButton::Right;

/// The school names the pane writes, in the shipped enumeration's own order, and the name
/// anything outside it takes. They are what the player reads, so they travel with the claim
/// rather than being read back out of the client's own table -- which is the one thing a
/// scenario asserting through that table could not catch.
pub const fn school_name(school: u32) -> &'static str {
    match school {
        1 => "War Magic",
        2 => "Life Magic",
        3 => "Item Enchantment",
        4 => "Creature Enchantment",
        5 => "Void Magic",
        _ => "None",
    }
}

/// The character these scenarios put in the world when they need one.
const PLAYER: ObjectId = ObjectId(0x5116_0000);
/// The foci a scenario puts in a side pack.
const FOCI: ObjectId = ObjectId(0x5116_0001);

/// A whole client on the shipped gameplay screen, and nothing else.
pub fn a_client() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::gameplay(4))
}

/// The same, with the two spells the pane's two forks need in the character's book and the
/// spellbook page up.
pub fn a_book_of_two() -> HeadlessClient {
    let mut c = a_client();
    learn_and_open(&mut c, &[super::FLAME_BOLT, super::STRENGTH_SELF]);
    c
}

/// One spell in the book, under a named account whose description has arrived.
pub fn a_book_of_one(spell: u32, account: &str) -> HeadlessClient {
    let mut c = a_client();
    greet(&mut c, account);
    describe(&mut c);
    learn_and_open(&mut c, &[spell]);
    c
}

/// Put `spells` in the character's book, as its own description would have left them, and
/// raise the spellbook's page.
pub fn learn_and_open(c: &mut HeadlessClient, spells: &[u32]) {
    let entries: Vec<SpellEntry> = spells
        .iter()
        .enumerate()
        .map(|(i, s)| entry(c, *s, u32::try_from(i).expect("a small book") + 1))
        .collect();
    c.hud_mut().spells = entries;
    open_the_spellbook(c);
    c.tick(1);
}

/// One row of the book. Everything but the id is read off the shipped table, which is where
/// the production host reads it too.
fn entry(c: &HeadlessClient, spell: u32, order: u32) -> SpellEntry {
    let b = shipped_spell(c, spell);
    SpellEntry {
        id: spell,
        name: b.name.clone(),
        icon: Some(DataId(b.icon)),
        school: b.school,
        level: 1,
        icon_power: 1,
        display_order: i32::try_from(order).expect("a small order"),
        bitfield: 0,
    }
}

/// One row of the shipped spell table, which is the pane's own source.
pub fn shipped_spell(c: &HeadlessClient, spell: u32) -> dereth_assets::tables::SpellBase {
    c.view()
        .expect_app()
        .hud()
        .spell_table
        .as_ref()
        .expect("the client loads the shipped spell table at startup")
        .spells
        .get(&spell)
        .unwrap_or_else(|| panic!("the shipped table carries spell {spell}"))
        .clone()
}

/// The names a formula's non-zero slots spell out, in slot order -- the lines the pane writes.
pub fn component_names(c: &HeadlessClient, formula: &[u32]) -> Vec<String> {
    let cat = &c.view().expect_app().hud().component_catalogue;
    formula
        .iter()
        .filter(|s| **s != 0)
        .map(|s| {
            cat.inq_spell_component_base(*s)
                .unwrap_or_else(|| panic!("the shipped component table carries slot {s}"))
                .name
                .clone()
        })
        .collect()
}

/// The range the pane writes, in yards, for a character with no skill of his own: the
/// constant, capped, over the metres in a yard. Done here in the scenario's own terms rather
/// than through the panel's helper.
pub fn yards(b: &dereth_assets::tables::SpellBase) -> f64 {
    let metres = f64::from(b.base_range_constant).min(75.0);
    metres / 0.9144
}

/// `Login_CharacterSet` with a synthetic account name, **through the codec** -- which is the
/// only way the client ever obtains one.
pub fn greet(c: &mut HeadlessClient, account: &str) {
    let set = dereth_protocol::login::LoginCharacterSet {
        status: 0,
        characters: Vec::new(),
        deleted: Vec::new(),
        num_allowed_characters: 5,
        account: account.to_owned(),
        use_turbine_chat: 1,
        has_throne_of_destiny: 1,
    };
    let bytes = dereth_protocol::write_body(&set).expect("the greeting encodes");
    let back: dereth_protocol::login::LoginCharacterSet =
        dereth_protocol::read_body(&bytes).expect("and decodes");
    assert_eq!(back.account, account, "the name survives the wire");
    c.when(Inbound::event(
        dereth_client_net::client_session::SessionEvent::CharacterSet(Box::new(back)),
    ));
    c.tick(1);
}

/// The character's own description. An empty one is enough: what these scenarios are about is
/// the hand-off it carries and not the qualities.
pub fn describe(c: &mut HeadlessClient) {
    c.when(Inbound::event(
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::default()),
    ));
    c.tick(1);
}

/// Put the player in the world with an inventory, so the walk that looks for a foci has a list
/// to walk and the formula does not take its "not in the world yet" arm.
pub fn stand_in_the_world(c: &mut HeadlessClient) -> ObjectId {
    let w = c.world_mut();
    let me = w.player.unwrap_or(PLAYER);
    if w.weenie(me).is_none() {
        let mut wn = dereth_client_model::weenie::Weenie::new(me);
        wn.pwd = dereth_protocol::types::PublicWeenieDesc {
            name: "Tester".into(),
            // The type the component walk expects of the thing whose pack it is reading.
            obj_type: 0x0000_0010,
            ..dereth_protocol::types::PublicWeenieDesc::default()
        };
        w.tables.weenies.insert(me, wn);
    }
    w.set_player(me);
    if let Some(x) = w.tables.weenies.get_mut(me) {
        if x.qualities.is_none() {
            x.qualities = Some(dereth_client_model::qualities::Qualities::default());
        }
    }
    if w.inventory(me).is_none() {
        w.tables
            .inventories
            .insert(me, dereth_client_model::objects::ObjectInventory::new(me));
    }
    me
}

/// The class the shipped table gives as `school`'s foci.
pub fn foci_of(c: &HeadlessClient, school: u32) -> u32 {
    c.view()
        .expect_app()
        .hud()
        .school_pack_wcid
        .iter()
        .find(|(s, _)| *s == school)
        .map(|(_, w)| *w)
        .unwrap_or_else(|| panic!("the shipped mapper names a foci for school {school}"))
}

/// Some other school the shipped mapper also names a foci for.
pub fn another_school(c: &HeadlessClient, not: u32) -> u32 {
    c.view()
        .expect_app()
        .hud()
        .school_pack_wcid
        .iter()
        .map(|(s, _)| *s)
        .find(|s| *s != not)
        .expect("the shipped mapper names more than one school")
}

/// A foci in the player's side-pack list, which is the only list the ownership walk reads.
pub fn carry_the_foci(c: &mut HeadlessClient, me: ObjectId, wcid: u32) {
    let w = c.world_mut();
    let mut foci = dereth_client_model::weenie::Weenie::new(FOCI);
    foci.pwd = dereth_protocol::types::PublicWeenieDesc {
        name: "A foci".into(),
        wcid,
        container_id: Some(me),
        ..dereth_protocol::types::PublicWeenieDesc::default()
    };
    w.tables.weenies.insert(FOCI, foci);
    let mut inv = w
        .inventory(me)
        .cloned()
        .expect("the player has an inventory");
    inv.containers.push(FOCI);
    w.tables.inventories.insert(me, inv);
}

/// Raise the spellbook's page, as the panel-visibility notice does.
pub fn open_the_spellbook(c: &mut HeadlessClient) {
    let (ui, screen) = parts(c);
    let root = screen.root().expect("the gameplay root");
    let page = ui
        .get_child_recursive(root, dereth_ui_screens::panels::remaining::SPELL_PAGE)
        .expect("the spell page is in the shipped layout");
    let panel_id = screen
        .panels
        .pages
        .iter()
        .find(|p| p.handle == page)
        .expect("the spell page is in the panel stack")
        .panel_id;
    screen.panels.recv_set_panel_visibility(ui, panel_id, true);
}

/// The shipped tree and the gameplay screen.
pub fn parts(c: &mut HeadlessClient) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = c.app_mut().ui_mut().expect("the UI shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a current screen");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<GamePlayScreen>()
        .expect("the gameplay screen");
    (ui, screen)
}

/// One real pointer gesture at a screen point, through the client's own pump and its own input
/// manager -- the delivery `dereth_testkit::Player::Click` makes, with the button the caller
/// names.
pub fn press(c: &mut HeadlessClient, x: i32, y: i32, at: u32, button: MouseButton) {
    let mut pump = Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    let m = pump.mouse_move_message(f64::from(x), f64::from(y), at);
    deliver(c, &mut pump, m);
    for (down, dt) in [(true, 10), (false, 20)] {
        let m = pump
            .button_message(button, down, at + dt)
            .expect("the client's own table names this button");
        deliver(c, &mut pump, m);
    }
    // Four, which is what the harness's own click step runs: two for the press and two so that
    // what it raised reaches the panels and the interaction layer.
    c.tick(4);
}

fn deliver(c: &mut HeadlessClient, pump: &mut Pump, m: Win32Message) {
    pump.dispatch(m);
    c.app_mut()
        .input_manager_mut()
        .expect("the real input maps")
        .on_message(m);
}

/// A point inside the spellbook row at `index`, in screen coordinates.
pub fn spellbook_row_point(c: &mut HeadlessClient, index: usize, spell: u32) -> (i32, i32) {
    let handle = {
        let list = c
            .view()
            .expect_app()
            .hud()
            .panels
            .spellbook
            .list
            .as_ref()
            .expect("the shipped spellbook list bound");
        assert_eq!(
            list.slots[index].spell,
            Some(spell),
            "row {index} carries the spell this scenario is about"
        );
        list.slots[index].handle
    };
    let b = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell")
        .ui
        .screen_box(handle);
    (b.x0 + 4, b.y0 + 4)
}

/// The centre of one element of the examine window.
pub fn centre_of(c: &mut HeadlessClient, id: ElementId) -> (i32, i32) {
    let h = find(c, id);
    let b = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell")
        .ui
        .screen_box(h);
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

fn find(c: &mut HeadlessClient, id: ElementId) -> ElemHandle {
    let (ui, screen) = parts(c);
    let root = screen.root().expect("the gameplay root");
    let window = ui
        .get_child_recursive(root, examination::WINDOW)
        .expect("the examine window is in the shipped layout");
    ui.get_child_recursive(window, id)
        .unwrap_or_else(|| panic!("{id:?}"))
}

/// Whether the examine window is up.
pub fn window_is_up(c: &mut HeadlessClient) -> bool {
    c.ui_snapshot().is_visible(examination::WINDOW)
}

/// The title the pane wrote, off the live tree.
pub fn title(c: &mut HeadlessClient) -> String {
    c.ui_snapshot()
        .text_of(examination::DISPLAYED_NAME_TEXT)
        .to_owned()
}

/// What the examine panel is showing, as an owned value.
#[derive(Debug, Clone)]
pub struct PaneFacts {
    pub spell: u32,
    pub component_names: Vec<String>,
    pub component_scids: Vec<u32>,
    pub rows_drawn: u32,
    pub awaiting: Option<ObjectId>,
    pub current: Option<ObjectId>,
    pub examines_pulled: u32,
    pub cancels: u32,
    pub closed: u32,
    pub filled: u32,
    pub components_marked: u32,
    pub rows_marked_missing: usize,
    pub notices_pulled: u32,
    pub component_clicks: u32,
    pub component_selections: u32,
    pub self_selections_absorbed: u32,
    pub examine_newly_selected_item: bool,
}

pub fn pane_facts(c: &mut HeadlessClient) -> PaneFacts {
    let p = &parts(c).1.examination;
    PaneFacts {
        spell: p.spell.spell,
        component_names: p.spell.component_names.clone(),
        component_scids: p.spell.component_scids.clone(),
        rows_drawn: p.spell.rows_drawn,
        awaiting: p.awaiting,
        current: p.current,
        examines_pulled: p.spell_examines_pulled,
        cancels: p.appraisals_cancelled,
        closed: p.closed,
        filled: p.spell.filled,
        components_marked: p.spell.components_marked,
        rows_marked_missing: p.spell.rows_marked_missing,
        notices_pulled: p.component_notices_pulled,
        component_clicks: p.component_clicks,
        component_selections: p.component_selections,
        self_selections_absorbed: p.self_selections_absorbed,
        examine_newly_selected_item: p.examine_newly_selected_item,
    }
}

/// Every appraisal the client has asked for since `from`, with the id each carried.
pub fn appraisals(c: &HeadlessClient, from: usize) -> Vec<ObjectId> {
    c.outbound()[from..]
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::Appraise(a) => Some(a.target),
            _ => None,
        })
        .collect()
}

// -----------------------------------------------------------------------------------------
// The pack, the formula icons and the marks over them, for `spell-examine.components.*` and
// `spell-examine.marks.*`.
// -----------------------------------------------------------------------------------------

/// The item type every call site of the component update gates on.
const TYPE_SPELL_COMPONENTS: u32 = 0x0000_1000;

/// A second object of one slot's kind, for the scenarios about owning a kind rather than a
/// thing.
pub const A_SECOND_OF_ITS_KIND: ObjectId = ObjectId(0x8116_0400);
/// Something that is not a component at all, in the same pack.
pub const A_PLAIN_ITEM: ObjectId = ObjectId(0x8116_0999);
/// Somewhere that is not the player's pack.
const ELSEWHERE: ObjectId = ObjectId(0x7116_0001);

/// The component object this family puts in the pack for formula slot `slot`.
pub fn component_object(slot: usize) -> ObjectId {
    ObjectId(0x8116_0300 + u32::try_from(slot).expect("a small formula"))
}

/// The formula the pane would draw for `spell`, truncated to the slots
/// that carry a component -- the pane's own source, and **not** the shipped table's raw slots
/// (see this section's header).
pub fn formula_of(c: &HeadlessClient, spell: u32) -> Vec<u32> {
    let base = shipped_spell(c, spell);
    let f = c.view().world().spell_formula(&base);
    let n = dereth_rules::magic::num_spell_components(&f);
    assert!(n > 0, "the shipped formula for spell {spell} has slots");
    f[..n].to_vec()
}

/// A client standing in the world with the shipped component catalogue handed over by its own
/// description, and the formula the pane would draw for `spell`.
fn a_client_with_a_pack(spell: u32) -> (HeadlessClient, Vec<u32>) {
    let mut c = a_client();
    describe(&mut c);
    let _ = stand_in_the_world(&mut c);
    assert!(
        !c.view().world().magic.catalogue.is_empty(),
        "the description handed the shipped component table to the game model"
    );
    let formula = formula_of(&c, spell);
    (c, formula)
}

/// Fill the pack with everything outside `missing`, learn `spell` and look at it.
fn look_at(c: &mut HeadlessClient, spell: u32, formula: &[u32], missing: &[usize]) {
    for (i, scid) in formula.iter().enumerate() {
        if !missing.contains(&i) {
            carry(c, component_object(i), *scid, 10);
        }
    }
    learn_and_open(c, &[spell]);
    let (x, y) = spellbook_row_point(c, 0, spell);
    press(c, x, y, 100_000, SECONDARY);
    let pane = pane_facts(c);
    assert!(window_is_up(c), "the look opened the description");
    assert_eq!(
        pane.component_scids, formula,
        "and its rows carry the formula"
    );
    assert_eq!(pane.rows_drawn as usize, formula.len(), "one icon per slot");
}

/// The pane open on `spell`, with every slot outside `missing` already in the player's pack --
/// so the marks are whatever the fill itself made them.
pub fn a_pane_on(spell: u32, missing: &[usize]) -> (HeadlessClient, Vec<u32>) {
    let (mut c, formula) = a_client_with_a_pack(spell);
    look_at(&mut c, spell, &formula, missing);
    (c, formula)
}

/// The same, with `n` slots left out -- chosen as slots whose **kind** appears in that formula
/// exactly once, because two slots of one kind stand or fall together and a scenario built on
/// a duplicated kind would be measuring something else.
pub fn a_pane_missing_unique(spell: u32, n: usize) -> (HeadlessClient, Vec<u32>, Vec<usize>) {
    let (mut c, formula) = a_client_with_a_pack(spell);
    let missing: Vec<usize> = (0..formula.len())
        .filter(|i| formula.iter().filter(|s| **s == formula[*i]).count() == 1)
        .take(n)
        .collect();
    assert_eq!(
        missing.len(),
        n,
        "the shipped formula has {n} slots of their own kind"
    );
    look_at(&mut c, spell, &formula, &missing);
    (c, formula, missing)
}

/// Put a stack of the kind slot `scid` names in the player's pack, through the client's own
/// move seam. Nothing here adds a row to the component tracker by hand.
pub fn carry(c: &mut HeadlessClient, id: ObjectId, scid: u32, stack: u16) {
    let (wcid, name) = {
        let cat = &c.view().world().magic.catalogue;
        let wcid = cat.scid_to_wcid(scid);
        assert_ne!(wcid, 0, "slot {scid} names a component kind");
        let name = cat
            .inq_spell_component_base(scid)
            .expect("the shipped table has a row for it")
            .name
            .clone();
        (wcid, name)
    };
    let me = c.view().world().player.expect("the player is in the world");
    let w = c.world_mut();
    put(
        w,
        id,
        dereth_protocol::types::PublicWeenieDesc {
            name,
            wcid,
            obj_type: TYPE_SPELL_COMPONENTS,
            stack_size: Some(stack),
            container_id: Some(me),
            ..dereth_protocol::types::PublicWeenieDesc::default()
        },
    );
    refresh_contents(w, me);
    w.server_says_move_item(
        id,
        me,
        0,
        ObjectId(0),
        0,
        true,
        &mut dereth_client_model::NullSink,
    );
}

/// The same for something that is not a component, which is the control for the notice.
pub fn carry_a_plain_item(c: &mut HeadlessClient, id: ObjectId) {
    let me = c.view().world().player.expect("the player is in the world");
    let w = c.world_mut();
    put(
        w,
        id,
        dereth_protocol::types::PublicWeenieDesc {
            name: "A sceptre".into(),
            wcid: 999,
            obj_type: 0,
            container_id: Some(me),
            ..dereth_protocol::types::PublicWeenieDesc::default()
        },
    );
    refresh_contents(w, me);
    w.server_says_move_item(
        id,
        me,
        0,
        ObjectId(0),
        0,
        true,
        &mut dereth_client_model::NullSink,
    );
}

/// Move one out of the player's ownership, through the same seam.
pub fn drop_it(c: &mut HeadlessClient, id: ObjectId) {
    let me = c.view().world().player.expect("the player is in the world");
    let w = c.world_mut();
    if w.weenie(ELSEWHERE).is_none() {
        put(
            w,
            ELSEWHERE,
            dereth_protocol::types::PublicWeenieDesc::default(),
        );
    }
    if let Some(x) = w.tables.weenies.get_mut(id) {
        x.pwd.container_id = Some(ELSEWHERE);
    }
    refresh_contents(w, me);
    w.server_says_move_item(
        id,
        ELSEWHERE,
        0,
        ObjectId(0),
        0,
        true,
        &mut dereth_client_model::NullSink,
    );
}

fn put(
    w: &mut dereth_client_model::World,
    id: ObjectId,
    pwd: dereth_protocol::types::PublicWeenieDesc,
) {
    let mut wn = dereth_client_model::weenie::Weenie::new(id);
    wn.pwd = pwd;
    w.tables.weenies.insert(id, wn);
}

fn refresh_contents(w: &mut dereth_client_model::World, container: ObjectId) {
    let held: Vec<dereth_protocol::types::ContentProfile> = w
        .tables
        .weenies
        .iter()
        .filter(|(_, x)| x.pwd.container_id == Some(container))
        .map(|(k, x)| dereth_protocol::types::ContentProfile {
            iid: k,
            container_properties: u32::from(x.is_container()),
        })
        .collect();
    w.view_object_contents(container, &held, &mut dereth_client_model::NullSink);
}

/// The slot number each drawn row is stamped with, read off the **live** rows rather than off
/// the panel's own list -- because the stamp is what the click's lookup is handed.
pub fn row_scids(c: &mut HeadlessClient, rows: usize) -> Vec<u32> {
    let handles: Vec<ElemHandle> = {
        let screen = parts(c).1;
        (0..rows)
            .map(|i| {
                screen
                    .examination
                    .spell
                    .component_row(i)
                    .unwrap_or_else(|| panic!("row {i} is drawn"))
            })
            .collect()
    };
    let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
    handles
        .iter()
        .enumerate()
        .map(|(i, h)| {
            dereth_ui_screens::panels::spell_examine::row_component_scid(ui, *h)
                .unwrap_or_else(|| panic!("row {i} is stamped with its own slot"))
        })
        .collect()
}

/// The centre of the formula icon at `index`.
pub fn component_icon_point(c: &mut HeadlessClient, index: usize) -> (i32, i32) {
    let row = parts(c)
        .1
        .examination
        .spell
        .component_row(index)
        .unwrap_or_else(|| panic!("row {index} is drawn"));
    let b = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell")
        .ui
        .screen_box(row);
    assert!(
        b.width() > 0 && b.height() > 0,
        "the icon has an extent to press on"
    );
    (b.x0 + b.width() / 2, b.y0 + b.height() / 2)
}

/// Which formula slots are showing the mark the shipped layout draws over every icon.
pub fn marked(c: &mut HeadlessClient, slots: usize) -> Vec<usize> {
    let marks: Vec<Option<ElemHandle>> = {
        let (ui, screen) = parts(c);
        (0..slots)
            .map(|i| screen.examination.spell.component_mark(ui, i))
            .collect()
    };
    let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
    marks
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            let h = m.unwrap_or_else(|| panic!("row {i} carries a mark child"));
            ui.node(h)
                .expect("a live mark node")
                .region
                .flags
                .visible
                .then_some(i)
        })
        .collect()
}
