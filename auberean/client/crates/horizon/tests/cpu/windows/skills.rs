//! The skill window: the kinds of skill, the tree and its nodes, the panels visited, the skill
//! selected and what may be done with it, and a skill dragged to the hotbar.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use std::collections::BTreeMap;

use auberean_client_sdk::contract::notice::examination::kind as examine;
use auberean_client_sdk::contract::notice::{ExaminationNotice, ExaminationRequest, Notice, Said};
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::{self as v, skill_node, View};
use auberean_client_sdk::contract::{shortcut_kind, Action, SkillAction, ToolbarShortcut};
use auberean_horizon::abilities::{Bar, Node, Skills, Slot};
use auberean_horizon::state::{Frame, GameState};
use auberean_horizon::ui::hotbar::slot_rect;
use auberean_horizon::ui::skills::{grouped, Question, SkillsWindow};

use crate::{centre, in_world, Harness, PLAYER};

const MELEE: u32 = 0x8000_0100;
const MISSILE: u32 = 0x8000_0101;
const SWORDS: u32 = 0x8000_0200;
const SLASH: u32 = 0x4000_0150;
const LUNGE: u32 = 0x4000_0151;

fn node(skill: u32, kind: u32, x: i32, y: i32, parents: &[u32]) -> Node {
    Node {
        skill,
        kind,
        name: format!("{skill:#x}"),
        x,
        y,
        parents: parents.to_vec(),
        ..Node::default()
    }
}

/// The melee panel: Slash trained (may be raised), Lunge under it not trained (may be trained),
/// and a node opening the swords panel.
fn melee() -> Skills {
    let mut states = BTreeMap::new();
    states.insert(
        SLASH,
        v::SkillState {
            trained: true,
            level: 12,
            can_raise: 0,
            can_untrain: 0,
            ..v::SkillState::default()
        },
    );
    states.insert(
        LUNGE,
        v::SkillState {
            can_train: 0,
            cost_to_train: 2,
            ..v::SkillState::default()
        },
    );
    Skills {
        version: 1,
        panel: MELEE,
        panel_name: "Melee".into(),
        nodes: vec![
            node(SLASH, skill_node::SKILL, 1, 0, &[]),
            node(LUNGE, skill_node::SKILL, 1, 1, &[SLASH]),
            node(SWORDS, skill_node::PLUS, 2, 1, &[SLASH]),
        ],
        states,
        roots: [
            Some((MELEE, "Melee".into(), 0)),
            Some((MISSILE, "Missile".into(), 0)),
            None,
            None,
        ],
        credits: 3,
        ..Skills::default()
    }
}

fn with_skills(skills: Skills) -> GameState {
    let mut s = in_world();
    s.skills = Some(skills);
    s
}

/// The window open and drawn once over `s`.
fn opened(h: &mut Harness, s: &GameState) -> auberean_horizon::ui::skills::Parts {
    h.ui.skills.toggle();
    h.frame(s);
    SkillsWindow::parts(
        h.ui.skills.body().expect("drawn"),
        h.ui.layout_scale(h.screen),
    )
}

fn node_at(
    h: &Harness,
    parts: &auberean_horizon::ui::skills::Parts,
    s: &Skills,
    skill: u32,
) -> (f32, f32) {
    let n = s.node(skill).expect("on the panel");
    centre(SkillsWindow::node_rect(parts.nodes, h.k(), &s.nodes, n))
}

/// The tab of the `kind`th kind of skill, as last drawn.
fn kind_tab(h: &Harness, kind: usize) -> auberean_horizon::draw::Rect {
    h.ui.skills
        .kind_tabs()
        .iter()
        .find(|(i, _)| *i == kind)
        .map(|(_, r)| *r)
        .expect("the kind's tab")
}

fn shows(panel: u32) -> Action {
    Action::Examine {
        kind: examine::PANEL,
        data_enum: panel,
        item: 0,
        data_id: 0,
        selection: 0,
    }
}

#[test]
fn a_kinds_tab_shows_its_first_panel_and_a_kind_the_race_lacks_has_no_tab() {
    let mut h = Harness::new();
    let s = with_skills(melee());
    opened(&mut h, &s);
    assert_eq!(
        h.ui.skills
            .kind_tabs()
            .iter()
            .map(|(i, _)| *i)
            .collect::<Vec<_>>(),
        [0, 1],
        "no magic and no others for this race"
    );
    assert_eq!(
        h.click(&s, centre(kind_tab(&h, 1))).acts,
        vec![shows(MISSILE)]
    );
    // The melee panel's own tab is lit: pressed, nothing; on a later panel, back to it.
    assert!(h.click(&s, centre(kind_tab(&h, 0))).acts.is_empty());
    let mut later = melee();
    later.panel = SWORDS;
    let s = with_skills(later);
    h.frame(&s);
    assert_eq!(
        h.click(&s, centre(kind_tab(&h, 0))).acts,
        vec![shows(MELEE)]
    );
}

#[test]
fn pressing_a_node_has_the_game_examine_its_skill_or_show_the_panel_it_opens() {
    let mut h = Harness::new();
    let skills = melee();
    let s = with_skills(skills.clone());
    let parts = opened(&mut h, &s);
    assert_eq!(
        h.click(&s, node_at(&h, &parts, &skills, LUNGE)).acts,
        vec![Action::Examine {
            kind: examine::SKILL,
            data_enum: LUNGE,
            item: 0,
            data_id: 0,
            selection: 0,
        }]
    );
    assert_eq!(
        h.click(&s, node_at(&h, &parts, &skills, SWORDS)).acts,
        vec![shows(SWORDS)]
    );
}

#[test]
fn the_tree_grows_up_from_its_first_row_and_its_nodes_stand_inside_it() {
    let h = Harness::new();
    let skills = melee();
    let tree = auberean_horizon::draw::Rect::new(100.0, 100.0, 500.0, 300.0);
    let at = |skill| {
        SkillsWindow::node_rect(
            tree,
            h.k(),
            &skills.nodes,
            skills.node(skill).expect("there"),
        )
    };
    let (slash, lunge, swords) = (at(SLASH), at(LUNGE), at(SWORDS));
    assert!(
        (slash.x - lunge.x).abs() < 0.5 && lunge.bottom() < slash.y,
        "the skill hung from the first row's stands above it"
    );
    assert!(
        tree.bottom() - slash.bottom() < 58.0 * h.k(),
        "the first row at the bottom"
    );
    assert!(
        swords.x > lunge.right() && (swords.y - lunge.y).abs() < 0.5,
        "beside it"
    );
    for r in [slash, lunge, swords] {
        assert!(tree.contains(r.x, r.y) && tree.contains(r.right() - 0.5, r.bottom() - 0.5));
    }
}

fn view_of(panel: u32, version: u64) -> View {
    View {
        player_id: Some(PLAYER),
        player: Some(v::Player {
            id: PLAYER,
            ..v::Player::default()
        }),
        skills: Some(v::Skills {
            version,
            panel,
            nodes: vec![v::SkillNode {
                skill: SLASH,
                kind: skill_node::SKILL,
                name: TextRef::default(),
                ..v::SkillNode::default()
            }],
            ..v::Skills::default()
        }),
        ..View::default()
    }
}

fn follow(s: &mut GameState, view: &View, notices: &[Notice]) {
    s.follow(&Frame {
        view: Some(view),
        notices,
        events: &[],
        told: &[],
        lines: &[],
        write: &|_| Some(String::new()),
        unknown_command: "",
    });
}

#[test]
fn each_panel_shown_goes_on_the_panels_visited_and_going_back_takes_it_off() {
    let mut s = GameState::default();
    follow(&mut s, &view_of(MELEE, 1), &[]);
    assert!(s.skills.as_ref().expect("the panel").history.is_empty());
    follow(&mut s, &view_of(SWORDS, 2), &[]);
    assert_eq!(s.skills.as_ref().expect("the panel").history, vec![MELEE]);
    follow(&mut s, &view_of(MELEE, 3), &[]);
    assert!(
        s.skills.as_ref().expect("the panel").history.is_empty(),
        "back to where it was"
    );
}

#[test]
fn the_skill_the_examination_asks_about_is_selected_and_another_panel_lets_it_go() {
    let mut s = GameState::default();
    let asked = |skill| Notice {
        about: Some(PLAYER),
        said: Said::Examination(ExaminationNotice::RequestChanged(Some(
            ExaminationRequest {
                kind: examine::SKILL,
                data_enum: skill,
                ..ExaminationRequest::default()
            },
        ))),
    };
    follow(&mut s, &view_of(MELEE, 1), &[]);
    follow(&mut s, &view_of(MELEE, 1), &[asked(SLASH)]);
    assert_eq!(s.skills.as_ref().expect("the panel").selected, SLASH);
    follow(&mut s, &view_of(MELEE, 1), &[asked(LUNGE)]);
    assert_eq!(
        s.skills.as_ref().expect("the panel").selected,
        SLASH,
        "not on the panel"
    );
    follow(&mut s, &view_of(SWORDS, 2), &[]);
    assert_eq!(s.skills.as_ref().expect("the panel").selected, 0);
}

#[test]
fn a_trained_skill_is_raised_at_once_and_untrained_once_the_question_is_answered_yes() {
    let mut h = Harness::new();
    let mut skills = melee();
    skills.selected = SLASH;
    let s = with_skills(skills);
    let parts = opened(&mut h, &s);
    let [_, raise, untrain, _] = parts.buttons;
    assert_eq!(
        h.click(&s, centre(raise)).acts,
        vec![Action::Skill(SkillAction::Raise(SLASH))]
    );
    assert!(h.click(&s, centre(untrain)).acts.is_empty(), "asked first");
    assert_eq!(h.ui.skills.question, Some(Question::Untrain(SLASH)));
    assert!(h.click(&s, centre(parts.no)).acts.is_empty());
    assert_eq!(h.ui.skills.question, None, "no: nothing");
    h.click(&s, centre(untrain));
    assert_eq!(
        h.click(&s, centre(parts.yes)).acts,
        vec![Action::Skill(SkillAction::Untrain(SLASH))]
    );
}

#[test]
fn training_asks_first_unless_the_training_may_be_undone_and_is_refused_when_it_may_not_be_had() {
    let mut h = Harness::new();
    let mut skills = melee();
    skills.selected = LUNGE;
    let s = with_skills(skills.clone());
    let parts = opened(&mut h, &s);
    let train = parts.buttons[0];
    assert!(h.click(&s, centre(train)).acts.is_empty());
    assert_eq!(h.ui.skills.question, Some(Question::Train(LUNGE)));
    assert_eq!(
        h.click(&s, centre(parts.yes)).acts,
        vec![Action::Skill(SkillAction::Train(LUNGE))]
    );
    skills.nodes[1].untrainable = true;
    let s = with_skills(skills.clone());
    assert_eq!(
        h.click(&s, centre(train)).acts,
        vec![Action::Skill(SkillAction::Train(LUNGE))],
        "its training may be undone: at once"
    );
    skills.states.get_mut(&LUNGE).expect("its state").can_train = 0x7558;
    let s = with_skills(skills);
    assert!(
        h.click(&s, centre(train)).acts.is_empty(),
        "may not be trained"
    );
}

#[test]
fn a_trained_skill_dragged_from_the_tree_onto_the_hotbar_goes_in_that_slot() {
    let mut h = Harness::new();
    let skills = melee();
    let mut s = with_skills(skills.clone());
    s.bar = Some(Bar {
        version: 1,
        slots: vec![Slot::default(); 100],
        set: 1,
        rows: 1,
        locked: false,
        numbers: true,
    });
    let parts = opened(&mut h, &s);
    let to = centre(slot_rect(h.screen, h.k(), 0, 6));
    let out = h.drag(&s, node_at(&h, &parts, &skills, SLASH), to);
    assert!(
        out.acts.contains(&Action::SetToolbarSlot {
            slot: 6,
            shortcut: ToolbarShortcut {
                kind: shortcut_kind::SKILL,
                data_enum: SLASH,
                ..ToolbarShortcut::default()
            },
        }),
        "{:?}",
        out.acts
    );
    // One not trained stays where it is.
    let out = h.drag(&s, node_at(&h, &parts, &skills, LUNGE), to);
    assert!(
        !out.acts
            .iter()
            .any(|a| matches!(a, Action::SetToolbarSlot { .. })),
        "{:?}",
        out.acts
    );
}

#[test]
fn numbers_are_written_with_their_thousands_set_apart() {
    assert_eq!(grouped(0), "0");
    assert_eq!(grouped(999), "999");
    assert_eq!(grouped(1_000), "1,000");
    assert_eq!(grouped(12_345_678), "12,345,678");
}

const MAGIC: u32 = 0x8000_0102;
const SPECIAL: u32 = 0x8000_0103;

/// The four kinds' root panels, named as the game names them, and the game's strings: the
/// kinds' short names by their names in the interface's string table.
fn named_kinds() -> (View, impl Fn(&TextRef) -> Option<String>) {
    let mut panels = BTreeMap::new();
    for (id, name) in [
        (MELEE, "Human Melee Skills"),
        (MISSILE, "Human Missile Skills"),
        (MAGIC, "Human Magic Skills"),
        (SPECIAL, "Noncombat and Kingdom Skills"),
    ] {
        panels.insert(id, (TextRef::literal(name), 0));
    }
    let mut view = view_of(MELEE, 1);
    let skills = view.skills.as_mut().expect("skills");
    skills.panels = panels;
    skills.roots = [Some(MELEE), Some(MISSILE), Some(MAGIC), Some(SPECIAL)];
    let short: Vec<(u32, &str)> = auberean_horizon::abilities::KIND_LABELS
        .iter()
        .map(|n| auberean_ui::token::Token::of(n).raw())
        .zip(["Melee", "Missile", "Magic", "Special"])
        .collect();
    let write = move |t: &TextRef| {
        if let Some(l) = &t.literal {
            return Some(String::from_utf16_lossy(l));
        }
        short
            .iter()
            .find(|(s, _)| t.table == auberean_horizon::STRINGS && *s == t.string)
            .map(|(_, w)| (*w).to_owned())
    };
    (view, write)
}

#[test]
fn the_kinds_tabs_read_the_games_short_names_whole_within_their_tabs() {
    let (view, write) = named_kinds();
    let mut s = in_world();
    s.follow(&Frame {
        view: Some(&view),
        notices: &[],
        events: &[],
        told: &[],
        lines: &[],
        write: &write,
        unknown_command: "",
    });
    let skills = s.skills.clone().expect("the panel");
    assert_eq!(
        skills.roots[3].as_ref().map(|r| r.1.as_str()),
        Some("Noncombat and Kingdom Skills")
    );
    let labels: Vec<&str> = (0..4)
        .map(|i| auberean_horizon::ui::skills::kind_label(&skills, i))
        .collect();
    assert_eq!(labels, ["Melee", "Missile", "Magic", "Special"]);
    for (screen, step) in crate::play::fit::screens() {
        let mut h = crate::play::fit::on(screen, step);
        let parts = opened(&mut h, &s);
        let k = h.ui.layout_scale(screen);
        let tabs = h.ui.skills.kind_tabs().to_vec();
        assert_eq!(tabs.len(), 4, "{screen:?}: a tab for each kind");
        for (n, (i, r)) in tabs.iter().enumerate() {
            // The room a tab keeps for its label before the kit cuts it.
            let room = r.w - 16.0 * k;
            let wide = crate::measured(&h, |p| {
                let style = dereth_ui_kit::paint::TextStyle::new(
                    dereth_ui_kit::art::Family::Body,
                    12.0,
                    p.art.palette().label,
                );
                p.measure(&style, labels[*i])
            });
            assert!(
                wide > 0.0 && wide <= room,
                "{screen:?} {}: {wide} in {room}",
                labels[*i]
            );
            assert!(r.right() <= parts.tabs.right(), "{screen:?}: in the strip");
            if let Some((_, next)) = tabs.get(n + 1) {
                assert!(r.x < next.x, "{screen:?}: the tabs in order");
            }
        }
    }
}

#[test]
fn a_press_on_the_skill_window_leaves_the_worlds_selection_as_it_is() {
    const TRADER: u64 = 0x7000_0000_0000_0077;
    let mut h = Harness::new();
    let skills = melee();
    let mut s = with_skills(skills.clone());
    s.target = Some(auberean_horizon::state::Target {
        id: TRADER,
        name: "Wolfgang the Trader".into(),
        health: None,
    });
    let parts = opened(&mut h, &s);
    let kept = |kind, data_enum| Action::Examine {
        kind,
        data_enum,
        item: 0,
        data_id: 0,
        selection: TRADER,
    };
    assert_eq!(
        h.click(&s, centre(kind_tab(&h, 1))).acts,
        vec![kept(examine::PANEL, MISSILE)],
        "a kind's tab"
    );
    assert_eq!(
        h.click(&s, node_at(&h, &parts, &skills, LUNGE)).acts,
        vec![kept(examine::SKILL, LUNGE)],
        "a skill"
    );
    assert_eq!(
        h.click(&s, node_at(&h, &parts, &skills, SWORDS)).acts,
        vec![kept(examine::PANEL, SWORDS)],
        "a node opening a panel"
    );
}

/// The window opens 336 units wide, no wider than its tree and foot need, and still holds them on
/// every screen and scale step: the game's six columns of nodes at their full size within the
/// tree's room, clear of its scroll bar; the points' line whole; the foot's buttons inside the
/// body, their labels whole at the button's own size; the window on the screen.
#[test]
fn the_skill_window_opens_narrow_and_holds_its_tree_and_foot() {
    let mut wide = melee();
    // A row of the game's six columns.
    wide.nodes = (0..6)
        .map(|x| {
            node(
                SLASH + x,
                skill_node::SKILL,
                i32::try_from(x).expect("small"),
                0,
                &[],
            )
        })
        .collect();
    wide.credits = 12;
    wide.hero_credits = 3;
    wide.experience = 12_345_678;
    let s = with_skills(wide.clone());
    for (screen, step) in crate::play::fit::screens() {
        let mut h = crate::play::fit::on(screen, step);
        let parts = opened(&mut h, &s);
        let k = h.ui.layout_scale(screen);
        let at = format!("{screen:?} {step:?}");
        let body = h.ui.skills.body().expect("drawn");
        let window = h.ui.skills.rect().expect("drawn");
        assert!(
            (body.w - 336.0 * k).abs() <= 1.0,
            "{at}: {} wide",
            body.w / k
        );
        assert!(
            window.x >= 0.0 && window.right() <= screen.0 && window.bottom() <= screen.1,
            "{at}: {window:?} on the screen"
        );
        let side = (auberean_horizon::ui::skills::NODE * k).round();
        for n in &wide.nodes {
            let r = SkillsWindow::node_rect(parts.nodes, k, &wide.nodes, n);
            assert!(
                (r.w - side).abs() <= 1.0,
                "{at}: a node {} wide, not {side}",
                r.w
            );
            assert!(
                r.x >= parts.nodes.x - 0.5 && r.right() <= parts.nodes.right() + 0.5,
                "{at}: {r:?} in the room {:?}",
                parts.nodes
            );
        }
        for line in SkillsWindow::points_lines(&wide) {
            let w = crate::measured(&h, |p| {
                let style = dereth_ui_kit::paint::TextStyle::new(
                    dereth_ui_kit::art::Family::Body,
                    12.0,
                    0xFFE8_E0D0,
                );
                p.measure(&style, &line)
            });
            assert!(
                w <= parts.points.w,
                "{at}: {line:?} {w} in {}",
                parts.points.w
            );
        }
        for (b, label) in parts
            .buttons
            .iter()
            .zip(["Train", "Raise", "Untrain", "Stop Untraining"])
        {
            assert!(
                b.x >= body.x && b.right() <= body.right() - 8.0 * k,
                "{at}: {label}"
            );
            let wide =
                crate::measured(&h, |p| dereth_ui_kit::widgets::button_label_width(p, label));
            assert!(wide <= b.w - 12.0 * k, "{at}: {label} {wide} on {}", b.w);
        }
    }
}

/// The window opens on the first kind's first panel at its bottom, whatever it showed before: on
/// a later panel, the game is asked for the first kind's; scrolled up, it opens at the bottom
/// again; for a race with no first kind, on the first kind it has.
#[test]
fn the_skill_window_opens_on_the_first_kinds_first_panel_at_its_bottom() {
    let mut h = Harness::new();
    let mut later = tall();
    later.panel = SWORDS;
    let s = with_skills(later);
    h.ui.skills.toggle();
    assert_eq!(
        h.frame(&s).acts,
        vec![shows(MELEE)],
        "a later panel shown before"
    );
    assert!(h.frame(&s).acts.is_empty(), "asked once");
    // The game shows it: the tree at its bottom, the first row in view.
    let skills = tall();
    let s = with_skills(skills.clone());
    h.frame(&s);
    let k = h.k();
    let parts = SkillsWindow::parts(h.ui.skills.body().expect("drawn"), k);
    assert!(SkillsWindow::rows_height(parts.nodes, k, &skills.nodes) > parts.tree.h);
    assert!(
        h.ui.skills.lifted(&parts, k, &skills.nodes).abs() < 0.5,
        "at its bottom"
    );
    for _ in 0..40 {
        h.input.mouse = centre(parts.tree);
        h.input.wheel = 1.0;
        h.frame(&s);
    }
    assert!(
        h.ui.skills.lifted(&parts, k, &skills.nodes) > 0.0,
        "scrolled up"
    );
    h.ui.skills.toggle();
    h.frame(&s);
    h.ui.skills.toggle();
    assert!(
        h.frame(&s).acts.is_empty(),
        "the first kind's first panel shows already"
    );
    assert!(
        h.ui.skills.lifted(&parts, k, &skills.nodes).abs() < 0.5,
        "at its bottom again"
    );
    // Nothing shown yet, and a race with no first kind: its first kind.
    let mut none = melee();
    none.panel = 0;
    none.roots[0] = None;
    let s = with_skills(none);
    let mut h = Harness::new();
    h.ui.skills.toggle();
    assert_eq!(h.frame(&s).acts, vec![shows(MISSILE)]);
}

/// A chain of skills, each hung from the one under it, taller than the tree's room.
fn tall() -> Skills {
    let mut skills = melee();
    skills.nodes = (0..12_u32)
        .map(|i| {
            let parents: Vec<u32> = if i == 0 {
                Vec::new()
            } else {
                vec![SLASH + i - 1]
            };
            node(
                SLASH + i,
                skill_node::SKILL,
                1,
                i32::try_from(i).expect("small"),
                &parents,
            )
        })
        .collect();
    skills
}

/// A tree taller than its room opens at its bottom, its first row in view, and scrolls up with the
/// wheel to its last; a node scrolled out of the room cannot be pressed through the window round it.
#[test]
fn a_tall_tree_opens_at_its_bottom_and_scrolls_to_its_top() {
    let mut h = Harness::new();
    let skills = tall();
    let s = with_skills(skills.clone());
    let parts = opened(&mut h, &s);
    let k = h.k();
    let rows = SkillsWindow::rows_height(parts.nodes, k, &skills.nodes);
    assert!(rows > parts.tree.h, "taller than its room");
    let at = |h: &Harness, skill: u32| {
        let lifted = h.ui.skills.lifted(&parts, k, &skills.nodes);
        let n = skills.node(skill).expect("there");
        SkillsWindow::node_rect_lifted(parts.nodes, k, &skills.nodes, n, lifted)
    };
    let (first, last) = (SLASH, SLASH + 11);
    let inside = |r: auberean_horizon::draw::Rect| {
        r.y >= parts.tree.y - 0.5 && r.bottom() <= parts.tree.bottom() + 0.5
    };
    assert!(inside(at(&h, first)), "the first row in view");
    assert!(!inside(at(&h, last)), "the last out of it");
    // The first node wholly above the room, pressed where it would stand: nothing.
    let hidden = (0..12)
        .map(|i| at(&h, SLASH + i))
        .find(|r| r.bottom() <= parts.tree.y)
        .expect("a node above the room");
    assert!(
        hidden.y >= h.ui.skills.body().expect("drawn").y,
        "inside the window"
    );
    assert!(h.click(&s, centre(hidden)).acts.is_empty());
    for _ in 0..40 {
        h.input.mouse = centre(parts.tree);
        h.input.wheel = 1.0;
        h.frame(&s);
    }
    assert!(inside(at(&h, last)), "scrolled to the last row");
    assert!(!inside(at(&h, first)));
    let examined = h.click(&s, centre(at(&h, last))).acts;
    assert_eq!(
        examined,
        vec![Action::Examine {
            kind: examine::SKILL,
            data_enum: last,
            item: 0,
            data_id: 0,
            selection: 0,
        }]
    );
}

/// The foot keeps no empty band: the skill's name and points at its top, its buttons right under
/// them, and the window's edge a margin below.
#[test]
fn the_skill_foot_holds_its_lines_and_buttons_close() {
    let h = Harness::new();
    let k = h.k();
    let body = auberean_horizon::draw::Rect::new(0.0, 0.0, 336.0 * k, 480.0 * k);
    let parts = SkillsWindow::parts(body, k);
    assert!(parts.foot.h <= 90.0 * k, "{}", parts.foot.h / k);
    for b in parts.buttons {
        assert!(b.y - parts.foot.y <= 56.0 * k, "under the lines");
        assert!(
            parts.foot.bottom() - b.bottom() <= 10.0 * k,
            "a margin below"
        );
        assert!(
            b.right() <= parts.foot.right() - 8.0 * k,
            "inside the window"
        );
    }
    assert!(parts.tree.bottom() <= parts.foot.y && parts.points.bottom() <= parts.tree.y);
}
