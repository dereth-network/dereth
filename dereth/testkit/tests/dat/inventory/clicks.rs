use dereth_primitives::ObjectId;
use dereth_testkit::HeadlessClient;
use dereth_ui::ElemHandle;

/// A press on `h` with `action`, through the shipped screen's own element-message route.
fn press(c: &mut HeadlessClient, h: ElemHandle, action: u32) {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell")
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::MOUSE_PRESS, action, 0);
    c.tick(2);
}

/// The double click, which is its own action and not two presses.
const DOUBLE_CLICK: u32 = 0x0A;

/// A click on a thing in the pack picks it.
pub fn a_click_on_a_thing_in_the_pack_picks_it() {
    let mut c = super::a_client_with_a_loose_item_and_a_side_pack();
    let before = super::rings::ringed(&mut c, super::DRAG_ITEM);
    let nothing_yet = !before.is_empty() && before.iter().all(|up| !*up);

    let tile = super::pack_slot(&mut c, super::DRAG_ITEM);
    super::clear_requests(c.ui_outbox());
    {
        let (ui, screen) = super::gameplay_screen(c.app_mut());
        screen.on_item_list_press(ui, tile, dereth_ui::focus::action::PRIMARY_CLICK);
    }
    let asked = super::take_requests(c.ui_outbox())
        == vec![dereth_client_contract::UiRequest::Select(super::DRAG_ITEM)];

    super::clear_requests(c.ui_outbox());
    press(&mut c, tile, dereth_ui::focus::action::PRIMARY_CLICK);
    c.tick(2);
    let ringed = super::rings::ringed(&mut c, super::DRAG_ITEM);
    let picked = !ringed.is_empty()
        && ringed.iter().all(|up| *up)
        && c.view().world().selected == Some(super::DRAG_ITEM);

    c.assert_behaviour(
        "inventory.selection.a-click-on-a-thing-in-the-pack-picks-it",
        { move |_| nothing_yet && asked && picked },
    );
    c.shutdown();
}

/// The right button picks and then examines; the double click only uses.
pub fn the_right_button_picks_and_examines_and_a_double_click_only_uses() {
    let mut c = super::a_client_with_a_loose_item_and_a_side_pack();
    let tile = super::pack_slot(&mut c, super::DRAG_ITEM);

    super::clear_requests(c.ui_outbox());
    {
        let (ui, screen) = super::gameplay_screen(c.app_mut());
        screen.on_item_list_press(ui, tile, dereth_ui::focus::action::SECONDARY_CLICK);
    }
    let right_routes = super::take_requests(c.ui_outbox())
        == vec![
            dereth_client_contract::UiRequest::Select(super::DRAG_ITEM),
            dereth_client_contract::UiRequest::Examine(super::DRAG_ITEM),
        ];

    super::clear_requests(c.ui_outbox());
    let mark = c.outbound().len();
    press(&mut c, tile, dereth_ui::focus::action::SECONDARY_CLICK);
    let right: Vec<&dereth_client_model::Request> = c.outbound()[mark..]
        .iter()
        .filter(|r| {
            matches!(
                r,
                dereth_client_model::Request::Appraise(_)
                    | dereth_client_model::Request::UseEvent(_)
            )
        })
        .collect();
    let picks_then_examines = c.view().world().selected == Some(super::DRAG_ITEM)
        && matches!(right.as_slice(), [dereth_client_model::Request::Appraise(m)] if m.target == super::DRAG_ITEM);

    super::clear_requests(c.ui_outbox());
    {
        let (ui, screen) = super::gameplay_screen(c.app_mut());
        screen.on_item_list_press(ui, tile, DOUBLE_CLICK);
    }
    let double_routes = super::take_requests(c.ui_outbox())
        == vec![dereth_client_contract::UiRequest::Use(super::DRAG_ITEM)];

    super::clear_requests(c.ui_outbox());
    let mark = c.outbound().len();
    press(&mut c, tile, DOUBLE_CLICK);
    let twice: Vec<&dereth_client_model::Request> = c.outbound()[mark..]
        .iter()
        .filter(|r| {
            matches!(
                r,
                dereth_client_model::Request::Appraise(_)
                    | dereth_client_model::Request::UseEvent(_)
            )
        })
        .collect();
    let only_uses = matches!(twice.as_slice(), [dereth_client_model::Request::UseEvent(m)] if m.object == super::DRAG_ITEM);

    c.assert_behaviour(
        "inventory.click.the-right-button-picks-and-examines-and-a-double-click-only-uses",
        move |_| right_routes && picks_then_examines && double_routes && only_uses,
    );
    c.shutdown();
}

/// On a list that allows one choice, a thing it shows twice keeps one ring.
///
/// No list the shipped screen owns allows only one choice -- the shop's does, and it is not
/// on this screen -- and a container's own contents list holds each thing once, so both the
/// flag and the second copy are premises here and are said so. Everything below them is the
/// shipped list, its shipped slots and its shipped ring.
pub fn a_single_choice_list_leaves_one_ring_on_the_slot_that_was_clicked() {
    let mut c = super::a_gameplay_client();
    let (ui, screen) = super::parts(c.app_mut());
    let root = screen.root().expect("the gameplay screen has a root");
    screen.shortcuts.init_shortcut_array(ui, root);
    let w = screen
        .shortcuts
        .slots
        .first_mut()
        .expect("the shipped tiles");
    w.single_selection = true;
    w.fixed_list_size = 3;
    w.update_fixed_slots(ui);

    let twice = ObjectId(0x5400_0001);
    let other = ObjectId(0x5400_0002);
    w.set_contents(ui, None, Some(3), &[twice, other, twice], &|_| None);
    // The list's own arm: the second slot holding the same thing arrives unable to be picked.
    let arrives_unpickable =
        w.slots.iter().map(|s| s.selectable).collect::<Vec<_>>() == vec![true, true, false];
    assert!(arrives_unpickable, "the duplicate arrives unselectable");

    for i in 0..3 {
        w.slots[i].set_selectable_state(true);
        w.slots[i].set_selected_state(ui, true);
    }
    let all_ringed = w.slots.iter().filter(|s| s.selected).count() == 3;
    assert!(all_ringed, "all three controls took a ring before the walk");

    let cleared = w.handle_single_selection(ui, 2);
    let one_ring = cleared == 1
        && w.slots.iter().map(|s| s.selected).collect::<Vec<_>>() == vec![false, true, true]
        && w.slots.iter().map(|s| s.selectable).collect::<Vec<_>>() == vec![false, true, true];
    assert!(one_ring, "the duplicate walk left the clicked copy ringed");
    let on_the_element = (0..3).all(|i| {
        let h = w.slots[i].selected_ring.expect("the shipped ring");
        ui.node(h).expect("alive").region.flags.visible == [false, true, true][i]
    });
    assert!(
        on_the_element,
        "the three live ring elements agree with the slots"
    );

    // The walk covers the slots that are really filled and no others: a list with a hole in
    // it has a copy past the count, and the client's own walk never reaches it.
    let counted =
        w.num_ui_items() == 3 && w.is_in_list(twice) && !w.is_in_list(ObjectId(0x5400_0003));
    assert!(counted, "the occupied count and membership controls hold");
    w.fixed_list_size = 4;
    w.update_fixed_slots(ui);
    w.slots[1].item = None;
    w.slots[3].item = Some(twice);
    w.slots[3].set_selectable_state(true);
    w.slots[3].set_selected_state(ui, true);
    w.slots[0].set_selectable_state(true);
    w.slots[0].set_selected_state(ui, true);
    let bounded = w.num_ui_items() == 3
        && w.handle_single_selection(ui, 2) == 1
        && !w.slots[0].selected
        && w.slots[3].selected;
    assert!(bounded, "the positional walk stops at the occupied count");

    c.assert_behaviour(
            "inventory.selection.a-list-that-allows-one-choice-leaves-one-ring-when-a-thing-listed-twice-is-clicked",
            move |_| arrives_unpickable && all_ringed && one_ring && on_the_element && counted && bounded,
        );
    c.shutdown();
}

/// A slot that cannot be picked can still lose its ring, and that is what the walk needs.
pub fn a_slot_that_cannot_be_picked_can_still_lose_its_ring() {
    let mut c = super::a_gameplay_client();
    let (ui, screen) = super::parts(c.app_mut());
    let root = screen.root().expect("the gameplay screen has a root");
    screen.shortcuts.init_shortcut_array(ui, root);
    let w = screen
        .shortcuts
        .slots
        .first_mut()
        .expect("the shipped tiles");
    w.single_selection = true;
    w.fixed_list_size = 2;
    w.update_fixed_slots(ui);

    let twice = ObjectId(0x5500_0001);
    w.set_contents(ui, None, Some(2), &[twice, twice], &|_| None);
    for i in 0..2 {
        w.slots[i].set_selectable_state(true);
        w.slots[i].set_selected_state(ui, true);
    }
    w.handle_single_selection(ui, 1);
    let lost_it = !w.slots[0].selectable && !w.slots[0].selected;
    // …and it cannot be given one back on its own, which is the other direction.
    let refused = !w.slots[0].set_selected_state(ui, true) && !w.slots[0].selected;

    // A click on it makes it pickable again first, which is the order the ring depends on.
    w.handle_single_selection(ui, 0);
    let takes_it_back = w.slots[0].selectable
        && w.slots[0].selected
        && !w.slots[1].selectable
        && !w.slots[1].selected
        && ui
            .node(w.slots[0].selected_ring.expect("the shipped ring"))
            .expect("alive")
            .region
            .flags
            .visible;

    c.assert_behaviour(
        "inventory.selection.a-slot-that-cannot-be-picked-can-still-lose-its-ring",
        move |_| lost_it && refused && takes_it_back,
    );
    c.shutdown();
}

/// Clicking a side pack shows what is inside it and moves the frame onto it.
pub fn clicking_a_side_pack_shows_what_is_inside_it() {
    let mut c = super::a_client_with_a_loose_item_and_a_side_pack();

    let before = grid_contents(&mut c);
    let frame_before = framed(&mut c);
    let at_rest = before == vec![super::DRAG_ITEM] && frame_before == vec![super::DRAG_PLAYER];

    // The pack's own contents, which is the world the shard would have built once it answered
    // about them.
    let inside = ObjectId(0x5600_0001);
    {
        let w = &mut c.app_mut().probe_mut().objects_mut().world;
        let mut thing = dereth_client_model::Weenie::new(inside);
        thing.valid = true;
        thing.pwd.name = "Prismatic Taper".into();
        thing.pwd.icon_id = 0x0600_2371;
        thing.pwd.container_id = Some(super::DRAG_PACK);
        w.tables.weenies.insert(inside, thing);
        w.tables
            .inventories
            .get_mut(super::DRAG_PACK)
            .expect("the side pack has a list of its own")
            .add_content(inside, false, 0);
    }
    c.tick(2);

    let strip = super::strip_slot(&mut c, super::DRAG_PACK);
    press(&mut c, strip, dereth_ui::focus::action::PRIMARY_CLICK);
    c.tick(2);

    let opened = grid_contents(&mut c) == vec![inside] && framed(&mut c) == vec![super::DRAG_PACK];

    c.assert_behaviour(
        "inventory.pack.clicking-a-side-pack-shows-what-is-inside-it-and-moves-the-frame-onto-it",
        move |_| at_rest && opened,
    );
    c.shutdown();
}

/// A pack the player loses while looking into it hands the grid back to his own things.
pub fn a_pack_that_leaves_the_player_hands_the_grid_back() {
    let mut c = super::a_client_with_a_loose_item_and_a_side_pack();
    let strip = super::strip_slot(&mut c, super::DRAG_PACK);
    press(&mut c, strip, dereth_ui::focus::action::PRIMARY_CLICK);
    c.tick(2);
    let looking = framed(&mut c) == vec![super::DRAG_PACK];
    // Opening the side pack took the open item away from the main pack.
    let main_pack_let_go = {
        let (_ui, screen) = super::parts(c.app_mut());
        screen
            .inventory
            .top_container
            .as_ref()
            .expect("the main pack slot")
            .open_item_id
            .is_none()
    };

    // Editing list membership alone is not an ownership notification.
    {
        let w = &mut c.app_mut().probe_mut().objects_mut().world;
        if let Some(inv) = w.tables.inventories.get_mut(super::DRAG_PLAYER) {
            inv.containers.retain(|id| *id != super::DRAG_PACK);
        }
    }
    c.tick(2);
    let silent_edit_keeps_open =
        dereth_client_contract::GameView::open_inventory_container(&c.snapshot())
            == Some(super::DRAG_PACK);
    // The shard then says the open pack left the player.
    c.world_mut().server_says_move_item(
        super::DRAG_PACK,
        ObjectId(0),
        0,
        ObjectId(0),
        0,
        true,
        &mut dereth_client_model::RecordingSink::default(),
    );
    c.tick(2);
    let handed_back = dereth_client_contract::GameView::open_inventory_container(&c.snapshot())
        == Some(super::DRAG_PLAYER)
        && grid_contents(&mut c) == vec![super::DRAG_ITEM]
        && framed(&mut c) == vec![super::DRAG_PLAYER];

    c.assert_behaviour(
            "inventory.pack.a-pack-that-leaves-the-player-hands-the-grid-back-to-the-players-own-things",
            move |_| looking && main_pack_let_go && silent_edit_keeps_open && handed_back,
        );
    c.shutdown();
}

/// A tile naming a thing the client has not seen waits for it before drawing the number.
pub fn a_tile_naming_an_unknown_thing_waits_before_drawing_its_number() {
    let mut c = super::a_gameplay_client();
    let (ui, screen) = super::parts(c.app_mut());

    // The bar as a login restores it: two tiles named, and the first tile's thing is one the
    // client has not heard of yet.
    let unknown = super::Bar {
        known: vec![super::OIL, super::GEM],
        ..super::Bar::default()
    };
    assert!(
        screen.update_shortcuts(ui, &unknown),
        "the first drive is always a redraw"
    );
    let waiting = screen.shortcuts.delayed_numerals == 1;
    let tile = &screen.shortcuts.slots[0].slots[0];
    let nothing_drawn = tile.delayed_shortcut_num == 0
        && tile.shortcut_num == -1
        && !tile.shortcut_num_elem.is_some_and(|h| ui.is_visible(h));
    assert!(waiting, "exactly one unknown shortcut waits");
    assert!(nothing_drawn, "the waiting tile draws no numeral");
    // The control, in the same pass: the tile whose thing the client already has.
    let control = &screen.shortcuts.slots[1].slots[0];
    let at_once = control.shortcut_num == 1
        && control.delayed_shortcut_num == -1
        && control.shortcut_num_elem.is_some_and(|h| ui.is_visible(h));
    assert_eq!(
        control.shortcut_num, 1,
        "the known control records its numeral"
    );
    assert_eq!(
        control.delayed_shortcut_num, -1,
        "the known control parks no numeral"
    );
    assert!(
        control.shortcut_num_elem.is_some_and(|h| ui.is_visible(h)),
        "the known control's numeral element is visible"
    );

    // The thing arrives.
    let arrived = super::Bar::default();
    assert!(
        screen.update_shortcuts(ui, &arrived),
        "a thing arriving is a redraw"
    );
    let tile = &screen.shortcuts.slots[0].slots[0];
    let taken_up = tile.shortcut_num == 0
        && tile.delayed_shortcut_num == -1
        && tile.shortcut_num_elem.is_some_and(|h| ui.is_visible(h));
    assert!(
        taken_up,
        "the delayed numeral appears when its object arrives"
    );

    c.assert_behaviour(
            "shortcut.number.a-tile-naming-a-thing-the-client-has-not-seen-waits-for-it-before-drawing-the-number",
            move |_| waiting && nothing_drawn && at_once && taken_up,
        );
    c.shutdown();
}

/// What the pack grid is showing, in list order.
fn grid_contents(c: &mut HeadlessClient) -> Vec<ObjectId> {
    let (_ui, screen) = super::gameplay_screen(c.app_mut());
    screen
        .inventory
        .item_list
        .as_ref()
        .map(|w| w.slots.iter().filter_map(|s| s.item).collect())
        .unwrap_or_default()
}

/// Which slots of the two strips of packs are wearing the open frame.
fn framed(c: &mut HeadlessClient) -> Vec<ObjectId> {
    let pairs: Vec<(ObjectId, ElemHandle)> = {
        let (_ui, screen) = super::gameplay_screen(c.app_mut());
        let p = &screen.inventory;
        p.top_container
            .iter()
            .chain(p.container_list.iter())
            .flat_map(|w| w.slots.iter())
            .filter_map(|s| s.item.map(|i| (i, s.handle)))
            .collect()
    };
    let (ui, _root) = super::gameplay_root(c.app_mut());
    pairs
        .into_iter()
        .filter(|(_, h)| {
            ui.get_child_recursive(
                *h,
                dereth_ui::ElementId(dereth_ui_screens::items::widget::child::OPEN_CONTAINER),
            )
            .and_then(|k| ui.node(k))
            .is_some_and(|n| n.region.flags.visible)
        })
        .map(|(i, _)| i)
        .collect()
}

dereth_testkit::scenarios! {
    scenario_a_click_on_a_thing_in_the_pack_picks_it => a_click_on_a_thing_in_the_pack_picks_it ["inventory.selection.a-click-on-a-thing-in-the-pack-picks-it"],
    scenario_the_right_button_picks_and_examines_and_a_double_click_only_uses => the_right_button_picks_and_examines_and_a_double_click_only_uses ["inventory.click.the-right-button-picks-and-examines-and-a-double-click-only-uses"],
    scenario_a_single_choice_list_leaves_one_ring_on_the_slot_that_was_clicked => a_single_choice_list_leaves_one_ring_on_the_slot_that_was_clicked ["inventory.selection.a-list-that-allows-one-choice-leaves-one-ring-when-a-thing-listed-twice-is-clicked"],
    scenario_a_slot_that_cannot_be_picked_can_still_lose_its_ring => a_slot_that_cannot_be_picked_can_still_lose_its_ring ["inventory.selection.a-slot-that-cannot-be-picked-can-still-lose-its-ring"],
    scenario_clicking_a_side_pack_shows_what_is_inside_it => clicking_a_side_pack_shows_what_is_inside_it ["inventory.pack.clicking-a-side-pack-shows-what-is-inside-it-and-moves-the-frame-onto-it"],
    scenario_a_pack_that_leaves_the_player_hands_the_grid_back => a_pack_that_leaves_the_player_hands_the_grid_back ["inventory.pack.a-pack-that-leaves-the-player-hands-the-grid-back-to-the-players-own-things"],
    scenario_a_tile_naming_an_unknown_thing_waits_before_drawing_its_number => a_tile_naming_an_unknown_thing_waits_before_drawing_its_number ["shortcut.number.a-tile-naming-a-thing-the-client-has-not-seen-waits-for-it-before-drawing-the-number"],
}
