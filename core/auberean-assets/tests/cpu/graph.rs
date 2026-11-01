//! The data graph a world-building tool may install: what goes in as records load.

use std::sync::{Arc, Mutex};

use auberean_assets::database::{graphs_type, DataGraph};
use auberean_assets::landblock::LandblockInfo;
use auberean_assets::strings::{StringEntry, StringTable};
use auberean_assets::tables::{Categories, IdMap};
use auberean_assets::Database;
use auberean_dat::RecordType;

#[derive(Default)]
struct Seen {
    nodes: Vec<u32>,
    edges: Vec<(u32, u32)>,
}

#[derive(Clone, Default)]
struct Graph(Arc<Mutex<Seen>>);

impl DataGraph for Graph {
    fn add_node(&mut self, id: u32) {
        self.0.lock().unwrap().nodes.push(id);
    }
    fn add_edge(&mut self, from: u32, to: u32) {
        self.0.lock().unwrap().edges.push((from, to));
    }
}

/// A record that loads goes into the installed data graph as a node with an edge to each record
/// it needs; a cell file's record, or one of the region's and the sounds' types (24 in all, id
/// maps and categories among them), does not; a category record links to the categories' enum
/// map only when a caller fills the graph with it.
///
/// Behaviour: data.database.data-graph
#[test]
fn a_loaded_record_is_a_node_with_an_edge_to_each_record_it_needs() {
    let db = Database::new(Vec::new());
    let g = Graph::default();
    db.set_data_graph(Some(Box::new(g.clone())));
    let table = StringTable {
        entries: vec![StringEntry {
            token: 1,
            table: 0x2500_0002,
            ..StringEntry::default()
        }],
        ..StringTable::default()
    };
    let _t = db.put(0x2500_0001, table).expect("not loaded yet");
    let _l = db.put(0x1234_FFFE, LandblockInfo::default());
    let _m = db.put(RecordType::ID_MAP.base_id() + 1, IdMap::default());
    {
        let seen = g.0.lock().unwrap();
        assert_eq!(seen.nodes, [0x2500_0001]);
        assert_eq!(seen.edges, [(0x2500_0001, 0x2500_0002)]);
    }
    assert!(graphs_type(RecordType::STRING_TABLE.0));
    assert!(
        !graphs_type(RecordType::LANDBLOCK_INFO.0),
        "a cell file's type"
    );
    assert!(!graphs_type(RecordType::ID_MAP.0));
    assert!(!graphs_type(RecordType::CATEGORIES.0));
    let mut direct = g.clone();
    let cat = RecordType::CATEGORIES.base_id() + 1;
    let _c = db.put(cat, Categories::default());
    assert_eq!(
        g.0.lock().unwrap().nodes.len(),
        1,
        "categories load ungraphed"
    );
    db.fill_categories_graph(cat, &mut direct);
    assert_eq!(
        g.0.lock().unwrap().edges.last(),
        Some(&(cat, 0)),
        "no enum map in an empty database"
    );
    assert!(db.take_data_graph().is_some());
    let _u = db.put(0x2500_0003, StringTable::default());
    assert_eq!(
        g.0.lock().unwrap().nodes.len(),
        1,
        "no graph, nothing added"
    );
}

/// The client's database never saves a record and never reads only in the foreground; its
/// caches have no last words. Category records keep names compared without case.
///
/// Behaviour: data.database.no-saving
#[test]
fn the_database_saves_nothing_and_a_category_name_ignores_case() {
    let db = Database::new(Vec::new());
    assert!(!db.save_to_disk(0x2500_0001));
    assert!(!db.blocking());
    db.ask_for_last_words();
    let mut c = Categories {
        mapping: vec![("Weapons".into(), 7)],
    };
    assert_eq!(c.category("WEAPONS"), Some(7));
    let mut copy = Categories::default();
    c.copy_into(&mut copy);
    assert_eq!(copy, c);
    c.clear();
    assert!(c.mapping.is_empty());
}
