//! The hotbar: the shortcut bar's set shown, using a slot, dragging one, the sets, a slot's wait
//! and its special attack's light, examining what a slot holds; and the attack button.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::notice::examination::kind as examine;
use auberean_client_sdk::contract::notice::{CombatNotice, Notice, Said};
use auberean_client_sdk::contract::view::{self as v, View};
use auberean_client_sdk::contract::{shortcut_kind, Action, ToolbarShortcut};
use auberean_horizon::abilities::{Bar, Combat, Light, Slot};
use auberean_horizon::draw::Rect;
use auberean_horizon::keys::Keyed;
use auberean_horizon::state::{Frame, GameState};
use auberean_horizon::ui::hotbar::{
    attack_icon, attack_icon_rect, attack_rect, set_rects, slot_rect, ASKED, ATTACK_GROUND,
    ATTACK_ICON, ATTACK_PAD, SHADE, STARTED, TOGGLED, UNUSED,
};

use crate::{centre, drew_piece, in_world, Harness, PLAYER};

const SWORD_SKILL: u32 = 0x4000_0150;
const POTION: u64 = 0x8130_0000_0000_0077;

/// A skill's slot.
fn skill(skill: u32) -> Slot {
    Slot {
        shortcut: v::Shortcut {
            kind: shortcut_kind::SKILL,
            data_enum: skill,
            ..v::Shortcut::default()
        },
        name: "Slash".into(),
        ..Slot::default()
    }
}

/// A bar of 100 empty slots, set `set` shown, `rows` rows, with `slots` put in.
fn bar(set: u32, rows: u32, slots: &[(u32, Slot)]) -> Bar {
    let mut b = Bar {
        version: 1,
        slots: vec![Slot::default(); 100],
        set,
        rows,
        locked: false,
    };
    for (n, s) in slots {
        b.slots[*n as usize] = s.clone();
    }
    b
}

fn with_bar(b: Bar) -> GameState {
    let mut s = in_world();
    s.bar = Some(b);
    s
}

fn skill_shortcut(skill: u32) -> ToolbarShortcut {
    ToolbarShortcut {
        kind: shortcut_kind::SKILL,
        data_enum: skill,
        ..ToolbarShortcut::default()
    }
}

/// The flat quads drawn in `colour`.
fn fills(h: &Harness, colour: u32) -> Vec<Rect> {
    h.list
        .quads
        .iter()
        .filter(|q| q.tex.is_none() && q.colour == colour)
        .map(|q| q.dst)
        .collect()
}

#[test]
fn a_click_on_a_slot_uses_the_slot_of_the_set_shown() {
    let mut h = Harness::new();
    // The third set: slots 20 to 29 on the first row, 30 to 39 on the second.
    let s = with_bar(bar(3, 2, &[(22, skill(SWORD_SKILL)), (31, skill(7))]));
    let k = h.k();
    let out = h.click(&s, centre(slot_rect(h.screen, k, 0, 2)));
    assert_eq!(out.acts, vec![Action::UseToolbarSlot { slot: 22 }]);
    let out = h.click(&s, centre(slot_rect(h.screen, k, 1, 1)));
    assert_eq!(
        out.acts,
        vec![Action::UseToolbarSlot { slot: 31 }],
        "the row over it"
    );
    let out = h.click(&s, centre(slot_rect(h.screen, k, 0, 5)));
    assert!(out.acts.is_empty(), "an empty slot: {:?}", out.acts);
    assert!(h.ui.covers(
        centre(slot_rect(h.screen, k, 0, 5)).0,
        centre(slot_rect(h.screen, k, 0, 5)).1
    ));
}

#[test]
fn the_bar_shows_the_players_rows_and_the_last_sets_slots_past_the_hundredth_are_never_used() {
    let mut h = Harness::new();
    let k = h.k();
    // The tenth set with two rows: its second row would be slots 100 to 109.
    h.frame(&with_bar(bar(10, 2, &[])));
    let unused = fills(&h, UNUSED);
    assert_eq!(unused.len(), 10, "{unused:?}");
    assert!(unused.contains(&slot_rect(h.screen, k, 1, 0)));
    let out = h.click(
        &with_bar(bar(10, 2, &[])),
        centre(slot_rect(h.screen, k, 1, 0)),
    );
    assert!(out.acts.is_empty());
    // One row: nothing over it answers the pointer.
    h.frame(&with_bar(bar(1, 1, &[])));
    let over = slot_rect(h.screen, k, 1, 0);
    assert!(!h.ui.covers(over.x + 2.0, over.y + 2.0));
    assert!(fills(&h, UNUSED).is_empty());
}

#[test]
fn the_arrows_show_the_set_after_and_the_set_before_round_past_either_end() {
    let mut h = Harness::new();
    let k = h.k();
    let (before, after, _) = set_rects(h.screen, k);
    let last = with_bar(bar(10, 1, &[]));
    assert_eq!(
        h.click(&last, centre(after)).acts,
        vec![Action::ToolbarSet { set: 1 }]
    );
    let first = with_bar(bar(1, 1, &[]));
    assert_eq!(
        h.click(&first, centre(before)).acts,
        vec![Action::ToolbarSet { set: 10 }]
    );
    assert_eq!(
        h.click(&with_bar(bar(4, 1, &[])), centre(after)).acts,
        vec![Action::ToolbarSet { set: 5 }]
    );
}

#[test]
fn a_slot_dragged_onto_another_moves_there_and_off_the_bar_empties_its_slot() {
    let mut h = Harness::new();
    let k = h.k();
    let s = with_bar(bar(1, 1, &[(0, skill(SWORD_SKILL)), (3, skill(7))]));
    let from = centre(slot_rect(h.screen, k, 0, 0));
    let to = centre(slot_rect(h.screen, k, 0, 3));
    let out = h.drag(&s, from, to);
    assert_eq!(
        out.acts,
        vec![
            Action::SetToolbarSlot {
                slot: 0,
                shortcut: ToolbarShortcut::default()
            },
            Action::SetToolbarSlot {
                slot: 3,
                shortcut: skill_shortcut(SWORD_SKILL)
            },
        ],
        "out of its slot, into the other"
    );
    let world = (h.screen.0 / 2.0, h.screen.1 / 2.0);
    let out = h.drag(&s, from, world);
    assert_eq!(
        out.acts,
        vec![Action::SetToolbarSlot {
            slot: 0,
            shortcut: ToolbarShortcut::default()
        }],
        "off the bar"
    );
    // Moved no further than a click: the slot is used.
    let out = h.drag(&s, from, (from.0 + 2.0, from.1));
    assert_eq!(out.acts, vec![Action::UseToolbarSlot { slot: 0 }]);
}

#[test]
fn while_the_slots_are_locked_a_slot_leaves_the_bar_only_with_shift_held() {
    let mut h = Harness::new();
    let k = h.k();
    let mut b = bar(1, 1, &[(0, skill(SWORD_SKILL))]);
    b.locked = true;
    let s = with_bar(b);
    let from = centre(slot_rect(h.screen, k, 0, 0));
    let world = (h.screen.0 / 2.0, h.screen.1 / 2.0);
    assert!(h.drag(&s, from, world).acts.is_empty());
    h.input.shift = true;
    assert_eq!(
        h.drag(&s, from, world).acts,
        vec![Action::SetToolbarSlot {
            slot: 0,
            shortcut: ToolbarShortcut::default()
        }]
    );
}

#[test]
fn a_right_click_examines_what_a_slot_holds_and_opens_the_examine_window() {
    let mut h = Harness::new();
    let k = h.k();
    let potion = Slot {
        shortcut: v::Shortcut {
            kind: shortcut_kind::ITEM,
            item: POTION,
            ..v::Shortcut::default()
        },
        ..Slot::default()
    };
    let s = with_bar(bar(1, 1, &[(0, skill(SWORD_SKILL)), (1, potion)]));
    let out = h.right_click(&s, centre(slot_rect(h.screen, k, 0, 0)));
    assert_eq!(
        out.acts,
        vec![
            Action::Examine {
                kind: examine::SKILL,
                data_enum: SWORD_SKILL,
                item: 0,
                data_id: 0,
                selection: 0,
            },
            Action::Examination {
                tabs: Some(h.ui.examine.asked())
            },
        ]
    );
    assert!(h.ui.examine.is_open());
    let out = h.right_click(&s, centre(slot_rect(h.screen, k, 0, 1)));
    assert_eq!(
        out.acts.first(),
        Some(&Action::Examine {
            kind: examine::ITEM,
            data_enum: 0,
            item: POTION,
            data_id: 0,
            selection: POTION,
        }),
        "an item examined is selected too"
    );
}

#[test]
fn a_skill_waiting_is_shaded_as_far_as_its_wait_has_left_and_one_not_usable_wholly() {
    let mut h = Harness::new();
    let k = h.k();
    let mut waiting = skill(SWORD_SKILL);
    // Ten seconds in all, ready at the game's time 104: at 101, three tenths left.
    waiting.wait = Some((10.0, 104.0));
    let mut unusable = skill(7);
    unusable.unusable = true;
    let mut s = with_bar(bar(1, 1, &[(0, waiting), (1, unusable)]));
    s.game_time = 101.0;
    h.frame(&s);
    let shade = fills(&h, SHADE);
    let r = slot_rect(h.screen, k, 0, 0);
    let wait = shade
        .iter()
        .find(|q| (q.x - r.x).abs() < 0.01 && (q.y - r.y).abs() < 0.01 && q.w == r.w && q.h < r.h)
        .unwrap_or_else(|| panic!("its shade: {shade:?}"));
    assert!((wait.h - r.h * 0.3).abs() < 0.01, "{} of {}", wait.h, r.h);
    assert!(
        shade.contains(&slot_rect(h.screen, k, 0, 1)),
        "the whole slot"
    );
    s.game_time = 105.0;
    h.frame(&s);
    assert!(
        !fills(&h, SHADE).iter().any(|q| q.x == r.x && q.y == r.y),
        "ready"
    );
}

#[test]
fn a_slots_special_attack_lights_it_asked_started_and_turned_on() {
    let mut h = Harness::new();
    let k = h.k();
    let mut s = with_bar(bar(1, 1, &[(4, skill(SWORD_SKILL))]));
    let r = slot_rect(h.screen, k, 0, 4);
    let lit = |h: &Harness, colour: u32| {
        fills(h, colour)
            .iter()
            .any(|q| r.contains(q.x + 0.5, q.y + 0.5))
    };
    h.frame(&s);
    assert!(!lit(&h, ASKED) && !lit(&h, STARTED) && !lit(&h, TOGGLED));
    s.combat.specials.push((9, SWORD_SKILL, false));
    h.frame(&s);
    assert!(lit(&h, ASKED));
    s.combat.specials[0].2 = true;
    h.frame(&s);
    assert!(lit(&h, STARTED) && !lit(&h, ASKED));
    s.combat.specials.clear();
    s.combat.toggled.insert(SWORD_SKILL);
    h.frame(&s);
    assert!(lit(&h, TOGGLED));
}

fn combat(n: CombatNotice) -> Notice {
    Notice {
        about: Some(PLAYER),
        said: Said::Combat(n),
    }
}

#[test]
fn the_special_attacks_light_as_the_scripts_tell_of_them() {
    let mut c = Combat::default();
    c.notice(&CombatNotice::SpecialAsked {
        skill: SWORD_SKILL,
        special: 9,
    });
    assert_eq!(c.light(SWORD_SKILL), Light::Asked);
    // Asked again under the same number: still one.
    c.notice(&CombatNotice::SpecialAsked {
        skill: SWORD_SKILL,
        special: 9,
    });
    assert_eq!(c.specials.len(), 1);
    c.notice(&CombatNotice::SpecialStarted { special: 9 });
    assert_eq!(c.light(SWORD_SKILL), Light::Started);
    c.notice(&CombatNotice::SpecialToggled {
        skill: SWORD_SKILL,
        on: true,
    });
    assert_eq!(c.light(SWORD_SKILL), Light::Started, "started outshines on");
    c.notice(&CombatNotice::SpecialEnded { special: 9 });
    assert_eq!(c.light(SWORD_SKILL), Light::Toggled, "back to on");
    c.notice(&CombatNotice::SpecialToggled {
        skill: SWORD_SKILL,
        on: false,
    });
    assert_eq!(c.light(SWORD_SKILL), Light::Plain);
    assert_eq!(c.light(7), Light::Plain, "another skill");
}

/// The quads drawn over the attack button and just round it: the flat ones' colours, and the
/// textured ones' places.
fn round_attack(h: &Harness) -> (Vec<u32>, Vec<Rect>) {
    let k = h.k();
    let r = attack_rect(h.screen, k);
    let near = Rect::new(r.x - 4.0 * k, r.y - 4.0 * k, r.w + 8.0 * k, r.h + 8.0 * k);
    let over = |q: &&auberean_horizon::draw::Quad| {
        q.dst.x < near.right()
            && q.dst.right() > near.x
            && q.dst.y < near.bottom()
            && q.dst.bottom() > near.y
    };
    let flat = h
        .list
        .quads
        .iter()
        .filter(over)
        .filter(|q| q.tex.is_none())
        .map(|q| q.colour)
        .collect();
    let textured = h
        .list
        .quads
        .iter()
        .filter(over)
        .filter(|q| q.tex.is_some())
        .map(|q| q.dst)
        .collect();
    (flat, textured)
}

#[test]
fn the_attack_button_starts_attacking_and_stops_it_and_draws_no_frame_while_the_player_attacks() {
    let mut h = Harness::with_built_in_art();
    let k = h.k();
    let mut s = in_world();
    let at = centre(attack_rect(h.screen, k));
    assert_eq!(h.click(&s, at).acts, vec![Action::Attack { start: true }]);
    // The scripts say the player attacks.
    let view = View {
        player_id: Some(PLAYER),
        player: Some(v::Player {
            id: PLAYER,
            ..v::Player::default()
        }),
        ..View::default()
    };
    let follow = |s: &mut GameState, attacking: bool| {
        s.follow(&Frame {
            view: Some(&view),
            notices: &[combat(CombatNotice::Attacking { attacking })],
            events: &[],
            told: &[],
            lines: &[],
            write: &|_| None,
            unknown_command: "",
        });
    };
    follow(&mut s, true);
    assert!(s.combat.attacking);
    assert_eq!(h.click(&s, at).acts, vec![Action::Attack { start: false }]);
    h.input.mouse = (h.screen.0 / 2.0, h.screen.1 / 2.0);
    h.frame(&s);
    let (flat, _) = round_attack(&h);
    assert!(
        flat.iter().all(|c| *c == ATTACK_GROUND),
        "nothing but its ground while attacking: {flat:x?}"
    );
    assert!(!flat.contains(&auberean_horizon::ui::hud::ATTACKING));
    follow(&mut s, false);
    h.frame(&s);
    assert_eq!(
        h.click(&s, at).acts,
        vec![Action::Attack { start: true }],
        "a click starts it again"
    );
}

#[test]
fn the_shortcut_keys_use_the_set_shown_and_its_second_row_only_when_shown() {
    let mut h = Harness::new();
    let one_row = with_bar(bar(2, 1, &[]));
    let key = |h: &mut Harness, s: &GameState, row, column| {
        h.ui.keyed(Keyed::Shortcut { row, column }, s)
    };
    assert_eq!(
        key(&mut h, &one_row, 0, 0),
        vec![Action::UseToolbarSlot { slot: 10 }]
    );
    assert_eq!(
        key(&mut h, &one_row, 0, 9),
        vec![Action::UseToolbarSlot { slot: 19 }]
    );
    assert!(
        key(&mut h, &one_row, 1, 0).is_empty(),
        "no second row shown"
    );
    let two_rows = with_bar(bar(2, 2, &[]));
    assert_eq!(
        key(&mut h, &two_rows, 1, 0),
        vec![Action::UseToolbarSlot { slot: 20 }]
    );
    let last = with_bar(bar(10, 2, &[]));
    assert!(key(&mut h, &last, 1, 0).is_empty(), "past the hundredth");
    assert_eq!(
        h.ui.keyed(Keyed::StepSet { next: true }, &last),
        vec![Action::ToolbarSet { set: 1 }]
    );
    assert_eq!(
        h.ui.keyed(Keyed::Set(7), &last),
        vec![Action::ToolbarSet { set: 7 }]
    );
    assert_eq!(
        h.ui.keyed(Keyed::Attack, &last),
        vec![Action::Attack { start: true }]
    );
}

#[test]
fn the_bar_follows_the_views_toolbar_with_each_items_name_and_icon_and_each_skills_wait() {
    let t = v::Toolbar {
        version: 4,
        shortcuts: vec![
            v::Shortcut {
                kind: shortcut_kind::SKILL,
                data_enum: SWORD_SKILL,
                ..v::Shortcut::default()
            },
            v::Shortcut {
                kind: shortcut_kind::ITEM,
                item: POTION,
                ..v::Shortcut::default()
            },
            v::Shortcut {
                kind: shortcut_kind::ITEM,
                item: 5,
                ..v::Shortcut::default()
            },
        ],
        slots: vec![
            v::ToolbarSlot {
                answer: 0,
                tooltip: "Slash".into(),
                icon: 0x0600_1000,
            },
            v::ToolbarSlot::default(),
            v::ToolbarSlot::default(),
        ],
        set: 3,
        height: 2,
        skills: vec![v::ToolbarSkill {
            skill: SWORD_SKILL,
            trained: true,
            usable: true,
            waiting: Some((8.0, 2.5)),
        }],
        items: vec![
            v::ToolbarItem {
                item: POTION,
                have: true,
                here: Some(("Healing Draught".into(), 0x0600_2000)),
                icon_layers: Vec::new(),
            },
            v::ToolbarItem {
                item: 5,
                have: false,
                here: None,
                icon_layers: Vec::new(),
            },
        ],
        ..v::Toolbar::default()
    };
    let view = View {
        toolbar: Some(t),
        clock: v::Clock {
            world_time: 0.0,
            game_time: 50.0,
        },
        ..View::default()
    };
    let mut s = GameState::default();
    s.follow(&Frame {
        view: Some(&view),
        notices: &[],
        events: &[],
        told: &[],
        lines: &[],
        write: &|_| None,
        unknown_command: "",
    });
    let b = s.bar.expect("the bar");
    assert_eq!((b.set, b.rows, b.first(), b.slots.len()), (3, 2, 20, 100));
    let slot = |n: u32| b.slot(n).expect("a slot");
    assert_eq!(
        (slot(0).name.as_str(), slot(0).icon),
        ("Slash", 0x0600_1000)
    );
    assert_eq!(
        slot(0).wait,
        Some((8.0, 52.5)),
        "ready two and a half seconds on"
    );
    assert_eq!(
        (slot(1).name.as_str(), slot(1).icon, slot(1).unusable),
        ("Healing Draught", 0x0600_2000, false)
    );
    assert!(slot(2).unusable, "an item not had");
    assert!(slot(3).is_empty());
}

#[test]
fn with_its_art_the_attack_button_shows_the_dove_and_while_attacking_the_axe_glowing() {
    let mut h = Harness::with_built_in_art();
    let k = h.k();
    let mut s = in_world();
    let button = attack_rect(h.screen, k);
    let icon = attack_icon_rect(button, k);
    h.input.mouse = (h.screen.0 / 2.0, h.screen.1 / 2.0);
    h.frame(&s);
    assert_eq!(attack_icon(false), "attack.off");
    assert_eq!(drew_piece(&h, "attack.off"), vec![icon], "the dove");
    assert!(drew_piece(&h, "attack.on").is_empty());
    assert!(
        drew_piece(&h, "attack.off.glow").is_empty(),
        "no glow at peace"
    );
    s.combat.attacking = true;
    h.frame(&s);
    assert_eq!(drew_piece(&h, "attack.on"), vec![icon], "the axe");
    assert!(drew_piece(&h, "attack.off").is_empty());
    let glow = drew_piece(&h, "attack.on.glow");
    assert_eq!(glow.len(), 1, "the axe glows while attacking");
    let (g, c) = (centre(glow[0]), centre(icon));
    assert!((g.0 - c.0).abs() < 0.5 && (g.1 - c.1).abs() < 0.5 && glow[0].w > icon.w);
    assert_eq!(
        h.click(&s, centre(button)).acts,
        vec![Action::Attack { start: false }],
        "a click still stops it"
    );
}

/// The attack button is its icon with room round it on its ground, and nothing else: no words
/// and no frame, at peace or attacking; the pointer over it lights the dove's glow.
#[test]
fn the_attack_button_is_its_icon_with_room_round_it_and_no_words_or_frame() {
    let mut h = Harness::with_built_in_art();
    let k = h.k();
    let button = attack_rect(h.screen, k);
    let icon = attack_icon_rect(button, k);
    assert!((icon.w - ATTACK_ICON * k).abs() < 0.01 && (icon.h - ATTACK_ICON * k).abs() < 0.01);
    for (edge, room) in [
        (icon.x - button.x, "left"),
        (button.right() - icon.right(), "right"),
        (icon.y - button.y, "top"),
        (button.bottom() - icon.bottom(), "bottom"),
    ] {
        assert!((edge - ATTACK_PAD * k).abs() < 0.01, "{room}: {edge}");
    }
    let mut s = in_world();
    for attacking in [false, true] {
        s.combat.attacking = attacking;
        h.input.mouse = (h.screen.0 / 2.0, h.screen.1 / 2.0);
        h.frame(&s);
        let face = attack_icon(attacking);
        let mut own = drew_piece(&h, face);
        assert_eq!(own, vec![icon], "{face}");
        own.extend(drew_piece(&h, &format!("{face}.glow")));
        let (flat, textured) = round_attack(&h);
        assert!(
            textured.iter().all(|r| own.contains(r)),
            "attacking {attacking}: only the icon and its glow, no words: {textured:?} of {own:?}"
        );
        assert!(!flat.is_empty(), "its ground");
        assert!(
            flat.iter().all(|c| *c == ATTACK_GROUND),
            "attacking {attacking}: no frame: {flat:x?}"
        );
        for name in [
            "button.normal.left",
            "button.hover.left",
            "button.pressed.left",
        ] {
            assert!(drew_piece(&h, name).is_empty(), "{name}");
        }
    }
    s.combat.attacking = false;
    h.input.mouse = centre(button);
    h.frame(&s);
    assert_eq!(
        drew_piece(&h, "attack.off.glow").len(),
        1,
        "the dove glows under the pointer"
    );
}
