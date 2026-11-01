//! A recorded game login as a server double plays it: the connections, and every message, referral
//! and world switch in recorded order.
//!
//! A script is built from the capture index ([`super::recorded`]) or written by hand for a test. Its
//! text form is a TSV (`#` lines are comments), times in seconds from the game login:
//!
//! ```text
//! meta      key  value
//! conn      n  kind(login|world)  server_port  server_id  iteration  net_id  server_time  t_request  t_connect  [switch|-]
//! msg       t  conn  dir(c2s|s2c)  queue  blob_id_hex  type_hex  raw_hex
//! referral  t  from_conn  to_conn|-
//! switch    t  conn
//! end       t  conn  dir
//! silent    t  conn
//! ```
//!
//! A referral names the connection it led to: a new one (the client logged in to that world server)
//! or one the client already had (it only switched back to it, and a `switch` row follows).

/// One recorded connection.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnSpec {
    pub n: usize,
    /// The login connection (opened by the game login) or a world connection (by a referral).
    pub login: bool,
    /// The recorded server's even port.
    pub server_port: u16,
    /// The id the server puts in its headers (the client's slot for it).
    pub server_id: u16,
    pub iteration: u16,
    /// The id the server gives the client for its headers.
    pub net_id: u32,
    /// The server's clock when the connection came up, seconds.
    pub server_time: f64,
    /// When the client asked to connect, and when the connection came up.
    pub t_request: f64,
    pub t_connect: f64,
    /// Whether the server made the connection the client's current world as it came up (a world
    /// server's first packet is then its switch).
    pub switched: bool,
}

/// One recorded message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Msg {
    pub conn: usize,
    pub c2s: bool,
    pub queue: u16,
    pub blob_id: u64,
    pub mtype: u32,
    pub raw: Vec<u8>,
}

/// One row of the timeline.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Msg {
        t: f64,
        msg: Msg,
    },
    /// A referral sent on `from`; `to` is the connection it led to, if the client followed it.
    Referral {
        t: f64,
        from: usize,
        to: Option<usize>,
    },
    /// The server made `conn` the client's current world again (a switch back to a world server
    /// the client was still connected to).
    Switch {
        t: f64,
        conn: usize,
    },
    /// The recorded connection ended (a goodbye from the client or the server).
    End {
        t: f64,
        conn: usize,
        c2s: bool,
    },
    /// The recorded server sent nothing more on the connection after this.
    Silent {
        t: f64,
        conn: usize,
    },
}

impl Event {
    #[must_use]
    pub fn t(&self) -> f64 {
        match self {
            Event::Msg { t, .. }
            | Event::Referral { t, .. }
            | Event::Switch { t, .. }
            | Event::End { t, .. }
            | Event::Silent { t, .. } => *t,
        }
    }

    /// The connection the event happens on.
    #[must_use]
    pub fn conn(&self) -> usize {
        match self {
            Event::Msg { msg, .. } => msg.conn,
            Event::Referral { from, .. } => *from,
            Event::Switch { conn, .. } | Event::End { conn, .. } | Event::Silent { conn, .. } => {
                *conn
            }
        }
    }
}

/// A whole script.
#[derive(Debug, Clone, Default)]
pub struct Script {
    pub meta: Vec<(String, String)>,
    pub conns: Vec<ConnSpec>,
    pub events: Vec<Event>,
}

/// Why a script could not be read.
#[derive(Debug)]
pub struct ScriptError(pub String);

impl core::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ScriptError {}

fn hex_bytes(s: &str) -> Result<Vec<u8>, ScriptError> {
    if !s.len().is_multiple_of(2) {
        return Err(ScriptError(format!("odd hex length {}", s.len())));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| ScriptError(e.to_string())))
        .collect()
}

fn int(s: &str) -> Result<u64, ScriptError> {
    let r = match s.strip_prefix("0x") {
        Some(h) => u64::from_str_radix(h, 16),
        None => s.parse(),
    };
    r.map_err(|e| ScriptError(format!("{s:?}: {e}")))
}

fn float(s: &str) -> Result<f64, ScriptError> {
    s.parse().map_err(|e| ScriptError(format!("{s:?}: {e}")))
}

fn narrow<T: TryFrom<u64>>(v: u64, what: &str) -> Result<T, ScriptError> {
    T::try_from(v).map_err(|_| ScriptError(format!("{what} {v} out of range")))
}

/// The type word of a message, or 0 when it is shorter than four bytes.
#[must_use]
pub fn message_type(raw: &[u8]) -> u32 {
    raw.get(..4)
        .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

impl Script {
    /// Parse a script's text.
    ///
    /// # Errors
    /// A row does not parse, or names a connection that is not declared.
    pub fn parse(text: &str) -> Result<Self, ScriptError> {
        let mut s = Script::default();
        for (ln, line) in text.lines().enumerate() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let f: Vec<&str> = line.split('\t').collect();
            let at = |m: ScriptError| ScriptError(format!("line {}: {}", ln + 1, m.0));
            let need = |n: usize| {
                if f.len() < n {
                    Err(ScriptError(format!(
                        "line {}: {} fields, need {n}",
                        ln + 1,
                        f.len()
                    )))
                } else {
                    Ok(())
                }
            };
            match f[0] {
                "meta" => {
                    need(3)?;
                    s.meta.push((f[1].to_owned(), f[2].to_owned()));
                }
                "conn" => {
                    need(10)?;
                    s.conns.push(ConnSpec {
                        n: narrow(int(f[1]).map_err(at)?, "conn")?,
                        login: f[2] == "login",
                        server_port: narrow(int(f[3]).map_err(at)?, "port")?,
                        server_id: narrow(int(f[4]).map_err(at)?, "server id")?,
                        iteration: narrow(int(f[5]).map_err(at)?, "iteration")?,
                        net_id: narrow(int(f[6]).map_err(at)?, "net id")?,
                        server_time: float(f[7]).map_err(at)?,
                        t_request: float(f[8]).map_err(at)?,
                        t_connect: float(f[9]).map_err(at)?,
                        switched: f.get(10).is_none_or(|v| *v != "-"),
                    });
                }
                "msg" => {
                    need(8)?;
                    let raw = hex_bytes(f[7]).map_err(at)?;
                    s.events.push(Event::Msg {
                        t: float(f[1]).map_err(at)?,
                        msg: Msg {
                            conn: narrow(int(f[2]).map_err(at)?, "conn")?,
                            c2s: f[3] == "c2s",
                            queue: narrow(int(f[4]).map_err(at)?, "queue")?,
                            blob_id: int(&format!("0x{}", f[5])).map_err(at)?,
                            mtype: message_type(&raw),
                            raw,
                        },
                    });
                }
                "referral" => {
                    need(4)?;
                    s.events.push(Event::Referral {
                        t: float(f[1]).map_err(at)?,
                        from: narrow(int(f[2]).map_err(at)?, "conn")?,
                        to: if f[3] == "-" {
                            None
                        } else {
                            Some(narrow(int(f[3]).map_err(at)?, "conn")?)
                        },
                    });
                }
                "switch" => {
                    need(3)?;
                    s.events.push(Event::Switch {
                        t: float(f[1]).map_err(at)?,
                        conn: narrow(int(f[2]).map_err(at)?, "conn")?,
                    });
                }
                "silent" => {
                    need(3)?;
                    s.events.push(Event::Silent {
                        t: float(f[1]).map_err(at)?,
                        conn: narrow(int(f[2]).map_err(at)?, "conn")?,
                    });
                }
                "end" => {
                    need(4)?;
                    s.events.push(Event::End {
                        t: float(f[1]).map_err(at)?,
                        conn: narrow(int(f[2]).map_err(at)?, "conn")?,
                        c2s: f[3] == "c2s",
                    });
                }
                other => {
                    return Err(ScriptError(format!(
                        "line {}: unknown row {other:?}",
                        ln + 1
                    )))
                }
            }
        }
        s.check()?;
        Ok(s)
    }

    /// The rules every script keeps: connections declared in order, the login connection first,
    /// every event on a declared connection.
    ///
    /// # Errors
    /// A rule is broken.
    pub fn check(&self) -> Result<(), ScriptError> {
        for (i, c) in self.conns.iter().enumerate() {
            if c.n != i {
                return Err(ScriptError(format!(
                    "connection {} declared at position {i}",
                    c.n
                )));
            }
        }
        if self.conns.first().is_none_or(|c| !c.login) {
            return Err(ScriptError(
                "the first connection must be the login connection".into(),
            ));
        }
        let n = self.conns.len();
        for e in &self.events {
            let bad = e.conn() >= n || matches!(e, Event::Referral { to: Some(t), .. } if *t >= n);
            if bad {
                return Err(ScriptError(format!(
                    "event at t={} names an undeclared connection",
                    e.t()
                )));
            }
        }
        Ok(())
    }

    /// A `meta` value.
    #[must_use]
    pub fn meta(&self, key: &str) -> Option<&str> {
        self.meta
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}
