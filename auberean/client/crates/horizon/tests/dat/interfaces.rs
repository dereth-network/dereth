//! Horizon beside the game's own interface: chosen and left in one session, each keeping its
//! windows and showing the shortcut bar's set the other showed, the game's own screens before the
//! world, the chat entry's command line, and the social window asking the game.

use auberean_client_sdk::protocol::ClientMessage;
use auberean_horizon::ui::hud;
use auberean_horizon::ui::settings::Settings;
use auberean_options::page::{section, Change, Request as PageRequest};
use auberean_shell::input::{Event, Key};
use auberean_shell::interface::{Choice, Phase};

use crate::session::Session;

/// What the options window's client page sends to choose `choice` and keep it.
fn choose_on_the_page(s: &mut Session, choice: Choice) {
    let p = &mut s.shell.preferences;
    p.handle(&PageRequest::Attach(vec![section::UI]));
    let mut row = p
        .listing()
        .and_then(|l| l.sections.iter().find(|(at, _)| *at == section::UI))
        .and_then(|(_, prefs)| {
            prefs.iter().find_map(|c| match c {
                Change::Choice(c) => Some(c.clone()),
                _ => None,
            })
        })
        .expect("the interface row");
    assert!(row.set_value(choice.value()));
    p.handle(&PageRequest::Change(Change::Choice(row)));
    p.handle(&PageRequest::Apply(section::UI));
    p.handle(&PageRequest::Save);
    p.handle(&PageRequest::Detach(vec![section::UI]));
    s.step();
    s.step();
}

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

/// The calls and chat the client sent so far.
fn calls(s: &Session) -> usize {
    s.shell
        .client
        .as_ref()
        .expect("the session")
        .sent()
        .iter()
        .filter(|m| {
            matches!(
                m.message,
                ClientMessage::ServerCall(_) | ClientMessage::Chat(_)
            )
        })
        .count()
}

/// In the world, the game's own interface is left for Horizon and Horizon for it again in the same
/// session: the player stays in the world and nothing logs in again; Horizon draws its HUD as
/// textured batches, and lets go of its textures when it is left; a choice made in Horizon's
/// settings is kept in the preferences file; and Horizon's log still holds what it showed when it
/// is chosen again.
///
/// Behaviour: desktop.horizon.switching-keeps-the-session-and-each-interface-s-log
#[test]
fn switching_to_horizon_and_back_keeps_the_session_and_its_log() {
    let mut s = Session::new(None);
    s.enter_the_world();
    assert_eq!(s.shell.interface(), Choice::Retail);
    choose_on_the_page(&mut s, Choice::Horizon);
    assert_eq!(s.shell.interface(), Choice::Horizon);
    let (items, textures) = s.step();
    assert!(
        items > 0 && textures > 0,
        "the HUD drawn: {items} batches, {textures} textures"
    );
    assert_eq!(s.shell.phase(), Phase::World, "still in the world");
    assert_eq!(s.horizon().borrow().screen(), None, "in the world");
    let logins = |s: &Session| {
        s.shell
            .client
            .as_ref()
            .expect("the session")
            .sent()
            .iter()
            .filter(|m| matches!(m.message, ClientMessage::PlayCharacter(_)))
            .count()
    };
    assert_eq!(logins(&s), 1);
    // A line typed in Horizon's chat entry is carried out by the player's command line.
    press(&mut s, Key::Enter);
    assert!(s.horizon().borrow().ui().typing(), "Enter opens the entry");
    s.shell.event(&Event::Text("/emotelist".into()));
    s.step();
    let before = s.horizon().borrow().state().log.len();
    press(&mut s, Key::Enter);
    assert!(
        !s.horizon().borrow().ui().typing(),
        "Enter sends the line and closes the entry"
    );
    assert!(
        s.until(2.0, |s| s.horizon().borrow().state().log.len() > before),
        "the command's lines are in the log"
    );
    let log = s.horizon().borrow().state().log.clone();
    // Back to the game's own interface from Horizon's settings: the menu bar's system menu, its
    // Settings, then Retail.
    let k = auberean_horizon::scale::layout_scale((1920.0, 1080.0));
    let screen = (1920.0, 1080.0);
    let menu = hud::menu_button_rect(screen, k, hud::MenuButton::ALL.len() - 1);
    s.click((menu.x + menu.w / 2.0, menu.y + menu.h / 2.0));
    let settings = hud::system_row_rect(screen, k, 0);
    s.click((settings.x + settings.w / 2.0, settings.y + settings.h / 2.0));
    let window = s
        .horizon()
        .borrow()
        .ui()
        .settings
        .rect()
        .expect("the settings open");
    let row =
        Settings::controls(s.horizon().borrow().ui().settings.body().expect("drawn"), k).interface;
    assert!(window.contains(row.x, row.y));
    s.click((row.x + 9.0 * k, row.y + row.h / 2.0));
    s.step();
    assert_eq!(
        s.shell.interface(),
        Choice::Retail,
        "the game's own interface again"
    );
    let (_, textures) = s.step();
    assert_eq!(textures, 0, "Horizon's textures let go of");
    let kept = s.shell.host.settings.clone().expect("kept");
    assert!(kept.contains("[UI]\r\nInterface=Retail"), "{kept}");
    assert_eq!(s.shell.phase(), Phase::World);
    assert_eq!(s.retail.borrow().screen(), "play", "the game's play screen");
    // And Horizon again: its log as it was.
    choose_on_the_page(&mut s, Choice::Horizon);
    assert_eq!(s.shell.interface(), Choice::Horizon);
    let now = s.horizon().borrow().state().log.clone();
    assert!(
        now.len() >= log.len() && now.iter().take(log.len()).eq(log.iter()),
        "the log kept"
    );
    assert_eq!(logins(&s), 1, "the one session throughout");
}

/// A line the player's scripts write for the chat windows, as they hand it the client: a saying's
/// text.
fn write_line(s: &mut Session, text: &str) {
    use auberean_client_sdk::runtime::host::Shown;
    use auberean_client_sdk::script::native_object::TextInfo;
    let c = s.shell.client.as_mut().expect("the session");
    c.shown.push(Shown {
        about: None,
        kind: 0x10,
        text: TextInfo {
            has_literal: 1,
            literal: Some(text.encode_utf16().collect()),
            ..TextInfo::default()
        },
        notices_before: 0,
    });
    s.step();
}

/// The chat lines written while one interface is shown are in the other's log once it is chosen:
/// Horizon, chosen for the first time, starts from the lines written before it, and chosen again
/// it has those written while it was not shown, each once.
///
/// Behaviour: desktop.horizon.a-switch-carries-the-chat-lines-written-before
#[test]
fn a_switch_of_interface_carries_the_chat_lines_written_before() {
    let log = |s: &Session| -> Vec<String> {
        s.horizon()
            .borrow()
            .state()
            .log
            .iter()
            .map(|l| l.text.clone())
            .collect()
    };
    let count = |lines: &[String], text: &str| lines.iter().filter(|l| *l == text).count();
    let mut s = Session::new(None);
    s.enter_the_world();
    write_line(&mut s, "Written before Horizon");
    choose_on_the_page(&mut s, Choice::Horizon);
    assert_eq!(s.shell.interface(), Choice::Horizon);
    assert_eq!(
        count(&log(&s), "Written before Horizon"),
        1,
        "Horizon starts from the lines before it: {:?}",
        log(&s)
    );
    write_line(&mut s, "Written in Horizon");
    choose_on_the_page(&mut s, Choice::Retail);
    assert_eq!(s.shell.interface(), Choice::Retail);
    write_line(&mut s, "Written away from Horizon");
    choose_on_the_page(&mut s, Choice::Horizon);
    let now = log(&s);
    for line in [
        "Written before Horizon",
        "Written in Horizon",
        "Written away from Horizon",
    ] {
        assert_eq!(count(&now, line), 1, "{line}, once: {now:?}");
    }
}

/// The game's own windows and panels a switch might open or close, by their fields' names.
const RETAIL_WINDOWS: [&str; 9] = [
    "SkillField",
    "OptionField",
    "InventoryField",
    "QuestField",
    "NavigationField",
    "ExaminationField",
    "SocialField",
    "ToolbarField",
    "RadarField",
];

/// Which of the game's own windows show, by name.
fn retail_shown(
    retail: &std::rc::Rc<std::cell::RefCell<auberean_interface::Retail>>,
) -> Vec<(&'static str, bool)> {
    let r = retail.borrow();
    RETAIL_WINDOWS
        .iter()
        .map(|name| {
            let shown = r
                .ui()
                .find(auberean_ui::Token::of(name))
                .is_some_and(|h| r.ui().shown(h));
            (*name, shown)
        })
        .collect()
}

/// Which of Horizon's windows are open, and where each was drawn.
fn horizon_open(s: &Session) -> Vec<(&'static str, Option<auberean_horizon::draw::Rect>)> {
    let h = s.horizon();
    let h = h.borrow();
    let ui = h.ui();
    vec![
        (
            "skills",
            ui.skills.is_open().then(|| ui.skills.rect()).flatten(),
        ),
        ("map", ui.map.is_open().then(|| ui.map.rect()).flatten()),
        (
            "inventory",
            ui.goods
                .inventory
                .is_open()
                .then(|| ui.goods.inventory.rect())
                .flatten(),
        ),
        (
            "journal",
            ui.journal.is_open().then(|| ui.journal.rect()).flatten(),
        ),
        (
            "examine",
            ui.examine.is_open().then(|| ui.examine.rect()).flatten(),
        ),
    ]
}

/// The set of ten the player's scripts say the bar shows.
fn scripts_set(s: &Session) -> u32 {
    let c = s.shell.client.as_ref().expect("the session");
    c.view().toolbar.as_ref().map_or(0, |t| t.set)
}

/// The set Horizon's hotbar shows.
fn horizon_set(s: &Session) -> Option<u32> {
    s.horizon().borrow().state().bar.as_ref().map(|b| b.set)
}

/// What the options window's client page sends to choose `choice`, and ten frames.
fn switch_to(s: &mut Session, choice: Choice) {
    let p = &mut s.shell.preferences;
    p.handle(&PageRequest::Attach(vec![section::UI]));
    let mut row = p
        .listing()
        .and_then(|l| l.sections.iter().find(|(at, _)| *at == section::UI))
        .and_then(|(_, prefs)| {
            prefs.iter().find_map(|c| match c {
                Change::Choice(c) => Some(c.clone()),
                _ => None,
            })
        })
        .expect("the interface row");
    assert!(row.set_value(choice.value()));
    p.handle(&PageRequest::Change(Change::Choice(row)));
    p.handle(&PageRequest::Apply(section::UI));
    p.handle(&PageRequest::Detach(vec![section::UI]));
    for _ in 0..10 {
        s.step();
    }
    assert_eq!(s.shell.interface(), choice);
}

/// Ctrl held over `key`: the keys' set key.
fn ctrl(s: &mut Session, key: Key) {
    let hold = |s: &mut Session, down| {
        s.shell.event(&Event::Key {
            key: Key::ControlLeft,
            down,
            repeat: false,
        });
        s.step();
    };
    hold(s, true);
    press(s, key);
    hold(s, false);
}

/// Horizon's next-set key (=) pressed until its hotbar shows `set`; whether it does.
fn horizon_shows(s: &mut Session, set: u32) -> bool {
    for _ in 0..10 {
        if horizon_set(s) == Some(set) {
            return true;
        }
        press(s, Key::Equal);
        s.until(1.0, |s| horizon_set(s) == Some(scripts_set(s)));
    }
    horizon_set(s) == Some(set)
}

/// The middle of a game's own element, by its name.
fn middle(
    retail: &std::rc::Rc<std::cell::RefCell<auberean_interface::Retail>>,
    name: &str,
) -> (f32, f32) {
    let r = retail.borrow();
    let h = r
        .ui()
        .find(auberean_ui::Token::of(name))
        .unwrap_or_else(|| panic!("{name} on the screen"));
    let rect = r.ui().screen_rect(h).expect("placed");
    #[allow(clippy::cast_precision_loss)]
    let m = (
        (rect.x0 + rect.x1) as f32 / 2.0,
        (rect.y0 + rect.y1) as f32 / 2.0,
    );
    m
}

/// In the world under the game's own interface: the examination and skills windows opened (X and
/// the skills button) and their places kept (a click on the skills window's top), set 4 shown
/// (Ctrl+4).
fn retail_with_windows_and_set_four() -> Session {
    let mut s = Session::new(None);
    s.enter_the_world();
    for _ in 0..10 {
        s.step();
    }
    press(&mut s, Key::X);
    let skills = middle(&s.retail, "SkillButton");
    s.click(skills);
    let top = middle(&s.retail, "SkillPanelTop");
    s.click(top);
    let kept = |s: &Session| {
        let c = s.shell.client.as_ref().expect("the session");
        c.view()
            .options
            .as_ref()
            .and_then(|o| o.ui_locations.as_ref())
            .is_some_and(|l| !l.is_empty())
    };
    assert!(s.until(1.0, kept), "the places kept");
    ctrl(&mut s, Key::Digit4);
    let shown = s.until(2.0, |s| s.retail.borrow().shortcut_set() == Some(4));
    assert!(
        shown,
        "Ctrl+4 shows set 4: {:?}",
        s.retail.borrow().shortcut_set()
    );
    assert!(s.until(1.0, |s| scripts_set(s) == 4), "the scripts told");
    s
}

/// The game's own interface with windows open and set 4 shown, left for Horizon and chosen again:
/// Horizon shows set 4; back, the same windows show and no others, and the bar shows the set
/// Horizon last showed (6). The windows closed then and the window resized while Horizon is shown
/// (the display set up again, the panels put back where they are kept), the game's own interface
/// shows them closed, as they were left, not as the places kept with them open have them.
///
/// Behaviour: desktop.horizon.switching-back-keeps-the-game-s-own-windows-and-the-set-shown
#[test]
fn switching_back_keeps_the_game_s_own_windows_and_shows_the_set_horizon_showed() {
    let mut s = retail_with_windows_and_set_four();
    let before = retail_shown(&s.retail);
    assert!(
        before.contains(&("SkillField", true)) && before.contains(&("ExaminationField", true)),
        "{before:?}"
    );
    switch_to(&mut s, Choice::Horizon);
    assert_eq!(horizon_set(&s), Some(4), "Horizon shows the set shown");
    switch_to(&mut s, Choice::Retail);
    assert_eq!(retail_shown(&s.retail), before, "the same windows");
    assert_eq!(s.retail.borrow().shortcut_set(), Some(4));
    // Closed now, those windows stay closed through a switch and a resize away.
    press(&mut s, Key::X);
    let skills = middle(&s.retail, "SkillButton");
    s.click(skills);
    let closed = retail_shown(&s.retail);
    assert!(
        closed.contains(&("SkillField", false)) && closed.contains(&("ExaminationField", false)),
        "{closed:?}"
    );
    switch_to(&mut s, Choice::Horizon);
    assert!(horizon_shows(&mut s, 6), "Horizon shows set 6");
    s.shell.size = (1600, 900);
    s.step();
    switch_to(&mut s, Choice::Retail);
    for _ in 0..10 {
        s.step();
    }
    assert_eq!(retail_shown(&s.retail), closed, "the windows as left");
    assert_eq!(
        s.retail.borrow().shortcut_set(),
        Some(6),
        "the set Horizon showed"
    );
}

/// Horizon with windows open and set 3 shown, left for the game's own interface and chosen again:
/// the game's own bar shows set 3 meanwhile, and back in Horizon its windows are open again where
/// they were, and its bar still shows set 3.
///
/// Behaviour: desktop.horizon.switching-back-keeps-horizon-s-windows-and-the-set-shown
#[test]
fn switching_back_keeps_horizon_s_windows_where_they_were_and_the_set_shown() {
    let mut s = Session::new(Some("[UI]\r\nInterface=Horizon\r\n"));
    s.enter_the_world();
    {
        let h = s.horizon();
        let mut h = h.borrow_mut();
        let ui = h.ui_mut();
        ui.skills.toggle();
        ui.map.toggle(false);
        ui.goods.inventory.toggle();
    }
    assert!(horizon_shows(&mut s, 3), "Horizon shows set 3");
    for _ in 0..5 {
        s.step();
    }
    let before = horizon_open(&s);
    let open: Vec<&str> = before
        .iter()
        .filter(|(_, r)| r.is_some())
        .map(|(n, _)| *n)
        .collect();
    assert_eq!(open, ["skills", "map", "inventory"], "{before:?}");
    switch_to(&mut s, Choice::Retail);
    assert!(
        s.until(2.0, |s| s.retail.borrow().shortcut_set() == Some(3)),
        "the game's own bar shows set 3: {:?}",
        s.retail.borrow().shortcut_set()
    );
    switch_to(&mut s, Choice::Horizon);
    assert_eq!(
        horizon_open(&s),
        before,
        "the same windows, where they were"
    );
    assert_eq!(horizon_set(&s), Some(3));
}

/// The game's own interface chosen for the first time in the world shows what logging in with it
/// shows: the panels its login shows and the windows the player's options keep open (here the
/// skills window), as the options keep them, and the bar on the set the player's scripts hold (2,
/// chosen in Horizon).
///
/// Behaviour: desktop.horizon.a-first-switch-to-the-game-s-own-interface-shows-its-login-s-panels
#[test]
fn a_first_switch_to_the_game_s_own_interface_shows_what_its_login_shows() {
    // What a login under the game's own interface shows, and the places it keeps with the skills
    // window open.
    let (login, kept) = {
        let mut s = Session::new(None);
        s.enter_the_world();
        for _ in 0..10 {
            s.step();
        }
        let login = retail_shown(&s.retail);
        let skills = middle(&s.retail, "SkillButton");
        s.click(skills);
        let r = s.retail.borrow();
        let frame = r.play().expect("the play screen").frame();
        let kept = auberean_interface::places::locations(&r.ui().frame_kept_places(frame));
        drop(r);
        (login, kept)
    };
    assert!(login.contains(&("SkillField", false)), "{login:?}");
    let mut s = Session::new(Some("[UI]\r\nInterface=Horizon\r\n"));
    s.enter_the_world();
    s.shell
        .client
        .as_mut()
        .expect("the session")
        .act(auberean_client_sdk::Action::KeepUiLocations { places: kept });
    assert!(horizon_shows(&mut s, 2), "Horizon shows set 2");
    assert!(s.until(1.0, |s| scripts_set(s) == 2), "the scripts told");
    switch_to(&mut s, Choice::Retail);
    assert!(
        s.until(2.0, |s| s.retail.borrow().screen() == "play"),
        "the play screen"
    );
    for _ in 0..10 {
        s.step();
    }
    let want: Vec<_> = login
        .iter()
        .map(|&(n, shown)| (n, shown || n == "SkillField"))
        .collect();
    assert_eq!(retail_shown(&s.retail), want, "the login's panels, as kept");
    assert_eq!(
        s.retail.borrow().shortcut_set(),
        Some(2),
        "the scripts' set"
    );
}

/// A line typed in Horizon's chat entry goes to the player's command line, as the game's own chat
/// sends it: the game's own answer (here, to a line the command line does not know, its line
/// asking for /help) shows in Horizon's log in the error's color, and nothing is sent for it.
///
/// Behaviour: desktop.horizon.a-line-typed-goes-to-the-command-line-and-its-answer-is-logged
#[test]
fn a_line_typed_in_horizon_goes_to_the_command_line_and_its_answer_is_logged() {
    let mut s = Session::new(Some("[UI]\r\nInterface=Horizon\r\n"));
    s.enter_the_world();
    for _ in 0..30 {
        s.step();
    }
    let (sent, lines) = (calls(&s), s.horizon().borrow().state().log.len());
    press(&mut s, Key::Enter);
    s.shell.event(&Event::Text("/nosuchcommand".into()));
    s.step();
    press(&mut s, Key::Enter);
    assert!(
        s.until(2.0, |s| s.horizon().borrow().state().log.len() > lines),
        "the answer shown"
    );
    for _ in 0..30 {
        s.step();
    }
    assert_eq!(calls(&s), sent, "nothing sent for it");
    let log = s.horizon().borrow().state().log.clone();
    let answer = log.iter().skip(lines).find(|l| l.text.contains("/help"));
    let answer = answer.unwrap_or_else(|| panic!("the game's line: {log:?}"));
    assert_eq!(answer.kind, auberean_horizon::state::ERROR_TEXT);
}

/// T, Horizon's reply key, opens the chat entry with the game's reply command in it (`/reply `), as
/// the game's own reply key starts its entry.
///
/// Behaviour: desktop.horizon.t-starts-a-reply-in-the-chat-entry
#[test]
fn t_starts_a_reply_to_the_last_tell_in_horizons_chat_entry() {
    let mut s = Session::new(Some("[UI]\r\nInterface=Horizon\r\n"));
    s.enter_the_world();
    for _ in 0..30 {
        s.step();
    }
    assert!(!s.horizon().borrow().ui().chat.typing());
    press(&mut s, Key::T);
    let h = s.horizon();
    let h = h.borrow();
    assert!(h.ui().chat.typing(), "the entry has the keys");
    assert_eq!(h.ui().chat.text(), "/reply ");
}

/// Horizon's key bindings page, in its settings: a key pressed for an action is bound to it, and
/// Apply writes Horizon's own key files and binds them from then on (Z runs forward), leaving the
/// player's own key map files alone.
///
/// Behaviour: desktop.horizon.the-key-bindings-page-writes-horizon-s-key-files
#[test]
fn horizons_key_bindings_page_writes_its_key_files_and_binds_them() {
    use auberean_horizon::ui::bindings::Bindings;
    use auberean_horizon::ui::settings::tab;
    use auberean_shell::interface_keys::HORIZON_FILES;
    const FORWARD: u32 = 0x4000_0020;
    let mut s = Session::new(Some("[UI]\r\nInterface=Horizon\r\n"));
    s.enter_the_world();
    s.step();
    press(&mut s, Key::K);
    s.horizon().borrow_mut().ui_mut().settings.tab = tab::KEYS;
    s.step();
    s.step();
    assert!(
        s.horizon().borrow().ui().settings.bindings.maps.is_some(),
        "the keys bound now read"
    );
    let theirs: Vec<String> = s.shell.host.files.keys().cloned().collect();
    s.horizon()
        .borrow_mut()
        .ui_mut()
        .settings
        .bindings
        .capturing = Some(FORWARD);
    press(&mut s, Key::Z);
    assert_eq!(s.horizon().borrow().ui().settings.bindings.capturing, None);
    let k = auberean_horizon::scale::layout_scale((1920.0, 1080.0));
    let body = s.horizon().borrow().ui().settings.body().expect("drawn");
    let apply = Bindings::parts(Settings::page(body, k), k).apply;
    s.click((apply.x + apply.w / 2.0, apply.y + apply.h / 2.0));
    s.step();
    for f in HORIZON_FILES {
        assert!(s.shell.host.files.contains_key(f), "{f} written");
    }
    for f in theirs {
        assert!(
            !HORIZON_FILES.contains(&f.as_str()),
            "{f} was not Horizon's"
        );
    }
    assert_eq!(
        s.shell.keys.stack.action(Key::Z.input_word(), 0),
        Some(FORWARD),
        "bound"
    );
}

/// Horizon's settings open on Horizon's key for them (K, from the key layers the shell pushes
/// while Horizon is shown) and close on Escape; a movement chosen there is applied to the orbit
/// camera, kept in the preferences file as `[Orbit] Movement` and shown as chosen.
///
/// Behaviour: desktop.horizon.the-settings-open-on-their-key-and-set-the-orbit-camera
#[test]
fn horizons_settings_open_on_their_key_and_set_the_orbit_cameras_movement() {
    let mut s = Session::new(Some("[UI]\r\nInterface=Horizon\r\n"));
    s.enter_the_world();
    s.step();
    press(&mut s, Key::K);
    assert!(s.horizon().borrow().ui().settings.is_open(), "K opens them");
    s.step();
    s.step();
    assert!(
        s.horizon().borrow().ui().orbit.is_some(),
        "the camera's preferences listed"
    );
    let k = auberean_horizon::scale::layout_scale((1920.0, 1080.0));
    let body = s.horizon().borrow().ui().settings.body().expect("drawn");
    let row = Settings::controls(body, k).movement;
    s.click((row.x + row.w / 2.0 + 9.0 * k, row.y + row.h / 2.0));
    s.step();
    let asked = auberean_shell::interface::OrbitSettings::default();
    assert_eq!(
        s.shell.preferences.orbit_over(asked).movement,
        auberean_shell::interface::MovementMode::Character,
        "applied"
    );
    let kept = s.shell.host.settings.clone().expect("kept");
    assert!(
        kept.contains("[Orbit]") && kept.contains("Movement=Character"),
        "{kept}"
    );
    // The window shows the movement chosen, though the client does not list it again.
    s.step();
    assert_eq!(
        s.horizon().borrow().ui().orbit.map(|o| o.movement),
        Some(auberean_shell::interface::MovementMode::Character),
        "shown as chosen"
    );
    press(&mut s, Key::Escape);
    assert!(
        !s.horizon().borrow().ui().settings.is_open(),
        "Escape closes them"
    );
}

/// In the world Horizon's minimap places the player as the game's radar does: inside a building
/// or a dungeon it reads "Indoors" and reaches the shorter range, and outdoors the map coordinates
/// of the world's size; and it holds the objects the player's scripts put on the radar within its
/// reach, the player not among them.
///
/// Behaviour: desktop.horizon.the-minimap-places-the-player-and-holds-the-radar-s-objects
#[test]
fn horizons_minimap_places_the_player_and_holds_the_radars_objects() {
    let mut s = Session::new(None);
    s.enter_the_world();
    choose_on_the_page(&mut s, Choice::Horizon);
    s.step();
    let view = s.shell.client.as_ref().expect("the session").view().clone();
    let body = view.player.as_ref().expect("the player").body;
    let region = &crate::session::data().0.region.region;
    let cells = |blocks: u32| i32::try_from(blocks * 8).expect("a size");
    let want = auberean_primitives::land::map_coords(
        body.cell,
        (cells(region.blocks_wide), cells(region.blocks_long)),
    );
    let state = s.horizon().borrow().state().clone();
    let inside = body.cell & 0xFFFF >= 0x100;
    assert_eq!(state.radar.inside, inside);
    assert_eq!(state.radar.coordinates, want);
    let line = auberean_horizon::ui::minimap::place_line(&state);
    let world = view.world.as_ref().expect("the world");
    let (short, long) = world
        .radar_ranges
        .unwrap_or(auberean_client_sdk::contract::view::RADAR_RANGES);
    if inside {
        assert_eq!(line.as_deref(), Some("Indoors"));
        assert!((state.radar.range - short).abs() < 1e-6);
    } else {
        assert!(want.is_some() && line.is_some());
        assert!((state.radar.range - long).abs() < 1e-6);
    }
    let range = state.radar.range;
    let on_radar = world
        .things
        .iter()
        .filter(|t| t.radar.is_some() && t.id != world.player)
        .filter(|t| {
            t.place.is_some_and(|p| {
                let [e, n, _] =
                    auberean_client_sdk::contract::view::offset((body.cell, body.origin), p);
                e * e + n * n <= range * range
            })
        })
        .count();
    // The player itself is the arrow at the middle, not a blip.
    assert_eq!(state.radar.blips.len(), on_radar);
}

/// Horizon's social window opens on Horizon's key for it (O); a fellowship asked for there, its name
/// typed, goes to the player's scripts, which send the server their call for it; Escape closes the
/// window.
///
/// Behaviour: desktop.horizon.the-social-window-opens-on-its-key-and-asks-the-game
#[test]
fn horizons_social_window_opens_on_its_key_and_a_fellowship_asked_for_reaches_the_server() {
    let mut s = Session::new(Some("[UI]\r\nInterface=Horizon\r\n"));
    s.enter_the_world();
    s.step();
    press(&mut s, Key::O);
    assert!(s.horizon().borrow().ui().social.is_open(), "O opens it");
    s.step();
    let k = auberean_horizon::scale::layout_scale((1920.0, 1080.0));
    let body = s.horizon().borrow().ui().social.body().expect("drawn");
    let parts = auberean_horizon::ui::social::parts(body, k);
    let centre = |r: auberean_horizon::draw::Rect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
    s.click(centre(parts.field));
    assert!(
        s.horizon().borrow().ui().typing(),
        "the name box has the keys"
    );
    s.shell.event(&Event::Text("Wanderers".into()));
    s.step();
    let before = calls(&s);
    s.click(centre(parts.foot[3]));
    assert!(
        s.until(2.0, |s| calls(s) > before),
        "the scripts sent their call"
    );
    press(&mut s, Key::Escape);
    assert!(
        !s.horizon().borrow().ui().social.is_open(),
        "Escape closes it"
    );
}
