//! `@weather` carried out on the world (CD-039).
//!
//! The weather asked for is kept with the world. The sky reads it every frame to choose the kind
//! of day it is drawn as, and the Weather effect to choose whether that day's rain falls as rain
//! or as snow. The calendar is never touched, so `auto` returns at once to the day's own weather,
//! and nothing is sent.

use dereth_client_model::cmd::weather::{self, WeatherCommand};

/// The weather asked for, as the world keeps it and the scene reads it.
pub use dereth_client_model::cmd::weather::Weather;

use crate::world_state::WorldState;

/// Carry out `command` on the world, and return the sentence that answers it. `weather_off` says
/// the player's options leave the falling weather out, which the answer then says.
pub fn apply(ws: &mut WorldState, command: WeatherCommand, weather_off: bool) -> String {
    match command {
        WeatherCommand::Show => weather::describe(ws.weather, weather_off),
        WeatherCommand::Set(asked) => {
            ws.weather = asked;
            // The land's light and the fog are read from the sky on their ticks. Make both due,
            // as a new world starts them, so the next frame draws the land under the new sky
            // rather than up to a light tick later.
            ws.next_tick = 0.0;
            ws.next_light_tick = 0.0;
            weather::set_reply(asked, weather_off)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn retail_world() -> WorldState {
        let store = dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail region is this test's fixture and the dats are not under {} -- \
                 set DERETH_TEST_DAT_DIR",
                dereth_dat::testing::dat_dir().display()
            )
        });
        let region =
            dereth_world_data::landblock::load_region(&store).expect("the retail region decodes");
        WorldState::new(&region, &crate::scene::SceneConfig::default())
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
        let shown = apply(&mut ws, WeatherCommand::Show, false);
        assert_eq!(shown, "The weather is the day's own.");
        for asked in [Weather::Rain, Weather::Snow, Weather::Clear, Weather::Auto] {
            ws.next_tick = now + 0.8;
            ws.next_light_tick = now + 15.0;
            let reply = apply(&mut ws, WeatherCommand::Set(asked), false);
            assert_eq!(ws.weather, asked);
            assert_eq!(reply, weather::set_reply(asked, false));
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
        // A bare @weather says what is asked, and the options' say over the falling weather.
        apply(&mut ws, WeatherCommand::Set(Weather::Snow), false);
        let shown = apply(&mut ws, WeatherCommand::Show, true);
        assert!(
            shown.starts_with("The weather is snow, set on this client."),
            "{shown}"
        );
        assert!(shown.ends_with(weather::WEATHER_OFF), "{shown}");
    }
}
