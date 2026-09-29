//! ACE: Source/ACE.DatLoader/DatManager.cs::DatManager
//! DatManager over FakeDats: up-front tables, AddRetiredSkills with ACE formulas (and ACE's throw
//! on a duplicate), decode-once cache, missing/mismatched files are None, iteration loop, file
//! list and cache count.
//! Fixture: synthetic dat records and locally constructed geometry.

use std::sync::Arc;

use empyrean_dat::fake::sample;
use empyrean_dat::file_types::{Iteration, SkillTable, XpTable};
use empyrean_dat::{file_id, DatDatabaseType, DatManagerError, FakeDats};

#[test]
fn the_up_front_tables_come_from_the_fake() {
    let dats = FakeDats::new()
        .with_xp_table(sample::xp_table())
        .with_char_gen(sample::char_gen())
        .with_skill_table(sample::skill_table())
        .with_spell_table(sample::spell_table())
        .with_spell_components_table(sample::spell_components_table())
        .build()
        .expect("fake dats build");
    let portal = dats.portal_dat();
    assert_eq!(portal.xp_table().level_xp, vec![0, 0, 1_000, 2_500]);
    assert_eq!(
        portal.char_gen().starter_areas[0].locations[0].cell,
        sample::START_CELL
    );
    assert!(portal.spell_table().spells.contains_key(&1));
    assert!(
        portal.try_taboo_table().is_none(),
        "not inserted, so absent"
    );
    let missing = portal.missing_tables();
    assert!(
        missing.contains(&"TabooTable") && missing.contains(&"RegionDesc"),
        "{missing:?}"
    );
    assert!(!missing.contains(&"XpTable"));
    assert!(
        dats.high_res_dat().is_none(),
        "a fake without high-res content has no high-res dat"
    );
}

#[test]
#[should_panic(expected = "the dats have no TabooTable (0x0E00001E)")]
fn an_absent_table_panics_with_its_name_on_the_plain_accessor() {
    let dats = FakeDats::new().build().expect("an empty fake builds");
    let _ = dats.portal_dat().taboo_table();
}

/// `SkillTable.AddRetiredSkills`: ten skills, each `new SkillFormula(attr1, attr2, divisor)` which
/// sets `X = 1`, `Z = divisor`, and leaves `W = Y = 0`.
#[test]
fn initialize_adds_the_retired_skills_with_aces_formulas() {
    let dats = FakeDats::new()
        .with_skill_table(sample::skill_table())
        .build()
        .expect("build");
    let skills = &dats.portal_dat().skill_table().skills;
    // (skill, attr1, attr2, divisor) from SkillTable.cs; Strength 1, Quickness 3, Coordination 4.
    let expected = [
        (1, 1, 4, 3),
        (2, 4, 0, 2),
        (3, 4, 0, 2),
        (4, 3, 4, 3),
        (5, 1, 4, 3),
        (9, 1, 4, 3),
        (10, 1, 4, 3),
        (11, 1, 4, 3),
        (12, 4, 0, 2),
        (13, 1, 4, 3),
    ];
    for (skill, a1, a2, z) in expected {
        let f = &skills
            .get(&skill)
            .unwrap_or_else(|| panic!("retired skill {skill}"))
            .formula;
        assert_eq!(
            (f.w, f.x, f.y, f.z, f.attr1, f.attr2),
            (0, 1, 0, z, a1, a2),
            "skill {skill}"
        );
    }
    assert_eq!(
        skills.len(),
        11,
        "the sample's Melee Defense plus ten retired skills"
    );
    // The cache holds the edited table: ReadFromDat<SkillTable> answers the same object.
    let cached = dats
        .portal_dat()
        .read_from_dat::<SkillTable>(file_id::SKILL_TABLE)
        .expect("cached");
    assert_eq!(cached.skills.len(), 11);
}

#[test]
fn a_retired_skill_already_in_the_table_fails_initialize_as_ace_throws() {
    let mut t = sample::skill_table();
    let axe = t.skills[&6].clone();
    t.skills.insert(1, axe);
    let err = FakeDats::new()
        .with_skill_table(t)
        .build()
        .expect_err("Dictionary.Add throws");
    assert!(
        matches!(
            err,
            DatManagerError::RetiredSkills(empyrean_dat::AceThrow::DuplicateKey(1))
        ),
        "{err:?}"
    );
}

#[test]
fn read_from_dat_decodes_once_and_shares_the_object() {
    let dats = FakeDats::new()
        .with_xp_table(sample::xp_table())
        .build()
        .expect("build");
    let a = dats
        .portal_dat()
        .read_from_dat::<XpTable>(file_id::XP_TABLE)
        .expect("present");
    let b = dats
        .portal_dat()
        .read_from_dat::<XpTable>(file_id::XP_TABLE)
        .expect("present");
    assert!(Arc::ptr_eq(&a, &b), "the second read is the cached object");
    assert!(Arc::ptr_eq(
        &a,
        dats.portal_dat().try_xp_table().expect("up front")
    ));
}

#[test]
fn a_missing_file_and_a_type_mismatch_answer_none() {
    let dats = FakeDats::new()
        .with_xp_table(sample::xp_table())
        .build()
        .expect("build");
    let portal = dats.portal_dat();
    assert!(
        portal.read_from_dat::<XpTable>(0x0E00_0099).is_none(),
        "ACE's new T() arm"
    );
    assert!(
        portal
            .read_from_dat::<SkillTable>(file_id::XP_TABLE)
            .is_none(),
        "an id cached as XpTable is not a SkillTable (ACE's InvalidCastException)"
    );
    assert!(portal.get_reader_for_file(0x0E00_0099).is_none());
}

/// ACE's `Iteration.Unpack`: `TotalIterations`, then `(consecutive, starting)` pairs while the
/// running count, starting at the total and adding each `consecutive`, stays positive.
#[test]
fn iteration_decodes_from_raw_bytes_with_aces_loop() {
    let mut bytes = Vec::new();
    for v in [2072_i32, -2072, 1] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    let dats = FakeDats::new()
        .with_raw(DatDatabaseType::Portal, Iteration::FILE_ID, bytes)
        .build()
        .expect("build");
    let it = dats
        .portal_dat()
        .read_from_dat::<Iteration>(Iteration::FILE_ID)
        .expect("decodes");
    assert_eq!(it.total_iterations, 2072);
    assert_eq!(it.ints, vec![(1, -2072)]);
    assert_eq!(dats.portal_dat().iteration(), 2072);
    assert_eq!(
        dats.cell_dat().iteration(),
        0,
        "no iteration record reads as 0, as in ACE"
    );
}

#[test]
fn iteration_with_a_short_payload_is_not_a_panic() {
    let dats = FakeDats::new()
        .with_raw(
            DatDatabaseType::Cell,
            Iteration::FILE_ID,
            5_i32.to_le_bytes().to_vec(),
        )
        .with_iteration(DatDatabaseType::Language, 994)
        .build()
        .expect("build");
    assert_eq!(dats.cell_dat().iteration(), 0, "undecodable reads as 0");
    assert_eq!(dats.language_dat().iteration(), 994);
}

#[test]
fn the_file_list_and_the_cache_count_are_aces_all_files_and_file_cache() {
    let dats = FakeDats::new()
        .with_flat_landblock(dereth_primitives::LandblockId(0xA9B4), 10)
        .with_raw(DatDatabaseType::Cell, 0xA9B4_0100, vec![1, 2, 3])
        .build()
        .expect("build");
    let cell = dats.cell_dat();
    assert_eq!(cell.all_files(), vec![0xA9B4_0100, 0xA9B4_FFFF]);
    assert_eq!(cell.all_files_count(), 2);
    assert!(cell.contains_file(0xA9B4_FFFF));
    assert_eq!(cell.get_reader_for_file(0xA9B4_0100), Some(vec![1, 2, 3]));
    let before = cell.file_cache_count();
    let _ = cell.read_from_dat::<empyrean_dat::file_types::CellLandblock>(0xA9B4_FFFF);
    assert_eq!(cell.file_cache_count(), before + 1);
}
