//! The character page's quality ids.
//!
//! Shared with `dereth_ui_screens::panels::characterinfo`, which keeps the panel and its string
//! tokens. What is here is the list of ids the page needs:
//! `dereth_client::hud` is what asks the qualities for them and what fills the view, and it names
//! `prop`, `LUMINANCE` and `AUGMENTATIONS` at eight sites while it does. Numbers and string
//! tokens; nothing draws.

/// The quality ids the character page reads that are not already fields of `CharacterInfo`.
/// Every one is read off the literal the page passes to the quality lookup.
pub mod prop {
    /// `0x62` — the creation timestamp, read by the birth/age/deaths row.
    pub const CREATION_TIMESTAMP: u32 = 98;
    /// `0x7D` — the age, and the one id the page registers a live quality handler for.
    pub const AGE: u32 = 125;
    /// `0x162` — the melee mastery, read by the augmentations row.
    pub const WEAPON_MASTERY: u32 = 354;
    /// `0x163` — the missile mastery, read beside it.
    pub const MISSILE_MASTERY: u32 = 355;
    /// `0x16A` — the summoning mastery, read beside it.
    pub const SUMMONING_MASTERY: u32 = 362;
    /// `0x186` — the enlightenment count, read by the birth/age/deaths row after the deaths line.
    pub const ENLIGHTENMENT: u32 = 390;
}

/// `%Mastery` for `ID_CharacterInfo_Mastery_Melee` — a switch on `value - 1` over
/// `0..=0xA`, with everything else falling through to `"Unknown"`.
///
/// The three holes (8, 9, 10) are the missile groups, which land on the default arm here and have
/// their own line; and `11` is `"Two Handed Weapons"`, at the **end** of the table rather than in
/// sequence. Both facts come from the table's dwords, not from the order the literals appear in.
#[must_use]
pub fn melee_mastery_name(v: i32) -> &'static str {
    match v {
        1 => "Unarmed Weapons",
        2 => "Swords",
        3 => "Axes",
        4 => "Maces",
        5 => "Spears",
        6 => "Daggers",
        7 => "Staves",
        11 => "Two Handed Weapons",
        _ => "Unknown",
    }
}

/// `%Mastery` for `ID_CharacterInfo_Mastery_Ranged` — a switch on `value - 8` over
/// `0..=4`.
///
/// `11` is inside that range and its entry points at the **default** arm, so it reads `"Unknown"`
/// here and `"Two Handed Weapons"` in [`melee_mastery_name`]. `12` is `"Magical Spells"`.
#[must_use]
pub fn ranged_mastery_name(v: i32) -> &'static str {
    match v {
        8 => "Bows",
        9 => "Crossbows",
        10 => "Thrown Weapons",
        12 => "Magical Spells",
        _ => "Unknown",
    }
}

/// `%Mastery` for `ID_CharacterInfo_Mastery_Summoning` — three compared arms, nothing tabular.
#[must_use]
pub fn summoning_mastery_name(v: i32) -> &'static str {
    match v {
        1 => "Primalist",
        2 => "Necromancer",
        3 => "Naturalist",
        _ => "Unknown",
    }
}

/// The eleven luminance ratings, in retail's order: `(quality id, base token, specialised token)`.
///
/// The four rows carrying a `Some(spec)` are the compare-against-5 pairs: the first five points are the
/// base aura and everything past five is the Seer's, and **both** lines can appear. The seven
/// with `None` are a single `if (v > 0)`.
///
/// `ID_CharacterInfo_Luminance_Base_Mana_Gain` is the shipped row's own spelling of *"The mana
/// provide by Mana Stones"*; it is quoted, not corrected.
pub const LUMINANCE: &[(u32, &str, Option<&str>)] = &[
    (
        0x14D,
        "ID_CharacterInfo_Luminance_Base_Damage",
        Some("ID_CharacterInfo_Luminance_Spec_Damage"),
    ),
    (
        0x14E,
        "ID_CharacterInfo_Luminance_Base_Reduction",
        Some("ID_CharacterInfo_Luminance_Spec_Reduction"),
    ),
    (
        0x14F,
        "ID_CharacterInfo_Luminance_Base_Crit_Damage",
        Some("ID_CharacterInfo_Luminance_Spec_Crit_Damage"),
    ),
    (
        0x150,
        "ID_CharacterInfo_Luminance_Base_Crit_Reduction",
        Some("ID_CharacterInfo_Luminance_Spec_Crit_Reduction"),
    ),
    (0x152, "ID_CharacterInfo_Luminance_Base_Surge_Chance", None),
    (0x153, "ID_CharacterInfo_Luminance_Base_Mana_Use", None),
    (0x154, "ID_CharacterInfo_Luminance_Base_Mana_Gain", None),
    (0x156, "ID_CharacterInfo_Luminance_Base_Healing", None),
    (0x157, "ID_CharacterInfo_Luminance_Base_Skilled_Craft", None),
    (0x158, "ID_CharacterInfo_Luminance_Spec_Skilled_Spec", None),
    (0x16D, "ID_CharacterInfo_Luminance_Base_All_Skills", None),
];

/// The 43 augmentation rows, in retail's order: `(quality id, token)`, each behind `if (v > 0)`.
///
/// Every one of them gets an augmentation-count integer variable in retail, including the
/// twenty whose shipped row has **no** variable slot at all (*"You earned the Infused War Magic
/// augmentation…"*). Retail's string resolution builds its value list from the
/// row's own variable list, so a variable the row does not name is never read — and
/// `dereth_ui::UiSystem::resolve_string_named` does not look it up for the same reason. That is
/// why this table has no "has a variable" column: the client does not have one either. Measured:
/// 24 of the 54 luminance and augmentation rows ship with no
/// variables at all, and `ID_CharacterInfo_Augmentation_JackOfAllTrades` is the pinned control.
///
/// `Resist_Nether` (`0x147`) is the one token with **no row in the shipped
/// `client_local_English.dat`** [measured against the shipped dat]. It stays in
/// the table because the client reads the quality and asks for the string; what a player sees is
/// whatever the missing-string path shows, and inventing a sentence for it would be worse.
pub const AUGMENTATIONS: &[(u32, &str)] = &[
    (0xDA, "ID_CharacterInfo_Augmentation_Attribute_Strength"),
    (0xDB, "ID_CharacterInfo_Augmentation_Attribute_Endurance"),
    (0xDC, "ID_CharacterInfo_Augmentation_Attribute_Coordination"),
    (0xDD, "ID_CharacterInfo_Augmentation_Attribute_Quickness"),
    (0xDE, "ID_CharacterInfo_Augmentation_Attribute_Focus"),
    (0xDF, "ID_CharacterInfo_Augmentation_Attribute_Self"),
    (0xF0, "ID_CharacterInfo_Augmentation_Resist_Slash"),
    (0xF1, "ID_CharacterInfo_Augmentation_Resist_Pierce"),
    (0xF2, "ID_CharacterInfo_Augmentation_Resist_Blunt"),
    (0xF3, "ID_CharacterInfo_Augmentation_Resist_Acid"),
    (0x147, "ID_CharacterInfo_Augmentation_Resist_Nether"),
    (0xF4, "ID_CharacterInfo_Augmentation_Resist_Fire"),
    (0xF5, "ID_CharacterInfo_Augmentation_Resist_Frost"),
    (0xF6, "ID_CharacterInfo_Augmentation_Resist_Lightning"),
    (0xE0, "ID_CharacterInfo_Augmentation_Spec_Salvaging"),
    (0xE1, "ID_CharacterInfo_Augmentation_Spec_ItemTinkering"),
    (0xE2, "ID_CharacterInfo_Augmentation_Spec_ArmorTinkering"),
    (
        0xE3,
        "ID_CharacterInfo_Augmentation_Spec_MagicItemTinkering",
    ),
    (0xE4, "ID_CharacterInfo_Augmentation_Spec_WeaponTinkering"),
    (0x125, "ID_CharacterInfo_Augmentation_Spec_Gearcraft"),
    (0xE5, "ID_CharacterInfo_Augmentation_ExtraPackSlot"),
    (
        0xE6,
        "ID_CharacterInfo_Augmentation_IncreasedCarryingCapacity",
    ),
    (0xE7, "ID_CharacterInfo_Augmentation_LessDeathItemLoss"),
    (0xE8, "ID_CharacterInfo_Augmentation_SpellsRemainPastDeath"),
    (0xE9, "ID_CharacterInfo_Augmentation_CriticalDefense"),
    (0xEA, "ID_CharacterInfo_Augmentation_BonusXP"),
    (0xEB, "ID_CharacterInfo_Augmentation_BonusSalvage"),
    (0xEC, "ID_CharacterInfo_Augmentation_BonusImbueChance"),
    (0xED, "ID_CharacterInfo_Augmentation_FasterRegen"),
    (0xEE, "ID_CharacterInfo_Augmentation_IncreasedSpellDuration"),
    (0x126, "ID_CharacterInfo_Augmentation_Infused_CreatureMagic"),
    (0x127, "ID_CharacterInfo_Augmentation_Infused_ItemMagic"),
    (0x128, "ID_CharacterInfo_Augmentation_Infused_LifeMagic"),
    (0x129, "ID_CharacterInfo_Augmentation_Infused_WarMagic"),
    (0x148, "ID_CharacterInfo_Augmentation_Infused_VoidMagic"),
    (0x12C, "ID_CharacterInfo_Augmentation_SkilledMelee"),
    (0x12D, "ID_CharacterInfo_Augmentation_SkilledMissile"),
    (0x12E, "ID_CharacterInfo_Augmentation_SkilledMagic"),
    (0x135, "ID_CharacterInfo_Augmentation_DamageBonus"),
    (0x136, "ID_CharacterInfo_Augmentation_DamageResist"),
    (0x12A, "ID_CharacterInfo_Augmentation_CriticalExpertise"),
    (0x12B, "ID_CharacterInfo_Augmentation_CriticalPower"),
    (0x146, "ID_CharacterInfo_Augmentation_JackOfAllTrades"),
];
