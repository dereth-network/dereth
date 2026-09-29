// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/SWVertex.cs
//! Port of `Source/ACE.Entity/SWVertex.cs`: a runtime-definable copy of DatLoader.SWVertex.

use crate::numerics::Vector3;

// ACE: SWVertex
/// A runtime-definable copy of DatLoader.SWVertex (texture coordinates excluded for server).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SWVertex {
    // ACE: SWVertex.Origin
    pub origin: Vector3,
    // ACE: SWVertex.Normal
    pub normal: Vector3,
}

impl SWVertex {
    // ACE: SWVertex.SWVertex
    #[must_use]
    pub const fn new(origin: Vector3, normal: Vector3) -> Self {
        Self { origin, normal }
    }
}
