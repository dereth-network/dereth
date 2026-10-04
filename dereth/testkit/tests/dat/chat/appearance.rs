use super::*;
// ---------------------------------------------------------------------------------------------
// The chat window itself: its entry, its font, and how solid it looks
// ---------------------------------------------------------------------------------------------

/// The main chat window's background picture -- a child of the window that owns no drawing surface
/// of its own, and therefore the first thing a change to the window's own has to reach.
const CHAT_BACKGROUND: ElementId = ElementId(0x1000_0010);
/// The stored setting for how solid an idle chat window is drawn.
const IDLE_OPACITY: u32 = 0x1000_0080;
/// A scrollbar's position along its length, which a slider reads.
const ATTR_POSITION: u32 = 0x86;
/// The fifth of the five chat font sizes, and the two fonts the first and the last resolve to.
const SIZE_LARGEST: i32 = 4;
const FONT_SMALL: dereth_primitives::DataId = dereth_primitives::DataId(0x4000_0000);
const FONT_LARGEST: dereth_primitives::DataId = dereth_primitives::DataId(0x4000_0005);

/// A client in gameplay with the player described.
fn a_client_with_a_described_player(
    module: dereth_protocol::login::PlayerModule,
) -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.when(Inbound::event(SessionEvent::PlayerDescription(Box::new(
        dereth_protocol::login::LoginPlayerDescription {
            player_module: module,
            ..dereth_protocol::login::LoginPlayerDescription::default()
        },
    ))));
    c.tick(3);
    c
}

/// The caret the entry draws, the entry's own box, the box its letters go in, and how far the
/// letters have been slid along under it.
fn entry_caret(
    c: &mut HeadlessClient,
) -> (dereth_ui::Box2D, dereth_ui::Box2D, dereth_ui::Box2D, i32) {
    let entry = element(c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    let screen = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .screen_box(entry);
    with_screen(c, |ui, _| {
        let t = ui
            .text_element_mut(entry)
            .expect("the entry is a text element");
        (
            t.caret_box(screen)
                .expect("a box that can be typed in and has the caret draws one"),
            screen,
            t.content_box(screen),
            t.scroll.x,
        )
    })
}

/// Where the entry draws its `i`th letter, with the sliding already applied.
fn entry_glyph_x(c: &mut HeadlessClient, i: usize) -> i32 {
    let entry = element(c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    let screen = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .screen_box(entry);
    with_screen(c, |ui, _| {
        let t = ui.text_element_mut(entry).expect("the entry");
        t.compose(screen).get(i).expect("that letter is composed").x
    })
}

fn entry_text(c: &mut HeadlessClient) -> String {
    let entry = element(c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    with_screen(c, |ui, _| {
        ui.text_element_mut(entry)
            .map(|t| t.glyphs.inq_text(false))
            .unwrap_or_default()
    })
}

/// Typing more than fits slides the line along so the caret stays where the player can see it,
/// and the sliding stays put rather than being worked out again every frame.
pub fn typing_past_the_entrys_edge_keeps_the_caret_in_view() {
    let mut c = a_client_with_a_described_player(dereth_protocol::login::PlayerModule::default());
    let mut hand = Hand::new();
    let entry = element(&c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    hand.click_element(&mut c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    let focused = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .focus_element()
        == Some(entry);

    // An empty box is not slid along at all, and the caret is inside it.
    let (caret, _, content, slid) = entry_caret(&mut c);
    let at_rest = slid == 0 && caret.x0 >= content.x0 && caret.x1 <= content.x1;

    let typed = "the quick brown fox jumps over the lazy dog and keeps on running past the edge";
    hand.type_text(&mut c, typed);
    let every_key = entry_text(&mut c) == typed;

    let (caret, element_box, content, slid) = entry_caret(&mut c);
    // **Where the caret lands is arithmetic and not taste.** It is asked for two pixels past the
    // end of the line and the sliding is allowed two pixels less than that, so it comes to rest
    // one pixel into the right-hand margin -- inside the element, and drawn rather than trimmed.
    let in_view = slid > 0
        && caret.x0 >= element_box.x0
        && caret.x1 <= element_box.x1
        && caret.x1 - content.x1 == 1;
    // And the line really moved: its first letter is off the left edge now.
    let moved = entry_glyph_x(&mut c, 0) < content.x0;

    c.tick(2);
    let (caret, element_box, _, still) = entry_caret(&mut c);
    let stays = still == slid && caret.x0 >= element_box.x0 && caret.x1 <= element_box.x1;

    c.assert_behaviour(
        "chat.entry.typing-past-the-edge-slides-the-line-so-the-caret-stays-in-view",
        move |_| focused && at_rest && every_key && in_view && moved && stays,
    );
    c.shutdown();
}

/// The font the log is drawn in, and the heights of the letters already in it.
fn log_font_and_heights(
    c: &mut HeadlessClient,
) -> (Option<dereth_primitives::DataId>, i32, Vec<i32>) {
    let log = element(c, dereth_ui_screens::chat::window::LOG);
    with_screen(c, |ui, _| {
        let t = ui.text_element_mut(log).expect("the log");
        let mut hs: Vec<i32> = t.glyphs.glyphs.iter().map(|g| g.height).collect();
        hs.sort_unstable();
        hs.dedup();
        (t.fonts.first().copied(), t.metrics.height(), hs)
    })
}

/// Put a line in the log the way the shard's own broadcast puts one there.
fn say_in_the_log(c: &mut HeadlessClient, text: &str) {
    let m = dereth_ui_screens::chat::interface::ChatMessage {
        feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
        window: 0,
        ty: 0,
        prefix: None,
        body: text.to_owned(),
    };
    let took = with_screen(c, |ui, s| s.recv_display_final_string_info(ui, &m));
    assert!(!took.is_empty(), "the line reached at least one window");
    c.tick(1);
}

/// Picking a different chat font size re-measures what is already in the log and the next line
/// that arrives.
pub fn changing_the_chat_font_size_remeasures_the_backlog() {
    let mut c = a_client_with_a_described_player(dereth_protocol::login::PlayerModule::default());
    let mut hand = Hand::new();

    say_in_the_log(&mut c, "a line of chat to measure");
    let (font, before_box, before_heights) = log_font_and_heights(&mut c);
    // The shipped setting and the layout's own font agree, which is why only a **change** to it is
    // observable at all.
    let starts_small = !before_heights.is_empty()
        && font == Some(FONT_SMALL)
        && dereth_ui_screens::options::store::inq_value("UI.ChatFontSize")
            == Some(dereth_ui_screens::view::PrefValue::Int(1));

    // The chat font's size is chosen on the chat options page.
    open_the_options_page(
        &mut c,
        &mut hand,
        dereth_ui_screens::options::chat::CHAT_PAGE_ELEMENT,
    );
    // Bring the row on to the screen the way the scrollbar does, then press it.
    let menu = {
        let i = with_screen(&mut c, |_, s| {
            s.config_page
                .options
                .iter()
                .position(|o| o.preference == "UI.ChatFontSize")
                .expect("the options window has a chat font size row")
        });
        with_screen(&mut c, |ui, s| {
            let row = s.config_page.options[i].row;
            if let Some(list) = s.chat_options.option_box.as_mut() {
                if let Some(k) = list.items.iter().position(|&h| h == row) {
                    list.scroll_to_view(ui, k);
                }
            }
        });
        c.tick(1);
        with_screen(&mut c, |_, s| s.config_page.options[i].element)
    };
    let popup = dereth_ui::widgets::menu::popup_handle(
        &c.view().expect_app().ui().expect("the shell").ui,
        menu,
    )
    .expect("the row carries a popup");
    let closed_to_start = !c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .is_visible(popup);
    hand.click_handle(&mut c, menu);
    let opened = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .is_visible(popup);
    #[allow(clippy::cast_sign_loss)]
    let row = dereth_ui::widgets::menu::get_item(
        &c.view().expect_app().ui().expect("the shell").ui,
        menu,
        SIZE_LARGEST as usize,
    )
    .expect("the largest size is a row of the list");
    hand.click_handle(&mut c, row);
    let picked = dereth_ui_screens::options::store::inq_value("UI.ChatFontSize")
        == Some(dereth_ui_screens::view::PrefValue::Int(SIZE_LARGEST));

    c.tick(1);
    let (font, after_box, after_heights) = log_font_and_heights(&mut c);
    let refonted = font == Some(FONT_LARGEST)
        && after_box > before_box
        && after_heights.iter().max() > before_heights.iter().max();

    // And the next line that arrives is measured with the new font as well.
    say_in_the_log(&mut c, "a second line");
    let (_, _, later) = log_font_and_heights(&mut c);
    let next_line_too = later == after_heights;

    c.assert_behaviour(
        "chat.log.changing-the-font-size-remeasures-the-backlog-and-the-next-line",
        move |_| starts_small && closed_to_start && opened && picked && refonted && next_line_too,
    );
    c.shutdown();
}

/// How solid the surface the chat window composes into is drawn.
fn chat_window_opacity(c: &mut HeadlessClient) -> f32 {
    let root = with_screen(c, |_, s| s.chat_windows.first().and_then(|w| w.root))
        .expect("the main chat window is bound");
    c.view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .material_opacity(root)
}

/// How solid one element's own drawing is, off the draw list.
fn drawn_alpha(c: &mut HeadlessClient, id: ElementId) -> f32 {
    let h = element(c, id);
    let mut back = dereth_ui::RecordingDrawBackend::default();
    with_screen(c, |ui, _| ui.draw(&mut back));
    back.calls
        .iter()
        .find(|call| call.who == h)
        .unwrap_or_else(|| panic!("{id:?} drew nothing at all"))
        .alpha_blend_mod
}

/// Dragging the idle-opacity slider fades the chat window at once, and a stored value is worn from
/// the first frame after login rather than crept towards.
pub fn the_idle_opacity_fades_the_chat_window_at_once() {
    let mut c = a_client_with_a_described_player(dereth_protocol::login::PlayerModule::default());
    let mut hand = Hand::new();
    // No shipped chat window says how solid it should be, so they all come up solid.
    let solid_to_start = (chat_window_opacity(&mut c) - 1.0).abs() < 1e-6
        && (drawn_alpha(&mut c, CHAT_BACKGROUND) - 1.0).abs() < 1e-6;

    open_the_options_page(
        &mut c,
        &mut hand,
        dereth_ui_screens::options::chat::CHAT_PAGE_ELEMENT,
    );
    // Bring the slider on to the screen and press it near its left-hand end.
    let bar = {
        let i = with_screen(&mut c, |_, s| {
            s.chat_options
                .slider_of(IDLE_OPACITY)
                .expect("the page has the idle slider")
        });
        with_screen(&mut c, |ui, s| {
            let row = match &s.chat_options.options[i] {
                dereth_ui_screens::options::chat::ChatOption::Opacity(o) => o.row,
                dereth_ui_screens::options::chat::ChatOption::Filter(f) => f.element,
            };
            let list = s.chat_options.option_box.as_mut().expect("the page's list");
            if let Some(k) = list.items.iter().position(|&h| h == row) {
                list.scroll_to_view(ui, k);
            }
        });
        c.tick(1);
        with_screen(&mut c, |_, s| match &s.chat_options.options[i] {
            dereth_ui_screens::options::chat::ChatOption::Opacity(o) => o.element,
            dereth_ui_screens::options::chat::ChatOption::Filter(_) => {
                unreachable!("a slider is not a filter")
            }
        })
    };
    let (x, y) = {
        let ui = &c.view().expect_app().ui().expect("the shell").ui;
        let b = ui.screen_box(bar);
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let x = b.x0 + ((b.x1 - b.x0) as f32 * 0.1) as i32;
        let y = (b.y0 + b.y1) / 2;
        let hit = ui.hit_test_screen(x, y);
        assert!(
            hit.is_some_and(|h| h == bar || ui.parent(h) == Some(bar)),
            "the pointer must land on the bar or its thumb, got {hit:?}"
        );
        (x, y)
    };
    hand.press_at(&mut c, x, y);
    let pos = dereth_ui_screens::bind::attr_float(
        &c.view().expect_app().ui().expect("the shell").ui,
        bar,
        ATTR_POSITION,
    )
    .unwrap_or(-1.0);
    let lowered = (0.0..0.3).contains(&pos);

    // **At once, not twenty frames later**: the window is idle, so the new value is pushed
    // straight through rather than crept towards.
    let (stored, current) = with_screen(&mut c, |_, s| {
        let w = s.chat.first().expect("the main window");
        (w.default_opacity, w.current_opacity)
    });
    let straight_through = (stored - pos).abs() < 1e-5
        && (current - pos).abs() < 1e-5
        && (chat_window_opacity(&mut c) - pos).abs() < 1e-5;
    // The half a player sees: the window's background, which owns no surface of its own and so
    // carries the window's.
    let background = (drawn_alpha(&mut c, CHAT_BACKGROUND) - pos).abs() < 1e-5;
    // **And it stops where the client stops it.** The log owns a surface of its own, so it is not
    // faded with the window around it -- a fading that ran to every descendant regardless would
    // look prettier and would not be what the client does.
    let stops_at_the_log =
        {
            let log = element(&c, dereth_ui_screens::chat::window::LOG);
            let ui = &c.view().expect_app().ui().expect("the shell").ui;
            ui.node(log).is_some_and(|n| n.flags.should_own_object())
                && ui
                    .node(log)
                    .and_then(|n| n.merged_properties().get_enum(0xCD))
                    == Some(3)
        } && (drawn_alpha(&mut c, dereth_ui_screens::chat::window::LOG) - 1.0).abs() < 1e-6;
    c.shutdown();

    // A client whose player arrives already carrying a stored value wears it on the first frame,
    // rather than fading towards it.
    let mut c = a_client_with_a_described_player(dereth_protocol::login::PlayerModule {
        gameplay_options: Some(dereth_protocol::property::PackObjPropertyCollection {
            version: 2,
            properties: dereth_protocol::property::PropertyCollection {
                bucket_index: 4,
                entries: vec![(
                    IDLE_OPACITY,
                    dereth_protocol::property::BaseProperty {
                        name: IDLE_OPACITY,
                        value: Some(dereth_protocol::property::BasePropertyValue::Float(0.25)),
                    },
                )],
            },
        }),
        ..dereth_protocol::login::PlayerModule::default()
    });
    let at_login = (chat_window_opacity(&mut c) - 0.25).abs() < 1e-5
        && (drawn_alpha(&mut c, CHAT_BACKGROUND) - 0.25).abs() < 1e-5;

    c.assert_behaviour(
        "chat.window.how-solid-it-is-follows-the-slider-at-once-and-is-worn-from-login",
        move |_| {
            solid_to_start
                && lowered
                && straight_through
                && background
                && stops_at_the_log
                && at_login
        },
    );
    c.shutdown();
}
