//! Behaviour: none (experimental hi-fi art packs: which retail draws an art pack's material for a
//! retail picture is drawn on, and how)

use std::sync::Arc;

use dereth_render::device::TextureSlot;
use dereth_render::{PipelineKey, VertexFormat};
use dereth_render_hifi::derive::cache::catalogue_keys;
use dereth_render_hifi::derive::material::{MATERIAL_OUT, ROUGHNESS_CODES};
use dereth_render_hifi::derive::reshade::{MaterialClass, PIXEL_STAGES};
use dereth_render_hifi::derive::{self, scope_range, DerivedKey, ModuleKey, Scope, Variant};
use dereth_render_hifi::materials::{drawn, material_draws, material_key};
use dereth_render_hifi::reshade::derived_key;
use dereth_render_hifi::snapshot::{HifiMaterials, HifiTextureMaterial};
use dereth_render_hifi::{BindNote, DrawNote, HifiFrame, HifiSettings, Level, Mark, SideTables};

use crate::common::wgsl_eval::{Evaluator, Value};

/// The slot of the retail picture the material stands for, and another picture's.
const WALL: u32 = 41;
const FLOOR: u32 = 42;

fn note(cmd: u32, key: PipelineKey) -> DrawNote {
    DrawNote {
        cmd,
        key,
        splat: false,
        frame: 0,
        draw: 0,
        vertices: 3,
    }
}

fn bind(cmd: u32, group: u32, slot: u32) -> BindNote {
    BindNote {
        cmd,
        group,
        slot: TextureSlot(slot),
        sampler: 0,
    }
}

/// A world of twenty commands: the wall's picture bound at 2, three draws; the floor's at 6, two
/// draws; the wall's again at 9, a second stage's texture bound at 10 (which changes nothing of
/// the first stage), three draws; a landscape splat at 14; and the wall's draws at 16 and 17 those
/// of an object an art pack has a model for.
fn world() -> SideTables {
    let key = catalogue_keys()[0];
    let mut t = SideTables {
        marks: vec![
            (1, Mark::WorldBegin),
            (16, Mark::ArtPackCovered),
            (18, Mark::ArtPackCoveredEnd),
            (20, Mark::WorldEnd),
        ],
        binds: vec![
            bind(2, 1, WALL),
            bind(6, 1, FLOOR),
            bind(9, 1, WALL),
            bind(10, 2, FLOOR),
        ],
        ..SideTables::default()
    };
    for cmd in [3, 4, 5, 7, 8, 11, 12, 13, 14, 16, 17, 19] {
        let mut n = note(cmd, key);
        n.splat = cmd == 14;
        t.draws.push(n);
    }
    t
}

#[test]
fn a_draw_with_a_retail_picture_a_material_stands_for_is_drawn_with_that_material() {
    let t = world();
    let drawn = material_draws(&t, &[(WALL, 0)], 1..20);
    // Every draw while the wall's picture is the first stage's texture, but the splat's and the
    // covered object's.
    assert_eq!(
        drawn,
        [(3, 0), (4, 0), (5, 0), (11, 0), (12, 0), (13, 0), (19, 0)]
    );
    // Two materials: each draw its own.
    let both = material_draws(&t, &[(WALL, 0), (FLOOR, 3)], 1..20);
    assert!(both.contains(&(7, 3)) && both.contains(&(8, 3)), "{both:?}");
    assert!(both.contains(&(3, 0)));
    // Only the commands re-shaded.
    assert_eq!(material_draws(&t, &[(WALL, 0)], 5..12), [(5, 0), (11, 0)]);
    // A picture no material stands for keeps retail's.
    assert!(material_draws(&t, &[(99, 0)], 1..20).is_empty());
    assert!(material_draws(&t, &[], 1..20).is_empty());
}

#[test]
fn an_object_the_art_pack_has_a_model_for_is_never_drawn_with_a_material() {
    let t = world();
    let drawn = material_draws(&t, &[(WALL, 0)], 1..20);
    assert!(
        !drawn.iter().any(|(cmd, _)| (16..18).contains(cmd)),
        "{drawn:?}"
    );
    // Without the object's note the same draws take the material.
    let mut open = world();
    open.marks
        .retain(|(_, m)| !matches!(m, Mark::ArtPackCovered | Mark::ArtPackCoveredEnd));
    let drawn = material_draws(&open, &[(WALL, 0)], 1..20);
    assert!(
        drawn.contains(&(16, 0)) && drawn.contains(&(17, 0)),
        "{drawn:?}"
    );
}

fn with_materials() -> HifiFrame {
    HifiFrame {
        materials: Some(Arc::new(HifiMaterials {
            serial: 1,
            materials: vec![HifiTextureMaterial::default()],
        })),
        material_slots: vec![(WALL, 0)],
        ..HifiFrame::default()
    }
}

#[test]
fn the_retail_look_draws_no_material() {
    let lit = HifiSettings {
        lighting: Level::Low,
        art_pack: true,
        ..HifiSettings::default()
    };
    assert!(drawn(&lit, &with_materials()));
    // The retail look asked for: the scene hands the presentation no art pack to draw.
    let retail_look = HifiSettings {
        art_pack: false,
        ..lit
    };
    assert!(!drawn(&retail_look, &with_materials()));
    // A pack with no materials, or none at all.
    assert!(!drawn(&lit, &HifiFrame::default()));
    let empty = HifiFrame {
        materials: Some(Arc::new(HifiMaterials::default())),
        ..HifiFrame::default()
    };
    assert!(!drawn(&lit, &empty));
}

#[test]
fn a_draw_with_a_material_keeps_its_surfaces_blend_cut_out_depth_and_cull() {
    let keys: Vec<PipelineKey> = catalogue_keys();
    for k in &keys {
        let n = note(3, *k);
        let m = material_key(&n);
        assert_eq!(m.key, *k, "the recorded draw's own state");
        assert_eq!(m.variant, Variant::Material);
        let reshade = derived_key(&n);
        // The same color and normal targets as the draw re-shaded without the material: the
        // same blend, and the normal written exactly where it is.
        assert_eq!(
            m.targets(dereth_render::wgpu::sidecar::ordinary_blend(k)),
            reshade.targets(dereth_render::wgpu::sidecar::ordinary_blend(k))
        );
    }
    assert!(
        keys.iter().any(|k| k.alpha_blend) && keys.iter().any(|k| k.alpha_test),
        "the catalogue holds blended and cut-out surfaces"
    );
}

fn material_modules() -> Vec<ModuleKey> {
    ModuleKey::all(Variant::Material)
        .filter(|m| !m.splat)
        .collect()
}

#[test]
fn every_material_shader_keeps_each_stages_alpha_test_and_ends_in_the_materials_output() {
    for m in material_modules() {
        let text = derive::derive(m).unwrap_or_else(|e| panic!("{m:?}: {e}"));
        for stage in PIXEL_STAGES {
            let range = scope_range(&text, Scope::Function(stage))
                .expect("one function")
                .unwrap_or_else(|| panic!("{m:?}: no {stage}"));
            let body = &text[range];
            assert!(
                body.contains("hifi_alpha_test(c.a, i.pos.xy);"),
                "{m:?}: {stage}"
            );
            assert!(body.contains(MATERIAL_OUT), "{m:?}: {stage}");
            assert!(!body.contains("return hifi_out("), "{m:?}: {stage}");
        }
        // The material's color is sampled where the retail picture was: the first stage's
        // texture, with its own sampler and coordinates.
        assert!(text.contains("(g_tex0, g_samp0, i.uv0"));
        let module = naga::front::wgsl::parse_str(&text)
            .unwrap_or_else(|e| panic!("{m:?}: {}", e.emit_to_string(&text)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{m:?}: {}", e.emit_to_string(&text)));
    }
    // The landscape is never drawn with one.
    let splat = ModuleKey {
        splat: true,
        ..material_modules()[0]
    };
    assert!(derive::derive(splat).is_err());
}

fn shader() -> Evaluator {
    let module = ModuleKey {
        format: VertexFormat::XyzNormalDiffuseTex1,
        splat: false,
        variant: Variant::Material,
    };
    Evaluator::new(&derive::derive(module).expect("derives"))
}

#[test]
fn a_flat_texel_of_the_normal_picture_leaves_the_normal_and_a_tilted_one_tilts_it_its_way() {
    let s = shader();
    let flat = s
        .call("hifi_mat_bend", &[Value::vector(&[0.5, 0.5, 0.0, 1.0])])
        .floats();
    for (got, want) in flat.iter().zip([0.0, 0.0, 1.0]) {
        assert!((got - want).abs() < 0.01, "{flat:?}");
    }
    // Leaning toward the picture's right and up it.
    let leaning = s
        .call("hifi_mat_bend", &[Value::vector(&[0.75, 0.75, 0.0, 1.0])])
        .floats();
    assert!(
        leaning[0] > 0.45 && leaning[1] > 0.45 && leaning[2] > 0.6,
        "{leaning:?}"
    );
    let len = leaning.iter().map(|v| v * v).sum::<f32>().sqrt();
    assert!((len - 1.0).abs() < 1e-4, "{len}");
}

#[test]
fn a_sunlit_surface_carries_the_materials_roughness_and_every_other_keeps_its_class() {
    let s = shader();
    let class = |c: f32, rough: f32| {
        s.call("hifi_mat_class", &[Value::F32(c), Value::F32(rough)])
            .f32()
    };
    let lit = MaterialClass::Lit.encoded();
    assert!((class(lit, 0.0) - ROUGHNESS_CODES.0).abs() < 1e-5);
    assert!((class(lit, 1.0) - ROUGHNESS_CODES.1).abs() < 1e-5);
    assert!((class(lit, 0.5) - (ROUGHNESS_CODES.0 + ROUGHNESS_CODES.1) * 0.5).abs() < 1e-5);
    // Lit without the sun (indoors), a room's burned light, cut-outs and unlit surfaces keep
    // theirs: the light per pixel reads them as it reads the retail surface.
    for other in [
        lit + 0.25,
        1.7,
        MaterialClass::Baked.encoded(),
        MaterialClass::Foliage.encoded(),
        MaterialClass::Unlit.encoded(),
    ] {
        assert!((class(other, 0.7) - other).abs() < 1e-6, "{other}");
    }
}

#[test]
fn the_material_variant_is_derived_for_a_draw_of_every_vertex_format_but_the_landscapes() {
    for m in material_modules() {
        let key = DerivedKey {
            key: PipelineKey {
                vertex_format: m.format,
                ..catalogue_keys()[0]
            },
            splat: false,
            variant: Variant::Material,
        };
        assert_eq!(key.module(), m);
        assert_eq!(
            key.fragment_entry(),
            key.key.stage_ops.pixel_shader().entry_point()
        );
    }
}
