// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/SquelchMask.cs

use crate::enums::SquelchMask;

impl SquelchMask {
    // ACE: SquelchMaskExtensions.Add
    #[allow(clippy::should_implement_trait)] // ACE's name; not an arithmetic `Add`
    pub fn add(self, mask_b: SquelchMask) -> SquelchMask {
        let mask_a = self;
        if mask_a == SquelchMask::AllChannels || mask_b == SquelchMask::AllChannels {
            return SquelchMask::AllChannels;
        }

        let result = mask_a | mask_b;

        if result == SquelchMask::Combined {
            SquelchMask::AllChannels
        } else {
            result
        }
    }

    // ACE: SquelchMaskExtensions.Remove
    pub fn remove(self, mask_b: SquelchMask) -> SquelchMask {
        if mask_b == SquelchMask::AllChannels {
            return SquelchMask::None;
        }

        let mut result = self;

        if result == SquelchMask::AllChannels {
            result = SquelchMask::Combined;
        }

        result &= !mask_b;

        if result == SquelchMask::Combined {
            result = SquelchMask::AllChannels;
        }

        result
    }
}
