//! The chat panel on the retail toolbar layouts: its four windows, what each shows, the entry and
//! its keys, the windows' buttons, the lock, the opacity slider, the line of messages; and the
//! options window's chat page: its sections, the player's chat options shown, Apply, Recall and
//! Default.

use auberean_text::StringInfo;
use auberean_ui::token::attr;
use auberean_ui::types::{button_id, key_id};
use auberean_ui::{Assets, Handle, Token, Ui, Value};
use auberean_ui_game::chat::{
    self, text_type as t, Ask, ChatSettings, ChatUi, History, TEXT_TYPES,
};
use auberean_ui_game::gameplay::{GamePlay, Notice, Record, Request, View};
use auberean_ui_game::options::chat::{label, ChatOptionsPage};
use auberean_ui_game::options::helper::{Control, Row};
use auberean_ui_game::options::{notice, OptionWindow};
use auberean_ui_game::toolbar::Toolbar;

use super::stage::{Lacking, LACKING};
use crate::common::records;

/// The play screen made and the player in the world.
fn play(ui: &mut Ui) -> GamePlay {
    let mut out = Vec::new();
    let mut p = GamePlay::new(ui, &mut out);
    p.login_finished(ui, &mut out);
    p
}

/// The first window's kinds in a new options profile.
const FIRST: u32 = t::ERROR
    | t::COMBAT
    | t::ADMIN
    | t::STANDARD
    | t::SAY
    | t::TELL
    | t::EMOTE
    | t::BROADCAST
    | t::REGION_CHANNEL
    | t::FACTION_CHANNEL;

/// The sound of the line of messages' rows.
const WARNING_SOUND: u32 = 0x2A00_02DF;

/// What a new options profile gives the chat (each kind its own color, the small font).
fn defaults(revision: u64) -> ChatSettings {
    let colors = [
        (t::ERROR, 0),
        (t::COMBAT, 1),
        (t::ADMIN, 2),
        (t::STANDARD, 3),
        (t::SAY, 4),
        (t::TELL, 5),
        (t::EMOTE, 6),
        (t::LOG, 4),
        (t::FELLOWSHIP, 7),
        (t::ALLEGIANCE, 8),
        (t::FACTION_CHANNEL, 4),
        (t::PK_CHANNEL, 4),
        (t::DEVOTED, 4),
        (t::CHAT_ENTRY, 4),
        (t::GENERAL_CHANNEL, 4),
        (t::TRADE_CHANNEL, 4),
        (t::REGION_CHANNEL, 4),
        (t::BROADCAST, 9),
    ];
    ChatSettings {
        revision,
        filters: [FIRST, t::GENERAL_CHANNEL, t::FELLOWSHIP, t::ALLEGIANCE],
        devoted: [t::SAY, t::GENERAL_CHANNEL, t::FELLOWSHIP, t::ALLEGIANCE],
        fonts: colors.iter().map(|(k, c)| (*k, 0, *c, false)).collect(),
        opacities: Vec::new(),
    }
}

/// The scripts' notice that the chat options changed.
fn reload_notice() -> Notice {
    Notice {
        native: chat::notice::RELOAD_CHAT_SETTINGS,
        ..Notice::default()
    }
}

/// The test's clock and what the game shows.
struct Frame {
    t: f64,
    settings: ChatSettings,
}

impl Frame {
    fn new() -> Self {
        Self {
            t: 0.0,
            settings: defaults(1),
        }
    }

    /// One frame: the clock, then the play screen's tick with these lines and notices.
    fn tick(
        &mut self,
        ui: &mut Ui,
        p: &mut GamePlay,
        lines: &[(u32, String)],
        notices: &[Notice],
    ) -> Vec<Request> {
        self.t += 0.05;
        ui.use_time(self.t);
        let mut out = Vec::new();
        let view = View {
            lines,
            chat: Some(&self.settings),
            ..View::default()
        };
        p.tick(ui, &view, notices, &mut out);
        out
    }

    /// One frame with lines that came as string references (`None` for a written line).
    fn tick_infos(
        &mut self,
        ui: &mut Ui,
        p: &mut GamePlay,
        lines: &[(u32, String)],
        infos: &[Option<StringInfo>],
    ) {
        self.t += 0.05;
        ui.use_time(self.t);
        let mut out = Vec::new();
        let view = View {
            lines,
            line_infos: infos,
            chat: Some(&self.settings),
            ..View::default()
        };
        p.tick(ui, &view, &[], &mut out);
    }

    /// The scripts said the chat options changed, and the client read them.
    fn reload(&mut self, ui: &mut Ui, p: &mut GamePlay) {
        self.tick(ui, p, &[], &[reload_notice()]);
        self.settings.revision += 1;
        self.tick(ui, p, &[], &[]);
    }

    fn key(&mut self, ui: &mut Ui, p: &mut GamePlay, k: u32) -> Vec<Request> {
        ui.key(k, true);
        let mut out = self.tick(ui, p, &[], &[]);
        ui.key(k, false);
        out.extend(self.tick(ui, p, &[], &[]));
        out
    }

    fn click(&mut self, ui: &mut Ui, p: &mut GamePlay, h: Handle) -> Vec<Request> {
        let r = ui.screen_rect(h).expect("placed");
        ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
        let mut out = self.tick(ui, p, &[], &[]);
        for down in [true, false] {
            ui.pointer_button(button_id::LEFT, down);
            out.extend(self.tick(ui, p, &[], &[]));
        }
        out
    }
}

fn chat(p: &GamePlay) -> &ChatUi {
    p.child::<Toolbar>()
        .and_then(Toolbar::child::<ChatUi>)
        .expect("the chat panel")
}

fn entry(p: &GamePlay, w: usize) -> Handle {
    chat(p).windows()[w].entry().expect("the entry")
}

fn log(p: &GamePlay, w: usize) -> Handle {
    chat(p).windows()[w].log().expect("the log")
}

fn text(ui: &Ui, h: Handle) -> String {
    ui.text(h).unwrap_or_default()
}

/// The color the text element offers as its `n`th (its own for 0).
fn offered(ui: &Ui, h: Handle, n: u32) -> [f32; 3] {
    let base = if n == 0 {
        "font_color".to_owned()
    } else {
        format!("font_color{n}")
    };
    let get = |c: &str| {
        ui.node(h)
            .and_then(|e| e.float_attr(Token::of(&format!("{base}_{c}"))))
            .unwrap_or(1.0)
    };
    [get("red"), get("green"), get("blue")]
}

/// The color of the glyph that starts `needle` in the text element.
fn color_at(ui: &Ui, h: Handle, needle: &str) -> [f32; 3] {
    let all = text(ui, h);
    let at = all.find(needle).expect("the text is there");
    let units = all[..at].encode_utf16().count();
    let g = &ui.text_state(h).expect("a text element").glyphs.glyphs[units];
    [g.color.r, g.color.g, g.color.b]
}

/// The chat panel makes four windows from the toolbar's layouts: the first shows, the others
/// hide until their buttons on the first show them. Once the chat options are read (the player's
/// options arriving read them), the first window's entry holds the prompt and each devoted window's entry its channel's name; each window
/// shows the kinds of text its filter holds (the first the errors, sayings, tells and the rest of
/// its kinds; the second the general channel), each in the color the options give its kind, the
/// lines added whole, newest at the foot. An error line also shows in the line of messages at the
/// screen's top, and goes after the six seconds its row says.
///
/// Behaviour: ui.chat.each-window-shows-the-kinds-its-filter-holds-in-their-colors
#[test]
fn each_window_shows_the_kinds_its_filter_holds_in_their_colors() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut f = Frame::new();
    f.tick(&mut ui, &mut p, &[], &[]);
    let windows = chat(&p).windows();
    assert_eq!(windows.len(), 4, "four windows");
    let shown: Vec<bool> = windows.iter().map(|w| w.shown(&ui)).collect();
    assert_eq!(shown, [true, false, false, false], "only the first shows");
    assert!(ui.shown(windows[0].root().unwrap()));
    assert!(!ui.shown(windows[1].root().unwrap()));
    assert_eq!(text(&ui, entry(&p, 0)), "", "no prompt before the options");

    // The player's options arriving read the chat's again too.
    f.tick(
        &mut ui,
        &mut p,
        &[],
        &[Notice {
            native: auberean_ui_game::gameplay::notice::RELOAD_OPTIONS,
            ..Notice::default()
        }],
    );
    f.settings.revision += 1;
    f.tick(&mut ui, &mut p, &[], &[]);
    assert_eq!(
        text(&ui, entry(&p, 0)),
        "Click and type here to talk to others."
    );
    assert_eq!(text(&ui, entry(&p, 1)), "<General Channel Messages>");
    assert_eq!(text(&ui, entry(&p, 2)), "<Fellowship Messages>");
    let filters: Vec<u32> = chat(&p).windows().iter().map(|w| w.filter()).collect();
    assert_eq!(
        filters,
        [
            FIRST | t::CHAT_ENTRY,
            t::GENERAL_CHANNEL | t::CHAT_ENTRY,
            t::FELLOWSHIP | t::CHAT_ENTRY,
            t::ALLEGIANCE | t::CHAT_ENTRY
        ],
        "each window's filter, the entry's kind added"
    );

    let lines = vec![
        (t::SAY, "Ayla says, \"Well met.\"\n".to_owned()),
        (
            t::GENERAL_CHANNEL,
            "[General] Bram says, \"LFG\"\n".to_owned(),
        ),
        (t::ERROR, "You cannot do that now.\n".to_owned()),
        (t::TELL, "Cora tells you, \"Over here.\"\n".to_owned()),
    ];
    f.tick(&mut ui, &mut p, &lines, &[]);
    f.tick(&mut ui, &mut p, &[], &[]);
    assert_eq!(
        chat(&p).windows()[0].lines(&ui),
        [
            "Ayla says, \"Well met.\"",
            "You cannot do that now.",
            "Cora tells you, \"Over here.\"",
            ""
        ],
        "the first window: the sayings, the errors and the tells, each line whole"
    );
    assert_eq!(
        chat(&p).windows()[1].lines(&ui),
        ["[General] Bram says, \"LFG\"", ""],
        "the second: the general channel"
    );
    let l = log(&p, 0);
    assert_eq!(
        color_at(&ui, l, "Ayla"),
        offered(&ui, l, 4),
        "sayings in color 4"
    );
    assert_eq!(
        color_at(&ui, l, "You cannot"),
        offered(&ui, l, 0),
        "errors in its own"
    );
    assert_eq!(
        color_at(&ui, l, "Cora"),
        offered(&ui, l, 5),
        "tells in color 5"
    );
    assert_ne!(offered(&ui, l, 4), offered(&ui, l, 5));

    assert_eq!(
        chat(&p).messages(&ui),
        ["You cannot do that now."],
        "the error in the line of messages"
    );
    let spew = ui
        .element(Token::of("SpewBox"))
        .expect("the line of messages");
    let row = ui.list_items(spew)[0];
    assert_eq!(
        ui.node(row).unwrap().float_attr(Token::of("decay_time")),
        Some(6.0),
        "the row says how long it stays"
    );
    for _ in 0..110 {
        f.tick(&mut ui, &mut p, &[], &[]);
    }
    assert_eq!(
        chat(&p).messages(&ui).len(),
        1,
        "there after five and a half seconds"
    );
    for _ in 0..20 {
        f.tick(&mut ui, &mut p, &[], &[]);
    }
    assert!(chat(&p).messages(&ui).is_empty(), "gone after six");
}

/// As a character enters, its scripts set its options and write the lines about what it could not
/// reach in one go: the options' notice and the lines come together. The windows take those lines
/// by the options the scripts left, their first: the first window shows the errors and the
/// standard lines, whether the client read the options before the frame or reads them only at the
/// next; the errors stand in the line of messages at once.
///
/// Behaviour: ui.chat.the-lines-written-with-the-options-show-by-those-options
#[test]
fn the_lines_written_with_the_options_show_by_those_options() {
    let entering = vec![
        (
            t::ERROR,
            "Cannot connect to GUN server. It may be down, or you did not connect through the \
             launcher.\n"
                .to_owned(),
        ),
        (
            t::ERROR,
            "Your character could not be logged into the authorization server.\n".to_owned(),
        ),
        (t::STANDARD, "Trade channel entered.\n".to_owned()),
    ];
    let options = Notice {
        native: auberean_ui_game::gameplay::notice::RELOAD_OPTIONS,
        ..Notice::default()
    };
    let want = [
        "Cannot connect to GUN server. It may be down, or you did not connect through the \
         launcher.",
        "Your character could not be logged into the authorization server.",
        "Trade channel entered.",
        "",
    ];

    // The client read the options as the scripts left them before the frame.
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut f = Frame::new();
    f.tick(&mut ui, &mut p, &entering, std::slice::from_ref(&options));
    f.tick(&mut ui, &mut p, &[], &[]);
    assert_eq!(chat(&p).windows()[0].lines(&ui), want, "read before");
    assert_eq!(
        chat(&p).messages(&ui),
        &want[..2],
        "the errors in the line of messages"
    );

    // The client reads them only at the next frame: the lines wait for it.
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut f = Frame::new();
    f.settings.revision = 0;
    f.tick(&mut ui, &mut p, &[], &[]);
    f.tick(&mut ui, &mut p, &entering, std::slice::from_ref(&options));
    assert!(
        chat(&p).windows()[0].lines(&ui).concat().is_empty(),
        "nothing before the options are read"
    );
    f.settings.revision = 1;
    f.tick(&mut ui, &mut p, &[], &[]);
    f.tick(&mut ui, &mut p, &[], &[]);
    assert_eq!(chat(&p).windows()[0].lines(&ui), want, "read after");
    assert_eq!(
        chat(&p).windows()[1].lines(&ui).concat(),
        "",
        "the second window shows the general channel alone"
    );
}

/// A line the scripts write before they set the options is taken by what the windows show then:
/// as a new character enters, the place's status comes right after the character's description,
/// the windows show nothing yet, and it never appears; the lines after the options do.
///
/// Behaviour: ui.chat.a-line-before-the-first-options-is-not-shown
#[test]
fn a_line_before_the_first_options_is_not_shown() {
    let entering = vec![
        (
            t::STANDARD,
            "You just entered a peace area (no player-vs.-player fighting allowed here).
"
            .to_owned(),
        ),
        (
            t::STANDARD,
            "Trade channel entered.
"
            .to_owned(),
        ),
    ];
    let options = Notice {
        native: auberean_ui_game::gameplay::notice::RELOAD_OPTIONS,
        ..Notice::default()
    };
    let settings = defaults(1);
    let enter = |ui: &mut Ui, p: &mut GamePlay, lines: &[(u32, String)], before: usize| {
        ui.use_time(0.05);
        let mut out = Vec::new();
        let view = View {
            lines,
            lines_before_options: before,
            chat: Some(&settings),
            ..View::default()
        };
        p.tick(ui, &view, std::slice::from_ref(&options), &mut out);
        ui.use_time(0.1);
        let view = View {
            chat: Some(&settings),
            ..View::default()
        };
        p.tick(ui, &view, &[], &mut out);
    };

    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    enter(&mut ui, &mut p, &entering, 1);
    assert_eq!(
        chat(&p).windows()[0].lines(&ui),
        ["Trade channel entered.", ""],
        "only the line after the options"
    );

    // Written after the options, the same line shows.
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    enter(&mut ui, &mut p, &entering, 0);
    assert_eq!(
        chat(&p).windows()[0].lines(&ui),
        [
            "You just entered a peace area (no player-vs.-player fighting allowed here).",
            "Trade channel entered.",
            "",
        ],
        "both after the options"
    );
}

/// An error line stands in the line of messages with its warning sound, once per line: two error
/// lines as a character enters play it twice.
///
/// Behaviour: ui.chat.each-error-line-plays-the-warning-sound
#[test]
fn each_error_line_plays_the_warning_sound() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut f = Frame::new();
    f.tick(&mut ui, &mut p, &[], &[]);
    ui.take_sounds();
    let lines = vec![
        (
            t::ERROR,
            "Cannot connect to GUN server.
"
            .to_owned(),
        ),
        (
            t::ERROR,
            "Could not be logged in.
"
            .to_owned(),
        ),
        (
            t::STANDARD,
            "Trade channel entered.
"
            .to_owned(),
        ),
    ];
    f.tick(&mut ui, &mut p, &lines, &[]);
    f.tick(&mut ui, &mut p, &[], &[]);
    let warnings = ui
        .take_sounds()
        .iter()
        .filter(|s| s.sound == WARNING_SOUND)
        .count();
    assert_eq!(warnings, 2);
}

/// A line that came as a string reference whose table the data lacks waits in the log until the
/// table has come, then shows as written from it; the lines after it wait behind it.
///
/// Behaviour: ui.chat.a-line-waits-in-the-log-for-its-string-tables
#[test]
fn a_line_waits_in_the_log_for_its_string_tables_and_the_lines_after_it_wait_behind_it() {
    let lacking = Lacking::new();
    let mut ui = Ui::new(lacking.clone(), 1024, 768);
    let mut p = play(&mut ui);
    let mut f = Frame::new();
    f.tick(&mut ui, &mut p, &[], &[reload_notice()]);
    f.tick(&mut ui, &mut p, &[], &[]);
    let lines = [
        (t::SAY, "<string table error>\n".to_owned()),
        (t::SAY, "After it.\n".to_owned()),
    ];
    let infos = [Some(StringInfo::entry(LACKING, 7)), None];
    f.tick_infos(&mut ui, &mut p, &lines, &infos);
    f.tick(&mut ui, &mut p, &[], &[]);
    assert_eq!(text(&ui, log(&p, 0)), "", "both wait for the table");
    lacking.arrive();
    f.tick(&mut ui, &mut p, &[], &[]);
    assert_eq!(text(&ui, log(&p, 0)), "From the server\nAfter it.\n");
}

/// A tell the player's scripts ask for (a fellow's or a friend's Tell) puts "/tell Name, " in the
/// entry of the window keys are for, selected, and starts typing there.
///
/// Behaviour: ui.chat.a-tell-asked-for-goes-in-the-entry
#[test]
fn a_tell_asked_for_goes_in_the_entry() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut f = Frame::new();
    f.reload(&mut ui, &mut p);
    let e = entry(&p, 0);
    assert_eq!(ui.active(), None);
    let mut who = StringInfo::default();
    who.set_literal(&"Cora".encode_utf16().collect::<Vec<u16>>(), false);
    f.tick(
        &mut ui,
        &mut p,
        &[],
        &[Notice {
            native: chat::notice::DO_ASSISTED_TELL,
            records: vec![Some(Record::Text(who))],
            ..Notice::default()
        }],
    );
    assert_eq!(text(&ui, e), "/tell Cora, ");
    assert_eq!(ui.active(), Some(e), "typing starts there");
}

/// Enter starts typing in the first window (its prompt goes); the keys typed go into the entry;
/// Enter hands the line to the player's command line and stops typing, the entry empty. `/`
/// starts a command and `;` an alias, the entry holding the mark. Escape stops typing at the next
/// tick, what was typed kept and selected when typing starts again. The arrows ask for the lines
/// typed before (with Ctrl the tells sent, with Shift the tells answered) only while typing, and
/// the line given back fills the entry.
///
/// Behaviour: ui.chat.the-entry-takes-a-line-and-hands-it-to-the-command-line
#[test]
fn the_entry_takes_a_line_and_hands_it_to_the_command_line() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut f = Frame::new();
    f.reload(&mut ui, &mut p);
    let e = entry(&p, 0);
    assert_eq!(ui.active(), None);

    f.key(&mut ui, &mut p, key_id::USE);
    assert_eq!(ui.active(), Some(e), "Enter starts typing");
    f.tick(&mut ui, &mut p, &[], &[]);
    assert_eq!(text(&ui, e), "", "the prompt goes");
    ui.type_text("Well met.");
    let out = f.key(&mut ui, &mut p, key_id::USE);
    assert!(
        out.contains(&Request::Chat(Ask::Text("Well met.".into()))),
        "the line to the command line: {out:?}"
    );
    assert_eq!(ui.active(), None, "typing stops");
    f.tick(&mut ui, &mut p, &[], &[]);
    assert_eq!(text(&ui, e), "", "the entry empty, no prompt again");

    f.key(&mut ui, &mut p, chat::key::START_COMMAND);
    assert_eq!(ui.active(), Some(e), "/ starts typing");
    assert_eq!(text(&ui, e), "/", "with the command's mark");
    ui.type_text("e wave");
    assert_eq!(text(&ui, e), "/e wave");
    ui.key(key_id::ESCAPE, true);
    f.tick(&mut ui, &mut p, &[], &[]);
    ui.key(key_id::ESCAPE, false);
    f.tick(&mut ui, &mut p, &[], &[]);
    assert_eq!(ui.active(), None, "Escape stops typing");
    assert_eq!(text(&ui, e), "/e wave", "what was typed stays");

    f.key(&mut ui, &mut p, chat::key::START_ALIAS);
    assert_eq!(text(&ui, e), ";", "; starts an alias");
    f.key(&mut ui, &mut p, key_id::USE);

    let out = f.key(&mut ui, &mut p, key_id::UP);
    assert!(
        !out.iter()
            .any(|r| matches!(r, Request::Chat(Ask::History { .. }))),
        "nothing asked while not typing"
    );
    f.key(&mut ui, &mut p, key_id::USE);
    assert_eq!(ui.active(), Some(e));
    let out = f.key(&mut ui, &mut p, key_id::UP);
    assert!(out.contains(&Request::Chat(Ask::History {
        list: History::Commands,
        earlier: true,
        window: 0
    })));
    ui.set_modifiers(false, true);
    let out = f.key(&mut ui, &mut p, key_id::DOWN);
    assert!(out.contains(&Request::Chat(Ask::History {
        list: History::Retells,
        earlier: false,
        window: 0
    })));
    ui.set_modifiers(true, false);
    let out = f.key(&mut ui, &mut p, key_id::UP);
    assert!(out.contains(&Request::Chat(Ask::History {
        list: History::Replies,
        earlier: true,
        window: 0
    })));
    ui.set_modifiers(false, false);
    ui.set_alt(true);
    for (shift, ctrl) in [(false, false), (false, true), (true, false)] {
        ui.set_modifiers(shift, ctrl);
        let out = f.key(&mut ui, &mut p, key_id::UP);
        assert!(
            !out.iter()
                .any(|r| matches!(r, Request::Chat(Ask::History { .. }))),
            "Alt held asks for no list, with Shift, Ctrl or neither"
        );
    }
    ui.set_alt(false);
    ui.set_modifiers(false, false);
    let c = p
        .child_mut::<Toolbar>()
        .and_then(Toolbar::child_mut::<ChatUi>)
        .unwrap();
    c.history_item(&mut ui, 0, History::Commands, "Well met.");
    assert_eq!(
        text(&ui, e),
        "Well met.",
        "the line given back fills the entry"
    );
    c.history_item(&mut ui, 0, History::Replies, "Cora");
    assert_eq!(
        text(&ui, e),
        "/tell Cora, ",
        "a name given back is a tell to them"
    );
}

/// The buttons on the first window show and hide the other windows, as do their keys; a line of
/// a kind a hidden window shows changes its button's look (back when it shows and hides again),
/// and a kind marked to pop up shows the window. A devoted window's entry says what is typed on
/// its channel: "hi" typed in the general channel's window goes as the channel's command.
///
/// Behaviour: ui.chat.the-buttons-and-keys-show-the-other-windows-and-a-devoted-window-speaks-on-its-channel
#[test]
fn the_buttons_and_keys_show_the_other_windows_and_a_devoted_window_speaks_on_its_channel() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut f = Frame::new();
    f.reload(&mut ui, &mut p);
    let toggle2 = ui
        .element(Token::of("ChatButtonToggle2"))
        .expect("a button");
    let shown = |p: &GamePlay, ui: &Ui, w: usize| chat(p).windows()[w].shown(ui);

    let general = vec![(
        t::GENERAL_CHANNEL,
        "[General] Bram says, \"LFG\"\n".to_owned(),
    )];
    f.tick(&mut ui, &mut p, &general, &[]);
    f.tick(&mut ui, &mut p, &[], &[]);
    assert_eq!(
        ui.node(toggle2).unwrap().state,
        Token::of("changed"),
        "a line for the hidden window changes its button"
    );
    f.click(&mut ui, &mut p, toggle2);
    assert!(shown(&p, &ui, 1), "its button shows the second window");
    f.click(&mut ui, &mut p, toggle2);
    assert!(!shown(&p, &ui, 1), "and hides it");
    assert_eq!(ui.node(toggle2).unwrap().state, Token::of("unchanged"));
    f.key(&mut ui, &mut p, chat::key::CHAT_WINDOW[2]);
    assert!(shown(&p, &ui, 2), "the third window's key shows it");
    f.key(&mut ui, &mut p, chat::key::CHAT_WINDOW[2]);
    assert!(!shown(&p, &ui, 2));

    f.settings
        .fonts
        .iter_mut()
        .filter(|k| k.0 == t::FELLOWSHIP)
        .for_each(|k| k.3 = true);
    f.reload(&mut ui, &mut p);
    let fellows = vec![(
        t::FELLOWSHIP,
        "[Fellowship] Ayla says, \"Inn?\"\n".to_owned(),
    )];
    f.tick(&mut ui, &mut p, &fellows, &[]);
    f.tick(&mut ui, &mut p, &[], &[]);
    assert!(shown(&p, &ui, 2), "a kind that pops up shows its window");

    f.click(&mut ui, &mut p, toggle2);
    let e = entry(&p, 1);
    f.click(&mut ui, &mut p, e);
    assert_eq!(
        ui.active(),
        Some(e),
        "a press on the entry starts typing there"
    );
    assert_eq!(text(&ui, e), "", "its label goes");
    ui.type_text("hi");
    let out = f.key(&mut ui, &mut p, key_id::USE);
    assert!(
        out.contains(&Request::Chat(Ask::Text("/cg hi".into()))),
        "said on the general channel: {out:?}"
    );
    f.tick(&mut ui, &mut p, &[], &[]);
    assert_eq!(
        text(&ui, e),
        "<General Channel Messages>",
        "the channel's name back in the empty entry"
    );
}

/// The lock button keeps the log where it is as lines come (the log locked, the button looking
/// locked) until pressed again; the options button shows the opacity slider, and moving it fades
/// the window, kept with the player's options when let go.
///
/// Behaviour: ui.chat.the-lock-holds-the-log-and-the-slider-fades-the-window
#[test]
fn the_lock_holds_the_log_and_the_slider_fades_the_window() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut f = Frame::new();
    f.reload(&mut ui, &mut p);
    let lock = ui.element(Token::of("ChatWindowLock")).expect("the lock");
    let l = log(&p, 0);
    let locked = |ui: &Ui| ui.node(l).unwrap().bool_attr(Token::of("text_locked"));
    assert!(!locked(&ui));
    f.click(&mut ui, &mut p, lock);
    assert!(locked(&ui), "the lock locks the log");
    assert_eq!(ui.node(lock).unwrap().state, Token::of("locked"));
    f.click(&mut ui, &mut p, lock);
    assert!(!locked(&ui));
    assert_eq!(ui.node(lock).unwrap().state, Token::of("unlocked"));

    let field = ui
        .element(Token::of("OptionsField"))
        .expect("the slider's field");
    assert!(!ui.node(field).unwrap().visible, "the slider hidden");
    let button = ui.element(Token::of("OptionsButton")).expect("the button");
    f.click(&mut ui, &mut p, button);
    assert!(
        ui.node(field).unwrap().visible,
        "the options button shows it"
    );
    let bar = ui
        .element(Token::of("OpacitySliderBar"))
        .expect("the slider");
    ui.set_attr(
        bar,
        Token::of("y_position"),
        auberean_ui::Value::Float(0.25),
    );
    let r = ui.screen_rect(bar).unwrap();
    // The slider's left edge, clear of the fields drawn over the rest of it.
    ui.pointer_move(r.x0 + 1, r.y0 + 10);
    f.tick(&mut ui, &mut p, &[], &[]);
    assert_eq!(ui.top(), Some(bar));
    ui.pointer_button(button_id::LEFT, true);
    f.tick(&mut ui, &mut p, &[], &[]);
    ui.pointer_button(button_id::LEFT, false);
    let out = f.tick(&mut ui, &mut p, &[], &[]);
    let kept: Vec<&Request> = out
        .iter()
        .filter(|r| matches!(r, Request::Chat(Ask::Opacity { .. })))
        .collect();
    assert!(!kept.is_empty(), "kept when let go: {out:?}");
    let Request::Chat(Ask::Opacity { window, opacity }) = kept[0] else {
        unreachable!()
    };
    assert_eq!(*window, Token::of("ChatWindow1"));
    let root = chat(&p).windows()[0].root().unwrap();
    assert!((ui.node(root).unwrap().alpha - opacity).abs() < 1e-6);
    assert!(
        (ui.node(l).unwrap().alpha - opacity).abs() < 1e-6,
        "all the way down"
    );
}

/// The strings the chat writes from its tables come out as the client writes them: the assisted
/// tell, a channel's label, the reply command's first name.
///
/// Behaviour: ui.chat.the-entry-is-labelled-with-the-chat-strings
#[test]
fn the_entry_is_labelled_with_the_chat_strings() {
    let a = records();
    let strings = a.data_id_by_enum(chat::STRINGS, 0x4000_0004);
    let mut tell = StringInfo::entry(strings, Token::of("ID_Chat_AssistedTell").raw());
    tell.add_variable_string_id(
        Token::of("prefix").raw(),
        Token::of("ID_Chat_CommandPrefix").raw(),
    );
    let name: Vec<u16> = "Cora".encode_utf16().collect();
    tell.add_variable_string(Token::of("target").raw(), &name);
    assert_eq!(a.write(&tell).as_deref(), Some("/tell Cora, "));
    let commands = a.data_id_by_enum(chat::COMMANDS, 0x4000_0004);
    assert_eq!(
        a.text(commands, Token::of("ID_Reply").raw()).as_deref(),
        Some("reply, r")
    );
}

/// The player's chat options the tests start from: a new player's, but sayings large and in the
/// eighth color, tells showing their window, combat not in the first window, the third window
/// devoted to the trade channel.
fn player() -> ChatSettings {
    let mut s = defaults(0);
    s.revision = 1;
    s.filters[0] &= !t::COMBAT;
    s.devoted[2] = t::TRADE_CHANNEL;
    for f in &mut s.fonts {
        if f.0 == t::SAY {
            (f.1, f.2) = (2, 7);
        }
        if f.0 == t::TELL {
            f.3 = true;
        }
    }
    s
}

struct Options {
    ui: Ui,
    p: GamePlay,
    t: f64,
    now: ChatSettings,
    defaults: ChatSettings,
}

impl Options {
    fn new() -> Self {
        let mut ui = Ui::new(records(), 1024, 768);
        let mut out = Vec::new();
        let mut p = GamePlay::new(&mut ui, &mut out);
        p.login_finished(&mut ui, &mut out);
        Self {
            ui,
            p,
            t: 0.0,
            now: player(),
            defaults: defaults(0),
        }
    }

    /// One frame with the player's chat options and a new player's, and the notices given.
    fn tick(&mut self, notices: &[Notice]) -> Vec<Request> {
        self.t += 0.05;
        self.ui.use_time(self.t);
        let mut out = Vec::new();
        let view = View {
            chat: Some(&self.now),
            chat_defaults: Some(&self.defaults),
            ..View::default()
        };
        self.p.tick(&mut self.ui, &view, notices, &mut out);
        out
    }

    fn el(&self, name: &str) -> Handle {
        self.ui
            .element(Token::of(name))
            .unwrap_or_else(|| panic!("{name} made"))
    }

    /// Point at the middle of an element and click it, the clock moving between.
    fn click(&mut self, h: Handle) -> Vec<Request> {
        let r = self.ui.screen_rect(h).expect("placed");
        self.ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
        let mut out = self.tick(&[]);
        for down in [true, false] {
            self.ui.pointer_button(button_id::LEFT, down);
            out.extend(self.tick(&[]));
        }
        out
    }

    fn page(&self) -> &ChatOptionsPage {
        self.p
            .child::<Toolbar>()
            .and_then(|t| t.window("OptionField"))
            .and_then(|c| c.child.as_any().downcast_ref::<OptionWindow>())
            .and_then(|w| w.chat.as_ref())
            .expect("the chat page")
    }

    fn option(&self, kind: u32, l: u32) -> &Row {
        self.page().option(kind, l).expect("the option")
    }

    /// The options window open on its chat page.
    fn open(&mut self) {
        let b = self.el("OptionButton");
        self.click(b);
        let tab = self.el("ChatTab");
        self.click(tab);
        assert!(self.ui.shown(self.el("ChatPage")), "the tab shows its page");
    }

    fn button(&self, name: &str) -> Handle {
        self.ui
            .child_recursive(self.el("ChatPage"), Token::of(name))
            .unwrap_or_else(|| panic!("the chat page's {name}"))
    }

    fn ghosted(&self, name: &str) -> bool {
        let b = self.button(name);
        self.ui.node(b).is_some_and(|n| n.bool_attr(attr::GHOSTED))
    }

    /// A window's box in a kind's section ticked or not, as a click ticks it.
    fn tick_box(&mut self, kind: u32, window: usize, on: bool) {
        let b = match &self.option(kind, label::WINDOWS[window]).control {
            Control::Flag {
                checkbox: Some(b), ..
            } => *b,
            _ => panic!("a check box"),
        };
        self.ui.set_attr(b, attr::HIGHLIGHTED, Value::Bool(on));
        self.tick(&[]);
    }

    /// A button pressed, its question answered yes; what followed.
    fn press_and_answer_yes(&mut self, name: &str) -> Vec<Request> {
        let b = self.button(name);
        let out = self.click(b);
        let asked: Vec<u32> = out
            .iter()
            .filter_map(|r| match r {
                Request::Dialog(d) => Some(d.id),
                _ => None,
            })
            .collect();
        assert_eq!(asked.len(), 1, "{name} asks first: {out:?}");
        let yes = Notice {
            native: notice::CLOSE_CONFIRMATION,
            words: vec![asked[0], 1],
            ..Notice::default()
        };
        self.tick(&[yes])
    }
}

fn asks(out: &[Request]) -> Vec<Ask> {
    out.iter()
        .filter_map(|r| match r {
            Request::Chat(a) => Some(a.clone()),
            _ => None,
        })
        .collect()
}

fn choice_parts(r: &Row) -> (Vec<(i32, u32)>, i32) {
    match &r.control {
        Control::Choice { pref, .. } => (
            pref.list_choices().iter().map(|c| (c.0, c.1 .1)).collect(),
            pref.default_value(),
        ),
        _ => panic!("a choice"),
    }
}

fn id(s: &str) -> u32 {
    Token::of(s).raw()
}

/// What is typed in a window's entry is drawn in the chat entry's font and color as the chat
/// options give them (a new player's: the fifth color, white), not in the entry's own color (the
/// red the error lines are drawn in); a window devoted to a channel types in that channel's.
///
/// Behaviour: ui.chat.the-entry-types-in-the-chat-entrys-color
#[test]
fn the_entry_types_in_the_chat_entrys_color() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut f = Frame::new();
    for k in &mut f.settings.fonts {
        if k.0 == t::GENERAL_CHANNEL {
            k.2 = 7;
        }
    }
    f.reload(&mut ui, &mut p);
    let e = entry(&p, 0);
    f.key(&mut ui, &mut p, key_id::USE);
    f.tick(&mut ui, &mut p, &[], &[]);
    ui.type_text("hello");
    let typed = color_at(&ui, e, "hello");
    assert_eq!(typed, offered(&ui, e, 4), "the chat entry's color");
    assert_ne!(typed, offered(&ui, e, 0), "not the entry's own red");
    f.key(&mut ui, &mut p, key_id::USE);

    // The second window, devoted to the general channel.
    f.key(&mut ui, &mut p, chat::key::CHAT_WINDOW[1]);
    let e2 = entry(&p, 1);
    let r = ui.screen_rect(e2).expect("placed");
    ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
    f.tick(&mut ui, &mut p, &[], &[]);
    for down in [true, false] {
        ui.pointer_button(button_id::LEFT, down);
        f.tick(&mut ui, &mut p, &[], &[]);
    }
    assert_eq!(ui.active(), Some(e2), "a press on the entry starts typing");
    ui.type_text("trade");
    assert_eq!(color_at(&ui, e2, "trade"), offered(&ui, e2, 7));
}

/// The chat page has a section for every kind of text the chat panel lists but the
/// administrators', in its order: a kind's font size (no large for the chat entry), font color,
/// the four windows' boxes and its popup box (the chat entry only the first two); the devoted
/// windows' four choices offer the sayings, fellowship, allegiance, faction, general, trade and
/// region channels and none, named from the kinds' list by place (general as the player-killer
/// channel, trade as general, region as trade, none as the chat entry). A new player's options are
/// the defaults; the page shows the player's own, nothing changed, Apply and Recall ghosted.
///
/// Behaviour: ui.options.the-chat-page-lists-each-kind-with-a-new-players-options-as-defaults
#[test]
fn the_chat_page_lists_each_kind_with_a_new_players_options_as_defaults() {
    let mut g = Options::new();
    g.open();
    let listed: Vec<u32> = TEXT_TYPES
        .iter()
        .filter(|t| t.kind != t::ADMIN)
        .map(|t| t.kind)
        .collect();
    assert_eq!(g.page().kinds(), listed);
    let say = g.page().section(t::SAY).expect("the sayings' section");
    assert_eq!(say.label, id("ID_Option_ChatSayMessages"));
    let (sizes, small) = choice_parts(g.option(t::SAY, label::FONT_SIZE));
    let size_names = [
        "ID_Option_ChatSmallerFont",
        "ID_Option_ChatSmallFont",
        "ID_Option_ChatMediumFont",
        "ID_Option_ChatLargeFont",
    ];
    assert_eq!(sizes, (0..).zip(size_names.map(id)).collect::<Vec<_>>());
    assert_eq!(small, 1, "a new player's sayings are small");
    let (colors, color) = choice_parts(g.option(t::SAY, label::FONT_COLOR));
    assert_eq!(colors.len(), 10);
    assert_eq!(color, 4);
    let (entry_sizes, _) = choice_parts(g.option(t::CHAT_ENTRY, label::FONT_SIZE));
    assert_eq!(entry_sizes.len(), 3, "no large for the chat entry");
    assert!(g.page().option(t::CHAT_ENTRY, label::WINDOWS[0]).is_none());
    assert!(g.page().option(t::CHAT_ENTRY, label::POPUP).is_none());
    let default_flag = |g: &Options, kind, l| match &g.option(kind, l).control {
        Control::Flag { pref, .. } => pref.default_value(),
        _ => panic!("a box"),
    };
    assert!(default_flag(&g, t::COMBAT, label::WINDOWS[0]));
    assert!(!default_flag(&g, t::COMBAT, label::WINDOWS[1]));
    assert!(default_flag(&g, t::GENERAL_CHANNEL, label::WINDOWS[1]));
    assert!(!default_flag(&g, t::TELL, label::POPUP));

    let devoted_names = [
        (4, "ID_Option_ChatSayMessages"),
        (8, "ID_Option_ChatFellowshipMessages"),
        (9, "ID_Option_ChatAllegianceMessages"),
        (10, "ID_Option_ChatFactionChannelMessages"),
        (11, "ID_Option_ChatPKChannelMessages"),
        (12, "ID_Option_ChatGeneralChannelMessages"),
        (13, "ID_Option_ChatTradeChannelMessages"),
        (17, "ID_Option_ChatEntryMessages"),
    ]
    .map(|(v, n)| (v, id(n)))
    .to_vec();
    for (w, default) in [(0, 4), (1, 11), (2, 8), (3, 9)] {
        let (offered, d) = choice_parts(g.option(t::DEVOTED, label::WINDOWS[w]));
        assert_eq!(offered, devoted_names, "window {}", w + 1);
        assert_eq!(d, default, "window {}'s default", w + 1);
    }

    // The player's own.
    assert_eq!(g.option(t::SAY, label::FONT_SIZE).choice_now(&g.ui), 3);
    assert_eq!(g.option(t::SAY, label::FONT_COLOR).choice_now(&g.ui), 7);
    assert!(g.option(t::TELL, label::POPUP).flag_now(&g.ui));
    assert!(!g.option(t::COMBAT, label::WINDOWS[0]).flag_now(&g.ui));
    assert!(g.option(t::SAY, label::WINDOWS[0]).flag_now(&g.ui));
    assert_eq!(
        g.option(t::DEVOTED, label::WINDOWS[2]).choice_now(&g.ui),
        12,
        "the trade channel, shown under the general channel's name"
    );
    assert!(!g.page().changed(&g.ui));
    assert!(g.ghosted("ApplyButton") && g.ghosted("RecallButton"));
}

/// A box ticked lights Apply and Recall; Apply asks first, then sets every option section by
/// section (the devoted channels, each kind's font size, color and popup flag), then each window's
/// filter as the boxes say (the administrators' kept as it was), the options read again and sent
/// to the server to keep; nothing is changed after.
///
/// Behaviour: ui.options.apply-asks-first-then-sets-every-chat-option-and-the-filters-last
#[test]
fn apply_asks_first_then_sets_every_chat_option_and_the_filters_last() {
    let mut g = Options::new();
    g.open();
    g.tick_box(t::SAY, 1, true);
    assert!(g.page().changed(&g.ui));
    assert!(!g.ghosted("ApplyButton") && !g.ghosted("RecallButton"));
    let out = g.press_and_answer_yes("ApplyButton");
    let a = asks(&out);
    let devoted: Vec<&Ask> = a
        .iter()
        .filter(|a| matches!(a, Ask::Devote { .. }))
        .collect();
    assert_eq!(
        devoted,
        [
            &Ask::Devote {
                window: 1,
                kind: t::SAY
            },
            &Ask::Devote {
                window: 2,
                kind: t::GENERAL_CHANNEL
            },
            &Ask::Devote {
                window: 3,
                kind: t::TRADE_CHANNEL
            },
            &Ask::Devote {
                window: 4,
                kind: t::ALLEGIANCE
            },
        ]
    );
    let count = |f: fn(&Ask) -> bool| a.iter().filter(|x| f(x)).count();
    assert_eq!(count(|a| matches!(a, Ask::FontSize { .. })), 16);
    assert_eq!(count(|a| matches!(a, Ask::FontColor { .. })), 16);
    assert_eq!(count(|a| matches!(a, Ask::Popup { .. })), 15);
    assert!(a.contains(&Ask::FontSize {
        kind: t::SAY,
        size: 2
    }));
    assert!(a.contains(&Ask::FontColor {
        kind: t::SAY,
        color: 7
    }));
    assert!(a.contains(&Ask::Popup {
        kind: t::TELL,
        on: true
    }));
    let first = player().filters[0];
    assert_eq!(
        a[a.len() - 6..],
        [
            Ask::Filter {
                window: 1,
                filter: first
            },
            Ask::Filter {
                window: 2,
                filter: t::GENERAL_CHANNEL | t::SAY
            },
            Ask::Filter {
                window: 3,
                filter: t::FELLOWSHIP
            },
            Ask::Filter {
                window: 4,
                filter: t::ALLEGIANCE
            },
            Ask::ReloadChatSettings,
            Ask::KeepChatOptions,
        ]
    );
    assert!(first & t::ADMIN != 0, "the administrators' kept");
    assert!(!g.page().changed(&g.ui));
    assert!(g.ghosted("ApplyButton"));
}

/// Recall (asked first) puts back what was applied; Default (asked first) puts a new player's
/// options in and applies them; the options window hidden with a box changed asks whether to
/// apply it.
///
/// Behaviour: ui.options.recall-and-default-ask-first-and-put-back-the-applied-or-the-defaults
#[test]
fn recall_and_default_ask_first_and_put_back_the_applied_or_the_defaults() {
    let mut g = Options::new();
    g.open();
    g.tick_box(t::COMBAT, 0, true);
    g.press_and_answer_yes("RecallButton");
    assert!(!g.option(t::COMBAT, label::WINDOWS[0]).flag_now(&g.ui));
    assert!(!g.page().changed(&g.ui) && g.ghosted("RecallButton"));

    let out = g.press_and_answer_yes("DefaultButton");
    assert_eq!(g.option(t::SAY, label::FONT_SIZE).choice_now(&g.ui), 1);
    assert!(g.option(t::COMBAT, label::WINDOWS[0]).flag_now(&g.ui));
    let a = asks(&out);
    assert!(a.contains(&Ask::Devote {
        window: 3,
        kind: t::FELLOWSHIP
    }));
    assert!(a.contains(&Ask::FontSize {
        kind: t::SAY,
        size: 0
    }));
    assert!(a.contains(&Ask::Filter {
        window: 1,
        filter: FIRST
    }));

    g.tick_box(t::TELL, 3, true);
    let close = g.el("OptionCloseButton");
    let out = g.click(close);
    assert!(
        out.iter().any(|r| matches!(r, Request::Dialog(_))),
        "hidden with a change, the window asks: {out:?}"
    );
}
