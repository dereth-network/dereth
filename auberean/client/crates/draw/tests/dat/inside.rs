//! The objects standing in the human training hall (hall copy 0x0298 of the September 2013 cell
//! file), lit by the lights each keeps where it stands, and its lit ground mist.

use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::inside::{particle_light, Outside};
use auberean_world::math::Mat4;

use auberean_draw::particles::{camera_axes, ParticleSet};
use auberean_draw::scene::{ObjectBatch, SceneBuilder, Spot};

use crate::common::install;

/// Inside, every object standing in the cells seen from the start is lit by the lights it keeps
/// where it stands in place of the sun: at most four point lights, as many as its layer's program
/// takes; an object near a lamp keeps that lamp among them, and the cells carry their light in their
/// vertices instead.
///
/// Behaviour: draw.inside.an-object-inside-is-lit-by-the-lights-it-keeps
#[test]
fn an_object_inside_is_lit_by_the_lights_it_keeps() {
    let i = install();
    let region = i.records.region().expect("the region");
    let mut builder = SceneBuilder::new(&i.records, &region);
    let spot = Spot {
        block: (0x02, 0x98),
        position: Vec3::new(101.0, 247.5, -44.9),
        heading: 180.0,
        pitch: 0.0,
        time_of_day: 0.5,
        day: None,
    };
    let outside = Outside {
        sun: Color::new(0.5, 0.5, 0.5, 1.0),
        brightness: 1.0,
        direction: Vec3::new(0.0, 0.0, -1.0),
        ambient: Color::new(0.0, 0.0, 0.0, 1.0),
    };
    let batches = builder.interior(&spot, (0x02, 0x98), 0x0120, &outside);
    let objects: Vec<_> = batches.iter().filter(|b| b.points.inside).collect();
    assert!(objects.len() > 20, "{} object batches", objects.len());
    assert!(objects.iter().all(|b| b.points.count <= 4));
    let lit = objects.iter().filter(|b| b.points.count > 0).count();
    assert!(lit > 20, "{lit} lit");
    // Every light an object takes lies within reach of it: its color as it shines is not black.
    for b in &objects {
        for l in &b.points.lights[..b.points.count] {
            assert!(l.diffuse.r + l.diffuse.g + l.diffuse.b > 0.0, "{l:?}");
            assert!(l.range > 0.0);
        }
    }
    // Some object stands within 3 m of a lamp it keeps first.
    let near = objects.iter().any(|b| {
        let at = b.transform.point(Vec3::ZERO);
        b.points.count > 0 && {
            let o = b.points.lights[0].origin;
            ((o.x - at.x).powi(2) + (o.y - at.y).powi(2) + (o.z - at.z).powi(2)).sqrt() < 3.0
        }
    });
    assert!(near, "an object beside the lamp it keeps first");
}

/// The colors of the drawn quads' corners and their centers.
fn quads(batches: &[ObjectBatch]) -> Vec<(Vec3, u32)> {
    let mut out = Vec::new();
    for b in batches {
        for q in b.vertices.chunks(4) {
            let c = [0, 1, 2].map(|i| q.iter().map(|v| v.position[i]).sum::<f32>() / 4.0);
            out.push((b.transform.point(Vec3::new(c[0], c[1], c[2])), q[0].diffuse));
        }
    }
    out
}

/// Inside, a lit particle system is lit by the lights kept where it stands (reaching no farther
/// than its place), each particle by its own distance from them, in place of the sun: the hall's
/// ground mist (system `0x3900018D`, set going by look `0x1F0002FC` in cell `0x010A`) takes at most
/// five lights there, none a sky light, and each of its particles is its color times their sum
/// where it is; with no lights about it, it is black.
///
/// Behaviour: draw.inside.a-lit-particle-inside-is-lit-by-the-lights-kept-where-its-system-stands
#[test]
fn a_lit_particle_inside_is_lit_by_the_lights_kept_where_its_system_stands() {
    let i = install();
    let region = i.records.region().expect("the region");
    let mut builder = SceneBuilder::new(&i.records, &region);
    let outside = Outside {
        sun: Color::new(0.5, 0.5, 0.5, 1.0),
        brightness: 1.0,
        direction: Vec3::new(0.0, 0.0, -1.0),
        ambient: Color::new(0.0, 0.0, 0.0, 1.0),
    };
    let at = Vec3::new(9.834_682, 390.168_06, -43.18);
    let lights = builder.particle_lights((0x02, 0x98), at, &outside, &[]);
    assert!(
        !lights.is_empty() && lights.len() <= 5,
        "{} lights about the mist",
        lights.len()
    );
    let one = Vec3::new(1.0, 1.0, 1.0);
    let dice = auberean_primitives::random::Random::new();
    let mut mist = ParticleSet::load(
        &i.records,
        0x3900_018D,
        Mat4::translation(at),
        one,
        0.0,
        &dice,
    )
    .expect("the mist");
    assert!(mist.lit(), "the mist's material asks for lit particles");
    mist.run(5.0, 0.05);
    let (right, forward, up) = camera_axes(0.0, 0.0);
    let eye = Vec3::new(at.x, at.y - 5.0, at.z + 1.0);
    let draw = |m: &ParticleSet| quads(&m.batches(5.0, eye, right, forward, up));
    mist.set_light(Color::WHITE);
    let white = draw(&mist);
    mist.set_lights(lights.clone());
    let lit = draw(&mist);
    mist.set_lights(Vec::new());
    let dark = draw(&mist);
    assert!(white.len() > 20, "{} particles drawn", white.len());
    assert_eq!(white.len(), lit.len());
    let channel = |w: u32, s: u32| f32::from(u8::try_from((w >> s) & 0xFF).unwrap_or(0)) / 255.0;
    let mut checked = 0;
    for ((c, w), (_, l)) in white.iter().zip(&lit) {
        assert_eq!(w >> 24, l >> 24, "the alpha stays");
        let k = particle_light(
            &lights,
            *c,
            0.0,
            &auberean_primitives::random::Random::new(),
        );
        for (s, kk) in [(16, k.r), (8, k.g), (0, k.b)] {
            let want = (channel(*w, s) * kk).clamp(0.0, 1.0);
            assert!(
                (channel(*l, s) - want).abs() <= 2.0 / 255.0,
                "at {c:?}: {:08X} from {:08X} by {k:?}",
                l,
                w
            );
        }
        checked += 1;
    }
    assert!(checked > 20);
    let mean = |q: &[(Vec3, u32)]| {
        q.iter()
            .map(|(_, d)| f64::from((d >> 16) & 0xFF))
            .sum::<f64>()
            / q.len().max(1) as f64
    };
    println!(
        "{} lights about the mist ({:?}); {checked} particles; mean red {:.1} lit by them, {:.1} by white",
        lights.len(),
        lights.iter().map(|l| (l.range, l.color)).collect::<Vec<_>>(),
        mean(&lit),
        mean(&white)
    );
    assert!(
        dark.iter().all(|(_, d)| d & 0x00FF_FFFF == 0),
        "black with no lights about it"
    );
    // The lights about it differ across the mist: its particles are not lit alike.
    let reds: Vec<u32> = lit.iter().map(|(_, l)| (l >> 16) & 0xFF).collect();
    assert!(reds.iter().min() != reds.iter().max(), "{reds:?}");
}

/// A light whose intensity moves (a torch that flickers by noise) lights what stands near it as its
/// intensity stands at the moment: its draws' colors change from one moment to the next, every draw it lights shines with the same intensity in a frame, and its
/// noise rolls the client's generator once a frame however many draws it lights.
///
/// Behaviour: draw.inside.a-flickering-light-lights-its-draws-as-it-stands-at-the-moment
#[test]
fn a_flickering_light_lights_its_draws_as_it_stands_at_the_moment() {
    use auberean_draw::scene::Spot;
    use auberean_primitives::random::Random;
    use auberean_world::inside::BlockLights;
    let i = install();
    let region = i.records.region().expect("the region");
    let mut builder = SceneBuilder::new(&i.records, &region);
    // the first block of the map with a light flickering by noise, and a room it stands in
    let (block, cell, key) = (0..256)
        .flat_map(|x| (0..256).map(move |y| (x, y)))
        .find_map(|b| {
            let l = BlockLights::of(&i.records, b.0, b.1)?;
            let (k, light) = l
                .lights
                .iter()
                .enumerate()
                .find(|(_, l)| l.flicker.is_some_and(|w| w.kind == 3))?;
            let low = u16::try_from(light.cell & 0xFFFF).ok()?;
            Some((b, low, l.block | u32::try_from(k).ok()?))
        })
        .expect("a light flickering by noise");
    let c = i.records.cell(block.0, block.1, cell).expect("its room");
    let spot = Spot {
        block,
        position: c.frame.origin,
        heading: 0.0,
        pitch: 0.0,
        time_of_day: 0.5,
        day: None,
    };
    let (frame, _) = builder.inside_frame(&spot, 60.0, block, cell);
    // the room's own mesh holds it in its vertices as baked, and does not keep it
    assert!(
        !frame.objects.iter().any(|b| {
            matches!(b.cull, auberean_draw::scene::Cull::Cell(_))
                && b.points.lights[..b.points.count]
                    .iter()
                    .any(|l| l.flicker.is_some_and(|f| f.key == key))
        }),
        "a room's own mesh takes the sky lights alone"
    );
    let lit_by = |f: &auberean_draw::scene::Frame| -> Vec<Color> {
        f.objects
            .iter()
            .flat_map(|b| b.points.lights[..b.points.count].iter())
            .filter(|l| l.flicker.is_some_and(|f| f.key == key))
            .map(|l| l.diffuse)
            .collect()
    };
    let (a, b) = (Random::new(), Random::new());
    let mut seen = Vec::new();
    for k in 0..5 {
        let mut f = frame.clone();
        f.move_materials(10.0 + f64::from(k), &a);
        let colors = lit_by(&f);
        assert!(!colors.is_empty(), "draws lit by the flickering light");
        assert!(
            colors.windows(2).all(|w| w[0] == w[1]),
            "one intensity a frame"
        );
        seen.push(colors[0]);
    }
    println!(
        "block {:02X}{:02X} room {cell:04X}: {seen:?}",
        block.0, block.1
    );
    assert!(seen.windows(2).any(|w| w[0] != w[1]), "it flickers");
    // the noise light drew once a frame (with any other moving lights of the frame in step)
    let mut once = frame.clone();
    once.move_materials(10.0, &b);
    let mut twice = frame.clone();
    twice.move_materials(10.0, &b);
    assert_ne!(
        lit_by(&once)[0],
        lit_by(&twice)[0],
        "a fresh roll each frame"
    );
}

/// Inside, a light an object carries lights what stands near it with the lights each keeps: a
/// torch held up in the hall over an object joins that object's point lights (the strongest first,
/// as it keeps them), shining its color times its intensity; nothing beyond its reach takes it, and
/// the draws go back to their own lights once it is gone; a body beside it takes it too, and throws
/// its shadow away from it.
///
/// Behaviour: draw.inside.a-carried-light-lights-what-stands-near-it-and-throws-shadows-away
#[test]
fn a_carried_light_lights_what_stands_near_it_and_throws_shadows_away() {
    use auberean_world::inside::Light;
    let i = install();
    let region = i.records.region().expect("the region");
    let mut builder = SceneBuilder::new(&i.records, &region);
    let spot = Spot {
        block: (0x02, 0x98),
        position: Vec3::new(101.0, 247.5, -44.9),
        heading: 180.0,
        pitch: 0.0,
        time_of_day: 0.5,
        day: None,
    };
    let outside = Outside {
        sun: Color::new(0.5, 0.5, 0.5, 1.0),
        brightness: 1.0,
        direction: Vec3::new(0.0, 0.0, -1.0),
        ambient: Color::new(0.0, 0.0, 0.0, 1.0),
    };
    let mut batches = builder.interior(&spot, (0x02, 0x98), 0x0120, &outside);
    let before: Vec<_> = batches.iter().map(|b| b.points).collect();
    // A torch held 1.5 m over an object standing in one of the rooms.
    let object = batches
        .iter()
        .find_map(|b| b.kept_by.filter(|k| k.key.1 != 0 && k.radius < 2.0))
        .expect("an object keeps its lights");
    let mut torch = Light::of_properties(object.cell, Vec3::ZERO, &Default::default()).0;
    torch.origin = Vec3::new(object.at.x, object.at.y, object.at.z + 1.5);
    torch.color = Color::new(1.0, 0.5, 0.0, 1.0);
    torch.intensity = 1.0;
    torch.range = 8.0;
    torch.most_range = 8.0;
    torch.most_intensity = Some(1.0);
    let orange = Color::new(1.0, 0.5, 0.0, 1.0);
    let lit_by_torch = |b: &ObjectBatch| {
        b.points.lights[..b.points.count]
            .iter()
            .any(|l| l.diffuse == orange)
    };
    builder.light_carried(&spot, &mut batches, std::slice::from_ref(&torch), &outside);
    let object_lit = batches
        .iter()
        .filter(|b| b.kept_by.is_some_and(|k| k.key == object.key))
        .any(lit_by_torch);
    assert!(object_lit, "the object below it takes it");
    let far = batches
        .iter()
        .filter(|b| {
            b.kept_by.is_some_and(|k| {
                let d = auberean_world::math::length(auberean_world::math::sub(k.at, torch.origin));
                d > 8.0 + k.radius + 1.0
            })
        })
        .all(|b| !lit_by_torch(b));
    assert!(far, "nothing out of its reach takes it");
    builder.light_carried(&spot, &mut batches, &[], &outside);
    let after: Vec<_> = batches.iter().map(|b| b.points).collect();
    assert_eq!(after, before, "their own lights again once it is gone");
    // A body 1 m west of the torch, standing on the floor below it: lit by the torch, its shadow
    // falling west, away from it.
    let mut body: Vec<ObjectBatch> = batches
        .iter()
        .find(|b| b.kept_by.is_some_and(|k| k.key.1 != 0 && k.radius < 1.5))
        .cloned()
        .into_iter()
        .collect();
    let feet = Vec3::new(torch.origin.x - 1.0, torch.origin.y, object.at.z);
    for b in &mut body {
        b.transform = Mat4::translation(feet);
        b.kept_by = None;
    }
    let alone = builder.light_inside(
        &spot,
        (0x02, 0x98),
        &mut body,
        feet,
        object.cell,
        &outside,
        &[],
    );
    let way = builder.light_inside(
        &spot,
        (0x02, 0x98),
        &mut body,
        feet,
        object.cell,
        &outside,
        std::slice::from_ref(&torch),
    );
    assert!(body.iter().all(lit_by_torch), "the body takes the torch");
    assert!(
        way.x < -0.2 && way.x < alone.x - 0.2,
        "its shadow turns west, away from the torch: {way:?}, {alone:?} without it"
    );
}

/// A decal inside is laid on the cells' ground about it: a body's disc where the player stands in
/// the start hall takes the hall's triangles about it (none farther than the disc's largest side,
/// two meters), cut to the disc's box and lifted off the floor; where the box reaches through a
/// doorway, the next cell's triangles are taken too, once.
///
/// Behaviour: draw.decal.a-decal-inside-is-laid-on-the-cells-ground-about-it
#[test]
fn a_decal_inside_is_laid_on_the_cells_ground_about_it() {
    use auberean_draw::decal::{disc, dungeon_decal, nearby_geometry, DECAL_LIFT};
    let r = &install().records;
    let block = (0x02, 0x98);
    let at = Vec3::new(101.385, 247.278, -44.215);
    let near = nearby_geometry(r, block, 0x0120, at, 2.0);
    assert!(near.len() >= 2, "{} triangles", near.len());
    // every triangle's box meets the cube about the point
    for t in &near {
        for k in 0..3 {
            let lo = t
                .iter()
                .map(|c| [c.0.x, c.0.y, c.0.z][k])
                .fold(f32::MAX, f32::min);
            let hi = t
                .iter()
                .map(|c| [c.0.x, c.0.y, c.0.z][k])
                .fold(f32::MIN, f32::max);
            let p = [at.x, at.y, at.z][k];
            assert!(lo <= p + 2.0 + 1e-3 && hi >= p - 2.0 - 1e-3);
        }
    }
    let floor = near
        .iter()
        .filter(|t| t.iter().all(|c| c.1.z > 0.9))
        .flat_map(|t| t.iter().map(|c| c.0.z))
        .filter(|z| *z <= at.z + 0.5)
        .fold(f32::MIN, f32::max);
    assert!(floor > at.z - 1.0, "a floor under the point: {floor}");
    let desc = disc(
        0x3100_031D,
        Vec3::new(-0.3, -0.25, 0.0),
        Vec3::new(0.3, 0.25, 1.8),
        Vec3::new(1.0, 1.0, 1.0),
        0.0,
        Color::new(0.45, 0.45, 0.45, 1.0),
    )
    .expect("a disc");
    let laid = dungeon_decal(r, block, 0x0120, at, &desc);
    assert!(!laid.is_empty(), "laid on the floor");
    let half = (0.5 * desc.size.x).max(0.5 * desc.size.y) + 1e-3;
    for v in laid.iter().flatten() {
        assert!((v.position.x - at.x).abs() <= half + DECAL_LIFT);
        assert!((v.position.y - at.y).abs() <= half + DECAL_LIFT);
        assert!((v.position.z - at.z).abs() <= 0.5 * desc.size.z + DECAL_LIFT + 1e-3);
    }
    assert!(laid
        .iter()
        .flatten()
        .any(|v| (v.position.z - floor - DECAL_LIFT).abs() < 0.01));
    // by a doorway, the next cell's ground too
    let hall = r.cell(block.0, block.1, 0x0120).expect("the hall");
    let mesh: auberean_assets::mesh::CellMesh = r.get(hall.mesh.0).expect("its mesh");
    let through = hall
        .portals
        .iter()
        .filter(|p| (0x100..0xFFFF).contains(&p.other_cell))
        .find_map(|p| {
            let d = mesh.portals.get(usize::from(p.portal))?;
            let poly = mesh.mesh.physics_polygons.get(usize::from(d.polygon))?;
            let v = auberean_draw::mesh::vertices(&mesh.mesh.vertices)?;
            let k = usize::from(*poly.vertices.first()?);
            let local = Vec3::new(v[k].position[0], v[k].position[1], v[k].position[2]);
            let frame = Mat4::from_parts(
                hall.frame.origin,
                hall.frame.rotation,
                Vec3::new(1.0, 1.0, 1.0),
            );
            Some((p.other_cell, frame.point(local)))
        });
    let (other, by_door) = through.expect("a doorway to another room");
    // from either side of the doorway the same ground: each room's own and the other's
    #[allow(clippy::cast_possible_truncation)]
    let keys = |t: &[[auberean_draw::decal::Corner; 3]]| {
        let mut k: Vec<[i64; 9]> = t
            .iter()
            .map(|t| {
                let p = t.map(|c| [c.0.x, c.0.y, c.0.z]);
                let f = p.as_flattened();
                std::array::from_fn(|i| (f[i] * 1000.0).round() as i64)
            })
            .collect();
        k.sort_unstable();
        k
    };
    // a room's own triangles near the point, worked out here from its mesh's first level
    let own = |low: u16| {
        let c = r.cell(block.0, block.1, low).expect("the room");
        let m: auberean_assets::mesh::CellMesh = r.get(c.mesh.0).expect("its mesh");
        let v = auberean_draw::mesh::vertices(&m.mesh.vertices).expect("its vertices");
        let frame = Mat4::from_parts(c.frame.origin, c.frame.rotation, Vec3::new(1.0, 1.0, 1.0));
        let here = frame.rigid_preimage(by_door);
        let mut out = Vec::new();
        for f in &m.mesh.levels[0].fragments {
            for t in f.indices.as_chunks::<3>().0 {
                let p = [t[0], t[1], t[2]].map(|k| {
                    let q = v[usize::from(k)].position;
                    Vec3::new(q[0], q[1], q[2])
                });
                let meets = (0..3).all(|a| {
                    let get = |p: Vec3| [p.x, p.y, p.z][a];
                    let lo = p.iter().map(|&q| get(q)).fold(f32::MAX, f32::min);
                    let hi = p.iter().map(|&q| get(q)).fold(f32::MIN, f32::max);
                    lo <= get(here) + 1.0 && hi >= get(here) - 1.0
                });
                if meets {
                    out.push(p.map(|q| (frame.point(q), Vec3::ZERO)));
                }
            }
        }
        keys(&out)
    };
    let from_hall = keys(&nearby_geometry(r, block, 0x0120, by_door, 1.0));
    let (hall_own, other_own) = (own(0x0120), own(other));
    assert!(!hall_own.is_empty() && !other_own.is_empty());
    assert!(hall_own
        .iter()
        .chain(&other_own)
        .all(|t| from_hall.contains(t)));
    assert!(from_hall.len() >= hall_own.len() + other_own.len());
}
