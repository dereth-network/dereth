use super::*;

/// The lit state of the backpack button's own overlay.
const OVERLAY_DRAG_OVER: StateId = StateId(0x1000_0046);
/// Its resting state.
const OVERLAY_NONE: StateId = StateId(0x1000_003F);

pub(super) const DOLL_WEAPON_SLOT: ElementId = ElementId(0x1000_01DF);
pub(super) const DOLL_SHIELD_SLOT: ElementId = ElementId(0x1000_01E1);
const DOLL_SHIRT_SLOT: ElementId = ElementId(0x1000_01E2);

const HINT_PLAYER: ObjectId = ObjectId(0x5000_0001);
const SHIELD: ObjectId = ObjectId(0x5000_0010);
const SWORD: ObjectId = ObjectId(0x5000_0011);
const TORCH: ObjectId = ObjectId(0x5000_0012);
const PLAIN_ROCK: ObjectId = ObjectId(0x5000_0013);
const SHIRT: ObjectId = ObjectId(0x5000_0014);
const HINT_ITEMS: [ObjectId; 4] = [SHIELD, SWORD, TORCH, PLAIN_ROCK];

/// A player carrying a shield, a one-handed sword, a torch and a rock. The first three can each
/// go in a different place on the body and the rock can go nowhere, which is what makes the
/// body-slot claim a claim rather than a constant.
fn seed_for_hints(w: &mut dereth_client_model::World) {
    use dereth_rules::slots::loc;

    w.player = Some(HINT_PLAYER);
    w.tables.inventories.insert(
        HINT_PLAYER,
        dereth_client_model::objects::ObjectInventory::new(HINT_PLAYER),
    );
    for id in HINT_ITEMS.iter().copied().chain([HINT_PLAYER]) {
        let mut wn = dereth_client_model::Weenie::new(id);
        wn.valid = true;
        wn.pwd.name = format!("obj{:X}", id.0 & 0xFF);
        w.tables.weenies.insert(id, wn);
    }
    {
        let p = w.tables.weenies.get_mut(HINT_PLAYER).expect("seeded");
        p.pwd.items_capacity = Some(102);
        p.pwd.containers_capacity = Some(7);
        p.pwd.bitfield |= dereth_rules::weenie::bitfield::OPENABLE;
    }
    for (id, mask) in [
        (SHIELD, loc::SHIELD),
        (SWORD, loc::MELEE_WEAPON),
        (TORCH, loc::HELD),
    ] {
        w.tables
            .weenies
            .get_mut(id)
            .expect("seeded")
            .pwd
            .valid_locations = Some(mask);
    }
    for (i, id) in HINT_ITEMS.iter().enumerate() {
        w.tables
            .weenies
            .get_mut(*id)
            .expect("seeded")
            .pwd
            .container_id = Some(HINT_PLAYER);
        w.tables
            .inventories
            .get_mut(HINT_PLAYER)
            .expect("seeded")
            .add_content(*id, false, i);
    }
}

fn a_client_with_four_things() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    seed_for_hints(&mut c.app_mut().probe_mut().objects_mut().world);
    open_pack(&mut c);
    c.tick(2);
    c
}

/// A shipped element of the gameplay screen, by id.
pub(super) fn shipped(c: &mut HeadlessClient, id: ElementId) -> ElemHandle {
    let (ui, root) = gameplay_root(c.app_mut());
    ui.get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped tree"))
}

/// The state the backpack button's own overlay is in.
fn backpack_overlay(c: &mut HeadlessClient) -> StateId {
    let h = shipped(c, INVENTORY_DRAG_OVERLAY);
    let (ui, _root) = gameplay_root(c.app_mut());
    ui.node(h).expect("alive").state
}

/// One body slot's tile.
pub(super) fn doll_tile(c: &mut HeadlessClient, element: ElementId) -> ElemHandle {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen
        .inventory
        .doll
        .iter()
        .find(|(_, w)| w.element == element)
        .unwrap_or_else(|| panic!("the body slot {element:?} is bound"))
        .1
        .slots[0]
        .handle
}

/// The hint one body slot is showing.
fn doll_hint(c: &mut HeadlessClient, element: ElementId) -> Option<StateId> {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen
        .inventory
        .doll
        .iter()
        .find(|(_, w)| w.element == element)
        .unwrap_or_else(|| panic!("the body slot {element:?} is bound"))
        .1
        .slots[0]
        .drag_accept_state
}

/// What the body slot is holding.
pub(super) fn doll_item(c: &mut HeadlessClient, element: ElementId) -> Option<ObjectId> {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen
        .inventory
        .doll
        .iter()
        .find(|(_, w)| w.element == element)
        .unwrap_or_else(|| panic!("the body slot {element:?} is bound"))
        .1
        .item_at(0)
}

pub(super) fn shortcut_tile(c: &mut HeadlessClient, n: usize) -> ElemHandle {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen
        .shortcuts
        .slots
        .get(n)
        .expect("the shortcut list is bound")
        .slots[0]
        .handle
}

fn shortcut_hint(c: &mut HeadlessClient, n: usize) -> Option<StateId> {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen.shortcuts.slots[n].slots[0].drag_accept_state
}

/// The state the shortcut tile's own hint element is in -- the element, not the panel's record.
fn shortcut_overlay(c: &mut HeadlessClient, n: usize) -> StateId {
    let h = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen.shortcuts.slots[n].slots[0]
            .drag_accept
            .expect("the tile's hint overlay is bound")
    };
    let (ui, _root) = gameplay_root(c.app_mut());
    ui.node(h).expect("alive").state
}

/// Carry the drag over `to`, having first required that the pointer really is over it: the
/// element the hit test finds must be the one that catches this drag, or the hint read next was
/// raised on something else.
pub(super) fn carry_over(c: &mut HeadlessClient, to: ElemHandle) -> Target {
    let (x, y) = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        centre(ui, to)
    };
    let at = Target::Point(ScreenPoint::new(x, y));
    c.when(Over(at));
    {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        let hit = ui
            .hit_test_screen(x, y)
            .expect("the pointer is over something");
        assert!(
            ui.drag_and_drop_catcher(hit) == Some(to) || ui.is_ancestor_of(to, hit),
            "the premise: the element under the pointer catches this drag"
        );
    }
    at
}

/// The text of every line the notice strip is showing.
pub(super) fn strip_lines(c: &mut HeadlessClient) -> Vec<String> {
    let list = shipped(c, LIST_BOX);
    let (ui, _root) = gameplay_root(c.app_mut());
    ui.children(list)
        .into_iter()
        .filter_map(|h| ui.text_element_mut(h).map(|t| t.glyphs.inq_text(false)))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// inventory.drag-hint.the-backpack-button-lights-while-something-is-carried-over-it
// ---------------------------------------------------------------------------------------------

/// The big backpack button on the toolbar lights while something is held over it, and goes dark
/// again the moment the pointer leaves.
pub(super) fn the_backpack_button_lights_while_something_is_over_it() {
    let mut c = a_client_with_four_things();
    // The control. Nothing has written this element yet, so it is in no state at all -- which is
    // not the lit one, which is all the measurement below needs of it.
    let resting = backpack_overlay(&mut c);

    let button = shipped(&mut c, INVENTORY_BUTTON);
    let rock = pack_slot(&mut c, PLAIN_ROCK);
    let from = point_of(&mut c, rock);
    c.when(Grab(from));
    carry_over(&mut c, button);
    let lit = backpack_overlay(&mut c) == OVERLAY_DRAG_OVER;

    let torch = pack_slot(&mut c, TORCH);
    let at = carry_over(&mut c, torch);
    let dark_again = backpack_overlay(&mut c) == OVERLAY_NONE;
    c.when(LetGo(at));
    clear_requests(c.ui_outbox());

    c.assert_behaviour(
        "inventory.drag-hint.the-backpack-button-lights-while-something-is-carried-over-it",
        move |_| resting != OVERLAY_DRAG_OVER && lit && dark_again,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.drag-hint.a-body-slot-shows-whether-it-could-take-what-is-carried
// ---------------------------------------------------------------------------------------------

/// Carry `item` over `over` and report what both the shield slot and the weapon slot say about
/// it, then let go where the pointer is so the next station starts clean.
fn hints_while_carrying(
    c: &mut HeadlessClient,
    item: ObjectId,
    over: ElemHandle,
) -> (Option<StateId>, Option<StateId>) {
    let tile = pack_slot(c, item);
    let from = point_of(c, tile);
    c.when(Grab(from));
    let at = carry_over(c, over);
    let both = (
        doll_hint(c, DOLL_SHIELD_SLOT),
        doll_hint(c, DOLL_WEAPON_SLOT),
    );
    c.when(LetGo(at));
    clear_requests(c.ui_outbox());
    both
}

/// Each place on the body says yes or no for the thing being carried over it, and the answer is
/// not the slot's own list of places alone.
///
/// Six stations, and the last two are the ones that matter: a torch may be held, so the weapon
/// hand says yes in peace and no with the weapon out -- the slot's list passes both times and
/// only the wield rule tells them apart.
pub(super) fn a_body_slot_shows_whether_it_could_take_what_is_carried() {
    let mut c = a_client_with_four_things();
    let shield_slot = doll_tile(&mut c, DOLL_SHIELD_SLOT);
    let weapon_slot = doll_tile(&mut c, DOLL_WEAPON_SLOT);
    let nothing_yet = doll_hint(&mut c, DOLL_SHIELD_SLOT).is_none();

    // A rock can go nowhere, so the shield slot refuses it.
    let (rock_on_shield, _) = hints_while_carrying(&mut c, PLAIN_ROCK, shield_slot);
    // A shield is not a weapon, so the weapon hand refuses it...
    let (_, shield_on_weapon) = hints_while_carrying(&mut c, SHIELD, weapon_slot);
    // ...and its own slot takes it, with the weapon hand saying nothing about it.
    let (shield_on_shield, weapon_untouched) = hints_while_carrying(&mut c, SHIELD, shield_slot);
    // A one-handed sword is taken in the shield slot too: that is the alias the client carries.
    let (sword_on_shield, _) = hints_while_carrying(&mut c, SWORD, shield_slot);
    // A torch may be held, and in peace the weapon hand takes it.
    let (_, torch_at_peace) = hints_while_carrying(&mut c, TORCH, weapon_slot);
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .combat
        .combat_mode = dereth_client_model::combat::CombatMode::Melee;
    c.tick(1);
    // With the weapon out, the same hand refuses the same torch.
    let (_, torch_in_combat) = hints_while_carrying(&mut c, TORCH, weapon_slot);

    let accept = Some(drag_accept_state::ACCEPT);
    let refuse = Some(drag_accept_state::REFUSE);
    c.assert_behaviour(
        "inventory.drag-hint.a-body-slot-shows-whether-it-could-take-what-is-carried",
        move |_| {
            nothing_yet
                && rock_on_shield == refuse
                && shield_on_weapon == refuse
                && shield_on_shield == accept
                && weapon_untouched != accept
                && sword_on_shield == accept
                && torch_at_peace == accept
                && torch_in_combat == refuse
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.drag-hint.a-shortcut-tile-offers-itself-for-anything-carried
// ---------------------------------------------------------------------------------------------

/// A shortcut tile says yes to anything carried over it, and says nothing again when the pointer
/// moves off -- it is a place to keep a reference to a thing, so nothing about the thing itself
/// can refuse it.
pub(super) fn a_shortcut_tile_offers_itself_for_anything_carried() {
    let mut c = a_client_with_four_things();
    let nothing_yet = shortcut_hint(&mut c, 0).is_none();
    let tile = shortcut_tile(&mut c, 0);

    let rock = pack_slot(&mut c, PLAIN_ROCK);
    let from = point_of(&mut c, rock);
    c.when(Grab(from));
    carry_over(&mut c, tile);
    let offered = shortcut_hint(&mut c, 0);

    let torch = pack_slot(&mut c, TORCH);
    let at = carry_over(&mut c, torch);
    let taken_back = shortcut_hint(&mut c, 0);
    c.when(LetGo(at));
    clear_requests(c.ui_outbox());

    c.assert_behaviour(
        "inventory.drag-hint.a-shortcut-tile-offers-itself-for-anything-carried",
        move |_| {
            nothing_yet
                && offered == Some(drag_accept_state::ACCEPT)
                && taken_back == Some(drag_accept_state::NONE)
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.drag-hint.letting-go-on-a-tile-takes-its-own-hint-down
// ---------------------------------------------------------------------------------------------

/// Letting go **on** the tile that is lit takes its hint down, on the element as well as in the
/// panel's own record, and ends the drag.
///
/// This is the half a scenario that only ever moves the pointer away cannot see: the hint could
/// be cleared by the leave alone and a drop would leave it standing for the rest of the session.
pub(super) fn letting_go_on_a_tile_takes_its_own_hint_down() {
    let mut c = a_client_with_four_things();
    let tile = shortcut_tile(&mut c, 0);
    let nothing_yet = shortcut_hint(&mut c, 0).is_none();

    let rock = pack_slot(&mut c, PLAIN_ROCK);
    let from = point_of(&mut c, rock);
    c.when(Grab(from));
    let at = carry_over(&mut c, tile);
    let lit = shortcut_hint(&mut c, 0) == Some(drag_accept_state::ACCEPT)
        && shortcut_overlay(&mut c, 0) == drag_accept_state::ACCEPT;

    c.when(LetGo(at));
    clear_requests(c.ui_outbox());
    let down = shortcut_hint(&mut c, 0) == Some(drag_accept_state::NONE)
        && shortcut_overlay(&mut c, 0) == drag_accept_state::NONE;
    let over_now = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.drag_state().element.is_none()
    };

    c.assert_behaviour(
        "inventory.drag-hint.letting-go-on-a-tile-takes-its-own-hint-down",
        move |_| nothing_yet && lit && down && over_now,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.wear.dropping-a-worn-thing-back-on-its-own-slot-says-it-is-already-worn
// ---------------------------------------------------------------------------------------------

/// Put `id` on the body, through the placement the client's own rebuild derives its masks from.
fn wear_it(
    w: &mut dereth_client_model::World,
    id: ObjectId,
    location: u32,
    priority: u32,
    name: &str,
) {
    let mut wn = dereth_client_model::Weenie::new(id);
    wn.valid = true;
    wn.pwd.name = name.into();
    wn.pwd.valid_locations = Some(location);
    wn.pwd.priority = Some(priority);
    wn.pwd.container_id = Some(HINT_PLAYER);
    wn.pwd.wielder_id = Some(HINT_PLAYER);
    wn.pwd.location = Some(location);
    w.tables.weenies.insert(id, wn);
    w.tables
        .inventories
        .get_mut(HINT_PLAYER)
        .expect("seeded")
        .set_placement(id, location, priority);
    w.remake_character_inventory();
}

/// A client wearing one thing on its chest, named however the caller asks.
fn a_client_wearing_something(material: Option<u32>) -> HeadlessClient {
    let mut c = a_client_with_four_things();
    {
        let w = &mut c.app_mut().probe_mut().objects_mut().world;
        wear_it(
            w,
            SHIRT,
            dereth_rules::slots::loc::CHEST_WEAR,
            0x20,
            "obj14",
        );
        if let Some(m) = material {
            let wn = w.tables.weenies.get_mut(SHIRT).expect("worn");
            wn.pwd.name = "Breastplate".into();
            wn.pwd.plural_name = None;
            wn.pwd.material_type = Some(m);
            wn.pwd.stack_size = Some(1);
        }
    }
    c.tick(2);
    assert_eq!(
        doll_item(&mut c, DOLL_SHIRT_SLOT),
        Some(SHIRT),
        "the premise: it is on the body before it is dragged off it"
    );
    c
}

/// Drag the worn thing off its own slot and drop it straight back on: the lines that appear, and
/// whether the hint said yes on the way.
fn drop_it_back_on(c: &mut HeadlessClient) -> (Vec<String>, bool, bool) {
    let slot = doll_tile(c, DOLL_SHIRT_SLOT);
    let before = strip_lines(c);
    let from = point_of(c, slot);
    c.when(Grab(from));
    let at = carry_over(c, slot);
    let green_on_the_way = doll_hint(c, DOLL_SHIRT_SLOT) == Some(drag_accept_state::ACCEPT);
    c.when(LetGo(at));
    clear_requests(c.ui_outbox());
    let after = strip_lines(c);
    let new_lines: Vec<String> = after.into_iter().skip(before.len()).collect();
    let not_ghosted = !c
        .app_mut()
        .objects()
        .world
        .weenie(SHIRT)
        .expect("worn")
        .waiting;
    (new_lines, green_on_the_way, not_ghosted)
}

/// A worn thing dropped back on the place it is already worn is refused in the client's own
/// words, and the words name the thing the way the player sees it named.
///
/// The hint on the way over says yes, deliberately: there is no exception for the slot a thing
/// came from, and the refusal is the drop's, not the hover's.
pub(super) fn dropping_a_worn_thing_back_on_its_own_slot_says_so() {
    let mut plain_client = a_client_wearing_something(None);
    let (plain, green, not_ghosted) = drop_it_back_on(&mut plain_client);
    plain_client.shutdown();
    let says_it_plainly = plain.iter().any(|l| l == "The obj14 is already being worn");

    let mut c = a_client_wearing_something(Some(0x3A));
    let (with_material, _, _) = drop_it_back_on(&mut c);
    let says_the_material_too = with_material
        .iter()
        .any(|l| l == "The Bronze Breastplate is already being worn");

    c.assert_behaviour(
        "inventory.wear.dropping-a-worn-thing-back-on-its-own-slot-says-it-is-already-worn",
        move |_| green && says_it_plainly && says_the_material_too && not_ghosted,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// The hints the open container lights while something is carried over it.
//
// What the window says *while* the icon is in the air, which is what the three-step drag is
// for.
//
// Every scenario reads the hint **twice** -- as the panel last decided it and as the live tile
// carries it -- because the panel's record alone cannot catch a decision that never reached the
// screen, and the tile alone cannot tell "never written" from "written none". And every one of
// them proves the pointer is really over the tile before it reads anything back: the hint is
// raised on whatever the pointer's own walk finds, so a scenario that skipped that would be
// measuring a hint raised somewhere else.
// ---------------------------------------------------------------------------------------------

/// One of the open container's three lists: its own row, its pack strip, its contents.
fn chest_list(c: &HeadlessClient, i: usize) -> &ItemListWidget {
    let p = &c.view().expect_app().hud().panels.external_container;
    [
        p.top_container.as_ref(),
        p.container_list.as_ref(),
        p.item_list.as_ref(),
    ][i]
        .expect("the window binds all three of its lists")
}

/// The hint on one tile, read twice: as the panel decided it, and as the live element carries it.
fn hint(c: &mut HeadlessClient, list: usize, row: usize) -> (Option<StateId>, Option<StateId>) {
    let (decided, h) = {
        let s = &chest_list(c, list).slots[row];
        (s.drag_accept_state, s.drag_accept)
    };
    let (ui, _root) = gameplay_root(c.app_mut());
    (decided, h.and_then(|k| ui.node(k)).map(|n| n.state))
}

/// One tile of one of the window's lists, as a pointer target -- and the scenario is told, before
/// it reads anything, that the pointer really would land on it.
fn chest_list_tile(c: &mut HeadlessClient, list: usize, row: usize) -> Target {
    let h = chest_list(c, list).slots[row].handle;
    let (ui, _root) = gameplay_root(c.app_mut());
    let (x, y) = centre(ui, h);
    let hit = ui
        .hit_test_screen(x, y)
        .expect("the pointer is over something");
    let catcher = ui
        .drag_and_drop_catcher(hit)
        .expect("and over something that catches drops");
    assert!(
        catcher == h || ui.is_ancestor_of(h, catcher),
        "the premise: the pointer at ({x}, {y}) reaches the tile this scenario means"
    );
    Target::Point(ScreenPoint::new(x, y))
}

/// The first pack on the **player's own** strip, and where its tile is.
fn a_carried_pack(c: &mut HeadlessClient) -> (ObjectId, Target) {
    let (item, h) = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        let strip = screen
            .inventory
            .container_list
            .as_ref()
            .expect("the side-pack strip");
        let slot = strip
            .slots
            .iter()
            .find(|s| s.item.is_some())
            .expect("the recording's character carries a pack");
        (slot.item.expect("checked"), slot.handle)
    };
    (item, point_of(c, h))
}

/// Make the open container a housing hook, which is the one thing it can refuse a drop for.
fn make_the_chest_a_hook(c: &mut HeadlessClient, hook: u16, accepts: u32, owner: ObjectId) {
    {
        let w = c
            .app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .weenie_mut(RECORDED_CHEST)
            .expect("the chest is in the object table");
        w.pwd.hook_type = Some(hook);
        w.pwd.hook_item_types = Some(accepts);
        w.pwd.house_owner_iid = Some(owner);
    }
    assert!(
        c.view()
            .world()
            .weenie(RECORDED_CHEST)
            .is_some_and(dereth_client_model::weenie::Weenie::is_hook),
        "the premise: the ground object really is a hook now"
    );
    c.tick(1);
}

/// Give the carried thing the two facts a hook asks about it.
fn make_it_hookable(c: &mut HeadlessClient, item: ObjectId, valid: u16, obj_type: u32) {
    let w = c
        .app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .weenie_mut(item)
        .expect("the carried object");
    w.pwd.hook_type = Some(valid);
    w.pwd.obj_type = obj_type;
    c.tick(1);
}

// ---------------------------------------------------------------------------------------------
// container.ground.the-open-one-lights-the-row-the-carried-thing-is-over-and-no-other
// ---------------------------------------------------------------------------------------------

/// Carrying something over the open container's contents lights the row it is over, and only
/// that row -- and a corpse answers exactly as a chest does.
///
/// The second half is the part that can fail. The window is the only one the client has for
/// anything lying in the world, and what it asks about a drop has nothing to do with whether the
/// thing on the ground is a corpse; the scenario turns the very same ground object into one and
/// demands the answer be unchanged, so "the window does not look" is stated as something that
/// could have gone the other way: a corpse showing nothing is the plausible wrong answer.
pub(super) fn the_open_chest_lights_the_row_the_carried_thing_is_over() {
    let (mut c, item) = a_chest_with_something_in_it();
    let (row, from) = chest_tile(&mut c, item);
    assert_eq!(row, 0, "the premise: the filled row is the first one");
    assert_eq!(
        hint(&mut c, 2, 1),
        (None, Some(StateId(0))),
        "the premise: row 1 is unpainted"
    );

    c.when(Player::Grab(from));
    let onto = chest_list_tile(&mut c, 2, 1);
    c.when(Player::Over(onto));
    let lit = hint(&mut c, 2, 1)
        == (
            Some(drag_accept_state::ACCEPT),
            Some(drag_accept_state::ACCEPT),
        );
    let and_no_other = hint(&mut c, 2, 0) == (None, Some(StateId(0)));
    c.when(Player::Release);

    // The same ground object, now a corpse.
    {
        let w = c
            .app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .weenie_mut(RECORDED_CHEST)
            .expect("the ground object");
        w.pwd.bitfield |= dereth_rules::weenie::bitfield::CORPSE;
    }
    c.tick(1);
    assert!(
        c.view()
            .world()
            .weenie(RECORDED_CHEST)
            .is_some_and(dereth_client_model::weenie::Weenie::is_corpse),
        "the premise: it really is a corpse now"
    );
    let (_, from) = chest_tile(&mut c, item);
    c.when(Player::Grab(from));
    let onto = chest_list_tile(&mut c, 2, 1);
    c.when(Player::Over(onto));
    let a_corpse_is_the_same = hint(&mut c, 2, 1)
        == (
            Some(drag_accept_state::ACCEPT),
            Some(drag_accept_state::ACCEPT),
        );
    c.when(Player::Release);

    c.assert_behaviour(
        "container.ground.the-open-one-lights-the-row-the-carried-thing-is-over-and-no-other",
        move |_| lit && and_no_other && a_corpse_is_the_same,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// container.ground.a-hook-shows-whether-it-could-take-what-is-carried
// ---------------------------------------------------------------------------------------------

/// A hook is the one ground container that says no, and it says no on two counts that are
/// independent of each other: where the hook is, and what kind of thing it takes.
///
/// Meeting one and missing the other is still a refusal, and meeting both is the green. Without
/// the two-count walk a client that read only one of them would pass half of this and look
/// right. A hook in nobody's house refuses everything before either count is looked at.
pub(super) fn a_hook_shows_whether_it_could_take_what_is_carried() {
    use dereth_client_model::housing::hook_type_enum;
    use dereth_rules::weenie::item_type;

    let wall = u16::try_from(hook_type_enum::WALL).expect("a hook location");
    let floor = u16::try_from(hook_type_enum::FLOOR).expect("a hook location");

    let (mut c, item) = a_chest_with_something_in_it();
    // A wall hook in somebody's house that takes clothing and armour only.
    make_the_chest_a_hook(&mut c, wall, item_type::VESTEMENTS, ObjectId(0x5000_0001));
    // The carried thing may only go on a floor hook, and is a weapon: both counts missed.
    make_it_hookable(&mut c, item, floor, item_type::MELEE_WEAPON);

    let (_, from) = chest_tile(&mut c, item);
    c.when(Player::Grab(from));
    let empty = chest_list_tile(&mut c, 2, 1);
    let filled = chest_list_tile(&mut c, 2, 0);
    c.when(Player::Over(empty));
    let both_missed = hint(&mut c, 2, 1)
        == (
            Some(drag_accept_state::REFUSE),
            Some(drag_accept_state::REFUSE),
        );

    // The right place now, still the wrong kind.
    make_it_hookable(&mut c, item, wall, item_type::MELEE_WEAPON);
    c.when(Player::Over(filled)).when(Player::Over(empty));
    let place_alone_is_not_enough = hint(&mut c, 2, 1).1 == Some(drag_accept_state::REFUSE);

    // Both met: armour on a wall hook that takes armour.
    make_it_hookable(&mut c, item, wall, item_type::ARMOR);
    c.when(Player::Over(filled)).when(Player::Over(empty));
    let both_met_is_green = hint(&mut c, 2, 1).1 == Some(drag_accept_state::ACCEPT);
    c.when(Player::Release);

    c.assert_behaviour(
        "container.ground.a-hook-shows-whether-it-could-take-what-is-carried",
        move |_| both_missed && place_alone_is_not_enough && both_met_is_green,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// container.ground.a-hook-in-nobodys-house-refuses-everything-carried-over-it
// ---------------------------------------------------------------------------------------------

/// A hook in nobody's house refuses everything, whatever it is and wherever the hook is.
///
/// It is the answer given before either of the two counts above is looked at, which is why it is
/// its own row: a client that folded it into them would let a thing that happened to match an
/// unowned hook's masks light green over it.
pub(super) fn a_hook_in_nobodys_house_refuses_everything_carried_over_it() {
    use dereth_client_model::housing::hook_type_enum;
    use dereth_rules::weenie::item_type;

    let wall = u16::try_from(hook_type_enum::WALL).expect("a hook location");
    let (mut c, item) = a_chest_with_something_in_it();
    make_the_chest_a_hook(&mut c, wall, item_type::ITEM, ObjectId(0));
    // Both counts would have been met, if anyone owned the house.
    make_it_hookable(&mut c, item, wall, item_type::ARMOR);

    let (_, from) = chest_tile(&mut c, item);
    c.when(Player::Grab(from));
    let onto = chest_list_tile(&mut c, 2, 1);
    c.when(Player::Over(onto));
    let refused = hint(&mut c, 2, 1).1 == Some(drag_accept_state::REFUSE);
    c.when(Player::Release);

    c.assert_behaviour(
        "container.ground.a-hook-in-nobodys-house-refuses-everything-carried-over-it",
        move |_| refused,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// container.ground.the-windows-own-row-answers-a-carried-thing-as-a-list-of-packs-would
// ---------------------------------------------------------------------------------------------

/// The window's own row -- the tile holding the container itself -- does not answer the way its
/// contents do. It answers as a list of packs answers a thing carried over a pack with room in
/// it, which is a third answer and not the green.
///
/// This is the scenario that catches a client which simply painted green on every tile of the
/// window.
pub(super) fn the_windows_own_row_answers_as_a_list_of_packs_would() {
    let (mut c, item) = a_chest_with_something_in_it();
    assert_eq!(
        chest_list(&c, 0).slots[0].item,
        Some(RECORDED_CHEST),
        "the premise: the top row holds the ground object itself"
    );

    let (_, from) = chest_tile(&mut c, item);
    c.when(Player::Grab(from));
    let onto = chest_list_tile(&mut c, 0, 0);
    c.when(Player::Over(onto));
    let third_answer = hint(&mut c, 0, 0)
        == (
            Some(drag_accept_state::INTO_CONTAINER),
            Some(drag_accept_state::INTO_CONTAINER),
        );
    c.when(Player::Release);

    c.assert_behaviour(
        "container.ground.the-windows-own-row-answers-a-carried-thing-as-a-list-of-packs-would",
        move |_| third_answer,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// container.ground.a-pack-over-the-contents-lights-green-and-is-still-refused-on-the-drop
// ---------------------------------------------------------------------------------------------

/// A pack carried over the open container's **contents** lights green on the way in and is
/// refused when it is let go -- and the green comes down anyway.
///
/// All three are true at once and the first two disagree on purpose: the window answers the
/// hover itself and says yes to anything, and the list answers the drop and says a contents list
/// does not take packs. The pack is not left marked, and the tile it landed on is not left lit:
/// nothing moves the pointer between the release and the drag ending, so a client that cleared
/// the hint on the next hover instead would leave that tile green for as long as the window is
/// open.
pub(super) fn a_pack_over_the_contents_lights_green_and_is_still_refused_on_the_drop() {
    let (mut c, _resident) = a_chest_with_something_in_it();
    let (pack, from) = a_carried_pack(&mut c);
    assert!(
        !chest_list(&c, 2).container_list,
        "the premise: the contents list is not a list of packs"
    );

    c.when(Player::Grab(from));
    assert!(
        object_waiting(&c, pack),
        "the premise: the source is marked the moment it is lifted"
    );
    let onto = chest_list_tile(&mut c, 2, 1);
    c.when(Player::Over(onto));
    let green_on_the_way_in = hint(&mut c, 2, 1)
        == (
            Some(drag_accept_state::ACCEPT),
            Some(drag_accept_state::ACCEPT),
        );

    let mark = c.outbound().len();
    c.when(Player::Release);
    let refused = !sent_since(&c, mark)
        .iter()
        .any(|r| matches!(r, Request::PutItemInContainer(_)))
        && c.view().expect_app().interaction().last_refusal.as_deref()
            == Some("Cannot place container in item list");
    let not_left_marked = !object_waiting(&c, pack);
    let hint_came_down =
        hint(&mut c, 2, 1) == (Some(drag_accept_state::NONE), Some(drag_accept_state::NONE));

    c.assert_behaviour(
        "container.ground.a-pack-over-the-contents-lights-green-and-is-still-refused-on-the-drop",
        move |_| green_on_the_way_in && refused && not_left_marked && hint_came_down,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// container.ground.a-pack-goes-in-through-the-windows-own-strip-of-packs
// ---------------------------------------------------------------------------------------------

/// The gesture that *does* put a pack into the container is its own strip of packs, and it sends
/// the move naming the container.
///
/// It is the other side of the row above -- the same pack, the same window, a different list --
/// and it is also what shows that the clear the drop makes does not eat the drop itself.
pub(super) fn a_pack_goes_into_the_chest_through_its_own_strip() {
    let (mut c, _resident) = a_chest_with_something_in_it();
    let (pack, from) = a_carried_pack(&mut c);
    assert!(
        chest_list(&c, 1).container_list,
        "the premise: the window's strip is a list of packs"
    );

    c.when(Player::Grab(from));
    let onto = {
        let strip = chest_list(&c, 1);
        let h = strip.slots.first().map_or(strip.handle, |s| s.handle);
        let (ui, _root) = gameplay_root(c.app_mut());
        let (x, y) = centre(ui, h);
        Target::Point(ScreenPoint::new(x, y))
    };
    c.when(Player::Over(onto));
    let mark = c.outbound().len();
    c.when(Player::Release);

    let went_in = sent_since(&c, mark).iter().any(
        |r| matches!(r, Request::PutItemInContainer(m) if m.item == pack && m.container == RECORDED_CHEST),
    );
    let not_refused_for_its_kind = c.view().expect_app().interaction().last_refusal.as_deref()
        != Some("Cannot place container in item list");

    c.assert_behaviour(
        "container.ground.a-pack-goes-in-through-the-windows-own-strip-of-packs",
        move |_| went_in && not_refused_for_its_kind,
    );
    c.shutdown();
}
