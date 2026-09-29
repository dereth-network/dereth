//! Vectors: empyrean/fixtures/vectors/content_export_json
//! Weenies, recipes, landblocks and quests read back from a pack export as ACE's JSON text;
//! metadata carried over; relaxed-encoder escapes; Utf8JsonWriter number text.
//! Fixture: tests/fixtures/patches/base.sql and locally constructed edge cases.

use std::path::PathBuf;

use empyrean_common::dotnet::{DotNetDateTime, DotNetDict};
use empyrean_common::vectors::{f32_of, f64_of, load_named};
use empyrean_content::export::json::{
    append_metadata, serialize, try_convert_ace_weenie_to_lsd_json, try_convert_cookbooks,
    try_convert_landblock, try_convert_quest, NEW_LINE,
};
use empyrean_content::import::build_from;
use empyrean_content::import::patch::{InputKind, Source};
use empyrean_content::pack::Pack;
use empyrean_content::PackContent;
use serde_json::Value;

const BASE: &str = include_str!("../../fixtures/patches/base.sql");

/// The harness's stand-in for `DateTime.UtcNow`.
fn now() -> DotNetDateTime {
    DotNetDateTime::new_hms(2026, 1, 2, 3, 4, 5)
}

fn pack_with(name: &str, sql: &str) -> PackContent {
    let sources: Vec<Source> = if sql.is_empty() {
        Vec::new()
    } else {
        vec![Source {
            kind: InputKind::Sql,
            path: PathBuf::from(format!("{name}.sql")),
            bytes: sql.as_bytes().to_vec(),
        }]
    };
    let (bytes, _) = build_from(BASE.as_bytes(), sources.into_iter().map(Ok), now())
        .unwrap_or_else(|e| panic!("{name}: the harness SQL does not apply: {e}"));
    PackContent::new(Pack::from_bytes(bytes).unwrap())
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} is not a string"))
}

fn u(v: &Value, key: &str) -> u64 {
    v[key]
        .as_u64()
        .unwrap_or_else(|| panic!("{key} is not an integer"))
}

/// ACE's text with its line breaks as this platform's `Environment.NewLine`. A JSON string never
/// holds a raw line break, so only the indentation's breaks change.
///
/// ACE's text then goes through Empyrean's brand rule: the export metadata names Empyrean where
/// ACE's names ACE.Adapter and ACEmulator (`empyrean_common::vectors::brand_ruled`).
fn ace(text: &str) -> String {
    empyrean_common::vectors::brand_ruled(&text.replace("\r\n", NEW_LINE))
}

#[track_caller]
fn same(name: &str, ours: &str, theirs: &str) {
    let theirs = ace(theirs);
    if ours != theirs {
        let line = ours
            .lines()
            .zip(theirs.lines())
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        panic!(
            "{name}: first difference at line {}:\n ours: {:?}\n ACE:  {:?}\n--- ours ---\n{ours}",
            line + 1,
            ours.lines().nth(line),
            theirs.lines().nth(line)
        );
    }
}

/// Our weenie JSON as ACE wrote it before V364's fix: without the pages'
/// `authorAccount`, which ACE's serializer skipped.
fn without_author_account(text: &str) -> String {
    let nl = NEW_LINE;
    text.split(nl)
        .filter(|l| !l.trim_start().starts_with("\"authorAccount\":"))
        .collect::<Vec<_>>()
        .join(nl)
}

/// Our recipe JSON as ACE wrote it before V364's fix: no `Message` on a
/// string requirement, and a bool requirement's left null.
fn without_string_and_bool_messages(
    mut recipe: empyrean_content::export::json::RecipeCombined,
) -> String {
    for r in recipe
        .recipe
        .iter_mut()
        .flat_map(|r| r.requirements.iter_mut())
        .flatten()
        .flatten()
    {
        for b in r.bool_requirements.iter_mut().flatten() {
            b.message = None;
        }
    }
    let text = serialize(&recipe).unwrap();
    let nl = NEW_LINE;
    let mut out = Vec::new();
    let mut in_strings: Option<usize> = None;
    for line in text.split(nl) {
        let indent = line.len() - line.trim_start().len();
        match in_strings {
            Some(i) if indent == i && line.trim_start().starts_with(']') => in_strings = None,
            Some(_) if line.trim_start().starts_with("\"Message\":") => continue,
            None if line.trim() == "\"StringRequirements\": [" => in_strings = Some(indent),
            _ => {}
        }
        out.push(line);
    }
    out.join(nl)
}

#[test]
fn weenies_export_as_ace_exports_them() {
    let set = load_named("content_export_json", "weenie");
    let mut partial_sets = 0;
    let mut ok = 0;
    for case in &set.cases {
        let name = s(&case.input, "name");
        let db = pack_with(name, s(&case.input, "sql"));
        let wcid = u32::try_from(u(&case.input, "wcid")).unwrap();
        let weenie = match db.get_weenie(wcid) {
            Some(w) => w,
            // The pack holds no class id 0; ACE's converter refuses it anyway.
            None if wcid == 0 => empyrean_content::models::world::Weenie {
                r#type: 1,
                ..Default::default()
            },
            None => panic!("{name}: weenie {wcid} not in the pack"),
        };
        let ours = try_convert_ace_weenie_to_lsd_json(&weenie, now());
        match case.output.get("json") {
            Some(json) => {
                let (text, _) = ours.unwrap_or_else(|| panic!("{name}: ACE exports it, we do not"));
                // V364 (a fix): the pages' author account is written
                same(name, &without_author_account(&text), json.as_str().unwrap());
                ok += 1;
            }
            None => {
                // V364 (a fix): a weenie with some but not all attributes (or vitals) is
                // exported with the ones it has; ACE failed on it
                let partial = |n: usize, all: usize| n > 0 && n < all;
                if partial(weenie.weenie_properties_attribute.len(), 6)
                    || partial(weenie.weenie_properties_attribute_2nd.len(), 3)
                {
                    let (text, _) =
                        ours.unwrap_or_else(|| panic!("{name}: a partial set is exported"));
                    assert!(text.contains("\"attributes\""), "{name}");
                    partial_sets += 1;
                } else {
                    assert!(
                        ours.is_none(),
                        "{name}: ACE fails ({}), we export it",
                        case.output["error"]
                    );
                }
            }
        }
    }
    assert!(
        partial_sets > 0,
        "the vectors hold a weenie with a partial set"
    );
    let expected = set
        .cases
        .iter()
        .filter(|case| case.output.get("json").is_some())
        .count();
    assert_eq!(ok, expected, "every successful recorded export is compared");
}

#[test]
fn append_metadata_carries_an_existing_file_over() {
    let set = load_named("content_export_json", "append_metadata");
    for case in &set.cases {
        let name = s(&case.input, "name");
        let db = pack_with(name, s(&case.input, "sql"));
        let wcid = u32::try_from(u(&case.input, "wcid")).unwrap();
        let weenie = db.get_weenie(wcid).unwrap();
        let (json, mut lsd) = try_convert_ace_weenie_to_lsd_json(&weenie, now()).unwrap();
        let appended = append_metadata(s(&case.input, "existing"), &mut lsd, now());
        if case.output.get("throws").is_some() {
            assert!(
                appended.is_err(),
                "{name}: ACE throws, we return {appended:?}"
            );
            continue;
        }
        let appended = appended.unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            appended,
            case.output["appended"].as_bool().unwrap(),
            "{name}"
        );
        // ExportJsonWeenie serializes again only when AppendMetadata returned true.
        let text = if appended {
            serialize(&lsd).unwrap()
        } else {
            json
        };
        same(name, &text, s(&case.output, "json"));
    }
    assert!(!set.cases.is_empty(), "the recorded cases are present");
}

#[test]
fn recipes_export_as_ace_exports_them() {
    let set = load_named("content_export_json", "recipe");
    for case in &set.cases {
        let name = s(&case.input, "name");
        let db = pack_with(name, s(&case.input, "sql"));
        let recipe_id = u32::try_from(u(&case.input, "recipe_id")).unwrap();
        let cookbooks = db.get_cookbooks_by_recipe_id(recipe_id);
        assert!(!cookbooks.is_empty(), "{name}");
        let ours = try_convert_cookbooks(&cookbooks);
        match case.output.get("json") {
            Some(json) => {
                let ours = ours.expect(name);
                if name == "full" {
                    // V364 (a fix): the string requirement's message is exported
                    assert!(
                        serialize(&ours)
                            .unwrap()
                            .contains("\"Message\": \"lost message\""),
                        "{name}"
                    );
                }
                same(
                    name,
                    &without_string_and_bool_messages(ours),
                    json.as_str().unwrap(),
                );
            }
            None => assert!(ours.is_none(), "{name}: ACE fails, we convert it"),
        }
    }
    assert!(!set.cases.is_empty(), "the recorded cases are present");
}

fn names(v: &Value) -> Option<DotNetDict<u32, String>> {
    let pairs = v.as_array()?;
    let mut d = DotNetDict::new();
    for p in pairs {
        d.add(
            u32::try_from(p[0].as_u64().unwrap()).unwrap(),
            p[1].as_str().unwrap().to_owned(),
        );
    }
    Some(d)
}

#[test]
fn landblocks_export_as_ace_exports_them() {
    let set = load_named("content_export_json", "landblock");
    for case in &set.cases {
        let name = s(&case.input, "name");
        let db = pack_with(name, s(&case.input, "sql"));
        let landblock = u16::try_from(u(&case.input, "landblock")).unwrap();
        let instances = db.get_cached_instances_by_landblock(landblock);
        let (n, c) = (
            names(&case.input["weenie_names"]),
            names(&case.input["weenie_class_names"]),
        );
        let ours = try_convert_landblock(&instances, n.as_ref(), c.as_ref()).expect(name);
        same(name, &serialize(&ours).unwrap(), s(&case.output, "json"));
    }
    assert!(!set.cases.is_empty(), "the recorded cases are present");
}

#[test]
fn quests_export_as_ace_exports_them() {
    let set = load_named("content_export_json", "quest");
    for case in &set.cases {
        let name = s(&case.input, "name");
        let db = pack_with(name, s(&case.input, "sql"));
        let quest = db.get_cached_quest(s(&case.input, "quest")).expect(name);
        let ours = try_convert_quest(&quest).unwrap();
        same(name, &serialize(&ours).unwrap(), s(&case.output, "json"));
    }
    assert!(!set.cases.is_empty(), "the recorded cases are present");
}

#[test]
fn strings_are_escaped_as_the_relaxed_encoder_escapes_them() {
    let set = load_named("content_export_json", "encoder");
    let mut ranges = None;
    for case in &set.cases {
        if case.input.get("bmp_escaped").is_some() {
            ranges = Some(case.output["ranges"].as_array().unwrap().clone());
            continue;
        }
        let text = s(&case.input, "s").to_owned();
        assert_eq!(
            serialize(&text).unwrap(),
            s(&case.output, "json"),
            "{text:?}"
        );
    }
    // Every BMP scalar value: escaped exactly where ACE's runtime escapes it.
    let ranges: Vec<(u32, u32)> = ranges
        .unwrap()
        .iter()
        .map(|r| {
            (
                u32::try_from(r[0].as_u64().unwrap()).unwrap(),
                u32::try_from(r[1].as_u64().unwrap()).unwrap(),
            )
        })
        .collect();
    let mut checked = 0;
    for cp in (0..=0xFFFFu32).filter(|cp| !(0xD800..=0xDFFF).contains(cp)) {
        let c = char::from_u32(cp).unwrap();
        let text = serialize(&c.to_string()).unwrap();
        let escaped = text != format!("\"{c}\"");
        let expected = ranges.iter().any(|&(lo, hi)| (lo..=hi).contains(&cp));
        assert_eq!(escaped, expected, "U+{cp:04X}: {text}");
        if escaped && cp >= 0x80 {
            assert_eq!(text, format!("\"\\u{cp:04X}\""));
        }
        checked += 1;
    }
    assert_eq!(checked, 0x10000 - 0x800);
}

#[test]
fn numbers_are_written_as_utf8_json_writer_writes_them() {
    let set = load_named("content_export_json", "numbers");
    for case in &set.cases {
        let ours = if let Some(v) = case.input.get("f64") {
            serialize(&f64_of(v).unwrap())
        } else {
            serialize(&f32_of(&case.input["f32"]).unwrap())
        };
        match case.output.get("json") {
            Some(json) => assert_eq!(
                ours.as_deref(),
                Ok(json.as_str().unwrap()),
                "{}",
                case.input
            ),
            None => assert!(ours.is_err(), "{}: ACE throws", case.input),
        }
    }
    assert!(!set.cases.is_empty(), "the recorded cases are present");
}
