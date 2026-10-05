use super::*;

// =============================================================================================
// The hint under a carried icon, on the live element tree.
//
// The widget-level hint claims are in the cpu tier; these six need the shipped tree -- the overlay
// every slot is built with, the hint going up and coming down across a real drag, the two edges the
// element manager raises as the icon crosses from one catcher to the next, and what the drop is
// finally aimed at. The first of them is a refutation: **no cursor the client ships is chosen by a
// drag at all**, so the hint is the only thing that answers a hover.
// =============================================================================================

pub(super) const HINT_DRAG_PLAYER: ObjectId = ObjectId(0x5000_0001);
pub(super) const HINT_DRAG_PACK: ObjectId = ObjectId(0x5000_00FF);
pub(super) const HINT_DRAG_ITEMS: [ObjectId; 3] = [
    ObjectId(0x5000_0010),
    ObjectId(0x5000_0011),
    ObjectId(0x5000_0012),
];

/// A player carrying three loose things and one empty side pack.
///
/// The pack is what makes the strip's three answers three: a cell holding it, an empty cell
/// beside it, and the pack itself once it is known to have room.
pub(super) fn seed_three_things_and_a_pack(w: &mut dereth_client_model::World) {
    use dereth_rules::weenie::bitfield;

    w.set_player(HINT_DRAG_PLAYER);
    let mut me = dereth_client_model::Weenie::new(HINT_DRAG_PLAYER);
    me.valid = true;
    me.pwd.bitfield |= bitfield::PLAYER | bitfield::OPENABLE;
    me.pwd.items_capacity = Some(102);
    me.pwd.containers_capacity = Some(7);
    w.tables.weenies.insert(HINT_DRAG_PLAYER, me);

    for id in HINT_DRAG_ITEMS {
        let mut it = dereth_client_model::Weenie::new(id);
        it.valid = true;
        it.pwd.name = format!("obj{:X}", id.0 & 0xFF);
        it.pwd.container_id = Some(HINT_DRAG_PLAYER);
        w.tables.weenies.insert(id, it);
    }
    let mut pack = dereth_client_model::Weenie::new(HINT_DRAG_PACK);
    pack.valid = true;
    pack.pwd.name = "Sack".into();
    pack.pwd.bitfield |= bitfield::OPENABLE;
    pack.pwd.items_capacity = Some(24);
    pack.pwd.container_id = Some(HINT_DRAG_PLAYER);
    w.tables.weenies.insert(HINT_DRAG_PACK, pack);

    w.tables.inventories.insert(
        HINT_DRAG_PLAYER,
        dereth_client_model::objects::ObjectInventory {
            container: ObjectId(0),
            items: HINT_DRAG_ITEMS.to_vec(),
            containers: vec![HINT_DRAG_PACK],
            placements: Vec::new(),
        },
    );
    w.tables.inventories.insert(
        HINT_DRAG_PACK,
        dereth_client_model::objects::ObjectInventory::new(HINT_DRAG_PACK),
    );
}

pub(super) fn a_client_with_three_things_and_a_pack() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    seed_three_things_and_a_pack(&mut c.app_mut().probe_mut().objects_mut().world);
    open_pack(&mut c);
    c.tick(2);
    c
}

/// The handle of one cell of the pack grid.
pub(super) fn grid_cell(c: &mut HeadlessClient, n: usize) -> ElemHandle {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen
        .inventory
        .item_list
        .as_ref()
        .expect("the pack grid is bound")
        .slots[n]
        .handle
}

/// The handle of one cell of the side-pack strip.
pub(super) fn strip_cell(c: &mut HeadlessClient, n: usize) -> ElemHandle {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen
        .inventory
        .container_list
        .as_ref()
        .expect("the side-pack strip is bound")
        .slots[n]
        .handle
}

/// The hint one cell of the pack grid is showing.
fn grid_hint(c: &mut HeadlessClient, n: usize) -> Option<StateId> {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen
        .inventory
        .item_list
        .as_ref()
        .expect("the pack grid is bound")
        .slots[n]
        .drag_accept_state
}

/// The hint one cell of the side-pack strip is showing.
fn strip_hint(c: &mut HeadlessClient, n: usize) -> Option<StateId> {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen
        .inventory
        .container_list
        .as_ref()
        .expect("the side-pack strip is bound")
        .slots[n]
        .drag_accept_state
}

/// The element the manager has recorded the drag cursor as being over.
fn cursor_is_over(c: &mut HeadlessClient) -> Option<ElemHandle> {
    let (ui, _screen) = gameplay_screen(c.app_mut());
    ui.drag_state().last_drag_cursor_over
}

// ---------------------------------------------------------------------------------------------
// inventory.drag-hint.no-cursor-the-client-ships-is-chosen-by-a-drag
// ---------------------------------------------------------------------------------------------

/// The mouse pointer does not change while an icon is being carried, and it **cannot**: the
/// thing that picks a pointer picture is not given the drag at all.
///
/// Every combination of that chooser's five inputs is driven -- busy and not, all four aiming
/// modes, all five fighting stances, hovering and not, over something usable and not -- and the
/// set of pictures that come back is the denominator this claim needs. None of the shipped
/// pictures is named for a drag, a drop or a refusal either, so there is no picture the client
/// could have picked even if something asked for one.
pub(super) fn no_cursor_the_client_ships_is_chosen_by_a_drag() {
    use dereth_assets::Decode;
    use dereth_client_model::combat::CombatMode;
    use dereth_primitives::AssetSource;
    use {
        dereth_client_shell::cursor::cursor_enum, dereth_client_shell::cursor::CursorInputs,
        dereth_client_shell::cursor::TargetMode, dereth_client_shell::cursor::UICURSOR_GROUP,
    };

    let c = HeadlessClient::new(ClientSpec::retail());

    // The shipped table of pointer pictures, read out of the retail data rather than written
    // here: a list written here would agree with itself whatever the client ships.
    let names: Vec<(u32, String)> = {
        let store = c.view().dat_store().clone();
        let source: &dyn AssetSource = store.as_ref();
        let master = dereth_assets::DidMapper::decode_payload(
            dereth_client_runtime::assets::MASTER_DID_MAPPER,
            &source
                .read(dereth_client_runtime::assets::MASTER_DID_MAPPER)
                .expect("the master table reads"),
        )
        .expect("the master table decodes");
        let which = master
            .enum_to_id
            .iter()
            .find(|(k, _)| *k == UICURSOR_GROUP)
            .map(|(_, v)| dereth_primitives::DataId(*v))
            .expect("the master table names the pointer group");
        let m = dereth_assets::DidMapper::decode_payload(
            which,
            &source.read(which).expect("the pointer table reads"),
        )
        .expect("the pointer table decodes");
        let mut v = m.enum_to_name.clone();
        v.sort_by_key(|(k, _)| *k);
        v
    };
    assert_eq!(
        names.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        (1..=cursor_enum::LAST).collect::<Vec<_>>(),
        "the premise: the shipped table is numbered straight through with no gaps"
    );

    let mut reachable: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let mut runs = 0;
    for busy in [0_u32, 1] {
        for aiming in [
            TargetMode::None,
            TargetMode::Use,
            TargetMode::Examine,
            TargetMode::UseTarget,
        ] {
            for stance in [
                CombatMode::Undef,
                CombatMode::NonCombat,
                CombatMode::Melee,
                CombatMode::Missile,
                CombatMode::Magic,
            ] {
                for hovering in [false, true] {
                    for usable in [false, true] {
                        let picked =
                            dereth_client_shell::cursor::update_cursor_state(CursorInputs {
                                busy,
                                target_mode: aiming,
                                combat_mode: stance,
                                hovering,
                                target_compatible: usable,
                            });
                        reachable.insert(picked.enum_value);
                        runs += 1;
                    }
                }
            }
        }
    }
    assert_eq!(
        runs, 160,
        "the denominator: every combination of the chooser's five inputs"
    );

    let never_chosen: Vec<&str> = names
        .iter()
        .filter(|(k, _)| !reachable.contains(k))
        .map(|(_, n)| n.as_str())
        .collect();
    // The table's counts, kept as premises so that this scenario cannot quietly become an assertion
    // about a table that has changed under it.
    assert_eq!(
        names.len(),
        41,
        "the premise: the shipped table holds forty-one pictures"
    );
    assert_eq!(reachable.len(), 15, "fifteen of them are selectable at all");
    assert_eq!(
        never_chosen.len(),
        26,
        "and twenty-six are selected by nothing"
    );
    assert_eq!(
        never_chosen
            .iter()
            .filter(|n| n.starts_with("Move_"))
            .count(),
        23,
        "twenty-three of those twenty-six are the movement family -- a different feature"
    );
    assert_eq!(
        never_chosen
            .iter()
            .filter(|n| n.starts_with("Combat_Advanced_"))
            .count(),
        3,
        "and the other three are the advanced-fighting pictures"
    );
    let some_are_never_chosen = !never_chosen.is_empty();
    let none_of_them_is_for_a_drag = !never_chosen.iter().any(|n| {
        let n = n.to_ascii_lowercase();
        n.contains("drag") || n.contains("drop")
    });
    let nothing_shipped_is_named_for_a_drag = !names.iter().any(|(_, n)| {
        let n = n.to_ascii_lowercase();
        n.contains("drag") || n.contains("drop")
    });
    let fewer_are_reachable_than_are_shipped = reachable.len() < names.len();

    let mut c = c;
    c.assert_behaviour(
        "inventory.drag-hint.no-cursor-the-client-ships-is-chosen-by-a-drag",
        move |_| {
            some_are_never_chosen
                && none_of_them_is_for_a_drag
                && nothing_shipped_is_named_for_a_drag
                && fewer_are_reachable_than_are_shipped
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.drag-hint.every-live-slot-carries-the-overlay-the-hint-is-drawn-on
// ---------------------------------------------------------------------------------------------

/// Every cell of the pack grid and of the side-pack strip is built with the overlay the hint is
/// drawn on, and none of them starts with a hint on it.
///
/// The denominator is asserted with it: a walk that found the overlay on no cells and a walk
/// that ran over no cells at all read exactly alike.
pub(super) fn every_live_slot_carries_the_overlay_the_hint_is_drawn_on() {
    let mut c = a_client_with_three_things_and_a_pack();

    let (grid_total, grid_with, grid_unhinted, strip_total, strip_with) = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        let grid = screen
            .inventory
            .item_list
            .as_ref()
            .expect("the pack grid is bound");
        let strip = screen
            .inventory
            .container_list
            .as_ref()
            .expect("the strip is bound");
        (
            grid.slots.len(),
            grid.slots
                .iter()
                .filter(|s| s.drag_accept.is_some())
                .count(),
            grid.slots.iter().all(|s| s.drag_accept_state.is_none()),
            strip.slots.len(),
            strip
                .slots
                .iter()
                .filter(|s| s.drag_accept.is_some())
                .count(),
        )
    };

    c.assert_behaviour(
        "inventory.drag-hint.every-live-slot-carries-the-overlay-the-hint-is-drawn-on",
        move |_| {
            grid_total > 1
                && strip_total > 1
                && grid_with == grid_total
                && strip_with == strip_total
                && grid_unhinted
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.drag-hint.a-real-drag-puts-the-hint-up-on-what-it-crosses-and-takes-it-down-again
// ---------------------------------------------------------------------------------------------

/// A drag is a sequence and a hint is a transition, so both are measured: the hint going **up**
/// on the cell the icon is carried over, the hint of the cell that is **left** coming down as
/// the icon moves on, and nothing left standing when the icon is let go.
///
/// The three stations are three different answers on purpose -- a cell of the player's own grid
/// takes a plain thing, the empty cell of the pack strip refuses one, and the pack itself says
/// "into this container" -- so a reading that wrote one state everywhere could not pass.
pub(super) fn a_real_drag_puts_the_hint_up_on_what_it_crosses_and_takes_it_down_again() {
    let mut c = a_client_with_three_things_and_a_pack();

    // The premise: the grid is drawing the three things and the strip the one pack, so each
    // station below is aimed at a cell that is really there.
    {
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
                .take(3)
                .collect::<Vec<_>>(),
            HINT_DRAG_ITEMS.to_vec(),
            "the three seeded things fill the first cells"
        );
        let strip = screen
            .inventory
            .container_list
            .as_ref()
            .expect("the strip is bound");
        assert_eq!(
            strip.slots[0].item,
            Some(HINT_DRAG_PACK),
            "the sack is the strip's first cell"
        );
        assert_eq!(strip.slots[1].item, None, "and the cell beside it is empty");
    }

    let from = grid_cell(&mut c, 0);
    let other_grid_cell = grid_cell(&mut c, 2);
    let empty_strip_cell = strip_cell(&mut c, 1);
    let the_pack = strip_cell(&mut c, 0);

    let from = point_of(&mut c, from);
    c.when(Grab(from));

    carry_over(&mut c, other_grid_cell);
    let over_the_grid = grid_hint(&mut c, 2);
    let the_manager_names_it = cursor_is_over(&mut c) == Some(other_grid_cell);
    let something_is_in_the_air = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen.drag_payload.is_some()
    };

    carry_over(&mut c, empty_strip_cell);
    let the_grid_cell_came_down = grid_hint(&mut c, 2) == Some(drag_accept_state::NONE);
    let over_an_empty_strip_cell = strip_hint(&mut c, 1);

    let at = carry_over(&mut c, the_pack);
    let the_empty_cell_came_down = strip_hint(&mut c, 1) == Some(drag_accept_state::NONE);
    let over_the_pack = strip_hint(&mut c, 0);

    c.when(LetGo(at));
    clear_requests(c.ui_outbox());
    let nothing_is_left_standing =
        strip_hint(&mut c, 0) == Some(drag_accept_state::NONE) && cursor_is_over(&mut c).is_none();

    c.assert_behaviour(
        "inventory.drag-hint.a-real-drag-puts-the-hint-up-on-what-it-crosses-and-takes-it-down-again",
        move |_| {
            something_is_in_the_air
                && the_manager_names_it
                && over_the_grid == Some(drag_accept_state::ACCEPT)
                && the_grid_cell_came_down
                && over_an_empty_strip_cell == Some(drag_accept_state::REFUSE)
                && the_empty_cell_came_down
                && over_the_pack == Some(drag_accept_state::INTO_CONTAINER)
                && nothing_is_left_standing
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// The two edges a crossing is reported as, and the message they are reported in.
// ---------------------------------------------------------------------------------------------

/// Move the pointer, collect the crossings that move produced, and then hand the batch to the
/// screen exactly as the frame's own delivery would.
///
/// The two halves are read at two different instants and cannot be confused: the **edges** are
/// the return value, taken before any handler has run, and the **hint** is read after the
/// caller's next frame. The delivery is done here rather than left to the frame only because
/// taking the batch to look at it is what takes it away from the frame -- a helper that drained
/// and dropped it would swallow the very messages this is about. The undoctored path, where the
/// frame drains and delivers on its own, is the scenario above.
fn cross_to(c: &mut HeadlessClient, t: f64, at: (i32, i32)) -> Vec<(ElemHandle, u32)> {
    use dereth_ui::Screen as _;

    let (ui, screen) = gameplay_screen(c.app_mut());
    ui.mouse_move(dereth_primitives::LocalTime(t), at.0, at.1);
    let batch = ui.drain_outbox();
    let edges = crossings(&batch);
    for d in &batch {
        if let dereth_ui::Delivery::Element { msg, .. } = d {
            screen.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), msg);
        }
    }
    edges
}

/// Every crossing in a batch, as (element, entering), with the copies a broadcast leaves on the
/// ancestors folded away: the claim is about the edges and not about how many ancestors the
/// shipped tree happens to give a cell.
fn crossings(msgs: &[dereth_ui::Delivery]) -> Vec<(ElemHandle, u32)> {
    let mut out: Vec<(ElemHandle, u32)> = Vec::new();
    for d in msgs {
        if let dereth_ui::Delivery::Element { msg, .. } = d {
            if msg.id == dereth_ui::msg::element::id::DRAG_CURSOR_OVER {
                let e = (msg.source, msg.p1);
                if out.last() != Some(&e) {
                    out.push(e);
                }
            }
        }
    }
    out
}

/// Every focus change in a batch, the same way.
fn focus_changes(msgs: &[dereth_ui::Delivery]) -> Vec<(ElemHandle, u32)> {
    let mut out: Vec<(ElemHandle, u32)> = Vec::new();
    for d in msgs {
        if let dereth_ui::Delivery::Element { msg, .. } = d {
            if msg.id == dereth_ui::msg::element::id::FOCUS_CHANGED {
                let e = (msg.source, msg.p1);
                if out.last() != Some(&e) {
                    out.push(e);
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// inventory.drag-hint.the-crossing-is-reported-as-a-leave-and-an-enter-in-that-order
// ---------------------------------------------------------------------------------------------

/// As a carried icon crosses from one thing that can take a drop to the next, the one being left
/// is told first and the one being entered second, and the hints follow.
///
/// Four stations, because a crossing is an edge and the three shapes it has are different: the
/// first entry, with nothing left behind; a crossing proper, with both halves; and going off
/// everything, with a leave and no enter at all.
pub(super) fn the_crossing_is_reported_as_a_leave_and_an_enter_in_that_order() {
    let mut c = a_client_with_three_things_and_a_pack();

    let from = grid_cell(&mut c, 0);
    let other_grid_cell = grid_cell(&mut c, 2);
    let empty_strip_cell = strip_cell(&mut c, 1);
    let the_pack = strip_cell(&mut c, 0);
    let at_grid = centre_of(&mut c, other_grid_cell);
    let at_empty = centre_of(&mut c, empty_strip_cell);
    let at_pack = centre_of(&mut c, the_pack);

    let start = point_of(&mut c, from);
    c.when(Grab(start));
    let really_dragging = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.is_dragging()
    };

    // Station 1: the first thing entered, with nothing left behind.
    let first = cross_to(&mut c, 1.1, at_grid);
    let named_it = cursor_is_over(&mut c) == Some(other_grid_cell);
    c.tick(1);
    let first_hint = grid_hint(&mut c, 2);

    // Station 2: leave the grid cell and enter the strip's empty one, in that order.
    let second = cross_to(&mut c, 1.2, at_empty);
    c.tick(1);
    let grid_came_down = grid_hint(&mut c, 2) == Some(drag_accept_state::NONE);
    let second_hint = strip_hint(&mut c, 1);

    // Station 3: on to the pack itself.
    let third = cross_to(&mut c, 1.3, at_pack);
    c.tick(1);
    let empty_came_down = strip_hint(&mut c, 1) == Some(drag_accept_state::NONE);
    let third_hint = strip_hint(&mut c, 0);

    // Station 4: off everything that can take a drop. A leave and no enter.
    let fourth = cross_to(&mut c, 1.4, (2, 2));
    c.tick(1);
    let pack_came_down = strip_hint(&mut c, 0) == Some(drag_accept_state::NONE);
    let over_nothing = cursor_is_over(&mut c).is_none();

    c.when(dereth_testkit::Player::Release);
    clear_requests(c.ui_outbox());

    c.assert_behaviour(
        "inventory.drag-hint.the-crossing-is-reported-as-a-leave-and-an-enter-in-that-order",
        move |_| {
            really_dragging
                && named_it
                && first == vec![(other_grid_cell, 1)]
                && first_hint == Some(drag_accept_state::ACCEPT)
                && second == vec![(other_grid_cell, 0), (empty_strip_cell, 1)]
                && grid_came_down
                && second_hint == Some(drag_accept_state::REFUSE)
                && third == vec![(empty_strip_cell, 0), (the_pack, 1)]
                && empty_came_down
                && third_hint == Some(drag_accept_state::INTO_CONTAINER)
                && fourth == vec![(the_pack, 0)]
                && pack_came_down
                && over_nothing
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.drag-hint.the-crossing-message-is-not-the-focus-message
// ---------------------------------------------------------------------------------------------

/// The message a crossing is reported in is not the message a focus change is reported in, and
/// neither of them is raised by an element merely being switched on or off.
///
/// This is asserted against what the client does and not against the constants: the two ids were
/// consistent throughout this workspace and consistently wrong, and a constant used everywhere
/// is not verified by being used everywhere. So the things that really do change the keyboard
/// focus are driven -- taking it, moving it and giving it up -- and the pairs they raise are
/// required exactly.
pub(super) fn the_crossing_message_is_not_the_focus_message() {
    let mut c = a_client_with_three_things_and_a_pack();
    let a = grid_cell(&mut c, 0);
    let b = grid_cell(&mut c, 1);

    let gained = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        let _ = ui.drain_outbox();
        ui.take_focus(a);
        focus_changes(&ui.drain_outbox())
    };
    let moved = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.take_focus(b);
        focus_changes(&ui.drain_outbox())
    };
    let given_up = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.relinquish_focus(b);
        focus_changes(&ui.drain_outbox())
    };

    // Switching an element on raises neither message, and switching it off raises the focus one
    // only for whatever holds the focus -- not for the element being switched off.
    let switched_on = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.activate(a);
        let d = ui.drain_outbox();
        (focus_changes(&d), crossings(&d))
    };
    let switched_off = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.deactivate(a);
        let d = ui.drain_outbox();
        (focus_changes(&d), crossings(&d))
    };
    let switched_off_while_something_holds_the_focus = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.activate(a);
        ui.take_focus(b);
        let _ = ui.drain_outbox();
        ui.deactivate(a);
        focus_changes(&ui.drain_outbox())
    };

    c.assert_behaviour(
        "inventory.drag-hint.the-crossing-message-is-not-the-focus-message",
        move |_| {
            gained == vec![(a, 1)]
                && moved == vec![(a, 0), (b, 1)]
                && given_up == vec![(b, 0)]
                && switched_on == (vec![], vec![])
                && switched_off == (vec![], vec![])
                && switched_off_while_something_holds_the_focus == vec![(b, 0)]
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.drag-hint.the-drop-is-aimed-at-the-element-the-cursor-was-over
// ---------------------------------------------------------------------------------------------

/// When the icon is let go, what it is let go **on** is the element the cursor was last over --
/// the cell itself, not the list that cell belongs to.
///
/// This is asserted on the message the release raises rather than on the move it becomes,
/// because the two cannot be told apart by the move: a bare list resolves to its own first cell,
/// so a reading that aimed the drop at the list would send the same request for any drag that
/// happens to end on the first cell. The message's source discriminates where the request
/// cannot.
pub(super) fn the_drop_is_aimed_at_the_element_the_cursor_was_over() {
    let mut c = a_client_with_three_things_and_a_pack();
    let from = grid_cell(&mut c, 0);
    let onto = grid_cell(&mut c, 2);
    let at_onto = centre_of(&mut c, onto);

    let start = point_of(&mut c, from);
    c.when(Grab(start));
    let picked_up = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.drag_state().owner.is_some()
    };
    let owner = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.drag_state().owner.expect("the icon was picked up").raw()
    };

    let released = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.mouse_move(dereth_primitives::LocalTime(2.1), at_onto.0, at_onto.1);
        let names_the_cell = ui.drag_state().last_drag_cursor_over == Some(onto);
        let _ = ui.drain_outbox();
        ui.mouse_up(
            dereth_ui::focus::action::PRIMARY_CLICK,
            at_onto.0,
            at_onto.1,
            false,
        );
        (names_the_cell, ui.drain_outbox())
    };
    let (the_field_names_the_cell, batch) = released;

    let aimed_at_a_target: Vec<ElemHandle> = batch
        .iter()
        .filter_map(|d| match d {
            dereth_ui::Delivery::Element { msg, .. }
                if msg.id == dereth_ui::msg::element::id::DROP_FAILED && msg.p2 != 0 =>
            {
                Some(msg.source)
            }
            _ => None,
        })
        .collect();
    let the_owners_own_copy_is_told_apart = batch.iter().any(|d| {
        matches!(d, dereth_ui::Delivery::Element { msg, .. }
            if msg.id == dereth_ui::msg::element::id::DROP_FAILED && msg.p2 == owner)
    });

    c.tick(2);
    clear_requests(c.ui_outbox());

    c.assert_behaviour(
        "inventory.drag-hint.the-drop-is-aimed-at-the-element-the-cursor-was-over",
        move |_| {
            picked_up
                && the_field_names_the_cell
                && !aimed_at_a_target.is_empty()
                && aimed_at_a_target.iter().all(|h| *h == onto)
                && the_owners_own_copy_is_told_apart
        },
    );
    c.shutdown();
}
