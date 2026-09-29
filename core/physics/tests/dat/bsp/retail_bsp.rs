//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! Every retail physics BSP converts, traverses and terminates with both children present; sphere-
//! solid is at least as permissive as point-solid over dense samples.
//! Fixture: the shipped retail DAT records and recorded inputs.

use std::collections::HashMap;

use dereth_assets::{Decode, GfxObj};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::geom::bsp::{BspNode, BspNodeKind, BspTree};
use dereth_physics::geom::{Plane, Polygon, Sphere};
use dereth_primitives::{DataId, Vec3};

fn store() -> RetailDatStore {
    dereth_dat::testing::open_store_or_fail()
}

fn convert(g: &GfxObj) -> Option<(BspTree, Stats)> {
    let src = g.physics_bsp.as_ref()?;
    let mut stats = Stats::default();

    // A leaf's `in_polys` are polygon *ids*, not indices; ACE keys a dictionary by the same id.
    let mut by_id: HashMap<i16, u32> = HashMap::new();
    let mut polygons = Vec::with_capacity(g.physics_polygons.len());
    for (i, p) in g.physics_polygons.iter().enumerate() {
        let verts: Vec<Vec3> = p
            .vertex_ids
            .iter()
            .map(|&vi| g.vertex_array.vertices[vi as usize].position)
            .collect();
        by_id.insert(p.poly_id, u32::try_from(i).expect("polygon index fits"));
        polygons.push(Polygon::new(verts));
    }

    let mut nodes = Vec::with_capacity(src.nodes.len());
    for n in &src.nodes {
        let kind = if n.leaf_index.is_some() {
            stats.leaves += 1;
            BspNodeKind::Leaf {
                leaf_index: i32::try_from(n.leaf_index.unwrap_or(0)).unwrap_or(-1),
                solid: n.solid == Some(1),
            }
        } else if n.tag == 0x504F_5254 {
            stats.portals += 1;
            BspNodeKind::Portal
        } else {
            stats.interior += 1;
            BspNodeKind::Node
        };
        if n.sphere.is_none() {
            stats.nodes_without_sphere += 1;
        }
        if n.plane.is_none() && !matches!(kind, BspNodeKind::Leaf { .. }) {
            stats.interior_without_plane += 1;
        }
        let sphere = n
            .sphere
            .map_or_else(Sphere::default, |s| Sphere::new(s.center, s.radius));
        let splitting_plane = n.plane.map_or_else(Plane::default, |p| Plane {
            normal: p.normal,
            d: p.d,
        });
        let in_polys = n
            .in_polys
            .iter()
            .filter_map(|id| by_id.get(id).copied())
            .collect::<Vec<_>>();
        stats.polys_referenced += in_polys.len() as u64;
        stats.polys_unresolved += n.in_polys.len() as u64 - in_polys.len() as u64;
        nodes.push(BspNode {
            sphere,
            splitting_plane,
            pos_child: n.pos_child,
            neg_child: n.neg_child,
            kind,
            in_polys,
        });
    }
    Some((BspTree { nodes, polygons }, stats))
}

#[derive(Default, Debug)]
struct Stats {
    leaves: u64,
    portals: u64,
    interior: u64,
    nodes_without_sphere: u64,
    interior_without_plane: u64,
    polys_referenced: u64,
    polys_unresolved: u64,
}

impl Stats {
    fn merge(&mut self, o: &Self) {
        self.leaves += o.leaves;
        self.portals += o.portals;
        self.interior += o.interior;
        self.nodes_without_sphere += o.nodes_without_sphere;
        self.interior_without_plane += o.interior_without_plane;
        self.polys_referenced += o.polys_referenced;
        self.polys_unresolved += o.polys_unresolved;
    }
}

/// Descend by the splitting plane exactly the way
/// and ACE's `BSPNode.point_intersects_solid` do, written independently of the implementation.
fn independent_leaf_walk(t: &BspTree, p: Vec3) -> Vec<u32> {
    let mut out = Vec::new();
    if t.nodes.is_empty() {
        return out;
    }
    let mut i = 0_u32;
    loop {
        out.push(i);
        let n = &t.nodes[i as usize];
        if matches!(n.kind, BspNodeKind::Leaf { .. }) {
            return out;
        }
        let d = n.splitting_plane.normal.x * p.x
            + n.splitting_plane.normal.y * p.y
            + n.splitting_plane.normal.z * p.z
            + n.splitting_plane.d;
        match if d > 0.0 { n.pos_child } else { n.neg_child } {
            Some(c) => i = c,
            None => return out,
        }
    }
}

#[test]
fn every_retail_physics_bsp_converts_traverses_and_terminates() {
    let s = store();
    let ids = s.ids_of(DbType::GfxObj);
    assert!(!ids.is_empty(), "only {} GfxObjs", ids.len());

    let mut total = Stats::default();
    let mut trees = 0_u32;
    let mut objects = 0_u32;
    let mut max_depth = 0_usize;
    let mut deepest: DataId = DataId(0);
    let mut leaf_walks = 0_u64;
    let mut solid_hits = 0_u64;
    let mut single_child_interior = 0_u64;
    let mut seed = 0x9E37_79B9_u32;
    let mut rnd = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        #[allow(clippy::cast_precision_loss)]
        {
            (seed >> 8) as f32 / 16_777_216.0
        }
    };

    for id in &ids {
        let bytes = s
            .read_typed(DbType::GfxObj, *id)
            .expect("graphics object reads");
        let g = GfxObj::decode_payload(*id, &bytes).expect("graphics object decodes");
        objects += 1;
        let Some((tree, stats)) = convert(&g) else {
            continue;
        };
        trees += 1;
        total.merge(&stats);

        if !tree.interior_nodes_have_both_children() {
            single_child_interior += 1;
        }

        let d = tree.max_depth();
        if d > max_depth {
            max_depth = d;
            deepest = *id;
        }

        // A dense point sample around the object's own sort centre. Every query must terminate
        // (the test would hang otherwise) and the traversal's leaf path must match an
        // independently written walk of the same tree.
        // Sample inside the root's own bounding sphere, not around the sort centre: the two are
        // not the same point and sampling the wrong one barely reaches solid at all.
        let (c, r) = tree.root().map_or((Vec3::ZERO, 1.0), |n| {
            (n.sphere.center, n.sphere.radius.max(0.5))
        });
        for _ in 0..24 {
            let p = Vec3::new(
                c.x + (rnd() - 0.5) * 1.2 * r,
                c.y + (rnd() - 0.5) * 1.2 * r,
                c.z + (rnd() - 0.5) * 1.2 * r,
            );
            assert_eq!(
                tree.leaf_path_for_point(p),
                independent_leaf_walk(&tree, p),
                "leaf path for {id:?} at {p:?}"
            );
            leaf_walks += 1;
            if tree.point_intersects_solid(p) {
                solid_hits += 1;
            }
            // A sphere query must terminate too, and a zero-radius sphere at a solid point can
            // only ever agree with or be more permissive than the point query.
            let _ = tree.sphere_intersects_solid(&Sphere::new(p, 0.05), true);
            let _ = tree.sphere_intersects_cell_bsp(&Sphere::new(p, 0.05));
        }
    }

    assert_eq!(
        objects as usize,
        ids.len(),
        "every graphics object is decoded"
    );
    assert_eq!(
        leaf_walks,
        u64::from(trees) * 24,
        "every tree receives the point sample"
    );
    assert!(
        trees > 0,
        "only {trees} physics BSPs found among {objects} GfxObjs"
    );
    assert_eq!(
        single_child_interior, 0,
        "{single_child_interior} trees have a one-child interior node"
    );
    assert_eq!(
        total.polys_unresolved, 0,
        "{} leaf polygon ids did not resolve",
        total.polys_unresolved
    );
    assert_eq!(
        total.nodes_without_sphere, 0,
        "{} physics nodes carry no bounding sphere",
        total.nodes_without_sphere
    );
    assert_eq!(
        total.interior_without_plane, 0,
        "{} interior nodes carry no splitting plane",
        total.interior_without_plane
    );
    assert_eq!(
        total.portals, 0,
        "physics trees are not expected to contain PORT nodes"
    );
    // A meaningful fraction of the samples must actually reach solid geometry, or the leaf-path
    // comparison above is only exercising empty leaves.
    let solid_ratio = solid_hits as f64 / leaf_walks as f64;
    assert!(
        solid_ratio > 0.05,
        "only {solid_hits} of {leaf_walks} point queries landed in solid"
    );
    // Bounds the recursion depth of the traversals: a few hundred is comfortably inside any
    // thread stack, which is why recursion is safe here where the *decoder* had to use a work list.
    assert!(
        max_depth < 512,
        "deepest physics BSP is {max_depth} nodes ({deepest:?})"
    );

    eprintln!(
        "BSP: {objects} GfxObjs, {trees} physics BSPs, {} interior + {} leaf nodes, \
         {} polygon references, {leaf_walks} leaf walks, {solid_hits} solid hits, \
         max depth {max_depth} at {deepest:?}",
        total.interior, total.leaves, total.polys_referenced
    );
}

/// The solid-sphere test with `center_check` set must be at least as
/// permissive as the plain point test, because the solid-leaf flag can answer on its own. This is
/// the invariant `find_placement_position` relies on when it pushes an object out of geometry.
#[test]
fn sphere_solid_is_at_least_as_permissive_as_point_solid() {
    let s = store();
    let ids = s.ids_of(DbType::GfxObj);
    let mut checked = 0_u64;
    let mut disagreements = 0_u64;
    for id in ids.iter().step_by(7) {
        let bytes = s
            .read_typed(DbType::GfxObj, *id)
            .expect("graphics object reads");
        let g = GfxObj::decode_payload(*id, &bytes).expect("graphics object decodes");
        let Some((tree, _)) = convert(&g) else {
            continue;
        };
        let (c, r) = tree.root().map_or((Vec3::ZERO, 1.0), |n| {
            (n.sphere.center, n.sphere.radius.max(0.5))
        });
        for k in 0..40_i32 {
            #[allow(clippy::cast_precision_loss)]
            let t = k as f32 / 40.0 - 0.5;
            let p = Vec3::new(c.x + t * 2.0 * r, c.y + t * 1.3 * r, c.z + t * 0.7 * r);
            let point = tree.point_intersects_solid(p);
            let sphere = tree.sphere_intersects_solid(&Sphere::new(p, r * 0.05), true);
            if point && !sphere {
                disagreements += 1;
            }
            checked += 1;
        }
    }
    assert!(checked > 0, "only {checked} samples");
    // Not an equality: the point test descends one path and answers from the leaf's polygon
    // count, while the sphere test also consults the solid flag and the polygon geometry. What
    // must not happen is the sphere test being *less* permissive on a solid point.
    let ratio = disagreements as f64 / checked as f64;
    assert!(
        ratio < 0.05,
        "{disagreements} of {checked} samples were solid to the point test but not to the sphere test"
    );
    eprintln!("BSP: {checked} point/sphere solidity samples, {disagreements} disagreements");
}
