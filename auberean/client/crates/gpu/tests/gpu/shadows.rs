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
