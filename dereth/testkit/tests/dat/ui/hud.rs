//! UI fixtures and scenarios for hud.

use super::*;
// =============================================================================================
// hud.* and character-page.raise.* -- three things about the heads-up display
//
// Three rows: one is a defect this client had, and two are the original client's own behaviour,
// asserted so that a change which reverses either reddens here. The session is read from the
// decoded corpus through the harness's own reader; the claim is about the buttons, not about the
// bytes of the login.
// =============================================================================================

/// The gameplay screen and the element tree together.
pub(super) fn hud_gameplay(c: &mut HeadlessClient) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    (
        ui,
        any.downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen"),
    )
}

fn hud_root(c: &HeadlessClient) -> ElemHandle {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .flow
        .current()
        .expect("a screen")
        .roots()[0]
}

pub(crate) fn hud_find(c: &HeadlessClient, id: ElementId) -> ElemHandle {
    let shell = c.view().expect_app().ui().expect("the UI shell is up");
    shell
        .ui
        .get_child_recursive(hud_root(c), id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

pub(super) fn hud_visible(c: &HeadlessClient, h: ElemHandle) -> bool {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .is_some_and(|n| n.region.flags.visible)
}

// ---------------------------------------------------------------------------------------------
// hud.power-bar.a-jump-shows-the-one-bar-the-client-listens-with-and-never-the-other
// ---------------------------------------------------------------------------------------------

/// Every power bar in the live tree, by its id and kind.
fn power_bars(c: &HeadlessClient) -> Vec<(u32, u32, ElemHandle)> {
    use dereth_ui_screens::hud::powerbar::{FLOATY_POWERBAR_TYPE, POWERBAR_TYPE};
    fn walk(ui: &UiSystem, h: ElemHandle, out: &mut Vec<(u32, u32, ElemHandle)>) {
        if let Some(n) = ui.node(h) {
            if n.ty() == POWERBAR_TYPE || n.ty() == FLOATY_POWERBAR_TYPE {
                out.push((n.element_id().0, n.ty().0, h));
            }
        }
        for child in ui.children(h) {
            walk(ui, child, out);
        }
    }
    let shell = c.view().expect_app().ui().expect("the UI shell is up");
    let mut out = Vec::new();
    walk(&shell.ui, hud_root(c), &mut out);
    out
}

/// How many of the frame's own drawings came from under this element.
fn draws_under(c: &HeadlessClient, h: ElemHandle) -> usize {
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the UI shell is up").ui;
    app.ui_draw_list()
        .iter()
        .filter(|cmd| cmd.who == h || ui.is_ancestor_of(h, cmd.who))
        .count()
}

/// The layout carries two power bars, one at the bottom of the screen and one in the middle; a
/// jump shows only the one the client listens with.
pub(super) fn a_jump_shows_one_power_bar_and_never_the_other() {
    use dereth_ui_screens::hud::powerbar::{FLOATY_POWERBAR_TYPE, POWERBAR_TYPE};

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));

    // The denominator first: both really are in the shipped tree, so "only one showed" cannot
    // pass because the other one is missing.
    let objects = power_bars(&c);
    let both_are_there = objects
        .iter()
        .map(|(id, ty, _)| (*id, *ty))
        .collect::<Vec<_>>()
        == vec![
            (0x1000_0044, POWERBAR_TYPE.0),
            (0x1000_0613, FLOATY_POWERBAR_TYPE.0),
        ];
    let classic = objects[0].2;
    let floaty = objects[1].2;
    // Only one of the two kinds asks to hear about a power bar at all.
    let one_listens = !dereth_ui_screens::hud::powerbar::registers_power_bar_notices(
        dereth_ui::ElementType(POWERBAR_TYPE.0),
    ) && dereth_ui_screens::hud::powerbar::registers_power_bar_notices(
        dereth_ui::ElementType(FLOATY_POWERBAR_TYPE.0),
    );
    let both_start_hidden = !hud_visible(&c, classic)
        && !hud_visible(&c, floaty)
        && c.view().expect_app().hud().panels.power_bar.bound() == 2;

    // The one line a jump runs.
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .combat
        .begin_power_bar(dereth_client_model::combat::PowerBarMode::Jump, false, 0);
    c.tick(1);

    let one_came_up = hud_visible(&c, floaty)
        && !hud_visible(&c, classic)
        && draws_under(&c, floaty) > 0
        && draws_under(&c, classic) == 0;
    // ...and it is the jump bar, on exactly one of them.
    let the_jump_bar = c
        .view()
        .expect_app()
        .hud()
        .panels
        .power_bar
        .bars
        .iter()
        .filter(|b| b.visible)
        .map(|b| b.mode)
        .collect::<Vec<_>>()
        == vec![dereth_ui_screens::hud::powerbar::PowerBarMode::Jump];

    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .combat
        .hide_power_bar();
    c.tick(1);
    let and_went_away = !hud_visible(&c, floaty) && !hud_visible(&c, classic);

    c.assert_behaviour(
        "hud.power-bar.a-jump-shows-the-one-bar-the-client-listens-with-and-never-the-other",
        move |_| {
            both_are_there
                && one_listens
                && both_start_hidden
                && one_came_up
                && the_jump_bar
                && and_went_away
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// character-page.raise.both-buttons-on-both-pages-put-the-request-on-the-wire
// ---------------------------------------------------------------------------------------------

/// What this process really put on the wire since it was last asked.
fn wire_actions(c: &mut HeadlessClient) -> Vec<(u32, Vec<u8>)> {
    let out = c.replay_net_mut().expect("the endpoint").take_outgoing();
    let mut actions = Vec::new();
    for (raw, _) in &out {
        let p =
            dereth_transport::wire::ParsedPacket::parse(raw).expect("this process's own datagram");
        for f in &p.fragments {
            if f.payload.len() < dereth_protocol::OrderedActionHeader::PACK_SIZE + 4 {
                continue;
            }
            if u32::from_le_bytes(f.payload[0..4].try_into().expect("four bytes"))
                != dereth_protocol::OrderedActionHeader::MAGIC
            {
                continue;
            }
            actions.push((
                u32::from_le_bytes(f.payload[8..12].try_into().expect("four bytes")),
                f.payload[12..].to_vec(),
            ));
        }
    }
    actions
}

/// Open the character page and put the tab that owns `sub` up.
fn open_character_page(c: &mut HeadlessClient, sub: ElementId) {
    let page = dereth_ui_screens::panels::remaining::CHARACTER_PAGE;
    {
        let (ui, screen) = hud_gameplay(c);
        let panel_id = screen
            .panels
            .pages
            .iter()
            .find(|p| p.element == page)
            .map(|p| p.panel_id)
            .expect("the character page is one of the panel bar's pages");
        screen.recv_set_panel_visibility(ui, panel_id, true);
    }
    c.tick(1);
    let tab = {
        let (ui, screen) = hud_gameplay(c);
        let r = screen.root().expect("the gameplay root");
        let h = ui
            .get_child_recursive(r, page)
            .expect("the character page is in the layout");
        let t = {
            let n = ui.node(h).expect("the page has a node");
            let b = n.behaviour.as_ref().expect("the page is a panel");
            let p = (**b)
                .as_any()
                .and_then(|a| a.downcast_ref::<dereth_ui::widgets::panel::Panel>())
                .expect("its behaviour is a panel");
            p.tab_to_page
                .iter()
                .find(|(_, pg)| **pg == sub)
                .map(|(t, _)| *t)
        };
        t.and_then(|t| ui.get_child_recursive(r, t))
    };
    let th = tab.expect("the panel names a tab for this sub-page");
    {
        let (ui, _) = hud_gameplay(c);
        ui.broadcast_element_message(th, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
    }
    c.tick(3);
}

/// One footer button of one sub-page, found the way the page finds it: the container the page's
/// own look picks, then the button inside it.
fn footer_button(c: &mut HeadlessClient, panel_id: ElementId, btn: u32) -> ElemHandle {
    let (ui, screen) = hud_gameplay(c);
    let r = screen.root().expect("the gameplay root");
    let page = ui
        .get_child_recursive(r, dereth_ui_screens::panels::remaining::CHARACTER_PAGE)
        .expect("the character page");
    let panel = ui
        .get_child_recursive(page, panel_id)
        .expect("the sub-page");
    let state = ui.node(panel).map_or(0, |n| n.state.0);
    let container = ui
        .get_child_recursive(panel, ElementId(statmgmt::Footer::container_for(state)))
        .expect("the footer this look selects");
    ui.get_child_recursive(container, ElementId(btn))
        .expect("the button inside it")
}

pub(super) fn hud_centre(c: &mut HeadlessClient, h: ElemHandle) -> (i32, i32) {
    let (ui, _) = hud_gameplay(c);
    let b = ui.screen_box(h);
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

/// Press a button where a player presses it, having first proved the pointer lands on it.
fn press_footer(c: &mut HeadlessClient, hands: &mut Hands, h: ElemHandle, want: u32) {
    let at = hud_centre(c, h);
    let hit = {
        let (ui, _) = hud_gameplay(c);
        ui.hit_test_screen(at.0, at.1)
            .and_then(|e| ui.node(e))
            .map(dereth_ui::ElementNode::element_id)
    };
    assert_eq!(
        hit,
        Some(ElementId(want)),
        "the middle of the button must hit the button"
    );
    hands.click_at(c, at.0, at.1);
    // The request the press queues becomes a datagram on the frame after the one that queues it.
    c.tick(2);
}

/// A client in the world with a recorded character's own skills and attributes, and a socket-free
/// endpoint to read what it sends.
fn a_character_with_credits_to_spend() -> HeadlessClient {
    let n = dereth_client_net::client_session::testing::Corpus::load("first-login-walk-jump")
        .expect("the recordings are committed to the repository")
        .expect("first-login-walk-jump is one of them")
        .blobs
        .len();
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut net =
        dereth_client_runtime::net::ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", 0)
            .expect("a socket-free endpoint");
    net.session.transport.add_connection(
        0xB,
        0,
        1,
        0xDEAD_BEEF,
        0x1234_5678,
        Some("127.0.0.1:19000".parse().expect("the peer address")),
    );
    c.attach_replay(net);
    c.when(dereth_testkit::Inbound::from_corpus(
        "first-login-walk-jump",
        0..n,
    ));
    c.tick(4);
    let _ = wire_actions(&mut c);
    c
}

/// The one-at-a-time and ten-at-a-time raise buttons both put a request on the wire.
pub(super) fn both_raise_buttons_on_both_pages_put_the_request_on_the_wire() {
    let mut c = a_character_with_credits_to_spend();
    let mut hands = Hands::new();
    let mut every_button = true;

    for (panel_id, opcode) in [(skills::PANEL, 0x0046_u32), (attributes::PANEL, 0x0045)] {
        open_character_page(&mut c, panel_id);

        // Pick a row the way a player does.
        let row = if panel_id == skills::PANEL {
            c.view()
                .expect_app()
                .hud()
                .panels
                .skills
                .rows
                .iter()
                .find(|r| r.group == skills::SkillGroup::Trained)
                .map(|r| r.element)
        } else {
            c.view()
                .expect_app()
                .hud()
                .panels
                .attributes
                .rows
                .first()
                .map(|r| r.element)
        }
        .expect("the recorded character has a row that can be raised");
        let at = hud_centre(&mut c, row);
        hands.click_at(&mut c, at.0, at.1);
        c.tick(1);
        let _ = wire_actions(&mut c);

        let footer = if panel_id == skills::PANEL {
            c.view()
                .expect_app()
                .hud()
                .panels
                .skills
                .footer_content
                .clone()
        } else {
            c.view()
                .expect_app()
                .hud()
                .panels
                .attributes
                .footer_content
                .clone()
        };
        every_button &= footer.button == statmgmt::button_state::ENABLED
            && footer.button_10 == statmgmt::button_state::ENABLED
            && footer.button_10_visible;

        // One at a time.
        let one = footer_button(&mut c, panel_id, statmgmt::child::BUTTON);
        press_footer(&mut c, &mut hands, one, statmgmt::child::BUTTON);
        let sent = wire_actions(&mut c);
        every_button &= sent.len() == 1 && sent[0].0 == opcode && sent[0].1.len() == 8;
        let one_xp = u32::from_le_bytes(sent[0].1[4..8].try_into().expect("four bytes"));

        // The shard's answer is what lets the next press through, and there is no shard here, so
        // it is cleared the way that answer clears it.
        if panel_id == skills::PANEL {
            c.app_mut()
                .probe_mut()
                .hud_mut()
                .panels
                .skills
                .clear_awaiting_raise();
        } else {
            c.app_mut()
                .probe_mut()
                .hud_mut()
                .panels
                .attributes
                .clear_awaiting_raise();
        }

        // Ten at a time.
        let ten = footer_button(&mut c, panel_id, statmgmt::child::BUTTON_10);
        press_footer(&mut c, &mut hands, ten, statmgmt::child::BUTTON_10);
        let sent = wire_actions(&mut c);
        every_button &= sent.len() == 1 && sent[0].0 == opcode && sent[0].1.len() == 8;
        let ten_xp = u32::from_le_bytes(sent[0].1[4..8].try_into().expect("four bytes"));
        let wanted = if panel_id == skills::PANEL {
            c.view().expect_app().hud().panels.skills.selected_skill
        } else {
            c.view()
                .expect_app()
                .hud()
                .panels
                .attributes
                .selected()
                .map(dereth_ui_screens::panels::attributes::AttributeRow::wire_stat)
                .expect("a picked row")
        };
        every_button &= sent[0].1[0..4] == wanted.to_le_bytes();
        // ...and the two presses are asking for different amounts, which is the whole difference.
        every_button &= ten_xp > one_xp;

        if panel_id == skills::PANEL {
            c.app_mut()
                .probe_mut()
                .hud_mut()
                .panels
                .skills
                .clear_awaiting_raise();
        } else {
            c.app_mut()
                .probe_mut()
                .hud_mut()
                .panels
                .attributes
                .clear_awaiting_raise();
        }
    }

    c.assert_behaviour(
        "character-page.raise.both-buttons-on-both-pages-put-the-request-on-the-wire",
        move |_| every_button,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// hud.vitals.the-first-press-changes-nothing-on-screen-and-the-second-hides-the-numbers
// ---------------------------------------------------------------------------------------------

/// The vitals bar takes two presses to lose its numbers, which is the original's own arithmetic
/// over the original's own layout.
pub(super) fn the_first_press_on_the_vitals_bar_changes_nothing_on_screen() {
    use dereth_ui_screens::hud::vitals::vitals_display as v;

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let stacked = hud_find(&c, window::STACKED_VITALS);
    let health_label = hud_find(&c, ElementId(0x1000_00EB));

    // The layout names no look for this window to start in, anywhere in its subtree.
    let no_default_look = {
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        fn every_default(ui: &UiSystem, h: ElemHandle, out: &mut Vec<(u32, u32)>) {
            if let Some(n) = ui.node(h) {
                out.push((n.element_id().0, n.desc.default_state.0));
            }
            for child in ui.children(h) {
                every_default(ui, child, out);
            }
        }
        let mut all = Vec::new();
        every_default(ui, stacked, &mut all);
        all.iter().all(|(_, s)| *s == 0) && all.len() > 20
    } && c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(stacked)
        .map(|n| n.state)
        == Some(StateId(0));

    // What the two looks it can be put into do, and what the one it starts in does not do.
    let the_two_looks = {
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        let d = &ui.node(health_label).expect("the health label").desc;
        let hide_in = |s: StateId| {
            d.access_state(s)
                .and_then(|st| st.properties.get_bool(dereth_ui::props::attr::HIDE))
        };
        hide_in(v::STATE_A) == Some(false)
            && hide_in(v::STATE_B) == Some(true)
            && d.base
                .properties
                .get_bool(dereth_ui::props::attr::HIDE)
                .is_none()
    };

    let starts_numeric = hud_visible(&c, health_label) && v::next_state(StateId(0)) == v::STATE_A;
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .set_state(stacked, v::STATE_A);
    c.tick(1);
    // The first press: the look changes and nothing on the screen does.
    let first_press_shows_nothing = hud_visible(&c, health_label);
    let next = v::next_state(v::STATE_A);
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .set_state(stacked, next);
    c.tick(1);
    let second_press_hides_them = !hud_visible(&c, health_label);

    c.assert_behaviour(
        "hud.vitals.the-first-press-changes-nothing-on-screen-and-the-second-hides-the-numbers",
        move |_| {
            no_default_look
                && the_two_looks
                && starts_numeric
                && first_press_shows_nothing
                && second_press_hides_them
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// hud.vitals.a-press-on-the-bar-flips-it-between-its-two-presentations
// ---------------------------------------------------------------------------------------------

/// Pressing the vitals bar flips it between its two presentations.
pub(super) fn a_press_on_the_vitals_bar_flips_it_between_its_two_presentations() {
    use dereth_ui_screens::hud::vitals::vitals_display as v;

    // The size matters: at the smaller one the notice strip lies over the lower part of the bar.
    let mut c = HeadlessClient::new(ClientSpec {
        width: 1024,
        height: 768,
        ..ClientSpec::gameplay(4)
    });

    // The two presentations are the **layout's**, so they are read before anything is asserted to
    // set them: a data change is then a different failure from a client one.
    let mut both_authored = true;
    for id in [window::STACKED_VITALS, window::SIDE_VITALS] {
        let h = hud_find(&c, id);
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        let desc = &shell.ui.node(h).expect("the window").desc;
        both_authored &=
            desc.access_state(v::STATE_A).is_some() && desc.access_state(v::STATE_B).is_some();
    }

    let stacked = hud_find(&c, window::STACKED_VITALS);
    let untouched = hud_state(&c, stacked) == StateId(0);

    // **Where the press lands, asserted before it is made.** The bar's own middle is what a
    // player aims at; the hit test is the only thing that decides which element a press reaches,
    // and a miss and a missing answer are the same red without this line.
    let at = {
        let b = c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .screen_box(stacked);
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    let hit = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .hit_test_screen(at.0, at.1);
    let lands_on_the_bar = hit.is_some_and(|h| under(&c, h, window::STACKED_VITALS));

    let mut hands = Hands::new();
    hands.click_at(&mut c, at.0, at.1);
    let answered = {
        let (_, s) = hud_gameplay(&mut c);
        s.vitals_display_toggles == 1
    } && hud_state(&c, stacked) == v::STATE_A;

    hands.click_at(&mut c, at.0, at.1);
    let the_other = hud_state(&c, stacked) == v::STATE_B;
    hands.click_at(&mut c, at.0, at.1);
    let a_toggle = hud_state(&c, stacked) == v::STATE_A;

    // The whole element swaps and not only its frame: the meters inside it author both
    // presentations too, so a change that stopped at the window would leave the numbers behind.
    let meter_group = hud_find(&c, ElementId(0x1000_00E6));
    let the_whole_thing = hud_state(&c, meter_group) == v::STATE_A;

    c.assert_behaviour(
        "hud.vitals.a-press-on-the-bar-flips-it-between-its-two-presentations",
        move |_| {
            both_authored
                && untouched
                && lands_on_the_bar
                && answered
                && the_other
                && a_toggle
                && the_whole_thing
        },
    );
    c.shutdown();
}
