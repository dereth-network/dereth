//! The passes, one directory each.

use crate::graph::{HifiPass, Slot};

pub mod art_pack;
pub mod atmosphere;
pub mod gi;
pub mod ground;
pub mod gtao;
pub mod lamps;
pub mod lighting;
pub mod rt;
pub mod taa;
pub mod weather;

/// Every pass, each with its slot.
#[must_use]
pub fn all() -> Vec<(Slot, Box<dyn HifiPass>)> {
    let (sky_tables, sky, air) = atmosphere::passes();
    let (pack, pack_casters) = art_pack::pass();
    let (taa_keep, taa_translucent, taa_resolve) = taa::passes();
    vec![
        (Slot::Compute, Box::new(sky_tables)),
        (Slot::WorldReplay, Box::new(sky)),
        // After the sky, whose say in the replay comes first.
        (Slot::WorldReplay, Box::new(pack)),
        // Once the world is re-shaded and the landscape's normals written, before the light.
        (Slot::WorldReplay, Box::new(ground::pass())),
        (
            Slot::Opaque,
            Box::new(lighting::Lighting::with_art_pack(pack_casters)),
        ),
        (Slot::Opaque, Box::new(gi::GlobalIllumination::default())),
        // After the bounced light, which it darkens where it darkens the leaves of a canopy.
        (Slot::Opaque, Box::new(gtao::Gtao::default())),
        // Once the opaque world is lit.
        (Slot::Opaque, Box::new(taa_keep)),
        (Slot::AlphaReplay, Box::new(taa_translucent)),
        (Slot::Atmosphere, Box::new(gtao::Gtao::late())),
        (Slot::Atmosphere, Box::new(air)),
        // Once the air is drawn over the world: the weather falls over the blended picture,
        // where the streaks of rain and the flakes of snow move too fast to follow.
        (Slot::Atmosphere, Box::new(taa_resolve)),
        // Once the light, the occlusion, the bounced light and the air are drawn, before the
        // weather.
        (
            Slot::Atmosphere,
            Box::new(lighting::OutdoorMeasure::default()),
        ),
        (Slot::Atmosphere, Box::new(weather::Weather::default())),
        (Slot::Hdr, Box::new(lighting::LightingPost::default())),
    ]
}
