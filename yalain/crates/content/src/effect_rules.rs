//! What the world database says of effects where their own data says nothing: what a ward keeps
//! off, the share of its holder's vitae an effect takes or the vitae it gives back, and what an
//! immunity leaves as it ends.

use std::collections::BTreeMap;

/// What a ward keeps off its holder: effects of these classes, and these effects by their saved
/// states, each by its chance.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ward {
    pub classes: Vec<u32>,
    pub effects: Vec<u32>,
    /// From 0 to 1, at any spellcraft.
    pub chance: f32,
}

/// Vitae an effect gives back when it is put on: points of it, never above full, to its player and,
/// where it reaches them, the player's fellows in the world.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct VitaeRestore {
    /// Above 0 and at most 100.
    pub points: f32,
    pub fellows: bool,
}

/// The world's effect rules, by the effect's saved state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectRules {
    pub wards: BTreeMap<u32, Ward>,
    /// The share of its player's vitae an effect takes when it is put on, above 0 and at most 1.
    pub vitae_taps: BTreeMap<u32, f32>,
    /// The vitae an effect gives back when it is put on.
    pub vitae_restores: BTreeMap<u32, VitaeRestore>,
    /// The effects that are an immunity under way: hidden from their holder, each leaves this
    /// effect as it ends (the ward that keeps its kind off).
    pub immunities: BTreeMap<u32, u32>,
}
