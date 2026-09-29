//! What the identify window and the *world* half of the client have to agree about.
//!
//! `dereth_ui_screens::panels::examination` is 6 000 lines of appraisal panel; these seven items are
//! the ones `dereth_client::hud` reads, at eleven sites, while it composes the `AppraisalView` the
//! panel draws. Every one is a table or a pure function over numbers -- the property keys the two
//! highlighting blocks ask about, the attribute and vital and skill name switches, the gear-rating
//! rows, the portal bitmask, the four description keys and the gem pluraliser. Nothing here draws.
//!
//! Each resolves through a `pub use` in `dereth_ui_screens::panels::examination`.

/// Every property the two highlighting blocks ask the enchantment-mod queries about, with whether
/// the question goes to the **float** table. `(key, is_float)`.
///
/// This is a property of the blocks rather than of the profile, which is why it lives here: each
/// entry is one enchantment-mod query inside the weapon-and-armour block or the armour-mods
/// block. The bit
/// pairs behind the keys are `dereth_client_model::appraisal`'s two tables and are not repeated here.
///
/// | key | float | the line it colours |
/// |---|---|---|
/// | `0x1C` | no | `Armor Level` |
/// | `0x2C` | no | `Damage` / `Damage Bonus`, tried first |
/// | `0x16` | yes | the same line's fallback, `DamageVariance` |
/// | `0x3F` | yes | `Damage Modifier` |
/// | `0x31` | no | `Speed` |
/// | `0x3E` | yes | `Bonus to Attack Skill` |
/// | `0x0D 0x0E 0x0F 0x11 0x10 0x12 0x13 0xA5` | yes | the eight resistances, in drawn order |
pub const HIGHLIGHTED_PROPERTIES: &[(u32, bool)] = &[
    (0x1C, false),
    (0x2C, false),
    (0x31, false),
    (0x16, true),
    // Three more enchantment-mod query sites, in two further blocks: the defence-mod block asks
    // about `0x1D` (and only about `0x1D`; its other two lines push a literal `0`), and the caster
    // block asks about `0x90` and `0x98`.
    (0x1D, true),
    (0x90, true),
    (0x98, true),
    (0x3E, true),
    (0x3F, true),
    (0x0D, true),
    (0x0E, true),
    (0x0F, true),
    (0x11, true),
    (0x10, true),
    (0x12, true),
    (0x13, true),
    (0xA5, true),
];

/// The attribute names — six wide literals chosen by a switch,
/// **not** a dat string.
///
/// Indexed by `STypeAttribute` (`1..=6`); the order here is the switch's, which is *not* the order
/// the creature pane draws them in. See `CREATURE_ATTRIBUTE_ROWS`.
#[must_use]
pub fn attribute_name(attribute: u32) -> Option<&'static str> {
    Some(match attribute {
        1 => "Strength",
        2 => "Endurance",
        3 => "Quickness",
        4 => "Coordination",
        5 => "Focus",
        6 => "Self",
        _ => return None,
    })
}

/// The secondary-attribute names — the same shape, by `STypeAttribute2nd`.
#[must_use]
pub fn vital_name(vital: u32) -> Option<&'static str> {
    Some(match vital {
        1 => "Maximum Health",
        2 => "Health",
        3 => "Maximum Stamina",
        4 => "Stamina",
        5 => "Maximum Mana",
        6 => "Mana",
        _ => return None,
    })
}

/// The appraisal system's skill names — a 54-arm switch, in the order the literals sit in the
/// binary.
///
/// **Not the `SkillTable`.** The dat table's `name` field is the skill panel's label; this switch
/// is the appraisal panel's, and the two differ (the table calls 47 `Missile Weapons` too, but the
/// switch is what the identify window reads and is what is transcribed). Recovered by following
/// each of the 0x36 jump-table entries to its first literal — anything
/// outside `1..=54` answers `false`, which skips the whole `Skill:` line.
#[must_use]
pub fn skill_to_string(skill: u32) -> Option<&'static str> {
    const NAMES: [&str; 54] = [
        "Axe",
        "Bow",
        "Crossbow",
        "Dagger",
        "Mace",
        "Melee Defense",
        "Missile Defense",
        "Sling",
        "Spear",
        "Staff",
        "Sword",
        "Thrown Weapon",
        "Unarmed Combat",
        "Arcane Lore",
        "Magic Defense",
        "Mana Conversion",
        "Spellcraft",
        "Item Tinkering",
        "Person Appraisal",
        "Deception",
        "Healing",
        "Jump",
        "Lockpick",
        "Run",
        "Awareness",
        "Armor Repair",
        "Creature Appraisal",
        "Weapon Tinkering",
        "Armor Tinkering",
        "Magic Item Tinkering",
        "Creature Enchantment",
        "Item Enchantment",
        "Life Magic",
        "War Magic",
        "Leadership",
        "Loyalty",
        "Fletching",
        "Alchemy",
        "Cooking",
        "Salvaging",
        "Two Handed Combat",
        "Gearcraft",
        "Void Magic",
        "Heavy Weapons",
        "Light Weapons",
        "Finesse Weapons",
        "Missile Weapons",
        "None",
        "Dual Wield",
        "Recklessness",
        "Sneak Attack",
        "Dirty Fighting",
        "Challenge",
        "Summoning",
    ];
    usize::try_from(skill)
        .ok()
        .and_then(|i| i.checked_sub(1))
        .and_then(|i| NAMES.get(i))
        .copied()
}

/// The gear-rating block's thirteen joined terms, in the order it **draws** them.
///
/// `(property, label)`. The fourteen integer queries are made in the order
/// `0x172 0x173 0x174 0x176 0x175 0x177 0x178 0x17A 0x179 0x17B 0x17F 0x180 0x184 0x185`, but
/// the drawn order is the one below: the *"Nether Resist"* term is guarded by the slot `0x179`
/// was read into, so those two swap back, and the four newest terms (overpower first, then
/// player-killer damage) sit between *"Crit Dam Resist"* and *"Heal Boost"*.
///
/// The ACE names line up term for term: `370 GearDamage`, `371 GearDamageResist`, `372 GearCrit`,
/// `374 GearCritDamage`, `373 GearCritResist`, `375 GearCritDamageResist`, `388 GearOverpower`,
/// `389 GearOverpowerResist`, `383 GearPKDamageRating`, `384 GearPKDamageResistRating`,
/// `376 GearHealingBoost`, `377 GearNetherResist`, `378 GearLifeResist`; `379 GearMaxHealth` is
/// read too, and is a sentence of its own rather than a term.
///
/// The two overpower labels carry their `%` sign in the text itself: *"Overpower% 12"*.
pub const GEAR_RATING_ROWS: [(u32, &str); 13] = [
    (0x172, "Dam"),
    (0x173, "Dam Resist"),
    (0x174, "Crit"),
    (0x176, "Crit Dam"),
    (0x175, "Crit Resist"),
    (0x177, "Crit Dam Resist"),
    (0x184, "Overpower%"),
    (0x185, "Overpower Reduction%"),
    (0x17F, "PK Dam"),
    (0x180, "PK Dam Resist"),
    (0x178, "Heal Boost"),
    (0x179, "Nether Resist"),
    (0x17A, "Life Resist"),
];

/// `GearMaxHealth`, the vitality sentence's property; it is read after the thirteen terms and
/// held in the slot after them.
pub const GEAR_MAX_HEALTH: u32 = 0x17B;

/// `PropertyInt.PortalBitmask` — the literal the examine block pushes, ACE's 111.
pub const PORTAL_BITMASK: u32 = 0x6F;

/// The three presence-gated integer keys at the head of the item-examine description block.
pub const LIFESPAN: u32 = 0x10B;
pub const CREATION_TIMESTAMP: u32 = 0x62;
pub const REMAINING_LIFESPAN: u32 = 0x10C;
/// The client's fallback string key.
pub const SHORT_DESC: u32 = 0x0F;

/// The pluralised gem name; the material mapper's singular name is
/// supplied by the host. The client ignores a mapper miss and still applies its suffix.
#[must_use]
pub fn pluralized_gem_name(material: u32, name: &str) -> String {
    match material {
        0x26 => "Rubies".to_owned(),
        0x0B | 0x18 | 0x1B | 0x1D | 0x20 | 0x25 | 0x28 | 0x2E | 0x24 | 0x2D => {
            format!("pieces of {name}")
        }
        0x1A | 0x31 => format!("{name}es"),
        0x1C => name.to_owned(),
        _ => format!("{name}s"),
    }
}
