//! `VitalsPanel`, `FloatingVitals`, `FloatingSideVitals` — the health / stamina / mana bars.

use dereth_primitives::ObjectId;
use dereth_ui::{ElemHandle, UiSystem};

use crate::bind::{attr, set_attr_float, Bound};
use crate::view::{GameView, Vital};

/// The label string every bar shows, with integer variables for the current and maximum — i.e. the
/// `"cur/max"` with no spaces. The shipped table enum `0x10000001` resolves to 0x23000001;
/// row 0x059385AC has fragments ["", "/", ""] and variables [0x48A2, 0x5168] (Cur, Max).
pub const VITAL_BAR_LABEL: &str = "ID_Vitals_VitalBarLabel";

/// The meter and label child of one bar, in the panel's post-init order.
#[must_use]
pub const fn fields(v: Vital) -> (&'static str, &'static str) {
    match v {
        Vital::Health => ("health_meter", "health_label"),
        Vital::Stamina => ("stamina_meter", "stamina_label"),
        Vital::Mana => ("mana_meter", "mana_label"),
    }
}

/// The fill level the update writes to the meter's attribute `0x69`: `cur/max`.
///
/// A zero maximum would divide by zero in the client; the guard here returns 0.0, which is the
/// value a meter with no data shows.
#[must_use]
pub fn meter_level(cur: u32, max: u32) -> f32 {
    dereth_presentation::stats::meter_level(cur, max)
}

/// Read the three vitals and drive the three meters.
///
/// Driven by the quality-changed and player-description-received notices, **not by
/// a timer** — there is no per-frame walk, and a panel that expects one silently never updates.
pub fn update(ui: &mut UiSystem, bound: &Bound, view: &dyn GameView, player: ObjectId) {
    for v in Vital::ALL {
        let (meter, _label) = fields(v);
        let Some((cur, max)) = view.vital(player, v) else {
            continue;
        };
        if let Some(h) = bound.get(meter) {
            set_attr_float(ui, h, attr::METER_LEVEL, meter_level(cur, max));
        }
    }
}

/// The label text for one bar, as [`VITAL_BAR_LABEL`] renders with the current and maximum.
#[must_use]
pub fn label_text(cur: u32, max: u32) -> String {
    format!("{cur}/{max}")
}

/// The vitals panel's element-message handler — the vitals bar's two authored
/// presentations, and the click that swaps them.
///
/// # It is not a hover, and not a param map
///
/// The side-by-side bar does not "expand on hover", and the press action does not pick the state
/// (`7 -> one state`, `10 -> the other`):
///
/// * The handler delegates directly to the base vitals implementation, and **all three**
///   vitals classes share it, so the stacked `<VITS>`
///   (`0x100005FA`) toggles too, not only the side-by-side `<SIDE>` (`0x100006D5`).
/// * It is not a hover and it is not a param map. `0x1C` is **mouse press**, `7` and `10` are two
///   of the four press actions, and the arm does the same thing for either — it reads the
///   element's **own current state** and flips it: on a mouse press with action 7 or 10, the new
///   state is `0x10000007` if the current one is `0x10000006`, and `0x10000006` otherwise; every
///   message then falls through to the base handler.
///
/// So a state that is *neither* — the state 0 an element starts in — becomes `0x10000006`, and a
/// second press then gives `0x10000007`. Clicking the vitals bar toggles between the numerical
/// display and bars with a glyph in the centre of each: **both presentations are authored**, on
/// both windows, and reading them off the shipped
/// tree shows `0x10000006` and `0x10000007` declared on the window, on each of the three meter
/// groups, and on the label under each — which is why one state change swaps the whole element.
/// The state change is what carries it down, through the `pass_to_children` rule; this module
/// needs no part of that.
///
/// The same set-state call is what the book panel's paging buttons use — see
/// [`crate::panels::book`].
pub mod vitals_display {
    use dereth_ui::StateId;
    /// The first of the two press actions the arm accepts.
    pub const PARAM_PRESS_A: u32 = 7;
    /// The second (`0xa`) — the arm does not distinguish them.
    pub const PARAM_PRESS_B: u32 = 10;
    /// The two authored states. Which is "numeric" and which is "graphical" is the layout's, not
    /// the code's: the client names neither and simply alternates.
    pub const STATE_A: StateId = StateId(0x1000_0006);
    /// See [`STATE_A`].
    pub const STATE_B: StateId = StateId(0x1000_0007);

    /// Whether a `0x1C` with this first parameter reaches the toggle at all.
    #[must_use]
    pub fn is_toggle_press(param: u32) -> bool {
        param == PARAM_PRESS_A || param == PARAM_PRESS_B
    }

    /// One equality test against `0x10000006` — the whole of the arm's arithmetic.
    ///
    /// **Anything that is not [`STATE_A`] answers [`STATE_A`]**, including the state 0 an element
    /// that has never been pressed is in. That is one comparison and not a two-way map, which is
    /// what makes the first press deterministic.
    #[must_use]
    pub fn next_state(current: StateId) -> StateId {
        if current == STATE_A {
            STATE_B
        } else {
            STATE_A
        }
    }
}

/// The gameplay screen's player-option-changed notice — the `SideBySideVitals` character
/// option "shows one while hiding the other".
pub fn apply_side_by_side_option(
    ui: &mut UiSystem,
    stacked: Option<ElemHandle>,
    side_by_side: Option<ElemHandle>,
    side_by_side_on: bool,
) {
    if let Some(h) = stacked {
        ui.set_visible(h, !side_by_side_on);
    }
    if let Some(h) = side_by_side {
        ui.set_visible(h, side_by_side_on);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// the current/max `Attribute2ndType` pairs and
    /// the `cur/max` fill.
    #[test]
    fn each_bars_level_is_the_documented_ratio_of_its_two_stats() {
        assert_eq!(Vital::Health.stats(), (2, 1));
        assert_eq!(meter_level(50, 100), 0.5);
        assert_eq!(meter_level(0, 100), 0.0);
        assert_eq!(meter_level(100, 100), 1.0);
        // A missing maximum must not divide by zero.
        assert_eq!(meter_level(7, 0), 0.0);
        assert_eq!(label_text(50, 100), "50/100");
        assert_eq!(VITAL_BAR_LABEL, "ID_Vitals_VitalBarLabel");
    }

    /// Oracle: the panel's six-row child table at post-init — the meter/label pairing per bar.
    #[test]
    fn the_three_bars_bind_the_documented_meter_and_label_fields() {
        assert_eq!(fields(Vital::Health), ("health_meter", "health_label"));
        assert_eq!(fields(Vital::Stamina), ("stamina_meter", "stamina_label"));
        assert_eq!(fields(Vital::Mana), ("mana_meter", "mana_label"));
        let spec = crate::panels::catalogue::spec("VitalsPanel").unwrap();
        for v in Vital::ALL {
            let (m, l) = fields(v);
            assert!(spec.children.iter().any(|c| c.field == m), "{m}");
            assert!(spec.children.iter().any(|c| c.field == l), "{l}");
        }
    }

    /// **A-F26 replaced `the_side_by_side_bar_toggles_between_its_two_states_on_hover`.** That
    /// test asserted `state_for(7) == STATE_B` and `state_for(10) == STATE_A` — a param-to-state
    /// map against a *hover*. The handler is a **press**, it
    /// treats 7 and 10 identically, and it reads the element's own state to decide; there is no
    /// direction in it at all. The function the old test pinned had **no caller outside that
    /// test**, so nothing in the tree could contradict it. The independent UI-bit evidence names
    /// both states.
    ///
    /// Oracle: the arm's logic, stated in the module doc.
    #[test]
    fn a_press_flips_the_vitals_bar_between_its_two_authored_states() {
        use vitals_display as v;
        // One comparison: anything that is not STATE_A answers STATE_A, and only
        // STATE_A answers STATE_B.
        assert_eq!(v::next_state(v::STATE_A), v::STATE_B);
        assert_eq!(v::next_state(v::STATE_B), v::STATE_A);
        assert_eq!(
            v::next_state(dereth_ui::StateId(0)),
            v::STATE_A,
            "an element that has never been pressed starts in state 0, and one comparison makes the \\
             first press deterministic"
        );
        // Two presses return to where they started -- it is a toggle and not a latch.
        assert_eq!(v::next_state(v::next_state(v::STATE_A)), v::STATE_A);
        // Action 7, then action 10 -- and nothing else reaches the arm.
        assert!(v::is_toggle_press(7) && v::is_toggle_press(10));
        assert!(!v::is_toggle_press(0) && !v::is_toggle_press(8) && !v::is_toggle_press(11));
    }

    /// Oracle: the player-option-changed notice — exactly one of the two
    /// vitals windows is visible at a time.
    #[test]
    fn the_side_by_side_option_shows_one_window_and_hides_the_other() {
        let mut ui = UiSystem::new((800, 600));
        let a = ui.create_hollow(None);
        let b = ui.create_hollow(None);
        apply_side_by_side_option(&mut ui, Some(a), Some(b), true);
        assert!(!ui.node(a).unwrap().region.flags.visible);
        assert!(ui.node(b).unwrap().region.flags.visible);
        apply_side_by_side_option(&mut ui, Some(a), Some(b), false);
        assert!(ui.node(a).unwrap().region.flags.visible);
        assert!(!ui.node(b).unwrap().region.flags.visible);
    }
}
