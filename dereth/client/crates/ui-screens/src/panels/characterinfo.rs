//! `CharacterInfoPanel` — the panel the **burden** lamp opens.
//!
//! Recovered HUD, toolbar, and panel behavior establish the element
//! type `0x1000001A`, instance `0x10000183`, one child: the info text `0x1000011D`, a
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
//! Read out of the shipped dats [verified against the shipped string table]; each is stored as the
//! literal pieces around its variables, so a line is `pieces[0] + v0 + pieces[1] + v1 + …`.
//!
//! **One reading is measured, not inferred.** `ID_CharacterInfo_Resists` has **four**
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

/// The `StringInfo` tokens the six sections compose.
pub mod string {
    /// `%BirthDate` — one variable, `strftime("%c")`'d.
    pub const BIRTH: &str = "ID_CharacterInfo_Birth";
    /// `%TimePlayed` — one variable, and it carries a whole nested `StringInfo`
    /// ([`super::query_duration`]) rather than a number.
    pub const PLAYED: &str = "ID_CharacterInfo_Played";
    /// The client's row, on table enum **2**.
    pub const DURATION_FORMAT: &str = "ID_DurationFormat";
    /// `%Mastery`.
    pub const MASTERY_MELEE: &str = "ID_CharacterInfo_Mastery_Melee";
    /// `%Mastery`.
    pub const MASTERY_RANGED: &str = "ID_CharacterInfo_Mastery_Ranged";
    /// `%Mastery`.
    pub const MASTERY_SUMMONING: &str = "ID_CharacterInfo_Mastery_Summoning";
    /// No variable, and **unconditional**.
    pub const LUMINANCE_HEADER: &str = "ID_CharacterInfo_Luminance_Header";
    pub const DEATHS_NONE: &str = "ID_CharacterInfo_Deaths_None";
    pub const DEATHS_ONE: &str = "ID_CharacterInfo_Deaths_One";
    pub const DEATHS_TWO: &str = "ID_CharacterInfo_Deaths_Two";
    /// `%NumberOfDeaths`.
    pub const DEATHS_MANY: &str = "ID_CharacterInfo_Deaths_Many";
    /// `%Enlightenment` — *"Enlightenment Tiers: "* and the count, after the deaths line.
    pub const ENLIGHTENMENT_TIERS: &str = "ID_CharacterInfo_Enlightenment_Tiers";
    /// `%Resists` twice and `%RegenerationBonus` — see the module header's note.
    pub const RESISTS: &str = "ID_CharacterInfo_Resists";
    /// Six `%`-variables, in the display order Strength, Endurance, Coordination, Quickness,
    /// Focus, Self.
    pub const INNATES: &str = "ID_CharacterInfo_Innates";
    /// `%ChessRank`.
    pub const CHESS: &str = "ID_CharacterInfo_Chess";
    /// `%FishingSkill`.
    pub const FISHING: &str = "ID_CharacterInfo_Fishing";
    pub const LOAD_NONE: &str = "ID_CharacterInfo_Load_None";
    /// `%Burden` and `%BurdenPenalty`.
    pub const LOAD_BURDENED: &str = "ID_CharacterInfo_Load_Burdened";
    /// `%NumAugmentations` and `%AdditionalLoad`.
    pub const LOAD_AUGMENTATIONS: &str = "ID_CharacterInfo_Load_Augmentations";
}

/// The **variable names** those rows file their values under — the key each value is added
/// under, which is a string hash of the name and never a slot number.
///
/// The client walks the *row's* own variable list and looks each id up in the caller's table, so
/// these names are the whole of what places a value and the order this file writes them in is
/// invisible. Every one is confirmed against the shipped rows' own variable lists.
pub mod var {
    /// The birth-date variable — the hash of `"DATE"`.
    pub const DATE: &str = "DATE";
    /// The time-played variable; its value is the whole nested `ID_DurationFormat` string.
    pub const DURATION: &str = "DURATION";
    /// The number-of-deaths variable.
    pub const DEATHS: &str = "DEATHS";
    /// The enlightenment count on [`super::string::ENLIGHTENMENT_TIERS`].
    pub const ENLIGHTENMENT: &str = "ENLIGHTENMENT";
    /// The resistances variable. `ID_CharacterInfo_Resists` names it **twice** while the client
    /// adds it **once**; the lookup simply answers twice, which is why this call site supplies one
    /// value.
    pub const RESIST: &str = "RESIST";
    /// The regeneration-bonus variable.
    pub const REGEN: &str = "REGEN";
    /// The Strength, Endurance, Coordination, Quickness, Focus and Self variables, in the display
    /// order the innates section adds them, which is also
    /// `ID_CharacterInfo_Innates`'s own order \[measured\].
    pub const INNATE: [&str; 6] = [
        "STRENGTH",
        "ENDURANCE",
        "COORDINATION",
        "QUICKNESS",
        "FOCUS",
        "SELF",
    ];
    /// The augmentation-count variable. Every luminance and augmentation row uses this one,
    /// and the 24 of them that ship with no variable at all never look it up.
    pub const NUM_AUGMENTATIONS: &str = "NUM_AUGMENTATIONS";
    /// The chess-rank variable.
    pub const CHESS: &str = "CHESS";
    /// The fishing-skill variable.
    pub const FISHING: &str = "FISHING";
    /// The burden variable.
    pub const BURDEN: &str = "BURDEN";
    /// The burden-penalty variable.
    pub const PENALTY: &str = "PENALTY";
    /// The additional-load variable.
    pub const ADDITIONAL_LOAD: &str = "ADDITIONAL_LOAD";
    /// The mastery variable. All three mastery rows name this same one.
    pub const MASTERY: &str = "MASTERY";
    /// `ID_DurationFormat`'s seven, in the order [`super::query_duration`] returns its terms.
    /// Unlike every other name here these are hashed **inline** where they are used rather than
    /// cached.
    pub const DURATION_TERMS: [&str; 7] = [
        "YEARS", "MONTHS", "WEEKS", "DAYS", "HOURS", "MINUTES", "SECONDS",
    ];
}

/// The six stored rating labels shared by both selection ladders.
pub mod band {
    /// No rating.
    pub const NONE: &str = "None";
    /// Poor rating.
    pub const POOR: &str = "Poor";
    /// Mediocre rating.
    pub const MEDIOCRE: &str = "Mediocre";
    /// Hardy rating.
    pub const HARDY: &str = "Hardy";
    /// Resilient rating.
    pub const RESILIENT: &str = "Resilient";
    /// Indomitable rating.
    pub const INDOMITABLE: &str = "Indomitable";
}

/// The int quality ids `CharacterInfoPanel` reads that are not already fields of
/// [`CharacterInfo`].
///
/// Defined in [`dereth_client_contract::panels::characterinfo::prop`], because
/// `dereth_client::hud` is what asks the qualities.
pub use dereth_client_contract::panels::characterinfo::prop;

/// The eleven luminance ratings, the eleven augmentation rows and the three mastery names.
///
/// Defined in [`dereth_client_contract::panels::characterinfo`], because
/// `dereth_client::hud` walks both tables to collect the ids it asks the qualities for, and the
/// three mastery names sit between them and are pure switches over the same three ids.
pub use dereth_client_contract::panels::characterinfo::{
    melee_mastery_name, ranged_mastery_name, summoning_mastery_name, AUGMENTATIONS, LUMINANCE,
};

/// The string table that table enum **2** names.
///
/// Every other row on this sheet comes from `0x10000001` → `0x23000001`; `ID_DurationFormat` is
/// the one row that does not, and `0x23000006` is the **only** one of the fifteen tables the dats
/// ship that holds it [measured against the shipped dats].
pub const DURATION_TABLE: dereth_primitives::DataId = dereth_primitives::DataId(0x2300_0006);

/// The duration breakdown — the seven values `ID_DurationFormat` interleaves, in its own order.
///
/// ```text
/// years   = s / 0x1E13380 ; s %= 0x1E13380      ; 31 536 000 = 365 days
/// months  = s / 0x278D00  ; s %= 0x278D00       ;  2 592 000 =  30 days
/// weeks   = s / 0x93A80   ; s %= 0x93A80        ;    604 800
/// days    = s / 0x15180   ; s %= 0x15180        ;     86 400
/// hours   = s / 0xE10     ; s %= 0xE10          ;      3 600
/// minutes = s / 0x3C      ; seconds = s % 0x3C
/// ```
///
/// The divisions are **unsigned**, which is why this takes a `u32`. The year is 365 days and the
/// month 30, so the terms do not add up to a calendar — that is the client's arithmetic and not a
/// simplification.
///
/// Each term is added under its name as an unsigned number when it is non-zero and as an empty
/// string when it is not.
/// **The empty string is the whole selector**: the metalanguage derives the `b` (blank) flag from
/// it, and `{ …[!b]}` is what deletes the term from the sentence. It is load-bearing, not a
/// placeholder.
#[must_use]
pub fn query_duration(seconds: u32) -> [String; 7] {
    let mut s = seconds;
    let mut term = |div: u32| {
        let n = s / div;
        s %= div;
        if n == 0 {
            String::new()
        } else {
            n.to_string()
        }
    };
    let y = term(0x1E13380);
    let mo = term(0x278D00);
    let w = term(0x93A80);
    let d = term(0x15180);
    let h = term(0xE10);
    let mi = term(0x3C);
    let se = if s == 0 { String::new() } else { s.to_string() };
    [y, mo, w, d, h, mi, se]
}

/// The endurance section's **first** ladder, over `strength + endurance`.
///
/// ```text
/// <= 0xC8  (200) -> "None"
/// <= 0x104 (260) -> "Poor"
/// <= 0x140 (320) -> "Mediocre"
/// <= 0x17C (380) -> "Hardy"
/// <= 0x1B8 (440) -> "Resilient", else "Indomitable"
/// ```
///
/// The comparisons are **unsigned**, which is what makes a negative sum — impossible
/// from attributes, but free to model — land in `Indomitable` rather than `None`.
#[must_use]
pub fn resist_band(sum: u32) -> &'static str {
    match sum {
        0..=0xC8 => band::NONE,
        0xC9..=0x104 => band::POOR,
        0x105..=0x140 => band::MEDIOCRE,
        0x141..=0x17C => band::HARDY,
        0x17D..=0x1B8 => band::RESILIENT,
        _ => band::INDOMITABLE,
    }
}

/// The endurance section's **second** ladder, over `strength + 2 * endurance`.
///
/// ```text
/// 0xC8 / 0x15A / 0x1D6 / 0x244 / 0x2B2   ; 200 / 346 / 470 / 580 / 690
/// ```
///
/// Same six literals, different bounds — which is why it cannot share [`resist_band`].
#[must_use]
pub fn regeneration_band(sum: u32) -> &'static str {
    match sum {
        0..=0xC8 => band::NONE,
        0xC9..=0x15A => band::POOR,
        0x15B..=0x1D6 => band::MEDIOCRE,
        0x1D7..=0x244 => band::HARDY,
        0x245..=0x2B2 => band::RESILIENT,
        _ => band::INDOMITABLE,
    }
}

/// The encumbrance system's load mod, then `(10 - (int)(mod * 10)) * 10`.
///
/// The percentage `ID_CharacterInfo_Load_Burdened` shows as *"reducing your Run, Jump, Melee
/// Defense and Missile Defense skills by N%"*. A load of exactly `1.0` gives `0`, `1.5` gives
/// `50`, and anything at or over `2.0` gives `100`.
///
/// The load mod is [`dereth_rules::burden::load_mod`], the one the run rate uses. It lives in
/// `dereth-rules` because this crate may not depend on `dereth-client-model`.
#[must_use]
pub fn burden_penalty_percent(load: f32) -> i32 {
    let load_mod = dereth_rules::burden::load_mod(load);
    let tenths = dereth_primitives::num::to_i32(load_mod * 10.0);
    (10 - tenths) * 10
}

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
    pub era_lacks_luminance: bool,
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
        let lacks_luminance = view.era().is_some_and(|e| !e.features().luminance);
        if lacks_luminance != self.era_lacks_luminance {
            self.era_lacks_luminance = lacks_luminance;
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

    /// Three mastery lines, the header, [`LUMINANCE`] and
    /// [`AUGMENTATIONS`], in that order; the module header has the shape.
    fn augmentations(&self, ui: &UiSystem, c: &CharacterInfo) -> String {
        let mut s = String::new();
        if c.melee_mastery > 0 {
            let name = melee_mastery_name(c.melee_mastery);
            s.push_str(&compose(ui, string::MASTERY_MELEE, &[(var::MASTERY, name)]));
        }
        if c.ranged_mastery > 0 {
            let name = ranged_mastery_name(c.ranged_mastery);
            s.push_str(&compose(
                ui,
                string::MASTERY_RANGED,
                &[(var::MASTERY, name)],
            ));
        }
        if c.summoning_mastery > 0 {
            let name = summoning_mastery_name(c.summoning_mastery);
            s.push_str(&compose(
                ui,
                string::MASTERY_SUMMONING,
                &[(var::MASTERY, name)],
            ));
        }
        // No gate and no variable. The header is on the sheet even for a character
        // with no luminance at all, which is what a new character sees first -- in an era that
        // has luminance.
        let luminance: &[_] = if self.era_lacks_luminance {
            &[]
        } else {
            s.push_str(&compose(ui, string::LUMINANCE_HEADER, &[]));
            LUMINANCE
        };
        for (id, base, spec) in luminance {
            let v = c.aug_ints.get(id).copied().unwrap_or(0);
            match spec {
                // The rated pairs: five splits the value, and **both** halves can show.
                Some(sp) => {
                    let (base_v, spec_v) = if v > 5 { (5, v - 5) } else { (v, 0) };
                    if base_v <= 0 {
                        continue;
                    }
                    let n = super::statmgmt::num(base_v);
                    s.push_str(&compose(ui, base, &[(var::NUM_AUGMENTATIONS, n.as_str())]));
                    if spec_v > 0 {
                        let n = super::statmgmt::num(spec_v);
                        s.push_str(&compose(ui, sp, &[(var::NUM_AUGMENTATIONS, n.as_str())]));
                    }
                }
                // The single ratings have **no** split at five (the client has just the `> 0`
                // gate), so nothing here may clamp them to five.
                None => {
                    if v > 0 {
                        let n = super::statmgmt::num(v);
                        s.push_str(&compose(ui, base, &[(var::NUM_AUGMENTATIONS, n.as_str())]));
                    }
                }
            }
        }
        for (id, token) in AUGMENTATIONS {
            let v = c.aug_ints.get(id).copied().unwrap_or(0);
            if v > 0 {
                let n = super::statmgmt::num(v);
                s.push_str(&compose(ui, token, &[(var::NUM_AUGMENTATIONS, n.as_str())]));
            }
        }
        s
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

    /// Oracle: the client's two ladders, at every bound.
    ///
    /// The bounds are **inclusive** because each step tests *above*, so `0xC8` itself
    /// is still `"None"` and `0xC9` is the first `"Poor"`. That off-by-one is the whole reason to
    /// pin every boundary rather than a value in the middle of each band.
    #[test]
    fn the_two_endurance_ladders_have_different_bounds_and_the_same_six_words() {
        assert_eq!(resist_band(0), band::NONE);
        assert_eq!(
            resist_band(200),
            band::NONE,
            "0xC8 is not above the first bound"
        );
        assert_eq!(resist_band(201), band::POOR);
        assert_eq!(resist_band(260), band::POOR);
        assert_eq!(resist_band(261), band::MEDIOCRE);
        assert_eq!(resist_band(320), band::MEDIOCRE);
        assert_eq!(resist_band(321), band::HARDY);
        assert_eq!(resist_band(380), band::HARDY);
        assert_eq!(resist_band(381), band::RESILIENT);
        assert_eq!(resist_band(440), band::RESILIENT);
        assert_eq!(resist_band(441), band::INDOMITABLE);

        assert_eq!(regeneration_band(200), band::NONE);
        assert_eq!(regeneration_band(201), band::POOR);
        assert_eq!(regeneration_band(346), band::POOR, "0x15A, not 0x104");
        assert_eq!(regeneration_band(347), band::MEDIOCRE);
        assert_eq!(regeneration_band(470), band::MEDIOCRE);
        assert_eq!(regeneration_band(471), band::HARDY);
        assert_eq!(regeneration_band(580), band::HARDY);
        assert_eq!(regeneration_band(581), band::RESILIENT);
        assert_eq!(regeneration_band(690), band::RESILIENT);
        assert_eq!(regeneration_band(691), band::INDOMITABLE);

        // The two ladders disagree over most of their range, which is what stops one standing in
        // for the other: 300 is Mediocre on the first and Poor on the second.
        assert_eq!(resist_band(300), band::MEDIOCRE);
        assert_eq!(regeneration_band(300), band::POOR);
    }

    /// Oracle: the client's `(10 - trunc(load_mod(load) * 10.0)) * 10`, in `f32`, and the load
    /// modifier's three branches.
    #[test]
    fn the_burden_penalty_is_ten_minus_the_load_mod_in_tenths_times_ten() {
        assert_eq!(
            burden_penalty_percent(0.5),
            0,
            "under capacity: the load mod is 1.0"
        );
        assert_eq!(
            burden_penalty_percent(1.0),
            0,
            "exactly at capacity, and still 0%"
        );
        assert_eq!(burden_penalty_percent(1.5), 50, "0.5 is exact in f32");
        // **The truncation is the client's and it is visible.** `2.0f - 1.2f` is `0.79999995`,
        // `* 10.0f` is `7.9999995`, and the truncation takes **7** -- so a load of 1.2 costs 30%, not
        // the 20% an `f64` implementation would print. Pinned because it is the one place this
        // function can silently disagree with retail.
        assert_eq!(burden_penalty_percent(1.2), 30);
        assert_eq!(
            burden_penalty_percent(1.9),
            90,
            "2.0f - 1.9f is 0.100000024, so (int)1"
        );
        assert_eq!(
            burden_penalty_percent(2.0),
            100,
            "the load mod clamps to 0 at twice capacity"
        );
        assert_eq!(burden_penalty_percent(5.0), 100);
    }

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
