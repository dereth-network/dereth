//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! The tables whose layouts changed over the life of the game read, in each held capture, into
//! the values its own client read: the October 1999 character generation and spell tables, the
//! spell records' three endings, the four character-generation layouts and the two sky-object
//! layouts of the later files, the quest table, and the ids only the older files type.
//! Fixture: the historical dat captures (`DERETH_TEST_DAT_CAPTURES_DIR`).

use dereth_assets::region::SkyObject;
use dereth_assets::{
    CharGen, Decode, QualityFilter, QuestTable, Region, SpellComponentTable, SpellTable,
};
use dereth_dat::DatFile;
use dereth_primitives::DataId;

/// One file of one capture.
fn capture(folder: &str, name: &str) -> DatFile {
    let files = dereth_dat::testing::dat_captures_or_fail();
    let (_, _, path) = files
        .iter()
        .find(|(c, n, _)| c == folder && n == name)
        .unwrap_or_else(|| panic!("no capture {folder}/{name}"));
    DatFile::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Record `id` of `f`, in the layouts of `f`'s container.
fn read<T: Decode>(f: &DatFile, id: u32) -> T {
    let bytes = f.read(DataId(id)).expect("present");
    T::decode_payload_in(f.era(), DataId(id), &bytes)
        .unwrap_or_else(|e| panic!("{}: {id:#010X}: {e}", f.path().display()))
}

/// Every component a spell names is one the component table of the same capture has: the
/// names and descriptions de-obfuscate to the bytes the component key is made from.
fn assert_components_resolve(f: &DatFile, t: &SpellTable) {
    let comps: SpellComponentTable = read(f, 0x0E00_000F);
    for (id, spell) in &t.spells {
        for c in &spell.comps {
            assert!(
                comps.components.contains_key(c),
                "spell {id} names component {c}"
            );
        }
    }
}

/// The October 1999 retail CD: eighteen starter areas (six per heritage), three heritages of a
/// female and a male each with 330 attribute and 50 skill credits, and eight templates, the last
/// the Warrior the later tables do not have.
#[test]
fn the_october_1999_chargen_has_eighteen_starter_areas_and_eight_templates() {
    let cg: CharGen = read(&capture("1999-10-09", "portal.dat"), 0x0E00_0002);
    assert_eq!(cg.starter_areas.len(), 18);
    assert_eq!(cg.starter_areas[0].name, "Holtburg South");
    assert_eq!(cg.starter_areas[0].locations[0].cell.0, 0xA9B0_0014);
    assert_eq!(cg.starter_areas[17].name, "Al-Arqas North");
    let names: Vec<&str> = cg
        .heritage_groups
        .values()
        .map(|h| h.name.as_str())
        .collect();
    assert_eq!(names, ["Aluvian", "Gharu'ndim", "Sho"]);
    for h in cg.heritage_groups.values() {
        assert_eq!(
            (h.attribute_credits, h.skill_credits),
            (330, 50),
            "{}",
            h.name
        );
        let templates: Vec<&str> = h.templates.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            templates,
            [
                "Adventurer",
                "Archer",
                "Blademaster",
                "Enchanter",
                "Life Mage",
                "Sorcerer",
                "Vagabond",
                "Warrior"
            ],
            "{}",
            h.name
        );
        assert_eq!(h.sexes[&1].name, "Male");
        assert_eq!(h.sexes[&2].name, "Female");
    }
    assert_eq!(
        cg.heritage_groups[&1].primary_start_areas,
        [0, 1, 2, 3, 4, 5]
    );
}

/// The October 1999 spells end after their recovery fields: no display order, target type or
/// per-target mana. Their names carry a terminating NUL in their length, which is not part of the
/// name.
#[test]
fn the_october_1999_spell_table_has_1635_spells_ending_after_the_recovery_fields() {
    let f = capture("1999-10-09", "portal.dat");
    let t: SpellTable = read(&f, 0x0E00_000E);
    assert_eq!(t.spells.len(), 1635);
    assert!(t.spellsets.is_empty());
    let s = &t.spells[&1];
    assert_eq!(s.name, "Strength Other I");
    assert_eq!(
        (s.base_mana, s.meta_spell_type, s.meta_spell_id),
        (10, 1, 1)
    );
    assert_eq!(s.duration, Some((150.0, 0.0, -666.0)));
    assert!(t
        .spells
        .values()
        .all(|s| (s.display_order, s.non_component_target_type, s.mana_mod) == (0, 0, 0)));
    assert_components_resolve(&f, &t);
}

/// From late 2002 a spell ends with its display order; from 2004 also its target type and
/// per-target mana, the ending the later files keep.
#[test]
fn the_spell_records_gain_the_display_order_in_2002_and_the_target_type_in_2004() {
    let f = capture("2002-10-21", "portal.dat");
    let t: SpellTable = read(&f, 0x0E00_000E);
    assert_eq!(t.spells[&1].name, "Strength Other I");
    assert_eq!(t.spells[&1].display_order, 0x11B8);
    assert!(t
        .spells
        .values()
        .all(|s| (s.non_component_target_type, s.mana_mod) == (0, 0)));
    assert_components_resolve(&f, &t);

    let f = capture("2004-01-28", "portal.dat");
    let t: SpellTable = read(&f, 0x0E00_000E);
    let s = &t.spells[&1];
    assert_eq!(
        (s.display_order, s.non_component_target_type, s.mana_mod),
        (0x123E, 0x10, 0)
    );
    assert_components_resolve(&f, &t);
}

/// The first spell tables of the later files end after the spells, with no spell-set table.
#[test]
fn the_2005_and_2006_spell_tables_of_the_later_files_have_no_spell_sets() {
    for (folder, spells) in [("2005-06-02", 3743), ("2006-01-12", 3796)] {
        let t: SpellTable = read(&capture(folder, "client_portal.dat"), 0x0E00_000E);
        assert_eq!(t.spells.len(), spells, "{folder}");
        assert!(t.spellsets.is_empty(), "{folder}");
    }
    let t: SpellTable = read(&capture("2017-02-18", "client_portal.dat"), 0x0E00_000E);
    assert!(!t.spellsets.is_empty());
}

/// The character-generation table of the later files in each of its four layouts: the 2005 one
/// with its help-text and description ids, the 2009 one without a sex's scale and tables, the
/// 2010 one without a hair style's alternate setup, and the current one.
#[test]
fn each_later_chargen_layout_reads_its_heritages_sexes_and_hair_styles() {
    for (folder, heritages, sexes, hair_styles, tables, alternate) in [
        ("2005-06-02", 4, 8, 56, false, false),
        ("2009-04-22", 4, 8, 56, false, false),
        ("2010-06-09", 10, 20, 707, true, false),
        ("2013-09-06", 13, 26, 869, true, true),
    ] {
        let cg: CharGen = read(&capture(folder, "client_portal.dat"), 0x0E00_0002);
        let all_sexes: Vec<_> = cg
            .heritage_groups
            .values()
            .flat_map(|h| h.sexes.values())
            .collect();
        let styles: Vec<_> = all_sexes.iter().flat_map(|s| &s.hair_styles).collect();
        assert_eq!(
            (cg.heritage_groups.len(), all_sexes.len(), styles.len()),
            (heritages, sexes, hair_styles),
            "{folder}"
        );
        assert_eq!(
            all_sexes.iter().all(|s| s.motion_table.0 != 0),
            tables,
            "{folder}: sex tables"
        );
        assert_eq!(
            styles.iter().any(|h| h.alternate_setup.0 != 0),
            alternate,
            "{folder}: alternate setups"
        );
        assert_eq!(cg.heritage_groups[&1].name, "Aluvian", "{folder}");
    }
}

/// A sky object has a particle-script id only from August 2012: the July 2012 region reads with
/// eight-word sky objects and no rainy days, the August one with nine, every rainy day with a
/// sky object that names a particle script.
#[test]
fn the_sky_objects_gain_a_particle_script_in_august_2012() {
    let july: Region = read(&capture("2012-07-27", "client_portal.dat"), 0x1300_0000);
    let august: Region = read(&capture("2012-08-19", "client_portal.dat"), 0x1300_0000);
    let groups = |r: &Region| -> Vec<(String, Vec<SkyObject>)> {
        r.sky_info
            .as_ref()
            .expect("a sky")
            .day_groups
            .iter()
            .map(|g| (g.day_name.clone(), g.sky_objects.clone()))
            .collect()
    };
    let (july, august) = (groups(&july), groups(&august));
    assert_eq!((july.len(), august.len()), (20, 20));
    assert!(july
        .iter()
        .all(|(name, objects)| name != "Rainy"
            && objects.iter().all(|o| o.default_pes_object.0 == 0)));
    assert!(august
        .iter()
        .filter(|(name, _)| name == "Rainy")
        .all(|(_, objects)| objects.iter().any(|o| o.default_pes_object.0 != 0)));
}

/// The quest table, present until Throne of Destiny: 190 quests in February 2005, the first the
/// fletching bestower's, solvable once, named "Fletcher".
#[test]
fn the_february_2005_quest_table_has_190_quests() {
    let t: QuestTable = read(&capture("2005-02", "portal.dat"), 0x0E00_001B);
    assert_eq!(t.quests.len(), 190);
    let q = &t.quests[0];
    assert_eq!(
        (
            q.key.as_str(),
            q.min_delta,
            q.max_solves,
            q.full_name.as_str()
        ),
        ("BestowerFletching1", 0, 1, "Fletcher")
    );
}

/// The older files' two quality filters are the later files' two, less the 64-bit integer list
/// those files added: the smaller lists the same properties and secondary attributes.
#[test]
fn the_older_quality_filters_are_the_later_ones_without_the_int64_list() {
    let old: QualityFilter = read(&capture("2005-02", "portal.dat"), 0x0E00_0017);
    let later_store = dereth_dat::RetailDatStore::open_dir(&dereth_dat::testing::dat_dir())
        .expect("the end-of-retail dats");
    let bytes = later_store
        .read_portal(DataId(0x0E01_0002))
        .expect("present");
    let later = QualityFilter::decode_payload(DataId(0x0E01_0002), &bytes).expect("decodes");
    assert_eq!(old.property_lists, later.property_lists);
    assert_eq!(old.attribute_lists, later.attribute_lists);
    assert!(old.property_lists[1].is_empty());
    let big: QualityFilter = read(&capture("1999-10-09", "portal.dat"), 0x0E00_0010);
    assert!(!big.property_lists[0].is_empty());
}

/// The second region of the older files, which the client loads in place of the first under one
/// of its display settings, is a whole region of the same world.
#[test]
fn the_older_files_second_region_is_a_region_of_the_same_world() {
    for folder in ["1999-10-09", "2005-02"] {
        let f = capture(folder, "portal.dat");
        let first: Region = read(&f, 0x1300_0000);
        let second: Region = read(&f, 0x130F_0000);
        assert_eq!(second.region_name, "Lands of Dereth", "{folder}");
        assert_eq!(second.region_name, first.region_name, "{folder}");
        assert_eq!(
            second.land_defs.land_height_table, first.land_defs.land_height_table,
            "{folder}"
        );
    }
}
