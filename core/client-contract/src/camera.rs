//! The projection *parameters* every renderer and every scene agree on: the near and far planes,
//! the FOV and aspect constants and the two functions that derive one from the other, the
//! fixed-function fog and light blocks, and `ViewParams`, which carries all of it.
//!
//! These live here because `dereth_client_runtime::present::Scene` --
//! `fn view_params(&self, w: u32, h: u32) -> ViewParams` -- has to be nameable by a crate that may
//! not depend on a renderer, as with `Viewport` and `GameView`: the type both sides agree on
//! belongs under both. `dereth_render_cpu::camera` re-exports every item here, so
//! `dereth_render::camera::ViewParams` still resolves.
//!
//! The arithmetic is not here. `projection`, `perspective_fov_lh`, `view_from_frame`,
//! `swap_forward_and_up`, `clamp_viewport` and `compute_game_viewport` belong to the renderer, in
//! `dereth-render-cpu`.

use dereth_primitives::num::math;
use glam::Mat4;

pub use dereth_primitives::Viewport;

/// The renderer's near plane, set to 0.1 during initialization.
pub const ZNEAR: f32 = 0.1;
/// The far plane the renderer is set up with.
pub const ZFAR: f32 = 4000.0;
/// The sky pass temporarily multiplies `zfar` by four, yielding 16000 at the default distance.
pub const ZFAR_SKY: f32 = ZFAR * 4.0;
/// The field-of-view preference's default, in degrees.
pub const DEFAULT_FOV_DEGREES: f32 = 90.0;
/// The default-field-of-view setter converts the preference with this constant, not with a
/// full-precision π/180. The value is the one compiled into the client.
pub const DEG_TO_RAD: f32 = 0.017_453_292;
/// The viewport-aspect calculation's constant factor.
pub const ASPECT_FACTOR: f32 = 0.75;
/// The subtrahend in `pref_fov_rad / (viewport_aspect − 0.1)`.
/// The subtraction by 0.1 is present in the shipped calculation.
pub const FOV_ASPECT_BIAS: f32 = 0.1;

/// `Render.AspectRatio`, choices `Auto, Normal, Wide`, as the render device sets the display
/// aspect ratio up from them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AspectPreference {
    /// The window's own ratio.
    Auto = 0,
    /// 4:3.
    #[default]
    Normal = 1,
    /// 16:9.
    Wide = 2,
}

impl AspectPreference {
    /// The display aspect ratio: pref 1 → 4/3, pref 2 → 16/9, otherwise `display_w / display_h`.
    #[must_use]
    pub fn display_aspect_ratio(self, display_w: f32, display_h: f32) -> f32 {
        match self {
            Self::Normal => 4.0 / 3.0,
            Self::Wide => 16.0 / 9.0,
            Self::Auto => display_w / display_h,
        }
    }
}

/// The viewport aspect.
///
/// `raw` → `w/h`; otherwise `(w/h) * display_aspect_ratio * 0.75`. With the default 4:3 preference
/// the two cancel and a full-window viewport gives exactly `w/h`; "Wide" multiplies by
/// `(16/9)*(3/4) = 4/3`.
#[must_use]
pub fn compute_aspect_for_viewport(
    viewport_w: f32,
    viewport_h: f32,
    display_aspect_ratio: f32,
    raw: bool,
) -> f32 {
    let ratio = viewport_w / viewport_h;
    if raw {
        ratio
    } else {
        ratio * display_aspect_ratio * ASPECT_FACTOR
    }
}

/// The FOV actually handed to the projection: `pref_fov_rad / (viewport_aspect − 0.1)`.
/// `pref_fov_rad` is `Render.FieldOfView * 0.017453292`.
#[must_use]
pub fn fov_y_from_preference(pref_fov_rad: f32, viewport_aspect: f32) -> f32 {
    pref_fov_rad / (viewport_aspect - FOV_ASPECT_BIAS)
}

/// The alternative "view distance" projection used by the
/// teleport effect and the examination preview: `fov = 2·atan(1/d)` and
/// `znear = (d >= 0.4) ? 0.1 : d * 0.25`.
#[must_use]
pub fn set_vdst(d: f32) -> (f32, f32) {
    let fov = 2.0 * math::atanf(1.0 / d);
    let znear = if d >= 0.4 { ZNEAR } else { d * 0.25 };
    (fov, znear)
}

/// The view distance with the field-of-view override **off**: `1 / tan(fov_y / 2)`.
///
/// The exact inverse of [`set_vdst`]'s FOV, and what the teleport animation caches as the game
/// view distance so both ends of the teleport ramp are the view the player actually had.
#[must_use]
pub fn view_distance_from_fov(fov_y_rad: f32) -> f32 {
    1.0 / math::tanf(fov_y_rad * 0.5)
}

/// Distance fog, as the world scene records it. The renderer only uploads it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FogParams {
    /// Packed the same way as the clear colour.
    pub color: u32,
    pub near: f32,
    pub far: f32,
    pub enabled: bool,
}

impl Default for FogParams {
    fn default() -> Self {
        // Retail's default device states: white fixed-function fog, near 0.0, far 1000.0; disabled.
        Self {
            color: 0x00FF_FFFF,
            near: 0.0,
            far: 1000.0,
            enabled: false,
        }
    }
}

/// One fixed-function light slot. `caps.MaxActiveLights` is clamped to 8.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Light {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub falloff: f32,
    pub enabled: bool,
}

/// The eight fixed-function light slots. Filled by the world scene; the renderer only uploads it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LightBlock {
    pub lights: [Light; 8],
    pub count: u32,
    /// `D3DRS_AMBIENT`, packed ARGB.
    pub ambient: u32,
}

/// Everything the renderer needs to build its per-frame matrices.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewParams {
    /// Built from a `Frame` with the Z-up ↔ D3D y/z swap; see `view_from_frame`.
    pub view: Mat4,
    /// `= pref_fov / (viewport_aspect − 0.1)`.
    pub fov_y_rad: f32,
    /// `display_aspect_ratio × viewport ratio × 0.75`.
    pub aspect: f32,
    pub znear: f32,
    /// 4000.0, ×4 during the sky pass.
    pub zfar: f32,
    pub fog: FogParams,
    /// Filled by the world scene; the renderer only uploads it.
    pub lights: LightBlock,
    /// The 3D viewport rectangle, as the UI computes it for the game view.
    pub viewport: Viewport,
}

impl Default for ViewParams {
    fn default() -> Self {
        let aspect = compute_aspect_for_viewport(
            800.0,
            600.0,
            AspectPreference::Normal.display_aspect_ratio(800.0, 600.0),
            false,
        );
        Self {
            view: Mat4::IDENTITY,
            fov_y_rad: fov_y_from_preference(DEFAULT_FOV_DEGREES * DEG_TO_RAD, aspect),
            aspect,
            znear: ZNEAR,
            zfar: ZFAR,
            fog: FogParams::default(),
            lights: LightBlock::default(),
            viewport: Viewport {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            },
        }
    }
}
