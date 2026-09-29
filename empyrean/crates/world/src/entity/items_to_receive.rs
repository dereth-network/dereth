// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/ItemsToReceive.cs
//! Port of `Source/ACE.Server/Entity/ItemsToReceive.cs`.
//!
//! Helper class to verify player has enough free inventory slots / container slots / burden to
//! receive some items.

use empyrean_entity::enums::PropertyInt;
use empyrean_entity::ObjectGuid;

use crate::world_objects::{container, player_inventory};
use crate::World;

// ACE: ItemsToReceive
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemsToReceive {
    /// The player to receive the items.
    // ACE: ItemsToReceive.player
    pub player: ObjectGuid,
    // ACE: ItemsToReceive.RequiredInventorySlots
    pub required_inventory_slots: i32,
    // ACE: ItemsToReceive.RequiredContainerSlots
    pub required_container_slots: i32,
    // ACE: ItemsToReceive.RequiredBurden
    pub required_burden: i32,
    // ACE: ItemsToReceive.playerFreeInventorySlots
    player_free_inventory_slots: i32,
    // ACE: ItemsToReceive.playerFreeContainerSlots
    player_free_container_slots: i32,
    // ACE: ItemsToReceive.playerAvailableBurden
    player_available_burden: i32,
}

impl ItemsToReceive {
    /// # Panics
    /// When the player is gone (ACE's `NullReferenceException`).
    // ACE: ItemsToReceive.ItemsToReceive
    #[must_use]
    pub fn new(w: &World, player: ObjectGuid) -> Self {
        let o = w
            .objects
            .get(player)
            .expect("System.NullReferenceException: player");
        Self {
            player,
            player_free_inventory_slots: container::get_free_inventory_slots(w, player, true),
            player_free_container_slots: container::get_free_container_slots(w, player),
            player_available_burden: player_inventory::get_available_burden(w, o),
            ..Self::default()
        }
    }

    /// The total amount of items needing to be created.
    // ACE: ItemsToReceive.RequiredSlots
    #[must_use]
    pub fn required_slots(&self) -> i32 {
        self.required_container_slots
            .wrapping_add(self.required_inventory_slots)
    }

    // ACE: ItemsToReceive.PlayerOutOfInventorySlots
    #[must_use]
    pub fn player_out_of_inventory_slots(&self) -> bool {
        self.required_inventory_slots > self.player_free_inventory_slots
    }

    // ACE: ItemsToReceive.PlayerOutOfContainerSlots
    #[must_use]
    pub fn player_out_of_container_slots(&self) -> bool {
        self.required_container_slots > self.player_free_container_slots
    }

    // ACE: ItemsToReceive.PlayerExceedsAvailableBurden
    #[must_use]
    pub fn player_exceeds_available_burden(&self) -> bool {
        self.required_burden > self.player_available_burden
    }

    // ACE: ItemsToReceive.PlayerExceedsLimits
    #[must_use]
    pub fn player_exceeds_limits(&self) -> bool {
        self.player_out_of_inventory_slots()
            || self.player_out_of_container_slots()
            || self.player_exceeds_available_burden()
    }

    // ACE: ItemsToReceive.Add
    pub fn add(&mut self, w: &World, weenie_class_id: u32, amount: i32) -> bool {
        self.process(w, weenie_class_id, amount, false)
    }

    // ACE: ItemsToReceive.Remove
    pub fn remove(&mut self, w: &World, weenie_class_id: u32, amount: i32) -> bool {
        self.process(w, weenie_class_id, amount, true)
    }

    /// ACE's default: `negate = false`.
    // ACE: ItemsToReceive.Process
    fn process(&mut self, w: &World, weenie_class_id: u32, amount: i32, negate: bool) -> bool {
        let (mut required_slots, mut required_encumbrance, item_requires_backpack_slot) =
            Self::get_item_slot_and_burden_requirements(w, weenie_class_id, amount);

        if negate {
            required_slots = required_slots.wrapping_mul(-1);
            required_encumbrance = required_encumbrance.wrapping_mul(-1);
        }

        self.required_burden = self.required_burden.wrapping_add(required_encumbrance);

        if item_requires_backpack_slot {
            self.required_container_slots =
                self.required_container_slots.wrapping_add(required_slots);
        } else {
            self.required_inventory_slots =
                self.required_inventory_slots.wrapping_add(required_slots);
        }

        !self.player_exceeds_limits()
    }

    /// Returns the number of slots required for the items, with ACE's `out requiredEncumbrance`
    /// and `out requiresBackpackSlot`.
    ///
    /// # Panics
    /// On a stackable weenie whose max stack size is 0 (ACE's `DivideByZeroException`).
    // ACE: ItemsToReceive.GetItemSlotAndBurdenRequirements
    fn get_item_slot_and_burden_requirements(
        w: &World,
        weenie_class_id: u32,
        amount: i32,
    ) -> (i32, i32, bool) {
        let item = w.content.get_cached_weenie(weenie_class_id);

        let Some(item) = item.filter(|i| !i.is_vendor_service()) else {
            return (0, 0, false);
        };

        let requires_backpack_slot = item.requires_backpack_slot_or_is_container();

        if !item.is_stackable() {
            // `amount * item.GetProperty(PropertyInt.EncumbranceVal) ?? 0`: the `??` covers the product.
            let required_encumbrance = item
                .get_property(PropertyInt::EncumbranceVal)
                .map_or(0, |e: i32| amount.wrapping_mul(e));
            return (amount, required_encumbrance, requires_backpack_slot);
        }

        let item_stack_unit_encumbrance = item.get_stack_unit_encumbrance();

        let item_max_stack_size = item.get_max_stack_size();

        let required_encumbrance = amount.wrapping_mul(item_stack_unit_encumbrance);

        let mut item_stacks = amount
            .checked_div(item_max_stack_size)
            .expect("System.DivideByZeroException");

        if amount % item_max_stack_size > 0 {
            item_stacks = item_stacks.wrapping_add(1);
        }

        (item_stacks, required_encumbrance, requires_backpack_slot)
    }
}
