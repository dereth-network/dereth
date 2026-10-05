//! Shared journal pages, editing actions, identity and host file requests.
//!
//! The model owns pages, drafts, timer stamps and save boundaries. Both interfaces project
//! [`JournalView`] and forward [`JournalAction`]; the host supplies [`JournalIdentity`] and
//! services tagged [`JournalIo`] requests. Pure captions and file parsing remain in presentation.

/// The stem of the journal file name, at both of the path builder's call sites. Retail's, and
/// also this client's.
///
/// A separate prefix is not needed to keep a retail journal from being clobbered. **That
/// separation is the directory's job**: this client keeps its files in its own settings directory
/// (`Dereth\client` under the roaming application data on Windows, ...) and retail keeps writing to
/// `Documents\Asheron's Call`, so the two never name the same file and a prefix would buy nothing
/// but an ugly name. A journal copied in by the first-run migration is read *and* written in
/// place, which is what carrying it over is for; the copy retail still owns is a different file
/// in a different directory and is not touched.
pub const STEM: &str = "Journal";

/// The three values the journal path needs and this crate cannot reach.
///
/// `directory` is the client's settings directory, `world` is the current world name, and
/// `character` is the local player's singular object name. All three are
/// process globals in the client; here the host hands them over through
/// [`crate::view::GameView::journal_identity`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalIdentity {
    pub directory: std::path::PathBuf,
    pub world: String,
    pub character: String,
}

impl JournalIdentity {
    /// The file this client reads and writes — [`STEM`] in [`Self::directory`].
    #[must_use]
    pub fn client_path(&self) -> std::path::PathBuf {
        create_journal_path(&self.directory, STEM, &self.world, &self.character)
    }
}

/// The journal file path: `sprintf("%s%s-%s-%s.txt", dir, stem, world, character)`, in that
/// argument order.
///
/// The directory is joined rather than concatenated, because the client's directory lookup returns a path with
/// its trailing separator and `Path::join` is that separator spelled portably.
#[must_use]
pub fn create_journal_path(
    directory: &std::path::Path,
    stem: &str,
    world: &str,
    character: &str,
) -> std::path::PathBuf {
    directory.join(format!("{stem}-{world}-{character}.txt"))
}

// The elapsed-time formatter and its four divisors, shared with
// `dereth_ui_screens::panels::journal`. `dereth_client_shell::hud` formats the house-purchase countdown
// with it, which is a number the world half owns; the function is arithmetic and `format!`.

/// One second in the client's timer units, and the four divisors the elapsed-time formatter uses:
/// `0x278D00`, `0x15180`, `0xE10`, `0x3C`.
const MONTH: u64 = 2_592_000;
const DAY: u64 = 86_400;
const HOUR: u64 = 3_600;
const MINUTE: u64 = 60;
/// The client's elapsed-time string.
///
/// `"%dmo "`, `"%dd "`, `"%dh "` and `"%dm "` are each emitted only when their term is non-zero;
/// `"%ds "` is **unconditional**; and the whole buffer then loses its last character, which is
/// always the trailing space. So one minute exactly is `"1m 0s"` and ninety seconds is
/// `"1m 30s"`, while fifty seconds is `"50s"`.
#[must_use]
pub fn delta_time_to_string(seconds: i64) -> String {
    let t = u64::try_from(seconds).unwrap_or(0);
    let (months, rest) = (t / MONTH, t % MONTH);
    let (days, rest) = (rest / DAY, rest % DAY);
    let (hours, rest) = (rest / HOUR, rest % HOUR);
    let (minutes, secs) = (rest / MINUTE, rest % MINUTE);
    let mut s = String::new();
    if months != 0 {
        s.push_str(&format!("{months}mo "));
    }
    if days != 0 {
        s.push_str(&format!("{days}d "));
    }
    if hours != 0 {
        s.push_str(&format!("{hours}h "));
    }
    if minutes != 0 {
        s.push_str(&format!("{minutes}m "));
    }
    s.push_str(&format!("{secs}s "));
    // `while (*p) p++; p[-1] = 0;` — the trailing space of whichever run was written last.
    s.pop();
    s
}

/// One journal page.
///
/// The two coordinates are named for what they hold rather than for retail's x/y; see
/// the location formatter.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JournalPage {
    /// The label.
    pub label: String,
    /// The title.
    pub title: String,
    /// The notes.
    pub notes: String,
    /// The page number — **1-based**, and rewritten over the whole vector when a page is deleted
    /// after deletion.
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

/// Shared notebook gestures. Drafts carry their identity generation and page.
#[derive(Debug, Clone, PartialEq)]
pub enum JournalAction {
    Edit {
        generation: u64,
        page: u32,
        draft: JournalPage,
    },
    SetField {
        generation: u64,
        page: u32,
        field: JournalField,
    },
    Goto(u32),
    Turn(i32),
    NewPage,
    Delete(u32),
    StampLocation,
    ToggleTimer,
    Visibility(bool),
    Sort(u8),
    Search(String),
    ResetSearch,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct JournalView {
    pub pages: Vec<JournalPage>,
    pub draft: JournalPage,
    pub current_page: u32,
    pub loaded: bool,
    pub revision: u64,
    pub generation: u64,
    pub sort: u8,
    pub reverse: bool,
    pub search: String,
    pub filtered: bool,
    pub page_loads: u32,
    pub page_saves: u32,
}

/// File work is performed by the host, outside the shared notebook.
#[derive(Debug, Clone, PartialEq)]
pub enum JournalIo {
    Load {
        identity: JournalIdentity,
        generation: u64,
        revision: u64,
    },
    Save {
        identity: JournalIdentity,
        generation: u64,
        pages: Vec<JournalPage>,
    },
}

/// One edited control; unrelated draft fields are left intact.
#[derive(Debug, Clone, PartialEq)]
pub enum JournalField {
    Label(String),
    Title(String),
    Notes(String),
    Days(u32),
    Hours(u32),
    Minutes(u32),
}
