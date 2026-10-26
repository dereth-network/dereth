//! Loot profiles: the `.utl` rule files players already have, read and matched natively.
//!
//! The file format and the matching are ported from VTClassic, the MIT-licensed loot plugin for
//! the retail client (Copyright (c) 2009-2010 VirindiPlugins; its notice is in
//! `docs/plugins/THIRD-PARTY.md`).
//!
//! A profile is a list of rules, tried in file order; the first rule all of whose requirements an
//! item meets decides the item's action (keep, salvage, sell, keep up to a count, …). An item no
//! rule matches is not looted. Some requirements read what only an appraisal tells (an item's
//! spells, its workmanship); [`Profile::needs_appraisal`] answers whether an item must be
//! appraised before the profile can decide, so that items it can decide from what every client
//! sees are never appraised.
//!
//! What this client cannot read is never matched: the colour and palette requirements (the client
//! keeps no palettes for items in containers) and the requirements computed from the character's
//! buffs. A rule holding one of them never matches.

use regex::Regex;

use crate::reads;
use dereth_client_model::World;
use dereth_primitives::ObjectId;

/// What a matching rule says to do with the item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LootAction {
    NoLoot,
    Keep,
    Salvage,
    Sell,
    Read,
    /// One of the five actions a profile leaves to the player.
    User(u8),
    /// Keep until the character holds this many.
    KeepUpTo(i32),
}

impl LootAction {
    fn from_code(code: i32, data: i32) -> Self {
        match code {
            1 => Self::Keep,
            2 => Self::Salvage,
            3 => Self::Sell,
            4 => Self::Read,
            5..=9 => Self::User(u8::try_from(code - 4).unwrap_or(1)),
            10 => Self::KeepUpTo(data),
            _ => Self::NoLoot,
        }
    }

    /// The action's name for plugins.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::NoLoot => "none".into(),
            Self::Keep => "keep".into(),
            Self::Salvage => "salvage".into(),
            Self::Sell => "sell".into(),
            Self::Read => "read".into(),
            Self::User(n) => format!("user{n}"),
            Self::KeepUpTo(_) => "keep_up_to".into(),
        }
    }
}

/// One requirement of a rule.
#[derive(Debug, Clone)]
enum Req {
    SpellName(Regex),
    StringValue(Regex, u32),
    IntLe(i64, u32),
    IntGe(i64, u32),
    FloatLe(f64, u32),
    FloatGe(f64, u32),
    /// Never matches (the retail plugin never implemented it).
    DamagePercent,
    Class(u32),
    SpellCount(i64),
    SpellMatch(Regex, Option<Regex>, i64),
    MinDamage(f64),
    IntFlag(i64, u32),
    IntEq(i64, u32),
    IntNe(i64, u32),
    SkillGe(i64, u32),
    PackSlotsGe(i64),
    LevelGe(i64),
    LevelLe(i64),
    BaseSkill(u32, i64, i64),
    /// A rule switched off in the editor.
    Disabled(bool),
    /// A requirement this client cannot evaluate: never matches.
    Unsupported,
}

/// What the matcher needs to know about the character.
#[derive(Debug, Clone, Default)]
pub struct Character {
    pub level: i64,
    /// Each skill's current (buffed) and base level, by skill id.
    pub skills: std::collections::BTreeMap<u32, (i64, i64)>,
    /// Free item slots in the main pack.
    pub main_pack_free: i64,
    /// The names of the spells the matcher may be asked about, by spell id (the host fills in the
    /// spells of the item being matched).
    pub spell_names: std::collections::BTreeMap<u32, String>,
}

/// One rule.
#[derive(Debug, Clone)]
pub struct Rule {
    pub name: String,
    pub action: LootAction,
    reqs: Vec<Req>,
}

impl Rule {
    /// The rule's requirements in plain words, in the order the profile lists them: what an
    /// item (or the character) must be for the rule to decide it.
    #[must_use]
    pub fn conditions(&self) -> Vec<String> {
        self.reqs.iter().filter_map(Req::words).collect()
    }
}

/// A property of an item a loot profile names, by its kind and key, in words.
fn property(kind: &str, key: u32) -> String {
    let named = match (kind, key) {
        ("text", 1) => Some("its name"),
        ("number", 5) => Some("its burden"),
        ("number", 19) => Some("its value"),
        ("number", 25) => Some("its level"),
        ("number", 28) => Some("its armor level"),
        ("number", 44) => Some("its damage"),
        ("number", 105) => Some("its workmanship"),
        ("number", 106) => Some("its spellcraft"),
        ("number", 107) => Some("its mana"),
        ("number", 108) => Some("its most mana"),
        ("number", 109) => Some("its arcane lore difficulty"),
        ("number", 131) => Some("its material"),
        ("number", 158) => Some("its wield requirement"),
        ("number", 159) => Some("its wield skill"),
        ("number", 160) => Some("its wield difficulty"),
        ("fraction", 29) => Some("its melee defense bonus"),
        ("fraction", 62) => Some("its attack bonus"),
        ("fraction", 63) => Some("its damage bonus"),
        _ => None,
    };
    named.map_or_else(|| format!("its {kind} property {key}"), str::to_owned)
}

/// An object class's name in words: `MeleeWeapon` as "melee weapon".
fn class_words(n: u32) -> String {
    let Some(name) = crate::reads::class_name(n) else {
        return format!("class {n}");
    };
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 {
            out.push(' ');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

impl Req {
    /// The requirement in plain words; nothing for a rule's switch when it is on.
    fn words(&self) -> Option<String> {
        Some(match self {
            Self::SpellName(rx) => format!("a spell whose name matches \"{}\"", rx.as_str()),
            Self::StringValue(rx, k) => {
                format!("{} matches \"{}\"", property("text", *k), rx.as_str())
            }
            Self::IntLe(v, k) => format!("{} at most {v}", property("number", *k)),
            Self::IntGe(v, k) => format!("{} at least {v}", property("number", *k)),
            Self::IntFlag(v, k) => {
                format!("{} has any of the flags {v:#X}", property("number", *k))
            }
            Self::IntEq(v, k) => format!("{} is {v}", property("number", *k)),
            Self::IntNe(v, k) => format!("{} is not {v}", property("number", *k)),
            Self::FloatLe(v, k) => format!("{} at most {v}", property("fraction", *k)),
            Self::FloatGe(v, k) => format!("{} at least {v}", property("fraction", *k)),
            Self::DamagePercent => "a damage share (never matches)".to_owned(),
            Self::Class(c) => format!("its class is {}", class_words(*c)),
            Self::SpellCount(n) => format!("at least {n} spells"),
            Self::SpellMatch(does, not, n) => match not {
                Some(not) => format!(
                    "at least {n} spells matching \"{}\" and not \"{}\"",
                    does.as_str(),
                    not.as_str()
                ),
                None => format!("at least {n} spells matching \"{}\"", does.as_str()),
            },
            Self::MinDamage(v) => format!("its least damage at least {v}"),
            Self::SkillGe(v, k) => format!("the character's skill {k} at least {v}"),
            Self::PackSlotsGe(v) => format!("at least {v} free slots in the pack"),
            Self::LevelGe(v) => format!("the character's level at least {v}"),
            Self::LevelLe(v) => format!("the character's level at most {v}"),
            Self::BaseSkill(s, lo, hi) => {
                format!("the character's base skill {s} from {lo} to {hi}")
            }
            Self::Disabled(true) => "switched off in the profile (never matches)".to_owned(),
            Self::Disabled(false) => return None,
            Self::Unsupported => "a colour (not read by this client: never matches)".to_owned(),
        })
    }
}

/// A read profile.
#[derive(Debug, Clone)]
pub struct Profile {
    pub rules: Vec<Rule>,
    /// The profile's salvage combining, when it has a block of it.
    pub salvage_combine: Option<SalvageCombine>,
}

/// A whole-number workmanship range, both ends counted: `1-6` is 1 to 6, `10` is 10 alone.
pub type WorkRange = (i64, i64);

/// Which salvage bags of one material may be combined: those whose workmanship falls in one
/// range of the material's own list, else of the profile's default list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SalvageCombine {
    /// The ranges for a material the profile names no ranges of.
    pub default: Vec<WorkRange>,
    /// Each material's own ranges, by material number, in the profile's order.
    pub materials: Vec<(i64, Vec<WorkRange>)>,
}

/// A list of workmanship ranges as a profile writes it (`1-6,7,8,9,10`); a part that is not one
/// is left out.
fn work_ranges(text: &str) -> Vec<WorkRange> {
    text.split(',')
        .filter_map(|part| {
            let part = part.trim();
            let (a, b) = part.split_once('-').unwrap_or((part, part));
            let (a, b): (i64, i64) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
            Some((a.min(b), a.max(b)))
        })
        .collect()
}

/// Why a profile did not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadError(pub String);

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the loot profile does not read: {}", self.0)
    }
}

/// The lines of a profile, with byte offsets, for the length-coded requirement bodies.
struct Lines<'a> {
    text: &'a str,
    at: usize,
}

impl<'a> Lines<'a> {
    fn line(&mut self) -> Result<&'a str, ReadError> {
        if self.at >= self.text.len() {
            return Err(ReadError("it ends early".into()));
        }
        let rest = &self.text[self.at..];
        let (line, used) = match rest.find('\n') {
            Some(i) => (&rest[..i], i + 1),
            None => (rest, rest.len()),
        };
        self.at += used;
        Ok(line.strip_suffix('\r').unwrap_or(line))
    }

    fn int(&mut self) -> Result<i64, ReadError> {
        let l = self.line()?;
        l.trim()
            .parse()
            .map_err(|_| ReadError(format!("{l:?} is not a whole number")))
    }

    fn float(&mut self) -> Result<f64, ReadError> {
        // Profiles written with a decimal comma read the same.
        Ok(self.line()?.trim().replace(',', ".").parse().unwrap_or(0.0))
    }

    fn key(&mut self) -> Result<u32, ReadError> {
        let v = self.int()?;
        u32::try_from(v).map_err(|_| ReadError(format!("{v} is not a property key")))
    }

    fn regex(&mut self) -> Result<Regex, ReadError> {
        Ok(pattern(self.line()?))
    }

    fn eof(&self) -> bool {
        self.at >= self.text.len()
    }
}

/// A profile's pattern, or, when it is not a valid pattern, the text itself matched literally.
fn pattern(s: &str) -> Regex {
    Regex::new(s).unwrap_or_else(|_| {
        Regex::new(&regex::escape(s)).unwrap_or_else(|e| unreachable!("an escaped pattern: {e}"))
    })
}

/// Whether [`read_req`] reads requirement type `ty`.
fn known(ty: i64) -> bool {
    matches!(ty, 0..=17 | 1000..=1004 | 9999)
}

/// The colour and palette requirements, which name an item's colours.
const COLOURS: [i64; 4] = [14, 15, 16, 17];

fn read_req(ty: i64, l: &mut Lines<'_>) -> Result<Req, ReadError> {
    if COLOURS.contains(&ty) {
        // Their bodies are read and dropped: the client keeps no palettes for these items.
        let lines = match ty {
            14 => 5,
            15 => 6,
            16 => 6,
            _ => 2,
        };
        for _ in 0..lines {
            l.line()?;
        }
        return Ok(Req::Unsupported);
    }
    Ok(match ty {
        0 => Req::SpellName(l.regex()?),
        1 => {
            let rx = l.regex()?;
            Req::StringValue(rx, l.key()?)
        }
        2 | 3 | 11 | 12 | 13 => {
            let v = l.int()?;
            let k = l.key()?;
            match ty {
                2 => Req::IntLe(v, k),
                3 => Req::IntGe(v, k),
                11 => Req::IntFlag(v, k),
                12 => Req::IntEq(v, k),
                _ => Req::IntNe(v, k),
            }
        }
        4 | 5 => {
            let v = l.float()?;
            let k = l.key()?;
            if ty == 4 {
                Req::FloatLe(v, k)
            } else {
                Req::FloatGe(v, k)
            }
        }
        6 => {
            l.float()?;
            Req::DamagePercent
        }
        7 => Req::Class(l.key()?),
        8 => Req::SpellCount(l.int()?),
        9 => {
            let does = l.regex()?;
            let not = l.line()?;
            let not = (!not.trim().is_empty()).then(|| pattern(not));
            Req::SpellMatch(does, not, l.int()?)
        }
        10 => Req::MinDamage(l.float()?),
        1000 => {
            let v = l.int()?;
            Req::SkillGe(v, l.key()?)
        }
        1001 => Req::PackSlotsGe(l.int()?),
        1002 => Req::LevelGe(l.int()?),
        1003 => Req::LevelLe(l.int()?),
        1004 => {
            let skill = l.key()?;
            let min = l.int()?;
            Req::BaseSkill(skill, min, l.int()?)
        }
        9999 => Req::Disabled(l.line()? == "true"),
        other => return Err(ReadError(format!("requirement type {other} has no reader"))),
    })
}

impl Profile {
    /// Read a profile's text.
    ///
    /// # Errors
    /// The text is not a profile.
    pub fn read(text: &str) -> Result<Self, ReadError> {
        let mut l = Lines { text, at: 0 };
        Self::read_lines(&mut l).map_err(|e| {
            // Where it stopped: the line after the last one read.
            let line = text[..l.at.min(text.len())].matches('\n').count() + 1;
            ReadError(format!("{} (line {line})", e.0))
        })
    }

    fn read_lines(l: &mut Lines<'_>) -> Result<Self, ReadError> {
        let first = l.line()?;
        let (version, count) = if first == "UTL" {
            let v = l.int()?;
            if v > 1 {
                return Err(ReadError(format!("file version {v} is newer than 1")));
            }
            (v, l.int()?)
        } else {
            (
                0,
                first
                    .trim()
                    .parse::<i64>()
                    .map_err(|_| ReadError("it starts with neither UTL nor a rule count".into()))?,
            )
        };
        let mut rules = Vec::new();
        for _ in 0..count {
            let name = l.line()?.to_owned();
            if version >= 1 {
                // The rule's custom expression, which matching does not use.
                l.line()?;
            }
            let big = l.line()?;
            let parts: Vec<i64> = big
                .split(';')
                .map(|p| {
                    p.trim()
                        .parse()
                        .map_err(|_| ReadError(format!("rule {name:?}: {big:?}")))
                })
                .collect::<Result<_, _>>()?;
            if parts.len() < 2 {
                return Err(ReadError(format!("rule {name:?} has no action")));
            }
            let code = i32::try_from(parts[1]).unwrap_or(0);
            let data = if code == 10 {
                i32::try_from(l.int()?).unwrap_or(i32::MAX)
            } else {
                0
            };
            let mut reqs = Vec::new();
            for &ty in &parts[2..] {
                if version >= 1 {
                    // Each requirement's body is preceded by its length. A requirement this
                    // reader knows is read line by line, as its own lines say (so a profile whose
                    // line ends were changed still reads); one it does not know is skipped by
                    // its length and never matches.
                    let len = usize::try_from(l.int()?)
                        .map_err(|_| ReadError(format!("rule {name:?}: a negative length")))?;
                    if known(ty) {
                        reqs.push(read_req(ty, l)?);
                    } else {
                        // The length counts each line as written, ending in CR LF; a profile
                        // whose line ends were changed to LF alone is skipped by the same lines.
                        let mut skipped = 0;
                        while skipped < len && !l.eof() {
                            skipped += l.line()?.len() + 2;
                        }
                        reqs.push(Req::Unsupported);
                    }
                } else {
                    reqs.push(read_req(ty, l)?);
                }
            }
            rules.push(Rule {
                name,
                action: LootAction::from_code(code, data),
                reqs,
            });
        }
        // Extra blocks follow, each its name, its length and its body: salvage combining is read;
        // any other is skipped by its length. A block that does not read ends the reading, the
        // rules kept.
        let mut salvage_combine = None;
        while !l.eof() {
            let name = l.line()?.trim().to_owned();
            if name.is_empty() {
                continue;
            }
            let Ok(len) = l.int().and_then(|n| {
                usize::try_from(n).map_err(|_| ReadError("a negative length".into()))
            }) else {
                break;
            };
            if name == "SalvageCombine" {
                match Self::read_salvage_combine(l) {
                    Ok(s) => salvage_combine = Some(s),
                    Err(_) => break,
                }
            } else {
                let mut skipped = 0;
                while skipped < len && !l.eof() {
                    let Ok(line) = l.line() else { break };
                    skipped += line.len() + 2;
                }
            }
        }
        Ok(Self {
            rules,
            salvage_combine,
        })
    }

    /// A salvage combining block's body: its version, the default ranges, then each material's
    /// number and its ranges.
    fn read_salvage_combine(l: &mut Lines<'_>) -> Result<SalvageCombine, ReadError> {
        let _version = l.int()?;
        let default = work_ranges(l.line()?);
        let count = l.int()?;
        let mut materials = Vec::new();
        for _ in 0..count.max(0) {
            let material = l.int()?;
            materials.push((material, work_ranges(l.line()?)));
        }
        Ok(SalvageCombine { default, materials })
    }

    /// The first rule `item` meets, or `None`: the item is not looted.
    #[must_use]
    pub fn classify(&self, world: &World, ch: &Character, item: ObjectId) -> Option<&Rule> {
        self.rules
            .iter()
            .find(|r| r.reqs.iter().all(|q| matches(q, world, ch, item)))
    }

    /// Whether `item` must be appraised before the profile can decide it. An item already
    /// appraised never needs it.
    #[must_use]
    pub fn needs_appraisal(&self, world: &World, ch: &Character, item: ObjectId) -> bool {
        if world.appraisal.get(item).is_some() {
            return false;
        }
        let mut walking: Option<LootAction> = None;
        for r in &self.rules {
            if walking.is_some_and(|a| a != r.action) {
                return true;
            }
            match early(r, world, ch, item) {
                Some(true) => return false,
                Some(false) => {}
                None => walking = Some(r.action),
            }
        }
        walking.is_some()
    }
}

/// Whether `k` is read only from an appraisal.
fn int_needs_appraisal(k: u32) -> bool {
    const X: u32 = reads::INT_EXTRA;
    !matches!(k, 10 | 19 | 91 | 92 | 131)
        && !matches!(
            k.wrapping_sub(X),
            0 | 1
                | 2
                | 4
                | 5
                | 6
                | 7
                | 9
                | 12
                | 13
                | 14
                | 15
                | 16
                | 17
                | 18
                | 20
                | 24
                | 25
                | 26
                | 27
                | 38
                | 41
                | 42
        )
}

fn float_needs_appraisal(k: u32) -> bool {
    let n = k.wrapping_sub(reads::FLOAT_EXTRA);
    !(n == 8 || n == 9)
}

fn magical(world: &World, item: ObjectId) -> bool {
    reads::int(world, item, reads::INT_EXTRA + 16).unwrap_or(0) & 1 != 0
}

/// A rule's decision from what is known before an appraisal: `Some(true)` it matches, `Some(false)`
/// it cannot, `None` only an appraisal can tell.
fn early(r: &Rule, world: &World, ch: &Character, item: ObjectId) -> Option<bool> {
    let mut need = false;
    for q in &r.reqs {
        let d = match q {
            Req::SpellName(_) | Req::SpellCount(_) | Req::SpellMatch(..) => {
                if magical(world, item) {
                    None
                } else {
                    Some(false)
                }
            }
            Req::StringValue(_, k) if *k != 1 && *k != 184_549_376 => None,
            Req::IntLe(_, k)
            | Req::IntGe(_, k)
            | Req::IntFlag(_, k)
            | Req::IntEq(_, k)
            | Req::IntNe(_, k)
                if int_needs_appraisal(*k) =>
            {
                None
            }
            Req::FloatLe(_, k) | Req::FloatGe(_, k) if float_needs_appraisal(*k) => None,
            Req::MinDamage(_) => {
                if world
                    .weenie(item)
                    .is_some_and(|w| reads::class_of(w).0 == 1)
                {
                    None
                } else {
                    Some(false)
                }
            }
            Req::Disabled(_) | Req::DamagePercent | Req::Unsupported => Some(false),
            other => Some(matches(other, world, ch, item)),
        };
        match d {
            Some(false) => return Some(false),
            None => need = true,
            Some(true) => {}
        }
    }
    if need {
        None
    } else {
        Some(true)
    }
}

fn matches(q: &Req, world: &World, ch: &Character, item: ObjectId) -> bool {
    let int = |k: u32| reads::int(world, item, k).unwrap_or(0);
    // Floats compare as the retail plugin compared them: in single precision.
    #[allow(clippy::cast_possible_truncation)]
    let single = |v: f64| v as f32;
    let float = |k: u32| reads::float(world, item, k).unwrap_or(0.0);
    let spell_names = || -> Vec<String> {
        reads::spells(world, item)
            .into_iter()
            .map(|s| ch.spell_names.get(&s).cloned().unwrap_or_default())
            .collect()
    };
    match q {
        Req::SpellName(rx) => spell_names().iter().any(|n| rx.is_match(n)),
        Req::StringValue(rx, k) => rx.is_match(&reads::string(world, item, *k).unwrap_or_default()),
        Req::IntLe(v, k) => int(*k) <= *v,
        Req::IntGe(v, k) => int(*k) >= *v,
        Req::IntFlag(v, k) => int(*k) & *v > 0,
        Req::IntEq(v, k) => int(*k) == *v,
        Req::IntNe(v, k) => int(*k) != *v,
        Req::FloatLe(v, k) => single(float(*k)) <= single(*v),
        Req::FloatGe(v, k) => single(float(*k)) >= single(*v),
        Req::Class(c) => world
            .weenie(item)
            .is_some_and(|w| reads::class_of(w).0 == *c),
        Req::SpellCount(n) => i64::try_from(reads::spells(world, item).len()).unwrap_or(0) >= *n,
        Req::SpellMatch(does, not, n) => {
            let hits = spell_names()
                .iter()
                .filter(|s| does.is_match(s) && not.as_ref().is_none_or(|x| !x.is_match(s)))
                .count();
            i64::try_from(hits).unwrap_or(0) >= *n
        }
        Req::MinDamage(v) => {
            let max = int(reads::INT_EXTRA + 34);
            #[allow(clippy::cast_precision_loss)]
            let max = max as f64;
            max - float(reads::FLOAT_EXTRA + 11) * max >= *v
        }
        Req::SkillGe(v, s) => ch.skills.get(s).is_some_and(|(buffed, _)| buffed >= v),
        Req::PackSlotsGe(v) => ch.main_pack_free >= *v,
        Req::LevelGe(v) => ch.level >= *v,
        Req::LevelLe(v) => ch.level <= *v,
        Req::BaseSkill(s, lo, hi) => ch
            .skills
            .get(s)
            .is_some_and(|(_, base)| base >= lo && base <= hi),
        Req::Disabled(off) => !off,
        Req::DamagePercent | Req::Unsupported => false,
    }
}

#[cfg(test)]
mod tests;
