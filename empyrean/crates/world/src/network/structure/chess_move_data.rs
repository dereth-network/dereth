// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/ChessMoveData.cs
//! Port of `Source/ACE.Server/Network/Structure/ChessMoveData.cs`.

use empyrean_entity::enums::{ChessColor, ChessMoveType};
use empyrean_entity::ObjectGuid;

use crate::network::game_messages::game_message::write_record;

pub use crate::entity::chess::chess_piece_coord::ChessPieceCoord;
pub use crate::entity::chess::game_move_data::GameMoveData;

// ACE: ChessMoveData
/// Set of information related to a chess game move.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChessMoveData {
    // ACE: ChessMoveData.Type
    /// type of move
    pub r#type: ChessMoveType,
    // ACE: ChessMoveData.PlayerGuid
    /// player making this move
    pub player_guid: ObjectGuid,
    // ACE: ChessMoveData.Color
    /// team making this move
    pub color: ChessColor,

    // Select one section based on the value of type:
    // case 0x4:
    // ACE: ChessMoveData.PieceGuid
    /// guid of piece being moved
    pub piece_guid: ObjectGuid,
    // ACE: ChessMoveData.From
    /// position moved from
    pub from: Option<ChessPieceCoord>,

    // case 0x5:
    // ACE: ChessMoveData.To
    /// position moved to
    pub to: Option<ChessPieceCoord>,
    // case 0x6: PieceGuid
}

impl ChessMoveData {
    // ACE: ChessMoveData.ChessMoveData
    /// `new ChessMoveData(ObjectGuid playerGuid, ObjectGuid pieceGuid, GameMoveData data)`.
    #[must_use]
    pub fn new(player_guid: ObjectGuid, piece_guid: ObjectGuid, data: &GameMoveData) -> Self {
        Self {
            r#type: data.move_type,
            player_guid,
            color: data.color,
            piece_guid,
            from: data.from,
            to: data.to,
        }
    }
}

// ACE: ChestMoveDataExtensions.Write
/// `writer.Write(ChessMoveData data)` (ACE names the class `ChestMoveDataExtensions`).
pub fn write(writer: &mut Vec<u8>, data: &ChessMoveData) {
    write_record(writer, &[], |w| record(data).write(w));
}

/// The dereth-protocol record the `Write` extension below writes, field for field. A null coord throws in ACE.
#[must_use]
pub fn record(data: &ChessMoveData) -> dereth_protocol::trade::GameMoveData {
    let coord = |c: Option<&ChessPieceCoord>| {
        let c = c.expect("ACE: ChessPieceCoord is null (NullReferenceException)");
        (c.x.cast_unsigned(), c.y.cast_unsigned())
    };
    let mut record = dereth_protocol::trade::GameMoveData {
        move_type: data.r#type.0.cast_unsigned(),
        player: data.player_guid.into(),
        ..Default::default()
    };
    //writer.Write((int)data.Color);

    // only ChessMoveType.FromTo used?
    match data.r#type {
        ChessMoveType::Grid => {
            // xgrid / ygrid?
            record.from = Some(coord(data.to.as_ref()));
        }
        ChessMoveType::FromTo => {
            //writer.Write(data.PieceGuid.Full);
            record.from = Some(coord(data.from.as_ref()));
            record.to = Some(coord(data.to.as_ref()));
        }
        ChessMoveType::SelectedPiece => {
            record.piece_index = Some(data.piece_guid.full());
        }
        _ => {}
    }
    record
}
