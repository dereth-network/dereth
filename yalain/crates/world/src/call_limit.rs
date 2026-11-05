//! How fast one connection may send game calls.
//!
//! The busiest of the 1,605 client connections recorded in January 2017, among 2.49 million messages,
//! sent 53 in a second, 287 in ten seconds and 905 in a minute; a limit of 200 at once and 20 a second
//! would have turned none of them away, and the limit is ten times that. What is past it is dropped
//! before it is read.
//!
//! This is the server's own rule. Nothing recorded shows the 2017 servers having one, or reaching it.

use std::collections::HashMap;

use yalain_net::{ConnId, LocalTime};

/// Calls a connection may send at once.
pub const BURST: f64 = 2000.0;
/// Calls a connection may send a second, after that.
pub const PER_SECOND: f64 = 200.0;

/// What a connection has left, and what it has lost.
#[derive(Debug, Clone, Copy)]
pub struct Bucket {
    burst: f64,
    per_second: f64,
    left: f64,
    at: f64,
    /// Calls dropped so far.
    pub dropped: u64,
}

impl Bucket {
    /// A connection's first call, under the world's limit.
    #[must_use]
    pub const fn new(now: LocalTime) -> Self {
        Self::with(BURST, PER_SECOND, now)
    }

    /// A connection's first call, under this limit.
    #[must_use]
    pub const fn with(burst: f64, per_second: f64, now: LocalTime) -> Self {
        Self {
            burst,
            per_second,
            left: burst,
            at: now.0,
            dropped: 0,
        }
    }

    /// A call now: may it go on?
    pub fn allow(&mut self, now: LocalTime) -> bool {
        self.left = (self.left + (now.0 - self.at).max(0.0) * self.per_second).min(self.burst);
        self.at = now.0;
        if self.left < 1.0 {
            self.dropped += 1;
            return false;
        }
        self.left -= 1.0;
        true
    }
}

/// Every connection's bucket.
#[derive(Debug, Default)]
pub struct CallLimits {
    by_conn: HashMap<ConnId, Bucket>,
}

impl CallLimits {
    /// A call on a connection now: may it go on? The operator is told the first time one may not.
    pub fn allow(&mut self, conn: ConnId, now: LocalTime) -> bool {
        let b = self.by_conn.entry(conn).or_insert_with(|| Bucket::new(now));
        let ok = b.allow(now);
        if !ok && b.dropped == 1 {
            tracing::warn!(
                ?conn,
                "this client is sending game calls faster than {PER_SECOND:.0} a second: what is \
                 over is dropped"
            );
        }
        ok
    }

    /// How many calls of a connection have been dropped.
    #[must_use]
    pub fn dropped(&self, conn: ConnId) -> u64 {
        self.by_conn.get(&conn).map_or(0, |b| b.dropped)
    }

    /// A connection is gone.
    pub fn closed(&mut self, conn: ConnId) {
        if let Some(b) = self.by_conn.remove(&conn) {
            if b.dropped > 0 {
                tracing::warn!(
                    ?conn,
                    dropped = b.dropped,
                    "a client that sent too fast has gone"
                );
            }
        }
    }
}
