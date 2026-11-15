//! The frame's lamps: its point lights as the shared passes' lamps take them.
//!
//! A frame's point lights are those its draws are lit by (each draw's own, at most four): every
//! light outdoors that an object carries, and every light indoors. Each is one lamp, worked out per
//! pixel as retail works it out at an object's vertices (`shaders/lamps.slang`): where retail
//! lights an object by it, the lamp takes the place of its share; outdoors the lamp lights the land
//! too, as night comes, and stands a soft halo round itself.

use std::collections::HashSet;

use auberean_draw::scene::Frame;
use dereth_gfx_passes::frame::Camera;
use dereth_gfx_passes::lamps::{Halo, View, LAMP_BYTES};
use glam::{Mat4, Vec3};

/// The kinds of lamp, as the shading tells them apart (`shaders/lamps.slang`).
const KIND_INDOOR: f32 = 1.0;
const KIND_OUTDOOR: f32 = 2.0;

/// The lamps a frame lists at most, the nearest the eye first.
const MAX_LAMPS: usize = 512;

/// How far a lamp reaches against its light's range: retail's attenuation never ends, and at three
/// ranges it is a hundredth of the light (`shaders/lamps.slang`).
const REACH: f32 = 3.0;

/// How far from the eye a lamp stands a halo, metres, and how many do in a frame.
const HALO_REACH: f32 = 120.0;
const MAX_HALOS: usize = 128;

/// A halo's width against its light's range, the narrowest and the widest, metres; and how bright
/// it is against its light's color.
const HALO_SIZE: (f32, f32, f32) = (0.06, 0.4, 1.0);
const HALO_GAIN: f32 = 0.6;

/// The frame's lamps: their rows as the lamps' buffer holds them, their places and ranges, and
/// their halos.
#[derive(Debug, Default)]
pub struct Found {
    pub rows: Vec<u8>,
    pub reaches: Vec<(Vec3, f32)>,
    pub halos: Vec<Halo>,
}

/// How far the land is lit by the lamps outdoors, 0 by day to 1 at night, from how dark the sky
/// is outdoors: the fog's color, which is the horizon's. The night's moon lights the land as the
/// day's sun does, from above and as brightly, so the light itself doesn't tell night from day.
/// The lamps come on as the fog's brightness falls below 0.3 and are full below 0.08.
#[must_use]
pub fn night(frame: &Frame) -> f32 {
    let c = frame.outside.as_ref().unwrap_or(&frame.light).fog_color;
    let luma = 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
    let t = ((0.3 - luma) / 0.22).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The lamps of `frame`, seen from `eye`: each point light its draws are lit by, once, the nearest
/// [`MAX_LAMPS`]; and the halos of those outdoors within [`HALO_REACH`] as night comes.
#[must_use]
pub fn of(frame: &Frame, eye: Vec3) -> Found {
    let n = night(frame);
    let mut seen = HashSet::new();
    let mut lamps = Vec::new();
    for b in frame.objects.iter().chain(&frame.decals) {
        let p = &b.points;
        for l in p.lights.iter().take(p.count.min(p.lights.len())) {
            let key = [
                l.origin.x.to_bits(),
                l.origin.y.to_bits(),
                l.origin.z.to_bits(),
                l.range.to_bits(),
                l.diffuse.r.to_bits(),
                l.diffuse.g.to_bits(),
                l.diffuse.b.to_bits(),
                u32::from(p.inside),
            ];
            if l.range <= 0.0 || !seen.insert(key) {
                continue;
            }
            let at = Vec3::new(l.origin.x, l.origin.y, l.origin.z);
            lamps.push((
                at,
                l.range,
                [l.diffuse.r, l.diffuse.g, l.diffuse.b],
                p.inside,
            ));
        }
    }
    lamps.sort_by(|a, b| {
        a.0.distance_squared(eye)
            .total_cmp(&b.0.distance_squared(eye))
    });
    lamps.truncate(MAX_LAMPS);
    let mut out = Found::default();
    for (at, range, color, inside) in &lamps {
        let mut f = |v: [f32; 4]| {
            for x in v {
                out.rows.extend_from_slice(&x.to_le_bytes());
            }
        };
        f([at.x, at.y, at.z, range * REACH]);
        f([
            color[0],
            color[1],
            color[2],
            if *inside { KIND_INDOOR } else { KIND_OUTDOOR },
        ]);
        f([1.0, if *inside { 0.0 } else { n }, *range, 0.0]);
        out.reaches.push((*at, range * REACH));
    }
    debug_assert_eq!(out.rows.len(), out.reaches.len() * LAMP_BYTES);
    if n > 0.0 {
        out.halos = lamps
            .iter()
            .filter(|(at, _, _, inside)| !inside && at.distance(eye) < HALO_REACH)
            .take(MAX_HALOS)
            .map(|(at, range, color, _)| {
                let top = color[0].max(color[1]).max(color[2]).max(1e-4);
                let k = n * HALO_GAIN / top.max(1.0);
                Halo {
                    position: *at,
                    size: (range * HALO_SIZE.0).clamp(HALO_SIZE.1, HALO_SIZE.2),
                    color: color.map(|c| c * k),
                }
            })
            .collect();
    }
    out
}

/// How the lamps cut the view of `camera` at `extent`: its view turned so its z runs ahead of the
/// eye, as the shared passes take it, and its projection turned to match.
#[must_use]
pub fn view(camera: &Camera, extent: (u32, u32)) -> View {
    let turn = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0));
    View {
        view: turn * camera.view,
        projection: camera.projection * turn,
        extent,
    }
}
