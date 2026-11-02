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

/// A modifier sets a layer's alpha reference, a stage's texture and its addressing, and the
/// layer's culling, depth test and depth writing as the client does: the reference's waveform
/// whole, the texture by its id, an address of zero clamped and any other wrapped, a culling of
/// zero one-sided and any other two-sided, a depth test of zero always passing and any other less
/// or equal, a depth writing on for any value but zero.
///
/// Behaviour: draw.material.a-modifier-sets-a-layers-reference-texture-and-switches
#[test]
fn a_modifier_sets_a_layers_reference_texture_and_switches() {
    use auberean_draw::material::d3d;
    let word = |v: u32| v.to_le_bytes().to_vec();
    let param = |name, data: Vec<u8>, field: Option<[u32; 4]>| MaterialParameter {
        name,
        data_type: 1000,
        data,
        fields: field
            .map(|w| vec![ParameterField { words: w }])
            .unwrap_or_default(),
    };
    let mut m = Material {
        parameters: MaterialModifier {
            id: 0,
            parameters: vec![
                param(1, waveform_bytes(0.1), Some([9, 1000, 0, 0])),
                param(2, word(0x0500_0001), Some([10, 1000, 0, 1])),
                param(3, word(1), Some([11, 1000, 0, 1])),
                param(4, word(1), Some([5, 1000, 0, 0])),
                param(5, word(1), Some([6, 1000, 0, 0])),
                param(6, word(1), Some([7, 1000, 0, 0])),
            ],
        },
        layers: vec![lit_layer(0.0)],
        sort_mode: 0,
    };
    apply_modifier(
        &mut m,
        &MaterialModifier {
            id: 0,
            parameters: vec![
                param(1, waveform_bytes(0.5), None),
                param(2, word(0x0500_1234), None),
                param(3, word(0), None),
                param(4, word(7), None),
                param(5, word(0), None),
                param(6, word(0), None),
            ],
        },
    );
    let l = &m.layers[0];
    assert_eq!(l.alpha_test_ref.base, 0.5);
    assert_eq!(
        l.stages[1].texture,
        auberean_primitives::DataId(0x0500_1234)
    );
    assert_eq!(l.stages[1].address_u, d3d::ADDRESS_CLAMP);
    assert_eq!(l.cull, d3d::CULL_NONE);
    assert_eq!(l.depth_test, d3d::CMP_ALWAYS);
    assert_eq!(l.depth_write, 0);
}

/// A device draws the layers made for its material detail and its fog: of four layers, one for
/// each detail and one for high detail with fog off, the device at high detail with fog on draws
/// the high detail's alone, at medium the medium's and at low the low's; one kept from neither
/// draws only what is for every detail.
///
/// Behaviour: draw.material.a-device-draws-the-layers-of-its-detail-and-fog
#[test]
fn a_device_draws_the_layers_of_its_detail_and_fog() {
    use auberean_draw::material::{device_flags, DEVICE_FLAGS};
    let layer = |set: u32, clear: u32, power: f32| MaterialLayer {
        flags_set: set,
        flags_clear: clear,
        ..lit_layer(power)
    };
    let m = Material {
        layers: vec![
            layer(0x1_0000, 0, 1.0),
            layer(0x2_0000, 0, 2.0),
            layer(0x4_0000, 0, 3.0),
            layer(0x4_0000, 0x8_0000, 4.0),
        ],
        ..Material::default()
    };
    let drawn = draw_layers(&m, DEVICE_FLAGS);
    assert_eq!(drawn.len(), 1);
    assert_eq!(drawn[0].specular_power, 3.0);
    let power = |detail: u32| {
        draw_layers(&m, device_flags(detail))
            .iter()
            .map(|l| l.specular_power)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        [0, 1, 2, 3].map(power),
        [vec![1.0], vec![2.0], vec![3.0], vec![3.0]]
    );
    let every = Material {
        layers: vec![layer(0, 0, 5.0), layer(0x1_0000, 0, 6.0)],
        ..Material::default()
    };
    let drawn = draw_layers(&every, DEVICE_FLAGS);
    assert_eq!(
        drawn.iter().map(|l| l.specular_power).collect::<Vec<_>>(),
        [5.0]
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

/// A turn and a stored matrix move a stage's coordinates as the client's do: a turn by its
/// waveform in degrees, counterclockwise as the coordinates run (a quarter turn takes (1, 0) to
/// (0, 1) and (0, 1) to (-1, 0)), applied after what came before it; a stored matrix by its first
/// two columns, its fourth row the slide.
///
/// Behaviour: draw.material.a-turn-and-a-stored-matrix-move-coordinates-as-the-clients-do
#[test]
fn a_turn_and_a_stored_matrix_move_coordinates_as_the_clients_do() {
    let at = |m: [f32; 6], u: f32, v: f32| {
        let p = (m[0] * u + m[1] * v + m[2], m[3] * u + m[4] * v + m[5]);
        ((p.0 * 1e4).round() / 1e4, (p.1 * 1e4).round() / 1e4)
    };
    let turn = StageAnimation {
        stage: 0,
        animation: CoordinateAnimation::Rotate(steady(90.0)),
    };
    let slide = StageAnimation {
        stage: 0,
        animation: CoordinateAnimation::Translate {
            u: steady(0.5),
            v: steady(0.0),
        },
    };
    let mut l = MaterialLayer {
        animations: vec![turn.clone()],
        ..MaterialLayer::default()
    };
    let m = stage_uv_transform(&l, 0);
    assert_eq!(at(m, 1.0, 0.0), (0.0, 1.0));
    assert_eq!(at(m, 0.0, 1.0), (-1.0, 0.0));
    // slid, then turned: the slide turns with it
    l.animations = vec![slide, turn];
    assert_eq!(at(stage_uv_transform(&l, 0), 0.0, 0.0), (0.0, 0.5));
    // a stored matrix: u' = u m00 + v m10 + m30, v' = u m01 + v m11 + m31
    let mut t = [0.0f32; 16];
    (t[0], t[1], t[4], t[5], t[12], t[13], t[10], t[15]) =
        (2.0, 0.5, 0.25, 3.0, 0.1, 0.2, 1.0, 1.0);
    l.animations = vec![StageAnimation {
        stage: 0,
        animation: CoordinateAnimation::Transform(t),
    }];
    assert_eq!(at(stage_uv_transform(&l, 0), 1.0, 0.0), (2.1, 0.7));
    assert_eq!(at(stage_uv_transform(&l, 0), 0.0, 1.0), (0.35, 3.2));
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
