// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/BaseDamage.cs
//! Port of `Source/ACE.Server/Entity/BaseDamage.cs`.

/// ACE class `BaseDamage`: a weapon's maximum damage and variance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BaseDamage {
    pub max_damage: i32,
    pub variance: f32,
}

impl BaseDamage {
    // ACE: BaseDamage.BaseDamage
    #[must_use]
    pub fn new(max_damage: i32, variance: f32) -> Self {
        Self {
            max_damage,
            variance,
        }
    }

    // ACE: BaseDamage.MinDamage
    /// `MaxDamage * (1.0f - Variance)`.
    #[must_use]
    pub fn min_damage(&self) -> f32 {
        self.max_damage as f32 * (1.0 - self.variance)
    }
}
