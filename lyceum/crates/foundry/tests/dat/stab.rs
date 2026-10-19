//! Behaviour: none (tooling: the stab-list builder, calibrated on retail's own lists)
//! The builder's lists for retail's cells and doorways, against the lists retail's files hold,
//! over every landblock with buildings and every sixteenth of the rest.

use std::collections::{BTreeMap, BTreeSet};

use dereth_assets::world::{EnvCell, LandblockInfo};
use dereth_assets::{Decode, Environment};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::DataId;
use lyceum_foundry::stab::{buildings, doorway_list, lists, shape, CellShape, Cells, WIDEN};

/// What the builder's lists are, against retail's.
#[derive(Debug, Default, PartialEq, Eq)]
struct Tally {
    cells: usize,
    equal: usize,
    in_order: usize,
    retail_entries: usize,
    missing: usize,
    /// The missing entries, each past two steps of the builder's sight, which retail's own rule
    /// does not reach: the rest of a building whose cell retail takes as seen from outside and
    /// the builder does not; and the others.
    missing_building_seen_outside: usize,
    missing_past_two_steps: usize,
    extra: usize,
    /// Of the extra entries, the rest of a building whose cell the builder takes as seen from
    /// outside and retail does not.
    extra_building_seen_outside: usize,
    seen_outside_equal: usize,
    doorways: usize,
    doorways_equal: usize,
}

fn tally(sample: impl Fn(u16, bool) -> bool) -> Tally {
    let s: RetailDatStore = dereth_dat::testing::open_store_or_fail();
    let ids: Vec<DataId> = s.cell().iter_ids().collect();
    let mut by_lb: BTreeMap<u16, Vec<DataId>> = BTreeMap::new();
    let mut infos: BTreeMap<u16, LandblockInfo> = BTreeMap::new();
    for id in &ids {
        #[allow(clippy::cast_possible_truncation)]
        let lb = (id.raw() >> 16) as u16;
        match dereth_dat::classify_cell_id(*id) {
            Some(DbType::Cell) => by_lb.entry(lb).or_default().push(*id),
            Some(DbType::Lbi) => {
                let b = s.read_cell(*id).unwrap();
                infos.insert(lb, LandblockInfo::decode_payload(*id, &b).unwrap());
            }
            _ => {}
        }
    }
    let mut envs: BTreeMap<u32, Option<Environment>> = BTreeMap::new();
    let mut t = Tally::default();
    for (lb, cids) in &by_lb {
        let info = infos.get(lb);
        let has_buildings = info.is_some_and(|i| !i.buildings.is_empty());
        if !sample(*lb, has_buildings) {
            continue;
        }
        let cells: Vec<EnvCell> = cids
            .iter()
            .map(|id| EnvCell::decode_payload(*id, &s.read_cell(*id).unwrap()).unwrap())
            .collect();
        let mut shapes: BTreeMap<u16, CellShape> = BTreeMap::new();
        for c in &cells {
            let e = envs.entry(c.environment.raw()).or_insert_with(|| {
                s.read_typed(DbType::Environment, c.environment)
                    .ok()
                    .and_then(|b| Environment::decode_payload(c.environment, &b).ok())
            });
            let sh = shape(c, e.as_ref().expect("every retail environment"))
                .expect("every retail cell's piece");
            shapes.insert(sh.id, sh);
        }
        let doorways: Vec<Vec<u16>> = info
            .map(|i| {
                i.buildings
                    .iter()
                    .map(|b| b.portals.iter().map(|p| p.other_cell_id).collect())
                    .collect()
            })
            .unwrap_or_default();
        let houses = buildings(&shapes, &doorways);
        let which: BTreeSet<u16> = shapes.keys().copied().collect();
        let ours = lists(&shapes, &houses, &which);
        // Two steps of sight, both ways, as the builder's one step sees.
        let g = Cells::widened(&shapes.values().cloned().collect::<Vec<_>>(), WIDEN);
        let mut one: BTreeMap<u16, BTreeSet<u16>> = BTreeMap::new();
        for c in &which {
            for x in g.seen(*c) {
                one.entry(*c).or_default().insert(x);
                one.entry(x).or_default().insert(*c);
            }
        }
        let two = |a: u16| -> BTreeSet<u16> {
            let first = one.get(&a).cloned().unwrap_or_default();
            let mut t = first.clone();
            for c in &first {
                t.extend(one.get(c).into_iter().flatten().copied());
            }
            t
        };
        for c in &cells {
            #[allow(clippy::cast_possible_truncation)]
            let me = (c.id.raw() & 0xFFFF) as u16;
            let (list, seen) = &ours[&me];
            let a: BTreeSet<u16> = list.iter().copied().collect();
            let r: BTreeSet<u16> = c.visible_cells.iter().copied().collect();
            t.cells += 1;
            t.retail_entries += r.len();
            t.missing += r.difference(&a).count();
            let reach = two(me);
            let retail_seen = c.flags & 1 != 0;
            let house = |b: &u16| houses.iter().any(|h| h.contains(&me) && h.contains(b));
            for b in r.difference(&a) {
                if reach.contains(b) {
                    continue;
                }
                if retail_seen && !*seen && house(b) {
                    t.missing_building_seen_outside += 1;
                } else {
                    t.missing_past_two_steps += 1;
                }
            }
            t.extra += a.difference(&r).count();
            if *seen && !retail_seen {
                t.extra_building_seen_outside += a
                    .difference(&r)
                    .filter(|b| house(b) && !reach.contains(b))
                    .count();
            }
            if a == r {
                t.equal += 1;
                if *list == c.visible_cells {
                    t.in_order += 1;
                }
            }
            if *seen == (c.flags & 1 != 0) {
                t.seen_outside_equal += 1;
            }
        }
        if let Some(i) = info {
            for (b, house) in i.buildings.iter().zip(&houses) {
                for p in &b.portals {
                    t.doorways += 1;
                    if doorway_list(house) == p.stab_list {
                        t.doorways_equal += 1;
                    }
                }
            }
        }
    }
    t
}

/// Every retail cell of a landblock with buildings: the builder's lists hold everything retail's
/// do but what retail lists past two steps of sight (its own rule does not reach those), a
/// cell's seen-outside flag is retail's for nearly all, and a doorway's list is its building.
#[test]
fn the_builders_lists_match_retails_in_landblocks_with_buildings() {
    let t = tally(|_, buildings| buildings);
    println!("{t:#?}");
    assert_eq!(t.cells, 94_268);
    assert_eq!(t.doorways, 16_937);
    assert_eq!(
        t.missing,
        t.missing_building_seen_outside + t.missing_past_two_steps,
        "every missing entry is past two steps of sight: {t:#?}"
    );
    assert_eq!(
        (t.missing_building_seen_outside, t.missing_past_two_steps),
        MEASURED_BUILDINGS,
        "{t:#?}"
    );
    assert!(t.doorways_equal * 1000 >= t.doorways * 989, "{t:#?}");
    assert!(t.equal * 100 >= t.cells * 88, "{t:#?}");
    assert!(t.seen_outside_equal * 1000 >= t.cells * 989, "{t:#?}");
    assert!(t.extra * 100 <= t.retail_entries * 21, "{t:#?}");
}

/// What retail lists past two steps of the builder's sight in landblocks with buildings: the
/// rest of a building retail takes as seen from outside and the builder does not, and the others
/// (as measured).
const MEASURED_BUILDINGS: (usize, usize) = (3_030, 5_681);

/// Every sixteenth landblock without buildings (its dungeons): the builder's lists hold
/// everything retail's do but what retail lists past two steps of sight.
#[test]
fn the_builders_lists_match_retails_in_dungeons() {
    let t = tally(|lb, buildings| !buildings && lb % 16 == 0);
    println!("{t:#?}");
    assert_eq!(t.cells, 55_549);
    assert_eq!(t.missing, t.missing_past_two_steps, "{t:#?}");
    assert_eq!(t.missing_past_two_steps, MEASURED_DUNGEONS, "{t:#?}");
    assert!(t.equal * 100 >= t.cells * 77, "{t:#?}");
    assert!(t.extra * 100 <= t.retail_entries * 2, "{t:#?}");
    assert_eq!(t.seen_outside_equal, t.cells, "{t:#?}");
}

/// What retail lists past two steps of the builder's sight in every sixteenth dungeon landblock
/// (as measured).
const MEASURED_DUNGEONS: usize = 4_344;

type P = [f64; 3];

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: P, b: P) -> P {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// One polygon of a cell in landblock space: its corners, its normal, and where it leads when it
/// is a portal (`Some(None)` to the outdoors).
struct Face {
    corners: Vec<P>,
    n: P,
    to: Option<Option<u16>>,
}

/// A cell as the rays see it: its polygons, its extent, and the planes of its cell BSP, each
/// facing the cell's middle (a point in front of all of them is in the cell).
struct Room {
    faces: Vec<Face>,
    lo: P,
    hi: P,
    planes: Vec<(P, f64)>,
}

fn room(c: &EnvCell, e: &Environment) -> Room {
    let st = &e.cells[usize::from(c.cell_struct)];
    let place = |p: dereth_primitives::Vec3| {
        let q = dereth_primitives::frame::localtoglobal(&c.frame, p);
        [f64::from(q.x), f64::from(q.y), f64::from(q.z)]
    };
    let portals: BTreeMap<u16, Option<u16>> = c
        .portals
        .iter()
        .map(|p| {
            #[allow(clippy::cast_possible_truncation)]
            let to = (p.flags & 4 == 0).then_some(p.other_cell_id as u16);
            (p.polygon_id, to)
        })
        .collect();
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    for v in &st.vertex_array.vertices {
        let p = place(v.position);
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let mut faces = Vec::new();
    for (i, poly) in st.polygons.iter().enumerate() {
        let corners: Vec<P> = poly
            .vertex_ids
            .iter()
            .filter_map(|v| st.vertex_array.vertices.get(usize::from(*v)))
            .map(|v| place(v.position))
            .collect();
        if corners.len() < 3 {
            continue;
        }
        let mut n = [0.0; 3];
        for k in 0..corners.len() {
            let (a, b) = (corners[k], corners[(k + 1) % corners.len()]);
            n[0] += (a[1] - b[1]) * (a[2] + b[2]);
            n[1] += (a[2] - b[2]) * (a[0] + b[0]);
            n[2] += (a[0] - b[0]) * (a[1] + b[1]);
        }
        let l = dot(n, n).sqrt();
        if l < 1e-9 {
            continue;
        }
        let to = u16::try_from(i).ok().and_then(|i| portals.get(&i).copied());
        faces.push(Face {
            corners,
            n: [n[0] / l, n[1] / l, n[2] / l],
            to,
        });
    }
    let middle = [
        (lo[0] + hi[0]) / 2.0,
        (lo[1] + hi[1]) / 2.0,
        (lo[2] + hi[2]) / 2.0,
    ];
    let origin = dereth_primitives::frame::localtoglobal(&c.frame, dereth_primitives::Vec3::ZERO);
    let planes = st
        .cell_bsp
        .nodes
        .iter()
        .filter_map(|node| node.plane.as_ref())
        .map(|pl| {
            let at = place(dereth_primitives::Vec3::new(
                -pl.d * pl.normal.x,
                -pl.d * pl.normal.y,
                -pl.d * pl.normal.z,
            ));
            let q = dereth_primitives::frame::localtoglobal(&c.frame, pl.normal);
            let mut n = [
                f64::from(q.x - origin.x),
                f64::from(q.y - origin.y),
                f64::from(q.z - origin.z),
            ];
            let mut d = -dot(n, at);
            if dot(n, middle) + d < 0.0 {
                n = [-n[0], -n[1], -n[2]];
                d = -d;
            }
            (n, d)
        })
        .collect();
    Room {
        faces,
        lo,
        hi,
        planes,
    }
}

/// How far along the ray from `p` going `dir` it meets `f`, if it does.
fn meets(f: &Face, p: P, dir: P) -> Option<f64> {
    let nd = dot(f.n, dir);
    if nd.abs() < 1e-12 {
        return None;
    }
    let t = dot(f.n, sub(f.corners[0], p)) / nd;
    if t <= 1e-7 {
        return None;
    }
    let e = [p[0] + dir[0] * t, p[1] + dir[1] * t, p[2] + dir[2] * t];
    let mut sign = 0.0f64;
    for k in 0..f.corners.len() {
        let (a, b) = (f.corners[k], f.corners[(k + 1) % f.corners.len()]);
        let x = dot(cross(sub(b, a), sub(e, a)), f.n);
        if x.abs() < 1e-9 {
            continue;
        }
        if sign == 0.0 {
            sign = x.signum();
        } else if x.signum() != sign {
            return None;
        }
    }
    Some(t)
}

/// Each cell's sight by `rays` rays from points inside it in every direction, each walked through
/// the cells' own polygons: on through a portal it leaves the cell by, into the cell beyond, and
/// stopped by anything else (a wall, the outdoors, or a portal met from its far side).
fn ray_sight(rooms: &BTreeMap<u16, Room>, rays: usize) -> BTreeMap<u16, BTreeSet<u16>> {
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut rnd = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        #[allow(clippy::cast_precision_loss)]
        let r = (seed >> 11) as f64 / (1u64 << 53) as f64;
        r
    };
    let mut out = BTreeMap::new();
    for (src, r) in rooms {
        let mut seen = BTreeSet::new();
        let (mut cast, mut tried) = (0, 0);
        while cast < rays && tried < rays * 50 {
            tried += 1;
            let a = [
                r.lo[0] + (r.hi[0] - r.lo[0]) * rnd(),
                r.lo[1] + (r.hi[1] - r.lo[1]) * rnd(),
                r.lo[2] + (r.hi[2] - r.lo[2]) * rnd(),
            ];
            if r.planes.iter().any(|(n, d)| dot(*n, a) + d < 0.0) {
                continue;
            }
            cast += 1;
            let z = 2.0 * rnd() - 1.0;
            let turn = 2.0 * std::f64::consts::PI * rnd();
            let flat = (1.0 - z * z).sqrt();
            let dir = [
                flat * dereth_primitives::num::math::cos(turn),
                flat * dereth_primitives::num::math::sin(turn),
                z,
            ];
            let (mut at, mut p) = (*src, a);
            for _ in 0..200 {
                let here = &rooms[&at];
                let Some((t, f)) = here
                    .faces
                    .iter()
                    .filter_map(|f| meets(f, p, dir).map(|t| (t, f)))
                    .min_by(|x, y| x.0.total_cmp(&y.0))
                else {
                    break;
                };
                let Some(Some(to)) = f.to else { break };
                let middle = [
                    (here.lo[0] + here.hi[0]) / 2.0,
                    (here.lo[1] + here.hi[1]) / 2.0,
                    (here.lo[2] + here.hi[2]) / 2.0,
                ];
                let face_middle = f
                    .corners
                    .iter()
                    .fold([0.0; 3], |m, c| [m[0] + c[0], m[1] + c[1], m[2] + c[2]]);
                #[allow(clippy::cast_precision_loss)]
                let k = f.corners.len() as f64;
                let face_middle = [face_middle[0] / k, face_middle[1] / k, face_middle[2] / k];
                let outward = if dot(f.n, sub(middle, face_middle)) > 0.0 {
                    -dot(f.n, dir)
                } else {
                    dot(f.n, dir)
                };
                if outward <= 0.0 || !rooms.contains_key(&to) {
                    break;
                }
                at = to;
                if at != *src {
                    seen.insert(at);
                }
                p = [
                    p[0] + dir[0] * (t + 1e-5),
                    p[1] + dir[1] * (t + 1e-5),
                    p[2] + dir[2] * (t + 1e-5),
                ];
            }
        }
        out.insert(*src, seen);
    }
    out
}

/// In a tower whose cells name cells elsewhere through portals with no way back (`F85D`), and in
/// two dungeons with such portals (`0020`, `0160`): every cell that rays walked through the
/// cells' own polygons see from a cell, the walk sees from it too.
#[test]
fn every_cell_rays_see_through_the_portals_the_walk_sees() {
    let s: RetailDatStore = dereth_dat::testing::open_store_or_fail();
    for (lb, least) in [(0xF85Du16, 50usize), (0x0020, 10_000), (0x0160, 10_000)] {
        let mut cells: BTreeMap<u16, EnvCell> = BTreeMap::new();
        for id in s.cell().iter_ids() {
            #[allow(clippy::cast_possible_truncation)]
            let (block, c) = ((id.raw() >> 16) as u16, (id.raw() & 0xFFFF) as u16);
            if block == lb && dereth_dat::classify_cell_id(id) == Some(DbType::Cell) {
                let cell = EnvCell::decode_payload(id, &s.read_cell(id).unwrap()).unwrap();
                cells.insert(c, cell);
            }
        }
        let mut envs: BTreeMap<u32, Environment> = BTreeMap::new();
        for c in cells.values() {
            envs.entry(c.environment.raw()).or_insert_with(|| {
                let bytes = s.read_typed(DbType::Environment, c.environment).unwrap();
                Environment::decode_payload(c.environment, &bytes).unwrap()
            });
        }
        let rooms: BTreeMap<u16, Room> = cells
            .iter()
            .map(|(id, c)| (*id, room(c, &envs[&c.environment.raw()])))
            .collect();
        let shapes: Vec<CellShape> = cells
            .values()
            .map(|c| shape(c, &envs[&c.environment.raw()]).unwrap())
            .collect();
        let walk = Cells::widened(&shapes, WIDEN);
        let rays = ray_sight(&rooms, 5_000);
        let sightings: usize = rays.values().map(BTreeSet::len).sum();
        assert!(sightings >= least, "{lb:04X}: {sightings} sightings");
        for (a, seen) in &rays {
            let walked: BTreeSet<u16> = walk.seen(*a).into_iter().collect();
            let beyond: Vec<&u16> = seen.difference(&walked).collect();
            assert!(
                beyond.is_empty(),
                "{lb:04X}: from {a:04X} rays see {beyond:04X?} the walk does not"
            );
        }
    }
}
