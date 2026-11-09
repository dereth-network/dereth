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
const STARTING_SKILL_1: u32 = 0x4100_0003;
const STARTING_SKILL_2: u32 = 0x4100_0004;
const DRAWABLE: u32 = 0x4000_0001;
const MINE_OBJECT: u32 = 0x4100_0005;
/// A node of the made-up install, giving a material when mined.
const NODE: u32 = 0x4700_0C8A;

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
        entities.insert(
            NODE,
            entity(
                CLASS,
                0x0C00,
                vec![
                    look.clone(),
                    (MINE_OBJECT, PropertyValue::DataFile(DataId(0x4700_0B31))),
                ],
            ),
        );
        entities.insert(0x4700_2224, entity(CLASS, 0x0E00, vec![look.clone()]));
        // A pet's template, naming two skills.
        entities.insert(
            0x4700_0846,
            entity(
                CLASS,
                0x0E00,
                vec![
                    look,
                    (STARTING_SKILL_1, PropertyValue::Enum(838)),
                    (STARTING_SKILL_2, PropertyValue::Enum(840)),
                ],
            ),
        );
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
            STARTING_SKILL_1 => Some("NPC_StartingSkill1".into()),
            STARTING_SKILL_2 => Some("NPC_StartingSkill2".into()),
            MINE_OBJECT => Some("Mine_Object".into()),
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
fn an_arrival_may_name_the_one_placed_portal_it_came_through() {
    let mut files = fixture();
    let quests = "town/server/quests/40000088.toml";
    edit(
        &mut files,
        quests,
        "trigger = { arrive = 0x4700115B }",
        "trigger = { arrive = 0x4700115B, from = 0x4077B70000000001 }",
    );
    assert_eq!(check(&files), []);
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert_eq!(
        t.quests.steps[2].trigger,
        Trigger::ArriveFrom(0x4700_115B, 0x4077_B700_0000_0001)
    );
    assert_eq!(
        yalain_content::QuestTable::from_text(&t.quests.to_text()).unwrap(),
        t.quests,
        "it reads back as written"
    );
    // The object must be placed, of the trigger's template, and lead somewhere.
    let problems = |from: &str, to: &str| {
        let mut f = files.clone();
        edit(&mut f, quests, from, to);
        check(&f)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        problems("from = 0x4077B70000000001", "from = 0x4077B700000000FF"),
        ["town/server/quests/40000088.toml:30: trigger: no placed object 0x4077B700000000FF"]
    );
    assert_eq!(
        problems("arrive = 0x4700115B, from", "arrive = 0x4700115C, from"),
        ["town/server/quests/40000088.toml:30: trigger: object 0x4077B70000000001 is not of template 0x4700115C"]
    );
    let mut f = files.clone();
    edit(
        &mut f,
        "town/server/landblocks/77B7.toml",
        "to = \"inn\"\nprov",
        "prov",
    );
    let messages: Vec<String> = check(&f).iter().map(ToString::to_string).collect();
    assert!(
        messages.contains(
            &"town/server/quests/40000088.toml:30: trigger: object 0x4077B70000000001 leads nowhere, so nobody arrives through it"
                .to_string()
        ),
        "{messages:?}"
    );
    assert!(!problems("trigger = { arrive", "to = \"inn\"\ntrigger = { arrive").is_empty());
}

/// A camp names its creatures by a string of the install, and a step moved by a kill may name the
/// one camp whose creature it wants.
#[test]
fn a_camp_may_name_its_creatures_and_a_kill_may_name_the_camp() {
    const BEETLE: u32 = 0x4700_2224;
    let mut files = fixture();
    let quests = "town/server/quests/40000088.toml";
    let fixes = "town-fixes/server/landblocks/77B7.toml";
    edit(
        &mut files,
        fixes,
        "prov = { from = \"authored\", note = \"moved up the hill\" }",
        "name = { table = 0x2500284B, string = 0xE77B18D7 }\nprov = { from = \"authored\", note = \"moved up the hill\" }",
    );
    edit(
        &mut files,
        quests,
        "trigger = { arrive = 0x4700115B }",
        "trigger = { kill = 0x47002224, camp = \"beetles\" }",
    );
    assert_eq!(check(&files), []);
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    let row = t
        .spawns
        .camps
        .iter()
        .position(|c| c.id == "beetles")
        .unwrap();
    assert_eq!(t.spawns.camps[row].name, Some((0xE77B_18D7, 0x2500_284B)));
    assert_eq!(
        t.quests.steps[2].trigger,
        Trigger::KillIn(BEETLE, u32::try_from(row).unwrap())
    );
    assert_eq!(
        yalain_content::SpawnTable::from_text(&t.spawns.to_text()).unwrap(),
        t.spawns,
        "the spawn table reads back as written"
    );
    assert_eq!(
        yalain_content::QuestTable::from_text(&t.quests.to_text()).unwrap(),
        t.quests,
        "the quest table reads back as written"
    );
    // A spawn table written before camps named their creatures still reads, naming none.
    let old: String = t
        .spawns
        .to_text()
        .lines()
        .map(|l| l.rsplit_once('\t').map_or(l, |(head, _)| head).to_string() + "\n")
        .collect();
    let read_old = yalain_content::SpawnTable::from_text(&old).unwrap();
    assert!(read_old.camps.iter().all(|c| c.name.is_none()));

    // The camp must be one the world has, and make creatures of the template.
    let problems =
        |f: &Files| -> Vec<String> { check(f).iter().map(ToString::to_string).collect() };
    let mut f = files.clone();
    edit(&mut f, quests, "camp = \"beetles\"", "camp = \"wasps\"");
    assert_eq!(
        problems(&f),
        ["town/server/quests/40000088.toml:30: trigger: no camp wasps"]
    );
    let mut f = files.clone();
    edit(&mut f, quests, "kill = 0x47002224", "kill = 0x47002225");
    assert_eq!(
        problems(&f),
        ["town/server/quests/40000088.toml:30: trigger: camp beetles makes no creature of template 0x47002225"]
    );
    // A key two landblocks share names one of them with its landblock.
    let mut f = files.clone();
    f.insert(
        "town/server/landblocks/77B8.toml".into(),
        "[[camp]]\nkey = \"beetles\"\nat = [10.0, 10.0, 140.0]\npopulation = 1\nregen = 60.0\n\
         spawns = [{ template = 0x47002224, weight = 1 }]\nprov = { from = \"authored\" }\n"
            .into(),
    );
    assert_eq!(
        problems(&f),
        ["town/server/quests/40000088.toml:30: trigger: several camps are beetles: name one as beetles@XXYY"]
    );
    edit(
        &mut f,
        quests,
        "camp = \"beetles\"",
        "camp = \"beetles@77B8\"",
    );
    assert_eq!(check(&f), []);
    let t = compile(&merge(&read(&f).unwrap()).unwrap(), &Fake::new()).unwrap();
    let row = t
        .spawns
        .camps
        .iter()
        .position(|c| c.id == "beetles@77B8")
        .unwrap();
    assert_eq!(
        t.quests.steps[2].trigger,
        Trigger::KillIn(BEETLE, u32::try_from(row).unwrap())
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
fn a_tool_row_gives_a_templates_unmade_yield_and_must_name_one_installed() {
    let mut files = fixture();
    files.insert(
        "town/server/crafting.toml".into(),
        "[[tool]]\ntemplate = 0x4700115B\nyield = 6\nprov = { from = \"authored\" }\n\n\
         [[tool]]\ntemplate = 0x47009999\nyield = 2\nprov = { from = \"authored\" }\n"
            .into(),
    );
    let merged = merge(&read(&files).unwrap()).unwrap();
    let problems = compile(&merged, &Fake::new()).unwrap_err();
    let messages: Vec<String> = problems.iter().map(ToString::to_string).collect();
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(messages[0].contains("template: template 47009999 is not in the install"));
    files.insert(
        "town/server/crafting.toml".into(),
        "[[tool]]\ntemplate = 0x4700115B\nyield = 6\nprov = { from = \"authored\" }\n".into(),
    );
    let merged = merge(&read(&files).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    assert_eq!(t.crafting.tool_yields.get(&0x4700_115B), Some(&6));
    assert_eq!(t.crafting.tool_yields.len(), 1);
    // A yield is a count.
    files.insert(
        "town/server/crafting.toml".into(),
        "[[tool]]\ntemplate = 0x4700115B\nyield = -1\nprov = { from = \"authored\" }\n".into(),
    );
    assert!(read(&files).is_err());
}

#[test]
fn an_effect_rule_row_gives_a_ward_of_classes_or_effects_at_a_chance() {
    let mut files = fixture();
    files.insert(
        "town/server/effects.toml".into(),
        "[[effect_rule]]\neffect = 0x6F000E94\nward = { classes = [93, 94, 95], chance = 1.0 }\nprov = { from = \"authored\" }\n\n\
         [[effect_rule]]\neffect = 0x6F000E95\nward = { effects = [0x700006C6], chance = 0.15 }\nprov = { from = \"authored\" }\n"
            .into(),
    );
    let merged = merge(&read(&files).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    let wards = &t.effect_rules.wards;
    assert_eq!(wards.len(), 2);
    assert_eq!(wards[&0x6F00_0E94].classes, [93, 94, 95]);
    assert!(wards[&0x6F00_0E94].effects.is_empty());
    assert_eq!(wards[&0x6F00_0E94].chance, 1.0);
    assert_eq!(wards[&0x6F00_0E95].effects, [0x7000_06C6]);
    assert_eq!(wards[&0x6F00_0E95].chance, 0.15);
    // A ward keeps off something, by a chance above nothing and at most certain, and names
    // nothing else.
    for ward in [
        "{ classes = [], chance = 1.0 }",
        "{ chance = 1.0 }",
        "{ classes = [93], chance = 0.0 }",
        "{ classes = [93], chance = 1.5 }",
        "{ classes = [93] }",
        "{ classes = [93], chance = 1.0, fellows = true }",
        "{ classes = [-1], chance = 1.0 }",
    ] {
        files.insert(
            "town/server/effects.toml".into(),
            format!(
                "[[effect_rule]]\neffect = 0x6F000E94\nward = {ward}\nprov = {{ from = \"authored\" }}\n"
            ),
        );
        assert!(read(&files).is_err(), "{ward}");
    }
    files.insert(
        "town/server/effects.toml".into(),
        "[[effect_rule]]\neffect = 0x6F000E94\nprov = { from = \"authored\" }\n".into(),
    );
    assert!(read(&files).is_err(), "a rule says what it does");
    // One rule an effect.
    files.insert(
        "town/server/effects.toml".into(),
        "[[effect_rule]]\neffect = 0x6F000E94\nward = { classes = [93], chance = 1.0 }\nprov = { from = \"authored\" }\n\n\
         [[effect_rule]]\neffect = 0x6F000E94\nward = { classes = [94], chance = 1.0 }\nprov = { from = \"authored\" }\n"
            .into(),
    );
    assert!(merge(&read(&files).unwrap()).is_err());
}

/// A mine row's fields, as the iron mine of 2017 writes them, with the template it may name.
fn mine_row(skill: &str, node: u32, tool: u32) -> String {
    format!(
        "[[mine]]\nskill = {skill}\nbehavior = 0x4100014D\ntools = [{tool:#010X}]\n\
         answer = {{ table = 0x25000CEC, string = 0x79C276DB }}\n\
         rich_answer = {{ table = 0x25000CEC, string = 0x53EE7371 }}\n\
         delving = {{ effect = \"Eff_SummonMine_Iron_Delving\", node = {node:#010X} }}\n\
         lodestones = [{{ from = 0, template = 0x4700115B }}, {{ from = 30, template = 0x47002224 }}]\n\
         find_chance = 0.05\nfinds = [{{ template = 0x47000001, share = 0.25 }}]\n\
         prov = {{ from = \"authored\" }}\n"
    )
}

#[test]
fn a_mine_row_compiles_to_its_skills_mine_and_must_name_installed_templates_and_a_node() {
    let mut files = fixture();
    files.insert(
        "town/server/mines.toml".into(),
        mine_row("0x6F00069C", NODE, 0x4700_115B),
    );
    let merged = merge(&read(&files).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    let m = &t.gathering.mines[&0x6F00_069C];
    assert_eq!(m.behavior, 0x4100_014D);
    assert_eq!(m.tools, [0x4700_115B]);
    assert_eq!(m.answer, (0x79C2_76DB, 0x2500_0CEC));
    assert_eq!(m.rich_answer, (0x53EE_7371, 0x2500_0CEC));
    assert_eq!(
        m.delving,
        Some(("Eff_SummonMine_Iron_Delving".to_string(), NODE))
    );
    assert_eq!(m.lodestone(29), Some(0x4700_115B));
    assert_eq!(m.lodestone(30), Some(0x4700_2224));
    assert!((m.find_chance - 0.05).abs() < 1e-6);
    assert_eq!(m.finds, [(0x4700_0001, 0.25)]);
    assert_eq!(
        t.gathering.delving_node("Eff_SummonMine_Iron_Delving"),
        Some(NODE)
    );
    assert!(t.gathering.map_regions.is_empty());
    // A tool the install lacks, and a delving node that gives nothing when mined.
    for (node, tool, said) in [
        (
            NODE,
            0x4700_9999,
            "tools: template 47009999 is not in the install",
        ),
        (
            0x4700_115B,
            0x4700_115B,
            "delving: template 4700115B gives no material when mined",
        ),
    ] {
        files.insert(
            "town/server/mines.toml".into(),
            mine_row("0x6F00069C", node, tool),
        );
        let merged = merge(&read(&files).unwrap()).unwrap();
        let problems = compile(&merged, &Fake::new()).unwrap_err();
        let messages: Vec<String> = problems.iter().map(ToString::to_string).collect();
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert!(messages[0].contains(said), "{messages:?}");
    }
    // Lodestones by levels rising from 0, finds by shares above 0 and at most 1 in all, a find
    // chance from 0 to 1, a delving stone's effect and node.
    let good = mine_row("0x6F00069C", NODE, 0x4700_115B);
    for (from, to) in [
        (
            "{ from = 0, template = 0x4700115B }, { from = 30",
            "{ from = 1, template = 0x4700115B }, { from = 30",
        ),
        (
            "{ from = 30, template = 0x47002224 }",
            "{ from = 0, template = 0x47002224 }",
        ),
        ("share = 0.25", "share = 0.0"),
        ("share = 0.25", "share = 1.5"),
        ("find_chance = 0.05", "find_chance = 1.5"),
        (", node = 0x47000C8A", ""),
        ("answer = {", "answers = {"),
    ] {
        assert!(good.contains(from), "{from}");
        files.insert("town/server/mines.toml".into(), good.replacen(from, to, 1));
        assert!(read(&files).is_err(), "{to}");
    }
    // One row a skill.
    files.insert(
        "town/server/mines.toml".into(),
        format!("{good}\n{}", good.replacen("0x4100014D", "0x41000149", 1)),
    );
    assert!(merge(&read(&files).unwrap()).is_err());
}

#[test]
fn a_map_region_row_compiles_to_its_nodes_by_effect_and_each_must_give_a_material() {
    let mut files = fixture();
    let row = |node: u32| {
        format!(
            "[[map_region]]\nregion = \"linvak\"\nnodes = [{{ effect = \"Eff_SummonMine_Crystal_Linvak\", node = {node:#010X} }}]\nprov = {{ from = \"authored\" }}\n"
        )
    };
    files.insert("town/server/maps.toml".into(), row(NODE));
    let merged = merge(&read(&files).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    assert_eq!(
        t.gathering.map_node("Eff_SummonMine_Crystal_Linvak"),
        Some(NODE)
    );
    assert_eq!(t.gathering.map_regions["linvak"].nodes.len(), 1);
    assert!(t.gathering.mines.is_empty());
    files.insert("town/server/maps.toml".into(), row(0x4700_115B));
    let merged = merge(&read(&files).unwrap()).unwrap();
    let problems = compile(&merged, &Fake::new()).unwrap_err();
    assert!(problems.iter().any(|p| p
        .to_string()
        .contains("nodes: template 4700115B gives no material when mined")));
    for bad in [
        "[[map_region]]\nregion = \"linvak\"\nnodes = []\nprov = { from = \"authored\" }\n",
        "[[map_region]]\nregion = \"Linvak\"\nnodes = [{ effect = \"Eff_SummonMine_Crystal_Linvak\", node = 0x47000C8A }]\nprov = { from = \"authored\" }\n",
        "[[map_region]]\nregion = \"linvak\"\nnodes = [{ effect = \"an effect\", node = 0x47000C8A }]\nprov = { from = \"authored\" }\n",
    ] {
        files.insert("town/server/maps.toml".into(), bad.into());
        assert!(read(&files).is_err(), "{bad}");
    }
}

#[test]
fn a_pet_row_compiles_to_a_kind_and_must_name_an_installed_template() {
    let mut files = fixture();
    files.insert(
        "town/server/pets.toml".into(),
        "[[pet]]\neffect = 0x7000092A\ntemplate = 0x4700115B\nflags = 1096\ncount = 4\nstays = true\n\
         health = [{ level = 10, health = 200 }, { level = 20, health = 400 }]\n\
         per_level_squared = 0.03\nlands = 0.5\nswing = 2\narea = 10.0\nno_corpse = true\n\
         ends_on_strike = true\nvigor_per_level = 80\nvigor_per_second = 7.5\n\
         tends = { whom = \"owners-pets\", effect = 0x70000772, every = 30.0, below = 0.75 }\n\
         hire = { class = 0x100000, seconds = 3600.0, level = 20.0 }\n\
         item_effects = [\"Eff_Test_Item_Summon\"]\n\
         prov = { from = \"authored\" }\n\n\
         [[pet_limit]]\nskill = 662\nlimits = [{ most = 2, classes = 0x800 }, { most = 1, classes = 0x4000 }]\n\
         prov = { from = \"authored\" }\n"
            .into(),
    );
    assert_eq!(check(&files), Vec::new());
    let merged = merge(&read(&files).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    let k = t.pet_kinds.get(&0x7000_092A).expect("a kind");
    assert_eq!(k.template, 0x4700_115B);
    assert_eq!((k.flags, k.made, k.stays), (1096, 4, true));
    assert!(k.ends_on_strike);
    assert_eq!(
        (k.vigor_per_level, k.vigor_per_second),
        (Some(80), Some(7.5))
    );
    assert_eq!(k.health, vec![(10, 200), (20, 400)]);
    assert_eq!(
        (k.per_level_squared, k.lands, k.hits, k.area),
        (0.03, 0.5, 2, 10.0)
    );
    assert_eq!(k.variance, 0.35, "the server's own, left out");
    assert_eq!(k.corpse_after, None);
    assert!(k.tends.is_some() && k.hire.is_some());
    assert_eq!(k.item_effects, vec!["Eff_Test_Item_Summon".to_string()]);
    assert_eq!(t.pet_limits.get(&662), Some(&vec![(2, 0x800), (1, 0x4000)]));
    // A template the install lacks, and rows that read wrong.
    files.insert(
        "town/server/pets.toml".into(),
        "[[pet]]\neffect = 1\ntemplate = 0x47009999\nprov = { from = \"authored\" }\n".into(),
    );
    let merged = merge(&read(&files).unwrap()).unwrap();
    let problems = compile(&merged, &Fake::new()).unwrap_err();
    assert!(problems[0].to_string().contains("47009999"), "{problems:?}");
    for bad in [
        "[[pet]]\neffect = 1\ntemplate = 0x4700115B\ncount = 0\nprov = { from = \"authored\" }\n",
        "[[pet]]\neffect = 1\ntemplate = 0x4700115B\nlands = 2.0\nprov = { from = \"authored\" }\n",
        "[[pet]]\neffect = 1\ntemplate = 0x4700115B\nhealth = [{ level = 9, health = 1 }, { level = 2, health = 1 }]\nprov = { from = \"authored\" }\n",
        "[[pet_limit]]\nskill = 1\nlimits = []\nprov = { from = \"authored\" }\n",
    ] {
        files.insert("town/server/pets.toml".into(), bad.into());
        assert!(read(&files).is_err(), "{bad}");
    }
}

/// A pet row may list the attacks its kind uses, each by its weight; each must be one its template
/// names, once, with a weight of at least 1.
/// Behaviour: server.pets.a-kind-weighs-its-attacks
#[test]
fn a_pet_rows_attacks_are_skills_its_template_names_each_weighed() {
    let pet = |attacks: &str| {
        format!(
            "[[pet]]\neffect = 0x70000805\ntemplate = 0x47000846\nflags = 1096\n\
             attacks = {attacks}\nprov = {{ from = \"authored\" }}\n"
        )
    };
    let mut files = fixture();
    files.insert(
        "town/server/pets.toml".into(),
        pet("[{ skill = 840, weight = 3 }, { skill = 838, weight = 2 }]"),
    );
    assert_eq!(check(&files), Vec::new());
    let merged = merge(&read(&files).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    let k = t.pet_kinds.get(&0x7000_0805).expect("a kind");
    assert_eq!(k.attacks, vec![(840, 3), (838, 2)]);
    files.insert(
        "town/server/pets.toml".into(),
        pet("[{ skill = 840, weight = 3 }, { skill = 839, weight = 2 }]"),
    );
    let merged = merge(&read(&files).unwrap()).unwrap();
    let problems = compile(&merged, &Fake::new()).unwrap_err();
    assert!(
        problems[0]
            .to_string()
            .contains("attacks: template 47000846 names no skill 839"),
        "{problems:?}"
    );
    for bad in [
        "[{ skill = 840, weight = 0 }]",
        "[{ skill = 840, weight = 1 }, { skill = 840, weight = 2 }]",
        "[{ skill = 840 }]",
        "[840, 838]",
    ] {
        files.insert("town/server/pets.toml".into(), pet(bad));
        assert!(read(&files).is_err(), "{bad}");
    }
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

/// A placed object's kingdom and heroes alone read and compile, the content's text keeps them, and a
/// quest gate can ask for a quest had in any way; a person can hold none of them, and a kingdom is
/// one of the three.
/// Behaviour: server.places.portal-requirement-rows
#[test]
fn a_static_objects_kingdom_heroes_and_had_quest_are_read_and_compiled() {
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/landblocks/77B7.toml",
        "to = \"inn\"\nprov = { from = \"authored\" }",
        "to = \"inn\"\nhero = true\nkingdom = 2\nrequires = { quest = 0x40000088, had = true }\nprov = { from = \"authored\" }",
    );
    assert_eq!(check(&files), Vec::new());
    let merged = merge(&read(&files).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    assert_eq!(
        yalain_content::ContentPack::from_text(&t.content.to_text()).unwrap(),
        t.content,
        "the content's text keeps them"
    );
    let o = t
        .content
        .objects
        .iter()
        .find(|o| o.object == Some(0x4077_B700_0000_0001))
        .unwrap();
    assert!(o.facts.hero);
    assert_eq!(o.facts.kingdom, Some(2));
    assert_eq!(
        o.gate,
        Some(yalain_content::QuestGate {
            quest: 0x4000_0088,
            status: yalain_content::QuestGate::HAD,
        })
    );
    edit(
        &mut files,
        "town/server/landblocks/77B7.toml",
        "kingdom = 2",
        "kingdom = 3",
    );
    assert!(check(&files)
        .iter()
        .any(|p| p.to_string().contains("kingdom: 1 the Shadow")));
}

/// A person keeps no one out by a kingdom or heroes alone: it is talked to.
/// Behaviour: server.places.portal-requirement-rows-person
#[test]
fn a_person_holds_no_kingdom_or_heroes_alone() {
    let mut files = fixture();
    let person = files
        .iter()
        .find(|(_, v)| v.contains("kind = \"npc\""))
        .map(|(k, _)| k.clone())
        .expect("a person in the fixture");
    let text = files.get_mut(&person).unwrap();
    *text = text.replacen("kind = \"npc\"", "kind = \"npc\"\nhero = true", 1);
    assert!(check(&files)
        .iter()
        .any(|p| p.to_string().contains("hero, kingdom: a person")));
}

/// A placed object's least level reads and compiles over its template's, and the content's text
/// keeps it; left out, the object has its template's.
/// Behaviour: server.places.portal-least-level-row
#[test]
fn a_static_objects_least_level_is_read_and_compiled() {
    let mut files = fixture();
    let level = |files: &Files| {
        let merged = merge(&read(files).unwrap()).unwrap();
        let t = compile(&merged, &Fake::new()).unwrap();
        assert_eq!(
            yalain_content::ContentPack::from_text(&t.content.to_text()).unwrap(),
            t.content,
            "the content's text keeps it"
        );
        t.content
            .objects
            .iter()
            .find(|o| o.object == Some(0x4077_B700_0000_0001))
            .unwrap()
            .facts
            .min_level
    };
    assert_eq!(level(&files), None);
    edit(
        &mut files,
        "town/server/landblocks/77B7.toml",
        "to = \"inn\"\nprov = { from = \"authored\" }",
        "to = \"inn\"\nmin_level = 6\nprov = { from = \"authored\" }",
    );
    assert_eq!(check(&files), Vec::new());
    assert_eq!(level(&files), Some(6));
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

/// Behaviour: server.quests.cancel-waits-as-rows-say
#[test]
fn a_quest_row_may_name_its_wait_after_a_cancel() {
    let t = compile(&merge(&read(&fixture()).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert_eq!(t.quests.quests[0].cancel_wait, None, "left out: the rule's");
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/quests/40000088.toml",
        "duration = 72000",
        "duration = 72000\ncancel_wait = 18000",
    );
    assert_eq!(check(&files), Vec::new());
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert_eq!(t.quests.quests[0].cancel_wait, Some(18_000.0));
    let again = yalain_content::QuestTable::from_text(&t.quests.to_text()).unwrap();
    assert_eq!(
        again.quests[0].cancel_wait,
        Some(18_000.0),
        "kept through its text form"
    );
    for bad in ["cancel_wait = -1", "cancel_wait = \"long\""] {
        let mut files = fixture();
        edit(
            &mut files,
            "town/server/quests/40000088.toml",
            "duration = 72000",
            &format!("duration = 72000\n{bad}"),
        );
        assert!(read(&files).is_err(), "{bad}");
    }
}

/// Behaviour: server.gives.hero-transformation
#[test]
fn a_give_s_step_may_transform_its_player_and_no_other_step_may() {
    use yalain_content::quests::Transformation;
    const TRANSFORMS: &str = "transforms = { seconds = 25.59, fx = [0x410004A5, 0x410004B5], behavior = 0x41000120, text = { table = 0x25002BE3, string = 0x2EE20FE4 }, name = 0x9D066C50 }\n";
    let give = yalain_content::quests::Trigger::Give(0x4700_20B9, 0x4700_33D0);
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/quests/40000088.toml",
        "trigger = { give = 0x470020B9, to = 0x470033D0 }\n",
        &format!("trigger = {{ give = 0x470020B9, to = 0x470033D0 }}\n{TRANSFORMS}"),
    );
    assert_eq!(check(&files), Vec::new());
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    let transforms = |t: &yalain_content::QuestTable| {
        t.steps
            .iter()
            .find(|s| s.trigger == give)
            .and_then(|s| s.transforms.clone())
    };
    assert_eq!(
        transforms(&t.quests),
        Some(Transformation {
            seconds: 25.59,
            fx: vec![0x4100_04A5, 0x4100_04B5],
            behavior: Some(0x4100_0120),
            text: Some((0x2EE2_0FE4, 0x2500_2BE3)),
            name: 0x9D06_6C50,
        })
    );
    assert_eq!(
        yalain_content::QuestTable::from_text(&t.quests.to_text()).unwrap(),
        t.quests,
        "it reads back as written"
    );
    // Its time alone is enough.
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/quests/40000088.toml",
        "trigger = { give = 0x470020B9, to = 0x470033D0 }\n",
        "trigger = { give = 0x470020B9, to = 0x470033D0 }\ntransforms = { seconds = 3 }\n",
    );
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert_eq!(
        transforms(&t.quests),
        Some(Transformation {
            seconds: 3.0,
            ..Transformation::default()
        })
    );
    assert_eq!(
        yalain_content::QuestTable::from_text(&t.quests.to_text()).unwrap(),
        t.quests,
        "it reads back as written"
    );
    // Its time from 0, a name only for a text, and nothing else.
    for bad in [
        "transforms = { fx = [0x410004A5] }",
        "transforms = { seconds = -1 }",
        "transforms = { seconds = 3, name = 0x9D066C50 }",
        "transforms = { seconds = 3, text = { table = 0x25002BE3 } }",
        "transforms = { seconds = 3, glow = 1 }",
        "transforms = 25.59",
    ] {
        let mut files = fixture();
        edit(
            &mut files,
            "town/server/quests/40000088.toml",
            "trigger = { give = 0x470020B9, to = 0x470033D0 }\n",
            &format!("trigger = {{ give = 0x470020B9, to = 0x470033D0 }}\n{bad}\n"),
        );
        assert!(read(&files).is_err(), "{bad}");
    }
    let mut files = fixture();
    edit(
        &mut files,
        "town/server/quests/40000088.toml",
        "needs = 0\ntrigger = { use = 0x470033D0 }\n",
        &format!("needs = 0\ntrigger = {{ use = 0x470033D0 }}\n{TRANSFORMS}"),
    );
    let messages: Vec<String> = check(&files).iter().map(ToString::to_string).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("only a give's step transforms its player")),
        "{messages:?}"
    );
}

/// Behaviour: server.gives.vitae-tap
#[test]
fn an_effect_rule_row_may_take_a_share_of_vitae_with_or_without_a_ward() {
    let mut files = fixture();
    files.insert(
        "town/server/effects.toml".into(),
        "[[effect_rule]]\neffect = 0x6F001C64\nvitae_share = 0.1\nprov = { from = \"authored\" }\n\n\
         [[effect_rule]]\neffect = 0x6F000E94\nward = { classes = [93], chance = 1.0 }\nvitae_share = 0.5\nprov = { from = \"authored\" }\n"
            .into(),
    );
    assert_eq!(check(&files), Vec::new());
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    let rules = &t.effect_rules;
    assert_eq!(rules.vitae_taps.get(&0x6F00_1C64), Some(&0.1));
    assert_eq!(rules.vitae_taps.get(&0x6F00_0E94), Some(&0.5));
    assert_eq!(rules.wards.len(), 1, "the tap alone names no ward");
    assert_eq!(rules.wards[&0x6F00_0E94].classes, [93]);
    // A share above nothing and at most the whole.
    for share in ["0", "0.0", "1.5", "-0.1", "\"a tenth\""] {
        files.insert(
            "town/server/effects.toml".into(),
            format!(
                "[[effect_rule]]\neffect = 0x6F001C64\nvitae_share = {share}\nprov = {{ from = \"authored\" }}\n"
            ),
        );
        assert!(read(&files).is_err(), "{share}");
    }
    files.insert(
        "town/server/effects.toml".into(),
        "[[effect_rule]]\neffect = 0x6F001C64\nvitae_share = 1\nprov = { from = \"authored\" }\n"
            .into(),
    );
    assert!(read(&files).is_ok(), "the whole of it");
}

/// Behaviour: server.effects.immunities-as-rows-say
#[test]
fn an_effect_rule_row_may_name_what_an_immunity_leaves_as_it_ends() {
    let mut files = fixture();
    files.insert(
        "town/server/effects.toml".into(),
        "[[effect_rule]]\neffect = 0x70000720\nthen = 0x70000721\nprov = { from = \"authored\" }\n\n\
         [[effect_rule]]\neffect = 0x70000721\nward = { classes = [55, 100], chance = 1.0 }\nprov = { from = \"authored\" }\n"
            .into(),
    );
    assert_eq!(check(&files), Vec::new());
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    let rules = &t.effect_rules;
    assert_eq!(
        rules.immunities,
        std::collections::BTreeMap::from([(0x7000_0720, 0x7000_0721)])
    );
    assert_eq!(rules.wards[&0x7000_0721].classes, [55, 100]);
    assert!(
        !rules.wards.contains_key(&0x7000_0720),
        "the short one is no ward"
    );
    assert!(rules.vitae_taps.is_empty());
    // An immunity leaves another effect, one named by its saved state.
    for then in ["0x70000720", "\"the long one\"", "-1"] {
        files.insert(
            "town/server/effects.toml".into(),
            format!(
                "[[effect_rule]]\neffect = 0x70000720\nthen = {then}\nprov = {{ from = \"authored\" }}\n"
            ),
        );
        assert!(read(&files).is_err(), "{then}");
    }
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

/// A death treasure row names a template or a camp (in another pack by `pack:key@XXYY`), an item and
/// a chance above 0 and at most 1; it compiles into the loot table, which reads back as written.
/// Behaviour: server.content.death-treasure-row
#[test]
fn a_death_treasure_row_names_a_template_or_a_camp_and_compiles_into_the_loot_table() {
    use yalain_content::{DeathTreasure, Dropper, LootTable};
    let row = |key: &str, dropper: &str, item: u32, chance: f32| {
        format!(
            "[[death_treasure]]\nkey = \"{key}\"\n{dropper}\nitem = {item:#010x}\nchance = {chance}\n\
             prov = {{ from = \"authored\", note = \"a test\" }}\n"
        )
    };
    let with = |text: String| {
        let mut files = fixture();
        files.insert("town-fixes/server/death_treasures.toml".into(), text);
        files
    };
    let files = with(format!(
        "{}\n{}",
        row("by-kind", "template = 0x47002224", 0x4700_0001, 0.05),
        row("by-camp", "camp = \"town:beetles@77B7\"", 0x4700_115B, 1.0)
    ));
    assert_eq!(check(&files), Vec::new());
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    let got: Vec<(Dropper, u32, f32)> = t
        .loot
        .treasures
        .iter()
        .map(|d: &DeathTreasure| (d.from.clone(), d.item, d.chance))
        .collect();
    assert_eq!(
        got,
        [
            (Dropper::Template(0x4700_2224), 0x4700_0001, 0.05),
            (Dropper::Camp("beetles".into()), 0x4700_115B, 1.0),
        ]
    );
    assert_eq!(
        LootTable::from_text(&t.loot.to_text()).unwrap(),
        t.loot,
        "it reads back as written"
    );
    // Rows that do not read, or name what is not there.
    for (bad, says) in [
        (
            row(
                "k",
                "template = 0x47002224\ncamp = \"town:beetles@77B7\"",
                0x4700_0001,
                0.5,
            ),
            "one of them",
        ),
        (row("k", "", 0x4700_0001, 0.5), "one of them"),
        (
            row("k", "template = 0x47002224", 0x4700_0001, 0.0),
            "above 0",
        ),
        (
            row("k", "template = 0x47002224", 0x4700_0001, 1.5),
            "above 0",
        ),
        (
            row("k", "camp = \"town:wasps@77B7\"", 0x4700_0001, 0.5),
            "no camp wasps@77B7",
        ),
        (
            row("k", "camp = \"town:beetles\"", 0x4700_0001, 0.5),
            "key@XXYY",
        ),
    ] {
        let problems = check(&with(bad.clone()));
        assert!(
            problems.iter().any(|p| p.to_string().contains(says)),
            "{bad}: {problems:?}"
        );
    }
    for (bad, says) in [
        (
            row("k", "template = 0x47002224", 0x4700_9999, 0.5),
            "item: template 47009999",
        ),
        (
            format!(
                "{}\n{}",
                row("by-kind", "template = 0x47002224", 0x4700_0001, 0.5),
                row("by-camp", "camp = \"town:beetles@77B7\"", 0x4700_0001, 0.5)
            ),
            "item: 47000001 is given by another row",
        ),
    ] {
        let files = with(bad.clone());
        assert_eq!(check(&files), Vec::new(), "{bad}");
        let problems = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap_err();
        assert!(
            problems.iter().any(|p| p.to_string().contains(says)),
            "{bad}: {problems:?}"
        );
    }
}

#[test]
fn treasure_pools_and_a_profiles_draws_compile_and_bad_ones_are_problems() {
    let with = |pools: &str, loot: &str| {
        let mut files = fixture();
        let path = files
            .keys()
            .find(|k| k.ends_with("loot.toml"))
            .expect("loot rows")
            .clone();
        let text = files.get(&path).expect("read").replacen(
            "items = [{ template = 0x47002108, weight = 3 }]\n",
            loot,
            1,
        );
        files.insert(path.clone(), format!("{text}\n{pools}"));
        files
    };
    let good_pools = "[[treasure_pool]]\nname = \"GP_A\"\npools = [{ pool = \"GP_AHuman\", weight = 2.5 }]\nprov = { from = \"authored\" }\n\n[[treasure_pool]]\nname = \"GP_AHuman\"\nrace = \"human\"\nown = 0.6\nitems = [{ template = 0x4700115B, weight = 1, least = 3, most = 9 }]\nprov = { from = \"authored\" }\n";
    let good_draws = "draws = [{ pool = \"GP_A\", weight = 1, least = 2 }]\nrare = { pool = \"GP_A\", chance = 0.01 }\n";
    let files = with(good_pools, good_draws);
    assert_eq!(check(&files), Vec::new());
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    let human = &t.loot.pools.pools["GP_AHuman"];
    assert_eq!((human.race, human.own), (1, 0.6));
    assert_eq!((human.entries[0].least, human.entries[0].most), (3, 9));
    let draws = &t.loot.pools.profiles["GP_TestBeetleHoard"];
    assert_eq!(draws.draws[0].least, 2);
    assert_eq!(
        draws.draws[0].most,
        yalain_content::pools::OPEN_ABOVE,
        "open above"
    );
    assert_eq!(draws.rare, Some(("GP_A".to_string(), 0.01)));
    assert_eq!(
        yalain_content::LootTable::from_text(&t.loot.to_text()).unwrap(),
        t.loot,
        "it reads back as written"
    );
    for (pools, loot, says) in [
        (
            good_pools.replace("GP_AHuman\", weight", "GP_A\", weight"),
            good_draws.to_string(),
            "pools: GP_A leads back to itself",
        ),
        (
            good_pools.to_string(),
            good_draws.replace("pool = \"GP_A\", weight", "pool = \"GP_B\", weight"),
            "draws: no treasure pool GP_B",
        ),
        (
            good_pools.to_string(),
            format!("{good_draws}items = [{{ template = 0x47002108, weight = 3 }}]\n"),
            "not both",
        ),
        (
            good_pools.replace("race = \"human\"\n", ""),
            good_draws.to_string(),
            "own: a pool takes a share",
        ),
        (
            good_pools.replace("least = 3, most = 9", "least = 9, most = 3"),
            good_draws.to_string(),
            "a least level above its most",
        ),
        (
            good_pools.to_string(),
            good_draws.replace("chance = 0.01", "chance = 0"),
            "rare: a chance above 0",
        ),
    ] {
        assert!(
            check(&with(&pools, &loot))
                .iter()
                .any(|p| p.to_string().contains(says)),
            "{says}: {:?}",
            check(&with(&pools, &loot))
        );
    }
}

#[test]
fn an_effect_name_row_compiles_to_the_state_its_name_stands_for() {
    let mut files = fixture();
    files.insert(
        "town/server/effects.toml".into(),
        "[[effect_name]]\nname = \"Eff_Test_Unheld\"\nstate = 0x6F001234\nprov = { from = \"authored\" }\n"
            .into(),
    );
    let db = read(&files).unwrap();
    assert!(db
        .rows()
        .any(|l| matches!(&l.row, Row::EffectName(r) if r.name == "Eff_Test_Unheld" && r.state == 0x6F00_1234)));
    assert!(check(&files).is_empty(), "{:?}", check(&files));
    let t = compile(&merge(&db).unwrap(), &Fake::new()).unwrap();
    assert_eq!(t.effect_names.get("Eff_Test_Unheld"), Some(&0x6F00_1234));
    // A row names its state and nothing else.
    for bad in [
        "[[effect_name]]\nname = \"Eff_Test_Unheld\"\nprov = { from = \"authored\" }\n",
        "[[effect_name]]\nname = \"Eff_Test_Unheld\"\nstate = 0x6F001234\nlasts = 3\nprov = { from = \"authored\" }\n",
        "[[effect_name]]\nname = \"not an identifier\"\nstate = 0x6F001234\nprov = { from = \"authored\" }\n",
    ] {
        files.insert("town/server/effects.toml".into(), bad.into());
        assert!(read(&files).is_err(), "{bad}");
    }
}

#[test]
fn an_event_row_reads_its_windows_and_gates_compile_to_the_event_table() {
    use yalain_content::events::Window;
    let mut files = fixture();
    files.insert(
        "town/server/events.toml".into(),
        "[[event]]\nkey = \"fair\"\ngame_event = 0x40000149\n\
         when = [{ from = \"2026-12-24T09:00Z\", until = \"2026-12-25T09:00Z\" }, \
         { days = [\"sat\", \"sun\"], from = \"22:00\", until = \"02:00\" }]\n\
         on_players = [0x70002015]\nprov = { from = \"authored\" }\n"
            .into(),
    );
    edit(
        &mut files,
        "town-fixes/server/landblocks/77B7.toml",
        "key = \"beetles\"\n",
        "key = \"beetles\"\nevent = \"fair\"\n",
    );
    edit(
        &mut files,
        "town/server/landblocks/77B7.toml",
        "key = \"gate\"\n",
        "key = \"gate\"\nevent = \"fair\"\n",
    );
    edit(
        &mut files,
        "town/server/quests/40000088.toml",
        "requires = [{ quest = 0x40000016, done = true }",
        "requires = [{ event = \"fair\" }, { quest = 0x40000016, done = true }",
    );
    assert!(check(&files).is_empty(), "{:?}", check(&files));
    let db = read(&files).unwrap();
    let event = db
        .rows()
        .find_map(|l| match &l.row {
            Row::Event(e) => Some(e.clone()),
            _ => None,
        })
        .expect("the event");
    assert_eq!(
        (
            event.game_event,
            event.on_players.clone(),
            event.when.clone()
        ),
        (
            Some(0x4000_0149),
            vec![0x7000_2015],
            vec![
                Window::Dated {
                    from: 1_798_102_800,
                    until: 1_798_189_200
                },
                Window::Weekly {
                    days: (1 << 5) | (1 << 6),
                    from: 22 * 3600,
                    until: 2 * 3600
                },
            ]
        )
    );
    let t = compile(&merge(&db).unwrap(), &Fake::new()).unwrap();
    let ev = &t.content.events;
    assert_eq!(ev.events.len(), 1);
    assert_eq!(ev.of_camp("beetles"), Some("fair"));
    assert_eq!(ev.objects.len(), 1);
    assert_eq!(ev.steps.len(), 1);
    assert_eq!(
        ev.steps[0].0.requires.len(),
        2,
        "the quests it requires are kept"
    );
    // A gate naming no event is a problem; so are bad times and unknown fields.
    let mut no_event = files.clone();
    no_event.remove("town/server/events.toml");
    let problems = check(&no_event);
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems
        .iter()
        .all(|p| p.message.contains("no [[event]] fair")));
    for bad in [
        "when = [{ from = \"2026-12-24T09:00\", until = \"2026-12-25T09:00Z\" }]",
        "when = [{ from = \"2026-12-25T09:00Z\", until = \"2026-12-24T09:00Z\" }]",
        "when = [{ days = [\"someday\"], from = \"22:00\", until = \"02:00\" }]",
        "when = [{ days = [\"sat\"], from = \"25:00\", until = \"02:00\" }]",
        "when = []",
        "when = [{ days = [\"sat\"], from = \"22:00\", until = \"02:00\" }]\nlasts = 3",
    ] {
        let mut f = files.clone();
        f.insert(
            "town/server/events.toml".into(),
            format!("[[event]]\nkey = \"fair\"\n{bad}\nprov = {{ from = \"authored\" }}\n"),
        );
        assert!(read(&f).is_err(), "{bad}");
    }
}

#[test]
fn popup_rows_compile_by_moment_and_bad_ones_are_problems() {
    use yalain_content::popups::Moment;
    let mut files = fixture();
    files.insert(
        "town/server/popups.toml".into(),
        "[[popup]]\non = \"first-entry\"\neffect = 0x6F001923\nprov = { from = \"authored\" }\n\n\
         [[popup]]\non = \"level-up\"\neffect = 0x6F001925\nprov = { from = \"authored\" }\n\n\
         [[popup]]\non = \"glyph\"\neffect = 0x6F001924\nprov = { from = \"authored\" }\n"
            .into(),
    );
    let db = read(&files).unwrap();
    assert!(check(&files).is_empty(), "{:?}", check(&files));
    let t = compile(&merge(&db).unwrap(), &Fake::new()).unwrap();
    assert_eq!(t.popups.effect(Moment::FirstEntry), Some(0x6F00_1923));
    assert_eq!(t.popups.effect(Moment::LevelUp), Some(0x6F00_1925));
    assert_eq!(t.popups.effect(Moment::Glyph), Some(0x6F00_1924));
    assert_eq!(
        compile(&merge(&read(&fixture()).unwrap()).unwrap(), &Fake::new())
            .unwrap()
            .popups
            .by_moment
            .len(),
        0,
        "a world with no rows has no tips"
    );

    // One row a moment.
    let mut twice = files.clone();
    twice.insert(
        "town/server/popups.toml".into(),
        "[[popup]]\non = \"glyph\"\neffect = 0x6F001924\nprov = { from = \"authored\" }\n\n\
         [[popup]]\non = \"glyph\"\neffect = 0x6F001925\nprov = { from = \"authored\" }\n"
            .into(),
    );
    let problems = check(&twice);
    assert!(
        problems.iter().any(|p| p.message.contains("set twice")),
        "{problems:?}"
    );

    // A moment by its word, and an effect; nothing else.
    for bad in [
        "on = \"first_entry\"\neffect = 0x6F001923",
        "on = \"login\"\neffect = 0x6F001923",
        "on = 1\neffect = 0x6F001923",
        "effect = 0x6F001923",
        "on = \"glyph\"",
        "on = \"glyph\"\neffect = 0x6F001924\ntext = 3",
    ] {
        let mut f = files.clone();
        f.insert(
            "town/server/popups.toml".into(),
            format!("[[popup]]\n{bad}\nprov = {{ from = \"authored\" }}\n"),
        );
        assert!(read(&f).is_err(), "{bad}");
    }
}

#[test]
fn item_use_book_and_start_item_rows_compile_and_bad_ones_are_problems() {
    use yalain_content::item_rows::{ItemUse, Shows};
    let mut files = fixture();
    files.insert(
        "town/server/items.toml".into(),
        "[[item_use]]\ntemplate = 0x4700115B\neffects = [{ effect = 0x6F002255, spellcraft = 40 }, { effect = 0x70000A13 }]\n\
         behavior = 0x41000023\nwears = 5\nprov = { from = \"authored\" }\n\n\
         [[item_use]]\ntemplate = 0x47002224\neffects = [{ effect = 0x70000A00 }]\nprov = { from = \"authored\" }\n\n\
         [[book]]\ntemplate = 0x4700115B\npicture = 0x410091EC\nprov = { from = \"authored\" }\n\n\
         [[book]]\ntemplate = 0x47002224\ntable = 0x25002C90\nprov = { from = \"authored\" }\n\n\
         [[start_item]]\ntemplate = 0x4700115B\nprov = { from = \"authored\" }\n\n\
         [[start_item]]\ntemplate = 0x47002224\ncount = 3\nprov = { from = \"authored\" }\n"
            .into(),
    );
    let db = read(&files).unwrap();
    assert!(check(&files).is_empty(), "{:?}", check(&files));
    let t = compile(&merge(&db).unwrap(), &Fake::new()).unwrap();
    assert_eq!(
        t.items.uses.get(&0x4700_115B),
        Some(&ItemUse {
            template: 0x4700_115B,
            effects: vec![(0x6F00_2255, 40.0), (0x7000_0A13, 1.0)],
            behavior: Some(0x4100_0023),
            wears: Some(5),
        })
    );
    assert_eq!(
        t.items.uses.get(&0x4700_2224),
        Some(&ItemUse {
            template: 0x4700_2224,
            effects: vec![(0x7000_0A00, 1.0)],
            behavior: None,
            wears: None,
        })
    );
    assert_eq!(
        t.items.books.get(&0x4700_115B),
        Some(&Shows::Picture(0x4100_91EC))
    );
    assert_eq!(
        t.items.books.get(&0x4700_2224),
        Some(&Shows::Book(0x2500_2C90))
    );
    assert_eq!(
        t.items
            .start_items
            .iter()
            .map(|(t, n)| (*t, *n))
            .collect::<Vec<_>>(),
        [(0x4700_115B, 1), (0x4700_2224, 3)],
        "one when the count is left out"
    );
    let none = compile(&merge(&read(&fixture()).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert_eq!(
        none.items,
        yalain_content::item_rows::ItemRows::default(),
        "a world with no rows has none"
    );

    // A template the install lacks is a problem.
    let mut lost = fixture();
    lost.insert(
        "town/server/items.toml".into(),
        "[[start_item]]\ntemplate = 0x47009999\nprov = { from = \"authored\" }\n".into(),
    );
    let problems = compile(&merge(&read(&lost).unwrap()).unwrap(), &Fake::new()).unwrap_err();
    assert!(
        problems.iter().any(|p| p
            .to_string()
            .contains("template 47009999 is not in the install")),
        "{problems:?}"
    );

    // One row a template, of each kind.
    let mut twice = files.clone();
    twice.insert(
        "town/server/more.toml".into(),
        "[[book]]\ntemplate = 0x4700115B\ntable = 0x25002C90\nprov = { from = \"authored\" }\n"
            .into(),
    );
    assert!(
        check(&twice)
            .iter()
            .any(|p| p.message.contains("set twice")),
        "{:?}",
        check(&twice)
    );

    // Their shapes.
    for (kind, bad) in [
        ("item_use", "template = 0x4700115B"),
        ("item_use", "template = 0x4700115B\neffects = []"),
        ("item_use", "template = 0x4700115B\neffects = [0x70000A13]"),
        (
            "item_use",
            "template = 0x4700115B\neffects = [{ state = 0x70000A13 }]",
        ),
        (
            "item_use",
            "template = 0x4700115B\neffects = [{ effect = 0x70000A13, spellcraft = -1 }]",
        ),
        (
            "item_use",
            "template = 0x4700115B\neffects = [{ effect = 0x70000A13 }]\nwears = 0",
        ),
        ("item_use", "effects = [{ effect = 0x70000A13 }]"),
        ("book", "template = 0x4700115B"),
        (
            "book",
            "template = 0x4700115B\ntable = 0x25002C90\npicture = 0x410091EC",
        ),
        ("book", "picture = 0x410091EC"),
        ("start_item", "template = 0x4700115B\ncount = 0"),
        ("start_item", "count = 2"),
        ("start_item", "template = 0x4700115B\nquantity = 2"),
    ] {
        let mut f = fixture();
        f.insert(
            "town/server/items.toml".into(),
            format!("[[{kind}]]\n{bad}\nprov = {{ from = \"authored\" }}\n"),
        );
        assert!(read(&f).is_err(), "{kind}: {bad}");
    }
}

#[test]
fn points_bonus_rows_and_kingdom_requirements_compile_and_bad_ones_are_problems() {
    use yalain_content::kingdoms::{kingdom, PointsBonus};
    let mut files = fixture();
    files.insert(
        "town/server/kingdoms.toml".into(),
        "[[points_bonus]]\neffect = 0x6F00188E\nadd = 200\nup_to = 1500\nprov = { from = \"authored\" }\n"
            .into(),
    );
    edit(
        &mut files,
        "town/server/quests/40000088.toml",
        "update = \"bestow\"\n",
        "update = \"bestow\"\nrequires = [{ kingdom = \"order\" }]\n",
    );
    files.insert(
        "town/server/gives/470033D0.toml".into(),
        "[[give]]\nkey = \"gem\"\nto = 0x470033D0\nitem = 0x470020BB\neffects = [0x6F00188E]\n\
         requires = [{ quest = 0x40000016, done = true }, { kingdom = \"dominion\" }]\n\
         prov = { from = \"authored\" }\n"
            .into(),
    );
    let db = read(&files).unwrap();
    assert!(db.rows().any(|l| matches!(&l.row, Row::PointsBonus(r)
        if r.effect == 0x6F00_188E && r.add == 200 && r.up_to == 1500)));
    assert!(check(&files).is_empty(), "{:?}", check(&files));
    let t = compile(&merge(&db).unwrap(), &Fake::new()).unwrap();
    assert_eq!(
        t.kingdoms.points.get(&0x6F00_188E),
        Some(&PointsBonus {
            add: 200,
            up_to: 1500
        })
    );
    assert_eq!(t.kingdoms.steps.len(), 1);
    assert_eq!(t.kingdoms.steps[0].1, kingdom::ORDER);
    assert_eq!(t.kingdoms.steps[0].0.trigger, Trigger::Use(0x4700_33D0));
    assert_eq!(t.kingdoms.gives.len(), 1);
    let (give, k) = &t.kingdoms.gives[0];
    assert_eq!(*k, kingdom::DOMINION);
    assert_eq!(
        give.requires,
        [(0x4000_0016, Need::Done)],
        "the quest it requires is kept beside the kingdom"
    );
    assert!(t.quests.gives.contains(give), "the give itself is served");

    // One row an effect.
    let mut twice = files.clone();
    twice.insert(
        "town/server/kingdoms.toml".into(),
        "[[points_bonus]]\neffect = 0x6F00188E\nadd = 200\nup_to = 1500\nprov = { from = \"authored\" }\n\n\
         [[points_bonus]]\neffect = 0x6F00188E\nadd = 250\nup_to = 1750\nprov = { from = \"authored\" }\n"
            .into(),
    );
    let problems = check(&twice);
    assert!(
        problems.iter().any(|p| p.message.contains("set twice")),
        "{problems:?}"
    );

    // An amount above 0 and a cap the points can hold; nothing else.
    for bad in [
        "effect = 0x6F00188E\nadd = 0\nup_to = 1500",
        "effect = 0x6F00188E\nadd = -200\nup_to = 1500",
        "effect = 0x6F00188E\nadd = 2147483648\nup_to = 1500",
        "effect = 0x6F00188E\nadd = 200\nup_to = 2147483648",
        "effect = 0x6F00188E\nadd = 200",
        "add = 200\nup_to = 1500",
        "effect = 0x6F00188E\nadd = 200\nup_to = 1500\nlasts = 3",
    ] {
        let mut f = files.clone();
        f.insert(
            "town/server/kingdoms.toml".into(),
            format!("[[points_bonus]]\n{bad}\nprov = {{ from = \"authored\" }}\n"),
        );
        assert!(read(&f).is_err(), "{bad}");
    }

    // A kingdom by its word, one at most, alone in its table; a give's requires names no event.
    for bad in [
        "requires = [{ kingdom = \"elves\" }]",
        "requires = [{ kingdom = 2 }]",
        "requires = [{ kingdom = \"order\" }, { kingdom = \"shadow\" }]",
        "requires = [{ kingdom = \"order\", quest = 0x40000016 }]",
        "requires = [{ event = \"fair\" }]",
    ] {
        let mut f = files.clone();
        f.insert(
            "town/server/gives/470033D0.toml".into(),
            format!(
                "[[give]]\nkey = \"gem\"\nto = 0x470033D0\nitem = 0x470020BB\n{bad}\nprov = {{ from = \"authored\" }}\n"
            ),
        );
        let problems = check(&f);
        assert!(
            problems.iter().any(|p| p.message.contains("requires")),
            "{bad}: {problems:?}"
        );
    }
    let mut f = files.clone();
    edit(
        &mut f,
        "town/server/quests/40000088.toml",
        "requires = [{ kingdom = \"order\" }]",
        "requires = [{ kingdom = \"elves\" }]",
    );
    assert!(read(&f).is_err(), "a step's unknown kingdom");
}

#[test]
fn treasure_values_compile_by_level_and_bad_ones_are_problems() {
    let with = |rows: &str| {
        let mut files = fixture();
        let path = files
            .keys()
            .find(|k| k.ends_with("loot.toml"))
            .expect("loot rows")
            .clone();
        let text = files.get(&path).expect("read").clone();
        files.insert(path, format!("{text}\n{rows}"));
        files
    };
    let good = "[[treasure_value]]\nlevel = 3\nvalues = [4, 4, 9]\nprov = { from = \"authored\" }\n\n[[treasure_value]]\nlevel = 7\nvalues = [20]\nprov = { from = \"authored\" }\n";
    let files = with(good);
    assert_eq!(check(&files), Vec::new());
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert_eq!(t.loot.values.get(&3), Some(&vec![4, 4, 9]), "as given");
    assert_eq!(t.loot.values_at(3), [4, 4, 9]);
    assert_eq!(t.loot.values_at(5), [4, 4, 9], "the lower of two as near");
    assert_eq!(t.loot.values_at(6), [20], "the nearest");
    assert_eq!(t.loot.values_at(90), [20], "the nearest, above all");
    assert_eq!(
        yalain_content::LootTable::from_text(&t.loot.to_text()).unwrap(),
        t.loot,
        "it reads back as written"
    );
    for (rows, says) in [
        (good.replace("values = [20]", "values = []"), "a list of at least one value"),
        (good.replace("values = [20]", "values = [-1]"), "a list of values"),
        (good.replace("level = 7\n", ""), "level"),
        (
            format!("{good}\n[[treasure_value]]\nlevel = 7\nvalues = [21]\nprov = {{ from = \"authored\" }}\n"),
            "treasure_value 7",
        ),
    ] {
        let files = with(&rows);
        let problems: Vec<String> = check(&files)
            .iter()
            .map(ToString::to_string)
            .chain(
                read(&files)
                    .ok()
                    .and_then(|r| merge(&r).err())
                    .into_iter()
                    .flatten()
                    .map(|p| p.to_string()),
            )
            .collect();
        assert!(
            problems.iter().any(|p| p.contains(says)),
            "{says}: {problems:?}"
        );
    }
}

/// The treasure numbers written as `[[treasure_rules]]` and `[[treasure_sort]]` rows, each weighted
/// list in its order.
fn treasure_rows(t: &yalain_content::treasure_tables::TreasureTables) -> String {
    use std::fmt::Write as _;
    use yalain_content::treasure_tables::Sort;
    let floats = |xs: &[f32]| {
        let v: Vec<String> = xs.iter().map(|x| format!("{x:?}")).collect();
        format!("[{}]", v.join(", "))
    };
    let r = &t.rules;
    let mut out = String::new();
    let _ = write!(
        out,
        "[[treasure_rules]]\nabove_base = {}\nmost_level = {}\nlore = {{ above = {}, from = {}, to = {} }}\npotion_tiers = {:?}\npotion_spellcraft = [{:?}, {:?}]\nlook = {{ glow_most = {:?}, round_chance = {:?}, round_from = {:?}, round_tenths = {} }}\nprov = {{ from = \"authored\" }}\n",
        floats(&r.above_base),
        r.most_level,
        r.lore.above,
        r.lore.from,
        r.lore.to,
        r.potion_tiers,
        r.potion_spellcraft.0,
        r.potion_spellcraft.1,
        r.look.glow_most,
        r.look.round_chance,
        r.look.round_from,
        r.look.round_tenths,
    );
    for sort in Sort::ALL {
        let s = t.sort(sort);
        let _ = write!(
            out,
            "\n[[treasure_sort]]\nsort = \"{}\"\ncounts = [",
            sort.name()
        );
        let bands: Vec<String> = s
            .counts
            .iter()
            .map(|b| format!("{{ from = {}, weights = {} }}", b.from, floats(&b.weights)))
            .collect();
        let _ = writeln!(out, "{}]", bands.join(", "));
        if let Some((lo, hi)) = s.damage_share {
            let _ = writeln!(out, "damage_share = [{lo:?}, {hi:?}]");
        }
        if !s.vigor.is_empty() {
            let bands: Vec<String> = s
                .vigor
                .iter()
                .map(|b| {
                    let every = b
                        .every
                        .map(|e| format!(", every = {e}"))
                        .unwrap_or_default();
                    format!(
                        "{{ from = {}, least = {}, most = {}{every} }}",
                        b.from, b.least, b.most
                    )
                })
                .collect();
            let _ = writeln!(out, "vigor = [{}]", bands.join(", "));
        }
        if let Some((lo, hi)) = s.span {
            let _ = writeln!(out, "span = [{lo:?}, {hi:?}]");
        }
        if let Some(d) = &s.delays {
            let _ = writeln!(
                out,
                "delays = {{ from = {}, weights = {} }}",
                d.from,
                floats(&d.weights)
            );
        }
        if !s.rating_share.is_empty() {
            let points: Vec<String> = s
                .rating_share
                .iter()
                .map(|(l, x)| format!("[{l}, {x:?}]"))
                .collect();
            let _ = writeln!(out, "rating_share = [{}]", points.join(", "));
            let _ = writeln!(out, "rating_spread = {:?}", s.rating_spread);
            let _ = writeln!(out, "slot_chance = {:?}", s.slot_chance);
        }
        let _ = writeln!(out, "prov = {{ from = \"authored\" }}");
    }
    out
}

/// The words, looks and potions written as `[[treasure_word]]`, `[[treasure_look]]` and
/// `[[treasure_potion]]` rows, words and potions with their order.
fn treasure_text_rows(t: &yalain_content::treasure_tables::TreasureTables) -> String {
    use std::fmt::Write as _;
    use yalain_content::treasure_tables::{Sort, POTION_KINDS};
    let floats = |xs: &[f32]| {
        let v: Vec<String> = xs.iter().map(|x| format!("{x:?}")).collect();
        format!("[{}]", v.join(", "))
    };
    let mut out = String::new();
    for sort in Sort::ALL {
        for (order, w) in t.sort(sort).words.iter().enumerate() {
            let _ = writeln!(
                out,
                "[[treasure_word]]\nsort = \"{}\"\norder = {order}\nword = {:#010x}\ndelays = [{}, {}]\nweights = {}\nprov = {{ from = \"authored\" }}\n",
                sort.name(),
                w.word,
                w.delays.0,
                w.delays.1,
                floats(&w.weights)
            );
        }
    }
    for l in &t.looks {
        let keys: Vec<String> = l
            .keys
            .iter()
            .map(|(k, m)| format!("{{ key = {k:#010x}, most = {m:?} }}"))
            .collect();
        let _ = writeln!(
            out,
            "[[treasure_look]]\ntemplate = {:#010x}\ntable = {:#010x}\nkeys = [{}]\nprov = {{ from = \"authored\" }}\n",
            l.template,
            l.table,
            keys.join(", ")
        );
    }
    for sort in Sort::ALL {
        for (order, f) in t.sort(sort).families.iter().enumerate() {
            let when = match f.when {
                yalain_content::treasure_effects::When::Permanent => "permanent",
                yalain_content::treasure_effects::When::Equipped => "equipped",
                yalain_content::treasure_effects::When::Hit => "hit",
            };
            let tiers: Vec<String> = f
                .tiers
                .iter()
                .map(|x| {
                    let mut one = format!(
                        "{{ least = {}, state = {:#010x}, chance = {:?}, glow = {:?}",
                        x.least, x.state, x.chance, x.glow
                    );
                    if x.puts_on != 0 {
                        let _ = write!(one, ", puts_on = {:#010x}", x.puts_on);
                    }
                    if let Some((_, p)) = t.pairs.iter().find(|(a, _)| *a == x.state) {
                        let _ = write!(one, ", pair = {p:#010x}");
                    }
                    if t.unnaming.contains(&x.state) {
                        one.push_str(", named = false");
                    }
                    one.push_str(" }");
                    one
                })
                .collect();
            let _ = writeln!(
                out,
                "[[treasure_family]]\nsort = \"{}\"\norder = {order}\nwhen = \"{when}\"\nweight = {:?}\nby_level = {}\ntiers = [{}]\nprov = {{ from = \"authored\" }}\n",
                sort.name(),
                f.weight,
                f.by_level,
                tiers.join(", ")
            );
        }
    }
    for (order, states) in t.potions.iter().enumerate() {
        let states: Vec<String> = states.iter().map(|s| format!("{s:#010x}")).collect();
        let _ = writeln!(
            out,
            "[[treasure_potion]]\nkind = \"{}\"\norder = {order}\nstates = [{}]\nprov = {{ from = \"authored\" }}\n",
            POTION_KINDS[order],
            states.join(", ")
        );
    }
    out
}

/// The test world with these rows in a file of its town's.
fn with_treasure(rows: &str) -> Files {
    let mut files = fixture();
    files.insert("town/server/treasure.toml".into(), rows.to_string());
    files
}

/// Every problem the check and the merge find.
fn problems_of(files: &Files) -> Vec<String> {
    check(files).iter().map(ToString::to_string).collect()
}

#[test]
fn the_2017_treasure_numbers_written_as_rows_compile_back_to_the_same_tables() {
    use yalain_content::treasure_tables::TreasureTables;
    let defaults = TreasureTables::retail_2017();
    let mut files = with_treasure(&treasure_rows(&defaults));
    files.insert(
        "town/server/treasure_words.toml".into(),
        treasure_text_rows(&defaults),
    );
    assert_eq!(problems_of(&files), Vec::<String>::new());
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert!(
        !t.warnings.iter().any(|w| w.message.contains("treasure")),
        "every band has a weighted word: {:?}",
        t.warnings
    );
    let (tables, fell_back) = TreasureTables::with_rows(&t.treasure);
    assert_eq!(fell_back, Vec::<&str>::new(), "the world gives every kind");
    assert_eq!(tables, defaults, "the rows are 2017's numbers, exactly");
    // A world with none plays 2017's, and says which kinds it fell back to.
    let t = compile(&merge(&read(&fixture()).unwrap()).unwrap(), &Fake::new()).unwrap();
    let (tables, fell_back) = TreasureTables::with_rows(&t.treasure);
    assert_eq!(
        fell_back,
        [
            "treasure_rules",
            "treasure_sort",
            "treasure_word",
            "treasure_look",
            "treasure_potion",
            "treasure_family"
        ]
    );
    assert_eq!(tables, defaults);
}

#[test]
fn a_worlds_treasure_rows_replace_the_kinds_they_give_whole() {
    use yalain_content::treasure_tables::{Sort, TreasureTables};
    let defaults = TreasureTables::retail_2017();
    let mut own = defaults.clone();
    own.rules.most_level = 40;
    own.rules.above_base = vec![0.0, 1.0];
    own.weapon.vigor[0].most = 2;
    own.shield.slot_chance = 0.25;
    own.jewelry.counts = vec![yalain_content::treasure_tables::CountBand {
        from: 0,
        weights: vec![1.0, 1.0],
    }];
    let files = with_treasure(&treasure_rows(&own));
    assert_eq!(problems_of(&files), Vec::<String>::new());
    let t = compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).unwrap();
    let (tables, _) = TreasureTables::with_rows(&t.treasure);
    assert_eq!(tables, own, "the world's numbers");
    assert_eq!(
        tables.sort(Sort::Weapon).families,
        defaults.sort(Sort::Weapon).families,
        "a sort keeps its families and words"
    );
}

#[test]
fn treasure_rows_that_break_their_rules_are_problems() {
    use yalain_content::treasure_tables::TreasureTables;
    let good = treasure_rows(&TreasureTables::retail_2017());
    assert_eq!(problems_of(&with_treasure(&good)), Vec::<String>::new());
    let shield_at = good.find("sort = \"shield\"").expect("a shield row");
    let without_shield = {
        let start = good[..shield_at]
            .rfind("[[treasure_sort]]")
            .expect("its header");
        let end = good[shield_at..]
            .find("[[treasure_sort]]")
            .map_or(good.len(), |e| shield_at + e);
        format!("{}{}", &good[..start], &good[end..])
    };
    for (rows, says) in [
        (without_shield, "no row of shield"),
        (
            good.replace("above_base = [436.0, 519.0, 220.0, 39.0]", "above_base = [0.0, 0.0]"),
            "above_base: weights",
        ),
        (
            good.replace("potion_tiers = [0, 16, 30, 40, 45]", "potion_tiers = [0, 16, 30, 45, 40]"),
            "potion_tiers",
        ),
        (
            good.replace("potion_tiers = [0, 16, 30, 40, 45]", "potion_tiers = [0, 16, 30, 40]"),
            "the server's own potions have 5 states",
        ),
        (
            good.replace("lore = { above = 51, from = -6, to = -1 }", "lore = { above = 51, from = -1, to = -6 }"),
            "lore: from at most to",
        ),
        (good.replace("most_level = 50", "most_level = 0"), "most_level"),
        (
            good.replace("round_chance = 0.1", "round_chance = 1.5"),
            "look: glow_most",
        ),
        (
            good.replacen("{ from = 0, least = 1, most = 1 }", "{ from = 0, least = 1, most = 1, every = 5 }", 1),
            "vigor: bands",
        ),
        (good.replacen("{ from = 8, least = 3, most = 3 }", "{ from = 8, least = 3, most = 2 }", 1), "vigor: bands"),
        (good.replacen("{ from = 10, weights", "{ from = 9, weights", 0).replacen("counts = [{ from = 0,", "counts = [{ from = 1,", 1), "counts: bands"),
        (
            good.replacen("{ from = 50, weights = [610.0, 64.0, 36.0, 12.0] }", "", 1).replacen(", ]", "]", 1),
            "the server's own words for a weapon are weighted by 6",
        ),
        (good.replacen("damage_share = [0.84, 0.97]", "damage_share = [0.97, 0.84]", 1), "damage_share"),
        (good.replacen("sort = \"jewelry\"", "sort = \"jewelry\"\nspan = [0.25, 0.29]", 1), "span: not a field of a jewelry row"),
        (good.replacen("span = [0.25, 0.29]\n", "", 1), "span: missing: a shield row has it"),
        (good.replacen("sort = \"weapon\"", "sort = \"wand\"", 1), "sort: weapon, armor, shield or jewelry"),
        (good.replacen("rating_share = [[0, 0.75], [50, 1.0]]", "rating_share = [[50, 0.75], [0, 1.0]]", 1), "rating_share"),
        (good.replacen("slot_chance = 0.5", "slot_chance = 2.0", 1), "slot_chance: from 0 to 1"),
        (format!("{good}\n[[treasure_rules]]\nabove_base = [1.0]\nmost_level = 9\nlore = {{ above = 51, from = -6, to = -1 }}\npotion_tiers = [0, 16, 30, 40, 45]\npotion_spellcraft = [0.0, 100.0]\nlook = {{ glow_most = 0.65, round_chance = 0.1, round_from = 0.1, round_tenths = 4 }}\nprov = {{ from = \"authored\" }}\n"), "treasure_rules"),
    ] {
        let files = with_treasure(&rows);
        let problems: Vec<String> = problems_of(&files);
        assert!(
            problems.iter().any(|p| p.contains(says)),
            "{says}: {problems:?}"
        );
    }
}

#[test]
fn treasure_words_looks_and_potions_that_break_their_rules_are_problems() {
    use yalain_content::treasure_tables::TreasureTables;
    let defaults = TreasureTables::retail_2017();
    let numbers = treasure_rows(&defaults);
    let good = treasure_text_rows(&defaults);
    let files = |numbers: &str, text: &str| {
        let mut f = with_treasure(numbers);
        f.insert("town/server/treasure_words.toml".into(), text.to_string());
        f
    };
    assert_eq!(problems_of(&files(&numbers, &good)), Vec::<String>::new());
    // Counts of three bands with the server's own words: refused; with words of three weights: fine.
    let three = numbers.replacen(
        "{ from = 0, weights = [102.0, 20.0, 0.0, 0.0] }, { from = 10, weights = [267.0, 34.0, 1.0, 1.0] }, { from = 20, weights = [462.0, 43.0, 7.0, 1.0] }, ",
        "",
        1,
    );
    assert_ne!(three, numbers);
    let three = three.replacen("counts = [{ from = 30,", "counts = [{ from = 0,", 1);
    let problems = problems_of(&with_treasure(&three));
    assert!(
        problems
            .iter()
            .any(|p| p.contains("the server's own words for a weapon are weighted by 6")),
        "{problems:?}"
    );
    let mut weapon_words = String::new();
    for (i, w) in defaults.weapon.words.iter().enumerate() {
        weapon_words.push_str(&format!(
            "[[treasure_word]]\nsort = \"weapon\"\norder = {i}\nword = {:#010x}\ndelays = [{}, {}]\nweights = [1.0, 1.0, 1.0]\nprov = {{ from = \"authored\" }}\n\n",
            w.word, w.delays.0, w.delays.1
        ));
    }
    let f = files(&three, &weapon_words);
    assert_eq!(problems_of(&f), Vec::<String>::new());
    let t = compile(&merge(&read(&f).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert!(
        t.warnings
            .iter()
            .any(|w| w.message.contains("no armor word is weighted")),
        "a sort the world gives no words: {:?}",
        t.warnings
    );
    let first_word = format!("word = {:#010x}", defaults.weapon.words[0].word);
    for (text, says) in [
        (
            good.replacen(
                "weights = [0.0, 136.0, 149.0, 29.0, 0.0, 0.0]",
                "weights = [0.0, 136.0, 149.0, 29.0, 0.0]",
                1,
            ),
            "weights: 5, but a weapon's counts have 6 bands",
        ),
        (
            good.replacen(
                "sort = \"weapon\"\norder = 1\n",
                "sort = \"weapon\"\norder = 0\n",
                1,
            ),
            "order 0: another weapon word has it",
        ),
        (
            good.replacen("sort = \"weapon\"", "sort = \"jewelry\"", 1),
            "sort: weapon, armor or shield",
        ),
        (
            good.replacen(
                &format!("{first_word}\ndelays = [7, 18]"),
                &format!("{first_word}\ndelays = [18, 7]"),
                1,
            ),
            "delays: least at most most",
        ),
        (
            good.replacen(
                "keys = [{ key = 0x41000021, most = 1.0 }]",
                "keys = [{ key = 0x41000021, most = 1.5 }]",
                1,
            ),
            "keys: one to three keys",
        ),
        (
            good.replacen("keys = [{ key = 0x41000021, most = 1.0 }]", "keys = []", 1),
            "keys: one to three keys",
        ),
        (
            good.replacen(
                "order = 0\nstates = [0x700009f7, 0x700009f4, 0x700009f5, 0x700009f8, 0x700009f6]",
                "order = 0\nstates = [0x700009f7, 0x700009f4]",
                1,
            ),
            "states: 2, but the potions have 5 tiers",
        ),
        (
            good.replacen(
                "kind = \"damage-reflected\"\norder = 1",
                "kind = \"damage-reflected\"\norder = 0",
                1,
            ),
            "order 0: another potion has it",
        ),
    ] {
        assert_ne!(text, good, "{says}: the case changes the rows");
        let problems = problems_of(&files(&numbers, &text));
        assert!(
            problems.iter().any(|p| p.contains(says)),
            "{says}: {problems:?}"
        );
    }
}

#[test]
fn treasure_families_that_break_their_rules_are_problems_and_a_dropped_state_is_warned_of() {
    use yalain_content::treasure_tables::TreasureTables;
    let defaults = TreasureTables::retail_2017();
    let good = treasure_text_rows(&defaults);
    let files = |text: &str| {
        let mut f = fixture();
        f.insert("town/server/treasure_words.toml".into(), text.to_string());
        f
    };
    assert_eq!(problems_of(&files(&good)), Vec::<String>::new());
    let first = format!(
        "state = {:#010x}",
        defaults.weapon.families[0].tiers[0].state
    );
    let second = format!(
        "state = {:#010x}",
        defaults.weapon.families[0].tiers[1].state
    );
    for (text, says) in [
        (
            good.replacen("when = \"hit\"", "when = \"struck\"", 1),
            "is not permanent, equipped or hit",
        ),
        (
            good.replacen("order = 1\nwhen", "order = 0\nwhen", 1),
            "treasure_family weapon 0 is set twice",
        ),
        (
            good.replacen(&second, &first, 1),
            "is a tier of another family too",
        ),
        (
            good.replacen("pair = 0x6f00266e", "pair = 0x6f00ffff", 1),
            "is no tier of the world's families",
        ),
        (
            good.replacen("{ least = 16,", "{ least = -1,", 1),
            "tiers: at least one, least from 0",
        ),
        (
            good.replacen("{ least = 16,", "{ least = 99,", 1),
            "a family by level gives its tiers from the lowest",
        ),
        (
            good.replacen("chance = 0.1,", "chance = 1.5,", 1),
            "tiers: at least one",
        ),
        (
            good.replacen("when = \"hit\"", "when = \"permanent\"", 1),
            "puts_on only in a family of effects on what an item hits",
        ),
    ] {
        assert_ne!(text, good, "{says}: the case changes the rows");
        let problems = problems_of(&files(&text));
        assert!(
            problems.iter().any(|p| p.contains(says)),
            "{says}: {problems:?}"
        );
    }
    // A world whose families leave out one of the server's own states is warned of, not refused.
    let start = good.find("[[treasure_family]]").expect("a family");
    let end = good[start + 1..]
        .find("[[treasure_family]]")
        .map_or(good.len(), |e| start + 1 + e);
    let dropped = format!("{}{}", &good[..start], &good[end..]);
    let f = files(&dropped);
    assert_eq!(problems_of(&f), Vec::<String>::new(), "not refused");
    let t = compile(&merge(&read(&f).unwrap()).unwrap(), &Fake::new()).unwrap();
    let warned: Vec<&str> = t
        .warnings
        .iter()
        .map(|w| w.message.as_str())
        .filter(|m| m.contains("enchantments are in none of the world's families"))
        .collect();
    assert_eq!(warned.len(), 1, "{:?}", t.warnings);
    assert!(
        warned[0].starts_with("[[treasure_family]]: 5 of"),
        "{}",
        warned[0]
    );
    let t = compile(&merge(&read(&files(&good)).unwrap()).unwrap(), &Fake::new()).unwrap();
    assert!(
        !t.warnings.iter().any(|w| w.message.contains("treasure")),
        "every state kept: {:?}",
        t.warnings
    );
}

#[test]
fn crafting_work_rows_compile_to_map_recipes_totems_and_refinings() {
    use yalain_content::treasure_tables::Sort;
    let mut files = fixture();
    let rows = "[[map_recipes]]\nregion = \"linvak\"\ndrawn = 0x6F00170C\n\
                found = \"Eff_ResetLinvakResetTimers\"\nrecipes = [0x6F00109B, 0x6F001099]\n\
                wait = 300.0\nprov = { from = \"authored\" }\n\n\
                [[map_recipes]]\nregion = \"osteth\"\ndrawn = 0x6F00170E\n\
                found = \"Eff_ResetOstethResetTimers\"\nrecipes = [0x6F001091]\n\
                prov = { from = \"authored\" }\n\n\
                [[spellbinding]]\nsort = \"shield\"\ntemplate = 0x4700115B\nword = 0x3B284FFE\n\
                prov = { from = \"authored\" }\n\n\
                [[refining]]\nrecipe = \"SteelRefiningRecipe\"\nproduct = 0x47002224\nfull_at = 150\n\
                prov = { from = \"authored\" }\n\n\
                [[refining]]\nrecipe = \"BrickRefiningRecipe\"\nproduct = 0x47002224\n\
                prov = { from = \"authored\" }\n";
    files.insert("town/server/crafting2.toml".into(), rows.to_string());
    let merged = merge(&read(&files).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    let c = &t.crafts;
    let linvak = &c.map_recipes["linvak"];
    assert_eq!(
        (
            linvak.drawn,
            linvak.found.as_str(),
            linvak.recipes.as_slice(),
            linvak.wait
        ),
        (
            0x6F00_170C,
            "Eff_ResetLinvakResetTimers",
            [0x6F00_109B, 0x6F00_1099].as_slice(),
            Some(300.0)
        )
    );
    assert_eq!(c.map_recipes["osteth"].wait, None, "the server's rule");
    assert_eq!(
        c.map_recipes_drawn(0x6F00_170E).map(|m| m.region.as_str()),
        Some("osteth")
    );
    assert_eq!(c.totem(Sort::Shield).map(|t| t.template), Some(0x4700_115B));
    assert_eq!(c.totem_word(0x4700_115B), Some(0x3B28_4FFE));
    assert_eq!(c.refining["SteelRefiningRecipe"].full_at, Some(150));
    assert_eq!(c.refining["BrickRefiningRecipe"].full_at, None);
    assert_eq!(c.refining["BrickRefiningRecipe"].product, 0x4700_2224);
    // Templates the install lacks.
    for (from, said) in [
        (
            "template = 0x4700115B",
            "template: template 47009999 is not in the install",
        ),
        (
            "product = 0x47002224\nfull_at",
            "product: template 47009999 is not in the install",
        ),
    ] {
        let to = from.replace(
            from.split(" = ").nth(1).and_then(|v| v.get(..10)).unwrap(),
            "0x47009999",
        );
        files.insert(
            "town/server/crafting2.toml".into(),
            rows.replacen(from, &to, 1),
        );
        let merged = merge(&read(&files).unwrap()).unwrap();
        let problems = compile(&merged, &Fake::new()).unwrap_err();
        let messages: Vec<String> = problems.iter().map(ToString::to_string).collect();
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert!(messages[0].contains(said), "{messages:?}");
    }
    // A sort of weapon, armor or shield; a level from 2; a wait from 0; recipes, a found effect.
    for (from, to) in [
        ("sort = \"shield\"", "sort = \"jewelry\""),
        ("full_at = 150", "full_at = 1"),
        ("wait = 300.0", "wait = -1.0"),
        ("recipes = [0x6F001091]", "recipes = []"),
        ("found = \"Eff_ResetOstethResetTimers\"\n", ""),
        (
            "found = \"Eff_ResetOstethResetTimers\"",
            "found = \"not an id!\"",
        ),
        ("word = 0x3B284FFE\n", ""),
    ] {
        assert!(rows.contains(from), "{from}");
        files.insert(
            "town/server/crafting2.toml".into(),
            rows.replacen(from, to, 1),
        );
        assert!(read(&files).is_err(), "{to}");
    }
    // One row a region, a sort, a recipe.
    for twice in [
        "[[spellbinding]]\nsort = \"shield\"\ntemplate = 0x47002224\nword = 1\nprov = { from = \"authored\" }\n",
        "[[refining]]\nrecipe = \"SteelRefiningRecipe\"\nproduct = 0x4700115B\nprov = { from = \"authored\" }\n",
        "[[map_recipes]]\nregion = \"linvak\"\ndrawn = 1\nfound = \"Eff_X\"\nrecipes = [1]\nprov = { from = \"authored\" }\n",
    ] {
        files.insert(
            "town/server/crafting2.toml".into(),
            format!("{rows}\n{twice}"),
        );
        assert!(merge(&read(&files).unwrap()).is_err(), "{twice}");
    }
}

/// A world's creature rows: what a class makes of a concealing effect's holder, a behavior table's
/// healers, a friend table's first attacks and a template's shooting rhythm, each named for an
/// override by its kind and key; a row of no such shape is refused, and a rhythm's template is
/// held to the install.
#[test]
fn a_worlds_creature_rows_read_compile_and_are_held_to_their_shape() {
    use yalain_content::creature_rows::{Regard, Rhythm};
    const ROWS: &str = "[[concealing]]\neffect = 0x6F001C46\nclass = 0x17\nregard = \"spares\"\nprov = { from = \"authored\" }\n\n\
        [[concealing]]\neffect = 0x6F000D8C\nregard = \"hates\"\nprov = { from = \"authored\" }\n\n\
        [[npc_table]]\nname = \"NPCTable_Crone\"\nheals_kin = true\nprov = { from = \"authored\" }\n\n\
        [[friend_table]]\ntable = 0x0E\nattacks_first = false\nprov = { from = \"authored\" }\n\n\
        [[creature]]\ntemplate = 0x47002224\nshot_gap = 2.37\nwindup = 0.79\nprov = { from = \"authored\" }\n";
    let mut files = fixture();
    files.insert("town/server/creatures.toml".into(), ROWS.into());
    assert_eq!(check(&files), Vec::new());
    let db = read(&files).unwrap();
    let keys: Vec<String> = db
        .rows()
        .filter(|l| l.file.ends_with("creatures.toml"))
        .filter_map(yalain_world_db::merge::item_key)
        .collect();
    assert_eq!(
        keys,
        [
            "concealing 0x6f001c46 0x17",
            "concealing 0x6f000d8c any",
            "creature 0x47002224",
            "friend_table 0xe",
            "npc_table NPCTable_Crone",
        ]
    );
    for k in &keys {
        let words: Vec<&str> = k.split(' ').collect();
        let item = auberean_pack::Item::parse(&words).unwrap_or_else(|| panic!("{k} parses"));
        assert_eq!(&item.to_string(), k, "{k} reads back");
    }
    let t = compile(&merge(&db).unwrap(), &Fake::new()).unwrap();
    let c = &t.creatures;
    assert_eq!(
        c.concealing.get(&(0x6F00_1C46, Some(0x17))),
        Some(&Regard::Spares)
    );
    assert_eq!(c.concealing.get(&(0x6F00_0D8C, None)), Some(&Regard::Hates));
    assert!(c.heals_kin(Some("NPCTable_Crone")));
    assert!(!c.may_attack_first(Some(0x0E)) && c.may_attack_first(Some(0x42)));
    assert_eq!(
        c.rhythms.get(&0x4700_2224),
        Some(&Rhythm {
            gap: 2.37,
            windup: 0.79
        })
    );
    for (from, to) in [
        ("regard = \"spares\"", "regard = \"ignores\""),
        ("regard = \"hates\"\n", ""),
        ("heals_kin = true", "heals_kin = 1"),
        ("attacks_first = false\n", ""),
        ("shot_gap = 2.37\n", ""),
        ("windup = 0.79", "windup = 0"),
        ("windup = 0.79", "windup = 0.79\nspeed = 2"),
    ] {
        let mut files = fixture();
        files.insert(
            "town/server/creatures.toml".into(),
            ROWS.replacen(from, to, 1),
        );
        assert!(read(&files).is_err(), "{to:?} for {from:?}");
    }
    let mut files = fixture();
    files.insert(
        "town/server/creatures.toml".into(),
        ROWS.replace("0x47002224", "0x47009999"),
    );
    assert!(
        compile(&merge(&read(&files).unwrap()).unwrap(), &Fake::new()).is_err(),
        "a template the install lacks"
    );
}

#[test]
fn plant_rows_compile_to_species_and_crosses() {
    let mut files = fixture();
    let rows = "[[plant]]\nplant = 0x47002224\nseed = 0x4700115B\npollen = 0x47000846\n\
                name = 0x811B71D6\nharvest = 0x470020BB\nprov = { from = \"authored\" }\n\n\
                [[plant]]\nplant = 0x470020B9\nseed = 0x470033D0\nharvest = 0x47000001\n\
                prov = { from = \"authored\" }\n\n\
                [[plant_cross]]\nplant = 0x47002224\npollen = 0x47000846\nseeds = 0x470033D0\n\
                count = 2\nprov = { from = \"authored\" }\n";
    files.insert("town/server/plants.toml".into(), rows.to_string());
    let merged = merge(&read(&files).unwrap()).unwrap();
    let t = compile(&merged, &Fake::new()).unwrap();
    let p = &t.plants;
    assert_eq!(p.species.len(), 2);
    let one = p.sown_by(0x4700_115B).expect("a seed");
    assert_eq!(
        (one.plant, one.pollen, one.name, one.harvest),
        (0x4700_2224, Some(0x4700_0846), 0x811B_71D6, 0x4700_20BB)
    );
    let two = p.sown_by(0x4700_33D0).expect("a seed");
    assert_eq!((two.plant, two.pollen, two.name), (0x4700_20B9, None, 0));
    assert_eq!(p.pollen_of(0x4700_0846).map(|s| s.plant), Some(0x4700_2224));
    let x = p.cross(0x4700_2224, 0x4700_0846).expect("a cross");
    assert_eq!((x.seeds, x.count), (0x4700_33D0, 2));
    // Templates the install lacks.
    for (from, said) in [
        (
            "seed = 0x4700115B",
            "seed: template 47009999 is not in the install",
        ),
        (
            "pollen = 0x47000846\nname",
            "pollen: template 47009999 is not in the install",
        ),
        (
            "harvest = 0x470020BB",
            "harvest: template 47009999 is not in the install",
        ),
        (
            "seeds = 0x470033D0",
            "seeds: template 47009999 is not in the install",
        ),
    ] {
        let to = from.replace(
            from.split(" = ").nth(1).and_then(|v| v.get(..10)).unwrap(),
            "0x47009999",
        );
        files.insert(
            "town/server/plants.toml".into(),
            rows.replacen(from, &to, 1),
        );
        let merged = merge(&read(&files).unwrap()).unwrap();
        let problems = compile(&merged, &Fake::new()).unwrap_err();
        let messages: Vec<String> = problems.iter().map(ToString::to_string).collect();
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert!(messages[0].contains(said), "{messages:?}");
    }
    // A pollen with its name or neither; a count of 1 to 3; a plant, its seed and its plant item.
    for (from, to) in [
        ("name = 0x811B71D6\n", ""),
        ("pollen = 0x47000846\n", ""),
        ("count = 2", "count = 4"),
        ("count = 2", "count = 0"),
        ("harvest = 0x47000001\n", ""),
        ("seed = 0x470033D0\n", ""),
    ] {
        assert!(rows.contains(from), "{from}");
        files.insert("town/server/plants.toml".into(), rows.replacen(from, to, 1));
        assert!(read(&files).is_err(), "{from} to {to}");
    }
    // One row a plant, a plant and pollen.
    for twice in [
        "[[plant]]\nplant = 0x47002224\nseed = 0x47000001\nharvest = 0x47000001\nprov = { from = \"authored\" }\n",
        "[[plant_cross]]\nplant = 0x47002224\npollen = 0x47000846\nseeds = 0x4700115B\ncount = 1\nprov = { from = \"authored\" }\n",
    ] {
        files.insert(
            "town/server/plants.toml".into(),
            format!("{rows}\n{twice}"),
        );
        assert!(merge(&read(&files).unwrap()).is_err(), "{twice}");
    }
}
