// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootTables.cs
//! Port of `Source/ACE.Server/Factories/LootTables.cs`.

use std::sync::LazyLock;

use empyrean_common::dotnet::DotNetHashSet;
use empyrean_entity::enums::{MaterialType, SpellId};
use empyrean_tables::logic::cantrips::{
    armor_cantrips, jewelry_cantrips, melee_cantrips, missile_cantrips, wand_cantrips,
};

/// `DefaultMaterial`: five default materials per item class (armor, missile, melee, caster,
/// dinnerware, clothes). (ACE: `LootTables.DefaultMaterial`, an auto-property.)
#[allow(clippy::cast_possible_wrap)]
pub static DEFAULT_MATERIAL: [[i32; 5]; 6] = [
    [
        MaterialType::Copper.0 as i32,
        MaterialType::Bronze.0 as i32,
        MaterialType::Iron.0 as i32,
        MaterialType::Steel.0 as i32,
        MaterialType::Silver.0 as i32,
    ], // Armor
    [
        MaterialType::Oak.0 as i32,
        MaterialType::Teak.0 as i32,
        MaterialType::Mahogany.0 as i32,
        MaterialType::Pine.0 as i32,
        MaterialType::Ebony.0 as i32,
    ], // Missile
    [
        MaterialType::Brass.0 as i32,
        MaterialType::Ivory.0 as i32,
        MaterialType::Gold.0 as i32,
        MaterialType::Steel.0 as i32,
        MaterialType::Diamond.0 as i32,
    ], // Melee
    [
        MaterialType::RedGarnet.0 as i32,
        MaterialType::Jet.0 as i32,
        MaterialType::BlackOpal.0 as i32,
        MaterialType::FireOpal.0 as i32,
        MaterialType::Emerald.0 as i32,
    ], // Caster
    [
        MaterialType::Granite.0 as i32,
        MaterialType::Ceramic.0 as i32,
        MaterialType::Porcelain.0 as i32,
        MaterialType::Alabaster.0 as i32,
        MaterialType::Marble.0 as i32,
    ], // Dinnerware
    [
        MaterialType::Linen.0 as i32,
        MaterialType::Wool.0 as i32,
        MaterialType::Velvet.0 as i32,
        MaterialType::Satin.0 as i32,
        MaterialType::Silk.0 as i32,
    ], // Clothes
];

/// The four cantrip level sets, for logging epic/legendary drops: built by the static
/// constructor on first use.
#[derive(Debug)]
pub struct CantripSets {
    pub minor_cantrips: DotNetHashSet<i32>,
    pub major_cantrips: DotNetHashSet<i32>,
    pub epic_cantrips: DotNetHashSet<i32>,
    pub legendary_cantrips: DotNetHashSet<i32>,
}

/// (ACE: `LootTables.MinorCantrips` .. `LegendaryCantrips`, static fields.)
pub static CANTRIP_SETS: LazyLock<CantripSets> = LazyLock::new(loot_tables);

/// `cantripTables`: armor, jewelry, wand, melee and missile cantrips, in that order.
fn cantrip_tables() -> [&'static [Vec<SpellId>]; 5] {
    [
        &armor_cantrips::TABLE,
        &jewelry_cantrips::TABLE,
        &wand_cantrips::TABLE,
        &melee_cantrips::TABLE,
        &missile_cantrips::TABLE,
    ]
}

/// The static constructor.
// ACE: LootTables.LootTables
fn loot_tables() -> CantripSets {
    CantripSets {
        minor_cantrips: build_cantrips_table(0),
        major_cantrips: build_cantrips_table(1),
        epic_cantrips: build_cantrips_table(2),
        legendary_cantrips: build_cantrips_table(3),
    }
}

/// Every cantrip table's `tier` column (0 minor ... 3 legendary).
// ACE: LootTables.BuildCantripsTable
fn build_cantrips_table(tier: usize) -> DotNetHashSet<i32> {
    let mut table = DotNetHashSet::new();

    for cantrip_table in cantrip_tables() {
        for category in cantrip_table {
            table.insert(category[tier].0.cast_signed());
        }
    }
    table
}
