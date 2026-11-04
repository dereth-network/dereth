//! The skill window's trees on the retail layouts: a panel laid out from the scripts' answers,
//! a skill selected, trained through its question.

use std::collections::BTreeMap;

use auberean_text::StringInfo;
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Token, Ui};
use auberean_ui_game::dialog::kind;
use auberean_ui_game::examination::ExamRequest;
use auberean_ui_game::gameplay::{notice, Examination, Notice, Record, Request};
use auberean_ui_game::skill_tree::{
    node_kind, Facts, Node, SkillRequest, State, BACK, EXAMINE_PANEL, EXAMINE_SKILL,
};
use auberean_ui_game::toolbar::action as tb;

use super::dialogs::World;

const MELEE: u32 = 0x8000_0017;
const STANCES: u32 = 0x8000_0020;

fn node(skill: u32, k: u32, at: (i32, i32), parents: &[u32], name: &str) -> Node {
    Node {
        skill,
        kind: k,
        name: StringInfo::literal(name),
        icon: 0x0600_1000 + skill,
        x: at.0,
        y: at.1,
        parents: parents.to_vec(),
        dependencies: vec![skill],
        ..Node::default()
    }
}

/// A melee panel: a trained skill that may be raised (buffed by 2), one hanging from it trained
/// and not, one below that not trained and trainable (its training cannot be undone, so it
/// asks), a node opening the stances' panel.
fn melee() -> Facts {
    let mut states = BTreeMap::new();
    states.insert(
        0xA,
        State {
            trained: true,
            base_level: 48,
            level: 50,
            experience_to_raise: 1200,
            can_train: 1,
            ..State::default()
        },
    );
    states.insert(
        0x9,
        State {
            trained: true,
            base_level: 12,
            level: 12,
            can_raise: 30_011,
            can_train: 1,
            ..State::default()
        },
    );
    states.insert(
        0xB,
        State {
            cost_to_train: 5,
            can_raise: 1,
            ..State::default()
        },
    );
    let mut panels = BTreeMap::new();
    panels.insert(MELEE, (StringInfo::literal("Melee"), 0x0600_2000));
    panels.insert(STANCES, (StringInfo::literal("Stances"), 0x0600_2001));
    Facts {
        version: 1,
        panel: MELEE,
        nodes: vec![
            node(0xA, node_kind::SKILL, (0, 0), &[], "Strike"),
            node(0x9, node_kind::SKILL, (0, 1), &[0xA], "Hew"),
            node(0xB, node_kind::SKILL, (0, 2), &[0x9], "Cleave"),
            node(STANCES, node_kind::PLUS, (1, 0), &[], "Stances"),
        ],
        panels,
        states,
        roots: [Some(MELEE), None, Some(0x8000_0016), None],
        experience: 9000,
        credits: 7,
        hero_credits: 0,
    }
}

fn request_changed(kind: u32, data: u32) -> Notice {
    Notice {
        native: notice::EXAMINATION_REQUEST_CHANGED,
        about: Some(1),
        words: vec![0],
        records: vec![Some(Record::Request(ExamRequest {
            kind,
            data_enum: data,
            ..ExamRequest::default()
        }))],
    }
}

fn item(ui: &Ui, x: i32, y: i32) -> Handle {
    let tree = ui.element(Token::of("SkillTree")).expect("the tree");
    ui.tree_item(tree, x, y)
        .unwrap_or_else(|| panic!("a node at ({x}, {y})"))
}

fn look(ui: &Ui, h: Handle, child: &str) -> Token {
    let c = ui.child_recursive(h, Token::of(child)).expect(child);
    ui.node(c).expect("node").state
}

fn ghosted(ui: &Ui, name: &str) -> bool {
    let f = ui.element(Token::of("ModifyField")).expect("the foot");
    let b = ui.child_recursive(f, Token::of(name)).expect(name);
    ui.node(b)
        .expect("node")
        .bool_attr(auberean_ui::token::attr::GHOSTED)
}

/// Press an element with the left button, a frame for each step.
fn press(w: &mut World, h: Handle) -> Vec<Request> {
    let r = w.ui.screen_rect(h).expect("placed");
    w.ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
    let mut out = w.tick(&[]);
    w.ui.pointer_button(button_id::LEFT, true);
    out.extend(w.tick(&[]));
    w.ui.pointer_button(button_id::LEFT, false);
    out.extend(w.tick(&[]));
    out
}

/// The skill tree's scroll bar, by the name the tree gives it, and its thumb.
fn tree_scroll(ui: &Ui) -> (Handle, Handle) {
    let tree = ui.element(Token::of("SkillTree")).expect("the tree");
    let id = ui
        .node(tree)
        .and_then(|n| n.id_attr(auberean_ui::token::attr::VERTICAL_SCROLLBAR))
        .expect("the tree names its scroll bar");
    let window = ui.element(Token::of("SkillField")).expect("window");
    let bar = ui
        .child_recursive(window, Token(id))
        .expect("the scroll bar");
    let thumb = ui
        .child(bar, auberean_ui::token::attr::WIDGET)
        .expect("its thumb");
    (bar, thumb)
}

/// With the whole tree in view the tree's scroll bar is disabled and shows no thumb, only its
/// track and arrows, from the moment the window first shows.
///
/// Behaviour: ui.skills.the-trees-scroll-bar-shows-no-thumb-while-the-whole-tree-shows
#[test]
fn the_trees_scroll_bar_shows_no_thumb_while_the_whole_tree_shows() {
    let mut w = World::new();
    w.key(tb::SKILL_PANEL);
    let (bar, thumb) = tree_scroll(&w.ui);
    assert!(w.ui.shown(bar), "the scroll bar shows");
    assert!(!w.ui.shown(thumb), "its thumb does not");
    w.skills = Some(melee());
    w.tick(&[request_changed(EXAMINE_PANEL, MELEE)]);
    let (bar, thumb) = tree_scroll(&w.ui);
    assert!(
        w.ui.shown(bar) && !w.ui.shown(thumb),
        "nor with a panel that fits"
    );
}

/// The panel the examination asks for is laid out as a tree, each node at its place and hung
/// from its parent: a trained skill shows its level (marked when a buff moves it) and whether it
/// may be raised, one untrained whether it may be trained; the root buttons follow the race's
/// root panels. Pressing a node asks the examination for it; the examination's skill selects it,
/// and the foot shows what training costs and the skill credits; training a skill whose training
/// cannot be undone asks first, and yes trains it. A node opening a panel loads it, the panel left
/// on the history the back node returns to.
///
/// Behaviour: ui.skills.a-panel-is-laid-out-as-a-tree-and-a-skill-trained-through-its-question
#[test]
fn a_panel_is_laid_out_as_a_tree_and_a_skill_trained_through_its_question() {
    let mut w = World::new();
    w.skills = Some(melee());
    w.key(tb::SKILL_PANEL);
    w.tick(&[request_changed(EXAMINE_PANEL, MELEE)]);
    let ui = &w.ui;
    let (strike, hew, cleave) = (item(ui, 0, 0), item(ui, 0, 1), item(ui, 0, 2));
    item(ui, 1, 0);
    assert_eq!(look(ui, strike, "IconArea"), Token::of("canraise"));
    assert_eq!(look(ui, hew, "IconArea"), Token::of("stateless"));
    assert_eq!(look(ui, cleave, "IconArea"), Token::of("cantrain"));
    let level = ui
        .child(strike, Token::of("SkillNodeLevelText"))
        .expect("level");
    assert!(
        ui.text(level).unwrap_or_default().contains("50"),
        "{:?}",
        ui.text(level)
    );
    assert_eq!(ui.node(level).expect("node").state, Token::of("buff"));
    let field = ui.element(Token::of("SkillField")).expect("window");
    let shown = |n: &str| ui.shown(ui.child_recursive(field, Token::of(n)).expect(n));
    assert!(shown("MeleeButton") && !shown("MeleeButton_disabled"));
    assert!(!shown("MissileButton") && shown("MissileButton_disabled"));
    let foot = ui.element(Token::of("ModifyField")).expect("foot");
    assert_eq!(ui.node(foot).expect("node").state, Token::of("panel"));

    // Pressing a node asks the examination for it.
    let icon = ui
        .child_recursive(cleave, Token::of("SkillNodeIcon"))
        .expect("icon");
    let out = press(&mut w, icon);
    let asked = Request::Examine(Examination {
        kind: EXAMINE_SKILL,
        data_enum: 0xB,
        ..Examination::default()
    });
    assert!(out.contains(&asked), "{out:?}");
    assert!(out.contains(&Request::OpenExaminationPanel), "{out:?}");
    w.tick(&[request_changed(EXAMINE_SKILL, 0xB)]);
    let ui = &w.ui;
    assert_eq!(ui.node(foot).expect("node").state, Token::of("untrained"));
    assert_eq!(look(ui, cleave, "Background"), Token::of("selected"));
    assert!(!ghosted(ui, "TrainButton"));
    let misc = ui.element(Token::of("MiscText")).expect("misc");
    let misc_text = ui.text(misc).unwrap_or_default();
    assert!(
        misc_text.contains('7') && misc_text.contains('5'),
        "{misc_text}"
    );
    assert_eq!(w.text_of("SelectedName"), "Cleave");

    // Training asks, and yes trains.
    let train = w.foot_button("TrainButton");
    let out = w.click(train);
    assert!(
        out.iter()
            .any(|r| matches!(r, Request::Dialog(d) if d.kind == kind::CONFIRMATION)),
        "{out:?}"
    );
    let out = w.click(w.button("YesButton"));
    assert!(
        out.contains(&Request::Skill(SkillRequest::Train(0xB))),
        "{out:?}"
    );

    // Another panel: the melee panel goes on the history, and the back node returns to it.
    let mut stances = melee();
    stances.version = 2;
    stances.panel = STANCES;
    stances.nodes = vec![
        node(BACK, node_kind::MINUS, (0, 0), &[], "Back"),
        node(0x20, node_kind::SKILL, (0, 1), &[BACK], "Low Guard"),
    ];
    w.skills = Some(stances);
    w.tick(&[request_changed(EXAMINE_PANEL, STANCES)]);
    let tree = w.play_tree();
    assert_eq!(tree, (STANCES, 0));
    let back = item(&w.ui, 0, 0);
    let icon =
        w.ui.child_recursive(back, Token::of("SkillNodeIcon"))
            .expect("icon");
    let out = press(&mut w, icon);
    let asked = Request::Examine(Examination {
        kind: EXAMINE_PANEL,
        data_enum: MELEE,
        ..Examination::default()
    });
    assert!(out.contains(&asked), "{out:?}");
}

/// The scripts' notice of the player's points carries the experience to spend as the script machine
/// holds a 64-bit number below a notice's receiver, its high word first: the foot shows the number,
/// with both words in it, not its words swapped.
///
/// Behaviour: ui.skills.the-foot-shows-the-experience-the-points-notice-carries
#[test]
fn the_foot_shows_the_experience_the_points_notice_carries() {
    let mut w = World::new();
    w.skills = Some(melee());
    w.key(tb::SKILL_PANEL);
    w.tick(&[request_changed(EXAMINE_PANEL, MELEE)]);
    w.tick(&[request_changed(EXAMINE_SKILL, 0xA)]);
    const EXPERIENCE: u64 = 0x1_0000_0119;
    let points = Notice {
        native: notice::UPDATE_SKILL_PANEL,
        about: Some(1),
        words: vec![1, 0x119, 7, 0],
        records: Vec::new(),
    };
    w.tick(&[points]);
    w.tick(&[request_changed(EXAMINE_SKILL, 0xA)]);
    let misc = w.text_of("MiscText").replace(',', "");
    assert!(
        misc.contains(&EXPERIENCE.to_string()),
        "the foot says {misc:?}"
    );
}

impl World {
    fn text_of(&self, name: &str) -> String {
        let h = self.ui.element(Token::of(name)).expect(name);
        self.ui.text(h).unwrap_or_default()
    }

    fn foot_button(&self, name: &str) -> Handle {
        let f = self.ui.element(Token::of("ModifyField")).expect("foot");
        self.ui.child_recursive(f, Token::of(name)).expect(name)
    }

    fn play_tree(&self) -> (u32, u32) {
        self.play
            .child::<auberean_ui_game::toolbar::Toolbar>()
            .and_then(|t| t.child::<auberean_ui_game::skills::SkillWindow>())
            .and_then(|s| s.tree.as_ref())
            .map(auberean_ui_game::skill_tree::SkillTree::shown)
            .expect("the tree")
    }
}
