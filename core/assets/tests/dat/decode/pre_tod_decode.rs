//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! The February 2005 dat set (before Throne of Destiny) decodes in its own record layouts into the
//! same values the later layouts give: the tables a server reads, the indoor geometry, and every
//! record of the census.
//! Fixture: the February 2005 portal and cell files (`DERETH_TEST_PRETOD_DAT_DIR`).

use std::collections::BTreeMap;

use dereth_assets::{
    exhaustive_decode, CharGen, Decode, EnvCell, Environment, GfxObj, SkillTable, SpellTable,
    XpTable,
};
use dereth_dat::{ContainerEra, DbType, RetailDatStore};
use dereth_primitives::DataId;

fn store() -> RetailDatStore {
    dereth_dat::testing::open_pre_tod_store_or_fail()
}

fn read<T: Decode>(s: &RetailDatStore, id: u32) -> T {
    let bytes = s.read_portal(DataId(id)).expect("present");
    T::decode_payload_in(ContainerEra::PreTod, DataId(id), &bytes)
        .unwrap_or_else(|e| panic!("{id:#010X}: {e}"))
}

/// 126 character levels, their XP a widened `u32`; the other curves' last values are the later
/// table's.
#[test]
fn the_february_2005_xp_table_has_126_levels() {
    let xp: XpTable = read(&store(), 0x0E00_0018);
    assert_eq!(xp.level_xp.len(), 127);
    assert_eq!(xp.level_credits.len(), 127);
    assert_eq!(xp.level_xp[1], 0);
    assert_eq!(xp.level_xp[126], 4_286_609_098);
    assert_eq!(xp.attribute_xp.len(), 191);
    assert_eq!(xp.attribute_xp[190], 4_019_438_644);
    assert_eq!(xp.vital_xp.len(), 197);
    assert_eq!(xp.trained_xp.len(), 209);
    assert_eq!(xp.specialized_xp.len(), 227);
    assert!(xp.level_xp.windows(2).all(|w| w[0] <= w[1]));
}

/// 3,737 spells in the later per-spell layout, with no spell sets; every component slot
/// de-obfuscates to a component the component table has.
#[test]
fn the_february_2005_spell_table_has_3737_spells_and_no_sets() {
    let s = store();
    let t: SpellTable = read(&s, 0x0E00_000E);
    assert_eq!(t.spells.len(), 3737);
    assert!(t.spellsets.is_empty());
    assert_eq!(t.spells[&1].name, "Strength Other I");
    assert_eq!(t.spells[&292].name, "Axe Mastery Other I");
    let comps: dereth_assets::SpellComponentTable = read(&s, 0x0E00_000F);
    for (id, spell) in &t.spells {
        for c in &spell.comps {
            assert!(
                comps.components.contains_key(c),
                "spell {id} names component {c}"
            );
        }
    }
}

/// The skill table carries the pre-2013 skills: Axe through Unarmed Combat, and Salvaging.
#[test]
fn the_february_2005_skill_table_has_the_old_weapon_skills() {
    let t: SkillTable = read(&store(), 0x0E00_0004);
    for (id, name) in [
        (1, "Axe"),
        (2, "Bow"),
        (11, "Sword"),
        (13, "Unarmed Combat"),
        (40, "Salvaging"),
    ] {
        assert_eq!(t.skills[&id].name, name, "skill {id}");
    }
    for later in [41, 44, 45, 46, 47, 48, 49, 50, 51, 52, 54] {
        assert!(!t.skills.contains_key(&later), "skill {later}");
    }
}

/// A physics script table from February 2005 answers in the later script-type numbering: the
/// types from 30 on were one lower then, so the human body's hide and unhide scripts (the
/// materialize at login) sit under the types the later file lists them under, and a type below
/// 30 is unmoved.
#[test]
fn a_february_2005_physics_script_table_answers_in_the_later_script_types() {
    use dereth_assets::PhysicsScriptTable;
    const HUMAN: u32 = 0x3400_0004;
    const UNHIDE: u32 = 117;
    const HIDE: u32 = 118;
    let old: PhysicsScriptTable = read(&store(), HUMAN);
    let later_store =
        RetailDatStore::open_dir(&dereth_dat::testing::dat_dir()).expect("the end-of-retail dats");
    let bytes = later_store.read_portal(DataId(HUMAN)).expect("present");
    let later = PhysicsScriptTable::decode_payload(DataId(HUMAN), &bytes).expect("decodes");
    let scripts = |t: &PhysicsScriptTable, key: u32| -> Vec<u32> {
        t.script_table[&key].iter().map(|r| r.script_id.0).collect()
    };
    for key in [UNHIDE, HIDE] {
        assert_eq!(
            scripts(&old, key),
            scripts(&later, key),
            "script type {key}"
        );
    }
    assert!(
        !old.script_table.contains_key(&30),
        "type 30 is later than 2005"
    );
    let low: Vec<u32> = old
        .script_table
        .keys()
        .copied()
        .filter(|&k| k < 30)
        .collect();
    assert!(!low.is_empty());
    for key in low {
        assert_eq!(
            scripts(&old, key),
            scripts(&later, key),
            "script type {key}"
        );
    }
}

/// A February 2005 clothing table's sub-palette ranges counted the colours of a 256-colour
/// palette; read into the later count they are the ranges the later file gives the same
/// clothing (a shirt's dye range 40 colours on, 24 long, is entries 320 to 512).
#[test]
fn a_february_2005_clothing_table_dyes_the_ranges_the_later_file_does() {
    use dereth_assets::ClothingTable;
    let old = store();
    let later =
        RetailDatStore::open_dir(&dereth_dat::testing::dat_dir()).expect("the end-of-retail dats");
    let mut compared = 0;
    for id in [0x1000_0001, 0x1000_0002, 0x1000_0004, 0x1000_0006] {
        let o: ClothingTable = read(&old, id);
        let bytes = later.read_portal(DataId(id)).expect("present");
        let l = ClothingTable::decode_payload(DataId(id), &bytes).expect("decodes");
        for (key, template) in &o.palette_templates {
            let Some(later_template) = l.palette_templates.get(key) else {
                continue;
            };
            assert_eq!(
                template.subpalette_effects, later_template.subpalette_effects,
                "{id:#010X} template {key}"
            );
            compared += 1;
        }
    }
    assert!(compared >= 4, "{compared} templates compared");
    let shirt: ClothingTable = read(&old, 0x1000_0001);
    let ranges = &shirt.palette_templates[&1].subpalette_effects[0].ranges;
    assert_eq!((ranges[0].offset, ranges[0].length), (320, 192));
}

/// Six outdoor starter areas, three heritages each with a male and a female, 330 attribute and
/// 50 skill credits, and seven templates.
#[test]
fn the_february_2005_chargen_has_three_heritages_and_six_outdoor_starter_areas() {
    let cg: CharGen = read(&store(), 0x0E00_0002);
    let areas: Vec<(&str, u32)> = cg
        .starter_areas
        .iter()
        .map(|a| (a.name.as_str(), a.locations[0].cell.0))
        .collect();
    assert_eq!(
        areas,
        [
            ("Holtburg South", 0xA9B0_0014),
            ("Holtburg West", 0xA5B4_002A),
            ("Shoushi Southeast", 0xDE51_001D),
            ("Shoushi West", 0xD655_0023),
            ("Yaraq North", 0x7D68_0012),
            ("Yaraq East", 0x8164_000D),
        ]
    );
    let names: BTreeMap<u32, &str> = cg
        .heritage_groups
        .iter()
        .map(|(k, h)| (*k, h.name.as_str()))
        .collect();
    assert_eq!(
        names,
        BTreeMap::from([(1, "Aluvian"), (2, "Gharu'ndim"), (3, "Sho")])
    );
    for h in cg.heritage_groups.values() {
        assert_eq!(
            (h.attribute_credits, h.skill_credits),
            (330, 50),
            "{}",
            h.name
        );
        assert_eq!(h.templates.len(), 7, "{}", h.name);
        assert_eq!(h.sexes[&1].name, "Male");
        assert_eq!(h.sexes[&2].name, "Female");
        assert_eq!(h.sexes[&1].hair_styles.len(), 4);
        assert!(!h.sexes[&2].shirts.is_empty());
    }
    let aluvian = &cg.heritage_groups[&1];
    assert_eq!(aluvian.primary_start_areas, [0, 1]);
    assert_eq!(aluvian.skills, [(4, 0, 4), (19, 0, 2)]);
    assert_eq!(aluvian.templates[1].name, "Bow Hunter");
    assert_eq!(aluvian.templates[1].attributes, [40, 30, 100, 100, 50, 10]);
}

/// Every Environment and GfxObj of the portal file and every EnvCell of the cell file decodes in
/// the older layout with the cursor on the end, and the geometry is whole: every polygon's
/// vertices are in its vertex array and every cell's environment and cell structure exist.
#[test]
fn every_february_2005_environment_gfxobj_and_envcell_decodes_whole() {
    let s = store();
    let mut environments = BTreeMap::new();
    for id in s.ids_of(DbType::Environment) {
        let env: Environment = read(&s, id.raw());
        for cell in &env.cells {
            let n = cell.vertex_array.vertices.len();
            for p in cell.polygons.iter().chain(&cell.physics_polygons) {
                assert!(p.vertex_ids.iter().all(|v| usize::from(*v) < n), "{id}");
            }
        }
        environments.insert(id.raw(), env);
    }
    assert_eq!(environments.len(), 686);

    let gfx = s.ids_of(DbType::GfxObj);
    assert_eq!(gfx.len(), 10_065);
    for id in gfx {
        let g: GfxObj = read(&s, id.raw());
        let n = g.vertex_array.vertices.len();
        for p in &g.physics_polygons {
            assert!(p.vertex_ids.iter().all(|v| usize::from(*v) < n), "{id}");
        }
        // A drawn polygon's surface is one the object lists.
        for p in &g.polygons {
            assert!(p.vertex_ids.iter().all(|v| usize::from(*v) < n), "{id}");
            assert!(
                usize::from(p.pos_surface) < g.surfaces.len() || p.stippling & 1 != 0,
                "{id}"
            );
        }
    }

    let cells = s.ids_of(DbType::Cell);
    assert_eq!(cells.len(), 455_641);
    for id in cells {
        let bytes = s.read_cell(id).expect("present");
        let cell = EnvCell::decode_payload_in(ContainerEra::PreTod, id, &bytes)
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        let env = environments
            .get(&cell.environment.raw())
            .unwrap_or_else(|| panic!("{id} names {}", cell.environment));
        assert!(usize::from(cell.cell_struct) < env.cells.len(), "{id}");
    }
}

/// The census: every record of both files decodes in the older layouts, the quest table and the
/// ids only the older files type (two quality filters and a second region) among them.
#[test]
fn every_february_2005_record_decodes() {
    let r = exhaustive_decode(&store()).expect("the census runs");
    let mut failures: BTreeMap<DbType, (usize, String)> = BTreeMap::new();
    for (id, kind, e) in &r.failures {
        let row = failures.entry(*kind).or_insert((0, format!("{id}: {e}")));
        row.0 += 1;
    }
    assert!(
        failures.is_empty(),
        "decoded {} of {} ({:?}); failures by type: {failures:#?}",
        r.decoded,
        r.entries,
        r.per_type,
    );
    assert!(r.no_decoder.is_empty(), "{:?}", r.no_decoder);
    assert!(r.untyped.is_empty(), "{:?}", r.untyped);
    assert_eq!(r.entries, 51_001 + 524_954);
    assert_eq!(r.decoded, r.entries);
    assert_eq!(r.per_type[&DbType::GfxObj], 10_065);
    assert_eq!(r.per_type[&DbType::RenderSurface], 9_930);
    assert_eq!(r.per_type[&DbType::SurfaceTexture], 5_118);
    assert_eq!(r.per_type[&DbType::Surface], 4_334);
    assert_eq!(r.per_type[&DbType::Cell], 455_641);
    assert_eq!(r.per_type[&DbType::QuestDefDb], 1);
    assert_eq!(r.per_type[&DbType::QualityFilter], 2);
    assert_eq!(r.per_type[&DbType::Region], 2);
}

/// A February 2005 graphics object names no detail record; it reaches one by its own id, with the
/// record type's top byte over its low 24 bits. The male torso's record lists six levels whose
/// nearest is a denser mesh than the torso itself (44 drawing polygons against 16), and the
/// object itself is the second level.
#[test]
fn a_february_2005_gfxobj_reaches_its_degrade_record_by_its_own_id() {
    let s = store();
    let torso: GfxObj = read(&s, 0x0100_004E);
    assert_eq!(torso.flags & 8, 0, "the older record names no degrade id");
    assert_eq!(torso.did_degrade, Some(DataId(0x1100_004E)));
    let info: dereth_assets::motion::GfxObjDegradeInfo = read(&s, 0x1100_004E);
    let levels: Vec<u32> = info.degrades.iter().map(|d| d.gfxobj_id.raw()).collect();
    assert_eq!(
        levels,
        [
            0x0100_1787,
            0x0100_004E,
            0x0100_01A2,
            0x0100_01A0,
            0x0100_01F1,
            0
        ]
    );
    let near: GfxObj = read(&s, 0x0100_1787);
    assert_eq!((near.polygons.len(), torso.polygons.len()), (44, 16));

    // An object with no record of that id still decodes; the id it carries simply names nothing.
    let no_record: GfxObj = read(&s, 0x0100_04B6);
    assert_eq!(no_record.did_degrade, Some(DataId(0x1100_04B6)));
    assert!(s.read_portal(DataId(0x1100_04B6)).is_err());
}
