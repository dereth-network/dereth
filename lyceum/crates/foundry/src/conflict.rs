//! Conflicts between a world's packs: two sources that set one thing differently, and the world's
//! word on which of two packs wins.
//!
//! What packs can both set, named as the build's report and an override name it:
//!
//! | item | what it is |
//! |---|---|
//! | `record 0x0E000123` | a record of the data files a pack replaces |
//! | `surface sample:stripes` | a new record, by its kind and symbol |
//! | `vertex 1354,1440 height` | one field of a terrain vertex (`height`, `terrain`, `scene`, `road`), by its place in the world's grid of vertices |
//! | `static A9B4 12`, `static A9B4 sample:post` | a static of landblock `A9B4`: the base's by its index, moved or removed; a pack's by its symbol, placed, moved or removed |
//! | `building A9B4 2` | a building of the base, moved |
//! | `cell A9B4 0105` | a cell of the landblock's own, reshaped |
//! | `cell-static A9B4 0105 2` | a static standing in an indoor cell, by its place among the cell's own, moved or removed |
//! | `weenie 1234` | a weenie class: every row of it, as ACE's export writes a class whole |
//! | `spawn 0x7A9B4001` | an instance: its row, and its links to the instances it owns |
//! | `quest drudgekills` | a quest's registration, by its name in lower case |
//!
//! Sources that set an item alike agree. Set differently, the item is a conflict, and a build
//! stops with one report of every conflict: the item, and each pack, file and line with what it
//! sets. A world settles a conflict between two packs by saying which wins:
//!
//! ```toml
//! [[override]]
//! pack = "holtburg-plus"                    # the pack that wins
//! over = "holtburg"                         # the pack it wins over
//! items = ["weenie 1234", "landblock A9B4"] # what it wins; without a list, all both set
//! ```
//!
//! `landblock A9B4` names everything of the landblock: its vertices, statics, buildings and
//! spawns. Where overrides of both packs cover an item, the one that names it most closely wins:
//! the item itself, then its landblock, then a list-less override. The build takes the winner's
//! and lists what it overrode; the loser's SQL for the item is left out of the build's. Two files
//! of one pack that disagree are no conflict a world settles: the pack is put right.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::Range;

use crate::error::Loc;

/// One thing packs set: its name, as the report and an override name it, the landblocks it
/// stands in, and where it is when it is a place: a landblock and its meters there.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub name: String,
    pub landblocks: BTreeSet<u16>,
    pub place: Option<(u16, [f32; 2])>,
}

impl Item {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            landblocks: BTreeSet::new(),
            place: None,
        }
    }

    /// The item, standing in landblock `lb` too.
    #[must_use]
    pub fn in_landblock(mut self, lb: u16) -> Self {
        self.landblocks.insert(lb);
        self
    }

    /// The item, at `(x, y)` of landblock `lb`.
    #[must_use]
    pub fn at(mut self, lb: u16, x: f32, y: f32) -> Self {
        self.landblocks.insert(lb);
        self.place = Some((lb, [x, y]));
        self
    }
}

/// A replaced record's item.
#[must_use]
pub fn record_item(id: u32) -> String {
    format!("record {id:#010X}")
}

/// A weenie class's item.
#[must_use]
pub fn weenie_item(class: u32) -> String {
    format!("weenie {class}")
}

/// An instance's item.
#[must_use]
pub fn spawn_item(guid: u32) -> String {
    format!("spawn {guid:#010X}")
}

/// A quest's item.
#[must_use]
pub fn quest_item(name: &str) -> String {
    format!("quest {}", crate::content_check::quest_key(name))
}

/// One source that sets an item: its pack, where, and what it sets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Claim {
    pub pack: String,
    pub at: Loc,
    /// What it sets, as the report says it: `sets it to 50`, `removes it`.
    pub sets: String,
    /// What it sets, compared: two claims that agree have the same.
    pub(crate) value: String,
}

impl Claim {
    #[must_use]
    pub fn new(pack: &str, at: &Loc, sets: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            pack: pack.to_owned(),
            at: at.clone(),
            sets: sets.into(),
            value: value.into(),
        }
    }
}

/// An item sources set differently, with no word from the world on which wins.
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    /// What is set: one item, or a run of terrain vertices two files both set.
    pub what: String,
    pub landblocks: BTreeSet<u16>,
    /// Where it is, when it is a place: a landblock and its meters there.
    pub place: Option<(u16, [f32; 2])>,
    /// Every source that sets it, in load order.
    pub claims: Vec<Claim>,
}

impl Conflict {
    /// Whether `other` is the same conflict, wherever in their files its sources now stand.
    #[must_use]
    pub fn same_as(&self, other: &Self) -> bool {
        self.what == other.what
            && self.claims.len() == other.claims.len()
            && self
                .claims
                .iter()
                .zip(&other.claims)
                .all(|(a, b)| a.pack == b.pack && a.at.file == b.at.file && a.value == b.value)
    }

    /// The conflict on one line, starting where its first source is: as the editor lists it.
    #[must_use]
    pub fn line(&self) -> String {
        let mut out = String::new();
        for (i, c) in self.claims.iter().enumerate() {
            if i == 0 {
                out.push_str(&format!("{}: {}: {} {}", c.at, self.what, c.pack, c.sets));
            } else {
                out.push_str(&format!("; {} {} ({})", c.pack, c.sets, c.at));
            }
        }
        out
    }
}

impl fmt::Display for Conflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.what)?;
        for c in &self.claims {
            write!(f, "\n  {}, {}: {}", c.pack, c.at, c.sets)?;
        }
        Ok(())
    }
}

/// An item sources set differently, settled by the world: the claim that stands, and those it
/// stands over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overridden {
    pub what: String,
    pub landblocks: BTreeSet<u16>,
    pub winner: Claim,
    pub over: Vec<Claim>,
}

impl fmt::Display for Overridden {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut losers: Vec<&str> = Vec::new();
        for c in &self.over {
            if !losers.contains(&c.pack.as_str()) {
                losers.push(&c.pack);
            }
        }
        write!(
            f,
            "{}: {}'s ({}) over {}'s",
            self.what,
            self.winner.pack,
            self.winner.at,
            losers.join("'s and ")
        )
    }
}

/// The conflicts found and the overrides taken, of some part of a world.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settled {
    pub conflicts: Vec<Conflict>,
    pub overridden: Vec<Overridden>,
}

impl Settled {
    pub fn extend(&mut self, other: Self) {
        self.conflicts.extend(other.conflicts);
        self.overridden.extend(other.overridden);
    }
}

/// What an override names: one item, or everything of a landblock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    Item(String),
    Landblock(u16),
}

impl Scope {
    /// An item as a world file writes it, read: its id written either way, a quest's name in any
    /// case.
    ///
    /// # Errors
    /// Text that names no item, with the forms there are.
    pub fn parse(text: &str) -> Result<Self, String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let bad = || {
            format!(
                "{text:?} is not an item: record <id>, weenie <class>, spawn <guid>, quest <name>, \
                 landblock <XXYY>, vertex <x>,<y> <field>, static <XXYY> <index or symbol>, \
                 building <XXYY> <index>, cell <XXYY> <cell>, cell-static <XXYY> <cell> <index>, \
                 or a new record's kind and symbol"
            )
        };
        let hex = |w: &str| {
            let h = w.strip_prefix("0x").or_else(|| w.strip_prefix("0X"))?;
            u32::from_str_radix(h, 16).ok()
        };
        let block = |w: &str| {
            (w.len() == 4)
                .then(|| u16::from_str_radix(w, 16).ok())
                .flatten()
        };
        let item = match words.as_slice() {
            ["landblock", l] => return block(l).map(Self::Landblock).ok_or_else(bad),
            ["record", id] => hex(id).map(record_item),
            ["weenie", c] => crate::sql_ids::number(c).map(weenie_item),
            ["spawn", g] => crate::sql_ids::number(g).map(spawn_item),
            ["quest", _, ..] => Some(quest_item(text.trim().split_at(5).1)),
            ["vertex", xy, field] => {
                let field = field.to_ascii_lowercase();
                xy.split_once(',')
                    .and_then(|(x, y)| Some((x.parse::<u32>().ok()?, y.parse::<u32>().ok()?)))
                    .filter(|_| ["height", "terrain", "scene", "road"].contains(&field.as_str()))
                    .map(|(x, y)| format!("vertex {x},{y} {field}"))
            }
            [what @ ("static" | "building"), l, which] => block(l).map(|lb| {
                let which = which
                    .parse::<usize>()
                    .map_or_else(|_| (*which).to_owned(), |i| i.to_string());
                format!("{what} {lb:04X} {which}")
            }),
            ["cell", l, c] => block(l)
                .zip(block(c))
                .map(|(lb, c)| format!("cell {lb:04X} {c:04X}")),
            ["cell-static", l, c, i] => block(l)
                .zip(block(c))
                .zip(i.parse::<usize>().ok())
                .map(|((lb, c), i)| format!("cell-static {lb:04X} {c:04X} {i}")),
            [kind, symbol] => crate::symbol::Kind::parse(kind)
                .filter(|k| k.is_record())
                .map(|k| format!("{k} {symbol}")),
            _ => None,
        };
        item.map(Self::Item).ok_or_else(bad)
    }

    /// How closely the scope names `item`: 3 for the item itself, 2 for its landblock.
    fn covers(&self, item: &Item) -> Option<u8> {
        match self {
            Self::Item(name) => (*name == item.name).then_some(3),
            Self::Landblock(lb) => item.landblocks.contains(lb).then_some(2),
        }
    }
}

/// A world's word that one pack wins over another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Override {
    /// The pack that wins, by name.
    pub pack: String,
    /// The pack it wins over.
    pub over: String,
    /// What it wins; `None` for everything both set.
    pub items: Option<Vec<Scope>>,
    /// Where the world file says it.
    pub at: Loc,
}

/// Whether `overrides` say pack `winner` wins `item` over pack `loser`, and how closely the
/// closest of them names it: 3 for the item itself, 2 for its landblock, 1 for an override
/// without a list.
#[must_use]
pub fn covers(overrides: &[Override], winner: &str, loser: &str, item: &Item) -> Option<u8> {
    overrides
        .iter()
        .filter(|o| o.pack == winner && o.over == loser)
        .filter_map(|o| match &o.items {
            None => Some(1),
            Some(scopes) => scopes.iter().filter_map(|s| s.covers(item)).max(),
        })
        .max()
}

/// Settle `item`, set by `claims` in load order: the place of the claim that stands. Claims that
/// agree stand together, the last of them returned. Otherwise a claim stands when the world says
/// its pack wins over every pack that sets the item otherwise (of two such, the one whose
/// overrides name the item more closely; of those, the later), recorded in `out` as an override;
/// with none, the conflict is recorded and the last claim stands, so an editor still shows a
/// world.
pub(crate) fn settle(
    overrides: &[Override],
    item: &Item,
    claims: &[Claim],
    out: &mut Settled,
) -> usize {
    let last = claims.len().saturating_sub(1);
    if claims.iter().all(|c| c.value == claims[0].value) {
        return last;
    }
    // How closely the world says a claim wins over every claim that differs, if it does.
    let wins = |w: &Claim| -> Option<u8> {
        claims
            .iter()
            .filter(|d| d.value != w.value)
            .map(|d| {
                if d.pack == w.pack {
                    None
                } else {
                    covers(overrides, &w.pack, &d.pack, item)
                }
            })
            .try_fold(u8::MAX, |least, r| r.map(|r| least.min(r)))
    };
    let best = (0..claims.len())
        .filter_map(|i| wins(&claims[i]).map(|r| (r, i)))
        .max();
    if let Some((_, w)) = best {
        out.overridden.push(Overridden {
            what: item.name.clone(),
            landblocks: item.landblocks.clone(),
            winner: claims[w].clone(),
            over: claims
                .iter()
                .filter(|d| d.value != claims[w].value)
                .cloned()
                .collect(),
        });
        return w;
    }
    out.conflicts.push(Conflict {
        what: item.name.clone(),
        landblocks: item.landblocks.clone(),
        place: item.place,
        claims: claims.to_vec(),
    });
    last
}

// -----------------------------------------------------------------------------------------------
// server SQL
// -----------------------------------------------------------------------------------------------

/// What a statement of server SQL belongs to.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SqlKey {
    Weenie(u32),
    Spawn(u32),
    Quest(String),
}

impl SqlKey {
    pub(crate) fn name(&self) -> String {
        match self {
            Self::Weenie(c) => weenie_item(*c),
            Self::Spawn(g) => spawn_item(*g),
            Self::Quest(q) => quest_item(q),
        }
    }
}

/// What one file of server SQL (its ids filled in) sets of one item.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SqlSet {
    pub key: SqlKey,
    /// The line of its first statement.
    pub line: usize,
    pub landblocks: BTreeSet<u16>,
    /// Where an instance it places stands: its landblock and the meters there.
    pub place: Option<(u16, [f32; 2])>,
    /// Each row it writes, by its table and key (`int 25`, `instance`, `link 0x7A9B4002`), with
    /// its values; a deletion as `delete` and its table.
    pub rows: BTreeMap<String, String>,
}

/// One statement of a file: its bytes, from the end of the one before, where its first word is
/// (the range's end for the text after the last), and what it belongs to: each row's item for
/// an `INSERT`, the one item of any other statement that has one.
struct Statement {
    range: Range<usize>,
    start: usize,
    items: Vec<Option<SqlKey>>,
}

/// A value as written, made comparable: a number in decimal, a string without its quotes.
fn plain(v: &str) -> String {
    let b = crate::content::bare(v);
    if let Some(n) = crate::sql_ids::number(&b) {
        return n.to_string();
    }
    if let Ok(i) = b.parse::<i64>() {
        return i.to_string();
    }
    if let Ok(f) = b.parse::<f64>() {
        return format!("{f}");
    }
    match b.to_ascii_lowercase().as_str() {
        "true" => "1".into(),
        "false" => "0".into(),
        _ => crate::content::unquote(&b),
    }
}

/// A table's name in the report: a weenie's tables without their common start.
fn short(table: &str) -> &str {
    match table {
        "landblock_instance" => "instance",
        "landblock_instance_link" => "link",
        t => t.strip_prefix("weenie_properties_").unwrap_or(t),
    }
}

/// The columns that tell a table's rows of one item apart; `None` for a table whose rows are a
/// list, told apart by their place.
fn key_columns(table: &str) -> Option<&'static [&'static str]> {
    Some(match table {
        "weenie" | "landblock_instance" | "quest" => &[],
        "weenie_properties_int"
        | "weenie_properties_int64"
        | "weenie_properties_bool"
        | "weenie_properties_float"
        | "weenie_properties_string"
        | "weenie_properties_d_i_d"
        | "weenie_properties_i_i_d"
        | "weenie_properties_position"
        | "weenie_properties_attribute"
        | "weenie_properties_attribute_2nd"
        | "weenie_properties_skill" => &["type"],
        "weenie_properties_body_part" => &["key"],
        "weenie_properties_spell_book" => &["spell"],
        "weenie_properties_event_filter" => &["event"],
        "weenie_properties_book_page_data" => &["page_Id"],
        "weenie_properties_anim_part" => &["index"],
        "landblock_instance_link" => &["child_GUID"],
        _ => return None,
    })
}

/// The columns that are no part of what a row sets: the database's own ids and dates, and the
/// item's own id.
const NOT_SET: [&str; 7] = [
    "id",
    "object_Id",
    "class_Id",
    "emote_Id",
    "guid",
    "parent_GUID",
    "last_Modified",
];

fn get<'a>(columns: &[String], values: &'a [String], name: &str) -> Option<&'a str> {
    columns
        .iter()
        .position(|c| c.eq_ignore_ascii_case(name))
        .and_then(|i| values.get(i))
        .map(String::as_str)
}

/// The item a row belongs to; `None` for a row of none (an emote's action, which belongs to the
/// emote before it).
fn row_key(table: &str, columns: &[String], values: &[String]) -> Option<SqlKey> {
    let number = |c: &str| get(columns, values, c).and_then(|v| crate::sql_ids::number(&plain(v)));
    match table {
        "weenie" => number("class_Id").map(SqlKey::Weenie),
        t if t.starts_with("weenie_properties_") => number("object_Id").map(SqlKey::Weenie),
        "landblock_instance" => number("guid").map(SqlKey::Spawn),
        "landblock_instance_link" => number("parent_GUID").map(SqlKey::Spawn),
        "quest" => get(columns, values, "name").map(|n| SqlKey::Quest(plain(n))),
        _ => None,
    }
}

/// The item a `DELETE` takes away (`DELETE FROM t WHERE c = v`), and the table.
fn deleted(statement: &str) -> Option<(SqlKey, String)> {
    let flat: String = crate::content::bare(statement).replace('`', "");
    let words: Vec<&str> = flat.split_whitespace().collect();
    let lower: Vec<String> = words.iter().map(|w| w.to_ascii_lowercase()).collect();
    if lower.len() < 6 || lower[0] != "delete" || lower[1] != "from" || lower[3] != "where" {
        return None;
    }
    let table = words[2];
    // `c = v`, `c=v` or `c =v`: the first condition.
    let rest = words[4..].join(" ");
    let (column, value) = rest.split_once('=')?;
    let value = value.trim().trim_end_matches(';').trim();
    let value = value
        .split_once(" AND ")
        .or_else(|| value.split_once(" and "))
        .map_or(value, |(v, _)| v);
    let (column, value) = (column.trim(), plain(value));
    let number = || crate::sql_ids::number(&value);
    let key = match (table, column.to_ascii_lowercase().as_str()) {
        ("weenie", "class_id") => SqlKey::Weenie(number()?),
        (t, "object_id") if t.starts_with("weenie_properties_") => SqlKey::Weenie(number()?),
        ("landblock_instance", "guid") => SqlKey::Spawn(number()?),
        ("landblock_instance_link", "parent_guid" | "child_guid") => SqlKey::Spawn(number()?),
        ("quest", "name") => SqlKey::Quest(value),
        _ => return None,
    };
    Some((key, table.to_owned()))
}

/// Every statement of `text`, with what it belongs to, and what each item's statements set.
fn read(text: &str) -> (Vec<Statement>, Vec<SqlSet>) {
    let pieces = crate::content::statements(text);
    // Every row, by the statement it is in.
    // A row: its table, its columns and its values.
    type RowOf = (String, Vec<String>, Vec<String>);
    let mut rows: Vec<Vec<RowOf>> = vec![Vec::new(); pieces.len()];
    crate::sql_ids::rows(text, |row| {
        let i = pieces.partition_point(|(r, _)| r.end <= row.at);
        if let Some(list) = rows.get_mut(i) {
            list.push((
                row.table.to_owned(),
                row.columns.to_vec(),
                row.values.iter().map(|v| (*v).to_owned()).collect(),
            ));
        }
    });
    let mut lines = crate::sql_ids::Lines::new(text);
    let mut sets: Vec<SqlSet> = Vec::new();
    let mut statements = Vec::new();
    // The weenie the statements before were of: an emote's actions belong to it.
    let mut weenie: Option<SqlKey> = None;
    // How many rows of each list table each item has, for the next one's place.
    let mut places: BTreeMap<(SqlKey, String), usize> = BTreeMap::new();
    for ((range, start), rows) in pieces.into_iter().zip(rows) {
        let line = lines.at(start);
        let mut items = Vec::new();
        let mut set = |key: &SqlKey, row: String, value: String, lb: Option<(u16, [f32; 2])>| {
            let s = match sets.iter_mut().find(|s| s.key == *key) {
                Some(s) => s,
                None => {
                    sets.push(SqlSet {
                        key: key.clone(),
                        line,
                        landblocks: BTreeSet::new(),
                        place: None,
                        rows: BTreeMap::new(),
                    });
                    sets.last_mut().expect("just pushed")
                }
            };
            if let Some((lb, xy)) = lb {
                s.landblocks.insert(lb);
                s.place.get_or_insert((lb, xy));
            }
            s.rows.insert(row, value);
        };
        let word = text[start..range.end]
            .split(|c: char| !c.is_ascii_alphabetic())
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if !rows.is_empty() {
            for (table, columns, values) in &rows {
                let key = row_key(table, columns, values).or_else(|| {
                    (table == crate::content::EMOTE_ACTIONS)
                        .then(|| weenie.clone())
                        .flatten()
                });
                let Some(key) = key else {
                    items.push(None);
                    continue;
                };
                if matches!(key, SqlKey::Weenie(_)) {
                    weenie = Some(key.clone());
                }
                let name = short(table);
                let row = match key_columns(table) {
                    Some([]) => name.to_owned(),
                    Some(cols) => {
                        let k: Vec<String> = cols
                            .iter()
                            .map(|c| get(columns, values, c).map(plain).unwrap_or_default())
                            .collect();
                        format!("{name} {}", k.join(" "))
                    }
                    None => {
                        let n = places.entry((key.clone(), name.to_owned())).or_default();
                        *n += 1;
                        format!("{name} #{n}")
                    }
                };
                let keys = key_columns(table).unwrap_or(&[]);
                let shown: Vec<(&String, String)> = columns
                    .iter()
                    .zip(values)
                    .filter(|(c, _)| {
                        !NOT_SET.iter().any(|n| c.eq_ignore_ascii_case(n))
                            && !keys.iter().any(|n| c.eq_ignore_ascii_case(n))
                            && !(table == "quest" && c.eq_ignore_ascii_case("name"))
                    })
                    .map(|(c, v)| (c, plain(v)))
                    .collect();
                let value = if let [(c, v)] = shown.as_slice() {
                    if c.as_str() == "value" {
                        v.clone()
                    } else {
                        format!("{c} = {v}")
                    }
                } else {
                    shown
                        .iter()
                        .map(|(c, v)| format!("{c} = {v}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let float = |c: &str| {
                    get(columns, values, c)
                        .and_then(|v| crate::content::bare(v).parse::<f32>().ok())
                        .unwrap_or(0.0)
                };
                let lb = (table == "landblock_instance")
                    .then(|| get(columns, values, "obj_Cell_Id"))
                    .flatten()
                    .and_then(|v| crate::sql_ids::number(&plain(v)))
                    .and_then(|cell| u16::try_from(cell >> 16).ok())
                    .map(|lb| (lb, [float("origin_X"), float("origin_Y")]));
                set(&key, row, value, lb);
                items.push(Some(key));
            }
        } else if word == "delete" {
            if let Some((key, table)) = deleted(&text[start..range.end]) {
                if matches!(key, SqlKey::Weenie(_)) {
                    weenie = Some(key.clone());
                }
                set(
                    &key,
                    format!("delete {}", short(&table)),
                    String::new(),
                    None,
                );
                items.push(Some(key));
            }
        } else if word == "set" && text[start..range.end].contains("@parent_id") {
            items.push(weenie.clone());
        }
        statements.push(Statement {
            range,
            start,
            items,
        });
    }
    // A spawn no row places stands in the landblock its guid names.
    for s in &mut sets {
        if let SqlKey::Spawn(g) = s.key {
            if s.landblocks.is_empty() && g >> 28 == 7 {
                s.landblocks.extend(u16::try_from((g >> 12) & 0xFFFF).ok());
            }
        }
    }
    (statements, sets)
}

/// What each item of `text`, a file of server SQL with its ids filled in, sets.
pub(crate) fn sql_sets(text: &str) -> Vec<SqlSet> {
    read(text).1
}

/// `sets`, the files that set one item, as claims: each with its pack and file, and what it sets
/// that the others set otherwise.
pub(crate) fn sql_claims(sets: &[(String, Loc, &SqlSet)]) -> Vec<Claim> {
    let keys: BTreeSet<&String> = sets.iter().flat_map(|(_, _, s)| s.rows.keys()).collect();
    let differ: Vec<&String> = keys
        .into_iter()
        .filter(|k| {
            let first = sets[0].2.rows.get(*k);
            sets.iter().any(|(_, _, s)| s.rows.get(*k) != first)
        })
        .collect();
    sets.iter()
        .map(|(pack, at, s)| {
            let value: Vec<String> = s.rows.iter().map(|(k, v)| format!("{k}={v}")).collect();
            let says = if s.rows.keys().all(|k| k.starts_with("delete")) {
                "deletes it".to_owned()
            } else {
                let mut parts: Vec<String> = differ
                    .iter()
                    .take(3)
                    .map(|k| match s.rows.get(*k) {
                        Some(v) if v.is_empty() => (*k).clone(),
                        Some(v) => format!("{k}: {v}"),
                        None => format!("no {k}"),
                    })
                    .collect();
                if differ.len() > 3 {
                    parts.push(format!("and {} more", differ.len() - 3));
                }
                format!("sets {}", parts.join("; "))
            };
            Claim::new(pack, at, says, value.join("\n"))
        })
        .collect()
}

/// `text` without the statements and rows of the items in `lost`; `None` when no statement is
/// left.
pub(crate) fn without(text: &str, lost: &BTreeSet<SqlKey>) -> Option<String> {
    let (statements, _) = read(text);
    let mut out = String::with_capacity(text.len());
    let mut kept = 0;
    let is_lost = |k: &Option<SqlKey>| k.as_ref().is_some_and(|k| lost.contains(k));
    for s in statements {
        let piece = &text[s.range.clone()];
        let gone = s.items.iter().filter(|k| is_lost(k)).count();
        if gone == 0 {
            out.push_str(piece);
            kept += usize::from(s.start < s.range.end);
        } else if gone < s.items.len() {
            // Rows of several items in one statement: only the lost ones' go.
            let mut doc = crate::content::SqlDoc::parse(piece);
            for p in &mut doc.pieces {
                if let crate::content::Piece::Insert(insert) = p {
                    let mut keep = s.items.iter().map(|k| !is_lost(k));
                    insert.rows.retain(|_| keep.next().unwrap_or(true));
                }
            }
            out.push_str(&doc.render());
            kept += 1;
        }
    }
    (kept > 0).then_some(out)
}
