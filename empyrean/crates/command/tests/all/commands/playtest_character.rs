//! Divergence: V453
//! Offline playtest characters made from recipes, made new by the server, their backups and their
//! restores, applied with no backup, and previewed.
use empyrean_command::empyrean::playtest_character::replace;
use empyrean_entity::{
    enums::{PropertyInstanceId, PropertyInt},
    Biota,
};
use empyrean_store::{adapter::BiotaConverter, MemShard, ShardDatabase};

/// A fresh scratch folder of this process's own.
fn scratch(tag: &str) -> std::path::PathBuf {
    let mut serial = 0;
    loop {
        let p = std::env::temp_dir().join(format!("{tag}-{}-{serial}", std::process::id()));
        match std::fs::create_dir(&p) {
            Ok(()) => return p,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => serial += 1,
            Err(e) => panic!("{e}"),
        }
    }
}

#[test]
fn an_old_template_is_refused_with_word_to_upgrade_it_in_lyceum() {
    let _table = super::command_manager::initialized_table();
    let mut ts = empyrean_testkit::TestServer::new();
    let dir = scratch("playtest-old-template");
    let input = dir.join("Old.json");
    std::fs::write(
        &input,
        r#"{"version":1,"level":50,"attributes":[100,90,80,70,60,50],"skills":[],"equipment":[]}"#,
    )
    .unwrap();
    empyrean_command::command_manager::start_console_capture();
    empyrean_command::empyrean::playtest_character::handle(
        &mut ts.world,
        None,
        &[
            "apply".into(),
            "+Old".into(),
            input.display().to_string(),
            dir.join("backup.sqlite").display().to_string(),
            dir.join("made.json").display().to_string(),
        ],
    );
    let output = empyrean_command::command_manager::take_console_output();
    assert_eq!(
        output,
        ["Playtest character unchanged: the recipe is an old template; open the world in Lyceum to upgrade it"]
    );
    assert!(!dir.join("backup.sqlite").exists(), "nothing was backed up");
    std::fs::remove_dir_all(dir).unwrap();
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
            "restore".into(),
            "Missing".into(),
            "unused.sqlite".into(),
            "unused-before.sqlite".into(),
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
fn a_new_character_is_refused_for_an_account_the_server_does_not_have() {
    let _table = super::command_manager::initialized_table();
    let mut ts = empyrean_testkit::TestServer::new();
    let dir = scratch("playtest-create-no-account");
    let input = dir.join("New.json");
    let order = dereth_progression::order::Order {
        version: dereth_progression::order::ORDER_VERSION,
        hash: "new".into(),
        level: 20,
        plan: dereth_progression::Archetype::named("archer").unwrap().plan,
        heritage: None,
        gender: None,
        gear: dereth_progression::order::Gear::None,
        seed: 1,
        items: Vec::new(),
        place_tier: None,
        slots: std::collections::BTreeMap::new(),
        extras: dereth_progression::Extras::default(),
    };
    std::fs::write(&input, serde_json::to_vec(&order).unwrap()).unwrap();
    empyrean_command::command_manager::start_console_capture();
    empyrean_command::empyrean::playtest_character::handle(
        &mut ts.world,
        None,
        &[
            "create".into(),
            "nobody".into(),
            "+Newcomer".into(),
            input.display().to_string(),
            dir.join("made.json").display().to_string(),
        ],
    );
    assert_eq!(
        empyrean_command::command_manager::take_console_output(),
        ["Playtest character unchanged: there is no account nobody"]
    );
    assert!(!dir.join("made.json").exists());
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

/// Against the retail data files' own tables: the builds the console applies.
#[cfg(feature = "real-content")]
mod real {
    use super::scratch;
    use dereth_progression::order::{Gear, Made, Order, ORDER_VERSION};
    use dereth_progression::{build, Archetype};
    use empyrean_command::empyrean::playtest_character::create::create;
    use empyrean_command::empyrean::playtest_character::gear::Wielder;
    use empyrean_command::empyrean::playtest_character::{apply_recipe, verify, with_tables};
    use empyrean_entity::enums::{
        AccessLevel, PositionType, PropertyAttribute2nd, PropertyDataId, PropertyInt,
        PropertyInt64, PropertyString, Skill, SkillAdvancementClass, WeenieType,
    };
    use empyrean_entity::Biota;
    use empyrean_testkit::TestServer;
    use empyrean_world::entity::i_player::IPlayer;
    use empyrean_world::entity::offline_player::OfflinePlayer;
    use empyrean_world::managers::player_manager::OrdinalIgnoreCase;

    fn dats() -> std::sync::Arc<empyrean_dat::DatManager> {
        let dir = dereth_dat::testing::dat_dir();
        let source = empyrean_dat::RealDats::open(&dir).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the retail dats under {} (set DERETH_TEST_DAT_DIR): {e}",
                dir.display()
            )
        });
        empyrean_dat::DatManager::initialize(std::sync::Arc::new(source)).expect("retail dats")
    }

    fn server() -> TestServer {
        TestServer::with_dats(dats())
    }

    /// A server with the world database, whose player weenies creation makes characters of, and
    /// the playtest account, an administrator's.
    fn server_with_content() -> TestServer {
        let path = empyrean_common::test_paths::world_pack();
        let pack = empyrean_content::PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs world.pack at {} (EMPYREAN_TEST_WORLD_PACK): {e}",
                path.display()
            )
        });
        let ts = TestServer::with_setup(dats(), |w| {
            w.content = std::sync::Arc::new(pack);
            empyrean_world::managers::guid_manager::initialize(
                w,
                &mut empyrean_testkit::EmptyShard,
            );
        });
        ts.auth()
            .create_account(
                "builder",
                "builder",
                AccessLevel::Admin,
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            )
            .unwrap();
        ts
    }

    /// An Aluvian of level 1, offline and saved, as the client's creation leaves one.
    fn character(ts: &mut TestServer, id: u32, name: &str) {
        let mut biota = Biota {
            id,
            weenie_class_id: 1,
            weenie_type: WeenieType::Creature,
            ..Default::default()
        };
        biota.set_property(PropertyString::Name, name.to_owned());
        biota.set_property(PropertyInt::HeritageGroup, 1);
        biota.set_property(PropertyInt::Level, 1);
        assert!(ts.shard().save_biota(&mut biota.clone(), false));
        let offline = {
            let mut shard = ts.shard();
            let mut auth = ts.auth();
            OfflinePlayer::new(biota, &mut **shard, &mut **auth)
        };
        let guid = offline.guid;
        ts.world.player_manager.offline_players.insert(id, offline);
        ts.world.player_manager.player_names.insert(
            OrdinalIgnoreCase(name.trim_start_matches('+').to_owned()),
            IPlayer::Offline(guid),
        );
    }

    fn order(level: u32) -> Order {
        Order {
            version: ORDER_VERSION,
            hash: format!("melee-{level}"),
            level,
            plan: Archetype::named("melee-heavy").unwrap().plan,
            heritage: None,
            gender: None,
            gear: Gear::None,
            seed: 1,
            items: Vec::new(),
            place_tier: None,
            slots: std::collections::BTreeMap::new(),
            extras: dereth_progression::Extras::default(),
        }
    }

    fn apply(
        ts: &mut TestServer,
        name: &str,
        o: &Order,
        dir: &std::path::Path,
        n: u32,
        replacing: bool,
    ) -> Result<Made, String> {
        let input = dir.join(format!("recipe-{n}.json"));
        std::fs::write(&input, serde_json::to_vec(o).unwrap()).unwrap();
        apply_recipe(
            &mut ts.world,
            name,
            &input,
            Some(&dir.join(format!("backup-{n}.sqlite"))),
            &dir.join("made.json"),
            replacing,
        )
    }

    const ID: u32 = 0x5000_0042;

    /// What the console says to `words`.
    fn say(ts: &mut TestServer, words: &[&str]) -> Vec<String> {
        empyrean_command::command_manager::start_console_capture();
        empyrean_command::empyrean::playtest_character::handle(
            &mut ts.world,
            None,
            &words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>(),
        );
        empyrean_command::command_manager::take_console_output()
    }

    #[test]
    fn the_console_previews_a_recipe_changing_nothing_and_applies_it_keeping_no_backup_when_told() {
        let _table = super::super::command_manager::initialized_table();
        let mut ts = server();
        character(&mut ts, ID, "+Melee");
        let dir = scratch("playtest-console-preview");
        let input = dir.join("recipe.json");
        std::fs::write(&input, serde_json::to_vec(&order(80)).unwrap()).unwrap();
        let path = |p: &std::path::Path| p.display().to_string();
        let level = |ts: &TestServer| {
            ts.world
                .player_manager
                .offline_players
                .get(&ID)
                .unwrap()
                .biota
                .get_property(PropertyInt::Level)
        };
        // A preview says what the recipe would make; the character stays as it was.
        assert_eq!(
            say(
                &mut ts,
                &["preview", &path(&input), &path(&dir.join("preview.json"))]
            ),
            ["Playtest character previewed."]
        );
        let shown: Made =
            serde_json::from_slice(&std::fs::read(dir.join("preview.json")).unwrap()).unwrap();
        assert_eq!(
            (shown.id, shown.level, shown.hash.as_str()),
            (0, 80, "melee-80")
        );
        assert_eq!(level(&ts), Some(1));
        // Applied with `-` for its backup: made, and no backup kept.
        assert_eq!(
            say(
                &mut ts,
                &[
                    "apply",
                    "+Melee",
                    &path(&input),
                    "-",
                    &path(&dir.join("made.json")),
                    "replace"
                ]
            ),
            ["Playtest character updated."]
        );
        assert_eq!(level(&ts), Some(80));
        let mut files: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        files.sort();
        assert_eq!(
            files,
            ["made.json", "preview.json", "recipe.json"],
            "no backup"
        );
        // With a backup named, one is kept, as before.
        assert_eq!(
            say(
                &mut ts,
                &[
                    "apply",
                    "+Melee",
                    &path(&input),
                    &path(&dir.join("backup.sqlite")),
                    &path(&dir.join("made.json")),
                ]
            ),
            ["Playtest character updated; the previous state is backed up."]
        );
        assert!(dir.join("backup.sqlite").is_file());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_recipe_makes_a_level_80_melee_whose_experience_and_credits_reconcile_with_full_vitals() {
        let mut ts = server();
        character(&mut ts, ID, "+Melee");
        let dir = scratch("playtest-recipe");
        let made = apply(&mut ts, "+Melee", &order(80), &dir, 0, true).unwrap();
        assert_eq!(
            (made.id, made.level, made.hash.as_str()),
            (ID, 80, "melee-80")
        );
        let p = ts
            .world
            .player_manager
            .offline_players
            .get(&ID)
            .unwrap()
            .biota
            .clone();
        assert_eq!(p.get_property(PropertyInt::Level), Some(80));
        let level_xp = with_tables(&ts.world, |t| t.xp.level_xp[80]);
        assert_eq!(
            p.get_property(PropertyInt64::TotalExperience),
            Some(i64::try_from(level_xp).unwrap())
        );
        assert_eq!(p.get_property(PropertyInt::TotalSkillCredits), Some(80));
        let heavy = p
            .properties_skill
            .as_ref()
            .unwrap()
            .get(&Skill::HeavyWeapons)
            .unwrap();
        assert_eq!(
            (heavy.sac, heavy.init_level),
            (SkillAdvancementClass::Specialized, 10)
        );
        assert!(heavy.level_from_pp > 100, "{}", heavy.level_from_pp);
        assert_eq!(
            p.get_property(PropertyInt::AugmentationJackOfAllTrades),
            Some(1),
            "the heritage's free augmentation"
        );
        let health = p
            .properties_attribute_2nd
            .as_ref()
            .unwrap()
            .get(&PropertyAttribute2nd::MaxHealth)
            .unwrap();
        assert!(health.current_level > 100, "{health:?}");
        verify(&ts.world, &p).unwrap();
        // What was stored is what the world holds, and the character before it is backed up.
        let stored = ts.shard().get_biota(ID, false).unwrap();
        let stored =
            empyrean_store::adapter::BiotaConverter::convert_to_entity_biota(&stored, false);
        assert_eq!(stored.get_property(PropertyInt::Level), Some(80));
        assert!(dir.join("backup-0.sqlite").is_file());
        let read: Made =
            serde_json::from_slice(&std::fs::read(dir.join("made.json")).unwrap()).unwrap();
        assert_eq!(read, made);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn only_a_character_a_recipe_made_is_made_again_without_being_told_to_replace_it() {
        let mut ts = server();
        character(&mut ts, ID, "+Melee");
        character(&mut ts, ID + 1, "+Other");
        let dir = scratch("playtest-marker");
        let refused = apply(&mut ts, "+Melee", &order(20), &dir, 0, false).unwrap_err();
        assert!(refused.contains("not made from a recipe"), "{refused}");
        assert!(!dir.join("backup-0.sqlite").exists());
        apply(&mut ts, "+Melee", &order(20), &dir, 1, true).unwrap();
        apply(&mut ts, "+Melee", &order(126), &dir, 2, false).unwrap();
        let refused = apply(&mut ts, "+Other", &order(20), &dir, 3, false).unwrap_err();
        assert!(refused.contains("not made from a recipe"), "{refused}");
        assert_eq!(
            ts.world
                .player_manager
                .offline_players
                .get(&(ID + 1))
                .unwrap()
                .biota
                .get_property(PropertyInt::Level),
            Some(1),
            "the other character is as it was"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_character_whose_experience_or_credits_do_not_add_up_fails_the_check() {
        let mut ts = server();
        character(&mut ts, ID, "+Melee");
        let dir = scratch("playtest-check");
        apply(&mut ts, "+Melee", &order(80), &dir, 0, true).unwrap();
        let good = ts
            .world
            .player_manager
            .offline_players
            .get(&ID)
            .unwrap()
            .biota
            .clone();
        let mut more = good.clone();
        more.properties_attribute
            .as_mut()
            .unwrap()
            .get_mut(&empyrean_entity::enums::PropertyAttribute::Strength)
            .unwrap()
            .cp_spent += 1_000;
        assert!(verify(&ts.world, &more).is_err(), "experience from nowhere");
        let mut credit = good.clone();
        credit.set_property(PropertyInt::AvailableSkillCredits, 50);
        assert!(verify(&ts.world, &credit).is_err(), "credits from nowhere");
        let mut no_aug = good;
        no_aug.set_property(PropertyInt::AugmentationJackOfAllTrades, 0);
        assert!(verify(&ts.world, &no_aug).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn stored(ts: &TestServer, id: u32) -> Biota {
        ts.world
            .player_manager
            .offline_players
            .get(&id)
            .unwrap()
            .biota
            .clone()
    }

    #[test]
    fn a_new_character_is_made_by_the_server_of_the_recipes_heritage_and_gender_on_its_account() {
        let mut ts = server_with_content();
        let dir = scratch("playtest-create");
        let input = dir.join("recipe.json");
        let mut o = order(50);
        o.plan = Archetype::named("war-mage").unwrap().plan;
        o.heritage = Some(dereth_progression::heritage::SHO);
        o.gender = Some(dereth_progression::gender::FEMALE);
        std::fs::write(&input, serde_json::to_vec(&o).unwrap()).unwrap();
        let made = create(
            &mut ts.world,
            "builder",
            "+Sho Mage",
            &input,
            &dir.join("made.json"),
        )
        .unwrap();
        assert_eq!((made.name.as_str(), made.level), ("Sho Mage", 50));
        let p = stored(&ts, made.id);
        assert_eq!(p.get_property(PropertyInt::HeritageGroup), Some(3));
        assert_eq!(p.get_property(PropertyInt::Gender), Some(2));
        assert_eq!(p.get_property(PropertyInt::Level), Some(50));
        let war = p
            .properties_skill
            .as_ref()
            .unwrap()
            .get(&Skill::WarMagic)
            .unwrap();
        assert_eq!(war.sac, SkillAdvancementClass::Specialized);
        verify(&ts.world, &p).unwrap();
        // Saved on the account, as creation saves a character: its record and its body.
        let account = ts.auth().get_account_by_name("builder").unwrap().account_id;
        let record = ts.shard().get_character(made.id).unwrap();
        assert_eq!(
            (record.account_id, record.name.as_str()),
            (account, "Sho Mage")
        );
        assert_eq!(
            record.total_logins, 1,
            "no newcomer's welcome at its first login"
        );
        assert!(ts.shard().get_biota(made.id, false).is_some());
        // The name is taken now.
        let refused = create(
            &mut ts.world,
            "builder",
            "+Sho Mage",
            &input,
            &dir.join("again.json"),
        )
        .unwrap_err();
        assert_eq!(refused, "the name is taken");
        // The server made it, so the next apply needs no word to replace it.
        apply(&mut ts, "+Sho Mage", &order(60), &dir, 1, false).unwrap();
        assert_eq!(
            stored(&ts, made.id).get_property(PropertyInt::Level),
            Some(60)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_made_character_holds_creations_free_ranks_and_verify_xp_and_skill_credits_find_nothing() {
        let _table = super::super::command_manager::initialized_table();
        let mut ts = server_with_content();
        ts.auth()
            .create_account(
                "player",
                "pw",
                AccessLevel::Player,
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            )
            .unwrap();
        let dir = scratch("playtest-verify");
        let input = dir.join("recipe.json");
        std::fs::write(&input, serde_json::to_vec(&order(80)).unwrap()).unwrap();
        let made = create(
            &mut ts.world,
            "player",
            "Plain Melee",
            &input,
            &dir.join("made.json"),
        )
        .unwrap();
        let skill = |ts: &TestServer, s: Skill| {
            let p = stored(ts, made.id);
            let k = p
                .properties_skill
                .as_ref()
                .unwrap()
                .get(&s)
                .cloned()
                .unwrap();
            (k.sac, k.level_from_pp, k.pp)
        };
        assert_eq!(
            skill(&ts, Skill::Salvaging),
            (SkillAdvancementClass::Trained, 5, 526),
            "creation's free ranks"
        );
        assert_eq!(
            skill(&ts, Skill::HeavyWeapons).0,
            SkillAdvancementClass::Specialized
        );
        // ACE's own checks of every character, a player's not skipped: nothing to fix.
        let fix = empyrean_command::handlers::developer_fix_commands::handle_verify_experience;
        let credits =
            empyrean_command::handlers::developer_fix_commands::handle_verify_skill_credits;
        empyrean_command::command_manager::start_console_capture();
        fix(&mut ts.world, None, &[]);
        credits(&mut ts.world, None, &[]);
        assert_eq!(
            empyrean_command::command_manager::take_console_output(),
            [
                "Verified XP for 1 players",
                "Verified skill credits for 1 players"
            ]
        );
        // An experience point that neither ranks nor creation account for is found.
        ts.world
            .player_manager
            .offline_players
            .get_mut(&made.id)
            .unwrap()
            .biota
            .properties_skill
            .as_mut()
            .unwrap()
            .get_mut(&Skill::Salvaging)
            .unwrap()
            .pp += 1;
        empyrean_command::command_manager::start_console_capture();
        fix(&mut ts.world, None, &[]);
        let found = empyrean_command::command_manager::take_console_output();
        assert!(found.iter().any(|l| l.contains("differs by")), "{found:?}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_recipe_of_another_heritage_and_gender_gives_its_character_a_new_body_where_it_stood() {
        let mut ts = server_with_content();
        let dir = scratch("playtest-new-body");
        let input = dir.join("recipe.json");
        std::fs::write(&input, serde_json::to_vec(&order(30)).unwrap()).unwrap();
        let made = create(
            &mut ts.world,
            "builder",
            "+Changer",
            &input,
            &dir.join("made.json"),
        )
        .unwrap();
        let before = stored(&ts, made.id);
        assert_eq!(before.get_property(PropertyInt::HeritageGroup), Some(1));
        let mut lugian = order(30);
        lugian.heritage = Some(dereth_progression::heritage::LUGIAN);
        lugian.gender = Some(dereth_progression::gender::FEMALE);
        let again = apply(&mut ts, "+Changer", &lugian, &dir, 1, false).unwrap();
        assert_eq!(again.id, made.id, "the same character");
        let after = stored(&ts, made.id);
        assert_eq!(after.get_property(PropertyInt::HeritageGroup), Some(8));
        assert_eq!(after.get_property(PropertyInt::Gender), Some(2));
        assert_ne!(
            after.get_property(PropertyDataId::Setup),
            before.get_property(PropertyDataId::Setup),
            "a Lugian's body"
        );
        let at = |b: &Biota| {
            b.get_position(PositionType::Location)
                .map(|p| (p.cell(), p.position_x, p.position_y, p.position_z))
        };
        assert!(at(&before).is_some());
        assert_eq!(at(&after), at(&before), "where it stood");
        verify(&ts.world, &after).unwrap();
        assert!(
            dir.join("backup-1.sqlite").is_file(),
            "the old body is kept"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_item_needing_more_than_the_build_has_is_not_one_it_can_wield() {
        let ts = server();
        let plan = Archetype::named("melee-heavy").unwrap().plan;
        with_tables(&ts.world, |t| {
            let b = build(&t, 80, 1, &plan).unwrap();
            let who = Wielder {
                build: &b,
                tables: t,
            };
            let heavy = i32::try_from(b.skill(&t, 44)).unwrap();
            let item = |kind: i32, of: i32, at: i32| {
                let mut i = Biota::default();
                i.set_property(PropertyInt::WieldRequirements, kind);
                i.set_property(PropertyInt::WieldSkillType, of);
                i.set_property(PropertyInt::WieldDifficulty, at);
                i
            };
            assert!(who.meets(&item(2, 44, heavy)), "its own base Heavy Weapons");
            assert!(!who.meets(&item(2, 44, heavy + 1)));
            assert!(who.meets(&item(7, 0, 80)) && !who.meets(&item(7, 0, 150)));
            assert!(who.meets(&Biota::default()), "nothing asked");
        });
    }
}
