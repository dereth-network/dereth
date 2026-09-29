// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/CreateProfile.cs
//! `CreateProfile`: one entry of a create list (fields only).

/// ACE: CreateProfile
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CreateProfile {
    // ACE: CreateProfile.ClassID
    pub class_id: u32,
    // ACE: CreateProfile.Bound
    pub bound: bool,
    // ACE: CreateProfile.PaletteID
    pub palette_id: u32,
    // ACE: CreateProfile.Shade
    pub shade: f32,
    // ACE: CreateProfile.Probability
    pub probability: f32,
    // ACE: CreateProfile.Destination
    pub destination: i32,
    // ACE: CreateProfile.Regen
    pub regen: i32,
    // ACE: CreateProfile.StackSize
    pub stack_size: u64,
    // ACE: CreateProfile.MaxNum
    pub max_num: u64,
    // ACE: CreateProfile.Amount
    pub amount: u64,
}
