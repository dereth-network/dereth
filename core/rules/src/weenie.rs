//! `ITEM_TYPE`, the object-type bitmask the rules test, and the public-description `_bitfield` bits.

/// `ITEM_TYPE` (`pwd._type`) — a **bitmask**, not an ordinal.
///
/// The composites that look like typos are not: `TYPE_ITEM` is `0x2DFBEF` and
/// the vendor-shopkeeper composite is `0x480467A7`.
pub mod item_type {
    pub const UNDEF: u32 = 0x0000_0000;
    pub const MELEE_WEAPON: u32 = 0x0000_0001;
    pub const ARMOR: u32 = 0x0000_0002;
    pub const CLOTHING: u32 = 0x0000_0004;
    pub const VESTEMENTS: u32 = ARMOR | CLOTHING;
    pub const JEWELRY: u32 = 0x0000_0008;
    pub const CREATURE: u32 = 0x0000_0010;
    pub const FOOD: u32 = 0x0000_0020;
    pub const MONEY: u32 = 0x0000_0040;
    pub const MISC: u32 = 0x0000_0080;
    pub const MISSILE_WEAPON: u32 = 0x0000_0100;
    pub const WEAPON: u32 = MELEE_WEAPON | MISSILE_WEAPON;
    pub const CONTAINER: u32 = 0x0000_0200;
    pub const LOCKABLE_MAGIC_TARGET: u32 = CONTAINER | MISC;
    pub const USELESS: u32 = 0x0000_0400;
    pub const GEM: u32 = 0x0000_0800;
    pub const SPELL_COMPONENTS: u32 = 0x0000_1000;
    pub const WRITABLE: u32 = 0x0000_2000;
    pub const KEY: u32 = 0x0000_4000;
    pub const CASTER: u32 = 0x0000_8000;
    pub const WEAPON_OR_CASTER: u32 = 0x0000_8101;
    pub const REDIRECTABLE_ITEM_ENCHANTMENT_TARGET: u32 = 0x0000_8107;
    pub const PORTAL: u32 = 0x0001_0000;
    pub const LOCKABLE: u32 = 0x0002_0000;
    pub const PROMISSORY_NOTE: u32 = 0x0004_0000;
    pub const MANASTONE: u32 = 0x0008_0000;
    pub const ITEM_ENCHANTABLE_TARGET: u32 = 0x0008_8B8F;
    pub const SERVICE: u32 = 0x0010_0000;
    pub const MAGIC_WIELDABLE: u32 = 0x0020_0000;
    /// "Anything that is an item". Not a typo.
    pub const ITEM: u32 = 0x002D_FBEF;
    pub const CRAFT_COOKING_BASE: u32 = 0x0040_0000;
    pub const VENDOR_GROCER: u32 = 0x0044_6220;
    pub const CRAFT_ALCHEMY_BASE: u32 = 0x0080_0000;
    pub const CRAFT_FLETCHING_BASE: u32 = 0x0100_0000;
    pub const CRAFT_ALCHEMY_INTERMEDIATE: u32 = 0x0400_0000;
    pub const CRAFT_FLETCHING_INTERMEDIATE: u32 = 0x0800_0000;
    pub const LIFESTONE: u32 = 0x1000_0000;
    pub const PORTAL_MAGIC_TARGET: u32 = 0x1001_0000;
    pub const TINKERING_TOOL: u32 = 0x2000_0000;
    pub const TINKERING_MATERIAL: u32 = 0x4000_0000;
    /// Not a typo either.
    pub const VENDOR_SHOPKEEP: u32 = 0x4804_67A7;
    pub const GAMEBOARD: u32 = 0x8000_0000;
}

/// The bits the retail client actually reads.
///
/// The client has no accessor for bits 8–12 and 14–20. Their
/// meanings come from ACE, so the word is stored opaquely and no branch depends on an inferred
/// bit; those bits are deliberately **not** named here.
///
/// Bit 4 is not on that list: the combat attackability check reads it, so it is named below.
/// Claims that nothing reads a field are only as complete as the search that produced them.
pub mod bitfield {
    pub const OPENABLE: u32 = 0x0000_0001;
    pub const INSCRIBABLE: u32 = 0x0000_0002;
    pub const STUCK: u32 = 0x0000_0004;
    pub const PLAYER: u32 = 0x0000_0008;
    /// **Bit 4, and it does have a reader.** The combat-system attackability check ends
    /// by returning bit 4 of the public bitfield, which is the entire answer for an ordinary
    /// non-player, non-pet creature. The retail enum names public object-description bit 16 as
    /// the attackable flag. \[verified\]
    pub const ATTACKABLE: u32 = 0x0000_0010;
    pub const PLAYER_KILLER: u32 = 0x0000_0020;
    pub const HIDDEN_ADMIN: u32 = 0x0000_0040;
    pub const UI_HIDDEN: u32 = 0x0000_0080;
    /// **Bit 9, and it has a reader too.** The place-in-3D path tests this one on the public
    /// bitfield to decide whether a dropped item is being **sold**. The retail enum names public
    /// object-description bit 512 as the vendor flag. \[verified\]
    pub const VENDOR: u32 = 0x0000_0200;
    pub const CORPSE: u32 = 0x0000_2000;
    /// `BF_HEALER = 65536` in the retail enum. The healing-kit and remaining-uses appraisal blocks
    /// all
    /// test it as a single-bit check, and it is what makes the identify pane say
    /// *"Bonus to Healing Skill"* instead of *"Restores %d Health when used."*.
    pub const HEALER: u32 = 0x0001_0000;
    /// `BF_LOCKPICK = 131072`. The other classification bit in the client's
    /// two-bit healer-or-lockpick check.
    pub const LOCKPICK: u32 = 0x0002_0000;
    pub const IMPENETRABLE: u32 = 0x0020_0000;
    /// **Bit 20, and it has a reader.** The move-restriction bypass test reads it
    /// **together with**
    /// [`CELL_BARRIER_IMMUNE`], and nothing else in the client reads it. ACE names the same bit
    /// `ObjectDescriptionFlag.Admin`.
    pub const ADMIN: u32 = 0x0010_0000;
    pub const CELL_BARRIER_IMMUNE: u32 = 0x0040_0000;
    /// The salvage-material query reads this and nothing else does; retain it opaquely and give it
    /// no other meaning.
    pub const CANNOT_BE_SALVAGED: u32 = 0x0100_0000;
    pub const PK_LITE: u32 = 0x0200_0000;
    /// A `RestrictionDB` and a second header dword follow.
    pub const HAS_RESTRICTIONS: u32 = 0x0400_0000;

    /// Combat attackability tests these two bits together.
    pub const PLAYER_KILLER_ANY: u32 = PLAYER_KILLER | PK_LITE;
}

/// Maps a spell formula's **target component** to the object types the spell may be cast at.
///
/// The creature-target components (0x31–0x38, 0x3C–0x3E and 0xBE) aim at creatures, 0x39 at
/// enchantable items and 0x3B at portals and lifestones. Every other component names no target, and a
/// spell whose target component is one of those is cast untargeted. See `docs/formats/30-spell-tables.md`.
#[must_use]
pub const fn spell_target_type_of_component(component: u32) -> u32 {
    match component {
        0x31..=0x38 | 0x3C..=0x3E | 0xBE => item_type::CREATURE,
        0x39 => item_type::ITEM_ENCHANTABLE_TARGET,
        0x3B => item_type::PORTAL_MAGIC_TARGET,
        _ => item_type::UNDEF,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered world-object item-type table. The composite masks are the point.
    #[test]
    fn item_type_composites_are_exact() {
        assert_eq!(item_type::ITEM, 0x002D_FBEF);
        assert_eq!(item_type::VENDOR_SHOPKEEP, 0x4804_67A7);
        assert_eq!(item_type::VESTEMENTS, 0x6);
        assert_eq!(item_type::WEAPON, 0x101);
        assert_eq!(item_type::LOCKABLE_MAGIC_TARGET, 0x280);
        assert_eq!(item_type::VENDOR_GROCER, 0x0044_6220);
        assert_eq!(item_type::PORTAL_MAGIC_TARGET, 0x1001_0000);
    }

    #[test]
    fn a_target_component_names_creatures_enchantable_items_or_portals() {
        for c in (0x31..=0x38).chain(0x3C..=0x3E).chain([0xBE]) {
            assert_eq!(spell_target_type_of_component(c), 0x10, "component {c:#x}");
        }
        assert_eq!(spell_target_type_of_component(0x39), 0x0008_8B8F);
        assert_eq!(spell_target_type_of_component(0x3B), 0x1001_0000);
        for c in [0, 0x30, 0x3A, 0x3F, 0xBD, 0xBF, 0xFFFF_FFFF] {
            assert_eq!(spell_target_type_of_component(c), 0, "component {c:#x}");
        }
    }
}
