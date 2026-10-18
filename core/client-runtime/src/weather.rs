//! `@weather` carried out on the world (CD-039).
//!
//! The weather asked for is kept with the world. The sky reads it every frame to choose the kind
//! of day it is drawn as, and the Weather effect to choose whether that day's rain falls as rain
//! or as snow. The calendar is never touched, so `reset` returns at once to the day's own weather,
//! and nothing is sent.

use dereth_client_model::cmd::weather::{self, WeatherCommand};

/// The weather asked for, as the world keeps it and the scene reads it.
pub use dereth_client_model::cmd::weather::Weather;

use crate::world_state::WorldState;

/// Carry out `command` on the world, and return the sentence that answers it.
pub fn apply(ws: &mut WorldState, command: WeatherCommand) -> String {
    if let WeatherCommand::Set(asked) = command {
        ws.weather = asked;
        // The land's light and the fog are read from the sky on their ticks. Make both due,
        // as a new world starts them, so the next frame draws the land under the new sky
        // rather than up to a light tick later.
        ws.next_tick = 0.0;
        ws.next_light_tick = 0.0;
    }
    weather::describe(ws.weather, days_own(ws))
}

/// The day's own weather, as the answer names it: the kind of day the calendar gives today, rain
/// on a rainy day, cloudy on a cloudy one and clear on any other. A region with no days draws no
/// weather, which is clear.
#[must_use]
pub fn days_own(ws: &WorldState) -> &'static str {
    use dereth_assets::region::DayKind;
    match ws.clock.present_day_name() {
        Some(name) if DayKind::Rainy.names(name) => "rain",
        Some(name) if !DayKind::Clear.names(name) => "cloudy",
        _ => "clear",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn retail_region() -> dereth_assets::Region {
        let store = dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail region is this test's fixture and the dats are not under {} -- \
                 set DERETH_TEST_DAT_DIR",
                dereth_dat::testing::dat_dir().display()
            )
        });
        dereth_world_data::landblock::load_region(&store).expect("the retail region decodes")
    }

    fn retail_world() -> WorldState {
        WorldState::new(&retail_region(), &crate::scene::SceneConfig::default())
    }

    /// Behaviour: rendering.sky.weather-draws-the-kind-of-day-asked-for-and-auto-returns-to-the-days-own
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn weather_is_kept_with_the_world_and_the_land_is_relit_on_the_next_frame() {
        let mut ws = retail_world();
        let now = 1_234_567.0;
        ws.clock.use_time(now);
        let (year, day, hour) = (
            ws.clock.current_year,
            ws.clock.current_day,
            ws.clock.present_time_of_day,
        );
        assert_eq!(
            ws.weather,
            Weather::Auto,
            "a world starts with the day's own"
        );
        for asked in [Weather::Rain, Weather::Snow, Weather::Clear, Weather::Auto] {
            ws.next_tick = now + 0.8;
            ws.next_light_tick = now + 15.0;
            apply(&mut ws, WeatherCommand::Set(asked));
            assert_eq!(ws.weather, asked);
            assert!(
                ws.next_tick <= now && ws.next_light_tick <= now,
                "{asked:?}: the land is relit on the next frame, not a tick later"
            );
            // The calendar is the server's throughout: only the kind of day drawn changes.
            assert_eq!(
                (
                    ws.clock.current_year,
                    ws.clock.current_day,
                    ws.clock.present_time_of_day
                ),
                (year, day, hour),
                "{asked:?}"
            );
            assert!(!ws.clock.is_adjusted(), "{asked:?}");
        }
    }

    /// Behaviour: chat.commands.weather-is-answered-by-this-client-and-never-sent
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn the_answer_names_the_days_own_weather_and_a_weather_set_here_as_forced() {
        let region = retail_region();
        let mut ws = WorldState::new(&region, &crate::scene::SceneConfig::default());
        let day_length = f64::from(region.game_time.day_length);
        // A day at a time through a year of the calendar, until a rainy day, a cloudy one and a
        // clear one have all been seen.
        let mut seen = [false; 3];
        for d in 0..360 {
            ws.clock.use_time(1_000_000.0 + f64::from(d) * day_length);
            let name = ws
                .clock
                .present_day_name()
                .expect("the shipped region names its days")
                .to_owned();
            let lower = name.to_ascii_lowercase();
            let (want, kind) = if lower.contains("rain") {
                ("rain", 0)
            } else if lower.contains("cloud") {
                ("cloudy", 1)
            } else {
                ("clear", 2)
            };
            seen[kind] = true;
            for command in [WeatherCommand::Show, WeatherCommand::Set(Weather::Auto)] {
                assert_eq!(
                    apply(&mut ws, command),
                    format!("Current weather: {want}"),
                    "{name}"
                );
            }
            // Whatever the day, a weather set here is named as forced.
            for (asked, word) in [
                (Weather::Rain, "rain"),
                (Weather::Snow, "snow"),
                (Weather::Clear, "clear"),
            ] {
                assert_eq!(
                    apply(&mut ws, WeatherCommand::Set(asked)),
                    format!("Current weather: {word} (forced)"),
                    "{name}"
                );
                assert_eq!(
                    apply(&mut ws, WeatherCommand::Show),
                    format!("Current weather: {word} (forced)"),
                    "{name}"
                );
            }
            apply(&mut ws, WeatherCommand::Set(Weather::Auto));
            if seen.iter().all(|s| *s) {
                return;
            }
        }
        panic!("a year of days without rainy, cloudy and clear ones: {seen:?}");
    }
}
