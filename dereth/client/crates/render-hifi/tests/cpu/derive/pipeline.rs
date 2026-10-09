//! Behaviour: none (the device-free description of a derived pipeline: its entry points and its targets).

use dereth_render::{PipelineKey, VertexFormat};
use dereth_render_hifi::derive::pipeline::{HDR_FORMAT, NORMAL_FORMAT};
use dereth_render_hifi::derive::{DerivedKey, ModuleKey, Variant};

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
