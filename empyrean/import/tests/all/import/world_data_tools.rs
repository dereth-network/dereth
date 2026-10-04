//! Divergence: V313, V337, V388
//! Empyrean-import --corrections lists applying/stale/absent entries and exits 1 on stale;
//! --check --fields lists field changes; --check --overlap lists upstream changes our content
//! touches; library and binary.
//! Fixture: temporary content files and the built importer.

use std::path::{Path, PathBuf};
use std::process::Command;

use empyrean_content::corrections::report::{CorrectionsReport, EntryState};
use empyrean_content::corrections::{self, SPELL_CORRECTIONS, WEENIE_CORRECTIONS};
use empyrean_content::import::check::{self, fields, overlap};
use empyrean_content::import::default_now;
use empyrean_content::import::patch::{Input, InputKind};
use empyrean_content::models::world::{Spell, Weenie, WeeniePropertiesCreateList};
use empyrean_content::pack::{Pack, TableId};
use empyrean_content::MemContent;
use empyrean_entity::enums::{
    PropertyDataId, PropertyFloat, PropertyInt, PropertyString, WeenieType,
};

/// The ring pieces' stored state, and the value retail flew them with.
const AFTER_IMPACT: i32 = 0x0012_0034;

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("empyrean-content-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn pack_of(m: &MemContent) -> Pack {
    Pack::from_bytes(m.content().to_pack([0; 16]).unwrap().0).unwrap()
}

fn write_pack(m: &MemContent, path: &Path) {
    std::fs::write(path, m.content().to_pack([0; 16]).unwrap().0).unwrap();
}

fn import_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_empyrean-import"))
}

/// Two entries that apply (a weenie and a spell), two stale ones, one weenie the default-script
/// rule changes and one emote motion the motion rule changes; every other entry is absent.
fn corrections_content() -> MemContent {
    use empyrean_content::models::world::{WeeniePropertiesEmote, WeeniePropertiesEmoteAction};

    let mut emoting = Weenie::new(25682, "guarddeepplaces", WeenieType::Creature);
    emoting.weenie_properties_emote.push(WeeniePropertiesEmote {
        id: 49419,
        object_id: 25682,
        category: 5,
        probability: 1.0,
        weenie_properties_emote_action: vec![WeeniePropertiesEmoteAction {
            emote_id: 49419,
            order: 0,
            r#type: 5,
            motion: Some(0x1300_0116),
            ..WeeniePropertiesEmoteAction::default()
        }],
        ..WeeniePropertiesEmote::default()
    });
    MemContent::new()
        .weenie(
            Weenie::new(33862, "flamewave", WeenieType::ProjectileSpell)
                .with_int(PropertyInt::PhysicsState, AFTER_IMPACT),
        )
        .weenie(
            Weenie::new(33845, "acidbomb", WeenieType::ProjectileSpell)
                .with_int(PropertyInt::PhysicsState, 0x408),
        )
        .weenie(
            Weenie::new(3768, "flamingclub", WeenieType::MeleeWeapon)
                .with_string(PropertyString::Name, "Flaming Club")
                .with_did(PropertyDataId::PhysicsEffectTable, 0x3400_0039)
                .with_did(PropertyDataId::PhysicsScript, 83),
        )
        .weenie(emoting)
        .spell(Spell {
            id: 5357,
            wcid: Some(43231),
            ..Spell::default()
        })
        .spell(Spell {
            id: 5358,
            wcid: Some(1),
            ..Spell::default()
        })
}

#[test]
fn the_corrections_report_lists_applying_stale_and_absent_entries_and_rule_changes() {
    let m = corrections_content();
    let r = CorrectionsReport::of(m.db().base());
    assert_eq!(r.digest, corrections::digest());
    let s = r.summary();
    let entries = WEENIE_CORRECTIONS.len() + SPELL_CORRECTIONS.len();
    assert_eq!(
        (s.entries, s.applies, s.stale, s.absent),
        (entries, 2, 2, entries - 4)
    );
    assert_eq!((s.play_script_shifts, s.emote_motion_shifts), (1, 1));

    let state = |wcid: u32| {
        r.weenie_entries
            .iter()
            .find(|e| e.correction.weenie_class_id == wcid)
            .map(|e| e.state.clone())
    };
    assert_eq!(state(33862), Some(EntryState::Applies));
    assert_eq!(
        state(33845),
        Some(EntryState::Stale("1032 (0x408)".to_owned()))
    );
    assert_eq!(state(33866), Some(EntryState::Absent));
    let spell = |id: u32| {
        r.spell_entries
            .iter()
            .find(|e| e.correction.spell_id == id)
            .map(|e| e.state.clone())
    };
    assert_eq!(spell(5357), Some(EntryState::Applies));
    assert_eq!(spell(5358), Some(EntryState::Stale("wcid 1".to_owned())));

    let text = r.render();
    assert!(
        text.starts_with(&format!(
            "corrections {} (empyrean corrections v2)\n",
            corrections::digest()
        )),
        "{text}"
    );
    assert!(text.contains(&format!("entries: 2 apply, 2 stale, {} absent (of {entries}); rules: 1 default scripts, 1 emote motions\n", entries - 4)), "{text}");
    assert!(text.contains("  applies 33862 flamewave: PropertyInt.PhysicsState 1179700 (0x120034) -> 133960 (0x20B48) [V313; "), "{text}");
    assert!(text.contains("  stale   33845 acidbomb: PropertyInt.PhysicsState 1179700 (0x120034) -> 133960 (0x20B48) [V313; "), "{text}");
    assert!(text.contains("          stores 1032 (0x408)\n"), "{text}");
    assert!(text.contains("  3768 flamingclub (Flaming Club): PhysicsScript 83 (PortalExit) -> 84 (BreatheFlame)\n"), "{text}");
    assert!(text.contains("  25682 guarddeepplaces: emote set 49419 action 0 motion 0x13000116 -> 0x13000119 (WarmHands)\n"), "{text}");
    assert!(
        text.contains(&format!("\n{} entries do not apply:\n", entries - 2)),
        "{text}"
    );
    assert_eq!(
        text,
        CorrectionsReport::of(m.db().base()).render(),
        "the same content reports the same way"
    );

    let json = r.to_json();
    assert_eq!(json["digest"], corrections::digest());
    assert_eq!(json["summary"]["stale"], 2);
    let acid = json["weenie_entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["weenie_class_id"] == 33845)
        .unwrap();
    assert_eq!(
        (
            acid["state"].as_str(),
            acid["found"].as_str(),
            acid["divergence"].as_str()
        ),
        (Some("stale"), Some("1032 (0x408)"), Some("V313"))
    );
    assert_eq!(json["rules"][0]["changes"][0]["weenie_class_id"], 3768);
    assert_eq!(json["rules"][1]["changes"][0]["corrected"], 0x1300_0119);
}

#[test]
fn empyrean_import_corrections_reports_and_exits_1_on_a_stale_entry() {
    let dir = temp_dir("corrections-cli");
    let pack = dir.join("world.pack");
    let report = dir.join("corrections.json");
    write_pack(&corrections_content(), &pack);
    let out = import_bin()
        .arg("--corrections")
        .arg(&pack)
        .arg("--report")
        .arg(&report)
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(
        text.starts_with(&format!("world.pack {}: content hash ", pack.display())),
        "{text}"
    );
    assert!(
        text.contains(&format!("corrections {}", corrections::digest())),
        "{text}"
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report).unwrap()).unwrap();
    assert_eq!(json["format"], "empyrean corrections report v1");
    assert_eq!(json["summary"]["applies"], 2);

    // Usage errors exit 2.
    let out = import_bin()
        .arg("--corrections")
        .arg(&pack)
        .arg("--fields")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Divergence: V388
#[test]
fn empyrean_import_corrections_exits_0_on_content_built_for_another_era() {
    let dir = temp_dir("corrections-era");
    let pack = dir.join("world.pack");
    write_pack(
        &corrections_content().era(empyrean_common::era::EraId::Infiltration),
        &pack,
    );
    let out = import_bin()
        .arg("--corrections")
        .arg(&pack)
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "{text}");
    let entries = WEENIE_CORRECTIONS.len() + SPELL_CORRECTIONS.len();
    assert!(
        text.contains(&format!(
            "the content is for era infiltration: the {entries} entries are for other eras and none applies\n"
        )),
        "{text}"
    );
    assert!(
        text.contains("rules: 1 default scripts, 1 emote motions"),
        "the rules still apply: {text}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

fn create_list(
    object_id: u32,
    weenie_class_id: u32,
    stack_size: i32,
) -> WeeniePropertiesCreateList {
    WeeniePropertiesCreateList {
        id: 0,
        object_id,
        destination_type: 2,
        weenie_class_id,
        stack_size,
        palette: 0,
        shade: 0.0,
        try_to_bond: false,
    }
}

/// Upstream's old and new world: 33862 (a corrected weenie) gains a property, changes its state
/// and swaps a create-list row; 500 changes a float; spell 5357 (a corrected spell) is renamed;
/// 600 is new; 700 is gone; 800 is unchanged.
fn upstream() -> (MemContent, MemContent) {
    let mut wave = Weenie::new(33862, "flamewave", WeenieType::ProjectileSpell)
        .with_int(PropertyInt::PhysicsState, AFTER_IMPACT);
    wave.weenie_properties_create_list = vec![create_list(33862, 1, 1), create_list(33862, 2, 1)];
    let mut wave2 = Weenie::new(33862, "flamewave", WeenieType::ProjectileSpell)
        .with_int(PropertyInt::PhysicsState, 0x408)
        .with_int(PropertyInt::Value, 5);
    // Row ids a dump renumbers are not a difference.
    wave2.weenie_properties_create_list = vec![create_list(33862, 2, 1), create_list(33862, 3, 1)];
    wave2.weenie_properties_create_list[0].id = 99;
    let plain = |id: u32| Weenie::new(id, "thing", WeenieType::Generic);
    let old = MemContent::new()
        .weenie(wave)
        .weenie(plain(500).with_float(PropertyFloat::DefaultScale, 1.0))
        .weenie(plain(700))
        .weenie(plain(800))
        .spell(Spell {
            id: 5357,
            name: "Nether Streak I".to_owned(),
            wcid: Some(43231),
            ..Spell::default()
        });
    let new = MemContent::new()
        .weenie(wave2)
        .weenie(plain(500).with_float(PropertyFloat::DefaultScale, 1.5))
        .weenie(plain(600))
        .weenie(plain(800))
        .spell(Spell {
            id: 5357,
            name: "Nether Streak One".to_owned(),
            wcid: Some(43231),
            ..Spell::default()
        });
    (old, new)
}

#[test]
fn check_fields_lists_what_changed_inside_each_record() {
    let (old, new) = upstream();
    let (old, new) = (pack_of(&old), pack_of(&new));
    let diffs = check::diff(&old, &new).unwrap();
    let f = fields::changed_fields(&diffs, &old, &new).unwrap();
    let keys: Vec<(&str, u64)> = f.iter().map(|r| (r.table, r.key)).collect();
    assert_eq!(
        keys,
        [("weenie", 500), ("weenie", 33862), ("spell", 5357)],
        "weenie_index is left out"
    );
    let change = |field: &str, old: Option<&str>, new: Option<&str>| fields::FieldChange {
        field: field.to_owned(),
        old: old.map(str::to_owned),
        new: new.map(str::to_owned),
    };
    assert_eq!(
        f[0].changes,
        [change("PropertyFloat.DefaultScale", Some("1"), Some("1.5"))]
    );
    let row = |wcid: u32| {
        format!("destination_type=2, weenie_class_id={wcid}, stack_size=1, palette=0, shade=0.0, try_to_bond=false")
    };
    assert_eq!(
        f[1].changes,
        [
            change("PropertyInt.PhysicsState", Some("1179700"), Some("1032")),
            change("PropertyInt.Value", None, Some("5")),
            change("CreateList", Some(&row(1)), None),
            change("CreateList", None, Some(&row(3))),
        ]
    );
    assert_eq!(
        f[2].changes,
        [change(
            "name",
            Some("\"Nether Streak I\""),
            Some("\"Nether Streak One\"")
        )]
    );

    let text = check::render_with(&diffs, Some(&f), None, &old, &new);
    assert!(
        text.contains("  ~ weenie 33862 flamewave ()\n      PropertyInt.PhysicsState: 1179700 -> 1032\n      PropertyInt.Value: (none) -> 5\n"),
        "{text}"
    );
    assert!(
        text.contains("      CreateList: (none) -> destination_type=2, weenie_class_id=3,"),
        "{text}"
    );
    assert!(
        text.contains("  ~ spell 5357\n      name: \"Nether Streak I\" -> \"Nether Streak One\"\n"),
        "{text}"
    );
    assert!(!text.contains("overlap"), "{text}");
    let json = check::to_json_with(&diffs, Some(&f), None);
    assert_eq!(
        json["fields"][2]["changes"][0]["new"],
        "\"Nether Streak One\""
    );
    assert!(json.get("overlap").is_none());
    // Without --fields the report is what it was.
    assert_eq!(
        check::render(&diffs, &old, &new),
        check::render_with(&diffs, None, None, &old, &new)
    );
    assert_eq!(
        check::to_json(&diffs),
        check::to_json_with(&diffs, None, None)
    );
}

#[test]
fn check_overlap_lists_upstream_changes_our_content_also_touches() {
    let (old, new) = upstream();
    let (old, new) = (pack_of(&old), pack_of(&new));
    let diffs = check::diff(&old, &new).unwrap();
    let mut ours = overlap::ours_from_corrections(&diffs, &old, &new).unwrap();
    ours.push(((TableId::WEENIE, 500), "patch ours/00500.sql".to_owned()));
    ours.push(((TableId::WEENIE, 700), "overlay dev.sqlite".to_owned()));
    ours.push(((TableId::WEENIE, 800), "patch ours/00800.sql".to_owned()));
    let o = overlap::overlap(&diffs, &ours);
    let got: Vec<(&str, u64, char, Vec<String>)> = o
        .iter()
        .map(|r| (r.table, r.key, r.mark, r.ours.clone()))
        .collect();
    let v = |s: &[&str]| s.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        got,
        [
            ("weenie", 500, '~', v(&["patch ours/00500.sql"])),
            ("weenie", 700, '-', v(&["overlay dev.sqlite"])),
            (
                "weenie",
                33862,
                '~',
                v(&["correction V313 PropertyInt.PhysicsState"])
            ),
            ("spell", 5357, '~', v(&["correction V313 spell wcid"])),
        ],
        "800 is unchanged upstream, 600 is nobody's"
    );
    let text = check::render_with(&diffs, None, Some(&o), &old, &new);
    assert!(text.contains("\noverlap: 4 changed records that ours also touches\n  ~ weenie 500 thing ()\n      patch ours/00500.sql\n"), "{text}");
    let json = check::to_json_with(&diffs, None, Some(&o));
    assert_eq!(json["overlap"][3]["upstream"], "~");
    assert_eq!(json["overlap"][3]["ours"][0], "correction V313 spell wcid");

    // A rule counts as ours on a weenie it changes in either pack.
    let script = |v: u32| {
        Weenie::new(3768, "flamingclub", WeenieType::MeleeWeapon)
            .with_did(PropertyDataId::PhysicsScript, v)
    };
    let (a, b) = (
        pack_of(&MemContent::new().weenie(script(83))),
        pack_of(&MemContent::new().weenie(script(84))),
    );
    let d = check::diff(&a, &b).unwrap();
    let o = overlap::overlap(&d, &overlap::ours_from_corrections(&d, &a, &b).unwrap());
    assert_eq!(o.len(), 1);
    assert_eq!(o[0].ours, ["rule play_script_shift V337"]);
    let none = overlap::overlap(&d, &Vec::new());
    assert!(none.is_empty());
    assert!(check::render_with(&d, None, Some(&none), &a, &b)
        .contains("\noverlap: none of ours touches a record that changed\n"));
}

#[test]
fn overlap_content_files_reach_what_they_replace_over_the_dump() {
    let patches = crate::patch_fixtures();
    let inputs = [Input {
        kind: InputKind::Sql,
        path: patches.join("sql/1 weenies"),
    }];
    let ours =
        overlap::ours_from_patches(&patches.join("base.sql"), &inputs, default_now()).unwrap();
    let mut keys: Vec<(u16, u64)> = ours.iter().map(|((t, k), _)| (t.0, *k)).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys, [(TableId::WEENIE.0, 100), (TableId::WEENIE.0, 90001)]);
    assert!(
        ours.iter()
            .all(|(_, what)| what.starts_with("patch ") && what.ends_with(".sql")),
        "{ours:?}"
    );
    assert!(overlap::ours_from_overlay(&patches.join("no-such.sqlite")).is_err());
}

#[test]
fn empyrean_import_check_fields_and_overlap_end_to_end() {
    let dir = temp_dir("check-cli");
    let (old, new) = upstream();
    let (a, b, report) = (
        dir.join("old.pack"),
        dir.join("new.pack"),
        dir.join("diff.json"),
    );
    write_pack(&old, &a);
    write_pack(&new, &b);
    let out = import_bin()
        .arg("--check")
        .arg(&a)
        .arg(&b)
        .args(["--fields", "--overlap", "--report"])
        .arg(&report)
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(
        text.contains("      PropertyFloat.DefaultScale: 1 -> 1.5\n"),
        "{text}"
    );
    assert!(
        text.contains("\noverlap: 2 changed records that ours also touches\n"),
        "{text}"
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report).unwrap()).unwrap();
    assert_eq!(json["format"], "world.pack check v1");
    assert_eq!(json["overlap"].as_array().map(Vec::len), Some(2));
    assert_eq!(json["fields"].as_array().map(Vec::len), Some(3));

    // Content files to overlap need a dump to go over; --fields needs --check.
    let out = import_bin()
        .arg("--check")
        .arg(&a)
        .arg(&b)
        .args(["--overlap", "ours"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let out = import_bin().arg("--fields").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    std::fs::remove_dir_all(&dir).unwrap();
}
