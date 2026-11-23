//! The world builders' machinery shared between them: the free camera an editor flies through its
//! world with, and the trees a new model or room needs.
//!
//! **Depends on** `dereth-primitives` (vectors and the math) and `dereth-assets` (the records the
//! trees belong to). **Used by** the world builders: Lyceum's editor and its foundry, and
//! `dereth-art`'s world builds.
//!
//! **Must never** read a device or a window: what keys are held arrives as [`camera::Held`], and
//! turning a window's keys into that is the editor's own business. Nor does it know a game's
//! client: an editor makes its renderer's camera from a [`camera::FlyCamera`] itself.
//!
//! [`camera`] is where the camera is in the world, which way it looks, how fast it flies (a
//! speed indoors and one outdoors), and the snap to the ground. [`bsp`] builds a model's physics
//! and drawing trees and a room's cell tree from its polygons, and a room's whole shape.

pub mod bsp;
pub mod camera;
