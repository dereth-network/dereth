use super::*;

// =============================================================================================
// The grey mark a tile wears while the thing on it is busy.
//
// The failure these guard against: dragging a side pack left its icon greyed for the rest of
// the session.
//
// The mark is read **off the element**, not off the panel's record of it, because that is what
// the player looks at: a fix that only moved a flag would pass a scenario that read the
// flag. And every scenario here asserts the mark went **up** before it asserts it came down --
// an instrument that cannot see the thing cannot report its absence.
// =============================================================================================

const BUSY_PLAYER: ObjectId = ObjectId(0x5000_0001);
/// The side pack that is dragged. It is a container, so it is drawn on the strip.
const SIDE_PACK: ObjectId = ObjectId(0x5000_0020);
/// A second one, so the strip has somewhere to drop onto.
const OTHER_PACK: ObjectId = ObjectId(0x5000_0021);
/// A loose thing in the grid, for the control.
const LOOSE: ObjectId = ObjectId(0x5000_0030);
/// A second loose thing, the physical source of the catcher scenario.
const SECOND_LOOSE: ObjectId = ObjectId(0x5000_0031);

fn seed_two_packs(w: &mut dereth_client_model::World) {
    w.player = Some(BUSY_PLAYER);
    w.tables.inventories.insert(
        BUSY_PLAYER,
        dereth_client_model::objects::ObjectInventory::new(BUSY_PLAYER),
    );
    for id in [BUSY_PLAYER, SIDE_PACK, OTHER_PACK, LOOSE] {
        let mut wn = dereth_client_model::Weenie::new(id);
        wn.valid = true;
        wn.pwd.name = format!("obj{:X}", id.0 & 0xFF);
        w.tables.weenies.insert(id, wn);
    }
    {
        let p = w.tables.weenies.get_mut(BUSY_PLAYER).expect("seeded");
        p.pwd.items_capacity = Some(102);
        p.pwd.containers_capacity = Some(7);
        p.pwd.bitfield |= dereth_rules::weenie::bitfield::OPENABLE;
    }
    for pack in [SIDE_PACK, OTHER_PACK] {
        let c = w.tables.weenies.get_mut(pack).expect("seeded");
        c.pwd.items_capacity = Some(24);
        c.pwd.containers_capacity = Some(0);
        c.pwd.bitfield |= dereth_rules::weenie::bitfield::OPENABLE;
        c.pwd.container_id = Some(BUSY_PLAYER);
        w.tables.inventories.insert(
            pack,
            dereth_client_model::objects::ObjectInventory::new(pack),
        );
    }
    w.tables
        .weenies
        .get_mut(LOOSE)
        .expect("seeded")
        .pwd
        .container_id = Some(BUSY_PLAYER);
    let inv = w.tables.inventories.get_mut(BUSY_PLAYER).expect("seeded");
    inv.add_content(SIDE_PACK, true, 0);
    inv.add_content(OTHER_PACK, true, 1);
    inv.add_content(LOOSE, false, 0);
}

fn a_client_with_two_side_packs() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    seed_two_packs(&mut c.app_mut().probe_mut().objects_mut().world);
    open_pack(&mut c);
    c.tick(2);
    {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        let strip: Vec<ObjectId> = screen
            .inventory
            .container_list
            .as_ref()
            .expect("the strip")
            .slots
            .iter()
            .filter_map(|s| s.item)
            .collect();
        assert_eq!(
            strip,
            vec![SIDE_PACK, OTHER_PACK],
            "the premise: both packs are on the strip"
        );
    }
    clear_requests(c.ui_outbox());
    c
}

/// The tile showing `item`, wherever in the inventory it is.
fn any_tile_of(c: &mut HeadlessClient, item: ObjectId) -> ElemHandle {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen
        .inventory
        .lists_mut()
        .find_map(|w| {
            w.slots
                .iter()
                .find(|s| s.item == Some(item))
                .map(|s| s.handle)
        })
        .unwrap_or_else(|| panic!("{item:?} has a tile somewhere in the inventory"))
}

/// **The observable.** Is the grey mark actually drawn on any tile showing `item`?
fn mark_is_up(c: &mut HeadlessClient, item: ObjectId) -> bool {
    let marks: Vec<ElemHandle> = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen
            .inventory
            .lists_mut()
            .flat_map(|w| w.slots.iter())
            .filter(|s| s.item == Some(item))
            .filter_map(|s| s.ghosted)
            .collect()
    };
    let (ui, _root) = gameplay_root(c.app_mut());
    marks.iter().any(|h| ui.is_visible(*h))
}

fn thing_is_busy(c: &mut HeadlessClient, item: ObjectId) -> bool {
    c.app_mut()
        .objects()
        .world
        .weenie(item)
        .is_some_and(|w| w.waiting)
}

/// Both halves gone: the element the player looks at, and the thing's own state.
fn mark_is_gone(c: &mut HeadlessClient, item: ObjectId) -> bool {
    !mark_is_up(c, item) && !thing_is_busy(c, item)
}

/// Pick `item` up off whatever tile it is on, and require that the mark went up -- the
/// instrument cannot report absence if it cannot see presence.
fn pick_up_and_see_the_mark(c: &mut HeadlessClient, item: ObjectId) -> ElemHandle {
    let tile = any_tile_of(c, item);
    let from = point_of(c, tile);
    c.when(Grab(from));
    assert!(
        mark_is_up(c, item),
        "the premise: picking {item:?} up greys its tile"
    );
    tile
}

/// A corner of the screen that is over no drop target at all.
const OFF_EVERYTHING: Target = Target::Point(ScreenPoint::new(2, 2));

/// Every move the client has asked for, over the whole scenario.
fn moves(c: &HeadlessClient) -> Vec<(ObjectId, ObjectId, u32)> {
    c.outbound()
        .iter()
        .filter_map(|r| match r {
            Request::PutItemInContainer(m) => Some((m.item, m.container, m.slot)),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// inventory.busy-mark.a-drag-that-ends-over-nothing-takes-it-off
// ---------------------------------------------------------------------------------------------

/// Letting a drag go where nothing can catch it takes the grey mark back off -- on the side-pack
/// strip, which is the window the stuck grey mark showed on, and on the pack grid too.
///
/// Both, in one scenario, because the mechanism is shared: a regression in it should show up as
/// one red rather than as an argument about which list is at fault.
pub(super) fn a_drag_that_ends_over_nothing_takes_the_mark_off() {
    let mut c = a_client_with_two_side_packs();
    pick_up_and_see_the_mark(&mut c, SIDE_PACK);
    c.when(LetGo(OFF_EVERYTHING));
    clear_requests(c.ui_outbox());
    let the_pack_is_clean = mark_is_gone(&mut c, SIDE_PACK);
    c.shutdown();

    // The control, on its own client: the pack grid is the family that already had this pass, so
    // a regression in the shared mechanism should show as one red and not as an argument about
    // which list is at fault.
    let mut c = a_client_with_two_side_packs();
    pick_up_and_see_the_mark(&mut c, LOOSE);
    c.when(LetGo(OFF_EVERYTHING));
    clear_requests(c.ui_outbox());
    let the_loose_thing_is_clean = mark_is_gone(&mut c, LOOSE);

    c.assert_behaviour(
        "inventory.busy-mark.a-drag-that-ends-over-nothing-takes-it-off",
        { move |_| the_pack_is_clean && the_loose_thing_is_clean },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.busy-mark.a-drop-that-moves-nothing-takes-it-off
// ---------------------------------------------------------------------------------------------

/// Dropping something back on the very slot it came from moves nothing, and takes the mark off.
pub(super) fn a_drop_that_moves_nothing_takes_the_mark_off() {
    let mut c = a_client_with_two_side_packs();
    let tile = pick_up_and_see_the_mark(&mut c, SIDE_PACK);
    let at = carry_over(&mut c, tile);
    c.when(LetGo(at));
    clear_requests(c.ui_outbox());
    let clean = mark_is_gone(&mut c, SIDE_PACK);

    c.assert_behaviour(
        "inventory.busy-mark.a-drop-that-moves-nothing-takes-it-off",
        { move |_| clean },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.busy-mark.a-drop-on-the-shortcut-bar-takes-it-off
// ---------------------------------------------------------------------------------------------

/// Dropping something on the shortcut bar makes the shortcut and leaves no grey behind, because
/// nothing was asked of the shard: a shortcut is a reference and the thing has not moved.
pub(super) fn a_drop_on_the_shortcut_bar_takes_the_mark_off() {
    let mut c = a_client_with_two_side_packs();
    pick_up_and_see_the_mark(&mut c, SIDE_PACK);
    let tile = shortcut_tile(&mut c, 0);
    let at = carry_over(&mut c, tile);
    c.when(LetGo(at));
    clear_requests(c.ui_outbox());
    let clean = mark_is_gone(&mut c, SIDE_PACK);

    c.assert_behaviour(
        "inventory.busy-mark.a-drop-on-the-shortcut-bar-takes-it-off",
        { move |_| clean },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.busy-mark.the-shards-refusal-and-the-shards-move-both-take-it-off
// ---------------------------------------------------------------------------------------------

/// The two answers a shard can give to a move both take the mark off: the refusal, and the
/// confirmation. The confirmation is the harder half, because the tile the mark has to come off
/// is not the tile it went on to -- the thing has moved.
pub(super) fn the_shards_two_answers_both_take_the_mark_off() {
    let mut c = a_client_with_two_side_packs();
    // The drop is made on the toolbar's backpack button, because that is the gesture that really
    // asks the shard for something: a drop that the client resolves to no move at all would
    // clear the mark by itself and leave the shard's answer with nothing to do.
    pick_up_and_see_the_mark(&mut c, SIDE_PACK);
    let button = shipped(&mut c, INVENTORY_BUTTON);
    let at = carry_over(&mut c, button);
    c.when(LetGo(at));
    clear_requests(c.ui_outbox());
    let still_marked_while_waiting = mark_is_up(&mut c, SIDE_PACK);
    {
        let mut out = dereth_client_model::RecordingSink::default();
        c.app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .server_says_attempt_failed(SIDE_PACK, 0, &mut out);
    }
    c.tick(2);
    let the_refusal_clears_it = mark_is_gone(&mut c, SIDE_PACK);

    // And the confirmation, from the same state -- this time the thing really moves, so the tile
    // the mark has to come off is not the tile it went on to.
    pick_up_and_see_the_mark(&mut c, SIDE_PACK);
    let button = shipped(&mut c, INVENTORY_BUTTON);
    let at = carry_over(&mut c, button);
    c.when(LetGo(at));
    clear_requests(c.ui_outbox());
    let marked_again = mark_is_up(&mut c, SIDE_PACK);
    {
        let mut out = dereth_client_model::RecordingSink::default();
        c.app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .server_says_move_item(SIDE_PACK, OTHER_PACK, 0, ObjectId(0), 0, true, &mut out);
    }
    c.tick(2);
    let the_move_clears_it = mark_is_gone(&mut c, SIDE_PACK);

    c.assert_behaviour(
        "inventory.busy-mark.the-shards-refusal-and-the-shards-move-both-take-it-off",
        move |_| {
            still_marked_while_waiting
                && the_refusal_clears_it
                && marked_again
                && the_move_clears_it
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.busy-mark.every-tile-mirrors-the-things-own-state
// ---------------------------------------------------------------------------------------------

/// The mark is not something a list remembers: every tile reads it off the thing itself, every
/// frame, so a list that is emptied and refilled comes back in the state the **thing** is in --
/// neither silently losing the mark nor keeping a stale one.
///
/// Driven both ways with the thing's own state and no pointer at all, so the answer cannot have
/// come from the gesture.
pub(super) fn every_tile_mirrors_the_things_own_state() {
    let mut c = a_client_with_two_side_packs();
    let clean_to_start = mark_is_gone(&mut c, SIDE_PACK);

    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .set_waiting_state(SIDE_PACK, true);
    c.tick(2);
    let follows_it_up = mark_is_up(&mut c, SIDE_PACK);

    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .set_waiting_state(SIDE_PACK, false);
    c.tick(2);
    let follows_it_down = mark_is_gone(&mut c, SIDE_PACK);

    c.assert_behaviour(
        "inventory.busy-mark.every-tile-mirrors-the-things-own-state",
        { move |_| clean_to_start && follows_it_up && follows_it_down },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.busy-mark.a-thing-the-shard-destroys-leaves-no-tile-and-no-mark
// ---------------------------------------------------------------------------------------------

/// A thing destroyed while it is marked leaves no tile behind, and therefore no mark: the tile
/// going away with it is the only thing that takes the grey off a destroyed thing.
pub(super) fn a_destroyed_thing_leaves_no_tile_and_no_mark() {
    let mut c = a_client_with_two_side_packs();
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .set_waiting_state(SIDE_PACK, true);
    c.tick(2);
    let marked_first = mark_is_up(&mut c, SIDE_PACK);

    {
        let mut out = dereth_client_model::RecordingSink::default();
        c.app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .server_says_remove(SIDE_PACK, ServerTime(0.0), &mut out);
    }
    c.tick(2);
    let no_tile = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        !screen
            .inventory
            .lists_mut()
            .flat_map(|w| w.slots.iter())
            .any(|s| s.item == Some(SIDE_PACK))
    };
    let no_mark = !mark_is_up(&mut c, SIDE_PACK);

    c.assert_behaviour(
        "inventory.busy-mark.a-thing-the-shard-destroys-leaves-no-tile-and-no-mark",
        move |_| marked_first && no_tile && no_mark,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.busy-mark.a-tile-clears-its-own-before-catching-another-thing
// ---------------------------------------------------------------------------------------------

/// A tile that is itself waiting on an earlier request clears its **own** mark when it catches a
/// drop, while the thing that was dropped keeps its mark and goes on waiting for the shard.
///
/// They are two different messages to the same tile, and a client that answered them with one
/// would either un-grey the thing it has just asked about or leave the catcher grey for ever.
/// The catcher's wait is not written by hand: it comes from a real part-of-a-stack wield, which
/// takes no one-at-a-time lock, so the drag that follows is a gesture the player could make.
pub(super) fn a_tile_clears_its_own_before_catching_another_thing() {
    let mut c = a_client_with_two_side_packs();
    let ammunition = dereth_rules::slots::loc::MISSILE_AMMO;
    {
        let w = &mut c.app_mut().probe_mut().objects_mut().world;
        let stack = w.weenie_mut(LOOSE).expect("seeded");
        stack.pwd.valid_locations = Some(ammunition);
        stack.pwd.stack_size = Some(2);
        stack.pwd.max_stack_size = Some(2);
        let mut second = dereth_client_model::Weenie::new(SECOND_LOOSE);
        second.valid = true;
        second.pwd.name = "drop source".into();
        second.pwd.container_id = Some(BUSY_PLAYER);
        w.tables.weenies.insert(SECOND_LOOSE, second);
        w.tables
            .inventories
            .get_mut(BUSY_PLAYER)
            .expect("seeded")
            .add_content(SECOND_LOOSE, false, 1);
    }
    c.tick(2);
    {
        let mut asked = dereth_client_model::RecordingRequests::default();
        let mut notices = dereth_client_model::RecordingSink::default();
        c.app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .attempt_wield(
                &mut asked,
                &mut notices,
                LOOSE,
                ammunition,
                dereth_client_model::inventory::SplitState {
                    split_size: 1,
                    max_split_size: 2,
                },
                ServerTime(0.8),
                false,
            )
            .expect("the earlier part-of-a-stack wield is accepted");
        assert!(
            matches!(
                asked.0.as_slice(),
                [Request::StackableSplitToWield(m)] if m.stack == LOOSE && m.amount == 1
            ),
            "the premise: the catcher's wait came from a real request"
        );
    }
    assert!(
        c.app_mut().objects().world.request_lock.is_idle(),
        "the premise: that wield takes no one-at-a-time lock, so the drag below is reachable"
    );
    c.tick(2);
    let catcher_is_marked = mark_is_up(&mut c, LOOSE);

    pick_up_and_see_the_mark(&mut c, SECOND_LOOSE);
    let catcher = any_tile_of(&mut c, LOOSE);
    let at = carry_over(&mut c, catcher);
    c.when(LetGo(at));
    clear_requests(c.ui_outbox());

    let sent = moves(&c);
    let one_move = sent
        .iter()
        .filter(|(item, _, _)| *item == SECOND_LOOSE)
        .count()
        == 1;
    let into_the_players_own = sent
        .iter()
        .any(|(item, container, _)| *item == SECOND_LOOSE && *container == BUSY_PLAYER);
    let the_dropped_one_waits =
        mark_is_up(&mut c, SECOND_LOOSE) && thing_is_busy(&mut c, SECOND_LOOSE);
    let the_catcher_is_clean = mark_is_gone(&mut c, LOOSE);

    c.assert_behaviour(
        "inventory.busy-mark.a-tile-clears-its-own-before-catching-another-thing",
        move |_| {
            catcher_is_marked
                && one_move
                && into_the_players_own
                && the_dropped_one_waits
                && the_catcher_is_clean
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.backpack-button.a-drop-sends-one-move-and-keeps-the-mark-until-the-answer
// ---------------------------------------------------------------------------------------------

/// The big backpack button on the toolbar is a drop target: letting something go on it asks the
/// shard once to put it in the player's own pack, and the mark deliberately stays until the
/// shard answers, because this time something really was asked.
pub(super) fn the_backpack_button_sends_one_move_and_keeps_the_mark() {
    let mut c = a_client_with_two_side_packs();
    pick_up_and_see_the_mark(&mut c, SIDE_PACK);
    let button = shipped(&mut c, INVENTORY_BUTTON);
    let at = carry_over(&mut c, button);
    c.when(LetGo(at));
    clear_requests(c.ui_outbox());

    let sent = moves(&c);
    let one_move = sent
        .iter()
        .filter(|(item, _, _)| *item == SIDE_PACK)
        .count()
        == 1;
    let into_the_players_own = sent.iter().any(|(item, container, slot)| {
        *item == SIDE_PACK && *container == BUSY_PLAYER && *slot == 0
    });
    let still_marked = mark_is_up(&mut c, SIDE_PACK) && thing_is_busy(&mut c, SIDE_PACK);

    c.assert_behaviour(
        "inventory.backpack-button.a-drop-sends-one-move-and-keeps-the-mark-until-the-answer",
        move |_| one_move && into_the_players_own && still_marked,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.backpack-button.a-refused-drop-sends-nothing-and-takes-the-mark-off
// ---------------------------------------------------------------------------------------------

/// The same drop while the player is already waiting on something else sends nothing, leaves the
/// request they were waiting on alone, and takes the mark off -- nothing was asked, so nothing
/// is pending.
pub(super) fn a_refused_backpack_drop_sends_nothing_and_takes_the_mark_off() {
    let mut c = a_client_with_two_side_packs();
    pick_up_and_see_the_mark(&mut c, SIDE_PACK);
    let already_waiting_on = ObjectId(0x5000_00EE);
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .request_lock
        .record(
            already_waiting_on,
            dereth_client_model::inventory::requests::InventoryRequest::Drop,
            ServerTime(1.1),
        );
    let button = shipped(&mut c, INVENTORY_BUTTON);
    let at = carry_over(&mut c, button);
    c.when(LetGo(at));
    clear_requests(c.ui_outbox());

    let sent_nothing = !moves(&c).iter().any(|(item, _, _)| *item == SIDE_PACK);
    let other_request_untouched =
        c.app_mut().objects().world.request_lock.object == Some(already_waiting_on);
    let clean = mark_is_gone(&mut c, SIDE_PACK);

    c.assert_behaviour(
        "inventory.backpack-button.a-refused-drop-sends-nothing-and-takes-the-mark-off",
        move |_| sent_nothing && other_request_untouched && clean,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.refusal.a-full-pack-is-named-the-way-the-player-would-name-it
// ---------------------------------------------------------------------------------------------

/// When nothing the player is carrying has room, the client says so by name -- and the name it
/// uses for the player's **own** pack is the word a player would use for it, while any other
/// container keeps its own name, material and all.
///
/// One row and not two, because it is one decision taken twice with one argument flipped;
/// asserting only the first half would leave a client that called every full container a
/// backpack looking correct.
pub(super) fn a_full_pack_is_named_the_way_the_player_would_name_it() {
    let mut c = a_client_with_two_side_packs();
    {
        let w = &mut c.app_mut().probe_mut().objects_mut().world;
        let player = w.weenie_mut(BUSY_PLAYER).expect("seeded");
        player.pwd.bitfield |= dereth_rules::weenie::bitfield::PLAYER;
        player.pwd.items_capacity = Some(1);
        player.pwd.name = "Larktest".into();
        for pack in [SIDE_PACK, OTHER_PACK] {
            w.weenie_mut(pack).expect("seeded").pwd.items_capacity = Some(0);
        }
        let stack = w.weenie_mut(LOOSE).expect("seeded");
        stack.pwd.stack_size = Some(2);
        stack.pwd.max_stack_size = Some(2);
    }

    // Part of a stack needs a new slot even when the whole stack is already in this pack, which
    // is what makes a full pack refuse it. The split comes through the shipped slider's own seam.
    pick_up_and_see_the_mark(&mut c, LOOSE);
    {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen.splitter.split_size = 1;
        screen.splitter.max_split_size = 2;
    }
    c.ui_outbox()
        .emit(UiRequest::StackSliderChanged { split: 1, max: 2 });
    c.tick(1);

    let before = strip_lines(&mut c).len();
    let button = shipped(&mut c, INVENTORY_BUTTON);
    let at = carry_over(&mut c, button);
    c.when(LetGo(at));
    clear_requests(c.ui_outbox());

    let sent_nothing = moves(&c).is_empty()
        && !c
            .outbound()
            .iter()
            .any(|r| matches!(r, Request::StackableSplitToContainer(_)));
    let named_as_your_own =
        c.app_mut().interaction().last_refusal.as_deref() == Some("Backpack is completely full!");
    let after = strip_lines(&mut c);
    let one_line = after.len() == before + 1
        && after.last().map(String::as_str) == Some("Backpack is completely full!");
    let clean = mark_is_gone(&mut c, LOOSE);

    // The other half of the same decision, on a container that is not the player. It is driven on
    // the client's own world rather than through a second screen, because the client cannot put
    // an arbitrary chest in the player's hands and the claim is about the name, not the gesture.
    const A_CHEST: ObjectId = ObjectId(0x5000_0040);
    const A_PEBBLE: ObjectId = ObjectId(0x5000_0041);
    let other_name = {
        let w = &mut c.app_mut().probe_mut().objects_mut().world;
        w.install_material_names(std::collections::BTreeMap::from([(0x3A, "Bronze".into())]));
        let mut chest = dereth_client_model::Weenie::new(A_CHEST);
        chest.valid = true;
        chest.pwd.name = "Chest".into();
        chest.pwd.material_type = Some(0x3A);
        chest.pwd.items_capacity = Some(0);
        chest.pwd.bitfield |= dereth_rules::weenie::bitfield::OPENABLE;
        w.tables.weenies.insert(A_CHEST, chest);
        w.tables.inventories.insert(
            A_CHEST,
            dereth_client_model::objects::ObjectInventory::new(A_CHEST),
        );
        let mut pebble = dereth_client_model::Weenie::new(A_PEBBLE);
        pebble.valid = true;
        pebble.pwd.name = "Pebble".into();
        w.tables.weenies.insert(A_PEBBLE, pebble);

        let mut asked = dereth_client_model::RecordingRequests::default();
        let mut notices = dereth_client_model::RecordingSink::default();
        let accepted = w.attempt_to_place_in_container(
            &mut asked,
            &mut notices,
            A_PEBBLE,
            A_CHEST,
            A_CHEST,
            false,
            0,
            dereth_client_model::inventory::SplitState::whole_stack(1),
            ServerTime(1.0),
        );
        assert!(
            !accepted,
            "the premise: a container with no room takes nothing"
        );
        assert!(asked.0.is_empty(), "and asks the shard for nothing");
        notices
            .0
            .iter()
            .find_map(|n| match n {
                dereth_client_model::Notice::DisplayString { text, .. } => Some(text.clone()),
                _ => None,
            })
            .expect("the refusal is spoken")
    };
    let named_by_its_own = other_name == "The Bronze Chest is completely full!";

    c.assert_behaviour(
        "inventory.refusal.a-full-pack-is-named-the-way-the-player-would-name-it",
        move |_| sent_nothing && named_as_your_own && one_line && clean && named_by_its_own,
    );
    c.shutdown();
}
