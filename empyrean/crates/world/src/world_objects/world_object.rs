// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject.cs
//! Port of `Source/ACE.Server/WorldObjects/WorldObject.cs`.
//!
//! # Construction
//!
//! ACE builds an object with `new <Class>(weenie, guid)` or `new <Class>(biota)`. The C#
//! constructor chain runs the base first: `WorldObject(..)` (the biota, `SetEphemeralValues`,
//! `InitializeGenerator`, `InitializeHeartbeats`, `CreationTimestamp`), then each derived
//! constructor body, most-derived last, each calling its own private `SetEphemeralValues`. Here
//! [`WorldObject::from_weenie`] / [`WorldObject::from_biota`] allocate the object with the
//! components and [`KindData`] variant of the class (C# field initializers run before any
//! constructor body, so the per-file field structs start at their `Default`), then call the
//! class's `<class>_ctor`, which calls its parent's `_ctor` first, exactly as `: base(..)` does.
//!
//! The object is not in `World.objects` while it is being built, so constructors take the object
//! itself (`&mut WorldObject`) and a read-only [`CtorEnv`] for the global state ACE's
//! constructors read (`Time`, `DatabaseManager.World`, `PropertyManager`, ...).

use std::sync::Arc;

use dereth_physics::PhysHandle;
use empyrean_common::dotnet::{CsCast, DotNetDict};
use empyrean_common::extensions::string_extensions::pluralize;
use empyrean_entity::enums::{
    AttunedStatus, CombatStyle, CombatUse, ContainerType, DestinationType, ItemType, MotionCommand,
    MotionStance, ObjectDescriptionFlag, Placement, PropertyInt, PropertyString, Quadrant, Skill,
    WeenieType,
};
use empyrean_entity::{
    convert_to_biota, Biota, LandblockId, ObjectGuid, Position, Vector3, Weenie,
};

use crate::dispatch::{self, Class};
use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::network::motion::movement_data::Motion;
use crate::world_objects::kinds::{
    ContainerData, CreatureData, KindData, PlayerData, WorldObjectData,
};
use crate::World;

/// Non-property fields declared in `WorldObject.cs` other than the core identity fields held
/// directly on [`WorldObject`].
///
/// Not here yet, because their types are not ported: `LastMoveToState`
/// (`MoveToState`), `ScatterPos` (`SetPosition`) and `Sequences` (wanted as a core field).
#[derive(Debug)]
pub struct WorldObjectFields {
    // ACE: WorldObject.ObjectDescriptionFlags
    pub object_description_flags: ObjectDescriptionFlag,
    /// `virtual` auto-property in ACE with no override; `Player.SetEphemeralValues` sets it.
    // ACE: WorldObject.ListeningRadius
    pub listening_radius: f32,
    // ACE: WorldObject.IsBusy
    pub is_busy: bool,
    /// FIXME: find a better way to do this for projectiles
    // ACE: WorldObject.HitMsg
    pub hit_msg: bool,
    // ACE: WorldObject.BumpVelocity
    pub bump_velocity: bool,
    /// This will be true when teleporting
    // ACE: WorldObject.Teleporting
    pub teleporting: bool,
    // ACE: WorldObject.RequestedLocation
    pub requested_location: Option<Position>,
    /// Flag indicates if RequestedLocation should be broadcast to other players: TRUE for AutoPos
    /// packets, FALSE for MoveToState packets.
    // ACE: WorldObject.RequestedLocationBroadcast
    pub requested_location_broadcast: bool,
    // ACE: WorldObject.DestinationType
    pub destination_type: DestinationType,
    // ACE: WorldObject.IsDestroyed
    pub is_destroyed: bool,
    /// `WorldObject.EnchantmentManager`: the caches of the object's
    /// `EnchantmentManagerWithCaching`.
    pub enchantment_manager: crate::world_objects::managers::enchantment_manager_with_caching::EnchantmentManagerWithCaching,
    /// `WorldObject.EmoteManager`: the object's `EmoteManager` state.
    // ACE: WorldObject.EmoteManager
    pub emote_manager: crate::world_objects::managers::emote_manager::EmoteManager,
    /// What ACE's destroyed `PhysicsObj` still answers after the landblock removed the object
    /// (`RemoveWorldObjectInternal` ran `PhysicsObj.DestroyObject`): the port drops the shared
    /// body there (the landblock's DIVERGE), so its position (cell 0, as `leave_world` leaves it),
    /// height and cached velocity are kept here until a new body is made.
    pub destroyed_physics_obj: Option<DestroyedPhysicsObj>,
    /// Set by a generator's scatter spawn for the one `EnterWorld` that follows it: the body is
    /// placed at a random walkable point around this position instead of at `Location`.
    // ACE: WorldObject.ScatterPos
    pub scatter_pos: Option<ScatterPos>,
}

/// A scatter placement request: up to `num_tries` random points within `rad_x` / `rad_y` of
/// `pos`, the first that places.
// ACE: Physics.Common.SetPosition
#[derive(Debug, Clone, Copy)]
pub struct ScatterPos {
    pub pos: dereth_primitives::Position,
    pub rad_x: f32,
    pub rad_y: f32,
    pub num_tries: i32,
}

impl ScatterPos {
    /// `SetPosition.Default_NumTries`.
    // ACE: Physics.Common.SetPosition.Default_NumTries
    pub const DEFAULT_NUM_TRIES: i32 = 20;

    /// `new SetPosition(pos, SetPositionFlags.RandomScatter, radius)`.
    #[must_use]
    pub const fn new(pos: dereth_primitives::Position, radius: f32) -> Self {
        Self {
            pos,
            rad_x: radius,
            rad_y: radius,
            num_tries: Self::DEFAULT_NUM_TRIES,
        }
    }
}

/// The state of a destroyed `PhysicsObj` that ACE's references can still read.
#[derive(Debug, Clone, Copy)]
pub struct DestroyedPhysicsObj {
    /// `PhysicsObj.Position`: the last frame, with `ObjCellID = 0` (`leave_world`).
    pub position: dereth_primitives::Position,
    /// `PhysicsObj.GetHeight()`: the part array's height times the scale.
    pub height: f32,
    /// `PhysicsObj.CachedVelocity`: the last update's (`DestroyObject` leaves it).
    pub cached_velocity: dereth_primitives::Vec3,
}

impl Default for WorldObjectFields {
    /// The C# field initializers (`ListeningRadius = 5f`); everything else starts at `default`.
    fn default() -> Self {
        WorldObjectFields {
            object_description_flags: ObjectDescriptionFlag::default(),
            listening_radius: 5.0,
            is_busy: false,
            hit_msg: false,
            bump_velocity: false,
            teleporting: false,
            requested_location: None,
            requested_location_broadcast: false,
            destination_type: DestinationType::default(),
            is_destroyed: false,
            enchantment_manager: Default::default(),
            emote_manager: Default::default(),
            destroyed_physics_obj: None,
            scatter_pos: None,
        }
    }
}

/// Links a projectile keeps to the objects involved, as guids.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProjectileLinks {
    // ACE: WorldObject.ProjectileSource
    pub source: Option<ObjectGuid>,
    // ACE: WorldObject.ProjectileTarget
    pub target: Option<ObjectGuid>,
    // ACE: WorldObject.ProjectileLauncher
    pub launcher: Option<ObjectGuid>,
    // ACE: WorldObject.ProjectileAmmo
    pub ammo: Option<ObjectGuid>,
}

/// ACE's `WorldObject`, flattened: the core identity and placement fields, the per-file field
/// structs of every partial, optional class components, and the leaf-class kind.
/// Per-file state goes in that file's `*Fields` struct; only core fields belong here.
#[derive(Debug, Default)]
pub struct WorldObject {
    /// A wrapper around `Biota.Id` in ACE.
    // ACE: WorldObject.Guid
    pub guid: ObjectGuid,
    /// The template this object was created from, if it came from a weenie rather than the shard.
    // ACE: WorldObject.Weenie
    pub weenie: Option<Arc<Weenie>>,
    /// All persisted properties. Mutate only through the ported `SetProperty`/`RemoveProperty`.
    // ACE: WorldObject.Biota
    pub biota: Biota,
    /// The object's body in `World.physics`, once `InitPhysicsObj` has run.
    // ACE: WorldObject.PhysicsObj
    pub phys: Option<PhysHandle>,
    // ACE: WorldObject.Sequences
    pub sequences: crate::network::sequence::sequence_manager::SequenceManager,
    /// Set only by the landblock code.
    // ACE: WorldObject.CurrentLandblock
    pub current_landblock: Option<LandblockId>,
    // ACE: WorldObject.Wielder
    pub wielder: Option<ObjectGuid>,
    /// `Some` for projectiles (missiles and spell projectiles).
    pub projectile: Option<ProjectileLinks>,
    /// Non-property fields from every `WorldObject_*.cs` partial.
    pub wo: WorldObjectData,
    /// `Some` for `Container` and every subclass.
    pub container: Option<Box<ContainerData>>,
    /// `Some` for `Creature` and every subclass.
    pub creature: Option<Box<CreatureData>>,
    /// `Some` for `Player` and every subclass.
    pub player: Option<Box<PlayerData>>,
    /// The most-derived class and its chain's data.
    pub kind: KindData,
}

// ================================================================================ construction

/// The global state ACE's constructors read. Built by the caller around a shared borrow of the
/// world; construction never mutates the world (the object joins `World.objects` afterwards).
pub struct CtorEnv<'a> {
    /// `Time.GetUnixTime()` / `DateTime.UtcNow` come from `w.now`; `PropertyManager`, the dats
    /// and the other managers from their `World` fields.
    pub w: &'a World,
    /// `DatabaseManager.World.GetCachedWeenie(wcid)`: the world wires it to `w.content`;
    /// tests pass a closure over synthetic weenies.
    pub get_cached_weenie: &'a dyn Fn(u32) -> Option<Arc<Weenie>>,
}

impl CtorEnv<'_> {
    /// Runs `f` with a construction environment whose weenie lookup is the world's content
    /// database (`DatabaseManager.World.GetCachedWeenie`).
    pub fn with_world<R>(w: &World, f: impl FnOnce(&CtorEnv<'_>) -> R) -> R {
        let lookup = |wcid: u32| w.content.get_cached_weenie(wcid);
        f(&CtorEnv {
            w,
            get_cached_weenie: &lookup,
        })
    }
}

impl std::fmt::Debug for CtorEnv<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CtorEnv")
            .field("now", &self.w.now)
            .finish_non_exhaustive()
    }
}

impl<'a> CtorEnv<'a> {
    /// An environment with no world content: every `GetCachedWeenie` finds nothing.
    pub fn without_content(w: &'a World) -> Self {
        CtorEnv {
            w,
            get_cached_weenie: &|_| None,
        }
    }
}

/// Which of the two ACE constructors runs: `(Weenie weenie, ObjectGuid guid)`, or `(Biota biota)`
/// to restore an object from the shard.
/// The biota is moved into the object, so it is carried by value rather than boxed.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum CtorSource {
    Weenie(Arc<Weenie>, ObjectGuid),
    Biota(Biota),
}

impl CtorSource {
    /// True for the `(Biota)` constructor.
    #[must_use]
    pub fn is_biota(&self) -> bool {
        matches!(self, CtorSource::Biota(_))
    }
}

impl WorldObject {
    /// A fresh object of `class` before any constructor body has run: the components its class
    /// chain carries, and its [`KindData`] variant, all at their field-initializer defaults.
    #[must_use]
    pub fn allocate(class: Class) -> WorldObject {
        let (depth, kind) = match class {
            Class::Admin => (3, KindData::Admin(Box::default())),
            Class::AdvocateFane => (0, KindData::AdvocateFane(Box::default())),
            Class::AdvocateItem => (0, KindData::AdvocateItem(Box::default())),
            Class::Allegiance => (0, KindData::Allegiance(Box::default())),
            Class::Ammunition => (0, KindData::Ammunition(Box::default())),
            Class::AttributeTransferDevice => {
                (0, KindData::AttributeTransferDevice(Box::default()))
            }
            Class::AugmentationDevice => (0, KindData::AugmentationDevice(Box::default())),
            Class::Bindstone => (0, KindData::Bindstone(Box::default())),
            Class::Book => (0, KindData::Book(Box::default())),
            Class::Caster => (0, KindData::Caster(Box::default())),
            Class::Chest => (1, KindData::Chest(Box::default())),
            Class::Clothing => (0, KindData::Clothing(Box::default())),
            Class::Coin => (0, KindData::Coin(Box::default())),
            Class::CombatPet => (2, KindData::CombatPet(Box::default())),
            Class::Container => (1, KindData::Container),
            Class::Corpse => (1, KindData::Corpse(Box::default())),
            Class::Cow => (2, KindData::Cow(Box::default())),
            Class::CraftTool => (0, KindData::CraftTool(Box::default())),
            Class::Creature => (2, KindData::Creature),
            Class::Door => (0, KindData::Door(Box::default())),
            Class::Food => (0, KindData::Food(Box::default())),
            Class::Game => (0, KindData::Game(Box::default())),
            Class::GamePiece => (2, KindData::GamePiece(Box::default())),
            Class::Gem => (0, KindData::Gem(Box::default())),
            Class::GenericObject => (0, KindData::GenericObject(Box::default())),
            Class::Healer => (0, KindData::Healer(Box::default())),
            Class::Hook => (1, KindData::Hook(Box::default())),
            Class::Hooker => (0, KindData::Hooker(Box::default())),
            Class::Hotspot => (0, KindData::Hotspot(Box::default())),
            Class::House => (0, KindData::House(Box::default())),
            Class::HousePortal => (0, KindData::HousePortal(Box::default())),
            Class::Key => (0, KindData::Key(Box::default())),
            Class::Lifestone => (0, KindData::Lifestone(Box::default())),
            Class::LightSource => (0, KindData::LightSource(Box::default())),
            Class::Lockpick => (0, KindData::Lockpick(Box::default())),
            Class::ManaStone => (0, KindData::ManaStone(Box::default())),
            Class::MeleeWeapon => (0, KindData::MeleeWeapon(Box::default())),
            Class::Missile => (0, KindData::Missile(Box::default())),
            Class::MissileLauncher => (0, KindData::MissileLauncher(Box::default())),
            Class::PKModifier => (0, KindData::PKModifier(Box::default())),
            Class::Pet => (2, KindData::Pet(Box::default())),
            Class::PetDevice => (0, KindData::PetDevice(Box::default())),
            Class::Player => (3, KindData::Player),
            Class::Portal => (0, KindData::Portal(Box::default())),
            Class::PressurePlate => (0, KindData::PressurePlate(Box::default())),
            Class::Scroll => (0, KindData::Scroll(Box::default())),
            Class::Sentinel => (3, KindData::Sentinel(Box::default())),
            Class::SkillAlterationDevice => (0, KindData::SkillAlterationDevice(Box::default())),
            Class::SlumLord => (1, KindData::SlumLord(Box::default())),
            Class::SpellComponent => (0, KindData::SpellComponent(Box::default())),
            Class::SpellProjectile => (0, KindData::SpellProjectile(Box::default())),
            Class::Stackable => (0, KindData::Stackable(Box::default())),
            Class::Storage => (1, KindData::Storage(Box::default())),
            Class::Switch => (0, KindData::Switch(Box::default())),
            Class::Vendor => (2, KindData::Vendor(Box::default())),
            Class::WorldObject => (0, KindData::WorldObject),
        };
        WorldObject {
            container: (depth >= 1).then(Box::default),
            creature: (depth >= 2).then(Box::default),
            player: (depth >= 3).then(Box::default),
            kind,
            ..Default::default()
        }
    }

    /// `new <class>(weenie, guid)`. A new biota is created taking all of its values from the
    /// weenie. Players take an account id: use
    /// [`player_from_weenie`](crate::world_objects::player::player_from_weenie).
    ///
    /// # Panics
    /// For `WorldObject` (abstract in ACE) and the player classes, which have no such
    /// constructor.
    #[must_use]
    pub fn from_weenie(
        env: &CtorEnv<'_>,
        class: Class,
        weenie: Arc<Weenie>,
        guid: ObjectGuid,
    ) -> WorldObject {
        construct(env, class, CtorSource::Weenie(weenie, guid))
    }

    /// `new <class>(biota)`: restore an object from the database. Players take their inventory
    /// and session: use [`player_from_biota_with_character`](crate::world_objects::player::player_from_biota_with_character).
    ///
    /// # Panics
    /// As [`WorldObject::from_weenie`].
    #[must_use]
    pub fn from_biota(env: &CtorEnv<'_>, class: Class, biota: Biota) -> WorldObject {
        construct(env, class, CtorSource::Biota(biota))
    }
}

/// Allocates `class` and runs its constructor chain. See the module documentation.
fn construct(env: &CtorEnv<'_>, class: Class, src: CtorSource) -> WorldObject {
    use crate::world_objects as c;

    let mut o = WorldObject::allocate(class);
    let ctor: fn(&mut WorldObject, &CtorEnv<'_>, CtorSource) = match class {
        Class::AdvocateFane => c::advocate_fane::advocate_fane_ctor,
        Class::AdvocateItem => c::advocate_item::advocate_item_ctor,
        Class::Allegiance => c::allegiance::allegiance_ctor,
        Class::Ammunition => c::ammunition::ammunition_ctor,
        Class::AttributeTransferDevice => {
            c::attribute_transfer_device::attribute_transfer_device_ctor
        }
        Class::AugmentationDevice => c::augmentation_device::augmentation_device_ctor,
        Class::Bindstone => c::bindstone::bindstone_ctor,
        Class::Book => c::book::book_ctor,
        Class::Caster => c::caster::caster_ctor,
        Class::Chest => c::chest::chest_ctor,
        Class::Clothing => c::clothing::clothing_ctor,
        Class::Coin => c::coin::coin_ctor,
        Class::CombatPet => c::combat_pet::combat_pet_ctor,
        Class::Container => c::container::container_ctor,
        Class::Corpse => c::corpse::corpse_ctor,
        Class::Cow => c::cow::cow_ctor,
        Class::CraftTool => c::craft_tool::craft_tool_ctor,
        Class::Creature => c::creature::creature_ctor,
        Class::Door => c::door::door_ctor,
        Class::Food => c::food::food_ctor,
        Class::Game => c::game::game_ctor,
        Class::GamePiece => c::game_piece::game_piece_ctor,
        Class::Gem => c::gem::gem_ctor,
        Class::GenericObject => c::generic_object::generic_object_ctor,
        Class::Healer => c::healer::healer_ctor,
        Class::Hook => c::hook::hook_ctor,
        Class::Hooker => c::hooker::hooker_ctor,
        Class::Hotspot => c::hotspot::hotspot_ctor,
        Class::House => c::house::house_ctor,
        Class::HousePortal => c::house_portal::house_portal_ctor,
        Class::Key => c::key::key_ctor,
        Class::Lifestone => c::lifestone::lifestone_ctor,
        Class::LightSource => c::light_source::light_source_ctor,
        Class::Lockpick => c::lockpick::lockpick_ctor,
        Class::ManaStone => c::mana_stone::mana_stone_ctor,
        Class::MeleeWeapon => c::melee_weapon::melee_weapon_ctor,
        Class::Missile => c::missile::missile_ctor,
        Class::MissileLauncher => c::missile_launcher::missile_launcher_ctor,
        Class::PKModifier => c::pk_modifier::pk_modifier_ctor,
        Class::Pet => c::pet::pet_ctor,
        Class::PetDevice => c::pet_device::pet_device_ctor,
        Class::Portal => c::portal::portal_ctor,
        Class::PressurePlate => c::pressure_plate::pressure_plate_ctor,
        Class::Scroll => c::scroll::scroll_ctor,
        Class::SkillAlterationDevice => c::skill_alteration_device::skill_alteration_device_ctor,
        Class::SlumLord => c::slum_lord::slum_lord_ctor,
        Class::SpellComponent => c::spell_component::spell_component_ctor,
        Class::SpellProjectile => c::spell_projectile::spell_projectile_ctor,
        Class::Stackable => c::stackable::stackable_ctor,
        Class::Storage => c::storage::storage_ctor,
        Class::Switch => c::switch::switch_ctor,
        Class::Vendor => c::vendor::vendor_ctor,
        Class::Player | Class::Sentinel | Class::Admin => {
            panic!("{} has no (Weenie, ObjectGuid) or (Biota) constructor; use player_from_weenie/player_from_biota_with_character", class.name())
        }
        Class::WorldObject => panic!("WorldObject is abstract; construct a concrete class"),
    };
    ctor(&mut o, env, src);
    o
}

/// `protected WorldObject(Weenie weenie, ObjectGuid guid)` and `protected WorldObject(Biota
/// biota)`: the base of every constructor chain.
// ACE: WorldObject.WorldObject
pub fn world_object_ctor(o: &mut WorldObject, env: &CtorEnv<'_>, src: CtorSource) {
    match src {
        CtorSource::Weenie(weenie, guid) => {
            o.biota = convert_to_biota(&weenie, guid.full(), false, true);
            o.weenie = Some(weenie);
            o.guid = guid;

            world_object_initialize_property_dictionaries(o);
            world_object_set_ephemeral_values(o, env);
            crate::world_objects::world_object_generators::initialize_generator(o, env);
            crate::world_objects::world_object_tick::initialize_heartbeats(o, env);

            // `(int)Time.GetUnixTime()`: a saturating double to int cast.
            o.set_creation_timestamp(Some(env.w.now.unix_time.cs_cast()));
        }
        CtorSource::Biota(biota) => {
            o.biota = biota;
            o.guid = ObjectGuid::new(o.biota.id);

            o.wo.world_object_database.biota_originated_from_database = true;

            world_object_initialize_property_dictionaries(o);
            world_object_set_ephemeral_values(o, env);
            crate::world_objects::world_object_generators::initialize_generator(o, env);
            crate::world_objects::world_object_tick::initialize_heartbeats(o, env);
        }
    }
}

// ACE: WorldObject.InitializePropertyDictionaries
fn world_object_initialize_property_dictionaries(o: &mut WorldObject) {
    if o.biota.properties_enchantment_registry.is_none() {
        o.biota.properties_enchantment_registry = Some(Vec::new());
    }
}

// ACE: WorldObject.SetEphemeralValues
fn world_object_set_ephemeral_values(o: &mut WorldObject, _env: &CtorEnv<'_>) {
    o.wo.world_object.object_description_flags = ObjectDescriptionFlag::Attackable;

    o.wo.world_object.emote_manager =
        crate::world_objects::managers::emote_manager::EmoteManager::new();
    o.wo.world_object.enchantment_manager =
        crate::world_objects::managers::enchantment_manager_with_caching::EnchantmentManagerWithCaching::new();

    if o.placement().is_none() {
        o.set_placement(Some(Placement::Resting));
    }

    if o.motion_table_id() != 0 {
        o.wo.world_object_properties.current_motion_state =
            Some(Motion::from_stance(MotionStance::Invalid));
    }
}

// ============================================================ WorldObject.cs members

/// `const float LocalBroadcastRange`.
// ACE: WorldObject.LocalBroadcastRange
pub const LOCAL_BROADCAST_RANGE: f32 = 96.0;
// ACE: WorldObject.LocalBroadcastRangeSq
pub const LOCAL_BROADCAST_RANGE_SQ: f32 = LOCAL_BROADCAST_RANGE * LOCAL_BROADCAST_RANGE;

impl WorldObject {
    // ACE: WorldObject.IsShield
    #[must_use]
    pub fn is_shield(&self) -> bool {
        let combat_use = self.combat_use();
        combat_use.is_some() && combat_use == Some(CombatUse::Shield)
    }

    // ValidLocations is bugged for some older two-handed weapons, still contains MeleeWeapon instead of TwoHanded?
    // ACE: WorldObject.IsTwoHanded
    #[must_use]
    pub fn is_two_handed(&self) -> bool {
        self.weapon_skill() == Skill::TwoHandedCombat
    }

    // ACE: WorldObject.IsBow
    #[must_use]
    pub fn is_bow(&self) -> bool {
        let style = self.default_combat_style();
        style.is_some() && (style == Some(CombatStyle::Bow) || style == Some(CombatStyle::Crossbow))
    }

    // ACE: WorldObject.IsAtlatl
    #[must_use]
    pub fn is_atlatl(&self) -> bool {
        let style = self.default_combat_style();
        style.is_some() && style == Some(CombatStyle::Atlatl)
    }

    // ACE: WorldObject.IsAmmoLauncher
    #[must_use]
    pub fn is_ammo_launcher(&self) -> bool {
        self.is_bow() || self.is_atlatl()
    }

    // ACE: WorldObject.IsThrownWeapon
    #[must_use]
    pub fn is_thrown_weapon(&self) -> bool {
        let style = self.default_combat_style();
        style.is_some() && style == Some(CombatStyle::ThrownWeapon)
    }

    // ACE: WorldObject.IsRanged
    #[must_use]
    pub fn is_ranged(&self) -> bool {
        self.is_ammo_launcher() || self.is_thrown_weapon()
    }

    // ACE: WorldObject.IsCaster
    #[must_use]
    pub fn is_caster_weapon(&self) -> bool {
        let style = self.default_combat_style();
        style.is_some() && style == Some(CombatStyle::Magic)
    }

    /// Logical Game Data
    // ACE: WorldObject.ContainerType
    #[must_use]
    pub fn container_type(&self) -> ContainerType {
        if self.biota.weenie_type == WeenieType::Container {
            ContainerType::Container
        } else if self.requires_pack_slot() {
            ContainerType::Foci
        } else {
            ContainerType::NonContainer
        }
    }

    /// `GetProperties(wo)`: every `PropertyInt` in `Enum.GetValues` order (ascending value), with
    /// the object's value or null. ACE calls it on another object; `self` is `wo` here, as the
    /// method reads nothing from `this`.
    // ACE: WorldObject.GetProperties
    #[must_use]
    pub fn get_properties(&self) -> DotNetDict<PropertyInt, Option<i32>> {
        let mut fields: Vec<PropertyInt> = PropertyInt::ALL.to_vec();
        fields.sort_by_key(|f| f.0);

        let mut props = DotNetDict::new();
        for field in fields {
            let prop = self.get_property(field);
            // Dictionary.Add throws on a duplicate key; PropertyInt has no aliases.
            assert!(
                props.insert(field, prop).is_none(),
                "System.ArgumentException: duplicate PropertyInt {field:?}"
            );
        }
        props
    }

    // ACE: WorldObject.IsTradeNote
    #[must_use]
    pub fn is_trade_note(&self) -> bool {
        self.item_type() == ItemType::PromissoryNote
    }

    // ACE: WorldObject.HasArmorLevel
    #[must_use]
    pub fn has_armor_level(&self) -> bool {
        // A null ArmorLevel compares false.
        self.armor_level().is_some_and(|a| a > 0)
    }

    // ACE: WorldObject.IsSocietyArmor
    #[must_use]
    pub fn is_society_armor(&self) -> bool {
        let wield_skill_type = self.wield_skill_type();
        wield_skill_type.is_some_and(|t| t >= i32::from(PropertyInt::SocietyRankCelhan.0))
            && wield_skill_type.is_some_and(|t| t <= i32::from(PropertyInt::SocietyRankRadblo.0))
    }
}

/// NPC refuses this item with a custom response, or accepts it. ACE's `out emote` is the answer:
/// `Some` for `true`.
// ACE: WorldObject.HasGiveOrRefuseEmoteForItem
pub fn has_give_or_refuse_emote_for_item(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
) -> Option<empyrean_entity::models::properties_emote::PropertiesEmote> {
    use crate::world_objects::managers::emote_manager;
    use empyrean_entity::enums::EmoteCategory;

    let item_wcid = w
        .objects
        .get(item)
        .expect("ACE: item is null (NullReferenceException)")
        .biota
        .weenie_class_id;

    // NPC refuses this item, with a custom response
    let refuse_item = emote_manager::get_emote_set(
        w,
        this,
        EmoteCategory::Refuse,
        None,
        None,
        Some(item_wcid),
        true,
    );
    if refuse_item.is_some() {
        return refuse_item;
    }

    // NPC accepts this item
    let give_item = emote_manager::get_emote_set(
        w,
        this,
        EmoteCategory::Give,
        None,
        None,
        Some(item_wcid),
        true,
    );
    if give_item.is_some() {
        return give_item;
    }

    None
}

/// Returns TRUE if this object has wo in VisibleTargets list (physics `ObjMaint`); false when
/// either has no physics body.
// ACE: WorldObject.IsVisibleTarget
pub fn is_visible_target(w: &World, this: ObjectGuid, wo: ObjectGuid) -> bool {
    let (Some(a), Some(b)) = (
        w.objects.get(this).and_then(|o| o.phys),
        w.objects.get(wo).and_then(|o| o.phys),
    ) else {
        return false;
    };

    // note: VisibleTargets is only maintained for monsters and combat pets
    let id = crate::physics::phys_ext::id(w, b).unwrap_or(0);
    crate::physics::object_maint::visible_targets_contains_key(w, a, id)
}

/// The line-of-sight probe (`PhysicsObj.makeObject(0x02000124, 0, false, true)` with the
/// missile state): a static body for the sight setup.
fn make_sight_object(w: &mut World) -> PhysHandle {
    use crate::physics::phys_ext;
    let sight_obj = phys_ext::make_object(w, 0x0200_0124, 0, false);
    let state = phys_ext::state(w, sight_obj);
    phys_ext::set_state(
        w,
        sight_obj,
        state | empyrean_entity::enums::PhysicsState::Missile,
    );
    sight_obj
}

/// Returns TRUE if this object has direct line-of-sight visibility to input object.
// DIVERGE: ACE sets `SightObj.ProjectileTarget` for the sweep; the shared crate's transition has no projectile-target filter (V1).
// ACE: WorldObject.IsDirectVisible
pub fn is_direct_visible(w: &mut World, this: ObjectGuid, wo: ObjectGuid) -> bool {
    use crate::physics::phys_ext;
    let (Some(h), Some(target)) = (phys_ext::physics_obj(w, this), phys_ext::physics_obj(w, wo))
    else {
        return false;
    };
    let (Some(mut start_pos), Some(mut target_pos)) =
        (phys_ext::position(w, h), phys_ext::position(w, target))
    else {
        return false;
    };

    let sight_obj = make_sight_object(w);

    if phys_ext::get_block_dist(start_pos.cell.0, target_pos.cell.0) > 1 {
        // (ACE returns without destroying the probe; nothing else holds it)
        phys_ext::destroy_object(w, sight_obj);
        return false;
    }

    let height = |w: &World, h: PhysHandle| {
        w.physics
            .get(h)
            .map_or(0.0, dereth_physics::obj::PhysicsObj::height)
    };
    let radius = |w: &World, h: PhysHandle| {
        w.physics
            .get(h)
            .map_or(0.0, dereth_physics::obj::PhysicsObj::radius)
    };

    // set to eye level
    start_pos.frame.origin.z += height(w, h) - height(w, sight_obj);
    target_pos.frame.origin.z += height(w, target) - height(w, sight_obj);

    // `Vector3.Normalize(target - start)`, then `start += dir * radsum`
    let (dx, dy, dz) = (
        target_pos.frame.origin.x - start_pos.frame.origin.x,
        target_pos.frame.origin.y - start_pos.frame.origin.y,
        target_pos.frame.origin.z - start_pos.frame.origin.z,
    );
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    let radsum = radius(w, h) + radius(w, sight_obj);
    start_pos.frame.origin.x += dx / len * radsum;
    start_pos.frame.origin.y += dy / len * radsum;
    start_pos.frame.origin.z += dz / len * radsum;

    // SightObj.CurCell = PhysicsObj.CurCell
    let cur_cell = w.physics.get(h).and_then(|o| o.cell);
    if let Some(o) = w.physics.get_mut(sight_obj) {
        o.cell = cur_cell;
    }

    // perform line of sight test
    let transition = w
        .physics
        .transition(sight_obj, &start_pos, &target_pos, false);

    phys_ext::destroy_object(w, sight_obj);

    let Some(transition) = transition else {
        return false;
    };

    // check if target object was reached
    transition
        .collision_info
        .collide_object
        .iter()
        .any(|(c, _)| *c == target)
}

/// Returns TRUE if this object has direct line-of-sight visibility to a position: the sweep runs
/// from the position back to this object.
// DIVERGE: as for the object overload (no `ProjectileTarget` on the probe).
// ACE: WorldObject.IsDirectVisible
pub fn is_direct_visible_position(w: &mut World, this: ObjectGuid, pos: &Position) -> bool {
    use crate::physics::phys_ext;
    let Some(h) = phys_ext::physics_obj(w, this) else {
        return false;
    };
    let Some(mut start_pos) = phys_ext::position(w, h) else {
        return false;
    };
    let mut target_pos = phys_ext::to_physics_position(pos);

    let sight_obj = make_sight_object(w);

    if phys_ext::get_block_dist(start_pos.cell.0, target_pos.cell.0) > 1 {
        phys_ext::destroy_object(w, sight_obj);
        return false;
    }

    let height = |w: &World, h: PhysHandle| {
        w.physics
            .get(h)
            .map_or(0.0, dereth_physics::obj::PhysicsObj::height)
    };
    let radius = |w: &World, h: PhysHandle| {
        w.physics
            .get(h)
            .map_or(0.0, dereth_physics::obj::PhysicsObj::radius)
    };

    // set to eye level
    start_pos.frame.origin.z += height(w, h) - height(w, sight_obj);
    target_pos.frame.origin.z += height(w, sight_obj);

    // `Vector3.Normalize(target - start)`, then `start += dir * radsum`
    let (dx, dy, dz) = (
        target_pos.frame.origin.x - start_pos.frame.origin.x,
        target_pos.frame.origin.y - start_pos.frame.origin.y,
        target_pos.frame.origin.z - start_pos.frame.origin.z,
    );
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    let radsum = radius(w, h) + radius(w, sight_obj);
    start_pos.frame.origin.x += dx / len * radsum;
    start_pos.frame.origin.y += dy / len * radsum;
    start_pos.frame.origin.z += dz / len * radsum;

    // SightObj.CurCell = PhysicsObj.CurCell
    let cur_cell = w.physics.get(h).and_then(|o| o.cell);
    if let Some(o) = w.physics.get_mut(sight_obj) {
        o.cell = cur_cell;
    }

    // perform line of sight test
    let transition = w
        .physics
        .transition(sight_obj, &target_pos, &start_pos, false);

    phys_ext::destroy_object(w, sight_obj);

    let Some(transition) = transition else {
        return false;
    };

    // check if target object was reached
    transition
        .collision_info
        .collide_object
        .iter()
        .any(|(c, _)| *c == h)
}

/// Whether a line-of-sight transition from this body to `wo`'s reaches it. The
/// target set around the sweep changes nothing unless this body is a missile (`MissileIgnore`).
// ACE: WorldObject.IsMeleeVisible
pub fn is_melee_visible(w: &mut World, this: ObjectGuid, wo: ObjectGuid) -> bool {
    use crate::physics::phys_ext;
    let (Some(h), Some(target)) = (phys_ext::physics_obj(w, this), phys_ext::physics_obj(w, wo))
    else {
        return false;
    };

    let (Some(start_pos), Some(target_pos)) =
        (phys_ext::position(w, h), phys_ext::position(w, target))
    else {
        return false;
    };

    phys_ext::set_projectile_target(w, h, Some(target));

    // perform line of sight test
    let transition = phys_ext::transition(w, h, &start_pos, &target_pos, false);

    phys_ext::set_projectile_target(w, h, None);

    let Some(transition) = transition else {
        return false;
    };

    // check if target object was reached
    transition
        .collision_info
        .collide_object
        .iter()
        .any(|(c, _)| *c == target)
}

/// Whether the projectile, from where it is, can see this creature's eyes: a line-of-sight
/// transition of the projectile's body to this creature's position raised to eye level (unit
/// 4.8b). Always true for a non-creature or an ethereal one.
/// The projectile's target is this creature for the sweep, so it passes through every other
/// creature (`ObjectInfo.MissileIgnore`, in the shared crate since A11).
// ACE: WorldObject.IsProjectileVisible
pub fn is_projectile_visible(w: &mut World, this: ObjectGuid, proj: ObjectGuid) -> bool {
    let Some(o) = w.objects.get(this) else {
        return false;
    };
    if !o.is_creature() || o.ethereal().unwrap_or(false) {
        return true;
    }

    let Some(p) = w.objects.get(proj) else {
        return false;
    };
    let (Some(h), Some(ph)) = (o.phys, p.phys) else {
        return false;
    };

    let (Some(mut target_pos), Some(start_pos)) = (
        crate::physics::phys_ext::position(w, h),
        crate::physics::phys_ext::position(w, ph),
    ) else {
        return false;
    };

    // set to eye level
    let height = w
        .physics
        .get(h)
        .map_or(0.0, dereth_physics::obj::PhysicsObj::height);
    let proj_height = w
        .physics
        .get(ph)
        .map_or(0.0, dereth_physics::obj::PhysicsObj::height);
    target_pos.frame.origin.z += height - proj_height;

    let prev_target = crate::physics::phys_ext::projectile_target(w, ph);
    crate::physics::phys_ext::set_projectile_target(w, ph, Some(h));

    // perform line of sight test
    let transition = crate::physics::phys_ext::transition(w, ph, &start_pos, &target_pos, false);

    crate::physics::phys_ext::set_projectile_target(w, ph, prev_target);

    let Some(transition) = transition else {
        return false;
    };

    // check if target object was reached
    transition
        .collision_info
        .collide_object
        .iter()
        .any(|&(c, _)| c == h)
}

/// The `propertydump` text: ACE's header lines (class file and guid).
// DIVERGE: ACE then lists every private field and public property of the runtime type by .NET reflection; the port has no reflection, so only the header and the two section titles are written (the member is a ledger skip, V8).
// ACE: WorldObject.DebugOutputString
pub fn debug_output_string(w: &World, this: ObjectGuid, obj: ObjectGuid) -> String {
    use std::fmt::Write as _;
    let mut sb = String::new();

    // DIVERGE: names Empyrean and the object's class where ACE's names ACE and the class's C# file (brand).
    let _ = writeln!(sb, "Empyrean Debug Output:");
    let _ = writeln!(sb, "Object class: {}", dispatch::class_of(w, this).name());
    let _ = writeln!(sb, "Guid: {} (0x{:X})", obj.full(), obj.full());

    let _ = writeln!(sb, "----- Private Fields -----");
    let _ = writeln!(sb, "----- Public Properties -----");

    sb
}

/// Sends `GameEventUpdateHealth` to the examiner.
// ACE: WorldObject.QueryHealth
pub fn query_health(w: &mut World, this: ObjectGuid, examiner: empyrean_net::SessionId) {
    let mut health_percentage = 1f32;

    if w.objects.get(this).is_some_and(WorldObject::is_creature) {
        let health = w
            .objects
            .get(this)
            .map(WorldObject::health)
            .expect("ACE: creature");
        let current = health.current(w.objects.get(this).expect("ACE: creature"));
        let max_value = health.max_value(
            &mut crate::world_objects::entity::creature_attribute::StatCtx::in_world(w, this),
        );
        #[allow(clippy::cast_precision_loss)]
        {
            health_percentage = current as f32 / max_value as f32;
        }
    }

    let update_health =
        crate::network::game_event::events::game_event_update_health::game_event_update_health(
            crate::network::game_event::game_event_message::session_data(w, examiner),
            this.full(),
            health_percentage,
        );
    crate::network::game_messages::game_message::enqueue_send(w, examiner, update_health);
}

// ACE: WorldObject.QueryItemMana
pub fn query_item_mana(w: &mut World, this: ObjectGuid, examiner: empyrean_net::SessionId) {
    let Some(o) = w.objects.get(this) else { return };

    let mut mana_percentage = 1.0f32;
    let mut success = 0u32;

    if let (Some(cur), Some(max)) = (o.item_cur_mana(), o.item_max_mana()) {
        mana_percentage = cur as f32 / max as f32;
        success = 1;
    }

    if success == 0 {
        // according to retail PCAPs, if success = 0, mana = 0.
        mana_percentage = 0.0;
    }

    let update_mana = crate::network::game_event::events::game_event_query_item_mana_response::game_event_query_item_mana_response(
        crate::network::game_event::game_event_message::session_data(w, examiner),
        this.full(),
        mana_percentage,
        success,
    );
    crate::network::game_messages::game_message::enqueue_send(w, examiner, update_mana);
}

/// Broadcasts `GameMessageSetState` (outbound messages and physics state).
// ACE: WorldObject.EnqueueBroadcastPhysicsState
pub fn enqueue_broadcast_physics_state(w: &mut World, this: ObjectGuid) {
    use crate::network::game_messages::messages::game_message_set_state::game_message_set_state;
    use crate::world_objects::world_object_networking::enqueue_broadcast;
    use empyrean_entity::enums::{CloakStatus, PhysicsState};

    let Some(o) = w.objects.get(this) else { return };
    let Some(h) = o.phys else { return };
    let visibility = o.visibility();
    let cloaked_player = o.is_player() && o.cloak_status() == CloakStatus::On;
    let state = crate::physics::phys_ext::state(w, h);

    if !visibility {
        let msg = game_message_set_state(w.objects.get_mut(this).expect("present"), state);
        enqueue_broadcast(w, this, true, &[msg]);
    } else if cloaked_player {
        let ps = PhysicsState(state.0 & !(PhysicsState::Cloaked.0 | PhysicsState::NoDraw.0));
        let own = game_message_set_state(w.objects.get_mut(this).expect("present"), state);
        if let Some(session) = crate::managers::player_manager::player_session(w, this) {
            crate::network::game_messages::game_message::enqueue_send(w, session, own);
        }
        let others = game_message_set_state(w.objects.get_mut(this).expect("present"), ps);
        enqueue_broadcast(w, this, false, &[others]);
    } else {
        let msg = game_message_set_state(w.objects.get_mut(this).expect("present"), state);
        enqueue_broadcast(w, this, true, &[msg]);
    }
}

// ACE: WorldObject.EnqueueBroadcastUpdateObject
/// Broadcasts a `GameMessageUpdateObject` of this object (to itself too).
pub fn enqueue_broadcast_update_object(w: &mut World, this: ObjectGuid) {
    let msg = crate::network::game_messages::messages::game_message_update_object::game_message_update_object(w, this, false, false);
    crate::world_objects::world_object_networking::enqueue_broadcast(w, this, true, &[msg]);
}

// ACE: WorldObject.ApplyVisualEffects
pub fn apply_visual_effects(
    w: &mut World,
    this: ObjectGuid,
    effect: empyrean_entity::enums::PlayScript,
    speed: f32,
) {
    let Some(o) = w.objects.get(this) else { return };
    if o.current_landblock.is_some() {
        play_particle_effect(w, this, effect, this, speed);
    }
}

/// plays particle effect like spell casting or bleed etc.. (`GameMessageScript`)
// ACE: WorldObject.PlayParticleEffect
pub fn play_particle_effect(
    w: &mut World,
    this: ObjectGuid,
    effect_id: empyrean_entity::enums::PlayScript,
    target_id: ObjectGuid,
    speed: f32,
) {
    let msg = crate::network::game_messages::messages::game_message_script::game_message_script(
        target_id, effect_id, speed,
    );
    crate::world_objects::world_object_networking::enqueue_broadcast(w, this, true, &[msg]);
}

// ACE: WorldObject.ApplySoundEffects
pub fn apply_sound_effects(
    w: &mut World,
    this: ObjectGuid,
    sound: empyrean_entity::enums::Sound,
    volume: f32,
) {
    let Some(o) = w.objects.get(this) else { return };
    if o.current_landblock.is_some() {
        play_sound_effect(w, this, sound, this, volume);
    }
}

/// Broadcasts `GameMessageSound(targetId, soundId, volume)` (`volume` defaults to 1).
// ACE: WorldObject.PlaySoundEffect
pub fn play_sound_effect(
    w: &mut World,
    this: ObjectGuid,
    sound_id: empyrean_entity::enums::Sound,
    target_id: ObjectGuid,
    volume: f32,
) {
    let msg = crate::network::game_messages::messages::game_message_sound::game_message_sound(
        target_id, sound_id, volume,
    );
    crate::world_objects::world_object_networking::enqueue_broadcast(w, this, true, &[msg]);
}

/// The dungeon fix-ups for a position in a landblock that holds a dungeon: `AdjustDungeonPos`,
/// then `AdjustDungeonCells`.
// ACE: WorldObject.AdjustDungeon
pub fn adjust_dungeon(w: &mut World, pos: &mut Position) {
    adjust_dungeon_pos(w, pos);
    adjust_dungeon_cells(w, pos);
}

/// `LScape.get_landblock(pos.Cell)`, which on the server loads the landblock
/// (`LandblockManager.GetLandblock(lbid, false, false)`), then its `HasDungeon`.
fn lscape_get_landblock_has_dungeon(w: &mut World, cell: u32) -> bool {
    let lbid = empyrean_entity::LandblockId::new(cell | 0xFFFF);
    let landblock = crate::managers::landblock_manager::get_landblock(w, lbid, false, false);
    w.landblock_manager
        .landblocks
        .expect_mut(landblock)
        .has_dungeon()
}

/// Moves a position in a dungeon landblock into the env cell that holds its point
/// (`AdjustCell.Get(dungeonID).GetCell(pos.Pos)`); true when the cell changed.
// ACE: WorldObject.AdjustDungeonCells
pub fn adjust_dungeon_cells(w: &mut World, pos: &mut Position) -> bool {
    if !lscape_get_landblock_has_dungeon(w, pos.cell()) {
        return false;
    }

    let dungeon_id = pos.cell() >> 16;

    let adjust_cell = crate::entity::position_extensions::adjust_cell_get(w, dungeon_id);
    let cell_id =
        crate::entity::position_extensions::adjust_cell_get_cell(w, &adjust_cell, pos.pos());

    if let Some(cell_id) = cell_id {
        if pos.cell() != cell_id {
            pos.set_landblock_id(empyrean_entity::LandblockId::new(cell_id));
            return true;
        }
    }
    false
}

/// `AdjustPos.Adjust(dungeonID, pos)` for a position in a dungeon landblock: true when the
/// dungeon has a position fix-up profile.
// ACE: WorldObject.AdjustDungeonPos
pub fn adjust_dungeon_pos(w: &mut World, pos: &mut Position) -> bool {
    if !lscape_get_landblock_has_dungeon(w, pos.cell()) {
        return false;
    }

    let dungeon_id = pos.cell() >> 16;

    adjust_pos_adjust(dungeon_id, pos)
}

/// `AdjustPos.Adjust` (`Physics/Util/AdjustPos.cs`): moves the position by the dungeon profile's
/// `GoodPosition - BadPosition`. ACE's `DungeonProfiles` table is empty (its three profiles, the
/// Burial Temple, North Glenden Prison and Nuhmudira's Dungeon, were commented out as no longer
/// needed in 2019), so no dungeon has one and the position is never moved.
fn adjust_pos_adjust(_dungeon_id: u32, _pos: &mut Position) -> bool {
    // `if (!DungeonProfiles.TryGetValue(dungeonID, out var profile)) return false;`
    false
}

/// Returns the modified damage for a weapon, with the wielder enchantments taken into account
/// (`weapon` defaults to the wielder's equipped weapon).
// ACE: WorldObject.GetDamageMod
pub fn get_damage_mod(
    w: &mut World,
    this: ObjectGuid,
    wielder: ObjectGuid,
    weapon: Option<ObjectGuid>,
) -> crate::entity::base_damage_mod::BaseDamageMod {
    let base_damage = crate::dispatch::get_base_damage::get_base_damage(w, this);

    let weapon = weapon.or_else(|| {
        crate::world_objects::creature_equipment::get_equipped_weapon(w, wielder, false)
    });

    crate::entity::base_damage_mod::BaseDamageMod::with_wielder(w, base_damage, wielder, weapon)
}

/// If this is a container or a creature, all of the inventory and/or equipped objects will also
/// be destroyed. An object should only be destroyed once.
// ACE: WorldObject.Destroy
pub fn destroy(
    w: &mut World,
    this: ObjectGuid,
    raise_notify_of_destruction_event: bool,
    from_landblock_unload: bool,
) {
    let now = w.now.unix_time;
    let Some(o) = w.objects.get_mut(this) else {
        return;
    };

    if o.wo.world_object.is_destroyed {
        //log.WarnFormat("Item 0x{0:X8}:{1} called destroy more than once.", Guid.Full, Name);

        // Not ACE: an object kept after its Destroy because a container still listed it (see the
        // end of this function) leaves the store once nothing lists it any more (C#'s GC).
        if !is_listed_by_live_container(w, this) && !is_held_by_projectile_in_flight(w, this) {
            release_equipped_objects(w, this);
            w.objects.remove(this);
        }
        return;
    }

    o.wo.world_object.is_destroyed = true;

    o.set_released_timestamp(Some(now));

    if o.is_container() {
        for item in crate::world_objects::container::inventory_values(w, this) {
            destroy(w, item, true, false);
        }
    }

    let Some(o) = w.objects.get(this) else { return };
    if o.is_creature() {
        for item in crate::world_objects::creature_equipment::equipped_objects_values(w, this) {
            destroy(w, item, true, false);
        }
    }

    let Some(o) = w.objects.get(this) else { return };
    if o.is_pet() {
        // `if (pet.P_PetOwner?.CurrentActivePet == this) pet.P_PetOwner.CurrentActivePet = null;`
        // (Pet.cs)
        if let Some(owner) = crate::world_objects::pet::p_pet_owner(w, this) {
            let f = crate::world_objects::player_use::fields_mut(w, owner);
            if f.current_active_pet == Some(this) {
                f.current_active_pet = None;
            }
        }

        // `if (pet.P_PetDevice?.Pet == Guid.Full) pet.P_PetDevice.Pet = null;`
        if let Some(device) = crate::world_objects::pet::p_pet_device(w, this) {
            let d = w.objects.get_mut(device).expect("ACE: P_PetDevice");
            if d.pet() == Some(this.full()) {
                d.set_pet(None);
            }
        }
    }
    let Some(o) = w.objects.get(this) else { return };

    if o.is_vendor() {
        for wo in crate::world_objects::vendor::default_items_for_sale(w, this) {
            destroy(w, wo, true, false);
        }

        for wo in crate::world_objects::vendor::unique_items_for_sale(w, this) {
            destroy(w, wo, true, false);
        }
    }

    if raise_notify_of_destruction_event {
        crate::world_objects::world_object_generators::notify_of_event(
            w,
            this,
            empyrean_entity::enums::RegenerationType::Destruction,
        );
    }

    if w.objects.get(this).is_some_and(WorldObject::is_generator) {
        if from_landblock_unload {
            crate::world_objects::world_object_generators::process_generator_destruction_directive(
                w,
                this,
                empyrean_entity::enums::GeneratorDestruct::Destroy,
                from_landblock_unload,
            );
        } else {
            crate::world_objects::world_object_generators::on_generator_destroy(w, this);
        }
    }

    let Some(o) = w.objects.get(this) else { return };
    if let Some(current_landblock) = o.current_landblock {
        crate::entity::landblock::remove_world_object(
            w,
            current_landblock,
            this,
            false,
            false,
            true,
        );
    }

    crate::world_objects::world_object_database::remove_biota_from_database(w, this, true);

    if this.is_dynamic() {
        crate::managers::guid_manager::recycle_dynamic_guid(w, this);
    }

    // Not ACE: nothing refers to a destroyed object any more, so the store drops it here (a
    // missing object stands for `IsDestroyed`), after every step above has read it. The one
    // exception is an object a live container still lists in its `Inventory` (any path that
    // destroys an object without first taking it out of its container; not the death loot
    // (V291), which leaves the creature's Inventory before going onto the corpse).
    // In ACE the container's reference keeps the destroyed object alive and usable; here it stays
    // in the store, `IsDestroyed`, until that container lets go of it (arch). Likewise an item
    // equipped by a creature whose own Destroy is running stays until that Destroy ends
    // (the creature's removal broadcast names each equipped item, `RemoveTrackedObject`).
    // Likewise the launcher or ammo of a projectile still in flight (a monster's
    // `SwitchToMeleeAttack` destroys its missile weapon, a thrower's last dart is destroyed at the
    // launch): ACE's `ProjectileLauncher`/`ProjectileAmmo` keep it, and the hit's
    // `DamageEvent` reads it; it stays until the projectile leaves the world.
    if is_listed_by_live_container(w, this)
        || is_equipped_by_wielder_being_destroyed(w, this)
        || is_held_by_projectile_in_flight(w, this)
    {
        return;
    }
    let links = w.objects.get(this).and_then(|o| o.projectile);
    release_equipped_objects(w, this);
    w.objects.remove(this);
    if let Some(links) = links {
        release_projectile_links(w, &links);
    }
}

/// Not ACE: `this` is the `ProjectileLauncher` or `ProjectileAmmo` of a projectile still on a
/// landblock (C#'s reference from the projectile keeps it readable). Only a missile weapon or
/// ammunition can be one, so only those scan the loaded landblocks.
fn is_held_by_projectile_in_flight(w: &World, this: ObjectGuid) -> bool {
    use empyrean_entity::enums::WeenieType;
    let Some(o) = w.objects.get(this) else {
        return false;
    };
    if !matches!(
        o.biota.weenie_type,
        WeenieType::MissileLauncher | WeenieType::Missile | WeenieType::Ammunition
    ) {
        return false;
    }
    crate::managers::landblock_manager::get_loaded_landblocks(w)
        .into_iter()
        .any(|id| {
            w.landblock_manager.landblocks.get(id).is_some_and(|lb| {
                lb.world_object_guids()
                    .chain(lb.pending_addition_guids())
                    .any(|&g| {
                        g != this
                            && w.objects.get(g).is_some_and(|p| {
                                p.current_landblock.is_some()
                                    && !p.wo.world_object.is_destroyed
                                    && p.projectile.as_ref().is_some_and(|l| {
                                        l.launcher == Some(this) || l.ammo == Some(this)
                                    })
                            })
                    })
            })
        })
}

/// Not ACE: a projectile left the world (it hit, or was destroyed); the destroyed launcher or ammo
/// [`is_held_by_projectile_in_flight`] kept for it leaves the store once nothing else holds it.
pub fn release_projectile_links(w: &mut World, links: &ProjectileLinks) {
    for g in [links.launcher, links.ammo].into_iter().flatten() {
        if w.objects
            .get(g)
            .is_some_and(|o| o.wo.world_object.is_destroyed)
        {
            destroy(w, g, false, false);
        }
    }
}

/// Not ACE: `this` is in the `EquippedObjects` of a wielder that is itself destroyed (a creature's
/// Destroy destroys its equipped items before it leaves its landblock); C#'s reference from that
/// dictionary keeps the item readable until the wielder goes.
fn is_equipped_by_wielder_being_destroyed(w: &World, this: ObjectGuid) -> bool {
    let Some(wielder) = w.objects.get(this).and_then(|o| o.wielder) else {
        return false;
    };
    w.objects
        .get(wielder)
        .is_some_and(|c| c.wo.world_object.is_destroyed)
        && crate::world_objects::creature_equipment::equipped_objects_values(w, wielder)
            .contains(&this)
}

/// Not ACE: the destroyed equipped items [`is_equipped_by_wielder_being_destroyed`] kept in the
/// store leave it with their wielder.
fn release_equipped_objects(w: &mut World, this: ObjectGuid) {
    if !w.objects.get(this).is_some_and(WorldObject::is_creature) {
        return;
    }
    for item in crate::world_objects::creature_equipment::equipped_objects_values(w, this) {
        if w.objects
            .get(item)
            .is_some_and(|i| i.wo.world_object.is_destroyed)
            && !is_listed_by_live_container(w, item)
        {
            w.objects.remove(item);
        }
    }
}

/// Not ACE: `this.Container` is a live container, not itself destroyed, whose `Inventory` still
/// lists `this` (C#'s reference from that dictionary keeps the object reachable).
fn is_listed_by_live_container(w: &World, this: ObjectGuid) -> bool {
    let Some(container) = w
        .objects
        .get(this)
        .and_then(|o| o.wo.world_object_properties.container)
    else {
        return false;
    };
    w.objects.get(container).is_some_and(|c| {
        !c.wo.world_object.is_destroyed
            && c.container
                .as_ref()
                .is_some_and(|d| d.container.inventory.contains_key(&this))
    })
}

// ACE: WorldObject.FadeOutAndDestroy
pub fn fade_out_and_destroy(
    w: &mut World,
    this: ObjectGuid,
    raise_notify_of_destruction_event: bool,
) {
    let script = crate::network::game_messages::messages::game_message_script::game_message_script(
        this,
        empyrean_entity::enums::PlayScript::Destroy,
        1.0,
    );
    crate::world_objects::world_object_networking::enqueue_broadcast(w, this, true, &[script]);

    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(1.0f32));
    action_chain.add_action(Actor::Object(this), move |w| {
        destroy(w, this, raise_notify_of_destruction_event, false)
    });
    action_chain.enqueue_chain(w);
}

// ACE: WorldObject.GetPluralName
#[must_use]
pub fn get_plural_name(w: &World, this: ObjectGuid) -> String {
    let plural_name = w.objects.get(this).and_then(WorldObject::plural_name);

    match plural_name {
        Some(p) => p,
        // `Name.Pluralize()`: a null Name throws in ACE.
        None => pluralize(
            &dispatch::name::name(w, this).expect("System.NullReferenceException: Name is null"),
        ),
    }
}

/// Returns TRUE if this object has non-cyclic animations in progress (physics).
// ACE: WorldObject.IsAnimating
pub fn is_animating(w: &World, this: ObjectGuid) -> bool {
    let Some(o) = w.objects.get(this) else {
        return false;
    };
    o.phys
        .is_some_and(|h| crate::physics::phys_ext::is_animating(w, h))
}

/// Executes a motion/animation for this object: adds to the physics animation system, and
/// broadcasts to nearby players. Returns the amount it takes to execute the motion.
/// `ExecuteMotion(Motion motion, bool sendClient = true, float? maxRange = null, bool persist =
/// false)`.
///
/// # Panics
/// When the object has no `CurrentMotionState` (ACE: `NullReferenceException`).
// ACE: WorldObject.ExecuteMotion
pub fn execute_motion(
    w: &mut World,
    this: ObjectGuid,
    mut motion: Motion,
    send_client: bool,
    max_range: Option<f32>,
    persist: bool,
) -> f32 {
    let mut motion_command = motion.motion_state.forward_command;

    if motion_command == MotionCommand::Ready {
        motion_command = MotionCommand(motion.stance.0);
    }

    // run motion command on server through physics animation system
    let body = object(w, this).phys;
    if let Some(h) = body.filter(|_| motion_command != MotionCommand::Ready) {
        // get_minterp(), a fresh raw state (forward 0, hold key Run, style = the command),
        // `UpdateTime = CurrentTime` when idle, then apply_raw_movement(true, true)
        crate::physics::phys_ext::execute_motion_physics(w, h, motion_command.0);
    }

    if persist
        && crate::managers::property_manager::get_bool(w, "persist_movement", false, true).item
    {
        let current = current_motion_state(w, this)
            .expect("ACE: CurrentMotionState is null (NullReferenceException)");
        motion.persist(&current);
    }

    // hardcoded ready?
    let current = current_motion_state(w, this)
        .expect("ACE: CurrentMotionState is null (NullReferenceException)");
    let anim_length = crate::physics::motion_table::get_animation_length_between(
        w,
        object(w, this).motion_table_id(),
        current.stance,
        current.motion_state.forward_command,
        motion_command,
        1.0,
    );
    let broadcast = send_client.then(|| motion.clone());
    set_current_motion_state(w, this, motion);

    // broadcast to nearby players
    if let Some(motion) = broadcast {
        crate::world_objects::world_object_networking::enqueue_broadcast_motion(
            w,
            this,
            &motion,
            max_range,
            Some(false),
        );
    }

    anim_length
}

// ACE: WorldObject.ExecuteMotionPersist
pub fn execute_motion_persist(
    w: &mut World,
    this: ObjectGuid,
    motion: Motion,
    send_client: bool,
    max_range: Option<f32>,
) -> f32 {
    execute_motion(w, this, motion, send_client, max_range, true)
}

/// `SetStance(MotionStance stance, bool broadcast = true)`: the stance becomes the current motion
/// (keeping the turn and sidestep when `persist_movement` is set) and is broadcast.
///
/// # Panics
/// With `persist_movement` set and no `CurrentMotionState` (ACE: `NullReferenceException`).
// ACE: WorldObject.SetStance
pub fn set_stance(w: &mut World, this: ObjectGuid, stance: MotionStance, broadcast: bool) {
    let mut motion = Motion::from_stance(stance);

    if crate::managers::property_manager::get_bool(w, "persist_movement", false, true).item {
        let current = current_motion_state(w, this)
            .expect("ACE: CurrentMotionState is null (NullReferenceException)");
        motion.persist(&current);
    }

    set_current_motion_state(w, this, motion);

    if broadcast {
        let current = current_motion_state(w, this).expect("set above");
        crate::world_objects::world_object_networking::enqueue_broadcast_motion(
            w, this, &current, None, None,
        );
    }
}

/// `CurrentMotionState` (a copy; ACE hands out the reference).
fn current_motion_state(w: &World, this: ObjectGuid) -> Option<Motion> {
    object(w, this)
        .wo
        .world_object_properties
        .current_motion_state
        .clone()
}

/// `CurrentMotionState = motion`.
fn set_current_motion_state(w: &mut World, this: ObjectGuid, motion: Motion) {
    w.objects
        .get_mut(this)
        .expect("ACE: this is null")
        .wo
        .world_object_properties
        .current_motion_state = Some(motion);
}

/// Returns the relative direction of this creature in relation to target expressed as a
/// quadrant: Front/Back, Left/Right.
// ACE: WorldObject.GetRelativeDir
pub fn get_relative_dir(w: &World, this: ObjectGuid, target: ObjectGuid) -> Quadrant {
    let location = w
        .objects
        .get(this)
        .and_then(WorldObject::location)
        .expect("System.NullReferenceException: Location");
    let target_location = w
        .objects
        .get(target)
        .and_then(WorldObject::location)
        .expect("System.NullReferenceException: target.Location");

    let source_pos = Vector3::new(location.position_x, location.position_y, 0.0);
    let target_pos = Vector3::new(target_location.position_x, target_location.position_y, 0.0);
    let mut target_dir = vector_heading(target_location.rotation());

    target_dir.z = 0.0;
    target_dir = Vector3::normalize(target_dir);

    let source_to_target = Vector3::normalize(source_pos - target_pos);

    let dir = dot(source_to_target, target_dir);
    let angle = cross(source_to_target, target_dir);

    let mut quadrant = if angle.z <= 0.0 {
        Quadrant::Left
    } else {
        Quadrant::Right
    };

    quadrant |= if dir >= 0.0 {
        Quadrant::Front
    } else {
        Quadrant::Back
    };

    quadrant
}

/// `new AFrame(origin, q).get_vector_heading()`: row 2 of `Matrix4x4.CreateFromQuaternion(q)`.
fn vector_heading(q: empyrean_entity::Quaternion) -> Vector3 {
    let xx = q.x * q.x;
    let zz = q.z * q.z;
    let xy = q.x * q.y;
    let wz = q.z * q.w;
    let yz = q.y * q.z;
    let wx = q.x * q.w;
    Vector3::new(2.0 * (xy - wz), 1.0 - 2.0 * (zz + xx), 2.0 * (yz + wx))
}

/// `Vector3.Dot`.
fn dot(a: Vector3, b: Vector3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

/// `Vector3.Cross`.
fn cross(a: Vector3, b: Vector3) -> Vector3 {
    Vector3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

/// Returns TRUE if this WorldObject is a generic linkspot. Linkspots are used for things like
/// Houses, where the portal destination should be populated at runtime.
// ACE: WorldObject.IsLinkSpot
pub fn is_link_spot(env: &CtorEnv<'_>, o: &WorldObject) -> bool {
    if o.biota.weenie_type != WeenieType::Generic {
        return false;
    }
    crate::world_objects::world_object_properties::weenie_class_name(
        env.get_cached_weenie,
        o.biota.weenie_class_id,
    ) == "portaldestination"
}

// ACE: WorldObject.ConvertToMoASkill
pub fn convert_to_mo_a_skill(w: &mut World, this: ObjectGuid, skill: Skill) -> Skill {
    use empyrean_entity::enums::ext::skill_extensions::{RETIRED_MELEE, RETIRED_MISSILE};

    if w.objects.get(this).is_some_and(WorldObject::is_player) {
        if RETIRED_MELEE.contains(&skill) {
            return crate::world_objects::player_combat::get_highest_melee_skill(w, this);
        }
        if RETIRED_MISSILE.contains(&skill) {
            return Skill::MissileWeapons;
        }
    }

    skill
}

/// `GetCurrentMotionState(out currentStance, out currentMotion)`: the current stance and forward
/// command, or `(Invalid, Ready)` without a `CurrentMotionState`.
// ACE: WorldObject.GetCurrentMotionState
pub fn get_current_motion_state(w: &World, this: ObjectGuid) -> (MotionStance, MotionCommand) {
    let mut current_stance = MotionStance::Invalid;
    let mut current_motion = MotionCommand::Ready;

    if let Some(current) = object(w, this)
        .wo
        .world_object_properties
        .current_motion_state
        .as_ref()
    {
        current_stance = current.stance;

        // `if (CurrentMotionState.MotionState != null)`: always set here
        current_motion = current.motion_state.forward_command;
    }

    (current_stance, current_motion)
}

/// `weenie.GetValue() / weenie.GetMaxStructure()`, floored at 0. A MaxStructure of 0 divides by
/// zero, which throws in ACE and panics here.
// ACE: WorldObject.StructureUnitValue
pub fn structure_unit_value(env: &CtorEnv<'_>, o: &WorldObject) -> i32 {
    let weenie = (env.get_cached_weenie)(o.biota.weenie_class_id);
    let weenie_value = weenie.as_deref().and_then(Weenie::get_value).unwrap_or(0);
    let weenie_max_structure = weenie
        .as_deref()
        .and_then(Weenie::get_max_structure)
        .unwrap_or(1);

    let structure_unit_value = weenie_value / weenie_max_structure;

    structure_unit_value.max(0)
}

/// The object behind `this` for a virtual body; the dispatcher has already resolved it.
fn object(w: &World, this: ObjectGuid) -> &WorldObject {
    w.objects
        .get(this)
        .unwrap_or_else(|| panic!("virtual call on missing object {this:?}"))
}

// ---- virtual-dispatch targets: each `not_ported!` until it is ported ----

/// Initializes a new default physics object: a static-geometry body for an object, an animated
/// one for a creature, named, linked to its `WeenieObject`, scaled and given its default state;
/// an object that bumps on creation (a corpse, a storage chest) starts moving up at 0.5 m/s.
// ACE: WorldObject.InitPhysicsObj
pub fn world_object_init_physics_obj(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    use crate::physics::phys_ext;

    let default_state = calculated_physics_state(w, this);

    let Some(o) = w.objects.get(this) else { return };
    let (is_creature, is_hook, mut setup_table_id) =
        (o.is_creature(), o.is_hook(), o.setup_table_id());
    let (wcid, motion_table_id, obj_scale) =
        (o.biota.weenie_class_id, o.motion_table_id(), o.obj_scale());
    let bump_velocity = o.wo.world_object.bump_velocity;

    let h = if !is_creature {
        // `Static == null || !Static.Value`. Not ACE's (a fix, V305): the calculated state's
        // Static, which is the weenie's before there is a body (ACE read false there).
        let is_dynamic = !default_state.contains(empyrean_entity::enums::PhysicsState::Static);

        // TODO: REMOVE ME?
        // Temporary workaround fix to account for ace spawn placement issues with certain hooked objects.
        if is_hook {
            let hook_weenie = w.content.get_cached_weenie(wcid);
            setup_table_id = hook_weenie
                .and_then(|hw| {
                    hw.properties_did.as_ref().and_then(|d| {
                        d.get(&empyrean_entity::enums::PropertyDataId::Setup)
                            .copied()
                    })
                })
                .unwrap_or(setup_table_id);
        }
        // TODO: REMOVE ME?

        phys_ext::make_object(w, setup_table_id, this.full(), is_dynamic)
    } else {
        phys_ext::make_anim_object(w, setup_table_id, true)
    };
    if let Some(o) = w.objects.get_mut(this) {
        o.phys = Some(h);
        o.wo.world_object.destroyed_physics_obj = None;
    }

    phys_ext::set_object_guid(w, h, this);

    let wobj = crate::physics::weenie_object::WeenieObject::new(w, this);
    phys_ext::set_weenie_obj(w, h, wobj);

    phys_ext::set_motion_table_id(w, h, motion_table_id);

    phys_ext::set_scale_static(w, h, obj_scale.unwrap_or(1.0));

    phys_ext::set_state(w, h, default_state);

    //if (creature != null) AllowEdgeSlide = true;

    if bump_velocity {
        phys_ext::set_velocity_field(w, h, empyrean_common::dotnet::Vector3::new(0.0, 0.0, 0.5));
    }
}

/// `CalculatedPhysicsState()` (WorldObject_Networking.cs).
fn calculated_physics_state(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) -> empyrean_entity::enums::PhysicsState {
    crate::world_objects::world_object_networking::calculated_physics_state(w, this)
}

// ACE: WorldObject.OnCollideObject
#[allow(unused_variables)]
pub fn world_object_on_collide_object(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    // thrown weapons
    let projectile_target = w
        .objects
        .get(this)
        .and_then(|o| o.projectile)
        .and_then(|p| p.target);
    if projectile_target.is_none() {
        return;
    }

    crate::world_objects::projectile_collision_helper::on_collide_object(w, this, target);
}

// ACE: WorldObject.OnCollideObjectEnd
#[allow(unused_variables)]
pub fn world_object_on_collide_object_end(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    // ACE's body is empty.
}

// ACE: WorldObject.OnCollideEnvironment
#[allow(unused_variables)]
pub fn world_object_on_collide_environment(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) {
    // thrown weapons
    let projectile_target = w
        .objects
        .get(this)
        .and_then(|o| o.projectile)
        .and_then(|p| p.target);
    if projectile_target.is_none() {
        return;
    }

    crate::world_objects::projectile_collision_helper::on_collide_environment(w, this);
}

// ACE: WorldObject.OnGeneration
#[allow(unused_variables)]
pub fn world_object_on_generation(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    generator: empyrean_entity::ObjectGuid,
) {
    //Console.WriteLine($"{Name}.OnGeneration()");

    crate::world_objects::managers::emote_manager::on_generation(w, this);
}

// ACE: WorldObject.EnterWorld
#[allow(unused_variables)]
pub fn world_object_enter_world(w: &mut crate::World, this: empyrean_entity::ObjectGuid) -> bool {
    let Some(o) = w.objects.get(this) else {
        return false;
    };
    if o.location().is_none() {
        return false;
    }

    if !crate::managers::landblock_manager::add_object(w, this, false) {
        return false;
    }

    if w.objects
        .get(this)
        .and_then(WorldObject::suppress_generate_effect)
        != Some(true)
    {
        apply_visual_effects(w, this, empyrean_entity::enums::PlayScript::Create, 1.0);
    }

    if let Some(generator) = w
        .objects
        .get(this)
        .and_then(|o| o.wo.world_object_generators.generator)
    {
        crate::dispatch::on_generation::on_generation(w, this, generator);
    }

    //Console.WriteLine($"{Name}.EnterWorld()");

    true
}

/// Not ACE: an object that ACE leaves unreferenced, typically after a failed
/// `EnterWorld` whose caller ignores the result (`@create`), leaves `World.objects` with its
/// physics body and everything it holds (its equipped objects and its inventory, recursively).
/// ACE's garbage collector takes it; it never calls `Destroy`, so nothing is broadcast and the
/// guid is not recycled (it is simply never handed out again). An object that is on a landblock,
/// in a container or wielded is still referenced and is left alone.
pub fn drop_unreferenced(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    let placed = w.objects.get(this).is_none_or(|o| {
        o.current_landblock.is_some() || o.container_id().is_some() || o.wielder_id().is_some()
    });
    if placed {
        return;
    }
    drop_with_contents(w, this);
}

/// Removes `this`, then what it holds, from the store.
fn drop_with_contents(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    let Some(o) = w.objects.get(this) else { return };
    let mut held: Vec<empyrean_entity::ObjectGuid> = Vec::new();
    if let Some(c) = o.container.as_ref() {
        held.extend(c.container.inventory.keys().copied());
    }
    if let Some(c) = o.creature.as_ref() {
        held.extend(c.creature_equipment.equipped_objects.keys().copied());
    }
    if let Some(h) = o.phys {
        crate::physics::phys_ext::destroy_object(w, h);
    }
    w.objects.remove(this);
    for g in held {
        drop_with_contents(w, g);
    }
}

// ACE: WorldObject.GetAttackMessage
/// Returns a strike message based on damage type and severity.
///
/// # Panics
/// When `creature` is gone, or the damage type has no name (ACE: `NullReferenceException`).
pub fn world_object_get_attack_message(
    w: &crate::World,
    _this: empyrean_entity::ObjectGuid,
    creature: empyrean_entity::ObjectGuid,
    damage_type: empyrean_entity::enums::DamageType,
    amount: u32,
) -> String {
    use empyrean_common::dotnet::cast::CsCast;
    let c = w
        .objects
        .get(creature)
        .expect("System.NullReferenceException: creature");
    let amount_f: f32 = amount.cs_cast();
    let base_f: f32 = c.health().base(w, c).cs_cast();
    let percent = amount_f / base_f;
    let (verb, _plural) =
        crate::world_objects::monster_combat::shim::strings_get_attack_verb(damage_type, percent);
    let r#type = damage_type
        .get_name()
        .expect("System.NullReferenceException: GetName()")
        .to_lowercase();
    let name = crate::dispatch::name::name(w, creature).unwrap_or_default();
    format!("You {verb} {name} for {amount} points of {type} damage!")
}

// ACE: WorldObject.GetBaseDamage
/// `Damage ?? 0` and `DamageVariance ?? 0`.
///
/// # Panics
/// When `this` is gone (ACE: `NullReferenceException`).
pub fn world_object_get_base_damage(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
) -> crate::entity::base_damage::BaseDamage {
    let o = w
        .objects
        .get(this)
        .expect("System.NullReferenceException: this");
    let max_damage = o
        .get_property(empyrean_entity::enums::PropertyInt::Damage)
        .unwrap_or(0);
    let variance = o
        .get_property(empyrean_entity::enums::PropertyFloat::DamageVariance)
        .unwrap_or(0.0);

    #[allow(clippy::cast_possible_truncation)] // ACE's `(float)` cast
    crate::entity::base_damage::BaseDamage::new(max_damage, variance as f32)
}

// ACE: WorldObject.HandleMotionDone
#[allow(unused_variables)]
pub fn world_object_handle_motion_done(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    motion_id: u32,
    success: bool,
) {
    // ACE's body is empty.
}

// ACE: WorldObject.OnMoveComplete
#[allow(unused_variables)]
pub fn world_object_on_move_complete(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    status: empyrean_entity::enums::WeenieError,
) {
    // ACE's body is empty.
}

// ACE: WorldObject.IsAttunedOrContainsAttuned
#[allow(unused_variables)]
pub fn world_object_is_attuned_or_contains_attuned(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
) -> bool {
    // A null Attuned compares false.
    object(w, this)
        .attuned()
        .is_some_and(|a| a >= AttunedStatus::Attuned)
}

// ACE: WorldObject.IsStickyAttunedOrContainsStickyAttuned
#[allow(unused_variables)]
pub fn world_object_is_sticky_attuned_or_contains_sticky_attuned(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
) -> bool {
    object(w, this)
        .attuned()
        .is_some_and(|a| a >= AttunedStatus::Sticky)
}

// ACE: WorldObject.IsUniqueOrContainsUnique
#[allow(unused_variables)]
pub fn world_object_is_unique_or_contains_unique(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
) -> bool {
    object(w, this).unique().is_some()
}

// ACE: WorldObject.GetUniqueObjects
#[allow(unused_variables)]
pub fn world_object_get_unique_objects(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
) -> Vec<empyrean_entity::ObjectGuid> {
    if object(w, this).unique().is_none() {
        Vec::new()
    } else {
        vec![this]
    }
}

// ACE: WorldObject.IsBeingTradedOrContainsItemBeingTraded
#[allow(unused_variables)]
pub fn world_object_is_being_traded_or_contains_item_being_traded(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    guid_list: &empyrean_common::dotnet::DotNetHashSet<empyrean_entity::ObjectGuid>,
) -> bool {
    guid_list.contains(&this)
}

// ACE: WorldObject.Name
#[allow(unused_variables)]
pub fn world_object_name(w: &crate::World, this: empyrean_entity::ObjectGuid) -> Option<String> {
    object(w, this).get_property(PropertyString::Name)
}

// ACE: WorldObject.Name
#[allow(unused_variables)]
pub fn world_object_set_name(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    value: String,
) {
    if let Some(o) = w.objects.get_mut(this) {
        o.set_property(PropertyString::Name, value);
    }
}

impl crate::network::sequence::sequence_manager::HasSequences for WorldObject {
    fn world_object(&self) -> &WorldObject {
        self
    }

    fn sequences(&mut self) -> &mut crate::network::sequence::sequence_manager::SequenceManager {
        &mut self.sequences
    }
}

// ---- physics hand-offs ----

/// Places the object's body at its `Location` (after `AdjustDungeon`); on failure the body is
/// destroyed and dropped. On success the location is synced from the body and `Home` is set.
// ACE: WorldObject.AddPhysicsObj
pub fn add_physics_obj(w: &mut crate::World, this: empyrean_entity::ObjectGuid) -> bool {
    use crate::physics::phys_ext;

    let Some(h) = phys_ext::physics_obj(w, this) else {
        return false;
    };
    if phys_ext::cur_cell(w, h).is_some() {
        return false;
    }

    adjust_dungeon_location(w, this);

    // exclude linkspots from spawning
    if w.objects
        .get(this)
        .is_some_and(|o| o.biota.weenie_class_id == 10762)
    {
        return true;
    }

    let Some(location) = w.objects.get(this).and_then(|o| o.location()) else {
        return false;
    };
    let Some(cell) = phys_ext::get_landcell(w, location.cell()) else {
        phys_ext::destroy_object(w, h);
        if let Some(o) = w.objects.get_mut(this) {
            o.phys = None;
        }
        return false;
    };

    let mut physics_location = phys_ext::to_physics_position(&location);
    physics_location.cell = cell;

    let success = phys_ext::enter_world(w, h, &physics_location);

    if !success || phys_ext::cur_cell(w, h).is_none() {
        phys_ext::destroy_object(w, h);
        if let Some(o) = w.objects.get_mut(this) {
            o.phys = None;
        }
        return false;
    }

    sync_location(w, this);

    if let Some(o) = w.objects.get_mut(this) {
        let home = o
            .location()
            .map(|l| empyrean_entity::Position::from_position(&l));
        o.set_position(empyrean_entity::enums::PositionType::Home, home);
    }

    true
}

/// Copies the body's cell, origin and rotation into `Location` (without the cell check).
// ACE: WorldObject.SyncLocation
pub fn sync_location(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    let body = w.objects.get(this).and_then(|o| o.phys);
    let Some(pos) = body.and_then(|h| crate::physics::phys_ext::position(w, h)) else {
        return;
    };
    let Some(o) = w.objects.get_mut(this) else {
        return;
    };
    let Some(location) = o.get_position_mut(empyrean_entity::enums::PositionType::Location) else {
        return;
    };

    location.set_landblock_id(empyrean_entity::LandblockId::new(pos.cell.0));

    // skip ObjCellID check when updating from physics
    location.position_x = pos.frame.origin.x;
    location.position_y = pos.frame.origin.y;
    location.position_z = pos.frame.origin.z;

    let r = pos.frame.rotation;
    location.set_rotation(empyrean_common::dotnet::Quaternion::new(r.x, r.y, r.z, r.w));
}

/// `AdjustDungeon(Location)`: the dungeon fix-ups on the object's location, written back.
fn adjust_dungeon_location(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    let Some(mut location) = w.objects.get(this).and_then(|o| o.location()) else {
        return;
    };
    adjust_dungeon(w, &mut location);
    if let Some(o) = w.objects.get_mut(this) {
        if let Some(l) = o.get_position_mut(empyrean_entity::enums::PositionType::Location) {
            *l = location;
        }
    }
}
