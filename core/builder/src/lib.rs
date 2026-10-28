//! The world builders' editor machinery shared between them: the free camera an editor flies
//! through its world with.
//!
//! **Depends on** `dereth-primitives` (vectors and the math). **Used by** the world builders'
//! editors: Lyceum's (`lyceum-editor`).
//!
//! **Must never** read a device or a window: what keys are held arrives as [`camera::Held`], and
//! turning a window's keys into that is the editor's own business. Nor does it know a game's
//! client: an editor makes its renderer's camera from a [`camera::FlyCamera`] itself.
//!
//! [`camera`] is where the camera is in the world, which way it looks, how fast it flies (a
//! speed indoors and one outdoors), and the snap to the ground.

pub mod camera;
