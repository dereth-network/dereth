// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject.cs
// @generated from ACE's `WorldObject` class tree in `Source/ACE.Server/WorldObjects`; do not edit by hand
//! Virtual-method dispatch for the ACE `WorldObject` class tree.
//!
//! Each `virtual`/`abstract` member `M` of a WorldObject class has one module here with one
//! function, `pub fn m(w, this, args..)`. It reads the object's most-derived class
//! ([`class_of`]) and calls the implementation of the nearest class up the chain that declares
//! or overrides `M`: `<class>_<m>` in the module of the ACE file that declares it (the class's
//! main module when that file is a `*_Properties.cs`). Getters take `&World`; so does a method
//! whose whole override chain is read-only in ACE. Everything else takes `&mut World`.
//!
//! **`base.X()` rule.** An override's body is ported by the unit that owns its file. Where ACE
//! calls `base.M(..)`, the port calls the parent class's implementation function explicitly
//! (for example `crate::world_objects::container::container_act_on_use(w, this, activator)`),
//! never the dispatch function, which would recurse into the override. `base.M` in ACE binds
//! to the nearest ancestor that implements `M`, which is exactly the function the stub calls.
//! Calls to `M` on `this` or on another object that ACE makes virtually go through
//! `crate::dispatch::m::m`.
//!
//! **Stubs.** Unported implementations are appended to their modules when this dispatch is
//! generated: an override calls `not_ported!` and then its parent's
//! implementation (ACE's base behaviour), so an object of any class still gets the base
//! behaviour. Replace a stub's body when porting it; keep its name and signature.
//!
//! A virtual call on a missing object panics: the caller must resolve the guid first, as ACE
//! would throw on a null reference.

use empyrean_entity::ObjectGuid;

use crate::world_objects::kinds::KindData;
use crate::world_objects::world_object::WorldObject;
use crate::World;

pub mod act_on_use;
pub mod broadcast_move_to;
pub mod calculate_obj_desc;
pub mod can_damage;
pub mod check_pk_status_vs_target;
pub mod check_use_requirements;
pub mod close;
pub mod default_chest_reset_interval;
pub mod die;
pub mod do_on_close_motion_changes;
pub mod do_on_open_motion_changes;
pub mod enqueue_action;
pub mod enter_world;
pub mod find_next_target;
pub mod finish_close;
pub mod get_accuracy_mod;
pub mod get_aim_height;
pub mod get_attack_message;
pub mod get_base_damage;
pub mod get_burden_mod;
pub mod get_combat_type;
pub mod get_current_attack_skill;
pub mod get_current_weapon_skill;
pub mod get_damage_type;
pub mod get_effective_attack_skill;
pub mod get_heritage_bonus;
pub mod get_natural_resistance;
pub mod get_power_mod;
pub mod get_power_range;
pub mod get_rotate_delay;
pub mod get_unique_objects;
pub mod handle_action_use_on_target;
pub mod handle_find_target;
pub mod handle_motion_done;
pub mod heartbeat;
pub mod init;
pub mod init_physics_obj;
pub mod is_attuned_or_contains_attuned;
pub mod is_being_traded_or_contains_item_being_traded;
pub mod is_sticky_attuned_or_contains_sticky_attuned;
pub mod is_unique_or_contains_unique;
pub mod motion_pickup;
pub mod move_to;
pub mod name;
pub mod on_activate;
pub mod on_add_item;
pub mod on_animate;
pub mod on_cast_spell;
pub mod on_collide_environment;
pub mod on_collide_object;
pub mod on_collide_object_end;
pub mod on_damage_target;
pub mod on_death;
pub mod on_emote;
pub mod on_evade;
pub mod on_generate;
pub mod on_generation;
pub mod on_initial_inventory_load_completed;
pub mod on_move_complete;
pub mod on_remove_item;
pub mod on_talk;
pub mod on_un_wield;
pub mod on_wield;
pub mod open;
pub mod reset;
pub mod reset_generator;
pub mod rotate;
pub mod save_biota_to_database;
pub mod send_partial_updates;
pub mod serialize_create_object;
pub mod serialize_game_data_only;
pub mod serialize_update_model_data;
pub mod serialize_update_object;
pub mod set_link_properties;
pub mod set_max_vitals;
pub mod sleep;
pub mod take_damage;
pub mod take_damage_over_time;
pub mod update_ammo_after_launch;
pub mod update_link_properties;
pub mod update_object_physics;
pub mod update_vital;
pub mod vital_heart_beat;

/// The most-derived ACE class of a world object: `KindData` without the data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Class {
    Admin,
    AdvocateFane,
    AdvocateItem,
    Allegiance,
    Ammunition,
    AttributeTransferDevice,
    AugmentationDevice,
    Bindstone,
    Book,
    Caster,
    Chest,
    Clothing,
    Coin,
    CombatPet,
    Container,
    Corpse,
    Cow,
    CraftTool,
    Creature,
    Door,
    Food,
    Game,
    GamePiece,
    Gem,
    GenericObject,
    Healer,
    Hook,
    Hooker,
    Hotspot,
    House,
    HousePortal,
    Key,
    Lifestone,
    LightSource,
    Lockpick,
    ManaStone,
    MeleeWeapon,
    Missile,
    MissileLauncher,
    PKModifier,
    Pet,
    PetDevice,
    Player,
    Portal,
    PressurePlate,
    Scroll,
    Sentinel,
    SkillAlterationDevice,
    SlumLord,
    SpellComponent,
    SpellProjectile,
    Stackable,
    Storage,
    Switch,
    Vendor,
    WorldObject,
}

impl Class {
    /// The object's most-derived class. A plain `KindData::WorldObject` that carries a component
    /// reads as the most derived component class (`Player`, then `Creature`, then `Container`).
    pub fn of(o: &WorldObject) -> Class {
        match &o.kind {
            KindData::WorldObject => {
                if o.player.is_some() {
                    Class::Player
                } else if o.creature.is_some() {
                    Class::Creature
                } else if o.container.is_some() {
                    Class::Container
                } else {
                    Class::WorldObject
                }
            }
            KindData::Admin(_) => Class::Admin,
            KindData::AdvocateFane(_) => Class::AdvocateFane,
            KindData::AdvocateItem(_) => Class::AdvocateItem,
            KindData::Allegiance(_) => Class::Allegiance,
            KindData::Ammunition(_) => Class::Ammunition,
            KindData::AttributeTransferDevice(_) => Class::AttributeTransferDevice,
            KindData::AugmentationDevice(_) => Class::AugmentationDevice,
            KindData::Bindstone(_) => Class::Bindstone,
            KindData::Book(_) => Class::Book,
            KindData::Caster(_) => Class::Caster,
            KindData::Chest(_) => Class::Chest,
            KindData::Clothing(_) => Class::Clothing,
            KindData::Coin(_) => Class::Coin,
            KindData::CombatPet(_) => Class::CombatPet,
            KindData::Container => Class::Container,
            KindData::Corpse(_) => Class::Corpse,
            KindData::Cow(_) => Class::Cow,
            KindData::CraftTool(_) => Class::CraftTool,
            KindData::Creature => Class::Creature,
            KindData::Door(_) => Class::Door,
            KindData::Food(_) => Class::Food,
            KindData::Game(_) => Class::Game,
            KindData::GamePiece(_) => Class::GamePiece,
            KindData::Gem(_) => Class::Gem,
            KindData::GenericObject(_) => Class::GenericObject,
            KindData::Healer(_) => Class::Healer,
            KindData::Hook(_) => Class::Hook,
            KindData::Hooker(_) => Class::Hooker,
            KindData::Hotspot(_) => Class::Hotspot,
            KindData::House(_) => Class::House,
            KindData::HousePortal(_) => Class::HousePortal,
            KindData::Key(_) => Class::Key,
            KindData::Lifestone(_) => Class::Lifestone,
            KindData::LightSource(_) => Class::LightSource,
            KindData::Lockpick(_) => Class::Lockpick,
            KindData::ManaStone(_) => Class::ManaStone,
            KindData::MeleeWeapon(_) => Class::MeleeWeapon,
            KindData::Missile(_) => Class::Missile,
            KindData::MissileLauncher(_) => Class::MissileLauncher,
            KindData::PKModifier(_) => Class::PKModifier,
            KindData::Pet(_) => Class::Pet,
            KindData::PetDevice(_) => Class::PetDevice,
            KindData::Player => Class::Player,
            KindData::Portal(_) => Class::Portal,
            KindData::PressurePlate(_) => Class::PressurePlate,
            KindData::Scroll(_) => Class::Scroll,
            KindData::Sentinel(_) => Class::Sentinel,
            KindData::SkillAlterationDevice(_) => Class::SkillAlterationDevice,
            KindData::SlumLord(_) => Class::SlumLord,
            KindData::SpellComponent(_) => Class::SpellComponent,
            KindData::SpellProjectile(_) => Class::SpellProjectile,
            KindData::Stackable(_) => Class::Stackable,
            KindData::Storage(_) => Class::Storage,
            KindData::Switch(_) => Class::Switch,
            KindData::Vendor(_) => Class::Vendor,
        }
    }

    /// The C# class name, as `GetType().Name` would print it.
    pub fn name(self) -> &'static str {
        match self {
            Class::Admin => "Admin",
            Class::AdvocateFane => "AdvocateFane",
            Class::AdvocateItem => "AdvocateItem",
            Class::Allegiance => "Allegiance",
            Class::Ammunition => "Ammunition",
            Class::AttributeTransferDevice => "AttributeTransferDevice",
            Class::AugmentationDevice => "AugmentationDevice",
            Class::Bindstone => "Bindstone",
            Class::Book => "Book",
            Class::Caster => "Caster",
            Class::Chest => "Chest",
            Class::Clothing => "Clothing",
            Class::Coin => "Coin",
            Class::CombatPet => "CombatPet",
            Class::Container => "Container",
            Class::Corpse => "Corpse",
            Class::Cow => "Cow",
            Class::CraftTool => "CraftTool",
            Class::Creature => "Creature",
            Class::Door => "Door",
            Class::Food => "Food",
            Class::Game => "Game",
            Class::GamePiece => "GamePiece",
            Class::Gem => "Gem",
            Class::GenericObject => "GenericObject",
            Class::Healer => "Healer",
            Class::Hook => "Hook",
            Class::Hooker => "Hooker",
            Class::Hotspot => "Hotspot",
            Class::House => "House",
            Class::HousePortal => "HousePortal",
            Class::Key => "Key",
            Class::Lifestone => "Lifestone",
            Class::LightSource => "LightSource",
            Class::Lockpick => "Lockpick",
            Class::ManaStone => "ManaStone",
            Class::MeleeWeapon => "MeleeWeapon",
            Class::Missile => "Missile",
            Class::MissileLauncher => "MissileLauncher",
            Class::PKModifier => "PKModifier",
            Class::Pet => "Pet",
            Class::PetDevice => "PetDevice",
            Class::Player => "Player",
            Class::Portal => "Portal",
            Class::PressurePlate => "PressurePlate",
            Class::Scroll => "Scroll",
            Class::Sentinel => "Sentinel",
            Class::SkillAlterationDevice => "SkillAlterationDevice",
            Class::SlumLord => "SlumLord",
            Class::SpellComponent => "SpellComponent",
            Class::SpellProjectile => "SpellProjectile",
            Class::Stackable => "Stackable",
            Class::Storage => "Storage",
            Class::Switch => "Switch",
            Class::Vendor => "Vendor",
            Class::WorldObject => "WorldObject",
        }
    }
}

/// The most-derived class of `this`. Panics if the object is not in the store.
pub fn class_of(w: &World, this: ObjectGuid) -> Class {
    match w.objects.get(this) {
        Some(o) => Class::of(o),
        None => panic!("virtual call on missing object {this:?}"),
    }
}

/// A member declared below `WorldObject` called on an object outside its declaring class's
/// subtree. C# cannot express this call without a failing cast (`InvalidCastException`).
#[cold]
pub fn wrong_class(member: &str, class: Class) -> ! {
    panic!(
        "{member} called on a {}, which does not derive from its declaring class",
        class.name()
    )
}
