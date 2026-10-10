//! The passes, one directory each.

use crate::graph::{HifiPass, Slot};

pub mod atmosphere;
pub mod gi;
pub mod gtao;
pub mod lamps;
pub mod lighting;
pub mod rt;
pub mod weather;

/// Every pass, each with its slot.
#[must_use]
pub fn all() -> Vec<(Slot, Box<dyn HifiPass>)> {
    let (sky_tables, sky, air) = atmosphere::passes();
    vec![
        (Slot::Compute, Box::new(sky_tables)),
        (Slot::WorldReplay, Box::new(sky)),
        (Slot::Opaque, Box::new(lighting::Lighting::default())),
        (Slot::Opaque, Box::new(gtao::Gtao::default())),
        (Slot::Opaque, Box::new(gi::GlobalIllumination::default())),
        (Slot::Atmosphere, Box::new(gtao::Gtao::late())),
        (Slot::Atmosphere, Box::new(air)),
        (Slot::Atmosphere, Box::new(weather::Weather::default())),
        (Slot::Hdr, Box::new(lighting::LightingPost::default())),
    ]
}
