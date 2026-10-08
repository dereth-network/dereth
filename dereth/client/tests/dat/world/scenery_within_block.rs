//! Which generated scenery a landblock keeps at its edges, and how a slope-aligned piece stands.
//!
//! A generated object is kept only if it stays within its landblock, and what has to stay inside
//! depends on its shape: a tree's trunk cylinders, a mesh object's sorting sphere, a sphere
//! object's sorting sphere, and a shapeless object's origin. So a tree whose canopy reaches over
//! the block line grows while its trunk is inside, a piece whose cylinder reaches over it does
//! not, and a mesh rock whose sphere reaches over it does not either, though its origin is inside;
//! a bare model whose mesh's sphere stays inside is placed. A piece placed to follow the slope is
//! turned to face downhill and stays upright.
//!
//! Fixture: the retail cell and portal dats, through the client's own landblock content (block
//! `0xA9B3` south of Holtburg, blocks `0x1381`, `0x31F0`, `0x31F2` and `0x805E`). Missing dats
//! fail. No server or GPU.

use dereth_client_runtime::world_build::{land_content, read_landblock, read_scene};
use dereth_dat::RetailDatStore;
use dereth_physics::V3;
use dereth_primitives::frame::{get_vector_heading, l2g, localtoglobal, localtoglobalvec};
use dereth_primitives::{DataId, Vec3};
use dereth_terrain::land::mesh::{generate_landblock_with_table, height_table, LandblockMesh};
use dereth_terrain::scenery::{generate_scenery, PlacedScenery, SceneryEnv, WithinBlockShape};
use dereth_world_data::env_cells::static_geometry;

fn store() -> RetailDatStore {
    dereth_dat::testing::open_store_or_fail()
}

/// The block's full-detail mesh, and the scenery the client keeps on it.
fn kept(s: &RetailDatStore, bx: i32, by: i32) -> (LandblockMesh, Vec<PlacedScenery>) {
    let region = dereth_world_data::landblock::load_region(s).expect("the region decodes");
    let table = height_table(&region);
    let lb = read_landblock(s, bx, by).expect("the landblock decodes");
    let mesh = generate_landblock_with_table(
        &lb,
        &region,
        &table,
        bx,
        by,
        1,
        dereth_terrain::land::mesh::Direction::InViewerBlock,
    );
    let placed = land_content(s, &region, &lb, &mesh, bx, by, true).placed;
    (mesh, placed)
}

/// The block's scenery with every object reduced to its origin: what the generator offers the
/// within-block test before the object's shape is consulted. No building is asked about, which
/// only adds candidates.
fn candidates(s: &RetailDatStore, bx: i32, by: i32) -> Vec<PlacedScenery> {
    let region = dereth_world_data::landblock::load_region(s).expect("the region decodes");
    let table = height_table(&region);
    let lb = read_landblock(s, bx, by).expect("the landblock decodes");
    let mesh = generate_landblock_with_table(
        &lb,
        &region,
        &table,
        bx,
        by,
        1,
        dereth_terrain::land::mesh::Direction::InViewerBlock,
    );
    let env = SceneryEnv {
        scenes: &|id| read_scene(s, id),
        has_building: &|_| false,
        shape: &|_| Some(WithinBlockShape::default()),
    };
    generate_scenery(&lb, &mesh, &region, bx, by, &env)
}

fn find(placed: &[PlacedScenery], id: u32, x: f32, y: f32) -> Option<&PlacedScenery> {
    placed.iter().find(|p| {
        p.gfxobj == DataId(id)
            && (p.frame.origin.x - x).abs() < 0.01
            && (p.frame.origin.y - y).abs() < 0.01
    })
}

/// Whether a sphere at `c` with radius `r` reaches over a block line.
fn crosses(c: Vec3, r: f32) -> bool {
    c.x < r || c.y < r || c.x >= 192.0 - r || c.y >= 192.0 - r
}

/// Behaviour: world.scenery.a-tree-near-the-block-line-grows-while-its-trunk-is-inside
#[test]
fn a_tree_whose_canopy_crosses_the_block_line_grows_where_its_trunk_is_inside() {
    let s = store();
    let (_, placed) = kept(&s, 0xA9, 0xB3);
    // The tree 3.2 m from the block's west line and 6.5 m from its south line.
    let tree = find(&placed, 0x0200_02D3, 3.179, 6.491)
        .expect("the tree near block 0xA9B3's south-west corner grows");
    let mut stats = dereth_world_data::setup::SetupPartStats::default();
    let g = static_geometry(&s, tree.gfxobj, &mut stats).expect("the tree's setup decodes");
    assert!(!g.caches_physics_bsp(), "the tree has no physics mesh");
    assert!(!g.cyl_spheres.is_empty(), "the tree has a trunk");
    let canopy = localtoglobal(&tree.frame, g.sorting_sphere.center);
    eprintln!(
        "the tree at {:?}: sorting sphere {canopy:?} r {:.3}, {} trunk(s)",
        tree.frame.origin,
        g.sorting_sphere.radius,
        g.cyl_spheres.len()
    );
    assert!(
        crosses(canopy, g.sorting_sphere.radius),
        "the tree's sorting sphere {canopy:?} r {} stays inside, so it does not show the arm",
        g.sorting_sphere.radius
    );
    for c in &g.cyl_spheres {
        let low = localtoglobal(&tree.frame, c.low_pt);
        assert!(
            !crosses(low, c.radius),
            "a trunk at {low:?} r {} crosses",
            c.radius
        );
    }
}

/// Behaviour: world.scenery.a-mesh-rock-over-the-block-line-is-not-placed
#[test]
fn a_mesh_rock_whose_sphere_crosses_the_block_line_is_not_placed() {
    let s = store();
    // A bare graphics object with a physics mesh, 9.75 m from the east line and 13.2 m from the
    // north one: its origin is inside, its mesh's sphere is not.
    let (id, x, y) = (0x0100_36E6, 182.25, 178.80);
    let offered = candidates(&s, 0x31, 0xF2);
    let rock = find(&offered, id, x, y).expect("the generator offers the rock");
    let mut stats = dereth_world_data::setup::SetupPartStats::default();
    let g = static_geometry(&s, rock.gfxobj, &mut stats).expect("the rock's mesh decodes");
    assert!(g.caches_physics_bsp(), "the rock has a physics mesh");
    assert!(g.spheres.is_empty() && g.cyl_spheres.is_empty());
    let c = localtoglobal(&rock.frame, g.sorting_sphere.center);
    assert!(
        crosses(c, g.sorting_sphere.radius),
        "the rock's sphere {c:?} r {} stays inside",
        g.sorting_sphere.radius
    );
    let (_, placed) = kept(&s, 0x31, 0xF2);
    assert!(
        find(&placed, id, x, y).is_none(),
        "the rock whose sphere crosses the block line is placed"
    );
}

/// Behaviour: world.scenery.a-piece-whose-cylinder-reaches-over-the-block-line-is-not-placed
#[test]
fn a_piece_whose_cylinder_crosses_the_block_line_is_not_placed() {
    let s = store();
    // A piece with one collision cylinder, 0.86 m from block 0x1381's south line: its origin is
    // inside, its cylinder is not.
    let (id, x, y) = (0x0200_05C9, 38.22, 0.857);
    let offered = candidates(&s, 0x13, 0x81);
    let piece = find(&offered, id, x, y).expect("the generator offers the piece");
    let mut stats = dereth_world_data::setup::SetupPartStats::default();
    let g = static_geometry(&s, piece.gfxobj, &mut stats).expect("the piece's setup decodes");
    assert!(!g.caches_physics_bsp(), "the piece has no physics mesh");
    assert!(
        !g.cyl_spheres.is_empty(),
        "the piece has collision cylinders"
    );
    eprintln!(
        "the piece at {:?}: {} cylinder(s), {} sphere(s), sorting sphere r {:.3}",
        piece.frame.origin,
        g.cyl_spheres.len(),
        g.spheres.len(),
        g.sorting_sphere.radius
    );
    let over: Vec<_> = g
        .cyl_spheres
        .iter()
        .map(|c| (localtoglobal(&piece.frame, c.low_pt), c.radius))
        .filter(|&(low, r)| crosses(low, r))
        .collect();
    assert!(
        !over.is_empty(),
        "every cylinder stays inside, so the piece does not show the arm"
    );
    eprintln!("cylinders over the line: {over:?}");
    let (_, placed) = kept(&s, 0x13, 0x81);
    assert!(
        find(&placed, id, x, y).is_none(),
        "the piece whose cylinder crosses the block line is placed"
    );
}

/// Behaviour: world.scenery.a-bare-model-near-the-block-line-is-placed-when-its-mesh-sphere-stays-inside
#[test]
fn a_bare_model_whose_mesh_sphere_stays_inside_the_block_is_placed() {
    let s = store();
    // A bare graphics object with a physics mesh, 8 m from block 0x31F0's north line.
    let (id, x, y) = (0x0100_369F, 150.30, 184.03);
    let (_, placed) = kept(&s, 0x31, 0xF0);
    let model = find(&placed, id, x, y).expect("the bare model near the north line is placed");
    let mut stats = dereth_world_data::setup::SetupPartStats::default();
    let g = static_geometry(&s, model.gfxobj, &mut stats).expect("the model's mesh decodes");
    assert!(g.caches_physics_bsp(), "the model has a physics mesh");
    assert!(g.spheres.is_empty() && g.cyl_spheres.is_empty());
    let c = localtoglobal(&model.frame, g.sorting_sphere.center);
    eprintln!(
        "the model at {:?}: sphere {c:?} r {:.3}",
        model.frame.origin, g.sorting_sphere.radius
    );
    assert!(
        !crosses(c, g.sorting_sphere.radius),
        "the model's sphere {c:?} r {} crosses the block line",
        g.sorting_sphere.radius
    );
}

/// Behaviour: world.scenery.a-slope-aligned-piece-faces-downhill-and-stays-upright
#[test]
fn a_slope_aligned_piece_faces_downhill_and_stays_upright() {
    let s = store();
    let (mesh, placed) = kept(&s, 0x80, 0x5E);
    // Two of the block's slope-aligned pieces: one on a slope, one on level ground.
    let on_slope = find(&placed, 0x0200_05C9, 137.52, 140.28).expect("the piece on the slope");
    let level = find(&placed, 0x0200_05C9, 41.52, 164.28).expect("the piece on level ground");
    for p in [on_slope, level] {
        let up = localtoglobalvec(l2g(p.frame.rotation), Vec3::new(0.0, 0.0, 1.0));
        assert!(
            (up.z - 1.0).abs() < 1e-4,
            "the piece at {:?} leans: up is {up:?}",
            p.frame.origin
        );
    }
    // The slope's downhill direction is the piece's forward direction.
    let plane = dereth_terrain::scenery::find_terrain_poly(
        &mesh,
        on_slope.cell.index(),
        on_slope.frame.origin.x,
        on_slope.frame.origin.y,
    )
    .expect("the piece stands on a terrain polygon");
    assert!(
        plane.normal.z < 0.999,
        "the ground there is level: {:?}",
        plane.normal
    );
    let mut downhill = Vec3::new(-plane.normal.x, -plane.normal.y, 0.0);
    assert!(!downhill.normalize_check_small());
    let fwd = get_vector_heading(&on_slope.frame);
    assert!(
        fwd.dot(downhill) > 0.9999 && fwd.z.abs() < 1e-4,
        "forward {fwd:?}, downhill {downhill:?}"
    );
    // Level ground has no downhill direction: the piece faces north.
    let fwd = get_vector_heading(&level.frame);
    assert!(fwd.y > 0.9999, "the level piece faces {fwd:?}");
}
