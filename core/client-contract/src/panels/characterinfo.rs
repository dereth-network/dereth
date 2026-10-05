//! Property identifiers shared by the character information view and its producers.

/// The quality ids the character page reads that are not already fields of `CharacterInfo`.
/// Every one is read off the literal the page passes to the quality lookup.
pub mod prop {
    /// `0x62` — the creation timestamp, read by the birth/age/deaths row.
    pub const CREATION_TIMESTAMP: u32 = 98;
    /// `0x7D` — the age, and the one id the page registers a live quality handler for.
    pub const AGE: u32 = 125;
    /// `0x162` — the melee mastery, read by the augmentations row.
    pub const WEAPON_MASTERY: u32 = 354;
    /// `0x163` — the missile mastery, read beside it.
    pub const MISSILE_MASTERY: u32 = 355;
    /// `0x16A` — the summoning mastery, read beside it.
    pub const SUMMONING_MASTERY: u32 = 362;
    /// `0x186` — the enlightenment count, read by the birth/age/deaths row after the deaths line.
    pub const ENLIGHTENMENT: u32 = 390;
}
