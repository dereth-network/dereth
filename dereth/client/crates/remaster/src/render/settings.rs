//! Every pass's settings past retail's own light, together: the shadows, the ambient occlusion,
//! the lamps, the sky, the post passes, the ground, what of an art pack is drawn, the water, the
//! weather, the bounced light, and the upscaler.

use super::ao::AoSettings;
use super::gi::GiSettings;
use super::lamps::LampSettings;
use super::landscape::LandscapeSettings;
use super::lod::LodSettings;
use super::post::PostSettings;
use super::rt_ao::RtAoSettings;
use super::rt_shadow::RtShadowSettings;
use super::shadow::ShadowSettings;
use super::sky::SkySettings;
use super::terrain::TerrainSettings;
use super::upscale::UpscaleSettings;
use super::water::WaterSettings;
use super::weather::WeatherSettings;
use crate::assets::pack::PackSettings;

/// Every pass's settings past retail's own light.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Settings {
    pub shadows: ShadowSettings,
    pub ao: AoSettings,
    pub lamps: LampSettings,
    pub sky: SkySettings,
    pub post: PostSettings,
    pub lod: LodSettings,
    pub terrain: TerrainSettings,
    /// What of an art pack is drawn.
    pub packs: PackSettings,
    /// How the water and the puddles are drawn.
    pub water: WaterSettings,
    /// Whether the rain and the snow fall.
    pub weather: WeatherSettings,
    /// How the bounced light is found.
    pub gi: GiSettings,
    /// Whether the sun's shadows and the occlusion are traced, where the device traces rays.
    pub rt_shadows: RtShadowSettings,
    pub rt_ao: RtAoSettings,
    /// How the frame is upscaled, and how far below the display's size it is drawn.
    pub upscale: UpscaleSettings,
    /// Which of the landscape's additions are drawn.
    pub landscape: LandscapeSettings,
}

impl Settings {
    /// Every pass off: retail's light alone, as the remaster draws it with nothing past it.
    #[must_use]
    pub fn off() -> Self {
        Self::default()
    }
}

impl super::Renderer {
    /// Every pass's settings at once, from the next frame on.
    pub fn set_settings(&mut self, s: Settings) {
        self.set_shadows(s.shadows);
        self.set_ao(s.ao);
        self.set_lamp_settings(s.lamps);
        self.set_sky(s.sky);
        self.set_post(s.post);
        self.lod.set_settings(s.lod);
        self.set_terrain(s.terrain);
        self.set_packs(s.packs);
        self.set_water(s.water);
        self.set_weather(s.weather);
        self.set_gi(s.gi);
        self.set_rt_shadows(s.rt_shadows);
        self.set_rt_ao(s.rt_ao);
        self.set_upscale(s.upscale);
        self.set_landscape(s.landscape);
    }

    /// Every pass's settings.
    #[must_use]
    pub fn settings(&self) -> Settings {
        Settings {
            shadows: self.shadows().0,
            ao: self.ao_settings(),
            lamps: self.lamps().settings(),
            sky: self.sky().0,
            post: self.post_settings(),
            lod: self.lod.settings(),
            terrain: self.terrain(),
            packs: self.packs(),
            water: self.water(),
            weather: self.weather(),
            gi: self.gi(),
            rt_shadows: self.rt_shadows(),
            rt_ao: self.rt_ao(),
            upscale: self.upscale(),
            landscape: self.landscape(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Settings;

    /// Off, no pass runs: no shadows, no occlusion, no lamps, neither the atmosphere nor the fog's
    /// volume, no post pass, nothing of an art pack, neither the water nor the puddles, no
    /// weather, nothing traced, and no upscaler.
    #[test]
    fn off_runs_no_pass() {
        let s = Settings::off();
        assert!(!s.shadows.on);
        assert!(!s.ao.on);
        assert!(!s.lamps.any());
        assert!(!s.sky.atmosphere && !s.sky.fog);
        assert!(!s.post.runs());
        assert!(!s.lod.on);
        assert!(!s.terrain.splat);
        assert!(!s.packs.any());
        assert!(!s.water.runs());
        assert!(!s.weather.on);
        assert!(!s.gi.on, "no bounced light");
        assert!(!s.rt_shadows.on && !s.rt_ao.on);
        assert!(!s.upscale.runs());
    }
}
