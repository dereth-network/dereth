//! A weenie's emotes as the editor shows and changes them: what each set answers to, how likely
//! it is, and each of its actions as a line of plain words that opens into its fields.
//!
//! The server draws among the sets one event calls (their category, and the quest, item, kind of
//! trade, health or stance they answer to) by one roll: the set it runs is the least likely of
//! those whose probability is over the roll. So the sets' probabilities are running totals, each
//! set's own chance its total less the one below it, and what is over the top total runs
//! nothing. Several sets of one event are how a weenie has random greetings, so the sets show
//! apart, each with its own chance, and the editor writes the totals.
//!
//! The common actions (what it says, gives, takes, stamps, casts, teaches, plays and so on) are
//! a line of words each; any other shows its type and the columns it has, as the server writes
//! them.

use std::rc::Rc;

use lyceum_foundry::content::{self, RowRef, SqlDoc, ACTION_COLUMNS, EMOTE_COLUMNS};
use lyceum_foundry::edit::Weenie;

use super::collections::{decimal, percent_typed, percent_words, TableRow};
use super::form::FormEdit;
use super::{EditorApp, UiAction};

/// The columns of an emote set that say which event's draw it is in.
const CALLED_BY: [&str; 8] = [
    "category",
    "weenie_Class_Id",
    "style",
    "substyle",
    "quest",
    "vendor_Type",
    "min_Health",
    "max_Health",
];

/// The categories whose sets answer to a quest, a test, a set's name, words heard or a signal:
/// their `quest`.
const ANSWERS: [u32; 17] = [
    12, 13, 22, 23, 24, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38,
];

/// The categories whose sets answer to an item given: their `weenie_Class_Id`.
const ITEMS: [u32; 2] = [1, 6];

/// The trading category, whose sets answer to a kind of trade.
const VENDOR: u32 = 2;

/// Taunting when hurt, whose sets answer to how hurt it is.
const WOUNDED: u32 = 15;

/// One set as the server draws it: the event's draw it is in (by its first set's place), and its
/// own chance, 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetChance {
    pub draw: usize,
    pub probability: f64,
    pub chance: f64,
}

/// A value as sets are told apart by: without its comment, a word in lower case.
fn plain(v: Option<String>) -> String {
    let v = v.map(|v| content::bare(&v)).unwrap_or_default();
    let v = v.trim();
    if v.eq_ignore_ascii_case("NULL") {
        String::new()
    } else {
        content::unquote(v).to_lowercase()
    }
}

/// Each set of `doc`, in order: the draw it is in, its probability as written, and its own
/// chance among its draw's.
#[must_use]
pub fn chances(doc: &SqlDoc) -> Vec<SetChance> {
    let sets = doc.emote_sets();
    let keys: Vec<Vec<String>> = sets
        .iter()
        .map(|s| {
            CALLED_BY
                .iter()
                .map(|c| plain(doc.cell(s.emote, c)))
                .collect()
        })
        .collect();
    let probability: Vec<f64> = sets
        .iter()
        .map(|s| {
            doc.cell(s.emote, "probability")
                .and_then(|v| v.trim().parse::<f64>().ok())
                .unwrap_or(1.0)
        })
        .collect();
    // Each set's draw: the place of the first set told apart as it is.
    let mut first: std::collections::HashMap<&[String], usize> = std::collections::HashMap::new();
    let draw: Vec<usize> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| *first.entry(k.as_slice()).or_insert(i))
        .collect();
    let mut out: Vec<SetChance> = (0..sets.len())
        .map(|i| SetChance {
            draw: draw[i],
            probability: probability[i],
            chance: 0.0,
        })
        .collect();
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); sets.len()];
    for (i, d) in draw.iter().enumerate() {
        members[*d].push(i);
    }
    for m in &mut members {
        rank(&out, m);
        let mut below = 0.0f64;
        for i in m.iter() {
            let top = out[*i].probability.clamp(0.0, 1.0);
            out[*i].chance = (top - below).max(0.0);
            below = below.max(top);
        }
    }
    out
}

/// `members` of one draw put least likely first: by probability, a tie in the order written.
fn rank(sets: &[SetChance], members: &mut [usize]) {
    members.sort_by(|a, b| {
        sets[*a]
            .probability
            .total_cmp(&sets[*b].probability)
            .then(a.cmp(b))
    });
}

/// The sets of draw `d`, least likely first: by probability, a tie in the order written (the
/// first of a tie runs, the others never).
fn ranked(sets: &[SetChance], d: usize) -> Vec<usize> {
    let mut members: Vec<usize> = (0..sets.len()).filter(|i| sets[*i].draw == d).collect();
    rank(sets, &mut members);
    members
}

/// The probabilities the sets of draw `d` are written with for each to have the chance `want`
/// gives it (its own otherwise), in the order they are drawn: each set's place and its total.
fn totals(sets: &[SetChance], d: usize, want: impl Fn(usize) -> Option<f64>) -> Vec<(usize, f64)> {
    let mut sum = 0.0;
    ranked(sets, d)
        .into_iter()
        .map(|i| {
            sum += want(i).unwrap_or(sets[i].chance);
            (i, sum.min(1.0))
        })
        .collect()
}

/// A chance that cannot be had: the most the set can have, in percent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TooLikely {
    pub most: f64,
}

/// The probabilities each set of set `i`'s draw takes for it to run `percent` of the time, the
/// others keeping their chances: each set's place and its new probability.
///
/// # Errors
/// More than its draw has room for besides its others: the most it can have.
pub fn with_chance(
    sets: &[SetChance],
    i: usize,
    percent: f64,
) -> Result<Vec<(usize, f64)>, TooLikely> {
    let Some(me) = sets.get(i) else {
        return Ok(Vec::new());
    };
    let others: f64 = (0..sets.len())
        .filter(|j| *j != i && sets[*j].draw == me.draw)
        .map(|j| sets[j].chance)
        .sum();
    let want = percent / 100.0;
    if others + want > 1.0 + 1e-9 {
        return Err(TooLikely {
            most: ((1.0 - others) * 100.0).max(0.0),
        });
    }
    Ok(totals(sets, me.draw, |j| (j == i).then_some(want)))
}

/// The probabilities the other sets of set `i`'s draw take once it is taken away, each keeping
/// its chance: its place (before `i` goes) and its new probability.
#[must_use]
pub fn without(sets: &[SetChance], i: usize) -> Vec<(usize, f64)> {
    let Some(me) = sets.get(i) else {
        return Vec::new();
    };
    totals(sets, me.draw, |j| (j == i).then_some(0.0))
        .into_iter()
        .filter(|(j, _)| *j != i)
        .collect()
}

/// A new set joining draw `d`: the probabilities its others take, and the new one's (it is drawn
/// last). It takes what runs nothing, or when nothing is left, an even share, the others made
/// less likely to give it room.
#[must_use]
pub fn joining(sets: &[SetChance], d: usize) -> (Vec<(usize, f64)>, f64) {
    let members = ranked(sets, d);
    let sum: f64 = members.iter().map(|i| sets[*i].chance).sum();
    if members.is_empty() {
        return (Vec::new(), 1.0);
    }
    if 1.0 - sum > 1e-6 {
        return (Vec::new(), 1.0);
    }
    #[allow(clippy::cast_precision_loss)] // a draw's few sets
    let n = members.len() as f64;
    let scaled = totals(sets, d, |j| Some(sets[j].chance * n / (n + 1.0)));
    (scaled, 1.0)
}

/// The set a new set of `category` is written after, so that it stands with its own: the last
/// of draw `joins`'s sets when it joins one, else the last set of its category (each set's in
/// `categories`, as told apart); none, for a category with no sets, written last.
///
/// The server draws among a category's sets alone, so where a set stands among the other
/// categories' changes nothing; among its draw's it stands last, as it would at the end.
#[must_use]
pub fn set_after(
    sets: &[SetChance],
    categories: &[String],
    category: &str,
    joins: Option<usize>,
) -> Option<usize> {
    match joins {
        Some(j) => (0..sets.len())
            .rev()
            .find(|k| sets[*k].draw == sets[j].draw),
        None => (0..categories.len())
            .rev()
            .find(|k| categories[*k] == category),
    }
}

/// One set of emotes and its actions as written, every column set but the weenie's own and the
/// actions' set and order: what puts a weenie's emotes back as another's.
#[derive(Debug, Clone, PartialEq)]
pub struct EmoteRows {
    pub set: TableRow,
    pub actions: Vec<TableRow>,
}

/// The emotes of `doc`, set by set.
#[must_use]
pub fn emotes_of(doc: &SqlDoc) -> Vec<EmoteRows> {
    let row = |r: RowRef, columns: &[&str]| TableRow {
        cells: columns
            .iter()
            .filter_map(|c| doc.raw(r, c).map(|v| ((*c).to_owned(), v.to_owned())))
            .filter(|(_, v)| v != "NULL")
            .collect(),
        comment: doc.comment(r).map(str::to_owned),
    };
    doc.emote_sets()
        .iter()
        .map(|s| EmoteRows {
            set: row(s.emote, &EMOTE_COLUMNS[1..]),
            actions: doc
                .actions(s)
                .into_iter()
                .map(|a| row(a, &ACTION_COLUMNS[2..]))
                .collect(),
        })
        .collect()
}

/// `doc`'s emotes written as `sets`: its own taken away, then each of `sets` with its actions.
pub fn write_emotes(doc: &mut SqlDoc, sets: &[EmoteRows]) {
    while let Some(last) = doc.emote_sets().len().checked_sub(1) {
        doc.remove_emote_set(last);
    }
    for s in sets {
        let category = s.set.get("category").unwrap_or("0").to_owned();
        let fields: Vec<(&str, String)> = s
            .set
            .cells
            .iter()
            .filter(|(c, _)| c != "category")
            .map(|(c, v)| (c.as_str(), v.clone()))
            .collect();
        let i = doc.add_emote_set(&category, &fields);
        for (n, a) in s.actions.iter().enumerate() {
            let fields: Vec<(&str, String)> = a
                .cells
                .iter()
                .map(|(c, v)| (c.as_str(), v.clone()))
                .collect();
            doc.add_action(i, n, &fields);
        }
    }
}

/// How an action's field is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// Words, written quoted.
    Words,
    /// Words players read as a paragraph: several lines, wrapped.
    Lines,
    Whole,
    /// A weenie, by a picker.
    Weenie,
    /// A spell, by a picker.
    Spell,
    /// A motion, by the server's names.
    Motion,
}

/// A field of a common action: the column it writes, its label's key (`emotes.field_<key>`) and
/// how it is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionField {
    pub column: &'static str,
    pub key: &'static str,
    pub pick: Pick,
}

/// A common action: its type, its line's key (`emotes.action_<key>`, its columns in braces) and
/// its fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionKind {
    pub ty: u32,
    pub key: &'static str,
    pub fields: &'static [ActionField],
}

const fn f(column: &'static str, key: &'static str, pick: Pick) -> ActionField {
    ActionField { column, key, pick }
}

const fn kind(ty: u32, key: &'static str, fields: &'static [ActionField]) -> ActionKind {
    ActionKind { ty, key, fields }
}

const WORDS: &[ActionField] = &[f("message", "words", Pick::Lines)];
const QUEST: &[ActionField] = &[f("message", "quest", Pick::Words)];
const EVENT: &[ActionField] = &[f("message", "event", Pick::Words)];
const ITEM: &[ActionField] = &[
    f("weenie_Class_Id", "item", Pick::Weenie),
    f("stack_Size", "how_many", Pick::Whole),
];
const SPELL: &[ActionField] = &[f("spell_Id", "spell", Pick::Spell)];
const XP: &[ActionField] = &[f("amount_64", "how_much", Pick::Whole)];

/// The common actions, the commonest first: what nine in ten of the server's are.
pub const ACTIONS: &[ActionKind] = &[
    kind(10, "tell", WORDS),
    kind(8, "say", WORDS),
    kind(17, "say_near", WORDS),
    kind(18, "show", WORDS),
    kind(13, "show_text", WORDS),
    kind(16, "say_world", WORDS),
    kind(5, "motion", &[f("motion", "motion", Pick::Motion)]),
    kind(12, "turn_to", &[]),
    kind(3, "give", ITEM),
    kind(74, "take", ITEM),
    kind(
        76,
        "asks_has",
        &[
            f("weenie_Class_Id", "item", Pick::Weenie),
            f("stack_Size", "how_many", Pick::Whole),
            f("message", "answers", Pick::Words),
        ],
    ),
    kind(21, "asks_quest", QUEST),
    kind(22, "stamps", QUEST),
    kind(20, "updates_quest", QUEST),
    kind(31, "erases", QUEST),
    kind(
        33,
        "adds_to_quest",
        &[
            f("message", "quest", Pick::Words),
            f("amount", "how_much", Pick::Whole),
        ],
    ),
    kind(
        30,
        "asks_solves",
        &[
            f("message", "quest", Pick::Words),
            f("min", "at_least", Pick::Whole),
            f("max", "at_most", Pick::Whole),
        ],
    ),
    kind(67, "goes_to", &[f("message", "answers", Pick::Words)]),
    kind(27, "teaches", SPELL),
    kind(14, "casts", SPELL),
    kind(19, "casts_now", SPELL),
    kind(2, "gives_xp", XP),
    kind(62, "gives_xp_alone", XP),
    kind(113, "gives_luminance", XP),
    kind(23, "starts_event", EVENT),
    kind(24, "stops_event", EVENT),
    kind(34, "gives_title", &[f("amount", "title", Pick::Whole)]),
    kind(88, "signals", WORDS),
    kind(4, "goes_home", &[]),
    kind(77, "goes_away", &[]),
    kind(78, "dies", &[]),
    kind(72, "generates", &[]),
];

/// The common action of type `ty`, when it is one.
#[must_use]
pub fn action_kind(ty: u32) -> Option<&'static ActionKind> {
    ACTIONS.iter().find(|k| k.ty == ty)
}

/// Every emote category the server has, as it numbers them.
pub const CATEGORIES: std::ops::RangeInclusive<u32> = 1..=38;

/// A whole number as its line says it: its thousands set apart.
#[must_use]
pub fn thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// The line of an action of type `ty`: its kind's words, each column in braces filled by
/// `value`, else its type by the server's name (`named`); then how long it waits first.
#[must_use]
pub fn action_words(
    t: &crate::text::Table,
    ty: u32,
    named: &str,
    value: &dyn Fn(&str) -> String,
) -> String {
    let line = match action_kind(ty) {
        Some(k) => {
            let mut s = t.get(&format!("emotes.action_{}", k.key)).to_owned();
            for c in k.fields.iter().map(|f| f.column).chain(["stack_Size"]) {
                let brace = format!("{{{c}}}");
                if s.contains(&brace) {
                    // On one line, and something to see where it says nothing yet.
                    let v = value(c).replace('\n', " ");
                    let v = if v.trim().is_empty() {
                        "…".to_owned()
                    } else {
                        v
                    };
                    s = s.replace(&brace, &v);
                }
            }
            s
        }
        None => named.to_owned(),
    };
    let delay = value("delay").trim().parse::<f64>().unwrap_or(0.0);
    if delay > 0.0 {
        t.fill("emotes.after", &[("line", &line), ("n", &decimal(delay))])
    } else {
        line
    }
}

/// Whether action `a` of `doc` is an Activate step: one that sets off what its spawn is linked to.
fn activates(doc: &SqlDoc, a: RowRef) -> bool {
    doc.cell(a, "type")
        .and_then(|v| lyceum_foundry::sql_ids::number(&v))
        == Some(super::sets_off::ACTIVATE)
}

/// What a column of an action shows in its line, as written (`raw`): words unquoted, an amount
/// with its thousands apart, a weenie, a spell or a motion by `name`.
fn column_words(
    column: &str,
    raw: Option<String>,
    name: &dyn Fn(&str, &str) -> Option<String>,
) -> String {
    let Some(v) = raw
        .map(|v| content::bare(&v))
        .filter(|v| v.trim() != "NULL")
    else {
        return match column {
            "stack_Size" => "1".to_owned(),
            _ => String::new(),
        };
    };
    match column {
        "message" | "test_String" => content::unquote(&v),
        "amount" | "amount_64" | "min" | "max" | "stack_Size" => v
            .trim()
            .parse::<i64>()
            .map_or_else(|_| v.trim().to_owned(), thousands),
        "weenie_Class_Id" | "spell_Id" | "motion" => {
            name(column, v.trim()).unwrap_or_else(|| v.trim().to_owned())
        }
        _ => v.trim().to_owned(),
    }
}

impl EditorApp {
    /// The server's name of value `v` of its enum `e`, in plain words.
    fn enum_words(&self, e: &str, v: u32) -> Option<String> {
        self.schema
            .as_ref()
            .and_then(|s| s.value(e, v))
            .map(super::fields::spaced)
    }

    /// Action `a` of `doc` as a line of plain words.
    pub(super) fn action_line(&mut self, doc: &SqlDoc, a: RowRef) -> String {
        let t = self.text.table();
        let ty = doc
            .cell(a, "type")
            .and_then(|v| lyceum_foundry::sql_ids::number(&v))
            .unwrap_or(0);
        // Each column it names something by, named.
        let mut names: Vec<(String, String)> = Vec::new();
        for c in ["weenie_Class_Id", "spell_Id", "motion"] {
            let Some(v) = doc
                .cell(a, c)
                .filter(|v| v.trim() != "NULL")
                .and_then(|v| lyceum_foundry::sql_ids::number(&v))
            else {
                continue;
            };
            let name = match c {
                "weenie_Class_Id" => self.class_name(&v.to_string()),
                "spell_Id" => self.spell_name(v),
                _ => self.enum_words("MotionCommand", v),
            };
            if let Some(n) = name {
                names.push((c.to_owned(), n));
            }
        }
        // A pack's weenie, named by its token.
        if let Some(tok) = doc
            .cell(a, "weenie_Class_Id")
            .filter(|v| v.trim().starts_with("{{"))
        {
            if let Some(n) = self.class_name(&tok) {
                names.push(("weenie_Class_Id".to_owned(), n));
            }
        }
        let name = |c: &str, _: &str| names.iter().find(|(k, _)| k == c).map(|(_, n)| n.clone());
        let named = self
            .enum_words("EmoteType", ty)
            .unwrap_or_else(|| t.fill("emotes.action_n", &[("n", &ty)]));
        action_words(&t, ty, &named, &|c| column_words(c, doc.cell(a, c), &name))
    }

    /// What calls set `set` of `doc`, in words: its category, and the quest, item, kind of trade,
    /// health or stance it answers to.
    pub(super) fn set_words(&mut self, doc: &SqlDoc, set: RowRef) -> String {
        let t = self.text.table();
        let number = |c: &str| {
            doc.cell(set, c)
                .filter(|v| v.trim() != "NULL")
                .and_then(|v| lyceum_foundry::sql_ids::number(&v))
        };
        let category = number("category").unwrap_or(0);
        let mut line = t.or(&format!("emotes.category_{category}"), "").to_owned();
        if line.is_empty() {
            line = self
                .enum_words("EmoteCategory", category)
                .unwrap_or_else(|| t.fill("emotes.category_n", &[("n", &category)]));
        }
        let quest = doc
            .cell(set, "quest")
            .filter(|v| v.trim() != "NULL")
            .map(|v| content::unquote(&v));
        if let Some(q) = quest.filter(|q| !q.is_empty()) {
            line.push_str(&format!(" · {q}"));
        }
        if let Some(w) = doc
            .cell(set, "weenie_Class_Id")
            .filter(|v| v.trim() != "NULL" && v.trim() != "0")
        {
            let name = self.class_name(&w).unwrap_or_else(|| w.trim().to_owned());
            line.push_str(&format!(" · {name}"));
        }
        if let Some(v) = number("vendor_Type") {
            let kind = self
                .enum_words("VendorType", v)
                .unwrap_or_else(|| v.to_string());
            line.push_str(&format!(" · {kind}"));
        }
        let health = |c: &str| {
            doc.cell(set, c)
                .and_then(|v| v.trim().parse::<f64>().ok())
                .map(|h| percent_words(h * 100.0))
        };
        if let (Some(lo), Some(hi)) = (health("min_Health"), health("max_Health")) {
            line.push_str(&format!(
                " · {}",
                t.fill("emotes.health", &[("lo", &lo), ("hi", &hi)])
            ));
        }
        for (c, e) in [("style", "MotionStance"), ("substyle", "MotionCommand")] {
            if let Some(v) = number(c) {
                let w = self
                    .enum_words(e, v)
                    .unwrap_or_else(|| format!("0x{v:08X}"));
                line.push_str(&format!(" · {w}"));
            }
        }
        line
    }

    /// The Definition pane's emotes: each set under what calls it, with its chance, and its
    /// actions as lines; beside an Activate step, what it sets off.
    pub(super) fn emote_lines(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let sets = doc.emote_sets();
        let chances = chances(doc);
        let id = egui::Id::new(("emote lines", w.clone()));
        super::seen::blocks(
            ui,
            id,
            sets.len(),
            60.0,
            |_| false,
            |ui, i| {
                let s = &sets[i];
                // The first of its draw's sets: under what calls them.
                if chances[i].draw == i {
                    ui.add_space(4.0);
                    ui.strong(self.set_words(doc, s.emote));
                }
                let lines: Vec<(String, bool)> = doc
                    .actions(s)
                    .into_iter()
                    .map(|a| (self.action_line(doc, a), activates(doc, a)))
                    .collect();
                ui.weak(t.fill(
                    "emotes.set_chance",
                    &[("p", &percent_words(chances[i].chance * 100.0))],
                ));
                for (l, activate) in lines {
                    ui.label(format!("    {l}"));
                    if activate {
                        self.activate_note(ui, w, out);
                    }
                }
            },
        );
        if sets.is_empty() {
            ui.weak(t.get("collections.empty"));
        }
    }

    /// The editor's emotes: each event's sets under what calls them, each set's chance and what
    /// it answers to, and its actions as lines that open into their fields, moved up and down,
    /// taken away and added by kind; sets added to an event or for a new one.
    #[allow(clippy::too_many_lines)]
    pub(super) fn emote_editor(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let sets = doc.emote_sets();
        let chances = Rc::new(chances(doc));
        let refused = t.get("messages.not_a_value").to_owned();
        let probability_edits = |list: &[(usize, f64)]| -> Vec<FormEdit> {
            list.iter()
                .map(|(i, p)| FormEdit::EmoteSet {
                    set: *i,
                    column: "probability".into(),
                    value: decimal(*p),
                })
                .collect()
        };
        // Each draw's chances in all; and the sets with a field asked for or being typed in,
        // drawn wherever they are in the list.
        let mut totals = vec![0.0f64; sets.len()];
        for c in chances.iter() {
            totals[c.draw] += c.chance;
        }
        let held = self.held_rows(w, &["emote", "action"]);
        let id = egui::Id::new(("emote sets", w.clone()));
        super::seen::blocks(
            ui,
            id,
            sets.len(),
            140.0,
            |i| held.contains(&i),
            |ui, i| {
                let s = &sets[i];
                let d = chances[i].draw;
                let category = doc
                    .cell(s.emote, "category")
                    .and_then(|v| lyceum_foundry::sql_ids::number(&v))
                    .unwrap_or(0);
                // The first of its draw's sets: under what calls them.
                if d == i {
                    ui.add_space(6.0);
                    ui.strong(self.set_words(doc, s.emote));
                    if totals[d] < 1.0 - 1e-6 {
                        ui.weak(t.fill(
                            "emotes.nothing_rest",
                            &[("p", &percent_words((1.0 - totals[d]) * 100.0))],
                        ));
                    }
                }
                let jump = self.editor.focus_on(w, &format!("emote:{i}"));
                let frame = egui::Frame::group(ui.style()).show(ui, |ui| {
                    // The width of the page, whatever its fields' widths.
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.weak(t.get("emotes.chance"));
                        let now = percent_words(chances[i].chance * 100.0);
                        let (list, refused, tt) = (Rc::clone(&chances), refused.clone(), t.clone());
                        self.field(
                            ui,
                            w,
                            &format!("emote:{i}:chance"),
                            &now,
                            46.0,
                            move |v| {
                                let Some(p) = percent_typed(&v) else {
                                    return FormEdit::Refused(refused);
                                };
                                match with_chance(&list, i, p) {
                                    Ok(edits) => FormEdit::Probabilities(
                                        edits.into_iter().map(|(j, p)| (j, decimal(p))).collect(),
                                    ),
                                    Err(e) => FormEdit::Refused(tt.fill(
                                        "emotes.too_likely",
                                        &[("most", &percent_words(e.most))],
                                    )),
                                }
                            },
                            out,
                        );
                        ui.weak("%");
                        let x = ui.small_button(t.get("emotes.remove_set"));
                        self.editor
                            .row_buttons
                            .push((format!("emote:{i}:remove"), x.rect));
                        if x.clicked() {
                            let mut edits = probability_edits(&without(&chances, i));
                            edits.push(FormEdit::RemoveEmoteSet { set: i });
                            out.push(UiAction::Forms(w.clone(), edits));
                        }
                    });
                    self.set_fields(ui, w, doc, i, category, out);
                    let actions = doc.actions(s);
                    for (n, a) in actions.iter().enumerate() {
                        self.action_row(ui, w, doc, i, n, *a, actions.len(), out);
                    }
                    self.add_action(ui, w, i, out);
                });
                if jump {
                    frame.response.scroll_to_me(Some(egui::Align::Center));
                }
            },
        );
        if sets.is_empty() {
            ui.weak(t.get("collections.empty"));
        }
        ui.add_space(6.0);
        // A set for an event: one it has sets for already joins their draw.
        let r = egui::ComboBox::from_id_salt(("add emote set", w.clone()))
            .selected_text(t.get("emotes.add_set"))
            .height(320.0)
            .show_ui(ui, |ui| {
                for c in CATEGORIES {
                    let words = t.or(&format!("emotes.category_{c}"), "").to_owned();
                    let r = ui.selectable_label(false, words);
                    self.editor
                        .picker_rows
                        .push((format!("emote-category:{c}"), r.rect));
                    if r.clicked() {
                        let same = (0..sets.len()).find(|j| {
                            let s = sets[*j].emote;
                            plain(doc.cell(s, "category")) == c.to_string()
                                && CALLED_BY
                                    .iter()
                                    .skip(1)
                                    .all(|col| plain(doc.cell(s, col)).is_empty())
                        });
                        // Beside the sets it joins, else its event's others.
                        let categories: Vec<String> = sets
                            .iter()
                            .map(|s| plain(doc.cell(s.emote, "category")))
                            .collect();
                        let after = set_after(&chances, &categories, &c.to_string(), same);
                        let n = after.map_or(sets.len(), |a| a + 1);
                        let mut edits = vec![FormEdit::AddEmoteSet {
                            category: c.to_string(),
                            after,
                        }];
                        if let Some(j) = same {
                            let (others, mine) = joining(&chances, chances[j].draw);
                            edits.extend(probability_edits(&others));
                            edits.push(FormEdit::EmoteSet {
                                set: n,
                                column: "probability".into(),
                                value: decimal(mine),
                            });
                        }
                        out.push(UiAction::Forms(w.clone(), edits));
                    }
                }
            });
        self.editor
            .add_buttons
            .push(("emote:add".to_owned(), r.response.rect));
    }

    /// What set `i` (of `category`) answers to, as its event has it: a quest, a test's or a
    /// set's name, words heard or a signal; an item given; a kind of trade; how hurt it is.
    fn set_fields(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        i: usize,
        category: u32,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let Some(s) = doc.emote_sets().get(i).copied() else {
            return;
        };
        let has = |c: &str| doc.cell(s.emote, c).is_some_and(|v| v.trim() != "NULL");
        if ANSWERS.contains(&category) || has("quest") {
            ui.horizontal(|ui| {
                ui.weak(t.get("emotes.answers_to"));
                let now = doc
                    .cell(s.emote, "quest")
                    .filter(|v| v.trim() != "NULL")
                    .map(|v| content::unquote(&v))
                    .unwrap_or_default();
                self.field(
                    ui,
                    w,
                    &format!("emote:{i}:quest"),
                    &now,
                    180.0,
                    move |v| FormEdit::EmoteSet {
                        set: i,
                        column: "quest".into(),
                        value: if v.trim().is_empty() {
                            "NULL".into()
                        } else {
                            content::quote(v.trim())
                        },
                    },
                    out,
                );
            });
        }
        if ITEMS.contains(&category) || has("weenie_Class_Id") {
            ui.horizontal(|ui| {
                ui.weak(t.get("emotes.field_item"));
                let now = doc.cell(s.emote, "weenie_Class_Id").unwrap_or_default();
                let mut budget = super::PICTURES_A_FRAME;
                if now.trim() != "NULL" && !now.trim().is_empty() {
                    self.weenie_row(ui, &now, None, &mut budget, out);
                }
                if let Some(tok) =
                    self.weenie_picker(ui, &format!("emote:{i}:item"), t.get("emotes.choose"))
                {
                    out.push(UiAction::Form(
                        w.clone(),
                        FormEdit::EmoteSet {
                            set: i,
                            column: "weenie_Class_Id".into(),
                            value: tok,
                        },
                    ));
                }
            });
        }
        if category == VENDOR || has("vendor_Type") {
            let values = super::fields::enum_values(self.schema.as_ref(), "VendorType");
            let now = doc
                .cell(s.emote, "vendor_Type")
                .and_then(|v| v.trim().parse::<u32>().ok());
            ui.horizontal(|ui| {
                ui.weak(t.get("emotes.trade"));
                let shown = values.iter().find(|(v, _)| Some(*v) == now).map_or_else(
                    || now.map(|n| n.to_string()).unwrap_or_default(),
                    |(_, n)| n.clone(),
                );
                egui::ComboBox::from_id_salt(("vendor type", i, w.clone()))
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        for (v, words) in &values {
                            if ui.selectable_label(now == Some(*v), words).clicked() {
                                out.push(UiAction::Form(
                                    w.clone(),
                                    FormEdit::EmoteSet {
                                        set: i,
                                        column: "vendor_Type".into(),
                                        value: v.to_string(),
                                    },
                                ));
                            }
                        }
                    });
            });
        }
        if category == WOUNDED || has("min_Health") {
            ui.horizontal(|ui| {
                ui.weak(t.get("emotes.health_from"));
                for (c, k) in [("min_Health", "from"), ("max_Health", "to")] {
                    let now = doc
                        .cell(s.emote, c)
                        .and_then(|v| v.trim().parse::<f64>().ok())
                        .map(|h| percent_words(h * 100.0))
                        .unwrap_or_default();
                    if k == "to" {
                        ui.weak(t.get("emotes.health_to"));
                    }
                    let refused = t.get("messages.not_a_value").to_owned();
                    self.field(
                        ui,
                        w,
                        &format!("emote:{i}:{c}"),
                        &now,
                        40.0,
                        move |v| match percent_typed(&v) {
                            Some(p) => FormEdit::EmoteSet {
                                set: i,
                                column: c.into(),
                                value: decimal(p / 100.0),
                            },
                            None => FormEdit::Refused(refused),
                        },
                        out,
                    );
                    ui.weak("%");
                }
            });
        }
    }

    /// Action `n` of set `i`: its line, which opens into its fields, and its buttons.
    #[allow(clippy::too_many_arguments)]
    fn action_row(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        i: usize,
        n: usize,
        a: RowRef,
        count: usize,
        out: &mut Vec<UiAction>,
    ) {
        let line = self.action_line(doc, a);
        let key = format!("action:{i}:{n}");
        let id = ui.make_persistent_id(("action", w.clone(), i, n));
        let mut state =
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
        let jump = self.editor.focus_on(w, &key);
        if jump {
            state.set_open(true);
        }
        let mut clicked = false;
        let mut header = state.show_header(ui, |ui| {
            let r = ui.add(
                egui::Label::new(&line)
                    .selectable(false)
                    .sense(egui::Sense::click()),
            );
            self.editor.row_buttons.push((key.clone(), r.rect));
            clicked = r.clicked();
            if jump {
                r.scroll_to_me(Some(egui::Align::Center));
            }
            for (to, enabled, glyph) in
                [(n.wrapping_sub(1), n > 0, "▲"), (n + 1, n + 1 < count, "▼")]
            {
                if ui
                    .add_enabled(enabled, egui::Button::new(glyph).small())
                    .clicked()
                {
                    out.push(UiAction::Form(
                        w.clone(),
                        FormEdit::MoveAction {
                            set: i,
                            from: n,
                            to,
                        },
                    ));
                }
            }
            let x = ui.small_button("×");
            self.editor
                .row_buttons
                .push((format!("{key}:remove"), x.rect));
            if x.clicked() {
                out.push(UiAction::Form(
                    w.clone(),
                    FormEdit::RemoveAction { set: i, action: n },
                ));
            }
        });
        // A click on the line opens it, as its arrow does.
        if clicked {
            header.toggle();
        }
        header.body(|ui| self.action_fields(ui, w, doc, i, n, a, out));
        if activates(doc, a) {
            self.activate_note(ui, w, out);
        }
    }

    /// The fields of action `n` of set `i`: a common one's own, its wait and its type; any other
    /// one's columns as written, and any other column by its name.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn action_fields(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        i: usize,
        n: usize,
        a: RowRef,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let ty = doc
            .cell(a, "type")
            .and_then(|v| lyceum_foundry::sql_ids::number(&v))
            .unwrap_or(0);
        let refused = t.get("messages.not_a_value").to_owned();
        let edit = move |column: &str, value: String| FormEdit::Action {
            set: i,
            action: n,
            column: column.to_owned(),
            value,
        };
        egui::Grid::new(("action fields", w.clone(), i, n))
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                // What it does: by the server's name of its type.
                ui.weak(t.get("emotes.field_type"));
                let typed_name = self
                    .schema
                    .as_ref()
                    .and_then(|s| s.value("EmoteType", ty))
                    .map_or_else(|| ty.to_string(), str::to_owned);
                self.field(
                    ui,
                    w,
                    &format!("action:{i}:{n}:type"),
                    &typed_name,
                    140.0,
                    move |v| edit("type", v),
                    out,
                );
                ui.end_row();
                ui.weak(t.get("emotes.field_delay"));
                let now = doc
                    .cell(a, "delay")
                    .filter(|v| v.trim() != "NULL")
                    .unwrap_or_default();
                let r = refused.clone();
                self.field(
                    ui,
                    w,
                    &format!("action:{i}:{n}:delay"),
                    &now,
                    50.0,
                    move |v| match v.trim().parse::<f64>() {
                        Ok(d) if d.is_finite() && d >= 0.0 => edit("delay", decimal(d)),
                        _ => FormEdit::Refused(r),
                    },
                    out,
                );
                ui.end_row();
                let Some(k) = action_kind(ty) else {
                    // Any other: each column it has, as written, and one it lacks by its name.
                    for c in ACTION_COLUMNS.iter().skip(5) {
                        let Some(now) = doc.cell(a, c).filter(|v| v.trim() != "NULL") else {
                            continue;
                        };
                        ui.weak(*c);
                        let now = match *c {
                            "message" | "test_String" => content::unquote(&now),
                            _ => now,
                        };
                        let col = (*c).to_owned();
                        self.field(
                            ui,
                            w,
                            &format!("action:{i}:{n}:{c}"),
                            &now,
                            if *c == "message" { 220.0 } else { 90.0 },
                            move |v| FormEdit::Action {
                                set: i,
                                action: n,
                                column: col,
                                value: v,
                            },
                            out,
                        );
                        ui.end_row();
                    }
                    ui.weak("");
                    let key = format!("action:{i}:{n}:column");
                    let typed = self.typed_row(
                        ui,
                        w,
                        &key,
                        &[
                            ("name", t.get("emotes.column_hint"), 110.0),
                            ("value", t.get("emotes.value_hint"), 140.0),
                        ],
                        t.get("emotes.set_column"),
                    );
                    ui.end_row();
                    if let Some([c, v]) = typed.as_deref() {
                        let c = c.trim();
                        if ACTION_COLUMNS.iter().skip(5).any(|x| *x == c) {
                            out.push(UiAction::Form(w.clone(), edit(c, v.trim().to_owned())));
                            self.forget_typed(w, &key, 2);
                        } else {
                            self.panels.message =
                                t.fill("messages.action_has_no", &[("column", &c)]);
                        }
                    }
                    return;
                };
                for field in k.fields {
                    ui.weak(t.get(&format!("emotes.field_{}", field.key)));
                    let column = field.column;
                    let key = format!("action:{i}:{n}:{column}");
                    let raw = doc.cell(a, column).filter(|v| v.trim() != "NULL");
                    match field.pick {
                        Pick::Words => {
                            let now = raw.map(|v| content::unquote(&v)).unwrap_or_default();
                            self.field(
                                ui,
                                w,
                                &key,
                                &now,
                                220.0,
                                move |v| edit(column, content::quote(&v)),
                                out,
                            );
                        }
                        Pick::Lines => {
                            let now = raw.map(|v| content::unquote(&v)).unwrap_or_default();
                            self.field_lines(
                                ui,
                                w,
                                &key,
                                &now,
                                300.0,
                                move |v| edit(column, content::quote(&v)),
                                out,
                            );
                        }
                        Pick::Whole => {
                            let now = raw.unwrap_or_default();
                            let r = refused.clone();
                            self.field(
                                ui,
                                w,
                                &key,
                                &now,
                                80.0,
                                move |v| match v.trim().replace(',', "").parse::<i64>() {
                                    Ok(x) => edit(column, x.to_string()),
                                    Err(_) => FormEdit::Refused(r),
                                },
                                out,
                            );
                        }
                        Pick::Weenie => {
                            ui.horizontal(|ui| {
                                let mut budget = super::PICTURES_A_FRAME;
                                if let Some(now) = &raw {
                                    self.weenie_row(ui, now, None, &mut budget, out);
                                }
                                if let Some(tok) =
                                    self.weenie_picker(ui, &key, t.get("emotes.choose"))
                                {
                                    out.push(UiAction::Form(w.clone(), edit(column, tok)));
                                }
                            });
                        }
                        Pick::Spell => {
                            ui.horizontal(|ui| {
                                let id = raw.as_deref().and_then(lyceum_foundry::sql_ids::number);
                                let name =
                                    id.and_then(|s| self.spell_name(s)).unwrap_or_else(|| {
                                        id.map(|s| s.to_string()).unwrap_or_default()
                                    });
                                ui.label(name);
                                if let Some(spell) =
                                    self.spell_picker(ui, &key, t.get("emotes.choose"))
                                {
                                    out.push(UiAction::Form(
                                        w.clone(),
                                        edit(column, spell.to_string()),
                                    ));
                                }
                            });
                        }
                        Pick::Motion => {
                            let values =
                                super::fields::enum_values(self.schema.as_ref(), "MotionCommand");
                            let now = raw.as_deref().and_then(lyceum_foundry::sql_ids::number);
                            if values.is_empty() {
                                self.field(
                                    ui,
                                    w,
                                    &key,
                                    &now.map(|n| n.to_string()).unwrap_or_default(),
                                    90.0,
                                    move |v| edit(column, v),
                                    out,
                                );
                            } else {
                                let shown =
                                    values.iter().find(|(v, _)| Some(*v) == now).map_or_else(
                                        || now.map(|n| format!("0x{n:08X}")).unwrap_or_default(),
                                        |(_, w)| w.clone(),
                                    );
                                let mut sorted = values.clone();
                                sorted.sort_by(|a, b| a.1.cmp(&b.1));
                                egui::ComboBox::from_id_salt(("motion", w.clone(), i, n))
                                    .selected_text(shown)
                                    .height(300.0)
                                    .show_ui(ui, |ui| {
                                        for (v, words) in &sorted {
                                            if ui.selectable_label(now == Some(*v), words).clicked()
                                            {
                                                out.push(UiAction::Form(
                                                    w.clone(),
                                                    edit(column, v.to_string()),
                                                ));
                                            }
                                        }
                                    });
                            }
                        }
                    }
                    ui.end_row();
                }
            });
    }

    /// A new action for set `i`, by its kind: the common ones by their words, then every other
    /// by the server's name of its type.
    fn add_action(&mut self, ui: &mut egui::Ui, w: &Weenie, i: usize, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let r = egui::ComboBox::from_id_salt(("add action", w.clone(), i))
            .selected_text(t.get("emotes.add_action"))
            .height(320.0)
            .show_ui(ui, |ui| {
                let others = other_actions(self.schema.as_ref());
                let common = ACTIONS
                    .iter()
                    .map(|k| (k.ty, t.get(&format!("emotes.kind_{}", k.key)).to_owned()));
                for (n, (ty, words)) in common.chain(others).enumerate() {
                    if n == ACTIONS.len() {
                        ui.separator();
                    }
                    let r = ui.selectable_label(false, words);
                    self.editor
                        .picker_rows
                        .push((format!("action-kind:{ty}"), r.rect));
                    if r.clicked() {
                        out.push(UiAction::Form(
                            w.clone(),
                            FormEdit::AddAction {
                                set: i,
                                kind: ty.to_string(),
                            },
                        ));
                    }
                }
            });
        self.editor
            .add_buttons
            .push((format!("emote:{i}:add-action"), r.response.rect));
    }
}

/// The kinds of action the server knows besides the common ones, each by its type and the
/// server's name in plain words, by name.
#[must_use]
pub fn other_actions(schema: Option<&lyceum_foundry::content::Schema>) -> Vec<(u32, String)> {
    let mut out: Vec<(u32, String)> = schema
        .and_then(|s| s.enums.get("EmoteType"))
        .into_iter()
        .flatten()
        .filter(|(ty, _)| **ty != 0 && !ACTIONS.iter().any(|k| k.ty == **ty))
        .map(|(ty, name)| (*ty, super::fields::spaced(name)))
        .collect();
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's emotes)
    use super::*;

    /// A weenie with emote sets of `(category, probability, quest)`, each with a tell.
    fn weenie(sets: &[(u32, f64, Option<&str>)]) -> SqlDoc {
        let mut text = "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES (9, 'thing', 10, '2005-02-09 10:00:00');\n".to_owned();
        for (c, p, q) in sets {
            let q = q.map_or_else(|| "NULL".to_owned(), content::quote);
            text.push_str(&format!("\nINSERT INTO `weenie_properties_emote` (`object_Id`, `category`, `probability`, `weenie_Class_Id`, `style`, `substyle`, `quest`, `vendor_Type`, `min_Health`, `max_Health`)\nVALUES (9, {c}, {p}, NULL, NULL, NULL, {q}, NULL, NULL, NULL);\n\nSET @parent_id = LAST_INSERT_ID();\n\nINSERT INTO `weenie_properties_emote_action` (`emote_Id`, `order`, `type`, `delay`, `extent`, `message`)\nVALUES (@parent_id, 0, 10, 0, 1, 'hello');\n"));
        }
        SqlDoc::parse(&text)
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn every_category_kind_field_and_word_the_emotes_show_has_words_in_the_text() {
        let t = crate::text::read(crate::text::BUILT_IN).unwrap();
        let mut need: Vec<String> = CATEGORIES.map(|c| format!("emotes.category_{c}")).collect();
        for k in ACTIONS {
            need.push(format!("emotes.action_{}", k.key));
            need.push(format!("emotes.kind_{}", k.key));
            for f in k.fields {
                need.push(format!("emotes.field_{}", f.key));
            }
        }
        let code = include_str!("emotes.rs");
        let code = code.split("#[cfg(test)]").next().unwrap_or_default();
        let mut rest = code;
        while let Some(at) = rest.find("\"emotes.") {
            let tail = &rest[at + 1..];
            let end = tail.find('"').unwrap_or(0);
            let key = &tail[..end];
            if !key.ends_with('_') && !key.contains('{') {
                need.push(key.to_owned());
            }
            rest = &tail[end..];
        }
        let missing: Vec<&String> = need.iter().filter(|k| !t.contains_key(*k)).collect();
        assert!(missing.is_empty(), "{missing:?}");
        // Each common action's line fills each of its fields.
        for k in ACTIONS {
            let line = &t[&format!("emotes.action_{}", k.key)];
            for f in k.fields {
                assert!(
                    line.contains(&format!("{{{}}}", f.column)),
                    "{} {line}",
                    k.key
                );
            }
        }
    }

    #[test]
    fn a_new_set_for_an_event_with_sets_is_written_beside_them_and_joins_their_draw() {
        // A greeting, a quest's answer, a death: another greeting goes after the first.
        let mut doc = weenie(&[(7, 1.0, None), (12, 1.0, Some("favor")), (3, 1.0, None)]);
        let c = chances(&doc);
        let categories: Vec<String> = doc
            .emote_sets()
            .iter()
            .map(|s| plain(doc.cell(s.emote, "category")))
            .collect();
        assert_eq!(set_after(&c, &categories, "7", Some(0)), Some(0));
        // Answering a quest of its own: after the last of its category's.
        assert_eq!(set_after(&c, &categories, "12", None), Some(1));
        // A category with none: last.
        assert_eq!(set_after(&c, &categories, "5", None), None);
        let edit = FormEdit::AddEmoteSet {
            category: "7".into(),
            after: set_after(&c, &categories, "7", Some(0)),
        };
        assert!(super::super::form::apply_edit(&mut doc, &edit, None));
        let after: Vec<String> = doc
            .emote_sets()
            .iter()
            .map(|s| plain(doc.cell(s.emote, "category")))
            .collect();
        assert_eq!(after, ["7", "7", "12", "3"]);
        let c = chances(&doc);
        assert_eq!(c.iter().map(|s| s.draw).collect::<Vec<_>>(), [0, 0, 2, 3]);
        // Each set keeps its own action, the new one none.
        let sets = doc.emote_sets();
        let actions: Vec<usize> = sets.iter().map(|s| doc.actions(s).len()).collect();
        assert_eq!(actions, [1, 0, 1, 1]);
    }

    #[test]
    fn the_sets_of_one_event_are_drawn_by_running_totals_and_each_has_its_own_chance() {
        // Three greetings, and a quest's answer apart from them; a tie runs only its first.
        let doc = weenie(&[
            (7, 0.33, None),
            (7, 0.66, None),
            (12, 1.0, Some("favor")),
            (7, 1.0, None),
            (12, 1.0, Some("FAVOR")),
        ]);
        let c = chances(&doc);
        let draws: Vec<usize> = c.iter().map(|s| s.draw).collect();
        assert_eq!(
            draws,
            [0, 0, 2, 0, 2],
            "a quest's case does not set it apart"
        );
        assert!(near(c[0].chance, 0.33) && near(c[1].chance, 0.33) && near(c[3].chance, 0.34));
        assert!(near(c[2].chance, 1.0) && near(c[4].chance, 0.0));
        // Written in any order, the least likely runs first.
        let doc = weenie(&[(5, 0.5, None), (5, 0.1, None)]);
        let c = chances(&doc);
        assert!(near(c[0].chance, 0.4) && near(c[1].chance, 0.1));
    }

    #[test]
    fn a_sets_chance_changed_rewrites_its_draws_totals_and_the_others_keep_theirs() {
        let doc = weenie(&[(7, 0.33, None), (7, 0.66, None), (7, 1.0, None)]);
        let c = chances(&doc);
        // The first to 20%: the others keep 33% and 34%, 13% of the time nothing.
        let p = with_chance(&c, 0, 20.0).unwrap();
        assert_eq!(p.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [0, 1, 2]);
        assert!(
            near(p[0].1, 0.2) && near(p[1].1, 0.53) && near(p[2].1, 0.87),
            "{p:?}"
        );
        // More than there is room for: refused, saying the most.
        let most = with_chance(&c, 1, 50.0).unwrap_err().most;
        assert!((most - 33.0).abs() < 1e-6, "{most}");
        // Taken away: the others keep their chances; the rest runs nothing.
        let w = without(&c, 0);
        assert_eq!(w.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [1, 2]);
        assert!(near(w[0].1, 0.33) && near(w[1].1, 0.67));
        // Joining a full draw: an even share.
        let (others, mine) = joining(&c, 0);
        assert!(near(mine, 1.0));
        let total = |list: &[(usize, f64)]| list.last().map_or(0.0, |(_, p)| *p);
        assert!(near(total(&others), 0.75), "{others:?}");
        // Joining one that runs nothing some of the time: the new one takes that.
        let short = chances(&weenie(&[(7, 0.4, None)]));
        let (others, mine) = joining(&short, 0);
        assert!(others.is_empty() && near(mine, 1.0));
    }

    #[test]
    fn emotes_are_put_back_as_another_weenies_with_their_actions_in_order() {
        let mut doc = weenie(&[(7, 1.0, None)]);
        let theirs = weenie(&[(7, 0.5, None), (12, 1.0, Some("favor"))]);
        write_emotes(&mut doc, &emotes_of(&theirs));
        assert_eq!(emotes_of(&doc), emotes_of(&theirs));
        assert_eq!(doc.emote_sets().len(), 2);
        assert_eq!(
            doc.render()
                .matches("SET @parent_id = LAST_INSERT_ID();")
                .count(),
            2
        );
        write_emotes(&mut doc, &[]);
        assert!(doc.emote_sets().is_empty());
        assert!(!doc.render().contains("weenie_properties_emote"));
    }

    #[test]
    fn a_common_action_is_a_line_of_plain_words_and_any_other_is_its_type() {
        let t = crate::text::Text::built_in().table();
        let value = |c: &str| match c {
            "message" => "Welcome, traveler.".to_owned(),
            "weenie_Class_Id" => "Town Stamp".to_owned(),
            "stack_Size" => "2".to_owned(),
            "amount_64" => thousands(15000),
            "delay" => "0".to_owned(),
            _ => String::new(),
        };
        assert_eq!(
            action_words(&t, 10, "Tell", &value),
            t.get("emotes.action_tell")
                .replace("{message}", "Welcome, traveler.")
        );
        let give = action_words(&t, 3, "Give", &value);
        assert!(give.contains("2 Town Stamp"), "{give}");
        assert!(action_words(&t, 2, "AwardXP", &value).contains("15,000"));
        assert_eq!(action_words(&t, 36, "Inq int stat", &value), "Inq int stat");
        let later = |c: &str| {
            if c == "delay" {
                "2.5".to_owned()
            } else {
                value(c)
            }
        };
        assert!(action_words(&t, 12, "TurnToTarget", &later).contains("2.5"));
        // Words over several lines on one; nothing yet, something to see.
        let lines = |c: &str| match c {
            "message" => "One.\nTwo.".to_owned(),
            _ => String::new(),
        };
        assert!(action_words(&t, 10, "Tell", &lines).contains("One. Two."));
        assert!(action_words(&t, 5, "Motion", &lines).contains('…'));
        assert_eq!(thousands(-1_234_567), "-1,234,567");
        assert_eq!(thousands(999), "999");
    }
}
