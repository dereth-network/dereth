// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Sequence/UShortSequence.cs
//! Port of `Source/ACE.Server/Network/Sequence/UShortSequence.cs`.

use super::i_sequence::ISequence;

// ACE: UShortSequence
/// ACE `UShortSequence`: counts up to `max_value`, then wraps to 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UShortSequence {
    max_value: u16,
    current_value: u16,
}

impl UShortSequence {
    // ACE: UShortSequence.UShortSequence
    /// `UShortSequence(u16 startingValue, u16 maxValue = UInt16.MaxValue)`.
    #[must_use]
    pub fn new(starting_value: u16, max_value: u16) -> Self {
        Self {
            max_value,
            current_value: starting_value,
        }
    }

    /// `UShortSequence(bool clientPrimed = true, u16 maxValue = UInt16.MaxValue)`: creates an instance without a
    /// starting value. `client_primed`: whether the value gets sent to the client before the first
    /// increment (0), or not (the first `NextValue` wraps from `max_value` to 0).
    #[must_use]
    pub fn new_primed(client_primed: bool, max_value: u16) -> Self {
        let current_value = if client_primed { 0 } else { max_value };
        Self {
            max_value,
            current_value,
        }
    }

    /// `CurrentValue`.
    #[must_use]
    pub fn current_value(&self) -> u16 {
        self.current_value
    }

    // ACE: UShortSequence.NextValue
    /// `NextValue`: the getter advances the counter.
    pub fn next_value(&mut self) -> u16 {
        if self.current_value == self.max_value {
            self.current_value = 0;
            return self.current_value;
        }
        self.current_value = self.current_value.wrapping_add(1);
        self.current_value
    }
}

impl ISequence for UShortSequence {
    // ACE: UShortSequence.NextBytes
    fn next_bytes(&mut self) -> Vec<u8> {
        self.next_value().to_le_bytes().to_vec()
    }

    // ACE: UShortSequence.CurrentBytes
    /// `BitConverter.GetBytes(CurrentValue)`: little-endian.
    fn current_bytes(&self) -> Vec<u8> {
        self.current_value.to_le_bytes().to_vec()
    }
}
