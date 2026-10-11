//! The re-shaded world: how a frame is cut up, the landscape field it reads, which pixels take
//! the field's normal, and where the world replay stops to keep its depth before any building is
//! stamped.

mod landscape_normals;
mod plan;
mod stamp_split;
mod terrain_field;
