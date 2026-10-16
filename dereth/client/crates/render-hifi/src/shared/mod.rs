//! What several passes share: the landscape field, the retained geometry, camera arithmetic,
//! noise, the full-screen triangle and an art pack's pictures on the device.

pub mod camera;
pub mod fullscreen;
pub mod noise;
pub mod pack_image;
pub mod resident;
pub mod runs;
pub mod static_feed;
pub mod terrain_field;
