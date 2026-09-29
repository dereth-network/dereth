//! Compile the shared WGSL shader permutations to SPIR-V with `naga`.
//!
//! This runs at device creation, exactly where the D3D12 build ran `D3DCompile`, so there is no
//! shader toolchain to install and a shader error is reported the same way a device error is.
//! WGSL has no preprocessor; the three permutation flags the HLSL carried as `#if`s are text
//! substitutions of the `{{...}}` markers in the source, made here.

use std::collections::HashMap;

use crate::pso::PixelShader;
use crate::vertex::VertexFormat;
use crate::wgsl::{bias_legacy_samples, bias_splat_samples};
use crate::RenderError;

use dereth_render_cpu::shader::{fragment_source, vertex_source};

/// The compiled shader set: one vertex-shader permutation per FVF code, and the five pixel
/// shaders, as SPIR-V words.
pub(super) type ShaderSet = (
    HashMap<VertexFormat, Vec<u32>>,
    HashMap<PixelShader, Vec<u32>>,
);

/// Compile the shader set.
///
/// **A deviation worth stating.** The client has two vertex shaders, one per FVF family.
/// A programmable pipeline requires every input the vertex shader declares to be present in the
/// vertex input state, so one vertex shader cannot serve both a layout with a `NORMAL` and one
/// without. There are two vertex *shaders* in the source -- the world-space one and the
/// pre-transformed one -- compiled into five permutations, one per FVF code. The pixel shaders
/// really are five.
///
/// `shader_lod_bias` makes every pixel-shader texture sample take the bound sampler's bias from
/// the frame block, for a device whose samplers cannot carry one.
///
/// # Errors
/// [`RenderError::Device`] with naga's diagnostic when the source does not parse, validate or
/// lower to SPIR-V -- the same shape as a `D3DCompile` failure.
pub(super) fn compile_shaders(
    bgra_vertex_colour: bool,
    shader_lod_bias: bool,
) -> Result<ShaderSet, RenderError> {
    let mut vs = HashMap::new();
    for f in VertexFormat::all() {
        let source = vertex_source(f, bgra_vertex_colour);
        let module = parse(&source, "vs_main")?;
        vs.insert(
            f,
            write(&module, naga::ShaderStage::Vertex, "vs_main", &source)?,
        );
    }
    let mut source = fragment_source();
    if shader_lod_bias {
        source = bias_legacy_samples(&source);
    }
    let module = parse(&source, "ps")?;
    let mut ps = HashMap::new();
    for s in PixelShader::all() {
        ps.insert(
            s,
            write(
                &module,
                naga::ShaderStage::Fragment,
                s.entry_point(),
                &source,
            )?,
        );
    }
    Ok((vs, ps))
}

/// Compile one entry point of a standalone WGSL module -- the landscape compositor and the splat
/// pixel shader, which are not permutations of the legacy set.
///
/// # Errors
/// As [`compile_shaders`].
pub(super) fn compile_entry(
    source: &str,
    stage: naga::ShaderStage,
    entry: &str,
) -> Result<Vec<u32>, RenderError> {
    let module = parse_with(source, entry, naga::valid::Capabilities::IMMEDIATES)?;
    write(&module, stage, entry, source)
}

/// The landscape splat pixel shader: `tail` appended to the legacy fragment source, so it shares
/// the legacy set's constant blocks, fog, gamma, lighting and alpha test rather than copying them.
/// `shader_lod_bias` biases its samples as [`compile_shaders`] does.
///
/// # Errors
/// As [`compile_shaders`].
pub(super) fn compile_splat_fragment(
    tail: &str,
    entry: &str,
    shader_lod_bias: bool,
) -> Result<Vec<u32>, RenderError> {
    let source = if shader_lod_bias {
        bias_legacy_samples(&fragment_source()) + &bias_splat_samples(tail)
    } else {
        fragment_source() + tail
    };
    compile_entry(&source, naga::ShaderStage::Fragment, entry)
}

/// Parse and validate one WGSL module.
fn parse(source: &str, what: &str) -> Result<(naga::Module, naga::valid::ModuleInfo), RenderError> {
    parse_with(source, what, naga::valid::Capabilities::empty())
}

fn parse_with(
    source: &str,
    what: &str,
    capabilities: naga::valid::Capabilities,
) -> Result<(naga::Module, naga::valid::ModuleInfo), RenderError> {
    let module = naga::front::wgsl::parse_str(source).map_err(|e| {
        RenderError::Device(format!("wgsl parse ({what}): {}", e.emit_to_string(source)))
    })?;
    let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), capabilities)
        .validate(&module)
        .map_err(|e| {
            RenderError::Device(format!(
                "wgsl validate ({what}): {}",
                e.emit_to_string(source)
            ))
        })?;
    Ok((module, info))
}

/// Lower one entry point to SPIR-V 1.0.
///
/// `ADJUST_COORDINATE_SPACE` is deliberately **off**: it would negate `Position.y` to turn
/// WGSL's y-up clip space into Vulkan's y-down one, and this renderer keeps Direct3D's clip
/// space instead, by installing every viewport with a negative height (see `Gpu::set_viewport`).
/// The shader arithmetic is therefore the HLSL's, unchanged.
fn write(
    module: &(naga::Module, naga::valid::ModuleInfo),
    stage: naga::ShaderStage,
    entry: &str,
    source: &str,
) -> Result<Vec<u32>, RenderError> {
    let mut options = naga::back::spv::Options {
        lang_version: (1, 0),
        ..naga::back::spv::Options::default()
    };
    options
        .flags
        .remove(naga::back::spv::WriterFlags::ADJUST_COORDINATE_SPACE);
    options.flags.remove(naga::back::spv::WriterFlags::DEBUG);
    let pipeline = naga::back::spv::PipelineOptions {
        shader_stage: stage,
        entry_point: entry.to_string(),
    };
    naga::back::spv::write_vec(&module.0, &module.1, &options, Some(&pipeline)).map_err(|e| {
        let _ = source;
        RenderError::Device(format!("spir-v write ({entry}): {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every permutation parses, validates and lowers -- on a machine with no GPU at all. This is
    /// the half of "the shaders compile" that does not need a device.
    #[test]
    fn every_permutation_compiles_to_spirv() {
        for (bgra, shader_lod_bias) in [(true, false), (false, false), (true, true), (false, true)]
        {
            let (vs, ps) = compile_shaders(bgra, shader_lod_bias).expect("the shader set compiles");
            assert_eq!(vs.len(), 5);
            assert_eq!(ps.len(), 5);
            for words in vs.values().chain(ps.values()) {
                assert_eq!(words[0], 0x0723_0203, "SPIR-V magic");
                assert!(words.len() > 16);
            }
        }
    }
}
