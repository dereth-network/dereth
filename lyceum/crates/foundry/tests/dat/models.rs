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
    let scenes = s.scenery(1);
    assert_eq!(scenes.len(), region.terrain_types[1].scene_types.len());
    assert!(scenes.iter().any(Vec::is_empty) && scenes.iter().any(|m| !m.is_empty()));
    for m in scenes.iter().flatten() {
        let u = names.get(*m).unwrap();
        assert!(u.scenery.iter().any(|t| t == "Grassland"), "{m:?}: {u:?}");
    }
    // Every tree that grows on grassland is in some scene of it.
    assert!(scenes.iter().flatten().any(|m| *m == DataId(0x0200_02D8)));
    assert!(s.scenery(999).is_empty());
}

/// A scene the world's import changes grows what the import says in the scene brush's list, not
/// what the player's own files say: a converted land's own scenery is its import's.
#[test]
fn a_scene_the_import_changes_grows_the_imports_models() {
    use dereth_assets::world::Scene;
    use dereth_dat::ModernDat;
    use lyceum_foundry::compile::{self, Records, Staged};

    let (dir, world) = sample("import-scenes");
    let dats = dereth_dat::testing::dat_dir();
    let before = Session::open(&world, &dats, None).unwrap();
    let base = before.base();
    let region = before.region().unwrap();
    let grassland = before.scenery(1);
    let descs = region.scene_info.as_deref().unwrap_or_default();
    let scenes_of = |st: i32| {
        usize::try_from(st)
            .ok()
            .and_then(|i| descs.get(i))
            .map(|d| d.scenes.clone())
            .unwrap_or_default()
    };
    let (value, st) = region.terrain_types[1]
        .scene_types
        .iter()
        .enumerate()
        .find(|(v, st)| !grassland[*v].is_empty() && !scenes_of(**st).is_empty())
        .map(|(v, st)| (v, *st))
        .expect("a grassland scene that grows something");
    let target = scenes_of(st)[0];
    // Another scene, whose first model grassland's scene value doesn't grow.
    let (donor, model) = descs
        .iter()
        .flat_map(|d| d.scenes.iter().copied())
        .find_map(|id| {
            let b = base.read_portal(id)?;
            let first = base
                .decode::<Scene>(id, &b)
                .ok()?
                .objects
                .into_iter()
                .map(|o| o.obj_id)
                .find(|m| m.raw() != 0)?;
            (!grassland[value].contains(&first)).then_some((id, first))
        })
        .expect("a scene growing something else");
    let mut bytes = base.read_portal(donor).unwrap();
    bytes[..4].copy_from_slice(&target.raw().to_le_bytes());
    let mut records = Records::new();
    records.entry(ModernDat::Portal).or_default().insert(
        target.raw(),
        Staged {
            bytes: bytes.into(),
            version: 3,
        },
    );
    let overlay = dir.path().join("import");
    compile::write_records(
        &records,
        &std::collections::BTreeMap::new(),
        base,
        "an-import",
        &overlay,
    )
    .unwrap();
    drop(before);
    let text = std::fs::read_to_string(&world).unwrap();
    let imports = format!(
        "imports = [{:?}]\npacks",
        overlay.display().to_string().replace('\\', "/")
    );
    std::fs::write(&world, text.replacen("packs", &imports, 1)).unwrap();

    let s = Session::open(&world, &dats, None).unwrap();
    let now = s.scenery(1);
    assert!(now[value].contains(&model), "{model:?} in {:?}", now[value]);
    assert!(!grassland[value].contains(&model));
}
