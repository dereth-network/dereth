//! The eras a world can play, and the later systems each has.
//!
//! A server names its era; the client and the server read the same table, so a front end can
//! hide a panel the era has no state for (ratings, aetheria, luminance, ...) before any state
//! arrives. Every era speaks the end-of-retail client protocol: an era changes data and rules,
//! never the wire.

/// An era, in the order of retail history, so "at or before" compares as the ordinal does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EraId {
    /// February 2005: the Infiltration patch, before Throne of Destiny.
    Infiltration,
    /// The end of retail.
    #[default]
    Eor,
}

impl EraId {
    /// Every era, in order.
    pub const ALL: [Self; 2] = [Self::Infiltration, Self::Eor];

    /// The era's name as configuration files, command lines and status documents spell it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Infiltration => "infiltration",
            Self::Eor => "eor",
        }
    }

    /// The era named `name` (as [`Self::name`] spells it, any case, surrounding space ignored).
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|e| e.name().eq_ignore_ascii_case(name.trim()))
    }

    /// The later systems the era has.
    #[must_use]
    pub const fn features(self) -> EraFeatures {
        match self {
            Self::Infiltration => EraFeatures::NONE,
            Self::Eor => EraFeatures::ALL,
        }
    }

    /// How much experience wins back a point of vitae in the era.
    #[must_use]
    pub const fn vitae_recovery(self) -> VitaeRecovery {
        match self {
            Self::Infiltration => VitaeRecovery::BeforeThroneOfDestiny,
            Self::Eor => VitaeRecovery::EndOfRetail,
        }
    }
}

/// The experience needed to win back one point (1%) of vitae.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VitaeRecovery {
    /// `(level^2.5 * 2.5 + 20) * vitae^5 + 0.5`.
    EndOfRetail,
    /// `(level^2 * 5 + 20) * vitae^5 + 0.5`, at most 12,500: the formula from launch, and the cap
    /// Throne of Destiny's level-cap update announced (it says the formula had not changed since
    /// 1999).
    BeforeThroneOfDestiny,
}

impl core::fmt::Display for EraId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}

/// The later systems an era has or lacks. The end of retail has every one; February 2005 none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct EraFeatures {
    /// Damage-resistance, critical, healing, life, damage-over-time and nether ratings (and
    /// Recklessness and Sneak Attack). Without them every such rating is 0 and its modifier 1.
    pub ratings: bool,
    /// The 2012 consolidation of the weapon skills: a player's old weapon skill (Axe, Sword, Bow,
    /// ...) is read as Heavy, Light, Finesse or Missile Weapons. Without it the old skill is used.
    pub consolidated_weapon_skills: bool,
    /// The weapon item spells that became auras on the wielder. Without it they are item spells.
    pub item_spell_auras: bool,
    /// Assessing a player or an unattackable creature shows its armour levels and ratings.
    pub assessed_armor_and_ratings: bool,
    /// A character may swear allegiance to a patron of lower level.
    pub swear_to_lower_level: bool,
    /// The pre-order gifts handed out at login, and rare items dropped by creatures.
    pub pre_order_items_and_rares: bool,
    /// Wielding a second weapon in the off hand.
    pub dual_wield: bool,
    /// The heritage weapon masteries: a bonus with the heritage's weapon types.
    pub weapon_masteries: bool,
    /// The augmentation each heritage is born with (Jack of All Trades for the three original
    /// heritages, ...).
    pub innate_augmentations: bool,
    /// Aetheria and its sigil slots.
    pub aetheria: bool,
    /// Luminance and the luminance auras.
    pub luminance: bool,
}

impl EraFeatures {
    /// Every later system, as at the end of retail.
    pub const ALL: Self = Self {
        ratings: true,
        consolidated_weapon_skills: true,
        item_spell_auras: true,
        assessed_armor_and_ratings: true,
        swear_to_lower_level: true,
        pre_order_items_and_rares: true,
        dual_wield: true,
        weapon_masteries: true,
        innate_augmentations: true,
        aetheria: true,
        luminance: true,
    };

    /// None of them: February 2005.
    pub const NONE: Self = Self {
        ratings: false,
        consolidated_weapon_skills: false,
        item_spell_auras: false,
        assessed_armor_and_ratings: false,
        swear_to_lower_level: false,
        pre_order_items_and_rares: false,
        dual_wield: false,
        weapon_masteries: false,
        innate_augmentations: false,
        aetheria: false,
        luminance: false,
    };
}

impl Default for EraFeatures {
    fn default() -> Self {
        EraId::default().features()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_era_round_trips_its_name_and_the_end_of_retail_has_every_system() {
        for e in EraId::ALL {
            assert_eq!(EraId::parse(e.name()), Some(e));
        }
        assert_eq!(EraId::parse(" INFILTRATION "), Some(EraId::Infiltration));
        assert_eq!(EraId::parse("tod"), None);
        assert!(EraId::Infiltration < EraId::Eor);
        assert_eq!(EraId::Eor.features(), EraFeatures::ALL);
        assert_eq!(EraId::Infiltration.features(), EraFeatures::NONE);
        assert_eq!(EraFeatures::default(), EraFeatures::ALL);
    }
}
