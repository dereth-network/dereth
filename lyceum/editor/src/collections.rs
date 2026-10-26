//! A weenie's lists of rows, each with an editor of its own: what it holds, wields and sells
//! (each entry's chance in percent where the server rolls for it, by set, with what fills a set
//! to 100% shown as nothing), what it spawns (each entry's share in percent), its spells by
//! name, its attributes and vitals, its skills and its body's parts.
//!
//! The server reads some of these as numbers that are not what they look like, and the editors
//! show and take what they mean instead:
//! - A generator's chances are written as running totals: each entry's share is its total less
//!   the one before it, a lower total starting a new set, and -1 always making it. Every share is
//!   of all the entries' together.
//! - A creature's treasure rows (a create list row for treasure, with a shade) take the shade as
//!   their chance. Rows are drawn in sets that fill 100% in turn, one entry of each set made, and
//!   an entry of class 0 makes nothing.
//! - A creature's spell is cast at the chance its spell book's row is over 2 (2.05 is 5%), or a
//!   hundredth of it when it is not over 2.

use std::rc::Rc;

use dereth_primitives::DataId;
use egui::containers::menu::{MenuButton, MenuConfig};
use lyceum_foundry::content::{self, SqlDoc};
use lyceum_foundry::edit::Weenie;

use super::definition::Drill;
use super::form::FormEdit;
use super::kinds::Kind;
use super::{EditorApp, UiAction};

/// The create list's table.
pub const CREATE_LIST: &str = "weenie_properties_create_list";
/// The generator profiles' table.
pub const GENERATOR: &str = "weenie_properties_generator";
/// The spell book's table.
pub const SPELL_BOOK: &str = "weenie_properties_spell_book";
pub const ATTRIBUTES: &str = "weenie_properties_attribute";
pub const VITALS: &str = "weenie_properties_attribute_2nd";
pub const SKILLS: &str = "weenie_properties_skill";
pub const BODY_PARTS: &str = "weenie_properties_body_part";
pub const POSITIONS: &str = "weenie_properties_position";

/// The create list's destination bits.
pub mod dest {
    pub const CONTAIN: u32 = 1;
    pub const WIELD: u32 = 2;
    pub const SHOP: u32 = 4;
    pub const TREASURE: u32 = 8;
}

/// A row of a table as written: each column's value, as the SQL has it, and its comment.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableRow {
    pub cells: Vec<(String, String)>,
    pub comment: Option<String>,
}

impl TableRow {
    /// A row of these values.
    #[must_use]
    pub fn of(cells: &[(&str, String)]) -> Self {
        Self {
            cells: cells
                .iter()
                .map(|(c, v)| ((*c).to_owned(), v.clone()))
                .collect(),
            comment: None,
        }
    }

    /// Column `c`'s value as written.
    #[must_use]
    pub fn get(&self, c: &str) -> Option<&str> {
        self.cells
            .iter()
            .find(|(k, _)| k == c)
            .map(|(_, v)| v.as_str())
    }

    /// Column `c` set to `v` (as it is to be written), added when the row lacks it.
    pub fn set(&mut self, c: &str, v: String) {
        match self.cells.iter_mut().find(|(k, _)| k == c) {
            Some((_, old)) => *old = v,
            None => self.cells.push((c.to_owned(), v)),
        }
    }

    /// Column `c` as a number.
    #[must_use]
    pub fn number(&self, c: &str) -> Option<f64> {
        let v = content::bare(self.get(c)?);
        let v = v.trim();
        v.parse::<f64>()
            .ok()
            .or_else(|| lyceum_foundry::sql_ids::number(v).map(f64::from))
    }

    /// Column `c` as a whole number of bits.
    #[must_use]
    pub fn bits(&self, c: &str) -> u32 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a column's bits
        self.number(c).map_or(0, |n| n as i64 as u32)
    }
}

/// The rows of `table` in `doc`, in order: each column of the table the server's export writes
/// (but the weenie's own class), as written.
#[must_use]
pub fn rows_of(doc: &SqlDoc, table: &str) -> Vec<TableRow> {
    let columns = super::form::columns(table);
    doc.rows(table)
        .into_iter()
        .map(|r| TableRow {
            cells: columns
                .iter()
                .skip(1)
                .filter_map(|c| doc.raw(r, c).map(|v| ((*c).to_owned(), v.to_owned())))
                .collect(),
            comment: doc.comment(r).map(str::to_owned),
        })
        .collect()
}

/// `doc`'s rows of `table` written as `rows`, in that order: each row there now takes the next of
/// `rows`, those left over are taken away, and any more are added after them.
pub fn write_rows(doc: &mut SqlDoc, table: &str, rows: &[TableRow]) {
    let columns = super::form::columns(table);
    let have = doc.rows(table).len();
    for (i, row) in rows.iter().enumerate().take(have) {
        let r = doc.rows(table)[i];
        for (c, v) in &row.cells {
            doc.set_cell(r, c, v);
        }
        if doc.comment(r) != row.comment.as_deref() {
            doc.set_comment(r, row.comment.clone());
        }
    }
    while doc.rows(table).len() > rows.len() {
        let Some(r) = doc.rows(table).last().copied() else {
            break;
        };
        doc.remove_row(r);
    }
    for row in rows.iter().skip(have) {
        let values: Vec<(&str, String)> = row
            .cells
            .iter()
            .map(|(c, v)| (c.as_str(), v.clone()))
            .collect();
        doc.add_row(table, columns, &values, None, row.comment.clone());
    }
}

/// `all` with the rows `member` takes as one list's put back as `list`: each of the list's places
/// takes the next of `list`, those left over are taken away, and any more follow the list's last
/// place (or the end, for a list with none).
#[must_use]
pub fn splice(
    all: &[TableRow],
    member: impl Fn(&TableRow) -> bool,
    list: &[TableRow],
) -> Vec<TableRow> {
    let last = all.iter().rposition(&member);
    let mut next = list.iter();
    let mut out = Vec::with_capacity(all.len() + list.len());
    for (i, row) in all.iter().enumerate() {
        if member(row) {
            if let Some(r) = next.next() {
                out.push(r.clone());
            }
        } else {
            out.push(row.clone());
        }
        if Some(i) == last {
            out.extend(next.by_ref().cloned());
        }
    }
    out.extend(next.cloned());
    out
}

/// A decimal as the SQL writes it: at most six places, no trailing zeros.
#[must_use]
pub fn decimal(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_owned()
    } else {
        s.to_owned()
    }
}

// -----------------------------------------------------------------------------------------------
// what it holds, wields and sells
// -----------------------------------------------------------------------------------------------

/// Whether a create list row of destination `d` is in list `list` of a weenie that is a creature
/// (`creature`) or not: what it wields; what it holds (for a creature, what it drops when it dies:
/// what it holds and its treasure not wielded); what it sells.
#[must_use]
pub const fn in_list(list: Drill, d: u32, creature: bool) -> bool {
    match list {
        Drill::Wields => d & dest::WIELD != 0,
        Drill::Holds => {
            d & dest::CONTAIN != 0 || (creature && d & dest::TREASURE != 0 && d & dest::WIELD == 0)
        }
        Drill::Sells => d & dest::SHOP != 0,
        _ => false,
    }
}

/// The table list `d` is rows of, when it has one: what a weenie holds, wields and sells are rows
/// of its create list.
#[must_use]
pub const fn table_of(d: Drill) -> Option<&'static str> {
    match d {
        Drill::Wields | Drill::Holds | Drill::Sells => Some(CREATE_LIST),
        Drill::Spells => Some(SPELL_BOOK),
        Drill::Spawns => Some(GENERATOR),
        Drill::Skills => Some(SKILLS),
        Drill::BodyParts => Some(BODY_PARTS),
        Drill::Emotes | Drill::Appears | Drill::UsedBy => None,
    }
}

/// Whether a row of list `d`'s table is in the list, for a weenie that is a creature or not:
/// every row but a create list's, which goes by its destination.
pub fn member(d: Drill, creature: bool) -> impl Fn(&TableRow) -> bool {
    move |r: &TableRow| match d {
        Drill::Wields | Drill::Holds | Drill::Sells => {
            in_list(d, r.bits("destination_Type"), creature)
        }
        _ => true,
    }
}

/// List `d`'s rows of `doc`, in order.
#[must_use]
pub fn list_of(doc: &SqlDoc, d: Drill) -> Vec<TableRow> {
    let Some(table) = table_of(d) else {
        return Vec::new();
    };
    let creature = super::kinds::Kind::of(doc).is_creature();
    rows_of(doc, table)
        .into_iter()
        .filter(member(d, creature))
        .collect()
}

/// A row's values without their comments, as rows are compared.
#[must_use]
pub fn row_words(r: &TableRow) -> String {
    r.cells
        .iter()
        .map(|(_, v)| content::bare(v).trim().to_owned())
        .collect::<Vec<_>>()
        .join("|")
}

/// Whether a position row says where a weenie sends who uses it.
#[must_use]
pub fn is_destination(r: &TableRow) -> bool {
    r.number("position_Type") == Some(2.0)
}

/// The destination a new row of list `list` takes: made always, or (`treasure`) by chance.
#[must_use]
pub const fn destination_of(list: Drill, treasure: bool) -> u32 {
    let base = match list {
        Drill::Wields => dest::WIELD,
        Drill::Sells => dest::SHOP,
        _ => dest::CONTAIN,
    };
    if treasure {
        base | dest::TREASURE
    } else {
        base
    }
}

/// Whether create list row `r` of a creature is made by chance: treasure with a shade, which is
/// its chance.
#[must_use]
pub fn rolls(r: &TableRow) -> bool {
    r.bits("destination_Type") & dest::TREASURE != 0 && r.number("shade").unwrap_or(0.0) != 0.0
}

/// Whether what a creature's create list row of destination `d` makes, in its list `list`, ends
/// up on its corpse, as the server's own settings have it: what it holds and its treasure not
/// wielded are made when it dies, onto it; what it wields only when its row is treasure, and never
/// a thing destroyed when whoever has it dies (`destroyed`).
#[must_use]
pub const fn drops(list: Drill, d: u32, destroyed: bool) -> bool {
    match list {
        Drill::Holds => true,
        Drill::Wields => d & dest::TREASURE != 0 && !destroyed,
        _ => false,
    }
}

/// Whether create list row `r` makes nothing: class 0.
#[must_use]
pub fn is_nothing(r: &TableRow) -> bool {
    r.number("weenie_Class_Id") == Some(0.0)
}

/// Whether create list row `r`'s shade colors what it makes: a row that is no treasure, as a
/// treasure row's shade is its chance.
#[must_use]
pub fn shades(r: &TableRow) -> bool {
    r.bits("destination_Type") & dest::TREASURE == 0
}

/// A row's chance, 0 to 1: its shade.
fn chance(r: &TableRow) -> f64 {
    r.number("shade").unwrap_or(0.0)
}

/// The set each row of `list` is drawn in, as the server draws them (`None` for a row made
/// always): a set closes once its chances reach 100%, counted as the server counts them.
#[must_use]
pub fn sets(list: &[TableRow]) -> Vec<Option<usize>> {
    let mut total = 0.0f32;
    let mut set = 0usize;
    let mut out = Vec::with_capacity(list.len());
    for r in list {
        if !rolls(r) {
            out.push(None);
            continue;
        }
        if total >= 1.0 {
            total = 0.0;
            set += 1;
        }
        #[allow(clippy::cast_possible_truncation)] // the server's own single precision
        {
            total += chance(r) as f32;
        }
        out.push(Some(set));
    }
    out
}

/// Each set's chances together, 0 to 1, in order.
#[must_use]
pub fn set_totals(list: &[TableRow]) -> Vec<f64> {
    let mut out: Vec<f64> = Vec::new();
    for (r, s) in list.iter().zip(sets(list)) {
        if let Some(s) = s {
            if out.len() <= s {
                out.resize(s + 1, 0.0);
            }
            out[s] += chance(r);
        }
    }
    out
}

/// A chance that cannot be had: the most the entry can have, in percent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TooLikely {
    pub most: f64,
}

/// Which rows of `list` set `s` has, by place.
fn members(list: &[TableRow], s: usize) -> Vec<bool> {
    sets(list).iter().map(|x| *x == Some(s)).collect()
}

/// Whether set `s` is the last of `list`.
fn is_last(list: &[TableRow], s: usize) -> bool {
    sets(list).iter().flatten().max().copied() == Some(s)
}

/// A row that makes nothing, at chance `c`, as a row `like` of the same list is written.
fn nothing_like(like: &TableRow, c: f64) -> TableRow {
    TableRow::of(&[
        (
            "destination_Type",
            like.bits("destination_Type").to_string(),
        ),
        ("weenie_Class_Id", "0".into()),
        ("stack_Size", "1".into()),
        ("palette", "0".into()),
        ("shade", decimal(c)),
        ("try_To_Bond", "False".into()),
    ])
}

/// The set of the rows `mine` marks filled to 100% with nothing: its row of nothing (the first,
/// any other taken away) made what is left, or a row of nothing added after its last when it has
/// none, unless it is the last set (`last`), whose rest makes nothing as it is. Nothing at 0% is
/// taken away. A set before another then closes where it did, counted as the server counts it.
fn fill(list: &mut Vec<TableRow>, mut mine: Vec<bool>, last: bool) {
    let Some(end) = mine.iter().rposition(|m| *m) else {
        return;
    };
    let like = list[end].clone();
    let made: f64 = (0..list.len())
        .filter(|j| mine[*j] && !is_nothing(&list[*j]))
        .map(|j| chance(&list[j]))
        .sum();
    let rest = (1.0 - made).max(0.0);
    let nothings: Vec<usize> = (0..list.len())
        .filter(|j| mine[*j] && is_nothing(&list[*j]))
        .collect();
    for j in nothings.iter().skip(1).rev() {
        list.remove(*j);
        mine.remove(*j);
    }
    match nothings.first() {
        Some(&j) if rest > 1e-7 => list[j].set("shade", decimal(rest)),
        Some(&j) => {
            list.remove(j);
            mine.remove(j);
        }
        None if rest > 1e-7 && !last => {
            let at = mine.iter().rposition(|m| *m).map_or(list.len(), |e| e + 1);
            list.insert(at, nothing_like(&like, rest));
            mine.insert(at, true);
        }
        None => {}
    }
    if !last {
        close(list, &mine);
    }
}

/// The set of the rows `mine` marks closing at its last row as the server adds its chances up, in
/// its own single precision: its last row made the millionths more likely it may need.
fn close(list: &mut [TableRow], mine: &[bool]) {
    let Some(end) = mine.iter().rposition(|m| *m) else {
        return;
    };
    for _ in 0..10_000 {
        #[allow(clippy::cast_possible_truncation)] // the server's own single precision
        let total = (0..=end)
            .filter(|j| mine[*j] && rolls(&list[*j]))
            .fold(0.0f32, |t, j| t + chance(&list[j]) as f32);
        if total >= 1.0 {
            break;
        }
        let c = chance(&list[end]) + 1e-6;
        list[end].set("shade", decimal(c));
    }
}

/// Row `i` of `list` (one drawn by chance) set to chance `percent`; what its set makes nothing at
/// takes the difference, so its sets stay as they are. A chance of 0 changes nothing: the server
/// makes a treasure row of no chance always.
///
/// # Errors
/// More than its set has room for besides its others: the most it can have.
pub fn set_chance(list: &mut Vec<TableRow>, i: usize, percent: f64) -> Result<(), TooLikely> {
    let Some(s) = sets(list).get(i).copied().flatten() else {
        return Ok(());
    };
    let mine = members(list, s);
    let last = is_last(list, s);
    let others: f64 = (0..list.len())
        .filter(|j| *j != i && mine[*j] && !is_nothing(&list[*j]))
        .map(|j| chance(&list[j]))
        .sum();
    let want = percent / 100.0;
    if others + want > 1.0 + 1e-9 {
        return Err(TooLikely {
            most: ((1.0 - others) * 100.0).max(0.0),
        });
    }
    if want <= 0.0 {
        return Ok(());
    }
    list[i].set("shade", decimal(want));
    fill(list, mine, last);
    Ok(())
}

/// Row `i` of `list` taken away: one drawn by chance leaves its chance to nothing, so its set
/// stays as it is, and a set left making only nothing goes.
pub fn remove_from(list: &mut Vec<TableRow>, i: usize) {
    if i >= list.len() {
        return;
    }
    let Some(s) = sets(list)[i] else {
        list.remove(i);
        return;
    };
    let mut mine = members(list, s);
    let last = is_last(list, s);
    list.remove(i);
    mine.remove(i);
    let left: Vec<usize> = (0..list.len()).filter(|j| mine[*j]).collect();
    if left.iter().all(|j| is_nothing(&list[*j])) {
        for j in left.iter().rev() {
            list.remove(*j);
        }
        return;
    }
    fill(list, mine, last);
}

/// `row` added to set `s` of `list`, after its last entry: it takes what the set makes nothing
/// at, or when it makes nothing at nothing, an even share, the set's other entries made less
/// likely to give it room.
pub fn add_to_set(list: &mut Vec<TableRow>, s: usize, mut row: TableRow) {
    let mut mine = members(list, s);
    let last = is_last(list, s);
    let made: Vec<usize> = (0..list.len())
        .filter(|j| mine[*j] && !is_nothing(&list[*j]))
        .collect();
    let sum: f64 = made.iter().map(|j| chance(&list[*j])).sum();
    let c = if 1.0 - sum > 1e-6 {
        1.0 - sum
    } else {
        #[allow(clippy::cast_precision_loss)] // a set's few entries
        let n = made.len() as f64;
        for j in &made {
            let scaled = chance(&list[*j]) * n / (n + 1.0);
            list[*j].set("shade", decimal(scaled));
        }
        1.0 / (n + 1.0)
    };
    row.set("shade", decimal(c));
    let at = made.last().map_or_else(
        || mine.iter().rposition(|m| *m).map_or(list.len(), |e| e + 1),
        |j| j + 1,
    );
    list.insert(at, row);
    mine.insert(at, true);
    fill(list, mine, last);
}

/// `row` added as a set of its own after the others, made always (100%); the last set before it
/// is filled with nothing first, so it does not take the new one in.
pub fn add_set(list: &mut Vec<TableRow>, mut row: TableRow) {
    if let Some(s) = sets(list).iter().flatten().max().copied() {
        let mine = members(list, s);
        fill(list, mine, false);
    }
    row.set("shade", "1".into());
    list.push(row);
}

/// Whether rows `a` and `b` of `list` may change places: both made always, or both of one set.
#[must_use]
pub fn may_swap(list: &[TableRow], a: usize, b: usize) -> bool {
    let s = sets(list);
    a < list.len() && b < list.len() && s[a] == s[b]
}

// -----------------------------------------------------------------------------------------------
// what it spawns
// -----------------------------------------------------------------------------------------------

/// A generator entry's chance: always, or a step of the draw in a set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Draw {
    Always,
    Chance { set: usize, step: f64 },
}

/// The draw a generator's running totals write, as the server reads them: each entry's step is
/// its total less the one before it, a lower total starting a new set, -1 always.
#[must_use]
pub fn draws(totals: &[f64]) -> Vec<Draw> {
    let mut last = 0.0f64;
    let mut set = 0usize;
    let mut seen = false;
    totals
        .iter()
        .map(|t| {
            if *t < 0.0 {
                return Draw::Always;
            }
            if seen && *t < last {
                last = 0.0;
                set += 1;
            }
            seen = true;
            let step = t - last;
            last = *t;
            Draw::Chance { set, step }
        })
        .collect()
}

/// The running totals that write `d`: each set's steps added up in turn, -1 always.
#[must_use]
pub fn totals(d: &[Draw]) -> Vec<f64> {
    let mut sum = 0.0;
    let mut set = None;
    d.iter()
        .map(|x| match x {
            Draw::Always => -1.0,
            Draw::Chance { set: s, step } => {
                if set != Some(*s) {
                    sum = 0.0;
                    set = Some(*s);
                }
                sum += step;
                sum
            }
        })
        .collect()
}

/// Each entry's share of the draw, in percent (`None` for always).
#[must_use]
pub fn percents(d: &[Draw]) -> Vec<Option<f64>> {
    let total: f64 = d
        .iter()
        .map(|x| match x {
            Draw::Chance { step, .. } => *step,
            Draw::Always => 0.0,
        })
        .sum();
    d.iter()
        .map(|x| match x {
            Draw::Always => None,
            Draw::Chance { step, .. } => Some(if total > 0.0 {
                step / total * 100.0
            } else {
                0.0
            }),
        })
        .collect()
}

/// When a generator's entry is made again: once what it made is gone (destroyed or picked up,
/// which the server takes alike, as it does a row that says neither), or when it dies; else a
/// value of neither, as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Again {
    Gone,
    Death,
    Other(u32),
}

impl Again {
    /// When the entry of a row whose `when_Create` is `when` is made again.
    #[must_use]
    pub const fn of(when: u32) -> Self {
        match when {
            0..=2 => Self::Gone,
            4 => Self::Death,
            n => Self::Other(n),
        }
    }

    /// What a row whose `when_Create` is `now` writes when `self` is chosen: nothing when it says
    /// so already, else destroyed for gone and death for death.
    #[must_use]
    pub const fn written(self, now: u32) -> Option<u32> {
        match (self, Self::of(now)) {
            (Self::Gone, Self::Gone) | (Self::Death, Self::Death) => None,
            (Self::Gone, _) => Some(1),
            (Self::Death, _) => Some(4),
            (Self::Other(n), _) => Some(n),
        }
    }

    /// Its words' key in the text, for the two a row is chosen between.
    #[must_use]
    pub const fn key(self) -> Option<&'static str> {
        match self {
            Self::Gone => Some("collections.when_gone"),
            Self::Death => Some("collections.when_death"),
            Self::Other(_) => None,
        }
    }
}

/// A change of a generator's draw that is not made: one of its sets would read back as part of
/// the set before it (its first entry as likely as all of that set), and the server would draw
/// the two as one, at other shares than those set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JoinsSets {
    /// The set that would join the one before it, and that one, counted from 1.
    pub set: usize,
    pub before: usize,
}

/// `d`'s sets numbered from 0 in the order they come, one left empty gone.
fn compact(d: &mut [Draw]) {
    let mut seen: Vec<usize> = Vec::new();
    for x in d.iter_mut() {
        if let Draw::Chance { set, .. } = x {
            let at = seen.iter().position(|s| s == set).unwrap_or_else(|| {
                seen.push(*set);
                seen.len() - 1
            });
            *set = at;
        }
    }
}

/// Whether `d` (its sets numbered in order) reads back as written: each set's first step below
/// what the set before it adds up to.
///
/// # Errors
/// The first set that would read back as part of the one before it.
fn readable(d: &[Draw]) -> Result<(), JoinsSets> {
    let mut sums: Vec<f64> = Vec::new();
    let mut firsts: Vec<f64> = Vec::new();
    for x in d {
        if let Draw::Chance { set, step } = x {
            if sums.len() <= *set {
                sums.resize(set + 1, 0.0);
                firsts.resize(set + 1, *step);
            }
            sums[*set] += step;
        }
    }
    match (1..sums.len()).find(|s| firsts[*s] >= sums[s - 1] - 1e-6) {
        Some(s) => Err(JoinsSets {
            set: s + 1,
            before: s,
        }),
        None => Ok(()),
    }
}

/// `next` written over `d` when it reads back as written, its sets numbered in order.
///
/// # Errors
/// A set of `next` that would read back as part of the one before it; `d` is as it was.
fn commit(d: &mut Vec<Draw>, mut next: Vec<Draw>) -> Result<(), JoinsSets> {
    compact(&mut next);
    readable(&next)?;
    *d = next;
    Ok(())
}

/// The total of `d`'s steps, or 1 for none: what new steps add up to.
fn scale_of(d: &[Draw]) -> f64 {
    let t: f64 = d
        .iter()
        .map(|x| match x {
            Draw::Chance { step, .. } => *step,
            Draw::Always => 0.0,
        })
        .sum();
    if t > 0.0 {
        t
    } else {
        1.0
    }
}

/// Entry `i` of `d` given share `percent` of the draw; the others keep their shares among them
/// in the rest. An entry made always joins the set of the entry before it.
///
/// # Errors
/// A set that would then read back as part of the one before it; nothing is changed.
pub fn set_share(d: &mut Vec<Draw>, i: usize, percent: f64) -> Result<(), JoinsSets> {
    if i >= d.len() {
        return Ok(());
    }
    let mut next = d.clone();
    let p = percent.clamp(0.0, 100.0);
    let scale = scale_of(&next);
    let shares = percents(&next);
    let old = shares[i].unwrap_or(0.0);
    let others: Vec<usize> = (0..next.len())
        .filter(|j| *j != i && shares[*j].is_some())
        .collect();
    let set = match next[i] {
        Draw::Chance { set, .. } => set,
        Draw::Always => (0..i)
            .rev()
            .find_map(|j| match next[j] {
                Draw::Chance { set, .. } => Some(set),
                Draw::Always => None,
            })
            .or_else(|| {
                (i + 1..next.len()).find_map(|j| match next[j] {
                    Draw::Chance { set, .. } => Some(set),
                    Draw::Always => None,
                })
            })
            .unwrap_or(0),
    };
    #[allow(clippy::cast_precision_loss)] // a generator's few entries
    let n = others.len() as f64;
    for j in &others {
        let q = shares[*j].unwrap_or(0.0);
        let new = if old < 100.0 - 1e-9 {
            q * (100.0 - p) / (100.0 - old)
        } else {
            (100.0 - p) / n
        };
        if let Draw::Chance { step, .. } = &mut next[*j] {
            *step = new / 100.0 * scale;
        }
    }
    next[i] = Draw::Chance {
        set,
        step: p / 100.0 * scale,
    };
    commit(d, next)
}

/// Entry `i` of `d` made always; the others' shares grow to fill the draw.
///
/// # Errors
/// As [`set_share`].
pub fn set_always(d: &mut Vec<Draw>, i: usize) -> Result<(), JoinsSets> {
    let mut next = d.clone();
    if let Some(x) = next.get_mut(i) {
        *x = Draw::Always;
    }
    commit(d, next)
}

/// A new entry at the end of `d`: the whole draw when it has no other by chance, else an even
/// share, in the last set.
///
/// # Errors
/// As [`set_share`].
pub fn add_draw(d: &mut Vec<Draw>) -> Result<(), JoinsSets> {
    let chances = d
        .iter()
        .filter(|x| matches!(x, Draw::Chance { .. }))
        .count();
    let set = d
        .iter()
        .rev()
        .find_map(|x| match x {
            Draw::Chance { set, .. } => Some(*set),
            Draw::Always => None,
        })
        .unwrap_or(0);
    let mut next = d.clone();
    next.push(Draw::Chance {
        set,
        step: if chances == 0 { 1.0 } else { 0.0 },
    });
    if chances > 0 {
        #[allow(clippy::cast_precision_loss)] // a generator's few entries
        let even = 100.0 / (chances as f64 + 1.0);
        let i = next.len() - 1;
        set_share(&mut next, i, even)?;
    }
    commit(d, next)
}

/// Entry `i` of `d` taken away; the others' shares grow to fill the draw.
///
/// # Errors
/// As [`set_share`].
pub fn remove_draw(d: &mut Vec<Draw>, i: usize) -> Result<(), JoinsSets> {
    let mut next = d.clone();
    if i < next.len() {
        next.remove(i);
    }
    commit(d, next)
}

/// Entries `a` and `b` of `d` changed places.
///
/// # Errors
/// As [`set_share`].
pub fn swap_draws(d: &mut Vec<Draw>, a: usize, b: usize) -> Result<(), JoinsSets> {
    let mut next = d.clone();
    if a < next.len() && b < next.len() {
        next.swap(a, b);
    }
    commit(d, next)
}

/// Which set each entry of `d` is drawn in, counted from 0 (`None`: made always), and each set's
/// share of the draw in all, in percent.
#[must_use]
pub fn set_shares(d: &[Draw]) -> (Vec<Option<usize>>, Vec<f64>) {
    let shares = percents(d);
    let mut totals: Vec<f64> = Vec::new();
    let of: Vec<Option<usize>> = d
        .iter()
        .zip(&shares)
        .map(|(x, p)| match x {
            Draw::Chance { set, .. } => {
                if totals.len() <= *set {
                    totals.resize(set + 1, 0.0);
                }
                totals[*set] += p.unwrap_or(0.0);
                Some(*set)
            }
            Draw::Always => None,
        })
        .collect();
    (of, totals)
}

// -----------------------------------------------------------------------------------------------
// spells
// -----------------------------------------------------------------------------------------------

/// The chance, in percent, a creature casts a spell its book writes at `probability`.
#[must_use]
pub fn cast_chance(probability: f64) -> f64 {
    if probability > 2.0 {
        (probability - 2.0) * 100.0
    } else {
        probability
    }
}

/// What a creature's spell book writes for a chance of `percent` to cast.
#[must_use]
pub fn cast_probability(percent: f64) -> f64 {
    if percent <= 0.0 {
        0.0
    } else {
        2.0 + percent / 100.0
    }
}

// -----------------------------------------------------------------------------------------------
// the editors
// -----------------------------------------------------------------------------------------------

/// A percent as the editors show it: at most two places.
#[must_use]
pub fn percent_words(p: f64) -> String {
    let s = format!("{p:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// What was typed as a percent: a number, with or without `%` after it.
#[must_use]
pub fn percent_typed(typed: &str) -> Option<f64> {
    typed
        .trim()
        .trim_end_matches('%')
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|p| p.is_finite() && *p >= 0.0)
}

/// What was typed as a whole number, as it is written.
fn whole_typed(typed: &str) -> Option<String> {
    typed.trim().parse::<i64>().ok().map(|n| n.to_string())
}

/// What was typed as a decimal, as it is written.
fn decimal_typed(typed: &str) -> Option<String> {
    typed
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .map(decimal)
}

/// The attributes, as the server numbers them, each with its name's key in the text.
pub const ATTRIBUTE_NAMES: &[(u32, &str)] = &[
    (1, "attribute_strength"),
    (2, "attribute_endurance"),
    (3, "attribute_quickness"),
    (4, "attribute_coordination"),
    (5, "attribute_focus"),
    (6, "attribute_self"),
];

/// The vitals a weenie's vitals table writes (each one's most), with their names' keys.
pub const VITAL_NAMES: &[(u32, &str)] =
    &[(1, "vital_health"), (3, "vital_stamina"), (5, "vital_mana")];

/// How trained a skill is.
const TRAINING: &[(i64, &str)] = &[
    (1, "training_untrained"),
    (2, "training_trained"),
    (3, "training_specialized"),
];

/// A body's parts, as the server numbers them, each with its name's key.
pub const BODY_PART_NAMES: &[(u32, &str)] = &[
    (0, "part_head"),
    (1, "part_chest"),
    (2, "part_abdomen"),
    (3, "part_upper_arm"),
    (4, "part_lower_arm"),
    (5, "part_hand"),
    (6, "part_upper_leg"),
    (7, "part_lower_leg"),
    (8, "part_foot"),
    (9, "part_horn"),
    (10, "part_front_leg"),
    (12, "part_front_foot"),
    (13, "part_rear_leg"),
    (15, "part_rear_foot"),
    (16, "part_torso"),
    (17, "part_tail"),
    (18, "part_arm"),
    (19, "part_leg"),
    (20, "part_claw"),
    (21, "part_wings"),
    (22, "part_breath"),
    (23, "part_tentacle"),
    (24, "part_upper_tentacle"),
    (25, "part_lower_tentacle"),
    (26, "part_cloak"),
];

/// What a body part hits with: any of these, the server choosing one for each blow.
const PART_DAMAGE: &[(i64, &str)] = &[
    (1, "damage_slash"),
    (2, "damage_pierce"),
    (4, "damage_bludgeon"),
    (8, "damage_cold"),
    (16, "damage_fire"),
    (32, "damage_acid"),
    (64, "damage_lightning"),
    (1024, "damage_nether"),
];

/// How high a body part is.
const HEIGHT: &[(i64, &str)] = &[(1, "height_high"), (2, "height_middle"), (3, "height_low")];

/// A body part's protections' columns, each with its kind of damage's key.
const PART_ARMOR: &[(&str, &str)] = &[
    ("armor_Vs_Slash", "damage_slash"),
    ("armor_Vs_Pierce", "damage_pierce"),
    ("armor_Vs_Bludgeon", "damage_bludgeon"),
    ("armor_Vs_Cold", "damage_cold"),
    ("armor_Vs_Fire", "damage_fire"),
    ("armor_Vs_Acid", "damage_acid"),
    ("armor_Vs_Electric", "damage_lightning"),
    ("armor_Vs_Nether", "damage_nether"),
];

/// Where a body part is hit: its chance of each blow, by height, side and facing.
const PART_HITS: &[(&str, &str)] = &[
    ("h_l_f", "hit_high_left_front"),
    ("m_l_f", "hit_middle_left_front"),
    ("l_l_f", "hit_low_left_front"),
    ("h_r_f", "hit_high_right_front"),
    ("m_r_f", "hit_middle_right_front"),
    ("l_r_f", "hit_low_right_front"),
    ("h_l_b", "hit_high_left_back"),
    ("m_l_b", "hit_middle_left_back"),
    ("l_l_b", "hit_low_left_back"),
    ("h_r_b", "hit_high_right_back"),
    ("m_r_b", "hit_middle_right_back"),
    ("l_r_b", "hit_low_right_back"),
];

/// Whether body part `row` hits with what it names: only a part with damage is used to attack,
/// and its kind of damage goes with that.
#[must_use]
pub fn hits(row: &TableRow) -> bool {
    row.number("d_Val").is_some_and(|d| d != 0.0)
}

/// A new body part's row: each column as `typical` (the server's weenies' commonest row of the
/// part) has it, else as `like` (the weenie's last part) has it, else nothing (and middle
/// height).
#[must_use]
pub fn new_part(typical: Option<&[(String, String)]>, like: Option<&TableRow>) -> TableRow {
    let mut r = TableRow::default();
    for c in super::form::columns(BODY_PARTS).iter().skip(1) {
        let v = match typical {
            Some(cells) => cells.iter().find(|(k, _)| k == c).map(|(_, v)| v.clone()),
            None => like.and_then(|l| l.get(c)).map(str::to_owned),
        };
        r.set(
            c,
            v.unwrap_or_else(|| if *c == "b_h" { "2".into() } else { "0".into() }),
        );
    }
    r
}

/// The words of `table`'s rows of `doc` named by `names`: each one's starting level.
fn levels_words(
    doc: &SqlDoc,
    table: &str,
    names: &[(u32, &str)],
    t: &crate::text::Table,
) -> String {
    let rows = rows_of(doc, table);
    names
        .iter()
        .filter_map(|(ty, key)| {
            let r = rows
                .iter()
                .find(|r| r.number("type") == Some(f64::from(*ty)))?;
            let level = content::bare(r.get("init_Level")?);
            Some(format!("{} {level}", t.get(&format!("fields.{key}"))))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The words of `doc`'s attributes: each one's name and starting level.
#[must_use]
pub fn attribute_words(doc: &SqlDoc, t: &crate::text::Table) -> String {
    levels_words(doc, ATTRIBUTES, ATTRIBUTE_NAMES, t)
}

/// The words of `doc`'s vitals: each one's name and starting most.
#[must_use]
pub fn vital_words(doc: &SqlDoc, t: &crate::text::Table) -> String {
    levels_words(doc, VITALS, VITAL_NAMES, t)
}

/// How tall a picker's row of a weenie stands: its picture's height.
const PICKER_ROW: f32 = 24.0;

/// Weenie `w` as a list's row writes it: a server's class by its id, a pack's own by its token.
#[must_use]
pub fn written_as(w: &Weenie) -> String {
    match w {
        Weenie::Class(c) => c.to_string(),
        Weenie::Symbol(s) => format!("{{{{wcid:{}}}}}", s.rsplit(':').next().unwrap_or(s)),
    }
}

/// `rows`, with row `i`'s `column` set to what was typed (`None`: not a value of it), as an edit
/// of `table`.
fn cell_edit(
    table: &str,
    rows: &[TableRow],
    i: usize,
    column: &str,
    value: Option<String>,
    refused: &str,
) -> FormEdit {
    match (value, rows.get(i)) {
        (Some(v), Some(_)) => {
            let mut new = rows.to_vec();
            new[i].set(column, v);
            FormEdit::Rows {
                table: table.to_owned(),
                rows: new,
            }
        }
        _ => FormEdit::Refused(refused.to_owned()),
    }
}

/// A create list row of list `list` (`treasure`: one drawn by chance) making `written`.
fn create_row(list: Drill, treasure: bool, written: &str) -> TableRow {
    TableRow::of(&[
        (
            "destination_Type",
            destination_of(list, treasure).to_string(),
        ),
        ("weenie_Class_Id", written.to_owned()),
        ("stack_Size", "1".into()),
        ("palette", "0".into()),
        ("shade", "0".into()),
        ("try_To_Bond", "False".into()),
    ])
}

/// Rows `a` and `b` of `list` changed places (both made always, or of one set), the set closing
/// where it did.
pub fn reorder(list: &mut [TableRow], a: usize, b: usize) {
    if !may_swap(list, a, b) {
        return;
    }
    let s = sets(list)[a];
    list.swap(a, b);
    if let Some(s) = s {
        if !is_last(list, s) {
            let mine = members(list, s);
            close(list, &mine);
        }
    }
}

/// A picker's popout: it stays open while its search is clicked into and typed in, and closes on
/// a choice, a click outside it or Escape.
fn stays_open() -> MenuConfig {
    MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
}

impl EditorApp {
    /// A weenie a list's row names as written: its picture and name, a click on which shows it;
    /// class 0 as nothing.
    fn listed_weenie(
        &mut self,
        ui: &mut egui::Ui,
        written: &str,
        budget: &mut u32,
        out: &mut Vec<UiAction>,
    ) {
        if lyceum_foundry::sql_ids::number(written) == Some(0) {
            ui.weak(self.text.table().get("collections.nothing"));
            return;
        }
        self.weenie_row(ui, written, None, budget, out);
    }

    /// A button opening every weenie, searched by name or id, with its picture: the one clicked,
    /// as a list's row writes it.
    pub(super) fn weenie_picker(
        &mut self,
        ui: &mut egui::Ui,
        salt: &str,
        label: &str,
    ) -> Option<String> {
        let t = self.text.table();
        let mut chosen = None;
        let (r, _) = MenuButton::new(label).config(stays_open()).ui(ui, |ui| {
            ui.set_min_width(300.0);
            let search = super::panels::search_box(
                ui,
                &mut self.editor.picker_search,
                t.get("collections.search"),
            );
            self.editor.picker_search_field = Some(search.rect);
            let f = self.editor.picker_search.trim().to_lowercase();
            let list = self.picker_matches(&f);
            if list.is_empty() {
                ui.weak(t.get("collections.no_match"));
            }
            let mut budget = super::PICTURES_A_FRAME;
            egui::ScrollArea::vertical()
                .id_salt(("weenie picker", salt))
                .max_height(320.0)
                .auto_shrink([false, true])
                .show_rows(ui, PICKER_ROW, list.len(), |ui, rows| {
                    for i in rows.map(|r| list[r]) {
                        let e = self.weenies[i].clone();
                        let picture = e.setup.and_then(|s| self.picture(DataId(s), &mut budget));
                        ui.horizontal(|ui| {
                            match picture {
                                Some(id) => {
                                    ui.add(egui::Image::new((id, egui::vec2(24.0, 24.0))));
                                }
                                None => {
                                    let _ = ui.allocate_exact_size(
                                        egui::vec2(24.0, 24.0),
                                        egui::Sense::hover(),
                                    );
                                }
                            }
                            let r = ui.selectable_label(false, &e.name);
                            ui.weak(super::form::weenie_label(&e.weenie, &t));
                            let token = written_as(&e.weenie);
                            self.editor.picker_rows.push((token.clone(), r.rect));
                            if r.clicked() {
                                chosen = Some(token);
                                ui.close();
                            }
                        });
                    }
                });
        });
        self.editor.add_buttons.push((salt.to_owned(), r.rect));
        if chosen.is_some() {
            self.editor.picker_search.clear();
        }
        chosen
    }

    /// Every spell of the files' spell table, by name, each with its name in lower case: made
    /// once the table is read.
    fn spell_list(&mut self) -> Rc<Vec<super::editor_tab::SpellLine>> {
        let _ = self.spell_name(0);
        let n = self.spells.as_ref().map_or(0, |m| m.len());
        if let Some((of, all)) = &self.editor.spell_list {
            if *of == n {
                return Rc::clone(all);
            }
        }
        let mut all: Vec<super::editor_tab::SpellLine> = self
            .spells
            .as_ref()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (*k, v.clone(), v.to_lowercase()))
                    .collect()
            })
            .unwrap_or_default();
        all.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
        let all = Rc::new(all);
        self.editor.spell_list = Some((n, Rc::clone(&all)));
        all
    }

    /// The weenies a picker's search `f` (lower case) finds, by their place in the weenies
    /// listed: found again only when the search or the weenies change.
    fn picker_matches(&mut self, f: &str) -> Rc<Vec<usize>> {
        let n = self.weenies.len();
        if let Some((search, of, found)) = &self.editor.picker_found {
            if search == f && *of == n {
                return Rc::clone(found);
            }
        }
        let found: Rc<Vec<usize>> =
            Rc::new((0..n).filter(|i| self.weenies[*i].matches(f)).collect());
        self.editor.picker_found = Some((f.to_owned(), n, Rc::clone(&found)));
        found
    }

    /// A button opening every spell, searched by name or id: the one clicked.
    pub(super) fn spell_picker(
        &mut self,
        ui: &mut egui::Ui,
        salt: &str,
        label: &str,
    ) -> Option<u32> {
        let t = self.text.table();
        let mut chosen = None;
        let (r, _) = MenuButton::new(label).config(stays_open()).ui(ui, |ui| {
            ui.set_min_width(280.0);
            let search = super::panels::search_box(
                ui,
                &mut self.editor.picker_search,
                t.get("collections.search_spells"),
            );
            self.editor.picker_search_field = Some(search.rect);
            let f = self.editor.picker_search.trim().to_lowercase();
            let all = self.spell_list();
            let list: Vec<&super::editor_tab::SpellLine> = all
                .iter()
                .filter(|(id, _, lower)| f.is_empty() || lower.contains(&f) || id.to_string() == f)
                .collect();
            if list.is_empty() {
                ui.weak(t.get("collections.no_match"));
            }
            egui::ScrollArea::vertical()
                .id_salt(("spell picker", salt))
                .max_height(320.0)
                .auto_shrink([false, true])
                .show_rows(ui, ui.spacing().interact_size.y, list.len(), |ui, rows| {
                    for (id, name, _) in rows.map(|r| list[r]) {
                        ui.horizontal(|ui| {
                            let r = ui.selectable_label(false, name);
                            ui.weak(id.to_string());
                            self.editor.picker_rows.push((id.to_string(), r.rect));
                            if r.clicked() {
                                chosen = Some(*id);
                                ui.close();
                            }
                        });
                    }
                });
        });
        self.editor.add_buttons.push((salt.to_owned(), r.rect));
        if chosen.is_some() {
            self.editor.picker_search.clear();
        }
        chosen
    }

    /// What weenie `w` holds, wields or sells (`list`): each entry's picture and name and how
    /// many; for a creature, by chance in sets, each set's chances and what makes nothing, and
    /// for what it wields, each entry's color scheme and its shade (none on a treasure row); else
    /// its color and shade. Entries move up and down in their set, are taken away, and are added
    /// by a picker, alone or to a set.
    #[allow(clippy::too_many_lines)]
    pub(super) fn create_list_editor(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        list_of_it: Drill,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let d = list_of_it;
        let creature = Kind::of(doc).is_creature();
        let all = Rc::new(rows_of(doc, CREATE_LIST));
        let list: Rc<Vec<TableRow>> = Rc::new(
            all.iter()
                .filter(|r| member(d, creature)(r))
                .cloned()
                .collect(),
        );
        let set_of = if creature {
            sets(&list)
        } else {
            vec![None; list.len()]
        };
        // What a creature wields is what it wears: each entry's colors too, and each one's place
        // among the create list's rows, where the preview follows a shade being dragged.
        let looks = creature && d == Drill::Wields;
        // Whether each of a creature's Equipped and Items rows ends up on its corpse.
        let marks_drops = creature && matches!(d, Drill::Wields | Drill::Holds);
        let places: Vec<usize> = all
            .iter()
            .enumerate()
            .filter(|(_, r)| member(d, creature)(r))
            .map(|(j, _)| j)
            .collect();
        let sums = set_totals(&list);
        let last = set_of.iter().flatten().max().copied();
        let last_row = set_of.iter().rposition(|s| s.is_some() && *s == last);
        let write = {
            let all = Rc::clone(&all);
            move |new: &[TableRow]| FormEdit::Rows {
                table: CREATE_LIST.to_owned(),
                rows: splice(&all, member(d, creature), new),
            }
        };
        let refused = t.get("messages.not_a_value").to_owned();
        let prefix = format!(
            "list:{}",
            match d {
                Drill::Wields => "wields",
                Drill::Holds => "holds",
                _ => "sells",
            }
        );
        let mut budget = super::PICTURES_A_FRAME;
        egui::Grid::new((prefix.as_str(), w.clone()))
            .num_columns(4 + usize::from(looks) + usize::from(marks_drops))
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.weak(t.get("collections.what"));
                ui.weak(t.get("collections.how_many"));
                ui.weak(t.get(if creature {
                    "collections.chance"
                } else {
                    "collections.look"
                }));
                if looks {
                    ui.weak(t.get("collections.look"));
                }
                if marks_drops {
                    ui.weak(t.get("collections.drops"));
                }
                ui.label("");
                ui.end_row();
                let held = self.held_rows(w, &[prefix.as_str()]);
                let id = egui::Id::new(("list rows", prefix.as_str(), w.clone()));
                super::seen::grid_rows(
                    ui,
                    id,
                    list.len(),
                    30.0,
                    |i| held.contains(&i),
                    |ui, i| {
                        let r = &list[i];
                        // The first of a set: under its heading.
                        if let Some(s) = set_of[i] {
                            if i == 0 || set_of[i - 1] != Some(s) {
                                ui.strong(t.fill("collections.set", &[("n", &(s + 1))]));
                                ui.label("");
                                ui.label("");
                                let salt = format!("{prefix}:set:{s}");
                                if let Some(tok) =
                                    self.weenie_picker(ui, &salt, t.get("collections.add_to_set"))
                                {
                                    let mut new = list.to_vec();
                                    add_to_set(&mut new, s, create_row(d, true, &tok));
                                    out.push(UiAction::Form(w.clone(), write(&new)));
                                }
                                ui.end_row();
                            }
                        }
                        let class = r.get("weenie_Class_Id").unwrap_or("0").to_owned();
                        // As wide as the widest row it has shown, as rows scroll in and out.
                        super::seen::held_cell(ui, id.with("what"), |ui| {
                            ui.horizontal(|ui| {
                                if set_of[i].is_some() {
                                    ui.add_space(12.0);
                                }
                                self.listed_weenie(ui, &class, &mut budget, out);
                            });
                        });
                        let stack = r.get("stack_Size").map(content::bare).unwrap_or_default();
                        {
                            let (list, write, refused) =
                                (Rc::clone(&list), write.clone(), refused.clone());
                            self.field(
                                ui,
                                w,
                                &format!("{prefix}:{i}:stack"),
                                &stack,
                                40.0,
                                move |v| match whole_typed(&v) {
                                    Some(n) => {
                                        let mut new = list.to_vec();
                                        new[i].set("stack_Size", n);
                                        write(&new)
                                    }
                                    None => FormEdit::Refused(refused),
                                },
                                out,
                            );
                        }
                        match set_of[i] {
                            Some(_) => {
                                let now = percent_words(chance(r) * 100.0);
                                let (list, write, refused, tt) =
                                    (Rc::clone(&list), write.clone(), refused.clone(), t.clone());
                                ui.horizontal(|ui| {
                                    self.field(
                                        ui,
                                        w,
                                        &format!("{prefix}:{i}:chance"),
                                        &now,
                                        50.0,
                                        move |v| {
                                            let Some(p) = percent_typed(&v) else {
                                                return FormEdit::Refused(refused);
                                            };
                                            let mut new = list.to_vec();
                                            match set_chance(&mut new, i, p) {
                                                Ok(()) => write(&new),
                                                Err(e) => FormEdit::Refused(tt.fill(
                                                    "collections.too_likely",
                                                    &[("most", &percent_words(e.most))],
                                                )),
                                            }
                                        },
                                        out,
                                    );
                                    ui.weak("%");
                                });
                            }
                            None if creature => {
                                ui.weak(t.get("collections.always"));
                            }
                            None => {
                                ui.horizontal(|ui| {
                                    for (column, width) in [("palette", 36.0), ("shade", 44.0)] {
                                        let now =
                                            r.get(column).map(content::bare).unwrap_or_default();
                                        let (list, write, refused) =
                                            (Rc::clone(&list), write.clone(), refused.clone());
                                        ui.weak(t.get(&format!("collections.{column}")));
                                        self.field(
                                            ui,
                                            w,
                                            &format!("{prefix}:{i}:{column}"),
                                            &now,
                                            width,
                                            move |v| {
                                                let typed = if column == "palette" {
                                                    whole_typed(&v)
                                                } else {
                                                    decimal_typed(&v)
                                                };
                                                match typed {
                                                    Some(x) => {
                                                        let mut new = list.to_vec();
                                                        new[i].set(column, x);
                                                        write(&new)
                                                    }
                                                    None => FormEdit::Refused(refused),
                                                }
                                            },
                                            out,
                                        );
                                    }
                                });
                            }
                        }
                        if looks {
                            if is_nothing(r) {
                                ui.label("");
                            } else {
                                let palette = r.bits("palette");
                                let shade = r.number("shade").unwrap_or(0.0);
                                // Its swatches follow its shade as it is dragged.
                                let shown = self
                                    .appearance
                                    .row_shade_held
                                    .as_ref()
                                    .filter(|(hw, j, _)| hw == w && *j == places[i])
                                    .map_or(shade, |(.., s)| *s);
                                let edit_of = |column: &'static str| {
                                    let (list, write) = (Rc::clone(&list), write.clone());
                                    move |v: String| {
                                        let mut new = list.to_vec();
                                        new[i].set(column, v);
                                        write(&new)
                                    }
                                };
                                let (palette_edit, shade_edit) =
                                    (edit_of("palette"), edit_of("shade"));
                                ui.horizontal(|ui| {
                                    self.row_scheme_widget(
                                        ui,
                                        w,
                                        &format!("{prefix}:{i}:palette"),
                                        &class,
                                        (palette, shades(r).then_some(shown)),
                                        &palette_edit,
                                        out,
                                    );
                                    if shades(r) {
                                        self.row_shade_widget(
                                            ui,
                                            w,
                                            &format!("{prefix}:{i}:shade"),
                                            places[i],
                                            shade,
                                            &shade_edit,
                                            out,
                                        );
                                    }
                                });
                            }
                        }
                        if marks_drops && is_nothing(r) {
                            ui.label("");
                        } else if marks_drops {
                            // A thing destroyed on death never drops.
                            let destroyed = self
                                .weenie_written(&class)
                                .and_then(|e| self.weenie_entry(&e).and_then(|e| e.bonded))
                                == Some(-2);
                            let mut on = drops(d, r.bits("destination_Type"), destroyed);
                            ui.add_enabled(false, egui::Checkbox::without_text(&mut on));
                            self.editor.drop_marks.push((format!("{prefix}:{i}"), on));
                        }
                        ui.horizontal(|ui| {
                            let up = i > 0 && may_swap(&list, i - 1, i);
                            if ui.add_enabled(up, egui::Button::new("▲").small()).clicked() {
                                let mut new = list.to_vec();
                                reorder(&mut new, i - 1, i);
                                out.push(UiAction::Form(w.clone(), write(&new)));
                            }
                            let down = may_swap(&list, i, i + 1);
                            if ui
                                .add_enabled(down, egui::Button::new("▼").small())
                                .clicked()
                            {
                                let mut new = list.to_vec();
                                reorder(&mut new, i, i + 1);
                                out.push(UiAction::Form(w.clone(), write(&new)));
                            }
                            let x = ui.small_button("×");
                            self.editor
                                .row_buttons
                                .push((format!("{prefix}:{i}:remove"), x.rect));
                            if x.clicked() {
                                let mut new = list.to_vec();
                                if creature {
                                    remove_from(&mut new, i);
                                } else {
                                    new.remove(i);
                                }
                                out.push(UiAction::Form(w.clone(), write(&new)));
                            }
                        });
                        ui.end_row();
                        // The rest of a last set short of 100% makes nothing.
                        if Some(i) == last_row {
                            if let Some(s) = last {
                                if sums[s] < 1.0 - 1e-6 {
                                    ui.horizontal(|ui| {
                                        ui.add_space(12.0);
                                        ui.weak(t.get("collections.nothing_rest"));
                                    });
                                    ui.label("");
                                    ui.weak(format!("{}%", percent_words((1.0 - sums[s]) * 100.0)));
                                    ui.label("");
                                    ui.end_row();
                                }
                            }
                        }
                    },
                );
            });
        if list.is_empty() {
            ui.weak(t.get("collections.empty"));
        }
        ui.horizontal(|ui| {
            if let Some(tok) =
                self.weenie_picker(ui, &format!("{prefix}:add"), t.get("collections.add"))
            {
                let mut new = list.to_vec();
                new.push(create_row(d, false, &tok));
                out.push(UiAction::Form(w.clone(), write(&new)));
            }
            if creature && d != Drill::Sells {
                if let Some(tok) = self.weenie_picker(
                    ui,
                    &format!("{prefix}:add-set"),
                    t.get("collections.add_set"),
                ) {
                    let mut new = list.to_vec();
                    add_set(&mut new, create_row(d, true, &tok));
                    out.push(UiAction::Form(w.clone(), write(&new)));
                }
            }
        });
    }

    /// What weenie `w` spawns: each entry's picture and name, its share of the draw in percent
    /// (or always), how many at first and at most, how long before it is made again, when and
    /// where. Entries move up and down, are taken away, and are added by a picker.
    #[allow(clippy::too_many_lines)]
    pub(super) fn generator_editor(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let rows = Rc::new(rows_of(doc, GENERATOR));
        let written: Vec<f64> = rows
            .iter()
            .map(|r| r.number("probability").unwrap_or(1.0))
            .collect();
        let draw = Rc::new(draws(&written));
        let shares = percents(&draw);
        let write = |rows: &[TableRow], d: &[Draw]| {
            let mut rows = rows.to_vec();
            for (r, total) in rows.iter_mut().zip(totals(d)) {
                if r.number("probability")
                    .is_none_or(|p| (p - total).abs() > 1e-9)
                {
                    r.set("probability", decimal(total));
                }
            }
            FormEdit::Rows {
                table: GENERATOR.to_owned(),
                rows,
            }
        };
        let refused = t.get("messages.not_a_value").to_owned();
        let joins = {
            let t = t.clone();
            move |e: JoinsSets| {
                t.fill(
                    "collections.joins_sets",
                    &[("n", &e.set), ("before", &e.before)],
                )
            }
        };
        let (set_of, set_totals) = set_shares(&draw);
        let many_sets = set_totals.len() > 1;
        let schema = self.schema.clone();
        let mut budget = super::PICTURES_A_FRAME;
        egui::ScrollArea::horizontal()
            .id_salt(("spawns scroll", w.clone()))
            .auto_shrink([false, true])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
                egui::Grid::new(("spawns", w.clone()))
                    .num_columns(9)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        for k in [
                            "collections.what",
                            "collections.share",
                            "collections.always",
                            "collections.first",
                            "collections.most",
                            "collections.again_after",
                            "collections.when",
                            "collections.where",
                        ] {
                            ui.weak(t.get(k));
                        }
                        ui.label("");
                        ui.end_row();
                        // A set's heading, when there are several, over its first entry.
                        let mut last = None;
                        let heads: Vec<Option<usize>> = set_of
                            .iter()
                            .map(|s| {
                                let s = s.filter(|s| many_sets && last != Some(*s))?;
                                last = Some(s);
                                Some(s)
                            })
                            .collect();
                        let held = self.held_rows(w, &["spawn"]);
                        let id = egui::Id::new(("spawn rows", w.clone()));
                        let n = rows.len();
                        super::seen::grid_rows(
                            ui,
                            id,
                            n,
                            30.0,
                            |i| held.contains(&i),
                            |ui, i| {
                                let r = &rows[i];
                                if let Some(s) = heads[i] {
                                    ui.strong(t.fill("collections.set", &[("n", &(s + 1))]));
                                    ui.end_row();
                                }
                                let class = r.get("weenie_Class_Id").unwrap_or("0").to_owned();
                                // An entry that makes treasure names a loot table, not a weenie;
                                // as wide as the widest row it has shown.
                                let treasure = r.bits("where_Create")
                                    & lyceum_foundry::sql_ids::TREASURE_PLACE
                                    != 0;
                                super::seen::held_cell(ui, id.with("what"), |ui| {
                                    if treasure {
                                        self.treasure_link(ui, &class, out);
                                    } else {
                                        self.listed_weenie(ui, &class, &mut budget, out);
                                    }
                                });
                                // Its share, or always.
                                ui.horizontal(|ui| {
                                    if let Some(p) = shares[i] {
                                        let (rows, draw, refused, joins) = (
                                            Rc::clone(&rows),
                                            Rc::clone(&draw),
                                            refused.clone(),
                                            joins.clone(),
                                        );
                                        self.field(
                                            ui,
                                            w,
                                            &format!("spawn:{i}:share"),
                                            &percent_words(p),
                                            46.0,
                                            move |v| match percent_typed(&v) {
                                                Some(p) => {
                                                    let mut d = draw.to_vec();
                                                    match set_share(&mut d, i, p) {
                                                        Ok(()) => write(&rows, &d),
                                                        Err(e) => FormEdit::Refused(joins(e)),
                                                    }
                                                }
                                                None => FormEdit::Refused(refused),
                                            },
                                            out,
                                        );
                                        ui.weak("%");
                                    }
                                });
                                ui.horizontal(|ui| {
                                    let mut always = shares[i].is_none();
                                    let r = ui.checkbox(&mut always, "");
                                    self.editor
                                        .row_buttons
                                        .push((format!("spawn:{i}:always"), r.rect));
                                    if r.changed() {
                                        let mut d = draw.to_vec();
                                        let done = if always {
                                            set_always(&mut d, i)
                                        } else {
                                            let n = shares.iter().flatten().count();
                                            #[allow(clippy::cast_precision_loss)]
                                            // a generator's few entries
                                            set_share(&mut d, i, 100.0 / (n as f64 + 1.0))
                                        };
                                        let edit = match done {
                                            Ok(()) => write(&rows, &d),
                                            Err(e) => FormEdit::Refused(joins(e)),
                                        };
                                        out.push(UiAction::Form(w.clone(), edit));
                                    }
                                });
                                for (column, width, decimal_column) in [
                                    ("init_Create", 36.0, false),
                                    ("max_Create", 36.0, false),
                                    ("delay", 50.0, true),
                                ] {
                                    let now = r
                                        .get(column)
                                        .map(content::bare)
                                        .filter(|v| v != "NULL")
                                        .unwrap_or_default();
                                    let (rows, refused) = (Rc::clone(&rows), refused.clone());
                                    self.field(
                                        ui,
                                        w,
                                        &format!("spawn:{i}:{column}"),
                                        &now,
                                        width,
                                        move |v| {
                                            let typed = if decimal_column {
                                                decimal_typed(&v)
                                            } else {
                                                whole_typed(&v)
                                            };
                                            cell_edit(GENERATOR, &rows, i, column, typed, &refused)
                                        },
                                        out,
                                    );
                                }
                                // When it is made again: one of two.
                                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                                let when = r.number("when_Create").map_or(0, |n| n as u32);
                                let again = Again::of(when);
                                let shown = again
                                    .key()
                                    .map_or_else(|| when.to_string(), |k| t.get(k).to_owned());
                                let combo =
                                    egui::ComboBox::from_id_salt(("spawn when", i, w.clone()))
                                        .width(110.0)
                                        .selected_text(shown)
                                        .show_ui(ui, |ui| {
                                            for (choice, tag) in
                                                [(Again::Gone, "gone"), (Again::Death, "death")]
                                            {
                                                let words = choice.key().map_or("", |k| t.get(k));
                                                let r = ui.selectable_label(again == choice, words);
                                                self.editor.picker_rows.push((
                                                    format!("spawn:{i}:when:{tag}"),
                                                    r.rect,
                                                ));
                                                if let (true, Some(v)) =
                                                    (r.clicked(), choice.written(when))
                                                {
                                                    out.push(UiAction::Form(
                                                        w.clone(),
                                                        cell_edit(
                                                            GENERATOR,
                                                            &rows,
                                                            i,
                                                            "when_Create",
                                                            Some(v.to_string()),
                                                            &refused,
                                                        ),
                                                    ));
                                                }
                                            }
                                        });
                                self.editor
                                    .add_buttons
                                    .push((format!("spawn:{i}:when"), combo.response.rect));
                                for (column, e) in [("where_Create", "RegenLocationType")] {
                                    let values = super::fields::enum_values(schema.as_ref(), e);
                                    #[allow(
                                        clippy::cast_possible_truncation,
                                        clippy::cast_sign_loss
                                    )]
                                    let now = r.number(column).map(|n| n as u32);
                                    if values.is_empty() {
                                        let (rows, refused) = (Rc::clone(&rows), refused.clone());
                                        self.field(
                                            ui,
                                            w,
                                            &format!("spawn:{i}:{column}"),
                                            &now.map(|n| n.to_string()).unwrap_or_default(),
                                            36.0,
                                            move |v| {
                                                cell_edit(
                                                    GENERATOR,
                                                    &rows,
                                                    i,
                                                    column,
                                                    whole_typed(&v),
                                                    &refused,
                                                )
                                            },
                                            out,
                                        );
                                        continue;
                                    }
                                    let shown =
                                        values.iter().find(|(v, _)| Some(*v) == now).map_or_else(
                                            || now.map(|n| n.to_string()).unwrap_or_default(),
                                            |(_, n)| n.clone(),
                                        );
                                    egui::ComboBox::from_id_salt(("spawn", i, column, w.clone()))
                                        .width(84.0)
                                        .selected_text(shown)
                                        .height(260.0)
                                        .show_ui(ui, |ui| {
                                            for (v, words) in &values {
                                                if ui
                                                    .selectable_label(now == Some(*v), words)
                                                    .clicked()
                                                    && now != Some(*v)
                                                {
                                                    out.push(UiAction::Form(
                                                        w.clone(),
                                                        cell_edit(
                                                            GENERATOR,
                                                            &rows,
                                                            i,
                                                            column,
                                                            Some(v.to_string()),
                                                            &refused,
                                                        ),
                                                    ));
                                                }
                                            }
                                        });
                                }
                                ui.horizontal(|ui| {
                                    let same = |a: usize, b: usize| match (draw.get(a), draw.get(b))
                                    {
                                        (Some(Draw::Always), Some(Draw::Always)) => true,
                                        (
                                            Some(Draw::Chance { set: x, .. }),
                                            Some(Draw::Chance { set: y, .. }),
                                        ) => x == y,
                                        _ => false,
                                    };
                                    for (to, enabled, glyph) in [
                                        (i.wrapping_sub(1), i > 0 && same(i - 1, i), "▲"),
                                        (i + 1, same(i, i + 1), "▼"),
                                    ] {
                                        if ui
                                            .add_enabled(enabled, egui::Button::new(glyph).small())
                                            .clicked()
                                        {
                                            let mut new = rows.to_vec();
                                            let mut d = draw.to_vec();
                                            new.swap(i, to);
                                            let edit = match swap_draws(&mut d, i, to) {
                                                Ok(()) => write(&new, &d),
                                                Err(e) => FormEdit::Refused(joins(e)),
                                            };
                                            out.push(UiAction::Form(w.clone(), edit));
                                        }
                                    }
                                    let x = ui.small_button("×");
                                    self.editor
                                        .row_buttons
                                        .push((format!("spawn:{i}:remove"), x.rect));
                                    if x.clicked() {
                                        let mut new = rows.to_vec();
                                        let mut d = draw.to_vec();
                                        new.remove(i);
                                        let edit = match remove_draw(&mut d, i) {
                                            Ok(()) => write(&new, &d),
                                            Err(e) => FormEdit::Refused(joins(e)),
                                        };
                                        out.push(UiAction::Form(w.clone(), edit));
                                    }
                                });
                                ui.end_row();
                            },
                        );
                    });
            });
        if rows.is_empty() {
            ui.weak(t.get("collections.empty"));
        }
        if let Some(tok) = self.weenie_picker(ui, "spawn:add", t.get("collections.add")) {
            let mut new = rows.to_vec();
            new.push(TableRow::of(&[
                ("probability", "1".into()),
                ("weenie_Class_Id", tok),
                ("delay", "180".into()),
                ("init_Create", "1".into()),
                ("max_Create", "1".into()),
                ("when_Create", "1".into()),
                ("where_Create", "2".into()),
            ]));
            let mut d = draw.to_vec();
            let edit = match add_draw(&mut d) {
                Ok(()) => write(&new, &d),
                Err(e) => FormEdit::Refused(joins(e)),
            };
            out.push(UiAction::Form(w.clone(), edit));
        }
    }

    /// Weenie `w`'s spells, each by name; a creature's with the chance it casts it, in percent.
    /// Each is taken away, and more are added by a picker.
    pub(super) fn spell_book_editor(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let creature = Kind::of(doc).is_creature();
        let rows = Rc::new(rows_of(doc, SPELL_BOOK));
        let refused = t.get("messages.not_a_value").to_owned();
        egui::Grid::new(("spell book", w.clone()))
            .num_columns(3)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.weak(t.get("collections.spell"));
                ui.weak(if creature {
                    t.get("collections.cast_chance")
                } else {
                    ""
                });
                ui.label("");
                ui.end_row();
                for (i, r) in rows.iter().enumerate() {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let id = r.number("spell").map(|n| n as u32);
                    let name = id.and_then(|s| self.spell_name(s));
                    ui.horizontal(|ui| {
                        ui.label(
                            name.unwrap_or_else(|| t.get("collections.unknown_spell").to_owned()),
                        );
                        ui.weak(id.map(|n| n.to_string()).unwrap_or_default());
                    });
                    if creature {
                        let now =
                            percent_words(cast_chance(r.number("probability").unwrap_or(2.0)));
                        let (rows, refused) = (Rc::clone(&rows), refused.clone());
                        ui.horizontal(|ui| {
                            self.field(
                                ui,
                                w,
                                &format!("spell:{i}:chance"),
                                &now,
                                46.0,
                                move |v| {
                                    let p = percent_typed(&v).map(|p| decimal(cast_probability(p)));
                                    cell_edit(SPELL_BOOK, &rows, i, "probability", p, &refused)
                                },
                                out,
                            );
                            ui.weak("%");
                        });
                    } else {
                        ui.label("");
                    }
                    let x = ui.small_button("×");
                    self.editor
                        .row_buttons
                        .push((format!("spell:{i}:remove"), x.rect));
                    if x.clicked() {
                        let mut new = rows.to_vec();
                        new.remove(i);
                        out.push(UiAction::Form(
                            w.clone(),
                            FormEdit::Rows {
                                table: SPELL_BOOK.to_owned(),
                                rows: new,
                            },
                        ));
                    }
                    ui.end_row();
                }
            });
        if rows.is_empty() {
            ui.weak(t.get("collections.empty"));
        }
        if let Some(id) = self.spell_picker(ui, "spell:add", t.get("collections.add_spell")) {
            let mut new = rows.to_vec();
            new.push(TableRow::of(&[
                ("spell", id.to_string()),
                ("probability", "2".into()),
            ]));
            out.push(UiAction::Form(
                w.clone(),
                FormEdit::Rows {
                    table: SPELL_BOOK.to_owned(),
                    rows: new,
                },
            ));
        }
    }

    /// The comment a row of `table` of type `ty` is written with: the server's name for it.
    fn row_name(&self, e: &str, ty: u32) -> Option<String> {
        self.schema
            .as_ref()
            .and_then(|s| s.value(e, ty))
            .map(str::to_owned)
    }

    /// Weenie `w`'s attributes: each one's starting level, a row added for one it has none of.
    pub(super) fn attributes_editor(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let rows = Rc::new(rows_of(doc, ATTRIBUTES));
        let refused = t.get("messages.not_a_value").to_owned();
        egui::Grid::new(("attributes", w.clone()))
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                for (ty, key) in ATTRIBUTE_NAMES {
                    let at = rows
                        .iter()
                        .position(|r| r.number("type") == Some(f64::from(*ty)));
                    let now = at
                        .and_then(|i| rows[i].get("init_Level"))
                        .map(content::bare)
                        .unwrap_or_default();
                    ui.label(t.get(&format!("fields.{key}")));
                    let (rows, refused) = (Rc::clone(&rows), refused.clone());
                    let comment = self.row_name("Attribute", *ty);
                    let ty = *ty;
                    self.field(
                        ui,
                        w,
                        &format!("attribute:{ty}"),
                        &now,
                        60.0,
                        move |v| {
                            let Some(n) = whole_typed(&v) else {
                                return FormEdit::Refused(refused);
                            };
                            let mut new = rows.to_vec();
                            match at {
                                Some(i) => new[i].set("init_Level", n),
                                None => {
                                    let mut row = TableRow::of(&[
                                        ("type", ty.to_string()),
                                        ("init_Level", n),
                                        ("level_From_C_P", "0".into()),
                                        ("c_P_Spent", "0".into()),
                                    ]);
                                    row.comment = comment;
                                    let place = new
                                        .iter()
                                        .position(|r| {
                                            r.number("type").unwrap_or(0.0) > f64::from(ty)
                                        })
                                        .unwrap_or(new.len());
                                    new.insert(place, row);
                                }
                            }
                            FormEdit::Rows {
                                table: ATTRIBUTES.to_owned(),
                                rows: new,
                            }
                        },
                        out,
                    );
                    ui.end_row();
                }
            });
    }

    /// Weenie `w`'s vitals: each one's starting most and what it has now, a row added for one it
    /// has none of (starting full).
    pub(super) fn vitals_editor(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let rows = Rc::new(rows_of(doc, VITALS));
        let refused = t.get("messages.not_a_value").to_owned();
        egui::Grid::new(("vitals", w.clone()))
            .num_columns(3)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("");
                ui.weak(t.get("collections.vital_most"));
                ui.weak(t.get("collections.vital_now"));
                ui.end_row();
                for (ty, key) in VITAL_NAMES {
                    let at = rows
                        .iter()
                        .position(|r| r.number("type") == Some(f64::from(*ty)));
                    ui.label(t.get(&format!("fields.{key}")));
                    for (column, part) in [("init_Level", "base"), ("current_Level", "now")] {
                        let now = at
                            .and_then(|i| rows[i].get(column))
                            .map(content::bare)
                            .unwrap_or_default();
                        let (rows, refused) = (Rc::clone(&rows), refused.clone());
                        let comment = self.row_name("Vital", *ty);
                        let ty = *ty;
                        self.field(
                            ui,
                            w,
                            &format!("vital:{ty}:{part}"),
                            &now,
                            60.0,
                            move |v| {
                                let Some(n) = whole_typed(&v) else {
                                    return FormEdit::Refused(refused);
                                };
                                let mut new = rows.to_vec();
                                match at {
                                    Some(i) => new[i].set(column, n),
                                    None => {
                                        let mut row = TableRow::of(&[
                                            ("type", ty.to_string()),
                                            ("init_Level", n.clone()),
                                            ("level_From_C_P", "0".into()),
                                            ("c_P_Spent", "0".into()),
                                            ("current_Level", n),
                                        ]);
                                        row.comment = comment;
                                        let place = new
                                            .iter()
                                            .position(|r| {
                                                r.number("type").unwrap_or(0.0) > f64::from(ty)
                                            })
                                            .unwrap_or(new.len());
                                        new.insert(place, row);
                                    }
                                }
                                FormEdit::Rows {
                                    table: VITALS.to_owned(),
                                    rows: new,
                                }
                            },
                            out,
                        );
                    }
                    ui.end_row();
                }
            });
    }

    /// Weenie `w`'s skills: each by name, how trained and its level; taken away, and added from
    /// the skills it lacks.
    #[allow(clippy::too_many_lines)]
    pub(super) fn skills_editor(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let rows = Rc::new(rows_of(doc, SKILLS));
        let refused = t.get("messages.not_a_value").to_owned();
        let names = super::fields::enum_values(self.schema.as_ref(), "Skill");
        let name_of = |ty: u32| {
            names.iter().find(|(v, _)| *v == ty).map_or_else(
                || t.fill("collections.skill_n", &[("n", &ty)]),
                |(_, n)| n.clone(),
            )
        };
        // The names in a column as wide as the longest a skill usually has, whatever skills
        // the weenie has.
        let name_width = super::seen::widest(ui, [super::seen::LONGEST_SKILL]);
        egui::Grid::new(("skills", w.clone()))
            .num_columns(4)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.weak(t.get("collections.skill"));
                ui.weak(t.get("collections.training"));
                ui.weak(t.get("collections.level"));
                ui.label("");
                ui.end_row();
                for (i, r) in rows.iter().enumerate() {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let ty = r.number("type").map_or(0, |n| n as u32);
                    super::seen::name_cell(ui, name_of(ty), name_width);
                    #[allow(clippy::cast_possible_truncation)]
                    let sac = r.number("s_a_c").map(|n| n as i64);
                    let shown = TRAINING.iter().find(|(v, _)| Some(*v) == sac).map_or_else(
                        || sac.map(|n| n.to_string()).unwrap_or_default(),
                        |(_, k)| t.get(&format!("collections.{k}")).to_owned(),
                    );
                    egui::ComboBox::from_id_salt(("training", i, w.clone()))
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            for (v, k) in TRAINING {
                                if ui
                                    .selectable_label(
                                        sac == Some(*v),
                                        t.get(&format!("collections.{k}")),
                                    )
                                    .clicked()
                                    && sac != Some(*v)
                                {
                                    out.push(UiAction::Form(
                                        w.clone(),
                                        cell_edit(
                                            SKILLS,
                                            &rows,
                                            i,
                                            "s_a_c",
                                            Some(v.to_string()),
                                            &refused,
                                        ),
                                    ));
                                }
                            }
                        });
                    let now = r.get("init_Level").map(content::bare).unwrap_or_default();
                    let (rows_c, refused_c) = (Rc::clone(&rows), refused.clone());
                    self.field(
                        ui,
                        w,
                        &format!("skill:{ty}:level"),
                        &now,
                        50.0,
                        move |v| {
                            cell_edit(
                                SKILLS,
                                &rows_c,
                                i,
                                "init_Level",
                                whole_typed(&v),
                                &refused_c,
                            )
                        },
                        out,
                    );
                    let x = ui.small_button("×");
                    self.editor
                        .row_buttons
                        .push((format!("skill:{ty}:remove"), x.rect));
                    if x.clicked() {
                        let mut new = rows.to_vec();
                        new.remove(i);
                        out.push(UiAction::Form(
                            w.clone(),
                            FormEdit::Rows {
                                table: SKILLS.to_owned(),
                                rows: new,
                            },
                        ));
                    }
                    ui.end_row();
                }
            });
        if rows.is_empty() {
            ui.weak(t.get("collections.empty"));
        }
        // Another skill: one of those it lacks, by name; by number without the server's names.
        let has: Vec<u32> = rows
            .iter()
            .filter_map(|r| r.number("type"))
            .map(|n| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let n = n as u32;
                n
            })
            .collect();
        let add = |ty: u32| {
            let mut new = rows.to_vec();
            let mut row = TableRow::of(&[
                ("type", ty.to_string()),
                ("level_From_P_P", "0".into()),
                ("s_a_c", "2".into()),
                ("p_p", "0".into()),
                ("init_Level", "0".into()),
                ("resistance_At_Last_Check", "0".into()),
                ("last_Used_Time", "0".into()),
            ]);
            row.comment = names.iter().find(|(v, _)| *v == ty).map(|(_, n)| n.clone());
            let place = new
                .iter()
                .position(|r| r.number("type").unwrap_or(0.0) > f64::from(ty))
                .unwrap_or(new.len());
            new.insert(place, row);
            FormEdit::Rows {
                table: SKILLS.to_owned(),
                rows: new,
            }
        };
        if names.is_empty() {
            ui.horizontal(|ui| {
                ui.weak(t.get("collections.add_skill_number"));
                let key = super::form::typing_key(w, "skill:add");
                let mut typed = self
                    .panels
                    .form_typing
                    .get(&key)
                    .cloned()
                    .unwrap_or_default();
                let r = ui.add(egui::TextEdit::singleline(&mut typed).desired_width(50.0));
                self.panels.form_typing.insert(key.clone(), typed.clone());
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Some(n) = typed
                        .trim()
                        .parse::<u32>()
                        .ok()
                        .filter(|n| !has.contains(n))
                    {
                        out.push(UiAction::Form(w.clone(), add(n)));
                    }
                    self.panels.form_typing.remove(&key);
                }
            });
        } else {
            let r = egui::ComboBox::from_id_salt(("add skill", w.clone()))
                .selected_text(t.get("collections.add_skill"))
                .height(320.0)
                .show_ui(ui, |ui| {
                    for (v, words) in &names {
                        if *v == 0 || has.contains(v) {
                            continue;
                        }
                        if ui.selectable_label(false, words).clicked() {
                            out.push(UiAction::Form(w.clone(), add(*v)));
                        }
                    }
                });
            self.editor
                .add_buttons
                .push(("skill:add".to_owned(), r.response.rect));
        }
    }

    /// Weenie `w`'s body: each part's name, how hard it hits and with what, its armor and its
    /// protection against each kind of damage, and how high it is; under them, where each part is
    /// hit. A part is taken away, and another added as the server's weenies typically have it
    /// (else like the last one).
    #[allow(clippy::too_many_lines)]
    pub(super) fn body_parts_editor(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let rows = Rc::new(rows_of(doc, BODY_PARTS));
        let refused = t.get("messages.not_a_value").to_owned();
        let part_name = |key: u32| {
            BODY_PART_NAMES.iter().find(|(k, _)| *k == key).map_or_else(
                || t.fill("collections.part_n", &[("n", &key)]),
                |(_, n)| t.get(&format!("collections.{n}")).to_owned(),
            )
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let key_of = |r: &TableRow| r.number("key").map_or(0, |n| n as u32);
        // A number field of row `i`'s `column`, whole or decimal.
        let number = |this: &mut Self,
                      ui: &mut egui::Ui,
                      i: usize,
                      column: &'static str,
                      decimal_column: bool,
                      width: f32,
                      out: &mut Vec<UiAction>| {
            let now = rows[i].get(column).map(content::bare).unwrap_or_default();
            let (rows, refused) = (Rc::clone(&rows), refused.clone());
            let key = key_of(&rows[i]);
            this.field(
                ui,
                w,
                &format!("part:{key}:{column}"),
                &now,
                width,
                move |v| {
                    let typed = if decimal_column {
                        decimal_typed(&v)
                    } else {
                        whole_typed(&v)
                    };
                    cell_edit(BODY_PARTS, &rows, i, column, typed, &refused)
                },
                out,
            );
        };
        // A choice of row `i`'s `column`.
        let choice = |ui: &mut egui::Ui,
                      i: usize,
                      column: &'static str,
                      choices: &[(i64, &str)],
                      out: &mut Vec<UiAction>| {
            #[allow(clippy::cast_possible_truncation)]
            let now = rows[i].number(column).map(|n| n as i64);
            let shown = choices.iter().find(|(v, _)| Some(*v) == now).map_or_else(
                || now.map(|n| n.to_string()).unwrap_or_default(),
                |(_, k)| t.get(&format!("fields.{k}")).to_owned(),
            );
            egui::ComboBox::from_id_salt(("part", i, column, w.clone()))
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (v, k) in choices {
                        if ui
                            .selectable_label(now == Some(*v), t.get(&format!("fields.{k}")))
                            .clicked()
                            && now != Some(*v)
                        {
                            out.push(UiAction::Form(
                                w.clone(),
                                cell_edit(
                                    BODY_PARTS,
                                    &rows,
                                    i,
                                    column,
                                    Some(v.to_string()),
                                    &refused,
                                ),
                            ));
                        }
                    }
                });
        };
        // Any of row `i`'s `column`'s bits, each a check box in a dropdown; closed, it names
        // those ticked. Each tick is a step of its own.
        let ticks = |ui: &mut egui::Ui,
                     i: usize,
                     column: &'static str,
                     bits: &[(i64, &str)],
                     out: &mut Vec<UiAction>| {
            let now = rows[i].bits(column);
            let on: Vec<String> = bits
                .iter()
                .filter(|(b, _)| now & super::kinds::int_bits(*b) == super::kinds::int_bits(*b))
                .map(|(_, k)| t.get(&format!("fields.{k}")).to_owned())
                .collect();
            egui::ComboBox::from_id_salt(("part", i, column, w.clone()))
                .selected_text(super::fields::chosen_words(&on, &t))
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show_ui(ui, |ui| {
                    for (b, k) in bits {
                        let bit = super::kinds::int_bits(*b);
                        let mut on = now & bit == bit;
                        if ui
                            .checkbox(&mut on, t.get(&format!("fields.{k}")))
                            .changed()
                        {
                            let v = if on { now | bit } else { now & !bit };
                            out.push(UiAction::Form(
                                w.clone(),
                                cell_edit(
                                    BODY_PARTS,
                                    &rows,
                                    i,
                                    column,
                                    Some(v.cast_signed().to_string()),
                                    &refused,
                                ),
                            ));
                        }
                    }
                });
        };
        egui::ScrollArea::horizontal()
            .id_salt(("body parts", w.clone()))
            .auto_shrink([false, true])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
                egui::Grid::new(("body parts grid", w.clone()))
                    .num_columns(15)
                    .spacing([6.0, 4.0])
                    .show(ui, |ui| {
                        for k in [
                            "collections.part",
                            "collections.damage",
                            "collections.spread",
                            "collections.hits_with",
                            "collections.armor",
                        ] {
                            ui.weak(t.get(k));
                        }
                        for (_, k) in PART_ARMOR {
                            ui.weak(t.get(&format!("fields.{k}")));
                        }
                        ui.weak(t.get("collections.height"));
                        ui.label("");
                        ui.end_row();
                        for i in 0..rows.len() {
                            let key = key_of(&rows[i]);
                            ui.label(part_name(key));
                            number(self, ui, i, "d_Val", false, 36.0, out);
                            number(self, ui, i, "d_Var", true, 40.0, out);
                            // What it hits with only counts while it hits: blank at no damage,
                            // the stored kind kept.
                            if hits(&rows[i]) {
                                ticks(ui, i, "d_Type", PART_DAMAGE, out);
                            } else {
                                ui.label("");
                            }
                            number(self, ui, i, "base_Armor", false, 36.0, out);
                            for (column, _) in PART_ARMOR {
                                number(self, ui, i, column, false, 36.0, out);
                            }
                            choice(ui, i, "b_h", HEIGHT, out);
                            let x = ui.small_button("×");
                            self.editor
                                .row_buttons
                                .push((format!("part:{key}:remove"), x.rect));
                            if x.clicked() {
                                let mut new = rows.to_vec();
                                new.remove(i);
                                out.push(UiAction::Form(
                                    w.clone(),
                                    FormEdit::Rows {
                                        table: BODY_PARTS.to_owned(),
                                        rows: new,
                                    },
                                ));
                            }
                            ui.end_row();
                        }
                    });
            });
        if rows.is_empty() {
            ui.weak(t.get("collections.empty"));
        }
        // Another part, one it lacks: as the server's weenies typically have it, else like the
        // last.
        let has: Vec<u32> = rows.iter().map(key_of).collect();
        let r = egui::ComboBox::from_id_salt(("add part", w.clone()))
            .selected_text(t.get("collections.add_part"))
            .height(320.0)
            .show_ui(ui, |ui| {
                for (k, n) in BODY_PART_NAMES {
                    if has.contains(k) {
                        continue;
                    }
                    let r = ui.selectable_label(false, t.get(&format!("collections.{n}")));
                    self.editor.picker_rows.push((format!("part:{k}"), r.rect));
                    if r.clicked() {
                        let mut new = rows.to_vec();
                        let like = new.last().cloned();
                        let mut row = new_part(self.session.typical_body_part(*k), like.as_ref());
                        row.set("key", k.to_string());
                        row.comment = None;
                        new.push(row);
                        out.push(UiAction::Form(
                            w.clone(),
                            FormEdit::Rows {
                                table: BODY_PARTS.to_owned(),
                                rows: new,
                            },
                        ));
                    }
                }
            });
        self.editor
            .add_buttons
            .push(("part:add".to_owned(), r.response.rect));
        // Where each part is hit: its share of the blows of each height, side and facing.
        if !rows.is_empty() {
            egui::CollapsingHeader::new(t.get("collections.where_hit"))
                .id_salt(("where hit", w.clone()))
                .show(ui, |ui| {
                    egui::ScrollArea::horizontal()
                        .id_salt(("where hit scroll", w.clone()))
                        .auto_shrink([false, true])
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                        )
                        .show(ui, |ui| {
                            egui::Grid::new(("where hit grid", w.clone()))
                                .num_columns(13)
                                .spacing([6.0, 4.0])
                                .show(ui, |ui| {
                                    ui.weak(t.get("collections.part"));
                                    for (_, k) in PART_HITS {
                                        ui.weak(t.get(&format!("collections.{k}")));
                                    }
                                    ui.end_row();
                                    for i in 0..rows.len() {
                                        ui.label(part_name(key_of(&rows[i])));
                                        for (column, _) in PART_HITS {
                                            number(self, ui, i, column, true, 40.0, out);
                                        }
                                        ui.end_row();
                                    }
                                });
                        });
                });
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's lists of a weenie's rows)
    use super::*;

    fn row(d: u32, class: u32, shade: f64) -> TableRow {
        TableRow::of(&[
            ("destination_Type", d.to_string()),
            ("weenie_Class_Id", class.to_string()),
            ("stack_Size", "1".into()),
            ("palette", "0".into()),
            ("shade", decimal(shade)),
            ("try_To_Bond", "False".into()),
        ])
    }

    #[test]
    fn a_body_parts_damage_kind_shows_only_while_it_does_damage_and_is_kept_unshown() {
        let part = |d_val: &str| TableRow::of(&[("d_Type", "4".into()), ("d_Val", d_val.into())]);
        assert!(hits(&part("5")));
        assert!(hits(&part("-2")));
        // No damage, or none written: blank, the kind still stored.
        assert!(!hits(&part("0")));
        let unset = TableRow::of(&[("d_Type", "4".into())]);
        assert!(!hits(&unset));
        assert_eq!(part("0").get("d_Type"), Some("4"));
    }

    fn chances(list: &[TableRow]) -> Vec<(u32, f64)> {
        list.iter()
            .map(|r| (r.bits("weenie_Class_Id"), (chance(r) * 1e4).round() / 100.0))
            .collect()
    }

    #[test]
    fn a_spawns_when_is_once_it_is_gone_or_when_it_dies_and_is_written_only_when_chosen_anew() {
        // Destroyed, picked up and neither are alike to the server.
        for v in [0, 1, 2] {
            assert_eq!(Again::of(v), Again::Gone, "{v}");
            assert_eq!(Again::Gone.written(v), None, "{v}");
            assert_eq!(Again::Death.written(v), Some(4), "{v}");
        }
        assert_eq!(Again::of(4), Again::Death);
        assert_eq!(Again::Death.written(4), None);
        assert_eq!(Again::Gone.written(4), Some(1));
        // A value of neither is shown as it is, and either choice writes its own.
        assert_eq!(Again::of(3), Again::Other(3));
        assert_eq!(Again::of(3).key(), None);
        assert_eq!(Again::Gone.written(3), Some(1));
        let t = crate::text::read(crate::text::BUILT_IN).unwrap();
        for a in [Again::Gone, Again::Death] {
            assert!(t.contains_key(a.key().unwrap()), "{a:?}");
        }
    }

    #[test]
    fn what_a_creature_has_equipped_drops_only_as_treasure_and_never_when_destroyed_on_death() {
        // Wielded: only a treasure row, and not a thing destroyed on death.
        assert!(!drops(Drill::Wields, dest::WIELD, false));
        assert!(drops(Drill::Wields, dest::WIELD | dest::TREASURE, false));
        assert!(!drops(Drill::Wields, dest::WIELD | dest::TREASURE, true));
        // What it holds, and its treasure not wielded, are made onto its corpse.
        assert!(drops(Drill::Holds, dest::CONTAIN, false));
        assert!(drops(Drill::Holds, dest::CONTAIN | dest::TREASURE, true));
        assert!(drops(Drill::Holds, dest::TREASURE, false));
        assert!(!drops(Drill::Sells, dest::SHOP, false));
    }

    #[test]
    fn a_wielded_rows_shade_colors_it_and_a_treasure_rows_is_its_chance() {
        assert!(shades(&row(dest::WIELD, 130, 0.67)));
        assert!(shades(&row(dest::WIELD, 130, 0.0)), "0 is its darkest");
        assert!(!shades(&row(dest::WIELD | dest::TREASURE, 130, 0.5)));
        assert!(!shades(&row(dest::WIELD | dest::TREASURE, 130, 0.0)));
    }

    #[test]
    fn every_name_and_heading_the_lists_show_has_words_in_the_text() {
        let t = crate::text::read(crate::text::BUILT_IN).unwrap();
        let mut need: Vec<String> = Vec::new();
        for (_, k) in ATTRIBUTE_NAMES.iter().chain(VITAL_NAMES) {
            need.push(format!("fields.{k}"));
        }
        for (_, k) in PART_DAMAGE.iter().chain(HEIGHT) {
            need.push(format!("fields.{k}"));
        }
        for (_, k) in PART_ARMOR {
            need.push(format!("fields.{k}"));
        }
        for (_, k) in TRAINING {
            need.push(format!("collections.{k}"));
        }
        for (_, k) in BODY_PART_NAMES.iter().chain(&[]) {
            need.push(format!("collections.{k}"));
        }
        for (_, k) in PART_HITS {
            need.push(format!("collections.{k}"));
        }
        // Every word of the editors' own.
        let code = include_str!("collections.rs");
        let code = code.split("#[cfg(test)]").next().unwrap_or_default();
        let mut rest = code;
        while let Some(at) = rest.find("\"collections.") {
            let tail = &rest[at + 1..];
            let end = tail.find('"').unwrap_or(0);
            let key = &tail[..end];
            if !key.contains('{') {
                need.push(key.to_owned());
            }
            rest = &tail[end..];
        }
        let missing: Vec<&String> = need.iter().filter(|k| !t.contains_key(*k)).collect();
        assert!(missing.is_empty(), "{missing:?}");
        assert!(
            need.iter().any(|k| k == "collections.too_likely"),
            "the scan read the code"
        );
    }

    #[test]
    fn treasure_is_drawn_in_sets_that_fill_a_hundred_percent_as_the_server_draws_them() {
        let list = vec![
            row(2, 100, 0.0),
            row(10, 1, 0.5),
            row(10, 2, 0.5),
            row(10, 3, 0.25),
            row(10, 0, 0.75),
            row(10, 4, 0.3),
        ];
        assert_eq!(
            sets(&list),
            [None, Some(0), Some(0), Some(1), Some(1), Some(2)]
        );
        let t = set_totals(&list);
        assert!(
            (t[0] - 1.0).abs() < 1e-9 && (t[1] - 1.0).abs() < 1e-9 && (t[2] - 0.3).abs() < 1e-9
        );
        // Counted in the server's single precision: ten tenths close their set.
        let tenths: Vec<TableRow> = (0..11).map(|i| row(10, i + 1, 0.1)).collect();
        let s = sets(&tenths);
        assert_eq!(s[9], Some(0), "{s:?}");
        assert_eq!(s[10], Some(1), "{s:?}");
        // A treasure row with no shade is made always, as the server makes it.
        assert_eq!(sets(&[row(10, 5, 0.0)]), [None]);
        assert!(in_list(Drill::Holds, 9, true) && in_list(Drill::Holds, 8, true));
        assert!(!in_list(Drill::Holds, 8, false) && !in_list(Drill::Holds, 10, true));
        assert!(in_list(Drill::Wields, 10, true) && in_list(Drill::Sells, 4, true));
    }

    #[test]
    fn a_chance_changed_takes_the_difference_from_nothing_so_the_sets_stay_as_they_are() {
        let mut list = vec![
            row(10, 1, 0.5),
            row(10, 0, 0.5),
            row(10, 2, 0.6),
            row(10, 3, 0.4),
        ];
        // 50% to 80%: nothing goes from 50% to 20%.
        set_chance(&mut list, 0, 80.0).unwrap();
        assert_eq!(chances(&list), [(1, 80.0), (0, 20.0), (2, 60.0), (3, 40.0)]);
        assert_eq!(sets(&list), [Some(0), Some(0), Some(1), Some(1)]);
        // All of it: nothing goes.
        set_chance(&mut list, 0, 100.0).unwrap();
        assert_eq!(chances(&list), [(1, 100.0), (2, 60.0), (3, 40.0)]);
        // Less again: nothing is made to keep the first set whole.
        set_chance(&mut list, 0, 30.0).unwrap();
        assert_eq!(chances(&list), [(1, 30.0), (0, 70.0), (2, 60.0), (3, 40.0)]);
        assert_eq!(sets(&list), [Some(0), Some(0), Some(1), Some(1)]);
        // More than the set has room for: refused, with the most it can have.
        let before = list.clone();
        assert_eq!(
            set_chance(&mut list, 2, 70.0),
            Err(TooLikely { most: 60.0 })
        );
        assert_eq!(list, before);
        // The last set may make nothing at what is left, with no row of its own.
        set_chance(&mut list, 3, 10.0).unwrap();
        assert_eq!(chances(&list), [(1, 30.0), (0, 70.0), (2, 60.0), (3, 10.0)]);
    }

    #[test]
    fn a_set_whose_tenths_add_up_short_in_single_precision_still_closes_where_it_did() {
        let mut list: Vec<TableRow> = (0..4).map(|i| row(10, i + 1, 0.1)).collect();
        list.push(row(10, 0, 0.6));
        list.push(row(10, 9, 1.0));
        for i in 0..4 {
            set_chance(&mut list, i, 10.0 + f64::from(u8::try_from(i).unwrap())).unwrap();
        }
        let s = sets(&list);
        assert_eq!(
            s.last().copied().flatten(),
            Some(1),
            "{s:?} {:?}",
            chances(&list)
        );
        assert_eq!(s.iter().filter(|x| **x == Some(1)).count(), 1);
    }

    #[test]
    fn an_entry_taken_away_or_added_keeps_its_set_whole() {
        let mut list = vec![row(10, 1, 0.5), row(10, 2, 0.5), row(10, 3, 1.0)];
        remove_from(&mut list, 0);
        assert_eq!(chances(&list), [(2, 50.0), (0, 50.0), (3, 100.0)]);
        assert_eq!(sets(&list), [Some(0), Some(0), Some(1)]);
        // Added: it takes what made nothing.
        add_to_set(&mut list, 0, row(10, 4, 0.0));
        assert_eq!(chances(&list), [(2, 50.0), (4, 50.0), (3, 100.0)]);
        // Added to a full set: an even share.
        add_to_set(&mut list, 1, row(10, 5, 0.0));
        assert_eq!(chances(&list), [(2, 50.0), (4, 50.0), (3, 50.0), (5, 50.0)]);
        assert_eq!(sets(&list), [Some(0), Some(0), Some(1), Some(1)]);
        // A set left with only nothing goes.
        let mut one = vec![row(10, 1, 0.4), row(10, 0, 0.6), row(10, 2, 1.0)];
        remove_from(&mut one, 0);
        assert_eq!(chances(&one), [(2, 100.0)]);
        // A new set after a last one short of 100%: that one is filled first.
        let mut short = vec![row(10, 1, 0.4)];
        add_set(&mut short, row(10, 2, 0.0));
        assert_eq!(chances(&short), [(1, 40.0), (0, 60.0), (2, 100.0)]);
        assert_eq!(sets(&short), [Some(0), Some(0), Some(1)]);
        assert!(may_swap(&short, 0, 1) && !may_swap(&short, 1, 2));
    }

    #[test]
    fn a_generators_shares_are_read_from_its_running_totals_and_written_back_the_same_both_ways() {
        // One set, the commonest: 25%, 25%, 50%.
        let d = draws(&[0.25, 0.5, 1.0]);
        let p: Vec<f64> = percents(&d).into_iter().flatten().collect();
        assert_eq!(p, [25.0, 25.0, 50.0]);
        assert_eq!(totals(&d), [0.25, 0.5, 1.0]);
        // Two sets and one made always: each share of all of them together.
        let raw = [0.5, 1.0, -1.0, 0.3, 1.0];
        let d = draws(&raw);
        assert_eq!(d[2], Draw::Always);
        assert!(matches!(d[3], Draw::Chance { set: 1, .. }));
        let p = percents(&d);
        assert_eq!(p[2], None);
        let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
        assert!(
            near(p[0].unwrap(), 25.0) && near(p[3].unwrap(), 15.0) && near(p[4].unwrap(), 35.0)
        );
        let back = totals(&d);
        for (a, b) in back.iter().zip(raw) {
            assert!(near(*a, b), "{back:?}");
        }
        // A share changed: the others keep theirs among them, the sets stay as they are, and the
        // totals read back give the shares set.
        let mut d = draws(&raw);
        set_share(&mut d, 0, 40.0).unwrap();
        let written = totals(&d);
        let again = percents(&draws(&written));
        assert!(near(again[0].unwrap(), 40.0), "{again:?}");
        assert!(near(again[1].unwrap(), 20.0) && near(again[3].unwrap(), 12.0));
        assert!(near(again[4].unwrap(), 28.0));
        assert!(
            matches!(draws(&written)[3], Draw::Chance { set: 1, .. }),
            "{written:?}"
        );
        // The total is kept: one set still ends at 1.
        let mut one = draws(&[0.25, 0.5, 1.0]);
        set_share(&mut one, 2, 20.0).unwrap();
        let w = totals(&one);
        assert!(near(*w.last().unwrap(), 1.0), "{w:?}");
        let p = percents(&draws(&w));
        assert!(
            near(p[0].unwrap(), 40.0) && near(p[2].unwrap(), 20.0),
            "{p:?}"
        );
    }

    #[test]
    fn a_change_that_would_join_a_set_to_the_one_before_is_refused_and_changes_nothing() {
        // Set 2's first entry made as likely as all of set 1: written as two sets it would read
        // back as one, so it is refused, saying which.
        let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
        let start = draws(&[0.5, 1.0, 0.2, 1.0]);
        let mut d = start.clone();
        assert_eq!(
            set_share(&mut d, 2, 80.0),
            Err(JoinsSets { set: 2, before: 1 })
        );
        assert_eq!(d, start);
        // Less of set 1 is made while it stays above set 2's first; set 1 taken away leaves set
        // 2 the first.
        assert!(set_share(&mut d, 0, 5.0).is_ok());
        let mut d = start.clone();
        assert!(remove_draw(&mut d, 0).is_ok() && remove_draw(&mut d, 0).is_ok());
        assert_eq!(percents(&d).len(), 2);
        assert!(matches!(d[0], Draw::Chance { set: 0, .. }));
        // Set 2's larger entry moved first, or set 1 made always in part, joins them: refused.
        let mut d = draws(&[0.2, 0.4, 0.1, 1.0]);
        assert_eq!(
            swap_draws(&mut d, 2, 3),
            Err(JoinsSets { set: 2, before: 1 })
        );
        let mut d = draws(&[0.3, 0.5, 0.25, 1.0]);
        assert!(set_always(&mut d, 0).is_err());
        assert!(set_always(&mut d, 1).is_ok());
        // A share that keeps the sets apart is made, and the sets show their own shares.
        let mut d = start.clone();
        set_share(&mut d, 2, 10.0).unwrap();
        let again = percents(&draws(&totals(&d)));
        assert!(near(again[2].unwrap(), 10.0), "{again:?}");
        let (of, sets) = set_shares(&d);
        assert_eq!(of, [Some(0), Some(0), Some(1), Some(1)]);
        assert!(near(sets[0] + sets[1], 100.0), "{sets:?}");
        // Always and back.
        let mut d = draws(&[0.5, 1.0]);
        set_always(&mut d, 0).unwrap();
        assert_eq!(totals(&d), [-1.0, 0.5]);
        assert_eq!(percents(&d)[1], Some(100.0));
        set_share(&mut d, 0, 50.0).unwrap();
        let p = percents(&draws(&totals(&d)));
        assert!(
            near(p[0].unwrap(), 50.0) && near(p[1].unwrap(), 50.0),
            "{p:?}"
        );
        // Added: the whole draw, then an even share.
        let mut d: Vec<Draw> = Vec::new();
        add_draw(&mut d).unwrap();
        assert_eq!(totals(&d), [1.0]);
        add_draw(&mut d).unwrap();
        let p = percents(&draws(&totals(&d)));
        assert!(
            near(p[0].unwrap(), 50.0) && near(p[1].unwrap(), 50.0),
            "{p:?}"
        );
        remove_draw(&mut d, 0).unwrap();
        assert_eq!(percents(&d), [Some(100.0)]);
    }

    #[test]
    fn a_new_body_part_takes_the_servers_typical_row_else_the_last_parts() {
        let typical: Vec<(String, String)> = [("key", "0"), ("d_Type", "4"), ("h_l_f", "0.33")]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        let last = TableRow::of(&[("key", "1".into()), ("d_Val", "7".into())]);
        let r = new_part(Some(&typical), Some(&last));
        assert_eq!(r.get("h_l_f"), Some("0.33"));
        assert_eq!(
            r.get("d_Val"),
            Some("0"),
            "the server's row, not the last part's"
        );
        assert_eq!(r.get("b_h"), Some("2"));
        assert_eq!(
            r.cells.len(),
            super::super::form::columns(BODY_PARTS).len() - 1
        );
        let r = new_part(None, Some(&last));
        assert_eq!(r.get("d_Val"), Some("7"));
        assert_eq!(new_part(None, None).get("d_Type"), Some("0"));
    }

    #[test]
    fn a_creatures_spell_chance_is_read_and_written_as_the_server_reads_it() {
        assert!((cast_chance(2.05) - 5.0).abs() < 1e-9);
        assert!((cast_chance(2.0) - 2.0).abs() < 1e-9);
        assert!((cast_probability(5.0) - 2.05).abs() < 1e-9);
        assert!((cast_chance(cast_probability(30.0)) - 30.0).abs() < 1e-9);
        assert_eq!(cast_probability(0.0), 0.0);
        assert_eq!(decimal(2.05), "2.05");
        assert_eq!(decimal(1.0), "1");
        assert_eq!(decimal(0.333_333_3), "0.333333");
    }

    #[test]
    fn rows_are_written_back_in_order_and_a_list_is_put_back_in_its_own_places() {
        let all = vec![
            row(1, 1, 0.0),
            row(2, 2, 0.0),
            row(2, 4, 0.0),
            row(1, 3, 0.0),
        ];
        let wields = |r: &TableRow| r.bits("destination_Type") & dest::WIELD != 0;
        // One more than the list had: it follows the list's last place, not the table's end.
        let list = vec![row(2, 5, 0.0), row(2, 4, 0.0), row(2, 6, 0.0)];
        let out = splice(&all, wields, &list);
        let classes: Vec<u32> = out.iter().map(|r| r.bits("weenie_Class_Id")).collect();
        assert_eq!(classes, [1, 5, 4, 6, 3]);
        let fewer = splice(&all, wields, &[row(2, 7, 0.0)]);
        let classes: Vec<u32> = fewer.iter().map(|r| r.bits("weenie_Class_Id")).collect();
        assert_eq!(classes, [1, 7, 3]);
        // Written into a weenie's SQL: its rows, in that order, with their comments.
        let mut doc = SqlDoc::parse(
            "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES (9, 'thing', 10, '2005-02-09 10:00:00');\nINSERT INTO `weenie_properties_create_list` (`object_Id`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`)\nVALUES (9, 1, 1, 1, 0, 0, False) /* one */\n     , (9, 2, 2, 1, 0, 0, False) /* two */;\n",
        );
        let mut rows = rows_of(&doc, CREATE_LIST);
        assert_eq!(rows[0].comment.as_deref(), Some("one"));
        rows.swap(0, 1);
        rows.push(row(2, 3, 0.25));
        write_rows(&mut doc, CREATE_LIST, &rows);
        assert_eq!(rows_of(&doc, CREATE_LIST), rows);
        let text = doc.render();
        assert!(
            text.contains("/* two */") && text.contains("/* one */"),
            "{text}"
        );
        write_rows(&mut doc, CREATE_LIST, &[]);
        assert!(!doc.render().contains("weenie_properties_create_list"));
    }
}
