//! The remaster's weather as the scene asks for it: the rain and the snow, kept while the remaster
//! is off, and what its last frame of it did.

pub use dereth_remaster::render::weather::{
    WeatherSettings as RemasterWeather, WeatherStats as RemasterWeatherStats,
};
use dereth_render::device::Gpu;

use super::remaster_view::remaster;
use super::SceneDraw;

impl SceneDraw {
    /// Whether the remaster's rain and snow fall, from its next frame on, and from when it is
    /// turned on.
    pub fn set_remaster_weather(&mut self, settings: RemasterWeather, gpu: &mut Gpu) {
        self.remaster_view.get_mut().weather = settings;
        if let Some(r) = remaster(gpu) {
            r.renderer().set_weather(settings);
        }
    }

    /// Whether the remaster's rain and snow fall.
    #[must_use]
    pub fn remaster_weather(&self) -> RemasterWeather {
        self.remaster_view.borrow().weather
    }

    /// What the remaster's weather counts; `None` while the remaster is off.
    #[must_use]
    pub fn remaster_weather_stats(&self, gpu: &mut Gpu) -> Option<RemasterWeatherStats> {
        Some(remaster(gpu)?.renderer().weather_stats())
    }
}
