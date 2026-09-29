//! The SQLite index: its schema, its version, and the writes the ingest makes.
//!
//! The schema is documented table by table, column by column, in the index's query guide; this
//! file and that guide change together, and a change that breaks a query written against the old
//! schema bumps [`SCHEMA_VERSION`].

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use crate::flows::Endpoints;
use crate::transport::{MessageRow, PacketRow, Summary};

/// The schema's version, in `meta.schema_version`. Bump it for any change that can break an
/// existing query (a renamed or removed column or table, a changed meaning); adding a column,
/// table, index or view does not need a bump.
///
/// Version 2 added the session's game (`session.game` and its evidence, `game_ac1`, `game_ac2`,
/// `game_other`) and files the messages of an Asheron's Call 2 session under status `ac2`
/// (counted in `ac2_c2s`/`ac2_s2c`) instead of AC1's `unknown`. An index of another version is
/// refused with a rebuild instruction; there is no in-place migration, because the change is in
/// how messages are decoded, which only a re-ingest redoes. Version 2 also gives every session a
/// stable key (`session.session_key`, see [`key_sessions`]); an index written before the keys were
/// added to version 2 has no such column and is refused the same way.
pub const SCHEMA_VERSION: u32 = 2;

/// Hex digits of the capture's content hash that begin a session key.
pub const KEY_HEX: usize = 16;

const META: &str = "CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);";

const SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS meta(
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS file(
    file_id INTEGER PRIMARY KEY,
    root    TEXT NOT NULL,
    path    TEXT NOT NULL,
    member  TEXT NOT NULL DEFAULT '',
    size    INTEGER NOT NULL,
    mtime   INTEGER,
    hash    TEXT,
    kind    TEXT NOT NULL,
    status  TEXT NOT NULL,
    note    TEXT,
    UNIQUE(root, path, member)
);
CREATE TABLE IF NOT EXISTS capture(
    capture_id       INTEGER PRIMARY KEY,
    hash             TEXT NOT NULL UNIQUE,
    file             TEXT NOT NULL,
    member           TEXT NOT NULL,
    format           TEXT NOT NULL,
    size             INTEGER NOT NULL,
    link_type        INTEGER,
    records          INTEGER NOT NULL DEFAULT 0,
    udp              INTEGER NOT NULL DEFAULT 0,
    game             INTEGER NOT NULL DEFAULT 0,
    non_game         INTEGER NOT NULL DEFAULT 0,
    unknown_link     INTEGER NOT NULL DEFAULT 0,
    not_ipv4         INTEGER NOT NULL DEFAULT 0,
    not_udp          INTEGER NOT NULL DEFAULT 0,
    truncated        INTEGER NOT NULL DEFAULT 0,
    ip_fragments     INTEGER NOT NULL DEFAULT 0,
    ip_reassembled   INTEGER NOT NULL DEFAULT 0,
    ip_unreassembled INTEGER NOT NULL DEFAULT 0,
    ip_len_zero      INTEGER NOT NULL DEFAULT 0,
    truncated_tail   INTEGER NOT NULL DEFAULT 0,
    t_first          REAL,
    t_last           REAL,
    sessions         INTEGER NOT NULL DEFAULT 0,
    error            TEXT,
    complete         INTEGER NOT NULL DEFAULT 0,
    ingested_at      TEXT
);
CREATE TABLE IF NOT EXISTS session(
    session_id         INTEGER PRIMARY KEY,
    capture_id         INTEGER NOT NULL,
    idx                INTEGER NOT NULL,
    session_key        TEXT,
    client_ip          TEXT,
    client_port        INTEGER,
    server_ip          TEXT,
    server_port        INTEGER,
    server_ports       TEXT,
    partial            INTEGER NOT NULL DEFAULT 1,
    login              INTEGER NOT NULL DEFAULT 0,
    world_login        INTEGER NOT NULL DEFAULT 0,
    referral           INTEGER NOT NULL DEFAULT 0,
    version            TEXT,
    seed_s2c           INTEGER,
    seed_c2s           INTEGER,
    disconnect         INTEGER NOT NULL DEFAULT 0,
    t_first            REAL,
    t_last             REAL,
    duration           REAL,
    packets_c2s        INTEGER NOT NULL DEFAULT 0,
    packets_s2c        INTEGER NOT NULL DEFAULT 0,
    bytes_c2s          INTEGER NOT NULL DEFAULT 0,
    bytes_s2c          INTEGER NOT NULL DEFAULT 0,
    messages_c2s       INTEGER NOT NULL DEFAULT 0,
    messages_s2c       INTEGER NOT NULL DEFAULT 0,
    ok_c2s             INTEGER NOT NULL DEFAULT 0,
    error_c2s          INTEGER NOT NULL DEFAULT 0,
    unknown_c2s        INTEGER NOT NULL DEFAULT 0,
    ok_s2c             INTEGER NOT NULL DEFAULT 0,
    error_s2c          INTEGER NOT NULL DEFAULT 0,
    unknown_s2c        INTEGER NOT NULL DEFAULT 0,
    ac2_c2s            INTEGER NOT NULL DEFAULT 0,
    ac2_s2c            INTEGER NOT NULL DEFAULT 0,
    game               TEXT NOT NULL DEFAULT 'unknown',
    game_ac1           INTEGER NOT NULL DEFAULT 0,
    game_ac2           INTEGER NOT NULL DEFAULT 0,
    game_other         INTEGER NOT NULL DEFAULT 0,
    short              INTEGER NOT NULL DEFAULT 0,
    parse_errors       INTEGER NOT NULL DEFAULT 0,
    duplicates         INTEGER NOT NULL DEFAULT 0,
    refused_fragments  INTEGER NOT NULL DEFAULT 0,
    orphans_head       INTEGER NOT NULL DEFAULT 0,
    orphans_tail       INTEGER NOT NULL DEFAULT 0,
    orphans_mid        INTEGER NOT NULL DEFAULT 0,
    cks_checked_c2s    INTEGER NOT NULL DEFAULT 0,
    cks_ok_c2s         INTEGER NOT NULL DEFAULT 0,
    cks_unchecked_c2s  INTEGER NOT NULL DEFAULT 0,
    desynced_after_c2s INTEGER,
    cks_checked_s2c    INTEGER NOT NULL DEFAULT 0,
    cks_ok_s2c         INTEGER NOT NULL DEFAULT 0,
    cks_unchecked_s2c  INTEGER NOT NULL DEFAULT 0,
    desynced_after_s2c INTEGER,
    complete           INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS packet(
    session_id  INTEGER NOT NULL,
    idx         INTEGER NOT NULL,
    dir         TEXT NOT NULL,
    t           REAL NOT NULL,
    seq         INTEGER NOT NULL,
    flags       INTEGER NOT NULL,
    len         INTEGER NOT NULL,
    frags       INTEGER NOT NULL,
    checksum    INTEGER,
    duplicate   INTEGER NOT NULL,
    parse_error TEXT,
    PRIMARY KEY(session_id, idx)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS message(
    message_id   INTEGER PRIMARY KEY,
    session_id   INTEGER NOT NULL,
    idx          INTEGER NOT NULL,
    dir          TEXT NOT NULL,
    t            REAL NOT NULL,
    packet_idx   INTEGER NOT NULL,
    queue        INTEGER NOT NULL,
    blob_id      INTEGER NOT NULL,
    frags        INTEGER NOT NULL,
    opcode       INTEGER NOT NULL,
    sub_opcode   INTEGER,
    mtype        INTEGER NOT NULL,
    order_iid    INTEGER,
    order_stamp  INTEGER,
    len          INTEGER NOT NULL,
    status       TEXT NOT NULL,
    codec_id     INTEGER,
    error_kind   TEXT,
    error_offset INTEGER,
    error        TEXT,
    padding      INTEGER NOT NULL DEFAULT 0,
    raw          BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS message_guid(
    guid       INTEGER NOT NULL,
    session_id INTEGER NOT NULL,
    message_id INTEGER NOT NULL,
    path       TEXT NOT NULL,
    PRIMARY KEY(guid, session_id, message_id, path)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS codec(
    codec_id INTEGER PRIMARY KEY,
    name     TEXT NOT NULL UNIQUE
);
CREATE TABLE IF NOT EXISTS opcode(
    code      INTEGER PRIMARY KEY,
    name      TEXT NOT NULL,
    direction TEXT NOT NULL
);
CREATE VIEW IF NOT EXISTS v_message AS
    SELECT m.*, printf('0x%04X', m.mtype) AS mtype_hex, o.name AS name, k.name AS codec,
           s.session_key, s.session_key || '#' || m.idx AS msg_key,
           s.capture_id, s.partial, s.version, s.game, c.file, c.member
    FROM message m
    JOIN session s ON s.session_id = m.session_id
    JOIN capture c ON c.capture_id = s.capture_id
    LEFT JOIN opcode o ON o.code = m.mtype
    LEFT JOIN codec k ON k.codec_id = m.codec_id;
CREATE VIEW IF NOT EXISTS v_session AS
    SELECT s.*, c.file, c.member, c.hash AS capture_hash
    FROM session s JOIN capture c ON c.capture_id = s.capture_id;
";

/// The indexes for the common questions. Created after a bulk ingest (building them once is
/// cheaper than maintaining them row by row) and kept up to date by incremental ingests after.
const INDEXES: &str = r"
CREATE INDEX IF NOT EXISTS message_type ON message(mtype, dir);
CREATE INDEX IF NOT EXISTS message_session ON message(session_id, t);
CREATE INDEX IF NOT EXISTS message_failed ON message(status, dir, mtype) WHERE status <> 'ok';
CREATE INDEX IF NOT EXISTS session_capture ON session(capture_id);
CREATE UNIQUE INDEX IF NOT EXISTS session_key ON session(session_key);
CREATE INDEX IF NOT EXISTS session_game ON session(game);
CREATE INDEX IF NOT EXISTS file_hash ON file(hash);
";

type Result<T> = rusqlite::Result<T>;

/// Open (creating if needed) an index for writing.
///
/// # Errors
/// On SQLite failure, or when the index was written by a different schema version (rebuild it
/// with `ingest --rebuild`).
pub fn open_for_write(path: &Path) -> std::result::Result<Connection, String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let conn = Connection::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA cache_size=-262144; \
         PRAGMA temp_store=MEMORY; PRAGMA foreign_keys=OFF;",
    )
    .map_err(|e| e.to_string())?;
    // The version is checked before the schema is applied, so an index of another version is
    // refused untouched.
    conn.execute_batch(META).map_err(|e| e.to_string())?;
    let have: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key='schema_version'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    match have {
        None => {
            conn.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO meta(key, value) VALUES ('schema_version', ?1)",
                [SCHEMA_VERSION.to_string()],
            )
            .map_err(|e| e.to_string())?;
        }
        Some(v) if v == SCHEMA_VERSION.to_string() => {
            if !has_session_keys(&conn).map_err(|e| e.to_string())? {
                return Err(unkeyed(path));
            }
            conn.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
        }
        Some(v) => return Err(version_mismatch(path, &v)),
    }
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    for info in dereth_protocol::OPCODES {
        let dir = match info.direction {
            dereth_protocol::Direction::S2C => "s2c",
            dereth_protocol::Direction::C2S => "c2s",
            dereth_protocol::Direction::Both => "both",
        };
        tx.execute(
            "INSERT OR REPLACE INTO opcode(code, name, direction) VALUES (?1, ?2, ?3)",
            params![info.opcode.0, info.name, dir],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(conn)
}

fn version_mismatch(path: &Path, have: &str) -> String {
    format!(
        "{}: schema version {have}, this build uses {SCHEMA_VERSION} (version 2 added each session's \
         game and the `ac2` message status); a rebuild is required: run `dereth-pcap ingest --rebuild`",
        path.display()
    )
}

/// Whether the `session` table has its `session_key` column (or does not exist yet).
fn has_session_keys(conn: &Connection) -> Result<bool> {
    let columns: i64 = conn.query_row(
        "SELECT count(*) FROM pragma_table_info('session')",
        [],
        |r| r.get(0),
    )?;
    let key: i64 = conn.query_row(
        "SELECT count(*) FROM pragma_table_info('session') WHERE name = 'session_key'",
        [],
        |r| r.get(0),
    )?;
    Ok(columns == 0 || key == 1)
}

fn unkeyed(path: &Path) -> String {
    format!(
        "{}: schema version {SCHEMA_VERSION} without session keys (built before `session_key` was added); \
         a rebuild is required: run `dereth-pcap ingest --rebuild`",
        path.display()
    )
}

/// Open an existing index read-only.
///
/// # Errors
/// When it is missing, unreadable or of another schema version.
pub fn open_for_read(path: &Path) -> std::result::Result<Connection, String> {
    if !path.exists() {
        return Err(format!(
            "{}: no index; run `dereth-pcap ingest` first",
            path.display()
        ));
    }
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let v: String = conn
        .query_row(
            "SELECT value FROM meta WHERE key='schema_version'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if v != SCHEMA_VERSION.to_string() {
        return Err(version_mismatch(path, &v));
    }
    if !has_session_keys(&conn).map_err(|e| e.to_string())? {
        return Err(unkeyed(path));
    }
    register_functions(&conn).map_err(|e| e.to_string())?;
    Ok(conn)
}

/// Build the query indexes and, when the contents changed, refresh the planner's statistics.
///
/// # Errors
/// On SQLite failure.
pub fn build_indexes(conn: &Connection, analyze: bool) -> Result<()> {
    conn.execute_batch(INDEXES)?;
    if analyze {
        conn.execute_batch("ANALYZE;")?;
    }
    Ok(())
}

/// Set a `meta` value.
///
/// # Errors
/// On SQLite failure.
pub fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO meta(key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

/// Remove captures and everything under them.
///
/// # Errors
/// On SQLite failure.
pub fn purge_captures(conn: &Connection, where_clause: &str) -> Result<usize> {
    conn.execute_batch(&format!(
        "CREATE TEMP TABLE IF NOT EXISTS purge_c(id INTEGER PRIMARY KEY);
         DELETE FROM purge_c;
         INSERT INTO purge_c SELECT capture_id FROM capture WHERE {where_clause};
         CREATE TEMP TABLE IF NOT EXISTS purge_s(id INTEGER PRIMARY KEY);
         DELETE FROM purge_s;
         INSERT INTO purge_s SELECT session_id FROM session WHERE capture_id IN (SELECT id FROM purge_c);
         CREATE TEMP TABLE IF NOT EXISTS purge_m(id INTEGER PRIMARY KEY);
         DELETE FROM purge_m;
         INSERT INTO purge_m SELECT message_id FROM message WHERE session_id IN (SELECT id FROM purge_s);
         DELETE FROM message_guid WHERE session_id IN (SELECT id FROM purge_s);
         DELETE FROM message WHERE session_id IN (SELECT id FROM purge_s);
         DELETE FROM packet WHERE session_id IN (SELECT id FROM purge_s);
         DELETE FROM session WHERE session_id IN (SELECT id FROM purge_s);
         DELETE FROM capture WHERE capture_id IN (SELECT id FROM purge_c);"
    ))?;
    let n: i64 = conn.query_row("SELECT count(*) FROM purge_c", [], |r| r.get(0))?;
    Ok(usize::try_from(n).unwrap_or(0))
}

/// What a capture's link layer and router counted.
#[derive(Debug, Clone, Default)]
pub struct CaptureStats {
    pub link_type: Option<u32>,
    pub records: u64,
    pub udp: u64,
    pub game: u64,
    pub non_game: u64,
    pub link: crate::link::LinkStats,
    pub truncated_tail: bool,
    pub t_first: Option<f64>,
    pub t_last: Option<f64>,
    pub sessions: u64,
    pub error: Option<String>,
}

/// A file row.
#[derive(Debug, Clone)]
pub struct FileRow {
    pub root: String,
    pub path: String,
    pub member: String,
    pub size: u64,
    pub mtime: Option<i64>,
    pub hash: Option<String>,
    pub kind: &'static str,
    pub status: &'static str,
    pub note: Option<String>,
}

fn i64_of(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

/// Insert a file row, replacing an older one for the same (root, path, member).
///
/// # Errors
/// On SQLite failure.
pub fn insert_file(conn: &Connection, f: &FileRow) -> Result<()> {
    conn.prepare_cached(
        "INSERT OR REPLACE INTO file(root, path, member, size, mtime, hash, kind, status, note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?
    .execute(params![
        f.root,
        f.path,
        f.member,
        i64_of(f.size),
        f.mtime,
        f.hash,
        f.kind,
        f.status,
        f.note
    ])?;
    Ok(())
}

/// Start a capture; returns its id, or `None` when a capture with this hash already exists.
///
/// # Errors
/// On SQLite failure.
pub fn begin_capture(
    conn: &Connection,
    hash: &str,
    file: &str,
    member: &str,
    format: &str,
    size: u64,
) -> Result<Option<i64>> {
    let n = conn
        .prepare_cached(
            "INSERT OR IGNORE INTO capture(hash, file, member, format, size, ingested_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
        )?
        .execute(params![hash, file, member, format, i64_of(size)])?;
    if n == 0 {
        return Ok(None);
    }
    Ok(Some(conn.last_insert_rowid()))
}

/// Finish a capture.
///
/// # Errors
/// On SQLite failure.
pub fn end_capture(conn: &Connection, id: i64, s: &CaptureStats) -> Result<()> {
    conn.prepare_cached(
        "UPDATE capture SET link_type=?2, records=?3, udp=?4, game=?5, non_game=?6, unknown_link=?7,
             not_ipv4=?8, not_udp=?9, truncated=?10, ip_fragments=?11, ip_reassembled=?12,
             ip_unreassembled=?13, truncated_tail=?14, t_first=?15, t_last=?16, sessions=?17,
             error=?18, ip_len_zero=?19, complete=1
         WHERE capture_id=?1",
    )?
    .execute(params![
        id,
        s.link_type,
        i64_of(s.records),
        i64_of(s.udp),
        i64_of(s.game),
        i64_of(s.non_game),
        i64_of(s.link.unknown_link),
        i64_of(s.link.not_ipv4),
        i64_of(s.link.not_udp),
        i64_of(s.link.truncated),
        i64_of(s.link.ip_fragments),
        i64_of(s.link.ip_reassembled),
        i64_of(s.link.ip_unreassembled),
        s.truncated_tail,
        s.t_first,
        s.t_last,
        i64_of(s.sessions),
        s.error,
        i64_of(s.link.ip_len_zero),
    ])?;
    key_sessions(conn, id)
}

/// Give each session of a capture its stable key, `<first KEY_HEX hex digits of the capture's
/// content hash>:<ordinal>`. The ordinal numbers the capture's sessions from 0 by first datagram
/// time, then server port, client port, client address, server address, and last the order the
/// file met them in (`idx`). Every input is the capture's own content, so the key is the same
/// whatever the file is called, whichever archive holds it, and whatever else is ingested with it,
/// in whatever order or with however many workers (for one `--server-ports` setting, which decides
/// what a session is).
///
/// # Errors
/// On SQLite failure (including two captures whose hashes share their first [`KEY_HEX`] digits).
pub fn key_sessions(conn: &Connection, capture: i64) -> Result<()> {
    let hash: String = conn.query_row(
        "SELECT hash FROM capture WHERE capture_id = ?1",
        [capture],
        |r| r.get(0),
    )?;
    let prefix = &hash[..hash.len().min(KEY_HEX)];
    let ids: Vec<i64> = conn
        .prepare_cached(
            "SELECT session_id FROM session WHERE capture_id = ?1
             ORDER BY t_first, server_port, client_port, client_ip, server_ip, idx",
        )?
        .query_map([capture], |r| r.get(0))?
        .collect::<Result<_>>()?;
    let mut st =
        conn.prepare_cached("UPDATE session SET session_key = ?2 WHERE session_id = ?1")?;
    for (ordinal, id) in ids.into_iter().enumerate() {
        st.execute(params![id, format!("{prefix}:{ordinal}")])?;
    }
    Ok(())
}

/// Start a session; returns its id.
///
/// # Errors
/// On SQLite failure.
pub fn begin_session(conn: &Connection, capture: i64, idx: usize) -> Result<i64> {
    conn.prepare_cached("INSERT INTO session(capture_id, idx) VALUES (?1, ?2)")?
        .execute(params![capture, i64_of(idx as u64)])?;
    Ok(conn.last_insert_rowid())
}

/// Finish a session.
///
/// # Errors
/// On SQLite failure.
pub fn end_session(conn: &Connection, id: i64, e: &Endpoints, s: &Summary) -> Result<()> {
    let ports: Vec<String> = e.server_ports.iter().map(u16::to_string).collect();
    let (c, sc) = (0, 1);
    conn.prepare_cached(
        "UPDATE session SET client_ip=?2, client_port=?3, server_ip=?4, server_port=?5,
             server_ports=?6, partial=?7, login=?8, version=?9, seed_s2c=?10, seed_c2s=?11,
             disconnect=?12, t_first=?13, t_last=?14, duration=?15,
             packets_c2s=?16, packets_s2c=?17, bytes_c2s=?18, bytes_s2c=?19,
             messages_c2s=?20, messages_s2c=?21, ok_c2s=?22, error_c2s=?23, unknown_c2s=?24,
             ok_s2c=?25, error_s2c=?26, unknown_s2c=?27, short=?28, parse_errors=?29,
             duplicates=?30, refused_fragments=?31, orphans_head=?32, orphans_tail=?33,
             orphans_mid=?34, cks_checked_c2s=?35, cks_ok_c2s=?36, cks_unchecked_c2s=?37,
             desynced_after_c2s=?38, cks_checked_s2c=?39, cks_ok_s2c=?40, cks_unchecked_s2c=?41,
             desynced_after_s2c=?42, world_login=?43, referral=?44, ac2_c2s=?45, ac2_s2c=?46,
             game=?47, game_ac1=?48, game_ac2=?49, game_other=?50, complete=1
         WHERE session_id=?1",
    )?
    .execute(params![
        id,
        e.client.0.to_string(),
        e.client.1,
        e.server_ip.to_string(),
        e.server_ports.iter().next().copied(),
        ports.join(","),
        s.partial,
        s.login,
        s.version,
        s.seeds.map(|x| x.0),
        s.seeds.map(|x| x.1),
        s.disconnect,
        s.t_first,
        s.t_last,
        s.t_last - s.t_first,
        i64_of(s.packets[c]),
        i64_of(s.packets[sc]),
        i64_of(s.bytes[c]),
        i64_of(s.bytes[sc]),
        i64_of(s.messages[c]),
        i64_of(s.messages[sc]),
        i64_of(s.status[c][0]),
        i64_of(s.status[c][1]),
        i64_of(s.status[c][2]),
        i64_of(s.status[sc][0]),
        i64_of(s.status[sc][1]),
        i64_of(s.status[sc][2]),
        i64_of(s.status[c][3] + s.status[sc][3]),
        i64_of(s.parse_errors),
        i64_of(s.duplicates),
        i64_of(s.refused_fragments),
        i64_of(s.orphans_head),
        i64_of(s.orphans_tail),
        i64_of(s.orphans_mid),
        i64_of(s.checksum[c].checked),
        i64_of(s.checksum[c].ok),
        i64_of(s.checksum[c].unchecked),
        s.checksum[c].desynced_after.map(i64_of),
        i64_of(s.checksum[sc].checked),
        i64_of(s.checksum[sc].ok),
        i64_of(s.checksum[sc].unchecked),
        s.checksum[sc].desynced_after.map(i64_of),
        s.world_login,
        s.referral,
        i64_of(s.status[c][4]),
        i64_of(s.status[sc][4]),
        s.game.as_str(),
        i64_of(s.evidence.ac1),
        i64_of(s.evidence.ac2),
        i64_of(s.evidence.other),
    ])?;
    Ok(())
}

/// The `codec` table's ids, cached.
#[derive(Debug, Default)]
pub struct Codecs(std::collections::BTreeMap<&'static str, i64>);

impl Codecs {
    /// The id of a codec name, adding it on first use.
    ///
    /// # Errors
    /// On SQLite failure.
    pub fn id(&mut self, conn: &Connection, name: &'static str) -> Result<i64> {
        if let Some(id) = self.0.get(name) {
            return Ok(*id);
        }
        conn.prepare_cached("INSERT OR IGNORE INTO codec(name) VALUES (?1)")?
            .execute([name])?;
        let id = conn.query_row("SELECT codec_id FROM codec WHERE name = ?1", [name], |r| {
            r.get(0)
        })?;
        self.0.insert(name, id);
        Ok(id)
    }
}

fn decode_args(ctx: &rusqlite::functions::Context<'_>) -> Result<Option<crate::decode::Decoded>> {
    let dir: Option<String> = ctx.get(0)?;
    let raw: Option<Vec<u8>> = ctx.get(1)?;
    let (Some(dir), Some(raw)) = (dir, raw) else {
        return Ok(None);
    };
    let dir = match dir.as_str() {
        "c2s" => crate::flows::Dir::C2s,
        "s2c" => crate::flows::Dir::S2c,
        _ => return Ok(None),
    };
    Ok(Some(crate::decode::decode(dir, &raw)))
}

/// Register the SQL functions every connection `dereth-pcap` opens for reading gets:
///
/// * `fields(dir, raw)`: the message's decoded fields as JSON (the decoded-fields form, computed
///   on demand from the stored bytes by the decoder the ingest ran), or NULL when it does not
///   decode;
/// * `guids(dir, raw)`: the object ids in it, as a JSON array of `[guid, path]`;
/// * `type_name(mtype)`: the master opcode table's name for a type;
/// * `sid(key)`: the current `session_id` of a session key (`3f2a9c0d1e4b5a67:4`), or of the
///   session of a message key (`3f2a9c0d1e4b5a67:4#120`); NULL when no session has it. The keys
///   are read when the connection opens, which is right for a read-only connection.
///
/// # Errors
/// On SQLite failure.
pub fn register_functions(conn: &Connection) -> Result<()> {
    use rusqlite::functions::FunctionFlags;
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    conn.create_scalar_function("fields", 2, flags, |ctx| {
        Ok(decode_args(ctx)?.and_then(|d| d.fields))
    })?;
    conn.create_scalar_function("guids", 2, flags, |ctx| {
        Ok(decode_args(ctx)?.map(|d| serde_json::json!(d.guids).to_string()))
    })?;
    conn.create_scalar_function("type_name", 1, flags, |ctx| {
        let code: Option<i64> = ctx.get(0)?;
        Ok(code
            .and_then(|c| u32::try_from(c).ok())
            .and_then(crate::decode::type_name))
    })?;
    let keys: std::collections::HashMap<String, i64> = conn
        .prepare("SELECT session_key, session_id FROM session WHERE session_key IS NOT NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_>>()?;
    conn.create_scalar_function("sid", 1, flags, move |ctx| {
        let key: Option<String> = ctx.get(0)?;
        Ok(key.and_then(|k| keys.get(k.split('#').next().unwrap_or_default()).copied()))
    })?;
    Ok(())
}

/// Insert packet rows.
///
/// # Errors
/// On SQLite failure.
pub fn insert_packets(conn: &Connection, session: i64, rows: &[PacketRow]) -> Result<()> {
    let mut st = conn.prepare_cached(
        "INSERT INTO packet(session_id, idx, dir, t, seq, flags, len, frags, checksum, duplicate, parse_error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?;
    for p in rows {
        st.execute(params![
            session,
            p.idx,
            p.dir.as_str(),
            p.t,
            p.seq,
            p.flags,
            p.len,
            p.frags,
            p.checksum,
            p.duplicate,
            p.parse_error,
        ])?;
    }
    Ok(())
}

/// Insert message rows and their object ids.
///
/// # Errors
/// On SQLite failure.
pub fn insert_messages(
    conn: &Connection,
    codecs: &mut Codecs,
    session: i64,
    rows: &[MessageRow],
) -> Result<()> {
    let mut st = conn.prepare_cached(
        "INSERT INTO message(session_id, idx, dir, t, packet_idx, queue, blob_id, frags, opcode,
             sub_opcode, mtype, order_iid, order_stamp, len, status, codec_id, error_kind,
             error_offset, error, padding, raw)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
             ?19, ?20, ?21)",
    )?;
    let mut g = conn.prepare_cached(
        "INSERT OR IGNORE INTO message_guid(guid, session_id, message_id, path) VALUES (?1, ?2, ?3, ?4)",
    )?;
    for m in rows {
        let d = &m.decoded;
        let f = d.failure.as_ref();
        let codec = match d.codec {
            Some(c) => Some(codecs.id(conn, c)?),
            None => None,
        };
        st.execute(params![
            session,
            m.idx,
            m.dir.as_str(),
            m.t,
            m.packet_idx,
            m.queue,
            // Blob ids use all 64 bits; SQLite integers are signed, so the bits are stored as is.
            i64::from_ne_bytes(m.blob_id.to_ne_bytes()),
            m.frags,
            d.opcode,
            d.sub_opcode,
            d.mtype,
            d.order_iid,
            d.order_stamp,
            i64_of(m.raw.len() as u64),
            d.status.as_str(),
            codec,
            f.map(|f| f.kind.as_str()),
            f.map(|f| i64_of(f.offset as u64)),
            f.map(|f| f.text.as_str()),
            d.padding,
            m.raw,
        ])?;
        let id = conn.last_insert_rowid();
        for (guid, path) in &d.guids {
            g.execute(params![guid, session, id, path])?;
        }
    }
    Ok(())
}

/// Decode a session's stored messages again as `game`, rewriting their decode columns and object
/// ids; returns the new per-direction status counts (ok, error, unknown, short, ac2). Used when
/// the game decided on a session's first messages is not the one the whole session votes for.
///
/// # Errors
/// On SQLite failure.
pub fn redecode_session(
    conn: &Connection,
    codecs: &mut Codecs,
    session: i64,
    game: crate::game::Game,
) -> Result<[[u64; 5]; 2]> {
    let mut counts = [[0u64; 5]; 2];
    conn.execute("DELETE FROM message_guid WHERE session_id = ?1", [session])?;
    let ids: Vec<(i64, String)> = conn
        .prepare("SELECT message_id, dir FROM message WHERE session_id = ?1 ORDER BY message_id")?
        .query_map([session], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_>>()?;
    let mut read = conn.prepare_cached("SELECT raw FROM message WHERE message_id = ?1")?;
    let mut st = conn.prepare_cached(
        "UPDATE message SET opcode=?2, sub_opcode=?3, mtype=?4, order_iid=?5, order_stamp=?6,
             status=?7, codec_id=?8, error_kind=?9, error_offset=?10, error=?11, padding=?12
         WHERE message_id=?1",
    )?;
    let mut g = conn.prepare_cached(
        "INSERT OR IGNORE INTO message_guid(guid, session_id, message_id, path) VALUES (?1, ?2, ?3, ?4)",
    )?;
    for (id, dir) in ids {
        let dir = if dir == "c2s" {
            crate::flows::Dir::C2s
        } else {
            crate::flows::Dir::S2c
        };
        let raw: Vec<u8> = read.query_row([id], |r| r.get(0))?;
        let d = crate::decode::decode_as(game, dir, &raw);
        let f = d.failure.as_ref();
        let codec = match d.codec {
            Some(c) => Some(codecs.id(conn, c)?),
            None => None,
        };
        st.execute(params![
            id,
            d.opcode,
            d.sub_opcode,
            d.mtype,
            d.order_iid,
            d.order_stamp,
            d.status.as_str(),
            codec,
            f.map(|f| f.kind.as_str()),
            f.map(|f| i64_of(f.offset as u64)),
            f.map(|f| f.text.as_str()),
            d.padding,
        ])?;
        for (guid, path) in &d.guids {
            g.execute(params![guid, session, id, path])?;
        }
        counts[dir.index()][d.status.index()] += 1;
    }
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_index_of_another_schema_version_is_refused_with_a_rebuild_instruction() {
        let dir = std::env::temp_dir().join(format!("pcap-schema-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("v1.sqlite");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO meta VALUES ('schema_version', '1');
                 CREATE TABLE session(session_id INTEGER PRIMARY KEY, capture_id INTEGER NOT NULL);",
            )
            .unwrap();
        }
        let w = open_for_write(&path).unwrap_err();
        assert!(
            w.contains("schema version 1") && w.contains("ingest --rebuild"),
            "{w}"
        );
        let r = open_for_read(&path).unwrap_err();
        assert!(r.contains("rebuild is required"), "{r}");
        // Refused untouched: the old session table did not gain the new columns.
        let c = Connection::open(&path).unwrap();
        let n: i64 = c
            .query_row(
                "SELECT count(*) FROM pragma_table_info('session') WHERE name = 'game'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
        drop(c);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_version_2_index_built_before_session_keys_is_refused_with_a_rebuild_instruction() {
        let dir = std::env::temp_dir().join(format!("pcap-schema-unkeyed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("v2.sqlite");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO meta VALUES ('schema_version', '2');
                 CREATE TABLE session(session_id INTEGER PRIMARY KEY, capture_id INTEGER NOT NULL, game TEXT);",
            )
            .unwrap();
        }
        let w = open_for_write(&path).unwrap_err();
        assert!(
            w.contains("without session keys") && w.contains("ingest --rebuild"),
            "{w}"
        );
        let r = open_for_read(&path).unwrap_err();
        assert!(r.contains("without session keys"), "{r}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
