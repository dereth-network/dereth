//! Which items the classic interface's equipment slots accept, the green or red frame a dragged
//! item shows over a slot, and the line the player reads when a wield is refused.
use dereth_client_model::World;
use dereth_primitives::ObjectId;

pub const GREEN: u32 = 0x060011f9;
pub const RED: u32 = 0x060011f8;
const EQUIPPABLE: u32 = 0x01ff8042;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ItemFacts {
    pub valid_locations: u32,
    pub item_type: u32,
    pub ammo_type: u16,
    pub combat_use: u8,
}
impl ItemFacts {
    pub fn from_world(world: &World, object: ObjectId) -> Option<Self> {
        let item = world.weenie(object)?;
        Some(Self {
            valid_locations: item.pwd.valid_locations.unwrap_or(0),
            item_type: item.pwd.obj_type,
            ammo_type: item.pwd.ammo_type.unwrap_or(0),
            combat_use: item.pwd.combat_use.unwrap_or(0),
        })
    }
    // The classic interface has no rule for combat_use == 5; later clients added one.
    fn blocks_shield(self) -> bool {
        (self.combat_use == 2 && self.ammo_type != 0) || self.item_type & 0x8000 != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EquipmentFacts {
    pub item: Option<ItemFacts>,
    pub ready: Option<ItemFacts>,
    pub combat_mode: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    MissingObject,
    NotEquippable,
    SlotMismatch,
    AmmunitionMismatch,
    ShieldBlocked,
    HeldDuringCombat,
}
impl EquipmentFacts {
    pub fn from_world(world: &World, object: ObjectId) -> Self {
        Self {
            item: ItemFacts::from_world(world, object),
            // The check reads the inventory panel's ready slot, not an appraisal or
            // a scan of every equipped item. The shared slot module retains that identity.
            ready: ItemFacts::from_world(world, world.inv_slots.weapon_ready()),
            combat_mode: world.combat.combat_mode.raw(),
        }
    }
    pub fn wield_result(self) -> Result<(), Refusal> {
        let item = self.item.ok_or(Refusal::MissingObject)?;
        if item.valid_locations & EQUIPPABLE == 0 {
            return Err(Refusal::NotEquippable);
        }
        if let Some(ready) = self.ready {
            if item.valid_locations & 0x800000 != 0
                && ready.ammo_type != 0
                && ready.ammo_type != item.ammo_type
            {
                return Err(Refusal::AmmunitionMismatch);
            }
            if item.valid_locations & 0x200000 != 0 && ready.blocks_shield() {
                return Err(Refusal::ShieldBlocked);
            }
        }
        if item.valid_locations & 0x1000000 != 0
            && self.combat_mode != 1
            && item.item_type & 0x8000 == 0
        {
            return Err(Refusal::HeldDuringCombat);
        }
        Ok(())
    }
    pub fn result(self, slot_mask: u32) -> Result<(), Refusal> {
        let item = self.item.ok_or(Refusal::MissingObject)?;
        if item.valid_locations & slot_mask == 0 {
            return Err(Refusal::SlotMismatch);
        }
        self.wield_result()
    }
    /// A slot shows no feedback for non-object drags or while a hover state is already active.
    /// Readiness for an inventory request is not read by this hover callback.
    pub fn hover(self, slot_mask: u32, object_drag: bool, hover_active: bool) -> Option<u32> {
        if !object_drag || hover_active {
            None
        } else {
            Some(if self.result(slot_mask).is_ok() {
                GREEN
            } else {
                RED
            })
        }
    }
    /// Modes understood by the item painter; absent feedback remains absent.
    pub fn hover_mode(self, slot_mask: u32, object_drag: bool, hover_active: bool) -> Option<u32> {
        self.hover(slot_mask, object_drag, hover_active)
            .map(|art| if art == GREEN { 4 } else { 5 })
    }
}
pub fn legal(world: &World, item: ObjectId, slot_mask: u32) -> bool {
    EquipmentFacts::from_world(world, item)
        .result(slot_mask)
        .is_ok()
}
pub fn wield_legal(world: &World, item: ObjectId) -> bool {
    EquipmentFacts::from_world(world, item)
        .wield_result()
        .is_ok()
}

/// The refusal lines the player sees. They name the ready item (appropriate form), the ready
/// item (singular form) and the incoming item (appropriate form), respectively; no punctuation
/// is appended.
pub fn refusal_message(world: &World, object: ObjectId, reason: Refusal) -> Option<String> {
    use dereth_client_model::weenie::NameType;
    let name = |id, kind| {
        world.weenie(id).map(|item| {
            item.display_name(
                kind,
                world.material_name(item.pwd.material_type.unwrap_or(0)),
            )
        })
    };
    match reason {
        Refusal::AmmunitionMismatch => Some(format!(
            "Cannot be used with {}",
            name(world.inv_slots.weapon_ready(), NameType::Appropriate)?
        )),
        Refusal::ShieldBlocked => Some(format!(
            "A shield may not be worn with the {}",
            name(world.inv_slots.weapon_ready(), NameType::Singular)?
        )),
        Refusal::HeldDuringCombat => Some(format!(
            "Cannot hold {} while in combat",
            name(object, NameType::Appropriate)?
        )),
        Refusal::MissingObject | Refusal::NotEquippable | Refusal::SlotMismatch => None,
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic panel adapter; no retail behaviour claim).
    use super::*;
    use dereth_client_model::Weenie;
    fn facts(mask: u32) -> EquipmentFacts {
        EquipmentFacts {
            item: Some(ItemFacts {
                valid_locations: mask,
                ..Default::default()
            }),
            ready: None,
            combat_mode: 1,
        }
    }
    #[test]
    fn ammunition_requires_matching_nonzero_ready_ammo_only() {
        let mut f = facts(0x80_0000);
        f.item.as_mut().unwrap().ammo_type = 1;
        assert_eq!(f.wield_result(), Ok(()));
        f.ready = Some(ItemFacts {
            ammo_type: 2,
            ..Default::default()
        });
        assert_eq!(f.wield_result(), Err(Refusal::AmmunitionMismatch));
        f.ready.as_mut().unwrap().ammo_type = 1;
        assert_eq!(f.wield_result(), Ok(()));
        f.ready.as_mut().unwrap().ammo_type = 0;
        assert_eq!(f.wield_result(), Ok(()));
    }
    #[test]
    fn shield_uses_classic_missile_and_caster_predicates() {
        let mut f = facts(0x200000);
        for (combat_use, ammo_type, item_type, blocked) in [
            (2, 1, 0, true),
            (2, 0, 0, false),
            (5, 0, 0, false),
            (0, 0, 0x8000, true),
        ] {
            f.ready = Some(ItemFacts {
                combat_use,
                ammo_type,
                item_type,
                ..Default::default()
            });
            assert_eq!(f.wield_result().is_err(), blocked);
        }
    }
    #[test]
    fn held_objects_require_peace_except_caster_type() {
        let mut f = facts(0x1000000);
        assert_eq!(f.wield_result(), Ok(()));
        f.combat_mode = 2;
        assert_eq!(f.wield_result(), Err(Refusal::HeldDuringCombat));
        f.item.as_mut().unwrap().item_type = 0x8000;
        assert_eq!(f.wield_result(), Ok(()));
    }
    #[test]
    fn hover_keeps_slot_and_drag_gates_separate() {
        let f = facts(0x8000);
        assert_eq!(f.hover(0x8000, true, false), Some(GREEN));
        assert_eq!(f.hover_mode(0x8000, true, false), Some(4));
        assert_eq!(f.hover(0x10000, true, false), Some(RED));
        assert_eq!(f.hover_mode(0x10000, true, false), Some(5));
        assert_eq!(f.hover(0x8000, false, false), None);
        assert_eq!(f.hover(0x8000, true, true), None);
        assert_eq!(facts(0x2000000).wield_result(), Err(Refusal::NotEquippable));
        assert_eq!(
            EquipmentFacts { item: None, ..f }.wield_result(),
            Err(Refusal::MissingObject)
        );
    }
    #[test]
    fn world_adapter_reads_ready_slot_and_raw_description_without_appraisal() {
        let mut world = World::default();
        let mut ammo = Weenie::new(ObjectId(10));
        ammo.pwd.valid_locations = Some(0x80_0000);
        ammo.pwd.ammo_type = Some(1);
        world.tables.weenies.insert(ammo.id, ammo);
        let mut bow = Weenie::new(ObjectId(11));
        bow.pwd.ammo_type = Some(2);
        bow.pwd.combat_use = Some(2);
        world.tables.weenies.insert(bow.id, bow);
        assert!(legal(&world, ObjectId(10), 0x800000));
        world.inv_slots.set_into_location(0x400000, ObjectId(11));
        assert!(!legal(&world, ObjectId(10), 0x800000));
        world.tables.weenies.remove(ObjectId(11));
        assert!(legal(&world, ObjectId(10), 0x800000));
    }
    #[test]
    fn refusal_strings_keep_source_name_form_and_silent_mask_failures() {
        let mut world = World::default();
        let mut item = Weenie::new(ObjectId(20));
        item.pwd.name = "Torch".into();
        item.pwd.plural_name = Some("Torches".into());
        item.pwd.stack_size = Some(3);
        world.tables.weenies.insert(item.id, item);
        world.inv_slots.set_into_location(0x1000000, ObjectId(20));
        assert_eq!(
            refusal_message(&world, ObjectId(20), Refusal::AmmunitionMismatch).as_deref(),
            Some("Cannot be used with Torches")
        );
        assert_eq!(
            refusal_message(&world, ObjectId(20), Refusal::ShieldBlocked).as_deref(),
            Some("A shield may not be worn with the Torch")
        );
        assert_eq!(
            refusal_message(&world, ObjectId(20), Refusal::HeldDuringCombat).as_deref(),
            Some("Cannot hold Torches while in combat")
        );
        assert_eq!(
            refusal_message(&world, ObjectId(20), Refusal::SlotMismatch),
            None
        );
    }
}
