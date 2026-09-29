//! Vectors: the configured ACE world dump, independently counted table by table
//! A pack built from ACE's real world dump has every table's row count, verifies, converts every
//! weenie, spot checks (drudge creature tables, chargen starter items).
//! Fixture: the configured ACE world dump, an independent row counter and a temporary pack.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::OnceLock;

use empyrean_content::adapter::convert_to_entity_weenie;
use empyrean_content::import::world_rows::WORLD_TABLES;
use empyrean_content::pack::{Pack, TableId};
use empyrean_content::PackContent;
use empyrean_entity::enums::{PropertyString, WeenieType};

fn dump_path() -> PathBuf {
    empyrean_common::test_paths::world_sql()
}

struct Built {
    pack: PathBuf,
    imported: empyrean_content::import::Imported,
    secs: f64,
}

fn built() -> &'static Built {
    static CELL: OnceLock<Built> = OnceLock::new();
    CELL.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("serv-content-real-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pack = dir.join("world.pack");
        let t0 = std::time::Instant::now();
        let imported = empyrean_content::import::import_file(
            &dump_path(),
            empyrean_content::import::Allow::default(),
            &pack,
            None,
        )
        .unwrap_or_else(|e| panic!("the real dump must import ({}): {e}", dump_path().display()));
        let secs = t0.elapsed().as_secs_f64();
        eprintln!(
            "real-content: built {} bytes in {secs:.1}s",
            imported.stats.file_len
        );
        Built {
            pack,
            imported,
            secs,
        }
    })
}

fn db() -> PackContent {
    PackContent::open(&built().pack).unwrap()
}

/// An independent reading of the dump: rows per table (tuples counted by a quote-aware scan),
/// every `weenie` row's class name and type, and every `Name` string row.
struct DumpFacts {
    rows: BTreeMap<String, u64>,
    weenies: BTreeMap<u32, (String, i32)>,
    names: HashMap<u32, String>,
}

fn facts() -> &'static DumpFacts {
    static CELL: OnceLock<DumpFacts> = OnceLock::new();
    CELL.get_or_init(|| {
        let text = std::fs::read(dump_path()).unwrap();
        let mut rows = BTreeMap::new();
        let mut weenies = BTreeMap::new();
        let mut names = HashMap::new();
        for line in text.split(|&b| b == b'\n') {
            let Some(rest) = line.strip_prefix(b"INSERT INTO `") else {
                continue;
            };
            let end = rest.iter().position(|&b| b == b'`').unwrap();
            let table = String::from_utf8(rest[..end].to_vec()).unwrap();
            let body = &rest[rest.windows(8).position(|w| w == b" VALUES ").unwrap() + 8..];
            let tuples = split_tuples(body);
            *rows.entry(table.clone()).or_insert(0) += tuples.len() as u64;
            if table == "weenie" || table == "weenie_properties_string" {
                for t in tuples {
                    let f = fields(t);
                    if table == "weenie" {
                        weenies.insert(num(&f[0]), (unquote(&f[1]), num(&f[2])));
                    } else if f[2] == b"1" {
                        names.insert(num(&f[1]), unquote(&f[3]));
                    }
                }
            }
        }
        DumpFacts {
            rows,
            weenies,
            names,
        }
    })
}

/// The `(...)` tuples of one `VALUES` list, respecting quotes and backslash escapes.
fn split_tuples(body: &[u8]) -> Vec<&[u8]> {
    let (mut out, mut depth, mut start, mut quoted, mut i) = (Vec::new(), 0, 0, false, 0);
    while i < body.len() {
        match body[i] {
            b'\\' if quoted => i += 1,
            b'\'' => quoted = !quoted,
            b'(' if !quoted => {
                if depth == 0 {
                    start = i + 1;
                }
                depth += 1;
            }
            b')' if !quoted => {
                depth -= 1;
                if depth == 0 {
                    out.push(&body[start..i]);
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}

fn fields(t: &[u8]) -> Vec<Vec<u8>> {
    let (mut out, mut cur, mut quoted, mut i) = (Vec::new(), Vec::new(), false, 0);
    while i < t.len() {
        match t[i] {
            b'\\' if quoted => {
                cur.push(t[i]);
                cur.push(t[i + 1]);
                i += 1;
            }
            b'\'' => {
                quoted = !quoted;
                cur.push(b'\'');
            }
            b',' if !quoted => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
        i += 1;
    }
    out.push(cur);
    out
}

fn num<T: std::str::FromStr>(f: &[u8]) -> T
where
    T::Err: std::fmt::Debug,
{
    std::str::from_utf8(f).unwrap().parse().unwrap()
}

fn unquote(f: &[u8]) -> String {
    let inner = &f[1..f.len() - 1];
    let mut out = Vec::new();
    let mut i = 0;
    while i < inner.len() {
        if inner[i] == b'\\' {
            out.push(match inner[i + 1] {
                b'n' => b'\n',
                b'r' => b'\r',
                b't' => b'\t',
                b'0' => 0,
                b'Z' => 0x1A,
                c => c,
            });
            i += 2;
        } else {
            out.push(inner[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

#[test]
fn every_table_is_imported_with_the_dumps_row_count() {
    let b = built();
    let f = facts();
    assert!(
        b.imported.missing_tables.is_empty(),
        "{:?}",
        b.imported.missing_tables
    );
    assert!(
        b.imported.unknown_tables.is_empty(),
        "{:?}",
        b.imported.unknown_tables
    );
    assert!(
        b.imported.unread_columns.is_empty(),
        "{:?}",
        b.imported.unread_columns
    );
    assert!(b.imported.orphans.is_empty(), "{:?}", b.imported.orphans);
    assert_eq!(
        f.rows
            .keys()
            .filter(|t| !WORLD_TABLES.contains(&t.as_str()))
            .count(),
        0
    );
    for (table, n) in &b.imported.rows {
        assert_eq!(
            *n,
            f.rows.get(*table).copied().unwrap_or(0),
            "rows of {table}"
        );
    }
    assert!(b.secs > 0.0);
}

#[test]
fn the_pack_holds_every_row_and_verifies() {
    let pack = Pack::open(&built().pack).unwrap();
    pack.verify_hash().unwrap();
    let f = facts();
    assert_eq!(u64::from(pack.count(TableId::WEENIE)), f.rows["weenie"]);
    assert_eq!(
        u64::from(pack.count(TableId::WEENIE_INDEX)),
        f.rows["weenie"]
    );
    let lbi: u64 = pack
        .all::<Vec<empyrean_content::models::world::LandblockInstance>>(TableId::LANDBLOCK_INSTANCE)
        .unwrap()
        .iter()
        .map(|(_, v)| v.len() as u64)
        .sum();
    assert_eq!(lbi, f.rows["landblock_instance"]);
    // Every child row is inside a weenie (GetAllWeenies has no type filter).
    let all = db().get_all_weenies();
    let sum = |g: fn(&empyrean_content::models::world::Weenie) -> usize| {
        all.iter().map(g).sum::<usize>() as u64
    };
    assert_eq!(
        sum(|w| w.weenie_properties_int.len()),
        f.rows["weenie_properties_int"]
    );
    assert_eq!(
        sum(|w| w.weenie_properties_string.len()),
        f.rows["weenie_properties_string"]
    );
    assert_eq!(
        sum(|w| w
            .weenie_properties_emote
            .iter()
            .map(|e| e.weenie_properties_emote_action.len())
            .sum()),
        f.rows["weenie_properties_emote_action"]
    );
    assert_eq!(
        sum(|w| usize::from(w.weenie_properties_book.is_some())),
        f.rows["weenie_properties_book"]
    );
}

#[test]
fn every_weenie_converts_and_matches_the_dump() {
    let f = facts();
    let db = db();
    let mut converted = 0usize;
    for w in db.get_all_weenies() {
        let (class_name, ty) = &f.weenies[&w.class_id];
        assert_eq!((&w.class_name, w.r#type), (class_name, *ty));
        let e = convert_to_entity_weenie(&w, false);
        assert_eq!(e.weenie_class_id, w.class_id);
        let name = e
            .properties_string
            .as_ref()
            .and_then(|s| s.get(&PropertyString::Name))
            .cloned();
        assert_eq!(
            name.as_ref(),
            f.names.get(&w.class_id),
            "Name of {}",
            w.class_id
        );
        converted += 1;
    }
    assert_eq!(converted as u64, f.rows["weenie"]);
    // The cached path too: GetCachedWeenie for every class id.
    for &wcid in f.weenies.keys() {
        assert!(db.get_cached_weenie(wcid).is_some(), "{wcid}");
    }
    assert_eq!(
        u64::try_from(db.get_weenie_cache_count()).unwrap(),
        f.rows["weenie"]
    );
}

/// The lowest class id whose `Name` is `name`, read from the dump.
fn wcid_named(name: &str, ty: Option<WeenieType>) -> u32 {
    let f = facts();
    #[allow(clippy::cast_possible_wrap)]
    let ty = ty.map(|t| t.0 as i32);
    *f.weenies
        .iter()
        .find(|(k, (_, t))| {
            f.names.get(k).is_some_and(|n| n == name) && ty.is_none_or(|ty| *t == ty)
        })
        .unwrap_or_else(|| panic!("no weenie named {name}"))
        .0
}

#[test]
fn a_drudge_is_a_creature_with_creature_tables() {
    let wcid = wcid_named("Drudge Skulker", Some(WeenieType::Creature));
    let db = db();
    let w = db.get_cached_weenie(wcid).unwrap();
    assert_eq!(w.weenie_type, WeenieType::Creature);
    assert!(
        w.properties_attribute
            .as_ref()
            .is_some_and(|a| a.len() == 6),
        "six attributes"
    );
    assert!(
        w.properties_attribute_2nd.is_some()
            && w.properties_body_part.is_some()
            && w.properties_skill.is_some()
    );
    assert!(db.is_creature_name_in_world_database("drudge skulker"));
}

#[test]
fn chargen_starter_items_resolve() {
    let db = db();
    for name in ["Handy Healing Kit", "Calling Stone", "Sack", "Pyreal"] {
        let wcid = wcid_named(name, None);
        let w = db.get_cached_weenie(wcid).unwrap();
        assert_eq!(
            w.properties_string
                .as_ref()
                .unwrap()
                .get(&PropertyString::Name)
                .map(String::as_str),
            Some(name)
        );
    }
}

#[test]
fn spot_checks_across_the_other_tables() {
    let db = db();
    assert!(db.is_world_database_guid_range_valid());
    assert!(db.get_version().is_some_and(|v| v.patch_version.is_some()));
    // Holtburg's landblock has statics, and every one of them carries its landblock.
    let lb = db.get_cached_instances_by_landblock(0xA9B4);
    assert!(!lb.is_empty() && lb.iter().all(|i| i.landblock == Some(0xA9B4)));
    let houses = db.get_houses_all();
    assert!(!houses.is_empty());
    db.cache_all_cookbooks();
    assert!(db.get_cookbook_cache_count() > 0);
    db.cache_all_spells();
    assert_eq!(
        u64::try_from(db.get_spell_cache_count()).unwrap(),
        facts().rows["spell"]
    );
    db.cache_all_weenies();
    assert!(db.get_all_events().len() as u64 == facts().rows["event"]);
    for (code, tier) in [(1, 1), (2, 3)] {
        if let Some(list) = db.get_cached_treasure_material_base(code, tier) {
            #[allow(clippy::cast_possible_truncation)]
            let total = list.iter().map(|m| f64::from(m.probability)).sum::<f64>() as f32;
            assert!((1.0 - total).abs() < 0.0001, "normalised: {total}");
        }
    }
}
