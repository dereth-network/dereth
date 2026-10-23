//! Divergence: V453
//! Offline playtest character templates.
use empyrean_command::empyrean::playtest_character::{replace, Template};
use empyrean_entity::{
    enums::{PropertyAttribute, PropertyInstanceId, PropertyInt, Skill, SkillAdvancementClass},
    Biota,
};
use empyrean_store::{adapter::BiotaConverter, MemShard, ShardDatabase};

fn template() -> Template {
    serde_json::from_str(r#"{"version":1,"level":50,"attributes":[100,90,80,70,60,50],"skills":[{"name":"WarMagic","training":"Specialized","ranks":200}],"equipment":[]}"#).unwrap()
}

#[test]
fn templates_round_trip_and_reject_unknown_fields_and_versions() {
    let t = template();
    let t: Template = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
    t.validate(275).unwrap();
    assert_eq!(t.skills[0].ranks, 200);
    let mut t = t;
    t.version = 2;
    assert!(t.validate(275).is_err());
    assert!(serde_json::from_str::<Template>(
        r#"{"version":1,"level":1,"attributes":[10,10,10,10,10,10],"account_id":5}"#
    )
    .is_err());
}

#[test]
fn invalid_levels_attributes_training_and_duplicate_skills_are_refused() {
    let mut t = template();
    t.level = 276;
    assert!(t.validate(275).is_err());
    t.level = 1;
    t.attributes[0] = 0;
    assert!(t.validate(275).is_err());
    t.attributes[0] = 10;
    t.skills[0].training = "Untrained".into();
    assert!(t.validate(275).is_err());
    t.skills[0].training = "Specialized".into();
    t.skills.push(t.skills[0].clone());
    assert!(t.validate(275).is_err());
    t.skills.pop();
    t.skills[0].name = "Unknown".into();
    assert!(t.validate(275).is_err());
}

#[test]
fn applying_statistics_keeps_identity_and_uses_named_attributes_and_skill_ranks() {
    let mut player = Biota {
        id: 0x50000001,
        weenie_class_id: 1,
        ..Default::default()
    };
    template().apply_statistics(&mut player, 123456).unwrap();
    assert_eq!(player.id, 0x50000001);
    assert_eq!(player.get_property(PropertyInt::Level), Some(50));
    let attributes = player.properties_attribute.unwrap();
    assert_eq!(
        attributes
            .get(&PropertyAttribute::Coordination)
            .unwrap()
            .init_level,
        80
    );
    assert_eq!(
        attributes
            .get(&PropertyAttribute::Quickness)
            .unwrap()
            .init_level,
        70
    );
    let skills = player.properties_skill.unwrap();
    let war = skills.get(&Skill::WarMagic).unwrap();
    assert_eq!(
        (war.sac, war.level_from_pp),
        (SkillAdvancementClass::Specialized, 200)
    );
}

#[test]
fn replacing_equipment_removes_nested_possessions_but_keeps_another_character() {
    let mut db = MemShard::default();
    let mut player = Biota {
        id: 0x50000001,
        ..Default::default()
    };
    let mut other = Biota {
        id: 0x50000002,
        ..Default::default()
    };
    let mut bag = Biota {
        id: 0x80000001,
        weenie_type: empyrean_entity::enums::WeenieType::Container,
        ..Default::default()
    };
    bag.set_property(PropertyInstanceId::Container, player.id);
    let mut item = Biota {
        id: 0x80000002,
        ..Default::default()
    };
    item.set_property(PropertyInstanceId::Container, bag.id);
    for b in [&mut player, &mut other, &mut bag, &mut item] {
        assert!(db.save_biota(b, false));
    }
    let mut new = Biota {
        id: 0x80000003,
        ..Default::default()
    };
    new.set_property(PropertyInstanceId::Wielder, player.id);
    replace(&mut db, player.id, &[player.clone(), new.clone()]).unwrap();
    assert!(db.get_biota(bag.id, false).is_none());
    assert!(db.get_biota(item.id, false).is_none());
    assert!(db.get_biota(other.id, false).is_some());
    let restored =
        BiotaConverter::convert_to_entity_biota(&db.get_biota(new.id, false).unwrap(), false);
    assert_eq!(
        restored.get_property(PropertyInstanceId::Wielder),
        Some(player.id)
    );
}

#[test]
fn a_restore_cannot_overwrite_an_object_owned_by_someone_else() {
    let mut db = MemShard::default();
    let mut player = Biota {
        id: 0x50000001,
        ..Default::default()
    };
    let mut item = Biota {
        id: 0x80000001,
        ..Default::default()
    };
    assert!(db.save_biota(&mut player, false));
    assert!(db.save_biota(&mut item, false));
    assert!(replace(&mut db, player.id, &[player.clone(), item.clone()]).is_err());
    assert!(db.get_biota(item.id, false).is_some());
}

#[test]
fn a_failed_equipment_write_restores_the_character_and_previous_inventory() {
    let mut db = MemShard::default();
    let mut player = Biota {
        id: 0x50000001,
        ..Default::default()
    };
    player.set_property(PropertyInt::Level, 10);
    let mut old = Biota {
        id: 0x80000001,
        ..Default::default()
    };
    old.set_property(PropertyInstanceId::Container, player.id);
    assert!(db.save_biota(&mut player, false));
    assert!(db.save_biota(&mut old, false));
    let mut next = Biota {
        id: 0x80000002,
        ..Default::default()
    };
    next.set_property(PropertyInstanceId::Container, player.id);
    player.set_property(PropertyInt::Level, 100);
    db.fail_writes_of(next.id, true);
    assert!(replace(&mut db, player.id, &[player.clone(), next.clone()]).is_err());
    assert!(db.get_biota(old.id, false).is_some());
    assert!(db.get_biota(next.id, false).is_none());
    let kept =
        BiotaConverter::convert_to_entity_biota(&db.get_biota(player.id, false).unwrap(), false);
    assert_eq!(kept.get_property(PropertyInt::Level), Some(10));
}

#[test]
fn a_failed_commit_keeps_the_previous_character() {
    let mut db = MemShard::default();
    let mut player = Biota {
        id: 0x50000001,
        ..Default::default()
    };
    player.set_property(PropertyInt::Level, 10);
    assert!(db.save_biota(&mut player, false));
    db.fail_next_commits(1);
    player.set_property(PropertyInt::Level, 100);
    assert!(replace(&mut db, player.id, &[player.clone()]).is_err());
    let kept =
        BiotaConverter::convert_to_entity_biota(&db.get_biota(player.id, false).unwrap(), false);
    assert_eq!(kept.get_property(PropertyInt::Level), Some(10));
}

#[test]
fn the_character_command_is_console_only_and_missing_characters_are_not_created_by_reset() {
    let _table = super::command_manager::initialized_table();
    let mut ts = empyrean_testkit::TestServer::new();
    empyrean_command::command_manager::start_console_capture();
    empyrean_command::empyrean::playtest_character::handle(
        &mut ts.world,
        None,
        &[
            "apply".into(),
            "Missing".into(),
            "unused.json".into(),
            "unused.sqlite".into(),
        ],
    );
    let output = empyrean_command::command_manager::take_console_output();
    assert_eq!(
        output,
        ["Playtest character unchanged: character must exist and be logged out"]
    );
    let row = empyrean_command::empyrean::playtest_character::command_handlers().remove(0);
    assert_eq!(
        row.attribute.flags,
        empyrean_command::command_handler_flag::CommandHandlerFlag::ConsoleInvoke
    );
}

#[test]
fn backups_restore_stats_and_equipment_and_never_overwrite_an_earlier_backup() {
    use empyrean_command::empyrean::playtest_character::backup;
    use empyrean_store::SqliteShard;
    let mut serial = 0;
    let dir = loop {
        let p =
            std::env::temp_dir().join(format!("playtest-backup-{}-{serial}", std::process::id()));
        match std::fs::create_dir(&p) {
            Ok(()) => break p,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => serial += 1,
            Err(e) => panic!("{e}"),
        }
    };
    let path = dir.join("before.sqlite");
    let mut db = MemShard::default();
    let mut player = Biota {
        id: 0x50000001,
        ..Default::default()
    };
    player.set_property(PropertyInt::Level, 10);
    let mut old = Biota {
        id: 0x80000001,
        ..Default::default()
    };
    old.set_property(PropertyInstanceId::Container, player.id);
    assert!(db.save_biota(&mut player, false));
    assert!(db.save_biota(&mut old, false));
    backup(&mut db, player.id, &path).unwrap();
    player.set_property(PropertyInt::Level, 100);
    replace(&mut db, player.id, &[player.clone()]).unwrap();
    assert!(backup(&mut db, player.id, &path).is_err());
    let mut saved = SqliteShard::open(&path).unwrap();
    let original =
        BiotaConverter::convert_to_entity_biota(&saved.get_biota(player.id, false).unwrap(), false);
    let item =
        BiotaConverter::convert_to_entity_biota(&saved.get_biota(old.id, false).unwrap(), false);
    assert_eq!(original.get_property(PropertyInt::Level), Some(10));
    replace(&mut db, player.id, &[original, item]).unwrap();
    assert!(db.get_biota(old.id, false).is_some());
    assert_eq!(
        BiotaConverter::convert_to_entity_biota(&db.get_biota(player.id, false).unwrap(), false)
            .get_property(PropertyInt::Level),
        Some(10)
    );
    drop(saved);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_unused_account_is_ready_to_connect_again() {
    let _table = super::command_manager::initialized_table();
    let mut ts = empyrean_testkit::TestServer::new();
    empyrean_command::command_manager::start_console_capture();
    empyrean_command::empyrean::playtest_character::handle(
        &mut ts.world,
        None,
        &["account".into(), "builder".into()],
    );
    assert_eq!(
        empyrean_command::command_manager::take_console_output(),
        ["Playtest account: ready"]
    );
}
