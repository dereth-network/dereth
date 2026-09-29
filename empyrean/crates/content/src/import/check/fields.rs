//! `empyrean-import --check --fields`: what changed inside each record two packs disagree on.
//!
//! Not ACE-derived. A weenie is compared property by property: each `weenie_properties_*` row
//! with a type is keyed by that type's name (`PropertyInt.ItemType`), the emote table by category
//! and the set's place among that category's sets (`Emote.Use#0.action[2].motion`), and the
//! remaining child tables (create list, generators, palettes, ...) as sets of rows, since their
//! row order carries no key. Every other record is compared field by field; records a pack groups
//! (landblock instances, encounters, cook books) are keyed by guid, cell and recipe. Row ids,
//! which a dump renumbers freely, are left out. Fields are listed in sorted order.

use std::collections::BTreeMap;
use std::fmt::Debug;

use empyrean_common::dotnet::DotNetDateTime;
use empyrean_entity::enums::{
    CombatBodyPart, EmoteCategory, PositionType, PropertyAttribute, PropertyAttribute2nd,
    PropertyBool, PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt, PropertyInt64,
    PropertyString, Skill, WeenieType,
};

use super::TableDiff;
use crate::error::PackError;
use crate::models::world::{
    CookBook, Encounter, Event, HousePortal, LandblockInstance, PointsOfInterest, Quest, Recipe,
    Spell, TreasureDeath, TreasureGemCount, TreasureMaterialBase, TreasureMaterialColor,
    TreasureMaterialGroups, TreasureWielded, Version, Weenie,
};
use crate::pack::{Codec, Pack, TableId};

/// One field that differs: `None` on a side that does not have it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChange {
    pub field: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

/// The field changes of one changed record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordFields {
    pub table: &'static str,
    pub id: u16,
    pub key: u64,
    pub changes: Vec<FieldChange>,
}

/// A record's fields: keyed values, and rows compared as a set (`(group, row)`).
#[derive(Debug, Default)]
struct Fields {
    keyed: Vec<(String, String)>,
    rows: Vec<(String, String)>,
}

/// The field changes of every record `diffs` lists as changed (`weenie_index`, derived from
/// `weenie`, is left out), in table and key order.
pub fn changed_fields(
    diffs: &[TableDiff],
    old: &Pack,
    new: &Pack,
) -> Result<Vec<RecordFields>, PackError> {
    let mut out = Vec::new();
    for d in diffs {
        let t = TableId(d.id);
        if t == TableId::WEENIE_INDEX {
            continue;
        }
        for &key in &d.changed {
            let changes = record_changes(t, key, old, new)?;
            out.push(RecordFields {
                table: d.table,
                id: d.id,
                key,
                changes,
            });
        }
    }
    Ok(out)
}

/// The fields of one record that differ between `old` and `new` (a side without the record has
/// no fields).
pub fn record_changes(
    t: TableId,
    key: u64,
    old: &Pack,
    new: &Pack,
) -> Result<Vec<FieldChange>, PackError> {
    Ok(diff_fields(
        &fields_of(t, key, old)?,
        &fields_of(t, key, new)?,
    ))
}

fn fields_of(t: TableId, key: u64, p: &Pack) -> Result<Fields, PackError> {
    fn one<T: Codec + Debug>(p: &Pack, t: TableId, key: u64) -> Result<Fields, PackError> {
        Ok(p.get::<T>(t, key)?
            .map(|r| Fields {
                keyed: flatten_debug(&r, ""),
                rows: Vec::new(),
            })
            .unwrap_or_default())
    }
    fn grouped<T: Codec + Debug>(
        p: &Pack,
        t: TableId,
        key: u64,
        name: impl Fn(&T) -> String,
    ) -> Result<Fields, PackError> {
        let rows = p.get::<Vec<T>>(t, key)?.unwrap_or_default();
        let mut keyed = Vec::new();
        for r in &rows {
            keyed.extend(flatten_debug(r, &name(r)));
        }
        Ok(Fields {
            keyed,
            rows: Vec::new(),
        })
    }
    match t {
        TableId::WEENIE => Ok(p
            .get::<Weenie>(t, key)?
            .map(|w| weenie_fields(&w))
            .unwrap_or_default()),
        TableId::LANDBLOCK_INSTANCE => {
            grouped::<LandblockInstance>(p, t, key, |i| format!("0x{:08X}", i.guid))
        }
        TableId::ENCOUNTER => {
            grouped::<Encounter>(p, t, key, |e| format!("cell {},{}", e.cell_x, e.cell_y))
        }
        TableId::COOK_BOOK => grouped::<CookBook>(p, t, key, |c| format!("recipe {}", c.recipe_id)),
        TableId::RECIPE => one::<Recipe>(p, t, key),
        TableId::EVENT => one::<Event>(p, t, key),
        TableId::HOUSE_PORTAL => one::<HousePortal>(p, t, key),
        TableId::POINTS_OF_INTEREST => one::<PointsOfInterest>(p, t, key),
        TableId::QUEST => one::<Quest>(p, t, key),
        TableId::SPELL => one::<Spell>(p, t, key),
        TableId::TREASURE_DEATH => one::<TreasureDeath>(p, t, key),
        TableId::TREASURE_GEM_COUNT => one::<TreasureGemCount>(p, t, key),
        TableId::TREASURE_MATERIAL_BASE => one::<TreasureMaterialBase>(p, t, key),
        TableId::TREASURE_MATERIAL_COLOR => one::<TreasureMaterialColor>(p, t, key),
        TableId::TREASURE_MATERIAL_GROUPS => one::<TreasureMaterialGroups>(p, t, key),
        TableId::TREASURE_WIELDED => one::<TreasureWielded>(p, t, key),
        TableId::VERSION => one::<Version>(p, t, key),
        _ => Ok(Fields::default()),
    }
}

fn named(kind: &str, name: Option<&str>, n: impl std::fmt::Display) -> String {
    name.map_or_else(|| format!("{kind}.{n}"), |name| format!("{kind}.{name}"))
}

/// A child row's fields as `field=value` pairs, its ids left out.
fn row_text<T: Debug>(row: &T) -> String {
    flatten_debug(row, "")
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn weenie_fields(w: &Weenie) -> Fields {
    #[allow(clippy::cast_sign_loss)]
    let weenie_type = WeenieType(w.r#type as u32)
        .name()
        .map_or_else(|| w.r#type.to_string(), str::to_owned);
    let mut k: Vec<(String, String)> = vec![
        ("class_name".to_owned(), w.class_name.clone()),
        ("type".to_owned(), weenie_type),
        ("last_modified".to_owned(), datetime(w.last_modified)),
    ];
    for r in &w.weenie_properties_int {
        k.push((
            named("PropertyInt", PropertyInt(r.r#type).name(), r.r#type),
            r.value.to_string(),
        ));
    }
    for r in &w.weenie_properties_int64 {
        k.push((
            named("PropertyInt64", PropertyInt64(r.r#type).name(), r.r#type),
            r.value.to_string(),
        ));
    }
    for r in &w.weenie_properties_bool {
        k.push((
            named("PropertyBool", PropertyBool(r.r#type).name(), r.r#type),
            r.value.to_string(),
        ));
    }
    for r in &w.weenie_properties_float {
        k.push((
            named("PropertyFloat", PropertyFloat(r.r#type).name(), r.r#type),
            r.value.to_string(),
        ));
    }
    for r in &w.weenie_properties_string {
        k.push((
            named("PropertyString", PropertyString(r.r#type).name(), r.r#type),
            format!("{:?}", r.value),
        ));
    }
    for r in &w.weenie_properties_did {
        k.push((
            named("PropertyDataId", PropertyDataId(r.r#type).name(), r.r#type),
            format!("0x{:08X}", r.value),
        ));
    }
    for r in &w.weenie_properties_iid {
        k.push((
            named(
                "PropertyInstanceId",
                PropertyInstanceId(r.r#type).name(),
                r.r#type,
            ),
            format!("0x{:08X}", r.value),
        ));
    }
    for r in &w.weenie_properties_position {
        k.push((
            named(
                "Position",
                PositionType(r.position_type).name(),
                r.position_type,
            ),
            format!(
                "0x{:08X} [{} {} {}] {} {} {} {}",
                r.obj_cell_id,
                r.origin_x,
                r.origin_y,
                r.origin_z,
                r.angles_w,
                r.angles_x,
                r.angles_y,
                r.angles_z
            ),
        ));
    }
    for r in &w.weenie_properties_attribute {
        k.push((
            named("Attribute", PropertyAttribute(r.r#type).name(), r.r#type),
            row_text(r),
        ));
    }
    for r in &w.weenie_properties_attribute_2nd {
        k.push((
            named(
                "Attribute2nd",
                PropertyAttribute2nd(r.r#type).name(),
                r.r#type,
            ),
            row_text(r),
        ));
    }
    for r in &w.weenie_properties_skill {
        k.push((
            named("Skill", Skill(i32::from(r.r#type)).name(), r.r#type),
            row_text(r),
        ));
    }
    for r in &w.weenie_properties_body_part {
        k.push((
            named("BodyPart", CombatBodyPart(i32::from(r.key)).name(), r.key),
            row_text(r),
        ));
    }
    for r in &w.weenie_properties_spell_book {
        k.push((
            format!("SpellBook.{}", r.spell),
            format!("probability {}", r.probability),
        ));
    }
    if let Some(b) = &w.weenie_properties_book {
        k.push(("Book".to_owned(), row_text(b)));
    }
    // Emote sets: by category and place among that category's sets.
    let mut seen: BTreeMap<u32, usize> = BTreeMap::new();
    for e in &w.weenie_properties_emote {
        let n = seen.entry(e.category).or_default();
        #[allow(clippy::cast_possible_wrap)]
        let prefix = format!(
            "{}#{n}",
            named("Emote", EmoteCategory(e.category as i32).name(), e.category)
        );
        *n += 1;
        for (path, v) in flatten_debug(e, &prefix) {
            k.push((
                path.replace(".weenie_properties_emote_action[", ".action["),
                v,
            ));
        }
    }
    let mut rows = Vec::new();
    let mut add = |group: &str, text: String| rows.push((group.to_owned(), text));
    w.weenie_properties_anim_part
        .iter()
        .for_each(|r| add("AnimPart", row_text(r)));
    w.weenie_properties_book_page_data
        .iter()
        .for_each(|r| add("BookPage", row_text(r)));
    w.weenie_properties_create_list
        .iter()
        .for_each(|r| add("CreateList", row_text(r)));
    w.weenie_properties_event_filter
        .iter()
        .for_each(|r| add("EventFilter", row_text(r)));
    w.weenie_properties_generator
        .iter()
        .for_each(|r| add("Generator", row_text(r)));
    w.weenie_properties_palette
        .iter()
        .for_each(|r| add("Palette", row_text(r)));
    w.weenie_properties_texture_map
        .iter()
        .for_each(|r| add("TextureMap", row_text(r)));
    Fields { keyed: k, rows }
}

fn diff_fields(old: &Fields, new: &Fields) -> Vec<FieldChange> {
    let a: BTreeMap<&str, &str> = old
        .keyed
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let b: BTreeMap<&str, &str> = new
        .keyed
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let mut out = Vec::new();
    let mut keys: Vec<&str> = a.keys().chain(b.keys()).copied().collect();
    keys.sort_unstable();
    keys.dedup();
    for k in keys {
        let (x, y) = (a.get(k), b.get(k));
        if x != y {
            out.push(FieldChange {
                field: k.to_owned(),
                old: x.map(|s| (*s).to_owned()),
                new: y.map(|s| (*s).to_owned()),
            });
        }
    }
    let mut counts: BTreeMap<(&str, &str), i64> = BTreeMap::new();
    for (g, r) in &old.rows {
        *counts.entry((g, r)).or_default() -= 1;
    }
    for (g, r) in &new.rows {
        *counts.entry((g, r)).or_default() += 1;
    }
    let mut row_changes: Vec<FieldChange> = Vec::new();
    for ((g, r), n) in counts {
        for _ in 0..n.unsigned_abs() {
            let (old, new) = if n < 0 {
                (Some(r.to_owned()), None)
            } else {
                (None, Some(r.to_owned()))
            };
            row_changes.push(FieldChange {
                field: g.to_owned(),
                old,
                new,
            });
        }
    }
    // Removed rows before added ones, per group.
    row_changes.sort_by(|x, y| (&x.field, x.old.is_none()).cmp(&(&y.field, y.old.is_none())));
    out.extend(row_changes);
    out
}

fn datetime(d: DotNetDateTime) -> String {
    d.format("yyyy-MM-dd HH:mm:ss")
}

/// A record's `Debug` form as `(path, value)` leaves under `prefix`: struct fields joined with
/// `.`, list items as `[i]`, `Some(x)` as `x`. Id fields (`id`, `object_id`, `emote_id`, and a
/// landblock instance link's `parent_guid`) are left out. A form that does not parse is one leaf.
#[must_use]
pub fn flatten_debug<T: Debug>(value: &T, prefix: &str) -> Vec<(String, String)> {
    let text = format!("{value:?}");
    let mut out = Vec::new();
    match (DebugParser {
        s: text.as_bytes(),
        i: 0,
    })
    .parse()
    {
        Some(node) => flatten(&node, prefix, &mut out),
        None => out.push((prefix.to_owned(), text)),
    }
    out
}

#[derive(Debug)]
enum Node {
    Leaf(String),
    Struct(String, Vec<(String, Node)>),
    Tuple(String, Vec<Node>),
    List(Vec<Node>),
}

const ID_FIELDS: &[&str] = &["id", "object_id", "emote_id", "parent_guid"];

fn join(prefix: &str, field: &str) -> String {
    if prefix.is_empty() {
        field.to_owned()
    } else {
        format!("{prefix}.{field}")
    }
}

fn flatten(n: &Node, path: &str, out: &mut Vec<(String, String)>) {
    match n {
        Node::Leaf(t) => out.push((path.to_owned(), t.clone())),
        Node::Struct(name, fields) => {
            if let [(f, Node::Leaf(ticks))] = fields.as_slice() {
                if name == "DotNetDateTime" && f == "ticks" {
                    if let Ok(t) = ticks.parse::<i64>() {
                        out.push((path.to_owned(), datetime(DotNetDateTime::from_ticks(t))));
                        return;
                    }
                }
            }
            if fields.is_empty() {
                out.push((path.to_owned(), name.clone()));
            }
            for (f, v) in fields {
                if !ID_FIELDS.contains(&f.as_str()) {
                    flatten(v, &join(path, f), out);
                }
            }
        }
        Node::Tuple(name, items) => match items.as_slice() {
            [one] if name == "Some" => flatten(one, path, out),
            [Node::Leaf(t)] => out.push((path.to_owned(), format!("{name}({t})"))),
            _ => {
                for (i, v) in items.iter().enumerate() {
                    flatten(v, &format!("{path}.{i}"), out);
                }
            }
        },
        Node::List(items) => {
            if items.is_empty() {
                out.push((path.to_owned(), "[]".to_owned()));
            }
            for (i, v) in items.iter().enumerate() {
                flatten(v, &format!("{path}[{i}]"), out);
            }
        }
    }
}

/// A reader of the compact `Debug` form of plain data: structs, tuples, lists, strings and
/// scalars.
struct DebugParser<'a> {
    s: &'a [u8],
    i: usize,
}

impl DebugParser<'_> {
    fn parse(mut self) -> Option<Node> {
        let n = self.value()?;
        self.ws();
        (self.i == self.s.len()).then_some(n)
    }

    fn ws(&mut self) {
        while self.s.get(self.i).is_some_and(u8::is_ascii_whitespace) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        if self.s.get(self.i) == Some(&c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn text(&self, from: usize) -> String {
        String::from_utf8_lossy(&self.s[from..self.i]).into_owned()
    }

    fn ident(&mut self) -> String {
        let from = self.i;
        loop {
            match self.s.get(self.i) {
                Some(c) if c.is_ascii_alphanumeric() || *c == b'_' => self.i += 1,
                Some(b':') if self.s.get(self.i + 1) == Some(&b':') => self.i += 2,
                _ => break,
            }
        }
        self.text(from)
    }

    fn quoted(&mut self, q: u8) -> Option<String> {
        let from = self.i;
        self.i += 1;
        loop {
            match *self.s.get(self.i)? {
                b'\\' => self.i += 2,
                c if c == q => {
                    self.i += 1;
                    return Some(self.text(from));
                }
                _ => self.i += 1,
            }
        }
    }

    fn items(&mut self, close: u8) -> Option<Vec<Node>> {
        let mut v = Vec::new();
        if self.eat(close) {
            return Some(v);
        }
        loop {
            v.push(self.value()?);
            if self.eat(close) {
                return Some(v);
            }
            if !self.eat(b',') {
                return None;
            }
            if self.eat(close) {
                return Some(v);
            }
        }
    }

    fn value(&mut self) -> Option<Node> {
        self.ws();
        let c = *self.s.get(self.i)?;
        match c {
            b'[' => {
                self.i += 1;
                self.items(b']').map(Node::List)
            }
            b'"' | b'\'' => self.quoted(c).map(Node::Leaf),
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let name = self.ident();
                if self.eat(b'{') {
                    let mut fields = Vec::new();
                    if self.eat(b'}') {
                        return Some(Node::Struct(name, fields));
                    }
                    loop {
                        self.ws();
                        let f = self.ident();
                        if f.is_empty() || !self.eat(b':') {
                            return None;
                        }
                        fields.push((f, self.value()?));
                        if self.eat(b'}') {
                            return Some(Node::Struct(name, fields));
                        }
                        if !self.eat(b',') {
                            return None;
                        }
                        if self.eat(b'}') {
                            return Some(Node::Struct(name, fields));
                        }
                    }
                }
                if self.eat(b'(') {
                    return self.items(b')').map(|v| Node::Tuple(name, v));
                }
                Some(Node::Leaf(name))
            }
            _ => {
                let from = self.i;
                while self.s.get(self.i).is_some_and(|c| {
                    !matches!(c, b',' | b')' | b']' | b'}') && !c.is_ascii_whitespace()
                }) {
                    self.i += 1;
                }
                (self.i > from).then(|| Node::Leaf(self.text(from)))
            }
        }
    }
}

/// The field changes as indented lines under a record's line.
#[must_use]
pub fn render_changes(changes: &[FieldChange], indent: &str) -> String {
    let side = |v: &Option<String>| v.clone().unwrap_or_else(|| "(none)".to_owned());
    let mut s = String::new();
    for c in changes {
        s += &format!(
            "{indent}{}: {} -> {}\n",
            c.field,
            side(&c.old),
            side(&c.new)
        );
    }
    s
}

/// The field changes as JSON.
#[must_use]
pub fn changes_json(changes: &[FieldChange]) -> serde_json::Value {
    changes
        .iter()
        .map(|c| serde_json::json!({"field": c.field, "old": c.old, "new": c.new}))
        .collect()
}
