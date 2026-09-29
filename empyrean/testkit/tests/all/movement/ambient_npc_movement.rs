//! ACE: Source/ACE.Server/WorldObjects/Managers/EmoteManager.cs::ExecuteEmote
//! (real-content) A character standing in Holtburg 60 s receives NPC idle motions, turns and
//! creature MoveToPosition plus per-tick UpdatePosition.
//! Fixture: virtual-time bots, retail dats and world.pack.

#[cfg(feature = "real-content")]
mod real {
    use std::collections::{BTreeMap, BTreeSet};

    use dereth_protocol::login::CharacterLoginCompleteNotification;
    use dereth_protocol::movement::{
        movement_type, position_flags, MovementPositionEvent, MovementSetObjectMovement,
    };
    use dereth_protocol::objects::{EffectsPlayerTeleport, ItemCreateObject};
    use empyrean_entity::ObjectGuid;
    use empyrean_testkit::decode;
    use empyrean_world::managers::landblock_manager;
    use empyrean_world::physics::object_maint;
    use empyrean_world::world_objects::world_object_networking;

    use crate::support::real_content_bot::real::{create_and_enter, Loop};

    /// The object classes of the capture tier's breakdown, from a CreateObject's item type and
    /// description flags (`ObjectDescriptionFlag`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Class {
        Npc,
        Monster,
        Other,
    }

    fn class_of(obj_type: u32, bitfield: u32) -> Class {
        const CREATURE: u32 = 0x10;
        const ATTACKABLE: u32 = 0x10;
        if obj_type & CREATURE == 0 {
            Class::Other
        } else if bitfield & ATTACKABLE != 0 {
            Class::Monster
        } else {
            Class::Npc
        }
    }

    /// Per second of the stand: (object, movement type) of every UpdateMotion, and (object,
    /// flags) of every UpdatePosition, about other objects.
    struct Stand {
        motions: Vec<Vec<(u32, u8)>>,
        positions: Vec<Vec<(u32, u32)>>,
        classes: BTreeMap<u32, (u32, Class)>,
        /// Per second: the objects the client was never created that it got updates about, with
        /// whether the player was in the object's reach (its landblock and the adjacent ones) and
        /// outside the player's create set, at the end of that second.
        unsent: Vec<Vec<(u32, bool, bool)>>,
    }

    fn stand_still(l: &mut Loop, secs: usize) -> Stand {
        let mut s = Stand {
            motions: Vec::new(),
            positions: Vec::new(),
            classes: BTreeMap::new(),
            unsent: Vec::new(),
        };
        for c in l.since::<ItemCreateObject>(0) {
            s.classes.insert(
                c.0.id.0,
                (
                    c.0.wdesc.wcid,
                    class_of(c.0.wdesc.obj_type, c.0.wdesc.bitfield),
                ),
            );
        }
        for _ in 0..secs {
            let mark = l.mark();
            l.advance(1.0);
            for c in l.since::<ItemCreateObject>(mark) {
                s.classes.insert(
                    c.0.id.0,
                    (
                        c.0.wdesc.wcid,
                        class_of(c.0.wdesc.obj_type, c.0.wdesc.bitfield),
                    ),
                );
            }
            let me = l.g.full();
            let motions = l
                .since::<MovementSetObjectMovement>(mark)
                .into_iter()
                .filter(|m| m.id.0 != me);
            s.motions.push(
                motions
                    .map(|m| {
                        (
                            m.id.0,
                            m.decoded_movement()
                                .expect("the movement buffer decodes")
                                .body
                                .movement_type,
                        )
                    })
                    .collect(),
            );
            let positions = l
                .since::<MovementPositionEvent>(mark)
                .into_iter()
                .filter(|p| p.id.0 != me);
            s.positions
                .push(positions.map(|p| (p.id.0, p.position.flags)).collect());

            let w = &l.ts.world;
            let player_phys = w.objects.get(l.g).and_then(|p| p.phys);
            let about: BTreeSet<u32> = s.motions[s.motions.len() - 1]
                .iter()
                .map(|m| m.0)
                .chain(s.positions[s.positions.len() - 1].iter().map(|p| p.0))
                .filter(|g| !s.classes.contains_key(g))
                .collect();
            s.unsent.push(
                about
                    .into_iter()
                    .map(|g| {
                        let obj = ObjectGuid::new(g);
                        let in_reach =
                            world_object_networking::reach_players(w, obj).contains(&l.g);
                        let outside_create_set =
                            match (player_phys, w.objects.get(obj).and_then(|o| o.phys)) {
                                (Some(p), Some(h)) => !object_maint::in_create_set(w, p, h),
                                _ => false,
                            };
                        (g, in_reach, outside_create_set)
                    })
                    .collect(),
            );
        }
        s
    }

    /// The chicken that crosses Holtburg's south road (ACE's world DB), and where to stand: 40 m
    /// from it and 50 m from the life stone, within `EmoteManager.ClientMaxAnimRange` (96 m) of
    /// both it and the town's NPCs.
    const CHICKEN_CROSSING_ROAD: u32 = 25578;
    const STAND_CELL: u32 = 0xA9B3_0017;
    const STAND_X: f32 = 60.0;
    const STAND_Y: f32 = 150.0;
    // (1617 since scattered spawns draw their points from the same stream)
    const SEED: u64 = 1617;

    fn teleloc(l: &mut Loop, line: &str) {
        let mark = l.mark();
        l.admin_command(line);
        let id = l.id;
        assert!(
            l.ts.run_until(5.0, |ts| !decode::all_of::<EffectsPlayerTeleport>(
                &ts.received_raw(id)[mark..]
            )
            .is_empty()),
            "the character is teleported"
        );
        l.advance(1.0);
        l.action(&CharacterLoginCompleteNotification);
    }

    /// Into Holtburg beside the life stone, then to the stand, out of portal space.
    fn arrive_in_holtburg(l: &mut Loop) {
        teleloc(l, "@teleloc 0xA9B40019 84 7.1 94");
        l.advance(2.0);
        let w = &l.ts.world;
        let chicken = landblock_manager::get_loaded_landblocks(w)
            .into_iter()
            .flat_map(|id| {
                w.landblock_manager
                    .landblocks
                    .expect(id)
                    .get_all_world_objects_for_diagnostics()
            })
            .find(|g| {
                w.objects
                    .get(*g)
                    .is_some_and(|o| o.biota.weenie_class_id == CHICKEN_CROSSING_ROAD)
            })
            .expect("the chicken that crosses the road is in Holtburg");
        let z = l.location_of(chicken).position_z;
        teleloc(
            l,
            &format!(
                "@teleloc 0x{STAND_CELL:08X} {STAND_X} {STAND_Y} {}",
                z + 0.5
            ),
        );
    }

    #[test]
    fn a_client_standing_in_holtburg_sees_npcs_and_creatures_move() {
        let mut l = create_and_enter();
        arrive_in_holtburg(&mut l);
        // the heartbeat emotes draw from the world thread's ThreadSafeRandom: a fixed seed keeps
        // which emotes run (and so which creatures walk) the same from run to run
        empyrean_common::thread_safe_random::ThreadSafeRandom::seed(SEED);
        let s = stand_still(&mut l, 60);

        let class = |g: u32| s.classes.get(&g).map(|c| c.1);
        let mut motion_count: BTreeMap<(Class, u8), usize> = BTreeMap::new();
        let mut unknown = BTreeSet::new();
        for &(g, mt) in s.motions.iter().flatten() {
            match class(g) {
                Some(c) => *motion_count.entry((c, mt)).or_insert(0) += 1,
                None => {
                    unknown.insert(g);
                }
            }
        }
        let mut position_count: BTreeMap<Class, usize> = BTreeMap::new();
        for &(g, _) in s.positions.iter().flatten() {
            match class(g) {
                Some(c) => *position_count.entry(c).or_insert(0) += 1,
                None => {
                    unknown.insert(g);
                }
            }
        }
        println!("UpdateMotion (class, movement type): {motion_count:?}");
        println!("UpdatePosition per class: {position_count:?}");
        // Retail's reach, not ACE's (V260/V278/V286/V287/V308, V286): updates go to every player in the
        // object's landblock and its adjacent ones, whether or not it was created for them; the
        // creates follow the player's create set (V260/V278/V286/V287/V308, V287), which is smaller. So an
        // update may name an object the client was never sent (our client holds such updates for
        // a later create), but only one whose reach holds the player and that lies outside the
        // player's create set (inside it, the client would have been sent its create).
        let unsent: BTreeMap<u32, (bool, bool)> = s
            .unsent
            .iter()
            .flatten()
            .map(|&(g, r, o)| (g, (r, o)))
            .collect();
        assert_eq!(
            unknown,
            unsent.keys().copied().collect::<BTreeSet<u32>>(),
            "the unsent objects are tracked"
        );
        let stray: Vec<String> = s
            .unsent
            .iter()
            .flatten()
            .filter(|&&(_, in_reach, outside)| !(in_reach && outside))
            .map(|(g, r, o)| format!("{g:08X} (in reach {r}, outside the create set {o})"))
            .collect();
        assert!(
            stray.is_empty(),
            "updates about unsent objects come only from objects in reach and outside the create set: {stray:?}"
        );
        println!("updates about objects in reach but never created: {unknown:08X?}");

        // NPCs: idle motions (the interpreted arm, a motion out and Ready back) from several of
        // them, as ACE's Holtburg recordings show (3 NPCs, a few motions each per minute)
        let npc_idle: Vec<u32> = s
            .motions
            .iter()
            .flatten()
            .filter(|(g, mt)| class(*g) == Some(Class::Npc) && *mt == movement_type::INVALID)
            .map(|m| m.0)
            .collect();
        let npcs: BTreeSet<u32> = npc_idle.iter().copied().collect();
        assert!(
            npcs.len() >= 2 && npc_idle.len() >= 4,
            "idle motions from NPCs: {} from {} NPCs",
            npc_idle.len(),
            npcs.len()
        );

        // a creature walked somewhere by its heartbeat emote: a MoveToPosition, then an
        // UpdatePosition every monster tick while it walks (5 a second), on the ground. ACE's
        // recordings carry the same flags for these (grounded, no velocity: the body moves by its
        // animation, `PhysicsObj.Velocity` stays zero)
        let walkers: BTreeSet<u32> = s
            .motions
            .iter()
            .flatten()
            .filter(|(g, mt)| {
                class(*g) == Some(Class::Monster) && *mt == movement_type::MOVE_TO_POSITION
            })
            .map(|m| m.0)
            .collect();
        assert!(
            walkers.iter().any(|g| s
                .classes
                .get(g)
                .is_some_and(|c| c.0 == CHICKEN_CROSSING_ROAD)),
            "the chicken crosses the road (its heartbeat emote's MoveToPosition)"
        );
        // the chicken's busiest second: one UpdatePosition per monster tick. (A MoveTo issued
        // while another is under way stacks a second `AddMoveToTick` chain; ACE's recordings show
        // such bursts too, up to 15 a second, so other walkers are not bounded here.)
        let chicken_busiest = walkers
            .iter()
            .filter(|g| {
                s.classes
                    .get(g)
                    .is_some_and(|c| c.0 == CHICKEN_CROSSING_ROAD)
            })
            .map(|w| {
                s.positions
                    .iter()
                    .map(|sec| sec.iter().filter(|(g, _)| g == w).count())
                    .max()
                    .unwrap_or(0)
            })
            .max()
            .unwrap_or(0);
        assert!(
            (4..=6).contains(&chicken_busiest),
            "a walking creature's UpdatePosition every 0.2 s (AddMoveToTick): {chicken_busiest} in its busiest second"
        );
        let walking: Vec<u32> = s
            .positions
            .iter()
            .flatten()
            .filter(|(g, _)| walkers.contains(g))
            .map(|p| p.1)
            .collect();
        assert!(
            walking.iter().all(|f| f & position_flags::IS_GROUNDED != 0),
            "a walking creature's positions are grounded: {walking:X?}"
        );

        // rough rates against ACE's recordings of the same kinds of objects: ambient
        // UpdatePosition and UpdateMotion stay within tens to a few hundred a minute
        let positions: usize = s.positions.iter().map(Vec::len).sum();
        let motions: usize = s.motions.iter().map(Vec::len).sum();
        assert!(
            (10..=1000).contains(&positions),
            "ambient UpdatePosition in a minute: {positions}"
        );
        assert!(
            (10..=500).contains(&motions),
            "ambient UpdateMotion in a minute: {motions}"
        );
    }
}
