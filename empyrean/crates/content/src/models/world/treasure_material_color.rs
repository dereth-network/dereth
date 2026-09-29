// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/TreasureMaterialColor.cs
//! `TreasureMaterialColor`: one row of the world-database table `treasure_material_color`.

// ACE: TreasureMaterialColor
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TreasureMaterialColor {
    pub id: u32,
    pub material_id: u32,
    pub color_code: u32,
    pub palette_template: u32,
    pub probability: f32,
}
