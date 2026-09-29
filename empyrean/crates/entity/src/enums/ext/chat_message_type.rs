// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ChatMessageType.cs

use crate::enums::{ChatMessageType, SquelchMask};

impl ChatMessageType {
    // ACE: ChatMessageTypeExtensions.ToMask
    pub fn to_mask(self) -> SquelchMask {
        if self == ChatMessageType::AllChannels {
            SquelchMask::AllChannels
        } else {
            // C# `(SquelchMask)(1 << (int)type)`: an int shift, whose count C# masks to its low
            // five bits, then an unchecked int -> uint conversion.
            #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
            let bits = 1i32.wrapping_shl(self.0) as u32;
            SquelchMask(bits)
        }
    }
}
