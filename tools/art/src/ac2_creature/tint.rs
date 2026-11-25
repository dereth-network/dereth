//! What a look of an AC2 body changes besides its pictures, made into what a classic surface can
//! show: the look's material color baked into its picture, its see-through color as the weenie's
//! translucency, and a note of what neither can carry.
//!
//! - **A lit material** is drawn as its picture times the light times its color. The classic look
//!   lights a creature's picture itself, so the look's picture is baked as the picture times the
//!   look's color over the body's own.
//! - **An unlit material** (a shadow) is drawn as its stages make it, without light. Its stages
//!   are worked out for a still moment: each later stage's picture taken at its mean, the texture
//!   factor and the diffuse color at the look's color, and the result clamped.

use auberean_assets::material::{Material, MaterialModifier, DIFFUSE_MAP};
use dereth_ac2_convert::records::{NewIds, Records, PFID_A8R8G8B8};
use dereth_ac2_convert::surface::{diffuse_stage, BLEND_SRC_ALPHA};
use dereth_assets::{RenderSurface, SurfaceTexture};
use dereth_primitives::DataId;

use crate::ac2_look::source::Ac2;

/// The name of a parameter that is a material's second picture.
const SECOND_MAP: u32 = 3001;
/// The names of a material's detail and normal pictures, and its specular color.
const DETAIL_MAP: u32 = 3010;
const NORMAL_MAP: u32 = 3011;
const SPECULAR_COLOR: u32 = 2001;

/// A texture stage's operations and arguments, as AC2's templates hold them.
mod op {
    pub const DISABLE: u32 = 1;
    pub const SELECT_ARG1: u32 = 2;
    pub const SELECT_ARG2: u32 = 3;
    pub const MODULATE: u32 = 4;
    pub const MODULATE_2X: u32 = 5;
    pub const MODULATE_4X: u32 = 6;
    pub const ADD: u32 = 7;
    pub const ADD_SIGNED: u32 = 8;
    pub const ADD_SIGNED_2X: u32 = 9;
    pub const SUBTRACT: u32 = 10;
}
mod arg {
    pub const DIFFUSE: u32 = 0;
    pub const CURRENT: u32 = 1;
    pub const TEXTURE: u32 = 2;
    pub const FACTOR: u32 = 3;
    pub const WHICH: u32 = 0xF;
    pub const COMPLEMENT: u32 = 0x10;
}

/// A texel's color made from the picture's, `a × picture + b` in each of red, green and blue,
/// clamped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bake {
    pub a: [f32; 3],
    pub b: [f32; 3],
}

impl Bake {
    const SAME: Self = Self {
        a: [1.0; 3],
        b: [0.0; 3],
    };

    fn is_same(&self) -> bool {
        (0..3).all(|c| (self.a[c] - 1.0).abs() < 1e-3 && self.b[c].abs() < 1e-3)
    }

    /// Its numbers, to key the baked pictures by.
    #[must_use]
    pub fn key(&self) -> [u32; 6] {
        [
            self.a[0], self.a[1], self.a[2], self.b[0], self.b[1], self.b[2],
        ]
        .map(f32::to_bits)
    }

    /// `rgba` (red, green, blue, alpha) with its colors made over.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: a clamped share of 255.
    pub fn apply(&self, rgba: &[u8]) -> Vec<u8> {
        rgba.chunks(4)
            .flat_map(|t| {
                let c = |k: usize| {
                    let v = self.a[k] * f32::from(t[k]) / 255.0 + self.b[k];
                    (v.clamp(0.0, 1.0) * 255.0).round() as u8
                };
                [c(0), c(1), c(2), t[3]]
            })
            .collect()
    }
}

/// What a look does to one material besides choosing its picture.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LookColor {
    /// The look's color baked into its picture, where it changes it.
    pub bake: Option<Bake>,
    /// Its color's alpha, where AC2 draws the material see-through by it.
    pub alpha: Option<f32>,
    /// What the look changes that a classic surface cannot show.
    pub lost: Vec<&'static str>,
}

/// A parameter's value as a material is made ready with `modifiers`: the template's own, then
/// each modifier's of a name the template has, the last winning.
fn parameter<'a>(
    template: &'a Material,
    modifiers: &'a [MaterialModifier],
    name: u32,
) -> Option<&'a auberean_assets::material::MaterialParameter> {
    if !template
        .parameters
        .parameters
        .iter()
        .any(|p| p.name == name)
    {
        return None;
    }
    std::iter::once(&template.parameters)
        .chain(modifiers)
        .flat_map(|m| &m.parameters)
        .rfind(|p| p.name == name)
}

/// The picture list stage `k` of the first layer shows as the material is made ready.
fn stage_picture(template: &Material, modifiers: &[MaterialModifier], k: usize) -> u32 {
    let name = match k {
        0 => DIFFUSE_MAP,
        1 => SECOND_MAP,
        _ => 0,
    };
    let own = template
        .layers
        .first()
        .and_then(|l| l.stages.get(k))
        .map_or(0, |s| s.texture.0);
    parameter(template, modifiers, name)
        .and_then(auberean_assets::material::MaterialParameter::texture)
        .map_or(own, |t| t.0)
}

/// The mean red, green and blue of picture list `list` (a half where it will not read).
#[allow(clippy::cast_precision_loss)] // LINT-OK: a mean of bytes.
fn mean(ac2: &Ac2, list: u32) -> [f32; 3] {
    let Some(p) = ac2.picture(list, 64) else {
        return [0.5; 3];
    };
    let n = (p.texels.len() / 4).max(1) as f32;
    let mut sum = [0.0f32; 3];
    for t in p.texels.chunks(4) {
        for c in 0..3 {
            sum[c] += f32::from(t[c]) / 255.0;
        }
    }
    sum.map(|s| s / n)
}

/// The first layer's color stages worked out as `a × picture + b` for a still moment: the
/// first stage's picture is the one baked, each later stage's taken at its mean, and the texture
/// factor and the diffuse color are `color`. `None` where a stage does what that cannot follow.
fn stages(
    ac2: &Ac2,
    template: &Material,
    modifiers: &[MaterialModifier],
    color: [f32; 4],
) -> Option<Bake> {
    let layer = template.layers.first()?;
    let mut current = Bake {
        a: [0.0; 3],
        b: [color[0], color[1], color[2]],
    };
    for (k, s) in layer.stages.iter().enumerate() {
        if s.color_op == op::DISABLE {
            break;
        }
        let texture = if k == 0 {
            Bake::SAME
        } else {
            Bake {
                a: [0.0; 3],
                b: mean(ac2, stage_picture(template, modifiers, k)),
            }
        };
        let value = |a: u32| {
            let v = match a & arg::WHICH {
                arg::CURRENT => current,
                arg::TEXTURE => texture,
                arg::DIFFUSE | arg::FACTOR => Bake {
                    a: [0.0; 3],
                    b: [color[0], color[1], color[2]],
                },
                _ => return None,
            };
            Some(if a & arg::COMPLEMENT == 0 {
                v
            } else {
                Bake {
                    a: v.a.map(|x| -x),
                    b: v.b.map(|x| 1.0 - x),
                }
            })
        };
        let (x, y) = (value(s.color_arg1)?, value(s.color_arg2)?);
        let sum = |k: f32, bias: f32| Bake {
            a: [0, 1, 2].map(|c| k * (x.a[c] + y.a[c])),
            b: [0, 1, 2].map(|c| k * (x.b[c] + y.b[c] + bias)),
        };
        current = match s.color_op {
            op::SELECT_ARG1 => x,
            op::SELECT_ARG2 => y,
            op::ADD => sum(1.0, 0.0),
            op::ADD_SIGNED => sum(1.0, -0.5),
            op::ADD_SIGNED_2X => sum(2.0, -0.5),
            op::SUBTRACT => Bake {
                a: [0, 1, 2].map(|c| x.a[c] - y.a[c]),
                b: [0, 1, 2].map(|c| x.b[c] - y.b[c]),
            },
            op::MODULATE | op::MODULATE_2X | op::MODULATE_4X => {
                let k = match s.color_op {
                    op::MODULATE => 1.0,
                    op::MODULATE_2X => 2.0,
                    _ => 4.0,
                };
                // One side must be still for the product to stay a scaling of the picture.
                let (still, moving) = if x.a == [0.0; 3] {
                    (x, y)
                } else if y.a == [0.0; 3] {
                    (y, x)
                } else {
                    return None;
                };
                Bake {
                    a: [0, 1, 2].map(|c| k * still.b[c] * moving.a[c]),
                    b: [0, 1, 2].map(|c| k * still.b[c] * moving.b[c]),
                }
            }
            _ => return None,
        };
    }
    Some(current)
}

/// Whether the first layer takes its alpha from the material's color (or the texture factor),
/// and blends by it.
fn blends_by_color(template: &Material) -> bool {
    let Some(l) = template.layers.first() else {
        return false;
    };
    let from_color = |a: u32| matches!(a & arg::WHICH, arg::DIFFUSE | arg::FACTOR);
    l.source_blend == BLEND_SRC_ALPHA
        && l.stages.first().is_some_and(|s| {
            (s.alpha_op == op::SELECT_ARG1 && from_color(s.alpha_arg1))
                || (s.alpha_op == op::SELECT_ARG2 && from_color(s.alpha_arg2))
        })
}

/// What a look's making ready of a material (`look`) changes against the body's own (`own`).
#[must_use]
pub fn look_color(
    ac2: &Ac2,
    own: (&Material, &[MaterialModifier]),
    look: (&Material, &[MaterialModifier]),
) -> LookColor {
    let (own_color, look_color) = (
        diffuse_stage(own.0, own.1).1,
        diffuse_stage(look.0, look.1).1,
    );
    let lit = look
        .0
        .layers
        .first()
        .is_none_or(|l| l.switches.dynamic_lighting);
    let changed = look.0 != own.0 || look_color != own_color;
    let mut out = LookColor::default();
    if !changed {
        return out;
    }
    let bake = if lit {
        Some(Bake {
            a: [0, 1, 2].map(|c| {
                if own_color[c] > 1e-3 {
                    look_color[c] / own_color[c]
                } else {
                    look_color[c]
                }
            }),
            b: [0.0; 3],
        })
    } else {
        out.lost.push("unlit shading");
        stages(ac2, look.0, look.1, look_color)
    };
    out.bake = bake.filter(|b| !b.is_same());
    if look_color[3] < 1.0 && blends_by_color(look.0) {
        out.alpha = Some(look_color[3]);
    }
    let (template, modifiers) = look;
    if template
        .layers
        .first()
        .is_some_and(|l| !l.animations.is_empty())
        && stage_picture(template, modifiers, 1) != 0
    {
        out.lost.push("a scrolling second picture");
    }
    let named = |name: u32| {
        modifiers
            .iter()
            .flat_map(|m| &m.parameters)
            .any(|p| p.name == name)
            && parameter(template, modifiers, name).is_some()
    };
    if named(SPECULAR_COLOR) {
        out.lost.push("specular color");
    }
    if named(NORMAL_MAP) {
        out.lost.push("a normal map");
    }
    if named(DETAIL_MAP) {
        out.lost.push("a detail map");
    }
    out
}

/// Picture list `list`'s largest level no more than `limit` a side, its colors made over by
/// `bake`, as an image texture: its id.
///
/// # Errors
/// The picture will not read.
pub fn baked_texture(
    ac2: &Ac2,
    list: u32,
    limit: u32,
    category: u32,
    bake: &Bake,
    ids: &mut dyn NewIds,
    out: &mut Records,
) -> Result<DataId, String> {
    let p = ac2
        .picture(list, limit)
        .ok_or_else(|| format!("AC2 picture {list:08X} will not read"))?;
    let rgba = bake.apply(&p.texels);
    let bgra: Vec<u8> = rgba
        .chunks(4)
        .flat_map(|t| [t[2], t[1], t[0], t[3]])
        .collect();
    let name = format!("picture {list:08X} {limit} baked {:?}", bake.key());
    let (rs_id, st_id) = (ids.next_for(0x06, &name), ids.next_for(0x05, &name));
    let rs = RenderSurface {
        id: rs_id,
        data_category: category,
        width: p.width,
        height: p.height,
        format: PFID_A8R8G8B8,
        image_size: u32::try_from(bgra.len()).unwrap_or(u32::MAX),
        data_offset: 0x18,
        default_palette_id: None,
    };
    out.add(rs_id, rs.encode(&bgra));
    out.add(
        st_id,
        SurfaceTexture {
            id: st_id,
            data_category: 0,
            unknown_byte: 2,
            source_levels: vec![rs_id],
        }
        .encode(),
    );
    Ok(st_id)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: an AC2 look's color baked into its picture)
    use super::*;

    #[test]
    fn a_bake_scales_and_offsets_each_color_and_keeps_alpha() {
        let b = Bake {
            a: [0.5, 1.0, 2.0],
            b: [0.0, -0.25, 0.25],
        };
        assert_eq!(b.apply(&[200, 200, 100, 7]), vec![100, 136, 255, 7]);
        assert!(Bake::SAME.is_same());
        assert!(!b.is_same());
    }
}
