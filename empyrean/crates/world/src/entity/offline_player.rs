// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/OfflinePlayer.cs
//! Port of `Source/ACE.Server/Entity/OfflinePlayer.cs`.
//!
//! An `OfflinePlayer` owns its biota (ACE: "object property overrides that should have come from
//! the shard db"). `PlayerManager` holds every one of them, keyed by guid; [`super::i_player`]
//! resolves references to them.

use empyrean_common::dotnet::{CsCast, DotNetDateTime};
use empyrean_entity::enums::{
    PropertyBool, PropertyDataId, PropertyInstanceId, PropertyInt, PropertyInt64, PropertyString,
};
use empyrean_entity::models::PropertyKey;
use empyrean_entity::{Biota, ObjectGuid};
use empyrean_store::models::auth::Account;
use empyrean_store::{AuthDatabase, ShardDatabase};

use crate::World;

// ACE: OfflinePlayer
/// A player who is not in the world.
#[derive(Debug, Clone)]
pub struct OfflinePlayer {
    /// `Biota`. Change it only through [`OfflinePlayer::set_property`] and
    /// [`OfflinePlayer::remove_property`].
    pub biota: Biota,
    /// `Guid`: a wrapper around `Biota.Id`.
    pub guid: ObjectGuid,
    /// `Account`: `null` when the character stub or its account is missing.
    pub account: Option<Account>,
    /// `LastRequestedDatabaseSave`.
    pub last_requested_database_save: DotNetDateTime,
    /// `ChangesDetected`.
    pub changes_detected: bool,
    /// `Allegiance`: the `Allegiance` world object's guid (ACE holds the object); `AllegianceManager`
    /// sets it.
    pub allegiance: Option<ObjectGuid>,
    /// `AllegianceNode`: the guid of the allegiance whose tree holds this player's node (the node
    /// itself is `NodeRef { allegiance, player }`, in that allegiance's arena).
    pub allegiance_node: Option<ObjectGuid>,
}

impl OfflinePlayer {
    // ACE: OfflinePlayer.OfflinePlayer
    /// Restore a player from the database: its character stub names the account, which is read
    /// from the authentication database. `shard` is `DatabaseManager.Shard.BaseDatabase`.
    pub fn new(biota: Biota, shard: &mut dyn ShardDatabase, auth: &mut dyn AuthDatabase) -> Self {
        let guid = ObjectGuid::new(biota.id);

        let character = shard.get_character_stub_by_guid(guid.full());

        let account = character.and_then(|character| auth.get_account_by_id(character.account_id));

        Self {
            biota,
            guid,
            account,
            last_requested_database_save: DotNetDateTime::MIN_VALUE,
            changes_detected: false,
            allegiance: None,
            allegiance_node: None,
        }
    }

    // ACE: OfflinePlayer.IsDeleted
    /// Reads the character stub from the shard each time.
    ///
    /// # Panics
    /// When the stub is gone (ACE's `NullReferenceException`).
    #[must_use]
    pub fn is_deleted(&self, w: &World) -> bool {
        w.shard
            .base_database()
            .get_character_stub_by_guid(self.guid.full())
            .expect("Object reference not set to an instance of an object.")
            .is_deleted
    }

    // ACE: OfflinePlayer.IsPendingDeletion
    /// # Panics
    /// When the stub is gone (ACE's `NullReferenceException`).
    #[must_use]
    pub fn is_pending_deletion(&self, w: &World) -> bool {
        let delete_time = w
            .shard
            .base_database()
            .get_character_stub_by_guid(self.guid.full())
            .expect("Object reference not set to an instance of an object.")
            .delete_time;
        delete_time > 0 && !self.is_deleted(w)
    }

    // ACE: OfflinePlayer.SaveBiotaToDatabase
    /// Sets `LastRequestedDatabaseSave` to `now` and clears `ChangesDetected`. With `enqueue_save`
    /// it returns the snapshot for `DatabaseManager.Shard.SaveBiota` (the caller hands it to
    /// `w.shard`); without, nothing (bulk saves collect the biotas themselves).
    pub fn save_biota_to_database(
        &mut self,
        now: DotNetDateTime,
        enqueue_save: bool,
    ) -> Option<Biota> {
        self.last_requested_database_save = now;
        self.changes_detected = false;

        enqueue_save.then(|| self.biota.clone())
    }

    // ACE: OfflinePlayer.GetProperty
    #[must_use]
    pub fn get_property<K: PropertyKey>(&self, property: K) -> Option<K::Value> {
        self.biota.get_property(property)
    }

    // ACE: OfflinePlayer.SetProperty
    pub fn set_property<K: PropertyKey>(&mut self, property: K, value: K::Value) {
        let changed = self.biota.set_property(property, value);
        if changed {
            self.changes_detected = true;
        }
    }

    // ACE: OfflinePlayer.RemoveProperty
    pub fn remove_property<K: PropertyKey>(&mut self, property: K) {
        if self.biota.try_remove_property(property) {
            self.changes_detected = true;
        }
    }

    // ACE: OfflinePlayer.Name
    /// `GetProperty(PropertyString.Name)`.
    #[must_use]
    pub fn name(&self) -> Option<String> {
        self.get_property(PropertyString::Name)
    }

    // ACE: OfflinePlayer.Level
    #[must_use]
    pub fn level(&self) -> Option<i32> {
        self.get_property(PropertyInt::Level)
    }

    // ACE: OfflinePlayer.Heritage
    #[must_use]
    pub fn heritage(&self) -> Option<i32> {
        self.get_property(PropertyInt::HeritageGroup)
    }

    // ACE: OfflinePlayer.Gender
    #[must_use]
    pub fn gender(&self) -> Option<i32> {
        self.get_property(PropertyInt::Gender)
    }

    // ACE: OfflinePlayer.MonarchId
    #[must_use]
    pub fn monarch_id(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::Monarch)
    }

    /// `MonarchId` (set).
    pub fn set_monarch_id(&mut self, value: Option<u32>) {
        self.set_or_remove(PropertyInstanceId::Monarch, value);
    }

    // ACE: OfflinePlayer.PatronId
    #[must_use]
    pub fn patron_id(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::Patron)
    }

    /// `PatronId` (set).
    pub fn set_patron_id(&mut self, value: Option<u32>) {
        self.set_or_remove(PropertyInstanceId::Patron, value);
    }

    // ACE: OfflinePlayer.AllegianceXPCached
    /// `(ulong)(GetProperty(PropertyInt64.AllegianceXPCached) ?? 0)`.
    #[must_use]
    pub fn allegiance_xp_cached(&self) -> u64 {
        self.get_property(PropertyInt64::AllegianceXPCached)
            .unwrap_or(0)
            .cs_cast()
    }

    /// `AllegianceXPCached` (set): 0 removes the property.
    pub fn set_allegiance_xp_cached(&mut self, value: u64) {
        if value == 0 {
            self.remove_property(PropertyInt64::AllegianceXPCached);
        } else {
            self.set_property(PropertyInt64::AllegianceXPCached, value.cs_cast());
        }
    }

    // ACE: OfflinePlayer.AllegianceXPGenerated
    #[must_use]
    pub fn allegiance_xp_generated(&self) -> u64 {
        self.get_property(PropertyInt64::AllegianceXPGenerated)
            .unwrap_or(0)
            .cs_cast()
    }

    /// `AllegianceXPGenerated` (set): 0 removes the property.
    pub fn set_allegiance_xp_generated(&mut self, value: u64) {
        if value == 0 {
            self.remove_property(PropertyInt64::AllegianceXPGenerated);
        } else {
            self.set_property(PropertyInt64::AllegianceXPGenerated, value.cs_cast());
        }
    }

    // ACE: OfflinePlayer.AllegianceRank
    #[must_use]
    pub fn allegiance_rank(&self) -> Option<i32> {
        self.get_property(PropertyInt::AllegianceRank)
    }

    /// `AllegianceRank` (set).
    pub fn set_allegiance_rank(&mut self, value: Option<i32>) {
        self.set_or_remove(PropertyInt::AllegianceRank, value);
    }

    // ACE: OfflinePlayer.AllegianceOfficerRank
    #[must_use]
    pub fn allegiance_officer_rank(&self) -> Option<i32> {
        self.get_property(PropertyInt::AllegianceOfficerRank)
    }

    /// `AllegianceOfficerRank` (set).
    pub fn set_allegiance_officer_rank(&mut self, value: Option<i32>) {
        self.set_or_remove(PropertyInt::AllegianceOfficerRank, value);
    }

    // ACE: OfflinePlayer.ExistedBeforeAllegianceXpChanges
    /// This flag indicates if a player can pass up allegiance XP (true when unset).
    #[must_use]
    pub fn existed_before_allegiance_xp_changes(&self) -> bool {
        self.get_property(PropertyBool::ExistedBeforeAllegianceXpChanges)
            .unwrap_or(true)
    }

    /// `ExistedBeforeAllegianceXpChanges` (set): true removes the property.
    pub fn set_existed_before_allegiance_xp_changes(&mut self, value: bool) {
        if value {
            self.remove_property(PropertyBool::ExistedBeforeAllegianceXpChanges);
        } else {
            self.set_property(PropertyBool::ExistedBeforeAllegianceXpChanges, value);
        }
    }

    // ACE: OfflinePlayer.HouseInstance
    /// Used for allegiance recall to monarch's mansion / villa.
    #[must_use]
    pub fn house_instance(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::House)
    }

    /// `HouseInstance` (set).
    pub fn set_house_instance(&mut self, value: Option<u32>) {
        self.set_or_remove(PropertyInstanceId::House, value);
    }

    // ACE: OfflinePlayer.HousePurchaseTimestamp
    #[must_use]
    pub fn house_purchase_timestamp(&self) -> Option<i32> {
        self.get_property(PropertyInt::HousePurchaseTimestamp)
    }

    /// `HousePurchaseTimestamp` (set).
    pub fn set_house_purchase_timestamp(&mut self, value: Option<i32>) {
        self.set_or_remove(PropertyInt::HousePurchaseTimestamp, value);
    }

    // ACE: OfflinePlayer.HouseRentTimestamp
    #[must_use]
    pub fn house_rent_timestamp(&self) -> Option<i32> {
        self.get_property(PropertyInt::HouseRentTimestamp)
    }

    /// `HouseRentTimestamp` (set).
    pub fn set_house_rent_timestamp(&mut self, value: Option<i32>) {
        self.set_or_remove(PropertyInt::HouseRentTimestamp, value);
    }

    // ACE: OfflinePlayer.HouseId
    #[must_use]
    pub fn house_id(&self) -> Option<u32> {
        self.get_property(PropertyDataId::HouseId)
    }

    /// `HouseId` (set).
    pub fn set_house_id(&mut self, value: Option<u32>) {
        self.set_or_remove(PropertyDataId::HouseId, value);
    }

    // ACE: OfflinePlayer.GetCurrentLoyalty
    /// `(uint?)GetProperty(PropertyInt.CurrentLoyaltyAtLastLogoff) ?? 0`.
    #[must_use]
    pub fn get_current_loyalty(&self) -> u32 {
        self.get_property(PropertyInt::CurrentLoyaltyAtLastLogoff)
            .map_or(0, CsCast::cs_cast)
    }

    // ACE: OfflinePlayer.GetCurrentLeadership
    #[must_use]
    pub fn get_current_leadership(&self) -> u32 {
        self.get_property(PropertyInt::CurrentLeadershipAtLastLogoff)
            .map_or(0, CsCast::cs_cast)
    }

    // ACE: OfflinePlayer.UpdateProperty
    /// `broadcast` is unused, as in ACE.
    pub fn update_property(
        &mut self,
        prop: PropertyInstanceId,
        value: Option<u32>,
        _broadcast: bool,
    ) {
        match value {
            Some(v) => self.set_property(prop, v),
            None => self.remove_property(prop),
        }
    }

    /// A C# nullable property setter: `null` removes, a value sets.
    fn set_or_remove<K: PropertyKey>(&mut self, property: K, value: Option<K::Value>) {
        match value {
            Some(v) => self.set_property(property, v),
            None => self.remove_property(property),
        }
    }
}
