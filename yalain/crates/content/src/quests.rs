//! Quests the world runs: each quest's definition as its journal shows it (name, description, icon,
//! the journal entry of each of its phases, the challenge, how long it lasts, when it may be taken again, the
//! experience it pays), whether it is hidden (kept but never shown), the effects each kind of update
//! puts on the player, and the steps that move it on (what triggers each, what the giver says, the
//! update it makes, what it pays and gives). Every row says where it came from.
//!
//! The text form is tab-separated, one row per line, the first column saying which kind of row:
//!
//! - `quest`: id, name `string:table`, description `string:table`, icon, play-effect flag (0/1),
//!   journal entries `string:table` by phase joined by `,`, challenge, duration in seconds (0 for
//!   none), retry wait in seconds, completion experience, can-be-cancelled flag, provenance; then,
//!   optionally, the hidden flag (0/1), the update effects (`KIND:STATE`, the update's kind and
//!   the effect's saved state, joined by `,`), the phases' subquests (`PHASE:ANY:QUEST/QUEST...`,
//!   ANY 1 when one of them is enough, joined by `,`), the least level it is given at (0 for any),
//!   whether it fails when its player dies (0/1), whether it is given only once (0/1), how a kill's
//!   move on it reaches the killer's fellows (added up: 1 it does, 2 only those near the killer's
//!   level, 4 only those near the killer), whether its last phase waits for something other than
//!   a kill to end it (0/1), and the one phase its duration runs in (0 or left out: all of them);
//! - `step`: quest, the phase it needs (0: the quest not under way), the trigger (`use:TEMPLATE`,
//!   `useon:ITEM:TARGET`, `give:ITEM:TARGET`, `arrive:TEMPLATE`, `take:TEMPLATE`, `kill:TEMPLATE`, `done:QUEST`, `given:QUEST`), the giver's line
//!   (`string:table`, `=` and literal text, or empty), the update (`bestow`, `advance:N`, `complete`), experience, items
//!   given (templates joined by `,`), provenance; then, optionally, what else it requires (other
//!   quests at a phase, done or free to be given: `QUEST:N`, `QUEST:done` or `QUEST:free`, joined by `,`), the items it takes from
//!   the pack (templates joined by `,`), where it takes the player (`CELL:X:Y:Z:HEADING`; an item's
//!   use on an object makes the update on arrival), its effects (saved states joined by `,`), the text shown after them
//!   (`string:table`), whether its update is made for the player's fellows too (0/1), and for a
//!   give how many of the item it wants (empty for whatever is given);
//! - `give`: what a person or an object does with an item given it that moves no quest: the template
//!   given to, the item's template (empty: any item), how many of it it wants (empty: whatever is
//!   given), the items given back (templates joined by `,`, a count after `x`), its effects (saved
//!   states joined by `,`), the text shown after them (`string:table`), what the person or the object plays
//!   (an effect's id, or empty), what it requires (as a step), provenance.

use std::fmt::Write as _;

use crate::talk::Words;

/// A quest's definition.
#[derive(Debug, Clone, PartialEq)]
pub struct QuestDef {
    pub id: u32,
    pub name: (u32, u32),
    pub description: (u32, u32),
    pub icon: u32,
    pub play_fx: bool,
    /// The journal entry of each phase, from the first.
    pub journals: Vec<(u32, u32)>,
    pub challenge: i32,
    pub duration: f64,
    /// The one phase its duration runs in, from when that phase comes (0: all, from its giving).
    pub timed_phase: u32,
    pub retry: f64,
    pub xp: u64,
    pub cancellable: bool,
    pub placed: String,
    /// Kept for the player but never shown: in no journal, no update told.
    pub hidden: bool,
    /// The effects an update puts on the player, by the update's kind (bestow, a phase's advance,
    /// complete, fail): the kind and the effect's saved state, in order.
    pub effects: Vec<(u32, u32)>,
    /// The phases that are done by doing other quests (subquests), each phase's in its order.
    pub subquests: Vec<Subquests>,
    /// The least level a player is given it at (0: any).
    pub min_level: i32,
    /// Whether it fails when its player dies.
    pub fails_on_death: bool,
    /// Whether it is given only once: done, it is never given again (else it is given again once
    /// its wait is over, at once when it has none). The install's setup of the quest says so; a
    /// world's row may say otherwise.
    pub once: bool,
    /// How a kill's move on it reaches the killer's fellows.
    pub fellows: Fellows,
    /// Its last phase waits for something other than a kill to end it (a phase that reads "return
    /// to" someone).
    pub last_waits: bool,
    /// Given only to heroes.
    pub hero_only: bool,
    /// How its completion's experience is worked out, beyond its own amount.
    pub pay: CompletionPay,
    /// A quest a player is to have at a status to be given it: the status as its update names it (a
    /// status, or under way at a phase).
    pub required: Option<(u32, u32)>,
}

/// A status a required quest is to be in when its setup names none: completed.
pub const COMPLETED: u32 = 0x3000_0000;

/// How a quest's completion works out its experience, beyond an amount of its own.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CompletionPay {
    /// It pays this share of what the player's level takes to reach the next, in place of an amount
    /// (0: it pays its amount).
    pub share: f32,
    /// A share pays at least this (0: no such limit).
    pub least: u64,
    /// A share pays at most this (0: no such limit).
    pub most: u64,
    /// A share pays at most this share of what this level takes to reach the next.
    pub most_at_level: Option<(f32, i32)>,
    /// It is paid whole, whatever its challenge against the player's level.
    pub flat: bool,
    /// What it adds to the player's quest rating, every time, whatever the player's level.
    pub points: u32,
}

impl CompletionPay {
    fn word(&self) -> String {
        let (most_share, most_level) = self.most_at_level.unwrap_or((0.0, 0));
        format!(
            "{}:{}:{}:{most_share}:{most_level}:{}:{}",
            self.share,
            self.least,
            self.most,
            u8::from(self.flat),
            self.points
        )
    }

    fn of_word(w: &str) -> Result<Self, String> {
        let c: Vec<&str> = w.split(':').collect();
        let [share, least, most, most_share, most_level, flat, points] = c[..] else {
            return Err(format!("a completion's pay has seven parts, not {w}"));
        };
        fn num<T: std::str::FromStr>(s: &str) -> Result<T, String> {
            s.parse().map_err(|_| format!("{s} is not a number"))
        }
        let most_share: f32 = num(most_share)?;
        let most_level: i32 = num(most_level)?;
        Ok(Self {
            share: num(share)?,
            least: num(least)?,
            most: num(most)?,
            most_at_level: (most_level != 0).then_some((most_share, most_level)),
            flat: flat == "1",
            points: num(points)?,
        })
    }
}

/// How a kill's move on a quest reaches the killer's fellows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fellows {
    /// It reaches them.
    pub shared: bool,
    /// Only those near the killer's level.
    pub near_level: bool,
    /// Only those near the killer.
    pub near_by: bool,
}

impl Fellows {
    fn word(self) -> u8 {
        u8::from(self.shared) | u8::from(self.near_level) << 1 | u8::from(self.near_by) << 2
    }

    fn of_word(w: u8) -> Self {
        Self {
            shared: w & 1 != 0,
            near_level: w & 2 != 0,
            near_by: w & 4 != 0,
        }
    }
}

/// A phase of a quest done by doing other quests: given with the phase, kept hidden, the phase
/// moving on once all of them (or one, for `any`) are done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subquests {
    pub phase: u32,
    pub quests: Vec<u32>,
    pub any: bool,
}

/// What moves a quest on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// Using an object of a template (a person, a stone, an item held).
    Use(u32),
    /// Using a held item of a template on an object of another (a glyph on a vault).
    UseOn(u32, u32),
    /// Giving an item of a template to a person or an object of another (an item dragged onto it).
    Give(u32, u32),
    /// Arriving through a portal of a template.
    Arrive(u32),
    /// Taking an item of a template.
    Take(u32),
    /// Killing a creature of a template.
    Kill(u32),
    /// Completing a quest.
    Done(u32),
    /// Being given a quest, shown or hidden.
    Given(u32),
}

/// The update a step makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Update {
    Bestow,
    Advance(u32),
    Complete,
    /// Fails it, from any phase but none.
    Fail,
    /// Moves it nowhere: the step only says, gives and sends.
    Nothing,
}

/// What a step requires of another quest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// Under way at this phase.
    Phase(u32),
    /// Done.
    Done,
    /// Free to be given: never had, or its wait to be taken again over.
    Free,
}

/// A place a step takes the player to: a cell, a point in its landblock, a heading (degrees
/// clockwise from north times 255/360).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Place {
    pub cell: u32,
    pub origin: [f32; 3],
    pub heading: u8,
}

/// A room of a row of like rooms: where a player lands in it, and the cell its creature stands in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Room {
    pub place: Place,
    pub holds: u32,
}

/// One step of a quest.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub quest: u32,
    /// The phase the quest must be at (0: not under way).
    pub needs: u32,
    pub trigger: Trigger,
    /// What the person used says.
    pub tell: Option<Words>,
    pub update: Update,
    pub xp: u64,
    pub gives: Vec<u32>,
    /// How many of each item given, in order (1 when left out).
    pub counts: Vec<u32>,
    /// The effects it puts on the player (their saved states), and a text shown after them.
    pub effects: Vec<u32>,
    pub text: Option<(u32, u32)>,
    pub placed: String,
    /// Other quests it requires, at a phase or done.
    pub requires: Vec<(u32, Need)>,
    /// The items it takes from the pack.
    pub takes: Vec<u32>,
    /// Where it takes the player: an item's use on an object makes its update on arrival, a use of
    /// a person or an object before the trip.
    pub to: Option<Place>,
    /// The races it is for (none: every race).
    pub races: Vec<u32>,
    /// Its effects come after its update, not before it.
    pub effects_last: bool,
    /// Its update is made for each of the player's fellows as well (a hoard's key opens the hoard
    /// to the whole fellowship).
    pub fellows: bool,
    /// For a give, exactly how many of the item it wants; any other count is refused and handed
    /// back. With none it runs for whatever is given, and takes all of it.
    pub wants: Option<u32>,
    /// The levels it is for, the least and the most (none: every level).
    pub levels: Option<(i32, i32)>,
    /// It pays this share of what the player's level takes to reach the next, whole.
    pub xp_share: f32,
}

/// What a person or an object does with an item given it that moves no quest (a toll, a donation,
/// an offering, a filled bucket): it keeps the item, says its lines, and gives what it gives.
#[derive(Debug, Clone, PartialEq)]
pub struct Give {
    /// The template of the person or the object given to.
    pub to: u32,
    /// The template of the item given (none: any item).
    pub item: Option<u32>,
    /// Exactly how many of the item it wants (none: whatever is given, all of it taken).
    pub wants: Option<u32>,
    pub gives: Vec<u32>,
    /// How many of each item given, in order (1 when left out).
    pub counts: Vec<u32>,
    /// The effects it puts on the player (their saved states), and a text shown after them.
    pub effects: Vec<u32>,
    pub text: Option<(u32, u32)>,
    /// What the person or the object plays as it takes the item.
    pub fx: Option<u32>,
    /// The quests it requires, at a phase or done.
    pub requires: Vec<(u32, Need)>,
    pub placed: String,
}

/// Every quest, step and give that moves no quest.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuestTable {
    pub quests: Vec<QuestDef>,
    pub steps: Vec<Step>,
    pub gives: Vec<Give>,
    /// Rows of like rooms: a step leading to one of a row's rooms leads to one of them.
    pub rooms: Vec<Vec<Room>>,
}

fn hex(s: &str) -> Result<u32, String> {
    u32::from_str_radix(s, 16).map_err(|e| format!("{s}: {e}"))
}

/// A column's value, if it holds one.
fn optional<T>(s: &str, read: impl Fn(&str) -> Result<T, String>) -> Result<Option<T>, String> {
    if s.is_empty() {
        Ok(None)
    } else {
        read(s).map(Some)
    }
}

/// What a row requires of other quests: `QUEST:N`, `QUEST:done` or `QUEST:free`, joined by `,`.
fn requirements(s: &str) -> Result<Vec<(u32, Need)>, String> {
    s.split(',')
        .filter(|x| !x.is_empty())
        .map(|x| {
            let (q, n) = x
                .split_once(':')
                .ok_or_else(|| format!("a requirement is QUEST:N, not {x}"))?;
            let need = match n {
                "done" => Need::Done,
                "free" => Need::Free,
                _ => Need::Phase(num(n)?),
            };
            Ok((hex(q)?, need))
        })
        .collect()
}

fn subquests(s: &str) -> Result<Subquests, String> {
    let mut f = s.splitn(3, ':');
    let (Some(phase), Some(any), Some(list)) = (f.next(), f.next(), f.next()) else {
        return Err(format!(
            "a phase's subquests are PHASE:ANY:QUEST/QUEST, not {s}"
        ));
    };
    Ok(Subquests {
        phase: num(phase)?,
        quests: list
            .split('/')
            .filter(|x| !x.is_empty())
            .map(hex)
            .collect::<Result<_, _>>()?,
        any: any == "1",
    })
}

fn pair(s: &str) -> Result<(u32, u32), String> {
    let (a, b) = s
        .split_once(':')
        .ok_or_else(|| format!("a string reference is string:table, not {s}"))?;
    Ok((hex(a)?, hex(b)?))
}

fn num<T: std::str::FromStr>(s: &str) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    s.parse::<T>().map_err(|e| format!("{s}: {e}"))
}

impl QuestTable {
    /// A quest's definition.
    #[must_use]
    pub fn quest(&self, id: u32) -> Option<&QuestDef> {
        self.quests.iter().find(|q| q.id == id)
    }

    /// The subquests a phase of a quest is done by, if it is.
    #[must_use]
    pub fn subquests(&self, quest: u32, phase: u32) -> Option<&Subquests> {
        self.quest(quest)?
            .subquests
            .iter()
            .find(|p| p.phase == phase && !p.quests.is_empty())
    }

    /// The quests and phases a quest is a subquest of.
    pub fn parents(&self, sub: u32) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.quests.iter().flat_map(move |q| {
            q.subquests
                .iter()
                .filter(move |p| p.quests.contains(&sub))
                .map(move |p| (q.id, p.phase))
        })
    }

    /// The table as text.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::from("# kind\tfields...\n");
        let r = |(a, b): (u32, u32)| format!("{a:08X}:{b:08X}");
        for q in &self.quests {
            let j = q
                .journals
                .iter()
                .map(|x| r(*x))
                .collect::<Vec<_>>()
                .join(",");
            let effects = q
                .effects
                .iter()
                .map(|(k, e)| format!("{k:08X}:{e:08X}"))
                .collect::<Vec<_>>()
                .join(",");
            let subs = q
                .subquests
                .iter()
                .map(|p| {
                    let list = p
                        .quests
                        .iter()
                        .map(|x| format!("{x:08X}"))
                        .collect::<Vec<_>>()
                        .join("/");
                    format!("{}:{}:{list}", p.phase, u8::from(p.any))
                })
                .collect::<Vec<_>>()
                .join(",");
            let _ = writeln!(
                out,
                "quest\t{:08X}\t{}\t{}\t{:08X}\t{}\t{j}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{effects}\t{subs}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                q.id,
                r(q.name),
                r(q.description),
                q.icon,
                u8::from(q.play_fx),
                q.challenge,
                q.duration,
                q.retry,
                q.xp,
                u8::from(q.cancellable),
                q.placed,
                u8::from(q.hidden),
                q.min_level,
                u8::from(q.fails_on_death),
                u8::from(q.once),
                q.fellows.word(),
                u8::from(q.last_waits),
                u8::from(q.hero_only),
                q.required
                    .map_or_else(String::new, |(q, st)| format!("{q:08X}:{st:08X}")),
                q.pay.word(),
                q.timed_phase,
            );
        }
        for s in &self.steps {
            let trigger = match s.trigger {
                Trigger::Use(t) => format!("use:{t:08X}"),
                Trigger::UseOn(i, t) => format!("useon:{i:08X}:{t:08X}"),
                Trigger::Give(i, t) => format!("give:{i:08X}:{t:08X}"),
                Trigger::Arrive(t) => format!("arrive:{t:08X}"),
                Trigger::Take(t) => format!("take:{t:08X}"),
                Trigger::Kill(t) => format!("kill:{t:08X}"),
                Trigger::Done(q) => format!("done:{q:08X}"),
                Trigger::Given(q) => format!("given:{q:08X}"),
            };
            let update = match s.update {
                Update::Bestow => "bestow".to_string(),
                Update::Advance(n) => format!("advance:{n}"),
                Update::Complete => "complete".to_string(),
                Update::Fail => "fail".to_string(),
                Update::Nothing => "none".to_string(),
            };
            let gives = s
                .gives
                .iter()
                .enumerate()
                .map(|(i, g)| match s.counts.get(i).copied().unwrap_or(1) {
                    1 => format!("{g:08X}"),
                    n => format!("{g:08X}x{n}"),
                })
                .collect::<Vec<_>>()
                .join(",");
            let effects = s
                .effects
                .iter()
                .map(|g| format!("{g:08X}"))
                .collect::<Vec<_>>()
                .join(",");
            let text = s.text.map_or_else(String::new, r);
            let requires = s
                .requires
                .iter()
                .map(|(q, n)| match n {
                    Need::Phase(p) => format!("{q:08X}:{p}"),
                    Need::Done => format!("{q:08X}:done"),
                    Need::Free => format!("{q:08X}:free"),
                })
                .collect::<Vec<_>>()
                .join(",");
            let takes = s
                .takes
                .iter()
                .map(|g| format!("{g:08X}"))
                .collect::<Vec<_>>()
                .join(",");
            let to = s.to.map_or_else(String::new, |p| {
                format!(
                    "{:08X}:{}:{}:{}:{}",
                    p.cell, p.origin[0], p.origin[1], p.origin[2], p.heading
                )
            });
            let _ = writeln!(
                out,
                "step\t{:08X}\t{}\t{trigger}\t{}\t{update}\t{}\t{gives}\t{}\t{requires}\t{takes}\t{to}\t{effects}\t{text}\t{}\t{}\t{}\t{}",
                s.quest,
                s.needs,
                s.tell.as_ref().map(Words::to_text).unwrap_or_default(),
                s.xp,
                s.placed,
                u8::from(s.fellows),
                s.wants.map_or_else(String::new, |n| n.to_string()),
                s.levels
                    .map_or_else(String::new, |(a, b)| format!("{a}:{b}")),
                s.xp_share,
            );
        }
        for g in &self.gives {
            let gives = g
                .gives
                .iter()
                .enumerate()
                .map(|(i, x)| match g.counts.get(i).copied().unwrap_or(1) {
                    1 => format!("{x:08X}"),
                    n => format!("{x:08X}x{n}"),
                })
                .collect::<Vec<_>>()
                .join(",");
            let effects = g
                .effects
                .iter()
                .map(|e| format!("{e:08X}"))
                .collect::<Vec<_>>()
                .join(",");
            let requires = g
                .requires
                .iter()
                .map(|(q, n)| match n {
                    Need::Phase(p) => format!("{q:08X}:{p}"),
                    Need::Done => format!("{q:08X}:done"),
                    Need::Free => format!("{q:08X}:free"),
                })
                .collect::<Vec<_>>()
                .join(",");
            let _ = writeln!(
                out,
                "give\t{:08X}\t{}\t{}\t{gives}\t{effects}\t{}\t{}\t{requires}\t{}",
                g.to,
                g.item.map_or_else(String::new, |i| format!("{i:08X}")),
                g.wants.map_or_else(String::new, |n| n.to_string()),
                g.text.map_or_else(String::new, r),
                g.fx.map_or_else(String::new, |f| format!("{f:08X}")),
                g.placed,
            );
        }
        for row in &self.rooms {
            let rooms = row
                .iter()
                .map(|r| {
                    let p = r.place;
                    format!(
                        "{:08X}:{}:{}:{}:{}:{:08X}",
                        p.cell, p.origin[0], p.origin[1], p.origin[2], p.heading, r.holds
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let _ = writeln!(out, "rooms\t{rooms}");
        }
        out
    }

    /// Read a table's text.
    ///
    /// # Errors
    /// A row without its columns, or a value that does not read.
    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut t = Self::default();
        for (n, line) in text.lines().enumerate() {
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let at = |e: String| format!("line {}: {e}", n + 1);
            let c: Vec<&str> = line.split('\t').collect();
            match c[0] {
                "rooms" if c.len() == 2 => {
                    let row = c[1]
                        .split(',')
                        .map(|r| {
                            let f: Vec<&str> = r.split(':').collect();
                            if f.len() != 6 {
                                return Err(format!("a room is CELL:X:Y:Z:HEADING:HOLDS, not {r}"));
                            }
                            Ok(Room {
                                place: Place {
                                    cell: hex(f[0])?,
                                    origin: [num(f[1])?, num(f[2])?, num(f[3])?],
                                    heading: num(f[4])?,
                                },
                                holds: hex(f[5])?,
                            })
                        })
                        .collect::<Result<_, _>>()
                        .map_err(at)?;
                    t.rooms.push(row);
                }
                "quest" if (13..=25).contains(&c.len()) => t.quests.push(QuestDef {
                    id: hex(c[1]).map_err(at)?,
                    name: pair(c[2]).map_err(at)?,
                    description: pair(c[3]).map_err(at)?,
                    icon: hex(c[4]).map_err(at)?,
                    play_fx: c[5] == "1",
                    journals: c[6]
                        .split(',')
                        .filter(|x| !x.is_empty())
                        .map(pair)
                        .collect::<Result<_, _>>()
                        .map_err(at)?,
                    challenge: num(c[7]).map_err(at)?,
                    duration: num(c[8]).map_err(at)?,
                    retry: num(c[9]).map_err(at)?,
                    xp: num(c[10]).map_err(at)?,
                    cancellable: c[11] == "1",
                    placed: c[12].to_string(),
                    hidden: c.get(13) == Some(&"1"),
                    effects: c
                        .get(14)
                        .copied()
                        .unwrap_or("")
                        .split(',')
                        .filter(|x| !x.is_empty())
                        .map(pair)
                        .collect::<Result<_, _>>()
                        .map_err(at)?,
                    subquests: c
                        .get(15)
                        .copied()
                        .unwrap_or("")
                        .split(',')
                        .filter(|x| !x.is_empty())
                        .map(subquests)
                        .collect::<Result<_, _>>()
                        .map_err(at)?,
                    min_level: match c.get(16) {
                        Some(v) if !v.is_empty() => num(v).map_err(at)?,
                        _ => 0,
                    },
                    fails_on_death: c.get(17) == Some(&"1"),
                    once: c.get(18) == Some(&"1"),
                    fellows: match c.get(19) {
                        Some(v) if !v.is_empty() => Fellows::of_word(num(v).map_err(at)?),
                        _ => Fellows::default(),
                    },
                    last_waits: c.get(20) == Some(&"1"),
                    hero_only: c.get(21) == Some(&"1"),
                    required: match c.get(22) {
                        Some(v) if !v.is_empty() => Some(pair(v).map_err(at)?),
                        _ => None,
                    },
                    pay: match c.get(23) {
                        Some(v) if !v.is_empty() => CompletionPay::of_word(v).map_err(at)?,
                        _ => CompletionPay::default(),
                    },
                    timed_phase: match c.get(24) {
                        Some(v) if !v.is_empty() => num(v).map_err(at)?,
                        _ => 0,
                    },
                }),
                "step" if (9..=18).contains(&c.len()) => {
                    let (kind, tpl) = c[3]
                        .split_once(':')
                        .ok_or_else(|| at(format!("a trigger is kind:template, not {}", c[3])))?;
                    let trigger = match kind {
                        "useon" | "give" => {
                            let (i, t) = tpl.split_once(':').ok_or_else(|| {
                                at(format!("{kind} takes ITEM:TARGET, not {tpl}"))
                            })?;
                            let (i, t) = (hex(i).map_err(at)?, hex(t).map_err(at)?);
                            if kind == "give" {
                                Trigger::Give(i, t)
                            } else {
                                Trigger::UseOn(i, t)
                            }
                        }
                        "use" => Trigger::Use(hex(tpl).map_err(at)?),
                        "arrive" => Trigger::Arrive(hex(tpl).map_err(at)?),
                        "take" => Trigger::Take(hex(tpl).map_err(at)?),
                        "kill" => Trigger::Kill(hex(tpl).map_err(at)?),
                        "done" => Trigger::Done(hex(tpl).map_err(at)?),
                        "given" => Trigger::Given(hex(tpl).map_err(at)?),
                        other => return Err(at(format!("a trigger of kind {other}"))),
                    };
                    let col = |i: usize| c.get(i).copied().unwrap_or("");
                    let requires = requirements(col(9)).map_err(at)?;
                    let takes = col(10)
                        .split(',')
                        .filter(|x| !x.is_empty())
                        .map(hex)
                        .collect::<Result<_, _>>()
                        .map_err(at)?;
                    let to = if col(11).is_empty() {
                        None
                    } else {
                        let f: Vec<&str> = col(11).split(':').collect();
                        if f.len() != 5 {
                            return Err(at(format!(
                                "a place is CELL:X:Y:Z:HEADING, not {}",
                                col(11)
                            )));
                        }
                        Some(Place {
                            cell: hex(f[0]).map_err(at)?,
                            origin: [
                                num(f[1]).map_err(at)?,
                                num(f[2]).map_err(at)?,
                                num(f[3]).map_err(at)?,
                            ],
                            heading: num(f[4]).map_err(at)?,
                        })
                    };
                    let gifts: Vec<(u32, u32)> = c[7]
                        .split(',')
                        .filter(|x| !x.is_empty())
                        .map(|x| match x.split_once('x') {
                            Some((g, n)) => Ok((hex(g)?, num(n)?)),
                            None => Ok((hex(x)?, 1)),
                        })
                        .collect::<Result<_, String>>()
                        .map_err(at)?;
                    let update = match c[5].split_once(':') {
                        Some(("advance", k)) => Update::Advance(num(k).map_err(at)?),
                        None if c[5] == "bestow" => Update::Bestow,
                        None if c[5] == "complete" => Update::Complete,
                        None if c[5] == "fail" => Update::Fail,
                        None if c[5] == "none" => Update::Nothing,
                        _ => return Err(at(format!("an update {}", c[5]))),
                    };
                    t.steps.push(Step {
                        quest: hex(c[1]).map_err(at)?,
                        needs: num(c[2]).map_err(at)?,
                        trigger,
                        tell: if c[4].is_empty() {
                            None
                        } else {
                            Some(Words::from_text(c[4]).map_err(at)?)
                        },
                        update,
                        xp: num(c[6]).map_err(at)?,
                        gives: gifts.iter().map(|(g, _)| *g).collect(),
                        counts: if gifts.iter().all(|(_, n)| *n == 1) {
                            Vec::new()
                        } else {
                            gifts.iter().map(|(_, n)| *n).collect()
                        },
                        effects: col(12)
                            .split(',')
                            .filter(|x| !x.is_empty())
                            .map(hex)
                            .collect::<Result<_, _>>()
                            .map_err(at)?,
                        text: if col(13).is_empty() {
                            None
                        } else {
                            Some(pair(col(13)).map_err(at)?)
                        },
                        placed: c[8].to_string(),
                        requires,
                        takes,
                        to,
                        races: Vec::new(),
                        effects_last: false,
                        fellows: col(14) == "1",
                        wants: if col(15).is_empty() {
                            None
                        } else {
                            Some(num(col(15)).map_err(at)?)
                        },
                        levels: match col(16).split_once(':') {
                            Some((a, b)) => Some((
                                a.parse().map_err(|_| at(format!("a level {a}")))?,
                                b.parse().map_err(|_| at(format!("a level {b}")))?,
                            )),
                            None => None,
                        },
                        xp_share: if col(17).is_empty() {
                            0.0
                        } else {
                            col(17)
                                .parse()
                                .map_err(|_| at(format!("a share {}", col(17))))?
                        },
                    });
                }
                "give" if c.len() == 10 => {
                    let gifts: Vec<(u32, u32)> = c[4]
                        .split(',')
                        .filter(|x| !x.is_empty())
                        .map(|x| match x.split_once('x') {
                            Some((g, n)) => Ok((hex(g)?, num(n)?)),
                            None => Ok((hex(x)?, 1)),
                        })
                        .collect::<Result<_, String>>()
                        .map_err(at)?;
                    t.gives.push(Give {
                        to: hex(c[1]).map_err(at)?,
                        item: optional(c[2], hex).map_err(at)?,
                        wants: optional(c[3], num).map_err(at)?,
                        gives: gifts.iter().map(|(g, _)| *g).collect(),
                        counts: if gifts.iter().all(|(_, n)| *n == 1) {
                            Vec::new()
                        } else {
                            gifts.iter().map(|(_, n)| *n).collect()
                        },
                        effects: c[5]
                            .split(',')
                            .filter(|x| !x.is_empty())
                            .map(hex)
                            .collect::<Result<_, _>>()
                            .map_err(at)?,
                        text: optional(c[6], pair).map_err(at)?,
                        fx: optional(c[7], hex).map_err(at)?,
                        requires: requirements(c[8]).map_err(at)?,
                        placed: c[9].to_string(),
                    });
                }
                other => {
                    return Err(at(format!(
                        "a row of kind {other} with {} columns",
                        c.len()
                    )))
                }
            }
        }
        Ok(t)
    }
}
