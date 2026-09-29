// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Enum/NetAuthType.cs

/// ACE `NetAuthType`. A raw value ACE's `(NetAuthType)` cast would accept but the enum does not
/// name is kept in [`NetAuthType::Other`], because C# enums hold any `uint`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NetAuthType {
    Undef,
    Account,
    AccountPassword,
    GlsTicket,
    Other(u32),
}

impl NetAuthType {
    #[must_use]
    pub const fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::Undef,
            1 => Self::Account,
            2 => Self::AccountPassword,
            0x4000_0002 => Self::GlsTicket,
            other => Self::Other(other),
        }
    }

    #[must_use]
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Undef => 0,
            Self::Account => 1,
            Self::AccountPassword => 2,
            Self::GlsTicket => 0x4000_0002,
            Self::Other(v) => v,
        }
    }

    /// C#'s `<` on the underlying `uint`, which `AccountSelectCallback` uses
    /// (`loginRequest.NetAuthType < NetAuthType.AccountPassword`).
    #[must_use]
    pub const fn lt(self, other: Self) -> bool {
        self.to_u32() < other.to_u32()
    }
}
