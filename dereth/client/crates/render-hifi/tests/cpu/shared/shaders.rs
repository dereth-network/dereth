//! The shader code the passes share is valid on its own.

use dereth_render_hifi::shared::fullscreen::{COLOUR_WGSL, DEPTH_WGSL, FULLSCREEN_WGSL};

/// Behaviour: hifi.shaders.the-shared-shader-code-validates
#[test]
fn every_shared_shader_parses_and_validates() {
    for (name, source) in [
        ("fullscreen", FULLSCREEN_WGSL),
        ("colour", COLOUR_WGSL),
        ("depth", DEPTH_WGSL),
    ] {
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    }
}

/// Behaviour: hifi.shaders.the-composite-shader-validates
#[test]
fn the_composite_shader_parses_and_validates() {
    let source = dereth_render_hifi::composite::composite_shader();
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|e| panic!("composite: {}", e.emit_to_string(source)));
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("composite: {e:?}"));
    let entries: Vec<&str> = module
        .entry_points
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(entries, ["vs", "fs_copy", "fs_depth"]);
    drop(info);
}

/// Behaviour: hifi.shaders.the-re-shade-shaders-validate
/// The shaders of the neutral resolve, the normal and census views and the landscape-normal pass
/// parse and validate without a device, with the entry points their pipelines name.
#[test]
fn the_re_shade_shaders_parse_and_validate() {
    let wanted: [&[&str]; 2] = [
        &["vs", "fs_lift", "fs_resolve", "fs_normals", "fs_census"],
        &["vs", "fs_ground"],
    ];
    for (source, want) in dereth_render_hifi::reshade::shaders().iter().zip(wanted) {
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{e:?}"));
        let entries: Vec<&str> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(entries, want);
    }
}

/// Behaviour: hifi.shaders.the-shared-shader-code-validates
#[test]
fn the_lighting_shaders_parse_and_validate() {
    for (name, source) in dereth_render_hifi::passes::lighting::shaders() {
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    }
}

/// Behaviour: none (a shader of an experimental pass is checked as it is written).
/// The traced scene's alpha atlas shader parses and validates without a device.
#[test]
fn the_traced_scene_shaders_parse_and_validate() {
    for (name, source) in dereth_render_hifi::passes::rt::shaders() {
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    }
}

/// Behaviour: none (a shader of an experimental pass is checked as it is written).
/// The lamp shaders parse and validate without a device.
#[test]
fn the_lamp_shaders_parse_and_validate() {
    for (name, source) in dereth_render_hifi::passes::lamps::shaders() {
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    }
}
