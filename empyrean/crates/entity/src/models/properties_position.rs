// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesPosition.cs
//! `PropertiesPosition`: a stored position: cell id, origin and rotation.

/// ACE: PropertiesPosition. `Clone` copies every field; ACE's own `Clone()` is [`PropertiesPosition::ace_clone`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropertiesPosition {
    // ACE: PropertiesPosition.ObjCellId
    pub obj_cell_id: u32,
    // ACE: PropertiesPosition.PositionX
    pub position_x: f32,
    // ACE: PropertiesPosition.PositionY
    pub position_y: f32,
    // ACE: PropertiesPosition.PositionZ
    pub position_z: f32,
    // ACE: PropertiesPosition.RotationW
    pub rotation_w: f32,
    // ACE: PropertiesPosition.RotationX
    pub rotation_x: f32,
    // ACE: PropertiesPosition.RotationY
    pub rotation_y: f32,
    // ACE: PropertiesPosition.RotationZ
    pub rotation_z: f32,
}

impl PropertiesPosition {
    /// ACE's `Clone()`: a copy of every field.
    // ACE: PropertiesPosition.Clone
    #[must_use]
    pub fn ace_clone(&self) -> PropertiesPosition {
        PropertiesPosition {
            obj_cell_id: self.obj_cell_id,
            position_x: self.position_x,
            position_y: self.position_y,
            position_z: self.position_z,
            rotation_w: self.rotation_w,
            rotation_x: self.rotation_x,
            rotation_y: self.rotation_y,
            rotation_z: self.rotation_z,
        }
    }
}
