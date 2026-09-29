// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesGenerator.cs
//! `PropertiesGenerator`: one generator-profile entry.

use crate::enums::{RegenLocationType, RegenerationType};

/// ACE: PropertiesGenerator. `Clone` copies every field; ACE's own `Clone()` is [`PropertiesGenerator::ace_clone`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropertiesGenerator {
    /// Only used to tie this record back to a specific database row.
    // ACE: PropertiesGenerator.DatabaseRecordId
    pub database_record_id: u32,
    // ACE: PropertiesGenerator.Probability
    pub probability: f32,
    // ACE: PropertiesGenerator.WeenieClassId
    pub weenie_class_id: u32,
    // ACE: PropertiesGenerator.Delay
    pub delay: Option<f32>,
    // ACE: PropertiesGenerator.InitCreate
    pub init_create: i32,
    // ACE: PropertiesGenerator.MaxCreate
    pub max_create: i32,
    // ACE: PropertiesGenerator.WhenCreate
    pub when_create: RegenerationType,
    // ACE: PropertiesGenerator.WhereCreate
    pub where_create: RegenLocationType,
    // ACE: PropertiesGenerator.StackSize
    pub stack_size: Option<i32>,
    // ACE: PropertiesGenerator.PaletteId
    pub palette_id: Option<u32>,
    // ACE: PropertiesGenerator.Shade
    pub shade: Option<f32>,
    // ACE: PropertiesGenerator.ObjCellId
    pub obj_cell_id: Option<u32>,
    // ACE: PropertiesGenerator.OriginX
    pub origin_x: Option<f32>,
    // ACE: PropertiesGenerator.OriginY
    pub origin_y: Option<f32>,
    // ACE: PropertiesGenerator.OriginZ
    pub origin_z: Option<f32>,
    // ACE: PropertiesGenerator.AnglesW
    pub angles_w: Option<f32>,
    // ACE: PropertiesGenerator.AnglesX
    pub angles_x: Option<f32>,
    // ACE: PropertiesGenerator.AnglesY
    pub angles_y: Option<f32>,
    // ACE: PropertiesGenerator.AnglesZ
    pub angles_z: Option<f32>,
}

impl PropertiesGenerator {
    /// ACE's `Clone()`: a copy with `DatabaseRecordId` reset to 0.
    // ACE: PropertiesGenerator.Clone
    #[must_use]
    pub fn ace_clone(&self) -> PropertiesGenerator {
        PropertiesGenerator {
            probability: self.probability,
            weenie_class_id: self.weenie_class_id,
            delay: self.delay,
            init_create: self.init_create,
            max_create: self.max_create,
            when_create: self.when_create,
            where_create: self.where_create,
            stack_size: self.stack_size,
            palette_id: self.palette_id,
            shade: self.shade,
            obj_cell_id: self.obj_cell_id,
            origin_x: self.origin_x,
            origin_y: self.origin_y,
            origin_z: self.origin_z,
            angles_w: self.angles_w,
            angles_x: self.angles_x,
            angles_y: self.angles_y,
            angles_z: self.angles_z,
            database_record_id: 0,
        }
    }
}
