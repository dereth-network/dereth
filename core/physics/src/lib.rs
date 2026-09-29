//! The swept-sphere physics engine: cells, BSP trees, collision and the object step.
//!
//! **Depends on** `dereth-primitives` (its tests also read the retail data through `dereth-dat` and
//! `dereth-assets`). **Used by** the shared world adapters (`dereth-world-data`), the client
//! runtime and the SDK, the client and its test kit, and the server (`empyrean-dat`,
//! `empyrean-world`, `empyrean-command`, `empyrean-testkit`).
//!
//! **Must never** decode the data files or drive animation: its inputs arrive as plain shapes from
//! `dereth-world-data`, and animation is reached only through the
//! [`MotionSource`] seam.
//!
//! Objects modelled by one or two spheres (or several cylinder-spheres) are stepped through a graph
//! of cells. There is no rigid-body solver: collisions project the requested offset onto contact
//! planes, slide along surfaces, and step up or down. Four things are load-bearing and easy to
//! "improve" by accident:
//!
//! 1. The 30 Hz gate, the 0.2 s sub-step ladder and the remainder test that lives **inside** the
//!    `dt > MAX_QUANTUM` branch. See [`step`].
//! 2. [`globals::FLOOR_Z`] is baked, not recomputed from a cosine.
//! 3. The sledding friction branch fires when the contact plane's `N.z` is **below** `cos(10
//!    degrees)`. ACE has both the direction and the constant wrong.
//! 4. The transition pool is exactly ten deep and the recursion is re-entrant, which is why every
//!    object is addressed by a handle and never by a borrow held across a call.
//!
//! **Specified in** `docs/formats/10-gfxobj.md` (the physics BSP trees), `docs/formats/11-setup.md`
//! (collision volumes) and `docs/formats/16-environment.md` (interior cell shapes).

#![doc(html_no_source)]

pub mod arena;
pub mod cell;
pub mod detect;
pub mod geom;
pub mod globals;
pub mod land;
pub mod landdefs;
pub mod longhash;
pub mod math;
pub mod motion;
pub mod obj;
pub mod pmanager;
pub mod source;
pub mod step;
#[cfg(feature = "trace")]
pub mod trace;
pub mod transition;

pub use arena::PhysHandle;
pub use cell::{Cell, CellArray, CellResolver, CellRuntime};
pub use geom::{
    BBoxExt, BspTree, CylSphere, CylSphereExt, Plane, PlaneExt, Polygon, Sphere, SphereExt,
};
pub use land::{LandblockCollision, SurfChar, WaterType};
pub use math::V3;
pub use motion::{MotionSource, NullMotion, ScriptedMotion};
pub use obj::{
    PhysicsObj, PhysicsState, PhysicsTimeStamp, SetPositionError, SetPositionFlags,
    SetPositionStruct, TransientState,
};
pub use pmanager::{InterpolateDecision, InterpolationManager, InterpolationStep, PositionManager};
pub use source::{
    BuildingGeometry, EnvCellGeometry, LandSource, PhysicsPart, SetupGeometry, StaticLandSource,
};
pub use step::{EtherealResult, MoveOrTeleport, PhysicsNotice, PhysicsWorld};
pub use transition::{CollisionCounters, Transition, TransitionState};

/// One error enum for the crate. The `SetPosition` family has its own richer `SetPositionError`,
/// which mirrors the client's own values.
#[derive(Debug, thiserror::Error)]
pub enum PhysicsError {
    /// The ten-deep transition pool is exhausted; the caller must abort the move.
    #[error("transition pool exhausted at depth {0}")]
    TransitionPoolExhausted(usize),
    /// The landblock is degenerate (`side_cell_count != 8`) and is excluded from physics.
    #[error("landblock {0} is degenerate and carries no physics geometry")]
    DegenerateLandblock(dereth_primitives::LandblockId),
}

/// Exactly `n.z >= floor_z`, no tolerance.
#[inline]
#[must_use]
pub fn is_valid_walkable(n: dereth_primitives::Vec3) -> bool {
    n.z >= globals::FLOOR_Z
}

/// The walkable-z threshold the object uses.
#[inline]
#[must_use]
pub const fn get_walkable_z() -> f32 {
    globals::FLOOR_Z
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::Vec3;

    // A surface is walkable exactly when its normal's vertical part reaches the fixed walkable
    // threshold: one comparison, no tolerance.
    #[test]
    fn walkability_is_a_bare_comparison_against_the_baked_constant() {
        assert!(is_valid_walkable(Vec3::new(0.0, 0.0, 1.0)));
        assert!(
            is_valid_walkable(Vec3::new(0.0, 0.0, get_walkable_z())),
            "the boundary is >="
        );
        let just_under = f32::from_bits(get_walkable_z().to_bits() - 1);
        assert!(
            !is_valid_walkable(Vec3::new(0.0, 0.0, just_under)),
            "one ulp decides it"
        );
    }
}
