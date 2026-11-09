//! The PSO catalogue: the whole legacy render state machine, as data.
//!
//! The translation follows the exact surface-binding algorithm and reproduces the 15-row catalogue
//! that maps surface flags and passes to fixed-function state.
//!
//! There are no programmable shaders in the client: the single `SetVertexShader` call in the whole
//! binary passes `NULL`, and there is no
//! `CreatePixelShader` anywhere. The five pixel shaders named by [`PixelShader`] exist only because
//! D3D12 has no fixed-function pipeline; each one *is* one of the texture-stage op combinations the
//! client issues.

use crate::surface::{surface_type as st, Surface};
use crate::vertex::VertexFormat;

/// `D3DBLEND`. Values use the client's D3D9 encoding:
/// `5 = SRCALPHA`, `6 = INVSRCALPHA`, `9 = DESTCOLOR`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Blend {
    Zero = 1,
    One = 2,
    SrcColor = 3,
    InvSrcColor = 4,
    SrcAlpha = 5,
    InvSrcAlpha = 6,
    DestAlpha = 7,
    InvDestAlpha = 8,
    DestColor = 9,
    InvDestColor = 10,
}

impl Blend {
    /// A colour blend factor's alpha-channel analogue.
    #[must_use]
    pub const fn alpha_factor(self) -> Self {
        match self {
            Self::SrcColor => Self::SrcAlpha,
            Self::InvSrcColor => Self::InvSrcAlpha,
            Self::DestColor => Self::DestAlpha,
            Self::InvDestColor => Self::InvDestAlpha,
            other => other,
        }
    }

    /// The destination-alpha rewrite:
    /// `DSTALPHA → SRCALPHA` and `INVDSTALPHA → INVSRCALPHA` whenever the back buffer has no alpha
    /// channel — which is the normal case, because the back buffer is `X8R8G8B8`.
    ///
    /// Dropping this changes blending on every path that asks for destination alpha.
    #[must_use]
    pub const fn rewrite_for_no_destination_alpha(self) -> Self {
        match self {
            Self::DestAlpha => Self::SrcAlpha,
            Self::InvDestAlpha => Self::InvSrcAlpha,
            other => other,
        }
    }
}

/// `DepthTestType`, whose values mirror `D3DCMP` (`DEPTHTEST_LESS` is the constant 2;
/// `DEPTHTEST_ALWAYS (8)` is named in the UI material table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZFunc {
    Never = 1,
    Less = 2,
    Equal = 3,
    LessEqual = 4,
    Greater = 5,
    NotEqual = 6,
    GreaterEqual = 7,
    Always = 8,
}

/// The cull mode, values mirroring `D3DCULL` (`CULLMODE_NONE (1)` is
/// named in the UI material table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cull {
    None = 1,
    Cw = 2,
    Ccw = 3,
}

/// `D3DTEXTUREOP`, restricted to the operations the client issues.
///
/// `TEXOP_DISABLE (1)`, `SELECTARG1 (2)`, `SELECTARG2 (3)`, `MODULATE (4)`,
/// `BLENDCURRENTALPHA (16)`, `PREMODULATE (17)` — the texture operations used by these states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TexOp {
    Disable = 1,
    SelectArg1 = 2,
    SelectArg2 = 3,
    Modulate = 4,
    BlendCurrentAlpha = 16,
    PreModulate = 17,
}

/// `D3DTA_*`: `DIFFUSE (0)`, `CURRENT (1)`, `TEXTURE (2)`, `TFACTOR (3)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TexArg {
    Diffuse = 0,
    Current = 1,
    Texture = 2,
    TFactor = 3,
}

/// One texture stage's colour and alpha op with their two arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StageOp {
    pub color_op: TexOp,
    pub color_arg1: TexArg,
    pub color_arg2: TexArg,
    pub alpha_op: TexOp,
    pub alpha_arg1: TexArg,
    pub alpha_arg2: TexArg,
}

impl StageOp {
    /// The terminator the client writes on the first unused stage:
    /// colour op `DISABLE` with args `(TEXTURE, CURRENT)` on stage n, and the alpha equivalent.
    pub const DISABLED: Self = Self {
        color_op: TexOp::Disable,
        color_arg1: TexArg::Texture,
        color_arg2: TexArg::Current,
        alpha_op: TexOp::Disable,
        alpha_arg1: TexArg::Texture,
        alpha_arg2: TexArg::Current,
    };
}

/// The texture-stage op chain for a draw. Two stages is the most the legacy path ever uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StageOps {
    pub stages: [StageOp; 2],
}

impl StageOps {
    /// Base pass, no detail texture — the client's surface setup, step 1, without single-pass detail:
    /// stage 0 COLOROP `MODULATE(TEXTURE, DIFFUSE)`, ALPHAOP `MODULATE(TEXTURE, DIFFUSE)`;
    /// stage 1 disabled.
    pub const BASE: Self = Self {
        stages: [
            StageOp {
                color_op: TexOp::Modulate,
                color_arg1: TexArg::Texture,
                color_arg2: TexArg::Diffuse,
                alpha_op: TexOp::Modulate,
                alpha_arg1: TexArg::Texture,
                alpha_arg2: TexArg::Diffuse,
            },
            StageOp::DISABLED,
        ],
    };

    /// Base pass with the detail texture in stage 1 — surface setup step 1 with
    /// single-pass detail: stage 0 COLOROP `MODULATE(TEXTURE, DIFFUSE)`, ALPHAOP
    /// `PREMODULATE(DIFFUSE, DIFFUSE)`; stage 1 COLOROP `BLENDCURRENTALPHA(TEXTURE, CURRENT)`,
    /// ALPHAOP `MODULATE(TEXTURE, CURRENT)`.
    pub const SINGLE_PASS_DETAIL: Self = Self {
        stages: [
            StageOp {
                color_op: TexOp::Modulate,
                color_arg1: TexArg::Texture,
                color_arg2: TexArg::Diffuse,
                alpha_op: TexOp::PreModulate,
                alpha_arg1: TexArg::Diffuse,
                alpha_arg2: TexArg::Diffuse,
            },
            StageOp {
                color_op: TexOp::BlendCurrentAlpha,
                color_arg1: TexArg::Texture,
                color_arg2: TexArg::Current,
                alpha_op: TexOp::Modulate,
                alpha_arg1: TexArg::Texture,
                alpha_arg2: TexArg::Current,
            },
        ],
    };

    /// The UI quad, opaque source:
    /// COLOROP `MODULATE(TEXTURE, TFACTOR)`, ALPHAOP `SELECTARG2(TEXTURE, TFACTOR)`.
    /// Colour modulation *and* opacity both come from `D3DRS_TEXTUREFACTOR`.
    pub const UI_OPAQUE: Self = Self {
        stages: [
            StageOp {
                color_op: TexOp::Modulate,
                color_arg1: TexArg::Texture,
                color_arg2: TexArg::TFactor,
                alpha_op: TexOp::SelectArg2,
                alpha_arg1: TexArg::Texture,
                alpha_arg2: TexArg::TFactor,
            },
            StageOp::DISABLED,
        ],
    };

    /// The UI quad when its surface has alpha: the alpha op becomes `MODULATE`.
    pub const UI_ALPHA: Self = Self {
        stages: [
            StageOp {
                color_op: TexOp::Modulate,
                color_arg1: TexArg::Texture,
                color_arg2: TexArg::TFactor,
                alpha_op: TexOp::Modulate,
                alpha_arg1: TexArg::Texture,
                alpha_arg2: TexArg::TFactor,
            },
            StageOp::DISABLED,
        ],
    };

    /// The texture-based font: COLOROP `SELECTARG2(TEXTURE, DIFFUSE)` — the
    /// colour comes entirely from the vertex — and ALPHAOP `MODULATE(TEXTURE, DIFFUSE)`, i.e. alpha
    /// is glyph coverage times vertex alpha.
    pub const TEXT: Self = Self {
        stages: [
            StageOp {
                color_op: TexOp::SelectArg2,
                color_arg1: TexArg::Texture,
                color_arg2: TexArg::Diffuse,
                alpha_op: TexOp::Modulate,
                alpha_arg1: TexArg::Texture,
                alpha_arg2: TexArg::Diffuse,
            },
            StageOp::DISABLED,
        ],
    };

    /// Which of the five pixel shaders covers this op chain.
    #[must_use]
    pub const fn pixel_shader(&self) -> PixelShader {
        // Stage 1 decides first: only the single-pass detail chain has a live second stage.
        match self.stages[1].color_op {
            TexOp::BlendCurrentAlpha => PixelShader::BlendCurrentAlpha,
            _ => match (self.stages[0].color_op, self.stages[0].alpha_op) {
                (_, TexOp::PreModulate) => PixelShader::PreModulate,
                (TexOp::SelectArg1, _) => PixelShader::SelectArg1,
                (TexOp::SelectArg2, _) => PixelShader::SelectArg2,
                _ => PixelShader::Modulate,
            },
        }
    }
}

/// The five pixel shaders that stand in for the fixed-function texture-stage chain.
///
/// Two vertex shaders (one per FVF family) and five pixel shaders: MODULATE,
/// SELECTARG1, SELECTARG2, PREMODULATE, BLENDCURRENTALPHA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelShader {
    Modulate,
    SelectArg1,
    SelectArg2,
    PreModulate,
    BlendCurrentAlpha,
}

impl PixelShader {
    /// The entry-point name in `src/shaders/legacy.hlsl`.
    #[must_use]
    pub const fn entry_point(self) -> &'static str {
        match self {
            Self::Modulate => "ps_modulate",
            Self::SelectArg1 => "ps_selectarg1",
            Self::SelectArg2 => "ps_selectarg2",
            Self::PreModulate => "ps_premodulate",
            Self::BlendCurrentAlpha => "ps_blendcurrentalpha",
        }
    }

    /// The same name as a `'static` C string, for the shader compiler's C ABI.
    #[must_use]
    pub const fn entry_point_c(self) -> &'static std::ffi::CStr {
        match self {
            Self::Modulate => c"ps_modulate",
            Self::SelectArg1 => c"ps_selectarg1",
            Self::SelectArg2 => c"ps_selectarg2",
            Self::PreModulate => c"ps_premodulate",
            Self::BlendCurrentAlpha => c"ps_blendcurrentalpha",
        }
    }

    #[must_use]
    pub const fn all() -> [Self; 5] {
        [
            Self::Modulate,
            Self::SelectArg1,
            Self::SelectArg2,
            Self::PreModulate,
            Self::BlendCurrentAlpha,
        ]
    }
}

/// The whole state machine, as data. Fifteen valid instances for the legacy surface path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PipelineKey {
    /// One of the five FVF families.
    pub vertex_format: VertexFormat,
    pub src_blend: Blend,
    /// `blendop` is always `ADD`.
    pub dst_blend: Blend,
    pub alpha_blend: bool,
    /// `alphafunc` is always `GREATEREQUAL` for surfaces (`GREATER` for `RenderMaterial` layers).
    pub alpha_test: bool,
    pub z_write: bool,
    /// `LESS` for the base pass, `LESSEQUAL` for a separate detail pass.
    pub z_func: ZFunc,
    /// `CW` unless `sides_type == 1`.
    pub cull: Cull,
    pub stage_ops: StageOps,
    pub fog: bool,
    /// Fixed-function lighting: on for objects, off for terrain.
    pub lighting: bool,
}

/// Everything the surface-binding call needs but the surface record does not carry.
#[derive(Debug, Clone, Copy)]
pub struct SurfaceContext {
    /// The polygon's stipple bit. Selects `WRAP` over `CLAMP` addressing and ORs `STIPPLED`
    /// into the surface type. Not a PSO field (it is a sampler), kept here because
    /// the surface setup reads it.
    pub tiled: bool,
    /// Single-pass detail texturing.
    pub detail_in_stage1: bool,
    /// Force alpha — set by the alpha-list flush (row 12).
    pub force_alpha: bool,
    /// The surface resolved to a real image texture.
    pub texture_is_set: bool,
    /// The texture has a palette — decides alpha ref 100 versus 200.
    pub texture_has_palette: bool,
    /// Current material: `None` when null, otherwise whether row 13 has alpha.
    pub material_has_alpha: Option<bool>,
    /// The polygon's sides type is 1.
    pub two_sided: bool,
    /// The fixed-function lighting enable the client sets, minus the sky term.
    pub lighting: bool,
    /// The device's current fog enable.
    pub fog_enabled: bool,
    /// The renderer is drawing the sky: suppresses the depth-mode write and forces lighting on.
    pub drawing_sky: bool,
    pub vertex_format: VertexFormat,
}

impl Default for SurfaceContext {
    fn default() -> Self {
        Self {
            tiled: false,
            detail_in_stage1: false,
            force_alpha: false,
            texture_is_set: true,
            texture_has_palette: false,
            material_has_alpha: None,
            two_sided: false,
            lighting: false,
            fog_enabled: true,
            drawing_sky: false,
            vertex_format: VertexFormat::XyzNormalDiffuseTex1,
        }
    }
}

/// Alpha-test reference for a palettised source: registry `RenderD3D.256AlphaTestRef`.
/// Never 128.
pub const ALPHA_TEST_REF_PALETTISED: u8 = 100;
/// Alpha-test reference for a DXT / other source.
pub const ALPHA_TEST_REF_BLOCK_COMPRESSED: u8 = 200;

/// The result of running the surface setup: the pipeline state, the alpha reference, and the vertex
/// alpha byte the caller writes into the diffuse colour.
#[derive(Debug, Clone, Copy)]
pub struct SurfaceState {
    pub key: PipelineKey,
    /// The reference value. **Only meaningful when `key.alpha_test` is set.** The alpha-test reference is set
    /// unconditionally with whatever the last clip-mapped surface left there, so a rebuild that
    /// hoists the reference must not treat it as meaningful otherwise; this field is 0 in that case.
    pub alpha_ref: u8,
    /// The vertex alpha the surface setup returns.
    pub vertex_alpha: u8,
    /// The alpha byte baked into the 1×1 solid-colour texture for an untextured surface, if any
    /// (surface setup step 2). Distinct from `vertex_alpha`, which step 6 then overwrites.
    pub solid_color: Option<u32>,
    /// Fog is disabled unless the device fog is on and the surface is not `ADDITIVE` — additive
    /// surfaces are drawn unfogged, because fogging an additive blend would brighten it.
    pub fog_alpha_disabled: bool,
}

impl PipelineKey {
    /// The exact translation of the client's surface binding.
    ///
    /// `alpha_ref` comes back separately because it is a root constant, not part of the PSO.
    /// See [`PipelineKey::state_from_surface`] for the full result including the vertex alpha.
    #[must_use]
    pub fn from_surface(s: &Surface, ctx: SurfaceContext) -> (Self, u8) {
        let st = Self::state_from_surface(s, ctx);
        (st.key, st.alpha_ref)
    }

    /// Bind the surface, updating material state in the order the retail client does.
    #[must_use]
    pub fn state_from_surface(s: &Surface, ctx: SurfaceContext) -> SurfaceState {
        // The surface type is the record's type | GOURAUD; STIPPLED is ORed in when the caller says tiled.
        let mut ty = s.r#type | st::GOURAUD;
        if ctx.tiled && ctx.texture_is_set {
            ty |= st::STIPPLED;
        }

        // Step 2. Texture or solid colour. The alpha of the 1x1 solid-colour texture is
        // (1 - translucency) * 255, truncated toward zero.
        let translucent_alpha = alpha_from_translucency(s.translucency);
        let solid_color = if ctx.texture_is_set {
            None
        } else {
            Some((u32::from(translucent_alpha) << 24) | (s.color_value & 0x00FF_FFFF))
        };

        // Step 4. Blend selection.
        let (mut src, mut dst, mut alpha_blend) = if ty & st::ALPHA != 0 || ctx.force_alpha {
            (
                Blend::SrcAlpha,
                if ty & st::ADDITIVE != 0 {
                    Blend::One
                } else {
                    Blend::InvSrcAlpha
                },
                true,
            )
        } else if ty & st::INVALPHA != 0 {
            (
                Blend::InvSrcAlpha,
                if ty & st::ADDITIVE != 0 {
                    Blend::One
                } else {
                    Blend::SrcAlpha
                },
                true,
            )
        } else if ty & st::ADDITIVE != 0 {
            (Blend::One, Blend::One, true)
        } else {
            (Blend::One, Blend::Zero, false)
        };

        // Step 5. Clip map (alpha test).
        let mut alpha_test = false;
        let mut alpha_ref = 0u8;
        if ty & st::BASE1_CLIPMAP != 0 && !ctx.force_alpha {
            if !alpha_blend {
                src = Blend::One;
                dst = Blend::InvSrcAlpha;
            }
            alpha_ref = if ctx.texture_is_set && ctx.texture_has_palette {
                ALPHA_TEST_REF_PALETTISED
            } else {
                ALPHA_TEST_REF_BLOCK_COMPRESSED
            };
            alpha_blend = true;
            alpha_test = true;
        }

        // Step 6. Translucency. A debug switch here is always 0, so the condition is
        // `!alphaBlend || alphaTest`. Note the ordering: this unconditionally rewrites the vertex alpha to
        // 0xFF for a non-TRANSLUCENT surface, including the untextured case step 2 just wrote.
        let vertex_alpha = if ty & st::TRANSLUCENT == 0 {
            0xFF
        } else {
            if !alpha_blend || alpha_test {
                src = Blend::SrcAlpha;
                dst = Blend::InvSrcAlpha;
                alpha_blend = true;
                alpha_test = false;
                alpha_ref = 0;
            }
            translucent_alpha
        };

        // Step 7. Depth write, including the material-has-alpha override.
        let z_func = ZFunc::Less; // the client's read-only depth-compare constant, 2.
        let z_write;
        let material_forces_alpha =
            matches!(ctx.material_has_alpha, Some(true)) && !(alpha_blend && !alpha_test);
        if material_forces_alpha {
            src = Blend::SrcAlpha;
            dst = Blend::InvSrcAlpha;
            alpha_blend = true;
            alpha_test = false;
            alpha_ref = 0;
            z_write = false;
        } else {
            z_write = !(alpha_blend && !alpha_test);
        }

        // Polygon and mesh-subset draws: cull CW unless the polygon is two-sided.
        // Two debug globals that could force CULLMODE_NONE are read-only constants whose values
        // were not recovered.
        // the six unnamed device toggles in the object draw path.
        // Implemented as "the debug override is off", which is the only setting under which the
        // shipped client culls back faces at all.
        let cull = if ctx.two_sided { Cull::None } else { Cull::Cw };

        // Meshes light when a debug lighting global is set or the sky is being drawn; terrain
        // never lights. The sky term is unconditional.
        // that global's value. Modelled as the caller's choice.
        let lighting = ctx.lighting || ctx.drawing_sky;

        // Step 8. Fog is disabled unless the device fog is on and the surface is not ADDITIVE.
        let fog_alpha_disabled = !(ctx.fog_enabled && ty & st::ADDITIVE == 0);

        let stage_ops = if ctx.detail_in_stage1 {
            StageOps::SINGLE_PASS_DETAIL
        } else {
            StageOps::BASE
        };

        SurfaceState {
            key: PipelineKey {
                vertex_format: ctx.vertex_format,
                src_blend: src,
                dst_blend: dst,
                alpha_blend,
                alpha_test,
                z_write,
                z_func,
                cull,
                stage_ops,
                fog: !fog_alpha_disabled,
                lighting,
            },
            alpha_ref,
            vertex_alpha,
            solid_color,
            fog_alpha_disabled,
        }
    }

    /// The separate detail pass — rows 14 and 15 of the catalogue.
    ///
    /// The single-pass detail fallback: blend the current detail source and destination blends with
    /// `ADD`, alpha blending on, depth test `LESSEQUAL` with depth writes on. Alpha test is left unchanged from the base
    /// pass, so `base` is threaded through.
    #[must_use]
    pub fn detail_second_pass(base: Self, content: DetailContent) -> Self {
        let (src, dst) = content.blend();
        Self {
            src_blend: src,
            dst_blend: dst,
            alpha_blend: true,
            z_write: true,
            z_func: ZFunc::LessEqual,
            ..base
        }
    }
}

/// The two debug masks the portal stamp selects between, read out of the retail client's own
/// data.
///
/// The outdoor stamp takes one, the indoor stamp the other.
/// Bit 0 replaces the stamped depth with the constant [`PORTAL_STAMP_FAR_DEPTH`], bit 1 forces the
/// vertex alpha to 0 so no colour is written, and bit 2 enables the depth write.
pub mod portal_stamp_mask {
    /// The **indoor** stamp, issued after the Z clear.
    /// Bits 1 and 2: the polygon's own depth, no colour, depth written.
    pub const INDOOR: u8 = 6;
    /// The **building** stamp, `mode == 1` of the outdoor portal pass. Bits 0, 1
    /// and 2: the far-plane constant, no colour, depth written.
    pub const BUILDING: u8 = 7;
    /// Bit 0 — stamp [`super::PORTAL_STAMP_FAR_DEPTH`] instead of the polygon's own depth.
    pub const CONSTANT_DEPTH: u8 = 1;
    /// Bit 2 — enable the depth write. Without it the draw is a no-op, because bit 1 has already
    /// masked the colour off.
    pub const DEPTH_WRITE: u8 = 4;
}

/// The constant device depth bit 0 of a portal-stamp mask selects, straight out of the client's
/// own constant. Not 1.0: the client stamps *just* short of the far plane.
pub const PORTAL_STAMP_FAR_DEPTH: f32 = 0.999_999;

impl PipelineKey {
    /// The device state the client sets before its one
    /// `DrawPrimitiveUP`, in order:
    ///
    /// ```text
    /// stage 0 texture: none
    /// alpha test:      off
    /// blend:           SRCALPHA, INVSRCALPHA, ADD
    /// depth:           test ALWAYS, write = (mask >> 2) & 1
    /// FVF:             0x144
    /// cull:            NONE
    /// ```
    ///
    /// Alpha blending is *enabled* — the client sets a blend function — and the vertex
    /// alpha is 0, which is what makes the draw write depth and nothing else. Fog and lighting are
    /// off: neither is touched here, and a pre-transformed vertex has no world position to light.
    #[must_use]
    pub const fn portal_stamp(mask: u8) -> Self {
        Self {
            vertex_format: VertexFormat::XyzRhwDiffuseTex1,
            src_blend: Blend::SrcAlpha,
            dst_blend: Blend::InvSrcAlpha,
            alpha_blend: true,
            alpha_test: false,
            z_write: mask & portal_stamp_mask::DEPTH_WRITE != 0,
            z_func: ZFunc::Always,
            cull: Cull::None,
            stage_ops: StageOps::BASE,
            fog: false,
            lighting: false,
        }
    }
}

/// The diffuse word one stamp vertex carries, exactly as the client packs it:
/// `((colour >> 8) & 0xFFFF) << 8 | (~(mask << 30) & 0x80000000) | (colour & 0xFF)`.
///
/// `colour` is the eight-hue debug cycle the stamp drives. The alpha byte is
/// the whole content: `mask << 30` puts bit 1 of the mask into bit 31, so alpha is **0** when bit 1
/// is set and `0x80` when it is clear. Both shipped masks set it, so no colour is ever written and
/// the hue is unobservable.
#[must_use]
pub const fn portal_stamp_diffuse(mask: u8, colour: u32) -> u32 {
    ((colour >> 8) & 0xFFFF) << 8 | (!((mask as u32) << 30) & 0x8000_0000) | (colour & 0xFF)
}

/// What the separate detail pass is drawn over. The blend factors are set by the caller before
/// drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailContent {
    /// The landscape detail surface.
    Landscape,
    /// A building or an environment cell.
    BuildingOrEnvCell,
}

impl DetailContent {
    #[must_use]
    pub const fn blend(self) -> (Blend, Blend) {
        match self {
            Self::Landscape => (Blend::SrcAlpha, Blend::InvSrcAlpha),
            Self::BuildingOrEnvCell => (Blend::DestColor, Blend::InvSrcAlpha),
        }
    }
}

/// `(1.0 - translucency) * 255.0`, truncated toward zero as the client does.
///
/// Surface setup steps 2 and 6. There is no clamp in the original, so a translucency outside `[0, 1]`
/// wraps through the same conversion the client would have done.
fn alpha_from_translucency(translucency: f32) -> u8 {
    let v = dereth_primitives::num::to_i32((1.0 - translucency) * 255.0);
    // The original stores the result into a byte, which keeps its low 8 bits.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        // LINT-OK: reproducing an 8-bit store of an already-integral value, not a float conversion.
        (v as u32 & 0xFF) as u8
    }
}

// --------------------------------------------------------------------------------------------
// The 15-row catalogue, as data, so a test can enumerate it.
// --------------------------------------------------------------------------------------------

/// One row of the "Surface flags × pass → D3D state" table.
#[derive(Debug, Clone, Copy)]
pub struct CatalogueRow {
    /// One-based row number in the surface-state catalogue below.
    pub row: u8,
    pub description: &'static str,
    pub src: Blend,
    pub dst: Blend,
    pub alpha_blend: bool,
    pub alpha_test: bool,
    /// `None` where the table prints "–".
    pub alpha_ref: Option<u8>,
    pub z_write: bool,
    pub z_func: ZFunc,
}

/// The PSO catalogue, verbatim from the 15-row table. This *is* the pipeline-state list: 15 states
/// cover the entire legacy surface path.
pub const CATALOGUE: [CatalogueRow; 15] = [
    CatalogueRow {
        row: 1,
        description: "opaque (BASE1_IMAGE or BASE1_SOLID only)",
        src: Blend::One,
        dst: Blend::Zero,
        alpha_blend: false,
        alpha_test: false,
        alpha_ref: None,
        z_write: true,
        z_func: ZFunc::Less,
    },
    CatalogueRow {
        row: 2,
        description: "ADDITIVE",
        src: Blend::One,
        dst: Blend::One,
        alpha_blend: true,
        alpha_test: false,
        alpha_ref: None,
        z_write: false,
        z_func: ZFunc::Less,
    },
    CatalogueRow {
        row: 3,
        description: "ALPHA",
        src: Blend::SrcAlpha,
        dst: Blend::InvSrcAlpha,
        alpha_blend: true,
        alpha_test: false,
        alpha_ref: None,
        z_write: false,
        z_func: ZFunc::Less,
    },
    CatalogueRow {
        row: 4,
        description: "ALPHA | ADDITIVE",
        src: Blend::SrcAlpha,
        dst: Blend::One,
        alpha_blend: true,
        alpha_test: false,
        alpha_ref: None,
        z_write: false,
        z_func: ZFunc::Less,
    },
    CatalogueRow {
        row: 5,
        description: "INVALPHA",
        src: Blend::InvSrcAlpha,
        dst: Blend::SrcAlpha,
        alpha_blend: true,
        alpha_test: false,
        alpha_ref: None,
        z_write: false,
        z_func: ZFunc::Less,
    },
    CatalogueRow {
        row: 6,
        description: "INVALPHA | ADDITIVE",
        src: Blend::InvSrcAlpha,
        dst: Blend::One,
        alpha_blend: true,
        alpha_test: false,
        alpha_ref: None,
        z_write: false,
        z_func: ZFunc::Less,
    },
    CatalogueRow {
        row: 7,
        description: "BASE1_CLIPMAP, palettised texture",
        src: Blend::One,
        dst: Blend::InvSrcAlpha,
        alpha_blend: true,
        alpha_test: true,
        alpha_ref: Some(ALPHA_TEST_REF_PALETTISED),
        z_write: true,
        z_func: ZFunc::Less,
    },
    CatalogueRow {
        row: 8,
        description: "BASE1_CLIPMAP, DXT/other texture",
        src: Blend::One,
        dst: Blend::InvSrcAlpha,
        alpha_blend: true,
        alpha_test: true,
        alpha_ref: Some(ALPHA_TEST_REF_BLOCK_COMPRESSED),
        z_write: true,
        z_func: ZFunc::Less,
    },
    CatalogueRow {
        row: 9,
        description: "BASE1_CLIPMAP | ALPHA",
        src: Blend::SrcAlpha,
        dst: Blend::InvSrcAlpha,
        alpha_blend: true,
        alpha_test: true,
        alpha_ref: None,
        z_write: true,
        z_func: ZFunc::Less,
    },
    CatalogueRow {
        row: 10,
        description: "TRANSLUCENT (any of the above)",
        src: Blend::SrcAlpha,
        dst: Blend::InvSrcAlpha,
        alpha_blend: true,
        alpha_test: false,
        alpha_ref: None,
        z_write: false,
        z_func: ZFunc::Less,
    },
    CatalogueRow {
        row: 11,
        description: "TRANSLUCENT | BASE1_CLIPMAP (alpha test dropped)",
        src: Blend::SrcAlpha,
        dst: Blend::InvSrcAlpha,
        alpha_blend: true,
        alpha_test: false,
        alpha_ref: None,
        z_write: false,
        z_func: ZFunc::Less,
    },
    CatalogueRow {
        row: 12,
        description: "any, with bForceAlpha (the alpha-list flush)",
        src: Blend::SrcAlpha,
        dst: Blend::InvSrcAlpha,
        alpha_blend: true,
        alpha_test: false,
        alpha_ref: None,
        z_write: false,
        z_func: ZFunc::Less,
    },
    CatalogueRow {
        row: 13,
        description: "any, while the current material has alpha",
        src: Blend::SrcAlpha,
        dst: Blend::InvSrcAlpha,
        alpha_blend: true,
        alpha_test: false,
        alpha_ref: None,
        z_write: false,
        z_func: ZFunc::Less,
    },
    CatalogueRow {
        row: 14,
        description: "landscape detail second pass",
        src: Blend::SrcAlpha,
        dst: Blend::InvSrcAlpha,
        alpha_blend: true,
        alpha_test: false,
        alpha_ref: None,
        z_write: true,
        z_func: ZFunc::LessEqual,
    },
    CatalogueRow {
        row: 15,
        description: "building / env-cell detail second pass",
        src: Blend::DestColor,
        dst: Blend::InvSrcAlpha,
        alpha_blend: true,
        alpha_test: false,
        alpha_ref: None,
        z_write: true,
        z_func: ZFunc::LessEqual,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::SurfaceHandler;

    fn surf(ty: u32) -> Surface {
        Surface {
            r#type: ty,
            handler: SurfaceHandler::Database,
            ..Surface::default()
        }
    }

    fn check(row: &CatalogueRow, s: &Surface, ctx: SurfaceContext) {
        let out = PipelineKey::state_from_surface(s, ctx);
        assert_eq!(out.key.src_blend, row.src, "row {} src", row.row);
        assert_eq!(out.key.dst_blend, row.dst, "row {} dst", row.row);
        assert_eq!(
            out.key.alpha_blend, row.alpha_blend,
            "row {} alphablend",
            row.row
        );
        assert_eq!(
            out.key.alpha_test, row.alpha_test,
            "row {} alphatest",
            row.row
        );
        assert_eq!(out.key.z_write, row.z_write, "row {} zwrite", row.row);
        assert_eq!(out.key.z_func, row.z_func, "row {} zfunc", row.row);
        if let Some(r) = row.alpha_ref {
            assert_eq!(out.alpha_ref, r, "row {} alpharef", row.row);
        }
    }

    // Oracle: the 15-row surface-flags-by-pass state table, driven through the transcription of
    // the surface binding in the same document's sections 1-8. Each row's inputs
    // are the flag combination the table names; the assertion is the table's own output columns.
    #[test]
    fn all_fifteen_catalogue_rows_are_reproduced_by_set_surface() {
        let base = SurfaceContext::default();

        // Row 1: opaque.
        check(&CATALOGUE[0], &surf(st::BASE1_IMAGE), base);
        check(&CATALOGUE[0], &surf(st::BASE1_SOLID), base);
        // Row 2: ADDITIVE.
        check(&CATALOGUE[1], &surf(st::BASE1_IMAGE | st::ADDITIVE), base);
        // Row 3: ALPHA.
        check(&CATALOGUE[2], &surf(st::BASE1_IMAGE | st::ALPHA), base);
        // Row 4: ALPHA | ADDITIVE.
        check(
            &CATALOGUE[3],
            &surf(st::BASE1_IMAGE | st::ALPHA | st::ADDITIVE),
            base,
        );
        // Row 5: INVALPHA.
        check(&CATALOGUE[4], &surf(st::BASE1_IMAGE | st::INVALPHA), base);
        // Row 6: INVALPHA | ADDITIVE.
        check(
            &CATALOGUE[5],
            &surf(st::BASE1_IMAGE | st::INVALPHA | st::ADDITIVE),
            base,
        );
        // Row 7: clip map with a palettised texture -> ref 100.
        check(
            &CATALOGUE[6],
            &surf(st::BASE1_IMAGE | st::BASE1_CLIPMAP),
            SurfaceContext {
                texture_has_palette: true,
                ..base
            },
        );
        // Row 8: clip map with a DXT texture -> ref 200.
        check(
            &CATALOGUE[7],
            &surf(st::BASE1_IMAGE | st::BASE1_CLIPMAP),
            SurfaceContext {
                texture_has_palette: false,
                ..base
            },
        );
        // Row 9: clip map | ALPHA keeps the alpha test and takes the ALPHA blend.
        check(
            &CATALOGUE[8],
            &surf(st::BASE1_IMAGE | st::BASE1_CLIPMAP | st::ALPHA),
            base,
        );
        // Row 10: TRANSLUCENT over an otherwise opaque surface.
        check(
            &CATALOGUE[9],
            &surf(st::BASE1_IMAGE | st::TRANSLUCENT),
            base,
        );
        // Row 11: TRANSLUCENT | CLIPMAP -- the alpha test is dropped.
        check(
            &CATALOGUE[10],
            &surf(st::BASE1_IMAGE | st::TRANSLUCENT | st::BASE1_CLIPMAP),
            base,
        );
        // Row 12: force alpha over a plain opaque surface (the alpha-list flush).
        check(
            &CATALOGUE[11],
            &surf(st::BASE1_IMAGE),
            SurfaceContext {
                force_alpha: true,
                ..base
            },
        );
        // Row 13: the material-has-alpha override over a plain opaque surface.
        check(
            &CATALOGUE[12],
            &surf(st::BASE1_IMAGE),
            SurfaceContext {
                material_has_alpha: Some(true),
                ..base
            },
        );
        // Rows 14 and 15: the separate detail pass, built on row 1's base state.
        let (row1, _) = PipelineKey::from_surface(&surf(st::BASE1_IMAGE), base);
        let land = PipelineKey::detail_second_pass(row1, DetailContent::Landscape);
        assert_eq!(
            (land.src_blend, land.dst_blend),
            (CATALOGUE[13].src, CATALOGUE[13].dst)
        );
        assert!(land.alpha_blend && land.z_write);
        assert_eq!(land.z_func, ZFunc::LessEqual);
        let bldg = PipelineKey::detail_second_pass(row1, DetailContent::BuildingOrEnvCell);
        assert_eq!(
            (bldg.src_blend, bldg.dst_blend),
            (CATALOGUE[14].src, CATALOGUE[14].dst)
        );
        assert_eq!(bldg.z_func, ZFunc::LessEqual);
    }

    // Oracle: 100 for a palettised source, 200 for a
    // block-compressed one -- never 128.
    #[test]
    fn alpha_test_references_are_100_and_200_never_128() {
        assert_eq!(ALPHA_TEST_REF_PALETTISED, 100);
        assert_eq!(ALPHA_TEST_REF_BLOCK_COMPRESSED, 200);
        let s = surf(st::BASE1_IMAGE | st::BASE1_CLIPMAP);
        let ctx = SurfaceContext::default();
        let (_, r) = PipelineKey::from_surface(
            &s,
            SurfaceContext {
                texture_has_palette: true,
                ..ctx
            },
        );
        assert_eq!(r, 100);
        let (_, r) = PipelineKey::from_surface(
            &s,
            SurfaceContext {
                texture_has_palette: false,
                ..ctx
            },
        );
        assert_eq!(r, 200);
        // A clip map whose texture did not resolve at all takes the block-compressed reference,
        // because the condition is "the texture is set and it has a palette".
        let (_, r) = PipelineKey::from_surface(
            &s,
            SurfaceContext {
                texture_is_set: false,
                texture_has_palette: true,
                ..ctx
            },
        );
        assert_eq!(r, 200);
    }

    // Oracle: section 5 of the same document -- "if (!alphaBlend) { src = ONE; dst = INVSRCALPHA; }"
    // fires only when the surface was not already blended, which is what distinguishes row 7/8 from
    // row 9.
    #[test]
    fn a_clip_map_over_an_already_blended_surface_keeps_its_blend() {
        let ctx = SurfaceContext::default();
        let (k, _) =
            PipelineKey::from_surface(&surf(st::BASE1_IMAGE | st::BASE1_CLIPMAP | st::ALPHA), ctx);
        assert_eq!(
            (k.src_blend, k.dst_blend),
            (Blend::SrcAlpha, Blend::InvSrcAlpha)
        );
        assert!(k.alpha_test);
        // ..and z-write survives, because (alpha_blend && !alpha_test) is false.
        assert!(k.z_write);
    }

    // Oracle: section 5's guard: `BASE1_CLIPMAP` set and force alpha clear. The alpha-list flush
    // therefore drops the alpha test even on foliage -- row 12 beats rows 7-9.
    #[test]
    fn force_alpha_suppresses_the_clip_map_branch_entirely() {
        let ctx = SurfaceContext {
            force_alpha: true,
            ..SurfaceContext::default()
        };
        let (k, r) = PipelineKey::from_surface(&surf(st::BASE1_IMAGE | st::BASE1_CLIPMAP), ctx);
        assert!(!k.alpha_test);
        assert_eq!(r, 0);
        assert_eq!(
            (k.src_blend, k.dst_blend),
            (Blend::SrcAlpha, Blend::InvSrcAlpha)
        );
    }

    // Oracle: section 6 -- "So a translucent clip-mapped surface loses its alpha test and becomes a
    // plain sorted blend." Row 11.
    #[test]
    fn translucency_drops_the_alpha_test() {
        let ctx = SurfaceContext {
            texture_has_palette: true,
            ..SurfaceContext::default()
        };
        let s = Surface {
            r#type: st::BASE1_IMAGE | st::BASE1_CLIPMAP | st::TRANSLUCENT,
            translucency: 0.25,
            ..Surface::default()
        };
        let out = PipelineKey::state_from_surface(&s, ctx);
        assert!(!out.key.alpha_test);
        assert!(out.key.alpha_blend);
        assert!(!out.key.z_write);
        // (1 - 0.25) * 255 = 191.25 -> 191 by truncation toward zero.
        assert_eq!(out.vertex_alpha, 191);
    }

    // Oracle: section 6's first line, which sets the vertex alpha to 0xFF for a non-TRANSLUCENT
    // surface and runs after section 2 has written the untextured surface's translucency-derived alpha into the
    // solid-colour texture. The two values are genuinely different things.
    #[test]
    fn an_untextured_surface_bakes_alpha_into_the_solid_texture_not_the_vertex() {
        let ctx = SurfaceContext {
            texture_is_set: false,
            ..SurfaceContext::default()
        };
        let s = Surface {
            r#type: st::BASE1_SOLID,
            translucency: 0.5,
            color_value: 0x0012_3456,
            ..Surface::default()
        };
        let out = PipelineKey::state_from_surface(&s, ctx);
        assert_eq!(out.vertex_alpha, 0xFF);
        // (1 - 0.5) * 255 = 127.5 -> 127.
        assert_eq!(out.solid_color, Some(0x7F12_3456));
    }

    // Oracle: section 7. The has_alpha override only fires when the surface is not *already* a
    // blended, non-alpha-tested draw -- `(alphaBlend && !alphaTest)` short-circuits it.
    #[test]
    fn the_material_alpha_override_does_not_fire_on_an_already_blended_surface() {
        let base = SurfaceContext::default();
        let ctx = SurfaceContext {
            material_has_alpha: Some(true),
            ..base
        };
        // An ADDITIVE surface is already (blend && !test), so it keeps ONE/ONE.
        let (k, _) = PipelineKey::from_surface(&surf(st::BASE1_IMAGE | st::ADDITIVE), ctx);
        assert_eq!((k.src_blend, k.dst_blend), (Blend::One, Blend::One));
        // A clip map is (blend && test), so the override does fire and eats the alpha test.
        let (k, r) = PipelineKey::from_surface(&surf(st::BASE1_IMAGE | st::BASE1_CLIPMAP), ctx);
        assert_eq!(
            (k.src_blend, k.dst_blend),
            (Blend::SrcAlpha, Blend::InvSrcAlpha)
        );
        assert!(!k.alpha_test);
        assert_eq!(r, 0);
        assert!(!k.z_write);
        // has_alpha == 0 takes the first branch, same as no material at all.
        let ctx0 = SurfaceContext {
            material_has_alpha: Some(false),
            ..base
        };
        let (k, _) = PipelineKey::from_surface(&surf(st::BASE1_IMAGE), ctx0);
        assert_eq!((k.src_blend, k.dst_blend), (Blend::One, Blend::Zero));
        assert!(k.z_write);
    }

    // Oracle: section 8's last line: fog is disabled unless the device fog is on and the surface
    // is not ADDITIVE.
    #[test]
    fn additive_surfaces_are_drawn_unfogged() {
        let ctx = SurfaceContext::default();
        let out = PipelineKey::state_from_surface(&surf(st::BASE1_IMAGE), ctx);
        assert!(out.key.fog);
        let out = PipelineKey::state_from_surface(&surf(st::BASE1_IMAGE | st::ADDITIVE), ctx);
        assert!(!out.key.fog);
        // ..and fog stays off when the device has it off, additive or not.
        let ctx = SurfaceContext {
            fog_enabled: false,
            ..ctx
        };
        let out = PipelineKey::state_from_surface(&surf(st::BASE1_IMAGE), ctx);
        assert!(!out.key.fog);
    }

    // Oracle: the stage-setup table in section 1 of the same document, and the renderer's five
    // pixel shaders.
    #[test]
    fn the_stage_op_chains_select_the_five_pixel_shaders() {
        assert_eq!(StageOps::BASE.pixel_shader(), PixelShader::Modulate);
        assert_eq!(
            StageOps::SINGLE_PASS_DETAIL.pixel_shader(),
            PixelShader::BlendCurrentAlpha
        );
        assert_eq!(StageOps::UI_OPAQUE.pixel_shader(), PixelShader::Modulate);
        assert_eq!(StageOps::UI_ALPHA.pixel_shader(), PixelShader::Modulate);
        assert_eq!(StageOps::TEXT.pixel_shader(), PixelShader::SelectArg2);
        // The PREMODULATE alpha op is what the detail chain's stage 0 uses; on its own (no live
        // stage 1) it selects the PREMODULATE shader.
        let mut only_stage0 = StageOps::SINGLE_PASS_DETAIL;
        only_stage0.stages[1] = StageOp::DISABLED;
        assert_eq!(only_stage0.pixel_shader(), PixelShader::PreModulate);
        assert_eq!(PixelShader::all().len(), 5);
    }

    // Oracle: the destination-alpha rewrite in the device state setup, section 5 of the same
    // document. The back buffer is X8R8G8B8, so this always fires.
    #[test]
    fn destination_alpha_blends_are_rewritten_to_source_alpha() {
        assert_eq!(
            Blend::DestAlpha.rewrite_for_no_destination_alpha(),
            Blend::SrcAlpha
        );
        assert_eq!(
            Blend::InvDestAlpha.rewrite_for_no_destination_alpha(),
            Blend::InvSrcAlpha
        );
        // Everything else passes through untouched.
        assert_eq!(
            Blend::DestColor.rewrite_for_no_destination_alpha(),
            Blend::DestColor
        );
        assert_eq!(Blend::One.rewrite_for_no_destination_alpha(), Blend::One);
    }

    // Oracle: the "Vertex alpha" note under the PSO table, and section 6.
    #[test]
    fn translucency_to_alpha_truncates_toward_zero() {
        assert_eq!(alpha_from_translucency(0.0), 255);
        assert_eq!(alpha_from_translucency(1.0), 0);
        assert_eq!(alpha_from_translucency(0.5), 127);
        assert_eq!(alpha_from_translucency(0.25), 191);
    }

    // Oracle: the polygon draw's cull-mode line in the same document's "Callers and the
    // surrounding state".
    #[test]
    fn two_sided_polygons_disable_culling() {
        let ctx = SurfaceContext::default();
        assert_eq!(
            PipelineKey::from_surface(&surf(st::BASE1_IMAGE), ctx)
                .0
                .cull,
            Cull::Cw
        );
        let ctx = SurfaceContext {
            two_sided: true,
            ..ctx
        };
        assert_eq!(
            PipelineKey::from_surface(&surf(st::BASE1_IMAGE), ctx)
                .0
                .cull,
            Cull::None
        );
    }

    // Oracle: SetSurface's prologue, which ORs GOURAUD into the surface type, and section 2, which
    // ORs in 0x40000000 when tiled. GOURAUD is unconditional; STIPPLED needs a resolved
    // texture, because that OR happens only when there is a texture.
    #[test]
    fn the_key_is_insensitive_to_gouraud_and_stipple_but_the_sampler_is_not() {
        let ctx = SurfaceContext::default();
        let a = PipelineKey::from_surface(&surf(st::BASE1_IMAGE), ctx).0;
        let b = PipelineKey::from_surface(&surf(st::BASE1_IMAGE | st::GOURAUD), ctx).0;
        let c = PipelineKey::from_surface(
            &surf(st::BASE1_IMAGE),
            SurfaceContext { tiled: true, ..ctx },
        )
        .0;
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    // Oracle: the fifteen-row table itself -- the catalogue is fifteen rows and no more.
    #[test]
    fn the_catalogue_has_exactly_fifteen_rows_numbered_one_to_fifteen() {
        assert_eq!(CATALOGUE.len(), 15);
        for (i, row) in CATALOGUE.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let expected = (i + 1) as u8;
            assert_eq!(row.row, expected);
        }
    }
}

#[cfg(test)]
mod portal_stamp_tests {
    use super::*;

    /// Oracle: the untextured arm's device state as retail sets it — no stage-0 texture, alpha
    /// test off, `SRCALPHA`/`INVSRCALPHA` blending, depth test `ALWAYS` with the write taken from
    /// mask bit 2, FVF `0x144`, no culling.
    ///
    /// `DEPTHTEST_ALWAYS` with the depth write **on** is the whole mechanism: the stamp does not
    /// compare, it overwrites. A key that compared would leave the landscape's depth in place and
    /// the stamp would do nothing at all.
    #[test]
    fn the_portal_stamp_writes_depth_unconditionally_and_never_compares() {
        for mask in [portal_stamp_mask::INDOOR, portal_stamp_mask::BUILDING] {
            let k = PipelineKey::portal_stamp(mask);
            assert_eq!(k.vertex_format, VertexFormat::XyzRhwDiffuseTex1);
            assert_eq!(k.vertex_format.fvf(), 0x144, "SetFVF(0x144)");
            assert_eq!(k.z_func, ZFunc::Always, "DEPTHTEST_ALWAYS");
            assert!(k.z_write, "both shipped masks set bit 2");
            assert_eq!(k.cull, Cull::None, "CULLMODE_NONE");
            assert!(!k.alpha_test, "alpha test off");
            assert!(k.alpha_blend);
            assert_eq!(
                (k.src_blend, k.dst_blend),
                (Blend::SrcAlpha, Blend::InvSrcAlpha)
            );
            assert!(!k.fog);
        }
        // Bit 2 clear is the one combination that would make the draw a no-op, and the key says so
        // rather than pretending the write is unconditional.
        assert!(!PipelineKey::portal_stamp(3).z_write);
    }

    /// Oracle: the same function's vertex loop — each vertex's diffuse keeps the colour's low 24
    /// bits and sets the alpha's top bit only when bit 1 of the mask is clear.
    ///
    /// The observed mask polarity makes alpha **0 when bit 1 of
    /// the mask is set**. Both shipped masks set it, which is why the stamp writes no colour.
    #[test]
    fn the_stamp_vertex_alpha_is_zero_for_both_shipped_masks() {
        for mask in [portal_stamp_mask::INDOOR, portal_stamp_mask::BUILDING] {
            assert_eq!(
                portal_stamp_diffuse(mask, 0x00FF_FFFF) & 0xFF00_0000,
                0,
                "mask {mask} must write alpha 0"
            );
        }
        // With bit 1 clear the client draws the debug hue at half transparency instead.
        assert_eq!(
            portal_stamp_diffuse(1, 0x00FF_FFFF) & 0xFF00_0000,
            0x8000_0000
        );
        // The RGB is carried through unchanged.
        assert_eq!(
            portal_stamp_diffuse(7, 0x00FF_00FF) & 0x00FF_FFFF,
            0x00FF_00FF
        );
    }

    /// Oracle: bit 0 of the mask — "makes the stamped depth a constant 0.999999 instead of the
    /// polygon's own". The building stamp (7) sets it, the indoor stamp (6) does not; that is the
    /// difference between "reset this opening to the far plane" and "restore this opening's own
    /// depth after the Z clear".
    #[test]
    fn only_the_building_stamp_writes_the_far_plane_constant() {
        assert_ne!(
            portal_stamp_mask::BUILDING & portal_stamp_mask::CONSTANT_DEPTH,
            0
        );
        assert_eq!(
            portal_stamp_mask::INDOOR & portal_stamp_mask::CONSTANT_DEPTH,
            0
        );
        // Just short of the far plane, not at it: the value retail uses is 0.999999, and
        // rounding it to 1.0 would put the stamp *at* the cleared depth, where `LESS` rejects
        // everything and the interior drawn next would never appear.
        assert_eq!(PORTAL_STAMP_FAR_DEPTH.to_bits(), 0.999_999_f32.to_bits());
        assert!(f32::from_bits(PORTAL_STAMP_FAR_DEPTH.to_bits()) < 1.0);
    }
}

/// The side rejected by the fixed-function cull mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CullFace {
    None,
    Front,
    Back,
}

/// Winding in framebuffer coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontFace {
    Clockwise,
    CounterClockwise,
}

/// The fixed-function pipeline uses clockwise front faces.
pub const FRONT_FACE: FrontFace = FrontFace::Clockwise;

impl Cull {
    /// Which side to reject when the viewport preserves the client's winding.
    #[must_use]
    pub const fn face(self) -> CullFace {
        match self {
            Self::None => CullFace::None,
            Self::Cw => CullFace::Front,
            Self::Ccw => CullFace::Back,
        }
    }
}

#[cfg(test)]
mod portable_state_tests {
    //! Behaviour: none (portable fixed-function state retains colour, alpha and winding meaning).
    use super::*;
    #[test]
    fn colour_factors_use_their_alpha_analogues_and_culling_uses_clockwise_fronts() {
        for (colour, alpha) in [
            (Blend::SrcColor, Blend::SrcAlpha),
            (Blend::InvSrcColor, Blend::InvSrcAlpha),
            (Blend::DestColor, Blend::DestAlpha),
            (Blend::InvDestColor, Blend::InvDestAlpha),
        ] {
            assert_eq!(colour.alpha_factor(), alpha);
        }
        for factor in [
            Blend::Zero,
            Blend::One,
            Blend::SrcAlpha,
            Blend::InvSrcAlpha,
            Blend::DestAlpha,
            Blend::InvDestAlpha,
        ] {
            assert_eq!(factor.alpha_factor(), factor);
        }
        assert_eq!(Cull::None.face(), CullFace::None);
        assert_eq!(Cull::Cw.face(), CullFace::Front);
        assert_eq!(Cull::Ccw.face(), CullFace::Back);
        assert_eq!(FRONT_FACE, FrontFace::Clockwise);
    }
}
