//! A landblock's path map (record type `0x40000035`, filed in the cell file at the block's
//! `0xFFFD`): its nodes, a tree of planes per cell that finds the node under a point, and while a
//! map is being built, the floor patch (a polygon) each node stands for.
//!
//! The record: its id, a version word, the counts of nodes, of cells and of trees, then the
//! nodes, each cell with its tree's id, and the trees. Reading it links every node's links to
//! the nodes of the same map.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use super::bsp::BspTree;
use super::ids;
use super::node::{Node, Spot};
use crate::geom::Polygon;
use crate::pack::Writer;
use auberean_dat::reader::Result;
use auberean_dat::Reader;

/// The record type of a path map.
// D2-13471
pub const RECORD_TYPE: u32 = 0x4000_0035;

/// A landblock's path map.
// D2-13470, D2-13465, D2-13466, D2-13483
#[derive(Clone, Debug, PartialEq)]
pub struct PathMap {
    pub id: u32,
    pub version: u32,
    /// The links counted when last asked (0: not counted since the nodes changed).
    pub num_links: u32,
    pub nodes: BTreeMap<u32, Node>,
    pub cluster: u32,
    /// Each tree by its id.
    pub bsp: BTreeMap<u32, BspTree>,
    /// Each cell's tree id.
    pub cell_ids: BTreeMap<u32, u32>,
    pub next_bsp_id: u32,
    pub next_node_id: u32,
    /// The floor patch of each node, kept while a map is built.
    pub node_polygons: Option<HashMap<u32, Polygon>>,
}

impl Default for PathMap {
    // D2-13484, D2-05017, D2-13487
    fn default() -> Self {
        Self {
            id: 0,
            version: 0,
            num_links: 0,
            nodes: BTreeMap::new(),
            cluster: 1,
            bsp: BTreeMap::new(),
            cell_ids: BTreeMap::new(),
            next_bsp_id: 0,
            next_node_id: 0,
            node_polygons: None,
        }
    }
}

impl PathMap {
    #[must_use]
    pub fn new(id: u32) -> Self {
        Self {
            id,
            ..Self::default()
        }
    }

    // D2-13458
    pub fn clear_node_polygons(&mut self) {
        self.node_polygons = None;
    }

    /// The links out of all its nodes.
    // D2-13459
    #[must_use]
    pub fn count_links(&self) -> u32 {
        let n: usize = self.nodes.values().map(|n| n.links.len()).sum();
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    /// Finds each node's links' nodes among this map's, and forgets the link count.
    // D2-13460
    pub fn link_path_nodes(&mut self) -> bool {
        let ids: BTreeSet<u32> = self.nodes.keys().copied().collect();
        for n in self.nodes.values_mut() {
            n.link(&ids);
        }
        self.num_links = 0;
        true
    }

    /// Drops every node.
    // D2-13461
    pub fn clear_table(&mut self) {
        self.nodes.clear();
        self.num_links = 0;
    }

    /// Drops every tree.
    // D2-13462
    pub fn clear_bsp(&mut self) {
        self.bsp.clear();
    }

    /// The number of links, counted when not known.
    // D2-13463
    #[must_use]
    pub fn num_links(&self) -> u32 {
        if self.num_links == 0 {
            self.count_links()
        } else {
            self.num_links
        }
    }

    /// The map as text: its id, its counts, and each node with its links' nodes and distances.
    // D2-13464
    pub fn dump(&self, out: &mut String) -> bool {
        let _ = writeln!(out, "PathMap:: id: 0x{:08X}", self.id);
        let _ = writeln!(
            out,
            "  [nodes: {}, links: {}]",
            self.nodes.len(),
            self.num_links()
        );
        for n in self.nodes.values() {
            let _ = write!(out, "id: 0x{:08X} [", n.id);
            let last = n.links.len().saturating_sub(1);
            for (i, l) in n.links.iter().enumerate() {
                let d = l.data.map_or(0.0, |d| d.distance);
                let sep = if i < last { "," } else { " " };
                let _ = write!(out, " 0x{:08X} ({:.1}){}", l.dest_id, d, sep);
            }
            out.push_str("]\n");
        }
        true
    }

    /// Empties the map and starts its id counters over: trees from the block's first tree id,
    /// inserted nodes from the block's first inserted node id (`first_insert_node`, the land's
    /// own nodes coming before).
    // D2-13467
    pub fn clear(&mut self, first_insert_node: u32) {
        self.clear_table();
        self.clear_bsp();
        self.cell_ids.clear();
        self.cluster = 1;
        self.next_bsp_id = (self.id & ids::BLOCK_MASK).wrapping_add(ids::FIRST_BSP_ID);
        self.next_node_id = (self.id & ids::BLOCK_MASK).wrapping_add(first_insert_node);
        self.clear_node_polygons();
    }

    // D2-13468
    #[must_use]
    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Where a node is.
    // D2-13472
    #[must_use]
    pub fn query_position(&self, id: u32) -> Option<Spot> {
        self.nodes.get(&id).map(|n| n.center)
    }

    /// Adds a node by its id; a node with id 0, or an id already held, is refused.
    // D2-13473
    pub fn add_path_node(&mut self, node: Node) -> bool {
        if node.id == 0 || self.nodes.contains_key(&node.id) {
            return false;
        }
        self.nodes.insert(node.id, node);
        true
    }

    /// The tree of a cell.
    // D2-13475
    #[must_use]
    pub fn bsp_of_cell(&self, cell: u32) -> Option<&BspTree> {
        let id = self.cell_ids.get(&cell)?;
        self.bsp.get(id)
    }

    // D2-13476
    #[must_use]
    pub fn node_polygon(&self, id: u32) -> Option<&Polygon> {
        self.node_polygons.as_ref()?.get(&id)
    }

    /// Whether `poly` is the floor patch kept for node `id`: as many corners, and each corner,
    /// starting from the one of `poly` nearest the kept patch's first (within 0.05 m, counted
    /// across x and y), within 0.05 m of the kept patch's corner in turn.
    // D2-13477
    #[must_use]
    pub fn check_node_polygon(&self, poly: &Polygon, id: u32) -> bool {
        let Some(kept) = self.node_polygon(id) else {
            return false;
        };
        let n = kept.len();
        if n == 0 || n != poly.len() {
            return false;
        }
        let k0 = kept[0];
        let mut best = None;
        let mut nearest = 0.05f32;
        for (i, v) in poly.iter().enumerate() {
            let d = (v.x - k0.x).abs() + (v.y - k0.y).abs();
            if d < nearest {
                best = Some(i);
                nearest = d;
            }
        }
        let Some(start) = best else {
            return false;
        };
        let mut same = true;
        for (i, k) in kept.iter().enumerate() {
            let v = poly[(start + i) % n];
            if 0.05 < (v.x - k.x).abs() + (v.y - k.y).abs() {
                same = false;
            }
        }
        same
    }

    /// The node under a place: found by the tree of its cell (none without one, or without a node
    /// by the id found).
    // D2-13485
    #[must_use]
    pub fn node_at(&self, at: &Spot) -> Option<&Node> {
        let id = self
            .bsp_of_cell(at.cell.0)
            .map_or(0, |t| t.find_node_id(at.offset));
        self.nodes.get(&id)
    }

    /// Keeps a node's floor patch; a node already with one, or id 0, is refused.
    // D2-13486
    pub fn set_node_polygon(&mut self, poly: Polygon, id: u32) -> bool {
        let table = self.node_polygons.get_or_insert_with(HashMap::new);
        if id == 0 || table.contains_key(&id) {
            return false;
        }
        table.insert(id, poly);
        true
    }

    /// The record (see the module's header). A node, tree or cell that cannot be added (an id of
    /// 0 or held twice) fails the read.
    // D2-13474
    pub fn read(r: &mut Reader<'_>, id: u32) -> Result<Self> {
        r.expect_id(id)?;
        let mut m = Self::new(id);
        m.version = r.u32()?;
        let nodes = r.count(16)?;
        let cells = r.count(8)?;
        let trees = r.count(12)?;
        for _ in 0..nodes {
            let at = r.pos();
            let n = Node::read(r)?;
            if !m.add_path_node(n) {
                return Err(auberean_dat::DecodeError {
                    at,
                    what: "a node with id 0 or held twice".into(),
                });
            }
        }
        for _ in 0..cells {
            let cell = r.u32()?;
            let tree = r.u32()?;
            m.cell_ids.insert(cell, tree);
        }
        for _ in 0..trees {
            let at = r.pos();
            let t = BspTree::read(r)?;
            if t.id == 0 || m.bsp.contains_key(&t.id) {
                return Err(auberean_dat::DecodeError {
                    at,
                    what: "a tree with id 0 or held twice".into(),
                });
            }
            m.bsp.insert(t.id, t);
        }
        m.link_path_nodes();
        Ok(m)
    }

    /// The record, in the order it is read; a node or tree that cannot be written fails.
    pub fn write(&self, w: &mut Writer) -> bool {
        w.u32(self.id);
        w.u32(self.version);
        w.u32(u32::try_from(self.nodes.len()).unwrap_or(u32::MAX));
        w.u32(u32::try_from(self.cell_ids.len()).unwrap_or(u32::MAX));
        w.u32(u32::try_from(self.bsp.len()).unwrap_or(u32::MAX));
        for n in self.nodes.values() {
            if !n.write(w) {
                return false;
            }
        }
        for (cell, tree) in &self.cell_ids {
            w.u32(*cell);
            w.u32(*tree);
        }
        self.bsp.values().all(|t| t.write(w))
    }
}
