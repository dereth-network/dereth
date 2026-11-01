//! The chat's part of the player's options, as a profile answers for it: each chat window's
//! filter (the kinds of text it shows) and the channel it is devoted to, each kind of text's font
//! size, font color and popup flag, each window's opacity.
//!
//! The profile answers each (see [`GameplayOptions`]'s chat getters): what its packed form
//! carried, or, for a table the packed form did not carry, what a new profile holds
//! ([`chat_defaults`]): the first window shows the errors, combat, standard messages, sayings,
//! tells, emotes and the broadcast, region and faction channels, the second the general channel,
//! the third the fellowship's, the fourth the allegiance's, each window devoted to the kind it was
//! made for (the sayings for the first); every kind in the small font, each in its own color; the
//! broadcasts, the fellowship's and the allegiance's lines show a hidden window. An opacity is kept
//! only once set.

pub use auberean_script::options::chat_defaults;
use auberean_script::options::GameplayOptions;

/// The kinds of text, a bit each (the chat windows' filters are their sums).
pub mod kind {
    pub const ERROR: u32 = 0x1;
    pub const COMBAT: u32 = 0x2;
    pub const ADMIN: u32 = 0x4;
    pub const STANDARD: u32 = 0x8;
    pub const SAY: u32 = 0x10;
    pub const TELL: u32 = 0x20;
    pub const EMOTE: u32 = 0x40;
    pub const LOG: u32 = 0x80;
    pub const BROADCAST: u32 = 0x100;
    pub const FELLOWSHIP: u32 = 0x1_0000;
    pub const ALLEGIANCE: u32 = 0x2_0000;
    pub const CHAT_ENTRY: u32 = 0x4_0000;
    pub const GENERAL_CHANNEL: u32 = 0x8_0000;
    pub const TRADE_CHANNEL: u32 = 0x10_0000;
    pub const REGION_CHANNEL: u32 = 0x20_0000;
    pub const FACTION_CHANNEL: u32 = 0x40_0000;
    pub const DEVOTED: u32 = 0x80_0000;
    pub const PK_CHANNEL: u32 = 0x100_0000;
}

/// The small font's number, every kind's in a new profile.
pub const SMALL_FONT: u32 = 0;

/// A chat window's filter (windows 1 to 4), 0 for none.
#[must_use]
pub fn filter(o: &GameplayOptions, window: u32) -> u32 {
    o.filter(window)
}

/// The channel a chat window is devoted to: the sayings' when none is named.
#[must_use]
pub fn devoted_channel(o: &GameplayOptions, window: u32) -> u32 {
    o.devoted_channel(window)
}

/// A kind of text's font size.
#[must_use]
pub fn font_size(o: &GameplayOptions, kind: u32) -> u32 {
    o.chat_font_size(kind)
}

/// A kind of text's font color.
#[must_use]
pub fn font_color(o: &GameplayOptions, kind: u32) -> u32 {
    o.chat_font_color(kind)
}

/// Whether a line of a kind of text shows a hidden chat window.
#[must_use]
pub fn popup(o: &GameplayOptions, kind: u32) -> bool {
    o.chat_popup_flag(kind) != 0
}

/// A window's opacity (by its name's token), once set.
// D2-11976
#[must_use]
pub fn window_opacity(o: &GameplayOptions, window: u32) -> Option<f32> {
    o.opacities
        .entries
        .iter()
        .find(|(k, _)| *k == window)
        .map(|(_, v)| f32::from_bits(*v))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (lookups over a profile's tables; the chat panel's data test reads them)
    use super::*;
    use auberean_script::options::Table;

    #[test]
    fn a_new_profile_answers_its_defaults_and_a_carried_table_replaces_them() {
        let mut o = GameplayOptions::default();
        let first = kind::ERROR
            | kind::COMBAT
            | kind::ADMIN
            | kind::STANDARD
            | kind::SAY
            | kind::TELL
            | kind::EMOTE
            | kind::BROADCAST
            | kind::REGION_CHANNEL
            | kind::FACTION_CHANNEL;
        assert_eq!(filter(&o, 1), first);
        assert_eq!(filter(&o, 5), 0);
        assert_eq!(devoted_channel(&o, 2), kind::GENERAL_CHANNEL);
        assert_eq!(devoted_channel(&o, 9), kind::SAY);
        assert_eq!(font_color(&o, kind::BROADCAST), 9);
        assert_eq!(font_size(&o, kind::TELL), SMALL_FONT);
        assert!(!popup(&o, kind::TELL));
        assert!(popup(&o, kind::FELLOWSHIP));
        assert_eq!(window_opacity(&o, 7), None);
        o.filters = Some(Table {
            header: 0,
            entries: vec![(2, kind::TELL)],
        });
        o.popup_flags.entries.push((kind::TELL, 1));
        o.opacities.entries.push((7, 0.5f32.to_bits()));
        assert_eq!(filter(&o, 1), 0, "a carried table holds only what it names");
        assert_eq!(filter(&o, 2), kind::TELL);
        assert!(popup(&o, kind::TELL));
        assert!(
            !popup(&o, kind::FELLOWSHIP),
            "a held popup table replaces the defaults"
        );
        assert_eq!(window_opacity(&o, 7), Some(0.5));
    }
}
