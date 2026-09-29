// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Chess/ChessSide.cs
//! Port of `Source/ACE.Server/Entity/Chess/ChessSide.cs`.

use empyrean_entity::enums::ChessColor;
use empyrean_entity::ObjectGuid;

use crate::managers::player_manager;
use crate::World;

// ACE: ChessSide
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChessSide {
    pub player_guid: ObjectGuid,
    pub color: ChessColor,
    pub stalemate: bool,
}

impl ChessSide {
    // ACE: ChessSide.ChessSide
    #[must_use]
    pub fn new(player_guid: ObjectGuid, color: ChessColor) -> Self {
        Self {
            player_guid,
            color,
            stalemate: false,
        }
    }

    // ACE: ChessSide.IsAi
    /// The AI plays as guid 0, which is not a player guid.
    #[must_use]
    pub fn is_ai(&self) -> bool {
        !self.player_guid.is_player()
    }

    // ACE: ChessSide.GetPlayer
    /// `PlayerManager.GetOnlinePlayer(PlayerGuid)`.
    #[must_use]
    pub fn get_player(&self, w: &World) -> Option<ObjectGuid> {
        player_manager::get_online_player(w, self.player_guid.full())
    }
}
