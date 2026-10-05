//! The client's four set-combat-mode notice handlers — without them entering combat opens no
//! combat UI.
//!
//! Setting the combat mode raises the set-combat-mode notice,
//! immediately after re-reading the advanced-combat-mode option and immediately before the input
//! maps are re-registered. The notice goes to every registered handler that does not veto it.
//! Four classes handle it:
//!
//! | handler | what it does |
//! |---|---|
//! | toolbar | marks the toolbar active when `mode != MAGIC`, re-numbers every quickbar slot, and shows exactly one of the four stance icons `0x10000192`…`0x10000195` |
//! | combat window page | **shows the combat cluster** and picks its state |
//! | spellcasting panel | shows the casting cluster in magic mode |
//! | combat panel | clears its default panel on leaving combat |
//!
//! The first is implemented, as a poll, in
//! [`crate::screens::gameplay::GamePlayScreen::update_indicators`] and
//! [`crate::toolbar::shortcuts`]; the other three are here.
//!
//! ## How the combat UI actually opens, end to end
//!
//! `<COMB>` (`0x100006B5`, element type `0x10000054`) starts **hidden** and is never shown
//! directly by player input. It becomes visible through this sequence:
//!
//! 1. The missile-mode handler writes state `0x10000004`, then shows element `0x1000005C`, the
//!    first page inside `<COMB>`.
//! 2. The element base's visibility write broadcasts element message `0x18`.
//! 3. The inherited panel handler finds that child in its page table and raises a panel-visibility
//!    notice using the child's current visibility.
//! 4. The notice handler shows that page, hides the page it covered, and shows `<COMB>` itself.
//!    The corresponding hide path hides the wrapper when no page remains visible.
//!
//! The two page ids and their panel ids were read off the shipped `0x21000005`:
//! `0x1000005C` carries panel id **17** and `0x10000061` panel id **22**, and all twenty-three
//! panel ids in the three stacks are distinct, so the fan-out in
//! `GamePlayScreen::recv_set_panel_visibility` cannot cross-talk.
//!
//! ## What is a deviation and what is not
//!
//! This build has no notice bus that reaches a UI element — `dereth_ui::NoticeBus` exists,
//! `UiSystem::send_notice` has one production caller and `Screen::on_notice` has **zero**
//! overrides — so all four handlers are driven from one edge on `GameView::combat_mode()`, the
//! same place the four stance icons are driven from. That is the declared
//! deviation, and it is the same one the six indicator lamps carry. Everything below the edge is
//! the client's own code.

use dereth_ui::{ElemHandle, ElementId, StateId, UiSystem};

/// The combat page: the first page inside `<COMB>`, panel id 17.
pub const COMBAT_UI_PAGE: ElementId = ElementId(0x1000_005C);
/// The spellcasting page: the second page inside `<COMB>`, panel id 22.
pub const SPELLCASTING_PAGE: ElementId = ElementId(0x1000_0061);

/// The two states the combat-mode handler writes.
///
/// **The shipped layout carries exactly these two and no others on `0x1000005C`** — an
/// independent confirmation from the dat that the two values the handler writes are element states and not
/// something else that happens to look like an id.
pub const MELEE_STATE: StateId = StateId(0x1000_0003);
/// See [`MELEE_STATE`].
pub const MISSILE_STATE: StateId = StateId(0x1000_0004);

/// The recklessness field, bound by the combat panel's post-init with a recursive child search
/// for `0x100005EF` — which then **hides it** immediately.
pub const RECKLESSNESS_FIELD: ElementId = ElementId(0x1000_05EF);

/// The skill the combat-mode notice handler's melee arm asks about.
///
/// Defined in [`dereth_client_contract::combat_notice`], because `dereth_client_shell::hud` is what asks
/// the qualities. `0x32` is `Recklessness`.
pub use dereth_client_contract::combat_notice::RECKLESSNESS_SKILL;

/// The skill advancement class: `Undef` 0, `Untrained` 1, `Trained` 2, `Specialized` 3. The
/// handler's
pub const SAC_TRAINED: u32 = 2;

/// The four `COMBAT_MODE` bits, re-exported so a call site here never spells one.
pub use crate::toolbar::combat_mode::{MAGIC, MELEE, MISSILE, NONCOMBAT};

/// What the combat-mode notice handler decided, so a test can read the halves separately
/// rather than only their effect on the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CombatUiArm {
    /// The page's own visibility.
    pub visible: bool,
    /// The page state, when the arm writes one. The hide arms write none.
    pub state: Option<StateId>,
    /// The recklessness field's visibility, when the arm writes one. **Only the melee arm
    /// does**; missile and the two hide arms leave the field where post-init put it.
    pub recklessness_visible: Option<bool>,
}

/// The combat panel's set combat mode notice, decided.
///
/// If the advanced combat UI option is on, the page is hidden. In melee it takes state
/// `0x10000003`, becomes visible, and shows the recklessness field only when the player's
/// advancement class in skill `0x32` is at least trained (2). In missile it takes state
/// `0x10000004` and becomes visible. Every other mode hides it.
///
/// **The advanced check is on the option, not on `CombatState::advanced_combat_mode`.** The
/// handler reads the option itself rather than the copy
/// the combat-mode setter just took, so a stale copy cannot leave the classic cluster up in the advanced
/// UI. Reproduced: the caller passes the option.
///
/// The advanced arm is a plain hide — the advanced combat interface is a **different** window, and
/// the classic cluster simply gets out of its way.
#[must_use]
pub fn combat_ui_arm(mode: u32, advanced_combat_ui: bool, recklessness_sac: u32) -> CombatUiArm {
    if advanced_combat_ui {
        return CombatUiArm {
            visible: false,
            state: None,
            recklessness_visible: None,
        };
    }
    match mode {
        MELEE => CombatUiArm {
            visible: true,
            state: Some(MELEE_STATE),
            recklessness_visible: Some(recklessness_sac >= SAC_TRAINED),
        },
        MISSILE => CombatUiArm {
            visible: true,
            state: Some(MISSILE_STATE),
            recklessness_visible: None,
        },
        _ => CombatUiArm {
            visible: false,
            state: None,
            recklessness_visible: None,
        },
    }
}

/// Apply [`combat_ui_arm`] to the tree in the client's order: the state before the visibility.
///
/// Reversing the two lines changes nothing observable in this build because
/// `UiSystem::set_state` raises no element message and `set_visible`'s `0x18` is *queued*, so
/// nothing runs between them; the page is already in its new state by the time any handler sees
/// the visibility change. In the client the two are synchronous and the order decides whether the
/// panel's show path can see the outgoing state for an instant. The order becomes observable if
/// this build gains a synchronous notice bus.
pub fn apply_combat_ui_arm(
    ui: &mut UiSystem,
    page: ElemHandle,
    recklessness: Option<ElemHandle>,
    arm: CombatUiArm,
) {
    if let Some(s) = arm.state {
        ui.set_state(page, s);
    }
    ui.set_visible(page, arm.visible);
    if let (Some(h), Some(v)) = (recklessness, arm.recklessness_visible) {
        ui.set_visible(h, v);
    }
}

/// The spellcasting panel's set combat mode notice.
///
/// In magic mode the panel becomes visible and refreshes its endowment icon and its cast button's
/// tooltip; in every other mode it is hidden.
///
/// Returns whether the two refreshes run, which is exactly `mode == MAGIC`; the caller owns
/// them because the endowment icon needs the player's held item and this module has
/// no view.
#[must_use]
pub fn spellcasting_arm(mode: u32) -> bool {
    mode == MAGIC
}

/// The combat panel's set-combat-mode notice, the whole of it: when the mode is
/// `NONCOMBAT` (1) it clears the stored default panel, and otherwise does nothing.
///
/// The stored default panel is the page used when the current one is hidden.
/// **Leaving combat forgets the fallback page.**
///
/// It does not "clear a cached target": there is no target on this class.
///
/// **This arm is unfalsifiable against the shipped layout, and that is a fact about the client
/// rather than about the test.** The default-page field is only ever written by the
/// panel-visibility notice, which sets it to the shown page when the entry's "should be default"
/// flag is set, and the combat panel's child set-up writes that flag as a literal **false** for
/// both of its two pages. So on this layout the field is null for the life of the session and
/// clearing it is a no-op. It is transcribed anyway, because a third-party layout pack can only
/// reach this class through the same two page ids and the flag is per-entry, not per-class.
#[must_use]
pub fn combat_panel_clears_default_page(mode: u32) -> bool {
    mode == NONCOMBAT
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the combat-mode notice handler's four arms. The literals are pinned here
    /// independently of the implementation constants so a wrong constant cannot validate itself.
    #[test]
    fn the_combat_cluster_opens_in_melee_and_missile_and_in_no_other_mode() {
        assert_eq!(MELEE_STATE, StateId(0x1000_0003));
        assert_eq!(MISSILE_STATE, StateId(0x1000_0004));
        assert_eq!(COMBAT_UI_PAGE, ElementId(0x1000_005C));
        assert_eq!(SPELLCASTING_PAGE, ElementId(0x1000_0061));
        assert_eq!(RECKLESSNESS_FIELD, ElementId(0x1000_05EF));
        assert_eq!(RECKLESSNESS_SKILL, 0x32);
        assert_eq!(SAC_TRAINED, 2);

        // Melee: shown, state 0x10000003, recklessness gated on the skill.
        let a = combat_ui_arm(MELEE, false, 3);
        assert_eq!(
            a,
            CombatUiArm {
                visible: true,
                state: Some(MELEE_STATE),
                recklessness_visible: Some(true)
            }
        );
        assert_eq!(
            combat_ui_arm(MELEE, false, 1).recklessness_visible,
            Some(false),
            "untrained"
        );
        assert_eq!(
            combat_ui_arm(MELEE, false, 0).recklessness_visible,
            Some(false),
            "undef"
        );
        assert_eq!(
            combat_ui_arm(MELEE, false, 2).recklessness_visible,
            Some(true),
            "exactly 2"
        );

        // Missile: shown, state 0x10000004, and the recklessness field is **not touched**.
        assert_eq!(
            combat_ui_arm(MISSILE, false, 3),
            CombatUiArm {
                visible: true,
                state: Some(MISSILE_STATE),
                recklessness_visible: None
            }
        );

        // Peace and magic: hidden, no state written.
        for m in [NONCOMBAT, MAGIC] {
            assert_eq!(
                combat_ui_arm(m, false, 3),
                CombatUiArm {
                    visible: false,
                    state: None,
                    recklessness_visible: None
                },
                "{m:#X}"
            );
        }
        // The advanced UI hides it in every mode, including the two that would open it.
        for m in [NONCOMBAT, MELEE, MISSILE, MAGIC] {
            assert_eq!(
                combat_ui_arm(m, true, 3),
                CombatUiArm {
                    visible: false,
                    state: None,
                    recklessness_visible: None
                },
                "advanced {m:#X}"
            );
        }
    }

    /// Oracle: the spellcasting panel's set combat mode notice, which tests the mode against 8.
    #[test]
    fn the_casting_cluster_opens_only_in_magic() {
        assert_eq!(MAGIC, 8);
        assert!(spellcasting_arm(MAGIC));
        for m in [NONCOMBAT, MELEE, MISSILE] {
            assert!(!spellcasting_arm(m), "{m:#X}");
        }
    }

    /// Oracle: the combat panel's set combat mode notice, which tests the mode against 1.
    #[test]
    fn only_leaving_combat_clears_the_default_page() {
        assert_eq!(NONCOMBAT, 1);
        assert!(combat_panel_clears_default_page(NONCOMBAT));
        for m in [MELEE, MISSILE, MAGIC] {
            assert!(!combat_panel_clears_default_page(m), "{m:#X}");
        }
    }
}
