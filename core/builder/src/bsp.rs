//! The trees a new model or dungeon piece needs, built from its polygons: the physics tree the
//! client and the server collide with, the drawing tree, and an indoor cell's tree.
//!
//! What the client asks of them, and what retail's own trees are like:
//!
//! - A plane is `N·p + d`, its positive side `> 0`. A sphere walk visits both children of a node
//!   whose plane it straddles, and skips a node whose sphere it misses, so **every node's sphere
//!   bounds its subtree's polygons**; the root's spheres are also the object's physics and drawing
//!   spheres.
//! - **Physics:** every interior node has both children (`BPnN`) and polygons live only in leaves.
//!   A leaf lies behind its polygons: its polygons' planes bound the region it is. A model's leaf
//!   with polygons is solid, an empty one is open space; an environment's leaves are never solid.
//!   A leaf's polygons go in ascending order, since the first hit ends a sweep.
//! - **Drawing:** a node lists the polygons on its plane, and every polygon is listed once. The
//!   retail client keeps only a model's portal nodes when it loads one and draws the rest from the
//!   polygon list, so the tree's order matters only to a painter that walks it.
//! - **Cell:** a chain of positive-only nodes (`BPnn`) ending in one `BPFL`, every plane facing into
//!   the cell; a point in front of all of them is in it.
//!
//! The physics tree is built the solid-leaf way: a polygon's plane splits the set; the polygons on
//! it facing the same way go behind it, and that plane is never used again below; one that crosses
//! it goes to both sides, so no sweep can miss it. When no plane is left, the polygons that
//! remain form a leaf. Each split therefore uses up at least one plane, and the build ends.

use dereth_assets::geometry::CellStruct;
use dereth_assets::{BspNode, BspTree, Plane, Polygon, Sphere, VertexArray};
use dereth_primitives::Vec3;

type V = [f64; 3];

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn v(p: Vec3) -> V {
    [f64::from(p.x), f64::from(p.y), f64::from(p.z)]
}

#[allow(clippy::cast_possible_truncation)]
fn f(p: V) -> Vec3 {
    Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32)
}

/// How far from a plane a point still counts as on it, in meters.
pub const ON_PLANE: f64 = 0.001;

/// The tags: both children, positive only, negative only, a leaf, and a cell chain's end.
const BPNN: u32 = 0x4250_6E4E;
const BPNN_POS: u32 = 0x4250_6E6E;
const BPIN: u32 = 0x4250_494E;
const BPIN_POS: u32 = 0x4250_496E;
const BPIN_NEG: u32 = 0x4270_494E;
const BPOL: u32 = 0x4250_4F4C;
const LEAF: u32 = 0x4C45_4146;
const BPFL: u32 = 0x4250_464C;

/// A polygon as the builder sees it: its corners and its plane, `n·p + d = 0`, facing the way its
/// winding does (counter-clockwise seen from the front).
#[derive(Clone, Debug)]
struct Face {
    corners: Vec<V>,
    n: V,
    d: f64,
}

fn face(va: &VertexArray, p: &Polygon) -> Option<Face> {
    let corners: Vec<V> = p
        .vertex_ids
        .iter()
        .filter_map(|i| va.vertices.get(usize::from(*i)))
        .map(|x| v(x.position))
        .collect();
    if corners.len() < 3 {
        return None;
    }
    let mut n = [0.0; 3];
    for i in 0..corners.len() {
        let (a, b) = (corners[i], corners[(i + 1) % corners.len()]);
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let len = dot(n, n).sqrt();
    if len < 1e-12 {
        return None;
    }
    let n = [n[0] / len, n[1] / len, n[2] / len];
    #[allow(clippy::cast_precision_loss)]
    let k = corners.len() as f64;
    let c = corners.iter().fold([0.0; 3], |s, p| {
        [s[0] + p[0] / k, s[1] + p[1] / k, s[2] + p[2] / k]
    });
    Some(Face {
        corners,
        n,
        d: -dot(n, c),
    })
}

/// Where a polygon lies against a plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Front,
    Back,
    /// On it, facing the same way.
    OnSame,
    /// On it, facing the other way.
    OnOpposite,
    Across,
}

/// The smallest sphere the builder finds around `points`: the middle of their box, and the
/// farthest of them from it.
fn bounding(points: &[V]) -> Sphere {
    if points.is_empty() {
        return Sphere::default();
    }
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    for p in points {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let c = [
        (lo[0] + hi[0]) / 2.0,
        (lo[1] + hi[1]) / 2.0,
        (lo[2] + hi[2]) / 2.0,
    ];
    let r = points
        .iter()
        .map(|p| {
            let d = sub(*p, c);
            dot(d, d).sqrt()
        })
        .fold(0.0, f64::max);
    #[allow(clippy::cast_possible_truncation)]
    Sphere {
        center: f(c),
        radius: (r + 1e-4) as f32,
    }
}

fn corners_of(faces: &[Option<Face>], set: &[usize]) -> Vec<V> {
    set.iter()
        .filter_map(|i| faces[*i].as_ref())
        .flat_map(|fc| fc.corners.iter().copied())
        .collect::<Vec<_>>()
}

/// A polygon as a tree names it: by its place in the list.
fn index(i: usize) -> i16 {
    i16::try_from(i).unwrap_or(i16::MAX)
}

fn node_index(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

#[allow(clippy::cast_possible_truncation)]
fn plane(n: V, d: f64) -> Plane {
    Plane {
        normal: f(n),
        d: d as f32,
    }
}

/// Where the corners of a polygon, or a piece of one, lie against a plane, a corner within `near`
/// of it counting as on it.
fn side_of(corners: &[V], own: V, n: V, d: f64, near: f64) -> Side {
    let (mut front, mut back) = (false, false);
    for c in corners {
        let s = dot(n, *c) + d;
        if s > near {
            front = true;
        } else if s < -near {
            back = true;
        }
    }
    match (front, back) {
        (true, true) => Side::Across,
        (true, false) => Side::Front,
        (false, true) => Side::Back,
        (false, false) if dot(own, n) > 0.0 => Side::OnSame,
        (false, false) => Side::OnOpposite,
    }
}

/// Where a polygon, or the piece of it `corners` is, lies against the plane of `by`: on it when
/// its own plane is that plane, even if its corners stray from it (a polygon that is not quite
/// flat).
/// The lowest score a plane being scored can still end with (`across * 8 + |lean|`): with
/// `across` polygons across it so far, `lean` more in front than behind, and `left` still to
/// place. `across` only grows, and each polygon left moves the lean by one at most.
fn lowest(across: i64, lean: i64, left: i64) -> i64 {
    across * 8 + (lean.abs() - left).max(0)
}

/// A plane's place in the choice: its score, then its place among the planes (`k`), so the lowest
/// key is the lowest score and, of scores alike, the first plane.
fn key(score: i64, k: usize) -> u64 {
    (u64::try_from(score).unwrap_or(u64::MAX >> 32) << 32) | u64::try_from(k).unwrap_or(u64::MAX)
}

/// How many polygons a node needs before its planes are scored on every core.
const SPLIT_ON_CORES: usize = 256;

/// The best of `n` planes, its place and score: the one `score` scores lowest, the first of those
/// alike, as scoring them one after another in order finds it. `score(k, beat)` scores plane `k`,
/// or gives up on it (`None`) once its [`key`] can only be `beat` or more; `beat` only ever falls,
/// so a plane given up on is never the best. A node of `size` polygons or more is scored on every
/// core (where it has more than one), which chooses the same plane.
fn best_split(
    n: usize,
    size: usize,
    score: impl Fn(usize, u64) -> Option<i64> + Sync,
) -> Option<(usize, i64)> {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    let best = AtomicU64::new(u64::MAX);
    let one = |k: usize| {
        if let Some(s) = score(k, best.load(Ordering::Relaxed)) {
            best.fetch_min(key(s, k), Ordering::Relaxed);
        }
    };
    // On the shared pool of workers: a node's own threads, made and joined at every big node, cost
    // more than its scoring where making a thread is slow (Windows).
    if size < SPLIT_ON_CORES || n < 2 {
        (0..n).for_each(one);
    } else {
        (0..n).into_par_iter().for_each(one);
    }
    let b = best.into_inner();
    (b != u64::MAX).then(|| {
        let k = usize::try_from(b & 0xFFFF_FFFF).unwrap_or(usize::MAX);
        let s = i64::try_from(b >> 32).unwrap_or(i64::MAX);
        (k, s)
    })
}

/// A count of polygons as the scores count them.
fn count(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

fn place(fc: &Face, corners: &[V], by: &Face) -> Side {
    let (n, d) = (by.n, by.d);
    let along = dot(fc.n, n);
    if along > 1.0 - 1e-6 && (fc.d - d).abs() < ON_PLANE {
        Side::OnSame
    } else if along < -(1.0 - 1e-6) && (fc.d + d).abs() < ON_PLANE {
        Side::OnOpposite
    } else {
        side_of(corners, fc.n, n, d, ON_PLANE)
    }
}

/// The part of a convex polygon in front of a plane (`keep_front`) or behind it.
fn clip(corners: &[V], n: V, d: f64, keep_front: bool) -> Vec<V> {
    let sign = if keep_front { 1.0 } else { -1.0 };
    let mut out = Vec::with_capacity(corners.len() + 1);
    for i in 0..corners.len() {
        let (a, b) = (corners[i], corners[(i + 1) % corners.len()]);
        let (sa, sb) = (sign * (dot(n, a) + d), sign * (dot(n, b) + d));
        if sa >= 0.0 {
            out.push(a);
        }
        if (sa >= 0.0) != (sb >= 0.0) {
            let t = sa / (sa - sb);
            out.push([
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ]);
        }
    }
    out
}

/// How far past a splitting plane a polygon may reach and still count as a sliver there.
pub const SLIVER: f64 = 0.01;

/// A polygon on its way down the tree: its place in the list, and the piece of it inside the
/// region the node it has reached stands for.
#[derive(Clone, Debug)]
struct Piece {
    polygon: usize,
    corners: Vec<V>,
    /// A sliver: the piece reaches no more than [`SLIVER`] past a plane the rest of the polygon
    /// is on the other side of. A sliver is named by every leaf it reaches, so no sweep misses
    /// it, but it splits nothing and makes no leaf solid: a polygon that is not quite flat, or
    /// two neighbors not quite in one plane, would otherwise fill a region outside the model.
    sliver: bool,
}

/// A node to fill, the pieces of polygons in the region it stands for, and the planes split by on
/// its way down.
type Work = (usize, Vec<Piece>, Vec<(V, f64)>);

/// How far `corners` reach in front of a plane (behind it, when `front` is false).
fn reach(corners: &[V], n: V, d: f64, front: bool) -> f64 {
    let sign = if front { 1.0 } else { -1.0 };
    corners
        .iter()
        .map(|c| sign * (dot(n, *c) + d))
        .fold(f64::MIN, f64::max)
}

/// The physics tree of `polygons` (whose vertices `va` holds): a leaf with polygons is solid when
/// `solid` (a model's), never otherwise (a piece's). A node names its polygons by their places in
/// the list, as the client reads them; a record's polygon ids should be the same numbers.
///
/// A polygon crossing a splitting plane goes to both sides, and each side goes on with the part of
/// it there, so a polygon reaches only the leaves its surface touches; the leaf still names the
/// whole polygon, which is what the client tests.
#[must_use]
pub fn physics(va: &VertexArray, polygons: &[Polygon], solid: bool) -> BspTree {
    let faces: Vec<Option<Face>> = polygons.iter().map(|p| face(va, p)).collect();
    let all: Vec<Piece> = (0..polygons.len())
        .filter_map(|i| {
            faces[i].as_ref().map(|fc| Piece {
                polygon: i,
                corners: fc.corners.clone(),
                sliver: false,
            })
        })
        .collect();
    let mut tree = BspTree::default();
    // The work: the node to fill, its polygons, and the planes used on its path.
    let mut work: Vec<Work> = Vec::new();
    tree.nodes.push(BspNode::default());
    work.push((0, all, Vec::new()));
    while let Some((at, set, used)) = work.pop() {
        let whole: Vec<usize> = set.iter().map(|p| p.polygon).collect();
        let sphere = bounding(&corners_of(&faces, &whole));
        let face_of = |i: usize| faces[i].as_ref().expect("a face");
        let used_plane = |n: V, d: f64| {
            used.iter()
                .any(|(m, e)| dot(*m, n) > 1.0 - 1e-6 && (e - d).abs() < ON_PLANE)
        };
        // The plane that splits best: fewest polygons across it, then the most even split; of
        // planes alike, the first. A plane one on the path or an earlier one here has is tried once.
        let mut tried: Vec<(V, f64)> = Vec::new();
        let mut planes: Vec<usize> = Vec::new();
        for c in set.iter().filter(|p| !p.sliver) {
            let cf = face_of(c.polygon);
            if used_plane(cf.n, cf.d)
                || tried
                    .iter()
                    .any(|(m, e)| dot(*m, cf.n) > 1.0 - 1e-6 && (e - cf.d).abs() < ON_PLANE)
            {
                continue;
            }
            tried.push((cf.n, cf.d));
            planes.push(c.polygon);
        }
        let best = best_split(planes.len(), set.len(), |k, beat| {
            let cf = face_of(planes[k]);
            let (mut front, mut back, mut across) = (0i64, 0i64, 0i64);
            let mut left = count(set.len());
            for p in &set {
                match place(face_of(p.polygon), &p.corners, cf) {
                    Side::Front | Side::OnOpposite => front += 1,
                    Side::Back | Side::OnSame => back += 1,
                    Side::Across => across += 1,
                }
                left -= 1;
                if key(lowest(across, front - back, left), k) >= beat {
                    return None;
                }
            }
            Some(across * 8 + (front - back).abs())
        })
        .map(|(k, score)| (planes[k], score));
        let Some((splitter, _)) = best else {
            // Every polygon here has split by its own plane: they bound this leaf from behind.
            let mut polys: Vec<i16> = whole.iter().map(|i| index(*i)).collect();
            polys.sort_unstable();
            polys.dedup();
            tree.nodes[at] = BspNode {
                tag: LEAF,
                leaf_index: Some(0),
                solid: Some(i32::from(solid && set.iter().any(|p| !p.sliver))),
                sphere: Some(sphere),
                in_polys: polys,
                ..BspNode::default()
            };
            continue;
        };
        let sf = face_of(splitter).clone();
        let (mut pos, mut neg) = (Vec::new(), Vec::new());
        for p in set {
            match place(face_of(p.polygon), &p.corners, &sf) {
                Side::Front | Side::OnOpposite => pos.push(p),
                Side::Back | Side::OnSame => neg.push(p),
                Side::Across => {
                    // The smaller reach of the two is a sliver, if it is small enough.
                    let (ahead, behind) = (
                        reach(&p.corners, sf.n, sf.d, true),
                        reach(&p.corners, sf.n, sf.d, false),
                    );
                    for (front, part, out) in [(true, ahead, &mut pos), (false, behind, &mut neg)] {
                        let other = if front { behind } else { ahead };
                        let corners = clip(&p.corners, sf.n, sf.d, front);
                        if corners.len() >= 3 {
                            out.push(Piece {
                                polygon: p.polygon,
                                corners,
                                sliver: p.sliver || (part <= SLIVER && part < other),
                            });
                        }
                    }
                }
            }
        }
        let mut used_below = used.clone();
        used_below.push((sf.n, sf.d));
        let p = tree.nodes.len();
        tree.nodes.push(BspNode::default());
        let n = tree.nodes.len();
        tree.nodes.push(BspNode::default());
        tree.nodes[at] = BspNode {
            tag: BPNN,
            plane: Some(plane(sf.n, sf.d)),
            pos_child: Some(node_index(p)),
            neg_child: Some(node_index(n)),
            sphere: Some(sphere),
            ..BspNode::default()
        };
        work.push((n, neg, used_below.clone()));
        work.push((p, pos, used_below));
    }
    renumber(tree)
}

/// The tree with its nodes in the order a record writes them: depth first, positive child first.
fn renumber(tree: BspTree) -> BspTree {
    if tree.nodes.is_empty() {
        return tree;
    }
    let mut order = Vec::with_capacity(tree.nodes.len());
    let mut stack = vec![0usize];
    while let Some(i) = stack.pop() {
        order.push(i);
        let n = &tree.nodes[i];
        if let Some(c) = n.neg_child {
            stack.push(c as usize);
        }
        if let Some(c) = n.pos_child {
            stack.push(c as usize);
        }
    }
    let mut new_of = vec![0u32; tree.nodes.len()];
    for (new, old) in order.iter().enumerate() {
        new_of[*old] = node_index(new);
    }
    let mut leaves = 0u32;
    let nodes = order
        .iter()
        .map(|old| {
            let mut n = tree.nodes[*old].clone();
            n.pos_child = n.pos_child.map(|c| new_of[c as usize]);
            n.neg_child = n.neg_child.map(|c| new_of[c as usize]);
            if n.leaf_index.is_some() {
                n.leaf_index = Some(leaves);
                leaves += 1;
            }
            n
        })
        .collect();
    BspTree { nodes }
}

/// The drawing tree of `polygons`: each node lists the polygons on its plane (facing either
/// way); the others go in front or behind it, one that crosses it by the side its middle is on.
#[must_use]
pub fn drawing(va: &VertexArray, polygons: &[Polygon]) -> BspTree {
    let faces: Vec<Option<Face>> = polygons.iter().map(|p| face(va, p)).collect();
    let all: Vec<usize> = (0..polygons.len())
        .filter(|i| faces[*i].is_some())
        .collect();
    let mut tree = BspTree::default();
    if all.is_empty() {
        tree.nodes.push(BspNode {
            tag: LEAF,
            leaf_index: Some(0),
            ..BspNode::default()
        });
        return tree;
    }
    tree.nodes.push(BspNode::default());
    let mut work = vec![(0usize, all)];
    while let Some((at, set)) = work.pop() {
        let sphere = bounding(&corners_of(&faces, &set));
        // The plane with the fewest crossings, then the most even split; of planes alike, the
        // first.
        let best = best_split(set.len(), set.len(), |k, beat| {
            let cf = faces[set[k]].as_ref().expect("a face");
            let (mut front, mut back, mut across) = (0i64, 0i64, 0i64);
            let mut left = count(set.len());
            for &i in &set {
                let fc = faces[i].as_ref().expect("a face");
                match place(fc, &fc.corners, cf) {
                    Side::Front => front += 1,
                    Side::Back => back += 1,
                    Side::Across => across += 1,
                    _ => {}
                }
                left -= 1;
                if key(lowest(across, front - back, left), k) >= beat {
                    return None;
                }
            }
            Some(across * 8 + (front - back).abs())
        });
        let (splitter, _) = best.map(|(k, s)| (set[k], s)).expect("a polygon");
        let sf = faces[splitter].as_ref().expect("a face").clone();
        let (mut on, mut pos, mut neg) = (Vec::new(), Vec::new(), Vec::new());
        for &i in &set {
            let fc = faces[i].as_ref().expect("a face");
            match place(fc, &fc.corners, &sf) {
                Side::OnSame | Side::OnOpposite => on.push(index(i)),
                Side::Front => pos.push(i),
                Side::Back => neg.push(i),
                Side::Across => {
                    #[allow(clippy::cast_precision_loss)]
                    let k = fc.corners.len() as f64;
                    let m = fc.corners.iter().fold([0.0; 3], |s, p| {
                        [s[0] + p[0] / k, s[1] + p[1] / k, s[2] + p[2] / k]
                    });
                    if dot(sf.n, m) + sf.d >= 0.0 {
                        pos.push(i);
                    } else {
                        neg.push(i);
                    }
                }
            }
        }
        let mut child = |set: Vec<usize>, tree: &mut BspTree| -> Option<u32> {
            if set.is_empty() {
                return None;
            }
            let c = tree.nodes.len();
            tree.nodes.push(BspNode::default());
            work.push((c, set));
            Some(node_index(c))
        };
        let pos_child = child(pos, &mut tree);
        let neg_child = child(neg, &mut tree);
        let tag = match (pos_child.is_some(), neg_child.is_some()) {
            (true, true) => BPIN,
            (true, false) => BPIN_POS,
            (false, true) => BPIN_NEG,
            (false, false) => BPOL,
        };
        tree.nodes[at] = BspNode {
            tag,
            plane: Some(plane(sf.n, sf.d)),
            pos_child,
            neg_child,
            sphere: Some(sphere),
            in_polys: on,
            ..BspNode::default()
        };
    }
    renumber(tree)
}

/// The cell tree of a piece whose vertices `va` holds and whose drawing polygons are `polygons`:
/// the faces of the smallest convex shape holding every vertex, facing in, as a chain. A
/// polygon's own plane is taken for a face it lies on.
#[must_use]
pub fn cell(va: &VertexArray, polygons: &[Polygon]) -> BspTree {
    let mut points: Vec<V> = Vec::new();
    for x in &va.vertices {
        let q = v(x.position);
        if !points.iter().any(|o| dot(sub(*o, q), sub(*o, q)) < 1e-12) {
            points.push(q);
        }
    }
    let supports = |n: V, d: f64, near: f64| points.iter().all(|q| dot(n, *q) + d >= -near);
    // The faces, each with how far from it a corner still counts as on it. One plane stands for
    // each face: a candidate facing within a degree of a face already found, its corners within
    // [`SLIVER`] of it, is that face again (two polygons not quite in one plane, say).
    let mut planes: Vec<(V, f64, f64)> = Vec::new();
    let same_face = |planes: &[(V, f64, f64)], n: V, corners: &[V]| {
        planes.iter().any(|(m, e, _)| {
            dot(*m, n) > 1.0 - 1.5e-4 && corners.iter().all(|q| (dot(*m, *q) + e).abs() <= SLIVER)
        })
    };
    // The biggest polygons first, so a face's plane is its main polygon's, not a strip's beside it.
    let mut faces: Vec<(f64, Face)> = polygons
        .iter()
        .filter_map(|p| face(va, p))
        .map(|fc| {
            let mut twice = [0.0; 3];
            for i in 1..fc.corners.len().saturating_sub(1) {
                let (e1, e2) = (
                    sub(fc.corners[i], fc.corners[0]),
                    sub(fc.corners[i + 1], fc.corners[0]),
                );
                twice[0] += e1[1] * e2[2] - e1[2] * e2[1];
                twice[1] += e1[2] * e2[0] - e1[0] * e2[2];
                twice[2] += e1[0] * e2[1] - e1[1] * e2[0];
            }
            (dot(twice, twice).sqrt() / 2.0, fc)
        })
        .collect();
    faces.sort_by(|a, b| b.0.total_cmp(&a.0));
    // Flat polygons first, then those not quite flat, which keep their own plane for their face
    // with their corners as far off it as they are.
    for bent in [false, true] {
        for (_, fc) in &faces {
            let bend = fc
                .corners
                .iter()
                .map(|c| (dot(fc.n, *c) + fc.d).abs())
                .fold(0.0, f64::max);
            if (bend > ON_PLANE) != bent {
                continue;
            }
            let near = ON_PLANE + bend;
            for (n, d) in [(fc.n, fc.d), ([-fc.n[0], -fc.n[1], -fc.n[2]], -fc.d)] {
                if supports(n, d, near) && !same_face(&planes, n, &fc.corners) {
                    planes.push((n, d, near));
                }
            }
        }
    }
    // The faces no polygon lies on: every plane through three corners with all of them in front,
    // unless the three are on a face already found.
    for i in 0..points.len() {
        for j in i + 1..points.len() {
            for k in j + 1..points.len() {
                let (a, b, c) = (points[i], points[j], points[k]);
                if planes
                    .iter()
                    .any(|(n, d, near)| [a, b, c].iter().all(|q| (dot(*n, *q) + d).abs() <= *near))
                {
                    continue;
                }
                let (e1, e2) = (sub(b, a), sub(c, a));
                let n = [
                    e1[1] * e2[2] - e1[2] * e2[1],
                    e1[2] * e2[0] - e1[0] * e2[2],
                    e1[0] * e2[1] - e1[1] * e2[0],
                ];
                // Three corners nearly in a line give no face, only an edge.
                let len = dot(n, n).sqrt();
                if len < 1e-3 * (dot(e1, e1) * dot(e2, e2)).sqrt() {
                    continue;
                }
                let n = [n[0] / len, n[1] / len, n[2] / len];
                let d = -dot(n, a);
                for (n, d) in [(n, d), ([-n[0], -n[1], -n[2]], -d)] {
                    if supports(n, d, ON_PLANE) && !same_face(&planes, n, &[a, b, c]) {
                        planes.push((n, d, ON_PLANE));
                    }
                }
            }
        }
    }
    let mut tree = BspTree::default();
    let last = planes.len().saturating_sub(1);
    for (i, (n, d, _)) in planes.iter().enumerate() {
        tree.nodes.push(BspNode {
            tag: if i == last { BPFL } else { BPNN_POS },
            plane: Some(plane(*n, *d)),
            pos_child: (i != last).then(|| node_index(i + 1)),
            ..BspNode::default()
        });
    }
    tree
}

/// A cell shape from its vertices, its polygons (drawn ones and its openings), which of those are
/// its openings, the polygons it collides with and its cell tree (the convex shape that holds it,
/// [`cell`]): its collision tree and its drawing tree with each opening at the node
/// holding it made, as retail's cells have them.
///
/// # Errors
/// More polygons or openings than a cell can number.
pub fn cell_struct(
    va: VertexArray,
    polys: Vec<Polygon>,
    portals: Vec<u16>,
    physics_polygons: Vec<Polygon>,
    cell_bsp: dereth_assets::BspTree,
) -> Result<CellStruct, String> {
    let physics_bsp = physics(&va, &physics_polygons, false);
    let mut drawing = drawing(&va, &polys);
    for i in 0..drawing.nodes.len() {
        let pairs: Vec<_> = portals
            .iter()
            .enumerate()
            .filter_map(|(slot, poly)| {
                let poly = i16::try_from(*poly).ok()?;
                drawing.nodes[i]
                    .in_polys
                    .contains(&poly)
                    .then_some((poly, i16::try_from(slot).ok()?))
            })
            .collect();
        if pairs.is_empty() {
            continue;
        }
        for (poly, _) in &pairs {
            drawing.nodes[i].in_polys.retain(|p| p != poly);
        }
        drawing.nodes[i].tag = 0x504F_5254;
        drawing.nodes[i].in_portals = pairs;
        for positive in [true, false] {
            let missing = if positive {
                drawing.nodes[i].pos_child.is_none()
            } else {
                drawing.nodes[i].neg_child.is_none()
            };
            if missing {
                let index = u32::try_from(drawing.nodes.len()).map_err(|e| e.to_string())?;
                drawing.nodes.push(BspNode {
                    tag: 0x4C45_4146,
                    leaf_index: Some(0),
                    ..BspNode::default()
                });
                if positive {
                    drawing.nodes[i].pos_child = Some(index);
                } else {
                    drawing.nodes[i].neg_child = Some(index);
                }
            }
        }
    }
    Ok(CellStruct {
        cellstruct_id: 0,
        vertex_array: va,
        polygons: polys,
        portals,
        cell_bsp,
        physics_polygons,
        physics_bsp,
        drawing_bsp: Some(drawing),
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the model builder's choice of splitting plane)
    use super::*;

    /// The best plane is the lowest score and, of scores alike, the first, as scoring every plane
    /// in order finds it: on one core or on all of them, and with planes given up on once they
    /// can no longer win.
    #[test]
    fn the_best_split_is_the_first_lowest_score_on_any_number_of_cores() {
        // Scores with many alike, from a fixed sequence.
        let scores: Vec<i64> = (0..2000u64)
            .map(|k| i64::try_from((k * 7919 + 13) % 37).unwrap())
            .collect();
        let first_lowest = {
            let low = *scores.iter().min().unwrap();
            (scores.iter().position(|&s| s == low).unwrap(), low)
        };
        let score = |k: usize, beat: u64| (key(scores[k], k) < beat).then_some(scores[k]);
        // Few polygons: in order, on one core; many: on every core.
        assert_eq!(best_split(scores.len(), 10, score), Some(first_lowest));
        assert_eq!(
            best_split(scores.len(), SPLIT_ON_CORES, score),
            Some(first_lowest)
        );
        assert_eq!(best_split(0, SPLIT_ON_CORES, score), None);
        // A plane's lowest score left bounds what it can still end with.
        assert_eq!(lowest(2, -5, 3), 16 + 2);
        assert_eq!(lowest(2, 1, 3), 16);
    }
}
