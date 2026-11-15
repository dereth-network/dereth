//! The rain on the world: what stands out in it is soaked, darker and deeper in color, with a
//! faint sheen of the overcast air where it is seen edge on (the shared passes' weather,
//! [`dereth_gfx_passes::weather`], worked out in each draw of the land and of what stands on it);
//! and none of it reaches what stands under a roof, by the overhead map of what stands over each
//! point round the eye, drawn from straight above with the frame's own casters (every object of
//! the detailed blocks, as the sun's cascades take them; bodies left out, as a body shelters
//! nothing it doesn't hide already).
//!
//! How wet the world is follows the rain amount retail's own streaks are drawn by: the wet comes in
//! as the streaks do, and is full at the heaviest rain. Rooms and what stands in them never are.

use auberean_draw::constants::{DrawUniforms, FrameUniforms};
use auberean_draw::material::d3d;
use auberean_draw::mesh::PosedVertex;
use auberean_draw::picture::Image;
use auberean_draw::scene::Frame;
use dereth_gfx::{Pipeline, Recorder, Texture};
use dereth_gfx_passes::frame::{PerFrame, Rows};
use dereth_gfx_passes::weather::{record_cover, Cover, State, COVER_SIDE};
use glam::Vec3;

use super::sun::Caster;
use super::{Key, Program, Remaster, RemasterError};

/// How the rain is laid on the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeatherSettings {
    /// Whether what stands out in the rain is wet while it rains.
    pub wet: bool,
    /// Whether the overhead map keeps the wet off what stands under a roof (off: every point is
    /// open to the sky).
    pub shelter: bool,
}

impl Default for WeatherSettings {
    fn default() -> Self {
        Self {
            wet: false,
            shelter: true,
        }
    }
}

/// The rain amounts the wet comes in over: none at the first, full at the second. Retail draws its
/// streaks from 0.4.
const WET_RAIN: (f32, f32) = (0.3, 0.8);

/// How wet what stands out in rain of amount `rain` is, 0 dry to 1 soaked.
#[must_use]
pub fn soak(rain: f32) -> f32 {
    let t = ((rain - WET_RAIN.0) / (WET_RAIN.1 - WET_RAIN.0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// What the rain draws this frame: its constants, and the overhead map with what stands over each
/// point drawn into it.
#[derive(Debug, Clone)]
pub(super) struct WeatherPlan {
    /// The weather's constants, by their index in the table.
    pub(super) constants: u32,
    /// The overhead map, and its casters with their constants and pipeline (none with the shelter
    /// off: the map is left clear, every point open).
    pub(super) map: Texture,
    casters: Vec<Caster>,
    frame: u32,
    pipeline: Pipeline,
}

impl Remaster {
    /// Work out the rain on `frame`: none when the wet is off or it doesn't rain enough. The
    /// overhead map's casters' constants go into `records` (lit by the outdoors' light `out_u`),
    /// a moved body's pose into `posed`.
    ///
    /// # Errors
    /// What the device refuses.
    pub(super) fn plan_weather(
        &mut self,
        frame: &Frame,
        eye: Vec3,
        out_u: &FrameUniforms,
        records: &mut Vec<DrawUniforms>,
        posed: &mut Vec<PosedVertex>,
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> Result<Option<WeatherPlan>, RemasterError> {
        let wet = soak(frame.rain);
        if !self.weather_settings.wet || wet <= 0.0 {
            return Ok(None);
        }
        let (map, map_index) = match self.shelter {
            Some(m) => m,
            None => {
                let m = dereth_gfx_passes::weather::cover_map(&mut self.gfx)?;
                self.shelter = Some(m);
                m
            }
        };
        let placed = Cover::around(eye);
        // The map's constants: the outdoors' light seen from straight above.
        let mut u = *out_u;
        u.view_proj = placed.view_proj().to_cols_array_2d();
        let bytes = Self::light_rows(&u, glam::Mat4::IDENTITY, (COVER_SIDE, COVER_SIDE));
        self.shelter_light.write(&mut self.gfx, &bytes)?;
        let mut casters = Vec::new();
        if self.weather_settings.shelter {
            for b in frame.shadow.iter().flat_map(|s| &s.casters) {
                if b.indices.is_empty() || b.posed.is_some() {
                    continue;
                }
                let (centre, radius) = self.reach_of(b);
                if !placed.reaches(centre, radius) {
                    continue;
                }
                casters.push(self.caster(b, out_u.sun_color, records, posed, load)?);
            }
        }
        let mut rows = Rows::default();
        dereth_gfx_passes::weather::rows(
            &mut rows,
            &State {
                wet,
                ..State::default()
            },
            &placed,
            map_index,
        );
        self.weather_constants.write(&mut self.gfx, &rows.bytes())?;
        let pipeline = self.pipeline(Key {
            program: Program::MeshCaster,
            src: d3d::BLEND_ONE,
            dst: d3d::BLEND_ZERO,
            depth_test: true,
            depth_write: true,
            cull: d3d::CULL_NONE,
            post: false,
        })?;
        Ok(Some(WeatherPlan {
            constants: self.weather_constants.index,
            map,
            casters,
            frame: self.shelter_light.index,
            pipeline,
        }))
    }
}

impl WeatherPlan {
    /// Draw the overhead map: what stands over each point, from straight above. `draws` and
    /// `posed` are the frame's constants' and poses' indices in the table; `white` and `wrap` the
    /// picture and sampler a stage without one takes.
    ///
    /// # Errors
    /// What the layer refuses.
    pub(super) fn record_map(
        &self,
        r: &mut Recorder<'_>,
        draws: u32,
        posed: u32,
        blank: (u32, u32),
    ) -> Result<(), dereth_gfx::Error> {
        record_cover(r, self.map, &[], |d| {
            if !self.casters.is_empty() {
                d.set_pipeline(self.pipeline)?;
            }
            for c in &self.casters {
                c.draw(d, self.frame, draws, posed, blank)?;
            }
            Ok(())
        })
    }
}

/// A per-frame buffer for the overhead map's constants, or the weather's.
pub(super) fn buffer(
    gfx: &mut dereth_gfx::Device,
    label: &'static str,
) -> Result<PerFrame, dereth_gfx::Error> {
    PerFrame::new(gfx, label, 4096, dereth_gfx::BufferUsage::default())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// No wet before the streaks are drawn, full at the heaviest rain, and more the more it rains.
    #[test]
    fn the_wet_comes_in_with_the_streaks_and_is_full_at_the_heaviest_rain() {
        assert_eq!(soak(0.0), 0.0);
        assert_eq!(soak(0.3), 0.0);
        assert!(soak(0.4) > 0.0, "the streaks are drawn from 0.4");
        assert!(soak(0.5) < soak(0.7));
        assert_eq!(soak(0.8), 1.0);
        assert_eq!(soak(1.0), 1.0);
    }
}
