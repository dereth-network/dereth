// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/MagicState.cs
//! Port of `Source/ACE.Server/Entity/MagicState.cs`.
//!
//! The state is `Player.MagicState` (`PlayerMagicFields.magic_state`). ACE's back-reference to the
//! `Player` becomes the `player` argument of the members that touch it; the
//! cast parameters hold guids.

use empyrean_common::dotnet::DotNetDateTime;
use empyrean_entity::enums::{ChatMessageType, MotionCommand};
use empyrean_entity::ObjectGuid;

use crate::entity::cast_queue::CastQueue;
use crate::entity::cast_spell_params::{CastSpellParams, CastingPreCheckStatus};
use crate::entity::spell::Spell;
use crate::entity::windup_params::WindupParams;
use crate::world_objects::player_magic;
use crate::World;

/*public enum CastingState
{
    WindupTurn,
    WindupGesture,
    CastGesture,
    CastTurn,
    Ready
};*/

// ACE: MagicState
#[derive(Debug, Clone)]
pub struct MagicState {
    //public CastingState CastingState { get; set; }
    /// This flag indicates if player is currently casting a spell.
    ///
    /// It gets set to TRUE when they press the cast button, and becomes FALSE again when their
    /// recoil animation has completed. If server is running with spellcast_recoil_queue = true, it
    /// becomes FALSE when their recoil animation has started.
    pub is_casting: bool,

    /// The formula test attached to this cast, acknowledged only on success.
    pub research_spell: Option<u32>,

    /// Returns TRUE if the first half of the 'launch spell' motion has made its way through the
    /// motion queue.
    pub cast_motion_done: bool,

    /// Returns TRUE if player has started turning for either the windup or cast launch. After
    /// turning has completed, this will still be true.
    pub turn_started: bool,

    /// Returns TRUE if current player animation frame is turning.
    pub is_turning: bool,

    /// Information required for performing the windup.
    pub windup_params: Option<WindupParams>,

    /// Information required for launching the spell.
    pub cast_spell_params: Option<CastSpellParams>,

    /// The 'launch spell' motion for the current cast.
    pub cast_gesture: MotionCommand,

    /// The time when the player pressed the key to begin spell casting.
    pub start_time: DotNetDateTime,

    /// If a player interrupts a TurnTo during casting, the TurnTo resumes when the player is no
    /// longer holding any Turn keys.
    pub pending_turn_release: bool,

    /// Tracks the cast # for /recordcast.
    pub cast_num: i32,

    /// If TRUE, the casting efficiency meter has been enabled with /castmeter. This shows the
    /// player information about how quickly they interrupted the cast motion.
    pub cast_meter: bool,

    /// The time when the player started performing the 'launch spell' casting gesture. This is
    /// used for CastMeter measurement.
    pub cast_gesture_start_time: DotNetDateTime,

    /// This is only used if server option spellcast_recoil_queue = true. Allows the player to
    /// queue the next spellcast as soon as the previous spell is released.
    pub can_queue: bool,
    pub cast_queue: Option<CastQueue>,

    /// By default, MoveToManager waits for the player to be in Ready state before beginning a
    /// TurnTo. For some motions, such as immmediately after the CastGesture, this can produce
    /// unnecessary delays. This will be set to TRUE only for the first turn after the CastGesture.
    pub always_turn: bool,
}

impl Default for MagicState {
    fn default() -> Self {
        Self::new()
    }
}

impl MagicState {
    /// `new MagicState(player)`: the `Player` link is the `player` argument of the members. The
    /// unset `MotionCommand` and `DateTime` read as their C# defaults (0 and `MinValue`).
    // ACE: MagicState.MagicState
    #[must_use]
    pub fn new() -> Self {
        MagicState {
            is_casting: false,
            research_spell: None,
            cast_motion_done: false,
            turn_started: false,
            is_turning: false,
            windup_params: None,
            cast_spell_params: None,
            cast_gesture: MotionCommand::Invalid,
            start_time: DotNetDateTime::MIN_VALUE,
            pending_turn_release: false,
            cast_num: 0,
            cast_meter: false,
            cast_gesture_start_time: DotNetDateTime::MIN_VALUE,
            can_queue: false,
            cast_queue: None,
            always_turn: false,
        }
    }
}

fn state(w: &World, player: ObjectGuid) -> &MagicState {
    &player_magic::fields(w, player).magic_state
}

fn state_mut(w: &mut World, player: ObjectGuid) -> &mut MagicState {
    &mut player_magic::fields_mut(w, player).magic_state
}

/// Called when the player begins casting a spell.
// ACE: MagicState.OnCastStart
pub fn on_cast_start(w: &mut World, player: ObjectGuid) {
    player_magic::set_is_busy(w, player, true);
    let now = w.now.utc;
    let s = state_mut(w, player);
    s.is_casting = true;
    s.research_spell = None;
    s.cast_motion_done = false;
    s.turn_started = false;
    s.is_turning = false;
    s.pending_turn_release = false;
    s.can_queue = false;
    s.cast_queue = None;
    s.always_turn = false;

    s.start_time = now;
    s.cast_gesture_start_time = DotNetDateTime::MIN_VALUE;

    if player_magic::under_lifestone_protection(w, player) {
        player_magic::lifestone_protection_dispel(w, player);
    }

    state_mut(w, player).cast_num += 1;

    if player_magic::record_cast_enabled(w, player) {
        let cast_num = state(w, player).cast_num;
        player_magic::send_system_chat(
            w,
            player,
            &format!("Cast #: {cast_num}"),
            ChatMessageType::Broadcast,
        );
        player_magic::record_cast_log(w, player, &format!("MagicState.OnCastStart({cast_num})"));
        player_magic::record_cast_log_location(w, player, "Player Location: ", player);
    }
}

/// Called when the player finishes casting a spell.
// ACE: MagicState.OnCastDone
pub fn on_cast_done(w: &mut World, player: ObjectGuid) {
    player_magic::set_is_busy(w, player, false);
    {
        let s = state_mut(w, player);
        s.is_casting = false;
        s.research_spell = None;
        s.cast_motion_done = false;
        s.turn_started = false;
        s.is_turning = false;
        s.pending_turn_release = false;
    }
    player_magic::fields_mut(w, player).turn_target = None;
    {
        let s = state_mut(w, player);
        s.can_queue = false;
        s.cast_queue = None;
        s.always_turn = false;

        s.cast_gesture = MotionCommand::Invalid;
        s.cast_gesture_start_time = DotNetDateTime::MIN_VALUE;
    }

    if player_magic::record_cast_enabled(w, player) {
        player_magic::record_cast_log(w, player, "MagicState.OnCastDone()");
        player_magic::record_cast_log_location(w, player, "Player Location: ", player);
        if let Some(target) = state(w, player)
            .cast_spell_params
            .as_ref()
            .and_then(|p| p.target)
        {
            player_magic::record_cast_log_location(w, player, "Target Location: ", target);
        }
        player_magic::record_cast_log(
            w,
            player,
            "================================================================================",
        );
        player_magic::record_cast_flush(w, player);
    }

    let s = state_mut(w, player);
    s.cast_spell_params = None;
    s.windup_params = None;
}

// ACE: MagicState.SetCastParams
#[allow(clippy::too_many_arguments)]
pub fn set_cast_params(
    w: &mut World,
    player: ObjectGuid,
    spell: Spell,
    caster_item: Option<ObjectGuid>,
    magic_skill: u32,
    mana_used: u32,
    target: Option<ObjectGuid>,
    status: CastingPreCheckStatus,
) {
    state_mut(w, player).cast_spell_params = Some(CastSpellParams::new(
        spell,
        caster_item,
        magic_skill,
        mana_used,
        target,
        status,
    ));

    if player_magic::record_cast_enabled(w, player) {
        if let Some(target) = target {
            player_magic::record_cast_log_location(w, player, "Target Location: ", target);
        }
    }
}

// ACE: MagicState.SetWindupParams
pub fn set_windup_params(
    w: &mut World,
    player: ObjectGuid,
    target_guid: u32,
    spell_id: u32,
    caster_item: Option<ObjectGuid>,
) {
    state_mut(w, player).windup_params =
        Some(WindupParams::new(target_guid, spell_id, caster_item));
}

/// The diagnostic dump (`CastMotionStarted` repeats `CastMotionDone`, as in ACE).
// ACE: MagicState.ToString
#[must_use]
pub fn to_string(w: &World, player: ObjectGuid) -> String {
    let s = state(w, player);
    let name = crate::dispatch::name::name(w, player).unwrap_or_default();
    let mut str = format!("Player: {name} ({player})\n");
    str += &format!("IsCasting: {}\n", dotnet_bool(s.is_casting));
    // ACE-BUG: "CastMotionStarted" prints CastMotionDone (the debug dump only).
    str += &format!("CastMotionStarted: {}\n", dotnet_bool(s.cast_motion_done));
    str += &format!("CastMotionDone: {}\n", dotnet_bool(s.cast_motion_done));
    str += &format!("TurnStarted: {}\n", dotnet_bool(s.turn_started));
    str += &format!("IsTurning: {}\n", dotnet_bool(s.is_turning));
    str += &format!(
        "WindupParams: {}\n",
        s.windup_params
            .as_ref()
            .map(|p| p.to_string(w))
            .unwrap_or_default()
    );
    str += &format!(
        "CastSpellParams: {}\n",
        s.cast_spell_params
            .as_ref()
            .map(|p| p.to_string(w))
            .unwrap_or_default()
    );
    str += &format!("CastGesture: {}\n", s.cast_gesture.to_dotnet_string());
    str += &format!("StartTime: {}", s.start_time);
    str
}

/// `bool.ToString()`.
fn dotnet_bool(b: bool) -> &'static str {
    if b {
        "True"
    } else {
        "False"
    }
}
