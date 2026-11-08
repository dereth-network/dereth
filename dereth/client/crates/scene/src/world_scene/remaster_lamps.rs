//! The remaster's lamps as the scene turns them on and reads them back.

use dereth_remaster::render::lamps::LampSettings;
use dereth_render::device::Gpu;

use super::remaster_view::remaster;
use super::SceneDraw;

/// What the remaster's lamps light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RemasterLamps {
    /// Rooms and the objects in them lit by the lamps, in place of retail's light.
    pub indoor: bool,
    /// The outdoors lit by the lamps at night.
    pub outdoor: bool,
}

/// One halo of the last frame, where it fell on the frame's target.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RemasterHaloSeen {
    /// The lamp's place on the target, in pixels.
    pub at: (f32, f32),
    /// How far the lamp stands from the eye, and the surface drawn at its pixel, metres.
    pub distance: f32,
    pub surface: f32,
    /// How wide it is, metres.
    pub size: f32,
    /// Its color, its night and flicker folded in.
    pub color: [f32; 3],
}

/// The remaster's lamps of the last frame it drew, read back.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, PartialEq)]
pub struct RemasterLampCapture {
    pub width: u32,
    pub height: u32,
    /// The lamps' light, RGBA a pixel: the light they add, and 1 in alpha where they take the
    /// place of retail's own light.
    pub light: Vec<[f32; 4]>,
    /// The lamps listed.
    pub lamps: u32,
    /// The halos that stand in front of the eye and fall on the target.
    pub halos: Vec<RemasterHaloSeen>,
    /// Each lamp listed: whether it lights the indoors, its reach, and its strengths on walls,
    /// on objects and outdoors.
    pub strengths: Vec<(bool, f32, [f32; 3])>,
    /// The remaster's material target, four bytes a pixel: the light class, the material's kind,
    /// and two bytes the class gives a meaning of its own.
    pub material: Vec<[u8; 4]>,
    /// Each piece of the view's list of the lamps that reach it, as the device listed them, and
    /// as the processor lists them from the same lamps.
    pub lists: Vec<Vec<u32>>,
    pub reference: Vec<Vec<u32>>,
    /// For each list that differs from its reference by a lamp: the piece, the lamp, and how far
    /// beyond the lamp's reach the piece lies, metres (negative within it).
    pub differences: Vec<(u32, u32, f32)>,
}

/// The last frame's halos on its target, each with the surface drawn at its pixel, read from
/// `depth` the way the halos' own pass reads it.
#[cfg(feature = "test-support")]
fn halos_seen(
    renderer: &dereth_remaster::render::Renderer,
    depth: &[u8],
) -> Option<Vec<RemasterHaloSeen>> {
    let (halos, view) = renderer.lamps().last_halos()?;
    let depth = depth.as_chunks::<4>().0;
    let view_proj = view.projection * view.view;
    let inverse = view_proj.inverse();
    let eye = view.view.inverse().w_axis.truncate();
    let (w, h) = view.extent;
    #[allow(clippy::cast_precision_loss)] // a window's size
    let (fw, fh) = (w as f32, h as f32);
    Some(
        halos
            .iter()
            .filter_map(|halo| {
                let clip = view_proj * halo.position.extend(1.0);
                if clip.w <= 0.0 {
                    return None;
                }
                let (nx, ny) = (clip.x / clip.w, clip.y / clip.w);
                let at = ((nx + 1.0) * 0.5 * fw, (1.0 - ny) * 0.5 * fh);
                if !(0.0..fw).contains(&at.0) || !(0.0..fh).contains(&at.1) {
                    return None;
                }
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // on the target, as just checked
                let pixel = at.1 as usize * w as usize + at.0 as usize;
                let d = f32::from_le_bytes(*depth.get(pixel)?);
                let point = inverse.project_point3(glam::Vec3::new(nx, ny, d));
                Some(RemasterHaloSeen {
                    at,
                    distance: halo.position.distance(eye),
                    surface: point.distance(eye),
                    size: halo.size,
                    color: halo.color,
                })
            })
            .collect(),
    )
}

impl SceneDraw {
    /// What the remaster's lamps light, from the next frame on.
    pub fn set_remaster_lamps(&mut self, lamps: RemasterLamps, gpu: &mut Gpu) {
        if let Some(r) = remaster(gpu) {
            r.renderer().set_lamp_settings(LampSettings {
                indoor: lamps.indoor,
                outdoor: lamps.outdoor,
            });
        }
    }

    /// The remaster's lamps of the last frame it drew; `None` while it is off or its lamps
    /// light nothing. The device waits for its work first.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn remaster_lamp_capture(&self, gpu: &mut Gpu) -> Option<RemasterLampCapture> {
        let r = remaster(gpu)?;
        let renderer = r.renderer();
        let t = renderer.targets()?;
        let (count, strengths, reference) = {
            let (lamps, reference) = renderer.lamps().reference()?;
            let strengths = lamps
                .iter()
                .map(|l| (l.indoor, l.reach, [l.walls, l.objects, l.outdoors]))
                .collect();
            (
                u32::try_from(lamps.len()).unwrap_or(u32::MAX),
                strengths,
                reference,
            )
        };
        let back = renderer.read_back()?;
        let material = back.material.as_chunks::<4>().0.to_vec();
        let halos = halos_seen(renderer, &back.depth).unwrap_or_default();
        let (light, lists) = renderer.lamp_read_back()?;
        let half = |w: [u8; 2]| f16_to_f32(u16::from_le_bytes(w));
        let light = light
            .as_chunks::<8>()
            .0
            .iter()
            .map(|p| {
                [
                    half([p[0], p[1]]),
                    half([p[2], p[3]]),
                    half([p[4], p[5]]),
                    half([p[6], p[7]]),
                ]
            })
            .collect();
        let mut differences = Vec::new();
        for (n, (a, b)) in lists.iter().zip(&reference).enumerate() {
            let piece = u32::try_from(n).unwrap_or(u32::MAX);
            for l in a
                .iter()
                .filter(|l| !b.contains(l))
                .chain(b.iter().filter(|l| !a.contains(l)))
            {
                differences.push((
                    piece,
                    *l,
                    renderer.lamps().edge(piece, *l).unwrap_or(f32::MAX),
                ));
            }
        }
        Some(RemasterLampCapture {
            width: t.width,
            height: t.height,
            light,
            lamps: count,
            halos,
            strengths,
            material,
            lists,
            reference,
            differences,
        })
    }
}

/// A half float's value.
pub(super) fn f16_to_f32(h: u16) -> f32 {
    let sign = u32::from(h >> 15) << 31;
    let exp = u32::from((h >> 10) & 0x1F);
    let frac = u32::from(h & 0x3FF);
    match exp {
        // Too small for a normal half: its fraction in units of 2^-24.
        0 => {
            let v = f32::from(u16::try_from(frac).unwrap_or(0)) * 5.960_464_5e-8;
            if sign == 0 {
                v
            } else {
                -v
            }
        }
        31 => f32::from_bits(sign | 0x7F80_0000 | (frac << 13)),
        _ => f32::from_bits(sign | ((exp + 112) << 23) | (frac << 13)),
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    #[test]
    fn a_half_float_reads_as_its_value() {
        assert_eq!(f16_to_f32(0x3C00), 1.0);
        assert_eq!(f16_to_f32(0xC000), -2.0);
        assert_eq!(f16_to_f32(0x3800), 0.5);
        assert_eq!(f16_to_f32(0x0001), 5.960_464_5e-8);
        assert_eq!(f16_to_f32(0x7BFF), 65504.0);
        assert_eq!(f16_to_f32(0), 0.0);
    }
}
