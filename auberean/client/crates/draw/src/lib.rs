//! What a frame of the world draws, with no device: the terrain's layers, the placed objects and
//! interiors, and the sky, as batches carrying their pictures, materials and the moment's light.
//!
//! **Depends on** `auberean-world` (the terrain, light, sky and placement), the records
//! (`auberean-assets`, `auberean-dat`), posed bodies (`auberean-animation`), polygons clipped to
//! planes (`auberean-physics`), `auberean-primitives`,
//! and three shared pieces: the block window (`dereth-landscape`), the picture codecs
//! (`dereth-render-cpu`) and the original's float-to-integer conversion (`dereth-primitives`).
//! **Used by** the device (`auberean-gpu`), the world a window shows (`auberean-scene`) and the
//! products that draw (`auberean-viewer`, `auberean-client`).
//!
//! - [`scene`]: a frame at a spot.
//! - [`cull`]: what a frame seen from inside shows at the moment, walked through the doorways.
//! - [`decal`]: pictures laid over the ground for a moment.
//! - [`body`]: a posed body, skinned, as mesh batches.
//! - [`material`]: a material instance resolved to the layers a device draws.
//! - [`mesh`]: a mesh's vertices and the detail level at a distance.
//! - [`picture`]: a picture list's levels, expanded to texels.
//! - [`portal`]: portal space, the tunnel drawn while the world is out of sight.
//! - [`shadow`]: the sun's shadow map of the near objects and the clouds' shadows.
//! - [`shore`]: the strips along the shores of the detailed blocks' water.
//! - [`streak`]: the trails a moving object's part leaves while they are on.
//! - [`volume`]: the dynamic objects' volumes of shadow.
//! - [`particles`], [`effects`], [`systems`]: particle systems run as the client runs them, the
//!   effects that set them going, and the systems kept together, shared and drawn once a frame.
//!
//! **Must never** hold a device: what leaves here is data a device can upload.

pub mod body;
pub mod cull;
pub mod decal;
pub mod effects;
pub mod flare;
pub mod material;
pub mod mesh;
pub mod particles;
pub mod picture;
pub mod portal;
pub mod quality;
pub mod rain;
pub mod scene;
pub mod shadow;
pub mod shore;
pub mod streak;
pub mod systems;
pub mod volume;
