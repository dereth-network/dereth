// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/MotionCommand.cs

use crate::enums::MotionCommand;

/// ACE's `MotionCommandHelper.GetMotion` is a plain static function, not an extension.
pub mod motion_command_helper {
    use crate::enums::MotionCommand;

    // ACE: MotionCommandHelper.GetMotion
    pub fn get_motion(motion_command: MotionCommand) -> MotionCommand {
        if motion_command.0 == 0x10000162 {
            return MotionCommand::Fishing;
        }

        motion_command
    }
}

impl MotionCommand {
    // ACE: MotionCommandHelper.IsMultiStrike
    pub fn is_multi_strike(self) -> bool {
        self >= MotionCommand::DoubleSlashLow && self <= MotionCommand::TripleThrustHigh
            || self >= MotionCommand::OffhandDoubleSlashLow
                && self <= MotionCommand::OffhandTripleThrustHigh
    }

    // ACE: MotionCommandHelper.ReduceMultiStrike
    pub fn reduce_multi_strike(self) -> MotionCommand {
        if !self.is_multi_strike() {
            return MotionCommand::Invalid;
        }

        match self {
            MotionCommand::DoubleSlashLow | MotionCommand::TripleSlashLow => {
                MotionCommand::SlashLow
            }

            MotionCommand::DoubleSlashMed | MotionCommand::TripleSlashMed => {
                MotionCommand::SlashMed
            }

            MotionCommand::DoubleSlashHigh | MotionCommand::TripleSlashHigh => {
                MotionCommand::SlashHigh
            }

            MotionCommand::DoubleThrustLow | MotionCommand::TripleThrustLow => {
                MotionCommand::ThrustLow
            }

            MotionCommand::DoubleThrustMed | MotionCommand::TripleThrustMed => {
                MotionCommand::ThrustMed
            }

            MotionCommand::DoubleThrustHigh | MotionCommand::TripleThrustHigh => {
                MotionCommand::ThrustHigh
            }

            MotionCommand::OffhandDoubleSlashLow | MotionCommand::OffhandTripleSlashLow => {
                MotionCommand::SlashLow
            }

            MotionCommand::OffhandDoubleSlashMed | MotionCommand::OffhandTripleSlashMed => {
                MotionCommand::SlashMed
            }

            MotionCommand::OffhandDoubleSlashHigh | MotionCommand::OffhandTripleSlashHigh => {
                MotionCommand::SlashHigh
            }

            MotionCommand::OffhandDoubleThrustLow | MotionCommand::OffhandTripleThrustLow => {
                MotionCommand::ThrustLow
            }

            MotionCommand::OffhandDoubleThrustMed | MotionCommand::OffhandTripleThrustMed => {
                MotionCommand::ThrustMed
            }

            MotionCommand::OffhandDoubleThrustHigh | MotionCommand::OffhandTripleThrustHigh => {
                MotionCommand::ThrustHigh
            }

            _ => MotionCommand::Invalid,
        }
    }

    // ACE: MotionCommandHelper.IsSubsequent
    pub fn is_subsequent(self) -> bool {
        self >= MotionCommand::AttackHigh2 && self <= MotionCommand::AttackLow3
            || self >= MotionCommand::AttackHigh4 && self <= MotionCommand::AttackLow6
    }

    // ACE: MotionCommandHelper.ReduceSubsequent
    pub fn reduce_subsequent(self) -> MotionCommand {
        if !self.is_subsequent() {
            return MotionCommand::Invalid;
        }

        match self {
            MotionCommand::AttackLow2
            | MotionCommand::AttackLow3
            | MotionCommand::AttackLow4
            | MotionCommand::AttackLow5
            | MotionCommand::AttackLow6 => MotionCommand::AttackLow1,

            MotionCommand::AttackMed2
            | MotionCommand::AttackMed3
            | MotionCommand::AttackMed4
            | MotionCommand::AttackMed5
            | MotionCommand::AttackMed6 => MotionCommand::AttackMed1,

            MotionCommand::AttackHigh2
            | MotionCommand::AttackHigh3
            | MotionCommand::AttackHigh4
            | MotionCommand::AttackHigh5
            | MotionCommand::AttackHigh6 => MotionCommand::AttackHigh1,

            _ => MotionCommand::Invalid,
        }
    }

    // ACE: MotionCommandHelper.GetAimAngle
    pub fn get_aim_angle(self) -> f32 {
        match self {
            MotionCommand::AimHigh15 => 15.0,
            MotionCommand::AimHigh30 => 30.0,
            MotionCommand::AimHigh45 => 45.0,
            MotionCommand::AimHigh60 => 60.0,
            MotionCommand::AimHigh75 => 75.0,
            MotionCommand::AimHigh90 => 90.0,

            MotionCommand::AimLow15 => -15.0,
            MotionCommand::AimLow30 => -30.0,
            MotionCommand::AimLow45 => -45.0,
            MotionCommand::AimLow60 => -60.0,
            MotionCommand::AimLow75 => -75.0,
            MotionCommand::AimLow90 => -90.0,

            _ => 0.0,
        }
    }
}
