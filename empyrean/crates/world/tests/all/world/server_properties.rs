//! Vectors: fixtures/vectors/properties/
//! PropertyManager default tables and ConfigurationEntry behaviour replay ACE properties vectors.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use empyrean_common::era::EraExt as _;
use std::time::Duration;

use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::DotNetDateTime;
use empyrean_common::not_ported::take_local;
use empyrean_common::vectors::{self, f64_of, i64_of, same_f64};
use empyrean_dat::FakeDats;
use empyrean_store::{MemShard, ShardConfigDatabase, SqliteShard};
use empyrean_world::factories::loot_generation_factory::tables_logic::cantrips::cantrip_chance;
use empyrean_world::managers::property_manager::{
    self as pm, default_property_manager as dpm, shard_config_handle, ConfigurationEntry,
    ShardConfigHandle, WORKER_INTERVAL,
};
use empyrean_world::World;
use serde_json::Value;

fn world_at(secs: u64) -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 0.0,
        unix_time: 1_000_000.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::from_secs(secs),
    };
    World::new(now, FakeDats::new().build().expect("empty fake dats"))
}

/// A world whose `DatabaseManager.ShardConfig` is `config`, initialised as `Program.Main` does.
fn started(config: &ShardConfigHandle) -> World {
    let mut w = world_at(1000);
    pm::install_shard_config(&mut w, config.clone());
    pm::initialize(&mut w, true);
    w
}

fn db(
    config: &ShardConfigHandle,
) -> std::sync::MutexGuard<'_, Box<dyn ShardConfigDatabase + Send>> {
    config
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn str_of(v: &Value) -> Option<String> {
    v.as_str().map(str::to_owned)
}

/// Boolean defaults ruled to retail (key, ACE's default, ours). ACE's recorded vectors stay as
/// ACE printed them; [`retail_ruled_default`] and [`retail_ruled_listing`] apply these rows, and
/// each must hit its vector, so a re-recorded ACE that changed the default fails here.
/// - `fellow_kt_killer`: V377 (retail pcaps, the fellowship kill-task credit finding): a fellow holding the kill task was credited on a kill by a
///   fellow without it.
const RETAIL_RULED_BOOLS: &[(&str, bool, bool)] = &[("fellow_kt_killer", true, false)];

/// One `default_boolean_properties` output with [`RETAIL_RULED_BOOLS`] applied; `hits` counts it.
fn retail_ruled_default(mut output: Value, hits: &mut [usize]) -> Value {
    for (i, &(key, ace, ours)) in RETAIL_RULED_BOOLS.iter().enumerate() {
        if output["key"].as_str() == Some(key) {
            assert_eq!(
                output["item"],
                Value::from(ace),
                "{key}: ACE's recorded default"
            );
            output["item"] = Value::from(ours);
            hits[i] += 1;
        }
    }
    output
}

/// A `ListProperties` listing with [`RETAIL_RULED_BOOLS`] applied to each untouched default line.
fn retail_ruled_listing(listing: &str) -> String {
    let s = |b: bool| if b { "True" } else { "False" };
    let mut listing = listing.to_owned();
    for &(key, ace, ours) in RETAIL_RULED_BOOLS {
        let at = listing
            .find(&format!("\t{key}: "))
            .unwrap_or_else(|| panic!("{key} in the listing"));
        let end = at + listing[at..].find('\n').unwrap();
        let line = &listing[at..end];
        let ace_tail = format!("(current is {}, default is {})", s(ace), s(ace));
        assert!(line.ends_with(&ace_tail), "{line}: ACE's recorded default");
        let ours_line = format!(
            "{}(current is {}, default is {})",
            &line[..line.len() - ace_tail.len()],
            s(ours),
            s(ours)
        );
        listing.replace_range(at..end, &ours_line);
    }
    listing
}

#[test]
fn default_tables_match_aces_in_order_with_descriptions() {
    let mut hits = vec![0usize; RETAIL_RULED_BOOLS.len()];
    let mut check = |name: &str, rust: Vec<(&str, Value, Option<String>)>| {
        let file = vectors::load_named("properties", name);
        assert_eq!(rust.len(), file.cases.len(), "{name}: entry count");
        for (i, (case, (key, item, description))) in file.cases.iter().zip(rust).enumerate() {
            assert_eq!(
                i64_of(&case.input["index"]),
                Some(i64::try_from(i).unwrap())
            );
            // ACE's text under Empyrean's brand rule (its sites and command names are ours)
            let output = vectors::brand_ruled_value(&case.output);
            // and its boolean defaults under the retail rulings
            let output = if name == "default_boolean_properties" {
                retail_ruled_default(output, &mut hits)
            } else {
                output
            };
            assert_eq!(output["key"].as_str(), Some(key), "{name}[{i}] key");
            match &item {
                Value::Number(_) if output["item"].is_f64() => {
                    assert!(
                        same_f64(f64_of(&output["item"]).unwrap(), f64_of(&item).unwrap()),
                        "{name}[{i}] {key} item"
                    );
                }
                _ => assert_eq!(output["item"], item, "{name}[{i}] {key} item"),
            }
            assert_eq!(
                str_of(&output["description"]),
                description,
                "{name}[{i}] {key} description"
            );
        }
    };
    // this server's own options follow ACE's
    let added: Vec<&str> = dpm::ADDED_BOOLEAN_PAIRS
        .iter()
        .map(|(k, _, _)| *k)
        .collect();
    let bools: Vec<_> = dpm::DEFAULT_BOOLEAN_PROPERTIES.iter().collect();
    let ace_count = bools.len() - added.len();
    assert_eq!(
        bools[ace_count..]
            .iter()
            .map(|(k, _)| **k)
            .collect::<Vec<_>>(),
        added,
        "the added options, after ACE's"
    );
    check(
        "default_boolean_properties",
        bools[..ace_count]
            .iter()
            .map(|(k, p)| (**k, Value::from(p.item), p.description.clone()))
            .collect(),
    );
    check(
        "default_long_properties",
        dpm::DEFAULT_LONG_PROPERTIES
            .iter()
            .map(|(k, p)| (*k, Value::from(p.item), p.description.clone()))
            .collect(),
    );
    check(
        "default_double_properties",
        dpm::DEFAULT_DOUBLE_PROPERTIES
            .iter()
            .map(|(k, p)| (*k, Value::from(p.item), p.description.clone()))
            .collect(),
    );
    check(
        "default_string_properties",
        dpm::DEFAULT_STRING_PROPERTIES
            .iter()
            .map(|(k, p)| (*k, Value::from(p.item.clone()), p.description.clone()))
            .collect(),
    );
    assert_eq!(
        hits,
        vec![1; RETAIL_RULED_BOOLS.len()],
        "each retail ruling hits ACE's defaults once"
    );
}

#[test]
fn configuration_entry_to_string_matches_net() {
    let file = vectors::load_named("properties", "configuration_entry_to_string");
    for case in &file.cases {
        let modified = case.input["modified"].as_bool().unwrap();
        let item = &case.input["item"];
        let got = match case.input["type"].as_str().unwrap() {
            "Boolean" => ConfigurationEntry::new(modified, item.as_bool().unwrap()).to_string(),
            "Int64" => ConfigurationEntry::new(modified, i64_of(item).unwrap()).to_string(),
            "Double" => ConfigurationEntry::new(modified, f64_of(item).unwrap()).to_string(),
            "String" => {
                ConfigurationEntry::new(modified, item.as_str().unwrap().to_owned()).to_string()
            }
            other => panic!("unknown type {other}"),
        };
        assert_eq!(Some(got.as_str()), case.output.as_str(), "{:?}", case.input);
    }
}

/// The harness ran `LoadDefaultProperties` on ACE's empty static caches; a new world's state is
/// that state (see the module docs of `property_manager`).
#[test]
fn cache_script_matches_ace_through_list_properties() {
    let w = world_at(0);
    let file = vectors::load_named("properties", "cache_script");
    assert!(file.cases.len() > 20);
    for case in &file.cases {
        let op = case.input["op"].as_str().unwrap();
        let key = case.input["key"].as_str().unwrap_or_default();
        let value = &case.input["value"];
        // ACE's text under Empyrean's brand rule (its sites and command names are ours)
        let out = &vectors::brand_ruled_value(&case.output);
        match op {
            "ListProperties" => {
                // this server's own options are listed after ACE's booleans
                // (and ACE's boolean defaults under the retail rulings)
                let mut expected = retail_ruled_listing(out.as_str().unwrap());
                let added: String = dpm::ADDED_BOOLEAN_PAIRS
                    .iter()
                    .map(|(k, default, d)| {
                        let s = |b: bool| if b { "True" } else { "False" };
                        format!(
                            "\t{k}: {d} (current is {}, default is {})\n",
                            s(pm::get_bool(&w, k, false, true).item),
                            s(*default)
                        )
                    })
                    .collect();
                let at = expected
                    .find("\nLong properties:")
                    .expect("the long properties");
                expected.insert_str(at, &added);
                assert_eq!(pm::list_properties(&w), expected);
            }
            "GetBool" => {
                let p = pm::get_bool(&w, key, false, true);
                assert_eq!(
                    (Value::from(p.item), p.description),
                    (out["item"].clone(), str_of(&out["description"])),
                    "{key}"
                );
            }
            "GetLong" => {
                let p = pm::get_long(&w, key, 0, true);
                assert_eq!(
                    (Some(p.item), p.description),
                    (i64_of(&out["item"]), str_of(&out["description"])),
                    "{key}"
                );
            }
            "GetDouble" => {
                let p = pm::get_double(&w, key, 0.0, true);
                assert!(same_f64(p.item, f64_of(&out["item"]).unwrap()), "{key}");
                assert_eq!(p.description, str_of(&out["description"]), "{key}");
            }
            "GetString" => {
                let p = pm::get_string(&w, key, "", true);
                assert_eq!(
                    (Some(p.item), p.description),
                    (str_of(&out["item"]), str_of(&out["description"])),
                    "{key}"
                );
            }
            "ModifyBool" => assert_eq!(
                Value::from(pm::modify_bool(&w, key, value.as_bool().unwrap())),
                *out,
                "{key}"
            ),
            "ModifyLong" => assert_eq!(
                Value::from(pm::modify_long(&w, key, i64_of(value).unwrap())),
                *out,
                "{key}"
            ),
            "ModifyDouble" => assert_eq!(
                Value::from(pm::modify_double(&w, key, f64_of(value).unwrap(), true)),
                *out,
                "{key}"
            ),
            "ModifyString" => assert_eq!(
                Value::from(pm::modify_string(&w, key, value.as_str().unwrap())),
                *out,
                "{key}"
            ),
            "ModifyBoolDescription" => pm::modify_bool_description(&w, key, value.as_str()),
            "ModifyLongDescription" => pm::modify_long_description(&w, key, value.as_str()),
            other => panic!("unknown op {other}"),
        }
    }
}

#[test]
fn modify_persists_on_resync_and_a_restart_reads_it_back() {
    for config in [
        shard_config_handle(Box::new(MemShard::new())),
        shard_config_handle(Box::new(SqliteShard::open_in_memory().expect("sqlite"))),
    ] {
        let mut w = started(&config);
        assert!(pm::modify_bool(&w, "world_closed", true));
        assert!(pm::modify_long(&w, "max_chars_per_account", 20));
        assert!(pm::modify_double(&w, "xp_modifier", 2.5, false));
        assert!(pm::modify_string(&w, "server_motd", "hello"));
        // Modified in the cache only until the worker runs.
        assert!(db(&config).get_bool("world_closed").is_none());
        assert_eq!(
            w.property_manager
                .cached_bool("world_closed")
                .map(|e| e.modified),
            Some(true)
        );

        pm::resync_variables(&mut w);

        // Every default was modified by LoadDefaultProperties, so all of them are written.
        assert_eq!(
            db(&config).get_all_bools().len(),
            dpm::DEFAULT_BOOLEAN_PROPERTIES.len()
        );
        assert_eq!(
            db(&config).get_all_longs().len(),
            dpm::DEFAULT_LONG_PROPERTIES.len()
        );
        assert_eq!(
            db(&config).get_all_doubles().len(),
            dpm::DEFAULT_DOUBLE_PROPERTIES.len()
        );
        assert_eq!(
            db(&config).get_all_strings().len(),
            dpm::DEFAULT_STRING_PROPERTIES.len()
        );
        let row = db(&config).get_bool("world_closed").expect("written");
        assert!(row.value);
        assert_eq!(
            row.description.as_deref(),
            Some("enable this to startup world as a closed to players world")
        );
        // LoadPropertiesFromDB replaced the entries: no longer modified.
        assert_eq!(
            w.property_manager
                .cached_bool("world_closed")
                .map(|e| e.modified),
            Some(false)
        );

        // A second save of a row that exists updates it (SaveBool, not AddBool).
        assert!(pm::modify_bool(&w, "world_closed", false));
        pm::do_work(&w);
        assert!(!db(&config).get_bool("world_closed").expect("row").value);
        assert!(pm::modify_bool(&w, "world_closed", true));
        pm::do_work(&w);
        drop(w);

        // A restart: the defaults, then the database over them.
        let w = started(&config);
        assert!(pm::get_bool(&w, "world_closed", false, true).item);
        assert_eq!(pm::get_long(&w, "max_chars_per_account", 0, true).item, 20);
        assert!(same_f64(
            pm::get_double(&w, "xp_modifier", 0.0, true).item,
            2.5
        ));
        assert_eq!(pm::get_string(&w, "server_motd", "", true).item, "hello");
        assert_eq!(
            w.property_manager
                .cached_long("max_chars_per_account")
                .map(|e| e.modified),
            Some(false)
        );
    }
}

#[test]
fn database_rows_override_defaults_and_bring_their_description() {
    let config = shard_config_handle(Box::new(MemShard::new()));
    db(&config).add_long("max_chars_per_account", 7, Some("from the shard"));
    let w = started(&config);
    let p = pm::get_long(&w, "max_chars_per_account", 0, true);
    assert_eq!(
        (p.item, p.description.as_deref()),
        (7, Some("from the shard"))
    );
}

/// Divergence: V399
/// An era's own defaults replace ACE's, and a value in the shard's configuration still wins.
#[test]
fn an_eras_property_defaults_replace_aces_and_the_database_still_wins() {
    let config = shard_config_handle(Box::new(MemShard::new()));
    db(&config).add_bool("allow_fast_chug", true, None);
    let mut w = world_at(1000);
    w.era = empyrean_common::era::EraId::Infiltration.rules();
    pm::install_shard_config(&mut w, config.clone());
    pm::initialize(&mut w, true);
    let b = |w: &World, k: &str| pm::get_bool(w, k, false, true).item;
    assert!(b(&w, "item_dispel"));
    assert!(!b(&w, "corpse_destroy_pyreals"));
    assert!(b(&w, "vendor_shop_uses_generator"));
    assert!(b(&w, "allow_fast_chug"), "the shard's own value");

    let w = started(&shard_config_handle(Box::new(MemShard::new())));
    assert!(!b(&w, "item_dispel"));
    assert!(b(&w, "corpse_destroy_pyreals"));
}

#[test]
fn get_of_an_unknown_key_reads_the_database_then_caches_as_ace_does() {
    let config = shard_config_handle(Box::new(MemShard::new()));
    let mut w = started(&config);

    // Not in the database: the fallback, cached as modified (so the next DoWork writes it).
    assert!(pm::get_bool(&w, "no_such_bool", true, true).item);
    assert_eq!(
        w.property_manager.cached_bool("no_such_bool"),
        Some(ConfigurationEntry {
            modified: true,
            item: true,
            description: None
        })
    );
    // cacheFallback false: not cached.
    assert_eq!(pm::get_long(&w, "no_such_long", 5, false).item, 5);
    assert!(w.property_manager.cached_long("no_such_long").is_none());

    // In the database: read, and cached as not modified; later reads come from the cache.
    db(&config).add_string("shard_only", "x", Some("d"));
    let p = pm::get_string(&w, "shard_only", "fallback", true);
    assert_eq!(
        (p.item.as_str(), p.description.as_deref()),
        ("x", Some("d"))
    );
    assert_eq!(
        w.property_manager
            .cached_string("shard_only")
            .map(|e| e.modified),
        Some(false)
    );
    let changed = empyrean_store::models::shard::ConfigPropertiesString {
        key: "shard_only".into(),
        value: "y".into(),
        description: Some("d".into()),
    };
    db(&config).save_string(&changed);
    assert_eq!(
        pm::get_string(&w, "shard_only", "", true).item,
        "x",
        "cached until the next resync"
    );

    // Modify only knows DefaultPropertyManager's keys (ordinal).
    assert!(!pm::modify_bool(&w, "no_such_bool", false));
    assert!(!pm::modify_bool(&w, "WORLD_CLOSED", true));
    assert!(!pm::modify_long(&w, "xp_modifier", 1));

    // The next DoWork writes the cached fallback and reloads the changed row.
    pm::resync_variables(&mut w);
    assert!(
        db(&config)
            .get_bool("no_such_bool")
            .expect("fallback written")
            .value
    );
    assert_eq!(pm::get_string(&w, "shard_only", "", true).item, "y");
}

#[test]
fn modify_description_marks_the_entry_or_warns() {
    let w = world_at(0);
    pm::modify_double_description(&w, "xp_modifier", Some("new"));
    let e = w.property_manager.cached_double("xp_modifier").unwrap();
    assert_eq!((e.description.as_deref(), e.modified), (Some("new"), true));
    pm::modify_string_description(&w, "missing", Some("x"));
    assert!(w.property_manager.cached_string("missing").is_none());
}

/// `ModifyDouble(key, value)` (init false) of `cantrip_drop_rate` runs
/// `CantripChance.ApplyNumCantripsMod`, of a `*_cantrip_drop_rate` level key
/// `ApplyCantripLevelsMod`; with `init` (LoadDefaultProperties, the database load) neither runs.
/// The tables are process-wide (ACE's statics), so the test holds [`crate::CANTRIP_TABLES`] for
/// writing and restores the default rates before releasing it.
#[test]
fn modify_double_rescales_the_cantrip_tables_unless_initialising() {
    let _tables = crate::cantrip_tables_write();
    let w = world_at(0);
    dpm::load_default_properties(&w.property_manager);
    let (num0, levels0) = cantrip_chance::current_tables(&w);

    assert!(pm::modify_double(&w, "cantrip_drop_rate", 2.0, true));
    assert!(pm::modify_double(&w, "major_cantrip_drop_rate", 2.0, true));
    assert_eq!(
        cantrip_chance::current_tables(&w),
        (num0.clone(), levels0.clone()),
        "init: no rescaling"
    );
    assert!(pm::modify_double(&w, "major_cantrip_drop_rate", 1.0, true));

    // ScaleNumCantrips: T1 (0, 0.95) (1, 0.05) at rate 2 -> (0, 1 - 0.1) (1, 0.05 * 2).
    assert!(pm::modify_double(&w, "cantrip_drop_rate", 2.0, false));
    let (num, levels) = cantrip_chance::current_tables(&w);
    assert_eq!(num[0], [(0, 1.0f32 - 0.05f32 * 2.0), (1, 0.05f32 * 2.0)]);
    assert_eq!(levels, levels0, "only NumCantrips");

    // ScaleCantripLevels: T1 is minor only (1, 1.0): a legendary rate leaves it; the last tier's
    // legendary chance doubles before the rescale to 1.
    assert!(pm::modify_double(
        &w,
        "legendary_cantrip_drop_rate",
        2.0,
        false
    ));
    let (_, levels) = cantrip_chance::current_tables(&w);
    assert_eq!(levels[0], levels0[0]);
    assert_ne!(levels[7], levels0[7], "T8 has legendary cantrips");
    let total: f32 = levels[7].iter().map(|e| e.1).sum();
    assert!((total - 1.0).abs() < 1e-6, "rescaled to 1: {total}");

    // Other keys rescale nothing; the default rates restore ACE's literal tables.
    let before = cantrip_chance::current_tables(&w);
    assert!(pm::modify_double(&w, "xp_modifier", 2.0, false));
    assert_eq!(cantrip_chance::current_tables(&w), before);
    assert!(pm::modify_double(&w, "cantrip_drop_rate", 1.0, false));
    assert!(pm::modify_double(
        &w,
        "legendary_cantrip_drop_rate",
        1.0,
        false
    ));
    assert_eq!(cantrip_chance::current_tables(&w), (num0, levels0));
    assert!(!take_local().contains_key("ACE: CantripChance.ApplyNumCantripsMod"));
}

/// No container special case: ACE forces `/ace/Content` in a container;
/// here the folder is the stored or default one, like any install.
#[test]
fn the_content_folder_has_no_container_special_case() {
    let mut w = world_at(0);
    pm::initialize(&mut w, true);
    assert_eq!(
        pm::get_string(&w, "content_folder", "", true).item,
        "Content"
    );
}

/// The worker timer: `DoWork` every 300 s from `Initialize`; `ResyncVariables` restarts the
/// period; `StopUpdating` stops it.
#[test]
fn the_worker_runs_every_300_seconds_from_initialize() {
    let config = shard_config_handle(Box::new(MemShard::new()));
    let mut w = started(&config); // at t = 1000 s
    assert_eq!(
        w.property_manager.worker_timer_due(),
        Some(Duration::from_secs(1300))
    );
    assert_eq!(WORKER_INTERVAL, Duration::from_secs(300));

    let at = |w: &mut World, secs: f64| {
        w.now.monotonic = Duration::from_secs_f64(secs);
        pm::run_worker_timer(w);
    };
    let written = |key: &str| db(&config).get_bool(key).map(|r| r.value);

    pm::modify_bool(&w, "world_closed", true);
    at(&mut w, 1299.999);
    assert_eq!(written("world_closed"), None, "not before 300 s");
    at(&mut w, 1300.0);
    assert_eq!(written("world_closed"), Some(true), "at 300 s");
    assert_eq!(
        w.property_manager.worker_timer_due(),
        Some(Duration::from_secs(1600))
    );

    // A late iteration keeps the period (AutoReset): due 1600, then 1900.
    pm::modify_bool(&w, "world_closed", false);
    at(&mut w, 1650.0);
    assert_eq!(written("world_closed"), Some(false));
    assert_eq!(
        w.property_manager.worker_timer_due(),
        Some(Duration::from_secs(1900))
    );

    // ResyncVariables: DoWork now, and the next Elapsed a whole interval later.
    at(&mut w, 1700.0);
    pm::modify_bool(&w, "world_closed", true);
    pm::resync_variables(&mut w);
    assert_eq!(written("world_closed"), Some(true));
    assert_eq!(
        w.property_manager.worker_timer_due(),
        Some(Duration::from_secs(2000))
    );

    // StopUpdating: nothing more is written.
    pm::stop_updating(&mut w);
    assert_eq!(w.property_manager.worker_timer_due(), None);
    pm::modify_bool(&w, "world_closed", false);
    at(&mut w, 5000.0);
    assert_eq!(written("world_closed"), Some(true));
}

#[test]
fn before_initialize_the_defaults_are_read_and_there_is_no_timer() {
    let mut w = world_at(0);
    assert!(pm::get_bool(&w, "use_turbine_chat", false, true).item);
    assert_eq!(pm::get_long(&w, "max_chars_per_account", 0, true).item, 11);
    assert_eq!(w.property_manager.worker_timer_due(), None);
    w.now.monotonic = Duration::from_secs(10_000);
    pm::run_worker_timer(&mut w);
    let config = w.property_manager.shard_config.clone();
    assert!(db(&config).get_all_bools().is_empty());
}
