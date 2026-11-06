//! The path system: the maps held and the search, answering path requests and questions about
//! nodes.
//!
//! The September 2013 client never makes its path system (the one instance it would use is never
//! set) and holds no path map, so the scripts' path calls find nothing there; the system is the
//! server's and the world builder's.

use dereth_primitives::Position;

use super::db::{NodeRef, PathMapDb, PathMaps};
use super::finder::Finder;
use super::node::{NodeData, Spot};
use super::search::{AStar, SearchHost};

/// The maps held and the search.
#[derive(Clone, Debug, Default)]
pub struct Pathing {
    pub db: PathMapDb,
    pub finder: AStar,
    pub dump: bool,
    pub visualize_paths: bool,
    pub smooth: bool,
    pub constrain: bool,
    pub cluster: bool,
}

impl Pathing {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Where a node is, as a position facing north.
    // D2-08819
    pub fn query_position<S: PathMaps + ?Sized>(
        &mut self,
        source: &mut S,
        id: u32,
    ) -> Option<Position> {
        self.db.query_position(source, id).map(|s| s.to_position())
    }

    /// A map as text.
    // D2-08822
    pub fn dump_path_map<S: PathMaps + ?Sized>(source: &mut S, id: u32, out: &mut String) -> bool {
        PathMapDb::dump_path_map(source, id, out)
    }

    /// The search's counters as text: the client writes nothing (the routine that would is
    /// empty).
    #[must_use]
    pub fn dump_search_stats() -> String {
        String::new()
    }

    /// The cache as text: only its heading.
    #[must_use]
    pub fn dump_cache() -> String {
        "PathCache: ".to_owned()
    }

    pub fn path_node<S: PathMaps + ?Sized>(&mut self, source: &mut S, id: u32) -> Option<NodeRef> {
        self.db.node(source, id)
    }

    /// Runs a request: the last search's own counters cleared, the search run, and every map it
    /// loaded let go.
    // D2-13506
    pub fn find_path(&mut self, finder: &mut dyn Finder, host: &mut dyn SearchHost) -> u32 {
        self.finder.stats.clear();
        let r = self.finder.find_path(finder, &mut self.db, host);
        self.db.clear_path_maps();
        r
    }

    /// What the ground at a node is like (the defaults for a node that has none).
    // D2-13507
    pub fn inq_node_data_by_id<S: PathMaps + ?Sized>(
        &mut self,
        source: &mut S,
        id: u32,
    ) -> Option<NodeData> {
        let n = self.db.node(source, id)?;
        Some(n.node().data.unwrap_or_default())
    }

    /// What the ground at the node under a place is like.
    pub fn inq_node_data_by_position<S: PathMaps + ?Sized>(
        &mut self,
        source: &mut S,
        p: &Position,
    ) -> Option<NodeData> {
        let id = self.db.node_id_at(source, &Spot::of(p));
        self.inq_node_data_by_id(source, id)
    }
}
