//! Shorthand for building world-database rows by hand (tests, `empyrean-testkit`). Not ported: ACE
//! builds these rows only by reading MySQL. Child-row ids are assigned in insertion order.

use empyrean_entity::enums::{
    PositionType, PropertyBool, PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt,
    PropertyInt64, PropertyString, WeenieType,
};

use crate::models::world::*;

fn next_id(len: usize) -> u32 {
    u32::try_from(len + 1).unwrap_or(u32::MAX)
}

impl Weenie {
    /// A weenie row with no properties.
    #[must_use]
    pub fn new(class_id: u32, class_name: &str, weenie_type: WeenieType) -> Self {
        #[allow(clippy::cast_possible_wrap)]
        let r#type = weenie_type.0 as i32;
        Self {
            class_id,
            class_name: class_name.to_owned(),
            r#type,
            ..Default::default()
        }
    }

    #[must_use]
    pub fn with_bool(mut self, p: PropertyBool, value: bool) -> Self {
        let id = next_id(self.weenie_properties_bool.len());
        self.weenie_properties_bool.push(WeeniePropertiesBool {
            id,
            object_id: self.class_id,
            r#type: p.0,
            value,
        });
        self
    }

    #[must_use]
    pub fn with_did(mut self, p: PropertyDataId, value: u32) -> Self {
        let id = next_id(self.weenie_properties_did.len());
        self.weenie_properties_did.push(WeeniePropertiesDID {
            id,
            object_id: self.class_id,
            r#type: p.0,
            value,
        });
        self
    }

    #[must_use]
    pub fn with_float(mut self, p: PropertyFloat, value: f64) -> Self {
        let id = next_id(self.weenie_properties_float.len());
        self.weenie_properties_float.push(WeeniePropertiesFloat {
            id,
            object_id: self.class_id,
            r#type: p.0,
            value,
        });
        self
    }

    #[must_use]
    pub fn with_iid(mut self, p: PropertyInstanceId, value: u32) -> Self {
        let id = next_id(self.weenie_properties_iid.len());
        self.weenie_properties_iid.push(WeeniePropertiesIID {
            id,
            object_id: self.class_id,
            r#type: p.0,
            value,
        });
        self
    }

    #[must_use]
    pub fn with_int(mut self, p: PropertyInt, value: i32) -> Self {
        let id = next_id(self.weenie_properties_int.len());
        self.weenie_properties_int.push(WeeniePropertiesInt {
            id,
            object_id: self.class_id,
            r#type: p.0,
            value,
        });
        self
    }

    #[must_use]
    pub fn with_int64(mut self, p: PropertyInt64, value: i64) -> Self {
        let id = next_id(self.weenie_properties_int64.len());
        self.weenie_properties_int64.push(WeeniePropertiesInt64 {
            id,
            object_id: self.class_id,
            r#type: p.0,
            value,
        });
        self
    }

    #[must_use]
    pub fn with_string(mut self, p: PropertyString, value: &str) -> Self {
        let id = next_id(self.weenie_properties_string.len());
        self.weenie_properties_string.push(WeeniePropertiesString {
            id,
            object_id: self.class_id,
            r#type: p.0,
            value: value.to_owned(),
        });
        self
    }

    /// A position property: cell, origin `(x, y, z)` and rotation `(w, x, y, z)`.
    #[must_use]
    pub fn with_position(
        mut self,
        p: PositionType,
        obj_cell_id: u32,
        origin: [f32; 3],
        angles_wxyz: [f32; 4],
    ) -> Self {
        let id = next_id(self.weenie_properties_position.len());
        self.weenie_properties_position
            .push(WeeniePropertiesPosition {
                id,
                object_id: self.class_id,
                position_type: p.0,
                obj_cell_id,
                origin_x: origin[0],
                origin_y: origin[1],
                origin_z: origin[2],
                angles_w: angles_wxyz[0],
                angles_x: angles_wxyz[1],
                angles_y: angles_wxyz[2],
                angles_z: angles_wxyz[3],
            });
        self
    }
}

impl LandblockInstance {
    /// An instance at `obj_cell_id`, facing north (identity rotation), not a link child.
    #[must_use]
    pub fn new(guid: u32, weenie_class_id: u32, obj_cell_id: u32, origin: [f32; 3]) -> Self {
        Self {
            guid,
            weenie_class_id,
            obj_cell_id,
            origin_x: origin[0],
            origin_y: origin[1],
            origin_z: origin[2],
            angles_w: 1.0,
            ..Default::default()
        }
    }
}
