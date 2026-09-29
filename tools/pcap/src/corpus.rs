//! The index for tests: iterate sessions and messages without writing SQL.
//!
//! ```no_run
//! // In a test:
//! let Some(corpus) = dereth_pcap::corpus::open_or_skip("my_test") else { return };
//! for s in corpus.sessions("partial = 0").unwrap() {
//!     corpus.for_each_message(s.session_id, "dir = 's2c'", |m| {
//!         assert!(m.raw.len() >= 4);
//!     }).unwrap();
//! }
//! ```
//!
//! **Unset is a counted skip, never a silent pass.** With `DERETH_RETAIL_PCAPS` unset,
//! [`open_or_skip`] prints one `SKIP` line naming the test and the running count of skips, and
//! returns `None`. Set but with no index is a failure: the corpus is there and was not ingested.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use rusqlite::Connection;

/// The variable that names the corpus roots.
pub const ENV_ROOTS: &str = "DERETH_RETAIL_PCAPS";
/// The variable that overrides where the index is.
pub const ENV_INDEX: &str = "DERETH_PCAP_INDEX";

static SKIPS: AtomicUsize = AtomicUsize::new(0);

/// The corpus roots from [`ENV_ROOTS`] (a path list: `;` on Windows, `:` elsewhere), if set.
#[must_use]
pub fn roots_from_env() -> Option<Vec<PathBuf>> {
    let v = std::env::var_os(ENV_ROOTS)?;
    let roots: Vec<PathBuf> = std::env::split_paths(&v)
        .filter(|p| !p.as_os_str().is_empty())
        .collect();
    (!roots.is_empty()).then_some(roots)
}

/// Where the index is: [`ENV_INDEX`], else `<first root>/.index/retail.sqlite`.
#[must_use]
pub fn index_path(roots: &[PathBuf]) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(ENV_INDEX) {
        return Some(PathBuf::from(p));
    }
    roots
        .first()
        .map(|r| r.join(".index").join("retail.sqlite"))
}

/// One session.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// The row id: renumbered by every rebuild. Cite [`SessionInfo::session_key`] instead.
    pub session_id: i64,
    /// The stable key, `<capture hash prefix>:<ordinal>` ([`crate::index::key_sessions`]).
    pub session_key: String,
    pub file: String,
    pub member: String,
    pub partial: bool,
    pub version: Option<String>,
    /// `ac1`, `ac2` or `unknown` ([`crate::game`]).
    pub game: String,
    pub messages_c2s: i64,
    pub messages_s2c: i64,
    pub duration: f64,
}

/// One message.
#[derive(Debug, Clone)]
pub struct MessageInfo {
    pub message_id: i64,
    pub session_id: i64,
    pub idx: i64,
    /// `"c2s"` or `"s2c"`.
    pub dir: String,
    pub t: f64,
    pub opcode: u32,
    pub sub_opcode: Option<u32>,
    pub mtype: u32,
    /// `ok`, `error`, `unknown`, `short`, or `ac2` (a message of an AC2 session, not decoded).
    pub status: String,
    pub error_kind: Option<String>,
    pub raw: Vec<u8>,
}

/// An open index.
#[derive(Debug)]
pub struct Corpus {
    pub conn: Connection,
    pub path: PathBuf,
}

/// Open the index named by the environment, or print a counted skip and return `None` when
/// [`ENV_ROOTS`] is unset.
///
/// # Panics
/// When the roots are set but the index is missing or unreadable: an available corpus that was
/// not ingested must not pass as a skip.
#[must_use]
pub fn open_or_skip(test: &str) -> Option<Corpus> {
    let Some(roots) = roots_from_env() else {
        let n = SKIPS.fetch_add(1, Ordering::Relaxed) + 1;
        println!("SKIP {test}: {ENV_ROOTS} is unset (retail-pcap skip #{n} in this process)");
        return None;
    };
    let path = index_path(&roots).expect("roots are non-empty");
    match crate::index::open_for_read(&path) {
        Ok(conn) => Some(Corpus { conn, path }),
        Err(e) => panic!("{test}: {ENV_ROOTS} is set but the index is not usable: {e}"),
    }
}

impl Corpus {
    /// Sessions matching an SQL condition over `session` columns (`"1"` for all).
    ///
    /// # Errors
    /// On SQL error.
    pub fn sessions(&self, condition: &str) -> rusqlite::Result<Vec<SessionInfo>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT session_id, session_key, file, member, partial, version, game, messages_c2s,
                    messages_s2c, coalesce(duration, 0)
             FROM v_session WHERE {condition} ORDER BY session_id"
        ))?;
        let rows = st.query_map([], |r| {
            Ok(SessionInfo {
                session_id: r.get(0)?,
                session_key: r.get(1)?,
                file: r.get(2)?,
                member: r.get(3)?,
                partial: r.get(4)?,
                version: r.get(5)?,
                game: r.get(6)?,
                messages_c2s: r.get(7)?,
                messages_s2c: r.get(8)?,
                duration: r.get(9)?,
            })
        })?;
        rows.collect()
    }

    /// Call `f` for each message of a session matching an SQL condition over `message` columns,
    /// in session order. Streams: the session is never loaded whole.
    ///
    /// # Errors
    /// On SQL error.
    pub fn for_each_message(
        &self,
        session_id: i64,
        condition: &str,
        mut f: impl FnMut(&MessageInfo),
    ) -> rusqlite::Result<()> {
        let mut st = self.conn.prepare(&format!(
            "SELECT message_id, session_id, idx, dir, t, opcode, sub_opcode, mtype, status,
                    error_kind, raw
             FROM message WHERE session_id = ?1 AND ({condition}) ORDER BY idx"
        ))?;
        let mut q = st.query([session_id])?;
        while let Some(r) = q.next()? {
            let m = MessageInfo {
                message_id: r.get(0)?,
                session_id: r.get(1)?,
                idx: r.get(2)?,
                dir: r.get(3)?,
                t: r.get(4)?,
                opcode: r.get(5)?,
                sub_opcode: r.get(6)?,
                mtype: r.get(7)?,
                status: r.get(8)?,
                error_kind: r.get(9)?,
                raw: r.get(10)?,
            };
            f(&m);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs against the real index when the corpus is configured, and is a counted skip otherwise.
    #[test]
    fn every_message_in_the_corpus_has_a_type_dword_or_is_counted_short() {
        let Some(c) =
            open_or_skip("every_message_in_the_corpus_has_a_type_dword_or_is_counted_short")
        else {
            return;
        };
        let short: i64 = c
            .conn
            .query_row(
                "SELECT count(*) FROM message WHERE len < 4 AND status <> 'short'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(short, 0);
        let sessions = c.sessions("messages_s2c > 0").unwrap();
        assert!(!sessions.is_empty(), "an ingested corpus has sessions");
        let mut n = 0;
        c.for_each_message(sessions[0].session_id, "1", |m| {
            assert!(m.raw.len() >= 4 || m.status == "short");
            n += 1;
        })
        .unwrap();
        assert!(n > 0);
    }
}
