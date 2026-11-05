//! Behaviour: none (checks the world database's tooling: its reader, checks, merge, compile and release archive)

use std::collections::HashMap;
use std::path::Path;

use auberean_assets::entity::Entity;
use auberean_assets::property::{
    Property, PropertyCollection, PropertyGroup, PropertyValue, StringInfo,
};
use auberean_assets::script::Qualities;
use auberean_primitives::DataId;
use dereth_primitives::num::math;
use yalain_content::quests::{Need, Trigger, Update};
use yalain_content::talk::{How, Role, TalkUpdate, When};
use yalain_content::{FactIds, Kind, Words};
use yalain_world_db::check::{check, check_read};
use yalain_world_db::compile::{compile, Install, InstallSale, QuestDefinition};
use yalain_world_db::merge::merge;
use yalain_world_db::release::{self, Host};
use yalain_world_db::rows::{read, Row};
use yalain_world_db::Files;

/// The test world's files, read from the server's test kit.
fn fixture() -> Files {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testkit/fixtures/world");
    let mut out = Files::new();
    let mut stack = vec![root.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, std::fs::read_to_string(&p).unwrap());
            }
        }
    }
    out
}

fn edit(files: &mut Files, path: &str, from: &str, to: &str) {
    let t = files.get_mut(path).unwrap();
    assert!(t.contains(from), "{path} has no {from:?}");
    *t = t.replacen(from, to, 1);
}

const NAME: u32 = 0x4100_0001;
const STORE_TEMPLATE: u32 = 0x4100_0002;
const LOOK: u32 = 0x4100_020B;
const CLASS: u32 = 0x4000_0002;
const DRAWABLE: u32 = 0x4000_0001;

fn entity(kind: u32, object: u32, props: Vec<(u32, PropertyValue)>) -> Entity {
    Entity {
        kind: Some(kind),
        object: Some(object),
        properties: Some(PropertyCollection {
            groups: vec![PropertyGroup {
                group: 0,
                properties: props
                    .into_iter()
                    .map(|(id, value)| Property {
                        id,
                        value,
                        ..Property::default()
                    })
                    .collect(),
                optional: Vec::new(),
            }],
        }),
        ..Entity::default()
    }
}

fn name(string: u32, table: u32) -> PropertyValue {
    PropertyValue::StringInfo {
        info: StringInfo {
            token: string,
            table: DataId(table),
            literal: None,
            variables: Vec::new(),
        },
        token_name: String::new(),
    }
}

/// An install made up for the tests: a storekeeper template naming its store, a portal, a creature
/// and two products, the store's sales, a quest's definition and one profile name.
struct Fake {
    entities: HashMap<u32, Entity>,
    /// What the quest's setup says of a kill's reach.
    fellows: yalain_content::quests::Fellows,
    /// The least level the quest's setup names.
    min_level: i32,
    setup_xp: u64,
    setup_pay: yalain_content::quests::CompletionPay,
}

impl Fake {
    fn new() -> Self {
        let look = (LOOK, PropertyValue::DataFile(DataId(0x4700_0001)));
        let mut entities = HashMap::new();
        entities.insert(0x4700_0001, entity(DRAWABLE, 0x1F00_0123, vec![]));
        entities.insert(
            0x4700_33D0,
            entity(
                CLASS,
                0x13A4,
                vec![
                    look.clone(),
                    (STORE_TEMPLATE, PropertyValue::DataFile(DataId(0x6F00_16F6))),
                ],
            ),
        );
        entities.insert(0x4700_115B, entity(CLASS, 0x0F00, vec![look.clone()]));
        entities.insert(0x4700_2224, entity(CLASS, 0x0E00, vec![look]));
        entities.insert(
            0x4700_20BB,
            entity(CLASS, 0x0D00, vec![(NAME, name(0xA860_7CEC, 0x2500_0857))]),
        );
        entities.insert(
            0x4700_20B9,
            entity(CLASS, 0x0D00, vec![(NAME, name(0x026F_0DD2, 0x2500_1BBC))]),
        );
        Self {
            entities,
            fellows: yalain_content::quests::Fellows::default(),
            min_level: 0,
            setup_xp: 0,
            setup_pay: yalain_content::quests::CompletionPay::default(),
        }
    }
}

impl Install for Fake {
    fn entity(&self, id: u32) -> Option<Entity> {
        self.entities.get(&id).cloned()
    }

    fn qualities(&self, _id: u32) -> Option<Qualities> {
        None
    }

    fn fact_ids(&self) -> FactIds {
        FactIds {
            name: NAME,
            ..FactIds::default()
        }
    }

    fn property_name(&self, id: u32) -> Option<String> {
        match id {
            NAME => Some("Name".into()),
            STORE_TEMPLATE => Some("StoreTemplate".into()),
            _ => None,
        }
    }

    fn profile_name(&self, value: u32) -> Option<String> {
        (value == 0x19B).then(|| "GP_TestBeetleHoard".into())
    }

    fn store_sales(&self, store: u32) -> Option<Vec<InstallSale>> {
        (store == 0x6F00_16F6).then(|| {
            vec![
                InstallSale {
                    ordinal: 0,
                    product: 0x4700_20BB,
                    cost: 10.0,
                    currency: 0x4700_0538,
                },
                InstallSale {
                    ordinal: 1,
                    product: 0x4700_20B9,
                    cost: 50.0,
                    currency: 0x4700_0538,
                },
            ]
        })
    }

    fn quest_definition(&self, state: Option<u32>, id: u32) -> Option<QuestDefinition> {
        (id == 0x4000_0088 && state.is_none()).then(|| QuestDefinition {
            state: 0x6F00_0D3F,
            name: (0xA860_7CEC, 0x2500_17EE),
            description: (0x6C45_9364, 0x2500_17EE),
            icon: 0x4100_034B,
            play_fx: true,
            journals: vec![(0x2E5C_1E52, 0x2500_17EE), (0x2E5F_1E52, 0x2500_17EE)],
            effects: Vec::new(),
            subquests: Vec::new(),
            once: false,
            fellows: self.fellows,
            last_waits: false,
            min_level: self.min_level,
            hero_only: false,
            monarch_only: false,
            xp: self.setup_xp,
            pay: self.setup_pay,
            required: None,
        })
    }

    fn label(&self, template: u32) -> String {
        format!("template {template:08X}")
    }
}

#[test]
fn the_test_world_reads_checks_and_merges_without_problems() {
    let files = fixture();
    assert_eq!(check(&files), Vec::new());
    let db = read(&files).unwrap();
    assert_eq!(db.world.era, "eor");
    assert_eq!(db.packs.len(), 2);
    assert_eq!(db.packs[0].rows.len(), 16);
}

#[test]
fn a_database_already_read_is_checked_and_merged_as_its_files_are() {
    let files = fixture();
    let db = read(&files).unwrap();
    let (problems, merged) = check_read(&db);
    assert_eq!(problems, check(&files));
    assert_eq!(merged, Some(merge(&db).unwrap()));
    // With no word from the world, the later pack's rows conflict: no rows merged, the problem said.
    let mut bad = files.clone();
    let world = files["world.toml"].clone();
    let cut = world.find("[[override]]").unwrap();
    bad.insert("world.toml".into(), world[..cut].to_string());
    let db = read(&bad).unwrap();
    let (problems, merged) = check_read(&db);
    assert!(!problems.is_empty(), "it says what is wrong");
    assert_eq!(problems, check(&bad));
    assert_eq!(merged, None);
}

#[test]
fn a_later_pack_takes_an_item_only_where_the_world_says_it_wins() {
    let mut files = fixture();
    let merged = merge(&read(&files).unwrap()).unwrap();
    let camps: Vec<_> = merged
        .rows
        .iter()
        .filter_map(|l| match &l.row {
            Row::Camp(c) => Some((c.population, l.pack.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(camps, [(3, "town-fixes")]);
    let keys: Vec<_> = merged
        .rows
        .iter()
        .filter_map(|l| match &l.row {
            Row::Object(o) => Some(o.key.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(keys, ["keeper", "gate"], "the sign is taken away");

    // Without the override, the two packs setting the camp stop the merge, naming both rows.
    let world = files["world.toml"].clone();
    let cut = world.find("[[override]]").unwrap();
    files.insert("world.toml".into(), world[..cut].to_string());
    let problems = check(&files);
    assert_eq!(problems.len(), 1, "{problems:?}");
    let p = &problems[0];
    assert_eq!(
        (p.file.as_str(), p.line),
        ("town-fixes/server/landblocks/77B7.toml", 1)
    );
    assert!(
        p.message.contains("camp beetles@77B7")
            && p.message.contains("town/server/landblocks/77B7.toml:21"),
        "{p}"
    );
}

#[test]
fn a_row_breaking_the_schema_is_a_problem_at_its_file_and_line() {
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/places.toml",
        "heading = 320.5",
        "heading = 320.5\nlabel = \"the inn\"",
    );
    edit(
        &mut files,
        "town/server/loot.toml",
        "profile = 0x19B",
        "profile = \"lots\"",
    );
    edit(
        &mut files,
        "town/server/stores.toml",
        "prov = { from = \"authored\" }\n\n[[sale]]",
        "\n[[sale]]",
    );
    files.insert(
        "town/server/extra.toml".into(),
        "[[treasure]]\nclass = 1\n".into(),
    );
    let problems = check(&files);
    let at: Vec<_> = problems
        .iter()
        .map(|p| (p.file.as_str(), p.line, p.message.as_str()))
        .collect();
    assert_eq!(
        at,
        [
            ("town/server/extra.toml", 1, "treasure is not a record kind"),
            (
                "town/server/loot.toml",
                1,
                "profile: \"lots\" is not a count (a whole number from 0)"
            ),
            (
                "town/server/places.toml",
                1,
                "label is not a field of this record kind"
            ),
            ("town/server/stores.toml", 1, "prov: missing"),
        ]
    );
}

#[test]
fn references_and_provenance_are_checked() {
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/landblocks/77B7.toml",
        "to = \"inn\"",
        "to = \"tavern\"",
    );
    edit(
        &mut files,
        "town/server/loot.toml",
        "prov = { from = \"authored\" }",
        "prov = { from = \"capture\", evidence = \"x\", ref = \"p:19b\", confidence = \"low\" }",
    );
    edit(
        &mut files,
        "town/server/landblocks/77B7.toml",
        "rule = \"camp-regen-median\"",
        "rule = \"guessed\"",
    );
    let messages: Vec<String> = check(&files).iter().map(ToString::to_string).collect();
    assert_eq!(
        messages,
        [
            "town/server/landblocks/77B7.toml:10: to: no place tavern in pack town",
            "town/server/landblocks/77B7.toml:21: regen: prov: derived by Some(\"guessed\"), not a listed rule",
            "town/server/loot.toml:1: prov: a measured row names its evidence handle, not Some(\"x\")",
        ]
    );
    // A provenance that does not read is a problem of reading, held alone.
    edit(
        &mut files,
        "town/server/loot.toml",
        "confidence = \"low\"",
        "confidence = \"sure\"",
    );
    let messages: Vec<String> = check(&files).iter().map(ToString::to_string).collect();
    assert_eq!(
        messages,
        [
            "town/server/loot.toml:1: prov: unknown variant `sure`, expected one of `low`, `medium`, `high`",
        ]
    );
    // Read in the shared grammar: a measurement names its handle, its ref and its confidence.
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/loot.toml",
        "prov = { from = \"authored\" }",
        "prov = { from = \"capture\", ref = \"p:19b\", confidence = \"low\" }",
    );
    let messages: Vec<String> = check(&files).iter().map(ToString::to_string).collect();
    assert_eq!(
        messages,
        ["town/server/loot.toml:1: prov: provenance from capture names its evidence"]
    );
}

#[test]
fn a_database_of_another_game_or_era_is_refused() {
    let mut files = fixture();
    edit(&mut files, "world.toml", "era = \"eor\"", "era = \"2005\"");
    let messages: Vec<String> = check(&files).iter().map(ToString::to_string).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.starts_with("world.toml: era \"2005\" is not one of [\"eor\"]")),
        "{messages:?}"
    );
    let mut files = fixture();
    edit(
        &mut files,
        "town/pack.toml",
        "game = \"ac2\"",
        "game = \"ac1\"",
    );
    let messages: Vec<String> = check(&files).iter().map(ToString::to_string).collect();
    assert_eq!(
        messages,
        ["town/pack.toml: this pack is for \"ac1\"; Yalain reads packs for \"ac2\""]
    );
    let mut files = fixture();
    edit(&mut files, "world.toml", "game = \"ac2\"\n", "");
    let messages: Vec<String> = check(&files).iter().map(ToString::to_string).collect();
    assert_eq!(
        messages,
        ["world.toml: this world is for \"ac1\"; Yalain reads worlds for \"ac2\""]
    );
}

#[test]
fn the_compile_takes_what_the_rows_leave_out_from_the_install() {
    let merged = merge(&read(&fixture()).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    assert_eq!(
        t.counts(),
        [
            ("content", 2),
            ("spawns", 1),
            ("loot", 1),
            ("keepers", 1),
            ("sales", 2),
            ("quests", 1),
            ("steps", 4),
            ("gives", 0),
            ("talk", 2)
        ]
    );
    // The keeper, described from its template; its cell given, its rotation from its heading.
    let keeper = &t.content.objects[0];
    assert_eq!(
        (keeper.kind, keeper.object, keeper.cell),
        (Kind::Npc, None, 0x77B7_0036)
    );
    assert_eq!(
        (keeper.description.package, keeper.description.look),
        (0x13A4, 0x1F00_0123)
    );
    assert_eq!(keeper.label, "template 470033D0");
    assert_eq!(keeper.placed, "authored by tests");
    assert_eq!(
        keeper.rotation,
        yalain_world_db::compile::heading_rotation(165.0)
    );
    let half = -165.0_f32.to_radians() / 2.0;
    assert!((keeper.rotation[0] - math::cosf(half)).abs() < 1e-6 && keeper.rotation[3] < 0.0);
    // The portal: its own id and name, its cell from its point, and where it leads.
    let gate = &t.content.objects[1];
    assert_eq!(
        (gate.kind, gate.object),
        (Kind::Static, Some(0x4077_B700_0000_0001))
    );
    assert_eq!(gate.cell, 0x77B7_0000 | (6 * 8 + 5 + 1));
    assert_eq!(gate.description.name, Some((0xF3E4_3482, 0x2500_1B41)));
    let to = gate.destination.unwrap();
    assert_eq!((to.cell, to.origin), (0x77B8_0003, [1.18, 53.42, 183.82]));
    // The camp the fixes pack moved, its cell from its point.
    let camp = &t.spawns.camps[0];
    assert_eq!(
        (camp.id.as_str(), camp.cell, camp.population, camp.regen),
        ("beetles", 0x77B7_0000 | (5 * 8 + 5 + 1), 3, 120.0)
    );
    assert_eq!(camp.confidence, "authored");
    // The loot profile, named by the install.
    assert_eq!(t.loot.profiles[0].name, "GP_TestBeetleHoard");
    // The keeper's store, from its template; the first sale wholly from the install, the second
    // with its own price, and its text left for an overlay (a warning, not a stop).
    assert_eq!(t.stores.keepers[0].store, 0x6F00_16F6);
    assert_eq!(
        t.stores.keepers[0].greeting,
        Some(Words::Ref(0x2824_938C, 0x2500_2AFC))
    );
    let s: Vec<_> = t
        .stores
        .sales
        .iter()
        .map(|s| (s.ordinal, s.product, s.cost, s.currency, s.name, s.stack))
        .collect();
    assert_eq!(
        s,
        [
            (
                0,
                0x4700_20BB,
                10.0,
                0x4700_0538,
                (0xA860_7CEC, 0x2500_0857),
                1
            ),
            (
                1,
                0x4700_20B9,
                75.0,
                0x4700_0538,
                (0x026F_0DD2, 0x2500_1BBC),
                1
            ),
        ]
    );
    assert_eq!(t.stores.sales[0].icon, [(1000, 0x4100_4552, 0xFFFF_FFFF)]);
    assert!(
        t.warnings
            .iter()
            .any(|w| w.message.starts_with("name_text")),
        "{:?}",
        t.warnings
    );
    // The quest's definition from the install, its numbers from the row.
    let q = &t.quests.quests[0];
    assert_eq!(
        (q.name, q.icon, q.journals.len()),
        ((0xA860_7CEC, 0x2500_17EE), 0x4100_034B, 2)
    );
    assert_eq!(
        (q.challenge, q.duration, q.retry, q.xp, q.cancellable),
        (-999, 72000.0, 0.0, 0, false)
    );
    let step = &t.quests.steps[0];
    assert_eq!(
        (step.trigger, step.update, step.tell.clone()),
        (
            Trigger::Use(0x4700_33D0),
            Update::Bestow,
            Some(Words::Ref(0x2824_FD4C, 0x2500_2881))
        )
    );
    // The tables read back as the server reads them.
    assert_eq!(
        yalain_content::SpawnTable::from_text(&t.spawns.to_text()).unwrap(),
        t.spawns
    );
    assert_eq!(
        yalain_content::StoreTable::from_text(&t.stores.to_text()).unwrap(),
        t.stores
    );
}

#[test]
fn a_row_naming_what_the_install_lacks_stops_the_compile_at_its_line() {
    let mut files = fixture();
    edit(
        &mut files,
        "town-fixes/server/landblocks/77B7.toml",
        "template = 0x47002224",
        "template = 0x47009999",
    );
    edit(
        &mut files,
        "town/server/loot.toml",
        "profile_name = \"GP_TestBeetleHoard\"",
        "profile_name = \"GP_SomethingElse\"",
    );
    let merged = merge(&read(&files).unwrap()).unwrap();
    let problems = compile(&merged, &Fake::new()).unwrap_err();
    let messages: Vec<String> = problems.iter().map(ToString::to_string).collect();
    assert_eq!(
        messages,
        [
            "town-fixes/server/landblocks/77B7.toml:1: spawns: template 47009999 is not in the install",
            "town/server/loot.toml:1: profile_name: the install names 0x19b GP_TestBeetleHoard, not GP_SomethingElse",
        ]
    );
}

#[test]
fn the_release_archive_is_the_same_bytes_for_the_same_files() {
    let files = fixture();
    let db = read(&files).unwrap();
    let carried = release::release_files(&files, &db);
    assert!(carried.contains_key("world.toml") && carried.contains_key("town/pack.toml"));
    let manifest = release::manifest("v0.1.0", &db);
    let a = release::archive(&carried, &manifest);
    let b = release::archive(&carried.clone(), &manifest.clone());
    assert_eq!(a, b);
    // The archive holds the files and the manifest, and reads back.
    let back = release::read_stored(&a).unwrap();
    assert_eq!(back.len(), carried.len() + 1);
    assert_eq!(back["manifest.json"], manifest);
    assert_eq!(
        back["town/server/loot.toml"],
        files["town/server/loot.toml"]
    );
    let m: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(m["rows"]["camp"], 2);
    assert_eq!(m["rows"]["remove"], 1);
    assert_eq!(m["provenance"]["authored"], 17);
    assert_eq!(m["rows"]["talk"], 2);
    assert_eq!(m["era"], "eor");
    // One file changed gives another archive.
    let mut changed = carried;
    changed.insert("world.lock".into(), "# changed\n".into());
    assert_ne!(release::archive(&changed, &manifest), a);
    // A damaged entry is not read back.
    let mut bad = a;
    let at = bad.len() / 3;
    bad[at] ^= 0xFF;
    assert_ne!(release::read_stored(&bad), Some(back));
}

#[test]
fn a_tag_is_three_numbers_with_or_without_the_v_and_the_pin_is_a_tag_and_a_sha256() {
    assert_eq!(release::parse_tag("0.1.0").as_deref(), Some("v0.1.0"));
    assert_eq!(release::parse_tag("v1.10.2").as_deref(), Some("v1.10.2"));
    for bad in ["", "v", "latest", "0.1", "0.1.0.1", "v0.x.0", "0.1.0/../x"] {
        assert_eq!(release::parse_tag(bad), None, "{bad:?}");
    }
    assert_eq!(
        release::parse_tag(release::PINNED_TAG).as_deref(),
        Some(release::PINNED_TAG)
    );
    assert_eq!(release::PINNED_ZIP_SHA256.len(), 64);
    assert!(release::PINNED_ZIP_SHA256
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    assert_eq!(release::zip_name("v0.1.0"), "yalain-world-v0.1.0.zip");
    assert_eq!(
        release::source_url("file:///d/yalain-world-{tag}.zip", "v0.2.0"),
        "file:///d/yalain-world-v0.2.0.zip"
    );
}

#[test]
fn the_cache_is_the_hosts_per_user_cache_folder() {
    fn env(vars: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<std::ffi::OsString> {
        let vars = vars.to_vec();
        move |n| {
            vars.iter()
                .find(|(k, _)| *k == n)
                .map(|(_, v)| std::ffi::OsString::from(v))
        }
    }
    let (win, home, xdg) = if cfg!(windows) {
        (r"C:\Users\p\AppData\Local", r"C:\Users\p", r"C:\x")
    } else {
        ("/win/local", "/home/p", "/xdg")
    };
    assert_eq!(
        release::cache_dir_for(Host::Windows, env(&[("LOCALAPPDATA", win)])),
        Some(Path::new(win).join("Yalain").join("world-database"))
    );
    assert_eq!(
        release::cache_dir_for(Host::MacOs, env(&[("HOME", home)])),
        Some(Path::new(home).join("Library/Caches/Yalain/world-database"))
    );
    assert_eq!(
        release::cache_dir_for(Host::Unix, env(&[("HOME", home)])),
        Some(
            Path::new(home)
                .join(".cache")
                .join("yalain")
                .join("world-database")
        )
    );
    assert_eq!(
        release::cache_dir_for(Host::Unix, env(&[("HOME", home), ("XDG_CACHE_HOME", xdg)])),
        Some(Path::new(xdg).join("yalain").join("world-database"))
    );
    assert_eq!(
        release::cache_dir_for(Host::Windows, env(&[("HOME", home)])),
        None
    );
}

#[test]
fn a_step_can_be_moved_by_an_item_given_to_a_person() {
    let merged = merge(&read(&fixture()).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    // The item's template and the person's.
    let s4 = &t.quests.steps[3];
    assert_eq!(
        (s4.trigger, s4.needs, s4.update),
        (Trigger::Give(0x4700_20B9, 0x4700_33D0), 2, Update::Complete)
    );
    assert_eq!(
        yalain_content::QuestTable::from_text(&t.quests.to_text()).unwrap(),
        t.quests,
        "it reads back as written"
    );
}

#[test]
fn a_portals_recall_flags_and_a_places_name_compile_for_the_server() {
    let t = compile(&merge(&read(&fixture()).unwrap()).unwrap(), &Fake::new()).unwrap();
    let gate = |t: &yalain_world_db::compile::Tables| {
        t.content
            .objects
            .iter()
            .find(|o| o.destination.is_some())
            .map(|o| o.facts.portal)
    };
    assert_eq!(
        gate(&t),
        Some(0),
        "a portal is recalled to only when its row says so"
    );
    assert!(t.places_by_name.is_empty());
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/landblocks/77B7.toml",
        "to = \"inn\"",
        "to = \"inn\"\nrecall = true\nsummon = true",
    );
    edit(
        &mut files,
        "town/server/places.toml",
        "heading = 320.5",
        "heading = 320.5\ndestination = \"Human_Recall_Town_01\"",
    );
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert_eq!(gate(&t), Some(0x3));
    let inn = t.places_by_name.get("human_recall_town_01").expect("named");
    assert_eq!((inn.cell, inn.origin), (0x77B8_0003, [1.18, 53.42, 183.82]));
    // Flags on an object that leads nowhere are a problem.
    let mut bad = fixture();
    edit(
        &mut bad,
        "town/server/landblocks/77B7.toml",
        "key = \"sign\"",
        "key = \"sign\"\nrecall = true",
    );
    assert!(check(&bad)
        .iter()
        .any(|p| p.to_string().contains("recall, summon: only a portal")));
}

#[test]
fn a_chests_row_compiles_with_its_level_and_band_or_is_a_problem_without_both() {
    let row = |extra: &str| {
        let mut files = fixture();
        let path = files
            .keys()
            .find(|k| k.ends_with("loot.toml"))
            .expect("loot rows")
            .clone();
        let text = files.get(&path).expect("read").replacen(
            "\nprov = ",
            &format!("\n{extra}\nprov = "),
            1,
        );
        files.insert(path, text);
        files
    };
    let files = row("level = 45\nvalue = [130, 189]");
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    let chest = t.loot.profiles[0].chest.expect("a chest's");
    assert_eq!((chest.level, chest.value), (45, (130, 189)));
    assert_eq!(
        yalain_content::LootTable::from_text(&t.loot.to_text())
            .unwrap()
            .profiles[0]
            .chest,
        Some(chest),
        "it reads back as written"
    );
    for bad in [
        "level = 45",
        "value = [130, 189]",
        "level = 45\nvalue = [9, 3]",
    ] {
        assert!(
            check(&row(bad))
                .iter()
                .any(|p| p.to_string().contains("level, value: a chest's")),
            "{bad}"
        );
    }
}

#[test]
fn a_places_row_of_rooms_compiles_with_where_each_ones_creature_stands() {
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/places.toml",
        "heading = 320.5",
        "heading = 320.5\nrow = \"arena\"\nholds = 0x77B80010",
    );
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert_eq!(t.quests.rooms.len(), 1);
    let room = t.quests.rooms[0][0];
    assert_eq!(
        (room.place.cell, room.place.origin, room.holds),
        (0x77B8_0003, [1.18, 53.42, 183.82], 0x77B8_0010)
    );
    assert_eq!(
        yalain_content::QuestTable::from_text(&t.quests.to_text())
            .unwrap()
            .rooms,
        t.quests.rooms,
        "it reads back as written"
    );
    let mut bad = fixture();
    edit(
        &mut bad,
        "town/server/places.toml",
        "heading = 320.5",
        "heading = 320.5\nrow = \"arena\"",
    );
    assert!(check(&bad)
        .iter()
        .any(|p| p.to_string().contains("row, holds: a room")));
}

#[test]
fn a_place_names_the_templates_whose_maps_lead_there_and_a_line_may_refuse_a_map() {
    let t = compile(&merge(&read(&fixture()).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert!(t.places_named.is_empty(), "no place names a template");
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/places.toml",
        "heading = 320.5",
        "heading = 320.5\nnamed_by = [0x47000C2A, 0x47000C31]",
    );
    edit(
        &mut files,
        "town/server/talk/470033D0.toml",
        "role = \"near\"",
        "role = \"refuse-map\"",
    );
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    let inn = t.places_named.get(&0x4700_0C2A).expect("named");
    assert_eq!((inn.cell, inn.origin), (0x77B8_0003, [1.18, 53.42, 183.82]));
    assert_eq!(t.places_named.get(&0x4700_0C31), Some(inn));
    assert_eq!(t.talk.lines[0].role, Some(Role::RefuseMap));
    // A template two places name is a problem.
    edit(
        &mut files,
        "town/server/places.toml",
        "prov = { from = \"authored\" }",
        "prov = { from = \"authored\" }\n\n[[place]]\nkey = \"cellar\"\ncell = 0x77B80003\nat = [2.0, 50.0, 183.8]\nnamed_by = [0x47000C31]\nprov = { from = \"authored\" }",
    );
    let p = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap_err();
    assert!(
        p.iter()
            .any(|p| p.to_string().contains("47000C31 is named by another place")),
        "{p:?}"
    );
}

#[test]
fn a_quest_row_may_name_the_one_phase_its_duration_runs_in() {
    let t = compile(&merge(&read(&fixture()).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert_eq!(t.quests.quests[0].timed_phase, 0, "left out: all of them");
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/quests/40000088.toml",
        "duration = 72000",
        "duration = 72000\ntimed_phase = 2",
    );
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert_eq!(t.quests.quests[0].timed_phase, 2);
    let text = t.quests.to_text();
    let again = yalain_content::QuestTable::from_text(&text).unwrap();
    assert_eq!(again.quests[0].timed_phase, 2, "kept through its text form");
}

#[test]
fn a_quest_row_may_say_it_is_given_once_and_a_line_may_say_it_is_too_soon() {
    let t = compile(&merge(&read(&fixture()).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert!(!t.quests.quests[0].once, "left out: given again");
    assert_eq!(t.talk.lines[0].role, Some(Role::Near));
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/quests/40000088.toml",
        "duration = 72000",
        "duration = 72000\nonce = true",
    );
    edit(
        &mut files,
        "town/server/talk/470033D0.toml",
        "role = \"near\"",
        "role = \"too-soon\"",
    );
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert!(t.quests.quests[0].once);
    assert_eq!(t.talk.lines[0].role, Some(Role::TooSoon));
    assert_eq!(
        yalain_content::QuestTable::from_text(&t.quests.to_text()).unwrap(),
        t.quests,
        "it reads back as written"
    );
}

#[test]
fn a_quest_row_may_say_whether_a_kills_move_on_it_reaches_fellows() {
    let t = compile(&merge(&read(&fixture()).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert!(!t.quests.quests[0].fellows.shared, "left out: the setup's");
    // A row's word over a setup that shares, with no limit: true keeps the setup's, false stops it.
    let sharing = Fake {
        fellows: yalain_content::quests::Fellows {
            shared: true,
            ..Default::default()
        },
        ..Fake::new()
    };
    let t = compile(&merge(&read(&fixture()).unwrap()).unwrap(), &sharing).unwrap();
    assert_eq!(
        t.quests.quests[0].fellows, sharing.fellows,
        "left out: the setup's"
    );
    for (written, shared) in [("fellows = true", true), ("fellows = false", false)] {
        let mut files = fixture();
        edit(
            &mut files,
            "town/server/quests/40000088.toml",
            "duration = 72000",
            &format!("duration = 72000\n{written}"),
        );
        let t = compile(&merge(&read(&files).unwrap()).unwrap(), &sharing).unwrap();
        let f = t.quests.quests[0].fellows;
        assert_eq!((f.shared, f.near_level, f.near_by), (shared, false, false));
    }
    for (written, shared) in [("fellows = true", true), ("fellows = false", false)] {
        let mut files = fixture();
        edit(
            &mut files,
            "town/server/quests/40000088.toml",
            "duration = 72000",
            &format!("duration = 72000\n{written}"),
        );
        let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
        let f = t.quests.quests[0].fellows;
        assert_eq!(f.shared, shared);
        assert_eq!(
            (f.near_level, f.near_by),
            (shared, shared),
            "a setup that says nothing"
        );
        assert_eq!(
            yalain_content::QuestTable::from_text(&t.quests.to_text()).unwrap(),
            t.quests,
            "it reads back as written"
        );
    }
}

#[test]
fn a_step_can_take_a_held_item_used_on_an_object_require_other_quests_and_send_the_player() {
    let merged = merge(&read(&fixture()).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    let s2 = &t.quests.steps[1];
    assert_eq!(s2.trigger, Trigger::UseOn(0x4700_20BB, 0x4700_115B));
    assert_eq!(s2.update, Update::Advance(2));
    assert_eq!(
        s2.requires,
        [(0x4000_0016, Need::Done), (0x4000_0017, Need::Phase(3))]
    );
    assert_eq!(s2.takes, [0x4700_20BB]);
    // Its place: the inn's cell and point, its heading 320.5 degrees as 255ths of a turn.
    let to = s2.to.unwrap();
    assert_eq!((to.cell, to.origin), (0x77B8_0003, [1.18, 53.42, 183.82]));
    assert_eq!(to.heading, 227);
    // New prose with no string of its own is said as literal text.
    assert_eq!(s2.tell, Some(Words::Literal("The way opens.".into())));
    let s3 = &t.quests.steps[2];
    assert_eq!(
        (s3.trigger, s3.update, s3.xp, s3.gives.as_slice()),
        (
            Trigger::Arrive(0x4700_115B),
            Update::Complete,
            40,
            &[0x4700_20B9][..]
        )
    );
    // The quest table reads back as written.
    assert_eq!(
        yalain_content::QuestTable::from_text(&t.quests.to_text()).unwrap(),
        t.quests
    );
    // A step naming a template the install lacks stops the compile; a place no pack has, the
    // check.
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/quests/40000088.toml",
        "on = 0x4700115B",
        "on = 0x47009998",
    );
    let problems = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap_err();
    assert_eq!(
        problems[0].to_string(),
        "town/server/quests/40000088.toml:17: trigger: template 47009998 is not in the install"
    );
    edit(
        &mut files,
        "town/server/quests/40000088.toml",
        "to = \"inn\"",
        "to = \"cellar\"",
    );
    let messages: Vec<String> = check(&files).iter().map(ToString::to_string).collect();
    assert_eq!(
        messages,
        ["town/server/quests/40000088.toml:17: to: no place cellar in pack town"]
    );
}

#[test]
fn a_line_is_a_string_or_new_prose_and_its_token_name_must_be_its_string_s() {
    let merged = merge(&read(&fixture()).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    let hello = &t.talk.lines[0];
    assert_eq!(
        (
            hello.who,
            hello.object,
            &hello.words,
            hello.role,
            hello.how,
            hello.when
        ),
        (
            0x4700_33D0,
            None,
            &Words::Ref(0x2824_938C, 0x2500_2AFC),
            Some(Role::Near),
            How::Tell,
            When::Use
        )
    );
    // The second line is one placed person's, new prose said aloud, second in its speech.
    let offer = &t.talk.lines[1];
    assert_eq!(
        offer.object,
        Some(0),
        "the keeper is the content pack's first object"
    );
    assert_eq!(offer.words, Words::Literal("Take this to the gate.".into()));
    assert_eq!(
        (
            offer.how,
            offer.quest,
            offer.update,
            offer.order,
            offer.measured
        ),
        (
            How::Say,
            Some(0x4000_0088),
            Some(TalkUpdate::Bestow),
            1,
            false
        )
    );
    assert_eq!(t.talk.of(0x4700_33D0, Some(0)).count(), 2);
    assert_eq!(t.talk.of(0x4700_33D0, Some(7)).count(), 1);
    assert_eq!(
        yalain_content::TalkTable::from_text(&t.talk.to_text()).unwrap(),
        t.talk
    );
    // A token name that is not the string's; a line with no words, of a person no pack places.
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/talk/470033D0.toml",
        "line_name = \"ID_Hello\"",
        "line_name = \"ID_Goodbye\"",
    );
    edit(
        &mut files,
        "town/server/talk/470033D0.toml",
        "object = \"keeper@77B7\"\nline_text = \"Take this to the gate.\"",
        "object = \"guard@77B7\"",
    );
    let messages: Vec<String> = check(&files).iter().map(ToString::to_string).collect();
    let goodbye = auberean_assets::strings::token("ID_Goodbye");
    assert_eq!(
        messages,
        [
            format!("town/server/talk/470033D0.toml:1: line_name: ID_Goodbye is the token {goodbye:#010X}, not the line's 0x2824938C"),
            "town/server/talk/470033D0.toml:10: line: a line is a string reference or line_text".into(),
            "town/server/talk/470033D0.toml:10: object: no object guard@77B7".into(),
        ]
    );
}

#[test]
fn packs_that_set_an_item_alike_agree_and_an_override_may_name_one_item_or_a_landblock() {
    // The same loot row in both packs: no conflict, without a word from the world.
    let mut files = fixture();
    let loot = files["town/server/loot.toml"].clone();
    files.insert("town-fixes/server/loot.toml".into(), loot);
    let world = files["world.toml"].clone();
    let cut = world.find("[[override]]").unwrap();
    files.insert("world.toml".into(), world[..cut].to_string());
    // The fixes pack's camp made the town's again: alike, no conflict.
    let camp = "[[camp]]\nkey = \"beetles\"\nat = [144.8, 144.7, 133.0]\nradius = 5.0\npopulation = 2\nregen = 192.0\nspawns = [{ template = 0x47002224, weight = 2 }]\nprofile = 0x19B\nprofile_name = \"GP_TestBeetleHoard\"\nprov = { from = \"authored\" }\n\n[[remove]]\nitems = [\"object sign@77B7\"]\n";
    files.insert(
        "town-fixes/server/landblocks/77B7.toml".into(),
        camp.to_string(),
    );
    assert_eq!(check(&files), Vec::new(), "rows set alike agree");
    // Moved again, the camp conflicts, and an override naming another item does not settle it.
    files.insert(
        "town-fixes/server/landblocks/77B7.toml".into(),
        camp.replace("population = 2", "population = 4"),
    );
    let with = |files: &Files, items: &str| {
        let mut f = files.clone();
        let w = f["world.toml"].clone();
        f.insert(
            "world.toml".into(),
            format!("{w}[[override]]\npack = \"town-fixes\"\nover = \"town\"\nitems = [{items}]\n"),
        );
        check(&f)
    };
    assert_eq!(with(&files, "\"keeper 0x470033D0\"").len(), 1);
    assert_eq!(with(&files, "\"camp beetles@77b7\""), Vec::new());
    assert_eq!(with(&files, "\"landblock 77B7\""), Vec::new());
    let bad = with(&files, "\"camp beetles\"");
    assert!(
        bad[0].message.contains("is not an item") && bad[0].message.contains("talk <key>"),
        "{bad:?}"
    );
}

#[test]
fn a_removal_names_its_items_in_the_one_form_the_report_does() {
    let mut files = fixture();
    edit(
        &mut files,
        "town-fixes/server/landblocks/77B7.toml",
        "items = [\"object sign@77B7\"]",
        "item = \"object sign@77b7\"",
    );
    let merged = merge(&read(&files).unwrap()).unwrap();
    assert!(!merged
        .rows
        .iter()
        .any(|l| matches!(&l.row, Row::Object(o) if o.key == "sign")));
    edit(
        &mut files,
        "town-fixes/server/landblocks/77B7.toml",
        "item = \"object sign@77b7\"",
        "item = \"landblock 77B7\"",
    );
    let messages: Vec<String> = check(&files).iter().map(ToString::to_string).collect();
    assert_eq!(
        messages,
        ["town-fixes/server/landblocks/77B7.toml:10: items: a [[remove]] takes out one item, not a landblock"]
    );
}

#[test]
fn a_doors_scene_is_kept_by_its_place_and_must_be_a_scene_served() {
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/landblocks/77B7.toml",
        "to = \"inn\"\n",
        "to = \"inn\"\nscene = 0x4000005A\n",
    );
    let merged = merge(&read(&files).unwrap()).unwrap();
    let problems = compile(&merged, &Fake::new()).unwrap_err();
    let messages: Vec<String> = problems.iter().map(ToString::to_string).collect();
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(messages[0].contains("scene: no scene 4000005A is served"));
    files.insert(
        "town/server/story.toml".into(),
        "[[scene]]\nid = 0x4000005A\nact = 0x4000000C\nnumber = 28\n\
         name = { table = 0x250018A7, string = 0xD2F7ECB9 }\n\
         keyframes = [{ seconds = 12.0, picture = 0x41004BD7, tracks = 0x01 }]\n\
         prov = { from = \"authored\" }\n"
            .into(),
    );
    let merged = merge(&read(&files).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    let gate = t
        .content
        .objects
        .iter()
        .find(|o| o.object == Some(0x4077_B700_0000_0001))
        .unwrap();
    assert_eq!(
        t.story.doors.get(&(gate.cell, gate.template)),
        Some(&0x4000_005A)
    );
}

#[test]
fn a_locked_doors_key_id_is_kept_by_its_id_and_only_a_static_is_locked() {
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/landblocks/77B7.toml",
        "to = \"inn\"\n",
        "to = \"inn\"\nlocked = true\nlock = 0x40000054\n",
    );
    edit(
        &mut files,
        "town/server/landblocks/77B7.toml",
        "heading = 165.0\n",
        "heading = 165.0\nlocked = true\n",
    );
    let merged = merge(&read(&files).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    assert_eq!(
        t.locks.of(0x4077_B700_0000_0001).and_then(|l| l.key),
        Some(0x4000_0054)
    );
    assert_eq!(t.locks.objects.len(), 1, "a person is never locked");
    assert!(t
        .warnings
        .iter()
        .any(|w| w.to_string().contains("locked: only a static object")));
}

#[test]
fn a_weapon_row_names_the_template_of_a_creatures_weapon_and_must_name_one_installed() {
    let mut files = fixture();
    files.insert(
        "town/server/weapons.toml".into(),
        "[[weapon]]\nname = \"ClubTestBrute\"\ntemplate = 0x4700115B\n\
         prov = { from = \"authored\" }\n\n\
         [[weapon]]\nname = \"SwordTestLost\"\ntemplate = 0x47009999\n\
         prov = { from = \"authored\" }\n"
            .into(),
    );
    let merged = merge(&read(&files).unwrap()).unwrap();
    let problems = compile(&merged, &Fake::new()).unwrap_err();
    let messages: Vec<String> = problems.iter().map(ToString::to_string).collect();
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(messages[0].contains("template: template 47009999 is not in the install"));
    files.insert(
        "town/server/weapons.toml".into(),
        "[[weapon]]\nname = \"ClubTestBrute\"\ntemplate = 0x4700115B\n\
         prov = { from = \"authored\" }\n"
            .into(),
    );
    let merged = merge(&read(&files).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    assert_eq!(t.weapons.get("ClubTestBrute"), Some(&0x4700_115B));
    // A name is an identifier of the install, never prose.
    files.insert(
        "town/server/weapons.toml".into(),
        "[[weapon]]\nname = \"a club\"\ntemplate = 0x4700115B\nprov = { from = \"authored\" }\n"
            .into(),
    );
    assert!(read(&files).is_err());
}

#[test]
fn an_area_row_sets_the_conflict_its_landblock_allows_and_peace_is_the_default() {
    use yalain_content::Conflict;
    let mut files = fixture();
    files.insert(
        "town/server/areas.toml".into(),
        "[[area]]\nlandblock = 0x77B8\nconflict = \"controlled\"\nprov = { from = \"authored\" }\n\n\
         [[area]]\nlandblock = 0x77B7\nconflict = \"free-for-all\"\nprov = { from = \"authored\" }\n\n\
         [[area]]\nlandblock = 0x77B6\nconflict = \"peace\"\nprov = { from = \"authored\" }\n"
            .into(),
    );
    assert_eq!(check(&files), Vec::new());
    let merged = merge(&read(&files).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    assert_eq!(
        t.content
            .conflict
            .iter()
            .map(|(k, v)| (*k, *v))
            .collect::<Vec<_>>(),
        [
            (0x77B7, Conflict::FreeForAll),
            (0x77B8, Conflict::Controlled)
        ],
        "a peace area is no entry"
    );
    // A later pack that overrides an earlier one names a landblock's area otherwise.
    files.insert(
        "town-fixes/server/areas.toml".into(),
        "[[area]]\nlandblock = 0x77B8\nconflict = \"kingdom-vs-kingdom\"\nprov = { from = \"authored\" }\n"
            .into(),
    );
    let merged = merge(&read(&files).unwrap()).unwrap();
    assert!(
        merged
            .overridden
            .iter()
            .any(|o| o.starts_with("area 77B8:")),
        "{:?}",
        merged.overridden
    );
    let t = compile(&merged, &Fake::new()).unwrap();
    assert_eq!(
        t.content.conflict.get(&0x77B8),
        Some(&Conflict::KingdomVsKingdom)
    );
    // Its words and its landblock are the format's.
    for bad in [
        "[[area]]\nlandblock = 0x77B8\nconflict = \"war\"\nprov = { from = \"authored\" }\n",
        "[[area]]\nlandblock = 0x77B80001\nconflict = \"peace\"\nprov = { from = \"authored\" }\n",
        "[[area]]\nlandblock = 0x77B8\nprov = { from = \"authored\" }\n",
    ] {
        files.insert("town-fixes/server/areas.toml".into(), bad.into());
        assert!(read(&files).is_err(), "{bad}");
    }
}

#[test]
fn a_camp_sets_how_its_creatures_move_and_names_a_route_they_walk() {
    use yalain_content::movement::{Gait, Shape};
    let mut files = fixture();
    edit(
        &mut files,
        "town-fixes/server/landblocks/77B7.toml",
        "prov = { from = \"authored\", note = \"moved up the hill\" }\n",
        "movement = \"free-range\"\nwanders = false\nperception = 12.0\nroute = \"rounds\"\n\
         route_start = 2\nroute_direction = \"backward\"\n\
         prov = { from = \"authored\", note = \"moved up the hill\" }\n",
    );
    let route = "[[route]]\nkey = \"rounds\"\nshape = \"loop\"\ngait = \"run\"\nrest = 20.0\n\
                 waypoints = [{ cell = 0x77B70031, at = [60.0, 0.0, 140.0] }, \
                 { cell = 0x77B70035, at = [60.0, 80.0, 140.0], rest = 5.0 }, \
                 { cell = 0x77B70065, at = [140.0, 80.0, 140.0] }]\n\
                 prov = { from = \"authored\" }\n";
    files.insert("town-fixes/server/routes.toml".into(), route.into());
    assert_eq!(check(&files), Vec::new());
    let merged = merge(&read(&files).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    let camp = t.spawns.camps.iter().find(|c| c.id == "beetles").unwrap();
    let m = &camp.moves;
    assert_eq!(
        m.settings.kind,
        Some(yalain_content::MovementType::FreeRange)
    );
    assert_eq!(m.settings.wanders, Some(false));
    assert_eq!(m.settings.perception, Some(12.0));
    assert_eq!(
        m.settings.wander_range, None,
        "what it leaves out is the template's"
    );
    let r = m.route.as_ref().unwrap();
    assert_eq!((r.shape, r.gait), (Shape::Loop, Gait::Run));
    let rests: Vec<f64> = r.waypoints.iter().map(|w| w.rest).collect();
    assert_eq!(
        rests,
        [20.0, 5.0, 20.0],
        "a waypoint's own rest, else the route's"
    );
    assert_eq!((m.start, m.backward), (2, true));
    assert_eq!(
        yalain_content::SpawnTable::from_text(&t.spawns.to_text()).unwrap(),
        t.spawns,
        "the spawn table's text keeps them"
    );

    // A start past the route's last waypoint, a route no pack has, a word that is no movement
    // type, a route without waypoints.
    let mut bad = files.clone();
    edit(
        &mut bad,
        "town-fixes/server/landblocks/77B7.toml",
        "route_start = 2",
        "route_start = 3",
    );
    assert!(check(&bad).iter().any(|p| p
        .to_string()
        .contains("route_start: route rounds has 3 waypoints")));
    let mut bad = files.clone();
    edit(
        &mut bad,
        "town-fixes/server/landblocks/77B7.toml",
        "route = \"rounds\"",
        "route = \"watch\"",
    );
    assert!(check(&bad)
        .iter()
        .any(|p| p.to_string().contains("route: no route watch")));
    let mut bad = files.clone();
    edit(
        &mut bad,
        "town-fixes/server/landblocks/77B7.toml",
        "\"free-range\"",
        "\"roaming\"",
    );
    assert!(read(&bad).is_err());
    let mut bad = files;
    edit(
        &mut bad,
        "town-fixes/server/routes.toml",
        "shape = \"loop\"",
        "shape = \"circle\"",
    );
    assert!(read(&bad).is_err());
    bad.insert(
        "town-fixes/server/routes.toml".into(),
        "[[route]]\nkey = \"rounds\"\nwaypoints = []\nprov = { from = \"authored\" }\n".into(),
    );
    assert!(read(&bad).is_err());
}

#[test]
fn a_person_names_a_route_it_walks_and_only_a_person_may() {
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/landblocks/77B7.toml",
        "heading = 165.0\nprov = { from = \"authored\", by = \"tests\" }",
        "heading = 165.0\nroute = \"town-fixes:rounds\"\nroute_start = 1\n\
         prov = { from = \"authored\", by = \"tests\" }",
    );
    let route = "[[route]]\nkey = \"rounds\"\nrest = 20.0\n\
                 waypoints = [{ cell = 0x77B70036, at = [129.0, 105.0, 250.0] }, \
                 { cell = 0x77B70036, at = [129.0, 115.0, 250.0], rest = 5.0 }]\n\
                 prov = { from = \"authored\" }\n";
    files.insert("town-fixes/server/routes.toml".into(), route.into());
    assert_eq!(check(&files), Vec::new());
    let merged = merge(&read(&files).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    let keeper = t
        .content
        .objects
        .iter()
        .find(|o| o.template == 0x4700_33D0)
        .unwrap();
    let walks = keeper.walks.as_ref().expect("its route");
    let r = walks.route.as_ref().unwrap();
    assert_eq!(r.waypoints.len(), 2);
    assert_eq!((walks.start, walks.backward), (1, false));
    assert_eq!(
        yalain_content::ContentPack::from_text(&t.content.to_text()).unwrap(),
        t.content,
        "the content's text keeps it"
    );
    // A start past the route's end, and a static object naming a route.
    let mut bad = files.clone();
    edit(
        &mut bad,
        "town/server/landblocks/77B7.toml",
        "route_start = 1",
        "route_start = 2",
    );
    assert!(check(&bad).iter().any(|p| p
        .to_string()
        .contains("route_start: route rounds has 2 waypoints")));
    let mut bad = files;
    edit(
        &mut bad,
        "town/server/landblocks/77B7.toml",
        "kind = \"npc\"",
        "kind = \"static\"",
    );
    assert!(check(&bad)
        .iter()
        .any(|p| p.to_string().contains("route: only a person walks a route")));
}

#[test]
fn a_clearing_is_a_landblocks_row_with_one_shape_a_pack_can_take_away() {
    let row = |body: &str| {
        format!("[[clearing]]\nkey = \"yard\"\nat = [10.0, 20.0, \"ground\"]\n{body}prov = {{ from = \"authored\" }}\n")
    };
    let with = |file: &str, text: String| {
        let mut files = fixture();
        files.insert(format!("town/server/{file}"), text);
        files
    };
    let at = "landblocks/1234.toml";
    let db = read(&with(at, row("radius = 12.0\n"))).unwrap();
    let merged = merge(&db).unwrap();
    let found: Vec<_> = merged
        .rows
        .iter()
        .filter_map(|l| match &l.row {
            Row::Clearing(c) => Some((c.key.clone(), c.radius, c.size, l.landblock)),
            _ => None,
        })
        .collect();
    assert_eq!(found, [("yard".to_owned(), Some(12.0), None, Some(0x1234))]);
    compile(&merged, &Fake::new()).expect("the server has no use for it");
    let mut taken = with(
        at,
        row("radius = 12.0
"),
    );
    taken.insert(
        "town/server/gone.toml".to_owned(),
        "[[remove]]
item = \"clearing yard@1234\"
"
        .to_owned(),
    );
    let merged = merge(&read(&taken).unwrap()).unwrap();
    assert!(!merged
        .rows
        .iter()
        .any(|l| matches!(l.row, Row::Clearing(_))));
    assert!(read(&with(at, row("size = [4.0, 5.0]\nheading = 30.0\n"))).is_ok());

    for (file, text, says) in [
        (at, row(""), "radius"),
        (at, row("radius = 1.0\nsize = [1.0, 2.0]\n"), "radius"),
        (at, row("radius = 0.0\n"), "radius"),
        (at, row("radius = 241.0\n"), "radius"),
        (at, row("size = [1.0]\n"), "size"),
        ("places.toml", row("radius = 1.0\n"), "landblock"),
        (at, row("radius = 1.0\n").replace("[10.0,", "[160.0,"), "at"),
    ] {
        let problems = read(&with(file, text.clone())).expect_err(&text);
        assert!(
            problems.iter().any(|p| p.to_string().contains(says)),
            "{text}: {problems:?}"
        );
    }
    let two = format!("{}{}", row("radius = 1.0\n"), row("radius = 2.0\n"));
    let files = with(at, two);
    assert!(
        !yalain_world_db::check::check(&files).is_empty(),
        "one key to a block"
    );
}

#[test]
fn an_objects_name_text_is_its_name_unless_it_has_a_recorded_one() {
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/landblocks/77B7.toml",
        "heading = 165.0",
        "heading = 165.0\nname_text = \"Old Tom\"",
    );
    edit(
        &mut files,
        "town/server/landblocks/77B7.toml",
        "to = \"inn\"\nprov = { from = \"authored\" }",
        "to = \"inn\"\nname_text = \"Ignored\"\nprov = { from = \"authored\" }",
    );
    let merged = merge(&read(&files).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    assert_eq!(
        yalain_content::ContentPack::from_text(&t.content.to_text()).unwrap(),
        t.content,
        "the content's text keeps it"
    );
    let named = |template: u32| {
        t.content
            .objects
            .iter()
            .find(|o| o.template == template)
            .unwrap()
            .description
            .name_text
            .clone()
    };
    assert_eq!(named(0x4700_33D0).as_deref(), Some("Old Tom"));
    assert_eq!(named(0x4700_115B), None, "its recorded name stands");
}

#[test]
fn an_objects_own_scale_is_served_in_place_of_its_templates() {
    let scale_with = |row: &str| {
        let mut files = fixture();
        edit(
            &mut files,
            "town/server/landblocks/77B7.toml",
            "to = \"inn\"\nprov = { from = \"authored\" }",
            &format!("to = \"inn\"\n{row}prov = {{ from = \"authored\" }}"),
        );
        let merged = merge(&read(&files).unwrap()).unwrap();
        let t = compile(&merged, &Fake::new()).unwrap();
        assert_eq!(
            yalain_content::ContentPack::from_text(&t.content.to_text()).unwrap(),
            t.content,
            "the content's text keeps it"
        );
        let scale = t
            .content
            .objects
            .iter()
            .find(|o| o.object == Some(0x4077_B700_0000_0001))
            .unwrap()
            .description
            .scale;
        let warned = t.warnings.iter().any(|w| w.message.starts_with("scale"));
        (scale, warned)
    };
    let (own, warned) = scale_with("");
    assert!(!warned);
    assert_ne!(own, Some(2.5));
    assert_eq!(scale_with("scale = 2.5\n"), (Some(2.5), false));
    assert_eq!(scale_with("scale = 0.0\n"), (own, true), "left as it was");
}

#[test]
fn a_static_objects_quest_gate_is_read_checked_and_compiled() {
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/landblocks/77B7.toml",
        "to = \"inn\"\nprov = { from = \"authored\" }",
        "to = \"inn\"\nrequires = { quest = 0x40000088, phase = 2 }\nprov = { from = \"authored\" }",
    );
    assert_eq!(check(&files), Vec::new());
    let gate = |files: &Files| {
        let merged = merge(&read(files).unwrap()).unwrap();
        let t = compile(&merged, &Fake::new()).unwrap();
        assert_eq!(
            yalain_content::ContentPack::from_text(&t.content.to_text()).unwrap(),
            t.content,
            "the content's text keeps it"
        );
        let g = t
            .content
            .objects
            .iter()
            .find(|o| o.object == Some(0x4077_B700_0000_0001))
            .unwrap()
            .gate;
        (g, t.warnings)
    };
    let (g, warnings) = gate(&files);
    assert_eq!(
        g,
        Some(yalain_content::QuestGate {
            quest: 0x4000_0088,
            status: 0x1000_0002
        }),
        "under way at the second phase"
    );
    assert!(!warnings.iter().any(|w| w.message.starts_with("requires")));
    let mut any = files.clone();
    edit(
        &mut any,
        "town/server/landblocks/77B7.toml",
        "phase = 2",
        "underway = true",
    );
    assert_eq!(
        gate(&any).0.map(|g| g.status),
        Some(0x1000_0000),
        "under way"
    );
    let mut done = files.clone();
    edit(
        &mut done,
        "town/server/landblocks/77B7.toml",
        "quest = 0x40000088, phase = 2",
        "quest = 0x40000099, done = true",
    );
    let (g, warnings) = gate(&done);
    assert_eq!(
        g,
        Some(yalain_content::QuestGate {
            quest: 0x4000_0099,
            status: 0x3000_0000
        }),
        "completed"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.message.contains("requires: quest 40000099 has no row")),
        "a gate on a quest nobody holds is told: {warnings:?}"
    );
    // A phase of 0 or two statuses do not read; a person keeps no gate.
    for wrong in ["phase = 0", "phase = 2, done = true"] {
        let mut bad = files.clone();
        edit(
            &mut bad,
            "town/server/landblocks/77B7.toml",
            "phase = 2",
            wrong,
        );
        assert!(read(&bad).is_err(), "{wrong} reads");
    }
    let mut bad = files;
    edit(
        &mut bad,
        "town/server/landblocks/77B7.toml",
        "heading = 165.0\nprov",
        "heading = 165.0\nrequires = { quest = 0x40000088, done = true }\nprov",
    );
    assert!(check(&bad)
        .iter()
        .any(|p| p.to_string().contains("requires: a person is talked to")));
}

/// A quest's least level is its setup's, unless its row says another.
/// Behaviour: server.quests.least-level-from-the-setup
#[test]
fn a_quests_least_level_is_its_setups_unless_its_row_names_one() {
    let install = Fake {
        min_level: 15,
        ..Fake::new()
    };
    let t = compile(&merge(&read(&fixture()).unwrap()).unwrap(), &install).unwrap();
    assert_eq!(t.quests.quests[0].min_level, 15);
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/quests/40000088.toml",
        "duration = 72000",
        "duration = 72000\nmin_level = 3",
    );
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &install).unwrap();
    assert_eq!(t.quests.quests[0].min_level, 3);
}

/// What a quest adds to a quest rating is its setup's, unless its row says, whatever else the row
/// says of its pay; and it survives the tables' text.
/// Behaviour: server.quests.completion-adds-points
#[test]
fn what_a_quest_adds_to_a_rating_is_its_setups_unless_its_row_says() {
    use yalain_content::quests::{CompletionPay, QuestTable};
    let install = Fake {
        setup_pay: CompletionPay {
            points: 400,
            ..CompletionPay::default()
        },
        ..Fake::new()
    };
    let points = |extra: &str| {
        let mut files = fixture();
        edit(
            &mut files,
            "town/server/quests/40000088.toml",
            "duration = 72000",
            &format!("duration = 72000\n{extra}"),
        );
        let t = compile(&merge(&read(&files).unwrap()).unwrap(), &install).unwrap();
        (t.quests.quests[0].pay.points, t.quests)
    };
    assert_eq!(points("xp = 1").0, 400, "left out: the setup's");
    assert_eq!(points("qp = 50").0, 50, "the row's");
    assert_eq!(points("qp = 0").0, 0, "a row may say none");
    assert_eq!(
        points("qp = 50\nxp_share = 0.005").0,
        50,
        "with a share of its own"
    );
    let (_, t) = points("qp = 50");
    let back = QuestTable::from_text(&t.to_text()).expect("reads");
    assert_eq!(back.quests[0].pay.points, 50, "through the text");
}

/// A shard's row may name the quest it completes, as the story's table keeps it.
/// Behaviour: server.story.shard-completes-its-quest
#[test]
fn a_shards_row_names_the_quest_it_completes() {
    let row = |quest: &str| {
        let mut files = fixture();
        files.insert(
            "town/server/story.toml".into(),
            format!(
                "[[shard]]\nobject = 0x4002990000000059\nscene = 0x40000021\n{quest}prov = {{ from = \"authored\", by = \"t\", note = \"t\", confidence = \"low\" }}\n"
            ),
        );
        let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
        (
            t.story.shards.get(&0x4002_9900_0000_0059).copied(),
            t.story.shard_quests.get(&0x4002_9900_0000_0059).copied(),
        )
    };
    assert_eq!(row(""), (Some(0x4000_0021), None));
    assert_eq!(
        row("quest = 0x40000033\n"),
        (Some(0x4000_0021), Some(0x4000_0033))
    );
}

/// What a completion pays is the setup's, unless the row says: a row's amount is paid in place of
/// the setup's share, and a row's share in place of all the setup says, paid whole, up to the row's
/// level.
/// Behaviour: server.quests.completion-pays-a-share
#[test]
fn what_a_quest_pays_is_its_setups_unless_its_row_says() {
    use yalain_content::quests::CompletionPay;
    let setup = CompletionPay {
        share: 0.04,
        least: 7,
        most_at_level: Some((0.03, 40)),
        ..CompletionPay::default()
    };
    let install = Fake {
        setup_xp: 90,
        setup_pay: setup,
        ..Fake::new()
    };
    let pay = |extra: &str| {
        let mut files = fixture();
        edit(
            &mut files,
            "town/server/quests/40000088.toml",
            "duration = 72000",
            &format!("duration = 72000\n{extra}"),
        );
        let t = compile(&merge(&read(&files).unwrap()).unwrap(), &install).unwrap();
        (t.quests.quests[0].xp, t.quests.quests[0].pay)
    };
    assert_eq!(
        pay("xp_share = 0.005\nxp_share_level = 72").1,
        CompletionPay {
            share: 0.005,
            most_at_level: Some((0.005, 72)),
            flat: true,
            ..CompletionPay::default()
        },
        "the row's share"
    );
    let t = compile(&merge(&read(&fixture()).unwrap()).unwrap(), &install).unwrap();
    assert_eq!(
        t.quests.quests[0].pay.share, setup.share,
        "left out: the setup's"
    );
    let (xp, row) = pay("xp = 12");
    assert_eq!((xp, row.share), (12, 0.0), "a row's amount, no share");
    assert!(!row.flat, "weighed as the setup says");
    assert!(
        pay("xp = 12\nxp_whole = true").1.flat,
        "whole where the row says so"
    );
    assert_eq!(
        pay("xp_share = 0.02\nxp_least = 3\nxp_most = 9").1,
        CompletionPay {
            share: 0.02,
            least: 3,
            most: 9,
            flat: true,
            ..CompletionPay::default()
        },
        "the row's share, within its limits"
    );
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/quests/40000088.toml",
        "duration = 72000",
        "duration = 72000\nxp_share = 0.005\nxp_share_level = 72",
    );
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &install).unwrap();
    assert_eq!(
        yalain_content::QuestTable::from_text(&t.quests.to_text()).unwrap(),
        t.quests,
        "kept in the text form"
    );
}

#[test]
fn a_step_that_fails_a_quest_must_need_it_under_way() {
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/quests/40000088.toml",
        "needs = 2\ntrigger = { arrive = 0x4700115B }\nupdate = \"complete\"\nxp = 40\ngives = [0x470020B9]\n",
        "needs = 2\ntrigger = { arrive = 0x4700115B }\nupdate = \"fail\"\n",
    );
    assert_eq!(check(&files), Vec::new(), "from a phase, a step may");
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert!(t.quests.steps.iter().any(|s| s.update == Update::Fail));
    assert_eq!(
        yalain_content::QuestTable::from_text(&t.quests.to_text()).unwrap(),
        t.quests,
        "it reads back as written"
    );
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/quests/40000088.toml",
        "needs = 0\ntrigger = { use = 0x470033D0 }\ntell = { table = 0x25002881, string = 0x2824FD4C }\nupdate = \"bestow\"",
        "needs = 0\ntrigger = { use = 0x470033D0 }\nupdate = \"fail\"",
    );
    let messages: Vec<String> = check(&files).iter().map(ToString::to_string).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("only a quest under way fails")),
        "{messages:?}"
    );
}

/// Behaviour: server.gives.wants-a-count
#[test]
fn a_give_s_step_may_want_a_count_and_no_other_step_may() {
    let mut files = fixture();
    let give = yalain_content::quests::Trigger::Give(0x4700_20B9, 0x4700_33D0);
    edit(
        &mut files,
        "town/server/quests/40000088.toml",
        "trigger = { give = 0x470020B9, to = 0x470033D0 }\n",
        "trigger = { give = 0x470020B9, to = 0x470033D0 }\nwants = 3\n",
    );
    assert_eq!(check(&files), Vec::new());
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    let wanted = |t: &yalain_content::QuestTable| {
        t.steps.iter().find(|s| s.trigger == give).map(|s| s.wants)
    };
    assert_eq!(wanted(&t.quests), Some(Some(3)));
    assert_eq!(
        yalain_content::QuestTable::from_text(&t.quests.to_text()).unwrap(),
        t.quests,
        "it reads back as written"
    );
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/quests/40000088.toml",
        "trigger = { give = 0x470020B9, to = 0x470033D0 }\n",
        "trigger = { give = 0x470020B9, to = 0x470033D0 }\nwants = 0\n",
    );
    let messages: Vec<String> = check(&files).iter().map(ToString::to_string).collect();
    assert!(
        messages.iter().any(|m| m.contains("a count of at least 1")),
        "{messages:?}"
    );
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/quests/40000088.toml",
        "needs = 0\ntrigger = { use = 0x470033D0 }\n",
        "needs = 0\ntrigger = { use = 0x470033D0 }\nwants = 2\n",
    );
    let messages: Vec<String> = check(&files).iter().map(ToString::to_string).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("only a give's step wants a count")),
        "{messages:?}"
    );
}

const GIVE_ROW: &str = r#"[[give]]
key = "bucket"
item = 0x470020B9
to = 0x470033D0
wants = 50
gives = [0x470020B9]
counts = [8]
effects = [0x6F0019F0]
fx = 0x410005EB
requires = [{ quest = 0x40000088, phase = 2 }]
prov = { from = "authored", by = "a test", note = "a test" }
"#;

/// Behaviour: server.gives.row-keeps-and-gives
#[test]
fn a_give_moving_no_quest_compiles_and_reads_back_as_written() {
    let mut files = fixture();
    files.insert("town/server/gives/470033D0.toml".into(), GIVE_ROW.into());
    assert_eq!(check(&files), Vec::new());
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    let g = &t.quests.gives[0];
    assert_eq!(
        (g.to, g.item, g.wants, g.fx),
        (0x4700_33D0, Some(0x4700_20B9), Some(50), Some(0x4100_05EB))
    );
    assert_eq!(
        (g.gives.as_slice(), g.counts.as_slice()),
        (&[0x4700_20B9][..], &[8][..])
    );
    assert_eq!(g.effects, vec![0x6F00_19F0]);
    assert_eq!(
        g.requires,
        vec![(0x4000_0088, yalain_content::quests::Need::Phase(2))]
    );
    assert_eq!(
        yalain_content::QuestTable::from_text(&t.quests.to_text()).unwrap(),
        t.quests,
        "it reads back as written"
    );
    let plain = GIVE_ROW
        .replace("item = 0x470020B9\n", "")
        .replace("wants = 50\n", "")
        .replace("counts = [8]\n", "")
        .replace("fx = 0x410005EB\n", "");
    let mut files = fixture();
    files.insert("town/server/gives/470033D0.toml".into(), plain);
    assert_eq!(check(&files), Vec::new());
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert_eq!((t.quests.gives[0].item, t.quests.gives[0].fx), (None, None));
    assert_eq!(
        yalain_content::QuestTable::from_text(&t.quests.to_text()).unwrap(),
        t.quests,
        "any item reads back as written"
    );
}

/// Behaviour: server.gives.row-wants-a-count
#[test]
fn a_give_moving_no_quest_is_checked_for_what_it_wants_and_names() {
    let problems = |row: String| -> Vec<String> {
        let mut files = fixture();
        files.insert("town/server/gives/470033D0.toml".into(), row);
        check(&files).iter().map(ToString::to_string).collect()
    };
    let m = problems(GIVE_ROW.replace("wants = 50", "wants = 0"));
    assert!(
        m.iter().any(|x| x.contains("a count of at least 1")),
        "{m:?}"
    );
    let m = problems(GIVE_ROW.replace("item = 0x470020B9\n", ""));
    assert!(
        m.iter()
            .any(|x| x.contains("a count of no item in particular")),
        "{m:?}"
    );
    let m = problems(GIVE_ROW.replace("counts = [8]", "counts = [8, 9]"));
    assert!(
        m.iter().any(|x| x.contains("a count for each item given")),
        "{m:?}"
    );
    let mut files = fixture();
    files.insert(
        "town/server/gives/470033D0.toml".into(),
        GIVE_ROW.replace("to = 0x470033D0", "to = 0x47009998"),
    );
    let compiled = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new());
    assert!(compiled.is_err(), "a template the install lacks");
}

#[test]
fn a_reminder_may_name_a_phase_and_no_other_line_but_an_advance_may() {
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/talk/470033D0.toml",
        "role = \"near\"",
        "role = \"remind\"\nphase = 2",
    );
    assert_eq!(check(&files), Vec::new());
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/talk/470033D0.toml",
        "role = \"near\"",
        "role = \"too-soon\"\nphase = 2",
    );
    let messages: Vec<String> = check(&files).iter().map(ToString::to_string).collect();
    assert!(
        messages.iter().any(|m| m.contains("names a phase")),
        "{messages:?}"
    );
}

/// A step may wait on a quest being given, and reads back as written.
/// Behaviour: server.quests.step-on-a-quest-given-row
#[test]
fn a_step_on_a_quest_given_compiles_and_reads_back_as_written() {
    let mut files = fixture();
    let quest = files["town/server/quests/40000088.toml"].clone();
    files.insert(
        "town/server/quests/40000088.toml".into(),
        format!(
            "{quest}\n[[step]]\nkey = \"on-given\"\nquest = 0x40000088\nneeds = 2\ntrigger = {{ given = 0x40000089 }}\nupdate = \"complete\"\nprov = {{ from = \"authored\", by = \"a test\", note = \"a test\" }}\n"
        ),
    );
    assert_eq!(check(&files), Vec::new());
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert!(t
        .quests
        .steps
        .iter()
        .any(|s| s.trigger == yalain_content::quests::Trigger::Given(0x4000_0089)));
    assert_eq!(
        yalain_content::QuestTable::from_text(&t.quests.to_text()).unwrap(),
        t.quests,
        "it reads back as written"
    );
}
