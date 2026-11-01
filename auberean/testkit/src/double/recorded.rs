//! Scripts built from the capture index: one per recorded game login.
//!
//! A game login is a login connection and every world connection the same client made after it
//! in the same capture, until its next login. The index holds every message both ways and every
//! packet's flags and time, but no packet bytes, so the transport facts are read off the flags:
//!
//! - a connection was asked for by the client's first login or world-login packet, and came up
//!   with the client's acknowledgement of the server's connection request;
//! - a referral is a server packet with the referral flag; it led to the world connection the
//!   client asked for within the next fifteen seconds, or, when it asked for none, to the world
//!   connection a server switch then made current again;
//! - a world switch makes its connection the current world: within two seconds of a world
//!   connection coming up it is the connection's own, later it is a switch back (and a world
//!   connection whose server sent none never became current); each goes out in recorded order;
//! - a world login to a server the client already had a connection to replaced it, so what the
//!   index files under the old connection after the new one was asked for belongs to the new one;
//! - a goodbye is a packet with the disconnect flag, or the server's or the client's error
//!   disconnect (how a server lets go of a world connection the player has left);
//! - a world server that sent nothing more on a connection a minute or more before the capture's
//!   end fell silent there (the client then let go of the connection on its own).
//!
//! What the index cannot say, the script makes up consistently: server ids by port (the login
//! server's and one per world server), iterations rising with each new connection to the same
//! server, client ids by connection, and the server clock from the connection's first position.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{params, Connection};

use super::script::{message_type, ConnSpec, Event, Msg, Script};
use crate::captures::open;

/// Packet flags read here.
mod flag {
    pub const SERVER_SWITCH: i64 = 0x100;
    pub const REFERRAL: i64 = 0x800;
    pub const DISCONNECT: i64 = 0x8000;
    pub const LOGIN_REQUEST: i64 = 0x1_0000;
    pub const WORLD_LOGIN_REQUEST: i64 = 0x2_0000;
    pub const CONNECT_RESPONSE: i64 = 0x8_0000;
    pub const NET_ERROR_DISCONNECT: i64 = 0x20_0000;
}

/// How long after a referral the client's world login (or the switch back) is looked for.
const REFERRAL_WINDOW: f64 = 15.0;

/// How soon after a world connection came up its server's switch counts as the connection's own.
const CONNECT_SWITCH: f64 = 2.0;

/// How long before the capture's end a connection's server must have fallen silent for the script
/// to say so, seconds.
const SILENCE: f64 = 60.0;

/// When the server last sent anything on a session within a window of its times.
fn last_heard_between(
    con: &Connection,
    session: i64,
    from: f64,
    until: f64,
) -> rusqlite::Result<Option<f64>> {
    let (from, until) = (from.max(-1e18), until.min(1e18));
    con.query_row(
        "SELECT max(t) FROM packet WHERE session_id = ?1 AND dir = 's2c' AND t >= ?2 AND t < ?3",
        params![session, from, until],
        |r| r.get::<_, Option<f64>>(0),
    )
}

/// A position message's type, and where its server-clock time sits.
const POSITION: u32 = 0x10086;

/// One recorded session of a game login.
#[derive(Debug, Clone)]
pub struct SessionRef {
    pub id: i64,
    pub key: String,
    pub t_first: f64,
    pub port: u16,
    pub login: bool,
}

/// One recorded game login: its login session and its world sessions, in order.
#[derive(Debug, Clone)]
pub struct GameLogin {
    pub capture: i64,
    /// The capture's file, from the capture root.
    pub file: String,
    pub sessions: Vec<SessionRef>,
}

impl GameLogin {
    /// The login session's key.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.sessions[0].key
    }

    /// World connections.
    #[must_use]
    pub fn world_logins(&self) -> usize {
        self.sessions.len() - 1
    }
}

/// Every game login recorded in a year (`2017`), in capture order.
///
/// # Errors
/// The index does not read.
pub fn game_logins(index: &Path, year: &str) -> rusqlite::Result<Vec<GameLogin>> {
    let con = open(index)?;
    let mut stmt = con.prepare(
        "SELECT s.session_id, s.session_key, s.capture_id, s.t_first, s.server_port, s.login, c.file \
         FROM session s JOIN capture c USING (capture_id) \
         WHERE s.game = 'ac2' AND (s.login = 1 OR s.world_login = 1) AND substr(c.file, 1, 4) = ?1 \
         ORDER BY s.capture_id, s.t_first",
    )?;
    let rows = stmt.query_map(params![year], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, f64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, String>(6)?,
        ))
    })?;
    let mut out: Vec<GameLogin> = Vec::new();
    for row in rows {
        let (id, key, capture, t_first, port, login, file) = row?;
        let s = SessionRef {
            id,
            key,
            t_first,
            port: u16::try_from(port).unwrap_or(0),
            login: login == 1,
        };
        if s.login {
            out.push(GameLogin {
                capture,
                file,
                sessions: vec![s],
            });
        } else if let Some(g) = out.last_mut().filter(|g| g.capture == capture) {
            g.sessions.push(s);
        }
    }
    Ok(out)
}

/// The server id the script gives a port: the login server's, or one per world server.
#[must_use]
pub fn server_id(port: u16) -> u16 {
    if port & !1 == 9000 {
        0xF1
    } else {
        0x0B + (port.saturating_sub(9002) / 2)
    }
}

struct Packet {
    t: f64,
    c2s: bool,
    flags: i64,
}

fn packets(con: &Connection, session: i64) -> rusqlite::Result<Vec<Packet>> {
    let mask = flag::SERVER_SWITCH
        | flag::REFERRAL
        | flag::DISCONNECT
        | flag::LOGIN_REQUEST
        | flag::WORLD_LOGIN_REQUEST
        | flag::CONNECT_RESPONSE
        | flag::NET_ERROR_DISCONNECT;
    let mut stmt = con.prepare(
        "SELECT t, dir, flags FROM packet WHERE session_id = ?1 AND duplicate = 0 AND (flags & ?2) != 0 ORDER BY idx",
    )?;
    let rows = stmt.query_map(params![session, mask], |r| {
        Ok(Packet {
            t: r.get(0)?,
            c2s: r.get::<_, String>(1)? == "c2s",
            flags: r.get(2)?,
        })
    })?;
    rows.collect()
}

/// The flags and size of each connection's first packet from the recorded client (its game login
/// or world login), in the game login's session order.
///
/// # Errors
/// The index does not read.
pub fn first_packets(index: &Path, login: &GameLogin) -> rusqlite::Result<Vec<(u32, u32)>> {
    let con = open(index)?;
    let mut stmt = con.prepare(
        "SELECT flags, len FROM packet WHERE session_id = ?1 AND dir = 'c2s' AND duplicate = 0 \
         ORDER BY idx LIMIT 1",
    )?;
    let mut out = Vec::new();
    for s in &login.sessions {
        let row = stmt.query_row(params![s.id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        });
        if let Ok((flags, len)) = row {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            out.push((flags as u32, len as u32));
        }
    }
    Ok(out)
}

/// The script of one game login.
///
/// # Errors
/// The index does not read.
pub fn script(index: &Path, login: &GameLogin) -> rusqlite::Result<Script> {
    let con = open(index)?;
    let t0 = login.sessions[0].t_first;
    let mut script = Script::default();
    script
        .meta
        .push(("login_session".into(), login.key().to_owned()));
    script.meta.push(("capture".into(), login.file.clone()));
    let mut iterations: BTreeMap<u16, u16> = BTreeMap::new();
    // Per connection: its transport packets (absolute to the login).
    let mut flagged: Vec<Vec<Packet>> = Vec::new();
    let mut msgs: Vec<(f64, usize, Msg)> = Vec::new();
    for (n, s) in login.sessions.iter().enumerate() {
        let base = s.t_first - t0;
        let mut ps = packets(&con, s.id)?;
        for p in &mut ps {
            p.t += base;
        }
        let t_request = ps
            .iter()
            .find(|p| p.c2s && p.flags & (flag::LOGIN_REQUEST | flag::WORLD_LOGIN_REQUEST) != 0)
            .map_or(base, |p| p.t);
        let t_connect = ps
            .iter()
            .find(|p| p.c2s && p.flags & flag::CONNECT_RESPONSE != 0)
            .map_or(t_request + 0.1, |p| p.t);
        let sid = server_id(s.port);
        let it = iterations.entry(sid).or_insert(if s.login { 4 } else { 1 });
        *it += 1;
        let mut stmt = con.prepare(
            "SELECT idx, t, dir, queue, blob_id, raw FROM message WHERE session_id = ?1 AND status = 'ac2' ORDER BY idx",
        )?;
        let rows = stmt.query_map(params![s.id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, f64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Vec<u8>>(5)?,
            ))
        })?;
        let mut server_time = 1.25e8;
        let mut clock_found = s.login;
        for row in rows {
            let (_, t, dir, queue, blob_id, raw) = row?;
            let t = t + base;
            let c2s = dir == "c2s";
            let mtype = message_type(&raw);
            if !clock_found && !c2s && mtype == POSITION {
                if let Some(b) = raw.get(16..24) {
                    let at = f64::from_le_bytes(b.try_into().unwrap_or([0; 8]));
                    if at.is_finite() && at > 0.0 {
                        server_time = at - (t - t_connect).max(0.0);
                        clock_found = true;
                    }
                }
            }
            #[allow(clippy::cast_sign_loss)]
            let m = Msg {
                conn: n,
                c2s,
                queue: u16::try_from(queue).unwrap_or(0),
                blob_id: blob_id as u64,
                mtype,
                raw,
            };
            msgs.push((t, n, m));
        }
        script.conns.push(ConnSpec {
            n,
            login: s.login,
            server_port: s.port & !1,
            server_id: sid,
            iteration: *it,
            net_id: u32::try_from(n + 1).unwrap_or(1).min(255),
            server_time,
            t_request,
            t_connect,
            // The connection's own switch is a timeline event, in order with everything else.
            switched: false,
        });
        flagged.push(ps);
    }

    // A world login to a server the client had a connection to replaced that connection: what the
    // index files under the old connection after the new one was asked for is the new one's.
    let owner = |n: usize, t: f64| -> usize {
        let c = &script.conns[n];
        if c.login {
            return n;
        }
        script
            .conns
            .iter()
            .filter(|d| !d.login && d.server_port == c.server_port && d.n >= n && d.t_request <= t)
            .map(|d| d.n)
            .max()
            .unwrap_or(n)
    };
    let mut moved: Vec<Vec<Packet>> = (0..flagged.len()).map(|_| Vec::new()).collect();
    for (n, ps) in flagged.iter().enumerate() {
        for p in ps {
            let to = owner(n, p.t);
            moved[to].push(Packet {
                t: p.t,
                c2s: p.c2s,
                flags: p.flags,
            });
        }
    }
    for ps in &mut moved {
        ps.sort_by(|a, b| a.t.total_cmp(&b.t));
    }
    let flagged = moved;
    let mut events: Vec<(f64, usize, Event)> = Vec::new();
    let mut order = 0usize;
    for (t, n, mut m) in msgs {
        m.conn = owner(n, t);
        events.push((t, order, Event::Msg { t, msg: m }));
        order += 1;
    }
    // Switches after a connection's first packet are switches back to it.
    let mut switches: Vec<(f64, usize, bool)> = Vec::new();
    for (n, ps) in flagged.iter().enumerate() {
        if script.conns[n].login {
            continue;
        }
        let up = script.conns[n].t_connect;
        for p in ps {
            if !p.c2s && p.flags & flag::SERVER_SWITCH != 0 {
                // The connection's own switch (as it came up) is no referral's.
                switches.push((p.t, n, p.t <= up + CONNECT_SWITCH));
            }
        }
    }
    let mut claimed = vec![false; script.conns.len()];
    for (n, ps) in flagged.iter().enumerate() {
        for p in ps
            .iter()
            .filter(|p| !p.c2s && p.flags & flag::REFERRAL != 0)
        {
            let t = p.t;
            let new_conn = script
                .conns
                .iter()
                .filter(|c| {
                    !c.login
                        && !claimed[c.n]
                        && c.t_request >= t
                        && c.t_request <= t + REFERRAL_WINDOW
                })
                .min_by(|a, b| a.t_request.total_cmp(&b.t_request))
                .map(|c| c.n);
            let to = new_conn.or_else(|| {
                let s = switches
                    .iter_mut()
                    .filter(|(ts, c, used)| {
                        !*used
                            && *ts >= t
                            && *ts <= t + REFERRAL_WINDOW
                            && script.conns[*c].t_connect < t
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))?;
                s.2 = true;
                Some(s.1)
            });
            if let Some(c) = new_conn {
                claimed[c] = true;
            }
            events.push((t, order, Event::Referral { t, from: n, to }));
            order += 1;
        }
    }
    for (t, conn, _) in switches {
        events.push((t, order, Event::Switch { t, conn }));
        order += 1;
    }
    for (n, ps) in flagged.iter().enumerate() {
        for c2s in [true, false] {
            if let Some(p) = ps.iter().find(|p| {
                p.c2s == c2s && p.flags & (flag::DISCONNECT | flag::NET_ERROR_DISCONNECT) != 0
            }) {
                events.push((
                    p.t,
                    order,
                    Event::End {
                        t: p.t,
                        conn: n,
                        c2s,
                    },
                ));
                order += 1;
            }
        }
    }
    // A world connection the server stopped sending on before the capture ended went silent:
    // the client let go of it when it heard nothing more.
    // When each connection last heard from its server: in its own session and in older sessions
    // to the same server, from when it was asked for until a newer one was.
    let mut capture_end: Vec<Option<f64>> = vec![None; script.conns.len()];
    for (m, sm) in login.sessions.iter().enumerate() {
        let base = sm.t_first - t0;
        for (n, c) in script.conns.iter().enumerate().skip(m) {
            if n != m
                && (c.login
                    || script.conns[m].login
                    || c.server_port != script.conns[m].server_port)
            {
                continue;
            }
            let from = if n == m {
                f64::NEG_INFINITY
            } else {
                c.t_request
            };
            let until = script
                .conns
                .iter()
                .filter(|d| !d.login && d.n > n && d.server_port == c.server_port)
                .map(|d| d.t_request)
                .fold(f64::INFINITY, f64::min);
            if let Some(t) = last_heard_between(&con, sm.id, from - base, until - base)? {
                let t = t + base;
                capture_end[n] = Some(capture_end[n].map_or(t, |x: f64| x.max(t)));
            }
        }
    }
    let end = capture_end.iter().copied().flatten().fold(0.0f64, f64::max);
    for (n, last) in capture_end.iter().enumerate() {
        let Some(last) = *last else { continue };
        let said_goodbye = flagged[n]
            .iter()
            .any(|p| !p.c2s && p.flags & flag::NET_ERROR_DISCONNECT != 0);
        if !script.conns[n].login && !said_goodbye && end - last > SILENCE {
            events.push((
                last + 0.001,
                order,
                Event::Silent {
                    t: last + 0.001,
                    conn: n,
                },
            ));
            order += 1;
        }
    }
    events.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    script.events = events.into_iter().map(|(_, _, e)| e).collect();
    Ok(script)
}
