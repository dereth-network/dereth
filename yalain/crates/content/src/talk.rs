//! What people say, and when: each line a person (or a creature) says, the moment it is said at
//! (used, given an item, fighting, dying, a player near, unprompted), how it reaches the player (a
//! tell, a saying everyone near hears, or an emote naming the speaker), and the quest update it
//! comes with. Every row says where it came from.
//!
//! A line is [`Words`]: a string reference into the install's text, or literal text a world
//! wrote when it has no string table of its own; the client shows a literal unchanged.
//!
//! The text form is tab-separated, one line per row: the template, the placed person it belongs
//! to (its place in the content pack, or empty for every person of the template), the words, the
//! role, how, when, the quest, the update, the phase, the item given, the order within a speech,
//! whether it was measured (1) or derived (0), the provenance. Empty columns are left out values.

use std::fmt::Write as _;

/// What is said: a string reference (string id, table), or literal text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Words {
    Ref(u32, u32),
    Literal(String),
}

impl Words {
    /// The words as text: `SSSSSSSS:TTTTTTTT` for a reference, `=` and the text for a literal
    /// (a tab, a line break and a backslash written `\t`, `\n`, `\\`).
    #[must_use]
    pub fn to_text(&self) -> String {
        match self {
            Self::Ref(s, t) => format!("{s:08X}:{t:08X}"),
            Self::Literal(l) => {
                let mut out = String::from("=");
                for c in l.chars() {
                    match c {
                        '\\' => out.push_str("\\\\"),
                        '\t' => out.push_str("\\t"),
                        '\n' => out.push_str("\\n"),
                        c => out.push(c),
                    }
                }
                out
            }
        }
    }

    /// Read the words' text.
    ///
    /// # Errors
    /// A reference that is not `string:table` in hexadecimal.
    pub fn from_text(s: &str) -> Result<Self, String> {
        if let Some(l) = s.strip_prefix('=') {
            let mut out = String::new();
            let mut it = l.chars();
            while let Some(c) = it.next() {
                if c == '\\' {
                    match it.next() {
                        Some('t') => out.push('\t'),
                        Some('n') => out.push('\n'),
                        Some(o) => out.push(o),
                        None => out.push('\\'),
                    }
                } else {
                    out.push(c);
                }
            }
            return Ok(Self::Literal(out));
        }
        let (a, b) = s
            .split_once(':')
            .ok_or_else(|| format!("words are string:table or =text, not {s}"))?;
        Ok(Self::Ref(hex(a)?, hex(b)?))
    }
}

/// What a line does, read from its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Offer,
    Remind,
    Advance,
    Accept,
    Complete,
    Fail,
    RefuseUse,
    RefuseGive,
    /// What a person with several quests says when it gave one too lately to give another.
    TooSoon,
    Use,
    Near,
    Idle,
    Combat,
    Death,
    /// What a person who sends players where their maps lead says as it does.
    Teleport,
    /// What such a person says of a map it does not send anyone with.
    RefuseMap,
}

/// How a line reaches the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    /// A tell to the player, naming the speaker.
    Tell,
    /// A saying everyone near the speaker hears.
    Say,
    /// An emote: text in the player's chat window that names the speaker itself.
    Emote,
    /// A line every player in the speaker's landblock and the eight around it reads, whether its
    /// client knows the speaker or not.
    Broadcast,
}

/// The moment a line is said at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// The player uses the speaker.
    Use,
    /// The player gives the speaker an item.
    Give,
    /// The speaker fights.
    Attack,
    /// The speaker dies.
    Death,
    /// A player comes near.
    Near,
    /// Unprompted.
    Idle,
}

/// The quest update a line comes with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TalkUpdate {
    Bestow,
    Advance,
    Complete,
    Fail,
}

const ROLES: [(Role, &str); 16] = [
    (Role::Offer, "offer"),
    (Role::Remind, "remind"),
    (Role::Advance, "advance"),
    (Role::Accept, "accept"),
    (Role::Complete, "complete"),
    (Role::Fail, "fail"),
    (Role::RefuseUse, "refuse-use"),
    (Role::RefuseGive, "refuse-give"),
    (Role::TooSoon, "too-soon"),
    (Role::Use, "use"),
    (Role::Near, "near"),
    (Role::Idle, "idle"),
    (Role::Combat, "combat"),
    (Role::Death, "death"),
    (Role::Teleport, "teleport"),
    (Role::RefuseMap, "refuse-map"),
];
const HOWS: [(How, &str); 4] = [
    (How::Tell, "tell"),
    (How::Say, "say"),
    (How::Emote, "emote"),
    (How::Broadcast, "broadcast"),
];
const WHENS: [(When, &str); 6] = [
    (When::Use, "use"),
    (When::Give, "give"),
    (When::Attack, "attack"),
    (When::Death, "death"),
    (When::Near, "near"),
    (When::Idle, "idle"),
];
const UPDATES: [(TalkUpdate, &str); 4] = [
    (TalkUpdate::Bestow, "bestow"),
    (TalkUpdate::Advance, "advance"),
    (TalkUpdate::Complete, "complete"),
    (TalkUpdate::Fail, "fail"),
];

fn word_of<T: PartialEq + Copy>(table: &[(T, &'static str)], v: T) -> &'static str {
    table.iter().find(|(x, _)| *x == v).map_or("", |(_, w)| w)
}

fn of_word<T: Copy>(table: &[(T, &'static str)], w: &str) -> Option<T> {
    table.iter().find(|(_, x)| *x == w).map(|(v, _)| *v)
}

macro_rules! words {
    ($t:ty, $table:ident) => {
        impl $t {
            /// How a row writes it.
            #[must_use]
            pub fn word(self) -> &'static str {
                word_of(&$table, self)
            }

            /// The value a row's word names.
            #[must_use]
            pub fn from_word(w: &str) -> Option<Self> {
                of_word(&$table, w)
            }
        }
    };
}
words!(Role, ROLES);
words!(How, HOWS);
words!(When, WHENS);
words!(TalkUpdate, UPDATES);

/// One line a person says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The template of the person (or creature) that says it.
    pub who: u32,
    /// Only the one placed person at this place in the content pack, when set.
    pub object: Option<usize>,
    pub words: Words,
    pub role: Option<Role>,
    pub how: How,
    pub when: When,
    pub quest: Option<u32>,
    pub update: Option<TalkUpdate>,
    /// An advance's phase.
    pub phase: Option<u32>,
    /// The item given, for a line said when one is.
    pub item: Option<u32>,
    /// Its place in a speech of several lines (0 first).
    pub order: u32,
    /// Whether the line was heard said at this moment, rather than derived from its name.
    pub measured: bool,
    pub placed: String,
}

/// Every line people say.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TalkTable {
    pub lines: Vec<Line>,
}

fn hex(s: &str) -> Result<u32, String> {
    u32::from_str_radix(s, 16).map_err(|e| format!("{s}: {e}"))
}

fn opt<T>(s: &str, f: impl Fn(&str) -> Result<T, String>) -> Result<Option<T>, String> {
    if s.is_empty() {
        Ok(None)
    } else {
        f(s).map(Some)
    }
}

impl TalkTable {
    /// The lines a template's people say (and, for `object`, those of that one person), in the
    /// table's order.
    pub fn of(&self, who: u32, object: Option<usize>) -> impl Iterator<Item = &Line> + '_ {
        self.lines
            .iter()
            .filter(move |l| l.who == who && (l.object.is_none() || l.object == object))
    }

    /// Whether a template says anything at a moment.
    #[must_use]
    pub fn says_at(&self, who: u32, when: When) -> bool {
        self.lines.iter().any(|l| l.who == who && l.when == when)
    }

    /// The table as text.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::from(
            "# who\tobject\twords\trole\thow\twhen\tquest\tupdate\tphase\titem\torder\tmeasured\tplaced\n",
        );
        let h = |v: Option<u32>| v.map(|x| format!("{x:08X}")).unwrap_or_default();
        for l in &self.lines {
            let _ = writeln!(
                out,
                "{:08X}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                l.who,
                l.object.map(|i| i.to_string()).unwrap_or_default(),
                l.words.to_text(),
                l.role.map_or("", Role::word),
                l.how.word(),
                l.when.word(),
                h(l.quest),
                l.update.map_or("", TalkUpdate::word),
                l.phase.map(|p| p.to_string()).unwrap_or_default(),
                h(l.item),
                l.order,
                u8::from(l.measured),
                l.placed.replace(['\t', '\n'], " ")
            );
        }
        out
    }

    /// Read a table's text.
    ///
    /// # Errors
    /// A row without its columns, or a value that does not read.
    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut t = Self::default();
        for (n, row) in text.lines().enumerate() {
            if row.trim().is_empty() || row.starts_with('#') {
                continue;
            }
            let at = |e: String| format!("line {}: {e}", n + 1);
            let c: Vec<&str> = row.split('\t').collect();
            if c.len() != 13 {
                return Err(at(format!("a line has 13 columns, not {}", c.len())));
            }
            let word = |s: &str, what: &str| format!("{what} {s:?}");
            t.lines.push(Line {
                who: hex(c[0]).map_err(at)?,
                object: opt(c[1], |s| s.parse::<usize>().map_err(|e| e.to_string())).map_err(at)?,
                words: Words::from_text(c[2]).map_err(at)?,
                role: opt(c[3], |s| {
                    Role::from_word(s).ok_or_else(|| word(s, "a role"))
                })
                .map_err(at)?,
                how: How::from_word(c[4]).ok_or_else(|| at(word(c[4], "how")))?,
                when: When::from_word(c[5]).ok_or_else(|| at(word(c[5], "when")))?,
                quest: opt(c[6], hex).map_err(at)?,
                update: opt(c[7], |s| {
                    TalkUpdate::from_word(s).ok_or_else(|| word(s, "an update"))
                })
                .map_err(at)?,
                phase: opt(c[8], |s| s.parse::<u32>().map_err(|e| e.to_string())).map_err(at)?,
                item: opt(c[9], hex).map_err(at)?,
                order: c[10].parse().map_err(|e| at(format!("{e}")))?,
                measured: c[11] == "1",
                placed: c[12].to_string(),
            });
        }
        Ok(t)
    }
}
