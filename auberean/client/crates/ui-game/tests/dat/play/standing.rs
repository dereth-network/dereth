//! The play screen on the retail layouts: its frame, its panels, the toolbar and its windows, the
//! camera, the game's flow.

use auberean_text::StringInfo;
use auberean_ui::framework::flow::{Factory, Screen};
use auberean_ui::types::{button_id, key_id};
use auberean_ui::{Token, Ui};
use auberean_ui_game::camera::{self, action as cam};
use auberean_ui_game::flow::{self, mode, Event, GameFlow};
use auberean_ui_game::gameplay::{
    self, action, layout_file, notice, GamePlay, Notice, Record, Request, View,
};
use auberean_ui_game::shortcuts::Shortcuts;
use auberean_ui_game::statusbar::{StatusBar, Vital};
use auberean_ui_game::toolbar::{action as tb, Toolbar};

use crate::common::records;

/// The play screen made and the player in the world: its panels up.
fn play(ui: &mut Ui) -> (GamePlay, Vec<Request>) {
    let mut out = Vec::new();
    let mut p = GamePlay::new(ui, &mut out);
    p.login_finished(ui, &mut out);
    (p, out)
}

/// One frame: the clock, then the play screen's tick.
fn tick(ui: &mut Ui, p: &mut GamePlay, t: &mut f64, notices: &[Notice]) -> Vec<Request> {
    *t += 0.05;
    ui.use_time(*t);
    let mut out = Vec::new();
    p.tick(ui, &View::default(), notices, &mut out);
    out
}

fn el(ui: &Ui, name: &str) -> auberean_ui::Handle {
    ui.element(Token::of(name))
        .unwrap_or_else(|| panic!("{name} made"))
}

fn shown(ui: &Ui, name: &str) -> bool {
    ui.shown(el(ui, name))
}

/// Point at the middle of an element and click it, the clock moving between.
fn click(ui: &mut Ui, p: &mut GamePlay, t: &mut f64, name: &str) -> Vec<Request> {
    let r = ui.screen_rect(el(ui, name)).expect("placed");
    ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
    let mut out = tick(ui, p, t, &[]);
    for down in [true, false] {
        ui.pointer_button(button_id::LEFT, down);
        out.extend(tick(ui, p, t, &[]));
    }
    out
}

fn key(ui: &mut Ui, p: &mut GamePlay, t: &mut f64, k: u32) -> Vec<Request> {
    ui.key(k, true);
    let mut out = tick(ui, p, t, &[]);
    ui.key(k, false);
    out.extend(tick(ui, p, t, &[]));
    out
}

/// The play screen makes no panel until the player is in the world; then the status bar, the
/// toolbar with its shortcut bar, the chat window and the radar show, the toolbar on the screen's
/// foot with its dismount and consignment buttons hidden, the seven toolbar windows made and
/// hidden, the mouse look crosshair hidden.
///
/// Behaviour: ui.hud.the-toolbar-sits-on-the-bottom-edge
#[test]
fn the_toolbar_sits_on_the_bottom_edge() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut out = Vec::new();
    let mut p = GamePlay::new(&mut ui, &mut out);
    assert!(out.contains(&Request::DisplayInWorld(true)));
    assert!(
        p.children().is_empty(),
        "no panel before the player is in the world"
    );
    assert!(ui.element(Token::of("ToolbarField")).is_none());
    p.login_finished(&mut ui, &mut out);
    assert_eq!(p.children().len(), 16, "sixteen panels");
    for n in [
        "StatusbarField",
        "ToolbarField",
        "ShortcutField",
        "ChatWindow1",
        "RadarField",
    ] {
        assert!(shown(&ui, n), "{n} shows");
    }
    let toolbar = el(&ui, "ToolbarField");
    assert_eq!(ui.screen_rect(toolbar).unwrap().y1, 767);
    for n in [
        "DismountButton",
        "ConsignmentsButton",
        "SkillField",
        "OptionField",
        "InventoryField",
        "QuestField",
        "NavigationField",
        "ExaminationField",
        "SocialField",
        "MouseLookCursorField",
    ] {
        assert!(!shown(&ui, n), "{n} hidden");
    }
    assert!(
        out.contains(&Request::PrecludeFromBottom(38)),
        "the toolbar covers 38 pixels"
    );
}

/// The vitals show health and vigor as "current / greatest"; anyone but a hero sees the focus
/// bar's ornament in its place, without numbers.
///
/// Behaviour: ui.hud.vitals-read-current-over-greatest-and-focus-is-a-heros
#[test]
fn vitals_read_current_over_greatest_and_focus_is_a_heros() {
    let mut ui = Ui::new(records(), 1024, 768);
    let (mut p, _) = play(&mut ui);
    let bar = p.child_mut::<StatusBar>().expect("the status bar");
    bar.set_vitals(
        &mut ui,
        Vital {
            current: 40,
            max: 100,
        },
        Vital {
            current: 100,
            max: 100,
        },
        None,
    );
    let text = |ui: &Ui, n: &str| ui.text(el(ui, n));
    assert_eq!(text(&ui, "HealthText").as_deref(), Some("40 / 100"));
    let focus = el(&ui, "FocusMeter");
    assert_eq!(ui.node(focus).unwrap().state, Token::of("not_hero"));
    assert!(!ui.node(el(&ui, "FocusText")).unwrap().visible);
    let health = el(&ui, "HealthMeter");
    let fill = ui.child(health, Token::of("left_fill")).unwrap();
    // The fill slides in from the left: 40% of the 215-pixel bar shows, the part hidden cut to a
    // whole pixel toward the bar.
    assert_eq!(ui.node(fill).unwrap().rect.x1, 86);
}

/// A toolbar window's button shows its window and, pressed again, hides it; its tooltip says
/// "Show" or "Hide" as the window is; the window's close button hides it.
///
/// Behaviour: ui.toolbar.a-windows-button-shows-and-hides-it-and-its-tooltip-says-which
#[test]
fn a_windows_button_shows_and_hides_it_and_its_tooltip_says_which() {
    let mut ui = Ui::new(records(), 1024, 768);
    let (mut p, _) = play(&mut ui);
    let mut t = 0.0;
    let tip = |ui: &Ui| ui.element_tooltip(el(ui, "SkillButton")).map(str::to_owned);
    assert_eq!(
        tip(&ui).as_deref(),
        Some("Show the Skills Panel (keyboard shortcut: F7).")
    );
    click(&mut ui, &mut p, &mut t, "SkillButton");
    assert!(shown(&ui, "SkillField"), "the button shows its window");
    assert_eq!(
        tip(&ui).as_deref(),
        Some("Hide the Skills Panel (keyboard shortcut: F7).")
    );
    click(&mut ui, &mut p, &mut t, "SkillButton");
    assert!(!shown(&ui, "SkillField"), "and hides it");
    click(&mut ui, &mut p, &mut t, "InventoryButton");
    assert!(shown(&ui, "InventoryField"));
    click(&mut ui, &mut p, &mut t, "InventoryCloseButton");
    assert!(!shown(&ui, "InventoryField"), "the close button hides it");
}

/// On a screen larger than the layout's, each toolbar window opened by its key keeps its width: the
/// map, skills, quests and social windows grow down to the screen's foot but no taller than a
/// thousand pixels, the options and inventory windows move to the foot's corners, the examination
/// window stays where the layout puts it.
///
/// Behaviour: ui.toolbar.the-windows-keep-their-width-on-a-larger-screen
#[test]
fn the_windows_keep_their_width_on_a_larger_screen() {
    let r = auberean_ui::Rect::new;
    let at_1080 = [
        (
            tb::NAVIGATION_PANEL,
            "NavigationField",
            r(0, 354, 255, 1079),
        ),
        (tb::SKILL_PANEL, "SkillField", r(648, 354, 963, 1079)),
        (tb::QUEST_PANEL, "QuestField", r(708, 354, 963, 1079)),
        (tb::SOCIAL_PANEL, "SocialField", r(708, 354, 963, 1079)),
        (tb::OPTION_PANEL, "OptionField", r(0, 666, 255, 1079)),
        (
            tb::INVENTORY_PANEL,
            "InventoryField",
            r(1604, 666, 1919, 1079),
        ),
        (
            tb::EXAMINATION_PANEL,
            "ExaminationField",
            r(0, 354, 255, 767),
        ),
    ];
    let at_2160 = [
        (
            tb::NAVIGATION_PANEL,
            "NavigationField",
            r(0, 354, 255, 1353),
        ),
        (tb::SKILL_PANEL, "SkillField", r(648, 354, 963, 1353)),
        (tb::OPTION_PANEL, "OptionField", r(0, 1746, 255, 2159)),
    ];
    for (size, windows) in [((1920, 1080), &at_1080[..]), ((3840, 2160), &at_2160[..])] {
        let mut ui = Ui::new(records(), size.0, size.1);
        let (mut p, _) = play(&mut ui);
        let mut t = 0.0;
        for (k, field, rect) in windows {
            key(&mut ui, &mut p, &mut t, *k);
            assert!(shown(&ui, field), "{field} shows");
            assert_eq!(
                ui.screen_rect(el(&ui, field)),
                Some(*rect),
                "{field} at {size:?}"
            );
            key(&mut ui, &mut p, &mut t, *k);
        }
    }
}

/// The windows' keys show and hide their windows; the toolbar's key hides the toolbar, its
/// shortcut bar, its windows and the chat window (and shows them again); the status bar's and the radar's keys
/// theirs; the key for the whole interface hides every panel.
///
/// Behaviour: ui.gameplay.the-panels-keys-show-and-hide-them
#[test]
fn the_panels_keys_show_and_hide_them() {
    let mut ui = Ui::new(records(), 1024, 768);
    let (mut p, _) = play(&mut ui);
    let mut t = 0.0;
    key(&mut ui, &mut p, &mut t, tb::SKILL_PANEL);
    assert!(shown(&ui, "SkillField"), "the skills key");
    key(&mut ui, &mut p, &mut t, tb::NAVIGATION_PANEL);
    assert!(shown(&ui, "NavigationField"), "the map key");
    assert!(shown(&ui, "SkillField"), "one window does not hide another");
    let out = key(&mut ui, &mut p, &mut t, action::TOOLBAR_PANEL);
    assert!(!shown(&ui, "ToolbarField") && !shown(&ui, "ShortcutField"));
    assert!(!shown(&ui, "ChatWindow1"), "the chat window goes with it");
    assert!(!shown(&ui, "SkillField"), "its windows go with it");
    assert!(out.contains(&Request::PrecludeFromBottom(0)));
    key(&mut ui, &mut p, &mut t, action::TOOLBAR_PANEL);
    assert!(shown(&ui, "ToolbarField") && shown(&ui, "ShortcutField"));
    assert!(shown(&ui, "SkillField"), "and come back with it");
    key(&mut ui, &mut p, &mut t, action::STATUSBAR_PANEL);
    assert!(!shown(&ui, "StatusbarField"));
    key(&mut ui, &mut p, &mut t, action::STATUSBAR_PANEL);
    key(&mut ui, &mut p, &mut t, action::RADAR);
    assert!(!shown(&ui, "RadarField"));
    key(&mut ui, &mut p, &mut t, action::RADAR);
    key(&mut ui, &mut p, &mut t, action::ALL_UI);
    for n in [
        "StatusbarField",
        "ToolbarField",
        "ShortcutField",
        "RadarField",
    ] {
        assert!(!shown(&ui, n), "{n} hidden with the whole interface");
    }
    key(&mut ui, &mut p, &mut t, action::ALL_UI);
    for n in [
        "StatusbarField",
        "ToolbarField",
        "ShortcutField",
        "RadarField",
    ] {
        assert!(shown(&ui, n), "{n} back");
    }
}

/// The war button asks the player's scripts to start attacking and goes on showing peace; the
/// scripts' notice that the player attacks shows war (the war button ghosted).
///
/// Behaviour: ui.shortcuts.the-attack-buttons-follow-the-scripts-notice
#[test]
fn the_attack_buttons_follow_the_scripts_notice() {
    let mut ui = Ui::new(records(), 1024, 768);
    let (mut p, _) = play(&mut ui);
    let mut t = 0.0;
    let ghosted = |ui: &Ui, n: &str| {
        ui.node(el(ui, n))
            .is_some_and(|n| n.bool_attr(auberean_ui::token::attr::GHOSTED))
    };
    assert!(!ghosted(&ui, "StartAttackButton") && ghosted(&ui, "StopAttackButton"));
    let out = click(&mut ui, &mut p, &mut t, "StartAttackButton");
    assert!(out.contains(&Request::Attack { start: true }));
    let attacking = |p: &GamePlay| {
        p.child::<Toolbar>()
            .and_then(|t| t.child::<Shortcuts>())
            .map(Shortcuts::attacking)
    };
    assert_eq!(attacking(&p), Some(false), "peace until the scripts say");
    assert!(!ghosted(&ui, "StartAttackButton"));
    let war = Notice {
        native: notice::UPDATE_ATTACK_STATE,
        about: Some(1),
        words: vec![1],
        ..Notice::default()
    };
    tick(&mut ui, &mut p, &mut t, &[war]);
    assert_eq!(attacking(&p), Some(true));
    assert!(ghosted(&ui, "StartAttackButton") && !ghosted(&ui, "StopAttackButton"));
}

/// The scripts' notices that the player is mounted and has a sale waiting show the dismount and
/// consignment buttons and light the wings (the consignment's alert sounding); pressed, the
/// buttons ask to dismount and to show the sales.
///
/// Behaviour: ui.toolbar.the-wings-follow-the-mount-and-the-sales
#[test]
fn the_wings_follow_the_mount_and_the_sales() {
    let mut ui = Ui::new(records(), 1024, 768);
    let (mut p, _) = play(&mut ui);
    let mut t = 0.0;
    let n = |native, w| Notice {
        native,
        about: Some(1),
        words: vec![w],
        ..Notice::default()
    };
    let out = tick(
        &mut ui,
        &mut p,
        &mut t,
        &[
            n(notice::UPDATE_MOUNT_STATE, 1),
            n(notice::SHOW_SALE_REMINDER, 1),
        ],
    );
    assert!(shown(&ui, "DismountButton") && shown(&ui, "ConsignmentsButton"));
    assert_eq!(
        ui.node(el(&ui, "RightWingField")).unwrap().state,
        Token::of("dismount")
    );
    assert_eq!(
        ui.node(el(&ui, "LeftWingField")).unwrap().state,
        Token::of("consignments")
    );
    assert!(
        out.contains(&Request::Sound(0x2A00_00EB)),
        "the alert: {out:?}"
    );
    let out = click(&mut ui, &mut p, &mut t, "DismountButton");
    assert!(out.contains(&Request::Dismount));
    let out = click(&mut ui, &mut p, &mut t, "ConsignmentsButton");
    assert!(out.contains(&Request::ShowSales));
    tick(
        &mut ui,
        &mut p,
        &mut t,
        &[
            n(notice::UPDATE_MOUNT_STATE, 0),
            n(notice::SHOW_SALE_REMINDER, 0),
        ],
    );
    assert!(!shown(&ui, "DismountButton") && !shown(&ui, "ConsignmentsButton"));
}

fn camera_asks(out: &[Request]) -> Vec<camera::Request> {
    out.iter()
        .filter_map(|r| match r {
            Request::Camera(c) => Some(*c),
            _ => None,
        })
        .collect()
}

/// Mouse look comes on while its key is held and goes when it is let go; its switch turns it on;
/// the crosshair shows only in mouse look; the left button runs forward in mouse look; the first
/// person switch; the arrows ask to turn and move, unless a text entry has the keyboard.
///
/// Behaviour: ui.camera.mouse-look-first-person-and-the-arrows
#[test]
fn mouse_look_first_person_and_the_arrows() {
    let mut ui = Ui::new(records(), 1024, 768);
    let (mut p, out) = play(&mut ui);
    let mut t = 0.0;
    assert!(camera_asks(&out).contains(&camera::Request::MouseLook(false)));
    ui.key(cam::INSTANT_MOUSE_LOOK, true);
    let out = tick(&mut ui, &mut p, &mut t, &[]);
    assert!(camera_asks(&out).contains(&camera::Request::MouseLook(true)));
    assert!(
        shown(&ui, "MouseLookCursorField"),
        "the crosshair in mouse look"
    );
    ui.pointer_button(button_id::LEFT, true);
    let out = tick(&mut ui, &mut p, &mut t, &[]);
    assert!(camera_asks(&out).contains(&camera::Request::Act {
        action: cam::FORWARD,
        extent: 1.0
    }));
    ui.pointer_button(button_id::LEFT, false);
    let out = tick(&mut ui, &mut p, &mut t, &[]);
    assert!(camera_asks(&out).contains(&camera::Request::Act {
        action: cam::FORWARD,
        extent: 0.0
    }));
    ui.key(cam::INSTANT_MOUSE_LOOK, false);
    let out = tick(&mut ui, &mut p, &mut t, &[]);
    assert!(camera_asks(&out).contains(&camera::Request::MouseLook(false)));
    assert!(!shown(&ui, "MouseLookCursorField"));
    let out = key(&mut ui, &mut p, &mut t, cam::TOGGLE_MOUSE_LOOK);
    assert_eq!(
        camera_asks(&out),
        vec![
            camera::Request::MouseLook(true),
            camera::Request::FirstPerson(false)
        ],
        "the switch turns it on; letting it go changes nothing"
    );
    let out = key(&mut ui, &mut p, &mut t, cam::FIRST_PERSON);
    assert!(camera_asks(&out).contains(&camera::Request::FirstPerson(true)));
    let out = key(&mut ui, &mut p, &mut t, key_id::LEFT);
    assert_eq!(
        camera_asks(&out),
        vec![
            camera::Request::Act {
                action: cam::TURN_LEFT,
                extent: 1.0
            },
            camera::Request::Act {
                action: cam::TURN_LEFT,
                extent: 0.0
            }
        ]
    );
    // A chat line being typed keeps the arrows.
    let entry = el(&ui, "ChatEntry");
    ui.activate(entry);
    let out = key(&mut ui, &mut p, &mut t, key_id::UP);
    assert!(camera_asks(&out).is_empty(), "{out:?}");
    assert!(p.camera().is_some_and(camera::Camera::mouse_look));
}

/// The log-out key ends the session at the next tick of the clock: please wait, the panels gone,
/// the character logged off, once.
///
/// Behaviour: ui.gameplay.the-log-out-key-ends-the-session-at-the-next-tick
#[test]
fn the_log_out_key_ends_the_session_at_the_next_tick() {
    let mut ui = Ui::new(records(), 1024, 768);
    let (mut p, _) = play(&mut ui);
    let mut t = 0.0;
    ui.key(action::LOGOUT, true);
    let mut out = Vec::new();
    p.tick(&mut ui, &View::default(), &[], &mut out);
    assert!(
        !out.contains(&Request::LogOff),
        "not before the clock ticks"
    );
    let out = tick(&mut ui, &mut p, &mut t, &[]);
    assert!(out.contains(&Request::LogOff));
    assert!(out.iter().any(|r| matches!(
        r,
        Request::Dialog(d) if d.kind == gameplay::dialog::WAIT && d.text == "Please wait ..."
    )));
    assert!(p.ended() && p.children().is_empty());
    assert!(
        ui.element(Token::of("ToolbarField")).is_none(),
        "the panels gone"
    );
    let out = key(&mut ui, &mut p, &mut t, action::LOGOUT);
    assert!(!out.contains(&Request::LogOff), "once");
}

/// The panels' places written as the game's layout file, and read back.
///
/// Behaviour: ui.gameplay.the-layout-file-keeps-the-windows-places
#[test]
fn the_layout_file_keeps_the_windows_places() {
    let mut ui = Ui::new(records(), 1024, 768);
    let (_p, _) = play(&mut ui);
    let text = layout_file::write(&ui);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 9);
    assert!(lines[0].starts_with("<SOCIA> X:"));
    assert_eq!(lines[5], "<CHAT1> X:270 Y: 586 W: 500 H: 144");
    layout_file::read(
        &mut ui,
        "<CHAT1> X:10 Y: 20 W: 400 H: 150\n<NOPE> X:1 Y: 1 W: 1 H: 1\n",
    );
    let r = ui.screen_rect(el(&ui, "ChatWindow1")).unwrap();
    assert_eq!((r.x0, r.y0, r.width(), r.height()), (10, 20, 400, 150));
}

/// The scripts' commands to write and read the panels' places ask for the file they name; with no
/// name, the file of this character, world and screen.
///
/// Behaviour: ui.gameplay.the-layout-commands-name-their-file
#[test]
fn the_layout_commands_name_their_file() {
    let mut ui = Ui::new(records(), 1024, 768);
    let (mut p, _) = play(&mut ui);
    let named = |native, name: &str| Notice {
        native,
        records: vec![Some(Record::Text(auberean_text::StringInfo {
            literal: Some(name.encode_utf16().collect()),
            ..auberean_text::StringInfo::default()
        }))],
        ..Notice::default()
    };
    let mut out = Vec::new();
    p.notice(&mut ui, &named(notice::SAVE_UI, "mine"), &mut out);
    p.notice(&mut ui, &named(notice::LOAD_UI, "mine"), &mut out);
    assert_eq!(
        out,
        [
            Request::SaveLayout {
                name: "mine".into()
            },
            Request::LoadLayout {
                name: "mine".into()
            }
        ]
    );
    assert_eq!(layout_file::file_name("mine", "A", "W", 1, 2), "mine.txt");
    assert_eq!(
        layout_file::file_name("", "Arwic", "Coldeve", 1024, 768),
        "UI-Arwic-Coldeve-1024-768.txt"
    );
}

struct Named;

impl Screen<Vec<Token>> for Named {
    fn show(&mut self, _ui: &mut Ui, _d: &mut Vec<Token>, _on: bool) {}
    fn update(&mut self, _ui: &mut Ui, _d: &mut Vec<Token>) {}
    fn argument(&mut self, _ui: &mut Ui, d: &mut Vec<Token>, a: &str) {
        if !a.is_empty() {
            d.push(Token::of(a));
        }
    }
    fn close(self: Box<Self>, _ui: &mut Ui, _d: &mut Vec<Token>) {}
}

fn named(_ui: &mut Ui, _d: &mut Vec<Token>) -> Option<Box<dyn Screen<Vec<Token>>>> {
    Some(Box::new(Named))
}

/// The game starts on the opening screen with the default cursor; a character the server refuses
/// for being in the world already goes back to the character list with the reason's message; one
/// logged off goes to the disconnected screen with the server's own words; a lost connection
/// changes nothing while that screen is up; a boot's words go there too, in place of the booted
/// message, when they are valid (a literal, or an entry its table holds: the line saying the
/// account is banned), and the booted message when they are not; the screenshot key, let go, asks
/// for a screenshot.
///
/// Behaviour: ui.flow.the-game-flow-starts-on-the-intro-and-follows-the-servers-reasons
#[test]
fn the_game_flow_starts_on_the_intro_and_follows_the_servers_reasons() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut f = GameFlow::new(&mut ui, Vec::new(), |_| Some(named as Factory<_>));
    assert_eq!(ui.cursor().map(|c| c.picture), Some(0x4100_0870));
    let mut t = 0.0;
    let mut tick = |ui: &mut Ui, f: &mut GameFlow<Vec<Token>>| {
        t += 0.05;
        ui.use_time(t);
        f.tick(ui)
    };
    tick(&mut ui, &mut f);
    assert_eq!(f.flow.mode(), mode::INTRO);
    f.character_error(&ui, 11, None);
    tick(&mut ui, &mut f);
    assert_eq!(f.flow.mode(), mode::CHARACTER_SELECTION);
    let said = flow::message(
        &ui,
        flow::MESSAGES,
        "ID_CHAR_ERROR_ENTER_GAME_CHARACTER_IN_WORLD",
    );
    assert!(!said.is_empty());
    assert_eq!(f.flow.data, vec![Token::of(&said)], "the reason's message");
    f.character_error(&ui, 4, Some(&StringInfo::literal("Gone.")));
    tick(&mut ui, &mut f);
    assert_eq!(f.flow.mode(), mode::DISCONNECTED);
    assert_eq!(
        f.flow.data.last(),
        Some(&Token::of("Gone.")),
        "the server's words"
    );
    f.server_died(&ui);
    assert_eq!(
        f.flow.queued_mode(),
        Token(0),
        "not while the disconnected screen is up"
    );
    // A boot: its line from the game's strings, else the booted message.
    let banned = ui
        .assets()
        .text(0x2500_0012, 0x974F_243D)
        .expect("the banned line");
    assert_eq!(banned, "Your account is currently banned.");
    f.character_error(&ui, 20, Some(&StringInfo::entry(0x2500_0012, 0x974F_243D)));
    tick(&mut ui, &mut f);
    assert_eq!(f.flow.mode(), mode::DISCONNECTED);
    assert_eq!(
        f.flow.data.last(),
        Some(&Token::of(&banned)),
        "the boot's line"
    );
    let booted = flow::message(&ui, flow::MESSAGES, "ID_CHAR_ERROR_CHARACTER_IS_BOOTED");
    assert!(!booted.is_empty());
    f.character_error(&ui, 20, Some(&StringInfo::entry(0x2500_0012, 0x1234_5678)));
    tick(&mut ui, &mut f);
    assert_eq!(
        f.flow.data.last(),
        Some(&Token::of(&booted)),
        "an entry the table lacks: the booted message"
    );
    ui.key(flow::SCREENSHOT_KEY, true);
    assert!(tick(&mut ui, &mut f).is_empty());
    ui.key(flow::SCREENSHOT_KEY, false);
    assert_eq!(tick(&mut ui, &mut f), vec![Event::Screenshot]);
}

/// What kind of panel a handler of the play screen is, by name.
fn handler_name(p: &GamePlay, h: gameplay::Handler) -> &'static str {
    use auberean_ui_game::{ingredients, inventory, names, pet, radar, shop, social, trade};
    let gameplay::Handler::Panel(id) = h else {
        return "play screen";
    };
    let Some(c) = p.panel(id) else {
        return "gone";
    };
    let a = c.as_any();
    if a.is::<names::Names>() {
        "names"
    } else if a.is::<radar::Radar>() {
        "radar"
    } else if a.is::<inventory::stack::StackSplitter>() {
        "stack"
    } else if a.is::<auberean_ui_game::selection::Selection>() {
        "selection"
    } else if a.is::<StatusBar>() {
        "status bar"
    } else if a.is::<Toolbar>() {
        "toolbar"
    } else if a.is::<social::mini::MiniFellowship>() {
        "fellowship"
    } else if a.is::<trade::Trade>() {
        "trade"
    } else if a.is::<inventory::external::ExternalContainer>() {
        "container"
    } else if a.is::<shop::Shop>() {
        "shop"
    } else if a.is::<shop::consignment::Consignments>() {
        "consignments"
    } else if a.is::<shop::new_sale::NewSale>() {
        "new sale"
    } else if a.is::<pet::Pets>() {
        "pets"
    } else if a.is::<ingredients::Ingredients>() {
        "ingredients"
    } else {
        "another"
    }
}

/// The play screen's panels register for their notices as the play screen makes them, in that
/// order, and the play screen registers for some of its own as it is made and for the rest once
/// its panels are made, after them: a notice reaches those who registered for it, in that order,
/// and no other.
///
/// Behaviour: ui.notices.the-play-screens-notices-reach-their-handlers-in-the-order-they-registered
#[test]
fn the_play_screens_notices_reach_their_handlers_in_the_order_they_registered() {
    let mut ui = Ui::new(records(), 1024, 768);
    let (p, _) = play(&mut ui);
    let heard = |native: u32| -> Vec<&'static str> {
        p.handlers(native)
            .iter()
            .map(|h| handler_name(&p, *h))
            .collect()
    };
    assert_eq!(
        heard(notice::RELOAD_OPTIONS),
        ["names", "radar", "status bar", "toolbar", "play screen"],
        "the play screen after its panels"
    );
    assert_eq!(
        heard(auberean_ui_game::dialog::notice::CLOSE_CONFIRMATION_DIALOG),
        [
            "toolbar",
            "trade",
            "shop",
            "consignments",
            "new sale",
            "ingredients",
            "play screen"
        ]
    );
    assert_eq!(
        heard(auberean_ui_game::inventory::notice::SELECTION_CHANGED),
        [
            "names",
            "radar",
            "toolbar",
            "fellowship",
            "trade",
            "container",
            "shop",
            "consignments",
            "pets"
        ]
    );
    assert_eq!(
        heard(auberean_ui_game::story::notice::FINISH_PLAY_SEQUENCE),
        ["play screen"],
        "registered as it was made"
    );
    assert_eq!(heard(notice::PLAY_SEQUENCE), ["play screen"]);
    assert_eq!(
        heard(auberean_ui_game::inventory::notice::INIT_STACK),
        ["stack"]
    );
}
