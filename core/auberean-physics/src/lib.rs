//! Bodies that move: the shared swept-sphere core with this world's settings, the world's cells for
//! it (terrain and the solid objects placed on it), the motion of a body from its movement input,
//! the physics step, and the chase camera.
//!
//! **Depends on** the shared transition (`dereth-physics`), terrain core (`dereth-terrain`) and
//! `dereth-primitives`, and this game's world (`auberean-world`), records (`auberean-assets`,
//! `auberean-dat`) and `auberean-primitives`. **Used by** the world viewer (`auberean-viewer`), the
//! world a window shows (`auberean-scene`) and the server (`yalain`).
//!
//! The swept-sphere transition is the shared engine's (`dereth-physics`): this world sets its rules
//! ([`rules`]) and hands it this world's cells ([`land`]), each 20 m land cell answering for its
//! ground and the solid objects placed in it, and its interior cells ([`interior`]), each with its
//! walls and floors, its inside test and its doorways. Another object's spheres and upright
//! cylinders are met by this world's own tests ([`obstacle`]), its trees by the shared ones. On top
//! of that this crate moves a body the way the client does: the motion interpreter turns movement
//! input into velocities ([`motion`]), the
//! physics step applies them with gravity and friction through the transition ([`body`]), and the
//! chase camera follows, kept off walls ([`camera`]).
//!
//! **Must never** do I/O or read a clock: time arrives as seconds from the caller.

pub mod body;
pub mod camera;
pub mod cells;
pub mod collision;
pub mod detect;
pub mod ethereal;
pub mod geom;
pub mod interior;
pub mod land;
pub mod material;
pub mod mesh;
pub mod missile;
pub mod motion;
pub mod object;
pub mod obstacle;
pub mod pack;
pub mod parts;
pub mod path;
pub mod pathfind;
pub mod place;
pub mod rules;
pub mod solid;
pub mod story;
pub mod target;
pub mod world;

pub use body::{Body, BodyConfig, Ticker};
pub use camera::{Camera, CameraSettings, View};
pub use land::Land;
pub use motion::{MotionInput, MotionInterp};
