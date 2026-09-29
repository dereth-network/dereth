//! Missile and melee open the combat cluster in their state and magic does not; the recklessness
//! meter shows only in melee when trained; the advanced-combat option keeps the classic cluster
//! down; the cluster declares exactly two states; 23 panel ids distinct.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_ui::{Delivery, ElemHandle, ElementId, Screen, StateId, UiSystem};
use dereth_ui_screens::hud::combat_notice as cn;
use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};
use dereth_ui_screens::view::GameView;

fn env() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui
}

fn screen() -> (UiSystem, GamePlayScreen) {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    pump(&mut ui, &mut s);
    ui.requests.clear();
    (ui, s)
}

/// `UiFlow::deliver`, bounded. `SetVisible` raises `0x18`, whose handler hides the page it covers,
/// which raises another `0x18`.
fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen) -> usize {
    let mut n = 0;
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            break;
        }
        for d in batch {
            if let Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
                n += 1;
            }
        }
    }
    n
}

fn h(ui: &UiSystem, s: &GamePlayScreen, id: u32) -> ElemHandle {
    let root = s.root().expect("the gameplay root");
    ui.get_child_recursive(root, ElementId(id))
        .unwrap_or_else(|| panic!("{id:#010X} is in the shipped classic_gameplay tree"))
}

fn visible(ui: &UiSystem, s: &GamePlayScreen, id: u32) -> bool {
    ui.node(h(ui, s, id)).expect("alive").region.flags.visible
}

fn state(ui: &UiSystem, s: &GamePlayScreen, id: u32) -> StateId {
    ui.node(h(ui, s, id)).expect("alive").state
}

/// A `GameView` that answers only the four questions this seam reads, so the test drives the same
/// entry point the running client drives (`GamePlayScreen::update_indicators`) rather than calling
/// the handler directly.
#[derive(Debug, Default, Clone, Copy)]
struct Mode {
    mode: u32,
    advanced: bool,
    recklessness_sac: u32,
}

impl GameView for Mode {
    fn combat_mode(&self) -> u32 {
        self.mode
    }
    fn advanced_combat_ui(&self) -> bool {
        self.advanced
    }
    fn recklessness_advancement_class(&self) -> u32 {
        self.recklessness_sac
    }
}

/// One frame of the seam: the mode edge, then the element messages it raised.
fn drive(ui: &mut UiSystem, s: &mut GamePlayScreen, v: Mode) {
    s.update_indicators(ui, &v);
    pump(ui, s);
}

// ---------------------------------------------------------------------------------------------
// 1. The defect, at two stations, per listener.
// ---------------------------------------------------------------------------------------------

/// Behaviour: combat.mode.a-change-in-the-model-opens-the-window-and-closing-combat-shuts-it
/// Entering missile mode opens the combat window and leaving closes it.
#[test]
fn entering_missile_mode_opens_the_combat_window_and_leaving_closes_it() {
    let (mut ui, mut s) = screen();

    // ---- Station A: peace. ------------------------------------------------------------------
    drive(
        &mut ui,
        &mut s,
        Mode {
            mode: cn::NONCOMBAT,
            ..Mode::default()
        },
    );
    assert!(
        !visible(&ui, &s, window::COMBAT_PANEL.0),
        "A: <COMB> is down at peace"
    );
    assert!(
        !visible(&ui, &s, cn::COMBAT_UI_PAGE.0),
        "A: the combat cluster is down"
    );
    assert!(
        !visible(&ui, &s, cn::SPELLCASTING_PAGE.0),
        "A: the casting cluster is down"
    );
    assert_eq!(s.combat_panel.current, None, "A: no page is current");

    // ---- Station B: missile. ----------------------------------------------------------------
    drive(
        &mut ui,
        &mut s,
        Mode {
            mode: cn::MISSILE,
            ..Mode::default()
        },
    );
    assert!(
        visible(&ui, &s, cn::COMBAT_UI_PAGE.0),
        "B: entering missile mode shows combat page 0x1000005C"
    );
    assert_eq!(
        state(&ui, &s, cn::COMBAT_UI_PAGE.0),
        cn::MISSILE_STATE,
        "B: and puts it in state 0x10000004, the missile one of the two the layout declares"
    );
    assert!(
        visible(&ui, &s, window::COMBAT_PANEL.0),
        "Combat window visibility: the 0x18 from that SetVisible reaches \
          and the panel-visibility handler puts <COMB> up"
    );
    assert_eq!(
        s.combat_panel.current,
        Some(cn::COMBAT_UI_PAGE),
        "B: and the stack knows which of its two pages is current"
    );
    assert!(
        !visible(&ui, &s, cn::SPELLCASTING_PAGE.0),
        "B: the casting cluster stays down in missile"
    );

    // ---- Station C: back to peace. ----------------------------------------------------------
    drive(
        &mut ui,
        &mut s,
        Mode {
            mode: cn::NONCOMBAT,
            ..Mode::default()
        },
    );
    assert!(
        !visible(&ui, &s, cn::COMBAT_UI_PAGE.0),
        "C: the cluster comes down"
    );
    assert!(
        !visible(&ui, &s, window::COMBAT_PANEL.0),
        "C: and with no page current, the panel-visibility handler hides <COMB>"
    );
    assert_eq!(s.combat_panel.current, None, "C: no page current again");
}

/// Melee is the other arm that opens the cluster, and it writes the **other** state. Missile and
/// melee differing is what makes the state write falsifiable at all: a handler that always wrote
/// `0x10000003` would pass the missile test above only if that test read the state, which it does.
#[test]
fn melee_opens_the_same_cluster_in_the_other_state_and_magic_does_not_open_it() {
    let (mut ui, mut s) = screen();

    drive(
        &mut ui,
        &mut s,
        Mode {
            mode: cn::MELEE,
            ..Mode::default()
        },
    );
    assert!(
        visible(&ui, &s, cn::COMBAT_UI_PAGE.0),
        "melee opens the cluster"
    );
    assert_eq!(
        state(&ui, &s, cn::COMBAT_UI_PAGE.0),
        cn::MELEE_STATE,
        "state 0x10000003"
    );
    assert!(
        visible(&ui, &s, window::COMBAT_PANEL.0),
        "and <COMB> with it"
    );

    // Magic: the melee/missile page hides and the spellcasting page shows. The window stays up,
    // because the stack swaps one page for the other -- a case a window-only assertion
    // cannot see, and the reason the page assertions are separate from the window's.
    drive(
        &mut ui,
        &mut s,
        Mode {
            mode: cn::MAGIC,
            ..Mode::default()
        },
    );
    assert!(
        !visible(&ui, &s, cn::COMBAT_UI_PAGE.0),
        "magic closes the combat cluster"
    );
    assert!(
        visible(&ui, &s, cn::SPELLCASTING_PAGE.0),
        "entering magic mode shows spellcasting page 0x10000061"
    );
    assert_eq!(
        s.combat_panel.current,
        Some(cn::SPELLCASTING_PAGE),
        "the casting page is current"
    );
    assert!(
        visible(&ui, &s, window::COMBAT_PANEL.0),
        "<COMB> stays up across the swap"
    );

    // And out again.
    drive(
        &mut ui,
        &mut s,
        Mode {
            mode: cn::NONCOMBAT,
            ..Mode::default()
        },
    );
    assert!(!visible(&ui, &s, cn::SPELLCASTING_PAGE.0));
    assert!(!visible(&ui, &s, window::COMBAT_PANEL.0));
    assert_eq!(s.combat_panel.current, None);
}

/// Behaviour: combat.window.the-recklessness-meter-appears-at-exactly-the-trained-class
/// The combat UI's post-init hides the recklessness field in its fourth step, and the melee
/// arm is the only one that ever shows it — gated on `Recklessness` being **trained or better**.
///
/// The shipped layout carries `0x100005EF` *visible*, so without the post-init hide the meter is
/// drawn in a bow stance for a character who has never trained the skill.
#[test]
fn the_recklessness_meter_is_hidden_until_melee_and_only_when_the_skill_is_trained() {
    let (mut ui, mut s) = screen();
    // Post-init's hide, before any mode edge has run.
    assert!(
        !visible(&ui, &s, cn::RECKLESSNESS_FIELD.0),
        "the recklessness field starts hidden"
    );

    // Missile does not touch it.
    drive(
        &mut ui,
        &mut s,
        Mode {
            mode: cn::MISSILE,
            recklessness_sac: 3,
            ..Mode::default()
        },
    );
    assert!(
        !visible(&ui, &s, cn::RECKLESSNESS_FIELD.0),
        "the missile arm writes no visibility"
    );

    // Melee, untrained: hidden.
    drive(
        &mut ui,
        &mut s,
        Mode {
            mode: cn::MELEE,
            recklessness_sac: 1,
            ..Mode::default()
        },
    );
    assert!(
        visible(&ui, &s, cn::COMBAT_UI_PAGE.0),
        "the cluster is up either way"
    );
    assert!(
        !visible(&ui, &s, cn::RECKLESSNESS_FIELD.0),
        "Untrained (1) is < 2"
    );

    // Melee, trained: shown. Same mode, so this is the second input's own edge.
    drive(
        &mut ui,
        &mut s,
        Mode {
            mode: cn::MELEE,
            recklessness_sac: 2,
            ..Mode::default()
        },
    );
    assert!(
        visible(&ui, &s, cn::RECKLESSNESS_FIELD.0),
        "Trained (2) is the boundary and passes"
    );

    drive(
        &mut ui,
        &mut s,
        Mode {
            mode: cn::MELEE,
            recklessness_sac: 0,
            ..Mode::default()
        },
    );
    assert!(
        !visible(&ui, &s, cn::RECKLESSNESS_FIELD.0),
        "Undef (0) hides it again"
    );
}

/// Behaviour: combat.advanced.the-option-suppresses-the-classic-combat-window
/// Enabling the advanced combat interface keeps the classic cluster down in every mode — the
/// advanced combat interface is a different window and this one gets out of its way.
///
/// This is the read that had no writer: `CombatState::advanced_combat_mode`'s only two writers in
/// the workspace were hand-writes inside `dereth-client-model`'s own tests. The handler reads the *option*
/// rather than that cached copy, and both now come from bit 12 of the first option word.
#[test]
fn the_advanced_combat_ui_option_keeps_the_classic_cluster_down_in_every_mode() {
    let (mut ui, mut s) = screen();
    for m in [cn::NONCOMBAT, cn::MELEE, cn::MISSILE, cn::MAGIC] {
        drive(
            &mut ui,
            &mut s,
            Mode {
                mode: m,
                advanced: true,
                recklessness_sac: 3,
            },
        );
        assert!(
            !visible(&ui, &s, cn::COMBAT_UI_PAGE.0),
            "advanced, mode {m:#X}"
        );
    }
    // …and with the option off, the same missile mode opens it. Without this half the test above
    // would pass against a handler that never shows the cluster at all.
    drive(
        &mut ui,
        &mut s,
        Mode {
            mode: cn::MISSILE,
            advanced: false,
            recklessness_sac: 3,
        },
    );
    assert!(
        visible(&ui, &s, cn::COMBAT_UI_PAGE.0),
        "the calibration: the option is what held it down"
    );
    assert!(visible(&ui, &s, window::COMBAT_PANEL.0));
}

// ---------------------------------------------------------------------------------------------
// 2. The layout facts the handlers rest on, read from the dat rather than asserted about it.
// ---------------------------------------------------------------------------------------------

/// The two states the combat-mode notice handler writes are **the only two `0x1000005C` declares**.
///
/// Setting a state records state **0** for an id the element does not declare, so
/// a wrong state constant here would be invisible in every visibility assertion above and would
/// simply draw the wrong artwork. This reads the state table straight out of the shipped layout.
#[test]
fn the_combat_cluster_declares_exactly_the_two_states_the_handler_writes() {
    let (ui, _s) = screen();
    let n = ui.node(h(&ui, &_s, cn::COMBAT_UI_PAGE.0)).expect("alive");
    let mut states: Vec<u32> = n.desc.states.keys().map(|s| s.0).collect();
    states.sort_unstable();
    assert_eq!(
        states,
        vec![cn::MELEE_STATE.0, cn::MISSILE_STATE.0],
        "classic_gameplay gives 0x1000005C exactly the melee and missile states"
    );
}

/// The twenty three panel ids across the three stacks are all distinct.
#[test]
fn the_twenty_three_panel_ids_across_the_three_stacks_are_all_distinct() {
    let (_ui, s) = screen();
    let mut ids: Vec<u32> = s
        .panels
        .pages
        .iter()
        .chain(s.env_panel.pages.iter())
        .chain(s.combat_panel.pages.iter())
        .map(|p| p.panel_id)
        .collect();
    assert_eq!(ids.len(), 23, "16 + 5 + 2");
    let n = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), n, "no two pages share a panel id: {ids:?}");
    assert!(!ids.contains(&0), "and none is the ignored id 0");

    let combat: Vec<u32> = s.combat_panel.pages.iter().map(|p| p.panel_id).collect();
    assert_eq!(
        combat,
        vec![17, 22],
        "0x1000005C is 17 and 0x10000061 is 22"
    );
}
