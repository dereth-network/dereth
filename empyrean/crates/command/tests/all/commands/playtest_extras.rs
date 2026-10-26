//! Divergence: V453
//! What a playtest character holds beyond its build: its character record kept by every backup
//! and put back by every restore, and, against the retail data files and the world database, its
//! augmentations, luminance, titles and quests, checked as the server's own console checks check
//! a player.

use empyrean_command::empyrean::playtest_character::{backup, replace_with};
use empyrean_entity::enums::PropertyInt;
use empyrean_entity::Biota;
use empyrean_store::models::shard::{
    Character, CharacterPropertiesQuestRegistry, CharacterPropertiesTitleBook,
};
use empyrean_store::{MemShard, ShardDatabase, SqliteShard};

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

const ID: u32 = 0x5000_0042;

fn record(quests: &[&str], titles: &[u32]) -> Character {
    Character {
        id: ID,
        account_id: 1,
        name: "+Recorded".into(),
        character_properties_quest_registry: quests
            .iter()
            .map(|q| CharacterPropertiesQuestRegistry {
                character_id: ID,
                quest_name: (*q).to_owned(),
                last_time_completed: 0,
                num_times_completed: 1,
            })
            .collect(),
        character_properties_title_book: titles
            .iter()
            .map(|t| CharacterPropertiesTitleBook {
                character_id: ID,
                title_id: *t,
            })
            .collect(),
        ..Character::default()
    }
}

fn quests(c: &Character) -> Vec<&str> {
    c.character_properties_quest_registry
        .iter()
        .map(|q| q.quest_name.as_str())
        .collect()
}

#[test]
fn a_backup_keeps_the_character_record_and_putting_it_back_restores_quests_and_titles() {
    let dir = scratch("playtest-record");
    let path = dir.join("before.sqlite");
    let mut db = MemShard::default();
    let mut player = Biota {
        id: ID,
        ..Default::default()
    };
    player.set_property(PropertyInt::Level, 10);
    assert!(db.save_biota(&mut player, false));
    assert!(db.save_character(&record(&["FirstQuest"], &[1, 2])));
    backup(&mut db, ID, &path).unwrap();

    // The record changed after the backup, as a recipe or play changes it.
    player.set_property(PropertyInt::Level, 100);
    replace_with(
        &mut db,
        ID,
        &[player.clone()],
        Some(&record(&["SecondQuest"], &[1])),
    )
    .unwrap();
    assert_eq!(quests(&db.get_character(ID).unwrap()), ["SecondQuest"]);

    // Put back: the stats and the record together.
    let mut saved = SqliteShard::open(&path).unwrap();
    let kept = saved
        .get_character(ID)
        .expect("the record is in the backup");
    assert_eq!(quests(&kept), ["FirstQuest"]);
    let biota = empyrean_store::adapter::BiotaConverter::convert_to_entity_biota(
        &saved.get_biota(ID, false).unwrap(),
        false,
    );
    replace_with(&mut db, ID, &[biota], Some(&kept)).unwrap();
    let now = db.get_character(ID).unwrap();
    assert_eq!(quests(&now), ["FirstQuest"]);
    assert_eq!(now.character_properties_title_book.len(), 2);
    assert_eq!(
        empyrean_store::adapter::BiotaConverter::convert_to_entity_biota(
            &db.get_biota(ID, false).unwrap(),
            false
        )
        .get_property(PropertyInt::Level),
        Some(10)
    );
    drop(saved);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn another_characters_record_is_never_written_over_this_ones() {
    let mut db = MemShard::default();
    let mut player = Biota {
        id: ID,
        ..Default::default()
    };
    assert!(db.save_biota(&mut player, false));
    assert!(db.save_character(&record(&["Mine"], &[])));
    let other = Character {
        id: ID + 1,
        ..record(&["Theirs"], &[])
    };
    assert!(replace_with(&mut db, ID, &[player], Some(&other)).is_err());
    assert_eq!(quests(&db.get_character(ID).unwrap()), ["Mine"]);
}

/// Against the retail data files and the world database's content pack.
#[cfg(feature = "real-content")]
mod real {
    use super::{quests, record, scratch, ID};
    use dereth_progression::extras::{Extras, Luminous, Mastery};
    use dereth_progression::order::{Gear, Made, Order, ORDER_VERSION};
    use dereth_progression::Archetype;
    use empyrean_command::empyrean::playtest_character::{apply_recipe, verify};
    use empyrean_command::handlers::developer_fix_commands as dfc;
    use empyrean_content::PackContent;
    use empyrean_entity::enums::{
        PropertyAttribute, PropertyInt, PropertyInt64, PropertyString, Skill,
        SkillAdvancementClass, WeenieType,
    };
    use empyrean_entity::Biota;
    use empyrean_store::ShardDatabase;
    use empyrean_testkit::{EmptyShard, TestServer};
    use empyrean_world::entity::i_player::IPlayer;
    use empyrean_world::entity::offline_player::OfflinePlayer;
    use empyrean_world::managers::guid_manager;
    use empyrean_world::managers::player_manager::OrdinalIgnoreCase;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn server_of(source: empyrean_dat::RealDats, pack: PackContent) -> TestServer {
        let dats = empyrean_dat::DatManager::initialize(Arc::new(source)).expect("the dats");
        TestServer::with_setup(dats, |w| {
            w.content = Arc::new(pack);
            w.era = empyrean_common::era::EraExt::rules(w.content.era());
            guid_manager::initialize(w, &mut EmptyShard);
        })
    }

    /// A server over the retail dats and the world database's content pack.
    fn server() -> TestServer {
        let dir = dereth_dat::testing::dat_dir();
        let source = empyrean_dat::RealDats::open(&dir).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the retail dats under {} (set DERETH_TEST_DAT_DIR): {e}",
                dir.display()
            )
        });
        let path = empyrean_common::test_paths::world_pack();
        let pack = PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs world.pack at {} (set EMPYREAN_TEST_WORLD_PACK): {e}",
                path.display()
            )
        });
        server_of(source, pack)
    }

    /// A server over the February 2005 dats and the Infiltration world's content pack.
    fn infiltration_server() -> TestServer {
        if let Some(msg) = dereth_dat::testing::classic_shortfall() {
            panic!("{msg}");
        }
        let dir = dereth_dat::testing::classic_dat_dir().unwrap_or_default();
        let source = empyrean_dat::RealDats::open(&dir)
            .unwrap_or_else(|e| panic!("the February 2005 dats under {}: {e}", dir.display()));
        let path = empyrean_common::test_paths::infiltration_pack();
        let pack = PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "these tests need an Infiltration world.pack at {} (EMPYREAN_TEST_INFILTRATION_PACK): {e}",
                path.display()
            )
        });
        server_of(source, pack)
    }

    /// An Aluvian of level 1, offline and saved with its character record, as creation leaves one.
    fn character(ts: &mut TestServer, name: &str) {
        let mut biota = Biota {
            id: ID,
            weenie_class_id: 1,
            weenie_type: WeenieType::Creature,
            ..Default::default()
        };
        biota.set_property(PropertyString::Name, name.to_owned());
        biota.set_property(PropertyInt::HeritageGroup, 1);
        biota.set_property(PropertyInt::Level, 1);
        assert!(ts.shard().save_biota(&mut biota.clone(), false));
        let mut r = record(&["MadeAtCreation"], &[]);
        r.name = name.to_owned();
        assert!(ts.shard().save_character(&r));
        let offline = {
            let mut shard = ts.shard();
            let mut auth = ts.auth();
            OfflinePlayer::new(biota, &mut **shard, &mut **auth)
        };
        let guid = offline.guid;
        ts.world.player_manager.offline_players.insert(ID, offline);
        ts.world.player_manager.player_names.insert(
            OrdinalIgnoreCase(name.trim_start_matches('+').to_owned()),
            IPlayer::Offline(guid),
        );
    }

    fn order(level: u32, extras: Extras) -> Order {
        Order {
            version: ORDER_VERSION,
            hash: format!("extras-{level}"),
            level,
            plan: Archetype::named("melee-heavy").unwrap().plan,
            heritage: None,
            gender: None,
            gear: Gear::None,
            seed: 1,
            items: Vec::new(),
            place_tier: None,
            slots: BTreeMap::new(),
            extras,
        }
    }

    fn apply(
        ts: &mut TestServer,
        o: &Order,
        dir: &std::path::Path,
        n: u32,
    ) -> Result<Made, String> {
        let input = dir.join(format!("recipe-{n}.json"));
        std::fs::write(&input, serde_json::to_vec(o).unwrap()).unwrap();
        apply_recipe(
            &mut ts.world,
            "+Augmented",
            &input,
            Some(&dir.join(format!("backup-{n}.sqlite"))),
            &dir.join("made.json"),
            true,
        )
    }

    fn bought(list: &[(&str, u32)]) -> BTreeMap<String, u32> {
        list.iter().map(|(id, n)| ((*id).to_owned(), *n)).collect()
    }

    fn stored(ts: &TestServer) -> Biota {
        ts.world
            .player_manager
            .offline_players
            .get(&ID)
            .unwrap()
            .biota
            .clone()
    }

    /// What console command `handler` says, run over every offline character.
    fn console(
        ts: &mut TestServer,
        handler: empyrean_command::command_handler::CommandHandler,
    ) -> Vec<String> {
        empyrean_command::command_manager::start_console_capture();
        handler(&mut ts.world, None, &[]);
        empyrean_command::command_manager::take_console_output()
    }

    #[test]
    fn a_level_275_with_augmentations_auras_titles_and_quests_passes_the_servers_own_checks() {
        let mut ts = server();
        character(&mut ts, "+Augmented");
        let dir = scratch("playtest-extras");
        let extras = Extras {
            augmentations: bought(&[
                ("steadfast-will", 2),
                ("clutch-of-the-miser", 3),
                ("enduring-enchantment", 1),
                ("might-of-the-seventh-mule", 5),
                ("shadow-of-the-seventh-mule", 1),
                ("enhancement-of-the-blade-turner", 2),
                ("jibrils-essence", 1),
                ("asherons-lesser-benediction", 1),
            ]),
            luminance: Some(Luminous {
                auras: bought(&[("aura-of-valor", 10), ("aura-of-the-world", 3)]),
                seer: Some("liam-of-gelid".into()),
            }),
            titles: vec![2, 1],
            quests: bought(&[("PlaytestDoorFlag", 1)]),
            quest_credits: true,
            summoning_mastery: None,
        };
        apply(&mut ts, &order(275, extras), &dir, 0).unwrap();
        let p = stored(&ts);
        let int = |x: PropertyInt| p.get_property(x);
        assert_eq!(int(PropertyInt::AugmentationInnateSelf), Some(2));
        assert_eq!(int(PropertyInt::AugmentationInnateFamily), Some(2));
        assert_eq!(int(PropertyInt::AugmentationResistanceFamily), Some(2));
        assert_eq!(int(PropertyInt::AugmentationLessDeathItemLoss), Some(3));
        assert_eq!(int(PropertyInt::AugmentationJackOfAllTrades), Some(1));
        assert_eq!(
            int(PropertyInt::ContainersCapacity),
            Some(8),
            "a side pack more"
        );
        assert_eq!(int(PropertyInt::LumAugDamageRating), Some(10));
        assert_eq!(int(PropertyInt::LumAugAllSkills), Some(3));
        assert_eq!(
            p.get_property(PropertyInt64::MaximumLuminance),
            Some(1_500_000)
        );
        let self_ = p
            .properties_attribute
            .as_ref()
            .unwrap()
            .get(&PropertyAttribute::Self_)
            .unwrap()
            .clone();
        assert_eq!(self_.init_level, 20, "made with 10, and two augmentations");
        let armor = p
            .properties_skill
            .as_ref()
            .unwrap()
            .get(&Skill::ArmorTinkering)
            .unwrap()
            .clone();
        assert_eq!(
            (armor.sac, armor.init_level),
            (SkillAdvancementClass::Specialized, 10)
        );
        let base = dereth_progression::augmentations::table();
        assert!(base.quest_credit.len() == 3);
        let level_credits = 52 + 46;
        assert_eq!(
            int(PropertyInt::TotalSkillCredits),
            Some(level_credits + 4),
            "four credits from quests"
        );
        // The record: the recipe's quests and the quests behind the credits, nothing else; the
        // titles with creation's.
        let r = ts.shard().get_character(ID).unwrap();
        let mut q = quests(&r);
        q.sort_unstable();
        assert_eq!(
            q,
            [
                "ArantahKill1",
                "LesserBenedictionAug",
                "LumAugSkillQuest",
                "OswaldManualCompleted",
                "PlaytestDoorFlag"
            ]
        );
        let titles: Vec<u32> = r
            .character_properties_title_book
            .iter()
            .map(|t| t.title_id)
            .collect();
        assert!(titles.contains(&1) && titles.contains(&2), "{titles:?}");
        assert_eq!(
            int(PropertyInt::CharacterTitleId),
            Some(2),
            "the recipe's first"
        );
        assert_eq!(
            int(PropertyInt::NumCharacterTitles),
            Some(i32::try_from(titles.len()).unwrap())
        );
        verify(&ts.world, &p).unwrap();
        // The server's own console checks find nothing wrong with it.
        let xp = console(&mut ts, dfc::handle_verify_experience);
        assert!(
            xp.iter().any(|l| l.starts_with("Verified XP for")),
            "{xp:?}"
        );
        let augs = console(&mut ts, dfc::handle_verify_max_augs);
        assert!(
            augs.iter().any(|l| l.starts_with("Verified max augs for")),
            "{augs:?}"
        );
        // The backup holds the record as it was before.
        let mut before = empyrean_store::SqliteShard::open(dir.join("backup-0.sqlite")).unwrap();
        assert_eq!(
            quests(&before.get_character(ID).unwrap()),
            ["MadeAtCreation"]
        );
        drop(before);

        // Made again with none of it: the quests and auras go, the free augmentation stays.
        apply(&mut ts, &order(275, Extras::default()), &dir, 1).unwrap();
        let p = stored(&ts);
        assert_eq!(p.get_property(PropertyInt::LumAugDamageRating), None);
        assert_eq!(
            p.get_property(PropertyInt::AugmentationJackOfAllTrades),
            Some(1)
        );
        assert_eq!(p.get_property(PropertyInt::ContainersCapacity), Some(7));
        assert!(quests(&ts.shard().get_character(ID).unwrap()).is_empty());
        verify(&ts.world, &p).unwrap();

        // Restored from the first backup: as it was before any recipe, its record too.
        empyrean_command::command_manager::start_console_capture();
        empyrean_command::empyrean::playtest_character::handle(
            &mut ts.world,
            None,
            &[
                "restore".into(),
                "+Augmented".into(),
                dir.join("backup-0.sqlite").display().to_string(),
                dir.join("before-restore.sqlite").display().to_string(),
            ],
        );
        assert_eq!(
            empyrean_command::command_manager::take_console_output(),
            ["Playtest character updated; the previous state is backed up."]
        );
        assert_eq!(
            quests(&ts.shard().get_character(ID).unwrap()),
            ["MadeAtCreation"]
        );
        assert_eq!(stored(&ts).get_property(PropertyInt::Level), Some(1));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_recipes_summoning_mastery_is_set_on_the_character_and_none_takes_it_away() {
        let mut ts = server();
        character(&mut ts, "+Augmented");
        let dir = scratch("playtest-mastery");
        let mastery = |m| Extras {
            summoning_mastery: m,
            ..Extras::default()
        };
        let held = |ts: &TestServer| stored(ts).get_property(PropertyInt::SummoningMastery);
        apply(
            &mut ts,
            &order(80, mastery(Some(Mastery::Necromancer))),
            &dir,
            0,
        )
        .unwrap();
        assert_eq!(held(&ts), Some(2));
        verify(&ts.world, &stored(&ts)).unwrap();
        apply(
            &mut ts,
            &order(80, mastery(Some(Mastery::Naturalist))),
            &dir,
            1,
        )
        .unwrap();
        assert_eq!(held(&ts), Some(3), "another chosen");
        // Before level 50 it is unusual, and still set.
        apply(
            &mut ts,
            &order(30, mastery(Some(Mastery::Primalist))),
            &dir,
            2,
        )
        .unwrap();
        assert_eq!(held(&ts), Some(1));
        apply(&mut ts, &order(80, mastery(None)), &dir, 3).unwrap();
        assert_eq!(held(&ts), None, "none: taken away");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn auras_before_level_200_and_a_quest_solved_past_its_limit_are_refused() {
        let mut ts = server();
        character(&mut ts, "+Augmented");
        let dir = scratch("playtest-extras-refused");
        let early = Extras {
            luminance: Some(Luminous {
                auras: bought(&[("aura-of-valor", 1)]),
                seer: None,
            }),
            ..Extras::default()
        };
        let e = apply(&mut ts, &order(150, early), &dir, 0).unwrap_err();
        assert!(e.contains("from level 200"), "{e}");
        let again = Extras {
            quests: bought(&[("OswaldManualCompleted", 2)]),
            ..Extras::default()
        };
        let e = apply(&mut ts, &order(150, again), &dir, 1).unwrap_err();
        assert!(e.contains("at most 1"), "{e}");
        assert!(!dir.join("backup-0.sqlite").exists(), "nothing was changed");
        assert_eq!(
            stored(&ts).get_property(PropertyInt::Level),
            Some(1),
            "nothing was changed"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_world_of_february_2005_has_no_gems_and_no_titles() {
        let mut ts = infiltration_server();
        character(&mut ts, "+Augmented");
        let dir = scratch("playtest-extras-infiltration");
        let gem = Extras {
            augmentations: bought(&[("clutch-of-the-miser", 1)]),
            ..Extras::default()
        };
        let e = apply(&mut ts, &order(126, gem), &dir, 0).unwrap_err();
        assert!(e.contains("no gem"), "{e}");
        let titled = Extras {
            titles: vec![1],
            ..Extras::default()
        };
        let e = apply(&mut ts, &order(50, titled), &dir, 1).unwrap_err();
        assert!(e.contains("no titles"), "{e}");
        // Quests are of every era.
        let quested = Extras {
            quests: bought(&[("PlaytestDoorFlag", 1)]),
            ..Extras::default()
        };
        apply(&mut ts, &order(50, quested), &dir, 2).unwrap();
        assert_eq!(
            quests(&ts.shard().get_character(ID).unwrap()),
            ["PlaytestDoorFlag"]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
