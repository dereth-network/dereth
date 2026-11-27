//! Logging out under Horizon on a clock stepped a sixtieth of a second a frame, against a server
//! that lets the character go as the game's own servers did: frozen in its glow at once, its
//! leaving played at 15.2 s, into portal space at 18.9 s and out of the world at 19.0 s. Then
//! logging out and entering again in the same run, against a server that lets the character go at
//! once and enters it again on a world server of its own: what each interface's chat log holds in
//! the second session, the one shown through the relog and the one hidden through it, what the
//! chat entry holds and recalls of the first, and which windows Horizon has open again.

use auberean_client_sdk::protocol::login::Empty;
use auberean_client_sdk::protocol::physics::{Fx, ObjectEvent};
use auberean_client_sdk::protocol::script::{Arg, ClientCall, FunctionId, ScriptArgs};
use auberean_client_sdk::protocol::types::{BehaviorParams, ObjectStamp};
use auberean_client_sdk::protocol::ServerMessage;
use auberean_horizon::ui::leaving;
use auberean_horizon::ui::pregame::Screen;
use auberean_shell::input::{Event, Key};
use auberean_shell::interface::{Choice, Phase};
use auberean_testkit::double::script::Event as Step;

use crate::session::{
    c2s, character_list, choose_on_the_page, enter_again, log_off_at_once, s2c, write_line,
    Session, LOGIN, PLAYER, WORLD,
};

const HORIZON: &str = "[UI]\r\nInterface=Horizon\r\n";

/// When the recording's player logs off, seconds on its clock.
const ASKED: f64 = 10.0;

/// A call of one of the player's functions with whole numbers.
fn call(function: u32, args: &[u32]) -> ServerMessage {
    let args: Vec<Arg> = args.iter().map(|a| Arg::Int(*a)).collect();
    ServerMessage::PlayerCall(ClientCall {
        function: FunctionId(function),
        args: Some(ScriptArgs::from_values(&args).expect("whole numbers pack")),
    })
}

fn on_player<A>(action: A) -> ObjectEvent<A> {
    ObjectEvent {
        sender: ObjectStamp {
            id: PLAYER,
            instance_stamp: 1,
            other_stamp: 0,
        },
        action,
    }
}

/// The log-off as the 2017 servers answered it, each step its time after the request.
fn log_off() -> Vec<Step> {
    let leaving = BehaviorParams {
        bits: BehaviorParams::BEHAVIOR,
        behavior: 0x4100_0083,
        ..BehaviorParams::default()
    };
    vec![
        c2s(ASKED, 0, 4, 0x10027),
        s2c(ASKED + 0.10, 1, 1, WORLD | 7, &call(0x8212_0021, &[1])),
        s2c(
            ASKED + 0.12,
            1,
            1,
            WORLD | 8,
            &ServerMessage::DoFx(on_player(Fx {
                fx: 0x4100_0010,
                scalar: 1.0,
            })),
        ),
        s2c(
            ASKED + 15.2,
            1,
            1,
            WORLD | 9,
            &ServerMessage::PlayBehavior(on_player(leaving)),
        ),
        s2c(
            ASKED + 18.9,
            1,
            1,
            WORLD | 10,
            &call(0x8212_0020, &[4, 0, 0]),
        ),
        s2c(
            ASKED + 19.0,
            0,
            1,
            LOGIN | 4,
            &ServerMessage::CharExitGame(Empty),
        ),
        s2c(
            ASKED + 19.01,
            0,
            1,
            LOGIN | 5,
            &ServerMessage::CharacterList(character_list()),
        ),
    ]
}

/// One frame of the leaving.
#[derive(Clone, Copy, Debug)]
struct Moment {
    /// Seconds since Log Out was asked.
    at: f64,
    phase: Phase,
    waiting: bool,
    /// Every quad of Horizon's stands in the wait's box.
    only_the_box: bool,
    /// Horizon drew nothing at all.
    nothing: bool,
    portal: bool,
    world: bool,
    screen: Option<Screen>,
}

/// Logging out under Horizon: from Log Out until the session leaves the world the HUD is gone and
/// the box says the client is logging out, the world drawn behind it, for as long as the server
/// holds the character (19 s here, its leaving played on the way); portal space then shows, and
/// stays in sight with nothing over it for [`leaving::PORTAL_SEEN`] after the session has left the
/// world, before character select comes up over its view.
///
/// Behaviour: desktop.horizon.logging-out-shows-the-wait-then-portal-space-then-character-select
#[test]
fn logging_out_shows_the_wait_then_portal_space_long_enough_to_see_then_character_select() {
    let mut s = Session::going_on(Some(HORIZON), log_off());
    let entered = s.until(20.0, |s| {
        s.shell.phase() == Phase::World && !s.shell.scene.in_portal_space()
    });
    assert!(entered, "in the world: {:?}", s.shell.phase());
    s.until(0.5, |_| false);
    let hud = s.horizon().borrow().draw_list().quads.len();
    s.horizon().borrow_mut().log_out();
    let asked = s.shell.host.time;
    let (sw, sh) = (1920.0, 1080.0);
    let k = auberean_horizon::scale::layout_scale((sw, sh));
    let r = leaving::layout((sw, sh), k);
    let near = 24.0 * k;
    let mut moments: Vec<Moment> = Vec::new();
    while s.shell.host.time < asked + 30.0 {
        s.advance();
        let (world, portal) = {
            let out = s.shell.frame();
            (out.world.is_some(), out.portal.is_some())
        };
        let h = s.horizon();
        let h = h.borrow();
        let quads = &h.draw_list().quads;
        let m = Moment {
            at: s.shell.host.time - asked,
            phase: s.shell.phase(),
            waiting: h.logging_out(),
            only_the_box: quads.iter().all(|q| {
                q.dst.x >= r.x - near
                    && q.dst.y >= r.y - near
                    && q.dst.right() <= r.right() + near
                    && q.dst.bottom() <= r.bottom() + near
            }),
            nothing: quads.is_empty(),
            portal,
            world,
            screen: h.screen(),
        };
        moments.push(m);
        if m.screen == Some(Screen::Select) && !m.portal {
            break;
        }
    }
    let last = moments.last().copied().expect("frames");
    assert_eq!(last.screen, Some(Screen::Select), "{last:?}");
    let left = moments
        .iter()
        .position(|m| m.phase != Phase::World)
        .expect("the session left the world");
    let portal_from = moments
        .iter()
        .position(|m| m.portal)
        .expect("portal space shown");
    let select_from = moments
        .iter()
        .position(|m| m.screen == Some(Screen::Select))
        .expect("character select");
    // The wait: the box alone over the world drawn behind it, the HUD gone, from the frame after
    // Log Out until the session leaves the world, as long as the server holds the character.
    let wait = &moments[1..left];
    assert!(hud > 40, "the HUD drawn before: {hud} quads");
    assert!(
        wait.iter()
            .all(|m| m.waiting && m.only_the_box && !m.nothing),
        "the box alone: {:?}",
        wait.iter()
            .find(|m| !(m.waiting && m.only_the_box && !m.nothing))
    );
    assert!(
        moments[1..portal_from].iter().all(|m| m.world),
        "the world behind the box"
    );
    let held = moments[left].at;
    assert!(
        (18.9..19.3).contains(&held),
        "the server held it {held:.2} s"
    );
    // Portal space from the server's word, before the session leaves the world.
    let into = moments[portal_from].at;
    assert!((18.8..19.0).contains(&into), "portal space at {into:.2} s");
    // Then in sight, nothing of Horizon's over it from the frame Horizon hears the world was left,
    // until it has been seen long enough.
    let gone = moments[portal_from..]
        .iter()
        .position(|m| !m.portal)
        .map(|i| portal_from + i)
        .expect("portal space goes");
    let after = &moments[left + 1..gone];
    assert!(
        after
            .iter()
            .all(|m| m.screen == Some(Screen::Passage) && m.nothing && !m.waiting),
        "nothing over portal space: {:?}",
        after
            .iter()
            .find(|m| !(m.screen == Some(Screen::Passage) && m.nothing))
    );
    let seen_after = moments[gone].at - moments[left].at;
    assert!(
        (leaving::PORTAL_SEEN - 0.05..leaving::PORTAL_SEEN + 0.1).contains(&seen_after),
        "portal space in sight {seen_after:.2} s after the world was left"
    );
    // Character select once it goes, and not before.
    assert!(
        select_from >= gone,
        "select at {select_from}, portal gone at {gone}"
    );
}

/// When the player logs off, and asks to enter again, seconds on the recording's clock.
const LOG_OFF: f64 = 10.0;
const AGAIN: f64 = 12.0;

/// A key pressed and let go, a frame after each.
fn press(s: &mut Session, key: Key) {
    for down in [true, false] {
        s.shell.event(&Event::Key {
            key,
            down,
            repeat: false,
        });
        s.step();
    }
}

/// The texts of Horizon's log, as the game wrote them and as its window holds them.
fn horizon_log(s: &Session) -> (Vec<String>, Vec<String>) {
    let h = s.horizon();
    let h = h.borrow();
    let written = h.state().log.iter().map(|l| l.text.clone()).collect();
    let held = h.ui().chat.lines().iter().map(|l| l.text.clone()).collect();
    (written, held)
}

/// In the world and out of portal space.
fn in_the_world(s: &Session) -> bool {
    s.shell.phase() == Phase::World && !s.shell.scene.in_portal_space()
}

/// Horizon's character select up, Enter asks to enter the world as the character selected and
/// the question's first button says yes.
fn enter_from_horizons_select(s: &mut Session) {
    let listed = s.until(30.0, |s| {
        s.horizon().borrow().screen() == Some(Screen::Select)
    });
    assert!(
        listed,
        "character select: {:?}",
        s.horizon().borrow().screen()
    );
    press(s, Key::Enter);
    let yes = s
        .horizon()
        .borrow()
        .ui()
        .pregame
        .confirm_buttons
        .first()
        .copied()
        .expect("the question asked");
    s.click((yes.x + yes.w / 2.0, yes.y + yes.h / 2.0));
    let entered = s.until(15.0, in_the_world);
    assert!(entered, "in the world again: {:?}", s.shell.phase());
}

/// Under Horizon, a relog starts the chat log empty and forgets the lines typed, as the game's
/// own chat windows and command line go with the session: the lines written and the line typed
/// before the log-off are gone from the log once the character has left the world, and are not
/// there in the second session, where a line written shows; the entry left open with half a line
/// in it is closed, opens again empty, and Up recalls nothing.
///
/// Behaviour: desktop.horizon.a-relog-starts-the-chat-log-empty
#[test]
fn a_relog_under_horizon_starts_the_chat_log_empty_and_forgets_the_lines_typed() {
    let mut more = log_off_at_once(LOG_OFF);
    more.extend(enter_again(AGAIN));
    let mut s = Session::going_on(Some(HORIZON), more);
    let entered = s.until(20.0, in_the_world);
    assert!(entered, "in the world: {:?}", s.shell.phase());
    // A line typed, which the player's command line keeps and answers in the log, and a saying
    // written.
    press(&mut s, Key::Enter);
    s.shell.event(&Event::Text("/emotelist".into()));
    s.step();
    let before = horizon_log(&s).0.len();
    press(&mut s, Key::Enter);
    assert!(
        s.until(2.0, |s| horizon_log(s).0.len() > before),
        "the command's lines are in the log"
    );
    write_line(&mut s, "Before the relog");
    let (written, held) = horizon_log(&s);
    assert!(
        written.iter().any(|l| l == "Before the relog")
            && held.iter().any(|l| l == "Before the relog"),
        "the saying in the log: {written:?}, {held:?}"
    );
    // Half a line typed and not sent, the entry left open.
    press(&mut s, Key::Enter);
    s.shell.event(&Event::Text("half a line".into()));
    s.step();
    assert_eq!(s.horizon().borrow().ui().chat.text(), "half a line");
    s.horizon().borrow_mut().log_out();
    let left = s.until(30.0, |s| s.shell.phase() != Phase::World);
    assert!(left, "out of the world");
    s.step();
    let (written, held) = horizon_log(&s);
    assert!(
        written.is_empty() && held.is_empty(),
        "the log empty once the world is left: {written:?}, {held:?}"
    );
    enter_from_horizons_select(&mut s);
    write_line(&mut s, "After the relog");
    let (written, held) = horizon_log(&s);
    for log in [&written, &held] {
        assert!(
            !log.iter().any(|l| l == "Before the relog"),
            "nothing of the first session: {log:?}"
        );
        assert!(
            log.iter().any(|l| l == "After the relog"),
            "the second session's line: {log:?}"
        );
    }
    // The entry closed, and opened again empty; Up steps back through the lines typed: none in
    // this session.
    assert!(!s.horizon().borrow().ui().typing(), "the entry closed");
    press(&mut s, Key::Enter);
    assert!(s.horizon().borrow().ui().typing(), "the entry open");
    assert_eq!(
        s.horizon().borrow().ui().chat.text(),
        "",
        "nothing of the half line"
    );
    press(&mut s, Key::ArrowUp);
    s.step();
    assert_eq!(
        s.horizon().borrow().ui().chat.text(),
        "",
        "nothing recalled from the first session"
    );
    assert_eq!(s.shell.interface(), Choice::Horizon);
}

/// The middle of one of the game's own elements on the screen, by its name.
fn middle_of(s: &Session, name: &str) -> (f32, f32) {
    let r = s.retail.borrow();
    let ui = r.ui();
    let h = ui
        .find(auberean_ui::Token::of(name))
        .unwrap_or_else(|| panic!("{name} on the screen"));
    let rect = ui.screen_rect(h).expect("placed");
    #[allow(clippy::cast_precision_loss)]
    let m = (
        (rect.x0 + rect.x1) as f32 / 2.0,
        (rect.y0 + rect.y1) as f32 / 2.0,
    );
    m
}

/// Frames until the game's own play screen has its panels up, within a few seconds of the host's
/// clock; then the player's options come, as the player's scripts send them as a character
/// enters (the server here sends none of its own), and the chat windows take from them what they
/// show. Whether the panels came.
fn panels_up(s: &mut Session) -> bool {
    use auberean_client_sdk::runtime::host::PanelNotice;
    let up = s.until(5.0, |s| {
        let r = s.retail.borrow();
        r.play().is_some_and(|p| !p.children().is_empty()) && r.chat().is_some()
    });
    let c = s.shell.client.as_mut().expect("the session");
    c.notices.push(PanelNotice {
        native: auberean_ui_game::gameplay::notice::RELOAD_OPTIONS,
        about: Some(PLAYER),
        ..PanelNotice::default()
    });
    s.step();
    up
}

/// The game's own first chat window's lines.
fn retail_log(s: &Session) -> Vec<String> {
    s.retail.borrow().chat_lines()
}

/// What the game's own first chat window's entry holds.
fn retail_entry(s: &Session) -> String {
    let r = s.retail.borrow();
    let entry = r
        .chat()
        .and_then(|c| c.windows().first())
        .and_then(auberean_ui_game::chat::ChatPane::entry);
    entry.and_then(|h| r.ui().text(h)).unwrap_or_default()
}

/// Under the game's own interface: in the world, a saying written and shown in its chat window
/// and a line typed there, a log-off, and the Enter button on its character list to enter again;
/// the session at the second entry's play screen.
fn relog_in_the_game_s_own_interface() -> Session {
    let mut more = log_off_at_once(LOG_OFF);
    more.extend(enter_again(AGAIN));
    let mut s = Session::going_on(None, more);
    s.enter_the_world();
    assert!(panels_up(&mut s), "the play screen's panels");
    write_line(&mut s, "Before the relog");
    let shown = s.until(2.0, |s| {
        retail_log(s).iter().any(|l| l == "Before the relog")
    });
    assert!(shown, "the saying in the chat window: {:?}", retail_log(&s));
    // A line typed in the chat window, which the player's command line keeps.
    press(&mut s, Key::Enter);
    s.shell.event(&Event::Text("/emotelist".into()));
    s.step();
    press(&mut s, Key::Enter);
    assert_eq!(retail_entry(&s), "", "the line sent");
    log_off_and_enter_on_the_game_s_own_screens(&mut s);
    s
}

/// Under the game's own interface, in the world: a log-off, and the Enter button on its character
/// list to enter again; the session at the second entry's play screen.
fn log_off_and_enter_on_the_game_s_own_screens(s: &mut Session) {
    // The log-off the play screen's Leave World asks for.
    s.shell
        .client
        .as_mut()
        .expect("the session")
        .act(auberean_client_sdk::contract::Action::LogOff);
    let listed = s.until(30.0, |s| s.retail.borrow().screen() == "select");
    assert!(listed, "the character list: {}", s.retail.borrow().screen());
    // The screens before the world are laid out at 1024 by 768, fitted to the window.
    let [left, top, width, _] = auberean_shell::interface::fitted((1024, 768), s.shell.size);
    let (x, y) = middle_of(s, "EnterButton");
    let k = width / 1024.0;
    s.click((left + x * k, top + y * k));
    let entered = s.until(15.0, |s| {
        in_the_world(s) && s.retail.borrow().screen() == "play"
    });
    assert!(
        entered,
        "in the world again: {:?}, {}",
        s.shell.phase(),
        s.retail.borrow().screen()
    );
    assert!(panels_up(s), "the second play screen's panels");
}

/// A relog under the game's own interface starts its chat windows empty: they go with the play
/// screen when the character leaves the world, and the next entry's are made anew, the player's
/// command line with them. A saying written before the log-off is not in the second session's
/// window, where one written shows; Up in its entry recalls nothing of the line typed before.
///
/// Behaviour: desktop.interface.a-relog-starts-the-chat-windows-empty
#[test]
fn a_relog_in_the_game_s_own_interface_starts_its_chat_windows_empty() {
    let mut s = relog_in_the_game_s_own_interface();
    write_line(&mut s, "After the relog");
    let shown = s.until(2.0, |s| {
        retail_log(s).iter().any(|l| l == "After the relog")
    });
    assert!(shown, "the second session's line: {:?}", retail_log(&s));
    assert!(
        !retail_log(&s).iter().any(|l| l == "Before the relog"),
        "nothing of the first session: {:?}",
        retail_log(&s)
    );
    press(&mut s, Key::Enter);
    press(&mut s, Key::ArrowUp);
    s.step();
    assert_eq!(
        retail_entry(&s),
        "",
        "nothing recalled from the first session"
    );
}

/// Horizon chosen for the first time after a relog under the game's own interface starts from the
/// lines written in the second session: those of the first are no longer carried to it.
///
/// Behaviour: desktop.horizon.a-first-switch-after-a-relog-carries-only-the-session-s-lines
#[test]
fn horizon_chosen_after_a_relog_is_not_handed_the_first_session_s_lines() {
    let mut s = relog_in_the_game_s_own_interface();
    write_line(&mut s, "After the relog");
    choose_on_the_page(&mut s, Choice::Horizon);
    assert_eq!(s.shell.interface(), Choice::Horizon);
    let (written, held) = horizon_log(&s);
    assert!(
        written.iter().any(|l| l == "After the relog"),
        "the second session's line: {written:?}"
    );
    for log in [&written, &held] {
        assert!(
            !log.iter().any(|l| l == "Before the relog"),
            "nothing of the first session: {log:?}"
        );
    }
}

/// The number of log-offs the client sent so far.
fn log_offs(s: &Session) -> usize {
    s.shell.client.as_ref().map_or(0, |c| {
        c.sent()
            .iter()
            .filter(|m| m.message.message_type() == 0x10027)
            .count()
    })
}

/// Horizon left in the world for the game's own interface, which logs out and enters again:
/// Horizon shown again in the second session starts its log from that session, nothing of what
/// it showed in the first kept, and its chat entry closed and empty.
///
/// Behaviour: desktop.horizon.shown-again-after-a-relog-its-chat-starts-empty
#[test]
fn horizon_hidden_through_a_relog_shows_only_the_second_session_s_lines_when_chosen_again() {
    let mut more = log_off_at_once(LOG_OFF);
    more.extend(enter_again(AGAIN));
    let mut s = Session::going_on(Some(HORIZON), more);
    let entered = s.until(20.0, in_the_world);
    assert!(entered, "in the world: {:?}", s.shell.phase());
    write_line(&mut s, "Before the relog");
    press(&mut s, Key::Enter);
    s.shell.event(&Event::Text("half a line".into()));
    s.step();
    assert!(
        horizon_log(&s).0.iter().any(|l| l == "Before the relog"),
        "the saying in Horizon's log"
    );
    choose_on_the_page(&mut s, Choice::Retail);
    assert!(panels_up(&mut s), "the game's own play screen");
    log_off_and_enter_on_the_game_s_own_screens(&mut s);
    write_line(&mut s, "After the relog");
    choose_on_the_page(&mut s, Choice::Horizon);
    assert_eq!(s.shell.interface(), Choice::Horizon);
    let (written, held) = horizon_log(&s);
    assert!(
        written.iter().any(|l| l == "After the relog"),
        "the second session's line: {written:?}"
    );
    for log in [&written, &held] {
        assert!(
            !log.iter().any(|l| l == "Before the relog"),
            "nothing of the first session: {log:?}"
        );
    }
    let h = s.horizon();
    let h = h.borrow();
    assert!(!h.ui().typing(), "the entry closed");
    assert_eq!(h.ui().chat.told(), "", "nothing of the half line kept");
}

/// The game's own interface left in the world for Horizon, which logs out and enters again: shown
/// again in the second session, the game's own play screen is a new one, its chat window holding
/// nothing of the first session, and the play screen of the first goes without logging the
/// character off again.
///
/// Behaviour: desktop.horizon.the-game-s-own-interface-shown-again-after-a-relog-starts-anew
#[test]
fn the_game_s_own_interface_hidden_through_a_relog_comes_back_with_a_new_play_screen() {
    let mut more = log_off_at_once(LOG_OFF);
    more.extend(enter_again(AGAIN));
    let mut s = Session::going_on(None, more);
    s.enter_the_world();
    assert!(panels_up(&mut s), "the play screen's panels");
    write_line(&mut s, "Before the relog");
    let shown = s.until(2.0, |s| {
        retail_log(s).iter().any(|l| l == "Before the relog")
    });
    assert!(shown, "the saying in the chat window: {:?}", retail_log(&s));
    choose_on_the_page(&mut s, Choice::Horizon);
    s.horizon().borrow_mut().log_out();
    let left = s.until(30.0, |s| s.shell.phase() != Phase::World);
    assert!(left, "out of the world");
    enter_from_horizons_select(&mut s);
    choose_on_the_page(&mut s, Choice::Retail);
    assert_eq!(s.shell.interface(), Choice::Retail);
    assert!(panels_up(&mut s), "the play screen's panels again");
    write_line(&mut s, "After the relog");
    let shown = s.until(2.0, |s| {
        retail_log(s).iter().any(|l| l == "After the relog")
    });
    assert!(shown, "the second session's line: {:?}", retail_log(&s));
    assert!(
        !retail_log(&s).iter().any(|l| l == "Before the relog"),
        "nothing of the first session: {:?}",
        retail_log(&s)
    );
    s.until(1.0, |_| false);
    assert_eq!(log_offs(&s), 1, "the one log-off");
    assert_eq!(s.shell.phase(), Phase::World, "still in the world");
}

/// Which of Horizon's windows are open, by name.
fn horizon_windows(s: &Session) -> Vec<&'static str> {
    let h = s.horizon();
    let h = h.borrow();
    let ui = h.ui();
    [
        ("skills", ui.skills.is_open()),
        ("examine", ui.examine.is_open()),
        ("map", ui.map.is_open()),
        ("journal", ui.journal.is_open()),
        ("social", ui.social.is_open()),
        ("inventory", ui.goods.inventory.is_open()),
        ("crafting", ui.crafting.is_open()),
        ("settings", ui.settings.is_open()),
    ]
    .into_iter()
    .filter_map(|(name, open)| open.then_some(name))
    .collect()
}

/// Horizon's windows opened: the skills, the map, the inventory, the journal, crafting, social,
/// examine and the settings.
fn open_windows(s: &mut Session) {
    let h = s.horizon();
    let mut h = h.borrow_mut();
    let ui = h.ui_mut();
    ui.skills.toggle();
    ui.map.toggle(false);
    ui.goods.inventory.toggle();
    ui.journal.toggle();
    ui.crafting.toggle();
    let mut asked = Vec::new();
    ui.social.toggle(&mut asked);
    ui.examine.open(&mut asked);
    ui.settings.toggle();
}

/// A session where Horizon, in the world with its windows open, is left for the game's own
/// interface; the session at the game's own play screen.
fn horizon_left_with_its_windows_open() -> Session {
    let mut more = log_off_at_once(LOG_OFF);
    more.extend(enter_again(AGAIN));
    let mut s = Session::going_on(Some(HORIZON), more);
    let entered = s.until(20.0, in_the_world);
    assert!(entered, "in the world: {:?}", s.shell.phase());
    open_windows(&mut s);
    s.until(0.5, |_| false);
    choose_on_the_page(&mut s, Choice::Retail);
    assert!(panels_up(&mut s), "the game's own play screen");
    s
}

/// Horizon hidden through a relog opens, once it is shown again, the windows a Horizon shown
/// throughout the relog has open at the second entry (those it was left with), whether it is
/// shown again in the second session's world or on the character list before it.
///
/// Behaviour: desktop.horizon.shown-again-after-a-relog-its-windows-are-those-of-one-shown-throughout
#[test]
fn horizon_hidden_through_a_relog_opens_the_windows_one_shown_throughout_has_open() {
    // Shown throughout.
    let mut more = log_off_at_once(LOG_OFF);
    more.extend(enter_again(AGAIN));
    let mut s = Session::going_on(Some(HORIZON), more);
    let entered = s.until(20.0, in_the_world);
    assert!(entered, "in the world: {:?}", s.shell.phase());
    open_windows(&mut s);
    s.until(0.5, |_| false);
    let opened = horizon_windows(&s);
    assert_eq!(opened.len(), 8, "every window open: {opened:?}");
    s.horizon().borrow_mut().log_out();
    let left = s.until(30.0, |s| s.shell.phase() != Phase::World);
    assert!(left, "out of the world");
    enter_from_horizons_select(&mut s);
    s.until(1.0, |_| false);
    let throughout = horizon_windows(&s);
    // Hidden through the relog, shown again in the second session's world.
    let mut s = horizon_left_with_its_windows_open();
    log_off_and_enter_on_the_game_s_own_screens(&mut s);
    choose_on_the_page(&mut s, Choice::Horizon);
    s.until(1.0, |_| false);
    assert_eq!(horizon_windows(&s), throughout, "shown again in the world");
    // Hidden through the log-off, shown again on the character list, entering through its own.
    let mut s = horizon_left_with_its_windows_open();
    s.shell
        .client
        .as_mut()
        .expect("the session")
        .act(auberean_client_sdk::contract::Action::LogOff);
    let listed = s.until(30.0, |s| s.retail.borrow().screen() == "select");
    assert!(listed, "the character list: {}", s.retail.borrow().screen());
    choose_on_the_page(&mut s, Choice::Horizon);
    enter_from_horizons_select(&mut s);
    s.until(1.0, |_| false);
    assert_eq!(
        horizon_windows(&s),
        throughout,
        "shown again on the character list"
    );
}
