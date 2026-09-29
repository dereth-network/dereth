//! Combat modes, the power bar, target selection and the two attack messages.
//!
//! **Nothing is predicted.** No animation is played, no stamina is deducted, no damage is estimated:
//! the client sends the request and waits for `Combat_CommenceAttack`. Two shipped bugs live
//! here and are reproduced deliberately: releasing an attack executes it **twice** in the
//! classic UI, and `"Critical hit!  "` has **two** trailing spaces in the attacker
//! message and one in the defender message.

use crate::inventory::slots::loc;
use crate::weenie::item_type;
use crate::world::World;
use crate::{Request, RequestSink};
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::combat::{
    CombatCancelAttack, CombatChangeCombatMode, CombatQueryHealth, CombatTargetedMeleeAttack,
    CombatTargetedMissileAttack,
};

/// `COMBAT_MODE` — a bit **mask**, not an ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum CombatMode {
    #[default]
    Undef = 0,
    NonCombat = 1,
    Melee = 2,
    Missile = 4,
    Magic = 8,
}

impl CombatMode {
    pub const COMBAT: u32 = 14;
    pub const VALID: u32 = 15;

    #[must_use]
    pub fn from_raw(v: u32) -> Self {
        match v {
            1 => Self::NonCombat,
            2 => Self::Melee,
            4 => Self::Missile,
            8 => Self::Magic,
            _ => Self::Undef,
        }
    }

    #[must_use]
    pub fn raw(self) -> u32 {
        self as u32
    }

    /// Mode name used only in the "you can't enter %hs mode…" error.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::NonCombat => "peace",
            Self::Melee => "melee",
            Self::Missile => "missile",
            Self::Magic => "magic",
            Self::Undef => "unknown",
        }
    }
}

/// `ATTACK_HEIGHT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum AttackHeight {
    Undef = 0,
    High = 1,
    #[default]
    Medium = 2,
    Low = 3,
}

/// The power bar's mode, shared with the power-bar widgets.
pub use dereth_client_contract::powerbar::PowerBarMode;

/// Ordered power-bar display notices. A final mode snapshot cannot represent combat-level zero
/// before undefined mode, or a hide/restart in one UI frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PowerBarNotice {
    /// The begin-notice subscriber reads combat mode and Recklessness
    /// synchronously. Capture those facts at the producer, not after another mode/quality change.
    Begin {
        mode: PowerBarMode,
        melee: bool,
        recklessness_sac: u32,
    },
    /// The set-power-bar-level notice, retaining the sender's mode at the time of the call.
    SetLevel { mode: PowerBarMode, level: f32 },
    /// The finish-power-bar notice; only a subscriber currently in this mode accepts it.
    Finish { mode: PowerBarMode },
}

/// `DAMAGE_TYPE` (mask).
pub mod damage_type {
    pub const UNDEF: u32 = 0;
    pub const SLASH: u32 = 1;
    pub const PIERCE: u32 = 2;
    pub const BLUDGEON: u32 = 4;
    pub const COLD: u32 = 8;
    pub const FIRE: u32 = 16;
    pub const ACID: u32 = 32;
    pub const ELECTRIC: u32 = 64;
    pub const HEALTH: u32 = 128;
    pub const STAMINA: u32 = 256;
    pub const MANA: u32 = 512;
    pub const NETHER: u32 = 1024;
    pub const BASE: u32 = 0x1000_0000;
}

/// `COMBAT_USE` — `pwd.combat_use`.
pub mod combat_use {
    pub const NONE: u8 = 0;
    pub const MELEE: u8 = 1;
    pub const MISSILE: u8 = 2;
    pub const AMMO: u8 = 3;
    pub const SHIELD: u8 = 4;
    pub const TWO_HANDED: u8 = 5;
}

/// `AttackConditions` — the bit mask on the notification messages. `[external, ACE names]`,
/// confirmed by the client's own decode.
pub mod attack_conditions {
    /// The target's Critical Protection augmentation cancelled the critical.
    pub const CRITICAL_PROTECTION: u32 = 1;
    pub const RECKLESSNESS: u32 = 2;
    pub const SNEAK_ATTACK: u32 = 4;
    /// The hit overpowered the target: "Overpower! " after the critical-hit prefix.
    pub const OVERPOWER: u32 = 8;
}

/// The motion style that shortens the power-bar charge.
pub const DUAL_WIELD_COMBAT_STYLE: u32 = 0x8000_0046;

/// The full-power charge time, in seconds: **1.000 s**, or **0.800 s** while dual-wielding. The only
/// timing constant in client combat.
pub const POWER_BAR_SECONDS: f64 = 1.0;
pub const POWER_BAR_SECONDS_DUAL_WIELD: f64 = 0.8;
/// `MIN_JUMP_EXTENT`.
pub const MIN_JUMP_EXTENT: f32 = 0.001;
/// The only tolerance constant: the auto-repeat loop re-fires early only if the slider moved by more
/// than this (the attack-done handler, `handle_attack_done`).
pub const SLIDER_TOLERANCE: f32 = 0.01;

/// `MotionCommand::Ready` — `interpreted_state.forward_command`'s idle value, and the one
/// `player_in_ready_position`'s missile arm demands (exactly `0x41000003`).
pub const MOTION_READY: u32 = 0x4100_0003;

/// The **six** `current_style` values the client's
/// missile arm accepts, as the final retail client numbers them.
///
/// The readiness check is a case table on `current_style`: every other stance returns false, and
/// these six go on to test the forward command.
///
/// | style | index | name |
/// |---|---|---|
/// | `0x8000003F` | 63 | `BowCombat` |
/// | `0x80000041` | 65 | `CrossbowCombat` |
/// | `0x80000043` | 67 | `SlingCombat` |
/// | `0x80000047` | 71 | `ThrownWeaponCombat` |
/// | `0x8000013B` | 315 | `AtlatlCombat` |
/// | `0x8000013C` | 316 | `ThrownShieldCombat` |
///
/// `\[verified\]`, with the last two from the final retail client. The 2013 client tested
/// `0x80000138` / `0x80000139` there: its command table
/// numbered those two stances three lower, before three commands were inserted at `0x10F`-`0x111`.
/// The data files and the server both use the final numbering, and a missile mode whose stance
/// the check cannot recognise is a mode the client can never leave (the retry is gated on the
/// same predicate): missile combat with an atlatl would not animate, would use the wrong
/// stance, and could not be exited.
pub const MISSILE_READY_STYLES: [u32; 6] = [
    0x8000_003F,
    0x8000_0041,
    0x8000_0043,
    0x8000_0047,
    0x8000_013B,
    0x8000_013C,
];

/// DataID quality key 4 — `CombatTable`, the DataID quality `player_in_ready_position`'s melee arm reads
/// from the player's qualities. The invalid data id is zero.
pub const COMBAT_TABLE_DID: u32 = 4;

/// Mutable combat state with its session-reset values.
///
/// The reset runs from the constructor **and** from the end-of-character-session reset, so the
/// state is fully rebuilt on every logout.
#[derive(Debug, Clone, PartialEq)]
pub struct CombatState {
    pub jump_pending: bool,
    pub tracking_target: bool,
    pub combat_mode: CombatMode,
    pub pending_combat_mode: CombatMode,
    pub requested_attack_height: AttackHeight,
    pub build_start_time: f64,
    pub build_in_progress: bool,
    pub power_bar_mode: PowerBarMode,
    pub latest_power_bar_level: f32,
    pub attack_in_progress: bool,
    pub attack_server_response_pending: bool,
    pub attack_request_in_progress: bool,
    pub requested_attack_power: f32,
    pub repeat_attacking: bool,
    pub current_build_is_automatic: bool,
    /// No known writer sets this true. Initialize false, clear where the client
    /// clears, never set.
    pub target_willingly_lost: bool,
    pub attack_when_response_received: bool,
    pub attack_when_response_received_power: f32,
    /// The power-slider setting — the "cap". Resets to **0.5**.
    pub ui_requested_power: f32,
    pub advanced_combat_mode: bool,
    /// Time of the last defender notification, measured on the client timer.
    ///
    /// Read by automatic targeting's 15-second freshness test and written
    /// by exactly one, which is the tail the two
    /// defender-notification handlers share. Without that writer the field would keep its zero
    /// for the whole session and the re-select arm could not fire.
    pub last_attacked_time: f64,
    /// `interpreted_state.current_style`, which the motion layer owns; combat only reads it.
    pub current_style: u32,
    /// `interpreted_state.forward_command`, the second field of the same struct
    /// the readiness check's missile arm reads alongside `current_style`. The motion layer
    /// owns it; combat only reads it, and only in `MISSILE_COMBAT_MODE`.
    pub forward_command: u32,
    /// Display-only journal, drained once per application UI frame, including when no gameplay
    /// subscriber exists. It is never a history to replay onto a subsequently constructed UI.
    power_bar_notices: Vec<PowerBarNotice>,
}

impl Default for CombatState {
    fn default() -> Self {
        Self::begin()
    }
}

impl CombatState {
    /// Reset combat state for a new character session.
    #[must_use]
    pub fn begin() -> Self {
        Self {
            jump_pending: false,
            tracking_target: false,
            combat_mode: CombatMode::NonCombat,
            pending_combat_mode: CombatMode::Undef,
            requested_attack_height: AttackHeight::Medium,
            build_start_time: 0.0,
            build_in_progress: false,
            power_bar_mode: PowerBarMode::Undef,
            latest_power_bar_level: 0.0,
            attack_in_progress: false,
            attack_server_response_pending: false,
            attack_request_in_progress: false,
            requested_attack_power: 0.0,
            repeat_attacking: false,
            current_build_is_automatic: false,
            target_willingly_lost: false,
            attack_when_response_received: false,
            attack_when_response_received_power: 0.0,
            ui_requested_power: 0.5,
            advanced_combat_mode: false,
            last_attacked_time: 0.0,
            current_style: 0x8000_003D, // NonCombat
            forward_command: MOTION_READY,
            power_bar_notices: Vec::new(),
        }
    }

    /// The power bar's current level.
    #[must_use]
    pub fn power_bar_level(&self, now: LocalTime) -> f32 {
        if !self.build_in_progress {
            return 0.0;
        }
        let t = if self.current_style == DUAL_WIELD_COMBAT_STYLE {
            POWER_BAR_SECONDS_DUAL_WIELD
        } else {
            POWER_BAR_SECONDS
        };
        let level = (now.0 - self.build_start_time) / t;
        #[allow(clippy::cast_possible_truncation)] // the client narrows the result to float
        let level = level as f32;
        level.clamp(0.0, 1.0)
    }

    /// Return `max(level, MIN_JUMP_EXTENT)` while a jump is pending, otherwise zero.
    #[must_use]
    pub fn jump_power_level(&self, now: LocalTime) -> f32 {
        if !self.jump_pending {
            return 0.0;
        }
        self.power_bar_level(now).max(MIN_JUMP_EXTENT)
    }

    /// Start building the power bar.
    pub fn start_power_bar_build(&mut self, now: LocalTime) {
        self.build_in_progress = true;
        self.build_start_time = now.0;
        self.set_power_bar_level(0.0);
    }

    /// Queue the power-bar level notice and update its cached value.
    ///
    /// The notice retains its mode even if another operation changes `power_bar_mode` before
    /// the UI frame drains it. The cache is retail state, not a replacement notice stream.
    pub fn set_power_bar_level(&mut self, level: f32) {
        self.power_bar_notices.push(PowerBarNotice::SetLevel {
            mode: self.power_bar_mode,
            level,
        });
        self.latest_power_bar_level = level;
    }

    /// Begin arm shared by the attack-start and jump-start paths.
    /// Classic combat sends a zero level instead of a Begin notice; neither arm changes the
    /// cached latest power-bar level (the subsequent Start/Set does that).
    pub fn begin_power_bar(&mut self, mode: PowerBarMode, melee: bool, recklessness_sac: u32) {
        self.power_bar_mode = mode;
        self.power_bar_notices
            .push(if mode == PowerBarMode::Combat {
                PowerBarNotice::SetLevel { mode, level: 0.0 }
            } else {
                PowerBarNotice::Begin {
                    mode,
                    melee,
                    recklessness_sac,
                }
            });
    }

    /// Consume the ordered display batch exactly once. A host with no UI must also drain and
    /// discard it each frame: retail notices sent without subscribers do not accumulate.
    pub fn take_power_bar_notices(&mut self) -> Vec<PowerBarNotice> {
        std::mem::take(&mut self.power_bar_notices)
    }

    /// Finish the jump's power bar; the first Escape-key handler reaches this path.
    ///
    /// When a jump is pending, stop the build and clear its start time. Mode 1 emits level zero;
    /// other modes emit completion. Then set the bar mode to undefined. The player-body callback
    /// clears its standing-long-jump flag, and the jump-pending flag is cleared unconditionally.
    ///
    /// Neither this nor hiding the bar zeroes the cached latest power-bar level: hiding clears
    /// the build-in-progress flag and the build start time, raises set-level-0 when the bar mode
    /// is 1 and finish otherwise, and sets the bar mode to 0.
    ///
    /// `jump_pending` is cleared **unconditionally**, outside the `if` — so finishing a jump on a
    /// body that was not jumping still clears the flag, and that is the store rather
    /// than an inference.
    ///
    /// Isolated model adapter. The client uses [`Self::finish_jump_with_body`] to clear the
    /// actual player's motion interpreter between hiding the power bar and the final pending
    /// clear.
    pub fn finish_jump(&mut self) {
        self.finish_jump_with_body(|| {});
    }

    /// Finish a jump by hiding the power bar, clearing player `standing_longjump` even when the
    /// jump is not pending, and then clearing `jump_pending`. The callback borrows the body.
    pub fn finish_jump_with_body(&mut self, clear_body: impl FnOnce()) {
        if self.jump_pending {
            self.hide_power_bar();
        }
        clear_body();
        self.jump_pending = false;
    }

    /// Hide the power bar.
    pub fn hide_power_bar(&mut self) {
        self.build_in_progress = false;
        self.build_start_time = 0.0;
        let mode = self.power_bar_mode;
        self.power_bar_notices
            .push(if mode == PowerBarMode::Combat {
                PowerBarNotice::SetLevel { mode, level: 0.0 }
            } else {
                PowerBarNotice::Finish { mode }
            });
        self.power_bar_mode = PowerBarMode::Undef;
    }
}

/// Power-step arithmetic: truncate `(power + 0.083333336) * 6`, add or subtract one notch,
/// multiply by `0.16666667`, then clamp to `[0, 1]` and announce the changed desired power.
///
/// So the gauge has **seven notches** — `0`, `1/6`, … `1` — the `+ 0.5/6` before the truncation
/// is a round-half-up onto the nearest notch, and the clamp is applied to the *scaled* value, so
/// stepping below 0 or above 1 saturates rather than wrapping. [verified against retail]
pub const POWER_NOTCHES: i32 = 6;

impl CombatState {
    /// One press of the four gauge keys: `CombatDecreaseAttackPower`/`CombatIncreaseAttackPower`
    /// (melee) and `CombatDecreaseMissileAccuracy`/`CombatIncreaseMissileAccuracy` (missile).
    ///
    /// **The four are one control set**, which is the combat-action handler's own shape: `0x1000005B`
    /// and `0x100000EF` share a `case`, as do `0x1000005C` and `0x100000F0`, and the shipped
    /// defaults bind both pairs to `DIK_INSERT` and `DIK_PRIOR`. The mode picks which map is
    /// registered; the arithmetic is identical.
    ///
    /// Returns the new value, which is the desired-attack-power-changed notice's argument.
    pub fn adjust_ui_requested_power(&mut self, increase: bool) -> f32 {
        // The client's float-to-int truncation wraps out of range; a Rust `as i32` saturates instead, which is why
        // `dereth_primitives::num::to_i32` exists.
        let notch = dereth_primitives::num::to_i32(
            (self.ui_requested_power + 0.5 / (POWER_NOTCHES as f32)) * (POWER_NOTCHES as f32),
        );
        let stepped = notch + if increase { 1 } else { -1 };
        #[allow(clippy::cast_precision_loss)] // seven notches; a small int converts exactly
        let v = (stepped as f32) * (1.0 / (POWER_NOTCHES as f32));
        self.ui_requested_power = v.clamp(0.0, 1.0);
        self.ui_requested_power
    }

    /// The client's element-message-`0x0A` arm — the combat
    /// window's power **scrollbar**, whose position arrives in thousandths.
    ///
    /// Treat the position as unsigned, multiply by `0.001`, and clamp the result to `[0, 1]`.
    /// The original signed load adds `2^32` for
    /// negative inputs, which is why this interface takes an unsigned position.
    pub fn set_ui_requested_power_from_scrollbar(&mut self, position: u32) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        // signed load then the unsigned fixup, exactly as above
        let v = (position as f32) * 0.001;
        self.ui_requested_power = v.clamp(0.0, 1.0);
        self.ui_requested_power
    }
}

impl World {
    /// The default combat mode for what the player holds.
    ///
    /// Returns the mode plus, when it refuses because a non-caster is held, the message the client
    /// would print (suppressed when `quiet`).
    #[must_use]
    pub fn get_default_combat_mode(&self, quiet: bool) -> (CombatMode, Option<String>) {
        let Some(player) = self.player else {
            return (CombatMode::NonCombat, None);
        };
        let Some(inv) = self.inventory(player) else {
            return (CombatMode::Melee, None); // unarmed
        };
        if let Some(id) = inv.object_at_location(loc::WEAPON, 0) {
            let Some(it) = self.weenie(id) else {
                return (CombatMode::NonCombat, None);
            };
            if it.pwd.combat_use == Some(combat_use::MISSILE) {
                return (CombatMode::Missile, None);
            }
            return (CombatMode::Melee, None);
        }
        if let Some(id) = inv.object_at_location(loc::HELD, 0) {
            let Some(it) = self.weenie(id) else {
                return (CombatMode::NonCombat, None);
            };
            if it.inq_type() & item_type::CASTER != 0 {
                return (CombatMode::Magic, None);
            }
            let msg = if quiet {
                None
            } else {
                Some(format!(
                    "You can't enter combat mode while wielding the {}",
                    it.object_name(crate::weenie::NameType::Appropriate)
                ))
            };
            return (CombatMode::NonCombat, msg);
        }
        (CombatMode::Melee, None) // unarmed
    }

    /// Whether the player's equipped inventory mask permits the requested combat mode.
    #[must_use]
    pub fn compatible_combat_mode(&self, mode: CombatMode) -> bool {
        let m = self.inventory_mask;
        match mode {
            CombatMode::Missile => m & loc::MISSILE_WEAPON != 0,
            CombatMode::Magic => m & loc::HELD != 0,
            // Melee fails only when there is no melee or two-handed weapon *and* something is in
            // the weapon-ready slot — i.e. you are holding a bow but asked for melee.
            CombatMode::Melee => {
                !(m & loc::MELEE_WEAPON == 0
                    && m & loc::TWO_HANDED == 0
                    && m & loc::WEAPON_READY_SLOT != 0)
            }
            CombatMode::NonCombat | CombatMode::Undef => true,
        }
    }

    /// Behavior: the selection, unless it is missing or owned by the player.
    #[must_use]
    pub fn get_attack_target(&self) -> Option<ObjectId> {
        let id = self.selected?;
        self.weenie(id)?;
        if self.is_owned_by_player(id) {
            return None;
        }
        Some(id)
    }

    /// Whether the selected object is eligible for attack.
    ///
    /// Not simply `player != id && is_creature(id)`: there is PK logic, and the client answers
    /// *true* for the player and for id 0. It has three callers (the melee and missile attack
    /// gates and the use path's tail).
    ///
    /// The whole function, in its own order:
    ///
    /// ```text
    /// if id == 0: true                                          // nothing selected
    /// if id == player: true                                     // yourself
    /// w = weenie(id), or false
    /// if w's type lacks 0x10: false                             // not ITEM_TYPE::Creature
    /// if w's bitfield has 0x200000: true
    /// me = the player's weenie, or false
    /// if me's bitfield has 0x200000: true
    /// if w is not a player:
    ///     if w has a pet owner: false
    ///     else: w's bitfield bit 4                              // BF_ATTACKABLE
    /// if w.is_pk() && me.is_pk(): true
    /// if w.is_pk_lite() && me.is_pk_lite(): true
    /// false
    /// ```
    ///
    /// **The two `0x200000` tests read through [`crate::weenie::bitfield::IMPENETRABLE`], and that
    /// symbol names bit 21, despite the different free-PK-status label in the original enum.**
    /// The mask is unambiguous; this documents the naming discrepancy without changing behavior.
    ///
    /// `is_creature` is not used for the type test on purpose: the client writes
    /// `type & 0x10` inline here. The helper uses the same mask, but keeping
    /// the mask visible is what makes the line comparable with the retail behaviour above.
    /// \[verified\]
    #[must_use]
    pub fn object_is_attackable(&self, id: ObjectId) -> bool {
        use crate::weenie::{bitfield, item_type};
        if id.0 == 0 || self.player == Some(id) {
            return true;
        }
        let Some(w) = self.weenie(id) else {
            return false;
        };
        if w.inq_type() & item_type::CREATURE == 0 {
            return false;
        }
        if w.pwd.bitfield & bitfield::IMPENETRABLE != 0 {
            return true;
        }
        let Some(me) = self.player.and_then(|p| self.weenie(p)) else {
            return false;
        };
        if me.pwd.bitfield & bitfield::IMPENETRABLE != 0 {
            return true;
        }
        if w.is_player() {
            return (w.is_pk() && me.is_pk()) || (w.is_pk_lite() && me.is_pk_lite());
        }
        if w.pwd.pet_owner.is_some_and(|o| o.0 != 0) {
            return false;
        }
        w.pwd.bitfield & bitfield::ATTACKABLE != 0
    }

    /// Player readiness for attack or a combat-mode change,
    /// both flavours.
    ///
    /// A missing player body returns false before the mode-specific checks.
    ///
    /// Thus the four arms are, with `COMBAT_MODE` a **mask** (`NONCOMBAT` 1, `MELEE` 2,
    /// `MISSILE` 4, `MAGIC` 8) and the switch running over `mode - 1`:
    ///
    /// | mode | answer |
    /// |---|---|
    /// | Undefined or unused bit pattern | `false` |
    /// | Noncombat or magic | `!motions_pending`, regardless of attack/change flavor |
    /// | Melee | Require a combat-table id; attacks may proceed, mode changes also require no pending motions |
    /// | Missile | Require a shipped missile-ready style and [`MOTION_READY`]; mode changes also require no pending motions |
    ///
    /// The two attack sends and power-bar startup use the attack flavor. Direct mode changes
    /// and the pending-mode retry use the stricter mode-change flavor.
    ///
    /// # The two arguments this build has to supply
    ///
    /// `motions_pending` describes the player's body, and `None` means that body is missing.
    /// It is `Option` rather than `bool` because "no body"
    /// and "a body with an empty motion queue" are the *same* answer only in the arms that reach
    /// `motions_pending` at all; in melee with `lenient` they are opposite answers, and this is
    /// the one place that distinction can be made.
    ///
    /// The melee arm originally requires a successful combat-table lookup; readiness is
    /// answered here from the DataID quality alone — see [`Self::combat_table_did`] for the half
    /// of it this build does not model.
    ///
    /// # What the melee arm reads and then throws away
    ///
    /// The original melee arm reads integer quality `0x2F` and defaults it to `0x19` on failure,
    /// and fetches the body's interpreted motion state. **Neither result is used
    /// again**, so neither is transcribed.
    /// `\[verified\]`
    #[must_use]
    pub fn player_in_ready_position(&self, lenient: bool, motions_pending: Option<bool>) -> bool {
        // A missing player body is never ready.
        let Some(pending) = motions_pending else {
            return false;
        };
        self.ready_position_mode_arm(lenient, pending)
    }

    /// Everything the readiness mode switch decides — [`Self::player_in_ready_position`]
    /// **minus** its null-physics-object arm.
    ///
    /// Split out because `dereth-client`'s frame slot reads its body out of a `WorldScene`, which
    /// cannot be built without a `Gpu`, so the null arm is not composable at one of the three
    /// attack call sites. See the declared deviation at that site. Everything below the null
    /// check is the same code for both callers.
    #[must_use]
    pub fn ready_position_mode_arm(&self, lenient: bool, motions_pending: bool) -> bool {
        match self.combat.combat_mode {
            // Non-combat and magic share one arm. `lenient` is not read.
            CombatMode::NonCombat | CombatMode::Magic => !motions_pending,
            // The attack flavor accepts once the table-id gate passes.
            CombatMode::Melee => self.combat_table_did().is_some() && (lenient || !motions_pending),
            // The attack flavor accepts once both motion-state gates pass.
            CombatMode::Missile => {
                MISSILE_READY_STYLES.contains(&self.combat.current_style)
                    && self.combat.forward_command == MOTION_READY
                    && (lenient || !motions_pending)
            }
            // Undefined and unused modes fail the unsigned `mode - 1` range check.
            CombatMode::Undef => false,
        }
    }

    /// Read nonzero data-id quality 4 from the local player's game-object qualities.
    ///
    /// **Stated residual.** The original path then loads the combat table from the portal DAT
    /// and refuses a missing table. `dereth-client-model` has no DAT access, so a character carrying a `CombatTable` DID
    /// that is **not** in `client_portal.dat` reads ready here and would not in retail. The
    /// shipped range is `0x30000000`-`0x3000004D` (71 files); every character the server hands out
    /// carries one of them, so the two answers differ only for a malformed weenie.
    #[must_use]
    pub fn combat_table_did(&self) -> Option<dereth_primitives::DataId> {
        self.player_qualities()?.inq_data_id(COMBAT_TABLE_DID)
    }

    /// Install the login description's qualities: all eight quality tables, including the
    /// combat-table data id.
    ///
    /// # Why this matters
    ///
    /// [`Self::combat_table_did`] above reads the local player's weenie row, and
    /// [`Self::instantiate`] gives that row an **empty** `Qualities` at `CreateObject` — which is
    /// right, because `0xF745` carries a `PublicWeenieDesc` and no quality tables. The tables come
    /// in `0x0013 Login_PlayerDescription`, the only message that carries them. If they stop
    /// short of this row, `inq_data_id(4)` answers `None` for the whole session and
    /// the readiness check's melee table-id gate
    /// answers `false` for ever: no `0x0008` on the wire from
    /// `execute_attack`, no power-bar build from `attempt_start_building_attack`,
    /// and no `0x0053`, whose per-frame retry is
    /// gated on the same predicate and therefore parks the mode change permanently — melee
    /// does not work and the player is stuck in combat mode.
    ///
    /// # One quality record, not two
    ///
    /// Retail has no partition between public and private quality updates: private updates
    /// substitute the local player id and call the same quality writer as public updates. Both
    /// maintain the player object's one quality record. The private form reaches
    /// `apply_player_quality_update` here.
    ///
    /// The tables **replace** rather than merge, exactly as `Qualities::apply_property_tables`
    /// treats a cleared header bit.
    /// Returns how many DataIDs landed, for the caller's log line.
    pub fn apply_player_desc_qualities(
        &mut self,
        q: &dereth_protocol::types::qualities::AcQualities,
        now: dereth_primitives::LocalTime,
    ) -> usize {
        let mut desc = crate::Qualities::new();
        desc.apply_ac_qualities(q, now);
        let dids = desc
            .dids
            .as_ref()
            .map_or(0, std::collections::BTreeMap::len);
        self.set_player_desc(desc);
        dids
    }

    /// Park a `PlayerDesc` and install it on the player's row — everything
    /// the description decoder's installation step, using already-decoded qualities.
    ///
    /// Public because it is the one seam a test can seed a logged-in player through without
    /// hand-building a `0x0013` blob, and because seeding the weenie row *directly* can leave
    /// tests green against a state the production path cannot produce. Going through here means
    /// a test's player is a player the login path could have made.
    pub fn set_player_desc(&mut self, desc: crate::Qualities) {
        self.player_desc = Some(desc);
        self.install_player_desc();
    }

    /// A logged-in player with a `PlayerDesc`, in one call: the identity, the weenie row and the
    /// description, in the order the wire delivers them.
    ///
    /// A `PlayerDesc` with no weenie behind it is a state the original client cannot reach,
    /// since the player description is allocated for the player's object. Tests that need a
    /// described player go through here so that what they assert against is a state the login
    /// path produces.
    pub fn seed_player_desc(&mut self, player: ObjectId, desc: crate::Qualities) {
        self.player = Some(player);
        if self.tables.weenies.get(player).is_none() {
            self.tables
                .weenies
                .insert(player, crate::weenie::Weenie::new(player));
        }
        self.set_player_desc(desc);
    }

    /// Drop the parked login description. Retail's teardown releases the `PlayerDesc` with the
    /// weenie; this is only the park,
    /// which must not survive into the next character's row.
    pub fn clear_player_desc(&mut self) {
        self.player_desc = None;
    }

    /// The client's **release** half, plus the park: the player
    /// has gone, so its quality record goes with it.
    ///
    /// If the player has a quality record it is released and the pointer cleared.
    ///
    /// The original reset reaches the same state by tearing down the whole object table; this
    /// build keeps the row and drops the `PlayerDesc` off it, which is the part every reader of
    /// the player's qualities can tell apart. After it,
    /// [`Self::apply_player_quality_update`]'s null test refuses to store -- so a `0x02CD` that
    /// arrives after a logoff changes nothing.
    pub fn release_player_desc(&mut self) {
        self.player_desc = None;
        if let Some(player) = self.player {
            if let Some(w) = self.tables.weenies.get_mut(player) {
                w.qualities = None;
            }
        }
    }

    /// Copy the parked description onto the local player's row, if it exists, then refresh the
    /// purse. Called after description arrival, player assignment and player-object creation.
    /// The quality tables replace the previous record rather than merging with it.
    pub(crate) fn install_player_desc(&mut self) {
        let Some(player) = self.player else { return };
        let Some(desc) = self.player_desc.clone() else {
            return;
        };
        let Some(w) = self.tables.weenies.get_mut(player) else {
            return;
        };
        w.qualities = Some(desc);
        // The purse is a pull, and this is one of the two moments the thing it pulls from changes.
        self.update_total_value();
    }

    /// Read the parked login description directly during the window
    /// between `0x0013` and the player's own `0xF745` when the row it belongs on does not exist
    /// yet.
    ///
    /// This is the same quality snapshot `Self::install_player_desc` copies onto the row and
    /// not a second store: it is the one filled, waiting for
    /// its owner to be named. It is exposed so that *"has a description been unpacked"* can be
    /// answered without also asking *"has the weenie that will hold it been created"* -- two
    /// questions the original client never has to tell apart, because it drains
    /// object creation (queue 10) before player description (queue 9), so the row is
    /// always there first.
    ///
    /// A caller must prefer the live player's qualities and fall back to this snapshot.
    /// Once the row exists, later quality updates write it and leave the parked snapshot behind.
    #[must_use]
    pub fn login_player_desc(&self) -> Option<&crate::Qualities> {
        self.player_desc.as_ref()
    }

    /// Change combat mode or park the request until the player is ready.
    ///
    /// **`ready_for_mode_change` uses strict readiness.** Its switch reads the current
    /// combat mode, the mode being left, rather than the requested `mode`.
    ///
    /// Step 2's ready-position failure is **not** an error: it stores the pending combat mode and
    /// the per-frame update retries it. The quality-changed handler for player quality `0x28` is the
    /// authoritative path and always wins over the client's optimistic guess.
    ///
    /// The original order of the tail after the mode-change request is:
    ///
    /// ```text
    /// combat_mode = mode;
    /// update_cursor();
    /// if (tracking_target) update_target_tracking();
    /// if (send_to_server) send_mode_change(combat_mode);
    /// advanced_ui = read_advanced_combat_option();
    /// notify_mode_change(combat_mode);
    /// register_input_maps(combat_mode, old_mode);
    /// if (combat_mode == MELEE || combat_mode == MISSILE) { ...selection fixup... }
    /// if (auto_target_enabled && no attackable target) auto_target();
    /// if (plugin_api_ready) plugin_mode_changed(old, new, send_to_server);
    /// ```
    ///
    /// Note the advanced-combat-mode re-read and the notice are **outside** the `send_to_server`
    /// test: the server-authoritative arm raises them too.
    ///
    /// Where each line is in this build:
    ///
    /// - the cursor update — [`crate::World`] has no cursor; `dereth-client`'s `cursor.rs` runs
    ///   once per frame from `App::frame`, so the
    ///   cursor follows the mode one frame later than retail. Declared deviation, not a gap.
    /// - target tracking — absent, and so is any writer of
    ///   [`CombatState::tracking_target`]; it needs a target-tracking update and the
    ///   `ViewCombatTarget` option side effect, neither of which exists yet.
    /// - `AdvancedCombatUI` — here, below.
    /// - the set-combat-mode notice — this build has no notice bus that
    ///   reaches a UI element, so its four subscribers are driven from one edge on
    ///   `GameView::combat_mode()` in `dereth_ui_screens::screens::gameplay`. Declared deviation.
    /// - the input-map registration — polled once per frame by the client's interaction layer.
    ///   Declared deviation.
    /// - the selection fixup — here, [`Self::combat_mode_fixup_target`].
    /// - automatic target selection — the geometry-dependent tail is
    ///   [`Self::combat_mode_auto_target`], connected by the application's three mode-change paths.
    /// - the Decal plugin entry point — this build has no plugin API.
    ///
    /// # Errors
    /// The refusal text the client would put on the scroll.
    pub fn set_combat_mode(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn crate::NoticeSink,
        mode: CombatMode,
        send_to_server: bool,
        ready_for_mode_change: bool,
        teleport_in_progress: bool,
    ) -> Result<(), String> {
        if mode == self.combat.combat_mode {
            return Ok(());
        }
        if send_to_server {
            if !self.compatible_combat_mode(mode) {
                return Err(format!("You can't enter {} mode", mode.name()));
            }
            if teleport_in_progress {
                return Err("You can't enter combat mode while in portal space".to_string());
            }
            if !ready_for_mode_change {
                self.combat.pending_combat_mode = mode;
                return Ok(());
            }
        }
        self.combat.combat_mode = mode;
        // Retail does **not** clear the pending combat mode here: the only two writes
        // are the store in the not-ready branch above and the session reset. The
        // per-frame retry clears the pending mode after attempting it. The difference is
        // visible on the `send_to_server == false` path (the quality-changed handler, player
        // quality `0x28`), where a server-authoritative mode change must **not** discard the
        // player's own queued request.
        if send_to_server {
            req.send(Request::ChangeCombatMode(CombatChangeCombatMode {
                combat_mode: mode.raw(),
            }));
        }
        // Refresh the advanced-combat option. This is the field's only production writer; six
        // readers gate `power_bar_mode`, the advanced UI's target-free swing and
        // `end_attack_request`'s cap on it.
        //
        // The option reader returns **bit 12 of the first option word**,
        // which is `PLAYER_OPTIONS[12] = ("AdvancedCombatUI", One, 0x1000)` here. [verified]
        // It is re-read on **every** mode change, including the `send_to_server == false` one, so a
        // player who flips the option mid-session gets the new value at the next mode change and
        // not before — which is the client's own behaviour and is why the read is here rather
        // than at the six use sites.
        self.combat.advanced_combat_mode = self.player_system.options.advanced_combat_ui();
        // Mode-change notification — see the
        // doc comment: delivered in this build by the gameplay screen's own mode edge.
        //
        // The input-map registration for `(combat_mode, old_mode)` — polled by the client.
        self.combat_mode_fixup_target(out);
        Ok(())
    }

    /// Repair the selection after changing combat mode.
    ///
    /// **Which mode this reads, and why.** The code reloads the current mode, so this
    /// is the mode being **entered**: the assignment `combat_mode = mode` has already happened
    /// above it. That is the opposite of `set_combat_mode`'s ready check, which runs *before*
    /// the assignment and therefore reads the mode being **left**. Both are reproduced: the caller
    /// takes `ready_for_mode_change` as a parameter and this reads `self.combat.combat_mode` after
    /// the store.
    ///
    /// ```text
    /// selected = current_selection;         // read BEFORE the mode test
    /// if (combat_mode != MELEE && combat_mode != MISSILE) goto plugin;
    /// target = get_attack_target();
    /// player = local_player_id_or_zero;
    /// if (selected != player) {
    ///     if (object_is_attackable(target)) {
    ///         if (selected == target) goto autotarget;
    ///         set_selected_object(target, 0);
    ///     } else {
    ///         set_selected_object(0, 0);
    ///     }
    /// }
    /// autotarget:
    /// if (auto_target_enabled) {
    ///     t = get_attack_target();
    ///     if (t != 0 && object_is_attackable(t)) goto plugin;
    ///     auto_target();
    /// }
    /// ```
    ///
    /// Three details are load-bearing and are reproduced exactly:
    ///
    /// - the guard is on the **selection**, not on the attack target: a player who has himself
    ///   selected keeps that selection through a mode change, because the attack-target lookup
    ///   would answer `None` for it and the fixup would clear it.
    /// - The attackability predicate returns true for id 0, so "nothing selected"
    ///   takes the attackable arm and finds `selected == target == 0`.
    ///
    ///   That fact is **not** why entering combat with an empty selection clears nothing.
    ///   Swapping `is_none_or` for `is_some_and` — i.e. treating a null target as
    ///   *not* attackable — changes no outcome at all, because when `target` is `None` the two
    ///   arms converge: the attackable arm reaches `set_selected_object(None)` through
    ///   `selected != target`, and the unattackable arm calls `set_selected_object(None)`
    ///   directly. The native control flow also joins both arms at one selection-setter call.
    ///   Thus treating a null target as attackable matters to attack execution and attack startup,
    ///   and does **no work here**. The
    ///   line is kept because it is the client's control flow; it is unfalsifiable in this
    ///   function.
    /// - the selection setter is called with `force = 0`, so an unchanged selection raises no
    ///   selection-changed notice.
    ///
    /// Automatic targeting runs in the separate geometry-dependent tail below, with both its
    /// fresh-last-attacker arm and its closest-compass-item fallback.
    fn combat_mode_fixup_target(&mut self, out: &mut dyn crate::NoticeSink) {
        if !matches!(
            self.combat.combat_mode,
            CombatMode::Melee | CombatMode::Missile
        ) {
            return;
        }
        let selected = self.selected;
        // `if (selected != player)`. Written as a nested block rather than an early return
        // because retail's guard covers the selection fixup **only** — `auto_target` runs either
        // way in the separate tail below.
        if selected != self.player {
            let target = self.get_attack_target();
            // `object_is_attackable(target)` — `None` here is the client's `0`, which it
            // answers *true* for.
            if target.is_none_or(|t| self.object_is_attackable(t)) {
                if selected != target {
                    self.set_selected_object(target, false, out);
                }
            } else {
                self.set_selected_object(None, false, out);
            }
        }
    }

    /// The client's automatic-target tail — the block *after* the selection fixup, which
    /// is the whole reason automatic targeting exists.
    ///
    /// With automatic targeting enabled, keep an existing attackable target; otherwise choose one.
    ///
    /// **The mode gate above it covers this too.** A mode other than melee or missile skips both the
    /// fixup and this block, so `auto_target` does not run on entering magic or peace — and the
    /// same is true of its other three call sites: the two defender notifications and the
    /// selection-change notification, each gated on combat mode 2 or 4. All four, so
    /// **automatic targeting is unreachable while casting in retail**; what the mode compare admits
    /// while casting is the selection cycle's compass-item arm, which the three
    /// `Selection*CompassItem` actions reach in any mode. Both are asserted.
    ///
    /// **Declared deviation: this is a separate call, one statement later.** Retail runs it inside
    /// the mode change itself; here the geometry seam [`crate::selection::SelectionPhysics`] lives in
    /// `dereth-client`, so `set_combat_mode` cannot reach it without carrying the lookup through
    /// nine parameters and fifteen call sites. `dereth_client::interaction::Interaction`'s
    /// `run_combat_mode_toggle` calls this immediately after `set_combat_mode` returns, in the
    /// same frame with nothing in between — the same shape as the input-map registration.
    /// It is guarded there on the mode having actually **changed**, because retail reaches
    /// this tail only by falling through the assignment, and three of
    /// `set_combat_mode`'s `Ok` returns do not.
    ///
    /// The toolbar toggle and authoritative player-quality handler run this tail in
    /// `dereth-client`. The pending-ready retry supplies it to
    /// [`Self::combat_use_time_with_mode_change`], so it runs before the per-frame retry clears
    /// the pending request, just as the call inside retail's mode change does.
    ///
    /// Returns whether `auto_target` ran, so the caller can count it rather than assume it.
    pub fn combat_mode_auto_target(
        &mut self,
        phys: &dyn Fn(ObjectId) -> Option<crate::selection::SelectionPhysics>,
        radar_radius: f32,
        now: LocalTime,
        out: &mut dyn crate::NoticeSink,
    ) -> bool {
        // the same gate the fixup is under, re-read because this is a second call.
        if !matches!(
            self.combat.combat_mode,
            CombatMode::Melee | CombatMode::Missile
        ) {
            return false;
        }
        // Behavior: the character option. The whole reader is
        // `return (options >> 0xd) & 1`, i.e. **bit 13 of the first option word**, which is
        // `PLAYER_OPTIONS[13] = ("AutoTarget", ...)` here — the same shape as
        // `AdvancedCombatUI`'s bit 12, read the same way. [verified]
        if !self.player_system.options.auto_target() {
            return false;
        }
        // The client computes the attack target twice and tests the two results separately; both
        // calls are pure and cannot disagree between them, so one is faithful to the value and
        // the double call is noted rather than reproduced.
        let keep = self.get_attack_target();
        if keep.is_some_and(|t| self.object_is_attackable(t)) {
            return false;
        }
        self.auto_target(phys, radar_radius, now, out);
        true
    }

    /// Behavior: re-select the thing that last hit you, or
    /// take the closest compass item.
    ///
    /// Read last-attacker quality `0x0B`. A nonzero id must be less than 15 seconds old, still
    /// present and not being removed. Otherwise select the closest compass item.
    ///
    /// Three details that are easy to get wrong:
    ///
    /// * **The 15-second window is strictly less than 15.** The fallback is taken exactly when
    ///   `cur_time - last_attacked_time` is **not** less than 15.0. At exactly 15.0 the attacker is
    ///   *not* re-selected. Asserted on both sides.
    /// * **The fallback arguments are** `(exclude_own_wielded = 0, kind = 2 (compass item),
    ///   ignore_current = 1, closer = 1)`. So **auto-target is
    ///   literally "select the closest compass item"** and shares every filter with `0x1000002F`.
    /// * **The re-select arm passes `force = 0`**, so re-selecting the attacker you
    ///   already have raises no `SelectionChanged`.
    ///
    /// [`CombatState::last_attacked_time`]'s only writer is the defender handlers' shared tail,
    /// [`Self::defender_notification_auto_target`].
    ///
    /// **What follows from where that writer sits.** The stamp is the step immediately before
    /// the auto-target call in both defender-notification paths, so either always reaches
    /// this function with an elapsed time of **zero** and therefore always takes the re-select arm
    /// when there is an attacker to re-select. The `>= 15.0` fallback leg is reachable only from
    /// the *other* two call sites — the mode-change and selection-change paths — some seconds
    /// after the last hit.
    pub fn auto_target(
        &mut self,
        phys: &dyn Fn(ObjectId) -> Option<crate::selection::SelectionPhysics>,
        radar_radius: f32,
        now: LocalTime,
        out: &mut dyn crate::NoticeSink,
    ) {
        let attacker = self.last_attacker();
        let fresh = now.0 - self.combat.last_attacked_time < 15.0;
        let alive = self.weenie(attacker).is_some_and(|w| !w.being_removed);
        if attacker.0 != 0 && fresh && alive {
            self.set_selected_object(Some(attacker), false, out);
            return;
        }
        self.select_next(
            true,
            true,
            crate::selection::SelectionType::CompassItem,
            false,
            phys,
            radar_radius,
            out,
        );
    }

    /// **The tail both defender-notification handlers share, and the only writer of
    /// [`CombatState::last_attacked_time`].**
    ///
    /// Both defender notification paths timestamp the hit, then conditionally auto-target.
    /// Three details affect behavior:
    ///
    /// * **The stamp is unconditional.** Message squelching controls only the displayed text;
    ///   a squelched attack still updates the clock and can auto-target.
    /// * **The mode test is a compare, not a mask** (equal to 2, else equal to 4), the
    ///   same shape as [`Self::combat_mode_auto_target`]'s. All four automatic-target call sites
    ///   require melee or missile mode, making automatic targeting unreachable while casting.
    /// * **The selection gate here requires no selection.** The mode-change path instead asks
    ///   for the attack target ([`Self::get_attack_target`]) twice and runs
    ///   [`Self::object_is_attackable`] on the answer; these two ask only whether *anything at all* is
    ///   selected. A selected non-attackable object therefore blocks the defender path and does
    ///   **not** block the mode-change path. Kept as two functions for that reason.
    ///
    /// **Declared deviation: this is called one step later than retail calls it, and from a
    /// different file.** Retail runs the whole tail inside the two handlers reached while
    /// draining the network-message queue.
    /// `auto_target` needs [`crate::selection::SelectionPhysics`], which lives in `dereth-client`, so
    /// `dereth_client::interaction::apply_events` records the notification and
    /// `dereth_client::interaction::use_time` runs this — **the same `App::frame`**, with the frame's
    /// own geometry snapshot rather than the previous frame's, and still before any input action is
    /// dispatched. The same seam and the same reasoning as [`Self::combat_mode_auto_target`].
    ///
    /// Returns whether `auto_target` ran, so the caller counts it rather than assuming it.
    pub fn defender_notification_auto_target(
        &mut self,
        phys: &dyn Fn(ObjectId) -> Option<crate::selection::SelectionPhysics>,
        radar_radius: f32,
        now: LocalTime,
        out: &mut dyn crate::NoticeSink,
    ) -> bool {
        // Stamp the current timer value; this is
        // the only production writer of the field [`Self::auto_target`]'s 15-second window reads.
        self.combat.last_attacked_time = now.0;
        // Mode equal to 2 (melee) or 4 (missile), compared exactly.
        if !matches!(
            self.combat.combat_mode,
            CombatMode::Melee | CombatMode::Missile
        ) {
            return false;
        }
        // Automatic targeting is bit 13 of the first option word.
        if !self.player_system.options.auto_target() {
            return false;
        }
        // An empty selection may trigger targeting. `None` and
        // `Some(ObjectId(0))` are the same zero to the client, so both take the arm.
        if self.selected.is_some_and(|id| id.0 != 0) {
            return false;
        }
        self.auto_target(phys, radar_radius, now, out);
        true
    }

    /// Whether auto-repeat is enabled and either an attack is running or the combat bar is active.
    ///
    /// The option is the **first** test and it dominates: with `AutoRepeatAttack` off this is
    /// `false` however the power bar is charging, which is why `EscapeKey`'s cascade reaches the
    /// selection legs at all for a player who has the option off.
    #[must_use]
    pub fn repeat_attack_in_progress(&self) -> bool {
        self.player_system.options.auto_repeat_attack()
            && (self.combat.attack_in_progress
                || self.combat.power_bar_mode == PowerBarMode::Combat)
    }

    /// Handle a selection change for attack cancellation and automatic targeting.
    /// Like the mode-change path, this can reach the 15-second fallback without first resetting
    /// the last-attacked timestamp.
    ///
    /// **Three details, each of which decides behaviour:**
    ///
    /// * **The target-willingly-lost flag is clear-once, and the clear is on the *refusing* leg.**
    ///   The first selection change after `EscapeKey` deselected on purpose is consumed by the
    ///   flag and does **not** auto-target; the flag is cleared by that same notice, so the
    ///   *second* one does. A test that raises one notice cannot tell this from "the flag blocks
    ///   for ever", and a test that never sets the flag cannot tell it from "there is no flag".
    ///   The one writer of the `true` is the client's `EscapeKey` arm
    ///   (it sets the flag to 1).
    /// * **The head aborts the automatic attack when a repeat attack is in progress**, inlined. Both
    ///   predicates and the whole body match their standalone functions, so
    ///   this calls the two functions rather than repeating them.
    /// * **The selection gate is "nothing selected"**, the same one the two defender notifications
    ///   ask and *not* `set_combat_mode`'s `get_attack_target`/`object_is_attackable` pair.
    ///
    /// **Why this site and not the others reaches the 15-second leg.** The defender notifications
    /// stamp the last-attacked time immediately before automatic targeting, so they arrive at
    /// [`Self::auto_target`]'s freshness test with an elapsed time of **zero** and can never take
    /// the fallback. This site stamps nothing, so it reaches the test with whatever the clock says
    /// — and once fifteen seconds have passed since the last hit, an unselect is answered by
    /// selecting the next (closest compass) item instead of by re-selecting the attacker.
    ///
    /// **Declared deviation, the same one [`Self::defender_notification_auto_target`] carries.**
    /// Retail dispatches selection-change subscribers synchronously when the selected id actually
    /// changes; `auto_target` needs
    /// `crate::selection::SelectionPhysics`, which lives in `dereth-client`, so
    /// `dereth_client::interaction` counts the notice as it is absorbed and runs this from
    /// `use_time` in the same frame.
    ///
    /// Returns whether `auto_target` ran, so the caller counts it rather than assuming it.
    pub fn on_selection_changed(
        &mut self,
        phys: &dyn Fn(ObjectId) -> Option<crate::selection::SelectionPhysics>,
        radar_radius: f32,
        now: LocalTime,
        req: &mut dyn RequestSink,
        out: &mut dyn crate::NoticeSink,
    ) -> bool {
        // The target-tracking update is not transcribed in this build; it is the
        // tracking-target half of the vivid target indicator and has no counterpart here.
        // Named so the omission is falsifiable.
        //
        // Cancel an automatic attack that is still
        // running, because the thing it was running against has just stopped being selected.
        //
        // **Inherited deviation, declared here because this arm is a caller of it.**
        // Retail gates its power-bar hide on a build being in progress and the bar mode being
        // `PBM_COMBAT`, and [`Self::abort_automatic_attack`] checks only the
        // mode. So an abort with no build in progress hides a power bar retail leaves alone. It is
        // not repaired here because that function has three other callers.
        if self.repeat_attack_in_progress() {
            self.abort_automatic_attack(req);
        }
        // A nonzero selection stops automatic targeting. `None` and `Some(ObjectId(0))`
        // both mean no selection, exactly as in `defender_notification_auto_target`.
        if self.selected.is_some_and(|id| id.0 != 0) {
            return false;
        }
        // Test the flag, then clear it — clear-once.
        if self.combat.target_willingly_lost {
            self.combat.target_willingly_lost = false;
            return false;
        }
        // Mode equal to 2 (melee) or 4 (missile), compared exactly.
        if !matches!(
            self.combat.combat_mode,
            CombatMode::Melee | CombatMode::Missile
        ) {
            return false;
        }
        // Automatic targeting is bit 13 of the first option word.
        if !self.player_system.options.auto_target() {
            return false;
        }
        // All guards passed; the original path tail-calls automatic targeting.
        self.auto_target(phys, radar_radius, now, out);
        true
    }

    /// Advance the power bar's per-frame charge and turn a *click* into an attack.
    ///
    /// **The attack control is a click, not a hold**: a single click starts the power bar
    /// charging, and the attack goes off when it reaches the requested spot. The mechanism is
    /// here rather than at either trigger: the per-frame update is a **three-part** function, and
    /// this is its first two parts; the third is the pending-combat-mode retry,
    /// [`Self::combat_use_time`]. Without these parts nothing advances the bar to the requested
    /// level, so [`Self::end_attack_request`]'s `ui_requested_power <= level` arm would be the
    /// only live producer of an attack — which is a hold, and is wrong.
    ///
    /// Combat-bar modes 1 and 2 can fire after the readiness, request and advanced-mode checks.
    /// A level below the requested power keeps charging; equality counts as arrival. Jump mode
    /// only reports its level, and automatic attack builds stop at arrival without sending again.
    ///
    /// So the attack fires **on arrival**, from the frame loop, at
    /// [`CombatState::requested_attack_height`] — never at an input edge — and only when the build
    /// is not the automatic one. `current_build_is_automatic` is set by
    /// [`Self::handle_attack_done`]'s `AutoRepeatAttack` restart, and its arm here stops the bar
    /// instead of swinging: while the checkbox is on the *server* keeps swinging (the client sent
    /// a sustained `TargetedMeleeAttack` and stops it with `Request::CancelAttack`), and this bar is
    /// the cosmetic re-run. That is why an automatic build must not fire.
    ///
    /// The not-ready arm and the `attempt_start_building_attack` restart are the rest
    /// of the same read. Returns `execute_attack`'s refusal text when it ran and refused, for the
    /// caller to put on the scroll, exactly as `combat_use_time` returns `set_combat_mode`'s.
    /// [verified against retail]
    pub fn combat_power_bar_use_time(
        &mut self,
        req: &mut dyn RequestSink,
        ready_for_attack: bool,
        now: LocalTime,
    ) -> Option<&'static str> {
        let mut refusal = None;
        if self.combat.build_in_progress {
            let level = self.combat.power_bar_level(now);
            match self.combat.power_bar_mode {
                // `0 < mode && mode < 3`, as signed ints — the two combat modes.
                PowerBarMode::Combat | PowerBarMode::AdvancedCombat => {
                    if ready_for_attack {
                        if self.combat.attack_request_in_progress
                            || self.combat.advanced_combat_mode
                            || level < self.combat.requested_attack_power
                        {
                            // Still charging: the bar follows the clock and nothing swings.
                            self.combat.set_power_bar_level(level);
                        } else {
                            // Arrived. The client's `min` returns the *first* argument unless
                            // the second is strictly smaller, and on this arm
                            // `level >= requested`, so the bar is pinned at the requested level
                            // and not at the clock's.
                            let pinned = self.combat.requested_attack_power.min(level);
                            self.combat.set_power_bar_level(pinned);
                            if self.combat.current_build_is_automatic {
                                self.combat.build_in_progress = false;
                                self.combat.build_start_time = 0.0;
                            } else {
                                let h = self.combat.requested_attack_height;
                                refusal = self.execute_attack(req, h, true, ready_for_attack);
                            }
                        }
                    } else {
                        // `player_in_ready_position` went false mid-charge. The cancel is gated on
                        // the **option**, not on `repeat_attacking`, which is the client's own
                        // asymmetry and is reproduced.
                        if self.player_system.options.auto_repeat_attack() {
                            req.send(Request::CancelAttack(CombatCancelAttack));
                            self.combat.repeat_attacking = false;
                        }
                        if self.combat.attack_request_in_progress {
                            self.combat.build_in_progress = false;
                            self.combat.build_start_time = 0.0;
                            self.combat.set_power_bar_level(0.0);
                        } else {
                            self.combat.hide_power_bar();
                        }
                    }
                }
                // `mode == PBM_JUMP` — the jump charge only follows the clock; the
                // release belongs to the jump itself, not this function.
                PowerBarMode::Jump => self.combat.set_power_bar_level(level),
                // `PBM_UNDEF` fails `0 < mode`; `PBM_DDD` is the dat-patch widget and fails
                // both `< 3` and `== 3`.
                PowerBarMode::Undef | PowerBarMode::Ddd => {}
            }
        }
        // a *held* request whose build has ended restarts it. This is what makes the
        // advanced UI's hold re-arm, and it runs whether or not the block above did anything.
        if self.combat.attack_request_in_progress
            && !self.combat.build_in_progress
            && !self.combat.attack_server_response_pending
        {
            self.attempt_start_building_attack(ready_for_attack, now);
        }
        refusal
    }

    /// Behavior: its tail — the **only** consumer of
    /// [`CombatState::pending_combat_mode`] in the client, and the reason a combat-mode toggle
    /// pressed while the body is busy is not lost:
    ///
    /// ```text
    /// if (pending_combat_mode != UNDEF_COMBAT_MODE) {
    ///   if (player_in_ready_position(false)) {
    ///     set_combat_mode(pending_combat_mode, true);
    ///     pending_combat_mode = UNDEF_COMBAT_MODE;
    ///   }
    /// }
    /// ```
    ///
    /// Three details are load-bearing and are reproduced exactly:
    ///
    /// - the retry is **one-shot**. The clear is outside `set_combat_mode`, so a mode that has
    ///   become incompatible in the meantime (the bow was stowed) is refused and then **dropped**,
    ///   not retried for ever.
    /// - it is gated on `player_in_ready_position(false)` — the *strict* form, the same one
    ///   `set_combat_mode` used to refuse in the first place, not the `true` form the power bar uses.
    /// - nothing is queued: the pending mode holds **one** mode, so a second toggle before the
    ///   body is ready overwrites the first and only one `Combat_ChangeCombatMode` is ever sent.
    ///
    /// Call once per frame, after the frame's input has been dispatched. Returns the refusal text
    /// the client would put on the scroll (from inside `set_combat_mode`), if the retry
    /// refused. This geometry-free entry point does not run `auto_target`; the application uses
    /// [`Self::combat_use_time_with_mode_change`] to include that tail before clearing pending.
    pub fn combat_use_time(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn crate::NoticeSink,
        ready_for_mode_change: bool,
    ) -> Option<String> {
        self.combat_use_time_with_mode_change(req, out, ready_for_mode_change, |_, _| {})
    }

    /// [`Self::combat_use_time`] with the application's geometry-dependent mode-change tail.
    ///
    /// The per-frame retry clears the pending mode only after the mode change returns,
    /// including automatic targeting and its selection effects. The callback therefore runs
    /// before that clear, and only after a real mode change: an equality return, not-ready frame
    /// or compatibility refusal must not reach the mode change's tail. Keeping the callback inside
    /// the retry also preserves the final clear if a tail handler changes the pending field.
    pub fn combat_use_time_with_mode_change(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn crate::NoticeSink,
        ready_for_mode_change: bool,
        on_mode_change: impl FnOnce(&mut Self, &mut dyn crate::NoticeSink),
    ) -> Option<String> {
        if self.combat.pending_combat_mode == CombatMode::Undef || !ready_for_mode_change {
            return None;
        }
        let mode = self.combat.pending_combat_mode;
        let before = self.combat.combat_mode;
        let refusal = self
            .set_combat_mode(req, out, mode, true, true, false)
            .err();
        if self.combat.combat_mode != before {
            on_mode_change(self, out);
        }
        self.combat.pending_combat_mode = CombatMode::Undef;
        refusal
    }

    /// Toggle from peace to the default combat mode, and from any other mode to peace.
    #[must_use]
    pub fn toggle_combat_mode_target(&self, quiet: bool) -> (CombatMode, Option<String>) {
        if self.combat.combat_mode == CombatMode::NonCombat {
            self.get_default_combat_mode(quiet)
        } else {
            (CombatMode::NonCombat, None)
        }
    }

    /// Start an attack request.
    ///
    /// **`ready_for_attack` is `player_in_ready_position(true)`.** Retail's attack-request start does
    /// not call it itself; its tail delegates to the ready-position check with `true`.
    ///
    /// In **advanced** combat mode the target check is skipped entirely, which is why the advanced
    /// UI can start a swing with nothing selected.
    ///
    /// # Errors
    /// The refusal text when a valid target is required and absent.
    pub fn start_attack_request(
        &mut self,
        ready_for_attack: bool,
        now: LocalTime,
    ) -> Result<(), &'static str> {
        if !self.combat.advanced_combat_mode {
            let target = self.get_attack_target();
            if target.is_none_or(|t| !self.object_is_attackable(t)) {
                return Err("You must select a valid combat target before attacking");
            }
        }
        self.combat.attack_request_in_progress = true;
        // Charge to full; the cap is applied on release.
        self.combat.requested_attack_power = 1.0;
        self.combat.current_build_is_automatic = false;
        self.attempt_start_building_attack(ready_for_attack, now);
        Ok(())
    }

    /// Set attack height, starting a request only for a changed height or a new press.
    ///
    /// ```text
    /// old = requested_attack_height;
    /// requested_attack_height = h;
    /// if (old != h || attack_request_in_progress == false) start_attack_request();
    /// ```
    ///
    /// **The guard is why holding an attack-height key does not refuse once per frame.** The repeat
    /// sweep re-delivers the same action with the same height while `attack_request_in_progress` is
    /// already set, and the client does nothing at all.
    ///
    /// It is *not* what stops the power bar restarting. Attack-build startup already returns
    /// when a build is in progress, so a build in progress survives a
    /// repeated `start_attack_request` with or without this guard. What the guard stops is the
    /// client's **target check**, whose failure arm prints
    /// "You must select a valid combat target before attacking" on channel `0x1A` — so
    /// without it, holding a key with nothing attackable selected prints that line once per frame.
    /// A test of this must assert the printed line, not the charge level, which cannot see it.
    ///
    /// Its two callers are the client's three height cases and
    /// the client's `0x1C` (mouse-press) arm on the three
    /// attack-height buttons, so the key and the button are the same act.
    ///
    /// # Errors
    /// `start_attack_request`'s refusal text, when it ran and refused.
    pub fn set_requested_attack_height(
        &mut self,
        height: AttackHeight,
        ready_for_attack: bool,
        now: LocalTime,
    ) -> Result<(), &'static str> {
        let old = self.combat.requested_attack_height;
        self.combat.requested_attack_height = height;
        if old != height || !self.combat.attack_request_in_progress {
            return self.start_attack_request(ready_for_attack, now);
        }
        Ok(())
    }

    /// Try to start building an attack.
    ///
    /// **`ready_for_attack` is `player_in_ready_position(true, …)`** — the lenient flavour, the
    /// first thing the function computes.
    pub fn attempt_start_building_attack(&mut self, ready_for_attack: bool, now: LocalTime) {
        if !ready_for_attack || self.combat.build_in_progress {
            return;
        }
        self.begin_attack_power_bar();
        self.combat.start_power_bar_build(now);
    }

    /// The source's synchronous Begin subscriber reads these facts immediately. Carrying them
    /// with the notice preserves its caption if a later operation changes mode before delivery.
    fn begin_attack_power_bar(&mut self) {
        let mode = if self.combat.advanced_combat_mode {
            PowerBarMode::AdvancedCombat
        } else {
            PowerBarMode::Combat
        };
        // The power-bar display reads player skill 0x32 (Recklessness), not a weapon fact.
        let sac = self.player_qualities().map_or(0, |q| {
            crate::skills::inq_skill_advancement_class(q, 0x32) as u32
        });
        self.combat
            .begin_power_bar(mode, self.combat.combat_mode == CombatMode::Melee, sac);
    }

    /// End an attack request at `height` with `power`: the release.
    ///
    /// **The double execute**: in the classic UI, releasing above the slider
    /// cap fires once at the released power and once more clamped to the cap. Two attack messages
    /// per request; it is packet-log-visible and servers accept it. Preserve it.
    ///
    /// `power == None` means "use what the bar says".
    pub fn end_attack_request(
        &mut self,
        req: &mut dyn RequestSink,
        height: AttackHeight,
        power: Option<f32>,
        ready_for_attack: bool,
        now: LocalTime,
    ) {
        if !self.combat.attack_request_in_progress {
            return;
        }
        self.combat.attack_request_in_progress = false;
        let level = self.combat.power_bar_level(now);
        self.combat.requested_attack_power = match power {
            Some(p) => p,
            None => {
                if self.combat.advanced_combat_mode {
                    level
                } else {
                    level.max(self.combat.ui_requested_power)
                }
            }
        };
        if self.combat.attack_server_response_pending {
            self.combat.attack_when_response_received = true;
            self.combat.attack_when_response_received_power = self.combat.requested_attack_power;
            return;
        }
        if self.combat.advanced_combat_mode
            || level >= self.combat.ui_requested_power
            || self.combat.repeat_attacking
        {
            self.execute_attack(req, height, true, ready_for_attack);
            if !self.combat.advanced_combat_mode
                && self.combat.requested_attack_power > self.combat.ui_requested_power
            {
                self.combat.requested_attack_power = self.combat.ui_requested_power;
                self.execute_attack(req, height, true, ready_for_attack);
            }
        }
    }

    /// Execute an attack at `height`, optionally keeping the request pending.
    ///
    /// **`ready_for_attack` is `player_in_ready_position(true)`**, checked between the
    /// attackable test and the targeted melee/missile attack pair.
    ///
    /// **Nothing is predicted**: the client sends the request and waits.
    pub fn execute_attack(
        &mut self,
        req: &mut dyn RequestSink,
        height: AttackHeight,
        keep_pending: bool,
        ready_for_attack: bool,
    ) -> Option<&'static str> {
        self.combat.build_in_progress = false;
        self.combat.build_start_time = 0.0;
        let mut target = self.get_attack_target();
        if target.is_none_or(|t| !self.object_is_attackable(t)) {
            target = None;
        }
        let mut refusal = None;
        let mut fired = false;
        if ready_for_attack {
            if let Some(t) = target {
                match self.combat.combat_mode {
                    CombatMode::Melee => {
                        req.send(Request::TargetedMeleeAttack(CombatTargetedMeleeAttack {
                            target: t,
                            attack_height: height as u32,
                            power_level: self.combat.requested_attack_power,
                        }));
                        fired = true;
                    }
                    CombatMode::Missile => {
                        req.send(Request::TargetedMissileAttack(
                            CombatTargetedMissileAttack {
                                target: t,
                                attack_height: height as u32,
                                power_level: self.combat.requested_attack_power,
                            },
                        ));
                        fired = true;
                    }
                    _ => {}
                }
                if fired {
                    if self.player_system.options.auto_repeat_attack() {
                        self.combat.repeat_attacking = true;
                    }
                    self.combat.attack_server_response_pending = true;
                }
            }
        }
        if !fired && !(target.is_some() && !ready_for_attack) {
            // Not ready with a target: silently drop. Otherwise the refusal is printed.
            refusal = Some("You must select a valid combat target before attacking");
        }
        if !self.combat.attack_server_response_pending {
            self.combat.hide_power_bar();
        }
        if !keep_pending {
            self.combat.attack_server_response_pending = false;
        }
        refusal
    }

    /// Abort an automatic (repeating) attack: cancel it on the server and hide a combat bar.
    pub fn abort_automatic_attack(&mut self, req: &mut dyn RequestSink) {
        let c = &self.combat;
        if c.attack_server_response_pending
            || c.attack_request_in_progress
            || c.attack_in_progress
            || c.repeat_attacking
        {
            req.send(Request::CancelAttack(CombatCancelAttack));
            self.combat.repeat_attacking = false;
            if self.combat.power_bar_mode == PowerBarMode::Combat {
                self.combat.hide_power_bar();
            }
        }
    }

    /// Behavior: the server's "yes, you swung".
    pub fn handle_commence_attack(&mut self) {
        self.combat.attack_in_progress = true;
        self.combat.attack_server_response_pending = true;
        if !self.combat.attack_request_in_progress {
            if self.combat.power_bar_mode == PowerBarMode::Undef {
                self.begin_attack_power_bar();
            }
            // Snap the bar to what was sent.
            self.combat
                .set_power_bar_level(self.combat.requested_attack_power);
            self.combat.build_in_progress = false;
            self.combat.build_start_time = 0.0;
        }
        // The global the rest of the client reads, and which blocks every inventory request.
        self.attack_in_progress = true;
    }

    /// The attack-done handler, given the result code.
    ///
    /// **`ready_for_attack`**: every arm below it reaches a lenient (`true`) readiness check
    /// (`attempt_start_building_attack`, `start_attack_request`, `end_attack_request` -> `execute_attack`).
    ///
    /// The community catalogue calls the field "unused"; it is not — a non-zero code aborts an
    /// auto-repeat attack; the protocol codec preserves that result field too.
    pub fn handle_attack_done(
        &mut self,
        req: &mut dyn RequestSink,
        result: u32,
        ready_for_attack: bool,
        now: LocalTime,
    ) {
        self.combat.attack_in_progress = false;
        self.combat.attack_server_response_pending = false;
        self.attack_in_progress = false;

        if result != 0 && self.combat.repeat_attacking {
            self.abort_automatic_attack(req);
        }
        let auto_repeat = self.player_system.options.auto_repeat_attack();
        if !self.combat.attack_request_in_progress
            && !self.combat.advanced_combat_mode
            && auto_repeat
            && self.combat.repeat_attacking
            && (self.combat.requested_attack_power - self.combat.ui_requested_power).abs()
                > SLIDER_TOLERANCE
        {
            self.combat.requested_attack_power = self.combat.ui_requested_power;
            let h = self.combat.requested_attack_height;
            self.execute_attack(req, h, false, ready_for_attack);
        }
        if auto_repeat && self.combat.repeat_attacking {
            if !self.combat.attack_request_in_progress {
                self.combat.start_power_bar_build(now);
                self.combat.current_build_is_automatic = true;
            }
        } else {
            self.combat.repeat_attacking = false;
            if self.combat.power_bar_mode != PowerBarMode::Jump {
                self.combat.hide_power_bar();
            }
        }
        if self.combat.attack_request_in_progress
            && !self.combat.build_in_progress
            && !self.combat.attack_server_response_pending
        {
            self.attempt_start_building_attack(ready_for_attack, now);
        }
        if self.combat.attack_when_response_received {
            let p = self.combat.attack_when_response_received_power;
            let h = self.combat.requested_attack_height;
            let _ = self.start_attack_request(ready_for_attack, now);
            self.end_attack_request(req, h, Some(p), ready_for_attack, now);
            self.combat.attack_when_response_received = false;
            self.combat.attack_when_response_received_power = 0.0;
        }
    }

    /// `Request::QueryHealth` — the client never computes another creature's health; it must ask, and
    /// is told a 0..1 fraction.
    pub fn query_health(&mut self, req: &mut dyn RequestSink, object: ObjectId) {
        req.send(Request::QueryHealth(CombatQueryHealth { target: object }));
    }
}

/// Choose singular and plural hit verbs by damage type and health fraction removed.
///
/// `percent` is the fraction of the defender's maximum health removed. Returns `("hit", "hits")`
/// for a negative percent or an unrecognised damage type.
///
/// **The comparisons are `<=`, not `<`**. The first threshold, and every one of
/// the twenty-six others, behaves the same way: the equal case falls in the **lower** bucket,
/// so exactly 0.25 falls in the second bucket, not the third.
/// The three thresholds are 0.1, 0.25 and 0.5, all stored as `f64`.
///
/// The negative guard compares against 0.0, and it
/// returns **without writing either string**, so the caller keeps the `"hit"`/`"hits"` the default
/// arm would have written.
///
/// The dispatch is an exact-value compare chain -- against 0x10 first, then
/// a bounds-checked eight-entry dispatch table; that table sends 3, 5, 6
/// and 7 to the default. A **combined** mask is therefore not decoded to its lowest
/// set bit; it says "hit".
#[must_use]
pub fn combat_hit_adjectives(damage_type: u32, percent: f64) -> (&'static str, &'static str) {
    if percent < 0.0 {
        return ("hit", "hits");
    }
    let bucket = if percent <= 0.10 {
        0
    } else if percent <= 0.25 {
        1
    } else if percent <= 0.50 {
        2
    } else {
        3
    };
    let row: [(&str, &str); 4] = match damage_type {
        damage_type::SLASH => [
            ("scratch", "scratches"),
            ("cut", "cuts"),
            ("slash", "slashes"),
            ("mangle", "mangles"),
        ],
        damage_type::PIERCE => [
            ("nick", "nicks"),
            ("stab", "stabs"),
            ("impale", "impales"),
            ("gore", "gores"),
        ],
        damage_type::BLUDGEON => [
            ("graze", "grazes"),
            ("bash", "bashes"),
            ("smash", "smashes"),
            ("crush", "crushes"),
        ],
        damage_type::COLD => [
            ("numb", "numbs"),
            ("chill", "chills"),
            ("frost", "frosts"),
            ("freeze", "freezes"),
        ],
        damage_type::FIRE => [
            ("singe", "singes"),
            ("scorch", "scorches"),
            ("burn", "burns"),
            ("incinerate", "incinerates"),
        ],
        damage_type::ACID => [
            ("blister", "blisters"),
            ("sear", "sears"),
            ("corrode", "corrodes"),
            ("dissolve", "dissolves"),
        ],
        damage_type::ELECTRIC => [
            ("spark", "sparks"),
            ("shock", "shocks"),
            ("jolt", "jolts"),
            ("blast", "blasts"),
        ],
        damage_type::HEALTH => [
            ("drain", "drains"),
            ("exhaust", "exhausts"),
            ("siphon", "siphons"),
            ("deplete", "depletes"),
        ],
        damage_type::NETHER => [
            ("scar", "scars"),
            ("twist", "twists"),
            ("wither", "withers"),
            ("eradicate", "eradicates"),
        ],
        _ => [("hit", "hits"); 4],
    };
    row[bucket]
}

/// The damage-type text buffer used by both attack notification handlers.
///
/// Formatting totals the size it needs first (1, plus each name, plus one for every `/`) and
/// writes nothing but the terminator when that total exceeds the buffer -- so a mask wide enough
/// to overrun 64 bytes produces an **empty** damage word rather than a truncated one. All nine
/// names need 73 bytes; the eight without `Prismatic` need 63 and fit.
pub const NOTIFICATION_DAMAGE_TYPE_BUFFER: usize = 0x40;

/// Damage-type names joined by `/` in the listed order.
///
/// `HEALTH` (0x80), `STAMINA` (0x100) and `MANA` (0x200) are **not** in the table, so they name
/// nothing at all even though `HEALTH` has its own row in [`combat_hit_adjectives`].
#[must_use]
pub fn damage_type_to_string(mask: u32) -> String {
    const NAMES: [(u32, &str); 9] = [
        (damage_type::SLASH, "Slashing"),
        (damage_type::PIERCE, "Piercing"),
        (damage_type::BLUDGEON, "Bludgeoning"),
        (damage_type::COLD, "Cold"),
        (damage_type::FIRE, "Fire"),
        (damage_type::ACID, "Acid"),
        (damage_type::ELECTRIC, "Electrical"),
        (damage_type::NETHER, "Nether"),
        (damage_type::BASE, "Prismatic"),
    ];
    NAMES
        .iter()
        .filter(|(m, _)| mask & m != 0)
        .map(|(_, n)| *n)
        .collect::<Vec<_>>()
        .join("/")
}

/// Map a body-part id to its display name, replace underscores with spaces and lowercase it.
///
/// `part + 1` is compared **unsigned**, so row 0 is reached only by
/// `part == 0xFFFFFFFF` and its string is `"UNDEFINED"` -- not `"Unknown"`. Rows 11 and 14 are
/// gaps carrying the literal `"Unknown"`, and the lower-case step (`replace("_", " ")`
/// then `_strlwr`) lowers **every** row including those, so the word in the sentence is
/// `"unknown"` with a small `u`.
#[must_use]
pub fn body_part_to_string(v: u32) -> String {
    let name = match v {
        u32::MAX => "UNDEFINED",
        0 => "HEAD",
        1 => "CHEST",
        2 => "ABDOMEN",
        3 => "UPPER_ARM",
        4 => "LOWER_ARM",
        5 => "HAND",
        6 => "UPPER_LEG",
        7 => "LOWER_LEG",
        8 => "FOOT",
        9 => "HORN",
        10 => "FRONT_LEG",
        0x0C => "FRONT_FOOT",
        0x0D => "REAR_LEG",
        0x0F => "REAR_FOOT",
        0x10 => "TORSO",
        0x11 => "TAIL",
        0x12 => "ARM",
        0x13 => "LEG",
        0x14 => "CLAW",
        0x15 => "WINGS",
        0x16 => "BREATH",
        0x17 => "TENTACLE",
        0x18 => "UPPER_TENTACLE",
        0x19 => "LOWER_TENTACLE",
        0x1A => "CLOAK",
        0x1B => "NUM",
        _ => "Unknown",
    };
    name.to_lowercase().replace('_', " ")
}

/// The `%s` immediately before `damage!` in both sentences.
///
/// Lowercase the damage-type text and add a trailing space only when the text is nonempty.
///
/// The gate is on the **string**, not on `damage_type != 0`: `HEALTH`, `STAMINA` and `MANA` have
/// no name, and a mask needing more than [`NOTIFICATION_DAMAGE_TYPE_BUFFER`] bytes leaves the
/// buffer empty too. Gating on the mask would give each of those a stray double space.
#[must_use]
pub fn notification_damage_word(damage_type: u32) -> String {
    let named = damage_type_to_string(damage_type);
    if named.is_empty() || named.len() + 1 > NOTIFICATION_DAMAGE_TYPE_BUFFER {
        return String::new();
    }
    format!("{} ", named.to_lowercase())
}

/// Weapon speed description. Lower time values are faster.
#[must_use]
pub fn weapon_time_to_string(t: i32) -> &'static str {
    if t < 11 {
        "Very Fast"
    } else if t < 31 {
        "Fast"
    } else if t <= 49 {
        "Average"
    } else if t < 80 {
        "Slow"
    } else {
        "Very Slow"
    }
}

/// The attacker notification — the own-combat chat text type (22).
///
/// **`"Critical hit!  "` has two trailing spaces**. It shows in the chat log. The prefixes run
/// critical, overpower, sneak attack, recklessness.
#[must_use]
pub fn attacker_notification_line(
    defender_name: &str,
    damage_type: u32,
    percent: f64,
    damage: u32,
    critical: bool,
    conditions: u32,
) -> String {
    let (verb, _) = combat_hit_adjectives(damage_type, percent);
    let mut s = String::new();
    if critical {
        s.push_str("Critical hit!  ");
    }
    if conditions & attack_conditions::OVERPOWER != 0 {
        s.push_str("Overpower! ");
    }
    if conditions & attack_conditions::SNEAK_ATTACK != 0 {
        s.push_str("Sneak Attack! ");
    }
    if conditions & attack_conditions::RECKLESSNESS != 0 {
        s.push_str("Recklessness! ");
    }
    let type_word = notification_damage_word(damage_type);
    s.push_str(&format!(
        "You {verb} {defender_name} for {damage} point{} of {type_word}damage!",
        if damage == 1 { "" } else { "s" }
    ));
    if conditions & attack_conditions::CRITICAL_PROTECTION != 0 {
        s.push_str(
            " Your target's Critical Protection augmentation allows them to avoid your critical hit!",
        );
    }
    s.push('\n');
    s
}

/// The defender notification — the enemy-combat chat text type (21).
///
/// The prefixes are the **shorter** forms: `"Critical hit! "` (one trailing space) and
/// `"Reckless! "`.
#[must_use]
pub fn defender_notification_line(
    attacker_name: &str,
    damage_type: u32,
    percent: f64,
    damage: u32,
    body_part: u32,
    critical: bool,
    conditions: u32,
) -> String {
    let (_, verbs) = combat_hit_adjectives(damage_type, percent);
    let mut s = String::new();
    if critical {
        s.push_str("Critical hit! ");
    }
    if conditions & attack_conditions::OVERPOWER != 0 {
        s.push_str("Overpower! ");
    }
    if conditions & attack_conditions::SNEAK_ATTACK != 0 {
        s.push_str("Sneak Attack! ");
    }
    if conditions & attack_conditions::RECKLESSNESS != 0 {
        s.push_str("Reckless! ");
    }
    let type_word = notification_damage_word(damage_type);
    // The body-part formatter reports zero only for an empty source. The
    // empty arm builds the sentence from `" you"` instead of `" your "`
    // and the part. `body_part_to_string` is total -- every input names at least
    // `"Unknown"` -- so that arm is unreachable in the shipped client; it is written out so the
    // rule is inspectable rather than lost.
    let part = body_part_to_string(body_part);
    let target = if part.is_empty() {
        " you".to_owned()
    } else {
        format!(" your {part}")
    };
    s.push_str(&format!(
        "{attacker_name} {verbs}{target} for {damage} point{} of {type_word}damage!",
        if damage == 1 { "" } else { "s" }
    ));
    if conditions & attack_conditions::CRITICAL_PROTECTION != 0 {
        s.push_str(" Your Critical Protection augmentation allows you to avoid a critical hit!");
    }
    s.push('\n');
    s
}

/// `0x01B3 Combat_HandleEvasionAttackerNotificationEvent`.
#[must_use]
pub fn evasion_attacker_line(defender_name: &str) -> String {
    format!("{defender_name} evaded your attack.\n")
}

/// `0x01B4 Combat_HandleEvasionDefenderNotificationEvent`.
#[must_use]
pub fn evasion_defender_line(attacker_name: &str) -> String {
    format!("You evaded {attacker_name}!\n")
}

/// The toolbar's two selected-object meters, which are the only place the original client keeps
/// these two fractions.
///
/// Health and mana updates both compare the notice's object id with the toolbar selection,
/// show the meter if hidden and write its level attribute (`0x69`). Neither value is
/// stored per object — which is why this is one pair of `Option<f32>` and not a table.
///
/// A fraction for an object that is **not** selected is therefore dropped, exactly as retail drops
/// it: the guard is the first line of both handlers.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SelectedMeters {
    /// The selected-object health meter's `METER_LEVEL`, 0.0–1.0, from `0x01C0`.
    pub health: Option<f32>,
    /// The selected-object mana meter's `METER_LEVEL`, 0.0–1.0, from `0x0264`.
    pub mana: Option<f32>,
}

impl World {
    /// Receive health-query response `0x01C0`.
    ///
    /// The response updates the selected-object health meter.
    ///
    /// Returns whether the value was kept — that is, whether the object was the selected one.
    /// The client has no other consumer, so a `false` here is retail behaviour and not a loss.
    pub fn update_object_health(&mut self, object: ObjectId, health: f32) -> bool {
        if self.selected != Some(object) {
            return false;
        }
        self.selected_meters.health = Some(health);
        true
    }

    /// Receive item-mana query response `0x0264`.
    ///
    /// `success == 0` takes the response handler's other branch, which
    /// re-sends rather than writing the meter. The argument of that
    /// re-send is not known, so this
    /// rebuild takes the branch and writes nothing rather than inventing a target for it.
    ///
    /// Returns whether the meter was written.
    pub fn update_item_mana(&mut self, object: ObjectId, mana: f32, success: bool) -> bool {
        if self.selected != Some(object) || !success {
            return false;
        }
        self.selected_meters.mana = Some(mana);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RecordingRequests, RecordingSink};
    use dereth_primitives::ServerTime;
    use dereth_protocol::types::PublicWeenieDesc;

    /// The three send sites report level zero, level zero again when the build starts, and then
    /// the requested level. A snapshot with equal
    /// endpoints cannot distinguish this sequence from no transition.
    #[test]
    fn power_bar_journal_preserves_same_frame_edges_and_retail_stale_cache() {
        let mut c = CombatState::begin();
        c.begin_power_bar(PowerBarMode::Combat, false, 0);
        c.start_power_bar_build(LocalTime(1.0));
        c.set_power_bar_level(0.75);
        let initial = c.take_power_bar_notices();
        assert_eq!(
            initial,
            vec![
                PowerBarNotice::SetLevel {
                    mode: PowerBarMode::Combat,
                    level: 0.0
                },
                PowerBarNotice::SetLevel {
                    mode: PowerBarMode::Combat,
                    level: 0.0
                },
                PowerBarNotice::SetLevel {
                    mode: PowerBarMode::Combat,
                    level: 0.75
                },
            ]
        );
        c.hide_power_bar();
        assert_eq!(c.latest_power_bar_level, 0.75);
        assert_eq!(c.power_bar_mode, PowerBarMode::Undef);
        c.begin_power_bar(PowerBarMode::Combat, false, 0);
        c.set_power_bar_level(0.75);
        assert_eq!(c.take_power_bar_notices(), initial);
        assert!(c.take_power_bar_notices().is_empty());
    }

    /// Finishing a jump only sends when `jump_pending` is set, and retains the original
    /// mode argument. Neither `finish_jump` nor `hide_power_bar` changes `latest_power_bar_level`.
    #[test]
    fn finish_jump_journals_original_mode_once_without_zeroing_the_retail_cache() {
        for mode in [
            PowerBarMode::Combat,
            PowerBarMode::AdvancedCombat,
            PowerBarMode::Jump,
        ] {
            let mut c = CombatState::begin();
            c.begin_power_bar(mode, true, 2);
            c.set_power_bar_level(0.625);
            let _ = c.take_power_bar_notices();
            c.finish_jump();
            assert!(
                c.take_power_bar_notices().is_empty(),
                "no jump pending: no Finish notice"
            );
            c.jump_pending = true;
            c.finish_jump();
            assert_eq!(c.latest_power_bar_level, 0.625);
            assert_eq!(c.power_bar_mode, PowerBarMode::Undef);
            assert!(!c.jump_pending);
            assert_eq!(
                c.take_power_bar_notices(),
                vec![if mode == PowerBarMode::Combat {
                    PowerBarNotice::SetLevel { mode, level: 0.0 }
                } else {
                    PowerBarNotice::Finish { mode }
                }]
            );
            c.finish_jump();
            assert!(c.take_power_bar_notices().is_empty());
        }
    }

    /// Begin's synchronous consumer sees the mode and skill at emission, even when
    /// the host delivers after a later mode/quality update in the same batch.
    #[test]
    fn attack_begin_notice_retains_its_original_caption_and_skill_context() {
        let mut w = world();
        let player = w.player.unwrap();
        w.tables.weenies.get_mut(player).unwrap().qualities = Some(crate::Qualities::new());
        w.player_qualities_mut().unwrap().set_skill(
            0x32,
            dereth_protocol::types::Skill {
                sac: 2,
                ..dereth_protocol::types::Skill::default()
            },
        );
        w.combat.advanced_combat_mode = true;
        w.attempt_start_building_attack(true, LocalTime(2.0));
        w.combat.combat_mode = CombatMode::Missile;
        w.player_qualities_mut()
            .unwrap()
            .set_skill_advancement_class(0x32, 1);
        assert_eq!(
            w.combat.take_power_bar_notices(),
            vec![
                PowerBarNotice::Begin {
                    mode: PowerBarMode::AdvancedCombat,
                    melee: true,
                    recklessness_sac: 2,
                },
                PowerBarNotice::SetLevel {
                    mode: PowerBarMode::AdvancedCombat,
                    level: 0.0
                },
            ]
        );
    }

    fn world() -> World {
        let mut w = World::new();
        w.set_player(ObjectId(1));
        let mut p = crate::weenie::Weenie::new(ObjectId(1));
        p.pwd = PublicWeenieDesc {
            name: "Lark".into(),
            ..PublicWeenieDesc::default()
        };
        w.tables.weenies.insert(ObjectId(1), p);
        let mut m = crate::weenie::Weenie::new(ObjectId(2));
        m.pwd = PublicWeenieDesc {
            name: "Drudge".into(),
            obj_type: item_type::CREATURE,
            bitfield: crate::weenie::bitfield::ATTACKABLE,
            ..PublicWeenieDesc::default()
        };
        w.tables.weenies.insert(ObjectId(2), m);
        w.tables.inventories.insert(
            ObjectId(1),
            crate::objects::ObjectInventory::new(ObjectId(1)),
        );
        w.combat.combat_mode = CombatMode::Melee;
        let mut out = RecordingSink::default();
        w.set_selected_object(Some(ObjectId(2)), false, &mut out);
        w
    }

    /// Give the player the `CombatTable` DataID quality retail characters are born with, through
    /// the same table `Qualities_PrivateUpdateDataID` writes.
    ///
    /// `0x30000000` is the first of the 71 shipped `.wct` files
    /// and is the default character combat-table entry in the data-id mapper, so it is an id a real character carries rather
    /// than an arbitrary non-zero.
    fn give_combat_table(w: &mut World) {
        let p = w.player.expect("the fixture has a player");
        let we = w.tables.weenies.get_mut(p).expect("and a weenie for it");
        we.qualities.get_or_insert_with(crate::Qualities::new).set(
            crate::qualities::StatKey::new(crate::qualities::StatType::Did, COMBAT_TABLE_DID),
            crate::qualities::StatValue::Did(dereth_primitives::DataId(0x3000_0000)),
        );
        assert_eq!(
            w.combat_table_did(),
            Some(dereth_primitives::DataId(0x3000_0000)),
            "the fixture must actually reach the DataID-4 lookup, or every melee arm below is \
             measuring the absence it was meant to remove"
        );
    }

    /// **No physics object means not ready, in every mode and both
    /// flavours.** The one arm `lenient` cannot rescue.
    #[test]
    fn a_null_physics_object_answers_false_in_every_mode_and_both_flavours() {
        let mut w = world();
        give_combat_table(&mut w);
        w.combat.current_style = MISSILE_READY_STYLES[0];
        for mode in [
            CombatMode::NonCombat,
            CombatMode::Melee,
            CombatMode::Missile,
            CombatMode::Magic,
        ] {
            w.combat.combat_mode = mode;
            for lenient in [false, true] {
                assert!(
                    !w.player_in_ready_position(lenient, None),
                    "{mode:?} lenient={lenient}: the player has no physics body"
                );
            }
            // The same world with a body answers true: the absent body caused the false
            // result above, and this fixture can reach the ready state.
            assert!(
                w.player_in_ready_position(true, Some(false)),
                "{mode:?}: the identical world with a settled body IS ready"
            );
        }
    }

    /// **The shared non-combat/magic arm.** `NONCOMBAT` and `MAGIC` reach
    /// `motions_pending` **without reading `lenient` at all**, so the two flavours are the same answer in those two modes.
    #[test]
    fn noncombat_and_magic_ignore_param_1_and_answer_the_motion_queue() {
        let mut w = world();
        // The table is present so that a melee-shaped answer cannot be mistaken for this one.
        give_combat_table(&mut w);
        for mode in [CombatMode::NonCombat, CombatMode::Magic] {
            w.combat.combat_mode = mode;
            for lenient in [false, true] {
                assert!(
                    w.player_in_ready_position(lenient, Some(false)),
                    "{mode:?} idle"
                );
                assert!(
                    !w.player_in_ready_position(lenient, Some(true)),
                    "{mode:?} lenient={lenient}: a pending motion refuses even the lenient \
                     flavour, because this mode does not consult the lenient flag"
                );
            }
        }
    }

    /// **The measurement the whole flavour split rests on.** One world, one body with a motion
    /// outstanding, melee mode, a combat table present: `lenient == true` answers **true** and
    /// `lenient == false` answers **false**.
    ///
    /// The attack flavor returns true without testing the motion queue.
    /// A build that used one value for both flavors would be more
    /// restrictive than retail in exactly this state, and this is the state that says so.
    #[test]
    fn the_melee_arm_is_where_the_two_flavours_come_apart() {
        let mut w = world();
        w.combat.combat_mode = CombatMode::Melee;

        // No `CombatTable` DataID: the DataID lookup for key 4 leaves the invalid id in place and
        // falls to the release-and-return-false block. `lenient` cannot rescue that either.
        assert_eq!(w.combat_table_did(), None);
        assert!(
            !w.player_in_ready_position(true, Some(false)),
            "no table, lenient"
        );
        assert!(
            !w.player_in_ready_position(false, Some(false)),
            "no table, strict"
        );

        // An explicit `INVALID_DID` is the same answer as an absent property, which is what
        // the invalid-id comparison means and is the whole reason the quality lookup's callers
        // pre-seed the out parameter. A quality that is *present* and zero must still refuse.
        let p = w.player.expect("player");
        w.tables
            .weenies
            .get_mut(p)
            .expect("weenie")
            .qualities
            .get_or_insert_with(crate::Qualities::new)
            .set(
                crate::qualities::StatKey::new(crate::qualities::StatType::Did, COMBAT_TABLE_DID),
                crate::qualities::StatValue::Did(dereth_primitives::DataId(0)),
            );
        assert_eq!(
            w.combat_table_did(),
            None,
            "an explicit INVALID_DID reads as no table"
        );
        assert!(
            !w.player_in_ready_position(true, Some(false)),
            "INVALID_DID, lenient"
        );

        give_combat_table(&mut w);
        assert!(
            w.player_in_ready_position(true, Some(false)),
            "table + idle, lenient"
        );
        assert!(
            w.player_in_ready_position(false, Some(false)),
            "table + idle, strict"
        );

        // The discriminating station.
        assert!(
            w.player_in_ready_position(true, Some(true)),
            "lenient readiness returns without consulting `motions_pending`"
        );
        assert!(
            !w.player_in_ready_position(false, Some(true)),
            "strict readiness checks `motions_pending` and refuses"
        );
    }

    /// **missile is the strict arm.** The stance must already be one of the six and
    /// `forward_command` must be `Ready`; only then does `lenient` matter.
    #[test]
    fn the_missile_arm_needs_one_of_the_six_styles_and_a_ready_forward_command() {
        let mut w = world();
        give_combat_table(&mut w); // present, and must not help: missile never reads it
        w.combat.combat_mode = CombatMode::Missile;

        w.combat.current_style = 0x8000_003D; // NonCombat -- the stance is not up
        assert!(
            !w.player_in_ready_position(true, Some(false)),
            "NonCombat stance, lenient"
        );

        // The neighbours of the accepted styles, which is what says the case table is a table and
        // not a range: every one of these is a `1` byte, and `0x80000046` is
        // `DualWieldCombat`, which the power bar reads and this arm must not accept.
        for style in [
            0x8000_0040_u32,
            0x8000_0042,
            0x8000_0044,
            0x8000_0045,
            0x8000_0046,
        ] {
            w.combat.current_style = style;
            assert!(
                !w.player_in_ready_position(true, Some(false)),
                "{style:#010X} is a `1` byte in the case table and must refuse"
            );
        }

        for style in [0x8000_0138_u32, 0x8000_0139] {
            w.combat.current_style = style;
            w.combat.forward_command = MOTION_READY;
            assert!(
                !w.player_in_ready_position(true, Some(false)),
                "{style:#010X} is the 2013 table's numbering, not a stance of the final client's"
            );
        }

        for style in MISSILE_READY_STYLES {
            w.combat.current_style = style;
            w.combat.forward_command = MOTION_READY;
            assert!(
                w.player_in_ready_position(true, Some(false)),
                "{style:#010X} idle, lenient"
            );
            assert!(
                w.player_in_ready_position(true, Some(true)),
                "{style:#010X}: lenient readiness skips `motions_pending`"
            );
            assert!(
                !w.player_in_ready_position(false, Some(true)),
                "{style:#010X}: strict readiness refuses pending motion"
            );
            // Only motion-ready (`0x41000003`) passes; every other forward command fails.
            w.combat.forward_command = 0x4000_0004; // WalkForward
            assert!(
                !w.player_in_ready_position(true, Some(false)),
                "{style:#010X}: the stance is up but the body is not in `Ready`"
            );
        }
    }

    /// Undefined is mode 0, so `mode - 1` is `0xFFFFFFFF` unsigned and fails the range check.
    #[test]
    fn an_undefined_combat_mode_answers_false() {
        let mut w = world();
        give_combat_table(&mut w);
        w.combat.current_style = MISSILE_READY_STYLES[0];
        w.combat.combat_mode = CombatMode::Undef;
        for lenient in [false, true] {
            assert!(
                !w.player_in_ready_position(lenient, Some(false)),
                "lenient={lenient}"
            );
        }
    }

    /// Pin literal values so that the six styles, Ready and the
    /// DataID key cannot be moved by an edit that keeps every other test symmetric.
    ///
    /// Every number here is retail's: the six styles (offsets 0, 2, 4, 8, 249 and 250 from
    /// `0x8000003F`), `Ready`'s `0x41000003`, and the DataID key 4.
    #[test]
    fn the_ready_position_constants_are_the_ones_in_the_shipped_image() {
        assert_eq!(
            MISSILE_READY_STYLES,
            [
                0x8000_003F,
                0x8000_0041,
                0x8000_0043,
                0x8000_0047,
                0x8000_013B,
                0x8000_013C
            ]
        );
        assert_eq!(MOTION_READY, 0x4100_0003);
        assert_eq!(COMBAT_TABLE_DID, 4);
        // The six are `BowCombat`, `CrossbowCombat`, `SlingCombat`, `ThrownWeaponCombat`,
        // `AtlatlCombat` and `ThrownShieldCombat` -- indices 63, 65, 67, 71, 315 and 316 of the
        // final client's `MotionCommand` name table.
        assert!(!MISSILE_READY_STYLES.contains(&DUAL_WIELD_COMBAT_STYLE));
    }

    fn enter_advanced_combat(w: &mut World) {
        w.player_system
            .options
            .set(crate::player::options::option::ADVANCED_COMBAT_UI, true);
        assert!(
            !w.combat.advanced_combat_mode,
            "the option alone changes nothing"
        );
        let mut req = RecordingRequests::default();
        let mut sink = RecordingSink::default();
        // `sendToServer = false` is the quality-changed handler's arm, which re-reads the option too.
        w.set_combat_mode(
            &mut req,
            &mut sink,
            CombatMode::NonCombat,
            false,
            true,
            false,
        )
        .unwrap();
        w.set_combat_mode(&mut req, &mut sink, CombatMode::Melee, false, true, false)
            .unwrap();
        assert!(
            w.combat.advanced_combat_mode,
            "mode change re-read the advanced-combat option"
        );
    }

    /// In classic UI, releasing above the slider cap fires once at released power and once
    /// more clamped to the cap.
    #[test]
    fn end_attack_request_fires_execute_attack_twice_in_the_classic_ui() {
        let mut w = world();
        let mut req = RecordingRequests::default();
        w.combat.ui_requested_power = 0.5;
        w.start_attack_request(true, LocalTime(0.0)).unwrap();
        // Release after a full second: level = 1.0, above the 0.5 cap.
        w.end_attack_request(&mut req, AttackHeight::Medium, None, true, LocalTime(1.0));

        let attacks: Vec<_> = req
            .0
            .iter()
            .filter_map(|r| match r {
                Request::TargetedMeleeAttack(a) => Some(a.power_level),
                _ => None,
            })
            .collect();
        assert_eq!(
            attacks.len(),
            2,
            "two attack messages per request; servers accept it"
        );
        assert_eq!(attacks[0], 1.0, "the first is at the released power");
        assert_eq!(attacks[1], 0.5, "the second is clamped to the slider cap");
    }

    /// The same release in **advanced** mode fires once: the second `execute_attack` is gated on
    /// advanced combat mode being off.
    #[test]
    fn advanced_combat_mode_fires_once() {
        let mut w = world();
        let mut req = RecordingRequests::default();
        enter_advanced_combat(&mut w);
        w.combat.ui_requested_power = 0.5;
        w.start_attack_request(true, LocalTime(0.0)).unwrap();
        w.end_attack_request(&mut req, AttackHeight::Medium, None, true, LocalTime(1.0));
        assert_eq!(
            req.0
                .iter()
                .filter(|r| matches!(r, Request::TargetedMeleeAttack(_)))
                .count(),
            1
        );
    }

    /// The power-bar duration is 1.000 seconds, or 0.800 seconds while dual-wielding, and
    /// the result is clamped to `[0, 1]`.
    #[test]
    fn the_power_bar_takes_one_second_or_point_eight_dual_wielding() {
        let mut c = CombatState::begin();
        assert_eq!(c.power_bar_level(LocalTime(5.0)), 0.0, "no build, no level");
        c.start_power_bar_build(LocalTime(10.0));
        assert_eq!(c.power_bar_level(LocalTime(10.0)), 0.0);
        assert_eq!(c.power_bar_level(LocalTime(10.5)), 0.5);
        assert_eq!(c.power_bar_level(LocalTime(11.0)), 1.0);
        assert_eq!(c.power_bar_level(LocalTime(99.0)), 1.0, "clamped");
        c.current_style = DUAL_WIELD_COMBAT_STYLE;
        assert_eq!(c.power_bar_level(LocalTime(10.8)), 1.0);
        assert!((c.power_bar_level(LocalTime(10.4)) - 0.5).abs() < 1e-6);
    }

    /// Jump power floors at `MIN_JUMP_EXTENT` only while a jump is pending.
    #[test]
    fn the_jump_bar_floors_at_the_minimum_extent() {
        let mut c = CombatState::begin();
        c.start_power_bar_build(LocalTime(0.0));
        assert_eq!(c.jump_power_level(LocalTime(0.0)), 0.0, "not jumping");
        c.jump_pending = true;
        assert_eq!(c.jump_power_level(LocalTime(0.0)), MIN_JUMP_EXTENT);
    }

    /// Combat compatibility, including the melee rule's
    /// three-way condition.
    #[test]
    fn combat_mode_compatibility_follows_the_inventory_mask() {
        let mut w = world();
        assert!(w.compatible_combat_mode(CombatMode::NonCombat));
        assert!(!w.compatible_combat_mode(CombatMode::Missile));
        w.inventory_mask = loc::MISSILE_WEAPON;
        assert!(w.compatible_combat_mode(CombatMode::Missile));
        assert!(!w.compatible_combat_mode(CombatMode::Magic));
        w.inventory_mask |= loc::HELD;
        assert!(w.compatible_combat_mode(CombatMode::Magic));

        // Melee: a bow in the ready slot with no melee or two-handed weapon refuses.
        w.inventory_mask = loc::MISSILE_WEAPON; // MISSILE_WEAPON is inside WEAPON_READY_SLOT
        assert!(!w.compatible_combat_mode(CombatMode::Melee));
        w.inventory_mask |= loc::MELEE_WEAPON;
        assert!(w.compatible_combat_mode(CombatMode::Melee));
        // Nothing wielded at all: melee is fine (unarmed).
        w.inventory_mask = 0;
        assert!(w.compatible_combat_mode(CombatMode::Melee));
    }

    /// The attack target is the selection, unless it is owned by the
    /// player.
    #[test]
    fn the_attack_target_excludes_the_players_own_things() {
        let mut w = world();
        assert_eq!(w.get_attack_target(), Some(ObjectId(2)));
        w.weenie_mut(ObjectId(2)).unwrap().pwd.container_id = Some(ObjectId(1));
        assert_eq!(
            w.get_attack_target(),
            None,
            "an owned object is never an attack target"
        );
    }

    /// Classic UI refuses to start a swing with no valid target; advanced UI
    /// skips the check entirely.
    #[test]
    fn the_target_check_is_skipped_in_advanced_mode() {
        let mut w = world();
        let mut out = RecordingSink::default();
        w.set_selected_object(None, false, &mut out);
        assert_eq!(
            w.start_attack_request(true, LocalTime(0.0)),
            Err("You must select a valid combat target before attacking")
        );
        enter_advanced_combat(&mut w);
        assert_eq!(w.start_attack_request(true, LocalTime(0.0)), Ok(()));
    }

    /// The hit adjective table boundaries belong to the lower bucket.
    #[test]
    fn the_hit_adjective_table_boundaries_belong_to_the_lower_bucket() {
        assert_eq!(
            combat_hit_adjectives(damage_type::SLASH, 0.09),
            ("scratch", "scratches")
        );
        assert_eq!(
            combat_hit_adjectives(damage_type::SLASH, 0.10),
            ("scratch", "scratches"),
            "exactly 0.10 is still the first bucket"
        );
        assert_eq!(
            combat_hit_adjectives(damage_type::SLASH, 0.1001),
            ("cut", "cuts")
        );
        assert_eq!(
            combat_hit_adjectives(damage_type::SLASH, 0.25),
            ("cut", "cuts"),
            "exactly 0.25 falls in the SECOND bucket, not the third"
        );
        assert_eq!(
            combat_hit_adjectives(damage_type::SLASH, 0.2501),
            ("slash", "slashes")
        );
        assert_eq!(
            combat_hit_adjectives(damage_type::SLASH, 0.50),
            ("slash", "slashes"),
            "and exactly 0.50 in the third, not the fourth"
        );
        assert_eq!(
            combat_hit_adjectives(damage_type::SLASH, 0.5001),
            ("mangle", "mangles")
        );
        assert_eq!(
            combat_hit_adjectives(damage_type::SLASH, -0.1),
            ("hit", "hits")
        );
        assert_eq!(combat_hit_adjectives(0x1234, 0.9), ("hit", "hits"));
        assert_eq!(
            combat_hit_adjectives(damage_type::SLASH | damage_type::PIERCE, 0.9),
            ("hit", "hits"),
            "the switch sends 3 to the default: no lowest-bit decode"
        );
        assert_eq!(
            combat_hit_adjectives(damage_type::NETHER, 0.9),
            ("eradicate", "eradicates")
        );
        assert_eq!(
            combat_hit_adjectives(damage_type::HEALTH, 0.0),
            ("drain", "drains")
        );
    }

    /// Oracle: contract 12.13 — `"Critical hit!  "` has **two** trailing spaces in the attacker
    /// message and one in the defender message.
    #[test]
    fn the_critical_hit_prefix_has_two_spaces_for_the_attacker_and_one_for_the_defender() {
        let a = attacker_notification_line("Drudge", damage_type::SLASH, 0.6, 42, true, 0);
        assert!(a.starts_with("Critical hit!  You mangle Drudge"), "{a}");
        let d = defender_notification_line("Drudge", damage_type::SLASH, 0.6, 42, 0, true, 0);
        assert!(
            d.starts_with("Critical hit! Drudge mangles your head"),
            "{d}"
        );
        assert_ne!(&a[..15], &d[..15]);
    }

    /// Both attack-notification message formats, field by field.
    #[test]
    fn the_notification_lines_match_the_documented_format() {
        assert_eq!(
            attacker_notification_line("Drudge", damage_type::FIRE, 0.3, 1, false, 0),
            "You burn Drudge for 1 point of fire damage!\n"
        );
        assert_eq!(
            attacker_notification_line("Drudge", damage_type::HEALTH, 0.3, 7, false, 0),
            "You siphon Drudge for 7 points of damage!\n",
            "HEALTH has an adjective row and no name, so the word is empty -- and the \
             space that used to be emitted for it was a double space"
        );
        assert_eq!(
            attacker_notification_line("Drudge", 0, 0.3, 7, false, 0),
            "You hit Drudge for 7 points of damage!\n",
            "an undefined damage type contributes no type word at all"
        );
        assert_eq!(
            attacker_notification_line(
                "Drudge",
                damage_type::SLASH,
                0.6,
                9,
                true,
                attack_conditions::SNEAK_ATTACK
                    | attack_conditions::RECKLESSNESS
                    | attack_conditions::CRITICAL_PROTECTION
            ),
            "Critical hit!  Sneak Attack! Recklessness! You mangle Drudge for 9 points of slashing \
             damage! Your target's Critical Protection augmentation allows them to avoid your \
             critical hit!\n"
        );
        assert_eq!(
            defender_notification_line("Drudge", damage_type::PIERCE, 0.05, 3, 3, false, 0),
            "Drudge nicks your upper arm for 3 points of piercing damage!\n"
        );
        assert_eq!(
            evasion_attacker_line("Drudge"),
            "Drudge evaded your attack.\n"
        );
        assert_eq!(evasion_defender_line("Drudge"), "You evaded Drudge!\n");
    }

    /// Condition bit `0x8` says "Overpower! " on both lines, after the critical prefix and before
    /// the sneak-attack one; alone it is the only prefix.
    #[test]
    fn an_overpowering_hit_says_so_on_both_lines_in_its_place() {
        let all = attack_conditions::OVERPOWER
            | attack_conditions::SNEAK_ATTACK
            | attack_conditions::RECKLESSNESS;
        assert_eq!(
            attacker_notification_line("Drudge", damage_type::SLASH, 0.6, 9, true, all),
            "Critical hit!  Overpower! Sneak Attack! Recklessness! You mangle Drudge for 9 points \
             of slashing damage!\n"
        );
        assert_eq!(
            defender_notification_line("Drudge", damage_type::PIERCE, 0.05, 3, 3, true, all),
            "Critical hit! Overpower! Sneak Attack! Reckless! Drudge nicks your upper arm for 3 \
             points of piercing damage!\n"
        );
        assert_eq!(
            attacker_notification_line(
                "Drudge",
                damage_type::FIRE,
                0.3,
                1,
                false,
                attack_conditions::OVERPOWER
            ),
            "Overpower! You burn Drudge for 1 point of fire damage!\n"
        );
        assert_eq!(
            attack_conditions::OVERPOWER,
            dereth_protocol::combat::attack_conditions::OVERPOWER
        );
    }

    /// Damage-type and body-part display text.
    #[test]
    fn damage_types_join_with_slashes_and_body_parts_lower_case() {
        assert_eq!(damage_type_to_string(damage_type::SLASH), "Slashing");
        assert_eq!(
            damage_type_to_string(damage_type::SLASH | damage_type::FIRE | damage_type::NETHER),
            "Slashing/Fire/Nether"
        );
        assert_eq!(damage_type_to_string(damage_type::BASE), "Prismatic");
        assert_eq!(damage_type_to_string(0), "");
        assert_eq!(body_part_to_string(0x18), "upper tentacle");
        assert_eq!(body_part_to_string(11), "unknown", "11 and 14 are gaps");
        assert_eq!(body_part_to_string(14), "unknown");
        assert_eq!(body_part_to_string(0x1b), "num");
        assert_eq!(
            body_part_to_string(0x1c),
            "unknown",
            "past the last entry, 0x1b"
        );
        assert_eq!(body_part_to_string(u32::MAX), "undefined");
        // The 64-byte buffer both notification handlers pass to `damage_type_to_string`.
        let eight = damage_type::SLASH
            | damage_type::PIERCE
            | damage_type::BLUDGEON
            | damage_type::COLD
            | damage_type::FIRE
            | damage_type::ACID
            | damage_type::ELECTRIC
            | damage_type::NETHER;
        assert_eq!(damage_type_to_string(eight).len() + 1, 63);
        assert_eq!(
            notification_damage_word(eight),
            "slashing/piercing/bludgeoning/cold/fire/acid/electrical/nether "
        );
        assert_eq!(
            damage_type_to_string(eight | damage_type::BASE).len() + 1,
            73
        );
        assert_eq!(
            notification_damage_word(eight | damage_type::BASE),
            "",
            "73 > 0x40, so the retail client writes only the terminator"
        );
        assert_eq!(notification_damage_word(damage_type::STAMINA), "");
        assert_eq!(notification_damage_word(damage_type::MANA), "");
    }

    /// Weapon-speed descriptions across four deliberately asymmetric ranges.
    #[test]
    fn weapon_time_adjectives_use_the_documented_boundaries() {
        assert_eq!(weapon_time_to_string(10), "Very Fast");
        assert_eq!(weapon_time_to_string(11), "Fast");
        assert_eq!(weapon_time_to_string(30), "Fast");
        assert_eq!(weapon_time_to_string(31), "Average");
        assert_eq!(weapon_time_to_string(49), "Average");
        assert_eq!(weapon_time_to_string(50), "Slow");
        assert_eq!(weapon_time_to_string(79), "Slow");
        assert_eq!(weapon_time_to_string(80), "Very Slow");
    }

    /// Attack commencement and completion update the request guard:
    /// `attack_in_progress` blocks every inventory request while it is set.
    #[test]
    fn an_attack_in_progress_blocks_inventory_requests() {
        let mut w = world();
        let mut req = RecordingRequests::default();
        assert_eq!(w.ready_for_inventory_request(true), Ok(()));
        w.handle_commence_attack();
        assert_eq!(
            w.ready_for_inventory_request(true),
            Err("You cannot move or use an item while attacking")
        );
        w.handle_attack_done(&mut req, 0, true, LocalTime(1.0));
        assert_eq!(w.ready_for_inventory_request(true), Ok(()));
        let _ = ServerTime(0.0);
    }

    /// Begin resets to the documented values.
    #[test]
    fn begin_resets_to_the_documented_values() {
        let c = CombatState::begin();
        assert_eq!(c.ui_requested_power, 0.5);
        assert_eq!(c.combat_mode, CombatMode::NonCombat);
        assert_eq!(c.pending_combat_mode, CombatMode::Undef);
        assert_eq!(
            c.pending_combat_mode.raw(),
            0,
            "UNDEF_COMBAT_MODE is 0, as a literal"
        );
        assert_eq!(c.requested_attack_height, AttackHeight::Medium);
        assert_eq!(c.power_bar_mode, PowerBarMode::Undef);
        assert!(!c.target_willingly_lost);
    }

    /// Combat enum values.
    #[test]
    fn the_enums_have_the_documented_values() {
        assert_eq!(CombatMode::NonCombat.raw(), 1);
        assert_eq!(CombatMode::Melee.raw(), 2);
        assert_eq!(CombatMode::Missile.raw(), 4);
        assert_eq!(CombatMode::Magic.raw(), 8);
        assert_eq!(
            CombatMode::COMBAT,
            CombatMode::Melee.raw() | CombatMode::Missile.raw() | CombatMode::Magic.raw()
        );
        assert_eq!(CombatMode::VALID, 15);
        assert_eq!(CombatMode::Undef.raw(), 0);
        assert_eq!(AttackHeight::Low as u32, 3);
        assert_eq!(PowerBarMode::Ddd as u32, 4);
        assert_eq!(damage_type::BASE, 0x1000_0000);
        assert_eq!(combat_use::TWO_HANDED, 5);
        assert_eq!(CombatMode::Melee.name(), "melee");
        assert_eq!(CombatMode::Undef.name(), "unknown");
    }

    /// Every `Combat_ChangeCombatMode` in the sink, as its raw mode word.
    fn modes_sent(req: &RecordingRequests) -> Vec<u32> {
        req.0
            .iter()
            .filter_map(|r| match r {
                Request::ChangeCombatMode(m) => Some(m.combat_mode),
                _ => None,
            })
            .collect()
    }

    /// A combat mode toggle pressed before the body is ready is retried not dropped.
    #[test]
    fn a_combat_mode_toggle_pressed_before_the_body_is_ready_is_retried_not_dropped() {
        let mut w = world(); // starts in melee
        let mut req = RecordingRequests::default();
        let mut sink = crate::RecordingSink::default();

        // The player presses the toggle while a motion is pending: peace is queued, nothing sent.
        let (mode, refusal) = w.toggle_combat_mode_target(false);
        assert_eq!(mode, CombatMode::NonCombat);
        assert!(refusal.is_none());
        assert_eq!(
            w.set_combat_mode(&mut req, &mut sink, mode, true, false, false),
            Ok(())
        );
        assert_eq!(
            w.combat.pending_combat_mode,
            CombatMode::NonCombat,
            "queued"
        );
        assert_eq!(
            w.combat.combat_mode,
            CombatMode::Melee,
            "and not applied locally"
        );
        assert_eq!(
            modes_sent(&req),
            Vec::<u32>::new(),
            "0 of 1 sent while not ready"
        );

        // Frames keep running while the body is still busy: still queued, still nothing sent.
        for _ in 0..10 {
            assert_eq!(w.combat_use_time(&mut req, &mut sink, false), None);
        }
        assert_eq!(
            w.combat.pending_combat_mode,
            CombatMode::NonCombat,
            "still queued after 10"
        );
        assert_eq!(
            modes_sent(&req),
            Vec::<u32>::new(),
            "0 of 1 sent across 10 not-ready frames"
        );

        // The first ready frame retries it.
        assert_eq!(w.combat_use_time(&mut req, &mut sink, true), None);
        assert_eq!(w.combat.combat_mode, CombatMode::NonCombat);
        assert_eq!(
            w.combat.pending_combat_mode,
            CombatMode::Undef,
            "cleared by UseTime"
        );
        assert_eq!(
            modes_sent(&req),
            vec![1],
            "1 of 1 sent; NONCOMBAT_COMBAT_MODE is 1"
        );

        // And it is one-shot: later frames send nothing more.
        for _ in 0..10 {
            assert_eq!(w.combat_use_time(&mut req, &mut sink, true), None);
        }
        assert_eq!(
            modes_sent(&req),
            vec![1],
            "still exactly 1 after 10 more ready frames"
        );
    }

    /// The pending mode is a single field, not a queue. Each request overwrites it, so two
    /// requests before the body is ready produce one
    /// message, carrying the **second** mode.
    #[test]
    fn two_combat_mode_requests_before_ready_collapse_to_the_second_one() {
        let mut w = world();
        w.inventory_mask = loc::MISSILE_WEAPON | loc::HELD | loc::MELEE_WEAPON;
        let mut req = RecordingRequests::default();
        let mut sink = crate::RecordingSink::default();

        assert_eq!(
            w.set_combat_mode(&mut req, &mut sink, CombatMode::Missile, true, false, false),
            Ok(())
        );
        assert_eq!(w.combat.pending_combat_mode, CombatMode::Missile);
        assert_eq!(
            w.set_combat_mode(&mut req, &mut sink, CombatMode::Magic, true, false, false),
            Ok(())
        );
        assert_eq!(
            w.combat.pending_combat_mode,
            CombatMode::Magic,
            "overwritten, not queued"
        );

        assert_eq!(w.combat_use_time(&mut req, &mut sink, true), None);
        assert_eq!(
            modes_sent(&req),
            vec![8],
            "one message, MAGIC_COMBAT_MODE = 8"
        );
        assert_eq!(w.combat.combat_mode, CombatMode::Magic);
    }

    /// Oracle: `UseTime`'s clear is **outside** the `SetCombatMode` call, so a queued mode that has
    /// become incompatible is refused once and then dropped rather than retried on every frame.
    #[test]
    fn a_pending_mode_that_became_incompatible_is_refused_once_and_then_dropped() {
        let mut w = world();
        w.inventory_mask = loc::MISSILE_WEAPON;
        let mut req = RecordingRequests::default();
        let mut sink = crate::RecordingSink::default();
        assert_eq!(
            w.set_combat_mode(&mut req, &mut sink, CombatMode::Missile, true, false, false),
            Ok(())
        );

        // The bow is stowed before the body comes to rest.
        w.inventory_mask = 0;
        assert_eq!(
            w.combat_use_time(&mut req, &mut sink, true),
            Some("You can't enter missile mode".to_string()),
            "the combat-mode check refuses, and the client puts the text on the scroll"
        );
        assert_eq!(
            w.combat.pending_combat_mode,
            CombatMode::Undef,
            "dropped, not re-armed"
        );
        assert_eq!(
            w.combat.combat_mode,
            CombatMode::Melee,
            "and the mode did not change"
        );
        assert_eq!(modes_sent(&req), Vec::<u32>::new());
        assert_eq!(
            w.combat_use_time(&mut req, &mut sink, true),
            None,
            "and never refuses twice"
        );
    }

    /// The retry runs the whole mode-change tail before clearing the pending mode.
    /// A callback observing or changing that field distinguishes
    /// an after-tail clear from an early one. The reentrant request is an ordering probe,
    /// not a claim that the application's currently deferred notice bus runs synchronously.
    #[test]
    fn a_pending_retry_finishes_its_mode_change_tail_before_clearing_pending() {
        let mut w = world();
        let mut req = RecordingRequests::default();
        let mut sink = RecordingSink::default();
        w.set_combat_mode(
            &mut req,
            &mut sink,
            CombatMode::NonCombat,
            true,
            false,
            false,
        )
        .unwrap();
        let mut callbacks = 0;
        assert_eq!(
            w.combat_use_time_with_mode_change(&mut req, &mut sink, true, |w, out| {
                callbacks += 1;
                assert_eq!(w.combat.combat_mode, CombatMode::NonCombat);
                assert_eq!(w.combat.pending_combat_mode, CombatMode::NonCombat);
                // A selection effect in the mode-change tail can notify other subscribers.
                w.set_selected_object(None, false, out);
                // Model a subscriber queuing a new request while the tail still owns control.
                w.set_combat_mode(
                    &mut crate::NullRequests,
                    out,
                    CombatMode::Melee,
                    true,
                    false,
                    false,
                )
                .unwrap();
                assert_eq!(w.combat.pending_combat_mode, CombatMode::Melee);
            }),
            None,
        );
        assert_eq!(callbacks, 1);
        assert_eq!(w.selected, None);
        assert!(sink
            .0
            .iter()
            .any(|n| matches!(n, crate::Notice::SelectionChanged { .. })));
        assert_eq!(
            w.combat.pending_combat_mode,
            CombatMode::Undef,
            "UseTime's final clear wins"
        );
        assert_eq!(modes_sent(&req), vec![1]);
    }

    /// Oracle: the client's pending/ready guards and the client's equality
    /// and compatibility returns all precede the mode-change tail.
    #[test]
    fn a_pending_retry_does_not_call_the_tail_without_a_mode_change() {
        let mut w = world();
        let mut req = RecordingRequests::default();
        let mut sink = RecordingSink::default();
        for (pending, ready, clears, refuses) in [
            (CombatMode::Undef, true, true, false),
            (CombatMode::NonCombat, false, false, false),
            (CombatMode::Melee, true, true, false),
            (CombatMode::Missile, true, true, true),
        ] {
            w.combat.pending_combat_mode = pending;
            let refusal = w.combat_use_time_with_mode_change(&mut req, &mut sink, ready, |_, _| {
                panic!("{pending:?}, ready={ready}: no mode change may reach the tail")
            });
            assert_eq!(refusal.is_some(), refuses);
            assert_eq!(w.combat.combat_mode, CombatMode::Melee);
            assert_eq!(
                w.combat.pending_combat_mode,
                if clears { CombatMode::Undef } else { pending },
            );
        }
        assert!(modes_sent(&req).is_empty());
    }

    /// A server driven mode change does not discard the players queued request.
    #[test]
    fn a_server_driven_mode_change_does_not_discard_the_players_queued_request() {
        let mut w = world();
        w.inventory_mask = loc::MISSILE_WEAPON | loc::MELEE_WEAPON;
        let mut req = RecordingRequests::default();
        let mut sink = crate::RecordingSink::default();
        assert_eq!(
            w.set_combat_mode(&mut req, &mut sink, CombatMode::Missile, true, false, false),
            Ok(())
        );
        assert_eq!(w.combat.pending_combat_mode, CombatMode::Missile);

        // The server says the character is in peace mode. `sendToServer = false`, so no message and
        // no ready check — the quality is authoritative.
        assert_eq!(
            w.set_combat_mode(
                &mut req,
                &mut sink,
                CombatMode::NonCombat,
                false,
                false,
                false
            ),
            Ok(())
        );
        assert_eq!(w.combat.combat_mode, CombatMode::NonCombat);
        assert_eq!(modes_sent(&req), Vec::<u32>::new(), "nothing is sent back");
        assert_eq!(
            w.combat.pending_combat_mode,
            CombatMode::Missile,
            "the player's own request survives the server's update"
        );

        assert_eq!(w.combat_use_time(&mut req, &mut sink, true), None);
        assert_eq!(modes_sent(&req), vec![4], "MISSILE_COMBAT_MODE = 4");
    }

    /// Setting combat mode reads the advanced-combat option and stores its bit. The option getter
    /// reads bit 12 from the option word.
    ///
    /// **`CombatState::advanced_combat_mode` had no production writer**: its only two writers in
    /// the workspace were assignments in this file's tests standing in for a missing producer.
    /// Both now use [`enter_advanced_combat`],
    /// which drives this.
    ///
    /// Asserted at two stations *in each direction*, because a producer that only ever sets and a
    /// producer that only ever clears both pass a one-station harness, and because the field's six
    /// readers all branch on it. The literal `0x1000` is pinned here so a wrong bit cannot hide
    /// behind the symbol.
    #[test]
    fn set_combat_mode_re_reads_the_advanced_combat_ui_option_in_both_directions() {
        use crate::player::options::{option, PLAYER_OPTIONS};
        // The bit tests, as a literal.
        assert_eq!(option::ADVANCED_COMBAT_UI, 12);
        assert_eq!(PLAYER_OPTIONS[12].0, "AdvancedCombatUI");
        assert_eq!(
            PLAYER_OPTIONS[12].2, 0x0000_1000,
            "1 << 12, bit 12 of the first option word"
        );

        let mut w = world();
        let mut req = RecordingRequests::default();
        let mut sink = RecordingSink::default();

        // Station A: the option is off and the field is false after a mode change.
        assert!(!w.player_system.options.advanced_combat_ui());
        w.set_combat_mode(
            &mut req,
            &mut sink,
            CombatMode::NonCombat,
            false,
            true,
            false,
        )
        .unwrap();
        assert!(!w.combat.advanced_combat_mode, "A: off");

        // Station B: turn the option on. **Nothing happens until a mode change** — the client
        // re-reads it inside `SetCombatMode` and nowhere else.
        w.player_system
            .options
            .set(option::ADVANCED_COMBAT_UI, true);
        assert!(
            !w.combat.advanced_combat_mode,
            "B: the option alone does not move the field"
        );
        w.set_combat_mode(&mut req, &mut sink, CombatMode::Missile, false, true, false)
            .unwrap();
        assert!(
            w.combat.advanced_combat_mode,
            "B: and the mode change picks it up"
        );

        // Station C: and back down again, on the `sendToServer = false` arm, which is the one
        // the quality-changed handler uses — the re-read is outside the `sendToServer` test.
        w.player_system
            .options
            .set(option::ADVANCED_COMBAT_UI, false);
        w.set_combat_mode(
            &mut req,
            &mut sink,
            CombatMode::NonCombat,
            false,
            true,
            false,
        )
        .unwrap();
        assert!(
            !w.combat.advanced_combat_mode,
            "C: cleared, and by the server-driven arm"
        );
    }

    /// Selection fixup after a combat-mode change.
    ///
    /// Four cases, and the two that must *not* change anything are the point: a fixup that always
    /// cleared the selection would satisfy "an unattackable selection is cleared" on its own.
    #[test]
    fn entering_combat_clears_a_selection_that_is_not_attackable_and_leaves_the_rest_alone() {
        // (a) an attackable creature survives the change.
        let mut w = world(); // ObjectId(2) is an attackable drudge, and is selected
        let mut req = RecordingRequests::default();
        let mut sink = RecordingSink::default();
        w.combat.combat_mode = CombatMode::NonCombat;
        w.set_combat_mode(&mut req, &mut sink, CombatMode::Melee, false, true, false)
            .unwrap();
        assert_eq!(
            w.selected,
            Some(ObjectId(2)),
            "a: an attackable target is kept"
        );
        assert!(
            !sink
                .0
                .iter()
                .any(|n| matches!(n, crate::Notice::SelectionChanged { .. })),
            "a: and `force = 0` means no notice for an unchanged selection"
        );

        // (b) a creature that is not attackable is dropped. Same object, `BF_ATTACKABLE` cleared,
        // which is the difference between a drudge and a townsfolk.
        let mut w = world();
        let mut req = RecordingRequests::default();
        let mut sink = RecordingSink::default();
        w.weenie_mut(ObjectId(2)).unwrap().pwd.bitfield = 0;
        w.combat.combat_mode = CombatMode::NonCombat;
        w.set_combat_mode(&mut req, &mut sink, CombatMode::Missile, false, true, false)
            .unwrap();
        assert_eq!(w.selected, None, "b: an unattackable selection is cleared");
        assert_eq!(
            sink.0
                .iter()
                .filter(|n| matches!(n, crate::Notice::SelectionChanged { .. }))
                .count(),
            1,
            "b: exactly one SelectionChanged"
        );

        // (c) **the same object, leaving combat**: the fixup is gated on the new mode, so peace
        // and magic must leave even an unattackable selection alone. This is the second station,
        // and without it (b) would also pass against a fixup that ran in every mode.
        let mut w = world();
        let mut req = RecordingRequests::default();
        let mut sink = RecordingSink::default();
        w.weenie_mut(ObjectId(2)).unwrap().pwd.bitfield = 0;
        w.combat.combat_mode = CombatMode::Missile;
        w.set_combat_mode(
            &mut req,
            &mut sink,
            CombatMode::NonCombat,
            false,
            true,
            false,
        )
        .unwrap();
        assert_eq!(
            w.selected,
            Some(ObjectId(2)),
            "c: peace does not touch the selection"
        );
        w.set_combat_mode(&mut req, &mut sink, CombatMode::Magic, false, true, false)
            .unwrap();
        assert_eq!(w.selected, Some(ObjectId(2)), "c: nor does magic");

        // (d) the player's own selection survives, because the guard is `selected != player`
        // and `get_attack_target` would answer `None` for it — the fixup would otherwise clear it.
        let mut w = world();
        let mut req = RecordingRequests::default();
        let mut sink = RecordingSink::default();
        w.set_selected_object(Some(ObjectId(1)), false, &mut sink);
        w.combat.combat_mode = CombatMode::NonCombat;
        w.set_combat_mode(&mut req, &mut sink, CombatMode::Melee, false, true, false)
            .unwrap();
        assert_eq!(
            w.selected,
            Some(ObjectId(1)),
            "d: the player keeps himself selected"
        );
    }

    /// Run one combat-update frame at `t`.
    fn frame(w: &mut World, req: &mut RecordingRequests, t: f64) {
        w.combat_power_bar_use_time(req, true, LocalTime(t));
    }

    fn attacks(req: &RecordingRequests) -> Vec<f32> {
        req.0
            .iter()
            .filter_map(|r| match r {
                Request::TargetedMeleeAttack(a) => Some(a.power_level),
                _ => None,
            })
            .collect()
    }

    fn cancels(req: &RecordingRequests) -> usize {
        req.0
            .iter()
            .filter(|r| matches!(r, Request::CancelAttack(_)))
            .count()
    }

    /// **A single click starts the charge, and the swing happens frames later, on arrival.**
    ///
    /// The owner's refutation of the held-control reading, asserted the way he described it:
    /// press and release inside one frame, then *nothing* for as long as the bar takes, then one
    /// attack at the requested level. A test that only looked at the release edge — which is what
    /// this build had — cannot tell that from a control that never fires at all.
    ///
    /// The gauge is left at its default of **0.5**, so the arrival is at the
    /// halfway point of a 1.000 s charge and the frame it lands on is a fact about the clock
    /// rather than about the frame rate.
    #[test]
    fn a_click_starts_the_charge_and_the_attack_fires_when_the_bar_reaches_the_requested_level() {
        let mut w = world();
        let mut req = RecordingRequests::default();

        assert!(
            (w.combat.ui_requested_power - 0.5).abs() < 1e-9,
            "the shipped gauge default"
        );

        // The click. `0x1C` / the key press -> `set_requested_attack_height` -> `start_attack_request`;
        // the completed click -> `end_attack_request(height, -1.0)`. Both inside one frame, which is
        // what "a single click" means.
        w.set_requested_attack_height(AttackHeight::High, true, LocalTime(10.0))
            .unwrap();
        assert!(w.combat.build_in_progress, "the press started the bar");
        assert!(w.combat.attack_request_in_progress);
        w.end_attack_request(&mut req, AttackHeight::High, None, true, LocalTime(10.0));

        // **The release fired nothing.** This is the assertion the held reading fails.
        assert!(
            attacks(&req).is_empty(),
            "a click does not swing on the release edge"
        );
        assert!(
            !w.combat.attack_request_in_progress,
            "but the request did end"
        );
        assert!(w.combat.build_in_progress, "and the bar is still charging");
        assert!(
            (w.combat.requested_attack_power - 0.5).abs() < 1e-9,
            "ending the attack request took max(level, cap) and level was 0"
        );

        // Now the frames. Each one is a separate observation, and the silence before arrival is
        // asserted per frame rather than once at the end: a swing that fired early and a swing
        // that fired late both pass an "eventually one attack" test.
        for i in 1..=4 {
            frame(&mut w, &mut req, 10.0 + f64::from(i) * 0.1);
            assert!(
                attacks(&req).is_empty(),
                "frame {i}: still charging, nothing sent"
            );
            assert!(
                w.combat.build_in_progress,
                "frame {i}: and the build is still live"
            );
            let expected = f64::from(i) * 0.1;
            #[allow(clippy::cast_possible_truncation)]
            let expected = expected as f32;
            assert!(
                (w.combat.latest_power_bar_level - expected).abs() < 1e-5,
                "frame {i}: the bar follows the clock, not the cap"
            );
        }

        // t = 10.5: level == 0.5 == the cap. Retail tests `level < requested`, so
        // equality is **arrival**, not "still charging". The boundary is the whole behaviour.
        frame(&mut w, &mut req, 10.5);
        assert_eq!(
            attacks(&req),
            vec![0.5],
            "one attack, at the requested level"
        );
        assert!(
            !w.combat.build_in_progress,
            "executing the attack ends the build"
        );
        assert!(
            (w.combat.latest_power_bar_level - 0.5).abs() < 1e-9,
            "the bar is pinned at the cap, not at the clock"
        );

        // And it does not fire again on the next frame.
        frame(&mut w, &mut req, 10.6);
        assert_eq!(attacks(&req).len(), 1, "exactly one swing per click");
    }

    /// **A frame that lands *past* the arrival still swings at the requested level, not at the
    /// clock's.** `min(requested_attack_power, level)`.
    ///
    /// **Added because a mutation survived.** The test above steps the clock so that the arrival
    /// frame has `level == requested` exactly, and at equality `min` is the identity — so dropping
    /// it changed nothing there. A real frame almost never lands on the boundary: the arrival is a
    /// clock time and the frames are wherever the frame rate puts them, so the overshoot is the
    /// *normal* case and the equality is the special one. Reading 1 of the four:
    /// the test was weak, and this is the assertion that would have caught it.
    #[test]
    fn a_frame_that_overshoots_the_arrival_still_pins_the_bar_at_the_requested_level() {
        let mut w = world();
        let mut req = RecordingRequests::default();
        w.set_requested_attack_height(AttackHeight::Medium, true, LocalTime(0.0))
            .unwrap();
        w.end_attack_request(&mut req, AttackHeight::Medium, None, true, LocalTime(0.0));
        assert!((w.combat.requested_attack_power - 0.5).abs() < 1e-9);

        // The premise, read *before* the frame consumes the build: the clock really has overshot,
        // so `level` and `requested` are different numbers and `min` is not the identity here.
        // Asserted first because `execute_attack` clears `build_in_progress` and `power_bar_level`
        // then answers 0.0 — the same shape as every other "assert the premise" in this project.
        let level_at_the_frame = w.combat.power_bar_level(LocalTime(0.7));
        assert!(
            level_at_the_frame > 0.69 && level_at_the_frame > w.combat.requested_attack_power,
            "the frame lands past the arrival: level {level_at_the_frame}, requested 0.5"
        );

        // One long frame: the bar is at 0.700 when it is looked at, and the arrival was at 0.500.
        frame(&mut w, &mut req, 0.7);
        assert_eq!(
            attacks(&req),
            vec![0.5],
            "the swing is at the requested level"
        );
        assert!(
            (w.combat.latest_power_bar_level - 0.5).abs() < 1e-9,
            "and so is the bar: min(requested, level), not level"
        );
    }

    /// **The charge is not restarted by a click that arrives while one is already running**, and a
    /// tap below the cap is still one swing. The complement of the test above: it asserts the
    /// arrival, this asserts that nothing else can produce one.
    #[test]
    fn a_second_click_mid_charge_does_not_produce_a_second_swing() {
        let mut w = world();
        let mut req = RecordingRequests::default();
        w.set_requested_attack_height(AttackHeight::Medium, true, LocalTime(0.0))
            .unwrap();
        w.end_attack_request(&mut req, AttackHeight::Medium, None, true, LocalTime(0.0));
        frame(&mut w, &mut req, 0.2);
        let start = w.combat.build_start_time;

        // The same height again. `set_requested_attack_height`'s guard is
        // `old != new || !attack_request_in_progress`, so `start_attack_request` *does* run — and
        // attack-build startup returns immediately when a build is in progress, so the
        // clock is not reset. That distinction is the reason a fast clicker does not stall.
        w.set_requested_attack_height(AttackHeight::Medium, true, LocalTime(0.2))
            .unwrap();
        assert!(
            (w.combat.build_start_time - start).abs() < 1e-9,
            "the charge already running is not restarted"
        );
        w.end_attack_request(&mut req, AttackHeight::Medium, None, true, LocalTime(0.2));
        assert!(attacks(&req).is_empty());

        frame(&mut w, &mut req, 0.5);
        assert_eq!(attacks(&req), vec![0.5], "still exactly one swing");
    }

    /// **`AutoRepeatAttack`, both ways** — the checkbox the owner named, and the reason it ships
    /// default-on.
    ///
    /// With automatic repeat enabled, the server's *attack done* restarts
    /// the bar (`start_power_bar_build`, `current_build_is_automatic = true`); with it off,
    /// `repeat_attacking` is cleared and the bar is hidden. **Both arms are asserted, because a
    /// repeat that never stops and one that never starts both pass a single-shot test.**
    ///
    /// The automatic build's own arrival is asserted too: the frame update's
    /// `current_build_is_automatic` arm **stops** the build instead of swinging, because the swing
    /// is the server's while the sustained attack runs. Without that arm the client would swing
    /// once per second for ever off one click.
    #[test]
    fn auto_repeat_attack_decides_whether_the_charge_restarts_by_itself() {
        for on in [true, false] {
            let mut w = world();
            w.player_system
                .options
                .set(crate::player::options::option::AUTO_REPEAT_ATTACK, on);
            let mut req = RecordingRequests::default();

            w.set_requested_attack_height(AttackHeight::Medium, true, LocalTime(0.0))
                .unwrap();
            w.end_attack_request(&mut req, AttackHeight::Medium, None, true, LocalTime(0.0));
            frame(&mut w, &mut req, 0.5);
            assert_eq!(
                attacks(&req),
                vec![0.5],
                "{on}: the first swing is the same either way"
            );
            assert_eq!(
                w.combat.repeat_attacking, on,
                "{on}: executing the attack sets the repeat flag only when the box is on"
            );

            // The server answers. `result == 0` is a clean finish.
            w.handle_attack_done(&mut req, 0, true, LocalTime(1.0));

            assert_eq!(
                w.combat.build_in_progress, on,
                "{on}: the charge restarts by itself only with the box on"
            );
            if on {
                assert!(
                    w.combat.current_build_is_automatic,
                    "and the restart is the automatic one"
                );
                assert_eq!(
                    w.combat.power_bar_mode,
                    PowerBarMode::Combat,
                    "the bar stays up"
                );
                assert!(
                    (w.combat.build_start_time - 1.0).abs() < 1e-9,
                    "restarted at *now*"
                );

                // The automatic build arrives and **does not swing**: the sustained attack is the
                // server's until `Request::CancelAttack`.
                for t in [1.1, 1.3, 1.49] {
                    frame(&mut w, &mut req, t);
                    assert_eq!(
                        attacks(&req).len(),
                        1,
                        "automatic build at {t}: no client swing"
                    );
                }
                frame(&mut w, &mut req, 1.5);
                assert_eq!(
                    attacks(&req).len(),
                    1,
                    "arrival of an automatic build never swings"
                );
                assert!(!w.combat.build_in_progress, "it stops the build instead");
            } else {
                assert!(
                    !w.combat.repeat_attacking,
                    "off: the repeat flag is cleared"
                );
                assert_eq!(
                    w.combat.power_bar_mode,
                    PowerBarMode::Undef,
                    "off: the bar is hidden"
                );
                // And no number of frames produces a second swing without another click.
                for t in [1.1, 1.5, 2.0, 5.0] {
                    frame(&mut w, &mut req, t);
                }
                assert_eq!(attacks(&req).len(), 1, "off: you must click again");
            }
        }
    }

    /// The not-ready arm of the same function, asserted because it is the only producer of a
    /// `CancelAttack` from the frame loop and it is gated on the **option** rather than on
    /// `repeat_attacking` — an asymmetry that reads like a bug and is the client's.
    #[test]
    fn losing_the_ready_position_mid_charge_cancels_and_hides_the_bar() {
        let mut w = world();
        let mut req = RecordingRequests::default();
        w.set_requested_attack_height(AttackHeight::Medium, true, LocalTime(0.0))
            .unwrap();
        w.end_attack_request(&mut req, AttackHeight::Medium, None, true, LocalTime(0.0));
        assert!(w.combat.build_in_progress);
        assert!(!w.combat.repeat_attacking, "nothing has swung yet");

        w.combat_power_bar_use_time(&mut req, false, LocalTime(0.2));
        assert_eq!(
            cancels(&req),
            1,
            "the cancel is sent on the option, not on the repeat flag"
        );
        assert!(!w.combat.build_in_progress);
        assert_eq!(
            w.combat.power_bar_mode,
            PowerBarMode::Undef,
            "the bar is hidden, not a stall"
        );
        assert!(attacks(&req).is_empty(), "and nothing swung");
    }

    /// **The re-arm, and it took a surviving mutation to find the case that needs
    /// it.**
    ///
    /// `if (attack_request_in_progress && !build_in_progress && !attack_server_response_pending)
    ///  attempt_start_building_attack();` is the last thing the frame update does before the mode
    /// retry.
    /// Deleting it survived every test this unit had written, because none of them reached the one
    /// state that can satisfy all three conditions at once: **the control is still held and the
    /// build has been torn down under it.**
    ///
    /// The producer of that state is the not-ready arm two tests up — its
    /// `attack_request_in_progress` branch clears `build_in_progress` and zeroes the clock but
    /// deliberately does **not** call `hide_power_bar`, so the bar stays up with nothing charging
    /// it.
    /// Without this re-arm a player who is interrupted mid-swing keeps holding the key over a dead
    /// bar until they let go. Reading 1 of the four: the code is right, the test was weak.
    #[test]
    fn a_held_request_re_arms_its_build_once_the_ready_position_comes_back() {
        let mut w = world();
        let mut req = RecordingRequests::default();
        // Held, not clicked: no `end_attack_request`, so the request stays open.
        w.set_requested_attack_height(AttackHeight::Medium, true, LocalTime(0.0))
            .unwrap();
        assert!(w.combat.attack_request_in_progress && w.combat.build_in_progress);

        // Interrupted. The bar is torn down but the request survives, which is the asymmetry.
        w.combat_power_bar_use_time(&mut req, false, LocalTime(0.2));
        assert!(
            w.combat.attack_request_in_progress,
            "the control is still down"
        );
        assert!(!w.combat.build_in_progress, "and the build is gone");
        assert_eq!(
            w.combat.power_bar_mode,
            PowerBarMode::Combat,
            "the bar was NOT hidden"
        );
        assert!(
            !w.combat.attack_server_response_pending,
            "the third condition holds too"
        );

        // Ready again: the next frame re-arms it, at *now* rather than at the original press.
        frame(&mut w, &mut req, 0.6);
        assert!(
            w.combat.build_in_progress,
            "the held request re-arms its own build"
        );
        assert!(
            (w.combat.build_start_time - 0.6).abs() < 1e-9,
            "restarted at this frame"
        );
        assert!(attacks(&req).is_empty(), "and re-arming is not a swing");

        // And it charges from there, so the interruption cost the player the charge and not the
        // swing: releasing after a full second still swings.
        frame(&mut w, &mut req, 1.2);
        assert!(attacks(&req).is_empty(), "still held, still charging");
        w.end_attack_request(&mut req, AttackHeight::Medium, None, true, LocalTime(1.6));
        assert!(
            !attacks(&req).is_empty(),
            "the release swings off the re-armed build"
        );
    }

    /// The keyboard gauge steps one seventh at a time and saturates.
    #[test]
    fn the_keyboard_gauge_steps_one_seventh_at_a_time_and_saturates() {
        let mut c = CombatState::begin();
        // The seven notches, walked up from the bottom.
        c.ui_requested_power = 0.0;
        let up: Vec<f32> = (0..8).map(|_| c.adjust_ui_requested_power(true)).collect();
        let sixth = 1.0_f32 / 6.0;
        for (i, v) in up.iter().enumerate().take(6) {
            let want = sixth * (i + 1) as f32;
            assert!((v - want).abs() < 1e-6, "step {i}: {v} != {want}");
        }
        assert!(
            (up[6] - 1.0).abs() < 1e-9,
            "the sixth step is the top notch"
        );
        assert!(
            (up[7] - 1.0).abs() < 1e-9,
            "and stepping up at the top saturates"
        );

        // Down again, ending at 0 and staying there. Both directions, because one step expression
        // produces `+1` or `-1` and a sign error is invisible from one side.
        let down: Vec<f32> = (0..8).map(|_| c.adjust_ui_requested_power(false)).collect();
        assert!(
            (down[5] - 0.0).abs() < 1e-9,
            "six steps down from the top is the bottom notch"
        );
        assert!(
            (down[6] - 0.0).abs() < 1e-9,
            "and stepping down at the bottom saturates"
        );

        // **Round-half-up onto the nearest notch, which is what the `+0.5/6` before the truncation
        // is for.** From a value *between* notches the key does not step by a sixth from where it
        // was; it snaps to the neighbouring notch. This is the assertion that fails if the offset
        // is dropped, and it is only reachable because the scrollbar can put it here.
        c.ui_requested_power = 0.30;
        assert!(
            (c.adjust_ui_requested_power(true) - sixth * 3.0).abs() < 1e-6,
            "0.30 rounds to notch 2 and steps to notch 3"
        );
        c.ui_requested_power = 0.30;
        assert!(
            (c.adjust_ui_requested_power(false) - sixth).abs() < 1e-6,
            "and downwards to notch 1"
        );

        // The seven notches are the *whole* reachable set from this arm — the count as a count.
        let mut reachable = std::collections::BTreeSet::new();
        for start in 0..=100_u8 {
            let mut c = CombatState::begin();
            c.ui_requested_power = f32::from(start) / 100.0;
            reachable.insert(dereth_primitives::num::to_i32(
                c.adjust_ui_requested_power(true) * 1e6,
            ));
            let mut c = CombatState::begin();
            c.ui_requested_power = f32::from(start) / 100.0;
            reachable.insert(dereth_primitives::num::to_i32(
                c.adjust_ui_requested_power(false) * 1e6,
            ));
        }
        assert_eq!(
            reachable.len(),
            7,
            "seven notches and no others: {reachable:?}"
        );
    }

    /// **The scrollbar arm is continuous, and nobody has to choose between the two readings.**
    ///
    /// The power scrollbar's element-message `0x0A` arm computes
    /// `requested_power = clamp(p1 * 0.001, 0, 1)` from an unsigned position. Its store is
    /// a plain 32-bit move with **no quantisation**. The owner thought a drag could land between
    /// the notches and asked to be reminded to check; it can, and the client says so.
    ///
    /// The load-bearing assertion is the last one: a drag reaches values the keyboard arm above
    /// **cannot produce at all**. Asserting the two paths together would let a single wrong
    /// implementation — quantising the drag — pass both.
    #[test]
    fn the_scrollbar_gauge_is_continuous_and_reaches_values_the_keyboard_cannot() {
        let mut c = CombatState::begin();
        assert!((c.set_ui_requested_power_from_scrollbar(0) - 0.0).abs() < 1e-9);
        assert!((c.set_ui_requested_power_from_scrollbar(1000) - 1.0).abs() < 1e-9);
        assert!(
            (c.set_ui_requested_power_from_scrollbar(1) - 0.001).abs() < 1e-7,
            "one thousandth"
        );
        assert!(
            (c.set_ui_requested_power_from_scrollbar(2500) - 1.0).abs() < 1e-9,
            "clamped above"
        );
        // The `jge`: `dwParam1` is read back as the `ulong` the message carries,
        // so a position with the top bit set is a huge positive and clamps, never a negative.
        assert!(
            (c.set_ui_requested_power_from_scrollbar(0x8000_0000) - 1.0).abs() < 1e-9,
            "the unsigned fixup, not a wrap to 0"
        );

        // How many of the 1,001 drag positions land *between* notches. **This is a count produced
        // by a threshold, so the threshold is stated and two brackets are given rather than one
        // number** — the first draft of this test asserted a single figure that
        // was wrong by six, and the six were entirely the epsilon.
        let notches: Vec<f32> = (0..=6_u8).map(|i| f32::from(i) / 6.0).collect();
        let mut between = |tol: f32| {
            (0..=1000_u32)
                .filter(|&pos| {
                    let v = c.set_ui_requested_power_from_scrollbar(pos);
                    notches.iter().all(|n| (v - n).abs() > tol)
                })
                .count()
        };
        let (wide, tight) = (between(1e-2), between(1e-3));
        assert!(
            (875..=885).contains(&wide),
            "at 0.010: {wide} of 1,001 between notches"
        );
        assert!(
            (983..=993).contains(&tight),
            "at 0.001: {tight} of 1,001 between notches"
        );
        assert!(
            tight > wide,
            "a tighter tolerance can only find more of them"
        );

        // And the two arms disagree on the same request, which is the point of filing them apart:
        // the drag sets 0.30 exactly, the key would have snapped it to a sixth.
        assert!((c.set_ui_requested_power_from_scrollbar(300) - 0.3).abs() < 1e-6);
        assert!(
            notches.iter().all(|n| (0.3 - n).abs() > 1e-3),
            "0.300 is not a notch, and the keyboard has no way to produce it"
        );
    }
}
