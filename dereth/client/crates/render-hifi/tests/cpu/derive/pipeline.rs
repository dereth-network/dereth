//! Behaviour: none (the device-free description of a derived pipeline: its entry points and its targets).

use dereth_render::{PipelineKey, VertexFormat};
use dereth_render_hifi::derive::pipeline::{AddedShare, HDR_FORMAT, NORMAL_FORMAT};
use dereth_render_hifi::derive::{derive, DerivedKey, ModuleKey, Variant};

use crate::common::wgsl_eval::{Evaluator, Value};

fn key() -> PipelineKey {
    PipelineKey {
        vertex_format: VertexFormat::XyzNormalDiffuseTex1,
        ..PipelineKey::portal_stamp(0)
    }
}

#[test]
fn a_derived_pipeline_keeps_the_ordinary_entry_point_and_shares_its_formats_module() {
    let plain = DerivedKey {
        key: key(),
        splat: false,
        variant: Variant::Reshade,
    };
    assert_eq!(
        plain.fragment_entry(),
        key().stage_ops.pixel_shader().entry_point()
    );
    let splat = DerivedKey {
        splat: true,
        ..plain
    };
    assert_eq!(splat.fragment_entry(), "ps_splat");
    assert_eq!(
        plain.module(),
        ModuleKey {
            format: VertexFormat::XyzNormalDiffuseTex1,
            splat: false,
            variant: Variant::Reshade,
        }
    );
    // One module per format, with and without the splat.
    assert_eq!(ModuleKey::all(Variant::Reshade).count(), 10);
}

/// Behaviour: hifi.lighting.every-additive-blend-adds-what-the-ordinary-frame-adds
#[test]
fn the_three_additive_blends_of_the_catalogue_count_as_additive_and_no_other_does() {
    use dereth_render_cpu::pso::CATALOGUE;
    let derived = |row: &dereth_render_cpu::pso::CatalogueRow, variant| DerivedKey {
        key: PipelineKey {
            src_blend: row.src,
            dst_blend: row.dst,
            alpha_blend: row.alpha_blend,
            ..key()
        },
        splat: false,
        variant,
    };
    let added: Vec<(u32, AddedShare)> = CATALOGUE
        .iter()
        .filter_map(|row| {
            derived(row, Variant::Reshade)
                .adds_in_display_light()
                .map(|share| (u32::from(row.row), share))
        })
        .collect();
    assert_eq!(
        added,
        [
            (2, AddedShare::Whole),
            (4, AddedShare::Alpha),
            (6, AddedShare::InverseAlpha),
        ]
    );
    for row in &CATALOGUE {
        assert_eq!(
            derived(row, Variant::Material).adds_in_display_light(),
            derived(row, Variant::Reshade).adds_in_display_light(),
            "a material draws as the re-shade does: row {}",
            row.row
        );
        for variant in [Variant::ShadowDepth, Variant::Surface, Variant::Motion] {
            assert_eq!(
                derived(row, variant).adds_in_display_light(),
                None,
                "{variant:?} writes no color: row {}",
                row.row
            );
        }
    }

    // The same light through each blend adds the same, and what it writes is taken whole.
    let shader = Evaluator::new(
        &derive(ModuleKey {
            format: VertexFormat::XyzNormalDiffuseTex1,
            splat: false,
            variant: Variant::Reshade,
        })
        .expect("the re-shaded module"),
    );
    let added = |shown: f32, alpha: f32, indoors: f32, share: AddedShare| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: the shader's constant is a small whole number
        let share = share.constant() as u32;
        shader
            .call(
                "hifi_added",
                &[
                    Value::vector(&[shown; 3]),
                    Value::F32(alpha),
                    Value::F32(indoors),
                    Value::U32(share),
                ],
            )
            .floats()
    };
    let linear = |c: f32| {
        shader
            .call("hifi_linear_rgb", &[Value::vector(&[c; 3])])
            .floats()[0]
    };
    let wanted = linear(0.3 + 0.2) - linear(0.3);
    let writes = [
        (added(0.4, 0.5, 0.0, AddedShare::Alpha), 1.0),
        (added(0.2, 0.9, 0.0, AddedShare::Whole), 1.0),
        (added(0.4, 0.5, 0.0, AddedShare::InverseAlpha), 0.0),
    ];
    for (write, alpha) in writes {
        assert!(
            (write[0] - wanted).abs() < 1e-5,
            "{write:?} against {wanted}"
        );
        assert!((write[3] - alpha).abs() < 1e-6, "{write:?}");
    }
    // Over the darker surface it is matched over indoors, the same light adds less.
    assert!(added(0.2, 0.0, 1.0, AddedShare::Whole)[0] < wanted);
    // The very faintest part is eased away, so a stack of quads shows no edges.
    assert!(added(0.01, 0.0, 0.0, AddedShare::Whole)[0] < linear(0.31) - linear(0.3));
    assert!(added(0.0, 0.0, 0.0, AddedShare::Whole)[0].abs() < 1e-6);
}

#[test]
fn a_blended_draw_blends_its_colour_and_leaves_the_normal_beneath_it() {
    let reshade = DerivedKey {
        key: key(),
        splat: false,
        variant: Variant::Reshade,
    };
    let blend = Some(wgpu::BlendState::ALPHA_BLENDING);
    let [colour, normal] = reshade.targets(blend).expect("re-shade has targets");
    assert_eq!(colour.format, HDR_FORMAT);
    assert_eq!(colour.blend, blend);
    assert_eq!(colour.write_mask, wgpu::ColorWrites::COLOR);
    assert_eq!(normal.format, NORMAL_FORMAT);
    assert_eq!(normal.blend, None);
    assert_eq!(normal.write_mask, wgpu::ColorWrites::empty());

    let [colour, normal] = reshade.targets(None).expect("re-shade has targets");
    assert_eq!(colour.blend, None);
    assert_eq!(normal.write_mask, wgpu::ColorWrites::ALL);

    let shadow = DerivedKey {
        variant: Variant::ShadowDepth,
        ..reshade
    };
    assert!(shadow.targets(None).is_none());
}
