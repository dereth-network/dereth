//! The device-free half of a derived pipeline: its entry points and its colour targets. The rest
//! of its state -- vertex layout, blend, depth compare and write, cull -- is the ordinary
//! pipeline's, taken from the same key.

use super::reshade::SPLAT_STAGE;
use super::{DerivedKey, Variant};

/// The vertex stage's entry point, the ordinary one's in every variant.
pub const VERTEX_ENTRY: &str = "vs_main";

/// The re-shaded colour target: linear light, unclamped.
pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// The re-shaded normal target: the view-space normal, with the material class in alpha.
pub const NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

impl DerivedKey {
    /// The pixel stage's entry point: the ordinary pipeline's.
    #[must_use]
    pub const fn fragment_entry(&self) -> &'static str {
        if self.splat {
            SPLAT_STAGE
        } else {
            self.key.stage_ops.pixel_shader().entry_point()
        }
    }

    /// The colour targets of this pipeline, given the ordinary pipeline's blend; `None` for a
    /// variant without colour targets of its own yet.
    #[must_use]
    pub fn targets(
        &self,
        ordinary_blend: Option<wgpu::BlendState>,
    ) -> Option<[wgpu::ColorTargetState; 2]> {
        match self.variant {
            // A material draws as the re-shade does, blend and cut-out alike.
            Variant::Reshade | Variant::Material => {
                Some(reshade_targets(ordinary_blend, self.key.alpha_test))
            }
            _ => None,
        }
    }

    /// Whether this draw is solid, in a variant that draws colour: it writes an alpha of 1.
    #[must_use]
    pub fn solid_in_display_light(&self) -> bool {
        matches!(self.variant, Variant::Reshade | Variant::Material) && !self.key.alpha_blend
    }
}

/// The re-shaded pipeline's two colour targets.
///
/// The colour blends as the ordinary pipeline does. A draw that does not blend writes alpha too,
/// so what a damp material said there (past 1, how damp it is) never outlives what covers it; one
/// that blends leaves alpha alone, like the ordinary pipeline. The normal is written only by a
/// draw that does not blend, or that blends but is cut out against its texture (the leaf cards of
/// most trees, drawn with both): a translucent surface leaves the normal of what is behind it, but
/// a cut-out one covers it wherever it is drawn at all.
#[must_use]
pub fn reshade_targets(
    ordinary_blend: Option<wgpu::BlendState>,
    cut_out: bool,
) -> [wgpu::ColorTargetState; 2] {
    [
        wgpu::ColorTargetState {
            format: HDR_FORMAT,
            blend: ordinary_blend,
            write_mask: if ordinary_blend.is_some() {
                wgpu::ColorWrites::COLOR
            } else {
                wgpu::ColorWrites::ALL
            },
        },
        wgpu::ColorTargetState {
            format: NORMAL_FORMAT,
            blend: None,
            write_mask: if ordinary_blend.is_some() && !cut_out {
                wgpu::ColorWrites::empty()
            } else {
                wgpu::ColorWrites::ALL
            },
        },
    ]
}
