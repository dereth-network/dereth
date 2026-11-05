//! The effects objects play from their looks, read from the install.

use auberean_assets::fx::Particles;
use auberean_draw::effects::{
    node_for, particle_is_looping, FxSpot, FxWho, NoRig, ObjectEffects, Rig,
};
use auberean_draw::particles::approximate_lifetime;
use auberean_draw::systems::ParticleSystems;
use auberean_primitives::{DataId, Vec3};
use auberean_world::color::Color;
use auberean_world::look::Look;
use auberean_world::math::Mat4;

use crate::common::{ids_of, install};

/// The camp's Arcane Fire (look `0x1F0001D3`) burns blue: its look names an effect table whose
/// five startup effects are orange flames, and its appearance at full strength lists blue ones
/// for the same five ids, which join its table as it starts, in place of the stored ones.
///
/// Behaviour: draw.effects.an-appearances-effects-play-in-place-of-its-tables
#[test]
fn the_arcane_fire_burns_its_appearances_blue_flames() {
    use auberean_draw::effects::ObjectFxTable;
    let r = &install().records;
    let look = Look::of_record(r, DataId(0x1F00_01D3)).expect("the arcane fire's look");
    let systems_of = |t: &ObjectFxTable| -> Vec<u32> {
        let mut d: Vec<u32> = (0x8100_0069..=0x8100_006D)
            .flat_map(|e| t.fx_data(r, e))
            .flat_map(|data| node_for(&data, 0).particles.clone().unwrap_or_default())
            .map(|id| id.0)
            .collect();
        d.sort_unstable();
        d
    };
    assert_eq!(
        systems_of(&ObjectFxTable::of_group(&look.global)),
        vec![
            0x3900_0126,
            0x3900_0127,
            0x3900_0128,
            0x3900_0129,
            0x3900_0140
        ],
        "the stored table's own"
    );
    assert_eq!(
        systems_of(&ObjectFxTable::of_group_with(r, &look.global)),
        vec![
            0x3900_0128,
            0x3900_0136,
            0x3900_0137,
            0x3900_0138,
            0x3900_0139
        ],
        "the appearance's blue systems"
    );
    let manager = ParticleSystems::default();
    let place = Mat4::translation(Vec3::new(130.0, 90.0, 82.0));
    let mut fx = ObjectEffects::of_look(&look).expect("its look carries effects");
    fx.start(
        &manager,
        r,
        (0xC1, 0x56),
        &place,
        Vec3::new(1.0, 1.0, 1.0),
        0.0,
    );
    let mut started: Vec<u32> = fx.systems().iter().map(|c| c.desc).collect();
    started.sort_unstable();
    assert_eq!(
        started,
        vec![
            0x3900_0128,
            0x3900_0136,
            0x3900_0137,
            0x3900_0138,
            0x3900_0139
        ],
        "started blue"
    );
}

/// The camp's Arcane Fire burns over the lake: its blue flames are drawn after the water. A frame
/// draws its objects not seen through, then the water, then the frills, what is seen through and
/// the particle systems, last of all. Every quad of the fire's five systems adds itself onto what
/// lies behind without marking how near it is, so water drawn after them would cover each flame
/// with the lake behind it.
///
/// Behaviour: draw.frame.the-water-is-drawn-before-what-is-seen-through
#[test]
fn the_arcane_fires_flames_are_drawn_after_the_water() {
    use auberean_draw::scene::{Pass, PASSES};
    let step = |p: Pass| PASSES.iter().position(|q| *q == p).expect("a step");
    assert!(
        step(Pass::Opaque) < step(Pass::Water),
        "the opaque objects first"
    );
    assert!(
        step(Pass::Water) < step(Pass::Frills)
            && step(Pass::Frills) < step(Pass::Translucent)
            && step(Pass::Translucent) < step(Pass::Particles),
        "the water, then the frills, what is seen through and the particles"
    );
    let r = &install().records;
    let look = Look::of_record(r, DataId(0x1F00_01D3)).expect("the arcane fire's look");
    let manager = ParticleSystems::default();
    let home = (0xC1, 0x56);
    let place = Mat4::translation(Vec3::new(122.8, 89.8, 82.2));
    let one = Vec3::new(1.0, 1.0, 1.0);
    let mut fx = ObjectEffects::of_look(&look).expect("its look carries effects");
    fx.start(&manager, r, home, &place, one, 0.0);
    let eye = Vec3::new(126.0, 84.0, 82.0);
    let axes = (
        Vec3::new(0.85, 0.53, 0.0),
        Vec3::new(-0.53, 0.85, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    );
    let mut t = 0.0;
    while t < 3.0 {
        t += 0.05;
        fx.think(&manager, home, &place, one, Color::WHITE, t);
    }
    let batches = manager.batches(home, 192.0, t, eye, axes);
    let quads: usize = batches.iter().map(|b| b.vertices.len() / 4).sum();
    assert!(quads > 50, "the fire burns: {quads} quads");
    for b in &batches {
        assert!(!b.layer.depth_write, "a flame marks no nearness");
        assert_eq!(b.pass(), Pass::Particles, "a flame is a particle's");
    }
}

/// A weapon's look makes its streak as it is made (the rest of its startup effects: the line along
/// its blade, white, trailing a fifth of a second), waiting to be turned on; a human's swings turn
/// the streaks of what it holds at a holding location on and off, as the common effect table's
/// entries say (`0x810002AE` on at location 1, `0x810002AF` off there). With the streaks preference
/// off no streak is made and none turned on; turning them off is still asked.
///
/// Behaviour: draw.streak.a-weapon-carries-a-streak-a-swing-turns-on
#[test]
fn a_weapons_streak_waits_for_a_swing_to_turn_it_on() {
    use auberean_draw::effects::{FxSpot, FxWho, NoRig};
    let r = &install().records;
    let unit = Vec3::new(1.0, 1.0, 1.0);
    let weapon = Look::of_record(r, DataId(0x1F00_0056)).expect("a weapon's look");
    let systems = ParticleSystems::default();
    let mut fx = ObjectEffects::of_look(&weapon).expect("its effects");
    fx.start(&systems, r, (0, 0), &Mat4::IDENTITY, unit, 0.0);
    assert_eq!(fx.streaks().len(), 1, "its streak");
    assert!(!fx.streaks()[0].active(), "waiting to be turned on");
    assert_eq!(fx.streaks()[0].material(), 0x3100_0200);
    fx.turn_streaks(true);
    assert!(fx.streaks()[0].active());
    let human = Look::of_record(r, DataId(0x1F00_0023)).expect("the human male's look");
    let spot = FxSpot {
        home: (0, 0),
        place: Mat4::IDENTITY,
        scale: unit,
        now: 1.0,
    };
    let mut swinger = ObjectEffects::of_look(&human).expect("its effects");
    let (took, _) = swinger.do_fx(
        &systems,
        r,
        0x8100_02AE,
        1.0,
        &spot,
        FxWho::default(),
        &NoRig,
    );
    assert!(took);
    let (took, _) = swinger.do_fx(
        &systems,
        r,
        0x8100_02AF,
        1.0,
        &spot,
        FxWho::default(),
        &NoRig,
    );
    assert!(took);
    assert_eq!(swinger.take_streak_asks(), [(true, 1), (false, 1)]);
    // the preference off
    let off = ParticleSystems::default();
    off.set_streak_effects(false);
    let mut fx = ObjectEffects::of_look(&weapon).expect("its effects");
    fx.start(&off, r, (0, 0), &Mat4::IDENTITY, unit, 0.0);
    assert!(fx.streaks().is_empty(), "no streak made");
    let mut swinger = ObjectEffects::of_look(&human).expect("its effects");
    let _ = swinger.do_fx(&off, r, 0x8100_02AE, 1.0, &spot, FxWho::default(), &NoRig);
    let _ = swinger.do_fx(&off, r, 0x8100_02AF, 1.0, &spot, FxWho::default(), &NoRig);
    assert_eq!(swinger.take_streak_asks(), [(false, 1)]);
}

/// The start hall's quick exit (look `0x1F00031E`) plays three startup effects from its look:
/// two swirls, a purple and an orange particle system a meter below the object's origin, and a
/// sound. Nothing on the wire asks for them.
///
/// Behaviour: draw.effects.an-object-plays-its-looks-startup-effects-when-it-is-made
#[test]
fn the_quick_exit_plays_its_swirls_from_its_look() {
    let r = &install().records;
    let look = Look::of_record(r, DataId(0x1F00_031E)).expect("the quick exit's look");
    let mut fx = ObjectEffects::of_look(&look).expect("its look carries effects");
    assert_eq!(fx.table().stored(), DataId(0x1E00_00E0));
    let place = Mat4::translation(Vec3::new(101.0, 240.1, -43.52));
    let manager = ParticleSystems::default();
    fx.start(
        &manager,
        r,
        (2, 0x98),
        &place,
        Vec3::new(1.0, 1.0, 1.0),
        0.0,
    );
    let systems: Vec<(u32, Vec3)> = fx
        .systems()
        .iter()
        .map(|c| (c.desc, c.offset.point(Vec3::ZERO)))
        .collect();
    assert_eq!(
        systems,
        vec![
            (0x3900_00C9, Vec3::new(0.0, 0.0, -1.0)),
            (0x3900_00C8, Vec3::new(0.0, 0.0, -1.0)),
        ]
    );
    let states: Vec<(u32, bool, usize)> = fx
        .states()
        .iter()
        .map(|(id, s)| (*id, s.startup, s.systems.len()))
        .collect();
    // The swirls run until stopped, so their effects keep them; the sound starts no system.
    assert_eq!(
        states,
        vec![
            (0x8100_0006, true, 0),
            (0x8100_01C3, true, 1),
            (0x8100_01C4, true, 1)
        ]
    );
    // Run a few seconds where it stands: both swirls have particles to draw.
    for k in 1..=60 {
        fx.think(
            &manager,
            (2, 0x98),
            &place,
            Vec3::new(1.0, 1.0, 1.0),
            auberean_world::color::Color::WHITE,
            f64::from(k) * 0.05,
        );
    }
    assert_eq!(fx.systems().len(), 2);
    assert!(fx.systems().iter().all(|c| manager.alive(&c.system) > 0));
}

/// The quick exit's swirls light nothing and live with their object, so objects placed with the
/// world share them: a second quick exit placed with the world draws the first one's two systems
/// at its own place, every particle twice; an object that is not placed with the world (one the
/// server makes) takes a shared system too, but then forbids sharing it, so a third placed quick
/// exit sets new ones going. A system its last owner lets go of discharges no more; one that may
/// not be shared draws where it was left until its particles are gone.
///
/// Behaviour: draw.systems.objects-placed-with-the-world-share-their-particle-systems
#[test]
fn objects_placed_with_the_world_share_their_particle_systems() {
    let r = &install().records;
    let look = Look::of_record(r, DataId(0x1F00_031E)).expect("the quick exit's look");
    let systems = ParticleSystems::default();
    let one = Vec3::new(1.0, 1.0, 1.0);
    let light = auberean_world::color::Color::WHITE;
    let make = |shared: bool, x: f32| {
        let mut fx = ObjectEffects::of_look(&look).expect("its look carries effects");
        fx.shared = shared;
        let place = Mat4::translation(Vec3::new(x, 240.0, -43.5));
        fx.start(&systems, r, (2, 0x98), &place, one, 0.0);
        (fx, place)
    };
    let ids =
        |fx: &ObjectEffects| -> Vec<u32> { fx.systems().iter().map(|c| c.system.id()).collect() };
    let (mut a, pa) = make(true, 100.0);
    let (mut b, pb) = make(true, 110.0);
    assert_eq!(
        ids(&a),
        ids(&b),
        "the second placed quick exit takes the first's systems"
    );
    assert_eq!(systems.counts(), (2, 0));
    assert!(a.systems().iter().all(|c| systems.owners(&c.system) == 2));
    let eye = Vec3::new(105.0, 230.0, -43.0);
    let axes = (
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    );
    let mut drawn = (0, 0);
    // Frames a little longer than the step, so the systems think at the first queue of a frame.
    for k in 1..=40 {
        let now = f64::from(k) * 0.0525;
        a.think(&systems, (2, 0x98), &pa, one, light, now);
        let alone: usize = systems
            .batches((2, 0x98), 192.0, now, eye, axes)
            .iter()
            .map(|b| b.vertices.len())
            .sum();
        a.think(&systems, (2, 0x98), &pa, one, light, now + 0.01);
        b.think(&systems, (2, 0x98), &pb, one, light, now + 0.01);
        let both: usize = systems
            .batches((2, 0x98), 192.0, now + 0.01, eye, axes)
            .iter()
            .map(|b| b.vertices.len())
            .sum();
        drawn = (alone, both);
    }
    assert!(drawn.0 > 0 && drawn.1 >= 2 * drawn.0 - 8, "{drawn:?}");
    // One the server makes takes them, and then they are no longer shared.
    let (c, _) = make(false, 120.0);
    assert_eq!(ids(&c), ids(&a));
    let (d, _) = make(true, 130.0);
    assert!(
        ids(&d).iter().all(|k| !ids(&a).contains(k)),
        "{:?} {:?}",
        ids(&d),
        ids(&a)
    );
    assert_eq!(systems.counts(), (4, 0));
    // Let go of: the unshared pair keeps drawing where it was until its particles are gone; the
    // new pair, with no particles yet, goes at once.
    drop((a, b, c, d));
    assert_eq!(systems.counts(), (0, 2));
    let mut t = 2.0;
    while systems.counts().1 > 0 && t < 60.0 {
        t += 0.05;
        let _ = systems.batches((2, 0x98), 192.0, t, eye, axes);
    }
    assert_eq!(systems.counts(), (0, 0), "gone by {t:.1} s");
}

/// Every look whose whole-object group names startup effects puts together, and each entry of
/// those effects that names a particle system sets one going (every system named exists); nearly
/// all of them run until stopped.
///
/// Behaviour: draw.effects.an-object-plays-its-looks-startup-effects-when-it-is-made
#[test]
fn every_look_with_startup_effects_sets_its_systems_going() {
    let r = &install().records;
    let (mut looks, mut with_systems, mut systems, mut endless) = (0, 0, 0, 0);
    let mut missing = Vec::new();
    for id in ids_of(0x1F) {
        let Ok(look) = Look::of_record(r, DataId(id)) else {
            continue;
        };
        if look.global.startup_fx.is_empty() {
            continue;
        }
        let Some(mut fx) = ObjectEffects::of_look(&look) else {
            continue;
        };
        looks += 1;
        let manager = ParticleSystems::default();
        fx.start(
            &manager,
            r,
            (0, 0),
            &Mat4::IDENTITY,
            Vec3::new(1.0, 1.0, 1.0),
            0.0,
        );
        // Every entry of every startup effect (its table as started) that names a particle system
        // sets one going.
        let named: usize = look
            .global
            .startup_fx
            .entries
            .iter()
            .flat_map(|(effect, _)| fx.table().fx_data(r, *effect))
            .filter(|d| {
                node_for(d, 0)
                    .particles
                    .as_ref()
                    .is_some_and(|p| !p.is_empty())
            })
            .count();
        if fx.systems().len() != named {
            missing.push(id);
        }
        if !fx.systems().is_empty() {
            with_systems += 1;
        }
        for c in fx.systems() {
            systems += 1;
            if particle_is_looping(r, c.desc) {
                endless += 1;
            }
        }
    }
    println!("{looks} looks with startup effects, {with_systems} set systems going: {systems} systems, {endless} endless");
    assert!(missing.is_empty(), "{missing:08X?}");
    assert!(looks > 1_000, "{looks}");
    assert!(with_systems > 600, "{with_systems}");
    assert!(endless * 10 > systems * 9, "{endless} of {systems}");
}

/// Every particle system in the data either runs until it is stopped or lasts a time its
/// emitters give.
///
/// Behaviour: draw.particles.every-stored-system-runs-until-stopped-or-for-a-time
#[test]
fn every_particle_system_has_a_lifetime() {
    let r = &install().records;
    let (mut n, mut endless) = (0, 0);
    for id in ids_of(0x39) {
        let p: Particles = r.get(id).unwrap_or_else(|| panic!("{id:08X} decodes"));
        n += 1;
        let t = approximate_lifetime(&p);
        assert!(t.is_finite(), "{id:08X}: {t}");
        if t < 0.0 {
            endless += 1;
        }
    }
    println!("{n} particle systems, {endless} run until stopped");
    assert!(n > 1_000, "{n}");
    // About two in three run until stopped.
    assert!(
        endless * 100 >= n * 60 && endless * 100 <= n * 75,
        "{endless} of {n}"
    );
}

/// A system set going on its own, owned by nothing, draws where it was set until it is spent and
/// its particles are dead, then goes: the first of the install's systems that lasts a few
/// seconds; a record that is not a system sets nothing going.
///
/// Behaviour: draw.systems.a-system-set-going-on-its-own-draws-until-it-is-spent
#[test]
fn a_system_set_going_on_its_own_draws_until_it_is_spent() {
    let r = &install().records;
    let (id, life) = ids_of(0x39)
        .into_iter()
        .find_map(|id| {
            let p: Particles = r.get(id)?;
            let t = approximate_lifetime(&p);
            (p.material.0 != 0 && (1.0..5.0).contains(&t) && !p.emitters.is_empty())
                .then_some((id, t))
        })
        .expect("a system lasting a few seconds");
    let systems = ParticleSystems::default();
    let one = Vec3::new(1.0, 1.0, 1.0);
    let place = Mat4::translation(Vec3::new(100.0, 240.0, -43.5));
    assert!(!systems.create_autonomous(r, 0x3900_0000 - 1, (2, 0x98), place, one, 1.0, 0.0));
    assert!(systems.create_autonomous(r, id, (2, 0x98), place, one, 1.0, 0.0));
    assert_eq!(systems.counts(), (0, 1));
    let eye = Vec3::new(100.0, 230.0, -43.0);
    let axes = (
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    );
    let (mut t, mut most) = (0.0, 0);
    while systems.counts().1 > 0 && t < 60.0 {
        t += 0.05;
        let drawn: usize = systems
            .batches((2, 0x98), 192.0, t, eye, axes)
            .iter()
            .map(|b| b.vertices.len())
            .sum();
        most = most.max(drawn);
    }
    println!("{id:08X}: lasts about {life:.2} s, gone by {t:.2} s, at most {most} vertices");
    assert!(most > 0, "{id:08X} drew nothing");
    assert_eq!(systems.counts(), (0, 0), "{id:08X} not gone by {t:.1} s");
    assert!(
        t >= f64::from(life) * 0.5,
        "{id:08X} gone at {t:.2} s, lasts {life:.2} s"
    );
}

/// A rig whose every bone stands 5 m east of the object's origin.
struct FarBones;

impl Rig for FarBones {
    fn holding_location(&self, _kind: u32) -> Option<Mat4> {
        None
    }
    fn bone(&self, _kind: u32) -> Option<Mat4> {
        Some(Mat4::translation(Vec3::new(5.0, 0.0, 0.0)))
    }
    fn water_height(&self) -> Option<f32> {
        None
    }
    fn holding_bone(&self, _kind: u32) -> Option<Mat4> {
        None
    }
    fn has_figure(&self) -> bool {
        true
    }
}

/// Effect `0x40000002` of effect table `0x1E000057` (a creature's step) fires and forgets two
/// dust systems, each at a foot's bone: set going on a body whose bones stand 5 m east of it,
/// their particles are drawn there; on a body with no such bones, at the effect's offset (none:
/// the body's own place).
///
/// Behaviour: draw.effects.a-fired-system-stands-at-its-effects-holding-location-or-bone
#[test]
fn a_fired_system_stands_at_its_effects_bone() {
    let r = &install().records;
    let look = ids_of(0x1F)
        .into_iter()
        .find_map(|id| {
            let l = Look::of_record(r, DataId(id)).ok()?;
            (l.global.fx_table == DataId(0x1E00_0057)).then_some(l)
        })
        .expect("a look of the table");
    let nodes = auberean_draw::effects::effect_nodes(r, 0x1E00_0057, 0x4000_0002, 0);
    assert!(
        nodes
            .iter()
            .all(|n| n.fire_and_forget == Some(true) && n.bone.is_some_and(|b| b != 0)),
        "fired at bones: {nodes:?}"
    );
    let place = Mat4::translation(Vec3::new(100.0, 240.0, -43.5));
    let spot = FxSpot {
        home: (2, 0x98),
        place,
        scale: Vec3::new(1.0, 1.0, 1.0),
        now: 0.0,
    };
    let eye = Vec3::new(100.0, 230.0, -43.0);
    let axes = (
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    );
    let mut near = Vec::new();
    for rig in [&FarBones as &dyn Rig, &NoRig] {
        let mut fx = ObjectEffects::of_look(&look).expect("its effects");
        let systems = ParticleSystems::default();
        let (took, _) = fx.do_fx(&systems, r, 0x4000_0002, 1.0, &spot, FxWho::default(), rig);
        assert!(took, "the step plays");
        assert_eq!(systems.counts(), (0, 2), "two systems on their own");
        let b = systems.batches((2, 0x98), 192.0, 0.1, eye, axes);
        near.push((
            within(&b, Vec3::new(105.0, 240.0, -43.5), 1.0),
            within(&b, Vec3::new(100.0, 240.0, -43.5), 1.0),
        ));
    }
    assert!(near[0].0 > 0 && near[0].1 == 0, "at the bones: {near:?}");
    assert!(
        near[1].0 == 0 && near[1].1 > 0,
        "with no bones, at the body: {near:?}"
    );
}

/// The start hall's ground mist (system `0x3900018D`, particles living in the world, moved by
/// repulsors) parts around a human walking through it: every human's look carries a repulsor
/// that pushes while it moves (3 m/s² at its center, 3.5 m out). Walked 6 m across the mist at
/// a walk's pace, fewer of the mist's particles lie within 2 m of the walker than when nothing
/// walks there.
///
/// Behaviour: draw.particles.a-repulsor-pushes-the-particles-in-its-range
#[test]
fn the_start_halls_mist_parts_around_a_walker() {
    let r = &install().records;
    let human = Look::of_record(r, DataId(0x1F00_0023)).expect("the human male's look");
    let home = (2, 0x98);
    let mist_at = Mat4::translation(Vec3::new(60.0, 240.0, -44.6));
    let unit = Vec3::new(1.0, 1.0, 1.0);
    let run = |walk: bool| {
        let systems = ParticleSystems::default();
        let mist = systems
            .create(r, 0x3900_018D, false, home, mist_at, unit, 1.0, 0.0)
            .expect("the mist");
        let mut walker = ObjectEffects::of_look(&human).expect("its look carries effects");
        let start = Mat4::translation(Vec3::new(57.0, 240.0, -44.6));
        walker.start(&systems, r, home, &start, unit, 0.0);
        assert_eq!(walker.repulsor_count(), 1, "its look's repulsor");
        let mut near = 0;
        let mut at = Vec3::new(57.0, 240.0, -44.6);
        for f in 0..600_u16 {
            let t = f32::from(f) / 30.0;
            let now = f64::from(t);
            let x = (57.0 + (t - 8.0).max(0.0) * 1.5).min(63.0);
            if walk {
                at.x = x;
            }
            if walk {
                walker.think(
                    &systems,
                    home,
                    &Mat4::translation(at),
                    unit,
                    Color::WHITE,
                    now,
                );
            }
            systems.queue(&mist, home, mist_at, unit, 1.0, Color::WHITE, now);
            let batches = systems.batches(
                home,
                160.0,
                now,
                Vec3::new(60.0, 230.0, -43.0),
                (
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                    Vec3::new(0.0, 0.0, 1.0),
                ),
            );
            if f % 3 == 0 && t > 9.0 && t < 12.0 {
                near += within(&batches, Vec3::new(x, 240.0, -44.6), 2.0);
            }
        }
        near
    };
    let undisturbed = run(false);
    let parted = run(true);
    assert!(undisturbed > 0, "mist about the path");
    assert!(
        parted < undisturbed,
        "parted: {parted} near the walker against {undisturbed}"
    );
}

/// A human's repulsor pushes at the speed its physics moves it, not at how far it was carried
/// since the last frame: one running in place in the start hall's mist (its velocity a run's,
/// standing where it is) parts the mist about it; one standing with no velocity does not.
///
/// Behaviour: draw.particles.a-repulsor-moves-at-its-objects-velocity
#[test]
fn a_human_running_in_place_parts_the_mist() {
    let r = &install().records;
    let human = Look::of_record(r, DataId(0x1F00_0023)).expect("the human male's look");
    let home = (2, 0x98);
    let mist_at = Mat4::translation(Vec3::new(60.0, 240.0, -44.6));
    let unit = Vec3::new(1.0, 1.0, 1.0);
    let stands = Vec3::new(58.0, 240.0, -44.6);
    let run = |velocity: Vec3| {
        let systems = ParticleSystems::default();
        let mist = systems
            .create(r, 0x3900_018D, false, home, mist_at, unit, 1.0, 0.0)
            .expect("the mist");
        let mut runner = ObjectEffects::of_look(&human).expect("its look carries effects");
        let place = Mat4::translation(stands);
        runner.start(&systems, r, home, &place, unit, 0.0);
        let mut near = 0;
        for f in 0..450_u16 {
            let t = f32::from(f) / 30.0;
            let now = f64::from(t);
            runner.set_velocity(velocity);
            runner.think(&systems, home, &place, unit, Color::WHITE, now);
            systems.queue(&mist, home, mist_at, unit, 1.0, Color::WHITE, now);
            let batches = systems.batches(
                home,
                160.0,
                now,
                Vec3::new(60.0, 230.0, -43.0),
                (
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                    Vec3::new(0.0, 0.0, 1.0),
                ),
            );
            if f % 3 == 0 && t > 9.0 {
                near += within(&batches, stands, 2.0);
            }
        }
        near
    };
    let standing = run(Vec3::ZERO);
    let running = run(Vec3::new(4.0, 0.0, 0.0));
    assert!(standing > 0, "mist about the one standing");
    assert!(
        running < standing,
        "parted: {running} near the one running in place against {standing}"
    );
}

/// An object going stops every effect it plays, startup or asked for, with what they keep going
/// and the sounds they play.
///
/// Behaviour: draw.effects.an-object-going-stops-its-effects
#[test]
fn an_object_going_stops_its_effects() {
    use auberean_draw::effects::FxSound;
    const WOUNDED: u32 = 0x4100_0002;
    let r = &install().records;
    let mut look = Look::default();
    look.global.fx_table = DataId(0x1E00_012E);
    let spot = FxSpot {
        home: (2, 0x98),
        place: Mat4::IDENTITY,
        scale: Vec3::new(1.0, 1.0, 1.0),
        now: 0.0,
    };
    let systems = ParticleSystems::default();
    let mut fx = ObjectEffects::of_look(&look).expect("its effects");
    fx.start(&systems, r, spot.home, &spot.place, spot.scale, 0.0);
    let (took, _) = fx.do_fx(&systems, r, WOUNDED, 0.3, &spot, FxWho::default(), &NoRig);
    assert!(took, "the wound plays");
    fx.take_sounds();
    fx.destroy();
    assert!(fx.states().is_empty() && fx.systems().is_empty());
    assert!(
        fx.take_sounds()
            .contains(&FxSound::Stop { effect: WOUNDED }),
        "its sound stops too"
    );
}

/// An effect asked again while it plays moves the looping sounds it keeps to its new strength;
/// a sound it only played once is not kept, so nothing is asked of it.
///
/// Behaviour: draw.effects.an-effects-kept-sounds-follow-its-strength
#[test]
fn an_effects_kept_sounds_follow_its_strength() {
    use auberean_draw::effects::FxSound;
    const LOOPING: u32 = 0x8100_02B3;
    let r = &install().records;
    let human = Look::of_record(r, DataId(0x1F00_0023)).expect("the human male's look");
    let spot = FxSpot {
        home: (2, 0x98),
        place: Mat4::IDENTITY,
        scale: Vec3::new(1.0, 1.0, 1.0),
        now: 0.0,
    };
    let systems = ParticleSystems::default();
    let mut fx = ObjectEffects::of_look(&human).expect("its effects");
    let (took, _) = fx.do_fx(&systems, r, LOOPING, 1.0, &spot, FxWho::default(), &NoRig);
    assert!(took, "the effect plays");
    let first = fx.take_sounds();
    assert!(
        first
            .iter()
            .any(|s| matches!(s, FxSound::Play { kept: true, .. })),
        "it keeps a sound going: {first:?}"
    );
    let _ = fx.do_fx(&systems, r, LOOPING, 0.5, &spot, FxWho::default(), &NoRig);
    assert_eq!(
        fx.take_sounds(),
        vec![FxSound::Volume {
            effect: LOOPING,
            volume: 0.5
        }]
    );
}

/// The wounded effect (`0x41000002`) of effect table `0x1E00012E` lays value 2 of four appearance
/// tables over its object, at the strength it is played at: played at 0.3 the four go on at 0.3;
/// played again at 0.6 while it plays, again at 0.6 (each time in the effect's order); its time
/// passing changes nothing (it does not ramp, and an appearance laid on runs until stopped);
/// stopped, the four keys come off. Stopped by force (its object's effects let go of), they stay.
///
/// Behaviour: draw.effects.an-effects-appearance-is-laid-over-its-object-until-it-stops
#[test]
fn the_wounded_effect_lays_its_appearance_over_its_object_until_it_stops() {
    use auberean_assets::fx::AppearanceKey;
    use auberean_draw::effects::AppearanceFx;
    const WOUNDED: u32 = 0x4100_0002;
    const TABLES: [u32; 4] = [0x2000_01CD, 0x2000_003A, 0x2000_01CE, 0x2000_0056];
    let r = &install().records;
    let mut look = Look::default();
    look.global.fx_table = DataId(0x1E00_012E);
    let spot = FxSpot {
        home: (2, 0x98),
        place: Mat4::IDENTITY,
        scale: Vec3::new(1.0, 1.0, 1.0),
        now: 0.0,
    };
    // Only the entry keyed to normal gore and red blood plays; stopping takes off all four.
    let applied = |scalar: f32| -> Vec<AppearanceFx> {
        vec![AppearanceFx::Apply {
            table: 0x2000_003A,
            key: AppearanceKey { key: 2, scalar },
        }]
    };
    let removed: Vec<AppearanceFx> = TABLES
        .iter()
        .map(|t| AppearanceFx::Remove { table: *t, key: 2 })
        .collect();
    let systems = ParticleSystems::default();
    let mut fx = ObjectEffects::of_look(&look).expect("its effects");
    fx.start(&systems, r, spot.home, &spot.place, spot.scale, 0.0);
    assert!(fx.take_appearance().is_empty(), "nothing as it starts");
    let (took, _) = fx.do_fx(&systems, r, WOUNDED, 0.3, &spot, FxWho::default(), &NoRig);
    assert!(took, "the wound plays");
    assert_eq!(fx.take_appearance(), applied(0.3));
    let (took, _) = fx.do_fx(&systems, r, WOUNDED, 0.6, &spot, FxWho::default(), &NoRig);
    assert!(took, "and plays on");
    assert_eq!(fx.take_appearance(), applied(0.6));
    fx.use_time(r, 0.5);
    fx.use_time(r, 60.0);
    assert!(
        fx.take_appearance().is_empty(),
        "the time passing changes nothing"
    );
    assert_eq!(fx.current_fx(), vec![(WOUNDED, 0.6)]);
    assert!(fx.stop_fx(r, WOUNDED, false), "stopped");
    assert_eq!(fx.take_appearance(), removed);
    let _ = fx.do_fx(&systems, r, WOUNDED, 0.3, &spot, FxWho::default(), &NoRig);
    let _ = fx.take_appearance();
    assert!(fx.stop_fx(r, WOUNDED, true), "let go of");
    assert!(
        fx.take_appearance().is_empty(),
        "let go of, it leaves its appearance"
    );
}

/// An appearance effect that ramps (its own list on the object: appearance table `0x2000003A`,
/// value 2, over a second) goes on at nothing (its key taken off) and grows by the time passed
/// over its ramp's time each time the effects have their time, laid over the object at each
/// strength, until it reaches the strength asked; asked again at less while it plays, it shrinks
/// the same way.
///
/// Behaviour: draw.effects.an-effects-appearance-is-laid-over-its-object-until-it-stops
#[test]
fn a_ramping_appearance_grows_by_the_time_passed_over_its_ramp() {
    use auberean_assets::fx::{AppearanceKey, FxData, FxNode};
    use auberean_draw::effects::AppearanceFx;
    const FX: u32 = 0x4100_0002;
    const TABLE: u32 = 0x2000_003A;
    let r = &install().records;
    let mut look = Look::default();
    let node = FxNode {
        appearance: Some(DataId(TABLE)),
        appearance_key: Some(AppearanceKey {
            key: 2,
            scalar: 1.0,
        }),
        ramp_time: Some(1.0),
        ..FxNode::default()
    };
    look.global.fx_overrides = vec![(
        FX,
        vec![FxData {
            default: node,
            by_terrain: Vec::new(),
        }],
    )];
    let spot = FxSpot {
        home: (2, 0x98),
        place: Mat4::IDENTITY,
        scale: Vec3::new(1.0, 1.0, 1.0),
        now: 0.0,
    };
    let strengths = |fx: &mut ObjectEffects| -> Vec<Option<f32>> {
        fx.take_appearance()
            .into_iter()
            .map(|a| match a {
                AppearanceFx::Apply { table, key } if table == TABLE && key.key == 2 => {
                    Some(key.scalar)
                }
                AppearanceFx::Remove { table, key } if table == TABLE && key == 2 => None,
                other => panic!("{other:?}"),
            })
            .collect()
    };
    let systems = ParticleSystems::default();
    let mut fx = ObjectEffects::of_look(&look).expect("its effects");
    fx.start(&systems, r, spot.home, &spot.place, spot.scale, 0.0);
    fx.use_time(r, 0.0);
    let _ = fx.do_fx(&systems, r, FX, 0.8, &spot, FxWho::default(), &NoRig);
    assert_eq!(strengths(&mut fx), vec![None], "on at nothing");
    fx.use_time(r, 0.25);
    fx.use_time(r, 0.5);
    assert_eq!(strengths(&mut fx), vec![Some(0.25), Some(0.5)]);
    fx.use_time(r, 1.0);
    fx.use_time(r, 2.0);
    assert_eq!(
        strengths(&mut fx),
        vec![Some(0.8)],
        "held at the strength asked"
    );
    let _ = fx.do_fx(&systems, r, FX, 0.5, &spot, FxWho::default(), &NoRig);
    assert_eq!(strengths(&mut fx), vec![Some(0.8)], "asked again, as it is");
    fx.use_time(r, 2.125);
    fx.use_time(r, 3.0);
    assert_eq!(strengths(&mut fx), vec![Some(0.675), Some(0.5)]);
}

/// How many particles drawn lie within `r` of `at` (across, as seen from above).
fn within(batches: &[auberean_draw::scene::ObjectBatch], at: Vec3, r: f32) -> usize {
    let mut n = 0;
    for b in batches {
        for q in b.vertices.chunks(4) {
            #[allow(clippy::cast_precision_loss)]
            let k = q.len() as f32;
            let c = Vec3::new(
                q.iter().map(|v| v.position[0]).sum::<f32>() / k,
                q.iter().map(|v| v.position[1]).sum::<f32>() / k,
                q.iter().map(|v| v.position[2]).sum::<f32>() / k,
            );
            let w = b.transform.point(c);
            let (dx, dy) = (w.x - at.x, w.y - at.y);
            if dx * dx + dy * dy < r * r {
                n += 1;
            }
        }
    }
    n
}

/// The town's torches light the land: the objects placed with the world about block `6FA9` place
/// orange point lights of range 8 with their startup effects, at their effects' offsets, and those
/// standing less than their range above the land lay a pool of their light there.
///
/// Behaviour: draw.effects.placed-objects-torches-light-the-land
#[test]
fn placed_objects_torches_light_the_land() {
    use auberean_draw::decal::{ground_height, landscape_decal, point_light_pool};
    use auberean_draw::effects::PlacedEffects;
    use auberean_draw::scene::{SceneBuilder, Spot};
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let spot = Spot {
        block: (0x6F, 0xA9),
        position: Vec3::new(96.0, 96.0, 100.0),
        heading: 0.0,
        pitch: 0.0,
        time_of_day: 0.05,
        day: None,
    };
    let frame = builder.frame(&spot, 60.0);
    let systems = ParticleSystems::default();
    let mut placed = PlacedEffects::default();
    placed.sync(&systems, r, &frame.placed, 0.0);
    let lights = placed.lights(spot.block, 160.0, 1.0);
    assert!(lights.len() >= 5, "{} lights", lights.len());
    let mut pools = 0;
    for l in &lights {
        assert!(
            (l.light.range.base - 8.0).abs() < 1e-4,
            "range {}",
            l.light.range.base
        );
        assert!(
            l.light.light.color.r > 0.9 && l.light.light.color.b < 0.1,
            "orange"
        );
        let Some(square) = builder.ground_square(spot.block, l.origin) else {
            continue;
        };
        let ground = ground_height(&square, l.origin.x, l.origin.y).expect("a height");
        // one high on a roof or a platform, its range above the land, lays none
        let Some(pool) = point_light_pool(1, l.origin, ground, 8.0, 1.0, l.light.light.color, 1.0)
        else {
            continue;
        };
        let at = |p: Vec3| builder.ground_square(spot.block, p);
        if !landscape_decal(l.origin, &pool, builder.square_side(), &at).is_empty() {
            pools += 1;
        }
    }
    println!("{} lights, {pools} pools", lights.len());
    assert!(pools >= 5, "{pools} pools");
}

/// With the light source effects preference off, the effects place no lights at all: the town's
/// torches made then carry none (so they light nothing and lay no pool), and turning the preference
/// on gives lights only to what is made after.
///
/// Behaviour: draw.effects.no-lights-while-the-light-source-effects-preference-is-off
#[test]
fn no_lights_while_the_light_source_effects_preference_is_off() {
    use auberean_draw::effects::PlacedEffects;
    use auberean_draw::scene::{SceneBuilder, Spot};
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let spot = Spot {
        block: (0x6F, 0xA9),
        position: Vec3::new(96.0, 96.0, 100.0),
        heading: 0.0,
        pitch: 0.0,
        time_of_day: 0.05,
        day: None,
    };
    let frame = builder.frame(&spot, 60.0);
    let systems = ParticleSystems::default();
    assert!(systems.light_sources(), "on unless set off");
    systems.set_light_sources(false);
    let mut placed = PlacedEffects::default();
    placed.sync(&systems, r, &frame.placed, 0.0);
    assert!(placed.len() >= 5, "{} objects play effects", placed.len());
    assert!(
        placed.lights(spot.block, 160.0, 1.0).is_empty(),
        "no lights"
    );
    systems.set_light_sources(true);
    assert!(
        placed.lights(spot.block, 160.0, 2.0).is_empty(),
        "none for what was made while off"
    );
    let mut again = PlacedEffects::default();
    again.sync(&systems, r, &frame.placed, 2.0);
    assert!(
        again.lights(spot.block, 160.0, 3.0).len() >= 5,
        "made after"
    );
}

/// With the volumetric fog effects off, the start hall's ground mist (system `0x3900018D`, moved
/// by repulsors) neither moves on nor draws. Turned on again, it moves on and draws.
///
/// Behaviour: draw.particles.a-mist-stands-unseen-without-volumetric-fog
#[test]
fn a_mist_stands_unseen_without_volumetric_fog() {
    let r = &install().records;
    let home = (2, 0x98);
    let at = Mat4::translation(Vec3::new(60.0, 240.0, -44.6));
    let unit = Vec3::new(1.0, 1.0, 1.0);
    let systems = ParticleSystems::default();
    let mist = systems
        .create(r, 0x3900_018D, false, home, at, unit, 1.0, 0.0)
        .expect("the mist");
    let frame = |now: f64| {
        systems.queue(&mist, home, at, unit, 1.0, Color::WHITE, now);
        systems
            .batches(
                home,
                160.0,
                now,
                Vec3::new(60.0, 230.0, -43.0),
                (
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                    Vec3::new(0.0, 0.0, 1.0),
                ),
            )
            .len()
    };
    let mut now = 0.0;
    let mut step = |n: u32| {
        let mut drawn = 0;
        for _ in 0..n {
            now += 1.0 / 30.0;
            drawn = frame(now);
        }
        drawn
    };
    assert!(systems.volumetric_fog(), "on unless set off");
    assert!(step(150) > 0, "the mist draws");
    let alive = systems.alive(&mist);
    assert!(alive > 0);
    systems.set_volumetric_fog(false);
    assert_eq!(step(150), 0, "not drawn");
    assert_eq!(systems.alive(&mist), alive, "not moved on");
    systems.set_volumetric_fog(true);
    assert!(step(90) > 0, "drawn again");
}

/// The avatar hall's two braziers play their effects on the scene's own clock, seen from where the
/// camera stands: without them the hall shows no flames.
///
/// Behaviour: draw.view.a-scenes-objects-play-their-effects
#[test]
fn a_scenes_objects_play_their_effects() {
    let r = &install().records;
    let scene = r
        .get::<auberean_assets::ui::UiScene>(0x2100_000B)
        .expect("the hall's scene");
    let mut effects = auberean_draw::effects::ViewEffects::of_scene(
        r,
        &scene.entities,
        &auberean_primitives::random::Random::default(),
    );
    let axes = (
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    );
    let eye = Vec3::new(2.25, 0.2, 3.1);
    let white = Color::new(1.0, 1.0, 1.0, 1.0);
    let mut drawn = 0;
    for frame in 0..200 {
        let now = f64::from(frame) * 0.05;
        drawn = drawn.max(effects.batches(r, now, white, None, eye, axes).len());
    }
    assert!(drawn > 0, "the hall's objects draw particles");
}

/// An effect keyed to the player's preferences plays the entries keyed as they ask and no others:
/// a wound's blood (`0x410004D3` of the shared table) has an entry for each level of gore and each
/// color, a level with nothing to show plays nothing, and a channel no preference holds says
/// nothing.
///
/// Behaviour: draw.effects.an-effects-entries-play-as-the-players-preferences-ask
#[test]
fn an_effects_entries_play_as_the_players_preferences_ask() {
    use auberean_draw::effects::{stored_fx_data, Channels};
    let r = &install().records;
    let list = stored_fx_data(r, 0x1E00_0057, 0x4100_04D3);
    let playing = |gore: u32, blood: u32| -> Vec<u32> {
        let asked = Channels {
            gore,
            blood,
            foot_step_particles: true,
        };
        list.iter()
            .filter(|d| !asked.mismatch(&d.default))
            .flat_map(|d| d.default.particles.iter().flatten().map(|p| p.0))
            .collect()
    };
    assert_eq!(playing(4, 2), [0x3900_0308]);
    assert_eq!(playing(4, 1), [0x3900_0309]);
    assert_eq!(playing(3, 2), [0x3900_0308]);
    assert_eq!(playing(2, 2), [0u32; 0], "none");
    let fun = [0x3900_030A, 0x3900_030B, 0x3900_030C, 0x3900_030D];
    assert_eq!(playing(1, 2), fun);
    assert_eq!(playing(1, 1), fun);
}

/// An effect that sees its object through while it plays puts it back: the arrival effect's entry
/// (`0x41000010` of the player's table) takes the alpha to a half and, as it stops, the color goes
/// back to white in a tenth of a second. Stopping it when it is not playing puts the color back
/// all the same.
///
/// Behaviour: draw.effects.an-effects-color-goes-back-as-it-ends
#[test]
fn an_effects_color_goes_back_as_it_ends() {
    use auberean_draw::effects::{FxSlider, FxSpot, FxWho, NoRig};
    const ARRIVAL: u32 = 0x4100_0010;
    const ALPHA: u32 = 0x4000_001A;
    let r = &install().records;
    let human = Look::of_record(r, DataId(0x1F00_0023)).expect("the human male's look");
    let systems = ParticleSystems::default();
    let spot = FxSpot {
        home: (0, 0),
        place: Mat4::IDENTITY,
        scale: Vec3::new(1.0, 1.0, 1.0),
        now: 1.0,
    };
    let mut fx = ObjectEffects::of_look(&human).expect("its effects");
    let (took, _) = fx.do_fx(&systems, r, ARRIVAL, 1.0, &spot, FxWho::default(), &NoRig);
    assert!(took);
    let asked = fx.take_sliders();
    assert!(
        asked.iter().any(|s| s.slider == ALPHA && s.value == 0.5),
        "{asked:?}"
    );
    assert!(fx.stop_fx(r, ARRIVAL, false));
    let back = fx.take_sliders();
    let white = |list: &[FxSlider]| {
        [0x4000_0016, 0x4000_0017, 0x4000_0018, ALPHA]
            .iter()
            .all(|k| {
                list.iter()
                    .any(|s| s.slider == *k && s.value == 1.0 && s.time == 0.1)
            })
    };
    assert!(white(&back), "{back:?}");
    assert!(!fx.stop_fx(r, ARRIVAL, false), "no longer playing");
    assert!(white(&fx.take_sliders()), "put back all the same");
}

/// An effect that simply runs out leaves the color as it is, unless an entry's color lasts a time:
/// `0x8100028E`, which takes the alpha to nothing and says to restore, keeps it, since only a stop
/// asked for puts it back; `0x8100028F`, whose color lasts, goes back to white as it runs out.
///
/// Behaviour: draw.effects.an-effect-running-out-restores-only-a-color-that-lasts
#[test]
fn an_effect_running_out_restores_only_a_color_that_lasts() {
    use auberean_draw::effects::{FxSpot, FxWho, NoRig};
    let r = &install().records;
    let human = Look::of_record(r, DataId(0x1F00_0023)).expect("the human male's look");
    let systems = ParticleSystems::default();
    let spot = FxSpot {
        home: (0, 0),
        place: Mat4::IDENTITY,
        scale: Vec3::new(1.0, 1.0, 1.0),
        now: 1.0,
    };
    let run_out = |effect: u32| {
        let mut fx = ObjectEffects::of_look(&human).expect("its effects");
        let (took, _) = fx.do_fx(&systems, r, effect, 1.0, &spot, FxWho::default(), &NoRig);
        assert!(took, "{effect:08X}");
        fx.take_sliders();
        fx.use_time(r, 1.0);
        fx.use_time(r, 1_000.0);
        let after = fx.take_sliders();
        assert!(!fx.states().contains_key(&effect), "{effect:08X} ran out");
        after.iter().any(|s| s.value == 1.0 && s.time == 0.1)
    };
    assert!(
        !run_out(0x8100_028E),
        "says to restore, but only a stop does"
    );
    assert!(run_out(0x8100_028F), "its color lasts a time");
}

/// How long an effect lasts asks whether what it plays runs until stopped: an entry whose music
/// loops (`0x810002B3` of table `0x1E00007A`) lasts until it is stopped, and so does one whose
/// particle system does, even fired and forgotten; a system with a life of its own doesn't hold it.
///
/// Behaviour: draw.effects.an-effect-lasts-while-its-sound-or-system-runs-until-stopped
#[test]
fn an_effect_lasts_until_stopped_when_its_sound_or_system_does() {
    use auberean_assets::fx::FxNode;
    use auberean_draw::effects::{calc_duration, particle_is_looping, stored_fx_data};
    let r = &install().records;
    let dice = auberean_world::chargen::Random::default();
    let list = stored_fx_data(r, 0x1E00_007A, 0x8100_02B3);
    let music = &list.first().expect("the entry").default;
    assert_eq!(calc_duration(r, music, &dice, None, false), None);
    let system = |looping: bool| {
        ids_of(0x39)
            .into_iter()
            .find(|id| particle_is_looping(r, *id) == looping)
            .map(DataId)
            .expect("a system")
    };
    let of = |p: DataId, fire_and_forget: bool| FxNode {
        particles: Some(vec![p]),
        fire_and_forget: Some(fire_and_forget),
        ..FxNode::default()
    };
    let endless = system(true);
    let finite = system(false);
    assert_eq!(
        calc_duration(r, &of(endless, false), &dice, None, false),
        None
    );
    assert_eq!(
        calc_duration(r, &of(endless, true), &dice, None, false),
        None
    );
    assert_eq!(
        calc_duration(r, &of(finite, false), &dice, None, false),
        Some(0.0)
    );
}

/// Stopping an effect that turned a streak on turns it off, whether or not the effect still plays:
/// `0x810002AE` of the human male's table.
///
/// Behaviour: draw.streak.stopping-the-effect-that-turned-a-streak-on-turns-it-off
#[test]
fn stopping_the_effect_that_turned_a_streak_on_turns_it_off() {
    use auberean_draw::effects::{FxSpot, FxWho, NoRig};
    const SWING: u32 = 0x8100_02AE;
    let r = &install().records;
    let human = Look::of_record(r, DataId(0x1F00_0023)).expect("the human male's look");
    let systems = ParticleSystems::default();
    let spot = FxSpot {
        home: (0, 0),
        place: Mat4::IDENTITY,
        scale: Vec3::new(1.0, 1.0, 1.0),
        now: 1.0,
    };
    let mut fx = ObjectEffects::of_look(&human).expect("its effects");
    let (took, _) = fx.do_fx(&systems, r, SWING, 1.0, &spot, FxWho::default(), &NoRig);
    assert!(took);
    let on = fx.take_streak_asks();
    let (_, location) = *on
        .iter()
        .find(|(start, _)| *start)
        .expect("it turns one on");
    fx.stop_fx(r, SWING, false);
    assert_eq!(
        fx.take_streak_asks(),
        [(false, location)],
        "playing or just ended"
    );
    fx.stop_fx(r, SWING, false);
    assert_eq!(
        fx.take_streak_asks(),
        [(false, location)],
        "no longer playing"
    );
}

/// An effect played on an object carries the repulsor its entry names: `0x810001C7` of the human
/// male's table.
///
/// Behaviour: draw.particles.an-effect-carries-the-repulsor-its-entry-names
#[test]
fn an_effect_played_carries_the_repulsor_its_entry_names() {
    use auberean_draw::effects::{FxSpot, FxWho, NoRig};
    let r = &install().records;
    let human = Look::of_record(r, DataId(0x1F00_0023)).expect("the human male's look");
    let systems = ParticleSystems::default();
    let spot = FxSpot {
        home: (0, 0),
        place: Mat4::IDENTITY,
        scale: Vec3::new(1.0, 1.0, 1.0),
        now: 1.0,
    };
    let mut fx = ObjectEffects::of_look(&human).expect("its effects");
    let before = fx.repulsor_count();
    let (took, _) = fx.do_fx(
        &systems,
        r,
        0x8100_01C7,
        1.0,
        &spot,
        FxWho::default(),
        &NoRig,
    );
    assert!(took);
    assert_eq!(fx.repulsor_count(), before + 1);
}

/// An effect with a decal (`0x810002C7` of the shared table) lays it: its material, its life, its
/// size and its color from the entry; with the decal effects preference off, none.
///
/// Behaviour: draw.effects.an-effect-lays-the-decal-its-entry-names
#[test]
fn an_effect_lays_the_decal_its_entry_names() {
    use auberean_draw::effects::{stored_fx_data, FxSpot, FxWho, NoRig};
    const LAYS: u32 = 0x8100_02C7;
    let r = &install().records;
    let unit = Vec3::new(1.0, 1.0, 1.0);
    let human = Look::of_record(r, DataId(0x1F00_0023)).expect("the human male's look");
    let systems = ParticleSystems::default();
    let spot = FxSpot {
        home: (0, 0),
        place: Mat4::IDENTITY,
        scale: unit,
        now: 1.0,
    };
    let mut fx = ObjectEffects::of_look(&human).expect("its effects");
    let (took, _) = fx.do_fx(&systems, r, LAYS, 1.0, &spot, FxWho::default(), &NoRig);
    assert!(took);
    let laid = fx.take_decals();
    let named: Vec<u32> = stored_fx_data(r, 0x1E00_0057, LAYS)
        .iter()
        .filter(|d| !systems.fx_channels().mismatch(&d.default))
        .filter_map(|d| d.default.decal.as_ref())
        .filter(|(on, _)| *on)
        .map(|(_, d)| d.material.0)
        .collect();
    assert!(!named.is_empty());
    assert_eq!(
        laid.iter().map(|d| d.desc.material).collect::<Vec<_>>(),
        named
    );
    for d in &laid {
        assert_eq!(d.desc.lifespan, 30.0);
        assert!(
            d.desc.size.x > 0.0 && d.desc.size.x == d.desc.size.y,
            "{:?}",
            d.desc.size
        );
        assert!((auberean_world::math::length(d.desc.direction) - 1.0).abs() < 1e-3);
    }
    assert!(fx.take_decals().is_empty(), "taken");
    let off = ParticleSystems::default();
    off.set_decal_effects(false);
    let mut fx = ObjectEffects::of_look(&human).expect("its effects");
    let _ = fx.do_fx(&off, r, LAYS, 1.0, &spot, FxWho::default(), &NoRig);
    assert!(fx.take_decals().is_empty(), "the preference off");
}

/// A figure of one bone (`0x8100_0005`) at a height that can change.
struct OneBone(std::cell::Cell<f32>);

impl auberean_draw::effects::Rig for OneBone {
    fn holding_location(&self, _kind: u32) -> Option<Mat4> {
        Some(Mat4::translation(Vec3::new(0.0, 0.0, 2.0)))
    }
    fn bone(&self, kind: u32) -> Option<Mat4> {
        (kind == 0x8100_0005).then(|| Mat4::translation(Vec3::new(0.0, 0.0, self.0.get())))
    }
    fn water_height(&self) -> Option<f32> {
        None
    }
    fn holding_bone(&self, _kind: u32) -> Option<Mat4> {
        Some(Mat4::translation(Vec3::new(0.0, 0.0, self.0.get())))
    }
    fn has_figure(&self) -> bool {
        true
    }
}

/// Every particle's height drawn at `now`, lowest and highest.
fn heights(systems: &ParticleSystems, now: f64) -> Option<(f32, f32)> {
    let axes = (
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    );
    let zs: Vec<f32> = systems
        .batches((0, 0), 1000.0, now, Vec3::new(0.0, -5.0, 0.0), axes)
        .iter()
        .flat_map(|b| b.vertices.iter().map(|v| v.position[2]))
        .collect();
    let lo = zs.iter().copied().reduce(f32::min)?;
    let hi = zs.iter().copied().reduce(f32::max)?;
    Some((lo, hi))
}

/// The glow as a body arrives or leaves (`0x4100_0010`) names a system for each of the figure's
/// bones: a figure with only one of them gets the entries of that one and no others, and they
/// stand at that bone and go with it as it moves, not at the object's feet.
///
/// Behaviour: draw.effects.a-carried-system-stands-on-the-bone-its-entry-names
#[test]
fn a_carried_system_stands_on_the_bone_its_entry_names() {
    use auberean_draw::effects::{stored_fx_data, CarriedOn, FxSpot, FxWho};
    const GLOW: u32 = 0x4100_0010;
    const BONE: u32 = 0x8100_0005;
    let r = &install().records;
    let unit = Vec3::new(1.0, 1.0, 1.0);
    let human = Look::of_record(r, DataId(0x1F00_0023)).expect("the human male's look");
    let systems = ParticleSystems::default();
    let rig = OneBone(std::cell::Cell::new(50.0));
    let mut fx = ObjectEffects::of_look(&human).expect("its effects");
    let spot = FxSpot {
        home: (0, 0),
        place: Mat4::IDENTITY,
        scale: unit,
        now: 0.0,
    };
    let (took, _) = fx.do_fx(&systems, r, GLOW, 1.0, &spot, FxWho::default(), &rig);
    assert!(took);
    let named = stored_fx_data(r, 0x1E00_0057, GLOW)
        .iter()
        .filter(|d| d.default.bone == Some(BONE))
        .count();
    assert!(named > 0);
    assert_eq!(fx.systems().len(), named, "only the bone the figure has");
    assert!(fx.systems().iter().all(|c| c.on == CarriedOn::Bone(BONE)));
    let mut t = 0.0;
    while t < 0.5 {
        t += 0.05;
        fx.think_on(
            &systems,
            (0, 0),
            &Mat4::IDENTITY,
            unit,
            Color::WHITE,
            t,
            &rig,
        );
    }
    let (lo, hi) = heights(&systems, t).expect("it glows");
    assert!(lo > 40.0 && hi < 60.0, "at the bone: {lo} to {hi}");
    rig.0.set(-50.0);
    while t < 1.0 {
        t += 0.05;
        fx.think_on(
            &systems,
            (0, 0),
            &Mat4::IDENTITY,
            unit,
            Color::WHITE,
            t,
            &rig,
        );
    }
    let (lo, _) = heights(&systems, t).expect("it glows");
    assert!(lo < -40.0, "it went with the bone: {lo}");
}

/// A system an entry keeps going at a holding location stands at that location, on the bone the
/// location is on, not at the object's feet.
///
/// Behaviour: draw.effects.a-carried-system-stands-at-the-holding-location-its-entry-names
#[test]
fn a_carried_system_stands_at_the_holding_location_its_entry_names() {
    use auberean_assets::fx::FxTable;
    use auberean_draw::effects::{CarriedOn, FxSpot, FxWho};
    let r = &install().records;
    let unit = Vec3::new(1.0, 1.0, 1.0);
    let table = r.get::<FxTable>(0x1E00_0057).expect("the shared table");
    let (effect, kind) = table
        .effects
        .iter()
        .find_map(|(fx, list)| {
            let [only] = list.as_slice() else { return None };
            let n = &only.default;
            let keeps = n.particles.as_ref().is_some_and(|p| !p.is_empty())
                && n.fire_and_forget != Some(true)
                && n.bone.unwrap_or(0) == 0;
            let kind = n.holding_location.filter(|k| *k != 0)?;
            keeps.then_some((*fx, kind))
        })
        .expect("an effect that keeps a system at a holding location");
    let human = Look::of_record(r, DataId(0x1F00_0023)).expect("the human male's look");
    let systems = ParticleSystems::default();
    let rig = OneBone(std::cell::Cell::new(50.0));
    let mut fx = ObjectEffects::of_look(&human).expect("its effects");
    let spot = FxSpot {
        home: (0, 0),
        place: Mat4::IDENTITY,
        scale: unit,
        now: 0.0,
    };
    let (took, _) = fx.do_fx(&systems, r, effect, 1.0, &spot, FxWho::default(), &rig);
    assert!(took, "{effect:#x}");
    assert_eq!(fx.systems().len(), 1);
    let carried = &fx.systems()[0];
    assert_eq!(carried.on, CarriedOn::HoldingLocation(kind));
    assert_eq!(
        carried.offset.point(Vec3::ZERO),
        Vec3::new(0.0, 0.0, 2.0),
        "the location's own place"
    );
    let mut t = 0.0;
    while t < 0.5 {
        t += 0.05;
        fx.think_on(
            &systems,
            (0, 0),
            &Mat4::IDENTITY,
            unit,
            Color::WHITE,
            t,
            &rig,
        );
    }
    let (lo, hi) = heights(&systems, t).expect("it shows");
    assert!(lo > 40.0 && hi < 65.0, "on the bone: {lo} to {hi}");
}

/// An object placed with the world carries a system its effect sets on a bone there, the bone as
/// the look's body first stands; a bone its look lacks gives no system. Fixture: look `0x1F0000AE`,
/// whose effect stands fires on bones over 6 m along it, and look `0x1F00002B`, whose bones the
/// effect names are not there.
///
/// Behaviour: draw.effects.a-placed-objects-systems-stand-on-its-bones-at-rest
#[test]
fn a_placed_objects_systems_stand_on_its_bones_at_rest() {
    use auberean_draw::effects::{PlacedEffects, PlacedLook};
    let r = &install().records;
    let drawn_along_y = |id: u32| {
        let look = Look::of_record(r, DataId(id)).expect("its look");
        let systems = ParticleSystems::default();
        let mut effects = PlacedEffects::default();
        let placed = PlacedLook {
            key: 1,
            block: (2, 0x98),
            look: std::sync::Arc::new(look),
            place: Mat4::translation(Vec3::new(100.0, 100.0, 0.0)),
            scale: Vec3::new(1.0, 1.0, 1.0),
        };
        effects.sync(&systems, r, std::slice::from_ref(&placed), 0.0);
        let eye = Vec3::new(100.0, 90.0, 2.0);
        let axes = (
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        );
        let mut far: f32 = 0.0;
        for k in 1..=60 {
            let now = f64::from(k) * 0.0525;
            effects.draw(&systems, now, Color::WHITE);
            for b in systems.batches((2, 0x98), 192.0, now, eye, axes) {
                for v in b.vertices.iter() {
                    far = far.max((v.position[1] - 100.0).abs());
                }
            }
        }
        far
    };
    assert!(drawn_along_y(0x1F00_00AE) > 5.0, "out on the bones");
    assert_eq!(drawn_along_y(0x1F00_002B), 0.0, "no bone, no system");
}

/// Puffs of cloud go by in portal space, drawn only while the camera is within the system's fade
/// distance of the tube's start: entering at once they show in the first moment and not once the
/// camera has rushed on. Coming in over a phase, none go until the tunnel is nearly in view.
///
/// Behaviour: draw.portal.puffs-of-cloud-show-near-the-tunnels-start
#[test]
fn puffs_of_cloud_show_near_the_tunnels_start() {
    use auberean_draw::portal::Passage;
    use auberean_draw::scene::{Pass, SceneBuilder};
    use auberean_primitives::random::Random;
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let mut drawn = |at_once: bool| {
        let mut portal = Passage::new(Random::default());
        portal.want(true, at_once, 0.0);
        let mut out = Vec::new();
        for step in 0..300_u16 {
            let now = f64::from(step) / 60.0;
            let look = portal.think(now);
            let Some(frame) = portal.frame(r, &mut builder, &look, now) else {
                continue;
            };
            let puffs = frame
                .objects
                .iter()
                .filter(|b| b.step == Some(Pass::Particles))
                .count();
            let eye = portal.rings().first().map_or(0.0, |(_, _, at)| {
                (at.x * at.x + at.y * at.y + at.z * at.z).sqrt()
            });
            out.push((now, puffs, eye));
        }
        out
    };
    let at_once = drawn(true);
    assert!(
        at_once.iter().any(|(_, puffs, _)| *puffs > 0),
        "they show as it starts"
    );
    for (now, puffs, eye) in &at_once {
        assert!(*eye < 70.0 || *puffs == 0, "{puffs} at {now}, {eye} m out");
    }
    let coming_in = drawn(false);
    for (now, puffs, _) in coming_in.iter().filter(|(now, _, _)| *now < 0.9) {
        assert_eq!(*puffs, 0, "none at {now} while it comes into view");
    }
}
