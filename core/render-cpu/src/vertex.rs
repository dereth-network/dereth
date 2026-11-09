//! Vertex formats: the client's `VertexLayoutInfo` word and the five FVF codes actually used.
//!
//! The vertex descriptions preserve the fixed-function format and stride mapping.
//!
//! The client's vertex format is a *superset* of the D3D9 FVF word; its offset generator decodes
//! the format into byte offsets. The renderer only ever hands D3D
//! the FVF word `format & 0xCE00FFFF`, i.e. the bone-count byte and the two tangent bits masked out.

/// The five FVF families the client actually draws with.
///
/// Five fixed vertex formats are used; other meshes supply their own `VertexLayoutInfo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VertexFormat {
    /// `0x142` = `XYZ | DIFFUSE | TEX1`, stride 24. Terrain triangles, and the font batch: the
    /// landscape polygon draw and the end of a text batch both use it.
    XyzDiffuseTex1,
    /// `0x242` = `XYZ | DIFFUSE | TEX2`, stride 32. Terrain with the landscape detail texture.
    XyzDiffuseTex2,
    /// `0x152` = `XYZ | NORMAL | DIFFUSE | TEX1`, stride 36 (the client's standard mesh vertex).
    /// `0x152` = `XYZ | NORMAL | DIFFUSE | TEX1`, stride 36 -- the ordinary mesh polygon draw.
    XyzNormalDiffuseTex1,
    /// `0x252` = `XYZ | NORMAL | DIFFUSE | TEX2`, stride 44 (the client's two-texture mesh vertex).
    XyzNormalDiffuseTex2,
    /// `0x144` = `XYZRHW | DIFFUSE | TEX1`, stride 28. The pre-transformed portal depth stamp.
    /// `0x144` = `XYZRHW | DIFFUSE | TEX1`, stride 28. The pre-transformed portal depth stamp.
    XyzRhwDiffuseTex1,
}

/// One field of a vertex, in the D3D9 declaration order the client uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertexElement {
    /// `float3` position.
    Position,
    /// `float4` pre-transformed position (x, y, z, rhw).
    PositionTransformed,
    /// `float3` normal.
    Normal,
    /// `D3DCOLOR` diffuse. **BGRA in memory**: little-endian `0xAARRGGBB` is B, G, R, A bytes.
    Diffuse,
    /// `float2` texture coordinate set `n`.
    TexCoord(u32),
}

/// The D3D9 FVF bits this crate cares about.
pub mod fvf {
    /// bits 1-3 == 2: `float3` position.
    pub const XYZ: u32 = 0x002;
    /// bits 1-3 == 4: `float4` pre-transformed position.
    pub const XYZRHW: u32 = 0x004;
    pub const NORMAL: u32 = 0x010;
    pub const PSIZE: u32 = 0x020;
    pub const DIFFUSE: u32 = 0x040;
    pub const SPECULAR: u32 = 0x080;
    /// Texture-coordinate-set count lives in bits 8-11.
    pub const TEX_COUNT_SHIFT: u32 = 8;
    pub const TEX_COUNT_MASK: u32 = 0xF00;
    /// The FVF word is `format & 0xCE00FFFF` — the mask applied before `SetFVF`.
    pub const FVF_MASK: u32 = 0xCE00_FFFF;
    /// Turbine extension: per-vertex bone indices + weights live in bits 16-23.
    pub const NUM_MATRICES_SHIFT: u32 = 16;
    pub const NUM_MATRICES_MASK: u32 = 0x00FF_0000;
    /// Round the stride up to the next multiple of 32.
    pub const PAD_TO_32: u32 = 0x0100_0000;
    /// Tangent vector S (`float3`). Non-FVF.
    pub const TANGENT_S: u32 = 0x1000_0000;
    /// Tangent vector T (`float3`). Non-FVF.
    pub const TANGENT_T: u32 = 0x2000_0000;
}

impl VertexFormat {
    /// The raw FVF code the client passes to `IDirect3DDevice9::SetFVF`.
    #[must_use]
    pub const fn fvf(self) -> u32 {
        match self {
            Self::XyzDiffuseTex1 => 0x142,
            Self::XyzDiffuseTex2 => 0x242,
            Self::XyzNormalDiffuseTex1 => 0x152,
            Self::XyzNormalDiffuseTex2 => 0x252,
            Self::XyzRhwDiffuseTex1 => 0x144,
        }
    }

    /// The byte stride, as computes it.
    #[must_use]
    pub const fn stride(self) -> u32 {
        match self {
            Self::XyzDiffuseTex1 => 24,
            Self::XyzDiffuseTex2 => 32,
            Self::XyzNormalDiffuseTex1 => 36,
            Self::XyzNormalDiffuseTex2 => 44,
            Self::XyzRhwDiffuseTex1 => 28,
        }
    }

    /// Every format whose FVF the client is known to issue, in ascending FVF order.
    #[must_use]
    pub const fn all() -> [Self; 5] {
        [
            Self::XyzDiffuseTex1,
            Self::XyzRhwDiffuseTex1,
            Self::XyzNormalDiffuseTex1,
            Self::XyzDiffuseTex2,
            Self::XyzNormalDiffuseTex2,
        ]
    }

    /// Recover the format from an FVF code, or `None` if the client never issues that code.
    #[must_use]
    pub const fn from_fvf(code: u32) -> Option<Self> {
        match code {
            0x142 => Some(Self::XyzDiffuseTex1),
            0x242 => Some(Self::XyzDiffuseTex2),
            0x152 => Some(Self::XyzNormalDiffuseTex1),
            0x252 => Some(Self::XyzNormalDiffuseTex2),
            0x144 => Some(Self::XyzRhwDiffuseTex1),
            _ => None,
        }
    }

    /// The elements in declaration order, each with its byte offset within the vertex.
    ///
    /// Field order is exactly D3D9's: `origin, weight0..4, normal, pointSize, diffuse, specular,
    /// tc0..tc7, vectorS, vectorT, matrices, matrixWeights`.
    #[must_use]
    pub fn elements(self) -> Vec<(VertexElement, u32)> {
        let mut out = Vec::new();
        let mut off = 0u32;
        match self {
            Self::XyzRhwDiffuseTex1 => {
                out.push((VertexElement::PositionTransformed, off));
                off += 16;
            }
            _ => {
                out.push((VertexElement::Position, off));
                off += 12;
            }
        }
        if matches!(
            self,
            Self::XyzNormalDiffuseTex1 | Self::XyzNormalDiffuseTex2
        ) {
            out.push((VertexElement::Normal, off));
            off += 12;
        }
        out.push((VertexElement::Diffuse, off));
        off += 4;
        for set in 0..self.tex_coord_sets() {
            out.push((VertexElement::TexCoord(set), off));
            off += 8;
        }
        debug_assert_eq!(off, self.stride());
        out
    }

    /// How many `float2` texture-coordinate sets this format carries.
    #[must_use]
    pub const fn tex_coord_sets(self) -> u32 {
        match self {
            Self::XyzDiffuseTex1 | Self::XyzNormalDiffuseTex1 | Self::XyzRhwDiffuseTex1 => 1,
            Self::XyzDiffuseTex2 | Self::XyzNormalDiffuseTex2 => 2,
        }
    }

    /// True when the position is already in clip space and must bypass the world/view/projection
    /// transforms (`D3DFVF_XYZRHW`).
    #[must_use]
    pub const fn is_pre_transformed(self) -> bool {
        matches!(self, Self::XyzRhwDiffuseTex1)
    }
}

/// The decoded shape of an arbitrary vertex-format word.
///
/// The mesh path (mesh rendering through a vertex buffer) carries its own format word rather than
/// one of the five fixed codes, so the decoder has to exist even though the *catalogue* is five
/// entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VertexLayoutInfo {
    pub format: u32,
    pub size: u32,
    /// `false` when the Turbine extensions (bone matrices, tangents) are present, in which case the
    /// client passes FVF 0 and describes the layout by hand.
    pub fvf_compatible: bool,
    pub num_matrices: u32,
}

impl VertexLayoutInfo {
    /// Decode a format word into a stride and the FVF-compatibility flag.
    ///
    /// The observed vertex-layout size contributions are:
    /// position 12 bytes plus 4 per blend weight, normal 12, point size 4, diffuse 4, specular 4,
    /// 8 per texture-coordinate pair, one byte of index per blend matrix plus 4 per matrix of
    /// weights, 12 per tangent vector, and finally the optional round-up to a multiple of 32.
    #[must_use]
    pub fn generate_offsets(format: u32) -> Self {
        let mut size = 0u32;
        let pos = format & 0x0E;
        if pos != 0 {
            size += 12;
            // 0x2 = XYZ (0 weights), 0x4 = XYZRHW (one extra float), 0x6..=0xE = XYZB1..XYZB5.
            let num_weights = match pos {
                0x2 => 0,
                0x4 => 1,
                other => (other - 4) / 2,
            };
            size += num_weights * 4;
        }
        if format & fvf::NORMAL != 0 {
            size += 12;
        }
        if format & fvf::PSIZE != 0 {
            size += 4;
        }
        if format & fvf::DIFFUSE != 0 {
            size += 4;
        }
        if format & fvf::SPECULAR != 0 {
            size += 4;
        }
        let tex_sets = (format & fvf::TEX_COUNT_MASK) >> fvf::TEX_COUNT_SHIFT;
        size += tex_sets * 8;
        if format & fvf::TANGENT_S != 0 {
            size += 12;
        }
        if format & fvf::TANGENT_T != 0 {
            size += 12;
        }
        let num_matrices = (format & fvf::NUM_MATRICES_MASK) >> fvf::NUM_MATRICES_SHIFT;
        size += num_matrices + num_matrices * 4;
        if format & fvf::PAD_TO_32 != 0 {
            size = size.div_ceil(32) * 32;
        }
        let fvf_compatible = num_matrices == 0 && format & (fvf::TANGENT_S | fvf::TANGENT_T) == 0;
        Self {
            format,
            size,
            fvf_compatible,
            num_matrices,
        }
    }

    /// The word actually handed to `SetFVF`: `format & 0xCE00FFFF`, or 0 when not FVF-compatible.
    #[must_use]
    pub const fn fvf_word(&self) -> u32 {
        if self.fvf_compatible {
            self.format & fvf::FVF_MASK
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Check the observed stride for each fixed vertex format against its input-element offsets.
    #[test]
    fn the_five_fvf_codes_have_the_documented_strides() {
        assert_eq!(VertexFormat::XyzDiffuseTex1.stride(), 24);
        assert_eq!(VertexFormat::XyzDiffuseTex2.stride(), 32);
        assert_eq!(VertexFormat::XyzNormalDiffuseTex1.stride(), 36);
        assert_eq!(VertexFormat::XyzNormalDiffuseTex2.stride(), 44);
        assert_eq!(VertexFormat::XyzRhwDiffuseTex1.stride(), 28);
    }

    // The observed input-element offsets for 0x152 are:
    // POSITION@0, NORMAL@12, COLOR@24, TEXCOORD0@28.
    #[test]
    fn element_offsets_match_the_d3d12_input_layout_table() {
        let e = VertexFormat::XyzNormalDiffuseTex1.elements();
        assert_eq!(
            e,
            vec![
                (VertexElement::Position, 0),
                (VertexElement::Normal, 12),
                (VertexElement::Diffuse, 24),
                (VertexElement::TexCoord(0), 28),
            ]
        );
        let e = VertexFormat::XyzNormalDiffuseTex2.elements();
        assert_eq!(e.last(), Some(&(VertexElement::TexCoord(1), 36)));
        let e = VertexFormat::XyzDiffuseTex1.elements();
        assert_eq!(
            e,
            vec![
                (VertexElement::Position, 0),
                (VertexElement::Diffuse, 12),
                (VertexElement::TexCoord(0), 16),
            ]
        );
        let e = VertexFormat::XyzDiffuseTex2.elements();
        assert_eq!(e.last(), Some(&(VertexElement::TexCoord(1), 24)));
        // Pre-transformed: POSITION is float4, so COLOR lands at 16 and TEXCOORD0 at 20.
        let e = VertexFormat::XyzRhwDiffuseTex1.elements();
        assert_eq!(
            e,
            vec![
                (VertexElement::PositionTransformed, 0),
                (VertexElement::Diffuse, 16),
                (VertexElement::TexCoord(0), 20),
            ]
        );
    }

    // Using the observed size contributions, each of the five codes must decode to the
    // same stride the fixed table gives, which is the cross-check that the general decoder is right.
    #[test]
    fn generate_offsets_reproduces_the_five_fixed_strides() {
        for f in VertexFormat::all() {
            let info = VertexLayoutInfo::generate_offsets(f.fvf());
            assert_eq!(info.size, f.stride(), "fvf {:#x}", f.fvf());
            assert!(info.fvf_compatible);
            assert_eq!(info.fvf_word(), f.fvf());
        }
    }

    // Oracle: same table. `num_matrices != 0` or a tangent bit clears `fvf_compatible`, and the
    // FVF word handed to D3D masks those bits out (`format & 0xCE00FFFF`).
    #[test]
    fn turbine_extensions_clear_fvf_compatibility() {
        // XYZ | DIFFUSE | TEX1 with two bone matrices.
        let info = VertexLayoutInfo::generate_offsets(0x142 | (2 << 16));
        assert!(!info.fvf_compatible);
        assert_eq!(info.num_matrices, 2);
        // 24 base + 2 index bytes + 2*4 weight bytes.
        assert_eq!(info.size, 24 + 2 + 8);
        assert_eq!(info.fvf_word(), 0);

        let info = VertexLayoutInfo::generate_offsets(0x142 | fvf::TANGENT_S | fvf::TANGENT_T);
        assert!(!info.fvf_compatible);
        assert_eq!(info.size, 24 + 24);
    }

    // Oracle: same table, bit 0x01000000 = "round the stride up to the next multiple of 32".
    #[test]
    fn the_pad_bit_rounds_the_stride_to_a_multiple_of_32() {
        let info = VertexLayoutInfo::generate_offsets(0x142 | fvf::PAD_TO_32);
        assert_eq!(info.size, 32);
        let info = VertexLayoutInfo::generate_offsets(0x252 | fvf::PAD_TO_32);
        assert_eq!(info.size, 64);
    }

    #[test]
    fn from_fvf_round_trips_and_rejects_unused_codes() {
        for f in VertexFormat::all() {
            assert_eq!(VertexFormat::from_fvf(f.fvf()), Some(f));
        }
        assert_eq!(VertexFormat::from_fvf(0x112), None);
    }
}

/// The device-neutral storage format of one vertex attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeFormat {
    Float2,
    Float3,
    Float4,
    Bgra8Unorm,
    Rgba8Unorm,
}

impl VertexElement {
    /// Shader input location shared by all vertex permutations.
    #[must_use]
    pub const fn location(self) -> u32 {
        match self {
            Self::Position | Self::PositionTransformed => 0,
            Self::Normal => 1,
            Self::Diffuse => 2,
            Self::TexCoord(n) => 3 + n,
        }
    }

    /// Attribute storage; hosts without BGRA vertex inputs swizzle diffuse in the shader.
    #[must_use]
    pub const fn attribute_format(self, bgra: bool) -> AttributeFormat {
        match self {
            Self::PositionTransformed => AttributeFormat::Float4,
            Self::Position | Self::Normal => AttributeFormat::Float3,
            Self::Diffuse if bgra => AttributeFormat::Bgra8Unorm,
            Self::Diffuse => AttributeFormat::Rgba8Unorm,
            Self::TexCoord(_) => AttributeFormat::Float2,
        }
    }
}

#[cfg(test)]
mod attribute_tests {
    //! Behaviour: none (vertex attributes match the shader locations and memory layout).
    use super::*;
    #[test]
    fn every_vertex_layout_has_unique_locations_and_only_diffuse_depends_on_bgra_support() {
        for format in VertexFormat::all() {
            let elements = format.elements();
            let locations: std::collections::BTreeSet<_> =
                elements.iter().map(|(e, _)| e.location()).collect();
            assert_eq!(locations.len(), elements.len());
            for (element, _) in elements {
                if element == VertexElement::Diffuse {
                    assert_eq!(element.attribute_format(true), AttributeFormat::Bgra8Unorm);
                    assert_eq!(element.attribute_format(false), AttributeFormat::Rgba8Unorm);
                } else {
                    assert_eq!(
                        element.attribute_format(true),
                        element.attribute_format(false)
                    );
                }
            }
        }
        assert_eq!(VertexElement::Position.location(), 0);
        assert_eq!(VertexElement::Normal.location(), 1);
        assert_eq!(VertexElement::Diffuse.location(), 2);
        assert_eq!(VertexElement::TexCoord(1).location(), 4);
    }
}
