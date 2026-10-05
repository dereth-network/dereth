//! Allegiance rules: the experience an oath costs on a world that charges for one, and the titles
//! of rank.
//!
//! Before Throne of Destiny an oath cost unassigned experience, and the clients of that time drew
//! the cost on the allegiance panel. A character who has never broken from a patron swears free.

/// The experience an oath costs a character whose next level is `level_span` experience away from
/// its current one (the curve's step from its level to the next), after `breaks` earlier breaks.
///
/// Five percent of the step, held between 100 and 5,000, then a quarter more for each break,
/// rounded half up.
#[must_use]
pub fn swear_xp_cost(level_span: u64, breaks: u32) -> u32 {
    #[allow(clippy::cast_precision_loss)] // a step of a few million is exact in an f64
    let base = (level_span as f64 * f64::from(0.05_f32)).clamp(100.0, 5000.0);
    u32::try_from(dereth_primitives::num::to_i64_f64(
        base * (1.0 + 0.25 * f64::from(breaks)) + 0.5,
    ))
    .unwrap_or(u32::MAX)
}

/// The cost of an oath on a world that charges for one: nothing for a character with no break,
/// otherwise [`swear_xp_cost`].
#[must_use]
pub fn swear_xp_cost_after_breaks(level_span: u64, breaks: u32) -> u32 {
    if breaks == 0 {
        0
    } else {
        swear_xp_cost(level_span, breaks)
    }
}

/// The heritage ids the title tables key on.
pub mod heritage {
    pub const ALUVIAN: u8 = 1;
    pub const GHARUNDIM: u8 = 2;
    pub const SHO: u8 = 3;
    pub const VIAMONTIAN: u8 = 4;
    /// Penumbraen.
    pub const SHADOWBOUND: u8 = 5;
    pub const GEARKNIGHT: u8 = 6;
    pub const TUMEROK: u8 = 7;
    pub const LUGIAN: u8 = 8;
    pub const EMPYREAN: u8 = 9;
    /// An alias for [`SHADOWBOUND`].
    pub const SHADOWBOUND_ALIAS: u8 = 10;
    pub const UNDEAD: u8 = 11;
}

const ALUVIAN_M: [&str; 10] = [
    "Yeoman",
    "Baronet",
    "Baron",
    "Reeve",
    "Thane",
    "Ealdor",
    "Duke",
    "Aetheling",
    "King",
    "High King",
];
const ALUVIAN_F: [&str; 10] = [
    "Yeoman",
    "Baronet",
    "Baroness",
    "Reeve",
    "Thane",
    "Ealdor",
    "Duchess",
    "Aetheling",
    "Queen",
    "High Queen",
];
const GHARUNDIM_M: [&str; 10] = [
    "Sayyid", "Shayk", "Maulan", "Mu'allim", "Naquib", "Qadi", "Mushir", "Amir", "Malik", "Sultan",
];
const GHARUNDIM_F: [&str; 10] = [
    "Sayyida",
    "Shayka",
    "Maulana",
    "Mu'allima",
    "Naquiba",
    "Qadiya",
    "Mushira",
    "Amira",
    "Malika",
    "Sultana",
];
const SHO_M: [&str; 10] = [
    "Jinin",
    "Jo-chueh",
    "Nan-chueh",
    "Shi-chueh",
    "Ta-chueh",
    "Kun-chueh",
    "Kou",
    "Taikou",
    "Ou",
    "Koutei",
];
const SHO_F: [&str; 10] = [
    "Jinin",
    "Jo-chueh",
    "Nan-chueh",
    "Shi-chueh",
    "Ta-chueh",
    "Kun-chueh",
    "Kou",
    "Taikou",
    "Jo-ou",
    "Koutei",
];
const VIAMONTIAN_M: [&str; 10] = [
    "Squire",
    "Banner",
    "Baron",
    "Viscount",
    "Count",
    "Marquis",
    "Duke",
    "Grand Duke",
    "King",
    "High King",
];
const VIAMONTIAN_F: [&str; 10] = [
    "Dame",
    "Banner",
    "Baroness",
    "Viscountess",
    "Countess",
    "Marquise",
    "Duchess",
    "Grand Duchess",
    "Queen",
    "High Queen",
];
const SHADOWBOUND_M: [&str; 10] = [
    "Tenebrous",
    "Shade",
    "Squire",
    "Knight",
    "Void Knight",
    "Void Lord",
    "Duke",
    "Archduke",
    "Highborn",
    "King",
];
const SHADOWBOUND_F: [&str; 10] = [
    "Tenebrous",
    "Shade",
    "Squire",
    "Knight",
    "Void Knight",
    "Void Lady",
    "Duchess",
    "Archduchess",
    "Highborn",
    "Queen",
];
const GEARKNIGHT_M: [&str; 10] = [
    "Tribunus",
    "Praefectus",
    "Optio",
    "Centurion",
    "Principes",
    "Legatus",
    "Consul",
    "Dux",
    "Secondus",
    "Primus",
];
const TUMEROK_M: [&str; 10] = [
    "Xutua", "Tuona", "Ona", "Nuona", "Turea", "Rea", "Nurea", "Kauh", "Sutah", "Tah",
];
const LUGIAN_M: [&str; 10] = [
    "Laigus", "Raigus", "Amploth", "Arintoth", "Obeloth", "Lithos", "Kantos", "Gigas", "Extas",
    "Tiatus",
];
const EMPYREAN_M: [&str; 10] = [
    "Ensign",
    "Corporal",
    "Lieutenant",
    "Commander",
    "Captain",
    "Commodore",
    "Admiral",
    "Warlord",
    "Ipharsin",
    "Aulin",
];
const EMPYREAN_F: [&str; 10] = [
    "Ensign",
    "Corporal",
    "Lieutenant",
    "Commander",
    "Captain",
    "Commodore",
    "Admiral",
    "Warlord",
    "Ipharsia",
    "Aulia",
];
const UNDEAD_M: [&str; 10] = [
    "Neophyte",
    "Acolyte",
    "Adept",
    "Esquire",
    "Squire",
    "Knight",
    "Count",
    "Viscount",
    "Highness",
    "Annointed",
];
const UNDEAD_F: [&str; 10] = [
    "Neophyte",
    "Acolyte",
    "Adept",
    "Esquire",
    "Squire",
    "Knight",
    "Countess",
    "Viscountess",
    "Highness",
    "Annointed",
];

/// The allegiance title lookup `(rank, heritage, gender)`.
///
/// Dispatches on gender (1 male, 2 female) then heritage. **Heritage 5 and 10 both map to
/// Shadowbound**, and heritages 6, 7 and 8 use the *male* table for both genders because no female
/// variants exist. Ranks are 1..=10; anything else fails and the caller falls back to the bare name.
#[must_use]
pub fn get_title(rank: u16, hg: u8, gender: u8) -> Option<&'static str> {
    if !matches!(gender, 1 | 2) {
        return None;
    }
    if !(1..=10).contains(&rank) {
        return None;
    }
    let female = gender == 2;
    let table: &[&str; 10] = match hg {
        heritage::ALUVIAN => {
            if female {
                &ALUVIAN_F
            } else {
                &ALUVIAN_M
            }
        }
        heritage::GHARUNDIM => {
            if female {
                &GHARUNDIM_F
            } else {
                &GHARUNDIM_M
            }
        }
        heritage::SHO => {
            if female {
                &SHO_F
            } else {
                &SHO_M
            }
        }
        heritage::VIAMONTIAN => {
            if female {
                &VIAMONTIAN_F
            } else {
                &VIAMONTIAN_M
            }
        }
        heritage::SHADOWBOUND | heritage::SHADOWBOUND_ALIAS => {
            if female {
                &SHADOWBOUND_F
            } else {
                &SHADOWBOUND_M
            }
        }
        // No female variants exist for these three.
        heritage::GEARKNIGHT => &GEARKNIGHT_M,
        heritage::TUMEROK => &TUMEROK_M,
        heritage::LUGIAN => &LUGIAN_M,
        heritage::EMPYREAN => {
            if female {
                &EMPYREAN_F
            } else {
                &EMPYREAN_M
            }
        }
        heritage::UNDEAD => {
            if female {
                &UNDEAD_F
            } else {
                &UNDEAD_M
            }
        }
        _ => return None,
    };
    Some(table[(rank - 1) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The step is held between 100 and 5,000 before the quarter per break is added, so a
    /// high-level character with breaks pays more than 5,000.
    #[test]
    fn the_cost_is_five_percent_of_the_level_step_held_to_100_and_5000_then_a_quarter_per_break() {
        assert_eq!(swear_xp_cost(0, 0), 100);
        assert_eq!(swear_xp_cost(20_000, 0), 1_000);
        assert_eq!(swear_xp_cost(20_000, 1), 1_250);
        assert_eq!(swear_xp_cost(2_010, 0), 101, "100.5 rounds up");
        assert_eq!(swear_xp_cost(1_000_000, 4), 10_000);
        // Level 126 on the February 2005 curve: the step past the curve's end is huge.
        assert_eq!(swear_xp_cost(8_358_197, 1), 6_250);
    }

    #[test]
    fn a_character_with_no_break_swears_free() {
        assert_eq!(swear_xp_cost_after_breaks(20_000, 0), 0);
        assert_eq!(swear_xp_cost_after_breaks(20_000, 2), 1_500);
    }
}
