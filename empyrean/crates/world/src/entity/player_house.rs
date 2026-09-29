// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/PlayerHouse.cs
//! Port of `Source/ACE.Server/Entity/PlayerHouse.cs`.

use std::cmp::Ordering;

use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_entity::enums::PropertyInt;
use empyrean_entity::ObjectGuid;

use crate::entity::i_player::{self, IPlayer};
use crate::World;

// ACE: PlayerHouse
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerHouse {
    // ACE: PlayerHouse.AccountId
    pub account_id: u32,
    // ACE: PlayerHouse.PlayerGuid
    pub player_guid: u32,
    // ACE: PlayerHouse.PlayerName
    pub player_name: Option<String>,
    // ACE: PlayerHouse.House
    pub house: ObjectGuid,
    // ACE: PlayerHouse.RentDue
    pub rent_due: DotNetDateTime,
}

impl PlayerHouse {
    /// # Panics
    /// When the player has no `HouseRentTimestamp` (`HouseRentTimestamp.Value`).
    // ACE: PlayerHouse.PlayerHouse
    #[must_use]
    pub fn new(w: &World, player: IPlayer, house: ObjectGuid, house_name: &str) -> Self {
        let player_name = i_player::name(w, player);
        let account_id = match i_player::account(w, player) {
            Some(account) => account.account_id,
            None => {
                // `Console.WriteLine`
                log::info!(
                    "PlayerHouse({}, {} ({})) - couldn't find account id",
                    player_name.clone().unwrap_or_default(),
                    house_name,
                    house
                );
                0
            }
        };

        let rent = i_player::get_property(w, player, PropertyInt::HouseRentTimestamp)
            .expect("System.InvalidOperationException: Nullable object must have a value (HouseRentTimestamp)");

        PlayerHouse {
            account_id,
            player_guid: player.guid().full(),
            player_name,
            house,
            // `DateTimeOffset.FromUnixTimeSeconds(...).UtcDateTime`
            rent_due: DotNetDateTime::UNIX_EPOCH.add_seconds(f64::from(rent)),
        }
    }

    // ACE: PlayerHouse.CompareTo
    #[must_use]
    pub fn compare_to(&self, player_house: &PlayerHouse) -> Ordering {
        let result = self.rent_due.cmp(&player_house.rent_due);

        if result == Ordering::Equal {
            return self.house.full().cmp(&player_house.house.full());
        }

        result
    }

    /// The `SortedSet` key: `CompareTo`'s two fields.
    #[must_use]
    pub fn key(&self) -> (DotNetDateTime, u32) {
        (self.rent_due, self.house.full())
    }
}
