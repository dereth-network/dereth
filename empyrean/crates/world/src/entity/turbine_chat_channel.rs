// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/TurbineChatChannel.cs
//! Port of `Source/ACE.Server/Entity/TurbineChatChannel.cs`.

// ACE: TurbineChatChannel
// ACE's `public static uint` fields (mutable statics it never writes), as constants.

pub const ALLEGIANCE: u32 = 1;

pub const GENERAL: u32 = 2;
pub const TRADE: u32 = 3;
pub const LFG: u32 = 4;
pub const ROLEPLAY: u32 = 5;

pub const SOCIETY: u32 = 6;

pub const SOCIETY_CELESTIAL_HAND: u32 = 7;
pub const SOCIETY_ELDRYTCH_WEB: u32 = 8;
pub const SOCIETY_RADIANT_BLOOD: u32 = 9;

pub const OLTHOI: u32 = 10;
