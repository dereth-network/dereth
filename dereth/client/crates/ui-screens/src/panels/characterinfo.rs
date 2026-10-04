//! `CharacterInfoPanel` — the panel the **burden** lamp opens.
//!
//! The panel has element type `0x1000001A`, instance `0x10000183`, one child: the info text `0x1000011D`, a
//! `TextElement` [measured on the shipped layout].
//!
//! **It is not a burden panel.** The burden lamp opens the character info panel, which is where
//! encumbrance is shown as text. The load section is one of six, and the other five have nothing to do with encumbrance.
//!
//! # The character info panel's update, the composer
//!
//! A hidden panel does nothing. Otherwise the client clears the info text to an empty string and
//! appends six sections, with a literal `"\n"` after each but the last: birth, age and deaths;
//! endurance; innate attributes; the fake skills; augmentations; load.
//!
//! Six sections into one text element, separated by newlines — and each section appends its own
//! trailing `\n` inside its `StringInfo`, so the blank line between sections is the separator's.
//!
//! # What is written here
//!
//! | section | here | state |
//! |---|---|---|
//! | birth, age and deaths | `birth_age_deaths` | **written**, all four arms |
//! | endurance | `endurance` | **written**, checked against retail |
//! | innate attributes | `innates` | **written**, checked against retail |
//! | fake skills | `fake_skills` | **written** |
//! | augmentations | `augmentations` | **written**, checked against retail |
//! | load | `load` | **written**, checked against retail |
//!
//! # Birth, played and augmentations
//!
//! **Birth.** The arm reads int `0x62` (`CreationTimestamp`) and gates on whether the quality is
//! **present**, not on its value. It then formats it with `localtime` and `strftime` into a
//! `0x400` buffer with `"%c"`.
//!
//! **That `%c` is not `asctime`.** The client calls `setlocale(LC_ALL, "English")` before
//! anything draws, so it runs in `English_United States.1252` and `%c` is that locale's
//! `M/d/yyyy h:mm:ss tt` — and the retail C runtime's *C*-locale `%c` is not `asctime` either.
//! The formatter is [`crate::ctime::strftime_c`], measured against the retail C runtime itself,
//! and the zone shift arrives on [`crate::view::CharacterInfo::utc_offset_secs`] rather than
//! being left at UTC.
//!
//! **Played.** Int `0x7D` (`Age`), broken down by [`query_duration`].
//! That breakdown divides the seconds by `0x1E13380 / 0x278D00 / 0x93A80 / 0x15180 /
//! 0xE10 / 0x3C` and adds **seven** variables to a `StringInfo` on table enum **2** — the value
//! when the term is non-zero and an **empty string** when it is not. The row it names,
//! `ID_DurationFormat`, is the first one this workspace composes that carries real metalanguage
//! markup, which is why [`dereth_ui::text::metalanguage`] exists. See
//! [`query_duration`].
//!
//! **Augmentations.** It is not 6 KB of per-augmentation strings: it is **three mastery lines**,
//! an unconditional header, **eleven** luminance ratings and **43** augmentation rows, every one
//! of them a single string shown when its int quality is above zero. [`LUMINANCE`] and
//! [`AUGMENTATIONS`] are those tables, in the order retail shows them, with the quality id each
//! row reads.
//!
//! The masteries are **not** in the fake-skills section, though that is a reasonable guess.
//! `ID_CharacterInfo_Mastery_Melee` / `_Ranged` / `_Summoning` belong to the augmentations
//! section: they are its first three lines, ahead of the luminance header.
//!
//! # The augmentations section
//!
//! - Melee (int `0x162`), ranged (`0x163`) and summoning (`0x16A`) mastery each show one line
//!   when above zero, with `%Mastery` named from the value: melee over 1..=11, ranged over
//!   8..=12, summoning 1/2/3 -> Primalist/Necromancer/Naturalist.
//! - The luminance header, unconditional and with no variable.
//! - The four rated pairs (`0x14D`, `0x14E`, `0x14F`, `0x150`): a value above 5 shows the base
//!   line with `%NumAugmentations = 5` and the specialised line with the excess; 1..=5 shows only
//!   the base line; zero or less shows nothing.
//! - The seven single ratings and the 43 augmentations: each shows its line, with
//!   `%NumAugmentations` the value, when the value is above zero.
//!
//! The specialised count is reset to zero before **every** pair, so a specialised rating cannot
//! leak into the next pair's line — checked because the shape of the logic invites exactly that
//! bug.
//!
//! # The three sections
//!
//! ### The innate attribute info update
//!
//! The queries run in display order: ids 1, 2, 4, 3, 5 and 6 supply Strength, Endurance,
//! Coordination, Quickness, Focus and Self, respectively.
//!
//! The value shown is each attribute's **innate** (initial) level, which is what the string's
//! *"Innate Strength: "* says, and not the level raised with experience. Note the id order
//! `1, 2, 4, 3, 5, 6`: attribute `3` is Quickness and `4` is Coordination, and the *display*
//! order puts Coordination first.
//!
//! ### The endurance info update
//!
//! Strength (`1`) and Endurance (`2`) are read raw; the resist word is a band over `s + e` and
//! the regeneration word a band over `s + 2 * e`, each added as a string variable, with the two
//! ladders sharing six words: `"None"`, `"Poor"`, `"Mediocre"`, `"Hardy"`, `"Resilient"`,
//! and `"Indomitable"`. See [`resist_band`] and [`regeneration_band`] for the bounds.
//!
//! ### The load update
//!
//! The section reads the load, Strength (not raw, defaulting to 10), the encumbrance value (int
//! `5`, `EncumbranceVal`, default 0) and the carrying-capacity augmentations (int `230`,
//! `AugIncCarryCap`, default 0), and computes the capacity from strength and augmentations. A
//! load under `1.0` shows `Load_None`; otherwise `Load_Burdened`, with the burden
//! `encumbrance - capacity` and the penalty `(10 - trunc(load_mod(load) * 10.0)) * 10`.
//! Independently, when the augmentations are above zero, `Load_Augmentations` shows their count
//! and the additional load `augs * 20`.
//!
//! The `augs * 20` is a **percentage**, and it agrees with the capacity formula's own
//! arithmetic: `min(augs * 30, 150) * strength` over a base of `strength * 150` is `augs * 20 %`.
//!
//! **The penalty is computed in `f32` and that is visible on screen.** `(10 - trunc(load_mod(load)
//! * 10.0)) * 10` at a load of `1.2` gives **30**, not 20: `2.0f - 1.2f` is `0.79999995`,
//! `* 10.0f` is `7.9999995`, and the truncation takes `7`. An implementation that did the
//! arithmetic in `f64` would print a different number for most loads, so [`burden_penalty_percent`]
//! keeps every step in `f32` and its test pins the two cases that separate the readings.
//!
//! # The strings, all from table enum `0x10000001` (`0x23000001`)
//!
//! Read out of the shipped dats; each is stored as the
//! literal pieces around its variables, so a line is `pieces[0] + v0 + pieces[1] + v1 + …`.
//!
//! `ID_CharacterInfo_Resists` has **four**
//! literal pieces — *"Natural Resistances: "*, *"\nDrain Resistances: "*, *"\nRegeneration Bonus:
//! "*, *"\n"* — i.e. three variable slots, and the endurance section supplies exactly **two**
//! string variables, so one variable has to fill two slots. The row's own variable list reads
//! `RESIST, RESIST, REGEN` (it stores the three names' string hashes; `"RESIST"` hashes to
//! `0x056A7E84`), and the lookup answers once per entry — so Natural and Drain really do show
//! the same word and the call site supplies it once. See [`var`].

use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::view::{CharacterInfo, GameView};

/// The `CharacterInfoPanel` element — the `0x57` listener for input action `0x10000005`
/// *"Show/Hide Character Info Panel"*, which the **burden** lamp at `0x100000F7` fires.
pub const PANEL: ElementId = ElementId(0x1000_0183);

/// The info text — the one child the post-init binds.
pub const INFO_TEXT: ElementId = ElementId(0x1000_011D);

pub use dereth_presentation::character::{
    band, burden_penalty_percent, melee_mastery_name, prop, query_duration, ranged_mastery_name,
    regeneration_band, resist_band, string, summoning_mastery_name, var, AUGMENTATIONS,
    DURATION_TABLE, LUMINANCE,
};
/// `CharacterInfoPanel`.
#[derive(Debug, Default)]
pub struct CharacterInfoPanel {
    /// The panel element. `None` when the layout has no such sub-panel.
    pub panel: Option<ElemHandle>,
    info_text: Option<ElemHandle>,
    /// What the info text last read — the whole composed sheet.
    pub text: String,
    /// The six sections' lines, in the composer's order, so a test can name one.
    pub sections: Vec<String>,
    /// How many times the composer has written.
    pub updates: u32,
    seen: Option<CharacterInfo>,
    was_visible: bool,
    /// The world's era has no luminance, so the sheet leaves out its luminance section
    /// ([`super::era`]).
    pub era_features: dereth_primitives::EraFeatures,
}

impl CharacterInfoPanel {
    /// The character info panel's post-init, minus the two notice-handler registrations (player
    /// description received and load changed) and the quality handler on the
    /// player's `Age` int (`0x7D`).
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        self.panel = ui.get_child_recursive(root, PANEL);
        let Some(p) = self.panel else { return };
        self.info_text = ui.get_child_recursive(p, INFO_TEXT);
        self.was_visible = self.visible(ui);
    }

    #[must_use]
    pub fn bound(&self) -> bool {
        self.panel.is_some()
    }

    #[must_use]
    pub fn fully_bound(&self) -> bool {
        self.panel.is_some() && self.info_text.is_some()
    }

    /// The panel's visibility — the update's first check.
    #[must_use]
    pub fn visible(&self, ui: &UiSystem) -> bool {
        self.panel
            .and_then(|h| ui.node(h))
            .is_some_and(|n| n.region.flags.visible)
    }

    /// The player-description-received notice, the load-changed notice and the `Age` quality
    /// change. All three re-run the composer.
    pub fn recv_changed(&mut self) {
        self.seen = None;
    }

    /// One frame's drive. The visibility edge is polled for the reason
    /// [`super::effects::EffectsPanel::update`] gives.
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
        let features = view.era_features();
        if features != self.era_features {
            self.era_features = features;
            self.seen = None;
        }
        let info = view.character_info();
        if self.seen == info {
            return false;
        }
        self.seen.clone_from(&info);
        self.write(ui, info.as_ref());
        // **The info text's scrollbar.**
        //
        // Nothing is missing from the layout and nothing is missing from the binding. The shipped
        // tree carries both halves: the info text `0x1000011D` declares attribute
        // **`0x72 = 0x1000011E`**, and `0x1000011E` is a real `Scrollbar` (type `0x0B`)
        // in the same 15 px column shipping `0x76 disabled = true`, `0x79 hide-when-disabled =
        // true` and `0x82 proportional` [measured on the shipped layout]. If the relayout the
        // client runs after a text change never ran after [`Self::write`] filled it, the pane would
        // report a content extent of **0 x 0** however many lines it held — so the scrollbar would
        // see `content <= view`, set itself disabled, and `hide-when-disabled` would take it off the
        // screen.
        //
        // No hand call is needed here: the relayout is a real layout pass every screen gets.
        // Setting the text raises a relayout flag and
        // `UiSystem::draw` consumes it, which is where the client consumes it too: the element's
        // draw starts by recalculating its glyph list.
        true
    }

    /// The body of the character info panel's update.
    ///
    /// `None` is the no-player-description arm, which writes nothing — the sheet
    /// keeps whatever the layout gave it rather than showing six sections of zeroes.
    pub fn write(&mut self, ui: &mut UiSystem, info: Option<&CharacterInfo>) {
        let Some(c) = info else { return };
        let s = vec![
            self.birth_age_deaths(ui, c),
            self.endurance(ui, c),
            self.innates(ui, c),
            self.fake_skills(ui, c),
            self.augmentations(ui, c),
            self.load(ui, c),
        ];
        // The composer's separator: one literal "\n" appended between every pair of sections.
        let text = s.join("\n");
        self.sections = s;
        self.text.clone_from(&text);
        if let Some(t) = self.info_text.and_then(|h| ui.text_element_mut(h)) {
            t.set_text(&text);
        }
        self.updates += 1;
    }

    /// **All four arms**; see the module header for the first two.
    ///
    /// The birth and played arms are gated on whether the int quality is **present**, not on the
    /// number, so a character whose player description carries neither quality gets a section
    /// that is only the deaths line. The deaths arm is not gated at all — the client ignores
    /// presence and switches on a default of `0`. The enlightenment arm needs the quality present
    /// and above zero.
    fn birth_age_deaths(&self, ui: &UiSystem, c: &CharacterInfo) -> String {
        let mut s = String::new();
        // `localtime` then `strftime(buf, 0x400, "%c", tm)`, using the stored "%c" format.
        // Not `asctime`: client initialization
        // calls `setlocale(LC_ALL, "English")`, so `%c` is `English_United States.1252`'s
        // `M/d/yyyy h:mm:ss tt` and not `asctime`'s shape at all. The zone comes from
        // `CharacterInfo::utc_offset_secs`, which the host fills from the OS.
        if let Some(t) = c.created {
            let when = crate::ctime::strftime_c(i64::from(t), c.utc_offset_secs);
            s.push_str(&compose(ui, string::BIRTH, &[(var::DATE, when.as_str())]));
        }
        // [`query_duration`] of the age — a nested `StringInfo` on its own table, whose
        // rendered text is then this row's one variable.
        if let Some(age) = c.age {
            // The client divides the int **unsigned**, so a negative age
            // is a very large duration rather than a clamp. Reinterpreted, not saturated.
            let secs = u32::from_ne_bytes(age.to_ne_bytes());
            // All seven terms are named on **every** call, `""` for the ones that are zero --
            // see [`query_duration`]. The blank is the `b` flag the row's `{ …[!b]}` blocks test,
            // so leaving a term out would not shorten the sentence: it would take the client's
            // missing-variable arm and render nothing at all.
            let terms = query_duration(secs);
            let named: Vec<(&str, &str)> = var::DURATION_TERMS
                .iter()
                .copied()
                .zip(terms.iter().map(String::as_str))
                .collect();
            let played = compose_in(ui, DURATION_TABLE, string::DURATION_FORMAT, &named);
            s.push_str(&compose(
                ui,
                string::PLAYED,
                &[(var::DURATION, played.as_str())],
            ));
        }
        s.push_str(&match c.num_deaths {
            0 => compose(ui, string::DEATHS_NONE, &[]),
            1 => compose(ui, string::DEATHS_ONE, &[]),
            2 => compose(ui, string::DEATHS_TWO, &[]),
            n => {
                let n = super::statmgmt::num(n);
                compose(ui, string::DEATHS_MANY, &[(var::DEATHS, n.as_str())])
            }
        });
        // The enlightenment line: present and above zero, unlike the examine pane's line.
        if let Some(n) = c.enlightenment.filter(|n| *n > 0) {
            let n = super::statmgmt::num(n);
            s.push_str(&compose(
                ui,
                string::ENLIGHTENMENT_TIERS,
                &[(var::ENLIGHTENMENT, n.as_str())],
            ));
        }
        s
    }

    /// Three mastery lines, the header, the luminance ratings and the augmentation rows, in that
    /// order: the shared section builder, composed through this sheet's string table.
    fn augmentations(&self, ui: &UiSystem, c: &CharacterInfo) -> String {
        dereth_presentation::character::augmentation_section(
            c,
            self.era_features,
            &mut |token, values| compose(ui, token, values),
        )
    }

    /// The endurance info update.
    fn endurance(&self, ui: &UiSystem, c: &CharacterInfo) -> String {
        let resists = resist_band(c.strength.saturating_add(c.endurance).unsigned_abs());
        let regen = regeneration_band(
            c.strength
                .saturating_add(c.endurance.saturating_mul(2))
                .unsigned_abs(),
        );
        // Three slots, **two** variables -- and this line does not have to know that.
        // The endurance section adds `RESIST` once, the row names it twice, and the lookup
        // answers twice.
        compose(
            ui,
            string::RESISTS,
            &[(var::RESIST, resists), (var::REGEN, regen)],
        )
    }

    /// Six innate attribute levels in display order.
    fn innates(&self, ui: &UiSystem, c: &CharacterInfo) -> String {
        let v: Vec<String> = c.innate.iter().map(|n| super::statmgmt::num(*n)).collect();
        let named: Vec<(&str, &str)> = var::INNATE
            .iter()
            .copied()
            .zip(v.iter().map(String::as_str))
            .collect();
        compose(ui, string::INNATES, &named)
    }

    /// Two sections of one variable each, appended in order.
    fn fake_skills(&self, ui: &UiSystem, c: &CharacterInfo) -> String {
        let rank = super::statmgmt::num(c.chess_rank);
        let chess = compose(ui, string::CHESS, &[(var::CHESS, rank.as_str())]);
        let skill = super::statmgmt::num(c.fishing_skill);
        let fishing = compose(ui, string::FISHING, &[(var::FISHING, skill.as_str())]);
        format!("{chess}{fishing}")
    }

    /// The burden arm and, independently, the augmentation arm.
    fn load(&self, ui: &UiSystem, c: &CharacterInfo) -> String {
        let mut s = if c.load < 1.0 {
            compose(ui, string::LOAD_NONE, &[])
        } else {
            let burden = super::statmgmt::num(c.encumbrance.saturating_sub(c.capacity));
            let penalty = super::statmgmt::num(burden_penalty_percent(c.load));
            compose(
                ui,
                string::LOAD_BURDENED,
                &[
                    (var::BURDEN, burden.as_str()),
                    (var::PENALTY, penalty.as_str()),
                ],
            )
        };
        // `if (augs > 0)` — a **separate** `if`, not the `else` of the one above, so an
        // over-burdened augmented character gets both lines.
        if c.augmentations > 0 {
            let num = super::statmgmt::num(c.augmentations);
            let extra = super::statmgmt::num(c.augmentations.saturating_mul(20));
            s.push_str(&compose(
                ui,
                string::LOAD_AUGMENTATIONS,
                &[
                    (var::NUM_AUGMENTATIONS, num.as_str()),
                    (var::ADDITIONAL_LOAD, extra.as_str()),
                ],
            ));
        }
        s
    }
}

/// One `StringInfo`, out of the sheet's own table `0x23000001`, with each value under the
/// **name** the client files it under. See [`var`].
#[must_use]
pub fn compose(ui: &UiSystem, token: &str, values: &[(&str, &str)]) -> String {
    compose_in(ui, super::statmgmt::STRING_TABLE, token, values)
}

/// One `StringInfo`, rendered the way the client renders it.
///
/// **Not the plain interleave.** [`dereth_ui::UiSystem::resolve_string_variants`] plus
/// `pieces[0] + v[0] + pieces[1] + …` is the client's plain, no-metalanguage string path; the path
/// the client actually takes is the metalanguage one, and it runs
/// [`dereth_ui::text::metalanguage`] over the assembled input. For rows without markup the two
/// agree byte for byte, and for the birth, played and augmentation rows they do not: `{ year[1!b]| years[!b]}` and `{time[1]|times}` would
/// otherwise reach the screen as literal braces.
///
/// **Matched by name, not by position.** Each value arrives as a `(name, value)` pair and
/// [`dereth_ui::UiSystem::resolve_string_named`] matches it the way the string-table read
/// does: walk the *row's* own variable list and look each hashed id up in the caller's table. So a row that names one variable twice gets that value twice
/// (`ID_CharacterInfo_Resists`), a row that names none ignores what it was handed
/// (`ID_CharacterInfo_Augmentation_JackOfAllTrades`), and a localised dat that reorders
/// `ID_DurationFormat`'s seven terms needs no change here. The names are in [`var`], and in
/// [`super::linkstatus::var`] for the two rows that panel composes.
///
/// `table` is a parameter because `ID_DurationFormat` is on [`DURATION_TABLE`] and every other row
/// on this sheet is not.
///
/// When no string service is installed — every headless test in this crate — the fall-back is the
/// token followed by the values in brackets, so that a test can still see **which** numbers
/// reached the element. `super::statmgmt::label`'s fall-back is the same idea; this one has to
/// carry the values too. It also stands for the client's missing-variable arm: a name the row does not carry renders nothing, and this hands back the token form rather
/// than the empty string would.
#[must_use]
pub fn compose_in(
    ui: &UiSystem,
    table: dereth_primitives::DataId,
    token: &str,
    values: &[(&str, &str)],
) -> String {
    let id = dereth_primitives::num::hash::str_hash(token.as_bytes());
    match ui.resolve_string_named(table, id, values) {
        Some(s) => s,
        None if values.is_empty() => token.to_owned(),
        None => {
            let vals: Vec<&str> = values.iter().map(|(_, v)| *v).collect();
            format!("{token}[{}]", vals.join("|"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The enlightenment line follows the deaths line, and only for a count above zero.
    #[test]
    fn the_enlightenment_tiers_line_follows_the_deaths_line_when_above_zero() {
        let first = |e: Option<i32>| {
            let mut ui = UiSystem::new((800, 600));
            let mut p = CharacterInfoPanel::default();
            p.write(
                &mut ui,
                Some(&CharacterInfo {
                    enlightenment: e,
                    ..CharacterInfo::default()
                }),
            );
            p.sections[0].clone()
        };
        assert_eq!(
            first(Some(3)),
            "ID_CharacterInfo_Deaths_NoneID_CharacterInfo_Enlightenment_Tiers[3]"
        );
        assert_eq!(first(Some(0)), "ID_CharacterInfo_Deaths_None");
        assert_eq!(first(Some(-1)), "ID_CharacterInfo_Deaths_None");
        assert_eq!(first(None), "ID_CharacterInfo_Deaths_None");
    }

    /// Oracle: `crate::panels::catalogue`'s `CharacterInfoPanel` row.
    #[test]
    fn the_one_bound_child_is_the_catalogued_one() {
        let spec = crate::panels::catalogue::spec("CharacterInfoPanel").expect("catalogued");
        assert_eq!(spec.children.len(), 1);
        assert_eq!(spec.children[0].id, INFO_TEXT);
    }
}
