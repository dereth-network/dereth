use super::*;

/// Integer property `0x26`, lockpick resistance, and `0xAD`, appraisal success percentage.
const RESIST_LOCKPICK: u32 = 0x26;
const LOCKPICK_SUCCESS_PERCENT: u32 = 0xAD;

/// The recorded client's chest, open, with the player's own pack page up behind it.
pub(super) fn a_client_at_the_open_chest() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    given_recorded_chest(&mut c);
    open_pack(&mut c);
    c.tick(2);
    c
}

/// The first loose thing in the player's own pack grid, and where its tile is.
///
/// A pack slot cannot be named by its element id -- the grid builds a hundred of them from one
/// template -- so the target is the tile's own centre. `Target::Element` on the id panics now
/// and says so.
pub(super) fn something_to_drag(c: &mut HeadlessClient) -> (ObjectId, Target) {
    let (item, h) = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        let grid = screen
            .inventory
            .item_list
            .as_ref()
            .expect("the pack grid is bound");
        let slot = grid
            .slots
            .iter()
            .find(|s| s.item.is_some())
            .expect("the recording's character carries a loose thing");
        (slot.item.expect("checked"), slot.handle)
    };
    (item, point_of(c, h))
}

/// The chest's own row -- the tile above its contents, which holds the container itself.
fn chest_own_row(c: &mut HeadlessClient) -> (Target, Option<ObjectId>) {
    let (h, item) = {
        let panel = &c.view().expect_app().hud().panels.external_container;
        let list = panel.top_container.as_ref().expect("the chest's top row");
        let slot = list
            .slots
            .first()
            .expect("the top row holds the ground object itself");
        (slot.handle, slot.item)
    };
    (point_of(c, h), item)
}

/// The first **empty** slot of the chest's own contents list, as a pointer target.
///
/// Empty on purpose: a drop onto an occupied slot is the merge question, which is another row's,
/// and the list shows the chest's whole capacity so there is always one.
fn empty_chest_slot(c: &mut HeadlessClient) -> Target {
    let h = {
        let panel = &c.view().expect_app().hud().panels.external_container;
        let list = panel.item_list.as_ref().expect("the chest's item list");
        list.slots
            .iter()
            .find(|s| s.item.is_none())
            .expect("the list shows the chest's capacity, so one of its slots is empty")
            .handle
    };
    point_of(c, h)
}

/// Every request the client produced after `mark`.
pub(super) fn sent_since(c: &HeadlessClient, mark: usize) -> &[Request] {
    &c.outbound()[mark..]
}

fn puts_in(sent: &[Request], item: ObjectId, container: ObjectId) -> usize {
    sent.iter()
        .filter(|r| {
            matches!(r, Request::PutItemInContainer(m) if m.item == item && m.container == container)
        })
        .count()
}

// ---------------------------------------------------------------------------------------------
// container.ground.a-drop-on-the-open-ones-own-list-puts-the-thing-in-it-exactly-once
// ---------------------------------------------------------------------------------------------

/// Letting something go on an empty slot of the open chest asks the shard to put it in the
/// chest, once.
///
/// **Once** is half the claim and the harder half. Two handlers see the release -- the gameplay
/// screen answers it itself and also queues it for the panels behind it -- so a change that made
/// the screen's own resolver answer for a list it does not own would send the same move twice,
/// and the shard would apply it twice.
pub(super) fn a_drop_on_the_open_chest_puts_the_thing_in_it_once() {
    let mut c = a_client_at_the_open_chest();
    let (item, from) = something_to_drag(&mut c);
    let onto = empty_chest_slot(&mut c);

    let mark = c.outbound().len();
    c.when(Player::Drag {
        from,
        to: onto,
        hold_frames: 1,
    });

    let once = puts_in(sent_since(&c, mark), item, RECORDED_CHEST) == 1;
    c.assert_behaviour(
        "container.ground.a-drop-on-the-open-ones-own-list-puts-the-thing-in-it-exactly-once",
        move |_| once,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// container.ground.a-drop-on-the-open-ones-own-row-goes-into-it-just-the-same
// ---------------------------------------------------------------------------------------------

/// The chest's own tile -- the row above its contents, which holds the container itself -- takes
/// a drop too, and the thing goes into the chest.
///
/// It is a different route through the same step and it is worth a row of its own: that list has
/// **no parent container** at all, so a reading that declined a drop whose list named no
/// container would pass the scenario above and fail this one. What answers is the thing under
/// the pointer, which is the chest.
pub(super) fn a_drop_on_the_chests_own_row_goes_into_it_just_the_same() {
    let mut c = a_client_at_the_open_chest();
    let (item, from) = something_to_drag(&mut c);
    let (onto, sitting_there) = chest_own_row(&mut c);
    assert_eq!(
        sitting_there,
        Some(RECORDED_CHEST),
        "the premise: the top row's tile is the chest"
    );
    let names_no_parent = c
        .view()
        .expect_app()
        .hud()
        .panels
        .external_container
        .top_container
        .as_ref()
        .expect("the chest's top row")
        .parent_container
        .is_none();
    assert!(
        names_no_parent,
        "the premise: and that list names no parent container"
    );

    let mark = c.outbound().len();
    c.when(Player::Drag {
        from,
        to: onto,
        hold_frames: 1,
    });

    let went_in = puts_in(sent_since(&c, mark), item, RECORDED_CHEST) == 1;
    c.assert_behaviour(
        "container.ground.a-drop-on-the-open-ones-own-row-goes-into-it-just-the-same",
        move |_| went_in,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// container.ground.a-hook-in-a-house-the-player-does-not-own-refuses-the-drop-in-words
// ---------------------------------------------------------------------------------------------

/// A container that is a hook in a house the player does not own refuses the drop where the
/// player is standing, in the client's own words, and asks the shard for nothing.
///
/// The refusal is local, and that is the point of the row: a client that sent the move and let
/// the shard say no would leave the player watching an icon sit in a slot it is about to be
/// thrown out of. The sentence is asserted verbatim, both where the panel keeps it and on the
/// strip the player actually reads.
pub(super) fn a_hook_the_player_does_not_own_refuses_the_drop_in_words() {
    const REFUSAL: &str =
        "That item cannot be placed on the hook. You must own the house to manipulate the hook.";

    let mut c = a_client_at_the_open_chest();
    {
        let w = c
            .app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .weenie_mut(RECORDED_CHEST)
            .expect("the recorded chest is live");
        // Only the hook inputs change. The recording's own openable bit and item capacity stay,
        // so this is still the same physical external container and the same drop route.
        w.pwd.hook_type = Some(1);
        w.pwd.hook_item_types = Some(u32::MAX);
        w.pwd.house_owner_iid = Some(ObjectId(0));
    }
    let (item, from) = something_to_drag(&mut c);
    let onto = empty_chest_slot(&mut c);

    let mark = c.outbound().len();
    c.when(Player::Drag {
        from,
        to: onto,
        hold_frames: 1,
    });

    let asked_for_nothing = puts_in(sent_since(&c, mark), item, RECORDED_CHEST) == 0;
    let kept = c.view().expect_app().interaction().last_refusal.clone();
    let on_the_strip = bubbles(&mut c).last().cloned();

    c.assert_behaviour(
        "container.ground.a-hook-in-a-house-the-player-does-not-own-refuses-the-drop-in-words",
        move |_| {
            asked_for_nothing
                && kept.as_deref() == Some(REFUSAL)
                && on_the_strip.as_deref() == Some(REFUSAL)
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// container.lock.identifying-a-locked-one-says-so-and-how-hard-the-lock-is
// ---------------------------------------------------------------------------------------------

/// Identifying a locked container says it is locked and says how hard the lock is to pick.
///
/// The recorded chest's own answer is the oracle for both halves: the scenario replays the
/// recording as far as the **first** of the two appraisals it carries for that chest -- the
/// second arrives after the key and drops the lock block entirely -- asserts that the recorded
/// answer really does say locked, and then reads the pane.
///
/// The percentage the shard sends is zero, which is the client's own word for *impossible*, and
/// a negative one draws no line at all. `Unlocked` is the other arm of the same question and
/// must not be drawn for this one; a pane that wrote both would read as a client that had not
/// decided.
pub(super) fn identifying_a_locked_container_says_so_and_how_hard_the_lock_is() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let at = given_recorded_chest(&mut c);
    let lock_at = first_blob_where(CHEST_SESSION, |e| {
        is_locked_appraisal_of(e, RECORDED_LOCKED_CHEST)
    })
    .expect("the recording answers about the locked chest");
    if lock_at >= at {
        c.when(Inbound::from_corpus(CHEST_SESSION, at..lock_at + 1))
            .tick(2);
    }

    // The data half, so that a failure in the presentation half cannot be blamed on the decode
    // or on the cache.
    let (says_locked, resist, percent) = {
        let p = c
            .view()
            .world()
            .appraisal
            .get(RECORDED_LOCKED_CHEST)
            .expect("the recorded answer is cached")
            .clone();
        let b = |k: u32| {
            p.tables
                .bools
                .as_ref()
                .and_then(|t| t.entries.iter().find(|(e, _)| *e == k).map(|(_, v)| *v))
        };
        let i = |k: u32| {
            p.tables
                .ints
                .as_ref()
                .and_then(|t| t.entries.iter().find(|(e, _)| *e == k).map(|(_, v)| *v))
        };
        (
            b(LOCKED_BOOL),
            i(RESIST_LOCKPICK),
            i(LOCKPICK_SUCCESS_PERCENT),
        )
    };
    assert_eq!(
        says_locked,
        Some(1),
        "the premise: the recorded answer says the chest is locked"
    );
    assert_eq!(
        percent,
        Some(0),
        "the premise: and that the lock cannot be picked at all"
    );
    let resist = resist.expect("the premise: and how much the lock resists");

    {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen.examination.examine_object(RECORDED_LOCKED_CHEST);
    }
    c.tick(2);

    let text = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen
            .examination
            .item_text
            .clone()
            .expect("the item pane is written")
    };
    let says_it_is_locked = text.contains("Locked") && !text.contains("Unlocked");
    let says_how_hard = text.contains(&format!(
        "The lock looks impossible to pick (Resistance {resist})."
    ));

    c.assert_behaviour(
        "container.lock.identifying-a-locked-one-says-so-and-how-hard-the-lock-is",
        move |_| says_it_is_locked && says_how_hard,
    );
    c.shutdown();
}

// =============================================================================================
// What is in the chest, and the optimistic row a drop draws before the shard has answered.
//
// One mechanism seen from four sides, on `given_recorded_chest`. A drop asks the shard to move
// something and the client draws the answer it expects **before** the answer arrives: the source
// tile greys, an optimistic row appears in the destination, and both come off when the shard
// speaks. What the scenarios pin is when the row is *not* drawn, what a second drop meets while it
// is up, and which of the two halves of the grey mark -- the object's own flag and the tile's -- is
// the one that can be cleared again.
// =============================================================================================

/// The recorded chest with the player's own loose thing moved into it, the pack page up, and
/// nothing ghosted yet.
pub(super) fn a_chest_with_something_in_it() -> (HeadlessClient, ObjectId) {
    let mut c = a_client_at_the_open_chest();
    let item = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen
            .inventory
            .item_list
            .as_ref()
            .expect("the pack grid is bound")
            .slots
            .iter()
            .find(|s| s.item.is_some())
            .expect("the recording's character carries a loose thing")
            .item
            .expect("checked")
    };
    // The shard's own authoritative move, which is the only way a ground container gets contents.
    // This is the model seam; every gesture under test is real pointer input.
    {
        let mut out = dereth_client_model::RecordingSink::default();
        c.app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .server_says_move_item(item, RECORDED_CHEST, 0, ObjectId(0), 0, true, &mut out);
    }
    c.tick(2);
    assert!(
        !chest_tile_ghosted(&c, item),
        "the premise: a freshly filled chest row is not grey"
    );
    (c, item)
}

/// The chest's own tile holding `item`, as a pointer target, and its place in the list.
pub(super) fn chest_tile(c: &mut HeadlessClient, item: ObjectId) -> (usize, Target) {
    let (i, h) = {
        let list = c
            .view()
            .expect_app()
            .hud()
            .panels
            .external_container
            .item_list
            .as_ref()
            .expect("the chest's item list");
        let i = list
            .slots
            .iter()
            .position(|s| s.item == Some(item))
            .unwrap_or_else(|| panic!("{item:?} is shown in the chest's own list"));
        (i, list.slots[i].handle)
    };
    (i, point_of(c, h))
}

/// Whether the chest row holding `item` is drawn grey -- the tile's half of the mark.
pub(super) fn chest_tile_ghosted(c: &HeadlessClient, item: ObjectId) -> bool {
    c.view()
        .expect_app()
        .hud()
        .panels
        .external_container
        .item_list
        .as_ref()
        .and_then(|l| l.slots.iter().find(|s| s.item == Some(item)))
        .is_some_and(|s| s.waiting)
}

/// The object's own half of the mark.
pub(super) fn object_waiting(c: &HeadlessClient, item: ObjectId) -> bool {
    c.view().world().weenie(item).is_some_and(|w| w.waiting)
}

/// The first empty cell of the player's pack grid, and its index.
fn empty_pack_cell(c: &mut HeadlessClient) -> (usize, Target) {
    let (i, h) = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        let grid = screen
            .inventory
            .item_list
            .as_ref()
            .expect("the pack grid is bound");
        let (i, slot) = grid
            .slots
            .iter()
            .enumerate()
            .find(|(_, s)| s.item.is_none())
            .expect("an empty pack cell");
        (i, slot.handle)
    };
    (i, point_of(c, h))
}

/// The first empty cell of the chest's own grid.
fn empty_chest_cell(c: &mut HeadlessClient) -> Target {
    let h = {
        let list = c
            .view()
            .expect_app()
            .hud()
            .panels
            .external_container
            .item_list
            .as_ref()
            .expect("the chest's item list");
        list.slots
            .iter()
            .find(|s| s.item.is_none())
            .expect("an empty chest cell")
            .handle
    };
    point_of(c, h)
}

/// The optimistic row the client drew, if it drew one.
pub(super) fn pending_row(
    c: &HeadlessClient,
) -> Option<dereth_client_model::inventory::PendingRow> {
    c.view().world().pending_row
}

/// Every cell of the pack grid showing `item`, with whether it is drawn grey.
fn pack_rows(c: &HeadlessClient, item: ObjectId) -> Vec<(usize, bool)> {
    let app = c.view().expect_app();
    let shell = app.ui().expect("the UI shell");
    let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
    any.downcast_ref::<GamePlayScreen>()
        .expect("the gameplay screen")
        .inventory
        .item_list
        .as_ref()
        .map(|l| {
            l.slots
                .iter()
                .enumerate()
                .filter(|(_, s)| s.item == Some(item))
                .map(|(i, s)| (i, s.waiting))
                .collect()
        })
        .unwrap_or_default()
}

/// Set a container's capacity to exactly what it already holds. Nothing else about the recorded
/// object is touched, so what changes is the one number the fit test reads.
fn fill_to_capacity(c: &mut HeadlessClient, container: ObjectId) -> usize {
    let held = c
        .view()
        .world()
        .inventory(container)
        .map_or(0, |i| i.items.len());
    let cap = u8::try_from(held).expect("the recorded container holds fewer than 256 things");
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .weenie_mut(container)
        .expect("a live container")
        .pwd
        .items_capacity = Some(cap);
    c.tick(1);
    held
}

/// The recording's own character.
pub(super) fn recorded_player(c: &HeadlessClient) -> ObjectId {
    c.view()
        .world()
        .player
        .expect("the recording's own character")
}

/// The first side pack the character carries -- where a move spills when the main pack is full.
fn a_side_pack(c: &HeadlessClient) -> ObjectId {
    let p = recorded_player(c);
    *c.view()
        .world()
        .inventory(p)
        .expect("the player's inventory")
        .containers
        .first()
        .expect("the recording's character carries at least one pack")
}

/// Whether an icon is in the air.
fn carrying(c: &mut HeadlessClient) -> bool {
    let (ui, _screen) = gameplay_screen(c.app_mut());
    ui.drag_state().element.is_some()
}

// ---------------------------------------------------------------------------------------------
// inventory.busy-mark.a-drag-out-of-the-open-one-that-ends-over-nothing-leaves-no-grey-row
// ---------------------------------------------------------------------------------------------

/// Picking a row out of the open container greys it while it is held, and letting go over
/// nothing takes the grey off again.
///
/// **Both halves of the mark are read, and that is the whole row.** The grey is kept twice --
/// once on the thing itself and once on the tile drawing it -- and only the thing's own flag has
/// anything that writes it back. A client that greyed the tile alone would leave that row grey
/// for the rest of the session, because nothing else in this window ever writes it -- the
/// stuck grey row this guards against.
pub(super) fn a_cancelled_drag_out_of_the_chest_leaves_no_grey_row() {
    let (mut c, item) = a_chest_with_something_in_it();
    let (_, from) = chest_tile(&mut c, item);

    let mark = c.outbound().len();
    c.when(Player::Grab(from));
    assert!(
        carrying(&mut c),
        "the premise: the press and the move lifted the row"
    );
    let held = object_waiting(&c, item) && chest_tile_ghosted(&c, item);

    c.when(Player::Drop(Target::Point(ScreenPoint::new(2, 2))));
    let asked_for_nothing = sent_since(&c, mark).is_empty();
    let let_go = !object_waiting(&c, item) && !chest_tile_ghosted(&c, item);

    c.assert_behaviour(
        "inventory.busy-mark.a-drag-out-of-the-open-one-that-ends-over-nothing-leaves-no-grey-row",
        move |_| held && asked_for_nothing && let_go,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.busy-mark.a-refused-move-out-of-the-open-one-clears-it-and-lets-the-next-one-go
// ---------------------------------------------------------------------------------------------

/// A move out of the open container keeps the grey until the shard answers; a refusal takes it
/// off and lets the very same gesture be made again.
///
/// The second half is the one worth the row. The client holds one inventory request at a time
/// and there is no timeout on it, so a refusal that cleared the mark without releasing the hold
/// would leave the player looking at an ungreyed row that refuses to move for the rest of the
/// session. The scenario makes the identical gesture twice on the same object, so nothing about
/// the item can explain the difference.
pub(super) fn a_refused_move_out_of_the_chest_clears_it_and_lets_the_next_one_go() {
    let (mut c, item) = a_chest_with_something_in_it();
    let (_, from) = chest_tile(&mut c, item);
    let (_, onto) = empty_pack_cell(&mut c);

    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Drop(onto));
    let asked = sent_since(&c, mark)
        .iter()
        .any(|r| matches!(r, Request::PutItemInContainer(m) if m.item == item));
    let still_grey = object_waiting(&c, item) && chest_tile_ghosted(&c, item);

    {
        let mut out = dereth_client_model::RecordingSink::default();
        c.app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .server_says_attempt_failed(item, 0, &mut out);
    }
    c.tick(2);
    let cleared = !object_waiting(&c, item) && !chest_tile_ghosted(&c, item);

    let (_, again_from) = chest_tile(&mut c, item);
    let mark2 = c.outbound().len();
    c.when(Player::Grab(again_from)).when(Player::Drop(onto));
    let asked_again = sent_since(&c, mark2)
        .iter()
        .any(|r| matches!(r, Request::PutItemInContainer(m) if m.item == item));

    c.assert_behaviour(
        "inventory.busy-mark.a-refused-move-out-of-the-open-one-clears-it-and-lets-the-next-one-go",
        move |_| asked && still_grey && cleared && asked_again,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.busy-mark.the-tile-a-drop-lands-on-clears-its-own-and-not-the-source-s
// ---------------------------------------------------------------------------------------------

/// The tile a drop lands **on** clears its own grey mark, whatever the drop then does -- and the
/// tile the drop came *from* keeps its own.
///
/// Two things are being marked at once here and they are not the same thing: the row that was
/// already waiting on an older request, which the drop lands on, and the thing that was just
/// asked about, which the drop carried. A client that cleared "the mark" on a drop would clear
/// both and un-grey a thing the shard has not answered about.
pub(super) fn the_tile_a_drop_lands_on_clears_its_own_grey_mark() {
    let (mut c, resident) = a_chest_with_something_in_it();
    let dragged = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen
            .inventory
            .item_list
            .as_ref()
            .expect("the pack grid")
            .slots
            .iter()
            .find(|s| s.item.is_some_and(|i| i != resident))
            .expect("the recording's character carries another loose thing")
            .item
            .expect("checked")
    };
    let from = {
        let h = pack_slot(&mut c, dragged);
        point_of(&mut c, h)
    };

    // The chest row is waiting on an older request: the thing's own flag and nothing else.
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .set_waiting_state(resident, true);
    c.tick(2);
    assert!(
        chest_tile_ghosted(&c, resident),
        "the premise: the row is grey before the drop"
    );

    let (_, onto) = chest_tile(&mut c, resident);
    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Drop(onto));

    let reached_the_shard = !sent_since(&c, mark).is_empty();
    let caught_tile_cleared = !object_waiting(&c, resident) && !chest_tile_ghosted(&c, resident);
    let the_carried_one_still_waits = object_waiting(&c, dragged);

    c.assert_behaviour(
        "inventory.busy-mark.the-tile-a-drop-lands-on-clears-its-own-and-not-the-source-s",
        move |_| reached_the_shard && caught_tile_cleared && the_carried_one_still_waits,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.pending-row.a-destination-that-is-full-draws-none-and-the-move-spills-to-a-side-pack
// ---------------------------------------------------------------------------------------------

/// Dropping something into a pack that is already full draws no optimistic row at the cell
/// aimed at, and the move still goes out -- naming the side pack it spilled into.
///
/// The two are separate gates and this is the row that says so. One decides whether to draw the
/// row, the other decides where the thing goes, and only the first of them looks at whether the
/// cell aimed at could hold it. A client that ran them as one would either draw a row in a pack
/// the thing never reaches, or refuse a move retail sends.
pub(super) fn a_full_destination_draws_no_row_and_the_move_spills_to_a_side_pack() {
    let (mut c, item) = a_chest_with_something_in_it();
    let (_, from) = chest_tile(&mut c, item);
    let (_, onto) = empty_pack_cell(&mut c);

    // The side pack has to be able to take it, or the spill would refuse locally and the
    // scenario would prove nothing about the gate. These are the two fields the fit test reads.
    let pack = a_side_pack(&c);
    {
        let w = c
            .app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .weenie_mut(pack)
            .expect("the side pack");
        w.pwd.bitfield |= dereth_client_model::weenie::bitfield::OPENABLE;
        w.pwd.items_capacity = Some(24);
    }
    let me = recorded_player(&c);
    let held = fill_to_capacity(&mut c, me);
    assert!(
        !c.view().world().will_item_fit_in_container(
            item,
            me,
            dereth_client_model::inventory::SplitState::whole_stack(1)
        ),
        "the premise: the pack holds {held} and its capacity now says {held}"
    );

    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Drop(onto));

    let sent = sent_since(&c, mark);
    let went_to_the_side_pack = sent.iter().any(
        |r| matches!(r, Request::PutItemInContainer(m) if m.item == item && m.container == pack),
    );
    let no_row = pending_row(&c).is_none() && pack_rows(&c, item).is_empty();
    let still_waiting = chest_tile_ghosted(&c, item) && object_waiting(&c, item);

    c.assert_behaviour(
        "inventory.pending-row.a-destination-that-is-full-draws-none-and-the-move-spills-to-a-side-pack",
        move |_| went_to_the_side_pack && no_row && still_waiting,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.pending-row.a-destination-with-room-draws-one-grey-in-the-very-cell-aimed-at
// ---------------------------------------------------------------------------------------------

/// The same gesture onto a pack with room draws exactly one optimistic row, grey, in the very
/// cell the pointer was over.
///
/// The control for the row above, and a claim of its own: *which* cell is the part a player
/// notices, because an item that appears somewhere else looks like the drop missed.
pub(super) fn a_destination_with_room_draws_one_grey_row_in_the_cell_aimed_at() {
    let (mut c, item) = a_chest_with_something_in_it();
    let (_, from) = chest_tile(&mut c, item);
    let (cell, onto) = empty_pack_cell(&mut c);

    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Drop(onto));

    let asked = sent_since(&c, mark)
        .iter()
        .any(|r| matches!(r, Request::PutItemInContainer(m) if m.item == item));
    let me = recorded_player(&c);
    let row = pending_row(&c);
    let drew_it_there = row.is_some_and(|r| {
        r.item == item
            && r.container == me
            && usize::try_from(r.index).unwrap_or(usize::MAX) == cell
    });
    let rows = pack_rows(&c, item);
    let one_grey_row = rows.len() == 1 && rows[0].1;

    c.assert_behaviour(
        "inventory.pending-row.a-destination-with-room-draws-one-grey-in-the-very-cell-aimed-at",
        move |_| asked && drew_it_there && one_grey_row,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.refusal.a-full-destination-with-nowhere-to-spill-says-so-and-sends-nothing
// ---------------------------------------------------------------------------------------------

/// A drop into a container that is full and has nothing to spill into is refused where the
/// player is standing, in the client's own words, and nothing is asked of the shard.
///
/// It is the same gate as the spilling row above, reaching its other end: the row is still not
/// drawn, and this time there is nowhere for the thing to go either, so the refusal the player
/// hears is the one about the container being full and not the one about being busy.
pub(super) fn a_full_destination_with_nowhere_to_spill_says_so_and_sends_nothing() {
    let mut c = a_client_at_the_open_chest();
    let (item, from) = something_to_drag(&mut c);
    let onto = empty_chest_cell(&mut c);
    let held = fill_to_capacity(&mut c, RECORDED_CHEST);
    assert!(
        held > 0,
        "the premise: the recorded chest holds something, and now says it is full"
    );

    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Drop(onto));

    let sent_nothing = !sent_since(&c, mark)
        .iter()
        .any(|r| matches!(r, Request::PutItemInContainer(_)));
    let no_row = pending_row(&c).is_none();
    let chest_shows_nothing_new = c
        .view()
        .expect_app()
        .hud()
        .panels
        .external_container
        .item_list
        .as_ref()
        .map_or(0, |l| {
            l.slots.iter().filter(|s| s.item == Some(item)).count()
        })
        == 0;
    let refusal = c.view().expect_app().interaction().last_refusal.clone();
    let said_it_is_full = refusal
        .as_deref()
        .is_some_and(|r| r.starts_with("The ") && r.ends_with("is completely full!"));

    c.assert_behaviour(
        "inventory.refusal.a-full-destination-with-nowhere-to-spill-says-so-and-sends-nothing",
        move |_| sent_nothing && no_row && chest_shows_nothing_new && said_it_is_full,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// The rest of the waiting row: the pending row itself, and a second drop while it is up.
// ---------------------------------------------------------------------------------------------

/// The shard's own move answer.
fn shard_moves(c: &mut HeadlessClient, item: ObjectId, container: ObjectId, place: u32) {
    let mut out = dereth_client_model::RecordingSink::default();
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .server_says_move_item(item, container, place, ObjectId(0), 0, true, &mut out);
    c.tick(2);
}

/// The shard's refusal.
fn shard_refuses(c: &mut HeadlessClient, item: ObjectId) {
    let mut out = dereth_client_model::RecordingSink::default();
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .server_says_attempt_failed(item, 0, &mut out);
    c.tick(2);
}

/// A second thing inside the chest. The recorded chest holds contents of its own, so the thing
/// put into it by [`a_chest_with_something_in_it`] is never the only row.
fn another_chest_item(c: &HeadlessClient, not: ObjectId) -> ObjectId {
    c.view()
        .expect_app()
        .hud()
        .panels
        .external_container
        .item_list
        .as_ref()
        .expect("the chest's item list")
        .slots
        .iter()
        .filter_map(|s| s.item)
        .find(|&i| i != not)
        .expect("the recorded chest holds contents of its own besides the one put in")
}

/// The name the client would put in a sentence about `item`.
fn spoken_name(c: &HeadlessClient, item: ObjectId) -> String {
    let world = c.view().world();
    let w = world.weenie(item).expect("a live object");
    let material = world.material_name(w.pwd.material_type.unwrap_or(0));
    w.display_name(dereth_client_model::weenie::NameType::Appropriate, material)
}

/// A cell of the pack grid the chest window is not sitting on top of.
///
/// At this fixture's screen size the chest window covers part of the pack grid, so a cell picked
/// by index alone can hit-test to the chest's own list instead. The walk asks the shell which
/// element really answers each cell's centre and takes the first that answers the grid.
pub(super) fn pack_cell_in_the_clear(c: &mut HeadlessClient, avoid: ObjectId) -> (usize, Target) {
    let (i, h) = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let grid = screen.inventory.item_list.as_ref().expect("the pack grid");
        let list = grid.handle;
        let mut found = None;
        for (i, slot) in grid.slots.iter().enumerate() {
            if slot.item == Some(avoid) {
                continue;
            }
            let (x, y) = centre(ui, slot.handle);
            let mut hit = ui.hit_test_screen(x, y);
            while let Some(k) = hit {
                if k == list {
                    found = Some((i, slot.handle));
                    break;
                }
                hit = ui.parent(k);
            }
            if found.is_some() {
                break;
            }
        }
        found.expect("some cell of the pack grid is uncovered")
    };
    (i, point_of(c, h))
}

/// Drop one: an ordinary chest-to-pack move, which draws the waiting row and takes the client's
/// one inventory hold. Answers the row and the point it was made at.
pub(super) fn arm_the_grid(
    c: &mut HeadlessClient,
    item: ObjectId,
) -> (dereth_client_model::inventory::PendingRow, Target) {
    let (_, from) = chest_tile(c, item);
    let (cell, onto) = empty_pack_cell(c);
    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Drop(onto));
    assert!(
        sent_since(c, mark)
            .iter()
            .any(|r| matches!(r, Request::PutItemInContainer(m) if m.item == item)),
        "the premise: the first drop is an ordinary move and it takes the hold"
    );
    let row = pending_row(c).expect("the premise: the first drop drew the waiting row");
    assert_eq!(row.item, item);
    assert_eq!(usize::try_from(row.index).unwrap_or(usize::MAX), cell);
    let said = c.view().expect_app().interaction().last_refusal.clone();
    assert!(
        !said
            .as_deref()
            .is_some_and(|s| s.starts_with("Already attempting to place")),
        "the premise: neither sentence under test is already standing; got {said:?}"
    );
    assert_ne!(
        said.as_deref(),
        Some(BUSY),
        "the premise: and the first drop did not meet the hold"
    );
    (row, onto)
}

// ---------------------------------------------------------------------------------------------
// inventory.pending-row.the-row-is-drawn-before-the-shard-answers-and-the-answer-replaces-it
// ---------------------------------------------------------------------------------------------

/// On the frame the player lets go, the destination already shows the thing, greyed, at the cell
/// aimed at -- and the shard's answer turns that row into the real one rather than adding a
/// second.
///
/// The source keeps its own grey while the destination shows the waiting copy, which is what
/// tells the player the move is in flight rather than done. *One* row after the answer and not
/// two is the half a plainer client gets wrong: it draws the optimistic row and then lets the
/// answer insert another beside it.
pub(super) fn the_waiting_row_is_drawn_at_once_and_the_answer_replaces_it() {
    let (mut c, item) = a_chest_with_something_in_it();
    let (_, from) = chest_tile(&mut c, item);
    let (cell, onto) = empty_pack_cell(&mut c);

    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Drop(onto));
    let asked = sent_since(&c, mark)
        .iter()
        .any(|r| matches!(r, Request::PutItemInContainer(m) if m.item == item));

    let me = recorded_player(&c);
    let row = pending_row(&c);
    let drawn_before_the_answer = row.is_some_and(|r| {
        r.item == item
            && r.container == me
            && !r.containers_list
            && usize::try_from(r.index).unwrap_or(usize::MAX) == cell
    });
    let rows = pack_rows(&c, item);
    let one_grey_row_at_that_cell = rows.len() == 1 && rows[0].0 == cell && rows[0].1;
    let source_still_grey = chest_tile_ghosted(&c, item);

    shard_moves(&mut c, item, me, u32::try_from(cell).expect("a cell index"));
    let replaced = pending_row(&c).is_none();
    let after = pack_rows(&c, item);
    let one_real_row = after.len() == 1 && !after[0].1;

    c.assert_behaviour(
        "inventory.pending-row.the-row-is-drawn-before-the-shard-answers-and-the-answer-replaces-it",
        move |_| {
            asked
                && drawn_before_the_answer
                && one_grey_row_at_that_cell
                && source_still_grey
                && replaced
                && one_real_row
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.pending-row.a-refusal-takes-the-row-away-and-un-greys-what-never-moved
// ---------------------------------------------------------------------------------------------

/// The shard refusing the move takes the waiting row away again and un-greys the thing, which is
/// still where it always was.
pub(super) fn a_refusal_takes_the_waiting_row_away_and_un_greys_the_source() {
    let (mut c, item) = a_chest_with_something_in_it();
    let (_, from) = chest_tile(&mut c, item);
    let (_, onto) = empty_pack_cell(&mut c);

    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Drop(onto));
    let armed = !sent_since(&c, mark).is_empty()
        && pending_row(&c).is_some()
        && pack_rows(&c, item).len() == 1;

    shard_refuses(&mut c, item);
    let gone = pending_row(&c).is_none() && pack_rows(&c, item).is_empty();
    let still_in_the_chest = !chest_tile_ghosted(&c, item)
        && c.view()
            .world()
            .weenie(item)
            .is_some_and(|w| w.pwd.container_id == Some(RECORDED_CHEST));

    c.assert_behaviour(
        "inventory.pending-row.a-refusal-takes-the-row-away-and-un-greys-what-never-moved",
        move |_| armed && gone && still_in_the_chest,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.pending-row.a-drop-aimed-at-a-side-pack-draws-none-and-still-sends-the-move
// ---------------------------------------------------------------------------------------------

/// A drop onto a side pack on the strip sends the move and draws no waiting row anywhere.
///
/// The row belongs to the list that caught the drop and is only drawn when the thing is going
/// into that list's own container. A side pack's tile is on the strip and the thing is going
/// into the pack, so there is no list to draw it in -- and a client that drew one in the strip
/// would show a pack where a pack's contents are not.
pub(super) fn a_drop_aimed_at_a_side_pack_draws_no_waiting_row() {
    let (mut c, item) = a_chest_with_something_in_it();
    let (_, from) = chest_tile(&mut c, item);
    let onto = {
        let h = {
            let (_ui, screen) = gameplay_screen(c.app_mut());
            screen
                .inventory
                .container_list
                .as_ref()
                .expect("the side-pack strip is bound")
                .slots
                .iter()
                .find(|s| s.item.is_some())
                .expect("the recording's character carries a pack")
                .handle
        };
        point_of(&mut c, h)
    };

    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Drop(onto));

    let sent_the_move = sent_since(&c, mark)
        .iter()
        .any(|r| matches!(r, Request::PutItemInContainer(_)));
    let no_row = pending_row(&c).is_none();

    c.assert_behaviour(
        "inventory.pending-row.a-drop-aimed-at-a-side-pack-draws-none-and-still-sends-the-move",
        move |_| sent_the_move && no_row,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.pending-row.a-drop-the-client-refuses-for-itself-draws-none-either
// ---------------------------------------------------------------------------------------------

/// A drop the client refuses for itself -- a row let go on the very cell it already occupies --
/// draws no waiting row and sends nothing.
///
/// Every refusal the client makes for itself happens before the row is drawn, so the row is
/// never a thing that has to be taken back. A client that drew it first would leave a grey copy
/// of the thing sitting in a list the shard is never going to be asked about.
pub(super) fn a_drop_the_client_refuses_for_itself_draws_no_waiting_row() {
    let (mut c, item) = a_chest_with_something_in_it();
    let (_, from) = chest_tile(&mut c, item);

    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Drop(from));

    let sent_nothing = sent_since(&c, mark).is_empty();
    let no_row = pending_row(&c).is_none() && pack_rows(&c, item).is_empty();

    c.assert_behaviour(
        "inventory.pending-row.a-drop-the-client-refuses-for-itself-draws-none-either",
        move |_| sent_nothing && no_row,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.backpack-button.a-drop-on-it-draws-the-waiting-row-at-the-head-of-the-pack
// ---------------------------------------------------------------------------------------------

/// Dropping something on the toolbar's backpack button draws the waiting row at the **head** of
/// the pack rather than at any cell, because the player did not aim at a cell.
///
/// Both rows are drawn grey while the shard has not answered: the thing's real row, which is
/// still where it was, and the waiting copy above it. The answer leaves one.
pub(super) fn a_drop_on_the_backpack_button_draws_the_waiting_row_at_the_head() {
    let mut c = a_client_at_the_open_chest();
    let (item, from) = something_to_drag(&mut c);
    let before = pack_rows(&c, item);
    assert_eq!(
        before.len(),
        1,
        "the premise: the carried thing has exactly one real row"
    );

    let onto = Target::Element(dereth_ui_screens::toolbar::INVENTORY_BUTTON);
    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Drop(onto));

    let asked = !sent_since(&c, mark).is_empty();
    let row = pending_row(&c);
    let at_the_head = row.is_some_and(|r| r.item == item && r.index == 0 && !r.containers_list);
    let rows = pack_rows(&c, item);
    let two_grey_rows = rows.len() == 2 && rows.iter().all(|(_, grey)| *grey);

    let me = recorded_player(&c);
    shard_moves(&mut c, item, me, 0);
    let one_left = pending_row(&c).is_none() && pack_rows(&c, item).len() == 1;

    c.assert_behaviour(
        "inventory.backpack-button.a-drop-on-it-draws-the-waiting-row-at-the-head-of-the-pack",
        move |_| asked && at_the_head && two_grey_rows && one_left,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.pending-row.a-second-drop-on-the-same-list-names-what-is-already-being-placed
// ---------------------------------------------------------------------------------------------

/// While a waiting row is up, a second drop on that same list is refused in a sentence naming
/// the thing **already** being placed, and nothing at all reaches the shard.
///
/// The sentence is the whole row. The client also holds one inventory request at a time, and a
/// client that let the second drop run down to that hold would refuse it with a true sentence
/// about a different rule -- *you can only move or use one item at a time* -- which tells the
/// player nothing about the row they are looking at. The first row is left exactly as it was,
/// and the thing that was refused is not left marked.
pub(super) fn a_second_drop_on_the_same_list_names_what_is_already_being_placed() {
    let (mut c, first) = a_chest_with_something_in_it();
    let second = another_chest_item(&c, first);
    let (row, _) = arm_the_grid(&mut c, first);

    let (_, from) = chest_tile(&mut c, second);
    let (_, onto) = pack_cell_in_the_clear(&mut c, first);
    let mark = c.outbound().len();
    c.when(Player::Grab(from));
    let row_survived_the_pick_up = pending_row(&c) == Some(row);
    c.when(Player::Drop(onto));

    let want = format!(
        "Already attempting to place {} here",
        spoken_name(&c, first)
    );
    let said_so =
        c.view().expect_app().interaction().last_refusal.as_deref() == Some(want.as_str());
    let sent_nothing = sent_since(&c, mark).is_empty();
    let row_untouched = pending_row(&c) == Some(row)
        && pack_rows(&c, second).is_empty()
        && pack_rows(&c, first).len() == 1
        && pack_rows(&c, first)[0].1
        && object_waiting(&c, first);
    let the_refused_one_is_not_marked = !object_waiting(&c, second);

    c.assert_behaviour(
        "inventory.pending-row.a-second-drop-on-the-same-list-names-what-is-already-being-placed",
        move |_| {
            row_survived_the_pick_up
                && said_so
                && sent_nothing
                && row_untouched
                && the_refused_one_is_not_marked
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.pending-row.a-second-drop-on-a-different-list-meets-the-one-request-hold-instead
// ---------------------------------------------------------------------------------------------

/// A second drop on a **different** list is not refused by the row at all -- it meets the
/// client's one-request-at-a-time hold, and the player hears that sentence instead.
///
/// The row belongs to one list, and a client that keyed the refusal on "a row exists anywhere"
/// would collapse the two rules into one and say the wrong sentence here.
pub(super) fn a_second_drop_on_a_different_list_meets_the_one_request_hold() {
    let (mut c, first) = a_chest_with_something_in_it();
    let second = another_chest_item(&c, first);
    let (row, _) = arm_the_grid(&mut c, first);

    let (_, from) = chest_tile(&mut c, second);
    let onto = {
        let h = {
            let (_ui, screen) = gameplay_screen(c.app_mut());
            screen
                .inventory
                .container_list
                .as_ref()
                .expect("the side-pack strip is bound")
                .slots
                .iter()
                .find(|s| s.item.is_some())
                .expect("the recording's character carries a pack")
                .handle
        };
        point_of(&mut c, h)
    };

    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Drop(onto));

    let said_the_hold = c.view().expect_app().interaction().last_refusal.as_deref() == Some(BUSY);
    let sent_nothing = sent_since(&c, mark).is_empty();
    let row_undisturbed = pending_row(&c) == Some(row);

    c.assert_behaviour(
        "inventory.pending-row.a-second-drop-on-a-different-list-meets-the-one-request-hold-instead",
        move |_| said_the_hold && sent_nothing && row_undisturbed,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.pending-row.once-the-row-is-gone-the-very-same-drop-is-taken-again
// ---------------------------------------------------------------------------------------------

/// The row is a gate and not a latch: the moment the shard's refusal clears it, the very gesture
/// that was refused is accepted, draws its row again, and reaches the shard.
///
/// It is literally the same drag on the same object, so nothing about the thing can explain the
/// difference.
pub(super) fn once_the_waiting_row_is_gone_the_same_drop_is_taken_again() {
    let (mut c, first) = a_chest_with_something_in_it();
    let (row, _) = arm_the_grid(&mut c, first);
    assert_eq!(
        pack_rows(&c, first).len(),
        1,
        "the premise: armed and drawn"
    );

    shard_refuses(&mut c, first);
    let cleared = pending_row(&c).is_none() && pack_rows(&c, first).is_empty();

    let (_, from) = chest_tile(&mut c, first);
    let (cell, onto) = empty_pack_cell(&mut c);
    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Drop(onto));

    let asked_again = sent_since(&c, mark)
        .iter()
        .any(|r| matches!(r, Request::PutItemInContainer(m) if m.item == first));
    let again = pending_row(&c);
    let armed_again = again.is_some_and(|r| {
        r.item == first
            && usize::try_from(r.index).unwrap_or(usize::MAX) == cell
            && r.container == row.container
    });
    let rows = pack_rows(&c, first);
    let drawn_once_and_grey = rows.len() == 1 && rows[0].1;

    c.assert_behaviour(
        "inventory.pending-row.once-the-row-is-gone-the-very-same-drop-is-taken-again",
        move |_| cleared && asked_again && armed_again && drawn_once_and_grey,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// What a list refuses outright, and the grey mark following the thing back down.
//
// Three of these are the list's own refusals -- the ones it makes before it has asked anything
// about the thing being carried -- and two are the mirror between a tile and the thing it draws,
// which the drop scenarios above only exercise in one direction.
// ---------------------------------------------------------------------------------------------

/// Which of the three shipped lists of the player's own pack window a cell is taken from.
#[derive(Debug, Clone, Copy)]
pub(super) enum Pick {
    /// The one row above the pack: the player.
    TopContainer,
    /// The side-pack strip.
    ContainerList,
    /// The pack grid.
    ItemList,
}

/// A cell of one of the player's own lists that no other window is sitting on top of, with what
/// is on it.
///
/// The walk asks the shell which element would **catch** a release at each cell's centre, which
/// is the question the release itself asks, and takes the first cell whose catcher lives in the
/// list wanted. A cell the chest window covers fails it.
pub(super) fn own_cell_in_the_clear(
    c: &mut HeadlessClient,
    which: Pick,
    filled: bool,
) -> (usize, Target, Option<ObjectId>) {
    let (i, h, item) = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let w = match which {
            Pick::TopContainer => screen.inventory.top_container.as_ref(),
            Pick::ContainerList => screen.inventory.container_list.as_ref(),
            Pick::ItemList => screen.inventory.item_list.as_ref(),
        }
        .expect("the shipped list is in the tree");
        let list = w.handle;
        let mut found = None;
        for (i, slot) in w.slots.iter().enumerate() {
            if slot.item.is_some() != filled {
                continue;
            }
            let (x, y) = centre(ui, slot.handle);
            let mut caught = ui
                .hit_test_screen(x, y)
                .and_then(|h| ui.drag_and_drop_catcher(h));
            while let Some(k) = caught {
                if k == list {
                    found = Some((i, slot.handle, slot.item));
                    break;
                }
                caught = ui.parent(k);
            }
            if found.is_some() {
                break;
            }
        }
        found.unwrap_or_else(|| panic!("no {which:?} cell of the wanted kind is uncovered"))
    };
    (i, point_of(c, h), item)
}

/// A row the chest is showing that the recording never sent an object for.
///
/// The contents list names ids the recording listed and never created, so they draw and they
/// drag -- both halves of that belong to the panel -- while the world knows nothing about them.
pub(super) fn a_chest_row_with_no_object_behind_it(c: &HeadlessClient, not: ObjectId) -> ObjectId {
    let app = c.view().expect_app();
    app.hud()
        .panels
        .external_container
        .item_list
        .as_ref()
        .expect("the chest's item list")
        .slots
        .iter()
        .filter_map(|s| s.item)
        .find(|&i| i != not && app.objects().world.weenie(i).is_none())
        .expect("the recorded chest lists contents the recording never created")
}

/// A second carried thing, moved into the chest at the model seam. Unlike the chest's recorded
/// contents this one is a real object the world knows, which a claim about the thing's own grey
/// mark needs.
pub(super) fn a_second_thing_into_the_chest(c: &mut HeadlessClient, not: ObjectId) -> ObjectId {
    let item = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen
            .inventory
            .item_list
            .as_ref()
            .expect("the pack grid")
            .slots
            .iter()
            .filter_map(|s| s.item)
            .find(|i| *i != not)
            .expect("the recording's character carries more than one loose thing")
    };
    assert!(
        c.view().world().weenie(item).is_some(),
        "the premise: a carried thing is a real object, which is the point of this fixture"
    );
    let mut out = dereth_client_model::RecordingSink::default();
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .server_says_move_item(item, RECORDED_CHEST, 0, ObjectId(0), 0, true, &mut out);
    c.tick(2);
    item
}

// ---------------------------------------------------------------------------------------------
// container.ground.a-drag-out-of-the-open-one-onto-the-pack-moves-it-and-keeps-the-mark
// ---------------------------------------------------------------------------------------------

/// A drag out of the open container onto the player's own pack asks the shard to put the thing
/// in the pack, once -- and the thing stays in the container, greyed, until the shard answers.
///
/// The second half is the one the player sees. A client that let the row leave on the release
/// would show the thing in two places if the shard then refused; one that never greyed it would
/// give no sign the move was asked for at all. What is carried is read off the icon in the air
/// and not off the list it came from, which is why a container the panel keeps in no registry
/// can still be dragged out of.
pub(super) fn a_drag_out_of_the_open_chest_moves_it_to_the_pack_and_keeps_the_mark() {
    let (mut c, item) = a_chest_with_something_in_it();
    let pack = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen
            .inventory
            .item_list
            .as_ref()
            .and_then(|w| w.parent_container)
            .expect("the pack grid knows its own container")
    };
    let (i, from) = chest_tile(&mut c, item);
    let (_, onto) = empty_pack_cell(&mut c);

    c.when(Player::Grab(from));
    let mark = c.outbound().len();
    c.when(Player::Drop(onto));

    let sent = sent_since(&c, mark);
    let moved_into_the_pack = sent
        .iter()
        .filter(|r| matches!(r, Request::PutItemInContainer(m) if m.item == item && m.container == pack))
        .count()
        == 1;

    let (still_there, grey) = {
        let h = {
            let list = c
                .view()
                .expect_app()
                .hud()
                .panels
                .external_container
                .item_list
                .as_ref()
                .expect("the chest's item list");
            (list.slots[i].item, list.slots[i].ghosted)
        };
        let (ui, _root) = gameplay_root(c.app_mut());
        (
            h.0,
            h.1.is_some_and(|k| ui.node(k).is_some_and(|n| n.region.flags.visible)),
        )
    };
    let waits_where_it_is = still_there == Some(item) && grey;

    c.assert_behaviour(
        "container.ground.a-drag-out-of-the-open-one-onto-the-pack-moves-it-and-keeps-the-mark",
        move |_| moved_into_the_pack && waits_where_it_is,
    );
    c.shutdown();
}

// =============================================================================================
// Three things that reached their handler and were then not shown, not accepted, not consumed.
//
// Each is a whole chain and each is measured at both ends, because each half can work while the
// join fails: the shop that opened into a window nobody had raised, the chest that took the key and
// then refused the use, and the shipped pick-up key that reached dispatch and matched nothing.
//
// Every message is the recording's own, and where a claim is about what goes out, the recording
// carries that message too.
// =============================================================================================

/// The vendor panel's own page, and the floating host that has to be raised over it.
const SHOP_PAGE: ElementId = ElementId(0x1000_0062);
const FLOATING_HOST: ElementId = ElementId(0x1000_05FD);
/// The grocer the recording opens, and the index of her first shop message.
const GROCER: ObjectId = ObjectId(0x77F0_3059);
const GROCER_OPENS_AT: usize = 4152;

/// The locked chest the recording unlocks and locks again, and the two moments.
const LOCKED_CHEST: ObjectId = ObjectId(0x77F0_3053);
const UNLOCKED_AT: usize = 4114;
const RELOCKED_AT: usize = 4274;

/// Whether an element is on screen: itself visible, and every ancestor visible with it. That is
/// the only question a player asks of it.
fn drawn_on_screen(c: &mut HeadlessClient, id: ElementId) -> bool {
    let mut h = element_of(c, id);
    let (ui, _root) = gameplay_root(c.app_mut());
    loop {
        if !ui.node(h).expect("a live element").region.flags.visible {
            return false;
        }
        let Some(parent) = ui.parent(h) else {
            return true;
        };
        h = parent;
    }
}

/// Whether the client believes `id` can be opened.
fn believes_it_opens(c: &HeadlessClient, id: ObjectId) -> bool {
    c.view()
        .world()
        .weenie(id)
        .unwrap_or_else(|| panic!("{id:?} is created by the recording"))
        .pwd
        .bitfield
        & dereth_client_model::weenie::bitfield::OPENABLE
        != 0
}

// ---------------------------------------------------------------------------------------------
// vendor.window.the-shards-own-message-puts-the-shop-on-screen-and-not-only-its-tabs
// ---------------------------------------------------------------------------------------------

/// The recording's own shop message puts the shop **on screen**, and not merely its tab strip.
///
/// Both ends are asserted, and the left one first, so that a failure on the right cannot be
/// blamed on decoding, dispatch or the model: the message names the shopkeeper the recording
/// named, exactly one shop was opened, the model says a shop is open and the window found its
/// own lists. Then the right end, which nothing measured: the shop's own page is visible and the
/// floating window that carries it has been raised.
pub(super) fn the_shards_own_message_puts_the_shop_on_screen_and_not_only_its_tabs() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.when(Inbound::from_corpus(
        "long-solo-play",
        0..GROCER_OPENS_AT + 1,
    ))
    .tick(4);

    let the_model_agrees = c.view().world().shop.vendor_id == Some(GROCER)
        && c.view().interaction().stats.vendor_opens == 1
        && dereth_client::vendor_view::shop(c.view().world()).open
        && c.view().expect_app().hud().panels.vendor.bound();

    let the_page_is_visible = {
        let h = element_of(&c, SHOP_PAGE);
        let (ui, _root) = gameplay_root(c.app_mut());
        ui.node(h).expect("a live element").region.flags.visible
    };
    let the_host_is_raised = drawn_on_screen(&mut c, FLOATING_HOST);
    let the_shop_is_really_on_screen = drawn_on_screen(&mut c, SHOP_PAGE);

    c.assert_behaviour(
        "vendor.window.the-shards-own-message-puts-the-shop-on-screen-and-not-only-its-tabs",
        move |_| {
            the_model_agrees
                && the_page_is_visible
                && the_host_is_raised
                && the_shop_is_really_on_screen
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// container.ground.the-shards-unlock-reaches-the-chest-and-the-relock-follows-it
// ---------------------------------------------------------------------------------------------

/// The shard saying a chest is unlocked reaches the chest, and saying it is locked again reaches
/// it too.
///
/// The control is the same run: before the unlock arrives the client believes the chest is
/// locked, so "it opens" afterwards cannot be the state the chest was created with. The relock
/// is the other half and it is what makes this a live receiver rather than a one-way switch --
/// a client that dropped these leaves one stale flag, and the chest is dead for the rest of the
/// session however many times the player uses the right key on it.
pub(super) fn the_shards_unlock_reaches_the_chest_and_the_relock_follows_it() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.when(Inbound::from_corpus("long-solo-play", 0..UNLOCKED_AT))
        .tick(1);
    let locked_to_begin_with = !believes_it_opens(&c, LOCKED_CHEST);

    c.when(Inbound::from_corpus(
        "long-solo-play",
        UNLOCKED_AT..UNLOCKED_AT + 1,
    ))
    .tick(1);
    let the_unlock_landed = believes_it_opens(&c, LOCKED_CHEST);

    c.when(Inbound::from_corpus(
        "long-solo-play",
        UNLOCKED_AT + 1..RELOCKED_AT + 1,
    ))
    .tick(1);
    let the_relock_landed = !believes_it_opens(&c, LOCKED_CHEST);

    c.assert_behaviour(
        "container.ground.the-shards-unlock-reaches-the-chest-and-the-relock-follows-it",
        move |_| locked_to_begin_with && the_unlock_landed && the_relock_landed,
    );
    c.shutdown();
}
