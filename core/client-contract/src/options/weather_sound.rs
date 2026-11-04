//! Weather Sounds: the on/off and the volume of the rain, storm wind and thunder this client
//! plays in bad weather.
//!
//! This is this client's own option, not a retail one: the original had no weather sound. It is a
//! sound row of the Client page's Sound heading, after Ambient Sounds, on the Horizon interface's
//! page alone, the only interface the weather is drawn and heard under; in a build with the
//! high-fidelity presentation both preferences are registered in the option value store
//! ([`register`]) and saved in the profile's `[Sound]` section. `Sound.WeatherSound` reads
//! naturally, unlike retail's three `*Disabled` keys: `True` is on.

use crate::view::PrefValue;

/// `[Sound] WeatherSound`: whether the weather is heard. `True` is on.
pub const WEATHER_SOUND: &str = crate::options::names::WEATHER_SOUND;

/// `[Sound] WeatherSoundVolume`: how loud the weather is, 0.0 to 1.0.
pub const WEATHER_SOUND_VOLUME: &str = crate::options::names::WEATHER_SOUND_VOLUME;

/// The row's caption, literal text: no string table has one.
pub const CAPTION: &str = "Weather Sounds";

/// What the row does, for its tooltip.
pub const NOTE: &str = "The rain, wind and thunder of bad weather, which the original did not \
                        play.";

/// Register the two preferences in the option value store, on at full volume.
pub fn register() -> usize {
    usize::from(super::store::register_preference(
        WEATHER_SOUND,
        PrefValue::Bool(true),
        super::store::DataType::Bool,
    )) + usize::from(super::store::register_preference(
        WEATHER_SOUND_VOLUME,
        PrefValue::Float(1.0),
        super::store::DataType::Float,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::interface::Interface;
    use crate::options::sheet::{self, PageId, Value};
    use crate::persist::preferences::UserPreferences;

    /// The row follows Ambient Sounds under the Sound heading of Horizon's page alone, and is on
    /// at full volume by default.
    #[test]
    fn the_row_follows_ambient_sounds_on_horizons_page_alone() {
        let row = sheet::row_of_preference(PageId::Client, WEATHER_SOUND).expect("its row");
        assert_eq!(row.caption, CAPTION);
        assert_eq!(
            row.value,
            Value::Sound {
                on: WEATHER_SOUND,
                volume: WEATHER_SOUND_VOLUME,
            }
        );
        assert_eq!(
            row.default,
            Some(crate::options::config::PrefValueConst::Bool(true))
        );
        assert_eq!(row.volume_default, Some(1.0));
        for face in [Interface::Modern, Interface::Classic] {
            assert!(!sheet::rows_for(PageId::Client, face).any(|r| r.caption == CAPTION));
        }
        let sound: Vec<&str> = sheet::page(PageId::Client).headings[0]
            .rows
            .iter()
            .map(|r| r.caption)
            .collect();
        let ambient = sound.iter().position(|c| *c == "Ambient Sounds").unwrap();
        assert_eq!(sound[ambient + 1], CAPTION);
        assert_eq!(
            sheet::rows_for(PageId::Client, Interface::Horizon).any(|r| r.caption == CAPTION),
            cfg!(feature = "hifi")
        );
    }

    /// Registered, the two are on at full volume, saved under `[Sound]`, and a saved profile
    /// brings them back.
    #[test]
    fn the_preferences_are_saved_under_sound_and_read_back() {
        super::super::store::clear();
        assert_eq!(register(), 2);
        assert_eq!(
            super::super::store::inq_value(WEATHER_SOUND),
            Some(PrefValue::Bool(true))
        );
        assert_eq!(
            super::super::store::inq_value(WEATHER_SOUND_VOLUME),
            Some(PrefValue::Float(1.0))
        );
        assert!(super::super::store::set_value(
            WEATHER_SOUND,
            PrefValue::Bool(false)
        ));
        assert!(super::super::store::set_value(
            WEATHER_SOUND_VOLUME,
            PrefValue::Float(0.25)
        ));
        let text = super::super::store::save().to_text();
        let sound = text
            .split("[Sound]\r\n")
            .nth(1)
            .and_then(|s| s.split("\r\n[").next())
            .unwrap_or_else(|| panic!("no [Sound] section: {text}"));
        assert!(sound.contains("WeatherSound=False\r\n"), "{sound}");
        assert!(sound.contains("WeatherSoundVolume=0.25"), "{sound}");

        super::super::store::clear();
        register();
        let ini = UserPreferences::parse(&text).expect("the saved profile parses");
        super::super::store::load(&ini);
        assert_eq!(
            super::super::store::inq_value(WEATHER_SOUND),
            Some(PrefValue::Bool(false))
        );
        assert_eq!(
            super::super::store::inq_value(WEATHER_SOUND_VOLUME),
            Some(PrefValue::Float(0.25))
        );
        super::super::store::clear();
    }
}
