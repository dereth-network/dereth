//! The dynamic objects' volumes of shadow from the install's bodies.

use std::collections::HashMap;

use auberean_animation::body::Body as Figure;
use auberean_animation::CachedRecords;
use auberean_assets::property::{PropertyList, PropertyValue};
use auberean_draw::body::BodyDrawer;
use auberean_draw::volume::{VolumeMaker, VolumeRoom};
use auberean_physics::world::render::VOLUME_SHADOW_CAST;
use auberean_primitives::{DataId, Vec3};
use auberean_world::math::{normalize, Mat4};
use auberean_world::place::PlaceRecords;

use crate::common::install;

/// The human male's look, and the entity a human male player is made from.
const HUMAN_MALE: DataId = DataId(0x1F00_0023);
const HUMAN_MALE_PLAYER: u32 = 0x4700_00CD;

/// Whether the triangles `indices` over `vertices` close up: each edge, by where its ends stand,
/// run as often one way as the other.
fn closed(vertices: &[[f32; 3]], indices: &[u16]) -> bool {
    #[allow(clippy::cast_possible_truncation)]
    let key = |p: [f32; 3]| p.map(|x| (x * 1000.0).round() as i64);
    let mut count: HashMap<([i64; 3], [i64; 3]), i32> = HashMap::new();
    for t in indices.as_chunks::<3>().0 {
        for k in 0..3 {
            let a = key(vertices[usize::from(t[k])]);
            let b = key(vertices[usize::from(t[(k + 1) % 3])]);
            if a == b {
                continue;
            }
            *count.entry((a, b)).or_default() += 1;
            *count.entry((b, a)).or_default() -= 1;
        }
    }
    count.values().all(|&c| c == 0)
}

/// A player casts a volume of shadow: the entity a human male player is made from says so among
/// its drawing properties, and the human male's posed body casts a closed volume from each of its
/// meshes of more than three triangles (five of its eight: the others are single triangles),
/// within the frame's room, each in its draw's frame.
///
/// Behaviour: draw.volume.a-player-casts-a-closed-volume-of-shadow
#[test]
fn a_player_casts_a_closed_volume_of_shadow() {
    let r = &install().records;
    let list: PropertyList = r.get(0x3400_0000).expect("the property list");
    let lookup = |id: u32| r.entity(DataId(id));
    let player = r
        .entity(DataId(HUMAN_MALE_PLAYER))
        .expect("the player's entity");
    let casts = player
        .total_properties(&lookup)
        .get(&list, VOLUME_SHADOW_CAST)
        .map(|p| p.value.clone());
    assert_eq!(casts, Some(PropertyValue::Bool(true)), "a player casts one");

    let recs = CachedRecords::new(r);
    let figure = Figure::assemble(&recs, HUMAN_MALE).expect("the human male");
    let poses = figure.pose(&recs);
    let place = Mat4::from_parts(
        Vec3::new(10.0, 20.0, 30.0),
        auberean_primitives::Quat::IDENTITY,
        Vec3::new(1.0, 1.0, 1.0),
    );
    let batches = BodyDrawer::default().batches(r, &figure, &poses, &place);
    let meshes = {
        let mut m: Vec<u32> = batches.iter().map(|b| b.mesh).collect();
        m.sort_unstable();
        m.dedup();
        m
    };
    assert!(meshes.len() > 3, "{} meshes", meshes.len());
    let sun = normalize(Vec3::new(0.5, 0.3, -0.8));
    let mut room = VolumeRoom::FRAME;
    let mut volumes = Vec::new();
    VolumeMaker::default().cast(r, &batches, sun, 1.0, 2, &mut room, &mut volumes);
    println!(
        "{} meshes, {} volumes, {} vertices, {} indices",
        meshes.len(),
        volumes.len(),
        VolumeRoom::FRAME.vertices - room.vertices,
        VolumeRoom::FRAME.indices - room.indices
    );
    // the meshes of more than three triangles at the level the volume is made from
    let solid = meshes
        .iter()
        .filter(|&&m| {
            let mesh: auberean_assets::mesh::Mesh = r.get(m).expect("a mesh");
            auberean_draw::volume::volume_level(&mesh, 1.0, 2).is_some_and(|l| {
                mesh.levels[l]
                    .fragments
                    .iter()
                    .map(|f| f.indices.len() / 3)
                    .sum::<usize>()
                    > 3
            })
        })
        .count();
    assert!(solid >= 5, "{solid} meshes with a body");
    assert_eq!(volumes.len(), solid, "one from each");
    for v in &volumes {
        assert!(closed(&v.vertices, &v.indices), "a volume closes up");
        assert_eq!(v.transform, place, "in its draw's frame");
        assert!(v.indices.iter().all(|&i| usize::from(i) < v.vertices.len()));
    }
    assert_eq!(VolumeRoom::FRAME.volumes - room.volumes, volumes.len());
}
