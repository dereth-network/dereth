//! The landscape: mesh construction, texturing, LOD, lighting, water, ordering and emission.
//!
//! Everything but emission is the landscape's own geometry, kept in `dereth-terrain` and
//! re-exported here at its old paths; emission is this crate's, because it draws.

pub mod emit;
pub use dereth_terrain::land::{lighting, merge, mesh, order, palshift, stitch, water};

#[cfg(test)]
pub(crate) use dereth_terrain::land::tests_support;
