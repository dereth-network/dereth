//! `@tod` carried out on the world's sky clock (CD-038).
//!
//! Setting a time moves the clock's local adjustment, the offset the sky adds to the server's
//! time, exactly as the `--time-of-day` switch does at start; resetting puts the offset back to
//! zero. The server's time itself is never touched, so its next time sync leaves the choice in
//! place, and nothing is sent.

use dereth_client_model::cmd::tod::{self, TimeOfDayCommand};

use crate::world_state::WorldState;

/// Carry out `command` on the world's sky clock at `now`, the time the frame loop advances the
/// clock with, and return the sentence that answers it.
pub fn apply(ws: &mut WorldState, command: TimeOfDayCommand, now: f64) -> String {
    match command {
        TimeOfDayCommand::Show => {}
        TimeOfDayCommand::Set(fraction) => ws.clock.set_time_of_day(now, fraction),
        TimeOfDayCommand::Reset => ws.clock.clear_time_adjustment(now),
    }
    if command != TimeOfDayCommand::Show {
        // The landscape's light is read from the sky on the light tick. Make both ticks due, as a
        // new world starts them, so the next frame lights the land at the new hour rather than up
        // to a light tick later.
        ws.next_tick = 0.0;
        ws.next_light_tick = 0.0;
    }
    let fraction = ws.clock.present_time_of_day;
    let name = ws.clock.time_of_day_name();
    match command {
        TimeOfDayCommand::Show => tod::describe(fraction, name, ws.clock.is_adjusted()),
        TimeOfDayCommand::Set(_) => tod::set_reply(fraction, name),
        TimeOfDayCommand::Reset => tod::reset_reply(fraction, name),
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

    /// The fraction the server's clock alone gives at `now`.
    fn servers_fraction(ws: &WorldState, now: f64) -> f32 {
        let mut clock = ws.clock.clone();
        clock.clear_time_adjustment(now);
        clock.present_time_of_day
    }

    /// Behaviour: rendering.sky.tod-moves-only-this-clients-clock-and-reset-returns-to-the-servers
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn tod_moves_the_sky_clock_and_reset_returns_it_to_the_servers_time() {
        let mut ws = retail_world();
        // A server's time of day, well into the calendar, as a time sync leaves the timer.
        let now = 1_234_567.0;
        ws.clock.use_time(now);
        let server = ws.clock.present_time_of_day;
        assert!(!ws.clock.is_adjusted());
        let shown = apply(&mut ws, TimeOfDayCommand::Show, now);
        assert!(shown.ends_with("from the server's clock."), "{shown}");
        assert!(shown.contains(&format!("{server:.3}")), "{shown}");

        // Pick a time far from the server's, so landing on it proves the clock moved.
        let want = (server + 0.5).rem_euclid(1.0);
        ws.next_light_tick = now + 15.0;
        let reply = apply(&mut ws, TimeOfDayCommand::Set(want), now);
        assert!(
            (ws.clock.present_time_of_day - want).abs() < 1e-3,
            "asked for {want}, the sky clock reads {}",
            ws.clock.present_time_of_day
        );
        assert!(ws.clock.is_adjusted());
        assert!(reply.contains("on this client only"), "{reply}");
        let name = ws
            .clock
            .time_of_day_name()
            .expect("the region names its times");
        assert!(reply.contains(name), "{reply}");
        assert!(
            ws.next_light_tick <= now,
            "the land is relit on the next frame, not a light tick later"
        );
        let shown = apply(&mut ws, TimeOfDayCommand::Show, now);
        assert!(shown.contains("set on this client"), "{shown}");

        // The choice runs on with the timer, and survives the server's clock moving underneath.
        let later = now + 762.0;
        ws.clock.use_time(later);
        assert!(
            (ws.clock.present_time_of_day - (want + 0.1).rem_euclid(1.0)).abs() < 1e-3,
            "a tenth of a day later the sky clock reads {}",
            ws.clock.present_time_of_day
        );

        // Reset is the server's clock again, at whatever time it has reached.
        let reply = apply(&mut ws, TimeOfDayCommand::Reset, later);
        assert!(!ws.clock.is_adjusted());
        let expected = servers_fraction(&ws, later);
        assert!(
            (ws.clock.present_time_of_day - expected).abs() < 1e-6,
            "reset reads {}, the server's clock {expected}",
            ws.clock.present_time_of_day
        );
        assert!(
            (expected - (server + 0.1).rem_euclid(1.0)).abs() < 1e-3,
            "the server's clock ran on a tenth of a day: {server} -> {expected}"
        );
        assert!(reply.starts_with("The time of day follows the server's clock again"));
    }

    /// Behaviour: rendering.sky.tod-moves-only-this-clients-clock-and-reset-returns-to-the-servers
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn every_named_time_lands_the_sky_clock_on_its_quarter_of_the_day() {
        let mut ws = retail_world();
        let now = 98_765.0;
        for (word, fraction, name) in [
            ("midnight", 0.0, "Darktide"),
            ("dawn", 0.25, "Dawnsong"),
            ("noon", 0.5, "Midsong"),
            ("dusk", 0.75, "Evensong"),
            ("night", 0.0, "Darktide"),
        ] {
            let command = tod::parse(&[word.to_owned()]).expect(word);
            apply(&mut ws, command, now);
            assert!(
                (ws.clock.present_time_of_day - fraction).abs() < 1e-3
                    || (ws.clock.present_time_of_day - 1.0).abs() < 1e-3,
                "{word}: {}",
                ws.clock.present_time_of_day
            );
            assert_eq!(ws.clock.time_of_day_name(), Some(name), "{word}");
        }
    }
}
