// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/Origin.cs
//! Port of `Source/ACE.Server/Network/Structure/Origin.cs`.

use empyrean_entity::{Position, Vector3};

use crate::network::game_messages::game_message::write_record;

// ACE: Origin
/// A cell and origin, without orientation.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Origin {
    // ACE: Origin.CellID
    pub cell_id: u32,
    // ACE: Origin.Position
    pub position: Vector3,
}

impl Origin {
    // ACE: Origin.Origin
    /// `new Origin(uint cellID, Vector3 position)`.
    #[must_use]
    pub const fn new(cell_id: u32, position: Vector3) -> Self {
        Self { cell_id, position }
    }

    // ACE: Origin.Origin
    /// `new Origin(Position pos)`.
    #[must_use]
    pub fn from_position(pos: &Position) -> Self {
        Self {
            cell_id: pos.cell(),
            position: pos.pos(),
        }
    }
}

// ACE: OriginExtensions.Write
pub fn write(writer: &mut Vec<u8>, origin: &Origin) {
    write_record(writer, &[], |w| record(origin).write(w));
}

/// The dereth-protocol record the `Write` extension below writes, field for field.
#[must_use]
pub fn record(origin: &Origin) -> dereth_protocol::types::Origin {
    dereth_protocol::types::Origin {
        objcell_id: origin.cell_id,
        origin: empyrean_entity::shared_types::wire_vec3(origin.position),
    }
}
