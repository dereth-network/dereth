// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Range.cs
//! Port of `Source/ACE.Server/Entity/Range.cs`.

use std::fmt;

use empyrean_common::dotnet;

/// ACE class `Range`: a float range and its average.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Range {
    pub min: f32,
    pub max: f32,
    pub avg: f32,
}

impl Range {
    // ACE: Range.Range
    /// `new Range(min, max)`; the parameterless constructor is [`Range::default`].
    #[must_use]
    pub fn new(min: f32, max: f32) -> Self {
        Self {
            min,
            max,
            avg: (min + max) / 2.0,
        }
    }
}

// ACE: Range.ToString
/// `$"{Min} - {Max}"`, each float in its default (`en-US`) format.
impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} - {}",
            dotnet::to_string(self.min),
            dotnet::to_string(self.max)
        )
    }
}
