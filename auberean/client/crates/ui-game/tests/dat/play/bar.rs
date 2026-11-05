//! The shortcut bar and the status bar on the retail layouts, fed what the player's scripts say.

use auberean_ui::types::button_id;
use auberean_ui::{Handle, Token, Ui};
use auberean_ui_game::examination::ExamRequest;
use auberean_ui_game::gameplay::{self, GamePlay, Notice, Record, Request, View};
use auberean_ui_game::inventory::{Container, Contents, Facts, Inventory, ItemFacts};
use auberean_ui_game::shortcuts::{
    self, action as key, kind, notice as bar_notice, ItemState, Shortcut, Shortcuts, SkillState,
    SlotInfo, Source,
};
use auberean_ui_game::statusbar::{
    notice as status_notice, vital, EffectList, EffectNode, Effects, StatusBar,
};
use auberean_ui_game::toolbar::Toolbar;

use crate::common::records;

/// Pictures of the install, standing in for the skills' icons.
const ICON_A: u32 = 0x4100_44D7;
const ICON_B: u32 = 0x4100_44D6;
const ICON_C: u32 = 0x4100_1710;

/// The layers of an item's look's icon (pictures of the install, untinted), bottom first.
const ITEM_LAYERS: [(u32, u32); 2] = [(0x4100_5FF5, 0xFFFF_FFFF), (0x4100_76CA, 0xFFFF_FFFF)];

/// The player, and a sword in its pack.
const PLAYER: u64 = 0x5000_0001;
const SWORD: u64 = 0x7000_0001;

/// Three skills, the third not usable with what the player carries.
const STRIKE: u32 = 0x10;
const BLOCK: u32 = 0x11;
const SHOOT: u32 = 0x12;

fn skill(e: u32) -> Shortcut {
    Shortcut {
        kind: kind::SKILL,
        data_enum: e,
        ..Shortcut::default()
    }
}

/// The scripts' word: a hundred slots, the three skills in slots 1, 2 and 10 (0, 1, 9), numbered,
/// set 1, one row, moving between sets at once.
fn source(version: u64) -> Source {
    let mut shortcuts = vec![Shortcut::default(); 100];
    shortcuts[0] = skill(STRIKE);
    shortcuts[1] = skill(BLOCK);
    shortcuts[9] = skill(SHOOT);
    let mut slots = vec![
        SlotInfo {
            answer: 1,
            tooltip: "Empty".into(),
            icon: 0,
        };
        100
    ];
    for (i, name, icon) in [
        (0, "Strike", ICON_A),
        (1, "Block", ICON_B),
        (9, "Shoot", ICON_C),
    ] {
        slots[i] = SlotInfo {
            answer: 1,
            tooltip: name.into(),
            icon,
        };
    }
    let state = |s, usable| SkillState {
        skill: s,
        trained: true,
        usable,
        waiting: None,
    };
    Source {
        version,
        shortcuts,
        slots,
        set: 1,
        show_numbers: true,
        height: 1,
        locked: false,
        insta_scroll: true,
        skills: vec![state(STRIKE, true), state(BLOCK, true), state(SHOOT, false)],
        items: Vec::new(),
    }
}

fn notice(native: u32, words: Vec<u32>) -> Notice {
    Notice {
        native,
        about: None,
        words,
        ..Notice::default()
    }
}

struct Play {
    ui: Ui,
    p: GamePlay,
    t: f64,
    source: Source,
    /// The inventory as the scripts answer for it, when the test gives one.
    facts: Option<Facts>,
    /// What the play screen asked when the shortcuts were reloaded.
    reloaded: Vec<Request>,
}

impl Play {
    /// The play screen with the player in the world, the scripts' word given and the shortcuts
    /// reloaded.
    fn new() -> Self {
        let mut ui = Ui::new(records(), 1024, 768);
        let mut out = Vec::new();
        let mut p = GamePlay::new(&mut ui, &mut out);
        p.login_finished(&mut ui, &mut out);
        let mut me = Self {
            ui,
            p,
            t: 0.0,
            source: source(1),
            facts: None,
            reloaded: Vec::new(),
        };
        me.reloaded = me.tick(&[notice(bar_notice::RELOAD_SHORTCUTS, vec![])]);
        me
    }

    fn tick(&mut self, notices: &[Notice]) -> Vec<Request> {
        self.t += 0.05;
        self.ui.use_time(self.t);
        let mut out = Vec::new();
        let view = View {
            shortcuts: Some(&self.source),
            inventory: self.facts.as_ref(),
            ..View::default()
        };
        self.p.tick(&mut self.ui, &view, notices, &mut out);
        out
    }

    fn bar(&self) -> &Shortcuts {
        self.p
            .child::<Toolbar>()
            .and_then(Toolbar::child::<Shortcuts>)
            .expect("the shortcut bar")
    }

    fn slot(&self, i: usize) -> Handle {
        self.bar().slots(&self.ui)[i]
    }

    fn part(&self, i: usize, name: &str) -> Handle {
        self.ui
            .child_recursive(self.slot(i), Token::of(name))
            .unwrap_or_else(|| panic!("{name} in slot {i}"))
    }

    fn text(&self, h: Handle) -> String {
        self.ui.text(h).unwrap_or_default()
    }

    /// The pointer to the middle of an element, the clock moving.
    fn point(&mut self, h: Handle) -> Vec<Request> {
        let r = self.ui.screen_rect(h).expect("placed");
        self.ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
        self.tick(&[])
    }

    fn click(&mut self, h: Handle, button: u32) -> Vec<Request> {
        let mut out = self.point(h);
        for down in [true, false] {
            self.ui.pointer_button(button, down);
            out.extend(self.tick(&[]));
        }
        out
    }

    /// Press on `from`, move a few pixels (the press becomes a drag of what is under it), then to
    /// `(x, y)`, and let go there.
    fn drag(&mut self, from: Handle, (x, y): (i32, i32)) -> Vec<Request> {
        let mut out = self.point(from);
        self.ui.pointer_button(button_id::LEFT, true);
        out.extend(self.tick(&[]));
        let (px, py) = self.ui.pointer();
        self.ui.pointer_move(px + 6, py);
        out.extend(self.tick(&[]));
        self.ui.pointer_move(x, y);
        out.extend(self.tick(&[]));
        self.ui.pointer_button(button_id::LEFT, false);
        out.extend(self.tick(&[]));
        out.extend(self.tick(&[]));
        out
    }

    fn middle(&self, h: Handle) -> (i32, i32) {
        let r = self.ui.screen_rect(h).expect("placed");
        ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2)
    }

    fn key(&mut self, k: u32) -> Vec<Request> {
        self.ui.key(k, true);
        let mut out = self.tick(&[]);
        self.ui.key(k, false);
        out.extend(self.tick(&[]));
        out
    }

    fn element(&self, name: &str) -> Handle {
        self.ui
            .element(Token::of(name))
            .unwrap_or_else(|| panic!("{name} made"))
    }
}

/// The shortcut bar holds a hundred slots (ten sets of ten, one row in view); each slot holding a
/// skill shows the scripts' icon over the slot and its number in its corner, with the scripts'
/// tooltip; an empty slot shows neither and cannot be dragged; a skill the player cannot use with
/// what it carries is shadowed; the set's number shows by the arrows and the scripts are told it.
///
/// Behaviour: ui.shortcuts.the-slots-show-the-scripts-icons-and-numbers
#[test]
fn the_slots_show_the_scripts_icons_and_numbers() {
    let mut s = Play::new();
    s.tick(&[]);
    assert_eq!(
        s.bar().slots(&s.ui).len(),
        100,
        "ten sets of ten for one row"
    );
    let icon = |s: &Play, i| {
        s.ui.node(s.part(i, "ShortcutIcon"))
            .and_then(|n| n.picture.map(|p| p.id))
    };
    assert_eq!(icon(&s, 0), Some(ICON_A));
    assert_eq!(icon(&s, 1), Some(ICON_B));
    assert_eq!(icon(&s, 9), Some(ICON_C));
    assert_eq!(icon(&s, 4), None, "an empty slot has no icon");
    assert_eq!(s.text(s.part(0, "NumberDisplay")), "1");
    assert_eq!(s.text(s.part(9, "NumberDisplay")), "0");
    assert_eq!(
        s.text(s.part(4, "NumberDisplay")),
        "",
        "an empty slot has no number"
    );
    assert_eq!(s.ui.element_tooltip(s.slot(0)), Some("Strike"));
    let dragable = |s: &Play, i| {
        s.ui.node(s.slot(i))
            .is_some_and(|n| n.bool_attr(Token::of("dragable")))
    };
    assert!(dragable(&s, 0));
    assert!(!dragable(&s, 4));
    let position = |s: &Play, i| {
        s.ui.node(s.part(i, "TimeLeftMeter"))
            .and_then(|n| n.float_attr(Token::of("position")))
    };
    assert!(
        position(&s, 9).is_some_and(|p| p < 0.001),
        "an unusable skill is shadowed"
    );
    assert_eq!(position(&s, 0), Some(1.0), "a ready skill is not");
    let set = s.element("SetNumberDisplay");
    assert_eq!(s.text(set), "1");
    assert!(
        s.reloaded.contains(&Request::ShortcutSet(1)),
        "the set told: {:?}",
        s.reloaded
    );
}

/// A slot's key (1 to 0 of the set in view) and a left click on it use it; a right click
/// examines what it holds; Ctrl with a digit shows that set at once, its number by the arrows,
/// and the slot keys then use that set's slots; the attack key asks to attack.
///
/// Behaviour: ui.shortcuts.keys-and-clicks-use-slots-and-sets-move-the-view
#[test]
fn keys_and_clicks_use_slots_and_sets_move_the_view() {
    let mut s = Play::new();
    s.tick(&[]);
    assert!(s.key(key::SHORTCUT_1 + 2).contains(&Request::UseSlot(2)));
    let b = s.part(1, "ShortcutButton");
    assert!(s.click(b, button_id::LEFT).contains(&Request::UseSlot(1)));
    let examined = s.click(b, button_id::RIGHT);
    assert!(
        examined.iter().any(|r| matches!(
            r,
            Request::Examine(e) if e.kind == shortcuts::examine::SKILL && e.data_enum == BLOCK
        )),
        "{examined:?}"
    );
    let shown = s.key(key::SET_1 + 2);
    assert!(shown.contains(&Request::ShortcutSet(3)), "{shown:?}");
    assert_eq!(s.bar().offset(), 20);
    assert_eq!(s.text(s.element("SetNumberDisplay")), "3");
    assert!(s.key(key::SHORTCUT_1).contains(&Request::UseSlot(20)));
    assert!(s.key(key::SET_PREV).contains(&Request::ShortcutSet(2)));
    assert!(s
        .key(key::AUTOATTACK)
        .contains(&Request::Attack { start: true }));
}

/// A slot dragged onto another empty one moves its shortcut there (its own slot emptied); one
/// dragged off the bar is emptied; a skill the player no longer has trained is taken off the
/// bar when the scripts say the skill changed.
///
/// Behaviour: ui.shortcuts.dragging-moves-and-clears-shortcuts
#[test]
fn dragging_moves_and_clears_shortcuts() {
    let mut s = Play::new();
    s.tick(&[]);
    let to = s.middle(s.slot(4));
    let out = s.drag(s.slot(0), to);
    assert!(
        out.contains(&Request::PutInSlot {
            slot: 0,
            shortcut: Shortcut::default()
        }),
        "{out:?}"
    );
    assert!(out.contains(&Request::PutInSlot {
        slot: 4,
        shortcut: skill(STRIKE)
    }));
    assert_eq!(s.bar().shortcuts()[4], skill(STRIKE));
    assert_eq!(s.bar().shortcuts()[0], Shortcut::default());

    let out = s.drag(s.slot(1), (512, 300));
    assert!(
        out.contains(&Request::PutInSlot {
            slot: 1,
            shortcut: Shortcut::default()
        }),
        "dragged off the bar: {out:?}"
    );

    s.source.version += 1;
    s.source.skills[2].trained = false;
    let out = s.tick(&[notice(bar_notice::UPDATE_SKILL, vec![SHOOT])]);
    assert!(out.contains(&Request::PutInSlot {
        slot: 9,
        shortcut: Shortcut::default()
    }));
}

/// A slot pressed keeps its icon, and dragged, the copy that follows the pointer shows the same
/// icon: the icon the bar gave the slot is kept through the slot's button changing state.
///
/// Behaviour: ui.shortcuts.a-slot-keeps-its-icon-while-pressed-and-dragged
#[test]
fn a_slot_keeps_its_icon_while_pressed_and_dragged() {
    let mut s = Play::new();
    s.tick(&[]);
    let icon_of = |s: &Play, h: Handle| {
        s.ui.child_recursive(h, Token::of("ShortcutIcon"))
            .and_then(|i| s.ui.node(i))
            .and_then(|n| n.picture.map(|p| p.id))
    };
    let slot = s.slot(0);
    assert_eq!(icon_of(&s, slot), Some(ICON_A));
    s.point(slot);
    s.ui.pointer_button(button_id::LEFT, true);
    s.tick(&[]);
    assert_eq!(icon_of(&s, slot), Some(ICON_A), "pressed");
    let (x, y) = s.ui.pointer();
    s.ui.pointer_move(x + 6, y);
    s.tick(&[]);
    s.ui.pointer_move(x + 6, y - 60);
    s.tick(&[]);
    let copy = s.ui.dragged().expect("dragging");
    assert_eq!(icon_of(&s, copy), Some(ICON_A), "the dragged copy");
    assert_eq!(icon_of(&s, slot), Some(ICON_A), "the slot, while dragged");
    s.ui.pointer_button(button_id::LEFT, false);
    s.tick(&[]);
}

/// The scripts' answer to the shortcuts the bar put in its slots: each slot asked about holds
/// what the bar put there, with the tooltip and icon the scripts give each skill (and the empty
/// slot's), as the next word from them.
fn answer(source: &mut Source, out: &[Request]) {
    let mut changed = false;
    for r in out {
        let Request::PutInSlot { slot, shortcut } = r else {
            continue;
        };
        let i = *slot as usize;
        source.shortcuts[i] = shortcut.clone();
        let (tooltip, icon) = match (shortcut.kind, shortcut.data_enum) {
            (kind::SKILL, STRIKE) => ("Strike", ICON_A),
            (kind::SKILL, BLOCK) => ("Block", ICON_B),
            (kind::SKILL, SHOOT) => ("Shoot", ICON_C),
            _ => ("Empty", 0),
        };
        source.slots[i] = SlotInfo {
            answer: 1,
            tooltip: tooltip.into(),
            icon,
        };
        changed = true;
    }
    if changed {
        source.version += 1;
    }
}

/// A slot dragged onto another shows its skill's icon and tooltip there, and the slot it left
/// shows none, once the scripts hold the moved shortcuts: onto an empty slot, and onto a slot
/// holding another skill (which it replaces).
///
/// Behaviour: ui.shortcuts.a-moved-slot-shows-its-icon-where-it-went
#[test]
fn a_moved_slot_shows_its_icon_where_it_went() {
    let mut s = Play::new();
    s.tick(&[]);
    let icon = |s: &Play, i| {
        s.ui.node(s.part(i, "ShortcutIcon"))
            .and_then(|n| n.picture.map(|p| p.id))
    };
    for (from, to, picture, tip) in [(0, 4, ICON_A, "Strike"), (1, 9, ICON_B, "Block")] {
        let at = s.middle(s.slot(to));
        let out = s.drag(s.slot(from), at);
        answer(&mut s.source, &out);
        let out = s.tick(&[]);
        assert!(
            !out.iter().any(|r| matches!(r, Request::PutInSlot { .. })),
            "nothing asked again: {out:?}"
        );
        assert_eq!(icon(&s, to), Some(picture), "slot {to}'s icon");
        assert_eq!(s.ui.element_tooltip(s.slot(to)), Some(tip));
        assert_eq!(icon(&s, from), None, "slot {from} emptied");
        assert_eq!(s.ui.element_tooltip(s.slot(from)), Some("Empty"));
        assert_eq!(
            s.text(s.part(to, "NumberDisplay")),
            format!("{}", (to + 1) % 10)
        );
    }
}

/// The status bar shows the vitals the scripts say ("current / greatest"), a hero's focus with its
/// numbers; a click on the health bar hides its numbers and its tooltip says a click shows them;
/// the conflict and vitae indicators follow the scripts; the effects show one icon per effect
/// with its name, marked while the examination asks for it, taken away once the scripts no longer
/// list it; the vitae's overlay shows while the examination asks for the worn vitae.
///
/// Behaviour: ui.statusbar.vitals-indicators-and-effects-follow-the-scripts
#[test]
fn vitals_indicators_and_effects_follow_the_scripts() {
    let mut s = Play::new();
    let long = |v: u64| [(v >> 32) as u32, (v & 0xFFFF_FFFF) as u32];
    let vital_notice = |which: u32, cur: u64, max: u64| {
        let mut w = vec![which];
        w.extend(long(cur));
        w.extend(long(max));
        notice(status_notice::UPDATE_VITAL, w)
    };
    let mut hero = vec![1];
    hero.extend(long(5));
    hero.extend(long(10));
    s.tick(&[
        vital_notice(vital::HEALTH, 37, 120),
        vital_notice(vital::VIGOR, 80, 80),
        notice(status_notice::UPDATE_HERO_STATUS, hero),
        notice(status_notice::UPDATE_FACTION_STATUS, vec![1]),
        notice(
            status_notice::UPDATE_VITAE,
            vec![95.0_f32.to_bits(), 1.0_f32.to_bits(), 500.0_f32.to_bits()],
        ),
    ]);
    assert_eq!(s.text(s.element("HealthText")), "37 / 120");
    assert_eq!(s.text(s.element("VigorText")), "80 / 80");
    assert_eq!(s.text(s.element("FocusText")), "5 / 10");
    let state = |s: &Play, n: &str| s.ui.node(s.element(n)).map(|n| n.state);
    assert_eq!(state(&s, "FocusMeter"), Some(Token::of("hero")));
    assert_eq!(state(&s, "PKStatusField"), Some(Token::of("peace")));
    assert_eq!(state(&s, "VitaeStatusField"), Some(Token::of("notfull")));
    let field = s.element("HealthField");
    let shown_before =
        s.ui.node(s.element("HealthText"))
            .is_some_and(|n| n.visible);
    assert!(shown_before);
    s.click(field, button_id::LEFT);
    assert!(!s
        .ui
        .node(s.element("HealthText"))
        .is_some_and(|n| n.visible));
    let tip = s.ui.element_tooltip(field).unwrap_or_default().to_owned();
    assert!(
        tip.contains("show"),
        "the tooltip says a click shows them: {tip}"
    );

    let effects = EffectList {
        version: 1,
        now: 100.0,
        nodes: vec![EffectNode {
            eid: 7,
            name: "Bless".into(),
            icon: ICON_A,
            time_cast: 90.0,
            duration: 60.0,
            pulsed: false,
        }],
    };
    let tick = |s: &mut Play, list: &EffectList, n: &[Notice]| {
        s.t += 0.05;
        s.ui.use_time(s.t);
        let view = View {
            effects: Some(list),
            ..View::default()
        };
        let mut out = Vec::new();
        s.p.tick(&mut s.ui, &view, n, &mut out);
    };
    tick(
        &mut s,
        &effects,
        &[notice(status_notice::UPDATE_EFFECT, vec![])],
    );
    let effect_icons = |s: &Play| {
        s.p.child::<StatusBar>()
            .and_then(StatusBar::child::<Effects>)
            .expect("the effects")
            .icons(&s.ui)
    };
    let icons = effect_icons(&s);
    assert_eq!(icons.len(), 1);
    assert_eq!(s.ui.element_tooltip(icons[0]), Some("Bless"));
    assert_eq!(
        s.ui.node(icons[0]).map(|n| n.state),
        Some(Token::of("active"))
    );
    // The scripts say the examination asks for the effect, then for an object, then for the vitae.
    let examine = |kind: u32, data_enum: u32| Notice {
        native: gameplay::notice::EXAMINATION_REQUEST_CHANGED,
        about: None,
        words: vec![0],
        records: vec![Some(Record::Request(ExamRequest {
            kind,
            data_enum,
            ..ExamRequest::default()
        }))],
    };
    let icon_state = |s: &Play| {
        let icons = effect_icons(s);
        s.ui.child_recursive(icons[0], Token::of("IconField"))
            .and_then(|i| s.ui.node(i))
            .map(|n| n.state)
    };
    let overlay = |s: &Play| {
        let o =
            s.ui.child_recursive(s.element("VitaeStatusField"), Token::of("Overlay"))
                .expect("the vitae's overlay");
        s.ui.node(o).is_some_and(|n| n.visible)
    };
    assert!(!overlay(&s));
    tick(&mut s, &effects, &[examine(0x4000_0001, 7)]);
    assert_eq!(icon_state(&s), Some(Token::of("selected")));
    tick(&mut s, &effects, &[examine(0x4000_0002, 0)]);
    assert_eq!(icon_state(&s), Some(Token::of("unselected")));
    tick(&mut s, &effects, &[examine(0x4000_000A, 0)]);
    assert!(overlay(&s), "the worn vitae's overlay shows");
    let gone = EffectList {
        version: 2,
        now: 101.0,
        nodes: Vec::new(),
    };
    tick(
        &mut s,
        &gone,
        &[notice(status_notice::UPDATE_EFFECT, vec![])],
    );
    assert!(effect_icons(&s).is_empty());
}

/// A pulsed effect that misses its pulse runs its bar out over what is left of its time at that
/// moment, however long ago the list last changed.
///
/// Behaviour: ui.statusbar.a-missed-pulse-runs-the-bar-out-over-the-time-left
#[test]
fn a_missed_pulse_runs_the_bar_out_over_the_time_left() {
    let mut s = Play::new();
    let effects = EffectList {
        version: 1,
        now: 100.0,
        nodes: vec![EffectNode {
            eid: 7,
            name: "Bless".into(),
            icon: ICON_A,
            time_cast: 90.0,
            duration: 60.0,
            pulsed: true,
        }],
    };
    let tick = |s: &mut Play, game_time: f64, n: &[Notice]| {
        s.t += 0.05;
        s.ui.use_time(s.t);
        let view = View {
            effects: Some(&effects),
            game_time,
            ..View::default()
        };
        let mut out = Vec::new();
        s.p.tick(&mut s.ui, &view, n, &mut out);
    };
    tick(
        &mut s,
        100.0,
        &[notice(status_notice::UPDATE_EFFECT, vec![])],
    );
    tick(
        &mut s,
        130.0,
        &[notice(status_notice::PULSE_MISSED, vec![7])],
    );
    let icons =
        s.p.child::<StatusBar>()
            .and_then(StatusBar::child::<Effects>)
            .expect("the effects")
            .icons(&s.ui);
    let meter =
        s.ui.child_recursive(icons[0], Token::of("EffectTimeMeter"))
            .expect("the bar");
    let left =
        s.ui.node(meter)
            .and_then(|n| n.float_attr(Token::of("smooth_movement_duration")))
            .expect("it runs out");
    assert!((left - 20.0).abs() < 0.01, "{left} seconds, not 20");
}

/// With the player's latency graph option on, the network indicator's tooltip also gives the bytes
/// in and out each second, and a graph is blended over the indicator inside its border: a column
/// a measure, the newest at the right, the bytes sent a second as a bar down from the top and
/// those received as one up from the foot, each as long as the eighth of the bytes takes halvings
/// to come down to one, the texel past its end part of the way to its color; the graph's dark
/// ground is left clear for the measure at which the indicator's state changes. With the option
/// off again, the graph goes.
///
/// Behaviour: ui.statusbar.the-latency-graph-option-draws-the-bandwidth-over-the-network-indicator
#[test]
fn the_latency_graph_option_draws_the_bandwidth_over_the_network_indicator() {
    use auberean_ui::draw::{self, Surface};
    use auberean_ui_game::options::{self, Values};
    use auberean_ui_game::statusbar::Link;

    let mut s = Play::new();
    // The pointer away from the status bar, so no tooltip covers it.
    s.ui.pointer_move(600, 300);
    let field = s.element("NetStatusField");
    let mut beat = 0;
    let mut measure =
        |s: &mut Play, sent: u32, received: u32, quiet: f32, bits: u32, n: &[Notice]| {
            beat += 1;
            let link = Link {
                beat,
                latency: 0.05,
                loss: 0.0,
                bytes_in: received,
                bytes_out: sent,
                span: 1.0,
                since_heard: quiet,
            };
            s.t += 0.05;
            s.ui.use_time(s.t);
            let view = View {
                link: Some(link),
                options: Some(Values {
                    bits,
                    ..Values::default()
                }),
                ..View::default()
            };
            let mut out = Vec::new();
            s.p.tick(&mut s.ui, &view, n, &mut out);
        };
    let overlay = |s: &Play| s.ui.node(field).and_then(|n| n.overlay.clone());
    let tip = |s: &Play| s.ui.element_tooltip(field).unwrap_or_default().to_owned();

    // The option off: no graph, and the tooltip without the bytes.
    measure(&mut s, 40, 8, 0.0, 0, &[]);
    assert!(overlay(&s).is_none(), "no graph while the option is off");
    let plain = tip(&s);
    let mut plain_screen = Surface::new(1024, 768);
    draw::draw(&s.ui, &mut plain_screen);

    // The option on, read again: a graph inside the 2-texel border, 16 by 28, and the bytes in the
    // tooltip.
    let reload = notice(gameplay::notice::RELOAD_OPTIONS, vec![]);
    measure(&mut s, 40, 8, 0.0, options::LATENCY_GRAPH, &[reload]);
    let (x, y, graph) = overlay(&s).expect("the graph over the indicator");
    assert_eq!((x, y, graph.width, graph.height), (2, 2, 16, 28));
    assert_ne!(tip(&s), plain, "the tooltip gives the bytes too");
    assert!(
        tip(&s).contains("40"),
        "the bytes sent a second: {}",
        tip(&s)
    );

    // Its newest column: 40 bytes sent (an eighth of 5: two halvings, 3 texels down, the fourth a
    // quarter of the way from the ground) and 8 received (one texel up from the foot).
    let ground = [0, 0, 0, 127];
    let sent = [119, 141, 151, 191];
    let received = [150, 170, 176, 191];
    let quarter = [29, 35, 37, 143];
    let column = |g: &Surface, x: i32| (0..g.height).map(|y| g.get(x, y)).collect::<Vec<_>>();
    let newest = column(&graph, 15);
    assert_eq!(&newest[..3], &[sent; 3]);
    assert_eq!(newest[3], quarter);
    assert!(newest[4..27].iter().all(|t| *t == ground));
    assert_eq!(newest[27], received);
    for x in 0..15 {
        assert!(
            column(&graph, x).iter().all(|t| *t == ground),
            "column {x}: no measure yet"
        );
    }

    // Blended over the indicator's picture: each texel moves toward the graph's by its alpha.
    let mut screen = Surface::new(1024, 768);
    draw::draw(&s.ui, &mut screen);
    let at = s.ui.screen_rect(field).expect("placed");
    let blend = |d: [u8; 4], g: [u8; 4]| {
        let a = i32::from(g[3]);
        let m = |dc: u8, gc: u8| {
            let dc = i32::from(dc);
            u8::try_from(dc - (dc - i32::from(gc)) * a / 256).unwrap_or(0)
        };
        [m(d[0], g[0]), m(d[1], g[1]), m(d[2], g[2])]
    };
    for (gx, gy) in [(15, 0), (15, 3), (15, 10), (15, 27), (0, 0)] {
        let (sx, sy) = (at.x0 + 2 + gx, at.y0 + 2 + gy);
        let before = plain_screen.get(sx, sy);
        assert_eq!(
            screen.get(sx, sy)[..3],
            blend(before, graph.get(gx, gy)),
            "texel ({gx}, {gy}) of the graph"
        );
    }

    // More measures move the columns left; one 800 bytes sent a second reaches 7 texels down.
    measure(&mut s, 800, 0, 0.0, options::LATENCY_GRAPH, &[]);
    let (_, _, graph) = overlay(&s).expect("the graph");
    assert_eq!(column(&graph, 14)[..3], [sent; 3]);
    let newest = column(&graph, 15);
    assert_eq!(&newest[..7], &[sent; 7]);
    assert_eq!(newest[27], ground, "nothing received");

    // The indicator turns bad (the server unheard for 12 seconds): for that measure the ground is
    // clear; the next measure, dark again.
    measure(&mut s, 0, 0, 12.0, options::LATENCY_GRAPH, &[]);
    let (_, _, graph) = overlay(&s).expect("the graph");
    assert_eq!(
        graph.get(0, 10)[3],
        0,
        "the ground left clear as the state changes"
    );
    measure(&mut s, 0, 0, 12.0, options::LATENCY_GRAPH, &[]);
    let (_, _, graph) = overlay(&s).expect("the graph");
    assert_eq!(graph.get(0, 10), ground);

    // The option turned off in the options window: the graph goes.
    let changed = notice(options::notice::OPTIONS_CHANGED, vec![]);
    measure(&mut s, 0, 0, 12.0, 0, &[changed]);
    assert!(overlay(&s).is_none(), "the graph goes with the option");
}

/// The player's pack holding the sword, whose look makes its icon of two layers (its own icon
/// number is 0, as for every item of the recorded sessions).
fn pack_with_a_sword() -> Facts {
    let slots = vec![(0, 1), (1, 0), (1, 0), (1, 0)];
    let segments = slots
        .iter()
        .map(|(first, count)| Contents {
            items: vec![SWORD],
            first: *first,
            count: *count,
        })
        .collect();
    let mut f = Facts {
        version: 1,
        player: PLAYER,
        segments: slots.clone(),
        coin_stack: 1000,
        ..Facts::default()
    };
    f.containers.insert(
        PLAYER,
        Container {
            size: 78,
            segments,
            slots,
        },
    );
    f.items.insert(
        SWORD,
        ItemFacts {
            quantity: 1,
            icon_layers: ITEM_LAYERS.to_vec(),
            stack_size: 1,
            name: "Sword".into(),
            had: true,
            ..ItemFacts::default()
        },
    );
    f
}

/// An item dragged from the pack onto an empty slot is put there; once the scripts hold it, the
/// slot shows the icon the item's look makes (its layers laid together; the item's own icon
/// number shows nothing), the item's name as its tooltip, and its number.
///
/// Behaviour: ui.shortcuts.an-item-slot-shows-the-icon-its-look-makes
#[test]
fn an_item_slot_shows_the_icon_its_look_makes() {
    let mut s = Play::new();
    s.facts = Some(pack_with_a_sword());
    s.tick(&[]);
    s.ui.key(auberean_ui_game::toolbar::action::INVENTORY_PANEL, true);
    s.tick(&[]);
    s.ui.key(auberean_ui_game::toolbar::action::INVENTORY_PANEL, false);
    s.tick(&[]);
    let sword =
        s.p.child::<Toolbar>()
            .and_then(|t| t.child::<Inventory>())
            .and_then(|i| i.backpack())
            .and_then(|b| b.packs()[0].item_display(SWORD))
            .and_then(|d| d.icon())
            .expect("the sword's icon in the pack");
    let to = s.middle(s.slot(5));
    let out = s.drag(sword, to);
    let put = Shortcut {
        kind: kind::ITEM,
        item: SWORD,
        ..Shortcut::default()
    };
    assert!(
        out.contains(&Request::PutInSlot {
            slot: 5,
            shortcut: put.clone()
        }),
        "{out:?}"
    );
    // The scripts hold it: an item's slot has no icon of the scripts' own.
    s.source.version += 1;
    s.source.shortcuts[5] = put;
    s.source.slots[5] = SlotInfo {
        answer: 1,
        tooltip: String::new(),
        icon: 0,
    };
    s.source.items.push(ItemState {
        item: SWORD,
        have: true,
        here: Some(("Sword".into(), 0)),
        icon_layers: ITEM_LAYERS.to_vec(),
    });
    s.tick(&[]);
    let icon = s.part(5, "ShortcutIcon");
    let made =
        s.ui.node(icon)
            .and_then(|n| n.picture)
            .map(|p| p.id)
            .expect("an icon");
    let laid = s.ui.picture(made).expect("the icon laid together");
    let bottom =
        s.ui.assets()
            .image(ITEM_LAYERS[0].0)
            .expect("the bottom layer");
    assert_eq!((laid.width, laid.height), (bottom.width, bottom.height));
    assert_ne!(laid.bgra, bottom.bgra, "the top layer over the bottom one");
    assert_eq!(s.ui.element_tooltip(s.slot(5)), Some("Sword"));
    assert_eq!(s.text(s.part(5, "NumberDisplay")), "6");
}
