//! Finding a way across the world: the path maps (a graph of nodes over each landblock's floors,
//! with a tree of planes per cell that finds the node under a point), and the search over them
//! that a path request runs.
//!
//! A request names where it starts and where it ends (as nodes, or as places whose nodes are
//! looked up), how big the body is, what it must avoid (water, solid ground, steep slopes,
//! leaving a landblock) and which answers it wants (reaching the end, the nearest or farthest
//! the search got from it, or getting a distance away). The search is A* from the start: each
//! step takes the open node with the least cost so far plus estimate, and opens the nodes its
//! links reach that the request accepts.

pub mod bsp;
pub mod db;
pub mod finder;
pub mod map;
pub mod node;
pub mod obstacle;
pub mod route;
pub mod search;
pub mod system;

/// How the ids of a landblock's path data are made.
pub mod ids {
    /// The landblock part of an id.
    pub const BLOCK_MASK: u32 = 0xFFFF_0000;
    /// The cell number a landblock's path map is filed under.
    pub const PATH_MAP_CELL: u32 = 0xFFFD;
    /// A landblock's first tree id.
    pub const FIRST_BSP_ID: u32 = 1;
    /// A landblock's first land node id.
    pub const FIRST_LAND_NODE_ID: u32 = 1;

    /// The first id of the nodes a map inserts, after the land's own: two per land polygon of a
    /// block `side_polygons` polygons on a side.
    #[must_use]
    pub fn first_insert_node_id(side_polygons: u32) -> u32 {
        FIRST_LAND_NODE_ID.wrapping_add(side_polygons.wrapping_mul(side_polygons).wrapping_mul(2))
    }

    /// The path map a node or cell belongs to: its landblock's.
    // D2-13439
    #[must_use]
    pub fn path_map_id(id: u32) -> u32 {
        (id & BLOCK_MASK) | PATH_MAP_CELL
    }
}
