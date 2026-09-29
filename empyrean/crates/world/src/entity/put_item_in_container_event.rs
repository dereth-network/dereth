// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/PutItemInContainerEvent.cs
//! Port of `Source/ACE.Server/Entity/PutItemInContainerEvent.cs`.
//!
//! This handles a peculiar sequence sent by the client in certain scenarios: the client will
//! double-send 0x19 PutItemInContainer for the same object (swapping dual wield weapons, swapping
//! ammo types in combat). `DateTime.UtcNow` is the world's clock, passed in (`w.now.utc`).

use empyrean_common::dotnet::datetime::{DotNetDateTime, TimeSpan};

// ACE: PutItemInContainerEvent
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PutItemInContainerEvent {
    // ACE: PutItemInContainerEvent.ItemGuid
    pub item_guid: u32,
    // ACE: PutItemInContainerEvent.ContainerGuid
    pub container_guid: u32,
    // ACE: PutItemInContainerEvent.Placement
    pub placement: i32,
    // ACE: PutItemInContainerEvent.Timestamp
    pub timestamp: DotNetDateTime,
}

impl PutItemInContainerEvent {
    /// `public static TimeSpan Threshold = TimeSpan.FromSeconds(0.5f)` (nothing reassigns it).
    // ACE: PutItemInContainerEvent.Threshold
    #[must_use]
    pub fn threshold() -> TimeSpan {
        TimeSpan::from_seconds(f64::from(0.5f32))
    }

    /// `Timestamp = DateTime.UtcNow` is `now`.
    // ACE: PutItemInContainerEvent.PutItemInContainerEvent
    #[must_use]
    pub fn new(item_guid: u32, container_guid: u32, placement: i32, now: DotNetDateTime) -> Self {
        Self {
            item_guid,
            container_guid,
            placement,
            timestamp: now,
        }
    }

    // ACE: PutItemInContainerEvent.IsDoubleSend
    #[must_use]
    pub fn is_double_send(&self, data: &PutItemInContainerEvent, now: DotNetDateTime) -> bool {
        self.item_guid == data.item_guid
            && self.container_guid == data.container_guid
            && self.placement == data.placement
            && now - self.timestamp < Self::threshold()
            && self.timestamp - data.timestamp < Self::threshold()
    }
}
