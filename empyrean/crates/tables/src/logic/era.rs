// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/ArmorTypeChance.cs, Source/ACE.Server/Factories/Tables/Wcids/ArmorWcids.cs, Source/ACE.Server/Factories/Tables/Wcids/ClothingWcids.cs, Source/ACE.Server/Factories/Tables/Wcids/ConsumeWcids.cs, Source/ACE.Server/Factories/Tables/Wcids/JewelryWcids.cs, Source/ACE.Server/Factories/Tables/WeaponTypeChance.cs, Source/ACE.Server/Factories/Tables/HeritageChance.cs, Source/ACE.Server/Factories/Tables/Wcids/WeaponWcids.cs, Source/ACE.Server/Factories/Tables/Wcids/Weapons/*.cs, Source/ACE.Server/Factories/Enum/TreasureWeaponType.cs

//! Not ACE: the rolls of the earlier eras' loot tables ([`crate::era`]), as ClassicACE makes them
//! under its Infiltration ruleset.

/// February 2005 (Infiltration).
pub mod infiltration {
    use crate::entity::ChanceTable;
    use crate::enums::{
        TreasureArmorType, TreasureHeritageGroup, TreasureWeaponType, WeenieClassName,
    };
    use crate::era::infiltration as t;
    use crate::logic::at;
    use crate::logic::tables::heritage_chance;
    use crate::rng;

    /// A weapon rolled from the era's tables: its weenie, the weapon type whose mutation applies
    /// (melee, missile or caster, as ACE's are), and the name the era's mutation scripts give its
    /// kind (`sword_ms`, `bow_short`, `atlatl_regular`, ...).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct EraWeapon {
        pub wcid: WeenieClassName,
        pub weapon_type: TreasureWeaponType,
        pub script: &'static str,
    }

    /// The weapon type of a random weapon: melee, missile or caster by the era's chances (no
    /// two-handed weapons); `MeleeWeapon` or `MissileWeapon` narrows the roll to that kind.
    // ClassicACE: WeaponTypeChance.Roll
    #[must_use]
    pub fn roll_weapon_type(filter: TreasureWeaponType) -> TreasureWeaponType {
        match filter {
            TreasureWeaponType::MeleeWeapon => t::weapon_type_chance::MELEE_CHANCES.roll(0.0),
            TreasureWeaponType::MissileWeapon => t::weapon_type_chance::MISSILE_CHANCES.roll(0.0),
            _ => t::weapon_type_chance::RETAIL_CHANCES.roll(0.0),
        }
    }

    /// The heritage a weapon is made for: the profile's chances for the twenty profiles of the
    /// era (the later societies' are not), else one of the three heritages evenly.
    // ClassicACE: HeritageChance.Roll
    #[must_use]
    pub fn roll_heritage(heritage_profile: i32) -> TreasureHeritageGroup {
        if !(1..=20).contains(&heritage_profile) {
            return TreasureHeritageGroup(rng::next_int(1, 3));
        }
        heritage_chance::roll(heritage_profile, false)
    }

    /// The mutation-script name of a weapon type (ClassicACE's `GetScriptName` outside its custom
    /// ruleset).
    // ClassicACE: TreasureWeaponTypeHelper.GetScriptName
    #[must_use]
    pub fn script_name(weapon_type: TreasureWeaponType) -> &'static str {
        match weapon_type {
            TreasureWeaponType::Axe => "axe",
            TreasureWeaponType::Dagger => "dagger",
            TreasureWeaponType::DaggerMS => "dagger_ms",
            TreasureWeaponType::Mace => "mace",
            TreasureWeaponType::MaceJitte => "mace_jitte",
            TreasureWeaponType::Spear | TreasureWeaponType::TwoHandedSpear => "spear",
            TreasureWeaponType::Staff => "staff",
            TreasureWeaponType::Sword => "sword",
            TreasureWeaponType::SwordMS => "sword_ms",
            TreasureWeaponType::Unarmed => "unarmed",
            TreasureWeaponType::Bow => "bow",
            TreasureWeaponType::Crossbow => "crossbow",
            TreasureWeaponType::Atlatl => "atlatl",
            TreasureWeaponType::Caster => "caster",
            _ => "cleaver",
        }
    }

    /// The offense/defense script name of a melee weapon type (`GetScriptShortName`).
    // ClassicACE: TreasureWeaponTypeHelper.GetScriptShortName
    #[must_use]
    pub fn script_short_name(script: &str) -> &str {
        match script {
            "dagger_ms" => "dagger",
            "sword_ms" => "sword",
            other => other,
        }
    }

    fn weapon(
        wcid: WeenieClassName,
        weapon_type: TreasureWeaponType,
        script: &'static str,
    ) -> EraWeapon {
        EraWeapon {
            wcid,
            weapon_type,
            script,
        }
    }

    /// Tier 1 has its own table; every later tier the heritage's main one.
    fn by_first_tier(
        tier: i32,
        first: &ChanceTable<WeenieClassName>,
        rest: &ChanceTable<WeenieClassName>,
    ) -> WeenieClassName {
        if tier > 1 {
            rest.roll(0.0)
        } else {
            first.roll(0.0)
        }
    }

    /// A weapon of `weapon_type` (as [`roll_weapon_type`] rolled it) for a treasure of `tier`
    /// whose heritage chances are `heritage_profile`; `None` for a type the era has no table for.
    // ClassicACE: WeaponWcids.Roll
    #[must_use]
    pub fn roll_weapon(
        weapon_type: TreasureWeaponType,
        tier: i32,
        heritage_profile: i32,
    ) -> Option<EraWeapon> {
        use TreasureHeritageGroup as H;
        use TreasureWeaponType as W;
        let melee = |wcid: WeenieClassName, wt: W| weapon(wcid, wt, script_name(wt));
        Some(match weapon_type {
            W::Sword => {
                let tables = match roll_heritage(heritage_profile) {
                    H::Aluvian => &t::sword_wcids_aluvian::WEAPON_TIERS,
                    H::Gharundim => &t::sword_wcids_gharundim::WEAPON_TIERS,
                    H::Sho => &t::sword_wcids_sho::WEAPON_TIERS,
                    _ => return None,
                };
                let wcid = at(tables, tier.clamp(1, 6) - 1).roll(0.0);
                melee(wcid, sword_type(wcid))
            }
            W::Mace => {
                let wcid = match roll_heritage(heritage_profile) {
                    H::Aluvian => by_first_tier(
                        tier,
                        &t::mace_wcids::MACE_WCIDS_ALUVIAN_T1,
                        &t::mace_wcids::MACE_WCIDS_ALUVIAN,
                    ),
                    H::Gharundim => by_first_tier(
                        tier,
                        &t::mace_wcids::MACE_WCIDS_GHARUNDIM_T1,
                        &t::mace_wcids::MACE_WCIDS_GHARUNDIM,
                    ),
                    H::Sho => by_first_tier(
                        tier,
                        &t::mace_wcids::MACE_WCIDS_SHO_T1,
                        &t::mace_wcids::MACE_WCIDS_SHO,
                    ),
                    _ => return None,
                };
                melee(wcid, W::Mace)
            }
            W::Axe => {
                let wcid = match roll_heritage(heritage_profile) {
                    H::Aluvian => by_first_tier(
                        tier,
                        &t::axe_wcids::AXE_WCIDS_ALUVIAN_T1,
                        &t::axe_wcids::AXE_WCIDS_ALUVIAN,
                    ),
                    H::Gharundim => by_first_tier(
                        tier,
                        &t::axe_wcids::AXE_WCIDS_GHARUNDIM_T1,
                        &t::axe_wcids::AXE_WCIDS_GHARUNDIM,
                    ),
                    H::Sho => by_first_tier(
                        tier,
                        &t::axe_wcids::AXE_WCIDS_SHO_T1,
                        &t::axe_wcids::AXE_WCIDS_SHO,
                    ),
                    _ => return None,
                };
                melee(wcid, W::Axe)
            }
            W::Spear => {
                let wcid = match roll_heritage(heritage_profile) {
                    H::Aluvian => by_first_tier(
                        tier,
                        &t::spear_wcids::SPEAR_WCIDS_ALUVIAN_T1,
                        &t::spear_wcids::SPEAR_WCIDS_ALUVIAN,
                    ),
                    H::Gharundim => by_first_tier(
                        tier,
                        &t::spear_wcids::SPEAR_WCIDS_GHARUNDIM_T1,
                        &t::spear_wcids::SPEAR_WCIDS_GHARUNDIM,
                    ),
                    H::Sho => by_first_tier(
                        tier,
                        &t::spear_wcids::SPEAR_WCIDS_SHO_T1,
                        &t::spear_wcids::SPEAR_WCIDS_SHO,
                    ),
                    _ => return None,
                };
                melee(wcid, W::Spear)
            }
            W::Unarmed => {
                let wcid = match roll_heritage(heritage_profile) {
                    H::Aluvian => by_first_tier(
                        tier,
                        &t::unarmed_wcids::UNARMED_WCIDS_ALUVIAN_T1,
                        &t::unarmed_wcids::UNARMED_WCIDS_ALUVIAN,
                    ),
                    H::Gharundim => by_first_tier(
                        tier,
                        &t::unarmed_wcids::UNARMED_WCIDS_GHARUNDIM_T1,
                        &t::unarmed_wcids::UNARMED_WCIDS_GHARUNDIM,
                    ),
                    H::Sho => by_first_tier(
                        tier,
                        &t::unarmed_wcids::UNARMED_WCIDS_SHO_T1,
                        &t::unarmed_wcids::UNARMED_WCIDS_SHO,
                    ),
                    _ => return None,
                };
                melee(wcid, W::Unarmed)
            }
            W::Staff => {
                let wcid = match roll_heritage(heritage_profile) {
                    H::Aluvian => by_first_tier(
                        tier,
                        &t::staff_wcids::STAFF_WCIDS_ALUVIAN_T1,
                        &t::staff_wcids::STAFF_WCIDS_ALUVIAN,
                    ),
                    H::Gharundim => by_first_tier(
                        tier,
                        &t::staff_wcids::STAFF_WCIDS_GHARUNDIM_T1,
                        &t::staff_wcids::STAFF_WCIDS_GHARUNDIM,
                    ),
                    H::Sho => by_first_tier(
                        tier,
                        &t::staff_wcids::STAFF_WCIDS_SHO_T1,
                        &t::staff_wcids::STAFF_WCIDS_SHO,
                    ),
                    _ => return None,
                };
                melee(wcid, W::Staff)
            }
            W::Dagger => {
                let tables = match roll_heritage(heritage_profile) {
                    H::Aluvian | H::Sho => &t::dagger_wcids_aluvian_sho::WEAPON_TIERS,
                    H::Gharundim => &t::dagger_wcids_gharundim::WEAPON_TIERS,
                    _ => return None,
                };
                let wcid = at(tables, tier.clamp(1, 6) - 1).roll(0.0);
                melee(wcid, dagger_type(wcid))
            }
            W::Bow => {
                let tables = match roll_heritage(heritage_profile) {
                    H::Aluvian => &t::bow_wcids_aluvian::BOW_TIERS,
                    H::Gharundim => &t::bow_wcids_gharundim::BOW_TIERS,
                    H::Sho => &t::bow_wcids_sho::BOW_TIERS,
                    _ => return None,
                };
                let wcid = at(tables, tier - 1).roll(0.0);
                // the short bow has its own mutations
                let script = if wcid == WeenieClassName::bowshort {
                    "bow_short"
                } else {
                    "bow"
                };
                weapon(wcid, W::Bow, script)
            }
            W::Crossbow => {
                let wcid = at(&t::crossbow_wcids::CROSSBOW_TIERS, tier - 1).roll(0.0);
                let script = if wcid == WeenieClassName::crossbowlight {
                    "crossbow_light"
                } else {
                    "crossbow"
                };
                weapon(wcid, W::Crossbow, script)
            }
            W::Atlatl => {
                let wcid = at(&t::atlatl_wcids::ATLATL_TIERS, tier - 1).roll(0.0);
                let script = if wcid == WeenieClassName::atlatl {
                    "atlatl_regular"
                } else {
                    "atlatl"
                };
                weapon(wcid, W::Atlatl, script)
            }
            W::Caster => {
                let tables = &t::caster_wcids::CASTER_TIERS;
                // ClassicACE rolls the tier's table twice and keeps the second.
                let _ = at(tables, tier - 1).roll(0.0);
                weapon(at(tables, tier - 1).roll(0.0), W::Caster, "caster")
            }
            _ => return None,
        })
    }

    /// Short swords, simis and yaojis strike twice (their own mutations); every other sword is a
    /// sword.
    // ClassicACE: SwordWcids_Aluvian.Roll, SwordWcids_Gharundim.Roll, SwordWcids_Sho.Roll
    fn sword_type(wcid: WeenieClassName) -> TreasureWeaponType {
        match wcid {
            WeenieClassName::swordshort
            | WeenieClassName::swordshortacid
            | WeenieClassName::swordshortelectric
            | WeenieClassName::swordshortfire
            | WeenieClassName::swordshortfrost
            | WeenieClassName::simi
            | WeenieClassName::simiacid
            | WeenieClassName::simielectric
            | WeenieClassName::simifire
            | WeenieClassName::simifrost
            | WeenieClassName::yaoji
            | WeenieClassName::yaojiacid
            | WeenieClassName::yaojielectric
            | WeenieClassName::yaojifire
            | WeenieClassName::yaojifrost => TreasureWeaponType::SwordMS,
            _ => TreasureWeaponType::Sword,
        }
    }

    /// Dirks are daggers; knives and daggers strike twice.
    // ClassicACE: DaggerWcids_Aluvian_Sho.Roll
    fn dagger_type(wcid: WeenieClassName) -> TreasureWeaponType {
        match wcid {
            WeenieClassName::dirk
            | WeenieClassName::dirkacid
            | WeenieClassName::dirkelectric
            | WeenieClassName::dirkfire
            | WeenieClassName::dirkfrost => TreasureWeaponType::Dagger,
            _ => TreasureWeaponType::DaggerMS,
        }
    }

    /// The armour type of a random armour piece: the era's chances for the tier (leather, studded
    /// leather, chainmail, platemail, the heritage armours and covenant).
    // ClassicACE: ArmorTypeChance.Roll
    #[must_use]
    pub fn roll_armor_type(tier: i32) -> TreasureArmorType {
        at(&t::armor_type_chance::ARMOR_TIERS, tier - 1).roll(0.0)
    }

    /// An armour piece of `armor_type` and the type it turned out to be (platemail and the
    /// heritage armours by heritage); `None` for a type the era has no table for.
    // ClassicACE: ArmorWcids.Roll
    #[must_use]
    pub fn roll_armor(
        armor_type: TreasureArmorType,
        heritage_profile: i32,
    ) -> Option<(WeenieClassName, TreasureArmorType)> {
        use t::armor_wcids as a;
        use TreasureArmorType as A;
        use TreasureHeritageGroup as H;
        let by_heritage = |aluvian: (&ChanceTable<WeenieClassName>, A),
                           gharundim: (&ChanceTable<WeenieClassName>, A),
                           sho: (&ChanceTable<WeenieClassName>, A)| {
            let (table, kind) = match roll_heritage(heritage_profile) {
                H::Aluvian => aluvian,
                H::Gharundim => gharundim,
                H::Sho => sho,
                _ => return None,
            };
            Some((table.roll(0.0), kind))
        };
        match armor_type {
            A::Leather => Some((a::LEATHER_WCIDS.roll(0.0), armor_type)),
            A::StuddedLeather => Some((a::STUDDED_LEATHER_WCIDS.roll(0.0), armor_type)),
            A::Chainmail => Some((a::CHAINMAIL_WCIDS.roll(0.0), armor_type)),
            A::Covenant => Some((a::COVENANT_WCIDS.roll(0.0), armor_type)),
            A::Platemail => by_heritage(
                (&a::PLATEMAIL_WCIDS, A::Platemail),
                (&a::SCALEMAIL_WCIDS, A::Scalemail),
                (&a::YOROI_WCIDS, A::Yoroi),
            ),
            A::HeritageLow => by_heritage(
                (&a::CELDON_WCIDS, A::Celdon),
                (&a::AMULI_WCIDS, A::Amuli),
                (&a::KOUJIA_WCIDS, A::Koujia),
            ),
            A::HeritageHigh => by_heritage(
                (&a::LORICA_WCIDS, A::Lorica),
                (&a::NARIYID_WCIDS, A::Nariyid),
                (&a::CHIRAN_WCIDS, A::Chiran),
            ),
            _ => None,
        }
    }

    /// A piece of clothing for the heritage the profile rolls (no Viamontian clothing).
    // ClassicACE: ClothingWcids.Roll
    #[must_use]
    pub fn roll_clothing(heritage_profile: i32) -> WeenieClassName {
        use t::clothing_wcids as c;
        match roll_heritage(heritage_profile) {
            TreasureHeritageGroup::Aluvian => c::CLOTHING_WCIDS_ALUVIAN.roll(0.0),
            TreasureHeritageGroup::Gharundim => c::CLOTHING_WCIDS_GHARUNDIM.roll(0.0),
            TreasureHeritageGroup::Sho => c::CLOTHING_WCIDS_SHO.roll(0.0),
            _ => WeenieClassName::undef,
        }
    }

    /// A piece of jewelry for the tier (tiers above 6 as 6).
    // ClassicACE: JewelryWcids.Roll
    #[must_use]
    pub fn roll_jewelry(tier: i32) -> WeenieClassName {
        at(&t::jewelry_wcids::TIER_CHANCES, tier.clamp(1, 6) - 1).roll(0.0)
    }

    /// Food or a potion for the tier, with the treasure's quality modifier.
    // ClassicACE: ConsumeWcids.Roll
    #[must_use]
    pub fn roll_consumable(tier: i32, quality_mod: f32) -> WeenieClassName {
        at(&t::consume_wcids::CONSUME_TIERS, tier - 1).roll(quality_mod)
    }
}
