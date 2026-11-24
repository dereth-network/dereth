//! Vectors: support/world_dump_fixture.rs, synthetic rows for every ACE world table
//! The rows' vocabulary `empyrean-import schema` gives an editor is the one world-db writes: every
//! file of a world's rows is where its kind's file rule puts it, and every field, map key, map
//! value and list entry of every row is described, of the type it is written as; a name is one the
//! schema's own tables give; and each row's item, made from its kind's pattern, is the one the
//! world's rows name, and the same item written another way reads back to it as theirs does.
//! Fixture: the synthetic dump's world written as rows, with a recipe's vital requirement and a
//! rules row added.

use std::collections::BTreeSet;

use empyrean_common::world_rules::{Allowed, WorldRules};
use empyrean_content::pack::Pack;
use serde_json::Value as J;
use toml::Value as T;

use crate::fixture;

/// Whether `v`, a row's value, is of type `ty` as the schema says; `why` says where not.
fn is(schema: &J, ty: &J, v: &T, at: &str, why: &mut Vec<String>) {
    let ok = match (ty, v) {
        (J::String(s), _) => matches!(
            (s.as_str(), v),
            ("int" | "hex", T::Integer(_))
                | ("float", T::Float(_) | T::Integer(_))
                | ("bool", T::Boolean(_))
                | ("text" | "time", T::String(_))
        ),
        (J::Object(o), _) if o.contains_key("name") => {
            let e = o["name"].as_str().unwrap_or_default();
            let names = &schema["enums"][e];
            assert!(names.is_object(), "{at}: no enum {e} in the schema");
            v.as_str()
                .is_some_and(|n| names.as_object().unwrap().values().any(|x| x == n))
        }
        (J::Object(o), T::Array(items)) if o.contains_key("list") => {
            let len_ok = o
                .get("len")
                .and_then(J::as_u64)
                .is_none_or(|n| n == items.len() as u64);
            for (i, x) in items.iter().enumerate() {
                is(schema, &o["list"], x, &format!("{at}[{i}]"), why);
            }
            len_ok
        }
        (J::Object(o), T::Table(t)) if o.contains_key("fields") => {
            fields(schema, &o["fields"], t, at, why);
            true
        }
        (J::Object(o), _) if o.contains_key("either") => {
            let mut any = false;
            for alt in o["either"].as_array().unwrap() {
                let mut w = Vec::new();
                is(schema, alt, v, at, &mut w);
                any |= w.is_empty();
            }
            any
        }
        _ => false,
    };
    if !ok {
        why.push(format!("{at}: {v} is not {ty}"));
    }
}

/// Every field of `t` is one `described` names, of its type.
fn fields(schema: &J, described: &J, t: &toml::Table, at: &str, why: &mut Vec<String>) {
    for (k, v) in t {
        match described.get(k) {
            Some(ty) => is(schema, ty, v, &format!("{at}.{k}"), why),
            None => why.push(format!("{at}.{k}: not described")),
        }
    }
}

/// `row`, of a kind `kind` describes: its fields, maps, tables and lists.
fn row(schema: &J, kind: &J, r: &toml::Table, at: &str, why: &mut Vec<String>) {
    let empty = J::Object(serde_json::Map::new());
    let maps = kind.get("maps").unwrap_or(&empty);
    let tables = kind.get("tables").unwrap_or(&empty);
    let lists = kind.get("lists").unwrap_or(&empty);
    for (k, v) in r {
        let here = format!("{at}.{k}");
        if let Some(m) = maps.get(k) {
            let Some(entries) = v.as_table() else {
                why.push(format!("{here}: a map isn't a table"));
                continue;
            };
            for (key, value) in entries {
                let key_ok = match &m["key"] {
                    J::String(s) if s == "int" => key.parse::<i64>().is_ok(),
                    J::String(s) if s == "text" => true,
                    J::Object(o) if o.contains_key("property") => schema["properties"]
                        [o["property"].as_str().unwrap()]
                    .as_object()
                    .is_some_and(|names| names.values().any(|n| n == key)),
                    J::Object(o) if o.contains_key("name") => schema["enums"]
                        [o["name"].as_str().unwrap()]
                    .as_object()
                    .is_some_and(|names| names.values().any(|n| n == key)),
                    other => panic!("{here}: a map key of {other}"),
                };
                if !key_ok {
                    why.push(format!("{here}: key {key} is not {}", m["key"]));
                }
                is(schema, &m["value"], value, &format!("{here}.{key}"), why);
            }
        } else if let Some(described) = tables.get(k) {
            match v.as_table() {
                Some(t) => fields(schema, described, t, &here, why),
                None => why.push(format!("{here}: not a table")),
            }
        } else if let Some(entry) = lists.get(k) {
            match v.as_array() {
                Some(items) => {
                    for (i, x) in items.iter().enumerate() {
                        match x.as_table() {
                            Some(t) => row(schema, entry, t, &format!("{here}[{i}]"), why),
                            None => why.push(format!("{here}[{i}]: not a table")),
                        }
                    }
                }
                None => why.push(format!("{here}: not a list")),
            }
        } else if let Some(ty) = kind["fields"].get(k) {
            is(schema, ty, v, &here, why);
        } else if k != "prov" {
            why.push(format!("{here}: not described"));
        }
    }
}

/// Every file of the fixture's world, written as rows, is described by the schema: where it is,
/// what each row holds, and the type of each value; every kind but the ac1999 world's source loot
/// inputs is among them.
#[test]
fn every_row_a_world_writes_is_described_by_the_schema_an_editor_reads() {
    let schema = empyrean_import_schema();
    let pack = Pack::from_bytes(fixture::imported().0.clone()).expect("the fixture's pack");
    let mut c = empyrean_world_db::from_pack(&pack).expect("its world");
    // The one optional table the dump leaves out: a recipe's vital requirement.
    c.recipe_vital_requirements
        .push(empyrean_content::models::world::RecipeVitalRequirement {
            id: c.recipes[0].id,
            health: 10,
            stamina: 5,
            mana: 0,
        });
    c.rules = Some(WorldRules {
        max_level: Some(50),
        heritages: Some(Allowed::Only(vec![1, 2])),
        features: [("housing".to_owned(), false)].into(),
        ..WorldRules::default()
    });
    let files =
        empyrean_world_db::write(&c, empyrean_world_db::files::default_stamp()).expect("its rows");
    let kinds = schema["rows"].as_object().expect("rows");
    let mut seen = BTreeSet::new();
    let mut why = Vec::new();
    for (path, text) in &files {
        if path.ends_with("pack.toml") || path.ends_with("world-db.toml") {
            continue;
        }
        let doc: toml::Table = toml::from_str(text).unwrap_or_else(|e| panic!("{path}: {e}"));
        for (kind, rows) in &doc {
            let Some(k) = kinds.get(kind) else {
                why.push(format!("{path}: no kind {kind}"));
                continue;
            };
            seen.insert(kind.clone());
            let file = k["file"].as_str().unwrap();
            let folder = file.split('{').next().unwrap();
            if !(path == file || (folder.ends_with('/') && path.starts_with(folder))) {
                why.push(format!("{path}: {kind} lives in {file}"));
            }
            // Each row's item, from the schema's pattern, is the one the world's own rows name.
            for r in rows.as_array().expect("a list of rows") {
                let r = r.as_table().expect("a row");
                let want = dereth_pack::records::RowKinds::item(
                    &empyrean_world_db::layers::Ac1Rows,
                    kind,
                    r,
                )
                .expect("an item")
                .name;
                match dereth_pack::records::item_from(k["item"].as_str().unwrap(), r) {
                    Ok(got) if got == want => {}
                    got => why.push(format!("{path} {kind}: item {got:?}, not {want:?}")),
                }
                // Written another way, it reads back to the same item as the world's rows read it.
                for other in [want.to_lowercase(), want.replace(' ', "  ")] {
                    let ours =
                        dereth_pack::records::item_in_form(k["item"].as_str().unwrap(), &other);
                    let theirs = empyrean_world_db::layers::item_form(&other);
                    if ours != theirs {
                        why.push(format!(
                            "{path} {kind}: {other:?} reads {ours:?}, not {theirs:?}"
                        ));
                    }
                }
            }
            if k.get("open").is_some() {
                continue;
            }
            for (i, r) in rows.as_array().expect("a list of rows").iter().enumerate() {
                row(
                    &schema,
                    k,
                    r.as_table().expect("a row"),
                    &format!("{path} {kind}[{i}]"),
                    &mut why,
                );
            }
        }
    }
    // What folds key by key is what the schema calls a map or a table, and nothing else.
    for (kind, k) in kinds {
        for part in ["fields", "maps", "tables", "lists"] {
            let names = k.get(part).and_then(J::as_object).into_iter().flatten();
            for (name, _) in names {
                let folds = dereth_pack::records::RowKinds::is_map(
                    &empyrean_world_db::layers::Ac1Rows,
                    kind,
                    name,
                );
                if folds != matches!(part, "maps" | "tables") {
                    why.push(format!(
                        "{kind}.{name}: a {part} entry, folding key by key {folds}"
                    ));
                }
            }
        }
    }
    assert!(
        why.is_empty(),
        "{} problem(s):\n{}",
        why.len(),
        why.join("\n")
    );
    let described: BTreeSet<String> = kinds
        .iter()
        .filter(|(_, k)| k.get("open").is_none())
        .map(|(n, _)| n.clone())
        .collect();
    assert_eq!(
        seen.intersection(&described).count(),
        described.len(),
        "seen {seen:?}"
    );
}

/// The schema as `empyrean-import schema` writes it.
fn empyrean_import_schema() -> J {
    let dir = std::env::temp_dir().join(format!("row-schema-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a folder");
    let out = dir.join("schema.json");
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_empyrean-import"))
        .arg("schema")
        .arg("--out")
        .arg(&out)
        .status()
        .expect("runs");
    assert!(status.success());
    let text = std::fs::read_to_string(&out).expect("written");
    let _ = std::fs::remove_dir_all(&dir);
    serde_json::from_str(&text).expect("JSON")
}
