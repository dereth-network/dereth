// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.DatLoader/FileTypes/CombatManeuverTable.cs
//! `CombatManeuverTable.GetMotion`: the attack motions for a stance, height and attack type.
//!
//! ACE's `Unpack` indexes the maneuver list into `Stances[style][height][type] -> motions`, each
//! motion list in file order. Filtering the list in file order answers the same lists.

use dereth_assets::CombatManeuverTable;
use empyrean_entity::enums::MotionCommand;

/// ACE's `MotionCommand.Invalid`.
pub const MOTION_COMMAND_INVALID: u32 = MotionCommand::Invalid.0;

/// ACE's `CombatManeuverTable` helpers.
pub trait CombatManeuverTableExt {
    /// Every motion the table lists for `(stance, attack_height, attack_type)`, in file order, or
    /// ACE's `Invalid` list (`[MotionCommand.Invalid]`) when there is none. `prev_motion` is unused,
    /// as in ACE: when there are two, the caller picks by the power bar.
    fn get_motion(
        &self,
        stance: u32,
        attack_height: u32,
        attack_type: u32,
        prev_motion: u32,
    ) -> Vec<u32>;
}

impl CombatManeuverTableExt for CombatManeuverTable {
    // ACE: CombatManeuverTable.Unpack, CombatManeuverTable.GetMotion
    fn get_motion(
        &self,
        stance: u32,
        attack_height: u32,
        attack_type: u32,
        _prev_motion: u32,
    ) -> Vec<u32> {
        let maneuvers: Vec<u32> = self
            .maneuvers
            .iter()
            .filter(|m| {
                m.style == stance
                    && m.attack_height == attack_height
                    && m.attack_type == attack_type
            })
            .map(|m| m.motion)
            .collect();
        if maneuvers.is_empty() {
            return vec![MOTION_COMMAND_INVALID];
        }
        // if the CMT contains > 1 entries for this lookup, return both
        maneuvers
    }
}
