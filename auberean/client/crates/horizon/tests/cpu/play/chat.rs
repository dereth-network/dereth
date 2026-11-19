//! The log window, Dereth's Horizon's own: its tabs and their menu, its windows popped out, and
//! its chat entry with the channel it keeps and the menu it is picked from.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::view::ChatOptions;
use auberean_client_sdk::contract::ChatWindowAction;
use auberean_client_sdk::Action;
use auberean_horizon::draw::Rect;
use auberean_horizon::own::Own;
use auberean_horizon::state::{GameState, PLUGIN_TEXT};
use auberean_horizon::ui::chat::{
    colour, entry_label, ground, kind, shows_in, tab_label, Chat, CHANNELS, DEFAULT_FILTERS, DOCK,
    ENTRY_GROUND, ENTRY_RULE, POP_OUT, SCROLL_THUMB,
};
use auberean_horizon::ui::options::CHAT_KINDS;
use auberean_horizon::ui::vk;

use crate::{centre, in_world, Harness};

/// The whole log window as it was drawn last frame, its tabs and the entry's room included.
fn window(h: &Harness) -> Rect {
    let k = h.k();
    let lines = h.ui.chat.dock;
    Rect::new(lines.x, lines.y - 24.0 * k, lines.w, lines.h + 24.0 * k)
}

/// The tab strip as drawn last frame.
fn strip(h: &Harness) -> Rect {
    let w = window(h);
    Rect::new(w.x, w.y, w.w, 24.0 * h.k())
}

/// Where tab `name` was drawn last frame.
fn tab(h: &Harness, name: &str) -> Rect {
    h.ui.chat
        .control(name)
        .unwrap_or_else(|| panic!("no tab {name:?}"))
}

/// The tabs' names in the log window as drawn last frame, left to right.
fn tab_names(h: &Harness) -> Vec<&'static str> {
    let mut drawn: Vec<(f32, &'static str)> =
        ["Main", "General", "Fellowship", "Allegiance", "Hunting"]
            .into_iter()
            .filter_map(|n| {
                let r = h.ui.chat.control(n)?;
                (r.y < h.ui.chat.dock.y && r.y >= strip(h).y - 0.5).then_some((r.x, n))
            })
            .collect();
    drawn.sort_by(|a, b| a.0.total_cmp(&b.0));
    drawn.into_iter().map(|(_, n)| n).collect()
}

/// A player whose chat options the game has told: a new character's.
fn told() -> GameState {
    let mut s = in_world();
    s.chat_options = Some(ChatOptions {
        filters: DEFAULT_FILTERS,
        ..ChatOptions::default()
    });
    s
}

#[test]
fn a_line_typed_in_the_entry_is_sent_with_enter_and_the_entry_closes() {
    let mut h = Harness::new();
    let s = in_world();
    h.frame(&s);
    assert!(!h.ui.typing());
    h.ui.chat.open("");
    h.type_text(&s, "hello there");
    assert!(h.ui.typing());
    let out = h.key(&s, vk::ENTER);
    assert_eq!(out.say.as_deref(), Some("hello there"));
    assert!(!h.ui.typing(), "closed");
    assert_eq!(h.ui.chat.text(), "");
}

#[test]
fn a_slash_command_is_sent_as_typed_for_the_command_line_to_carry_out() {
    let mut h = Harness::new();
    let s = in_world();
    h.ui.chat.open("/");
    h.type_text(&s, "where");
    let out = h.key(&s, vk::ENTER);
    assert_eq!(out.say.as_deref(), Some("/where"));
}

#[test]
fn the_slash_key_opens_the_entry_on_one_slash_though_it_types_one_too() {
    let mut h = Harness::new();
    let s = in_world();
    h.frame(&s);
    h.ui.chat.begin(true);
    h.type_text(&s, "/");
    assert_eq!(h.ui.chat.text(), "/");
    h.type_text(&s, "who");
    assert_eq!(h.ui.chat.text(), "/who");
}

#[test]
fn an_empty_line_sends_nothing_and_escape_keeps_what_is_typed_for_the_next_opening() {
    let mut h = Harness::new();
    let s = in_world();
    h.ui.chat.open("");
    h.type_text(&s, "   ");
    let out = h.key(&s, vk::ENTER);
    assert_eq!(out.say, None);
    h.ui.chat.begin(false);
    h.type_text(&s, "not yet");
    let out = h.key(&s, vk::ESCAPE);
    assert_eq!(out.say, None);
    assert!(!h.ui.typing());
    assert_eq!(h.ui.chat.text(), "", "closed");
    h.ui.chat.begin(false);
    h.frame(&s);
    assert_eq!(
        h.ui.chat.text(),
        "not yet",
        "the next opening begins with it"
    );
    h.key(&s, vk::ENTER);
    h.ui.chat.begin(false);
    h.frame(&s);
    assert_eq!(h.ui.chat.text(), "", "a line sent is not kept");
}

#[test]
fn up_and_down_ask_for_the_lines_sent_before_and_the_answer_fills_the_entry() {
    let mut h = Harness::new();
    let mut s = in_world();
    h.ui.chat.open("draft");
    h.frame(&s);
    assert_eq!(h.key(&s, vk::UP).recall, Some(true));
    assert_eq!(h.key(&s, vk::DOWN).recall, Some(false));
    s.recalled = vec![Some("/emotelist".into())];
    h.frame(&s);
    assert_eq!(h.ui.chat.text(), "/emotelist");
    s.recalled = vec![None];
    h.frame(&s);
    assert_eq!(
        h.ui.chat.text(),
        "/emotelist",
        "nothing more that way: as it was"
    );
}

#[test]
fn a_press_off_the_log_window_closes_the_entry() {
    let mut h = Harness::new();
    let s = in_world();
    h.ui.chat.open("");
    h.type_text(&s, "half");
    h.click(&s, (900.0, 300.0));
    assert!(!h.ui.typing());
}

#[test]
fn each_kind_of_text_has_its_color_and_errors_and_tells_differ_from_sayings() {
    let say = colour(0x10);
    let error = colour(0x1);
    let tell = colour(0x20);
    assert_ne!(error, say);
    assert_ne!(tell, say);
    assert_eq!(colour(0x10 | 0x1), error, "the first bit decides");
    assert_eq!(colour(0), 0xFFE8_E0D0, "a kind with none of the bits");
}

#[test]
fn closed_the_log_window_shows_no_entry_and_typing_opens_one_under_the_log() {
    let mut h = Harness::new();
    let s = in_world();
    let drawn = |h: &Harness, c: u32| {
        h.list
            .quads
            .iter()
            .any(|q| q.tex.is_none() && q.colour == c)
    };
    h.frame(&s);
    let w = window(&h);
    assert_eq!(h.ui.chat.entry_drawn(), None, "closed: no entry");
    assert!(
        !drawn(&h, ENTRY_GROUND) && !drawn(&h, ENTRY_RULE),
        "nor its ground or rule"
    );
    assert!(
        !h.ui.covers(w.x + 5.0, w.bottom() - 5.0),
        "the entry's room is the world's while it is closed"
    );
    assert!(h.ui.covers(w.x + 5.0, w.y + 40.0), "the log shows");
    h.ui.chat.open("");
    h.list = Default::default();
    h.frame(&s);
    assert!(h.ui.typing());
    let (entry, _) = h.ui.chat.entry_drawn().expect("typing: an entry");
    assert!(
        entry.bottom() <= w.bottom() + 0.5 && entry.y > w.y + w.h / 2.0,
        "{entry:?} at the foot of {w:?}"
    );
    assert!(
        h.list
            .quads
            .iter()
            .any(|q| q.tex.is_none() && q.dst == entry && q.colour == ENTRY_GROUND),
        "its ground"
    );
    assert!(
        h.list.quads.iter().any(|q| q.tex.is_none()
            && q.colour == ENTRY_RULE
            && (q.dst.y - entry.y).abs() < 0.5
            && (q.dst.w - entry.w).abs() < 0.5),
        "the thin rule along its top"
    );
    assert!(h.ui.covers(w.x + 5.0, entry.y + entry.h / 2.0));
}

/// The log window's ground as drawn last frame: its colour.
fn ground_drawn(h: &Harness) -> Option<u32> {
    let lines = h.ui.chat.dock;
    h.list
        .quads
        .iter()
        .find(|q| {
            q.tex.is_none() && (q.dst.x - lines.x).abs() < 0.5 && (q.dst.y - lines.y).abs() < 0.5
        })
        .map(|q| q.colour)
}

#[test]
fn the_log_window_is_one_translucent_pane_whose_opacity_is_horizon_s_own_setting() {
    let mut h = Harness::new();
    let s = in_world();
    h.input.mouse = (1500.0, 200.0);
    h.frame(&s);
    assert_eq!(ground_drawn(&h), Some(ground()), "at first");
    h.ui.settings.own.chat_opacity[0] = (0.9, 0.2);
    h.frame(&s);
    assert_eq!(
        ground_drawn(&h),
        Some(dereth_ui_kit::hud::log_ground(0.2)),
        "inactive"
    );
    h.input.mouse = centre(h.ui.chat.dock);
    h.frame(&s);
    assert_eq!(
        ground_drawn(&h),
        Some(dereth_ui_kit::hud::log_ground(0.9)),
        "active under the pointer"
    );
}

#[test]
fn the_corner_sizes_the_log_window_from_its_top_left_and_horizon_keeps_the_size() {
    let mut h = Harness::new();
    let s = in_world();
    h.frame(&s);
    let before = window(&h);
    let _ = h.ui.settings.take_own_changed();
    let grip = h.ui.chat.control("Resize 1").expect("its corner");
    let k = h.k();
    let from = centre(grip);
    h.drag(&s, from, (from.0 + 80.0 * k, from.1 - 40.0 * k));
    h.frame(&s);
    let after = window(&h);
    assert!(
        (after.x - before.x).abs() < 0.5 && (after.y - before.y).abs() < 0.5,
        "{before:?} to {after:?}: the top left stays"
    );
    assert!((after.w - (before.w + 80.0 * k)).abs() < 1.0, "wider");
    assert!(h.ui.settings.take_own_changed(), "kept");
    let (w, _) = h.ui.settings.own.chat_size.expect("its size");
    assert!((w * k - after.w).abs() < 1.0);
    assert_eq!(Own::read(&h.ui.settings.own.write()), h.ui.settings.own);
}

#[test]
fn a_game_line_ending_in_a_line_break_takes_one_row_of_the_log() {
    let glyphs = |lines: [&str; 2]| {
        let mut h = Harness::with_built_in_art();
        let mut s = in_world();
        for line in lines {
            s.say(0x10, line.to_owned());
        }
        h.frame(&s);
        let log = h.ui.chat.dock;
        let drawn: Vec<_> = h
            .list
            .quads
            .iter()
            .filter(|q| q.tex.is_some() && q.clip.is_some_and(|c| c.intersect(&log).is_some()))
            .map(|q| q.dst)
            .collect();
        drawn
    };
    let plain = glyphs(["You say, \"one\"", "You say, \"two\""]);
    assert!(!plain.is_empty(), "the lines drawn");
    assert_eq!(
        glyphs(["You say, \"one\"\n", "You say, \"two\"\n"]),
        plain,
        "each line a row, as though it had no break at its end"
    );
}

#[test]
fn the_entry_names_each_channel_it_speaks_on_and_the_command_a_line_begins() {
    for c in CHANNELS {
        assert_eq!(
            entry_label("", c.kind),
            (c.name, colour(c.kind)),
            "{}",
            c.name
        );
        assert_eq!(entry_label("hello", c.kind).0, c.name);
    }
    assert_eq!(entry_label("", kind::SAY).0, "Chat");
    assert_eq!(
        entry_label("/t Ayla, hi", kind::SAY),
        ("Tell", colour(kind::TELL))
    );
    assert_eq!(
        entry_label("/reply ", kind::GENERAL).0,
        "Tell",
        "a reply is a tell"
    );
    assert_eq!(entry_label("/em waves", kind::SAY).0, "Emote");
    assert_eq!(entry_label("/cg", kind::SAY).0, "General");
    assert_eq!(entry_label("/where", kind::GENERAL).0, "Command");
}

/// The text quads drawn in `colour` in `r` last frame.
fn text_in(h: &Harness, r: Rect, c: u32) -> usize {
    h.list
        .quads
        .iter()
        .filter(|q| q.tex.is_some() && q.colour == c && r.contains(q.dst.x, q.dst.y))
        .count()
}

#[test]
fn the_open_entry_shows_its_channel_before_the_text_and_the_closed_one_nothing() {
    let mut h = Harness::with_built_in_art();
    let s = in_a_fellowship();
    h.ui.chat.open("");
    h.frame(&s);
    h.frame(&s);
    let (entry, label) = h.ui.chat.entry_drawn().expect("open");
    assert!(text_in(&h, label, colour(kind::SAY)) > 0, "Chat, open");
    assert_eq!(text_in(&h, entry, colour(kind::FELLOWSHIP)), 0);
    h.ui.chat.close();
    h.frame(&s);
    assert_eq!(h.ui.chat.entry_drawn(), None, "closed, no name");
    assert_eq!(text_in(&h, entry, colour(kind::SAY)), 0);
    h.ui.chat.open("/f ");
    h.frame(&s);
    h.frame(&s);
    assert_eq!(h.ui.chat.mode(), kind::FELLOWSHIP);
    let (_, label) = h.ui.chat.entry_drawn().expect("open");
    assert!(
        text_in(&h, label, colour(kind::FELLOWSHIP)) > 0,
        "Fellowship, open"
    );
}

/// A player in a fellowship, whose `/f` the entry takes.
fn in_a_fellowship() -> GameState {
    let mut s = in_world();
    s.social.party = Some(Default::default());
    s
}

#[test]
fn a_channel_s_command_switches_the_entry_to_it_and_later_lines_are_said_there() {
    let mut h = Harness::new();
    let s = in_world();
    h.ui.chat.open("");
    h.type_text(&s, "/cg ");
    assert_eq!(h.ui.chat.mode(), kind::GENERAL, "switched");
    assert_eq!(h.ui.chat.text(), "", "and the command taken off the line");
    h.type_text(&s, "anyone selling ore?");
    let out = h.key(&s, vk::ENTER);
    assert_eq!(out.say.as_deref(), Some("/cg anyone selling ore?"));
    h.ui.chat.begin(false);
    h.type_text(&s, "still here");
    let out = h.key(&s, vk::ENTER);
    assert_eq!(
        out.say.as_deref(),
        Some("/cg still here"),
        "the channel is kept for the next line"
    );
}

#[test]
fn a_bare_command_sent_changes_the_channel_and_says_nothing() {
    let mut h = Harness::new();
    let s = in_a_fellowship();
    h.ui.chat.open("");
    h.type_text(&s, "/f");
    let out = h.key(&s, vk::ENTER);
    assert_eq!(out.say, None, "nothing said");
    assert_eq!(h.ui.chat.mode(), kind::FELLOWSHIP);
    h.ui.chat.begin(false);
    h.frame(&s);
    assert_eq!(h.ui.chat.text(), "", "and nothing kept typed");
    h.type_text(&s, "/s");
    let out = h.key(&s, vk::ENTER);
    assert_eq!(out.say, None);
    assert_eq!(h.ui.chat.mode(), kind::SAY, "back to sayings");
}

#[test]
fn a_channel_the_player_may_not_use_now_is_not_switched_to_and_its_line_goes_as_typed() {
    let mut h = Harness::new();
    let s = in_world();
    h.ui.chat.open("");
    h.type_text(&s, "/f hello");
    assert_eq!(h.ui.chat.mode(), kind::SAY, "in no fellowship");
    let out = h.key(&s, vk::ENTER);
    assert_eq!(
        out.say.as_deref(),
        Some("/f hello"),
        "for the command line to answer"
    );
}

#[test]
fn tab_steps_the_entry_to_the_next_channel_the_player_may_use() {
    let mut h = Harness::new();
    let mut s = in_world();
    h.ui.chat.open("");
    h.frame(&s);
    h.key(&s, vk::TAB);
    assert_eq!(
        h.ui.chat.mode(),
        kind::ALLEGIANCE,
        "past the fellowship and the kingdom, in neither"
    );
    s.social.party = Some(Default::default());
    s.social.kingdom = Some(1);
    h.key(&s, vk::TAB);
    assert_eq!(h.ui.chat.mode(), kind::KINGDOM);
    for _ in 0..3 {
        h.key(&s, vk::TAB);
    }
    assert_eq!(h.ui.chat.mode(), kind::REGION);
    h.key(&s, vk::TAB);
    assert_eq!(
        h.ui.chat.mode(),
        kind::SAY,
        "around again, past the player-killers'"
    );
    h.key(&s, vk::TAB);
    assert_eq!(h.ui.chat.mode(), kind::FELLOWSHIP, "in a fellowship now");
}

/// The entry open and the channel's name on it clicked: its menu open.
fn mode_menu(h: &mut Harness, s: &GameState) {
    h.ui.chat.begin(false);
    h.frame(s);
    let (_, label) = h.ui.chat.entry_drawn().expect("open");
    h.click(s, centre(label));
    assert!(h.ui.chat.focus_menu_open(), "the menu opens");
}

#[test]
fn a_click_on_the_channel_s_name_lists_the_game_s_channels_the_unusable_ones_dimmed() {
    let mut h = Harness::with_built_in_art();
    let s = in_world();
    mode_menu(&mut h, &s);
    let listed: Vec<u32> = CHANNELS
        .iter()
        .map(|c| c.kind)
        .filter(|k| h.ui.chat.focus_row(*k).is_some())
        .collect();
    assert_eq!(
        listed,
        [
            kind::SAY,
            kind::FELLOWSHIP,
            kind::ALLEGIANCE,
            kind::KINGDOM,
            kind::GENERAL,
            kind::TRADE,
            kind::REGION
        ],
        "every channel but the player-killers'"
    );
    // In order, top to bottom.
    let ys: Vec<f32> = listed
        .iter()
        .map(|k| h.ui.chat.focus_row(*k).unwrap().y)
        .collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "{ys:?}");
    let row = |h: &Harness, k| h.ui.chat.focus_row(k).unwrap();
    assert!(
        text_in(&h, row(&h, kind::FELLOWSHIP), 0xFF80_8080) > 0,
        "the fellowship dimmed, in none"
    );
    assert!(text_in(&h, row(&h, kind::GENERAL), colour(kind::GENERAL)) > 0);
    // A dimmed one does nothing; another is picked, closing the menu.
    let fellowship = row(&h, kind::FELLOWSHIP);
    h.click(&s, centre(fellowship));
    assert!(h.ui.chat.focus_menu_open(), "still open");
    assert_eq!(h.ui.chat.mode(), kind::SAY);
    let general = row(&h, kind::GENERAL);
    h.click(&s, centre(general));
    assert_eq!(h.ui.chat.mode(), kind::GENERAL);
    assert!(!h.ui.chat.focus_menu_open(), "closed");
    assert!(h.ui.typing(), "the entry stays open");
}

#[test]
fn the_mode_menu_lists_the_fellowship_bright_while_the_player_is_in_one() {
    let mut h = Harness::with_built_in_art();
    let s = in_a_fellowship();
    mode_menu(&mut h, &s);
    let row = h.ui.chat.focus_row(kind::FELLOWSHIP).expect("listed");
    assert!(text_in(&h, row, colour(kind::FELLOWSHIP)) > 0);
    h.click(&s, centre(row));
    assert_eq!(h.ui.chat.mode(), kind::FELLOWSHIP);
}

#[test]
fn the_tabs_are_main_general_fellowship_and_allegiance_until_the_player_names_them() {
    let mut h = Harness::new();
    let s = told();
    h.frame(&s);
    assert_eq!(
        tab_names(&h),
        ["Main", "General", "Fellowship", "Allegiance"]
    );
    assert_eq!(tab_label("  ", 1), "General");
    assert_eq!(tab_label(" Loot ", 1), "Loot");
}

#[test]
fn a_window_shows_what_its_filter_holds_and_the_first_the_plugins_lines() {
    assert!(shows_in(1, kind::GENERAL, kind::GENERAL));
    assert!(!shows_in(1, kind::GENERAL, kind::SAY));
    assert!(shows_in(0, 0, PLUGIN_TEXT), "the first takes the plugins'");
    assert!(!shows_in(1, 0, PLUGIN_TEXT));
}

/// The text quads drawn in the log's lines last frame.
fn glyphs_in_log(h: &Harness) -> usize {
    let log = h.ui.chat.dock;
    h.list
        .quads
        .iter()
        .filter(|q| q.tex.is_some() && log.contains(q.dst.x, q.dst.y + 1.0))
        .count()
}

#[test]
fn a_tab_clicked_shows_its_window_s_lines_and_the_entry_keeps_its_channel() {
    let mut h = Harness::with_built_in_art();
    let mut s = told();
    s.say(kind::SAY, "You say, \"Hi\"".into());
    h.frame(&s);
    let main = glyphs_in_log(&h);
    assert!(main > 0, "Main shows the saying");
    h.click(&s, centre(tab(&h, "General")));
    h.frame(&s);
    assert_eq!(h.ui.chat.shown_tab(), 1);
    assert_eq!(glyphs_in_log(&h), 0, "General holds none of it");
    assert_eq!(
        h.ui.chat.mode(),
        kind::SAY,
        "a tab has no channel of its own"
    );
}

#[test]
fn the_chat_windows_keys_show_their_tabs_and_again_the_first() {
    let mut h = Harness::new();
    let s = told();
    h.frame(&s);
    h.ui.keyed(auberean_horizon::keys::Keyed::ChatTab(2), &s);
    assert_eq!(h.ui.chat.shown_tab(), 2);
    h.ui.keyed(auberean_horizon::keys::Keyed::ChatTab(2), &s);
    assert_eq!(h.ui.chat.shown_tab(), 0, "again: the first");
}

#[test]
fn a_tab_not_shown_draws_the_same_when_a_line_comes_for_it() {
    let mut h = Harness::with_built_in_art();
    let mut s = told();
    h.frame(&s);
    let tabs = strip(&h);
    let drawn = |h: &Harness| -> Vec<_> {
        h.list
            .quads
            .iter()
            .filter(|q| q.dst.intersect(&tabs).is_some())
            .cloned()
            .collect()
    };
    let before = drawn(&h);
    s.say(kind::GENERAL, "[General] Ayla says, \"Hi\"".into());
    h.frame(&s);
    assert_eq!(drawn(&h), before, "no mark on General's tab");
}

/// The lines of `n` sayings.
fn sayings(n: usize) -> GameState {
    let mut s = in_world();
    for i in 0..n {
        s.say(kind::SAY, format!("You say, \"{i}\""));
    }
    s
}

/// The plain scrollbar's thumb drawn this frame.
fn thumb(h: &Harness) -> Option<Rect> {
    h.list
        .quads
        .iter()
        .find(|q| q.tex.is_none() && q.colour == SCROLL_THUMB)
        .map(|q| q.dst)
}

#[test]
fn a_log_holding_more_than_it_shows_has_a_scrollbar_whose_thumb_follows_the_scroll() {
    let mut h = Harness::new();
    h.frame(&sayings(3));
    assert_eq!(thumb(&h), None, "a few lines fit");
    let mut h = Harness::new();
    let s = sayings(200);
    h.frame(&s);
    let log = h.ui.chat.dock;
    let newest = thumb(&h).expect("a full log has one");
    assert!(log.contains(newest.x, newest.y), "{newest:?} in {log:?}");
    h.input.mouse = centre(log);
    h.input.wheel = 5.0;
    h.frame(&s);
    let back = thumb(&h).expect("its thumb");
    assert!(back.y < newest.y, "scrolled back, the thumb goes up");
}

/// Tab `name`'s menu opened by a right click on it, and drawn.
fn menu_of(h: &mut Harness, s: &GameState, name: &str) {
    h.frame(s);
    let at = tab(h, name);
    h.right_click(s, centre(at));
    assert!(h.ui.chat.filters_open(), "{name}'s menu");
}

/// Tab `name` popped out from its menu, and drawn.
fn pop_out(h: &mut Harness, s: &GameState, name: &str) {
    menu_of(h, s, name);
    let pop = h.ui.chat.control(POP_OUT).expect("its button");
    h.click(s, centre(pop));
    h.frame(s);
}

#[test]
fn a_right_click_on_a_tab_shows_it_and_opens_dereth_s_filters_menu_over_the_tabs() {
    let mut h = Harness::new();
    let s = told();
    menu_of(&mut h, &s, "Fellowship");
    assert_eq!(h.ui.chat.shown_tab(), 2, "shown");
    assert_eq!(h.ui.chat.mode(), kind::SAY, "the channel stays");
    let tabs = strip(&h);
    let menu = h.ui.chat.menu_rect().expect("drawn");
    assert!(menu.bottom() <= tabs.y, "over the tabs: {menu:?}");
    assert!((menu.x - tabs.x).abs() < 0.5, "from their left edge");
    for (_, name) in CHAT_KINDS {
        assert!(h.ui.chat.control(name).is_some(), "a box for {name}");
    }
    // Its boxes stand inside its thin frame by a small gap.
    let k = h.k();
    let first = h.ui.chat.control("Errors").expect("the first box");
    let edge = menu.x + (dereth_ui_kit::chat::MENU_RIM + 8.0) * k;
    assert!((first.x - edge).abs() < 0.01, "{first:?} from {edge}");
    assert!(h.ui.chat.control(POP_OUT).is_some());
    assert!(
        h.ui.covers(menu.x + 4.0, menu.y + 4.0),
        "it answers the pointer"
    );
}

#[test]
fn a_box_in_a_tab_s_menu_sets_its_window_s_filter_and_the_chat_options_are_kept() {
    let mut h = Harness::new();
    let s = told();
    menu_of(&mut h, &s, "General");
    let say = h.ui.chat.control("Say").expect("the sayings' box");
    let out = h.click(&s, centre(say));
    assert_eq!(
        out.acts,
        vec![
            Action::ChatWindows(ChatWindowAction::Filter {
                window: 2,
                filter: DEFAULT_FILTERS[1] | kind::SAY
            }),
            Action::ChatWindows(ChatWindowAction::Keep),
        ]
    );
    assert!(h.ui.chat.filters_open(), "a box leaves it open");
}

#[test]
fn a_tab_s_menu_changes_nothing_until_the_game_has_told_the_chat_options() {
    let mut h = Harness::new();
    let s = in_world();
    menu_of(&mut h, &s, "General");
    let say = h.ui.chat.control("Say").expect("the sayings' box");
    assert!(h.click(&s, centre(say)).acts.is_empty());
}

#[test]
fn a_tab_s_menu_closes_on_a_press_elsewhere_or_a_key_but_not_as_its_press_is_let_go() {
    let mut h = Harness::new();
    let s = told();
    menu_of(&mut h, &s, "General");
    h.frame(&s);
    assert!(
        h.ui.chat.filters_open(),
        "open after the right click let go"
    );
    h.click(&s, (900.0, 300.0));
    assert!(!h.ui.chat.filters_open(), "a press elsewhere");
    menu_of(&mut h, &s, "General");
    h.key(&s, vk::ESCAPE);
    assert!(!h.ui.chat.filters_open(), "Escape");
}

#[test]
fn pop_out_takes_a_tab_into_a_window_of_its_own_over_the_log_window() {
    let mut h = Harness::new();
    let s = told();
    menu_of(&mut h, &s, "Main");
    assert!(
        h.ui.chat.control(POP_OUT).is_none(),
        "the first tab stays in the log window"
    );
    h.click(&s, (900.0, 300.0));
    pop_out(&mut h, &s, "General");
    assert!(h.ui.chat.floating()[1]);
    assert!(!h.ui.chat.filters_open(), "the button closes the menu");
    assert_eq!(tab_names(&h), ["Main", "Fellowship", "Allegiance"]);
    let log = window(&h);
    let (x, y) = h.ui.chat.popped_at(1).expect("drawn");
    assert!(y < log.y, "over the log window");
    assert!((x - log.x).abs() < 0.5);
    let popped = h.ui.chat.popped_rects()[0];
    assert!(h.ui.covers(popped.x + 10.0, popped.y + popped.h / 2.0));
}

#[test]
fn a_tab_popped_out_moves_by_its_tab_docks_from_its_menu_and_horizon_keeps_where_it_was_put() {
    let mut h = Harness::new();
    let s = told();
    pop_out(&mut h, &s, "Fellowship");
    let _ = h.ui.settings.take_own_changed();
    let (x, y) = h.ui.chat.popped_at(2).expect("drawn");
    let from = (x + 10.0, y + 8.0);
    h.drag(&s, from, (from.0 + 300.0, from.1 - 100.0));
    h.frame(&s);
    let moved = h.ui.chat.popped_at(2).expect("drawn");
    assert!(
        (moved.0 - (x + 300.0)).abs() < 0.5 && (moved.1 - (y - 100.0)).abs() < 0.5,
        "{x},{y} to {moved:?}"
    );
    assert!(h.ui.settings.take_own_changed(), "kept");
    let k = h.k();
    let kept = h.ui.settings.own.chat_popped[2].expect("its place");
    assert!((kept.0 * k - moved.0).abs() < 0.5 && (kept.1 * k - moved.1).abs() < 0.5);
    assert_eq!(Own::read(&h.ui.settings.own.write()), h.ui.settings.own);
    // Docked from its own tab's menu, and popped out again: it stands where it was put.
    let own_tab = h.ui.chat.control("Fellowship").expect("its own tab");
    h.right_click(&s, centre(own_tab));
    let dock = h.ui.chat.control(DOCK).expect("its button");
    h.click(&s, centre(dock));
    h.frame(&s);
    assert!(!h.ui.chat.floating()[2]);
    assert_eq!(
        tab_names(&h),
        ["Main", "General", "Fellowship", "Allegiance"]
    );
    pop_out(&mut h, &s, "Fellowship");
    assert_eq!(h.ui.chat.popped_at(2), Some(moved));
}

#[test]
fn a_tab_popped_out_is_sized_by_its_corner_and_horizon_keeps_the_size() {
    let mut h = Harness::new();
    let s = told();
    pop_out(&mut h, &s, "Allegiance");
    let _ = h.ui.settings.take_own_changed();
    let before = h.ui.chat.popped_rects()[0];
    let grip = h.ui.chat.control("Resize 4").expect("its corner");
    let k = h.k();
    let from = centre(grip);
    h.drag(&s, from, (from.0 - 100.0 * k, from.1 + 40.0 * k));
    h.frame(&s);
    let after = h.ui.chat.popped_rects()[0];
    assert!(
        (after.x - before.x).abs() < 0.5 && (after.y - before.y).abs() < 0.5,
        "top left stays"
    );
    assert!(
        (after.w - (before.w - 100.0 * k)).abs() < 1.0
            && (after.h - (before.h + 40.0 * k)).abs() < 1.0,
        "{before:?} to {after:?}"
    );
    assert!(h.ui.settings.take_own_changed(), "kept");
    assert!(h.ui.settings.own.chat_popped_size[3].is_some());
}

#[test]
fn a_tab_the_player_named_shows_that_name_and_a_blank_one_its_own() {
    let mut h = Harness::new();
    let s = told();
    h.ui.settings.own.tab_names[1] = "Hunting".into();
    h.frame(&s);
    assert_eq!(
        tab_names(&h),
        ["Main", "Hunting", "Fellowship", "Allegiance"]
    );
    h.ui.settings.own.tab_names[1].clear();
    h.frame(&s);
    assert_eq!(
        tab_names(&h),
        ["Main", "General", "Fellowship", "Allegiance"]
    );
    let _ = Chat::default();
}

#[test]
fn the_mode_menu_names_each_channel_in_full_on_an_opaque_plate_its_rows_padded_apart() {
    let mut h = Harness::with_built_in_art();
    let s = in_a_fellowship();
    mode_menu(&mut h, &s);
    let menu = h.ui.chat.menu_rect().expect("drawn");
    assert!(
        h.list
            .quads
            .iter()
            .any(|q| q.tex.is_none() && q.dst == menu && q.colour >> 24 == 0xFF),
        "an opaque ground under the plate"
    );
    let k = h.k();
    // Inside the thin frame by its padding, the first row just there.
    let (pad, rim) = (dereth_ui_kit::chat::MODE_PAD, dereth_ui_kit::chat::MENU_RIM);
    assert_eq!(rim, 2.0, "a thin frame");
    let inside = Rect::new(
        menu.x + (rim + pad.0 / 2.0) * k,
        menu.y + (rim + pad.1) * k,
        menu.w - (2.0 * rim + pad.0) * k,
        menu.h - 2.0 * (rim + pad.1) * k,
    );
    let first = h.ui.chat.focus_row(kind::SAY).expect("listed");
    assert!(
        (first.x - inside.x).abs() < 0.01 && (first.y - inside.y).abs() < 0.01,
        "{first:?} stands at the frame's inset and its padding, {inside:?}"
    );
    let rows: Vec<Rect> = CHANNELS
        .iter()
        .filter_map(|c| h.ui.chat.focus_row(c.kind))
        .collect();
    for (i, r) in rows.iter().enumerate() {
        assert!(
            r.x >= inside.x - 0.01
                && r.y >= inside.y - 0.01
                && r.right() <= inside.right() + 0.01
                && r.bottom() <= inside.bottom() + 0.01,
            "{r:?} inside {inside:?}"
        );
        for other in &rows[i + 1..] {
            assert!(r.intersect(other).is_none(), "{r:?} over {other:?}");
        }
    }
    for c in CHANNELS.iter().filter(|c| c.kind != kind::PVP) {
        assert_eq!(h.ui.chat.focus_row_name(c.kind), Some(c.name), "whole");
    }
    assert_eq!(h.ui.chat.entry_label(), Some("Chat"), "the line's the same");
}
