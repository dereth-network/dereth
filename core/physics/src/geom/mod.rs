//! Geometry primitives: spheres, cylinder-spheres, planes, polygons and BSP trees.
//!
//! Everything in here is **pure**: it takes geometry and returns a number or
//! a boolean. Transition-aware sphere, cylinder-sphere and BSP collision dispatchers
//! need the full transition state and live in [`crate::transition::collide`]; this
//! split keeps the geometry module testable without a world.
//!
//! Two epsilon conventions live here and they have **opposite signs**:
//! overlap tests use `r1 + r2 - 0.0002` and swept tests use `+ 0.0002`. Flipping one makes objects
//! either interpenetrate or refuse to touch.

pub mod bbox;
pub mod bsp;
pub mod plane;
pub mod polygon;
pub mod sphere;

pub use bbox::{BBox, BBoxExt};
pub use bsp::{BspNode, BspNodeKind, BspTree};
pub use plane::{Plane, PlaneExt, Sidedness};
pub use polygon::Polygon;
pub use sphere::{CylSphere, CylSphereExt, Ray, Sphere, SphereExt};

/// The result of the cell-BSP volume tests: the shared outside / straddling / inside answer.
pub use dereth_primitives::shape::Bounding;
