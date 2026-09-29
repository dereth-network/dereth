// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/CommandHandlerFlag.cs
//! Port of `Source/ACE.Server/Command/CommandHandlerFlag.cs`.

use std::ops::{BitAnd, BitOr};

// ACE: CommandHandlerFlag
/// `[Flags] enum CommandHandlerFlag`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CommandHandlerFlag(pub i32);

#[allow(non_upper_case_globals)]
impl CommandHandlerFlag {
    pub const None: Self = Self(0x00);
    pub const ConsoleInvoke: Self = Self(0x01);
    pub const RequiresWorld: Self = Self(0x02);
}

impl BitOr for CommandHandlerFlag {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitAnd for CommandHandlerFlag {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}
