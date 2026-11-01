//! A material's layers as the device draws them: specular, coordinate transforms, modifiers and
//! the pixel programs' options.

use auberean_assets::common::Waveform;
use auberean_assets::material::{
    CoordinateAnimation, LayerSwitches, Material, MaterialLayer, MaterialModifier,
    MaterialParameter, ParameterField, StageAnimation, TextureStage,
};
use auberean_draw::material::{apply_modifier, draw_layers, stage_uv_transform, PixelProgram};

fn steady(v: f32) -> Waveform {
    Waveform {
        kind: 1,
        base: v,
        amplitude: 1.0,
        frequency: 1.0,
        ..Waveform::default()
    }
}

fn waveform_bytes(v: f32) -> Vec<u8> {
    let mut b = 1u32.to_le_bytes().to_vec();
    for x in [v, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0] {
        b.extend_from_slice(&f32::to_le_bytes(x));
    }
    b
}

fn lit_layer(power: f32) -> MaterialLayer {
    MaterialLayer {
        stages: vec![TextureStage::default(); 3],
        source_blend: 2,
        dest_blend: 1,
        switches: LayerSwitches {
            dynamic_lighting: true,
            specular_highlights: true,
            material_diffuse: true,
            ..LayerSwitches::default()
        },
        specular: auberean_primitives::Rgba {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        },
        specular_power: steady(power),
        ..MaterialLayer::default()
    }
}

/// A layer whose specular power is zero draws no specular light at all, even with highlights
/// switched on: its specular color and power both go to zero. A non-zero power keeps them.
///
/// Behaviour: draw.material.specular-is-off-at-zero-power
#[test]
fn specular_is_off_at_zero_power() {
    let material = |power| Material {
        layers: vec![lit_layer(power)],
        ..Material::default()
    };
    let off = &draw_layers(&material(0.0), 0)[0];
    assert_eq!(
        (off.specular.r, off.specular.g, off.specular.b),
        (0.0, 0.0, 0.0)
    );
    assert_eq!(off.specular_power, 0.0);
    let on = &draw_layers(&material(20.0), 0)[0];
    assert_eq!((on.specular.r, on.specular_power), (1.0, 20.0));
}

/// A modifier's specular power and its scale of a stage's coordinates reach the layer, through
/// the material's own parameters of the same names.
///
/// Behaviour: draw.material.a-modifier-sets-power-and-coordinate-scale
#[test]
fn a_modifier_sets_power_and_coordinate_scale() {
    let field = |kind, layer, index| ParameterField {
        words: [kind, 1000, layer, index],
    };
    let mut layer = lit_layer(0.0);
    layer.animations = vec![StageAnimation {
        stage: 2,
        animation: CoordinateAnimation::Scale {
            u: steady(10.0),
            v: steady(10.0),
        },
    }];
    let mut m = Material {
        parameters: MaterialModifier {
            id: 0,
            parameters: vec![
                MaterialParameter {
                    name: 1000,
                    data_type: 1000,
                    data: waveform_bytes(0.0),
                    fields: vec![field(3, 0, 0)],
                },
                MaterialParameter {
                    name: 1041,
                    data_type: 1000,
                    data: waveform_bytes(10.0),
                    fields: vec![field(36, 0, 0)],
                },
            ],
        },
        layers: vec![layer],
        sort_mode: 0,
    };
    apply_modifier(
        &mut m,
        &MaterialModifier {
            id: 0,
            parameters: vec![
                MaterialParameter {
                    name: 1000,
                    data_type: 1000,
                    data: waveform_bytes(35.0),
                    fields: Vec::new(),
                },
                MaterialParameter {
                    name: 1041,
                    data_type: 1000,
                    data: waveform_bytes(4.0),
                    fields: Vec::new(),
                },
            ],
        },
    );
    let l = &draw_layers(&m, 0)[0];
    assert_eq!(l.specular_power, 35.0);
    assert_eq!(
        stage_uv_transform(&m.layers[0], 2),
        [4.0, 0.0, 0.0, 0.0, 10.0, 0.0],
        "u scaled by the modifier's 4, v by the material's own 10"
    );
}

/// A stage's coordinate animations apply in list order: a scale then a slide moves the scaled
/// coordinates; a slide then a scale scales the slide too. Other stages are untouched.
///
/// Behaviour: draw.material.coordinate-animations-apply-in-order
#[test]
fn coordinate_animations_apply_in_order() {
    let scale = StageAnimation {
        stage: 0,
        animation: CoordinateAnimation::Scale {
            u: steady(2.0),
            v: steady(3.0),
        },
    };
    let slide = StageAnimation {
        stage: 0,
        animation: CoordinateAnimation::Translate {
            u: steady(0.5),
            v: steady(0.25),
        },
    };
    let mut l = MaterialLayer {
        animations: vec![scale.clone(), slide.clone()],
        ..MaterialLayer::default()
    };
    assert_eq!(stage_uv_transform(&l, 0), [2.0, 0.0, 0.5, 0.0, 3.0, 0.25]);
    l.animations = vec![slide, scale];
    assert_eq!(stage_uv_transform(&l, 0), [2.0, 0.0, 1.0, 0.0, 3.0, 0.75]);
    assert_eq!(stage_uv_transform(&l, 1), [1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
}

/// A pixel program's options are read from its instructions: a detail map added, signed, to
/// the diffuse map or to the normal map, the diffuse map's alpha in the layer's alpha, a gloss
/// map scaling the specular, and an environment map added to the diffuse map, alone or times the
/// diffuse map's alpha. Comments and spacing do not matter.
///
/// Behaviour: draw.material.a-pixel-program-is-read-from-its-instructions
#[test]
fn a_pixel_program_is_read_from_its_instructions() {
    let plain = "ps.1.1\ntex t0\ntex t1\ndp3_sat r0.rgb, t0_bx2, v0_bx2\n";
    assert_eq!(
        PixelProgram::from_text(plain.as_bytes()),
        PixelProgram::default()
    );
    let detail =
        "ps.1.1\ntex t2 ; detail\nadd t1.rgb, t1, t2_bias ; signed\n+mul r0.a, c4.a, t1.a\n";
    let p = PixelProgram::from_text(detail.as_bytes());
    assert!(p.detail_on_diffuse && p.alpha_from_diffuse && !p.detail_on_normal && !p.gloss);
    let bumpy = "add  t0.rgb,t0,t2_bias\nmad r0.rgb, r1, t1.a, r0 ; gloss\n";
    let p = PixelProgram::from_text(bumpy.as_bytes());
    assert!(p.detail_on_normal && p.gloss && !p.detail_on_diffuse);
    let shiny = "tex t2 ; environment map\nadd t1.rgb, t1, t2 ; add environment map\n";
    let p = PixelProgram::from_text(shiny.as_bytes());
    assert!(p.environment_add && !p.environment_gloss && !p.detail_on_diffuse);
    let glossy = "mad t1.rgb, t2, t1.a, t1 ; gloss/add environment map\n";
    let p = PixelProgram::from_text(glossy.as_bytes());
    assert!(p.environment_gloss && !p.environment_add && !p.gloss);
}

/// A coordinate-animation field sets the animation in that place of the layer's list by its
/// place in the animation, whatever its kind: the first waveform (a slide's u, a turn, a
/// stretch's u) or the second (a slide's or a stretch's v).
///
/// Behaviour: draw.material.an-animation-field-sets-its-first-or-second-waveform
#[test]
fn an_animation_field_sets_its_first_or_second_waveform() {
    let field = |kind, index| ParameterField {
        words: [kind, 1000, 0, index],
    };
    let mut layer = lit_layer(0.0);
    layer.animations = vec![
        StageAnimation {
            stage: 0,
            animation: CoordinateAnimation::Rotate(steady(1.0)),
        },
        StageAnimation {
            stage: 0,
            animation: CoordinateAnimation::Scale {
                u: steady(1.0),
                v: steady(1.0),
            },
        },
    ];
    let param = |name: u32, fields: Vec<ParameterField>, v: f32| MaterialParameter {
        name,
        data_type: 1000,
        data: waveform_bytes(v),
        fields,
    };
    let mut m = Material {
        parameters: MaterialModifier {
            id: 0,
            parameters: vec![
                param(1, vec![field(35, 0)], 0.0),
                param(2, vec![field(33, 1)], 0.0),
                param(3, vec![field(37, 1)], 0.0),
            ],
        },
        layers: vec![layer],
        sort_mode: 0,
    };
    apply_modifier(
        &mut m,
        &MaterialModifier {
            id: 0,
            parameters: vec![
                param(1, Vec::new(), 30.0),
                param(2, Vec::new(), 2.0),
                param(3, Vec::new(), 3.0),
            ],
        },
    );
    let a = &m.layers[0].animations;
    assert_eq!(a[0].animation, CoordinateAnimation::Rotate(steady(30.0)));
    assert_eq!(
        a[1].animation,
        CoordinateAnimation::Scale {
            u: steady(2.0),
            v: steady(3.0)
        }
    );
}

/// A dynamically lit layer has a vertex program for each count up to one directional and four
/// point lights, never neither, leaving out a kind it ignores; a layer lit per pixel by one light
/// has a directional and a point program; any other one program with no light. A device with more
/// lights than any program takes uses the program with the most point lights it has (the most
/// directional lights when it has fewer point lights than directional ones), and with none at all
/// the first program.
///
/// Behaviour: draw.material.a-layers-programs-cover-its-light-counts
#[test]
fn a_layers_programs_cover_its_light_counts() {
    use auberean_draw::material::{light_variants, switch, variant_for};
    let lit = light_variants(switch::DYNAMIC_LIGHTING);
    assert_eq!(lit.len(), 9);
    assert_eq!(lit[0], (0, 1));
    assert!(!lit.contains(&(0, 0)));
    // The interior cells' material ignores directional lights.
    let cell = light_variants(
        switch::DYNAMIC_LIGHTING | switch::VERTEX_DIFFUSE | switch::IGNORE_DIRECTIONAL_LIGHTS,
    );
    assert_eq!(cell, [(0, 1), (0, 2), (0, 3), (0, 4)]);
    assert_eq!(light_variants(switch::PIXEL_LIGHTING), [(1, 0), (0, 1)]);
    assert_eq!(
        light_variants(switch::PIXEL_LIGHTING | switch::PIXEL_MULTIPLE_LIGHTS).len(),
        9
    );
    assert_eq!(light_variants(switch::MATERIAL_DIFFUSE), [(0, 0)]);
    // Six point lights about a cell: by its switches alone, its program takes four.
    assert_eq!(variant_for(&cell, 0, 6).map(|k| cell[k]), Some((0, 4)));
    assert_eq!(variant_for(&cell, 0, 2).map(|k| cell[k]), Some((0, 2)));
    // No light: the first program.
    assert_eq!(variant_for(&cell, 0, 0), Some(0));
    // The sun alone, on a layer that ignores it: still the first.
    assert_eq!(variant_for(&cell, 1, 0), Some(0));
    // More directional than point lights: the most directional lights it has.
    assert_eq!(variant_for(&lit, 2, 1).map(|k| lit[k]), Some((1, 0)));
    assert_eq!(variant_for(&[], 1, 1), None);
}
