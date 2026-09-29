//! ACE: Source/ACE.Server/WorldObjects/Creature_Missile.cs::LaunchProjectile
//! (real-content) Atlatl darts, arrows and thrown darts reach a Holtburg drudge from range, point
//! blank, while it chases and after crossing a landblock edge; no 'hit the environment'.
//! Fixture: virtual-time bots, retail dats and world.pack.

#![allow(clippy::disallowed_methods)]

#[cfg(feature = "real-content")]
mod real {
    use crate::support::real_content_bot::real::{create_character, Loop as Bot};

    use dereth_primitives::ObjectId;
    use dereth_protocol::combat::{
        AttackerNotification, CombatChangeCombatMode, CombatHandleAttackDoneEvent,
        CombatTargetedMissileAttack, EvasionAttackerNotification,
    };
    use dereth_protocol::comms::CommunicationTextboxString;
    use dereth_protocol::items::InventoryGetAndWieldItem;
    use dereth_protocol::login::CharacterLoginCompleteNotification;
    use dereth_protocol::movement::{
        AutonomousPosition, MoveTimestamps, MovementAutonomousPosition,
    };
    use dereth_protocol::objects::{EffectsPlayerTeleport, ItemCreateObject};
    use dereth_protocol::types::space::{Frame, PositionWire, Quat, Vec3};

    use empyrean_entity::enums::{CombatMode, EquipMask};
    use empyrean_entity::ObjectGuid;

    use empyrean_testkit::decode;

    use empyrean_world::world_objects::world_object::WorldObject;
    use empyrean_world::world_objects::{
        container, creature_combat, creature_equipment, monster_combat,
    };

    /// The weenies (ACE's world DB).
    const DRUDGE_SKULKER: u32 = 7;
    const ATLATL: u32 = 12463;
    const ATLATL_DART: u32 = 12464;
    const SHORTBOW: u32 = 307;
    const ARROW: u32 = 300;
    /// Thrown darts (`WeenieType.Missile`: a thrown weapon, no launcher).
    const DART: u32 = 316;

    const ENVIRONMENT: &str = "Your missile attack hit the environment.";

    /// A landblock-local x as a global x (`landblock x * 192 + x`).
    fn global_x(cell: u32, x: f32) -> f32 {
        f32::from(u8::try_from(cell >> 24).expect("block x")) * 192.0 + x
    }

    impl Bot {
        /// Walks east to `x`, 0.5 m per 0.1 s, with AutonomousPositions at the terrain height the
        /// server's body stands at (a retail client reports its own).
        /// One AutonomousPosition `dx` metres west.
        fn step_west(&mut self, dx: f32) {
            let mut at = self
                .ts
                .world
                .objects
                .get(self.g)
                .and_then(WorldObject::location)
                .expect("a location");
            at.position_x -= dx;
            self.send_position(&at);
        }

        fn send_position(&mut self, at: &empyrean_entity::Position) {
            self.action(&MovementAutonomousPosition(AutonomousPosition {
                position: PositionWire {
                    objcell_id: at.cell(),
                    frame: Frame {
                        origin: Vec3 {
                            x: at.position_x,
                            y: at.position_y,
                            z: at.position_z,
                        },
                        orientation: Quat {
                            w: at.rotation_w,
                            x: at.rotation_x,
                            y: at.rotation_y,
                            z: at.rotation_z,
                        },
                    },
                },
                timestamps: MoveTimestamps::default(),
                contact: 1,
            }));
        }

        /// Walks along y to global x `gx_target` (`landblock x * 192 + x`), 0.5 m per 0.1 s, with
        /// AutonomousPositions as a retail client sends them: the outdoor cell (and landblock) each
        /// step is in, and the terrain height there.
        fn walk_to_global(&mut self, gx_target: f32) {
            for _ in 0..2000 {
                let at = self
                    .ts
                    .world
                    .objects
                    .get(self.g)
                    .and_then(WorldObject::location)
                    .expect("a location");
                let gx = global_x(at.cell(), at.position_x);
                if (gx - gx_target).abs() <= 0.01 {
                    return;
                }
                let gx = if gx > gx_target {
                    (gx - 0.5).max(gx_target)
                } else {
                    (gx + 0.5).min(gx_target)
                };
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let bx = (gx / 192.0).floor() as u32;
                #[allow(clippy::cast_precision_loss)]
                let lx = gx - bx as f32 * 192.0;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let (cx, cy) = (
                    (lx / 24.0).floor() as u32,
                    (at.position_y / 24.0).floor() as u32,
                );
                let cell = (bx << 24) | (at.cell() & 0x00FF_0000) | (cx * 8 + cy + 1);
                let mut to = at;
                to.set_landblock_id(empyrean_entity::LandblockId::new(cell));
                to.position_x = lx;
                let probe = dereth_primitives::Position {
                    cell: dereth_primitives::CellId(cell),
                    frame: dereth_primitives::Frame {
                        origin: dereth_primitives::Vec3::new(lx, at.position_y, 0.0),
                        ..Default::default()
                    },
                };
                to.position_z = self
                    .ts
                    .world
                    .physics
                    .terrain_height_at(&probe)
                    .expect("outdoor terrain");
                self.send_position(&to);
                self.ts.advance(0.1);
            }
            panic!("never arrived");
        }

        /// [`Self::walk_to_global`] to local `x` of `0xAAB4`.
        fn walk_to(&mut self, x: f32) {
            self.walk_to_global(global_x(0xAAB4_0000, x));
        }

        fn pack_item(&self, wcid: u32) -> ObjectGuid {
            container::inventory_values(&self.ts.world, self.g)
                .into_iter()
                .find(|i| {
                    self.ts
                        .world
                        .objects
                        .get(*i)
                        .is_some_and(|o| o.biota.weenie_class_id == wcid)
                })
                .unwrap_or_else(|| panic!("wcid {wcid} in the pack"))
        }

        fn wield(&mut self, wcid: u32, slot: EquipMask) {
            let item = self.pack_item(wcid);
            self.action(&InventoryGetAndWieldItem {
                item: ObjectId(item.full()),
                slot: slot.0,
            });
            self.ts.advance(1.0);
            let o = self.ts.world.objects.get(item).expect("the item");
            assert_eq!(
                o.wielder_id(),
                Some(self.g.full()),
                "wcid {wcid} is wielded"
            );
        }
    }

    /// Creates a character, enters the world, then teleports to `(x, y)` of Holtburg's `0xAAB40040`.
    fn enter_at(x: f32, y: f32) -> Bot {
        enter_at_cell(0xAAB4_0040, x, y)
    }

    /// As [`enter_at`], at `(x, y)` of outdoor cell `cell`.
    fn enter_at_cell(cell: u32, x: f32, y: f32) -> Bot {
        let mut b = create_character(&[47, 6, 22, 24]);
        b.enter_world();
        let id = b.id;

        let mark = b.mark();
        b.admin_command(&format!("@teleloc 0x{cell:08X} {x} {y} 27.6"));
        assert!(
            b.ts.run_until(5.0, |ts| !decode::all_of::<EffectsPlayerTeleport>(
                &ts.received_raw(id)[mark..]
            )
            .is_empty()),
            "teleported"
        );
        b.ts.advance(1.0);
        b.action(&CharacterLoginCompleteNotification);
        b.ts.advance(1.0);
        b
    }

    /// Wield `launcher` (if any) and `ammo` and go into missile mode at `x` on the line west of
    /// the generator's drudge (187.1, 183.7); answers the bot and the drudge.
    fn arm(launcher: Option<u32>, ammo: u32, ammo_slot: EquipMask, x: f32) -> (Bot, ObjectGuid) {
        // arrive 20 m further west, in the next outdoor cell, then walk in as a retail client does
        // (AutonomousPositions across the cell edge)
        let mut b = enter_at(x - 20.0, 183.7);
        b.walk_to(x);
        let drudges = b.nearby_wcid(DRUDGE_SKULKER);
        assert_eq!(drudges.len(), 1, "the generator's drudge skulker");
        let drudge = drudges[0];

        if let Some(launcher) = launcher {
            b.admin_command(&format!("@ci {launcher}"));
            b.wield(launcher, EquipMask::MissileWeapon);
        }
        b.admin_command(&format!("@ci {ammo} {STACK}"));
        b.wield(ammo, ammo_slot);

        b.action(&CombatChangeCombatMode {
            combat_mode: u32::try_from(CombatMode::Missile.0).expect("a mode"),
        });
        b.ts.advance(2.0);
        assert_eq!(
            creature_combat::combat_mode(&b.ts.world, b.g),
            CombatMode::Missile,
            "in missile mode"
        );
        (b, drudge)
    }

    const STACK: u32 = 50;

    /// The horizontal distance from the character to `o`.
    fn distance(b: &Bot, o: ObjectGuid) -> f32 {
        let at = |o: ObjectGuid| {
            b.ts.world
                .objects
                .get(o)
                .and_then(WorldObject::location)
                .expect("a location")
        };
        let (me, it) = (at(b.g), at(o));
        (me.position_x - it.position_x).hypot(me.position_y - it.position_y)
    }

    /// The ammo left: the wielded stack of `ammo`.
    fn ammo_left(b: &Bot, ammo: u32) -> u32 {
        creature_equipment::equipped_objects_values(&b.ts.world, b.g)
            .into_iter()
            .filter_map(|g| b.ts.world.objects.get(g))
            .find(|o| o.biota.weenie_class_id == ammo)
            .and_then(WorldObject::stack_size)
            .map_or(0, |n| u32::try_from(n).unwrap_or(0))
    }

    /// Shoots the drudge from `x` until it dies (at most 60 s, the client attacking again after
    /// each AttackDone as a retail client does): the drudge notices the first hit, charges and
    /// fights in melee, and every projectile, from range to point-blank, reaches it (a hit, an
    /// evade, or the kill); none hits the environment. Answers the closest distance at an attack.
    fn shoot_the_drudge(launcher: Option<u32>, ammo: u32, ammo_slot: EquipMask, x: f32) -> f32 {
        shoot_the_drudge_kiting(launcher, ammo, ammo_slot, x, false)
    }

    /// As [`shoot_the_drudge`]; when `kite`, the character also backs off west 0.3 m per 0.1 s
    /// once the drudge is within 4 m, so the drudge chases (a moving target at close range).
    fn shoot_the_drudge_kiting(
        launcher: Option<u32>,
        ammo: u32,
        ammo_slot: EquipMask,
        x: f32,
        kite: bool,
    ) -> f32 {
        let (b, drudge) = arm(launcher, ammo, ammo_slot, x);
        engage(b, drudge, ammo, kite)
    }

    /// The engagement of [`shoot_the_drudge_kiting`], armed and in missile mode.
    fn engage(mut b: Bot, drudge: ObjectGuid, ammo: u32, kite: bool) -> f32 {
        let id = b.id;
        let mark = b.mark();
        let start = b.ts.seconds();
        let mut attacks = 0;
        let mut dones = 0;
        let mut closest = f32::MAX;
        while b
            .ts
            .world
            .objects
            .get(drudge)
            .is_some_and(|o| !monster_combat::is_dead(o))
        {
            if b.ts.seconds() - start >= 60.0 {
                break;
            }
            let done =
                decode::all_of::<CombatHandleAttackDoneEvent>(&b.ts.received_raw(id)[mark..]).len();
            if attacks == 0 || done > dones {
                dones = done;
                closest = closest.min(distance(&b, drudge));
                b.action(&CombatTargetedMissileAttack {
                    target: ObjectId(drudge.full()),
                    attack_height: 2,
                    power_level: 0.5,
                });
                attacks += 1;
            }
            if kite && distance(&b, drudge) < 4.0 {
                b.step_west(0.3);
            }
            b.ts.advance(0.1);
        }
        b.ts.advance(3.0);

        let launched = STACK - ammo_left(&b, ammo);
        let chat: Vec<String> = b
            .since::<CommunicationTextboxString>(mark)
            .into_iter()
            .map(|t| t.text)
            .collect();
        let hits = b.since::<AttackerNotification>(mark).len();
        let evades = b.since::<EvasionAttackerNotification>(mark).len();
        let seen = b
            .since::<ItemCreateObject>(mark)
            .iter()
            .filter(|c| c.0.wdesc.wcid == ammo)
            .count();
        let summary = format!("{attacks} attacks, {launched} launched, {seen} seen created, {hits} hits, {evades} evades, closest {closest:.2} m; chat {chat:?}");
        eprintln!("Missile checks: {summary}");
        assert!(
            !chat.iter().any(|t| t == ENVIRONMENT),
            "a projectile hit the environment: {summary}"
        );
        assert!(
            b.ts.world
                .objects
                .get(drudge)
                .is_none_or(monster_combat::is_dead),
            "the drudge dies within a minute: {summary}"
        );
        // at least one projectile flew (a first-dart kill is a legitimate seeded outcome)
        assert!(launched >= 1, "{summary}");
        // every projectile reached the drudge: a hit or an evade each, but the killing one
        assert!(
            hits + evades + 1 >= usize::try_from(launched).expect("small"),
            "{summary}"
        );
        closest
    }

    /// 12 m west of the drudge.
    const RANGE: f32 = 175.0;
    /// 1 m west of the drudge; it keeps to its melee distance (about 1.6 m) once it fights.
    const POINT_BLANK: f32 = 186.1;

    #[test]
    fn atlatl_darts_reach_a_holtburg_drudge_from_range() {
        shoot_the_drudge(Some(ATLATL), ATLATL_DART, EquipMask::MissileAmmo, RANGE);
    }

    #[test]
    fn atlatl_darts_reach_a_holtburg_drudge_point_blank() {
        let closest = shoot_the_drudge(
            Some(ATLATL),
            ATLATL_DART,
            EquipMask::MissileAmmo,
            POINT_BLANK,
        );
        assert!(closest < 2.0, "in melee range: {closest}");
    }

    #[test]
    fn arrows_reach_a_holtburg_drudge_from_range() {
        shoot_the_drudge(Some(SHORTBOW), ARROW, EquipMask::MissileAmmo, RANGE);
    }

    #[test]
    fn arrows_reach_a_holtburg_drudge_point_blank() {
        let closest = shoot_the_drudge(Some(SHORTBOW), ARROW, EquipMask::MissileAmmo, POINT_BLANK);
        assert!(closest < 2.0, "in melee range: {closest}");
    }

    #[test]
    fn thrown_darts_reach_a_holtburg_drudge_from_range() {
        shoot_the_drudge(None, DART, EquipMask::MissileWeapon, RANGE);
    }

    #[test]
    fn thrown_darts_reach_a_holtburg_drudge_point_blank() {
        let closest = shoot_the_drudge(None, DART, EquipMask::MissileWeapon, POINT_BLANK);
        assert!(closest < 2.0, "in melee range: {closest}");
    }

    #[test]
    fn atlatl_darts_reach_a_chasing_holtburg_drudge() {
        shoot_the_drudge_kiting(
            Some(ATLATL),
            ATLATL_DART,
            EquipMask::MissileAmmo,
            RANGE,
            true,
        );
    }

    #[test]
    fn arrows_reach_a_chasing_holtburg_drudge() {
        shoot_the_drudge_kiting(
            Some(SHORTBOW),
            ARROW,
            EquipMask::MissileAmmo,
            POINT_BLANK,
            true,
        );
    }

    /// The character arrives in the next landblock east (`0xABB4`) and walks west across the edge
    /// to 12 m from the drudge, as a player walking out of town does, then shoots.
    fn shoot_after_crossing_a_landblock_edge(launcher: u32, ammo: u32) {
        let mut b = enter_at_cell(0xABB4_0008, 10.0, 183.7);
        b.walk_to(RANGE);
        let drudges = b.nearby_wcid(DRUDGE_SKULKER);
        assert_eq!(drudges.len(), 1, "the generator's drudge skulker");
        let drudge = drudges[0];
        b.admin_command(&format!("@ci {launcher}"));
        b.wield(launcher, EquipMask::MissileWeapon);
        b.admin_command(&format!("@ci {ammo} {STACK}"));
        b.wield(ammo, EquipMask::MissileAmmo);
        b.action(&CombatChangeCombatMode {
            combat_mode: u32::try_from(CombatMode::Missile.0).expect("a mode"),
        });
        b.ts.advance(2.0);
        engage(b, drudge, ammo, false);
    }

    #[test]
    fn arrows_reach_a_holtburg_drudge_after_walking_across_a_landblock_edge() {
        shoot_after_crossing_a_landblock_edge(SHORTBOW, ARROW);
    }

    #[test]
    fn atlatl_darts_reach_a_holtburg_drudge_after_walking_across_a_landblock_edge() {
        shoot_after_crossing_a_landblock_edge(ATLATL, ATLATL_DART);
    }
}
