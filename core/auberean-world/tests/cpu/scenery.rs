//! Scenery on a synthetic flat block: which scene a vertex grows, and where a group puts its
//! object; and the landscape's ambient sound points, which follow the same chain from a vertex to
//! its scenery kind.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_assets::landblock::Landblock;
use auberean_assets::region::{
    Landscape, SceneDesc, SceneryKind, SurfaceDesc, TerrainDesc, TerrainScenery, TerrainType,
    VertexFields, WeightedScene,
};
use auberean_assets::visual::{ScatterGroup, Scene, SceneLevel, SceneObject};
use auberean_primitives::{DataId, Vec3};
use auberean_world::land::LandConstants;
use auberean_world::math::rotation_rows;
use auberean_world::scenery::{
    block_scenery, vertex_scene, ClearCylinder, KeptClear, SceneSource, SceneryInput,
    SceneryObject, CLEARANCE_GROUP,
};
use auberean_world::sound::{block_sound_points, vertex_sound};
use auberean_world::terrain::{build_block, Surfaces};

const SCENE: u32 = 0x0200_0001;
const TREE: u32 = 0x0400_0001;

fn land() -> LandConstants {
    LandConstants::from_record(&Landscape {
        cell_side: 20.0,
        road_width: 4.0,
        cells_per_block_side: 8,
        pixels_per_cell_side: 2,
        max_object_height: 200.0,
        sky_height: 2000.0,
        vertex_fields: VertexFields {
            terrain: 7,
            scene: 5,
            encounter: 4,
            color: 8,
            road: 4,
            replace: 4,
        },
        heights: vec![0.0; 256],
    })
    .expect("in range")
}

/// Terrain kind 1 grows scenery kind 5 for scene field 1; kind 5 has one scene.
fn descs() -> (TerrainDesc, SceneDesc) {
    let terrain = TerrainDesc {
        version: 0,
        default_terrain: None,
        terrains: vec![TerrainType {
            name: String::from("grass"),
            surface: 0,
            index: 1,
            hard_edge_angle: std::f32::consts::FRAC_1_SQRT_2,
            default_scenery: None,
            scenery: vec![TerrainScenery { kind: 5, index: 1 }],
        }],
    };
    let scenes = SceneDesc {
        version: 0,
        scene_types: vec![SceneryKind {
            index: 5,
            sound: 0,
            scenes: vec![WeightedScene {
                scene: DataId(SCENE),
                frequency: 1.0,
                rotate: 0,
            }],
        }],
    };
    (terrain, scenes)
}

/// A group that always places, two east and three north of its vertex, turned `turn` degrees, on
/// the ground, holding one tree.
fn group(frequency: f32, turn: f32) -> ScatterGroup {
    ScatterGroup {
        min_location: Vec3::new(2.0, 3.0, 0.0),
        max_location: Vec3::new(2.0, 3.0, 0.0),
        min_rotation: turn,
        max_rotation: turn,
        frequency,
        min_scale: Vec3::new(1.0, 1.0, 1.0),
        max_scale: Vec3::new(1.0, 1.0, 1.0),
        min_slope: 1.0,
        max_slope: 0.0,
        above_dry_land_type: 3,
        below_ground_type: 3,
        objects: vec![SceneObject::Record {
            kind: 1,
            id: DataId(TREE),
        }],
        ..ScatterGroup::default()
    }
}

struct Scenes(HashMap<u32, Arc<Scene>>);

impl SceneSource for Scenes {
    fn scene(&self, id: DataId) -> Option<Arc<Scene>> {
        self.0.get(&id.0).cloned()
    }
}

fn scenes(g: ScatterGroup) -> Scenes {
    Scenes(HashMap::from([(
        SCENE,
        Arc::new(Scene {
            version: 0,
            allow_rotate: false,
            decoration: None,
            levels: vec![SceneLevel {
                min_distance: 0.0,
                ideal_distance: 50.0,
                max_distance: 100.0,
                objects: vec![SceneObject::Group(Box::new(g))],
            }],
        }),
    )]))
}

/// The scenery of block (10, 10) whose only scenery vertex is `(i, j)`, with `road` set there.
fn place(g: ScatterGroup, i: usize, j: usize, road: bool) -> Vec<SceneryObject> {
    place_by(g, i, j, road, &[])
}

/// The scenery of [`place`] with the block's own placed objects `obstacles`.
fn place_by(
    g: ScatterGroup,
    i: usize,
    j: usize,
    road: bool,
    obstacles: &[KeptClear],
) -> Vec<SceneryObject> {
    let land = land();
    let (terrain, scene_desc) = descs();
    let mut words = vec![1u32; 17 * 17];
    words[17 * i + j] = 1 | (1 << 7);
    if road {
        for (di, dj) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            words[17 * (i + di) + j + dj] |= 1 << 24;
        }
    }
    let lb = Landblock {
        info: DataId::NONE,
        path_map: DataId::NONE,
        heights: vec![0; 17 * 17],
        vertices: words,
    };
    let blocks = HashMap::from([((10, 10), lb)]);
    let surface = SurfaceDesc::default();
    let surfaces = Surfaces {
        terrain: &terrain,
        surface: &surface,
    };
    let tb = build_block(&land, surfaces, &blocks, 10, 10).expect("the block");
    let input = SceneryInput {
        land: &land,
        terrain: &terrain,
        scenes: &scene_desc,
        vertices: &blocks[&(10, 10)].vertices,
        block: &tb,
        obstacles,
        degrade: 1.0,
        water: None,
    };
    block_scenery(&input, &scenes(g))
}

/// A vertex grows a scene through its terrain kind's scenery for its scene field, picked among
/// that scenery kind's scenes by frequency; a field the kind does not list grows nothing.
///
/// Behaviour: world.scenery.a-vertex-grows-its-terrain-kinds-scene
#[test]
fn a_vertex_grows_its_terrain_kinds_scene() {
    let (terrain, mut scenes) = descs();
    scenes.scene_types[0].scenes.push(WeightedScene {
        scene: DataId(0x0200_0002),
        frequency: 3.0,
        rotate: 0,
    });
    assert_eq!(
        vertex_scene(&terrain, &scenes, 1, 1, 0.2),
        Some(DataId(SCENE)),
        "a pick in the first quarter takes the first scene"
    );
    assert_eq!(
        vertex_scene(&terrain, &scenes, 1, 1, 0.3),
        Some(DataId(0x0200_0002))
    );
    assert_eq!(vertex_scene(&terrain, &scenes, 1, 2, 0.2), None);
    assert_eq!(vertex_scene(&terrain, &scenes, 9, 1, 0.2), None);
}

/// A group stands at its offset from its vertex (vertices a hair under ten units apart), on the
/// ground, turned by its rotation clockwise from north; its object stands with it.
///
/// Behaviour: world.scenery.a-group-stands-at-its-offset-turned-on-the-ground
#[test]
fn a_group_stands_at_its_offset_turned_on_the_ground() {
    let placed = place(group(1.0, 90.0), 4, 6, false);
    assert_eq!(placed.len(), 1);
    let o = placed[0];
    assert_eq!(o.object, DataId(TREE));
    assert!(
        (o.origin.x - (4.0 * 9.9998 + 2.0)).abs() < 1e-3,
        "{:?}",
        o.origin
    );
    assert!(
        (o.origin.y - (6.0 * 9.9998 + 3.0)).abs() < 1e-3,
        "{:?}",
        o.origin
    );
    assert!(o.origin.z.abs() < 1e-4);
    let north = rotation_rows(o.rotation)[1];
    assert!(
        (north[0] - 1.0).abs() < 1e-4 && north[1].abs() < 1e-4,
        "turned 90 degrees, north faces east: {north:?}"
    );
    assert!(!o.frill);
}

/// A group places nothing when the hash misses its frequency, when it would stand nearer the
/// block's edge than its clearance, or on a road.
///
/// Behaviour: world.scenery.a-group-is-kept-off-the-edge-and-the-road
#[test]
fn a_group_is_kept_off_the_edge_and_the_road() {
    assert!(place(group(0.0, 0.0), 4, 6, false).is_empty());
    assert!(
        place(group(1.0, 0.0), 16, 6, false).is_empty(),
        "two east of the last vertex is off the block"
    );
    assert!(
        place(group(1.0, 0.0), 4, 6, true).is_empty(),
        "a square with road at all four corners is all road"
    );
    let mut wide = group(1.0, 0.0);
    wide.clearance_radius = 50.0;
    assert!(place(wide, 4, 6, false).is_empty(), "too near the edge");
}

/// A block's placed object keeps a group from its spot (within the group's clearance radius), or,
/// when its clearance properties set boxes or upright cylinders about it, from those, measured in
/// the object's own frame: a group whose spot falls in one places nothing; a box with no extent
/// and a cylinder with no radius keep nothing clear. The properties name each box's corners and
/// each cylinder's radius, base and height; a box or cylinder named is there with those before it.
///
/// Behaviour: world.scenery.placed-objects-keep-their-clearance-boxes-and-cylinders
#[test]
fn placed_objects_keep_their_clearance_boxes_and_cylinders() {
    use auberean_assets::property::{Property, PropertyCollection, PropertyGroup, PropertyValue};
    use auberean_primitives::Quat;
    // The group's spot: vertex (4, 6) and two east, three north.
    let spot = Vec3::new(4.0 * 9.9998 + 2.0, 6.0 * 9.9998 + 3.0, 0.0);
    let mut g = group(1.0, 0.0);
    g.clearance_radius = 1.0;
    let at = |o: Vec3| KeptClear {
        origin: o,
        rotation: Quat::IDENTITY,
        boxes: Vec::new(),
        cylinders: Vec::new(),
    };
    let placed = |obstacles: &[KeptClear]| place_by(g.clone(), 4, 6, false, obstacles).len();
    assert_eq!(placed(&[]), 1);
    assert_eq!(
        placed(&[at(Vec3::new(spot.x + 0.9, spot.y, 0.0))]),
        0,
        "a spot within the radius"
    );
    assert_eq!(
        placed(&[at(Vec3::new(spot.x + 1.1, spot.y, 0.0))]),
        1,
        "a spot beyond it"
    );
    // An object 8 m east of the spot, turned a quarter (its north is the world's east): its box
    // reaching 7 to 9 m along its north covers the spot's west, the group's radius taking it in.
    let turned = Quat {
        w: std::f32::consts::FRAC_1_SQRT_2,
        x: 0.0,
        y: 0.0,
        z: -std::f32::consts::FRAC_1_SQRT_2,
    };
    let east = Vec3::new(spot.x - 8.0, spot.y, 0.0);
    let with_box = |lo: Vec3, hi: Vec3| KeptClear {
        origin: east,
        rotation: turned,
        boxes: vec![(lo, hi)],
        cylinders: Vec::new(),
    };
    assert_eq!(
        placed(&[with_box(
            Vec3::new(-1.0, 7.5, -1.0),
            Vec3::new(1.0, 9.0, 3.0)
        )]),
        0,
        "the spot in the box, in the object's frame"
    );
    assert_eq!(
        placed(&[with_box(
            Vec3::new(-1.0, 5.0, -1.0),
            Vec3::new(1.0, 6.5, 3.0)
        )]),
        1,
        "the box 1.5 m short of the spot"
    );
    assert_eq!(
        placed(&[with_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0))]),
        1,
        "a box of no extent keeps the object's spot no more"
    );
    let with_cylinder = |radius: f32| KeptClear {
        origin: east,
        rotation: turned,
        boxes: Vec::new(),
        cylinders: vec![ClearCylinder {
            low: Vec3::new(0.0, 8.0, 0.0),
            height: 4.0,
            radius,
        }],
    };
    assert_eq!(
        placed(&[with_cylinder(0.5)]),
        0,
        "the spot within the cylinder"
    );
    assert_eq!(placed(&[with_cylinder(0.0)]), 1, "a cylinder of no radius");

    let property = |id: u32, value: PropertyValue| Property {
        id,
        value_type: 0,
        group: CLEARANCE_GROUP,
        value,
    };
    let props = PropertyCollection {
        groups: vec![PropertyGroup {
            group: CLEARANCE_GROUP,
            properties: vec![
                property(
                    0x4000_0029,
                    PropertyValue::Vector(Vec3::new(-1.0, -2.0, 0.0)),
                ),
                property(0x4000_002D, PropertyValue::Vector(Vec3::new(1.0, 2.0, 3.0))),
                property(0x4000_0034, PropertyValue::Float(2.5)),
                property(0x4000_0032, PropertyValue::Float(9.0)),
            ],
            optional: Vec::new(),
        }],
    };
    let c = KeptClear::of_entity(east, turned, Some(&props));
    assert_eq!(
        c.boxes,
        vec![
            (Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0)),
            (Vec3::new(-1.0, -2.0, 0.0), Vec3::new(1.0, 2.0, 3.0))
        ],
        "the second box, and the first with nothing set"
    );
    assert_eq!(
        c.cylinders,
        vec![ClearCylinder {
            low: Vec3::new(0.0, 0.0, 0.0),
            height: 2.5,
            radius: 0.0,
        }],
        "a base given as a number sets nothing"
    );
    assert!(KeptClear::of_entity(east, turned, None).boxes.is_empty());
}

// The landscape's ambient sound points.

const FOREST: u32 = 0x8100_0018;
const MEADOW: u32 = 0x8100_0019;

/// The retail grid: 20-meter cells, 8 to a block side, 2 vertex steps a cell (17 vertices a
/// side); heights 0, 1, 2, ... by height byte.
fn sound_land() -> LandConstants {
    #[allow(clippy::cast_precision_loss)] // 256 entries
    let heights = (0..256).map(|h| h as f32).collect();
    LandConstants::from_record(&Landscape {
        cell_side: 20.0,
        road_width: 4.0,
        cells_per_block_side: 8,
        pixels_per_cell_side: 2,
        max_object_height: 200.0,
        sky_height: 2000.0,
        vertex_fields: VertexFields {
            terrain: 7,
            scene: 5,
            encounter: 4,
            color: 8,
            road: 4,
            replace: 4,
        },
        heights,
    })
    .expect("in range")
}

fn sound_terrain(index: u32, scenery: Vec<TerrainScenery>, default: Option<u32>) -> TerrainType {
    TerrainType {
        name: String::from("ground"),
        surface: 0,
        index,
        hard_edge_angle: 0.5,
        default_scenery: default.map(|kind| TerrainScenery { kind, index: 0 }),
        scenery,
    }
}

fn sound_kind(index: u32, sound: u32) -> SceneryKind {
    SceneryKind {
        index,
        sound,
        scenes: Vec::new(),
    }
}

/// Terrain kind 1: scene field 1 grows scenery kind 5 (the forest's sound), field 2 kind 6 (no
/// sound), field 3 kind 0, field 4 kind 9 (not described), anything else the default kind 7 (the
/// meadow's). Terrain kind 2 has no default scenery. The default terrain grows kind 5 for field 1.
fn sound_descs() -> (TerrainDesc, SceneDesc) {
    let t = TerrainDesc {
        version: 0,
        default_terrain: Some(sound_terrain(
            99,
            vec![TerrainScenery { kind: 5, index: 1 }],
            None,
        )),
        terrains: vec![
            sound_terrain(
                1,
                vec![
                    TerrainScenery { kind: 5, index: 1 },
                    TerrainScenery { kind: 6, index: 2 },
                    TerrainScenery { kind: 0, index: 3 },
                    TerrainScenery { kind: 9, index: 4 },
                ],
                Some(7),
            ),
            sound_terrain(2, Vec::new(), None),
        ],
    };
    let s = SceneDesc {
        version: 0,
        scene_types: vec![
            sound_kind(5, FOREST),
            sound_kind(6, 0),
            sound_kind(7, MEADOW),
            sound_kind(0, MEADOW),
        ],
    };
    (t, s)
}

/// Behaviour: world.sound.a-vertex-names-its-scenery-kinds-sound
#[test]
fn a_vertex_names_the_sound_of_its_scenery_kind_and_nothing_past_a_gap() {
    let (t, s) = sound_descs();
    assert_eq!(vertex_sound(&t, &s, 1, 1), Some(FOREST));
    // The terrain's default scenery for a field it does not list.
    assert_eq!(vertex_sound(&t, &s, 1, 31), Some(MEADOW));
    // A scenery kind whose sound word is 0, the scenery kind 0 (even though a kind 0 is described),
    // and a scenery kind the scene description lacks: no sound.
    assert_eq!(vertex_sound(&t, &s, 1, 2), None);
    assert_eq!(vertex_sound(&t, &s, 1, 3), None);
    assert_eq!(vertex_sound(&t, &s, 1, 4), None);
    // A terrain kind with no scenery for the field and no default.
    assert_eq!(vertex_sound(&t, &s, 2, 1), None);
    // A terrain kind not listed takes the default terrain.
    assert_eq!(vertex_sound(&t, &s, 50, 1), Some(FOREST));
}

/// Behaviour: world.sound.a-blocks-points-fall-short-of-its-far-edges
#[test]
fn a_blocks_points_stand_a_seventeenth_of_its_side_apart_at_their_heights() {
    let land = sound_land();
    let (t, s) = sound_descs();
    // Every vertex terrain kind 1; scene field 1 (the forest) but for vertex (3, 5), field 2 (no
    // sound); height byte = x + y.
    let side = 17;
    let mut vertices = vec![1 | (1 << 7); side * side];
    vertices[3 * side + 5] = 1 | (2 << 7);
    let heights = (0..side * side)
        .map(|k| u8::try_from(k / side + k % side).expect("small"))
        .collect();
    let block = Landblock {
        info: DataId(0),
        path_map: DataId(0),
        heights,
        vertices,
    };
    let points = block_sound_points(&land, &t, &s, &block);
    assert_eq!(points.len(), side * side - 1);
    assert!(points.iter().all(|p| p.kind == FOREST));
    let step = 160.0 / 17.0;
    let first = points[0];
    assert_eq!(
        (first.origin.x, first.origin.y, first.origin.z),
        (0.0, 0.0, 0.0)
    );
    // The second point is the next vertex north: y fastest.
    assert_eq!(points[1].origin.y, step);
    assert_eq!(points[1].origin.x, 0.0);
    // The last row and column stop short of the block's far edges, at 16/17 of its side.
    let last = points[points.len() - 1];
    assert_eq!(last.origin.x, 16.0 * step);
    assert_eq!(last.origin.y, 16.0 * step);
    assert!(last.origin.x < 151.0);
    assert_eq!(last.origin.z, 32.0);
    // The vertex without a sound is left out, so x = 3's row holds sixteen points.
    let row3 = points.iter().filter(|p| p.origin.x == 3.0 * step).count();
    assert_eq!(row3, side - 1);
    assert!(!points
        .iter()
        .any(|p| p.origin.x == 3.0 * step && p.origin.y == 5.0 * step));
}
