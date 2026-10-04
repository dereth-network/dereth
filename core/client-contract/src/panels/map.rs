//! World-profile map locations and the map panel's date-and-time formatter.
//!
//! `dereth_ui_screens::mapradar::map` re-exports it: the calendar clock the
//! world state owns (`dereth_client_runtime::game_clock`) formats through it, and the panel reads the
//! result.

/// The date buffer is 60 bytes; the time buffer is 30. Overflowing either yields a single
/// space.
pub const DATE_BUFFER_LEN: usize = 0x3C;
/// See [`DATE_BUFFER_LEN`].
pub const TIME_BUFFER_LEN: usize = 0x1E;

/// The Dereth date and the named time of day.
///
/// The day and year are each formatted with `"%d"`, then the date is assembled as:
///
/// ```text
///   "%s %s, %s %s"  <-  season_name, day, year, year_spec
/// ```
///
/// so the date is `"<season> <day>, <year> <year_spec>"` — with the shipped region that is
/// `"Morningthaw 14, 10 P.Y."`-shaped. The time is the named time of day verbatim.
///
/// Both guards are retail's and both yield a **single space**, not an empty string:
/// the client writes `' '` when the time-of-day name does not fit in 30 bytes, and does the
/// same for the date when the four pieces plus 9 reach 60.
#[must_use]
pub fn date_time_strings(
    season_name: &str,
    day: u32,
    year: u32,
    year_spec: &str,
    time_of_day_name: &str,
) -> (String, String) {
    // The client measures the name's length and takes the space arm at 30 or more. So the
    // test is on the length alone.
    let time = if time_of_day_name.len() < TIME_BUFFER_LEN {
        time_of_day_name.to_owned()
    } else {
        " ".to_owned()
    };
    let day = day.to_string();
    let year = year.to_string();
    // The four lengths plus nine, against the 60-byte buffer.
    let fits = season_name.len() + day.len() + year.len() + year_spec.len() + 9 < DATE_BUFFER_LEN;
    let date = if fits {
        format!("{season_name} {day}, {year} {year_spec}")
    } else {
        " ".to_owned()
    };
    (date, time)
}

/// A named rectangle in the map image's own pixel space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapNote {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub name: &'static str,
}
impl MapNote {
    /// The right and bottom edges are outside the rollover rectangle.
    #[must_use]
    pub const fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}
struct Region {
    note: MapNote,
    earlier_x: Option<i32>,
}
const fn n(x: i32, y: i32, w: i32, h: i32, name: &'static str) -> Region {
    Region {
        note: MapNote { x, y, w, h, name },
        earlier_x: Some(x),
    }
}
const fn later(x: i32, y: i32, w: i32, h: i32, name: &'static str) -> Region {
    Region {
        note: MapNote { x, y, w, h, name },
        earlier_x: None,
    }
}
const fn shifted(x: i32, earlier_x: i32, y: i32, w: i32, h: i32, name: &'static str) -> Region {
    Region {
        note: MapNote { x, y, w, h, name },
        earlier_x: Some(earlier_x),
    }
}

/// The connected world chooses content; a view without a world uses the default profile.
#[must_use]
pub fn profile(view: &dyn crate::GameView) -> dereth_primitives::EraId {
    view.era()
        .map_or(dereth_primitives::EraId::Eor, |era| era.era)
}

/// Locations in name order, independent of interface artwork.
pub fn notes(profile: dereth_primitives::EraId) -> impl DoubleEndedIterator<Item = MapNote> {
    REGIONS.iter().filter_map(move |region| {
        let mut note = region.note;
        if profile == dereth_primitives::EraId::Infiltration {
            note.x = region.earlier_x?;
        }
        Some(note)
    })
}

const REGIONS: [Region; 53] = [
    n(178, 20, 11, 12, "Aerlinthe Island"),
    n(18, 74, 5, 5, "Ahurenga"),
    n(141, 166, 7, 6, "Al-Arqas"),
    n(129, 121, 7, 6, "Al-Jalima"),
    n(190, 88, 9, 8, "Arwic"),
    n(19, 201, 7, 6, "Ayan Baqur"),
    n(200, 190, 7, 6, "Baishi"),
    n(184, 53, 5, 5, "Bandit Castle"),
    n(34, 84, 5, 5, "Bluespire"),
    n(44, 235, 5, 5, "Candeth Keep"),
    n(180, 97, 9, 8, "Cragstone"),
    n(91, 102, 5, 5, "Danby's Outpost"),
    n(211, 138, 9, 8, "Dryreach"),
    n(199, 106, 9, 8, "Eastham"),
    later(56, 13, 5, 5, "Fiun Outpost"),
    n(37, 127, 9, 8, "Fort Tethana"),
    n(156, 94, 9, 8, "Glenden Wood"),
    n(43, 79, 5, 5, "Greenspire"),
    n(224, 177, 7, 6, "Hebian-to"),
    n(164, 77, 9, 8, "Holtburg"),
    n(182, 230, 7, 6, "Kara"),
    n(155, 185, 7, 6, "Khayyaban"),
    n(224, 218, 7, 6, "Kryst"),
    n(212, 195, 7, 6, "Lin"),
    n(159, 224, 5, 5, "Linvak Tukal"),
    n(185, 126, 9, 8, "Lytelthorpe"),
    n(235, 220, 7, 6, "MacNiall's Freehold"),
    n(223, 203, 7, 6, "Mayoi"),
    n(141, 53, 5, 5, "Mt Esper-Crater Village"),
    n(224, 191, 7, 6, "Nanto"),
    n(142, 46, 5, 5, "Neydisa"),
    n(240, 128, 5, 5, "Oolutanga's Refuge"),
    n(74, 79, 5, 5, "Plateau Village"),
    n(148, 218, 7, 6, "Qalaba'r"),
    n(26, 83, 5, 5, "Redspire"),
    n(193, 114, 9, 8, "Rithwic"),
    n(146, 133, 7, 6, "Samsur"),
    later(50, 42, 5, 5, "Sanamar"),
    n(195, 163, 7, 6, "Sawato"),
    n(213, 171, 7, 6, "Shoushi"),
    later(41, 25, 5, 5, "Silyun"),
    n(6, 239, 15, 16, "Singularity Caul Island"),
    n(100, 48, 5, 5, "Stonehold"),
    n(32, 76, 5, 5, "Timaru"),
    n(239, 163, 7, 6, "Tou-Tou"),
    n(131, 148, 7, 6, "Tufa"),
    n(112, 244, 5, 5, "Ulgrim's Island"),
    n(159, 160, 7, 6, "Uziz"),
    n(63, 203, 7, 6, "Wai Jhou"),
    n(144, 181, 7, 6, "Xarabydun"),
    shifted(175, 179, 145, 7, 6, "Yanshi"),
    n(121, 156, 7, 6, "Yaraq"),
    n(123, 112, 7, 6, "Zaikhal"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::EraId;

    /// Behaviour: map.regions.follow-world-profile
    #[test]
    fn profiles_select_regions_and_yanshi_position() {
        let early: Vec<_> = notes(EraId::Infiltration).collect();
        let later: Vec<_> = notes(EraId::Eor).collect();
        assert_eq!((early.len(), later.len()), (50, 53));
        for (name, x, y) in [
            ("Fiun Outpost", 56, 13),
            ("Sanamar", 50, 42),
            ("Silyun", 41, 25),
        ] {
            assert!(!early.iter().any(|n| n.name == name));
            let n = later.iter().find(|n| n.name == name).unwrap();
            assert_eq!((n.x, n.y, n.w, n.h), (x, y, 5, 5));
            assert!(n.contains(x, y));
            assert!(!n.contains(x + 5, y));
            assert!(!n.contains(x, y + 5));
        }
        assert_eq!(early.iter().find(|n| n.name == "Yanshi").unwrap().x, 179);
        assert_eq!(later.iter().find(|n| n.name == "Yanshi").unwrap().x, 175);
        assert_eq!(profile(&crate::EmptyGameView), EraId::Eor);
    }
}
