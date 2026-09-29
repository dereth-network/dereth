// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/IPlayer.cs
//! Port of `Source/ACE.Server/Entity/IPlayer.cs`.
//!
//! ACE's `IPlayer` is the interface `Player` and `OfflinePlayer` share, so that `PlayerManager` can
//! keep online and offline players in one name index and return either. Here it is a reference
//! ([`IPlayer`]) that names which of the two it is; the interface members are free functions that
//! resolve it: an online player is a `WorldObject` in `w.objects`, an offline one lives in
//! `w.player_manager`.
//!
//! For an online player the property-backed members (`Name`, `Level`, `MonarchId`, ...) read and
//! write the `WorldObject`'s properties, which is what ACE's `Player` wrappers do. The members
//! `Player` implements from its own state (`Account`, `IsDeleted`, `SaveBiotaToDatabase`) use the
//! player's `PlayerManager` entry or a pointer to `Player`.

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{
    PropertyBool, PropertyDataId, PropertyInstanceId, PropertyInt, PropertyInt64, PropertyString,
};
use empyrean_entity::ObjectGuid;
use empyrean_store::models::auth::Account;

use crate::world_objects::world_object_properties::WoPropertyKey;
use crate::World;

// ACE: IPlayer
/// A player, online or offline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IPlayer {
    /// A `Player`: an object in `w.objects` listed in `PlayerManager.onlinePlayers`.
    Online(ObjectGuid),
    /// An `OfflinePlayer` in `PlayerManager.offlinePlayers`.
    Offline(ObjectGuid),
}

impl IPlayer {
    /// `IPlayer.Guid`.
    #[must_use]
    pub const fn guid(self) -> ObjectGuid {
        match self {
            Self::Online(g) | Self::Offline(g) => g,
        }
    }

    /// `player is Player`.
    #[must_use]
    pub const fn is_online(self) -> bool {
        matches!(self, Self::Online(_))
    }
}

/// `IPlayer.Account`: `Player.Account` for an online player (kept on its `PlayerManager` entry
/// until `Player.Account` is ported), `OfflinePlayer.Account` otherwise.
#[must_use]
pub fn account(w: &World, p: IPlayer) -> Option<Account> {
    match p {
        IPlayer::Online(g) => w
            .player_manager
            .online_players
            .get(&g.full())
            .and_then(|e| e.account.clone()),
        IPlayer::Offline(g) => w
            .player_manager
            .offline_players
            .get(&g.full())
            .and_then(|o| o.account.clone()),
    }
}

/// `IPlayer.GetProperty(..)`, every overload.
#[must_use]
pub fn get_property<K: WoPropertyKey>(w: &World, p: IPlayer, property: K) -> Option<K::Value> {
    match p {
        IPlayer::Online(g) => w.objects.get(g).and_then(|o| o.get_property(property)),
        IPlayer::Offline(g) => w
            .player_manager
            .offline_players
            .get(&g.full())
            .and_then(|o| o.get_property(property)),
    }
}

/// `IPlayer.SetProperty(..)`, every overload.
pub fn set_property<K: WoPropertyKey>(w: &mut World, p: IPlayer, property: K, value: K::Value) {
    match p {
        IPlayer::Online(g) => {
            if let Some(o) = w.objects.get_mut(g) {
                o.set_property(property, value);
            }
        }
        IPlayer::Offline(g) => {
            if let Some(o) = w.player_manager.offline_players.get_mut(&g.full()) {
                o.set_property(property, value);
            }
        }
    }
}

/// `IPlayer.RemoveProperty(..)`, every overload.
pub fn remove_property<K: WoPropertyKey>(w: &mut World, p: IPlayer, property: K) {
    match p {
        IPlayer::Online(g) => {
            if let Some(o) = w.objects.get_mut(g) {
                o.remove_property(property);
            }
        }
        IPlayer::Offline(g) => {
            if let Some(o) = w.player_manager.offline_players.get_mut(&g.full()) {
                o.remove_property(property);
            }
        }
    }
}

/// `IPlayer.Name`.
#[must_use]
pub fn name(w: &World, p: IPlayer) -> Option<String> {
    get_property(w, p, PropertyString::Name)
}

/// `IPlayer.Level`.
#[must_use]
pub fn level(w: &World, p: IPlayer) -> Option<i32> {
    get_property(w, p, PropertyInt::Level)
}

/// `IPlayer.Heritage`.
#[must_use]
pub fn heritage(w: &World, p: IPlayer) -> Option<i32> {
    get_property(w, p, PropertyInt::HeritageGroup)
}

/// `IPlayer.Gender`.
#[must_use]
pub fn gender(w: &World, p: IPlayer) -> Option<i32> {
    get_property(w, p, PropertyInt::Gender)
}

/// `IPlayer.IsDeleted`: `OfflinePlayer.IsDeleted`, or `Player.IsDeleted` (`Character.IsDeleted`;
/// a pointer until `Player.Character` is ported, answering `false`).
#[must_use]
pub fn is_deleted(w: &World, p: IPlayer) -> bool {
    match p {
        IPlayer::Online(g) => crate::world_objects::player::is_deleted(w, g),
        IPlayer::Offline(g) => w
            .player_manager
            .offline_players
            .get(&g.full())
            .is_some_and(|o| o.is_deleted(w)),
    }
}

/// `IPlayer.IsPendingDeletion`; see [`is_deleted`].
#[must_use]
pub fn is_pending_deletion(w: &World, p: IPlayer) -> bool {
    match p {
        IPlayer::Online(g) => crate::world_objects::player::is_pending_deletion(w, g),
        IPlayer::Offline(g) => w
            .player_manager
            .offline_players
            .get(&g.full())
            .is_some_and(|o| o.is_pending_deletion(w)),
    }
}

/// `IPlayer.MonarchId` (get).
#[must_use]
pub fn monarch_id(w: &World, p: IPlayer) -> Option<u32> {
    get_property(w, p, PropertyInstanceId::Monarch)
}

/// `IPlayer.MonarchId` (set).
pub fn set_monarch_id(w: &mut World, p: IPlayer, value: Option<u32>) {
    set_or_remove(w, p, PropertyInstanceId::Monarch, value);
}

/// `IPlayer.PatronId` (get).
#[must_use]
pub fn patron_id(w: &World, p: IPlayer) -> Option<u32> {
    get_property(w, p, PropertyInstanceId::Patron)
}

/// `IPlayer.PatronId` (set).
pub fn set_patron_id(w: &mut World, p: IPlayer, value: Option<u32>) {
    set_or_remove(w, p, PropertyInstanceId::Patron, value);
}

/// `IPlayer.AllegianceRank` (get).
#[must_use]
pub fn allegiance_rank(w: &World, p: IPlayer) -> Option<i32> {
    get_property(w, p, PropertyInt::AllegianceRank)
}

/// `IPlayer.AllegianceRank` (set).
pub fn set_allegiance_rank(w: &mut World, p: IPlayer, value: Option<i32>) {
    set_or_remove(w, p, PropertyInt::AllegianceRank, value);
}

/// `IPlayer.HouseId` (get).
#[must_use]
pub fn house_id(w: &World, p: IPlayer) -> Option<u32> {
    get_property(w, p, PropertyDataId::HouseId)
}

/// `IPlayer.HouseInstance` (get).
#[must_use]
pub fn house_instance(w: &World, p: IPlayer) -> Option<u32> {
    get_property(w, p, PropertyInstanceId::House)
}

/// `IPlayer.AllegianceXPCached` (get): `(ulong)(GetProperty(PropertyInt64.AllegianceXPCached) ?? 0)`.
#[must_use]
pub fn allegiance_xp_cached(w: &World, p: IPlayer) -> u64 {
    get_property(w, p, PropertyInt64::AllegianceXPCached)
        .unwrap_or(0)
        .cs_cast()
}

/// `IPlayer.ExistedBeforeAllegianceXpChanges` (get): true when unset.
#[must_use]
pub fn existed_before_allegiance_xp_changes(w: &World, p: IPlayer) -> bool {
    get_property(w, p, PropertyBool::ExistedBeforeAllegianceXpChanges).unwrap_or(true)
}

/// `IPlayer.SaveBiotaToDatabase(enqueueSave)`: `OfflinePlayer`'s, or `Player`'s (WorldObject's).
pub fn save_biota_to_database(w: &mut World, p: IPlayer, enqueue_save: bool) {
    match p {
        IPlayer::Online(g) => {
            crate::dispatch::save_biota_to_database::save_biota_to_database(w, g, enqueue_save)
        }
        IPlayer::Offline(g) => {
            let now = w.now.utc;
            let snapshot = w
                .player_manager
                .offline_players
                .get_mut(&g.full())
                .and_then(|o| o.save_biota_to_database(now, enqueue_save));
            if let Some(biota) = snapshot {
                w.shard.save_biota(biota, None);
            }
        }
    }
}

/// A C# nullable property setter: `null` removes, a value sets.
fn set_or_remove<K: WoPropertyKey>(
    w: &mut World,
    p: IPlayer,
    property: K,
    value: Option<K::Value>,
) {
    match value {
        Some(v) => set_property(w, p, property, v),
        None => remove_property(w, p, property),
    }
}
