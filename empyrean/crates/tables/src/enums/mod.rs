//! ACE.Server's `Source/ACE.Server/Factories/Enum` enums, one module per C# file, re-exported
//! flat, in the same representation as empyrean-entity's enums; `support.rs` (the `ace_enum!`
//! machinery) and `ext.rs` (the extension methods) are hand-written.
// @generated from ACE's `Source/ACE.Server/Factories/Enum` folder; do not edit by hand

pub mod ext;
mod support;

#[rustfmt::skip]
mod level8_spell_component_type;
#[rustfmt::skip]
mod loot_bias;
#[rustfmt::skip]
mod melee_weapon_skill;
#[rustfmt::skip]
mod society_armor_type;
#[rustfmt::skip]
mod society_type;
#[rustfmt::skip]
mod treasure_armor_type;
#[rustfmt::skip]
mod treasure_heritage_group;
#[rustfmt::skip]
mod treasure_item_category;
#[rustfmt::skip]
mod treasure_item_type;
#[rustfmt::skip]
mod treasure_table_type;
#[rustfmt::skip]
mod treasure_weapon_type;
#[rustfmt::skip]
mod weenie_class_name;

pub use level8_spell_component_type::Level8_SpellComponentType;
pub use loot_bias::LootBias;
pub use melee_weapon_skill::MeleeWeaponSkill;
pub use society_armor_type::SocietyArmorType;
pub use society_type::SocietyType;
pub use treasure_armor_type::TreasureArmorType;
pub use treasure_heritage_group::TreasureHeritageGroup;
pub use treasure_item_category::TreasureItemCategory;
pub use treasure_item_type::TreasureItemType;
pub use treasure_table_type::TreasureTableType;
pub use treasure_weapon_type::TreasureWeaponType;
pub use weenie_class_name::WeenieClassName;
