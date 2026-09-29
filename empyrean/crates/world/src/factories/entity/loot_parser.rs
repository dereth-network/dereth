// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Entity/LootParser.cs
//! Port of `Source/ACE.Server/Factories/Entity/LootParser.cs`.
//!
//! Parses the `ChanceTable<T>` literals out of a copy of ACE's `Factories/Tables/*.cs` source files
//! (the `/lootswap` admin command's input). The regexes are run by [`Regex`], a small backtracking
//! matcher for the constructs ACE's patterns use (`\d`, `\s`, `.`, `[..]`, `[^..]`, `+`, groups),
//! with .NET's leftmost-first semantics.

use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::SpellId;
use empyrean_tables::entity::ChanceTable;
use empyrean_tables::enums::{
    TreasureArmorType, TreasureHeritageGroup, TreasureItemType, TreasureTableType,
    TreasureWeaponType, WeenieClassName,
};

use crate::entity::mutations::mutation_cache::enum_try_parse_by;

/// The `object` a parsed table is stored as: one variant per `TreasureTableType`.
#[derive(Debug)]
pub enum ParsedTable {
    Int(ChanceTable<i32>),
    Spell(ChanceTable<SpellId>),
    Wcid(ChanceTable<WeenieClassName>),
    Bool(ChanceTable<bool>),
    Heritage(ChanceTable<TreasureHeritageGroup>),
    ItemType(ChanceTable<TreasureItemType>),
    ArmorType(ChanceTable<TreasureArmorType>),
    WeaponType(ChanceTable<TreasureWeaponType>),
}

/// Every non-readonly `ChanceTable<T> Name = new ChanceTable<T>()` literal in the file, by field
/// name (`null` for a table of an unknown type).
///
/// # Errors
/// The file cannot be read (`File.ReadAllLines` throws).
///
/// # Panics
/// Two tables share a name (`Dictionary.Add` throws `ArgumentException`).
// ACE: LootParser.ParseFile
pub fn parse_file(filename: &str) -> std::io::Result<DotNetDict<String, Option<ParsedTable>>> {
    let text = std::fs::read_to_string(filename)?;
    Ok(parse_lines(&read_all_lines(&text)))
}

/// The body of [`parse_file`] over the file's lines.
///
/// # Panics
/// As [`parse_file`].
#[must_use]
pub fn parse_lines(lines: &[String]) -> DotNetDict<String, Option<ParsedTable>> {
    let mut tables = DotNetDict::new();

    //Console.WriteLine($"Parsed {filename}");

    for (i, line) in lines.iter().enumerate() {
        if line.contains("List<") {
            continue;
        }

        if !line.contains("ChanceTable<") || !line.contains(" = new") || !line.contains("()") {
            continue;
        }

        if line.contains("readonly") || line.ends_with(';') {
            continue;
        }

        let (table_name, table) = parse_chance_table(lines, i);

        let table_name = table_name.expect("ArgumentNullException: a table without a name");
        assert!(
            !tables.contains_key(&table_name),
            "ArgumentException: An item with the same key has already been added. Key: {table_name}"
        );
        tables.insert(table_name, table);
    }
    tables
}

/// `File.ReadAllLines`: `\r\n`, `\n` and `\r` end lines; a final line ending adds no empty line.
#[must_use]
pub fn read_all_lines(text: &str) -> Vec<String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(i) = rest.find(['\r', '\n']) {
            lines.push(rest[..i].to_owned());
            let skip = if rest[i..].starts_with("\r\n") { 2 } else { 1 };
            rest = &rest[i + skip..];
        } else {
            lines.push(rest.to_owned());
            rest = "";
        }
    }
    lines
}

// ACE: LootParser.ParseChanceTable
fn parse_chance_table(
    lines: &[String],
    start_line: usize,
) -> (Option<String>, Option<ParsedTable>) {
    let line = &lines[start_line];

    let table_type = get_table_type(line);
    let table_name = get_table_name(line);

    //Console.WriteLine($" - {tableName}");

    let chance_table = match table_type {
        TreasureTableType::ChanceInt => {
            Some(ParsedTable::Int(parse_chance_table_int(lines, start_line)))
        }
        TreasureTableType::ChanceSpell => Some(ParsedTable::Spell(parse_chance_table_spell(
            lines, start_line,
        ))),
        TreasureTableType::ChanceWcid => Some(ParsedTable::Wcid(parse_chance_table_wcid(
            lines, start_line,
        ))),
        TreasureTableType::ChanceBool => Some(ParsedTable::Bool(parse_chance_table_bool(
            lines, start_line,
        ))),
        TreasureTableType::ChanceHeritage => Some(ParsedTable::Heritage(
            parse_chance_table_heritage(lines, start_line),
        )),
        TreasureTableType::ChanceItemType => Some(ParsedTable::ItemType(
            parse_chance_table_item_type(lines, start_line),
        )),
        TreasureTableType::ChanceArmorType => Some(ParsedTable::ArmorType(
            parse_chance_table_armor_type(lines, start_line),
        )),
        TreasureTableType::ChanceWeaponType => Some(ParsedTable::WeaponType(
            parse_chance_table_weapon_type(lines, start_line),
        )),
        _ => {
            empyrean_common::console_write_line!("Unknown table type {table_type}");
            None
        }
    };

    (table_name, chance_table)
}

/// The shared loop of every `ParseChanceTable_*`: the lines after the declaration up to the
/// first two-character line (`};`), skipping short lines and comments; `parse` returns the entry
/// or `None` ("Couldn't parse").
fn parse_entries<T: Clone + 'static>(
    lines: &[String],
    start_line: usize,
    parse: impl Fn(&str) -> Option<(T, f32)>,
) -> ChanceTable<T> {
    let mut chance_table = Vec::new();

    for line in &lines[start_line + 1..] {
        let line = line.trim();

        // string.Length counts UTF-16 units
        let len = line.encode_utf16().count();

        if len < 2 {
            continue;
        }

        if len == 2 {
            break;
        }

        if line.starts_with("//") {
            continue;
        }

        match parse(line) {
            Some(entry) => chance_table.push(entry),
            None => empyrean_common::console_write_line!("Couldn't parse {line}"),
        }
    }

    ChanceTable::from_vec(chance_table)
}

/// `float.TryParse` of a `[\d.]+` capture.
fn parse_chance(s: &str) -> Option<f32> {
    s.parse::<f32>().ok()
}

// ACE: LootParser.ParseChanceTable_Int
#[must_use]
pub fn parse_chance_table_int(lines: &[String], start_line: usize) -> ChanceTable<i32> {
    parse_entries(lines, start_line, |line| {
        let m = Regex::new(r"(\d+),\s+([\d.]+)").captures(line)?;
        Some((m[0].parse::<i32>().ok()?, parse_chance(m[1])?))
    })
}

// ACE: LootParser.ParseChanceTable_Spell
#[must_use]
pub fn parse_chance_table_spell(lines: &[String], start_line: usize) -> ChanceTable<SpellId> {
    parse_entries(lines, start_line, |line| {
        let m = Regex::new(r"SpellId.([^,]+),\s+([\d.]+)").captures(line)?;
        let spell = enum_try_parse_by(m[0], |n| SpellId::from_name(n).map(|e| i64::from(e.0)))?;
        Some((SpellId(u32::try_from(spell).ok()?), parse_chance(m[1])?))
    })
}

// ACE: LootParser.ParseChanceTable_Wcid
#[must_use]
pub fn parse_chance_table_wcid(
    lines: &[String],
    start_line: usize,
) -> ChanceTable<WeenieClassName> {
    parse_entries(lines, start_line, |line| {
        let m = Regex::new(r"WeenieClassName.([^,]+),\s+([\d.]+)").captures(line)?;
        let wcid = enum_try_parse_by(m[0], |n| {
            WeenieClassName::from_name(n).map(|e| i64::from(e.0))
        })?;
        Some((
            WeenieClassName(i32::try_from(wcid).ok()?),
            parse_chance(m[1])?,
        ))
    })
}

// ACE: LootParser.ParseChanceTable_Bool
#[must_use]
pub fn parse_chance_table_bool(lines: &[String], start_line: usize) -> ChanceTable<bool> {
    parse_entries(lines, start_line, |line| {
        let m = Regex::new(r"\(\s+([^,]+),\s+([\d.]+)").captures(line)?;
        Some((bool_try_parse(m[0])?, parse_chance(m[1])?))
    })
}

/// `bool.TryParse`: "True"/"False" in any case, surrounding white space (and NULs) ignored.
fn bool_try_parse(s: &str) -> Option<bool> {
    let s = s.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    if s.eq_ignore_ascii_case("true") {
        Some(true)
    } else if s.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
}

// ACE: LootParser.ParseChanceTable_Heritage
#[must_use]
pub fn parse_chance_table_heritage(
    lines: &[String],
    start_line: usize,
) -> ChanceTable<TreasureHeritageGroup> {
    parse_entries(lines, start_line, |line| {
        let m = Regex::new(r"TreasureHeritageGroup.([^,]+),\s+([\d.]+)").captures(line)?;
        let v = enum_try_parse_by(m[0], |n| {
            TreasureHeritageGroup::from_name(n).map(|e| i64::from(e.0))
        })?;
        Some((
            TreasureHeritageGroup(i32::try_from(v).ok()?),
            parse_chance(m[1])?,
        ))
    })
}

// ACE: LootParser.ParseChanceTable_ItemType
#[must_use]
pub fn parse_chance_table_item_type(
    lines: &[String],
    start_line: usize,
) -> ChanceTable<TreasureItemType> {
    parse_entries(lines, start_line, |line| {
        let m = Regex::new(r"TreasureItemType_Orig.([^,]+),\s+([\d.]+)").captures(line)?;
        let v = enum_try_parse_by(m[0], |n| {
            TreasureItemType::from_name(n).map(|e| i64::from(e.0))
        })?;
        Some((
            TreasureItemType(i32::try_from(v).ok()?),
            parse_chance(m[1])?,
        ))
    })
}

// ACE: LootParser.ParseChanceTable_ArmorType
#[must_use]
pub fn parse_chance_table_armor_type(
    lines: &[String],
    start_line: usize,
) -> ChanceTable<TreasureArmorType> {
    parse_entries(lines, start_line, |line| {
        let m = Regex::new(r"TreasureArmorType.([^,]+),\s+([\d.]+)").captures(line)?;
        let v = enum_try_parse_by(m[0], |n| {
            TreasureArmorType::from_name(n).map(|e| i64::from(e.0))
        })?;
        Some((
            TreasureArmorType(i32::try_from(v).ok()?),
            parse_chance(m[1])?,
        ))
    })
}

// ACE: LootParser.ParseChanceTable_WeaponType
#[must_use]
pub fn parse_chance_table_weapon_type(
    lines: &[String],
    start_line: usize,
) -> ChanceTable<TreasureWeaponType> {
    parse_entries(lines, start_line, |line| {
        let m = Regex::new(r"TreasureWeaponType.([^,]+),\s+([\d.]+)").captures(line)?;
        let v = enum_try_parse_by(m[0], |n| {
            TreasureWeaponType::from_name(n).map(|e| i64::from(e.0))
        })?;
        Some((
            TreasureWeaponType(i32::try_from(v).ok()?),
            parse_chance(m[1])?,
        ))
    })
}

/// The table's element type from the first `<...>` on the declaration line.
// ACE: LootParser.GetTableType
#[must_use]
pub fn get_table_type(line: &str) -> TreasureTableType {
    let Some(m) = Regex::new(r"<([^>]+)").captures(line) else {
        return TreasureTableType::Undef;
    };

    match m[0] {
        "int" => TreasureTableType::ChanceInt,
        "SpellId" => TreasureTableType::ChanceSpell,
        "WeenieClassName" => TreasureTableType::ChanceWcid,
        "bool" => TreasureTableType::ChanceBool,
        "TreasureHeritageGroup" => TreasureTableType::ChanceHeritage,
        "TreasureItemType_Orig" => TreasureTableType::ChanceItemType,
        "TreasureArmorType" => TreasureTableType::ChanceArmorType,
        "TreasureWeaponType" => TreasureTableType::ChanceWeaponType,
        _ => TreasureTableType::Undef,
    }
}

/// The field name: the word after the first `> `.
// ACE: LootParser.GetTableName
#[must_use]
pub fn get_table_name(line: &str) -> Option<String> {
    let m = Regex::new(r"> ([^ ]+)").captures(line)?;

    Some(m[0].to_owned())
}

// ---- a small backtracking regex (not ported) ----

#[derive(Clone, Debug)]
enum Atom {
    Char(char),
    Any,
    Class {
        negated: bool,
        digits: bool,
        spaces: bool,
        chars: Vec<char>,
    },
}

impl Atom {
    fn matches(&self, c: char) -> bool {
        match self {
            Atom::Char(x) => *x == c,
            // `.` matches anything but `\n`
            Atom::Any => c != '\n',
            Atom::Class {
                negated,
                digits,
                spaces,
                chars,
            } => {
                let hit = (*digits && c.is_ascii_digit())
                    || (*spaces && c.is_whitespace())
                    || chars.contains(&c);
                hit != *negated
            }
        }
    }
}

#[derive(Clone, Debug)]
enum Node {
    Atom { atom: Atom, plus: bool },
    Open(usize),
    Close(usize),
}

/// A compiled pattern: atoms (optionally `+`), and capture groups.
#[derive(Debug)]
pub struct Regex {
    nodes: Vec<Node>,
    groups: usize,
}

impl Regex {
    /// Compiles the pattern subset ACE's loot parser uses.
    ///
    /// # Panics
    /// On a construct outside that subset.
    #[must_use]
    pub fn new(pattern: &str) -> Self {
        let chars: Vec<char> = pattern.chars().collect();
        let mut nodes = Vec::new();
        let mut groups = 0;
        let mut open = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let atom = match chars[i] {
                '(' => {
                    open.push(groups);
                    nodes.push(Node::Open(groups));
                    groups += 1;
                    i += 1;
                    continue;
                }
                ')' => {
                    nodes.push(Node::Close(open.pop().expect("balanced groups")));
                    i += 1;
                    continue;
                }
                '.' => Atom::Any,
                '\\' => {
                    i += 1;
                    match chars[i] {
                        'd' => Atom::Class {
                            negated: false,
                            digits: true,
                            spaces: false,
                            chars: Vec::new(),
                        },
                        's' => Atom::Class {
                            negated: false,
                            digits: false,
                            spaces: true,
                            chars: Vec::new(),
                        },
                        c => Atom::Char(c),
                    }
                }
                '[' => {
                    i += 1;
                    let negated = chars[i] == '^';
                    if negated {
                        i += 1;
                    }
                    let (mut digits, mut spaces, mut set) = (false, false, Vec::new());
                    while chars[i] != ']' {
                        if chars[i] == '\\' {
                            i += 1;
                            match chars[i] {
                                'd' => digits = true,
                                's' => spaces = true,
                                c => set.push(c),
                            }
                        } else {
                            set.push(chars[i]);
                        }
                        i += 1;
                    }
                    Atom::Class {
                        negated,
                        digits,
                        spaces,
                        chars: set,
                    }
                }
                c => Atom::Char(c),
            };
            i += 1;
            let plus = chars.get(i) == Some(&'+');
            if plus {
                i += 1;
            }
            nodes.push(Node::Atom { atom, plus });
        }
        Self { nodes, groups }
    }

    /// `Regex.Match(input)`: the captures of the leftmost match, or `None` when it fails.
    #[must_use]
    pub fn captures<'a>(&self, input: &'a str) -> Option<Vec<&'a str>> {
        let chars: Vec<(usize, char)> = input.char_indices().collect();
        for start in 0..=chars.len() {
            let mut caps = vec![(0usize, 0usize); self.groups];
            if self.match_at(&chars, 0, start, &mut caps) {
                let byte = |k: usize| chars.get(k).map_or(input.len(), |&(b, _)| b);
                return Some(
                    caps.iter()
                        .map(|&(s, e)| &input[byte(s)..byte(e)])
                        .collect(),
                );
            }
        }
        None
    }

    fn match_at(
        &self,
        chars: &[(usize, char)],
        node: usize,
        pos: usize,
        caps: &mut [(usize, usize)],
    ) -> bool {
        let Some(n) = self.nodes.get(node) else {
            return true;
        };
        match n {
            Node::Open(g) | Node::Close(g) => {
                let saved = caps[*g];
                if matches!(n, Node::Open(_)) {
                    caps[*g].0 = pos;
                } else {
                    caps[*g].1 = pos;
                }
                let r = self.match_at(chars, node + 1, pos, caps);
                if !r {
                    caps[*g] = saved;
                }
                r
            }
            Node::Atom { atom, plus: false } => {
                pos < chars.len()
                    && atom.matches(chars[pos].1)
                    && self.match_at(chars, node + 1, pos + 1, caps)
            }
            Node::Atom { atom, plus: true } => {
                // greedy: take as many as possible, then give back one at a time
                let mut end = pos;
                while end < chars.len() && atom.matches(chars[end].1) {
                    end += 1;
                }
                while end > pos {
                    if self.match_at(chars, node + 1, end, caps) {
                        return true;
                    }
                    end -= 1;
                }
                false
            }
        }
    }
}
