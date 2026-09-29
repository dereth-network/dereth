// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/JumpPack.cs
//! Port of `Source/ACE.Server/Network/Structure/JumpPack.cs`.

use empyrean_common::dotnet::binary_reader::{BinaryReader, ReadError};
use empyrean_entity::Vector3;

use super::extensions::read_vector3;

// ACE: JumpPack
/// The client's jump request.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct JumpPack {
    // ACE: JumpPack.Extent
    /// jump power 0-1
    pub extent: f32,
    // ACE: JumpPack.Velocity
    pub velocity: Vector3,
    // ACE: JumpPack.InstanceSequence
    pub instance_sequence: u16,
    // ACE: JumpPack.ServerControlSequence
    pub server_control_sequence: u16,
    // ACE: JumpPack.TeleportSequence
    pub teleport_sequence: u16,
    // ACE: JumpPack.ForcePositionSequence
    pub force_position_sequence: u16,
}

impl JumpPack {
    // ACE: JumpPack.JumpPack
    /// `new JumpPack(BinaryReader reader)`, ACE's layout (the sequences straight after the
    /// velocity). The jump handler does not use it: it reads the client's 56-byte pack, whose
    /// position comes between the velocity and the sequences (V300).
    pub fn read(reader: &mut BinaryReader<'_>) -> Result<Self, ReadError> {
        Ok(JumpPack {
            extent: reader.read_f32()?,
            velocity: read_vector3(reader)?,
            instance_sequence: reader.read_u16()?,
            server_control_sequence: reader.read_u16()?,
            teleport_sequence: reader.read_u16()?,
            force_position_sequence: reader.read_u16()?,
        })
    }
}

impl std::fmt::Display for JumpPack {
    // ACE: JumpPack.ToString
    /// `StringBuilder.AppendLine` ends each line with `"\r\n"` on Windows.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use empyrean_common::dotnet::to_string;
        let v = self.velocity;
        let mut sb = String::new();
        sb += &format!("Extent: {}\r\n", to_string(self.extent));
        sb += &format!(
            "Velocity: <{}, {}, {}>\r\n",
            to_string(v.x),
            to_string(v.y),
            to_string(v.z)
        );
        sb += &format!("InstanceSequence: {}\r\n", self.instance_sequence);
        sb += &format!(
            "ServerControlSequence: {}\r\n",
            self.server_control_sequence
        );
        sb += &format!("TeleportSequence: {}\r\n", self.teleport_sequence);
        sb += &format!(
            "ForcePositionSequence: {}\r\n",
            self.force_position_sequence
        );
        f.write_str(&sb)
    }
}
