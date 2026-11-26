//! Vectors: support/world_dump_fixture.rs, synthetic rows for every ACE world table
//! The rows' vocabulary `empyrean-import schema` gives an editor is the one world-db writes: every
//! file of a world's rows is where its kind's file rule puts it, and every field, map key, map
//! value and list entry of every row is described, of the type it is written as; a name is one the
//! schema's own tables give; and each row's item, made from its kind's pattern, is the one the
//! world's rows name, and the same item written another way reads back to it as theirs does.
//! Fixture: the synthetic dump's world written as rows, with a recipe's vital requirement and a
//! rules row added.
//!
//! The same world's weenies, quests and treasures, as rows and as ACE's SQL, with a schema of just
//! the names they use,
//! are the pair an editor's SQL view of a weenie's rows is checked against (Lyceum's
//! `world::row_sql` test): kept in Lyceum's tests, and the same as this world writes them.
//! `EMPYREAN_WRITE_ROW_SQL=1` writes them again.

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
            ("int" | "hex" | "block", T::Integer(_))
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
/// The fixture's world, with the one optional table its dump leaves out and a rules row.
fn fixture_world() -> empyrean_content::WorldContent {
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
    c
}

#[test]
fn every_row_a_world_writes_is_described_by_the_schema_an_editor_reads() {
    let schema = empyrean_import_schema();
    let c = fixture_world();
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
                // And its file, from the kind's file rule, is the one it is written in.
                match dereth_pack::records::item_from(file, r) {
                    Ok(got) if got == *path => {}
                    got => why.push(format!("{path} {kind}: file {got:?}")),
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
    // A folder of each call's own: the tests here run at once.
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("row-schema-{}-{n}", std::process::id()));
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

/// Every string a value of `v` holds, and every key of its tables, for the names a schema keeps.
fn words(v: &T, out: &mut BTreeSet<String>) {
    match v {
        T::String(s) => {
            out.insert(s.clone());
        }
        T::Array(a) => a.iter().for_each(|v| words(v, out)),
        T::Table(t) => {
            for (k, v) in t {
                out.insert(k.clone());
                words(v, out);
            }
        }
        _ => {}
    }
}

/// The fixture world's weenies, quests and death and wielded treasures as rows
/// (`<kind>-<key>.toml`) and as ACE's SQL (`<kind>-<key>.sql`), and the schema with the names they
/// use (`schema.json`), are the pair kept in Lyceum's tests for its SQL view of rows.
#[test]
fn the_row_and_sql_pair_an_editors_view_is_checked_against_is_the_worlds_own() {
    let c = fixture_world();
    let files =
        empyrean_world_db::write(&c, empyrean_world_db::files::default_stamp()).expect("its rows");
    let mut want: Vec<(String, String)> = Vec::new();
    let mut used = BTreeSet::new();
    use empyrean_content::export::sql::{
        quest_sql_writer::QuestSQLWriter, sql_writer::SqlOut,
        treasure_death_sql_writer::TreasureDeathSQLWriter,
        treasure_wielded_sql_writer::TreasureWieldedSQLWriter, weenie_sql_writer::WeenieSQLWriter,
    };
    for (path, text) in &files {
        let Some((kind, key)) = path.strip_suffix(".toml").and_then(|p| p.split_once('/')) else {
            continue;
        };
        let rows: toml::Table = toml::from_str(text).expect("rows");
        let mut out = SqlOut::new("\n");
        match kind {
            "weenie" => {
                let class: u32 = key.parse().expect("a class");
                let w = c
                    .weenies
                    .iter()
                    .find(|w| w.class_id == class)
                    .expect("its weenie");
                WeenieSQLWriter::default()
                    .create_sql_insert_statement(w, &mut out)
                    .expect("its SQL");
            }
            "quest" => {
                for r in rows["quest"].as_array().expect("quests") {
                    let name = r["name"].as_str().expect("a name");
                    let q = c.quests.iter().find(|q| q.name == name).expect("its quest");
                    QuestSQLWriter::default().create_sql_insert_statement(q, &mut out);
                }
            }
            "treasure_death" => {
                let t: u32 = key.parse().expect("a type");
                let d = c
                    .treasure_death
                    .iter()
                    .find(|d| d.treasure_type == t)
                    .expect("its treasure");
                TreasureDeathSQLWriter::default().create_sql_insert_statement(d, &mut out);
            }
            "treasure_wielded" => {
                let t: u32 = key.parse().expect("a type");
                let entries: Vec<_> = c
                    .treasure_wielded
                    .iter()
                    .filter(|w| w.treasure_type == t)
                    .cloned()
                    .collect();
                TreasureWieldedSQLWriter::default()
                    .create_sql_insert_statement(&entries, &mut out)
                    .expect("its SQL");
            }
            _ => continue,
        }
        words(&T::Table(rows), &mut used);
        let name = format!("{kind}-{}", key.trim_start_matches('0'));
        want.push((format!("{name}.toml"), text.clone()));
        want.push((format!("{name}.sql"), out.text));
    }
    for kind in ["weenie", "quest", "treasure_death", "treasure_wielded"] {
        assert!(
            want.iter().any(|(n, _)| n.starts_with(&format!("{kind}-"))),
            "the fixture has a {kind}"
        );
    }
    // The schema's names the rows use, and the weenie's rows' description.
    let mut schema = empyrean_import_schema();
    for part in ["properties", "enums"] {
        for (_, names) in schema[part].as_object_mut().expect(part).iter_mut() {
            names
                .as_object_mut()
                .expect("names")
                .retain(|_, n| n.as_str().is_some_and(|n| used.contains(n)));
        }
        schema[part]
            .as_object_mut()
            .expect(part)
            .retain(|_, names| !names.as_object().is_some_and(serde_json::Map::is_empty));
    }
    let mut rows = serde_json::Map::new();
    for kind in [
        "weenie",
        "placement",
        "quest",
        "treasure_death",
        "treasure_wielded",
        "house_portal",
        "rules",
    ] {
        rows.insert(kind.into(), schema["rows"][kind].take());
    }
    schema["rows"] = J::Object(rows);
    want.push((
        "schema.json".into(),
        serde_json::to_string_pretty(&schema).expect("JSON") + "\n",
    ));
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../lyceum/crates/foundry/tests/cpu/world/row_sql_pair");
    if std::env::var_os("EMPYREAN_WRITE_ROW_SQL").is_some() {
        std::fs::create_dir_all(&dir).expect("its folder");
        for (name, text) in &want {
            std::fs::write(dir.join(name), text).expect("written");
        }
    }
    for (name, text) in &want {
        let kept = std::fs::read_to_string(dir.join(name)).unwrap_or_default();
        assert!(
            kept == *text,
            "{name} in {} is not what the world writes; EMPYREAN_WRITE_ROW_SQL=1 writes it again",
            dir.display()
        );
    }
}
