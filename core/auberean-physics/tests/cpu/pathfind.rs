//! Path requests over a made-up landblock's path map: the search, what it avoids, the way it
//! returns, and the record the map is read from.
//! Fixture: landblock `0x1010`, a 3 by 3 grid of nodes 20 m apart (node `k` at row `(k-1)/3`,
//! column `(k-1)%3`, each in a cell of its own whose tree is one leaf), each linked to its four
//! neighbours by a 20 m link through a 4 m doorway halfway between them.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_dat::Reader;
use auberean_physics::geom;
use auberean_physics::geom::{Plane, Ray};
use auberean_physics::pack::Writer;
use auberean_physics::pathfind::bsp::{Bsp, BspLeaf, BspNode, BspTree};
use auberean_physics::pathfind::db::{PathMapDb, PathMaps};
use auberean_physics::pathfind::finder::{solution, Finder, NormalFinder};
use auberean_physics::pathfind::map::PathMap;
use auberean_physics::pathfind::node::{Link, LinkData, Node, NodeData, Spot};
use auberean_physics::pathfind::search::{outcome, SearchHost};
use auberean_physics::pathfind::system::Pathing;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

const BLOCK: u32 = 0x1010_0000;
const MAP_ID: u32 = BLOCK | 0xFFFD;

struct World {
    maps: HashMap<u32, Arc<PathMap>>,
}

impl PathMaps for World {
    fn path_map(&mut self, id: u32) -> Option<Arc<PathMap>> {
        self.maps.get(&id).cloned()
    }
}

impl SearchHost for World {
    fn now(&mut self) -> f64 {
        0.0
    }
    fn roll_dice(&mut self, lo: f32, _hi: f32) -> f32 {
        lo
    }
}

fn id(k: u32) -> u32 {
    BLOCK | k
}

fn cell(k: u32) -> CellId {
    CellId(BLOCK | k)
}

fn center(k: u32) -> Vec3 {
    let (row, col) = ((k - 1) / 3, (k - 1) % 3);
    #[allow(clippy::cast_precision_loss)]
    Vec3::new(10.0 + 20.0 * col as f32, 10.0 + 20.0 * row as f32, 0.0)
}

fn link(from: u32, to: u32, distance: f32) -> Link {
    let (a, b) = (center(from), center(to));
    let mid = Vec3::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5, 0.0);
    let (begin, end) = if (a.x - b.x).abs() > 0.0 {
        (
            Vec3::new(mid.x, mid.y - 2.0, 0.0),
            Vec3::new(mid.x, mid.y + 2.0, 0.0),
        )
    } else {
        (
            Vec3::new(mid.x - 2.0, mid.y, 0.0),
            Vec3::new(mid.x + 2.0, mid.y, 0.0),
        )
    };
    Link {
        dest_id: id(to),
        dest_in_map: false,
        data: Some(LinkData {
            distance,
            start_width: 5.0,
            mid_width: 5.0,
            end_width: 5.0,
            width: 5.0,
            height: 12.0,
            portal_begin: begin,
            portal_end: end,
        }),
    }
}

/// The grid, each link `distance(from, to)` long, node `k` with `data(k)`.
fn grid(distance: impl Fn(u32, u32) -> f32, data: impl Fn(u32) -> NodeData) -> PathMap {
    let mut m = PathMap::new(MAP_ID);
    for k in 1..=9u32 {
        let (row, col) = ((k - 1) / 3, (k - 1) % 3);
        let mut n = Node::new();
        n.id = id(k);
        n.center = Spot::new(cell(k), center(k));
        n.data = Some(data(k));
        let mut near = Vec::new();
        if col > 0 {
            near.push(k - 1);
        }
        if col < 2 {
            near.push(k + 1);
        }
        if row > 0 {
            near.push(k - 3);
        }
        if row < 2 {
            near.push(k + 3);
        }
        n.links = near
            .into_iter()
            .map(|t| link(k, t, distance(k, t)))
            .collect();
        assert!(m.add_path_node(n));
        m.cell_ids.insert(cell(k).0, k);
        m.bsp.insert(
            k,
            BspTree {
                id: k,
                is_leaf: true,
                root: Some(Bsp::Leaf(BspLeaf { node_id: id(k) })),
            },
        );
    }
    m.link_path_nodes();
    m
}

fn world(m: PathMap) -> World {
    let mut maps = HashMap::new();
    maps.insert(m.id, Arc::new(m));
    World { maps }
}

fn dry(_k: u32) -> NodeData {
    NodeData::default()
}

/// Runs a request from node `from` to node `to`, with `set` adjusting it; the outcome and the
/// way's nodes (as grid numbers).
fn search(w: &mut World, from: u32, to: u32, set: impl Fn(&mut NormalFinder)) -> (u32, Vec<u32>) {
    let mut sys = Pathing::new();
    let mut f = NormalFinder::new();
    f.base.soln = solution::END;
    set(&mut f);
    assert!(f.base.set_start(id(from), &mut sys.db, w));
    assert!(f.base.set_end(id(to), &mut sys.db, w));
    let r = sys.find_path(&mut f, w);
    let nodes = f
        .base()
        .inq_solution(solution::END)
        .map(|p| p.nodes.iter().map(|n| n & 0xFFFF).collect())
        .unwrap_or_default();
    (r, nodes)
}

/// Behaviour: pathfind.search.a-search-takes-the-least-distance-way
#[test]
fn a_search_takes_the_least_distance_way() {
    // Every link 30 m but those along the top row and down the right column.
    let cheap = |a: u32, b: u32| {
        let top = [1, 2, 3, 6, 9];
        let pos = |k| top.iter().position(|&t| t == k);
        match (pos(a), pos(b)) {
            (Some(i), Some(j)) if i.abs_diff(j) == 1 => 20.0,
            _ => 30.0,
        }
    };
    let mut w = world(grid(cheap, dry));
    let (r, nodes) = search(&mut w, 1, 9, |_| {});
    assert_eq!(r, outcome::FOUND);
    assert_eq!(nodes, vec![1, 2, 3, 6, 9]);
}

/// Behaviour: pathfind.search.a-search-keeps-a-dry-body-out-of-water
#[test]
fn a_search_keeps_a_dry_body_out_of_water() {
    let wet = |k: u32| NodeData {
        water: k == 2,
        ..NodeData::default()
    };
    let mut w = world(grid(|_, _| 20.0, wet));
    let (r, nodes) = search(&mut w, 1, 3, |_| {});
    assert_eq!(r, outcome::FOUND);
    assert_eq!(nodes, vec![1, 4, 5, 6, 3], "around the water");
    // Allowed in, the step into water costs 40 m: 60 m through still beats 80 m around.
    let (r, nodes) = search(&mut w, 1, 3, |f| f.water_constrained = false);
    assert_eq!(r, outcome::FOUND);
    assert_eq!(nodes, vec![1, 2, 3]);
    // Through water as long as around it, the way around is taken.
    let mut w = world(grid(|a, b| if a == 2 || b == 2 { 30.0 } else { 20.0 }, wet));
    let (_, nodes) = search(&mut w, 1, 3, |f| f.water_constrained = false);
    assert_eq!(nodes, vec![1, 4, 5, 6, 3]);
}

/// Behaviour: pathfind.search.a-search-ends-after-the-nodes-it-allows
#[test]
fn a_search_ends_after_the_nodes_it_allows() {
    // Along the bottom row the search takes node 1, then 2, then the end, 3.
    let mut w = world(grid(|_, _| 20.0, dry));
    let (r, nodes) = search(&mut w, 1, 3, |f| f.base.max_nodes = 2);
    assert_eq!(r, outcome::NOT_FOUND);
    assert!(nodes.is_empty());
    let (r, nodes) = search(&mut w, 1, 3, |f| f.base.max_nodes = 3);
    assert_eq!(r, outcome::FOUND, "the end is the third node taken");
    assert_eq!(nodes, vec![1, 2, 3]);
    assert_eq!(NormalFinder::new().base.max_nodes, 300);
}

/// Behaviour: pathfind.route.a-way-walks-through-each-doorway-facing-the-next
#[test]
fn a_way_walks_through_each_doorway_facing_the_next() {
    let mut w = world(grid(|_, _| 20.0, dry));
    let mut sys = Pathing::new();
    let mut f = NormalFinder::new();
    f.base.soln = solution::END;
    let start = Position::new(
        cell(1),
        Frame::new(Vec3::new(9.0, 11.0, 0.0), Quat::IDENTITY),
    );
    let end = Position::new(
        cell(3),
        Frame::new(Vec3::new(52.0, 8.0, 0.0), Quat::IDENTITY),
    );
    assert!(f.base.set_start_position(&start, &mut sys.db, &mut w));
    assert!(f.base.set_end_position(&end, &mut sys.db, &mut w));
    assert_eq!(sys.find_path(&mut f, &mut w), outcome::FOUND);
    let way = f.base.inq_solution(solution::END).expect("a way").clone();
    assert_eq!(way.nodes, vec![id(1), id(2), id(3)]);
    let mut db = PathMapDb::new();
    let ps = way.to_position_list(&mut db, &mut w).expect("positions");
    assert_eq!(ps.len(), 3);
    // Node 1 stands in the doorway to node 2, node 2 in the doorway to node 3; the end is the
    // request's own.
    assert_eq!(ps[0].cell, cell(1));
    assert_eq!(ps[0].frame.origin, Vec3::new(20.0, 10.0, 0.0));
    assert_eq!(ps[1].cell, cell(2));
    assert_eq!(ps[1].frame.origin, Vec3::new(40.0, 10.0, 0.0));
    assert_eq!(ps[2].cell, cell(3));
    assert_eq!(ps[2].frame.origin, end.frame.origin);
    // Each faces the next (east, then a little south of east); the last as it was reached.
    let h0 = geom::frame_heading(&ps[0].frame).degrees();
    let h1 = geom::frame_heading(&ps[1].frame).degrees();
    let h2 = geom::frame_heading(&ps[2].frame).degrees();
    assert!((h0 - 90.0).abs() < 0.01, "{h0}");
    assert!(h1 > 90.0 && h1 < 120.0, "{h1}");
    assert!((h2 - h1).abs() < 0.01, "{h2} {h1}");
}

/// Behaviour: pathfind.link.a-wide-body-keeps-to-the-wide-part-of-a-doorway
#[test]
fn a_wide_body_keeps_to_the_wide_part_of_a_doorway() {
    let door = |start, mid, end| Link {
        dest_id: 1,
        dest_in_map: true,
        data: Some(LinkData {
            start_width: start,
            mid_width: mid,
            end_width: end,
            portal_begin: Vec3::ZERO,
            portal_end: Vec3::new(4.0, 0.0, 0.0),
            ..LinkData::default()
        }),
    };
    let mut r = Ray::default();
    // Any body: the whole doorway.
    assert!(door(2.0, 5.0, 1.0).portal_constrained(&mut r, -1.0));
    assert_eq!((r.point, r.length), (Vec3::ZERO, 4.0));
    // 1.5 m wide: the 1 m end is drawn in an eighth of the way to the middle.
    assert!(door(2.0, 5.0, 1.0).portal_constrained(&mut r, 1.5));
    assert_eq!(r.point, Vec3::ZERO);
    assert!((r.length - 3.75).abs() < 1e-5, "{}", r.length);
    // 6 m wide, wider than the middle: only the half by the 12 m start, out to a seventh of the
    // way from the middle.
    assert!(door(12.0, 5.0, 1.0).portal_constrained(&mut r, 6.0));
    assert_eq!(r.point, Vec3::ZERO);
    assert!((r.length - (2.0 - 2.0 / 7.0)).abs() < 1e-5, "{}", r.length);
    // Wider than every part: the doorway is left whole.
    assert!(door(2.0, 5.0, 1.0).portal_constrained(&mut r, 6.0));
    assert_eq!(r.length, 4.0);
    // A link with no way leaves the ray alone.
    let mut none = Ray::default();
    assert!(!Link::default().portal_constrained(&mut none, 1.0));
    assert_eq!(none, Ray::default());
}

/// Behaviour: pathfind.record.a-path-map-keeps-places-to-a-tenth-and-every-node-passable
#[test]
fn a_path_map_keeps_places_to_a_tenth_and_every_node_passable() {
    let mut m = PathMap::new(MAP_ID);
    m.version = 7;
    let mut a = Node::new();
    a.id = id(1);
    a.center = Spot::new(cell(1), Vec3::new(12.34, 56.78, -1.05));
    a.data = Some(NodeData {
        slope: 3,
        passable: false,
        water: true,
        solid: false,
        height: 5.5,
        width: 0.5,
    });
    a.links = vec![Link {
        dest_id: id(2),
        dest_in_map: false,
        data: Some(LinkData {
            distance: 7.25,
            start_width: 1.5,
            mid_width: 13.0,
            end_width: -1.0,
            width: 0.0,
            height: 41.0,
            portal_begin: Vec3::new(3000.0, -3000.04, 0.0),
            portal_end: Vec3::new(3000.01, 0.0, 0.0),
        }),
    }];
    let mut b = Node::new();
    b.id = id(2);
    b.center = Spot::new(cell(1), Vec3::new(30.0, 50.0, 0.0));
    b.data = Some(NodeData::default());
    assert!(m.add_path_node(a));
    assert!(m.add_path_node(b));
    m.cell_ids.insert(cell(1).0, 1);
    let mut split = BspNode::new();
    split.plane = Plane {
        normal: Vec3::new(1.0, 0.0, 0.0),
        d: -20.0,
    };
    split.make_pos_leaf(id(2));
    split.make_neg_leaf(id(1));
    m.bsp.insert(
        1,
        BspTree {
            id: 1,
            is_leaf: false,
            root: Some(Bsp::Node(Box::new(split))),
        },
    );
    let mut w = Writer::new();
    assert!(m.write(&mut w));
    let mut r = Reader::new(w.as_bytes());
    let back = PathMap::read(&mut r, MAP_ID).expect("reads");
    r.finish().expect("every byte");

    assert_eq!(back.version, 7);
    let a = back.node(id(1)).expect("node 1");
    assert_eq!(a.center.offset, Vec3::new(12.3, 56.8, -1.0));
    let d = a.data.expect("ground");
    assert!(d.passable, "every node read is passable");
    assert!(d.water && !d.solid);
    assert_eq!((d.slope, d.height, d.width), (3, 5.0, 0.0));
    let l = a.links[0];
    assert!(l.dest_in_map);
    let ld = l.data.expect("way");
    assert_eq!(ld.distance, 7.25);
    assert_eq!(
        (ld.start_width, ld.mid_width, ld.end_width, ld.height),
        (1.0, 12.0, -1.0, 40.0)
    );
    assert_eq!(ld.width, 12.0, "the widest of the three");
    // 3,000 m itself is on the grid; past the bound a coordinate saturates.
    assert_eq!(ld.portal_begin, Vec3::new(3000.0, -3000.0, 0.0));
    assert_eq!(ld.portal_end.x, 3000.0);
    // The tree finds the node on each side of x = 20.
    let at = |x| {
        back.node_at(&Spot::new(cell(1), Vec3::new(x, 5.0, 0.0)))
            .map(|n| n.id)
    };
    assert_eq!(at(19.0), Some(id(1)));
    assert_eq!(at(20.0), Some(id(2)), "on the plane is the positive side");
}
