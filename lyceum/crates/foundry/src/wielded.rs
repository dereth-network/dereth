//! A wielded treasure read as the server draws it, and written back so that it draws the same.
//!
//! The server reads a wielded treasure's rows in order:
//!
//! - **Sets.** Rows are drawn in sets: one row of a set is made at most, each at its own chance
//!   (`probability`, 0 to 1), and what the set's chances leave short of 1 makes nothing. A row
//!   starts a new set when it says so (`set_Start`), or when the chances of the set before it
//!   already reach 1; they are added up in single precision. A row's chance is what is left of 1
//!   at most: a set whose chances add up past 1 makes its later rows less likely than they say.
//! - **Sub-sets.** A row that has a sub-set (`has_Sub_Set`) is followed by the rows of its own
//!   sets, drawn only when it is made (and made or not, they are read past). Those rows end at a
//!   row that continues the set before them (`continues_Previous_Set`), which takes up the sets
//!   the row with the sub-set was in, where they were. A row that continues a set when no sub-set
//!   came before it ends the table: the rows after it are never read.
//! - A row of class 0 makes nothing, but its sub-set is drawn all the same when it is chosen.
//!
//! [`read`] makes of the rows a tree of sets, each row with the chance it is made in its set as
//! the server reads it ([`Entry::chance`]), and the rows the server never reads ([`Wielded::unread`]).
//! [`write()`] makes rows of a tree that the server reads back as the same tree: each set's first row
//! starts it, each row's chance is written as it is, a row with sub-sets says so and the row after
//! them continues. Where a row would be read into a sub-set it does not belong to (after sub-sets
//! that end with sub-sets of their own), a row of class 0 at no chance is written between to end
//! them, which [`read`] passes over. The rows the server never reads are left out.
//!
//! The editing functions change a tree so that its sets still add up to 1 at most: a chance
//! changed takes from what makes nothing, and is refused when its set has no room for it.

use crate::sql_ids::WieldedItem;

/// One row of a set: what it makes (its row as written, but for the columns [`write()`] sets), its
/// chance of being made in its set, 0 to 1, and the sets drawn when it is made.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub item: WieldedItem,
    pub chance: f64,
    pub then: Vec<Set>,
}

/// One set of rows, of which one is made at most.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Set {
    pub entries: Vec<Entry>,
}

impl Set {
    /// The chances of its rows together, 0 to 1.
    #[must_use]
    pub fn total(&self) -> f64 {
        self.entries.iter().map(|e| e.chance).sum()
    }

    /// The chance it makes nothing.
    #[must_use]
    pub fn nothing(&self) -> f64 {
        (1.0 - self.total()).max(0.0)
    }
}

/// A wielded treasure as the server draws it: its sets, and the rows it never reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wielded {
    pub sets: Vec<Set>,
    pub unread: Vec<WieldedItem>,
}

/// Where an entry is: the set and the place in it of each entry on the way down, the first of
/// the top's sets.
pub type At = [(usize, usize)];

/// Whether `item` is a row [`write()`] puts in to end sub-sets: class 0, no chance, no sub-set.
fn is_spacer(e: &Entry) -> bool {
    crate::sql_ids::number(&e.item.weenie) == Some(0) && e.chance <= 0.0 && e.then.is_empty()
}

/// The rows of one level, from `rows[*at]` on: its sets, and whether it ended at a row that
/// continues a set (left for the level above), which at the top ends the table.
fn read_level(rows: &[WieldedItem], at: &mut usize) -> (Vec<Set>, bool) {
    let mut sets: Vec<Set> = Vec::new();
    // The chances of the set being drawn, added up as the server adds them.
    let mut total = 0.0f32;
    let mut after_sub_set = false;
    while *at < rows.len() {
        let row = &rows[*at];
        if row.continues {
            if !after_sub_set {
                return (sets, true);
            }
            after_sub_set = false;
        }
        if sets.is_empty() || row.set_start || total >= 1.0 {
            sets.push(Set::default());
            total = 0.0;
        }
        let before = total.min(1.0);
        total += row.probability;
        // To a ten-millionth, which single precision holds no closer: 0.3 read as 0.3.
        let chance = (f64::from((total.min(1.0) - before).max(0.0)) * 1e7).round() / 1e7;
        *at += 1;
        let then = if row.has_sub_set {
            let (then, _) = read_level(rows, at);
            after_sub_set = true;
            then
        } else {
            Vec::new()
        };
        let mut item = row.clone();
        item.set_start = false;
        item.has_sub_set = false;
        item.continues = false;
        sets.last_mut()
            .expect("a set is started before its first row")
            .entries
            .push(Entry { item, chance, then });
    }
    (sets, false)
}

/// The sets `sets` with the rows [`write()`] put in to end sub-sets taken out, and a set left
/// with none gone.
fn without_spacers(sets: Vec<Set>) -> Vec<Set> {
    sets.into_iter()
        .map(|s| Set {
            entries: s
                .entries
                .into_iter()
                .filter(|e| !is_spacer(e))
                .map(|mut e| {
                    e.then = without_spacers(std::mem::take(&mut e.then));
                    e
                })
                .collect(),
        })
        .filter(|s| !s.entries.is_empty())
        .collect()
}

/// A wielded treasure's rows, in order, read as the server draws them.
#[must_use]
pub fn read(rows: &[WieldedItem]) -> Wielded {
    let mut at = 0;
    let (sets, _) = read_level(rows, &mut at);
    Wielded {
        sets: without_spacers(sets),
        unread: rows[at.min(rows.len())..].to_vec(),
    }
}

/// A row that ends sub-sets: class 0, no chance, continuing the set before them.
fn spacer() -> WieldedItem {
    WieldedItem {
        weenie: "0".into(),
        continues: true,
        ..WieldedItem::default()
    }
}

/// Write `sets` (one level) after `out`: `pending` counts the levels just written that ended
/// right after sub-sets of their own, each of which needs a row of its own before the next row of
/// a level above can be read as that level's. Answers whether this level ended right after
/// sub-sets.
fn write_level(sets: &[Set], out: &mut Vec<WieldedItem>, pending: &mut usize) -> bool {
    let mut after_sub_set = false;
    for set in sets {
        for (j, e) in set.entries.iter().enumerate() {
            for _ in 0..std::mem::take(pending) {
                out.push(spacer());
            }
            let mut item = e.item.clone();
            item.set_start = j == 0;
            item.continues = after_sub_set;
            #[allow(clippy::cast_possible_truncation)] // the server's own single precision
            {
                item.probability = e.chance as f32;
            }
            let then: Vec<Set> = e
                .then
                .iter()
                .filter(|s| !s.entries.is_empty())
                .cloned()
                .collect();
            item.has_sub_set = !then.is_empty();
            out.push(item);
            after_sub_set = false;
            if !then.is_empty() {
                if write_level(&then, out, pending) {
                    *pending += 1;
                }
                after_sub_set = true;
            }
        }
    }
    after_sub_set
}

/// The rows that write `w`, in order: read back, the same sets at the same chances. The rows
/// the server never reads are left out.
#[must_use]
pub fn write(w: &Wielded) -> Vec<WieldedItem> {
    let mut out = Vec::new();
    let mut pending = 0;
    write_level(&w.sets, &mut out, &mut pending);
    out
}

/// What a change of a tree that is not made says: a chance more than its set has room for
/// (the most it can have, in percent), or none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Refused {
    TooLikely {
        most: f64,
    },
    NoChance,
    /// No entry or set where it was asked for.
    Gone,
}

impl Wielded {
    /// The sets of the level under the entry at `at` (the top's for none).
    fn level_mut(&mut self, at: &At) -> Option<&mut Vec<Set>> {
        let mut sets = &mut self.sets;
        for (s, e) in at {
            sets = &mut sets.get_mut(*s)?.entries.get_mut(*e)?.then;
        }
        Some(sets)
    }

    /// The entry at `at`.
    #[must_use]
    pub fn entry(&self, at: &At) -> Option<&Entry> {
        let ((last_s, last_e), up) = at.split_last()?;
        let mut sets = &self.sets;
        for (s, e) in up {
            sets = &sets.get(*s)?.entries.get(*e)?.then;
        }
        sets.get(*last_s)?.entries.get(*last_e)
    }

    /// The entry at `at`, to change.
    pub fn entry_mut(&mut self, at: &At) -> Option<&mut Entry> {
        let ((s, e), up) = at.split_last()?;
        self.level_mut(up)?.get_mut(*s)?.entries.get_mut(*e)
    }

    /// The entry at `at` given chance `percent` of its set: what the set makes nothing at takes
    /// the difference, the others keep theirs.
    ///
    /// # Errors
    /// No chance, or more than the set has room for besides its others (the most it can have);
    /// nothing is changed.
    pub fn set_chance(&mut self, at: &At, percent: f64) -> Result<(), Refused> {
        let ((s, e), up) = at.split_last().ok_or(Refused::Gone)?;
        let set = self
            .level_mut(up)
            .and_then(|l| l.get_mut(*s))
            .ok_or(Refused::Gone)?;
        if *e >= set.entries.len() {
            return Err(Refused::Gone);
        }
        let want = percent / 100.0;
        if !want.is_finite() || want <= 0.0 {
            return Err(Refused::NoChance);
        }
        let others: f64 = set
            .entries
            .iter()
            .enumerate()
            .filter(|(j, _)| j != e)
            .map(|(_, x)| x.chance)
            .sum();
        if others + want > 1.0 + 1e-9 {
            return Err(Refused::TooLikely {
                most: ((1.0 - others) * 100.0).max(0.0),
            });
        }
        set.entries[*e].chance = want.min(1.0 - others).max(0.0);
        Ok(())
    }

    /// `item` added to set `set` of the level under `at` (the top's for none), after its last
    /// entry: at what the set makes nothing at, or when that is nothing, an even share, the
    /// set's other entries made less likely to give it room.
    ///
    /// # Errors
    /// No such set.
    pub fn add_to_set(&mut self, at: &At, set: usize, item: WieldedItem) -> Result<(), Refused> {
        let s = self
            .level_mut(at)
            .and_then(|l| l.get_mut(set))
            .ok_or(Refused::Gone)?;
        let rest = s.nothing();
        let chance = if rest > 1e-6 {
            rest
        } else {
            #[allow(clippy::cast_precision_loss)] // a set's few entries
            let n = s.entries.len() as f64;
            for e in &mut s.entries {
                e.chance *= n / (n + 1.0);
            }
            1.0 / (n + 1.0)
        };
        s.entries.push(Entry {
            item,
            chance,
            then: Vec::new(),
        });
        Ok(())
    }

    /// `item` added as a set of its own after the others of the level under `at` (the top's for
    /// none), made always.
    ///
    /// # Errors
    /// No entry at `at`.
    pub fn add_set(&mut self, at: &At, item: WieldedItem) -> Result<(), Refused> {
        self.level_mut(at).ok_or(Refused::Gone)?.push(Set {
            entries: vec![Entry {
                item,
                chance: 1.0,
                then: Vec::new(),
            }],
        });
        Ok(())
    }

    /// The entry at `at` taken away with its sub-sets; a set left with none goes.
    ///
    /// # Errors
    /// No entry there.
    pub fn remove(&mut self, at: &At) -> Result<(), Refused> {
        let ((s, e), up) = at.split_last().ok_or(Refused::Gone)?;
        let level = self.level_mut(up).ok_or(Refused::Gone)?;
        let set = level.get_mut(*s).ok_or(Refused::Gone)?;
        if *e >= set.entries.len() {
            return Err(Refused::Gone);
        }
        set.entries.remove(*e);
        if set.entries.is_empty() {
            level.remove(*s);
        }
        Ok(())
    }

    /// The entry at `at` and the one after it in its set (`later`), or before, change places;
    /// each keeps its chance.
    ///
    /// # Errors
    /// No entry there, or none to change places with.
    pub fn swap(&mut self, at: &At, later: bool) -> Result<(), Refused> {
        let ((s, e), up) = at.split_last().ok_or(Refused::Gone)?;
        let set = self
            .level_mut(up)
            .and_then(|l| l.get_mut(*s))
            .ok_or(Refused::Gone)?;
        let other = if later {
            e.checked_add(1)
        } else {
            e.checked_sub(1)
        }
        .filter(|o| *o < set.entries.len())
        .ok_or(Refused::Gone)?;
        if *e >= set.entries.len() {
            return Err(Refused::Gone);
        }
        set.entries.swap(*e, other);
        Ok(())
    }

    /// How many rows it has, its sub-sets' with them.
    #[must_use]
    pub fn len(&self) -> usize {
        fn count(sets: &[Set]) -> usize {
            sets.iter()
                .flat_map(|s| &s.entries)
                .map(|e| 1 + count(&e.then))
                .sum()
        }
        count(&self.sets)
    }

    /// Whether it makes nothing at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    /// Every entry, with where it is and how deep, in the order its rows are written: an entry,
    /// then its sub-sets.
    #[must_use]
    pub fn entries(&self) -> Vec<(Vec<(usize, usize)>, &Entry)> {
        fn walk<'a>(
            sets: &'a [Set],
            at: &mut Vec<(usize, usize)>,
            out: &mut Vec<(Vec<(usize, usize)>, &'a Entry)>,
        ) {
            for (s, set) in sets.iter().enumerate() {
                for (e, entry) in set.entries.iter().enumerate() {
                    at.push((s, e));
                    out.push((at.clone(), entry));
                    walk(&entry.then, at, out);
                    at.pop();
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.sets, &mut Vec::new(), &mut out);
        out
    }

    /// The chance each entry is made at all, in [`Self::entries`]' order: its chance in its set,
    /// times its way down's.
    #[must_use]
    pub fn odds(&self) -> Vec<f64> {
        fn walk(sets: &[Set], over: f64, out: &mut Vec<f64>) {
            for set in sets {
                for e in &set.entries {
                    out.push(over * e.chance);
                    walk(&e.then, over * e.chance, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.sets, 1.0, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a wielded treasure's sets read and written as the server draws them)
    use super::*;

    /// A row making class `class` at `p`, with its flags: `s` starts a set, `h` has a sub-set,
    /// `c` continues a set.
    fn row(class: u32, p: f32, flags: &str) -> WieldedItem {
        WieldedItem {
            weenie: class.to_string(),
            probability: p,
            set_start: flags.contains('s'),
            has_sub_set: flags.contains('h'),
            continues: flags.contains('c'),
            ..WieldedItem::default()
        }
    }

    /// The class of each row the tree makes, and its chance in its set, in the order written.
    fn shape(w: &Wielded) -> Vec<(usize, u32, f64)> {
        w.entries()
            .into_iter()
            .map(|(at, e)| {
                (
                    at.len(),
                    crate::sql_ids::number(&e.item.weenie).unwrap_or(0),
                    (e.chance * 1e4).round() / 1e4,
                )
            })
            .collect()
    }

    /// A small random number generator, the same numbers every run.
    struct Lcg(u64);

    impl Lcg {
        /// A number from 0 up to 1, in single precision as the server draws them.
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        fn next(&mut self) -> f32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 40) as f32 / (1u64 << 24) as f32).min(0.999_999_9)
        }
    }

    /// The rows made by one draw of `rows`, by their place, drawn the way the server reads them:
    /// row by row, a running total, a new draw at each set, and a sub-set's rows walked past
    /// unless its row was made.
    fn draw(rows: &[WieldedItem], rng: &mut Lcg) -> Vec<usize> {
        fn level(
            rows: &[WieldedItem],
            i: &mut usize,
            skip: bool,
            rng: &mut Lcg,
            out: &mut Vec<usize>,
        ) {
            let mut roll = rng.next();
            let mut total = 0.0f32;
            let mut rolled = false;
            let mut continued = false;
            while *i < rows.len() {
                let r = &rows[*i];
                if r.continues {
                    if !continued {
                        return;
                    }
                    continued = false;
                }
                let mut made = false;
                if !skip {
                    if r.set_start || total >= 1.0 {
                        roll = rng.next();
                        total = 0.0;
                        rolled = false;
                    }
                    total += r.probability;
                    if roll < total && !rolled {
                        rolled = true;
                        made = true;
                        out.push(*i);
                    }
                }
                *i += 1;
                if r.has_sub_set {
                    level(rows, i, !made, rng, out);
                    continued = true;
                }
            }
        }
        let mut out = Vec::new();
        level(rows, &mut 0, false, rng, &mut out);
        out
    }

    /// How often each class is made in `n` draws of `rows`, as a share of the draws.
    fn made(rows: &[WieldedItem], n: usize) -> std::collections::BTreeMap<String, f64> {
        let mut rng = Lcg(7);
        let mut count: std::collections::BTreeMap<String, f64> = Default::default();
        for _ in 0..n {
            for i in draw(rows, &mut rng) {
                *count.entry(rows[i].weenie.clone()).or_default() += 1.0;
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let n = n as f64;
        count.values_mut().for_each(|v| *v /= n);
        count.retain(|k, _| k != "0");
        count
    }

    /// What `w` makes, by class: the odds of each entry, added up by class.
    fn odds_by_class(w: &Wielded) -> std::collections::BTreeMap<String, f64> {
        let mut out: std::collections::BTreeMap<String, f64> = Default::default();
        for ((_, e), p) in w.entries().into_iter().zip(w.odds()) {
            *out.entry(e.item.weenie.clone()).or_default() += p;
        }
        out.retain(|k, v| k != "0" && *v > 0.0);
        out
    }

    fn near(
        a: &std::collections::BTreeMap<String, f64>,
        b: &std::collections::BTreeMap<String, f64>,
        by: f64,
    ) {
        let keys: std::collections::BTreeSet<&String> = a.keys().chain(b.keys()).collect();
        for k in keys {
            let (x, y) = (
                a.get(k).copied().unwrap_or(0.0),
                b.get(k).copied().unwrap_or(0.0),
            );
            assert!((x - y).abs() < by, "{k}: {x} against {y}\n{a:?}\n{b:?}");
        }
    }

    /// A treasure of the server's own shape: an always row; a set of one row at 30% with a
    /// sub-set of two halves; a row continuing that set but starting its own, with a sub-set of
    /// its own continued by a row that starts the last set, which makes nothing half the time.
    fn crossbow_and_bolts() -> Vec<WieldedItem> {
        vec![
            row(273, 1.0, "s"),
            row(258, 0.3, "sh"),
            row(265, 0.5, "s"),
            row(259, 0.5, ""),
            row(547, 0.3, "shc"),
            row(260, 1.0, "s"),
            row(549, 0.4, "sc"),
            row(297, 0.3, "s"),
            row(294, 0.1, ""),
            row(1442, 0.1, ""),
        ]
    }

    #[test]
    fn the_rows_are_read_into_sets_and_sub_sets_each_row_at_its_chance_in_its_set() {
        let w = read(&crossbow_and_bolts());
        assert_eq!(
            shape(&w),
            [
                (1, 273, 1.0),
                (1, 258, 0.3),
                (2, 265, 0.5),
                (2, 259, 0.5),
                (1, 547, 0.3),
                (2, 260, 1.0),
                (1, 549, 0.4),
                (1, 297, 0.3),
                (1, 294, 0.1),
                (1, 1442, 0.1),
            ]
        );
        // Five sets at the top: 273; 258; 547; 549; and the last three, half of it nothing.
        assert_eq!(w.sets.len(), 5);
        assert!((w.sets[4].nothing() - 0.5).abs() < 1e-6);
        assert!(w.unread.is_empty());
        near(
            &made(&crossbow_and_bolts(), 200_000),
            &odds_by_class(&w),
            0.005,
        );
    }

    #[test]
    fn a_set_past_a_hundred_percent_makes_its_later_rows_less_likely_and_the_next_row_starts_a_set()
    {
        // 30% and 50% then 40% that is 20% in truth; the next row starts a set though it does not
        // say so.
        let rows = vec![
            row(1, 0.3, "s"),
            row(2, 0.5, ""),
            row(3, 0.4, ""),
            row(4, 0.25, ""),
        ];
        let w = read(&rows);
        assert_eq!(
            shape(&w),
            [(1, 1, 0.3), (1, 2, 0.5), (1, 3, 0.2), (1, 4, 0.25)]
        );
        assert_eq!(w.sets.len(), 2);
        near(&made(&rows, 200_000), &odds_by_class(&w), 0.005);
        // Written back: the chances the server read, each set started.
        let back = write(&w);
        assert!((back[2].probability - 0.2).abs() < 1e-6 && back[3].set_start);
        assert_eq!(shape(&read(&back)), shape(&w));
    }

    #[test]
    fn a_row_continuing_a_set_with_no_sub_set_before_it_ends_the_table_and_what_follows_is_never_read(
    ) {
        // A sub-set, the row that continues after it, then rows that each say they continue too:
        // the first of them ends the table.
        let rows = vec![
            row(10, 0.6, "sh"),
            row(11, 1.0, "s"),
            row(12, 0.1, "sc"),
            row(13, 0.1, "c"),
            row(14, 0.5, "c"),
            row(15, 0.7, "s"),
        ];
        let w = read(&rows);
        assert_eq!(shape(&w), [(1, 10, 0.6), (2, 11, 1.0), (1, 12, 0.1)]);
        assert_eq!(
            w.unread
                .iter()
                .map(|r| r.weenie.as_str())
                .collect::<Vec<_>>(),
            ["13", "14", "15"]
        );
        near(&made(&rows, 100_000), &odds_by_class(&w), 0.005);
        // Its first row continuing: the table makes nothing.
        let none = read(&[row(1, 0.1, "shc"), row(2, 1.0, "s")]);
        assert!(none.is_empty() && none.unread.len() == 2);
        // Written, the rows never read are left out, and what is read draws the same.
        let back = write(&w);
        assert_eq!(back.len(), 3);
        near(&made(&back, 100_000), &odds_by_class(&w), 0.005);
    }

    #[test]
    fn sub_sets_inside_sub_sets_are_written_so_the_rows_after_them_are_read_where_they_belong() {
        // A, with a sub-set holding B, with a sub-set holding C; then D, back at the top, in A's
        // set; then E in a set of its own.
        let c = Entry {
            item: row(3, 0.0, ""),
            chance: 0.5,
            then: Vec::new(),
        };
        let b = Entry {
            item: row(2, 0.0, ""),
            chance: 0.8,
            then: vec![Set { entries: vec![c] }],
        };
        let a = Entry {
            item: row(1, 0.0, ""),
            chance: 0.25,
            then: vec![Set { entries: vec![b] }],
        };
        let d = Entry {
            item: row(4, 0.0, ""),
            chance: 0.5,
            then: Vec::new(),
        };
        let e = Entry {
            item: row(5, 0.0, ""),
            chance: 1.0,
            then: Vec::new(),
        };
        let w = Wielded {
            sets: vec![
                Set {
                    entries: vec![a, d],
                },
                Set { entries: vec![e] },
            ],
            unread: Vec::new(),
        };
        let rows = write(&w);
        // A row of nothing ends B's level, whose last row had a sub-set, so D is read at the top.
        assert_eq!(
            rows.iter().map(|r| r.weenie.as_str()).collect::<Vec<_>>(),
            ["1", "2", "3", "0", "4", "5"]
        );
        assert!(rows[4].continues && !rows[4].set_start && rows[5].set_start);
        let again = read(&rows);
        assert_eq!(shape(&again), shape(&w));
        near(&made(&rows, 200_000), &odds_by_class(&w), 0.005);
        // A tree of the server's own: written and read back, the same; drawn, the same.
        let server = read(&crossbow_and_bolts());
        let back = write(&server);
        assert_eq!(shape(&read(&back)), shape(&server));
        near(
            &made(&back, 200_000),
            &made(&crossbow_and_bolts(), 200_000),
            0.006,
        );
    }

    #[test]
    fn a_chance_changed_takes_from_nothing_and_more_than_the_set_has_room_for_is_refused() {
        let mut w = read(&crossbow_and_bolts());
        // The last set, 30% 10% 10%: the first made 40%, nothing goes from 50% to 40%.
        w.set_chance(&[(4, 0)], 40.0).unwrap();
        assert!((w.sets[4].nothing() - 0.4).abs() < 1e-6);
        assert_eq!(
            w.set_chance(&[(4, 1)], 60.0),
            Err(Refused::TooLikely { most: 50.0 })
        );
        assert_eq!(w.set_chance(&[(4, 1)], 0.0), Err(Refused::NoChance));
        // In a sub-set: 265 and 259 at half each; 265 at 70% has no room.
        assert!(matches!(
            w.set_chance(&[(1, 0), (0, 0)], 70.0),
            Err(Refused::TooLikely { .. })
        ));
        w.set_chance(&[(1, 0), (0, 1)], 20.0).unwrap();
        assert!((w.sets[1].entries[0].then[0].nothing() - 0.3).abs() < 1e-6);
        // Written and read back: the chances set.
        let back = read(&write(&w));
        assert_eq!(shape(&back), shape(&w));
    }

    #[test]
    fn entries_added_taken_away_and_moved_keep_their_sets_whole() {
        let mut w = read(&crossbow_and_bolts());
        // Added to the last set: it takes what made nothing.
        w.add_to_set(&[], 4, row(9, 0.0, "")).unwrap();
        assert!((w.sets[4].entries[3].chance - 0.5).abs() < 1e-9);
        // Added to a full set: an even share, the others made less likely.
        w.add_to_set(&[(1, 0)], 0, row(8, 0.0, "")).unwrap();
        let sub = &w.sets[1].entries[0].then[0];
        assert!(sub
            .entries
            .iter()
            .all(|e| (e.chance - 1.0 / 3.0).abs() < 1e-9));
        // A set of its own under 549, always.
        w.add_set(&[(3, 0)], row(7, 0.0, "")).unwrap();
        assert_eq!(w.sets[3].entries[0].then.len(), 1);
        let back = read(&write(&w));
        assert_eq!(shape(&back), shape(&w));
        // Moved: each keeps its chance.
        w.swap(&[(4, 0)], true).unwrap();
        assert_eq!(w.sets[4].entries[1].item.weenie, "297");
        assert!(w.swap(&[(0, 0)], true).is_err(), "nothing after it");
        // Taken away with its sub-set; a set left empty goes.
        let n = w.len();
        w.remove(&[(1, 0)]).unwrap();
        assert_eq!(w.len(), n - 4);
        assert_eq!(w.sets.len(), 4);
    }
}
