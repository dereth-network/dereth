// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/HouseCallback.cs
//! Port of `Source/ACE.Server/Entity/HouseCallback.cs`.

use empyrean_entity::ObjectGuid;

use crate::World;

/// An `Action<House>` run once, with the world.
pub type HouseAction = Box<dyn FnOnce(&mut World, ObjectGuid) + Send>;

// ACE: HouseCallback
pub struct HouseCallback {
    // ACE: HouseCallback.House
    pub house: ObjectGuid,
    // ACE: HouseCallback.Callback
    pub callback: HouseAction,
}

impl std::fmt::Debug for HouseCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HouseCallback")
            .field("house", &self.house)
            .finish_non_exhaustive()
    }
}

impl HouseCallback {
    // ACE: HouseCallback.HouseCallback
    #[must_use]
    pub fn new(house: ObjectGuid, callback: HouseAction) -> Self {
        HouseCallback { house, callback }
    }

    // ACE: HouseCallback.Run
    pub fn run(self, w: &mut World) {
        (self.callback)(w, self.house);
    }
}
