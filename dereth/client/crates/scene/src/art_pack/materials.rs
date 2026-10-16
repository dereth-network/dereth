//! A pack's materials for retail pictures, their pictures packed for drawing: the base color in
//! BC7, its normal in BC5 the same size, and its roughness and occlusion in BC5 at half that.
//!
//! The base color is held as the retail picture is held: in the display's encoding and read as
//! stored, so the stages a retail surface is drawn with compute with it exactly as they compute
//! with the retail picture. Its smaller levels are still averaged in linear light, so it does not
//! darken with distance.

use dereth_assets::art_pack::{Credit, ManifestMaterial};
use dereth_primitives::DataId;

use super::images::{self, BlockFormat, Image};

/// One material drawn in place of retail pictures, its pictures packed for drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureMaterial {
    /// The retail pictures (texture records) it is drawn in place of.
    pub textures: Vec<DataId>,
    /// The base color with the retail picture's alpha, read as stored (see the module).
    pub albedo: Image,
    /// The normal's parts along the picture's width and up it (-1..1 as 0..1), in red and green,
    /// bent by the material's normal scale; the base color's size.
    pub normal: Image,
    /// The roughness and the ambient occlusion, in red and green, half the base color's size.
    pub surface: Image,
    /// Who made it, with what, and under which license.
    pub credit: Credit,
}

/// What a material takes where it names no picture: a flat normal, a rough surface and no
/// occlusion.
const DEFAULTS: [u8; 4] = [128, 128, 230, 255];

/// The files of one material, as read.
pub(super) struct MaterialFiles {
    base: Vec<u8>,
    normal: Option<Vec<u8>>,
    roughness: Option<Vec<u8>>,
    occlusion: Option<Vec<u8>>,
}

impl MaterialFiles {
    /// The files `entry` names, read through `read`.
    pub(super) fn read(
        entry: &ManifestMaterial,
        read: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
    ) -> Result<Self, String> {
        let base = read(&entry.base_color)?;
        let mut channel = |path: &Option<String>| path.as_deref().map(&mut *read).transpose();
        Ok(Self {
            base,
            normal: channel(&entry.normal)?,
            roughness: channel(&entry.roughness)?,
            occlusion: channel(&entry.occlusion)?,
        })
    }
}

/// The part a normal picture's byte `v` holds (0..255 for -1..1), times `scale`.
fn bent(v: u8, scale: f32) -> f32 {
    (f32::from(v) / 255.0 * 2.0 - 1.0) * scale
}

/// The material `entry` from its `files`: every picture brought to the base color's size, made
/// whole blocks of four, with its levels, and compressed.
pub(super) fn material(
    entry: &ManifestMaterial,
    files: &MaterialFiles,
) -> Result<TextureMaterial, String> {
    let picture = |bytes: &[u8], path: &str| -> Result<(u32, u32, Vec<u8>), String> {
        images::decode_png(bytes).map_err(|e| format!("{path}: {e}"))
    };
    let (w, h, base) = picture(&files.base, &entry.base_color)?;
    if w == 0 || h == 0 {
        return Err(format!("{}: an empty picture", entry.base_color));
    }
    let (w, h, base) = images::to_whole_blocks(w, h, base);
    let channel =
        |bytes: &Option<Vec<u8>>, path: &Option<String>| -> Result<Option<Vec<u8>>, String> {
            let (Some(bytes), Some(path)) = (bytes, path) else {
                return Ok(None);
            };
            let (pw, ph, rgba) = picture(bytes, path)?;
            Ok(Some(images::stretched(pw, ph, &rgba, w, h)))
        };
    let normal = channel(&files.normal, &entry.normal)?;
    let roughness = channel(&files.roughness, &entry.roughness)?;
    let occlusion = channel(&files.occlusion, &entry.occlusion)?;
    let texels = (w as usize) * (h as usize);
    let at = |p: &Option<Vec<u8>>, i: usize, c: usize, default: u8| {
        p.as_ref().map_or(default, |p| p[i * 4 + c])
    };
    let scale = entry.normal_scale;
    let mut bends = Vec::with_capacity(texels * 4);
    let mut surface = Vec::with_capacity(texels * 4);
    for i in 0..texels {
        let x = bent(at(&normal, i, 0, DEFAULTS[0]), scale);
        let y = bent(at(&normal, i, 1, DEFAULTS[1]), scale);
        // A bend past the picture's plane is held just short of it.
        let k = 0.98 / dereth_primitives::num::math::hypotf(x, y).max(0.98);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: -1..1 brought to 0..255 and rounded.
        let byte = |v: f32| ((v * k * 0.5 + 0.5) * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
        bends.extend([byte(x), byte(y), 0, 255]);
        surface.extend([
            at(&roughness, i, 0, DEFAULTS[2]),
            at(&occlusion, i, 0, DEFAULTS[3]),
            0,
            255,
        ]);
    }
    let (hw, hh) = ((w / 2).max(1), (h / 2).max(1));
    let surface = images::stretched(w, h, &surface, hw, hh);
    let albedo = Image {
        srgb: false,
        ..images::compressed(
            images::with_levels(w, h, base, true, None),
            BlockFormat::Bc7,
        )
    };
    Ok(TextureMaterial {
        textures: entry.textures.clone(),
        albedo,
        normal: images::compressed(
            images::with_levels(w, h, bends, false, None),
            BlockFormat::Bc5,
        ),
        surface: images::compressed(
            images::with_levels(hw, hh, surface, false, None),
            BlockFormat::Bc5,
        ),
        credit: entry.credit.clone(),
    })
}
