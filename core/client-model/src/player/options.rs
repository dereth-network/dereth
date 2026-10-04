//! `PlayerOption` — the 53 UI-facing ordinals and their bits in the two option words.
//!
//! The ordinal and the bit are **different numbering schemes** and the client keeps both:
//! `PlayerOption` is what the options screen and `Character_PlayerOptionChangedEvent` (0x0005) use;
//! the bit is what the blob carries. This module is the bridge, and its test asserts that the
//! bridge reproduces both independently-derived default words.

/// Which option word a bit lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionWord {
    /// The first option word — `CharacterOption`.
    One,
    /// The second option word — `CharacterOptions2`.
    Two,
}

/// The 53 `PlayerOption` ordinals, each with its name and its bit.
///
/// Index **is** the ordinal, so `PLAYER_OPTIONS[12]` is `AdvancedCombatUI`. The last one,
/// `HearPKDeaths` (52, second word bit 25), decides whether a player-killer death broadcast is
/// shown at all.
pub const PLAYER_OPTIONS: [(&str, OptionWord, u32); 53] = [
    ("AutoRepeatAttack", OptionWord::One, 0x0000_0002),
    ("IgnoreAllegianceRequests", OptionWord::One, 0x0000_0004),
    ("IgnoreFellowshipRequests", OptionWord::One, 0x0000_0008),
    ("IgnoreTradeRequests", OptionWord::One, 0x0002_0000),
    ("DisableMostWeatherEffects", OptionWord::One, 0x0001_0000),
    ("PersistentAtDay", OptionWord::Two, 0x0000_0001),
    ("AllowGive", OptionWord::One, 0x0000_0040),
    ("ViewCombatTarget", OptionWord::One, 0x0000_0080),
    ("ShowTooltips", OptionWord::One, 0x0000_0100),
    ("UseDeception", OptionWord::One, 0x0000_0200),
    ("ToggleRun", OptionWord::One, 0x0000_0400),
    ("StayInChatMode", OptionWord::One, 0x0000_0800),
    ("AdvancedCombatUI", OptionWord::One, 0x0000_1000),
    ("AutoTarget", OptionWord::One, 0x0000_2000),
    ("VividTargetingIndicator", OptionWord::One, 0x0000_8000),
    ("FellowshipShareXP", OptionWord::One, 0x0004_0000),
    ("AcceptLootPermits", OptionWord::One, 0x0008_0000),
    ("FellowshipShareLoot", OptionWord::One, 0x0010_0000),
    ("FellowshipAutoAcceptRequests", OptionWord::One, 0x2000_0000),
    ("SideBySideVitals", OptionWord::One, 0x0020_0000),
    ("CoordinatesOnRadar", OptionWord::One, 0x0040_0000),
    ("SpellDuration", OptionWord::One, 0x0080_0000),
    (
        "DisableHouseRestrictionEffects",
        OptionWord::One,
        0x0200_0000,
    ),
    (
        "DragItemOnPlayerOpensSecureTrade",
        OptionWord::One,
        0x0400_0000,
    ),
    (
        "DisplayAllegianceLogonNotifications",
        OptionWord::One,
        0x0800_0000,
    ),
    ("UseChargeAttack", OptionWord::One, 0x1000_0000),
    ("UseCraftSuccessDialog", OptionWord::One, 0x8000_0000),
    ("HearAllegianceChat", OptionWord::One, 0x4000_0000),
    ("DisplayDateOfBirth", OptionWord::Two, 0x0000_0002),
    ("DisplayAge", OptionWord::Two, 0x0000_0020),
    ("DisplayChessRank", OptionWord::Two, 0x0000_0004),
    ("DisplayFishingSkill", OptionWord::Two, 0x0000_0008),
    ("DisplayNumberDeaths", OptionWord::Two, 0x0000_0010),
    ("DisplayTimeStamps", OptionWord::Two, 0x0000_0040),
    ("SalvageMultiple", OptionWord::Two, 0x0000_0080),
    ("HearGeneralChat", OptionWord::Two, 0x0000_0100),
    ("HearTradeChat", OptionWord::Two, 0x0000_0200),
    ("HearLFGChat", OptionWord::Two, 0x0000_0400),
    ("HearRoleplayChat", OptionWord::Two, 0x0000_0800),
    ("AppearOffline", OptionWord::Two, 0x0000_1000),
    ("DisplayNumberCharacterTitles", OptionWord::Two, 0x0000_2000),
    ("MainPackPreferred", OptionWord::Two, 0x0000_4000),
    ("LeadMissileTargets", OptionWord::Two, 0x0000_8000),
    ("UseFastMissiles", OptionWord::Two, 0x0001_0000),
    ("FilterLanguage", OptionWord::Two, 0x0002_0000),
    ("ConfirmVolatileRareUse", OptionWord::Two, 0x0004_0000),
    ("HearSocietyChat", OptionWord::Two, 0x0008_0000),
    ("ShowHelm", OptionWord::Two, 0x0010_0000),
    ("DisableDistanceFog", OptionWord::Two, 0x0020_0000),
    ("UseMouseTurning", OptionWord::Two, 0x0040_0000),
    ("ShowCloak", OptionWord::Two, 0x0080_0000),
    ("LockUI", OptionWord::Two, 0x0100_0000),
    ("HearPKDeaths", OptionWord::Two, 0x0200_0000),
];

/// The named ordinals this crate branches on. The rest are reached by index.
pub mod option {
    pub const AUTO_REPEAT_ATTACK: usize = 0;
    pub const IGNORE_ALLEGIANCE_REQUESTS: usize = 1;
    pub const IGNORE_FELLOWSHIP_REQUESTS: usize = 2;
    pub const IGNORE_TRADE_REQUESTS: usize = 3;
    pub const DISABLE_MOST_WEATHER_EFFECTS: usize = 4;
    pub const PERSISTENT_AT_DAY: usize = 5;
    pub const ALLOW_GIVE: usize = 6;
    pub const VIEW_COMBAT_TARGET: usize = 7;
    /// **The gate on the 3-D object tooltip.**
    ///
    /// The player module's show-tooltips is `(word >> 8) & 1` of the first option word, i.e.
    /// `word & 0x100`,
    /// which is this ordinal's bit.
    /// The smart-box wrapper's object-found notice calls it through
    /// the player module and shows the hovered object's
    /// name only when it answers true. **Default on** (`DEFAULT_TRUE_ORDINALS`, and bit `0x100`
    /// is set in `DEFAULT_OPTIONS`), so a ticked option with no tooltip means nothing is
    /// reading it.
    pub const SHOW_TOOLTIPS: usize = 8;
    /// The advanced-combat-UI option is `(word >> 11) & 1` of the first option word, read
    /// by the chat input before it decides whether Enter also drops
    /// focus. Named here because that seam has to reach it.
    pub const STAY_IN_CHAT_MODE: usize = 11;
    /// **This is the option that decides whether Shift runs you or walks you.**
    ///
    /// The command interpreter reads the player's toggles-run option, and
    /// its set-hold-run **XORs** the physical key with it:
    ///
    /// ```text
    ///   hold_run      = (key held)
    ///   effective_run = (hold_run == 0) != (toggles_run == 0)
    ///   update the motion interpreter's hold-run state
    /// ```
    ///
    /// It is **default-on** (`DEFAULT_TRUE_ORDINALS`, and bit `0x0400` is set in
    /// `DEFAULT_OPTIONS`), so on a shipped character `effective_run = !hold_run`: **run is the
    /// default state and holding the key walks you.** The client's own name for the input action
    /// says the same thing — `0x00000032` is `MovementWalkMode`, not a run modifier.
    pub const TOGGLE_RUN: usize = 10;
    pub const ADVANCED_COMBAT_UI: usize = 12;
    pub const AUTO_TARGET: usize = 13;
    pub const FELLOWSHIP_AUTO_ACCEPT_REQUESTS: usize = 18;
    pub const DISABLE_HOUSE_RESTRICTION_EFFECTS: usize = 22;
    pub const DRAG_ITEM_ON_PLAYER_OPENS_SECURE_TRADE: usize = 23;
    pub const USE_CRAFT_SUCCESS_DIALOG: usize = 26;
    pub const DISPLAY_TIME_STAMPS: usize = 33;
    pub const SALVAGE_MULTIPLE: usize = 34;
    pub const MAIN_PACK_PREFERRED: usize = 41;
    pub const FILTER_LANGUAGE: usize = 44;
    pub const CONFIRM_VOLATILE_RARE_USE: usize = 45;
    pub const DISABLE_DISTANCE_FOG: usize = 48;
    /// The lock-UI setter tail-calls the player module's changed hook for ordinal 51.
    pub const LOCK_UI: usize = 51;
    /// Whether player-killer death broadcasts are shown. Off drops a system line that carries
    /// the death tag; on shows the line with the tag removed. On by default (second word bit 25).
    pub const HEAR_PK_DEATHS: usize = 52;
}

/// The default first character-options word.
pub const DEFAULT_OPTIONS: u32 = 0x50C4_A54A;
/// The default second character-options word. Also the literal the unpacker substitutes when
/// the packed-second-options bit is absent.
///
/// `0x0294_8700`: the older `0x0094_8700` plus bit 25, `HearPKDeaths`, so a character whose
/// second word was never saved hears player-killer deaths.
pub const DEFAULT_OPTIONS2: u32 = 0x0294_8700;

/// The default-value query returns `true` for exactly these 16 ordinals.
pub const DEFAULT_TRUE_ORDINALS: [usize; 16] = [
    0,  // AutoRepeatAttack
    2,  // IgnoreFellowshipRequests
    6,  // AllowGive
    8,  // ShowTooltips
    10, // ToggleRun
    13, // AutoTarget
    14, // VividTargetingIndicator
    15, // FellowshipShareXP
    20, // CoordinatesOnRadar
    21, // SpellDuration
    25, // UseChargeAttack
    27, // HearAllegianceChat
    35, // HearGeneralChat
    36, // HearTradeChat
    37, // HearLFGChat
    42, // LeadMissileTargets
];

/// Behavior: the 22 options sent **immediately** as a
/// single-option `Character_PlayerOptionChangedEvent` rather than batched into the 480 s flush.
///
/// These are the options with a server-visible effect: chat channel subscriptions,
/// fellowship/allegiance behaviour, the two appearance flags the server has to broadcast, and
/// whether player-killer deaths are heard.
pub const AUTO_SAVE_ORDINALS: [usize; 22] = [
    0,  // AutoRepeatAttack
    1,  // IgnoreAllegianceRequests
    2,  // IgnoreFellowshipRequests
    15, // FellowshipShareXP
    16, // AcceptLootPermits
    17, // FellowshipShareLoot
    18, // FellowshipAutoAcceptRequests
    25, // UseChargeAttack
    27, // HearAllegianceChat
    35, // HearGeneralChat
    36, // HearTradeChat
    37, // HearLFGChat
    38, // HearRoleplayChat
    39, // AppearOffline
    42, // LeadMissileTargets
    43, // UseFastMissiles
    46, // HearSocietyChat
    47, // ShowHelm
    49, // UseMouseTurning
    50, // ShowCloak
    51, // LockUI
    52, // HearPKDeaths
];

/// The is-auto-save-option predicate.
#[must_use]
pub fn is_auto_save_option(ordinal: usize) -> bool {
    AUTO_SAVE_ORDINALS.contains(&ordinal)
}

/// The default option value.
#[must_use]
pub fn default_option_value(ordinal: usize) -> bool {
    DEFAULT_TRUE_ORDINALS.contains(&ordinal)
}

/// The two option words, with the ordinal bridge on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub options: u32,
    pub options2: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            options: DEFAULT_OPTIONS,
            options2: DEFAULT_OPTIONS2,
        }
    }
}

impl Options {
    /// The player module's option getter.
    #[must_use]
    pub fn get(&self, ordinal: usize) -> bool {
        let Some((_, word, mask)) = PLAYER_OPTIONS.get(ordinal).copied() else {
            return false;
        };
        match word {
            OptionWord::One => self.options & mask != 0,
            OptionWord::Two => self.options2 & mask != 0,
        }
    }

    /// Set a boolean player option. Returns whether the bit changed,
    /// which is what gates the `OnChanged` hook.
    pub fn set(&mut self, ordinal: usize, value: bool) -> bool {
        let Some((_, word, mask)) = PLAYER_OPTIONS.get(ordinal).copied() else {
            return false;
        };
        let field = match word {
            OptionWord::One => &mut self.options,
            OptionWord::Two => &mut self.options2,
        };
        let was = *field & mask != 0;
        if was == value {
            return false;
        }
        if value {
            *field |= mask;
        } else {
            *field &= !mask;
        }
        true
    }

    /// The name of an ordinal, for debugging output only.
    #[must_use]
    pub fn name(ordinal: usize) -> &'static str {
        PLAYER_OPTIONS
            .get(ordinal)
            .map_or("Invalid_PlayerOption", |o| o.0)
    }

    // The handful of accessors this crate itself branches on. The client has 53 named pairs; only
    // these have a reader inside the gameplay model.

    #[must_use]
    pub fn auto_repeat_attack(&self) -> bool {
        self.get(option::AUTO_REPEAT_ATTACK)
    }

    #[must_use]
    pub fn auto_target(&self) -> bool {
        self.get(option::AUTO_TARGET)
    }

    #[must_use]
    pub fn advanced_combat_ui(&self) -> bool {
        self.get(option::ADVANCED_COMBAT_UI)
    }

    /// Toggle the run option, returning the new state that movement input XORs the hold key
    /// against. See
    /// [`option::TOGGLE_RUN`] for the logic and for why a shipped character runs by default.
    ///
    /// The runtime copies this into `MovementCommands::ui_toggles_run`, the second input of the
    /// hold-run XOR in `dereth_client_runtime::actions::movement::CommandLists::set_hold_run`.
    /// If that copy is missing the field stays `false` and the XOR runs against the wrong input:
    /// the formula is right and one of its inputs is missing, so the symptom is a clean
    /// inversion of walk and run.
    #[must_use]
    pub fn toggle_run(&self) -> bool {
        self.get(option::TOGGLE_RUN)
    }

    /// What makes picked-up items go to the main pack rather than the open side pack.
    #[must_use]
    pub fn main_pack_preferred(&self) -> bool {
        self.get(option::MAIN_PACK_PREFERRED)
    }

    /// What allows mixing materials in one salvage operation.
    #[must_use]
    pub fn salvage_multiple(&self) -> bool {
        self.get(option::SALVAGE_MULTIPLE)
    }

    /// Decide whether volatile rare use raises
    /// the volatile-rare usage confirmation before using an item with the volatile-rare bit.
    ///
    /// It is one of the **three** options the client's own defaults set (with `ShowHelm` and
    /// `ShowCloak`), so the ask is on unless the character turned it off.
    #[must_use]
    pub fn confirm_volatile_rare_use(&self) -> bool {
        self.get(option::CONFIRM_VOLATILE_RARE_USE)
    }

    #[must_use]
    pub fn disable_house_restriction_effects(&self) -> bool {
        self.get(option::DISABLE_HOUSE_RESTRICTION_EFFECTS)
    }

    #[must_use]
    pub fn allow_give(&self) -> bool {
        self.get(option::ALLOW_GIVE)
    }

    /// Whether player-killer death broadcasts are shown; the system-line filter reads it.
    #[must_use]
    pub fn hear_pk_deaths(&self) -> bool {
        self.get(option::HEAR_PK_DEATHS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: `04-player-and-character-state.md` §3's `PlayerOption` ordinal table cross-checked
    /// against `02-qualities-and-properties.md` §11's two mask tables. The two are derived
    /// independently — one from the client's enum, one from the option words — so the fact that every
    /// name in one appears exactly once in the other, and that the resulting bridge reproduces both
    /// documented default words, is the check.
    #[test]
    fn the_ordinal_to_bit_bridge_reproduces_both_default_words() {
        // Every mask is used exactly once inside its word.
        for word in [OptionWord::One, OptionWord::Two] {
            let mut masks: Vec<u32> = PLAYER_OPTIONS
                .iter()
                .filter(|(_, w, _)| *w == word)
                .map(|(_, _, m)| *m)
                .collect();
            let n = masks.len();
            masks.sort_unstable();
            masks.dedup();
            assert_eq!(masks.len(), n, "a mask is claimed twice in {word:?}");
        }
        // 27 CharacterOption bits and 26 CharacterOptions2 bits: 53 in all.
        assert_eq!(
            PLAYER_OPTIONS
                .iter()
                .filter(|(_, w, _)| *w == OptionWord::One)
                .count(),
            27
        );
        assert_eq!(
            PLAYER_OPTIONS
                .iter()
                .filter(|(_, w, _)| *w == OptionWord::Two)
                .count(),
            26
        );

        // The union of each word's masks: every documented bit is accounted for and no other.
        let union = |word: OptionWord| {
            PLAYER_OPTIONS
                .iter()
                .filter(|(_, w, _)| *w == word)
                .fold(0u32, |a, (_, _, m)| a | m)
        };
        assert_eq!(union(OptionWord::One), 0xFEFF_BFCE);
        assert_eq!(union(OptionWord::Two), 0x03FF_FFFF);
        assert_eq!(DEFAULT_OPTIONS & !union(OptionWord::One), 0);
        assert_eq!(DEFAULT_OPTIONS2 & !union(OptionWord::Two), 0);
    }

    /// Oracle: §3's default first options word = `0x50C4A54A` and default second word =
    /// `0x02948700`, decomposed through the bridge into names.
    #[test]
    fn the_default_words_decompose_into_the_documented_names() {
        let d = Options::default();
        let set: Vec<&str> = (0..PLAYER_OPTIONS.len())
            .filter(|i| d.get(*i))
            .map(Options::name)
            .collect();
        assert_eq!(
            set,
            vec![
                "AutoRepeatAttack",
                "IgnoreFellowshipRequests",
                "AllowGive",
                "ShowTooltips",
                "ToggleRun",
                "AutoTarget",
                "VividTargetingIndicator",
                "FellowshipShareXP",
                "CoordinatesOnRadar",
                "SpellDuration",
                "UseChargeAttack",
                "HearAllegianceChat",
                "HearGeneralChat",
                "HearTradeChat",
                "HearLFGChat",
                "LeadMissileTargets",
                "ConfirmVolatileRareUse",
                "ShowHelm",
                "ShowCloak",
                "HearPKDeaths",
            ]
        );
    }

    /// Oracle: §3's list of 16, compared against the two default
    /// **words** decomposed through the bridge.
    ///
    /// They disagree in four places, and the disagreement is real rather than a bridge error: the
    /// default *words* set `ConfirmVolatileRareUse`, `ShowHelm`, `ShowCloak` and `HearPKDeaths`
    /// while the default-value query returns false for all four (it did not gain the newest
    /// option when the default word did). A character created from the default words therefore
    /// does not match what the query would answer for those four. Recorded rather than
    /// reconciled.
    #[test]
    fn get_default_option_value_and_the_default_words_disagree_in_exactly_four_places() {
        let d = Options::default();
        let n = PLAYER_OPTIONS.len();
        let from_words: Vec<usize> = (0..n).filter(|i| d.get(*i)).collect();
        let from_fn: Vec<usize> = (0..n).filter(|i| default_option_value(*i)).collect();
        assert_eq!(from_fn.len(), 16, "the function names exactly 16");
        let only_words: Vec<&str> = from_words
            .iter()
            .filter(|i| !from_fn.contains(i))
            .map(|i| Options::name(*i))
            .collect();
        assert_eq!(
            only_words,
            vec![
                "ConfirmVolatileRareUse",
                "ShowHelm",
                "ShowCloak",
                "HearPKDeaths"
            ]
        );
        let only_fn: Vec<&str> = from_fn
            .iter()
            .filter(|i| !from_words.contains(i))
            .map(|i| Options::name(*i))
            .collect();
        assert!(
            only_fn.is_empty(),
            "the function's 16 are all in the words: {only_fn:?}"
        );
    }

    /// Oracle: §4's is-auto-save-option list, 21 ordinals plus `HearPKDeaths`.
    #[test]
    fn exactly_twenty_two_options_are_saved_immediately() {
        assert_eq!(AUTO_SAVE_ORDINALS.len(), 22);
        let names: Vec<&str> = AUTO_SAVE_ORDINALS
            .iter()
            .map(|i| Options::name(*i))
            .collect();
        assert_eq!(
            names,
            vec![
                "AutoRepeatAttack",
                "IgnoreAllegianceRequests",
                "IgnoreFellowshipRequests",
                "FellowshipShareXP",
                "AcceptLootPermits",
                "FellowshipShareLoot",
                "FellowshipAutoAcceptRequests",
                "UseChargeAttack",
                "HearAllegianceChat",
                "HearGeneralChat",
                "HearTradeChat",
                "HearLFGChat",
                "HearRoleplayChat",
                "AppearOffline",
                "LeadMissileTargets",
                "UseFastMissiles",
                "HearSocietyChat",
                "ShowHelm",
                "UseMouseTurning",
                "ShowCloak",
                "LockUI",
                "HearPKDeaths",
            ]
        );
        // A window position is *not* auto-saved, which is the observable "my window positions were
        // lost when the client crashed" behaviour.
        assert!(!is_auto_save_option(option::ADVANCED_COMBAT_UI));
        assert!(!is_auto_save_option(option::MAIN_PACK_PREFERRED));
    }

    /// The newest option: ordinal 52 is second-word bit 25, set in the default word, saved to
    /// the shard the moment it changes, and read and written through the same bridge as the rest.
    #[test]
    fn hear_pk_deaths_is_ordinal_52_on_by_default_and_saved_at_once() {
        assert_eq!(PLAYER_OPTIONS.len(), 53);
        assert_eq!(
            PLAYER_OPTIONS[option::HEAR_PK_DEATHS],
            ("HearPKDeaths", OptionWord::Two, 0x0200_0000)
        );
        assert_eq!(DEFAULT_OPTIONS2, 0x0294_8700);
        assert_eq!(DEFAULT_OPTIONS2 & !0x0200_0000, 0x0094_8700);
        let mut o = Options::default();
        assert!(o.hear_pk_deaths(), "heard by default");
        assert!(is_auto_save_option(option::HEAR_PK_DEATHS));
        assert!(!default_option_value(option::HEAR_PK_DEATHS));
        assert!(o.set(option::HEAR_PK_DEATHS, false));
        assert_eq!(o.options2, 0x0094_8700, "only bit 25 moved");
        assert!(!o.hear_pk_deaths());
        assert_eq!(o.options, DEFAULT_OPTIONS, "the first word is untouched");
    }

    #[test]
    fn set_reports_whether_the_bit_actually_changed() {
        let mut o = Options::default();
        assert!(o.auto_repeat_attack());
        assert!(
            !o.set(option::AUTO_REPEAT_ATTACK, true),
            "no change, no hook"
        );
        assert!(o.set(option::AUTO_REPEAT_ATTACK, false));
        assert!(!o.auto_repeat_attack());
        assert_eq!(o.options & 0x2, 0);
        assert!(!o.set(99, true), "an ordinal outside 0..52 is invalid");
        assert_eq!(Options::name(99), "Invalid_PlayerOption");
    }

    /// Toggle run ships on so shift walks rather than runs.
    #[test]
    fn toggle_run_ships_on_so_shift_walks_rather_than_runs() {
        let o = Options::default();
        assert!(
            o.toggle_run(),
            "run is the DEFAULT movement state on a shipped character"
        );

        // The two statements of that default agree, and they could have disagreed: one is an
        // ordinal in the default-value query's list, the other is a bit in a packed word.
        assert!(DEFAULT_TRUE_ORDINALS.contains(&option::TOGGLE_RUN));
        assert_eq!(PLAYER_OPTIONS[option::TOGGLE_RUN].0, "ToggleRun");
        assert_eq!(PLAYER_OPTIONS[option::TOGGLE_RUN].2, 0x0000_0400);
        assert_eq!(DEFAULT_OPTIONS & 0x0000_0400, 0x0000_0400);

        // And the polarity that follows, spelled out so a reader does not have to compose the XOR
        // in their head: with the option on, the *held* key is the walk key.
        let effective_run = |hold_run: bool| hold_run != o.toggle_run();
        assert!(effective_run(false), "Shift up: running");
        assert!(!effective_run(true), "Shift down: walking");

        // The other arm of the same function, so this is not a one-sided pin: a player who turns
        // the option off gets the modifier the other way round, which is the behaviour the option
        // exists to offer.
        let mut off = Options::default();
        assert!(off.set(option::TOGGLE_RUN, false));
        let effective_run = |hold_run: bool| hold_run != off.toggle_run();
        assert!(!effective_run(false), "option off, Shift up: walking");
        assert!(effective_run(true), "option off, Shift down: running");
    }
}
