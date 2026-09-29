// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Adjacency.cs
//! Port of `Source/ACE.Server/Entity/Adjacency.cs`.

/// ACE enum `Adjacency` (declared in ACE.Server, so not generated), underlying `int`. A newtype so
/// that, as in C#, any `int` value is representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct Adjacency(pub i32);

#[allow(non_upper_case_globals)]
impl Adjacency {
    pub const NorthWest: Self = Self(0);
    pub const North: Self = Self(1);
    pub const NorthEast: Self = Self(2);
    pub const East: Self = Self(3);
    pub const SouthEast: Self = Self(4);
    pub const South: Self = Self(5);
    pub const SouthWest: Self = Self(6);
    pub const West: Self = Self(7);
}

// ACE: AdjacencyHelper.GetInverse
/// The opposite direction; `None` for a value outside the enum.
#[must_use]
pub fn get_inverse(adj: Adjacency) -> Option<Adjacency> {
    match adj {
        Adjacency::NorthWest => Some(Adjacency::SouthEast),
        Adjacency::North => Some(Adjacency::South),
        Adjacency::NorthEast => Some(Adjacency::SouthWest),
        Adjacency::East => Some(Adjacency::West),
        Adjacency::SouthEast => Some(Adjacency::NorthWest),
        Adjacency::South => Some(Adjacency::North),
        Adjacency::SouthWest => Some(Adjacency::NorthEast),
        Adjacency::West => Some(Adjacency::East),
        _ => None,
    }
}
