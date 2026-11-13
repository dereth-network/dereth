//! The AC2 look: Asheron's Call 2's ground, trees, rocks and sky drawn in Asheron's Call, as an art
//! pack built on the player's own computer from their own Asheron's Call 2 files. Nothing of either
//! game's art is shipped: the [`tables`] say which AC2 thing stands for which Asheron's Call thing
//! and how each is fitted, and the build reads every picture and every vertex from the player's
//! files.
//!
//! [`build::build`] writes the parts asked for as one pack's folder and packs it: [`source`] reads
//! the AC2 records and the Asheron's Call collision, [`model`] makes each model and fits it,
//! [`picture`] tints a ground and derives its relief.

pub mod build;
pub mod model;
pub mod picture;
pub mod source;
pub mod tables;
