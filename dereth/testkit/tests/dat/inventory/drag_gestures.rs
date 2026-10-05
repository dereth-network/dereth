use super::*;

pub(super) const DRAG_PLAYER: ObjectId = ObjectId(0x5000_0011);
pub(super) const DRAG_ITEM: ObjectId = ObjectId(0x5000_0012);
pub(super) const DRAG_PACK: ObjectId = ObjectId(0x5000_0013);
/// A surface id nothing else in this file is drawing, so a picture that matches it came from
/// this item and from nowhere else.
const DRAG_ICON: DataId = DataId(0x0600_1234);

/// A player carrying one loose item and one side pack, and nothing else.
fn seed_a_pack(w: &mut dereth_client_model::World) {
    use dereth_client_model::weenie::{bitfield, item_type};

    w.set_player(DRAG_PLAYER);
    let mut me = dereth_client_model::Weenie::new(DRAG_PLAYER);
    me.valid = true;
    me.pwd.bitfield |= bitfield::PLAYER | bitfield::OPENABLE;
    me.pwd.items_capacity = Some(24);
    me.pwd.containers_capacity = Some(7);
    w.tables.weenies.insert(DRAG_PLAYER, me);

    let mut item = dereth_client_model::Weenie::new(DRAG_ITEM);
    item.valid = true;
    item.pwd.name = "Black Opal Ring".into();
    item.pwd.icon_id = DRAG_ICON.0;
    item.pwd.obj_type = item_type::JEWELRY;
    item.pwd.container_id = Some(DRAG_PLAYER);
    w.tables.weenies.insert(DRAG_ITEM, item);

    let mut pack = dereth_client_model::Weenie::new(DRAG_PACK);
    pack.valid = true;
    pack.pwd.name = "Backpack".into();
    pack.pwd.icon_id = 0x0600_1235;
    pack.pwd.obj_type = item_type::CONTAINER;
    pack.pwd.bitfield |= bitfield::OPENABLE;
    pack.pwd.items_capacity = Some(24);
    pack.pwd.container_id = Some(DRAG_PLAYER);
    w.tables.weenies.insert(DRAG_PACK, pack);

    w.tables.inventories.insert(
        DRAG_PLAYER,
        dereth_client_model::objects::ObjectInventory {
            container: ObjectId(0),
            items: vec![DRAG_ITEM],
            containers: vec![DRAG_PACK],
            placements: Vec::new(),
        },
    );
    w.tables.inventories.insert(
        DRAG_PACK,
        dereth_client_model::objects::ObjectInventory::new(DRAG_PACK),
    );
}

/// A whole client, with the pack page up, one loose item in the grid and one side pack on the
/// strip.
pub(super) fn a_client_with_a_loose_item_and_a_side_pack() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    seed_a_pack(&mut c.app_mut().probe_mut().objects_mut().world);
    open_pack(&mut c);
    c.tick(2);
    c
}

/// The strip slot a side pack is drawn in.
pub(super) fn strip_slot(c: &mut HeadlessClient, item: ObjectId) -> ElemHandle {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen
        .inventory
        .container_list
        .as_ref()
        .expect("the side-pack strip is bound")
        .slots
        .iter()
        .find(|s| s.item == Some(item))
        .unwrap_or_else(|| panic!("{item:?} is not on the side-pack strip"))
        .handle
}

// ---------------------------------------------------------------------------------------------
// inventory.drag.a-pack-slot-answers-the-pointer-at-its-own-centre
// ---------------------------------------------------------------------------------------------

/// Every slot the grid shows answers a pointer at its own centre.
///
/// The claim is about the slots the list **shows**: the grid is laid out with more cells than
/// its own box holds, and the ones below it are not hit-testable and must not be, because a cell
/// outside the list is outside the panel too. So the walk splits them and asserts over the
/// on-screen half, having first required that half to be at least two rows -- otherwise an empty
/// answer would score as a pass.
pub(super) fn a_pack_slot_answers_the_pointer_at_its_own_centre() {
    let mut c = a_client_with_a_loose_item_and_a_side_pack();

    let (probes, shown, every_slot_can_be_lifted_from) = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let grid = screen
            .inventory
            .item_list
            .as_ref()
            .expect("the pack grid is bound");
        let list_box = ui.node(grid.handle).expect("alive").region.box_;
        let handles: Vec<(ElemHandle, bool, bool)> = grid
            .slots
            .iter()
            .map(|s| {
                let n = ui.node(s.handle).expect("alive");
                (s.handle, n.is_mouse_visible, s.drag_icon.is_some())
            })
            .collect();
        let shown = handles.len();
        let mut liftable = true;
        let mut probes = Vec::new();
        for (h, mouse_visible, has_drag_icon) in handles {
            liftable &= mouse_visible && has_drag_icon;
            let b = ui.node(h).expect("alive").region.box_;
            let (cx, cy) = (b.x0 + b.width() / 2, b.y0 + b.height() / 2);
            if cx >= 0 && cy >= 0 && cx < list_box.width() && cy < list_box.height() {
                probes.push((h, centre(ui, h)));
            }
        }
        (probes, shown, liftable)
    };
    assert!(shown > 1, "the premise: the grid has slots at all");
    assert!(
        probes.len() >= 12,
        "the premise: at least two rows of six are on screen"
    );

    let (ui, _screen) = gameplay_screen(c.app_mut());
    let answered = probes
        .iter()
        .filter(|(h, (x, y))| ui.hit_test_screen(*x, *y) == Some(*h))
        .count();
    let all_of_them = answered == probes.len();

    c.assert_behaviour(
        "inventory.drag.a-pack-slot-answers-the-pointer-at-its-own-centre",
        { move |_| all_of_them && every_slot_can_be_lifted_from },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.drag.a-press-and-a-move-lift-the-items-own-picture-off-its-slot
// ---------------------------------------------------------------------------------------------

/// A press and a move lift a copy of what the slot is drawing, and the slot greys behind it.
///
/// Driven in three steps rather than through `Player::Drag`, because every assertion here is
/// about the state the client is in **while** the icon is in the air: after a release there is
/// no proxy left to read.
pub(super) fn a_press_and_a_move_lift_the_items_own_picture() {
    let mut c = a_client_with_a_loose_item_and_a_side_pack();
    let slot = pack_slot(&mut c, DRAG_ITEM);
    let drawn = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let grid = screen.inventory.item_list.as_ref().expect("the grid");
        let s = grid
            .slots
            .iter()
            .find(|s| s.handle == slot)
            .expect("the slot");
        let icon = s.icon.expect("the slot's own picture element");
        ui.node(icon).expect("alive").region.image.clone()
    };
    assert!(
        drawn.is_some(),
        "the premise: the slot is drawing the item's picture"
    );

    let from = point_of(&mut c, slot);
    c.when(Grab(from));

    let (carried, is_a_plain_move, picture, was) = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        let proxy = ui
            .drag_state()
            .element
            .expect("the press and the move made no drag proxy");
        let info = inq_drop_icon_info(ui, proxy);
        let picture = ui.node(proxy).expect("alive").region.image.clone();
        let box_ = ui.node(proxy).expect("alive").region.box_;
        (info.item, info.is_inventory_move(), picture, box_)
    };
    // **The copy is not byte-identical to the slot, and the row says so.** What the slot draws
    // is the item's picture *inside the slot's own frame*; what is lifted is the picture with
    // the frame left behind, which is what a player sees follow the cursor. So the claim is over
    // the surface each of them names, which is the item's own and nothing else's.
    let surface_of = |g: &Option<dereth_ui::GraphicRef>| {
        g.as_ref().map(|g| {
            let icon = match g.op.and_then(|op| op.icon_recipe()) {
                Some(dereth_ui::region::IconRecipe::Object { icon, .. }) => icon,
                _ => None,
            };
            (g.did, icon)
        })
    };
    let the_items_own_picture = surface_of(&picture) == Some((DRAG_ICON, Some(DRAG_ICON)))
        && surface_of(&drawn) == surface_of(&picture);

    // The copy follows the pointer.
    let elsewhere = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        let (x, y) = ui.mouse_pos();
        Target::Point(ScreenPoint::new(x + 60, y + 40))
    };
    c.when(Over(elsewhere));
    let moved = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        let proxy = ui.drag_state().element.expect("still in the air");
        ui.node(proxy).expect("alive").region.box_ != was
    };

    let (greyed, still_where_it_was) = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        let s = screen
            .inventory
            .item_list
            .as_ref()
            .expect("the grid")
            .slots
            .iter()
            .find(|s| s.handle == slot)
            .expect("still there");
        (s.waiting, s.item == Some(DRAG_ITEM))
    };
    let unmoved = c
        .app_mut()
        .objects()
        .world
        .weenie(DRAG_ITEM)
        .expect("seeded")
        .pwd
        .container_id
        == Some(DRAG_PLAYER);
    c.when(Release);

    c.assert_behaviour(
        "inventory.drag.a-press-and-a-move-lift-the-items-own-picture-off-its-slot",
        move |_| {
            carried == Some(DRAG_ITEM)
                && is_a_plain_move
                && the_items_own_picture
                && moved
                && greyed
                && still_where_it_was
                && unmoved
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.drag.a-drop-on-a-side-pack-becomes-one-move-naming-the-pack-under-the-pointer
// ---------------------------------------------------------------------------------------------

/// **The `Player::Drag` proof.** One whole-gesture step from the grid to the side-pack strip
/// becomes exactly one move, naming the pack and the place in the strip it landed on.
///
/// The second half drives the two links the pointer cannot reach from here -- the request being
/// turned into the shard message, and the shard's own answer moving the item -- on the client's
/// own world, with the ids the drag itself produced.
pub(super) fn a_drop_on_a_side_pack_becomes_one_move() {
    let mut c = a_client_with_a_loose_item_and_a_side_pack();
    let source = pack_slot(&mut c, DRAG_ITEM);
    let target = strip_slot(&mut c, DRAG_PACK);
    let (from, to) = (point_of(&mut c, source), point_of(&mut c, target));
    clear_requests(c.ui_outbox());

    c.when(Player::Drag {
        from,
        to,
        hold_frames: 1,
    });

    // **Nothing is left in the screen's own outbox to read.** The frames `Player::Drag` runs
    // after the release are the frames that dispatch the drop, and `App::frame` drains that
    // outbox into the interaction layer inside them -- so the `DropTarget` shape the drop
    // resolved to is gone by the time a scenario can look, and what is asserted instead is the
    // request the client actually sent and the slot the panel resolves. The receipt ranks that
    // as a harness gap; it is not a weakening of this row, whose claim is the move.
    let nothing_left_over = take_requests(c.ui_outbox()).is_empty();

    // The strip slot the pointer landed on resolves, through the panel's own map, to the pack --
    // dropping into the wrong container is the invisible-wrong case this tier exists for.
    let resolves_to_the_pack = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        let (list, slot, at) = screen
            .inventory
            .locate(target)
            .expect("a slot the panel knows");
        at == Some(DRAG_PACK)
            && screen
                .inventory
                .resolve_drop(DropTarget::ItemList { list, slot })
                == Some(DRAG_PACK)
    };

    // **The drag has already been routed**, which is the point: the frames `Player::Drag` runs
    // after the release hand the drop to the client's own interaction layer, and what that layer
    // sent is what a shard would have received. Re-driving the move by hand here is refused for
    // the right reason -- the one-item-at-a-time lock the drag itself took -- so the request is
    // read off the client rather than made again.
    let sent: Vec<_> = c
        .outbound()
        .iter()
        .filter_map(|r| match r {
            Request::PutItemInContainer(m) => Some((m.item, m.container)),
            _ => None,
        })
        .collect();
    let one_message = sent.len() == 1;
    let names_them = sent.first() == Some(&(DRAG_ITEM, DRAG_PACK));

    // Until the shard answers, the client predicts nothing and holds the slot greyed.
    let (ghosted, not_yet_moved) = {
        let w = &c.app_mut().objects().world;
        let we = w.weenie(DRAG_ITEM).expect("seeded");
        (we.waiting, we.pwd.container_id == Some(DRAG_PLAYER))
    };
    let (moved_by_the_shard, grey_cleared) = {
        let w = &mut c.app_mut().probe_mut().objects_mut().world;
        w.server_says_move_item(
            DRAG_ITEM,
            DRAG_PACK,
            0,
            ObjectId(0),
            0,
            true,
            &mut dereth_client_model::NullSink,
        );
        let we = w.weenie(DRAG_ITEM).expect("still there");
        (we.pwd.container_id == Some(DRAG_PACK), !we.waiting)
    };
    c.assert_behaviour(
        "inventory.drag.a-drop-on-a-side-pack-becomes-one-move-naming-the-pack-under-the-pointer",
        move |_| {
            nothing_left_over
                && resolves_to_the_pack
                && one_message
                && names_them
                && ghosted
                && not_yet_moved
                && moved_by_the_shard
                && grey_cleared
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.drag.an-empty-slot-starts-nothing-and-a-list-that-forbids-it-starts-nothing
// ---------------------------------------------------------------------------------------------

/// The two gates that decide whether a slot can be picked up at all.
pub(super) fn an_empty_slot_and_a_forbidden_list_start_nothing() {
    let mut c = a_client_with_a_loose_item_and_a_side_pack();

    let empty = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        let grid = screen.inventory.item_list.as_ref().expect("the grid");
        assert!(
            grid.allow_dragging,
            "the premise: the pack grid may be dragged from"
        );
        grid.slots
            .iter()
            .find(|s| s.item.is_none())
            .expect("an empty slot")
            .handle
    };
    let at_the_empty_slot = point_of(&mut c, empty);
    c.when(Grab(at_the_empty_slot));
    let nothing_was_lifted = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.drag_state().element.is_none()
    };
    c.when(Release);
    let left_clean = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        !ui.drag_state().started
    };

    // The `AllowDragging` gate itself, over a filled slot, off and on.
    let filled = pack_slot(&mut c, DRAG_ITEM);
    let (refused_without_it, allowed_with_it) = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let (x, y) = centre(ui, filled);
        let grid: &mut ItemListWidget = screen.inventory.item_list.as_mut().expect("the grid");
        grid.allow_dragging = false;
        let refused = grid.begin_drag(ui, x, y).is_none();
        grid.allow_dragging = true;
        let allowed = grid.begin_drag(ui, x, y).is_some();
        (refused, allowed)
    };

    c.assert_behaviour(
        "inventory.drag.an-empty-slot-starts-nothing-and-a-list-that-forbids-it-starts-nothing",
        move |_| nothing_was_lifted && left_clean && refused_without_it && allowed_with_it,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.refusal.dragging-yourself-into-your-own-pack-is-refused-by-the-list-itself
// ---------------------------------------------------------------------------------------------

/// Dragging the player's own row into the player's own pack is refused by the list, in the
/// list's own sentence, before anything is sent or drawn.
///
/// There is a second rule further down that would also refuse it, in a different sentence, and
/// the row is about which of the two the player hears: the list asks first, because the
/// destination it is being dropped into *is* the thing being dropped.
pub(super) fn dragging_yourself_into_your_own_pack_is_refused_by_the_list_itself() {
    let mut c = a_client_at_the_open_chest();
    let (_, from, who) = own_cell_in_the_clear(&mut c, Pick::TopContainer, true);
    let me = recorded_player(&c);
    assert_eq!(
        who,
        Some(me),
        "the premise: the row above the pack is the player"
    );
    // A **filled** cell of the grid: the empty ones are under the chest window at this size, and
    // the rule under test runs before anything about what is already on the tile.
    let (_, onto, _) = own_cell_in_the_clear(&mut c, Pick::ItemList, true);

    // The window is the **release**, not the press: pressing on a thing also selects it, and a
    // selection asks the shard about the thing's own state. What this row is about is what the
    // drop sends, which is nothing.
    c.when(Player::Grab(from));
    let mark = c.outbound().len();
    c.when(Player::Drop(onto));

    let said = c.view().expect_app().interaction().last_refusal.clone();
    let said_the_lists_own_line =
        said.as_deref() == Some("You cannot place yourself in your inventory!");
    let nothing_happened = sent_since(&c, mark).is_empty() && pending_row(&c).is_none();

    c.assert_behaviour(
        "inventory.refusal.dragging-yourself-into-your-own-pack-is-refused-by-the-list-itself",
        move |_| said_the_lists_own_line && nothing_happened,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.refusal.a-plain-thing-on-an-empty-pack-strip-slot-is-refused-for-being-the-wrong-kind
// ---------------------------------------------------------------------------------------------

/// A plain thing let go on an **empty** slot of the side-pack strip is refused for being the
/// wrong kind of thing for that list, and the thing it came from is left unmarked.
///
/// Empty on purpose: the same thing let go on a *pack* in that strip is the ordinary "put it in
/// that pack" drop, which has a row of its own and has to keep working. What this one says is
/// that the strip holds packs, and there is nothing under the pointer to redirect the drop to.
pub(super) fn a_plain_thing_on_an_empty_pack_strip_slot_is_refused_for_its_kind() {
    let (mut c, item) = a_chest_with_something_in_it();
    let (_, from) = chest_tile(&mut c, item);
    let (_, onto, _) = own_cell_in_the_clear(&mut c, Pick::ContainerList, false);

    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Drop(onto));

    let said = c.view().expect_app().interaction().last_refusal.clone();
    let said_so = said.as_deref() == Some("Cannot place item in container list");
    let nothing_happened = sent_since(&c, mark).is_empty() && pending_row(&c).is_none();
    let source_unmarked = !object_waiting(&c, item);

    c.assert_behaviour(
        "inventory.refusal.a-plain-thing-on-an-empty-pack-strip-slot-is-refused-for-being-the-wrong-kind",
        move |_| said_so && nothing_happened && source_unmarked,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.refusal.a-pack-on-the-item-grid-is-refused-for-being-the-wrong-kind
// ---------------------------------------------------------------------------------------------

/// A pack let go on the pack grid is refused the other way round, in the list's own sentence,
/// and the pack is left unmarked.
///
/// The pair with the row above: each of the two lists takes one kind of thing and says so, and
/// the two sentences are different because they are two different mistakes.
pub(super) fn a_pack_on_the_item_grid_is_refused_for_its_kind() {
    let mut c = a_client_at_the_open_chest();
    let (_, from, pack) = own_cell_in_the_clear(&mut c, Pick::ContainerList, true);
    let pack = pack.expect("the recording's character carries a pack");
    let (_, onto, _) = own_cell_in_the_clear(&mut c, Pick::ItemList, true);

    // The release is the window, as above: the press selects.
    c.when(Player::Grab(from));
    let mark = c.outbound().len();
    c.when(Player::Drop(onto));

    let said = c.view().expect_app().interaction().last_refusal.clone();
    let said_so = said.as_deref() == Some("Cannot place container in item list");
    let nothing_happened = sent_since(&c, mark).is_empty() && pending_row(&c).is_none();
    let source_unmarked = !object_waiting(&c, pack);

    c.assert_behaviour(
        "inventory.refusal.a-pack-on-the-item-grid-is-refused-for-being-the-wrong-kind",
        move |_| said_so && nothing_happened && source_unmarked,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.busy-mark.a-refused-drag-takes-the-mark-off-the-tile-as-well-as-off-the-thing
// ---------------------------------------------------------------------------------------------

/// A drag out of the open container that is refused takes the grey mark off the **tile** as well
/// as off the thing, and leaves the mark on a row that really is still waiting.
///
/// The mark is kept twice and the tile follows the thing in both directions. Before this it only
/// followed it up: the thing was clear and the tile the player is looking at stayed grey, for the
/// rest of the session, because nothing else in that window ever writes it. The control in the
/// same pass -- a row whose thing *is* waiting keeps its mark -- is what makes it a mirror rather
/// than a sweep.
pub(super) fn a_refused_drag_takes_the_mark_off_the_tile_as_well() {
    let (mut c, first) = a_chest_with_something_in_it();
    let second = a_second_thing_into_the_chest(&mut c, first);
    let _ = arm_the_grid(&mut c, first);

    let (_, from) = chest_tile(&mut c, second);
    assert!(
        !chest_tile_ghosted(&c, second),
        "the premise: the second row starts unmarked"
    );
    let (_, onto) = pack_cell_in_the_clear(&mut c, first);

    let mark = c.outbound().len();
    c.when(Player::Grab(from));
    let marked_on_the_way_up = chest_tile_ghosted(&c, second) && object_waiting(&c, second);
    c.when(Player::Drop(onto));

    let refused = sent_since(&c, mark).is_empty();
    let came_back_down = !object_waiting(&c, second) && !chest_tile_ghosted(&c, second);
    let the_waiting_one_keeps_it = object_waiting(&c, first) && chest_tile_ghosted(&c, first);

    c.assert_behaviour(
        "inventory.busy-mark.a-refused-drag-takes-the-mark-off-the-tile-as-well-as-off-the-thing",
        move |_| marked_on_the_way_up && refused && came_back_down && the_waiting_one_keeps_it,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.busy-mark.a-row-with-nothing-behind-it-does-not-keep-a-mark-it-was-given
// ---------------------------------------------------------------------------------------------

/// A row the container is showing that the client has no object for can still be picked up, and
/// the grey mark that put on it comes off again on the next redraw.
///
/// It is the same mirror as the row above reaching the case that has no object at all. Nothing
/// in the world ever changes here -- there is nothing to change -- so a client that only ever
/// copied a thing's mark onto its tile would leave this row grey for good -- the stuck grey row
/// this guards against. The control is in the same pass and the same list.
pub(super) fn a_row_with_nothing_behind_it_does_not_keep_a_mark_it_was_given() {
    let (mut c, first) = a_chest_with_something_in_it();
    let ghostless = a_chest_row_with_no_object_behind_it(&c, first);
    let (_, from) = chest_tile(&mut c, ghostless);
    // Deliberately not back onto its own cell: a drop on a tile of this window clears that tile's
    // own mark for a reason that has nothing to do with the mirror under test.
    let (_, onto, _) = own_cell_in_the_clear(&mut c, Pick::ItemList, true);

    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Drop(onto));
    let asked_for_nothing = sent_since(&c, mark).is_empty() && !object_waiting(&c, ghostless);

    c.tick(2);
    let the_tile_came_back = !chest_tile_ghosted(&c, ghostless);

    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .set_waiting_state(first, true);
    c.tick(2);
    let the_backed_one_is_marked = chest_tile_ghosted(&c, first);

    c.assert_behaviour(
        "inventory.busy-mark.a-row-with-nothing-behind-it-does-not-keep-a-mark-it-was-given",
        move |_| asked_for_nothing && the_tile_came_back && the_backed_one_is_marked,
    );
    c.shutdown();
}

/// The book the recording puts into a container on the landscape, and the container.
const BOOK: ObjectId = ObjectId(0x8000_0A22);
const BOOK_CONTAINER: ObjectId = ObjectId(0x8000_0997);
const BOOK_PUT_AT: usize = 2674;
const BOOK_REFUSAL_AT: usize = 2675;

/// The thing the recording drops into the world, and the refusal that comes back.
const FOCI: ObjectId = ObjectId(0x8000_099E);
const FOCI_DROP_AT: usize = 4025;
const FOCI_REFUSAL_AT: usize = 4026;

/// The one recorded appraisal in the whole corpus that carries the attuned property.
const ATTUNED_SUBJECT: ObjectId = ObjectId(0x8000_0A99);
const ATTUNED_APPRAISAL_AT: usize = 6683;

/// A client with the recording replayed up to and including blob `idx`.
fn a_client_played_to(idx: usize) -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    // The identity is chosen before the description arrives, and a corpus replay carries no
    // login step to carry it across.
    c.world_mut().player = Some(RECORDED_PLAYER);
    c.when(Inbound::from_corpus("long-solo-play", 0..idx + 1))
        .tick(4);
    c
}

/// One more recorded blob, by index.
fn one_more_blob(c: &mut HeadlessClient, idx: usize) {
    c.when(Inbound::from_corpus("long-solo-play", idx..idx + 1))
        .tick(3);
}

// ---------------------------------------------------------------------------------------------
// inventory.refusal.a-world-drop-the-shard-refuses-names-the-thing-as-the-player-sees-it
// ---------------------------------------------------------------------------------------------

/// When the shard refuses a thing dropped into the world, the player is told so in a sentence
/// naming the thing the way they see it named -- material and all.
///
/// Both halves are the recording's own: the thing, the request the recorded client made, and the
/// refusal that came back. The second measurement changes only the thing's own description, into
/// one that carries a material, so that what is isolated is the naming and not the refusal.
pub(super) fn a_refused_world_drop_names_the_thing_as_the_player_sees_it() {
    let mut c = a_client_played_to(FOCI_DROP_AT - 1);
    let name = c
        .view()
        .world()
        .weenie(FOCI)
        .expect("the recording creates it")
        .pwd
        .name
        .clone();
    assert_eq!(
        name, "Foci of Enchantment",
        "the premise: the recording's own thing"
    );

    // The request the recorded client made, at the producer that records what kind it is --
    // which is what picks the sentence. Letting go over the world only *arms* a pick, and the
    // hop after it needs a rendered scene and a body on the ground, which is another row's.
    let mut req = dereth_client_model::RecordingRequests::default();
    let mut out = dereth_client_model::RecordingSink::default();
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .attempt_put_in_3d(&mut req, &mut out, FOCI, ServerTime(1.0), false)
        .expect("the premise: nothing else is in flight at this point of the recording");
    assert!(
        req.0
            .iter()
            .any(|r| matches!(r, Request::DropItem(m) if m.item == FOCI)),
        "the premise: this is the drop the recording's next blob answers"
    );

    one_more_blob(&mut c, FOCI_REFUSAL_AT);
    let said_so = feedback(&c)
        .iter()
        .any(|l| l == "The Foci of Enchantment can't be dropped");
    // One client at a time: see the shop scenario above.
    c.shutdown();

    // The same real request and the same recorded refusal, with the thing described with a
    // material in its name.
    let mut c = a_client_played_to(FOCI_DROP_AT - 1);
    let mapper_knows_it = dereth_client_runtime::hud::material_name_of(
        c.view().expect_app().hud().material_names.as_ref(),
        0x3A,
    )
    .as_deref()
        == Some("Bronze");
    assert!(
        mapper_knows_it,
        "the premise: the shipped material table is the prefix's oracle"
    );
    {
        let item = c
            .app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .weenie_mut(FOCI)
            .expect("the recorded thing");
        item.pwd.name = "Salvage (100)".into();
        item.pwd.plural_name = None;
        item.pwd.material_type = Some(0x3A);
        item.pwd.stack_size = Some(1);
    }
    let mut req = dereth_client_model::RecordingRequests::default();
    let mut out = dereth_client_model::RecordingSink::default();
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .attempt_put_in_3d(&mut req, &mut out, FOCI, ServerTime(1.0), false)
        .expect("the premise: the same producer takes the same thing");
    assert!(req
        .0
        .iter()
        .any(|r| matches!(r, Request::DropItem(m) if m.item == FOCI)));
    one_more_blob(&mut c, FOCI_REFUSAL_AT);
    let named_its_material = feedback(&c)
        .iter()
        .any(|l| l == "The Bronze Salvage (100) can't be dropped");

    c.assert_behaviour(
        "inventory.refusal.a-world-drop-the-shard-refuses-names-the-thing-as-the-player-sees-it",
        move |_| said_so && named_its_material,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.refusal.a-container-drop-the-shard-refuses-says-so-in-its-own-words
// ---------------------------------------------------------------------------------------------

/// A thing the shard refuses to let into a container is refused in the container's own sentence,
/// which is not the one a refused world drop gets.
///
/// Two refusals of the same kind from the same answer, and which one the player reads is decided
/// by what the client recorded itself as asking for. The recording is the oracle for both ends:
/// the request its own client sent and the refusal that came back.
pub(super) fn a_refused_container_drop_says_so_in_its_own_words() {
    let mut c = a_client_played_to(BOOK_PUT_AT - 1);
    let name = c
        .view()
        .world()
        .weenie(BOOK)
        .expect("the recording creates it")
        .pwd
        .name
        .clone();
    assert_eq!(
        name, "Restoring the Training Academies",
        "the premise: the recording's own thing"
    );

    let mark = c.outbound().len();
    c.when(Player::Ui(vec![UiRequest::DragDrop {
        item: BOOK,
        target: DropTarget::Container(BOOK_CONTAINER),
    }]))
    .tick(3);
    assert!(
        sent_since(&c, mark)
            .iter()
            .any(|r| matches!(r, Request::PutItemInContainer(m) if m.item == BOOK)),
        "the premise: this is the move the recording's next blob answers"
    );

    one_more_blob(&mut c, BOOK_REFUSAL_AT);
    let said_so = feedback(&c)
        .iter()
        .any(|l| l == "The Restoring the Training Academies can't be put in the container");

    c.assert_behaviour(
        "inventory.refusal.a-container-drop-the-shard-refuses-says-so-in-its-own-words",
        move |_| said_so,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.attunement.an-attuned-thing-says-so-in-its-description
// ---------------------------------------------------------------------------------------------

/// A thing that cannot be dropped says so in its own description, before the player tries.
///
/// That is the other half of a refused drop, and the half that saves the player the attempt. The
/// recording carries exactly one appraisal with the attuned property in it and it carries the
/// bonded one too; both are drawn, in that order, under one heading.
pub(super) fn an_attuned_thing_says_so_in_its_description() {
    let mut c = a_client_played_to(ATTUNED_APPRAISAL_AT - 1);
    // The recorded answer itself, read out of the corpus, so that a failure in the pane cannot
    // be blamed on the decode.
    let profile = {
        let corpus = Corpus::load("long-solo-play")
            .expect("the recording parses")
            .expect("the corpus carries this recording");
        let row = corpus
            .blobs
            .iter()
            .find(|b| b.idx == ATTUNED_APPRAISAL_AT)
            .expect("the recorded index exists");
        let mut rd = dereth_protocol::archive::Reader::new(&row.payload[16..]);
        let m = <dereth_protocol::objects::ItemSetAppraiseInfo as dereth_protocol::Message>::read(
            &mut rd,
        )
        .expect("the recorded appraisal decodes");
        assert_eq!(
            m.object, ATTUNED_SUBJECT,
            "the premise: it is about this thing"
        );
        let ints = m
            .profile
            .tables
            .ints
            .as_ref()
            .expect("the recorded answer has an int table");
        assert!(
            ints.entries.contains(&(114, 1)),
            "the premise: it says the thing is attuned"
        );
        assert!(
            ints.entries.contains(&(33, 1)),
            "the premise: and that it is bonded"
        );
        m.profile
    };

    {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen.examination.examine_object(ATTUNED_SUBJECT);
    }
    {
        let mut sink = dereth_client_model::RecordingSink::default();
        c.app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .set_appraise_info(ATTUNED_SUBJECT, profile, &mut sink);
    }
    c.tick(1);

    let text = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen
            .examination
            .item_text
            .clone()
            .expect("the item pane is written")
    };
    let says_both_in_order = text.contains("Properties: Attuned, Bonded");

    c.assert_behaviour(
        "inventory.attunement.an-attuned-thing-says-so-in-its-description",
        move |_| says_both_in_order,
    );
    c.shutdown();
}

// =============================================================================================
// The place a real drag across the shipped grid asks for.
//
// The placement arithmetic alone is claimed in the cpu tier; this is the one that runs the whole
// chain -- a pointer press on one cell of the live pack grid, a drag to another, and the move that
// leaves with the place the player aimed at in it.
// =============================================================================================

const PLACE_PLAYER: ObjectId = ObjectId(0x5000_0001);
/// Five loose things in the player's own pack, in list order.
const PLACE_ITEMS: [ObjectId; 5] = [
    ObjectId(0x5000_0010),
    ObjectId(0x5000_0011),
    ObjectId(0x5000_0012),
    ObjectId(0x5000_0013),
    ObjectId(0x5000_0014),
];

/// A player carrying five loose things and nothing else.
///
/// Five, because a front-insert and a correct insert agree in a list holding one thing and agree
/// at the head of any list: the cell aimed at below is the fourth, which is neither.
fn seed_five_loose_things(w: &mut dereth_client_model::World) {
    use dereth_client_model::weenie::bitfield;

    w.set_player(PLACE_PLAYER);
    let mut me = dereth_client_model::Weenie::new(PLACE_PLAYER);
    me.valid = true;
    me.pwd.bitfield |= bitfield::PLAYER | bitfield::OPENABLE;
    me.pwd.items_capacity = Some(102);
    me.pwd.containers_capacity = Some(7);
    w.tables.weenies.insert(PLACE_PLAYER, me);

    for id in PLACE_ITEMS {
        let mut it = dereth_client_model::Weenie::new(id);
        it.valid = true;
        it.pwd.name = format!("obj{:X}", id.0 & 0xFF);
        it.pwd.container_id = Some(PLACE_PLAYER);
        w.tables.weenies.insert(id, it);
    }
    w.tables.inventories.insert(
        PLACE_PLAYER,
        dereth_client_model::objects::ObjectInventory {
            container: ObjectId(0),
            items: PLACE_ITEMS.to_vec(),
            containers: Vec::new(),
            placements: Vec::new(),
        },
    );
}

// ---------------------------------------------------------------------------------------------
// inventory.place.a-real-drag-across-the-grid-asks-for-the-place-the-player-aimed-at
// ---------------------------------------------------------------------------------------------

/// A drag from the second cell of the player's own pack to the fourth asks the shard to put that
/// thing in the player's pack at place two.
///
/// Two answers are wrong in ways a player sees, and both are excluded here in one run: asking
/// for the head of the list, which lands every drag there whatever cell was aimed at;
/// and naming the **thing** that happened to be drawn in that cell as the destination, which is
/// a request no shard can answer and which leaves the icon greyed until the player does
/// something else with their pack.
pub(super) fn a_real_drag_across_the_grid_asks_for_the_place_the_player_aimed_at() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    seed_five_loose_things(&mut c.app_mut().probe_mut().objects_mut().world);
    open_pack(&mut c);
    c.tick(2);

    // The premise: the grid really is drawing the five things, in list order. A drag aimed at a
    // cell of a grid that had not filled would be a gesture at nothing.
    let (from, to) = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        let grid = screen
            .inventory
            .item_list
            .as_ref()
            .expect("the pack grid is bound");
        assert_eq!(
            grid.slots
                .iter()
                .filter_map(|s| s.item)
                .take(5)
                .collect::<Vec<_>>(),
            PLACE_ITEMS.to_vec(),
            "the five seeded things fill the first five cells, in list order"
        );
        assert_eq!(
            grid.num_ui_items(),
            5,
            "and the list says it is showing five"
        );
        (grid.slots[1].handle, grid.slots[3].handle)
    };
    let from = point_of(&mut c, from);
    let to = point_of(&mut c, to);

    let mark = c.outbound().len();
    c.when(Player::Drag {
        from,
        to,
        hold_frames: 1,
    });
    clear_requests(c.ui_outbox());

    let moves: Vec<(ObjectId, ObjectId, u32)> = c.outbound()[mark..]
        .iter()
        .filter_map(|r| match r {
            Request::PutItemInContainer(m) => Some((m.item, m.container, m.slot)),
            _ => None,
        })
        .collect();

    c.assert_behaviour(
        "inventory.place.a-real-drag-across-the-grid-asks-for-the-place-the-player-aimed-at",
        move |_| moves == vec![(PLACE_ITEMS[1], PLACE_PLAYER, 2)],
    );
    c.shutdown();
}
