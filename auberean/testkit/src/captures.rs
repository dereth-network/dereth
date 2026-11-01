//! The recorded sessions: messages read from the capture index, for tests that replay them.
//!
//! The index is opened read-only. Messages come in session order (by session, then by their
//! place in the session), each with the year its capture was recorded in, read from the start of
//! the capture's file name.

use std::path::Path;

use rusqlite::{Connection, OpenFlags};

/// Message types of script calls: client to server, and the three server-to-client carriers
/// (to the player, to an object in a cell, to an object by its visual).
pub mod carrier {
    pub const TO_SERVER: u32 = 0x10053;
    pub const TO_PLAYER: u32 = 0x10066;
    pub const TO_CELL_OBJECT: u32 = 0x10067;
    pub const TO_VISUAL_OBJECT: u32 = 0x100C4;
    /// Every carrier.
    pub const ALL: [u32; 4] = [TO_SERVER, TO_PLAYER, TO_CELL_OBJECT, TO_VISUAL_OBJECT];
}

/// One recorded message.
#[derive(Clone, Debug)]
pub struct Message {
    /// The capture the session was recorded in: one run of the client, its sessions in order.
    pub capture: i64,
    pub session: i64,
    pub index: i64,
    /// Server to client.
    pub to_client: bool,
    /// The year the capture was recorded.
    pub year: u16,
    pub message_type: u32,
    /// The whole message, its type word first.
    pub raw: Vec<u8>,
    /// Its 64-bit blob id: ordering type, server, ordering stamp, sequence.
    pub blob_id: u64,
    /// When it was recorded, seconds on the capture's clock.
    pub time: f64,
}

/// A script call carried by a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptCall {
    /// The object a server-to-client call is for, when the carrier names one.
    pub object: Option<u64>,
    pub function: u32,
    pub args: Vec<u8>,
}

fn word(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

impl Message {
    /// The script call this message carries: client to server, a function, a size and the
    /// arguments; server to client, the object for the cell and visual carriers (a 64-bit id
    /// and two 16-bit stamps), the function, alignment, then a size and the arguments when any
    /// bytes are left.
    #[must_use]
    pub fn script_call(&self) -> Option<ScriptCall> {
        let b = &self.raw;
        match self.message_type {
            carrier::TO_SERVER => {
                let function = word(b, 4)?;
                let n = word(b, 8)? as usize;
                let args = b.get(12..12 + n)?.to_vec();
                (12 + n == b.len()).then_some(ScriptCall {
                    object: None,
                    function,
                    args,
                })
            }
            carrier::TO_PLAYER | carrier::TO_CELL_OBJECT | carrier::TO_VISUAL_OBJECT => {
                let mut at = 4;
                let object = if self.message_type == carrier::TO_PLAYER {
                    None
                } else {
                    let lo = word(b, at)?;
                    let hi = word(b, at + 4)?;
                    at += 12;
                    Some(u64::from(hi) << 32 | u64::from(lo))
                };
                let function = word(b, at)?;
                at += 4;
                at = (at + 3) & !3;
                if b.len() < at + 4 {
                    return (b.len() == at).then_some(ScriptCall {
                        object,
                        function,
                        args: Vec::new(),
                    });
                }
                let n = word(b, at)? as usize;
                let args = b.get(at + 4..at + 4 + n)?.to_vec();
                (at + 4 + n == b.len()).then_some(ScriptCall {
                    object,
                    function,
                    args,
                })
            }
            _ => None,
        }
    }
}

/// Open the index read-only.
pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_URI;
    let uri = format!(
        "file:{}?mode=ro&immutable=1",
        path.to_string_lossy().replace('\\', "/")
    );
    Connection::open_with_flags(uri, flags)
}

/// Every decoded message of the given types recorded in the given years, in capture and session
/// order, handed to `each`. Returns how many there were.
pub fn messages(
    path: &Path,
    types: &[u32],
    years: &[u16],
    each: impl FnMut(Message),
) -> rusqlite::Result<usize> {
    messages_ordered(
        path,
        types,
        years,
        "s.capture_id, m.session_id, m.idx",
        each,
    )
}

/// [`messages`] in the order they were recorded within each capture, across its sessions (a
/// session's messages keep their own order).
pub fn messages_in_time_order(
    path: &Path,
    types: &[u32],
    years: &[u16],
    each: impl FnMut(Message),
) -> rusqlite::Result<usize> {
    messages_ordered(
        path,
        types,
        years,
        "s.capture_id, s.t_first + m.t, m.session_id, m.idx",
        each,
    )
}

/// The messages of the given types of one capture (the run of the client the session with key
/// `session_key` belongs to), in the order they were recorded across its sessions.
pub fn capture_messages_in_time_order(
    path: &Path,
    types: &[u32],
    session_key: &str,
    mut each: impl FnMut(Message),
) -> rusqlite::Result<usize> {
    let con = open(path)?;
    let capture: i64 = con.query_row(
        "SELECT capture_id FROM session WHERE session_key = ?1",
        [session_key],
        |r| r.get(0),
    )?;
    capture_messages_by_id(&con, types, capture, &mut each)
}

/// The messages of the given types of the capture `capture` (by its id in the index), in the
/// order they were recorded across its sessions.
pub fn capture_by_id_in_time_order(
    path: &Path,
    types: &[u32],
    capture: i64,
    mut each: impl FnMut(Message),
) -> rusqlite::Result<usize> {
    let con = open(path)?;
    capture_messages_by_id(&con, types, capture, &mut each)
}

fn capture_messages_by_id(
    con: &Connection,
    types: &[u32],
    capture: i64,
    each: &mut impl FnMut(Message),
) -> rusqlite::Result<usize> {
    let types: Vec<String> = types.iter().map(u32::to_string).collect();
    let sql = format!(
        "SELECT m.session_id, m.idx, m.dir, m.mtype, m.raw, substr(c.file, 1, 4), s.capture_id, m.blob_id, s.t_first + m.t \
         FROM message m JOIN session s USING (session_id) JOIN capture c USING (capture_id) \
         WHERE m.status = 'ac2' AND m.mtype IN ({}) AND s.capture_id = ?1 \
         ORDER BY s.t_first + m.t, m.session_id, m.idx",
        types.join(",")
    );
    let mut stmt = con.prepare(&sql)?;
    let mut rows = stmt.query([capture])?;
    let mut n = 0;
    while let Some(r) = rows.next()? {
        let dir: String = r.get(2)?;
        let year: String = r.get(5)?;
        each(Message {
            capture: r.get(6)?,
            session: r.get(0)?,
            index: r.get(1)?,
            to_client: dir == "s2c",
            year: year.parse().unwrap_or(0),
            message_type: r.get(3)?,
            raw: r.get(4)?,
            #[allow(clippy::cast_sign_loss)]
            blob_id: r.get::<_, i64>(7)? as u64,
            time: r.get(8)?,
        });
        n += 1;
    }
    Ok(n)
}

fn messages_ordered(
    path: &Path,
    types: &[u32],
    years: &[u16],
    order: &str,
    mut each: impl FnMut(Message),
) -> rusqlite::Result<usize> {
    let con = open(path)?;
    let types: Vec<String> = types.iter().map(u32::to_string).collect();
    let years: Vec<String> = years.iter().map(|y| format!("'{y}'")).collect();
    let sql = format!(
        "SELECT m.session_id, m.idx, m.dir, m.mtype, m.raw, substr(c.file, 1, 4), s.capture_id, m.blob_id, s.t_first + m.t \
         FROM message m JOIN session s USING (session_id) JOIN capture c USING (capture_id) \
         WHERE m.status = 'ac2' AND m.mtype IN ({}) AND substr(c.file, 1, 4) IN ({}) \
         ORDER BY {order}",
        types.join(","),
        years.join(",")
    );
    let mut stmt = con.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut n = 0;
    while let Some(r) = rows.next()? {
        let dir: String = r.get(2)?;
        let year: String = r.get(5)?;
        each(Message {
            capture: r.get(6)?,
            session: r.get(0)?,
            index: r.get(1)?,
            to_client: dir == "s2c",
            year: year.parse().unwrap_or(0),
            message_type: r.get(3)?,
            raw: r.get(4)?,
            #[allow(clippy::cast_sign_loss)]
            blob_id: r.get::<_, i64>(7)? as u64,
            time: r.get(8)?,
        });
        n += 1;
    }
    Ok(n)
}
