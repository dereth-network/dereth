use super::*;
// ---------------------------------------------------------------------------------------------
// The chat tab of the options: which kinds of line each window shows
// ---------------------------------------------------------------------------------------------

/// The five chat windows the page edits, in the order it builds them.
const FILTER_WINDOWS: [u32; 5] = [8, 2, 3, 4, 5];
/// What a check box carries when it is ticked.
const ATTR_CHECKED: u32 = 0x0E;

/// The thirteen rows the page offers, in the order it adds them, as `(token, mask, caption)`.
///
/// **Five of them are the global channels**: they are rows of this same list and not a second
/// group, a second page or a set the shard sends.
const FILTER_ROWS: [(&str, u64, &str); 13] = [
    ("ID_ChatOption_TextFilter_Gameplay", 0x8391_2021, "Gameplay"),
    ("ID_ChatOption_TextFilter_Combat", 0x0060_0040, "Combat"),
    ("ID_ChatOption_TextFilter_Magic", 0x0002_0080, "Magic"),
    (
        "ID_ChatOption_TextFilter_AreaSpeech",
        0x0000_1004,
        "Area Chat",
    ),
    ("ID_ChatOption_TextFilter_Tells", 0x0000_0018, "Tells"),
    (
        "ID_ChatOption_TextFilter_Allegience",
        0x0004_0C00,
        "Allegiance",
    ),
    (
        "ID_ChatOption_TextFilter_Fellowship",
        0x0008_0000,
        "Fellowship",
    ),
    (
        "ID_ChatOption_TextFilter_General",
        0x0800_0000,
        "General Channel",
    ),
    (
        "ID_ChatOption_TextFilter_Trade",
        0x1000_0000,
        "Trade Channel",
    ),
    ("ID_ChatOption_TextFilter_LFG", 0x2000_0000, "LFG Channel"),
    (
        "ID_ChatOption_TextFilter_Roleplay",
        0x4000_0000,
        "Roleplay Channel",
    ),
    (
        "ID_ChatOption_TextFilter_Society",
        0x1_0000_0000,
        "Society Channel",
    ),
    ("ID_ChatOption_TextFilter_Error", 0x0400_0000, "Errors"),
];

/// A client with the chat tab of the options open, reached the way a player reaches it.
fn a_client_on_the_chat_options_tab(hand: &mut Hand) -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    // Without a description of the player there is nothing for an option to be written into.
    c.when(Inbound::event(SessionEvent::PlayerDescription(
        Box::default(),
    )));
    c.tick(3);
    open_the_options_page(
        &mut c,
        hand,
        dereth_ui_screens::options::chat::CHAT_PAGE_ELEMENT,
    );
    c
}

/// Every row of one window's filter, as `(token, mask, the caption drawn, the box's element)`.
type FilterRow = (&'static str, u64, Option<String>, Option<ElemHandle>);
fn filter_rows(c: &mut HeadlessClient, window: u32) -> Vec<FilterRow> {
    with_screen(c, |_, s| {
        let i = s
            .chat_options
            .filter_of(window)
            .expect("a filter for that window");
        match &s.chat_options.options[i] {
            dereth_ui_screens::options::chat::ChatOption::Filter(f) => f
                .children
                .iter()
                .map(|ch| (ch.label_token, ch.mask, ch.label.clone(), ch.element))
                .collect(),
            dereth_ui_screens::options::chat::ChatOption::Opacity(_) => {
                unreachable!("a filter is not an opacity")
            }
        }
    })
}

/// The control one window's rows live inside.
fn filter_control(c: &mut HeadlessClient, window: u32) -> ElemHandle {
    with_screen(c, |_, s| {
        let i = s
            .chat_options
            .filter_of(window)
            .expect("a filter for that window");
        match &s.chat_options.options[i] {
            dereth_ui_screens::options::chat::ChatOption::Filter(f) => f.element,
            dereth_ui_screens::options::chat::ChatOption::Opacity(_) => {
                unreachable!("a filter is not an opacity")
            }
        }
    })
}

/// What one chat window itself is showing, which is what a line is tested against.
fn window_filter(c: &mut HeadlessClient, window: u32) -> u64 {
    with_screen(c, |_, s| {
        s.chat
            .iter()
            .find(|w| w.window_id == window)
            .expect("the chat window")
            .filter
    })
}

/// Every run of letters the interface draws this frame.
fn drawn_runs(c: &mut HeadlessClient) -> Vec<String> {
    let mut back = dereth_ui::RecordingDrawBackend::default();
    c.app_mut()
        .ui_mut()
        .expect("the shell is up")
        .ui
        .draw(&mut back);
    back.calls
        .into_iter()
        .map(|call| {
            call.glyphs
                .iter()
                .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
                .collect::<String>()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// Every chat window offers the same rows, and every row is drawn inside the control that owns it.
pub fn every_window_offers_the_same_filter_rows() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_chat_options_tab(&mut hand);

    let mut total = 0_usize;
    let mut holds = true;
    for w in FILTER_WINDOWS {
        // The main window is given one row fewer: the everything-else row is not offered for it.
        let expected: Vec<&(&str, u64, &str)> = if w == 8 {
            FILTER_ROWS[1..].iter().collect()
        } else {
            FILTER_ROWS.iter().collect()
        };
        let rows = filter_rows(&mut c, w);
        assert_eq!(
            rows.len(),
            expected.len(),
            "window {w} offers a different number of rows"
        );
        let control = filter_control(&mut c, w);
        let control_box = c
            .view()
            .expect_app()
            .ui()
            .expect("the shell")
            .ui
            .screen_box(control);
        for (k, (token, mask, caption)) in expected.iter().enumerate() {
            let (got_token, got_mask, got_caption, element) = &rows[k];
            let h = element.unwrap_or_else(|| panic!("window {w} row {k} has no box"));
            let row_box = c
                .view()
                .expect_app()
                .ui()
                .expect("the shell")
                .ui
                .screen_box(h);
            // **The containment is the half that can break**: a control that was not made as tall
            // as its own rows leaves everything from the sixth down hanging outside it, where the
            // next section is drawn over them.
            let row_holds = got_token == token
                && got_mask == mask
                && got_caption.as_deref() == Some(*caption)
                && control_box.y0 <= row_box.y0
                && row_box.y1 <= control_box.y1;
            assert!(
                row_holds,
                "window {w} row {k} ({caption}) at {row_box:?} in {control_box:?}"
            );
            holds &= row_holds;
            total += 1;
        }
    }
    let all_of_them = total == 64;

    c.assert_behaviour(
        "chat.filters.every-window-offers-the-same-rows-and-each-row-lies-inside-its-control",
        move |_| holds && all_of_them,
    );
    c.shutdown();
}

/// The global channels are rows of that same list, and they are on the screen -- which is a
/// different fact from being in the list.
pub fn the_global_channels_are_rows_of_that_list_and_are_drawn() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_chat_options_tab(&mut hand);

    let at_rest = drawn_runs(&mut c);
    // The first two are on screen the moment the tab comes up (the chat font's two rows above
    // the list push the others down); the rest are the last boxes of the main window's list and
    // sit just below what the page shows at rest.
    let mut drawn = true;
    for caption in ["General Channel", "Trade Channel"] {
        assert!(
            at_rest.iter().any(|s| s == caption),
            "{caption:?} is not drawn: {at_rest:?}"
        );
        drawn &= at_rest.iter().any(|s| s == caption);
    }

    // And the scrollbar reaches the rest of the list and the sections below it.
    let list = with_screen(&mut c, |_, s| {
        s.chat_options
            .option_box
            .as_ref()
            .expect("the page's list")
            .handle
    });
    dereth_ui::widgets::listbox::set_scroll_offset(
        &mut c.app_mut().ui_mut().expect("the shell is up").ui,
        list,
        0,
        200,
    );
    c.tick(1);
    let scrolled = drawn_runs(&mut c);
    let mut reachable = true;
    for caption in [
        "LFG Channel",
        "Roleplay Channel",
        "Society Channel",
        "Errors",
        "Gameplay",
    ] {
        assert!(
            scrolled.iter().any(|s| s == caption),
            "{caption:?} is not reachable by scrolling: {scrolled:?}"
        );
        reachable &= scrolled.iter().any(|s| s == caption);
    }

    c.assert_behaviour(
        "chat.filters.the-global-channels-are-rows-of-that-list-and-are-drawn",
        move |_| drawn && reachable,
    );
    c.shutdown();
}

/// Ticking a row writes that window's own filter, and sends nothing: what a window shows is the
/// player's own setting and not something the shard is told about at once.
pub fn ticking_a_filter_row_writes_that_windows_own_filter() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_chat_options_tab(&mut hand);

    // The main window's rows are the thirteen less the first, so the general channel is the
    // seventh of them.
    let rows = filter_rows(&mut c, 8);
    let (token, mask, _, element) = rows[6].clone();
    let is_general = token == FILTER_ROWS[7].0 && mask == FILTER_ROWS[7].1;
    let boxh = element.expect("the general channel has a box");

    let before = window_filter(&mut c, 8);
    let on_to_start = before & mask == mask
        && dereth_ui_screens::bind::attr_bool(
            &c.view().expect_app().ui().expect("the shell").ui,
            boxh,
            ATTR_CHECKED,
        ) == Some(true);

    let mark_out = mark(&c);
    hand.click_handle(&mut c, boxh);
    let cleared = window_filter(&mut c, 8) & mask == 0
        && dereth_ui_screens::bind::attr_bool(
            &c.view().expect_app().ui().expect("the shell").ui,
            boxh,
            ATTR_CHECKED,
        ) == Some(false);

    hand.click_handle(&mut c, boxh);
    let restored = window_filter(&mut c, 8) == before;
    // Nothing went out: what a window shows is written locally and told to the shard later.
    let silent = c.view().outbound()[mark_out..].is_empty();

    c.assert_behaviour(
        "chat.filters.ticking-a-row-writes-that-windows-own-filter-and-sends-nothing",
        move |_| is_general && on_to_start && cleared && restored && silent,
    );
    c.shutdown();
}
