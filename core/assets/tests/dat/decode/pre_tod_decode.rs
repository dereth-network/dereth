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

/// The census: every record of both files decodes in the older layouts, except the one quest
/// table, a type that ended at Throne of Destiny and that nothing reads.
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
    assert_eq!(r.no_decoder, BTreeMap::from([(DbType::QuestDefDb, 1)]));
    assert_eq!(r.entries, 51_001 + 524_954);
    assert_eq!(
        r.decoded + 1 + r.untyped.len(),
        r.entries,
        "untyped: {:?}",
        r.untyped
    );
    assert_eq!(r.per_type[&DbType::GfxObj], 10_065);
    assert_eq!(r.per_type[&DbType::RenderSurface], 9_930);
    assert_eq!(r.per_type[&DbType::SurfaceTexture], 5_118);
    assert_eq!(r.per_type[&DbType::Surface], 4_334);
    assert_eq!(r.per_type[&DbType::Cell], 455_641);
    // Two tables and a second region whose ids the later type ranges do not name.
    assert_eq!(
        r.untyped,
        [
            DataId(0x0E00_0010),
            DataId(0x0E00_0017),
            DataId(0x130F_0000)
        ]
    );
}
