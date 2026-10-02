//! `MotionCommand`: the 412-entry id table, the wire index, and the class bits.
//!
//! **The table is the final retail client's** (build 0.0.11.6096, June 2015), dumped from its two
//! parallel 412-entry arrays, one of command ids and one of command names. Where the final client
//! differs from the 2013 one, the rebuild follows the final client. Here that is the one change
//! that matters: the 2015 table inserts `SkillHealOther`,
//! `CombatEat` and `CombatDrink` at indices `0x10F`-`0x111`, `StretchUI` at `0x161` and
//! `AI_TelegraphCast` at `0x19B`, and drops `SideBySideVitals`, so every command from `0x10F` on
//! sits three indices higher than in the 2013 table. The end-of-retail data files key their motion
//! tables by these ids, and so does the server, so this is the table that agrees with both.
//!
//! The recorded sessions in `fixtures/` were made with the 2013 client. [`RETAIL_2013_INDEX`] maps
//! its indices onto this table, for reading those recordings and nothing else.
//!
//! **The wire form is the array index, not the id.** Raw-motion unpacking
//! and command lookup both read a `u16` and evaluate
//! `command_ids[index]`. The low 16 bits of every id happen to equal its index, but only because
//! the shipped table is dense and in order; [`MotionCommand::from_index`] goes through the table so
//! that a future gap cannot become a silent bug.
//!
//! Name lookup is a case-insensitive linear scan of the name array, reproduced by
//! [`MotionCommand::from_name`].

use std::fmt;

/// One `MotionCommand` id. The high byte is a class bit-field; the low 24 bits are the ordinal.
///
/// The high-byte bit *names* are inferred from ACE's value table and only
/// `0x20000000` is directly confirmed by the client's own code. Every branch that reads them is
/// traced, so a wrong name costs nothing; the predicates below are the branches.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct MotionCommand(pub u32);

impl fmt::Debug for MotionCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(n) => write!(f, "MotionCommand({n} = 0x{:08X})", self.0),
            None => write!(f, "MotionCommand(0x{:08X})", self.0),
        }
    }
}

impl MotionCommand {
    /// The all-zero command. Not an entry in the table: the client uses a bare `0` in
    /// `MotionState` and in the raw state's sidestep and turn fields to mean "no command
    /// on this axis", which is distinct from [`MotionCommand::INVALID`] (`0x80000000`).
    pub const NONE: Self = Self(0);

    /// Index 0, `Invalid`.
    pub const INVALID: Self = Self(0x80000000);
    /// Index 1, `HoldRun`.
    pub const HOLD_RUN: Self = Self(0x85000001);
    /// Index 2, `HoldSidestep`.
    pub const HOLD_SIDESTEP: Self = Self(0x85000002);
    /// Index 3, `Ready`.
    pub const READY: Self = Self(0x41000003);
    /// Index 4, `Stop`.
    pub const STOP: Self = Self(0x40000004);
    /// Index 5, `WalkForward`.
    pub const WALK_FORWARD: Self = Self(0x45000005);
    /// Index 6, `WalkBackwards`.
    pub const WALK_BACKWARDS: Self = Self(0x45000006);
    /// Index 7, `RunForward`.
    pub const RUN_FORWARD: Self = Self(0x44000007);
    /// Index 8, `Fallen`.
    pub const FALLEN: Self = Self(0x40000008);
    /// Index 9, `Interpolating`.
    pub const INTERPOLATING: Self = Self(0x40000009);
    /// Index 10, `Hover`.
    pub const HOVER: Self = Self(0x4000000A);
    /// Index 11, `On`.
    pub const ON: Self = Self(0x4000000B);
    /// Index 12, `Off`.
    pub const OFF: Self = Self(0x4000000C);
    /// Index 13, `TurnRight`.
    pub const TURN_RIGHT: Self = Self(0x6500000D);
    /// Index 14, `TurnLeft`.
    pub const TURN_LEFT: Self = Self(0x6500000E);
    /// Index 15, `SideStepRight`.
    pub const SIDE_STEP_RIGHT: Self = Self(0x6500000F);
    /// Index 16, `SideStepLeft`.
    pub const SIDE_STEP_LEFT: Self = Self(0x65000010);
    /// Index 17, `Dead`.
    pub const DEAD: Self = Self(0x40000011);
    /// Index 18, `Crouch`.
    pub const CROUCH: Self = Self(0x41000012);
    /// Index 19, `Sitting`.
    pub const SITTING: Self = Self(0x41000013);
    /// Index 20, `Sleeping`.
    pub const SLEEPING: Self = Self(0x41000014);
    /// Index 21, `Falling`.
    pub const FALLING: Self = Self(0x40000015);
    /// Index 22, `Reload`.
    pub const RELOAD: Self = Self(0x40000016);
    /// Index 23, `Unload`.
    pub const UNLOAD: Self = Self(0x40000017);
    /// Index 24, `Pickup`.
    pub const PICKUP: Self = Self(0x40000018);
    /// Index 25, `StoreInBackpack`.
    pub const STORE_IN_BACKPACK: Self = Self(0x40000019);
    /// Index 26, `Eat`.
    pub const EAT: Self = Self(0x4000001A);
    /// Index 27, `Drink`.
    pub const DRINK: Self = Self(0x4000001B);
    /// Index 28, `Reading`.
    pub const READING: Self = Self(0x4000001C);
    /// Index 29, `JumpCharging`.
    pub const JUMP_CHARGING: Self = Self(0x4000001D);
    /// Index 30, `AimLevel`.
    pub const AIM_LEVEL: Self = Self(0x4000001E);
    /// Index 31, `AimHigh15`.
    pub const AIM_HIGH15: Self = Self(0x4000001F);
    /// Index 32, `AimHigh30`.
    pub const AIM_HIGH30: Self = Self(0x40000020);
    /// Index 33, `AimHigh45`.
    pub const AIM_HIGH45: Self = Self(0x40000021);
    /// Index 34, `AimHigh60`.
    pub const AIM_HIGH60: Self = Self(0x40000022);
    /// Index 35, `AimHigh75`.
    pub const AIM_HIGH75: Self = Self(0x40000023);
    /// Index 36, `AimHigh90`.
    pub const AIM_HIGH90: Self = Self(0x40000024);
    /// Index 37, `AimLow15`.
    pub const AIM_LOW15: Self = Self(0x40000025);
    /// Index 38, `AimLow30`.
    pub const AIM_LOW30: Self = Self(0x40000026);
    /// Index 39, `AimLow45`.
    pub const AIM_LOW45: Self = Self(0x40000027);
    /// Index 40, `AimLow60`.
    pub const AIM_LOW60: Self = Self(0x40000028);
    /// Index 41, `AimLow75`.
    pub const AIM_LOW75: Self = Self(0x40000029);
    /// Index 42, `AimLow90`.
    pub const AIM_LOW90: Self = Self(0x4000002A);
    /// Index 43, `MagicBlast`.
    pub const MAGIC_BLAST: Self = Self(0x4000002B);
    /// Index 44, `MagicSelfHead`.
    pub const MAGIC_SELF_HEAD: Self = Self(0x4000002C);
    /// Index 45, `MagicSelfHeart`.
    pub const MAGIC_SELF_HEART: Self = Self(0x4000002D);
    /// Index 46, `MagicBonus`.
    pub const MAGIC_BONUS: Self = Self(0x4000002E);
    /// Index 47, `MagicClap`.
    pub const MAGIC_CLAP: Self = Self(0x4000002F);
    /// Index 48, `MagicHarm`.
    pub const MAGIC_HARM: Self = Self(0x40000030);
    /// Index 49, `MagicHeal`.
    pub const MAGIC_HEAL: Self = Self(0x40000031);
    /// Index 50, `MagicThrowMissile`.
    pub const MAGIC_THROW_MISSILE: Self = Self(0x40000032);
    /// Index 51, `MagicRecoilMissile`.
    pub const MAGIC_RECOIL_MISSILE: Self = Self(0x40000033);
    /// Index 52, `MagicPenalty`.
    pub const MAGIC_PENALTY: Self = Self(0x40000034);
    /// Index 53, `MagicTransfer`.
    pub const MAGIC_TRANSFER: Self = Self(0x40000035);
    /// Index 54, `MagicVision`.
    pub const MAGIC_VISION: Self = Self(0x40000036);
    /// Index 55, `MagicEnchantItem`.
    pub const MAGIC_ENCHANT_ITEM: Self = Self(0x40000037);
    /// Index 56, `MagicPortal`.
    pub const MAGIC_PORTAL: Self = Self(0x40000038);
    /// Index 57, `MagicPray`.
    pub const MAGIC_PRAY: Self = Self(0x40000039);
    /// Index 58, `StopTurning`.
    pub const STOP_TURNING: Self = Self(0x2000003A);
    /// Index 59, `Jump`.
    pub const JUMP: Self = Self(0x2500003B);
    /// Index 60, `HandCombat`.
    pub const HAND_COMBAT: Self = Self(0x8000003C);
    /// Index 61, `NonCombat`.
    pub const NON_COMBAT: Self = Self(0x8000003D);
    /// Index 62, `SwordCombat`.
    pub const SWORD_COMBAT: Self = Self(0x8000003E);
    /// Index 63, `BowCombat`.
    pub const BOW_COMBAT: Self = Self(0x8000003F);
    /// Index 64, `SwordShieldCombat`.
    pub const SWORD_SHIELD_COMBAT: Self = Self(0x80000040);
    /// Index 65, `CrossbowCombat`.
    pub const CROSSBOW_COMBAT: Self = Self(0x80000041);
    /// Index 66, `UnusedCombat`.
    pub const UNUSED_COMBAT: Self = Self(0x80000042);
    /// Index 67, `SlingCombat`.
    pub const SLING_COMBAT: Self = Self(0x80000043);
    /// Index 68, `2HandedSwordCombat`.
    pub const TWO_HANDED_SWORD_COMBAT: Self = Self(0x80000044);
    /// Index 69, `2HandedStaffCombat`.
    pub const TWO_HANDED_STAFF_COMBAT: Self = Self(0x80000045);
    /// Index 70, `DualWieldCombat`.
    pub const DUAL_WIELD_COMBAT: Self = Self(0x80000046);
    /// Index 71, `ThrownWeaponCombat`.
    pub const THROWN_WEAPON_COMBAT: Self = Self(0x80000047);
    /// Index 72, `Graze`.
    pub const GRAZE: Self = Self(0x80000048);
    /// Index 73, `Magic`.
    pub const MAGIC: Self = Self(0x80000049);
    /// Index 74, `Hop`.
    pub const HOP: Self = Self(0x1000004A);
    /// Index 75, `Jumpup`.
    pub const JUMPUP: Self = Self(0x1000004B);
    /// Index 76, `Cheer`.
    pub const CHEER: Self = Self(0x1300004C);
    /// Index 77, `ChestBeat`.
    pub const CHEST_BEAT: Self = Self(0x1000004D);
    /// Index 78, `TippedLeft`.
    pub const TIPPED_LEFT: Self = Self(0x1000004E);
    /// Index 79, `TippedRight`.
    pub const TIPPED_RIGHT: Self = Self(0x1000004F);
    /// Index 80, `FallDown`.
    pub const FALL_DOWN: Self = Self(0x10000050);
    /// Index 81, `Twitch1`.
    pub const TWITCH1: Self = Self(0x10000051);
    /// Index 82, `Twitch2`.
    pub const TWITCH2: Self = Self(0x10000052);
    /// Index 83, `Twitch3`.
    pub const TWITCH3: Self = Self(0x10000053);
    /// Index 84, `Twitch4`.
    pub const TWITCH4: Self = Self(0x10000054);
    /// Index 85, `StaggerBackward`.
    pub const STAGGER_BACKWARD: Self = Self(0x10000055);
    /// Index 86, `StaggerForward`.
    pub const STAGGER_FORWARD: Self = Self(0x10000056);
    /// Index 87, `Sanctuary`.
    pub const SANCTUARY: Self = Self(0x10000057);
    /// Index 88, `ThrustMed`.
    pub const THRUST_MED: Self = Self(0x10000058);
    /// Index 89, `ThrustLow`.
    pub const THRUST_LOW: Self = Self(0x10000059);
    /// Index 90, `ThrustHigh`.
    pub const THRUST_HIGH: Self = Self(0x1000005A);
    /// Index 91, `SlashHigh`.
    pub const SLASH_HIGH: Self = Self(0x1000005B);
    /// Index 92, `SlashMed`.
    pub const SLASH_MED: Self = Self(0x1000005C);
    /// Index 93, `SlashLow`.
    pub const SLASH_LOW: Self = Self(0x1000005D);
    /// Index 94, `BackhandHigh`.
    pub const BACKHAND_HIGH: Self = Self(0x1000005E);
    /// Index 95, `BackhandMed`.
    pub const BACKHAND_MED: Self = Self(0x1000005F);
    /// Index 96, `BackhandLow`.
    pub const BACKHAND_LOW: Self = Self(0x10000060);
    /// Index 97, `Shoot`.
    pub const SHOOT: Self = Self(0x10000061);
    /// Index 98, `AttackHigh1`.
    pub const ATTACK_HIGH1: Self = Self(0x10000062);
    /// Index 99, `AttackMed1`.
    pub const ATTACK_MED1: Self = Self(0x10000063);
    /// Index 100, `AttackLow1`.
    pub const ATTACK_LOW1: Self = Self(0x10000064);
    /// Index 101, `AttackHigh2`.
    pub const ATTACK_HIGH2: Self = Self(0x10000065);
    /// Index 102, `AttackMed2`.
    pub const ATTACK_MED2: Self = Self(0x10000066);
    /// Index 103, `AttackLow2`.
    pub const ATTACK_LOW2: Self = Self(0x10000067);
    /// Index 104, `AttackHigh3`.
    pub const ATTACK_HIGH3: Self = Self(0x10000068);
    /// Index 105, `AttackMed3`.
    pub const ATTACK_MED3: Self = Self(0x10000069);
    /// Index 106, `AttackLow3`.
    pub const ATTACK_LOW3: Self = Self(0x1000006A);
    /// Index 107, `HeadThrow`.
    pub const HEAD_THROW: Self = Self(0x1000006B);
    /// Index 108, `FistSlam`.
    pub const FIST_SLAM: Self = Self(0x1000006C);
    /// Index 109, `BreatheFlame_`.
    pub const BREATHE_FLAME_: Self = Self(0x1000006D);
    /// Index 110, `SpinAttack`.
    pub const SPIN_ATTACK: Self = Self(0x1000006E);
    /// Index 111, `MagicPowerUp01`.
    pub const MAGIC_POWER_UP01: Self = Self(0x1000006F);
    /// Index 112, `MagicPowerUp02`.
    pub const MAGIC_POWER_UP02: Self = Self(0x10000070);
    /// Index 113, `MagicPowerUp03`.
    pub const MAGIC_POWER_UP03: Self = Self(0x10000071);
    /// Index 114, `MagicPowerUp04`.
    pub const MAGIC_POWER_UP04: Self = Self(0x10000072);
    /// Index 115, `MagicPowerUp05`.
    pub const MAGIC_POWER_UP05: Self = Self(0x10000073);
    /// Index 116, `MagicPowerUp06`.
    pub const MAGIC_POWER_UP06: Self = Self(0x10000074);
    /// Index 117, `MagicPowerUp07`.
    pub const MAGIC_POWER_UP07: Self = Self(0x10000075);
    /// Index 118, `MagicPowerUp08`.
    pub const MAGIC_POWER_UP08: Self = Self(0x10000076);
    /// Index 119, `MagicPowerUp09`.
    pub const MAGIC_POWER_UP09: Self = Self(0x10000077);
    /// Index 120, `MagicPowerUp10`.
    pub const MAGIC_POWER_UP10: Self = Self(0x10000078);
    /// Index 121, `ShakeFist`.
    pub const SHAKE_FIST: Self = Self(0x13000079);
    /// Index 122, `Beckon`.
    pub const BECKON: Self = Self(0x1300007A);
    /// Index 123, `BeSeeingYou`.
    pub const BE_SEEING_YOU: Self = Self(0x1300007B);
    /// Index 124, `BlowKiss`.
    pub const BLOW_KISS: Self = Self(0x1300007C);
    /// Index 125, `BowDeep`.
    pub const BOW_DEEP: Self = Self(0x1300007D);
    /// Index 126, `ClapHands`.
    pub const CLAP_HANDS: Self = Self(0x1300007E);
    /// Index 127, `Cry`.
    pub const CRY: Self = Self(0x1300007F);
    /// Index 128, `Laugh`.
    pub const LAUGH: Self = Self(0x13000080);
    /// Index 129, `MimeEat`.
    pub const MIME_EAT: Self = Self(0x13000081);
    /// Index 130, `MimeDrink`.
    pub const MIME_DRINK: Self = Self(0x13000082);
    /// Index 131, `Nod`.
    pub const NOD: Self = Self(0x13000083);
    /// Index 132, `Point`.
    pub const POINT: Self = Self(0x13000084);
    /// Index 133, `ShakeHead`.
    pub const SHAKE_HEAD: Self = Self(0x13000085);
    /// Index 134, `Shrug`.
    pub const SHRUG: Self = Self(0x13000086);
    /// Index 135, `Wave`.
    pub const WAVE: Self = Self(0x13000087);
    /// Index 136, `Akimbo`.
    pub const AKIMBO: Self = Self(0x13000088);
    /// Index 137, `HeartyLaugh`.
    pub const HEARTY_LAUGH: Self = Self(0x13000089);
    /// Index 138, `Salute`.
    pub const SALUTE: Self = Self(0x1300008A);
    /// Index 139, `ScratchHead`.
    pub const SCRATCH_HEAD: Self = Self(0x1300008B);
    /// Index 140, `SmackHead`.
    pub const SMACK_HEAD: Self = Self(0x1300008C);
    /// Index 141, `TapFoot`.
    pub const TAP_FOOT: Self = Self(0x1300008D);
    /// Index 142, `WaveHigh`.
    pub const WAVE_HIGH: Self = Self(0x1300008E);
    /// Index 143, `WaveLow`.
    pub const WAVE_LOW: Self = Self(0x1300008F);
    /// Index 144, `YawnStretch`.
    pub const YAWN_STRETCH: Self = Self(0x13000090);
    /// Index 145, `Cringe`.
    pub const CRINGE: Self = Self(0x13000091);
    /// Index 146, `Kneel`.
    pub const KNEEL: Self = Self(0x13000092);
    /// Index 147, `Plead`.
    pub const PLEAD: Self = Self(0x13000093);
    /// Index 148, `Shiver`.
    pub const SHIVER: Self = Self(0x13000094);
    /// Index 149, `Shoo`.
    pub const SHOO: Self = Self(0x13000095);
    /// Index 150, `Slouch`.
    pub const SLOUCH: Self = Self(0x13000096);
    /// Index 151, `Spit`.
    pub const SPIT: Self = Self(0x13000097);
    /// Index 152, `Surrender`.
    pub const SURRENDER: Self = Self(0x13000098);
    /// Index 153, `Woah`.
    pub const WOAH: Self = Self(0x13000099);
    /// Index 154, `Winded`.
    pub const WINDED: Self = Self(0x1300009A);
    /// Index 155, `YMCA`.
    pub const YMCA: Self = Self(0x1200009B);
    /// Index 156, `EnterGame`.
    pub const ENTER_GAME: Self = Self(0x1000009C);
    /// Index 157, `ExitGame`.
    pub const EXIT_GAME: Self = Self(0x1000009D);
    /// Index 158, `OnCreation`.
    pub const ON_CREATION: Self = Self(0x1000009E);
    /// Index 159, `OnDestruction`.
    pub const ON_DESTRUCTION: Self = Self(0x1000009F);
    /// Index 160, `EnterPortal`.
    pub const ENTER_PORTAL: Self = Self(0x100000A0);
    /// Index 161, `ExitPortal`.
    pub const EXIT_PORTAL: Self = Self(0x100000A1);
    /// Index 162, `Cancel`.
    pub const CANCEL: Self = Self(0x080000A2);
    /// Index 163, `UseSelected`.
    pub const USE_SELECTED: Self = Self(0x090000A3);
    /// Index 164, `AutosortSelected`.
    pub const AUTOSORT_SELECTED: Self = Self(0x090000A4);
    /// Index 165, `DropSelected`.
    pub const DROP_SELECTED: Self = Self(0x090000A5);
    /// Index 166, `GiveSelected`.
    pub const GIVE_SELECTED: Self = Self(0x090000A6);
    /// Index 167, `SplitSelected`.
    pub const SPLIT_SELECTED: Self = Self(0x090000A7);
    /// Index 168, `ExamineSelected`.
    pub const EXAMINE_SELECTED: Self = Self(0x090000A8);
    /// Index 169, `CreateShortcutToSelected`.
    pub const CREATE_SHORTCUT_TO_SELECTED: Self = Self(0x080000A9);
    /// Index 170, `PreviousCompassItem`.
    pub const PREVIOUS_COMPASS_ITEM: Self = Self(0x090000AA);
    /// Index 171, `NextCompassItem`.
    pub const NEXT_COMPASS_ITEM: Self = Self(0x090000AB);
    /// Index 172, `ClosestCompassItem`.
    pub const CLOSEST_COMPASS_ITEM: Self = Self(0x090000AC);
    /// Index 173, `PreviousSelection`.
    pub const PREVIOUS_SELECTION: Self = Self(0x090000AD);
    /// Index 174, `LastAttacker`.
    pub const LAST_ATTACKER: Self = Self(0x090000AE);
    /// Index 175, `PreviousFellow`.
    pub const PREVIOUS_FELLOW: Self = Self(0x090000AF);
    /// Index 176, `NextFellow`.
    pub const NEXT_FELLOW: Self = Self(0x090000B0);
    /// Index 177, `ToggleCombat`.
    pub const TOGGLE_COMBAT: Self = Self(0x090000B1);
    /// Index 178, `HighAttack`.
    pub const HIGH_ATTACK: Self = Self(0x0D0000B2);
    /// Index 179, `MediumAttack`.
    pub const MEDIUM_ATTACK: Self = Self(0x0D0000B3);
    /// Index 180, `LowAttack`.
    pub const LOW_ATTACK: Self = Self(0x0D0000B4);
    /// Index 181, `EnterChat`.
    pub const ENTER_CHAT: Self = Self(0x080000B5);
    /// Index 182, `ToggleChat`.
    pub const TOGGLE_CHAT: Self = Self(0x080000B6);
    /// Index 183, `SavePosition`.
    pub const SAVE_POSITION: Self = Self(0x080000B7);
    /// Index 184, `OptionsPanel`.
    pub const OPTIONS_PANEL: Self = Self(0x090000B8);
    /// Index 185, `ResetView`.
    pub const RESET_VIEW: Self = Self(0x090000B9);
    /// Index 186, `CameraLeftRotate`.
    pub const CAMERA_LEFT_ROTATE: Self = Self(0x0D0000BA);
    /// Index 187, `CameraRightRotate`.
    pub const CAMERA_RIGHT_ROTATE: Self = Self(0x0D0000BB);
    /// Index 188, `CameraRaise`.
    pub const CAMERA_RAISE: Self = Self(0x0D0000BC);
    /// Index 189, `CameraLower`.
    pub const CAMERA_LOWER: Self = Self(0x0D0000BD);
    /// Index 190, `CameraCloser`.
    pub const CAMERA_CLOSER: Self = Self(0x0D0000BE);
    /// Index 191, `CameraFarther`.
    pub const CAMERA_FARTHER: Self = Self(0x0D0000BF);
    /// Index 192, `FloorView`.
    pub const FLOOR_VIEW: Self = Self(0x090000C0);
    /// Index 193, `MouseLook`.
    pub const MOUSE_LOOK: Self = Self(0x0C0000C1);
    /// Index 194, `PreviousItem`.
    pub const PREVIOUS_ITEM: Self = Self(0x090000C2);
    /// Index 195, `NextItem`.
    pub const NEXT_ITEM: Self = Self(0x090000C3);
    /// Index 196, `ClosestItem`.
    pub const CLOSEST_ITEM: Self = Self(0x090000C4);
    /// Index 197, `ShiftView`.
    pub const SHIFT_VIEW: Self = Self(0x0D0000C5);
    /// Index 198, `MapView`.
    pub const MAP_VIEW: Self = Self(0x090000C6);
    /// Index 199, `AutoRun`.
    pub const AUTO_RUN: Self = Self(0x090000C7);
    /// Index 200, `DecreasePowerSetting`.
    pub const DECREASE_POWER_SETTING: Self = Self(0x090000C8);
    /// Index 201, `IncreasePowerSetting`.
    pub const INCREASE_POWER_SETTING: Self = Self(0x090000C9);
    /// Index 202, `Pray`.
    pub const PRAY: Self = Self(0x130000CA);
    /// Index 203, `Mock`.
    pub const MOCK: Self = Self(0x130000CB);
    /// Index 204, `Teapot`.
    pub const TEAPOT: Self = Self(0x130000CC);
    /// Index 205, `SpecialAttack1`.
    pub const SPECIAL_ATTACK1: Self = Self(0x100000CD);
    /// Index 206, `SpecialAttack2`.
    pub const SPECIAL_ATTACK2: Self = Self(0x100000CE);
    /// Index 207, `SpecialAttack3`.
    pub const SPECIAL_ATTACK3: Self = Self(0x100000CF);
    /// Index 208, `MissileAttack1`.
    pub const MISSILE_ATTACK1: Self = Self(0x100000D0);
    /// Index 209, `MissileAttack2`.
    pub const MISSILE_ATTACK2: Self = Self(0x100000D1);
    /// Index 210, `MissileAttack3`.
    pub const MISSILE_ATTACK3: Self = Self(0x100000D2);
    /// Index 211, `CastSpell`.
    pub const CAST_SPELL: Self = Self(0x400000D3);
    /// Index 212, `Flatulence`.
    pub const FLATULENCE: Self = Self(0x120000D4);
    /// Index 213, `FirstPersonView`.
    pub const FIRST_PERSON_VIEW: Self = Self(0x090000D5);
    /// Index 214, `AllegiancePanel`.
    pub const ALLEGIANCE_PANEL: Self = Self(0x090000D6);
    /// Index 215, `FellowshipPanel`.
    pub const FELLOWSHIP_PANEL: Self = Self(0x090000D7);
    /// Index 216, `SpellbookPanel`.
    pub const SPELLBOOK_PANEL: Self = Self(0x090000D8);
    /// Index 217, `SpellComponentsPanel`.
    pub const SPELL_COMPONENTS_PANEL: Self = Self(0x090000D9);
    /// Index 218, `HousePanel`.
    pub const HOUSE_PANEL: Self = Self(0x090000DA);
    /// Index 219, `AttributesPanel`.
    pub const ATTRIBUTES_PANEL: Self = Self(0x090000DB);
    /// Index 220, `SkillsPanel`.
    pub const SKILLS_PANEL: Self = Self(0x090000DC);
    /// Index 221, `MapPanel`.
    pub const MAP_PANEL: Self = Self(0x090000DD);
    /// Index 222, `InventoryPanel`.
    pub const INVENTORY_PANEL: Self = Self(0x090000DE);
    /// Index 223, `Demonet`.
    pub const DEMONET: Self = Self(0x120000DF);
    /// Index 224, `UseMagicStaff`.
    pub const USE_MAGIC_STAFF: Self = Self(0x400000E0);
    /// Index 225, `UseMagicWand`.
    pub const USE_MAGIC_WAND: Self = Self(0x400000E1);
    /// Index 226, `Blink`.
    pub const BLINK: Self = Self(0x100000E2);
    /// Index 227, `Bite`.
    pub const BITE: Self = Self(0x100000E3);
    /// Index 228, `TwitchSubstate1`.
    pub const TWITCH_SUBSTATE1: Self = Self(0x400000E4);
    /// Index 229, `TwitchSubstate2`.
    pub const TWITCH_SUBSTATE2: Self = Self(0x400000E5);
    /// Index 230, `TwitchSubstate3`.
    pub const TWITCH_SUBSTATE3: Self = Self(0x400000E6);
    /// Index 231, `CaptureScreenshotToFile`.
    pub const CAPTURE_SCREENSHOT_TO_FILE: Self = Self(0x090000E7);
    /// Index 232, `BowNoAmmo`.
    pub const BOW_NO_AMMO: Self = Self(0x800000E8);
    /// Index 233, `CrossBowNoAmmo`.
    pub const CROSS_BOW_NO_AMMO: Self = Self(0x800000E9);
    /// Index 234, `ShakeFistState`.
    pub const SHAKE_FIST_STATE: Self = Self(0x430000EA);
    /// Index 235, `PrayState`.
    pub const PRAY_STATE: Self = Self(0x430000EB);
    /// Index 236, `BowDeepState`.
    pub const BOW_DEEP_STATE: Self = Self(0x430000EC);
    /// Index 237, `ClapHandsState`.
    pub const CLAP_HANDS_STATE: Self = Self(0x430000ED);
    /// Index 238, `CrossArmsState`.
    pub const CROSS_ARMS_STATE: Self = Self(0x430000EE);
    /// Index 239, `ShiverState`.
    pub const SHIVER_STATE: Self = Self(0x430000EF);
    /// Index 240, `PointState`.
    pub const POINT_STATE: Self = Self(0x430000F0);
    /// Index 241, `WaveState`.
    pub const WAVE_STATE: Self = Self(0x430000F1);
    /// Index 242, `AkimboState`.
    pub const AKIMBO_STATE: Self = Self(0x430000F2);
    /// Index 243, `SaluteState`.
    pub const SALUTE_STATE: Self = Self(0x430000F3);
    /// Index 244, `ScratchHeadState`.
    pub const SCRATCH_HEAD_STATE: Self = Self(0x430000F4);
    /// Index 245, `TapFootState`.
    pub const TAP_FOOT_STATE: Self = Self(0x430000F5);
    /// Index 246, `LeanState`.
    pub const LEAN_STATE: Self = Self(0x430000F6);
    /// Index 247, `KneelState`.
    pub const KNEEL_STATE: Self = Self(0x430000F7);
    /// Index 248, `PleadState`.
    pub const PLEAD_STATE: Self = Self(0x430000F8);
    /// Index 249, `ATOYOT`.
    pub const ATOYOT: Self = Self(0x420000F9);
    /// Index 250, `SlouchState`.
    pub const SLOUCH_STATE: Self = Self(0x430000FA);
    /// Index 251, `SurrenderState`.
    pub const SURRENDER_STATE: Self = Self(0x430000FB);
    /// Index 252, `WoahState`.
    pub const WOAH_STATE: Self = Self(0x430000FC);
    /// Index 253, `WindedState`.
    pub const WINDED_STATE: Self = Self(0x430000FD);
    /// Index 254, `AutoCreateShortcuts`.
    pub const AUTO_CREATE_SHORTCUTS: Self = Self(0x090000FE);
    /// Index 255, `AutoRepeatAttacks`.
    pub const AUTO_REPEAT_ATTACKS: Self = Self(0x090000FF);
    /// Index 256, `AutoTarget`.
    pub const AUTO_TARGET: Self = Self(0x09000100);
    /// Index 257, `AdvancedCombatInterface`.
    pub const ADVANCED_COMBAT_INTERFACE: Self = Self(0x09000101);
    /// Index 258, `IgnoreAllegianceRequests`.
    pub const IGNORE_ALLEGIANCE_REQUESTS: Self = Self(0x09000102);
    /// Index 259, `IgnoreFellowshipRequests`.
    pub const IGNORE_FELLOWSHIP_REQUESTS: Self = Self(0x09000103);
    /// Index 260, `InvertMouseLook`.
    pub const INVERT_MOUSE_LOOK: Self = Self(0x09000104);
    /// Index 261, `LetPlayersGiveYouItems`.
    pub const LET_PLAYERS_GIVE_YOU_ITEMS: Self = Self(0x09000105);
    /// Index 262, `AutoTrackCombatTargets`.
    pub const AUTO_TRACK_COMBAT_TARGETS: Self = Self(0x09000106);
    /// Index 263, `DisplayTooltips`.
    pub const DISPLAY_TOOLTIPS: Self = Self(0x09000107);
    /// Index 264, `AttemptToDeceivePlayers`.
    pub const ATTEMPT_TO_DECEIVE_PLAYERS: Self = Self(0x09000108);
    /// Index 265, `RunAsDefaultMovement`.
    pub const RUN_AS_DEFAULT_MOVEMENT: Self = Self(0x09000109);
    /// Index 266, `StayInChatModeAfterSend`.
    pub const STAY_IN_CHAT_MODE_AFTER_SEND: Self = Self(0x0900010A);
    /// Index 267, `RightClickToMouseLook`.
    pub const RIGHT_CLICK_TO_MOUSE_LOOK: Self = Self(0x0900010B);
    /// Index 268, `VividTargetIndicator`.
    pub const VIVID_TARGET_INDICATOR: Self = Self(0x0900010C);
    /// Index 269, `SelectSelf`.
    pub const SELECT_SELF: Self = Self(0x0900010D);
    /// Index 270, `SkillHealSelf`.
    pub const SKILL_HEAL_SELF: Self = Self(0x1000010E);
    /// Index 271, `SkillHealOther`.
    pub const SKILL_HEAL_OTHER: Self = Self(0x1000010F);
    /// Index 272, `CombatEat`.
    pub const COMBAT_EAT: Self = Self(0x10000110);
    /// Index 273, `CombatDrink`.
    pub const COMBAT_DRINK: Self = Self(0x10000111);
    /// Index 274, `NextMonster`.
    pub const NEXT_MONSTER: Self = Self(0x09000112);
    /// Index 275, `PreviousMonster`.
    pub const PREVIOUS_MONSTER: Self = Self(0x09000113);
    /// Index 276, `ClosestMonster`.
    pub const CLOSEST_MONSTER: Self = Self(0x09000114);
    /// Index 277, `NextPlayer`.
    pub const NEXT_PLAYER: Self = Self(0x09000115);
    /// Index 278, `PreviousPlayer`.
    pub const PREVIOUS_PLAYER: Self = Self(0x09000116);
    /// Index 279, `ClosestPlayer`.
    pub const CLOSEST_PLAYER: Self = Self(0x09000117);
    /// Index 280, `SnowAngelState`.
    pub const SNOW_ANGEL_STATE: Self = Self(0x43000118);
    /// Index 281, `WarmHands`.
    pub const WARM_HANDS: Self = Self(0x13000119);
    /// Index 282, `CurtseyState`.
    pub const CURTSEY_STATE: Self = Self(0x4300011A);
    /// Index 283, `AFKState`.
    pub const AFK_STATE: Self = Self(0x4300011B);
    /// Index 284, `MeditateState`.
    pub const MEDITATE_STATE: Self = Self(0x4300011C);
    /// Index 285, `TradePanel`.
    pub const TRADE_PANEL: Self = Self(0x0900011D);
    /// Index 286, `LogOut`.
    pub const LOG_OUT: Self = Self(0x1000011E);
    /// Index 287, `DoubleSlashLow`.
    pub const DOUBLE_SLASH_LOW: Self = Self(0x1000011F);
    /// Index 288, `DoubleSlashMed`.
    pub const DOUBLE_SLASH_MED: Self = Self(0x10000120);
    /// Index 289, `DoubleSlashHigh`.
    pub const DOUBLE_SLASH_HIGH: Self = Self(0x10000121);
    /// Index 290, `TripleSlashLow`.
    pub const TRIPLE_SLASH_LOW: Self = Self(0x10000122);
    /// Index 291, `TripleSlashMed`.
    pub const TRIPLE_SLASH_MED: Self = Self(0x10000123);
    /// Index 292, `TripleSlashHigh`.
    pub const TRIPLE_SLASH_HIGH: Self = Self(0x10000124);
    /// Index 293, `DoubleThrustLow`.
    pub const DOUBLE_THRUST_LOW: Self = Self(0x10000125);
    /// Index 294, `DoubleThrustMed`.
    pub const DOUBLE_THRUST_MED: Self = Self(0x10000126);
    /// Index 295, `DoubleThrustHigh`.
    pub const DOUBLE_THRUST_HIGH: Self = Self(0x10000127);
    /// Index 296, `TripleThrustLow`.
    pub const TRIPLE_THRUST_LOW: Self = Self(0x10000128);
    /// Index 297, `TripleThrustMed`.
    pub const TRIPLE_THRUST_MED: Self = Self(0x10000129);
    /// Index 298, `TripleThrustHigh`.
    pub const TRIPLE_THRUST_HIGH: Self = Self(0x1000012A);
    /// Index 299, `MagicPowerUp01Purple`.
    pub const MAGIC_POWER_UP01_PURPLE: Self = Self(0x1000012B);
    /// Index 300, `MagicPowerUp02Purple`.
    pub const MAGIC_POWER_UP02_PURPLE: Self = Self(0x1000012C);
    /// Index 301, `MagicPowerUp03Purple`.
    pub const MAGIC_POWER_UP03_PURPLE: Self = Self(0x1000012D);
    /// Index 302, `MagicPowerUp04Purple`.
    pub const MAGIC_POWER_UP04_PURPLE: Self = Self(0x1000012E);
    /// Index 303, `MagicPowerUp05Purple`.
    pub const MAGIC_POWER_UP05_PURPLE: Self = Self(0x1000012F);
    /// Index 304, `MagicPowerUp06Purple`.
    pub const MAGIC_POWER_UP06_PURPLE: Self = Self(0x10000130);
    /// Index 305, `MagicPowerUp07Purple`.
    pub const MAGIC_POWER_UP07_PURPLE: Self = Self(0x10000131);
    /// Index 306, `MagicPowerUp08Purple`.
    pub const MAGIC_POWER_UP08_PURPLE: Self = Self(0x10000132);
    /// Index 307, `MagicPowerUp09Purple`.
    pub const MAGIC_POWER_UP09_PURPLE: Self = Self(0x10000133);
    /// Index 308, `MagicPowerUp10Purple`.
    pub const MAGIC_POWER_UP10_PURPLE: Self = Self(0x10000134);
    /// Index 309, `Helper`.
    pub const HELPER: Self = Self(0x13000135);
    /// Index 310, `Pickup5`.
    pub const PICKUP5: Self = Self(0x40000136);
    /// Index 311, `Pickup10`.
    pub const PICKUP10: Self = Self(0x40000137);
    /// Index 312, `Pickup15`.
    pub const PICKUP15: Self = Self(0x40000138);
    /// Index 313, `Pickup20`.
    pub const PICKUP20: Self = Self(0x40000139);
    /// Index 314, `HouseRecall`.
    pub const HOUSE_RECALL: Self = Self(0x1000013A);
    /// Index 315, `AtlatlCombat`.
    pub const ATLATL_COMBAT: Self = Self(0x8000013B);
    /// Index 316, `ThrownShieldCombat`.
    pub const THROWN_SHIELD_COMBAT: Self = Self(0x8000013C);
    /// Index 317, `SitState`.
    pub const SIT_STATE: Self = Self(0x4300013D);
    /// Index 318, `SitCrossleggedState`.
    pub const SIT_CROSSLEGGED_STATE: Self = Self(0x4300013E);
    /// Index 319, `SitBackState`.
    pub const SIT_BACK_STATE: Self = Self(0x4300013F);
    /// Index 320, `PointLeftState`.
    pub const POINT_LEFT_STATE: Self = Self(0x43000140);
    /// Index 321, `PointRightState`.
    pub const POINT_RIGHT_STATE: Self = Self(0x43000141);
    /// Index 322, `TalktotheHandState`.
    pub const TALKTOTHE_HAND_STATE: Self = Self(0x43000142);
    /// Index 323, `PointDownState`.
    pub const POINT_DOWN_STATE: Self = Self(0x43000143);
    /// Index 324, `DrudgeDanceState`.
    pub const DRUDGE_DANCE_STATE: Self = Self(0x43000144);
    /// Index 325, `PossumState`.
    pub const POSSUM_STATE: Self = Self(0x43000145);
    /// Index 326, `ReadState`.
    pub const READ_STATE: Self = Self(0x43000146);
    /// Index 327, `ThinkerState`.
    pub const THINKER_STATE: Self = Self(0x43000147);
    /// Index 328, `HaveASeatState`.
    pub const HAVE_A_SEAT_STATE: Self = Self(0x43000148);
    /// Index 329, `AtEaseState`.
    pub const AT_EASE_STATE: Self = Self(0x43000149);
    /// Index 330, `NudgeLeft`.
    pub const NUDGE_LEFT: Self = Self(0x1300014A);
    /// Index 331, `NudgeRight`.
    pub const NUDGE_RIGHT: Self = Self(0x1300014B);
    /// Index 332, `PointLeft`.
    pub const POINT_LEFT: Self = Self(0x1300014C);
    /// Index 333, `PointRight`.
    pub const POINT_RIGHT: Self = Self(0x1300014D);
    /// Index 334, `PointDown`.
    pub const POINT_DOWN: Self = Self(0x1300014E);
    /// Index 335, `Knock`.
    pub const KNOCK: Self = Self(0x1300014F);
    /// Index 336, `ScanHorizon`.
    pub const SCAN_HORIZON: Self = Self(0x13000150);
    /// Index 337, `DrudgeDance`.
    pub const DRUDGE_DANCE: Self = Self(0x13000151);
    /// Index 338, `HaveASeat`.
    pub const HAVE_A_SEAT: Self = Self(0x13000152);
    /// Index 339, `LifestoneRecall`.
    pub const LIFESTONE_RECALL: Self = Self(0x10000153);
    /// Index 340, `CharacterOptionsPanel`.
    pub const CHARACTER_OPTIONS_PANEL: Self = Self(0x09000154);
    /// Index 341, `SoundAndGraphicsPanel`.
    pub const SOUND_AND_GRAPHICS_PANEL: Self = Self(0x09000155);
    /// Index 342, `HelpfulSpellsPanel`.
    pub const HELPFUL_SPELLS_PANEL: Self = Self(0x09000156);
    /// Index 343, `HarmfulSpellsPanel`.
    pub const HARMFUL_SPELLS_PANEL: Self = Self(0x09000157);
    /// Index 344, `CharacterInformationPanel`.
    pub const CHARACTER_INFORMATION_PANEL: Self = Self(0x09000158);
    /// Index 345, `LinkStatusPanel`.
    pub const LINK_STATUS_PANEL: Self = Self(0x09000159);
    /// Index 346, `VitaePanel`.
    pub const VITAE_PANEL: Self = Self(0x0900015A);
    /// Index 347, `ShareFellowshipXP`.
    pub const SHARE_FELLOWSHIP_XP: Self = Self(0x0900015B);
    /// Index 348, `ShareFellowshipLoot`.
    pub const SHARE_FELLOWSHIP_LOOT: Self = Self(0x0900015C);
    /// Index 349, `AcceptCorpseLooting`.
    pub const ACCEPT_CORPSE_LOOTING: Self = Self(0x0900015D);
    /// Index 350, `IgnoreTradeRequests`.
    pub const IGNORE_TRADE_REQUESTS: Self = Self(0x0900015E);
    /// Index 351, `DisableWeather`.
    pub const DISABLE_WEATHER: Self = Self(0x0900015F);
    /// Index 352, `DisableHouseEffect`.
    pub const DISABLE_HOUSE_EFFECT: Self = Self(0x09000160);
    /// Index 353, `StretchUI`.
    pub const STRETCH_UI: Self = Self(0x09000161);
    /// Index 354, `ShowRadarCoordinates`.
    pub const SHOW_RADAR_COORDINATES: Self = Self(0x09000162);
    /// Index 355, `ShowSpellDurations`.
    pub const SHOW_SPELL_DURATIONS: Self = Self(0x09000163);
    /// Index 356, `MuteOnLosingFocus`.
    pub const MUTE_ON_LOSING_FOCUS: Self = Self(0x09000164);
    /// Index 357, `Fishing`.
    pub const FISHING: Self = Self(0x10000165);
    /// Index 358, `MarketplaceRecall`.
    pub const MARKETPLACE_RECALL: Self = Self(0x10000166);
    /// Index 359, `EnterPKLite`.
    pub const ENTER_PK_LITE: Self = Self(0x10000167);
    /// Index 360, `AllegianceChat`.
    pub const ALLEGIANCE_CHAT: Self = Self(0x09000168);
    /// Index 361, `AutomaticallyAcceptFellowshipRequests`.
    pub const AUTOMATICALLY_ACCEPT_FELLOWSHIP_REQUESTS: Self = Self(0x09000169);
    /// Index 362, `Reply`.
    pub const REPLY: Self = Self(0x0900016A);
    /// Index 363, `MonarchReply`.
    pub const MONARCH_REPLY: Self = Self(0x0900016B);
    /// Index 364, `PatronReply`.
    pub const PATRON_REPLY: Self = Self(0x0900016C);
    /// Index 365, `ToggleCraftingChanceOfSuccessDialog`.
    pub const TOGGLE_CRAFTING_CHANCE_OF_SUCCESS_DIALOG: Self = Self(0x0900016D);
    /// Index 366, `UseClosestUnopenedCorpse`.
    pub const USE_CLOSEST_UNOPENED_CORPSE: Self = Self(0x0900016E);
    /// Index 367, `UseNextUnopenedCorpse`.
    pub const USE_NEXT_UNOPENED_CORPSE: Self = Self(0x0900016F);
    /// Index 368, `IssueSlashCommand`.
    pub const ISSUE_SLASH_COMMAND: Self = Self(0x09000170);
    /// Index 369, `AllegianceHometownRecall`.
    pub const ALLEGIANCE_HOMETOWN_RECALL: Self = Self(0x10000171);
    /// Index 370, `PKArenaRecall`.
    pub const PK_ARENA_RECALL: Self = Self(0x10000172);
    /// Index 371, `OffhandSlashHigh`.
    pub const OFFHAND_SLASH_HIGH: Self = Self(0x10000173);
    /// Index 372, `OffhandSlashMed`.
    pub const OFFHAND_SLASH_MED: Self = Self(0x10000174);
    /// Index 373, `OffhandSlashLow`.
    pub const OFFHAND_SLASH_LOW: Self = Self(0x10000175);
    /// Index 374, `OffhandThrustHigh`.
    pub const OFFHAND_THRUST_HIGH: Self = Self(0x10000176);
    /// Index 375, `OffhandThrustMed`.
    pub const OFFHAND_THRUST_MED: Self = Self(0x10000177);
    /// Index 376, `OffhandThrustLow`.
    pub const OFFHAND_THRUST_LOW: Self = Self(0x10000178);
    /// Index 377, `OffhandDoubleSlashLow`.
    pub const OFFHAND_DOUBLE_SLASH_LOW: Self = Self(0x10000179);
    /// Index 378, `OffhandDoubleSlashMed`.
    pub const OFFHAND_DOUBLE_SLASH_MED: Self = Self(0x1000017A);
    /// Index 379, `OffhandDoubleSlashHigh`.
    pub const OFFHAND_DOUBLE_SLASH_HIGH: Self = Self(0x1000017B);
    /// Index 380, `OffhandTripleSlashLow`.
    pub const OFFHAND_TRIPLE_SLASH_LOW: Self = Self(0x1000017C);
    /// Index 381, `OffhandTripleSlashMed`.
    pub const OFFHAND_TRIPLE_SLASH_MED: Self = Self(0x1000017D);
    /// Index 382, `OffhandTripleSlashHigh`.
    pub const OFFHAND_TRIPLE_SLASH_HIGH: Self = Self(0x1000017E);
    /// Index 383, `OffhandDoubleThrustLow`.
    pub const OFFHAND_DOUBLE_THRUST_LOW: Self = Self(0x1000017F);
    /// Index 384, `OffhandDoubleThrustMed`.
    pub const OFFHAND_DOUBLE_THRUST_MED: Self = Self(0x10000180);
    /// Index 385, `OffhandDoubleThrustHigh`.
    pub const OFFHAND_DOUBLE_THRUST_HIGH: Self = Self(0x10000181);
    /// Index 386, `OffhandTripleThrustLow`.
    pub const OFFHAND_TRIPLE_THRUST_LOW: Self = Self(0x10000182);
    /// Index 387, `OffhandTripleThrustMed`.
    pub const OFFHAND_TRIPLE_THRUST_MED: Self = Self(0x10000183);
    /// Index 388, `OffhandTripleThrustHigh`.
    pub const OFFHAND_TRIPLE_THRUST_HIGH: Self = Self(0x10000184);
    /// Index 389, `OffhandKick`.
    pub const OFFHAND_KICK: Self = Self(0x10000185);
    /// Index 390, `AttackHigh4`.
    pub const ATTACK_HIGH4: Self = Self(0x10000186);
    /// Index 391, `AttackMed4`.
    pub const ATTACK_MED4: Self = Self(0x10000187);
    /// Index 392, `AttackLow4`.
    pub const ATTACK_LOW4: Self = Self(0x10000188);
    /// Index 393, `AttackHigh5`.
    pub const ATTACK_HIGH5: Self = Self(0x10000189);
    /// Index 394, `AttackMed5`.
    pub const ATTACK_MED5: Self = Self(0x1000018A);
    /// Index 395, `AttackLow5`.
    pub const ATTACK_LOW5: Self = Self(0x1000018B);
    /// Index 396, `AttackHigh6`.
    pub const ATTACK_HIGH6: Self = Self(0x1000018C);
    /// Index 397, `AttackMed6`.
    pub const ATTACK_MED6: Self = Self(0x1000018D);
    /// Index 398, `AttackLow6`.
    pub const ATTACK_LOW6: Self = Self(0x1000018E);
    /// Index 399, `PunchFastHigh`.
    pub const PUNCH_FAST_HIGH: Self = Self(0x1000018F);
    /// Index 400, `PunchFastMed`.
    pub const PUNCH_FAST_MED: Self = Self(0x10000190);
    /// Index 401, `PunchFastLow`.
    pub const PUNCH_FAST_LOW: Self = Self(0x10000191);
    /// Index 402, `PunchSlowHigh`.
    pub const PUNCH_SLOW_HIGH: Self = Self(0x10000192);
    /// Index 403, `PunchSlowMed`.
    pub const PUNCH_SLOW_MED: Self = Self(0x10000193);
    /// Index 404, `PunchSlowLow`.
    pub const PUNCH_SLOW_LOW: Self = Self(0x10000194);
    /// Index 405, `OffhandPunchFastHigh`.
    pub const OFFHAND_PUNCH_FAST_HIGH: Self = Self(0x10000195);
    /// Index 406, `OffhandPunchFastMed`.
    pub const OFFHAND_PUNCH_FAST_MED: Self = Self(0x10000196);
    /// Index 407, `OffhandPunchFastLow`.
    pub const OFFHAND_PUNCH_FAST_LOW: Self = Self(0x10000197);
    /// Index 408, `OffhandPunchSlowHigh`.
    pub const OFFHAND_PUNCH_SLOW_HIGH: Self = Self(0x10000198);
    /// Index 409, `OffhandPunchSlowMed`.
    pub const OFFHAND_PUNCH_SLOW_MED: Self = Self(0x10000199);
    /// Index 410, `OffhandPunchSlowLow`.
    pub const OFFHAND_PUNCH_SLOW_LOW: Self = Self(0x1000019A);
    /// Index 411, `AI_TelegraphCast`.
    pub const AI_TELEGRAPH_CAST: Self = Self(0x1000019B);

    /// The wire form: the index into the 412-entry table. `None` for an id that is not in it.
    ///
    /// The exact inverse of [`Self::from_index`].
    #[must_use]
    pub fn to_index(self) -> Option<u16> {
        // A linear scan, like the client's name lookup; 412 entries and this is not a hot path.
        // Deliberately not `self.0 as u16`: see the module note.
        let i = COMMAND_IDS.iter().position(|&id| id == self.0)?;
        u16::try_from(i).ok()
    }

    /// `command_ids[index]`, the client's own unpack step. `None` when the index is out of range.
    #[must_use]
    pub fn from_index(i: u16) -> Option<Self> {
        COMMAND_IDS.get(usize::from(i)).copied().map(Self)
    }

    /// The command a **2013 client** meant by wire index `i`, as this table numbers it. `None`
    /// past the end of the 2013 table.
    ///
    /// Only for reading the recorded sessions (see [`RETAIL_2013_INDEX`]); nothing the rebuild
    /// sends or receives may use it.
    #[must_use]
    pub fn from_retail_2013_index(i: u16) -> Option<Self> {
        Self::from_index(retail_2013_index_to_current(i)?)
    }

    /// Style / stance. treats `(int)cmd < 0` as a style.
    #[must_use]
    pub const fn is_style(self) -> bool {
        self.0 & 0x8000_0000 != 0
    }

    /// Substate / cycle - a looping motion.
    #[must_use]
    pub const fn is_substate(self) -> bool {
        self.0 & 0x4000_0000 != 0
    }

    /// Modifier - combines with the current cycle rather than replacing it. The one bit the
    /// client's own code confirms directly.
    #[must_use]
    pub const fn is_modifier(self) -> bool {
        self.0 & 0x2000_0000 != 0
    }

    /// Action - one-shot, queued in the action list, capped at six outstanding.
    #[must_use]
    pub const fn is_action(self) -> bool {
        self.0 & 0x1000_0000 != 0
    }

    /// UI / client-only command. rejects these outright.
    #[must_use]
    pub const fn is_ui(self) -> bool {
        self.0 & 0x0800_0000 != 0
    }

    /// Has an associated speed / is a movement key. selects the substate
    /// list with `0x40000000 | 0x04000000`.
    #[must_use]
    pub const fn has_speed(self) -> bool {
        self.0 & 0x0400_0000 != 0
    }

    /// Emote. rejects these outside `NonCombat` with `0x42`.
    #[must_use]
    pub const fn is_emote(self) -> bool {
        self.0 & 0x0200_0000 != 0
    }

    /// The secondary flag on `Ready`/`Crouch`/`Sitting`/`Sleeping` and the walk/run/turn/sidestep
    /// commands.
    #[must_use]
    pub const fn is_persistent(self) -> bool {
        self.0 & 0x0100_0000 != 0
    }

    /// The 24-bit ordinal, which is what the combined motion-table key carries.
    #[must_use]
    pub const fn ordinal(self) -> u32 {
        self.0 & 0x00FF_FFFF
    }

    /// The command whose name matches `s`: a case-insensitive linear scan of the client's name
    /// table, as the client does it.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        COMMAND_NAMES
            .iter()
            .position(|n| n.eq_ignore_ascii_case(s))
            .map(|i| Self(COMMAND_IDS[i]))
    }

    /// The client's own string for this id, or `None` if the id is not in the table.
    #[must_use]
    pub fn name(self) -> Option<&'static str> {
        COMMAND_IDS
            .iter()
            .position(|&id| id == self.0)
            .map(|i| COMMAND_NAMES[i])
    }
}

/// The stance set - `MotionStance` in ACE's naming: every `0x8xxxxxxx` entry of the table.
///
/// `NonCombat` is the client's default style: both motion-state constructors start there and both
/// fall back to it when a style is removed.
pub const STANCES: [MotionCommand; 19] = [
    MotionCommand::INVALID,
    MotionCommand::HAND_COMBAT,
    MotionCommand::NON_COMBAT,
    MotionCommand::SWORD_COMBAT,
    MotionCommand::BOW_COMBAT,
    MotionCommand::SWORD_SHIELD_COMBAT,
    MotionCommand::CROSSBOW_COMBAT,
    MotionCommand::UNUSED_COMBAT,
    MotionCommand::SLING_COMBAT,
    MotionCommand::TWO_HANDED_SWORD_COMBAT,
    MotionCommand::TWO_HANDED_STAFF_COMBAT,
    MotionCommand::DUAL_WIELD_COMBAT,
    MotionCommand::THROWN_WEAPON_COMBAT,
    MotionCommand::GRAZE,
    MotionCommand::MAGIC,
    MotionCommand::BOW_NO_AMMO,
    MotionCommand::CROSS_BOW_NO_AMMO,
    MotionCommand::ATLATL_COMBAT,
    MotionCommand::THROWN_SHIELD_COMBAT,
];

/// Every (index, id, name) triple, in table order. Public so a test - or the world-data packer -
/// can walk the whole table rather than guessing at it.
pub fn all() -> impl Iterator<Item = (u16, MotionCommand, &'static str)> {
    (0..COMMAND_IDS.len()).map(|i| {
        (
            u16::try_from(i).unwrap_or(u16::MAX),
            MotionCommand(COMMAND_IDS[i]),
            COMMAND_NAMES[i],
        )
    })
}

// -------------------------------------------------------------------------------------------
// The 2013 client's numbering, for the recordings only.
// -------------------------------------------------------------------------------------------

/// **How the 2013 client's command indices map onto this table.** The recorded sessions under
/// `fixtures/` are the September 2013 client talking to a server, so a command index that client
/// wrote means what the 2013 table said, not what this one says.
///
/// The two tables are identical below `0x10F`. From `0x10F` on, every 2013 command sits exactly
/// three indices higher here, with the same name and the same class byte, except the one the
/// final client dropped: 2013 index `0x15E` (`SideBySideVitals`, `0x0900015E`) has no entry here.
/// The 2013 table ends at index 407.
///
/// Rows are `(first 2013 index, last 2013 index, shift)`. Used by
/// [`retail_2013_index_to_current`] and nothing else. `\[verified\]` against both clients' tables,
/// entry by entry.
pub const RETAIL_2013_INDEX: [(u16, u16, u16); 3] =
    [(0x000, 0x10E, 0), (0x10F, 0x15D, 3), (0x15F, 0x197, 3)];

/// The 2013 client's `SideBySideVitals`, a UI command the final client no longer has.
pub const RETAIL_2013_SIDE_BY_SIDE_VITALS: u32 = 0x0900_015E;

/// A 2013-client command index as this table's index; `None` for the dropped row and past the
/// end of the 2013 table. See [`RETAIL_2013_INDEX`].
#[must_use]
pub fn retail_2013_index_to_current(i: u16) -> Option<u16> {
    RETAIL_2013_INDEX
        .iter()
        .find(|(lo, hi, _)| (*lo..=*hi).contains(&i))
        .map(|(_, _, d)| i + d)
}

// -------------------------------------------------------------------------------------------
// The numbering a world's data files use.
// -------------------------------------------------------------------------------------------

/// **How a set of data files numbers its motion commands**, which is how the client of their day
/// numbered them on the wire too.
///
/// The command table grew over the game's life. Until 2013 it only ever grew at its end, with
/// one exception, so a motion table from 1999 reads correctly under the 2013 table; the final
/// client then inserted three rows at `0x10F` and moved everything above them. The end-of-retail
/// files are in the final numbering; every older set of files is in one of the two earlier ones:
///
/// | numbering | the files it reads |
/// |---|---|
/// | [`Self::Final`] | the final client's, June 2015 on (and every file that keeps no command above `0x10E`) |
/// | [`Self::Before2015`] | October 1999 to December 2013, the files from before Throne of Destiny included |
/// | [`Self::January2002`] | the client of January 2002, whose emote block was reordered by August 2002 |
///
/// The rebuild keeps one numbering inside, [`Self::Final`], which is [`MotionCommand`]'s.
/// Files and wire words in an older numbering are translated at the edge, through
/// [`Self::to_final`] and [`Self::command_from_wire`], and back through [`Self::from_final`] and
/// [`Self::command_to_wire`].
///
/// A command keeps its class byte across numberings (only its index moves), so an older
/// numbering's ids are the final table's class bytes over the older index. The commands the
/// final table has no row for are listed per numbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CommandNumbering {
    /// The final client's 412 commands: the files from June 2015 on.
    #[default]
    Final,
    /// The 408 commands of the clients up to September 2013, of which every client from
    /// October 1999 on (but those of [`Self::January2002`]) has a prefix. Identity below `0x10F`,
    /// three lower from there; the one row the final client dropped (`SideBySideVitals`, a
    /// screen command no motion table names) has no counterpart.
    Before2015,
    /// The 352 commands of the January 2002 client. Below `0x140` it is
    /// [`Self::Before2015`]; above, the emote block was later reordered, and seventeen commands it
    /// has were removed (two of them, `PointUpState` and `PointUp`, are played by that winter's
    /// human motion table). A numbering of its own because the files of that winter are keyed
    /// by it.
    January2002,
}

/// `(first index, last index, shift to the final index)` per numbering. Indices outside every
/// row have no final counterpart.
const FINAL_ROWS: [(u16, u16, i16); 1] = [(0x000, 0x19B, 0)];
const BEFORE_2015_ROWS: [(u16, u16, i16); 3] =
    [(0x000, 0x10E, 0), (0x10F, 0x15D, 3), (0x15F, 0x197, 3)];
const JANUARY_2002_ROWS: [(u16, u16, i16); 9] = [
    (0x000, 0x0D9, 0),
    (0x0DB, 0x10E, 0),
    (0x10F, 0x13F, 3),
    (0x141, 0x145, 2),
    (0x148, 0x148, 0),
    (0x14C, 0x14C, -3),
    (0x14E, 0x151, -4),
    (0x153, 0x154, -5),
    (0x15A, 0x15C, -10),
];

/// The ids an older numbering has that the final table has no row for, by index.
const BEFORE_2015_ONLY: [(u16, u32); 1] = [(0x15E, RETAIL_2013_SIDE_BY_SIDE_VITALS)];
const JANUARY_2002_ONLY: [(u16, u32); 17] = [
    (0x0DA, 0x0900_00DA), // SpellResearchPanel
    (0x140, 0x4300_0140), // PointUpState
    (0x146, 0x4300_0146), // HeadStandState
    (0x147, 0x4300_0147), // DizzyState
    (0x149, 0x4300_0149), // PatBellyState
    (0x14A, 0x4300_014A), // JumpingJackState
    (0x14B, 0x4300_014B), // MimeState
    (0x14D, 0x4300_014D), // ScanHorizonState
    (0x152, 0x1300_0152), // PointUp
    (0x155, 0x1300_0155), // ComeOn
    (0x156, 0x1300_0156), // Lost
    (0x157, 0x1300_0157), // WhyIOughta
    (0x158, 0x1300_0158), // Giggle
    (0x159, 0x1300_0159), // Puzzlement
    (0x15D, 0x1300_015D), // Sit
    (0x15E, 0x1300_015E), // SitCrossLegged
    (0x15F, 0x1300_015F), // SitBack
];

impl CommandNumbering {
    /// Every numbering, in the order [`Self::infer`] tries them.
    pub const ALL: [Self; 3] = [Self::Final, Self::Before2015, Self::January2002];

    fn rows(self) -> &'static [(u16, u16, i16)] {
        match self {
            Self::Final => &FINAL_ROWS,
            Self::Before2015 => &BEFORE_2015_ROWS,
            Self::January2002 => &JANUARY_2002_ROWS,
        }
    }

    fn only(self) -> &'static [(u16, u32)] {
        match self {
            Self::Final => &[],
            Self::Before2015 => &BEFORE_2015_ONLY,
            Self::January2002 => &JANUARY_2002_ONLY,
        }
    }

    /// How many commands the numbering's table has.
    #[must_use]
    pub const fn command_count(self) -> u16 {
        match self {
            Self::Final => 412,
            Self::Before2015 => 408,
            Self::January2002 => 352,
        }
    }

    /// The final table's index of this numbering's index `i`; `None` for a command the final
    /// table has no row for, and past the end.
    #[must_use]
    pub fn to_final_index(self, i: u16) -> Option<u16> {
        let (_, _, d) = self
            .rows()
            .iter()
            .find(|(lo, hi, _)| (*lo..=*hi).contains(&i))?;
        i.checked_add_signed(*d)
    }

    /// This numbering's index of the final table's index `j`; `None` for a command this
    /// numbering does not have.
    #[must_use]
    pub fn from_final_index(self, j: u16) -> Option<u16> {
        self.rows().iter().find_map(|(lo, hi, d)| {
            let i = j.checked_add_signed(-*d)?;
            (*lo..=*hi).contains(&i).then_some(i)
        })
    }

    /// This numbering's id at index `i`, or `None` past its end.
    #[must_use]
    pub fn id_at(self, i: u16) -> Option<u32> {
        if let Some(j) = self.to_final_index(i) {
            return Some(COMMAND_IDS[usize::from(j)] & 0xFFFF_0000 | u32::from(i));
        }
        self.only().iter().find(|(k, _)| *k == i).map(|(_, id)| *id)
    }

    /// Whether `id` is one of this numbering's commands, class byte and all.
    #[must_use]
    pub fn holds(self, id: u32) -> bool {
        u16::try_from(id & 0x00FF_FFFF)
            .ok()
            .and_then(|i| self.id_at(i))
            == Some(id)
    }

    /// The final command an id of this numbering names; `None` when the id is not this
    /// numbering's or the final table has no row for it.
    #[must_use]
    pub fn to_final(self, id: u32) -> Option<MotionCommand> {
        if !self.holds(id) {
            return None;
        }
        let j = self.to_final_index(u16::try_from(id & 0xFFFF).ok()?)?;
        MotionCommand::from_index(j)
    }

    /// The id this numbering gives a final command; `None` when it does not have it.
    #[must_use]
    pub fn from_final(self, c: MotionCommand) -> Option<u32> {
        self.id_at(self.from_final_index(c.to_index()?)?)
    }

    /// The command a wire index in this numbering names: the final table's command, or `None`
    /// for an index with no final counterpart.
    #[must_use]
    pub fn command_from_wire(self, i: u16) -> Option<MotionCommand> {
        MotionCommand::from_index(self.to_final_index(i)?)
    }

    /// The wire index this numbering gives a final command; `None` when it does not have it.
    #[must_use]
    pub fn command_to_wire(self, c: MotionCommand) -> Option<u16> {
        self.from_final_index(c.to_index()?)
    }

    /// The numbering a set of files uses, told from the full command ids its human motion table
    /// carries (its default style, the style defaults and the link destinations): the first of
    /// [`Self::ALL`] that holds every one of them. Files that keep no command above `0x10E` read
    /// the same under all three, and come out [`Self::Final`]; ids no numbering holds also come
    /// out [`Self::Final`].
    pub fn infer(ids: impl IntoIterator<Item = u32> + Clone) -> Self {
        Self::ALL
            .into_iter()
            .find(|n| {
                ids.clone()
                    .into_iter()
                    .filter(|&id| id != 0)
                    .all(|id| n.holds(id))
            })
            .unwrap_or_default()
    }
}

// -------------------------------------------------------------------------------------------
// The client's two parallel arrays, verbatim.
// -------------------------------------------------------------------------------------------

pub(crate) const COMMAND_IDS: [u32; 412] = [
    0x80000000, 0x85000001, 0x85000002, 0x41000003, 0x40000004, 0x45000005, 0x45000006, 0x44000007,
    0x40000008, 0x40000009, 0x4000000A, 0x4000000B, 0x4000000C, 0x6500000D, 0x6500000E, 0x6500000F,
    0x65000010, 0x40000011, 0x41000012, 0x41000013, 0x41000014, 0x40000015, 0x40000016, 0x40000017,
    0x40000018, 0x40000019, 0x4000001A, 0x4000001B, 0x4000001C, 0x4000001D, 0x4000001E, 0x4000001F,
    0x40000020, 0x40000021, 0x40000022, 0x40000023, 0x40000024, 0x40000025, 0x40000026, 0x40000027,
    0x40000028, 0x40000029, 0x4000002A, 0x4000002B, 0x4000002C, 0x4000002D, 0x4000002E, 0x4000002F,
    0x40000030, 0x40000031, 0x40000032, 0x40000033, 0x40000034, 0x40000035, 0x40000036, 0x40000037,
    0x40000038, 0x40000039, 0x2000003A, 0x2500003B, 0x8000003C, 0x8000003D, 0x8000003E, 0x8000003F,
    0x80000040, 0x80000041, 0x80000042, 0x80000043, 0x80000044, 0x80000045, 0x80000046, 0x80000047,
    0x80000048, 0x80000049, 0x1000004A, 0x1000004B, 0x1300004C, 0x1000004D, 0x1000004E, 0x1000004F,
    0x10000050, 0x10000051, 0x10000052, 0x10000053, 0x10000054, 0x10000055, 0x10000056, 0x10000057,
    0x10000058, 0x10000059, 0x1000005A, 0x1000005B, 0x1000005C, 0x1000005D, 0x1000005E, 0x1000005F,
    0x10000060, 0x10000061, 0x10000062, 0x10000063, 0x10000064, 0x10000065, 0x10000066, 0x10000067,
    0x10000068, 0x10000069, 0x1000006A, 0x1000006B, 0x1000006C, 0x1000006D, 0x1000006E, 0x1000006F,
    0x10000070, 0x10000071, 0x10000072, 0x10000073, 0x10000074, 0x10000075, 0x10000076, 0x10000077,
    0x10000078, 0x13000079, 0x1300007A, 0x1300007B, 0x1300007C, 0x1300007D, 0x1300007E, 0x1300007F,
    0x13000080, 0x13000081, 0x13000082, 0x13000083, 0x13000084, 0x13000085, 0x13000086, 0x13000087,
    0x13000088, 0x13000089, 0x1300008A, 0x1300008B, 0x1300008C, 0x1300008D, 0x1300008E, 0x1300008F,
    0x13000090, 0x13000091, 0x13000092, 0x13000093, 0x13000094, 0x13000095, 0x13000096, 0x13000097,
    0x13000098, 0x13000099, 0x1300009A, 0x1200009B, 0x1000009C, 0x1000009D, 0x1000009E, 0x1000009F,
    0x100000A0, 0x100000A1, 0x080000A2, 0x090000A3, 0x090000A4, 0x090000A5, 0x090000A6, 0x090000A7,
    0x090000A8, 0x080000A9, 0x090000AA, 0x090000AB, 0x090000AC, 0x090000AD, 0x090000AE, 0x090000AF,
    0x090000B0, 0x090000B1, 0x0D0000B2, 0x0D0000B3, 0x0D0000B4, 0x080000B5, 0x080000B6, 0x080000B7,
    0x090000B8, 0x090000B9, 0x0D0000BA, 0x0D0000BB, 0x0D0000BC, 0x0D0000BD, 0x0D0000BE, 0x0D0000BF,
    0x090000C0, 0x0C0000C1, 0x090000C2, 0x090000C3, 0x090000C4, 0x0D0000C5, 0x090000C6, 0x090000C7,
    0x090000C8, 0x090000C9, 0x130000CA, 0x130000CB, 0x130000CC, 0x100000CD, 0x100000CE, 0x100000CF,
    0x100000D0, 0x100000D1, 0x100000D2, 0x400000D3, 0x120000D4, 0x090000D5, 0x090000D6, 0x090000D7,
    0x090000D8, 0x090000D9, 0x090000DA, 0x090000DB, 0x090000DC, 0x090000DD, 0x090000DE, 0x120000DF,
    0x400000E0, 0x400000E1, 0x100000E2, 0x100000E3, 0x400000E4, 0x400000E5, 0x400000E6, 0x090000E7,
    0x800000E8, 0x800000E9, 0x430000EA, 0x430000EB, 0x430000EC, 0x430000ED, 0x430000EE, 0x430000EF,
    0x430000F0, 0x430000F1, 0x430000F2, 0x430000F3, 0x430000F4, 0x430000F5, 0x430000F6, 0x430000F7,
    0x430000F8, 0x420000F9, 0x430000FA, 0x430000FB, 0x430000FC, 0x430000FD, 0x090000FE, 0x090000FF,
    0x09000100, 0x09000101, 0x09000102, 0x09000103, 0x09000104, 0x09000105, 0x09000106, 0x09000107,
    0x09000108, 0x09000109, 0x0900010A, 0x0900010B, 0x0900010C, 0x0900010D, 0x1000010E, 0x1000010F,
    0x10000110, 0x10000111, 0x09000112, 0x09000113, 0x09000114, 0x09000115, 0x09000116, 0x09000117,
    0x43000118, 0x13000119, 0x4300011A, 0x4300011B, 0x4300011C, 0x0900011D, 0x1000011E, 0x1000011F,
    0x10000120, 0x10000121, 0x10000122, 0x10000123, 0x10000124, 0x10000125, 0x10000126, 0x10000127,
    0x10000128, 0x10000129, 0x1000012A, 0x1000012B, 0x1000012C, 0x1000012D, 0x1000012E, 0x1000012F,
    0x10000130, 0x10000131, 0x10000132, 0x10000133, 0x10000134, 0x13000135, 0x40000136, 0x40000137,
    0x40000138, 0x40000139, 0x1000013A, 0x8000013B, 0x8000013C, 0x4300013D, 0x4300013E, 0x4300013F,
    0x43000140, 0x43000141, 0x43000142, 0x43000143, 0x43000144, 0x43000145, 0x43000146, 0x43000147,
    0x43000148, 0x43000149, 0x1300014A, 0x1300014B, 0x1300014C, 0x1300014D, 0x1300014E, 0x1300014F,
    0x13000150, 0x13000151, 0x13000152, 0x10000153, 0x09000154, 0x09000155, 0x09000156, 0x09000157,
    0x09000158, 0x09000159, 0x0900015A, 0x0900015B, 0x0900015C, 0x0900015D, 0x0900015E, 0x0900015F,
    0x09000160, 0x09000161, 0x09000162, 0x09000163, 0x09000164, 0x10000165, 0x10000166, 0x10000167,
    0x09000168, 0x09000169, 0x0900016A, 0x0900016B, 0x0900016C, 0x0900016D, 0x0900016E, 0x0900016F,
    0x09000170, 0x10000171, 0x10000172, 0x10000173, 0x10000174, 0x10000175, 0x10000176, 0x10000177,
    0x10000178, 0x10000179, 0x1000017A, 0x1000017B, 0x1000017C, 0x1000017D, 0x1000017E, 0x1000017F,
    0x10000180, 0x10000181, 0x10000182, 0x10000183, 0x10000184, 0x10000185, 0x10000186, 0x10000187,
    0x10000188, 0x10000189, 0x1000018A, 0x1000018B, 0x1000018C, 0x1000018D, 0x1000018E, 0x1000018F,
    0x10000190, 0x10000191, 0x10000192, 0x10000193, 0x10000194, 0x10000195, 0x10000196, 0x10000197,
    0x10000198, 0x10000199, 0x1000019A, 0x1000019B,
];

pub(crate) const COMMAND_NAMES: [&str; 412] = [
    "Invalid",
    "HoldRun",
    "HoldSidestep",
    "Ready",
    "Stop",
    "WalkForward",
    "WalkBackwards",
    "RunForward",
    "Fallen",
    "Interpolating",
    "Hover",
    "On",
    "Off",
    "TurnRight",
    "TurnLeft",
    "SideStepRight",
    "SideStepLeft",
    "Dead",
    "Crouch",
    "Sitting",
    "Sleeping",
    "Falling",
    "Reload",
    "Unload",
    "Pickup",
    "StoreInBackpack",
    "Eat",
    "Drink",
    "Reading",
    "JumpCharging",
    "AimLevel",
    "AimHigh15",
    "AimHigh30",
    "AimHigh45",
    "AimHigh60",
    "AimHigh75",
    "AimHigh90",
    "AimLow15",
    "AimLow30",
    "AimLow45",
    "AimLow60",
    "AimLow75",
    "AimLow90",
    "MagicBlast",
    "MagicSelfHead",
    "MagicSelfHeart",
    "MagicBonus",
    "MagicClap",
    "MagicHarm",
    "MagicHeal",
    "MagicThrowMissile",
    "MagicRecoilMissile",
    "MagicPenalty",
    "MagicTransfer",
    "MagicVision",
    "MagicEnchantItem",
    "MagicPortal",
    "MagicPray",
    "StopTurning",
    "Jump",
    "HandCombat",
    "NonCombat",
    "SwordCombat",
    "BowCombat",
    "SwordShieldCombat",
    "CrossbowCombat",
    "UnusedCombat",
    "SlingCombat",
    "2HandedSwordCombat",
    "2HandedStaffCombat",
    "DualWieldCombat",
    "ThrownWeaponCombat",
    "Graze",
    "Magic",
    "Hop",
    "Jumpup",
    "Cheer",
    "ChestBeat",
    "TippedLeft",
    "TippedRight",
    "FallDown",
    "Twitch1",
    "Twitch2",
    "Twitch3",
    "Twitch4",
    "StaggerBackward",
    "StaggerForward",
    "Sanctuary",
    "ThrustMed",
    "ThrustLow",
    "ThrustHigh",
    "SlashHigh",
    "SlashMed",
    "SlashLow",
    "BackhandHigh",
    "BackhandMed",
    "BackhandLow",
    "Shoot",
    "AttackHigh1",
    "AttackMed1",
    "AttackLow1",
    "AttackHigh2",
    "AttackMed2",
    "AttackLow2",
    "AttackHigh3",
    "AttackMed3",
    "AttackLow3",
    "HeadThrow",
    "FistSlam",
    "BreatheFlame_",
    "SpinAttack",
    "MagicPowerUp01",
    "MagicPowerUp02",
    "MagicPowerUp03",
    "MagicPowerUp04",
    "MagicPowerUp05",
    "MagicPowerUp06",
    "MagicPowerUp07",
    "MagicPowerUp08",
    "MagicPowerUp09",
    "MagicPowerUp10",
    "ShakeFist",
    "Beckon",
    "BeSeeingYou",
    "BlowKiss",
    "BowDeep",
    "ClapHands",
    "Cry",
    "Laugh",
    "MimeEat",
    "MimeDrink",
    "Nod",
    "Point",
    "ShakeHead",
    "Shrug",
    "Wave",
    "Akimbo",
    "HeartyLaugh",
    "Salute",
    "ScratchHead",
    "SmackHead",
    "TapFoot",
    "WaveHigh",
    "WaveLow",
    "YawnStretch",
    "Cringe",
    "Kneel",
    "Plead",
    "Shiver",
    "Shoo",
    "Slouch",
    "Spit",
    "Surrender",
    "Woah",
    "Winded",
    "YMCA",
    "EnterGame",
    "ExitGame",
    "OnCreation",
    "OnDestruction",
    "EnterPortal",
    "ExitPortal",
    "Cancel",
    "UseSelected",
    "AutosortSelected",
    "DropSelected",
    "GiveSelected",
    "SplitSelected",
    "ExamineSelected",
    "CreateShortcutToSelected",
    "PreviousCompassItem",
    "NextCompassItem",
    "ClosestCompassItem",
    "PreviousSelection",
    "LastAttacker",
    "PreviousFellow",
    "NextFellow",
    "ToggleCombat",
    "HighAttack",
    "MediumAttack",
    "LowAttack",
    "EnterChat",
    "ToggleChat",
    "SavePosition",
    "OptionsPanel",
    "ResetView",
    "CameraLeftRotate",
    "CameraRightRotate",
    "CameraRaise",
    "CameraLower",
    "CameraCloser",
    "CameraFarther",
    "FloorView",
    "MouseLook",
    "PreviousItem",
    "NextItem",
    "ClosestItem",
    "ShiftView",
    "MapView",
    "AutoRun",
    "DecreasePowerSetting",
    "IncreasePowerSetting",
    "Pray",
    "Mock",
    "Teapot",
    "SpecialAttack1",
    "SpecialAttack2",
    "SpecialAttack3",
    "MissileAttack1",
    "MissileAttack2",
    "MissileAttack3",
    "CastSpell",
    "Flatulence",
    "FirstPersonView",
    "AllegiancePanel",
    "FellowshipPanel",
    "SpellbookPanel",
    "SpellComponentsPanel",
    "HousePanel",
    "AttributesPanel",
    "SkillsPanel",
    "MapPanel",
    "InventoryPanel",
    "Demonet",
    "UseMagicStaff",
    "UseMagicWand",
    "Blink",
    "Bite",
    "TwitchSubstate1",
    "TwitchSubstate2",
    "TwitchSubstate3",
    "CaptureScreenshotToFile",
    "BowNoAmmo",
    "CrossBowNoAmmo",
    "ShakeFistState",
    "PrayState",
    "BowDeepState",
    "ClapHandsState",
    "CrossArmsState",
    "ShiverState",
    "PointState",
    "WaveState",
    "AkimboState",
    "SaluteState",
    "ScratchHeadState",
    "TapFootState",
    "LeanState",
    "KneelState",
    "PleadState",
    "ATOYOT",
    "SlouchState",
    "SurrenderState",
    "WoahState",
    "WindedState",
    "AutoCreateShortcuts",
    "AutoRepeatAttacks",
    "AutoTarget",
    "AdvancedCombatInterface",
    "IgnoreAllegianceRequests",
    "IgnoreFellowshipRequests",
    "InvertMouseLook",
    "LetPlayersGiveYouItems",
    "AutoTrackCombatTargets",
    "DisplayTooltips",
    "AttemptToDeceivePlayers",
    "RunAsDefaultMovement",
    "StayInChatModeAfterSend",
    "RightClickToMouseLook",
    "VividTargetIndicator",
    "SelectSelf",
    "SkillHealSelf",
    "SkillHealOther",
    "CombatEat",
    "CombatDrink",
    "NextMonster",
    "PreviousMonster",
    "ClosestMonster",
    "NextPlayer",
    "PreviousPlayer",
    "ClosestPlayer",
    "SnowAngelState",
    "WarmHands",
    "CurtseyState",
    "AFKState",
    "MeditateState",
    "TradePanel",
    "LogOut",
    "DoubleSlashLow",
    "DoubleSlashMed",
    "DoubleSlashHigh",
    "TripleSlashLow",
    "TripleSlashMed",
    "TripleSlashHigh",
    "DoubleThrustLow",
    "DoubleThrustMed",
    "DoubleThrustHigh",
    "TripleThrustLow",
    "TripleThrustMed",
    "TripleThrustHigh",
    "MagicPowerUp01Purple",
    "MagicPowerUp02Purple",
    "MagicPowerUp03Purple",
    "MagicPowerUp04Purple",
    "MagicPowerUp05Purple",
    "MagicPowerUp06Purple",
    "MagicPowerUp07Purple",
    "MagicPowerUp08Purple",
    "MagicPowerUp09Purple",
    "MagicPowerUp10Purple",
    "Helper",
    "Pickup5",
    "Pickup10",
    "Pickup15",
    "Pickup20",
    "HouseRecall",
    "AtlatlCombat",
    "ThrownShieldCombat",
    "SitState",
    "SitCrossleggedState",
    "SitBackState",
    "PointLeftState",
    "PointRightState",
    "TalktotheHandState",
    "PointDownState",
    "DrudgeDanceState",
    "PossumState",
    "ReadState",
    "ThinkerState",
    "HaveASeatState",
    "AtEaseState",
    "NudgeLeft",
    "NudgeRight",
    "PointLeft",
    "PointRight",
    "PointDown",
    "Knock",
    "ScanHorizon",
    "DrudgeDance",
    "HaveASeat",
    "LifestoneRecall",
    "CharacterOptionsPanel",
    "SoundAndGraphicsPanel",
    "HelpfulSpellsPanel",
    "HarmfulSpellsPanel",
    "CharacterInformationPanel",
    "LinkStatusPanel",
    "VitaePanel",
    "ShareFellowshipXP",
    "ShareFellowshipLoot",
    "AcceptCorpseLooting",
    "IgnoreTradeRequests",
    "DisableWeather",
    "DisableHouseEffect",
    "StretchUI",
    "ShowRadarCoordinates",
    "ShowSpellDurations",
    "MuteOnLosingFocus",
    "Fishing",
    "MarketplaceRecall",
    "EnterPKLite",
    "AllegianceChat",
    "AutomaticallyAcceptFellowshipRequests",
    "Reply",
    "MonarchReply",
    "PatronReply",
    "ToggleCraftingChanceOfSuccessDialog",
    "UseClosestUnopenedCorpse",
    "UseNextUnopenedCorpse",
    "IssueSlashCommand",
    "AllegianceHometownRecall",
    "PKArenaRecall",
    "OffhandSlashHigh",
    "OffhandSlashMed",
    "OffhandSlashLow",
    "OffhandThrustHigh",
    "OffhandThrustMed",
    "OffhandThrustLow",
    "OffhandDoubleSlashLow",
    "OffhandDoubleSlashMed",
    "OffhandDoubleSlashHigh",
    "OffhandTripleSlashLow",
    "OffhandTripleSlashMed",
    "OffhandTripleSlashHigh",
    "OffhandDoubleThrustLow",
    "OffhandDoubleThrustMed",
    "OffhandDoubleThrustHigh",
    "OffhandTripleThrustLow",
    "OffhandTripleThrustMed",
    "OffhandTripleThrustHigh",
    "OffhandKick",
    "AttackHigh4",
    "AttackMed4",
    "AttackLow4",
    "AttackHigh5",
    "AttackMed5",
    "AttackLow5",
    "AttackHigh6",
    "AttackMed6",
    "AttackLow6",
    "PunchFastHigh",
    "PunchFastMed",
    "PunchFastLow",
    "PunchSlowHigh",
    "PunchSlowMed",
    "PunchSlowLow",
    "OffhandPunchFastHigh",
    "OffhandPunchFastMed",
    "OffhandPunchFastLow",
    "OffhandPunchSlowHigh",
    "OffhandPunchSlowMed",
    "OffhandPunchSlowLow",
    "AI_TelegraphCast",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// ORACLE: the final retail client's two parallel 412-entry arrays, ids and names.
    #[test]
    fn all_412_triples_round_trip() {
        assert_eq!(COMMAND_IDS.len(), 412);
        assert_eq!(COMMAND_NAMES.len(), 412);
        let mut n = 0;
        for (i, cmd, name) in all() {
            assert_eq!(MotionCommand::from_index(i), Some(cmd), "index {i}");
            assert_eq!(cmd.to_index(), Some(i), "{name}");
            assert_eq!(cmd.name(), Some(name));
            assert_eq!(MotionCommand::from_name(name), Some(cmd));
            n += 1;
        }
        assert_eq!(n, 412);
    }

    /// **The table is the 2015 one at the rows that moved**, pinned by id, name and index -- the
    /// rows a 2013 table gets wrong. `SnowAngelState` is the owner's case: under the 2013 numbering
    /// it was `0x43000115`, which no shipped motion table plays.
    #[test]
    fn the_rows_the_2015_client_renumbered_have_their_2015_ids() {
        for (i, id, name, c) in [
            (
                0x10F_u16,
                0x1000_010F_u32,
                "SkillHealOther",
                MotionCommand::SKILL_HEAL_OTHER,
            ),
            (0x110, 0x1000_0110, "CombatEat", MotionCommand::COMBAT_EAT),
            (
                0x111,
                0x1000_0111,
                "CombatDrink",
                MotionCommand::COMBAT_DRINK,
            ),
            (
                0x112,
                0x0900_0112,
                "NextMonster",
                MotionCommand::NEXT_MONSTER,
            ),
            (
                0x118,
                0x4300_0118,
                "SnowAngelState",
                MotionCommand::SNOW_ANGEL_STATE,
            ),
            (0x119, 0x1300_0119, "WarmHands", MotionCommand::WARM_HANDS),
            (0x11E, 0x1000_011E, "LogOut", MotionCommand::LOG_OUT),
            (
                0x11F,
                0x1000_011F,
                "DoubleSlashLow",
                MotionCommand::DOUBLE_SLASH_LOW,
            ),
            (
                0x13B,
                0x8000_013B,
                "AtlatlCombat",
                MotionCommand::ATLATL_COMBAT,
            ),
            (
                0x13C,
                0x8000_013C,
                "ThrownShieldCombat",
                MotionCommand::THROWN_SHIELD_COMBAT,
            ),
            (0x13D, 0x4300_013D, "SitState", MotionCommand::SIT_STATE),
            (0x161, 0x0900_0161, "StretchUI", MotionCommand::STRETCH_UI),
            (
                0x185,
                0x1000_0185,
                "OffhandKick",
                MotionCommand::OFFHAND_KICK,
            ),
            (
                0x19A,
                0x1000_019A,
                "OffhandPunchSlowLow",
                MotionCommand::OFFHAND_PUNCH_SLOW_LOW,
            ),
            (
                0x19B,
                0x1000_019B,
                "AI_TelegraphCast",
                MotionCommand::AI_TELEGRAPH_CAST,
            ),
        ] {
            assert_eq!(c.0, id, "{name}");
            assert_eq!(
                MotionCommand::from_index(i),
                Some(c),
                "{name} is index {i:#X}"
            );
            assert_eq!(c.to_index(), Some(i), "{name}");
            assert_eq!(c.name(), Some(name));
        }
        assert_eq!(
            MotionCommand::from_name("SideBySideVitals"),
            None,
            "the final client dropped it"
        );
    }

    /// `from_index` is total over `u16` and refuses everything past the end of the table.
    #[test]
    fn from_index_is_none_past_the_end() {
        assert!(MotionCommand::from_index(411).is_some());
        assert!(MotionCommand::from_index(412).is_none());
        assert!(MotionCommand::from_index(u16::MAX).is_none());
    }

    /// The 2013 map: identity below `0x10F`, three up from there, the dropped row refused, and
    /// every 2013 command landing on the row of the same name. The 2013 names are the ones the
    /// 2015 table keeps, so the name at the mapped index is the check.
    #[test]
    fn a_2013_index_maps_onto_the_row_that_kept_its_name() {
        assert_eq!(
            retail_2013_index_to_current(0x0087),
            Some(0x0087),
            "Wave, below the insertion"
        );
        assert_eq!(
            retail_2013_index_to_current(0x010E),
            Some(0x010E),
            "SkillHealSelf"
        );
        // 2013 `NextMonster`, `SnowAngelState`, `LogOut`, `AtlatlCombat`, `SitState`.
        for (old, new) in [
            (0x10F, 0x112),
            (0x115, 0x118),
            (0x11B, 0x11E),
            (0x138, 0x13B),
            (0x13A, 0x13D),
        ] {
            assert_eq!(retail_2013_index_to_current(old), Some(new), "{old:#X}");
        }
        assert_eq!(
            retail_2013_index_to_current(0x015E),
            None,
            "SideBySideVitals is gone"
        );
        assert_eq!(
            retail_2013_index_to_current(0x015F),
            Some(0x0162),
            "ShowRadarCoordinates"
        );
        assert_eq!(
            retail_2013_index_to_current(0x0197),
            Some(0x019A),
            "the 2013 table's last row"
        );
        assert_eq!(
            retail_2013_index_to_current(0x0198),
            None,
            "past the 2013 table"
        );
        assert_eq!(
            MotionCommand::from_retail_2013_index(0x013A),
            Some(MotionCommand::SIT_STATE),
            "a 2013 client's SitState"
        );
        // Every 2013 index but the dropped one lands inside this table, on a distinct row, with
        // the class byte the 2013 id carried (the low half is the new index).
        let mut seen = std::collections::BTreeSet::new();
        for old in 0u16..408 {
            let Some(new) = retail_2013_index_to_current(old) else {
                assert_eq!(old, 0x015E);
                continue;
            };
            assert!(
                seen.insert(new),
                "{old:#X} and another 2013 index share {new:#X}"
            );
            assert!(
                MotionCommand::from_index(new).is_some(),
                "{old:#X} -> {new:#X}"
            );
        }
        assert_eq!(seen.len(), 407);
        assert_eq!(RETAIL_2013_SIDE_BY_SIDE_VITALS & 0xFFFF, 0x015E);
    }

    /// The numbering before 2015 is the 2013 map, both ways, and its ids carry the final table's
    /// class bytes over the older index: `LogOut` is `0x1000011B`, `SitState` `0x4300013A`,
    /// `AtlatlCombat` `0x80000138`.
    #[test]
    fn the_numbering_before_2015_is_the_2013_table() {
        let n = CommandNumbering::Before2015;
        for i in 0..n.command_count() {
            assert_eq!(
                n.to_final_index(i),
                retail_2013_index_to_current(i),
                "{i:#X}"
            );
            if let Some(j) = n.to_final_index(i) {
                assert_eq!(n.from_final_index(j), Some(i), "{j:#X}");
            }
        }
        for (old, c) in [
            (0x1000_011B, MotionCommand::LOG_OUT),
            (0x4300_013A, MotionCommand::SIT_STATE),
            (0x8000_0138, MotionCommand::ATLATL_COMBAT),
            (0x1000_011C, MotionCommand::DOUBLE_SLASH_LOW),
            (0x4100_0003, MotionCommand::READY),
        ] {
            assert_eq!(n.to_final(old), Some(c), "{old:#010X}");
            assert_eq!(n.from_final(c), Some(old), "{c:?}");
        }
        assert_eq!(n.command_from_wire(0x11B), Some(MotionCommand::LOG_OUT));
        assert_eq!(n.command_to_wire(MotionCommand::LOG_OUT), Some(0x11B));
        // The commands the final client added have no older index, and an id whose class byte
        // is not the older table's is not one of its commands.
        assert_eq!(n.from_final(MotionCommand::COMBAT_EAT), None);
        assert_eq!(n.command_to_wire(MotionCommand::AI_TELEGRAPH_CAST), None);
        assert_eq!(
            n.to_final(0x1000_011E),
            Some(MotionCommand::DOUBLE_SLASH_HIGH)
        );
        assert_eq!(n.to_final(0x4300_011B), None, "the final AFKState id");
        assert_eq!(n.id_at(0x15E), Some(RETAIL_2013_SIDE_BY_SIDE_VITALS));
        assert_eq!(n.to_final(RETAIL_2013_SIDE_BY_SIDE_VITALS), None);
        assert_eq!(n.id_at(408), None);
    }

    /// The final numbering is the table itself.
    #[test]
    fn the_final_numbering_is_the_identity() {
        let n = CommandNumbering::Final;
        for (i, c, _) in all() {
            assert_eq!(n.id_at(i), Some(c.0));
            assert_eq!(n.to_final(c.0), Some(c));
            assert_eq!(n.from_final(c), Some(c.0));
            assert_eq!(n.command_from_wire(i), Some(c));
            assert_eq!(n.command_to_wire(c), Some(i));
        }
        assert_eq!(n.id_at(412), None);
    }

    /// The January 2002 numbering: the older one below its emote block, then a reordered block
    /// whose survivors land on the rows of the same name, and seventeen commands with no later
    /// row. Every index maps to a distinct final row or to none.
    #[test]
    fn the_january_2002_numbering_reorders_the_emote_block() {
        let n = CommandNumbering::January2002;
        for (old, c) in [
            (0x1000_011B, MotionCommand::LOG_OUT),
            (0x8000_0138, MotionCommand::ATLATL_COMBAT),
            (0x4300_013A, MotionCommand::SIT_STATE),
            (0x4300_0141, MotionCommand::POINT_DOWN_STATE),
            (0x4300_0148, MotionCommand::HAVE_A_SEAT_STATE),
            (0x4300_014C, MotionCommand::AT_EASE_STATE),
            (0x1300_014E, MotionCommand::NUDGE_LEFT),
            (0x1300_0153, MotionCommand::POINT_DOWN),
            (0x1300_015C, MotionCommand::HAVE_A_SEAT),
        ] {
            assert_eq!(n.to_final(old), Some(c), "{old:#010X}");
            assert_eq!(n.from_final(c), Some(old), "{c:?}");
        }
        assert_eq!(n.to_final(0x4300_0140), None, "PointUpState");
        assert!(n.holds(0x4300_0140));
        assert_eq!(n.to_final(0x1300_0152), None, "PointUp");
        assert_eq!(n.id_at(0x160), None);
        let mut seen = std::collections::BTreeSet::new();
        let mut gone = 0;
        for i in 0..n.command_count() {
            assert!(n.id_at(i).is_some(), "{i:#X} has an id");
            match n.to_final_index(i) {
                Some(j) => assert!(seen.insert(j), "{i:#X} shares {j:#X}"),
                None => gone += 1,
            }
        }
        assert_eq!(gone, 17);
    }

    /// The numbering is told by the first table that holds every id: the end-of-retail ids are
    /// the final table's, the 2005 `LogOut` link is only the older one's, and the January 2002
    /// emote ids only that winter's. Ids below the insertion read the same everywhere and come
    /// out final.
    #[test]
    fn a_numbering_is_inferred_from_the_ids_a_table_holds() {
        let ids = |v: &[u32]| v.to_vec();
        assert_eq!(
            CommandNumbering::infer(ids(&[0x8000_003D, 0x4100_0003, 0x1000_011E, 0x4300_013D])),
            CommandNumbering::Final
        );
        assert_eq!(
            CommandNumbering::infer(ids(&[0x8000_003D, 0x4100_0003, 0x1000_011B, 0x4300_013A])),
            CommandNumbering::Before2015
        );
        assert_eq!(
            CommandNumbering::infer(ids(&[0x8000_003D, 0x1000_011B, 0x1300_0152])),
            CommandNumbering::January2002
        );
        assert_eq!(
            CommandNumbering::infer(ids(&[0x8000_003D, 0x4100_0003, 0x1000_0087])),
            CommandNumbering::Final
        );
        assert_eq!(
            CommandNumbering::infer(ids(&[0xDEAD_BEEF])),
            CommandNumbering::Final
        );
    }

    /// The ids are unique, which is what makes `to_index` well defined, and the names are unique
    /// case-insensitively, which is what makes the name lookup's scan well defined.
    #[test]
    fn ids_and_names_are_unique() {
        let mut ids: Vec<u32> = COMMAND_IDS.to_vec();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "duplicate id in the table");

        let mut names: Vec<String> = COMMAND_NAMES
            .iter()
            .map(|n| n.to_ascii_lowercase())
            .collect();
        names.sort();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "duplicate name in the table");
    }

    /// The trap: the low 16 bits happen to equal the index today. Assert that this is true of the
    /// shipped table *and* that the code does not rely on it - the first test proves the second
    /// half by going through `COMMAND_IDS`.
    #[test]
    fn the_low_16_bits_coincide_with_the_index_but_are_not_the_contract() {
        for (i, cmd, _) in all() {
            assert_eq!(cmd.0 & 0xFFFF, u32::from(i));
        }
    }

    /// The name lookup is case-insensitive.
    #[test]
    fn from_name_ignores_case() {
        assert_eq!(
            MotionCommand::from_name("runforward"),
            Some(MotionCommand::RUN_FORWARD)
        );
        assert_eq!(
            MotionCommand::from_name("RUNFORWARD"),
            Some(MotionCommand::RUN_FORWARD)
        );
        assert_eq!(
            MotionCommand::from_name("RunForward"),
            Some(MotionCommand::RUN_FORWARD)
        );
        assert_eq!(
            MotionCommand::from_name("BreatheFlame_"),
            Some(MotionCommand::BREATHE_FLAME_)
        );
        assert_eq!(
            MotionCommand::from_name("2HandedSwordCombat"),
            Some(MotionCommand::TWO_HANDED_SWORD_COMBAT)
        );
        assert_eq!(
            MotionCommand::from_name("snowangelstate"),
            Some(MotionCommand(0x4300_0118))
        );
        assert_eq!(MotionCommand::from_name("nosuchcommand"), None);
    }

    /// The class-bit predicates agree with the high byte of every id in the table.
    #[test]
    fn class_bits_agree_with_the_high_byte_for_every_id() {
        for (_, cmd, name) in all() {
            let hi = cmd.0 >> 24;
            assert_eq!(cmd.is_style(), hi & 0x80 != 0, "{name}");
            assert_eq!(cmd.is_substate(), hi & 0x40 != 0, "{name}");
            assert_eq!(cmd.is_modifier(), hi & 0x20 != 0, "{name}");
            assert_eq!(cmd.is_action(), hi & 0x10 != 0, "{name}");
            assert_eq!(cmd.is_ui(), hi & 0x08 != 0, "{name}");
            assert_eq!(cmd.has_speed(), hi & 0x04 != 0, "{name}");
            assert_eq!(cmd.is_emote(), hi & 0x02 != 0, "{name}");
            assert_eq!(cmd.is_persistent(), hi & 0x01 != 0, "{name}");
            assert_eq!(cmd.ordinal(), cmd.0 & 0x00FF_FFFF, "{name}");
        }
    }

    /// Spot checks against the values the track spec quotes.
    #[test]
    fn the_spec_quoted_constants_have_the_spec_quoted_values() {
        assert_eq!(MotionCommand::INVALID.0, 0x8000_0000);
        assert_eq!(MotionCommand::READY.0, 0x4100_0003);
        assert_eq!(MotionCommand::NON_COMBAT.0, 0x8000_003D);
        assert_eq!(MotionCommand::WALK_FORWARD.0, 0x4500_0005);
        assert_eq!(MotionCommand::RUN_FORWARD.0, 0x4400_0007);
        assert_eq!(MotionCommand::JUMP.0, 0x2500_003B);
        assert!(MotionCommand::JUMP.is_modifier());
        assert!(MotionCommand::NON_COMBAT.is_style());
        assert!(MotionCommand::READY.is_substate());
        assert!(MotionCommand::CHEER.is_action());
        assert!(MotionCommand::CHEER.is_emote());
    }

    /// Every stance in `STANCES` is a style, and the list is exactly the `0x8xxxxxxx` block.
    #[test]
    fn the_stance_set_is_the_0x8_block() {
        for s in STANCES {
            assert!(s.is_style(), "{s:?}");
            assert!(s.name().is_some(), "{s:?}");
        }
        // 21 ids have bit 31, not 19: `HoldRun (0x85000001)` and `HoldSidestep (0x85000002)`
        // carry it too, and they are hold-key commands rather than stances. `STANCES` is the
        // `MotionStance` set the motion tables are keyed by, which is the smaller list.
        let from_table: Vec<MotionCommand> =
            all().map(|(_, c, _)| c).filter(|c| c.is_style()).collect();
        assert_eq!(from_table.len(), 21, "ids with bit 31 set");
        assert_eq!(STANCES.len(), 19, "the MotionStance subset");
        for s in STANCES {
            assert!(from_table.contains(&s), "{s:?} missing from the table scan");
        }
        for extra in [MotionCommand::HOLD_RUN, MotionCommand::HOLD_SIDESTEP] {
            assert!(extra.is_style(), "carries bit 31");
            assert!(!STANCES.contains(&extra), "but is not a stance");
        }
        assert_eq!(MotionCommand::ATLATL_COMBAT.0, 0x8000_013B);
        assert_eq!(MotionCommand::THROWN_SHIELD_COMBAT.0, 0x8000_013C);
    }
}
