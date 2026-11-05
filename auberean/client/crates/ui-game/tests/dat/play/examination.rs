//! The examination window and the quest journal on the retail layouts.

use auberean_text::StringInfo;
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Token, Ui};
use auberean_ui_game::examination::{kind, ExamRequest, Line, Profile};
use auberean_ui_game::gameplay::{notice, GamePlay, Notice, Record, Request, View};
use auberean_ui_game::quests::{change, status, Quest, QuestWindow};
use auberean_ui_game::story_quests::{Act, Scene};

use super::dialogs::World;
use auberean_ui_game::toolbar::action as tb;

use crate::common::records;

fn play(ui: &mut Ui) -> GamePlay {
    let mut out = Vec::new();
    let mut p = GamePlay::new(ui, &mut out);
    p.login_finished(ui, &mut out);
    p
}

fn tick(ui: &mut Ui, p: &mut GamePlay, t: &mut f64, notices: &[Notice]) -> Vec<Request> {
    *t += 0.05;
    ui.use_time(*t);
    let mut out = Vec::new();
    p.tick(ui, &View::default(), notices, &mut out);
    out
}

fn key(ui: &mut Ui, p: &mut GamePlay, t: &mut f64, k: u32) -> Vec<Request> {
    ui.key(k, true);
    let mut out = tick(ui, p, t, &[]);
    ui.key(k, false);
    out.extend(tick(ui, p, t, &[]));
    out
}

fn el(ui: &Ui, name: &str) -> Handle {
    ui.element(Token::of(name))
        .unwrap_or_else(|| panic!("{name} made"))
}

fn click(ui: &mut Ui, p: &mut GamePlay, t: &mut f64, h: Handle) -> Vec<Request> {
    let r = ui.screen_rect(h).expect("placed");
    ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
    let mut out = tick(ui, p, t, &[]);
    for down in [true, false] {
        ui.pointer_button(button_id::LEFT, down);
        out.extend(tick(ui, p, t, &[]));
    }
    out
}

fn note(native: u32, words: Vec<u32>, records: Vec<Option<Record>>) -> Notice {
    Notice {
        native,
        about: Some(1),
        words,
        records,
    }
}

fn line(k: u32, tabs: u32, value: u64, text: Option<&str>) -> Line {
    Line {
        kind: k,
        tabs,
        appearance: 0,
        value,
        time: -1.0,
        text: text.map(StringInfo::literal),
        icon: Vec::new(),
        effect: Vec::new(),
    }
}

/// A profile's icon is drawn from its layers on the line, as wide as its picture, and what follows
/// goes after it.
///
/// Behaviour: ui.examination.the-window-lays-out-a-profile-line-by-line
#[test]
fn the_examination_window_draws_an_icon_from_its_layers_and_goes_on_after_it() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    key(&mut ui, &mut p, &mut t, tb::EXAMINATION_PANEL);
    let list = el(&ui, "ExaminationListBox");
    let profile = Profile {
        request: ExamRequest::default(),
        tabs: 0x3FF,
        lines: vec![
            Line {
                icon: vec![(super::stage::ICON, 0xFFFF_FFFF)],
                ..line(kind::ICON, 0x3FF, 0, None)
            },
            line(kind::STRING, 0x3FF, 0, Some("Iron Sword")),
        ],
    };
    let shown = note(
        notice::UPDATE_EXAMINATION_PROFILE,
        vec![0],
        vec![Some(Record::Profile(profile))],
    );
    tick(&mut ui, &mut p, &mut t, &[shown]);
    let rows = ui.list_items(list);
    let children = ui.node(rows[0]).expect("row").children.clone();
    let icon = children
        .iter()
        .copied()
        .find(|c| ui.image_size(*c).is_some())
        .expect("a picture on the line");
    let (w, _) = ui.image_size(icon).expect("its size");
    let name = children
        .iter()
        .find(|c| ui.text(**c).as_deref() == Some("Iron Sword"))
        .and_then(|c| ui.node(*c))
        .expect("the name");
    assert!(w > 0, "as wide as its picture");
    assert_eq!(name.rect.x0, i32::try_from(w).expect("small"), "after it");
}

/// The examination window, shown, comes to the front of the toolbar's windows (another shown
/// later pushes it back one) and asks to examine with every tab (it starts as the classic
/// window) and, hidden, to stop; while a profile is on its way it shows a row saying so; a profile
/// is laid out in one row of its list, line by line: text from the left, a break to a new line, an
/// indent 16 pixels in, the lines of tabs not asked for left out, the row as tall as its lines.
///
/// Behaviour: ui.examination.the-window-lays-out-a-profile-line-by-line
#[test]
fn the_examination_window_lays_out_a_profile_line_by_line() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let out = key(&mut ui, &mut p, &mut t, tb::EXAMINATION_PANEL);
    assert!(
        out.contains(&Request::Examination { tabs: Some(0x3FF) }),
        "{out:?}"
    );
    assert!(ui.shown(el(&ui, "ExaminationField")));
    let z = |ui: &Ui, n: &str| ui.node(el(ui, n)).expect("made").z;
    assert_eq!(
        z(&ui, "ExaminationField"),
        10,
        "the window comes to the front"
    );
    key(&mut ui, &mut p, &mut t, tb::QUEST_PANEL);
    assert_eq!(z(&ui, "QuestField"), 10);
    assert_eq!(z(&ui, "ExaminationField"), 11, "pushed back one");
    key(&mut ui, &mut p, &mut t, tb::QUEST_PANEL);
    let list = el(&ui, "ExaminationListBox");
    let request = ExamRequest {
        kind: 0x4000_0002,
        object: 7,
        tabs: 0x3FF,
        data_enum: 0,
        data_id: 0,
    };
    let changed = note(
        notice::EXAMINATION_REQUEST_CHANGED,
        vec![0],
        vec![Some(Record::Request(request))],
    );
    tick(&mut ui, &mut p, &mut t, &[changed]);
    assert_eq!(ui.list_items(list).len(), 1, "the waiting row");
    let profile = Profile {
        request,
        tabs: 0x203 | 0x400,
        lines: vec![
            line(kind::STRING, 0x203, 0, Some("Iron Sword")),
            line(kind::BREAK, 0x203, 0, None),
            line(kind::INDENT, 0x203, 1, None),
            line(kind::INT, 0x203, 25, None),
            line(kind::STRING, 0x400, 0, Some("left out")),
        ],
    };
    let shown = note(
        notice::UPDATE_EXAMINATION_PROFILE,
        vec![0],
        vec![Some(Record::Profile(profile))],
    );
    tick(&mut ui, &mut p, &mut t, &[shown]);
    let rows = ui.list_items(list);
    assert_eq!(rows.len(), 1, "one row");
    let row = rows[0];
    let parts: Vec<(String, i32, i32)> = ui
        .node(row)
        .expect("row")
        .children
        .iter()
        .filter_map(|c| {
            let r = ui.node(*c)?.rect;
            Some((ui.text(*c).unwrap_or_default(), r.x0, r.y0))
        })
        .collect();
    let name = parts
        .iter()
        .find(|(s, ..)| s == "Iron Sword")
        .expect("the name");
    assert_eq!((name.1, name.2), (0, 0), "{parts:?}");
    let number = parts
        .iter()
        .find(|(s, ..)| s.contains("25"))
        .expect("the number");
    assert_eq!(number.1, 16, "indented one: {parts:?}");
    assert!(number.2 >= 16, "on the next line: {parts:?}");
    assert!(!parts.iter().any(|(s, ..)| s == "left out"));
    let height = ui.node(row).expect("row").rect.height();
    assert!(height >= number.2 + 16, "{height}");
    let out = key(&mut ui, &mut p, &mut t, tb::EXAMINATION_PANEL);
    assert!(
        out.contains(&Request::Examination { tabs: None }),
        "{out:?}"
    );
}

/// A profile's line whose string reference reads a string table the data lacks is laid out
/// empty, taking the rest of its row (here on a new line, the row holding the name); when the table comes the window empties to its waiting row
/// until the next profile, which shows the line's text.
///
/// Behaviour: ui.examination.a-line-waiting-on-its-string-table-empties-the-window-when-it-comes
#[test]
fn a_line_waiting_on_its_string_table_empties_the_window_when_it_comes() {
    use super::stage::{Lacking, World, LACKING};
    let assets = Lacking::new();
    let mut w = World::over(assets.clone());
    w.ui.key(tb::EXAMINATION_PANEL, true);
    w.tick(&[]);
    w.ui.key(tb::EXAMINATION_PANEL, false);
    w.tick(&[]);
    let list = w.el("ExaminationListBox");
    let request = ExamRequest {
        kind: 0x4000_0002,
        object: 7,
        tabs: 0x3FF,
        data_enum: 0,
        data_id: 0,
    };
    w.tick(&[note(
        notice::EXAMINATION_REQUEST_CHANGED,
        vec![0],
        vec![Some(Record::Request(request))],
    )]);
    let mut from_server = line(kind::STRING, 0x203, 0, None);
    from_server.text = Some(StringInfo::entry(LACKING, 0x1234));
    let profile = Profile {
        request,
        tabs: 0x203,
        lines: vec![line(kind::STRING, 0x203, 0, Some("Sword")), from_server],
    };
    let shown = note(
        notice::UPDATE_EXAMINATION_PROFILE,
        vec![0],
        vec![Some(Record::Profile(profile))],
    );
    w.tick(std::slice::from_ref(&shown));
    let rows = w.ui.list_items(list);
    assert_eq!(rows.len(), 1, "the profile's row");
    let row = rows[0];
    let row_w = w.ui.node(row).expect("row").rect.width();
    let parts: Vec<(String, i32, i32, i32)> =
        w.ui.node(row)
            .expect("row")
            .children
            .iter()
            .filter_map(|c| {
                let r = w.ui.node(*c)?.rect;
                Some((w.ui.text(*c).unwrap_or_default(), r.x0, r.y0, r.width()))
            })
            .collect();
    assert_eq!(parts.len(), 2, "{parts:?}");
    let sword = parts.iter().find(|p| p.0 == "Sword").expect("the name");
    let waiting = parts
        .iter()
        .find(|p| p.0.is_empty())
        .expect("the line waiting");
    assert!(waiting.2 > sword.2, "on a new line: {parts:?}");
    assert_eq!(
        (waiting.1, waiting.3),
        (0, row_w),
        "the whole row: {parts:?}"
    );
    assets.arrive();
    w.tick(&[]);
    w.tick(&[]);
    let rows = w.ui.list_items(list);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        w.ui.node(rows[0]).expect("row").id,
        Token::of("ExaminationPendingRow"),
        "back to the waiting row"
    );
    w.tick(&[shown]);
    let rows = w.ui.list_items(list);
    let texts: Vec<String> =
        w.ui.node(rows[0])
            .expect("row")
            .children
            .iter()
            .map(|c| w.ui.text(*c).unwrap_or_default())
            .collect();
    assert!(
        texts.iter().any(|t| t.starts_with("From the server")),
        "{texts:?}"
    );
}

/// The player's options can make the examination window the tabbed one: it asks for the first
/// tab and shows its tab buttons, each active (asked for), inactive (lines made for it) or
/// disabled (none); pressing an inactive tab asks to examine with that tab's tabs.
///
/// Behaviour: ui.examination.the-tabbed-window-asks-for-the-tab-pressed
#[test]
fn the_tabbed_window_asks_for_the_tab_pressed() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    key(&mut ui, &mut p, &mut t, tb::EXAMINATION_PANEL);
    let options = note(
        notice::RELOAD_OPTIONS,
        vec![],
        vec![Some(Record::OptionBits(0))],
    );
    let out = tick(&mut ui, &mut p, &mut t, &[options]);
    assert!(
        out.contains(&Request::Examination { tabs: Some(0x203) }),
        "{out:?}"
    );
    let internal = el(&ui, "ExaminationInternal");
    assert_eq!(ui.node(internal).expect("made").state, Token::of("tabbed"));
    let request = ExamRequest {
        kind: 0x4000_0002,
        object: 7,
        tabs: 0x203,
        data_enum: 0,
        data_id: 0,
    };
    let profile = Profile {
        request,
        tabs: 0x203 | 0x10,
        lines: vec![line(kind::STRING, 0x203, 0, Some("Iron Sword"))],
    };
    let shown = note(
        notice::UPDATE_EXAMINATION_PROFILE,
        vec![0],
        vec![Some(Record::Profile(profile))],
    );
    tick(&mut ui, &mut p, &mut t, &[shown]);
    let state = |ui: &Ui, n: &str| ui.node(el(ui, n)).expect("made").state;
    assert_eq!(state(&ui, "GeneralTab"), Token::of("active"));
    assert_eq!(state(&ui, "EffectsTab"), Token::of("inactive"));
    assert_eq!(state(&ui, "DetailsTab"), Token::of("disabled"));
    assert!(ui.shown(el(&ui, "EffectsTab")));
    let effects = el(&ui, "EffectsTab");
    let out = click(&mut ui, &mut p, &mut t, effects);
    assert!(
        out.contains(&Request::Examination { tabs: Some(0x10) }),
        "{out:?}"
    );
}

fn quest(id: u32, st: u32, given: f64, name: &str) -> Quest {
    Quest {
        id,
        name: StringInfo::literal(name),
        status: st,
        given,
        cancellable: true,
        ..Quest::default()
    }
}

/// The journal reads the player's quests at the session's start: those under way on their page
/// and those finished on theirs, newest given first, each row named; when a quest ends it moves to
/// the finished page, open (its row and its text), and the window's button shows the change while
/// the window is hidden.
///
/// Behaviour: ui.quests.the-journal-keeps-quests-under-way-and-finished
#[test]
fn the_journal_keeps_quests_under_way_and_finished() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let all = vec![
        quest(1, status::UNDER_WAY, 20.0, "Under Way, Older"),
        quest(2, status::COMPLETED, 10.0, "Done"),
        quest(3, status::UNDER_WAY, 30.0, "Under Way, Newer"),
    ];
    let start = note(
        notice::UPDATE_QUESTS,
        vec![change::ALL, 0],
        vec![None, Some(Record::Quests(all))],
    );
    tick(&mut ui, &mut p, &mut t, &[start]);
    tick(&mut ui, &mut p, &mut t, &[]);
    let underway = el(&ui, "UnderwayQuestListBox");
    let finished = el(&ui, "FinishedQuestListBox");
    let names = |ui: &Ui, list| -> Vec<String> {
        ui.list_items(list)
            .iter()
            .filter_map(|r| ui.child_recursive(*r, Token::of("QuestName")))
            .map(|n| ui.text(n).unwrap_or_default())
            .collect()
    };
    assert_eq!(
        names(&ui, underway),
        vec!["Under Way, Newer", "Under Way, Older"]
    );
    assert_eq!(names(&ui, finished), vec!["Done"]);
    let ended = note(
        notice::UPDATE_QUESTS,
        vec![change::ONE, 0],
        vec![
            None,
            Some(Record::Quest(quest(
                1,
                status::COMPLETED,
                20.0,
                "Under Way, Older",
            ))),
        ],
    );
    tick(&mut ui, &mut p, &mut t, &[ended]);
    tick(&mut ui, &mut p, &mut t, &[]);
    assert_eq!(names(&ui, underway), vec!["Under Way, Newer"]);
    assert_eq!(names(&ui, finished), vec!["Under Way, Older", "Done"]);
    assert_eq!(
        ui.list_items(finished).len(),
        3,
        "the ended quest open: its text row"
    );
    let button = el(&ui, "QuestButton");
    assert_eq!(ui.node(button).expect("made").state, Token::of("changed"));
}

/// A quest that changes has its row's name and time marked in the changed look; the others keep
/// theirs. The whole list read at the session's start marks nothing.
///
/// Behaviour: ui.quests.a-quest-that-changed-is-marked-in-its-row
#[test]
fn a_quest_that_changed_is_marked_in_its_row() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let all = vec![
        quest(1, status::UNDER_WAY, 20.0, "Changes"),
        quest(2, status::UNDER_WAY, 10.0, "Stays"),
    ];
    let start = note(
        notice::UPDATE_QUESTS,
        vec![change::ALL, 0],
        vec![None, Some(Record::Quests(all))],
    );
    tick(&mut ui, &mut p, &mut t, &[start]);
    tick(&mut ui, &mut p, &mut t, &[]);
    let underway = el(&ui, "UnderwayQuestListBox");
    let looks = |ui: &Ui, name: &str| -> Vec<(String, Token)> {
        ui.list_items(underway)
            .iter()
            .filter_map(|r| {
                let n = ui.child_recursive(*r, Token::of("QuestName"))?;
                let e = ui.child_recursive(*r, Token::of(name))?;
                Some((ui.text(n).unwrap_or_default(), ui.node(e)?.state))
            })
            .collect()
    };
    let changed = Token::of("changed");
    for field in ["QuestName", "QuestTime"] {
        assert!(
            looks(&ui, field).iter().all(|(_, s)| *s != changed),
            "the list read marks no {field}"
        );
    }
    let update = note(
        notice::UPDATE_QUESTS,
        vec![change::ONE, 0],
        vec![
            None,
            Some(Record::Quest(quest(1, status::UNDER_WAY, 20.0, "Changes"))),
        ],
    );
    tick(&mut ui, &mut p, &mut t, &[update]);
    tick(&mut ui, &mut p, &mut t, &[]);
    for field in ["QuestName", "QuestTime"] {
        let rows = looks(&ui, field);
        let state = |name: &str| rows.iter().find(|(n, _)| n == name).map(|(_, s)| *s);
        assert_eq!(
            state("Changes"),
            Some(changed),
            "the changed quest's {field}"
        );
        assert_ne!(state("Stays"), Some(changed), "the other quest's {field}");
    }
}

/// The quest window's button opens it and stays on the toolbar: the window shown, the button takes
/// its unchanged look (a change it showed is seen) and with it the face it showed before; pressed
/// again it hides the window.
///
/// Behaviour: ui.quests.the-quest-button-keeps-its-look-as-it-opens-the-window
#[test]
fn the_quest_button_keeps_its_look_as_it_opens_the_window() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    tick(&mut ui, &mut p, &mut t, &[]);
    let button = el(&ui, "QuestButton");
    // The button's face: the button its state brings, and the picture that shows.
    let face = |ui: &Ui| {
        let n = ui.node(button).expect("made");
        n.children
            .iter()
            .filter_map(|c| ui.node(*c))
            .map(|c| (c.kind, c.picture))
            .collect::<Vec<_>>()
    };
    let before = face(&ui);
    assert!(
        before.iter().any(|(_, p)| p.is_some()),
        "the button shows a picture: {before:?}"
    );
    let changed = note(
        notice::UPDATE_QUESTS,
        vec![change::ALL, 0],
        vec![
            None,
            Some(Record::Quests(vec![quest(
                1,
                status::UNDER_WAY,
                1.0,
                "Under Way",
            )])),
        ],
    );
    tick(&mut ui, &mut p, &mut t, &[changed]);
    click(&mut ui, &mut p, &mut t, button);
    assert!(ui.shown(el(&ui, "QuestField")), "the window shows");
    assert_eq!(ui.node(button).expect("made").state, Token::of("unchanged"));
    ui.pointer_move(512, 300);
    tick(&mut ui, &mut p, &mut t, &[]);
    assert_eq!(face(&ui), before, "the button shows the face it showed");
    click(&mut ui, &mut p, &mut t, button);
    assert!(!ui.shown(el(&ui, "QuestField")), "pressed again it hides");
}

fn scene(id: u32, name: &str, hidden: bool, playable: bool) -> Scene {
    Scene {
        id,
        name: StringInfo::literal(name),
        hidden,
        playable,
    }
}

/// As the player enters the world the scripts give the journal its whole list and the story its
/// whole story, and the window stands on the quests under way. A scene added to the story with the
/// window hidden, as a new character's first entry adds its starting scenes, turns it to the story.
///
/// Behaviour: ui.quests.the-quest-window-stands-on-the-quests-under-way-until-a-scene-is-added
#[test]
fn the_quest_window_stands_on_the_quests_under_way_until_a_scene_is_added() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let state = |ui: &Ui, n: &str| ui.node(el(ui, n)).expect("node").state;
    let list = note(
        notice::UPDATE_QUESTS,
        vec![change::ALL, 0],
        vec![
            None,
            Some(Record::Quests(vec![quest(
                1,
                status::UNDER_WAY,
                5.0,
                "One",
            )])),
        ],
    );
    let story = |how: u32, act: u32| Notice {
        native: notice::UPDATE_STORY_QUESTS,
        about: Some(1),
        words: vec![how, act],
        records: Vec::new(),
    };
    tick(&mut ui, &mut p, &mut t, &[list, story(0x4000_0002, 0)]);
    tick(&mut ui, &mut p, &mut t, &[]);
    assert_eq!(state(&ui, "UnderwayTab"), Token::of("open"));
    assert_eq!(state(&ui, "StoryTab"), Token::of("closed"));
    tick(&mut ui, &mut p, &mut t, &[story(0x4000_0001, 2)]);
    tick(&mut ui, &mut p, &mut t, &[]);
    assert_eq!(state(&ui, "StoryTab"), Token::of("open"));
    assert_eq!(state(&ui, "UnderwayTab"), Token::of("closed"));
    assert_eq!(state(&ui, "QuestButton"), Token::of("changed"));
}

/// The story page lays out the visible acts, in order, each with its scenes' icons (the hidden
/// ones left out), completed when the scene may be played again, else not; an act the scripts say
/// changed while the page is not shown marks the window's button, is open (its scenes' rows listed)
/// and its name marked; pressing an act's row opens or closes it, and a completed scene's row
/// pressed twice plays it again.
///
/// Behaviour: ui.quests.the-story-page-lists-the-acts-and-opens-the-one-changed
#[test]
fn the_story_page_lists_the_acts_and_opens_the_one_changed() {
    let mut w = World::new();
    w.acts = Some(vec![
        Act {
            id: 1,
            name: StringInfo::literal("Act One"),
            visible: true,
            scenes: vec![scene(11, "Arrival", false, true)],
        },
        Act {
            id: 2,
            name: StringInfo::literal("Act Two"),
            visible: true,
            scenes: vec![
                scene(21, "The Gate", false, true),
                scene(22, "Unseen", true, false),
                scene(23, "The Hall", false, false),
            ],
        },
        Act {
            id: 3,
            name: StringInfo::literal("Not Yet"),
            visible: false,
            scenes: Vec::new(),
        },
    ]);
    let changed = Notice {
        native: notice::UPDATE_STORY_QUESTS,
        about: Some(1),
        words: vec![0x4000_0001, 2],
        records: Vec::new(),
    };
    w.tick(&[changed]);
    w.tick(&[]);
    let button = w.ui.element(Token::of("QuestButton")).expect("button");
    assert_eq!(w.ui.node(button).expect("node").state, Token::of("changed"));
    let list = w.ui.element(Token::of("ActListBox")).expect("the acts");
    let rows = w.ui.list_items(list);
    let named = |ui: &Ui, h: Handle, n: &str| {
        ui.child_recursive(h, Token::of(n))
            .and_then(|t| ui.text(t))
            .unwrap_or_default()
    };
    let shown: Vec<String> = rows
        .iter()
        .map(|r| {
            let act = named(&w.ui, *r, "ActName");
            if act.is_empty() {
                named(&w.ui, *r, "SceneName")
            } else {
                act
            }
        })
        .collect();
    assert_eq!(shown, vec!["Act One", "Act Two", "The Gate", "The Hall"]);
    let state = |ui: &Ui, h: Handle| ui.node(h).expect("node").state;
    assert_eq!(state(&w.ui, rows[0]), Token::of("normal"));
    assert_eq!(state(&w.ui, rows[1]), Token::of("highlight"));
    let act_name =
        w.ui.child_recursive(rows[1], Token::of("ActName"))
            .expect("name");
    assert_eq!(state(&w.ui, act_name), Token::of("changed"));
    let icons =
        w.ui.child_recursive(rows[1], Token::of("SceneIconListBox"))
            .expect("icons");
    let looks: Vec<Token> =
        w.ui.list_items(icons)
            .iter()
            .map(|i| state(&w.ui, *i))
            .collect();
    assert_eq!(
        looks,
        vec![Token::of("completed"), Token::of("notcompleted")],
        "the hidden scene left out"
    );

    // Shown: pressing the first act opens it; a completed scene pressed twice plays again.
    w.key(tb::QUEST_PANEL);
    QuestWindow::set_active_panel(&mut w.ui, Token::of("StoryTab"));
    w.tick(&[]);
    let rows = w.ui.list_items(list);
    w.click(rows[0]);
    w.tick(&[]);
    let rows = w.ui.list_items(list);
    assert_eq!(state(&w.ui, rows[0]), Token::of("highlight"));
    let gate = rows
        .iter()
        .copied()
        .find(|r| named(&w.ui, *r, "SceneName") == "The Gate")
        .expect("the gate's row");
    let r = w.ui.screen_rect(gate).expect("placed");
    w.ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
    w.tick(&[]);
    let mut out = Vec::new();
    // The second press of a double press comes as the double button.
    for b in [button_id::LEFT, button_id::DOUBLE_LEFT] {
        w.ui.pointer_button(b, true);
        out.extend(w.tick(&[]));
        w.ui.pointer_button(b, false);
        out.extend(w.tick(&[]));
    }
    assert!(
        out.contains(&Request::PlayScene { act: 2, scene: 21 }),
        "{out:?}"
    );
}

/// An effect line of a profile lays out the effect's own lines in its place, the effects among
/// them left out, as the window's other lines are laid out.
///
/// Behaviour: ui.examination.an-effect-line-lays-out-the-effects-own-lines
#[test]
fn an_effect_line_lays_out_the_effects_own_lines() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    key(&mut ui, &mut p, &mut t, tb::EXAMINATION_PANEL);
    let list = el(&ui, "ExaminationListBox");
    let request = ExamRequest {
        kind: 0x4000_0002,
        object: 7,
        tabs: 0x3FF,
        ..ExamRequest::default()
    };
    let mut effect = line(kind::EFFECT, 0x10, 0x51, None);
    effect.effect = vec![
        line(kind::STRING, 0x10, 0, Some("Swiftness")),
        line(kind::BREAK, 0x10, 0, None),
        line(kind::EFFECT, 0x10, 0x52, None),
        line(kind::STRING, 0x10, 0, Some("Run faster")),
    ];
    let profile = Profile {
        request,
        tabs: 0x203 | 0x10,
        lines: vec![
            line(kind::STRING, 0x203, 0, Some("Boots")),
            line(kind::BREAK, 0x203, 0, None),
            effect,
        ],
    };
    let shown = note(
        notice::UPDATE_EXAMINATION_PROFILE,
        vec![0],
        vec![Some(Record::Profile(profile))],
    );
    tick(&mut ui, &mut p, &mut t, &[shown]);
    let rows = ui.list_items(list);
    assert_eq!(rows.len(), 1, "one row");
    let mut parts: Vec<(String, i32, i32)> = ui
        .node(rows[0])
        .expect("row")
        .children
        .iter()
        .filter_map(|c| {
            let r = ui.node(*c)?.rect;
            Some((ui.text(*c).unwrap_or_default(), r.y0, r.x0))
        })
        .collect();
    parts.sort_by_key(|(_, y, x)| (*y, *x));
    let texts: Vec<&str> = parts.iter().map(|(s, ..)| s.as_str()).collect();
    assert_eq!(texts, ["Boots", "Swiftness", "Run faster"], "{parts:?}");
    assert!(
        parts[1].1 > parts[0].1 && parts[2].1 > parts[1].1,
        "{parts:?}"
    );
}

/// One frame at exactly `now`.
fn at(ui: &mut Ui, p: &mut GamePlay, now: f64, notices: &[Notice]) {
    ui.use_time(now);
    p.tick(ui, &View::default(), notices, &mut Vec::new());
}

/// A quest under way with a time left is written in the string table's words for how much is
/// left (above a month, months and days; above a day, days and hours; above sixty minutes, hours
/// and minutes; else minutes and seconds), and the clock keeps the words current: each looked at
/// again when its smallest unit runs out, until the time is out.
///
/// Behaviour: ui.quests.a-quests-time-left-is-written-in-words-the-clock-keeps-current
#[test]
fn a_quests_time_left_is_written_in_words_the_clock_keeps_current() {
    const DAY: i32 = 86_400;
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let left = [
        (1, 2 * 31 * DAY + 3 * DAY + 7),
        (2, 2 * DAY + 3 * 3600 + 5),
        (3, DAY),
        (4, 3661),
        (5, 3600),
        (6, 65),
    ];
    let all = left
        .iter()
        .map(|&(id, s)| Quest {
            seconds_remaining: s,
            ..quest(id, status::UNDER_WAY, f64::from(id), &format!("Q{id}"))
        })
        .collect();
    let start = note(
        notice::UPDATE_QUESTS,
        vec![change::ALL, 0],
        vec![None, Some(Record::Quests(all))],
    );
    at(&mut ui, &mut p, 1000.0, &[start]);
    at(&mut ui, &mut p, 1000.0, &[]);
    let underway = el(&ui, "UnderwayQuestListBox");
    let times = |ui: &Ui| -> Vec<String> {
        let mut rows: Vec<(String, String)> = ui
            .list_items(underway)
            .iter()
            .filter_map(|r| {
                let name = ui.text(ui.child_recursive(*r, Token::of("QuestName"))?)?;
                let time = ui.text(ui.child_recursive(*r, Token::of("QuestTime"))?)?;
                Some((name, time))
            })
            .collect();
        rows.sort();
        rows.into_iter().map(|(_, t)| t).collect()
    };
    assert_eq!(
        times(&ui),
        [
            "2 months, 3 days",
            "2 days, 3 hours",
            "24:00",
            "1:01",
            "60:00",
            "1:05"
        ]
    );
    at(&mut ui, &mut p, 1002.0, &[]);
    assert_eq!(
        times(&ui),
        [
            "2 months, 3 days",
            "2 days, 3 hours",
            "23:59",
            "60:59",
            "59:58",
            "1:03"
        ],
        "two seconds on"
    );
    at(&mut ui, &mut p, 1006.0, &[]);
    assert_eq!(
        times(&ui),
        [
            "2 months, 3 days",
            "2 days, 2 hours",
            "23:59",
            "60:55",
            "59:54",
            "0:59"
        ],
        "six seconds on: the days' hour runs out"
    );
    at(&mut ui, &mut p, 1065.0, &[]);
    assert_eq!(times(&ui)[5], "0:00", "the time out");
    at(&mut ui, &mut p, 1200.0, &[]);
    assert_eq!(times(&ui)[5], "0:00", "no longer looked at once out");
}

/// A countdown line of a profile shows the time left until its end in the string table's words,
/// as the journal's quests do, kept current by the clock; the next profile takes it away.
///
/// Behaviour: ui.examination.a-countdown-shows-the-time-left-the-clock-keeps-current
#[test]
fn a_countdown_shows_the_time_left_the_clock_keeps_current() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    key(&mut ui, &mut p, &mut t, tb::EXAMINATION_PANEL);
    let list = el(&ui, "ExaminationListBox");
    let request = ExamRequest {
        kind: 0x4000_0002,
        object: 7,
        tabs: 0x3FF,
        data_enum: 0,
        data_id: 0,
    };
    let changed = note(
        notice::EXAMINATION_REQUEST_CHANGED,
        vec![0],
        vec![Some(Record::Request(request))],
    );
    at(&mut ui, &mut p, 2000.0, &[changed]);
    let countdown = |end: f64| Line {
        time: end,
        ..line(kind::COUNTDOWN, 0x3FF, 0, None)
    };
    let profile = |lines| {
        note(
            notice::UPDATE_EXAMINATION_PROFILE,
            vec![0],
            vec![Some(Record::Profile(Profile {
                request,
                tabs: 0x3FF,
                lines,
            }))],
        )
    };
    let shown = profile(vec![
        line(kind::STRING, 0x3FF, 0, Some("Haste")),
        countdown(2000.0 + 3661.0),
        line(kind::BREAK, 0x3FF, 0, None),
        countdown(2000.0 + 2.0 * 86_400.0 + 5.0),
    ]);
    at(&mut ui, &mut p, 2000.0, &[shown]);
    // The row's words in the order laid out.
    let texts = |ui: &Ui| -> Vec<String> {
        let rows = ui.list_items(list);
        let mut parts: Vec<(i32, i32, String)> = ui
            .node(rows[0])
            .expect("row")
            .children
            .iter()
            .filter_map(|c| {
                let r = ui.node(*c)?.rect;
                Some((r.y0, r.x0, ui.text(*c)?))
            })
            .collect();
        parts.sort();
        parts.into_iter().map(|(.., s)| s).collect()
    };
    assert_eq!(texts(&ui), ["Haste", "1:01", "2 days, 0 hours"]);
    at(&mut ui, &mut p, 2002.0, &[]);
    assert_eq!(
        texts(&ui),
        ["Haste", "60:59", "2 days, 0 hours"],
        "two seconds on"
    );
    at(&mut ui, &mut p, 2006.0, &[]);
    assert_eq!(
        texts(&ui),
        ["Haste", "60:55", "47:59"],
        "six seconds on: a day or less is hours and minutes"
    );
    at(
        &mut ui,
        &mut p,
        2006.0,
        &[profile(vec![line(kind::STRING, 0x3FF, 0, Some("Slow"))])],
    );
    assert_eq!(
        texts(&ui),
        ["Slow"],
        "the next profile takes the countdowns away"
    );
}
