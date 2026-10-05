//! The emote input-action hash: the 91 `(input action -> motion command)` pairs that are
//! the whole of the emote and stance keys.
//!
//! # Why this file exists at all
//!
//! [`crate::actions::movement::on_action`] has an arm that reads this hash and takes it as a
//! **closure**, because the hash is the interpreter's own member and the decode is a free
//! function. A caller that passes `|_| None` sends every one of the 91 actions through to
//! `MovementAction::NotHandled`, and they are handed back to a dispatch list that has no other arm
//! for them — so the four stance keys (`Y` Ready, `H` Crouch, `G` Sitting, `B` Sleeping) and every
//! emote key do nothing. This is that closure.
//!
//! # The table is retail's own, not typed
//!
//! The hash is built by 91 consecutive action-to-motion insertions, where
//! both operands are `ulong` constants. The pairs below are those constants' **values**, in the
//! client's own call order. Nothing here is
//! transcribed from a name: `Wave` is `0x100000E5` and the wave motion is `0x13000087` because
//! those are the two dwords the client pairs.
//!
//! # What the 91 are, and why the split matters downstream
//!
//! Four of them are the **stances** declared in input map `4 MovementCommands` — `Ready`,
//! `Crouch`, `Sitting`, `Sleeping` — and the other 87 are input map `0x10000006 Emotes`. The
//! motion commands fall into three classes, and the client's command-list test puts
//! **all three on no list at all**, because the substate bit it tests (`0x04000000`) is clear in
//! every one of them:
//!
//! | class | high byte | example | what it is |
//! |---|---|---|---|
//! | `0x41……` | motion + persistent | sitting, `0x41000013` | the four stances |
//! | `0x42……` / `0x43……` | motion + emote (+ persistent) | the wave state, `0x430000F1` | a looping emote *state* |
//! | `0x12……` / `0x13……` | action + emote (+ persistent) | wave, `0x13000087` | a one-shot emote |
//!
//! So none of them can be projected onto the six direction bools of
//! `dereth_client_runtime::character::CharacterInput`; they are calls, which is
//! exactly what the action handler's tail does with them
//! (`DoMotion(player, cmd, params, 1)` on the press, `StopMotion` on the release, with
//! `action_stamp++` for the `0x10000000` class).
//!
//! **None of the 91 carries `0x08000000`**, so none of them is stopped by
//! the client's `if (cmd & 0x08000000) return;`. That is worth stating because it is easy to
//! assume the opposite: `MovementCommands::on_action`'s `KeyboardCommand::Ignored` arm is
//! unreachable for every command in this table.
//! The bit `0x08000000` is `MotionCommand::is_ui`, and the only command in the input layer that
//! sets it is `AutoRun 0x090000C7`, which the same function special-cases one line earlier.

use crate::actions::ActionId;

/// The emote input-action → command table, in the client's own insertion order.
///
/// `(input action id, motion command)`. 91 entries: the four stances of input map `4` first, then
/// the 87 emotes of input map `0x10000006`.
///
/// **The final retail client's values** (June 2015). The actions and the
/// order are the 2013 client's; the motion ids from `SnowAngelState` up are three higher, because
/// the final client's command table inserted three commands at `0x10F`-`0x111`
/// (`dereth_animation::command`). The end-of-retail motion tables are keyed by these ids, so a 2013 id here
/// plays no animation (the 2013 `SnowAngelState`, `0x43000115`, is a stance emote no table has).
pub const INPUT_ACTION_COMMANDS: &[(u32, u32)] = &[
    (0x10000094, 0x41000003), // Ready
    (0x10000095, 0x41000012), // Crouch
    (0x10000096, 0x41000013), // Sitting
    (0x10000097, 0x41000014), // Sleeping
    (0x100000CF, 0x430000EA), // ShakeFistState
    (0x100000C7, 0x430000EB), // PrayState
    (0x100000A1, 0x430000EC), // BowDeepState
    (0x100000A4, 0x430000ED), // ClapHandsState
    (0x100000A6, 0x430000EE), // CrossArmsState
    (0x100000D2, 0x430000EF), // ShiverState
    (0x100000BE, 0x430000F0), // PointState
    (0x100000E6, 0x430000F1), // WaveState
    (0x1000009B, 0x430000F2), // AkimboState
    (0x100000CA, 0x430000F3), // SaluteState
    (0x100000CD, 0x430000F4), // ScratchHeadState
    (0x100000E1, 0x430000F5), // TapFootState
    (0x100000B3, 0x430000F6), // LeanState
    (0x100000B0, 0x430000F7), // KneelState
    (0x100000BC, 0x430000F8), // PleadState
    (0x1000009A, 0x420000F9), // ATOYOT
    (0x100000D9, 0x430000FA), // SlouchState
    (0x100000DE, 0x430000FB), // SurrenderState
    (0x100000EC, 0x430000FC), // WoahState
    (0x100000EA, 0x430000FD), // WindedState
    (0x100000DB, 0x43000118), // SnowAngelState
    (0x100000A8, 0x4300011A), // CurtseyState
    (0x10000098, 0x4300011B), // AFKState
    (0x100000B4, 0x4300011C), // MeditateState
    (0x100000D5, 0x4300013D), // SitState
    (0x100000D7, 0x4300013E), // SitCrossleggedState
    (0x100000D6, 0x4300013F), // SitBackState
    (0x100000C2, 0x43000140), // PointLeftState
    (0x100000C4, 0x43000141), // PointRightState
    (0x100000DF, 0x43000142), // TalktotheHandState
    (0x100000C0, 0x43000143), // PointDownState
    (0x100000AA, 0x43000144), // DrudgeDanceState
    (0x100000C5, 0x43000145), // PossumState
    (0x100000C8, 0x43000146), // ReadState
    (0x100000E3, 0x43000147), // ThinkerState
    (0x100000AC, 0x43000148), // HaveASeatState
    (0x1000009C, 0x43000149), // AtEaseState
    (0x100000A2, 0x1300004C), // Cheer
    (0x100000A7, 0x1300007F), // Cry
    (0x100000CE, 0x13000079), // ShakeFist
    (0x1000009D, 0x1300007A), // Beckon
    (0x1000009E, 0x1300007B), // BeSeeingYou
    (0x1000009F, 0x1300007C), // BlowKiss
    (0x100000A0, 0x1300007D), // BowDeep
    (0x100000A3, 0x1300007E), // ClapHands
    (0x100000B2, 0x13000080), // Laugh
    (0x100000B6, 0x13000081), // MimeEat
    (0x100000B5, 0x13000082), // MimeDrink
    (0x100000B8, 0x13000083), // Nod
    (0x100000BD, 0x13000084), // Point
    (0x100000D0, 0x13000085), // ShakeHead
    (0x100000D4, 0x13000086), // Shrug
    (0x100000E5, 0x13000087), // Wave
    (0x10000099, 0x13000088), // Akimbo
    (0x100000AD, 0x13000089), // HeartyLaugh
    (0x100000C9, 0x1300008A), // Salute
    (0x100000CC, 0x1300008B), // ScratchHead
    (0x100000DA, 0x1300008C), // SmackHead
    (0x100000E0, 0x1300008D), // TapFoot
    (0x100000E8, 0x1300008E), // WaveHigh
    (0x100000E7, 0x1300008F), // WaveLow
    (0x100000ED, 0x13000090), // YawnStretch
    (0x100000A5, 0x13000091), // Cringe
    (0x100000AF, 0x13000092), // Kneel
    (0x100000BB, 0x13000093), // Plead
    (0x100000D1, 0x13000094), // Shiver
    (0x100000D3, 0x13000095), // Shoo
    (0x100000D8, 0x13000096), // Slouch
    (0x100000DC, 0x13000097), // Spit
    (0x100000DD, 0x13000098), // Surrender
    (0x100000EB, 0x13000099), // Woah
    (0x100000E9, 0x1300009A), // Winded
    (0x100000EE, 0x1200009B), // YMCA
    (0x100000C6, 0x130000CA), // Pray
    (0x100000B7, 0x130000CB), // Mock
    (0x100000E2, 0x130000CC), // Teapot
    (0x100000E4, 0x13000119), // WarmHands
    (0x100000B9, 0x1300014A), // NudgeLeft
    (0x100000BA, 0x1300014B), // NudgeRight
    (0x100000C1, 0x1300014C), // PointLeft
    (0x100000C3, 0x1300014D), // PointRight
    (0x100000BF, 0x1300014E), // PointDown
    (0x100000B1, 0x1300014F), // Knock
    (0x100000CB, 0x13000150), // ScanHorizon
    (0x100000A9, 0x13000151), // DrudgeDance
    (0x100000AB, 0x13000152), // HaveASeat
    (0x100000AE, 0x13000135), // Helper
];

/// The hash lookup itself — the closure [`crate::actions::movement::on_action`] takes.
///
/// A linear scan over 91 entries rather than a `HashMap`, because it runs once per input action
/// and the table is `const`: the client's own intrusive hash table is an implementation detail of
/// a structure that is built once and never changed, and reproducing it here would be a second
/// thing to keep right for no observable difference.
#[must_use]
pub fn command_for_action(action: ActionId) -> Option<u32> {
    INPUT_ACTION_COMMANDS
        .iter()
        .find(|(a, _)| *a == action.0)
        .map(|(_, c)| *c)
}

/// The four stance actions, which are the entries declared in input map `4` rather than in
/// `Emotes`. Named because they are the four the shipped keymap binds by default (`Y`, `H`, `G`,
/// `B`) and because [`crate::actions::movement::action`] declares them separately.
pub const STANCE_ACTIONS: [ActionId; 4] = [
    crate::actions::movement::action::READY,
    crate::actions::movement::action::CROUCH,
    crate::actions::movement::action::SIT,
    crate::actions::movement::action::LAY_DOWN,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of the hash: 91 pairs, no duplicate key, the four stances at the head.
    #[test]
    fn the_hash_is_ninety_one_pairs_with_the_four_stances_first() {
        assert_eq!(
            INPUT_ACTION_COMMANDS.len(),
            91,
            "the hash is built with 91 adds"
        );
        let mut keys: Vec<u32> = INPUT_ACTION_COMMANDS.iter().map(|(a, _)| *a).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), 91, "a hash cannot carry one action twice");
        for (i, s) in STANCE_ACTIONS.iter().enumerate() {
            assert_eq!(
                INPUT_ACTION_COMMANDS[i].0, s.0,
                "the stances are inserted first"
            );
        }
        assert_eq!(
            command_for_action(ActionId(0x1000_00E5)),
            Some(0x1300_0087),
            "Wave"
        );
        assert_eq!(
            command_for_action(ActionId(0x1000_0097)),
            Some(0x4100_0014),
            "Sleeping"
        );
        assert_eq!(
            command_for_action(ActionId(0x29)),
            None,
            "MovementForward is not an emote"
        );
        // The final client's numbering, pinned where it moved.
        assert_eq!(
            command_for_action(ActionId(0x1000_00DB)),
            Some(0x4300_0118),
            "SnowAngelState"
        );
        assert_eq!(
            command_for_action(ActionId(0x1000_00D5)),
            Some(0x4300_013D),
            "SitState"
        );
        assert_eq!(
            command_for_action(ActionId(0x1000_00E4)),
            Some(0x1300_0119),
            "WarmHands"
        );
        assert_eq!(
            command_for_action(ActionId(0x1000_00AE)),
            Some(0x1300_0135),
            "Helper"
        );
    }

    /// **The claim the header makes, as an assertion.** `HandleKeyboardCommand`'s
    /// `if (cmd & 0x08000000) return;` cannot reject any of these, so `KeyboardCommand::Ignored`
    /// is unreachable from this table -- and every one of them is on **no** command list, so
    /// `MovePlayer`'s generic `DoMotion` tail is the only thing that can issue one.
    #[test]
    fn no_emote_command_is_a_non_motion_command_and_none_is_on_a_list() {
        use crate::actions::movement::{command, which_list, CommandList};
        for (a, c) in INPUT_ACTION_COMMANDS {
            assert_eq!(
                c & command::BIT_NOT_MOTION,
                0,
                "{a:#010X} -> {c:#010X} would be dropped by HandleKeyboardCommand"
            );
            assert_eq!(
                which_list(*c),
                CommandList::None,
                "{a:#010X} -> {c:#010X}: tests 0x04000000, which is clear"
            );
        }
        // …and the instrument can say otherwise, or the two zeroes above mean nothing.
        assert_eq!(
            command::AUTO_RUN & command::BIT_NOT_MOTION,
            command::BIT_NOT_MOTION
        );
        assert_eq!(which_list(command::WALK_FORWARD), CommandList::Substate);
    }
}
