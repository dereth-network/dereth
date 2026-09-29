// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Sequence/ByteSequence.cs
//! Port of `Source/ACE.Server/Network/Sequence/ByteSequence.cs`.

use super::i_sequence::ISequence;

// ACE: ByteSequence
/// ACE `ByteSequence`: counts up to `max_value`, then wraps to 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteSequence {
    max_value: u8,
    current_value: u8,
}

impl ByteSequence {
    // ACE: ByteSequence.ByteSequence
    /// `ByteSequence(u8 startingValue, u8 maxValue = Byte.MaxValue)`.
    #[must_use]
    pub fn new(starting_value: u8, max_value: u8) -> Self {
        Self {
            max_value,
            current_value: starting_value,
        }
    }

    /// `ByteSequence(bool clientPrimed = true, u8 maxValue = Byte.MaxValue)`: creates an instance without a
    /// starting value. `client_primed`: whether the value gets sent to the client before the first
    /// increment (0), or not (the first `NextValue` wraps from `max_value` to 0).
    #[must_use]
    pub fn new_primed(client_primed: bool, max_value: u8) -> Self {
        let current_value = if client_primed { 0 } else { max_value };
        Self {
            max_value,
            current_value,
        }
    }

    /// `CurrentValue`.
    #[must_use]
    pub fn current_value(&self) -> u8 {
        self.current_value
    }

    // ACE: ByteSequence.NextValue
    /// `NextValue`: the getter advances the counter.
    pub fn next_value(&mut self) -> u8 {
        if self.current_value == self.max_value {
            self.current_value = 0;
            return self.current_value;
        }
        self.current_value = self.current_value.wrapping_add(1);
        self.current_value
    }
}

impl ISequence for ByteSequence {
    // ACE: ByteSequence.NextBytes
    fn next_bytes(&mut self) -> Vec<u8> {
        vec![self.next_value()]
    }

    // ACE: ByteSequence.CurrentBytes
    /// `new byte[] { CurrentValue }`.
    fn current_bytes(&self) -> Vec<u8> {
        vec![self.current_value]
    }
}
