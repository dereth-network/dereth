//! The map panel's date-and-time formatter.
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
