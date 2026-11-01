//! The players' qualities as the servers sent them, against the client's own header and size.

use auberean_protocol::ServerMessage;
use rusqlite::{Connection, OpenFlags};

/// The server's description of the player's qualities.
const PLAYER_QUALITIES: u32 = 0x10068;

/// Every player's qualities the 2005, 2013 and 2017 servers sent carry the header the client would
/// work out from them (the description's bit, and a bit for each table holding an entry: no table
/// is sent empty), and pack to as many bytes as were sent after the message's type word.
///
/// Behaviour: protocol.qualities.the-header-names-the-tables-that-hold-entries
#[test]
fn every_recorded_players_qualities_carry_the_header_the_client_works_out() {
    let index = auberean_testkit::data::require_capture_index();
    let uri = format!(
        "file:{}?mode=ro&immutable=1",
        index.to_string_lossy().replace(char::from(92), "/")
    );
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_URI;
    let con = Connection::open_with_flags(uri, flags).expect("the capture index opens read-only");
    let mut stmt = con
        .prepare(&format!(
            "SELECT m.session_id, m.idx, m.raw FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) WHERE m.status = 'ac2' \
             AND m.mtype = {PLAYER_QUALITIES} AND c.file NOT LIKE '2009-%' \
             ORDER BY m.session_id, m.idx"
        ))
        .expect("the query");
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })
        .expect("the rows");
    let (mut seen, mut same_header, mut same_size) = (0u64, 0u64, 0u64);
    let mut differ = Vec::new();
    for row in rows {
        let (session, index, raw) = row.expect("a row");
        let Ok(ServerMessage::PlayerQualities(q)) = ServerMessage::decode(&raw) else {
            continue;
        };
        seen += 1;
        if q.0.header == q.0.pack_header() {
            same_header += 1;
        } else if differ.len() < 5 {
            differ.push((session, index, q.0.header, q.0.pack_header()));
        }
        if q.0.pack_size() == raw.len() - 4 {
            same_size += 1;
        }
    }
    eprintln!("{seen} players' qualities: {same_header} the same header, {same_size} the same size; {differ:x?}");
    assert!(seen >= 169, "{seen}");
    assert_eq!(same_header, seen, "{differ:x?}");
    assert_eq!(same_size, seen);
}
