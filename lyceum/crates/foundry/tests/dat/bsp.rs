//! Behaviour: none (tooling: the tree builder, calibrated on retail's own models and pieces)
//! The builder's trees for every retail model and indoor piece, rebuilt from their own polygons
//! and asked what the client asks of retail's trees, over sampled spheres and points; and a body
//! walked at every model under both trees.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use dereth_assets::{BspTree, Decode, Environment, GfxObj, Polygon, Setup, VertexArray};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::geom::Bounding;
use dereth_physics::globals::FLOOR_Z;
use dereth_physics::{Sphere, V3};
use dereth_primitives::{DataId, Vec3};
use lyceum_foundry::{bsp, fit};

/// How many spheres and points each tree is asked about.
const SAMPLES: usize = 64;

/// How many queries of each kind were asked, and how many came out as the name says.
#[derive(Debug, Default)]
struct Tally {
    trees: usize,
    asked: BTreeMap<&'static str, (usize, usize)>,
}

impl Tally {
    fn add(&mut self, query: &'static str, yes: bool) {
        let e = self.asked.entry(query).or_default();
        e.0 += 1;
        e.1 += usize::from(yes);
    }

    /// How many of the `query` asked came out otherwise.
    fn otherwise(&self, query: &str) -> usize {
        self.asked.get(query).map_or(0, |(n, y)| n - y)
    }

    fn yes(&self, query: &str) -> usize {
        self.asked.get(query).map_or(0, |(_, y)| *y)
    }

    fn print(&self, name: &str) {
        println!("{name}: {} trees", self.trees);
        for (q, (n, y)) in &self.asked {
            println!("  {q:<60} {y:>8} of {n:>8}");
        }
    }
}

/// A small deterministic source of numbers in `[0, 1)`.
struct Draw(u64);

impl Draw {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        #[allow(clippy::cast_precision_loss)]
        let r = (self.0 >> 40) as f32 / (1u64 << 24) as f32;
        r
    }

    /// A point in the box `lo..hi` grown by `grow` each way.
    fn point(&mut self, lo: Vec3, hi: Vec3, grow: f32) -> Vec3 {
        Vec3::new(
            lo.x - grow + (hi.x - lo.x + 2.0 * grow) * self.next(),
            lo.y - grow + (hi.y - lo.y + 2.0 * grow) * self.next(),
            lo.z - grow + (hi.z - lo.z + 2.0 * grow) * self.next(),
        )
    }
}

fn bounds(va: &VertexArray) -> (Vec3, Vec3) {
    let mut lo = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut hi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for v in &va.vertices {
        let p = v.position;
        lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
        hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
    }
    (lo, hi)
}

/// A tree as the client walks it, from a polygon list and a tree in record form.
fn walked(va: &VertexArray, polys: &[Polygon], tree: &BspTree) -> dereth_physics::BspTree {
    let g = GfxObj {
        id: DataId(0x0100_0000),
        flags: 1,
        surfaces: Vec::new(),
        vertex_array: va.clone(),
        physics_polygons: polys.to_vec(),
        physics_bsp: Some(tree.clone()),
        sort_center: Vec3::ZERO,
        polygons: Vec::new(),
        drawing_bsp: None,
        did_degrade: None,
    };
    dereth_world_data::env_cells::gfxobj_physics_bsp(&g).expect("a tree")
}

/// Whether the polygons close a volume: every edge, between corners a millimeter apart, run once
/// each way.
fn closed(va: &VertexArray, polys: &[Polygon]) -> bool {
    #[allow(clippy::cast_possible_truncation)]
    let key = |i: u16| {
        let p = va.vertices[usize::from(i)].position;
        [
            (p.x * 1000.0).round() as i64,
            (p.y * 1000.0).round() as i64,
            (p.z * 1000.0).round() as i64,
        ]
    };
    let mut edges: HashMap<([i64; 3], [i64; 3]), i32> = HashMap::new();
    for p in polys {
        let ids = &p.vertex_ids;
        for k in 0..ids.len() {
            let (a, b) = (key(ids[k]), key(ids[(k + 1) % ids.len()]));
            if a < b {
                *edges.entry((a, b)).or_default() += 1;
            } else if b < a {
                *edges.entry((b, a)).or_default() -= 1;
            }
        }
    }
    edges.values().all(|n| *n == 0)
}

/// How many of `polys` a ray from `p` straight up (leaning a little, to miss edges) crosses.
fn crossings(polys: &[dereth_physics::Polygon], p: Vec3) -> usize {
    let dir = Vec3::new(0.000_31, 0.000_17, 1.0);
    let cross = |a: Vec3, b: Vec3| {
        Vec3::new(
            a.y * b.z - a.z * b.y,
            a.z * b.x - a.x * b.z,
            a.x * b.y - a.y * b.x,
        )
    };
    polys
        .iter()
        .filter(|poly| {
            let v = &poly.vertices;
            (1..v.len().saturating_sub(1)).any(|k| {
                let (a, b, c) = (v[0], v[k], v[k + 1]);
                let (e1, e2) = (b.sub(a), c.sub(a));
                let h = cross(dir, e2);
                let det = e1.dot(h);
                if det.abs() < 1e-9 {
                    return false;
                }
                let sv = p.sub(a);
                let u = sv.dot(h) / det;
                let q = cross(sv, e1);
                let w = dir.dot(q) / det;
                (0.0..=1.0).contains(&u) && w >= 0.0 && u + w <= 1.0 && e2.dot(q) / det > 0.0
            })
        })
        .count()
}

/// Retail's physics tree for `polys` and the builder's, asked the same queries.
fn ask_physics(
    t: &mut Tally,
    va: &VertexArray,
    polys: &[Polygon],
    retail: &BspTree,
    solid: bool,
    seed: u64,
) {
    let ours = bsp::physics(va, polys, solid);
    let (a, b) = (walked(va, polys, retail), walked(va, polys, &ours));
    let (lo, hi) = bounds(va);
    let is_closed = solid && closed(va, polys);
    let mut d = Draw(seed | 1);
    t.trees += 1;
    for _ in 0..SAMPLES {
        let c = d.point(lo, hi, 1.0);
        let s = Sphere::new(c, 0.05 + d.next());
        let m = Vec3::new(d.next() - 0.5, d.next() - 0.5, d.next() - 0.5);
        // A sweep: hit or not, and which polygon. Every polygon tried is the answer no tree may
        // miss; when two or more are hit, each tree names the first its own walk reaches.
        let (mut ha, mut hb) = (None, None);
        let (mut ca, mut cb) = (Vec3::ZERO, Vec3::ZERO);
        let ra = a.sphere_intersects_poly(&s, m, &mut ha, &mut ca);
        let rb = b.sphere_intersects_poly(&s, m, &mut hb, &mut cb);
        let hits = a
            .polygons
            .iter()
            .filter(|p| {
                let mut c = Vec3::ZERO;
                p.pos_hits_sphere_parts(&s, m, &mut c).1
            })
            .count();
        t.add("sweep: retail's is every polygon's", ra == (hits > 0));
        t.add("sweep: ours is every polygon's", rb == (hits > 0));
        if ra && rb {
            let query = if hits == 1 {
                "sweep, one hit: the same polygon"
            } else {
                "sweep, more hit: the same polygon"
            };
            t.add(query, ha == hb);
        }
        // The rest of what collision asks.
        let up = Vec3::new(0.0, 0.0, 1.0);
        t.add(
            "walkable: the same",
            a.hits_walkable(&s, up, FLOOR_Z) == b.hits_walkable(&s, up, FLOOR_Z),
        );
        let touched = a.polygons.iter().any(|p| p.hits_sphere(&s));
        t.add(
            "solid: retail's is every polygon's",
            a.sphere_intersects_solid(&s, false) == touched,
        );
        t.add(
            "solid: ours is every polygon's",
            b.sphere_intersects_solid(&s, false) == touched,
        );
        let (pa, pb) = (
            a.sphere_intersects_solid(&s, true),
            b.sphere_intersects_solid(&s, true),
        );
        if is_closed {
            let inside = crossings(&a.polygons, c) % 2 == 1;
            t.add(
                "closed model, centre check: retail's is inside or touching",
                pa == (inside || touched),
            );
            t.add(
                "closed model, centre check: ours is inside or touching",
                pb == (inside || touched),
            );
        } else {
            t.add("open model or piece, centre check: the same", pa == pb);
        }
    }
}

#[test]
fn the_builders_physics_trees_answer_as_retails_do() {
    let s: RetailDatStore = dereth_dat::testing::open_store_or_fail();
    let mut models = Tally::default();
    for id in s.ids_of(DbType::GfxObj) {
        let g = GfxObj::decode_payload(id, &s.read_typed(DbType::GfxObj, id).unwrap()).unwrap();
        let Some(tree) = &g.physics_bsp else { continue };
        let seed = u64::from(id.raw());
        ask_physics(
            &mut models,
            &g.vertex_array,
            &g.physics_polygons,
            tree,
            true,
            seed,
        );
    }
    let mut pieces = Tally::default();
    for id in s.ids_of(DbType::Environment) {
        let e = Environment::decode_payload(id, &s.read_typed(DbType::Environment, id).unwrap())
            .unwrap();
        for (i, c) in e.cells.iter().enumerate() {
            let seed = u64::from(id.raw()) << 8 | i as u64;
            let (va, polys) = (&c.vertex_array, &c.physics_polygons);
            ask_physics(&mut pieces, va, polys, &c.physics_bsp, false, seed);
        }
    }
    models.print("models");
    pieces.print("pieces");
    assert_eq!((models.trees, pieces.trees), (1_258, 3_168));
    for (name, t) in [("models", &models), ("pieces", &pieces)] {
        for always in [
            "sweep: ours is every polygon's",
            "sweep, one hit: the same polygon",
            "walkable: the same",
            "solid: ours is every polygon's",
            "closed model, centre check: ours is inside or touching",
        ] {
            assert_eq!(t.otherwise(always), 0, "{name}: {always}");
        }
    }
    // Retail's own trees miss a few hits every polygon finds, where a polygon reaches a little
    // past a split, and judge the inside of a closed model otherwise at a few points.
    assert_eq!(models.otherwise("sweep: retail's is every polygon's"), 3);
    assert_eq!(models.otherwise("solid: retail's is every polygon's"), 2);
    assert_eq!(
        models.otherwise("closed model, centre check: retail's is inside or touching"),
        18
    );
    // A piece's leaves are never solid, so the centre check changes nothing there. An open model
    // has no inside to judge by, and the two trees carve the space behind it differently.
    assert_eq!(
        pieces.otherwise("open model or piece, centre check: the same"),
        0
    );
    assert!(models.otherwise("open model or piece, centre check: the same") <= 611);
}

#[test]
fn the_builders_cell_trees_answer_as_retails_do() {
    let s: RetailDatStore = dereth_dat::testing::open_store_or_fail();
    let mut t = Tally::default();
    let mut spheres_otherwise: BTreeSet<(u32, usize)> = BTreeSet::new();
    let name = |x: Bounding| match x {
        Bounding::Outside => 0,
        Bounding::PartiallyInside => 1,
        Bounding::EntirelyInside => 2,
    };
    for id in s.ids_of(DbType::Environment) {
        let e = Environment::decode_payload(id, &s.read_typed(DbType::Environment, id).unwrap())
            .unwrap();
        for (i, c) in e.cells.iter().enumerate() {
            let ours = bsp::cell(&c.vertex_array, &c.polygons);
            let a = walked(&c.vertex_array, &c.polygons, &c.cell_bsp);
            let b = walked(&c.vertex_array, &c.polygons, &ours);
            let same_planes = a.nodes.len() == b.nodes.len()
                && b.nodes.iter().all(|n| {
                    a.nodes.iter().any(|m| {
                        let (p, q) = (&n.splitting_plane, &m.splitting_plane);
                        let dn = p.normal.sub(q.normal);
                        dn.dot(dn) < 1e-6 && (p.d - q.d).abs() < 1e-3
                    })
                });
            let (lo, hi) = bounds(&c.vertex_array);
            let mut d = Draw((u64::from(id.raw()) << 8 | i as u64) | 1);
            t.trees += 1;
            t.add("the same planes", same_planes);
            for _ in 0..SAMPLES {
                let p = d.point(lo, hi, 0.5);
                t.add(
                    "point in the cell: the same",
                    a.point_inside_cell_bsp(p) == b.point_inside_cell_bsp(p),
                );
                let sp = Sphere::new(p, 0.05 + d.next());
                let same = name(a.sphere_intersects_cell_bsp(&sp))
                    == name(b.sphere_intersects_cell_bsp(&sp));
                t.add("sphere in the cell: the same", same);
                if !same {
                    spheres_otherwise.insert((id.raw(), i));
                }
            }
        }
    }
    t.print("cells");
    assert_eq!(t.trees, 3_168);
    assert_eq!(t.otherwise("point in the cell: the same"), 0);
    // Two spheres whose edges fall between the two trees' planes: a sloped face made of a polygon
    // and a doorway 1.4 mm out of line, which retail gives one plane for the whole face and ours a
    // plane through three of its corners; and a sphere 4 µm from a plane both trees share.
    assert_eq!(
        spheres_otherwise,
        BTreeSet::from([(0x0D00_0499, 5), (0x0D00_05AF, 3)]),
    );
}

#[test]
fn the_builders_drawing_trees_list_every_polygon_once_inside_their_spheres() {
    let s: RetailDatStore = dereth_dat::testing::open_store_or_fail();
    let mut t = Tally::default();
    let mut ratio: Vec<f32> = Vec::new();
    let mut check = |t: &mut Tally, va: &VertexArray, polys: &[Polygon], retail: &BspTree| {
        let ours = bsp::drawing(va, polys);
        t.trees += 1;
        let corners: Vec<Vec3> = polys
            .iter()
            .flat_map(|p| {
                p.vertex_ids
                    .iter()
                    .map(|i| va.vertices[usize::from(*i)].position)
            })
            .collect();
        let holds = |s: dereth_assets::Sphere| {
            corners
                .iter()
                .all(|c| c.sub(s.center).dot(c.sub(s.center)).sqrt() <= s.radius + 1e-3)
        };
        let ra = retail.nodes[0].sphere.unwrap_or_default();
        let rb = ours.nodes[0].sphere.unwrap_or_default();
        t.add("retail's root sphere holds every polygon", holds(ra));
        t.add("ours holds every polygon", holds(rb));
        if ra.radius > 0.0 {
            ratio.push(rb.radius / ra.radius);
        }
        let mut listed = vec![0usize; polys.len()];
        for n in &ours.nodes {
            for p in &n.in_polys {
                listed[usize::try_from(*p).unwrap()] += 1;
            }
        }
        t.add("every polygon listed once", listed.iter().all(|n| *n == 1));
    };
    for id in s.ids_of(DbType::GfxObj) {
        let g = GfxObj::decode_payload(id, &s.read_typed(DbType::GfxObj, id).unwrap()).unwrap();
        if let Some(tree) = &g.drawing_bsp {
            check(&mut t, &g.vertex_array, &g.polygons, tree);
        }
    }
    for id in s.ids_of(DbType::Environment) {
        let e = Environment::decode_payload(id, &s.read_typed(DbType::Environment, id).unwrap())
            .unwrap();
        for c in &e.cells {
            if let Some(tree) = &c.drawing_bsp {
                check(&mut t, &c.vertex_array, &c.polygons, tree);
            }
        }
    }
    t.print("drawing");
    ratio.sort_by(f32::total_cmp);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let at = |f: f64| ratio[((ratio.len() - 1) as f64 * f) as usize];
    println!(
        "our root radius over retail's: 1% {} median {} 99% {}",
        at(0.01),
        at(0.5),
        at(0.99)
    );
    assert_eq!(t.trees, 18_483);
    assert_eq!(t.otherwise("every polygon listed once"), 0);
    assert_eq!(t.otherwise("ours holds every polygon"), 0);
    assert_eq!(t.otherwise("retail's root sphere holds every polygon"), 228);
    assert!((0.9..1.0).contains(&at(0.5)));
}

#[test]
fn walks_at_the_builders_trees_end_where_walks_at_retails_do_as_often_as_retails_own_tie_breaks() {
    let s: RetailDatStore = dereth_dat::testing::open_store_or_fail();
    let human = DataId(0x0200_0001);
    let setup = Setup::decode_payload(human, &s.read_typed(DbType::Setup, human).unwrap()).unwrap();
    let body = Arc::new(dereth_world_data::setup::setup_geometry(&setup));
    let mut t = Tally::default();
    for id in s.ids_of(DbType::GfxObj) {
        let g = GfxObj::decode_payload(id, &s.read_typed(DbType::GfxObj, id).unwrap()).unwrap();
        let Some(tree) = &g.physics_bsp else { continue };
        let reach = g
            .vertex_array
            .vertices
            .iter()
            .map(|v| (v.position.x * v.position.x + v.position.y * v.position.y).sqrt())
            .fold(0.0, f32::max);
        if reach > 80.0 {
            // Too wide to walk round inside the nine landblocks a walk has.
            continue;
        }
        let ours = bsp::physics(&g.vertex_array, &g.physics_polygons, true);
        // The control: retail's own tree with each leaf's polygons the other way round, which
        // changes nothing but which of two polygons hit at once a sweep names.
        let mut reversed = tree.clone();
        for n in &mut reversed.nodes {
            n.in_polys.reverse();
        }
        let standing =
            |tr: &BspTree| fit::standing(&g, walked(&g.vertex_array, &g.physics_polygons, tr));
        let (ma, mb, mc) = (standing(tree), standing(&ours), standing(&reversed));
        let polys = walked(&g.vertex_array, &g.physics_polygons, tree).polygons;
        // Whether a walk ever sank the body 5 cm into the model.
        let sinks = |path: &[Vec3]| {
            path.iter().any(|at| {
                body.spheres.iter().any(|b| {
                    let s = Sphere::new(at.add(b.center), b.radius - 0.05);
                    polys.iter().any(|p| p.hits_sphere(&s))
                })
            })
        };
        let apart = |x: &[Vec3], y: &[Vec3]| {
            x.last()
                .zip(y.last())
                .map_or(f32::MAX, |(p, q)| p.sub(*q).dot(p.sub(*q)).sqrt())
        };
        t.trees += 1;
        for k in 0..8 {
            let (start, heading) = fit::approach(reach, k, 8);
            let pa = fit::path(&ma, &body, start, heading);
            let pb = fit::path(&mb, &body, start, heading);
            let pc = fit::path(&mc, &body, start, heading);
            t.add("ours ends within 1 cm of retail's", apart(&pa, &pb) < 0.01);
            t.add(
                "the control ends within 1 cm of retail's",
                apart(&pa, &pc) < 0.01,
            );
            t.add("retail's never sinks 5 cm in", !sinks(&pa));
            t.add("ours never sinks 5 cm in", !sinks(&pb));
        }
    }
    t.print("walks");
    assert_eq!(t.trees, 1_255);
    assert_eq!(t.otherwise("ours never sinks 5 cm in"), 0);
    assert_eq!(t.otherwise("retail's never sinks 5 cm in"), 0);
    // Where a walk meets two faces at once, the face it slides along is the tree's tie-break; ours
    // may part from retail's no more often than retail's own tree does with its leaves reversed.
    assert!(
        t.yes("ours ends within 1 cm of retail's")
            >= t.yes("the control ends within 1 cm of retail's"),
        "{t:?}"
    );
}
