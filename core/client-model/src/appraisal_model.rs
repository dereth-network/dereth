//! The appraisal profile's own accessors — the half of `ExaminationPanel` that reads.
//!
//! `crate::appraisal` owns the cache, the poll and the highlight bitfields. This file is the
//! *reading* half: primary-attribute lookup and its four siblings, the property
//! keys used by each appraisal display block, and the two `PublicWeenieDesc` enums those blocks
//! branch on. Nothing here formats: the strings belong to the item-examination panel.

use dereth_protocol::types::AppraisalProfile;

/// `CreatureAppraisalProfile`'s ten-dword `attributes` block, by wire index.
///
/// The profile decoder writes them in **this** order, which is not the
/// order the panel asks for them in:
///
/// ```text
/// strength endurance quickness coordination focus self stamina mana max_stamina max_mana
/// ```
///
/// `health` and `max_health` are the two dwords **outside** the block and are always present.
pub mod creature_slot {
    pub const STRENGTH: usize = 0;
    pub const ENDURANCE: usize = 1;
    pub const QUICKNESS: usize = 2;
    pub const COORDINATION: usize = 3;
    pub const FOCUS: usize = 4;
    pub const SELF: usize = 5;
    pub const STAMINA: usize = 6;
    pub const MANA: usize = 7;
    pub const MAX_STAMINA: usize = 8;
    pub const MAX_MANA: usize = 9;
}

/// Behavior: the six primary attributes by
/// `STypeAttribute`, and **zero is absent**.
///
/// The function's own answer is `*out != 0`, so a zero attribute and an unsent one are the same
/// thing to the panel: prints `"???"` for both.
#[must_use]
pub fn creature_attribute(p: &AppraisalProfile, attribute: u32) -> Option<u32> {
    let a = p.creature_profile?.attributes?;
    let v = match attribute {
        1 => a[creature_slot::STRENGTH],
        2 => a[creature_slot::ENDURANCE],
        3 => a[creature_slot::QUICKNESS],
        4 => a[creature_slot::COORDINATION],
        5 => a[creature_slot::FOCUS],
        6 => a[creature_slot::SELF],
        _ => return None,
    };
    (v != 0).then_some(v)
}

/// Behavior: the six vitals by `STypeAttribute2nd`,
/// likewise zero-is-absent.
///
/// `1 MaxHealth, 2 Health, 3 MaxStamina, 4 Stamina, 5 MaxMana, 6 Mana`. Health and its maximum are
/// the profile's own two fields; the other four live in the attribute block, which is why a
/// profile whose `flags & 0x08` is clear answers for health and for nothing else.
#[must_use]
pub fn creature_vital(p: &AppraisalProfile, vital: u32) -> Option<u32> {
    let c = p.creature_profile?;
    let v = match vital {
        1 => c.max_health,
        2 => c.health,
        3 => c.attributes?[creature_slot::MAX_STAMINA],
        4 => c.attributes?[creature_slot::STAMINA],
        5 => c.attributes?[creature_slot::MAX_MANA],
        6 => c.attributes?[creature_slot::MANA],
        _ => return None,
    };
    (v != 0).then_some(v)
}

/// Compute the enchantment modifier for a primary attribute.
///
/// `Some(true)` is enchanted and beneficial, `Some(false)` enchanted and harmful, `None` plain —
/// the same low-bit/high-bit pair as [`crate::appraisal::highlight_state`], sixteen apart, on the
/// creature profile's own bitfield rather than on one of the three item ones.
#[must_use]
pub fn creature_attribute_enchanted(p: &AppraisalProfile, attribute: u32) -> Option<bool> {
    let bits = p.creature_profile?.enchantment_bitfield?;
    if !(1..=6).contains(&attribute) {
        return None;
    }
    let low = 1u32 << (attribute - 1);
    (bits & low != 0).then_some(bits & (low << 16) != 0)
}

/// Compute the enchantment modifier for a secondary attribute — **the maxima only**:
/// `1 MaxHealth` (`0x40`), `3 MaxStamina` (`0x80`), `5 MaxMana` (`0x100`). A current vital has no
/// bit, which is why the vital-info update asks about the maximum and not the current value.
#[must_use]
pub fn creature_vital_enchanted(p: &AppraisalProfile, max_vital: u32) -> Option<bool> {
    let bits = p.creature_profile?.enchantment_bitfield?;
    let low = match max_vital {
        1 => 0x40u32,
        3 => 0x80,
        5 => 0x100,
        _ => return None,
    };
    (bits & low != 0).then_some(bits & (low << 16) != 0)
}

/// Query the integer, float, string, and boolean property tables by key.
///
/// Each is a linear scan of the `PackedHash`'s entries, equivalent to the client's hash-table
/// lookup for tables this size.
pub mod inq {
    use dereth_protocol::types::AppraisalProfile;

    #[must_use]
    pub fn int(p: &AppraisalProfile, k: u32) -> Option<i32> {
        p.tables
            .ints
            .as_ref()?
            .entries
            .iter()
            .find(|(i, _)| *i == k)
            .map(|(_, v)| *v)
    }

    #[must_use]
    pub fn float(p: &AppraisalProfile, k: u32) -> Option<f64> {
        p.tables
            .floats
            .as_ref()?
            .entries
            .iter()
            .find(|(i, _)| *i == k)
            .map(|(_, v)| *v)
    }

    #[must_use]
    pub fn string(p: &AppraisalProfile, k: u32) -> Option<String> {
        p.tables
            .strings
            .as_ref()?
            .entries
            .iter()
            .find(|(i, _)| *i == k)
            .map(|(_, v)| v.clone())
    }

    #[must_use]
    pub fn boolean(p: &AppraisalProfile, k: u32) -> Option<bool> {
        p.tables
            .bools
            .as_ref()?
            .entries
            .iter()
            .find(|(i, _)| *i == k)
            .map(|(_, v)| *v != 0)
    }

    /// Query the `int64s` table. The appraisal panel asks it twice (key 5 for
    /// `ItemBaseXp`, key 4 for `ItemTotalXp`) and is its only caller in the pane.
    #[must_use]
    pub fn int64(p: &AppraisalProfile, k: u32) -> Option<i64> {
        p.tables
            .int64s
            .as_ref()?
            .entries
            .iter()
            .find(|(i, _)| *i == k)
            .map(|(_, v)| *v)
    }

    /// Query the `dids` table. The client's DataID quality `0x37` is its first caller.
    #[must_use]
    pub fn data_id(p: &AppraisalProfile, k: u32) -> Option<u32> {
        p.tables
            .dids
            .as_ref()?
            .entries
            .iter()
            .find(|(i, _)| *i == k)
            .map(|(_, v)| *v)
    }
}

/// The `PropertyInt` keys `ExaminationPanel`'s blocks ask for, named where they are read.
pub mod property {
    /// `EncumbranceVal`, read by the burden-information block.
    pub const BURDEN: u32 = 5;
    /// `CreatureType` — the client's int quality `2`, fed to the creature display-name resolver.
    pub const CREATURE_TYPE: u32 = 2;
    /// `Value`, read by the value-information block.
    pub const VALUE: u32 = 0x13;
    /// `Level`, read from integer quality `0x19` for the basic creature appraisal view.
    pub const LEVEL: u32 = 0x19;
    /// `ArmorLevel` — int quality `0x1C` in both the armor and shield blocks.
    pub const ARMOR_LEVEL: u32 = 0x1C;
    /// `WeaponType` — integer quality `0x161` in the weapon-and-armor block, the parenthesised
    /// weapon family appended to the skill name.
    pub const WEAPON_TYPE: u32 = 0x161;
    /// `CharacterTitleId` — the character-pane fork's second term.
    pub const CHARACTER_TITLE_ID: u32 = 0x105;
    /// `ElementalDamageBonus` — int quality `0xCC`.
    pub const ELEMENTAL_DAMAGE_BONUS: u32 = 0xCC;
    /// `ResistLockpick` — the client's int quality `0x26`, queried on both the locked and unlocked
    /// arms.
    ///
    /// It is the block's own second gate as well as the `%d` of its last line: with the lock table
    /// present and this key absent the client prints *"You can't tell how hard the lock is to
    /// pick."* and stops.
    pub const RESIST_LOCKPICK: u32 = 0x26;
    /// `AppraisalLockpickSuccessPercent` — int quality `0xAD`, formatted as a lockpick-success
    /// percentage.
    pub const APPRAISAL_LOCKPICK_SUCCESS_PERCENT: u32 = 0xAD;

    /// The `PropertyInt`, `PropertyFloat`, `PropertyBool` and `PropertyDataId` keys that the
    /// appraisal panel reads, in the order it reads
    /// them. Every one is the literal key of a query call in the
    /// retail appraisal block.
    pub mod special {
        /// `MaxStackSize`-style carry limit, int quality `0x117`.
        pub const UNIQUE_LIMIT: u32 = 0x117;
        /// float quality `0xA7` — the cooldown pair's first half.
        pub const COOLDOWN_DURATION: u32 = 0xA7;
        /// int quality `0x118` — its second.
        pub const SHARED_COOLDOWN: u32 = 0x118;
        /// int quality `0x124` — *"Cleave: %d enemies in front arc."*, drawn only when the value is
        /// above 1; values 0 and 1 take the skip branch.
        pub const CLEAVE: u32 = 0x124;
        /// `SlayerCreatureType`, int quality `0xA6`, is fed to the **same** `EnumMapper` the creature
        /// pane's [`super::CREATURE_TYPE`] uses.
        pub const SLAYER_CREATURE_TYPE: u32 = 0xA6;
        /// `WeaponSkill`, int quality `0x2F`. *"Multi-Strike"* is `value & 0x79E0`.
        pub const WEAPON_SKILL: u32 = 0x2F;
        /// `ImbuedEffect`, int quality `0xB3`, and its four continuations `0x12F`,
        /// `0x130`, `0x131`, `0x132`... The five are **or**-ed into one
        /// mask before any name is drawn.
        pub const IMBUED_EFFECT: [u32; 5] = [0xB3, 0x12F, 0x130, 0x131, 0x132];
        /// float quality `0x9F` — *"Magic Absorbing"*, on presence alone.
        pub const ABSORB_MAGIC_DAMAGE: u32 = 0x9F;
        /// `ItemMaxMana`-adjacent int quality `0x24` — *"Unenchantable"* when the value
        /// is **above 9998** (`0x270E`).
        pub const ITEM_SPELLCRAFT: u32 = 0x24;
        /// `Attuned`, int quality `0x72`. The attuned-status text answers
        /// *"Attuned"* for `1..=2` and nothing for anything else.
        pub const ATTUNED: u32 = 0x72;
        /// `Bonded`, int quality `0x21`. The bonded-status text answers
        /// *"Destroyed on Death"* for `-2`, *"Dropped on Death"* for `-1` and *"Bonded"* for `1`.
        pub const BONDED: u32 = 0x21;
        /// bool quality `0x5B` — *"Retained"*, and the test is `== 1`.
        pub const RETAINED: u32 = 0x5B;
        /// float quality `0x88` — *"Crushing Blow"*.
        pub const CRITICAL_MULTIPLIER: u32 = 0x88;
        /// float quality `0x93` — *"Biting Strike"*.
        pub const CRITICAL_FREQUENCY: u32 = 0x93;
        /// float quality `0x9B` — *"Armor Cleaving"*.
        pub const IGNORE_ARMOR: u32 = 0x9B;
        /// float quality `0x9D`, paired with int quality `0x107` — both are
        /// needed, and only the second reaches the text.
        pub const IGNORE_SHIELD: u32 = 0x9D;
        /// See `Self::IGNORE_SHIELD`; turns it into the `%s` of
        /// *"Resistance Cleaving: %s"*.
        pub const RESISTANCE_MODIFIER_TYPE: u32 = 0x107;
        /// DataID quality `0x37` — *"Cast on Strike"*, on presence alone.
        pub const PROC_SPELL: u32 = 0x37;
        /// bool quality `0x63` — *"Ivoryable"*, `== 1`.
        pub const IVORYABLE: u32 = 0x63;
        /// bool quality `0x64` — *"Dyeable"*, `== 1`.
        pub const DYEABLE: u32 = 0x64;
        /// bool quality `0x82` — *"This item is tethered to the left side."*, `== 1`.
        pub const TETHERED_LEFT: u32 = 0x82;
    }

    /// The four `PropertyInt` keys the item-level block reads, each pushed immediately before an
    /// integer query.
    ///
    /// All four are inside the block's own `if (any non-enchantment spell)` gate,
    /// so an item whose only spell ids carry `0x80000000` shows none of them.
    pub mod magic {
        /// `ItemSpellcraft`, int quality `0x6A` — *"Spellcraft: %d."*.
        ///
        /// **Not** [`super::special::ITEM_SPELLCRAFT`], which is `0x24`: the special-properties
        /// block compares that key against `0x270F` for *"Unenchantable"*, and this is the one the
        /// client itself labels *Spellcraft*.
        pub const ITEM_SPELLCRAFT: u32 = 0x6A;
        /// `ItemCurMana`, int quality `0x6B` — the first `%d` of *"Mana: %d / %d."*.
        pub const ITEM_CUR_MANA: u32 = 0x6B;
        /// `ItemMaxMana`, int quality `0x6C` — the second. Both must be present or
        /// neither line is drawn; both failed queries jump to the same skip.
        pub const ITEM_MAX_MANA: u32 = 0x6C;
        /// `ItemManaCost`, int quality `0x75` — *"Mana Cost: %d."*, reached **only**
        /// when [`super::float::MANA_RATE`] is absent.
        pub const ITEM_MANA_COST: u32 = 0x75;
    }

    /// The `PropertyFloat` keys.
    pub mod float {
        /// `ManaRate` — float quality 5 in the magic-information block.
        ///
        /// Its presence takes the *"Mana Cost: 1 point per %d seconds."* arm and the `%d` is
        /// `|1.0 / rate| + 0.5`, truncated to an integer.
        pub const MANA_RATE: u32 = 5;
    }

    /// The `PropertyBool` keys.
    pub mod boolean {
        /// `Locked` — bool quality 3 in the lock-appraisal block,
        /// and the same `PropertyBool` `0x02D2 Qualities_UpdateBool` carries when a key turns.
        pub const LOCKED: u32 = 3;
    }

    /// The `PropertyString` keys.
    pub mod string {
        /// `Inscription` — `SetInscription`'s string quality `7`.
        pub const INSCRIPTION: u32 = 7;
        /// `ScribeName` — string quality `8`; its absence is what puts `<Inscribe here>` in the box.
        pub const SCRIBE_NAME: u32 = 8;
        /// `Use` — string quality `0x0E` in the usage block.
        pub const USE: u32 = 0x0E;
        /// `LongDesc` — string quality `0x10` in the description block.
        pub const LONG_DESC: u32 = 0x10;
        /// `ScribeAccount` — string quality `0x17`, PSR-only.
        pub const SCRIBE_ACCOUNT: u32 = 0x17;
        /// `Template` — the character-pane fork's first term.
        pub const TEMPLATE: u32 = 5;
        /// `GearPlatingName` — the appraisal panel's title override.
        pub const GEAR_PLATING_NAME: u32 = 0x34;
    }
}

/// `AMMO_TYPE` from the public object description, read by the two ammunition arms of
/// the weapon-and-armor appraisal block.
pub mod ammo_type {
    pub const NONE: u16 = 0;
    pub const ARROW: u16 = 1;
    pub const BOLT: u16 = 2;
    pub const ATLATL: u16 = 3;
}

/// The `EquipMask` bits tests on `valid_locations`.
///
/// In retail, `& 0x3F00000` gates the whole weapon half, `0x200000` the shield
/// block, `0x400000` everything that depends on being a *launcher* (the ammunition line, the
/// `Damage Bonus` label, the range), `0x800000` the "used as ammunition by" arm, `0x2500000` the
/// speed-and-range pair, and `0x8007FFF` the clothing-priority fallback.
pub mod equip {
    pub const MELEE_WEAPON: u32 = 0x0010_0000;
    pub const SHIELD: u32 = 0x0020_0000;
    pub const MISSILE_WEAPON: u32 = 0x0040_0000;
    pub const AMMUNITION: u32 = 0x0080_0000;
    pub const WAND: u32 = 0x0200_0000;
    /// `mask & 0x3F00000` — any weapon-ish slot at all.
    pub const ANY_WEAPON: u32 = 0x03F0_0000;
    /// `mask & 0x2500000` — the slots that show Speed (and, with `MISSILE_WEAPON`, Range).
    pub const SPEED_SHOWN: u32 = MELEE_WEAPON | MISSILE_WEAPON | WAND;
    /// `mask & 0x8007FFF` — armour and clothing, the clothing-priority-name fallback.
    pub const CLOTHING: u32 = 0x0800_7FFF;
}

/// Public object-description bit `0x2`, which the inscription display reads before it looks at the
/// profile at all.
pub const OBJECT_DESC_INSCRIBABLE: u32 = 0x0000_0002;

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_protocol::types::CreatureAppraisalProfile;

    /// Oracle: the client's ten profile writes in wire order plus the primary- and
    /// secondary-attribute lookup switches.
    ///
    /// The block's order is **not** the switches' order — quickness is wire slot 2 and attribute
    /// 3, coordination wire slot 3 and attribute 4 — and the pane draws coordination *before*
    /// quickness, so all three orders differ. This is the test that pins them apart.
    #[test]
    fn the_wire_order_the_attribute_ids_and_the_drawn_order_are_three_different_orders() {
        let p = AppraisalProfile {
            creature_profile: Some(CreatureAppraisalProfile {
                flags: CreatureAppraisalProfile::HAS_ATTRIBUTES,
                health: 12,
                max_health: 31,
                // strength endurance quickness coordination focus self stamina mana maxst maxmn
                attributes: Some([10, 20, 30, 40, 50, 60, 7, 8, 70, 80]),
                enchantment_bitfield: None,
            }),
            ..AppraisalProfile::default()
        };
        assert_eq!(creature_attribute(&p, 1), Some(10), "Strength");
        assert_eq!(creature_attribute(&p, 2), Some(20), "Endurance");
        assert_eq!(
            creature_attribute(&p, 3),
            Some(30),
            "Quickness is wire slot 2"
        );
        assert_eq!(
            creature_attribute(&p, 4),
            Some(40),
            "Coordination is wire slot 3"
        );
        assert_eq!(creature_attribute(&p, 5), Some(50), "Focus");
        assert_eq!(creature_attribute(&p, 6), Some(60), "Self");
        assert_eq!(creature_attribute(&p, 0), None);
        assert_eq!(creature_attribute(&p, 7), None);

        assert_eq!(
            creature_vital(&p, 1),
            Some(31),
            "MaxHealth is outside the block"
        );
        assert_eq!(
            creature_vital(&p, 2),
            Some(12),
            "Health is outside the block"
        );
        assert_eq!(creature_vital(&p, 3), Some(70), "MaxStamina");
        assert_eq!(creature_vital(&p, 4), Some(7), "Stamina");
        assert_eq!(creature_vital(&p, 5), Some(80), "MaxMana");
        assert_eq!(creature_vital(&p, 6), Some(8), "Mana");
    }

    /// Oracle: both `Inq*`'s `return *out != 0`, and `UnPack`'s clear-zeroes-the-block branch.
    ///
    /// A profile without `flags & 0x08` still answers for health, and for nothing else — the
    /// distinction the panel turns into `31/31` beside `???`.
    #[test]
    fn a_profile_with_no_attribute_block_answers_for_health_alone() {
        let p = AppraisalProfile {
            creature_profile: Some(CreatureAppraisalProfile {
                flags: 0,
                health: 5,
                max_health: 5,
                attributes: None,
                enchantment_bitfield: None,
            }),
            ..AppraisalProfile::default()
        };
        assert_eq!(creature_vital(&p, 2), Some(5));
        assert_eq!(creature_vital(&p, 1), Some(5));
        assert_eq!(creature_vital(&p, 4), None, "no block, no stamina");
        assert_eq!(creature_attribute(&p, 1), None);

        // Zero is absent, not zero: the client's own `!= 0`.
        let z = AppraisalProfile {
            creature_profile: Some(CreatureAppraisalProfile {
                flags: CreatureAppraisalProfile::HAS_ATTRIBUTES,
                health: 0,
                max_health: 0,
                attributes: Some([0; 10]),
                enchantment_bitfield: None,
            }),
            ..AppraisalProfile::default()
        };
        assert_eq!(creature_attribute(&z, 1), None);
        assert_eq!(creature_vital(&z, 2), None);
    }

    /// Oracle: the six primary-attribute cases and the **three** secondary-attribute maximum cases.
    #[test]
    fn only_the_three_maximum_vitals_carry_an_enchantment_bit() {
        let with = |bits: u32| AppraisalProfile {
            creature_profile: Some(CreatureAppraisalProfile {
                flags: CreatureAppraisalProfile::HAS_ENCHANTMENTS,
                enchantment_bitfield: Some(bits),
                ..CreatureAppraisalProfile::default()
            }),
            ..AppraisalProfile::default()
        };
        assert_eq!(
            creature_attribute_enchanted(&with(0x1), 1),
            Some(false),
            "harmful"
        );
        assert_eq!(
            creature_attribute_enchanted(&with(0x1 | 0x1_0000), 1),
            Some(true)
        );
        assert_eq!(
            creature_attribute_enchanted(&with(0x1_0000), 1),
            None,
            "high bit alone"
        );
        assert_eq!(
            creature_attribute_enchanted(&with(0x20), 6),
            Some(false),
            "Self is 0x20"
        );
        assert_eq!(creature_attribute_enchanted(&with(0xFFFF_FFFF), 7), None);

        assert_eq!(
            creature_vital_enchanted(&with(0x40), 1),
            Some(false),
            "MaxHealth"
        );
        assert_eq!(
            creature_vital_enchanted(&with(0x80), 3),
            Some(false),
            "MaxStamina"
        );
        assert_eq!(
            creature_vital_enchanted(&with(0x100), 5),
            Some(false),
            "MaxMana"
        );
        assert_eq!(
            creature_vital_enchanted(&with(0xFFFF_FFFF), 2),
            None,
            "Health has no bit"
        );
        assert_eq!(creature_vital_enchanted(&with(0xFFFF_FFFF), 4), None);
        assert_eq!(creature_vital_enchanted(&with(0xFFFF_FFFF), 6), None);
    }

    /// The four equip-mask constants are pinned exactly by this test.
    #[test]
    fn the_equip_masks_are_the_functions_own_immediates() {
        assert_eq!(equip::ANY_WEAPON, 0x03F0_0000);
        assert_eq!(equip::SPEED_SHOWN, 0x0250_0000);
        assert_eq!(equip::CLOTHING, 0x0800_7FFF);
        assert_eq!(equip::SHIELD, 0x0020_0000);
        assert_eq!(equip::MISSILE_WEAPON, 0x0040_0000);
        assert_eq!(equip::AMMUNITION, 0x0080_0000);
    }
}
