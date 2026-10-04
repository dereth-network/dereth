use super::*;

/// The world and the character the notebook's own name is made of.
const JOURNAL_WORLD: &str = "Frostfell";
const JOURNAL_CHARACTER: &str = "Kupotest";

/// A client in the world whose settings live in a disposable directory of their own.
fn a_client_with_a_notebook(tag: &str) -> HeadlessClient {
    HeadlessClient::new(ClientSpec::gameplay_in_world(4).with_scratch_settings(tag))
}

/// Tell the client who it is playing, the way logging in does.
///
/// **It happens after the opening frames on purpose.** A real client has no character name until
/// it logs on, so the notebook's own latch happens first and the file is read on a later frame --
/// which is the half of the load a scenario that set this up front would never reach.
fn playing_as(c: &mut HeadlessClient, character: &str) {
    let host = c.app_mut().probe_mut().host_state_mut();
    host.entered_character = Some(character.to_owned());
    host.world_name = Some(JOURNAL_WORLD.to_owned());
    c.tick(2);
}

/// Where this client's notebook is written.
fn notebook_path(c: &HeadlessClient, character: &str) -> std::path::PathBuf {
    let dir = c
        .scratch_settings()
        .expect("this scenario asked for a settings directory")
        .dir();
    journal::JournalIdentity {
        directory: dir.to_path_buf(),
        world: JOURNAL_WORLD.to_owned(),
        character: character.to_owned(),
    }
    .client_path()
}

/// One element of the shipped layout under the gameplay root.
pub(super) fn el(c: &mut HeadlessClient, id: ElementId) -> ElemHandle {
    let (ui, screen) = gameplay_screen(c.app_mut());
    let root = screen.root().expect("the gameplay screen has a root");
    ui.get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

pub(super) fn el_text(c: &mut HeadlessClient, id: ElementId) -> String {
    let h = el(c, id);
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

pub(super) fn el_visible(c: &mut HeadlessClient, id: ElementId) -> bool {
    let h = el(c, id);
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.node(h).expect("a live node").region.flags.visible
}

/// Open one page of the toolbar's stack, the way the toolbar button does.
pub(super) fn open_the_page(c: &mut HeadlessClient, page: ElementId) {
    {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let panel_id = screen
            .panels
            .pages
            .iter()
            .find(|p| p.element == page)
            .map(|p| p.panel_id)
            .unwrap_or_else(|| panic!("{page:?} is one of the shipped registered pages"));
        screen.recv_set_panel_visibility(ui, panel_id, true);
    }
    c.tick(3);
}

/// Press the tab that owns `sub`, found through the page's own tab table rather than named.
pub(super) fn click_the_tab(c: &mut HeadlessClient, page: ElementId, sub: ElementId) {
    let handle = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let root = screen.root().expect("the gameplay screen has a root");
        let h = ui
            .get_child_recursive(root, page)
            .expect("the page is in the shipped layout");
        let tab = {
            let n = ui.node(h).expect("the page has a node");
            let b = n.behaviour.as_ref().expect("the page has a behaviour");
            (**b)
                .as_any()
                .and_then(|a| a.downcast_ref::<dereth_ui::widgets::panel::Panel>())
                .expect("the page is a panel")
                .tab_to_page
                .iter()
                .find(|(_, pg)| **pg == sub)
                .map(|(t, _)| *t)
                .expect("the page names a tab for this sub-panel")
        };
        ui.get_child_recursive(root, tab)
            .expect("the tab element is in the shipped layout")
    };
    {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.broadcast_element_message(handle, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
    }
    c.tick(3);
}

/// Open the quest page with `sub` up. Every journal scenario starts here.
fn open_the_journal(c: &mut HeadlessClient, sub: ElementId) {
    open_the_page(c, journal::PAGE);
    click_the_tab(c, journal::PAGE, sub);
}

/// **A real pointer press**, with the element the hit test chose handed back -- so a scenario can
/// say that the press landed on the control it meant and not on whatever is drawn over it.
fn press_control(c: &mut HeadlessClient, id: ElementId) -> Option<ElementId> {
    let h = el(c, id);
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

/// Put the caret in a box **by pressing it**, and prove the caret arrived before anything is
/// typed.
fn press_into(c: &mut HeadlessClient, id: ElementId) {
    let h = el(c, id);
    let hit = press_control(c, id);
    assert_eq!(
        hit,
        Some(id),
        "the pointer landed on {id:?} and not on something drawn over it"
    );
    let (ui, _) = gameplay_screen(c.app_mut());
    assert_eq!(
        ui.focus_element(),
        Some(h),
        "a press on {id:?} must take the caret; nothing else in this client moves it on a click"
    );
}

/// Empty a box. The client's own clear is not reachable by any gesture, so a scenario that needs
/// an empty box says so here rather than pretending to rub it out.
fn empty_the_box(c: &mut HeadlessClient, id: ElementId) {
    let h = el(c, id);
    let (ui, _) = gameplay_screen(c.app_mut());
    if let Some(t) = ui.text_element_mut(h) {
        t.set_text("");
    }
}

/// Press a box and type into it.
fn write_in(c: &mut HeadlessClient, id: ElementId, text: &str) {
    press_into(c, id);
    dereth_testkit::input_steps::type_text(c, text);
    c.tick(1);
}

/// Every row of the page list, as `(number, title, timer, label)`.
fn page_list_rows(c: &mut HeadlessClient) -> Vec<(String, String, String, String)> {
    let list = el(c, pagelist::LIST);
    let (ui, _) = gameplay_screen(c.app_mut());
    let mut out = Vec::new();
    for row in ui.children(list) {
        let mut cell = |id: ElementId| -> String {
            ui.get_child_recursive(row, id)
                .and_then(|h| ui.text_element_mut(h))
                .map_or_else(String::new, |t| t.glyphs.inq_text(false))
        };
        let n = cell(pagelist::ROW_PAGE_NUMBER);
        if n.is_empty() {
            continue;
        }
        out.push((
            n,
            cell(pagelist::ROW_TITLE),
            cell(pagelist::ROW_TIMER),
            cell(pagelist::ROW_LABEL),
        ));
    }
    out
}

fn page_titles(c: &mut HeadlessClient) -> Vec<String> {
    page_list_rows(c).into_iter().map(|r| r.1).collect()
}

/// Write three titled pages through the journal tab, every keystroke through the pointer.
fn three_journal_pages(c: &mut HeadlessClient) {
    open_the_journal(c, journal::PANEL);
    for (i, title) in ["Aluvian", "Banderling", "Colosseum"]
        .into_iter()
        .enumerate()
    {
        empty_the_box(c, journal::TITLE_EDIT);
        write_in(c, journal::TITLE_EDIT, title);
        if i < 2 {
            press_control(c, journal::NEW_PAGE_BUTTON);
        }
    }
    press_control(c, journal::FIRST_PAGE_BUTTON);
}

/// Nothing about the journal ever leaves the machine.
fn nothing_was_sent(c: &HeadlessClient) -> bool {
    c.view().outbound().is_empty()
}

/// **The gate.** The quest page opens on another tab; pressing the journal's own tab puts a
/// notebook up, open at page one, with the timer's three boxes showing and its countdown down.
pub(super) fn the_journal_opens_on_page_one_with_the_timer_editable() {
    let mut c = a_client_with_a_notebook("journal-open");

    let closed_first = !el_visible(&mut c, journal::PANEL);
    open_the_journal(&mut c, journal::PANEL);
    let up = el_visible(&mut c, journal::PANEL);

    let page_one = el_text(&mut c, journal::PAGE_NUMBER_TEXT) == "~ 1 ~";
    let button = el_text(&mut c, journal::START_TIMER_BUTTON) == journal::START;
    let nowhere = el_text(&mut c, journal::LOCATION_TEXT) == journal::NONE;
    let editable = [
        journal::DAYS_EDIT,
        journal::DAYS_TEXT,
        journal::HOURS_EDIT,
        journal::HOURS_TEXT,
        journal::MINUTES_EDIT,
        journal::MINUTES_TEXT,
    ]
    .into_iter()
    .all(|id| el_visible(&mut c, id));
    let countdown_down = !el_visible(&mut c, journal::TIMER_TEXT);
    let made = c.view().expect_app().hud().panels.journal.pages.len() == 1
        && c.view().expect_app().hud().panels.journal.fully_bound();
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.page.the-tab-opens-on-page-one-with-the-timer-ready-to-set",
        move |_| {
            closed_first
                && up
                && page_one
                && button
                && nowhere
                && editable
                && countdown_down
                && made
                && silent
        },
    );
    c.shutdown();
}

/// **The typing reaches the notebook.** A title typed on page one is gone from the box on page
/// two and back again on the way back -- which is the page being written down rather than the box
/// being left alone.
pub(super) fn a_typed_title_survives_a_page_turn() {
    let mut c = a_client_with_a_notebook("journal-turn");
    open_the_journal(&mut c, journal::PANEL);

    let title = "Hollow Minion";
    write_in(&mut c, journal::TITLE_EDIT, title);
    let typed = el_text(&mut c, journal::TITLE_EDIT) == title;

    let hit = press_control(&mut c, journal::NEW_PAGE_BUTTON);
    let turned = hit == Some(journal::NEW_PAGE_BUTTON)
        && el_text(&mut c, journal::PAGE_NUMBER_TEXT) == "~ 2 ~"
        && el_text(&mut c, journal::TITLE_EDIT).is_empty()
        && c.view().expect_app().hud().panels.journal.pages.len() == 2;

    let hit = press_control(&mut c, journal::PREV_PAGE_BUTTON);
    let back = hit == Some(journal::PREV_PAGE_BUTTON)
        && el_text(&mut c, journal::PAGE_NUMBER_TEXT) == "~ 1 ~"
        && el_text(&mut c, journal::TITLE_EDIT) == title
        && c.view().expect_app().hud().panels.journal.pages[0].title == title;
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.page.a-title-typed-on-one-page-is-there-on-the-way-back",
        move |_| typed && turned && back && silent,
    );
    c.shutdown();
}

/// **The countdown.** Two minutes typed in and started: the button says reset, the three boxes go
/// away, the countdown comes up reading what was set, and it runs down on its own with no further
/// input. Pressing again puts the boxes back.
pub(super) fn the_journal_timer_counts_down_and_the_button_resets_it() {
    let mut c = a_client_with_a_notebook("journal-timer");
    open_the_journal(&mut c, journal::PANEL);

    empty_the_box(&mut c, journal::MINUTES_EDIT);
    write_in(&mut c, journal::MINUTES_EDIT, "2");
    let hit = press_control(&mut c, journal::START_TIMER_BUTTON);
    let started = hit == Some(journal::START_TIMER_BUTTON)
        && el_text(&mut c, journal::START_TIMER_BUTTON) == journal::RESET
        && el_visible(&mut c, journal::TIMER_TEXT)
        && !el_visible(&mut c, journal::MINUTES_EDIT);
    let first = el_text(&mut c, journal::TIMER_TEXT);
    // **Two minutes, less the frames the press itself costs.** The countdown starts the moment
    // the button is pressed and the client's clock moves a fixed amount per frame, so the first
    // reading a scenario can take is already a second or so down. The shape is exact and the
    // number is a window.
    let read_seconds = |t: &str| -> Option<i64> {
        t.split_once("m ").and_then(|(m, s)| {
            Some(m.parse::<i64>().ok()? * 60 + s.strip_suffix('s')?.parse::<i64>().ok()?)
        })
    };
    let reads_two_minutes = read_seconds(&first).is_some_and(|r| (117..=120).contains(&r));

    // Nothing but time: the headless client steps its own clock a fixed amount per frame.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (5.0 / dereth_client::app::HEADLESS_STEP).ceil() as u64 + 2;
    c.tick(frames);
    let later = el_text(&mut c, journal::TIMER_TEXT);
    let moved = later != first;
    let remaining = read_seconds(&later);
    // Two minutes less about five seconds of frames; the frame step and the redraw period both
    // round, so the window is the claim and the shape is exact.
    let counted_down = remaining.is_some_and(|r| (112..120).contains(&r));

    let hit = press_control(&mut c, journal::START_TIMER_BUTTON);
    let reset = hit == Some(journal::START_TIMER_BUTTON)
        && el_text(&mut c, journal::START_TIMER_BUTTON) == journal::START
        && el_visible(&mut c, journal::MINUTES_EDIT)
        && !el_visible(&mut c, journal::TIMER_TEXT);
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.timer.it-counts-down-on-its-own-and-the-button-puts-it-back",
        move |_| started && reads_two_minutes && moved && counted_down && reset && silent,
    );
    c.shutdown();
}

/// **The page list is the journal's own pages.** Three written pages are three rows, numbered and
/// titled in page order, with no timer running on any of them.
pub(super) fn the_page_list_lists_the_journals_pages() {
    let mut c = a_client_with_a_notebook("journal-list");
    three_journal_pages(&mut c);
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);
    let up = el_visible(&mut c, pagelist::PANEL);

    let rows = page_list_rows(&mut c);
    let numbered = rows.iter().map(|r| r.0.clone()).collect::<Vec<_>>() == vec!["1", "2", "3"];
    let titled = rows.iter().map(|r| r.1.clone()).collect::<Vec<_>>()
        == vec!["Aluvian", "Banderling", "Colosseum"];
    let no_timers = rows.iter().all(|r| r.2 == journal::NONE);
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.list.the-list-is-the-journals-own-pages-in-page-order",
        move |_| up && numbered && titled && no_timers && silent,
    );
    c.shutdown();
}

/// **Search, from both sides.** Typing into the search box narrows
/// nothing by itself -- the search control is the only thing that filters -- and pressing it
/// narrows the list to the pages whose words contain what was typed, whatever the case.
pub(super) fn only_the_search_control_filters_the_page_list() {
    let mut c = a_client_with_a_notebook("journal-search");
    three_journal_pages(&mut c);
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);
    let all_three = page_titles(&mut c) == vec!["Aluvian", "Banderling", "Colosseum"];

    write_in(&mut c, pagelist::SEARCH_EDIT, "band");
    let typed_but_unfiltered = el_text(&mut c, pagelist::SEARCH_EDIT) == "band"
        && page_titles(&mut c) == vec!["Aluvian", "Banderling", "Colosseum"];

    let hit = press_control(&mut c, pagelist::SEARCH_BUTTON);
    let filtered =
        hit == Some(pagelist::SEARCH_BUTTON) && page_titles(&mut c) == vec!["Banderling"];
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.list.typing-alone-does-not-filter-and-the-search-control-does",
        move |_| all_three && typed_but_unfiltered && filtered && silent,
    );
    c.shutdown();
}

/// **The reset control clears the search and deletes nothing.** It puts the whole list back with
/// every page still in the notebook -- and on an empty box it is a gesture that changes nothing
/// and still ran, which is the half that would hide a reset wired to the delete arm.
pub(super) fn reset_clears_the_search_and_deletes_nothing() {
    let mut c = a_client_with_a_notebook("journal-reset");
    three_journal_pages(&mut c);
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);

    write_in(&mut c, pagelist::SEARCH_EDIT, "colo");
    press_control(&mut c, pagelist::SEARCH_BUTTON);
    let narrowed = page_titles(&mut c) == vec!["Colosseum"];

    let hit = press_control(&mut c, pagelist::CLEAR_SEARCH_BUTTON);
    let cleared = hit == Some(pagelist::CLEAR_SEARCH_BUTTON)
        && el_text(&mut c, pagelist::SEARCH_EDIT).is_empty()
        && page_titles(&mut c) == vec!["Aluvian", "Banderling", "Colosseum"]
        && c.view().expect_app().hud().panels.journal.pages.len() == 3;

    // On an empty box: nothing changes, and nothing is deleted either.
    let before = page_list_rows(&mut c);
    let hit = press_control(&mut c, pagelist::CLEAR_SEARCH_BUTTON);
    let harmless = hit == Some(pagelist::CLEAR_SEARCH_BUTTON)
        && page_list_rows(&mut c) == before
        && c.view().expect_app().hud().panels.journal.pages.len() == 3;
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.list.the-reset-control-clears-the-search-and-deletes-no-page",
        move |_| narrowed && cleared && harmless && silent,
    );
    c.shutdown();
}

/// **Selection, asserted by its consequence.** Pressing a row and
/// then the delete control removes *that* row's page and no other -- which is the only way to see
/// that the press selected anything at all.
pub(super) fn pressing_a_row_then_delete_removes_that_rows_page() {
    let mut c = a_client_with_a_notebook("journal-delete");
    three_journal_pages(&mut c);
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);
    let all_three = page_titles(&mut c) == vec!["Aluvian", "Banderling", "Colosseum"];

    let row = {
        let list = el(&mut c, pagelist::LIST);
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.children(list).get(1).copied().expect("three rows")
    };
    let at = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(row);
        ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    // A press over a row lands on the **list**, because no row of the shipped list is a target of
    // its own: the list works out which row was pressed from where the pointer was.
    let hit = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.hit_test_screen(at.x, at.y)
            .and_then(|h| ui.node(h))
            .map(dereth_ui::ElementNode::element_id)
    };
    let on_the_list = hit == Some(pagelist::LIST);
    c.when(Player::Click(Target::Point(at)));
    c.tick(2);

    let hit = press_control(&mut c, pagelist::DELETE_BUTTON);
    let deleted = hit == Some(pagelist::DELETE_BUTTON)
        && page_titles(&mut c) == vec!["Aluvian", "Colosseum"]
        && c.view().expect_app().hud().panels.journal.pages.len() == 2;
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.list.a-pressed-row-is-the-one-the-delete-control-removes",
        move |_| all_three && on_the_list && deleted && silent,
    );
    c.shutdown();
}

/// **A double press on a row opens that page in the journal.** Two presses inside the double-press
/// window on the third row, and the journal comes forward showing page three.
pub(super) fn a_double_press_on_a_row_opens_that_page() {
    let mut c = a_client_with_a_notebook("journal-open-row");
    three_journal_pages(&mut c);
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);
    let three_rows = page_list_rows(&mut c).len() == 3;
    let on_page_one = el_text(&mut c, journal::PAGE_NUMBER_TEXT) == "~ 1 ~";

    let row = {
        let list = el(&mut c, pagelist::LIST);
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.children(list).get(2).copied().expect("three rows")
    };
    let at = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(row);
        ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    for _ in 0..2 {
        c.when(Player::Click(Target::Point(at)));
        c.tick(1);
    }

    let opened = el_text(&mut c, journal::PAGE_NUMBER_TEXT) == "~ 3 ~"
        && el_text(&mut c, journal::TITLE_EDIT) == "Colosseum";
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.list.a-double-press-on-a-row-opens-that-page-in-the-journal",
        move |_| three_rows && on_page_one && opened && silent,
    );
    c.shutdown();
}

/// **The notebook survives a restart, and the file is the shipped format.** A page written and
/// committed is on disk in the twelve records the client writes; a client started afresh with that
/// file beside it comes up with the page in it.
///
/// The file's name is the world's and the character's, which is what the next scenario is about.
pub(super) fn a_journal_page_survives_a_restart() {
    let (written, path_name) = {
        let mut c = a_client_with_a_notebook("journal-save");
        playing_as(&mut c, JOURNAL_CHARACTER);
        let path = notebook_path(&c, JOURNAL_CHARACTER);
        let name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .expect("the notebook has a name")
            .to_owned();

        open_the_page(&mut c, journal::PAGE);
        click_the_tab(&mut c, journal::PAGE, journal::PANEL);
        // Opening the tab has already written the blank page: both edges save.
        assert_eq!(c.view().expect_app().hud().panels.journal.page_saves, 1);
        assert!(
            path.exists(),
            "the file is on disk before a single character is typed"
        );

        write_in(&mut c, journal::LABEL_EDIT, "Hunt");
        write_in(&mut c, journal::TITLE_EDIT, "The Bandit Camp");
        write_in(&mut c, journal::NOTES_EDIT, "Two lives left");
        empty_the_box(&mut c, journal::DAYS_EDIT);
        write_in(&mut c, journal::DAYS_EDIT, "3");

        // Leaving the tab is what commits the page.
        click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);
        assert_eq!(c.view().expect_app().hud().panels.journal.page_saves, 2);
        let text = std::fs::read_to_string(&path).expect("the client wrote the notebook");
        assert!(nothing_was_sent(&c), "the journal sends nothing");
        c.shutdown();
        (text, name)
    };

    let named = path_name == "Journal-Frostfell-Kupotest.txt";
    let in_format = written
        == "<NEWP>\n\
            <LABE> Hunt\n\
            <TITL> The Bandit Camp\n\
            <NOTE> Two lives left\n\
            <DAYS> 3\n\
            <HOUR> 0\n\
            <MINU> 0\n\
            <LOC?> FALSE\n\
            <LOCX> 0.000000\n\
            <LOCY> 0.000000\n\
            <TIM?> FALSE\n\
            <TIME> 0.000000\n\
            \n";

    // A client started afresh with that file beside it. It is a second client and not the same
    // one: a new element tree, a new panel, a new notebook.
    let mut c = a_client_with_a_notebook("journal-load");
    std::fs::write(notebook_path(&c, JOURNAL_CHARACTER), &written)
        .expect("the disposable settings directory is writable");
    playing_as(&mut c, JOURNAL_CHARACTER);
    let read_it = c.view().expect_app().hud().panels.journal.page_loads == 1;

    open_the_page(&mut c, journal::PAGE);
    click_the_tab(&mut c, journal::PAGE, journal::PANEL);
    let came_back = el_text(&mut c, journal::TITLE_EDIT) == "The Bandit Camp"
        && el_text(&mut c, journal::LABEL_EDIT) == "Hunt"
        && el_text(&mut c, journal::NOTES_EDIT) == "Two lives left"
        && el_text(&mut c, journal::DAYS_EDIT) == "3"
        && el_text(&mut c, journal::PAGE_NUMBER_TEXT) == "~ 1 ~"
        && c.view().expect_app().hud().panels.journal.pages.len() == 1;
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.file.a-page-written-here-is-on-disk-in-the-shipped-format-and-comes-back",
        move |_| named && in_format && read_it && came_back && silent,
    );
    c.shutdown();
}

/// **The negative that makes the one above a measurement.** A client that does not know which
/// character it is playing writes **nothing at all** -- where one shared notebook for every
/// character on every world would pass the restart claim and be wrong in the way that matters
/// most.
pub(super) fn a_client_that_does_not_know_its_character_writes_nothing() {
    let mut c = a_client_with_a_notebook("journal-nameless");
    let dir = c
        .scratch_settings()
        .expect("a settings directory")
        .dir()
        .to_path_buf();

    open_the_page(&mut c, journal::PAGE);
    click_the_tab(&mut c, journal::PAGE, journal::PANEL);
    write_in(&mut c, journal::TITLE_EDIT, "Nobody");
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);

    let no_file = c.view().expect_app().hud().panels.journal.file.is_none();
    let nothing_written = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .all(|f| !f.file_name().to_string_lossy().starts_with("Journal-"));
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.file.a-client-that-does-not-know-its-character-writes-no-notebook",
        move |_| no_file && nothing_written && silent,
    );
    c.shutdown();
}

/// **A notebook in the shipped format is read and then written back in place.** One written by
/// another client -- with a record this client does not write and a note whose line break is
/// stored as a tab -- is parsed whole, and an edit lands in that same file.
pub(super) fn a_notebook_in_the_shipped_format_is_read_and_written_back() {
    let mut c = a_client_with_a_notebook("journal-retail");
    let path = notebook_path(&c, JOURNAL_CHARACTER);
    let original = "<NEWP>\n\
                    <PNUM> 1\n\
                    <LABE> Old\n\
                    <TITL> Written by another client\n\
                    <NOTE> first\tsecond\n\
                    <DAYS> 0\n\
                    <HOUR> 2\n\
                    <MINU> 0\n\
                    <LOC?> TRUE\n\
                    <LOCX> 33.800000\n\
                    <LOCY> -42.200000\n\
                    <TIM?> FALSE\n\
                    <TIME> 0.000000\n\
                    \n";
    std::fs::write(&path, original).expect("the disposable settings directory is writable");
    playing_as(&mut c, JOURNAL_CHARACTER);

    let read_it = c.view().expect_app().hud().panels.journal.page_loads == 1;
    let parsed = {
        let p = &c.view().expect_app().hud().panels.journal.pages[0];
        p.title == "Written by another client"
            // The tab came back as a line break.
            && p.notes == "first\nsecond"
            && p.hours == 2
            && p.location_set
            && (p.ew - 33.8).abs() < 1e-6
            && (p.ns + 42.2).abs() < 1e-6
    };

    open_the_page(&mut c, journal::PAGE);
    click_the_tab(&mut c, journal::PAGE, journal::PANEL);
    empty_the_box(&mut c, journal::TITLE_EDIT);
    write_in(&mut c, journal::TITLE_EDIT, "Written by this one");
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);

    let written = std::fs::read_to_string(&path).expect("the client wrote the file back");
    let in_place = written != original
        && written.contains("<TITL> Written by this one")
        // The record only the reader knows about is not one the writer writes.
        && !written.contains("<PNUM>");
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.file.a-notebook-in-the-shipped-format-is-read-and-written-back-in-place",
        move |_| read_it && parsed && in_place && silent,
    );
    c.shutdown();
}

/// **A second character gets a notebook of their own**, and the first character's is not written
/// over -- which is the worst failure this feature can have.
pub(super) fn a_second_character_gets_its_own_notebook() {
    let mut c = a_client_with_a_notebook("journal-twochars");
    playing_as(&mut c, JOURNAL_CHARACTER);
    let first = notebook_path(&c, JOURNAL_CHARACTER);

    open_the_page(&mut c, journal::PAGE);
    click_the_tab(&mut c, journal::PAGE, journal::PANEL);
    write_in(&mut c, journal::TITLE_EDIT, "Kupotest was here");
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);
    let on_disk = std::fs::read_to_string(&first)
        .expect("the first character's page is on disk")
        .contains("Kupotest was here");

    // The next log-on, as somebody else.
    playing_as(&mut c, "Otherguy");
    let repointed = c
        .view()
        .expect_app()
        .hud()
        .panels
        .journal
        .file
        .as_deref()
        .and_then(std::path::Path::file_name)
        .and_then(std::ffi::OsStr::to_str)
        == Some("Journal-Frostfell-Otherguy.txt");
    let fresh = c.view().expect_app().hud().panels.journal.pages.len() == 1
        && c.view().expect_app().hud().panels.journal.pages[0]
            .title
            .is_empty();

    click_the_tab(&mut c, journal::PAGE, journal::PANEL);
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);
    let untouched = std::fs::read_to_string(&first)
        .expect("the first character's notebook is still there")
        .contains("Kupotest was here");
    let silent = nothing_was_sent(&c);

    c.assert_behaviour(
        "journal.file.a-second-character-gets-a-notebook-of-their-own",
        move |_| on_disk && repointed && fresh && untouched && silent,
    );
    c.shutdown();
}

/// A log-off and a log-on again, with the screen genuinely rebound in between.
fn relog_as(c: &mut HeadlessClient, character: &str) {
    c.app_mut().probe_mut().host_state_mut().entered_character = None;
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::CHARACTER_MANAGEMENT);
    c.tick(3);
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    c.tick(4);
    playing_as(c, character);
    c.tick(3);
}

/// **A relog keeps each character's notebook, and saves the page that was still open.**
///
/// The page one character typed is on disk and off it again when they come back; the other
/// character's notebook is a different file and a blank one, and neither writes over the other.
/// And the page the player was still looking at when they logged off is there afterwards --
/// which is the edit a relog would otherwise throw away.
pub(super) fn a_relog_keeps_each_characters_notebook() {
    let mut c = a_client_with_a_notebook("journal-relog");
    playing_as(&mut c, JOURNAL_CHARACTER);
    let first = notebook_path(&c, JOURNAL_CHARACTER);
    let second = notebook_path(&c, "Otherguy");
    let per_character = first != second;

    open_the_page(&mut c, journal::PAGE);
    click_the_tab(&mut c, journal::PAGE, journal::PANEL);
    write_in(&mut c, journal::TITLE_EDIT, "Gharundim Dig");
    write_in(&mut c, journal::LABEL_EDIT, "Dig");
    write_in(&mut c, journal::NOTES_EDIT, "Second cellar, north wall");
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);
    let on_disk = std::fs::read_to_string(&first).expect("the first notebook");
    let written = on_disk.contains("<TITL> Gharundim Dig") && !second.exists();

    // ...and as somebody else, a blank notebook that is not the first one.
    let loads = c.view().expect_app().hud().panels.journal.page_loads;
    relog_as(&mut c, "Otherguy");
    open_the_page(&mut c, journal::PAGE);
    click_the_tab(&mut c, journal::PAGE, journal::PANEL);
    let blank = el_text(&mut c, journal::TITLE_EDIT).is_empty()
        && el_text(&mut c, journal::NOTES_EDIT).is_empty()
        && std::fs::read_to_string(&first).expect("still there") == on_disk
        // Nothing was read, so the blank page is a new one and not a read of somebody's file.
        && c.view().expect_app().hud().panels.journal.page_loads == loads;

    // ...and back again: the page comes off disk.
    click_the_tab(&mut c, journal::PAGE, pagelist::PANEL);
    relog_as(&mut c, JOURNAL_CHARACTER);
    open_the_page(&mut c, journal::PAGE);
    click_the_tab(&mut c, journal::PAGE, journal::PANEL);
    let came_back = c.view().expect_app().hud().panels.journal.page_loads == loads + 1
        && el_text(&mut c, journal::TITLE_EDIT) == "Gharundim Dig"
        && el_text(&mut c, journal::LABEL_EDIT) == "Dig"
        && el_text(&mut c, journal::NOTES_EDIT) == "Second cellar, north wall";

    // The page that was **still open** when the player logged off. The tab is left up: nothing
    // has committed the typing, and the relog is the only thing that can.
    empty_the_box(&mut c, journal::TITLE_EDIT);
    write_in(&mut c, journal::TITLE_EDIT, "Unsaved");
    let uncommitted = !std::fs::read_to_string(&first)
        .expect("the notebook")
        .contains("Unsaved");
    relog_as(&mut c, JOURNAL_CHARACTER);
    open_the_page(&mut c, journal::PAGE);
    click_the_tab(&mut c, journal::PAGE, journal::PANEL);
    let saved_on_the_way_out = el_text(&mut c, journal::TITLE_EDIT) == "Unsaved"
        && std::fs::read_to_string(&first)
            .expect("the notebook")
            .contains("<TITL> Unsaved");

    c.assert_behaviour(
        "journal.file.a-relog-keeps-each-characters-notebook-and-saves-the-page-still-open",
        move |_| {
            per_character && written && blank && came_back && uncommitted && saved_on_the_way_out
        },
    );
    c.shutdown();
}
