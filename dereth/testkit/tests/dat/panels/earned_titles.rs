use super::*;

// =============================================================================================
// titles.*
//
// The Titles tab draws the earned titles the shard sends: the message carrying the list reaches
// the panel, and the panel draws it.
// =============================================================================================

/// Three titles the shard says this character has earned, and the order their names sort in.
const WARRIOR: u32 = 8;
const ADVENTURER: u32 = 1;
const BLADEMASTER: u32 = 3;
const TITLES_EARNED: [u32; 3] = [WARRIOR, ADVENTURER, BLADEMASTER];
const TITLES_SORTED: [u32; 3] = [ADVENTURER, BLADEMASTER, WARRIOR];

/// A client on the character page's Titles tab, with a list of earned titles delivered through
/// the real arm.
fn a_client_on_the_titles_tab() -> (HeadlessClient, dereth_testkit::Peer) {
    use dereth_ui_screens::panels::titles;

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let peer = dereth_testkit::Peer::attach_creating(&mut c, HOUSE_PLAYER);
    c.world_mut().player = Some(HOUSE_PLAYER);
    c.when(Inbound::message(
        &dereth_protocol::social::CharacterTitlesMessage {
            version: 1,
            display_title: WARRIOR,
            titles: TITLES_EARNED.to_vec(),
        },
    ));
    c.tick(2);
    open_the_page(&mut c, dereth_ui_screens::panels::remaining::CHARACTER_PAGE);
    click_the_tab(
        &mut c,
        dereth_ui_screens::panels::remaining::CHARACTER_PAGE,
        titles::PANEL,
    );
    (c, peer)
}

/// The titles panel, as the client holds it.
fn titles_panel(c: &HeadlessClient) -> &dereth_ui_screens::panels::titles::TitlesPanel {
    &c.view().expect_app().hud().panels.titles
}

/// A **real press** at the centre of a row, with the element the hit test chose handed back --
/// which is the list and never the row, because no row of a shipped list is a target of its own.
fn press_title_row(c: &mut HeadlessClient, h: ElemHandle) -> Option<ElementId> {
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

/// Whether the button that wears a title is armed.
fn title_button_armed(c: &mut HeadlessClient) -> bool {
    let b = titles_panel(c).button.expect("the button is bound");
    let (ui, _) = gameplay_screen(c.app_mut());
    dereth_ui_screens::panels::titles::button_enabled(ui, b)
}

/// Every request this client has made to wear a title.
fn title_requests(c: &HeadlessClient) -> Vec<u32> {
    c.view()
        .outbound()
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::SetDisplayCharacterTitle(m) => Some(m.title_id),
            _ => None,
        })
        .collect()
}

/// **The gate.** The Titles tab draws one row per title the shard said the character has earned,
/// sorted by the name each resolves to rather than by its number, with the worn one named above
/// them -- and every row keeps its own title's number, which is what the send arm reads back.
pub(super) fn the_titles_tab_draws_every_earned_title_sorted_by_name() {
    use dereth_ui_screens::panels::titles;

    let (mut c, _peer) = a_client_on_the_titles_tab();

    let (bound, templates, rebuilt, unresolved, shown, selected) = {
        let p = titles_panel(&c);
        (
            p.bound(),
            p.templates(),
            p.rebuilds,
            p.unresolved,
            p.shown(),
            p.selected,
        )
    };
    let list_is_right = bound
        && templates == 1
        // Rebuilt at least once, which is what tells an empty list from an unwired one.
        && rebuilt > 0
        && unresolved == 0
        && shown == TITLES_SORTED
        && selected.is_none();

    let rows: Vec<(u32, ElemHandle)> = titles_panel(&c)
        .rows
        .iter()
        .map(|r| (r.id, r.element))
        .collect();
    // An instrument that cannot look reports absence: without this the loop below is vacuous.
    let there_are_rows = rows.iter().map(|(id, _)| *id).collect::<Vec<_>>() == TITLES_SORTED;
    let display = titles_panel(&c)
        .display_text
        .expect("the worn title's line is bound");
    let (drawn, ids_on_the_rows, worn) = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let drawn: Vec<String> = rows
            .iter()
            .map(|(_, h)| {
                ui.get_child_recursive(*h, ElementId(titles::ROW_TEXT))
                    .and_then(|ch| ui.text_element_mut(ch))
                    .map_or_else(String::new, |t| t.glyphs.inq_text(false))
            })
            .collect();
        let ids = rows.iter().all(|(id, h)| {
            dereth_ui_screens::bind::attr_enum(ui, *h, titles::ATTR_TITLE_ID) == Some(*id)
        });
        let worn = ui
            .text_element_mut(display)
            .map(|t| t.glyphs.inq_text(false));
        (drawn, ids, worn)
    };
    let named =
        drawn == vec!["Adventurer", "Blademaster", "Warrior"] && worn.as_deref() == Some("Warrior");

    c.assert_behaviour(
        "titles.tab.the-tab-draws-every-earned-title-sorted-by-name",
        move |_| list_is_right && there_are_rows && named && ids_on_the_rows,
    );
    c.shutdown();
}

/// Picking a row arms the button only when the title picked is not the one already worn -- and
/// picking sends nothing, which matters because a client that sent on a pick would rewrite the
/// worn title on every stray press.
pub(super) fn picking_a_title_arms_the_button_and_sends_nothing() {
    use dereth_ui_screens::panels::titles;

    let (mut c, _peer) = a_client_on_the_titles_tab();
    let nothing_armed = !title_button_armed(&mut c);

    let worn = titles_panel(&c)
        .rows
        .iter()
        .position(|r| r.id == WARRIOR)
        .expect("it is drawn");
    let h = titles_panel(&c).rows[worn].element;
    let hit = press_title_row(&mut c, h);
    let on_the_list = hit == Some(titles::TITLE_LIST);
    let worn_picked = titles_panel(&c).selected == Some(worn)
        && !title_button_armed(&mut c)
        && title_requests(&c).is_empty();

    let other = titles_panel(&c)
        .rows
        .iter()
        .position(|r| r.id == ADVENTURER)
        .expect("it is drawn");
    let h = titles_panel(&c).rows[other].element;
    press_title_row(&mut c, h);
    let other_picked = titles_panel(&c).selected == Some(other)
        && title_button_armed(&mut c)
        && title_requests(&c).is_empty();

    c.assert_behaviour(
        "titles.tab.picking-a-row-arms-the-button-only-for-a-title-not-already-worn",
        move |_| nothing_armed && on_the_list && worn_picked && other_picked,
    );
    c.shutdown();
}

/// **The gesture, end to end.** The button with nothing picked sends nothing; a row pressed and
/// then the button sends exactly one request carrying that row's own title -- and nothing local
/// moves, because which title is worn is the shard's to say.
pub(super) fn the_display_button_puts_the_set_title_request_on_the_wire() {
    let (mut c, _peer) = a_client_on_the_titles_tab();
    let button = titles_panel(&c).button.expect("the button is bound");

    {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.broadcast_element_message(button, dereth_ui::msg::element::id::BUTTON_CLICKED, 0, 0);
    }
    c.tick(3);
    let idle = title_requests(&c).is_empty();

    let want = titles_panel(&c)
        .rows
        .iter()
        .position(|r| r.id == BLADEMASTER)
        .expect("it is drawn");
    let h = titles_panel(&c).rows[want].element;
    press_title_row(&mut c, h);
    {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.broadcast_element_message(button, dereth_ui::msg::element::id::BUTTON_CLICKED, 0, 0);
    }
    c.tick(3);

    let sent = title_requests(&c) == vec![BLADEMASTER];
    let framed = c.outbound_wire().contains(&0x0000_002C);
    // What that request encodes to, and the queue it goes out on.
    let bytes_and_queue = {
        let m = dereth_protocol::social::SocialSetDisplayCharacterTitle {
            title_id: BLADEMASTER,
        };
        let mut session = dereth_client_net::client_session::Session::new(
            dereth_client_net::client_session::testing::MockTransport::new(),
        );
        let ok = dereth_client_runtime::requests::send_request(
            &mut session,
            &dereth_client_model::Request::SetDisplayCharacterTitle(m),
        );
        let packet = session.transport.sent.last().expect("one datagram").clone();
        let mut want = 0xF7B1_u32.to_le_bytes().to_vec();
        want.extend_from_slice(&1_u32.to_le_bytes());
        want.extend_from_slice(&0x0000_002C_u32.to_le_bytes());
        want.extend_from_slice(&BLADEMASTER.to_le_bytes());
        (ok, packet.payload == want, packet.queue)
    };

    // Nothing local moved: the header still says the title the shard last said was worn.
    let unchanged =
        c.view().expect_app().hud().display_title == WARRIOR && title_button_armed(&mut c);

    c.assert_behaviour(
        "titles.tab.the-button-sends-one-request-for-the-picked-title-and-nothing-local-moves",
        move |_| {
            idle && sent
                && framed
                && bytes_and_queue == (true, true, dereth_primitives::NetQueue::Weenie)
                && unchanged
        },
    );
    c.shutdown();
}
