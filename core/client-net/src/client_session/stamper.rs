//! `PropertySequenceGate` — the per-(object, stat-type, property) 8-bit sequence gate.
//!
//! Source: `docs/networking/messages/00-dispatch-and-queues.md`
//! §"Per-property sequencing", transcribing and
//! the house-restriction stamp.
//!
//! Quality updates are **not** ordered by `OrderedEventHeader`. They carry a single byte per (object,
//! quality-type, property-id), with one lazily created stamper per game object.
//! The key is `propertyId | (StatType << 16)`, so *skill*, *skill level* and *skill
//! AC* share one counter, as do *attribute* and *attribute level*.
//!
//! The house-restriction timestamp is a **separate byte** on the same object, not an entry in the
//! table.

use dereth_protocol::wrap::not_older_u8;
use std::collections::HashMap;

/// One object's `PropertySequenceGate`.
#[derive(Debug, Clone, Default)]
pub struct PropertySequenceGate {
    stamps: HashMap<u32, u8>,
    house_ts: Option<u8>,
    /// The client keeps one of these per object; here it is per-stamper, because a global would be
    /// untestable and nothing reads it.
    rejected: u32,
}

impl PropertySequenceGate {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The time stamper's update.
    ///
    /// * key absent → insert and accept;
    /// * key present → accept only when the new byte is *not older* under the 8-bit wrap rule
    ///   (`|new − old| < 0x80 ? new >= old : new <= old`), and record the value when accepted.
    ///
    /// A rejected update bumps the rejected counter and returns `false`. Note that **equal
    /// accepts**: the client's rule is "not older", not "newer", so a repeated stamp is applied
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

    /// The house-restriction stamp — the same rule on a byte kept **outside** the table.
    pub fn update_house(&mut self, stamp: u8) -> bool {
        match self.house_ts {
            None => {
                self.house_ts = Some(stamp);
                true
            }
            Some(old) => {
                if not_older_u8(stamp, old) {
                    self.house_ts = Some(stamp);
                    true
                } else {
                    self.rejected += 1;
                    false
                }
            }
        }
    }

    /// The stamp currently held for a key, if any.
    #[must_use]
    pub fn get(&self, key: u32) -> Option<u8> {
        self.stamps.get(&key).copied()
    }

    /// The house-restriction stamp, if one has arrived.
    #[must_use]
    pub fn house(&self) -> Option<u8> {
        self.house_ts
    }

    /// How many updates this stamper has rejected as stale.
    #[must_use]
    pub fn rejected(&self) -> u32 {
        self.rejected
    }

    /// The number of distinct keys held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.stamps.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stamps.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_protocol::qualities::stat_type;

    /// Oracle: `docs/networking/messages/00-dispatch-and-queues.md` §5, per-property sequencing.
    /// The 8-bit wrap rule accepts and rejects correctly across a wrap.
    #[test]
    fn the_eight_bit_wrap_rule_survives_a_wrap() {
        let key = stat_type::key(stat_type::INT, 25);
        let mut s = PropertySequenceGate::new();

        // The first stamp for a key is always accepted, whatever its value.
        assert!(s.update(key, 200));
        assert_eq!(s.get(key), Some(200));

        // Forward within the half-range.
        assert!(s.update(key, 201));
        // Backward within the half-range is rejected and does not move the stored value.
        assert!(!s.update(key, 200));
        assert_eq!(s.get(key), Some(201));
        assert_eq!(s.rejected(), 1);

        // Across the wrap: 0x03 after 0xFE is five steps forward, not 251 back.
        assert!(s.update(key, 0xFE));
        assert!(s.update(key, 0x03));
        assert_eq!(s.get(key), Some(0x03));
        // And going back the other way across the wrap is still a rejection.
        assert!(!s.update(key, 0xFE));
    }

    /// Equal accepts: the rule is "not older", not "newer".
    #[test]
    fn a_repeated_stamp_is_accepted() {
        let mut s = PropertySequenceGate::new();
        assert!(s.update(1, 7));
        assert!(s.update(1, 7));
        assert_eq!(s.rejected(), 0);
    }

    /// Because *skill*, *skill level* and *skill AC* all share tag `0x00040000`, the three
    /// messages for one skill share a **single** sequence counter. Sending them out of order loses
    /// updates. `docs/networking/messages/03-qualities-and-updates.md` §2.
    #[test]
    fn skill_and_skill_level_share_one_counter() {
        let mut s = PropertySequenceGate::new();
        let skill_key = stat_type::key(stat_type::SKILL, 22);
        assert!(s.update(skill_key, 5));
        // The "skill level" message for the same skill computes the same key, so a lower stamp on
        // it is rejected — which is the whole point.
        assert!(!s.update(skill_key, 4));
        // A different property is a different counter.
        assert!(s.update(stat_type::key(stat_type::SKILL, 23), 1));
        // A different stat type on the same property id is also a different counter.
        assert!(s.update(stat_type::key(stat_type::FLOAT, 22), 1));
        assert_eq!(s.len(), 3);
    }

    /// The house timestamp is kept **outside** the table, so it cannot collide with a property key.
    #[test]
    fn the_house_timestamp_is_separate() {
        let mut s = PropertySequenceGate::new();
        assert!(s.update_house(1));
        assert!(!s.update_house(0));
        assert_eq!(s.house(), Some(1));
        assert!(s.is_empty(), "nothing was added to the property table");
    }

    /// `0x0197 Item_UpdateStackSize` shares `PropertyInt::StackSize`'s counter, key `0x1000C`.
    #[test]
    fn the_stack_size_message_shares_the_stack_size_property_counter() {
        use dereth_protocol::items::ItemUpdateStackSize;
        let mut s = PropertySequenceGate::new();
        assert!(s.update(ItemUpdateStackSize::STAMPER_KEY, 3));
        assert!(!s.update(stat_type::key(stat_type::INT, 12), 2));
    }
}
