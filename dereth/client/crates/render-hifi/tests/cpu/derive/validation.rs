//! Every derived shader is valid WGSL, with the entry points its pipelines name and the outputs
//! its targets expect, checked without a device.

use dereth_render_hifi::derive::reshade::{MaterialClass, PIXEL_STAGES, SPLAT_STAGE};
use dereth_render_hifi::derive::{self, legacy_source, ModuleKey, Variant};

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

/// The locations a fragment entry point writes.
fn output_locations(module: &naga::Module, entry: &naga::EntryPoint) -> Vec<u32> {
    let result = entry
        .function
        .result
        .as_ref()
        .expect("a pixel stage returns");
    let naga::TypeInner::Struct { members, .. } = &module.types[result.ty].inner else {
        panic!("{}: the output is not a struct", entry.name);
    };
    members
        .iter()
        .map(|m| match m.binding {
            Some(naga::Binding::Location { location, .. }) => location,
            ref other => panic!("{}: output binding {other:?}", entry.name),
        })
        .collect()
}

/// Behaviour: hifi.derive.every-derived-shader-validates
#[test]
fn every_derived_permutation_parses_and_validates_with_two_outputs_per_pixel_stage() {
    for m in ModuleKey::all(Variant::Reshade) {
        let name = format!("{m:?}");
        // The ordinary source validates under the same rules, so a failure below is the
        // derivation's.
        validate(
            &format!("{name} (ordinary)"),
            &legacy_source(m.format, m.splat),
        );
        let module = validate(&name, &derive::derive(m).expect("derives"));
        let mut wanted: Vec<&str> = PIXEL_STAGES.to_vec();
        if m.splat {
            wanted.push(SPLAT_STAGE);
        }
        for stage in &wanted {
            let entry = module
                .entry_points
                .iter()
                .find(|e| e.name == *stage)
                .unwrap_or_else(|| panic!("{name}: no {stage}"));
            assert_eq!(entry.stage, naga::ShaderStage::Fragment);
            assert_eq!(output_locations(&module, entry), [0, 1], "{name}: {stage}");
        }
        assert!(
            module
                .entry_points
                .iter()
                .any(|e| e.name == derive::pipeline::VERTEX_ENTRY
                    && e.stage == naga::ShaderStage::Vertex),
            "{name}: no vertex stage"
        );
    }
}

/// Behaviour: hifi.derive.every-derived-shader-validates
#[test]
fn each_material_class_the_shader_writes_is_the_value_the_presentation_reads_back() {
    let m = ModuleKey::all(Variant::Reshade).next().expect("one module");
    let module = validate("classes", &derive::derive(m).expect("derives"));
    for c in MaterialClass::ALL {
        let (_, constant) = module
            .constants
            .iter()
            .find(|(_, k)| k.name.as_deref() == Some(c.wgsl_name()))
            .unwrap_or_else(|| panic!("no {}", c.wgsl_name()));
        let naga::Expression::Literal(naga::Literal::F32(w)) =
            module.global_expressions[constant.init]
        else {
            panic!("{} is not a float", c.wgsl_name());
        };
        assert_eq!(w, c.encoded());
        // A small whole number, so the half-float normal target holds it exactly.
        assert!(w.fract() == 0.0 && (0.0..2048.0).contains(&w));
        assert_eq!(MaterialClass::from_encoded(w), Some(c));
    }
    assert_eq!(MaterialClass::from_encoded(0.5), None);
}
