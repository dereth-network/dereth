// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Enum/CharacterGenerationVerificationResponse.cs

/// ACE `CharacterGenerationVerificationResponse`, the result `GameMessageCharacterCreateResponse`
/// carries (written as its `uint` value).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CharacterGenerationVerificationResponse(pub u32);

#[allow(non_upper_case_globals)]
impl CharacterGenerationVerificationResponse {
    pub const Undef: Self = Self(0);
    pub const Ok: Self = Self(1);
    pub const Pending: Self = Self(2);
    pub const NameInUse: Self = Self(3);
    pub const NameBanned: Self = Self(4);
    pub const Corrupt: Self = Self(5);
    pub const DatabaseDown: Self = Self(6);
    pub const AdminPrivilegeDenied: Self = Self(7);
    pub const Count: Self = Self(8);
}
