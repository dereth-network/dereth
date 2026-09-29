// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Emote.cs
//! `Emote`: the legacy emote record (fields only). Emotes are similar to a data-driven event
//! system. Reference-typed fields are nullable in ACE and default to `null`.

use crate::create_profile::CreateProfile;
use crate::enums::{EmoteType, PlayScript, Sound};
use crate::frame::Frame;
use crate::position::Position;

/// ACE: Emote
#[derive(Debug, Clone, Default)]
pub struct Emote {
    // ACE: Emote.Type
    pub r#type: EmoteType,
    // ACE: Emote.Delay
    pub delay: f32,
    // ACE: Emote.Extent
    pub extent: f32,
    // ACE: Emote.Amount
    pub amount: u64,
    // ACE: Emote.HeroXP
    pub hero_xp: u64,
    // ACE: Emote.Min
    pub min: u64,
    // ACE: Emote.Max
    pub max: u64,
    // ACE: Emote.MinFloat
    pub min_float: f64,
    // ACE: Emote.MaxFloat
    pub max_float: f64,
    // ACE: Emote.Stat
    pub stat: u32,
    // ACE: Emote.Motion
    pub motion: u32,
    // ACE: Emote.PScript
    pub p_script: PlayScript,
    // ACE: Emote.Sound
    pub sound: Sound,
    // ACE: Emote.CreateProfile
    pub create_profile: Option<CreateProfile>,
    // ACE: Emote.Frame
    pub frame: Option<Frame>,
    // ACE: Emote.SpellId
    pub spell_id: u32,
    // ACE: Emote.TestString
    pub test_string: Option<String>,
    // ACE: Emote.Message
    pub message: Option<String>,
    // ACE: Emote.Percent
    pub percent: f64,
    // ACE: Emote.Display
    pub display: i32,
    // ACE: Emote.Wealth
    pub wealth: i32,
    // ACE: Emote.Loot
    pub loot: i32,
    // ACE: Emote.LootType
    pub loot_type: i32,
    // ACE: Emote.Position
    pub position: Option<Position>,
}
