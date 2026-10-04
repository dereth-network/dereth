use super::*;

/// The four contracts these scenarios drive, with shipped contract table `0x0E00001D`'s own
/// names as literals, so a wrong id cannot agree with a wrong name.
///
/// `"The Shadows of Bitter Winter"` -- a repeat flag, no end NPC, quest area `0xC8DC0033`.
const CONTRACTS_SHADOWS: u32 = 1;
/// `"Test Quest Stamping"` -- the only one of the four with **both** NPCs and a timer flag.
const CONTRACTS_STAMPING: u32 = 2;
/// `"Reign of Terror"`.
const CONTRACTS_TERROR: u32 = 5;
/// `"Glenden Wood Invasion (Low)"`.
const CONTRACTS_GLENDEN: u32 = 6;

/// The order the shard sends them in -- deliberately **not** the display order.
const CONTRACTS_ARRIVAL: [u32; 4] = [
    CONTRACTS_SHADOWS,
    CONTRACTS_STAMPING,
    CONTRACTS_TERROR,
    CONTRACTS_GLENDEN,
];
/// Name sorting is case-insensitive by `_contract_name`, so
/// `"Test Quest Stamping"` sorts before `"The Shadows of Bitter Winter"` -- `'e' < 'h'`.
const CONTRACTS_BY_NAME: [u32; 4] = [
    CONTRACTS_GLENDEN,
    CONTRACTS_TERROR,
    CONTRACTS_STAMPING,
    CONTRACTS_SHADOWS,
];
/// The names, in that order, as literals out of the shipped dat.
const CONTRACTS_BY_NAME_TEXT: [&str; 4] = [
    "Glenden Wood Invasion (Low)",
    "Reign of Terror",
    "Test Quest Stamping",
    "The Shadows of Bitter Winter",
];

/// One wire contract tracker. Its receipt timestamp is **not** on the wire -- the client
/// stamps it on receipt.
fn contracts_tracker(
    id: u32,
    stage: u32,
    when_done: f64,
    when_repeats: f64,
) -> dereth_protocol::social::ContractTracker {
    dereth_protocol::social::ContractTracker {
        version: 0,
        contract_id: id,
        contract_stage: stage,
        time_when_done: when_done,
        time_when_repeats: when_repeats,
    }
}

/// `0x0314`, carrying the four contracts in [`CONTRACTS_ARRIVAL`] order.
fn contracts_table() -> dereth_protocol::social::SocialSendClientContractTrackerTable {
    dereth_protocol::social::SocialSendClientContractTrackerTable(
        dereth_protocol::archive::PackedHash {
            table_size: 32,
            entries: vec![
                (
                    CONTRACTS_SHADOWS,
                    contracts_tracker(CONTRACTS_SHADOWS, 1, 0.0, 0.0),
                ),
                (
                    CONTRACTS_STAMPING,
                    contracts_tracker(CONTRACTS_STAMPING, 2, 90.0, 0.0),
                ),
                (
                    CONTRACTS_TERROR,
                    contracts_tracker(CONTRACTS_TERROR, 3, 0.0, 0.0),
                ),
                (
                    CONTRACTS_GLENDEN,
                    contracts_tracker(CONTRACTS_GLENDEN, 4, 0.0, 0.0),
                ),
            ],
        },
    )
}

/// A client with the quest page open on the Contracts tab and the four contracts delivered
/// through the **real** `0x0314` arm.
fn a_client_on_the_contracts_tab() -> (HeadlessClient, Peer) {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let peer = Peer::attach_creating(&mut c, HOUSE_PLAYER);
    c.world_mut().player = Some(HOUSE_PLAYER);
    c.when(Inbound::message(&contracts_table()));
    c.tick(2);
    open_the_page(&mut c, contracts::PAGE);
    click_the_tab(&mut c, contracts::PAGE, contracts::PANEL);
    (c, peer)
}

/// The contracts panel, as the client holds it.
fn contracts_panel(c: &HeadlessClient) -> &ContractsPanel {
    &c.view().expect_app().hud().panels.contracts
}

/// The list box's live rows, in list order.
fn contracts_rows(c: &HeadlessClient) -> Vec<ElemHandle> {
    contracts_panel(c)
        .list
        .as_ref()
        .map(|l| l.items.clone())
        .unwrap_or_default()
}

/// The text one child of one row is really drawing, read back off the tree.
fn contracts_row_text(c: &mut HeadlessClient, id: u32) -> Vec<String> {
    let rows = contracts_rows(c);
    let (ui, _) = gameplay_screen(c.app_mut());
    rows.iter()
        .map(|r| {
            ui.get_child_recursive(*r, ElementId(id))
                .and_then(|ch| ui.text_element_mut(ch))
                .map_or_else(String::new, |t| t.glyphs.inq_text(false))
        })
        .collect()
}

/// One of the six detail lines, read back off the tree.
fn contracts_field(c: &mut HeadlessClient, id: ElementId) -> String {
    let root = {
        let (_, screen) = gameplay_screen(c.app_mut());
        screen.root().expect("the gameplay screen has a root")
    };
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.get_child_recursive(root, id)
        .and_then(|h| ui.text_element_mut(h))
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// A **real press** at the centre of a row, with the element the hit test chose handed back.
///
/// **The same rule as the Titles tab, one level further down.** A press never
/// lands on the row element: the Titles tab's row is a bare container so the hit test stops at
/// the list box, while the contracts row carries two text children that fill it,
/// so the hit test stops at `0x100005D1`. Either way the press is the **list box's** --
/// list-box press handling is what finds the item under the mouse -- and the claim
/// that matters is the selection, not the element under the cursor.
fn contracts_press_row(c: &mut HeadlessClient, index: usize) -> Option<ElementId> {
    let h = *contracts_rows(c)
        .get(index)
        .expect("the contracts list has that row");
    let at = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(h);
        ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    let hit = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.hit_test_screen(at.x, at.y)
    };
    c.when(Player::Click(Target::Point(at)));
    c.tick(2);
    let (ui, _) = gameplay_screen(c.app_mut());
    hit.and_then(|e| ui.node(e))
        .map(dereth_ui::ElementNode::element_id)
}

/// Send a button-release message to one of the panel's three buttons.
fn contracts_click_button(c: &mut HeadlessClient, id: ElementId) {
    let h = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let root = screen.root().expect("the gameplay screen has a root");
        ui.get_child_recursive(root, id)
            .unwrap_or_else(|| panic!("{id:?} is in the shipped tree"))
    };
    {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 0, 0);
    }
    c.tick(3);
}

/// Every contract this client has asked to give up.
fn contracts_abandoned(c: &HeadlessClient) -> Vec<u32> {
    c.view()
        .outbound()
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::SocialAbandonContract(m) => Some(m.contract_id),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// contracts.tab.the-tab-draws-a-row-per-contract-the-shard-sent-in-name-order
// ---------------------------------------------------------------------------------------------

/// **This cannot pass without all four halves.** Without the `0x0314` arm the tracker table is
/// empty; without the shipped contract-table loader every tracker is unresolved and drops; without a
/// reader for the world's contract trackers there is nothing to join; and without the panel there is no
/// module to draw it.
///
/// The panel joins the trackers to shipped contract table `0x0E00001D`, defaults to name sorting,
/// and fills the status column. Three status arms that were previously transcribed incorrectly
/// are asserted here on screen.
pub(super) fn the_contracts_tab_draws_a_row_per_contract_in_name_order() {
    let (mut c, _peer) = a_client_on_the_contracts_tab();

    let list_is_right = {
        let p = contracts_panel(&c);
        // Initialization found the contracts list box, the list box carries one row template,
        // the rebuild really ran -- which is what separates empty from unwired -- name order is the
        // constructor's criterion, and the rebuild ends by clearing the selected item.
        p.bound()
            && p.templates() == 1
            && p.rebuilds > 0
            && p.sort == ContractSort::Name
            && p.shown() == CONTRACTS_BY_NAME
            // ...and the sort is doing work rather than echoing the wire.
            && p.shown().as_slice() != CONTRACTS_ARRIVAL
            && p.drawn() == 4
            && p.selected.is_none()
    };

    let named = contracts_row_text(&mut c, contracts::ROW_NAME) == CONTRACTS_BY_NAME_TEXT.to_vec();
    let statuses = contracts_row_text(&mut c, contracts::ROW_STATUS)
        == vec![
            // GLENDEN is stage 4 with an empty `_description_progress` -> `"In Progress"`,
            // and not an empty line.
            "In Progress",
            // TERROR is stage 3 with no repeat timer and a **non-empty**
            // `_questflag_repeat_time` -> `"Available"`, which that transcription
            // rendered as the empty string.
            "Available",
            "In Progress",
            "Available",
        ];

    // The whole chain's counters, so an empty tab could say which link broke.
    let counters = {
        let s = &c.view().expect_app().hud().stats;
        s.contract_tables == 1 && s.contracts == 4 && s.contracts_unresolved == 0
    };

    c.assert_behaviour(
        "contracts.tab.the-tab-draws-a-row-per-contract-the-shard-sent-in-name-order",
        move |_| list_is_right && named && statuses && counters,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// contracts.detail.picking-a-contract-fills-the-pane-beside-the-list
// ---------------------------------------------------------------------------------------------

/// The detail update supplies six fields, including three that are easy to transcribe wrong.
/// Its contact choice is checked on the one contract of the four that carries **both** NPCs.
pub(super) fn picking_a_contract_fills_the_pane_beside_the_list() {
    let (mut c, _peer) = a_client_on_the_contracts_tab();

    // Nothing picked: the detail update's second early return leaves every field alone.
    let empty = contracts_panel(&c).selected.is_none()
        && contracts_field(&mut c, contracts::NOTES_TEXT).is_empty();

    // The fourth row is SHADOWS -- stage 1, no end NPC, quest area `0xC8DC0033`.
    let hit = contracts_press_row(&mut c, 3);
    let landed =
        hit == Some(contracts::CONTRACTS_BOX) || hit == Some(ElementId(contracts::ROW_NAME));
    let resolved = contracts_panel(&c).selected == Some(3);

    let notes = contracts_field(&mut c, contracts::NOTES_TEXT)
        == "Farelaith's younger brothers have been corrupted into Shadows and must be destroyed.";
    // `_name_npc_end` is empty, so `contact_is_the_end_npc` takes the start NPC.
    let contact = contracts_field(&mut c, contracts::CONTACT_TEXT) == "Shade of Farelaith";
    let progress = contracts_field(&mut c, contracts::PROGRESS_TEXT) == "Available";
    // No `_questflag_timer` selects the literal `None`.
    let timed = contracts_field(&mut c, contracts::TIMED_TEXT) == "None";
    // `0xC8DC0033` -> lcoord (1606, 1762) -> ((1762-0x400)*0.1+0.5, (1606-0x400)*0.1+0.5).
    // **Both axes subtract 0x400 and add 0.5**; this is the pane's own arithmetic rather than a
    // shared cell-coordinate conversion.
    let area = contracts_field(&mut c, contracts::AREA_TEXT) == "74.3N, 58.7E";
    // `_location_npc_start` is the same cell for this contract.
    let contact_at = contracts_field(&mut c, contracts::CONTACT_LOC_TEXT) == "74.3N, 58.7E";

    // The third row is STAMPING -- stage 2, both NPCs, a timer flag.
    let _ = contracts_press_row(&mut c, 2);
    let the_named_one = contracts_panel(&c).shown()[2] == CONTRACTS_STAMPING;
    // Stage 2 with a non-empty `_name_npc_end` takes the end NPC.
    let end_npc = contracts_field(&mut c, contracts::CONTACT_TEXT) == "Bob the Righteous and Bold";
    // `_questflag_timer` is set, so the timed line is the delta-time text of what is left of
    // `_time_when_done` -- 90 s, truncated. A running countdown, and
    // not either fallback.
    let counting = {
        let t = contracts_field(&mut c, contracts::TIMED_TEXT);
        t.ends_with('s') && t != "None" && t != "Finished"
    };

    // Picking sends nothing -- asserted rather than assumed.
    let silent = contracts_abandoned(&c).is_empty()
        && c.view().expect_app().interaction().stats.contract_requests == 0;

    c.assert_behaviour(
        "contracts.detail.picking-a-contract-fills-the-pane-beside-the-list",
        move |_| {
            empty
                && landed
                && resolved
                && notes
                && contact
                && progress
                && timed
                && area
                && contact_at
                && the_named_one
                && end_npc
                && counting
                && silent
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// contracts.abandon.the-button-gives-up-the-picked-contract-and-nothing-local-moves
// ---------------------------------------------------------------------------------------------

/// Element arm `0x100005DC` sends an abandon-contract request on the object queue:
/// opcode `0x0316`, with a four-byte body.
pub(super) fn the_abandon_button_gives_up_the_picked_contract_and_nothing_local_moves() {
    let (mut c, _peer) = a_client_on_the_contracts_tab();
    // Whatever the fixture framed is not this claim's business.
    let _ = c.take_wire_count(0x0316);

    // With nothing picked the button is silent because there is no selected index.
    contracts_click_button(&mut c, contracts::ABANDON_BUTTON);
    let idle = c.take_wire_count(0x0316) == 0 && contracts_abandoned(&c).is_empty();
    // ...and the refusal is counted, not silent.
    let counted = contracts_panel(&c).abandon_refusals == 1;

    // Pick `Reign of Terror` -- the second row -- and press Abandon.
    let _ = contracts_press_row(&mut c, 1);
    let the_named_one = contracts_panel(&c).shown()[1] == CONTRACTS_TERROR;
    contracts_click_button(&mut c, contracts::ABANDON_BUTTON);
    c.tick(3);

    let sent = contracts_abandoned(&c) == vec![CONTRACTS_TERROR]
        && c.view().expect_app().interaction().stats.contract_requests == 1;
    let framed = c.take_wire_count(0x0316) == 1;

    // What that request encodes to, and the queue it goes out on: four body bytes carrying the
    // picked row's own `_contract_id` and nothing else.
    let bytes_and_queue = {
        let m = dereth_protocol::social::SocialAbandonContract {
            contract_id: CONTRACTS_TERROR,
        };
        let mut session = dereth_client_net::client_session::Session::new(
            dereth_client_net::client_session::testing::MockTransport::new(),
        );
        let ok = dereth_client_runtime::requests::send_request(
            &mut session,
            &dereth_client_model::Request::SocialAbandonContract(m),
        );
        let packet = session.transport.sent.last().expect("one datagram").clone();
        let mut want = 0xF7B1_u32.to_le_bytes().to_vec();
        want.extend_from_slice(&1_u32.to_le_bytes());
        want.extend_from_slice(&0x0000_0316_u32.to_le_bytes());
        want.extend_from_slice(&CONTRACTS_TERROR.to_le_bytes());
        (ok, packet.payload == want, packet.queue)
    };

    // **The tracker is NOT removed locally.** The table changes when the shard answers with a
    // `0x0315` carrying `deleteContract`; a client that removed it here would show the row gone
    // and then get it back on the next `0x0314`.
    let nothing_local_moved = contracts_panel(&c).shown() == CONTRACTS_BY_NAME
        && c.view().expect_app().hud().stats.contracts == 4;

    c.assert_behaviour(
        "contracts.abandon.the-button-gives-up-the-picked-contract-and-nothing-local-moves",
        move |_| {
            idle && counted
                && the_named_one
                && sent
                && framed
                && bytes_and_queue == (true, true, dereth_primitives::NetQueue::Weenie)
                && nothing_local_moved
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// contracts.list.one-contract-at-a-time-is-added-changed-or-taken-away
// ---------------------------------------------------------------------------------------------

/// The tracker update's add, update, and delete arms are driven through the
/// production router and read back off the **tree**.
pub(super) fn one_contract_at_a_time_is_added_changed_or_taken_away() {
    let (mut c, _peer) = a_client_on_the_contracts_tab();

    // An update in place: SHADOWS moves from stage 1 to stage 2.
    deliver(
        &mut c,
        &dereth_protocol::social::SocialSendClientContractTracker {
            tracker: contracts_tracker(CONTRACTS_SHADOWS, 2, 0.0, 0.0),
            delete_contract: 0,
            set_as_display_contract: 0,
        },
    );
    c.tick(1);
    let counted_once = c.view().expect_app().hud().stats.contract_trackers_updated == 1;
    let in_place = contracts_panel(&c).shown() == CONTRACTS_BY_NAME;
    // ...and the row on screen redrew from stage 2.
    let redrawn = contracts_row_text(&mut c, contracts::ROW_STATUS)[3] == "In Progress";
    let updated = counted_once && in_place && redrawn;

    // A delete removes the matching contract entry.
    deliver(
        &mut c,
        &dereth_protocol::social::SocialSendClientContractTracker {
            tracker: contracts_tracker(CONTRACTS_SHADOWS, 2, 0.0, 0.0),
            delete_contract: 1,
            set_as_display_contract: 0,
        },
    );
    c.tick(1);
    let removed_once = c.view().expect_app().hud().stats.contract_trackers_removed == 1;
    let three_left =
        contracts_panel(&c).shown() == [CONTRACTS_GLENDEN, CONTRACTS_TERROR, CONTRACTS_STAMPING];
    // ...and the list box lost a row too.
    let removed = removed_once && three_left && contracts_panel(&c).drawn() == 3;

    // And an add, for a contract the character did not hold.
    deliver(
        &mut c,
        &dereth_protocol::social::SocialSendClientContractTracker {
            tracker: contracts_tracker(CONTRACTS_SHADOWS, 1, 0.0, 0.0),
            delete_contract: 0,
            set_as_display_contract: 1,
        },
    );
    c.tick(1);
    let added_once = {
        let s = &c.view().expect_app().hud().stats;
        // `setAsDisplayContract` is counted, because this build pins nothing to the HUD.
        s.contract_trackers_added == 1 && s.contract_display_requests == 1
    };
    let added = added_once && contracts_panel(&c).shown() == CONTRACTS_BY_NAME;

    c.assert_behaviour(
        "contracts.list.one-contract-at-a-time-is-added-changed-or-taken-away",
        move |_| updated && removed && added,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// contracts.sort.the-two-buttons-choose-the-order-and-pressing-one-twice-turns-it-round
// ---------------------------------------------------------------------------------------------

/// The sort buttons use element arms `0x100005CE` and `0x100005D6`: a press on
/// the criterion already in force **flips** the direction; a press on the other one selects it
/// and resets the direction to forward.
pub(super) fn the_sort_buttons_choose_the_order_and_a_second_press_turns_it_round() {
    let (mut c, _peer) = a_client_on_the_contracts_tab();
    let start = contracts_panel(&c).shown() == CONTRACTS_BY_NAME;

    // The name button again: same criterion -> reverse.
    contracts_click_button(&mut c, contracts::SORT_NAME_BUTTON);
    let reversed = {
        let p = contracts_panel(&c);
        p.sort == ContractSort::Name && p.reverse
    };
    let mut backwards = CONTRACTS_BY_NAME;
    backwards.reverse();
    let list_reversed = contracts_panel(&c).shown() == backwards;
    // ...and the rows on screen followed, which a model-only check would not catch.
    let mut names = CONTRACTS_BY_NAME_TEXT;
    names.reverse();
    let drawn_reversed = contracts_row_text(&mut c, contracts::ROW_NAME) == names.to_vec();

    // The status button: a different criterion -> forward, not "reverse by status".
    contracts_click_button(&mut c, contracts::SORT_STATUS_BUTTON);
    let switched = {
        let p = contracts_panel(&c);
        p.sort == ContractSort::Status && !p.reverse
    };
    // "available" before "in progress", ties by name.
    let by_status = contracts_panel(&c).shown()
        == [
            CONTRACTS_TERROR,
            CONTRACTS_SHADOWS,
            CONTRACTS_GLENDEN,
            CONTRACTS_STAMPING,
        ];

    // Sorting sends nothing.
    let silent = contracts_abandoned(&c).is_empty()
        && c.view().expect_app().interaction().stats.contract_requests == 0;

    c.assert_behaviour(
        "contracts.sort.the-two-buttons-choose-the-order-and-pressing-one-twice-turns-it-round",
        move |_| {
            start && reversed && list_reversed && drawn_reversed && switched && by_status && silent
        },
    );
    c.shutdown();
}
