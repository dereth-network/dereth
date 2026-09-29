//! `VitaePanel` — the panel the vitae lamp opens.
//!
//! Retail's vitae panel is element type `0x10000020`, instance `0x1000018A`, with one child: the
//! main text `0x100001C3`, a `TextElement` [measured on the shipped layout].
//!
//! A `panels::catalogue` row alone is not a panel: without this constructor, clicking the lamp
//! opens an empty box.
//!
//! # What the vitae panel's update does
//!
//! Nothing while the panel is hidden. Otherwise the penalty percentage is `100 - trunc(vitae *
//! 100)`. Below 1 the text is `ID_Vitae_Text_Full` ("…is at full strength."). Otherwise it reads
//! int qualities `VitaeCpPool` (129) and `DeathLevel` (139), falling back to `Level` (25) when the
//! death level is 0, computes `need = threshold(vitae, level) - pool`, and writes
//! `ID_Vitae_Text_Vitae` and `ID_Vitae_Text_Skills` (each with `%Percentage` = the percentage)
//! followed by `ID_Vitae_Text_Experience` (with `%Experience` = `need`).
//!
//! **The three inputs.** `dereth_client_model::advancement::vitae_cp_pool_threshold` is the
//! client's vitae CP-pool threshold. `GameView::vitae` is the vitae-value query that also drives
//! the lamp. The two int qualities `VitaeCpPool` (129, `0x81`) and `DeathLevel` (139, `0x8B`) are
//! named in `dereth_client::hud` beside `LEVEL` (25, `0x19`).
//!
//! The threshold's constants are measured against **retail** rather than taken from ACE:
//! `(long)((pow(level, 2.5) * 2.5 + 20.0) * pow(vitae, 5.0) + 0.5)`, with `level` unsigned and
//! `vitae` an `f32` widened by the load.
//!
//! # The three strings, and how the numbers get into them
//!
//! Each is a `StringInfo` in table enum `0x10000001` (`0x23000001`) holding **two literal pieces
//! around one variable**, read out of the shipped dats
//! [verified against the shipped string table]:
//!
//! ```text
//! ID_Vitae_Text_Full       ["Your Vitae, or life force, is at full strength."]
//! ID_Vitae_Text_Vitae      ["Due to your recent death, you have temporarily lost ",
//!                           "% of your Vitae, or life force."]
//! ID_Vitae_Text_Skills     ["\n\nThis means that your health, stamina, mana, and skills are
//!                           temporarily reduced by ",
//!                           "%.  A reduction of less than 15% will not hinder you much, …"]
//! ID_Vitae_Text_Experience ["\n\nYou will regain 1% of your Vitae once you earn ",
//!                           " more experience."]
//! ```
//!
//! so the composed line is `pieces[0] + value + pieces[1]`, and the value goes through
//! [`super::statmgmt::num`] because every one of them is an integer variable and the long-integer
//! string conversion groups the digits (see [`super::numfmt`]).

use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::view::GameView;

/// The `VitaePanel` element — the `0x57` listener for input action `0x1000000C`
/// *"Show/Hide Vitae Panel"*, which the lamp at `0x100000F4` fires.
pub const PANEL: ElementId = ElementId(0x1000_018A);

/// The main text — the panel's one child binding at post-init.
pub const MAIN_TEXT: ElementId = ElementId(0x1000_01C3);

/// The four `StringInfo` tokens the update composes. Table enum `0x10000001`.
pub mod string {
    /// The no-penalty line.
    pub const FULL: &str = "ID_Vitae_Text_Full";
    /// `%Percentage` — the penalty percentage.
    pub const VITAE: &str = "ID_Vitae_Text_Vitae";
    /// `%Percentage` again, the same number.
    pub const SKILLS: &str = "ID_Vitae_Text_Skills";
    /// `%Experience` — the experience still needed to burn one point off.
    pub const EXPERIENCE: &str = "ID_Vitae_Text_Experience";
}

/// `VitaePanel`.
#[derive(Debug, Default)]
pub struct VitaePanel {
    /// The panel element. `None` when the layout has no such sub-panel.
    pub panel: Option<ElemHandle>,
    main_text: Option<ElemHandle>,
    /// The current vitae multiplier, as of the last write — 1.0 means no penalty.
    pub current_vitae: f32,
    /// What the main text last read — the pane read back without walking glyphs.
    pub text: String,
    /// `100 - trunc(vitae * 100)` as of the last write.
    pub penalty_percent: i32,
    /// How many times the update has written.
    pub updates: u32,
    /// The snapshot the last write was made from, so an unchanged frame writes nothing.
    seen: Option<(u32, i32, i32)>,
    was_visible: bool,
}

impl VitaePanel {
    /// The panel's post-init, minus the two notice registrations (player-description-received and
    /// vitae-changed) and the quality registration for int `0x81` — this build has no notice bus,
    /// and all three handlers' bodies are [`Self::recv_vitae_changed`].
    ///
    /// Bound off the screen root, because `0x1000018A` is a page of `<PANS>` and the child lookup
    /// is recursive.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        self.panel = ui.get_child_recursive(root, PANEL);
        let Some(p) = self.panel else { return };
        self.main_text = ui.get_child_recursive(p, MAIN_TEXT);
        self.current_vitae = 1.0;
        self.was_visible = self.visible(ui);
    }

    /// Whether `post_init` found the panel element at all.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.panel.is_some()
    }

    /// Whether the one child post-init binds is in the shipped layout too.
    #[must_use]
    pub fn fully_bound(&self) -> bool {
        self.panel.is_some() && self.main_text.is_some()
    }

    /// The panel's visibility — the update's first check.
    #[must_use]
    pub fn visible(&self, ui: &UiSystem) -> bool {
        self.panel
            .and_then(|h| ui.node(h))
            .is_some_and(|n| n.region.flags.visible)
    }

    /// The vitae-changed notice, the player-description-received notice, and the quality-changed
    /// handler for `VitaeCpPool`: all three re-read the value and run the update.
    pub fn recv_vitae_changed(&mut self) {
        self.seen = None;
    }

    /// One frame's drive.
    ///
    /// Returns true on a frame that rewrote the text. The visibility edge is polled for the reason
    /// [`super::effects::EffectsPanel::update`] gives: the element-message handler's `0x18` arm is
    /// what runs the update in the client, and a plain struct receives no `0x18`.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        if !self.fully_bound() {
            return false;
        }
        let visible = self.visible(ui);
        let shown_now = visible && !self.was_visible;
        self.was_visible = visible;
        if !visible {
            return false;
        }
        if shown_now {
            self.seen = None;
        }
        let d = view.vitae_display();
        let snapshot = d.map_or((0, 0, 0), |d| {
            (d.multiplier.to_bits(), d.cp_pool, d.threshold)
        });
        if self.seen == Some(snapshot) {
            return false;
        }
        self.seen = Some(snapshot);
        self.write(ui, d);
        true
    }

    /// The update's body, with the qualities already gathered.
    ///
    /// `None` is the client's no-player-description case, which returns **false** without touching
    /// the main text — so a panel opened before `0x0013` keeps whatever the layout gave it rather
    /// than claiming full health.
    pub fn write(&mut self, ui: &mut UiSystem, d: Option<crate::view::VitaeDisplay>) {
        let Some(d) = d else { return };
        self.current_vitae = d.multiplier;
        // Truncate `vitae * 100`, then `100 - that`. The multiply is done in `f32` and truncated,
        // exactly as retail does.
        let pct = 100 - dereth_primitives::num::to_i32(d.multiplier * 100.0);
        self.penalty_percent = pct;
        let text = if pct < 1 {
            label(ui, string::FULL)
        } else {
            // `threshold(vitae, level) - pool`, with the level fall-back already applied on the
            // host side (see `VitaeDisplay::threshold`).
            let need = i64::from(d.threshold) - i64::from(d.cp_pool);
            let mut s = compose_int(ui, string::VITAE, i64::from(pct));
            s.push_str(&compose_int(ui, string::SKILLS, i64::from(pct)));
            s.push_str(&compose_int(ui, string::EXPERIENCE, need));
            s
        };
        self.text.clone_from(&text);
        if let Some(t) = self.main_text.and_then(|h| ui.text_element_mut(h)) {
            t.set_text(&text);
        }
        self.updates += 1;
    }
}

/// One `StringInfo` with no variables, resolved from the string table.
fn label(ui: &UiSystem, token: &str) -> String {
    super::statmgmt::label(ui, token)
}

/// One `StringInfo` with a single integer variable, rendered with the meta-language pass on —
/// which is what the row needs: `ID_Vitae_Text_Skills` ships a double space that
/// the string renderer's excess-space trim removes and a plain `pieces[0] + value + pieces[1..]`
/// interleave would not.
///
/// The fall-back when no string service is installed is the token followed by the value in
/// brackets. It matters for the same reason [`super::statmgmt::label`]'s does: a headless panel
/// that silently drew nothing would be indistinguishable from one that was never written, and the
/// **number** is the half a test must be able to see.
#[must_use]
pub fn compose_int(ui: &UiSystem, token: &str, value: i64) -> String {
    let id = dereth_primitives::num::hash::str_hash(token.as_bytes());
    ui.resolve_string_rendered(
        super::statmgmt::STRING_TABLE,
        id,
        &[super::statmgmt::num(value)],
    )
    .unwrap_or_else(|| format!("{token}[{}]", super::statmgmt::num(value)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the update's `pct = 100 - trunc(vitae * 100)` and its `pct < 1` branch, at the
    /// boundary.
    ///
    /// A multiplier of exactly `1.0` gives `0`, which is `< 1` — the "full strength" arm. `0.95`
    /// is the one 5% death and gives `5`. The truncation matters: `0.999` gives `100 - 99 = 1`,
    /// which is **not** `< 1`, so a sliver of vitae still draws the three-part text.
    #[test]
    fn the_penalty_percentage_is_a_truncating_subtraction_from_a_hundred() {
        let pct = |m: f32| {
            #[allow(clippy::cast_possible_truncation)]
            let v = 100 - (m * 100.0) as i32;
            v
        };
        assert_eq!(pct(1.0), 0, "no vitae at all: the vitae value is 1.0");
        assert!(pct(1.0) < 1, "and 0 takes the full-strength arm");
        assert_eq!(pct(0.95), 5, "one death");
        // `0.9f32` is `0.89999997615…`, but the single-precision **product** rounds back to
        // exactly `90.0`, so this one is 10 and not 9. The truncation still bites elsewhere --
        // see the 0.999 row -- which is why every boundary is written out rather than reasoned
        // about.
        assert_eq!(pct(0.90), 10);
        assert_eq!(pct(0.999), 1, "still a penalty, and not < 1");
    }

    /// Oracle: `crate::panels::catalogue`'s `VitaePanel` row.
    #[test]
    fn the_one_bound_child_is_the_catalogued_one() {
        let spec = crate::panels::catalogue::spec("VitaePanel").expect("VitaePanel is catalogued");
        assert_eq!(spec.children.len(), 1);
        assert_eq!(spec.children[0].id, MAIN_TEXT);
    }
}
