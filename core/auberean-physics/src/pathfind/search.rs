//! The A* search: markers (a node reached, by which link, from which marker, at what cost so far
//! and what estimate on), the open heap ordered by cost plus estimate, the table of markers made
//! and the search's counters.

use std::collections::HashMap;

use super::db::{NodeRef, PathMapDb, PathMaps};
use super::finder::Finder;
use super::node::Link;

/// What a search needs from the world beyond its maps.
pub trait SearchHost: PathMaps {
    /// The clock, seconds (the search's timings only).
    fn now(&mut self) -> f64;
    /// A random number between two bounds (either order), the way the client rolls one.
    fn roll_dice(&mut self, lo: f32, hi: f32) -> f32;
}

/// A node reached.
#[derive(Clone, Debug)]
pub struct Marker {
    /// What the heap orders by: cost plus estimate (or their negation, moving away).
    pub f: f32,
    /// The cost so far.
    pub dist: f32,
    /// The estimate on.
    pub h: f32,
    /// The marker it was reached from (an index into the pool).
    pub prev: Option<usize>,
    pub node: NodeRef,
    /// The link it was reached by.
    pub link: Option<Link>,
    /// Its place in the heap (0: not in it).
    pub index: usize,
}

/// The open markers: a binary heap counted from 1, least first.
// D2-13489
#[derive(Clone, Debug)]
pub struct OpenHeap {
    /// Marker indices; slot 0 unused.
    pub data: Vec<usize>,
}

impl Default for OpenHeap {
    fn default() -> Self {
        Self { data: vec![0] }
    }
}

impl OpenHeap {
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len() - 1
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&mut self) {
        self.data.truncate(1);
    }

    fn put(&mut self, m: &mut [Marker], at: usize, which: usize) {
        self.data[at] = which;
        m[which].index = at;
    }

    /// Sinks the marker at `i` below any child that orders strictly before it, the left child
    /// first.
    // D2-13492
    pub fn heapify(&mut self, m: &mut [Marker], mut i: usize) {
        let n = self.len();
        loop {
            let l = 2 * i;
            let r = l | 1;
            let mut least = i;
            if l <= n && m[self.data[l]].f < m[self.data[i]].f {
                least = l;
            }
            if r <= n && m[self.data[r]].f < m[self.data[least]].f {
                least = r;
            }
            if least == i {
                return;
            }
            let (a, b) = (self.data[i], self.data[least]);
            self.put(m, i, b);
            self.put(m, least, a);
            i = least;
        }
    }

    /// Adds a marker, raised above every parent that does not order strictly before it.
    // D2-13493
    pub fn insert(&mut self, m: &mut [Marker], which: usize) -> bool {
        self.data.push(which);
        let mut i = self.len();
        let f = m[which].f;
        while i > 1 {
            let parent = self.data[i >> 1];
            if m[parent].f < f {
                break;
            }
            self.put(m, i, parent);
            i >>= 1;
        }
        self.put(m, i, which);
        true
    }

    /// Raises the marker at `i` above every parent that does not order strictly before it;
    /// answers whether it moved.
    // D2-13494
    pub fn heapify_up(&mut self, m: &mut [Marker], mut i: usize) -> bool {
        let mut moved = false;
        while i > 1 {
            let p = i >> 1;
            if m[self.data[p]].f < m[self.data[i]].f {
                break;
            }
            let (a, b) = (self.data[p], self.data[i]);
            self.put(m, p, b);
            self.put(m, i, a);
            moved = true;
            i = p;
        }
        moved
    }

    /// Takes the least marker off: the last takes its place and sinks. Kept as the client does
    /// it: the last marker is given place 1 before the count drops, so a marker taken off as the
    /// only one open keeps place 1, and is not opened again if it is later reached more cheaply.
    pub fn pop(&mut self, m: &mut [Marker]) -> Option<usize> {
        if self.is_empty() {
            return None;
        }
        let top = self.data[1];
        m[top].index = 0;
        let last = self.data[self.len()];
        self.put(m, 1, last);
        self.data.pop();
        self.heapify(m, 1);
        Some(top)
    }
}

/// The markers, kept between searches and handed out again from the start.
#[derive(Clone, Debug, Default)]
pub struct MarkerStore {
    pub pool: Vec<Marker>,
    pub index: usize,
}

impl MarkerStore {
    /// The next marker, for `node`: one from an earlier search keeps everything but its node; a
    /// new one starts with no link and out of the heap.
    // D2-13500
    pub fn get_new_marker(&mut self, node: NodeRef) -> usize {
        if self.index < self.pool.len() {
            let i = self.index;
            self.pool[i].node = node;
            self.index += 1;
            return i;
        }
        self.pool.push(Marker {
            f: 0.0,
            dist: 0.0,
            h: 0.0,
            prev: None,
            node,
            link: None,
            index: 0,
        });
        self.index = self.pool.len();
        self.pool.len() - 1
    }
}

/// A search's counters and timings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchStats {
    pub num_enqueues: i32,
    pub num_dequeues: i32,
    pub num_updates: i32,
    pub nodes_visited: i32,
    pub nodes_left: i32,
    pub path_length: i32,
    pub path_cost: f32,
    pub min_path_cost: f32,
    pub max_path_cost: f32,
    pub connections_searched: i32,
    pub connections_visited: i32,
    pub connections_left: i32,
    pub connections_rejected: i32,
    pub time_enqueue: f64,
    pub time_dequeue: f64,
    pub time_update: f64,
    pub time_add: f64,
    pub time_find: f64,
    pub time_clear: f64,
    pub time_findpath: f64,
    pub time_min_findpath: f64,
    pub time_max_findpath: f64,
    pub time_ave_findpath: f64,
    pub time_sum_findpath: f64,
    pub time_sum_enqueue: f64,
    pub time_sum_dequeue: f64,
    pub time_sum_update: f64,
    pub time_sum_add: f64,
    pub time_sum_find: f64,
    pub time_sum_clear: f64,
    pub time_sum_other: f64,
    pub findpaths: i32,
    pub num_finds: i32,
    pub num_adds: i32,
    pub time_interval: f64,
    pub time_min_interval: f64,
    pub time_max_interval: f64,
    pub time_ave_interval: f64,
    pub time_sum_interval: f64,
    pub t1: f64,
    pub t2: f64,
}

impl SearchStats {
    /// A search starts: counted, and the time since the last one ended noted.
    // D2-13488
    pub fn record_begin(&mut self, now: f64) {
        let before = self.findpaths;
        self.t1 = now;
        self.findpaths = before.wrapping_add(1);
        if self.t2 != 0.0 {
            let d = self.t1 - self.t2;
            self.time_interval = d;
            self.time_sum_interval += d;
            if self.time_min_interval == 0.0 || d < self.time_min_interval {
                self.time_min_interval = d;
            }
            if self.time_max_interval < d {
                self.time_max_interval = d;
            }
            self.time_ave_interval = self.time_sum_interval / f64::from(before);
        }
    }

    /// A search ends: its time, and each part's time, added in.
    // D2-13491
    pub fn record_end(&mut self, now: f64) {
        self.t2 = now;
        let d = now - self.t1;
        self.time_findpath = d;
        self.time_sum_findpath += d;
        if self.time_min_findpath == 0.0 || d < self.time_min_findpath {
            self.time_min_findpath = d;
        }
        if self.time_max_findpath < d {
            self.time_max_findpath = d;
        }
        self.time_ave_findpath = self.time_sum_findpath / f64::from(self.findpaths);
        self.time_sum_enqueue += self.time_enqueue;
        self.time_sum_dequeue += self.time_dequeue;
        self.time_sum_update += self.time_update;
        self.time_sum_add += self.time_add;
        self.time_sum_find += self.time_find;
        self.time_sum_clear += self.time_clear;
        self.time_sum_other += d
            - (self.time_clear
                + self.time_dequeue
                + self.time_update
                + self.time_add
                + self.time_find
                + self.time_enqueue);
    }

    /// The last search's own counts and times cleared (the running sums kept).
    // D2-13503
    pub fn clear(&mut self) {
        self.nodes_visited = 0;
        self.nodes_left = 0;
        self.path_length = 0;
        self.path_cost = 0.0;
        self.min_path_cost = 0.0;
        self.max_path_cost = 0.0;
        self.connections_searched = 0;
        self.connections_visited = 0;
        self.connections_left = 0;
        self.connections_rejected = 0;
        self.time_enqueue = 0.0;
        self.time_dequeue = 0.0;
        self.time_update = 0.0;
        self.time_add = 0.0;
        self.time_find = 0.0;
        self.num_enqueues = 0;
        self.num_dequeues = 0;
        self.num_updates = 0;
        self.num_adds = 0;
        self.num_finds = 0;
        self.time_clear = 0.0;
        self.time_findpath = 0.0;
        self.time_interval = 0.0;
    }
}

/// What a search answers: a way found for a solution asked for, or none.
pub mod outcome {
    pub const FOUND: u32 = 1;
    /// The request names no start node.
    pub const NO_START: u32 = 3;
    /// The request wants the end reached and names no end node.
    pub const NO_END: u32 = 2;
    pub const NOT_FOUND: u32 = 100;
}

/// The search.
#[derive(Clone, Debug, Default)]
pub struct AStar {
    pub queue: OpenHeap,
    /// The marker made for each node, by node id.
    pub table: HashMap<u32, usize>,
    pub pool: MarkerStore,
    pub stats: SearchStats,
}

impl AStar {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The marker made for a node this search, by the node's id.
    // D2-13490
    pub fn meta_find(&mut self, node: &NodeRef, host: &mut dyn SearchHost) -> Option<usize> {
        let t0 = host.now();
        let found = self.table.get(&node.id).copied();
        let t1 = host.now();
        self.stats.num_finds = self.stats.num_finds.wrapping_add(1);
        self.stats.time_find += t1 - t0;
        found
    }

    // D2-13495
    pub fn meta_enqueue(&mut self, which: usize, host: &mut dyn SearchHost) -> bool {
        let t0 = host.now();
        let ok = self.queue.insert(&mut self.pool.pool, which);
        let t1 = host.now();
        self.stats.num_enqueues = self.stats.num_enqueues.wrapping_add(1);
        self.stats.time_enqueue += t1 - t0;
        ok
    }

    // D2-13496
    pub fn meta_dequeue(&mut self, host: &mut dyn SearchHost) -> Option<usize> {
        let t0 = host.now();
        let m = self.queue.pop(&mut self.pool.pool);
        let t1 = host.now();
        self.stats.num_dequeues = self.stats.num_dequeues.wrapping_add(1);
        self.stats.time_dequeue += t1 - t0;
        m
    }

    /// A marker's order changed: one out of the heap goes in, one in it rises (or else sinks).
    // D2-13497
    pub fn meta_update(&mut self, which: usize, host: &mut dyn SearchHost) -> bool {
        let t0 = host.now();
        let at = self.pool.pool[which].index;
        let ok = if at == 0 {
            self.queue.insert(&mut self.pool.pool, which)
        } else {
            if !self.queue.heapify_up(&mut self.pool.pool, at) {
                self.queue.heapify(&mut self.pool.pool, at);
            }
            true
        };
        let t1 = host.now();
        self.stats.num_updates = self.stats.num_updates.wrapping_add(1);
        self.stats.time_update += t1 - t0;
        ok
    }

    /// Files a new marker under its node, counting the links it opens.
    // D2-13498
    pub fn meta_add(&mut self, which: usize, host: &mut dyn SearchHost) -> bool {
        let t0 = host.now();
        let id = self.pool.pool[which].node.id;
        self.table.insert(id, which);
        let t1 = host.now();
        self.stats.num_adds = self.stats.num_adds.wrapping_add(1);
        self.stats.time_add += t1 - t0;
        let links = self.pool.pool[which].node.node().links.len();
        self.stats.connections_visited = self
            .stats
            .connections_visited
            .wrapping_add(i32::try_from(links).unwrap_or(i32::MAX));
        true
    }

    /// Reaches `to` from marker `from` by `link`: a node not reached before gets a marker with
    /// the cost so far plus the link's cost and the finder's estimate, and opens; a node reached
    /// before is re-reached only for a strictly lower cost.
    // D2-13501
    pub fn relax(
        &mut self,
        finder: &mut dyn Finder,
        from: usize,
        to: NodeRef,
        link: &Link,
        host: &mut dyn SearchHost,
    ) {
        let dest = link.dest_in_map.then(|| to.node());
        let cost = finder.cost(link, dest, host);
        let dist = cost + self.pool.pool[from].dist;
        match self.meta_find(&to, host) {
            None => {
                let m = self.pool.get_new_marker(to);
                let est = {
                    let node = self.pool.pool[m].node.clone();
                    finder.estimate(node.node(), host)
                };
                let mk = &mut self.pool.pool[m];
                mk.prev = Some(from);
                mk.link = Some(*link);
                mk.dist = dist;
                mk.h = est;
                finder.update_marker(mk);
                self.meta_enqueue(m, host);
                self.meta_add(m, host);
            }
            Some(m) => {
                if dist < self.pool.pool[m].dist {
                    let mk = &mut self.pool.pool[m];
                    mk.link = Some(*link);
                    mk.dist = dist;
                    finder.update_marker(mk);
                    mk.prev = Some(from);
                    self.meta_update(m, host);
                }
            }
        }
    }

    /// Searches from the finder's start node until the heap empties, more nodes than the finder
    /// allows have been taken, or the finder says a node taken ends it; then the finder's
    /// solutions are set up from the markers it kept. A link back to the node a marker came from
    /// is not followed; a link to a node of another map loads that map. Answers
    /// [`outcome::FOUND`] when any solution was found, [`outcome::NOT_FOUND`] when none, or the
    /// finder's own complaint about its request.
    // D2-13502
    #[allow(clippy::cast_possible_wrap)]
    pub fn find_path(
        &mut self,
        finder: &mut dyn Finder,
        db: &mut PathMapDb,
        host: &mut dyn SearchHost,
    ) -> u32 {
        let bad = finder.verify_data();
        if bad != 0 {
            return bad;
        }
        let t = host.now();
        self.stats.record_begin(t);
        let Some(start) = finder.base().start_node.clone() else {
            return outcome::NO_START;
        };
        let s = self.pool.get_new_marker(start);
        {
            let mk = &mut self.pool.pool[s];
            finder.set_start_marker(mk, host);
        }
        self.meta_enqueue(s, host);
        self.meta_add(s, host);
        let max = finder.base().max_nodes;
        let mut taken: u32 = 0;
        let mut cur = self.meta_dequeue(host);
        while let Some(m) = cur {
            taken = taken.wrapping_add(1);
            if max < taken || finder.check_node(&self.pool.pool[m], m) {
                break;
            }
            let here = self.pool.pool[m].node.clone();
            let links = here.node().links.clone();
            for link in &links {
                self.stats.connections_searched = self.stats.connections_searched.wrapping_add(1);
                let to = if link.dest_in_map {
                    here.sibling(link.dest_id)
                } else {
                    db.get_map(host, super::ids::path_map_id(link.dest_id))
                        .and_then(|map| {
                            map.node(link.dest_id)?;
                            Some(NodeRef {
                                map,
                                id: link.dest_id,
                            })
                        })
                };
                let Some(to) = to else {
                    continue;
                };
                if let Some(p) = self.pool.pool[m].prev {
                    if self.pool.pool[p].node.same(&to) {
                        continue;
                    }
                }
                if finder.is_acceptable(here.node(), link, to.node()) {
                    self.relax(finder, m, to.clone(), link, host);
                    self.meta_find(&to, host);
                } else {
                    self.stats.connections_rejected =
                        self.stats.connections_rejected.wrapping_add(1);
                }
            }
            cur = self.meta_dequeue(host);
        }
        finder.setup_solutions(&self.pool.pool);
        self.stats.nodes_visited = i32::try_from(self.table.len()).unwrap_or(i32::MAX);
        let t0 = host.now();
        self.queue.clear();
        self.pool.index = 0;
        self.table.clear();
        let t1 = host.now();
        self.stats.time_clear += t1 - t0;
        self.stats.connections_left =
            (db.num_links() as i32).wrapping_sub(self.stats.connections_visited);
        self.stats.nodes_left = (db.num_nodes() as i32).wrapping_sub(self.stats.nodes_visited);
        let t = host.now();
        self.stats.record_end(t);
        if finder.base().has_any_solutions() {
            outcome::FOUND
        } else {
            outcome::NOT_FOUND
        }
    }
}
