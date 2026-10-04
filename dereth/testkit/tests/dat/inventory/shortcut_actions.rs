use super::*;

pub(super) const NUM_PLAYER: ObjectId = ObjectId(0x5000_0001);
pub(super) const NUM_PACK: ObjectId = ObjectId(0x5000_0020);
pub(super) const NUM_ROCK: ObjectId = ObjectId(0x5000_0030);
/// Carried and assigned to nothing: the control.
const STICK: ObjectId = ObjectId(0x5000_0031);
/// Worn, so its picture is on the figure rather than in the pack.
pub(super) const WORN_SHIRT: ObjectId = ObjectId(0x5000_0040);
const CHEST_WEAR: u32 = 0x0000_0002;
const SHIRT_SLOT: ElementId = ElementId(0x1000_01E2);

/// A player with a side pack, two loose things and one worn thing -- one of each place an item's
/// picture can be drawn, so "wherever its picture appears" is three different widgets.
fn seed_for_numerals(w: &mut dereth_client_model::World) {
    w.player = Some(NUM_PLAYER);
    w.tables.inventories.insert(
        NUM_PLAYER,
        dereth_client_model::objects::ObjectInventory::new(NUM_PLAYER),
    );
    for id in [NUM_PLAYER, NUM_PACK, NUM_ROCK, STICK] {
        let mut wn = dereth_client_model::Weenie::new(id);
        wn.valid = true;
        wn.pwd.name = format!("obj{:X}", id.0 & 0xFF);
        w.tables.weenies.insert(id, wn);
    }
    {
        let p = w.tables.weenies.get_mut(NUM_PLAYER).expect("seeded");
        p.pwd.items_capacity = Some(102);
        p.pwd.containers_capacity = Some(7);
        p.pwd.bitfield |= dereth_client_model::weenie::bitfield::OPENABLE;
    }
    {
        let c = w.tables.weenies.get_mut(NUM_PACK).expect("seeded");
        c.pwd.items_capacity = Some(24);
        c.pwd.bitfield |= dereth_client_model::weenie::bitfield::OPENABLE;
        c.pwd.container_id = Some(NUM_PLAYER);
    }
    w.tables.inventories.insert(
        NUM_PACK,
        dereth_client_model::objects::ObjectInventory::new(NUM_PACK),
    );
    for id in [NUM_ROCK, STICK] {
        w.tables
            .weenies
            .get_mut(id)
            .expect("seeded")
            .pwd
            .container_id = Some(NUM_PLAYER);
    }
    {
        let inv = w.tables.inventories.get_mut(NUM_PLAYER).expect("seeded");
        inv.add_content(NUM_PACK, true, 0);
        inv.add_content(NUM_ROCK, false, 0);
        inv.add_content(STICK, false, 1);
    }

    let mut wn = dereth_client_model::Weenie::new(WORN_SHIRT);
    wn.valid = true;
    wn.pwd.name = "objSHIRT".to_string();
    wn.pwd.valid_locations = Some(CHEST_WEAR);
    wn.pwd.priority = Some(4);
    wn.pwd.container_id = Some(NUM_PLAYER);
    wn.pwd.wielder_id = Some(NUM_PLAYER);
    wn.pwd.location = Some(CHEST_WEAR);
    w.tables.weenies.insert(WORN_SHIRT, wn);
    w.tables
        .inventories
        .get_mut(NUM_PLAYER)
        .expect("seeded")
        .set_placement(WORN_SHIRT, CHEST_WEAR, 4);
    w.remake_character_inventory();
}

fn assign_shortcut(w: &mut dereth_client_model::World, slot: i32, item: ObjectId) {
    assert!(
        w.player_system.add_shortcut(ShortCutData {
            index: slot,
            object_id: item,
            spell_id: 0
        }),
        "the shortcut was taken"
    );
}

/// A client with three different things in three different shortcut slots, so "the slot number"
/// is measurable rather than a coin toss.
pub(super) fn a_client_with_shortcuts() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    {
        let w = &mut c.app_mut().probe_mut().objects_mut().world;
        seed_for_numerals(w);
        assign_shortcut(w, 0, NUM_ROCK);
        assign_shortcut(w, 3, WORN_SHIRT);
        assign_shortcut(w, 6, NUM_PACK);
    }
    open_pack(&mut c);
    c.tick(2);
    {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        assert!(
            screen
                .inventory
                .container_list
                .as_ref()
                .expect("the strip")
                .slots
                .iter()
                .any(|s| s.item == Some(NUM_PACK)),
            "the premise: the side pack is on the strip"
        );
        let grid = screen.inventory.item_list.as_ref().expect("the grid");
        assert!(
            grid.slots.iter().any(|s| s.item == Some(NUM_ROCK))
                && grid.slots.iter().any(|s| s.item == Some(STICK)),
            "the premise: both loose things are in the grid"
        );
    }
    assert_eq!(
        doll_item(&mut c, SHIRT_SLOT),
        Some(WORN_SHIRT),
        "the premise: the worn thing is drawn on the figure"
    );
    clear_requests(c.ui_outbox());
    c
}

/// For every tile anywhere in the inventory that is showing `item`: the slot number it records,
/// whether the number is actually drawn, and which picture it is drawn from.
fn numerals(c: &mut HeadlessClient, item: ObjectId) -> Vec<(i32, bool, Option<DataId>)> {
    let found: Vec<(i32, Option<ElemHandle>)> = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen
            .inventory
            .lists_mut()
            .flat_map(|w| w.slots.iter())
            .filter(|s| s.item == Some(item))
            .map(|s| (s.shortcut_num, s.shortcut_num_elem))
            .collect()
    };
    let (ui, _root) = gameplay_root(c.app_mut());
    found
        .into_iter()
        .map(|(n, h)| match h {
            Some(h) => (
                n,
                ui.is_visible(h),
                ui.node(h)
                    .and_then(|x| x.region.image.as_ref())
                    .map(|g| g.did),
            ),
            None => (n, false, None),
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// shortcut.number.an-assigned-thing-draws-its-slot-number-wherever-its-picture-appears
// ---------------------------------------------------------------------------------------------

/// A thing in a shortcut slot draws that slot's number on every tile its picture appears in --
/// the pack grid, the side-pack strip and the figure -- and a thing in no slot draws nothing.
///
/// Three different things in three different slots, so a client that drew one number everywhere
/// could not pass, and the three pictures are required to be three different pictures.
pub(super) fn an_assigned_thing_draws_its_slot_number_wherever_its_picture_appears() {
    let mut c = a_client_with_shortcuts();

    let mut seen: Vec<(ObjectId, i32, Vec<(i32, bool, Option<DataId>)>)> = Vec::new();
    for (item, slot) in [(NUM_ROCK, 0), (WORN_SHIRT, 3), (NUM_PACK, 6)] {
        let tiles = numerals(&mut c, item);
        assert!(
            !tiles.is_empty(),
            "the premise: {item:?} has a tile somewhere"
        );
        seen.push((item, slot, tiles));
    }
    let every_tile_agrees = seen.iter().all(|(_, slot, tiles)| {
        tiles
            .iter()
            .all(|(n, drawn, did)| n == slot && *drawn && did.is_some())
    });
    let pictures: Vec<Option<DataId>> = seen.iter().map(|(_, _, t)| t[0].2).collect();
    let three_different_pictures =
        pictures[0] != pictures[1] && pictures[1] != pictures[2] && pictures[0] != pictures[2];

    let unassigned = numerals(&mut c, STICK);
    let draws_nothing =
        !unassigned.is_empty() && unassigned.iter().all(|(n, drawn, _)| *n == -1 && !*drawn);

    c.assert_behaviour(
        "shortcut.number.an-assigned-thing-draws-its-slot-number-wherever-its-picture-appears",
        move |_| every_tile_agrees && three_different_pictures && draws_nothing,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// shortcut.number.it-follows-the-assignment-off-and-on-to-a-different-slot
// ---------------------------------------------------------------------------------------------

/// The number follows the assignment down as well as up: taking the shortcut away takes the
/// number off every tile, and putting the same thing in a **different** slot draws a different
/// number rather than merely drawing one again.
///
/// The down edge is the half that matters. Only the up edge would pass over a client whose
/// number, once drawn, could never come off.
pub(super) fn the_number_follows_the_assignment_off_and_on_again() {
    let mut c = a_client_with_shortcuts();
    let first = numerals(&mut c, NUM_ROCK);
    let started_at_zero = first.first().is_some_and(|t| t.0 == 0);
    let first_picture = first.first().and_then(|t| t.2);

    assert!(
        c.app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .player_system
            .remove_shortcut(0),
        "the shortcut row went away"
    );
    c.tick(2);
    let after_removal = numerals(&mut c, NUM_ROCK);
    let came_off = after_removal
        .iter()
        .all(|(n, drawn, _)| *n == -1 && !*drawn);

    assign_shortcut(
        &mut c.app_mut().probe_mut().objects_mut().world,
        5,
        NUM_ROCK,
    );
    c.tick(2);
    let after = numerals(&mut c, NUM_ROCK);
    let moved = after
        .iter()
        .all(|(n, drawn, did)| *n == 5 && *drawn && *did != first_picture);

    c.assert_behaviour(
        "shortcut.number.it-follows-the-assignment-off-and-on-to-a-different-slot",
        move |_| started_at_zero && first_picture.is_some() && came_off && moved,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// shortcut.number.the-magic-stance-dims-it-everywhere-it-is-drawn
// ---------------------------------------------------------------------------------------------

/// Going into the magic stance dims the shortcut bar, and the number on the item's own picture
/// dims with it -- everywhere the picture is drawn, not only on the bar. The slot has not moved
/// and the number is still drawn; it is drawn from the dimmed picture instead.
pub(super) fn the_magic_stance_dims_the_number_everywhere() {
    let mut c = a_client_with_shortcuts();
    let bright = numerals(&mut c, NUM_ROCK)
        .first()
        .and_then(|t| t.2)
        .expect("the premise");

    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .combat
        .combat_mode = dereth_client_model::combat::CombatMode::from_raw(
        dereth_ui_screens::toolbar::combat_mode::MAGIC,
    );
    c.tick(2);

    let after = numerals(&mut c, NUM_ROCK);
    let dimmed = !after.is_empty()
        && after
            .iter()
            .all(|(n, drawn, did)| *n == 0 && *drawn && *did != Some(bright));

    c.assert_behaviour(
        "shortcut.number.the-magic-stance-dims-it-everywhere-it-is-drawn",
        move |_| dimmed,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// shortcut.number.it-is-drawn-over-the-busy-mark-and-the-selection-ring
// ---------------------------------------------------------------------------------------------

/// The number is the topmost of the three marks a tile can wear, so it is legible on an item
/// that is also selected and also waiting for the shard.
///
/// The order is read out of the live tree and not from the ids, because the ids read like a draw
/// order and are not one: the busy mark is drawn **under** the selection ring although its id is
/// the higher of the two.
pub(super) fn the_number_is_drawn_over_the_busy_mark_and_the_ring() {
    let mut c = a_client_with_shortcuts();
    let tile = pack_slot(&mut c, NUM_ROCK);
    let (ui, _root) = gameplay_root(c.app_mut());
    let overlays = ui
        .get_child_recursive(tile, ElementId(child::OVERLAYS))
        .expect("the tile's marks live together");
    let kids = ui.children(overlays);
    let index_of = |id: u32| {
        let h = ui
            .get_child_recursive(overlays, ElementId(id))
            .unwrap_or_else(|| panic!("{id:#X} is bound"));
        kids.iter()
            .position(|k| *k == h)
            .unwrap_or_else(|| panic!("{id:#X} is a direct child"))
    };
    let selected = index_of(child::SELECTED);
    let ghosted = index_of(child::GHOSTED);
    let numeral = index_of(child::SHORTCUT_NUM);
    let topmost = ghosted < selected && selected < numeral;

    c.assert_behaviour(
        "shortcut.number.it-is-drawn-over-the-busy-mark-and-the-selection-ring",
        move |_| topmost,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// shortcut.number.a-thing-pushed-off-its-tile-keeps-its-shortcut-in-the-next-free-slot
// ---------------------------------------------------------------------------------------------

/// The plate the numeral strip draws for the second row of nine: the frame with no figure in it.
const SECOND_ROW_PLATE: DataId = DataId(0x0600_74D3);

/// Dropping something on an occupied tile does not take the occupant's shortcut away: the
/// occupant moves to the first empty slot to the right of the tile. When the visible row is full
/// to the right that slot is in the hidden second row, and the occupant's picture keeps a plate
/// with no figure on it.
///
/// Both halves are measured: a push into the visible row draws that slot's own number, and a
/// push past it draws the second row's unnumbered plate rather than losing the plate.
pub(super) fn a_thing_pushed_off_its_tile_keeps_its_shortcut_in_the_next_free_slot() {
    let mut c = a_client_with_shortcuts();
    let slot_of = |c: &mut HeadlessClient, item: ObjectId| {
        c.app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .player_system
            .shortcut_slot_of(item)
    };
    let rock_at_zero = numerals(&mut c, NUM_ROCK)
        .first()
        .and_then(|t| t.2)
        .expect("the premise: the rock is numbered");

    // Slot 1 is empty, so the rock moves one to the right and draws slot 1's number.
    let stick = pack_slot(&mut c, STICK);
    drop_on_tile(&mut c, stick, 0);
    c.tick(2);
    let stick_took_zero = slot_of(&mut c, STICK) == Some(0);
    let rock_tiles = numerals(&mut c, NUM_ROCK);
    let pushed_into_view = slot_of(&mut c, NUM_ROCK) == Some(1)
        && !rock_tiles.is_empty()
        && rock_tiles.iter().all(|(n, drawn, did)| {
            *n == 1 && *drawn && did.is_some() && *did != Some(rock_at_zero)
        });

    // Fill the two slots right of the pack's, then drop on the pack's tile: the first empty slot
    // to its right is the first of the hidden row.
    {
        let w = &mut c.app_mut().probe_mut().objects_mut().world;
        assert!(w.player_system.remove_shortcut(1) && w.player_system.remove_shortcut(3));
        assign_shortcut(w, 7, WORN_SHIRT);
        assign_shortcut(w, 8, NUM_ROCK);
    }
    c.tick(2);
    let stick = pack_slot(&mut c, STICK);
    drop_on_tile(&mut c, stick, 6);
    c.tick(2);
    let pack_tiles = numerals(&mut c, NUM_PACK);
    let pushed_past_view = slot_of(&mut c, STICK) == Some(6)
        && slot_of(&mut c, NUM_PACK) == Some(9)
        && !pack_tiles.is_empty()
        && pack_tiles
            .iter()
            .all(|(n, drawn, did)| *n == 9 && *drawn && *did == Some(SECOND_ROW_PLATE));

    c.assert_behaviour(
        "shortcut.number.a-thing-pushed-off-its-tile-keeps-its-shortcut-in-the-next-free-slot",
        move |_| stick_took_zero && pushed_into_view && pushed_past_view,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// Dragging back out: off the shortcut bar, and out of the open container.
//
// Two of the three claims are about the shortcut bar and one is the other half of the chest's
// own drop; they sit together because they are the same gesture read at three different lists.
// ---------------------------------------------------------------------------------------------

/// The shortcut slots this family uses: one to fill and one to move to.
const KIT_SLOT: usize = 7;
const PACK_SLOT: usize = 8;

/// What a shortcut tile is painted with, and whether its grey overlay is up -- the two things
/// the player can see.
fn shortcut_face(c: &mut HeadlessClient, n: usize) -> (Option<DataId>, bool) {
    let (icon, ghost) = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        let s = &screen.shortcuts.slots[n].slots[0];
        (s.icon.expect("the tile's picture element"), s.ghosted)
    };
    let (ui, _root) = gameplay_root(c.app_mut());
    let did = ui
        .node(icon)
        .and_then(|n| n.region.image.as_ref().map(|g| g.did));
    let grey = ghost.is_some_and(|h| ui.node(h).is_some_and(|n| n.region.flags.visible));
    (did, grey)
}

/// What the bar says is in slot `n`.
fn shortcut_holds(c: &mut HeadlessClient, n: usize) -> Option<ObjectId> {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen
        .shortcuts
        .item_at(u32::try_from(n).expect("a slot number"))
}

/// Put a carried thing into shortcut slot `n` the way a player does: a real drag from the pack
/// grid onto the tile.
fn fill_a_shortcut(c: &mut HeadlessClient, n: usize) -> ObjectId {
    let (item, from) = something_to_drag(c);
    let onto = {
        let h = shortcut_tile(c, n);
        point_of(c, h)
    };
    c.when(Player::Grab(from)).when(Player::Drop(onto));
    assert_eq!(
        shortcut_holds(c, n),
        Some(item),
        "the premise: the drag onto the bar put {item:?} in slot {n}"
    );
    item
}

// ---------------------------------------------------------------------------------------------
// shortcut.bar.dragging-one-off-the-bar-empties-its-tile-and-tells-the-shard
// ---------------------------------------------------------------------------------------------

/// Dragging a shortcut off the bar takes it off at the moment it is picked up, puts the tile's
/// numbered plate back, tells the shard, and invents no move for the thing it is let go over.
///
/// All three matter and the last one is the one that makes the rest honest. The icon is let go
/// over the pack grid, which is a list that takes drops; a client that treated the carried
/// shortcut as an ordinary carried thing would ask the shard to move something that never left
/// the pack. The tile is read as the player sees it -- what it is painted with, and whether it
/// is greyed -- because a client that only wrote the model would look identical to the defect.
pub(super) fn dragging_a_shortcut_off_the_bar_empties_its_tile_and_tells_the_shard() {
    let mut c = a_client_at_the_open_chest();
    let item = fill_a_shortcut(&mut c, KIT_SLOT);
    let (filled, grey_before) = shortcut_face(&mut c, KIT_SLOT);
    assert!(
        filled.is_some(),
        "the premise: a filled tile draws the thing's own picture"
    );
    assert!(
        !grey_before,
        "the premise: and nothing is waiting on the shard yet"
    );

    let from = {
        let h = shortcut_tile(&mut c, KIT_SLOT);
        point_of(&mut c, h)
    };
    let (_, onto, _) = own_cell_in_the_clear(&mut c, Pick::ItemList, true);

    // The window starts at the **press**: the bar tells the shard when the icon is picked up,
    // not when it is let go, so a reader that only watched the release would see nothing.
    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Drop(onto));

    let emptied = shortcut_holds(&mut c, KIT_SLOT).is_none();
    let (after, grey_after) = shortcut_face(&mut c, KIT_SLOT);
    let plate_is_back = after != filled && !grey_after;
    let sent = sent_since(&c, mark);
    let told_the_shard = sent.iter().any(
        |r| matches!(r, Request::RemoveShortCut(m) if usize::try_from(m.index).ok() == Some(KIT_SLOT)),
    );
    let invented_no_move = !sent
        .iter()
        .any(|r| matches!(r, Request::PutItemInContainer(m) if m.item == item));

    c.assert_behaviour(
        "shortcut.bar.dragging-one-off-the-bar-empties-its-tile-and-tells-the-shard",
        move |_| emptied && plate_is_back && told_the_shard && invented_no_move,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// shortcut.bar.dragging-one-tile-to-tile-moves-it-and-tells-the-shard-both-halves
// ---------------------------------------------------------------------------------------------

/// Dragging a shortcut from one tile to another moves it: the tile it left goes back to its
/// plate, the tile it arrived on draws the picture, and the shard is told **both** halves.
///
/// Neither half alone survives the next login, which is what the player would notice: one
/// message says where it was and the other says where it is, and the arrival is sent exactly
/// once however many handlers saw the release.
pub(super) fn dragging_a_shortcut_tile_to_tile_moves_it_and_tells_the_shard_both_halves() {
    let mut c = a_client_at_the_open_chest();
    let item = fill_a_shortcut(&mut c, KIT_SLOT);
    let (filled, _) = shortcut_face(&mut c, KIT_SLOT);
    let (empty_plate, _) = shortcut_face(&mut c, PACK_SLOT);

    let from = {
        let h = shortcut_tile(&mut c, KIT_SLOT);
        point_of(&mut c, h)
    };
    let onto = {
        let h = shortcut_tile(&mut c, PACK_SLOT);
        point_of(&mut c, h)
    };

    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Drop(onto));

    let moved = shortcut_holds(&mut c, PACK_SLOT) == Some(item)
        && shortcut_holds(&mut c, KIT_SLOT).is_none();
    let (from_now, _) = shortcut_face(&mut c, KIT_SLOT);
    let (onto_now, onto_grey) = shortcut_face(&mut c, PACK_SLOT);
    let both_tiles_followed =
        from_now != filled && onto_now != empty_plate && onto_now == filled && !onto_grey;

    let sent = sent_since(&c, mark);
    let said_where_it_was = sent.iter().any(
        |r| matches!(r, Request::RemoveShortCut(m) if usize::try_from(m.index).ok() == Some(KIT_SLOT)),
    );
    let arrivals = sent
        .iter()
        .filter(|r| {
            matches!(r, Request::AddShortCut(m)
                if m.shortcut.object_id == item
                    && usize::try_from(m.shortcut.index).ok() == Some(PACK_SLOT))
        })
        .count();

    c.assert_behaviour(
        "shortcut.bar.dragging-one-tile-to-tile-moves-it-and-tells-the-shard-both-halves",
        move |_| moved && both_tiles_followed && said_where_it_was && arrivals == 1,
    );
    c.shutdown();
}

// =============================================================================================
// The shortcut bar: what a drop on a tile does, and what its number key does.
//
// The gesture under test: put a healing kit on tile 8 and a backpack on tile 9, then press 8
// and 9 to heal.
//
// **These run on a made-up pack.** Nothing claimed turns on which things the character carries,
// and a recorded character's own bar starts empty exactly as a made-up one does. The last claim
// arms the pointer with the Examine button instead of a healing kit, because no object in
// `long-solo-play` can arm the use-on-something cursor at all; it arms the same field through the
// same production producer and says so at the line.
//
// The keys go in at the **key**, not at the action: the tile's number key is pressed through the
// client's own input manager and the shipped keymap's binding for that tile is asserted with it,
// because a press that injected the action would be asserting only half the chain.
// =============================================================================================

/// The two tiles of the gesture, counting from zero: "8" and "9" on the bar.
const KIT_TILE: u32 = 7;
const PACK_TILE: u32 = 8;

/// What the shipped keymap calls "use what is in tile `n`".
fn use_tile_action(n: u32) -> dereth_input::ActionId {
    dereth_input::ActionId(0x1000_0042 + n)
}

/// The input map the tile keys live on.
const QUICKSLOT_COMMANDS: dereth_input::InputMapId = dereth_input::InputMapId(0x1000_000C);

/// What one tile of the bar is holding.
fn tile_holds(c: &mut HeadlessClient, n: u32) -> Option<ObjectId> {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen.shortcuts.item_at(n)
}

/// The picture one tile is drawing. An empty tile draws the numbered plate the bar is shipped
/// with, which is what a refused drop leaves unchanged.
fn tile_picture(c: &mut HeadlessClient, n: u32) -> Option<DataId> {
    let icon = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen.shortcuts.slots[n as usize].slots[0]
            .icon
            .expect("the tile draws a picture")
    };
    let (ui, _screen) = gameplay_screen(c.app_mut());
    ui.node(icon)
        .and_then(|n| n.region.image.as_ref().map(|g| g.did))
}

/// Carry what is drawn at `from` onto tile `n` and let it go there.
fn drop_on_tile(c: &mut HeadlessClient, from: ElemHandle, n: u32) {
    let tile = shortcut_tile(c, n as usize);
    let from = point_of(c, from);
    c.when(Grab(from));
    let at = carry_over(c, tile);
    c.when(LetGo(at));
    clear_requests(c.ui_outbox());
}

/// Press the key the **shipped keymap** binds to "use what is in tile `n`".
pub(super) fn press_tile_key(c: &mut HeadlessClient, n: u32, code: winit::keyboard::KeyCode) {
    let key =
        dereth_client::platform::window::key_from_key_code(code).expect("the host names this key");
    dereth_testkit::input_steps::press_bound(c, use_tile_action(n), QUICKSLOT_COMMANDS, key);
}

// ---------------------------------------------------------------------------------------------
// inventory.shortcut-bar.a-thing-dropped-on-a-tile-fills-it-and-tells-the-shard-once
// ---------------------------------------------------------------------------------------------

/// A thing carried onto a tile of the shortcut bar goes into that tile, the tile stops drawing
/// its empty numbered plate and draws the thing, and the shard is told once.
///
/// Both halves are asserted because the client does both under one condition: a build that only
/// kept the shortcut locally would lose the bar at the next login, and one that only told the
/// shard would leave the tile looking empty in front of the player. **Once** is part of it --
/// two handlers see the release, and a second telling would put the shortcut in twice.
pub(super) fn a_thing_dropped_on_a_tile_fills_it_and_tells_the_shard_once() {
    use dereth_protocol::Message as _;
    // The one place the two message numbers this subject turns on are written down, so that a
    // rename cannot quietly swap them under every assertion that reads them through a symbol.
    assert_eq!(
        dereth_protocol::login::CharacterAddShortCut::OPCODE,
        dereth_protocol::Opcode(0x019C)
    );
    assert_eq!(
        dereth_protocol::login::CharacterRemoveShortCut::OPCODE,
        dereth_protocol::Opcode(0x019D)
    );

    let mut c = a_client_with_three_things_and_a_pack();
    let starts_empty = tile_holds(&mut c, KIT_TILE).is_none();
    let plate = tile_picture(&mut c, KIT_TILE);

    let item = HINT_DRAG_ITEMS[0];
    let from = grid_cell(&mut c, 0);
    let mark = c.outbound().len();
    drop_on_tile(&mut c, from, KIT_TILE);

    let held = tile_holds(&mut c, KIT_TILE) == Some(item);
    let repainted = {
        let after = tile_picture(&mut c, KIT_TILE);
        after.is_some() && after != plate
    };
    let told: Vec<(ObjectId, u32, u32)> = c.outbound()[mark..]
        .iter()
        .filter_map(|r| match r {
            Request::AddShortCut(m) => Some((
                m.shortcut.object_id,
                u32::try_from(m.shortcut.index).unwrap_or(u32::MAX),
                m.shortcut.spell_id,
            )),
            _ => None,
        })
        .collect();

    c.assert_behaviour(
        "inventory.shortcut-bar.a-thing-dropped-on-a-tile-fills-it-and-tells-the-shard-once",
        move |_| starts_empty && held && repainted && told == vec![(item, KIT_TILE, 0)],
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.shortcut-bar.a-pack-can-be-put-on-a-tile-like-anything-else
// ---------------------------------------------------------------------------------------------

/// The other half of the gesture: a pack goes on a tile by exactly the same drag, and nothing about
/// being a container refuses it.
pub(super) fn a_pack_can_be_put_on_a_tile_like_anything_else() {
    let mut c = a_client_with_three_things_and_a_pack();
    let from = strip_cell(&mut c, 0);
    let mark = c.outbound().len();
    drop_on_tile(&mut c, from, PACK_TILE);

    let held = tile_holds(&mut c, PACK_TILE) == Some(HINT_DRAG_PACK);
    let told = c.outbound()[mark..].iter().any(|r| {
        matches!(r, Request::AddShortCut(m)
            if m.shortcut.object_id == HINT_DRAG_PACK
                && u32::try_from(m.shortcut.index).ok() == Some(PACK_TILE))
    });

    c.assert_behaviour(
        "inventory.shortcut-bar.a-pack-can-be-put-on-a-tile-like-anything-else",
        move |_| held && told,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.shortcut-bar.the-number-key-of-a-filled-tile-uses-what-is-in-it
// ---------------------------------------------------------------------------------------------

/// Pressing the number key of a filled tile uses the thing in that tile.
///
/// The key is the shipped one and the binding is asserted with the press, so this is the whole
/// chain: the key, the keymap, the bar's own listener, and the use. Which exit the use takes
/// belongs to the thing being used and is not this claim's to choose, so what is required is that
/// the press reached the use path naming that thing.
pub(super) fn the_number_key_of_a_filled_tile_uses_what_is_in_it() {
    let mut c = a_client_with_three_things_and_a_pack();
    // The thing in the tile has to be usable, or a press that reached the use path and a press
    // that reached nothing would read alike.
    let item = HINT_DRAG_ITEMS[0];
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .weenie_mut(item)
        .expect("seeded")
        .pwd
        .useability = Some(0x20);
    c.tick(1);

    let from = grid_cell(&mut c, 0);
    drop_on_tile(&mut c, from, KIT_TILE);
    let filled = tile_holds(&mut c, KIT_TILE) == Some(item);

    let mark = c.outbound().len();
    press_tile_key(&mut c, KIT_TILE, winit::keyboard::KeyCode::Digit8);
    let used = c.outbound()[mark..].iter().any(|r| match r {
        Request::UseEvent(m) => m.object == item,
        Request::UseWithTargetEvent(m) => m.object == item,
        Request::PutItemInContainer(m) => m.item == item,
        Request::GetAndWieldItem(m) => m.item == item,
        _ => false,
    }) || c.view().interaction().stats.target_modes_armed > 0;
    clear_requests(c.ui_outbox());

    c.assert_behaviour(
        "inventory.shortcut-bar.the-number-key-of-a-filled-tile-uses-what-is-in-it",
        move |_| filled && used,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.shortcut-bar.a-number-key-pressed-with-a-cursor-armed-finishes-that-gesture
// ---------------------------------------------------------------------------------------------

/// The gesture's second half: a tile's key pressed while the pointer is already armed for
/// something **finishes** that gesture with the thing in the tile, instead of starting a new
/// one -- and it disarms the pointer afterwards, so the next press is an ordinary use again.
///
/// That is what makes "8 then 9" heal the player rather than open their pack, and it is one
/// branch and not two: the bar reads whether the pointer is armed before it can reach the use at
/// all.
///
/// **The pointer is armed with the toolbar's own Examine button, and the reason is measured.**
/// The branch, the call it makes and the disarming afterwards are the same whichever gesture is
/// pending, and the examine one is the only one that can be told apart here: a build with the
/// defect answers an armed use-on-something press with a plain use of the pack, which is what a
/// correct build would send as well.
pub(super) fn a_number_key_pressed_with_a_cursor_armed_finishes_that_gesture() {
    let mut c = a_client_with_three_things_and_a_pack();
    let from = strip_cell(&mut c, 0);
    drop_on_tile(&mut c, from, PACK_TILE);
    let filled = tile_holds(&mut c, PACK_TILE) == Some(HINT_DRAG_PACK);

    // The drag selected the tile it started on, and the Examine button only arms the pointer
    // when nothing is selected -- with something selected it appraises that instead.
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .set_selected_object(
            None,
            false,
            &mut dereth_client_model::RecordingSink::default(),
        );
    c.tick(1);
    let nothing_selected = c.view().world().selected.is_none();

    c.when(Player::click(
        dereth_ui_screens::toolbar::target_mode::EXAMINE_BUTTON,
    ));
    clear_requests(c.ui_outbox());
    let armed =
        c.view().interaction().target_mode() != dereth_client::interaction::TargetMode::None;

    let mark = c.outbound().len();
    press_tile_key(&mut c, PACK_TILE, winit::keyboard::KeyCode::Digit9);
    let after: Vec<Request> = c.outbound()[mark..].to_vec();
    let finished_the_gesture = after
        .iter()
        .any(|r| matches!(r, Request::Appraise(m) if m.target == HINT_DRAG_PACK));
    let not_a_plain_use = !after.iter().any(|r| {
        matches!(r, Request::UseEvent(m) if m.object == HINT_DRAG_PACK)
            || matches!(r, Request::PutItemInContainer(m) if m.item == HINT_DRAG_PACK)
    });

    // ...and the pointer is disarmed, so a second press is an ordinary use again.
    let mark = c.outbound().len();
    press_tile_key(&mut c, PACK_TILE, winit::keyboard::KeyCode::Digit9);
    let disarmed = !c.outbound()[mark..]
        .iter()
        .any(|r| matches!(r, Request::Appraise(m) if m.target == HINT_DRAG_PACK));
    clear_requests(c.ui_outbox());

    c.assert_behaviour(
        "inventory.shortcut-bar.a-number-key-pressed-with-a-cursor-armed-finishes-that-gesture",
        move |_| {
            filled
                && nothing_selected
                && armed
                && finished_the_gesture
                && not_a_plain_use
                && disarmed
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.shortcut-bar.a-kit-key-then-the-main-pack-key-uses-the-kit-on-the-player
// ---------------------------------------------------------------------------------------------

/// A healing kit on tile 8 and the player's own main pack on tile 9: pressing 8 asks for a
/// target and leaves the pointer armed, and pressing 9 then uses the kit on the player.
///
/// **The client is brought into the game from the character wizard**, which is the path a new
/// character takes and the one that broke it: a screen that has gone must not go on hearing the
/// keys, or the gameplay screen hears each press twice and the second hearing of 8 finishes the
/// gesture 8 itself started, on the kit.
pub(super) fn a_kit_key_then_the_main_pack_key_uses_the_kit_on_the_player() {
    use dereth_client_model::weenie::item_type;
    let mut c = HeadlessClient::new(ClientSpec::screen(dereth_ui::framework::mode::CHAR_GEN, 8));
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    c.tick(8);
    let came_from_the_wizard = c.app_mut().ui_mut().and_then(|s| s.flow.current_mode())
        == Some(dereth_ui::framework::mode::GAME_PLAY);
    seed_three_things_and_a_pack(&mut c.app_mut().probe_mut().objects_mut().world);
    let kit = HINT_DRAG_ITEMS[0];
    {
        let w = &mut c.app_mut().probe_mut().objects_mut().world;
        // The shipped healing kit: carried, and used on a creature, the player included.
        let k = w.weenie_mut(kit).expect("seeded");
        k.pwd.name = "Healing Kit".into();
        k.pwd.useability = Some(0x0022_0008);
        k.pwd.target_type = Some(item_type::CREATURE);
        k.pwd.obj_type = item_type::MISC;
        let me = w.weenie_mut(HINT_DRAG_PLAYER).expect("seeded");
        me.pwd.obj_type = item_type::CREATURE;
    }
    open_pack(&mut c);
    c.tick(2);

    let from = grid_cell(&mut c, 0);
    drop_on_tile(&mut c, from, KIT_TILE);
    let main_pack = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen
            .inventory
            .top_container
            .as_ref()
            .expect("the main pack's own row")
            .slots[0]
            .handle
    };
    drop_on_tile(&mut c, main_pack, PACK_TILE);
    let tiles = (tile_holds(&mut c, KIT_TILE), tile_holds(&mut c, PACK_TILE))
        == (Some(kit), Some(HINT_DRAG_PLAYER));
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .set_selected_object(
            None,
            false,
            &mut dereth_client_model::RecordingSink::default(),
        );
    c.tick(1);

    let mark = c.outbound().len();
    press_tile_key(&mut c, KIT_TILE, winit::keyboard::KeyCode::Digit8);
    let armed =
        c.view().interaction().target_mode() == dereth_client::interaction::TargetMode::UseTarget;
    let eight_sent_nothing = c.outbound().len() == mark;

    let mark = c.outbound().len();
    press_tile_key(&mut c, PACK_TILE, winit::keyboard::KeyCode::Digit9);
    let after: Vec<Request> = c.outbound()[mark..].to_vec();
    let used_on_me = after.iter().any(|r| {
        matches!(r, Request::UseWithTargetEvent(m)
            if m.object == kit && m.target == HINT_DRAG_PLAYER)
    });
    let disarmed =
        c.view().interaction().target_mode() == dereth_client::interaction::TargetMode::None;
    clear_requests(c.ui_outbox());

    c.assert_behaviour(
        "inventory.shortcut-bar.a-kit-key-then-the-main-pack-key-uses-the-kit-on-the-player",
        move |_| {
            came_from_the_wizard && tiles && armed && eight_sent_nothing && used_on_me && disarmed
        },
    );
    c.shutdown();
}
