//! ACE: Source/ACE.Server/WorldObjects/Player_Location.cs::Teleport
//! (real-content) After exit-token, admin teleports, a portal into an unloaded dungeon and a
//! lifestone recall, the destination's objects are created for the client a second later.
//! Fixture: virtual-time bots, retail dats and world.pack.

// V260, V278, V286, V287, V308.
#![allow(clippy::disallowed_methods)]

#[cfg(feature = "real-content")]
mod real {
    use std::collections::BTreeSet;

    use dereth_primitives::ObjectId;
    use dereth_protocol::combat::CharacterTeleToLifestone;
    use dereth_protocol::items::{InventoryGiveObjectRequest, InventoryUseEvent};
    use dereth_protocol::login::CharacterLoginCompleteNotification;
    use dereth_protocol::objects::{EffectsPlayerTeleport, ItemCreateObject, ItemDeleteObject};
    use empyrean_entity::enums::{PositionType, PropertyString};
    use empyrean_entity::{LandblockId, ObjectGuid};
    use empyrean_testkit::decode;
    use empyrean_world::managers::landblock_manager;
    use empyrean_world::physics::{object_maint, phys_ext};
    use empyrean_world::world_objects::container;

    use crate::support::real_content_bot::real::{create_and_enter, Loop};

    /// Jonathan, the Academy NPC who takes the exit token (ACE's world DB).
    const JONATHAN: u32 = 29324;
    /// The Academy exit token he gives on Use and takes back.
    const EXIT_TOKEN: u32 = 29335;
    /// Holtburg's life stone and one of its NPCs (Alcott, next to it).
    const LIFE_STONE: u32 = 509;
    const ALCOTT: u32 = 44895;
    /// Where Jonathan's emote teleports the character (`TeleportTarget`).
    const HOLTBURG_ARRIVAL_CELL: u32 = 0xA9B4_0019;
    /// Holtburg's "Portal to Town Network" (outdoors, west of the life stone), and its destination
    /// landblock (a dungeon).
    const TOWN_NETWORK_PORTAL: u32 = 43065;
    const TOWN_NETWORK_LANDBLOCK: u32 = 0x0007;

    // ---- the arrival ----------------------------------------------------------------------

    /// Waits for the teleport since `mark`, then leaves portal space as the client does
    /// (LoginComplete a second later), and gives the arrival 3 s.
    fn arrive(l: &mut Loop, mark: usize, max_secs: f64) {
        let id = l.id;
        assert!(
            l.ts.run_until(max_secs, |ts| !decode::all_of::<EffectsPlayerTeleport>(
                &ts.received_raw(id)[mark..]
            )
            .is_empty()),
            "the character is teleported"
        );
        l.collect_not_ported();
        l.advance(1.0);
        l.action(&CharacterLoginCompleteNotification);
        l.advance(3.0);
    }

    /// The character's create set over every loaded landblock's objects: the guids a client must
    /// be sent: visible objects in the player's create set (V260/V278/V286/V287/V308).
    fn expected_in_view(l: &Loop) -> BTreeSet<u32> {
        let w = &l.ts.world;
        let me = phys_ext::physics_obj(w, l.g).expect("a body");
        let mut out = BTreeSet::new();
        for id in landblock_manager::get_loaded_landblocks(w) {
            for g in w
                .landblock_manager
                .landblocks
                .expect(id)
                .get_all_world_objects_for_diagnostics()
            {
                let Some(o) = w.objects.get(g) else { continue };
                if g == l.g || o.visibility() {
                    continue;
                }
                if o.phys
                    .is_some_and(|h| object_maint::in_create_set(w, me, h))
                {
                    out.insert(g.full());
                }
            }
        }
        out
    }

    fn created_since(l: &Loop, mark: usize) -> BTreeSet<u32> {
        l.since::<ItemCreateObject>(mark)
            .iter()
            .map(|c| c.0.id.0)
            .collect()
    }

    fn describe(l: &Loop, g: u32) -> String {
        l.ts.world.objects.get(ObjectGuid::new(g)).map_or_else(
            || format!("{g:08X} (gone)"),
            |o| {
                format!(
                    "{g:08X} wcid {} at {:08X} {:?}",
                    o.biota.weenie_class_id,
                    o.location().map_or(0, |p| p.cell()),
                    o.get_property(PropertyString::Name)
                )
            },
        )
    }

    /// Every object in view of the arrival spot was sent to the client after the teleport; returns
    /// the wcids created.
    fn assert_arrival_objects(l: &Loop, mark: usize, what: &str) -> Vec<u32> {
        let created = created_since(l, mark);
        let expected = expected_in_view(l);
        let missing: Vec<String> = expected
            .iter()
            .filter(|g| !created.contains(g))
            .map(|g| describe(l, *g))
            .collect();
        println!(
            "{what}: {} objects in view of {:08X}, {} created",
            expected.len(),
            l.location().cell(),
            created.len()
        );
        assert!(!expected.is_empty(), "{what}: the arrival area has objects");
        assert!(
            missing.is_empty(),
            "{what}: {} of {} objects in view never reached the client: {missing:#?}",
            missing.len(),
            expected.len()
        );
        created
            .iter()
            .filter_map(|g| l.ts.world.objects.get(ObjectGuid::new(*g)))
            .map(|o| o.biota.weenie_class_id)
            .collect()
    }

    /// The objects sent to the character, by guid: the create set's record (V260/V278/V286/V287/V308).
    fn visible(l: &Loop) -> BTreeSet<u32> {
        let me = phys_ext::physics_obj(&l.ts.world, l.g).expect("a body");
        object_maint::get_known_objects_values(&l.ts.world, me)
            .into_iter()
            .filter_map(|h| phys_ext::id(&l.ts.world, h))
            .collect()
    }

    /// The objects left behind: out of the create set, in the forget queue, and not deleted on the
    /// client by the server (no DeleteObject is sent for them).
    fn assert_departed(l: &Loop, mark: usize, before: &BTreeSet<u32>) {
        let w = &l.ts.world;
        let me = phys_ext::physics_obj(w, l.g).expect("a body");
        let now_in_set = expected_in_view(l);
        let queued: BTreeSet<u32> = object_maint::get_destruction_queue_copy(w, me)
            .into_iter()
            .filter_map(|(h, _)| phys_ext::id(w, h))
            .collect();
        let left: Vec<u32> = before
            .iter()
            .copied()
            .filter(|g| !now_in_set.contains(g))
            .collect();
        assert!(
            !left.is_empty(),
            "the departed area's objects left the create set"
        );
        let unqueued: Vec<String> = left
            .iter()
            .filter(|g| !queued.contains(g) && w.objects.get(ObjectGuid::new(**g)).is_some())
            .map(|g| describe(l, *g))
            .collect();
        assert!(
            unqueued.is_empty(),
            "left behind but not queued for destruction: {unqueued:#?}"
        );
        let deleted: Vec<u32> = l
            .since::<ItemDeleteObject>(mark)
            .iter()
            .map(|d| d.id.0)
            .filter(|g| left.contains(g))
            .collect();
        assert!(
            deleted.is_empty(),
            "ACE sends no DeleteObject for objects left behind: {deleted:08X?}"
        );
    }

    // ---- the scenarios --------------------------------------------------------------------

    /// Use Jonathan for the exit token, give it back, and arrive in Holtburg with
    /// its NPCs and life stone.
    fn leave_the_academy(l: &mut Loop) {
        let jonathan = *l
            .nearby_wcid(JONATHAN)
            .first()
            .expect("Jonathan is in the Academy");
        let at = l.location_of(jonathan);
        // Jonathan stands in another room of the Academy: go there (an in-landblock move)
        l.admin_command(&format!(
            "@teleloc 0x{:08X} {} {} {}",
            at.cell(),
            at.position_x + 1.0,
            at.position_y,
            at.position_z
        ));
        l.action(&CharacterLoginCompleteNotification);
        l.advance(2.0);

        l.action(&InventoryUseEvent {
            object: ObjectId(jonathan.full()),
        });
        l.walk_toward(&at, 0.6);
        l.advance(3.0);
        let token = container::inventory_values(&l.ts.world, l.g)
            .into_iter()
            .find(|i| {
                l.ts.world
                    .objects
                    .get(*i)
                    .is_some_and(|o| o.biota.weenie_class_id == EXIT_TOKEN)
            })
            .expect("Jonathan gave the exit token");

        let before = visible(l);
        let mark = l.mark();
        l.action(&InventoryGiveObjectRequest {
            target: ObjectId(jonathan.full()),
            item: ObjectId(token.full()),
            amount: 1,
        });
        arrive(l, mark, 30.0);
        assert_eq!(
            l.location().cell() >> 16,
            HOLTBURG_ARRIVAL_CELL >> 16,
            "in Holtburg"
        );
        let wcids = assert_arrival_objects(l, mark, "Jonathan to Holtburg");
        assert!(
            wcids.contains(&LIFE_STONE) && wcids.contains(&ALCOTT),
            "the life stone and Alcott: {wcids:?}"
        );
        assert_departed(l, mark, &before);
        l.assert_all_decode(mark, "leave the Academy");
    }

    /// Giving the exit token back to Jonathan brings the character to Holtburg.
    #[test]
    fn jonathans_exit_token_arrives_in_holtburg_with_its_objects() {
        let mut l = create_and_enter();
        leave_the_academy(&mut l);
    }

    /// `@telepoi Holtburg` and `@teleloc` into Holtburg, each from the Academy.
    #[test]
    fn admin_teleports_into_holtburg_bring_its_objects() {
        let mut l = create_and_enter();
        let mark = l.mark();
        l.admin_command("@telepoi Holtburg");
        arrive(&mut l, mark, 5.0);
        assert_eq!(
            l.location().cell() >> 16,
            HOLTBURG_ARRIVAL_CELL >> 16,
            "in Holtburg"
        );
        let wcids = assert_arrival_objects(&l, mark, "@telepoi Holtburg");
        assert!(wcids.contains(&LIFE_STONE), "the life stone");

        let mut l = create_and_enter();
        let mark = l.mark();
        l.admin_command("@teleloc 0xA9B40019 84 7.1 94");
        arrive(&mut l, mark, 5.0);
        let wcids = assert_arrival_objects(&l, mark, "@teleloc Holtburg");
        assert!(
            wcids.contains(&LIFE_STONE) && wcids.contains(&ALCOTT),
            "the life stone and Alcott"
        );
    }

    /// From Holtburg, the Portal to Town Network: arriving in a dungeon landblock nobody loaded.
    #[test]
    fn a_portal_into_an_unloaded_dungeon_brings_its_objects() {
        let mut l = create_and_enter();
        let mark = l.mark();
        l.admin_command("@teleloc 0xA9B40003 16.4 55.6 78.2");
        arrive(&mut l, mark, 5.0);
        assert_arrival_objects(&l, mark, "@teleloc beside the portal");
        // 2 m from the portal: within its use radius, so the Use needs no walk
        let portal = *l
            .nearby_wcid(TOWN_NETWORK_PORTAL)
            .first()
            .expect("the Portal to Town Network");

        let before = visible(&l);
        let mark = l.mark();
        l.action(&InventoryUseEvent {
            object: ObjectId(portal.full()),
        });
        arrive(&mut l, mark, 10.0);
        assert_eq!(
            l.location().cell() >> 16,
            TOWN_NETWORK_LANDBLOCK,
            "in the Town Network"
        );
        assert_arrival_objects(&l, mark, "the Portal to Town Network");
        assert_departed(&l, mark, &before);
        l.assert_all_decode(mark, "the portal");
    }

    /// The lifestone recall into a landblock that unloaded while the character was away: out of
    /// the Academy with the token (recalls are disabled inside), attuned at Holtburg's life stone,
    /// away until Holtburg unloads, then `/ls` back.
    #[test]
    fn a_lifestone_recall_into_an_unloaded_landblock_brings_its_objects() {
        let mut l = create_and_enter();
        leave_the_academy(&mut l);
        // Set the recall destination beside the stone; this scenario exercises recall and arrival.
        assert!(
            !l.nearby_wcid(LIFE_STONE).is_empty(),
            "Holtburg's life stone"
        );
        let here = l.location();
        l.ts.world
            .objects
            .get_mut(l.g)
            .expect("in the world")
            .set_position(PositionType::Sanctuary, Some(here));

        let mark = l.mark();
        l.admin_command("@telepoi Rithwic");
        arrive(&mut l, mark, 5.0);
        let holtburg = LandblockId::new(HOLTBURG_ARRIVAL_CELL | 0xFFFF);
        let unloaded = l.ts.run_until(900.0, |ts| {
            !landblock_manager::is_loaded(&ts.world, holtburg)
        });
        l.collect_not_ported();
        assert!(unloaded, "Holtburg unloads while nobody is there");

        let mark = l.mark();
        l.action(&CharacterTeleToLifestone);
        arrive(&mut l, mark, 30.0);
        assert_eq!(
            l.location().cell() >> 16,
            HOLTBURG_ARRIVAL_CELL >> 16,
            "back in Holtburg"
        );
        let wcids = assert_arrival_objects(&l, mark, "the lifestone recall");
        assert!(wcids.contains(&LIFE_STONE), "the life stone");
    }
}

mod visibility {
    //! ACE: Source/ACE.Server/WorldObjects/Player_Use.cs::HandleActionUseItem
    use crate::support::object_use_world::*;

    /// `PhysicsObj.enqueue_obj`: within `TeleportCreateObjectDelay` (1 s) of the player's
    /// `LastTeleportTime`, a newly visible object's CreateObject waits a second; otherwise it goes at
    /// once (PhysicsObj.cs).
    #[test]
    fn creates_wait_a_second_after_a_teleport() {
        let mut ts = server();
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        ts.advance(0.5);
        let p = ObjectGuid::new(ALPHA);

        let a = during(&mut ts, alpha, 0.3, |ts| {
            let _ = on_ground(ts, LIFESTONE, at(24.0, 20.0));
        });
        assert_eq!(all(&a, 0xF745).len(), 1, "no recent teleport: at once");

        let now = ts.world.now.utc;
        obj_mut(&mut ts, p)
            .player
            .as_mut()
            .expect("a player")
            .player_location
            .last_teleport_time = now;
        let a = during(&mut ts, alpha, 0.3, |ts| {
            let _ = on_ground(ts, LIFESTONE, at(24.0, 22.0));
        });
        assert!(
            all(&a, 0xF745).is_empty(),
            "within a second of the teleport: held back"
        );
        let a = during(&mut ts, alpha, 1.0, |_| {});
        assert_eq!(all(&a, 0xF745).len(), 1, "a second later");
    }

    /// `Creature.FakeTeleport` (Creature_Navigation.cs): a blip within the landblock moves the body
    /// and the Location (5 mm up per unit of scale) and sends an UpdatePosition; another landblock is
    /// refused.
    #[test]
    fn fake_teleport_blips_within_the_landblock() {
        let mut ts = server();
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        ts.advance(0.5);
        let p = ObjectGuid::new(ALPHA);

        let a = during(&mut ts, alpha, 0.2, |ts| {
            empyrean_world::world_objects::creature_navigation::fake_teleport(
                &mut ts.world,
                p,
                &at(40.0, 30.0),
            )
        });
        let loc = obj(&ts, p).location().expect("placed");
        assert!(
            (loc.position_x - 40.0).abs() < 1e-3 && (loc.position_y - 30.0).abs() < 1e-3,
            "{loc:?}"
        );
        assert!(
            !all(&a, 0xF748).is_empty(),
            "an UpdatePosition: {:04X?}",
            kinds(&a)
        );

        let far =
            Position::from_components(0xA9B5_0001, 10.0, 10.0, 0.0, 0.0, 0.0, 0.0, 1.0, false);
        empyrean_world::world_objects::creature_navigation::fake_teleport(&mut ts.world, p, &far);
        assert_eq!(
            obj(&ts, p).location().expect("placed").landblock(),
            loc.landblock(),
            "another landblock: refused"
        );
    }

    /// `GameActionAdvocateTeleport.Handle`: ignored from a plain player; an admin is told where they
    /// are going ("Teleporting to: (<map coords>)").
    #[test]
    fn the_minimap_teleport_is_for_admins() {
        use dereth_protocol::trade::AdvocateTeleport;
        use dereth_protocol::types::PositionWire;

        let mut ts = server();
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        ts.advance(0.5);
        let mut destination = PositionWire {
            objcell_id: LB | 0x0001,
            ..PositionWire::default()
        };
        destination.frame.origin.x = 40.0;
        destination.frame.origin.y = 40.0;
        destination.frame.orientation.w = 1.0;
        let action = AdvocateTeleport {
            target_name: String::new(),
            destination,
        };

        let a = during(&mut ts, alpha, 0.2, |ts| {
            ts.send_game_action(alpha, &action)
        });
        assert!(chats(&a).is_empty(), "{:?}", chats(&a));

        obj_mut(&mut ts, ObjectGuid::new(ALPHA)).set_is_admin_prop(true);
        let a = during(&mut ts, alpha, 0.2, |ts| {
            ts.send_game_action(alpha, &action)
        });
        let c = chats(&a);
        assert_eq!(c.len(), 1, "{c:?}");
        assert!(
            c[0].starts_with("Teleporting to: (")
                && c[0].ends_with(')')
                && c[0].len() > "Teleporting to: ()".len(),
            "{c:?}"
        );
    }
}
