//! The client's objects as the scene keeps them with the client, in recorded 2017 sessions: each
//! body made through the client from the physics record its object's repository names, the cell
//! each stands in told to the client, the camera's stand-ins and the objects the client made among
//! the client's objects, and the region the player's creation names taken up.
//! Fixture: the end-of-retail install and the capture index.

use auberean_client_sdk::model::ids;
use auberean_client_sdk::runtime::world::{PhysicsObjOrder, WorldScene};
use auberean_scene::others::is_loaded;

use crate::combat::{played_in_frames, FRAME};

/// A session that fights at once: a human among creatures that shoot.
const FIGHT: &str = "1e177f6bfede2082:3";

/// The physics record the client names for an object's physics object, as a scene would be asked.
struct Named(Option<u32>);

impl WorldScene for Named {
    fn make_physics_obj(&mut self, order: &PhysicsObjOrder) -> bool {
        self.0 = Some(order.physics_record);
        true
    }
    fn is_static(&self, _: u64) -> bool {
        false
    }
    fn has_parent(&self, _: u64) -> bool {
        false
    }
    fn leave_visibility(&mut self, _: u64) {}
    fn refresh_landscape(&mut self) -> bool {
        false
    }
}

/// Whether the scene draws object `o` as a body of its own.
fn drawn(o: &auberean_client_sdk::model::Object) -> bool {
    o.in_world && o.place.is_some() && o.parent.is_none() && o.container.is_none()
}

/// Two minutes of a 2017 session played through the client: after every frame, the client knows
/// the cell each body the scene draws stands in (none for one lost in a cell not loaded), every body
/// in the world stands in a cell the scene holds, and the camera's two stand-ins are among the
/// client's objects, with a physics object and no game object, from the player's arrival. After
/// it, every body was made from the physics record the client names (its repository's, which for
/// many objects is their template's and not their own), the region the player's creation named
/// was taken up, and the world's refresh makes the landscape again.
///
/// Behaviour: desktop.world.the-client-knows-where-the-scenes-bodies-stand
#[test]
fn the_client_knows_where_each_body_stands_and_names_its_physics_record() {
    let (mut told, mut lost, mut stand_ins) = (0u64, 0u64, 0u64);
    let (mut c, mut scene, _) = played_in_frames(FIGHT, 120.0, FRAME, &[], &mut |c, s| {
        let Some(player) = c.model.player.filter(|p| s.body(*p).is_some()) else {
            return;
        };
        let around = s.body(player).map(|b| b.position.cell.0);
        for o in c.model.objects().filter(|o| drawn(o)) {
            if let Some(b) = s.body(o.id) {
                let cell = b.position.cell.0;
                assert_eq!(c.queue.cell_of(o.id), Some(cell), "{:#x}", o.id);
                assert!(
                    o.id == player || is_loaded(cell, around, s.land()),
                    "{:#x} stands in {cell:#010x}, which is not loaded",
                    o.id
                );
                told += 1;
            } else if s.lost_cell(o.id) != 0 {
                assert_eq!(c.queue.cell_of(o.id), Some(0), "{:#x} is lost", o.id);
                lost += 1;
            }
        }
        if s.has_player() {
            for id in [ids::camera_parent(), ids::camera_target()] {
                assert_eq!(c.queue.has_game_object(id), Some(false), "{id:#x}");
                assert_eq!(c.queue.has_physics_object(id), Some(true), "{id:#x}");
            }
            stand_ins += 1;
        }
    });
    eprintln!("{told} bodies' cells told, {lost} lost, {stand_ins} frames with the stand-ins");
    assert!(
        told > 30_000 && lost > 1_000 && stand_ins > 3_000,
        "{told} {lost} {stand_ins}"
    );
    let ids: Vec<u64> = c.model.objects().map(|o| o.id).collect();
    let (mut made, mut templates) = (0, 0);
    for id in ids {
        let Some(record) = scene.physics_record(id) else {
            continue;
        };
        let mut named = Named(None);
        assert!(c.create_physics_obj(id, &mut named));
        assert_eq!(Some(record), named.0, "{id:#x}");
        let own = c
            .model
            .object(id)
            .and_then(|o| o.qualities.data_ids.get(&1).copied())
            .unwrap_or(0);
        made += 1;
        templates += usize::from(record != own);
    }
    eprintln!("{made} physics objects, {templates} from a record not their own quality");
    assert!(made >= 10 && templates >= 5, "{made} {templates}");
    assert_eq!(c.world.take_region_change(), None, "the region taken up");
    assert_eq!(c.world.region(), 1);
    // The world's refresh makes the landscape's frame again.
    assert!(scene.frame(c.local_time()).is_some() && scene.world_built());
    assert!(c.world.refresh(&mut scene));
    assert!(!scene.world_built(), "made again at the next frame");
}

/// Four minutes of a 2017 session in which the player shoots its bow: every object the client
/// makes itself (each arrow) is among the client's objects while the scene has it, with a physics
/// object and no game object, and its timeout as its lifetime; once the scene lets it go, the
/// client has it no more. One the client is asked to destroy goes from the client a quarter of a
/// second later, and from the scene with it.
///
/// Behaviour: desktop.world.the-objects-the-client-makes-are-among-its-objects
#[test]
fn the_objects_the_client_makes_are_among_its_objects_while_they_last() {
    // The ids the scene had in the last frame (an id given back is taken again by the next made).
    let mut had: Vec<u64> = Vec::new();
    let (mut appeared, mut frames, mut timed) = (0u64, 0u64, 0u64);
    // The first arrow made: asked to be destroyed when made, and when it went.
    let mut asked: Option<(u64, f64)> = None;
    let mut went = None;
    let (c, scene, _) = played_in_frames("aae4d0e791da97b9:1", 240.0, FRAME, &[], &mut |c, s| {
        let made = s.made_objects();
        for id in &made {
            assert_eq!(c.queue.has_game_object(*id), Some(false), "{id:#x}");
            assert_eq!(c.queue.has_physics_object(*id), Some(true), "{id:#x}");
            timed += u64::from(c.queue.temporary_ends(*id).is_some());
            appeared += u64::from(!had.contains(id));
        }
        for id in had.iter().filter(|id| !made.contains(id)) {
            assert!(!c.queue.has_object(*id), "{id:#x} let go of");
        }
        frames += u64::from(!made.is_empty());
        let now = c.local_time();
        match asked {
            None => {
                if let Some(id) = made.first() {
                    c.queue.add_object_to_be_destroyed(*id, now);
                    asked = Some((*id, now));
                }
            }
            Some((id, at)) if went.is_none() && !made.contains(&id) => {
                assert!(!c.queue.has_object(id));
                went = Some(now - at);
            }
            Some(_) => {}
        }
        had = made;
    });
    eprintln!("{appeared} objects made, {frames} frames with some, {timed} with a lifetime");
    assert!(
        appeared >= 13 && frames > 100 && timed > 0,
        "{appeared} {frames} {timed}"
    );
    assert!(scene
        .made_objects()
        .iter()
        .all(|id| c.queue.has_object(*id)));
    let went = went.expect("the arrow asked to be destroyed went");
    assert!((0.25..0.25 + 2.0 * FRAME).contains(&went), "{went} s on");
}

/// The player's arrival has its scripts preload what they keep about the player: by the first frame
/// the scene has the player, the master skill list (the scripts' singleton of package 5858) is made.
///
/// Behaviour: desktop.world.the-players-arrival-preloads-its-scripts-singletons
#[test]
fn the_players_arrival_preloads_the_master_skill_list() {
    const MASTER_SKILL_LIST: u32 = 5858;
    let mut checked = false;
    played_in_frames(FIGHT, 10.0, FRAME, &[], &mut |c, s| {
        if checked || !s.has_player() {
            return;
        }
        checked = true;
        let heap = &mut c.scripts.as_mut().expect("the scripts run").vm.heap;
        let before = heap.singleton_ids().count();
        heap.singleton(auberean_client_sdk::script::library::STATE_BASE | MASTER_SKILL_LIST);
        assert_eq!(heap.singleton_ids().count(), before, "made already");
    });
    assert!(checked, "the player arrived");
}

/// A minute of a 2017 fight: each body the server asks into a mode (its creatures into combat and
/// out, the dead into death) tells its object the mode, as a notice about the object; and every
/// mode the server asked of a body the scene has is among those told.
///
/// Behaviour: desktop.world.an-object-is-told-the-mode-its-body-goes-into
#[test]
fn an_object_is_told_each_mode_its_body_goes_into() {
    use auberean_client_sdk::protocol::ServerMessage;
    let mut asked: Vec<(u64, u32)> = Vec::new();
    let mut had = std::collections::BTreeSet::new();
    let (_, _, r) = played_in_frames(FIGHT, 60.0, FRAME, &[], &mut |c, s| {
        had.extend(
            c.model
                .objects()
                .map(|o| o.id)
                .filter(|id| s.figure(*id).is_some()),
        );
    });
    let index = auberean_testkit::data::require_capture_index();
    let mut entered = None;
    auberean_testkit::captures::capture_messages_in_time_order(
        &index,
        &[0x1_0060, 0x1_0082],
        FIGHT,
        |m| match ServerMessage::decode(&m.raw) {
            Ok(ServerMessage::PlayerObject(_)) => {
                entered.get_or_insert(m.time);
            }
            Ok(ServerMessage::DoMode(e)) if entered.is_some_and(|t| m.time <= t + 50.0) => {
                asked.push((e.sender.id, e.action.0));
            }
            _ => {}
        },
    )
    .expect("the index reads");
    eprintln!("{} modes told, {} asked", r.modes.len(), asked.len());
    assert!(r.modes.len() >= 5, "{:x?}", r.modes);
    let told: std::collections::BTreeSet<(u64, u32)> = r.modes.iter().copied().collect();
    let missed: Vec<&(u64, u32)> = asked
        .iter()
        .filter(|(id, _)| had.contains(id))
        .filter(|m| !told.contains(m))
        .collect();
    assert!(missed.is_empty(), "{missed:x?}");
}

/// Each body is drawn once a frame: it stands in one of the scene's lists (the player, the others,
/// what they hold, what the client made), however many cells it reaches into, so nothing has to ask
/// whether it has been drawn already. (The 2017 fight and the bow session replayed.)
///
/// Behaviour: desktop.bodies.each-body-is-drawn-once-a-frame
#[test]
fn each_body_is_drawn_once_a_frame() {
    for session in ["1e177f6bfede2082:3", "aae4d0e791da97b9:1"] {
        let (mut frames, mut bodies) = (0u64, 0u64);
        played_in_frames(session, 240.0, FRAME, &[], &mut |c, s| {
            if s.frame(c.local_time()).is_none() {
                return;
            }
            let mut ids: Vec<u64> = s.seen.bodies.iter().map(|b| b.id).collect();
            frames += 1;
            bodies += ids.len() as u64;
            ids.sort_unstable();
            let n = ids.len();
            ids.dedup();
            assert_eq!(ids.len(), n, "a body drawn twice in {session}");
        });
        eprintln!("{session}: {frames} frames, {bodies} bodies");
        assert!(frames > 100 && bodies > 1000, "{frames} {bodies}");
    }
}

/// Four minutes of a 2017 session in which the player shoots its bow, its frames drawn: each body
/// drawn is seen as faded out exactly when its figure's opacity (its fade slider, held to 0..1)
/// is under 0.0002 (the arrow on the string is hidden as it is let go of); a figure at that
/// opacity or more is seen.
///
/// Behaviour: desktop.bodies.a-body-faded-out-is-not-seen
#[test]
fn a_body_faded_out_by_its_animation_is_seen_as_faded() {
    use auberean_physics::body::parent::FADE_SLIDER;
    use auberean_scene::is_visible;
    let (mut drawn, mut faded) = (0u64, 0u64);
    let mut figure = None;
    played_in_frames("aae4d0e791da97b9:1", 240.0, FRAME, &[], &mut |c, s| {
        if s.frame(c.local_time()).is_none() {
            return;
        }
        for b in &s.seen.bodies {
            let f = s.figure(b.id);
            assert_eq!(b.faded, f.is_some_and(|f| !is_visible(f)), "{:#x}", b.id);
            drawn += 1;
            faded += u64::from(b.faded);
            if figure.is_none() {
                figure = f.cloned();
            }
        }
    });
    eprintln!("{drawn} bodies drawn, {faded} faded");
    assert!(drawn > 1000 && faded > 0, "{drawn} {faded}");
    let mut f = figure.expect("a figure");
    for (opacity, seen) in [
        (1.0, true),
        (0.0001, false),
        (0.5, true),
        (0.0002, true),
        (-0.5, false),
    ] {
        f.sliders.do_slider(0, FADE_SLIDER, opacity, 0.0);
        assert_eq!(is_visible(&f), seen, "{opacity}");
    }
}
