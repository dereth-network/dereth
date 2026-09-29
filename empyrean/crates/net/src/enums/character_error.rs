// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Enum/CharacterError.cs

/// ACE `CharacterError` (the client's `CharError`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum CharacterError {
    Logon = 0x0000_0001,
    AccountLogin = 0x0000_0003,
    ServerCrash1 = 0x0000_0004,
    Logoff = 0x0000_0005,
    Delete = 0x0000_0006,
    ServerCrash2 = 0x0000_0008,
    AccountInvalid = 0x0000_0009,
    AccountDoesntExist = 0x0000_000A,
    EnterGameGeneric = 0x0000_000B,
    EnterGameStressAccount = 0x0000_000C,
    EnterGameCharacterInWorld = 0x0000_000D,
    EnterGamePlayerAccountMissing = 0x0000_000E,
    EnterGameCharacterNotOwned = 0x0000_000F,
    EnterGameCharacterInWorldServer = 0x0000_0010,
    EnterGameOldCharacter = 0x0000_0011,
    EnterGameCorruptCharacter = 0x0000_0012,
    EnterGameStartServerDown = 0x0000_0013,
    EnterGameCouldntPlaceCharacter = 0x0000_0014,
    LogonServerFull = 0x0000_0015,
    EnterGameCharacterLocked = 0x0000_0017,
    SubscriptionExpired = 0x0000_0018,
}
