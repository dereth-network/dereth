//! World streaming and rendering over shared simulation state.
//!
//! The scene owns device resources, baked geometry and palettes, particle emitters,
//! draw ordering, lighting and the view of the active landblock window. Pure world
//! rules live in `dereth-world-render`; device commands live in `dereth-render`.
//! Cohesive child modules share the scene records without changing resource ownership.

#[cfg(gpu)]
pub use imp::*;

#[cfg(gpu)]
#[path = "world_scene/imp.rs"]
mod imp;
