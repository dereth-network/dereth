//! Vectors: empyrean/fixtures/vectors/content_json
//! ACE's import-json (json2sql) for weenie/recipe/landblock/quest documents yields ACE's SQL line
//! for line; BOM dropped; deep nesting fails at the default max depth; content-folder detection.
//! Fixture: checked-in ACE JSON vectors and the local case adapters.

use std::path::Path;

use empyrean_common::dotnet::DotNetDateTime;
use empyrean_common::vectors;
use empyrean_content::import::json::{json_to_sql, JsonKind};

/// The harness's stand-in for `DateTime.UtcNow`.
fn now() -> DotNetDateTime {
    DotNetDateTime::new_hms(2026, 1, 2, 3, 4, 5)
}

/// Drop `/* ... */` comments (they may span lines), collapse runs of spaces, trim each line, drop
/// empty lines and the space a removed label leaves before `,` or `;`.
fn normalize(sql: &str) -> Vec<String> {
    let mut out = String::new();
    let mut rest = sql;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        rest = rest[start..]
            .find("*/")
            .map_or("", |end| &rest[start + end + 2..]);
    }
    out.push_str(rest);
    // Split on LF only: a CR inside a value must survive (`str::lines` would eat it).
    out.split('\n')
        .map(|l| {
            let l = l
                .split(' ')
                .filter(|w| !w.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            // A removed label leaves a space before the `,` or `;` that followed it.
            l.replace(" ,", ",").replace(" ;", ";")
        })
        .filter(|l| !l.is_empty())
        .collect()
}

fn check_area(name: &str, kind: JsonKind) -> usize {
    let file = vectors::load_named("content_json", name);
    let mut failures = Vec::new();
    for case in &file.cases {
        let case_name = case.input["name"].as_str().unwrap();
        let json = case.input["json"].as_str().unwrap();
        let got = json_to_sql(kind, json, now());
        match (case.output.get("sql").and_then(|s| s.as_str()), &got) {
            (Some(want), Ok(got)) => {
                let (w, g) = (normalize(want), normalize(got));
                // V364 (a fix): a line may differ from ACE's only because ACE's null-field
                // replacements also ran inside string values
                let g_ace = normalize(&crate::support::sql_text::as_ace_wrote(got));
                // V364 (a fix): a create-list item without try_to_bond is not bonded, and a
                // page's author account is read; ACE bonded it and kept ''
                let g_ace: Vec<String> = g_ace
                    .into_iter()
                    .map(|l| match case_name {
                        "create_list_quirks" => l.replace("0.123457, False);", "0.123457, True);"),
                        "book_pages" => l.replace("'Me', 'acct',", "'Me', '',"),
                        _ => l,
                    })
                    .collect();
                let fixed = w.len() == g.len()
                    && g.len() == g_ace.len()
                    && (0..w.len()).all(|i| w[i] == g[i] || w[i] == g_ace[i]);
                if matches!(case_name, "create_list_quirks" | "book_pages") {
                    assert_ne!(w, g, "{case_name}: the fix changes this case");
                }
                if w != g && !fixed {
                    let first = w
                        .iter()
                        .zip(&g)
                        .position(|(a, b)| a != b)
                        .unwrap_or(w.len().min(g.len()));
                    failures.push(format!(
                        "{case_name}: line {first}\n  ACE:  {:?}\n  ours: {:?}",
                        w.get(first),
                        g.get(first)
                    ));
                }
            }
            (None, Err(_)) => {}
            (Some(_), Err(e)) => {
                failures.push(format!("{case_name}: ACE wrote SQL, ours failed: {e}"))
            }
            (None, Ok(_)) => failures.push(format!(
                "{case_name}: ACE failed ({}), ours wrote SQL",
                case.output["error"]
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        file.cases.len(),
        failures.join("\n")
    );
    file.cases.len()
}

#[test]
fn weenie_json_matches_ace() {
    assert!(check_area("weenie", JsonKind::Weenie) > 0);
}

#[test]
fn recipe_json_matches_ace() {
    assert!(check_area("recipe", JsonKind::Recipe) > 0);
}

#[test]
fn landblock_json_matches_ace() {
    assert!(check_area("landblock", JsonKind::Landblock) > 0);
}

#[test]
fn quest_json_matches_ace() {
    assert!(check_area("quest", JsonKind::Quest) > 0);
}

#[test]
fn byte_order_mark_is_dropped_as_read_all_text_does() {
    let sql = json_to_sql(
        JsonKind::Quest,
        "\u{feff}{\"key\": \"q\", \"value\": {}}",
        now(),
    )
    .unwrap();
    assert!(
        sql.contains("DELETE FROM `quest` WHERE `name` = 'q';"),
        "{sql}"
    );
}

#[test]
fn deep_nesting_fails_like_the_default_max_depth() {
    let deep = format!("{}{}", "[".repeat(70), "]".repeat(70));
    let doc = format!("{{\"wcid\": 1, \"x\": {deep}}}");
    assert!(json_to_sql(JsonKind::Weenie, &doc, now()).is_err());
    let ok = format!(
        "{{\"wcid\": 1, \"x\": {}{}}}",
        "[".repeat(60),
        "]".repeat(60)
    );
    assert!(json_to_sql(JsonKind::Weenie, &ok, now()).is_ok());
}

#[test]
fn detect_prefers_the_content_folder_then_the_keys() {
    let v = |s: &str| serde_json::from_str::<serde_json::Value>(s).unwrap();
    let quest = v(r#"{"key": "q", "value": {"fullname": "x"}}"#);
    assert_eq!(
        JsonKind::detect(Path::new("content/json/weenies/1 - x.json"), &quest),
        Some(JsonKind::Weenie)
    );
    assert_eq!(
        JsonKind::detect(Path::new("c/json/Landblocks/sub/A9B4.json"), &quest),
        Some(JsonKind::Landblock)
    );
    assert_eq!(
        JsonKind::detect(Path::new("x/recipes/quests/a.json"), &quest),
        Some(JsonKind::Quest)
    );
    let here = Path::new("patches/a.json");
    assert_eq!(
        JsonKind::detect(here, &v(r#"{"wcid": 1}"#)),
        Some(JsonKind::Weenie)
    );
    assert_eq!(
        JsonKind::detect(here, &v(r#"{"key": 1, "precursors": []}"#)),
        Some(JsonKind::Recipe)
    );
    assert_eq!(
        JsonKind::detect(here, &v(r#"{"key": 1, "value": {"weenies": []}}"#)),
        Some(JsonKind::Landblock)
    );
    assert_eq!(JsonKind::detect(here, &quest), Some(JsonKind::Quest));
    assert_eq!(
        JsonKind::detect(here, &v(r#"{"key": 1, "value": {}}"#)),
        None
    );
    assert_eq!(JsonKind::detect(here, &v("[]")), None);
}
