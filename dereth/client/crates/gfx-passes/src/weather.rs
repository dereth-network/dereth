//! The weather as the world's surfaces take it, naming nothing of the game: the overhead map of
//! what stands over each point round the eye, where it lies, and the weather's first constants'
//! rows. The Slang module `shaders/weather.slang` reads them: how wet a surface out in the rain
//! is, how much of the flat ground is puddle, and the shelter that keeps both off what stands
//! under a roof.
//!
//! The game draws what stands overhead into the map ([`record_cover`]) with its own casters,
//! from straight above ([`Cover::view_proj`]), and adds its own rows after the first ones
//! ([`rows`]).

use dereth_gfx::{
    DepthAttachment, Device, Draw, Format, RasterPass, Recorder, Rect, Texture, TextureDesc,
    TextureUsage, Touch,
};
use glam::{Mat4, Vec3, Vec4};

use crate::frame::Rows;

/// The overhead map's side, texels.
pub const COVER_SIDE: u32 = 2048;
/// How far the overhead map reaches from its centre, meters.
pub const COVER_REACH: f32 = 128.0;
/// The steps the overhead map's centre moves in, meters: it is drawn again only when the eye
/// crosses one.
pub const COVER_STEP: f32 = 32.0;
/// How far above and below the eye the overhead map sees, meters, and the steps its top moves in.
const COVER_DEPTH_REACH: f32 = 300.0;
const COVER_DEPTH_STEP: f32 = 64.0;
/// The overhead map's depth.
pub const COVER_FORMAT: Format = Format::D32Float;
/// Where the wet starts to fade with distance, and where it is gone, meters.
pub const WET_FADE: [f32; 2] = [120.0, 260.0];

/// Where the overhead map stands: its centre (the frame's space) in whole steps of
/// [`COVER_STEP`], how far it reaches, and the heights its depth spans.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cover {
    pub centre: [f32; 2],
    pub reach: f32,
    /// The height its depth starts at: depth 0 is this high, depth 1 is `span` below it.
    pub top: f32,
    pub span: f32,
}

impl Cover {
    /// The map round `eye`.
    #[must_use]
    pub fn around(eye: Vec3) -> Self {
        let snap = |v: f32| (v / COVER_STEP).round() * COVER_STEP;
        Self {
            centre: [snap(eye.x), snap(eye.y)],
            reach: COVER_REACH,
            top: ((eye.z + COVER_DEPTH_REACH) / COVER_DEPTH_STEP).ceil() * COVER_DEPTH_STEP,
            span: 2.0 * COVER_DEPTH_REACH,
        }
    }

    /// The view and projection that draw the map: straight down, the frame's `x` across and `y`
    /// up the map, the depth 0 at [`Self::top`].
    #[must_use]
    pub fn view_proj(&self) -> Mat4 {
        let (r, s) = (self.reach, self.span);
        Mat4::from_cols(
            Vec4::new(1.0 / r, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 1.0 / r, 0.0, 0.0),
            Vec4::new(0.0, 0.0, -1.0 / s, 0.0),
            Vec4::new(-self.centre[0] / r, -self.centre[1] / r, self.top / s, 1.0),
        )
    }

    /// Whether a sphere at `centre` of `radius` reaches into the map.
    #[must_use]
    pub fn reaches(&self, centre: Vec3, radius: f32) -> bool {
        (centre.x - self.centre[0]).abs() <= self.reach + radius
            && (centre.y - self.centre[1]).abs() <= self.reach + radius
            && centre.z - radius <= self.top
            && centre.z + radius >= self.top - self.span
    }
}

/// The weather's state, as its first row and the world's place give it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct State {
    /// How wet what stands out in the rain is, 0 dry to 1 soaked.
    pub wet: f32,
    /// How much the ground gathers puddles, 0 none to 1 in full.
    pub gathers: f32,
    /// The weather's clock, seconds within its period.
    pub time: f32,
    /// How much of what falls is snow.
    pub snow: f32,
    /// Where the frame's origin stands in the world, meters.
    pub origin: [f32; 2],
}

/// The weather's first four rows (`W_STATE` to `W_COVER_DEPTH` in `shaders/weather.slang`): the
/// state, where the world stands and where the wet fades, and the overhead map `placed`, by its
/// index `map` in the table.
#[allow(clippy::cast_precision_loss)] // the map's side
pub fn rows(rows: &mut Rows, state: &State, placed: &Cover, map: u32) {
    rows.0
        .push([state.wet, state.gathers, state.time, state.snow].map(f32::to_bits));
    rows.0
        .push([state.origin[0], state.origin[1], WET_FADE[0], WET_FADE[1]].map(f32::to_bits));
    rows.0.push(
        [
            placed.centre[0],
            placed.centre[1],
            placed.reach,
            COVER_SIDE as f32,
        ]
        .map(f32::to_bits),
    );
    rows.0.push([
        placed.top.to_bits(),
        placed.span.to_bits(),
        map,
        1.0f32.to_bits(),
    ]);
}

/// The overhead map's picture, and its index in the table.
///
/// # Errors
/// What the device refuses.
pub fn cover_map(d: &mut Device) -> Result<(Texture, u32), dereth_gfx::Error> {
    let map = d.create_texture(&TextureDesc {
        label: "remaster weather cover",
        width: COVER_SIDE,
        height: COVER_SIDE,
        mips: 1,
        format: COVER_FORMAT,
        usage: TextureUsage::DEPTH_TARGET | TextureUsage::SAMPLED,
    })?;
    Ok((map, d.bind_texture(map)?))
}

/// Draw the overhead map `map` again: cleared, then what `draw` draws into it from straight
/// above, `touches` what that reads.
///
/// # Errors
/// What the layer refuses.
pub fn record_cover(
    r: &mut Recorder<'_>,
    map: Texture,
    touches: &[Touch],
    draw: impl FnOnce(&mut Draw<'_>) -> Result<(), dereth_gfx::Error>,
) -> Result<(), dereth_gfx::Error> {
    r.timestamp("weather cover");
    r.raster(
        &RasterPass {
            name: "weather cover",
            color: &[],
            depth: Some(DepthAttachment {
                texture: map,
                clear: Some(1.0),
                write: true,
            }),
            touches,
            area: Some(Rect {
                x: 0,
                y: 0,
                width: COVER_SIDE,
                height: COVER_SIDE,
            }),
        },
        draw,
    )
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// The map stands on whole steps round the eye, and draws a point at its top over its
    /// centre at depth 0 and one a span below at depth 1.
    #[test]
    fn the_map_stands_on_whole_steps_and_spans_its_heights() {
        let c = Cover::around(Vec3::new(45.0, -17.0, 12.0));
        assert_eq!(c.centre, [32.0, -32.0]);
        let vp = c.view_proj();
        let top = vp * Vec4::new(32.0, -32.0, c.top, 1.0);
        let bottom = vp * Vec4::new(32.0 + c.reach, -32.0, c.top - c.span, 1.0);
        assert!(top.z.abs() < 1e-6 && top.x.abs() < 1e-6, "{top}");
        assert!(
            (bottom.z - 1.0).abs() < 1e-6 && (bottom.x - 1.0).abs() < 1e-6,
            "{bottom}"
        );
        assert!(c.reaches(Vec3::new(32.0, -32.0, 12.0), 1.0));
        assert!(!c.reaches(Vec3::new(32.0 + c.reach + 5.0, -32.0, 12.0), 1.0));
    }

    /// The first rows say the state, the world's place and the map's, as the shaders read them.
    #[test]
    fn the_first_rows_say_the_state_and_the_maps_place() {
        let mut r = Rows::default();
        let s = State {
            wet: 0.5,
            ..State::default()
        };
        rows(&mut r, &s, &Cover::around(Vec3::ZERO), 7);
        assert_eq!(r.0.len(), 4);
        assert_eq!(f32::from_bits(r.0[0][0]), 0.5);
        assert_eq!(r.0[3][2], 7);
        assert_eq!(f32::from_bits(r.0[1][3]), WET_FADE[1]);
    }
}
