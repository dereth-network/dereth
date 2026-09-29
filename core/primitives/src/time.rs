//! The two clocks, as separate types so they cannot be mixed.
//!
//! The original keeps both and they have different reader sets: [`LocalTime`] is purely local and
//! drives resource ageing, UV animation and round-trip timing; [`ServerTime`] is synchronized with
//! the server and drives physics, gameplay and the network. Conflating them is a class of bug a
//! newtype removes for free, matching the client's own time arithmetic.

/// Client-local, monotonic seconds. Never corrected against the server.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct LocalTime(pub f64);

/// Server-synchronised seconds. Corrected forward by an offset; a backward correction rewinds
/// `LocalTime` instead, which is why these are different types.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct ServerTime(pub f64);

impl LocalTime {
    #[inline]
    #[must_use]
    pub fn seconds_since(self, earlier: Self) -> f64 {
        self.0 - earlier.0
    }
}

impl ServerTime {
    #[inline]
    #[must_use]
    pub fn seconds_since(self, earlier: Self) -> f64 {
        self.0 - earlier.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_clocks_do_not_mix() {
        let a = LocalTime(10.0);
        let b = LocalTime(4.0);
        assert_eq!(a.seconds_since(b), 6.0);
        // `a.seconds_since(ServerTime(4.0))` would not compile, which is the point.
        let s = ServerTime(10.0);
        assert_eq!(s.seconds_since(ServerTime(4.0)), 6.0);
    }
}
