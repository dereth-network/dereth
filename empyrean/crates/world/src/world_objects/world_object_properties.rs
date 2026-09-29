// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Properties.cs
//! Port of `Source/ACE.Server/WorldObjects/WorldObject_Properties.cs`.
//!
//! The base property machinery: `GetProperty`, `SetProperty`, `RemoveProperty`, `IncProperty`, the
//! `GetAllProperty*` enumerators and the position cache, including ACE's ephemeral overrides
//! (`[Ephemeral]` properties live in per-object dictionaries and never reach the biota). ACE has
//! one overload per property type; here one generic method takes any [`WoPropertyKey`].
//!
//! The typed wrappers (`Level`, `ItemType`, ...) are generated into `world_objects::props` from
//! ACE's property declarations; the few with custom logic from this file are hand-ported at
//! the end of this module.

use empyrean_common::dotnet::math::round;
use empyrean_common::dotnet::{CsCast, DotNetDict, DotNetHashSet};
use empyrean_entity::enums::ext::ephemeral_properties;
use std::sync::Arc;

use empyrean_common::dotnet::numerics::Vector3;
use empyrean_dat::file_types::SetupModel;
use empyrean_entity::enums::{
    CoverageMask, EquipMask, PKLevel, PhysicsState, Placement, PositionType, PropertyBool,
    PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt, PropertyInt64, PropertyString,
    WeenieType,
};
use empyrean_entity::shared_types::vector3_of_data;
use empyrean_entity::{ObjectGuid, Position, PropertyKey};

use crate::physics::phys_ext;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `WorldObject_Properties.cs`.
///
/// ACE's comment: "These dictionaries should ONLY be referenced by SetEphemeralValues, GetProperty,
/// SetProperty and RemoveProperty functions." A `None` value is a stored C# `null`: it hides the
/// biota value (see [`WorldObject::get_property`]).
#[derive(Debug, Default)]
pub struct WorldObjectPropertiesFields {
    // ACE: WorldObject.ephemeralPropertyBools
    pub(crate) ephemeral_property_bools: Option<DotNetDict<PropertyBool, Option<bool>>>,
    // ACE: WorldObject.ephemeralPropertyDataIds
    pub(crate) ephemeral_property_data_ids: Option<DotNetDict<PropertyDataId, Option<u32>>>,
    // ACE: WorldObject.ephemeralPropertyFloats
    pub(crate) ephemeral_property_floats: Option<DotNetDict<PropertyFloat, Option<f64>>>,
    // ACE: WorldObject.ephemeralPropertyInstanceIds
    pub(crate) ephemeral_property_instance_ids: Option<DotNetDict<PropertyInstanceId, Option<u32>>>,
    /// `protected` in ACE: subclasses store `EncumbranceVal` and `Value` here directly.
    // ACE: WorldObject.ephemeralPropertyInts
    pub ephemeral_property_ints: Option<DotNetDict<PropertyInt, Option<i32>>>,
    // ACE: WorldObject.ephemeralPropertyInt64s
    pub(crate) ephemeral_property_int64s: Option<DotNetDict<PropertyInt64, Option<i64>>>,
    // ACE: WorldObject.ephemeralPropertyStrings
    pub(crate) ephemeral_property_strings: Option<DotNetDict<PropertyString, Option<String>>>,
    // ACE: WorldObject.ephemeralPositions
    pub(crate) ephemeral_positions: DotNetDict<PositionType, Option<Position>>,
    /// Only `GetPosition`, `SetPosition`, `RemovePosition` and `SaveBiotaToDatabase` use this.
    // ACE: WorldObject.positionCache
    pub position_cache: DotNetDict<PositionType, Option<Position>>,
    /// The object's current motion (`null` until a constructor sets it).
    // ACE: WorldObject.CurrentMotionState
    pub current_motion_state: Option<crate::network::motion::movement_data::Motion>,
    /// The container whose `Inventory` holds this object (`Container.TryAddToInventory` sets it,
    /// `TryRemoveFromInventory` clears it); ACE's `public WorldObject Container;`.
    // ACE: WorldObject.Container
    pub container: Option<empyrean_entity::ObjectGuid>,
    /// The held items a creature shows as physics children, in insertion order (ACE's
    /// `public List<HeldItem> Children { get; } = new List<HeldItem>();`).
    // ACE: WorldObject.Children
    pub children: Vec<crate::entity::held_item::HeldItem>,
    /// A vendor's default item: the Shop create-list row's stack size (-1 = unlimited supply).
    // ACE: WorldObject.VendorShopCreateListStackSize
    pub vendor_shop_create_list_stack_size: Option<i32>,
    /// The object's last broadcast movement (`new MovementData()` until the first one).
    // ACE: WorldObject.CurrentMovementData
    pub current_movement_data: crate::network::motion::movement_data::MovementData,
}

/// A property enum `WorldObject` can get, set and remove: ACE's seven overload sets. Ties each
/// type to its ephemeral dictionary and its `EphemeralProperties` set.
pub trait WoPropertyKey: PropertyKey + 'static {
    /// ACE's `EphemeralProperties.Properties*` set for this type.
    const EPHEMERAL: &'static [Self];

    /// `SetProperty(PropertyInt)` also treats a key already present in the ephemeral dictionary as
    /// ephemeral ("EncumbranceVal and Value are sometimes treated as ephemeral"); no other
    /// overload does.
    const SET_CHECKS_EPHEMERAL_DICT: bool = false;

    /// The `ushort` value (`(ushort)r.Key`).
    fn ushort(self) -> u16;

    /// This type's `ephemeralProperty*` dictionary.
    fn ephemeral(f: &WorldObjectPropertiesFields)
        -> &Option<DotNetDict<Self, Option<Self::Value>>>;

    /// This type's `ephemeralProperty*` dictionary, mutably.
    fn ephemeral_mut(
        f: &mut WorldObjectPropertiesFields,
    ) -> &mut Option<DotNetDict<Self, Option<Self::Value>>>;
}

macro_rules! wo_property_key {
    ($key:ty, $set:ident, $dict:ident $(, $checks:literal)?) => {
        impl WoPropertyKey for $key {
            const EPHEMERAL: &'static [Self] = ephemeral_properties::$set;
            $(const SET_CHECKS_EPHEMERAL_DICT: bool = $checks;)?

            fn ushort(self) -> u16 {
                self.0
            }

            fn ephemeral(
                f: &WorldObjectPropertiesFields,
            ) -> &Option<DotNetDict<Self, Option<Self::Value>>> {
                &f.$dict
            }

            fn ephemeral_mut(
                f: &mut WorldObjectPropertiesFields,
            ) -> &mut Option<DotNetDict<Self, Option<Self::Value>>> {
                &mut f.$dict
            }
        }
    };
}

wo_property_key!(PropertyBool, PROPERTIES_BOOL, ephemeral_property_bools);
wo_property_key!(
    PropertyDataId,
    PROPERTIES_DATA_ID,
    ephemeral_property_data_ids
);
wo_property_key!(PropertyFloat, PROPERTIES_DOUBLE, ephemeral_property_floats);
wo_property_key!(
    PropertyInstanceId,
    PROPERTIES_INSTANCE_ID,
    ephemeral_property_instance_ids
);
wo_property_key!(PropertyInt, PROPERTIES_INT, ephemeral_property_ints, true);
wo_property_key!(PropertyInt64, PROPERTIES_INT64, ephemeral_property_int64s);
wo_property_key!(
    PropertyString,
    PROPERTIES_STRING,
    ephemeral_property_strings
);

impl WorldObject {
    fn props_fields(&self) -> &WorldObjectPropertiesFields {
        &self.wo.world_object_properties
    }

    fn props_fields_mut(&mut self) -> &mut WorldObjectPropertiesFields {
        &mut self.wo.world_object_properties
    }

    fn mark_changes_detected(&mut self) {
        self.wo.world_object_database.changes_detected = true;
    }

    // ========================================
    // GetProperty / SetProperty / RemoveProperty
    // ========================================

    /// The ephemeral value if the ephemeral dictionary has the key (even a stored `null`, which
    /// hides the biota), else the biota's.
    // ACE: WorldObject.GetProperty
    pub fn get_property<K: WoPropertyKey>(&self, property: K) -> Option<K::Value> {
        if let Some(value) = K::ephemeral(self.props_fields())
            .as_ref()
            .and_then(|d| d.get(&property))
        {
            return value.clone();
        }

        self.biota.get_property(property)
    }

    /// Ephemeral properties go to the ephemeral dictionary; anything else to the biota, setting
    /// `ChangesDetected` when the stored value changed.
    // ACE: WorldObject.SetProperty
    pub fn set_property<K: WoPropertyKey>(&mut self, property: K, value: K::Value) {
        let ephemeral = K::EPHEMERAL.contains(&property)
            || (K::SET_CHECKS_EPHEMERAL_DICT
                && K::ephemeral(self.props_fields())
                    .as_ref()
                    .is_some_and(|d| d.contains_key(&property)));

        if ephemeral {
            let dict =
                K::ephemeral_mut(self.props_fields_mut()).get_or_insert_with(DotNetDict::new);
            dict.insert(property, Some(value));
        } else {
            let changed = self.biota.set_property(property, value);

            if changed {
                self.mark_changes_detected();
            }
        }
    }

    // ACE: WorldObject.IncProperty
    pub fn inc_property(&mut self, property: PropertyFloat, value: f64) {
        let prop = self.get_property(property).unwrap_or(0.0);
        self.set_property(property, prop + value);
    }

    /// An ephemeral property is set to `null` (only if the dictionary exists); anything else is
    /// removed from the biota, setting `ChangesDetected` if it was there.
    // ACE: WorldObject.RemoveProperty
    pub fn remove_property<K: WoPropertyKey>(&mut self, property: K) {
        if K::EPHEMERAL.contains(&property) {
            if let Some(dict) = K::ephemeral_mut(self.props_fields_mut()).as_mut() {
                dict.insert(property, None);
            }
        } else if self.biota.try_remove_property(property) {
            self.mark_changes_detected();
        }
    }

    // ========================================
    // GetAllProperty / GetAllProperty*Where
    // ========================================

    /// The biota's bag in its enumeration order, then each ephemeral entry in the ephemeral
    /// dictionary's order: a value overwrites (or appends), a `null` removes (freeing its slot,
    /// which a later append reuses, as in .NET's `Dictionary`).
    fn get_all_property<K: WoPropertyKey>(
        &self,
        keys: Option<&DotNetHashSet<u16>>,
    ) -> DotNetDict<K, K::Value> {
        let wanted = |k: &K| keys.is_none_or(|keys| keys.contains(&k.ushort()));
        let mut results = DotNetDict::new();

        if let Some(bag) = K::bag(&self.biota).as_ref() {
            for (k, v) in bag.iter().filter(|(k, _)| wanted(k)) {
                results.insert(*k, v.clone());
            }
        }

        if let Some(ephemeral) = K::ephemeral(self.props_fields()).as_ref() {
            for (k, v) in ephemeral.iter().filter(|(k, _)| wanted(k)) {
                match v {
                    Some(v) => {
                        results.insert(*k, v.clone());
                    }
                    None => {
                        results.remove(k);
                    }
                }
            }
        }

        results
    }

    // ACE: WorldObject.GetAllPropertyBools
    pub fn get_all_property_bools(&self) -> DotNetDict<PropertyBool, bool> {
        self.get_all_property(None)
    }

    // ACE: WorldObject.GetAllPropertyDataId
    pub fn get_all_property_data_id(&self) -> DotNetDict<PropertyDataId, u32> {
        self.get_all_property(None)
    }

    // ACE: WorldObject.GetAllPropertyFloat
    pub fn get_all_property_float(&self) -> DotNetDict<PropertyFloat, f64> {
        self.get_all_property(None)
    }

    // ACE: WorldObject.GetAllPropertyInstanceId
    pub fn get_all_property_instance_id(&self) -> DotNetDict<PropertyInstanceId, u32> {
        self.get_all_property(None)
    }

    // ACE: WorldObject.GetAllPropertyInt
    pub fn get_all_property_int(&self) -> DotNetDict<PropertyInt, i32> {
        self.get_all_property(None)
    }

    // ACE: WorldObject.GetAllPropertyInt64
    pub fn get_all_property_int64(&self) -> DotNetDict<PropertyInt64, i64> {
        self.get_all_property(None)
    }

    // ACE: WorldObject.GetAllPropertyString
    pub fn get_all_property_string(&self) -> DotNetDict<PropertyString, String> {
        self.get_all_property(None)
    }

    // ACE: WorldObject.GetAllPropertyBoolsWhere
    pub fn get_all_property_bools_where(
        &self,
        keys: &DotNetHashSet<u16>,
    ) -> DotNetDict<PropertyBool, bool> {
        self.get_all_property(Some(keys))
    }

    // ACE: WorldObject.GetAllPropertyDataIdWhere
    pub fn get_all_property_data_id_where(
        &self,
        keys: &DotNetHashSet<u16>,
    ) -> DotNetDict<PropertyDataId, u32> {
        self.get_all_property(Some(keys))
    }

    // ACE: WorldObject.GetAllPropertyFloatWhere
    pub fn get_all_property_float_where(
        &self,
        keys: &DotNetHashSet<u16>,
    ) -> DotNetDict<PropertyFloat, f64> {
        self.get_all_property(Some(keys))
    }

    // ACE: WorldObject.GetAllPropertyInstanceIdWhere
    pub fn get_all_property_instance_id_where(
        &self,
        keys: &DotNetHashSet<u16>,
    ) -> DotNetDict<PropertyInstanceId, u32> {
        self.get_all_property(Some(keys))
    }

    // ACE: WorldObject.GetAllPropertyIntWhere
    pub fn get_all_property_int_where(
        &self,
        keys: &DotNetHashSet<u16>,
    ) -> DotNetDict<PropertyInt, i32> {
        self.get_all_property(Some(keys))
    }

    // ACE: WorldObject.GetAllPropertyInt64Where
    pub fn get_all_property_int64_where(
        &self,
        keys: &DotNetHashSet<u16>,
    ) -> DotNetDict<PropertyInt64, i64> {
        self.get_all_property(Some(keys))
    }

    // ACE: WorldObject.GetAllPropertyStringWhere
    pub fn get_all_property_string_where(
        &self,
        keys: &DotNetHashSet<u16>,
    ) -> DotNetDict<PropertyString, String> {
        self.get_all_property(Some(keys))
    }

    // ========================================
    // GetPosition, SetPosition, RemovePosition, GetAllPositions
    // ========================================

    /// The biota's position as ACE's cache fill would store it, with an invalid rotation repaired
    /// (`IsRotationValid` / `AttemptToFixRotation`, `Entity/PositionExtensions.cs`).
    fn load_position(&self, position_type: PositionType) -> Option<Position> {
        let mut position = self.biota.get_position(position_type);

        if let Some(p) = position.as_mut() {
            if !crate::entity::position_extensions::is_rotation_valid(p.rotation()) {
                crate::entity::position_extensions::attempt_to_fix_rotation(p, self, position_type);
            }
        }

        position
    }

    /// A copy of the position: the ephemeral entry (even a stored `null`), else the cached one,
    /// else the biota's.
    // ACE: WorldObject.GetPosition
    // DIVERGE: a `&self` read does not fill `positionCache`; `get_position_mut` does. ACE returns a shared reference that callers mutate in place; here that is `get_position_mut`.
    pub fn get_position(&self, position_type: PositionType) -> Option<Position> {
        let fields = self.props_fields();

        if let Some(ephemeral_position) = fields.ephemeral_positions.get(&position_type) {
            return *ephemeral_position;
        }

        if let Some(cached_position) = fields.position_cache.get(&position_type) {
            return *cached_position;
        }

        self.load_position(position_type)
    }

    /// ACE's `GetPosition` with its cache fill: the returned position is the live one (the
    /// ephemeral or cached entry) that callers may change in place, as ACE callers do with the
    /// returned reference; `SaveBiotaToDatabase` writes the cache back.
    // ACE: WorldObject.GetPosition
    pub fn get_position_mut(&mut self, position_type: PositionType) -> Option<&mut Position> {
        if self
            .props_fields()
            .ephemeral_positions
            .contains_key(&position_type)
        {
            return self
                .props_fields_mut()
                .ephemeral_positions
                .get_mut(&position_type)?
                .as_mut();
        }

        if !self
            .props_fields()
            .position_cache
            .contains_key(&position_type)
        {
            let position = self.load_position(position_type);
            self.props_fields_mut()
                .position_cache
                .insert(position_type, position);
        }

        self.props_fields_mut()
            .position_cache
            .get_mut(&position_type)?
            .as_mut()
    }

    /// ACE takes the `Position` by reference and stores that same object; here the position is a
    /// value, which is what ACE's documented `new Position(...)` copy idiom gives.
    // ACE: WorldObject.SetPosition
    pub fn set_position(&mut self, position_type: PositionType, position: Option<Position>) {
        if ephemeral_properties::POSITION_TYPES.contains(&position_type) {
            self.props_fields_mut()
                .ephemeral_positions
                .insert(position_type, position);
        } else {
            match position {
                None => self.remove_position(position_type),
                Some(position) => {
                    self.props_fields_mut()
                        .position_cache
                        .insert(position_type, Some(position));

                    self.biota.set_position(position_type, &position);
                    self.mark_changes_detected();
                }
            }
        }
    }

    // ACE: WorldObject.RemovePosition
    pub fn remove_position(&mut self, position_type: PositionType) {
        if ephemeral_properties::POSITION_TYPES.contains(&position_type) {
            self.props_fields_mut()
                .ephemeral_positions
                .insert(position_type, None);
        } else {
            self.props_fields_mut()
                .position_cache
                .remove(&position_type);

            if self.biota.try_remove_property_position(position_type) {
                self.mark_changes_detected();
            }
        }
    }

    /// Every biota position (rebuilt with the 8-argument `Position` constructor, so not from the
    /// cache), then the ephemeral positions applied as in `get_all_property_int`.
    ///
    /// # Panics
    /// When the biota has no position bag, as ACE throws `NullReferenceException`.
    // ACE: WorldObject.GetAllPositions
    // ACE-BUG: enumerates Biota.PropertiesPosition without a null check, so an object whose biota has no positions throws NullReferenceException.
    pub fn get_all_positions(&self) -> DotNetDict<PositionType, Position> {
        let mut results = DotNetDict::new();

        let bag = self
            .biota
            .properties_position
            .as_ref()
            .expect("NullReferenceException: Biota.PropertiesPosition is null");
        for (k, v) in bag.iter() {
            results.insert(
                *k,
                Position::from_components(
                    v.obj_cell_id,
                    v.position_x,
                    v.position_y,
                    v.position_z,
                    v.rotation_x,
                    v.rotation_y,
                    v.rotation_z,
                    v.rotation_w,
                    false,
                ),
            );
        }

        for (k, v) in self.props_fields().ephemeral_positions.iter() {
            match v {
                Some(v) => {
                    results.insert(*k, *v);
                }
                None => {
                    results.remove(k);
                }
            }
        }

        results
    }

    // ========================================
    // Physics state
    // ========================================

    /// `SetPhysicsState` for an object with no physics body (under construction, or in a pack):
    /// ACE sets or clears `state` on `PhysicsObj.State` only when there is a physics object, so
    /// this does nothing. An object with a body goes through the world-level
    /// [`phys_ext::set_physics_state`], which reaches `World.physics`.
    ///
    /// # Panics
    /// When the object has a body: the change would be lost, and every caller that can meet a
    /// placed object uses the world-level form.
    pub fn set_physics_state(&mut self, state: PhysicsState, value: Option<bool>) {
        let _ = (state, value);
        assert!(
            self.phys.is_none(),
            "0x{:08X} has a physics body: set its physics state through phys_ext::set_physics_state (world-level)",
            self.guid.full()
        );
    }

    /// The property half of `SetPhysicsPropertyState`, and [`Self::set_physics_state`] (an object
    /// with no body). An object with a body goes through [`phys_ext::set_physics_property_state`].
    ///
    /// # Panics
    /// As [`Self::set_physics_state`].
    // ACE: WorldObject.SetPhysicsPropertyState
    pub fn set_physics_property_state(
        &mut self,
        property: PropertyBool,
        state: PhysicsState,
        value: Option<bool>,
    ) {
        match value {
            Some(v) => {
                self.set_property(property, v);
                self.set_physics_state(state, value);
            }
            None => {
                self.remove_property(property);
                self.set_physics_state(state, Some(false)); // default to false for null, should get real physics default for this field
            }
        }
    }
}

// ========================================
// Hand-ported wrappers (the ones the generator leaves to a hand port)
// ========================================

/// `bool?` properties whose setter goes through `SetPhysicsPropertyState`: the getter and the
/// setter for an object with no physics body on `WorldObject`, and the world-level setter (a free
/// function of the same name) that also writes the body's state.
macro_rules! physics_property {
    ($get:ident, $set:ident, $prop:ident, $state:ident) => {
        impl WorldObject {
            pub fn $get(&self) -> Option<bool> {
                self.get_property(PropertyBool::$prop)
            }

            /// The setter for an object with no physics body (see
            /// [`WorldObject::set_physics_state`]).
            ///
            /// # Panics
            /// When the object has a physics body.
            pub fn $set(&mut self, value: Option<bool>) {
                self.set_physics_property_state(PropertyBool::$prop, PhysicsState::$state, value);
            }
        }

        /// The setter on a stored object: the property, and the body's state bit when it has
        /// a body (`SetPhysicsPropertyState`).
        pub fn $set(w: &mut World, wo: ObjectGuid, value: Option<bool>) {
            phys_ext::set_physics_property_state(
                w,
                wo,
                PropertyBool::$prop,
                PhysicsState::$state,
                value,
            );
        }
    };
}

// ACE: WorldObject.Ethereal
physics_property!(ethereal, set_ethereal, Ethereal, Ethereal);
// ACE: WorldObject.ReportCollisions
physics_property!(
    report_collisions,
    set_report_collisions,
    ReportCollisions,
    ReportCollisions
);
// ACE: WorldObject.IgnoreCollisions
physics_property!(
    ignore_collisions,
    set_ignore_collisions,
    IgnoreCollisions,
    IgnoreCollisions
);
// ACE: WorldObject.NoDraw
physics_property!(no_draw, set_no_draw, NoDraw, NoDraw);
// ACE: WorldObject.GravityStatus
physics_property!(gravity_status, set_gravity_status, GravityStatus, Gravity);
// ACE: WorldObject.LightsStatus
physics_property!(lights_status, set_lights_status, LightsStatus, LightingOn);
// ACE: WorldObject.ScriptedCollision
physics_property!(
    scripted_collision,
    set_scripted_collision,
    ScriptedCollision,
    ScriptedCollision
);
// ACE: WorldObject.Inelastic
physics_property!(inelastic, set_inelastic, Inelastic, Inelastic);
// ACE: WorldObject.ReportCollisionsAsEnvironment
physics_property!(
    report_collisions_as_environment,
    set_report_collisions_as_environment,
    ReportCollisionsAsEnvironment,
    ReportCollisionsAsEnvironment
);
// ACE: WorldObject.AllowEdgeSlide
physics_property!(
    allow_edge_slide,
    set_allow_edge_slide,
    AllowEdgeSlide,
    EdgeSlide
);
// ACE: WorldObject.IsFrozen
physics_property!(is_frozen, set_is_frozen, IsFrozen, Frozen);

impl WorldObject {
    /// `VisualClothingPriority` if set, else `ClothingPriority`.
    // ACE: WorldObject.VisualClothingPriority
    pub fn visual_clothing_priority(&self) -> Option<CoverageMask> {
        if self
            .get_property(PropertyInt::VisualClothingPriority)
            .map(|v| CoverageMask(v.cs_cast()))
            .is_some()
        {
            self.get_property(PropertyInt::VisualClothingPriority)
                .map(|v| CoverageMask(v.cs_cast()))
        } else {
            self.get_property(PropertyInt::ClothingPriority)
                .map(|v| CoverageMask(v.cs_cast()))
        }
    }

    /// Named `_prop` because `set_visual_clothing_priority` is ACE's method
    /// `setVisualClothingPriority` (renamed where the wrappers are generated).
    // ACE: WorldObject.VisualClothingPriority
    pub fn set_visual_clothing_priority_prop(&mut self, value: Option<CoverageMask>) {
        match value {
            None => self.remove_property(PropertyInt::VisualClothingPriority),
            Some(v) => self.set_property(PropertyInt::VisualClothingPriority, v.0.cs_cast()),
        }
    }

    /// `ItemWorkmanship / NumItemsInMaterial`. A value outside 1..10 is taken to be an old
    /// encoding: it is recomputed, **written back to `ItemWorkmanship`** (so this getter takes
    /// `&mut self`), and clamped.
    // ACE: WorldObject.Workmanship
    #[allow(clippy::manual_range_contains, clippy::manual_clamp)] // C# comparisons: NaN skips both
    pub fn workmanship(&mut self) -> Option<f32> {
        let item_workmanship = self.item_workmanship()?;

        let num_items_in_material = self
            .get_property(PropertyInt::NumItemsInMaterial)
            .unwrap_or(1);
        let num_items: f32 = num_items_in_material.cs_cast();

        let mut workmanship = CsCast::<f32>::cs_cast(item_workmanship) / num_items;

        // try to recover from previous botched formula...

        // TODO: remove this code after awhile
        if workmanship < 1.0 || workmanship > 10.0 {
            let structure: f32 = self.structure().unwrap_or(1).cs_cast();

            workmanship = CsCast::<f32>::cs_cast(item_workmanship) / 10000.0 / structure;

            self.set_item_workmanship(Some(round(f64::from(workmanship * num_items)).cs_cast()));

            // Math.Clamp(float, float, float)
            workmanship = if workmanship < 1.0 {
                1.0
            } else if workmanship > 10.0 {
                10.0
            } else {
                workmanship
            };
        }

        Some(workmanship)
    }

    // ACE: WorldObject.Workmanship
    pub fn set_workmanship(&mut self, value: Option<f32>) {
        match value {
            Some(value) => {
                let num_items_in_material = self
                    .get_property(PropertyInt::NumItemsInMaterial)
                    .unwrap_or(1);
                let num_items: f32 = num_items_in_material.cs_cast();
                self.set_item_workmanship(Some(round(f64::from(value * num_items)).cs_cast()));
            }
            None => self.set_item_workmanship(None),
        }
    }

    // ACE: WorldObject.WeenieClassId
    /// wcid - stands for weenie class id.
    #[must_use]
    pub fn weenie_class_id(&self) -> u32 {
        self.biota.weenie_class_id
    }

    // ACE: WorldObject.WeenieType
    #[must_use]
    pub fn weenie_type(&self) -> WeenieType {
        self.biota.weenie_type
    }

    // ACE: WorldObject.InitCreate
    #[must_use]
    pub fn init_create(&self) -> i32 {
        self.init_generated_objects()
    }

    // ACE: WorldObject.InitCreate
    pub fn set_init_create(&mut self, value: i32) {
        self.set_init_generated_objects(value);
    }

    // ACE: WorldObject.MaxCreate
    #[must_use]
    pub fn max_create(&self) -> i32 {
        self.max_generated_objects()
    }

    // ACE: WorldObject.MaxCreate
    pub fn set_max_create(&mut self, value: i32) {
        self.set_max_generated_objects(value);
    }

    // ACE: WorldObject.MaterialCode
    /// `(byte?)TsysMutationData`.
    #[must_use]
    pub fn material_code(&self) -> Option<u8> {
        self.tsys_mutation_data().map(|v| v.cs_cast())
    }

    // ACE: WorldObject.GemCode
    /// `(byte?)(TsysMutationData >> 8)`.
    #[must_use]
    pub fn gem_code(&self) -> Option<u8> {
        self.tsys_mutation_data().map(|v| (v >> 8).cs_cast())
    }

    // ACE: WorldObject.ColorCode
    /// `(byte?)(TsysMutationData >> 16)`.
    #[must_use]
    pub fn color_code(&self) -> Option<u8> {
        self.tsys_mutation_data().map(|v| (v >> 16).cs_cast())
    }

    // ACE: WorldObject.SpellSelectionCode
    /// `(byte?)(TsysMutationData >> 24)` (an arithmetic shift of the `int`).
    #[must_use]
    pub fn spell_selection_code(&self) -> Option<u8> {
        self.tsys_mutation_data().map(|v| (v >> 24).cs_cast())
    }

    // ACE: WorldObject.PkLevel
    /// `(PKLevel)PkLevelModifier`.
    #[must_use]
    pub fn pk_level(&self) -> PKLevel {
        PKLevel(self.pk_level_modifier().cs_cast())
    }

    // ACE: WorldObject.PkLevel
    pub fn set_pk_level(&mut self, value: PKLevel) {
        self.set_pk_level_modifier(value.0.cs_cast());
    }
}

// ========================================
// Members that read the physics body, the dats or other objects (world-level)
// ========================================

fn body(w: &World, wo: ObjectGuid) -> Option<&dereth_physics::obj::PhysicsObj> {
    let h = w.objects.get(wo)?.phys?;
    w.physics.get(h)
}

// ACE: WorldObject.Height
/// `PhysicsObj != null ? PhysicsObj.GetHeight() : 0.0f`.
#[must_use]
pub fn height(w: &World, wo: ObjectGuid) -> f32 {
    body(w, wo).map_or(0.0, dereth_physics::obj::PhysicsObj::height)
}

// ACE: WorldObject.Velocity
/// `PhysicsObj?.Velocity ?? Vector3.Zero`.
#[must_use]
pub fn velocity(w: &World, wo: ObjectGuid) -> Vector3 {
    body(w, wo).map_or(Vector3::ZERO, |b| vector3_of_data(b.velocity_vector))
}

// ACE: WorldObject.Acceleration
/// `PhysicsObj?.Acceleration ?? Vector3.Zero`.
#[must_use]
pub fn acceleration(w: &World, wo: ObjectGuid) -> Vector3 {
    body(w, wo).map_or(Vector3::ZERO, |b| vector3_of_data(b.acceleration_vector))
}

// ACE: WorldObject.Omega
/// `PhysicsObj?.Omega ?? Vector3.Zero`.
#[must_use]
pub fn omega(w: &World, wo: ObjectGuid) -> Vector3 {
    body(w, wo).map_or(Vector3::ZERO, |b| vector3_of_data(b.omega_vector))
}

// ACE: WorldObject.CSetup
/// `DatManager.PortalDat.ReadFromDat<SetupModel>(SetupTableId)`; `None` for a file the dats do
/// not have (ACE returns an empty model).
#[must_use]
pub fn c_setup(w: &World, wo: ObjectGuid) -> Option<Arc<SetupModel>> {
    let setup_table_id = w.objects.get(wo)?.setup_table_id();
    w.dats
        .portal_dat()
        .read_from_dat::<SetupModel>(setup_table_id)
}

// ACE: WorldObject.HasMissileFlightPlacement
/// `CSetup.HasMissileFlightPlacement`: whether the setup has a MissileFlight placement frame.
///
/// # Panics
/// Without the setup file (ACE: `NullReferenceException` on its placement frames).
#[must_use]
pub fn has_missile_flight_placement(w: &World, wo: ObjectGuid) -> bool {
    c_setup(w, wo)
        .expect("ACE: CSetup is null (NullReferenceException)")
        .placement_frames
        .contains_key(&Placement::MissileFlight.0)
}

/// The physics-state-backed `bool?` properties (`Static`, `Missile`, ...): the getter reads the
/// body (`GetPhysicsState`, false without one, so never null) and the setter writes it
/// (`SetPhysicsState`).
macro_rules! physics_state_property {
    ($get:ident, $set:ident, $state:ident) => {
        #[must_use]
        pub fn $get(w: &World, wo: ObjectGuid) -> Option<bool> {
            Some(phys_ext::get_physics_state(w, wo, PhysicsState::$state))
        }

        pub fn $set(w: &mut World, wo: ObjectGuid, value: Option<bool>) {
            phys_ext::set_physics_state(w, wo, PhysicsState::$state, value);
        }
    };
}

// ACE: WorldObject.Static
physics_state_property!(static_state, set_static_state, Static);
// ACE: WorldObject.Missile
physics_state_property!(missile, set_missile, Missile);
// ACE: WorldObject.AlignPath
physics_state_property!(align_path, set_align_path, AlignPath);
// ACE: WorldObject.PathClipped
physics_state_property!(path_clipped, set_path_clipped, PathClipped);
// ACE: WorldObject.ParticleEmitter
physics_state_property!(particle_emitter, set_particle_emitter, ParticleEmitter);
// ACE: WorldObject.Hidden
physics_state_property!(hidden, set_hidden, Hidden);
// ACE: WorldObject.Cloaked
physics_state_property!(cloaked, set_cloaked, Cloaked);
// ACE: WorldObject.Sledding
physics_state_property!(sledding, set_sledding, Sledding);

// ACE: WorldObject.Pushable
#[must_use]
pub fn pushable(w: &World, wo: ObjectGuid) -> Option<bool> {
    Some(phys_ext::get_physics_state(w, wo, PhysicsState::Pushable))
}

// ACE: WorldObject.Pushable
// ACE-BUG: the Pushable setter writes PhysicsState.Missile instead of Pushable, so setting
// Pushable toggles the missile flag and leaves Pushable unchanged.
pub fn set_pushable(w: &mut World, wo: ObjectGuid, value: Option<bool>) {
    phys_ext::set_physics_state(w, wo, PhysicsState::Missile, value);
}

// ACE: WorldObject.NameWithMaterial
/// `GetNameWithMaterial()`.
#[must_use]
pub fn name_with_material(w: &World, wo: ObjectGuid) -> String {
    get_name_with_material(w, wo, None)
}

// ACE: WorldObject.GetNameWithMaterial
/// The name prefixed with the material name (`stack_size` other than 1 uses the plural name).
///
/// # Panics
/// With a material and no name (ACE: `NullReferenceException` on `name.Contains`).
#[must_use]
pub fn get_name_with_material(w: &World, wo: ObjectGuid, stack_size: Option<i32>) -> String {
    let name = if stack_size.is_some() && stack_size != Some(1) {
        Some(crate::world_objects::world_object::get_plural_name(w, wo))
    } else {
        crate::dispatch::name::name(w, wo)
    };

    let Some(material_type) = w.objects.get(wo).and_then(WorldObject::material_type) else {
        return name.unwrap_or_default();
    };

    let material = crate::managers::recipe_manager::get_material_name(w, material_type);

    let mut name = name.expect("ACE: name is null (NullReferenceException)");
    if name.contains(material.as_str()) {
        name = name.replace(material.as_str(), "");
    }

    format!("{material} {name}")
}

// ACE: WorldObject.WeenieClassName
/// The cached weenie's class name; `"WeenieClassName_NOT_FOUND"` (with ACE's warning) when the
/// content has no weenie for this wcid. This can happen if a weenie no longer exists in ACE_WORLD,
/// but instances of it still exist in ACE_SHARD.
#[must_use]
pub fn weenie_class_name(
    get_cached_weenie: &dyn Fn(u32) -> Option<Arc<empyrean_entity::Weenie>>,
    weenie_class_id: u32,
) -> String {
    match get_cached_weenie(weenie_class_id) {
        Some(weenie) => weenie.class_name.clone().unwrap_or_default(),
        None => {
            log::warn!(
                "WorldObject.WeenieClassName -- No cached weenie found for WCID {weenie_class_id}"
            );
            "WeenieClassName_NOT_FOUND".to_owned()
        }
    }
}

// ACE: WorldObject.WeenieClassName
/// The setter renames the cached weenie itself.
// DIVERGE: the content cache is immutable and shared (`Arc<dyn WorldDatabase>`), so the rename is
// logged and dropped; ACE has no caller of the setter.
pub fn set_weenie_class_name(
    content: &dyn empyrean_content::WorldDatabase,
    weenie_class_id: u32,
    value: &str,
) {
    if content.get_cached_weenie(weenie_class_id).is_some() {
        log::error!("WorldObject.WeenieClassName setter -- the content cache is read-only; {value} not applied to WCID {weenie_class_id}");
    } else {
        log::error!("WorldObject.WeenieClassName setter -- No cached weenie found for WCID {weenie_class_id}");
    }
}

// ACE: WorldObject.setVisualClothingPriority
/// Function to genreate and set the VisualClothingPriority of the armor piece.
pub fn set_visual_clothing_priority(w: &mut World, wo: ObjectGuid) {
    use empyrean_dat::file_types::clothing_table::{
        ClothingTableExt, DEFAULT_VISUAL_PRIORITY_SETUP,
    };
    use empyrean_dat::file_types::ClothingTable;

    let Some(o) = w.objects.get(wo) else { return };
    let armor_or_extremity = EquipMask(EquipMask::Armor.0 | EquipMask::Extremity.0);
    let Some(clothing_base) = o.clothing_base() else {
        return;
    };
    if o.current_wielded_location()
        .is_some_and(|c| (c & armor_or_extremity).0 != 0)
    {
        let item = w
            .dats
            .portal_dat()
            .read_from_dat::<ClothingTable>(clothing_base);
        let priority = item
            .and_then(|i| i.get_visual_priority(DEFAULT_VISUAL_PRIORITY_SETUP))
            .map(CoverageMask);
        if let Some(o) = w.objects.get_mut(wo) {
            o.set_visual_clothing_priority_prop(priority);
        }
    }
}
