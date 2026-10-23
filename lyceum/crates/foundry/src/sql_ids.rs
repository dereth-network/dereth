//! The data-file records server SQL names: every one must be a record the files hold, of the kind
//! its column means.
//!
//! A weenie whose setup, motion table, palette or parts name records the files do not hold is
//! drawn as nothing, or not at all, by every client, while the server serves it happily: the fault
//! shows only on screen. The columns read:
//!
//! | table | column | record |
//! |---|---|---|
//! | `weenie_properties_d_i_d` | `value`, by `type` ([`did_kind`]) | setup, motion table, sound table, combat table, quality filter, palette, clothing, icon, face texture, head, physics effect table |
//! | `weenie_properties_anim_part` | `animation_Id` | a GfxObj |
//! | `weenie_properties_palette` | `sub_Palette_Id` | a palette |
//! | `weenie_properties_texture_map` | `old_Id`, `new_Id` | an image texture |
//! | `landblock_instance`, `weenie_properties_position`, `weenie_properties_emote_action` | `obj_Cell_Id` | a cell: an outdoor cell's landblock (index 0 to `0x40`; 0 lets the server work the cell out), or the interior cell itself |
//!
//! Both of ACE's forms are read: its export (`INSERT INTO t (columns) VALUES ...`) and a database
//! dump (`INSERT INTO t VALUES ...`, its columns from the dump's own `CREATE TABLE`). A value that
//! is not a number (a variable, `NULL`) and the id 0 name nothing.

use std::collections::BTreeMap;
use std::path::Path;

use dereth_primitives::DataId;

use crate::dats::DatSet;
use crate::error::Loc;

/// What a column's id must be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expect {
    /// A portal-file record whose id starts with this byte, named for the report.
    Record(u8, &'static str),
    /// A cell of the world: an outdoor cell's landblock, or the interior cell itself.
    Cell,
}

/// One id the SQL names, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Named {
    pub id: u32,
    pub expect: Expect,
    pub at: Loc,
    /// The table and column, as the report names them.
    pub what: String,
}

/// The record a `weenie_properties_d_i_d` row's `value` names, by its `type`; `None` for a type
/// whose value is no record (a motion command, a spell, a treasure type, a class id).
#[must_use]
pub const fn did_kind(did_type: u32) -> Option<(u8, &'static str)> {
    Some(match did_type {
        1 => (0x02, "setup"),
        2 => (0x09, "motion table"),
        3 => (0x20, "sound table"),
        4 => (0x30, "combat table"),
        5 => (0x0E, "quality filter"),
        6 | 15..=17 => (0x04, "palette"),
        7 => (0x10, "clothing"),
        8 | 50..=52 => (0x06, "icon"),
        9..=14 => (0x05, "image texture"),
        18 => (0x01, "GfxObj"),
        22 => (0x34, "physics effect table"),
        _ => return None,
    })
}

/// The columns read in `table`, with what each must name; `type` is the DID row's type.
fn expectations(table: &str, column: &str, did_type: Option<u32>) -> Option<Expect> {
    match (table, column) {
        ("weenie_properties_d_i_d", "value") => did_type
            .and_then(did_kind)
            .map(|(b, n)| Expect::Record(b, n)),
        ("weenie_properties_anim_part", "animation_Id") => Some(Expect::Record(0x01, "GfxObj")),
        ("weenie_properties_palette", "sub_Palette_Id") => Some(Expect::Record(0x04, "palette")),
        ("weenie_properties_texture_map", "old_Id" | "new_Id") => {
            Some(Expect::Record(0x05, "image texture"))
        }
        (
            "landblock_instance" | "weenie_properties_position" | "weenie_properties_emote_action",
            "obj_Cell_Id",
        ) => Some(Expect::Cell),
        _ => None,
    }
}

/// A scanner over SQL text that skips comments and keeps the line.
struct Scan<'a> {
    text: &'a str,
    at: usize,
}

impl<'a> Scan<'a> {
    fn rest(&self) -> &'a str {
        &self.text[self.at..]
    }

    /// Skip white space and comments.
    fn skip(&mut self) {
        loop {
            let r = self.rest();
            let trimmed = r.trim_start();
            self.at += r.len() - trimmed.len();
            if let Some(after) = trimmed.strip_prefix("/*") {
                self.at += 2 + after.find("*/").map_or(after.len(), |i| i + 2);
            } else if trimmed.starts_with("--") || trimmed.starts_with('#') {
                self.at += trimmed.find('\n').unwrap_or(trimmed.len());
            } else {
                return;
            }
        }
    }

    /// The next word, a backquoted name, or one punctuation character.
    fn token(&mut self) -> Option<&'a str> {
        self.skip();
        let r = self.rest();
        let c = r.chars().next()?;
        let len = if c == '`' {
            r[1..].find('`').map_or(r.len(), |i| i + 2)
        } else if c.is_ascii_alphanumeric() || c == '_' || c == '@' {
            r.find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '@'))
                .unwrap_or(r.len())
        } else {
            c.len_utf8()
        };
        let t = &r[..len];
        self.at += len;
        Some(t)
    }

    /// One value of a tuple, as written, up to the `,` or `)` that ends it (not taken).
    fn value(&mut self) -> &'a str {
        self.skip();
        let start = self.at;
        let mut depth = 0i32;
        let bytes = self.text.as_bytes();
        while self.at < bytes.len() {
            match bytes[self.at] {
                b'\'' | b'"' => {
                    let q = bytes[self.at];
                    self.at += 1;
                    while self.at < bytes.len() {
                        if bytes[self.at] == b'\\' {
                            self.at += 2;
                            continue;
                        }
                        if bytes[self.at] == q {
                            if bytes.get(self.at + 1) == Some(&q) {
                                self.at += 2;
                                continue;
                            }
                            break;
                        }
                        self.at += 1;
                    }
                    self.at += 1;
                }
                b'/' if bytes.get(self.at + 1) == Some(&b'*') => {
                    let after = &self.text[self.at + 2..];
                    self.at += 2 + after.find("*/").map_or(after.len(), |i| i + 2);
                }
                b'(' => {
                    depth += 1;
                    self.at += 1;
                }
                b')' if depth > 0 => {
                    depth -= 1;
                    self.at += 1;
                }
                b',' | b')' if depth == 0 => break,
                _ => self.at += 1,
            }
        }
        self.text[start..self.at.min(bytes.len())].trim()
    }
}

/// A value as an id: decimal or `0x` hexadecimal.
pub fn number(v: &str) -> Option<u32> {
    let v = v.trim();
    if let Some(h) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        return u32::from_str_radix(h, 16).ok();
    }
    v.parse::<u32>().ok()
}

fn unquote(name: &str) -> &str {
    name.trim_matches('`')
}

/// Line numbers of offsets met in increasing order, counted once.
pub(crate) struct Lines<'a> {
    text: &'a [u8],
    offset: usize,
    line: usize,
}

impl<'a> Lines<'a> {
    /// The lines of `text`, from its first.
    pub(crate) fn new(text: &'a str) -> Self {
        Self {
            text: text.as_bytes(),
            offset: 0,
            line: 1,
        }
    }

    /// The line byte `offset` is on.
    pub(crate) fn at(&mut self, offset: usize) -> usize {
        if offset < self.offset {
            self.offset = 0;
            self.line = 1;
        }
        let end = offset.min(self.text.len());
        self.line += self.text[self.offset..end]
            .iter()
            .filter(|b| **b == b'\n')
            .count();
        self.offset = end;
        self.line
    }
}

/// One row of an `INSERT`: its table, the columns it fills and their values as written, and
/// where the row starts.
#[derive(Debug)]
pub struct Row<'a> {
    pub table: &'a str,
    pub columns: &'a [String],
    pub values: Vec<&'a str>,
    /// The byte the row's values start at.
    pub at: usize,
}

impl Row<'_> {
    /// The value of column `name`, as written.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.columns
            .iter()
            .position(|c| c == name)
            .and_then(|i| self.values.get(i))
            .copied()
    }
}

/// Every row of every `INSERT` in `text`, in order, handed to `each`. An insert that lists no
/// columns takes its table's from the text's own `CREATE TABLE`, and one with neither is
/// skipped.
pub fn rows(text: &str, mut each: impl FnMut(&Row<'_>)) {
    // Each table's columns, from the file's own `CREATE TABLE`s, for an insert that lists none.
    let mut tables: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut s = Scan { text, at: 0 };
    while let Some(t) = s.token() {
        if t.eq_ignore_ascii_case("CREATE") {
            if !s.token().is_some_and(|t| t.eq_ignore_ascii_case("TABLE")) {
                continue;
            }
            let Some(name) = s.token() else { break };
            let name = unquote(name).to_owned();
            if s.token() != Some("(") {
                continue;
            }
            // Each definition up to the closing parenthesis; a column's starts with its name.
            let mut columns = Vec::new();
            loop {
                let def = s.value();
                if let Some(first) = def.split_whitespace().next() {
                    if first.starts_with('`') {
                        columns.push(unquote(first).to_owned());
                    }
                }
                match s.token() {
                    Some(",") => {}
                    _ => break,
                }
            }
            tables.insert(name, columns);
        } else if t.eq_ignore_ascii_case("INSERT") {
            if !s.token().is_some_and(|t| t.eq_ignore_ascii_case("INTO")) {
                continue;
            }
            let Some(name) = s.token() else { break };
            let table = unquote(name);
            let mut next = s.token();
            let columns: Vec<String> = if next == Some("(") {
                let mut cols = Vec::new();
                loop {
                    let c = s.value();
                    cols.push(unquote(c).to_owned());
                    if s.token() != Some(",") {
                        break;
                    }
                }
                next = s.token();
                cols
            } else {
                tables.get(table).cloned().unwrap_or_default()
            };
            if !next.is_some_and(|t| t.eq_ignore_ascii_case("VALUES")) {
                continue;
            }
            loop {
                s.skip();
                if s.token() != Some("(") {
                    break;
                }
                let at = s.at;
                let mut values = Vec::new();
                loop {
                    values.push(s.value());
                    if s.token() != Some(",") {
                        break;
                    }
                }
                if !columns.is_empty() {
                    each(&Row {
                        table,
                        columns: &columns,
                        values,
                        at,
                    });
                }
                if s.token() != Some(",") {
                    break;
                }
            }
        }
    }
}

/// Every id `text` (the SQL file `path`) names in the columns this module reads.
#[must_use]
pub fn named_ids(path: &Path, text: &str) -> Vec<Named> {
    let mut out = Vec::new();
    let mut lines = Lines {
        text: text.as_bytes(),
        offset: 0,
        line: 1,
    };
    rows(text, |row| {
        let table = row.table;
        if !row
            .columns
            .iter()
            .any(|c| expectations(table, c, Some(1)).is_some())
        {
            return;
        }
        let did_type = row.get("type").and_then(number);
        for (i, c) in row.columns.iter().enumerate() {
            let Some(expect) = expectations(table, c, did_type) else {
                continue;
            };
            let Some(id) = row.values.get(i).copied().and_then(number) else {
                continue;
            };
            if id == 0 {
                continue;
            }
            let what = match (table, did_type) {
                ("weenie_properties_d_i_d", Some(ty)) => format!("{table} type {ty}"),
                _ => format!("{table} {c}"),
            };
            out.push(Named {
                id,
                expect,
                at: Loc::new(path, lines.at(row.at)),
                what,
            });
        }
    });
    out
}

/// A value as a float.
fn float(v: &str) -> Option<f32> {
    v.trim().parse::<f32>().ok()
}

/// One placed instance (`landblock_instance`), as SQL writes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    /// Its guid as written: a number or a `{{guid:...}}` token.
    pub guid: String,
    /// Its weenie as written: a class id or a `{{wcid:...}}` token.
    pub weenie: String,
    /// Its cell; for a pack's new cell, named by a `{{cell:...}}` token, its landblock alone.
    pub cell: u32,
    /// A pack's new cell's symbol, as its `{{cell:...}}` token writes it.
    pub cell_symbol: Option<String>,
    /// Cell-local meters.
    pub origin: [f32; 3],
    /// `w, x, y, z`.
    pub angles: [f32; 4],
    /// Whether a generator links it (`is_Link_Child`).
    pub link_child: bool,
    /// The line its values start on.
    pub line: usize,
}

/// The symbol a `{{cell:...}}` token names, as written: `None` for anything else.
#[must_use]
pub fn cell_token(v: &str) -> Option<String> {
    let inner = v.trim().strip_prefix("{{")?.strip_suffix("}}")?;
    let (kind, symbol) = inner.split_once(':')?;
    (kind.trim() == "cell").then(|| symbol.trim().to_owned())
}

/// One instance's row as `text` reads it, or `None` for a row that is not one.
fn instance_of(row: &Row<'_>, line: usize) -> Option<Instance> {
    if row.table != "landblock_instance" {
        return None;
    }
    let f = |c: &str| row.get(c).and_then(float).unwrap_or(0.0);
    let (Some(guid), Some(weenie), Some(cell_text)) = (
        row.get("guid"),
        row.get("weenie_Class_Id"),
        row.get("obj_Cell_Id"),
    ) else {
        return None;
    };
    let (cell, cell_symbol) = match number(cell_text) {
        Some(c) => (c, None),
        None => {
            let symbol = cell_token(cell_text)?;
            let lb = crate::symbol::instance_landblock(&symbol).ok()??;
            (u32::from(lb) << 16, Some(symbol))
        }
    };
    Some(Instance {
        guid: guid.to_owned(),
        weenie: weenie.to_owned(),
        cell,
        cell_symbol,
        origin: [f("origin_X"), f("origin_Y"), f("origin_Z")],
        angles: [f("angles_W"), f("angles_X"), f("angles_Y"), f("angles_Z")],
        link_child: row.get("is_Link_Child").is_some_and(truth),
        line,
    })
}

/// What a server's SQL (an export or a whole dump) holds of its world, read in one pass: each
/// weenie by class id, each instance it places, and each generator's links to the instances it
/// owns (parent guid, child guid).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerContent {
    pub weenies: BTreeMap<u32, WeenieFacts>,
    pub instances: Vec<Instance>,
    pub links: Vec<(u32, u32)>,
    /// The quests it registers, by name.
    pub quests: BTreeMap<String, Quest>,
    /// The death treasures, by their treasure type (what a weenie's data id 35 names).
    pub treasure: BTreeMap<u32, Treasure>,
    /// Each house weenie's (type 53) property rows: what a copy of it takes.
    pub houses: BTreeMap<u32, Vec<HouseRow>>,
}

/// One property row of a weenie: its table without `weenie_properties_` (`int`, `d_i_d`, ...),
/// its type and its value as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HouseRow {
    pub table: String,
    pub kind: u32,
    pub value: String,
}

/// The weenie type of a house.
pub const HOUSE_TYPE: u32 = 53;

/// The property tables of one value each that a house's rows are kept from.
const HOUSE_TABLES: [&str; 7] = [
    "weenie_properties_int",
    "weenie_properties_int64",
    "weenie_properties_bool",
    "weenie_properties_float",
    "weenie_properties_string",
    "weenie_properties_d_i_d",
    "weenie_properties_i_i_d",
];

/// A registered quest: how long before it can be solved again (seconds), how many times it can
/// be (-1: without end), and its message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Quest {
    pub min_delta: u32,
    pub max_solves: i32,
    pub message: String,
}

/// A death treasure: its tier, and how often and how many of each kind of item it drops.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Treasure {
    pub tier: i32,
    pub quality: f32,
    /// Chance (percent), least and most of items, magic items and mundane items.
    pub items: (i32, i32, i32),
    pub magic: (i32, i32, i32),
    pub mundane: (i32, i32, i32),
}

/// A quest of `row` (a `quest` row), with its name.
pub fn quest_of(row: &Row<'_>) -> Option<(String, Quest)> {
    if row.table != "quest" {
        return None;
    }
    let name = unquote_string(row.get("name")?);
    Some((
        name,
        Quest {
            min_delta: row.get("min_Delta").and_then(number).unwrap_or(0),
            max_solves: row
                .get("max_Solves")
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(-1),
            message: row.get("message").map(unquote_string).unwrap_or_default(),
        },
    ))
}

/// The quests `text` registers.
#[must_use]
pub fn quests_in(text: &str) -> BTreeMap<String, Quest> {
    let mut out = BTreeMap::new();
    rows(text, |row| out.extend(quest_of(row)));
    out
}

/// [`ServerContent`] of `text`.
#[must_use]
pub fn server_content(text: &str) -> ServerContent {
    let mut facts: BTreeMap<String, WeenieFacts> = BTreeMap::new();
    let mut out = ServerContent::default();
    rows(text, |row| {
        if let Some(i) = instance_of(row, 0) {
            out.instances.push(i);
            return;
        }
        if let Some((n, q)) = quest_of(row) {
            out.quests.insert(n, q);
            return;
        }
        if row.table == "treasure_death" {
            let i = |c: &str| {
                row.get(c)
                    .and_then(|v| v.trim().parse::<i32>().ok())
                    .unwrap_or(0)
            };
            if let Some(t) = row.get("treasure_Type").and_then(number) {
                out.treasure.insert(
                    t,
                    Treasure {
                        tier: i("tier"),
                        quality: row.get("loot_Quality_Mod").and_then(float).unwrap_or(0.0),
                        items: (i("item_Chance"), i("item_Min_Amount"), i("item_Max_Amount")),
                        magic: (
                            i("magic_Item_Chance"),
                            i("magic_Item_Min_Amount"),
                            i("magic_Item_Max_Amount"),
                        ),
                        mundane: (
                            i("mundane_Item_Chance"),
                            i("mundane_Item_Min_Amount"),
                            i("mundane_Item_Max_Amount"),
                        ),
                    },
                );
            }
            return;
        }
        if row.table == "landblock_instance_link" {
            let g = |c: &str| {
                row.get(c)
                    .or_else(|| row.get(&c.replace("GUID", "Guid")))
                    .and_then(number)
            };
            if let (Some(p), Some(c)) = (g("parent_GUID"), g("child_GUID")) {
                out.links.push((p, c));
            }
            return;
        }
        // A house's property rows: its `weenie` row, which says it is one, comes first.
        if HOUSE_TABLES.contains(&row.table) {
            let w = row.get("object_Id");
            if w.and_then(|w| facts.get(w)).and_then(|f| f.kind) == Some(HOUSE_TYPE) {
                if let (Some(w), Some(kind), Some(value)) = (
                    w.and_then(number),
                    row.get("type").and_then(number),
                    row.get("value"),
                ) {
                    let table = &row.table["weenie_properties_".len()..];
                    // A dump writes a bool as a bit; an export as a word.
                    let value = if table == "bool" {
                        if truth(value) { "True" } else { "False" }.to_owned()
                    } else {
                        value.trim().to_owned()
                    };
                    out.houses.entry(w).or_default().push(HouseRow {
                        table: table.to_owned(),
                        kind,
                        value,
                    });
                }
            }
        }
        fact(&mut facts, row);
    });
    out.weenies = facts
        .into_iter()
        .filter_map(|(w, f)| Some((number(&w)?, f)))
        .collect();
    out
}

/// The guids of the instances `text` deletes one by one (`DELETE FROM landblock_instance WHERE
/// guid = ...`), as written.
#[must_use]
pub fn deleted_instances(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for statement in text.split(';') {
        let s = statement
            .lines()
            .filter(|l| !l.trim_start().starts_with("--"))
            .collect::<Vec<_>>()
            .join(" ");
        let flat: String = s.replace('`', "");
        let lower = flat.to_ascii_lowercase();
        let Some(at) = lower.find("delete from landblock_instance where guid") else {
            continue;
        };
        let rest = &flat[at + "delete from landblock_instance where guid".len()..];
        if let Some(v) = rest.trim().strip_prefix('=') {
            let v = v.trim();
            if !v.is_empty() {
                out.push(v.to_owned());
            }
        }
    }
    out
}

/// Every instance `text` places.
#[must_use]
pub fn instances(text: &str) -> Vec<Instance> {
    let mut out = Vec::new();
    let mut lines = Lines {
        text: text.as_bytes(),
        offset: 0,
        line: 1,
    };
    rows(text, |row| {
        if row.table == "landblock_instance" {
            let line = lines.at(row.at);
            out.extend(instance_of(row, line));
        }
    });
    out
}

/// The setup each weenie of `text` names (`weenie_properties_d_i_d`, type 1): the weenie as
/// written (a class id or a `{{wcid:...}}` token) and the setup's id.
#[must_use]
pub fn weenie_setups(text: &str) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    rows(text, |row| {
        if row.table != "weenie_properties_d_i_d" || row.get("type").and_then(number) != Some(1) {
            return;
        }
        if let (Some(w), Some(v)) = (row.get("object_Id"), row.get("value").and_then(number)) {
            out.push((w.to_owned(), v));
        }
    });
    out
}

/// The name each weenie of `text` is given (`weenie_properties_string`, type 1): the weenie as
/// written and its name, unquoted.
#[must_use]
pub fn weenie_names(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    rows(text, |row| {
        if row.table != "weenie_properties_string" || row.get("type").and_then(number) != Some(1) {
            return;
        }
        if let (Some(w), Some(v)) = (row.get("object_Id"), row.get("value")) {
            out.push((w.to_owned(), unquote_string(v)));
        }
    });
    out
}

/// What a server's SQL says of one weenie: its name, the setup it is drawn with, its weenie type,
/// and whether it is stuck in place and whether it can be attacked, when the SQL says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeenieFacts {
    pub name: Option<String>,
    pub setup: Option<u32>,
    /// The `weenie` row's `type`.
    pub kind: Option<u32>,
    /// Bool property 1.
    pub stuck: Option<bool>,
    /// Bool property 19.
    pub attackable: Option<bool>,
    /// Float property 43: how far around it a generator scatters what it makes.
    pub generator_radius: Option<f32>,
    /// Its destination's cell (position 2): where a portal sends who uses it.
    pub destination: Option<u32>,
    /// Its destination's place in that cell's landblock, in meters.
    pub destination_at: Option<[f32; 3]>,
}

/// A bool property's value as written: `0`, `false` and a dump's zero bit are false.
fn truth(v: &str) -> bool {
    let v = v.trim();
    !(v == "0" || v.eq_ignore_ascii_case("false") || v == "'\\0'" || v == "'\0'" || v == "b'0'")
}

/// What `text` says of each weenie it holds, by the weenie as written, read in one pass.
#[must_use]
pub fn weenie_facts(text: &str) -> BTreeMap<String, WeenieFacts> {
    let mut out: BTreeMap<String, WeenieFacts> = BTreeMap::new();
    rows(text, |row| fact(&mut out, row));
    out
}

/// What `row` says of its weenie, added to `out`.
fn fact(out: &mut BTreeMap<String, WeenieFacts>, row: &Row<'_>) {
    {
        let key = match row.table {
            "weenie" => "class_Id",
            "weenie_properties_string"
            | "weenie_properties_d_i_d"
            | "weenie_properties_bool"
            | "weenie_properties_float"
            | "weenie_properties_position" => "object_Id",
            _ => return,
        };
        let Some(w) = row.get(key) else { return };
        let e = out.entry(w.to_owned()).or_default();
        let ty = row.get("type").and_then(number);
        let value = row.get("value");
        match (row.table, ty) {
            ("weenie", t) => e.kind = t,
            ("weenie_properties_string", Some(1)) => e.name = value.map(unquote_string),
            ("weenie_properties_d_i_d", Some(1)) => e.setup = value.and_then(number),
            ("weenie_properties_bool", Some(1)) => e.stuck = value.map(truth),
            ("weenie_properties_bool", Some(19)) => e.attackable = value.map(truth),
            ("weenie_properties_float", Some(43)) => e.generator_radius = value.and_then(float),
            ("weenie_properties_position", _)
                if row.get("position_Type").and_then(number) == Some(2) =>
            {
                e.destination = row.get("obj_Cell_Id").and_then(number);
                let at = |c: &str| row.get(c).and_then(float);
                e.destination_at = match (at("origin_X"), at("origin_Y"), at("origin_Z")) {
                    (Some(x), Some(y), Some(z)) => Some([x, y, z]),
                    _ => None,
                };
            }
            _ => {}
        }
    }
}

/// A quoted SQL string's text: the quotes taken off, `\'`, `''` and `\\` read as the one
/// character.
fn unquote_string(v: &str) -> String {
    let v = v.trim();
    let Some(inner) = v
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .or_else(|| v.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
    else {
        return v.to_owned();
    };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            }
            '\'' if chars.peek() == Some(&'\'') => {
                chars.next();
                out.push('\'');
            }
            _ => out.push(c),
        }
    }
    out
}

/// Where in `text` each value of the instance whose guid is written `guid` lies, by column: what
/// an edit of the instance replaces. `None` when `text` places no such instance.
#[must_use]
pub fn instance_values(text: &str, guid: &str) -> Option<BTreeMap<String, std::ops::Range<usize>>> {
    let mut found = None;
    rows(text, |row| {
        if found.is_some() || row.table != "landblock_instance" || row.get("guid") != Some(guid) {
            return;
        }
        let spans = row
            .columns
            .iter()
            .zip(&row.values)
            .map(|(c, v)| {
                let start = v.as_ptr() as usize - text.as_ptr() as usize;
                (c.clone(), start..start + v.len())
            })
            .collect();
        found = Some(spans);
    });
    found
}

/// Whether `set` holds the cell `id`: an outdoor cell's landblock, or the interior cell.
fn holds_cell(set: &DatSet, id: u32) -> bool {
    let (lb, index) = (id >> 16, id & 0xFFFF);
    if (lb >> 8) == 0xFF || (lb & 0xFF) == 0xFF {
        return false;
    }
    // Index 0 is an outdoor place whose cell the server works out from the coordinates.
    match index {
        0..=0x40 => set.cell.contains(DataId((lb << 16) | 0xFFFF)),
        0x0100..=0xFFFD => set.cell.contains(DataId(id)),
        _ => false,
    }
}

/// One line for each id in `named` that `set` does not hold as its column means.
#[must_use]
pub fn check(set: &DatSet, named: &[Named]) -> Vec<String> {
    let mut out = Vec::new();
    for n in named {
        match n.expect {
            Expect::Record(byte, kind) => {
                #[allow(clippy::cast_possible_truncation)] // the id's top byte
                let top = (n.id >> 24) as u8;
                if top != byte {
                    out.push(format!(
                        "{}: {} names {:#010X}, which is not a {kind} ({byte:#04X}...)",
                        n.at, n.what, n.id
                    ));
                } else if !set.has_portal(DataId(n.id)) {
                    out.push(format!(
                        "{}: {} names {kind} {:#010X}, which the files do not hold",
                        n.at, n.what, n.id
                    ));
                }
            }
            Expect::Cell => {
                if !holds_cell(set, n.id) {
                    out.push(format!(
                        "{}: {} names cell {:#010X}, which the files do not hold",
                        n.at, n.what, n.id
                    ));
                }
            }
        }
    }
    out
}
