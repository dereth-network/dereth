//! Missiles: how one flies ([`MissileParameters`]), what it aims at and where it starts
//! ([`MissileTargetingParameters`]), and its guidance ([`Guidance`]), which places it, launches it,
//! steers it toward its target and decides when a collision sets it off. The missile's body (its
//! place, its velocity, its effects) and the objects it aims at arrive through [`GuidanceHost`].

mod guidance;
mod params;
mod targeting;

pub use guidance::{fail, Guidance, GuidanceHost, FLIGHT_FX};
pub use params::{ground, set, MissileParameters};
pub use targeting::{aim, MissileTargetingParameters};
