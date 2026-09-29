// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesGenerator.cs
//! `WeeniePropertiesGenerator`: one row of the world-database table `weenie_properties_generator`.

/// Generator Properties of Weenies
// ACE: WeeniePropertiesGenerator
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesGenerator {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    pub probability: f32,
    /// Weenie Class Id of object to generate
    pub weenie_class_id: u32,
    /// Amount of delay before generation
    pub delay: Option<f32>,
    /// Number of object to generate initially
    pub init_create: i32,
    /// Maximum amount of objects to generate
    pub max_create: i32,
    /// When to generate the weenie object
    pub when_create: u32,
    /// Where to generate the weenie object
    pub where_create: u32,
    /// StackSize of object generated
    pub stack_size: Option<i32>,
    /// Palette Color of Object Generated
    pub palette_id: Option<u32>,
    /// Shade of Object generated&apos;s Palette
    pub shade: Option<f32>,
    pub obj_cell_id: Option<u32>,
    pub origin_x: Option<f32>,
    pub origin_y: Option<f32>,
    pub origin_z: Option<f32>,
    pub angles_w: Option<f32>,
    pub angles_x: Option<f32>,
    pub angles_y: Option<f32>,
    pub angles_z: Option<f32>,
    // ACE: WeeniePropertiesGenerator.Object navigates back to the parent row, not carried.
}
