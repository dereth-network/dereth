//! `stats`, `query` and `triage`: reading the index back.
//!
//! Both reports are per game ([`crate::game`]). The AC1 decode rates and type lists count
//! sessions of game `ac1` only; the lists of AC1 decode failures and codec-less types cover every
//! session the AC1 decoder ran on (`ac1` and `unknown`); AC2 sessions get their own census.

use std::fmt::Write as _;
use std::time::Instant;

use rusqlite::types::ValueRef;
use rusqlite::Connection;

type Res<T> = Result<T, String>;

fn e(e: rusqlite::Error) -> String {
    e.to_string()
}

/// Run a query and return its column names and rows as display strings. Blobs are shown as hex,
/// truncated to `blob_cap` bytes (0: no limit).
///
/// # Errors
/// On SQL error.
pub fn rows(conn: &Connection, sql: &str, blob_cap: usize) -> Res<(Vec<String>, Vec<Vec<String>>)> {
    let mut st = conn.prepare(sql).map_err(e)?;
    let cols: Vec<String> = st.column_names().iter().map(|s| (*s).to_string()).collect();
    let n = cols.len();
    let mut out = Vec::new();
    let mut q = st.query([]).map_err(e)?;
    while let Some(r) = q.next().map_err(e)? {
        let mut row = Vec::with_capacity(n);
        for i in 0..n {
            row.push(cell(r.get_ref(i).map_err(e)?, blob_cap));
        }
        out.push(row);
    }
    Ok((cols, out))
}

fn cell(v: ValueRef<'_>, blob_cap: usize) -> String {
    match v {
        ValueRef::Null => String::new(),
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(f) => f.to_string(),
        ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
        ValueRef::Blob(b) => {
            let shown = if blob_cap == 0 {
                b
            } else {
                &b[..b.len().min(blob_cap)]
            };
            let mut s = hex(shown);
            if shown.len() < b.len() {
                let _ = write!(s, "..(+{})", b.len() - shown.len());
            }
            s
        }
    }
}

/// Lower-case hex.
#[must_use]
pub fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        let _ = write!(s, "{x:02x}");
    }
    s
}

/// Render rows as an aligned text table (cells cut at `width` characters).
#[must_use]
pub fn table(cols: &[String], rows: &[Vec<String>], width: usize) -> String {
    let cut = |s: &str| -> String {
        let s = s.replace(['\n', '\r', '\t'], " ");
        if s.chars().count() > width {
            let mut t: String = s.chars().take(width.saturating_sub(1)).collect();
            t.push('~');
            t
        } else {
            s
        }
    };
    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|r| r.iter().map(|c| cut(c)).collect())
        .collect();
    let mut w: Vec<usize> = cols.iter().map(|c| c.chars().count()).collect();
    for r in &cells {
        for (i, c) in r.iter().enumerate() {
            w[i] = w[i].max(c.chars().count());
        }
    }
    let mut out = String::new();
    let line = |out: &mut String, r: &[String]| {
        let parts: Vec<String> = r
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{c:<width$}", width = w[i]))
            .collect();
        out.push_str(parts.join(" | ").trim_end());
        out.push('\n');
    };
    line(&mut out, cols);
    out.push_str(
        &w.iter()
            .map(|n| "-".repeat(*n))
            .collect::<Vec<_>>()
            .join("-+-"),
    );
    out.push('\n');
    for r in &cells {
        line(&mut out, r);
    }
    out
}

/// Render rows as CSV (RFC 4180 quoting).
#[must_use]
pub fn csv(cols: &[String], rows: &[Vec<String>]) -> String {
    let q = |s: &str| {
        if s.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    };
    let mut out = String::new();
    out.push_str(&cols.iter().map(|c| q(c)).collect::<Vec<_>>().join(","));
    out.push('\n');
    for r in rows {
        out.push_str(&r.iter().map(|c| q(c)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

fn scalar(conn: &Connection, sql: &str) -> Res<String> {
    let (_, r) = rows(conn, sql, 0)?;
    Ok(r.first()
        .and_then(|r| r.first())
        .cloned()
        .unwrap_or_default())
}

fn section(out: &mut String, conn: &Connection, title: &str, sql: &str) -> Res<()> {
    let (c, r) = rows(conn, sql, 32)?;
    let _ = writeln!(out, "\n## {title}\n");
    out.push_str(&table(&c, &r, 60));
    Ok(())
}

/// The `stats` report.
///
/// # Errors
/// On SQL error.
pub fn stats(conn: &Connection) -> Res<String> {
    let mut out = String::new();
    let _ = writeln!(out, "# dereth-pcap stats");
    let _ = writeln!(
        out,
        "\nschema {} | last ingest {} | server ports {} | roots {}",
        scalar(conn, "SELECT value FROM meta WHERE key='schema_version'")?,
        scalar(conn, "SELECT value FROM meta WHERE key='last_ingest'")?,
        scalar(conn, "SELECT value FROM meta WHERE key='server_ports'")?,
        scalar(conn, "SELECT value FROM meta WHERE key='roots'")?,
    );
    section(
        &mut out,
        conn,
        "Files by kind and status",
        "SELECT kind, status, member = '' AS top_level, count(*) AS files, sum(size) AS bytes
         FROM file GROUP BY 1, 2, 3 ORDER BY 4 DESC",
    )?;
    section(
        &mut out,
        conn,
        "Captures",
        "SELECT format, count(*) AS captures, sum(size) AS bytes, sum(records) AS records,
                sum(game) AS game_datagrams, sum(non_game) AS non_game, sum(truncated) AS truncated,
                sum(ip_fragments) AS ip_frags, sum(truncated_tail) AS cut_tail,
                sum(error IS NOT NULL) AS with_error, sum(sessions) AS sessions
         FROM capture GROUP BY format",
    )?;
    section(
        &mut out,
        conn,
        "Sessions by game (evidence: messages by the form of their first dword)",
        GAMES_SQL,
    )?;
    section(
        &mut out,
        conn,
        "Sessions",
        "SELECT game, count(*) AS sessions, sum(partial) AS partial, sum(1 - partial) AS with_handshake,
                sum(login) AS with_login, sum(world_login) AS with_world_login,
                sum(referral) AS with_referral, sum(disconnect) AS with_disconnect,
                round(sum(duration) / 3600.0, 1) AS hours,
                sum(packets_c2s + packets_s2c) AS packets, sum(parse_errors) AS parse_errors,
                sum(duplicates) AS duplicates, sum(orphans_head) AS orphans_head,
                sum(orphans_tail) AS orphans_tail, sum(orphans_mid) AS orphans_mid
         FROM session GROUP BY game ORDER BY game",
    )?;
    section(
        &mut out,
        conn,
        "Largest sessions (cite a session by `session_key`: `session_id` is renumbered by a rebuild)",
        "SELECT session_key, session_id, game, partial, messages_c2s + messages_s2c AS messages,
                round(duration / 60.0, 1) AS minutes, file, member
         FROM v_session ORDER BY messages DESC, session_key LIMIT 10",
    )?;
    section(
        &mut out,
        conn,
        "Client versions",
        "SELECT game, coalesce(version, '(none captured)') AS version, count(*) AS sessions FROM session
         GROUP BY 1, 2 ORDER BY 1, 3 DESC",
    )?;
    section(
        &mut out,
        conn,
        "Checksums (reported, never required)",
        "SELECT game, sum(cks_checked_c2s) AS checked_c2s, sum(cks_ok_c2s) AS ok_c2s,
                sum(cks_unchecked_c2s) AS unchecked_c2s, sum(desynced_after_c2s IS NOT NULL) AS desynced_c2s,
                sum(cks_checked_s2c) AS checked_s2c, sum(cks_ok_s2c) AS ok_s2c,
                sum(cks_unchecked_s2c) AS unchecked_s2c, sum(desynced_after_s2c IS NOT NULL) AS desynced_s2c
         FROM session GROUP BY game ORDER BY game",
    )?;
    section(
        &mut out,
        conn,
        "Messages by game, direction and decode status",
        STATUS_SQL,
    )?;
    for dir in ["s2c", "c2s"] {
        section(
            &mut out,
            conn,
            &format!("Top 25 AC1 types, {dir} (sessions of game ac1)"),
            &format!(
                "SELECT printf('0x%04X', m.mtype) AS type, o.name, count(*) AS messages,
                        sum(m.status = 'ok') AS ok, sum(m.status = 'error') AS error,
                        sum(m.status = 'unknown') AS unknown, count(DISTINCT m.session_id) AS sessions
                 FROM message m JOIN session s ON s.session_id = m.session_id
                 LEFT JOIN opcode o ON o.code = m.mtype
                 WHERE m.dir = '{dir}' AND s.game = 'ac1' GROUP BY m.mtype ORDER BY 3 DESC LIMIT 25"
            ),
        )?;
    }
    section(
        &mut out,
        conn,
        "AC1 decode failures by type and error (sessions decoded as AC1: game ac1 or unknown)",
        "SELECT m.dir, printf('0x%04X', m.mtype) AS type, o.name, m.error_kind, count(*) AS messages,
                count(DISTINCT m.session_id) AS sessions
         FROM message m JOIN session s ON s.session_id = m.session_id
         LEFT JOIN opcode o ON o.code = m.mtype
         WHERE m.status = 'error' AND s.game <> 'ac2'
         GROUP BY m.dir, m.mtype, m.error_kind ORDER BY 5 DESC LIMIT 40",
    )?;
    section(
        &mut out,
        conn,
        "AC1 unknown types (no codec in this direction; sessions decoded as AC1)",
        "SELECT m.dir, printf('0x%04X', m.mtype) AS type, o.name, o.direction AS table_dir,
                count(*) AS messages, count(DISTINCT m.session_id) AS sessions
         FROM message m JOIN session s ON s.session_id = m.session_id
         LEFT JOIN opcode o ON o.code = m.mtype
         WHERE m.status IN ('unknown', 'short') AND s.game <> 'ac2'
         GROUP BY m.dir, m.mtype ORDER BY 5 DESC LIMIT 40",
    )?;
    section(
        &mut out,
        conn,
        "AC2 census (top 40 types, both directions; status ac2)",
        &ac2_census_sql(40),
    )?;
    Ok(out)
}

/// Sessions per game, with the evidence the game was decided on.
const GAMES_SQL: &str = "SELECT game, count(*) AS sessions, sum(partial) AS partial,
        sum(game_ac1) AS form_ac1, sum(game_ac2) AS form_ac2, sum(game_other) AS form_other,
        sum(messages_c2s + messages_s2c) AS messages, round(sum(duration) / 3600.0, 1) AS hours,
        count(DISTINCT capture_id) AS captures
     FROM session GROUP BY game ORDER BY game";

/// Messages per game, direction and status, with the share each status has of its game and
/// direction: the AC1 decode rate is the `ok` row of game `ac1`.
const STATUS_SQL: &str = "SELECT game, dir, status, messages, types, bytes,
        round(100.0 * messages / sum(messages) OVER (PARTITION BY game, dir), 2) AS pct
     FROM (SELECT s.game, m.dir, m.status, count(*) AS messages, count(DISTINCT m.mtype) AS types,
                  sum(m.len) AS bytes
           FROM message m JOIN session s ON s.session_id = m.session_id
           GROUP BY 1, 2, 3)
     ORDER BY 1, 2, 4 DESC";

/// The AC2 message types: `mtype` is the full first dword (`0x0001_xxxx`), `ac2_type` its low
/// u16, the AC2 type number.
fn ac2_census_sql(limit: usize) -> String {
    format!(
        "SELECT m.dir, printf('0x%04X', m.mtype) AS mtype, printf('0x%04X', m.mtype & 0xFFFF) AS ac2_type,
                count(*) AS messages, count(DISTINCT m.session_id) AS sessions,
                min(m.len) AS len_min, max(m.len) AS len_max
         FROM message m WHERE m.status = 'ac2'
         GROUP BY m.dir, m.mtype ORDER BY 4 DESC, 1, 2 LIMIT {limit}"
    )
}

/// The hex around `offset`, 16 bytes a line, with the failing byte bracketed.
#[must_use]
pub fn hex_around(raw: &[u8], offset: usize, before: usize, after: usize) -> String {
    let start = offset.saturating_sub(before) / 16 * 16;
    let end = (offset + after).min(raw.len());
    let mut out = String::new();
    let mut line = start;
    while line < end.max(start + 1) && line < raw.len().max(1) {
        let _ = write!(out, "{line:06x}:");
        for (i, b) in raw
            .iter()
            .enumerate()
            .take((line + 16).min(raw.len()))
            .skip(line)
        {
            if i == offset {
                let _ = write!(out, "[{b:02x}]");
            } else if i == offset + 1 {
                let _ = write!(out, "{b:02x}");
            } else {
                let _ = write!(out, " {b:02x}");
            }
        }
        if offset >= raw.len() && line + 16 > offset {
            out.push_str(" []<- end");
        }
        out.push('\n');
        line += 16;
    }
    out
}

/// The triage report as markdown.
///
/// # Errors
/// On SQL error.
pub fn triage(conn: &Connection, examples: usize) -> Res<String> {
    let t = Instant::now();
    let mut out = String::new();
    let _ = writeln!(out, "# Retail pcaps: decode-failure triage");
    let _ = writeln!(
        out,
        "\n*Generated by `dereth-pcap triage` from the index (schema {}, last ingest {}). Regenerate \
         rather than edit. Examples are cited by message key, `session_key#idx` (`session_id` and \
         `message_id` are renumbered by a rebuild). Each group is a (direction, type, error kind); \
         the class column is the default — **1, our bug** — until the evidence shows a \
         protocol difference between client builds (class 2) or a message only the retail \
         servers sent (class 3); the tool's README describes the three. The failure and \
         codec-less lists cover the sessions decoded as AC1 (game `ac1` or `unknown`); Asheron's \
         Call 2 sessions (game `ac2`) are stored undecoded and listed in the AC2 census.*",
        scalar(conn, "SELECT value FROM meta WHERE key='schema_version'")?,
        scalar(conn, "SELECT value FROM meta WHERE key='last_ingest'")?,
    );
    section_md(&mut out, conn, "Sessions by game", GAMES_SQL)?;
    let totals = rows(
        conn,
        "SELECT s.game, m.dir, sum(m.status = 'ok'), sum(m.status = 'error'), sum(m.status = 'unknown'),
                sum(m.status = 'short'), sum(m.status = 'ac2'), count(*),
                printf('%.2f', 100.0 * sum(m.status = 'ok') / count(*))
         FROM message m JOIN session s ON s.session_id = m.session_id
         GROUP BY 1, 2 ORDER BY 1, 2",
        0,
    )?;
    let _ = writeln!(out, "\n## Totals by game\n");
    let _ = writeln!(
        out,
        "The AC1 decode rate is `ok %` on the `ac1` rows.\n\n\
         | game | dir | ok | error | unknown | short | ac2 | all | ok % |\n|---|---|---|---|---|---|---|---|---|"
    );
    for r in &totals.1 {
        let _ = writeln!(out, "| {} |", r.join(" | "));
    }

    let (_, groups) = rows(
        conn,
        "SELECT m.dir, m.mtype, coalesce(o.name, ''), m.error_kind, count(*), count(DISTINCT m.session_id),
                sum(s.partial = 0), group_concat(DISTINCT k.name), min(m.len), max(m.len)
         FROM message m JOIN session s ON s.session_id = m.session_id
         LEFT JOIN opcode o ON o.code = m.mtype LEFT JOIN codec k ON k.codec_id = m.codec_id
         WHERE m.status = 'error' AND s.game <> 'ac2' GROUP BY m.dir, m.mtype, m.error_kind ORDER BY 5 DESC",
        0,
    )?;
    let _ = writeln!(out, "\n## Decode failures ({} groups)\n", groups.len());
    let _ = writeln!(
        out,
        "| # | dir | type | name | error kind | messages | sessions | non-partial msgs | codec | len min–max | class |\n\
         |---|---|---|---|---|---|---|---|---|---|---|"
    );
    for (i, g) in groups.iter().enumerate() {
        let ty: u32 = g[1].parse().unwrap_or(0);
        let _ = writeln!(
            out,
            "| {} | {} | 0x{ty:04X} | {} | {} | {} | {} | {} | `{}` | {}–{} | 1 (default) |",
            i + 1,
            g[0],
            g[2],
            g[3],
            g[4],
            g[5],
            g[6],
            g[7],
            g[8],
            g[9]
        );
    }
    for (i, g) in groups.iter().enumerate() {
        let ty: u32 = g[1].parse().unwrap_or(0);
        let _ = writeln!(
            out,
            "\n### {}. {} 0x{ty:04X} {} — {}\n",
            i + 1,
            g[0],
            g[2],
            g[3]
        );
        let kind = g[3].replace('\'', "''");
        let sql = format!(
            "SELECT m.message_id, m.session_id, m.idx, c.file, c.member, s.partial, m.len, m.error_offset,
                    m.error, m.raw, s.session_key, min(m.idx)
             FROM message m JOIN session s ON s.session_id = m.session_id
             JOIN capture c ON c.capture_id = s.capture_id
             WHERE m.status = 'error' AND s.game <> 'ac2' AND m.dir = '{}' AND m.mtype = {}
                   AND m.error_kind = '{kind}'
             GROUP BY m.session_id ORDER BY s.partial, s.session_key LIMIT {examples}",
            g[0], g[1]
        );
        let mut st = conn.prepare(&sql).map_err(e)?;
        let mut q = st.query([]).map_err(e)?;
        let mut first = true;
        while let Some(r) = q.next().map_err(e)? {
            let mid: i64 = r.get(0).map_err(e)?;
            let sid: i64 = r.get(1).map_err(e)?;
            let idx: i64 = r.get(2).map_err(e)?;
            let file: String = r.get(3).map_err(e)?;
            let member: String = r.get(4).map_err(e)?;
            let partial: bool = r.get(5).map_err(e)?;
            let len: i64 = r.get(6).map_err(e)?;
            let off: i64 = r.get(7).map_err(e)?;
            let err: String = r.get(8).map_err(e)?;
            let raw: Vec<u8> = r.get(9).map_err(e)?;
            let key: String = r.get(10).map_err(e)?;
            let place = if member.is_empty() {
                file
            } else {
                format!("{file} :: {member}")
            };
            let _ = writeln!(
                out,
                "- `{key}#{idx}` (session_id {sid}, message_id {mid}{}, {len} bytes) in `{place}`: {err}",
                if partial { ", partial" } else { "" }
            );
            if first {
                let o = usize::try_from(off).unwrap_or(0);
                let _ = writeln!(out, "\n```text\n{}```", hex_around(&raw, o, 48, 32));
                first = false;
            }
        }
    }

    let (_, unknown) = rows(
        conn,
        "SELECT m.dir, m.mtype, coalesce(o.name, ''), coalesce(o.direction, ''), m.status, count(*),
                count(DISTINCT m.session_id), sum(s.partial = 0), min(m.message_id)
         FROM message m JOIN session s ON s.session_id = m.session_id
         LEFT JOIN opcode o ON o.code = m.mtype
         WHERE m.status IN ('unknown', 'short') AND s.game <> 'ac2'
         GROUP BY m.dir, m.mtype, m.status ORDER BY 6 DESC",
        0,
    )?;
    let _ = writeln!(
        out,
        "\n## Types with no codec in their direction ({} groups)\n",
        unknown.len()
    );
    let _ = writeln!(
        out,
        "`table dir` is the master opcode table's direction for the value; empty means the table \
         does not know it.\n\n| dir | type | name | table dir | status | messages | sessions | non-partial msgs | first bytes |\n\
         |---|---|---|---|---|---|---|---|---|"
    );
    for u in &unknown {
        let ty: u32 = u[1].parse().unwrap_or(0);
        let raw: Vec<u8> = conn
            .query_row(
                "SELECT raw FROM message WHERE message_id = ?1",
                [&u[8]],
                |r| r.get(0),
            )
            .map_err(e)?;
        let _ = writeln!(
            out,
            "| {} | 0x{ty:04X} | {} | {} | {} | {} | {} | {} | `{}` |",
            u[0],
            u[2],
            u[3],
            u[4],
            u[5],
            u[6],
            u[7],
            hex(&raw[..raw.len().min(24)])
        );
    }

    section_md(
        &mut out,
        conn,
        "AC2 census (every type, both directions; status ac2)",
        &ac2_census_sql(1000),
    )?;
    section_md(
        &mut out,
        conn,
        "Transport quality (every game: the transport is shared)",
        "SELECT sum(parse_errors) AS parse_errors, sum(duplicates) AS duplicates,
                sum(refused_fragments) AS refused_fragments, sum(orphans_head) AS orphans_head,
                sum(orphans_tail) AS orphans_tail, sum(orphans_mid) AS orphans_mid,
                sum(orphans_mid > 0) AS sessions_with_mid_orphans FROM session",
    )?;
    section_md(
        &mut out,
        conn,
        "Datagrams dereth-transport refused to parse, by reason",
        "SELECT dir, parse_error, count(*) AS datagrams, count(DISTINCT session_id) AS sessions
         FROM packet WHERE parse_error IS NOT NULL GROUP BY 1, 2 ORDER BY 3 DESC LIMIT 30",
    )?;
    let _ = writeln!(
        out,
        "\n*Report built in {:.1} s.*",
        t.elapsed().as_secs_f64()
    );
    Ok(out)
}

fn section_md(out: &mut String, conn: &Connection, title: &str, sql: &str) -> Res<()> {
    let (c, r) = rows(conn, sql, 32)?;
    let _ = writeln!(out, "\n## {title}\n");
    let _ = writeln!(out, "| {} |", c.join(" | "));
    let _ = writeln!(out, "|{}", "---|".repeat(c.len()));
    for row in &r {
        let _ = writeln!(
            out,
            "| {} |",
            row.iter()
                .map(|s| s.replace('|', "\\|"))
                .collect::<Vec<_>>()
                .join(" | ")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quotes_what_needs_quoting() {
        let c = vec!["a".to_string(), "b".to_string()];
        let r = vec![vec!["x,y".to_string(), "say \"hi\"".to_string()]];
        assert_eq!(csv(&c, &r), "a,b\n\"x,y\",\"say \"\"hi\"\"\"\n");
    }

    #[test]
    fn the_hex_dump_brackets_the_failing_byte() {
        let raw: Vec<u8> = (0..40).collect();
        let h = hex_around(&raw, 20, 4, 4);
        assert!(h.contains("[14]"), "{h}");
        assert!(h.starts_with("000010:"), "{h}");
        let end = hex_around(&raw, 40, 8, 4);
        assert!(end.contains("<- end"), "{end}");
    }
}
