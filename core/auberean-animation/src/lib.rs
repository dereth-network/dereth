//! Bodies that animate: skeletons and their rest poses, keyframe clips, the blend graph that picks
//! and mixes them, and skinning a mesh to the posed skeleton.
//!
//! **Depends on** the decoded records (`auberean-assets`, `auberean-dat`), the world's transforms
//! and records (`auberean-world`), `auberean-primitives` and the shared math (`dereth-primitives`). **Used by** drawing (`auberean-draw`)
//! and the world viewer (`auberean-viewer`).
//!
//! A body is drawn from part groups (the body, its head, a tail...), each a setup: meshes skinned to
//! a skeleton. A part group's animation map names, for each animation key (standing, moving, a
//! swing...), an animation: a keyframe clip, or a node that picks or blends others (by combat mode,
//! by a random draw, along sliders such as forward speed and turning). This crate builds the
//! skeleton and its rest pose ([`skeleton`]), samples clips ([`clip`]), evaluates the animation
//! graph into one pose ([`graph`]), and skins meshes to it ([`skin`]); [`body`] puts a whole body
//! together, with what its appearance does to each part ([`look`]).
//!
//! **Must never** do I/O or read a clock: records arrive already decoded, through
//! [`graph::Animations`].

pub mod body;
pub mod bones;
pub mod clip;
pub mod compositor;
pub mod dice;
pub mod graph;
pub mod host;
pub mod lookup;
pub mod math;
pub mod modes;
pub mod queries;
pub mod records;
pub mod scripts;
pub mod setups;
pub mod skeleton;
pub mod skin;
pub mod sliders;
pub mod table;
pub mod track;
pub mod tracker;
pub mod triggers;
pub mod turns;

/// A body's look is the world's: the same rules put a placed object together.
pub use auberean_world::look;
pub use auberean_world::look::{Desc, Look, LookRecords, PartLook};
pub use body::BodyRecords;
pub use body::{Body, BodyPart};
pub use clip::{Clip, Pose};
pub use graph::{Animations, Eval, GraphState};
pub use records::CachedRecords;
pub use skeleton::Skeleton;
