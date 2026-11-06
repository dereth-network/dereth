//! Divergence: V466
//! Source loot retains six groups and a required versioned policy through packs and overlays.
//! Fixture: original synthetic loot inputs and the shared SQL overlay fixture.

use crate::support::overlay_fixture::*;
use empyrean_content::{
    loot::{DeathLootProfile, GroupedTreasureDeath, SlotGroup, SourceLootPolicy},
    WorldContent,
};

fn policy() -> SourceLootPolicy {
    SourceLootPolicy::from_json(include_bytes!(
        "../../../../../fixtures/source-loot/synthetic.json"
    ))
    .unwrap()
}
fn row() -> GroupedTreasureDeath {
    GroupedTreasureDeath {
        id: 7,
        treasure_type: 99,
        tier: 1,
        quality: 0.5,
        groups: [SlotGroup {
            chance: 50,
            min: 1,
            max: 2,
        }; 6],
    }
}
fn packed() -> Vec<u8> {
    WorldContent {
        grouped_treasure: vec![row()],
        loot_policies: vec![policy()],
        ..Default::default()
    }
    .to_pack([0; 16])
    .unwrap()
    .0
}
fn sql_record(table: &str, id: u32, value: &impl serde::Serialize) -> String {
    let json = serde_json::to_string(value)
        .unwrap()
        .replace('\\', "\\\\")
        .replace('\'', "\\'");
    format!("INSERT INTO {table} (id,profile) VALUES ({id},'{json}');")
}
fn extension() -> Vec<u8> {
    format!("CREATE TABLE empyrean_grouped_treasure (id int unsigned NOT NULL,profile mediumtext NOT NULL,PRIMARY KEY(id));CREATE TABLE empyrean_loot_policy (id int unsigned NOT NULL,profile mediumtext NOT NULL,PRIMARY KEY(id));{}{}",sql_record("empyrean_loot_policy",1,&policy()),sql_record("empyrean_grouped_treasure",99,&row())).into_bytes()
}

#[test]
fn source_profiles_round_trip_without_ace_placeholders() {
    let p = Pack::from_bytes(packed()).unwrap();
    p.verify_hash().unwrap();
    assert_eq!(
        p.header().flags,
        empyrean_content::pack::format::FLAG_INDEX_SORTED
            | empyrean_content::pack::format::FLAG_SOURCE_LOOT
    );
    assert_eq!(p.count(TableId::TREASURE_DEATH), 0);
    let db = PackContent::new(p);
    assert_eq!(db.get_grouped_treasure(99), Some(row()));
    assert!(db.get_cached_death_treasure(99).is_none());
    assert!(matches!(
        db.get_death_loot_profile(99),
        Some(DeathLootProfile::Ac1999(..))
    ));
    assert!(Arc::ptr_eq(
        &db.get_source_loot_policy().unwrap(),
        &db.get_source_loot_policy().unwrap()
    ));
}
#[test]
fn unsupported_or_incomplete_source_policies_are_rejected() {
    let mut v = serde_json::to_value(policy()).unwrap();
    v["schema"] = 2.into();
    assert!(SourceLootPolicy::from_json(&serde_json::to_vec(&v).unwrap()).is_err());
    v["schema"] = 1.into();
    v["pools"]["gems"][0]["entries"][0]["newField"] = true.into();
    assert!(SourceLootPolicy::from_json(&serde_json::to_vec(&v).unwrap()).is_err());
    let content = WorldContent {
        grouped_treasure: vec![row()],
        ..Default::default()
    };
    assert!(content.to_pack([0; 16]).is_err());
    let mut p = policy();
    p.tiers.get_mut(&1).unwrap().max_spell_level = 7;
    assert!(p.validate().is_err());
}
#[test]
fn capability_and_table_presence_must_agree() {
    let mut bytes = packed();
    bytes[0x10..0x14].copy_from_slice(&1u32.to_le_bytes());
    assert!(Pack::from_bytes(bytes).is_err());
    let (mut bytes, _) = WorldContent::default().to_pack([0; 16]).unwrap();
    bytes[0x10..0x14].copy_from_slice(&5u32.to_le_bytes());
    assert!(Pack::from_bytes(bytes).is_err());
}
#[test]
fn overlay_edits_are_live_and_policy_cannot_be_orphaned_or_downgraded() {
    let dir = tmp("source-loot");
    {
        let (db, o) = open(&dir);
        o.import_sql(&extension(), "loot.sql").unwrap();
        assert_eq!(db.get_grouped_treasure(99), Some(row()));
        let before = db.get_source_loot_policy().unwrap();
        let mut after = policy();
        after.tiers.get_mut(&1).unwrap().chance_of_spells = 0.75;
        let sql = format!(
            "DELETE FROM empyrean_loot_policy WHERE id=1;{}",
            sql_record("empyrean_loot_policy", 1, &after)
        );
        o.import_sql(sql.as_bytes(), "policy.sql").unwrap();
        assert_eq!(
            db.get_source_loot_policy().unwrap().tiers[&1].chance_of_spells,
            0.75
        );
        assert_eq!(before.tiers[&1].chance_of_spells, 0.0);
        assert!(o
            .import_sql(
                b"DELETE FROM empyrean_loot_policy WHERE id=1;",
                "orphan.sql"
            )
            .is_err());
        assert!(db.get_source_loot_policy().is_some());
        o.import_sql(b"CREATE TABLE recipe_requirements_vital (id int unsigned NOT NULL,health int unsigned NOT NULL,stamina int unsigned NOT NULL,mana int unsigned NOT NULL,PRIMARY KEY(id));INSERT INTO recipe_requirements_vital VALUES(5,1,0,0);","recipe.sql").unwrap();
    }
    let (db, _) = open(&dir);
    assert_eq!(
        db.get_source_loot_policy().unwrap().tiers[&1].chance_of_spells,
        0.75
    );
    let conn = rusqlite::Connection::open(dir.join("overlay.sqlite")).unwrap();
    let format: String = conn
        .query_row("SELECT value FROM meta WHERE key='format'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(format, empyrean_content::overlay::file::FORMAT_SOURCE_LOOT);
}
/// An overlay whose records lost the policy (changed outside the import, which refuses that)
/// gives the grouped profile no loot instead of stopping the world on the next death.
#[test]
fn a_grouped_profile_without_its_policy_makes_no_loot_and_does_not_panic() {
    let dir = tmp("source-loot-no-policy");
    {
        let (db, o) = open(&dir);
        o.import_sql(&extension(), "loot.sql").unwrap();
        assert!(matches!(
            db.get_death_loot_profile(99),
            Some(DeathLootProfile::Ac1999(..))
        ));
    }
    let conn = rusqlite::Connection::open(dir.join("overlay.sqlite")).unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO records (tbl, key, data) VALUES (?1, 1, NULL)",
        [TableId::LOOT_POLICY.0],
    )
    .unwrap();
    drop(conn);
    let (db, _) = open(&dir);
    assert_eq!(db.get_grouped_treasure(99), Some(row()));
    assert!(db.get_source_loot_policy().is_none());
    assert!(db.get_death_loot_profile(99).is_none());
}

#[test]
fn identical_typed_inputs_produce_identical_content() {
    assert_eq!(packed(), packed());
    let mut p = policy();
    p.files_sha256.insert("profile.json".into(), "1".repeat(64));
    let changed = WorldContent {
        grouped_treasure: vec![row()],
        loot_policies: vec![p],
        ..Default::default()
    }
    .to_pack([0; 16])
    .unwrap()
    .0;
    assert_ne!(
        Pack::from_bytes(packed()).unwrap().header().content_hash,
        Pack::from_bytes(changed).unwrap().header().content_hash
    );
}
