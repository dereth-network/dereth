//! Asheron's Call 2 plain data: the ids that name things, where a cell lies on the land, the
//! vectors, rotations and frames that place them, the scramble every stored and sent string goes
//! through, the string library's own rules for narrow text (hashes, case, trimming, packed size),
//! the client's clock over the host's elapsed time, the client's random numbers, and the whole
//! numbers of any size its key exchange works in.
//!
//! **Depends on** nothing. **Used by** every other crate of Asheron's Call 2, from the container
//! (`auberean-dat`) up.
//!
//! **Must never** do I/O, read a clock or hold game logic: plain data, pure conversions and the
//! generator's arithmetic only, buildable for any target, the browser's included.

#![no_std]

extern crate alloc;

pub mod ids;
pub mod land;
pub mod math;
pub mod random;
pub mod scramble;
pub mod strings;
pub mod time;
pub mod vlong;

pub use ids::{CellId, DataId};
pub use math::{BBox, Frame, Quat, Rgba, Sphere, Vec3};
