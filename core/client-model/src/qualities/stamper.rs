//! `PropertySequenceGate` — the per-`(StatType, propertyId)` 8-bit sequence gate.
//!
//! The time stamper's update. The sequence gate is per (StatType, propertyId) and 8-bit, and the
//! comparison is the wraparound one, not a plain `>`.
//!
//! The weenie's stamper setup allocates one lazily per object, so an object that
//! never receives a property update carries no per-key state — see [`crate::weenie::Weenie`].
//!
//! The wrap rule itself is [`dereth_protocol::wrap::not_older_u8`]; the protocol crate owns it
//! because the same
//! comparison decides blob ordering. `dereth_client_net::client_session::stamper` holds a second copy for the *dispatch*
//! path. This one is the object's own, which is where the client keeps it.

use dereth_protocol::wrap::not_older_u8;
use std::collections::BTreeMap;

/// One object's timestamp table.
#[derive(Debug, Clone, Default)]
pub struct PropertySequenceGate {
    stamps: BTreeMap<u32, u8>,
    rejected: u32,
}

impl PropertySequenceGate {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Update `key` with the supplied sequence number.
    ///
    /// * key absent → insert and accept;
    /// * key present → accept only when the byte is **not older** under the wrap rule.
    ///
    /// Equal accepts: the client's rule is "not older", not "newer", so a repeated stamp is applied
    /// again rather than dropped.
    pub fn update(&mut self, key: u32, stamp: u8) -> bool {
        match self.stamps.get_mut(&key) {
            None => {
                self.stamps.insert(key, stamp);
                true
            }
            Some(old) => {
                if not_older_u8(stamp, *old) {
                    *old = stamp;
                    true
                } else {
                    self.rejected += 1;
                    false
                }
            }
        }
    }

    /// The stamp currently held for a key.
    #[must_use]
    pub fn stamp(&self, key: u32) -> Option<u8> {
        self.stamps.get(&key).copied()
    }

    /// How many updates this stamper has dropped. The client keeps the same count in a global
    /// and nothing reads it; it is here because a test can.
    #[must_use]
    pub fn rejected(&self) -> u32 {
        self.rejected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qualities::{StatKey, StatType};

    /// Oracle: the recovered qualities behavior §8, transcribing the
    /// stamper's update.
    #[test]
    fn an_out_of_order_sequence_is_rejected_by_the_modulo_256_gate() {
        let mut s = PropertySequenceGate::new();
        let k = StatKey::new(StatType::Int, 12).0;
        assert!(s.update(k, 3), "first stamp always accepts");
        assert!(s.update(k, 4));
        assert!(!s.update(k, 3), "3 is older than 4");
        assert_eq!(s.rejected(), 1);
        assert!(s.update(k, 4), "equal accepts: the rule is not-older");
        assert!(
            !s.update(k, 0xFE),
            "0xFE after 0x04 is six steps *backwards* under the half-range window, not 250 forward \
             — which is exactly what a naive `>` would get wrong in the other direction"
        );

        // Across the wrap, on a counter that has actually walked there.
        let k2 = StatKey::new(StatType::Int, 13).0;
        assert!(s.update(k2, 0xFE));
        assert!(
            s.update(k2, 0x02),
            "0x02 after 0xFE is four steps forward, not 252 back"
        );
        assert!(!s.update(k2, 0xFE));
        assert_eq!(s.stamp(k2), Some(0x02));
    }

    /// Oracle: §8 — the key is `(StatType << 16) | propertyId`, so *skill*, *skill level* and
    /// *skill AC* share one counter while two different stat types on the same property do not.
    #[test]
    fn the_gate_is_per_stat_type_and_property() {
        let mut s = PropertySequenceGate::new();
        let int12 = StatKey::new(StatType::Int, 12).0;
        let bool12 = StatKey::new(StatType::Bool, 12).0;
        let int13 = StatKey::new(StatType::Int, 13).0;
        assert!(s.update(int12, 9));
        assert!(
            s.update(bool12, 1),
            "a different stat type is a different counter"
        );
        assert!(
            s.update(int13, 1),
            "a different property is a different counter"
        );
        assert!(!s.update(int12, 8));
    }
}
