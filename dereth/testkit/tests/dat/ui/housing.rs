//! UI fixtures and scenarios for housing.

use super::*;
// ---------------------------------------------------------------------------------------------
// house.tab.*
//
// Read through `HeadlessClient::ui_snapshot`, which walks to the gameplay screen, finds an element
// by id, reads its visibility and collects the text off a list box's children; what is left below
// is the gesture and the claim.
// ---------------------------------------------------------------------------------------------

/// Open one page of the toolbar's panel stack the way the toolbar button does, through the page's
/// own registered id read off the live stack.
fn open_page(c: &mut HeadlessClient, page: dereth_ui::ElementId) {
    {
        let app = c.app_mut();
        let shell = app.ui_mut().expect("the shell is up");
        let ui = &mut shell.ui;
        let screen = shell.flow.current_mut().expect("a screen is current");
        let any: &mut dyn std::any::Any = &mut **screen;
        let gameplay = any
            .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the gameplay screen is current");
        let panel_id = gameplay
            .panels
            .pages
            .iter()
            .find(|p| p.element == page)
            .map(|p| p.panel_id)
            .unwrap_or_else(|| panic!("{page:?} is one of the shipped registered pages"));
        gameplay.recv_set_panel_visibility(ui, panel_id, true);
    }
    c.tick(3);
}

/// **The gate.** Open the map page, click the House tab, read the pane.
pub(super) fn the_house_tab_tells_a_houseless_character_they_have_no_house() {
    use dereth_ui_screens::panels::house;

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));

    // The shipped layout opens that page on the *map* sub-panel, so the House pane starts down.
    assert!(
        !c.ui_snapshot().is_visible(house::PANEL),
        "the map sub-panel is the one the shipped page opens on"
    );

    open_page(&mut c, house::PAGE);
    c.when(Player::click(house::TAB));

    let after = c.ui_snapshot();
    after.assert_visible(house::PANEL);
    let lines: Vec<String> = after
        .rows_of(house::TEXT_BOX)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let pane = after.tree_text(Some(house::PANEL));
    let displays = c.view().expect_app().hud().panels.house.displays;
    let owns = c.view().expect_app().hud().panels.house.owns_house;
    let bound = c.view().expect_app().hud().panels.house.fully_bound();

    // The whole pane, and not four fields of it: a row that appeared, one that moved and one that
    // lost its text are three different changes and this is where all three show up.
    after.assert_tree("house_pane_with_no_house", Some(house::PANEL));

    c.assert_behaviour(
        "house.tab.a-character-with-no-house-is-told-so",
        move |_| {
            lines == vec![house::NO_HOUSE.to_owned()]
                && displays == 1
                && !owns
                && bound
                && pane.lines().count() > 1
        },
    );
    c.shutdown();
}

/// The other half, so the gate above cannot be satisfied by a pane that writes that line into
/// everything: the map tab of the same page shows no such row, and the House pane holds one row
/// and not one per frame.
pub(super) fn the_houseless_line_is_written_once_and_into_that_pane_alone() {
    use dereth_ui_screens::panels::house;

    /// The map sub-panel of the same page -- the one the shipped layout opens on.
    const MAP_PANEL: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_01F6);

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    open_page(&mut c, house::PAGE);
    // Several frames with the map tab up: the House pane must not add a row per frame.
    c.tick(8);
    let map_lines: Vec<String> = c
        .ui_snapshot()
        .rows_of(MAP_PANEL)
        .into_iter()
        .map(str::to_owned)
        .collect();

    c.when(Player::click(house::TAB));
    c.tick(8);

    let after = c.ui_snapshot();
    let lines: Vec<String> = after
        .rows_of(house::TEXT_BOX)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let displays = c.view().expect_app().hud().panels.house.displays;

    c.assert_behaviour(
        "house.tab.the-line-is-written-once-and-into-that-pane-alone",
        move |_| {
            lines.len() == 1
                && displays == 1
                && !map_lines.iter().any(|s| s.contains("own a house"))
        },
    );
    c.shutdown();
}
