// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject.cs
// @generated from ACE's `WorldObject` class tree in `Source/ACE.Server/WorldObjects`; do not edit by hand

//! The ACE `WorldObject` class tree (every class under `WorldObjects/`) as data: `KindData` (one variant per class), the
//! component aggregates, and C# `is` predicates.

#![allow(clippy::large_enum_variant)]

use crate::world_objects::world_object::WorldObject;

/// Non-property fields of every `WorldObject` partial file.
#[derive(Debug, Default)]
pub struct WorldObjectData {
    pub world_object: crate::world_objects::world_object::WorldObjectFields,
    pub world_object_combat: crate::world_objects::world_object_combat::WorldObjectCombatFields,
    pub world_object_database:
        crate::world_objects::world_object_database::WorldObjectDatabaseFields,
    pub world_object_decay: crate::world_objects::world_object_decay::WorldObjectDecayFields,
    pub world_object_equipment:
        crate::world_objects::world_object_equipment::WorldObjectEquipmentFields,
    pub world_object_generators:
        crate::world_objects::world_object_generators::WorldObjectGeneratorsFields,
    pub world_object_links: crate::world_objects::world_object_links::WorldObjectLinksFields,
    pub world_object_magic: crate::world_objects::world_object_magic::WorldObjectMagicFields,
    pub world_object_networking:
        crate::world_objects::world_object_networking::WorldObjectNetworkingFields,
    pub world_object_properties:
        crate::world_objects::world_object_properties::WorldObjectPropertiesFields,
    pub world_object_set: crate::world_objects::world_object_set::WorldObjectSetFields,
    pub world_object_tick: crate::world_objects::world_object_tick::WorldObjectTickFields,
    pub world_object_use: crate::world_objects::world_object_use::WorldObjectUseFields,
    pub world_object_weapon: crate::world_objects::world_object_weapon::WorldObjectWeaponFields,
}

/// Non-property fields of every `Container` partial file.
#[derive(Debug, Default)]
pub struct ContainerData {
    pub container: crate::world_objects::container::ContainerFields,
    pub container_properties: crate::world_objects::container_properties::ContainerPropertiesFields,
    pub container_tick: crate::world_objects::container_tick::ContainerTickFields,
}

/// Non-property fields of every `Creature` partial file.
#[derive(Debug, Default)]
pub struct CreatureData {
    pub creature: crate::world_objects::creature::CreatureFields,
    pub creature_attributes: crate::world_objects::creature_attributes::CreatureAttributesFields,
    pub creature_combat: crate::world_objects::creature_combat::CreatureCombatFields,
    pub creature_death: crate::world_objects::creature_death::CreatureDeathFields,
    pub creature_equipment: crate::world_objects::creature_equipment::CreatureEquipmentFields,
    pub creature_magic: crate::world_objects::creature_magic::CreatureMagicFields,
    pub creature_melee: crate::world_objects::creature_melee::CreatureMeleeFields,
    pub creature_missile: crate::world_objects::creature_missile::CreatureMissileFields,
    pub creature_navigation: crate::world_objects::creature_navigation::CreatureNavigationFields,
    pub creature_networking: crate::world_objects::creature_networking::CreatureNetworkingFields,
    pub creature_properties: crate::world_objects::creature_properties::CreaturePropertiesFields,
    pub creature_rating: crate::world_objects::creature_rating::CreatureRatingFields,
    pub creature_skills: crate::world_objects::creature_skills::CreatureSkillsFields,
    pub creature_tick: crate::world_objects::creature_tick::CreatureTickFields,
    pub creature_vitals: crate::world_objects::creature_vitals::CreatureVitalsFields,
    pub monster: crate::world_objects::monster::MonsterFields,
    pub monster_awareness: crate::world_objects::monster_awareness::MonsterAwarenessFields,
    pub monster_combat: crate::world_objects::monster_combat::MonsterCombatFields,
    pub monster_inventory: crate::world_objects::monster_inventory::MonsterInventoryFields,
    pub monster_magic: crate::world_objects::monster_magic::MonsterMagicFields,
    pub monster_melee: crate::world_objects::monster_melee::MonsterMeleeFields,
    pub monster_missile: crate::world_objects::monster_missile::MonsterMissileFields,
    pub monster_navigation: crate::world_objects::monster_navigation::MonsterNavigationFields,
    pub monster_properties: crate::world_objects::monster_properties::MonsterPropertiesFields,
    pub monster_tick: crate::world_objects::monster_tick::MonsterTickFields,
}

/// Non-property fields of every `Player` partial file.
#[derive(Debug, Default)]
pub struct PlayerData {
    pub player: crate::world_objects::player::PlayerFields,
    pub player_allegiance: crate::world_objects::player_allegiance::PlayerAllegianceFields,
    pub player_allowed_spell_id:
        crate::world_objects::player_allowed_spell_id::PlayerAllowedSpellIDFields,
    pub player_attributes: crate::world_objects::player_attributes::PlayerAttributesFields,
    pub player_book: crate::world_objects::player_book::PlayerBookFields,
    pub player_character: crate::world_objects::player_character::PlayerCharacterFields,
    pub player_chess: crate::world_objects::player_chess::PlayerChessFields,
    pub player_combat: crate::world_objects::player_combat::PlayerCombatFields,
    pub player_commerce: crate::world_objects::player_commerce::PlayerCommerceFields,
    pub player_contracts: crate::world_objects::player_contracts::PlayerContractsFields,
    pub player_crafting: crate::world_objects::player_crafting::PlayerCraftingFields,
    pub player_database: crate::world_objects::player_database::PlayerDatabaseFields,
    pub player_death: crate::world_objects::player_death::PlayerDeathFields,
    pub player_fellowship: crate::world_objects::player_fellowship::PlayerFellowshipFields,
    pub player_house: crate::world_objects::player_house::PlayerHouseFields,
    pub player_inventory: crate::world_objects::player_inventory::PlayerInventoryFields,
    pub player_location: crate::world_objects::player_location::PlayerLocationFields,
    pub player_luminance: crate::world_objects::player_luminance::PlayerLuminanceFields,
    pub player_magic: crate::world_objects::player_magic::PlayerMagicFields,
    pub player_melee: crate::world_objects::player_melee::PlayerMeleeFields,
    pub player_missile: crate::world_objects::player_missile::PlayerMissileFields,
    pub player_monster: crate::world_objects::player_monster::PlayerMonsterFields,
    pub player_move: crate::world_objects::player_move::PlayerMoveFields,
    pub player_move2: crate::world_objects::player_move2::PlayerMove2Fields,
    pub player_networking: crate::world_objects::player_networking::PlayerNetworkingFields,
    pub player_properties: crate::world_objects::player_properties::PlayerPropertiesFields,
    pub player_skills: crate::world_objects::player_skills::PlayerSkillsFields,
    pub player_spells: crate::world_objects::player_spells::PlayerSpellsFields,
    pub player_tick: crate::world_objects::player_tick::PlayerTickFields,
    pub player_tracking: crate::world_objects::player_tracking::PlayerTrackingFields,
    pub player_trade: crate::world_objects::player_trade::PlayerTradeFields,
    pub player_use: crate::world_objects::player_use::PlayerUseFields,
    pub player_vitals: crate::world_objects::player_vitals::PlayerVitalsFields,
    pub player_xp: crate::world_objects::player_xp::PlayerXpFields,
}

/// Data for a `Admin`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct AdminData {
    pub sentinel: crate::world_objects::sentinel::SentinelFields,
    pub admin: crate::world_objects::admin::AdminFields,
}

/// Data for a `AdvocateFane`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct AdvocateFaneData {
    pub advocate_fane: crate::world_objects::advocate_fane::AdvocateFaneFields,
}

/// Data for a `AdvocateItem`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct AdvocateItemData {
    pub generic_object: crate::world_objects::generic_object::GenericObjectFields,
    pub advocate_item: crate::world_objects::advocate_item::AdvocateItemFields,
}

/// Data for a `Allegiance`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct AllegianceData {
    pub allegiance: crate::world_objects::allegiance::AllegianceFields,
}

/// Data for a `Ammunition`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct AmmunitionData {
    pub stackable: crate::world_objects::stackable::StackableFields,
    pub ammunition: crate::world_objects::ammunition::AmmunitionFields,
}

/// Data for a `AttributeTransferDevice`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct AttributeTransferDeviceData {
    pub attribute_transfer_device:
        crate::world_objects::attribute_transfer_device::AttributeTransferDeviceFields,
}

/// Data for a `AugmentationDevice`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct AugmentationDeviceData {
    pub augmentation_device: crate::world_objects::augmentation_device::AugmentationDeviceFields,
}

/// Data for a `Bindstone`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct BindstoneData {
    pub bindstone: crate::world_objects::bindstone::BindstoneFields,
}

/// Data for a `Book`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct BookData {
    pub book: crate::world_objects::book::BookFields,
}

/// Data for a `Caster`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct CasterData {
    pub caster: crate::world_objects::caster::CasterFields,
}

/// Data for a `Chest`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct ChestData {
    pub chest: crate::world_objects::chest::ChestFields,
}

/// Data for a `Clothing`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct ClothingData {
    pub clothing: crate::world_objects::clothing::ClothingFields,
}

/// Data for a `Coin`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct CoinData {
    pub stackable: crate::world_objects::stackable::StackableFields,
    pub coin: crate::world_objects::coin::CoinFields,
}

/// Data for a `CombatPet`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct CombatPetData {
    pub pet: crate::world_objects::pet::PetFields,
    pub combat_pet: crate::world_objects::combat_pet::CombatPetFields,
    pub pet_monster: crate::world_objects::pet_monster::PetMonsterFields,
}

/// Data for a `Corpse`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct CorpseData {
    pub corpse: crate::world_objects::corpse::CorpseFields,
}

/// Data for a `Cow`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct CowData {
    pub cow: crate::world_objects::cow::CowFields,
}

/// Data for a `CraftTool`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct CraftToolData {
    pub stackable: crate::world_objects::stackable::StackableFields,
    pub craft_tool: crate::world_objects::craft_tool::CraftToolFields,
}

/// Data for a `Door`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct DoorData {
    pub door: crate::world_objects::door::DoorFields,
}

/// Data for a `Food`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct FoodData {
    pub stackable: crate::world_objects::stackable::StackableFields,
    pub food: crate::world_objects::food::FoodFields,
}

/// Data for a `Game`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct GameData {
    pub game: crate::world_objects::game::GameFields,
}

/// Data for a `GamePiece`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct GamePieceData {
    pub game_piece: crate::world_objects::game_piece::GamePieceFields,
}

/// Data for a `Gem`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct GemData {
    pub stackable: crate::world_objects::stackable::StackableFields,
    pub gem: crate::world_objects::gem::GemFields,
}

/// Data for a `GenericObject`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct GenericObjectData {
    pub generic_object: crate::world_objects::generic_object::GenericObjectFields,
}

/// Data for a `Healer`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct HealerData {
    pub healer: crate::world_objects::healer::HealerFields,
}

/// Data for a `Hook`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct HookData {
    pub hook: crate::world_objects::hook::HookFields,
}

/// Data for a `Hooker`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct HookerData {
    pub hooker: crate::world_objects::hooker::HookerFields,
}

/// Data for a `Hotspot`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct HotspotData {
    pub hotspot: crate::world_objects::hotspot::HotspotFields,
}

/// Data for a `House`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct HouseData {
    pub house: crate::world_objects::house::HouseFields,
}

/// Data for a `HousePortal`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct HousePortalData {
    pub portal: crate::world_objects::portal::PortalFields,
    pub portal_properties: crate::world_objects::portal_properties::PortalPropertiesFields,
    pub house_portal: crate::world_objects::house_portal::HousePortalFields,
}

/// Data for a `Key`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct KeyData {
    pub key: crate::world_objects::key::KeyFields,
}

/// Data for a `Lifestone`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct LifestoneData {
    pub lifestone: crate::world_objects::lifestone::LifestoneFields,
}

/// Data for a `LightSource`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct LightSourceData {
    pub generic_object: crate::world_objects::generic_object::GenericObjectFields,
    pub light_source: crate::world_objects::light_source::LightSourceFields,
}

/// Data for a `Lockpick`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct LockpickData {
    pub lockpick: crate::world_objects::lockpick::LockpickFields,
}

/// Data for a `ManaStone`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct ManaStoneData {
    pub mana_stone: crate::world_objects::mana_stone::ManaStoneFields,
}

/// Data for a `MeleeWeapon`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct MeleeWeaponData {
    pub melee_weapon: crate::world_objects::melee_weapon::MeleeWeaponFields,
}

/// Data for a `Missile`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct MissileData {
    pub stackable: crate::world_objects::stackable::StackableFields,
    pub missile: crate::world_objects::missile::MissileFields,
}

/// Data for a `MissileLauncher`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct MissileLauncherData {
    pub missile_launcher: crate::world_objects::missile_launcher::MissileLauncherFields,
}

/// Data for a `PKModifier`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct PKModifierData {
    pub pk_modifier: crate::world_objects::pk_modifier::PKModifierFields,
}

/// Data for a `Pet`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct PetData {
    pub pet: crate::world_objects::pet::PetFields,
}

/// Data for a `PetDevice`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct PetDeviceData {
    pub pet_device: crate::world_objects::pet_device::PetDeviceFields,
}

/// Data for a `Portal`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct PortalData {
    pub portal: crate::world_objects::portal::PortalFields,
    pub portal_properties: crate::world_objects::portal_properties::PortalPropertiesFields,
}

/// Data for a `PressurePlate`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct PressurePlateData {
    pub pressure_plate: crate::world_objects::pressure_plate::PressurePlateFields,
}

/// Data for a `Scroll`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct ScrollData {
    pub scroll: crate::world_objects::scroll::ScrollFields,
}

/// Data for a `Sentinel`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct SentinelData {
    pub sentinel: crate::world_objects::sentinel::SentinelFields,
}

/// Data for a `SkillAlterationDevice`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct SkillAlterationDeviceData {
    pub skill_alteration_device:
        crate::world_objects::skill_alteration_device::SkillAlterationDeviceFields,
}

/// Data for a `SlumLord`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct SlumLordData {
    pub slum_lord: crate::world_objects::slum_lord::SlumLordFields,
}

/// Data for a `SpellComponent`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct SpellComponentData {
    pub stackable: crate::world_objects::stackable::StackableFields,
    pub spell_component: crate::world_objects::spell_component::SpellComponentFields,
}

/// Data for a `SpellProjectile`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct SpellProjectileData {
    pub spell_projectile: crate::world_objects::spell_projectile::SpellProjectileFields,
}

/// Data for a `Stackable`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct StackableData {
    pub stackable: crate::world_objects::stackable::StackableFields,
}

/// Data for a `Storage`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct StorageData {
    pub chest: crate::world_objects::chest::ChestFields,
    pub storage: crate::world_objects::storage::StorageFields,
}

/// Data for a `Switch`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct SwitchData {
    pub switch: crate::world_objects::switch::SwitchFields,
}

/// Data for a `Vendor`: the fields of each class between it and its component ancestor.
#[derive(Debug, Default)]
pub struct VendorData {
    pub vendor: crate::world_objects::vendor::VendorFields,
}

/// The most-derived ACE class of a world object, with that class chain's data.
#[derive(Debug, Default)]
pub enum KindData {
    Admin(Box<AdminData>),
    AdvocateFane(Box<AdvocateFaneData>),
    AdvocateItem(Box<AdvocateItemData>),
    Allegiance(Box<AllegianceData>),
    Ammunition(Box<AmmunitionData>),
    AttributeTransferDevice(Box<AttributeTransferDeviceData>),
    AugmentationDevice(Box<AugmentationDeviceData>),
    Bindstone(Box<BindstoneData>),
    Book(Box<BookData>),
    Caster(Box<CasterData>),
    Chest(Box<ChestData>),
    Clothing(Box<ClothingData>),
    Coin(Box<CoinData>),
    CombatPet(Box<CombatPetData>),
    Container,
    Corpse(Box<CorpseData>),
    Cow(Box<CowData>),
    CraftTool(Box<CraftToolData>),
    Creature,
    Door(Box<DoorData>),
    Food(Box<FoodData>),
    Game(Box<GameData>),
    GamePiece(Box<GamePieceData>),
    Gem(Box<GemData>),
    GenericObject(Box<GenericObjectData>),
    Healer(Box<HealerData>),
    Hook(Box<HookData>),
    Hooker(Box<HookerData>),
    Hotspot(Box<HotspotData>),
    House(Box<HouseData>),
    HousePortal(Box<HousePortalData>),
    Key(Box<KeyData>),
    Lifestone(Box<LifestoneData>),
    LightSource(Box<LightSourceData>),
    Lockpick(Box<LockpickData>),
    ManaStone(Box<ManaStoneData>),
    MeleeWeapon(Box<MeleeWeaponData>),
    Missile(Box<MissileData>),
    MissileLauncher(Box<MissileLauncherData>),
    PKModifier(Box<PKModifierData>),
    Pet(Box<PetData>),
    PetDevice(Box<PetDeviceData>),
    Player,
    Portal(Box<PortalData>),
    PressurePlate(Box<PressurePlateData>),
    Scroll(Box<ScrollData>),
    Sentinel(Box<SentinelData>),
    SkillAlterationDevice(Box<SkillAlterationDeviceData>),
    SlumLord(Box<SlumLordData>),
    SpellComponent(Box<SpellComponentData>),
    SpellProjectile(Box<SpellProjectileData>),
    Stackable(Box<StackableData>),
    Storage(Box<StorageData>),
    Switch(Box<SwitchData>),
    Vendor(Box<VendorData>),
    #[default]
    WorldObject,
}

impl KindData {
    /// The C# class name, as `GetType().Name` would print it.
    pub fn class_name(&self) -> &'static str {
        match self {
            KindData::Admin(_) => "Admin",
            KindData::AdvocateFane(_) => "AdvocateFane",
            KindData::AdvocateItem(_) => "AdvocateItem",
            KindData::Allegiance(_) => "Allegiance",
            KindData::Ammunition(_) => "Ammunition",
            KindData::AttributeTransferDevice(_) => "AttributeTransferDevice",
            KindData::AugmentationDevice(_) => "AugmentationDevice",
            KindData::Bindstone(_) => "Bindstone",
            KindData::Book(_) => "Book",
            KindData::Caster(_) => "Caster",
            KindData::Chest(_) => "Chest",
            KindData::Clothing(_) => "Clothing",
            KindData::Coin(_) => "Coin",
            KindData::CombatPet(_) => "CombatPet",
            KindData::Container => "Container",
            KindData::Corpse(_) => "Corpse",
            KindData::Cow(_) => "Cow",
            KindData::CraftTool(_) => "CraftTool",
            KindData::Creature => "Creature",
            KindData::Door(_) => "Door",
            KindData::Food(_) => "Food",
            KindData::Game(_) => "Game",
            KindData::GamePiece(_) => "GamePiece",
            KindData::Gem(_) => "Gem",
            KindData::GenericObject(_) => "GenericObject",
            KindData::Healer(_) => "Healer",
            KindData::Hook(_) => "Hook",
            KindData::Hooker(_) => "Hooker",
            KindData::Hotspot(_) => "Hotspot",
            KindData::House(_) => "House",
            KindData::HousePortal(_) => "HousePortal",
            KindData::Key(_) => "Key",
            KindData::Lifestone(_) => "Lifestone",
            KindData::LightSource(_) => "LightSource",
            KindData::Lockpick(_) => "Lockpick",
            KindData::ManaStone(_) => "ManaStone",
            KindData::MeleeWeapon(_) => "MeleeWeapon",
            KindData::Missile(_) => "Missile",
            KindData::MissileLauncher(_) => "MissileLauncher",
            KindData::PKModifier(_) => "PKModifier",
            KindData::Pet(_) => "Pet",
            KindData::PetDevice(_) => "PetDevice",
            KindData::Player => "Player",
            KindData::Portal(_) => "Portal",
            KindData::PressurePlate(_) => "PressurePlate",
            KindData::Scroll(_) => "Scroll",
            KindData::Sentinel(_) => "Sentinel",
            KindData::SkillAlterationDevice(_) => "SkillAlterationDevice",
            KindData::SlumLord(_) => "SlumLord",
            KindData::SpellComponent(_) => "SpellComponent",
            KindData::SpellProjectile(_) => "SpellProjectile",
            KindData::Stackable(_) => "Stackable",
            KindData::Storage(_) => "Storage",
            KindData::Switch(_) => "Switch",
            KindData::Vendor(_) => "Vendor",
            KindData::WorldObject => "WorldObject",
        }
    }
}

/// C# `is` checks: true for the class itself and every subclass.
impl WorldObject {
    pub fn is_admin(&self) -> bool {
        matches!(self.kind, KindData::Admin(_))
    }
    pub fn is_advocate_fane(&self) -> bool {
        matches!(self.kind, KindData::AdvocateFane(_))
    }
    pub fn is_advocate_item(&self) -> bool {
        matches!(self.kind, KindData::AdvocateItem(_))
    }
    pub fn is_allegiance(&self) -> bool {
        matches!(self.kind, KindData::Allegiance(_))
    }
    pub fn is_ammunition(&self) -> bool {
        matches!(self.kind, KindData::Ammunition(_))
    }
    pub fn is_attribute_transfer_device(&self) -> bool {
        matches!(self.kind, KindData::AttributeTransferDevice(_))
    }
    pub fn is_augmentation_device(&self) -> bool {
        matches!(self.kind, KindData::AugmentationDevice(_))
    }
    pub fn is_bindstone(&self) -> bool {
        matches!(self.kind, KindData::Bindstone(_))
    }
    pub fn is_book(&self) -> bool {
        matches!(self.kind, KindData::Book(_))
    }
    pub fn is_caster(&self) -> bool {
        matches!(self.kind, KindData::Caster(_))
    }
    pub fn is_chest(&self) -> bool {
        matches!(self.kind, KindData::Chest(_) | KindData::Storage(_))
    }
    pub fn is_clothing(&self) -> bool {
        matches!(self.kind, KindData::Clothing(_))
    }
    pub fn is_coin(&self) -> bool {
        matches!(self.kind, KindData::Coin(_))
    }
    pub fn is_combat_pet(&self) -> bool {
        matches!(self.kind, KindData::CombatPet(_))
    }
    pub fn is_container(&self) -> bool {
        self.container.is_some()
    }
    pub fn is_corpse(&self) -> bool {
        matches!(self.kind, KindData::Corpse(_))
    }
    pub fn is_cow(&self) -> bool {
        matches!(self.kind, KindData::Cow(_))
    }
    pub fn is_craft_tool(&self) -> bool {
        matches!(self.kind, KindData::CraftTool(_))
    }
    pub fn is_creature(&self) -> bool {
        self.creature.is_some()
    }
    pub fn is_door(&self) -> bool {
        matches!(self.kind, KindData::Door(_))
    }
    pub fn is_food(&self) -> bool {
        matches!(self.kind, KindData::Food(_))
    }
    pub fn is_game(&self) -> bool {
        matches!(self.kind, KindData::Game(_))
    }
    pub fn is_game_piece(&self) -> bool {
        matches!(self.kind, KindData::GamePiece(_))
    }
    pub fn is_gem(&self) -> bool {
        matches!(self.kind, KindData::Gem(_))
    }
    pub fn is_generic_object(&self) -> bool {
        matches!(
            self.kind,
            KindData::AdvocateItem(_) | KindData::GenericObject(_) | KindData::LightSource(_)
        )
    }
    pub fn is_healer(&self) -> bool {
        matches!(self.kind, KindData::Healer(_))
    }
    pub fn is_hook(&self) -> bool {
        matches!(self.kind, KindData::Hook(_))
    }
    pub fn is_hooker(&self) -> bool {
        matches!(self.kind, KindData::Hooker(_))
    }
    pub fn is_hotspot(&self) -> bool {
        matches!(self.kind, KindData::Hotspot(_))
    }
    pub fn is_house(&self) -> bool {
        matches!(self.kind, KindData::House(_))
    }
    pub fn is_house_portal(&self) -> bool {
        matches!(self.kind, KindData::HousePortal(_))
    }
    pub fn is_key(&self) -> bool {
        matches!(self.kind, KindData::Key(_))
    }
    pub fn is_lifestone(&self) -> bool {
        matches!(self.kind, KindData::Lifestone(_))
    }
    pub fn is_light_source(&self) -> bool {
        matches!(self.kind, KindData::LightSource(_))
    }
    pub fn is_lockpick(&self) -> bool {
        matches!(self.kind, KindData::Lockpick(_))
    }
    pub fn is_mana_stone(&self) -> bool {
        matches!(self.kind, KindData::ManaStone(_))
    }
    pub fn is_melee_weapon(&self) -> bool {
        matches!(self.kind, KindData::MeleeWeapon(_))
    }
    pub fn is_missile(&self) -> bool {
        matches!(self.kind, KindData::Missile(_))
    }
    pub fn is_missile_launcher(&self) -> bool {
        matches!(self.kind, KindData::MissileLauncher(_))
    }
    pub fn is_pk_modifier(&self) -> bool {
        matches!(self.kind, KindData::PKModifier(_))
    }
    pub fn is_pet(&self) -> bool {
        matches!(self.kind, KindData::CombatPet(_) | KindData::Pet(_))
    }
    pub fn is_pet_device(&self) -> bool {
        matches!(self.kind, KindData::PetDevice(_))
    }
    pub fn is_player(&self) -> bool {
        self.player.is_some()
    }
    pub fn is_portal(&self) -> bool {
        matches!(self.kind, KindData::HousePortal(_) | KindData::Portal(_))
    }
    pub fn is_pressure_plate(&self) -> bool {
        matches!(self.kind, KindData::PressurePlate(_))
    }
    pub fn is_scroll(&self) -> bool {
        matches!(self.kind, KindData::Scroll(_))
    }
    pub fn is_sentinel(&self) -> bool {
        matches!(self.kind, KindData::Admin(_) | KindData::Sentinel(_))
    }
    pub fn is_skill_alteration_device(&self) -> bool {
        matches!(self.kind, KindData::SkillAlterationDevice(_))
    }
    pub fn is_slum_lord(&self) -> bool {
        matches!(self.kind, KindData::SlumLord(_))
    }
    pub fn is_spell_component(&self) -> bool {
        matches!(self.kind, KindData::SpellComponent(_))
    }
    pub fn is_spell_projectile(&self) -> bool {
        matches!(self.kind, KindData::SpellProjectile(_))
    }
    pub fn is_stackable(&self) -> bool {
        matches!(
            self.kind,
            KindData::Ammunition(_)
                | KindData::Coin(_)
                | KindData::CraftTool(_)
                | KindData::Food(_)
                | KindData::Gem(_)
                | KindData::Missile(_)
                | KindData::SpellComponent(_)
                | KindData::Stackable(_)
        )
    }
    pub fn is_storage(&self) -> bool {
        matches!(self.kind, KindData::Storage(_))
    }
    pub fn is_switch(&self) -> bool {
        matches!(self.kind, KindData::Switch(_))
    }
    pub fn is_vendor(&self) -> bool {
        matches!(self.kind, KindData::Vendor(_))
    }
}
