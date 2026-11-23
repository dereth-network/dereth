//! AC2's dungeon lights as AC1's: a point light on a setup of no parts, which a room's static
//! stands for; and the lights a set of AC2's dungeons use, by color and reach.

use std::collections::BTreeMap;

use auberean_world::inside::{Light, LightKind};
use dereth_primitives::{DataId, EraId, Frame, Quat, Vec3};

use crate::records::{NewIds, Records};
use crate::source::Ac2;

/// An AC1 light's intensity, as the rooms Lyceum builds are lit.
pub const LIGHT_INTENSITY: f32 = 100.0;

/// A light and nothing else: a setup of no parts with one point light of `color` (ARGB) reaching
/// `reach` at its origin.
/// # Errors
/// A setup that will not encode.
pub fn light_setup(
    color: u32,
    reach: f32,
    ids: &mut dyn NewIds,
    out: &mut Records,
) -> Result<DataId, String> {
    use dereth_assets::geometry::{LightInfo, Placement, SetupOrder};
    use dereth_assets::{Setup, Sphere};
    let id = ids.next_for(0x02, &light_name(color, reach));
    let sphere = Sphere {
        center: Vec3::ZERO,
        radius: 0.1,
    };
    let setup = Setup {
        id,
        flags: 0,
        parts: Vec::new(),
        parent_index: None,
        default_scale: None,
        allow_free_heading: false,
        has_physics_bsp: false,
        holding_locations: BTreeMap::new(),
        connection_points: BTreeMap::new(),
        placement_frames: BTreeMap::from([(
            0,
            Placement {
                frames: Vec::new(),
                hooks: Vec::new(),
            },
        )]),
        cylspheres: Vec::new(),
        spheres: Vec::new(),
        height: 0.0,
        radius: 0.0,
        step_up_height: 0.0,
        step_down_height: 0.0,
        sorting_sphere: sphere,
        selection_sphere: sphere,
        lights: BTreeMap::from([(
            0,
            LightInfo {
                frame: Frame::new(Vec3::ZERO, Quat::IDENTITY),
                color_argb: color,
                intensity: LIGHT_INTENSITY,
                falloff: reach,
                cone_angle: 0.0,
            },
        )]),
        default_anim_id: DataId(0),
        default_script_id: DataId(0),
        default_mtable_id: DataId(0),
        default_stable_id: DataId(0),
        default_phstable_id: DataId(0),
        written_order: SetupOrder::default(),
    };
    out.add(
        id,
        setup
            .encode_for(EraId::Eor)
            .map_err(|e| format!("{id:?}: {e}"))?,
    );
    Ok(id)
}

/// What a light is called by its color and reach: `light ffd450 12m`, its reach to a tenth when it
/// is not a whole meter (`light ffd450 12.5m`).
#[must_use]
pub fn light_name(color: u32, reach: f32) -> String {
    let rgb = color & 0x00FF_FFFF;
    if (reach - reach.round()).abs() < 0.05 {
        format!("light {rgb:06x} {reach:.0}m")
    } else {
        format!("light {rgb:06x} {reach:.1}m")
    }
}

/// A light's color as AC1 packs it, opaque.
#[must_use]
pub fn argb(l: &Light) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    0xFF00_0000 | (c(l.color.r) << 16) | (c(l.color.g) << 8) | c(l.color.b)
}

/// The point lights AC2's dungeons in `blocks` light their rooms with, by color (AC1's packing)
/// and reach (meters, to a tenth), each with how many of them stand in those dungeons, most first.
#[must_use]
pub fn dungeon_lights(ac2: &Ac2, blocks: &[(i32, i32)]) -> Vec<(u32, f32, usize)> {
    let mut seen: BTreeMap<(u32, i32), usize> = BTreeMap::new();
    for &(x, y) in blocks {
        let Some(li) = ac2.records().light_info(x, y) else {
            continue;
        };
        for l in &li.lights {
            let o = auberean_primitives::Vec3::new(l.origin.x, l.origin.y, l.origin.z);
            let (light, _, _) = Light::of_properties(l.cell.0, o, &l.properties);
            if light.kind == LightKind::Directional {
                continue;
            }
            let reach = light.most_range.max(light.range);
            let tenths = dereth_primitives::num::to_i32((reach * 10.0).round());
            *seen.entry((argb(&light), tenths)).or_default() += 1;
        }
    }
    let mut out: Vec<(u32, f32, usize)> = seen
        .into_iter()
        .map(|((c, r), n)| {
            #[allow(clippy::cast_precision_loss)] // tenths of meters
            let reach = r as f32 / 10.0;
            (c, reach, n)
        })
        .collect();
    out.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)).then(a.1.total_cmp(&b.1)));
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: AC2's dungeon lights as AC1's)
    use super::*;

    #[test]
    fn a_light_is_named_by_its_color_and_its_reach_to_a_tenth_when_not_whole() {
        assert_eq!(light_name(0xFFFF_D450, 12.0), "light ffd450 12m");
        assert_eq!(light_name(0xFFFF_D450, 12.5), "light ffd450 12.5m");
        assert_eq!(light_name(0xFF02_9502, 8.02), "light 029502 8m");
    }
}
