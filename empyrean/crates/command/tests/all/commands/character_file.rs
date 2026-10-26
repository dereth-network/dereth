//! Divergence: V453, V459
//! Characters carried between worlds: exported to a character file, which import-character
//! brings into another world's account when its operator allows it, with new object ids, after
//! the era, class and stats checks; and everything it refuses.

use empyrean_command::command_manager::{start_console_capture, take_console_output};
use empyrean_command::empyrean::character_file::handle_import;
use empyrean_command::empyrean::playtest_character::handle;
use empyrean_common::master_configuration::MasterConfiguration;
use empyrean_store::SqliteShard;

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

/// A configuration whose operator lets characters be imported.
fn importing() -> MasterConfiguration {
    let mut c = MasterConfiguration::default();
    c.server.accounts.allow_character_import = true;
    c
}

fn console(
    ts: &mut empyrean_testkit::TestServer,
    f: impl FnOnce(&mut empyrean_world::World),
) -> Vec<String> {
    start_console_capture();
    f(&mut ts.world);
    take_console_output()
}

fn import(ts: &mut empyrean_testkit::TestServer, args: &[&str]) -> Vec<String> {
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    console(ts, |w| handle_import(w, None, &args))
}

#[test]
fn importing_is_off_unless_the_operator_turns_it_on() {
    let _table = super::command_manager::initialized_table();
    let mut ts = empyrean_testkit::TestServer::new();
    assert_eq!(
        import(&mut ts, &["missing.empyrean-character", "player"]),
        ["Character not imported: importing characters is off; set allow_character_import = true under [server.accounts] in empyrean.toml to allow it"]
    );
    assert!(
        !MasterConfiguration::default()
            .server
            .accounts
            .allow_character_import
    );
    let mut on = empyrean_testkit::TestServer::with_config(
        empyrean_testkit::dats::with_stat_tables(empyrean_dat::FakeDats::new())
            .build()
            .unwrap(),
        importing(),
        |_| {},
    );
    let out = import(&mut on, &["missing.empyrean-character", "player"]);
    assert_eq!(out.len(), 1);
    assert!(out[0].contains("there is no file"), "{out:?}");
    let row = empyrean_command::empyrean::character_file::command_handlers().remove(0);
    assert_eq!(row.attribute.command, "import-character");
    assert_eq!(
        row.attribute.flags,
        empyrean_command::command_handler_flag::CommandHandlerFlag::ConsoleInvoke
    );
}

#[test]
fn a_shard_that_is_not_a_character_file_is_refused_and_left_as_it_was() {
    let _table = super::command_manager::initialized_table();
    let dir = scratch("import-plain-shard");
    let path = dir.join("plain.sqlite");
    drop(SqliteShard::open(&path).unwrap());
    let before = std::fs::read(&path).unwrap();
    let mut ts = empyrean_testkit::TestServer::with_config(
        empyrean_testkit::dats::with_stat_tables(empyrean_dat::FakeDats::new())
            .build()
            .unwrap(),
        importing(),
        |_| {},
    );
    let out = import(&mut ts, &[path.to_str().unwrap(), "player"]);
    assert_eq!(
        out,
        ["Character not imported: the file is not a character file"]
    );
    assert_eq!(std::fs::read(&path).unwrap(), before, "the operator's file");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn only_an_offline_character_is_exported() {
    let _table = super::command_manager::initialized_table();
    let mut ts = empyrean_testkit::TestServer::new();
    let dir = scratch("export-missing");
    let out = dir.join("Missing.empyrean-character");
    let args = [
        "export".to_owned(),
        "+Missing".to_owned(),
        out.display().to_string(),
    ];
    assert_eq!(
        console(&mut ts, |w| handle(w, None, &args)),
        ["Playtest character not exported: character must exist and be logged out"]
    );
    assert!(!out.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

/// Against the retail data files and the world database's content pack: a character made in one
/// world, exported, and imported into another.
#[cfg(feature = "real-content")]
mod real {
    use super::{import, importing, scratch};
    use dereth_progression::order::{ExportNote, Gear, Order, EXPORT_VERSION, ORDER_VERSION};
    use dereth_progression::Archetype;
    use empyrean_command::empyrean::playtest_character::create::create;
    use empyrean_command::empyrean::playtest_character::{handle, verify};
    use empyrean_entity::enums::{
        AccessLevel, PropertyAttribute, PropertyInstanceId, PropertyInt, WeenieType,
    };
    use empyrean_entity::Biota;
    use empyrean_store::adapter::BiotaConverter;
    use empyrean_store::character_file::read_manifest;
    use empyrean_store::{ShardDatabase, SqliteShard};
    use empyrean_testkit::{EmptyShard, TestServer};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    fn dats() -> Arc<empyrean_dat::DatManager> {
        let dir = dereth_dat::testing::dat_dir();
        let source = empyrean_dat::RealDats::open(&dir).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the retail dats under {} (set DERETH_TEST_DAT_DIR): {e}",
                dir.display()
            )
        });
        empyrean_dat::DatManager::initialize(Arc::new(source)).expect("retail dats")
    }

    fn pack() -> empyrean_content::PackContent {
        let path = empyrean_common::test_paths::world_pack();
        empyrean_content::PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs world.pack at {} (EMPYREAN_TEST_WORLD_PACK): {e}",
                path.display()
            )
        })
    }

    /// A world over the retail dats and the world database, with the accounts `accounts`.
    fn world(
        config: Option<empyrean_common::master_configuration::MasterConfiguration>,
        accounts: &[(&str, AccessLevel)],
    ) -> TestServer {
        let setup = |w: &mut empyrean_world::World| {
            w.content = Arc::new(pack());
            empyrean_world::managers::guid_manager::initialize(w, &mut EmptyShard);
        };
        let ts = match config {
            Some(c) => TestServer::with_config(dats(), c, setup),
            None => TestServer::with_setup(dats(), setup),
        };
        for (name, level) in accounts {
            ts.auth()
                .create_account(
                    name,
                    "pw",
                    *level,
                    std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                )
                .unwrap();
        }
        ts
    }

    fn order(level: u32, hash: &str) -> Order {
        Order {
            version: ORDER_VERSION,
            hash: hash.into(),
            level,
            plan: Archetype::named("melee-heavy").unwrap().plan,
            heritage: None,
            gender: None,
            gear: Gear::Level,
            seed: 7,
            items: Vec::new(),
            place_tier: None,
            slots: std::collections::BTreeMap::new(),
            extras: dereth_progression::Extras::default(),
        }
    }

    /// A character made in a world of its own by an administrator's recipe, exported to `dir`:
    /// the file, the character's id in it, and the classes of what it wears.
    fn exported(dir: &Path) -> (PathBuf, u32, Vec<u32>) {
        let mut a = world(None, &[("builder", AccessLevel::Admin)]);
        let input = dir.join("recipe.json");
        std::fs::write(&input, serde_json::to_vec(&order(80, "melee-80")).unwrap()).unwrap();
        let made = create(
            &mut a.world,
            "builder",
            "+Exporter",
            &input,
            &dir.join("made.json"),
        )
        .unwrap();
        let note = dir.join("note.json");
        let class = made.items[0].class;
        std::fs::write(
            &note,
            serde_json::to_vec(&ExportNote {
                version: EXPORT_VERSION,
                recipe: "version = 2\nlevel = 80\narchetype = \"melee-heavy\"\n".into(),
                hash: "melee-80".into(),
                symbols: std::collections::BTreeMap::from([(class, "loot:weapon".to_owned())]),
            })
            .unwrap(),
        )
        .unwrap();
        let out = dir.join("Exporter.empyrean-character");
        let args = [
            "export".to_owned(),
            "+Exporter".to_owned(),
            out.display().to_string(),
            note.display().to_string(),
        ];
        assert_eq!(
            super::console(&mut a, |w| handle(w, None, &args)),
            ["Playtest character exported."]
        );
        // Asked again, it never writes over the file.
        let again = super::console(&mut a, |w| handle(w, None, &args));
        assert!(
            again[0].starts_with("Playtest character not exported:"),
            "{again:?}"
        );
        let mut shard = SqliteShard::open(&out).unwrap();
        let mut worn: Vec<u32> = shard
            .get_possessed_biotas_in_parallel(made.id)
            .wielded_items
            .iter()
            .map(|b| b.weenie_class_id)
            .collect();
        drop(shard);
        worn.sort_unstable();
        assert!(worn.contains(&class), "{worn:?}");
        (out, made.id, worn)
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
    fn a_character_exported_from_one_world_enters_another_as_a_player_with_new_ids_its_items_following(
    ) {
        let _table = super::super::command_manager::initialized_table();
        let dir = scratch("character-file-round-trip");
        let (file, old_id, classes) = exported(&dir);

        // The file says what it is.
        let shard = SqliteShard::open(&file).unwrap();
        let m = read_manifest(&shard).unwrap().unwrap();
        drop(shard);
        assert_eq!(
            (m.era.as_str(), m.name.as_str(), m.id),
            ("eor", "Exporter", old_id)
        );
        assert_eq!((m.hash.as_str(), m.checks.as_str()), ("melee-80", ""));
        assert!(m.recipe.contains("melee-heavy"));
        assert!(classes.iter().all(|k| m
            .classes
            .iter()
            .any(|c| c.class == *k && !c.name.is_empty())));
        let symbols: Vec<_> = m.classes.iter().filter_map(|c| c.symbol.clone()).collect();
        assert_eq!(
            symbols,
            ["loot:weapon"],
            "the world's own weenie, by its symbol"
        );
        assert!(
            m.classes.iter().any(|c| c.name == "admin"),
            "{:?}",
            m.classes
        );

        // Another world, where someone already plays, so its ids have moved on.
        let mut b = world(
            Some(importing()),
            &[
                ("builder", AccessLevel::Admin),
                ("player", AccessLevel::Player),
            ],
        );
        let input = dir.join("resident.json");
        std::fs::write(&input, serde_json::to_vec(&order(10, "resident")).unwrap()).unwrap();
        let resident = create(
            &mut b.world,
            "builder",
            "+Resident",
            &input,
            &dir.join("resident-made.json"),
        )
        .unwrap();
        let out = import(&mut b, &[file.to_str().unwrap(), "player"]);
        assert_eq!(out.len(), 1);
        assert!(out[0].starts_with("Imported Exporter with "), "{out:?}");
        let found = empyrean_world::managers::player_manager::find_by_name(&b.world, "Exporter")
            .0
            .unwrap();
        let id = found.guid().full();
        assert_ne!(id, resident.id);
        let p = stored(&b, id);
        assert_eq!(p.get_property(PropertyInt::Level), Some(80));
        assert_eq!(
            p.weenie_type,
            WeenieType::Creature,
            "a player's account: a player's body"
        );
        verify(&b.world, &p).unwrap();
        let account = b.auth().get_account_by_name("player").unwrap().account_id;
        let record = b.shard().get_character(id).unwrap();
        assert_eq!(
            (record.account_id, record.name.as_str(), record.is_plussed),
            (account, "Exporter", false)
        );
        // What it owns came with it, under new ids, and follows it.
        let held = b.shard().get_possessed_biotas_in_parallel(id);
        let items: Vec<Biota> = held
            .inventory
            .iter()
            .chain(&held.wielded_items)
            .map(|x| BiotaConverter::convert_to_entity_biota(x, false))
            .collect();
        let mut came: Vec<u32> = items
            .iter()
            .filter(|i| i.get_property(PropertyInstanceId::Wielder) == Some(id))
            .map(|i| i.weenie_class_id)
            .collect();
        came.sort_unstable();
        assert_eq!(came, classes, "everything it wore");
        let owners: Vec<u32> = items
            .iter()
            .filter_map(|i| {
                i.get_property(PropertyInstanceId::Wielder)
                    .or(i.get_property(PropertyInstanceId::Container))
            })
            .collect();
        assert!(
            owners
                .iter()
                .all(|o| *o == id || items.iter().any(|i| i.id == *o)),
            "{owners:?}"
        );
        let mut every: Vec<u32> = items.iter().map(|i| i.id).collect();
        every.sort_unstable();
        every.dedup();
        assert_eq!(every.len(), items.len(), "an id each");

        // The same file again needs a name of its own.
        let out = import(&mut b, &[file.to_str().unwrap(), "player"]);
        assert_eq!(
            out,
            ["Character not imported: the name Exporter is taken; give the character a new one"]
        );
        let out = import(&mut b, &[file.to_str().unwrap(), "player", "Second Copy"]);
        assert!(out[0].starts_with("Imported Second Copy with "), "{out:?}");
        // An administrator's account keeps an administrator's body.
        let out = import(&mut b, &[file.to_str().unwrap(), "builder", "Staff Copy"]);
        assert!(out[0].starts_with("Imported Staff Copy"), "{out:?}");
        let staff = empyrean_world::managers::player_manager::find_by_name(&b.world, "Staff Copy")
            .0
            .unwrap();
        assert_eq!(
            stored(&b, staff.guid().full()).weenie_type,
            WeenieType::Admin
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A copy of `file` changed by `edit`.
    fn edited(file: &Path, to: &Path, edit: impl FnOnce(&mut SqliteShard)) -> PathBuf {
        std::fs::copy(file, to).unwrap();
        let mut shard = SqliteShard::open(to).unwrap();
        edit(&mut shard);
        drop(shard);
        to.to_path_buf()
    }

    #[test]
    fn a_file_of_another_era_a_class_named_otherwise_here_or_stats_from_nowhere_is_refused() {
        let _table = super::super::command_manager::initialized_table();
        let dir = scratch("character-file-refused");
        let (file, old_id, classes) = exported(&dir);
        let mut b = world(Some(importing()), &[("player", AccessLevel::Player)]);
        let refused = |b: &mut TestServer, f: &Path| {
            let out = import(b, &[f.to_str().unwrap(), "player"]);
            assert_eq!(out.len(), 1);
            out[0].clone()
        };

        let era = edited(&file, &dir.join("era.sqlite"), |s| {
            s.connection()
                .execute_batch("UPDATE character_file SET value = 'infiltration' WHERE key = 'era'")
                .unwrap();
        });
        let e = refused(&mut b, &era);
        assert!(
            e.ends_with("the character is of the infiltration era, and this world plays eor"),
            "{e}"
        );

        let class = edited(&file, &dir.join("class.sqlite"), |s| {
            s.connection()
                .execute_batch(&format!(
                    "UPDATE character_file_class SET name = 'someoneelsessword' WHERE class = {}",
                    classes[0]
                ))
                .unwrap();
        });
        let e = refused(&mut b, &class);
        assert!(
            e.contains(&format!("class {} is ", classes[0]))
                && e.ends_with("someoneelsessword in the file"),
            "{e}"
        );

        let stats = edited(&file, &dir.join("stats.sqlite"), |s| {
            let mut p = BiotaConverter::convert_to_entity_biota(
                &s.get_biota(old_id, false).unwrap(),
                false,
            );
            p.properties_attribute
                .as_mut()
                .unwrap()
                .get_mut(&PropertyAttribute::Strength)
                .unwrap()
                .cp_spent += 1_000;
            assert!(s.save_biota(&mut p, false));
        });
        let e = refused(&mut b, &stats);
        assert!(e.contains("the checks find it wrong"), "{e}");

        let nobody = import(&mut b, &[file.to_str().unwrap(), "nobody"]);
        assert_eq!(
            nobody,
            ["Character not imported: there is no account nobody"]
        );
        // Nothing came in.
        assert!(
            empyrean_world::managers::player_manager::find_by_name(&b.world, "Exporter")
                .0
                .is_none()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
