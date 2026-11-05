//! Divergence: V504
//! A landblock's surface over the threshold brews, is imminent, then moves players, and quiets down
//! once under it; with no threshold set nothing happens. Fixture: a fake clock's numbers alone.

use empyrean_common::master_configuration::PortalStormConfiguration;
use empyrean_world::world_objects::portal_storm::{Stage, Storm, StormStep};

const ON: PortalStormConfiguration = PortalStormConfiguration {
    threshold: 10,
    brew_seconds: 60,
    imminent_seconds: 60,
};

#[test]
fn a_crowd_brews_then_is_imminent_then_moves_a_player_every_ten_seconds_until_it_thins() {
    let mut s = Storm::default();
    assert_eq!(
        s.advance(10, 0.0, ON),
        StormStep::Nothing,
        "at the threshold"
    );
    assert_eq!(s.advance(11, 1.0, ON), StormStep::Warn);
    assert_eq!(s.stage, Stage::Brewing);
    assert_eq!(s.advance(11, 60.0, ON), StormStep::Warn);
    assert_eq!(s.stage, Stage::Brewing, "not yet");
    s.advance(11, 61.0, ON);
    assert_eq!(s.stage, Stage::Imminent);
    assert_eq!(s.advance(11, 100.0, ON), StormStep::Warn, "not yet");
    assert_eq!(s.advance(11, 121.0, ON), StormStep::Move);
    assert_eq!(
        s.advance(11, 125.0, ON),
        StormStep::Warn,
        "ten seconds apart"
    );
    assert_eq!(s.advance(11, 131.0, ON), StormStep::Move);
    assert_eq!(s.advance(10, 132.0, ON), StormStep::Subsided);
    assert_eq!(s.stage, Stage::Calm);
    assert_eq!(s.advance(10, 133.0, ON), StormStep::Nothing);
}

#[test]
fn with_no_threshold_there_are_no_storms_however_many_players() {
    let mut s = Storm::default();
    let off = PortalStormConfiguration::default();
    assert_eq!(off.threshold, 0, "off by default");
    assert_eq!(s.advance(5000, 1.0, off), StormStep::Nothing);
}
