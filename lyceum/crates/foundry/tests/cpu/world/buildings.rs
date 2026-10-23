//! Behaviour: none (tooling: a retail building's copy and the row that drops it)

use std::collections::BTreeMap;

use dereth_assets::world::{
    BuildInfo, BuildingPortal, CellPortal, EnvCell, LandblockInfo, ObjectEntry,
};
use dereth_primitives::{DataId, Frame, Quat, Vec3};
use lyceum_foundry::buildings::Site;
use lyceum_foundry::cells::carried;
use lyceum_foundry::compile::heading_quat;
use lyceum_foundry::drops::{self, Template};
use lyceum_foundry::source::{self, Furnished, Height, Rotation};

use crate::common::Fixture;

fn site(s: &str) -> Site {
    Site::parse(s).unwrap()
}

#[test]
fn a_site_is_written_as_its_landblock_and_its_place_in_the_buildings() {
    let s = site("3B9C/3");
    assert_eq!((s.landblock, s.index), (0x3B9C, 3));
    assert_eq!(s.to_string(), "3B9C/3");
    for bad in ["3B9C", "3B9/3", "3B9C/x", "ZZZZ/1", ""] {
        assert_eq!(Site::parse(bad), None, "{bad}");
    }
}

/// The landblock file `0102.toml` of the fixture's pack, read.
fn read(f: &Fixture, text: &str) -> Result<source::LandblockEdits, lyceum_foundry::Error> {
    f.world();
    f.write("sample/edits/landblocks/0102.toml", text);
    let pack = source::read_pack(&f.path().join("sample"))?;
    Ok(pack.edits.into_iter().next().unwrap())
}

#[test]
fn an_add_building_row_reads_its_models_templates_and_furniture() {
    let f = Fixture::new("drop-row");
    let e = read(
        &f,
        "[[add-building]]\nname = \"cottage-1\"\nmodel = 0x01002228\ntemplate = \"3B9C/3\"\n\
         at = [61.3, 79.8, \"ground\"]\nheading = 180.0\n\n\
         [[add-building]]\nname = \"mansion-1\"\nmodel = [0x01002286, 0x01002287]\n\
         template = [\"3ED0/0\", \"3ED0/1\"]\nat = [132.0, 108.0, 40.0]\nfurnished = \"bare\"\n\n\
         [[add-building]]\nname = \"cottage-2\"\nmodel = 0x01002228\ntemplate = \"3B9C/3\"\n\
         at = [10.0, 10.0, 1.0]\nfurnished = \"3DC7/1\"\nhouse = \"cottage-2-house\"\n",
    )
    .unwrap();
    assert!(e.touches_statics());
    let [a, b, c] = &e.drops[..] else {
        panic!("{:?}", e.drops)
    };
    assert_eq!(a.item.name, "sample:cottage-1");
    assert_eq!(a.item.models, vec![DataId(0x0100_2228)]);
    assert_eq!(a.item.templates, vec![site("3B9C/3")]);
    assert_eq!(
        (a.item.at.x, a.item.at.y, a.item.at.z),
        (61.3, 79.8, Height::Ground)
    );
    assert_eq!(a.item.rotation, Rotation::Heading(180.0));
    assert_eq!(a.item.furnished, Furnished::Template);
    assert_eq!(b.item.templates, vec![site("3ED0/0"), site("3ED0/1")]);
    assert_eq!(b.item.furnished, Furnished::Bare);
    assert_eq!(b.item.at.z, Height::At(40.0));
    assert_eq!(c.item.furnished, Furnished::Like(vec![site("3DC7/1")]));
    // A house players can buy names the spawn its rooms belong to; a plain building none.
    assert!(a.item.house.is_empty() && b.item.house.is_empty());
    assert_eq!(c.item.house, vec!["sample:cottage-2-house".to_owned()]);
}

#[test]
fn a_set_s_house_names_each_building_s_spawn() {
    let f = Fixture::new("drop-row-house");
    let e = read(
        &f,
        "[[add-building]]\nname = \"mansion-1\"\nmodel = [0x01002286, 0x01002287]\n\
         template = [\"3ED0/0\", \"3ED0/1\"]\nat = [132.0, 108.0, 40.0]\n\
         house = [\"mansion-1-annex-0\", \"mansion-1-house\"]\n",
    )
    .unwrap();
    assert_eq!(
        e.drops[0].item.house,
        vec![
            "sample:mansion-1-annex-0".to_owned(),
            "sample:mansion-1-house".to_owned()
        ]
    );
}

#[test]
fn an_add_dungeon_row_reads_its_template_and_its_house() {
    let f = Fixture::new("dungeon-row");
    let e = read(
        &f,
        "[[add-dungeon]]\nname = \"villa-1-basement\"\ntemplate = 0x0398\n\
         house = \"villa-1-basement-house\"\n\n\
         [[add-dungeon]]\nname = \"crypt\"\ntemplate = 0x0163\n",
    )
    .unwrap();
    assert!(e.touches_statics());
    let [a, b] = &e.dungeon_copies[..] else {
        panic!("{:?}", e.dungeon_copies)
    };
    assert_eq!(
        a.item,
        source::AddDungeon {
            name: "sample:villa-1-basement".into(),
            template: 0x0398,
            house: Some("sample:villa-1-basement-house".into()),
        }
    );
    assert_eq!((b.item.template, b.item.house.as_deref()), (0x0163, None));
    for (text, says) in [
        (
            "[[add-dungeon]]\nname = \"d\"\ntemplate = 0x1FF00\n",
            "is not a landblock",
        ),
        (
            "[[add-dungeon]]\nname = \"d\"\ntemplate = 0x0398\nhouse = \"other:h\"\n",
            "bare symbol",
        ),
        (
            "[[add-building]]\nname = \"d\"\nmodel = 0x01002228\ntemplate = \"3B9C/3\"\n\
             at = [1.0, 2.0, 3.0]\n\n[[add-dungeon]]\nname = \"d\"\ntemplate = 0x0398\n",
            "names them",
        ),
        (
            "[[cell]]\nname = \"d-room-001\"\nenvironment = 0x0D000001\nsurfaces = []\n\
             at = [0.0, 0.0, 0.0]\nportals = []\n\n[[add-dungeon]]\nname = \"d\"\n\
             template = 0x0398\n",
            "named as a room of dungeon",
        ),
    ] {
        let why = read(&f, text).unwrap_err().to_string();
        assert!(why.contains(says), "{text}: {why}");
    }
}

#[test]
fn an_add_building_row_that_does_not_hold_together_is_refused_with_its_line() {
    let f = Fixture::new("drop-row-refused");
    let row =
        |body: &str| format!("[[add-building]]\nname = \"b\"\n{body}\nat = [1.0, 2.0, 3.0]\n");
    for (body, says) in [
        (
            "model = [0x01002286, 0x01002287]\ntemplate = \"3ED0/0\"",
            "one template for each model",
        ),
        (
            "model = 0x01002228\ntemplate = \"3B9C\"",
            "is written \"3B9C/3\"",
        ),
        (
            "model = 0x05000001\ntemplate = \"3B9C/3\"",
            "is not a model",
        ),
        (
            "model = 0x01002228\ntemplate = \"3B9C/3\"\nfurnished = [\"3DC7/1\", \"3DC7/2\"]",
            "one retail building for each template",
        ),
        (
            "model = [0x01002286, 0x01002287]\ntemplate = [\"3ED0/0\", \"3ED0/1\"]\n\
             house = \"b-house\"",
            "the spawn of each building",
        ),
        (
            "model = 0x01002228\ntemplate = \"3B9C/3\"\nhouse = \"b-house@0102\"",
            "bare symbol",
        ),
    ] {
        let why = read(&f, &row(body)).unwrap_err().to_string();
        assert!(why.contains("0102.toml:1") && why.contains(says), "{why}");
    }
    let twice = format!(
        "{}\n{}",
        row("model = 0x01002228\ntemplate = \"3B9C/3\""),
        row("model = 0x01002228\ntemplate = \"3B9C/2\"")
    );
    assert!(read(&f, &twice)
        .unwrap_err()
        .to_string()
        .contains("drops it"));
    // A cell may not take a room's name.
    let cell = format!(
        "[[cell]]\nname = \"b-room-000\"\nenvironment = 0x0D000001\nsurfaces = []\n\
         at = [0.0, 0.0, 0.0]\nportals = []\n\n{}",
        row("model = 0x01002228\ntemplate = \"3B9C/3\"")
    );
    let why = read(&f, &cell).unwrap_err().to_string();
    assert!(why.contains("named as a room of building"), "{why}");
}

fn portal(flags: u16, polygon: u16, to: u32, back: i16) -> CellPortal {
    CellPortal {
        flags,
        polygon_id: polygon,
        other_cell_id: to,
        other_portal_id: back,
    }
}

fn room(n: u16, at: Vec3, portals: Vec<CellPortal>, visible: Vec<u16>) -> EnvCell {
    let id = 0x3B9C_0000 | u32::from(n);
    EnvCell {
        id: DataId(id),
        flags: 1 | 2 | 8,
        cell_id_repeat: id,
        surfaces: vec![DataId(0x0800_0001)],
        environment: DataId(0x0D00_0001),
        cell_struct: 0,
        frame: Frame::new(at, Quat::IDENTITY),
        portals,
        visible_cells: visible,
        static_objects: vec![ObjectEntry {
            id: DataId(0x0200_0001),
            frame: Frame::new(Vec3::new(at.x + 1.0, at.y, at.z), Quat::IDENTITY),
        }],
        restriction_obj: Some(0x73B9_C001),
    }
}

/// A house of two rooms: the first's ways are, in order, one into a cell of a dungeon under it,
/// one to the outdoors (its doorway) and one into the second room.
fn template() -> Template {
    let at = Frame::new(Vec3::new(30.0, 30.0, 10.0), Quat::IDENTITY);
    let rooms = BTreeMap::from([
        (
            0x0105,
            room(
                0x0105,
                Vec3::new(30.0, 32.0, 10.0),
                vec![
                    portal(0, 0, 0x0300, 0),
                    portal(4 | 1, 1, 0xFFFF_FFFF, -1),
                    portal(1, 2, 0x0106, 0),
                ],
                vec![0x0106, 0x0300],
            ),
        ),
        (
            0x0106,
            room(
                0x0106,
                Vec3::new(30.0, 36.0, 10.0),
                vec![portal(1, 0, 0x0105, 2)],
                vec![0x0105],
            ),
        ),
    ]);
    Template {
        site: site("3B9C/0"),
        building: BuildInfo {
            id: DataId(0x0100_2228),
            frame: at,
            num_leaves: 200,
            portals: vec![BuildingPortal {
                flags: 3,
                other_cell_id: 0x0105,
                other_portal_id: 1,
                stab_list: vec![0x0105, 0x0106],
            }],
        },
        rooms,
        ground: Some(10.0),
    }
}

#[test]
fn a_copy_renumbers_its_rooms_ways_and_lists_and_shuts_its_way_into_other_cells() {
    let t = template();
    let to = Frame::new(Vec3::new(100.0, 50.0, 20.0), heading_quat(90.0));
    let c = drops::copy(&t, Some(&t), 0x9C9C, &[0x0120, 0x0121], &to).unwrap();
    assert_eq!(c.shut, 1);
    assert_eq!(c.building.id, DataId(0x0100_2228));
    assert_eq!((c.building.frame, c.building.num_leaves), (to, 200));
    assert_eq!(
        c.building.portals,
        vec![BuildingPortal {
            flags: 3,
            other_cell_id: 0x0120,
            other_portal_id: 0,
            stab_list: vec![0x0120, 0x0121],
        }]
    );
    let (a, b) = (&c.rooms[&0x0120], &c.rooms[&0x0121]);
    assert_eq!((a.id, a.cell_id_repeat), (DataId(0x9C9C_0120), 0x9C9C_0120));
    assert_eq!(
        a.portals,
        vec![portal(4 | 1, 1, 0xFFFF_FFFF, -1), portal(1, 2, 0x0121, 0)]
    );
    assert_eq!(b.portals, vec![portal(1, 0, 0x0120, 1)]);
    assert_eq!(
        (&a.visible_cells, &b.visible_cells),
        (&vec![0x0121], &vec![0x0120])
    );
    // Carried about the building's origin, furnished, and no house's.
    for (n, r) in &c.rooms {
        let old = &t.rooms[&(n - 0x0120 + 0x0105)];
        assert_eq!(r.frame, carried(&old.frame, &t.building.frame, &to));
        assert_eq!(
            r.static_objects[0].frame,
            carried(&old.static_objects[0].frame, &t.building.frame, &to)
        );
        assert_eq!((r.restriction_obj, r.flags), (None, 1 | 2));
    }
    // Bare, the rooms hold nothing.
    let bare = drops::copy(&t, None, 0x9C9C, &[0x0120, 0x0121], &to).unwrap();
    assert!(bare
        .rooms
        .values()
        .all(|r| r.static_objects.is_empty() && r.flags == 1));
    // A count of numbers that is not the rooms' is refused.
    assert!(drops::copy(&t, None, 0x9C9C, &[0x0120], &to).is_err());
}

#[test]
fn a_building_is_refused_outside_its_landblock_and_in_a_land_cell_that_holds_one() {
    let info = LandblockInfo {
        id: DataId(0x0101_FFFE),
        num_cells: 0,
        objects: Vec::new(),
        pack_mask: 0,
        buildings: vec![template().building],
        restrictions: None,
    };
    let at = |x: f32, y: f32| BuildInfo {
        frame: Frame::new(Vec3::new(x, y, 0.0), Quat::IDENTITY),
        ..template().building
    };
    // The land cell next to it, and one of two in the same land cell.
    assert!(drops::check_sites(0x0101, "b", &info, &[at(50.0, 30.0)]).is_ok());
    let why = drops::check_sites(0x0101, "b", &info, &[at(26.0, 47.0)]).unwrap_err();
    assert!(
        why.contains("building 0 (0x01002228)") && why.contains("only one building in each"),
        "{why}"
    );
    let why =
        drops::check_sites(0x0101, "b", &info, &[at(50.0, 30.0), at(60.0, 40.0)]).unwrap_err();
    assert!(why.contains("building 1"), "{why}");
    let why = drops::check_sites(0x0101, "b", &info, &[at(-1.0, 30.0)]).unwrap_err();
    assert!(why.contains("outside landblock 0101"), "{why}");
}

#[test]
fn a_drop_on_the_command_line_names_its_model_place_and_furniture() {
    use lyceum_foundry::script::{add_building, Action};
    let words = |s: &str| s.split(' ').map(str::to_owned).collect::<Vec<_>>();
    let a = add_building(
        &words("01002228 A9B4 60 40.5 90"),
        None,
        Some("3DC7/1"),
        false,
        false,
    )
    .unwrap();
    assert_eq!(
        a,
        Action::AddBuilding {
            model: DataId(0x0100_2228),
            layout: 0,
            furnished: Furnished::Like(vec![site("3DC7/1")]),
            house: true,
            landblock: 0xA9B4,
            x: 60.0,
            y: 40.5,
            heading: 90.0,
        }
    );
    let bare = add_building(&words("0x01002228 A9B4 1 2 3"), Some("1"), None, true, true).unwrap();
    assert!(matches!(
        bare,
        Action::AddBuilding {
            layout: 1,
            furnished: Furnished::Bare,
            house: false,
            ..
        }
    ));
    for (w, layout, furnished, bare, says) in [
        ("01002228 A9B4 1 2", None, None, false, "takes <model>"),
        ("05000001 A9B4 1 2 3", None, None, false, "model id"),
        ("01002228 A9B 1 2 3", None, None, false, "not a landblock"),
        ("01002228 A9B4 1 x 3", None, None, false, "is a number"),
        ("01002228 A9B4 1 2 3", Some("one"), None, false, "a layout"),
        (
            "01002228 A9B4 1 2 3",
            None,
            Some("3DC7/1"),
            true,
            "not both",
        ),
    ] {
        let why = add_building(&words(w), layout, furnished, bare, false).unwrap_err();
        assert!(why.contains(says), "{w}: {why}");
    }
}

#[test]
fn a_door_room_and_a_drop_are_named_by_the_building() {
    assert_eq!(
        drops::room_name("sample:cottage-1", 7),
        "sample:cottage-1-room-007"
    );
    assert_eq!(drops::door_name("cottage-1", 2), "cottage-1-door-2");
    // Its template's height over its ground.
    assert_eq!(template().height(), 0.0);
}
