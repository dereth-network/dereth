//! Rate limits: a token bucket on a caller's clock, and the hub's default limits.
//!
//! Nothing here reads a clock. Each call passes the time, in seconds on any monotonic scale the
//! caller keeps, so the same bucket runs in a hub, a world or a test on virtual time.

use serde::{Deserialize, Serialize};

/// A token bucket: up to `burst` messages at once, refilled at `per_sec` a second.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bucket {
    pub burst: f64,
    pub per_sec: f64,
    tokens: f64,
    at: f64,
}

impl Bucket {
    /// A full bucket at time `now`.
    #[must_use]
    pub fn new(rate: Rate, now: f64) -> Self {
        Self {
            burst: rate.burst,
            per_sec: rate.per_sec,
            tokens: rate.burst,
            at: now,
        }
    }

    /// Takes one token at `now`; false (and nothing taken) when the bucket is empty.
    pub fn take(&mut self, now: f64) -> bool {
        self.refill(now);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn refill(&mut self, now: f64) {
        if now > self.at {
            self.tokens = (self.tokens + (now - self.at) * self.per_sec).min(self.burst);
            self.at = now;
        }
    }

    /// Whether a token is there to take at `now` (nothing is taken).
    #[must_use]
    pub fn has(&self, now: f64) -> bool {
        let mut b = *self;
        b.refill(now);
        b.tokens >= 1.0
    }

    /// How many seconds from `now` until a token is there to take: none while one is.
    #[must_use]
    pub fn wait(&self, now: f64) -> f64 {
        let mut b = *self;
        b.refill(now);
        if b.tokens >= 1.0 {
            0.0
        } else if b.per_sec > 0.0 {
            (1.0 - b.tokens) / b.per_sec
        } else {
            f64::INFINITY
        }
    }

    /// Whether the bucket has refilled completely by `now` (so it can be forgotten).
    #[must_use]
    pub fn full_at(&self, now: f64) -> bool {
        let mut b = *self;
        b.refill(now);
        b.tokens >= b.burst
    }
}

/// A rate: a burst and a sustained rate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rate {
    pub burst: f64,
    pub per_sec: f64,
}

/// The hub's limits. [`Limits::DEFAULT`] are the values the specification gives; an operator may
/// tighten them. A world reads every field it knows from `welcome`; a field the hub leaves out
/// takes its default, and one it does not know is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Limits {
    /// Tells a character sends (to characters and channels together).
    pub tells_per_character: Rate,
    /// Room lines a character sends.
    pub room_lines_per_character: Rate,
    /// Everything one world sends, all characters together.
    pub messages_per_world: Rate,
    /// Room lines one world sends, all its characters together. The hub knows a character only
    /// by the name its world gives it, so the per-character limits hold only for an honest
    /// world; this one holds for any.
    #[serde(default = "default_room_lines_per_world")]
    pub room_lines_per_world: Rate,
    /// Tells one world sends one recipient, all its characters together (see
    /// [`Limits::room_lines_per_world`]).
    #[serde(default = "default_tells_per_recipient")]
    pub tells_per_recipient: Rate,
    /// The loop guard: when two correspondents have each sent the other more than this many
    /// tells within [`Limits::pair_window_secs`], tells between them are refused for
    /// [`Limits::pair_block_secs`]. It catches two automated answerers replying to each other.
    pub pair_burst: u32,
    pub pair_window_secs: f64,
    pub pair_block_secs: f64,
    /// How long the hub holds a tell for a federated world whose link is down, waiting for it to
    /// come back, before it answers that the world is unavailable.
    pub hold_secs: f64,
    /// How long a world waits for a tell's result before it gives up on it.
    pub result_timeout_secs: f64,
}

impl Limits {
    /// Tells one world sends any one other world, all its characters together: a quarter of
    /// [`Limits::messages_per_world`]. Each is owed an answer that counts against the target's
    /// own pace, so no one world can send another more than that world can answer. Not a field
    /// of its own: it follows `messages_per_world`.
    #[must_use]
    pub fn tells_per_world_pair(&self) -> Rate {
        Rate {
            burst: (self.messages_per_world.burst / 4.0).max(1.0),
            per_sec: self.messages_per_world.per_sec / 4.0,
        }
    }

    /// Watches one world sends any one other world, all its characters together: the same share
    /// as [`Limits::tells_per_world_pair`], for the same reason (each is owed a `presence` answer).
    #[must_use]
    pub fn watches_per_world_pair(&self) -> Rate {
        self.tells_per_world_pair()
    }

    /// The specification's values.
    pub const DEFAULT: Self = Self {
        tells_per_character: Rate {
            burst: 10.0,
            per_sec: 0.5,
        },
        room_lines_per_character: Rate {
            burst: 3.0,
            per_sec: 0.2,
        },
        messages_per_world: Rate {
            burst: 200.0,
            per_sec: 50.0,
        },
        room_lines_per_world: ROOM_LINES_PER_WORLD,
        tells_per_recipient: TELLS_PER_RECIPIENT,
        pair_burst: 20,
        pair_window_secs: 60.0,
        pair_block_secs: 300.0,
        hold_secs: 60.0,
        // Longer than the hub's hold and the target's answer window together (60 + 30), so a
        // tell the hub may still deliver is never given up on first.
        result_timeout_secs: 100.0,
    };
}

const ROOM_LINES_PER_WORLD: Rate = Rate {
    burst: 10.0,
    per_sec: 1.0,
};

const TELLS_PER_RECIPIENT: Rate = Rate {
    burst: 20.0,
    per_sec: 1.0,
};

fn default_room_lines_per_world() -> Rate {
    ROOM_LINES_PER_WORLD
}

fn default_tells_per_recipient() -> Rate {
    TELLS_PER_RECIPIENT
}

impl Default for Limits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec: rate limits: a spent bucket says how long until it holds a token again.
    #[test]
    fn a_spent_bucket_says_how_long_until_its_next_token() {
        let mut b = Bucket::new(
            Rate {
                burst: 2.0,
                per_sec: 0.25,
            },
            0.0,
        );
        assert!(b.wait(0.0) == 0.0);
        assert!(b.take(0.0) && b.take(0.0) && !b.take(0.0));
        assert!((b.wait(0.0) - 4.0).abs() < 1e-9);
        assert!((b.wait(3.0) - 1.0).abs() < 1e-9);
        assert!(b.wait(4.0) == 0.0);
    }

    /// Spec: rate limits: a bucket allows its burst at once, then its rate.
    #[test]
    fn a_bucket_allows_its_burst_then_refills_at_its_rate() {
        let mut b = Bucket::new(
            Rate {
                burst: 3.0,
                per_sec: 0.5,
            },
            0.0,
        );
        assert!(b.take(0.0) && b.take(0.0) && b.take(0.0));
        assert!(!b.take(0.0), "the burst is spent");
        assert!(!b.take(1.9), "not yet a whole token");
        assert!(b.take(2.0), "one token after two seconds");
        assert!(!b.take(2.0));
        assert!(!b.has(2.0) && b.has(4.0), "a look takes nothing");
        assert!(b.full_at(100.0), "it refills to the burst and no further");
        assert!(b.take(100.0) && b.take(100.0) && b.take(100.0) && !b.take(100.0));
    }

    /// Spec: link protocol: a world's give-up on a tell outlasts the hub's hold and answer window.
    #[test]
    fn a_world_waits_longer_for_a_result_than_the_hub_may_take_to_give_one() {
        let l = Limits::DEFAULT;
        assert!(l.result_timeout_secs > l.hold_secs + 30.0);
    }

    /// Spec: link protocol: a limit a hub leaves out of `welcome` takes its default.
    #[test]
    fn a_limit_left_out_takes_its_default() {
        let mut v = serde_json::to_value(Limits::DEFAULT).expect("written");
        let o = v.as_object_mut().expect("an object");
        o.remove("room_lines_per_world");
        o.remove("tells_per_recipient");
        o.insert("a_later_limit".into(), serde_json::json!(7));
        let back: Limits = serde_json::from_value(v).expect("read");
        assert_eq!(back, Limits::DEFAULT);
    }
}
