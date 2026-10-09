//! The surface variant: every permutation derives from the re-shaded text with each of its
//! anchors matching once, and is valid WGSL whose pixel stages write the four surface targets.

use dereth_render_hifi::derive::reshade::{PIXEL_STAGES, SPLAT_STAGE};
use dereth_render_hifi::derive::{self, matches, ModuleKey, Variant};
use dereth_render_hifi::passes::lighting::derive::{SurfaceKind, TARGETS};

fn validate(name: &str, source: &str) -> naga::Module {
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::default(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
    module
}

/// Behaviour: hifi.lighting.every-surface-shader-derives-and-validates
#[test]
fn every_surface_permutation_derives_once_per_anchor_and_writes_four_targets() {
    assert_eq!(TARGETS.len(), 4);
    for m in ModuleKey::all(Variant::Surface) {
        let name = format!("{m:?}");
        let reshaded = derive::derive(ModuleKey {
            variant: Variant::Reshade,
            ..m
        })
        .expect("re-shade derives");
        for a in derive::anchors(m).expect("surfaces derive") {
            assert_eq!(
                matches(&reshaded, &a).expect("one scope"),
                1,
                "{name}: {} must match once in the re-shaded text",
                a.name
            );
        }
        let text = derive::derive(m).unwrap_or_else(|e| panic!("{name}: {e}"));
        let module = validate(&name, &text);
        let mut wanted: Vec<&str> = PIXEL_STAGES.to_vec();
        if m.splat {
            wanted.push(SPLAT_STAGE);
        }
        for stage in wanted {
            let entry = module
                .entry_points
                .iter()
                .find(|e| e.name == stage)
                .unwrap_or_else(|| panic!("{name}: no {stage}"));
            assert_eq!(entry.stage, naga::ShaderStage::Fragment);
            let result = entry
                .function
                .result
                .as_ref()
                .expect("a pixel stage returns");
            let naga::TypeInner::Struct { members, .. } = &module.types[result.ty].inner else {
                panic!("{name}: {stage} returns no struct");
            };
            let locations: Vec<u32> = members
                .iter()
                .filter_map(|m| match m.binding {
                    Some(naga::Binding::Location { location, .. }) => Some(location),
                    _ => None,
                })
                .collect();
            assert_eq!(locations, [0, 1, 2, 3], "{name}: {stage}");
        }
    }
}

/// Behaviour: hifi.lighting.every-surface-shader-derives-and-validates
/// Each surface kind is a whole number of eighths, so the albedo target's 8-bit alpha holds it
/// exactly, and no two kinds share a value.
#[test]
fn each_surface_kind_fits_the_albedo_alpha_exactly() {
    let mut seen = std::collections::HashSet::new();
    for k in SurfaceKind::ALL {
        let v = k as u8;
        assert!(v < 8);
        assert!(seen.insert(v));
        let stored = (f32::from(v) / 8.0 * 255.0).round() / 255.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..8
        let back = (stored * 8.0).round() as u8;
        assert_eq!(back, v);
    }
}
