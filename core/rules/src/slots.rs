//! `INVENTORY_LOC` and the client's 24 inventory slots.
//!
//! The composites matter, so the enum is reproduced in full.

use dereth_primitives::ObjectId;

/// `INVENTORY_LOC` — a 31-bit location mask.
pub mod loc {
    pub const NONE: u32 = 0x0000_0000;
    pub const HEAD_WEAR: u32 = 0x0000_0001;
    pub const CHEST_WEAR: u32 = 0x0000_0002;
    pub const ABDOMEN_WEAR: u32 = 0x0000_0004;
    pub const UPPER_ARM_WEAR: u32 = 0x0000_0008;
    pub const LOWER_ARM_WEAR: u32 = 0x0000_0010;
    pub const HAND_WEAR: u32 = 0x0000_0020;
    pub const UPPER_LEG_WEAR: u32 = 0x0000_0040;
    pub const LOWER_LEG_WEAR: u32 = 0x0000_0080;
    pub const FOOT_WEAR: u32 = 0x0000_0100;
    pub const CHEST_ARMOR: u32 = 0x0000_0200;
    pub const ABDOMEN_ARMOR: u32 = 0x0000_0400;
    pub const UPPER_ARM_ARMOR: u32 = 0x0000_0800;
    pub const LOWER_ARM_ARMOR: u32 = 0x0000_1000;
    pub const UPPER_LEG_ARMOR: u32 = 0x0000_2000;
    pub const LOWER_LEG_ARMOR: u32 = 0x0000_4000;
    /// All six armour bits.
    pub const ARMOR: u32 = 0x0000_7E00;
    pub const NECK_WEAR: u32 = 0x0000_8000;
    pub const WRIST_WEAR_LEFT: u32 = 0x0001_0000;
    pub const WRIST_WEAR_RIGHT: u32 = 0x0002_0000;
    pub const WRIST_WEAR: u32 = 0x0003_0000;
    pub const FINGER_WEAR_LEFT: u32 = 0x0004_0000;
    pub const FINGER_WEAR_RIGHT: u32 = 0x0008_0000;
    pub const FINGER_WEAR: u32 = 0x000C_0000;
    pub const MELEE_WEAPON: u32 = 0x0010_0000;
    pub const SHIELD: u32 = 0x0020_0000;
    pub const MISSILE_WEAPON: u32 = 0x0040_0000;
    pub const MISSILE_AMMO: u32 = 0x0080_0000;
    pub const HELD: u32 = 0x0100_0000;
    pub const TWO_HANDED: u32 = 0x0200_0000;
    pub const WEAPON: u32 = 0x0250_0000;
    pub const WEAPON_READY_SLOT: u32 = 0x0350_0000;
    pub const READY_SLOT: u32 = 0x03F0_0000;
    pub const TRINKET_ONE: u32 = 0x0400_0000;
    pub const CLOAK: u32 = 0x0800_0000;
    pub const CLOTHING: u32 = 0x0800_01FF;
    pub const SIGIL_ONE: u32 = 0x1000_0000;
    pub const SIGIL_TWO: u32 = 0x2000_0000;
    pub const SIGIL_THREE: u32 = 0x4000_0000;
    pub const SIGIL: u32 = 0x7000_0000;
    pub const JEWELRY: u32 = 0x7C0F_8000;
    /// Every location bit; the same value as the can-go-in-ready-slot mask.
    pub const ALL: u32 = 0x7FFF_FFFF;

    /// The "wearable" mask, tested by the auto-wear predicate and by
    /// the auto-sort. An item with any of these bits is *worn*, not wielded.
    pub const WEARABLE: u32 = 0x0800_7FFF;
    /// The "wieldable" mask, tested by `AutoSort`'s wield branch.
    pub const WIELDABLE: u32 = 0x7EFF_8000;
}

/// The slot side — disambiguates the paired wrist/finger locations when an item's
/// `_valid_locations` names both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlotSide {
    #[default]
    Null = 0,
    Left = 1,
    Right = 2,
}

/// One inventory slot record: the item in it and the slot's own location mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InvSlotInfo {
    pub item: ObjectId,
    pub inv_loc: u32,
}

/// The 24 named slots, in the order the slot module's constructor initialises them.
///
/// The *order* is the constructor's, which is the order the paperdoll's slot array is laid out
/// in.
pub const SLOT_MASKS: [(&str, u32); 24] = [
    ("neckSlot", loc::NECK_WEAR),
    ("headSlot", loc::HEAD_WEAR),
    ("leftWristSlot", loc::WRIST_WEAR_LEFT),
    ("chestSlot", loc::CHEST_ARMOR),
    ("leftRingSlot", loc::FINGER_WEAR_LEFT),
    ("abdomenSlot", loc::ABDOMEN_ARMOR),
    ("rightWristSlot", loc::WRIST_WEAR_RIGHT),
    ("upperArmSlot", loc::UPPER_ARM_ARMOR),
    ("rightRingSlot", loc::FINGER_WEAR_RIGHT),
    ("lowerArmSlot", loc::LOWER_ARM_ARMOR),
    ("weaponReadySlot", loc::WEAPON_READY_SLOT),
    ("handSlot", loc::HAND_WEAR),
    ("ammoReadySlot", loc::MISSILE_AMMO),
    ("upperLegSlot", loc::UPPER_LEG_ARMOR),
    ("shieldReadySlot", 0x0030_0000),
    ("lowerLegSlot", loc::LOWER_LEG_ARMOR),
    ("clothesPantsSlot", loc::UPPER_LEG_WEAR),
    ("footSlot", loc::FOOT_WEAR),
    ("clothesShirtSlot", loc::CHEST_WEAR),
    ("trinketOneSlot", loc::TRINKET_ONE),
    ("cloakSlot", loc::CLOAK),
    ("sigilOneSlot", loc::SIGIL_ONE),
    ("sigilTwoSlot", loc::SIGIL_TWO),
    ("sigilThreeSlot", loc::SIGIL_THREE),
];

/// The twenty-four `if ((loc & <literal>) != 0)` tests of the paper doll's
/// set-item-into-location, in [`SLOT_MASKS`]' order.
///
/// This is *not* the same table as [`SLOT_MASKS`], which is each slot's `inv_loc` — the value
/// the slot module's constructor stores in each slot record and the value the auto-wield
/// walk compares against the item's inventory mask. The two agree everywhere except the shield
/// slot: `inv_loc` is `0x00300000` and the write test is the bare `0x00200000`, between the
/// weapon slot's `0x3500000` test and the ammunition slot's bit-23 test. Nothing in the client
/// derives one from the other, so neither constant is derived here.
///
/// The neck slot's test is bit 15 — `NECK_WEAR`.
pub const SET_LOCATION_MASKS: [u32; 24] = [
    loc::NECK_WEAR,
    loc::HEAD_WEAR,
    loc::WRIST_WEAR_LEFT,
    loc::CHEST_ARMOR,
    loc::FINGER_WEAR_LEFT,
    loc::ABDOMEN_ARMOR,
    loc::WRIST_WEAR_RIGHT,
    loc::UPPER_ARM_ARMOR,
    loc::FINGER_WEAR_RIGHT,
    loc::LOWER_ARM_ARMOR,
    loc::WEAPON_READY_SLOT,
    loc::HAND_WEAR,
    loc::MISSILE_AMMO,
    loc::UPPER_LEG_ARMOR,
    // The shield bit alone, where `inv_loc` is `0x00300000`.
    loc::SHIELD,
    loc::LOWER_LEG_ARMOR,
    loc::UPPER_LEG_WEAR,
    loc::FOOT_WEAR,
    loc::CHEST_WEAR,
    loc::TRINKET_ONE,
    loc::CLOAK,
    loc::SIGIL_ONE,
    loc::SIGIL_TWO,
    loc::SIGIL_THREE,
];

/// The client's slot module — 24 slot records.
#[derive(Debug, Clone, Default)]
pub struct InvSlotModule {
    pub slots: [InvSlotInfo; 24],
}

impl InvSlotModule {
    #[must_use]
    pub fn new() -> Self {
        let mut m = Self::default();
        for (i, (_, mask)) in SLOT_MASKS.iter().enumerate() {
            m.slots[i].inv_loc = *mask;
        }
        m
    }

    /// Behavior: clears every slot's item, keeping the masks.
    pub fn reset(&mut self) {
        for s in &mut self.slots {
            s.item = ObjectId(0);
        }
    }

    #[must_use]
    pub fn by_name(&self, name: &str) -> Option<&InvSlotInfo> {
        let i = SLOT_MASKS.iter().position(|(n, _)| *n == name)?;
        Some(&self.slots[i])
    }

    /// The item in the slot whose mask intersects `loc`, if any.
    #[must_use]
    pub fn item_at(&self, loc_mask: u32) -> Option<ObjectId> {
        self.slots
            .iter()
            .find(|s| s.inv_loc & loc_mask != 0 && s.item.0 != 0)
            .map(|s| s.item)
    }

    pub fn set(&mut self, loc_mask: u32, item: ObjectId) {
        if let Some(s) = self.slots.iter_mut().find(|s| s.inv_loc & loc_mask != 0) {
            s.item = item;
        }
    }

    /// The paper doll's item-into-location write — **every** slot the mask
    /// names, not the first.
    ///
    /// Twenty-four bit tests in a row, each writing `item` into its slot when the mask has that
    /// bit, so a composite mask writes several slots and `set_into_location(0x7FFFFFFF, 0)` clears the doll — which is
    /// exactly what the doll's inventory rebuild does before it replays the placement
    /// list. [`Self::set`] stops at the first match and is kept for the callers that hand it a
    /// single bit.
    ///
    /// Returns the number of slots written, so a caller can assert it wrote what it meant to.
    ///
    /// **The test mask is the function's own literal, not `inv_loc`.** They agree
    /// for twenty-three of the twenty-four slots and differ for the shield, whose `inv_loc` is
    /// `0x00300000` (the slot module's constructor) while the set-item-into-location
    /// tests `if ((loc & 0x200000) != 0)`. Reading `inv_loc` here would put a
    /// one-handed melee weapon wielded in the **main hand** (`_location == 0x00100000`) into
    /// the shield slot as well as the weapon slot — so the shield slot's item, which
    /// the auto-wield reads to decide whether an off-hand rules a bow out, would name a sword
    /// that is not in the off-hand at all. See [`SET_LOCATION_MASKS`].
    pub fn set_into_location(&mut self, loc_mask: u32, item: ObjectId) -> usize {
        let mut n = 0;
        for (s, test) in self.slots.iter_mut().zip(SET_LOCATION_MASKS) {
            if test & loc_mask != 0 {
                s.item = item;
                n += 1;
            }
        }
        n
    }

    /// The weapon-ready slot — the slot `auto_wield_is_legal` reads for the ammunition and shield checks.
    #[must_use]
    pub fn weapon_ready(&self) -> ObjectId {
        self.by_name("weaponReadySlot")
            .map_or(ObjectId(0), |s| s.item)
    }
}

/// The paper doll's element-id-to-location lookup — the clickable regions.
///
/// The UI owns the pixels; this table is here because the *mapping* is game state, and the UI
/// needs a stable contract for it.
pub const PAPERDOLL_REGIONS: [(u32, u32, SlotSide); 24] = [
    (0x1000_01DA, loc::NECK_WEAR, SlotSide::Null),
    (0x1000_01DB, loc::WRIST_WEAR_LEFT, SlotSide::Left),
    (0x1000_01DC, loc::FINGER_WEAR_LEFT, SlotSide::Left),
    (0x1000_01DD, loc::WRIST_WEAR_RIGHT, SlotSide::Right),
    (0x1000_01DE, loc::FINGER_WEAR_RIGHT, SlotSide::Right),
    (0x1000_01DF, loc::WEAPON_READY_SLOT, SlotSide::Null),
    (0x1000_01E0, loc::MISSILE_AMMO, SlotSide::Null),
    (0x1000_01E1, loc::SHIELD, SlotSide::Null),
    (0x1000_01E2, loc::CHEST_WEAR, SlotSide::Null),
    (0x1000_01E3, loc::UPPER_LEG_WEAR, SlotSide::Null),
    (0x1000_058E, loc::TRINKET_ONE, SlotSide::Null),
    (0x1000_0595, loc::SIGIL_ONE, SlotSide::Null),
    (0x1000_0596, loc::SIGIL_TWO, SlotSide::Null),
    (0x1000_0597, loc::SIGIL_THREE, SlotSide::Null),
    (0x1000_05AB, loc::HEAD_WEAR, SlotSide::Null),
    (0x1000_05AC, loc::CHEST_ARMOR, SlotSide::Null),
    (0x1000_05AD, loc::ABDOMEN_ARMOR, SlotSide::Null),
    (0x1000_05AE, loc::UPPER_ARM_ARMOR, SlotSide::Null),
    (0x1000_05AF, loc::LOWER_ARM_ARMOR, SlotSide::Null),
    (0x1000_05B0, loc::HAND_WEAR, SlotSide::Null),
    (0x1000_05B1, loc::UPPER_LEG_ARMOR, SlotSide::Null),
    (0x1000_05B2, loc::LOWER_LEG_ARMOR, SlotSide::Null),
    (0x1000_05B3, loc::FOOT_WEAR, SlotSide::Null),
    (0x1000_05E9, loc::CLOAK, SlotSide::Null),
];

/// The element-id-to-location lookup as the client calls it — the element
/// under the cursor in, `(*loc, *side)` out, `None` when the element is not one of the twenty-four
/// (which is the fork the paper doll's drop release makes between the slot accept
/// and the body accept).
///
/// **The side is the half that had no consumer.** `DropTarget::EquipSlot` carries only an element
/// id, so without this the left and right wrist — and the left and right ring — are
/// indistinguishable at the point the wield is decided.
#[must_use]
pub fn location_info_from_element_id(element: u32) -> Option<(u32, SlotSide)> {
    PAPERDOLL_REGIONS
        .iter()
        .find(|(e, _, _)| *e == element)
        .map(|(_, m, s)| (*m, *s))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered inventory behavior §4's `INVENTORY_LOC` table.
    /// The composites are what a rebuild gets wrong.
    #[test]
    fn the_location_composites_are_exact() {
        assert_eq!(loc::ARMOR, 0x0000_7E00);
        assert_eq!(
            loc::ARMOR,
            loc::CHEST_ARMOR
                | loc::ABDOMEN_ARMOR
                | loc::UPPER_ARM_ARMOR
                | loc::LOWER_ARM_ARMOR
                | loc::UPPER_LEG_ARMOR
                | loc::LOWER_LEG_ARMOR
        );
        assert_eq!(
            loc::WRIST_WEAR,
            loc::WRIST_WEAR_LEFT | loc::WRIST_WEAR_RIGHT
        );
        assert_eq!(
            loc::FINGER_WEAR,
            loc::FINGER_WEAR_LEFT | loc::FINGER_WEAR_RIGHT
        );
        assert_eq!(loc::WEAPON, 0x0250_0000);
        assert_eq!(loc::WEAPON_READY_SLOT, 0x0350_0000);
        assert_eq!(loc::READY_SLOT, 0x03F0_0000);
        assert_eq!(loc::CLOTHING, 0x0800_01FF);
        assert_eq!(
            loc::SIGIL,
            loc::SIGIL_ONE | loc::SIGIL_TWO | loc::SIGIL_THREE
        );
        assert_eq!(loc::JEWELRY, 0x7C0F_8000);
        assert_eq!(loc::ALL, 0x7FFF_FFFF);
    }

    /// Oracle: §4 — the two operative masks in the code.
    #[test]
    fn the_wearable_and_wieldable_masks_are_the_ones_the_code_tests() {
        assert_eq!(loc::WEARABLE, 0x0800_7FFF);
        assert_eq!(loc::WIELDABLE, 0x7EFF_8000);
        // Automatic equipping uses CLOTHING to choose wear over wield.
        assert_eq!(loc::CLOTHING & loc::WEARABLE, loc::CLOTHING);
    }

    /// Oracle: §4's table — 24 slots, each with its
    /// mask.
    #[test]
    fn there_are_twenty_four_slots_with_the_documented_masks() {
        let m = InvSlotModule::new();
        assert_eq!(m.slots.len(), 24);
        assert_eq!(m.by_name("neckSlot").unwrap().inv_loc, 0x8000);
        assert_eq!(m.by_name("headSlot").unwrap().inv_loc, 0x1);
        assert_eq!(m.by_name("weaponReadySlot").unwrap().inv_loc, 0x0350_0000);
        assert_eq!(m.by_name("shieldReadySlot").unwrap().inv_loc, 0x0030_0000);
        assert_eq!(m.by_name("ammoReadySlot").unwrap().inv_loc, 0x0080_0000);
        assert_eq!(m.by_name("sigilThreeSlot").unwrap().inv_loc, 0x4000_0000);
        assert_eq!(m.by_name("clothesShirtSlot").unwrap().inv_loc, 0x2);
        assert_eq!(m.by_name("clothesPantsSlot").unwrap().inv_loc, 0x40);
    }

    #[test]
    fn the_paperdoll_region_table_has_one_row_per_clickable_slot() {
        assert_eq!(PAPERDOLL_REGIONS.len(), 24);
        let (_, l, s) = PAPERDOLL_REGIONS[1];
        assert_eq!((l, s), (loc::WRIST_WEAR_LEFT, SlotSide::Left));
        // Every element id is distinct.
        let mut ids: Vec<u32> = PAPERDOLL_REGIONS.iter().map(|(e, _, _)| *e).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 24);
    }

    #[test]
    fn slot_lookup_and_reset() {
        let mut m = InvSlotModule::new();
        m.set(loc::MELEE_WEAPON, ObjectId(7));
        assert_eq!(
            m.weapon_ready(),
            ObjectId(7),
            "MELEE_WEAPON lands in the weapon-ready slot"
        );
        assert_eq!(m.item_at(loc::MELEE_WEAPON), Some(ObjectId(7)));
        m.reset();
        assert_eq!(m.weapon_ready(), ObjectId(0));
        assert_eq!(m.by_name("weaponReadySlot").unwrap().inv_loc, 0x0350_0000);
    }
}
