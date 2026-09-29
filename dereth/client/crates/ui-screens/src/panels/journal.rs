//! `JournalPanel` — the per-character notebook with a countdown timer.
//!
//! The recovered toolbar and panel behavior gives element type `0x10000048`; the instance is
//! `0x10000563`, the second of three sub-panels of the quest page `<QUES>` `0x10000559`
//! (`ContractsPanel` `0x100005D4` is the tab that opens by default, [`super::pagelist`]'s
//! `0x10000564` is the third). Like the House tab the three are pages of that element's own
//! `Panel` tab table, so the Journal tab is the *text* element `0x10000560`.
//!
//! # This half is genuinely client-local, and that is the point
//!
//! Unlike Contracts, **no message carries a journal page.** `JournalPanel` handles no server notice of any kind; its only inputs are its own
//! element messages, global message `3` (the per-frame tick) and global message `0x0B`. The store
//! is a single process-wide list of pages shared with `PageListPanel` through a process-wide
//! pointer to the journal panel, and the only thing that ever leaves the process is
//! writing a **text file**. So this module is UI plus a small model plus
//! arithmetic, and there is no protocol work in it at all.
//!
//! [`JournalPanel::pages`] is that list; the panel owns it here rather than a file-scope
//! global because `PageListPanel` reaches it through the journal panel anyway (going to a page
//! and deleting one are calls on the panel), so "the page list borrows the journal panel" is the
//! same shape one level down.
//!
//! # Persistence
//!
//! The page load and the page save read and write the journal path's
//! `"%s%s-%s-%s.txt"`. The path needs the character and server names, which reach this crate
//! through [`crate::view::GameView::journal_identity`]; and [`JournalPanel::load`] /
//! [`JournalPanel::on_visibility_changed`] are the whole of their retail bodies.
//!
//! **The journal path's four `%s` are, in order,** the directory, the stem, the **world name**
//! and the **character name**. Both call sites — the global-message handler and the
//! visibility-changed handler — pass the literal `"Journal"` as the stem,
//! so retail's file is `<dir>Journal-<World>-<Character>.txt`.
//!
//! **The tag table.** The load compares seven-byte prefixes against the bare tags; the forms
//! the save writes are the `%`-bearing ones:
//!
//! ```text
//!   "%s%s-%s-%s.txt"             "<NEWP>\n"
//!   "Problem loading journal: Your journal file does not create a new page!"
//!   "<TIME>"   "<TIME> %f\n\n"   <- and a blank line ends every page
//!   "<TIM?>"  "<TIM?> %s\n"
//!   "<LOCY>"  "<LOCY> %f\n"
//!   "<LOCX>"  "<LOCX> %f\n"
//!   "TRUE"    "FALSE"
//!   "<LOC?>"  "<LOC?> %s\n"
//!   "<MINU>"  "<MINU> %d\n"
//!   "<HOUR>"  "<HOUR> %d\n"
//!   "<DAYS>"  "<DAYS> %d\n"
//!   "\t"      "\n\r"      " \t\n\r"
//!   "<NOTE>"  "<NOTE> %s\n"
//!   "<TITL>"  "<TITL> %s\n"
//!   "<LABE>"  "<LABE> %s\n"
//!   "<PNUM>"      (read only -- see below)
//!   "r"       "w+"       "Journal"
//! ```
//!
//! **The file has thirteen tags, not twelve, and the thirteenth is write-only-by-omission.**
//! The save writes twelve tags. The load also accepts **`<PNUM>`**: it reads the next token as a `%d` and moves to the end
//! of the line with **no store at all**. A page's number comes only from its position in the
//! file, which the `<NEWP>` arm assigns (the page number is the page count after the new page is
//! added). So `<PNUM>` is accepted from a file some older build wrote and thrown away.
//!
//! `%f` is C's default six decimals and `%d` is signed; `<LOC?>`/`<TIM?>` are written as the
//! literal `TRUE` or `FALSE`, and anything that is not exactly `TRUE` reads back false. `<LABE>`, `<TITL>` and `<NOTE>` take the **rest of the
//! line** (`strtok(NULL, "\n\r")`), so they may contain spaces; a note's own newlines are stored
//! as tabs (`replace("\n", "\t")` on the way out, `replace("\t", "\n")` on the way
//! back in) because the format is one record per line.
//!
//! Reporting a load complaint appends its text to chat on channel `0x1A`, and its
//! one caller is the load's "the first token is not `<NEWP>`" arm. It is recorded on
//! [`JournalPanel::load_complaint`]; this crate cannot reach the scroll, so the host drains it.
//!
//! **One declared deviation.** Retail's `fgets` buffer is `0x834` bytes, so a line over 2099
//! characters is split and its tail ignored; nothing here truncates.
//!
//! The stem is retail's own. The client's settings directory is its own
//! ([`dereth_client_contract::journal::STEM`] says where), so retail's journal file is never in
//! the same directory and cannot be clobbered.
//!
//! # The strings and the arithmetic
//!
//! The strings the journal formats with, and where each is used:
//!
//! ```text
//!   "None"             timer_text, update_location
//!   "Ready"            timer_text
//!   "Start"            show_editable_timer
//!   "%.1f%s, %.1f%s"   update_location
//!   "N"  "S"  "E"  "W" update_location
//!   "~ %d ~"           update_page_number
//!   "Reset"            show_running_timer
//! ```
//!
//! and the client UI system's delta-time-to-string's five:
//! `"%ds "`, `"%dm "`, `"%dh "`, `"%dd "`, `"%dmo "` — every one with a **trailing space**, which
//! is why the function's last act is to drop the final character.
//!
//! Three facts in here are easy to get wrong:
//!
//! 1. **The timer text's subtraction.** The remaining time
//!    is `(int)(stamp - now)` and the three arms are `!running -> "None"`,
//!    `stamp <= 0 -> "None"`, `remaining <= 0 -> "Ready"`, else the delta-time string of `remaining`.
//! 2. **The editable-timer and running-timer displays are visibility pairs**: editable shows
//!    the six days/hours/minutes elements and hides the static timer text; running
//!    hides the six and shows the timer. The button's caption is `"Start"` and
//!    `"Reset"` respectively, and editable also writes the **empty string** into the timer label.
//! 3. **The location's argument order.** The format is `"%.1f%s, %.1f%s"` and the
//!    arguments are the stored y coordinate and its N/S letter first and the stored x coordinate
//!    and its E/W letter second — so the stored x is the **east/west** value and the stored y the
//!    north/south one, the opposite of what retail's field names suggest. That agrees with
//!    the player system's coordinate read, whose first output (stored as x by the location
//!    reset) is the landscape coordinate's **x**. The rendering
//!    is therefore the radar's, and [`location_text`] produces the same `42.2N, 33.8E` shape as
//!    [`crate::mapradar::radar::update_coordinates`] — with one deliberate difference recorded on
//!    that function.

use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::view::GameView;

/// `<QUES>` — the quest page of `PanelStack`'s stack, and the `Panel` owning the tabs.
pub const PAGE: ElementId = ElementId(0x1000_0559);
/// The `JournalPanel` sub-panel.
pub const PANEL: ElementId = ElementId(0x1000_0563);
/// The **Journal tab**, paired with [`PANEL`] by the shipped `0x2E` table.
pub const TAB: ElementId = ElementId(0x1000_0560);

/// The label edit box.
pub const LABEL_EDIT: ElementId = ElementId(0x1000_0569);
/// The title edit box.
pub const TITLE_EDIT: ElementId = ElementId(0x1000_056B);
/// The notes edit box.
pub const NOTES_EDIT: ElementId = ElementId(0x1000_056D);
/// The page-number text.
pub const PAGE_NUMBER_TEXT: ElementId = ElementId(0x1000_0570);
/// The location text.
pub const LOCATION_TEXT: ElementId = ElementId(0x1000_0573);
/// The days edit box.
pub const DAYS_EDIT: ElementId = ElementId(0x1000_0576);
/// The days label.
pub const DAYS_TEXT: ElementId = ElementId(0x1000_0577);
/// The hours edit box.
pub const HOURS_EDIT: ElementId = ElementId(0x1000_0578);
/// The hours label.
pub const HOURS_TEXT: ElementId = ElementId(0x1000_0579);
/// The minutes edit box.
pub const MINUTES_EDIT: ElementId = ElementId(0x1000_057A);
/// The minutes label.
pub const MINUTES_TEXT: ElementId = ElementId(0x1000_057B);
/// The timer text.
pub const TIMER_TEXT: ElementId = ElementId(0x1000_057C);
/// The start-timer button — the one control that is both a button and a label.
pub const START_TIMER_BUTTON: ElementId = ElementId(0x1000_057D);

/// Save, then previous page.
pub const PREV_PAGE_BUTTON: ElementId = ElementId(0x1000_0565);
/// Save, then next page.
pub const NEXT_PAGE_BUTTON: ElementId = ElementId(0x1000_0566);
/// Save, append a page, go to it.
pub const NEW_PAGE_BUTTON: ElementId = ElementId(0x1000_0567);
/// Save, then page 1.
pub const FIRST_PAGE_BUTTON: ElementId = ElementId(0x1000_056F);
/// Save, then the last page.
pub const LAST_PAGE_BUTTON: ElementId = ElementId(0x1000_0571);
/// [`JournalPanel::reset_location`], i.e. "stamp where I am standing onto this page".
pub const STAMP_LOCATION_BUTTON: ElementId = ElementId(0x1000_0574);

/// The client's "no timer" answer, and the location display's "no location" one.
/// Stored as a wide string.
pub const NONE: &str = "None";
/// The timer text's expired answer.
pub const READY: &str = "Ready";
/// The editable timer's button caption.
pub const START: &str = "Start";
/// The running timer's button caption.
pub const RESET: &str = "Reset";

/// The two journal-file stems and the path they build.
///
/// [`JournalIdentity`] is defined in [`dereth_client_contract::journal`], because
/// `GameView::journal_identity` returns it and the contract crate may not depend on this one;
/// its one inherent method builds a path, so [`create_journal_path`] and [`STEM`] live there too.
pub use dereth_client_contract::journal::{create_journal_path, JournalIdentity, STEM};

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

/// The client's delta-time-to-string.
///
/// Defined in [`dereth_client_contract::journal`], beside `JournalIdentity`,
/// because `dereth_client::hud` formats the house-purchase countdown with it.
pub use dereth_client_contract::journal::delta_time_to_string;

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
/// [`crate::mapradar::radar::format_coordinate`]**: this function's letter for an exactly-zero
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

/// One journal page, shared by `JournalPanel` and `PageListPanel`.
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
    /// The page number — **1-based**, and rewritten over the whole vector by
    /// [`JournalPanel::delete_page`].
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

/// `JournalPanel`.
#[derive(Debug, Default)]
pub struct JournalPanel {
    /// The `JournalPanel` element.
    pub panel: Option<ElemHandle>,
    label: Option<ElemHandle>,
    title: Option<ElemHandle>,
    notes: Option<ElemHandle>,
    page_number: Option<ElemHandle>,
    location: Option<ElemHandle>,
    days_edit: Option<ElemHandle>,
    days_text: Option<ElemHandle>,
    hours_edit: Option<ElemHandle>,
    hours_text: Option<ElemHandle>,
    minutes_edit: Option<ElemHandle>,
    minutes_text: Option<ElemHandle>,
    timer: Option<ElemHandle>,
    start_button: Option<ElemHandle>,

    /// The journal's pages. See the module header for why they live here.
    pub pages: Vec<JournalPage>,
    /// The page being edited, which is a **copy** of `pages[current_page - 1]` until
    /// [`Self::save_this_page`] writes it back.
    pub info: JournalPage,
    /// The current page, 1-based.
    pub current_page: u32,
    /// Whether the pages are loaded — every handler in the panel is a no-op until this is true.
    pub loaded: bool,
    /// When the running timer next redraws.
    time_next_update: f64,

    /// How many [`Self::goto_page`] calls actually moved, how many saves ran, how many timers
    /// were started. Denominators, so a test can tell "nothing happened" from "it happened and
    /// drew nothing".
    pub page_turns: u32,
    pub saves: u32,
    pub timer_starts: u32,

    // ---- persistence ---------------------------------------------------------------------------
    /// The file [`Self::on_visibility_changed`] writes, once the host has named one. `None` is a
    /// build with no preferences file, no world name or no character name — in which the journal
    /// is session-local and **nothing is written at all**, which is strictly safer than guessing
    /// at a path.
    pub file: Option<std::path::PathBuf>,
    /// The client's line, waiting for a host that can reach the scroll.
    /// [`Self::take_load_complaint`] drains it.
    pub load_complaint: Option<&'static str>,
    /// How many page loads read a file, and how many page saves wrote one.
    /// Denominators, so a test can tell "nothing happened" from "it happened and read nothing".
    pub page_loads: u32,
    pub page_saves: u32,
}

impl JournalPanel {
    /// The journal panel's post-init — thirteen child lookups, then
    /// [`Self::show_editable_timer`], then registration for global messages `3` and `0x0B`.
    ///
    /// The two registrations are [`Self::tick`] and [`Self::load`] here; the process-wide
    /// journal-panel pointer is the caller holding this struct.
    ///
    /// # `loaded = false`
    ///
    /// This runs once per screen build (`Hud::panels_bound_to`), and a screen build is a new
    /// `JournalPanel`: its constructor leaves the loaded flag clear, so the next
    /// `0x0B` loads the pages again for whoever just logged on. The page list is a **global**
    /// in the client and survives the panel, but the page load erases it
    /// before it parses, so the observable is the same: after a relog the boxes hold the file's
    /// first page and nothing of the previous session.
    ///
    /// Without this a relog would leave the loaded flag latched, [`Self::load`] would return on its
    /// `!first && !read` line, and the freshly built edit boxes would never be filled — the journal
    /// would come up blank and the next save would write the blank over the file.
    ///
    /// The pages are saved on the way out, not here: `App::frame` runs
    /// [`Self::on_visibility_changed`]`(false)` when a mode is queued out of gameplay, which is
    /// retail's showing the old framework false before destroying
    /// it. This function is the destruction, and by then the boxes are already gone.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        self.loaded = false;
        self.file = None;
        self.pages.clear();
        self.info = JournalPage::default();
        self.current_page = 0;
        self.panel = ui.get_child_recursive(root, PANEL);
        let Some(p) = self.panel else { return };
        let find = |ui: &UiSystem, id: ElementId| ui.get_child_recursive(p, id);
        self.start_button = find(ui, START_TIMER_BUTTON);
        self.label = find(ui, LABEL_EDIT);
        self.title = find(ui, TITLE_EDIT);
        self.notes = find(ui, NOTES_EDIT);
        self.days_edit = find(ui, DAYS_EDIT);
        self.hours_edit = find(ui, HOURS_EDIT);
        self.minutes_edit = find(ui, MINUTES_EDIT);
        self.location = find(ui, LOCATION_TEXT);
        self.timer = find(ui, TIMER_TEXT);
        self.days_text = find(ui, DAYS_TEXT);
        self.hours_text = find(ui, HOURS_TEXT);
        self.minutes_text = find(ui, MINUTES_TEXT);
        self.page_number = find(ui, PAGE_NUMBER_TEXT);
        self.show_editable_timer(ui);
    }

    /// Whether `post_init` found the `JournalPanel` element.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.panel.is_some()
    }

    /// Whether all thirteen of the post-init's children are in the shipped layout.
    #[must_use]
    pub fn fully_bound(&self) -> bool {
        self.panel.is_some()
            && self.label.is_some()
            && self.title.is_some()
            && self.notes.is_some()
            && self.page_number.is_some()
            && self.location.is_some()
            && self.days_edit.is_some()
            && self.days_text.is_some()
            && self.hours_edit.is_some()
            && self.hours_text.is_some()
            && self.minutes_edit.is_some()
            && self.minutes_text.is_some()
            && self.timer.is_some()
            && self.start_button.is_some()
    }

    // ---- reading and writing the elements -----------------------------------------------------

    fn set_text(ui: &mut UiSystem, h: Option<ElemHandle>, s: &str) {
        if let Some(t) = h.and_then(|h| ui.text_element_mut(h)) {
            t.set_text(s);
        }
    }

    /// The element's text, with the tags stripped the way every other reader in this crate
    /// takes it.
    fn get_text(ui: &mut UiSystem, h: Option<ElemHandle>) -> String {
        h.and_then(|h| ui.text_element_mut(h))
            .map_or_else(String::new, |t| t.glyphs.inq_text(false))
    }

    /// `wcstoul(text, NULL, 0)` — base-`0` C parsing of whatever is in one of the three timer edit
    /// boxes, which means **a leading `0x` is hexadecimal and anything unparseable is zero**, and
    /// `errno` is cleared first so an overflow is `ULONG_MAX` rather than an error.
    ///
    /// The timer start uses the result unchecked, so a box reading `"abc"` contributes
    /// nothing rather than refusing the start. Reproduced.
    fn wcstoul(s: &str) -> u32 {
        let t = s.trim();
        let (digits, radix) =
            if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
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

    // ---- the timer ----------------------------------------------------------------------------

    /// The three edit boxes and their three labels
    /// visible, the countdown label hidden and blanked, the button captioned `"Start"`.
    pub fn show_editable_timer(&mut self, ui: &mut UiSystem) {
        self.info.timer_running = false;
        for h in [
            self.days_edit,
            self.days_text,
            self.hours_edit,
            self.hours_text,
            self.minutes_edit,
            self.minutes_text,
        ]
        .into_iter()
        .flatten()
        {
            ui.set_visible(h, true);
        }
        if let Some(h) = self.timer {
            ui.set_visible(h, false);
        }
        Self::set_text(ui, self.timer, "");
        Self::set_text(ui, self.start_button, START);
    }

    /// The mirror image, captioned `"Reset"`, ending
    /// in [`Self::update_running_timer`].
    pub fn show_running_timer(&mut self, ui: &mut UiSystem) {
        self.info.timer_running = true;
        for h in [
            self.days_edit,
            self.days_text,
            self.hours_edit,
            self.hours_text,
            self.minutes_edit,
            self.minutes_text,
        ]
        .into_iter()
        .flatten()
        {
            ui.set_visible(h, false);
        }
        if let Some(h) = self.timer {
            ui.set_visible(h, true);
        }
        Self::set_text(ui, self.start_button, RESET);
        self.update_running_timer(ui);
    }

    /// The journal panel's running timer update — the timer text becomes [`timer_text`] of the
    /// edited page's running flag and stamp.
    pub fn update_running_timer(&mut self, ui: &mut UiSystem) {
        let s = timer_text(self.info.timer_running, self.info.timer_stamp, ui.now.0);
        Self::set_text(ui, self.timer, &s);
    }

    /// Zero the three counts and the stamp and empty
    /// the three edit boxes. [`Self::reset_timer`]'s failure path, and nothing else calls it.
    pub fn clear_timer_text(&mut self, ui: &mut UiSystem) {
        self.info.timer_stamp = 0.0;
        self.info.timer_running = false;
        self.info.days = 0;
        self.info.hours = 0;
        self.info.minutes = 0;
        Self::set_text(ui, self.days_edit, "");
        Self::set_text(ui, self.hours_edit, "");
        Self::set_text(ui, self.minutes_edit, "");
    }

    /// Read the three boxes and stamp
    /// `now + days*86400 + hours*3600 + minutes*60`.
    ///
    /// The three multipliers are the literals `0x15180`, `0xE10` and `0x3C`.
    /// Returns the client's own return value, which is **always 0** — its caller tests it and
    /// calls [`Self::clear_timer_text`] on anything else, so the arm exists and is unreachable. Kept because
    /// deleting it would silently change what a malformed box does.
    pub fn reset_timer(&mut self, ui: &mut UiSystem) -> i32 {
        self.info.days = Self::wcstoul(&Self::get_text(ui, self.days_edit));
        self.info.hours = Self::wcstoul(&Self::get_text(ui, self.hours_edit));
        self.info.minutes = Self::wcstoul(&Self::get_text(ui, self.minutes_edit));
        let seconds = f64::from(self.info.days) * 86_400.0
            + f64::from(self.info.hours) * 3_600.0
            + f64::from(self.info.minutes) * 60.0;
        self.info.timer_running = true;
        self.info.timer_stamp = ui.now.0 + seconds;
        self.timer_starts += 1;
        0
    }

    // ---- the page ------------------------------------------------------------------------------

    /// The journal panel's location update.
    pub fn update_location(&mut self, ui: &mut UiSystem) {
        let s = location_text(self.info.location_set, self.info.ns, self.info.ew);
        Self::set_text(ui, self.location, &s);
    }

    /// The journal panel's page number update.
    pub fn update_page_number(&mut self, ui: &mut UiSystem) {
        let s = page_number_text(self.current_page);
        Self::set_text(ui, self.page_number, &s);
    }

    /// Read the player's coordinates into the page, set the location flag to whether that
    /// succeeded, then redraw.
    ///
    /// The client zeroes both coordinates **before** the read, so a failed read
    /// leaves `(0, 0)` behind and not the previous stamp. Reproduced.
    pub fn reset_location(&mut self, ui: &mut UiSystem, view: &dyn GameView) {
        self.info.ns = 0.0;
        self.info.ew = 0.0;
        match view.player_coords() {
            Some((ns, ew)) => {
                self.info.ns = f64::from(ns);
                self.info.ew = f64::from(ew);
                self.info.location_set = true;
            }
            None => self.info.location_set = false,
        }
        self.update_location(ui);
    }

    /// Pull the three text boxes and the three timer
    /// boxes into [`Self::info`], set its page number to the current page, and write it back over
    /// `pages[current_page - 1]` **if the index is in range**.
    pub fn save_this_page(&mut self, ui: &mut UiSystem) {
        self.info.label = Self::get_text(ui, self.label);
        self.info.title = Self::get_text(ui, self.title);
        self.info.notes = Self::get_text(ui, self.notes);
        self.info.page_number = self.current_page;
        self.info.days = Self::wcstoul(&Self::get_text(ui, self.days_edit));
        self.info.hours = Self::wcstoul(&Self::get_text(ui, self.hours_edit));
        self.info.minutes = Self::wcstoul(&Self::get_text(ui, self.minutes_edit));
        if self.current_page >= 1 && (self.current_page as usize) <= self.pages.len() {
            self.pages[self.current_page as usize - 1] = self.info.clone();
        }
        self.saves += 1;
    }

    /// The tail [`Self::goto_page`], [`Self::clear_current_page`] and [`Self::update`] share: the
    /// three timer values into their boxes, the three redraws, and then whichever timer face the
    /// running flag selects.
    fn redraw(&mut self, ui: &mut UiSystem) {
        Self::set_text(ui, self.days_edit, &format!("{}", self.info.days));
        Self::set_text(ui, self.hours_edit, &format!("{}", self.info.hours));
        Self::set_text(ui, self.minutes_edit, &format!("{}", self.info.minutes));
        self.update_running_timer(ui);
        self.update_location(ui);
        self.update_page_number(ui);
        if self.info.timer_running {
            self.show_running_timer(ui);
        } else {
            self.show_editable_timer(ui);
        }
    }

    /// The journal panel's goto page.
    ///
    /// The guard is `page <= page count` **unsigned and with no lower bound**, so going to page 0
    /// indexes one before the first page. That is a real out-of-range read in the client and is
    /// the one place this file refuses rather than reproduces: nothing reaches it, because every
    /// call site passes `1`, the current page ± 1 behind an is-last-page / `!= 1` test, or a page
    /// number out of the vector.
    pub fn goto_page(&mut self, ui: &mut UiSystem, page: u32) {
        if !self.loaded {
            return;
        }
        if page == 0 || page as usize > self.pages.len() {
            return;
        }
        self.info = self.pages[page as usize - 1].clone();
        self.current_page = page;
        Self::set_text(ui, self.label, &self.info.label.clone());
        Self::set_text(ui, self.title, &self.info.title.clone());
        Self::set_text(ui, self.notes, &self.info.notes.clone());
        self.redraw(ui);
        self.page_turns += 1;
    }

    /// Clear both the stored page and
    /// [`Self::info`], then the same redraw. [`Self::delete_page`]'s one-page arm.
    pub fn clear_current_page(&mut self, ui: &mut UiSystem) {
        if !self.loaded {
            return;
        }
        if self.current_page >= 1 && (self.current_page as usize) <= self.pages.len() {
            self.pages[self.current_page as usize - 1] = JournalPage::default();
        }
        self.info = JournalPage::default();
        Self::set_text(ui, self.label, "");
        Self::set_text(ui, self.title, "");
        Self::set_text(ui, self.notes, "");
        self.redraw(ui);
    }

    /// Append a blank page numbered `size + 1`.
    pub fn new_page(&mut self) {
        if !self.loaded {
            return;
        }
        let n = u32::try_from(self.pages.len()).unwrap_or(u32::MAX) + 1;
        self.pages.push(JournalPage {
            page_number: n,
            ..JournalPage::default()
        });
    }

    /// The journal panel's is-last-page — the current page equals the page count.
    #[must_use]
    pub fn is_last_page(&self) -> bool {
        self.current_page as usize == self.pages.len()
    }

    /// With one page left it clears it in place; otherwise
    /// it erases, **renumbers every remaining page from 1**, and goes to page 1.
    pub fn delete_page(&mut self, ui: &mut UiSystem, page: u32) {
        if !self.loaded || page == 0 || page as usize > self.pages.len() {
            return;
        }
        if self.pages.len() < 2 {
            self.clear_current_page(ui);
            return;
        }
        self.pages.remove(page as usize - 1);
        for (i, p) in self.pages.iter_mut().enumerate() {
            p.page_number = u32::try_from(i).unwrap_or(0) + 1;
        }
        self.goto_page(ui, 1);
    }

    /// The three redraws and the timer face, with no page move.
    /// Raised by the panel's own `0x18` when it becomes visible.
    pub fn update(&mut self, ui: &mut UiSystem) {
        if !self.loaded {
            return;
        }
        self.redraw(ui);
    }

    /// The client's `0x0B` arm, whole, including the page load.
    ///
    /// On global message `0x0B`, if not yet loaded, retail sets the loaded flag, loads the pages
    /// with the stem `"Journal"`, then adds a new page if there are none or else copies page 1
    /// into the edited page and goes to page 1, and finally sets the current page to 1.
    ///
    /// **The one deviation, and it is about when rather than what.** Retail's `0x0B` is raised
    /// once, by the engine, at the moment the player object exists — so the journal path's
    /// player-name lookup always has a name. This build polls from
    /// `RemainingPanels::update`, and the first poll can land before the host knows the character.
    /// The loaded flag still latches on the first call, because every gesture in this panel is a
    /// no-op until it does; the **file** is read on the first call at which the host can name one,
    /// and only while the store is still the single blank page [`Self::new_page`] just made. So a journal
    /// cannot be overwritten by a late read, and a late read cannot be missed.
    pub fn load(&mut self, ui: &mut UiSystem, view: &dyn GameView) {
        // The journal path build. `None` until the host has all three globals.
        let identity = view.journal_identity();
        // **A different character is a different `JournalPanel`.** In the client the whole
        // framework is destroyed on the way back to character select, so the next session's panel
        // comes up not loaded and the page load erases the page list before it
        // parses. Here the panel outlives the mode switch, so the change of path is what stands
        // in for that destruction — without it a second log-on would show, and then overwrite,
        // the previous character's notebook.
        if let Some(id) = identity.as_ref() {
            if self.file.as_ref().is_some_and(|f| *f != id.client_path()) {
                self.loaded = false;
                self.file = None;
                self.pages.clear();
                self.info = JournalPage::default();
                self.current_page = 0;
            }
        }
        let first = !self.loaded;
        self.loaded = true;
        let mut read = false;
        if self.file.is_none() {
            if let Some(id) = identity {
                // One path, read and written, which is retail's own arrangement; see the module
                // header for why it cannot clobber a retail journal.
                let path = id.client_path();
                self.file = Some(path.clone());
                read = self.load_pages(&path);
            }
        }
        // The `0x0B` tail runs on the latching call and again on the call that actually read a
        // file, because that is the one that gives going to page 1 something to show.
        if !first && !read {
            return;
        }
        if self.pages.is_empty() {
            self.new_page();
        } else {
            self.info = self.pages[0].clone();
            self.goto_page(ui, 1);
        }
        self.current_page = 1;
        self.update_page_number(ui);
    }

    /// The file half. See [`parse_pages`] for the parser and
    /// the module header for the tags.
    ///
    /// A file that does not open is **not** an error and raises nothing: `fopen(path, "r")`
    /// returning `NULL` falls straight through to the epilogue. Only a file whose first token is
    /// not `<NEWP>` complains, and it complains through the chat scroll.
    ///
    /// The store is emptied before the parse,
    /// so a refused file leaves no pages — which is why this only runs while the store is still
    /// the blank page [`Self::load`] made.
    /// Returns whether the store was replaced.
    fn load_pages(&mut self, path: &std::path::Path) -> bool {
        let pristine = self.pages.iter().all(|p| {
            *p == JournalPage {
                page_number: p.page_number,
                ..JournalPage::default()
            }
        });
        if !pristine {
            return false;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            return false;
        };
        self.page_loads += 1;
        self.pages.clear();
        match parse_pages(&text) {
            Ok(pages) => self.pages = pages,
            // Report on channel `0x1A`, close the file, return — and the store stays empty.
            Err(()) => self.load_complaint = Some(LOAD_COMPLAINT),
        }
        true
    }

    /// The journal panel's save pages — the file half.
    ///
    /// `fopen(path, "w")` and, if that fails, `fopen(path, "w+")`; a second failure writes nothing
    /// and says nothing. Rust's `File::create` is the `"w"` of the pair and the retry adds
    /// nothing over it, so the two are one call here.
    fn save_pages(&mut self) {
        let Some(path) = self.file.clone() else {
            return;
        };
        if std::fs::write(&path, save_pages_text(&self.pages)).is_ok() {
            self.page_saves += 1;
        }
    }

    /// Take the pending scroll report. The host puts it in the scroll at channel
    /// [`LOAD_COMPLAINT_CHANNEL`]; this crate cannot reach the chat scroll.
    pub fn take_load_complaint(&mut self) -> Option<&'static str> {
        self.load_complaint.take()
    }

    /// The client's `3` arm: while the panel is **visible** and at most
    /// twice a second, redraw the countdown.
    ///
    /// The visibility test is the panel's own visible flag, which is what
    /// stops a journal page counting down in a panel nobody has open — and it is why the
    /// countdown only moves once the player has actually opened the tab.
    pub fn tick(&mut self, ui: &mut UiSystem) -> bool {
        let visible = self
            .panel
            .and_then(|h| ui.node(h))
            .is_some_and(|n| n.region.flags.visible);
        if !visible || ui.now.0 < self.time_next_update {
            return false;
        }
        self.update_running_timer(ui);
        self.time_next_update = ui.now.0 + TICK_PERIOD;
        true
    }

    /// The journal panel's visibility-changed handler, whole, including the page save.
    ///
    /// After the base handler, nothing happens unless loaded; then, when shown, the pages are
    /// sorted by page number, and on either edge this page is saved and the pages are written
    /// with the stem `"Journal"`.
    ///
    /// The sort runs only on the *shown* edge; the save-page / save-pages pair runs on both,
    /// which is what commits an edit when the player switches away to the page list — and it is
    /// the **only** thing in the client that ever writes the file. There is no save on exit and no
    /// timer: close the panel, and the journal is on disk.
    pub fn on_visibility_changed(&mut self, ui: &mut UiSystem, visible: bool) {
        if !self.loaded {
            return;
        }
        if visible {
            self.pages.sort_by_key(|p| p.page_number);
        }
        self.save_this_page(ui);
        self.save_pages();
    }

    /// The journal panel's element-message handler, whole.
    ///
    /// Returns true when an arm consumed the message. The `0x18` arm is first and is **not**
    /// exclusive with the `1` arm in the client either — it is a separate `if`, not an `else`.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        use dereth_ui::msg::element::id;
        if !self.loaded {
            return false;
        }
        if m.id == id::VISIBILITY_CHANGED && m.source_id == PANEL {
            // The message is the panel's own, and `dwParam1 != 0` means shown.
            if m.p1 != 0 {
                self.update(ui);
            }
            self.on_visibility_changed(ui, m.p1 != 0);
            return true;
        }
        if m.id != id::BUTTON_CLICKED {
            return false;
        }
        match m.source_id {
            PREV_PAGE_BUTTON => {
                self.save_this_page(ui);
                if self.current_page != 1 {
                    self.goto_page(ui, self.current_page - 1);
                }
            }
            NEXT_PAGE_BUTTON => {
                self.save_this_page(ui);
                if !self.is_last_page() {
                    self.goto_page(ui, self.current_page + 1);
                }
            }
            NEW_PAGE_BUTTON => {
                self.save_this_page(ui);
                self.new_page();
                let last = u32::try_from(self.pages.len()).unwrap_or(u32::MAX);
                self.goto_page(ui, last);
            }
            FIRST_PAGE_BUTTON => {
                self.save_this_page(ui);
                self.goto_page(ui, 1);
            }
            LAST_PAGE_BUTTON => {
                self.save_this_page(ui);
                let last = u32::try_from(self.pages.len()).unwrap_or(u32::MAX);
                self.goto_page(ui, last);
            }
            STAMP_LOCATION_BUTTON => self.reset_location(ui, view),
            START_TIMER_BUTTON => {
                if self.info.timer_running {
                    // The **Reset** face: drop the stamp and go back to the three boxes.
                    self.info.timer_stamp = 0.0;
                    self.info.timer_running = false;
                    self.show_editable_timer(ui);
                } else if self.reset_timer(ui) == 0 {
                    self.show_running_timer(ui);
                } else {
                    self.clear_timer_text(ui);
                }
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The five formats each have a trailing space; the unconditional seconds run ends by
    /// dropping the last character, which eats the last space.
    #[test]
    fn delta_time_to_string_matches_the_five_formats_and_drops_the_trailing_space() {
        assert_eq!(delta_time_to_string(0), "0s");
        assert_eq!(delta_time_to_string(50), "50s");
        assert_eq!(delta_time_to_string(60), "1m 0s");
        assert_eq!(delta_time_to_string(90), "1m 30s");
        assert_eq!(
            delta_time_to_string(3600),
            "1h 0s",
            "minutes is zero and is skipped"
        );
        assert_eq!(delta_time_to_string(3661), "1h 1m 1s");
        assert_eq!(delta_time_to_string(86_400), "1d 0s");
        assert_eq!(delta_time_to_string(2_592_000), "1mo 0s");
        assert_eq!(
            delta_time_to_string(2_592_000 + 86_400 + 3661),
            "1mo 1d 1h 1m 1s"
        );
        assert!(!delta_time_to_string(1).ends_with(' '));
    }

    /// The three formatting arms and their subtraction of the current time establish the timer
    /// behavior.
    #[test]
    fn the_timer_text_has_exactly_three_arms() {
        assert_eq!(timer_text(false, 1000.0, 0.0), NONE, "not running");
        assert_eq!(timer_text(true, 0.0, 0.0), NONE, "stamp <= 0 is also None");
        assert_eq!(timer_text(true, -5.0, 0.0), NONE);
        assert_eq!(timer_text(true, 100.0, 100.0), READY, "remaining <= 0");
        assert_eq!(timer_text(true, 99.0, 100.0), READY);
        assert_eq!(timer_text(true, 190.0, 100.0), "1m 30s");
    }

    /// Oracle: the client's argument order and its `== 0` arm. `ns` first, `ew`
    /// second; an exactly-zero coordinate uses an empty direction suffix.
    #[test]
    fn the_location_line_is_north_south_first_and_zero_has_no_letter() {
        assert_eq!(location_text(false, 42.2, 33.8), NONE);
        assert_eq!(location_text(true, 42.2, 33.8), "42.2N, 33.8E");
        assert_eq!(location_text(true, -42.2, -33.8), "42.2S, 33.8W");
        assert_eq!(location_text(true, 0.0, 0.0), "0.0, 0.0");
    }

    /// Oracle: the client's page-number label — a zero or negative current page prints 1.
    #[test]
    fn the_page_number_label_is_tilde_wrapped_and_never_prints_zero() {
        assert_eq!(page_number_text(1), "~ 1 ~");
        assert_eq!(page_number_text(12), "~ 12 ~");
        assert_eq!(page_number_text(0), "~ 1 ~");
    }

    /// Oracle: the client's `wcstoul(str, NULL, 0)` — base 0, no error checking.
    #[test]
    fn the_timer_boxes_are_parsed_the_way_wcstoul_base_zero_parses_them() {
        assert_eq!(JournalPanel::wcstoul("12"), 12);
        assert_eq!(JournalPanel::wcstoul(""), 0);
        assert_eq!(
            JournalPanel::wcstoul("abc"),
            0,
            "unparseable contributes nothing"
        );
        assert_eq!(
            JournalPanel::wcstoul("12abc"),
            12,
            "strtoul stops at the first non-digit"
        );
        assert_eq!(
            JournalPanel::wcstoul("0x10"),
            16,
            "base 0 means a 0x prefix is hex"
        );
    }

    /// Oracle: `crate::panels::catalogue` — the page is a stack page, the panel and the tab are
    /// not, and the thirteen bound ids are the catalogue's.
    #[test]
    fn the_thirteen_children_are_the_catalogued_ones() {
        assert!(crate::panels::catalogue::PANEL_PAGES.contains(&PAGE.0));
        assert!(!crate::panels::catalogue::PANEL_PAGES.contains(&PANEL.0));
        let spec = crate::panels::catalogue::spec("JournalPanel").expect("catalogued");
        let ids: Vec<u32> = spec.children.iter().map(|c| c.id.0).collect();
        for id in [
            LABEL_EDIT,
            TITLE_EDIT,
            NOTES_EDIT,
            PAGE_NUMBER_TEXT,
            LOCATION_TEXT,
            DAYS_EDIT,
            DAYS_TEXT,
            HOURS_EDIT,
            HOURS_TEXT,
            MINUTES_EDIT,
            MINUTES_TEXT,
            TIMER_TEXT,
            START_TIMER_BUTTON,
        ] {
            assert!(
                ids.contains(&id.0),
                "{id:?} is not in the JournalPanel binding table"
            );
        }
        assert_eq!(ids.len(), 13);
    }

    /// Oracle: the client's four path arguments, in `sprintf` order: directory, stem, world,
    /// character.
    #[test]
    fn the_journal_path_is_the_directory_the_stem_the_world_and_the_character() {
        let id = JournalIdentity {
            directory: std::path::PathBuf::from("C:/ac"),
            world: "Frostfell".into(),
            character: "Kupo".into(),
        };
        assert_eq!(
            id.client_path()
                .file_name()
                .and_then(std::ffi::OsStr::to_str),
            Some("Journal-Frostfell-Kupo.txt"),
            "the path's second part is the literal \"Journal\", which this client writes too"
        );
        assert_eq!(
            STEM, "Journal",
            "no prefix; the settings directory is the separation"
        );
    }

    /// Oracle: the twelve `fwrite`s of the page save and the `else` ladder of
    /// the page load. Everything a page carries goes out and comes back.
    #[test]
    fn every_field_of_a_page_survives_the_round_trip() {
        let pages = vec![
            JournalPage {
                label: "a label with spaces".into(),
                title: "Title".into(),
                notes: "one\ntwo\nthree".into(),
                page_number: 1,
                timer_stamp: 1234.5,
                days: 1,
                hours: 2,
                minutes: 3,
                ns: -42.25,
                ew: 33.5,
                timer_running: true,
                location_set: true,
            },
            JournalPage {
                page_number: 2,
                ..JournalPage::default()
            },
        ];
        let text = save_pages_text(&pages);
        assert!(
            !text.contains(tag::PAGE_NUMBER),
            "the save writes twelve records, not thirteen"
        );
        let back = parse_pages(&text).expect("the file opens with <NEWP>");
        assert_eq!(back, pages);
    }

    /// Oracle: the client -- `strtok` / `sscanf("%d")`, then on to the next line with **no store**.
    /// The page number comes from the `<NEWP>` arm's page count, so a file whose `<PNUM>` disagrees is
    /// read in file order and the tag is thrown away.
    #[test]
    fn pnum_is_accepted_and_discarded() {
        let text = "<NEWP>\n<PNUM> 97\n<TITL> first\n\n<NEWP>\n<PNUM> 4\n<TITL> second\n\n";
        let pages = parse_pages(text).expect("it opens with <NEWP>");
        assert_eq!(pages.len(), 2);
        assert_eq!(
            (pages[0].page_number, pages[1].page_number),
            (1, 2),
            "position, not <PNUM>"
        );
        assert_eq!(pages[1].title, "second");
    }

    /// Oracle: the page load's first arm -- the only refusal the function has, and the only caller of
    /// the in-scroll report. An unknown tag *inside* a file is the ladder's fall-through
    /// and is silently ignored.
    #[test]
    fn a_file_that_does_not_open_with_new_page_is_the_one_complaint() {
        assert!(parse_pages("<TITL> orphan\n").is_err());
        assert!(
            parse_pages("   \n\n<TITL> orphan\n").is_err(),
            "blank lines do not count"
        );
        let pages = parse_pages("<NEWP>\n<WHAT> ?\n<TITL> kept\n").expect("it opens with <NEWP>");
        assert_eq!(
            pages[0].title, "kept",
            "an unknown tag falls through the ladder"
        );
        assert_eq!(
            LOAD_COMPLAINT_CHANNEL, 0x1A,
            "the complaint goes to chat channel 0x1A"
        );
    }

    /// Oracle: `<LABE>`/`<TITL>`/`<NOTE>` are `strtok(NULL, "\n\r")` -- the rest of the line,
    /// spaces and all -- while `<LOC?>`/`<TIM?>` compare against the literal `TRUE` and everything
    /// else is false.
    #[test]
    fn the_line_rest_arms_keep_their_spaces_and_the_boolean_arms_only_accept_true() {
        let text = "<NEWP>\n<LABE>  two  spaces \n<LOC?> TRUE\n<TIM?> true\n";
        let p = &parse_pages(text).expect("opens")[0];
        assert_eq!(
            p.label, " two  spaces ",
            "only the one delimiter strtok overwrote is eaten"
        );
        assert!(p.location_set);
        assert!(
            !p.timer_running,
            "the comparison is with \"TRUE\" and it is case sensitive"
        );
    }
}
