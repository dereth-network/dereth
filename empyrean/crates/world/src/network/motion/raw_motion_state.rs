// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Motion/RawMotionState.cs
//! Port of `Source/ACE.Server/Network/Motion/RawMotionState.cs`.
//!
//! ACE keeps a back reference to the `MoveToState` being read; only its `WorldObject` is used
//! (for the command items), so the reader takes that object's guid.

use empyrean_common::dotnet::binary_reader::{BinaryReader, ReadError};
use empyrean_common::dotnet::to_string;
use empyrean_entity::enums::{HoldKey, MotionCommand, MotionStance};
use empyrean_entity::ObjectGuid;

use super::motion_item::MotionItem;
use crate::world_objects::world_object_networking::shims;

/// `ACE.Server.Network.Enum.RawMotionFlags` (`[Flags] : uint`); not ported yet by its owner.
#[allow(non_snake_case, non_upper_case_globals)]
pub mod RawMotionFlags {
    pub const Invalid: u32 = 0x0;
    pub const CurrentHoldKey: u32 = 0x1;
    pub const CurrentStyle: u32 = 0x2;
    pub const ForwardCommand: u32 = 0x4;
    pub const ForwardHoldKey: u32 = 0x8;
    pub const ForwardSpeed: u32 = 0x10;
    pub const SideStepCommand: u32 = 0x20;
    pub const SideStepHoldKey: u32 = 0x40;
    pub const SideStepSpeed: u32 = 0x80;
    pub const TurnCommand: u32 = 0x100;
    pub const TurnHoldKey: u32 = 0x200;
    pub const TurnSpeed: u32 = 0x400;

    /// `RawMotionFlags.ToString()` for a flags value: the set member names joined by `", "`,
    /// `Invalid` for zero, the number when a bit has no name.
    #[must_use]
    pub fn to_dotnet_string(flags: u32) -> String {
        const NAMES: [(u32, &str); 11] = [
            (CurrentHoldKey, "CurrentHoldKey"),
            (CurrentStyle, "CurrentStyle"),
            (ForwardCommand, "ForwardCommand"),
            (ForwardHoldKey, "ForwardHoldKey"),
            (ForwardSpeed, "ForwardSpeed"),
            (SideStepCommand, "SideStepCommand"),
            (SideStepHoldKey, "SideStepHoldKey"),
            (SideStepSpeed, "SideStepSpeed"),
            (TurnCommand, "TurnCommand"),
            (TurnHoldKey, "TurnHoldKey"),
            (TurnSpeed, "TurnSpeed"),
        ];
        if flags == 0 {
            return "Invalid".to_owned();
        }
        if flags & !0x7FF != 0 {
            return flags.to_string();
        }
        NAMES
            .iter()
            .filter(|(v, _)| flags & v != 0)
            .map(|(_, n)| *n)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

// ACE: RawMotionState
/// The raw movement commands sent by client.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawMotionState {
    // ACE: RawMotionState.PackedFlags
    pub packed_flags: u32,

    // ACE: RawMotionState.Flags
    /// stored as the first 11 bits of PackedFlags (`RawMotionFlags`)
    pub flags: u32,
    // ACE: RawMotionState.CommandListLength
    /// starts at bit 12 of PackedFlags
    pub command_list_length: u16,

    // choose valid sections by masking against Flags
    // ACE: RawMotionState.CurrentHoldKey
    /// 0x1 - walk/run
    pub current_hold_key: HoldKey,
    // ACE: RawMotionState.CurrentStyle
    /// 0x2 - current stance
    pub current_style: MotionStance,
    // ACE: RawMotionState.ForwardCommand
    /// 0x4 - forward movement or motion command (default invalid or ready?)
    pub forward_command: MotionCommand,
    // ACE: RawMotionState.ForwardHoldKey
    /// 0x8 - whether forward/back key is being held
    pub forward_hold_key: HoldKey,
    // ACE: RawMotionState.ForwardSpeed
    /// 0x10 - forward/back movement speed
    pub forward_speed: f32,
    // ACE: RawMotionState.SidestepCommand
    /// 0x20 - sidestep movement command
    pub sidestep_command: MotionCommand,
    // ACE: RawMotionState.SidestepHoldKey
    /// 0x40 - indicates whether a sidestep key is being held
    pub sidestep_hold_key: HoldKey,
    // ACE: RawMotionState.SidestepSpeed
    /// 0x80 - sidestep movement speed
    pub sidestep_speed: f32,
    // ACE: RawMotionState.TurnCommand
    /// 0x100 - turn command - this is always sent as 1 direction in RawMotion, with a negative
    /// speed for the opposite direction. A negative speed is then in turn translated to the
    /// opposite Motion in InterpretedMotionState.
    pub turn_command: MotionCommand,
    // ACE: RawMotionState.TurnHoldKey
    /// 0x200 - whether turn key is being held, or mouselook in progress
    pub turn_hold_key: HoldKey,
    // ACE: RawMotionState.TurnSpeed
    /// 0x400 - turn movement speed - somewhat static
    pub turn_speed: f32,

    // ACE: RawMotionState.Commands
    /// commands: list of length commandListLength (`null` when the length is 0)
    pub commands: Option<Vec<MotionItem>>,
}

// ACE: RawMotionState.None
/// `RawMotionState.None`: an empty state.
#[must_use]
pub fn none() -> RawMotionState {
    RawMotionState::default()
}

impl RawMotionState {
    // ACE: RawMotionState.RawMotionState
    /// `new RawMotionState(MoveToState moveToState, BinaryReader reader)`; `world_object` is
    /// `moveToState.WorldObject`. Command items that are not a soul emote at speed 1.0 are
    /// dropped (and logged), after being read. Not ACE: the client wrote its stance and commands
    /// in `numbering`, the world's files' (ACE's is always the final one); they are read into the
    /// final numbering.
    pub fn read(
        world_object: ObjectGuid,
        reader: &mut BinaryReader<'_>,
        numbering: dereth_world_data::command_numbering::CommandNumbering,
    ) -> Result<Self, ReadError> {
        use dereth_world_data::command_numbering::final_id;
        use RawMotionFlags as F;

        let mut s = RawMotionState {
            packed_flags: reader.read_u32()?,
            ..Default::default()
        };

        // security vulnerability here:
        // untrusted client input sending command list length
        s.flags = s.packed_flags & 0x7FF;
        #[allow(clippy::cast_possible_truncation)] // `(ushort)(PackedFlags >> 11)`
        {
            s.command_list_length = (s.packed_flags >> 11) as u16;
        }

        if s.flags & F::CurrentHoldKey != 0 {
            s.current_hold_key = HoldKey(reader.read_u32()?);
        }
        if s.flags & F::CurrentStyle != 0 {
            s.current_style = MotionStance(final_id(numbering, reader.read_u32()?));
        }
        if s.flags & F::ForwardCommand != 0 {
            s.forward_command = MotionCommand(final_id(numbering, reader.read_u32()?));
        }
        if s.flags & F::ForwardHoldKey != 0 {
            s.forward_hold_key = HoldKey(reader.read_u32()?);
        }
        if s.flags & F::ForwardSpeed != 0 {
            s.forward_speed = reader.read_f32()?;
        }
        if s.flags & F::SideStepCommand != 0 {
            s.sidestep_command = MotionCommand(final_id(numbering, reader.read_u32()?));
        }
        if s.flags & F::SideStepHoldKey != 0 {
            s.sidestep_hold_key = HoldKey(reader.read_u32()?);
        }
        if s.flags & F::SideStepSpeed != 0 {
            s.sidestep_speed = reader.read_f32()?;
        }
        if s.flags & F::TurnCommand != 0 {
            s.turn_command = MotionCommand(final_id(numbering, reader.read_u32()?));
        }
        if s.flags & F::TurnHoldKey != 0 {
            s.turn_hold_key = HoldKey(reader.read_u32()?);
        }
        if s.flags & F::TurnSpeed != 0 {
            s.turn_speed = reader.read_f32()?;
        }

        // cases where this is > 1?
        if s.command_list_length > 0 {
            let mut commands = Vec::new();
            for _ in 0..s.command_list_length {
                let motion_item = MotionItem::read(world_object, reader, numbering)?;
                if shims::soul_emote_contains(motion_item.motion_command)
                    && motion_item.speed == 1.0
                {
                    commands.push(motion_item);
                } else {
                    log::error!(
                        "RawMotionState reader - received non-standard action {}, Speed: {} for {:?}",
                        motion_item.motion_command,
                        to_string(motion_item.speed),
                        world_object
                    );
                }
            }
            s.commands = Some(commands);
        }
        Ok(s)
    }

    // ACE: RawMotionState.ToString
    /// `ToString(bool showFlags)`; `ToString()` is `ToString(true)`.
    #[must_use]
    pub fn to_string_flags(&self, show_flags: bool) -> String {
        use RawMotionFlags as F;

        let mut sb = String::new();
        let mut line = |s: String| {
            sb += &s;
            sb += "\r\n";
        };

        if show_flags {
            line(format!("Flags: {}", F::to_dotnet_string(self.flags)));
        }

        if self.flags & F::CurrentHoldKey != 0 {
            line(format!("CurrentHoldKey: {}", self.current_hold_key));
        }
        if self.flags & F::CurrentStyle != 0 {
            line(format!("CurrentStyle: {}", self.current_style));
        }
        if self.flags & F::ForwardCommand != 0 {
            line(format!("ForwardCommand: {}", self.forward_command));
        }
        if self.flags & F::ForwardHoldKey != 0 {
            line(format!("ForwardHoldKey: {}", self.forward_hold_key));
        }
        if self.flags & F::ForwardSpeed != 0 {
            line(format!("ForwardSpeed: {}", to_string(self.forward_speed)));
        }
        if self.flags & F::SideStepCommand != 0 {
            line(format!("SidestepCommand: {}", self.sidestep_command));
        }
        if self.flags & F::SideStepHoldKey != 0 {
            line(format!("SidestepHoldKey: {}", self.sidestep_hold_key));
        }
        if self.flags & F::SideStepSpeed != 0 {
            line(format!("SidestepSpeed: {}", to_string(self.sidestep_speed)));
        }
        if self.flags & F::TurnCommand != 0 {
            line(format!("TurnCommand: {}", self.turn_command));
        }
        if self.flags & F::TurnHoldKey != 0 {
            line(format!("TurnHoldKey: {}", self.turn_hold_key));
        }
        if self.flags & F::TurnSpeed != 0 {
            line(format!("TurnSpeed: {}", to_string(self.turn_speed)));
        }

        if self.command_list_length > 0 {
            line(format!("CommandListLength: {}", self.command_list_length));
            let commands: String = self
                .commands
                .iter()
                .flatten()
                .map(std::string::ToString::to_string)
                .collect();
            sb += &commands;
        }

        sb += "---\r\n";

        sb
    }

    // ACE: RawMotionState.HasMovement
    #[must_use]
    pub fn has_movement(&self) -> bool {
        use RawMotionFlags as F;
        (self.flags & (F::ForwardCommand | F::TurnCommand | F::SideStepCommand)) != 0
    }

    // ACE: RawMotionState.HasSoulEmote
    /// `check_forward` defaults to true in ACE. With commands dropped by the reader,
    /// `Commands[0]` of an empty list throws in ACE (a panic here).
    #[must_use]
    pub fn has_soul_emote(&self, check_forward: bool) -> bool {
        if check_forward
            && (self.flags & RawMotionFlags::ForwardCommand) != 0
            && shims::soul_emote_contains(self.forward_command)
        {
            return true;
        }

        if self.command_list_length > 0 {
            let first = self
                .commands
                .as_ref()
                .and_then(|c| c.first())
                .expect("ACE: Commands[0] (ArgumentOutOfRangeException)");
            if shims::soul_emote_contains(first.motion_command) {
                return true;
            }
        }

        false
    }
}

impl std::fmt::Display for RawMotionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_string_flags(true))
    }
}
