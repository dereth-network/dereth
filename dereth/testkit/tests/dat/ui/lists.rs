//! UI fixtures and scenarios for lists.

use super::*;
// ---------------------------------------------------------------------------------------------
// ui.list.*
//
// Click a friend and the row highlights.
//
// What is read is the picture each row really put on the frame's blit list -- a drawn result and
// not a recorded intention -- rather than the rasterised pixels below it.
// ---------------------------------------------------------------------------------------------

/// Pressing a row draws the band across that row and no other.
pub(super) fn pressing_a_row_draws_the_band_across_that_row() {
    use dereth_ui::{RecordingDrawBackend, StateId};

    /// The friends panel's row template, and the two pictures a row can blit.
    const ROW_TEMPLATE: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_0519);
    const SELECTED_PLATE: dereth_primitives::DataId = dereth_primitives::DataId(0x0600_1AAF);
    const BASE_PLATE: dereth_primitives::DataId = dereth_primitives::DataId(0x0600_4CCA);
    /// The social panel, and the page the friends tab is one of.
    const SOCIAL_PANEL: u32 = 0x0C;
    const SOCIAL_PAGE: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_018F);

    fn a_friend(id: u32, name: &str) -> dereth_protocol::social::FriendData {
        dereth_protocol::social::FriendData {
            id: dereth_primitives::ObjectId(id),
            online: 1,
            appear_offline: 0,
            name: name.to_owned(),
            friends_list: Vec::new(),
            friend_of_list: Vec::new(),
        }
    }

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let mut hands = dereth_testkit::adapters_shell::Hands::new();

    // Open the social panel and its friends tab, by clicking what a player clicks.
    let button = {
        let app = c.view().expect_app();
        let shell = app.ui().expect("the UI shell is up");
        let any: &dyn std::any::Any = shell.flow.current().expect("a screen is current");
        any.downcast_ref::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the gameplay screen")
            .toolbar
            .buttons
            .iter()
            .find(|b| b.panel_id == SOCIAL_PANEL)
            .expect("the toolbar has a social button")
            .handle
    };
    hands.click_handle(&mut c, button);
    let tab = {
        let app = c.view().expect_app();
        let shell = app.ui().expect("the UI shell is up");
        let any: &dyn std::any::Any = shell.flow.current().expect("a screen is current");
        let root = any
            .downcast_ref::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the gameplay screen")
            .root()
            .expect("the gameplay root");
        let page = shell
            .ui
            .get_child_recursive(root, SOCIAL_PAGE)
            .expect("the social page");
        // The tab is discovered rather than named: it is the one whose page carries the
        // friends list, off the social panel's own table of pages and tabs.
        let pairs: Vec<(dereth_ui::ElementId, dereth_ui::ElementId)> = shell
            .ui
            .node(page)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::panel::Panel>()
            })
            .expect("the social page is a panel")
            .page_to_tab
            .iter()
            .map(|(p, t)| (*p, *t))
            .collect();
        pairs
            .into_iter()
            .find_map(|(page_id, tab_id)| {
                let pe = shell.ui.get_child_recursive(page, page_id)?;
                shell
                    .ui
                    .get_child_recursive(pe, dereth_ui_screens::panels::friends::FRIENDS_LIST)?;
                shell.ui.get_child_recursive(page, tab_id)
            })
            .expect("one page of the social panel carries the friends list")
    };
    hands.click_handle(&mut c, tab);
    c.tick(2);

    // Three friends, through the client's own receiver for the shard's message.
    c.when(dereth_testkit::Inbound::message(
        &dereth_protocol::social::SocialFriendsUpdate {
            friends: vec![
                a_friend(0x5000_001E, "Ash"),
                a_friend(0x5000_001F, "Bex"),
                a_friend(0x5000_0020, "Caius"),
            ],
            update_type: 0,
        },
    ));
    c.tick(3);

    /// The friends list, and the row elements it built from its own entry template.
    fn the_list(c: &mut HeadlessClient) -> dereth_ui::ElemHandle {
        with_gameplay_ui(c, |ui, s| {
            let root = s.root().expect("the gameplay root");
            ui.get_child_recursive(root, dereth_ui_screens::panels::friends::FRIENDS_LIST)
                .expect("the friends list")
        })
    }

    fn the_rows(c: &mut HeadlessClient) -> Vec<dereth_ui::ElemHandle> {
        let list = the_list(c);
        with_gameplay_ui(c, |ui, _| {
            ui.node(list)
                .and_then(|n| {
                    n.behaviour
                        .as_ref()?
                        .as_any()?
                        .downcast_ref::<dereth_ui::widgets::listbox::ListBox>()
                })
                .expect("the friends list is a list box")
                .items
                .clone()
        })
    }

    fn which_row_is_selected(c: &mut HeadlessClient) -> Option<usize> {
        let list = the_list(c);
        with_gameplay_ui(c, |ui, _| {
            ui.node(list)
                .and_then(|n| n.behaviour.as_ref())
                .and_then(|b| (**b).as_any())
                .and_then(|a| a.downcast_ref::<dereth_ui::widgets::listbox::ListBox>())
                .and_then(|l| l.selected)
        })
    }

    let rows = the_rows(&mut c);
    let three_rows = rows.len() == 3;

    /// The pictures this element put on the frame's own blit list.
    fn pictures(
        c: &mut HeadlessClient,
        h: dereth_ui::ElemHandle,
    ) -> Vec<dereth_primitives::DataId> {
        let mut back = RecordingDrawBackend::default();
        c.app_mut()
            .ui_mut()
            .expect("the UI shell is up")
            .ui
            .draw(&mut back);
        c.view()
            .expect_app()
            .ui_draw_list()
            .iter()
            .filter(|cmd| cmd.who == h)
            .filter_map(|cmd| cmd.image)
            .collect()
    }

    let right_template = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the UI shell is up").ui;
        rows.iter().all(|h| {
            let n = ui.node(*h).expect("the row is live");
            n.element_id() == ROW_TEMPLATE
                && n.desc.access_state(StateId(6)).is_some()
                && n.state == StateId(0)
        })
    };
    let nothing_selected = which_row_is_selected(&mut c).is_none();

    let list = the_list(&mut c);
    hands.click_row(&mut c, list, rows[1]);
    c.tick(1);

    let selected = which_row_is_selected(&mut c) == Some(1);
    let in_the_selected_state = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the UI shell is up").ui;
        ui.node(rows[1]).expect("live").state == StateId(6)
            && ui.node(rows[0]).expect("live").state == StateId(0)
            && ui.node(rows[2]).expect("live").state == StateId(0)
    };
    let bands = [
        pictures(&mut c, rows[0]),
        pictures(&mut c, rows[1]),
        pictures(&mut c, rows[2]),
    ];
    let only_that_row = bands[0] == vec![BASE_PLATE]
        && bands[1] == vec![SELECTED_PLATE]
        && bands[2] == vec![BASE_PLATE];

    // A second press moves the band and puts the first row back.
    hands.click_row(&mut c, list, rows[2]);
    c.tick(1);
    let moved = which_row_is_selected(&mut c) == Some(2)
        && pictures(&mut c, rows[1]) == vec![BASE_PLATE]
        && pictures(&mut c, rows[2]) == vec![SELECTED_PLATE]
        && pictures(&mut c, rows[0]) == vec![BASE_PLATE];

    c.assert_behaviour(
        "ui.list.pressing-a-row-draws-the-band-across-that-row-and-no-other",
        move |_| {
            three_rows
                && right_template
                && nothing_selected
                && selected
                && in_the_selected_state
                && only_that_row
                && moved
        },
    );
    c.shutdown();
}

/// Do something with the live gameplay screen.
fn with_gameplay_ui<R>(
    c: &mut HeadlessClient,
    f: impl FnOnce(
        &mut dereth_ui::UiSystem,
        &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
    ) -> R,
) -> R {
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    let screen = shell.flow.current_mut().expect("a screen is current");
    let any: &mut dyn std::any::Any = &mut **screen;
    let gameplay = any
        .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
        .expect("the gameplay screen is current");
    f(&mut shell.ui, gameplay)
}
