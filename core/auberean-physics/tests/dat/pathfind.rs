//! The path data of the September 2013 world: every path map in the cell files (a landblock's
//! `0xFFFD`) and every obstacle description in the portal file (`0x38000000` on), each read.

use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::require_target_install;
use auberean_dat::{cell_type, type_of_id, DatFile, Reader, RecordType};
use auberean_physics::pathfind::map::PathMap;
use auberean_physics::pathfind::obstacle::ObstacleShape;

/// Behaviour: pathfind.record.the-world-holds-no-path-maps
#[test]
fn the_world_holds_no_path_maps() {
    let dir = require_target_install();
    let (mut maps, mut obstacles, mut cell_files) = (0, 0, 0);
    for (name, path) in install_dats(&dir).expect("list the install") {
        let dat = DatFile::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let cell_file = matches!(name, DatName::Cell(_));
        cell_files += usize::from(cell_file);
        for e in dat.entries() {
            let bytes = || dat.read_entry(e).expect("read the record");
            if cell_file && cell_type(e.id) == Some(RecordType::PATH_MAP) {
                let b = bytes();
                let mut r = Reader::new(&b);
                PathMap::read(&mut r, e.id).unwrap_or_else(|x| panic!("{:08X}: {x}", e.id));
                r.finish().expect("every byte");
                maps += 1;
            }
            if name == DatName::Portal && type_of_id(e.id) == Some(RecordType::OBSTACLE_DESC) {
                let b = bytes();
                let mut r = Reader::new(&b);
                ObstacleShape::read(&mut r).unwrap_or_else(|x| panic!("{:08X}: {x}", e.id));
                r.finish().expect("every byte");
                obstacles += 1;
            }
        }
    }
    assert!(cell_files > 0, "the install has its cell files");
    assert_eq!(
        (maps, obstacles),
        (0, 0),
        "path maps and obstacle descriptions"
    );
}
