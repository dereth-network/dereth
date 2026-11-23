//! An AC2 material as an AC1 surface: how AC2 makes it ready (its template, the instance's and a
//! look's modifiers), whether it is drawn with its picture, in a plain color or not at all, from
//! one side or both, cut out or blended; and the surface and image texture that draw it so.

use std::collections::BTreeMap;

use auberean_assets::material::{
    Material, MaterialInstance, MaterialModifier, DIFFUSE_COLOR, DIFFUSE_MAP,
};
use auberean_world::look::PartLook;
use dereth_assets::Surface;
use dereth_primitives::DataId;

use crate::records::{NewIds, Pictures, Records};
use crate::source::Ac2;

/// The greatest side of a static's picture taken, texels: smaller than a tree's, as AC1's own
/// buildings' are, since a town holds hundreds of them.
pub const PICTURE_LIMIT: u32 = 256;
/// The image category the converter's model pictures carry.
pub const MODEL_IMAGE_CATEGORY: u32 = 4;
/// A picture is drawn cut out when this share of its texels is clear, measured on its level this
/// many texels a side (the AC2 look's rule).
pub const CLEAR_SHARE: f64 = 0.25;
pub const CLEAR_SIZE: u32 = 128;
/// AC1's surface kinds.
pub const SURFACE_IMAGE: u32 = 2;
pub const SURFACE_CLIP_MAP: u32 = 4;
pub const SURFACE_SOLID: u32 = 1;
/// Blended by the picture's alpha.
pub const SURFACE_ALPHA: u32 = 0x100;
/// How AC2 draws one material of a static's mesh, as the converter keeps it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shown {
    /// With picture list `list` (the list AC2 makes it ready with, the part's look's changes
    /// applied).
    Picture(u32),
    /// In a plain color: its diffuse stage has no picture, or one the install does not hold, and
    /// AC2 draws white there, times the material's color.
    Color([f32; 4]),
    /// Not at all: its color is clear, and AC2 blends by it.
    Clear,
}

impl Shown {
    /// The ways a material is drawn, as the tables name them.
    pub const OWN_PICTURE: &'static str = "its picture";
    pub const SMALLEST_LEVEL: &'static str = "its picture's smallest level, past the limit";
    pub const NO_PICTURE: &'static str = "a plain color: no picture in AC2";
    pub const MISSING: &'static str = "a plain color: its picture is not in the install";
    pub const UNREAD: &'static str = "a plain color: its picture's format is not converted";
    pub const CLEAR: &'static str = "not drawn: clear in AC2";
    pub const BLENDED: &'static str = "its picture, mostly clear, blended as AC2 blends it";

    /// The way and its values, ordered, to key the surfaces made by.
    pub fn key(self) -> (u8, u32, [u32; 4]) {
        match self {
            Self::Picture(l) => (0, l, [0; 4]),
            Self::Color(c) => (1, 0, c.map(f32::to_bits)),
            Self::Clear => (2, 0, [0; 4]),
        }
    }
}

/// A surface made for a material drawn one way: its id and whether it is drawn from both sides (a
/// cut-out picture, or a material AC2 does not cull; `None`: not drawn), the picture list, and how
/// it is drawn.
pub type Made = (Option<(DataId, bool)>, u32, &'static str);

/// What a model is made once for: its AC2 mesh, its kind, material and pictures, whether it moves
/// and whether its picture is its own.
pub type MadeKey = (u32, (u8, u32, [u32; 4]), bool, bool);

/// AC2's kind for a material a mesh names directly rather than through an instance.
pub const DIRECT_MATERIAL_KIND: u32 = 2;
/// The source blend that blends by the source's alpha.
pub const BLEND_SRC_ALPHA: u32 = 5;

/// A material's diffuse stage as AC2 makes it ready: the picture list it shows (0 for none) and
/// its color (red, green, blue, alpha). The template's first layer's first stage and its first
/// layer's color, then its own parameters, then each modifier's in order, a modifier's parameter
/// taken only where the template has one of its name, the last taken winning.
pub fn diffuse_stage(template: &Material, modifiers: &[MaterialModifier]) -> (u32, [f32; 4]) {
    let first = template.layers.first();
    let mut picture = first
        .and_then(|l| l.stages.first())
        .map_or(0, |s| s.texture.0);
    let mut color = first.map_or([1.0; 4], |l| {
        [l.diffuse.r, l.diffuse.g, l.diffuse.b, l.diffuse.a]
    });
    let named = |name: u32| {
        template
            .parameters
            .parameters
            .iter()
            .any(|p| p.name == name)
    };
    for p in std::iter::once(&template.parameters)
        .chain(modifiers)
        .flat_map(|m| &m.parameters)
    {
        if p.name == DIFFUSE_MAP && named(DIFFUSE_MAP) {
            if let Some(t) = p.texture() {
                picture = t.0;
            }
        } else if p.name == DIFFUSE_COLOR && named(DIFFUSE_COLOR) {
            if let Some(c) = p.color() {
                color = c;
            }
        }
    }
    (picture, color)
}

/// Material `mi` of a mesh of kind `mesh_kind` as AC2 makes it ready with `look`'s changes: its
/// template (the look's when it gives one) and its modifiers, the instance's own then the look's.
/// `None` when the install lacks the instance or the template.
pub fn made_ready(
    ac2: &Ac2,
    mi: u32,
    mesh_kind: u32,
    look: Option<&PartLook>,
) -> Option<(Material, Vec<MaterialModifier>)> {
    let (own, kind, mut modifiers) = if mi >> 24 == 0x31 {
        let i = ac2.record::<MaterialInstance>(mi)?;
        (
            i.material.0,
            i.kind,
            i.modifiers.iter().map(|m| m.0).collect::<Vec<_>>(),
        )
    } else {
        (mi, DIRECT_MATERIAL_KIND, Vec::new())
    };
    let (template, extra) = look.map_or((None, Vec::new()), |l| {
        l.material(mesh_kind, kind, &mut |a| {
            (a.modifier.is_none() || ac2.record::<MaterialModifier>(a.modifier.0).is_some())
                && (a.template.is_none() || ac2.record::<Material>(a.template.0).is_some())
        })
    });
    modifiers.extend(extra.iter().map(|m| m.0));
    let template = ac2.record::<Material>(template.map_or(own, |t| t.0))?;
    let modifiers: Vec<MaterialModifier> =
        modifiers.iter().filter_map(|&m| ac2.record(m)).collect();
    Some((template, modifiers))
}

/// A layer's cull state the template's parameters write, and AC2's cull mode that draws both
/// faces.
pub const CULL_FIELD: u32 = 5;
pub const CULL_NONE: u32 = 1;

/// Whether AC2 draws a material made ready from `template` and `modifiers` from both sides: its
/// first layer's cull mode, then each parameter the template maps onto that layer's cull state
/// (the template's own, then each modifier's of a name the template has, the last winning), whose
/// value draws both sides when it is not zero.
pub fn two_sided(template: &Material, modifiers: &[MaterialModifier]) -> bool {
    let both = template.layers.first().is_some_and(|l| l.cull == CULL_NONE);
    layer_switch(template, modifiers, CULL_FIELD, both)
}

/// A switch of the first layer's state (`field`), from `initial` as the layer stores it: each
/// parameter the template maps onto it (the template's own, then each modifier's of a name the
/// template has, the last winning) turns it on when its value is not zero, else off.
pub fn layer_switch(
    template: &Material,
    modifiers: &[MaterialModifier],
    field: u32,
    initial: bool,
) -> bool {
    let mut on = initial;
    let maps = |name: u32| {
        template.parameters.parameters.iter().any(|p| {
            p.name == name
                && p.fields
                    .iter()
                    .any(|f| f.words[0] == field && f.words[2] == 0)
        })
    };
    for p in std::iter::once(&template.parameters)
        .chain(modifiers)
        .flat_map(|m| &m.parameters)
    {
        if maps(p.name) {
            if let Some(w) = p.data.get(..4).and_then(|d| <[u8; 4]>::try_from(d).ok()) {
                on = u32::from_le_bytes(w) != 0;
            }
        }
    }
    on
}

/// The first layer's alpha test switch the template's parameters write.
pub const ALPHA_TEST_FIELD: u32 = 8;

/// Whether AC2 blends a material made ready from `template` and `modifiers` by its picture's alpha
/// rather than testing it: its first layer blends by the source's alpha, and its alpha test is off.
/// AC2 then draws every texel with some alpha, partly see-through.
pub fn blends_untested(template: &Material, modifiers: &[MaterialModifier]) -> bool {
    let Some(l) = template.layers.first() else {
        return false;
    };
    l.source_blend == BLEND_SRC_ALPHA
        && !layer_switch(template, modifiers, ALPHA_TEST_FIELD, l.alpha_test != 0)
}

/// How a material made ready from `template` and `modifiers` is drawn: not at all when its color
/// is clear and it blends by its alpha; in its color when its diffuse stage shows no picture; else
/// with its picture.
pub fn shown_by(template: &Material, modifiers: &[MaterialModifier]) -> Shown {
    let (picture, color) = diffuse_stage(template, modifiers);
    let blends = template
        .layers
        .iter()
        .any(|l| l.source_blend == BLEND_SRC_ALPHA);
    if blends && color[3] <= 0.0 {
        Shown::Clear
    } else if picture & 0x000F_FFFF == 0 {
        Shown::Color(color)
    } else {
        Shown::Picture(picture)
    }
}

/// The greatest side a picture list with levels `levels` (width, height) is taken at, for at most
/// `limit`: `limit` when a level fits it, else its smallest level's (a picture stored at one size
/// only, larger than the limit); `None` when it has no level.
pub fn taken_at(levels: &[(u32, u32)], limit: u32) -> Option<u32> {
    let sides = levels.iter().map(|(w, h)| *w.max(h));
    if sides.clone().any(|s| s <= limit) {
        Some(limit)
    } else {
        sides.min()
    }
}

/// An AC1 surface in a plain color, red, green, blue from 0 to 1.
pub fn plain(color: [f32; 4]) -> Surface {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let c = |v: f32| u32::from((v.clamp(0.0, 1.0) * 255.0).round() as u8);
    Surface {
        surface_type: SURFACE_SOLID,
        orig_texture_id: None,
        orig_palette_id: None,
        color_value: Some(0xFF00_0000 | c(color[0]) << 16 | c(color[1]) << 8 | c(color[2])),
        translucency: 0.0,
        luminosity: 0.0,
        diffuse: 1.0,
    }
}

/// The surface material `mi` of a mesh of kind `mesh_kind` draws with as AC2 makes it ready with
/// `look`'s changes: its diffuse picture (when mostly clear, blended where AC2 blends it by its
/// alpha, else cut out), or a plain color where AC2 shows none; `None` where AC2 draws it clear.
/// Also how it is drawn, and the picture list.
#[allow(clippy::too_many_arguments)]
pub fn surface_of(
    ac2: &Ac2,
    mi: u32,
    mesh_kind: u32,
    look: Option<&PartLook>,
    pictures: &mut Pictures,
    ids: &mut dyn NewIds,
    out: &mut Records,
    made: &mut BTreeMap<MadeKey, Made>,
) -> Result<Made, String> {
    let ready = made_ready(ac2, mi, mesh_kind, look);
    let how = ready
        .as_ref()
        .map_or(Shown::Color([1.0; 4]), |(t, m)| shown_by(t, m));
    let both = ready.as_ref().is_some_and(|(t, m)| two_sided(t, m));
    let blends = ready.as_ref().is_some_and(|(t, m)| blends_untested(t, m));
    if let Some(s) = made.get(&(mi, how.key(), both, blends)) {
        return Ok(*s);
    }
    let list = if let Shown::Picture(l) = how { l } else { 0 };
    let levels = ac2.levels(list);
    let side = taken_at(&levels, PICTURE_LIMIT);
    let mut way = match how {
        Shown::Clear => Shown::CLEAR,
        Shown::Color(_) => Shown::NO_PICTURE,
        Shown::Picture(_) => match side {
            None => Shown::MISSING,
            Some(s) if ac2.picture(list, s).is_none() => Shown::UNREAD,
            Some(s) if s > PICTURE_LIMIT => Shown::SMALLEST_LEVEL,
            Some(_) => Shown::OWN_PICTURE,
        },
    };
    let (surface, cut) = match (how, side) {
        (Shown::Clear, _) => {
            made.insert((mi, how.key(), both, blends), (None, list, way));
            return Ok((None, list, way));
        }
        (Shown::Picture(_), Some(side)) if ac2.picture(list, side).is_some() => {
            let cut = taken_at(&levels, CLEAR_SIZE)
                .and_then(|s| ac2.picture(list, s))
                .is_some_and(|p| p.clear_share() >= CLEAR_SHARE);
            let (tex, _) = pictures.texture(ac2, list, side, MODEL_IMAGE_CATEGORY, ids, out)?;
            // A mostly clear picture AC2 blends is blended here too: cut out at AC1's alpha
            // reference, its partly clear texels (a sail's canvas) would be thrown away.
            if cut && blends && way == Shown::OWN_PICTURE {
                way = Shown::BLENDED;
            }
            (
                Surface {
                    surface_type: match (cut, blends) {
                        (true, true) => SURFACE_IMAGE | SURFACE_ALPHA,
                        (true, false) => SURFACE_CLIP_MAP,
                        (false, _) => SURFACE_IMAGE,
                    },
                    orig_texture_id: Some(tex),
                    orig_palette_id: Some(DataId(0)),
                    color_value: None,
                    translucency: 0.0,
                    luminosity: 0.0,
                    diffuse: 1.0,
                },
                cut,
            )
        }
        (Shown::Color(c), _) => (plain(c), false),
        // AC2 draws white where it cannot load the picture.
        (Shown::Picture(_), _) => (plain([1.0; 4]), false),
    };
    let id = ids.next_for(
        0x08,
        &format!("surface {mi:08X} {:?} {both} {blends}", how.key()),
    );
    out.add(id, surface.encode());
    // A cut-out picture is drawn from each side, as is a material AC2 draws without culling.
    let sides = cut || both;
    made.insert(
        (mi, how.key(), both, blends),
        (Some((id, sides)), list, way),
    );
    Ok((Some((id, sides)), list, way))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: an AC2 material as an AC1 surface)
    use super::*;

    fn parameter(
        name: u32,
        data_type: u32,
        data: Vec<u8>,
    ) -> auberean_assets::material::MaterialParameter {
        auberean_assets::material::MaterialParameter {
            name,
            data_type,
            data,
            fields: Vec::new(),
        }
    }

    fn picture(name: u32, list: u32) -> auberean_assets::material::MaterialParameter {
        parameter(
            name,
            auberean_assets::material::TEXTURE_DATA,
            list.to_le_bytes().to_vec(),
        )
    }

    fn color(c: [f32; 4]) -> auberean_assets::material::MaterialParameter {
        parameter(
            DIFFUSE_COLOR,
            auberean_assets::material::COLOR_DATA,
            c.iter().flat_map(|f| f.to_le_bytes()).collect(),
        )
    }

    fn modifier(parameters: Vec<auberean_assets::material::MaterialParameter>) -> MaterialModifier {
        MaterialModifier { id: 0, parameters }
    }

    /// A template as AC2's statics' are: an empty diffuse picture and white, blended by alpha.
    fn template() -> Material {
        let layer = auberean_assets::material::MaterialLayer {
            source_blend: BLEND_SRC_ALPHA,
            stages: vec![auberean_assets::material::TextureStage::default()],
            diffuse: auberean_primitives::Rgba {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
            ..Default::default()
        };
        Material {
            parameters: modifier(vec![picture(DIFFUSE_MAP, 0x4000_0000), color([1.0; 4])]),
            layers: vec![layer],
            sort_mode: 1,
        }
    }

    /// A layer-state parameter `name` the template maps onto its first layer's cull state.
    fn cull_parameter(name: u32, on: u32) -> auberean_assets::material::MaterialParameter {
        auberean_assets::material::MaterialParameter {
            name,
            data_type: 4000,
            data: on.to_le_bytes().to_vec(),
            fields: vec![auberean_assets::material::ParameterField {
                words: [CULL_FIELD, 4000, 0, 0],
            }],
        }
    }

    #[test]
    fn a_material_is_drawn_from_both_sides_as_its_cull_parameter_says_the_last_one_winning() {
        // The tent's: its layer culls (2), and the template's parameter 4003 turns that off.
        let mut t = template();
        t.layers[0].cull = 2;
        assert!(!two_sided(&t, &[]), "its layer culls");
        t.parameters.parameters.push(cull_parameter(4003, 1));
        assert!(two_sided(&t, &[]), "the template's own parameter");
        let off = modifier(vec![parameter(4003, 4000, 0u32.to_le_bytes().to_vec())]);
        let on = modifier(vec![parameter(4003, 4000, 1u32.to_le_bytes().to_vec())]);
        assert!(
            !two_sided(&t, &[on.clone(), off.clone()]),
            "the last modifier's"
        );
        assert!(two_sided(&t, &[off, on]));
        // A modifier's parameter of a name the template does not map onto the cull state does
        // nothing; a layer that does not cull is drawn from both sides.
        let mut plain = template();
        plain.layers[0].cull = 2;
        let stray = modifier(vec![parameter(4003, 4000, 1u32.to_le_bytes().to_vec())]);
        assert!(!two_sided(&plain, &[stray]));
        plain.layers[0].cull = CULL_NONE;
        assert!(two_sided(&plain, &[]));
    }

    #[test]
    fn a_material_blended_by_its_alpha_is_blended_unless_its_alpha_test_is_switched_on() {
        // The template blends by the source's alpha and tests nothing: AC2 blends it.
        let mut t = template();
        assert!(blends_untested(&t, &[]));
        // A parameter the template maps onto the alpha test turns it on, the last one winning.
        let mut test = cull_parameter(4010, 1);
        test.fields[0].words[0] = ALPHA_TEST_FIELD;
        t.parameters.parameters.push(test);
        assert!(!blends_untested(&t, &[]));
        let off = modifier(vec![parameter(4010, 4000, 0u32.to_le_bytes().to_vec())]);
        assert!(blends_untested(&t, &[off]));
        // A layer that tests alpha itself, or does not blend by it, is not blended.
        let mut tested = template();
        tested.layers[0].alpha_test = 1;
        assert!(!blends_untested(&tested, &[]));
        let mut opaque = template();
        opaque.layers[0].source_blend = 2;
        assert!(!blends_untested(&opaque, &[]));
    }

    #[test]
    fn a_material_shows_the_picture_its_last_modifier_names_the_looks_after_the_instances() {
        let own = modifier(vec![
            picture(DIFFUSE_MAP, 0x4010_0061),
            picture(3011, 0x4010_0062),
        ]);
        let look = modifier(vec![picture(DIFFUSE_MAP, 0x4010_0063)]);
        assert_eq!(
            shown_by(&template(), std::slice::from_ref(&own)),
            Shown::Picture(0x4010_0061)
        );
        assert_eq!(
            shown_by(&template(), &[own, look]),
            Shown::Picture(0x4010_0063)
        );
    }

    #[test]
    fn a_material_whose_modifiers_name_no_picture_is_drawn_in_its_color() {
        let none = modifier(vec![picture(DIFFUSE_MAP, 0)]);
        assert_eq!(shown_by(&template(), &[none]), Shown::Color([1.0; 4]));
        assert_eq!(shown_by(&template(), &[]), Shown::Color([1.0; 4]));
    }

    #[test]
    fn a_material_made_clear_is_not_drawn_whatever_its_picture() {
        let clear = modifier(vec![picture(DIFFUSE_MAP, 0), color([1.0, 1.0, 1.0, 0.0])]);
        assert_eq!(shown_by(&template(), &[clear]), Shown::Clear);
        let pictured = modifier(vec![picture(DIFFUSE_MAP, 0x4000_0315)]);
        let look = modifier(vec![color([1.0, 1.0, 1.0, 0.0])]);
        assert_eq!(shown_by(&template(), &[pictured, look]), Shown::Clear);
        let mut opaque = template();
        opaque.layers[0].source_blend = 2;
        let clear = modifier(vec![color([1.0, 1.0, 1.0, 0.0])]);
        assert_eq!(
            shown_by(&opaque, &[clear]),
            Shown::Color([1.0, 1.0, 1.0, 0.0])
        );
    }

    #[test]
    fn a_modifier_changes_only_what_its_template_names() {
        let mut bare = template();
        bare.parameters.parameters.clear();
        bare.layers[0].stages[0].texture = auberean_primitives::DataId(0x4000_0111);
        let m = modifier(vec![picture(DIFFUSE_MAP, 0x4000_0222), color([0.0; 4])]);
        assert_eq!(shown_by(&bare, &[m]), Shown::Picture(0x4000_0111));
    }

    #[test]
    fn a_picture_with_no_level_within_the_limit_is_taken_at_its_smallest() {
        assert_eq!(taken_at(&[(512, 256)], 256), Some(512));
        assert_eq!(
            taken_at(&[(1024, 1024), (512, 512), (256, 256)], 256),
            Some(256)
        );
        assert_eq!(taken_at(&[(2048, 1024), (1024, 512)], 256), Some(1024));
        assert_eq!(taken_at(&[], 256), None);
    }

    #[test]
    fn a_plain_color_is_written_as_its_red_green_and_blue() {
        assert_eq!(plain([1.0, 0.5, 0.0, 1.0]).color_value, Some(0xFFFF_8000));
        assert_eq!(plain([1.0; 4]).surface_type, SURFACE_SOLID);
    }
}
