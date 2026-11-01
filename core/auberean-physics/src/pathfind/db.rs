//! The path maps a search reaches: loaded by id as the search needs them and held until it ends.
//! The first map held answers first; any other is loaded again (and held again) each time.

use std::sync::Arc;

use super::ids::{path_map_id, BLOCK_MASK, PATH_MAP_CELL};
use super::map::PathMap;
use super::node::{Node, Spot};

/// Where path maps come from: the cell file's record at a landblock's `0xFFFD`.
pub trait PathMaps {
    /// The map by its id, or none.
    fn path_map(&mut self, id: u32) -> Option<Arc<PathMap>>;
}

/// A node of a loaded map.
#[derive(Clone, Debug)]
pub struct NodeRef {
    pub map: Arc<PathMap>,
    pub id: u32,
}

impl NodeRef {
    /// The node (a reference is made only to a node its map holds).
    #[must_use]
    pub fn node(&self) -> &Node {
        &self.map.nodes[&self.id]
    }

    /// The same node of the same map.
    #[must_use]
    pub fn same(&self, other: &NodeRef) -> bool {
        self.id == other.id && Arc::ptr_eq(&self.map, &other.map)
    }

    /// The node `id` of the same map, when it holds one.
    #[must_use]
    pub fn sibling(&self, id: u32) -> Option<NodeRef> {
        self.map.node(id).map(|_| NodeRef {
            map: Arc::clone(&self.map),
            id,
        })
    }
}

/// The maps held.
#[derive(Clone, Debug, Default)]
pub struct PathMapDb {
    pub maps: Vec<Arc<PathMap>>,
}

impl PathMapDb {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads a map and holds it (again, when already held).
    // D2-13499
    pub fn get_map<S: PathMaps + ?Sized>(
        &mut self,
        source: &mut S,
        id: u32,
    ) -> Option<Arc<PathMap>> {
        let m = source.path_map(id)?;
        self.maps.push(Arc::clone(&m));
        Some(m)
    }

    /// Lets go of every map held.
    // D2-13505
    pub fn clear_path_maps(&mut self) {
        self.maps.clear();
    }

    /// The links of the first map held.
    // D2-13509
    #[must_use]
    pub fn num_links(&self) -> u32 {
        self.maps.first().map_or(0, |m| m.num_links())
    }

    /// The nodes of the first map held.
    // D2-13510
    #[must_use]
    pub fn num_nodes(&self) -> u32 {
        self.maps
            .first()
            .map_or(0, |m| u32::try_from(m.nodes.len()).unwrap_or(u32::MAX))
    }

    /// A map as text (loaded for the purpose, not held); none when it does not load.
    // D2-13511
    pub fn dump_path_map<S: PathMaps + ?Sized>(source: &mut S, id: u32, out: &mut String) -> bool {
        match source.path_map(id) {
            Some(m) => m.dump(out),
            None => false,
        }
    }

    fn map_for<S: PathMaps + ?Sized>(
        &mut self,
        source: &mut S,
        map_id: u32,
    ) -> Option<Arc<PathMap>> {
        if let Some(first) = self.maps.first() {
            if first.id == map_id {
                return Some(Arc::clone(first));
            }
        }
        self.get_map(source, map_id)
    }

    /// A node by its id, from its landblock's map.
    // D2-13512
    pub fn node<S: PathMaps + ?Sized>(&mut self, source: &mut S, id: u32) -> Option<NodeRef> {
        let map = self.map_for(source, path_map_id(id))?;
        map.node(id)?;
        Some(NodeRef { map, id })
    }

    /// The node under a place, from its landblock's map.
    // D2-13513
    pub fn node_at<S: PathMaps + ?Sized>(&mut self, source: &mut S, at: &Spot) -> Option<NodeRef> {
        let map_id = (at.cell.0 & BLOCK_MASK) | PATH_MAP_CELL;
        let map = self.map_for(source, map_id)?;
        let id = map.node_at(at)?.id;
        Some(NodeRef { map, id })
    }

    /// The id of the node under a place (0 for none).
    // D2-13514
    pub fn node_id_at<S: PathMaps + ?Sized>(&mut self, source: &mut S, at: &Spot) -> u32 {
        self.node_at(source, at).map_or(0, |n| n.id)
    }

    /// Where a node is.
    // D2-13515
    pub fn query_position<S: PathMaps + ?Sized>(
        &mut self,
        source: &mut S,
        id: u32,
    ) -> Option<Spot> {
        let map = self.map_for(source, path_map_id(id))?;
        map.query_position(id)
    }
}
