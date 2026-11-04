//! The dynamic objects' volumes of shadow on the land.

use auberean_animation::body::Body as Figure;
use auberean_animation::CachedRecords;
use auberean_draw::body::BodyDrawer;
use auberean_draw::scene::{SceneBuilder, Spot};
use auberean_draw::volume::{VolumeMaker, VolumeRoom, Volumes};
use auberean_primitives::{DataId, Quat, Vec3};
use auberean_world::math::Mat4;

use crate::common::{checked, install, renderer};

/// A body's volume of shadow darkens the ground where it falls and never the body: the human
/// male standing on the land by the crystals in the morning sun, drawn with its volume and
/// without it, is darker in a few hundred places with it, brighter in none, and none of the
/// places where the body shows (found by drawing the land without it) is darkened as the ground
/// is.
///
/// Behaviour: draw.volume.a-bodys-shadow-darkens-the-ground-and-not-the-body
#[test]
fn a_bodys_shadow_darkens_the_ground_and_not_the_body() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let at = Vec3::new(127.0, 148.0, 79.5);
    let spot = Spot {
        block: (0xCB, 0x94),
        position: Vec3::new(at.x + 1.0, at.y - 3.5, at.z + 2.5),
        heading: -10.0,
        pitch: -30.0,
        time_of_day: 0.4,
        day: None,
    };
    let bare = builder.frame(&spot, 60.0);
    let recs = CachedRecords::new(r);
    let figure = Figure::assemble(&recs, DataId(0x1F00_0023)).expect("the human male");
    let poses = figure.pose(&recs);
    let place = Mat4::from_parts(at, Quat::IDENTITY, Vec3::new(1.0, 1.0, 1.0));
    let mut batches = BodyDrawer::default().batches(r, &figure, &poses, &place);
    for b in &mut batches {
        b.casts_volume = true;
    }
    let mut plain = bare.clone();
    plain.objects.extend(batches.iter().cloned());
    let mut shadowed = plain.clone();
    shadowed.volumes.strength = Volumes::strength_of(shadowed.light.shadow_color, false);
    let mut room = VolumeRoom::FRAME;
    VolumeMaker::default().cast(
        r,
        &batches,
        shadowed.light.light_direction,
        1.0,
        2,
        &mut room,
        &mut shadowed.volumes.volumes,
    );
    assert!(
        !shadowed.volumes.volumes.is_empty(),
        "the body casts volumes"
    );
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let [without_body, without, with] = checked(&mut device, |d| {
        [&bare, &plain, &shadowed].map(|f| d.render(f, 480, 360, &mut load).expect("read back"))
    });
    let sum = |p: &[u8]| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]);
    let (mut darker, mut brighter, mut on_body) = (0, 0, 0);
    for ((a, b), c) in with
        .rgba
        .chunks(4)
        .zip(without.rgba.chunks(4))
        .zip(without_body.rgba.chunks(4))
    {
        let (a, b, c) = (sum(a), sum(b), sum(c));
        let body = b.abs_diff(c) > 6;
        if b > a + 6 {
            darker += 1;
            // darkened as the shadowed ground is (by more than three tenths), not an edge pixel
            // a sample of whose ground lies in the shadow
            if body && a * 10 < b * 7 {
                on_body += 1;
            }
        }
        if a > b + 6 {
            brighter += 1;
        }
    }
    let n = shadowed.volumes.volumes.len();
    let strength = shadowed.volumes.strength.r;
    println!("{n} volumes, strength {strength}; {darker} pixels darker ({on_body} on the body)");
    println!("{brighter} pixels brighter");
    assert!(darker > 200, "{darker} pixels darker");
    assert_eq!(brighter, 0);
    assert_eq!(on_body, 0, "the body is drawn after its shadow");
}

/// At the self shadowing setting a body is drawn before the darkening and makes its volumes as it
/// is drawn: the human male standing by the crystals in the morning, drawn with its volumes and
/// without, is darker on the ground and on its own side away from the sun, deeply, and brighter
/// nowhere.
///
/// Behaviour: draw.volume.at-self-shadowing-a-body-shadows-itself
#[test]
fn at_self_shadowing_a_body_shadows_itself() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let at = Vec3::new(127.0, 148.0, 79.5);
    let spot = Spot {
        block: (0xCB, 0x94),
        position: Vec3::new(at.x + 1.0, at.y - 3.5, at.z + 2.5),
        heading: -10.0,
        pitch: -30.0,
        time_of_day: 0.4,
        day: None,
    };
    let mut bare = builder.frame(&spot, 60.0);
    // the camera on the body's shadowed side, looking back at it against the light
    let l = bare.light.light_direction;
    let flat = (l.x * l.x + l.y * l.y).sqrt();
    let (dx, dy) = (l.x / flat, l.y / flat);
    bare.camera.position = Vec3::new(at.x + 3.0 * dx, at.y + 3.0 * dy, at.z + 1.5);
    bare.camera.heading = dereth_primitives::num::math::atan2f(-dx, -dy).to_degrees();
    bare.camera.pitch = -15.0;
    let recs = CachedRecords::new(r);
    let figure = Figure::assemble(&recs, DataId(0x1F00_0023)).expect("the human male");
    let poses = figure.pose(&recs);
    let place = Mat4::from_parts(at, Quat::IDENTITY, Vec3::new(1.0, 1.0, 1.0));
    // drawn with the other opaque objects, before the darkening
    let batches = BodyDrawer::default().batches(r, &figure, &poses, &place);
    let mut plain = bare.clone();
    plain.objects.extend(batches.iter().cloned());
    let mut shadowed = plain.clone();
    shadowed.volumes.strength = Volumes::strength_of(shadowed.light.shadow_color, false);
    let mut room = VolumeRoom::FRAME;
    VolumeMaker::default().cast_as_drawn(
        r,
        &batches,
        shadowed.light.light_direction,
        1.0,
        2,
        &mut room,
        &mut shadowed.volumes.volumes,
    );
    assert!(
        !shadowed.volumes.volumes.is_empty(),
        "the body casts volumes"
    );
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let [without_body, without, with] = checked(&mut device, |d| {
        [&bare, &plain, &shadowed].map(|f| d.render(f, 480, 360, &mut load).expect("read back"))
    });
    let sum = |p: &[u8]| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]);
    let (mut darker, mut brighter, mut on_body) = (0, 0, 0);
    for ((a, b), c) in with
        .rgba
        .chunks(4)
        .zip(without.rgba.chunks(4))
        .zip(without_body.rgba.chunks(4))
    {
        let (a, b, c) = (sum(a), sum(b), sum(c));
        let body = b.abs_diff(c) > 6;
        if b > a + 6 {
            darker += 1;
            if body && a * 10 < b * 7 {
                on_body += 1;
            }
        }
        if a > b + 6 {
            brighter += 1;
        }
    }
    let n = shadowed.volumes.volumes.len();
    println!("{n} volumes; {darker} pixels darker ({on_body} on the body), {brighter} brighter");
    assert!(darker > 200, "{darker} pixels darker");
    assert!(
        on_body > 100,
        "{on_body} pixels of the body in its own shadow"
    );
    assert_eq!(brighter, 0);
}

/// At the simple setting a body lays a dark disc on the land under it: the human male standing by
/// the crystals in the morning, drawn with the disc and without it, is darker in a few hundred
/// places with it and brighter in none.
///
/// Behaviour: draw.decal.a-simple-shadow-is-a-dark-disc-under-the-body
#[test]
fn a_simple_shadow_is_a_dark_disc_under_the_body() {
    use auberean_draw::decal::{decal_batches, disc, disc_material, landscape_decal, DecalManager};
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let at = Vec3::new(127.0, 148.0, 79.5);
    let spot = Spot {
        block: (0xCB, 0x94),
        position: Vec3::new(at.x + 1.0, at.y - 3.5, at.z + 2.5),
        heading: -10.0,
        pitch: -30.0,
        time_of_day: 0.4,
        day: None,
    };
    let mut plain = builder.frame(&spot, 60.0);
    let recs = CachedRecords::new(r);
    let figure = Figure::assemble(&recs, DataId(0x1F00_0023)).expect("the human male");
    let poses = figure.pose(&recs);
    let place = Mat4::from_parts(at, Quat::IDENTITY, Vec3::new(1.0, 1.0, 1.0));
    plain
        .objects
        .extend(BodyDrawer::default().batches(r, &figure, &poses, &place));
    let b = figure.parts[0].setup.physics_bounds;
    let strength = Volumes::strength_of(plain.light.shadow_color, false);
    let desc = disc(
        disc_material(r),
        Vec3::new(b.min.x, b.min.y, b.min.z),
        Vec3::new(b.max.x, b.max.y, b.max.z),
        Vec3::new(1.0, 1.0, 1.0),
        0.0,
        strength,
    )
    .expect("a disc");
    let ground = |p: Vec3| builder.ground_square(spot.block, p);
    let triangles = landscape_decal(at, &desc, builder.square_side(), &ground);
    assert!(!triangles.is_empty(), "laid on the land");
    let mut decals = DecalManager::default();
    assert!(decals.create(0xCB94_0038, &desc, 0.0, triangles, true));
    let mut with = plain.clone();
    with.decals = decal_batches(r, &mut decals, &mut std::collections::HashMap::new());
    assert!(!with.decals.is_empty(), "drawn with its material");
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let [without, with] = checked(&mut device, |d| {
        [&plain, &with].map(|f| d.render(f, 480, 360, &mut load).expect("read back"))
    });
    let sum = |p: &[u8]| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]);
    let (mut darker, mut brighter) = (0, 0);
    for (a, b) in with.rgba.chunks(4).zip(without.rgba.chunks(4)) {
        let (a, b) = (sum(a), sum(b));
        if b > a + 6 {
            darker += 1;
        }
        if a > b + 6 {
            brighter += 1;
        }
    }
    println!("{darker} pixels darker, {brighter} brighter");
    assert!(darker > 200, "{darker} pixels darker");
    assert_eq!(brighter, 0);
}

/// Inside, the simple setting's disc is laid on the room's floor under the body and drawn over
/// it: the human male standing in the start hall, drawn with its disc and without, is darker in a
/// few hundred places with it and brighter in none.
///
/// Behaviour: draw.decal.a-body-inside-lays-its-disc-on-the-floor
#[test]
fn a_body_inside_lays_its_disc_on_the_floor() {
    use auberean_draw::decal::{decal_batches, disc, disc_material, dungeon_decal, DecalManager};
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let at = Vec3::new(101.385, 247.278, -44.215);
    let spot = Spot {
        block: (0x02, 0x98),
        position: Vec3::new(at.x + 0.5, at.y - 3.0, at.z + 2.5),
        heading: -10.0,
        pitch: -30.0,
        time_of_day: 0.5,
        day: None,
    };
    let (mut plain, _) = builder.inside_frame(&spot, 60.0, (0x02, 0x98), 0x0120);
    let recs = CachedRecords::new(r);
    let figure = Figure::assemble(&recs, DataId(0x1F00_0023)).expect("the human male");
    let poses = figure.pose(&recs);
    let place = Mat4::from_parts(at, Quat::IDENTITY, Vec3::new(1.0, 1.0, 1.0));
    plain
        .objects
        .extend(BodyDrawer::default().batches(r, &figure, &poses, &place));
    let b = figure.parts[0].setup.physics_bounds;
    let strength = Volumes::strength_of(plain.light.shadow_color, true);
    let desc = disc(
        disc_material(r),
        Vec3::new(b.min.x, b.min.y, b.min.z),
        Vec3::new(b.max.x, b.max.y, b.max.z),
        Vec3::new(1.0, 1.0, 1.0),
        0.0,
        strength,
    )
    .expect("a disc");
    let triangles = dungeon_decal(r, (0x02, 0x98), 0x0120, at, &desc);
    assert!(!triangles.is_empty(), "laid on the floor");
    let mut decals = DecalManager::default();
    assert!(decals.create(0x0298_0120, &desc, 0.0, triangles, true));
    let mut with = plain.clone();
    with.decals = decal_batches(r, &mut decals, &mut std::collections::HashMap::new());
    assert!(!with.decals.is_empty(), "drawn with its material");
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let [without, with] = checked(&mut device, |d| {
        [&plain, &with].map(|f| d.render(f, 480, 360, &mut load).expect("read back"))
    });
    let sum = |p: &[u8]| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]);
    let (mut darker, mut brighter) = (0, 0);
    for (a, b) in with.rgba.chunks(4).zip(without.rgba.chunks(4)) {
        let (a, b) = (sum(a), sum(b));
        if b > a + 6 {
            darker += 1;
        }
        if a > b + 6 {
            brighter += 1;
        }
    }
    println!("{darker} pixels darker, {brighter} brighter");
    assert!(darker > 200, "{darker} pixels darker");
    assert_eq!(brighter, 0);
}

/// A torch's light lays a warm pool on the land at night: the town's placed objects' lights,
/// drawn with their pools and without, are brighter in many places with them and darker in none.
///
/// Behaviour: draw.decal.a-torchs-pool-brightens-the-land
#[test]
fn a_torchs_pool_brightens_the_land() {
    use auberean_draw::decal::{
        decal_batches, ground_height, landscape_decal, point_light_pool, pool_material,
        DecalManager,
    };
    use auberean_draw::effects::PlacedEffects;
    use auberean_draw::systems::ParticleSystems;
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let spot = Spot {
        block: (0x6F, 0xA9),
        position: Vec3::new(72.0, 70.0, 91.0),
        heading: 40.0,
        pitch: -20.0,
        time_of_day: 0.05,
        day: None,
    };
    let plain = builder.frame(&spot, 60.0);
    let systems = ParticleSystems::default();
    let mut placed = PlacedEffects::default();
    placed.sync(&systems, r, &plain.placed, 0.0);
    let material = pool_material(r, spot.time_of_day);
    assert_ne!(material, 0, "the night's pool material");
    let mut decals = DecalManager::default();
    for l in placed.lights(spot.block, 160.0, 1.0) {
        let ground = |p: Vec3| builder.ground_square(spot.block, p);
        let Some(square) = ground(l.origin) else {
            continue;
        };
        let Some(h) = ground_height(&square, l.origin.x, l.origin.y) else {
            continue;
        };
        let Some(desc) = point_light_pool(
            material,
            l.origin,
            h,
            l.light.range.base,
            l.light.intensity.base,
            l.light.light.color,
            l.opacity,
        ) else {
            continue;
        };
        let tris = landscape_decal(l.origin, &desc, builder.square_side(), &ground);
        decals.create(l.cell, &desc, 1.0, tris, true);
    }
    let mut lit = plain.clone();
    lit.decals = decal_batches(r, &mut decals, &mut std::collections::HashMap::new());
    assert!(!lit.decals.is_empty(), "the pools are drawn");
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let [without, with] = checked(&mut device, |d| {
        [&plain, &lit].map(|f| d.render(f, 480, 360, &mut load).expect("read back"))
    });
    let sum = |p: &[u8]| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]);
    let (mut brighter, mut darker) = (0, 0);
    for (a, b) in with.rgba.chunks(4).zip(without.rgba.chunks(4)) {
        let (a, b) = (sum(a), sum(b));
        if a > b + 6 {
            brighter += 1;
        }
        if b > a + 6 {
            darker += 1;
        }
    }
    println!("{brighter} pixels brighter, {darker} darker");
    assert!(brighter > 300, "{brighter} pixels brighter");
    assert_eq!(darker, 0);
}

/// Outdoors, the lights objects carry light the objects near them beside the sun: the town's
/// torches at night, their lights taken in by the objects placed about them, leave the frame
/// brighter in more places than darker (a normal map they reach is lit from them in place of the
/// sun), and a human standing by a torch is brighter with it.
///
/// Behaviour: draw.effects.carried-lights-light-what-stands-near-them-outdoors
#[test]
fn carried_lights_light_what_stands_near_them_outdoors() {
    use auberean_draw::effects::PlacedEffects;
    use auberean_draw::scene::Frame;
    use auberean_draw::systems::ParticleSystems;
    use auberean_world::inside::Outside;
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let spot = Spot {
        block: (0x6F, 0xA9),
        position: Vec3::new(72.0, 70.0, 91.0),
        heading: 40.0,
        pitch: -20.0,
        time_of_day: 0.05,
        day: None,
    };
    let plain = builder.frame(&spot, 60.0);
    let systems = ParticleSystems::default();
    let mut placed = PlacedEffects::default();
    placed.sync(&systems, r, &plain.placed, 0.0);
    let dice = auberean_primitives::random::Random::new();
    let carried: Vec<_> = placed
        .lights(spot.block, 160.0, 1.0)
        .iter()
        .map(|l| l.standing(spot.block, 160.0, 1.0, &dice))
        .collect();
    assert!(carried.len() >= 5, "{} torches", carried.len());
    let outside = Outside::of(&plain.light);
    let mut lit = plain.clone();
    builder.light_carried(&spot, &mut lit.objects, &carried, &outside);
    let taken = lit.objects.iter().filter(|b| b.points.count > 0).count();
    assert!(taken > 0, "objects near the torches take them");
    // A human standing a meter east of one of the town's torches, seen from 4 m south of it, lit
    // by the torch and not.
    let torch = carried
        .iter()
        .find(|l| l.most_range > 0.0 && l.cell >> 16 == 0x6FA9)
        .expect("a torch of the block reaching anything")
        .clone();
    let (x, y) = (torch.origin.x + 1.0, torch.origin.y);
    let square = builder
        .ground_square(spot.block, Vec3::new(x, y, 0.0))
        .expect("the ground by the torch");
    let z = auberean_draw::decal::ground_height(&square, x, y).expect("its height");
    let feet = Vec3::new(x, y, z);
    let close = Spot {
        position: Vec3::new(x, y - 4.0, z + 1.7),
        heading: 0.0,
        pitch: -10.0,
        ..spot
    };
    let mut scene = builder.frame(&close, 60.0);
    let one = std::slice::from_ref(&torch);
    builder.light_carried(&close, &mut scene.objects, one, &outside);
    let recs = CachedRecords::new(r);
    let figure = Figure::assemble(&recs, DataId(0x1F00_0023)).expect("the human male");
    let poses = figure.pose(&recs);
    let place = Mat4::from_parts(feet, Quat::IDENTITY, Vec3::new(1.0, 1.0, 1.0));
    let body = BodyDrawer::default().batches(r, &figure, &poses, &place);
    let mut by_torch = body.clone();
    let way = builder.light_outdoors(&close, &mut by_torch, feet, torch.cell, &outside, one);
    assert!(way.is_some(), "the torch throws its shadow");
    assert!(
        by_torch.iter().any(|b| b.points.count > 0),
        "the human takes it"
    );
    let with_body = |batches: Vec<auberean_draw::scene::ObjectBatch>| -> Frame {
        let mut f = scene.clone();
        f.objects.extend(batches);
        f
    };
    let (unlit, beside) = (with_body(body), with_body(by_torch));
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let [dark, bright, alone, by] = checked(&mut device, |d| {
        [&plain, &lit, &unlit, &beside]
            .map(|f| d.render(f, 480, 360, &mut load).expect("read back"))
    });
    let sum = |p: &[u8]| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]);
    let count = |a: &[u8], b: &[u8]| {
        let (mut brighter, mut darker) = (0, 0);
        for (x, y) in a.chunks(4).zip(b.chunks(4)) {
            let (x, y) = (sum(x), sum(y));
            if x > y + 6 {
                brighter += 1;
            }
            if y > x + 6 {
                darker += 1;
            }
        }
        (brighter, darker)
    };
    let (brighter, darker) = count(&bright.rgba, &dark.rgba);
    println!("the objects: {brighter} pixels brighter, {darker} darker");
    assert!(brighter > 100, "{brighter} pixels brighter");
    assert!(
        darker < brighter,
        "{brighter} pixels brighter, {darker} darker"
    );
    let (brighter, darker) = count(&by.rgba, &alone.rgba);
    println!("the human: {brighter} pixels brighter, {darker} darker");
    assert!(brighter > 50, "{brighter} pixels brighter");
}

/// A body standing on the land takes the sun's shadow map about its spot in its light: over the
/// camp's own spots it is darkened where a shadow falls, never brightened, and whole off the map.
///
/// Behaviour: draw.world.a-body-takes-the-sun-shadow-about-its-spot
#[test]
fn a_body_takes_the_sun_shadow_about_its_spot() {
    use auberean_draw::scene::{Camera, SceneBuilder, Spot};
    use auberean_primitives::Vec3;
    use auberean_world::math::Mat4;
    let r = &crate::common::install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let camp = Spot {
        block: (0xC1, 0x56),
        position: Vec3::new(122.842, 89.758, 81.6),
        heading: 90.0,
        pitch: 5.0,
        time_of_day: 0.45,
        day: None,
    };
    let mut frame = builder.frame(&camp, 60.0);
    assert!(frame.shadow.is_some(), "the camp's sun casts shadows");
    frame.water.clear();
    frame.shore.clear();
    frame.foam.clear();
    frame.terrain.clear();
    frame.sky_before.clear();
    frame.sky_after.clear();
    frame.clouds = None;
    frame.camera = Camera {
        position: Vec3::new(60.0, 60.0, 500.0),
        heading: 0.0,
        pitch: 0.0,
        fov_y: 60.0,
        near: 0.1,
        far: 2000.0,
        orthographic: None,
    };
    let mut wall = crate::fade::wall(10.0, 0.5, false, 10.0);
    wall.transform = Mat4::translation(Vec3::new(60.0, 60.0, 500.0));
    wall.visible = None;
    let mut device = renderer();
    let mut at = |device: &mut auberean_gpu::Renderer, ground: Option<Vec3>| {
        frame.objects = vec![auberean_draw::scene::ObjectBatch {
            posed: None,
            ground,
            ..wall.clone()
        }];
        crate::fade::middle(device, &frame)
    };
    let whole = at(&mut device, None);
    let off = at(&mut device, Some(Vec3::new(-20000.0, -20000.0, 80.0)));
    assert_eq!(off, whole, "off the map");
    let (mut least, mut most) = (255u8, 0u8);
    for i in 0..8 {
        for j in 0..8 {
            let spot = Vec3::new(i as f32 * 20.0 + 5.0, j as f32 * 20.0 + 5.0, 80.0);
            let v = at(&mut device, Some(spot));
            least = least.min(v);
            most = most.max(v);
        }
    }
    println!("whole {whole}, over the camp {least}..{most}");
    assert!(most <= whole, "{most} over {whole}");
    assert!(
        f32::from(least) < 0.95 * f32::from(whole),
        "{least} against {whole}"
    );
}
