//! Behaviour: none (tooling: what the world editor's model browser calls each model and files it under, over the retail files)

use dereth_assets::Decode;
use dereth_primitives::DataId;
use lyceum_foundry::edit::Session;
use lyceum_foundry::models::{Category, ModelNames, Placements};

use super::sample::sample;

const HOLTBURG: u16 = 0xA9B4;

/// The town the tests name: only Holtburg's landblock is in one.
fn town(lb: u16) -> Option<String> {
    (lb == HOLTBURG).then(|| "Holtburg".to_owned())
}

/// Three server drudges drawn with the setup the sample's guide is drawn with, and a door.
const SERVER: &str = "\
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (90001, 'drudgeskulker', 10, '2005-02-09 10:00:00')
     , (90002, 'drudgeprowler', 10, '2005-02-09 10:00:00')
     , (90003, 'drudgerobber', 10, '2005-02-09 10:00:00')
     , (90004, 'doorplain', 19, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (90001, 1, 'Drudge Skulker'), (90002, 1, 'Drudge Prowler'), (90003, 1, 'Drudge Robber')
     , (90004, 1, 'Door');
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (90001, 1, 0x020007DD), (90002, 1, 0x020007DD), (90003, 1, 0x020007DD)
     , (90004, 1, 0x02000DD2);
";

const DRUDGE: DataId = DataId(0x0200_07DD);

#[test]
fn a_model_is_named_and_filed_by_its_weenies_its_scenery_or_where_the_world_places_it() {
    let (_dir, world) = sample("pack-models");
    let mut s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    s.learn_weenies(SERVER);
    let names = ModelNames::read(s.base(), s.region(), &s.weenies_by_setup(), &town);
    let is = |id: u32, c: Category, label: &str| {
        let id = DataId(id);
        assert_eq!(
            (names.category(id), names.label(id).as_deref()),
            (c, Some(label)),
            "{:#010X}",
            id.raw()
        );
    };
    // Weenies: the word most of their names share (the sample's guide is one of the four), and
    // most of them creatures; a part of their setup is named by it.
    is(DRUDGE.raw(), Category::Creature, "Drudge (4 weenies)");
    let setup = dereth_assets::geometry::Setup::decode_payload(
        DRUDGE,
        &s.base().read_portal(DRUDGE).unwrap(),
    )
    .unwrap();
    is(
        setup.parts[0].raw(),
        Category::Part,
        "Part of Drudge (4 weenies)",
    );
    is(0x0200_0DD2, Category::Door, "Door");
    // Where the world places it: a building, a piece outdoors, a thing mostly inside.
    is(0x0100_0C1E, Category::Building, "Holtburg building");
    is(
        0x0200_0279,
        Category::BuildingPiece,
        "Holtburg building piece",
    );
    is(0x0200_01BD, Category::Furniture, "Holtburg furniture");
    // Scenery, by the first terrain (in the region's order) it grows on and its shape.
    is(0x0200_02D8, Category::Tree, "Grassland scenery: tree");
    is(0x0200_07A3, Category::Plant, "Grassland scenery: plant");
    is(0x0200_05C9, Category::Rock, "Barren rock scenery: rock");
    // The filter reads the id, the names, the category, the towns and the tags.
    assert!(names.matches(DRUDGE, "SKULKER", &[]));
    assert!(names.matches(DRUDGE, "07dd", &[]) && names.matches(DRUDGE, "0x020007DD", &[]));
    assert!(!names.matches(DRUDGE, "olthoi", &[]));
    assert!(names.matches(DataId(0x0200_02D8), "tree", &[]));
    assert!(names.matches(DataId(0x0200_02D8), "lush grass", &[]));
    assert!(names.matches(DataId(0x0100_0C1E), "holtburg", &[]));
    assert!(names.matches(DataId(0x0100_0C1E), "inn", &["Inn".into()]));
    assert!(!names.matches(DataId(0x0100_0C1E), "inn", &[]));
}

#[test]
fn placements_kept_in_a_file_read_back_as_they_were() {
    let (dir, world) = sample("pack-placements");
    let s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    let p = Placements::read(s.base(), s.region(), &town);
    let text = p.to_text();
    assert_eq!(Placements::from_text(&text).as_ref(), Some(&p));
    let file = dir.path().join("cache").join("placements.tsv");
    p.save(&file).unwrap();
    assert_eq!(Placements::load(&file).as_ref(), Some(&p));
    // Another layout's file is not read as one.
    assert_eq!(
        Placements::from_text(&text.replacen("placements 1", "placements 0", 1)),
        None
    );
    // The same names either way.
    let weenies = s.weenies_by_setup();
    assert_eq!(
        ModelNames::new(&Placements::load(&file).unwrap(), &weenies),
        ModelNames::new(&p, &weenies)
    );
}

#[test]
fn a_terrains_scenes_list_the_models_they_grow_and_each_is_that_terrains_scenery() {
    let (_dir, world) = sample("pack-scenes");
    let s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    let region = s.region().unwrap();
    let names = ModelNames::read(s.base(), Some(region), &s.weenies_by_setup(), &town);
    // Grassland, terrain 1: as many scene values as its list holds, some growing nothing.
    let scenes = lyceum_foundry::models::scenery_of(s.base(), region, 1);
    assert_eq!(scenes.len(), region.terrain_types[1].scene_types.len());
    assert!(scenes.iter().any(Vec::is_empty) && scenes.iter().any(|m| !m.is_empty()));
    for m in scenes.iter().flatten() {
        let u = names.get(*m).unwrap();
        assert!(u.scenery.iter().any(|t| t == "Grassland"), "{m:?}: {u:?}");
    }
    // Every tree that grows on grassland is in some scene of it.
    assert!(scenes.iter().flatten().any(|m| *m == DataId(0x0200_02D8)));
    assert!(lyceum_foundry::models::scenery_of(s.base(), region, 999).is_empty());
}
