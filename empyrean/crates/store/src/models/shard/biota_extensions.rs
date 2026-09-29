// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaExtensions.cs
//! `BiotaExtensions` over the database row model [`Biota`]: property get, set and remove on the
//! child row lists (`FirstOrDefault` by type, append when absent).
//!
//! C# overloads on the property enum become one method per bag (`get_property_bool`, ...). The
//! removed row that ACE returns through `out` is returned in the `Option`.

use empyrean_entity::enums::{
    PositionType, PropertyBool, PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt,
    PropertyInt64, PropertyString,
};
use empyrean_entity::Position;

use super::{
    Biota, BiotaPropertiesBool, BiotaPropertiesDID, BiotaPropertiesFloat, BiotaPropertiesIID,
    BiotaPropertiesInt, BiotaPropertiesInt64, BiotaPropertiesPosition, BiotaPropertiesSkill,
    BiotaPropertiesString,
};

// ACE: BiotaExtensions
impl Biota {
    // =====================================
    // Get
    // Bool, DID, Float, IID, Int, Int64, String, Position
    // =====================================

    // ACE: BiotaExtensions.GetProperty
    #[must_use]
    pub fn get_property_bool(&self, property: PropertyBool) -> Option<bool> {
        self.biota_properties_bool
            .iter()
            .find(|x| x.r#type == property.0)
            .map(|x| x.value)
    }

    // ACE: BiotaExtensions.GetProperty
    #[must_use]
    pub fn get_property_did(&self, property: PropertyDataId) -> Option<u32> {
        self.biota_properties_did
            .iter()
            .find(|x| x.r#type == property.0)
            .map(|x| x.value)
    }

    // ACE: BiotaExtensions.GetProperty
    #[must_use]
    pub fn get_property_float(&self, property: PropertyFloat) -> Option<f64> {
        self.biota_properties_float
            .iter()
            .find(|x| x.r#type == property.0)
            .map(|x| x.value)
    }

    // ACE: BiotaExtensions.GetProperty
    #[must_use]
    pub fn get_property_iid(&self, property: PropertyInstanceId) -> Option<u32> {
        self.biota_properties_iid
            .iter()
            .find(|x| x.r#type == property.0)
            .map(|x| x.value)
    }

    // ACE: BiotaExtensions.GetProperty
    #[must_use]
    pub fn get_property_int(&self, property: PropertyInt) -> Option<i32> {
        self.biota_properties_int
            .iter()
            .find(|x| x.r#type == property.0)
            .map(|x| x.value)
    }

    // ACE: BiotaExtensions.GetProperty
    #[must_use]
    pub fn get_property_int64(&self, property: PropertyInt64) -> Option<i64> {
        self.biota_properties_int64
            .iter()
            .find(|x| x.r#type == property.0)
            .map(|x| x.value)
    }

    // ACE: BiotaExtensions.GetProperty
    #[must_use]
    pub fn get_property_string(&self, property: PropertyString) -> Option<&str> {
        self.biota_properties_string
            .iter()
            .find(|x| x.r#type == property.0)
            .map(|x| x.value.as_str())
    }

    // ACE: BiotaExtensions.GetProperty
    /// The position row itself (`GetProperty(PositionType)`).
    #[must_use]
    pub fn get_property_position(
        &self,
        position_type: PositionType,
    ) -> Option<&BiotaPropertiesPosition> {
        self.biota_properties_position
            .iter()
            .find(|x| x.position_type == position_type.0)
    }

    // ACE: BiotaExtensions.GetPosition
    #[must_use]
    pub fn get_position(&self, position_type: PositionType) -> Option<Position> {
        let result = self
            .biota_properties_position
            .iter()
            .find(|x| x.position_type == position_type.0)?;

        Some(Position::from_components(
            result.obj_cell_id,
            result.origin_x,
            result.origin_y,
            result.origin_z,
            result.angles_x,
            result.angles_y,
            result.angles_z,
            result.angles_w,
            false,
        ))
    }

    // =====================================
    // Set
    // Bool, DID, Float, IID, Int, Int64, String, Position
    // =====================================

    // ACE: BiotaExtensions.SetProperty
    pub fn set_property_bool(&mut self, property: PropertyBool, value: bool) {
        if let Some(result) = self
            .biota_properties_bool
            .iter_mut()
            .find(|x| x.r#type == property.0)
        {
            result.value = value;
        } else {
            let entity = BiotaPropertiesBool {
                object_id: self.id,
                r#type: property.0,
                value,
            };
            self.biota_properties_bool.push(entity);
        }
    }

    // ACE: BiotaExtensions.SetProperty
    pub fn set_property_did(&mut self, property: PropertyDataId, value: u32) {
        if let Some(result) = self
            .biota_properties_did
            .iter_mut()
            .find(|x| x.r#type == property.0)
        {
            result.value = value;
        } else {
            let entity = BiotaPropertiesDID {
                object_id: self.id,
                r#type: property.0,
                value,
            };
            self.biota_properties_did.push(entity);
        }
    }

    // ACE: BiotaExtensions.SetProperty
    pub fn set_property_float(&mut self, property: PropertyFloat, value: f64) {
        if let Some(result) = self
            .biota_properties_float
            .iter_mut()
            .find(|x| x.r#type == property.0)
        {
            result.value = value;
        } else {
            let entity = BiotaPropertiesFloat {
                object_id: self.id,
                r#type: property.0,
                value,
            };
            self.biota_properties_float.push(entity);
        }
    }

    // ACE: BiotaExtensions.SetProperty
    pub fn set_property_iid(&mut self, property: PropertyInstanceId, value: u32) {
        if let Some(result) = self
            .biota_properties_iid
            .iter_mut()
            .find(|x| x.r#type == property.0)
        {
            result.value = value;
        } else {
            let entity = BiotaPropertiesIID {
                object_id: self.id,
                r#type: property.0,
                value,
            };
            self.biota_properties_iid.push(entity);
        }
    }

    // ACE: BiotaExtensions.SetProperty
    pub fn set_property_int(&mut self, property: PropertyInt, value: i32) {
        if let Some(result) = self
            .biota_properties_int
            .iter_mut()
            .find(|x| x.r#type == property.0)
        {
            result.value = value;
        } else {
            let entity = BiotaPropertiesInt {
                object_id: self.id,
                r#type: property.0,
                value,
            };
            self.biota_properties_int.push(entity);
        }
    }

    // ACE: BiotaExtensions.SetProperty
    pub fn set_property_int64(&mut self, property: PropertyInt64, value: i64) {
        if let Some(result) = self
            .biota_properties_int64
            .iter_mut()
            .find(|x| x.r#type == property.0)
        {
            result.value = value;
        } else {
            let entity = BiotaPropertiesInt64 {
                object_id: self.id,
                r#type: property.0,
                value,
            };
            self.biota_properties_int64.push(entity);
        }
    }

    // ACE: BiotaExtensions.SetProperty
    pub fn set_property_string(&mut self, property: PropertyString, value: &str) {
        if let Some(result) = self
            .biota_properties_string
            .iter_mut()
            .find(|x| x.r#type == property.0)
        {
            value.clone_into(&mut result.value);
        } else {
            let entity = BiotaPropertiesString {
                object_id: self.id,
                r#type: property.0,
                value: value.to_owned(),
            };
            self.biota_properties_string.push(entity);
        }
    }

    // ACE: BiotaExtensions.SetPosition
    pub fn set_position(&mut self, position_type: PositionType, position: &Position) {
        if let Some(result) = self
            .biota_properties_position
            .iter_mut()
            .find(|x| x.position_type == position_type.0)
        {
            result.obj_cell_id = position.cell();
            result.origin_x = position.position_x;
            result.origin_y = position.position_y;
            result.origin_z = position.position_z;
            result.angles_w = position.rotation_w;
            result.angles_x = position.rotation_x;
            result.angles_y = position.rotation_y;
            result.angles_z = position.rotation_z;
        } else {
            let entity = BiotaPropertiesPosition {
                object_id: self.id,
                position_type: position_type.0,
                obj_cell_id: position.cell(),
                origin_x: position.position_x,
                origin_y: position.position_y,
                origin_z: position.position_z,
                angles_w: position.rotation_w,
                angles_x: position.rotation_x,
                angles_y: position.rotation_y,
                angles_z: position.rotation_z,
            };
            self.biota_properties_position.push(entity);
        }
    }

    // =====================================
    // Remove
    // Bool, DID, Float, IID, Int, Int64, String, Position
    // =====================================

    // ACE: BiotaExtensions.TryRemoveProperty
    pub fn try_remove_property_bool(
        &mut self,
        property: PropertyBool,
    ) -> Option<BiotaPropertiesBool> {
        let i = self
            .biota_properties_bool
            .iter()
            .position(|x| x.r#type == property.0)?;
        Some(self.biota_properties_bool.remove(i))
    }

    // ACE: BiotaExtensions.TryRemoveProperty
    pub fn try_remove_property_did(
        &mut self,
        property: PropertyDataId,
    ) -> Option<BiotaPropertiesDID> {
        let i = self
            .biota_properties_did
            .iter()
            .position(|x| x.r#type == property.0)?;
        Some(self.biota_properties_did.remove(i))
    }

    // ACE: BiotaExtensions.TryRemoveProperty
    pub fn try_remove_property_float(
        &mut self,
        property: PropertyFloat,
    ) -> Option<BiotaPropertiesFloat> {
        let i = self
            .biota_properties_float
            .iter()
            .position(|x| x.r#type == property.0)?;
        Some(self.biota_properties_float.remove(i))
    }

    // ACE: BiotaExtensions.TryRemoveProperty
    pub fn try_remove_property_iid(
        &mut self,
        property: PropertyInstanceId,
    ) -> Option<BiotaPropertiesIID> {
        let i = self
            .biota_properties_iid
            .iter()
            .position(|x| x.r#type == property.0)?;
        Some(self.biota_properties_iid.remove(i))
    }

    // ACE: BiotaExtensions.TryRemoveProperty
    pub fn try_remove_property_int(&mut self, property: PropertyInt) -> Option<BiotaPropertiesInt> {
        let i = self
            .biota_properties_int
            .iter()
            .position(|x| x.r#type == property.0)?;
        Some(self.biota_properties_int.remove(i))
    }

    // ACE: BiotaExtensions.TryRemoveProperty
    pub fn try_remove_property_int64(
        &mut self,
        property: PropertyInt64,
    ) -> Option<BiotaPropertiesInt64> {
        let i = self
            .biota_properties_int64
            .iter()
            .position(|x| x.r#type == property.0)?;
        Some(self.biota_properties_int64.remove(i))
    }

    // ACE: BiotaExtensions.TryRemoveProperty
    pub fn try_remove_property_string(
        &mut self,
        property: PropertyString,
    ) -> Option<BiotaPropertiesString> {
        let i = self
            .biota_properties_string
            .iter()
            .position(|x| x.r#type == property.0)?;
        Some(self.biota_properties_string.remove(i))
    }

    // ACE: BiotaExtensions.TryRemovePosition
    pub fn try_remove_position(
        &mut self,
        position_type: PositionType,
    ) -> Option<BiotaPropertiesPosition> {
        let i = self
            .biota_properties_position
            .iter()
            .position(|x| x.position_type == position_type.0)?;
        Some(self.biota_properties_position.remove(i))
    }

    // =====================================
    // BiotaPropertiesSkill
    // =====================================

    // ACE: BiotaExtensions.GetSkill
    #[must_use]
    pub fn get_skill(&self, r#type: u16) -> Option<&BiotaPropertiesSkill> {
        self.biota_properties_skill
            .iter()
            .find(|x| x.r#type == r#type)
    }

    // ACE: BiotaExtensions.GetOrAddSkill
    /// The skill row and whether it was added (ACE's `out bool skillAdded`).
    pub fn get_or_add_skill(&mut self, r#type: u16) -> (&mut BiotaPropertiesSkill, bool) {
        if let Some(i) = self
            .biota_properties_skill
            .iter()
            .position(|x| x.r#type == r#type)
        {
            return (&mut self.biota_properties_skill[i], false);
        }

        let entity = BiotaPropertiesSkill {
            object_id: self.id,
            r#type,
            ..Default::default()
        };
        self.biota_properties_skill.push(entity);
        let last = self.biota_properties_skill.len() - 1;
        (&mut self.biota_properties_skill[last], true)
    }
}
