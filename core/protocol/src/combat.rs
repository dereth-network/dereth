//! Family: combat and magic — `docs/networking/messages/06-combat-and-magic.md`.
//!
//! The enchantment and spellbook messages of the magic half live in [`crate::qualities`], with the
//! other quality updates; what is here is the combat set plus the cast and spell-bar actions.
//!
//! **Two corrections the community catalogue gets wrong, both in `0x01B1`/`0x01B2`:**
//!
//! * `percent` is a **`f64`**, not an `f32`. The client advances exactly 0x18 (attacker) or 0x1C
//!   (defender) bytes past the name string, and ACE writes a `double`. Reading it as a float shifts
//!   every following field.
//! * `attackConditions` is **4 bytes** where ACE writes 8. The UI
//!   queue's blob dispatcher reads it as a single 32-bit value and sign-extends it,
//!   in both the `0x01B1` and the `0x01B2` arm — and advances the cursor by **four**, to `0x18` /
//!   `0x1C` past the string. The sign extension is there because the client's in-memory `AttackConditions` is 64-bit while the wire field is 32-bit,
//!   which is the likeliest reason every emulator widened it. It is the last field and neither arm
//!   ever compares the cursor against the end of the blob, so the servers' extra four bytes are
//!   trailing slack the retail client never reads. ACE's `Writer.Write((ulong)attackConditions)`
//!   and GDLE's `Write<uint64_t>` (attacker) / `Write<uint32_t>` then
//!   `Write<uint32_t>(0) // probably uint32_t align` for the defender
//!   agree on the bytes. **The retail server wrote them too**: every one of 41,767 retail `0x01B1`/`0x01B2` bodies in the January 2017
//!   captures carries the eight-byte field, and its high dword is zero in all 41,767 (the low dword
//!   varies: attacker 0/2/4/6, defender 0/1/4/5). So the codec reads the client's four bytes and
//!   then, when four more remain, the server's high dword ([`AttackerNotification::attack_conditions_high`]);
//!   it writes the eight-byte form, as retail's server, ACE and ours do (V293). A four-byte body
//!   still reads, with the high dword 0. Asserted on the 47 recorded ACE bodies by the
//!   client-session tests, and on retail bodies by this module's tests.
//!
//! And one where ACE is wrong: the damage-location table has **28** entries, not ACE's 0–8, and the
//! higher values are reachable in normal play on non-humanoid creatures.

use crate::archive::{Reader, Writer};
use crate::error::MessageError;
use crate::opcodes::Opcode;
use crate::Message;
use dereth_primitives::ObjectId;

/// `AttackConditions`, the bits a damage notification carries beside its numbers. Each set bit
/// adds a word to the combat line: `0x8` "Overpower! ", `0x4` "Sneak Attack! ", `0x2`
/// "Recklessness! " (the defender's line says "Reckless! "), and `0x1` the critical-protection
/// sentence at the end.
pub mod attack_conditions {
    pub const CRITICAL_PROTECTION_AUGMENTATION: u32 = 0x1;
    pub const RECKLESSNESS: u32 = 0x2;
    pub const SNEAK_ATTACK: u32 = 0x4;
    /// The hit overpowered the target's defences: "Overpower! " follows the critical-hit prefix
    /// and comes before the sneak-attack one, on both the attacker's and the defender's line.
    pub const OVERPOWER: u32 = 0x8;
}

/// The damage-location table.
///
/// **28 entries**, 0..=27, plus `-1` for undefined. ACE's `DamageLocation` enum defines only 0–8;
/// the retail server used the higher values for non-humanoid creatures, so a rebuild must keep the
/// full table. Values 11 and 14 are gaps and render as `Unknown`.
///
/// The client lower-cases the name and replaces `_` with a space before display
/// (the display mapper lower-cases and drops underscores), so `UPPER_ARM` renders `upper arm`.
#[must_use]
pub fn damage_location_name(v: i32) -> Option<&'static str> {
    Some(match v {
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
        12 => "FRONT_FOOT",
        13 => "REAR_LEG",
        15 => "REAR_FOOT",
        16 => "TORSO",
        17 => "TAIL",
        18 => "ARM",
        19 => "LEG",
        20 => "CLAW",
        21 => "WINGS",
        22 => "BREATH",
        23 => "TENTACLE",
        24 => "UPPER_TENTACLE",
        25 => "LOWER_TENTACLE",
        26 => "CLOAK",
        27 => "NUM",
        -1 => "UNDEFINED",
        // 11, 14 and anything else render as "Unknown".
        _ => return None,
    })
}

/// The number of named damage locations the client's table holds, `NUM` included.
pub const DAMAGE_LOCATION_COUNT: usize = 26;

// ---------------------------------------------------------------------------------------------
// Client → server
// ---------------------------------------------------------------------------------------------

/// `0x0008 Combat_TargetedMeleeAttack` and `0x000A Combat_TargetedMissileAttack` share a layout;
/// only the third field's meaning differs (attack power versus accuracy).
macro_rules! targeted_attack {
    ($(#[$m:meta])* $name:ident, $op:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Default)]
        pub struct $name {
            pub target: ObjectId,
            /// `ATTACK_HEIGHT`: 1, 2 or 3.
            pub attack_height: u32,
            /// 0.0–1.0.
            pub power_level: f32,
        }

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;

            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self {
                    target: ObjectId(r.u32()?),
                    attack_height: r.u32()?,
                    power_level: r.f32()?,
                })
            }

            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.u32(self.target.0);
                w.u32(self.attack_height);
                w.f32(self.power_level);
                Ok(())
            }
        }
    };
}

targeted_attack!(
    /// `0x0008` — sent only when `combatMode == MELEE_COMBAT_MODE` and the target is attackable.
    CombatTargetedMeleeAttack,
    COMBAT_TARGETED_MELEE_ATTACK
);
targeted_attack!(
    /// `0x000A` — the same layout; the third field is the *accuracy* level.
    CombatTargetedMissileAttack,
    COMBAT_TARGETED_MISSILE_ATTACK
);

empty_message!(
    /// `0x01B7 Combat_CancelAttack` (C2S) — no payload.
    CombatCancelAttack,
    COMBAT_CANCEL_ATTACK
);
empty_message!(
    /// `0x01B8 Combat_HandleCommenceAttackEvent` (S2C) — no payload. It increments the UI busy
    /// count, so a server that sends it must eventually send `0x01A7`.
    CombatHandleCommenceAttackEvent,
    COMBAT_HANDLE_COMMENCE_ATTACK_EVENT
);
empty_message!(
    /// `0x0026 Character_TeleToPKLArena`.
    CharacterTeleToPklArena,
    CHARACTER_TELE_TO_PKLARENA
);
empty_message!(
    /// `0x0027 Character_TeleToPKArena`.
    CharacterTeleToPkArena,
    CHARACTER_TELE_TO_PKARENA
);
empty_message!(
    /// `0x028F Character_EnterPKLite` — the server answers with a confirmation request.
    CharacterEnterPkLite,
    CHARACTER_ENTER_PKLITE
);
empty_message!(
    /// `0x0279 Character_Suicide` — the client shows its own confirmation dialog first.
    CharacterSuicide,
    CHARACTER_SUICIDE
);
empty_message!(
    /// `0x0063 Character_TeleToLifestone`.
    CharacterTeleToLifestone,
    CHARACTER_TELE_TO_LIFESTONE
);
empty_message!(
    /// `0x028D Character_TeleToMarketplace`.
    CharacterTeleToMarketplace,
    CHARACTER_TELE_TO_MARKETPLACE
);

/// `0x0053 Combat_ChangeCombatMode` (C2S).
///
/// The client learns the authoritative value back through `Qualities_UpdateInt` on property `0x28`,
/// not through a reply to this message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CombatChangeCombatMode {
    /// `COMBAT_MODE`.
    pub combat_mode: u32,
}

impl Message for CombatChangeCombatMode {
    const OPCODE: Opcode = Opcode::COMBAT_CHANGE_COMBAT_MODE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            combat_mode: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.combat_mode);
        Ok(())
    }
}

/// `0x01A7 Combat_HandleAttackDoneEvent` (S2C).
///
/// The community catalogue calls this field `number` and says it is unused. **That is wrong**: a
/// non-zero code makes the client abort an auto-repeat attack and send `Combat_CancelAttack`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CombatHandleAttackDoneEvent {
    /// A `WeenieError`; 0 = success.
    pub error: u32,
}

impl Message for CombatHandleAttackDoneEvent {
    const OPCODE: Opcode = Opcode::COMBAT_HANDLE_ATTACK_DONE_EVENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self { error: r.u32()? })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.error);
        Ok(())
    }
}

/// `0x01B1 Combat_HandleAttackerNotificationEvent` (S2C) — "You hurt X."
///
/// The client advances exactly **0x18** bytes past the name string. `percent` is an `f64`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AttackerNotification {
    pub defender_name: String,
    /// `DAMAGE_TYPE`, a bit mask; the verb table keys off the lowest set bit.
    pub damage_type: u32,
    /// **`f64`** — the catalogue says `f32`. Fraction of the target's max health removed;
    /// the hit-adjective lookup writes nothing when it is negative.
    pub percent: f64,
    pub damage: u32,
    pub critical: u32,
    /// The low dword, the only part the client reads: it reads one dword, sign-extends it and
    /// advances by 4. See the module doc.
    pub attack_conditions: u32,
    /// The high dword the server writes after it (retail's server, ACE and ours all write the
    /// field as eight bytes); zero in every retail capture. Never read by the client; 0 when the
    /// body stops after the low dword.
    pub attack_conditions_high: u32,
}

/// The server's high dword of `attackConditions`, when the body carries one.
fn attack_conditions_high(r: &mut Reader<'_>) -> Result<u32, MessageError> {
    if r.remaining() >= 4 {
        r.u32()
    } else {
        Ok(0)
    }
}

impl Message for AttackerNotification {
    const OPCODE: Opcode = Opcode::COMBAT_HANDLE_ATTACKER_NOTIFICATION_EVENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            defender_name: r.pstring()?,
            damage_type: r.u32()?,
            percent: r.f64()?,
            damage: r.u32()?,
            critical: r.u32()?,
            attack_conditions: r.u32()?,
            attack_conditions_high: attack_conditions_high(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.defender_name)?;
        w.u32(self.damage_type);
        w.f64(self.percent);
        w.u32(self.damage);
        w.u32(self.critical);
        w.u32(self.attack_conditions);
        w.u32(self.attack_conditions_high);
        Ok(())
    }
}

/// `0x01B2 Combat_HandleDefenderNotificationEvent` (S2C) — "X hurts you."
///
/// The same shape plus a damage location; the client advances **0x1C** bytes past the string.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DefenderNotification {
    pub attacker_name: String,
    pub damage_type: u32,
    /// **`f64`**, as in [`AttackerNotification`].
    pub percent: f64,
    pub damage: u32,
    /// A body part; see [`damage_location_name`]. The client understands 0–27, ACE only 0–8.
    pub damage_location: u32,
    pub critical: u32,
    /// The low dword, the only part the client reads, as in [`AttackerNotification`]. GDLE
    /// writes this one as a `uint32_t` followed by an explicit zero dword it labels *"probably
    /// uint32_t align"*; ACE writes one `ulong`; retail's server wrote eight bytes.
    pub attack_conditions: u32,
    /// The server's high dword, as in [`AttackerNotification::attack_conditions_high`].
    pub attack_conditions_high: u32,
}

impl Message for DefenderNotification {
    const OPCODE: Opcode = Opcode::COMBAT_HANDLE_DEFENDER_NOTIFICATION_EVENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            attacker_name: r.pstring()?,
            damage_type: r.u32()?,
            percent: r.f64()?,
            damage: r.u32()?,
            damage_location: r.u32()?,
            critical: r.u32()?,
            attack_conditions: r.u32()?,
            attack_conditions_high: attack_conditions_high(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.attacker_name)?;
        w.u32(self.damage_type);
        w.f64(self.percent);
        w.u32(self.damage);
        w.u32(self.damage_location);
        w.u32(self.critical);
        w.u32(self.attack_conditions);
        w.u32(self.attack_conditions_high);
        Ok(())
    }
}

string_message!(
    /// `0x01B3 Combat_HandleEvasionAttackerNotificationEvent` — "X evaded your attack. "
    EvasionAttackerNotification,
    COMBAT_HANDLE_EVASION_ATTACKER_NOTIFICATION_EVENT,
    defender_name
);
string_message!(
    /// `0x01B4 Combat_HandleEvasionDefenderNotificationEvent` — "You evaded X! ", and unlike the
    /// attacker variant it also runs the auto-target pair.
    EvasionDefenderNotification,
    COMBAT_HANDLE_EVASION_DEFENDER_NOTIFICATION_EVENT,
    attacker_name
);
string_message!(
    /// `0x01AC Combat_HandleVictimNotificationEventSelf`.
    ///
    /// Both `0x01AC` and `0x01AD` reach the **same** handler with identical arguments: the client
    /// draws no distinction between "you died" and "you killed something". The difference exists
    /// only in ACE.
    VictimNotificationSelf,
    COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_SELF,
    message
);
string_message!(
    /// `0x01AD Combat_HandleVictimNotificationEventOther`.
    VictimNotificationOther,
    COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_OTHER,
    message
);

/// `0x019E Combat_HandlePlayerDeathEvent` (S2C, **unordered**).
///
/// Printed only when neither id is the local player — the local player's own death is reported by
/// `0x01AC`/`0x01AD`. This is purely the "someone nearby was PKed" broadcast.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CombatHandlePlayerDeathEvent {
    pub message: String,
    pub killed: ObjectId,
    pub killer: ObjectId,
}

impl Message for CombatHandlePlayerDeathEvent {
    const OPCODE: Opcode = Opcode::COMBAT_HANDLE_PLAYER_DEATH_EVENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            message: r.pstring()?,
            killed: ObjectId(r.u32()?),
            killer: ObjectId(r.u32()?),
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.message)?;
        w.u32(self.killed.0);
        w.u32(self.killer.0);
        Ok(())
    }
}

/// `0x01BF Combat_QueryHealth` (C2S).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CombatQueryHealth {
    pub target: ObjectId,
}

impl Message for CombatQueryHealth {
    const OPCODE: Opcode = Opcode::COMBAT_QUERY_HEALTH;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            target: ObjectId(r.u32()?),
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.target.0);
        Ok(())
    }
}

/// `0x01C0 Combat_QueryHealthResponse` (S2C).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CombatQueryHealthResponse {
    pub object: ObjectId,
    /// 0.0–1.0.
    pub health: f32,
}

impl Message for CombatQueryHealthResponse {
    const OPCODE: Opcode = Opcode::COMBAT_QUERY_HEALTH_RESPONSE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            object: ObjectId(r.u32()?),
            health: r.f32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.object.0);
        w.f32(self.health);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// Magic — the cast and spell-bar actions
// ---------------------------------------------------------------------------------------------

/// `0x0048 Magic_CastUntargetedSpell` (C2S).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MagicCastUntargetedSpell {
    pub spell_id: u32,
}

impl Message for MagicCastUntargetedSpell {
    const OPCODE: Opcode = Opcode::MAGIC_CAST_UNTARGETED_SPELL;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self { spell_id: r.u32()? })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.spell_id);
        Ok(())
    }
}

/// `0x004B Magic_TestSpellFormula` (C2S): a spell formula tried on a target — the eight
/// component slots of the formula (spell component ids, unused slots zero), then the target.
/// Sent by the spell research panel of clients up to January 2002; a server answers with a
/// cast of the spell the formula makes, or a casting refusal such as an impossible spell path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MagicTestSpellFormula {
    pub components: [u32; 8],
    pub target: ObjectId,
}

impl Message for MagicTestSpellFormula {
    const OPCODE: Opcode = Opcode::MAGIC_TEST_SPELL_FORMULA;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let mut components = [0; 8];
        for c in &mut components {
            *c = r.u32()?;
        }
        Ok(Self {
            components,
            target: ObjectId(r.u32()?),
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        for c in self.components {
            w.u32(c);
        }
        w.u32(self.target.0);
        Ok(())
    }
}

/// `0x004A Magic_CastTargetedSpell` (C2S) — **target first, spell second**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MagicCastTargetedSpell {
    pub target: ObjectId,
    pub spell_id: u32,
}

impl Message for MagicCastTargetedSpell {
    const OPCODE: Opcode = Opcode::MAGIC_CAST_TARGETED_SPELL;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            target: ObjectId(r.u32()?),
            spell_id: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.target.0);
        w.u32(self.spell_id);
        Ok(())
    }
}

/// `0x0224 Character_SetDesiredComponentLevel` (C2S).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ComponentLevelRequest {
    /// A spell-component (taper) DataID.
    pub component_did: u32,
    pub level: i32,
}

impl Message for ComponentLevelRequest {
    const OPCODE: Opcode = Opcode::CHARACTER_SET_DESIRED_COMPONENT_LEVEL;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            component_did: r.u32()?,
            level: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.component_did);
        w.i32(self.level);
        Ok(())
    }
}

/// `0x0286 Character_SpellbookFilterEvent` (C2S).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharacterSpellbookFilterEvent {
    pub filter_mask: u32,
}

impl Message for CharacterSpellbookFilterEvent {
    const OPCODE: Opcode = Opcode::CHARACTER_SPELLBOOK_FILTER_EVENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            filter_mask: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.filter_mask);
        Ok(())
    }
}

/// `0x01E3 Character_AddSpellFavorite` (C2S) — 16 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharacterAddSpellFavorite {
    pub spell_id: u32,
    /// Slot in the bar.
    pub index: i32,
    /// The spell bar the favourite goes on.
    pub spell_bank: i32,
}

impl Message for CharacterAddSpellFavorite {
    const OPCODE: Opcode = Opcode::CHARACTER_ADD_SPELL_FAVORITE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            spell_id: r.u32()?,
            index: r.i32()?,
            spell_bank: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.spell_id);
        w.i32(self.index);
        w.i32(self.spell_bank);
        Ok(())
    }
}

/// `0x01E4 Character_RemoveSpellFavorite` (C2S) — 12 bytes, no slot index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharacterRemoveSpellFavorite {
    pub spell_id: u32,
    pub spell_bank: i32,
}

impl Message for CharacterRemoveSpellFavorite {
    const OPCODE: Opcode = Opcode::CHARACTER_REMOVE_SPELL_FAVORITE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            spell_id: r.u32()?,
            spell_bank: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.spell_id);
        w.i32(self.spell_bank);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{read_body, round_trip, write_body};

    /// Oracle: `docs/CORRECTIONS.md` and `docs/networking/messages/06-combat-and-magic.md` §1 —
    /// `percent` is a **double**, and the client advances exactly 0x18 bytes past the name string.
    ///
    /// A four-byte `percent` would make the body 0x14 past the
    /// string, and every field after it would be read from the wrong place — which `expect_exhausted`
    /// then catches as a trailing dword.
    #[test]
    fn attacker_notification_percent_is_a_double() {
        let m = AttackerNotification {
            defender_name: "Drudge".into(),
            damage_type: 0x1,
            percent: 0.25,
            damage: 42,
            critical: 0,
            attack_conditions: attack_conditions::SNEAK_ATTACK,
            attack_conditions_high: 0,
        };
        let bytes = write_body(&m).unwrap();
        // "Drudge" packs as 2 + 6 + 0 pad = 8 bytes at blob offset 4.
        assert_eq!(
            bytes.len(),
            8 + 0x1C,
            "0x18 bytes past the string, then the server's high dword"
        );
        // The eight bytes after the damage type are the double.
        let percent_bytes = &bytes[12..20];
        assert_eq!(f64::from_le_bytes(percent_bytes.try_into().unwrap()), 0.25);
        let _: AttackerNotification = round_trip(&bytes);
    }

    /// The failure the `f32` reading causes: four bytes left over, which `expect_exhausted` rejects.
    #[test]
    fn reading_percent_as_a_float_would_leave_a_trailing_dword() {
        let m = AttackerNotification {
            defender_name: "Drudge".into(),
            percent: 0.25,
            ..AttackerNotification::default()
        };
        let bytes = write_body(&m).unwrap();
        // Simulate the wrong reading: string, u32, f32, u32, u32, u32 = 4 bytes short of the body.
        let mut r = Reader::body(&bytes);
        r.pstring().unwrap();
        r.u32().unwrap();
        r.f32().unwrap();
        r.u32().unwrap();
        r.u32().unwrap();
        r.u32().unwrap();
        r.u32().unwrap(); // the server's high dword
        assert_eq!(
            r.remaining(),
            4,
            "the misread leaves the second half of the double"
        );
    }

    /// Oracle: `docs/networking/messages/06-combat-and-magic.md` §1 — the defender form adds a
    /// damage location, so it is 0x1C past the string rather than 0x18.
    #[test]
    fn defender_notification_adds_a_damage_location() {
        let m = DefenderNotification {
            attacker_name: "Drudge".into(),
            damage_type: 0x1,
            percent: 0.25,
            damage: 42,
            damage_location: 3,
            critical: 1,
            attack_conditions: 0,
            attack_conditions_high: 0,
        };
        let bytes = write_body(&m).unwrap();
        assert_eq!(
            bytes.len(),
            8 + 0x1C + 4,
            "0x1C past the string, then the server's high dword"
        );
        let _: DefenderNotification = round_trip(&bytes);
    }

    /// Oracle: retail server bodies from the January 2017 captures (the body after the event
    /// type). They carry the eight-byte `attackConditions`, high dword zero, and round-trip byte
    /// for byte. The low dwords are 2 (Recklessness), 6 (Recklessness | Sneak Attack), 1 (Critical
    /// Protection) and 4 (Sneak Attack).
    #[test]
    fn retail_bodies_carry_eight_bytes_of_attack_conditions_and_round_trip() {
        fn hex(s: &str) -> Vec<u8> {
            (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
                .collect()
        }
        for (body, low) in [
            ("0c005475736b657220536c617665000010000000b81e85eb51b8e23fea000000000000000000000000000000", 0),
            ("0c005475736b65722047756172640000100000003d0ad7a3703dde3fbd000000000000000200000000000000", 2),
            ("0c005475736b657220536c6176650000100000001f85eb51b81ee73f21010000000000000600000000000000", 6),
        ] {
            let body = hex(body);
            let m: AttackerNotification = round_trip(&body);
            assert_eq!((m.attack_conditions, m.attack_conditions_high), (low, 0));
            assert!(m.defender_name.starts_with("Tusker"));
        }
        for (body, low, location) in [
            ("0f0042616e6465726c696e67204f6772650000000400000074c06e8fb50c6d3f0100000008000000000000000000000000000000", 0, 8),
            ("0b0046726f737420476f6c656d0000000400000081849f5c88ba893f0500000007000000000000000100000000000000", 1, 7),
            ("1200456e736f7263656c6c656420576561706f6e04000000a225b3faedf5a03f1000000004000000000000000400000000000000", 4, 4),
        ] {
            let body = hex(body);
            let m: DefenderNotification = round_trip(&body);
            assert_eq!((m.attack_conditions, m.attack_conditions_high, m.damage_location), (low, 0, location));
        }
    }

    /// The four-byte form the client itself reads (no high dword) still decodes, exhausted, with
    /// the high dword 0; it re-encodes as the eight-byte form.
    #[test]
    fn a_body_without_the_high_dword_still_reads() {
        let m = DefenderNotification {
            attacker_name: "Drudge".into(),
            critical: 1,
            attack_conditions: attack_conditions::SNEAK_ATTACK,
            ..DefenderNotification::default()
        };
        let eight = write_body(&m).unwrap();
        let four = &eight[..eight.len() - 4];
        let back: DefenderNotification = read_body(four).unwrap();
        assert_eq!(back, m);
        assert_eq!(write_body(&back).unwrap(), eight);

        let a = AttackerNotification {
            defender_name: "X".into(),
            attack_conditions: 2,
            ..AttackerNotification::default()
        };
        let eight = write_body(&a).unwrap();
        let back: AttackerNotification = read_body(&eight[..eight.len() - 4]).unwrap();
        assert_eq!(back, a);
    }

    /// Oracle: the client's damage-location name table. ACE's enum
    /// stops at 8; the client's table has 28 slots with two gaps.
    #[test]
    fn the_damage_location_table_has_twenty_eight_slots_not_nine() {
        let named = (0..28)
            .filter(|v| damage_location_name(*v).is_some())
            .count();
        assert_eq!(named, DAMAGE_LOCATION_COUNT, "26 named values in 0..=27");
        assert_eq!(damage_location_name(-1), Some("UNDEFINED"));
        // The two gaps.
        assert_eq!(damage_location_name(11), None);
        assert_eq!(damage_location_name(14), None);
        // The values ACE does not define but the client does.
        assert_eq!(damage_location_name(21), Some("WINGS"));
        assert_eq!(damage_location_name(26), Some("CLOAK"));
    }

    /// The overpower bit round trips without being acted on.
    #[test]
    fn the_overpower_bit_round_trips_without_being_acted_on() {
        let m = AttackerNotification {
            defender_name: "X".into(),
            attack_conditions: attack_conditions::OVERPOWER,
            ..AttackerNotification::default()
        };
        let bytes = write_body(&m).unwrap();
        let back: AttackerNotification = round_trip(&bytes);
        assert_eq!(back.attack_conditions, 0x8);
    }

    /// Both victim-notification opcodes reach the same handler with the same body.
    #[test]
    fn the_two_victim_notifications_are_the_same_message() {
        let a = write_body(&VictimNotificationSelf {
            message: "You were slain".into(),
        })
        .unwrap();
        let b = write_body(&VictimNotificationOther {
            message: "You were slain".into(),
        })
        .unwrap();
        assert_eq!(a, b);
        assert_ne!(
            VictimNotificationSelf::OPCODE,
            VictimNotificationOther::OPCODE
        );
    }

    #[test]
    fn every_combat_message_round_trips() {
        let _: CombatTargetedMeleeAttack = round_trip(
            &write_body(&CombatTargetedMeleeAttack {
                target: ObjectId(1),
                attack_height: 2,
                power_level: 1.0,
            })
            .unwrap(),
        );
        let _: CombatTargetedMissileAttack = round_trip(
            &write_body(&CombatTargetedMissileAttack {
                target: ObjectId(1),
                attack_height: 2,
                power_level: 1.0,
            })
            .unwrap(),
        );
        assert_eq!(write_body(&CombatCancelAttack).unwrap().len(), 0);
        assert_eq!(
            write_body(&CombatHandleCommenceAttackEvent).unwrap().len(),
            0
        );
        let _: CombatChangeCombatMode =
            round_trip(&write_body(&CombatChangeCombatMode { combat_mode: 2 }).unwrap());
        let _: CombatHandleAttackDoneEvent =
            round_trip(&write_body(&CombatHandleAttackDoneEvent { error: 0 }).unwrap());
        let _: EvasionAttackerNotification = round_trip(
            &write_body(&EvasionAttackerNotification {
                defender_name: "Drudge".into(),
            })
            .unwrap(),
        );
        let _: EvasionDefenderNotification = round_trip(
            &write_body(&EvasionDefenderNotification {
                attacker_name: "Drudge".into(),
            })
            .unwrap(),
        );
        let _: CombatHandlePlayerDeathEvent = round_trip(
            &write_body(&CombatHandlePlayerDeathEvent {
                message: "Bob was slain".into(),
                killed: ObjectId(1),
                killer: ObjectId(2),
            })
            .unwrap(),
        );
        let _: CombatQueryHealth = round_trip(
            &write_body(&CombatQueryHealth {
                target: ObjectId(1),
            })
            .unwrap(),
        );
        let _: CombatQueryHealthResponse = round_trip(
            &write_body(&CombatQueryHealthResponse {
                object: ObjectId(1),
                health: 0.5,
            })
            .unwrap(),
        );
        let _: MagicCastUntargetedSpell =
            round_trip(&write_body(&MagicCastUntargetedSpell { spell_id: 157 }).unwrap());
        let _: MagicCastTargetedSpell = round_trip(
            &write_body(&MagicCastTargetedSpell {
                target: ObjectId(1),
                spell_id: 157,
            })
            .unwrap(),
        );
        let _: MagicTestSpellFormula = round_trip(
            &write_body(&MagicTestSpellFormula {
                components: [1, 2, 3, 0, 0, 0, 0, 0],
                target: ObjectId(0x5000_0001),
            })
            .unwrap(),
        );
        let _: ComponentLevelRequest = round_trip(
            &write_body(&ComponentLevelRequest {
                component_did: 0x0500_0001,
                level: 3,
            })
            .unwrap(),
        );
        let _: CharacterSpellbookFilterEvent = round_trip(
            &write_body(&CharacterSpellbookFilterEvent {
                filter_mask: 0x3FFF,
            })
            .unwrap(),
        );
        let _: CharacterAddSpellFavorite = round_trip(
            &write_body(&CharacterAddSpellFavorite {
                spell_id: 1,
                index: 2,
                spell_bank: 0,
            })
            .unwrap(),
        );
        let _: CharacterRemoveSpellFavorite = round_trip(
            &write_body(&CharacterRemoveSpellFavorite {
                spell_id: 1,
                spell_bank: 0,
            })
            .unwrap(),
        );
        for m in [
            write_body(&CharacterTeleToPklArena).unwrap(),
            write_body(&CharacterTeleToPkArena).unwrap(),
            write_body(&CharacterEnterPkLite).unwrap(),
            write_body(&CharacterSuicide).unwrap(),
            write_body(&CharacterTeleToLifestone).unwrap(),
            write_body(&CharacterTeleToMarketplace).unwrap(),
        ] {
            assert!(m.is_empty());
        }
        // And the empty ones decode from an empty body.
        assert_eq!(
            read_body::<CombatCancelAttack>(&[]).unwrap(),
            CombatCancelAttack
        );
    }
}
