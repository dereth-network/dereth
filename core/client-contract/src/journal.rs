//! The journal panel's identity and the path it builds.
//!
//! `GameView::journal_identity` returns [`JournalIdentity`], so the type is contract: it is the
//! three process globals the host hands the panel, not anything the panel draws. Its two inherent
//! methods build paths, so [`create_journal_path`] and the two stems come with it (an inherent
//! `impl` has to live with its type). The rest of `panels::journal` — the page model, the timers,
//! the captions, page saving — stays in `dereth-ui-screens`, which `pub use`s these four names so
//! `dereth_ui_screens::panels::journal::JournalIdentity` still resolves.

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
// `dereth_ui_screens::panels::journal`. `dereth_client::hud` formats the house-purchase countdown
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
