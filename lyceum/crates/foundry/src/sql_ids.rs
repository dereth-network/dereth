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

/// A value as an id: decimal or `0x` hexadecimal, a comment after it (`22 /* StampQuest */`, as
/// an export writes an emote's category and type) not read.
pub fn number(v: &str) -> Option<u32> {
    let v = v.split("/*").next().unwrap_or(v).trim();
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
    /// The wielded treasures, by their treasure type (what a weenie's data id 32 names), each
    /// one's rows in the order the server reads them.
    pub wielded: BTreeMap<u32, Vec<WieldedItem>>,
    /// Who names each loot table, by the table's id: each weenie whose data ids or generator name
    /// it, and how, in the order the SQL has them.
    pub loot_uses: BTreeMap<u32, Vec<(u32, LootUse)>>,
    /// Each house weenie's (type 53) property rows: what a copy of it takes.
    pub houses: BTreeMap<u32, Vec<HouseRow>>,
    /// Who names each class, by the class named: each weenie whose create list, generator or
    /// emotes name it, and how, in the order the SQL has them.
    pub uses: BTreeMap<u32, Vec<(u32, Use)>>,
    /// Each body part's typical row, by its part (`key`): the one the most weenies share, every
    /// column but its own id and its weenie's, as written.
    pub body_parts: BTreeMap<u32, Vec<(String, String)>>,
    /// Who names each quest, by the quest as registered ([`crate::content_check::quest_key`]).
    pub quest_uses: BTreeMap<String, QuestNamers>,
}

/// Who names one quest among a server's weenies: the quest as first written (before any `@`),
/// and each weenie whose emotes or properties name it, and how, in the order the SQL has them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuestNamers {
    pub name: String,
    pub by: Vec<(u32, QuestUse)>,
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
    /// Which heritages its clothing, armor and weapons are drawn for (`unknown_Chances`).
    pub heritage: i32,
    /// Which mix of things each kind of item is drawn from: items, magic items, mundane items
    /// (the `..._Type_Selection_Chances` columns).
    pub kinds: (i32, i32, i32),
}

/// The death treasure of a `treasure_death` row, with its treasure type.
#[must_use]
pub fn treasure_of(row: &Row<'_>) -> Option<(u32, Treasure)> {
    if row.table != "treasure_death" {
        return None;
    }
    let i = |c: &str| {
        row.get(c)
            .and_then(|v| crate::content::bare(v).parse::<i32>().ok())
            .unwrap_or(0)
    };
    let t = row.get("treasure_Type").and_then(number)?;
    Some((
        t,
        Treasure {
            tier: i("tier"),
            quality: row
                .get("loot_Quality_Mod")
                .and_then(|v| float(&crate::content::bare(v)))
                .unwrap_or(0.0),
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
            heritage: i("unknown_Chances"),
            kinds: (
                i("item_Treasure_Type_Selection_Chances"),
                i("magic_Item_Treasure_Type_Selection_Chances"),
                i("mundane_Item_Type_Selection_Chances"),
            ),
        },
    ))
}

/// The death treasures `text` writes, by treasure type.
#[must_use]
pub fn death_treasures_in(text: &str) -> BTreeMap<u32, Treasure> {
    let mut out = BTreeMap::new();
    rows(text, |row| out.extend(treasure_of(row)));
    out
}

/// One row of a wielded treasure: what it makes (as written: a class id or a pack weenie's
/// token), its color and shade, how many and by how much fewer at most, how likely, and how it
/// stands among the rows around it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WieldedItem {
    pub weenie: String,
    pub palette: u32,
    pub shade: f32,
    pub stack: i32,
    pub variance: f32,
    pub probability: f32,
    /// It starts a draw of its own (`set_Start`).
    pub set_start: bool,
    /// The rows after it are drawn only when it is made (`has_Sub_Set`).
    pub has_sub_set: bool,
    /// It follows the rows drawn after an entry before it (`continues_Previous_Set`).
    pub continues: bool,
    /// The columns the server reads nothing from, `unknown_1`, `_3`, `_4`, `_5`, `_9` to `_12`.
    pub unknown: [u32; 8],
}

/// The columns of a wielded treasure's row the server reads nothing from, in
/// [`WieldedItem::unknown`]'s order.
pub const WIELDED_UNKNOWN: [&str; 8] = [
    "unknown_1",
    "unknown_3",
    "unknown_4",
    "unknown_5",
    "unknown_9",
    "unknown_10",
    "unknown_11",
    "unknown_12",
];

/// A wielded treasure's row of a `treasure_wielded` row, with its treasure type.
#[must_use]
pub fn wielded_of(row: &Row<'_>) -> Option<(u32, WieldedItem)> {
    if row.table != "treasure_wielded" {
        return None;
    }
    let t = row.get("treasure_Type").and_then(number)?;
    let b = |c: &str| crate::content::bare(row.get(c).unwrap_or("0"));
    let f = |c: &str| float(&b(c)).unwrap_or(0.0);
    let mut unknown = [0u32; 8];
    for (u, c) in unknown.iter_mut().zip(WIELDED_UNKNOWN) {
        *u = number(&b(c)).unwrap_or(0);
    }
    Some((
        t,
        WieldedItem {
            weenie: b("weenie_Class_Id"),
            palette: number(&b("palette_Id")).unwrap_or(0),
            shade: f("shade"),
            stack: b("stack_Size").parse().unwrap_or(0),
            variance: f("stack_Size_Variance"),
            probability: f("probability"),
            set_start: row.get("set_Start").is_some_and(truth),
            has_sub_set: row.get("has_Sub_Set").is_some_and(truth),
            continues: row.get("continues_Previous_Set").is_some_and(truth),
            unknown,
        },
    ))
}

/// The wielded treasures `text` writes, by treasure type, each one's rows in order.
#[must_use]
pub fn wielded_in(text: &str) -> BTreeMap<u32, Vec<WieldedItem>> {
    let mut out: BTreeMap<u32, Vec<WieldedItem>> = BTreeMap::new();
    rows(text, |row| {
        if let Some((t, w)) = wielded_of(row) {
            out.entry(t).or_default().push(w);
        }
    });
    out
}

/// The id of every loot table `text` writes, death and wielded alike, each once, in the order
/// written.
#[must_use]
pub fn loot_ids(text: &str) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    rows(text, |row| {
        if matches!(row.table, "treasure_death" | "treasure_wielded") {
            if let Some(id) = row.get("treasure_Type").and_then(number) {
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
    });
    out
}

/// How a weenie names a loot table: the death treasure it drops (data id 35), the wielded
/// treasure it wields (data id 32), or a generator's entry that makes treasure (`where_Create`
/// with the treasure bit), with that entry's `where_Create`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LootUse {
    Dies,
    Wields,
    Generator { place: u32 },
}

/// The bit of a generator entry's `where_Create` that says it makes treasure from a loot table,
/// which its class id names.
pub const TREASURE_PLACE: u32 = 0x40;

/// A loot table a row names: the weenie whose row it is (as written), the table, and how.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LootRef {
    pub by: String,
    pub table: u32,
    pub how: LootUse,
}

/// The loot table `row` names, when it names one (0 names none).
fn loot_ref(row: &Row<'_>) -> Option<LootRef> {
    let by = || row.get("object_Id").map(|v| v.trim().to_owned());
    match row.table {
        "weenie_properties_d_i_d" => {
            let how = match row.get("type").and_then(number)? {
                35 => LootUse::Dies,
                32 => LootUse::Wields,
                _ => return None,
            };
            let table = row.get("value").and_then(number).filter(|t| *t != 0)?;
            Some(LootRef {
                by: by()?,
                table,
                how,
            })
        }
        "weenie_properties_generator" => {
            let place = row.get("where_Create").and_then(number)?;
            if place & TREASURE_PLACE == 0 {
                return None;
            }
            let table = row
                .get("weenie_Class_Id")
                .and_then(number)
                .filter(|t| *t != 0)?;
            Some(LootRef {
                by: by()?,
                table,
                how: LootUse::Generator { place },
            })
        }
        _ => None,
    }
}

/// Every loot table `text` names, with the weenie whose row names it, in order.
#[must_use]
pub fn loot_uses(text: &str) -> Vec<LootRef> {
    let mut out = Vec::new();
    rows(text, |row| out.extend(loot_ref(row)));
    out
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
    let mut scan = UseScan::default();
    let mut parts = PartCounts::default();
    let mut quests = QuestScan::default();
    let mut quest_uses = QuestUseScan::default();
    rows(text, |row| {
        quests.row(&mut facts, row);
        if let Some(q) = quest_uses.row(row) {
            if let Some(by) = number(&q.by) {
                let e = out
                    .quest_uses
                    .entry(crate::content_check::quest_key(&q.quest))
                    .or_insert_with(|| QuestNamers {
                        name: q.quest.split('@').next().unwrap_or("").trim().to_owned(),
                        by: Vec::new(),
                    });
                e.by.push((by, q.how));
            }
        }
        if let Some(i) = instance_of(row, 0) {
            out.instances.push(i);
            return;
        }
        if let Some((n, q)) = quest_of(row) {
            out.quests.insert(n, q);
            return;
        }
        if row.table == "treasure_death" {
            out.treasure.extend(treasure_of(row));
            return;
        }
        if row.table == "treasure_wielded" {
            if let Some((t, w)) = wielded_of(row) {
                out.wielded.entry(t).or_default().push(w);
            }
            return;
        }
        if let Some(l) = loot_ref(row) {
            if let Some(by) = number(&l.by) {
                out.loot_uses.entry(l.table).or_default().push((by, l.how));
            }
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
        if row.table == "weenie_properties_body_part" {
            parts.row(row);
        }
        if let Some(u) = scan.row(row) {
            if let (Some(by), Some(named)) = (number(&u.by), number(&u.names)) {
                out.uses.entry(named).or_default().push((by, u.how));
            }
        }
        fact(&mut facts, row);
    });
    out.weenies = facts
        .into_iter()
        .filter_map(|(w, f)| Some((number(&w)?, f)))
        .collect();
    out.body_parts = parts.typical();
    out
}

/// How a weenie names another class: a row of its create list (with the row's destination), a
/// profile of its generator, or its emotes (the set's category, and the action's type when an
/// action names it rather than the set).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    CreateList { destination: u32 },
    Generator,
    Emote { category: u32, action: Option<u32> },
}

/// A class a row names: the weenie whose row it is and the class, both as written, how, and the
/// byte the row's values start at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassUse {
    pub by: String,
    pub names: String,
    pub how: Use,
    pub at: usize,
}

/// The rows that name classes, read in order: an emote action's weenie and category are its
/// set's, found by the set's row id in a dump, else the set written last (an export's
/// `@parent_id`).
#[derive(Default)]
struct UseScan {
    sets: BTreeMap<u32, (String, u32)>,
    last: Option<(String, u32)>,
}

impl UseScan {
    /// The class `row` names, when it names one (class 0 and `NULL` name none).
    fn row(&mut self, row: &Row<'_>) -> Option<ClassUse> {
        let named = |c: &str| {
            row.get(c)
                .map(str::trim)
                .filter(|v| !v.eq_ignore_ascii_case("NULL") && number(v) != Some(0))
                .map(str::to_owned)
        };
        let at = row.at;
        match row.table {
            "weenie_properties_create_list" => Some(ClassUse {
                by: row.get("object_Id")?.trim().to_owned(),
                names: named("weenie_Class_Id")?,
                how: Use::CreateList {
                    destination: row.get("destination_Type").and_then(number).unwrap_or(0),
                },
                at,
            }),
            // An entry that makes treasure names a loot table, not a class.
            "weenie_properties_generator"
                if row
                    .get("where_Create")
                    .and_then(number)
                    .is_none_or(|p| p & TREASURE_PLACE == 0) =>
            {
                Some(ClassUse {
                    by: row.get("object_Id")?.trim().to_owned(),
                    names: named("weenie_Class_Id")?,
                    how: Use::Generator,
                    at,
                })
            }
            "weenie_properties_emote" => {
                let by = row.get("object_Id")?.trim().to_owned();
                let category = row.get("category").and_then(number).unwrap_or(0);
                if let Some(id) = row.get("id").and_then(number) {
                    self.sets.insert(id, (by.clone(), category));
                }
                self.last = Some((by.clone(), category));
                Some(ClassUse {
                    by,
                    names: named("weenie_Class_Id")?,
                    how: Use::Emote {
                        category,
                        action: None,
                    },
                    at,
                })
            }
            crate::content::EMOTE_ACTIONS => {
                let names = named("weenie_Class_Id")?;
                let set = match row.get("emote_Id").and_then(number) {
                    Some(id) => self.sets.get(&id).cloned(),
                    None => self.last.clone(),
                };
                let (by, category) = set?;
                Some(ClassUse {
                    by,
                    names,
                    how: Use::Emote {
                        category,
                        action: row.get("type").and_then(number),
                    },
                    at,
                })
            }
            _ => None,
        }
    }
}

/// Every class `text` names in create lists, generator profiles and emotes, with the weenie
/// whose row names it, in order.
#[must_use]
pub fn class_uses(text: &str) -> Vec<ClassUse> {
    let mut scan = UseScan::default();
    let mut out = Vec::new();
    rows(text, |row| out.extend(scan.row(row)));
    out
}

/// The emote categories a test of a quest answers with, its set's `quest` the test's message:
/// done, not done, and not in a fellowship.
pub const QUEST_ANSWERS: [u32; 3] = [12, 13, 30];

/// The string properties that name a quest: 33, stamped on whoever picks it up or opens it; 37,
/// the quest needed to use it (a portal's way through); 45, 49 and 50, a kill of it counting
/// toward it.
pub const QUEST_PROPERTIES: [u32; 5] = [33, 37, 45, 49, 50];

/// How a weenie names a quest: an emote action of its (the action's type, in a set of
/// `category`), an emote set that answers a test of it, or a string property.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QuestUse {
    Action { category: u32, action: u32 },
    Answers { category: u32 },
    Property(u32),
}

/// What a use of a quest does with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QuestDoes {
    /// Stamps it as done (when it can be done again, for an update).
    Stamps,
    /// Asks whether it is done, how often, or its bits.
    Tests,
    Erases,
    /// Adds to it, takes from it, sets how often it is done, or sets its bits.
    Changes,
    /// Runs when a test of it is answered.
    Answers,
    /// Lets only those who have it use it.
    Needs,
    /// Counts a kill of it toward it.
    Kills,
    /// Stamps it on whoever picks it up or opens it.
    StampsWhenTaken,
}

impl QuestUse {
    /// What it does with the quest.
    #[must_use]
    pub const fn does(self) -> QuestDoes {
        match self {
            Self::Action { action, .. } => match action {
                20 | 22 | 60 | 61 | 79 | 81 => QuestDoes::Stamps,
                31 | 83 => QuestDoes::Erases,
                21 | 30 | 58 | 80 | 82 | 102..=105 => QuestDoes::Tests,
                _ => QuestDoes::Changes,
            },
            Self::Answers { .. } => QuestDoes::Answers,
            Self::Property(37) => QuestDoes::Needs,
            Self::Property(33) => QuestDoes::StampsWhenTaken,
            Self::Property(_) => QuestDoes::Kills,
        }
    }
}

/// A quest a row names: the weenie whose row it is as written, the quest as written (unquoted,
/// with any `@` suffix), and how.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestRef {
    pub by: String,
    pub quest: String,
    pub how: QuestUse,
}

/// The rows that name quests, read in order: an emote action's weenie and category are its set's,
/// found by the set's row id in a dump, else the set written last (an export's `@parent_id`).
#[derive(Default)]
struct QuestUseScan {
    sets: BTreeMap<u32, (String, u32)>,
    last: Option<(String, u32)>,
}

impl QuestUseScan {
    /// The quest `row` names, when it names one.
    fn row(&mut self, row: &Row<'_>) -> Option<QuestRef> {
        let named = |c: &str| {
            row.get(c)
                .map(unquote_string)
                .filter(|q| !q.trim().is_empty() && !q.trim().eq_ignore_ascii_case("NULL"))
        };
        match row.table {
            "weenie_properties_emote" => {
                let by = row.get("object_Id")?.trim().to_owned();
                let category = row.get("category").and_then(number).unwrap_or(0);
                if let Some(id) = row.get("id").and_then(number) {
                    self.sets.insert(id, (by.clone(), category));
                }
                self.last = Some((by.clone(), category));
                if !QUEST_ANSWERS.contains(&category) {
                    return None;
                }
                Some(QuestRef {
                    by,
                    quest: named("quest")?,
                    how: QuestUse::Answers { category },
                })
            }
            crate::content::EMOTE_ACTIONS => {
                let action = row.get("type").and_then(number)?;
                if !crate::content_check::QUEST_ACTIONS.contains(&action) {
                    return None;
                }
                let quest = named("message")?;
                let set = match row.get("emote_Id").and_then(number) {
                    Some(id) => self.sets.get(&id).cloned(),
                    None => self.last.clone(),
                };
                let (by, category) = set?;
                Some(QuestRef {
                    by,
                    quest,
                    how: QuestUse::Action { category, action },
                })
            }
            "weenie_properties_string" => {
                let ty = row.get("type").and_then(number)?;
                if !QUEST_PROPERTIES.contains(&ty) {
                    return None;
                }
                Some(QuestRef {
                    by: row.get("object_Id")?.trim().to_owned(),
                    quest: named("value")?,
                    how: QuestUse::Property(ty),
                })
            }
            _ => None,
        }
    }
}

/// Every quest `text` names in emotes and string properties, with the weenie whose row names it,
/// in order.
#[must_use]
pub fn quest_uses(text: &str) -> Vec<QuestRef> {
    let mut scan = QuestUseScan::default();
    let mut out = Vec::new();
    rows(text, |row| out.extend(scan.row(row)));
    out
}

/// A row's columns and values, as written.
type Cells = Vec<(String, String)>;

/// The body parts' rows, counted by part: how many weenies have each row exactly.
#[derive(Default)]
struct PartCounts {
    /// By part, each row met: when it was first met, and how many have it.
    rows: BTreeMap<u32, BTreeMap<Cells, (usize, usize)>>,
    met: usize,
}

impl PartCounts {
    fn row(&mut self, row: &Row<'_>) {
        let Some(key) = row.get("key").and_then(number) else {
            return;
        };
        let cells: Cells = row
            .columns
            .iter()
            .zip(&row.values)
            .filter(|(c, _)| !matches!(c.as_str(), "id" | "object_Id"))
            .map(|(c, v)| (c.clone(), v.trim().to_owned()))
            .collect();
        self.met += 1;
        let met = self.met;
        self.rows
            .entry(key)
            .or_default()
            .entry(cells)
            .or_insert((met, 0))
            .1 += 1;
    }

    /// Each part's commonest row, the first met of those as common.
    fn typical(self) -> BTreeMap<u32, Cells> {
        self.rows
            .into_iter()
            .filter_map(|(k, seen)| {
                seen.into_iter()
                    .max_by(|(_, (a_at, a_n)), (_, (b_at, b_n))| a_n.cmp(b_n).then(b_at.cmp(a_at)))
                    .map(|(r, _)| (k, r))
            })
            .collect()
    }
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

/// What a server's SQL says of one weenie: its name, the setup it is drawn with and its icon, its
/// weenie type, and whether it is stuck in place and whether it can be attacked, when the SQL says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeenieFacts {
    pub name: Option<String>,
    pub setup: Option<u32>,
    /// Data id 8: the picture of it in a pack.
    pub icon: Option<u32>,
    /// Data id 7: the clothing table it is dressed in.
    pub clothing_base: Option<u32>,
    /// Data ids 2, 3 and 6: its motion table, its sound table and its base palette.
    pub motion_table: Option<u32>,
    pub sound_table: Option<u32>,
    pub palette_base: Option<u32>,
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
    /// Int property 25: a creature's level.
    pub level: Option<i32>,
    /// Data id 35: the death treasure it drops.
    pub death_treasure: Option<u32>,
    /// Data id 32: the wielded treasure it is made with.
    pub wielded_treasure: Option<u32>,
    /// Whether it makes things: it has generator rows, or int property 81 (how many it keeps at
    /// most).
    pub generates: bool,
    /// Int properties 5 and 13: its burden, and for a stackable one the burden of each of a
    /// stack.
    pub burden: Option<i32>,
    pub unit_burden: Option<i32>,
    /// Int property 11: the most of it one stack holds.
    pub max_stack: Option<i32>,
    /// Int property 33: what becomes of it when whoever has it dies (-2: it is destroyed).
    pub bonded: Option<i32>,
    /// Int property 6: for a pack, how many things it holds.
    pub holds: Option<i32>,
    /// Int property 9: where it may be worn or wielded.
    pub locations: Option<u32>,
    /// Its four wield requirements, each what it is, of what, and how much (int properties 158 to
    /// 160, then 270 to 278); 0 where it has none.
    pub wield: [i32; 12],
    /// Int64 property 3: what an augmentation gem costs to use, in experience.
    pub augmentation_cost: Option<i64>,
    /// String property 37: the quest a portal lets only those who have it through.
    pub quest_restriction: Option<String>,
    /// The quests its emotes ask whether a player has (or has solved some number of times), as
    /// the actions name them.
    pub tests_quests: Vec<String>,
}

/// The emote actions that ask after a quest of the player's: whether it has it, how many times
/// it solved it, and its bits.
pub const QUEST_TESTS: [u32; 4] = [21, 30, 102, 103];

/// The quests each weenie's emotes test, read in order: an action's weenie is its set's, found
/// by the set's row id in a dump, else the set written last (an export's `@parent_id`).
#[derive(Default)]
struct QuestScan {
    sets: BTreeMap<u32, String>,
    last: Option<String>,
}

impl QuestScan {
    fn row(&mut self, out: &mut BTreeMap<String, WeenieFacts>, row: &Row<'_>) {
        match row.table {
            "weenie_properties_emote" => {
                let Some(by) = row.get("object_Id").map(|v| v.trim().to_owned()) else {
                    return;
                };
                if let Some(id) = row.get("id").and_then(number) {
                    self.sets.insert(id, by.clone());
                }
                self.last = Some(by);
            }
            crate::content::EMOTE_ACTIONS => {
                if !row
                    .get("type")
                    .and_then(number)
                    .is_some_and(|t| QUEST_TESTS.contains(&t))
                {
                    return;
                }
                let by = match row.get("emote_Id").and_then(number) {
                    Some(id) => self.sets.get(&id).cloned(),
                    None => self.last.clone(),
                };
                let (Some(by), Some(m)) = (by, row.get("message")) else {
                    return;
                };
                let quest = unquote_string(m);
                if quest.trim().is_empty() {
                    return;
                }
                let tests = &mut out.entry(by).or_default().tests_quests;
                if !tests.contains(&quest) {
                    tests.push(quest);
                }
            }
            _ => {}
        }
    }
}

/// The int properties of the four wield requirements, in [`WeenieFacts::wield`] order.
pub const WIELD_PROPERTIES: [u32; 12] =
    [158, 159, 160, 270, 271, 272, 273, 274, 275, 276, 277, 278];

/// A bool property's value as written: `0`, `false` and a dump's zero bit are false.
fn truth(v: &str) -> bool {
    let v = v.trim();
    !(v == "0" || v.eq_ignore_ascii_case("false") || v == "'\\0'" || v == "'\0'" || v == "b'0'")
}

/// What `text` says of each weenie it holds, by the weenie as written, read in one pass.
#[must_use]
pub fn weenie_facts(text: &str) -> BTreeMap<String, WeenieFacts> {
    let mut out: BTreeMap<String, WeenieFacts> = BTreeMap::new();
    let mut quests = QuestScan::default();
    rows(text, |row| {
        fact(&mut out, row);
        quests.row(&mut out, row);
    });
    out
}

/// What `row` says of its weenie, added to `out`.
fn fact(out: &mut BTreeMap<String, WeenieFacts>, row: &Row<'_>) {
    {
        let key = match row.table {
            "weenie" => "class_Id",
            "weenie_properties_string"
            | "weenie_properties_generator"
            | "weenie_properties_int"
            | "weenie_properties_d_i_d"
            | "weenie_properties_bool"
            | "weenie_properties_float"
            | "weenie_properties_int64"
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
            ("weenie_properties_string", Some(37)) => {
                e.quest_restriction = value.map(unquote_string).filter(|q| !q.trim().is_empty());
            }
            ("weenie_properties_int64", Some(3)) => {
                e.augmentation_cost = value.and_then(|v| v.trim().parse().ok());
            }
            ("weenie_properties_d_i_d", Some(1)) => e.setup = value.and_then(number),
            ("weenie_properties_d_i_d", Some(8)) => e.icon = value.and_then(number),
            ("weenie_properties_d_i_d", Some(7)) => e.clothing_base = value.and_then(number),
            ("weenie_properties_d_i_d", Some(2)) => e.motion_table = value.and_then(number),
            ("weenie_properties_d_i_d", Some(3)) => e.sound_table = value.and_then(number),
            ("weenie_properties_d_i_d", Some(6)) => e.palette_base = value.and_then(number),
            ("weenie_properties_d_i_d", Some(35)) => e.death_treasure = value.and_then(number),
            ("weenie_properties_d_i_d", Some(32)) => e.wielded_treasure = value.and_then(number),
            ("weenie_properties_generator", _) | ("weenie_properties_int", Some(81)) => {
                e.generates = true;
            }
            ("weenie_properties_int", Some(25)) => {
                e.level = value.and_then(|v| v.trim().parse().ok());
            }
            ("weenie_properties_int", Some(n @ (5 | 6 | 9 | 11 | 13 | 33))) => {
                let v: Option<i32> = value.and_then(|v| v.trim().parse().ok());
                match n {
                    5 => e.burden = v,
                    6 => e.holds = v,
                    9 => e.locations = v.map(i32::cast_unsigned),
                    11 => e.max_stack = v,
                    33 => e.bonded = v,
                    _ => e.unit_burden = v,
                }
            }
            ("weenie_properties_int", Some(n)) if WIELD_PROPERTIES.contains(&n) => {
                if let Some(i) = WIELD_PROPERTIES.iter().position(|p| *p == n) {
                    e.wield[i] = value.and_then(|v| v.trim().parse().ok()).unwrap_or(0);
                }
            }
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
