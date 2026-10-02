//! The journal: a character's notebook of pages, each with a label, a title, notes, a stamped
//! location and a countdown timer, kept in a text file beside the client's settings and never sent
//! to a server.
//!
//! Here are the facts both interfaces show it with: the page, the file's text in both directions,
//! the timer, location and page-number captions, how a timer box is read and when a started timer
//! runs out. Reading and writing the file is the interface's, as is when it does so.
//!
//! The file is one record per line, `<NEWP>` opening each page and a blank line ending it:
//! `<LABE>`, `<TITL>` and `<NOTE>` take the rest of the line (a note's newlines are stored as
//! tabs), `<DAYS>`, `<HOUR>` and `<MINU>` a count, `<LOC?>` and `<TIM?>` the word `TRUE` or
//! `FALSE`, and `<LOCX>`, `<LOCY>` and `<TIME>` a number with six decimals. `<PNUM>` is read and
//! ignored: a page's number is its place in the file.

pub use dereth_client_contract::journal::{
    create_journal_path, delta_time_to_string, JournalIdentity, STEM,
};

/// The client's "no timer" answer, and the location display's "no location" one.
/// Stored as a wide string.
pub const NONE: &str = "None";
/// The timer text's expired answer.
pub const READY: &str = "Ready";
/// The editable timer's button caption.
pub const START: &str = "Start";
/// The running timer's button caption.
pub const RESET: &str = "Reset";

/// The load-error report's line, at channel [`LOAD_COMPLAINT_CHANNEL`].
pub const LOAD_COMPLAINT: &str =
    "Problem loading journal: Your journal file does not create a new page!";

/// The chat channel the complaint is appended on — `dereth_client_model::scroll::LOCAL_ERROR_TYPE`.
pub const LOAD_COMPLAINT_CHANNEL: u32 = 0x1A;

/// The twelve record tags, in the order their `fwrite` calls go out.
pub mod tag {
    /// The record separator and the only tag that may open the file.
    pub const NEW_PAGE: &str = "<NEWP>";
    /// Accepted by the load and **discarded**; never written. See the module
    /// header.
    pub const PAGE_NUMBER: &str = "<PNUM>";
    pub const LABEL: &str = "<LABE>";
    pub const TITLE: &str = "<TITL>";
    pub const NOTES: &str = "<NOTE>";
    pub const DAYS: &str = "<DAYS>";
    pub const HOURS: &str = "<HOUR>";
    pub const MINUTES: &str = "<MINU>";
    /// Whether a location is set.
    pub const LOCATION_SET: &str = "<LOC?>";
    /// The stored x — the **east/west** value. See [`super::location_text`].
    pub const LOCATION_X: &str = "<LOCX>";
    /// The stored y — the **north/south** value.
    pub const LOCATION_Y: &str = "<LOCY>";
    /// Whether the timer is running.
    pub const TIMER_RUNNING: &str = "<TIM?>";
    /// The timer stamp.
    pub const TIMER_STAMP: &str = "<TIME>";
}

/// The stored word used to write `<LOC?>` and `<TIM?>`, and the
/// **only** spelling the load reads back as true.
const TRUE_WORD: &str = "TRUE";
const FALSE_WORD: &str = "FALSE";

/// `strtok`'s delimiter set for a tag and for a number.
const WHITESPACE: [char; 4] = [' ', '\t', '\n', '\r'];

/// The client's body, minus the `fopen`/`fwrite`/`fclose`.
///
/// Twelve records per page in the order the twelve `fwrite`s go out, and `<TIME>`'s format carries
/// the **second** newline that separates one page from the next.
#[must_use]
pub fn save_pages_text(pages: &[JournalPage]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for p in pages {
        // `"<NEWP>\n"` is the one record with no `%s` in it.
        let _ = writeln!(out, "{}", tag::NEW_PAGE);
        let _ = writeln!(out, "{} {}", tag::LABEL, p.label);
        let _ = writeln!(out, "{} {}", tag::TITLE, p.title);
        // `replace("\n", "\t")`: the record is one line, so the note's own newlines
        // ride across as tabs and the load turns them back.
        let _ = writeln!(out, "{} {}", tag::NOTES, p.notes.replace('\n', "\t"));
        let _ = writeln!(out, "{} {}", tag::DAYS, p.days);
        let _ = writeln!(out, "{} {}", tag::HOURS, p.hours);
        let _ = writeln!(out, "{} {}", tag::MINUTES, p.minutes);
        let _ = writeln!(out, "{} {}", tag::LOCATION_SET, bool_word(p.location_set));
        let _ = writeln!(out, "{} {:.6}", tag::LOCATION_X, p.ew);
        let _ = writeln!(out, "{} {:.6}", tag::LOCATION_Y, p.ns);
        let _ = writeln!(out, "{} {}", tag::TIMER_RUNNING, bool_word(p.timer_running));
        // `"<TIME> %f\n\n"` — the blank line is part of the format.
        let _ = writeln!(out, "{} {:.6}\n", tag::TIMER_STAMP, p.timer_stamp);
    }
    out
}

const fn bool_word(v: bool) -> &'static str {
    if v {
        TRUE_WORD
    } else {
        FALSE_WORD
    }
}

/// The client's body, minus the `fopen`/`fgets`/`fclose`.
///
/// `Err(())` is the one refusal the function has: the first token that is not whitespace is not
/// `<NEWP>`, which reports [`LOAD_COMPLAINT`] in the chat scroll and abandons the file with
/// the store already emptied. Every other unrecognised tag is ignored, exactly as the `else`
/// ladder's fall-through is.
///
/// # Errors
/// `Err(())` when the file does not open with a `<NEWP>` record.
#[allow(clippy::result_unit_err)] // the one refusal carries nothing
pub fn parse_pages(text: &str) -> Result<Vec<JournalPage>, ()> {
    let mut pages: Vec<JournalPage> = Vec::new();
    for line in text.lines() {
        // `strtok(buf, " \t\n\r")` — a line of nothing but whitespace has no token and is skipped,
        // which is what makes the blank line after `<TIME>` harmless.
        let ws = &WHITESPACE[..];
        let head = line.trim_start_matches(ws);
        let tag_end = head.find(ws).unwrap_or(head.len());
        let (tag, after) = head.split_at(tag_end);
        if tag.is_empty() {
            continue;
        }
        // `strtok(NULL, "\n\r")` resumes at the character **after** the one delimiter the first
        // call overwrote, so only that single separator is consumed and any further space is part
        // of the value. (`\n` and `\r` are already gone: `str::lines` took them.)
        let line_rest = after.strip_prefix(ws).unwrap_or(after);
        // `strtok(NULL, " \t\n\r")` for the numeric and boolean arms, which does skip a run.
        let tail = after.trim_start_matches(ws);
        let word = &tail[..tail.find(ws).unwrap_or(tail.len())];

        if pages.is_empty() && tag != tag::NEW_PAGE {
            return Err(());
        }
        if tag == tag::NEW_PAGE {
            let n = u32::try_from(pages.len()).unwrap_or(u32::MAX) + 1;
            pages.push(JournalPage {
                page_number: n,
                ..JournalPage::default()
            });
            continue;
        }
        let Some(p) = pages.last_mut() else { continue };
        match tag {
            // Read, parsed with `%d`, and dropped on the floor. See the module header.
            tag::PAGE_NUMBER => {}
            tag::LABEL => p.label = line_rest.to_owned(),
            tag::TITLE => p.title = line_rest.to_owned(),
            tag::NOTES => p.notes = line_rest.replace('\t', "\n"),
            // `if (sscanf(tok, "%d", &v) == 1)` — a field that will not parse leaves the previous
            // value alone rather than zeroing it.
            tag::DAYS => {
                if let Ok(v) = word.parse() {
                    p.days = v;
                }
            }
            tag::HOURS => {
                if let Ok(v) = word.parse() {
                    p.hours = v;
                }
            }
            tag::MINUTES => {
                if let Ok(v) = word.parse() {
                    p.minutes = v;
                }
            }
            tag::LOCATION_SET => p.location_set = word == TRUE_WORD,
            tag::LOCATION_X => {
                if let Ok(v) = word.parse() {
                    p.ew = v;
                }
            }
            tag::LOCATION_Y => {
                if let Ok(v) = word.parse() {
                    p.ns = v;
                }
            }
            tag::TIMER_RUNNING => p.timer_running = word == TRUE_WORD,
            tag::TIMER_STAMP => {
                if let Ok(v) = word.parse() {
                    p.timer_stamp = v;
                }
            }
            _ => {}
        }
    }
    Ok(pages)
}

// One second in the clock's units, and the four divisors
// the client's delta-time-to-string uses.
//
// Defined in [`dereth_client_contract::journal`] with
// [`delta_time_to_string`], their only reader.

/// How often the running timer redraws — the client schedules the next update half a second
/// after the current time.
pub const TICK_PERIOD: f64 = 0.5;

/// See the module header for the strings and the three arms.
#[must_use]
pub fn timer_text(running: bool, stamp: f64, now: f64) -> String {
    if !running || stamp <= 0.0 {
        return NONE.to_owned();
    }
    let remaining = i64::from(dereth_primitives::num::to_i32_f64(stamp - now));
    if remaining > 0 {
        delta_time_to_string(remaining)
    } else {
        READY.to_owned()
    }
}

/// The journal panel's location update's string.
///
/// `ns` is the stored y and `ew` the stored x; see the module header for why the names are that
/// way round. **The zero arm is the one difference from
/// the radar's coordinates**: this function's letter for an exactly-zero
/// coordinate is the *empty string*, because the location update tests `< 0` and
/// then `== 0` separately, where the radar's takes the positive
/// letter for everything `>= 0`.
#[must_use]
pub fn location_text(location_set: bool, ns: f64, ew: f64) -> String {
    if !location_set {
        return NONE.to_owned();
    }
    let letter = |v: f64, pos: &str, neg: &str| -> &'static str {
        if v < 0.0 {
            if neg == "S" {
                "S"
            } else {
                "W"
            }
        } else if v > 0.0 {
            if pos == "N" {
                "N"
            } else {
                "E"
            }
        } else {
            ""
        }
    };
    format!(
        "{:.1}{}, {:.1}{}",
        ns.abs(),
        letter(ns, "N", "S"),
        ew.abs(),
        letter(ew, "E", "W")
    )
}

/// The journal panel's page number update — `"~ %d ~"` over the current page, or 1 when the
/// current page is zero or negative.
#[must_use]
pub fn page_number_text(current_page: u32) -> String {
    let n = if current_page == 0 { 1 } else { current_page };
    format!("~ {n} ~")
}

/// One journal page.
///
/// The two coordinates are named for what they hold rather than for retail's x/y; see
/// [`location_text`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JournalPage {
    /// The label.
    pub label: String,
    /// The title.
    pub title: String,
    /// The notes.
    pub notes: String,
    /// The page number — **1-based**, and rewritten over the whole vector when a page is deleted
    /// ([`renumber`]).
    pub page_number: u32,
    /// The timer stamp — an absolute clock time, not a duration.
    pub timer_stamp: f64,
    /// Days / hours / minutes, as `wcstoul` read them out of the three edit boxes.
    pub days: u32,
    pub hours: u32,
    pub minutes: u32,
    /// The stored y — the **north/south** value.
    pub ns: f64,
    /// The stored x — the **east/west** value.
    pub ew: f64,
    /// Whether the timer is running.
    pub timer_running: bool,
    /// Whether a location is set.
    pub location_set: bool,
}

/// A timer box's text as a count, the way C's `wcstoul(text, NULL, 0)` reads it: a leading `0x`
/// is hexadecimal, the digits stop at the first that is not one, anything unparseable is zero and
/// an overflow is the largest count.
///
/// A timer started from a box reading `abc` therefore adds nothing rather than being refused.
#[must_use]
pub fn parse_count(s: &str) -> u32 {
    let t = s.trim();
    let (digits, radix) = if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        (hex, 16)
    } else {
        (t, 10)
    };
    let taken: String = digits.chars().take_while(|c| c.is_digit(radix)).collect();
    if taken.is_empty() {
        return 0;
    }
    u32::from_str_radix(&taken, radix).unwrap_or(u32::MAX)
}

/// When a timer started at `now` for the given days, hours and minutes runs out: `now` plus
/// 86400, 3600 and 60 seconds a unit.
#[must_use]
pub fn timer_stamp(now: f64, days: u32, hours: u32, minutes: u32) -> f64 {
    now + f64::from(days) * 86_400.0 + f64::from(hours) * 3_600.0 + f64::from(minutes) * 60.0
}

/// Number the pages from 1 in their order, as a deletion leaves them.
pub fn renumber(pages: &mut [JournalPage]) {
    for (i, p) in pages.iter_mut().enumerate() {
        p.page_number = u32::try_from(i).unwrap_or(0) + 1;
    }
}

/// The page-list sort criterion. The values follow the sort's callers: page number 0, title 1, label 2, timer 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JournalSortCriteria {
    #[default]
    PageNumber,
    Title,
    Label,
    Timer,
}

/// The four page-list sorts — by page number, by title,
/// by contract name and by timer — plus their four reverse
/// twins, as one function.
///
/// * **page number** — ascending page number.
/// * **title** — `wcscmp` of the two titles **lower-cased**, so the comparison is
///   case-insensitive.
/// * **label** — retail reuses the contract-name sort here; it reads the first field, which is
///   the contract's name in one record and the page's label in the other. Same lower-cased
///   `wcscmp`.
/// * **timer** — a running timer sorts before a stopped one; two running ones by timer stamp
///   ascending; two stopped ones by page number. (Retail's three arms.)
#[must_use]
pub fn compare(a: &JournalPage, b: &JournalPage, by: JournalSortCriteria) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match by {
        JournalSortCriteria::PageNumber => a.page_number.cmp(&b.page_number),
        JournalSortCriteria::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
        JournalSortCriteria::Label => a.label.to_lowercase().cmp(&b.label.to_lowercase()),
        JournalSortCriteria::Timer => match (a.timer_running, b.timer_running) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (true, true) => a
                .timer_stamp
                .partial_cmp(&b.timer_stamp)
                .unwrap_or(Ordering::Equal),
            (false, false) => a.page_number.cmp(&b.page_number),
        },
    }
}

/// The page list panel's page contains string.
///
/// **An empty needle matches everything** — retail returns true first when the needle's length
/// (which counts the terminator) is 1. Otherwise the needle and each of the label, title and notes
/// are lower-cased and `wcsstr`'d, in that order.
#[must_use]
pub fn page_contains_string(page: &JournalPage, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let n = needle.to_lowercase();
    page.label.to_lowercase().contains(&n)
        || page.title.to_lowercase().contains(&n)
        || page.notes.to_lowercase().contains(&n)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the shared rules' own arithmetic; the journal's behaviour is tested where
    //! each interface shows it).
    use super::*;

    #[test]
    fn a_timer_box_is_read_as_c_reads_it() {
        assert_eq!(parse_count("12"), 12);
        assert_eq!(parse_count(" 0x1f "), 31);
        assert_eq!(parse_count("7days"), 7);
        assert_eq!(parse_count("abc"), 0);
        assert_eq!(parse_count("99999999999"), u32::MAX);
    }

    #[test]
    fn a_timer_runs_out_after_its_days_hours_and_minutes() {
        assert_eq!(
            timer_stamp(100.0, 1, 2, 3),
            100.0 + 86_400.0 + 7_200.0 + 180.0
        );
    }

    #[test]
    fn deleting_a_page_numbers_the_rest_from_one() {
        let mut pages = vec![
            JournalPage {
                page_number: 2,
                ..JournalPage::default()
            },
            JournalPage {
                page_number: 5,
                ..JournalPage::default()
            },
        ];
        renumber(&mut pages);
        assert_eq!(
            pages.iter().map(|p| p.page_number).collect::<Vec<_>>(),
            [1, 2]
        );
    }
}
