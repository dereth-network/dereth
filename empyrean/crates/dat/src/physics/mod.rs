//! What the shared `PhysicsWorld` needs from the dats: a [`LandSource`] over the cell dat,
//! setup collision geometry, and a flat-terrain source for tests.
//!
//! [`LandSource`]: dereth_physics::source::LandSource

pub mod convert;
pub mod flat;
pub mod land_source;
pub mod setup;

pub use flat::{flat_land_source, linear_height_table};
pub use land_source::{DatLandSource, LandSourceError};
pub use setup::{load_setup_geometry, SetupGeometryCache};
