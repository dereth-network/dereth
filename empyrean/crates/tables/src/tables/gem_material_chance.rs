// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/GemMaterialChance.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/GemMaterialChance.cs`; do not edit by hand

//! The literal data of ACE's `GemMaterialChance` (`Factories/Tables/GemMaterialChance.cs`).
//!
//! Runtime state, hand-ported in `crate::logic` (not literal here):
//! - `gemMaterialValue` (`Dictionary<MaterialType, int>`)
//! - `_combined` (`HashSet<WeenieClassName>`)

use crate::entity::{ChanceTable, GemResult};
use crate::enums::WeenieClassName;
use empyrean_entity::enums::MaterialType;

/// ACE `GemMaterialChance.class1_materialChance` (`ChanceTable<GemResult>`).
pub static CLASS1_MATERIAL_CHANCE: ChanceTable<GemResult> = ChanceTable::new(&[
    (GemResult::new(WeenieClassName::gemagate, MaterialType::Agate), 0.13),
    (GemResult::new(WeenieClassName::gemazurite, MaterialType::Azurite), 0.13),
    (GemResult::new(WeenieClassName::gemlapislazuli, MaterialType::LapisLazuli), 0.13),
    (GemResult::new(WeenieClassName::gemmalachite, MaterialType::Malachite), 0.13),
    (GemResult::new(WeenieClassName::gemsmokeyquartz, MaterialType::SmokeyQuartz), 0.12),
    (GemResult::new(WeenieClassName::gemtigereye, MaterialType::TigerEye), 0.12),
    (GemResult::new(WeenieClassName::gemturquoise, MaterialType::Turquoise), 0.12),
    (GemResult::new(WeenieClassName::gemwhitequartz, MaterialType::WhiteQuartz), 0.12),
]);

/// ACE `GemMaterialChance.class2_materialChance` (`ChanceTable<GemResult>`).
pub static CLASS2_MATERIAL_CHANCE: ChanceTable<GemResult> = ChanceTable::new(&[
    (GemResult::new(WeenieClassName::gemamber, MaterialType::Amber), 0.10),
    (GemResult::new(WeenieClassName::gembloodstone, MaterialType::Bloodstone), 0.10),
    (GemResult::new(WeenieClassName::gemcarnelian, MaterialType::Carnelian), 0.10),
    (GemResult::new(WeenieClassName::gemcitrine, MaterialType::Citrine), 0.10),
    (GemResult::new(WeenieClassName::gemhematite, MaterialType::Hematite), 0.10),
    (GemResult::new(WeenieClassName::gemmoonstone, MaterialType::Moonstone), 0.10),
    (GemResult::new(WeenieClassName::gemonyx, MaterialType::Onyx), 0.10),
    (GemResult::new(WeenieClassName::gemrosequartz, MaterialType::RoseQuartz), 0.10),
    (GemResult::new(WeenieClassName::gemlavenderjade, MaterialType::LavenderJade), 0.10),
    (GemResult::new(WeenieClassName::gemredjade, MaterialType::RedJade), 0.10),
]);

/// ACE `GemMaterialChance.class3_materialChance` (`ChanceTable<GemResult>`).
pub static CLASS3_MATERIAL_CHANCE: ChanceTable<GemResult> = ChanceTable::new(&[
    (GemResult::new(WeenieClassName::gemamethyst, MaterialType::Amethyst), 0.11),
    (GemResult::new(WeenieClassName::gemblackgarnet, MaterialType::BlackGarnet), 0.11),
    (GemResult::new(WeenieClassName::gemgreenjade, MaterialType::GreenJade), 0.11),
    (GemResult::new(WeenieClassName::gemjet, MaterialType::Jet), 0.11),
    (GemResult::new(WeenieClassName::gemredgarnet, MaterialType::RedGarnet), 0.11),
    (GemResult::new(WeenieClassName::gemtourmaline, MaterialType::Tourmaline), 0.11),
    (GemResult::new(WeenieClassName::gemwhitejade, MaterialType::WhiteJade), 0.11),
    (GemResult::new(WeenieClassName::gemyellowgarnet, MaterialType::YellowGarnet), 0.11),
    (GemResult::new(WeenieClassName::gemzircon, MaterialType::Zircon), 0.12),
]);

/// ACE `GemMaterialChance.class4_materialChance` (`ChanceTable<GemResult>`).
pub static CLASS4_MATERIAL_CHANCE: ChanceTable<GemResult> = ChanceTable::new(&[
    (GemResult::new(WeenieClassName::gemaquamarine, MaterialType::Aquamarine), 0.20),
    (GemResult::new(WeenieClassName::gemgreengarnet, MaterialType::GreenGarnet), 0.20),
    (GemResult::new(WeenieClassName::gemopal, MaterialType::Opal), 0.20),
    (GemResult::new(WeenieClassName::gemperidot, MaterialType::Peridot), 0.20),
    (GemResult::new(WeenieClassName::gemyellowtopaz, MaterialType::YellowTopaz), 0.20),
]);

/// ACE `GemMaterialChance.class5_materialChance` (`ChanceTable<GemResult>`).
pub static CLASS5_MATERIAL_CHANCE: ChanceTable<GemResult> = ChanceTable::new(&[
    (GemResult::new(WeenieClassName::gemblackopal, MaterialType::BlackOpal), 0.20),
    (GemResult::new(WeenieClassName::gemfireopal, MaterialType::FireOpal), 0.20),
    (GemResult::new(WeenieClassName::gemimperialtopaz, MaterialType::ImperialTopaz), 0.20),
    (GemResult::new(WeenieClassName::gemsunstone, MaterialType::Sunstone), 0.20),
    (GemResult::new(WeenieClassName::gemwhitesapphire, MaterialType::WhiteSapphire), 0.20),
]);

/// ACE `GemMaterialChance.class6_materialChance` (`ChanceTable<GemResult>`).
pub static CLASS6_MATERIAL_CHANCE: ChanceTable<GemResult> = ChanceTable::new(&[
    (GemResult::new(WeenieClassName::jeweldiamond, MaterialType::Diamond), 0.13),
    (GemResult::new(WeenieClassName::jewelemerald, MaterialType::Emerald), 0.29),
    (GemResult::new(WeenieClassName::jewelruby, MaterialType::Ruby), 0.29),
    (GemResult::new(WeenieClassName::jewelsapphire, MaterialType::Sapphire), 0.29),
]);

/// ACE `GemMaterialChance.gemMaterialChances` (`List<ChanceTable<GemResult>>`).
pub static GEM_MATERIAL_CHANCES: [&ChanceTable<GemResult>; 6] = [
    &CLASS1_MATERIAL_CHANCE,
    &CLASS2_MATERIAL_CHANCE,
    &CLASS3_MATERIAL_CHANCE,
    &CLASS4_MATERIAL_CHANCE,
    &CLASS5_MATERIAL_CHANCE,
    &CLASS6_MATERIAL_CHANCE,
];

/// ACE `GemMaterialChance.gemClassValue` (`List<int>`).
pub static GEM_CLASS_VALUE: [i32; 6] = [
    10,
    50,
    100,
    250,
    500,
    1000,
];
