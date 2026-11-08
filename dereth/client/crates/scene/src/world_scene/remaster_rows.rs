//! The remaster's own boxes on the Horizon page, as the scene carries them out: each turns one
//! thing the remaster can draw on or off, live. Off, each leaves that thing as it was asked for
//! another way, so a capture or a test that sets one keeps it.

use dereth_client_runtime::render_prefs::{FidelityFeature as F, FidelityPreferences};
use dereth_remaster::render::lod::LodSettings;
use dereth_render::device::Gpu;

use super::remaster_effects::{
    RemasterAo, RemasterPost, RemasterUpscale, RemasterUpscaleMode, RemasterWater,
};
use super::remaster_lamps::RemasterLamps;
use super::remaster_pack::RemasterPacks;
use super::remaster_rt::RemasterGi;
use super::remaster_traced::{RemasterTracedAo, RemasterTracedShadows};
use super::remaster_view::{RemasterLight, ShadowSettings, SkySettings};
use super::remaster_weather::RemasterWeather;
use super::SceneDraw;

/// How much of the ground past the game's fog comes through the fog, as the player draws it: the
/// far mountains keep their shape and some of their color against the sky.
const FAR_KEEP: f32 = 0.2;

/// What the remaster's own boxes ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RemasterRows {
    pub shadows: bool,
    pub sky: bool,
    pub fog: bool,
    pub occlusion: bool,
    pub indoor_lamps: bool,
    pub outdoor_lamps: bool,
    pub bloom: bool,
    pub smoothing: bool,
    pub water: bool,
    pub puddles: bool,
    pub rain: bool,
    pub detail: bool,
    pub bounce: bool,
    pub traced_shadows: bool,
    pub traced_occlusion: bool,
    pub upscale: bool,
    pub dlss: bool,
    /// The share of the pixels drawn each way while upscaling, in percent; 0 until one is chosen.
    pub scale: u32,
    /// How strongly the sunlit air shows shafts of light: 0 none, 1 low, 2 high.
    pub god_rays: u32,
    /// What of the art pack is drawn.
    pub packs: RemasterPacks,
}

impl RemasterRows {
    /// What the page asks, in effect.
    #[must_use]
    pub fn of(f: &FidelityPreferences) -> Self {
        Self {
            shadows: f.remaster_feature(F::RemasterShadows),
            sky: f.remaster_feature(F::RemasterSky),
            fog: f.remaster_feature(F::RemasterFog),
            occlusion: f.remaster_feature(F::RemasterOcclusion),
            indoor_lamps: f.remaster_feature(F::RemasterIndoorLamps),
            outdoor_lamps: f.remaster_feature(F::RemasterOutdoorLamps),
            bloom: f.remaster_feature(F::RemasterBloom),
            smoothing: f.remaster_feature(F::RemasterSmoothing),
            water: f.remaster_feature(F::RemasterWater),
            puddles: f.remaster_feature(F::RemasterPuddles),
            rain: f.remaster_feature(F::RemasterRain),
            detail: f.remaster_feature(F::RemasterDetail),
            bounce: f.remaster_feature(F::RemasterBounce),
            traced_shadows: f.remaster_feature(F::RemasterTracedShadows),
            traced_occlusion: f.remaster_feature(F::RemasterTracedOcclusion),
            upscale: f.remaster_feature(F::RemasterUpscale),
            dlss: f.remaster_feature(F::RemasterDlss),
            scale: f.effective(F::RemasterScale),
            god_rays: f.effective(F::RemasterGodRays),
            packs: if f.remaster_feature(F::RemasterArtPack) {
                RemasterPacks::all()
            } else {
                RemasterPacks::default()
            },
        }
    }

    /// The upscaler the boxes ask for. Over Metal either box is MetalFX's: its one upscaling box is
    /// the page's, and a profile from elsewhere may have the other ticked.
    #[must_use]
    pub fn upscale_mode(self) -> RemasterUpscaleMode {
        if !(self.upscale || self.dlss) {
            RemasterUpscaleMode::Off
        } else if cfg!(target_os = "macos") {
            RemasterUpscaleMode::MetalFx
        } else if self.dlss {
            RemasterUpscaleMode::DlssSr
        } else {
            RemasterUpscaleMode::Fsr
        }
    }

    /// The share of the pixels drawn each way while upscaling, 0 to 1.
    #[must_use]
    pub fn drawn_share(self) -> f32 {
        let share = match self.scale {
            0 => dereth_client_contract::options::remaster::SCALE_DEFAULT,
            share => share.clamp(50, 100),
        };
        #[allow(clippy::cast_precision_loss)] // a share of at most a hundred
        let scale = share as f32 / 100.0;
        scale
    }

    /// Whether what they ask is worked out per pixel, which only the light per pixel draws.
    #[must_use]
    pub fn per_pixel(self) -> bool {
        self.shadows
            || self.sky
            || self.fog
            || self.occlusion
            || self.indoor_lamps
            || self.outdoor_lamps
            || self.bounce
            || self.traced_shadows
            || self.traced_occlusion
    }
}

impl SceneDraw {
    /// Carry out what moved of the remaster's own boxes since the last poll. The lamps and the
    /// levels are held only by the running remaster, so they are told again as it starts.
    pub(super) fn apply_remaster_rows(&mut self, now: RemasterRows, gpu: &mut Gpu) {
        let on = self.remaster_on();
        let view = self.remaster_view.get_mut();
        let was = view.rows;
        let started = on && !view.rows_on;
        view.rows = now;
        view.rows_on = on;
        if now.per_pixel() != was.per_pixel() {
            let light = if now.per_pixel() {
                RemasterLight::Pixel
            } else {
                RemasterLight::Vertex
            };
            self.set_remaster_light(light, gpu);
        }
        // Shadows and occlusion traced are still shadows and occlusion.
        let shadows = now.shadows || now.traced_shadows;
        if shadows != (was.shadows || was.traced_shadows) {
            let s = ShadowSettings {
                on: shadows,
                ..ShadowSettings::default()
            };
            self.set_remaster_shadows(s, gpu);
        }
        if (now.sky, now.fog, now.god_rays) != (was.sky, was.fog, was.god_rays) {
            let s = SkySettings {
                atmosphere: now.sky,
                fog: now.fog,
                far_keep: FAR_KEEP,
                ..SkySettings::default()
            }
            .with_god_rays(now.god_rays);
            self.set_remaster_sky(s, gpu);
        }
        let occlusion = now.occlusion || now.traced_occlusion;
        if occlusion != (was.occlusion || was.traced_occlusion) {
            let s = RemasterAo {
                on: occlusion,
                ..RemasterAo::default()
            };
            self.set_remaster_ao(s, gpu);
        }
        if (now.bloom, now.smoothing) != (was.bloom, was.smoothing) {
            let s = RemasterPost {
                hdr: now.bloom,
                taa: now.smoothing,
                ..RemasterPost::default()
            };
            self.set_remaster_post(s, gpu);
        }
        if (now.water, now.puddles) != (was.water, was.puddles) {
            let s = RemasterWater {
                on: now.water,
                puddles: now.puddles,
                ..RemasterWater::default()
            };
            self.set_remaster_water(s, gpu);
        }
        if now.rain != was.rain {
            self.set_remaster_weather(RemasterWeather { on: now.rain }, gpu);
        }
        let lamps = (now.indoor_lamps, now.outdoor_lamps);
        if lamps != (was.indoor_lamps, was.outdoor_lamps) || (started && lamps != (false, false)) {
            let l = RemasterLamps {
                indoor: now.indoor_lamps,
                outdoor: now.outdoor_lamps,
            };
            self.set_remaster_lamps(l, gpu);
        }
        if now.bounce != was.bounce {
            let s = RemasterGi {
                on: now.bounce,
                ..RemasterGi::default()
            };
            self.set_remaster_gi(s, gpu);
        }
        if now.traced_shadows != was.traced_shadows {
            let s = RemasterTracedShadows {
                on: now.traced_shadows,
                ..RemasterTracedShadows::default()
            };
            self.set_remaster_traced_shadows(s, gpu);
        }
        if now.traced_occlusion != was.traced_occlusion {
            let s = RemasterTracedAo {
                on: now.traced_occlusion,
                ..RemasterTracedAo::default()
            };
            self.set_remaster_traced_ao(s, gpu);
        }
        let upscaled = now.upscale || now.dlss;
        if (now.upscale, now.dlss) != (was.upscale, was.dlss)
            || (upscaled && now.scale != was.scale)
        {
            let s = RemasterUpscale {
                mode: now.upscale_mode(),
                scale: if upscaled { now.drawn_share() } else { 1.0 },
                ..RemasterUpscale::default()
            };
            self.set_remaster_upscale(s, gpu);
        }
        if now.packs != was.packs {
            self.set_remaster_packs(now.packs, gpu);
        }
        if now.detail != was.detail || (started && now.detail) {
            let lod = if now.detail {
                LodSettings::ON
            } else {
                LodSettings::OFF
            };
            self.set_remaster_lod(lod, gpu);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    #[test]
    fn rain_and_puddles_are_asked_for_apart_and_every_box_starts_off() {
        assert_eq!(
            RemasterRows::of(&FidelityPreferences::default()),
            RemasterRows::default()
        );
        let rain = FidelityPreferences::parse_switch("RemasterRain=1").expect("parses");
        assert_eq!(
            RemasterRows::of(&rain),
            RemasterRows {
                rain: true,
                ..RemasterRows::default()
            }
        );
    }

    /// The render scale draws what the player chose, and two thirds or so until they choose.
    #[test]
    fn the_render_scale_says_how_many_pixels_are_drawn_while_upscaling() {
        let rows = |spec: &str| {
            RemasterRows::of(&FidelityPreferences::parse_switch(spec).expect("parses"))
        };
        assert!((rows("RemasterUpscale=1").drawn_share() - 0.67).abs() < 1e-6);
        let half = rows("RemasterUpscale=1,RemasterScale=50");
        assert!((half.drawn_share() - 0.5).abs() < 1e-6);
        assert_eq!(rows("RemasterScale=100").drawn_share(), 1.0);
    }

    #[test]
    fn the_upscaling_boxes_ask_for_the_devices_upscaler() {
        let upscale = RemasterRows {
            upscale: true,
            ..RemasterRows::default()
        };
        let dlss = RemasterRows {
            dlss: true,
            ..RemasterRows::default()
        };
        assert_eq!(
            RemasterRows::default().upscale_mode(),
            RemasterUpscaleMode::Off
        );
        if cfg!(target_os = "macos") {
            assert_eq!(upscale.upscale_mode(), RemasterUpscaleMode::MetalFx);
            assert_eq!(dlss.upscale_mode(), RemasterUpscaleMode::MetalFx);
        } else {
            assert_eq!(upscale.upscale_mode(), RemasterUpscaleMode::Fsr);
            assert_eq!(dlss.upscale_mode(), RemasterUpscaleMode::DlssSr);
        }
    }

    #[test]
    fn only_the_boxes_worked_out_per_pixel_ask_for_the_light_per_pixel() {
        assert!(!RemasterRows::default().per_pixel());
        let drawn_either_way = RemasterRows {
            bloom: true,
            smoothing: true,
            water: true,
            puddles: true,
            rain: true,
            detail: true,
            ..RemasterRows::default()
        };
        assert!(!drawn_either_way.per_pixel());
        for ask in [
            "RemasterShadows",
            "RemasterSky",
            "RemasterFog",
            "RemasterOcclusion",
            "RemasterIndoorLamps",
            "RemasterOutdoorLamps",
            "RemasterBounce",
            "RemasterTracedShadows",
            "RemasterTracedOcclusion",
        ] {
            let f = FidelityPreferences::parse_switch(&format!("{ask}=1")).expect("parses");
            assert!(RemasterRows::of(&f).per_pixel(), "{ask}");
        }
    }
}
