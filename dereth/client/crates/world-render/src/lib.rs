//! What the world draws each frame and in what order, emitted as draw batches with no GPU code.
//!
//! **Depends on** `dereth-primitives` (the vocabulary, the arithmetic policy and the
//! [`RenderBackend`](dereth_primitives::RenderBackend) seam), the decoded world records of
//! `dereth-assets`, the block window of `dereth-landscape` and the landscape's geometry and static
//! contents in `dereth-terrain`, whose modules it re-exports at their old paths (`land::mesh`,
//! `scenery`, `consts` and the rest). **Used by** the client (`dereth-client`).
//!
//! **Must never** hold GPU code, link a renderer crate or decode anything: everything leaves
//! through [`RenderBackend`](dereth_primitives::RenderBackend) as
//! [`DrawBatch`](dereth_primitives::DrawBatch)es in submission order, and it never reaches the
//! client runtime (`cargo xtask seams`, `seam: client crates`). Its tests read the retail data and
//! **fail** when it is absent.
//!
//! | Module | What it owns |
//! |---|---|
//! | [`land::mesh`] | vertices, the diagonal split, planes, UVs, per-cell rotation |
//! | [`land::merge`] | the terrain merge key, road codes, alpha maps, the integer blend |
//! | [`land::stitch`] | the transition adjustment between detail rings |
//! | [`land::lighting`], [`land::water`] | per-vertex terrain colours, water classification |
//! | [`scenery`], [`road`] | deterministic scenery, its hashes and filters |
//! | [`land::order`], [`land::emit`], [`cells::cull`] | draw order and vertical-column culling |
//! | [`objects`] | the part hierarchy, degrade and billboarding, the alpha list |
//! | [`cells::portal_view`] | portal traversal and the indoor path |
//! | [`lighting`] | the light pools, the eight-light cap |
//! | [`sky`] | the sky lookup, the day cycle, fog |
//! | [`particles`] | the closed-form emitter evaluation |
//! | [`degrade_loop`] | adaptive degradation, pinnable for tests |
//! | [`frame`] | the pass sequence, in the client's order |
//!
//! Four rendering rules that most often survive review:
//!
//! 1. The terrain diagonal split is the **sign bit** of a 32-bit hash of global cell coordinates;
//!    ACE's mesh code returns the inverse boolean under the opposite name. See
//!    [`land::mesh::sw_to_ne_cut`].
//! 2. The terrain blend is **integer** arithmetic and **alpha 0 means the overlay wins**. See
//!    [`land::merge::integer_blend`].
//! 3. **Everything draws far to near**, including cells within a block, and the alpha list is
//!    flushed in insertion order with no depth sort. See [`land::order`] and [`objects::alpha`].
//! 4. `degrade_mode` is a **billboarding** mode, not a level-of-detail hint. See
//!    [`objects::degrade`].

#![doc(html_no_source)]
#![forbid(unsafe_code)]

pub mod cells;
pub mod creature_mode;
pub mod degrade_loop;
pub mod detail;
pub mod frame;
pub mod land;
pub mod lighting;
pub mod objects;
pub mod particles;
pub mod sky;

// The landscape's geometry and static contents are `dereth-terrain`'s; they are re-exported at the
// paths they have always had here.
pub(crate) use dereth_terrain::narrow;
pub use dereth_terrain::{consts, math, road, scenery, testing};

pub use land::merge::{integer_blend, MergeKey, TerrainMergeCache};
pub use land::mesh::{
    generate_landblock, sw_to_ne_cut, Direction, LandPolygon, LandblockMesh, Rotation,
};
pub use land::order::{block_draw_order, block_orient, cell_draw_order};
pub use land::water::WaterType;
pub use scenery::{generate_scenery, PlacedScenery};

/// A plane in the client's form: a **unit** normal and the signed distance `d` such that
/// `N·p + d == 0` on the plane.
///
/// The same type the dat decoders produce and physics collides with; here it is constructed at run
/// time from polygon geometry and is the one terrain, scenery placement and picking all read.
pub use dereth_primitives::shape::Plane;
