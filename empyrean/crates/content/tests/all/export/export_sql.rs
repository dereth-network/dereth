//! Vectors: empyrean/fixtures/vectors/content_export_sql
//! Weenie, recipe/cookbook, landblock instance, encounter, event/quest/spell writers write ACE's
//! SQL exactly (plain and named); trim negative zero, null-field fixes, enum names, treasure
//! labels.
//! Fixture: tests/fixtures/patches/base.sql and locally constructed edge cases.

use std::ops::DerefMut;
use std::path::PathBuf;

use empyrean_common::dotnet::{format as dn, DotNetDict};
use empyrean_common::vectors::{self, VectorFile};
use empyrean_content::export::sql::*;
use empyrean_content::import::patch::{InputKind, Source};
use empyrean_content::import::{build_from, default_now};
use empyrean_content::models::world::{
    TreasureDeath, TreasureWielded, Weenie, WeeniePropertiesDID,
};
use empyrean_content::pack::Pack;
use empyrean_content::PackContent;
use empyrean_entity::enums::{PropertyDataId, PropertyInt};
use serde_json::Value;

const BASE: &str = include_str!("../../fixtures/patches/base.sql");

pub(crate) fn load(name: &str) -> VectorFile {
    vectors::load_named("content_export_sql", name)
}

fn u32_of(v: &Value) -> u32 {
    u32::try_from(v.as_u64().unwrap()).unwrap()
}

fn i32_of(v: &Value) -> i32 {
    i32::try_from(v.as_i64().unwrap()).unwrap()
}

fn f32_of(v: &Value) -> f32 {
    vectors::f32_of(v).unwrap()
}

fn pairs(v: &Value) -> DotNetDict<u32, String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| (u32_of(&p[0]), p[1].as_str().unwrap().to_owned()))
        .collect()
}

fn treasure_wielded(v: &Value) -> TreasureWielded {
    TreasureWielded {
        weenie_class_id: u32_of(&v["weenie_class_id"]),
        probability: f32_of(&v["probability"]),
        set_start: v["set_start"].as_bool().unwrap(),
        has_sub_set: v["has_sub_set"].as_bool().unwrap(),
        continues_previous_set: v["continues_previous_set"].as_bool().unwrap(),
        stack_size: i32_of(&v["stack_size"]),
        stack_size_variance: f32_of(&v["stack_size_variance"]),
        palette_id: u32_of(&v["palette_id"]),
        shade: f32_of(&v["shade"]),
        ..TreasureWielded::default()
    }
}

/// `PacketOpCodeNames.Values`, checked against the harness's dump of ACE's table (every pair,
/// in enumeration order).
fn packet_op_code_names(ace: &Value) -> DotNetDict<u32, String> {
    let ours = empyrean_entity::packet_op_code_names::values();
    let ace = pairs(ace);
    let ours_pairs: Vec<_> = ours.iter().map(|(k, v)| (*k, v.clone())).collect();
    let ace_pairs: Vec<_> = ace.iter().map(|(k, v)| (*k, v.clone())).collect();
    assert_eq!(
        ours_pairs, ace_pairs,
        "PacketOpCodeNames.Values differs from ACE's"
    );
    ours
}

/// `dicts.json`: the dictionaries of every named writer, and `Environment.NewLine`.
pub(crate) struct Dicts {
    environment_new_line: &'static str,
    named: SQLWriter,
}

pub(crate) fn dicts() -> Dicts {
    let file = load("dicts");
    let d = &file.cases[0].output;
    let environment_new_line = match d["environment_new_line"].as_str().unwrap() {
        "\r\n" => "\r\n",
        "\n" => "\n",
        other => panic!("unexpected Environment.NewLine {other:?}"),
    };
    let named = SQLWriter {
        weenie_names: Some(pairs(&d["weenie_names"])),
        spell_names: Some(pairs(&d["spell_names"])),
        packet_op_codes: Some(packet_op_code_names(&d["packet_op_codes"])),
        treasure_death: Some(
            d["treasure_death"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| {
                    (
                        u32_of(&p[0]),
                        TreasureDeath {
                            treasure_type: u32_of(&p[0]),
                            tier: i32_of(&p[1]),
                            ..TreasureDeath::default()
                        },
                    )
                })
                .collect(),
        ),
        treasure_wielded: Some(
            d["treasure_wielded"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| {
                    (
                        u32_of(&p[0]),
                        p[1].as_array()
                            .unwrap()
                            .iter()
                            .map(treasure_wielded)
                            .collect(),
                    )
                })
                .collect(),
        ),
        weenies: Some(
            d["weenies"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| {
                    let wcid = u32_of(&p[0]);
                    let dids = p[1]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|d| WeeniePropertiesDID {
                            object_id: wcid,
                            r#type: u16::try_from(d[0].as_u64().unwrap()).unwrap(),
                            value: u32_of(&d[1]),
                            ..WeeniePropertiesDID::default()
                        })
                        .collect();
                    (
                        wcid,
                        Weenie {
                            class_id: wcid,
                            weenie_properties_did: dids,
                            ..Weenie::default()
                        },
                    )
                })
                .collect(),
        ),
    };
    Dicts {
        environment_new_line,
        named,
    }
}

/// A writer with no dictionaries (`plain`) or with every dictionary (`named`).
pub(crate) fn writer<W: Default + DerefMut<Target = SQLWriter>>(d: &Dicts, named: bool) -> W {
    let mut w = W::default();
    if named {
        *w = d.named.clone();
    }
    w
}

/// Apply a case's SQL to the fixture dump and open the pack.
fn database(sql: &str) -> PackContent {
    let source = Source {
        kind: InputKind::Sql,
        path: PathBuf::from("case.sql"),
        bytes: sql.as_bytes().to_vec(),
    };
    let (bytes, _) =
        build_from(BASE.as_bytes(), std::iter::once(Ok(source)), default_now()).unwrap();
    PackContent::new(Pack::from_bytes(bytes).unwrap())
}

/// Collects mismatches instead of stopping at the first.
pub(crate) struct Checker {
    failures: Vec<String>,
    checked: usize,
    /// Calls where ACE threw on one of V364's fixed defects and ours writes the export.
    pub(crate) fixed_throws: usize,
    /// Lines that differ from ACE's only by V364's fixes (see [`as_ace_wrote`]).
    pub(crate) fixed_lines: usize,
}

/// The exceptions ACE's writers threw on the defects V364 fixed (an unnamed skill, a missing
/// weenie name or name list, a teleloc without origin or angles, an unnamed modification index).
const FIXED_THROWS: [&str; 4] = [
    "System.NullReferenceException",
    "System.InvalidOperationException",
    "System.Collections.Generic.KeyNotFoundException",
    "System.ArgumentOutOfRangeException",
];

/// Our text as ACE's writer would have written it before V364's fixes: the
/// gameplay-options blob as ACE's `System.Byte[]`, and ACE's null-field replacements run over the
/// whole line, string values and comments included (ours leave those alone).
pub(crate) use crate::support::sql_text::as_ace_wrote;

impl Checker {
    pub(crate) fn new() -> Self {
        Self {
            failures: Vec::new(),
            checked: 0,
            fixed_throws: 0,
            fixed_lines: 0,
        }
    }

    /// A `Program.Try` value: a string, null, or `{throws}`.
    pub(crate) fn value(
        &mut self,
        at: &str,
        want: &Value,
        got: Result<Option<String>, SqlWriterError>,
    ) {
        self.checked += 1;
        match (vectors::throws(want), got) {
            (Some(t), Err(e)) if t == e.exception => {}
            (None, Ok(g)) if want.as_str() == g.as_deref() => {}
            // V364 (a fix): ours writes what ACE threw on
            (Some(t), Ok(Some(_))) if FIXED_THROWS.contains(&t) => self.fixed_throws += 1,
            (_, got) => self
                .failures
                .push(format!("{at}\n  ACE:  {want}\n  ours: {got:?}")),
        }
    }

    /// A default file name. `IllegalInFileName` is `Path.GetInvalidFileNameChars()` of the host,
    /// and the vectors were recorded on Windows; elsewhere the host set is only `\0` and `/`, so
    /// the characters only Windows forbids are replaced here as ACE-on-Windows replaced them, and
    /// the rest of the name is still compared exactly.
    pub(crate) fn file_name(&mut self, at: &str, want: &Value, got: String) {
        let windows_only = |c: char| {
            matches!(
                c,
                '"' | '<' | '>' | '|' | ':' | '*' | '?' | '\\' | '\u{1}'..='\u{1f}'
            )
        };
        let got = if cfg!(windows) {
            got
        } else {
            got.chars()
                .map(|c| if windows_only(c) { '_' } else { c })
                .collect()
        };
        self.value(at, want, Ok(Some(got)));
    }

    /// A `Capture`d writer call: `{text[, throws]}`.
    pub(crate) fn text(
        &mut self,
        at: &str,
        want: &Value,
        out: &SqlOut,
        err: Option<SqlWriterError>,
    ) {
        self.checked += 1;
        let want_text = want["text"].as_str().unwrap();
        let want_throws = want.get("throws").and_then(Value::as_str);
        if let (Some(t), None) = (want_throws, &err) {
            if FIXED_THROWS.contains(&t) {
                // V364 (a fix): ours writes the whole export where ACE threw part way; ACE's
                // text is where it stopped
                let ace: Vec<&str> = want_text.split('\n').collect();
                let ours: Vec<&str> = out.text.split('\n').collect();
                let head = &ace[..ace.len() - 1];
                let same = head.len() <= ours.len()
                    && head
                        .iter()
                        .zip(&ours)
                        .all(|(a, b)| a == b || *a == as_ace_wrote(b));
                if !same {
                    self.failures.push(format!(
                        "{at}: ACE threw {t}; its text so far differs from ours"
                    ));
                }
                self.fixed_throws += 1;
                return;
            }
        }
        if want_throws != err.as_ref().map(|e| e.exception) {
            self.failures
                .push(format!("{at}: ACE throws {want_throws:?}, ours {err:?}"));
        }
        let (w, g): (Vec<&str>, Vec<&str>) = (
            want_text.split('\n').collect(),
            out.text.split('\n').collect(),
        );
        // V364 (a fix): a line may differ from ACE's only by the null-field and blob fixes
        let fixed = w.len() == g.len()
            && w.iter()
                .zip(&g)
                .all(|(a, b)| a == b || *a == as_ace_wrote(b));
        if want_text != out.text && fixed {
            self.fixed_lines += 1;
        } else if want_text != out.text {
            let first = w
                .iter()
                .zip(&g)
                .position(|(a, b)| a != b)
                .unwrap_or(w.len().min(g.len()));
            self.failures.push(format!(
                "{at}: line {first}\n  ACE:  {:?}\n  ours: {:?}",
                w.get(first),
                g.get(first)
            ));
        }
    }

    pub(crate) fn finish(self, what: &str) {
        assert!(
            self.failures.is_empty(),
            "{what}: {} of {} differ:\n{}",
            self.failures.len(),
            self.checked,
            self.failures.join("\n")
        );
        assert!(self.checked > 0, "{what}: only {} checks ran", self.checked);
    }
}

/// Run one writer call into a fresh [`SqlOut`].
pub(crate) fn run(
    d: &Dicts,
    f: impl FnOnce(&mut SqlOut) -> Result<(), SqlWriterError>,
) -> (SqlOut, Option<SqlWriterError>) {
    let mut out = SqlOut::with_environment_new_line("\n", d.environment_new_line);
    let err = f(&mut out).err();
    (out, err)
}

pub(crate) const MODES: [(&str, bool); 2] = [("plain", false), ("named", true)];

#[test]
fn weenie_writer_matches_ace() {
    let d = dicts();
    let mut c = Checker::new();
    for case in &load("weenie").cases {
        let name = case.input["name"].as_str().unwrap();
        let db = database(case.input["sql"].as_str().unwrap());
        let weenie = db
            .get_weenie(u32_of(&case.input["key"]))
            .unwrap_or_else(|| panic!("{name}: weenie not loaded"));
        for (mode, named) in MODES {
            let w: WeenieSQLWriter = writer(&d, named);
            let want = &case.output[mode];
            let at = |part: &str| format!("{name}/{mode}/{part}");
            c.file_name(
                &at("file_name"),
                &want["file_name"],
                w.get_default_file_name(&weenie),
            );
            c.value(
                &at("subfolder"),
                &want["subfolder"],
                Ok(Some(w.get_default_subfolder(&weenie))),
            );
            let (out, err) = run(&d, |o| {
                w.create_sql_delete_statement(&weenie, o);
                Ok(())
            });
            c.text(&at("delete"), &want["delete"], &out, err);
            let (out, err) = run(&d, |o| w.create_sql_insert_statement(&weenie, o));
            c.text(&at("insert"), &want["insert"], &out, err);
        }
    }
    c.finish("weenie");
}

#[test]
fn recipe_and_cook_book_writers_match_ace() {
    let d = dicts();
    let mut c = Checker::new();
    for case in &load("recipe").cases {
        let name = case.input["name"].as_str().unwrap();
        let db = database(case.input["sql"].as_str().unwrap());
        let cook_books: Vec<_> = db
            .get_cookbooks_by_recipe_id(u32_of(&case.input["key"]))
            .into_iter()
            .map(|c| (*c.unwrap()).clone())
            .collect();
        let recipe = cook_books[0].recipe.clone().unwrap();
        for (mode, named) in MODES {
            let rw: RecipeSQLWriter = writer(&d, named);
            let cw: CookBookSQLWriter = writer(&d, named);
            let want = &case.output[mode];
            let at = |part: &str| format!("{name}/{mode}/{part}");
            c.file_name(
                &at("file_name"),
                &want["file_name"],
                rw.get_default_file_name(&recipe, Some(&cook_books), false)
                    .unwrap(),
            );
            c.file_name(
                &at("file_name_no_cookbooks"),
                &want["file_name_no_cookbooks"],
                rw.get_default_file_name(&recipe, None, false).unwrap(),
            );
            c.value(
                &at("description"),
                &want["description"],
                Ok(rw.get_default_file_name(&recipe, Some(&cook_books), true)),
            );
            c.file_name(
                &at("cookbook_file_name"),
                &want["cookbook_file_name"],
                cw.get_default_file_name(&cook_books[0]),
            );
            let (out, err) = run(&d, |o| {
                rw.create_sql_delete_statement(&recipe, o);
                Ok(())
            });
            c.text(&at("delete"), &want["delete"], &out, err);
            let (out, err) = run(&d, |o| rw.create_sql_insert_statement(&recipe, o));
            c.text(&at("insert"), &want["insert"], &out, err);
            let (out, err) = run(&d, |o| cw.create_sql_delete_statement(&cook_books, o));
            c.text(&at("cookbook_delete"), &want["cookbook_delete"], &out, err);
            let (out, err) = run(&d, |o| cw.create_sql_insert_statement(&cook_books, o));
            c.text(&at("cookbook_insert"), &want["cookbook_insert"], &out, err);
        }
    }
    c.finish("recipe");
}

#[test]
fn landblock_instance_writer_matches_ace() {
    let d = dicts();
    let mut c = Checker::new();
    for case in &load("landblock").cases {
        let name = case.input["name"].as_str().unwrap();
        let db = database(case.input["sql"].as_str().unwrap());
        let landblock = u16::try_from(case.input["key"].as_u64().unwrap()).unwrap();
        let instances = db.get_cached_instances_by_landblock(landblock);
        for (mode, named) in MODES {
            let w: LandblockInstanceWriter = writer(&d, named);
            let want = &case.output[mode];
            let at = |part: &str| format!("{name}/{mode}/{part}");
            c.file_name(
                &at("file_name"),
                &want["file_name"],
                w.get_default_file_name(&instances[0]),
            );
            let (out, err) = run(&d, |o| w.create_sql_delete_statement(&instances, o));
            c.text(&at("delete"), &want["delete"], &out, err);
            let (out, err) = run(&d, |o| w.create_sql_insert_statement(&instances, o));
            c.text(&at("insert"), &want["insert"], &out, err);
        }
    }
    c.finish("landblock");
}

#[test]
fn encounter_writer_matches_ace() {
    let d = dicts();
    let mut c = Checker::new();
    for case in &load("encounter").cases {
        let name = case.input["name"].as_str().unwrap();
        let db = database(case.input["sql"].as_str().unwrap());
        let landblock = u16::try_from(case.input["key"].as_u64().unwrap()).unwrap();
        let encounters = db.get_cached_encounters_by_landblock(landblock);
        for (mode, named) in MODES {
            let w: EncounterSQLWriter = writer(&d, named);
            let want = &case.output[mode];
            let at = |part: &str| format!("{name}/{mode}/{part}");
            c.file_name(
                &at("file_name"),
                &want["file_name"],
                w.get_default_file_name(&encounters[0]),
            );
            let (out, err) = run(&d, |o| w.create_sql_delete_statement(&encounters, o));
            c.text(&at("delete"), &want["delete"], &out, err);
            let (out, err) = run(&d, |o| w.create_sql_insert_statement(&encounters, o));
            c.text(&at("insert"), &want["insert"], &out, err);
        }
    }
    c.finish("encounter");
}

#[test]
fn event_quest_and_spell_writers_match_ace() {
    let d = dicts();
    let mut c = Checker::new();
    for case in &load("event").cases {
        let name = case.input["name"].as_str().unwrap();
        let db = database(case.input["sql"].as_str().unwrap());
        let event = db
            .get_cached_event(case.input["key"].as_str().unwrap())
            .unwrap();
        for (mode, named) in MODES {
            let w: EventSQLWriter = writer(&d, named);
            let want = &case.output[mode];
            let at = |part: &str| format!("event {name}/{mode}/{part}");
            c.file_name(
                &at("file_name"),
                &want["file_name"],
                w.get_default_file_name(&event),
            );
            let (out, err) = run(&d, |o| {
                w.create_sql_delete_statement(&event, o);
                Ok(())
            });
            c.text(&at("delete"), &want["delete"], &out, err);
            let (out, err) = run(&d, |o| {
                w.create_sql_insert_statement(&event, o);
                Ok(())
            });
            c.text(&at("insert"), &want["insert"], &out, err);
        }
    }
    for case in &load("quest").cases {
        let name = case.input["name"].as_str().unwrap();
        let db = database(case.input["sql"].as_str().unwrap());
        let quest = db
            .get_cached_quest(case.input["key"].as_str().unwrap())
            .unwrap();
        for (mode, named) in MODES {
            let w: QuestSQLWriter = writer(&d, named);
            let want = &case.output[mode];
            let at = |part: &str| format!("quest {name}/{mode}/{part}");
            c.file_name(
                &at("file_name"),
                &want["file_name"],
                w.get_default_file_name(&quest),
            );
            let (out, err) = run(&d, |o| {
                w.create_sql_delete_statement(&quest, o);
                Ok(())
            });
            c.text(&at("delete"), &want["delete"], &out, err);
            let (out, err) = run(&d, |o| {
                w.create_sql_insert_statement(&quest, o);
                Ok(())
            });
            c.text(&at("insert"), &want["insert"], &out, err);
        }
    }
    for case in &load("spell").cases {
        let name = case.input["name"].as_str().unwrap();
        let db = database(case.input["sql"].as_str().unwrap());
        let spell = db.get_cached_spell(u32_of(&case.input["key"])).unwrap();
        for (mode, named) in MODES {
            let w: SpellSQLWriter = writer(&d, named);
            let want = &case.output[mode];
            let at = |part: &str| format!("spell {name}/{mode}/{part}");
            c.file_name(
                &at("file_name"),
                &want["file_name"],
                w.get_default_file_name(&spell),
            );
            let (out, err) = run(&d, |o| {
                w.create_sql_delete_statement(&spell, o);
                Ok(())
            });
            c.text(&at("delete"), &want["delete"], &out, err);
            let (out, err) = run(&d, |o| {
                w.create_sql_insert_statement(&spell, o);
                Ok(())
            });
            c.text(&at("insert"), &want["insert"], &out, err);
        }
    }
    c.finish("event/quest/spell");
}

#[test]
fn trim_negative_zero_matches_ace() {
    let mut c = Checker::new();
    for case in &load("trim_negative_zero").cases {
        let input = vectors::f32_of(&case.input["f"]);
        let got = SQLWriter::trim_negative_zero(input);
        let at = format!("TrimNegativeZero({input:?})");
        match (&case.output, got) {
            (Value::Null, None) => c.checked += 1,
            (want, Some(r)) if !want.is_null() => {
                c.checked += 1;
                if !vectors::same_f32(r, f32_of(&want["r"])) {
                    c.failures
                        .push(format!("{at}: ACE {} ours {r:?}", want["r"]));
                }
                c.value(&format!("{at} g"), &want["g"], Ok(Some(dn(r, "0.######"))));
                c.value(&format!("{at} f6"), &want["f6"], Ok(Some(dn(r, "F6"))));
            }
            (want, got) => c.failures.push(format!("{at}: ACE {want} ours {got:?}")),
        }
    }
    c.finish("trim_negative_zero");
}

#[test]
fn fix_null_fields_and_sql_strings_match_ace() {
    let mut c = Checker::new();
    for case in &load("fix_null_fields").cases {
        let s = case.input["s"].as_str();
        c.value(
            &format!("GetSQLString({s:?})"),
            &case.output["sql"],
            Ok(SQLWriter::get_sql_string(s)),
        );
        if let Some(s) = s {
            c.value(
                &format!("FixNullFields({s:?})"),
                &case.output["fix"],
                Ok(Some(SQLWriter::fix_null_fields(s))),
            );
        }
    }
    c.finish("fix_null_fields");
}

#[test]
fn value_enum_names_match_ace() {
    let d = dicts();
    let plain = SQLWriter::default();
    let mut c = Checker::new();
    for case in &load("enum_name_int").cases {
        let p = PropertyInt(u16::try_from(case.input["p"].as_u64().unwrap()).unwrap());
        let v = i32_of(&case.input["v"]);
        c.value(
            &format!("GetValueEnumName({p:?}, {v}) plain"),
            &case.output["plain"],
            Ok(plain.get_value_enum_name_int(p, v)),
        );
        c.value(
            &format!("GetValueEnumName({p:?}, {v}) named"),
            &case.output["named"],
            Ok(d.named.get_value_enum_name_int(p, v)),
        );
    }
    for case in &load("enum_name_did").cases {
        let p = PropertyDataId(u16::try_from(case.input["p"].as_u64().unwrap()).unwrap());
        let v = u32_of(&case.input["v"]);
        let nl = d.environment_new_line;
        c.value(
            &format!("GetValueEnumName({p:?}, {v}) plain"),
            &case.output["plain"],
            plain.get_value_enum_name_did(p, v, nl),
        );
        c.value(
            &format!("GetValueEnumName({p:?}, {v}) named"),
            &case.output["named"],
            d.named.get_value_enum_name_did(p, v, nl),
        );
    }
    c.finish("enum names");
}

#[test]
fn treasure_labels_match_ace() {
    let d = dicts();
    let nl = d.environment_new_line;
    let plain = SQLWriter::default();
    let mut c = Checker::new();
    for (n, case) in load("treasure_did").cases.iter().enumerate() {
        let list: Vec<TreasureWielded> = case.input["list"]
            .as_array()
            .unwrap()
            .iter()
            .map(treasure_wielded)
            .collect();
        c.value(
            &format!("GetValuesForTreasureDID #{n}"),
            &case.output,
            d.named.get_values_for_treasure_did(&list, nl).map(Some),
        );
    }
    for case in &load("treasure_data").cases {
        let v = u32_of(&case.input["v"]);
        let weenie = case.input["weenie"].as_bool().unwrap();
        let at = format!("GetValueForTreasureData({v}, {weenie})");
        c.value(
            &format!("{at} plain"),
            &case.output["plain"],
            plain.get_value_for_treasure_data(v, weenie, nl).map(Some),
        );
        c.value(
            &format!("{at} named"),
            &case.output["named"],
            d.named.get_value_for_treasure_data(v, weenie, nl).map(Some),
        );
    }
    c.finish("treasure labels");
}
