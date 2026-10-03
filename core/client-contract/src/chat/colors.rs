//! The 34-entry chat colour table.
//!
//! The client's chat-colour table builder fills the log element's per-chat-type
//! colour table. It first fills **all 34 entries (0…33) with green** — a default fill over `0x22`
//! entries — and then overrides fourteen groups. The table reflects the retail colour values and
//! the order retail assigns them per chat type, so the *type numbers* are as load-bearing as the
//! RGB values.
//!
//! Keep the **34-entry** colour table and its exact defaults: the palette is what makes AC chat
//! recognisable, and several types share a colour deliberately.

/// One of the fourteen chat colours, with its float and packed representations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChatColor {
    /// The float triple retail uses. Alpha is 1.0 for every entry.
    pub rgb: (f32, f32, f32),
    /// The 8-bit form, `0xRRGGBB`.
    pub hex: u32,
}

const fn c(rgb: (f32, f32, f32), hex: u32) -> ChatColor {
    ChatColor { rgb, hex }
}

/// The fill colour: all 34 slots start here, and any type the overrides never name keeps it.
pub const GREEN: ChatColor = c((0.500, 1.000, 0.498), 0x80FF7F);
/// Chat type 2.
pub const WHITE: ChatColor = c((1.0, 1.0, 1.0), 0xFFFFFF);
/// Chat type 12 — also the colour of **every message prefix**.
pub const GREY: ChatColor = c((0.824, 0.824, 0.784), 0xD2D2C8);
/// Chat types 3, 10, 19 and 31.
pub const YELLOW: ChatColor = c((1.000, 1.000, 0.247), 0xFFFF3F);
/// Chat types 4 and 11.
pub const TAN: ChatColor = c((0.824, 0.824, 0.392), 0xD2D264);
/// Chat types 8 and 9.
pub const PINK: ChatColor = c((1.000, 0.588, 0.588), 0xFF9696);
/// Chat types 18 and 33.
pub const ORANGE: ChatColor = c((0.933, 0.573, 0.118), 0xEE921E);
/// Chat types 14, 27, 28, 29, 30 and 32.
pub const BLUE_GREY: ChatColor = c((0.706, 0.863, 0.941), 0xB4DCF0);
/// Chat types 6, 15 and 21.
pub const DARK_RED: ChatColor = c((1.000, 0.247, 0.247), 0xFF3F3F);
/// Chat type 22.
pub const LIGHT_RED: ChatColor = c((0.960, 0.459, 0.447), 0xF57572);
/// Chat types 7 and 17.
pub const LIGHT_BLUE: ChatColor = c((0.247, 0.749, 1.000), 0x3FBFFF);
/// Chat type 13.
pub const CYAN: ChatColor = c((0.247, 0.863, 0.863), 0x3FDCDC);
/// Chat type 5.
pub const BRIGHT_PURPLE: ChatColor = c((1.000, 0.498, 1.000), 0xFF7FFF);
/// Chat type `0x1A` (26), the over-head bubble channel.
pub const BRIGHT_RED: ChatColor = c((1.000, 0.000, 0.000), 0xFF0000);

/// The table is 34 entries wide — the default fill covers `0x22` entries.
pub const CHAT_COLOR_COUNT: usize = 34;

/// The timestamp colour index: the timestamp is always drawn with index 12,
/// the `Emote`/grey colour, regardless of the message's own type.
pub const PREFIX_COLOR_INDEX: u8 = 12;

/// The overrides [`build_chat_color_lookup_table`] applies after the green fill, in the order
/// the client applies them. Types not named here keep green.
const OVERRIDES: &[(ChatColor, &[u8])] = &[
    (WHITE, &[2]),
    (GREY, &[12]),
    (YELLOW, &[3, 10, 19, 31]),
    (TAN, &[4, 11]),
    (PINK, &[8, 9]),
    (ORANGE, &[18, 33]),
    (BLUE_GREY, &[14, 27, 28, 29, 30, 32]),
    (DARK_RED, &[6, 15, 21]),
    (LIGHT_RED, &[22]),
    (LIGHT_BLUE, &[7, 17]),
    (CYAN, &[13]),
    (BRIGHT_PURPLE, &[5]),
    (BRIGHT_RED, &[26]),
];

/// Build the 34-entry table.
#[must_use]
pub fn build_chat_color_lookup_table() -> [ChatColor; CHAT_COLOR_COUNT] {
    let mut t = [GREEN; CHAT_COLOR_COUNT];
    for (color, types) in OVERRIDES {
        for ty in *types {
            t[*ty as usize] = *color;
        }
    }
    t
}

/// The colour one chat type is drawn in. Out-of-range types fall back to green, which is what a
/// 34-entry table filled with green does for anything it never overrode.
#[must_use]
pub fn color_for_type(ty: u8) -> ChatColor {
    let t = build_chat_color_lookup_table();
    t.get(ty as usize).copied().unwrap_or(GREEN)
}

/// The palette for the active interface. World capabilities do not choose colours.
#[must_use]
pub fn interface_color(ty: u32, interface: crate::options::interface::Interface) -> ChatColor {
    use crate::options::interface::Interface;
    if interface == Interface::Classic {
        match ty {
            9 => return c((0.863, 0.627, 0.627), 0xDCA0A0),
            20 => return BRIGHT_PURPLE,
            19 | 26..=33 => return GREEN,
            _ => {}
        }
    }
    u8::try_from(ty).map_or(GREEN, color_for_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered chat behavior's table of retail colours per chat type. Every one of
    /// the 34 slots is asserted, type by type.
    /// Behaviour: chat.interface-colors
    #[test]
    fn every_one_of_the_thirty_four_slots_has_its_documented_colour() {
        let t = build_chat_color_lookup_table();
        assert_eq!(t.len(), 34, "the default fill covers 0x22 entries");
        let want: [(u8, u32); 34] = [
            (0, 0x80FF7F),  // Broadcast — green (default)
            (1, 0x80FF7F),  // AllChannels
            (2, 0xFFFFFF),  // Speech
            (3, 0xFFFF3F),  // Tell
            (4, 0xD2D264),  // OutgoingTell
            (5, 0xFF7FFF),  // System
            (6, 0xFF3F3F),  // Combat
            (7, 0x3FBFFF),  // Magic
            (8, 0xFF9696),  // Channel
            (9, 0xFF9696),  // ChannelSend
            (10, 0xFFFF3F), // Social
            (11, 0xD2D264), // SocialSend
            (12, 0xD2D2C8), // Emote
            (13, 0x3FDCDC), // Advancement
            (14, 0xB4DCF0), // Abuse
            (15, 0xFF3F3F), // Help
            (16, 0x80FF7F), // Appraisal
            (17, 0x3FBFFF), // Spellcasting
            (18, 0xEE921E), // Allegiance
            (19, 0xFFFF3F), // Fellowship
            (20, 0x80FF7F), // WorldBroadcast
            (21, 0xFF3F3F), // CombatEnemy
            (22, 0xF57572), // CombatSelf
            (23, 0x80FF7F), // Recall
            (24, 0x80FF7F), // Craft
            (25, 0x80FF7F), // Salvaging
            (26, 0xFF0000), // 0x1A — the over-head bubble channel
            (27, 0xB4DCF0),
            (28, 0xB4DCF0),
            (29, 0xB4DCF0),
            (30, 0xB4DCF0),
            (31, 0xFFFF3F), // AdminTell
            (32, 0xB4DCF0),
            (33, 0xEE921E),
        ];
        for (ty, hex) in want {
            assert_eq!(t[ty as usize].hex, hex, "chat type {ty}");
            assert_eq!(color_for_type(ty).hex, hex);
        }
    }

    /// Oracle: §3's opening sentence — the fill is green, so a type nobody overrode is green, and
    /// so is anything past the end of the table.
    /// Behaviour: chat.interface-colors
    #[test]
    fn green_is_the_default_and_the_out_of_range_answer() {
        assert_eq!(color_for_type(0), GREEN);
        assert_eq!(color_for_type(200).hex, GREEN.hex);
    }

    /// Oracle: §2 — the prefix is drawn with index 12 whatever the message's own type is.
    /// Behaviour: chat.interface-colors
    #[test]
    fn the_prefix_colour_index_is_twelve_and_that_slot_is_grey() {
        assert_eq!(PREFIX_COLOR_INDEX, 12);
        assert_eq!(color_for_type(PREFIX_COLOR_INDEX), GREY);
    }

    /// Oracle: §3's float column. The 8-bit column is the same colour rounded, and checking the two
    /// against each other is what catches a transcription slip in either.
    /// Behaviour: chat.interface-colors
    #[test]
    fn the_float_and_eight_bit_forms_of_every_colour_agree() {
        for c in [
            GREEN,
            WHITE,
            GREY,
            YELLOW,
            TAN,
            PINK,
            ORANGE,
            BLUE_GREY,
            DARK_RED,
            LIGHT_RED,
            LIGHT_BLUE,
            CYAN,
            BRIGHT_PURPLE,
            BRIGHT_RED,
        ] {
            let to8 = |v: f32| {
                u32::try_from(dereth_primitives::num::to_i32(v * 255.0 + 0.5)).unwrap_or(0)
            };
            let got = (to8(c.rgb.0) << 16) | (to8(c.rgb.1) << 8) | to8(c.rgb.2);
            // The document rounds each channel to three decimals before tabulating, so allow the
            // one-count slack that introduces.
            for shift in [16, 8, 0] {
                let a = (got >> shift) & 0xFF;
                let b = (c.hex >> shift) & 0xFF;
                assert!(a.abs_diff(b) <= 1, "{:#08X} vs {got:#08X}", c.hex);
            }
        }
    }
    /// Behaviour: chat.interface-colors
    #[test]
    fn classic_overrides_are_narrow_and_modern_keeps_the_full_table() {
        use crate::options::interface::Interface;
        for ty in 0..=255 {
            let modern = color_for_type(u8::try_from(ty).unwrap()).hex;
            assert_eq!(interface_color(ty, Interface::Retail).hex, modern);
            let classic = match ty {
                9 => 0xDCA0A0,
                20 => 0xFF7FFF,
                19 | 26..=33 => 0x80FF7F,
                _ => modern,
            };
            assert_eq!(interface_color(ty, Interface::Classic).hex, classic, "{ty}");
        }
        assert_eq!(interface_color(u32::MAX, Interface::Classic).hex, 0x80FF7F);
    }
}
