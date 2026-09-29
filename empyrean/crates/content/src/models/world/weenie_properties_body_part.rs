// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesBodyPart.cs
//! `WeeniePropertiesBodyPart`: one row of the world-database table `weenie_properties_body_part`.

/// Body Part Properties of Weenies
// ACE: WeeniePropertiesBodyPart
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesBodyPart {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    /// Type of Property the value applies to (PropertySkill.????)
    pub key: u16,
    pub d_type: i32,
    pub d_val: i32,
    pub d_var: f32,
    pub base_armor: i32,
    pub armor_vs_slash: i32,
    pub armor_vs_pierce: i32,
    pub armor_vs_bludgeon: i32,
    pub armor_vs_cold: i32,
    pub armor_vs_fire: i32,
    pub armor_vs_acid: i32,
    pub armor_vs_electric: i32,
    pub armor_vs_nether: i32,
    pub bh: i32,
    pub hlf: f32,
    pub mlf: f32,
    pub llf: f32,
    pub hrf: f32,
    pub mrf: f32,
    pub lrf: f32,
    pub hlb: f32,
    pub mlb: f32,
    pub llb: f32,
    pub hrb: f32,
    pub mrb: f32,
    pub lrb: f32,
    // ACE: WeeniePropertiesBodyPart.Object navigates back to the parent row, not carried.
}
