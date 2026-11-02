//! Pathfinding: the path maps (a graph of nodes over each landblock's floors) and the search a
//! path request runs over them.
//!
//! A row claims what the September 2013 client's path code does with a request or a path map.

use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "pathfind.link.a-wide-body-keeps-to-the-wide-part-of-a-doorway",
        says: "A link's doorway is narrowed for a body: each end narrower than the body is drawn \
               in toward the middle by how much wider the body is relative to how much wider the \
               middle is; when the middle is too narrow too, only the half beside a wide enough \
               end is kept.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PATHFIND-PORTAL"),
        station: "auberean-physics::cpu::paths::pathfind::a_wide_body_keeps_to_the_wide_part_of_a_doorway",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "pathfind.record.a-path-map-keeps-places-to-a-tenth-and-every-node-passable",
        says: "A path map record keeps places to a tenth of a metre and widths and heights as size \
               classes; a link's distance is kept whole, its width is the widest of its three, \
               and every node read is passable whatever its record says.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PATHFIND-RECORD"),
        station: "auberean-physics::cpu::paths::pathfind::a_path_map_keeps_places_to_a_tenth_and_every_node_passable",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "pathfind.record.an-obstacle-keeps-each-box-as-its-two-corners",
        says: "An obstacle description keeps each box of its footprint, and the box of what lies \
               beneath it, as its two corners, six floats with nothing before them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PATHFIND-OBSTACLE-BOX"),
        station: "auberean-physics::cpu::paths::pathfind::an_obstacle_keeps_each_box_as_its_two_corners",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "pathfind.record.the-world-holds-no-path-maps",
        says: "The September 2013 world holds no path map and no obstacle description, so the \
               client has nothing to search.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PATHFIND-NONE"),
        station: "auberean-physics::dat::pathfind::the_world_holds_no_path_maps",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "pathfind.route.a-way-walks-through-each-doorway-facing-the-next",
        says: "A way found is walked through the middle of the doorway of each link it takes, \
               ending at the request's own end, each place facing the next and the last facing \
               the way it was reached.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PATHFIND-ROUTE"),
        station: "auberean-physics::cpu::paths::pathfind::a_way_walks_through_each_doorway_facing_the_next",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "pathfind.search.a-search-ends-after-the-nodes-it-allows",
        says: "A search that has taken more nodes than its request allows (300 by default) ends \
               without an answer.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PATHFIND-LIMIT"),
        station: "auberean-physics::cpu::paths::pathfind::a_search_ends_after_the_nodes_it_allows",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "pathfind.search.a-search-keeps-a-dry-body-out-of-water",
        says: "By default a search does not step from dry ground into water, and goes around; \
               allowed in, it pays double for each step into water.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PATHFIND-WATER"),
        station: "auberean-physics::cpu::paths::pathfind::a_search_keeps_a_dry_body_out_of_water",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "pathfind.search.a-search-takes-the-least-distance-way",
        says: "A path request from a start node to an end node finds the way of least summed link \
               distance, searching A* with the straight-line distance to the end as estimate.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PATHFIND-SEARCH"),
        station: "auberean-physics::cpu::paths::pathfind::a_search_takes_the_least_distance_way",
        tier: Tier::Cpu,
    },
];
