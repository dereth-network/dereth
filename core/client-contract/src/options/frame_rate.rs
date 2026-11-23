//! The frame rate limits: the most frames a second the client draws while its window is the
//! active one ([`MAX_FRAME_RATE`]), and while it is not ([`MAX_BACKGROUND_FRAME_RATE`]).
//!
//! The original client drew as fast as it could while its window was the active one, and spaced
//! its frames at least 99 milliseconds apart, about ten a second, while it was not. Those are the
//! limits at first: Unlimited in the active window (vertical sync, when it is on, still holds the
//! rate to the display's) and 10 in the background. Each offers 10, 30, 60, 90, 120, 144, 240 and
//! Unlimited. They are this client's own options, not retail ones, and belong to no interface:
//! every interface's Client Options page shows them under Display, and they are saved in the
//! profile like any other. The frame pace reads them every frame, so a change applies at once.
//!
//! A window nobody can see, or a minimized one, is held to about sixty a second whatever its
//! limit says, so an Unlimited one never spins; a minimized window draws nothing, and its frames
//! keep the background limit. A browser paces a page itself, so there neither limit changes
//! anything.
//!
//! They replace an earlier option, `[Display] SlowDownInBackground`: a profile that has it and
//! not the background limit is read once into the background limit, on as 10 and off as
//! Unlimited ([`migrate`]), and the next save writes the limits.

use crate::view::PrefValue;

/// `[Display] MaxFrameRate`: the most frames a second while the window is the active one, `0`
/// for no limit.
pub const MAX_FRAME_RATE: &str = crate::options::names::DISPLAY_MAX_FRAME_RATE;

/// `[Display] MaxBackgroundFrameRate`: the most frames a second while the window is not the
/// active one, `0` for no limit.
pub const MAX_BACKGROUND_FRAME_RATE: &str =
    crate::options::names::DISPLAY_MAX_BACKGROUND_FRAME_RATE;

/// The earlier option the background limit replaces: on was the original's pace in the
/// background, off was no limit.
pub const SLOW_DOWN_IN_BACKGROUND: &str = crate::options::names::DISPLAY_SLOW_DOWN_IN_BACKGROUND;

/// The active window's caption, literal text: no string table has one. Frames a second, as
/// FPS, so that both captions fit beside their lists on every interface's page.
pub const CAPTION: &str = "Max FPS";

/// The background's caption, literal text.
pub const BACKGROUND_CAPTION: &str = "Max Background FPS";

/// What the active window's limit does, where a page explains it.
pub const NOTE: &str = "The most frames a second while the window is the active one. Vertical \
                        sync, when it is on, still holds the rate to the display's. A browser \
                        paces a page itself, so there this changes nothing.";

/// What the background limit does, where a page explains it.
pub const BACKGROUND_NOTE: &str = "The most frames a second while the window is not the active \
                                   one. The original drew about ten. A minimized window draws \
                                   none. A browser slows a background tab by itself, so there \
                                   this changes nothing.";

/// No limit, as the store holds it.
pub const UNLIMITED: u32 = 0;

/// The limits a page offers, in frames a second, the lowest first and [`UNLIMITED`] last.
pub const CHOICES: [u32; 8] = [10, 30, 60, 90, 120, 144, 240, UNLIMITED];

/// The active window's limit at first: none.
pub const DEFAULT: u32 = UNLIMITED;

/// The background limit at first: the original's pace.
pub const DEFAULT_BACKGROUND: u32 = 10;

/// How far apart frames come at ten a second, in microseconds: the original's 99 milliseconds,
/// not a tenth of a second.
pub const ORIGINAL_SPACING_US: u64 = 99_000;

/// How far apart frames come, at the least, in a window nobody can see, in microseconds: about
/// sixty a second.
pub const HIDDEN_SPACING_US: u64 = 17_000;

/// Register both limits in the option value store, at their defaults.
pub fn register() -> usize {
    [
        (MAX_FRAME_RATE, DEFAULT),
        (MAX_BACKGROUND_FRAME_RATE, DEFAULT_BACKGROUND),
    ]
    .into_iter()
    .map(|(name, fps)| {
        usize::from(super::store::register_preference(
            name,
            PrefValue::Int(i32::try_from(fps).unwrap_or(0)),
            super::store::DataType::UInt,
        ))
    })
    .sum()
}

/// A limit as the store holds it: one of [`CHOICES`], else `default`.
fn limit(name: &str, default: u32) -> u32 {
    match super::store::inq_value(name) {
        Some(PrefValue::Int(v)) => u32::try_from(v)
            .ok()
            .filter(|fps| CHOICES.contains(fps))
            .unwrap_or(default),
        _ => default,
    }
}

/// The active window's limit, in frames a second, [`UNLIMITED`] for none.
#[must_use]
pub fn foreground() -> u32 {
    limit(MAX_FRAME_RATE, DEFAULT)
}

/// The background limit, in frames a second, [`UNLIMITED`] for none.
#[must_use]
pub fn background() -> u32 {
    limit(MAX_BACKGROUND_FRAME_RATE, DEFAULT_BACKGROUND)
}

/// How far apart, at the least, the starts of two frames come under a limit of `fps`, in
/// microseconds: `0` for no limit, the original's 99 milliseconds at ten.
#[must_use]
pub const fn spacing_us(fps: u32) -> u64 {
    match fps {
        UNLIMITED => 0,
        10 => ORIGINAL_SPACING_US,
        _ => 1_000_000 / fps as u64,
    }
}

/// How far apart, at the least, the starts of two frames come: the active window's limit or the
/// background's, and in a window nobody can see never less than [`HIDDEN_SPACING_US`].
#[must_use]
pub fn frame_spacing_us(is_active_app: bool, window_hidden: bool) -> u64 {
    let fps = if is_active_app {
        foreground()
    } else {
        background()
    };
    let spacing = spacing_us(fps);
    if window_hidden {
        spacing.max(HIDDEN_SPACING_US)
    } else {
        spacing
    }
}

/// Read a profile's earlier `SlowDownInBackground` into the background limit, once: only when the
/// profile does not hold the background limit itself. On is the original's ten a second, off is
/// no limit. Returns whether it did.
pub fn migrate(ini: &crate::persist::preferences::UserPreferences) -> bool {
    let held = |name: &str| {
        ini.entries
            .iter()
            .find(|(k, _)| k.trim().eq_ignore_ascii_case(name))
            .map(|(_, v)| v.trim().to_owned())
    };
    if held(MAX_BACKGROUND_FRAME_RATE).is_some() {
        return false;
    }
    let Some(old) = held(SLOW_DOWN_IN_BACKGROUND) else {
        return false;
    };
    let Some(on) = super::store::parse_bool(&old) else {
        return false;
    };
    let fps = if on { DEFAULT_BACKGROUND } else { UNLIMITED };
    super::store::set_value(
        MAX_BACKGROUND_FRAME_RATE,
        PrefValue::Int(i32::try_from(fps).unwrap_or(0)),
    )
}

/// Whether `name` is one of the two limits.
#[must_use]
pub fn is_limit(name: &str) -> bool {
    name.eq_ignore_ascii_case(MAX_FRAME_RATE)
        || name.eq_ignore_ascii_case(MAX_BACKGROUND_FRAME_RATE)
}

/// The value a word stands for: a limit in frames a second, or `Unlimited`. `None` for another
/// name or another word, which leaves the option as it was.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !is_limit(name) {
        return None;
    }
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("Unlimited") {
        return Some(0);
    }
    let fps: u32 = raw.parse().ok()?;
    CHOICES
        .contains(&fps)
        .then(|| i32::try_from(fps).ok())
        .flatten()
}

/// The words for a limit.
fn words(fps: u32) -> String {
    if fps == UNLIMITED {
        "Unlimited".to_owned()
    } else {
        fps.to_string()
    }
}

/// The words for a value: the limit, or `Unlimited`. `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !is_limit(name) {
        return None;
    }
    match v {
        PrefValue::Int(i) => u32::try_from(*i).ok().map(words),
        _ => None,
    }
}

/// The limits a page offers ([`CHOICES`]), each as its words; `None` for any other name.
#[must_use]
pub fn choice_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !is_limit(name) {
        return None;
    }
    Some(
        CHOICES
            .iter()
            .map(|&fps| super::store::Choice {
                label: words(fps),
                value: i32::try_from(fps).unwrap_or(0),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::interface::Interface;
    use crate::options::{sheet, store};
    use crate::persist::preferences::UserPreferences;

    /// The `[Display]` section of a saved profile.
    fn display_section(text: &str) -> String {
        text.split("[Display]\r\n")
            .nth(1)
            .and_then(|s| s.split("\r\n[").next())
            .unwrap_or_else(|| panic!("no [Display] section: {text}"))
            .to_owned()
    }

    /// Both limits are on every interface's Client Options page under Display, one after the
    /// other after the Performance Panel, offering the eight limits in their words; at first the
    /// active window has none and the background ten, as the original, and a store that holds
    /// neither paces so too. A limit chosen is saved in the profile in its words and read back.
    ///
    /// Behaviour: frame.pace.the-active-window-and-the-background-each-have-a-frame-rate-limit
    #[test]
    fn both_limits_are_on_every_page_at_the_original_s_pace_and_a_saved_profile_keeps_a_choice() {
        store::clear();
        assert_eq!((foreground(), background()), (UNLIMITED, 10));
        store::init();
        assert_eq!(
            store::inq_value(MAX_FRAME_RATE),
            Some(PrefValue::Int(0)),
            "unlimited at first"
        );
        assert_eq!(
            store::inq_value(MAX_BACKGROUND_FRAME_RATE),
            Some(PrefValue::Int(10))
        );
        for face in [Interface::Modern, Interface::Classic, Interface::Horizon] {
            let page: Vec<&str> = sheet::rows_for(sheet::PageId::Client, face)
                .filter_map(sheet::Row::preference)
                .collect();
            let at = |p: &str| page.iter().position(|x| *x == p);
            let panel = at(crate::options::performance::PERFORMANCE_PANEL).expect("the panel");
            assert_eq!(at(MAX_FRAME_RATE), Some(panel + 1), "{face:?}");
            assert_eq!(at(MAX_BACKGROUND_FRAME_RATE), Some(panel + 2), "{face:?}");
            assert_eq!(at(SLOW_DOWN_IN_BACKGROUND), None, "{face:?}");
        }
        let words: Vec<String> = choice_rows(MAX_FRAME_RATE)
            .unwrap()
            .into_iter()
            .map(|c| c.label)
            .collect();
        assert_eq!(
            words,
            ["10", "30", "60", "90", "120", "144", "240", "Unlimited"]
        );

        assert!(store::set_value(MAX_FRAME_RATE, PrefValue::Int(144)));
        assert!(store::set_value(
            MAX_BACKGROUND_FRAME_RATE,
            PrefValue::Int(0)
        ));
        let text = store::save().to_text();
        let display = display_section(&text);
        assert!(display.contains("MaxFrameRate=144\r\n"), "{display}");
        assert!(
            display.contains("MaxBackgroundFrameRate=Unlimited\r\n"),
            "{display}"
        );
        assert!(!display.contains("SlowDownInBackground"), "{display}");

        store::init();
        let ini = UserPreferences::parse(&text).expect("the saved profile parses");
        store::load(&ini);
        assert_eq!((foreground(), background()), (144, UNLIMITED));
        store::clear();
    }

    /// Each limit spaces the starts of frames by its share of a second, ten by the original's 99
    /// milliseconds, and no limit not at all; a window nobody can see is spaced at least a
    /// sixtieth of a second whatever its limit, and the active window's limit is not the
    /// background's.
    ///
    /// Behaviour: frame.pace.the-active-window-and-the-background-each-have-a-frame-rate-limit
    #[test]
    fn each_limit_spaces_frames_by_its_share_of_a_second() {
        assert_eq!(
            CHOICES.map(spacing_us),
            [99_000, 33_333, 16_666, 11_111, 8_333, 6_944, 4_166, 0]
        );
        store::init();
        assert_eq!(frame_spacing_us(true, false), 0, "the active window: none");
        assert_eq!(
            frame_spacing_us(false, false),
            99_000,
            "the background: ten"
        );
        assert_eq!(
            frame_spacing_us(true, true),
            17_000,
            "nobody sees it: rests"
        );
        assert_eq!(frame_spacing_us(false, true), 99_000);
        assert!(store::set_value(MAX_FRAME_RATE, PrefValue::Int(240)));
        assert!(store::set_value(
            MAX_BACKGROUND_FRAME_RATE,
            PrefValue::Int(0)
        ));
        assert_eq!(frame_spacing_us(true, false), 4_166);
        assert_eq!(frame_spacing_us(false, false), 0);
        assert_eq!(frame_spacing_us(true, true), 17_000);
        assert_eq!(frame_spacing_us(false, true), 17_000, "never spins");
        store::clear();
    }

    /// A profile from before the limits that had Slow Down in the Background on is read as a
    /// background limit of ten, and one that had it off as none; the next save writes the limit,
    /// and a profile that already holds the limit keeps it whatever the earlier option says.
    ///
    /// Behaviour: frame.pace.the-active-window-and-the-background-each-have-a-frame-rate-limit
    #[test]
    fn slow_down_in_the_background_is_read_once_into_the_background_limit() {
        for (old, fps, words) in [("True", 10, "10"), ("False", UNLIMITED, "Unlimited")] {
            store::init();
            assert!(store::set_value(
                MAX_BACKGROUND_FRAME_RATE,
                PrefValue::Int(30)
            ));
            let ini =
                UserPreferences::parse(&format!("[Display]\r\nSlowDownInBackground={old}\r\n"))
                    .unwrap();
            store::load(&ini);
            assert_eq!(background(), fps, "{old}");
            assert_eq!(foreground(), UNLIMITED, "{old}");
            let mut saved = ini.clone();
            store::save_into(&mut saved);
            let display = display_section(&saved.to_text());
            assert!(
                display.contains(&format!("MaxBackgroundFrameRate={words}")),
                "{display}"
            );

            // The limit, once written, is the one read.
            store::init();
            let both = UserPreferences::parse(&format!(
                "[Display]\r\nSlowDownInBackground={old}\r\nMaxBackgroundFrameRate=60\r\n"
            ))
            .unwrap();
            store::load(&both);
            assert_eq!(background(), 60, "{old}");
        }
        store::clear();
    }
}
