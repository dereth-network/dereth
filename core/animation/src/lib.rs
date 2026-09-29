//! Animation and motion: motion tables, the frame player, part arrays, animation hooks, physics
//! scripts and particles.
//!
//! **Depends on** `dereth-primitives` and `dereth-rules` (its tests read the retail data through
//! `dereth-dat`, `dereth-assets` and `dereth-world-data`). **Used by** the shared world adapters
//! (`dereth-world-data`), the client runtime and the SDK, the client, and the server
//! (`empyrean-world`).
//!
//! **Must never** depend on the data-file decoders: its runtime shapes are its own, and
//! `dereth-world-data` converts decoded records into them. Across the physics seam
//! ([`MotionDriver`] implements [`MotionSource`](dereth_primitives::MotionSource)), `advance` must
//! never scale the returned origin by the object's scale or zero it when airborne: physics owns
//! both.
//!
//! The four things it would be easy to get wrong:
//!
//! 1. **Animation frames snap; there is no interpolation.** Every part is placed from
//!    `part_frames[floor(frame_number)]`, and the player crosses whole frames one at a time,
//!    integrating the motion table's velocity once per frame crossed. See [`seq::update`].
//! 2. **The motion-command wire format transmits an array index, not the id.** See [`command`].
//! 3. **The run-rate calculation is non-monotonic**: an early return at exactly skill 800 that does
//!    not lie on its own curve. See [`motion::get_run_rate`].
//! 4. **Preserve the shipped bugs.** `Explode` and `Implode` reuse one axis for all three
//!    ([`particles`]); the physics-script comparator never returns zero ([`script`]).
//!
//! **Specified in** `docs/formats/12-animation.md` (key frames and hooks),
//! `docs/formats/17-scene-and-particles.md` (particle emitters),
//! `docs/formats/18-physics-scripts.md` (effect timelines) and `docs/formats/19-motion-table.md`
//! (the motion state machine).

#![doc(html_no_source)]

pub mod command;
pub mod data;
pub mod driver;
pub mod frame;
pub mod hooks;
pub mod motion;
pub mod particles;
pub mod parts;
pub mod script;
pub mod seq;
pub mod table;

pub use command::{MotionCommand, STANCES};
pub use data::{
    AnimAssets, AnimData, AnimFrame, AnimationData, DegradeInfo, MapAssets, MotionData,
    MotionTableData, NoAssets, ParticleEmitterInfo, ParticleType, PhysicsScriptData,
    PhysicsScriptTableData, SetupData,
};
pub use driver::{step_animation, FpHook, FpHookKind, MotionDriver};
pub use hooks::{AnimEvent, AnimHook, AttackCone, HookKind, HookQueue};
pub use motion::{
    get_jump_height, get_run_rate, load_mod, HoldKey, MotionInterp, MovementManager,
    MovementParameters,
};
pub use particles::{Particle, ParticleEmitter, ParticleManager};
pub use parts::{PartArray, PhysicsPart};
pub use script::{get_script, vc7_qsort, ScriptManager};
pub use seq::{AnimSequenceNode, Sequence};
pub use table::{MotionState, MotionTable, MotionTableManager};

/// This crate's error type. Almost everything here reports failure the way the client does — a
/// zero return, a `None`, a numeric `WeenieError` — so there is very little for it to carry.
#[derive(Debug, thiserror::Error)]
pub enum AnimError {
    /// A `WeenieError` code from the movement layer, passed through unchanged because the client's
    /// callers compare against the numbers and the UI maps them to chat strings.
    #[error("movement refused with error 0x{0:02X}")]
    Movement(u32),
}
