//! ACE: Source/ACE.Server/WorldObjects/Player_Inventory.cs::TryShuffleStance / RequiresStanceSwap
//! (real-content) Swapping weapons in combat as a client does it (the wielded weapon put in the
//! pack, and once the server reports it moved, the new one wielded) leaves the player in the new
//! weapon's combat mode and stance, whichever of the two waiting mode changes the server's clock
//! would otherwise run first; taking the arrows off a drawn bow returns to peace.
//! Fixture: virtual-time bots, retail dats and world.pack; a live server's clock drift between
//! the unwield and the wield is reproduced by moving the portal-year clock 5 ms back.

#![allow(clippy::disallowed_methods)]

#[cfg(feature = "real-content")]
mod real {
    use crate::support::real_content_bot::real::{create_character, Loop as Bot};

    use dereth_primitives::ObjectId;
    use dereth_protocol::combat::CombatChangeCombatMode;
    use dereth_protocol::items::{InventoryGetAndWieldItem, InventoryPutItemInContainer};
    use dereth_protocol::objects::ItemParentEvent;
    use dereth_protocol::qualities::QualitiesPrivateUpdateInt;

    use empyrean_entity::enums::{CombatMode, EquipMask, MotionStance};
    use empyrean_entity::ObjectGuid;

    use empyrean_testkit::decode;

    use empyrean_world::world_objects::{
        container, creature_combat, creature_equipment, player_melee,
    };

    /// The weenies (ACE's world DB): the training wand, knuckles and shortbow, and arrows.
    const WAND: u32 = 12748;
    const KNUCKLES: u32 = 45558;
    const BOW: u32 = 12741;
    const ARROW: u32 = 31717;

    /// `PropertyInt.CombatMode`.
    const COMBAT_MODE: u32 = 40;
    /// `InventoryPutObjInContainer` (`0x0022`): the server's report that an item moved, which a
    /// client waits for before wielding what the moved item was blocking.
    const MOVED: u32 = 0x0022;
    /// How far behind the wall clock the portal-year clock has drifted by the time the wield
    /// arrives, as the two drift on a live server between ticks.
    const DRIFT: f64 = 0.005;

    impl Bot {
        fn carried(&self, wcid: u32) -> ObjectGuid {
            let mut all = container::inventory_values(&self.ts.world, self.g);
            all.extend(creature_equipment::equipped_objects_values(
                &self.ts.world,
                self.g,
            ));
            all.into_iter()
                .find(|i| {
                    self.ts
                        .world
                        .objects
                        .get(*i)
                        .is_some_and(|o| o.biota.weenie_class_id == wcid)
                })
                .unwrap_or_else(|| panic!("wcid {wcid} carried"))
        }

        /// The wielded item of `wcid` (a starting pack can hold a second stack of arrows).
        fn wielded(&self, wcid: u32) -> ObjectGuid {
            creature_equipment::equipped_objects_values(&self.ts.world, self.g)
                .into_iter()
                .find(|i| {
                    self.ts
                        .world
                        .objects
                        .get(*i)
                        .is_some_and(|o| o.biota.weenie_class_id == wcid)
                })
                .unwrap_or_else(|| panic!("wcid {wcid} wielded"))
        }

        fn wield_now(&mut self, wcid: u32, slot: EquipMask) {
            let item = self.carried(wcid);
            self.action(&InventoryGetAndWieldItem {
                item: ObjectId(item.full()),
                slot: slot.0,
            });
        }

        fn unwield_now(&mut self, wcid: u32) {
            let item = self.wielded(wcid);
            self.action(&InventoryPutItemInContainer {
                item: ObjectId(item.full()),
                container: ObjectId(self.g.full()),
                slot: 0,
            });
        }

        /// The weapon in the way put in the pack; on the server's report that it moved, the new
        /// weapon wielded at once, with the clocks drifted as a live server's do.
        fn swap(&mut self, from: u32, to: u32, slot: EquipMask) {
            let mark = self.mark();
            self.unwield_now(from);
            let id = self.id;
            assert!(
                self.ts.run_until(5.0, |ts| ts.received_raw(id)[mark..]
                    .iter()
                    .any(|m| decode::decode(m).kind == MOVED)),
                "the unwield is reported"
            );
            self.ts.world.timers.portal_year_ticks -= DRIFT;
            self.wield_now(to, slot);
            self.ts.advance(5.0);
        }

        fn mode(&self) -> CombatMode {
            creature_combat::combat_mode(&self.ts.world, self.g)
        }

        fn stance(&self) -> MotionStance {
            player_melee::current_stance(&self.ts.world, self.g)
        }

        /// The combat modes the client was told since `mark`, in order.
        fn modes_told(&self, mark: usize) -> Vec<i32> {
            decode::all_of::<QualitiesPrivateUpdateInt>(&self.ts.received_raw(self.id)[mark..])
                .into_iter()
                .filter(|u| u.0.property_id == COMBAT_MODE)
                .map(|u| u.0.value)
                .collect()
        }

        /// The parent events the client was told since `mark` for `item`: their locations.
        fn parents_told(&self, mark: usize, item: ObjectGuid) -> Vec<u32> {
            decode::all_of::<ItemParentEvent>(&self.ts.received_raw(self.id)[mark..])
                .into_iter()
                .filter(|p| p.item.0 == item.full())
                .map(|p| p.location)
                .collect()
        }
    }

    /// A character with the three weapons and arrows, the arrows ready and `first` wielded, in
    /// `mode`.
    fn armed(first: u32, slot: EquipMask, mode: CombatMode) -> Bot {
        let mut b = create_character(&[47, 6, 22, 24]);
        b.enter_world();
        for wcid in [WAND, KNUCKLES, BOW] {
            b.admin_command(&format!("@ci {wcid}"));
        }
        b.admin_command(&format!("@ci {ARROW} 50"));
        b.ts.advance(1.0);
        b.wield_now(ARROW, EquipMask::MissileAmmo);
        b.ts.advance(1.0);
        b.wield_now(first, slot);
        b.ts.advance(1.0);
        b.action(&CombatChangeCombatMode {
            combat_mode: u32::try_from(mode.0).expect("a mode"),
        });
        b.ts.advance(3.0);
        assert_eq!(b.mode(), mode, "in {mode:?} mode with wcid {first}");
        b
    }

    #[test]
    fn a_wand_swapped_for_knuckles_in_magic_mode_leaves_melee_mode() {
        let mut b = armed(WAND, EquipMask::Held, CombatMode::Magic);
        let mark = b.mark();
        b.swap(WAND, KNUCKLES, EquipMask::MeleeWeapon);
        assert_eq!(b.mode(), CombatMode::Melee);
        assert_eq!(b.stance(), MotionStance::HandCombat);
        assert_eq!(
            b.modes_told(mark).last(),
            Some(&2),
            "the client was told melee last"
        );
    }

    #[test]
    fn knuckles_swapped_for_a_wand_in_melee_mode_leave_magic_mode() {
        let mut b = armed(KNUCKLES, EquipMask::MeleeWeapon, CombatMode::Melee);
        let mark = b.mark();
        b.swap(KNUCKLES, WAND, EquipMask::Held);
        assert_eq!(b.mode(), CombatMode::Magic, "not peace");
        assert_eq!(b.stance(), MotionStance::Magic);
        assert_eq!(
            b.modes_told(mark).last(),
            Some(&8),
            "the client was told magic last"
        );
    }

    #[test]
    fn a_wand_swapped_for_a_bow_in_magic_mode_leaves_missile_mode_with_the_arrows_in_hand() {
        let mut b = armed(WAND, EquipMask::Held, CombatMode::Magic);
        let arrows = b.wielded(ARROW);
        let mark = b.mark();
        b.swap(WAND, BOW, EquipMask::MissileWeapon);
        assert_eq!(b.mode(), CombatMode::Missile, "not peace");
        assert_eq!(b.stance(), MotionStance::BowCombat);
        assert_eq!(
            b.modes_told(mark).last(),
            Some(&4),
            "the client was told missile last"
        );
        assert_eq!(
            b.parents_told(mark, arrows).last(),
            Some(&1),
            "the arrows are put in the right hand"
        );
    }

    #[test]
    fn the_arrows_taken_off_a_drawn_bow_return_to_peace_and_put_back_stay_off_the_body() {
        let mut b = armed(WAND, EquipMask::Held, CombatMode::Magic);
        b.swap(WAND, BOW, EquipMask::MissileWeapon);
        assert_eq!(b.mode(), CombatMode::Missile);
        let arrows = b.wielded(ARROW);

        let mark = b.mark();
        b.unwield_now(ARROW);
        b.ts.advance(15.0);
        assert_eq!(b.mode(), CombatMode::NonCombat);
        assert_eq!(b.stance(), MotionStance::NonCombat);
        assert_eq!(
            b.modes_told(mark).last(),
            Some(&1),
            "the client was told peace"
        );

        let mark = b.mark();
        b.action(&InventoryGetAndWieldItem {
            item: ObjectId(arrows.full()),
            slot: EquipMask::MissileAmmo.0,
        });
        b.ts.advance(5.0);
        assert_eq!(b.mode(), CombatMode::NonCombat);
        assert!(
            b.parents_told(mark, arrows).is_empty(),
            "arrows readied at peace are not drawn"
        );
    }
}
