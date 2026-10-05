//! Clicks at the pointer on the buff/debuff, vitae, burden and link lamps open their panels:
//! enchantment rows by kind with duration option, vitae penalty text, character-sheet encumbrance
//! and resistance lines, link panel asks for a ping and shows ms/packet loss.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use crate::common::*;
use dereth_primitives::LocalTime;
use dereth_ui::{Delivery, ElemHandle, ElementId, Screen, UiSystem};
use dereth_ui_screens::panels::remaining::RemainingPanels;
use dereth_ui_screens::panels::{characterinfo, effects, linkstatus, vitae};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::{
    CharacterInfo, EffectEntry, GameView, PlayerOption, UiRequest, VitaeDisplay,
};

/// The effects-status indicator, `0x1000000C` = 1 — action `0x10000006`.
const BUFF_LAMP: ElementId = ElementId(0x1000_00F5);
/// The effects-status indicator, `0x1000000C` = 2 — action `0x10000007`.
const DEBUFF_LAMP: ElementId = ElementId(0x1000_00F6);
/// The vitae indicator — action `0x1000000C`.
const VITAE_LAMP: ElementId = ElementId(0x1000_00F4);
/// The burden indicator — action `0x10000005`, which opens the **character info** panel.
const BURDEN_LAMP: ElementId = ElementId(0x1000_00F7);
/// The link-status indicator — action `0x10000009`.
const LINK_LAMP: ElementId = ElementId(0x1000_00F8);

// ---------------------------------------------------------------------------------------------
// Harness — `panel_stack_visibility.rs`'s, plus `lamp_strip_hit_test.rs`'s pointer.
// ---------------------------------------------------------------------------------------------

fn env() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui
}

/// `UiFlow::deliver`, bounded, exactly as `panel_stack_visibility.rs` does it.
fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen) {
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            return;
        }
        for d in batch {
            if let Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            }
        }
    }
}

/// The shipped gameplay screen, pumped to quiescence, with `RemainingPanels` bound off its root
/// the way `dereth_client_shell::hud::Hud::drive` binds it.
fn screen() -> (UiSystem, GamePlayScreen, RemainingPanels) {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    pump(&mut ui, &mut s);
    let root = s.root().expect("the gameplay root");
    let mut panels = RemainingPanels::default();
    panels.post_init(&mut ui, root);
    ui.requests.clear();
    (ui, s, panels)
}

fn find(ui: &UiSystem, s: &GamePlayScreen, id: ElementId) -> ElemHandle {
    let root = s.root().expect("the gameplay root");
    ui.get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{:#010X} is not in the shipped tree", id.0))
}

fn visible(ui: &UiSystem, h: ElemHandle) -> bool {
    ui.node(h).expect("alive").region.flags.visible
}

/// The three events a player produces, and nothing else.
fn click(ui: &mut UiSystem, s: &mut GamePlayScreen, h: ElemHandle) {
    let b = ui.screen_box(h);
    assert!(b.is_valid(), "the lamp has no box to point at");
    let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    ui.mouse_move(LocalTime(0.0), x, y);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    pump(ui, s);
}

// ---------------------------------------------------------------------------------------------
// The view — real shapes, no network
// ---------------------------------------------------------------------------------------------

/// A `GameView` carrying nothing but the enchantment list the effects panels read.
///
/// The entries are the shape `dereth_client_runtime::hud::HudView::active_effects` produces from the
/// player's own enchantment registry joined to the shipped `SpellTable`: a spell id, its name,
/// its `_bitfield & 4`, and `(start + duration) - cur_time` already rebased on receipt.
#[derive(Debug, Default)]
struct Effects {
    list: Vec<EffectEntry>,
    spell_duration_option: bool,
}

impl GameView for Effects {
    fn active_effects(&self) -> Vec<EffectEntry> {
        self.list.clone()
    }
    fn player_option(&self, o: PlayerOption) -> bool {
        o == PlayerOption::SpellDuration && self.spell_duration_option
    }
}

fn entry(spell: u32, name: &str, beneficial: bool, remaining: f64) -> EffectEntry {
    EffectEntry {
        spell,
        name: name.to_owned(),
        description: format!("{name} description"),
        icon: None,
        beneficial,
        remaining,
        permanent: false,
        category: 0,
        power_level: 6,
    }
}

/// Two buffs and one debuff, deliberately given names whose alphabetical order is **not** their
/// id order, so that the panel's sorted insertion is observable.
fn three_enchantments() -> Vec<EffectEntry> {
    vec![
        entry(0x0101, "Strength Self VI", true, 1810.0),
        entry(0x0102, "Armor Self VI", true, 75.0),
        entry(0x0203, "Weakness Other VI", false, 3661.0),
    ]
}

// ---------------------------------------------------------------------------------------------
// 1. The buff lamp
// ---------------------------------------------------------------------------------------------

/// A click on the buff lamp fills the positive magic panel from the enchantment registry.
#[test]
fn a_click_on_the_buff_lamp_fills_the_positive_magic_panel_from_the_enchantment_registry() {
    let (mut ui, mut s, mut panels) = screen();
    let view = Effects {
        list: three_enchantments(),
        spell_duration_option: true,
    };

    let panel = find(&ui, &s, effects::HELPFUL_PANEL);
    assert!(
        !visible(&ui, panel),
        "setup_children leaves every registered page hidden"
    );
    assert_eq!(
        panels.effects_helpful.rows.len(),
        0,
        "nothing is drawn before the click"
    );

    let lamp = find(&ui, &s, BUFF_LAMP);
    light(&mut ui, lamp);
    click(&mut ui, &mut s, lamp);
    assert!(
        visible(&ui, panel),
        "the click has to open the panel before its contents can matter"
    );

    // The host's frame pull, exactly as `Hud::drive` makes it.
    panels.update(&mut ui, &view);

    assert_eq!(
        panels.effects_helpful.shown(),
        vec![0x0102, 0x0101],
        "the two helpful enchantments, ascending by spell NAME -- Armor before Strength, which is \
         the reverse of their id order"
    );
    let names: Vec<String> = panels
        .effects_helpful
        .rows
        .iter()
        .map(|r| r.name.clone())
        .collect();
    assert_eq!(
        names,
        vec!["Armor Self VI".to_owned(), "Strength Self VI".to_owned()]
    );
    // And the text really is on the elements, not only in the struct.
    for r in &panels.effects_helpful.rows {
        let label = ui
            .get_child_recursive(r.element, ElementId(effects::row::LABEL))
            .expect("every row template carries 0x1000012A");
        let drawn: String = ui
            .text_element_mut(label)
            .expect("a text element")
            .glyphs
            .glyphs
            .iter()
            .filter_map(|g| char::from_u32(u32::from(g.data)))
            .collect();
        assert_eq!(
            drawn, r.name,
            "the row's label element carries the spell name"
        );
    }
    // The effect region's two duration shapes, from the rebased remaining time.
    let durations: Vec<String> = panels
        .effects_helpful
        .rows
        .iter()
        .map(|r| r.duration.clone())
        .collect();
    assert_eq!(durations, vec!["1:15".to_owned(), "30:10".to_owned()]);
}

/// Behaviour: enchantments.lamp-panel.the-buff-and-debuff-lamps-each-list-only-their-own-kind
/// The debuff lamp opens the other instance and it shows only the harmful enchantment.
#[test]
fn the_debuff_lamp_opens_the_other_instance_and_it_shows_only_the_harmful_enchantment() {
    let (mut ui, mut s, mut panels) = screen();
    let view = Effects {
        list: three_enchantments(),
        spell_duration_option: true,
    };

    assert_eq!(
        panels.effects_helpful.ui_type,
        effects::kind::HELPFUL,
        "0x10000184 reads 1"
    );
    assert_eq!(
        panels.effects_harmful.ui_type,
        effects::kind::HARMFUL,
        "0x10000185 reads 2"
    );

    let lamp = find(&ui, &s, DEBUFF_LAMP);
    light(&mut ui, lamp);
    click(&mut ui, &mut s, lamp);
    assert!(visible(&ui, find(&ui, &s, effects::HARMFUL_PANEL)));
    panels.update(&mut ui, &view);

    assert_eq!(
        panels.effects_harmful.shown(),
        vec![0x0203],
        "the one harmful enchantment"
    );
    assert_eq!(
        panels.effects_helpful.shown(),
        Vec::<u32>::new(),
        "the positive panel is still closed, so it has drawn nothing at all"
    );
}

/// The empty case, which a rows-only test cannot tell from a broken one: with no enchantments the
/// panel opens, draws **zero** rows, and the info text `0x10000126` carries
/// `ID_Effects_Info_NoSpells` — the selection update's first arm.
#[test]
fn an_empty_registry_opens_the_panel_with_no_rows_and_the_no_spells_line() {
    let (mut ui, mut s, mut panels) = screen();
    let view = Effects::default();
    let lamp = find(&ui, &s, BUFF_LAMP);
    light(&mut ui, lamp);
    click(&mut ui, &mut s, lamp);
    panels.update(&mut ui, &view);

    assert!(visible(&ui, find(&ui, &s, effects::HELPFUL_PANEL)));
    assert_eq!(panels.effects_helpful.rows.len(), 0);
    assert_eq!(
        panels.effects_helpful.info,
        effects::string::NO_SPELLS,
        "with no string service installed `statmgmt::label` falls back to the token, which is what \
         the headless harness sees; `tests/g18_probe.rs` resolves it to \"NO SPELLS\" out of \
         0x23000001"
    );
}

/// A dark buff lamp still refuses its click and the panel stays empty.
#[test]
fn a_dark_buff_lamp_still_refuses_its_click_and_the_panel_stays_empty() {
    let (mut ui, mut s, mut panels) = screen();
    let view = Effects {
        list: three_enchantments(),
        spell_duration_option: true,
    };
    let lamp = find(&ui, &s, BUFF_LAMP);
    ui.set_state(
        lamp,
        dereth_ui::StateId(dereth_ui_screens::hud::indicators::STATE_NOTHING),
    );

    click(&mut ui, &mut s, lamp);
    let panel = find(&ui, &s, effects::HELPFUL_PANEL);
    assert!(
        !visible(&ui, panel),
        "a disabled button's click is swallowed before handle_button_click"
    );
    panels.update(&mut ui, &view);
    assert_eq!(
        panels.effects_helpful.rows.len(),
        0,
        "and an invisible panel does no work: the panel's update returns on its first line"
    );
}

/// The duration column is the **Spell Duration** character option's, and it is off by default:
/// the effect region's update returns before writing the value cell, so the
/// column is blank rather than `0:00`.
///
/// The rows are still there, which is what separates "the option is off" from "the panel is
/// broken".
#[test]
fn with_the_spell_duration_option_off_the_rows_are_there_and_the_duration_column_is_blank() {
    let (mut ui, mut s, mut panels) = screen();
    let view = Effects {
        list: three_enchantments(),
        spell_duration_option: false,
    };
    let lamp = find(&ui, &s, BUFF_LAMP);
    light(&mut ui, lamp);
    click(&mut ui, &mut s, lamp);
    panels.update(&mut ui, &view);

    assert_eq!(
        panels.effects_helpful.shown().len(),
        2,
        "both buffs are listed"
    );
    for r in &panels.effects_helpful.rows {
        assert_eq!(r.duration, "", "the spell-duration display option is off");
    }
}

// ---------------------------------------------------------------------------------------------
// 2. The vitae lamp
// ---------------------------------------------------------------------------------------------

/// A `GameView` carrying only the three inputs the vitae panel's update reads.
#[derive(Debug)]
struct Vitae(Option<VitaeDisplay>);

impl GameView for Vitae {
    fn vitae_display(&self) -> Option<VitaeDisplay> {
        self.0
    }
    fn vitae(&self) -> Option<f32> {
        self.0.map(|d| d.multiplier)
    }
}

/// Behaviour: vitae.lamp.lights-for-a-live-penalty-and-the-panel-says-how-much
/// A click on the vitae lamp composes the penalty text from the pool and the threshold.
#[test]
fn a_click_on_the_vitae_lamp_composes_the_penalty_text_from_the_pool_and_the_threshold() {
    let (mut ui, mut s, mut panels) = screen();
    let view = Vitae(Some(VitaeDisplay {
        multiplier: 0.95,
        cp_pool: 1_000,
        threshold: 3_476,
    }));

    let panel = find(&ui, &s, vitae::PANEL);
    assert!(!visible(&ui, panel));
    assert_eq!(
        panels.vitae.updates, 0,
        "nothing is written before the click"
    );

    let lamp = find(&ui, &s, VITAE_LAMP);
    light(&mut ui, lamp);
    click(&mut ui, &mut s, lamp);
    assert!(visible(&ui, panel), "the click has to open the panel first");

    panels.update(&mut ui, &view);
    assert_eq!(panels.vitae.updates, 1, "one write");
    assert_eq!(panels.vitae.penalty_percent, 5, "100 - (int)(0.95 * 100)");
    let t = &panels.vitae.text;
    assert!(t.contains(vitae::string::VITAE), "the first StringInfo");
    assert!(t.contains(vitae::string::SKILLS), "the second");
    assert!(t.contains(vitae::string::EXPERIENCE), "the third");
    assert!(
        t.contains("[5]"),
        "%Percentage, twice -- headless fall-back shape: {t}"
    );
    assert!(
        t.contains("[2,476]"),
        "%Experience = threshold - pool, grouped: {t}"
    );
    assert!(
        !t.contains(vitae::string::FULL),
        "and not the no-penalty line"
    );
}

/// The other arm, and the one that shows the panel is reading a value rather than printing a
/// constant: with no vitae at all the vitae value is **1.0**, `100 - 100` is `0`, and
/// `Update`'s `if (pct < 1)` writes `ID_Vitae_Text_Full` and nothing else.
#[test]
fn with_no_vitae_at_all_the_panel_says_full_strength_and_composes_no_numbers() {
    let (mut ui, mut s, mut panels) = screen();
    let view = Vitae(Some(VitaeDisplay {
        multiplier: 1.0,
        cp_pool: 0,
        threshold: 3_476,
    }));
    let lamp = find(&ui, &s, VITAE_LAMP);
    light(&mut ui, lamp);
    click(&mut ui, &mut s, lamp);
    panels.update(&mut ui, &view);

    assert_eq!(panels.vitae.penalty_percent, 0);
    assert_eq!(panels.vitae.text, vitae::string::FULL);
}

/// The null player-description-interface arm: before `0x0013` the client's update returns
/// without touching the main text. A panel that claimed full health in that state would be
/// telling a character who has just died that they are fine.
///
/// **This is a control and it passes in both worlds**, like
/// [`a_dark_buff_lamp_still_refuses_its_click_and_the_panel_stays_empty`]: it asserts an absence,
/// so it was green in the red run too. It is here to stop the two tests above from being made
/// green by a panel that writes unconditionally.
#[test]
fn before_the_player_description_arrives_the_vitae_panel_writes_nothing() {
    let (mut ui, mut s, mut panels) = screen();
    let view = Vitae(None);
    let lamp = find(&ui, &s, VITAE_LAMP);
    light(&mut ui, lamp);
    click(&mut ui, &mut s, lamp);
    panels.update(&mut ui, &view);

    assert!(
        visible(&ui, find(&ui, &s, vitae::PANEL)),
        "the panel is open"
    );
    assert_eq!(
        panels.vitae.updates, 0,
        "and empty, because Update wrote nothing"
    );
    assert_eq!(panels.vitae.text, "");
}

// ---------------------------------------------------------------------------------------------
// 3. The burden lamp -- which opens the character info panel, not a burden panel
// ---------------------------------------------------------------------------------------------

/// A game view carrying only the character-info panel's six sections' inputs.
#[derive(Debug)]
struct Info(Option<CharacterInfo>);

impl GameView for Info {
    fn character_info(&self) -> Option<CharacterInfo> {
        self.0.clone()
    }
}

/// A character with real numbers: innate 60/55/50/45/40/35, one death, a chess rank and a fishing
/// skill, carrying 9,000 burden units against a capacity of 7,500 (load 1.2) with two
/// augmentations.
fn a_character() -> CharacterInfo {
    CharacterInfo {
        innate: [60, 55, 50, 45, 40, 35],
        chess_rank: 1_400,
        fishing_skill: 0,
        num_deaths: 1,
        strength: 190,
        endurance: 180,
        load: 1.2,
        encumbrance: 9_000,
        capacity: 7_500,
        augmentations: 2,
        ..CharacterInfo::default()
    }
}

/// **The burden lamp's panel, and the premise it corrects.** the layout description: the burden lamp opens
/// the character-info panel `0x10000183`, which is a whole character sheet — six sections, of which
/// encumbrance is one.
///
/// A click at the pointer over the (always lit) burden lamp opens it and the sheet carries the
/// deaths line, the two resistance bands, all six innate attributes, chess and fishing, and both
/// load lines.
#[test]
fn a_click_on_the_burden_lamp_opens_the_character_sheet_and_encumbrance_is_one_of_its_sections() {
    let (mut ui, mut s, mut panels) = screen();
    let view = Info(Some(a_character()));

    let panel = find(&ui, &s, characterinfo::PANEL);
    assert!(!visible(&ui, panel));
    assert_eq!(panels.character_info.updates, 0);

    // The burden lamp never shows STATE_NOTHING -- `burden_state` maps "no value" and
    // "under-burdened" to the same 0x0E -- so it is clickable without being lit first.
    let lamp = find(&ui, &s, BURDEN_LAMP);
    click(&mut ui, &mut s, lamp);
    assert!(
        visible(&ui, panel),
        "the burden lamp opens the character info panel"
    );

    panels.update(&mut ui, &view);
    assert_eq!(panels.character_info.updates, 1);
    // Six sections, not five: the panel's fifth appended section is included as well.
    assert_eq!(
        panels.character_info.sections.len(),
        6,
        "the six sections composes"
    );

    let t = &panels.character_info.text;
    assert!(
        t.contains(characterinfo::string::DEATHS_ONE),
        "died once: {t}"
    );
    assert!(
        t.contains(characterinfo::string::INNATES),
        "the innate block: {t}"
    );
    assert!(
        t.contains("60|55|50|45|40|35"),
        "six innates in display order: {t}"
    );
    assert!(t.contains(characterinfo::string::CHESS), "chess: {t}");
    assert!(t.contains("[1,400]"), "the chess rank, grouped: {t}");
    // Encumbrance: 9,000 - 7,500 = 1,500 units over. The penalty is **30**, not 20, and that is
    // the client's own single-precision truncation rather than a rounding choice made here:
    // `2.0f - 1.2f` is `0.79999995`, `* 10.0f` is `7.9999995`, truncation takes 7, and
    // `(10 - 7) * 10` is 30. An implementation that did the arithmetic in `f64` would print 20.
    assert!(
        t.contains(characterinfo::string::LOAD_BURDENED),
        "over capacity: {t}"
    );
    assert!(
        t.contains("1,500|30"),
        "burden units and the penalty percentage: {t}"
    );
    assert!(
        t.contains(characterinfo::string::LOAD_AUGMENTATIONS),
        "and the augmentation line"
    );
    assert!(
        t.contains("2|40"),
        "two augmentations, +40% carrying capacity: {t}"
    );
}

/// The resistance bands are read off the character's attributes and not printed as a constant:
/// `strength + endurance` picks the first word and `strength + 2 * endurance` the second, and the
/// two ladders have **different bounds**, so one character can sit in two different bands.
///
/// 190 + 180 = 370 is `Hardy` on the first ladder; 190 + 360 = 550 is also `Hardy` on the second,
/// so the pair is driven a second time with a character who splits them.
#[test]
fn the_two_resistance_lines_come_from_the_two_different_ladders() {
    let (mut ui, mut s, mut panels) = screen();
    let lamp = find(&ui, &s, BURDEN_LAMP);
    click(&mut ui, &mut s, lamp);

    let mut c = a_character();
    c.strength = 100;
    c.endurance = 100;
    panels.update(&mut ui, &Info(Some(c.clone())));
    let line = &panels.character_info.sections[1];
    assert!(
        line.contains("None|Poor"),
        "sum 200 is None on the first ladder and 300 is Poor on the second: {line}"
    );

    // And a second station, so the line is shown to move: 250 -> "Poor", 450 -> "Mediocre".
    c.strength = 150;
    c.endurance = 100;
    panels.character_info.recv_changed();
    panels.update(&mut ui, &Info(Some(c)));
    let line = &panels.character_info.sections[1];
    assert!(line.contains("Poor|Mediocre"), "sum 250 and 350: {line}");
}

/// The unburdened arm, which is the one a normal character sees: `load < 1.0` writes
/// `ID_CharacterInfo_Load_None` and no numbers, and a character with no augmentations gets no
/// augmentation line at all.
///
/// **This is a control**: it asserts that two lines are *absent*, so it passes in both worlds.
#[test]
fn an_unburdened_character_gets_the_short_load_line_and_no_augmentation_line() {
    let (mut ui, mut s, mut panels) = screen();
    let mut c = a_character();
    c.load = 0.4;
    c.augmentations = 0;
    let lamp = find(&ui, &s, BURDEN_LAMP);
    click(&mut ui, &mut s, lamp);
    panels.update(&mut ui, &Info(Some(c)));

    let t = &panels.character_info.text;
    assert!(t.contains(characterinfo::string::LOAD_NONE));
    assert!(!t.contains(characterinfo::string::LOAD_BURDENED));
    assert!(!t.contains(characterinfo::string::LOAD_AUGMENTATIONS));
}

// ---------------------------------------------------------------------------------------------
// 4. The link-status lamp, and both halves of the ping
// ---------------------------------------------------------------------------------------------

/// A `GameView` with a clock and a `0x01EA` arrival counter.
///
/// **No datagram leaves this process.** `ping_returns` is a plain counter this file moves by hand
/// — the same thing `crate::net::ping_holder` is written by when a real `0x01EA` is decoded — and
/// the request half is asserted against `dereth_ui_screens::requests`, i.e. against *what would be
/// sent*, never by sending it.
#[derive(Debug, Default)]
struct Link {
    now: f64,
    returns: u64,
}

impl GameView for Link {
    fn now(&self) -> f64 {
        self.now
    }
    fn ping_returns(&self) -> u64 {
        self.returns
    }
}

/// Opening the link status panel is what asks the server for a ping.
#[test]
fn opening_the_link_status_panel_is_what_asks_the_server_for_a_ping() {
    let (mut ui, mut s, mut panels) = screen();
    let view = Link {
        now: 100.0,
        returns: 0,
    };

    ui.requests.clear();
    let lamp = find(&ui, &s, LINK_LAMP);
    click(&mut ui, &mut s, lamp);
    assert!(
        visible(&ui, find(&ui, &s, linkstatus::PANEL)),
        "the panel opens"
    );
    assert_eq!(
        panels.link_status.pings_requested, 0,
        "and nothing has been asked for until the frame loop runs"
    );

    panels.update(&mut ui, &view);
    assert_eq!(panels.link_status.pings_requested, 1);
    let asked = ui.requests.take();
    assert!(
        asked.iter().any(|r| matches!(r, UiRequest::RequestPing)),
        "the panel emits the request the host turns into 0x01E9: {asked:?}"
    );

    // A second frame with nothing changed must not ask again: the gate is 120 s.
    panels.update(
        &mut ui,
        &Link {
            now: 101.0,
            returns: 0,
        },
    );
    assert_eq!(panels.link_status.pings_requested, 1, "the 120-second gate");
}

/// **The answer half.** Until a `0x01EA` comes back the ping line shows the client's own
/// `"????"`; the arrival turns it into a millisecond figure, and the figure is the round trip the
/// panel measured rather than anything the message carried (its body is empty).
///
/// Two stations on the same panel, because a ping arriving is an edge.
#[test]
fn a_returned_ping_turns_the_four_question_marks_into_a_millisecond_figure() {
    let (mut ui, mut s, mut panels) = screen();
    let lamp = find(&ui, &s, LINK_LAMP);
    click(&mut ui, &mut s, lamp);

    // Station one: the panel is up, the request has gone, no answer yet.
    panels.update(
        &mut ui,
        &Link {
            now: 100.0,
            returns: 0,
        },
    );
    assert!(panels.link_status.ping_round_trip <= 0.0);
    assert!(
        panels.link_status.text.contains(linkstatus::UNKNOWN),
        "before the answer: {}",
        panels.link_status.text
    );

    // Station two: one `0x01EA`, 90 ms later.
    panels.update(
        &mut ui,
        &Link {
            now: 100.09,
            returns: 1,
        },
    );
    assert!(
        (panels.link_status.ping_round_trip - 0.09).abs() < 1e-6,
        "cur_time - the last ping request time, in seconds: {}",
        panels.link_status.ping_round_trip
    );
    let t = &panels.link_status.text;
    assert!(
        t.contains("[90]"),
        "90 ms, from the seconds-to-milliseconds scale: {t}"
    );
    assert!(
        !t.contains(&format!("{}{}", linkstatus::string::PING, "[????]")),
        "and the ping line is no longer unknown: {t}"
    );
}

/// Behaviour: link.packet-loss.the-line-always-carries-a-number-and-it-is-the-ping-that-can-be-unknown
/// The packet loss line is a number and the ping line is the one with the marker.
#[test]
fn the_packet_loss_line_is_a_number_and_the_ping_line_is_the_one_with_the_marker() {
    let (mut ui, mut s, mut panels) = screen();
    let lamp = find(&ui, &s, LINK_LAMP);
    click(&mut ui, &mut s, lamp);
    panels.update(
        &mut ui,
        &Link {
            now: 5.0,
            returns: 0,
        },
    );

    let t = &panels.link_status.text;
    for token in [
        linkstatus::string::INFO,
        linkstatus::string::COLORS,
        linkstatus::string::DISCONNECT,
        linkstatus::string::PACKET_LOSS,
        linkstatus::string::PING,
    ] {
        assert!(t.contains(token), "{token} is missing from the panel: {t}");
    }
    assert!(
        !t.contains(&format!(
            "{}[{}]",
            linkstatus::string::PACKET_LOSS,
            linkstatus::UNKNOWN
        )),
        "the loss line must never carry the ping's marker: {t}"
    );
    assert!(
        t.contains(&format!("{}[1.00]", linkstatus::string::PACKET_LOSS)),
        "the sentinel, formatted with the float variable's precision of 2: {t}"
    );
    assert!(
        t.contains(&format!(
            "{}[{}]",
            linkstatus::string::PING,
            linkstatus::UNKNOWN
        )),
        "the ping is genuinely unknown until 0x01EA comes back: {t}"
    );
}

mod description_scroll {
    //! The Effects panel's description pane names its own scrollbar, reports its text extent, shows the
    //! bar when the text overflows and hides it when it fits; the bottom arrow scrolls the end into
    //! view.
    //! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

    use crate::common::layout::RegistrationOrder;

    use dereth_primitives::LocalTime;
    use dereth_ui::{Delivery, ElemHandle, ElementId, Screen, UiSystem};
    use dereth_ui_screens::panels::effects;
    use dereth_ui_screens::panels::remaining::RemainingPanels;
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;
    use dereth_ui_screens::view::{EffectEntry, GameView, PlayerOption};

    /// The Effects indicator, `0x1000000C` = 1 — the lamp that opens `0x10000184`.
    const BUFF_LAMP: ElementId = ElementId(0x1000_00F5);
    /// The info text — the description pane.
    const INFO_TEXT: ElementId = effects::INFO_TEXT;
    /// The pane's own vertical bar, named by its attribute `0x72`. Measured, not assumed.
    const INFO_SCROLLBAR: ElementId = ElementId(0x1000_0127);
    /// The scrolling area's **decrement** button, attribute `0x78`: the arrow at the
    /// *bottom* of a vertical bar, which raises `0x0E` with a delta the client
    /// does **not** negate, so it walks the content forward.
    const ARROW_DOWN: ElementId = ElementId(0x1000_0071);

    /// The last words of the description this file gives its spell. They sit below the bottom edge of
    /// a 78 px pane and only a scroll can bring them inside.
    const MARKER: &str = "ZZTAILZZ";

    // ---------------------------------------------------------------------------------------------
    // Harness — the shipped gameplay tree, `lamp_panels.rs`'s, plus the panel fan-out
    // ---------------------------------------------------------------------------------------------

    fn env() -> UiSystem {
        let (ui, _flow, _store) =
            crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
        ui
    }

    /// One enchantment, whose description is long enough to overflow a 78 px pane many times over.
    #[derive(Debug, Default)]
    struct Effects {
        list: Vec<EffectEntry>,
    }

    impl GameView for Effects {
        fn active_effects(&self) -> Vec<EffectEntry> {
            self.list.clone()
        }
        fn player_option(&self, o: PlayerOption) -> bool {
            o == PlayerOption::SpellDuration
        }
    }

    /// A long spell description — twenty-odd lines of
    /// prose against a 280 px pane, ending in [`MARKER`].
    fn long_description() -> String {
        let mut s = String::new();
        for i in 0..24 {
            s.push_str(&format!(
                "Increases the target's Strength by a considerable amount, line {i} of the wordy \
                 description that the shipped spell table hands the panel. "
            ));
        }
        s.push_str(MARKER);
        s
    }

    fn one_buff(description: String) -> Vec<EffectEntry> {
        vec![EffectEntry {
            spell: 0x0101,
            name: "Strength Self VI".to_owned(),
            description,
            icon: None,
            beneficial: true,
            remaining: 1810.0,
            permanent: false,
            category: 0,
            power_level: 6,
        }]
    }

    struct Harness {
        ui: UiSystem,
        screen: GamePlayScreen,
        panels: RemainingPanels,
        view: Effects,
    }

    impl Harness {
        fn new(description: String) -> Self {
            let mut ui = env();
            let mut screen = GamePlayScreen::default();
            screen
                .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
                .expect("the gameplay screen builds from the shipped layout");
            let mut h = Self {
                ui,
                screen,
                panels: RemainingPanels::default(),
                view: Effects {
                    list: one_buff(description),
                },
            };
            h.pump();
            let root = h.screen.root().expect("the gameplay root");
            h.panels.post_init(&mut h.ui, root);
            h.ui.requests.clear();
            h
        }

        /// The screen **and** `RemainingPanels` both see every element message, exactly as
        /// `dereth_client_shell::hud::Hud::drive` arranges.
        fn pump(&mut self) {
            for _ in 0..16 {
                let batch = self.ui.drain_outbox();
                if batch.is_empty() {
                    return;
                }
                for d in batch {
                    if let Delivery::Element { msg, .. } = d {
                        self.screen.on_element_message(
                            &mut dereth_ui::framework::ScreenCx::new(&mut self.ui),
                            &msg,
                        );
                        self.panels
                            .on_element_message(&mut self.ui, &msg, &self.view);
                    }
                }
            }
        }

        fn find(&self, id: ElementId) -> ElemHandle {
            let root = self.screen.root().expect("the gameplay root");
            self.ui
                .get_child_recursive(root, id)
                .unwrap_or_else(|| panic!("{:#010X} is not in the shipped tree", id.0))
        }

        /// The pane, resolved **inside the helpful panel** — the two instances share every child id.
        fn pane(&self) -> ElemHandle {
            let p = self.find(effects::HELPFUL_PANEL);
            self.ui
                .get_child_recursive(p, INFO_TEXT)
                .expect("info text 0x10000126")
        }

        fn bar(&self) -> ElemHandle {
            let p = self.find(effects::HELPFUL_PANEL);
            self.ui
                .get_child_recursive(p, INFO_SCROLLBAR)
                .expect("the pane's bar 0x10000127")
        }

        fn visible(&self, h: ElemHandle) -> bool {
            self.ui.node(h).expect("alive").region.flags.visible
        }

        fn click_at(&mut self, x: i32, y: i32) {
            self.ui.mouse_move(LocalTime(0.0), x, y);
            self.ui
                .mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
            self.ui
                .mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
            self.pump();
            let mut back = dereth_ui::RecordingDrawBackend::default();
            self.ui.draw(&mut back);
        }

        fn click(&mut self, h: ElemHandle) {
            let b = self.ui.screen_box(h);
            assert!(b.is_valid(), "nothing to point at");
            self.click_at((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        }

        /// Put the lamp in a lit state the way its own `Update` does — four of the six rest in
        /// `0x0D`, which is the button's **disabled** attribute, and retail refuses a dark
        /// lamp's click.
        fn light(&mut self, h: ElemHandle) {
            self.ui.set_state(h, dereth_ui::StateId(1));
        }

        fn frame(&mut self) {
            let view = Effects {
                list: self.view.list.clone(),
            };
            self.panels.update(&mut self.ui, &view);
            self.pump();
            let mut back = dereth_ui::RecordingDrawBackend::default();
            self.ui.draw(&mut back);
        }

        /// The scrollable height against the pane's own height — the two numbers
        /// the scrollbar sizer compares to decide whether the bar has anything to do.
        fn extent(&mut self) -> (i32, i32) {
            let h = self.pane();
            let view = self.ui.screen_box(h).height();
            (
                self.ui
                    .text_element_mut(h)
                    .expect("a text element")
                    .scroll
                    .height,
                view,
            )
        }

        fn offset(&mut self) -> i32 {
            let h = self.pane();
            self.ui
                .text_element_mut(h)
                .expect("a text element")
                .scroll
                .y
        }

        /// Every character of the info text whose composed cell lies **inside the pane's box**, in
        /// glyph order — what a player can actually read, with the scroll offset applied.
        fn readable(&mut self) -> String {
            let h = self.pane();
            let box_ = self.ui.screen_box(h);
            let t = self.ui.text_element_mut(h).expect("a text element");
            t.compose(box_)
                .into_iter()
                .filter(|g| g.y >= box_.y0 && g.y < box_.y1)
                .filter_map(|g| char::from_u32(u32::from(g.ch)))
                .collect()
        }

        /// Open the panel with a real click on the lamp, take a frame so the list fills, then select
        /// the one row with a real press inside the list box.
        fn open_and_select(&mut self) {
            let lamp = self.find(BUFF_LAMP);
            self.light(lamp);
            self.click(lamp);
            self.frame();
            assert_eq!(
                self.panels.effects_helpful.rows.len(),
                1,
                "the click must have filled the list before a row can be pressed"
            );
            let row = self.panels.effects_helpful.rows[0].element;
            self.click(row);
            assert_eq!(
                self.panels.effects_helpful.selected_spell, 0x0101,
                "the press inside the list box must have selected the row under the mouse"
            );
        }
    }

    // =============================================================================================
    // 1. The instrument, and the fact the probe read off the shipped tree
    // =============================================================================================

    /// The pane names a bar, and the bar is a real scrollbar. If this ever fails the
    /// defect is in the layout and not in this code.
    #[test]
    fn the_description_pane_names_a_bar_of_its_own() {
        let mut h = Harness::new(long_description());
        h.open_and_select();

        let bar = h.bar();
        assert_eq!(
            h.ui.node(bar).expect("a node").ty().0,
            0x0B,
            "0x10000127 is a scrollbar element"
        );
        let pane = h.pane();
        let s = h.ui.text_element_mut(pane).expect("a text element").scroll;
        assert_eq!(
            s.v_scrollbar,
            Some(INFO_SCROLLBAR),
            "attribute 0x72 binds the pane to 0x10000127"
        );
    }

    /// The description pane reports the extent of the text it holds.
    #[test]
    fn the_description_pane_reports_the_extent_of_the_text_it_holds() {
        let mut h = Harness::new(long_description());
        h.open_and_select();

        let (content, view) = h.extent();
        assert!(
            content > view,
            "twenty-four lines of description do not fit a {view} px pane, but the pane reports a \
             content height of {content}: resize_scrollable_area never ran after the text was set"
        );
    }

    /// Behaviour: enchantments.lamp-panel.a-long-description-shows-its-scrollbar-and-a-short-one-hides-it
    /// *"Beneficial Spells: doesn't show scrollbar (when necessary) for spell description area."*
    #[test]
    fn a_description_longer_than_the_pane_shows_the_bar() {
        let mut h = Harness::new(long_description());
        h.open_and_select();

        let bar = h.bar();
        assert!(
            h.visible(bar),
            "the pane overflows, so scrollbar sizing must clear 0x76 and hide-when-disabled must \
             let the bar draw"
        );
    }

    /// The negative control: a description that fits reports an extent that fits, and retail's
    /// `hide-when-disabled` keeps the bar off the screen. A fix that simply showed the bar always
    /// would redden this.
    #[test]
    fn a_description_that_fits_keeps_its_bar_hidden() {
        let mut h = Harness::new("Increases Strength.".to_owned());
        h.open_and_select();

        let (content, view) = h.extent();
        assert!(
            content <= view,
            "one line of description fits a {view} px pane, not {content}"
        );
        assert!(
            !h.visible(h.bar()),
            "0x10000127 ships 0x76 disabled and 0x79 hide-when-disabled; nothing should have shown it"
        );
    }

    /// **The gesture.** A click at the screen coordinates of the bar's bottom arrow, hit-tested by
    /// `UiSystem`'s own pipeline, and the bottom of the description comes into view.
    #[test]
    fn a_click_on_the_bars_bottom_arrow_brings_the_end_of_the_description_into_view() {
        let mut h = Harness::new(long_description());
        h.open_and_select();

        let before = h.readable();
        assert!(
            !before.contains(MARKER),
            "the instrument is pointed at nothing: {MARKER} is already readable before any scroll:\n\
             {before}"
        );

        let bar = h.bar();
        let arrow =
            h.ui.get_child_recursive(bar, ARROW_DOWN)
                .expect("the bar's decrement button");
        // One click is one line, so the arrow is clicked until the
        // offset stops moving — the scroll setter's clamp is what stops it, and reaching
        // that clamp is the bottom of the description.
        let mut clicks = 0;
        loop {
            let before_offset = h.offset();
            h.click(arrow);
            clicks += 1;
            if h.offset() == before_offset || clicks >= 400 {
                break;
            }
        }

        assert!(
            h.offset() > 0,
            "{clicks} clicks on the bottom arrow moved the scroll offset not at all"
        );
        let after = h.readable();
        assert!(
            after.contains(MARKER),
            "the end of the description is still below the fold after {clicks} arrow clicks:\n{after}"
        );
    }
}

mod spell_selection {
    use crate::common::widget_fixture::*;

    /// A press on a spell row lands on the list box and not on the row.
    #[test]
    fn a_press_on_a_spell_row_lands_on_the_list_box_and_not_on_the_row() {
        let view = Effects {
            list: three_buffs(),
        };
        let (mut ui, mut s, mut panels) = screen(&view);
        open_buff_panel(&mut ui, &mut s, &mut panels, &view);

        let rows = &panels.effects_helpful.rows;
        assert_eq!(rows.len(), 3, "three helpful enchantments, three rows");
        let row = rows[1].element;
        assert!(
            !ui.node(row).expect("alive").is_mouse_visible,
            "a list row is not mouse-visible in retail either -- the press is *meant* to hit the box"
        );
        let at = centre(&ui, row);
        let hit = ui
            .hit_test_screen(at.0, at.1)
            .expect("something is under the pointer");
        assert_eq!(
            ui.node(hit).expect("alive").element_id(),
            effects::LIST_BOX,
            "the hit test resolves a press over a spell row to the list box"
        );
    }

    /// Behaviour: enchantments.lamp-panel.a-press-on-a-spell-row-selects-and-describes-it-and-a-second-clears-it
    /// *"Beneficial-spell LAMP lists spells that cannot be clicked."* A real press at the row's own
    /// screen centre selects it — moves the row to state `6` and
    /// the info text becomes the spell's name and description.
    #[test]
    fn a_real_press_on_a_spell_row_selects_it_and_describes_the_spell() {
        let view = Effects {
            list: three_buffs(),
        };
        let (mut ui, mut s, mut panels) = screen(&view);
        open_buff_panel(&mut ui, &mut s, &mut panels, &view);

        assert_eq!(
            panels.effects_helpful.selected_spell, 0,
            "nothing is selected before the press"
        );
        let row = panels.effects_helpful.rows[1].element;
        let want = panels.effects_helpful.rows[1].spell;
        let name = panels.effects_helpful.rows[1].name.clone();
        let at = centre(&ui, row);
        click(&mut ui, &mut s, &mut panels, &view, at);

        assert_eq!(
            panels.effects_helpful.selected_spell, want,
            "a press at the second row's own screen centre selected nothing"
        );
        assert!(
            panels.effects_helpful.info.starts_with(&name),
            "the selection update's selected arm writes the spell name, a blank line, then the \
             spell description; it reads {:?}",
            panels.effects_helpful.info
        );
        assert_eq!(
            ui.node(row).expect("alive").state.0,
            effects::row_state::SELECTED,
            "the selected row goes to state 6 and the others stay at 1"
        );
    }

    /// Setting the selected spell to the one already selected zeroes it — pressing the
    /// selected row again clears it, which is the client's own toggle and not a convenience.
    #[test]
    fn a_second_press_on_the_selected_spell_row_clears_the_selection() {
        let view = Effects {
            list: three_buffs(),
        };
        let (mut ui, mut s, mut panels) = screen(&view);
        open_buff_panel(&mut ui, &mut s, &mut panels, &view);

        let row = panels.effects_helpful.rows[0].element;
        let at = centre(&ui, row);
        click(&mut ui, &mut s, &mut panels, &view, at);
        assert_ne!(
            panels.effects_helpful.selected_spell, 0,
            "the first press selects"
        );
        click(&mut ui, &mut s, &mut panels, &view, at);
        assert_eq!(
            panels.effects_helpful.selected_spell, 0,
            "the second press clears"
        );
    }
}
