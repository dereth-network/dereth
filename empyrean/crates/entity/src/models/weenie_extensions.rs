// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/WeenieExtensions.cs
//! `WeenieExtensions`: typed property reads and item helpers on a [`Weenie`], as inherent
//! methods. As on [`Biota`](crate::models::Biota), one generic `get_property` replaces ACE's
//! overload set.

use empyrean_common::extensions::string_extensions::pluralize;

use crate::enums::{ItemType, PositionType, PropertyBool, PropertyInt, PropertyString, WeenieType};
use crate::models::i_weenie::PropertyKey;
use crate::models::{PropertiesPosition, Weenie};
use crate::position::Position;

impl Weenie {
    // =====================================
    // Get
    // Bool, DID, Float, IID, Int, Int64, String, Position
    // =====================================

    /// The value of `property`, or `None` if the bag is null or lacks it.
    // ACE: WeenieExtensions.GetProperty
    #[must_use]
    pub fn get_property<K: PropertyKey>(&self, property: K) -> Option<K::Value> {
        let bag = K::bag(self).as_ref()?;

        bag.get(&property).cloned()
    }

    /// The stored record for position `property`.
    // ACE: WeenieExtensions.GetProperty
    #[must_use]
    pub fn get_property_position(&self, property: PositionType) -> Option<&PropertiesPosition> {
        let bag = self.properties_position.as_ref()?;

        bag.get(&property)
    }

    /// Position `property` as a [`Position`]. Unlike the biota version this never builds a
    /// relative position, not even for `RelativeDestination`.
    // ACE: WeenieExtensions.GetPosition
    #[must_use]
    pub fn get_position(&self, property: PositionType) -> Option<Position> {
        let bag = self.properties_position.as_ref()?;

        let value = bag.get(&property)?;
        Some(Position::from_components(
            value.obj_cell_id,
            value.position_x,
            value.position_y,
            value.position_z,
            value.rotation_x,
            value.rotation_y,
            value.rotation_z,
            value.rotation_w,
            false,
        ))
    }

    // =====================================
    // Utility
    // =====================================

    // ACE: WeenieExtensions.GetName
    #[must_use]
    pub fn get_name(&self) -> Option<String> {
        self.get_property(PropertyString::Name)
    }

    /// `PluralName`, or the pluralized `Name`.
    ///
    /// # Panics
    /// ACE-BUG: with neither `PluralName` nor `Name`, ACE calls `Pluralize()` on a null string
    /// and throws a `NullReferenceException`; so does this.
    // ACE: WeenieExtensions.GetPluralName
    #[must_use]
    pub fn get_plural_name(&self) -> String {
        let mut plural_name = self.get_property(PropertyString::PluralName);

        if plural_name.is_none() {
            let name = self
                .get_property(PropertyString::Name)
                .expect("NullReferenceException: Pluralize() on a null Name");
            plural_name = Some(pluralize(&name));
        }

        plural_name.unwrap_or_default()
    }

    // ACE: WeenieExtensions.GetItemType
    #[must_use]
    #[allow(clippy::cast_sign_loss)]
    pub fn get_item_type(&self) -> ItemType {
        let item_type = self.get_property(PropertyInt::ItemType).unwrap_or(0);

        ItemType(item_type as u32)
    }

    // ACE: WeenieExtensions.GetValue
    #[must_use]
    pub fn get_value(&self) -> Option<i32> {
        self.get_property(PropertyInt::Value)
    }

    // ACE: WeenieExtensions.IsStackable
    #[must_use]
    pub fn is_stackable(&self) -> bool {
        matches!(
            self.weenie_type,
            WeenieType::Stackable
                | WeenieType::Ammunition
                | WeenieType::Coin
                | WeenieType::CraftTool
                | WeenieType::Food
                | WeenieType::Gem
                | WeenieType::Missile
                | WeenieType::SpellComponent
        )
    }

    // ACE: WeenieExtensions.IsStuck
    #[must_use]
    pub fn is_stuck(&self) -> bool {
        self.get_property(PropertyBool::Stuck).unwrap_or(false)
    }

    // ACE: WeenieExtensions.RequiresBackpackSlotOrIsContainer
    #[must_use]
    pub fn requires_backpack_slot_or_is_container(&self) -> bool {
        let requires_back_pack_slot = self
            .get_property(PropertyBool::RequiresBackpackSlot)
            .unwrap_or(false);

        requires_back_pack_slot || self.weenie_type == WeenieType::Container
    }

    // ACE: WeenieExtensions.IsVendorService
    #[must_use]
    pub fn is_vendor_service(&self) -> bool {
        self.get_property(PropertyBool::VendorService)
            .unwrap_or(false)
    }

    /// `StackUnitEncumbrance` for a stackable weenie that has it, else `EncumbranceVal` (0 if
    /// absent).
    // ACE: WeenieExtensions.GetStackUnitEncumbrance
    #[must_use]
    pub fn get_stack_unit_encumbrance(&self) -> i32 {
        if self.is_stackable() {
            let stack_unit_encumbrance = self.get_property(PropertyInt::StackUnitEncumbrance);

            if let Some(v) = stack_unit_encumbrance {
                return v;
            }
        }
        self.get_property(PropertyInt::EncumbranceVal).unwrap_or(0)
    }

    /// `MaxStackSize` for a stackable weenie that has it, else 1.
    // ACE: WeenieExtensions.GetMaxStackSize
    #[must_use]
    pub fn get_max_stack_size(&self) -> i32 {
        if self.is_stackable() {
            let max_stack_size = self.get_property(PropertyInt::MaxStackSize);

            if let Some(v) = max_stack_size {
                return v;
            }
        }
        1
    }

    // ACE: WeenieExtensions.GetMaxStructure
    #[must_use]
    pub fn get_max_structure(&self) -> Option<i32> {
        self.get_property(PropertyInt::MaxStructure)
    }
}
