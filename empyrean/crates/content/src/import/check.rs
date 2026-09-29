//! `empyrean-import --check`: the records two packs disagree on, for reviewing a content change.
//!
//! Not ACE-derived. Records are compared by their stored bytes, table by table; a record is
//! added (only in the new pack), removed (only in the old) or changed (in both, different).

use crate::error::PackError;
use crate::pack::{Pack, TableId};
use crate::records::WeenieIndex;

pub mod fields;
pub mod overlap;

use fields::RecordFields;
use overlap::Overlap;

/// One table's differences, keys ascending.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TableDiff {
    pub table: &'static str,
    pub id: u16,
    pub added: Vec<u64>,
    pub removed: Vec<u64>,
    pub changed: Vec<u64>,
}

impl TableDiff {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

/// Every table on which `old` and `new` differ.
pub fn diff(old: &Pack, new: &Pack) -> Result<Vec<TableDiff>, PackError> {
    let mut out = Vec::new();
    for &t in TableId::ALL {
        let (a, b) = (old.keys(t), new.keys(t));
        let mut d = TableDiff {
            table: t.name(),
            id: t.0,
            ..TableDiff::default()
        };
        let (mut i, mut j) = (0, 0);
        while i < a.len() || j < b.len() {
            match (a.get(i), b.get(j)) {
                (Some(&x), Some(&y)) if x == y => {
                    if old.raw(t, x)? != new.raw(t, y)? {
                        d.changed.push(x);
                    }
                    i += 1;
                    j += 1;
                }
                (Some(&x), Some(&y)) if x < y => {
                    d.removed.push(x);
                    i += 1;
                }
                (Some(&x), None) => {
                    d.removed.push(x);
                    i += 1;
                }
                (_, Some(&y)) => {
                    d.added.push(y);
                    j += 1;
                }
                (None, None) => break,
            }
        }
        if !d.is_empty() {
            out.push(d);
        }
    }
    Ok(out)
}

/// A key as a person reads it: a weenie's class id and name, a landblock in hex, a cook book's
/// source and target.
#[must_use]
pub fn describe(table: u16, key: u64, old: &Pack, new: &Pack) -> String {
    match TableId(table) {
        TableId::WEENIE | TableId::WEENIE_INDEX => {
            let name = |p: &Pack| {
                p.get::<WeenieIndex>(TableId::WEENIE_INDEX, key)
                    .ok()
                    .flatten()
            };
            match name(new).or_else(|| name(old)) {
                Some(w) => format!("{key} {} ({})", w.class_name, w.name.unwrap_or_default()),
                None => key.to_string(),
            }
        }
        TableId::LANDBLOCK_INSTANCE | TableId::ENCOUNTER => format!("0x{key:04X}"),
        TableId::COOK_BOOK => format!("{} + {}", key >> 32, key & 0xFFFF_FFFF),
        _ => key.to_string(),
    }
}

/// The report as text, one line per record.
#[must_use]
pub fn render(diffs: &[TableDiff], old: &Pack, new: &Pack) -> String {
    render_with(diffs, None, None, old, new)
}

/// The report as text: one line per record, each changed record's field changes under it when
/// `fields` is given, then the overlap section when `overlap` is.
#[must_use]
pub fn render_with(
    diffs: &[TableDiff],
    fields: Option<&[RecordFields]>,
    overlap: Option<&[Overlap]>,
    old: &Pack,
    new: &Pack,
) -> String {
    let mut s = render_records(diffs, fields, old, new);
    if let Some(o) = overlap {
        if o.is_empty() {
            s.push_str("\noverlap: none of ours touches a record that changed\n");
        } else {
            s.push_str(&format!(
                "\noverlap: {} changed records that ours also touches\n",
                o.len()
            ));
            for r in o {
                s.push_str(&format!(
                    "  {} {} {}\n",
                    r.mark,
                    r.table,
                    describe(r.id, r.key, old, new)
                ));
                for what in &r.ours {
                    s.push_str(&format!("      {what}\n"));
                }
            }
        }
    }
    s
}

fn render_records(
    diffs: &[TableDiff],
    fields: Option<&[RecordFields]>,
    old: &Pack,
    new: &Pack,
) -> String {
    let mut s = String::new();
    if diffs.is_empty() {
        s.push_str("no differences\n");
        return s;
    }
    for d in diffs {
        s.push_str(&format!(
            "{}: {} added, {} changed, {} removed\n",
            d.table,
            d.added.len(),
            d.changed.len(),
            d.removed.len()
        ));
        for (mark, keys) in [("+", &d.added), ("~", &d.changed), ("-", &d.removed)] {
            for &k in keys {
                s.push_str(&format!(
                    "  {mark} {} {}\n",
                    d.table,
                    describe(d.id, k, old, new)
                ));
                if let Some(f) = fields.and_then(|f| f.iter().find(|r| r.id == d.id && r.key == k))
                {
                    s.push_str(&fields::render_changes(&f.changes, "      "));
                }
            }
        }
    }
    s
}

/// The report as JSON.
#[must_use]
pub fn to_json(diffs: &[TableDiff]) -> serde_json::Value {
    to_json_with(diffs, None, None)
}

/// The report as JSON, with `fields` (per changed record, its field changes) and `overlap`
/// when given.
#[must_use]
pub fn to_json_with(
    diffs: &[TableDiff],
    fields: Option<&[RecordFields]>,
    overlap: Option<&[Overlap]>,
) -> serde_json::Value {
    let mut v = to_json_records(diffs);
    if let Some(f) = fields {
        v["fields"] = f
            .iter()
            .map(|r| serde_json::json!({"table": r.table, "key": r.key, "changes": fields::changes_json(&r.changes)}))
            .collect();
    }
    if let Some(o) = overlap {
        v["overlap"] = o
            .iter()
            .map(|r| serde_json::json!({"table": r.table, "key": r.key, "upstream": r.mark.to_string(), "ours": r.ours}))
            .collect();
    }
    v
}

fn to_json_records(diffs: &[TableDiff]) -> serde_json::Value {
    serde_json::json!({
        "format": "world.pack check v1",
        "tables": diffs.iter().map(|d| serde_json::json!({
            "table": d.table,
            "added": d.added,
            "changed": d.changed,
            "removed": d.removed,
        })).collect::<Vec<_>>(),
    })
}
